//! T-121 AC-2: a late artifact failure rolls the whole run snapshot back.

use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use loadout_lib::store::{Result as StoreResult, Store};
use rusqlite::Connection;
use rusqlite::types::Value;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

const LOG_NAME: &str = "agent-0199a200-0000-7000-8000-000000000122.jsonl";
const OLD_HANDOFF: &str = "01__build__old.md";
const LATE_HANDOFF: &str = "99__build__late.md";
const TRIGGER_NAME: &str = "t121_reject_late_artifact";
const TRIGGER_REFUSAL: &str = "t121 late artifact refused";
const OLD_LOG: &str = concat!(
    r#"{"type":"old-zulu","message":"old-last"}"#,
    "\n",
    r#"{"type":"old-alpha","message":"old-first"}"#,
    "\n",
);
const NEW_LOG: &str = concat!(
    r#"{"type":"new-zulu","message":"new-last"}"#,
    "\n",
    r#"{"type":"new-alpha","message":"new-first"}"#,
    "\n",
);
const OLD_RUN: &str = r#"{
  "id": "0199a200-0000-7000-8000-000000000121",
  "workflow_id": "rollback-old",
  "workflow_snapshot": {"revision": "old"},
  "title": "Old rollback snapshot",
  "status": "failed",
  "concurrency": 1,
  "created_at": 3000,
  "boot_id": "old-boot",
  "started_at": 3100,
  "ended_at": 3900,
  "error": "old run error",
  "steps": [{
    "id": "0199a200-0000-7000-8000-000000000122",
    "node_key": "build",
    "name": "Old rollback step",
    "agent": "claude",
    "depends_on": ["old-parent"],
    "status": "failed",
    "attempt": 1,
    "agent_session_id": "old-session",
    "pid": 301,
    "pgid": 301,
    "exit_code": 1,
    "started_at": 3200,
    "ended_at": 3800,
    "cost_usd": null,
    "summary": "old rollback summary",
    "error": "old step error"
  }]
}
"#;
const NEW_RUN: &str = r#"{
  "id": "0199a200-0000-7000-8000-000000000121",
  "workflow_id": "rollback-new",
  "workflow_snapshot": {"revision": "new"},
  "title": "New rollback snapshot",
  "status": "succeeded",
  "concurrency": 5,
  "created_at": 4000,
  "boot_id": "new-boot",
  "started_at": 4100,
  "ended_at": 4900,
  "error": null,
  "steps": [{
    "id": "0199a200-0000-7000-8000-000000000122",
    "node_key": "build",
    "name": "New rollback step",
    "agent": "codex",
    "depends_on": [],
    "status": "succeeded",
    "attempt": 2,
    "agent_session_id": "new-session",
    "pid": 402,
    "pgid": 402,
    "exit_code": 0,
    "started_at": 4200,
    "ended_at": 4900,
    "cost_usd": null,
    "summary": "new rollback summary",
    "error": null
  }]
}
"#;

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    runs: Vec<String>,
    steps: Vec<String>,
    events: Vec<String>,
    artifacts: Vec<String>,
}

fn write_fixture(
    run_dir: &Path,
    run: &str,
    log: &str,
    handoff_name: &str,
    handoff: &str,
) -> TestResult {
    fs::create_dir_all(run_dir.join("logs"))?;
    fs::create_dir_all(run_dir.join("handoffs"))?;
    for stale in [OLD_HANDOFF, LATE_HANDOFF] {
        let path = run_dir.join("handoffs").join(stale);
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    fs::write(run_dir.join("run.json"), run)?;
    fs::write(run_dir.join("logs").join(LOG_NAME), log)?;
    fs::write(run_dir.join("handoffs").join(handoff_name), handoff)?;
    Ok(())
}

fn row_key(row: &[Value]) -> TestResult<String> {
    let mut key = String::new();
    for value in row {
        let cell = match value {
            Value::Null => "null".to_owned(),
            Value::Integer(value) => format!("integer:{value}"),
            Value::Real(value) => format!("real:{:016x}", value.to_bits()),
            Value::Text(value) => format!("text:{}:{value}", value.len()),
            Value::Blob(value) => format!("blob:{}:{value:?}", value.len()),
        };
        write!(&mut key, "{}:{cell}", cell.len())?;
    }
    Ok(key)
}

fn read_rows(conn: &Connection, sql: &str) -> TestResult<Vec<String>> {
    let mut statement = conn.prepare(sql)?;
    let width = statement.column_count();
    let rows = statement.query_map([], move |row| {
        let mut values = Vec::with_capacity(width);
        for column in 0..width {
            values.push(row.get::<_, Value>(column)?);
        }
        Ok(values)
    })?;
    let mut keys = rows
        .collect::<rusqlite::Result<Vec<_>>>()?
        .iter()
        .map(|row| row_key(row))
        .collect::<TestResult<Vec<_>>>()?;
    keys.sort();
    Ok(keys)
}

fn read_snapshot(conn: &Connection) -> TestResult<Snapshot> {
    Ok(Snapshot {
        runs: read_rows(
            conn,
            "SELECT id, workflow_id, workflow_snapshot, title, status, concurrency, created_at, \
             started_at, ended_at, error, boot_id FROM runs",
        )?,
        steps: read_rows(
            conn,
            "SELECT id, run_id, node_key, name, agent, depends_on, status, attempt, \
             agent_session_id, pid, pgid, exit_code, started_at, ended_at, cost_usd, summary, \
             error FROM steps",
        )?,
        events: read_rows(
            conn,
            "SELECT run_id, step_id, ts, kind, level, body FROM events",
        )?,
        artifacts: read_rows(
            conn,
            "SELECT id, run_id, step_id, kind, name, path, bytes, created_at FROM artifacts",
        )?,
    })
}

fn source_snapshot(run_dir: &Path) -> TestResult<Vec<(String, Vec<u8>)>> {
    let mut files = Vec::new();
    for relative_dir in ["", "logs", "handoffs"] {
        let directory = run_dir.join(relative_dir);
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let path = entry.path();
                let relative = path.strip_prefix(run_dir)?.to_string_lossy().into_owned();
                files.push((relative, fs::read(path)?));
            }
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(files)
}

async fn fresh_snapshot(database: &Path, run_dir: &Path) -> TestResult<Snapshot> {
    let store = Store::open(database)?;
    store.rebuild_from(run_dir).await?;
    let reader = store.reader()?;
    let snapshot = read_snapshot(&reader)?;
    drop(reader);
    store.close().await?;
    Ok(snapshot)
}

fn assert_all_four_collections_changed(old: &Snapshot, new: &Snapshot) -> TestResult {
    if old.runs == new.runs {
        return Err("the run fixture did not change".into());
    }
    if old.steps == new.steps {
        return Err("the step fixture did not change".into());
    }
    if old.events == new.events {
        return Err("the event fixture did not change".into());
    }
    if old.artifacts == new.artifacts {
        return Err("the artifact fixture did not change".into());
    }
    Ok(())
}

fn install_late_artifact_trigger(database: &Path) -> TestResult {
    let connection = Connection::open(database)?;
    connection.execute_batch(&format!(
        "CREATE TRIGGER {TRIGGER_NAME} BEFORE INSERT ON artifacts \
         WHEN NEW.name = '{LATE_HANDOFF}' BEGIN \
         SELECT RAISE(ABORT, '{TRIGGER_REFUSAL}'); END"
    ))?;
    Ok(())
}

fn remove_late_artifact_trigger(database: &Path) -> TestResult {
    let connection = Connection::open(database)?;
    connection.execute_batch(&format!("DROP TRIGGER {TRIGGER_NAME}"))?;
    Ok(())
}

fn assert_late_refusal(outcome: StoreResult<()>) -> TestResult {
    match outcome {
        Ok(()) => Err("the named late artifact was accepted despite the trigger".into()),
        Err(error) => {
            let message = error.to_string();
            assert!(
                message.contains(TRIGGER_REFUSAL),
                "the rebuild failed before reaching the named late artifact: {message}"
            );
            Ok(())
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn late_artifact_failure_keeps_the_whole_old_snapshot_until_retry() -> TestResult {
    let directory = tempfile::tempdir()?;
    let run_dir = directory.path().join("runs").join("t121-rollback");
    let database = directory.path().join("loadout.db");
    let expected_database = directory.path().join("expected.db");
    write_fixture(&run_dir, OLD_RUN, OLD_LOG, OLD_HANDOFF, "old handoff\n")?;

    let store = Arc::new(Store::open(&database)?);
    store.rebuild_from(&run_dir).await?;
    let old_snapshot = read_snapshot(&store.reader()?)?;

    write_fixture(
        &run_dir,
        NEW_RUN,
        NEW_LOG,
        LATE_HANDOFF,
        "late new handoff\n",
    )?;
    let source_files = source_snapshot(&run_dir)?;
    let expected_snapshot = fresh_snapshot(&expected_database, &run_dir).await?;
    assert_all_four_collections_changed(&old_snapshot, &expected_snapshot)?;
    install_late_artifact_trigger(&database)?;

    let rebuild_store = Arc::clone(&store);
    let rebuild_run_dir = run_dir.clone();
    let rebuild = tokio::spawn(async move { rebuild_store.rebuild_from(&rebuild_run_dir).await });
    let mut snapshots_seen = 0_usize;
    while !rebuild.is_finished() {
        assert_eq!(
            read_snapshot(&store.reader()?)?,
            old_snapshot,
            "a reader observed an empty or mixed snapshot while replacement was in flight"
        );
        snapshots_seen += 1;
        tokio::task::yield_now().await;
    }
    assert!(
        snapshots_seen > 0,
        "the rebuild completed before the concurrent reader could observe the database"
    );
    assert_late_refusal(rebuild.await?)?;
    assert_eq!(read_snapshot(&store.reader()?)?, old_snapshot);
    assert_eq!(source_snapshot(&run_dir)?, source_files);

    remove_late_artifact_trigger(&database)?;
    store.rebuild_from(&run_dir).await?;
    assert_eq!(read_snapshot(&store.reader()?)?, expected_snapshot);
    assert_eq!(
        source_snapshot(&run_dir)?,
        source_files,
        "index replacement changed one or more authoritative source files"
    );
    let store = Arc::try_unwrap(store)
        .map_err(|_| "the concurrent rebuild kept an extra Store owner alive")?;
    store.close().await?;
    Ok(())
}
