//! Z-20: płótno sądzi umiejętność po miejscu, w którym krok naprawdę pracuje.
//!
//! Testy idą przez `check_workflow_inner`, czyli tę samą drogę, którą okno zasila kropkę na
//! kafelku i listę rzeczy do naprawienia. Sama odpowiedź pomocniczej funkcji nie dowodziłaby,
//! że człowiek zobaczy właściwy stan (niezmiennik 29).

use std::error::Error;
use std::fs;

use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::commands::workflows::check_workflow_inner;
use loadout_lib::library::agents::Agent;
use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::check::{Level, Note};
use loadout_lib::workflow::roster::Fix;
use serde_json::{Value, json};
use tempfile::TempDir;

const SKILL: &str = "playwright-cli";

struct Library {
    home: TempDir,
    builder: Agent,
    design_qa: Agent,
}

fn library() -> Result<Library, Box<dyn Error>> {
    let home = TempDir::new()?;
    let mut builder = Agent::example();
    builder.id = uuid::Uuid::now_v7();
    "Builder".clone_into(&mut builder.name);

    let mut design_qa = Agent::example();
    design_qa.id = uuid::Uuid::now_v7();
    "Design QA".clone_into(&mut design_qa.name);
    design_qa.skills = vec![SKILL.to_owned()];

    save_agent_inner(home.path(), &builder, None)?;
    save_agent_inner(home.path(), &design_qa, None)?;
    fs::create_dir_all(home.path().join("skills").join(SKILL))?;

    Ok(Library {
        home,
        builder,
        design_qa,
    })
}

fn step(id: &str, name: &str, agent: &Agent, folder: &Value) -> Value {
    json!({
        "kind": "agent",
        "id": id,
        "name": name,
        "agent": agent.id.to_string(),
        "overrides": {},
        "instructions": "Do the work.",
        "folder": folder
    })
}

fn fresh_copy() -> Value {
    json!({ "use": "fresh-copy" })
}

fn project() -> Value {
    json!({ "use": "project" })
}

fn same_copy() -> Value {
    json!({ "use": "same-copy" })
}

fn workflow(steps: &[Value], links: &[(&str, &str)]) -> Result<WorkflowFile, Box<dyn Error>> {
    let links: Vec<Value> = links
        .iter()
        .map(|(from, to)| json!({ "from": from, "to": to }))
        .collect();
    Ok(serde_json::from_value(json!({
        "format": 1,
        "id": "wf-skill-rides-copy",
        "name": "Skill rides copy",
        "steps": steps,
        "links": links
    }))?)
}

fn problems_for<'a>(notes: &'a [Note], step_id: &str) -> Vec<&'a Note> {
    notes
        .iter()
        .filter(|note| note.level == Level::Problem && note.step_id.as_deref() == Some(step_id))
        .collect()
}

#[test]
fn a_step_in_the_copy_before_it_keeps_its_skill() -> Result<(), Box<dyn Error>> {
    let library = library()?;
    let file = workflow(
        &[
            step("s_front", "Front", &library.builder, &fresh_copy()),
            step("s_figma", "Figma check", &library.design_qa, &same_copy()),
        ],
        &[("s_front", "s_figma")],
    )?;

    let notes = check_workflow_inner(library.home.path(), &file);
    assert!(
        problems_for(&notes, "s_figma").is_empty(),
        "\"Figma check\" works in the fresh copy made by \"Front\", so Loadout owns the \
         folder where it places the skill. The canvas refused it: {notes:?}"
    );
    Ok(())
}

#[test]
fn a_step_joining_two_copies_keeps_its_skill() -> Result<(), Box<dyn Error>> {
    let library = library()?;
    let file = workflow(
        &[
            step("s_front", "Front", &library.builder, &fresh_copy()),
            step("s_back", "Back", &library.builder, &fresh_copy()),
            step("s_figma", "Figma check", &library.design_qa, &same_copy()),
        ],
        &[("s_front", "s_figma"), ("s_back", "s_figma")],
    )?;

    let notes = check_workflow_inner(library.home.path(), &file);
    assert!(
        problems_for(&notes, "s_figma").is_empty(),
        "a step joining two copies gets a copy of its own, so Loadout has a place for its \
         skill. The canvas refused it: {notes:?}"
    );
    Ok(())
}

#[test]
fn a_step_with_nothing_before_it_is_told_one_thing() -> Result<(), Box<dyn Error>> {
    let library = library()?;
    let file = workflow(
        &[
            step("s_figma", "Figma check", &library.design_qa, &same_copy()),
            step("s_tail", "Finish", &library.builder, &fresh_copy()),
        ],
        &[("s_figma", "s_tail")],
    )?;

    let notes = check_workflow_inner(library.home.path(), &file);
    let problems = problems_for(&notes, "s_figma");
    assert_eq!(
        problems.len(),
        1,
        "one unresolved folder is one thing to fix on the canvas. Got: {notes:?}"
    );
    assert!(
        problems[0].message.contains("nothing comes before it"),
        "the one visible sentence should explain why the folder cannot be found. Got: {notes:?}"
    );
    Ok(())
}

#[test]
fn a_step_in_your_own_folder_still_has_nowhere_to_put_a_skill() -> Result<(), Box<dyn Error>> {
    let library = library()?;
    let file = workflow(
        &[
            step("s_front", "Front", &library.builder, &project()),
            step("s_figma", "Figma check", &library.design_qa, &same_copy()),
        ],
        &[("s_front", "s_figma")],
    )?;

    let notes = check_workflow_inner(library.home.path(), &file);
    let problems = problems_for(&notes, "s_figma");
    assert_eq!(
        problems.len(),
        1,
        "a skill still cannot be placed in the folder the person is editing. Got: {notes:?}"
    );
    assert_eq!(
        problems[0].message,
        "\"Figma check\" uses the skill \"playwright-cli\", and it works straight inside your \
         project folder. Loadout writes nothing into a folder of yours, so it has nowhere to put \
         the skill. Give the step its own copy of your files, or take the skill off it."
    );
    assert_eq!(
        problems[0].fix.as_deref(),
        Some(&Fix::GiveItAFreshCopy {
            step: "s_figma".to_owned(),
        }),
        "the visible repair should still offer a fresh copy for the refused step"
    );
    Ok(())
}
