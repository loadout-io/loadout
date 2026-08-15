//! AC-3 dla T-03: limit czasu przechodzi przez ścieżkę zabijania, a nie przez porzucenie
//! future'a.
//!
//! Słaba wersja tego kryterium to `assert!(tokio::time::timeout(d, fut).await.is_err())`, czyli
//! sprawdzenie, że limit został **zgłoszony**. Przechodzi je jedna linijka:
//! `let _ = tokio::time::timeout(d, child.wait()).await;` — zadanie Rusta znika, proces zostaje
//! przy życiu, a agent dalej pali limit. To jedyny defekt w T7 z adnotacją „łatwo zregresować,
//! pokryj testem" [T7 §10.8, niezmiennik 10].
//!
//! Rozróżniają dwie rzeczy, obie mierzone przez system operacyjny, nie przez nasz kod: `ESRCH`
//! na `-pgid` **po powrocie funkcji** i brak wnuka w `ps`. Wnuk jest tu istotny, bo to on
//! przeżywa `Child::kill()` [T7 §3.1] — a plik gotowości dowodzi, że w ogóle powstał, żeby
//! „nie ma go w `ps`" nie było prawdą trywialnie.

use std::error::Error;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use loadout_lib::engine::supervisor::{self, RunOutcome, StdinPlan};
use tokio::process::Command;

/// Limit podany do `run_with_deadline`. Krótki, bo mierzymy ścieżkę, nie cierpliwość.
const LIMIT: Duration = Duration::from_millis(300);

/// Rodzic: odpala wnuka ze znacznikiem, melduje gotowość i śpi 30 s.
///
/// `exit 0` po `sleep 30` stoi tam po to, żeby `sleep` nie był ostatnią komendą skryptu:
/// powłoka potrafi wtedy zrobić `exec` i cały proces zamienia się w `sleep`, gubiąc `argv`,
/// po którym skanujemy [T7 §8.2].
const PARENT: &str = r#"#!/bin/sh
# $1 = skrypt wnuka, $2 = znacznik, $3 = plik gotowości
"$1" "$2" &
: > "$3"
sleep 30
exit 0
"#;

/// Wnuk: też śpi 30 s i też trzyma znacznik w `argv`.
const GRANDCHILD: &str = r#"#!/bin/sh
# $1 = znacznik
sleep 30
exit 0
"#;

/// Znacznik unikalny dla tego biegu — inaczej skan `ps` widziałby resztki poprzedniego.
fn unique_marker(tag: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("loadout-t03-{tag}-{}-{nanos}", std::process::id())
}

/// Zapisuje wykonywalny skrypt `#!/bin/sh` i zwraca jego ścieżkę [T7 §8.2].
fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// Pyta jądro, czy w grupie `pgid` jest jeszcze ktokolwiek — bez wysyłania sygnału.
// 2026-08-15 — `kill(2)` nie ma bezpiecznego opakowania w std. Plik testowy jest wyłączony ze
// wszystkich trzech granic architektury po ŚCIEŻCE (checks/quick-boundary.sh), a ten pomiar
// z definicji ma pochodzić od systemu operacyjnego, nie od naszego kodu (niezmiennik 20).
#[allow(unsafe_code)]
fn group_probe(pgid: i32) -> io::Result<()> {
    // SAFETY: `kill` z sygnałem 0 niczego nie dostarcza — sprawdza istnienie i prawa. Argumenty
    // to zwykłe liczby, więc nie ma tu wskaźnika ani czasu życia do złamania.
    let rc = unsafe { libc::kill(-pgid, 0) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Wiersze `ps -eo pid,ppid,pgid,args` zawierające `marker`.
async fn ps_scan(marker: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let output = Command::new("ps")
        .args(["-eo", "pid,ppid,pgid,args"])
        .output()
        .await?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains(marker))
        .map(str::to_owned)
        .collect())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "odpala prawdziwe procesy; bramka woła to z --include-ignored"]
async fn the_deadline_goes_through_the_kill_path_and_not_through_a_dropped_future()
-> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let marker = unique_marker("timeout");
    let ready_file = dir.path().join("grandchild-forked");
    let grandchild = write_script(dir.path(), "grandchild.sh", GRANDCHILD)?;
    let parent = write_script(dir.path(), "parent.sh", PARENT)?;

    let mut command = Command::new(&parent);
    command.arg(&grandchild).arg(&marker).arg(&ready_file);

    // Własny limit czasu wokół całości: bez niego regresja objawi się jako zawieszenie, bramka
    // zwróci rc 124, a to jest fałszywa czerwień, nie dowód.
    let began = Instant::now();
    let deadline = supervisor::run_with_deadline(command, LIMIT);
    let outcome = tokio::time::timeout(Duration::from_secs(10), deadline)
        .await
        .map_err(|_| "run_with_deadline() never returned")??;
    let elapsed = began.elapsed();

    assert!(
        matches!(outcome, RunOutcome::TimedOut { .. }),
        "the script sleeps 30s and the deadline was {LIMIT:?}, so the timeout variant is the \
         only honest outcome; it reported {outcome:?}"
    );
    let RunOutcome::TimedOut { group, .. } = outcome else {
        return Err("unreachable: the assertion above already ruled this out".into());
    };

    // Plik gotowości zamyka jedyną furtkę, którą to kryterium dałoby się przejść na pusto:
    // wnuk, którego nigdy nie było, też „nie ma go w ps".
    assert!(
        ready_file.exists(),
        "the parent never got as far as forking its grandchild, so nothing below would be \
         about a grandchild surviving the deadline"
    );

    let probe = group_probe(group.pgid);
    let errno = probe.err().and_then(|e| e.raw_os_error());
    assert_eq!(
        errno,
        Some(libc::ESRCH),
        "after run_with_deadline() returned its timeout variant, kill(-{}, 0) still finds \
         somebody. This is exactly what `let _ = tokio::time::timeout(d, child.wait()).await` \
         looks like from the outside: the Rust task is gone, the agent is not [T7 §10.8]",
        group.pgid
    );

    let survivors = ps_scan(&marker).await?;
    assert!(
        survivors.is_empty(),
        "ps still finds the grandchild after the deadline path returned. The direct child is \
         not the process that burns quota — the grandchild is [T7 §3.1]: {survivors:?}"
    );

    assert!(
        elapsed < Duration::from_secs(5),
        "the whole deadline path took {elapsed:?} for a {LIMIT:?} limit. A timeout that costs \
         seconds to enforce is one that waits for the process on the way out"
    );

    Ok(())
}
