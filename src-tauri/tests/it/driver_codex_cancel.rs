//! AC-6 dla T-10: anulowanie jest wartością i wraca dopiero po dowodzie śmierci.
//!
//! **Słaba wersja tego kryterium to `assert!(matches!(outcome.reason, FinishReason::Cancelled))`.**
//! Przechodzi ją sterownik, który zwraca `Cancelled` **natychmiast**, zostawiając żywy proces.
//! Osierocony agent pali limit w tle — to jest błąd finansowy, nie higieniczny — i nie widać go
//! z okna, bo z okna wszystko wygląda na zatrzymane. Zmierzone w tym samym kształcie:
//! `A after kill: total=2 orphaned=2` przy statusie dziecka mówiącym „zabity" [T7 §3.1].
//!
//! Rozróżniają to dwie asercje:
//!
//! 1. po powrocie z `wait()` `kill(-pgid, 0)` odpowiada `ESRCH` — w grupie nie ma **nikogo**,
//!    także żadnego zombie, bo zombie nadal odpowiada na sygnał zerowy;
//! 2. `wait()` **nie** rozstrzyga się przed upływem tych 200 ms, które atrapa spędza,
//!    ignorując SIGTERM — czyli sterownik naprawdę czekał na dowód, zamiast założyć skutek.
//!
//! Druga bez pierwszej przechodzi dla sterownika, który po prostu śpi; pierwsza bez drugiej —
//! dla takiego, który zdąży zapytać jądro, zanim proces w ogóle dostanie sygnał.
//!
//! **Anulowanie jest wartością, nigdy błędem** (niezmiennik 7). `Err(Cancelled)` z `wait()`
//! zmusza każdego wołającego do rozróżniania „to się nie udało" od „to zatrzymał człowiek",
//! a rozróżnienie zgubione raz jest zgubione wszędzie: UI mówi wtedy, że coś się zepsuło,
//! bo `?` wrzuciło Stop do tej samej gałęzi co padnięte połączenie.
//!
//! Ten plik pyta jądro o sygnały, a sam sterownik nie ma do nich prawa (niezmiennik 3):
//! `checks/quick-boundary.sh` wyłącza ścieżki `*/tests/*`, bo pliki testowe nie są częścią
//! wysyłanego artefaktu.

use std::error::Error;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::{
    AgentEvent, AgentHandle, DecodedEvent, FinishReason, Policy, RunSpec,
};
use loadout_lib::engine::supervisor::GroupProof;
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

/// Sufit na pojedyncze oczekiwanie. Regresja ma być czerwonym testem, nie zawieszeniem:
/// bramka odpowiada na zawieszenie kodem 124, a to jest fałszywa czerwień.
const LIMIT: Duration = Duration::from_secs(8);

/// Sufit na całość. Anulowanie, które trwa dłużej, jest anulowaniem, na które użytkownik
/// przestaje czekać.
const BUDGET: Duration = Duration::from_secs(15);

/// Ile atrapa ignoruje SIGTERM. Dokładnie o tyle `wait()` ma **nie** zdążyć.
const IGNORES_TERM: Duration = Duration::from_millis(200);

/// Miejsce w kanale zdarzeń, z zapasem.
const CHANNEL: usize = 256;

/// Atrapa `codex`, która nie daje się zdjąć od razu.
///
/// Mówi jedno zdanie (żeby test miał dowód, że tura naprawdę ruszyła, zanim ją przerwiemy),
/// a potem **ignoruje SIGTERM przez 200 ms** i wychodzi sama. Kopia `sleep` w pętli jest
/// dzieckiem powłoki, więc grupa ma kogo stracić przy pierwszym sygnale — a `sleep 0.2` po
/// złapaniu pułapki jest już nowym procesem i przeżywa dokładnie tyle, ile trzeba.
const IGNORES_SIGTERM: &str = r#"#!/bin/sh
printf '{"type":"thread.started","thread_id":"thread-cancel"}\n'
printf '{"type":"item.completed","item":{"type":"agent_message","id":"item_0","text":"working"}}\n'

caught=""
trap 'caught=1' TERM

while [ -z "$caught" ]; do
  sleep 0.05
done

sleep 0.2
exit 0
"#;

/// Zapisuje wykonywalny skrypt i zwraca jego ścieżkę.
fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// Pyta jądro, czy w grupie `pgid` jest jeszcze ktokolwiek — **nie wysyłając sygnału**.
// `kill(2)` nie ma bezpiecznego opakowania w std, a ten plik z definicji pyta system
// operacyjny zamiast naszego kodu (niezmiennik 20). Pliki testowe są wyłączone z granic
// architektury po ŚCIEŻCE (checks/quick-boundary.sh), bo nie są częścią wysyłanego artefaktu.
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

/// Kolejne zdarzenie z kanału sterownika.
async fn next_event(rx: &mut mpsc::Receiver<DecodedEvent>) -> Result<AgentEvent, Box<dyn Error>> {
    let decoded = timeout(LIMIT, rx.recv())
        .await?
        .ok_or("the driver closed the event channel before the turn ever said anything")?;
    Ok(decoded.event)
}

/// Wszystko, co zostało w kanale, aż do jego końca.
async fn rest_of(rx: &mut mpsc::Receiver<DecodedEvent>) -> Result<Vec<AgentEvent>, Box<dyn Error>> {
    let mut events = Vec::new();
    while let Some(decoded) = timeout(LIMIT, rx.recv()).await? {
        events.push(decoded.event);
    }
    Ok(events)
}

/// `RunSpec` jednej tury.
fn spec(cwd: &Path) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: "start a long job".to_owned(),
        model: Some("gpt-5-codex".to_owned()),
        system_append: None,
        reaches_the_web: false,
        policy: Policy::ReadOnly,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_comes_back_as_a_value_and_only_after_the_group_is_gone()
-> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let binary = write_script(dir.path(), "codex", IGNORES_SIGTERM)?;

    let (tx, mut rx) = mpsc::channel(CHANNEL);
    let driver = CodexDriver::with_binary(binary);
    let mut handle = timeout(LIMIT, driver.start_session(spec(dir.path()), tx)).await??;

    // Anulowanie tury, która jeszcze nie ruszyła, mierzyłoby wyścig zamiast eskalacji.
    let opening = next_event(&mut rx).await?;
    assert!(
        matches!(opening, AgentEvent::Said { .. }),
        "the dummy speaks one line before it starts ignoring signals; without it this test would \
         be cancelling a turn that had not begun. It produced {opening:?}"
    );

    let group = handle.group().ok_or(
        "a live turn has to hand over its process group: that is what T-06 stores next to the \
         step and what recovery kills after a crash",
    )?;

    // ── Anulowanie ────────────────────────────────────────────────────────────────────────
    let began = Instant::now();
    let proof = timeout(LIMIT, handle.cancel()).await?;
    let outcome = timeout(LIMIT, handle.wait()).await??;
    let waited = began.elapsed();

    assert!(
        waited >= IGNORES_TERM,
        "cancelling came back in {waited:?}, and the dummy spends {IGNORES_TERM:?} ignoring \
         SIGTERM before it exits. Returning sooner means the driver ASSUMED the effect instead \
         of waiting for the proof - and the process it left behind burns quota in the \
         background, invisibly"
    );

    assert_eq!(
        outcome.reason,
        FinishReason::Cancelled,
        "cancelling is a value, never an error (invariant 7): a step somebody stopped on purpose \
         must not land in the same branch as a connection that broke, or the window says \
         something is wrong when nothing is. It came out as {outcome:?}"
    );
    assert!(
        !outcome.ok,
        "a cancelled turn did not finish its work, whatever else it did. It came out as {outcome:?}"
    );

    // ── Dowód śmierci ─────────────────────────────────────────────────────────────────────
    assert!(
        matches!(proof, GroupProof::Dead { .. }),
        "cancelling has to come back with PROOF, not with a report that a signal was sent. \
         It came back {proof:?}"
    );
    let errno = group_probe(group.pgid).err().and_then(|e| e.raw_os_error());
    assert_eq!(
        errno,
        Some(libc::ESRCH),
        "kill(-{}, 0) still finds somebody in the group. This is the measurement that came back \
         total=2 orphaned=2 in T7 section 3.1 while the child's own exit status said 'killed'",
        group.pgid
    );

    // ── Drugie anulowanie ─────────────────────────────────────────────────────────────────
    let again = timeout(LIMIT, handle.cancel()).await?;
    assert!(
        matches!(again, GroupProof::Dead { .. }),
        "cancelling twice has to stay truthful: the group is still gone, so the answer is still \
         proof of death. It came back {again:?}"
    );

    drop(handle);
    let mut events = vec![opening];
    events.extend(rest_of(&mut rx).await?);

    let endings = events
        .iter()
        .filter(|event| matches!(event, AgentEvent::Finished(_)))
        .count();
    assert_eq!(
        endings, 1,
        "exactly one end of turn, even though cancel() was called twice. A second Finished draws \
         a second summary on the rail for a turn that happened once. The stream produced {events:?}"
    );
    let last = events
        .last()
        .ok_or("the stream carried nothing at all, so there was no turn to cancel")?;
    assert!(
        matches!(last, AgentEvent::Finished(_)),
        "and nothing may arrive after it. The stream produced {events:?}"
    );

    assert!(
        began.elapsed() < BUDGET,
        "cancelling took {:?}, which is longer than anybody keeps waiting for a Stop button",
        began.elapsed()
    );

    Ok(())
}
