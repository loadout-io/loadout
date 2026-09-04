//! Regresja Z-43: zajętość triggera należy do folderu zapisanego w jego pliku, nie do aplikacji.

#![allow(clippy::expect_used)]

use std::cell::Cell;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use loadout_lib::commands::Drivers;
use loadout_lib::commands::triggers::{self, TriggerPoll};
use loadout_lib::commands::workspaces;
use loadout_lib::engine::drivers::AgentDriver;
use loadout_lib::engine::drivers::absent::Absent;
use loadout_lib::ipc::AppState;
use loadout_lib::store::Store;
use serde_json::json;
use tempfile::TempDir;

const NOW: i64 = 1_777_777_777_000;
const POLL_INTERVAL: Duration = Duration::from_mins(1);
const KEY: &str = "lin_api_1234567890123456789012345678901234567890";

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_run_in_another_folder_does_not_hold_this_trigger() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let state = bench.app_state()?;
    let live = state.begin_run(&bench.workspace_a)?;
    live.control.begin();

    // 2026-09: zegar stoi, żeby test dowodził dwóch pełnych odstępów triggera, podczas gdy
    // uchwyt wolnego kroku w A nadal nie ma dowodu zejścia (niezmienniki 11 i 19).
    let slow_step = tokio::time::sleep(POLL_INTERVAL * 3);
    tokio::pin!(slow_step);
    let calls = Cell::new(0_usize);
    let first = state.trigger_poll_permit().poll_with("in-b", NOW, |_| {
        calls.set(calls.get() + 1);
        Ok(answer())
    })?;
    tokio::time::advance(POLL_INTERVAL).await;
    let second = state
        .trigger_poll_permit()
        .poll_with("in-b", NOW + 60_000, |_| {
            calls.set(calls.get() + 1);
            Ok(answer())
        })?;

    assert_eq!([first, second], [TriggerPoll::Armed, TriggerPoll::Armed]);
    assert_eq!(
        calls.get(),
        2,
        "the trigger in B did not poll once per interval"
    );

    tokio::time::advance(POLL_INTERVAL * 2).await;
    slow_step.await;
    live.control.settle();
    Ok(())
}

#[tokio::test]
async fn a_run_in_this_folder_says_which_folder_it_is() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let state = bench.app_state()?;
    let live = state.begin_run(&bench.workspace_a)?;
    live.control.begin();
    let calls = Cell::new(0_usize);

    let polled = state.trigger_poll_permit().poll_with("in-a", NOW, |_| {
        calls.set(calls.get() + 1);
        Ok(answer())
    })?;

    assert_waiting_in(polled, "Workspace A")?;
    assert_eq!(calls.get(), 0, "a waiting trigger still reached Linear");
    live.control.settle();
    Ok(())
}

#[tokio::test]
async fn two_live_folders_hold_only_their_own_triggers() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let state = bench.app_state()?;
    let live_a = state.begin_run(&bench.workspace_a)?;
    let live_b = state.begin_run(&bench.workspace_b)?;
    live_a.control.begin();
    live_b.control.begin();

    assert_waiting_in(
        state
            .trigger_poll_permit()
            .poll_with("in-a", NOW, |_| Ok(answer()))?,
        "Workspace A",
    )?;
    assert_waiting_in(
        state
            .trigger_poll_permit()
            .poll_with("in-b", NOW, |_| Ok(answer()))?,
        "Workspace B",
    )?;

    live_a.control.settle();
    live_b.control.settle();
    Ok(())
}

#[tokio::test]
async fn run_again_names_the_busy_folder() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let state = bench.app_state()?;
    let live = state.begin_run(&bench.workspace_b)?;
    live.control.begin();

    let refused = state.trigger_poll_permit().retry("in-b", NOW);

    assert!(
        refused.as_ref().is_err_and(|sentence| {
            sentence.contains("Workspace B") && sentence.contains("Press Stop")
        }),
        "Run again did not name its busy folder: {refused:?}"
    );
    live.control.settle();
    Ok(())
}

#[tokio::test]
async fn a_trigger_without_a_folder_is_never_held() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let state = bench.app_state()?;
    let live_a = state.begin_run(&bench.workspace_a)?;
    let live_b = state.begin_run(&bench.workspace_b)?;
    live_a.control.begin();
    live_b.control.begin();
    let calls = Cell::new(0_usize);

    let refused = state
        .trigger_poll_permit()
        .poll_with("without-workspace", NOW, |_| {
            calls.set(calls.get() + 1);
            Ok(answer())
        });

    assert!(
        refused
            .as_ref()
            .is_err_and(|sentence| sentence == "Choose a workspace before saving this trigger."),
        "a legacy trigger hid its missing workspace behind busy: {refused:?}"
    );
    assert_eq!(
        calls.get(),
        0,
        "a trigger without a workspace reached Linear"
    );
    live_a.control.settle();
    live_b.control.settle();
    Ok(())
}

fn assert_waiting_in(poll: TriggerPoll, name: &str) -> Result<(), Box<dyn Error>> {
    let TriggerPoll::Busy { sentence } = poll else {
        return Err(format!("a live run did not hold its own trigger: {poll:?}").into());
    };
    assert!(
        sentence.starts_with("Waiting — a run is going in") && sentence.contains(name),
        "the waiting sentence did not name {name}: {sentence}"
    );
    Ok(())
}

fn answer() -> Vec<u8> {
    serde_json::to_vec(&json!({"data":{"issues":{"nodes":[]}}})).expect("empty Linear answer JSON")
}

struct Bench {
    home: TempDir,
    workspace_a: PathBuf,
    workspace_b: PathBuf,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let workspace_a = home.path().join("workspace-a");
        let workspace_b = home.path().join("workspace-b");
        fs::create_dir_all(workspace_a.join(".loadout"))?;
        fs::create_dir_all(&workspace_b)?;
        fs::create_dir_all(home.path().join(triggers::TRIGGERS_DIR))?;
        workspaces::save_workspace_inner(home.path(), "Workspace A", path_text(&workspace_a)?)?;
        workspaces::save_workspace_inner(home.path(), "Workspace B", path_text(&workspace_b)?)?;
        write_trigger(home.path(), "in-a", Some(path_text(&workspace_a)?))?;
        write_trigger(home.path(), "in-b", Some(path_text(&workspace_b)?))?;
        write_trigger(home.path(), "without-workspace", None)?;
        Ok(Self {
            home,
            workspace_a,
            workspace_b,
        })
    }

    fn app_state(&self) -> Result<AppState, Box<dyn Error>> {
        let store = Store::open(&self.workspace_a.join(".loadout/loadout.db"))?;
        Ok(AppState::new(
            self.home.path().to_path_buf(),
            self.workspace_a.clone(),
            store,
            no_agents_needed(),
        ))
    }
}

fn write_trigger(home: &Path, slug: &str, workspace: Option<&str>) -> Result<(), Box<dyn Error>> {
    let mut trigger = json!({
        "schema": 1,
        "source": "linear",
        "enabled": true,
        "workflow": "ship-it",
        "condition": "assigned-to-me",
        "api_key": KEY
    });
    if let Some(workspace) = workspace {
        trigger["workspace"] = json!(workspace);
    }
    fs::write(
        home.join(triggers::TRIGGERS_DIR)
            .join(format!("{slug}.json")),
        serde_json::to_vec_pretty(&trigger)?,
    )?;
    Ok(())
}

fn path_text(path: &Path) -> Result<&str, Box<dyn Error>> {
    path.to_str().ok_or_else(|| "test path is not UTF-8".into())
}

fn no_agents_needed() -> Drivers {
    let absent: Arc<dyn AgentDriver> = Arc::new(Absent::new("nobody", "Z-43"));
    Arc::new(move |_vendor| Arc::clone(&absent))
}
