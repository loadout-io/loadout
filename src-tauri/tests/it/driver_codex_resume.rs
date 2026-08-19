//! AC-4 dla T-10: jedna tożsamość przez wiele tur, nawet gdy Codex zmieni `thread_id`.
//!
//! To jest cicha porażka numer jeden tego zadania. Sterownik, który przy każdej turze mintuje
//! nowy `SessionRef`, bo przecież `thread.started` przyszło znowu, pokazuje na szynie **trzech
//! agentów zamiast jednego**: trzy podsumowania „Done", trzy koszty, i wszystko wygląda na
//! skończone — więc nikt tego nie zgłosi.
//!
//! **Słaba wersja tego kryterium to `assert_eq!(handle.session().id, first)` po dwóch turach.**
//! Przechodzi trywialnie, kiedy atrapa wypisuje ten sam identyfikator w obu turach — czyli
//! w przypadku, którego się boimy, test **milczy**. T1 §11 pytanie 5 nie rozstrzyga, czy
//! `codex exec resume` oddaje ten sam `thread_id`, czy mintuje nowy, więc sterownik nie ma
//! prawa założyć żadnej z dwóch odpowiedzi.
//!
//! Rozróżnia to atrapa z **różnymi** identyfikatorami w każdej turze plus trzy asercje razem:
//! `session()` niezmienione, `resume` z **najnowszym**, i dokładnie jeden wpis na turę
//! w [`CodexHandle::threads_seen`] — bo to jest różnica między „widzieliśmy dwa identyfikatory
//! i pamiętamy oba" a „drugi nadpisał pierwszy", której z zewnątrz nie da się inaczej odróżnić.
//!
//! Sam kształt tego pliku jest dowodem, że `codex exec` nie ma trybu dwukierunkowego [T1 §6.4]:
//! każda tura to osobny plik `argv-N.log`, bo każda tura to osobny **proces**.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::{AgentHandle, Policy, RunSpec};
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

/// Sufit na pojedyncze oczekiwanie. Regresja ma być czerwonym testem, nie zawieszeniem.
const LIMIT: Duration = Duration::from_secs(8);

/// Miejsce w kanale zdarzeń, z zapasem.
const CHANNEL: usize = 256;

/// Prompt pierwszej tury.
const FIRST_PROMPT: &str = "start the job";

/// Prompt drugiej tury.
const SECOND_PROMPT: &str = "druga tura";

/// Prompt trzeciej tury.
const THIRD_PROMPT: &str = "trzecia tura";

/// Atrapa `codex`: liczy tury, zapisuje argumenty i stdin każdej z osobna i wypisuje **inny**
/// `thread_id` za każdym razem.
///
/// Ten inny identyfikator jest całą treścią fikstury. Atrapa powtarzająca ten sam numer
/// mierzyłaby przypadek, który i tak działa.
const COUNTS_TURNS: &str = r#"#!/bin/sh
here="$(dirname "$0")"

n=0
if [ -f "$here/turns" ]; then
  n="$(cat "$here/turns")"
fi
n=$((n + 1))
printf '%s' "$n" > "$here/turns"

: > "$here/argv-$n.log"
for a in "$@"; do
  printf '%s\n' "$a" >> "$here/argv-$n.log"
done

printf '{"type":"thread.started","thread_id":"thread-%s"}\n' "$n"

cat > "$here/stdin-$n.log"

printf '{"type":"turn.completed","usage":{"input_tokens":1,"cached_input_tokens":2,"output_tokens":3}}\n'
exit 0
"#;

/// Zapisuje wykonywalny skrypt i zwraca jego ścieżkę.
fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// Linie pliku, który mógł jeszcze nie powstać.
fn lines_of(path: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(fs::read_to_string(path)?
        .lines()
        .map(String::from)
        .collect())
}

/// Zawartość pliku, który mógł jeszcze nie powstać.
fn text_of(path: &Path) -> Result<String, Box<dyn Error>> {
    if !path.exists() {
        return Ok(String::new());
    }
    Ok(fs::read_to_string(path)?)
}

/// Argumenty, z jakimi ma ruszyć tura wznawiająca `thread` [T1 §8.4].
fn resume_argv(thread: &str, cwd: &Path) -> Vec<String> {
    vec![
        "exec".to_owned(),
        "resume".to_owned(),
        thread.to_owned(),
        "--json".to_owned(),
        "--ignore-user-config".to_owned(),
        "-C".to_owned(),
        cwd.display().to_string(),
        "-".to_owned(),
    ]
}

/// `RunSpec` pierwszej tury.
fn spec(cwd: &Path) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: FIRST_PROMPT.to_owned(),
        model: Some("gpt-5-codex".to_owned()),
        system_append: None,
        policy: Policy::EditInFolder,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn three_turns_keep_one_identity_and_resume_the_newest_thread() -> Result<(), Box<dyn Error>>
{
    let dir = tempfile::tempdir()?;
    let binary = write_script(dir.path(), "codex", COUNTS_TURNS)?;

    let (tx, _rx) = mpsc::channel(CHANNEL);
    let driver = CodexDriver::with_binary(binary);
    let mut handle = timeout(LIMIT, driver.start_session(spec(dir.path()), tx)).await??;

    // ── Tura pierwsza ─────────────────────────────────────────────────────────────────────
    //
    // Wynik tury odbieramy, ale rozpakowujemy go DOPIERO po asercji o tożsamości: to ona jest
    // kryterium, więc to ona ma być zdaniem, które bramka pokaże, kiedy padnie.
    let first = timeout(LIMIT, handle.wait()).await?;
    assert_eq!(
        handle.session().id,
        "thread-1",
        "the first thread.started is the identity of this session: it is what the rail shows, \
         what T-06 stores next to the step, and what a crash recovery resumes. It came out as \
         {:?}",
        handle.session()
    );
    assert_eq!(
        handle.session().vendor,
        "codex",
        "a session that does not say which adapter minted it resumes into the wrong CLI"
    );
    assert_eq!(
        first?.session.id, "thread-1",
        "the outcome of the turn has to be signed with the same session as the handle"
    );

    // ── Tura druga: nowy proces, wznowienie po PIERWSZYM identyfikatorze ───────────────────
    timeout(LIMIT, handle.send(SECOND_PROMPT.to_owned())).await??;
    let _second = timeout(LIMIT, handle.wait()).await??;

    assert_eq!(
        lines_of(&dir.path().join("argv-2.log"))?,
        resume_argv("thread-1", dir.path()),
        "the second turn is a FRESH PROCESS resuming the first thread - codex exec has no \
         bidirectional mode, so this is the only way a second turn exists at all [T1 6.4]. \
         Note what is absent from this line and must stay absent: -m and -s belong to the first \
         turn, and --skip-git-repo-check with it"
    );
    assert_eq!(
        text_of(&dir.path().join("stdin-2.log"))?,
        SECOND_PROMPT,
        "the follow-up prompt rides on stdin, byte for byte, exactly like the first one \
         (invariant 9)"
    );
    assert_eq!(
        handle.session().id,
        "thread-1",
        "THE dummy just handed back a different thread_id, and the identity must not move. A \
         driver that mints a new SessionRef here shows three agents on the rail instead of one, \
         with three 'Done' summaries and three costs - and it all looks finished, so nobody \
         reports it. It came out as {:?}",
        handle.session()
    );

    // ── Tura trzecia: wznowienie po NAJNOWSZYM identyfikatorze ────────────────────────────
    timeout(LIMIT, handle.send(THIRD_PROMPT.to_owned())).await??;
    let _third = timeout(LIMIT, handle.wait()).await??;

    assert_eq!(
        lines_of(&dir.path().join("argv-3.log"))?,
        resume_argv("thread-2", dir.path()),
        "resuming has to use the NEWEST thread id, because that is the one the vendor last \
         acknowledged - T1 section 11 question 5 leaves open whether resume mints a new id, so \
         the driver has to be right either way. Resuming thread-1 here would be a driver that \
         assumed the answer"
    );
    assert_eq!(
        handle.session().id,
        "thread-1",
        "and the identity STILL does not move, three turns in"
    );

    assert_eq!(
        handle.threads_seen(),
        ["thread-1", "thread-2", "thread-3"],
        "one entry per turn, in order, no repeats: the first is who this session is, the last is \
         what the next turn resumes, and the difference between them is recorded once instead of \
         being overwritten. It came out as {:?}",
        handle.threads_seen()
    );

    Ok(())
}
