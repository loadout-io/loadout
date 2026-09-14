//! 2026-09-14: import, który wnosi włączone połączenie, dopełnia agentów bez ani jednego.
//!
//! Zapisane `connections: []` znaczy „nikt nigdy nie zapytał", a nie „żadnych": pola Connections
//! nie było w formularzu agenta, więc 26 z 32 agentów w bibliotece właściciela ma pustą listę bez
//! ani jednej decyzji za sobą. Rozstrzygnięcie właściciela z 2026-09-11 mówi, że wolno ją
//! jednorazowo dopełnić — a agent, który coś wymienia, zostaje nietknięty, bo tam decyzja zapadła.

use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::commands::{agents, project_setup};
use loadout_lib::connections::{Connection, Origin, Transport};
use loadout_lib::library::agents::{Agent, read_agent_file};

/// Połączenie w bibliotece. Droga B kopiuje ten plik co do bajtu, więc `enabled` przeżywa kopię.
fn write_connection(library: &Path, name: &str, enabled: bool) -> Result<(), Box<dyn Error>> {
    let one = Connection {
        id: name.to_owned(),
        name: name.to_owned(),
        enabled,
        transport: Transport::Http {
            url: format!("https://{name}.example.test/mcp"),
            token_environment: None,
        },
        source: Path::new(".mcp.json").to_path_buf(),
        source_hash: format!("hash-of-{name}"),
        origin: Origin::Project,
    };
    fs::create_dir_all(library.join("connections"))?;
    fs::write(
        library.join("connections").join(format!("{name}.json")),
        serde_json::to_vec_pretty(&one)?,
    )?;
    Ok(())
}

/// Agent, który leży w bibliotece projektu, zanim ten import się zacznie.
fn saved_agent(library: &Path, name: &str, connections: &[&str]) -> Result<(), Box<dyn Error>> {
    let mut agent = Agent::example();
    agent.id = uuid::Uuid::now_v7();
    name.clone_into(&mut agent.name);
    agent.connections = connections.iter().map(|one| (*one).to_owned()).collect();
    agents::save_agent_inner(library, &agent, None)?;
    Ok(())
}

/// Co niesie plik agenta PO imporcie — z dysku, nie z wartości zwróconej przez import.
fn connections_of(library: &Path, file: &str) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(read_agent_file(&library.join("agents").join(file))?.connections)
}

#[test]
fn a_copy_from_another_project_gives_its_enabled_connections_to_the_empty_agents()
-> Result<(), Box<dyn Error>> {
    let source = tempfile::tempdir()?;
    let destination = tempfile::tempdir()?;
    write_connection(source.path(), "figma", true)?;
    write_connection(source.path(), "notion", false)?;
    /* Obaj agenci leżą w bibliotece DOCELOWEJ, bo to jest ruch właściciela: `lead-orchestrator`
     * jest w urc-monorepo od dawna, a połączenia dopiero przyjeżdżają ze starej biblioteki.
     * W wyborze stoją więc same połączenia — agent z niepustą listą nie mógłby zresztą przyjechać
     * ze źródła, bo `connection:playwright` byłoby tam wymaganiem bez pozycji. */
    saved_agent(destination.path(), "lead", &[])?;
    saved_agent(destination.path(), "picky", &["playwright"])?;

    let seen = project_setup::preview(source.path(), None, destination.path())?;
    project_setup::apply(
        source.path(),
        None,
        destination.path(),
        &seen.revision,
        &[
            "connection:figma".to_owned(),
            "connection:notion".to_owned(),
        ],
    )?;

    assert_eq!(
        connections_of(destination.path(), "lead.md")?,
        vec!["figma".to_owned()],
        "an agent whose list is empty was never asked, so an import that brings a switched-on \
         connection is the moment to answer for it"
    );
    assert_eq!(
        connections_of(destination.path(), "picky.md")?,
        vec!["playwright".to_owned()],
        "and an agent that already names something has an answer — overwriting it would be this \
         import deciding for a person who already decided"
    );
    for entry in fs::read_dir(destination.path().join("agents"))? {
        let path = entry?.path();
        assert!(
            !fs::read_to_string(&path)?.contains("notion"),
            "a connection nobody switched on has no business in an agent file: {}",
            path.display()
        );
    }
    Ok(())
}

#[test]
fn a_scanned_setup_gives_its_turned_on_connections_to_the_agent_that_named_none()
-> Result<(), Box<dyn Error>> {
    use loadout_lib::commands::import::{ApplySetup, apply_setup_inner, scan_setup_inner};

    let repo = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    /* Pusty katalog domowy: ten zestaw sądzi import PROJEKTU i nie ma prawa czytać
     * `~/.claude.json` człowieka, który akurat uruchomił testy. */
    let nothing = tempfile::tempdir()?;
    fs::create_dir_all(repo.path().join(".claude/agents"))?;
    fs::write(
        repo.path().join(".mcp.json"),
        r#"{"mcpServers":{"context7":{"command":"npx","args":["-y","@upstash/context7-mcp"]}}}"#,
    )?;
    // Agent BEZ bloku `mcpServers:`, czyli zwykły przypadek: brak bloku daje dziś pustą listę.
    fs::write(
        repo.path().join(".claude/agents/builder.md"),
        "---\nname: builder\ndescription: Builds\n---\nBuild the task.\n",
    )?;

    let preview = scan_setup_inner(nothing.path(), repo.path())?;
    let ticked: Vec<String> = preview
        .draft
        .connections
        .iter()
        .map(|one| one.id.clone())
        .collect();
    let receipt = apply_setup_inner(
        home.path(),
        nothing.path(),
        &ApplySetup {
            workspace: repo.path().to_path_buf(),
            expected_source_hashes: preview.draft.source_hashes,
            enable_connections: ticked,
            leave_out: vec![],
            excluded_items: vec![],
            without_behavior: vec![],
        },
    )?;

    assert_eq!(
        connections_of(home.path(), "builder.md")?,
        vec!["context7".to_owned()],
        "the person ticked this connection in the same window that brought this agent over, and \
         an agent that arrives without it starts with nothing the person asked for"
    );
    assert_eq!(
        receipt
            .filled_agents
            .iter()
            .map(|one| one.agent.as_str())
            .collect::<Vec<_>>(),
        vec!["builder"],
        "and the sentence a person reads afterwards takes its names from here, not from a guess \
         made on the other side of the wire"
    );
    Ok(())
}

#[test]
fn an_import_without_a_turned_on_connection_leaves_every_agent_file_byte_for_byte()
-> Result<(), Box<dyn Error>> {
    let source = tempfile::tempdir()?;
    let destination = tempfile::tempdir()?;
    write_connection(source.path(), "notion", false)?;
    saved_agent(destination.path(), "lead", &[])?;
    let before = fs::read(destination.path().join("agents/lead.md"))?;

    let seen = project_setup::preview(source.path(), None, destination.path())?;
    project_setup::apply(
        source.path(),
        None,
        destination.path(),
        &seen.revision,
        &["connection:notion".to_owned()],
    )?;

    assert_eq!(
        fs::read(destination.path().join("agents/lead.md"))?,
        before,
        "an import that switches nothing on has nothing to hand out, and an agent file rewritten \
         with the same bytes is still a file somebody else's editor could have been holding"
    );
    Ok(())
}
