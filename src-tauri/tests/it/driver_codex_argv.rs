//! AC-1 dla T-10: argv jest kompletne, polityka jest totalna, a prompt nigdy nie jest w argv.
//!
//! **Słaba wersja tego kryterium to `assert!(argv.contains(&"read-only".into()))`.** Przechodzi
//! ją implementacja, która oprócz flagi wkłada do argv **także prompt** — flaga przecież jest.
//! A prompt w argv to niezmiennik 9 złamany po cichu: `ps aux` każdego użytkownika maszyny
//! wypisuje wtedy cudzą treść zadania, i to samo robi każdy raport awarii. T1 §8.4 pokazuje
//! prompt jako ostatni element argv, więc jest to pomyłka, do której zaprasza sama dokumentacja.
//!
//! Rozróżniają to dwie asercje i **obie są potrzebne**:
//!
//! 1. `argv.iter().all(|a| !a.contains(MARKER))` — nie ma go w argumentach;
//! 2. porównanie tego, co atrapa binarki zrzuciła ze swojego **stdinu**, z promptem bajt
//!    w bajt — bo bez tej drugiej „nie ma w argv" spełnia też sterownik, który po prostu
//!    **gubi prompt** i uruchamia agenta bez zadania.
//!
//! Sam szkielet tego zadania jest tego dowodem: pusta lista argumentów przechodzi asercję (1)
//! i (2) już nie.
//!
//! Trzecia rzecz, którą mierzy ten plik: **EOF na stdinie**. Bez zamknięcia deskryptora
//! `codex exec` wypisuje `Reading additional input from stdin...` i czeka [T1, „Worth adding"].
//! Atrapa czyta stdin przez `cat`, które kończy się **wyłącznie** na EOF — więc plik z pełną
//! treścią promptu jest jedynym dowodem, że deskryptor został zamknięty.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use loadout_lib::engine::drivers::codex::{CodexDriver, build_exec_argv};
use loadout_lib::engine::drivers::{AgentDriver, AgentHandle, Policy, RunSpec};
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

/// Sufit na pojedyncze oczekiwanie. Regresja ma być czerwonym testem, nie zawieszeniem:
/// bramka odpowiada na zawieszenie kodem 124, a to jest fałszywa czerwień.
const LIMIT: Duration = Duration::from_secs(8);

/// Miejsce w kanale zdarzeń, z zapasem.
const CHANNEL: usize = 256;

/// Treść zadania, po której poznamy przeciek. Nic innego w tym teście tak nie brzmi.
const PROMPT: &str = "MARKER-9f3c-do-not-leak";

/// Fragment, którego szukamy w każdym elemencie argv — krótszy niż cały prompt, żeby złapać
/// także przeciek przycięty albo sklejony z czymś innym.
const MARKER: &str = "MARKER-9f3c";

/// Flaga, która wyłącza **cały** dial uprawnień naraz. Nie jest czwartym stopniem polityki,
/// tylko jej obejściem, więc nie ma prawa pojawić się w żadnym wariancie [T1 §6.1].
const BYPASS: &str = "--dangerously-bypass-approvals-and-sandbox";

/// Model zamawiany w tym teście — dowolny, byle rozpoznawalny w porównaniu całej linii.
const MODEL: &str = "gpt-5-codex";

/// Katalog roboczy kroku. Czysta wartość, nie ścieżka na dysku: `build_exec_argv` jest funkcją
/// czystą i nie ma prawa niczego szukać.
const CWD: &str = "/loadout/step/one";

/// `RunSpec` różniący się od pozostałych **wyłącznie** polityką.
fn spec(cwd: &Path, policy: Policy) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: PROMPT.to_owned(),
        model: Some(MODEL.to_owned()),
        system_append: None,
        policy,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Tryb piaskownicy, czyli wartość stojąca **zaraz za** `-s`, plus liczba wystąpień samej flagi.
fn sandbox_of(policy: Policy) -> Result<(String, usize), Box<dyn Error>> {
    let argv = build_exec_argv(&spec(Path::new(CWD), policy));
    let flags = argv.iter().filter(|arg| arg.as_str() == "-s").count();
    let at = argv
        .iter()
        .position(|arg| arg.as_str() == "-s")
        .ok_or_else(|| format!("{policy:?} produced argv without a -s flag at all: {argv:?}"))?;
    let mode = argv
        .get(at + 1)
        .ok_or_else(|| format!("{policy:?} put -s at the very end, with nothing after it"))?;
    Ok((mode.clone(), flags))
}

/// Zapisuje wykonywalny skrypt i zwraca jego ścieżkę.
fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// Bajty pliku, który mógł jeszcze nie powstać. Pusto znaczy „atrapa tego nie zapisała",
/// a nie „nie da się przeczytać" — asercja wołającego ma powiedzieć, czego zabrakło.
fn bytes_of(path: &Path) -> Result<Vec<u8>, Box<dyn Error>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(fs::read(path)?)
}

/// Atrapa `codex`: zrzuca swoje argumenty i swój stdin obok siebie, mówi jedną linię
/// i wychodzi.
///
/// `cat` kończy się **wyłącznie na EOF**, więc kompletny `stdin.log` jest dowodem, że
/// sterownik zamknął deskryptor wejściowy. Logujemy obok skryptu, nigdy przez zmienną
/// środowiskową: supervisor robi `env_clear()`, więc fikstura sterowana envem po cichu
/// przestałaby działać.
const RECORDS: &str = r#"#!/bin/sh
here="$(dirname "$0")"

: > "$here/argv.log"
for a in "$@"; do
  printf '%s\n' "$a" >> "$here/argv.log"
done

printf '{"type":"thread.started","thread_id":"01a01b33-argv"}\n'

cat > "$here/stdin.log"

printf '{"type":"turn.completed","usage":{"input_tokens":1,"cached_input_tokens":2,"output_tokens":3}}\n'
exit 0
"#;

#[test]
fn the_first_turn_is_exactly_the_line_from_t1() {
    let argv = build_exec_argv(&spec(Path::new(CWD), Policy::ReadOnly));

    let expected: Vec<String> = vec![
        "exec".to_owned(),
        "--json".to_owned(),
        "--ignore-user-config".to_owned(),
        "--skip-git-repo-check".to_owned(),
        "-C".to_owned(),
        CWD.to_owned(),
        "-m".to_owned(),
        MODEL.to_owned(),
        "-s".to_owned(),
        "read-only".to_owned(),
        "-".to_owned(),
    ];

    assert_eq!(
        argv, expected,
        "this is the binding argv from T1 sections 6.1 and 8.4, in this order. Two elements \
         carry the whole point: --ignore-user-config, without which the user's own config.toml \
         dumped four expired-OAuth ERROR lines into a real run, and the trailing '-', which is \
         how codex exec reads the prompt from stdin instead of argv"
    );
}

#[test]
fn each_policy_reaches_the_sandbox_as_its_own_mode() -> Result<(), Box<dyn Error>> {
    let (read_only, read_only_flags) = sandbox_of(Policy::ReadOnly)?;
    let (editing, editing_flags) = sandbox_of(Policy::EditInFolder)?;
    let (unlimited, unlimited_flags) = sandbox_of(Policy::Unrestricted)?;

    assert_eq!(
        read_only, "read-only",
        "'Read only' has to reach the sandbox as read-only; it came out as {read_only:?}"
    );
    assert_eq!(
        editing, "workspace-write",
        "'Can edit this folder' has to reach the sandbox as workspace-write; it came out as \
         {editing:?}"
    );
    assert_eq!(
        unlimited, "danger-full-access",
        "'No limits' has to actually be no limits - an agent promised it and handed read-only \
         cannot write a line. It came out as {unlimited:?}"
    );

    assert!(
        read_only != editing && editing != unlimited && read_only != unlimited,
        "three plain-language policies have to reach the CLI as three different modes. An \
         adapter that prints one mode for all three passes every check that only asks whether \
         the flag is there. They came out as {read_only:?}, {editing:?} and {unlimited:?}"
    );

    for (policy, flags) in [
        ("read-only", read_only_flags),
        ("workspace-write", editing_flags),
        ("danger-full-access", unlimited_flags),
    ] {
        assert_eq!(
            flags, 1,
            "exactly one -s has to reach the CLI for {policy}: zero means the dial decides \
             nothing and codex falls back to its own default, and two means the last one wins \
             while whoever reads the command line believes the first"
        );
    }

    for policy in [Policy::ReadOnly, Policy::EditInFolder, Policy::Unrestricted] {
        let argv = build_exec_argv(&spec(Path::new(CWD), policy));
        assert!(
            !argv.iter().any(|arg| arg == BYPASS),
            "{BYPASS} is not a fourth step of the dial, it is the door around it: it turns off \
             approvals AND the sandbox at once, so no policy may reach for it. {policy:?} \
             produced {argv:?}"
        );
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_prompt_never_rides_in_argv_and_arrives_whole_on_stdin() -> Result<(), Box<dyn Error>> {
    // ── Połowa pierwsza: czysta funkcja ───────────────────────────────────────────────────
    let argv = build_exec_argv(&spec(Path::new(CWD), Policy::EditInFolder));
    assert!(
        argv.iter().all(|arg| !arg.contains(MARKER)),
        "the task text must never reach argv (invariant 9): every user of this machine sees it \
         in `ps aux`, and so does every crash report. T1 section 8.4 draws the prompt as the \
         last element of argv, so this is the mistake the documentation itself invites. \
         It produced {argv:?}"
    );

    // ── Połowa druga: prawdziwy proces ────────────────────────────────────────────────────
    //
    // Bez niej asercja wyżej jest spełnialna przez sterownik, który prompt po prostu GUBI:
    // pustego argv nie ma w czym przeciec.
    let dir = tempfile::tempdir()?;
    let binary = write_script(dir.path(), "codex", RECORDS)?;

    let (tx, _rx) = mpsc::channel(CHANNEL);
    let driver = CodexDriver::with_binary(binary);
    let mut handle: Box<dyn AgentHandle> = timeout(
        LIMIT,
        driver.start(spec(dir.path(), Policy::EditInFolder), tx),
    )
    .await??;
    // Wynik tury nas tu nie interesuje — interesuje nas, że tura się SKOŃCZYŁA, bo dopiero
    // wtedy atrapa ma na dysku komplet tego, co dostała.
    let _ended = timeout(LIMIT, handle.wait()).await?;
    drop(handle);

    let recorded = String::from_utf8(bytes_of(&dir.path().join("argv.log"))?)?;
    assert!(
        !recorded.is_empty(),
        "the dummy binary recorded no arguments at all, so this driver launched no process - \
         and every assertion below would then be true about nothing"
    );
    assert!(
        recorded.lines().all(|arg| !arg.contains(MARKER)),
        "the prompt reached the real command line of the real process. This is the assertion \
         that survives an adapter which builds clean argv and then appends the prompt anyway. \
         The dummy was called with {recorded:?}"
    );

    let written = bytes_of(&dir.path().join("stdin.log"))?;
    assert_eq!(
        written,
        PROMPT.as_bytes(),
        "the prompt has to arrive on stdin byte for byte, and the file has to be complete. \
         The dummy reads with `cat`, which ends only on EOF: an empty or partial file means \
         either the driver never wrote the task, or it left the descriptor open - and an open \
         stdin is what makes codex exec print 'Reading additional input from stdin...' and wait \
         forever. It received {:?}",
        String::from_utf8_lossy(&written)
    );

    Ok(())
}
