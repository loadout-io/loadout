#[test]
fn approved_draft_is_applied_atomically() -> Result<(), Box<dyn std::error::Error>> {
    use std::collections::BTreeMap;
    use std::path::Path;

    fn snapshot(root: &Path) -> std::io::Result<BTreeMap<String, Option<Vec<u8>>>> {
        fn visit(
            root: &Path,
            current: &Path,
            out: &mut BTreeMap<String, Option<Vec<u8>>>,
        ) -> std::io::Result<()> {
            let mut entries = std::fs::read_dir(current)?.collect::<std::io::Result<Vec<_>>>()?;
            entries.sort_by_key(std::fs::DirEntry::file_name);
            for entry in entries {
                let path = entry.path();
                let relative = path
                    .strip_prefix(root)
                    .map_err(std::io::Error::other)?
                    .to_string_lossy()
                    .into_owned();
                if entry.file_type()?.is_dir() {
                    out.insert(relative, None);
                    visit(root, &path, out)?;
                } else {
                    out.insert(relative, Some(std::fs::read(path)?));
                }
            }
            Ok(())
        }

        let mut out = BTreeMap::new();
        visit(root, root, &mut out)?;
        Ok(out)
    }

    let repo = tempfile::tempdir()?;
    std::fs::create_dir_all(repo.path().join(".claude/agents"))?;
    std::fs::write(
        repo.path().join(".claude/agents/builder.md"),
        "---\nname: builder\ndescription: Builds code\n---\nBuild it.",
    )?;
    let preview = loadout_lib::import::translate::preview(repo.path())?;

    let probe = tempfile::tempdir()?;
    let moves = loadout_lib::import::apply::apply(probe.path(), &preview.draft)?
        .written
        .len();
    for fail_after in 1..=moves {
        let home = tempfile::tempdir()?;
        std::fs::write(home.path().join("keep.txt"), "untouched")?;
        std::fs::create_dir(home.path().join("existing"))?;
        std::fs::write(home.path().join("existing/data.bin"), [0_u8, 1, 2, 255])?;
        let before = snapshot(home.path())?;
        let result = loadout_lib::import::apply::apply_with_hook(
            home.path(),
            &preview.draft,
            |move_number| {
                if move_number == fail_after {
                    Err("injected failure".into())
                } else {
                    Ok(())
                }
            },
        );
        assert!(result.is_err());
        assert_eq!(
            snapshot(home.path())?,
            before,
            "failure after move {fail_after}"
        );
    }

    let home = tempfile::tempdir()?;
    let receipt = loadout_lib::import::apply::apply(home.path(), &preview.draft)?;
    assert!(
        receipt
            .written
            .iter()
            .any(|path| path.starts_with("agents"))
    );
    assert!(
        receipt
            .written
            .iter()
            .any(|path| path.starts_with("imports"))
    );
    Ok(())
}
