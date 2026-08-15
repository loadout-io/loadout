//! AC-7 dla T-04: anulowanie eskaluje — `control_request` **tylko pod zdolnością**, potem
//! zabicie grupy.
//!
//! **Słaba wersja tego kryterium to `assert!(handle.cancel().await.is_ok())`.** Jest prawdziwa
//! także dla sterownika, który od razu wysyła SIGKILL — a wtedy tracimy wznawialność sesji,
//! transkrypt nie zostaje dosypany i hooki `SessionEnd` nie biegną [T1 §4.6]. Jest prawdziwa
//! również dla sterownika, który wysyła `control_request` **zawsze**, także tam, gdzie CLI go
//! nie obsługuje, i wisi pięć sekund na odpowiedzi, która nie przyjdzie.
//!
//! Rozróżnia **treść `stdin.log` w obu atrapach**: dokładnie jedna linia przerwania w A,
//! zero czegokolwiek w kształcie `control_request` w B. Obie atrapy dostają ten sam sterownik
//! i różnią się wyłącznie tym, co ogłosiły w `capabilities` — więc różnica w `stdin.log` może
//! pochodzić tylko z feature-detekcji [T1 §4.1].
//!
//! Atrapa B **zapisuje** stdin, choć na niego nie odpowiada. Bez tego zapisu asercja
//! „`stdin.log` nie zawiera `control_request`" przechodziłaby na nieistniejącym pliku, czyli
//! na niczym — a asercja, która jest prawdziwa, bo nic się nie wydarzyło, jest dokładnie tym,
//! czego zabrania niezmiennik 20.
//!
//! Testy odpalają prawdziwe procesy, więc są `#[ignore]`; bramka woła je linią `check:`
//! z `-- --include-ignored`.

use std::error::Error;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, FinishReason, Policy, RunSpec,
};
use loadout_lib::engine::supervisor::GroupProof;
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

/// Sufit na pojedyncze oczekiwanie. Regresja ma być czerwonym testem, nie zawieszeniem:
/// bramka odpowiada na zawieszenie kodem 124, a to jest fałszywa czerwień.
const LIMIT: Duration = Duration::from_secs(8);

/// Sufit na całość jednego przypadku. Anulowanie, które trwa dłużej, jest anulowaniem,
/// na które użytkownik przestaje czekać.
const BUDGET: Duration = Duration::from_secs(15);

/// Miejsce w kanale zdarzeń, z zapasem.
const CHANNEL: usize = 256;

/// Po czym poznajemy przerwanie w paśmie w zapisanym stdinie.
const INTERRUPT: &str = r#""subtype":"interrupt""#;

/// Po czym poznajemy jakikolwiek `control_request`.
const CONTROL: &str = "control_request";

/// Zdolność, pod którą wolno wysłać przerwanie.
const CAPABILITY: &str = "interrupt_receipt_v1";

/// Atrapa A: ogłasza zdolności, zapisuje każdą linię stdinu, odpowiada na `control_request`
/// i **wychodzi sama**.
const ANNOUNCES: &str = r#"#!/bin/sh
here="$(dirname "$0")"

session=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--session-id" ]; then
    session="$2"
  fi
  shift
done

printf '{"type":"system","subtype":"init","session_id":"%s","capabilities":["interrupt_receipt_v1","interrupt_cancel_queued_v1","msg_lifecycle_v1"]}\n' "$session"

while IFS= read -r line; do
  printf '%s\n' "$line" >> "$here/stdin.log"
  case "$line" in
    *control_request*)
      rid="$(printf '%s' "$line" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')"
      printf '{"type":"control_response","response":{"subtype":"success","request_id":"%s","response":{"still_queued":[]}}}\n' "$rid"
      printf '{"type":"result","subtype":"error_during_execution","is_error":true,"terminal_reason":"cancelled","session_id":"%s","num_turns":1,"total_cost_usd":0.002,"usage":{"input_tokens":1,"cache_read_input_tokens":2,"output_tokens":3},"duration_ms":2,"result":"interrupted"}\n' "$session"
      exit 0
      ;;
  esac
done

exit 0
"#;

/// Atrapa B: ogłasza pustą listę zdolności, zapisuje stdin i **nigdy nie odpowiada**.
/// Nie kończy się sama — zdjąć ją ma eskalacja.
///
/// 2026-08-15 — `exec 3<&0` i `read <&3` **nie są ozdobą**. POSIX: „standardowe wejście listy
/// asynchronicznej, przed jawnymi przekierowaniami, jest przypisane do pliku o właściwościach
/// `/dev/null`, chyba że włączone jest sterowanie zadaniami" — a w skrypcie odpalonym przez
/// bramkę sterowania zadaniami nie ma. Zmierzone na tej maszynie: z gołym `done &` plik
/// `stdin.log` nie powstawał **nigdy**, więc asercja o kopercie promptu nie mogła przejść przy
/// żadnej implementacji sterownika — a asercja, której nie da się spełnić, mierzy tyle samo co
/// asercja, którą spełnia wszystko. Kopia deskryptora zostaje w tle celowo: dzięki niej grupa
/// ma **wnuka** (podpowłoka + `sleep`), czyli dokładnie to, co przeżyło pomiar
/// `total=2 orphaned=2` z T7 §3.1 i co dowód z `ESRCH` ma tu wykluczyć.
const SILENT: &str = r#"#!/bin/sh
here="$(dirname "$0")"

session=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--session-id" ]; then
    session="$2"
  fi
  shift
done

printf '{"type":"system","subtype":"init","session_id":"%s","capabilities":[]}\n' "$session"

exec 3<&0

while IFS= read -r line <&3; do
  printf '%s\n' "$line" >> "$here/stdin.log"
done &

while :; do
  sleep 0.2
done
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

/// Czeka, aż plik urośnie do `want` linii. Zwraca ostatni odczyt także wtedy, gdy jest za
/// krótki — asercja wołającego ma powiedzieć, czego zabrakło.
async fn wait_for_lines(
    path: &Path,
    want: usize,
    limit: Duration,
) -> Result<Vec<String>, Box<dyn Error>> {
    let deadline = Instant::now() + limit;
    loop {
        let lines = lines_of(path)?;
        if lines.len() >= want || Instant::now() >= deadline {
            return Ok(lines);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Czeka na `Started` i oddaje zdolności, które przyszły z `system/init`.
///
/// Anulowanie przed tym zdarzeniem nie miałoby czego feature-detektować, więc test, który nie
/// czeka, mierzyłby wyścig zamiast eskalacji.
async fn announced(rx: &mut mpsc::Receiver<AgentEvent>) -> Result<Vec<String>, Box<dyn Error>> {
    loop {
        let event = rx
            .recv()
            .await
            .ok_or("the driver closed the event channel before the session ever started")?;
        if let AgentEvent::Started { capabilities, .. } = event {
            return Ok(capabilities);
        }
    }
}

/// Pyta jądro, czy w grupie `pgid` jest jeszcze ktokolwiek — **nie wysyłając sygnału**.
// 2026-08-15 — `kill(2)` nie ma bezpiecznego opakowania w std, a ten plik z definicji pyta
// system operacyjny zamiast naszego kodu (niezmiennik 20). Pliki testowe są wyłączone
// z granic architektury po ŚCIEŻCE (checks/quick-boundary.sh), bo nie są częścią wysyłanego
// artefaktu.
#[allow(unsafe_code)]
fn group_probe(pgid: i32) -> io::Result<()> {
    // SAFETY: `kill` z sygnałem 0 niczego nie dostarcza — sprawdza wyłącznie istnienie
    // i prawa. Argumenty to zwykłe liczby, więc nie ma tu wskaźnika ani czasu życia.
    let rc = unsafe { libc::kill(-pgid, 0) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// `RunSpec` jednej tury.
fn spec(run_id: Uuid, cwd: &Path) -> RunSpec {
    RunSpec {
        run_id,
        cwd: cwd.to_path_buf(),
        prompt: "start a long job".to_owned(),
        model: None,
        system_append: None,
        policy: Policy::ReadOnly,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "odpala prawdziwy proces; bramka woła to z --include-ignored"]
async fn under_the_capability_the_session_is_asked_to_stop_and_ends_itself()
-> Result<(), Box<dyn Error>> {
    let began = Instant::now();
    let dir = tempfile::tempdir()?;
    let binary = write_script(dir.path(), "claude", ANNOUNCES)?;
    let log = dir.path().join("stdin.log");

    let (tx, mut rx) = mpsc::channel(CHANNEL);
    let driver = ClaudeDriver::with_binary(binary);
    let mut handle: Box<dyn AgentHandle> =
        timeout(LIMIT, driver.start(spec(Uuid::now_v7(), dir.path()), tx)).await??;

    let capabilities = timeout(LIMIT, announced(&mut rx)).await??;
    assert!(
        capabilities.iter().any(|name| name.as_str() == CAPABILITY),
        "this dummy announces the interrupt capability; without it the rest of this test would \
         be measuring the other branch. It announced {capabilities:?}"
    );

    let proof = timeout(LIMIT, handle.cancel()).await?;

    // Pierwsza linia stdinu to koperta z promptem, druga to przerwanie.
    let written = wait_for_lines(&log, 2, LIMIT).await?;
    let interrupts = written
        .iter()
        .filter(|line| line.contains(INTERRUPT))
        .count();
    assert_eq!(
        interrupts, 1,
        "exactly one in-band interrupt has to reach the CLI: none means the driver led with a \
         signal and threw away a resumable session, and more than one means it kept asking \
         while the answer was already on its way. stdin.log held {written:?}"
    );

    let outcome = timeout(LIMIT, handle.wait()).await??;
    assert_eq!(
        outcome.reason,
        FinishReason::Cancelled,
        "a step somebody stopped on purpose must not read the same as a step that broke. \
         It came out as {outcome:?}"
    );

    let GroupProof::Dead { status } = &proof else {
        return Err(
            format!("cancelling has to end in proof that the group is gone: {proof:?}").into(),
        );
    };
    let status = status.ok_or("the leader's exit status was never collected")?;
    assert!(
        status.signal().is_none(),
        "the process asked nicely has to exit ON ITS OWN. Dying from a signal here means the \
         driver escalated anyway: no appended transcript, no SessionEnd hooks, and a session \
         nobody can resume. It exited as {status:?}"
    );
    assert!(
        began.elapsed() < BUDGET,
        "cancelling took {:?}",
        began.elapsed()
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "odpala prawdziwy proces; bramka woła to z --include-ignored"]
async fn without_the_capability_the_group_is_killed_and_proved_gone() -> Result<(), Box<dyn Error>>
{
    let began = Instant::now();
    let dir = tempfile::tempdir()?;
    let binary = write_script(dir.path(), "claude", SILENT)?;
    let log = dir.path().join("stdin.log");

    let (tx, mut rx) = mpsc::channel(CHANNEL);
    let driver = ClaudeDriver::with_binary(binary);
    let mut handle: Box<dyn AgentHandle> =
        timeout(LIMIT, driver.start(spec(Uuid::now_v7(), dir.path()), tx)).await??;

    let capabilities = timeout(LIMIT, announced(&mut rx)).await??;
    assert!(
        capabilities.is_empty(),
        "this dummy announces nothing, which is the whole point of the case. It announced \
         {capabilities:?}"
    );

    // Log musi już istnieć, zanim zapytamy, czego w nim nie ma. Asercja o nieobecności,
    // postawiona na pustym pliku, jest prawdziwa i nic nie znaczy.
    let before = wait_for_lines(&log, 1, LIMIT).await?;
    assert!(
        !before.is_empty(),
        "the prompt envelope has to be in stdin.log before we can claim an interrupt is not; \
         an empty file makes the next assertion pass for free"
    );

    let group = handle.group().ok_or(
        "a live session has to hand over the process group, because that is what T-06 \
                stores and what recovery kills after a crash",
    )?;
    let proof = timeout(LIMIT, handle.cancel()).await?;

    let after = lines_of(&log)?;
    let asked: Vec<&String> = after.iter().filter(|line| line.contains(CONTROL)).collect();
    assert!(
        asked.is_empty(),
        "a CLI that never announced the interrupt capability must not be sent one: the request \
         goes nowhere and the driver spends five seconds waiting for an answer that is not \
         coming. stdin.log held {asked:?}"
    );

    assert!(
        matches!(proof, GroupProof::Dead { .. }),
        "cancelling has to come back with proof, not with a report that a signal was sent. \
         It came back {proof:?}"
    );
    let errno = group_probe(group.pgid).err().and_then(|e| e.raw_os_error());
    assert_eq!(
        errno,
        Some(libc::ESRCH),
        "kill(-{}, 0) still finds somebody in the group. This is the measurement that returned \
         total=2 orphaned=2 in T7 section 3.1 while the child's own exit status said 'killed' - \
         and an orphaned agent burns quota in the background, invisibly",
        group.pgid
    );
    assert!(
        began.elapsed() < BUDGET,
        "cancelling took {:?}",
        began.elapsed()
    );

    Ok(())
}
