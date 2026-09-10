//! 2026-09-10: nowy projekt nie jest widokiem cudzej biblioteki.
use std::{error::Error, fs};

use loadout_lib::commands::{memory, skills, workflows};
use loadout_lib::memory::notes::{self, Kind, NoteDraft, Scope, Status};
use loadout_lib::workflow::WorkflowFile;

#[test]
fn a_new_project_neither_lists_nor_opens_a_shared_workflow() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let workflow: WorkflowFile = serde_json::from_value(serde_json::json!({
        "format": 1, "id": "private-flow", "name": "Private workflow",
        "steps": [{"kind": "agent", "id": "write", "name": "Write",
            "agent": "01990000-0000-7000-8000-0000000000f1",
            "instructions": "Write the change.", "overrides": {},
            "folder": {"use": "fresh-copy"}}], "links": []
    }))?;
    let saved = workflows::save_workflow_inner(home.path(), None, "private.json", &workflow, None)?;
    let before = fs::read(&saved.path)?;
    assert!(
        workflows::list_workflow_definitions_inner(home.path(), Some(project.path()))?.is_empty()
    );
    assert!(
        workflows::load_workflow_inner(home.path(), Some(project.path()), "private.json").is_err()
    );
    assert_eq!(fs::read(&saved.path)?, before);
    Ok(())
}

#[test]
fn knowledge_does_not_inherit_a_note_marked_everywhere() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let shared = memory::notes_root(home.path());
    notes::record_candidate(
        &shared,
        NoteDraft {
            title: "Only project A knows this".to_owned(),
            rule: "This private convention belongs to project A.".to_owned(),
            because: "The owner added it there.".to_owned(),
            scope: Scope::Everywhere,
            kind: Kind::Rule,
            status: Status::Suggested,
            at: "2026-09-10T00:00:00Z".to_owned(),
        },
    )?;
    assert_eq!(notes::scan_notes(&shared)?.len(), 1);
    assert!(memory::list_notes_for_project_inner(&shared, project.path())?.is_empty());
    assert_eq!(notes::scan_notes(&shared)?.len(), 1);
    Ok(())
}

#[test]
fn knowledge_does_not_fill_a_new_project_with_home_skills() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let library = home.path().join(".loadout");
    let installed = home.path().join(".claude/skills/private-skill");
    fs::create_dir_all(&installed)?;
    fs::write(
        installed.join("SKILL.md"),
        "---\nname: private-skill\ndescription: Private guidance\n---\nOnly for project A.\n",
    )?;
    assert_eq!(skills::list_skills_in(&library, None)?.len(), 1);
    assert!(skills::list_skills_in(&library, Some(project.path()))?.is_empty());
    Ok(())
}

#[test]
fn import_makes_only_selected_independent_copies_and_requires_workflow_agents()
-> Result<(), Box<dyn Error>> {
    use loadout_lib::commands::{agents, project_setup};
    use loadout_lib::library::agents::Agent;
    let source = tempfile::tempdir()?;
    let destination = tempfile::tempdir()?;
    let mut writer = Agent::example();
    writer.name = "Source writer".to_owned();
    agents::save_agent_inner(source.path(), &writer, None)?;
    let mut omitted = writer.clone();
    omitted.id = uuid::Uuid::now_v7();
    omitted.name = "Do not copy me".to_owned();
    agents::save_agent_inner(source.path(), &omitted, None)?;
    let flow: WorkflowFile = serde_json::from_value(serde_json::json!({
        "format":1,"id":"selected-flow","name":"Selected flow","steps":[{"kind":"agent","id":"write","name":"Write","agent":writer.id.to_string(),"overrides":{},"folder":{"use":"fresh-copy"}}],"links":[]
    }))?;
    workflows::save_workflow_inner(source.path(), None, "selected.json", &flow, None)?;
    let seen = project_setup::preview(source.path(), None, destination.path())?;
    let key = format!("agent:{}", writer.id);
    assert!(
        seen.items
            .iter()
            .any(|item| item.key == "workflow:selected-flow" && item.requires.contains(&key))
    );
    assert!(
        project_setup::apply(
            source.path(),
            None,
            destination.path(),
            &seen.revision,
            &["workflow:selected-flow".to_owned()]
        )
        .is_err()
    );
    assert!(agents::list_agents_inner(destination.path())?.is_empty());
    let receipt = project_setup::apply(
        source.path(),
        None,
        destination.path(),
        &seen.revision,
        &[key, "workflow:selected-flow".to_owned()],
    )?;
    assert_eq!(receipt.imported.len(), 2);
    assert_eq!(agents::list_agents_inner(destination.path())?.len(), 1);
    let source_bytes = fs::read(source.path().join("agents/source-writer.md"))?;
    fs::write(
        destination.path().join("agents/source-writer.md"),
        "changed only in destination",
    )?;
    assert_eq!(
        fs::read(source.path().join("agents/source-writer.md"))?,
        source_bytes
    );
    assert!(!destination.path().join("agents/do-not-copy-me.md").exists());
    Ok(())
}

#[test]
fn import_refuses_changed_source_and_never_overwrites_destination() -> Result<(), Box<dyn Error>> {
    use loadout_lib::commands::{agents, project_setup};
    let source = tempfile::tempdir()?;
    let destination = tempfile::tempdir()?;
    let mut agent = loadout_lib::library::agents::Agent::example();
    agent.name = "One".to_owned();
    let written = agents::save_agent_inner(source.path(), &agent, None)?;
    let seen = project_setup::preview(source.path(), None, destination.path())?;
    agent.summary = "A newer edit".to_owned();
    agents::save_agent_inner(source.path(), &agent, Some(&written.revision))?;
    let selected = vec![format!("agent:{}", agent.id)];
    assert!(
        project_setup::apply(
            source.path(),
            None,
            destination.path(),
            &seen.revision,
            &selected
        )
        .is_err()
    );
    assert!(agents::list_agents_inner(destination.path())?.is_empty());
    let seen = project_setup::preview(source.path(), None, destination.path())?;
    agents::save_agent_inner(destination.path(), &agent, None)?;
    let bytes = fs::read(destination.path().join("agents/one.md"))?;
    assert!(
        project_setup::apply(
            source.path(),
            None,
            destination.path(),
            &seen.revision,
            &selected
        )
        .is_err()
    );
    assert_eq!(fs::read(destination.path().join("agents/one.md"))?, bytes);
    Ok(())
}

#[test]
fn imported_knowledge_and_context_keep_their_files_and_executable_scripts()
-> Result<(), Box<dyn Error>> {
    use loadout_lib::{commands::project_setup, context::files, engine::supervisor};
    let source_project = tempfile::tempdir()?;
    let destination_project = tempfile::tempdir()?;
    let source = loadout_lib::library::project_root(source_project.path());
    let destination = loadout_lib::library::project_root(destination_project.path());
    let skill = source.join("skills/helper");
    fs::create_dir_all(skill.join("scripts"))?;
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: helper\ndescription: Project helper\n---\nRun scripts/check.sh.\n",
    )?;
    let script = skill.join("scripts/check.sh");
    fs::write(&script, "#!/bin/sh\nprintf 'checked\\n'\n")?;
    supervisor::set_executable_file(&fs::File::open(&script)?, true)?;
    notes::record_candidate(
        &memory::notes_root(&source),
        NoteDraft {
            title: "Naming".into(),
            rule: "Name things clearly".into(),
            because: "It avoids confusion".into(),
            scope: Scope::Everywhere,
            kind: Kind::Rule,
            status: Status::Suggested,
            at: "2026-09-10T00:00:00Z".into(),
        },
    )?;
    let context = files::create_set(
        &source.join("contexts"),
        "Reference",
        "2026-09-10T00:00:00Z",
    )?;
    let context_folder = files::folder_of(&source.join("contexts"), &context.set.id)?;
    files::publish_source_file(
        &context_folder,
        "sources/example.txt",
        b"Exact reference bytes",
    )?;
    let seen = project_setup::preview(&source, Some(source_project.path()), &destination)?;
    let selected = seen
        .items
        .iter()
        .map(|item| item.key.clone())
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), 3);
    project_setup::apply(
        &source,
        Some(source_project.path()),
        &destination,
        &seen.revision,
        &selected,
    )?;
    let script_copy = destination.join("skills/helper/scripts/check.sh");
    assert_eq!(fs::read(&script_copy)?, fs::read(&script)?);
    assert!(supervisor::executable_bits(&fs::metadata(&script_copy)?));
    assert_eq!(
        memory::list_notes_for_project_inner(&destination, destination_project.path())?.len(),
        1
    );
    let copied = files::folder_of(&destination.join("contexts"), &context.set.id)?;
    assert_eq!(
        fs::read(copied.join("sources/example.txt"))?,
        b"Exact reference bytes"
    );
    Ok(())
}

#[test]
fn interrupted_import_removes_only_its_new_files() -> Result<(), Box<dyn Error>> {
    use loadout_lib::commands::{agents, project_setup};
    let source = tempfile::tempdir()?;
    let destination = tempfile::tempdir()?;
    let mut agent = loadout_lib::library::agents::Agent::example();
    agent.name = "Copy me".into();
    agents::save_agent_inner(source.path(), &agent, None)?;
    fs::write(destination.path().join("keep.txt"), "Existing work")?;
    let seen = project_setup::preview(source.path(), None, destination.path())?;
    let selected = seen
        .items
        .iter()
        .map(|item| item.key.clone())
        .collect::<Vec<_>>();
    let result = project_setup::apply_with_hook(
        source.path(),
        None,
        destination.path(),
        &seen.revision,
        &selected,
        |_| Err("Disk interrupted".into()),
    );
    assert!(result.is_err());
    assert!(agents::list_agents_inner(destination.path())?.is_empty());
    assert_eq!(
        fs::read_to_string(destination.path().join("keep.txt"))?,
        "Existing work"
    );
    assert_eq!(agents::list_agents_inner(source.path())?.len(), 1);
    Ok(())
}

// Ten panic jest pułapką testową: samo dotarcie do sterownika obcego agenta musi dać czerwone.
#[allow(clippy::panic)]
#[tokio::test]
async fn application_start_and_lead_resolve_only_the_selected_projects_agents()
-> Result<(), Box<dyn Error>> {
    use loadout_lib::{
        commands::{RunRequest, agents, chat::Lead, run::run_workflow_inner},
        ipc::{AppState, QUEUE_CAP, line_channel},
        store::Store,
    };
    let home = tempfile::tempdir()?;
    let first = tempfile::tempdir()?;
    let second = tempfile::tempdir()?;
    let agent = loadout_lib::library::agents::Agent::example();
    agents::save_agent_inner(home.path(), &agent, None)?;
    agents::save_agent_inner(
        &loadout_lib::library::project_root(first.path()),
        &agent,
        None,
    )?;
    let state = AppState::new(
        home.path().to_path_buf(),
        first.path().to_path_buf(),
        Store::open(&home.path().join("test.db"))?,
        std::sync::Arc::new(|_| panic!("a foreign agent must never reach a driver")),
    );
    let first_library = state
        .library_for(Some(first.path().to_str().ok_or("path")?))
        .await?;
    let second_library = state
        .library_for(Some(second.path().to_str().ok_or("path")?))
        .await?;
    assert!(Lead::pointed_at(&first_library, Some(&agent.id.to_string())).is_ok());
    assert!(Lead::pointed_at(&second_library, Some(&agent.id.to_string())).is_err());
    let workflow: WorkflowFile = serde_json::from_value(
        serde_json::json!({"format":1,"id":"private","name":"Private","steps":[{"kind":"agent","id":"one","name":"One","agent":agent.id,"folder":{"use":"project"},"overrides":{}}],"links":[]}),
    )?;
    let saved =
        workflows::save_workflow_inner(&second_library, None, "private.json", &workflow, None)?;
    let deps = state.begin_run(second.path())?;
    let request = RunRequest {
        workflow: saved.path,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (lines, _) = line_channel(QUEUE_CAP);
    let answer = run_workflow_inner(&deps, &request, lines).await;
    deps.control.settle();
    assert!(
        answer.is_err(),
        "a project cannot start a workflow using an agent that exists only elsewhere"
    );
    assert!(agents::list_agents_inner(&second_library)?.is_empty());
    Ok(())
}

#[test]
fn another_workflow_can_reuse_an_identical_agent_already_imported() -> Result<(), Box<dyn Error>> {
    use loadout_lib::commands::{agents, project_setup};
    let source = tempfile::tempdir()?;
    let destination = tempfile::tempdir()?;
    let agent = loadout_lib::library::agents::Agent::example();
    agents::save_agent_inner(source.path(), &agent, None)?;
    let key = format!("agent:{}", agent.id);
    let preview = project_setup::preview(source.path(), None, destination.path())?;
    project_setup::apply(
        source.path(),
        None,
        destination.path(),
        &preview.revision,
        std::slice::from_ref(&key),
    )?;
    let original = agents::list_agents_inner(destination.path())?;
    let flow: WorkflowFile = serde_json::from_value(serde_json::json!({
        "format":1,"id":"reuse","name":"Reuse imported agent","steps":[{"kind":"agent","id":"one","name":"One","agent":agent.id,"overrides":{},"folder":{"use":"fresh-copy"}}],"links":[]
    }))?;
    workflows::save_workflow_inner(source.path(), None, "reuse.json", &flow, None)?;
    let preview = project_setup::preview(source.path(), None, destination.path())?;
    let receipt = project_setup::apply(
        source.path(),
        None,
        destination.path(),
        &preview.revision,
        &[key, "workflow:reuse".into()],
    )?;
    assert_eq!(receipt.imported, vec!["workflow:reuse"]);
    assert_eq!(
        agents::list_agents_inner(destination.path())?[0].instructions,
        original[0].instructions
    );
    assert_eq!(
        workflows::list_workflow_definitions_inner(destination.path(), None)?.len(),
        1
    );
    Ok(())
}

#[test]
fn interrupted_import_keeps_an_edited_copy_and_removes_the_other_new_files()
-> Result<(), Box<dyn Error>> {
    use loadout_lib::commands::{agents, project_setup};
    let source = tempfile::tempdir()?;
    let destination = tempfile::tempdir()?;
    for name in ["Alpha", "Beta"] {
        let mut agent = loadout_lib::library::agents::Agent::example();
        agent.id = uuid::Uuid::now_v7();
        agent.name = name.into();
        agents::save_agent_inner(source.path(), &agent, None)?;
    }
    let preview = project_setup::preview(source.path(), None, destination.path())?;
    let selected = preview
        .items
        .iter()
        .map(|item| item.key.clone())
        .collect::<Vec<_>>();
    let result = project_setup::apply_with_hook(
        source.path(),
        None,
        destination.path(),
        &preview.revision,
        &selected,
        |count| {
            if count == 2 {
                // An editor may update bytes in the same inode instead of replacing the file.
                fs::write(destination.path().join("agents/beta.md"), "User's new edit")
                    .map_err(|e| e.to_string())?;
                Err("Interrupted".to_owned())
            } else {
                Ok(())
            }
        },
    );
    assert!(result.is_err());
    assert_eq!(
        fs::read_to_string(destination.path().join("agents/beta.md"))?,
        "User's new edit"
    );
    assert!(!destination.path().join("agents/alpha.md").exists());
    Ok(())
}

#[test]
fn import_refuses_a_workflow_that_would_write_back_to_its_source_project()
-> Result<(), Box<dyn Error>> {
    use loadout_lib::commands::{agents, project_setup};
    let source_project = tempfile::tempdir()?;
    let source = source_project.path().join(".loadout");
    let destination = tempfile::tempdir()?;
    let agent = loadout_lib::library::agents::Agent::example();
    agents::save_agent_inner(&source, &agent, None)?;
    let flow: WorkflowFile = serde_json::from_value(serde_json::json!({
        "format":1,"id":"write-back","name":"Write back","steps":[{"kind":"agent","id":"write","name":"Write","agent":agent.id,"overrides":{"writeResultsTo":source_project.path().join("result.md")},"folder":{"use":"fresh-copy"}}],"links":[]
    }))?;
    workflows::save_workflow_inner(&source, None, "back.json", &flow, None)?;
    let seen = project_setup::preview(&source, Some(source_project.path()), destination.path())?;
    let item = seen
        .items
        .iter()
        .find(|item| item.key == "workflow:write-back")
        .ok_or("workflow absent")?;
    assert!(
        item.problems
            .iter()
            .any(|said| said.contains("source project")),
        "{}",
        item.problems.join(" ")
    );
    assert!(
        project_setup::apply(
            &source,
            Some(source_project.path()),
            destination.path(),
            &seen.revision,
            &seen
                .items
                .iter()
                .map(|item| item.key.clone())
                .collect::<Vec<_>>()
        )
        .is_err()
    );
    assert!(!destination.path().join("workflows/back.json").exists());
    Ok(())
}
