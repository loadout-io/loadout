//! Z-32: zgodność biblioteki między wydaniem a nowszym buildem deweloperskim.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::commands::agents::{list_agent_definitions_inner, save_agent_inner};
use loadout_lib::commands::settings::{
    SettingsError, read_settings_snapshot_inner, save_settings_with_revision_inner,
};
use loadout_lib::commands::workflows::{check_workflow_inner, save_workflow_inner};
use loadout_lib::library::agents::Agent;
use loadout_lib::library::definition::{Definition, DefinitionProblemKind, Shelf};
use loadout_lib::workflow::check::check;
use loadout_lib::workflow::file::load;
use loadout_lib::workflow::unroll::unroll;
use tempfile::TempDir;

fn agent(name: &str) -> Agent {
    Agent {
        id: uuid::Uuid::now_v7(),
        name: name.to_owned(),
        summary: format!("{name} stays distinct."),
        instructions: format!("Act as {name}.\n"),
        ..Agent::example()
    }
}

fn insert_after_opening_dashes(path: &Path, setting: &str) -> Result<(), Box<dyn Error>> {
    let original = fs::read_to_string(path)?;
    let changed = original.replacen("---\n", &format!("---\n{setting}\n"), 1);
    fs::write(path, changed)?;
    Ok(())
}

#[test]
fn unknown_agent_field_is_reported_as_newer_loadout_not_malformed() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let mut wire = serde_json::to_value(Agent::example())?;
    wire.as_object_mut()
        .ok_or("the agent wire shape must be an object")?
        .insert("futureCapability".to_owned(), serde_json::json!("careful"));
    let carried: Agent = serde_json::from_value(wire)?;
    assert_eq!(
        carried.extra.get("futureCapability"),
        Some(&serde_json::json!("careful")),
        "the newer field was accepted but not retained in the compatibility map"
    );
    assert_eq!(
        serde_json::to_value(carried)?["futureCapability"],
        serde_json::json!("careful"),
        "a save through the older wire shape would silently erase the newer field"
    );

    let written = save_agent_inner(home.path(), Agent::example(), None)?;
    insert_after_opening_dashes(&written.path, "futureCapability: careful")?;

    let listed = list_agent_definitions_inner(home.path())?;
    assert!(
        matches!(
            listed.as_slice(),
            [Definition::DefinitionProblem {
                shelf: Shelf::Agents,
                problem: DefinitionProblemKind::NewerFormat,
                ..
            }]
        ),
        "a field from a newer Loadout should stay an actionable newer-format problem, not make \
         the whole file malformed: {listed:?}"
    );
    Ok(())
}

#[test]
fn close_unknown_agent_field_is_still_reported_as_a_typo() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let written = save_agent_inner(home.path(), Agent::example(), None)?;
    let original = fs::read_to_string(&written.path)?;
    fs::write(
        &written.path,
        original.replace("summary: Writes code\n", "summry: Writes code\n"),
    )?;

    let listed = list_agent_definitions_inner(home.path())?;
    assert!(matches!(
        listed.as_slice(),
        [Definition::DefinitionProblem {
            problem: DefinitionProblemKind::Malformed,
            ..
        }]
    ));
    let detail = loadout_lib::library::agents::read_agent_file(&written.path)
        .expect_err("the close unknown key must remain a typo")
        .to_string();
    assert!(detail.contains("did you mean `summary`"));
    Ok(())
}

#[test]
fn agent_names_are_occupied_without_regard_to_ascii_case() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    save_agent_inner(home.path(), agent("Forge"), None)?;

    let refused = save_agent_inner(home.path(), agent("forge"), None)
        .expect_err("a second visible name differing only by case must be refused");
    assert_eq!(
        refused.to_string(),
        "This agent was not saved: \"forge\" is already used by another agent. Pick a different name."
    );
    Ok(())
}

#[test]
fn technical_slug_collision_gets_the_eight_character_id_suffix() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    save_agent_inner(home.path(), agent("W"), None)?;
    let turtle = agent("Żółw");
    let expected = format!("w-{}.md", &turtle.id.to_string()[..8]);

    let written = save_agent_inner(home.path(), &turtle, None)?;
    assert_eq!(
        written.path.file_name().and_then(|name| name.to_str()),
        Some(expected.as_str())
    );
    assert_eq!(list_agent_definitions_inner(home.path())?.len(), 2);
    Ok(())
}

#[test]
fn renaming_an_agent_removes_the_old_file_in_the_same_publication() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let original = agent("Before rename");
    let first = save_agent_inner(home.path(), &original, None)?;
    let renamed = Agent {
        name: "After rename".to_owned(),
        ..original
    };

    let second = save_agent_inner(home.path(), &renamed, Some(&first.revision))?;
    assert!(
        !first.path.exists(),
        "the old slug survived the successful rename"
    );
    assert!(second.path.exists());
    let files = fs::read_dir(home.path().join("agents"))?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("md"))
        .count();
    assert_eq!(files, 1, "one stable id must own exactly one agent file");
    Ok(())
}

#[test]
fn large_workflow_numbers_open_and_are_pinned_to_their_tiles() -> Result<(), Box<dyn Error>> {
    let folder = TempDir::new()?;
    let path = folder.path().join("large.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "format": 1,
            "id": "wf-large",
            "name": "Large values",
            "steps": [
                {
                    "kind": "agent",
                    "id": "s_build",
                    "name": "Build",
                    "agent": "agent-build",
                    "overrides": {},
                    "copies": 300,
                    "instructions": "Build it.",
                    "folder": { "use": "fresh-copy" }
                },
                { "kind": "checkpoint", "id": "s_review", "name": "Review" }
            ],
            "links": [
                { "from": "s_build", "to": "s_review" },
                { "from": "s_review", "to": "s_build", "max_turns": 300 }
            ]
        }))?,
    )?;

    let workflow = load(&path)?;
    let notes = check(&workflow);
    assert!(notes.iter().any(|note| {
        note.step_id.as_deref() == Some("s_build")
            && note.message
                == "\"Build\" would run 300 copies at the same time. Pick a number from 1 to 8."
    }));
    assert!(notes.iter().any(|note| {
        note.step_id.as_deref() == Some("s_review")
            && note.message
                == "\"Review\" would send the work back 300 times. Pick a number from 1 to 10."
    }));
    assert_eq!(
        unroll(&workflow).nodes.len(),
        2,
        "a direct caller of an invalid file must not expand 300 copies or turns"
    );
    Ok(())
}

#[test]
fn skill_directory_names_match_without_regard_to_ascii_case() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let mut skilled = agent("Browser worker");
    skilled.skills = vec!["Playwright-CLI".to_owned()];
    save_agent_inner(home.path(), &skilled, None)?;
    fs::create_dir_all(home.path().join("skills/playwright-cli"))?;
    let workflow: loadout_lib::workflow::WorkflowFile =
        serde_json::from_value(serde_json::json!({
            "format": 1,
            "id": "wf-skill-case",
            "name": "Skill case",
            "steps": [{
                "kind": "agent",
                "id": "s_browser",
                "name": "Browser",
                "agent": skilled.id.to_string(),
                "overrides": {},
                "instructions": "Use the browser.",
                "folder": { "use": "fresh-copy" }
            }],
            "links": []
        }))?;

    let notes = check_workflow_inner(home.path(), &workflow);
    assert!(
        notes.iter().all(|note| !note
            .message
            .contains("your library has nothing saved under")),
        "Playwright-CLI did not match the saved playwright-cli directory: {notes:?}"
    );
    Ok(())
}

#[test]
fn refused_workflow_does_not_create_its_directory() -> Result<(), Box<dyn Error>> {
    let library = TempDir::new()?;
    let project = TempDir::new()?;
    let invalid: loadout_lib::workflow::WorkflowFile = serde_json::from_value(serde_json::json!({
        "format": 1,
        "id": "wf-refused",
        "name": "Refused",
        "steps": [{
            "kind": "agent",
            "id": "s_zero",
            "name": "Zero",
            "agent": "agent-zero",
            "overrides": {},
            "copies": 0,
            "instructions": "Never runs."
        }],
        "links": []
    }))?;

    assert!(
        save_workflow_inner(
            library.path(),
            Some(project.path()),
            "refused.json",
            &invalid,
            None,
        )
        .is_err()
    );
    assert!(
        !project.path().join(".loadout/workflows").exists(),
        "validation refused the file after creating an empty workflow directory"
    );
    Ok(())
}

#[test]
fn stale_settings_revision_cannot_overwrite_a_newer_file() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let first =
        save_settings_with_revision_inner(home.path(), "first", 40.0, false, 10, true, None)?;
    let opened = read_settings_snapshot_inner(home.path())?;
    assert_eq!(opened.revision, first.revision);
    let newer = save_settings_with_revision_inner(
        home.path(),
        "newer",
        50.0,
        true,
        20,
        false,
        opened.revision.as_deref(),
    )?;

    let refused = save_settings_with_revision_inner(
        home.path(),
        "stale",
        60.0,
        false,
        30,
        true,
        opened.revision.as_deref(),
    )
    .expect_err("the stale window must not replace newer settings");
    assert!(matches!(refused, SettingsError::Changed));
    assert_eq!(
        refused.to_string(),
        "These settings were not saved: the file changed on disk after you opened it, so nothing was overwritten."
    );
    let final_settings = read_settings_snapshot_inner(home.path())?;
    assert_eq!(final_settings.revision, newer.revision);
    assert_eq!(final_settings.settings.default_lead, "newer");
    Ok(())
}

#[test]
fn numeric_agent_name_is_read_as_text() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let written = save_agent_inner(home.path(), Agent::example(), None)?;
    let original = fs::read_to_string(&written.path)?;
    fs::write(
        &written.path,
        original.replace("name: Forge\n", "name: 2026\n"),
    )?;

    let read = loadout_lib::library::agents::read_agent_file(&written.path)?;
    assert_eq!(read.name, "2026");
    Ok(())
}
