//! T-139 AC-1: two roots, frozen bytes, wrapped reflection and cross-root tombstones.

use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::memory::{
    NoteAddress, NotePlace, NoteWire, discard_addressed_note_inner, list_notes_for_project_inner,
    move_note_to_project_inner, put_addressed_note_to_use_inner, stop_using_addressed_note_inner,
};
use loadout_lib::commands::run::{
    FrozenPromptHook, REFLECTION_BUDGET_USD, run_workflow_with_snapshot_hook,
};
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, StepSettings, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::{EvidenceIdentity, EvidenceStreams, EvidenceTarget};
use loadout_lib::ipc::{QUEUE_CAP, line_channel};
use loadout_lib::library::agents::Vendor;
use loadout_lib::memory::notes::{
    Error as NoteError, Kind, NoteDraft, NoteId, Origin, Scope, Status, record_candidate,
    record_imported, record_project_candidate_from_run, scan_notes,
};
use loadout_lib::store::Store;
use serde_json::Value;
use tempfile::TempDir;
use tokio::sync::mpsc;

const PATIENCE: Duration = Duration::from_secs(20);
const EVERYWHERE: &str = "T139-EVERYWHERE reaches both projects";
const AGENT_ONLY: &str = "T139-AGENT-ONLY reaches Builder";
const LEGACY: &str = "T139-LEGACY stays out until Move";
const STEP_ANSWER: &str = "## Answer\nDone.\n\n## Evidence\nfixture.rs:1\n\n## Open\nNone.\n";

const AGENT: &str = r"---
schema: 1
id: 01990000-0000-7000-8000-000000000139
name: T139 Builder
summary: Exercises the memory boundary
color: slate
runsWith: claude-code
model: haiku
thinking: balanced
fileAccess: look-only
giveUpAfterMinutes: 20
writeResultsTo: handoffs/result.md
tools: everything
skills: []
connections: []
---
Build the requested change.
";

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_t139_two_roots",
  "name": "T139 two roots",
  "steps": [{
    "kind": "agent", "id": "build", "name": "Build", "agent":
    "01990000-0000-7000-8000-000000000139", "overrides": {},
    "instructions": "Return the fixture result.", "folder": { "use": "project" },
    "at": { "x": 0, "y": 0 }
  }],
  "links": []
}"#;

#[test]
fn a_manual_import_is_not_suppressed_by_an_exact_tombstone() -> Result<(), Box<dyn StdError>> {
    let root = tempfile::tempdir()?;
    fs::create_dir_all(root.path().join("discarded"))?;
    fs::write(
        root.path()
            .join("discarded/imported-rule__2026-08-26T10-00-00Z.md"),
        b"old discarded bytes",
    )?;
    let imported = record_imported(
        root.path(),
        draft(
            "Imported rule",
            "T139 imported manually",
            Scope::ThisProject,
        ),
        None,
        &Origin {
            from: "Earlier Project".to_owned(),
            source: PathBuf::from("AGENTS.md"),
            source_hash: "fixture-hash".to_owned(),
            app: "claude".to_owned(),
        },
    )?;

    assert_eq!(imported.id.0, "imported-rule");
    assert!(fs::read_to_string(imported.path)?.contains("T139 imported manually"));
    Ok(())
}

#[test]
fn the_noncanonical_snapshot_fixture_is_a_real_readable_note() -> Result<(), Box<dyn StdError>> {
    let root = tempfile::tempdir()?;
    fs::create_dir_all(root.path().join("notes"))?;
    fs::write(
        root.path().join("notes/frozen.md"),
        noncanonical_note("T139-PROJECT-FIXTURE", "T139-SEEN-BEFORE-EDIT"),
    )?;

    let notes = scan_notes(root.path())?;
    assert_eq!(notes.len(), 1, "product parser rejected the byte fixture");
    assert!(notes[0].rule.contains("T139-SEEN-BEFORE-EDIT"));
    assert_eq!(
        notes[0].extra.get("unknown_key").map(String::as_str),
        Some("keep\tthis")
    );
    Ok(())
}

#[test]
fn catalog_is_an_exact_multiset_of_place_and_id_across_both_roots() -> Result<(), Box<dyn StdError>>
{
    let tree = Tree::new()?;
    seed_note(
        &tree.library,
        "same",
        Scope::Everywhere,
        Status::Suggested,
        "library same",
    )?;
    seed_note(
        &tree.project_a_memory(),
        "same",
        Scope::ThisProject,
        Status::Suggested,
        "project same",
    )?;
    seed_note(
        &tree.library,
        "library-only",
        Scope::Everywhere,
        Status::InUse,
        EVERYWHERE,
    )?;
    seed_note(
        &tree.project_a_memory(),
        "project-only",
        Scope::ThisProject,
        Status::InUse,
        "project A only",
    )?;

    let actual = list_notes_for_project_inner(&tree.library, tree.project_a.path())?;
    assert_eq!(
        multiset(&actual),
        BTreeMap::from([
            ((NotePlace::Library, "library-only".to_owned()), 1),
            ((NotePlace::Library, "same".to_owned()), 1),
            ((NotePlace::Project, "project-only".to_owned()), 1),
            ((NotePlace::Project, "same".to_owned()), 1),
        ])
    );
    Ok(())
}

#[test]
fn addressed_actions_change_one_physical_note_and_move_legacy_through_the_filesystem()
-> Result<(), Box<dyn StdError>> {
    let tree = Tree::new()?;
    seed_addressed_notes(&tree)?;
    let project_same = NoteAddress {
        place: NotePlace::Project,
        id: "same".to_owned(),
    };

    let used = accepted(put_addressed_note_to_use_inner(
        &tree.library,
        tree.project_a.path(),
        &project_same,
        "2026-08-26T10:10:00Z",
    ))?;
    assert_statuses(&used, Statuses::LibrarySuggestedProjectInUse)?;

    let stopped = accepted(stop_using_addressed_note_inner(
        &tree.library,
        tree.project_a.path(),
        &project_same,
        "2026-08-26T10:11:00Z",
    ))?;
    assert_statuses(&stopped, Statuses::BothSuggested)?;
    assert_discard_and_move(&tree, &project_same)?;
    Ok(())
}

fn seed_addressed_notes(tree: &Tree) -> Result<(), Box<dyn StdError>> {
    seed_note(
        &tree.library,
        "same",
        Scope::Everywhere,
        Status::Suggested,
        "library marker",
    )?;
    seed_note(
        &tree.project_a_memory(),
        "same",
        Scope::ThisProject,
        Status::Suggested,
        "project marker",
    )?;
    seed_note(
        &tree.library,
        "legacy",
        Scope::ThisProject,
        Status::Suggested,
        LEGACY,
    )?;
    Ok(())
}

fn assert_discard_and_move(
    tree: &Tree,
    project_same: &NoteAddress,
) -> Result<(), Box<dyn StdError>> {
    let discarded = accepted(discard_addressed_note_inner(
        &tree.library,
        tree.project_a.path(),
        project_same,
        "2026-08-26T10:12:00Z",
    ))?;
    assert!(has(&discarded, NotePlace::Library, "same"));
    assert!(!has(&discarded, NotePlace::Project, "same"));

    let source = tree.library.join("notes/legacy.md");
    let source_bytes = fs::read(&source)?;
    let moved = accepted(move_note_to_project_inner(
        &tree.library,
        tree.project_a.path(),
        &NoteAddress {
            place: NotePlace::Library,
            id: "legacy".to_owned(),
        },
    ))?;
    let target = tree.project_a_memory().join("notes/legacy.md");
    assert!(!source.exists());
    assert_eq!(fs::read(target)?, source_bytes);
    assert!(has(&moved, NotePlace::Project, "legacy"));
    Ok(())
}

#[test]
fn automatic_candidates_use_the_exact_tombstone_prefix_and_live_notes_win()
-> Result<(), Box<dyn StdError>> {
    let root = tempfile::tempdir()?;
    fs::create_dir_all(root.path().join("discarded"))?;
    fs::write(
        root.path()
            .join("discarded/similar-slug-extra__2026-08-26T10-30-00Z.md"),
        b"neighbor tombstone",
    )?;
    record_candidate(
        root.path(),
        draft(
            "Similar slug",
            "T139 similar slug must survive",
            Scope::ThisProject,
        ),
    )?;
    assert!(root.path().join("notes/similar-slug.md").exists());

    assert_exact_local_tombstone(root.path())?;
    assert_live_note_wins(root.path())?;
    Ok(())
}

fn assert_exact_local_tombstone(root: &Path) -> Result<(), Box<dyn StdError>> {
    fs::write(
        root.join("discarded/exact-slug__2026-08-26T10-31-00Z.md"),
        b"exact tombstone",
    )?;
    let refused = record_candidate(
        root,
        draft(
            "Exact slug",
            "T139 exact slug stays gone",
            Scope::ThisProject,
        ),
    );
    assert!(matches!(
        refused,
        Err(NoteError::PreviouslyDiscarded(NoteId(ref id))) if id == "exact-slug"
    ));
    assert!(!root.join("notes/exact-slug.md").exists());
    Ok(())
}

fn assert_live_note_wins(root: &Path) -> Result<(), Box<dyn StdError>> {
    seed_note(
        root,
        "live-wins",
        Scope::ThisProject,
        Status::Suggested,
        "the live note wins",
    )?;
    fs::write(
        root.join("discarded/live-wins__2026-08-26T10-32-00Z.md"),
        b"older tombstone",
    )?;
    let live = record_candidate(
        root,
        draft(
            "Live wins",
            "a repeated candidate cannot erase live bytes",
            Scope::ThisProject,
        ),
    )?;
    assert_eq!(live.occurrences, 2);
    assert_eq!(live.rule, "the live note wins");
    Ok(())
}

#[test]
fn an_exact_library_tombstone_blocks_a_project_candidate_without_touching_the_project()
-> Result<(), Box<dyn StdError>> {
    let roots = CandidateRoots::new()?;
    let expected = NoteId("blocked-by-library".to_owned());
    fs::create_dir_all(roots.library.path().join("discarded"))?;
    fs::write(
        roots
            .library
            .path()
            .join("discarded/blocked-by-library__2026-08-26T11-00-00Z.md"),
        b"library-only exact tombstone",
    )?;
    let before = snapshot_tree(roots.project.path())?;

    let refused = record_project_candidate_from_run(
        roots.library.path(),
        roots.project.path(),
        draft(
            "Blocked by library",
            "T139 must not recreate discarded library memory in the project",
            Scope::ThisProject,
        ),
        "run-t139-blocked",
    );

    assert!(matches!(refused, Err(NoteError::PreviouslyDiscarded(id)) if id == expected));
    assert_eq!(snapshot_tree(roots.project.path())?, before);
    assert!(!has_scanned_id(roots.project.path(), &expected)?);
    assert!(
        !roots
            .library
            .path()
            .join("notes/blocked-by-library.md")
            .exists()
    );
    assert!(
        !roots
            .project
            .path()
            .join("notes/blocked-by-library.md")
            .exists()
    );
    Ok(())
}

#[test]
fn a_neighbor_library_tombstone_allows_one_project_file_and_keeps_library_bytes()
-> Result<(), Box<dyn StdError>> {
    let roots = CandidateRoots::new()?;
    fs::create_dir_all(roots.library.path().join("discarded"))?;
    fs::write(
        roots
            .library
            .path()
            .join("discarded/similar-slug-extra__2026-08-26T11-01-00Z.md"),
        b"neighbor tombstone must stay byte-identical",
    )?;
    let before = snapshot_tree(roots.library.path())?;

    let saved = record_project_candidate_from_run(
        roots.library.path(),
        roots.project.path(),
        draft(
            "Similar slug",
            "T139 neighbor prefix must allow the project candidate",
            Scope::ThisProject,
        ),
        "run-t139-allowed",
    )?;

    assert_eq!(saved.id, NoteId("similar-slug".to_owned()));
    assert_eq!(scan_notes(roots.project.path())?.len(), 1);
    assert_eq!(notes_file_count(roots.project.path())?, 1);
    assert_eq!(snapshot_tree(roots.library.path())?, before);
    Ok(())
}

struct CandidateRoots {
    library: TempDir,
    project: TempDir,
}

impl CandidateRoots {
    fn new() -> Result<Self, io::Error> {
        let library = tempfile::tempdir()?;
        let project = tempfile::tempdir()?;
        Ok(Self { library, project })
    }
}

fn has_scanned_id(root: &Path, wanted: &NoteId) -> Result<bool, NoteError> {
    Ok(scan_notes(root)?.iter().any(|note| &note.id == wanted))
}

fn notes_file_count(root: &Path) -> Result<usize, io::Error> {
    match fs::read_dir(root.join("notes")) {
        Ok(entries) => Ok(entries
            .flatten()
            .filter(|entry| entry.path().is_file())
            .count()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error),
    }
}

fn snapshot_tree(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, io::Error> {
    let mut snapshot = BTreeMap::new();
    snapshot_dir(root, root, &mut snapshot)?;
    Ok(snapshot)
}

fn snapshot_dir(
    root: &Path,
    directory: &Path,
    snapshot: &mut BTreeMap<PathBuf, Vec<u8>>,
) -> Result<(), io::Error> {
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            snapshot_dir(root, &path, snapshot)?;
        } else if path.is_file() {
            let relative = path.strip_prefix(root).map_err(io::Error::other)?;
            snapshot.insert(relative.to_path_buf(), fs::read(path)?);
        }
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn frozen_prompt_bytes_drive_the_stamp_and_reflection_starts_once_per_project()
-> Result<(), Box<dyn StdError>> {
    let tree = Tree::new()?;
    tree.seed_product()?;
    seed_shared_run_notes(&tree)?;

    run_and_assert_project(
        &tree,
        tree.project_a.path(),
        "T139-PROJECT-A",
        "T139-PROJECT-B",
        "T139-REFLECTION-A",
    )
    .await?;
    run_and_assert_project(
        &tree,
        tree.project_b.path(),
        "T139-PROJECT-B",
        "T139-PROJECT-A",
        "T139-REFLECTION-B",
    )
    .await?;
    Ok(())
}

fn seed_shared_run_notes(tree: &Tree) -> Result<(), Box<dyn StdError>> {
    seed_note(
        &tree.library,
        "everywhere",
        Scope::Everywhere,
        Status::InUse,
        EVERYWHERE,
    )?;
    seed_agent_note(&tree.library, "agent-only", AGENT_ONLY)?;
    seed_note(
        &tree.library,
        "legacy",
        Scope::ThisProject,
        Status::InUse,
        LEGACY,
    )?;
    seed_note(
        &tree.project_a_memory(),
        "project-a",
        Scope::ThisProject,
        Status::InUse,
        "T139-PROJECT-A only",
    )?;
    seed_note(
        &tree.project_b_memory(),
        "project-b",
        Scope::ThisProject,
        Status::InUse,
        "T139-PROJECT-B only",
    )?;
    Ok(())
}

async fn run_and_assert_project(
    tree: &Tree,
    project: &Path,
    own: &str,
    other: &str,
    reflection: &str,
) -> Result<(), Box<dyn StdError>> {
    let fixture = FrozenFixture::plant(project, own)?;
    let seen = Arc::new(Seen::default());
    let observed = Arc::new(Mutex::new(None::<String>));
    let callbacks = Arc::new(AtomicUsize::new(0));
    let hook = fixture.hook(Arc::clone(&observed), Arc::clone(&callbacks));

    let report = run_fixture(
        tree,
        project,
        Arc::clone(&seen),
        None,
        reflection,
        Some(hook),
    )
    .await?;
    let frozen = observed
        .lock()
        .ok()
        .and_then(|slot| slot.clone())
        .ok_or("the frozen prompt callback did not publish its prompt")?;

    assert_run_prompt(&report, &seen, &frozen, own, other);
    assert_reflection_artifacts(&report, &seen)?;
    fixture.assert_stamp()?;
    assert_reflection_location(tree, project, reflection)?;
    assert_eq!(callbacks.load(Ordering::Acquire), 1);
    Ok(())
}

struct FrozenFixture {
    note_path: PathBuf,
    original: String,
    edited: String,
}

impl FrozenFixture {
    fn plant(project: &Path, own: &str) -> Result<Self, Box<dyn StdError>> {
        let original = noncanonical_note(own, "T139-SEEN-BEFORE-EDIT");
        let edited = original.replace("T139-SEEN-BEFORE-EDIT", "T139-EDITED-AFTER-PROMPT");
        let note_path = project.join(".loadout/memory/notes/frozen.md");
        fs::create_dir_all(note_path.parent().ok_or("frozen note has no parent")?)?;
        fs::write(&note_path, &original)?;
        Ok(Self {
            note_path,
            original,
            edited,
        })
    }

    fn hook(
        &self,
        observed: Arc<Mutex<Option<String>>>,
        callbacks: Arc<AtomicUsize>,
    ) -> FrozenPromptHook {
        let note_path = self.note_path.clone();
        let edited = self.edited.clone();
        Arc::new(move |prompt| {
            assert!(prompt.contains("T139-SEEN-BEFORE-EDIT"));
            callbacks.fetch_add(1, Ordering::AcqRel);
            if let Ok(mut slot) = observed.lock() {
                *slot = Some(prompt.to_owned());
            }
            let written = fs::write(&note_path, &edited);
            assert!(written.is_ok(), "snapshot hook could not edit its fixture");
            let changed = fs::read_to_string(&note_path).unwrap_or_default();
            assert!(changed.contains("T139-EDITED-AFTER-PROMPT"));
        })
    }

    fn assert_stamp(&self) -> Result<(), Box<dyn StdError>> {
        let stamped = fs::read_to_string(&self.note_path)?;
        assert_eq!(restore_null_stamp(&stamped)?, self.edited);
        assert!(stamped.contains("T139-EDITED-AFTER-PROMPT"));
        Ok(())
    }
}

fn assert_run_prompt(report: &RunReport, seen: &Seen, frozen: &str, own: &str, other: &str) {
    assert_eq!(report.steps, vec![StepState::Succeeded]);
    assert_eq!(seen.prompts(), vec![frozen.to_owned()]);
    assert!(frozen.contains(EVERYWHERE));
    assert!(frozen.contains(AGENT_ONLY));
    assert!(frozen.contains(own));
    assert!(!frozen.contains(other));
    assert!(!frozen.contains(LEGACY));
    assert!(frozen.contains("T139-SEEN-BEFORE-EDIT"));
    assert!(!frozen.contains("T139-EDITED-AFTER-PROMPT"));
}

fn assert_reflection_artifacts(report: &RunReport, seen: &Seen) -> Result<(), Box<dyn StdError>> {
    assert_eq!(
        seen.trace(),
        vec![
            Trace::Reflecting,
            Trace::Settings,
            Trace::Evidence,
            Trace::Budget,
            Trace::Start,
        ]
    );
    assert_eq!(seen.reflection_starts.load(Ordering::Acquire), 1);
    assert_reflection_receipt(report)?;
    for name in [
        "reflection.jsonl",
        "reflection.stderr.log",
        "reflection.input.json",
    ] {
        assert!(
            report.dir.join("logs").join(name).is_file(),
            "missing {name}"
        );
    }
    Ok(())
}

fn assert_reflection_location(
    tree: &Tree,
    project: &Path,
    reflection: &str,
) -> Result<(), Box<dyn StdError>> {
    let project_rules = rules(&project.join(".loadout/memory"))?;
    assert!(project_rules.iter().any(|rule| rule.contains(reflection)));
    assert!(
        !rules(&tree.library)?
            .iter()
            .any(|rule| rule.contains(reflection))
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_missing_reflection_wrapper_never_reaches_start_or_claims_it_ran()
-> Result<(), Box<dyn StdError>> {
    let tree = Tree::new()?;
    tree.seed_product()?;
    let seen = Arc::new(Seen::default());
    let marker = "T139-REFLECTION-WITHOUT-EVIDENCE";

    let report = run_fixture(
        &tree,
        tree.project_a.path(),
        Arc::clone(&seen),
        Some(Wrapper::Evidence),
        marker,
        None,
    )
    .await?;

    assert_eq!(seen.reflection_starts.load(Ordering::Acquire), 0);
    assert!(!seen.trace().contains(&Trace::Start));
    let run: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    assert_eq!(run.pointer("/reflection/ran"), Some(&Value::Bool(false)));
    assert!(
        !rules(&tree.project_a_memory())?
            .iter()
            .any(|rule| rule.contains(marker))
    );
    Ok(())
}

fn multiset(notes: &[NoteWire]) -> BTreeMap<(NotePlace, String), usize> {
    let mut counts = BTreeMap::new();
    for note in notes {
        *counts.entry((note.place, note.id.clone())).or_default() += 1;
    }
    counts
}

fn has(notes: &[NoteWire], place: NotePlace, id: &str) -> bool {
    notes
        .iter()
        .any(|note| note.place == place && note.id == id)
}

fn accepted<T>(result: Result<T, loadout_lib::commands::memory::NoteRefusal>) -> io::Result<T> {
    result.map_err(|refusal| io::Error::other(format!("addressed action refused: {refusal:?}")))
}

#[derive(Clone, Copy)]
enum Statuses {
    LibrarySuggestedProjectInUse,
    BothSuggested,
}

fn assert_statuses(notes: &[NoteWire], wanted: Statuses) -> Result<(), Box<dyn StdError>> {
    let library = notes
        .iter()
        .find(|note| note.place == NotePlace::Library && note.id == "same")
        .ok_or("the library duplicate vanished")?;
    let project = notes
        .iter()
        .find(|note| note.place == NotePlace::Project && note.id == "same")
        .ok_or("the project duplicate vanished")?;
    let expected = match wanted {
        Statuses::LibrarySuggestedProjectInUse => ("suggested", "in-use"),
        Statuses::BothSuggested => ("suggested", "suggested"),
    };
    assert_eq!((library.status.as_str(), project.status.as_str()), expected);
    Ok(())
}

fn draft(title: &str, rule: &str, scope: Scope) -> NoteDraft {
    NoteDraft {
        title: title.to_owned(),
        rule: rule.to_owned(),
        because: "the T-139 acceptance fixture observes it".to_owned(),
        scope,
        kind: Kind::Rule,
        status: Status::Suggested,
        at: "2026-08-26T10:00:00Z".to_owned(),
    }
}

fn seed_note(
    root: &Path,
    id: &str,
    scope: Scope,
    status: Status,
    rule: &str,
) -> Result<(), Box<dyn StdError>> {
    fs::create_dir_all(root.join("notes"))?;
    fs::write(
        root.join("notes").join(format!("{id}.md")),
        format!(
            "---\nscope: {}\nkind: rule\ntitle: {id}\nrule: {rule}\nbecause: the T-139 fixture observes it\nstatus: {}\noccurrences: 1\nmodified: 2026-08-26T10:00:00Z\nlast_used_at: null\n---\n\nfixture body\n",
            scope_word(scope),
            status_word(status),
        ),
    )?;
    Ok(())
}

fn seed_agent_note(root: &Path, id: &str, rule: &str) -> Result<(), Box<dyn StdError>> {
    fs::create_dir_all(root.join("notes"))?;
    fs::write(
        root.join("notes").join(format!("{id}.md")),
        format!(
            "---\nscope: this-agent\nagent: T139 Builder\nkind: rule\ntitle: {id}\nrule: {rule}\nbecause: the T-139 fixture observes it\nstatus: in-use\noccurrences: 1\nmodified: 2026-08-26T10:00:00Z\nlast_used_at: null\n---\n"
        ),
    )?;
    Ok(())
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

fn noncanonical_note(project_marker: &str, seen_marker: &str) -> String {
    format!(
        "---\nscope:   this-project\nkind:\trule\ntitle:  Frozen snapshot\nrule: {project_marker} {seen_marker}\nbecause:   byte preservation is observable\nstatus: in-use\noccurrences: 1\nunknown_key:   keep\tthis\n\nmodified: 2026-08-26T10:00:00Z\nlast_used_at: null\n---\n\nBody with  two spaces.\n\nLast paragraph.\n"
    )
}

fn restore_null_stamp(stamped: &str) -> Result<String, Box<dyn StdError>> {
    let line = stamped
        .lines()
        .find(|line| line.starts_with("last_used_at:"))
        .ok_or("the stamped file has no last_used_at line")?;
    assert_ne!(line, "last_used_at: null", "carried note was never stamped");
    Ok(stamped.replacen(line, "last_used_at: null", 1))
}

fn rules(root: &Path) -> Result<Vec<String>, Box<dyn StdError>> {
    Ok(scan_notes(root)?
        .into_iter()
        .map(|note| note.rule)
        .collect())
}

fn assert_reflection_receipt(report: &RunReport) -> Result<(), Box<dyn StdError>> {
    let run: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    assert_eq!(run.pointer("/reflection/ran"), Some(&Value::Bool(true)));
    assert_eq!(
        run.pointer("/reflection/kept").and_then(Value::as_u64),
        Some(1)
    );
    Ok(())
}

struct Tree {
    _root: TempDir,
    home: PathBuf,
    library: PathBuf,
    project_a: TempDir,
    project_b: TempDir,
}

impl Tree {
    fn new() -> Result<Self, Box<dyn StdError>> {
        let root = tempfile::tempdir()?;
        let home = root.path().join("home");
        let library = home.join("memory");
        fs::create_dir_all(&library)?;
        let project_a = tempfile::tempdir()?;
        let project_b = tempfile::tempdir()?;
        fs::create_dir_all(project_a.path().join(".loadout"))?;
        fs::create_dir_all(project_b.path().join(".loadout"))?;
        Ok(Self {
            _root: root,
            home,
            library,
            project_a,
            project_b,
        })
    }

    fn seed_product(&self) -> Result<(), Box<dyn StdError>> {
        fs::create_dir_all(self.home.join("agents"))?;
        fs::create_dir_all(self.home.join("workflows"))?;
        fs::write(self.home.join("agents/t139-builder.md"), AGENT)?;
        fs::write(self.workflow(), WORKFLOW)?;
        Ok(())
    }

    fn workflow(&self) -> PathBuf {
        self.home.join("workflows/t139.json")
    }

    fn project_a_memory(&self) -> PathBuf {
        self.project_a.path().join(".loadout/memory")
    }

    fn project_b_memory(&self) -> PathBuf {
        self.project_b.path().join(".loadout/memory")
    }
}

async fn run_fixture(
    tree: &Tree,
    project: &Path,
    seen: Arc<Seen>,
    missing: Option<Wrapper>,
    reflection_marker: &str,
    hook: Option<FrozenPromptHook>,
) -> Result<RunReport, Box<dyn StdError>> {
    fs::create_dir_all(project.join(".loadout"))?;
    let store = Store::open(&project.join(".loadout/loadout.db"))?;
    let deps = RunDeps {
        home: &tree.home,
        project,
        store: &store,
        drivers: fake_drivers(seen, missing, reflection_marker),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: tree.workflow(),
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (lines, _source) = line_channel(QUEUE_CAP);
    let report = tokio::time::timeout(
        PATIENCE,
        run_workflow_with_snapshot_hook(&deps, &request, lines, None, true, hook),
    )
    .await
    .map_err(|_| "the T-139 workflow timed out")??;
    Ok(report)
}

fn fake_drivers(seen: Arc<Seen>, missing: Option<Wrapper>, marker: &str) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake::step(seen, missing, marker));
    Arc::new(move |_vendor: Vendor| Arc::clone(&driver))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wrapper {
    Settings,
    Evidence,
    Budget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Step,
    Reflection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Trace {
    Reflecting,
    Settings,
    Evidence,
    Budget,
    Start,
}

#[derive(Default)]
struct Seen {
    trace: Mutex<Vec<Trace>>,
    prompts: Mutex<Vec<String>>,
    reflection_starts: AtomicUsize,
}

impl Seen {
    fn record(&self, one: Trace) {
        if let Ok(mut trace) = self.trace.lock() {
            trace.push(one);
        }
    }

    fn remember_prompt(&self, prompt: String) {
        if let Ok(mut prompts) = self.prompts.lock() {
            prompts.push(prompt);
        }
    }

    fn trace(&self) -> Vec<Trace> {
        self.trace
            .lock()
            .map(|trace| trace.clone())
            .unwrap_or_default()
    }

    fn prompts(&self) -> Vec<String> {
        self.prompts
            .lock()
            .map(|prompts| prompts.clone())
            .unwrap_or_default()
    }
}

#[derive(Clone)]
struct Fake {
    mode: Mode,
    seen: Arc<Seen>,
    missing: Option<Wrapper>,
    reflection_answer: String,
    settings: Option<StepSettings>,
    evidence: Option<EvidenceTarget>,
    budget: Option<f64>,
}

impl Fake {
    fn step(seen: Arc<Seen>, missing: Option<Wrapper>, marker: &str) -> Self {
        Self {
            mode: Mode::Step,
            seen,
            missing,
            reflection_answer: format!(
                "rule: {marker} keep project memory private\nbecause: the run handoff proves it\n"
            ),
            settings: None,
            evidence: None,
            budget: None,
        }
    }

    fn validate_reflection(&self, spec: &RunSpec) -> anyhow::Result<()> {
        let settings = self
            .settings
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("reflection has no private settings"))?;
        let evidence = self
            .evidence
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("reflection has no evidence target"))?;
        if settings.dir != spec.cwd
            || settings.work_key != "_reflection"
            || settings.memory != spec.cwd.join("mem/_reflection")
        {
            return Err(anyhow::anyhow!(
                "reflection settings do not own _reflection"
            ));
        }
        if evidence.root() != spec.cwd
            || !matches!(evidence.identity(), EvidenceIdentity::Reflection)
        {
            return Err(anyhow::anyhow!(
                "reflection has the wrong evidence identity"
            ));
        }
        if self.budget != Some(REFLECTION_BUDGET_USD) {
            return Err(anyhow::anyhow!("reflection has the wrong price ceiling"));
        }
        Ok(())
    }

    async fn write_evidence(&self) -> anyhow::Result<()> {
        let target = self
            .evidence
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("started turn has no evidence target"))?;
        let EvidenceStreams {
            mut stdout,
            mut stderr,
        } = target.open().await?;
        stdout.write(b"{\"type\":\"t139-fixture\"}\n").await?;
        stderr.write(b"t139 fixture stderr\n").await?;
        stdout.close().await?;
        stderr.close().await?;
        Ok(())
    }
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        "t139-fake"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("t139".to_owned()),
        })
    }

    fn reflecting(&self) -> Option<Arc<dyn AgentDriver>> {
        self.seen.record(Trace::Reflecting);
        Some(Arc::new(Self {
            mode: Mode::Reflection,
            seen: Arc::clone(&self.seen),
            missing: self.missing,
            reflection_answer: self.reflection_answer.clone(),
            settings: None,
            evidence: None,
            budget: None,
        }))
    }

    fn with_settings(
        &self,
        settings: &StepSettings,
    ) -> Option<anyhow::Result<Arc<dyn AgentDriver>>> {
        if self.mode == Mode::Reflection {
            self.seen.record(Trace::Settings);
            if self.missing == Some(Wrapper::Settings) {
                return Some(Err(anyhow::anyhow!("injected settings refusal")));
            }
        }
        let mut next = self.clone();
        next.settings = Some(settings.clone());
        Some(Ok(Arc::new(next)))
    }

    fn with_evidence(&self, target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        if self.mode == Mode::Reflection {
            self.seen.record(Trace::Evidence);
            if self.missing == Some(Wrapper::Evidence) {
                return None;
            }
        }
        let mut next = self.clone();
        next.evidence = Some(target);
        Some(Arc::new(next))
    }

    fn with_budget(&self, dollars: f64) -> Option<Arc<dyn AgentDriver>> {
        if self.mode == Mode::Reflection {
            self.seen.record(Trace::Budget);
            if self.missing == Some(Wrapper::Budget) {
                return None;
            }
        }
        let mut next = self.clone();
        next.budget = Some(dollars);
        Some(Arc::new(next))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let text = match self.mode {
            Mode::Step => {
                self.seen.remember_prompt(spec.prompt.clone());
                STEP_ANSWER.to_owned()
            }
            Mode::Reflection => {
                self.validate_reflection(&spec)?;
                self.seen.record(Trace::Start);
                self.seen.reflection_starts.fetch_add(1, Ordering::AcqRel);
                self.reflection_answer.clone()
            }
        };
        self.write_evidence().await?;
        let session = SessionRef {
            vendor: "t139-fake",
            id: spec.run_id.to_string(),
        };
        let _ = events
            .send(
                AgentEvent::Started {
                    session: session.clone(),
                    model: spec.model.unwrap_or_default(),
                    tools: Vec::new(),
                    capabilities: Vec::new(),
                }
                .into(),
            )
            .await;
        Ok(Box::new(Turn {
            events,
            session,
            text,
            cost: (self.mode == Mode::Reflection).then_some(0.031),
        }))
    }
}

struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    text: String,
    cost: Option<f64>,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: self.text.clone(),
            cost_usd: self.cost,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(2),
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send(AgentEvent::Finished(outcome.clone()).into())
            .await;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}
