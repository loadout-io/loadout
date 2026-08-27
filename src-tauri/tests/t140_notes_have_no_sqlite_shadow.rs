//! AC-2 dla T-140: zapis notatki ma jeden plik i żadnego cienia w `SQLite`.

use std::fs;
use std::path::Path;

use loadout_lib::memory::notes::{Kind, NoteDraft, Scope, Status, record_candidate};

const TITLE: &str = "Files are the truth for notes";
const RULE: &str = "Read notes from their Markdown files.";
const BECAUSE: &str = "Deleting the disposable index must not delete project knowledge.";
const AT: &str = "2026-08-27T10:00:00Z";
const NOTE_FILE: &str = "notes/files-are-the-truth-for-notes.md";

const EXPECTED_MARKDOWN: &str = "---\n\
scope: this-project\n\
kind: fact\n\
title: Files are the truth for notes\n\
rule: Read notes from their Markdown files.\n\
because: Deleting the disposable index must not delete project knowledge.\n\
status: suggested\n\
occurrences: 1\n\
modified: 2026-08-27T10:00:00Z\n\
last_used_at: null\n\
---\n";

const NOTES_SOURCE: &str = include_str!("../src/memory/notes.rs");

fn every_file(dir: &Path, prefix: &str, out: &mut Vec<String>) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
        if entry.file_type()?.is_dir() {
            every_file(&entry.path(), &format!("{name}/"), out)?;
        } else {
            out.push(name);
        }
    }
    out.sort();
    Ok(())
}

/// Normalizuje wyłącznie nagłówek modułu, żeby łamanie wierszy rustfmtem nie było częścią
/// kontraktu dokumentacji.
fn normalized_module_header() -> String {
    let header = NOTES_SOURCE
        .split("\nuse std::collections")
        .next()
        .unwrap_or(NOTES_SOURCE);
    header
        .lines()
        .filter_map(|line| line.strip_prefix("//!"))
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[test]
fn a_note_is_one_markdown_file_and_the_module_says_files_are_the_only_truth() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    let note = record_candidate(
        root.path(),
        NoteDraft {
            title: TITLE.to_owned(),
            rule: RULE.to_owned(),
            because: BECAUSE.to_owned(),
            scope: Scope::ThisProject,
            kind: Kind::Fact,
            // Agent nie może sam zatwierdzić notatki; pole celowo dowodzi pełnego zapisu API.
            status: Status::InUse,
            at: AT.to_owned(),
        },
    )?;

    let markdown = fs::read_to_string(&note.path)?;
    assert_eq!(
        markdown, EXPECTED_MARKDOWN,
        "the public notes API did not leave the complete candidate Markdown on disk"
    );
    assert!(
        !root.path().join("loadout.db").exists(),
        "recording one note created loadout.db beside the file"
    );

    let mut files = Vec::new();
    every_file(root.path(), "", &mut files)?;
    assert_eq!(
        files,
        vec![NOTE_FILE.to_owned()],
        "the candidate content has another persisted copy. The temporary root contains {files:?}"
    );

    // Zachowanie plikowe wykonało się powyżej. Dzisiejsze `before` pada dopiero tutaj, na
    // fałszywym nagłówku modułu, a nie na braku targetu, modułu albo zapisu.
    let header = normalized_module_header();
    assert!(
        !header.contains("wiersz do `sqlite` wkłada `store::writer`"),
        "the notes module still claims that store::writer inserts a SQLite shadow row: {header}"
    );
    assert!(
        header
            .contains("pliki biblioteki i projektu są jedynym miejscem zapisu oraz źródłem prawdy"),
        "the notes module must say directly that library and project files are the only write \
         location and source of truth: {header}"
    );

    Ok(())
}
