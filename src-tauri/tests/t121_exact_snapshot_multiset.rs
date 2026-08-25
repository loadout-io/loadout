//! T-121 AC-1: rebuilding an existing run replaces its four-table index exactly.

use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use loadout_lib::store::Store;
use rusqlite::Connection;
use rusqlite::types::Value;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

const RUN_ID: &str = "0199a100-0000-7000-8000-000000000121";
const STEP_ID: &str = "0199a100-0000-7000-8000-000000000122";
const LOG_NAME: &str = "agent-0199a100-0000-7000-8000-000000000122.jsonl";
const OLD_HANDOFF: &str = "01__build__old.md";
const NEW_HANDOFF: &str = "02__build__new.md";
const OLD_LOG: &str = concat!(
    r#"{"type":"zulu","message":"old-last"}"#,
    "\n",
    r#"{"type":"alpha","message":"old-first"}"#,
    "\n",
);
const NEW_LOG: &str = concat!(
    r#"{"type":"zulu","message":"new-last"}"#,
    "\n",
    r#"{"type":"alpha","message":"new-first"}"#,
    "\n",
);
const OLD_RUN: &str = r#"{
  "id": "0199a100-0000-7000-8000-000000000121",
  "workflow_id": "old-workflow",
  "workflow_snapshot": {"revision": "old"},
  "title": "Old snapshot",
  "status": "failed",
  "concurrency": 1,
  "created_at": 1000,
  "boot_id": "old-boot",
  "started_at": 1100,
  "ended_at": 1900,
  "error": "old error",
  "steps": [{
    "id": "0199a100-0000-7000-8000-000000000122",
    "node_key": "build",
    "name": "Old step",
    "agent": "claude",
    "depends_on": ["old-parent"],
    "status": "failed",
    "attempt": 1,
    "agent_session_id": "old-session",
    "pid": 101,
    "pgid": 101,
    "exit_code": 1,
    "started_at": 1200,
    "ended_at": 1800,
    "cost_usd": null,
    "summary": "old summary",
    "error": "old step error"
  }]
}
"#;
const NEW_RUN: &str = r#"{
  "id": "0199a100-0000-7000-8000-000000000121",
  "workflow_id": "new-workflow",
  "workflow_snapshot": {"revision": "new"},
  "title": "New snapshot",
  "status": "succeeded",
  "concurrency": 4,
  "created_at": 2000,
  "boot_id": "new-boot",
  "started_at": 2100,
  "ended_at": 2900,
  "error": null,
  "steps": [{
    "id": "0199a100-0000-7000-8000-000000000122",
    "node_key": "build",
    "name": "New step",
    "agent": "codex",
    "depends_on": [],
    "status": "succeeded",
    "attempt": 2,
    "agent_session_id": "new-session",
    "pid": 202,
    "pgid": 202,
    "exit_code": 0,
    "started_at": 2400,
    "ended_at": 2900,
    "cost_usd": null,
    "summary": "new summary",
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
    for stale in [OLD_HANDOFF, NEW_HANDOFF] {
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

fn canonical_rows(rows: &[Vec<Value>]) -> TestResult<Vec<String>> {
    let mut keys = rows
        .iter()
        .map(|row| row_key(row))
        .collect::<TestResult<Vec<_>>>()?;
    keys.sort();
    Ok(keys)
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
    let rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    canonical_rows(&rows)
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

fn expected_runs() -> TestResult<Vec<String>> {
    let rows = vec![vec![
        Value::Text(RUN_ID.to_owned()),
        Value::Text("new-workflow".to_owned()),
        Value::Text(r#"{"revision":"new"}"#.to_owned()),
        Value::Text("New snapshot".to_owned()),
        Value::Text("succeeded".to_owned()),
        Value::Integer(4),
        Value::Integer(2000),
        Value::Integer(2100),
        Value::Integer(2900),
        Value::Null,
        Value::Text("new-boot".to_owned()),
    ]];
    canonical_rows(&rows)
}

fn expected_steps() -> TestResult<Vec<String>> {
    let rows = vec![vec![
        Value::Text(STEP_ID.to_owned()),
        Value::Text(RUN_ID.to_owned()),
        Value::Text("build".to_owned()),
        Value::Text("New step".to_owned()),
        Value::Text("codex".to_owned()),
        Value::Text("[]".to_owned()),
        Value::Text("succeeded".to_owned()),
        Value::Integer(2),
        Value::Text("new-session".to_owned()),
        Value::Integer(202),
        Value::Integer(202),
        Value::Integer(0),
        Value::Integer(2400),
        Value::Integer(2900),
        Value::Null,
        Value::Text("new summary".to_owned()),
        Value::Null,
    ]];
    canonical_rows(&rows)
}

fn expected_events() -> TestResult<Vec<String>> {
    let rows: Vec<Vec<Value>> = NEW_LOG
        .lines()
        .map(|body| {
            let kind = if body.contains(r#""type":"zulu""#) {
                "zulu"
            } else {
                "alpha"
            };
            vec![
                Value::Text(RUN_ID.to_owned()),
                Value::Text(STEP_ID.to_owned()),
                Value::Integer(2400),
                Value::Text(kind.to_owned()),
                Value::Text("raw".to_owned()),
                Value::Text(body.to_owned()),
            ]
        })
        .collect();
    canonical_rows(&rows)
}

fn expected_artifacts(run_dir: &Path) -> TestResult<Vec<String>> {
    let log = run_dir.join("logs").join(LOG_NAME);
    let handoff = run_dir.join("handoffs").join(NEW_HANDOFF);
    let log_bytes = i64::try_from(fs::metadata(&log)?.len())?;
    let handoff_bytes = i64::try_from(fs::metadata(&handoff)?.len())?;
    let rows = vec![
        vec![
            Value::Text(format!("{RUN_ID}::logs/{LOG_NAME}")),
            Value::Text(RUN_ID.to_owned()),
            Value::Text(STEP_ID.to_owned()),
            Value::Text("raw_log".to_owned()),
            Value::Text(LOG_NAME.to_owned()),
            Value::Text(log.to_string_lossy().into_owned()),
            Value::Integer(log_bytes),
            Value::Integer(2400),
        ],
        vec![
            Value::Text(format!("{RUN_ID}::handoffs/{NEW_HANDOFF}")),
            Value::Text(RUN_ID.to_owned()),
            Value::Null,
            Value::Text("handoff".to_owned()),
            Value::Text(NEW_HANDOFF.to_owned()),
            Value::Text(handoff.to_string_lossy().into_owned()),
            Value::Integer(handoff_bytes),
            Value::Integer(2000),
        ],
    ];
    canonical_rows(&rows)
}

fn expected_snapshot(run_dir: &Path) -> TestResult<Snapshot> {
    Ok(Snapshot {
        runs: expected_runs()?,
        steps: expected_steps()?,
        events: expected_events()?,
        artifacts: expected_artifacts(run_dir)?,
    })
}

fn assert_old_artifact_absent(conn: &Connection) -> TestResult {
    let id = format!("{RUN_ID}::handoffs/{OLD_HANDOFF}");
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM artifacts WHERE id = ?1",
        [id],
        |row| row.get(0),
    )?;
    assert_eq!(
        count, 0,
        "the old artifact survived the replacement even though its source file is gone"
    );
    Ok(())
}

#[tokio::test]
async fn repeated_rebuild_is_the_exact_new_multiset_and_leaves_no_old_rows() -> TestResult {
    let directory = tempfile::tempdir()?;
    let run_dir = directory.path().join("runs").join("t121-exact-snapshot");
    let database = directory.path().join("loadout.db");
    write_fixture(&run_dir, OLD_RUN, OLD_LOG, OLD_HANDOFF, "old handoff\n")?;

    let store = Store::open(&database)?;
    store.rebuild_from(&run_dir).await?;
    write_fixture(&run_dir, NEW_RUN, NEW_LOG, NEW_HANDOFF, "new handoff\n")?;
    let source_run = fs::read(run_dir.join("run.json"))?;
    let expected = expected_snapshot(&run_dir)?;

    store.rebuild_from(&run_dir).await?;
    let reader = store.reader()?;
    assert_eq!(read_snapshot(&reader)?, expected);
    assert_old_artifact_absent(&reader)?;
    drop(reader);

    store.rebuild_from(&run_dir).await?;
    let reader = store.reader()?;
    assert_eq!(read_snapshot(&reader)?, expected_snapshot(&run_dir)?);
    assert_old_artifact_absent(&reader)?;
    assert_eq!(
        fs::read(run_dir.join("run.json"))?,
        source_run,
        "rebuilding the index rewrote the source run.json"
    );
    drop(reader);
    store.close().await?;
    Ok(())
}
