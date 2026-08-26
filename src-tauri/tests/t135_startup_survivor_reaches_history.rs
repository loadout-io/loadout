//! T-135 AC-2: an orphan that survives startup cleanup is recorded on the history path.

use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::reconcile::with_reaper;
use loadout_lib::engine::supervisor::machine_booted_at;
use loadout_lib::recovery::ReapOutcome;
use serde_json::Value;

const LEFT_OVER: &str = "20260827-101500__01990000-0000-7000-8000-000000000135";
const FINISHED_FOLDER: &str = "20260827-101400__01990000-0000-7000-8000-000000000136";
const RUN_ID: &str = "01990000-0000-7000-8000-000000000135";
const DEAD_STEP: &str = "01990000-0000-7000-8000-000000001351";
const SURVIVOR_STEP: &str = "01990000-0000-7000-8000-000000001352";
const DEAD_PID: i32 = 713_501;
const DEAD_PGID: i32 = 713_502;
const SURVIVOR_PID: i32 = 713_503;
const SURVIVOR_PGID: i32 = 713_504;
const CUT_OFF: &str =
    "Loadout closed while this step was still running, so the step was cut off with it.";
const STALE_SURVIVOR_ERROR: &str = "The agent failed before startup reconciliation completed.";

const FINISHED: &str = r#"{
  "id": "01990000-0000-7000-8000-000000000136",
  "title": "Already done",
  "status": "succeeded",
  "ended_at": 1787825640000,
  "steps": [
    { "id": "done", "name": "Done", "status": "succeeded", "ended_at": 1787825640000 }
  ]
}
"#;

#[test]
fn a_surviving_process_is_written_once_and_read_back_by_history() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let project = root.path();
    // This mirrors `with_reaper`: a host that cannot report boot time uses the empty sentinel on
    // both sides. The injected closer makes this criterion about persisted product behavior, not
    // about whether the test runner exposes macOS `kern.boottime`.
    let boot = machine_booted_at().unwrap_or_default();
    put(project, LEFT_OVER, &left_over_run(&boot))?;
    put(project, FINISHED_FOLDER, FINISHED)?;
    let finished_before = read_text(project, FINISHED_FOLDER)?;

    let mut calls = Vec::new();
    let reconciled = with_reaper(project, &mut |pgid| {
        calls.push(pgid);
        if pgid == DEAD_PGID {
            ReapOutcome::ProvenDead
        } else {
            ReapOutcome::StillAlive
        }
    });

    assert_eq!(
        calls,
        vec![DEAD_PGID, SURVIVOR_PGID],
        "each recorded group must be closed exactly once, in step order"
    );
    assert_eq!(reconciled.runs, 1);
    assert_eq!(reconciled.steps, 2);
    assert_eq!(reconciled.reaped, 1);
    assert_eq!(reconciled.still_alive, 1);

    let repaired = read_json(project, LEFT_OVER)?;
    assert_eq!(repaired["status"].as_str(), Some("interrupted"));
    assert!(
        repaired["ended_at"].as_i64().is_some(),
        "the interrupted run has no end time"
    );
    assert_eq!(
        repaired["future_top"]["kept"].as_bool(),
        Some(true),
        "rewriting through a known schema discarded an unknown top-level field"
    );

    let dead = step(&repaired, DEAD_STEP)?;
    assert_eq!(dead["status"].as_str(), Some("failed"));
    assert!(dead["ended_at"].as_i64().is_some());
    assert_eq!(
        dead["error"].as_str(),
        Some(CUT_OFF),
        "a group proven dead received the survivor warning"
    );
    assert_eq!(dead["future_step"].as_str(), Some("keep-dead"));

    let survivor = step(&repaired, SURVIVOR_STEP)?;
    assert_eq!(survivor["status"].as_str(), Some("failed"));
    assert!(survivor["ended_at"].as_i64().is_some());
    assert_eq!(survivor["future_step"].as_str(), Some("keep-survivor"));
    let survivor_error = survivor["error"]
        .as_str()
        .ok_or("the surviving process left no visible error on its step")?;
    assert_survivor_sentence(survivor_error);
    assert_ne!(
        survivor_error, CUT_OFF,
        "both outcomes still collapse into the same generic cut-off sentence"
    );
    assert_ne!(
        survivor_error, STALE_SURVIVOR_ERROR,
        "a stale step error hid the warning that its process survived startup cleanup"
    );

    let past = read_run_inner(project, LEFT_OVER)?;
    let history_dead = past
        .steps
        .iter()
        .find(|one| one.id == DEAD_STEP)
        .ok_or("history omitted the proven-dead step")?;
    let history_survivor = past
        .steps
        .iter()
        .find(|one| one.id == SURVIVOR_STEP)
        .ok_or("history omitted the surviving step")?;
    assert_eq!(history_dead.error, CUT_OFF);
    assert_eq!(
        history_survivor.error, survivor_error,
        "run.json has the warning, but PastStepWire.error -- the field rendered by history -- \
         does not expose that exact sentence"
    );

    assert_eq!(
        read_text(project, FINISHED_FOLDER)?,
        finished_before,
        "startup reconciliation rewrote a run that had already finished"
    );

    let repaired_once = read_text(project, LEFT_OVER)?;
    let mut second_calls = Vec::new();
    let second = with_reaper(project, &mut |pgid| {
        second_calls.push(pgid);
        ReapOutcome::ProvenDead
    });
    assert!(
        second_calls.is_empty(),
        "the second reconciliation tried to stop an already settled group: {second_calls:?}"
    );
    assert_eq!(second.runs, 0);
    assert_eq!(second.steps, 0);
    assert_eq!(
        read_text(project, LEFT_OVER)?,
        repaired_once,
        "the second reconciliation changed the file, duplicated the warning, or lost PID/PGID"
    );
    Ok(())
}

fn assert_survivor_sentence(said: &str) {
    assert!(
        said.to_ascii_lowercase().contains("surviv") && said.to_ascii_lowercase().contains("stop"),
        "the warning does not say in plain English that the process survived stopping: {said}"
    );
    assert!(
        said.contains(&format!("PID {SURVIVOR_PID}")),
        "the warning does not show the surviving process PID as a decimal value: {said}"
    );
    assert!(
        said.contains(&format!("PGID {SURVIVOR_PGID}")),
        "the warning does not show the surviving process group separately as a decimal value: \
         {said}"
    );
    assert!(
        !said.contains(&DEAD_PID.to_string()) && !said.contains(&DEAD_PGID.to_string()),
        "the survivor warning names identifiers from the proven-dead step: {said}"
    );
}

fn left_over_run(boot: &str) -> String {
    format!(
        r#"{{
  "id": "{RUN_ID}",
  "workflow_id": "t135.json",
  "workflow_hash": "t135-hash",
  "workflow_snapshot": {{ "format": 1 }},
  "title": "Startup cleanup",
  "status": "running",
  "concurrency": 2,
  "created_at": 1787825700000,
  "boot_id": "{boot}",
  "started_at": 1787825701000,
  "ended_at": null,
  "error": null,
  "future_top": {{ "kept": true }},
  "steps": [
    {{
      "id": "{DEAD_STEP}",
      "node_key": "dead",
      "name": "Stopped cleanly",
      "agent": "codex",
      "kind": "agent",
      "depends_on": [],
      "status": "running",
      "attempt": 0,
      "agent_session_id": "dead-session",
      "pid": {DEAD_PID},
      "pgid": {DEAD_PGID},
      "started_at": 1787825701000,
      "ended_at": null,
      "error": null,
      "future_step": "keep-dead"
    }},
    {{
      "id": "{SURVIVOR_STEP}",
      "node_key": "survivor",
      "name": "Still alive",
      "agent": "claude",
      "kind": "agent",
      "depends_on": [],
      "status": "running",
      "attempt": 0,
      "agent_session_id": "survivor-session",
      "pid": {SURVIVOR_PID},
      "pgid": {SURVIVOR_PGID},
      "started_at": 1787825701000,
      "ended_at": null,
      "error": "{STALE_SURVIVOR_ERROR}",
      "future_step": "keep-survivor"
    }}
  ]
}}
"#
    )
}

fn put(project: &Path, folder: &str, text: &str) -> Result<(), Box<dyn Error>> {
    let dir = project.join(".loadout").join("runs").join(folder);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("run.json"), text)?;
    Ok(())
}

fn read_text(project: &Path, folder: &str) -> Result<String, Box<dyn Error>> {
    Ok(fs::read_to_string(
        project
            .join(".loadout")
            .join("runs")
            .join(folder)
            .join("run.json"),
    )?)
}

fn read_json(project: &Path, folder: &str) -> Result<Value, Box<dyn Error>> {
    Ok(serde_json::from_str(&read_text(project, folder)?)?)
}

fn step<'a>(run: &'a Value, id: &str) -> Result<&'a Value, Box<dyn Error>> {
    run["steps"]
        .as_array()
        .and_then(|steps| {
            steps
                .iter()
                .find(|one| one["id"].as_str().is_some_and(|found| found == id))
        })
        .ok_or_else(|| format!("run.json omitted step {id}").into())
}
