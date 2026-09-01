#[test]
fn scan_and_apply_cross_the_real_product_seam() -> Result<(), Box<dyn std::error::Error>> {
    use loadout_lib::commands::import::{ApplySetup, apply_setup_inner, scan_setup_inner};
    let repo = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    /* Pusty katalog domowy: ten zestaw sądzi import PROJEKTU i nie ma prawa czytać
     * `~/.claude.json` człowieka, który akurat uruchomił testy. */
    let nothing = tempfile::tempdir()?;
    std::fs::create_dir_all(repo.path().join(".codex/agents"))?;
    std::fs::write(
        repo.path().join(".codex/agents/builder.toml"),
        "name = \"builder\"\ndescription = \"Builds\"\ndeveloper_instructions = \"Build the task.\"\n",
    )?;
    let preview = scan_setup_inner(nothing.path(), repo.path())?;
    let receipt = apply_setup_inner(
        home.path(),
        nothing.path(),
        &ApplySetup {
            workspace: repo.path().to_path_buf(),
            expected_source_hashes: preview.draft.source_hashes,
            enable_connections: vec![],
            leave_out: vec![],
            excluded_items: vec![],
            without_behavior: vec![],
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
    /* Pusty katalog domowy: ten zestaw sądzi import PROJEKTU i nie ma prawa czytać
     * `~/.claude.json` człowieka, który akurat uruchomił testy. */
    let nothing = tempfile::tempdir()?;
    std::fs::create_dir_all(repo.path().join(".claude/commands"))?;
    /* 2026-08-29: wyborem jest teraz CEREMONIA bez wykonalnego kontraktu, a nie
     * `.claude/settings.json`. Hak z ustawień cudzej aplikacji przestał być pozycją importu —
     * Loadout go nie uruchamia, więc pytanie o niego nie miało odpowiedzi. Mechanizm, który
     * ten zestaw sądzi, jest ten sam: wybór człowieka przejeżdża przez IPC i odblokowuje zapis. */
    std::fs::write(
        repo.path().join(".claude/commands/ship.md"),
        "Ship whatever looks ready.",
    )?;
    let preview = scan_setup_inner(nothing.path(), repo.path())?;
    let choice = preview
        .draft
        .report
        .mappings
        .iter()
        .find(|mapping| mapping.compatibility == loadout_lib::import::Compatibility::NeedsChoice)
        .ok_or("the routine was not presented as a choice")?
        .item_id
        .clone();
    let receipt = apply_setup_inner(
        home.path(),
        nothing.path(),
        &ApplySetup {
            workspace: repo.path().to_path_buf(),
            expected_source_hashes: preview.draft.source_hashes,
            enable_connections: vec![],
            leave_out: vec![choice],
            excluded_items: vec![],
            without_behavior: vec![],
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
    /* Pusty katalog domowy: ten zestaw sądzi import PROJEKTU i nie ma prawa czytać
     * `~/.claude.json` człowieka, który akurat uruchomił testy. */
    let nothing = tempfile::tempdir()?;
    std::fs::create_dir_all(repo.path().join(".codex/agents"))?;
    std::fs::create_dir_all(repo.path().join(".claude/skills/dreaming"))?;
    std::fs::write(
        repo.path().join(".codex/agents/builder.toml"),
        "name = \"builder\"\ndeveloper_instructions = \"Build the task.\"\n",
    )?;
    /* 2026-08-29: nie do odtworzenia jest teraz SKILL z ukrytym tekstem, a nie
     * `.claude/future.json`. Nierozpoznany plik nie jest już pozycją, więc nie ma czego
     * pomijać; skill jest — i to on ma nie blokować agenta obok siebie. */
    std::fs::write(
        repo.path().join(".claude/skills/dreaming/SKILL.md"),
        "---\nname: dreaming\ndescription: Dream about the task.\n---\nDream\u{200b} about it.",
    )?;
    let preview = scan_setup_inner(nothing.path(), repo.path())?;
    let unknown = preview
        .draft
        .report
        .mappings
        .iter()
        .find(|mapping| mapping.compatibility == loadout_lib::import::Compatibility::Unsupported)
        .ok_or("the skill that cannot be reproduced was not reported")?
        .item_id
        .clone();

    let receipt = apply_setup_inner(
        home.path(),
        nothing.path(),
        &ApplySetup {
            workspace: repo.path().to_path_buf(),
            expected_source_hashes: preview.draft.source_hashes,
            enable_connections: vec![],
            leave_out: vec![unknown],
            excluded_items: vec![],
            without_behavior: vec![],
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
