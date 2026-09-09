#![allow(clippy::expect_used)]
//! WP-08: zapis materializuje odziedziczone Use, a panel nazywa jego źródło.
//!
//! `expect` jest celowe wyłącznie w tym module testowym: pomocniki rozpakowują stałą fiksturę,
//! a każda utrata kroku ma zatrzymać test w miejscu, w którym sama fikstura przestała być prawdą.

use std::error::Error;
use std::fs;

use loadout_lib::commands::workflow_plan::resolve_workflow_plan_inner;
use loadout_lib::commands::workflows::{load_workflow_inner, save_workflow_inner};
use loadout_lib::workflow::file::{CURRENT, PLAN_FORMAT};
use loadout_lib::workflow::{Link, Step, WorkflowFile};
use serde_json::{Value, json};

const FILE: &str = "work-plan-use-is-inherited.json";

fn agent(id: &str, name: &str, plan: Option<Value>) -> Value {
    let mut step = json!({
        "kind": "agent",
        "id": id,
        "name": name,
        "agent": "worker",
        "instructions": "Do this work.",
        "folder": { "use": "fresh-copy" }
    });
    if let Some(plan) = plan {
        step["plan"] = plan;
    }
    step
}

fn check(id: &str, name: &str) -> Value {
    json!({
        "kind": "check",
        "id": id,
        "name": name,
        "command": "cargo test --test it work_plan_use_is_inherited::",
        "proof": "(\\d+) passed",
        "folder": { "use": "fresh-copy" }
    })
}

fn link(from: &str, to: &str) -> Value {
    json!({ "from": from, "to": to })
}

fn workflow(steps: &[Value], links: &[Value]) -> WorkflowFile {
    serde_json::from_value(json!({
        "format": CURRENT,
        "id": "work-plan-use-is-inherited",
        "name": "Work plan inheritance",
        "steps": steps,
        "links": links
    }))
    .expect("the test workflow is valid")
}

fn inheritance_scene() -> WorkflowFile {
    workflow(
        &[
            agent("prepare", "Prepare", None),
            agent("planner", "Planner", Some(json!({ "mode": "create" }))),
            agent("backend", "Backend", None),
            check("gate", "Run the checks"),
            agent("far", "Far", None),
            agent("aside", "Aside", None),
        ],
        &[
            link("prepare", "planner"),
            link("planner", "backend"),
            link("backend", "gate"),
            link("gate", "far"),
        ],
    )
}

fn read_saved(home: &std::path::Path, file: &str) -> Result<Value, Box<dyn Error>> {
    let bytes = fs::read(home.join("workflows").join(file))?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn saved_step<'a>(file: &'a Value, id: &str) -> &'a Value {
    file["steps"]
        .as_array()
        .and_then(|steps| steps.iter().find(|step| step["id"] == id))
        .expect("the saved workflow kept the named step")
}

fn set_plan(file: &mut WorkflowFile, id: &str, plan: Value) {
    let agent = file
        .steps
        .iter_mut()
        .find(|step| step.id() == id)
        .and_then(|step| match step {
            Step::Agent(agent) => Some(agent),
            Step::Checkpoint(_) | Step::Check(_) | Step::Serve(_) => None,
        })
        .expect("the test workflow kept the named agent step");
    agent.extra.insert("plan".to_owned(), plan);
}

fn count_mode(file: &Value, mode: &str) -> usize {
    file["steps"].as_array().map_or(0, |steps| {
        steps
            .iter()
            .filter(|step| step["plan"]["mode"] == mode)
            .count()
    })
}

#[test]
fn an_unset_step_after_the_author_is_written_as_use_in_the_saved_file() -> Result<(), Box<dyn Error>>
{
    let home = tempfile::tempdir()?;
    save_workflow_inner(home.path(), None, FILE, inheritance_scene(), None)?;

    let saved = read_saved(home.path(), FILE)?;
    let inherited = json!({ "mode": "use", "inherited": true });
    for id in ["backend", "far"] {
        assert_eq!(saved_step(&saved, id).get("plan"), Some(&inherited));
    }
    for id in ["prepare", "aside", "gate"] {
        assert!(saved_step(&saved, id).get("plan").is_none());
    }
    Ok(())
}

#[test]
fn a_step_switched_off_by_hand_stays_off_after_the_next_save_and_the_next_author()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let first = save_workflow_inner(home.path(), None, FILE, inheritance_scene(), None)?;
    let mut edited = load_workflow_inner(home.path(), None, FILE)?.workflow;
    set_plan(&mut edited, "backend", json!({ "mode": "off" }));

    let second = save_workflow_inner(home.path(), None, FILE, &edited, Some(&first.revision))?;
    assert_eq!(
        saved_step(&read_saved(home.path(), FILE)?, "backend").get("plan"),
        Some(&json!({ "mode": "off" }))
    );

    set_plan(&mut edited, "aside", json!({ "mode": "create" }));
    edited.links.push(Link {
        from: "aside".to_owned(),
        to: "backend".to_owned(),
        max_turns: None,
    });
    save_workflow_inner(home.path(), None, FILE, &edited, Some(&second.revision))?;
    assert_eq!(
        saved_step(&read_saved(home.path(), FILE)?, "backend").get("plan"),
        Some(&json!({ "mode": "off" }))
    );
    Ok(())
}

#[test]
fn a_workflow_with_no_author_saves_exactly_as_it_came_in() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let file = workflow(&[agent("worker", "Worker", None)], &[]);
    let before = serde_json::to_value(&file)?;

    save_workflow_inner(home.path(), None, FILE, &file, None)?;
    let saved = read_saved(home.path(), FILE)?;

    assert_eq!(saved["steps"], before["steps"]);
    assert_eq!(saved["format"], CURRENT);
    assert!(saved_step(&saved, "worker").get("plan").is_none());
    Ok(())
}

#[test]
fn the_saved_format_and_a_duplicate_keep_what_wp_02_settled() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    save_workflow_inner(home.path(), None, FILE, inheritance_scene(), None)?;
    let saved = read_saved(home.path(), FILE)?;
    assert_eq!(saved["format"], PLAN_FORMAT);
    assert_eq!(count_mode(&saved, "create"), 1);

    let mut duplicate = load_workflow_inner(home.path(), None, FILE)?.workflow;
    duplicate.id = "work-plan-use-is-inherited-copy".to_owned();
    duplicate.name = "Work plan inheritance (copy)".to_owned();
    save_workflow_inner(home.path(), None, "copy.json", &duplicate, None)?;
    let copied = read_saved(home.path(), "copy.json")?;
    assert_eq!(copied["format"], PLAN_FORMAT);
    assert_eq!(count_mode(&copied, "create"), 1);
    Ok(())
}

#[test]
fn the_panel_says_the_use_was_inherited_and_names_the_step() {
    let file = workflow(
        &[
            agent("planner", "Planner", Some(json!({ "mode": "create" }))),
            agent("worker", "Worker", None),
        ],
        &[link("planner", "worker")],
    );

    let view = resolve_workflow_plan_inner(&file);
    let worker = view
        .steps
        .iter()
        .find(|step| step.step_id == "worker")
        .expect("the panel kept the worker step");
    assert_eq!(worker.mode, loadout_lib::work_plan::Mode::Use);
    assert_eq!(
        worker.source.as_ref().map(|source| source.said.as_str()),
        Some("Inherited from Planner.")
    );
}
