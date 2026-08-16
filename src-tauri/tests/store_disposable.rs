//! AC-4 dla T-06: skasuj bazę, odbuduj z plików, dostań te same wiersze.
//!
//! Niezmiennik 4, i to jest jedyne kryterium, które trzyma ten podsystem przy życiu. `loadout.db`
//! jest indeksem: wolno go skasować i nic się nie stanie (`docs/ARCHITECTURE.md` §2 pyt. 2).
//! Cichy tryb porażki jest dokładnie taki: ktoś zapisuje jedno pole, którego nie ma w żadnym
//! pliku — koszt kroku, podsumowanie dla szyny — i przez trzy tygodnie wszystko działa, bo nikt
//! bazy nie kasuje. W dniu, w którym ktoś ją skasuje, ta wiedza znika.
//!
//! **Słaba wersja tego kryterium to porównanie `SELECT count(*)` po każdej tabeli albo
//! porównanie samej tabeli `runs`.** Przechodzi, kiedy odbudowa gubi `cost_usd` i `summary` —
//! czyli dokładnie te dwa pola, które nikt nie pomyślał zapisać do `run.json`. Rozróżniają je
//! dwie rzeczy naraz, i obie są potrzebne:
//!
//! 1. **Lista kolumn wyliczana z `PRAGMA table_info`**, nie wpisana w ten plik. Dzięki temu
//!    kolumna dodana jutro wchodzi do porównania sama i nie da się jej po cichu wyjąć spod
//!    niezmiennika 4.
//! 2. **Asercje na konkretnych wartościach z fikstury** — `steps.cost_usd`, `steps.summary`,
//!    `steps.agent_session_id`, `runs.workflow_snapshot`. Bez nich obydwa zrzuty mogą być
//!    zgodnie puste w tych kolumnach i porównanie zrzutów tego nie zobaczy: równość dwóch
//!    `NULL`i jest równością.
//!
//! `run.json` w tym pliku jest napisany ręcznie, a nie wyprodukowany naszym serializatorem,
//! i to jest cała jego wartość. Fikstura zbudowana naszym kodem definiuje kształt, zamiast go
//! sprawdzać — zmiana kształtu przechodziłaby po obu stronach naraz [04 §6.4].
//!
//! **Konsekwencja, którą trzeba nazwać wprost:** wymaganie „oba zrzuty są równe" zmusza każdą
//! kolumnę do bycia **funkcją plików**, także `events.ts` i `artifacts.id`. Odbudowa, która
//! stempluje którąkolwiek z nich czasem teraźniejszym albo świeżym uuid-em, jest tu czerwona
//! i ma być — bo to znaczy, że po skasowaniu bazy dostaje się co innego, niż się miało.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use rusqlite::types::Value as SqlValue;
use serde_json::Value as Json;

use loadout_lib::store::Store;

/// Bieg z fikstury.
const RUN_ID: &str = "01996500-0000-7000-8000-000000000001";
/// Pierwszy krok.
const STEP_A: &str = "01996500-0000-7000-8000-00000000000a";
/// Drugi krok.
const STEP_B: &str = "01996500-0000-7000-8000-00000000000b";

/// Ile linii niosą razem oba pliki `logs/agent-<id>.jsonl`. Górna granica liczby zdarzeń:
/// żadna kuracja nie robi z jednej linii dwóch.
const LOG_LINES: i64 = 14;

/// Dolna granica liczby zdarzeń.
///
/// Nie jest to liczba dokładna **z rozmysłem**: dokładna kuracja zdarzenie→linia jest kontraktem
/// T-05 (`system/init` nie daje nic, sąsiednie odczyty sklejają się w oknie 2 s), a to kryterium
/// nie jest o niej. Jest o tym, że zdarzenia **przeżywają** skasowanie bazy i zachowują
/// kolejność. Zero zdarzeń przewraca ten warunek, a o to tu chodzi.
const LOG_EVENTS_AT_LEAST: i64 = 8;

/// `workflow_snapshot` — kopia grafu **jak biegł**. Bez niej stary bieg po edycji workflow po
/// cichu zaczyna opowiadać o sobie coś innego [T7 §5.4].
const SNAPSHOT: &str = r#"{
  "nodes": [
    { "key": "research", "agent": "claude", "model": "opus" },
    { "key": "fix", "agent": "claude", "model": "sonnet" }
  ],
  "edges": [{ "from": "research", "to": "fix" }]
}"#;

/// `run.json` — bieg i jego dwa kroki. Pisane ręcznie, bo to jest kontrakt na dysku.
const RUN_JSON: &str = r#"{
  "id": "01996500-0000-7000-8000-000000000001",
  "workflow_id": "ship-a-feature",
  "title": "Fix the CSV parser",
  "status": "succeeded",
  "concurrency": 3,
  "created_at": 1755300000000,
  "started_at": 1755300001000,
  "ended_at": 1755300042000,
  "error": null,
  "workflow_snapshot": {
    "nodes": [
      { "key": "research", "agent": "claude", "model": "opus" },
      { "key": "fix", "agent": "claude", "model": "sonnet" }
    ],
    "edges": [{ "from": "research", "to": "fix" }]
  },
  "steps": [
    {
      "id": "01996500-0000-7000-8000-00000000000a",
      "node_key": "research",
      "name": "Read the parser",
      "agent": "claude",
      "depends_on": [],
      "status": "succeeded",
      "attempt": 0,
      "agent_session_id": "5f6d1c22-0000-4000-8000-00000000000a",
      "pid": 44101,
      "pgid": 44101,
      "exit_code": 0,
      "started_at": 1755300001000,
      "ended_at": 1755300019000,
      "cost_usd": 0.0123,
      "summary": "Read six files and found the bug",
      "error": null
    },
    {
      "id": "01996500-0000-7000-8000-00000000000b",
      "node_key": "fix",
      "name": "Fix the parser",
      "agent": "claude",
      "depends_on": ["research"],
      "status": "succeeded",
      "attempt": 1,
      "agent_session_id": "5f6d1c22-0000-4000-8000-00000000000b",
      "pid": 44118,
      "pgid": 44118,
      "exit_code": 0,
      "started_at": 1755300020000,
      "ended_at": 1755300042000,
      "cost_usd": 0.0456,
      "summary": "Changed one line and the checks went green",
      "error": null
    }
  ]
}
"#;

/// Surowy strumień pierwszego agenta: osiem linii, nietknięte.
const LOG_A: &str = r#"{"type":"system","subtype":"init","session_id":"5f6d1c22-0000-4000-8000-00000000000a","tools":["Read","Grep","Edit"]}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Let me look at how the parser splits a line."}]}}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_01","name":"Read","input":{"file_path":"src/csv.rs","description":"Read the splitter"}}]}}
{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_01","content":"pub fn split(line: &str) -> Vec<&str> { line.split(',').collect() }"}]}}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_02","name":"Grep","input":{"pattern":"split\\(","description":"Find every caller"}}]}}
{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_02","content":"src/csv.rs:14\nsrc/import.rs:87"}]}}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"It splits on every comma, including the ones inside quotes."}]}}
{"type":"result","subtype":"success","is_error":false,"num_turns":3,"duration_ms":18000,"total_cost_usd":0.0123}
"#;

/// Surowy strumień drugiego agenta: sześć linii.
const LOG_B: &str = r#"{"type":"system","subtype":"init","session_id":"5f6d1c22-0000-4000-8000-00000000000b","tools":["Read","Edit","Bash"]}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Quoted fields need their own branch."}]}}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_03","name":"Edit","input":{"file_path":"src/csv.rs","description":"Teach the splitter about quotes"}}]}}
{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_03","content":"Applied 1 edit to src/csv.rs"}]}}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_04","name":"Bash","input":{"command":"cargo test","description":"Run the checks"}}]}}
{"type":"result","subtype":"success","is_error":false,"num_turns":4,"duration_ms":22000,"total_cost_usd":0.0456}
"#;

/// Plik przekazania. Front-matter pisze Loadout, nie agent [T6 §10.2].
const HANDOFF: &str = r#"---
from: research
to: fix
title: What the parser does with quoted commas
---

It splits on every comma, including the ones inside quotes.
"#;

/// Krok w postaci, w jakiej go porównujemy: klucz węzła i trzy pola, których nikt nie pomyślał
/// zapisać do pliku.
///
/// Jedna funkcja formatuje obie strony porównania, więc różnica może pochodzić wyłącznie
/// z wartości, nigdy z tego, jak którąś ze stron zapisano.
fn step_line(
    node_key: &str,
    cost: Option<f64>,
    summary: Option<&str>,
    session: Option<&str>,
) -> String {
    format!("{node_key} · cost {cost:?} · summary {summary:?} · session {session:?}")
}

/// Dwa kroki z fikstury, w kolejności `ORDER BY node_key`.
///
/// To są pola, które nikną, kiedy ktoś zapisuje je wyłącznie do bazy w trakcie biegu i nigdy do
/// `run.json`. Porównanie zrzutów samo tego nie widzi: dwa razy `NULL` to też równość.
fn wanted_steps() -> Vec<String> {
    vec![
        step_line(
            "fix",
            Some(0.0456),
            Some("Changed one line and the checks went green"),
            Some("5f6d1c22-0000-4000-8000-00000000000b"),
        ),
        step_line(
            "research",
            Some(0.0123),
            Some("Read six files and found the bug"),
            Some("5f6d1c22-0000-4000-8000-00000000000a"),
        ),
    ]
}

/// Buduje na dysku prawdziwy katalog biegu z `docs/ARCHITECTURE.md` §8.
fn write_run_directory(run_dir: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(run_dir.join("logs"))?;
    fs::create_dir_all(run_dir.join("handoffs"))?;
    fs::write(run_dir.join("run.json"), RUN_JSON)?;
    fs::write(
        run_dir.join("logs").join(format!("agent-{STEP_A}.jsonl")),
        LOG_A,
    )?;
    fs::write(
        run_dir.join("logs").join(format!("agent-{STEP_B}.jsonl")),
        LOG_B,
    )?;
    fs::write(
        run_dir.join("handoffs").join("01__research__findings.md"),
        HANDOFF,
    )?;
    Ok(())
}

/// Kasuje bazę — **razem** z `-wal` i `-shm`.
///
/// Zostawienie tamtych dwóch nie byłoby „skasowaniem bazy": to są pliki tej samej bazy i
/// zostaje w nich każdy zapis, którego nie zdążył przenieść checkpoint.
fn remove_database(db: &Path) -> anyhow::Result<()> {
    for suffix in ["", "-wal", "-shm"] {
        let mut name = db.as_os_str().to_os_string();
        name.push(suffix);
        let path = PathBuf::from(name);
        if path.exists() {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// Nazwy tabel, prosto z bazy.
fn table_names(conn: &Connection) -> anyhow::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' \
         ORDER BY name",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Nazwy kolumn tabeli, z `PRAGMA table_info` — **nigdy** z listy wpisanej w test.
fn column_names(conn: &Connection, table: &str) -> anyhow::Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Wszystkie wiersze wszystkich tabel, ze wszystkimi kolumnami.
///
/// Lista kolumn każdej tabeli jest w zrzucie osobnym wierszem, więc porównanie widzi także
/// tabelę, która zniknęła, i kolumnę, która przestała istnieć — nie tylko różnicę w danych.
/// `ORDER BY` po numerach kolumn, nie po `rowid`: kolejność wstawiania nie ma prawa być częścią
/// odpowiedzi na pytanie „czy to są te same wiersze".
fn dump_everything(conn: &Connection) -> anyhow::Result<Vec<String>> {
    let mut dump = Vec::new();
    for table in table_names(conn)? {
        let columns = column_names(conn, &table)?;
        if columns.is_empty() {
            continue;
        }
        dump.push(format!("{table} :: {}", columns.join(", ")));

        let list = columns
            .iter()
            .map(|column| format!("\"{column}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let order = (1..=columns.len())
            .map(|position| position.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let mut stmt = conn.prepare(&format!("SELECT {list} FROM \"{table}\" ORDER BY {order}"))?;
        let width = stmt.column_count();
        let rows = stmt.query_map([], move |row| {
            let mut cells = Vec::with_capacity(width);
            for index in 0..width {
                cells.push(format!("{:?}", row.get::<_, SqlValue>(index)?));
            }
            Ok(cells.join(" | "))
        })?;
        for row in rows {
            dump.push(format!("{table} == {}", row?));
        }
    }
    Ok(dump)
}

/// Porównuje dwa zrzuty i pada **krótko**.
///
/// `assert_eq!` na dwóch wektorach po kilkaset wierszy wypisuje kilkaset wierszy dwa razy,
/// i raport bramki przestaje dać się przeczytać — a to jest dokładnie ta chwila, w której ktoś
/// ma go przeczytać.
fn assert_same_dump(rebuilt: &[String], indexed: &[String]) {
    let divergence = rebuilt
        .iter()
        .zip(indexed)
        .position(|(after, before)| after != before);
    assert!(
        divergence.is_none(),
        "the rebuilt index is not the index that was there before the file was deleted. They \
         part company at row {divergence:?}: after the rebuild {:?}, before it {:?}. Every \
         column has to be a function of the files on disk — a value stamped with the current \
         time or a fresh uuid during the rebuild lands here",
        divergence.and_then(|at| rebuilt.get(at)),
        divergence.and_then(|at| indexed.get(at)),
    );
    assert_eq!(
        rebuilt.len(),
        indexed.len(),
        "the rebuilt index holds a different number of rows than the one that was deleted. \
         The shorter side is the answer to 'what does deleting loadout.db cost'"
    );
}

/// Kontrola przeciw pustej asercji **i** cztery pola, o które chodzi.
///
/// Wołane po obu indeksowaniach. Bez tego całe kryterium przechodzi na odbudowie, która nie
/// robi nic: dwa puste zrzuty są równe.
fn assert_fixture_landed(conn: &Connection, when: &str) -> anyhow::Result<()> {
    let runs: i64 = conn.query_row("SELECT count(*) FROM runs", [], |row| row.get(0))?;
    assert_eq!(
        runs, 1,
        "{when}: run.json describes one run and the index holds {runs}. Two empty dumps compare \
         equal, so without this line the whole criterion passes on a rebuild that does nothing"
    );

    let steps: i64 = conn.query_row("SELECT count(*) FROM steps", [], |row| row.get(0))?;
    assert_eq!(
        steps, 2,
        "{when}: run.json describes two steps and the index holds {steps}"
    );

    let events: i64 = conn.query_row("SELECT count(*) FROM events", [], |row| row.get(0))?;
    assert!(
        (LOG_EVENTS_AT_LEAST..=LOG_LINES).contains(&events),
        "{when}: the two log files carry {LOG_LINES} lines between them and the index holds \
         {events} events. The exact number is the curation contract of T-05 and not this \
         criterion's business, but zero means the raw logs were never opened — and then the \
         transcript is a thing that only exists while the database does"
    );

    let (lowest, highest): (i64, i64) =
        conn.query_row("SELECT min(seq), max(seq) FROM events", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
    assert_eq!(
        (lowest, highest),
        (1, events),
        "{when}: seq should run 1..{events} with no gaps. seq IS the order of the transcript, \
         so a gap is a line that a reopened run will never show"
    );

    let stepped: i64 = conn.query_row(
        "SELECT count(DISTINCT step_id) FROM events WHERE step_id IS NOT NULL",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(
        stepped, 2,
        "{when}: events name {stepped} distinct steps, not 2. One log file per agent, and the \
         rail opens ONE agent at a time — events that do not know their step cannot be shown \
         to anybody"
    );

    let stray: i64 = conn.query_row(
        "SELECT count(*) FROM events WHERE run_id <> ?1",
        [RUN_ID],
        |row| row.get(0),
    )?;
    assert_eq!(stray, 0, "{when}: {stray} events belong to no known run");

    // ── Cztery pola, których nikt nie pomyślał zapisać do run.json ─────────────────────────
    let mut stmt = conn.prepare(
        "SELECT node_key, cost_usd, summary, agent_session_id FROM steps ORDER BY node_key",
    )?;
    let landed = stmt
        .query_map([], |row| {
            let node_key: String = row.get(0)?;
            let cost: Option<f64> = row.get(1)?;
            let summary: Option<String> = row.get(2)?;
            let session: Option<String> = row.get(3)?;
            Ok(step_line(
                &node_key,
                cost,
                summary.as_deref(),
                session.as_deref(),
            ))
        })?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    assert_eq!(
        landed,
        wanted_steps(),
        "{when}: cost_usd, summary and agent_session_id did not come back off disk with the \
         values run.json carries. These are the three columns that break invariant 4 quietly: \
         written to the database while the run is going and never to a file, they look right \
         for weeks and are gone the first time somebody deletes loadout.db"
    );

    let snapshot: String = conn.query_row(
        "SELECT workflow_snapshot FROM runs WHERE id = ?1",
        [RUN_ID],
        |row| row.get(0),
    )?;
    let stored: Json = serde_json::from_str(&snapshot)?;
    let wanted: Json = serde_json::from_str(SNAPSHOT)?;
    assert_eq!(
        stored, wanted,
        "{when}: workflow_snapshot is not the graph run.json froze. Compared as parsed JSON, not \
         as text, so key order and whitespace are not part of the contract — but the content is: \
         users edit workflows while old runs sit in history, and without the snapshot those runs \
         silently re-describe themselves [T7 §5.4]"
    );

    Ok(())
}

#[tokio::test]
async fn deleting_the_database_and_rebuilding_from_files_gives_the_same_rows() -> anyhow::Result<()>
{
    let dir = tempfile::tempdir()?;
    let run_dir = dir
        .path()
        .join("runs")
        .join("2026-08-16T01-00-00Z__01996500");
    write_run_directory(&run_dir)?;
    let db = dir.path().join("loadout.db");

    // ── Zaindeksuj katalog biegu ───────────────────────────────────────────────────────────
    let store = Store::open(&db)?;
    store.rebuild_from(&run_dir).await?;
    let reader = store.reader()?;
    assert_fixture_landed(&reader, "after the first index")?;
    let indexed = dump_everything(&reader)?;
    drop(reader);
    store.close().await?;

    // ── Skasuj plik bazy ───────────────────────────────────────────────────────────────────
    remove_database(&db)?;
    assert!(
        !db.exists(),
        "the database file is still there, so nothing below is about rebuilding it"
    );

    // ── Odbuduj z tych samych plików ───────────────────────────────────────────────────────
    let store = Store::open(&db)?;
    store.rebuild_from(&run_dir).await?;
    let reader = store.reader()?;
    assert_fixture_landed(&reader, "after the rebuild")?;
    let rebuilt = dump_everything(&reader)?;
    assert_same_dump(&rebuilt, &indexed);
    drop(reader);
    store.close().await?;

    Ok(())
}
