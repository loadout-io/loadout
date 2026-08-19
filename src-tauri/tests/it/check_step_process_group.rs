//! AC-3 dla T-55: komenda sprawdzająca biegnie pod TYM SAMYM nadzorem co agent i zostawia po
//! sobie **dowód śmierci** — nie raport o wysłanym sygnale.
//!
//! # Kształt fikstury jest kształtem prawdziwej komendy
//!
//! `verify.sh` woła `cargo`, `cargo` woła `rustc`; `npm test` woła `vitest`. Każda komenda
//! sprawdzająca rozwidla dzieci, więc skrypt niżej odpala **dwoje dzieci w tle** i sam kręci się
//! dalej. Pętla krótkich snów, nie pojedyncze `sleep`: powłoka exec-optymalizuje ostatnią komendę
//! i znacznik znika wtedy z `argv`, a skan `ps` przestaje cokolwiek widzieć [T7 §8.2].
//!
//! # SŁABA WERSJA
//!
//! `assert!(matches!(how, CheckHow::Stopped(_)))`. Przechodzi dla DWÓCH złych implementacji:
//!
//! * tej, która owija czekanie w `tokio::time::timeout` i wraca, **nie wysławszy ani jednego
//!   sygnału** — czyli zostawia żywą grupę mielącą w tle (niezmiennik 10);
//! * tej, która robi `child.kill()`, bo bezpośrednie dziecko naprawdę ginie, a `wait()` naprawdę
//!   wraca ze statusem „zabity". Dokładnie ten pomiar zwrócił `A after kill: total=2 orphaned=2`
//!   [T7 §3.1] — test był zielony, a rachunek rósł w tle.
//!
//! Rozróżniają to wyłącznie asercje (c) i (d), bo obie mierzą SYSTEM OPERACYJNY, a nie naszą
//! wartość zwrotną. W tej fali ten sam kształt zmierzono drugi raz: hak repo gospodarza startował
//! proces we własnej grupie, jego dziecko dostawało `ppid=1` i **przeżywało wyjście** — jeden bieg
//! zostawił 14 sierot [zmierzone 2026-08-19].
//!
//! # Dlaczego to NIE jest `#[ignore]`
//!
//! Procesy, które ten test odpala, to `/bin/sh` i dwa skrypty w `tempfile::tempdir()` —
//! milisekundy i zero pieniędzy, w przeciwieństwie do testów z prawdziwym `claude`. Linia
//! `check:` tego kryterium nie niesie `--include-ignored`, więc test oznaczony `#[ignore]`
//! zameldowałby `0 passed`, a to nie jest dowód (niezmiennik 19).
//!
//! # Granica z niezmiennika 3
//!
//! Sygnał zerowy **w pliku testu** jest w porządku: `checks/quick-boundary.sh` wyłącza ścieżki
//! `*/tests/*` ze wszystkich trzech granic, po ŚCIEŻCE, nigdy po treści, bo test nie jest częścią
//! wysyłanego artefaktu. To, że `command.rs` tej granicy nie przekracza, sprawdza tamten skrypt,
//! nie ten test — jest tu wymienione, żeby nikt nie „naprawił" testu, wkładając `libc::kill`
//! do sterownika.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use loadout_lib::engine::drivers::command::{CheckHow, CheckSpec, CommandDriver};
use loadout_lib::engine::supervisor::GroupProof;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

/// Sufit cierpliwości. Bez niego regresja objawi się jako zawieszenie, bramka zwróci rc 124,
/// a to jest fałszywa czerwień, nie dowód.
const PATIENCE: Duration = Duration::from_secs(20);

/// Rodzic odpala dwoje wnucząt w tle, każde z własnym znacznikiem w `argv`, i czeka.
const PARENT: &str = r#"#!/bin/sh
# $1 = ścieżka skryptu-wnuka, $2 = znacznik
"$1" "$2-a" &
"$1" "$2-b" &
while :; do
  sleep 0.2
done
"#;

/// Wnuk: nic nie robi poza tym, że **jest widoczny w `ps`** pod swoim znacznikiem.
const GRANDCHILD: &str = r"#!/bin/sh
# $1 = znacznik; ma zostać w argv, więc pętla, nie pojedyncze `sleep`
while :; do
  sleep 0.2
done
";

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
    format!("loadout-t55-{tag}-{}-{nanos}", std::process::id())
}

/// Zapisuje wykonywalny skrypt `#!/bin/sh` i zwraca jego ścieżkę.
fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// Pyta JĄDRO, czy w grupie `pgid` jest jeszcze ktokolwiek — nie wysyłając sygnału.
///
/// To jedyny pomiar, który liczy się w niezmienniku 6, i jedyny spoza drzewa naszego procesu:
/// status zebrany przez `wait()` mówi wyłącznie o bezpośrednim dziecku, a płacimy za wnuki.
// `kill(2)` nie ma bezpiecznego opakowania w std, a ten test z definicji pyta system operacyjny
// zamiast naszego kodu (niezmiennik 20). Plik testowy jest wyłączony ze wszystkich trzech granic
// architektury po ŚCIEŻCE (checks/quick-boundary.sh).
#[allow(unsafe_code)]
fn group_probe(pgid: i32) -> io::Result<()> {
    // ZAPORA, NIE OZDOBA. `kill(-0, …)` znaczy „moja własna grupa procesów", czyli ten proces
    // testowy i wszystko, co go uruchomiło. Szkielet, który nie startuje niczego, oddaje `pgid`
    // równy zeru — a pytanie o niego wyglądałoby jak zieleń, zamiast jak brak procesu.
    assert!(
        pgid > 1,
        "pgid {pgid} is not a process group this test may ask about: 0 means our own group and \
         the answer would be about the test runner, not about the check"
    );
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
        // `parent`/`group` zamiast `ppid`/`pgid`: dwie nazwy różniące się jedną literą w środku to
        // dokładnie ten rodzaj pary, w której podmiana jednej na drugą przechodzi przez recenzję
        // niezauważona — a tutaj jedna odpowiada na „czy osierocony", druga na „czy w naszej
        // grupie".
        let (Ok(parent), Ok(group)) = (fields[1].parse::<i32>(), fields[2].parse::<i32>()) else {
            continue;
        };
        rows.push(PsRow {
            ppid: parent,
            pgid: group,
            args: fields[3..].join(" "),
        });
    }
    Ok(rows)
}

/// Czeka, aż `ps` pokaże co najmniej `want` procesów ze znacznikiem. Zwraca ostatni skan — także
/// wtedy, gdy jest za krótki, żeby asercja wołającego mogła powiedzieć, czego brakuje.
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
async fn a_stopped_check_leaves_no_grandchild_behind() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let marker = unique_marker("check-group");
    let grandchild = write_script(dir.path(), "grandchild.sh", GRANDCHILD)?;
    let parent = write_script(dir.path(), "parent.sh", PARENT)?;

    let spec = CheckSpec {
        // Wiersz powłoki, dosłownie jak wpisałby go człowiek — nie lista argumentów.
        command: format!("{} {} {marker}", parent.display(), grandchild.display()),
        proof: r"(\d+) passed".to_owned(),
        cwd: dir.path().to_path_buf(),
    };

    let driver = CommandDriver::new();
    let mut live = driver.start(&spec)?;

    // ── (a) GRUPA JEST ZNANA, ZANIM PRZECZYTALIŚMY JEDEN BAJT WYJŚCIA ─────────────────────
    // `pgid` jest zwykłą wartością dostępną od razu po starcie, nie czymś wyłuskanym z pierwszej
    // linii [T7 §6.2]. To jest ta kolejność — „wygeneruj, zapisz, dopiero potem czytaj" — która
    // w ogóle czyni sprzątanie po awarii aplikacji możliwym.
    let group = live.group();
    assert!(
        group.pgid > 1,
        "the check has to report a real process group the moment it starts; it reported {group:?}"
    );

    // ── Wnuki naprawdę żyją, zanim cokolwiek zatrzymamy ───────────────────────────────────
    // Bez tego cała reszta testu przechodzi na PUSTYM ZBIORZE: „nie znaleziono procesów ze
    // znacznikiem" jest prawdą także wtedy, gdy żaden nigdy nie wystartował.
    let a = format!("{marker}-a");
    let b = format!("{marker}-b");
    let before = wait_for_rows(&marker, 3, Duration::from_secs(5)).await?;
    assert!(
        before.iter().any(|row| row.args.contains(&a)),
        "the first grandchild never showed up in ps, so this run has nothing to prove dead later. \
         A check step that starts no process cannot leak one either — and cannot check anything. \
         ps saw {before:?}"
    );
    assert!(
        before.iter().any(|row| row.args.contains(&b)),
        "the second grandchild never showed up in ps; ps saw {before:?}"
    );
    assert!(
        before.iter().all(|row| row.pgid == group.pgid),
        "every process carrying the marker has to sit in the group we were handed (pgid {}); a \
         grandchild in another group is one that a group-wide stop will never reach, and that is \
         the whole leak. ps saw {before:?}",
        group.pgid
    );

    // ── (b) ZATRZYMANIE W TRAKCIE ODDAJE WARTOŚĆ, NIE BŁĄD ────────────────────────────────
    // Token anulowany PRZED wywołaniem, a nie w wyścigu z nim: proces właśnie potwierdził w `ps`,
    // że biegnie, więc to jest zatrzymanie w trakcie — tylko bez chwili, w której wynik zależy od
    // tego, który poll wypadł pierwszy.
    let cancel = CancellationToken::new();
    cancel.cancel();
    let end = tokio::time::timeout(PATIENCE, live.settle(&cancel))
        .await
        .map_err(|_| format!("the check did not come back within {PATIENCE:?}"))?;

    assert_eq!(
        end.group, group,
        "the group in the result is the same plain value the check reported at start — not \
         something read back out of the output"
    );
    let described = format!("{:?}", end.how);
    let CheckHow::Stopped(proof) = end.how else {
        return Err(format!(
            "a check stopped by a person is Stopped, and stopping is a VALUE, never an error \
             (invariant 7). It came back as: {described}"
        )
        .into());
    };
    assert!(
        matches!(proof, GroupProof::Dead { .. }),
        "and it carries the proof that the group is GONE, not a report that a signal was sent. \
         `Ok(())` after a signal reads as 'dead' to the caller while the group is still burning \
         quota (invariant 6). It carried {proof:?}"
    );

    // ── (c) PYTAMY JĄDRO, NIE SIEBIE ──────────────────────────────────────────────────────
    let asked = group_probe(group.pgid);
    let errno = asked.err().and_then(|error| error.raw_os_error());
    assert_eq!(
        errno,
        Some(libc::ESRCH),
        "kill(-{}, 0) still finds somebody in the group after the check called itself stopped. \
         This is the measurement that returned total=2 orphaned=2 in T7 §3.1 while the child's \
         own exit status said 'killed'",
        group.pgid
    );

    // ── (d) I TA ASERCJA WIDZI WNUKI, KTÓRYCH NASZ `wait()` NIE ZOBACZY NIGDY ─────────────
    let after = ps_scan(&marker).await?;
    let orphaned: Vec<&PsRow> = after.iter().filter(|row| row.ppid == 1).collect();
    assert!(
        orphaned.is_empty(),
        "total={} orphaned={} — processes carrying our marker were reparented to PID 1 and are \
         still running. That is the leak from T7 §3.1 verbatim, measured a second time on this \
         wave as 14 orphans from one run, and it burns quota invisibly: {orphaned:?}",
        after.len(),
        orphaned.len()
    );
    assert!(
        after.is_empty(),
        "ps still finds process(es) carrying the marker after the check reported Dead: {after:?}"
    );

    // ── (e) DRUGIE ZATRZYMANIE TEJ SAMEJ GRUPY ────────────────────────────────────────────
    // Normalna ścieżka, nie błąd: anulowanie biegu kończy się zatrzymaniem, po którym i tak
    // przyjdzie gwardia porzucenia uchwytu. Status jest do odebrania raz, a odpowiedź „nie żyje"
    // musi zostać ta sama.
    let again = tokio::time::timeout(Duration::from_secs(5), live.cancel())
        .await
        .map_err(|_| "the second stop hung on a group that is already dead")?;
    assert!(
        matches!(again, GroupProof::Dead { .. }),
        "stopping an already-stopped check has to keep answering Dead; it answered {again:?}"
    );
    Ok(())
}
