//! AC-2 dla T-03: prowadzimy SIGTERM-em, a SIGKILL przychodzi dopiero po oknie łaski.
//!
//! Słaba wersja tego kryterium to „proces zniknął". Spełnia ją prowadzenie SIGKILL-em, które
//! jest o tyle gorsze, że `claude` na SIGTERM dosypuje transkrypt, zwalnia zamek sesji i odpala
//! hooki `SessionEnd` (wychodząc 143), a na SIGKILL nie robi nic z tych rzeczy [T1 §4.6,
//! 2026-08-15]. Efekt jest niewidoczny aż do pierwszej sesji, której nie da się wznowić.
//!
//! Rozróżniają dwie rzeczy, obie w tym pliku: **istnienie pliku znacznika** w przypadku
//! grzecznym — bo zapisuje go dopiero handler SIGTERM, więc jego istnienie dowodzi, że sygnał
//! dotarł i został *obsłużony*, a nie że proces po prostu zniknął — **oraz `elapsed >= grace`**
//! w przypadku upartym, bo tylko to dowodzi, że na łaskę naprawdę czekaliśmy.
//!
//! Okno łaski to w produkcji 5–10 s i jedno ukryte ustawienie, nigdy kontrolka w UI [T7 §3.3];
//! tutaj idzie argumentem, żeby test trwał sekundy, a nie dziesiątki sekund.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// Adres tymczasowy. Docelowo `loadout_lib::engine::supervisor` — powód, dla którego dziś stoi
// on w korzeniu skrzyni, i warunek przestawienia stoją przy deklaracji w `src-tauri/src/lib.rs`.
use loadout_lib::supervisor::{self, GroupProof, StdinPlan};
use tokio::process::Command;

/// Okno łaski w tym teście. Musi być wyraźnie dłuższe niż czas reakcji grzecznego skryptu
/// (~0,2 s) i wyraźnie krótsze niż limit, w który owijamy całe oczekiwanie.
const GRACE: Duration = Duration::from_secs(2);

/// Grzeczny: łapie SIGTERM, zapisuje znacznik, wychodzi czysto.
///
/// Plik gotowości powstaje **po** zainstalowaniu trapu i przed pętlą. Bez tej synchronizacji
/// SIGTERM potrafi dotrzeć, zanim `trap` w ogóle się wykona — proces ginie od akcji domyślnej,
/// znacznika nie ma i test oskarża implementację o prowadzenie KILL-em, którego nie było.
const POLITE: &str = r#"#!/bin/sh
# $1 = plik znacznika (pisany dopiero z handlera), $2 = plik gotowości
MARKER_FILE="$1"
trap 'echo bye > "$MARKER_FILE"; exit 0' TERM
: > "$2"
while :; do
  sleep 0.2
done
"#;

/// Uparty: SIGTERM ignoruje. Wyjść może wyłącznie od SIGKILL-a po oknie łaski.
const STUBBORN: &str = r#"#!/bin/sh
# $1 = plik gotowości
trap '' TERM
: > "$1"
while :; do
  sleep 0.2
done
"#;

/// Zapisuje wykonywalny skrypt `#!/bin/sh` i zwraca jego ścieżkę.
///
/// Plik ze skryptem, nigdy `sh -c "jedna komenda"`: powłoka exec-optymalizuje pojedynczą
/// komendę, a wtedy trap instaluje się w procesie, którego już nie ma [T7 §8.2].
fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// Czeka, aż skrypt zamelduje gotowość. Zwraca `false`, jeśli się nie doczekał.
async fn wait_for_ready(path: &Path, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "odpala prawdziwe procesy; bramka woła to z --include-ignored"]
async fn a_polite_child_handles_sigterm_and_we_do_not_wait_out_the_grace()
-> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let marker_file = dir.path().join("sigterm-was-handled");
    let ready_file = dir.path().join("trap-installed");
    let script = write_script(dir.path(), "polite.sh", POLITE)?;

    let mut command = Command::new(&script);
    command.arg(&marker_file).arg(&ready_file);

    let mut handle = supervisor::spawn(command, StdinPlan::Null)?;
    assert!(
        wait_for_ready(&ready_file, Duration::from_secs(5)).await,
        "the script never reported that its TERM trap was installed, so nothing this test \
         measures afterwards would be about SIGTERM"
    );

    let began = Instant::now();
    let proof = tokio::time::timeout(Duration::from_secs(10), handle.stop(GRACE))
        .await
        .map_err(|_| "stop() did not return within 10s")?;
    let elapsed = began.elapsed();

    let GroupProof::Dead { status } = proof else {
        return Err("stop() returned Alive: a polite child exits on the first signal".into());
    };

    // Istnienie pliku jest tu całym dowodem: pisze go wyłącznie handler SIGTERM-a. Proces
    // zabity dziewiątką nie ma jak go zostawić, a „proces zniknął" spełnia też prowadzenie
    // SIGKILL-em, które kosztuje transkrypt i zamek sesji [T1 §4.6].
    let Ok(handled) = fs::read_to_string(&marker_file) else {
        return Err("the SIGTERM handler never ran: no marker file".into());
    };
    assert!(
        !handled.trim().is_empty(),
        "the marker file exists but is empty, so the handler was interrupted rather than run"
    );

    // Bez statusu to kryterium nie odróżnia czystego wyjścia od dziewiątki, czyli nie odróżnia
    // niczego, o co w nim chodzi.
    let Some(status) = status else {
        return Err("stop() proved Dead without the leader's exit status".into());
    };
    assert_eq!(
        status.signal(),
        None,
        "the child handled SIGTERM and exited by itself, so its status must not carry a signal \
         at all; it carried {status:?}"
    );
    assert_eq!(
        status.code(),
        Some(0),
        "the trap ends in `exit 0`, so a clean exit is the only status this run can produce; \
         it produced {status:?}"
    );
    assert!(
        elapsed < Duration::from_secs(1),
        "stop() took {elapsed:?} against a child that dies on the first signal. The grace \
         window is a ceiling to escalate from, not a delay to sit out: waiting {GRACE:?} here \
         would cost that on every cancelled step of every run"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "odpala prawdziwe procesy; bramka woła to z --include-ignored"]
async fn a_stubborn_child_survives_the_grace_and_then_takes_signal_nine()
-> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let ready_file = dir.path().join("trap-installed");
    let script = write_script(dir.path(), "stubborn.sh", STUBBORN)?;

    let mut command = Command::new(&script);
    command.arg(&ready_file);

    let mut handle = supervisor::spawn(command, StdinPlan::Null)?;
    assert!(
        wait_for_ready(&ready_file, Duration::from_secs(5)).await,
        "the script never reported that it was ignoring SIGTERM, so a fast stop() below would \
         not mean what this test reads into it"
    );

    let began = Instant::now();
    let proof = tokio::time::timeout(Duration::from_secs(15), handle.stop(GRACE))
        .await
        .map_err(|_| "stop() never returned against a child that ignores SIGTERM")?;
    let elapsed = began.elapsed();

    let GroupProof::Dead { status } = proof else {
        return Err("stop() returned Alive: the escalation to SIGKILL never happened".into());
    };

    assert!(
        elapsed >= GRACE,
        "stop() returned after {elapsed:?}, which is less than the {GRACE:?} it was given. \
         Escalating before the window has elapsed is leading with SIGKILL wearing a timer: \
         claude would lose the transcript and the session lock it flushes on SIGTERM [T1 §4.6]"
    );
    assert!(
        elapsed < GRACE + Duration::from_secs(3),
        "stop() returned after {elapsed:?}: it waited out the grace window and then kept \
         waiting, so the SIGKILL is late rather than paced"
    );

    let Some(status) = status else {
        return Err("stop() proved Dead without the leader's exit status".into());
    };
    assert_eq!(
        status.signal(),
        Some(9),
        "a child that ignores SIGTERM can only be ended by SIGKILL, so its status has to carry \
         signal 9; it carried {status:?}"
    );

    Ok(())
}
