//! AC-6 for T-114: the source of a back edge is the judge and must have one copy.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::error::Error;
use std::sync::Arc;

use loadout_lib::commands::RunRequest;
use loadout_lib::commands::workflows::{check_workflow_inner, save_workflow_inner};
use loadout_lib::store::Store;
use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::check::{Level, Note, check_to_run};

use super::t114_copies_get_noncolliding_git_branches::support::{Rig, Spy, run};

const REFUSAL: &str = "\"Judge\" closes a loop, so it can only run once at a time.";

const LOOP: &str = r#"{
  "format": 1, "id": "wf_t114_loop_judge", "name": "A loop with one judge",
  "steps": [
    { "kind": "agent", "id": "s_work", "name": "Work",
      "agent": "01990000-0000-7000-8000-000000001114", "overrides": {},
      "instructions": "work", "folder": { "use": "fresh-copy" } },
    { "kind": "agent", "id": "s_judge", "name": "Judge",
      "agent": "01990000-0000-7000-8000-000000001114", "overrides": {}, "copies": 2,
      "instructions": "judge", "folder": { "use": "fresh-copy" } }
  ],
  "links": [
    { "from": "s_work", "to": "s_judge" },
    { "from": "s_judge", "to": "s_work", "max_turns": 2 }
  ]
}"#;

fn parsed(text: &str) -> WorkflowFile {
    serde_json::from_str(text).expect("the T-114 loop fixture is valid JSON")
}

fn judge_notes(notes: &[Note]) -> Vec<(Level, String)> {
    notes
        .iter()
        .filter(|note| note.message.contains("closes a loop"))
        .map(|note| (note.level, note.message.clone()))
        .collect()
}

fn problems(workflow: &WorkflowFile) -> Vec<String> {
    check_to_run(workflow)
        .into_iter()
        .filter(|note| note.level == Level::Problem)
        .map(|note| note.message)
        .collect()
}

#[test]
fn the_window_save_and_start_checks_name_the_judge_once() -> Result<(), Box<dyn Error>> {
    let rig = Rig::plain()?;
    let workflow = parsed(LOOP);
    assert_eq!(
        judge_notes(&check_workflow_inner(rig.home.path(), &workflow)),
        [(Level::Problem, REFUSAL.to_owned())]
    );
    assert_eq!(
        judge_notes(&check_to_run(&workflow)),
        [(Level::Problem, REFUSAL.to_owned())]
    );
    let two_returns = LOOP.replace(
        "    { \"from\": \"s_judge\", \"to\": \"s_work\", \"max_turns\": 2 }",
        "    { \"from\": \"s_judge\", \"to\": \"s_work\", \"max_turns\": 2 },\n    { \"from\": \"s_judge\", \"to\": \"s_work\", \"max_turns\": 3 }",
    );
    assert_eq!(
        judge_notes(&check_to_run(&parsed(&two_returns))),
        [(Level::Problem, REFUSAL.to_owned())],
        "two ways back from the same judge must not duplicate the judge sentence"
    );

    let refusal = save_workflow_inner(rig.home.path(), "judge.json", &workflow, None)
        .expect_err("a loop judge with two copies was saved");
    let loadout_lib::workflow::file::SaveError::Refused(note) = refusal else {
        return Err("the save failed for something other than the named judge rule".into());
    };
    assert_eq!(note.message, REFUSAL);
    Ok(())
}

#[test]
fn copies_on_the_target_and_on_an_ordinary_step_remain_legal() {
    let target_copies = LOOP
        .replacen(
            "\"instructions\": \"work\"",
            "\"copies\": 2, \"instructions\": \"work\"",
            1,
        )
        .replacen(
            "\"overrides\": {}, \"copies\": 2,\n      \"instructions\": \"judge\"",
            "\"overrides\": {}, \"copies\": 1,\n      \"instructions\": \"judge\"",
            1,
        );
    assert!(problems(&parsed(&target_copies)).is_empty());

    let ordinary = target_copies.replace(
        "  ],\n  \"links\"",
        "    ,{ \"kind\": \"agent\", \"id\": \"s_other\", \"name\": \"Other\", \"agent\": \"01990000-0000-7000-8000-000000001114\", \"overrides\": {}, \"copies\": 2, \"instructions\": \"other\", \"folder\": { \"use\": \"fresh-copy\" } }\n  ],\n  \"links\"",
    );
    assert!(problems(&parsed(&ordinary)).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn start_refuses_the_judge_before_the_first_driver_call() -> Result<(), Box<dyn Error>> {
    let rig = Rig::git()?;
    let workflow = rig.workflow("loop-judge", LOOP)?;
    let store = Store::open(&rig.db())?;
    let spy = Arc::new(Spy::answering(|_| "outcome: pass\n".to_owned()));
    let result = run(
        &rig,
        &store,
        Arc::clone(&spy),
        RunRequest {
            workflow,
            how_many_at_once: 3,
            task: None,
            part: None,
            handoffs_from: None,
        },
    )
    .await?;

    assert_eq!(
        spy.count(),
        0,
        "the loop judge rule ran a driver before refusing"
    );
    assert!(
        rig.run_dirs().is_empty(),
        "the loop judge rule created a run directory"
    );
    let Err(loadout_lib::commands::RunError::Refused(note)) = result else {
        return Err("Start did not return the loop judge's validator note".into());
    };
    assert_eq!(
        (note.level, note.message.as_str()),
        (Level::Problem, REFUSAL)
    );
    Ok(())
}
