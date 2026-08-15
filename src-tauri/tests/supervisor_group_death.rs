//! AC-1 dla T-03: po `stop()` nie żyje **cała grupa**, a dowodem jest `ESRCH`, nie zwrócony
//! status dziecka.
//!
//! Słaba wersja tego kryterium to `assert!(!status.success())` na statusie bezpośredniego
//! dziecka. To jest **dokładnie ten pomiar**, który w T7 §3.1 zwrócił
//! `A after kill: total=2 orphaned=2`: rodzic zginął, status brzmiał „zabity", a dwoje wnucząt
//! przeszło pod PID 1 i dalej paliło limit. Test był zielony, a rachunek rósł w tle.
//!
//! Dlatego mierzy tu **system operacyjny, nie nasz kod**: `kill(-pgid, 0)` musi odpowiedzieć
//! `ESRCH`, a skan `ps -eo pid,ppid,pgid,args` po unikalnym znaczniku nie może znaleźć ani
//! jednego procesu — również żadnego z `ppid == 1`. Obie te rzeczy widzą wnuki, których nasz
//! `wait()` nigdy nie zobaczy.
//!
//! Test odpala prawdziwe procesy, więc jest `#[ignore]` i nie biegnie w pętli wewnętrznej.
//! Uruchamia go wyłącznie bramka, linią `check:` z `-- --include-ignored`; bez tej flagi cargo
//! zamelduje `0 passed`, a to nie jest dowód (niezmiennik 19).

use std::error::Error;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use loadout_lib::engine::supervisor::{self, GroupProof, StdinPlan};
use tokio::process::Command;

/// Okno łaski podane argumentem, nie wzięte ze stałej produkcyjnej: w teście chcemy, żeby
/// niepowodzenie było krótkie, a nie żeby trwało pięć sekund [T7 §3.3].
const GRACE: Duration = Duration::from_secs(2);

/// Rodzic odpala dwoje wnucząt w tle, każde z własnym znacznikiem w `argv`, i czeka.
///
/// Pętla krótkich snów zamiast jednego `sleep 30`: pojedyncza, ostatnia komenda skryptu bywa
/// przez powłokę exec-optymalizowana i wtedy znacznik znika z `argv`, a skan `ps` przestaje
/// cokolwiek widzieć [T7 §8.2].
const PARENT: &str = r#"#!/bin/sh
# $1 = ścieżka skryptu-wnuka, $2 = znacznik
"$1" "$2-a" &
"$1" "$2-b" &
while :; do
  sleep 0.2
done
"#;

/// Wnuk: nic nie robi poza tym, że **jest widoczny w `ps`** pod swoim znacznikiem.
const GRANDCHILD: &str = r#"#!/bin/sh
# $1 = znacznik; ma zostać w argv, więc pętla, nie pojedyncze `sleep`
while :; do
  sleep 0.2
done
"#;

/// Jeden wiersz `ps -eo pid,ppid,pgid,args`.
#[derive(Debug)]
struct PsRow {
    ppid: i32,
    pgid: i32,
    args: String,
}

/// Znacznik unikalny dla tego biegu. Bez unikalności skan `ps` łapałby procesy z poprzedniego,
/// przerwanego biegu i meldował wyciek, którego nie ma — albo zieleń, której nie ma.
fn unique_marker(tag: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("loadout-t03-{tag}-{}-{nanos}", std::process::id())
}

/// Zapisuje wykonywalny skrypt `#!/bin/sh` i zwraca jego ścieżkę.
///
/// Plik ze skryptem, nigdy `sh -c "jedna komenda"` i nigdy kopia `/bin/sleep`: powłoka
/// exec-optymalizuje pojedynczą komendę i znacznik znika z `argv`, a skopiowany binarny plik
/// systemowy dostaje na macOS SIGKILL od podpisu kodu (`Killed: 9`) [T7 §8.2].
fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// Pyta jądro, czy w grupie `pgid` jest jeszcze ktokolwiek — **nie wysyłając sygnału**.
///
/// To jedyny pomiar, który liczy się w niezmienniku 6, i jedyny spoza drzewa naszego procesu:
/// status zebrany przez `wait()` mówi wyłącznie o bezpośrednim dziecku.
// 2026-08-15 — `kill(2)` nie ma bezpiecznego opakowania w std. Plik testowy jest wyłączony ze
// wszystkich trzech granic architektury po ŚCIEŻCE (checks/quick-boundary.sh), bo nie jest
// częścią wysyłanego artefaktu — a ten konkretny test z definicji pyta system operacyjny
// zamiast naszego kodu (niezmiennik 20).
#[allow(unsafe_code)]
fn group_probe(pgid: i32) -> io::Result<()> {
    // SAFETY: `kill` z sygnałem 0 niczego nie dostarcza — sprawdza tylko istnienie i prawa.
    // Argumenty to zwykłe liczby, więc nie ma tu żadnego wskaźnika ani czasu życia do złamania.
    let rc = unsafe { libc::kill(-pgid, 0) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Wiersze `ps` zawierające `marker`. Pomiar spoza naszego drzewa procesów.
async fn ps_scan(marker: &str) -> Result<Vec<PsRow>, Box<dyn Error>> {
    let output = Command::new("ps")
        .args(["-eo", "pid,ppid,pgid,args"])
        .output()
        .await?;
    let text = String::from_utf8_lossy(&output.stdout);

    let mut rows = Vec::new();
    for line in text.lines() {
        if !line.contains(marker) {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 {
            continue;
        }
        let (Ok(ppid), Ok(pgid)) = (fields[1].parse::<i32>(), fields[2].parse::<i32>()) else {
            continue;
        };
        rows.push(PsRow {
            ppid,
            pgid,
            args: fields[3..].join(" "),
        });
    }
    Ok(rows)
}

/// Czeka, aż `ps` pokaże co najmniej `want` procesów ze znacznikiem. Zwraca ostatni skan —
/// także wtedy, gdy jest za krótki, żeby asercja wołającego mogła powiedzieć, czego brakuje.
async fn wait_for_rows(
    marker: &str,
    want: usize,
    limit: Duration,
) -> Result<Vec<PsRow>, Box<dyn Error>> {
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
async fn stop_proves_the_whole_group_is_dead_not_just_the_child() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let marker = unique_marker("group-death");
    let grandchild = write_script(dir.path(), "grandchild.sh", GRANDCHILD)?;
    let parent = write_script(dir.path(), "parent.sh", PARENT)?;

    let mut command = Command::new(&parent);
    command.arg(&grandchild).arg(&marker);

    let mut handle = supervisor::spawn(command, StdinPlan::Null)?;
    let group = handle.group();

    // ── Wnuki naprawdę żyją, zanim cokolwiek zabijemy ─────────────────────────────────────
    // Bez tego cała reszta testu przechodzi na pustym zbiorze: „nie znaleziono procesów ze
    // znacznikiem" jest prawdą także wtedy, gdy żaden nigdy nie wystartował.
    let a = format!("{marker}-a");
    let b = format!("{marker}-b");
    let before = wait_for_rows(&marker, 3, Duration::from_secs(5)).await?;
    assert!(
        before.iter().any(|row| row.args.contains(&a)),
        "the first grandchild never showed up in ps, so this run has nothing to prove dead \
         later; ps saw {before:?}"
    );
    assert!(
        before.iter().any(|row| row.args.contains(&b)),
        "the second grandchild never showed up in ps, so this run has nothing to prove dead \
         later; ps saw {before:?}"
    );
    assert!(
        before.iter().all(|row| row.pgid == group.pgid),
        "every process carrying the marker has to sit in the group we were handed (pgid {}); \
         a grandchild in another group is one that kill(-pgid, ..) will never reach, and that \
         is the whole leak. ps saw {before:?}",
        group.pgid
    );

    // ── Zatrzymanie ───────────────────────────────────────────────────────────────────────
    // Własny limit czasu wokół oczekiwania: bez niego regresja objawi się jako zawieszenie,
    // bramka zwróci rc 124, a to jest fałszywa czerwień, nie dowód.
    let proof = tokio::time::timeout(Duration::from_secs(10), handle.stop(GRACE))
        .await
        .map_err(|_| "stop() did not return within 10s")?;

    assert!(
        matches!(proof, GroupProof::Dead { .. }),
        "stop() has to return the proof that the group is gone, not a report that a signal was \
         sent; it returned {proof:?}"
    );

    let probe = group_probe(group.pgid);
    let errno = probe.err().and_then(|e| e.raw_os_error());
    assert_eq!(
        errno,
        Some(libc::ESRCH),
        "kill(-{}, 0) still finds somebody in the group after stop() called it dead. This is \
         the measurement that returned total=2 orphaned=2 in T7 §3.1 while the child's own \
         exit status said 'killed'",
        group.pgid
    );

    let after = ps_scan(&marker).await?;
    let orphaned: Vec<&PsRow> = after.iter().filter(|row| row.ppid == 1).collect();
    assert!(
        orphaned.is_empty(),
        "total={} orphaned={} — processes carrying our marker were reparented to PID 1 and are \
         still running. That is the leak from T7 §3.1 verbatim, and it burns quota invisibly: \
         {orphaned:?}",
        after.len(),
        orphaned.len()
    );
    assert!(
        after.is_empty(),
        "ps still finds process(es) carrying the marker after stop() returned Dead: {after:?}"
    );

    // ── Drugie zatrzymanie tej samej grupy ────────────────────────────────────────────────
    // Normalna ścieżka, nie błąd: anulowanie biegu kończy się `stop()`, po którym i tak
    // przyjdzie gwardia `Drop`. `stop()` nie zwraca `Result`, więc „nie zwraca błędu" jest tu
    // własnością sygnatury — sprawdzamy, że powtórzenie nadal daje dowód, a nie `Alive`.
    let again = tokio::time::timeout(Duration::from_secs(5), handle.stop(GRACE))
        .await
        .map_err(|_| "the second stop() hung on a group that is already dead")?;
    assert!(
        matches!(again, GroupProof::Dead { .. }),
        "stopping an already-stopped group has to keep answering Dead; it answered {again:?}"
    );

    Ok(())
}
