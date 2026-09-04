//! Regresja D-2: każda podkomenda `codex exec resume` przechodzi własną bramkę gita.
//!
//! Słaby test porównuje wektor argumentów z napisem i przechodzi także wtedy, gdy żywy proces
//! dostaje inną linię. Ta fikstura zachowuje się jak codex-cli 0.152.0: w folderze bez `.git`
//! odmawia tury bez `--skip-git-repo-check`, a dopiero po przejściu bramki wypisuje odpowiedź.
//! Kuracja tej odpowiedzi i odmowy do [`Line`] przypina więc to, co widzi człowiek
//! (niezmienniki 20 i 29), nie samą obecność tekstu w funkcji składającej argv.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::{
    AgentEvent, AgentHandle, DecodedEvent, Outcome, Policy, RunSpec, SessionRef,
};
use loadout_lib::engine::line::{Curator, Line, Seen};
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

/// Sufit na pojedyncze oczekiwanie. Regresja ma być czerwonym testem, nie zawieszeniem.
const LIMIT: Duration = Duration::from_secs(8);

/// Miejsce w kanale zdarzeń, z zapasem.
const CHANNEL: usize = 256;

/// Zdanie codex-cli 0.152.0 z sondy D-2 wykonanej 2026-09-02 bez repozytorium.
const TRUSTED_DIRECTORY: &str =
    "Not inside a trusted directory and --skip-git-repo-check was not specified.";

/// Atrapa `codex`, która odtwarza bramkę podkomendy `resume`, a po niej pełną udaną turę.
///
/// 2026-09 — warunek zależy od **dwóch** faktów procesowych: folder nie ma `.git`, a żywe argv
/// nie ma flagi. Dzięki temu fikstura nie jest testem obecności napisu (niezmiennik 20).
const REQUIRES_THE_FLAG_WITHOUT_GIT: &str = r#"#!/bin/sh
here="$(dirname "$0")"

has_skip_git_repo_check=0
: > "$here/argv-current.log"
for a in "$@"; do
  printf '%s\n' "$a" >> "$here/argv-current.log"
  if [ "$a" = "--skip-git-repo-check" ]; then
    has_skip_git_repo_check=1
  fi
done

if [ ! -d "$here/.git" ] && [ "$has_skip_git_repo_check" -eq 0 ]; then
  printf '%s\n' "Not inside a trusted directory and --skip-git-repo-check was not specified." >&2
  exit 1
fi

n=0
if [ -f "$here/turns" ]; then
  n="$(cat "$here/turns")"
fi
n=$((n + 1))
printf '%s' "$n" > "$here/turns"
cp "$here/argv-current.log" "$here/argv-$n.log"

cat > "$here/stdin-$n.log"

printf '{"type":"thread.started","thread_id":"thread-%s"}\n' "$n"
printf '{"type":"item.completed","item":{"type":"agent_message","id":"item-%s","text":"reply-%s"}}\n' "$n" "$n"
printf '{"type":"turn.completed","usage":{"input_tokens":1,"cached_input_tokens":0,"output_tokens":3}}\n'
exit 0
"#;

/// Zapisuje wykonywalny skrypt i zwraca jego ścieżkę.
fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// `RunSpec` zwykłej pierwszej tury.
fn spec(cwd: &Path, prompt: &str) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: prompt.to_owned(),
        model: Some("gpt-5-codex".to_owned()),
        system_append: None,
        reaches_the_web: false,
        policy: Policy::EditInFolder,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Czyta dokładnie jedną turę z trwałego kanału sesji.
async fn next_turn(
    rx: &mut mpsc::Receiver<DecodedEvent>,
) -> Result<Vec<DecodedEvent>, Box<dyn Error>> {
    let mut events = Vec::new();
    loop {
        let decoded = timeout(LIMIT, rx.recv())
            .await?
            .ok_or("the Codex event channel closed before the turn finished")?;
        let finished = matches!(&decoded.event, AgentEvent::Finished(_));
        events.push(decoded);
        if finished {
            return Ok(events);
        }
    }
}

/// Przepuszcza zdarzenia tą samą kuracją, która buduje widoczne wiersze biegu.
fn visible_lines(events: &[DecodedEvent]) -> Vec<Line> {
    let mut curator = Curator::new();
    let mut lines = Vec::new();
    for (at_ms, decoded) in events.iter().enumerate() {
        lines.extend(curator.observe(Seen {
            agent: "Codex",
            at_ms: u64::try_from(at_ms).unwrap_or_default(),
            event: &decoded.event,
            tool: decoded.tool.as_ref(),
        }));
    }
    lines.extend(curator.flush());
    lines
}

/// Dowodzi, że odpowiedź dotarła do widocznego strumienia i nie została zastąpiona odmową.
fn assert_reached_agent(turn: &str, reply: &str, outcome: &Outcome, events: &[DecodedEvent]) {
    let lines = visible_lines(events);
    assert!(
        outcome.ok,
        "{turn}: the turn never reached the agent in a folder without git: {outcome:?}; visible \
         lines: {lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| matches!(line, Line::Note { text, .. } if text == reply)),
        "{turn}: the agent's answer must reach the curated stream a person reads. It produced \
         {lines:?}"
    );
    assert!(
        !lines.iter().any(|line| {
            matches!(line, Line::Problem { text, .. } if text.contains("trusted directory"))
        }),
        "{turn}: the git gate still became a visible Problem row: {lines:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_follow_up_turn_in_a_folder_without_git_reaches_the_agent() -> Result<(), Box<dyn Error>>
{
    let dir = tempfile::tempdir()?;
    assert!(
        !dir.path().join(".git").exists(),
        "the fixture must exercise a folder without git"
    );
    let binary = write_script(dir.path(), "codex", REQUIRES_THE_FLAG_WITHOUT_GIT)?;

    let (tx, mut rx) = mpsc::channel(CHANNEL);
    let driver = CodexDriver::with_binary(binary.clone());
    let mut handle = timeout(
        LIMIT,
        driver.start_session(spec(dir.path(), "first turn"), tx.clone()),
    )
    .await??;

    let first = timeout(LIMIT, handle.wait()).await??;
    let first_events = next_turn(&mut rx).await?;
    assert_reached_agent("first turn", "reply-1", &first, &first_events);

    timeout(LIMIT, handle.send("second turn".to_owned())).await??;
    let second = timeout(LIMIT, handle.wait()).await??;
    let second_events = next_turn(&mut rx).await?;
    assert_reached_agent("follow-up turn 2", "reply-2", &second, &second_events);

    timeout(LIMIT, handle.send("third turn".to_owned())).await??;
    let third = timeout(LIMIT, handle.wait()).await??;
    let third_events = next_turn(&mut rx).await?;
    assert_reached_agent("follow-up turn 3", "reply-3", &third, &third_events);

    // 2026-09 — `RunSpec.resume` jest drugą drogą do tego samego argv: świeży sterownik nie ma
    // poprzedniej tury, która mogłaby przypadkiem przenieść ustawienie w pamięci procesu.
    let mut resumed = spec(dir.path(), "resume in a fresh driver");
    resumed.resume = Some(SessionRef {
        vendor: "codex",
        id: "thread-from-an-earlier-driver".to_owned(),
    });
    let fresh_driver = CodexDriver::with_binary(binary.clone());
    let mut fresh_handle = timeout(LIMIT, fresh_driver.start_session(resumed, tx)).await??;
    let fresh = timeout(LIMIT, fresh_handle.wait()).await??;
    let fresh_events = next_turn(&mut rx).await?;
    assert_reached_agent(
        "RunSpec.resume in a fresh driver",
        "reply-4",
        &fresh,
        &fresh_events,
    );

    // Kontrola fikstury: to samo wykonywalne CLI bez flagi ma odmówić PRZED odpowiedzią agenta.
    let rejected = Command::new(&binary)
        .args(["exec", "-C"])
        .arg(dir.path())
        .args([
            "resume",
            "thread-control",
            "--json",
            "--ignore-user-config",
            "-",
        ])
        .current_dir(dir.path())
        .output()?;
    assert!(
        !rejected.status.success(),
        "the parser-faithful fixture accepted resume without its per-invocation git flag"
    );
    assert_eq!(
        String::from_utf8_lossy(&rejected.stderr).trim(),
        TRUSTED_DIRECTORY,
        "the fixture must reproduce the vendor gate, not merely search production argv for a \
         string (invariant 20)"
    );

    Ok(())
}
