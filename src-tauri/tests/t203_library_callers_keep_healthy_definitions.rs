//! T-203 AC-2: prawdziwi Rustowi callerzy widzą zdrowych agentów obok wadliwego pliku.
//!
//! Każda asercja woła istniejącą drogę produktu: wybór lidera, wybór autora skilla i roster
//! workflowu. Sam `healthy_only` nie jest tu wyrocznią.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::commands::chat::Lead;
use loadout_lib::commands::skills::the_agent_saved_as;
use loadout_lib::commands::workflows::check_workflow_inner;
use loadout_lib::library::agents::{Agent, Tools};
use loadout_lib::workflow::WorkflowFile;
use serde_json::json;
use tempfile::TempDir;

fn agent(name: &str) -> Agent {
    Agent {
        id: uuid::Uuid::now_v7(),
        name: name.to_owned(),
        summary: format!("{name} remains selectable."),
        instructions: format!("Act as {name}.\n"),
        ..Agent::example()
    }
}

fn workflow(agent: &Agent) -> WorkflowFile {
    serde_json::from_value(json!({
        "format": 1,
        "id": "wf-t203-callers",
        "name": "Caller probe",
        "steps": [{
            "kind": "agent",
            "id": "step-agent",
            "name": "Use the healthy agent",
            "agent": agent.id.to_string(),
            "overrides": {},
            "instructions": "Run the probe.",
            "folder": { "use": "fresh-copy" }
        }],
        "links": []
    }))
    .expect("the caller fixture is a production workflow")
}

fn replace_atomically(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
    let temporary = path.with_extension("repairing");
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn assert_all_callers(home: &Path, healthy_agents: &[&Agent], roster_agent: &Agent) {
    for &healthy in healthy_agents {
        let lead = Lead::pointed_at(home, Some(&healthy.id.to_string()));
        assert!(
            lead.is_ok(),
            "the Lead picker lost healthy agent {} because a neighboring file was malformed: \
             {lead:?}",
            healthy.name
        );
        assert_eq!(lead.unwrap().agent, *healthy);

        let skill_agent = the_agent_saved_as(home, &healthy.id.to_string());
        assert!(
            skill_agent.is_ok(),
            "the Skills caller lost healthy agent {} because a neighboring file was malformed: \
             {skill_agent:?}",
            healthy.name
        );
        assert_eq!(skill_agent.unwrap(), *healthy);
    }

    let notes = check_workflow_inner(home, workflow(roster_agent));
    assert!(
        notes.iter().any(|note| {
            note.step_id.as_deref() == Some("step-agent")
                && note.message.to_ascii_lowercase().contains("tool")
        }),
        "the roster caller behaved as if the whole agent library were empty. It should have \
         inspected the healthy agent and reported its deliberately empty tool list: {notes:?}"
    );
}

#[test]
fn a_bad_neighbor_does_not_hide_agents_from_any_rust_caller() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let mut alpha = agent("Alpha");
    alpha.tools = Tools::Only(Vec::new());
    let beta = agent("Beta");
    // `None` przy każdym zasianiu: te pliki mają POWSTAĆ, więc nie ma czego nadpisać.
    save_agent_inner(home.path(), &alpha, None)?;
    save_agent_inner(home.path(), &beta, None)?;
    let bad = home.path().join("agents/broken.md");
    fs::write(&bad, b"not agent front matter\n")?;

    assert_all_callers(home.path(), &[&alpha, &beta], &alpha);

    let repaired = agent("Broken");
    let stage = TempDir::new()?;
    let staged = save_agent_inner(stage.path(), &repaired, None)?;
    replace_atomically(&bad, &fs::read(staged.path)?)?;

    assert_all_callers(home.path(), &[&alpha, &beta, &repaired], &alpha);
    Ok(())
}
