//! AC-2 dla T-72: zamknięcie okna nie zostawia sierot — także wtedy, gdy człowiek uruchomił
//! dwie rzeczy.
//!
//! # Dlaczego DWIE, a nie jedna
//!
//! Bo test na jednej przechodzi dla implementacji trzymającej JEDEN uchwyt — czyli dla tej,
//! w której drugie `/start` osieroca pierwsze. Ta wada nie objawia się niczym na ekranie:
//! kafelków jest dwa, oba mówią „running", a uchwyt jest jeden, więc jedna z tych grup nie ma
//! już nikogo, kto mógłby zażądać od niej dowodu śmierci. Dokładnie ten kształt zamknęło T-69
//! po stronie biegów (`ipc::AppState::begin_run`) i T-71 po stronie żywej komendy — a wraca on
//! powierzchnia po powierzchni, więc rozróżnia to tutaj asercja (a): **różne** grupy i oba wpisy
//! naraz w rejestrze.
//!
//! # Dlaczego to kończy się dowodem, a nie liczbą
//!
//! Powód stoi w nagłówku `recovery.rs`: rzecz, która przeżyje Loadouta, przechodzi pod PID 1
//! i pracuje dalej, a odzyskiwanie po niej nie posprząta — nie ma wpisu w indeksie biegów.
//! „Zamknięto dwie" jest zdaniem o naszej pętli; `ESRCH` jest zdaniem o systemie. Jeden `Alive`
//! wśród dwóch `Dead` to stan, o którym z liczby nikt się nie dowie, więc `close()` oddaje po
//! jednym dowodzie na rzecz i ten test czyta każdy z nich osobno.
//!
//! # Czego ten plik NIE dowodzi, i mówię to wprost
//!
//! Że okno naprawdę woła tę drogę przy zamykaniu. Obsługa `CloseRequested` mieszka
//! w `src-tauri/src/lib.rs`, poza blokiem OWNS tego zadania, a `Failed to launch` jest na liście
//! `NOT_A_REAL_RED`, więc żywe Tauri nie może być kryterium (dokładnie ta granica stoi
//! w `src/sections/commands-wired.test.ts`). Ten plik dowodzi więc drugiej połowy: że droga,
//! którą zamknięcie ma zawołać, kończy KAŻDĄ rzecz i oddaje dowód po każdej. Ten sam podział ma
//! `commands::chat::Threads::close`, wołane z `lib.rs` jedną linią.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use loadout_lib::commands::processes::{Processes, StartedProcess};
use loadout_lib::engine::drivers::command::StartSpec;
use loadout_lib::engine::supervisor::{self, GroupProof};
use tokio::process::Command;

/// Sufit cierpliwości. Bez niego regresja objawia się jako zawieszenie, bramka zwraca rc 124,
/// a to jest fałszywa czerwień, nie dowód.
const PATIENCE: Duration = Duration::from_secs(20);

/// Rzecz, która rozwidla dziecko i biegnie dalej — kształt każdej apki, jaką człowiek odpali.
const PARENT: &str = r#"#!/bin/sh
# $1 = ścieżka skryptu-wnuka, $2 = znacznik
"$1" "$2-child" &
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
    format!("loadout-t72-{tag}-{}-{nanos}", std::process::id())
}

/// Znacznik wnuka tej rzeczy — procesu, którego żaden nasz `wait()` nie zobaczy nigdy.
///
/// To on przeżył pomiar `total=2 orphaned=2` [T7 §3.1], więc asercje o „naprawdę wstało" pytają
/// o NIEGO, a nie o samą liczbę wierszy: rodzic bez wnuka wygląda w liczbie tak samo, a jest
/// dokładnie tym przypadkiem, którego zabicie jednego procesu nie dotyczy.
fn child_of(marker: &str) -> String {
    format!("{marker}-child")
}

fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// Pyta JĄDRO, czy w grupie `pgid` jest jeszcze ktokolwiek — nie wysyłając sygnału.
///
/// Powód, dla którego to jedyny pomiar liczący się w niezmienniku 6, i powód, dla którego wolno
/// mu stać w pliku testu, stoją w całości w `started_process_is_ours.rs`.
#[allow(unsafe_code)]
fn group_probe(pgid: i32) -> io::Result<()> {
    assert!(
        pgid > 1,
        "pgid {pgid} is not a process group this test may ask about: 0 means our own group and \
         the answer would be about the test runner, not about what was started"
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

/// Dwie rzeczy zamówione z wiersza wejścia, każda z własnym znacznikiem w `argv`.
///
/// Zwraca oba znaczniki obok obu wpisów, bo asercje niżej muszą umieć powiedzieć, KTÓRA z nich
/// przeżyła — „jedna z dwóch została" i „obie zostały" naprawia się inaczej.
struct Pair {
    markers: [String; 2],
    started: [StartedProcess; 2],
}

fn start_two(processes: &Processes, dir: &Path, tag: &str) -> Result<Pair, Box<dyn Error>> {
    let grandchild = write_script(dir, "grandchild.sh", GRANDCHILD)?;
    let parent = write_script(dir, "parent.sh", PARENT)?;

    let mut markers = Vec::new();
    let mut started = Vec::new();
    for which in ["one", "two"] {
        let marker = unique_marker(&format!("{tag}-{which}"));
        let line = format!("{} {} {marker}", parent.display(), grandchild.display());
        started.push(processes.start(&StartSpec {
            command: line,
            cwd: dir.to_path_buf(),
        })?);
        markers.push(marker);
    }

    Ok(Pair {
        markers: [markers[0].clone(), markers[1].clone()],
        started: [started[0].clone(), started[1].clone()],
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_started_commands_never_share_one_group() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let processes = Processes::new();
    let pair = start_two(&processes, dir.path(), "two-groups")?;
    let [one, two] = &pair.started;

    // ── (a) RÓŻNE GRUPY ───────────────────────────────────────────────────────────────────
    assert_ne!(
        one.pgid, two.pgid,
        "both landed in the same group, so one handle is doing the work of two. Then ending the \
         second ends the first as well — or, in the version that actually shipped elsewhere, the \
         second START simply replaces the handle and the first keeps running with nobody left to \
         ask it for proof (invariants 6 and 11). one={one:?} two={two:?}"
    );
    for started in [one, two] {
        assert!(
            started.pgid > 1 && started.pgid != supervisor::own_process_group(),
            "each one needs a real group of its own, not ours: {started:?}"
        );
    }

    // ── (d) OBA NAPRAWDĘ WSTAŁY, KAŻDE W SWOJEJ GRUPIE ────────────────────────────────────
    for (marker, started) in pair.markers.iter().zip([one, two]) {
        let rows = wait_for_rows(marker, 2, Duration::from_secs(5)).await?;
        assert!(
            rows.iter().any(|row| row.args.contains(&child_of(marker))),
            "one of the two never came up, so 'they do not share a group' is a statement about a \
             thing that is not there. ps saw {rows:?}"
        );
        assert!(
            rows.iter().all(|row| row.pgid == started.pgid),
            "everything carrying its marker has to sit in ITS group (pgid {}), or a group-wide \
             stop will never reach it. ps saw {rows:?}",
            started.pgid
        );
    }

    // A rejestr zna oba naraz — nie ostatnie, które przyszło.
    let known: Vec<i32> = processes.list().iter().map(|one| one.pgid).collect();
    assert!(
        known.contains(&one.pgid) && known.contains(&two.pgid),
        "the list has to hold BOTH while both run; it holds {known:?}. A list with one entry is \
         a screen with one tile, and the other thing keeps running where nobody can see it"
    );

    // Sprzątanie jest częścią tego testu, nie uprzejmością.
    let _ = tokio::time::timeout(PATIENCE, processes.close()).await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_the_window_proves_both_gone_and_empties_the_list() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let processes = Processes::new();
    let pair = start_two(&processes, dir.path(), "closing")?;

    // ── (d) KONTROLA: OBA NAPRAWDĘ ŻYJĄ, ZANIM COKOLWIEK JE ZAMKNIE ───────────────────────
    // Bez tego cała reszta przechodzi na PUSTYM ZBIORZE: „nie znaleziono procesów ze znacznikiem"
    // jest prawdą także wtedy, gdy żaden nigdy nie wystartował.
    for (marker, started) in pair.markers.iter().zip(&pair.started) {
        let rows = wait_for_rows(marker, 2, Duration::from_secs(5)).await?;
        assert!(
            rows.iter().any(|row| row.args.contains(&child_of(marker))),
            "this case proves two things dead, so two things have to be alive first — including \
             the child, which no wait() of ours will ever see. ps saw {rows:?}"
        );
        group_probe(started.pgid).map_err(|why| {
            format!(
                "kill(-{}, 0) says that group is not there BEFORE the window closed: {why}",
                started.pgid
            )
        })?;
    }

    // ── (b) ZAMKNIĘCIE KOŃCZY OBA, KAŻDE Z DOWODEM ────────────────────────────────────────
    let proofs = tokio::time::timeout(PATIENCE, processes.close())
        .await
        .map_err(|_| format!("closing did not come back within {PATIENCE:?}"))?;

    assert_eq!(
        proofs.len(),
        2,
        "one proof per thing, because the balance is only complete when every one of them is \
         visible: a single Alive among Deads is exactly the state nobody learns about from the \
         number 'closed two'. It gave {proofs:?}"
    );
    assert!(
        proofs
            .iter()
            .all(|proof| matches!(proof, GroupProof::Dead { .. })),
        "and each proof has to say the group is GONE, not that a signal went out. Ok(()) after a \
         signal reads as 'dead' to the caller while the thing keeps burning the machine \
         (invariant 6). It gave {proofs:?}"
    );

    for (marker, started) in pair.markers.iter().zip(&pair.started) {
        let asked = group_probe(started.pgid);
        let errno = asked.err().and_then(|error| error.raw_os_error());
        assert_eq!(
            errno,
            Some(libc::ESRCH),
            "kill(-{}, 0) still finds somebody in that group after closing said it is gone. This \
             is the measurement that returned total=2 orphaned=2 in T7 §3.1 while the child's own \
             exit status said 'killed'",
            started.pgid
        );

        let after = ps_scan(marker).await?;
        let orphaned: Vec<&PsRow> = after.iter().filter(|row| row.ppid == 1).collect();
        assert!(
            orphaned.is_empty(),
            "total={} orphaned={} — things carrying our marker were reparented to PID 1 and are \
             still running after the window went away. That is the whole reason this task exists: \
             recovery will not clean them up either, because they have no row in the run index. \
             {orphaned:?}",
            after.len(),
            orphaned.len()
        );
        assert!(
            after.is_empty(),
            "ps still finds something carrying the marker after closing reported it gone: \
             {after:?}"
        );
    }

    // ── (c) I NIC PO NICH NIE ZOSTAJE W REJESTRZE ─────────────────────────────────────────
    assert!(
        processes.list().is_empty(),
        "the list still holds {:?} after everything in it was proven gone. A tile lives exactly \
         as long as the thing behind it (invariant 17); a leftover entry is 'Running' over \
         something that went down two minutes ago",
        processes.list()
    );
    Ok(())
}
