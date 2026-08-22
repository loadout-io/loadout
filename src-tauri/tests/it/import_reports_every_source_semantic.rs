#[test]
fn every_source_item_gets_one_compatibility_result() -> Result<(), Box<dyn std::error::Error>> {
    let repo = tempfile::tempdir()?;
    std::fs::create_dir_all(repo.path().join(".claude"))?;
    std::fs::write(repo.path().join(".claude/settings.json"), "{\"hooks\":{}}")?;
    std::fs::write(
        repo.path().join(".claude/future.json"),
        "super-secret-value",
    )?;
    let preview = loadout_lib::import::translate::preview(repo.path())?;
    assert_eq!(
        preview.snapshot.items.len(),
        preview.draft.report.mappings.len()
    );
    for item in &preview.snapshot.items {
        assert_eq!(
            preview
                .draft
                .report
                .mappings
                .iter()
                .filter(|mapping| mapping.item_id == item.id)
                .count(),
            1
        );
    }
    assert!(!preview.draft.runnable());
    assert!(!format!("{:?}", preview.draft.report).contains("super-secret-value"));
    Ok(())
}
