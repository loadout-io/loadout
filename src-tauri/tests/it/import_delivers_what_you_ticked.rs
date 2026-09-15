//! 2026-09-16 — import właściciela, Loadout 0.6.1, projekt urc-monorepo.
//!
//! Zaznaczył sześć połączeń, przyszły cztery; z trzynastu agentów przyszedł jeden. Ani jedno
//! zdanie na ekranie tego nie nazwało. Trzy wady, każda dowodzona tu osobno: ptaszek przegrywał
//! z regułą „plik wypadł, więc jego połączenie też", wybór, którego obie odpowiedzi dają plik
//! identyczny co do bajtu, odznaczał agenta za człowieka, a paragon na dysku mówił, że nikt
//! niczego nie dostał.

use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::commands::import::{ApplySetup, apply_setup_inner, scan_setup_inner};
use loadout_lib::import::{Compatibility, ImportStatus, ItemKind};
use loadout_lib::library::agents::Agent;

/// Agent, który jako JEDYNY deklaruje serwer `figma` — kształt nagłówka żywcem z repo właściciela.
const FIGMA_EXTRACTOR: &str = "---\n\
                               name: figma-extractor\n\
                               description: Reads a design.\n\
                               mcpServers:\n  \
                                 figma:\n    \
                                   type: http\n    \
                                   url: http://127.0.0.1:3845/mcp\n\
                               ---\n\
                               Read the design and write down what it says.\n";

/// Agent, który w tym imporcie ZOSTAJE. U właściciela została ich większość, a to jego obecność
/// przeprowadza połączenie przez `apply::record_file` — plan z jedną pozycją omija tę drogę.
const WRITER: &str = "---\n\
                      name: writer\n\
                      description: Writes the notes.\n\
                      ---\n\
                      Write down what happened.\n";

/// Agent z `memory:` i `maxTurns:` — oba klucze dają plik identyczny co do bajtu.
const AGENT_WITH_MEMORY_AND_TURNS: &str = "---\n\
                                           name: builder\n\
                                           description: Builds the project\n\
                                           model: opus\n\
                                           memory: project\n\
                                           maxTurns: 25\n\
                                           ---\n\
                                           Build the requested change.\n";

/// Agent z `type:` — ten klucz zostaje przy wyborze, bo tak mówi zlecenie.
const AGENT_WITH_TYPE: &str = "---\n\
                               name: planner\n\
                               description: Plans the work\n\
                               model: opus\n\
                               type: general-purpose\n\
                               ---\n\
                               Plan the requested change.\n";

fn write_agent(root: &Path, file: &str, content: &str) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(root.join(".claude/agents"))?;
    fs::write(root.join(".claude/agents").join(file), content)?;
    Ok(())
}

/// Jeden przebieg: skan, wykluczenie pliku agenta, import z podaną listą zaznaczonych połączeń.
fn import_without_the_agent(ticked: Vec<String>) -> Result<tempfile::TempDir, Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    // Pusty katalog domowy: ten zestaw sądzi import PROJEKTU i nie ma prawa czytać
    // `~/.claude.json` człowieka, który akurat uruchomił testy.
    let nothing = tempfile::tempdir()?;
    write_agent(repo.path(), "figma-extractor.md", FIGMA_EXTRACTOR)?;
    write_agent(repo.path(), "writer.md", WRITER)?;

    let preview = scan_setup_inner(nothing.path(), repo.path())?;
    let agent_item = preview
        .draft
        .items
        .iter()
        .find(|item| {
            item.kind == ItemKind::Agent
                && item
                    .sources
                    .iter()
                    .any(|source| source.path == Path::new(".claude/agents/figma-extractor.md"))
        })
        .ok_or("the fixture produced no typed item for the agent that declares figma")?
        .id
        .clone();
    assert_eq!(
        preview
            .draft
            .connections
            .iter()
            .map(|one| one.name.as_str())
            .collect::<Vec<_>>(),
        vec!["figma"],
        "the fixture has to declare figma in the agent header and nowhere else, or this test \
         judges an easier project than the one a person actually had"
    );

    apply_setup_inner(
        home.path(),
        nothing.path(),
        &ApplySetup {
            workspace: repo.path().to_path_buf(),
            expected_source_hashes: preview.draft.source_hashes,
            enable_connections: ticked,
            leave_out: vec![],
            excluded_items: vec![agent_item],
            without_behavior: vec![],
        },
    )?;
    Ok(home)
}

#[test]
fn a_ticked_connection_lands_even_when_its_agent_stays_behind() -> Result<(), Box<dyn Error>> {
    let kept = import_without_the_agent(vec!["figma".to_owned()])?;
    assert!(
        kept.path().join("connections/figma.json").is_file(),
        "the tick is a decision about this connection, not about the file it happened to be \
         found in — taking the agent out of the import cannot silently take the connection too"
    );
    assert!(
        !kept.path().join("agents/figma-extractor.md").exists(),
        "and the file the person did untick really stays out, or this proves nothing"
    );

    // Świeże katalogi, ten sam projekt, bez ptaszka: brak zaznaczenia dalej znaczy „nie wnoś".
    let left = import_without_the_agent(vec![])?;
    assert!(
        !left.path().join("connections/figma.json").exists(),
        "and a connection nobody ticked still stays out, or this would be an import that brings \
         whatever it found"
    );
    Ok(())
}

#[test]
fn project_memory_and_a_turn_limit_do_not_ask_a_question_with_one_answer()
-> Result<(), Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    write_agent(repo.path(), "builder.md", AGENT_WITH_MEMORY_AND_TURNS)?;
    write_agent(repo.path(), "planner.md", AGENT_WITH_TYPE)?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;
    let builder = preview
        .draft
        .items
        .iter()
        .find(|item| {
            item.sources
                .iter()
                .any(|source| source.path == Path::new(".claude/agents/builder.md"))
        })
        .ok_or("the agent with memory and a turn limit disappeared")?;
    assert_eq!(
        builder.status,
        ImportStatus::Ready,
        "both answers to this question write the same file byte for byte, so asking it only \
         costs a person the agent: the screen unticks every row that is not ready"
    );
    assert!(
        builder.status_message.contains("project memory")
            && builder.status_message.contains("turn limit"),
        "and the row still has to say what Loadout leaves behind, or the agent arrives quieter \
         than it was: {}",
        builder.status_message
    );

    let planner = preview
        .draft
        .items
        .iter()
        .find(|item| {
            item.sources
                .iter()
                .any(|source| source.path == Path::new(".claude/agents/planner.md"))
        })
        .ok_or("the agent with a type disappeared")?;
    assert_eq!(
        planner.status,
        ImportStatus::NeedsChoice,
        "and the key that was never measured keeps its question"
    );
    assert!(
        preview.draft.report.mappings.iter().any(|mapping| {
            mapping.compatibility == Compatibility::NeedsChoice
                && mapping.message.contains("agent type")
        }),
        "named, so a person knows which behavior is waiting on them"
    );
    Ok(())
}

#[test]
fn the_saved_import_file_lists_the_agents_the_window_named() -> Result<(), Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    let nothing = tempfile::tempdir()?;
    fs::write(
        repo.path().join(".mcp.json"),
        r#"{"mcpServers":{"context7":{"command":"npx","args":["-y","@upstash/context7-mcp"]}}}"#,
    )?;
    // Agent, który leżał w bibliotece PRZED tym importem, z pustą listą połączeń: to jego
    // dopełnia przebieg po przeniesieniu plików, czyli po zapisaniu paragonu.
    let mut lead = Agent::example();
    lead.id = uuid::Uuid::now_v7();
    "lead".clone_into(&mut lead.name);
    lead.connections = Vec::new();
    save_agent_inner(home.path(), &lead, None)?;

    let preview = scan_setup_inner(nothing.path(), repo.path())?;
    let receipt = apply_setup_inner(
        home.path(),
        nothing.path(),
        &ApplySetup {
            workspace: repo.path().to_path_buf(),
            expected_source_hashes: preview.draft.source_hashes,
            enable_connections: vec!["context7".to_owned()],
            leave_out: vec![],
            excluded_items: vec![],
            without_behavior: vec![],
        },
    )?;
    assert_eq!(
        receipt
            .filled_agents
            .iter()
            .map(|one| one.agent.as_str())
            .collect::<Vec<_>>(),
        vec!["lead"],
        "the window says this out loud after the import"
    );

    // BAJTY PLIKU, nie wartość zwrócona przez funkcję: to ten plik człowiek otwiera w edytorze.
    let saved = fs::read_to_string(
        home.path()
            .join("imports")
            .join(format!("{}.json", receipt.id)),
    )?;
    let filled: serde_json::Value = serde_json::from_str(&saved)?;
    assert_eq!(
        filled["filledAgents"]
            .as_array()
            .ok_or("the saved import file has no list of agents that were filled in")?
            .iter()
            .filter_map(|one| one["agent"].as_str())
            .collect::<Vec<_>>(),
        vec!["lead"],
        "and the file on disk says the same thing, or a person reading it is told that nobody \
         got anything while two agent files on that same disk say otherwise"
    );
    Ok(())
}
