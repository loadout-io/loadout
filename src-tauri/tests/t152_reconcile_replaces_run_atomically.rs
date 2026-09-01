//! T-152 AC-4: recovery publikuje cały `run.json` przez wspólny durable replace albo nic.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use loadout_lib::commands::reconcile::with_reaper;
use loadout_lib::durable_file::{
    FaultAction, FaultInjector, FaultPoint, PublicationEvent, PublicationOperation, scoped_faults,
};
use loadout_lib::engine::supervisor::machine_booted_at;
use loadout_lib::recovery::ReapOutcome;
use serde_json::{Value, json};
use tempfile::TempDir;

const FOLDER: &str = "20260828-130000__019b0152-0000-7000-8000-000000000006";
const RUN_ID: &str = "019b0152-0000-7000-8000-000000000006";
const STEP_ID: &str = "019b0152-0000-7000-8000-000000000007";
const PGID: i32 = 31_552;

#[derive(Clone, Copy, Debug)]
enum PathUnderTest {
    ParkedQuestion,
    RunningProcess,
}

#[derive(Debug)]
struct Bench {
    _root: TempDir,
    project: PathBuf,
    run_dir: PathBuf,
}

impl Bench {
    fn new(path: PathUnderTest) -> Result<Self, Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let project = root.path().join("project");
        let run_dir = project.join(".loadout/runs").join(FOLDER);
        fs::create_dir_all(&run_dir)?;
        let boot = machine_booted_at().unwrap_or_default();
        let (run_status, step_status, process_id, process_group_id) = match path {
            PathUnderTest::ParkedQuestion => ("paused", "pending", Value::Null, Value::Null),
            PathUnderTest::RunningProcess => {
                ("running", "running", Value::from(PGID), Value::from(PGID))
            }
        };
        let receipt = json!({
            "id": RUN_ID,
            "workflow_id": "t152-recovery.json",
            "workflow_hash": "before-reconcile",
            "workflow_snapshot": {
                "format": 1,
                "unknownGraphKey": { "kept": true }
            },
            "title": "T152 recovery",
            "status": run_status,
            "concurrency": 1,
            "created_at": 1_787_922_000_000_i64,
            "boot_id": boot,
            "started_at": 1_787_922_000_001_i64,
            "ended_at": null,
            "error": null,
            "unknownRunKey": { "bytes": "must survive" },
            "steps": [{
                "id": STEP_ID,
                "node_key": "recover",
                "name": "Recover me",
                "agent": "codex",
                "kind": "agent",
                "depends_on": [],
                "status": step_status,
                "attempt": 0,
                "pid": process_id,
                "pgid": process_group_id,
                "started_at": 1_787_922_000_001_i64,
                "ended_at": null,
                "error": null,
                "unknownStepKey": ["also", "kept"]
            }]
        });
        fs::write(
            run_dir.join("run.json"),
            serde_json::to_vec_pretty(&receipt)?,
        )?;
        Ok(Self {
            _root: root,
            project,
            run_dir,
        })
    }

    fn bytes(&self) -> Result<Vec<u8>, Box<dyn Error>> {
        Ok(fs::read(self.run_dir.join("run.json"))?)
    }

    fn reconcile(&self, path: PathUnderTest, survivor: bool) {
        let _ = with_reaper(&self.project, &mut |_pgid| match (path, survivor) {
            (PathUnderTest::RunningProcess, true) => ReapOutcome::StillAlive,
            _ => ReapOutcome::ProvenDead,
        });
    }
}

#[derive(Debug)]
struct RefuseOnce {
    point: FaultPoint,
    armed: AtomicBool,
    seen: AtomicUsize,
}

impl RefuseOnce {
    fn new(point: FaultPoint) -> Self {
        Self {
            point,
            armed: AtomicBool::new(true),
            seen: AtomicUsize::new(0),
        }
    }
}

impl FaultInjector for RefuseOnce {
    fn action(&self, event: &PublicationEvent) -> FaultAction {
        if event.operation == PublicationOperation::Replace
            && event.point == self.point
            && event
                .target
                .file_name()
                .is_some_and(|name| name == "run.json")
            && self.armed.swap(false, Ordering::AcqRel)
        {
            self.seen.fetch_add(1, Ordering::AcqRel);
            return FaultAction::Fail;
        }
        FaultAction::Continue
    }
}

#[test]
fn both_reconcile_paths_keep_old_bytes_when_publish_is_refused() -> Result<(), Box<dyn Error>> {
    for path in [PathUnderTest::ParkedQuestion, PathUnderTest::RunningProcess] {
        let bench = Bench::new(path)?;
        let before = bench.bytes()?;
        let faults = Arc::new(RefuseOnce::new(FaultPoint::BeforeCommit));
        let hook: Arc<dyn FaultInjector> = faults.clone();
        let _scope = scoped_faults(&bench.run_dir, hook)?;

        bench.reconcile(path, false);

        assert_eq!(
            faults.seen.load(Ordering::Acquire),
            1,
            "{path:?} bypassed the shared durable publisher"
        );
        assert_eq!(
            bench.bytes()?,
            before,
            "{path:?} exposed changed bytes although publication was refused before commit"
        );
        assert_no_temps(&bench.run_dir)?;
    }
    Ok(())
}

#[test]
fn commit_point_failure_exposes_only_old_or_complete_new_json() -> Result<(), Box<dyn Error>> {
    for path in [PathUnderTest::ParkedQuestion, PathUnderTest::RunningProcess] {
        let bench = Bench::new(path)?;
        let before = bench.bytes()?;
        let faults = Arc::new(RefuseOnce::new(FaultPoint::AfterCommit));
        let hook: Arc<dyn FaultInjector> = faults.clone();
        let scope = scoped_faults(&bench.run_dir, hook)?;
        bench.reconcile(path, false);
        let after = bench.bytes()?;

        assert_eq!(faults.seen.load(Ordering::Acquire), 1);
        let parsed: Value = serde_json::from_slice(&after).map_err(|error| {
            format!("{path:?} exposed a partial run.json at the commit point: {error}")
        })?;
        assert!(
            after == before || parsed.get("ended_at").is_some_and(|value| !value.is_null()),
            "{path:?} exposed neither the exact old receipt nor a complete reconciled receipt"
        );
        assert_unknown_fields(&parsed);
        assert_no_temps(&bench.run_dir)?;

        drop(scope);
        bench.reconcile(path, false);
        let stable = bench.bytes()?;
        let _: Value = serde_json::from_slice(&stable)?;
        assert_no_temps(&bench.run_dir)?;
    }
    Ok(())
}

#[test]
fn successful_reconcile_is_complete_preserves_unknowns_and_is_idempotent()
-> Result<(), Box<dyn Error>> {
    for path in [PathUnderTest::ParkedQuestion, PathUnderTest::RunningProcess] {
        let bench = Bench::new(path)?;
        bench.reconcile(path, matches!(path, PathUnderTest::RunningProcess));
        let once = bench.bytes()?;
        let parsed: Value = serde_json::from_slice(&once)?;
        assert_unknown_fields(&parsed);
        assert!(
            parsed.get("ended_at").is_some_and(|value| !value.is_null()),
            "reconciled receipt has no end time: {parsed:?}"
        );
        if matches!(path, PathUnderTest::ParkedQuestion) {
            assert!(
                parsed.get("error").and_then(Value::as_str).is_some(),
                "the abandoned question has no durable reason"
            );
        } else {
            assert!(
                parsed
                    .pointer("/steps/0/error")
                    .and_then(Value::as_str)
                    .is_some_and(|error| error.contains("survived")),
                "the survivor warning did not reach the only error shown in history: {parsed:?}"
            );
        }
        assert_no_temps(&bench.run_dir)?;

        bench.reconcile(path, matches!(path, PathUnderTest::RunningProcess));
        assert_eq!(bench.bytes()?, once, "second reconcile rewrote {path:?}");
        assert_no_temps(&bench.run_dir)?;
    }
    Ok(())
}

fn assert_unknown_fields(run: &Value) {
    assert_eq!(
        run.pointer("/unknownRunKey/bytes").and_then(Value::as_str),
        Some("must survive")
    );
    assert_eq!(
        run.pointer("/workflow_snapshot/unknownGraphKey/kept")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        run.pointer("/steps/0/unknownStepKey/1")
            .and_then(Value::as_str),
        Some("kept")
    );
}

fn assert_no_temps(run_dir: &Path) -> Result<(), Box<dyn Error>> {
    let leftovers: Vec<String> = fs::read_dir(run_dir)?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| {
            name.starts_with(".loadout-writing-")
                || Path::new(name)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("tmp"))
        })
        .collect();
    assert!(
        leftovers.is_empty(),
        "publisher temps survived: {leftovers:?}"
    );
    Ok(())
}
