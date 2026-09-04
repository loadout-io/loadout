//! Z-42: folder, którego Loadout nie może czytać, odmawia przed biegiem i mówi to w historii.
//!
//! Oba kryteria idą drogą okna (niezmiennik 29): Start przez `run_workflow_from_window`, a
//! historia przez `read_run_inner`. Sama funkcja sprawdzająca prawa nie dowodziłaby, że człowiek
//! zobaczy jej zdanie.

#![allow(clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use loadout_lib::commands::Drivers;
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::engine::drivers::AgentDriver;
use loadout_lib::ipc::{AppState, line_channel, run_workflow_from_window};
use loadout_lib::store::Store;

const CANNOT_READ: &str = concat!(
    "Loadout can't read this folder. Allow it under System Settings › Privacy & Security › ",
    "Files and Folders."
);
const RUN: &str = "20260904-120000__0199f066-2e00-7000-8000-000000000042";
const RUN_JSON: &str = r#"{
  "id": "0199f066-2e00-7000-8000-000000000042",
  "title": "Permission witness",
  "status": "succeeded",
  "steps": []
}"#;

struct ModeGuard {
    path: PathBuf,
    old_mode: u32,
}

impl ModeGuard {
    fn set(path: &Path, mode: u32) -> std::io::Result<Self> {
        let old_mode = fs::metadata(path)?.permissions().mode() & 0o777;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
        Ok(Self {
            path: path.to_owned(),
            old_mode,
        })
    }
}

impl Drop for ModeGuard {
    fn drop(&mut self) {
        // 2026-09 (Z-42) — bez przywrócenia prawa `TempDir` nie posprząta fikstury i każdy
        // bieg testu zostawi następny nieusuwalny katalog (audyt C-3).
        let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(self.old_mode));
    }
}

#[tokio::test]
async fn a_folder_it_cannot_read_is_turned_down_before_the_run_starts() -> Result<(), Box<dyn Error>>
{
    let root = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    let project = root.path().join("cannot-read");
    fs::create_dir(&project)?;
    let state = AppState::new(
        home.path().to_path_buf(),
        project.clone(),
        Store::open(&home.path().join("index.db"))?,
        no_drivers(),
    );

    let sealed = ModeGuard::set(&project, 0o000)?;
    let (lines, _source) = line_channel(8);
    let refusal = run_workflow_from_window(
        &state,
        "missing.json",
        1,
        project.to_str(),
        None,
        None,
        lines,
    )
    .await
    .expect_err("Start entered a folder the application cannot read");
    assert_eq!(
        refusal, CANNOT_READ,
        "Start did not give the person the one action that makes this folder readable"
    );

    drop(sealed);
    let (lines, _source) = line_channel(8);
    let allowed = run_workflow_from_window(
        &state,
        "missing.json",
        1,
        project.to_str(),
        None,
        None,
        lines,
    )
    .await
    .expect_err("the deliberately missing workflow unexpectedly started");
    assert_ne!(
        allowed, CANNOT_READ,
        "the same folder still got the access refusal after its permissions were restored"
    );
    Ok(())
}

#[test]
fn a_run_whose_handoffs_cannot_be_read_says_so_in_its_history() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let run = project.path().join(".loadout").join("runs").join(RUN);
    let handoffs = run.join("handoffs");
    fs::create_dir_all(&handoffs)?;
    fs::write(run.join("run.json"), RUN_JSON)?;
    let _sealed = ModeGuard::set(&handoffs, 0o000)?;

    let opened = serde_json::to_value(read_run_inner(project.path(), RUN)?)?;
    assert_eq!(
        opened
            .pointer("/handoffsSaid")
            .and_then(serde_json::Value::as_str),
        Some(CANNOT_READ),
        "the history screen received an empty handoff list without the reason it was empty"
    );
    Ok(())
}

fn no_drivers() -> Drivers {
    Arc::new(|_vendor| -> Arc<dyn AgentDriver> {
        unreachable!("an access refusal must happen before any agent is chosen")
    })
}
