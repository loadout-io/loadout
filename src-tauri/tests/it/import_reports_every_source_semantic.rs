#[test]
fn every_source_item_gets_one_compatibility_result() -> Result<(), Box<dyn std::error::Error>> {
    let repo = tempfile::tempdir()?;
    std::fs::create_dir_all(repo.path().join(".claude/commands"))?;
    /* 2026-08-29: scena stoi na dwóch rzeczach, które Loadout u siebie STAWIA. Do tego dnia
     * stały tu `.claude/settings.json` i `.claude/future.json` — pliki, których importer nie
     * czyta już jako pozycji, więc oba zdania niżej byłyby zdaniami o pustym planie.
     *
     * Sekret siedzi teraz tam, gdzie sekrety siedzą naprawdę: w pliku połączenia. */
    std::fs::write(
        repo.path().join(".mcp.json"),
        r#"{"mcpServers":{"tracker":{"command":"npx","env":{"TOKEN":"super-secret-value"}}}}"#,
    )?;
    /* Ceremonia bez wykonalnego kontraktu: zostaje wyborem, więc plan nie jest gotowy. */
    std::fs::write(
        repo.path().join(".claude/commands/ship.md"),
        "Ship whatever looks ready.",
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
