//! AC-1 dla T-140: nowy indeks nie tworzy cienia notatek, a stary toleruje bez przepisywania.
//!
//! Test nie czyta DDL produkcji jako tekstu. Otwiera prawdziwy [`Store`], sadzi historyczny
//! schemat z wierszami, a po skasowaniu indeksu uruchamia produkcyjną odbudowę z `run.json`.

use std::fs;
use std::path::Path;

use rusqlite::types::Value;
use rusqlite::{Connection, params};

use loadout_lib::store::{Store, apply_pragmas, migrate};

const RUN_ID: &str = "019b0000-0000-7000-8000-000000000140";
const STEP_ID: &str = "019b0000-0000-7000-8000-000000000141";
const NOTE_BODY: &str = "Files are the only truth for notes.";
const LOG_LINE: &str = r#"{"type":"assistant","message":"The index can be rebuilt."}"#;

const LIVE_TABLES: [&str; 4] = ["artifacts", "events", "runs", "steps"];
const LEGACY_TABLES: [&str; 5] = ["artifacts", "events", "memory", "runs", "steps"];

const RUN_JSON: &str = r#"{
  "id": "019b0000-0000-7000-8000-000000000140",
  "workflow_id": "prove-the-index-is-disposable",
  "workflow_snapshot": {"nodes": [], "edges": []},
  "title": "Keep notes in files",
  "status": "succeeded",
  "concurrency": 2,
  "created_at": 1787781600000,
  "started_at": 1787781601000,
  "ended_at": 1787781603000,
  "error": null,
  "boot_id": "fixture-boot",
  "steps": [
    {
      "id": "019b0000-0000-7000-8000-000000000141",
      "node_key": "inspect",
      "name": "Inspect the files",
      "agent": "codex",
      "depends_on": [],
      "status": "succeeded",
      "attempt": 1,
      "agent_session_id": "session-t140",
      "pid": 4140,
      "pgid": 4140,
      "exit_code": 0,
      "started_at": 1787781601000,
      "ended_at": 1787781603000,
      "cost_usd": 0.014,
      "summary": "The run is recoverable from files",
      "error": null
    }
  ]
}
"#;

/// Historyczny kształt z T-06. Stoi w fixture jawnie, bo po naprawie produkcyjny `migrate`
/// nie będzie już umiał stworzyć tej tabeli na nowej bazie.
const LEGACY_MEMORY_DDL: &str = "
CREATE TABLE IF NOT EXISTS memory (
  id              TEXT    NOT NULL PRIMARY KEY,
  scope           TEXT    NOT NULL,
  key             TEXT    NOT NULL,
  path            TEXT    NOT NULL,
  title           TEXT    NOT NULL,
  body            TEXT    NOT NULL,
  written_by_run  TEXT             REFERENCES runs(id)  ON DELETE SET NULL,
  written_by_step TEXT             REFERENCES steps(id) ON DELETE SET NULL,
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL,
  UNIQUE (scope, key)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_memory_scope ON memory(scope, updated_at DESC);
";

fn write_run_files(run_dir: &Path) -> anyhow::Result<()> {
    let logs = run_dir.join("logs");
    fs::create_dir_all(&logs)?;
    fs::write(run_dir.join("run.json"), RUN_JSON)?;
    fs::write(logs.join(format!("agent-{STEP_ID}.jsonl")), LOG_LINE)?;
    Ok(())
}

fn user_tables(conn: &Connection) -> anyhow::Result<Vec<String>> {
    let mut statement = conn.prepare(
        "SELECT name FROM sqlite_master \
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn object_exists(conn: &Connection, kind: &str, name: &str) -> anyhow::Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = ?1 AND name = ?2",
        params![kind, name],
        |row| row.get(0),
    )?;
    Ok(count == 1)
}

/// Wszystkie kolumny i wszystkie wiersze wskazanych tabel. Lista kolumn pochodzi z bazy, więc
/// otwarcie, które dopisałoby albo przepisało pole, nie może schować tego przed porównaniem.
fn dump_rows(conn: &Connection, tables: &[&str]) -> anyhow::Result<Vec<String>> {
    let mut dump = Vec::new();
    for table in tables {
        let mut columns_statement = conn.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
        let columns = columns_statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        assert!(
            !columns.is_empty(),
            "the fixture has no {table} table, so preserving its rows would prove nothing"
        );

        let mut rows_statement = conn.prepare(&format!("SELECT * FROM \"{table}\" ORDER BY 1"))?;
        let width = rows_statement.column_count();
        let rows = rows_statement.query_map([], move |row| {
            let mut cells = Vec::with_capacity(width);
            for index in 0..width {
                cells.push(format!("{:?}", row.get::<_, Value>(index)?));
            }
            Ok(cells.join(" | "))
        })?;

        dump.push(format!("{table} :: {}", columns.join(", ")));
        for row in rows {
            dump.push(format!("{table} == {}", row?));
        }
    }
    Ok(dump)
}

fn legacy_snapshot(conn: &Connection) -> anyhow::Result<Vec<String>> {
    let mut statement = conn.prepare(
        "SELECT type, name, tbl_name, sql FROM sqlite_master \
         WHERE name IN ('memory', 'idx_memory_scope') ORDER BY type, name",
    )?;
    let objects = statement
        .query_map([], |row| {
            Ok(format!(
                "{} | {} | {} | {}",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?.unwrap_or_default(),
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    assert_eq!(
        objects.len(),
        2,
        "the legacy fixture must carry both the memory table and idx_memory_scope"
    );

    let mut snapshot = objects;
    snapshot.extend(dump_rows(conn, &LEGACY_TABLES)?);
    Ok(snapshot)
}

fn assert_legacy_rows_are_planted(conn: &Connection) -> anyhow::Result<()> {
    for table in LEGACY_TABLES {
        let rows: i64 =
            conn.query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |row| {
                row.get(0)
            })?;
        assert_eq!(
            rows, 1,
            "the legacy fixture needs one control row in {table}, but planted {rows}"
        );
    }
    assert!(object_exists(conn, "index", "idx_memory_scope")?);
    Ok(())
}

fn plant_legacy_index(db: &Path, run_dir: &Path) -> anyhow::Result<(Vec<String>, Vec<String>)> {
    let conn = Connection::open(db)?;
    apply_pragmas(&conn)?;
    migrate(&conn)?;
    conn.execute_batch(LEGACY_MEMORY_DDL)?;

    let snapshot_value: serde_json::Value = serde_json::from_str(r#"{"nodes": [], "edges": []}"#)?;
    let workflow_snapshot = serde_json::to_string(&snapshot_value)?;
    conn.execute(
        "INSERT INTO runs (id, workflow_id, workflow_snapshot, title, status, concurrency, \
         created_at, started_at, ended_at, error, boot_id) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            RUN_ID,
            "prove-the-index-is-disposable",
            workflow_snapshot,
            "Keep notes in files",
            "succeeded",
            2_i64,
            1_787_781_600_000_i64,
            1_787_781_601_000_i64,
            1_787_781_603_000_i64,
            Option::<String>::None,
            "fixture-boot",
        ],
    )?;
    conn.execute(
        "INSERT INTO steps (id, run_id, node_key, name, agent, depends_on, status, attempt, \
         agent_session_id, pid, pgid, exit_code, started_at, ended_at, cost_usd, summary, error) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        params![
            STEP_ID,
            RUN_ID,
            "inspect",
            "Inspect the files",
            "codex",
            "[]",
            "succeeded",
            1_i64,
            "session-t140",
            4_140_i64,
            4_140_i64,
            0_i64,
            1_787_781_601_000_i64,
            1_787_781_603_000_i64,
            0.014_f64,
            "The run is recoverable from files",
            Option::<String>::None,
        ],
    )?;
    conn.execute(
        "INSERT INTO events (run_id, step_id, ts, kind, level, body) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            RUN_ID,
            STEP_ID,
            1_787_781_601_000_i64,
            "assistant",
            "raw",
            LOG_LINE,
        ],
    )?;

    let log_path = run_dir.join("logs").join(format!("agent-{STEP_ID}.jsonl"));
    let log_bytes = i64::try_from(LOG_LINE.len())?;
    conn.execute(
        "INSERT INTO artifacts (id, run_id, step_id, kind, name, path, bytes, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            format!("{RUN_ID}::logs/agent-{STEP_ID}.jsonl"),
            RUN_ID,
            STEP_ID,
            "raw_log",
            format!("agent-{STEP_ID}.jsonl"),
            log_path.to_string_lossy(),
            log_bytes,
            1_787_781_601_000_i64,
        ],
    )?;
    conn.execute(
        "INSERT INTO memory (id, scope, key, path, title, body, written_by_run, \
         written_by_step, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            "legacy-note",
            "this-project",
            "files-are-the-truth",
            "notes/files-are-the-truth.md",
            "Files are the truth",
            NOTE_BODY,
            RUN_ID,
            STEP_ID,
            1_787_781_602_000_i64,
            1_787_781_602_000_i64,
        ],
    )?;

    assert_legacy_rows_are_planted(&conn)?;

    Ok((legacy_snapshot(&conn)?, dump_rows(&conn, &LIVE_TABLES)?))
}

async fn snapshot_after_real_open(db: &Path) -> anyhow::Result<Vec<String>> {
    let store = Store::open(db)?;
    let reader = store.reader()?;
    let snapshot = legacy_snapshot(&reader)?;
    drop(reader);
    store.close().await?;
    Ok(snapshot)
}

async fn fresh_schema(db: &Path) -> anyhow::Result<(Vec<String>, bool)> {
    assert!(
        !db.exists(),
        "the fresh-index fixture started with a database"
    );
    let store = Store::open(db)?;
    let reader = store.reader()?;
    let tables = user_tables(&reader)?;
    let memory_index = object_exists(&reader, "index", "idx_memory_scope")?;
    drop(reader);
    store.close().await?;
    Ok((tables, memory_index))
}

#[tokio::test]
async fn fresh_and_rebuilt_indexes_have_only_live_tables_while_old_memory_is_left_alone()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let run_dir = root.path().join("runs").join("fixture-t140");
    write_run_files(&run_dir)?;

    // Stara baza ma przetrwać dwa prawdziwe starty bez destrukcyjnej migracji.
    let legacy_db = root.path().join("legacy-loadout.db");
    let (legacy_before, live_before) = plant_legacy_index(&legacy_db, &run_dir)?;
    assert_eq!(snapshot_after_real_open(&legacy_db).await?, legacy_before);
    assert_eq!(snapshot_after_real_open(&legacy_db).await?, legacy_before);

    // To jest uczciwa czerwień fazy `before`: dzisiejszy Store wciąż tworzy piątą tabelę.
    let fresh_db = root.path().join("fresh-loadout.db");
    let fresh = fresh_schema(&fresh_db).await?;
    assert_eq!(
        fresh,
        (LIVE_TABLES.iter().map(ToString::to_string).collect(), false),
        "a fresh Store must have exactly runs, steps, events and artifacts, and it must not \
         create idx_memory_scope. It actually opened with {fresh:?}"
    );

    // Kasujemy wyłącznie odtwarzalny indeks. Pliki biegu zostają i produkcyjna odbudowa musi
    // odtworzyć z nich wszystkie żywe fakty, bez przywracania historycznej tabeli `memory`.
    fs::remove_file(&legacy_db)?;
    assert!(run_dir.join("run.json").is_file());
    assert!(
        run_dir
            .join("logs")
            .join(format!("agent-{STEP_ID}.jsonl"))
            .is_file()
    );

    let rebuilt_store = Store::open(&legacy_db)?;
    rebuilt_store.rebuild_from(&run_dir).await?;
    let rebuilt_reader = rebuilt_store.reader()?;
    let rebuilt_schema = (
        user_tables(&rebuilt_reader)?,
        object_exists(&rebuilt_reader, "index", "idx_memory_scope")?,
    );
    let live_after = dump_rows(&rebuilt_reader, &LIVE_TABLES)?;
    drop(rebuilt_reader);
    rebuilt_store.close().await?;

    assert_eq!(
        rebuilt_schema,
        (LIVE_TABLES.iter().map(ToString::to_string).collect(), false),
        "rebuilding from run files recreated a dead memory table or lost a live table"
    );
    assert_eq!(
        live_after, live_before,
        "deleting the index changed live run facts even though run.json and its log remained"
    );

    Ok(())
}
