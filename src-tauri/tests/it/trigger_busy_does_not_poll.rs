//! AC-4 dla T-65: o zajętości triggera rozstrzyga `AppState.live`, przed fetcherem i dyskiem.
//!
//! `RunState.workflow` z okna potrafi zostać wyzerowany przez `finally` odrzuconego drugiego
//! Startu, kiedy pierwszy bieg nadal żyje. Ten test nie ma więc żadnego argumentu opisującego
//! stan webviewa: bierze dokładnie ten sam rustowy uchwyt co Start i `/ask`, a następnie pyta
//! pozwolenie używane przez produkcyjne `check_trigger`.

#![allow(clippy::expect_used)]

use std::cell::Cell;
use std::error::Error;
use std::fs;
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use loadout_lib::commands::triggers::{self, TriggerPoll};
use loadout_lib::commands::{Drivers, RunDeps};
use loadout_lib::engine::drivers::AgentDriver;
use loadout_lib::engine::drivers::absent::Absent;
use loadout_lib::ipc::AppState;
use loadout_lib::store::Store;
use serde_json::json;
use tempfile::TempDir;

const NOW: i64 = 1_777_777_777_000;
const KEY: &str = "lin_api_1234567890123456789012345678901234567890";

#[derive(Debug, Clone, Copy)]
enum Road {
    Start,
    Ask,
}

impl Road {
    fn name(self) -> &'static str {
        match self {
            Self::Start => "run_workflow",
            Self::Ask => "run_agent",
        }
    }

    fn take<'a>(self, state: &'a AppState, project: &'a Path) -> Result<RunDeps<'a>, String> {
        match self {
            Self::Start => state.begin_run(project),
            Self::Ask => state.begin_a_run(project),
        }
    }
}

const ROADS: [Road; 2] = [Road::Start, Road::Ask];

#[tokio::test]
async fn both_run_doors_return_busy_before_fetch_or_any_trigger_write() -> Result<(), Box<dyn Error>>
{
    for road in ROADS {
        let bench = Bench::new()?;
        let state = bench.app_state()?;
        let live = road.take(&state, bench.project.path()).map_err(|said| {
            format!(
                "{} refused the first run in an idle app: {said}",
                road.name()
            )
        })?;
        live.control.begin();

        let before = names_in(&bench.trigger_dir())?;
        let calls = Cell::new(0_usize);
        let polled = state.trigger_poll_permit().poll_with("mine", NOW, |_| {
            calls.set(calls.get() + 1);
            Ok(answer())
        })?;

        assert_eq!(
            polled,
            TriggerPoll::Busy,
            "a live {} did not make the Rust-owned poll return busy",
            road.name()
        );
        assert_eq!(
            calls.get(),
            0,
            "busy was reported only after the fetcher ran; this spends a request and can stage a \
             delivery while another run owns the only Stop handle"
        );
        assert_eq!(
            names_in(&bench.trigger_dir())?,
            before,
            "a busy poll changed the cursor or delivery ledger before refusing"
        );

        live.control.settle();
        let after = state
            .trigger_poll_permit()
            .poll_with("mine", NOW + 1, |_| {
                calls.set(calls.get() + 1);
                Ok(answer())
            })?;
        assert_eq!(
            after,
            TriggerPoll::Armed,
            "after {} settled, the next poll stayed latched instead of arming",
            road.name()
        );
        assert_eq!(
            calls.get(),
            1,
            "the first idle poll did not call its fetcher once"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_reserved_run_is_busy_before_its_first_line() -> Result<(), Box<dyn Error>> {
    for road in ROADS {
        let bench = Bench::new()?;
        let state = bench.app_state()?;
        let reserved = road.take(&state, bench.project.path()).map_err(|said| {
            format!(
                "{} refused the first run in an idle app: {said}",
                road.name()
            )
        })?;
        let calls = Cell::new(0_usize);

        let polled = state.trigger_poll_permit().poll_with("mine", NOW, |_| {
            calls.set(calls.get() + 1);
            Ok(answer())
        })?;
        assert_eq!(
            polled,
            TriggerPoll::Busy,
            "{} had already reserved AppState.live but the trigger slipped through before the \
             run's first line",
            road.name()
        );
        assert_eq!(calls.get(), 0, "the race-window poll reached the fetcher");
        reserved.control.settle();
    }
    Ok(())
}

#[tokio::test]
async fn busy_replays_a_durable_acceptance_but_never_hides_pending_work()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let state = bench.app_state()?;
    assert_eq!(
        state
            .trigger_poll_permit()
            .poll_with("mine", NOW, |_| Ok(answer()))?,
        TriggerPoll::Armed
    );
    let pending = state
        .trigger_poll_permit()
        .poll_with("mine", NOW + 1, |_| Ok(answer_at("issue-a", "LOAD-2", 9)))?;
    let TriggerPoll::Pending { delivery } = pending else {
        return Err(format!("the new issue did not become pending: {pending:?}").into());
    };

    let run_file = bench
        .project
        .path()
        .join(".loadout/runs/already-accepted/run.json");
    triggers::bind_delivery(bench.home.path(), &delivery.claim, &run_file)?;
    fs::create_dir_all(run_file.parent().ok_or("run.json has no parent")?)?;
    fs::write(
        &run_file,
        serde_json::to_vec_pretty(&json!({
            "id": delivery.claim.run_id,
            "created_at": delivery.created_at,
            "trigger_origin": {
                "slug": delivery.claim.slug,
                "delivery_id": delivery.claim.delivery_id,
                "issue_id": delivery.issue.id
            }
        }))?,
    )?;
    assert!(matches!(
        triggers::reconcile_delivery(
            bench.home.path(),
            &delivery.claim,
            read_and_sync_fixture_run,
        )?,
        triggers::DeliveryState::Accepted { .. }
    ));

    let live = state.begin_run(bench.project.path())?;
    live.control.begin();
    let before = snapshot(&bench.trigger_dir())?;
    let calls = Cell::new(0_usize);
    let receipt = state
        .trigger_poll_permit()
        .poll_with("mine", NOW + 2, |_| {
            calls.set(calls.get() + 1);
            Ok(answer_at("must-not-fetch", "LOAD-3", 10))
        })?;
    assert_eq!(
        receipt,
        TriggerPoll::Accepted {
            workflow: "ship-it".to_owned(),
            receipt_at: delivery.created_at,
        },
        "busy hid a receipt that was already durable before this run"
    );
    assert_eq!(calls.get(), 0, "busy acceptance reached the fetcher");
    assert_eq!(
        snapshot(&bench.trigger_dir())?,
        before,
        "busy acceptance rewrote trigger control files"
    );
    live.control.settle();

    let later = state
        .trigger_poll_permit()
        .poll_with("mine", NOW + 3, |_| Ok(answer_at("issue-b", "LOAD-4", 11)))?;
    assert!(
        matches!(
            later,
            TriggerPoll::Pending { ref delivery } if delivery.issue.id == "issue-b"
        ),
        "the accepted receipt blocked discovery of a later issue: {later:?}"
    );

    let live = state.begin_a_run(bench.project.path())?;
    live.control.begin();
    let before = snapshot(&bench.trigger_dir())?;
    let calls = Cell::new(0_usize);
    let busy = state
        .trigger_poll_permit()
        .poll_with("mine", NOW + 4, |_| {
            calls.set(calls.get() + 1);
            Ok(answer_at("must-not-fetch", "LOAD-5", 12))
        })?;
    assert_eq!(
        busy,
        TriggerPoll::Busy,
        "an old acceptance was shown while a newer delivery was still pending"
    );
    assert_eq!(calls.get(), 0, "busy with pending work reached the fetcher");
    assert_eq!(
        snapshot(&bench.trigger_dir())?,
        before,
        "busy with pending work changed the ledger or cursor"
    );
    live.control.settle();
    Ok(())
}

fn read_and_sync_fixture_run(path: &Path) -> io::Result<Option<Vec<u8>>> {
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut raw = Vec::new();
    file.read_to_end(&mut raw)?;
    file.sync_all()?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("fixture run has no parent"))?;
    fs::File::open(parent)?.sync_all()?;
    Ok(Some(raw))
}

fn answer() -> Vec<u8> {
    answer_at("old", "LOAD-1", 8)
}

fn answer_at(id: &str, identifier: &str, hour: u8) -> Vec<u8> {
    serde_json::to_vec(&json!({"data":{"issues":{"nodes":[{
        "id":id, "identifier":identifier, "title":format!("Issue {identifier}"),
        "url":format!("https://linear.app/loadout/issue/{identifier}"), "description":null,
        "updatedAt":format!("2026-08-21T{hour:02}:00:00.000Z")
    }]}}}))
    .expect("answer JSON")
}

type DirectorySnapshot = Vec<(String, Vec<u8>)>;

fn snapshot(dir: &Path) -> Result<DirectorySnapshot, Box<dyn Error>> {
    let mut files = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            entry
                .file_type()
                .ok()?
                .is_file()
                .then(|| (name, entry.path()))
        })
        .map(|(name, path)| fs::read(path).map(|bytes| (name, bytes)))
        .collect::<Result<Vec<_>, _>>()?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(files)
}

fn names_in(dir: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let mut names = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<_>>();
    names.sort();
    Ok(names)
}

#[derive(Debug)]
struct Bench {
    home: TempDir,
    project: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join(triggers::TRIGGERS_DIR))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(
            home.path().join(triggers::TRIGGERS_DIR).join("mine.json"),
            serde_json::to_vec_pretty(&json!({
                "schema": 1, "source": "linear", "enabled": true,
                "workflow": "ship-it", "condition": "assigned to me", "api_key": KEY
            }))?,
        )?;
        Ok(Self { home, project })
    }

    fn trigger_dir(&self) -> PathBuf {
        self.home.path().join(triggers::TRIGGERS_DIR)
    }

    fn app_state(&self) -> Result<AppState, Box<dyn Error>> {
        let store = Store::open(&self.project.path().join(".loadout/loadout.db"))?;
        Ok(AppState::new(
            self.home.path().to_path_buf(),
            self.project.path().to_path_buf(),
            store,
            no_agents_needed(),
        ))
    }
}

fn no_agents_needed() -> Drivers {
    let absent: Arc<dyn AgentDriver> = Arc::new(Absent::new("nobody", "T-65 AC-4"));
    Arc::new(move |_vendor| Arc::clone(&absent))
}
