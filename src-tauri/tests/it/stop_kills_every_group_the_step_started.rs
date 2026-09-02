//! Z-01c: Stop człowieka i limit czasu kroku mają zabić **wszystko, co krok uruchomił** — także
//! to, co uciekło do własnej grupy procesów.
//!
//! Słaba wersja tego kryterium to `stop() == Dead`. Spełnia ją dzisiejszy kod, i to jest cała
//! wada: eskalacja i dowód są zdaniem o JEDNEJ grupie (`killpg(pgid, TERM)` → łaska →
//! `killpg(pgid, KILL)` → `ESRCH`), a Claude Code uruchamia każdą komendę narzędzia Bash we
//! własnej grupie procesów. Wszystko, co taka powłoka odpali — `cargo test`, serwer
//! deweloperski — leży poza grupą, którą zabijamy i której śmierci dowodzimy.
//!
//! Dowód z produktu: bieg w `../meetnotes` z 2026-09-01, krok Combine, `death_proof: true`
//! w `run.json`, a binarium testowe (PPID 1, inny `pgid`, 262 MB) żyło szesnaście godzin po
//! anulowaniu biegu. Osierocony proces pali limit u dostawcy w tle — niezmiennik 6 nazywa to
//! błędem finansowym, nie higienicznym.
//!
//! Dlatego oba testy mierzą **system operacyjny, nie nasz kod**: skan `ps` po unikalnym
//! znaczniku nie może znaleźć ani jednego wiersza, także żadnego z `ppid == 1`. Wołają
//! produkcyjne [`supervisor::spawn`] i `Supervised::stop`, nie funkcję pomocniczą — droga,
//! której nikt nie woła, niczego nie dowodzi (niezmiennik 29).

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use loadout_lib::engine::supervisor::{self, GroupProof, StdinPlan};
use tokio::process::Command;

/// Okno łaski podane argumentem, nie wzięte ze stałej produkcyjnej: w teście chcemy, żeby
/// niepowodzenie było krótkie, a nie żeby trwało pięć sekund [T7 §3.3].
const GRACE: Duration = Duration::from_secs(2);

/// Ile czekamy na `stop()`, zanim uznamy to za zawieszenie. Musi zmieścić pełną eskalację
/// grupy odkrytej później: jej łaska zaczyna się po naszej.
const PATIENCE: Duration = Duration::from_secs(20);

/// Lider odpala uciekiniera i czeka.
///
/// `set -m` włącza sterowanie zadaniami, więc zadanie w tle dostaje **własną grupę procesów** —
/// dokładnie to, co Claude Code robi z każdą komendą narzędzia Bash. `setsid(1)` na macOS nie
/// istnieje, a to jest ta sama ucieczka bez dodatkowego programu.
///
/// Pętla krótkich snów zamiast jednego `sleep 30`: pojedyncza, ostatnia komenda skryptu bywa
/// przez powłokę exec-optymalizowana i wtedy znacznik znika z `argv` [T7 §8.2].
const LEADER: &str = r#"#!/bin/sh
# $1 = skrypt uciekiniera, $2 = znacznik
set -m
"$1" "$2-escapee" &
set +m
while :; do
  sleep 0.2
done
"#;

/// Uciekinier: nic nie robi poza tym, że **jest widoczny w `ps`** pod swoim znacznikiem.
///
/// Własny sufit ~12 s zamiast `while :`, żeby czerwony przebieg nie zostawił po sobie dokładnie
/// tej sieroty, którą ten plik opisuje.
const ESCAPEE: &str = r#"#!/bin/sh
# $1 = znacznik
i=0
while [ "$i" -lt 60 ]; do
  sleep 0.2
  i=$((i+1))
done
"#;

/// Lider drugiego testu: odpala dziecko w swojej grupie i czeka.
const LATE_LEADER: &str = r#"#!/bin/sh
# $1 = skrypt dziecka, $2 = skrypt wnuka, $3 = znacznik, $4 = plik dowodu, $5 = plik gotowości
"$1" "$2" "$3" "$4" "$5" &
while :; do
  sleep 0.2
done
"#;

/// Dziecko, które **ignoruje pierwszy SIGTERM** i dopiero wtedy odpala wnuka.
///
/// To jest ta grupa, której poprzednia wersja poprawki nie widziała: drugi przegląd drzewa
/// robiła wyłącznie przy żywym liderze, a lider ginie od tej samej piętnastki, która budzi ten
/// trap. Sen 0,05 s, nie 0,2 s — `sh` wykonuje trap dopiero po zakończeniu bieżącej komendy,
/// więc od długości snu zależy, jak późno wnuk w ogóle powstanie.
const STUBBORN_CHILD: &str = r#"#!/bin/sh
# $1 = skrypt wnuka, $2 = znacznik, $3 = plik dowodu, $4 = plik gotowości
GRANDCHILD="$1"
MARKER="$2"
PROOF_FILE="$3"
trap '"$GRANDCHILD" "$MARKER-grandchild" "$PROOF_FILE" &' TERM
: > "$4"
i=0
while [ "$i" -lt 240 ]; do
  sleep 0.05
  i=$((i+1))
done
"#;

/// Wnuk urodzony po pierwszym sygnale. Plik dowodu pisze **wyłącznie handler SIGTERM-a**, więc
/// jego istnienie odróżnia piętnastkę od dziewiątki: grupa odkryta później ma dostać pełną
/// eskalację, a nie od razu KILL-a [T1 §4.6].
///
/// # Dlaczego akurat perl, a nie `#!/bin/sh` z `set -m` (2026-09)
///
/// Bo kolejność „najpierw handler, dopiero potem własna grupa" jest jedyną, w której ten dowód
/// nie jest wyścigiem — a `sh` jej nie umie: nową grupę zakłada tam RODZIC, w chwili `fork()`,
/// więc między narodzinami wnuka a jego `trap` zostaje okno, w którym piętnastka zabija go
/// bezgłośnie. Zmierzone: przy przeglądzie co 250 ms to okno ma ~200 ms i zamknęło się raz na
/// pięć przebiegów pod obciążeniem, czyli test kłamałby o produkcji co piąty raz.
///
/// Dopóki wnuk siedzi w grupie lidera, nikt go już nie sygnalizuje — ta grupa dostała swoją
/// jedyną piętnastkę, zanim on w ogóle powstał. `setpgrp` czyni go widocznym dla przeglądu
/// drzewa, a wtedy handler stoi od dawna. `setsid(1)` na macOS nie istnieje; perl jest częścią
/// systemu bazowego, a test i tak biegnie wyłącznie na macOS (`platform_agent_cli_dirs`).
const LATE_GRANDCHILD: &str = r#"#!/usr/bin/perl
# $ARGV[0] = znacznik (ma zostać w argv, po nim skanuje ps), $ARGV[1] = plik dowodu
$SIG{TERM} = sub {
    open my $out, '>', $ARGV[1] or exit 1;
    print {$out} "bye\n";
    close $out;
    exit 0;
};
setpgrp(0, 0);
my $left = 240;
while ($left-- > 0) { select undef, undef, undef, 0.05 }
"#;

/// Program, którego wnuk potrzebuje, żeby założyć własną grupę. Sprawdzany wprost, bo bez niego
/// wnuk ginie na `exec` w tle i test opowiadałby o zieleni, której nie zmierzył.
const NEW_GROUP_LAUNCHER: &str = "/usr/bin/perl";

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
    format!("loadout-z01c-{tag}-{}-{nanos}", std::process::id())
}

/// Zapisuje wykonywalny skrypt `#!/bin/sh` i zwraca jego ścieżkę [T7 §8.2].
fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// Wiersze `ps` zawierające `marker`. Pomiar spoza naszego drzewa procesów — jedyny, który widzi
/// grupę, do której `killpg(pgid, …)` nigdy nie dotarł.
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
        // `parent`/`group` zamiast `ppid`/`pgid`: dwie nazwy różniące się jedną literą w środku
        // to dokładnie ten rodzaj pary, w której podmiana jednej na drugą przechodzi przez
        // recenzję niezauważona (wzór z `supervisor_group_death.rs`).
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
async fn stop_is_not_dead_until_a_child_in_its_own_group_is_gone() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let marker = unique_marker("escapee");
    let escapee = write_script(dir.path(), "escapee.sh", ESCAPEE)?;
    let leader = write_script(dir.path(), "leader.sh", LEADER)?;

    let mut command = Command::new(&leader);
    command.arg(&escapee).arg(&marker);

    let mut handle = supervisor::spawn(command, StdinPlan::Null)?;
    let group = handle.group();

    // ── Uciekinier naprawdę uciekł, zanim cokolwiek zabijemy ──────────────────────────────
    // Bez tego cała reszta przechodzi na pustym zbiorze: „nie znaleziono nic ze znacznikiem"
    // jest prawdą także wtedy, gdy nic nigdy nie wystartowało — i tak samo wtedy, gdy `set -m`
    // na tej maszynie nie założyło nowej grupy, czyli gdy test mierzy coś innego, niż opisuje.
    let runaway = format!("{marker}-escapee");
    let before = wait_for_rows(&marker, 2, Duration::from_secs(5)).await?;
    let outside: Vec<&PsRow> = before
        .iter()
        .filter(|row| row.args.contains(&runaway) && row.pgid != group.pgid)
        .collect();
    assert!(
        !outside.is_empty(),
        "the runaway is not sitting outside the group we were handed (pgid {}), so this run has \
         nothing to prove dead later; ps saw {before:?}",
        group.pgid
    );

    // ── Zatrzymanie ───────────────────────────────────────────────────────────────────────
    // Własny limit czasu wokół oczekiwania: bez niego regresja objawi się jako zawieszenie,
    // bramka zwróci rc 124, a to jest fałszywa czerwień, nie dowód.
    let proof = tokio::time::timeout(PATIENCE, handle.stop(GRACE))
        .await
        .map_err(|_| "stop() did not come back within the patience of this test")?;

    let after = ps_scan(&marker).await?;
    let orphaned: Vec<&PsRow> = after.iter().filter(|row| row.ppid == 1).collect();
    assert!(
        orphaned.is_empty(),
        "total={} orphaned={} — the step started these in their own group, stop() answered \
         {proof:?}, and they were reparented to PID 1 and are still running. That is the \
         sixteen-hour leak from ../meetnotes verbatim, and it burns quota invisibly: {orphaned:?}",
        after.len(),
        orphaned.len()
    );
    assert!(
        after.is_empty(),
        "ps still finds what the step started after stop() answered {proof:?}: {after:?}"
    );
    assert!(
        matches!(proof, GroupProof::Dead { .. }),
        "with nothing left alive, stop() has to hand back the proof, not a guess; it returned \
         {proof:?}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_group_born_after_the_first_signal_still_gets_term_then_kill()
-> Result<(), Box<dyn Error>> {
    assert!(
        Path::new(NEW_GROUP_LAUNCHER).exists(),
        "{NEW_GROUP_LAUNCHER} is missing, so the grandchild below could never move into a group \
         of its own and this test would measure nothing"
    );

    let dir = tempfile::tempdir()?;
    let marker = unique_marker("late-group");
    let handled = dir.path().join("grandchild-handled-sigterm");
    let ready = dir.path().join("trap-installed");
    let grandchild = write_script(dir.path(), "grandchild.pl", LATE_GRANDCHILD)?;
    let child = write_script(dir.path(), "child.sh", STUBBORN_CHILD)?;
    let leader = write_script(dir.path(), "leader.sh", LATE_LEADER)?;

    // Kolejność argumentów lidera to (dziecko, wnuk, znacznik, dowód, gotowość).
    let mut command = Command::new(&leader);
    command
        .arg(&child)
        .arg(&grandchild)
        .arg(&marker)
        .arg(&handled)
        .arg(&ready);

    let mut handle = supervisor::spawn(command, StdinPlan::Null)?;
    let group = handle.group();

    assert!(
        wait_for_ready(&ready, Duration::from_secs(5)).await,
        "the child never reported that its TERM trap was installed, so the first signal below \
         would kill it outright and no later group would ever be born"
    );

    // ── Przed sygnałem nie ma jeszcze czego zgubić ────────────────────────────────────────
    // Wnuk rodzi się dopiero z trapu, więc wszystko ze znacznikiem siedzi teraz w grupie, którą
    // dostaliśmy. To jest ta różnica, na której padła poprzednia wersja poprawki.
    let before = wait_for_rows(&marker, 2, Duration::from_secs(5)).await?;
    assert!(
        before.iter().all(|row| row.pgid == group.pgid),
        "before the first signal everything carrying the marker has to sit in the group we were \
         handed (pgid {}); ps saw {before:?}",
        group.pgid
    );

    let proof = tokio::time::timeout(PATIENCE, handle.stop(GRACE))
        .await
        .map_err(|_| "stop() did not come back within the patience of this test")?;

    let after = ps_scan(&marker).await?;
    let orphaned: Vec<&PsRow> = after.iter().filter(|row| row.ppid == 1).collect();
    assert!(
        orphaned.is_empty(),
        "a group born inside the grace window outlived stop(), which answered {proof:?}; \
         looking again only while the leader is alive misses exactly this one, because the \
         leader dies from the same signal that wakes the trap: {orphaned:?}"
    );
    assert!(
        after.is_empty(),
        "ps still finds what the step started after stop() answered {proof:?}: {after:?}"
    );

    // Istnienie pliku jest tu całym dowodem: pisze go wyłącznie handler SIGTERM-a. Wnuk zabity
    // dziewiątką nie ma jak go zostawić, a „grupa zniknęła" spełnia też prowadzenie KILL-em,
    // które kosztuje transkrypt i zamek sesji [T1 §4.6].
    let Ok(said) = fs::read_to_string(&handled) else {
        return Err(
            "the grandchild born after the first signal never ran its TERM handler, so it \
                    took signal nine; a group found later has to get the whole escalation too"
                .into(),
        );
    };
    assert!(
        !said.trim().is_empty(),
        "the marker file exists but is empty, so the handler was interrupted rather than run"
    );
    assert!(
        matches!(proof, GroupProof::Dead { .. }),
        "with nothing left alive, stop() has to hand back the proof, not a guess; it returned \
         {proof:?}"
    );

    Ok(())
}
