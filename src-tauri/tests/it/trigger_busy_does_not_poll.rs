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

fn answer() -> Vec<u8> {
    serde_json::to_vec(&json!({"data":{"issues":{"nodes":[{
        "id":"old", "identifier":"LOAD-1", "title":"Existing backlog",
        "url":"https://linear.app/loadout/issue/LOAD-1", "description":null,
        "updatedAt":"2026-08-21T08:00:00.000Z"
    }]}}}))
    .expect("answer JSON")
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
