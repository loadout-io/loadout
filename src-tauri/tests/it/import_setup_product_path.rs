#[test]
fn scan_and_apply_cross_the_real_product_seam() -> Result<(), Box<dyn std::error::Error>> {
    use loadout_lib::commands::import::{ApplySetup, apply_setup_inner, scan_setup_inner};
    let repo = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    std::fs::create_dir_all(repo.path().join(".codex/agents"))?;
    std::fs::write(
        repo.path().join(".codex/agents/builder.toml"),
        "name = \"builder\"\ndescription = \"Builds\"\ndeveloper_instructions = \"Build the task.\"\n",
    )?;
    let preview = scan_setup_inner(repo.path())?;
    let receipt = apply_setup_inner(
        home.path(),
        &ApplySetup {
            workspace: repo.path().to_path_buf(),
            expected_source_hashes: preview.draft.source_hashes,
            enable_connections: vec![],
            leave_out: vec![],
        },
    )?;
    assert!(
        receipt
            .written
            .iter()
            .any(|path| path.starts_with("agents"))
    );
    assert!(home.path().join("agents/builder.md").is_file());
    Ok(())
}

#[test]
fn an_explicit_leave_out_choice_crosses_ipc_and_unblocks_apply()
-> Result<(), Box<dyn std::error::Error>> {
    use loadout_lib::commands::import::{ApplySetup, apply_setup_inner, scan_setup_inner};

    let repo = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    std::fs::create_dir_all(repo.path().join(".claude"))?;
    std::fs::write(
        repo.path().join(".claude/settings.json"),
        r#"{"hooks":{"PostToolUse":[{"command":"./format.sh"}]}}"#,
    )?;
    let preview = scan_setup_inner(repo.path())?;
    let choice = preview
        .draft
        .report
        .mappings
        .iter()
        .find(|mapping| mapping.compatibility == loadout_lib::import::Compatibility::NeedsChoice)
        .ok_or("the hook was not presented as a choice")?
        .item_id
        .clone();
    let receipt = apply_setup_inner(
        home.path(),
        &ApplySetup {
            workspace: repo.path().to_path_buf(),
            expected_source_hashes: preview.draft.source_hashes,
            enable_connections: vec![],
            leave_out: vec![choice],
        },
    )?;
    assert_eq!(
        receipt.written.len(),
        1,
        "only the import record is written"
    );
    Ok(())
}

#[test]
fn an_explicit_skip_keeps_an_unknown_setting_from_blocking_compatible_items()
-> Result<(), Box<dyn std::error::Error>> {
    use loadout_lib::commands::import::{ApplySetup, apply_setup_inner, scan_setup_inner};

    let repo = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    std::fs::create_dir_all(repo.path().join(".codex/agents"))?;
    std::fs::create_dir_all(repo.path().join(".claude"))?;
    std::fs::write(
        repo.path().join(".codex/agents/builder.toml"),
        "name = \"builder\"\ndeveloper_instructions = \"Build the task.\"\n",
    )?;
    std::fs::write(repo.path().join(".claude/future.json"), "{\"new\":true}")?;
    let preview = scan_setup_inner(repo.path())?;
    let unknown = preview
        .draft
        .report
        .mappings
        .iter()
        .find(|mapping| mapping.compatibility == loadout_lib::import::Compatibility::Unsupported)
        .ok_or("the unknown setting was not reported")?
        .item_id
        .clone();

    let receipt = apply_setup_inner(
        home.path(),
        &ApplySetup {
            workspace: repo.path().to_path_buf(),
            expected_source_hashes: preview.draft.source_hashes,
            enable_connections: vec![],
            leave_out: vec![unknown],
        },
    )?;

    assert!(
        receipt
            .written
            .iter()
            .any(|path| path.starts_with("agents"))
    );
    assert!(home.path().join("agents/builder.md").is_file());
    Ok(())
}
