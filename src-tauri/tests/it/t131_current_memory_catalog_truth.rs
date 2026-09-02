//! T-131 AC-1: the current catalog reports prompt omissions and typed provenance.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use loadout_lib::commands::memory::{NoteAddress, NotePlace, list_note_catalog_inner, notes_root};
use loadout_lib::memory::notes::{
    Budget, Kind, Note, NoteDraft, NoteId, Scope, Status, record_candidate_from_run, scan_notes,
    what_you_know,
};
use serde_json::Value;
use tempfile::TempDir;

const OPAQUE_ORIGIN: &str = "019b0131-aaaa-7bbb-8ccc-0123456789ab";
const IMPORTED_RULE: &str = "T131 imported memory keeps the literal project name";
const RUN_RULE: &str = "T131 a run suggested this exact rule";

struct CatalogTree {
    _temp: TempDir,
    library: PathBuf,
    project: PathBuf,
}

impl CatalogTree {
    fn new() -> Result<Self, Box<dyn Error>> {
        let temp = tempfile::tempdir()?;
        let library = temp.path().join("library-memory");
        let project = temp.path().join("project-memory");
        fs::create_dir_all(library.join("notes"))?;
        fs::create_dir_all(project.join("notes"))?;
        Ok(Self {
            _temp: temp,
            library,
            project,
        })
    }
}

fn scope_word(scope: Scope) -> &'static str {
    match scope {
        Scope::Everywhere => "everywhere",
        Scope::ThisProject => "this-project",
        Scope::ThisAgent => "this-agent",
    }
}

fn status_word(status: Status) -> &'static str {
    match status {
        Status::Suggested => "suggested",
        Status::InUse => "in-use",
    }
}

fn seed_note(
    root: &Path,
    id: &str,
    scope: Scope,
    status: Status,
    rule: &str,
    extra: &[(&str, &str)],
) -> Result<PathBuf, Box<dyn Error>> {
    let notes = root.join("notes");
    fs::create_dir_all(&notes)?;
    let mut front = format!(
        "---\nscope: {}\nkind: rule\ntitle: {id}\nrule: {rule}\nbecause: T131 fixture reason\nstatus: {}\noccurrences: 1\nmodified: 2026-08-26T10:00:00Z\nlast_used_at: null\n",
        scope_word(scope),
        status_word(status),
    );
    for (key, value) in extra {
        front.push_str(key);
        front.push_str(": ");
        front.push_str(value);
        front.push('\n');
    }
    front.push_str("---\n");
    let path = notes.join(format!("{id}.md"));
    fs::write(&path, front)?;
    Ok(path)
}

fn seed_budget_tree() -> Result<CatalogTree, Box<dyn Error>> {
    let tree = CatalogTree::new()?;
    seed_note(
        &tree.library,
        "a-library-too-long",
        Scope::Everywhere,
        Status::InUse,
        &"L".repeat(4_004),
        &[],
    )?;
    seed_note(
        &tree.library,
        "same",
        Scope::Everywhere,
        Status::InUse,
        "library twin fits",
        &[],
    )?;
    seed_note(
        &tree.library,
        "z-library-short",
        Scope::Everywhere,
        Status::InUse,
        "short fits after the long rule was left out",
        &[],
    )?;
    seed_note(
        &tree.project,
        "a-project-filler",
        Scope::ThisProject,
        Status::InUse,
        &"P".repeat(5_960),
        &[],
    )?;
    seed_note(
        &tree.project,
        "same",
        Scope::ThisProject,
        Status::InUse,
        &"T".repeat(80),
        &[],
    )?;
    seed_note(
        &tree.project,
        "z-project-short",
        Scope::ThisProject,
        Status::InUse,
        "tiny",
        &[],
    )?;
    seed_note(
        &tree.library,
        "a-forge-large",
        Scope::ThisAgent,
        Status::InUse,
        &"F".repeat(2_400),
        &[("agent", "Forge")],
    )?;
    seed_note(
        &tree.library,
        "b-forge-over-limit",
        Scope::ThisAgent,
        Status::InUse,
        &"f".repeat(1_000),
        &[("agent", "forge")],
    )?;
    seed_note(
        &tree.library,
        "a-scout-large",
        Scope::ThisAgent,
        Status::InUse,
        &"S".repeat(2_400),
        &[("agent", "Scout")],
    )?;
    seed_note(
        &tree.library,
        "legacy-project-note",
        Scope::ThisProject,
        Status::InUse,
        "legacy stays out until Move",
        &[],
    )?;
    seed_note(
        &tree.project,
        "misplaced-everywhere",
        Scope::Everywhere,
        Status::InUse,
        "a misplaced note does not gain a prompt claim",
        &[],
    )?;
    seed_note(
        &tree.library,
        "suggested-never-counted",
        Scope::Everywhere,
        Status::Suggested,
        &"C".repeat(8_000),
        &[],
    )?;
    Ok(tree)
}

fn ids(notes: &[Note], scope: Scope, owners: &[&str]) -> Vec<Note> {
    notes
        .iter()
        .filter(|note| {
            note.scope == scope
                && (owners.is_empty()
                    || note
                        .agent
                        .as_deref()
                        .is_some_and(|owner| owners.contains(&owner)))
        })
        .cloned()
        .collect()
}

fn words(ids: &[NoteId]) -> Vec<String> {
    ids.iter().map(ToString::to_string).collect()
}

fn files_in(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, Box<dyn Error>> {
    let mut files = BTreeMap::new();
    for entry in fs::read_dir(root.join("notes"))? {
        let path = entry?.path();
        files.insert(path.clone(), fs::read(path)?);
    }
    Ok(files)
}

fn catalog_values(
    library: &Path,
    project: &Path,
) -> Result<BTreeMap<(String, String), Value>, Box<dyn Error>> {
    let mut values = BTreeMap::new();
    for wire in list_note_catalog_inner(library, project)? {
        let value = serde_json::to_value(wire)?;
        let place = value
            .get("place")
            .and_then(Value::as_str)
            .ok_or("a catalog row has no serialized place")?
            .to_owned();
        let id = value
            .get("id")
            .and_then(Value::as_str)
            .ok_or("a catalog row has no serialized id")?
            .to_owned();
        values.insert((place, id), value);
    }
    Ok(values)
}

fn draft(title: &str, rule: &str) -> NoteDraft {
    NoteDraft {
        title: title.to_owned(),
        rule: rule.to_owned(),
        because: "T131 fixture reason".to_owned(),
        scope: Scope::ThisProject,
        kind: Kind::Rule,
        status: Status::Suggested,
        at: "2026-08-26T10:00:00Z".to_owned(),
    }
}

fn note_with_rule<'a>(notes: &'a [Note], rule: &str) -> Result<&'a Note, Box<dyn Error>> {
    notes
        .iter()
        .find(|note| note.rule == rule)
        .ok_or_else(|| format!("no note carries {rule:?}").into())
}

#[test]
fn block_receipts_prove_skip_and_continue_with_independent_owner_budgets()
-> Result<(), Box<dyn Error>> {
    let tree = seed_budget_tree()?;
    let library = scan_notes(&tree.library)?;
    let project = scan_notes(&tree.project)?;

    let everywhere = what_you_know(&library, Budget::of(Scope::Everywhere));
    assert_eq!(
        words(&everywhere.used),
        ["same", "z-library-short"],
        "a shorter later note still enters after an oversized note is dropped"
    );
    assert_eq!(words(&everywhere.dropped), ["a-library-too-long"]);

    let this_project = what_you_know(&project, Budget::of(Scope::ThisProject));
    assert_eq!(
        words(&this_project.used),
        ["a-project-filler", "z-project-short"]
    );
    assert_eq!(words(&this_project.dropped), ["same"]);

    let forge = ids(&library, Scope::ThisAgent, &["Forge", "forge"]);
    let forge = what_you_know(&forge, Budget::of(Scope::ThisAgent));
    assert_eq!(words(&forge.used), ["a-forge-large"]);
    assert_eq!(words(&forge.dropped), ["b-forge-over-limit"]);

    let scout = ids(&library, Scope::ThisAgent, &["Scout"]);
    let scout = what_you_know(&scout, Budget::of(Scope::ThisAgent));
    assert_eq!(words(&scout.used), ["a-scout-large"]);
    assert!(scout.dropped.is_empty());
    Ok(())
}

#[test]
fn catalog_marks_exact_addresses_without_writing_a_derived_fact_to_files()
-> Result<(), Box<dyn Error>> {
    let tree = seed_budget_tree()?;
    let before_library = files_in(&tree.library)?;
    let before_project = files_in(&tree.project)?;
    let values = catalog_values(&tree.library, &tree.project)?;
    assert_eq!(files_in(&tree.library)?, before_library);
    assert_eq!(files_in(&tree.project)?, before_project);

    let expected = BTreeMap::from([
        (("library", "a-library-too-long"), true),
        (("library", "same"), false),
        (("library", "z-library-short"), false),
        (("project", "a-project-filler"), false),
        (("project", "same"), true),
        (("project", "z-project-short"), false),
        (("library", "a-forge-large"), false),
        (("library", "b-forge-over-limit"), true),
        (("library", "a-scout-large"), false),
        (("library", "legacy-project-note"), false),
        (("project", "misplaced-everywhere"), false),
        (("library", "suggested-never-counted"), false),
    ]);
    let actual: BTreeMap<(&str, &str), Option<bool>> = values
        .iter()
        .map(|((place, id), value)| {
            (
                (place.as_str(), id.as_str()),
                value.get("leftOut").and_then(Value::as_bool),
            )
        })
        .collect();
    let expected: BTreeMap<(&str, &str), Option<bool>> = expected
        .into_iter()
        .map(|((place, id), left_out)| ((place, id), Some(left_out)))
        .collect();
    assert_eq!(
        actual, expected,
        "leftOut must be a literal bool on every row and belong to the full (place, id) address"
    );

    for bytes in before_library.values().chain(before_project.values()) {
        assert!(
            !String::from_utf8_lossy(bytes).contains("leftOut"),
            "leftOut is a current catalog fact, never front matter"
        );
    }
    Ok(())
}

#[test]
fn preview_and_apply_keep_a_uuid_shaped_project_separate_from_a_run() -> Result<(), Box<dyn Error>>
{
    let parent = tempfile::tempdir()?;
    let imported_project = parent.path().join(OPAQUE_ORIGIN);
    fs::create_dir_all(imported_project.join(".claude/learnings"))?;
    fs::write(
        imported_project.join(".claude/learnings/t131.md"),
        format!("# Imported origin\n\n{IMPORTED_RULE}.\n\nWhy: the typed field is the authority\n"),
    )?;
    let preview = loadout_lib::import::translate::preview(&imported_project)?;
    let library_home = tempfile::tempdir()?;
    loadout_lib::import::apply::apply(library_home.path(), &preview.draft)?;

    let library_root = notes_root(library_home.path());
    let imported_notes = scan_notes(&library_root)?;
    let imported = note_with_rule(&imported_notes, &format!("{IMPORTED_RULE}."))?;
    let imported_file = fs::read_to_string(&imported.path)?;

    let run_root = tempfile::tempdir()?;
    let suggested = record_candidate_from_run(
        run_root.path(),
        draft("T131 run origin", RUN_RULE),
        OPAQUE_ORIGIN,
    )?;
    let run_file = fs::read_to_string(&suggested.path)?;

    let empty_project = tempfile::tempdir()?;
    let imported_wire = catalog_values(&library_root, empty_project.path())?
        .into_values()
        .find(|value| {
            value.get("rule").and_then(Value::as_str) == Some(&format!("{IMPORTED_RULE}."))
        })
        .ok_or("the imported note did not reach the catalog")?;
    let run_wire = catalog_values(run_root.path(), empty_project.path())?
        .into_values()
        .find(|value| value.get("rule").and_then(Value::as_str) == Some(RUN_RULE))
        .ok_or("the run suggestion did not reach the catalog")?;

    assert_eq!(
        (
            imported_file.contains(&format!("project: {OPAQUE_ORIGIN}\n")),
            imported_file.contains("\nfrom:"),
            imported_wire.get("project"),
            imported_wire.get("from"),
        ),
        (
            true,
            false,
            Some(&Value::String(OPAQUE_ORIGIN.to_owned())),
            Some(&Value::Null)
        ),
        "the project basename is opaque data selected by the project field, never by UUID shape"
    );
    assert_eq!(
        (
            run_file.contains(&format!("from: {OPAQUE_ORIGIN}\n")),
            run_file.contains("\nproject:"),
            run_wire.get("project"),
            run_wire.get("from"),
        ),
        (
            true,
            false,
            Some(&Value::Null),
            Some(&Value::String(OPAQUE_ORIGIN.to_owned()))
        ),
        "the same literal means a run only when it came through the run field"
    );
    Ok(())
}

#[test]
fn legacy_origin_is_typed_by_companion_fields_and_reading_is_byte_preserving()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let imported_path = seed_note(
        root.path(),
        "legacy-import",
        Scope::ThisProject,
        Status::Suggested,
        "legacy import",
        &[
            ("from", OPAQUE_ORIGIN),
            ("source", ".claude/learnings/legacy.md"),
            ("source_hash", "legacy-hash"),
            ("app", "claude"),
        ],
    )?;
    let run_path = seed_note(
        root.path(),
        "legacy-run",
        Scope::ThisProject,
        Status::Suggested,
        "legacy run",
        &[("from", OPAQUE_ORIGIN)],
    )?;
    let before_import = fs::read(&imported_path)?;
    let before_run = fs::read(&run_path)?;
    let empty_project = tempfile::tempdir()?;
    let values = catalog_values(root.path(), empty_project.path())?;
    assert_eq!(fs::read(&imported_path)?, before_import);
    assert_eq!(fs::read(&run_path)?, before_run);

    let imported = values
        .get(&("library".to_owned(), "legacy-import".to_owned()))
        .ok_or("legacy imported note is absent")?;
    let run = values
        .get(&("library".to_owned(), "legacy-run".to_owned()))
        .ok_or("legacy run note is absent")?;
    assert_eq!(
        (imported.get("project"), imported.get("from")),
        (
            Some(&Value::String(OPAQUE_ORIGIN.to_owned())),
            Some(&Value::Null)
        )
    );
    assert_eq!(
        (run.get("project"), run.get("from")),
        (
            Some(&Value::Null),
            Some(&Value::String(OPAQUE_ORIGIN.to_owned()))
        )
    );
    Ok(())
}

#[test]
fn mutation_address_stays_exactly_place_and_id() -> Result<(), Box<dyn Error>> {
    let address = NoteAddress {
        place: NotePlace::Project,
        id: "same".to_owned(),
    };
    let value = serde_json::to_value(address)?;
    let keys: BTreeSet<&str> = value
        .as_object()
        .ok_or("the address did not serialize as an object")?
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, BTreeSet::from(["id", "place"]));
    Ok(())
}
