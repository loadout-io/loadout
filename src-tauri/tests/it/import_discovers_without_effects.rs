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
