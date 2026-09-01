//! T-78 AC-1: jeden obiekt ma nieść pochodzenie, plan i domknięcie zależności.
//!
//! Te testy celowo przechodzą przez prawdziwy skan i granicę komendy. Pusty `items` oraz pola
//! decyzji ignorowane przez backend są szkieletem kontraktowym: kompilują się, ale każda ścieżka
//! pada na asercji o brakującym zachowaniu.

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::commands::import::{ApplySetup, apply_setup_inner, scan_setup_inner};
use loadout_lib::import::{Compatibility, ImportSourceRole, ImportStatus, ItemKind, SourceKind};

const AGENT: &str = "---\n\
                     name: builder\n\
                     description: Builds the project\n\
                     model: opus\n\
                     tools: [Read, Write]\n\
                     ---\n\
                     Build the requested change.\n";

const AGENT_WITH_MISSING_SKILL: &str = "---\n\
                                        name: builder\n\
                                        description: Builds the project\n\
                                        model: opus\n\
                                        tools: [Read, Write]\n\
                                        skills: [missing-skill]\n\
                                        ---\n\
                                        Build the requested change.\n";

/// Ten agent jest formatowo poprawny, ale jedno zachowanie wymaga decyzji człowieka.
const AGENT_WITH_CHOICE: &str = "---\n\
                                 name: builder\n\
                                 description: Builds the project\n\
                                 model: opus\n\
                                 tools: [Read, Write]\n\
                                 maxTurns: 12\n\
                                 ---\n\
                                 Build the requested change.\n";

fn write_agent(root: &Path, content: &str) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(root.join(".claude/agents"))?;
    fs::write(root.join(".claude/agents/builder.md"), content)?;
    Ok(())
}

fn write_skill(root: &Path) -> Result<(), Box<dyn Error>> {
    let folder = root.join(".agents/skills/project-guide");
    fs::create_dir_all(&folder)?;
    fs::write(
        folder.join("SKILL.md"),
        "---\nname: project-guide\ndescription: Explains this project\n---\nFollow the project checks.\n",
    )?;
    Ok(())
}

fn fingerprint(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[test]
fn every_discovered_source_becomes_one_stable_typed_item() -> Result<(), Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    write_agent(repo.path(), AGENT)?;
    write_skill(repo.path())?;

    let first = loadout_lib::import::translate::preview(repo.path())?;
    let second = loadout_lib::import::translate::preview(repo.path())?;

    assert_eq!(
        first.draft.items.len(),
        first.snapshot.items.len(),
        "every discovered source must have exactly one typed item"
    );
    assert_eq!(
        first.draft.items.len(),
        2,
        "the fixture must judge more than one source"
    );
    let ids: BTreeSet<_> = first
        .draft
        .items
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    assert_eq!(
        ids.len(),
        first.draft.items.len(),
        "typed ids must be unique"
    );
    for source in &first.snapshot.items {
        assert_eq!(
            first
                .draft
                .items
                .iter()
                .filter(|item| item.id == source.id)
                .count(),
            1,
            "{} must map to exactly one typed item",
            source.path.display()
        );
    }
    let source = first
        .snapshot
        .items
        .iter()
        .find(|source| source.path == Path::new(".claude/agents/builder.md"))
        .ok_or("scan found no agent source")?;
    let item = first
        .draft
        .items
        .iter()
        .find(|item| item.id == source.id)
        .ok_or("typed agent disappeared")?;
    let same_item = second
        .draft
        .items
        .iter()
        .find(|candidate| candidate.id == item.id)
        .ok_or("the second scan lost the typed item")?;

    assert_eq!(item.id, source.id);
    assert_eq!(item.id, same_item.id, "the id must survive a fresh scan");
    assert_eq!(item.kind, source.kind);
    assert_eq!(item.target.as_deref(), Some(Path::new("agents/builder.md")));
    assert!(item.dependencies.is_empty());
    assert_eq!(item.status, ImportStatus::Ready);
    assert_eq!(item.sources.len(), 1);
    assert_eq!(item.sources[0].provider, SourceKind::Claude);
    assert_eq!(item.sources[0].path, source.path);
    assert_eq!(item.sources[0].hash, source.hash);
    assert_eq!(item.sources[0].role, ImportSourceRole::Definition);

    let home = tempfile::tempdir()?;
    loadout_lib::import::apply::apply(home.path(), &first.draft)?;
    let written = fs::read(home.path().join("agents/builder.md"))?;
    let written_hash = fingerprint(&written);
    assert_eq!(item.generated_hash.as_deref(), Some(written_hash.as_str()));
    Ok(())
}

#[test]
fn exact_format_with_a_missing_dependency_is_not_ready() -> Result<(), Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    write_agent(repo.path(), AGENT_WITH_MISSING_SKILL)?;
    let preview = loadout_lib::import::translate::preview(repo.path())?;

    assert!(
        preview
            .draft
            .report
            .mappings
            .iter()
            .any(|mapping| mapping.compatibility == Compatibility::Exact),
        "the fixture must stay format-compatible so this test judges dependency closure"
    );
    assert!(
        !preview.draft.runnable(),
        "format compatibility cannot make an item runnable while its skill is missing"
    );
    assert_eq!(preview.draft.items.len(), 1);
    let item = &preview.draft.items[0];
    assert!(
        item.dependencies
            .iter()
            .any(|dependency| dependency.contains("missing-skill")),
        "the dependency must be visible in the typed plan"
    );
    assert_eq!(item.status, ImportStatus::MissingDependencies);
    assert!(item.status_message.contains("missing-skill"));
    Ok(())
}

#[test]
fn dependency_requires_a_selected_ready_item_not_only_a_legacy_output() -> Result<(), Box<dyn Error>>
{
    let repo = tempfile::tempdir()?;
    write_agent(repo.path(), AGENT_WITH_CHOICE)?;
    write_skill(repo.path())?;
    let mut preview = loadout_lib::import::translate::preview(repo.path())?;
    let agent_id = preview
        .draft
        .agents
        .first()
        .ok_or("the fixture produced no legacy agent output")?
        .id
        .to_string();
    let agent_item_id = preview
        .draft
        .items
        .iter()
        .find(|item| item.kind == ItemKind::Agent)
        .ok_or("the fixture produced no typed agent item")?
        .id
        .clone();
    let dependent_id = {
        let dependent = preview
            .draft
            .items
            .iter_mut()
            .find(|item| item.kind == ItemKind::Skill)
            .ok_or("the fixture produced no ready dependent item")?;
        dependent.dependencies.push(format!("agent:{agent_id}"));
        dependent.id.clone()
    };

    loadout_lib::import::translate::refresh_statuses(&mut preview.draft);
    assert_eq!(
        preview
            .draft
            .items
            .iter()
            .find(|item| item.id == dependent_id)
            .ok_or("the dependent item disappeared")?
            .status,
        ImportStatus::MissingDependencies,
        "an unresolved dependency item cannot satisfy the graph through draft.agents"
    );

    preview
        .draft
        .report
        .mappings
        .iter_mut()
        .find(|mapping| mapping.item_id == agent_item_id)
        .ok_or("the agent compatibility mapping disappeared")?
        .compatibility = Compatibility::Adjusted;
    loadout_lib::import::translate::refresh_statuses(&mut preview.draft);
    assert_eq!(
        preview
            .draft
            .items
            .iter()
            .find(|item| item.id == dependent_id)
            .ok_or("the dependent item disappeared")?
            .status,
        ImportStatus::Ready,
        "the dependency closes only after its selected item becomes Ready"
    );

    preview.draft.items.retain(|item| item.id != agent_item_id);
    assert!(
        !preview.draft.agents.is_empty(),
        "the fixture must retain the legacy output to catch presence-only closure"
    );
    loadout_lib::import::translate::refresh_statuses(&mut preview.draft);
    assert_eq!(
        preview
            .draft
            .items
            .iter()
            .find(|item| item.id == dependent_id)
            .ok_or("the dependent item disappeared")?
            .status,
        ImportStatus::MissingDependencies,
        "an unselected typed item cannot be replaced by its stale legacy output"
    );
    Ok(())
}

#[test]
fn excluding_an_item_removes_it_from_the_write_plan() -> Result<(), Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    let personal = tempfile::tempdir()?;
    write_agent(repo.path(), AGENT_WITH_CHOICE)?;
    let preview = scan_setup_inner(personal.path(), repo.path())?;
    let choice = preview
        .draft
        .report
        .mappings
        .iter()
        .find(|mapping| mapping.compatibility == Compatibility::NeedsChoice)
        .ok_or("the fixture did not produce a choice")?
        .item_id
        .clone();

    let result = apply_setup_inner(
        home.path(),
        personal.path(),
        &ApplySetup {
            workspace: repo.path().to_path_buf(),
            expected_source_hashes: preview.draft.source_hashes,
            enable_connections: vec![],
            leave_out: vec![],
            excluded_items: vec![choice],
            without_behavior: vec![],
        },
    );
    assert!(
        result.is_ok(),
        "an explicit exclusion must resolve the choice: {result:?}"
    );
    let receipt = result?;
    assert!(
        receipt
            .written
            .iter()
            .all(|target| !target.starts_with("agents"))
    );
    assert!(!home.path().join("agents/builder.md").exists());
    Ok(())
}

#[test]
fn importing_without_one_behavior_keeps_the_item_in_the_write_plan() -> Result<(), Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    let personal = tempfile::tempdir()?;
    write_agent(repo.path(), AGENT_WITH_CHOICE)?;
    let preview = scan_setup_inner(personal.path(), repo.path())?;
    let choice = preview
        .draft
        .report
        .mappings
        .iter()
        .find(|mapping| mapping.compatibility == Compatibility::NeedsChoice)
        .ok_or("the fixture did not produce a choice")?
        .item_id
        .clone();

    let result = apply_setup_inner(
        home.path(),
        personal.path(),
        &ApplySetup {
            workspace: repo.path().to_path_buf(),
            expected_source_hashes: preview.draft.source_hashes,
            enable_connections: vec![],
            leave_out: vec![],
            excluded_items: vec![],
            without_behavior: vec![choice],
        },
    );
    assert!(
        result.is_ok(),
        "removing one behavior must resolve the choice without removing the item: {result:?}"
    );
    let receipt = result?;
    assert!(
        receipt
            .written
            .iter()
            .any(|target| target == Path::new("agents/builder.md"))
    );
    assert!(home.path().join("agents/builder.md").is_file());
    Ok(())
}
