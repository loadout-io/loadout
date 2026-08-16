//! AC-5 dla T-06: jeden pisarz wytrzymuje ośmiu producentów i nikt nie gubi wiersza.
//!
//! `SQLite` dopuszcza jednego pisarza. Zamiast walczyć z `SQLITE_BUSY`, wszystkie zapisy idą
//! kanałem do jednego zadania [T7 §5.3]. Drugie połączenie zapisujące nie jest „czasem
//! wolniejsze" — jest zakleszczeniem [T7 ryzyko 7], i łamie się to cicho: funkcja czytająca
//! dostaje `Connection::open(path)` „bo tak prościej", działa miesiąc w testach jednowątkowych
//! i wykłada się dopiero przy dwóch agentach naraz, czyli w jedynym scenariuszu, dla którego
//! ten produkt istnieje.
//!
//! **Słaba wersja tego kryterium to jeden producent, 4000 wierszy, `count == 4000`.** Przechodzi
//! na implementacji bez kanału i bez zadania pisarza, która po prostu zapisuje z bieżącego
//! wątku — i wykłada się dopiero na drugim agencie. Rozróżniają je dwie rzeczy: **osiem**
//! równoległych producentów oraz porównanie **zbioru treści**, nie licznika. Licznik nie
//! odróżnia „zgubiłem jeden i zdublowałem inny" od poprawnego biegu, a to jest dokładnie ten
//! rodzaj uszkodzenia, którego nikt nie zauważy, dopóki nie zacznie czytać transkryptu.
//!
//! Druga połowa jest o czymś innym i dlatego stoi tutaj, a nie w osobnym pliku: publiczne API
//! nie ma dawać **żadnej** drogi do drugiego połączenia zapisującego. Gdyby dawało, cała reszta
//! tego kryterium mierzyłaby uprzejmość wołających, nie własność systemu.

use std::collections::HashSet;

use rusqlite::{Connection, params};

use loadout_lib::store::{NewEvent, NewRun, Store, StoreError};

/// Bieg, do którego należą wszystkie zdarzenia.
const RUN_ID: &str = "01996500-0000-7000-8000-000000000001";

/// Ilu producentów naraz.
const PRODUCERS: usize = 8;

/// Ile zdarzeń wysyła każdy.
const PER_PRODUCER: usize = 500;

/// Ile ma być na końcu.
const TOTAL: i64 = 4_000;

/// Treść zdarzenia — unikalna w całym biegu, żeby dało się porównać **zbiory**, nie liczniki.
fn body_of(producer: usize, line: usize) -> String {
    format!("producer {producer} line {line}")
}

/// Bieg w kształcie, w jakim wchodzi do bazy.
fn a_run() -> NewRun {
    NewRun {
        id: RUN_ID.to_owned(),
        workflow_id: "ship-a-feature".to_owned(),
        workflow_snapshot: r#"{"nodes":[],"edges":[]}"#.to_owned(),
        title: "Fix the CSV parser".to_owned(),
        status: "running".to_owned(),
        concurrency: 3,
        created_at: 1_755_300_000_000,
        started_at: Some(1_755_300_001_000),
        ended_at: None,
        error: None,
    }
}

/// Zdarzenie transkryptu.
fn an_event(body: String) -> NewEvent {
    NewEvent {
        run_id: RUN_ID.to_owned(),
        step_id: None,
        ts: 1_755_300_002_000,
        kind: "assistant".to_owned(),
        level: "detail".to_owned(),
        body: Some(body),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn eight_producers_write_at_once_and_not_one_row_goes_missing() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("loadout.db");

    let store = Store::open(&db)?;
    store.writer().insert_run(a_run()).await?;

    // ── Ośmiu producentów, po 500 wysłań każdy ─────────────────────────────────────────────
    let mut producers = tokio::task::JoinSet::new();
    for producer in 0..PRODUCERS {
        // Klon uchwytu na producenta. Wszystkie osiem prowadzi do JEDNEGO połączenia — to jest
        // cała teza tego kryterium.
        let writer = store.writer();
        producers.spawn(async move {
            for line in 0..PER_PRODUCER {
                writer
                    .append_events(vec![an_event(body_of(producer, line))])
                    .await?;
            }
            Ok::<(), StoreError>(())
        });
    }

    let mut refusals: Vec<String> = Vec::new();
    while let Some(joined) = producers.join_next().await {
        if let Err(refused) = joined? {
            refusals.push(refused.to_string());
        }
    }

    let locked: Vec<&String> = refusals
        .iter()
        .filter(|refusal| refusal.contains("SQLITE_BUSY") || refusal.contains("database is locked"))
        .collect();
    assert!(
        locked.is_empty(),
        "a send came back with SQLite's lock error. That is the signature of a second writing \
         connection: funnelling every write through one task exists precisely so this error \
         cannot be reached, and it is the error meetnotes shipped to users as a random \
         'Save failed' twice a week. {locked:?}"
    );
    assert!(
        refusals.is_empty(),
        "every one of the {TOTAL} sends has to come back Ok. {} did not: {refusals:?}",
        refusals.len()
    );

    // ── Druga połowa: publiczne API nie daje drogi do drugiego pisarza ─────────────────────
    let read_only = store.reader()?;
    let attempt = read_only.execute(
        "INSERT INTO events (run_id, ts, kind, level, body) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            RUN_ID,
            1_755_300_009_000_i64,
            "assistant",
            "detail",
            "written from a reader"
        ],
    );
    let refused = attempt
        .as_ref()
        .err()
        .map(ToString::to_string)
        .unwrap_or_default();
    assert!(
        attempt.is_err(),
        "Store::reader() handed back a connection that can WRITE. Then invariant 2 is a comment, \
         not a property: the second writing connection exists and the deadlock is one concurrent \
         run away"
    );
    assert!(
        refused.contains("readonly"),
        "the write through Store::reader() failed, but not because the connection is read-only. \
         Any other reason (a missing table, a constraint) would also refuse this INSERT while \
         leaving the connection perfectly able to write to something else: {refused:?}"
    );

    // Dopiero teraz zamykamy kanał i CZEKAMY na pisarza. Bez czekania „zapisane" znaczy tylko
    // „wysłane", a różnicę między nimi widać wyłącznie w takim teście.
    store.close().await?;

    // ── Stan, nie kod powrotu (niezmiennik 19) ─────────────────────────────────────────────
    let conn = Connection::open(&db)?;
    let landed: i64 = conn.query_row("SELECT count(*) FROM events", [], |row| row.get(0))?;
    assert_eq!(
        landed, TOTAL,
        "the eight producers sent {TOTAL} events and events holds {landed}"
    );

    let mut stmt = conn.prepare("SELECT body FROM events")?;
    let bodies = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<HashSet<String>>>()?;

    let expected: HashSet<String> = (0..PRODUCERS)
        .flat_map(|producer| (0..PER_PRODUCER).map(move |line| body_of(producer, line)))
        .collect();

    let missing: Vec<&String> = expected.difference(&bodies).take(5).collect();
    assert!(
        missing.is_empty(),
        "these lines were sent and are not in the transcript: {missing:?}. A count can be right \
         while the contents are wrong — lose one row, write another twice, and the number still \
         reads 4000"
    );
    assert_eq!(
        bodies.len(),
        expected.len(),
        "events holds {landed} rows but only {} distinct bodies. Every send carried a unique \
         line, so a row written twice is a row that was written instead of another one",
        bodies.len()
    );

    Ok(())
}
