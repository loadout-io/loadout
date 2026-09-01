//! T-203 AC-1: jeden wadliwy plik zostaje jednym wpisem problemu, a zdrowa biblioteka działa.
//!
//! Test przechodzi przez produkcyjne spacery obu półek. Nie czyta prywatnego katalogu operatora
//! ani nie testuje helpera w oderwaniu od komend, które zasilają IPC.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::commands::agents::{list_agent_definitions_inner, save_agent_inner};
use loadout_lib::commands::workflows::{list_workflow_definitions_inner, save_workflow_inner};
use loadout_lib::library::agents::Agent;
use loadout_lib::library::definition::{Definition, Shelf};
use loadout_lib::workflow::WorkflowFile;
use serde_json::json;
use tempfile::TempDir;

fn agent(name: &str) -> Agent {
    Agent {
        id: uuid::Uuid::now_v7(),
        name: name.to_owned(),
        summary: format!("{name} stays usable."),
        instructions: format!("Keep {name} healthy.\n"),
        ..Agent::example()
    }
}

fn workflow(id: &str, name: &str) -> WorkflowFile {
    serde_json::from_value(json!({
        "format": 1,
        "id": id,
        "name": name,
        "steps": [],
        "links": []
    }))
    .expect("the workflow fixture has the production shape")
}

fn replace_atomically(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
    let temporary = path.with_extension("repairing");
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn assert_problem_shape<T: serde::Serialize>(
    definition: &Definition<T>,
    shelf: Shelf,
    file_name: &str,
) {
    let encoded = serde_json::to_value(definition).expect("the IPC value serializes");
    assert_eq!(
        encoded,
        json!({
            "kind": "definitionProblem",
            "shelf": shelf,
            "fileName": file_name,
            "problem": "malformed"
        }),
        "a problem crosses IPC with only its shelf, safe file name and closed category. An \
         absolute path, parser detail or invented definition field leaks private file content: \
         {encoded:?}"
    );
}

#[test]
fn both_shelves_keep_good_definitions_and_refresh_a_repaired_file() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let alpha_agent = agent("Alpha");
    let zulu_agent = agent("Zulu");
    // `None` przy każdym zasianiu: te pliki mają POWSTAĆ, więc nie ma czego nadpisać.
    save_agent_inner(home.path(), &zulu_agent, None)?;
    save_agent_inner(home.path(), &alpha_agent, None)?;
    let broken_agent = home.path().join("agents/broken.md");
    fs::write(&broken_agent, b"this is not agent front matter\n")?;

    let alpha_workflow = workflow("wf-alpha", "Alpha workflow");
    let zulu_workflow = workflow("wf-zulu", "Zulu workflow");
    save_workflow_inner(home.path(), "zulu.json", &zulu_workflow, None)?;
    save_workflow_inner(home.path(), "alpha.json", &alpha_workflow, None)?;
    let broken_workflow = home.path().join("workflows/broken.json");
    fs::write(&broken_workflow, b"{ definitely not workflow json")?;

    let agents = list_agent_definitions_inner(home.path());
    assert!(
        agents.is_ok(),
        "one malformed agent still overturned the production list instead of becoming one \
         DefinitionProblem: {agents:?}"
    );
    let agents = agents.unwrap();
    assert_eq!(
        agents.len(),
        3,
        "two healthy agents plus one problem stay visible"
    );
    assert!(matches!(
        &agents[0],
        Definition::Healthy { value, .. } if value.name == "Alpha"
    ));
    assert_problem_shape(&agents[1], Shelf::Agents, "broken.md");
    assert!(matches!(
        &agents[2],
        Definition::Healthy { value, .. } if value.name == "Zulu"
    ));

    let workflows = list_workflow_definitions_inner(home.path());
    assert!(
        workflows.is_ok(),
        "one malformed workflow still overturned the production list instead of becoming one \
         DefinitionProblem: {workflows:?}"
    );
    let workflows = workflows.unwrap();
    assert_eq!(
        workflows.len(),
        3,
        "two healthy workflows plus one problem stay visible"
    );
    assert!(matches!(
        &workflows[0],
        Definition::Healthy { value, .. } if value.path == "alpha.json"
    ));
    assert_problem_shape(&workflows[1], Shelf::Workflows, "broken.json");
    assert!(matches!(
        &workflows[2],
        Definition::Healthy { value, .. } if value.path == "zulu.json"
    ));

    let repaired_agent = agent("Broken");
    let agent_stage = TempDir::new()?;
    let staged_agent = save_agent_inner(agent_stage.path(), &repaired_agent, None)?;
    let repaired_agent_bytes = fs::read(&staged_agent.path)?;
    replace_atomically(&broken_agent, &repaired_agent_bytes)?;
    let repaired_workflow = workflow("wf-broken", "Broken repaired");
    let repaired_workflow_bytes = serde_json::to_vec_pretty(&repaired_workflow)?;
    replace_atomically(&broken_workflow, &repaired_workflow_bytes)?;

    let agents_after = list_agent_definitions_inner(home.path())?;
    assert!(
        agents_after
            .iter()
            .all(|entry| matches!(entry, Definition::Healthy { .. })),
        "a fresh scan kept the old agent problem after the file was atomically repaired: \
         {agents_after:?}"
    );
    let workflows_after = list_workflow_definitions_inner(home.path())?;
    assert!(
        workflows_after
            .iter()
            .all(|entry| matches!(entry, Definition::Healthy { .. })),
        "a fresh scan kept the old workflow problem after the file was atomically repaired: \
         {workflows_after:?}"
    );

    fs::remove_dir_all(home.path().join("agents"))?;
    fs::write(home.path().join("agents"), b"not a directory")?;
    assert!(
        list_agent_definitions_inner(home.path()).is_err(),
        "a broken shelf as a whole was presented as an empty or partly healthy library"
    );
    fs::remove_dir_all(home.path().join("workflows"))?;
    fs::write(home.path().join("workflows"), b"not a directory")?;
    assert!(
        list_workflow_definitions_inner(home.path()).is_err(),
        "a broken workflow shelf as a whole was presented as an empty or partly healthy library"
    );
    Ok(())
}

#[test]
fn agent_save_does_not_overwrite_a_problem_with_the_same_canonical_name()
-> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    fs::create_dir_all(home.path().join("agents"))?;
    let collision = home.path().join("agents/Collision.MD");
    let collision_bytes = b"this malformed file already owns the canonical name\n";
    fs::write(&collision, collision_bytes)?;
    assert!(
        save_agent_inner(home.path(), agent("Collision"), None).is_err(),
        "saving an agent overwrote the differently-cased DefinitionProblem that owns its \
         canonical macOS file name"
    );
    assert_eq!(
        fs::read(&collision)?,
        collision_bytes,
        "the refused save still changed the malformed file"
    );
    Ok(())
}

#[test]
fn workflow_scanner_admits_a_case_variant_before_create_checks_occupied_names()
-> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let collision = home.path().join("workflows/New-Workflow.JSON");
    fs::create_dir_all(
        collision
            .parent()
            .expect("the fixture has a workflows directory"),
    )?;
    fs::write(&collision, b"this malformed workflow owns the APFS name\n")?;

    let listed = list_workflow_definitions_inner(home.path())?;
    assert_eq!(
        listed.len(),
        1,
        "the production scanner hid an uppercase JSON extension from collision checks"
    );
    assert_problem_shape(&listed[0], Shelf::Workflows, "New-Workflow.JSON");
    assert_eq!(
        fs::read(&collision)?,
        b"this malformed workflow owns the APFS name\n",
        "listing a differently-cased extension changed the malformed file"
    );
    Ok(())
}
