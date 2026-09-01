//! Jedna rzecz to jeden wiersz i jeden plik, choćby leżała w trzech katalogach naraz.
//!
//! ZGŁOSZENIE WŁAŚCICIELA, 2026-08-29. Skan `meetnotes` postawił 67 wierszy, a rzeczy było
//! w nim około 43: 31 wierszy skilli to 17 skilli, 18 wierszy agentów to 11 agentów,
//! 16 notatek to 8. Reszta była tą samą treścią widzianą przez drugą aplikację — `.agents/`
//! obok `.claude/`, `.codex/` obok `.claude/`. Ekran to WIEDZIAŁ (pisał „This is another
//! app's copy of the same portable skill", „This app's copy differs from the native agent"),
//! a mimo to kazał rozstrzygać każdy plik osobno: **17 z 23 blokad było tą samą decyzją
//! zadaną dwa razy.**
//!
//! Widać było też drugą stronę tego samego: dwa wiersze `Ready` celowały w ten sam
//! `skills/github-actions/SKILL.md`, więc drugi nadpisałby pierwszy, a osiem notatek dzieliło
//! cel `memory/notes` — ścieżkę, której żaden powstały plik nie ma.
//!
//! Wzorzec nie jest nowy: `reconcile_workflow_targets` scala tak workflowy od 2026-08-22.
//! Ten zestaw rozciąga tę samą zasadę na resztę: **cel jest tożsamością rzeczy.**

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use loadout_lib::import::{ImportItem, ImportPreview, ImportStatus, ItemKind, translate};

const SHIP: &str = "---\nname: ship\ndescription: Ship the change.\n---\nShip the change.";
const AUDIT_HERE: &str = "---\nname: audit\ndescription: Audit it.\n---\nRead the code.";
const AUDIT_THERE: &str = "---\nname: audit\ndescription: Audit it.\n---\nRead the tests too.";
const MAIN_LOOP: &str = "# Main loop\n\nThe main loop keeps exactly one writer.\n";
const RETRIES: &str = "# Retries\n\nA retry never reuses the old folder.\n";

fn write(root: &Path, path: &str, content: &str) -> Result<(), Box<dyn std::error::Error>> {
    let file = root.join(path);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(file, content)?;
    Ok(())
}

/// Projekt, w którym każda rzecz leży w dwóch aplikacjach naraz — poza jedną notatką.
fn two_apps() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let repo = tempfile::tempdir()?;
    let root = repo.path();
    // Ten sam skill, co do bajta, w dwóch aplikacjach.
    write(root, ".agents/skills/ship/SKILL.md", SHIP)?;
    write(root, ".claude/skills/ship/SKILL.md", SHIP)?;
    // Ten sam skill, ale kopie się różnią — to jest prawdziwy wybór dla człowieka.
    write(root, ".agents/skills/audit/SKILL.md", AUDIT_HERE)?;
    write(root, ".claude/skills/audit/SKILL.md", AUDIT_THERE)?;
    // Ten sam agent, zapisany w dwóch formatach.
    write(root, ".claude/agents/builder.md", "Build the task.")?;
    write(
        root,
        ".codex/agents/builder.toml",
        "name = \"builder\"\ndeveloper_instructions = \"Build the task.\"\n",
    )?;
    // Ta sama notatka w dwóch aplikacjach, i druga notatka tylko w jednej.
    write(root, ".claude/learnings/main-loop.md", MAIN_LOOP)?;
    write(root, ".codex/learnings/main-loop.md", MAIN_LOOP)?;
    write(root, ".claude/learnings/retries.md", RETRIES)?;
    Ok(repo)
}

fn row_for<'a>(preview: &'a ImportPreview, target: &str) -> Option<&'a ImportItem> {
    preview
        .draft
        .items
        .iter()
        .find(|item| item.target.as_deref() == Some(Path::new(target)))
}

fn source_paths(item: &ImportItem) -> BTreeSet<PathBuf> {
    item.sources
        .iter()
        .map(|source| source.path.clone())
        .collect()
}

#[test]
fn the_same_thing_in_two_apps_is_one_row() -> Result<(), Box<dyn std::error::Error>> {
    let repo = two_apps()?;
    let preview = translate::preview(repo.path())?;

    // Kontrola przeciw pustej asercji: dziewięć plików weszło do skanu.
    assert_eq!(
        preview.snapshot.items.len(),
        9,
        "the scan read a different set of files than this fixture wrote"
    );

    let rows: Vec<_> = preview
        .draft
        .items
        .iter()
        .map(|item| item.target.clone())
        .collect();
    assert_eq!(
        preview.draft.items.len(),
        5,
        "nine files are five things — ship, audit, builder, main-loop, retries — and the plan \
         listed {rows:?}"
    );
    Ok(())
}

#[test]
fn no_two_rows_write_the_same_file() -> Result<(), Box<dyn std::error::Error>> {
    let repo = two_apps()?;
    let preview = translate::preview(repo.path())?;

    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    for item in &preview.draft.items {
        let Some(target) = &item.target else { continue };
        assert!(
            seen.insert(target.clone()),
            "{} is written by two rows at once, so one of them silently loses",
            target.display()
        );
    }
    assert!(!seen.is_empty(), "the plan writes nothing at all");
    Ok(())
}

#[test]
fn a_merged_row_still_names_every_place_it_came_from() -> Result<(), Box<dyn std::error::Error>> {
    let repo = two_apps()?;
    let preview = translate::preview(repo.path())?;

    let ship = row_for(&preview, "skills/ship/SKILL.md").ok_or("the ship skill left the plan")?;
    assert!(
        source_paths(ship).contains(Path::new(".agents/skills/ship/SKILL.md")),
        "the merged row forgot the copy it came from: {:?}",
        source_paths(ship)
    );
    assert!(
        source_paths(ship).contains(Path::new(".claude/skills/ship/SKILL.md")),
        "the merged row forgot the copy it came from: {:?}",
        source_paths(ship)
    );

    let builder =
        row_for(&preview, "agents/builder.md").ok_or("the builder agent left the plan")?;
    assert!(
        source_paths(builder).contains(Path::new(".codex/agents/builder.toml")),
        "the merged agent row forgot the second app: {:?}",
        source_paths(builder)
    );
    Ok(())
}

#[test]
fn merging_never_swallows_a_real_choice() -> Result<(), Box<dyn std::error::Error>> {
    let repo = two_apps()?;
    let preview = translate::preview(repo.path())?;

    let ship = row_for(&preview, "skills/ship/SKILL.md").ok_or("the ship skill left the plan")?;
    assert_eq!(
        ship.status,
        ImportStatus::Ready,
        "two copies that are the same thing should not ask a question: {}",
        ship.status_message
    );

    let audit =
        row_for(&preview, "skills/audit/SKILL.md").ok_or("the audit skill left the plan")?;
    assert_eq!(
        audit.status,
        ImportStatus::NeedsChoice,
        "two copies that differ still need a person to pick one"
    );
    Ok(())
}

#[test]
fn every_note_names_its_own_file() -> Result<(), Box<dyn std::error::Error>> {
    let repo = two_apps()?;
    let preview = translate::preview(repo.path())?;

    let notes: Vec<_> = preview
        .draft
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::Memory)
        .collect();
    assert_eq!(notes.len(), 2, "two notes, one of them written by two apps");

    for note in &notes {
        let target = note
            .target
            .as_deref()
            .ok_or("a note row promises no file at all")?;
        assert_ne!(
            target,
            Path::new("memory/notes"),
            "the row promises a folder, and no imported file will ever have that path"
        );
        assert_eq!(
            target.extension().and_then(|ext| ext.to_str()),
            Some("md"),
            "{} is not a note file",
            target.display()
        );
    }
    Ok(())
}
