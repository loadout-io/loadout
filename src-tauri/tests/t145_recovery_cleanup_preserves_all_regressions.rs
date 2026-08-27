//! AC-1 dla T-145: recovery sprząta i oznacza tylko faktycznie przerwane kroki.
//!
//! Każdy scenariusz wchodzi przez produkcyjne [`recovery::rows_to_judge`]. Dzięki temu skrajne
//! wartości w kolumnach adapterowych istnieją naprawdę w `SQLite`, lecz nie mogą już sterować
//! decyzją recovery.

use anyhow::{Context as _, Result};
use loadout_lib::recovery::{self, Machine, RecoveryPlan, RecoveryRow};
use rusqlite::{Connection, params};
use serde_json::Value as Json;

const BOOT_NOW: &str = "1787900000";
const BOOT_OLD: &str = "1787800000";
const OWN_PGID: i32 = 8145;
const SHARED_PGID: i32 = 8101;

struct Fixture {
    conn: Connection,
}

impl Fixture {
    fn new() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            r"CREATE TABLE runs (
                  id TEXT PRIMARY KEY,
                  status TEXT NOT NULL,
                  boot_id TEXT
              );
              CREATE TABLE steps (
                  id TEXT PRIMARY KEY,
                  run_id TEXT NOT NULL,
                  status TEXT NOT NULL,
                  pid INTEGER,
                  pgid INTEGER,
                  agent_session_id TEXT,
                  attempt INTEGER NOT NULL
              );",
        )?;
        Ok(Self { conn })
    }

    fn run(&self, id: &str, status: &str, boot: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO runs (id, status, boot_id) VALUES (?1, ?2, ?3)",
            params![id, status, boot],
        )?;
        Ok(())
    }

    fn step(
        &self,
        id: &str,
        run: &str,
        status: &str,
        pgid: Option<i32>,
        session: Option<&str>,
        attempt: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO steps
                (id, run_id, status, pid, pgid, agent_session_id, attempt)
             VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6)",
            params![id, run, status, pgid, session, attempt],
        )?;
        Ok(())
    }

    fn rows(&self) -> Result<Vec<RecoveryRow>> {
        Ok(recovery::rows_to_judge(&self.conn)?)
    }
}

fn machine() -> Machine {
    Machine {
        boot_id: BOOT_NOW.to_owned(),
        own_pgid: OWN_PGID,
    }
}

fn row_ids(rows: &[RecoveryRow]) -> Vec<String> {
    let mut ids: Vec<String> = rows.iter().map(|row| row.step_id.clone()).collect();
    ids.sort();
    ids
}

fn changed_run_ids(plan: &RecoveryPlan) -> Vec<String> {
    let mut ids: Vec<String> = plan
        .run_status
        .iter()
        .map(|change| change.run_id.clone())
        .collect();
    ids.sort();
    ids
}

fn changed_step_ids(plan: &RecoveryPlan) -> Vec<String> {
    let mut ids: Vec<String> = plan
        .step_status
        .iter()
        .map(|change| change.step_id.clone())
        .collect();
    ids.sort();
    ids
}

fn unreadable_ids(plan: &RecoveryPlan) -> Vec<String> {
    let mut ids: Vec<String> = plan
        .unreadable
        .iter()
        .map(|entry| entry.step_id.clone())
        .collect();
    ids.sort();
    ids
}

fn assert_failed_as_interrupted(plan: &RecoveryPlan, expected: &[&str]) {
    let mut actual: Vec<String> = plan
        .step_status
        .iter()
        .map(|change| {
            format!(
                "{} -> {} / {}",
                change.step_id, change.status, change.reason
            )
        })
        .collect();
    actual.sort();
    let mut expected: Vec<String> = expected
        .iter()
        .map(|step| format!("{step} -> failed / interrupted"))
        .collect();
    expected.sort();
    assert_eq!(
        actual, expected,
        "only ready/running steps may become failed with interrupted as a separate reason"
    );
}

fn resolved_rows(rows: &[RecoveryRow], plan: &RecoveryPlan) -> Vec<RecoveryRow> {
    rows.iter()
        .filter(|row| {
            plan.run_status
                .iter()
                .any(|change| change.run_id == row.run_id)
        })
        .map(|row| {
            let mut resolved = row.clone();
            if let Some(change) = plan
                .run_status
                .iter()
                .find(|change| change.run_id == row.run_id)
            {
                resolved.run_status.clone_from(&change.status);
            }
            if let Some(change) = plan
                .step_status
                .iter()
                .find(|change| change.step_id == row.step_id)
            {
                resolved.step_status.clone_from(&change.status);
            }
            resolved
        })
        .collect()
}

fn plan_is_empty(plan: &RecoveryPlan) -> bool {
    plan.reap.is_empty()
        && plan.run_status.is_empty()
        && plan.step_status.is_empty()
        && plan.unreadable.is_empty()
}

fn collect_keys(value: &Json, path: &str, found: &mut Vec<String>) {
    match value {
        Json::Object(fields) => {
            for (key, child) in fields {
                let here = format!("{path}.{key}");
                found.push(here.clone());
                collect_keys(child, &here, found);
            }
        }
        Json::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                collect_keys(child, &format!("{path}[{index}]"), found);
            }
        }
        _ => {}
    }
}

#[test]
fn the_production_query_includes_live_domains_and_excludes_finished_ones() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.run("running-run", "running", BOOT_NOW)?;
    fixture.run("paused-run", "paused", BOOT_NOW)?;
    fixture.run("finished-run", "succeeded", BOOT_NOW)?;
    fixture.step(
        "finished-inside-running-run",
        "running-run",
        "succeeded",
        Some(8001),
        None,
        0,
    )?;
    fixture.step(
        "running-inside-paused-run",
        "paused-run",
        "running",
        Some(8002),
        None,
        0,
    )?;
    fixture.step(
        "finished-outside-sweep",
        "finished-run",
        "succeeded",
        Some(8003),
        None,
        0,
    )?;

    assert_eq!(
        row_ids(&fixture.rows()?),
        vec![
            "finished-inside-running-run".to_owned(),
            "running-inside-paused-run".to_owned(),
        ],
        "rows_to_judge must return every row of a running run and a running step from another \
         run, while a finished step in a finished run remains outside recovery"
    );
    Ok(())
}

#[test]
fn adapter_session_and_attempt_extremes_do_not_block_cleanup() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.run("mixed-run", "running", BOOT_NOW)?;
    fixture.step(
        "running-no-session-negative-attempt",
        "mixed-run",
        "running",
        Some(SHARED_PGID),
        None,
        -1,
    )?;
    fixture.step(
        "ready-empty-session-max-attempt",
        "mixed-run",
        "ready",
        Some(SHARED_PGID),
        Some(""),
        i64::MAX,
    )?;
    fixture.step(
        "finished-with-leftover-pgid",
        "mixed-run",
        "succeeded",
        Some(8099),
        Some("finished-session"),
        7,
    )?;

    let plan = recovery::decide(&fixture.rows()?, &machine());
    assert_eq!(
        plan.reap,
        vec![SHARED_PGID],
        "both interrupted rows share one safe process group, so cleanup targets it once; the \
         finished row's leftover pgid must not enter the list"
    );
    assert_eq!(changed_run_ids(&plan), vec!["mixed-run".to_owned()]);
    assert_failed_as_interrupted(
        &plan,
        &[
            "running-no-session-negative-attempt",
            "ready-empty-session-max-attempt",
        ],
    );
    assert!(
        plan.unreadable.is_empty(),
        "session_id and attempt belong to explicit adapter transport, so none of their extreme \
         values can make an otherwise safe recovery row unreadable: {:?}",
        plan.unreadable
    );
    Ok(())
}

#[test]
fn a_run_changes_only_after_one_of_its_rows_is_proven_cut_off() -> Result<()> {
    let fixture = Fixture::new()?;
    for (run, status) in [
        ("settled-only", "running"),
        ("unknown-step-run", "running"),
        ("unknown-run", "draining"),
        ("paused-with-live-step", "paused"),
    ] {
        fixture.run(run, status, BOOT_NOW)?;
    }
    fixture.step(
        "settled-step",
        "settled-only",
        "succeeded",
        Some(8201),
        None,
        0,
    )?;
    fixture.step(
        "future-step-status",
        "unknown-step-run",
        "waiting_for_vendor",
        Some(8202),
        Some("adapter-data"),
        0,
    )?;
    fixture.step(
        "future-run-status",
        "unknown-run",
        "running",
        Some(8203),
        Some("adapter-data"),
        0,
    )?;
    fixture.step(
        "paused-live-step",
        "paused-with-live-step",
        "running",
        Some(8204),
        Some("adapter-data"),
        0,
    )?;

    let plan = recovery::decide(&fixture.rows()?, &machine());
    assert_eq!(
        changed_run_ids(&plan),
        vec!["paused-with-live-step".to_owned()],
        "a running label on a run is not enough: only RowVerdict::CutOff permits the run-status \
         change. A finished-only run and either unknown wire status provide no such proof"
    );
    assert_eq!(changed_step_ids(&plan), vec!["paused-live-step".to_owned()]);
    assert_eq!(plan.reap, vec![8204]);
    assert_eq!(
        unreadable_ids(&plan),
        vec![
            "future-run-status".to_owned(),
            "future-step-status".to_owned(),
        ],
        "unknown statuses remain named refusals instead of being guessed or dropped"
    );
    Ok(())
}

#[test]
fn rebooted_rows_skip_pgid_validation_but_current_rows_do_not() -> Result<()> {
    let fixture = Fixture::new()?;
    let invalid = [None, Some(0), Some(-9), Some(OWN_PGID)];
    let mut old_steps = Vec::new();
    let mut old_runs = Vec::new();
    let mut current_steps = Vec::new();

    for (index, pgid) in invalid.into_iter().enumerate() {
        let old_run = format!("old-run-{index}");
        let old_step = format!("old-step-{index}");
        fixture.run(&old_run, "running", BOOT_OLD)?;
        fixture.step(
            &old_step,
            &old_run,
            "running",
            pgid,
            Some("adapter-data"),
            0,
        )?;
        old_runs.push(old_run);
        old_steps.push(old_step);

        let current_run = format!("current-run-{index}");
        let current_step = format!("current-step-{index}");
        fixture.run(&current_run, "running", BOOT_NOW)?;
        fixture.step(
            &current_step,
            &current_run,
            "running",
            pgid,
            Some("adapter-data"),
            0,
        )?;
        current_steps.push(current_step);
    }

    old_runs.sort();
    old_steps.sort();
    current_steps.sort();
    let plan = recovery::decide(&fixture.rows()?, &machine());
    assert!(
        plan.reap.is_empty(),
        "a reboot means no recorded pgid is signalled, while every current-boot value in this \
         fixture is unsafe: {:?}",
        plan.reap
    );
    assert_eq!(changed_run_ids(&plan), old_runs);
    assert_eq!(changed_step_ids(&plan), old_steps);
    assert_eq!(
        unreadable_ids(&plan),
        current_steps,
        "None, zero, a negative pgid and Loadout's own pgid are refusals on the current boot. \
         After a reboot they cannot be reap targets, so their values must not block marking"
    );
    Ok(())
}

#[test]
fn the_serialized_plan_has_one_recursive_shape_and_a_resolved_second_pass_is_empty() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.run("idempotent-run", "running", BOOT_NOW)?;
    fixture.step(
        "idempotent-step",
        "idempotent-run",
        "running",
        Some(8401),
        Some("session-must-stay-in-sql"),
        23,
    )?;
    let rows = fixture.rows()?;
    let first = recovery::decide(&rows, &machine());
    assert_failed_as_interrupted(&first, &["idempotent-step"]);

    let second = recovery::decide(&resolved_rows(&rows, &first), &machine());
    assert!(
        plan_is_empty(&second),
        "the first pass resolved the only row; signalling or marking it again risks a recycled \
         process group and duplicates the status writes: {second:?}"
    );

    let wire = serde_json::to_value(&first)?;
    let object = wire
        .as_object()
        .context("RecoveryPlan must serialize as the object consumed by startup")?;
    let mut top_level: Vec<&str> = object.keys().map(String::as_str).collect();
    top_level.sort_unstable();
    assert_eq!(
        top_level,
        vec!["reap", "run_status", "step_status", "unreadable"],
        "recovery has one output shape; top-level keys were {top_level:?}"
    );

    let mut keys = Vec::new();
    collect_keys(&wire, "plan", &mut keys);
    assert!(
        !keys.is_empty(),
        "the recursive key sweep must not be vacuous"
    );
    for forbidden in [
        "ask",
        "question",
        "session",
        "attempt",
        "option",
        "effect",
        "resume",
        "pick_up",
        "start_over",
    ] {
        assert!(
            !keys
                .iter()
                .any(|path| path.to_lowercase().contains(forbidden)),
            "{forbidden:?} appeared below one of the four allowed lists. The ban is recursive, \
             so nested status entries cannot smuggle resume transport back in; keys were \
             {keys:?}"
        );
    }
    Ok(())
}
