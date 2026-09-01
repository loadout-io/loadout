//! T-78 AC-2: receipt jest odwracalnym powiązaniem celu z prawdziwym źródłem.

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;

use loadout_lib::import::apply::ImportReceipt;

fn fingerprint(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[test]
fn every_written_target_names_its_source_and_both_real_hashes() -> Result<(), Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    fs::create_dir_all(repo.path().join(".claude/agents"))?;
    fs::write(
        repo.path().join(".claude/agents/builder.md"),
        "---\nname: builder\ndescription: Builds\n---\nBuild the task.\n",
    )?;
    let skill = repo.path().join(".agents/skills/project-guide");
    fs::create_dir_all(skill.join("scripts"))?;
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: project-guide\ndescription: Explains the project\n---\nFollow the checks.\n",
    )?;
    fs::write(skill.join("scripts/check.sh"), "exit 0\n")?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;
    assert!(
        preview.draft.runnable(),
        "the receipt fixture must be importable"
    );
    let receipt = loadout_lib::import::apply::apply(home.path(), &preview.draft)?;

    let receipt_path = std::path::PathBuf::from("imports").join(format!("{}.json", receipt.id));
    let without_provenance: Vec<_> = receipt
        .written
        .iter()
        .filter(|target| !receipt.files.contains_key(*target))
        .cloned()
        .collect();
    assert_eq!(
        without_provenance,
        vec![receipt_path.clone()],
        "the administrative receipt is the only written path without provenance"
    );

    /* `written` historycznie zawiera też sam plik receipt i T-75 zamraża ten kontrakt. Nie jest
     * on plikiem zaimportowanym ze źródłowego repo (ani nie może nieść własnego hasha bez
     * samoodwołania), więc provenance obejmuje każdy POZOSTAŁY wpis. */
    let written: BTreeSet<_> = receipt
        .written
        .iter()
        .filter(|target| *target != &receipt_path)
        .cloned()
        .collect();
    let mapped: BTreeSet<_> = receipt.files.keys().cloned().collect();
    assert_eq!(
        mapped, written,
        "a file reported as written without provenance makes reimport unsafe"
    );
    assert!(receipt.written.contains(&receipt_path));

    for target in &written {
        let provenance = receipt
            .files
            .get(target)
            .ok_or_else(|| format!("{} has no provenance", target.display()))?;
        assert!(!provenance.source_path.is_absolute());
        let source = repo.path().join(&provenance.source_path);
        assert!(
            source.is_file(),
            "{} is not a source file",
            source.display()
        );
        assert_eq!(provenance.source_hash, fingerprint(&fs::read(source)?));
        assert_eq!(
            provenance.written_hash,
            fingerprint(&fs::read(home.path().join(target))?),
            "the target hash must come from the bytes that landed on disk"
        );
    }

    let persisted: ImportReceipt =
        serde_json::from_slice(&fs::read(home.path().join(&receipt_path))?)?;
    assert_eq!(persisted.files, receipt.files);
    assert_eq!(persisted.written, receipt.written);
    Ok(())
}
