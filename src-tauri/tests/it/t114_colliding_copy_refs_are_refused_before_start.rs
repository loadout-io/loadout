//! AC-2 for T-114: two planned fresh copies may never collapse onto one Git ref.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::error::Error;
use std::sync::Arc;

use loadout_lib::commands::RunRequest;
use loadout_lib::commands::workflows::check_workflow_inner;
use loadout_lib::store::Store;
use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::check::{Level, check_to_run};

use super::t114_copies_get_noncolliding_git_branches::support::{Rig, Spy, run};

const REFUSAL: &str = "\"Build twice\" and \"Literal suffix\" would use the same work branch \"s_2-2\". Rename one of them before starting.";

const COLLISION: &str = r#"{
  "format": 1, "id": "wf_t114_ref_collision", "name": "A ref collision",
  "steps": [
    { "kind": "agent", "id": "s_2", "name": "Build twice",
      "agent": "01990000-0000-7000-8000-000000001114", "overrides": {}, "copies": 2,
      "instructions": "build copy {{copy}} of {{copies}}", "folder": { "use": "fresh-copy" } },
    { "kind": "agent", "id": "s_2-2", "name": "Literal suffix",
      "agent": "01990000-0000-7000-8000-000000001114", "overrides": {},
      "instructions": "literal", "folder": { "use": "fresh-copy" } }
  ],
  "links": []
}"#;

fn parsed(text: &str) -> WorkflowFile {
    serde_json::from_str(text).expect("the T-114 collision fixture is valid JSON")
}

fn named(notes: &[loadout_lib::workflow::check::Note]) -> Vec<(Level, String)> {
    notes
        .iter()
        .filter(|note| note.message.contains("same work branch"))
        .map(|note| (note.level, note.message.clone()))
        .collect()
}

#[test]
fn the_window_warns_once_and_start_has_the_same_problem_once() {
    let rig = Rig::plain().expect("the fixture has a private library");
    let workflow = parsed(COLLISION);

    assert_eq!(
        named(&check_workflow_inner(rig.home.path(), &workflow)),
        [(Level::Warning, REFUSAL.to_owned())]
    );
    assert_eq!(
        named(&check_to_run(&workflow)),
        [(Level::Problem, REFUSAL.to_owned())]
    );
}

#[test]
fn only_fresh_copy_refs_reserve_the_encoded_tail() {
    let noncolliding = COLLISION.replace(
        "\"s_2-2\", \"name\": \"Literal suffix\"",
        "\"s_other\", \"name\": \"Literal suffix\"",
    );
    assert!(named(&check_to_run(&parsed(&noncolliding))).is_empty());

    let project_folder = COLLISION.replacen(
        "\"instructions\": \"literal\", \"folder\": { \"use\": \"fresh-copy\" }",
        "\"instructions\": \"literal\"",
        1,
    );
    assert!(named(&check_to_run(&parsed(&project_folder))).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn start_refuses_before_any_run_artifact_or_driver_call() -> Result<(), Box<dyn Error>> {
    let rig = Rig::git()?;
    let workflow = rig.workflow("ref-collision", COLLISION)?;
    let store = Store::open(&rig.db())?;
    let spy = Arc::new(Spy::answering(|_| String::new()));
    let branches_before = loadout_lib::commands::isolate::branches_under(rig.project(), "loadout/");
    let trees_before = loadout_lib::commands::isolate::branches_in_use(rig.project());
    let request = RunRequest {
        workflow,
        how_many_at_once: 3,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let result = run(&rig, &store, Arc::clone(&spy), request).await?;
    assert_eq!(spy.count(), 0, "a colliding workflow reached the driver");
    assert!(
        rig.run_dirs().is_empty(),
        "a refused Start created a run directory"
    );
    assert_eq!(
        loadout_lib::commands::isolate::branches_under(rig.project(), "loadout/"),
        branches_before,
        "a refused Start created a branch"
    );
    assert_eq!(
        loadout_lib::commands::isolate::branches_in_use(rig.project()),
        trees_before,
        "a refused Start created a worktree"
    );
    let Err(loadout_lib::commands::RunError::Refused(note)) = result else {
        return Err("Start did not return the validator's named refusal".into());
    };
    assert_eq!(
        (note.level, note.message.as_str()),
        (Level::Problem, REFUSAL)
    );
    Ok(())
}
