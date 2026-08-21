#[test]
fn complete_skill_bundle_is_preserved() -> Result<(), Box<dyn std::error::Error>> {
    let repo = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    let skill = repo.path().join(".agents/skills/project-guide");
    std::fs::create_dir_all(skill.join("scripts"))?;
    std::fs::create_dir_all(skill.join("references"))?;
    let source_document = "---\n# keep this comment byte-for-byte\nname: project-guide\ndescription: Explains this repository\n---\nFollow the repository checks.\n";
    std::fs::write(skill.join("SKILL.md"), source_document)?;
    std::fs::write(skill.join("scripts/run.sh"), "exit 88\n")?;
    std::fs::write(
        skill.join("references/rules.md"),
        "Keep the source intact.\n",
    )?;
    let preview = loadout_lib::import::translate::preview(repo.path())?;
    assert!(preview.draft.runnable());
    let receipt = loadout_lib::import::apply::apply(home.path(), &preview.draft)?;
    assert!(
        receipt
            .written
            .iter()
            .any(|path| path.ends_with("scripts/run.sh"))
    );
    assert_eq!(
        std::fs::read(home.path().join("skills/project-guide/scripts/run.sh"))?,
        b"exit 88\n"
    );
    assert_eq!(
        std::fs::read(home.path().join("skills/project-guide/SKILL.md"))?,
        source_document.as_bytes()
    );
    assert_eq!(std::fs::read(skill.join("scripts/run.sh"))?, b"exit 88\n");
    Ok(())
}
