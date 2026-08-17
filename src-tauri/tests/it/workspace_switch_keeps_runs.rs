//! AC-1 dla T-24: bieg w karcie w tle działa dalej i nie gubi ani jednej linii.
//!
//! To jest cicha porażka, dla której to zadanie istnieje. Implementacja, w której przełączenie
//! karty odpina odbiornik strumienia, przechodzi **każdy** test pisany na karcie aktywnej —
//! bo każdy taki test patrzy dokładnie tam, gdzie odbiornik akurat wisi. Widać ją dopiero
//! z drugiej strony: wracasz do folderu po dwóch minutach i zastajesz „Thinking…" sprzed dwóch
//! minut albo historię z dziurą, bo linie poleciały do kanału, którego nikt nie czytał.
//!
//! **Słaba asercja: `lines.len() == 200`.** Przechodzi na implementacji, która gubi linie
//! 51..150 i dokłada sto duplikatów na końcu — a dokładnie tak wygląda odpięcie i ponowne
//! podpięcie odbiornika bez kursora: po powrocie do karty ktoś czyta kanał od nowa i to, co
//! przyszło w międzyczasie, wchodzi drugi raz zamiast pierwszy. Długość się zgadza, transkrypt
//! jest zniszczony. Rozróżnia **porównanie sekwencji numerów** z `1..=200`: jedno zdanie, które
//! widzi naraz luki, duplikaty i kolejność.
//!
//! Drugi przypadek pyta o tę samą rzecz od najostrzejszej strony: karta, która **ani przez
//! chwilę** nie jest na wierzchu. Jeżeli pompa wisi na widoku, tutaj nie doniesie ani jednej
//! linii — a to jest zwykły układ pracy, w którym uruchamiasz coś w jednym folderze i od razu
//! przechodzisz do drugiego.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use loadout_lib::engine::limits::Limiter;
use loadout_lib::store::{NewRun, Store};
use loadout_lib::workspace::{Registry, RunLine, RunOutcome, WorkspaceId};

/// Ile linii emituje każdy bieg.
const LINES: u32 = 200;

/// Po której linii pierwszego biegu odchodzimy do drugiej karty.
const SWITCH_AWAY_AT: u32 = 50;

/// Po której linii pierwszego biegu wracamy. Sto linii w tle to okno, w którym implementacja
/// z odpiętym odbiornikiem gubi dokładnie tyle, ile w nim przeleciało.
const SWITCH_BACK_AT: u32 = 150;

/// Bieg w karcie `meetnotes`.
const RUN_FIRST: &str = "01996500-0000-7000-8000-00000000a001";

/// Bieg w karcie `spreadsheet`.
const RUN_SECOND: &str = "01996500-0000-7000-8000-00000000a002";

/// Ile miejsc ma pula. Dwa biegi po jednym kroku mieszczą się w niej naraz, bo to kryterium
/// jest o strumieniu, nie o limicie — od limitu jest AC-2.
const AT_ONCE: usize = 2;

/// Zakłada folder pod `root` i oddaje jego ścieżkę.
fn folder(root: &Path, name: &str) -> anyhow::Result<PathBuf> {
    let path = root.join(name);
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// Wiersz `runs`, bez którego zdarzenia nie mają do czego należeć: `events.run_id` wskazuje
/// na `runs`, a klucze obce są włączone na każdym połączeniu. Bieg zakłada ten, kto go zaczyna
/// (T-07) — karta dostaje już tylko jego identyfikator.
fn a_run(id: &str) -> NewRun {
    NewRun {
        id: id.to_owned(),
        workflow_id: "ship-a-feature".to_owned(),
        workflow_snapshot: r#"{"nodes":[],"edges":[]}"#.to_owned(),
        title: "Fix the CSV parser".to_owned(),
        status: "running".to_owned(),
        concurrency: 3,
        created_at: 1_755_300_000_000,
        started_at: Some(1_755_300_001_000),
        ended_at: None,
        boot_id: None,
        error: None,
    }
}

/// Linia numer `n`. Numer jest w treści, bo to on, a nie długość listy, jest tu dowodem.
fn numbered(n: u32) -> RunLine {
    RunLine {
        ts: 1_755_300_002_000 + i64::from(n),
        kind: "assistant".to_owned(),
        level: "detail".to_owned(),
        body: format!("line {n}"),
    }
}

/// Zakłada bieg w magazynie tej karty.
async fn seed_run(registry: &Registry, id: &WorkspaceId, run_id: &str) -> anyhow::Result<()> {
    let store = registry
        .store(id)
        .ok_or_else(|| anyhow!("the registry opened {id} and then had no store for it"))?;
    store
        .writer()
        .insert_run(a_run(run_id))
        .await
        .with_context(|| format!("seeding run {run_id} in {id}"))?;
    Ok(())
}

/// Emituje [`LINES`] ponumerowanych linii do karty `id`, przełączając widok w zadanych punktach.
///
/// Przełączenia dzieją się **w środku strumienia**, nie przed nim i nie po nim. Test, który
/// przełącza kartę przed pierwszą linią, mierzy wyłącznie to, że bieg w ogóle wystartował.
async fn stream_run(
    registry: &Registry,
    id: &WorkspaceId,
    run_id: &str,
    switches: &[(u32, &WorkspaceId)],
) -> anyhow::Result<RunOutcome> {
    let mut sink = registry
        .attach_run(id, run_id)
        .with_context(|| format!("attaching run {run_id} to the tab for {id}"))?;

    for n in 1..=LINES {
        sink.send(numbered(n)).await?;
        for (after, show) in switches {
            if n == *after {
                registry.set_active(show).with_context(|| {
                    format!("switching the visible tab to {show} after line {n}")
                })?;
            }
        }
    }

    let ended = sink.finish(RunOutcome::Succeeded).await?;
    Ok(ended)
}

/// Numery linii tego biegu, w kolejności, w jakiej stoją w magazynie.
///
/// `ORDER BY seq`, bo `seq` nadaje `SQLite` i jest globalnie monotoniczne — to jest kolejność
/// zapisu, a nie kolejność, w jakiej ten test o nie poprosił.
fn numbers_in(store: &Store, run_id: &str) -> anyhow::Result<Vec<u32>> {
    let conn = store.reader()?;
    let mut stmt = conn.prepare("SELECT body FROM events WHERE run_id = ?1 ORDER BY seq")?;
    let bodies = stmt
        .query_map([run_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;

    bodies
        .iter()
        .map(|body| {
            body.strip_prefix("line ")
                .and_then(|n| n.parse::<u32>().ok())
                .ok_or_else(|| {
                    anyhow!("the transcript holds a line this test never sent: {body:?}")
                })
        })
        .collect()
}

/// Krótkie zdanie o tym, CO poszło nie tak z sekwencją — dwieście liczb obok drugich dwustu
/// jest nieczytelne, a pytanie brzmi „czego brakuje i co weszło dwa razy".
fn diagnose(numbers: &[u32]) -> String {
    let seen: BTreeSet<u32> = numbers.iter().copied().collect();
    let missing: Vec<u32> = (1..=LINES).filter(|n| !seen.contains(n)).take(8).collect();

    let mut times = BTreeMap::new();
    for n in numbers {
        *times.entry(*n).or_insert(0_usize) += 1;
    }
    let doubled: Vec<u32> = times
        .iter()
        .filter(|&(_, &count)| count > 1)
        .map(|(&n, _)| n)
        .take(8)
        .collect();

    format!(
        "{} lines landed, {} of them distinct; missing (first few) {missing:?}; written more \
         than once (first few) {doubled:?}",
        numbers.len(),
        seen.len()
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn switching_tabs_mid_run_loses_nothing() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let registry = Registry::new(Limiter::new(AT_ONCE));

    let meetnotes = registry.open(&folder(root.path(), "meetnotes")?)?;
    let spreadsheet = registry.open(&folder(root.path(), "spreadsheet")?)?;
    registry.set_active(&meetnotes)?;

    seed_run(&registry, &meetnotes, RUN_FIRST).await?;
    seed_run(&registry, &spreadsheet, RUN_SECOND).await?;

    // Osobne wiązanie, nie tymczasowa tablica w wywołaniu: `try_join!` trzyma oba future'y
    // dłużej niż zdanie, w którym powstały, więc tymczasowa ginęłaby w trakcie biegu.
    let switches = [(SWITCH_AWAY_AT, &spreadsheet), (SWITCH_BACK_AT, &meetnotes)];
    let (ended_first, ended_second) = tokio::try_join!(
        stream_run(&registry, &meetnotes, RUN_FIRST, &switches),
        stream_run(&registry, &spreadsheet, RUN_SECOND, &[]),
    )?;

    assert_eq!(
        registry.active(),
        Some(meetnotes.clone()),
        "the run put the first tab back on top after its line {SWITCH_BACK_AT}, so that is where \
         the view has to stand when both runs are done"
    );

    let expected: Vec<u32> = (1..=LINES).collect();

    let first_store = registry
        .store(&meetnotes)
        .ok_or_else(|| anyhow!("no store for {meetnotes} after its run finished"))?;
    let landed = numbers_in(&first_store, RUN_FIRST)?;
    assert!(
        landed == expected,
        "the run in the first tab has to hold lines 1..={LINES} in order, with no gaps and no \
         duplicates. {}. Comparing only the count would pass right here on the implementation \
         this criterion exists to reject: drop lines {}..{} while the tab is in the background, \
         then replay the channel from its start when the tab comes back, and the total still \
         reads {LINES}",
        diagnose(&landed),
        SWITCH_AWAY_AT + 1,
        SWITCH_BACK_AT
    );
    assert_eq!(
        ended_first,
        RunOutcome::Succeeded,
        "and the run has to end as succeeded. A tab that could not carry every line it accepted \
         has no business calling that run finished"
    );

    // Druga karta jest kontrolą na pompę piszącą do magazynu KARTY WIDOCZNEJ zamiast do
    // magazynu swojego folderu — implementacja, która to robi, przechodzi wszystko wyżej
    // i miesza dwa transkrypty w jednym pliku.
    let second_store = registry
        .store(&spreadsheet)
        .ok_or_else(|| anyhow!("no store for {spreadsheet} after its run finished"))?;
    let landed = numbers_in(&second_store, RUN_SECOND)?;
    assert!(
        landed == expected,
        "the run in the second tab has to hold its own lines 1..={LINES} in its own folder's \
         store. {}",
        diagnose(&landed)
    );
    assert_eq!(
        ended_second,
        RunOutcome::Succeeded,
        "and it ends as succeeded too — it was in the background for the first {SWITCH_AWAY_AT} \
         lines and on top for the next hundred, and neither is a thing that happens to a run"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_run_in_a_tab_that_is_never_on_top_still_lands_every_line() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let registry = Registry::new(Limiter::new(AT_ONCE));

    let meetnotes = registry.open(&folder(root.path(), "meetnotes")?)?;
    let spreadsheet = registry.open(&folder(root.path(), "spreadsheet")?)?;
    // Widok stoi gdzie indziej przez CAŁY bieg. To jest zwykły układ pracy — uruchamiasz coś
    // w jednym folderze i od razu przechodzisz do drugiego — a nie przypadek brzegowy.
    registry.set_active(&spreadsheet)?;

    seed_run(&registry, &meetnotes, RUN_FIRST).await?;
    let ended = stream_run(&registry, &meetnotes, RUN_FIRST, &[]).await?;

    assert_eq!(
        registry.active(),
        Some(spreadsheet),
        "nothing in this case switches the view, so it has to stand where it was put"
    );

    let store = registry
        .store(&meetnotes)
        .ok_or_else(|| anyhow!("no store for {meetnotes} after its run finished"))?;
    let landed = numbers_in(&store, RUN_FIRST)?;
    assert!(
        landed == (1..=LINES).collect::<Vec<u32>>(),
        "a run whose tab was never on top has to land all {LINES} lines just the same. {}. \
         A pump that hangs off the visible tab writes nothing at all here, which reads on screen \
         as a run that never started",
        diagnose(&landed)
    );
    assert_eq!(
        ended,
        RunOutcome::Succeeded,
        "and it ends as succeeded, because being in the background is not something that \
         happens to a run"
    );
    Ok(())
}
