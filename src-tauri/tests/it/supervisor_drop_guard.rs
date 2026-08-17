//! AC-6 dla T-03: uchwyt porzucony na ścieżce błędu i tak zabija grupę, a po biegu nie zostaje
//! zombie.
//!
//! Słaba wersja tego kryterium to `assert!(child.id().is_none())` po `wait()`. Mówi tylko, że
//! **my** przestaliśmy trzymać uchwyt; nie mówi nic o grupie i w ogóle nie dotyka ścieżki, na
//! której wołający wraca wcześniej przez `?` — a to jest ta ścieżka, którą naprawdę wychodzi
//! się z funkcji spawnującej. Rozróżnia je `ESRCH` po **samym porzuceniu uchwytu**, bez ani
//! jednego wywołania `stop()`.
//!
//! Druga część pilnuje zombie: `wait()` musi paść na każdej ścieżce terminalnej [T7 §3.3].
//! Zombie nadal odpowiada na sygnał zerowy, więc grupa z zombie w środku **nigdy** nie da
//! `ESRCH` — czyli dowód z niezmiennika 6 nie nadejdzie i `stop()` będzie czekać na coś, co
//! już nie żyje.

use std::error::Error;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use loadout_lib::engine::supervisor::{self, GroupProof, StdinPlan};
use tokio::process::Command;

/// Okno łaski dla `stop()` w tym teście.
const GRACE: Duration = Duration::from_secs(2);

/// Ile procesów przemielić w części drugiej. Pięć, bo jeden nie odróżnia „zbieramy" od
/// „mieliśmy szczęście".
const ROUNDS: usize = 5;

/// Rodzic odpala wnuka i czeka. Ten uchwyt zostanie porzucony **bez** `stop()`.
const PARENT: &str = r#"#!/bin/sh
# $1 = skrypt wnuka, $2 = znacznik
"$1" "$2" &
while :; do
  sleep 0.2
done
"#;

/// Wnuk: widoczny w `ps` pod znacznikiem, dopóki żyje.
const GRANDCHILD: &str = r"#!/bin/sh
# $1 = znacznik
while :; do
  sleep 0.2
done
";

/// Krótki proces do części drugiej: żyje na tyle długo, żeby `stop()` zastało go żywym.
const SHORT: &str = r"#!/bin/sh
sleep 0.1
exit 0
";

/// Znacznik unikalny dla tego biegu.
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

/// Stan procesu według `ps`. Pusty napis, kiedy procesu już nie ma — i to jest wynik dobry.
async fn ps_state(pid: i32) -> Result<String, Box<dyn Error>> {
    let pid_text = pid.to_string();
    let output = Command::new("ps")
        .args(["-o", "stat=", "-p", &pid_text])
        .output()
        .await?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Czeka na `ESRCH` dla grupy, najwyżej przez `limit`. Zwraca zmierzony czas albo `None`.
async fn wait_for_esrch(pgid: i32, limit: Duration) -> Option<Duration> {
    let began = Instant::now();
    loop {
        let errno = group_probe(pgid).err().and_then(|e| e.raw_os_error());
        if errno == Some(libc::ESRCH) {
            return Some(began.elapsed());
        }
        if began.elapsed() >= limit {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Czeka, aż `ps` pokaże co najmniej `want` procesów ze znacznikiem.
async fn wait_for_rows(
    marker: &str,
    want: usize,
    limit: Duration,
) -> Result<Vec<String>, Box<dyn Error>> {
    let deadline = Instant::now() + limit;
    loop {
        let rows = ps_scan(marker).await?;
        if rows.len() >= want || Instant::now() >= deadline {
            return Ok(rows);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "odpala prawdziwe procesy; bramka woła to z --include-ignored"]
async fn a_handle_dropped_without_stop_still_takes_the_group_with_it() -> Result<(), Box<dyn Error>>
{
    let dir = tempfile::tempdir()?;
    let marker = unique_marker("drop-guard");
    let grandchild = write_script(dir.path(), "grandchild.sh", GRANDCHILD)?;
    let parent = write_script(dir.path(), "parent.sh", PARENT)?;

    let mut command = Command::new(&parent);
    command.arg(&grandchild).arg(&marker);

    let group = {
        let handle = supervisor::spawn(command, StdinPlan::Null)?;
        let group = handle.group();

        // Zanim porzucimy uchwyt, grupa musi mieć kogo stracić: bez tego „ps nic nie znalazł"
        // jest prawdą także dla grupy, która nigdy nie wystartowała.
        let alive = wait_for_rows(&marker, 2, Duration::from_secs(5)).await?;
        assert!(
            alive.len() >= 2,
            "the parent and its grandchild were supposed to be running before the handle is \
             dropped; ps saw {alive:?}"
        );

        group
        // 2026-08-15 — uchwyt ginie TUTAJ, bez ani jednego wywołania stop(). To jest symulacja
        // wczesnego `?`, czyli ścieżki, którą naprawdę wychodzi się z funkcji spawnującej.
    };

    let observed = wait_for_esrch(group.pgid, Duration::from_secs(1)).await;
    assert!(
        observed.is_some(),
        "one second after the handle was dropped, kill(-{}, 0) still finds the group. A handle \
         that leaves its group behind on the error path leaves a claude burning quota, and \
         nobody is left holding anything to stop it [T7 §3.1]",
        group.pgid
    );

    let survivors = ps_scan(&marker).await?;
    assert!(
        survivors.is_empty(),
        "ps still finds processes carrying the marker after the handle was dropped: \
         {survivors:?}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "odpala prawdziwe procesy; bramka woła to z --include-ignored"]
async fn a_run_of_short_processes_leaves_no_zombie_behind() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let script = write_script(dir.path(), "short.sh", SHORT)?;

    let mut seen = Vec::new();
    for _ in 0..ROUNDS {
        let command = Command::new(&script);
        let mut handle = supervisor::spawn(command, StdinPlan::Null)?;
        seen.push(handle.group().pid);

        let proof = tokio::time::timeout(Duration::from_secs(10), handle.stop(GRACE))
            .await
            .map_err(|_| "stop() did not return within 10s")?;
        assert!(
            matches!(proof, GroupProof::Dead { .. }),
            "stopping a short-lived process still has to end in proof, not a guess: {proof:?}"
        );
    }

    // Bez tego cała pętla poniżej przechodzi na pustym zbiorze, co jest tym samym rodzajem
    // fałszywej zieleni, przed którym broni niezmiennik 19.
    assert!(
        !seen.is_empty(),
        "no pid was recorded, so the zombie check below would assert over nothing"
    );

    for pid in &seen {
        let state = ps_state(*pid).await?;
        assert!(
            !state.contains('Z'),
            "pid {pid} is a zombie ({state:?}) after its run finished. Every terminal path has \
             to wait() the child [T7 §3.3] — and a zombie keeps answering kill(-pgid, 0), so \
             its group can never produce the ESRCH that invariant 6 asks for"
        );
    }

    Ok(())
}
