#[test]
fn arbitrary_repository_is_inspected_without_effects() -> Result<(), Box<dyn std::error::Error>> {
    let repo = tempfile::tempdir()?;
    std::fs::create_dir_all(repo.path().join(".claude/agents"))?;
    std::fs::write(
        repo.path().join(".claude/agents/reviewer.md"),
        "Review code.",
    )?;
    std::fs::write(repo.path().join(".claude/future.json"), "{\"new\":true}")?;
    let first = loadout_lib::import::discover::scan(repo.path())?;
    let second = loadout_lib::import::discover::scan(repo.path())?;
    assert_eq!(first.snapshot, second.snapshot);
    assert_eq!(first.snapshot.items.len(), 2);
    assert!(
        first
            .snapshot
            .items
            .iter()
            .all(|item| item.path.is_relative())
    );
    assert!(
        first
            .snapshot
            .items
            .iter()
            .all(|item| item.hash.len() == 16)
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join(".claude/agents/reviewer.md"))?,
        "Review code."
    );
    Ok(())
}

#[test]
fn selecting_the_claude_folder_still_scans_the_project() -> Result<(), Box<dyn std::error::Error>> {
    let repo = tempfile::tempdir()?;
    let claude = repo.path().join(".claude");
    std::fs::create_dir_all(claude.join("agents"))?;
    std::fs::write(
        claude.join("agents/frontend-dev.md"),
        "Build the interface.",
    )?;

    let inspection = loadout_lib::import::discover::scan(&claude)?;

    assert_eq!(inspection.snapshot.root, repo.path().canonicalize()?);
    assert!(
        inspection
            .snapshot
            .items
            .iter()
            .any(|item| item.path == std::path::Path::new(".claude/agents/frontend-dev.md"))
    );
    Ok(())
}

#[test]
fn run_evidence_is_not_mistaken_for_project_configuration() -> Result<(), Box<dyn std::error::Error>>
{
    let repo = tempfile::tempdir()?;
    std::fs::create_dir_all(repo.path().join(".claude/agents"))?;
    std::fs::create_dir_all(repo.path().join(".claude/traces/a-run"))?;
    std::fs::create_dir_all(repo.path().join(".claude/workflows"))?;
    std::fs::write(
        repo.path().join(".claude/agents/frontend-dev.md"),
        "Build the interface.",
    )?;
    std::fs::write(
        repo.path().join(".claude/traces/a-run/screenshot.png"),
        [0_u8, 159, 146, 150],
    )?;
    std::fs::write(
        repo.path().join(".claude/workflows/ship.js"),
        "agent('frontend-dev')",
    )?;

    let inspection = loadout_lib::import::discover::scan(repo.path())?;

    assert_eq!(inspection.snapshot.items.len(), 2);
    assert!(inspection.snapshot.items.iter().any(|item| {
        item.path == std::path::Path::new(".claude/workflows/ship.js")
            && item.kind == loadout_lib::import::ItemKind::Workflow
    }));
    assert!(
        inspection
            .snapshot
            .items
            .iter()
            .all(|item| !item.path.starts_with(".claude/traces"))
    );
    Ok(())
}

#[test]
fn open_standards_and_rulesync_are_part_of_the_inventory() -> Result<(), Box<dyn std::error::Error>>
{
    let repo = tempfile::tempdir()?;
    std::fs::create_dir_all(repo.path().join(".rulesync/rules"))?;
    std::fs::create_dir_all(repo.path().join(".rulesync/commands"))?;
    std::fs::create_dir_all(repo.path().join(".rulesync/checks"))?;
    std::fs::write(repo.path().join("AGENTS.md"), "Keep checks deterministic.")?;
    std::fs::write(repo.path().join("CLAUDE.md"), "Use the project skills.")?;
    std::fs::write(
        repo.path().join(".rulesync/rules/angular.md"),
        "---\ntargets: [\"*\"]\n---\nUse standalone components.",
    )?;
    std::fs::write(
        repo.path().join(".rulesync/commands/review.md"),
        "---\ndescription: Review the change\n---\nReview $ARGUMENTS.",
    )?;
    std::fs::write(
        repo.path().join(".rulesync/checks/security.md"),
        "---\nseverity: high\n---\nCheck authentication boundaries.",
    )?;

    let inspection = loadout_lib::import::discover::scan(repo.path())?;
    let paths: Vec<_> = inspection
        .snapshot
        .items
        .iter()
        .map(|item| item.path.as_path())
        .collect();

    assert_eq!(paths.len(), 5);
    assert!(paths.contains(&std::path::Path::new("AGENTS.md")));
    assert!(paths.contains(&std::path::Path::new("CLAUDE.md")));
    assert!(paths.contains(&std::path::Path::new(".rulesync/rules/angular.md")));
    assert!(paths.contains(&std::path::Path::new(".rulesync/commands/review.md")));
    assert!(paths.contains(&std::path::Path::new(".rulesync/checks/security.md")));
    Ok(())
}
