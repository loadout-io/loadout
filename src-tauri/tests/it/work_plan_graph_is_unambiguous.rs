#![allow(clippy::expect_used)]
// 2026-09-08 — dwa allow z zapisanym powodem, oba wylacznie w tym module testowym.
// `needless_pass_by_value`: pomocniki buduja drzewa JSON-a wprost z `json!`, wiec referencje
// kazalyby wolac je z ampersandem w kilkunastu miejscach, nie zmieniajac ani jednej asercji.
// `similar_names`: ten test z natury trzyma obok siebie `planned`/`planner` i `plan` —
// nazwy sa BLISKIE, bo opisuja sasiadujace pojecia tej samej sceny, a rozjechanie ich na
// sile („planned_document_bytes", „planning_step") czytaloby sie gorzej niz problem, ktory
// rozwiazuje. Bramka `suppressions` skanuje `src/` i `src-tauri/src/`, nie `tests/`.
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::similar_names)]
//! WP-02: ustawienie Plan ma jeden resolver dla zapisu, podglądu i Startu.
//!
//! Każdy błąd grafu może powstać w trzech miejscach: podczas zapisu szkicu, w kontroli
//! wyświetlanej przez okno i w bramie Startu. Pierwsze miejsce ma ostrzegać i zapisać, dwa
//! pozostałe mają odmówić. Trzy testy tabelaryczne niżej przechodzą tę samą listę stanów,
//! dzięki czemu dopisanie wyjątku tylko do jednego etapu nie daje fałszywej zieleni.

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::commands::workflow_plan::{plan_ready_to_start, resolve_workflow_plan_inner};
use loadout_lib::commands::workflows::{load_workflow_inner, save_workflow_inner};
use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::check::Level;
use loadout_lib::workflow::file::{
    CONTEXT_FORMAT, CURRENT, HIGHEST_SUPPORTED, PLAN_FORMAT, load_snapshot, save,
};
use loadout_lib::workflow::work_plan::authors;
use serde_json::{Value, json};

fn step(id: &str, name: &str, plan: Option<Value>) -> Value {
    let mut step = json!({
        "kind": "agent",
        "id": id,
        "name": name,
        "agent": "worker",
        "instructions": "Do this work.",
        "folder": { "use": "fresh-copy" }
    });
    if let Some(plan) = plan {
        step["plan"] = plan;
    }
    step
}

fn workflow(steps: Vec<Value>, links: Vec<Value>) -> WorkflowFile {
    serde_json::from_value(json!({
        "format": 1,
        "id": "work-plan-graph",
        "name": "Work plan graph",
        "steps": steps,
        "links": links
    }))
    .expect("the test workflow is valid")
}

fn link(from: &str, to: &str) -> Value {
    json!({ "from": from, "to": to })
}

fn included(file: &WorkflowFile) -> BTreeSet<&str> {
    file.steps
        .iter()
        .map(loadout_lib::workflow::Step::id)
        .collect()
}

fn two_unordered_authors() -> WorkflowFile {
    workflow(
        vec![
            step("prepare", "Prepare", None),
            step("planner", "Planner", Some(json!({ "mode": "create" }))),
            step(
                "designer",
                "Designer",
                Some(json!({ "mode": "update", "canUpdate": ["Design"] })),
            ),
            step("finish", "Finish", None),
        ],
        vec![
            link("prepare", "planner"),
            link("prepare", "designer"),
            link("planner", "finish"),
            link("designer", "finish"),
        ],
    )
}

struct BrokenGraph {
    slug: &'static str,
    file: WorkflowFile,
    words: Vec<&'static str>,
}

fn broken_graphs() -> Vec<BrokenGraph> {
    let missing_create = workflow(
        vec![step("consumer", "Consumer", Some(json!({ "mode": "use" })))],
        Vec::new(),
    );
    let second_create = workflow(
        vec![
            step("planner", "Planner", Some(json!({ "mode": "create" }))),
            step(
                "other-planner",
                "Other Planner",
                Some(json!({ "mode": "create" })),
            ),
        ],
        vec![link("planner", "other-planner")],
    );
    // Konfiguracje obu równoległych aktualizacji różnią się tylko zakresem. Rozłączne sekcje
    // nadal nie tworzą porządku publikacji (2026-09-08, WP-02).
    let unordered_scopes = workflow(
        vec![
            step("planner", "Planner", Some(json!({ "mode": "create" }))),
            step(
                "implementation",
                "Implementation writer",
                Some(json!({ "mode": "update", "canUpdate": ["Implementation"] })),
            ),
            step(
                "designer",
                "Design writer",
                Some(json!({ "mode": "update", "canUpdate": ["Design"] })),
            ),
        ],
        vec![
            link("planner", "implementation"),
            link("planner", "designer"),
        ],
    );
    let ambiguous_join = workflow(
        vec![
            step("planner", "Planner", Some(json!({ "mode": "create" }))),
            step("designer", "Designer", Some(json!({ "mode": "update" }))),
            step("relay", "Relay", Some(json!({ "mode": "use" }))),
            step("join", "Join", Some(json!({ "mode": "use" }))),
        ],
        vec![
            link("planner", "designer"),
            link("planner", "relay"),
            link("designer", "join"),
            link("relay", "join"),
        ],
    );
    let author_loop = workflow(
        vec![
            step("planner", "Loop Planner", Some(json!({ "mode": "create" }))),
            step("judge", "Judge", None),
        ],
        vec![
            link("planner", "judge"),
            json!({ "from": "judge", "to": "planner", "max_turns": 2 }),
        ],
    );
    let copied_author = workflow(
        vec![step(
            "planner",
            "Copied Planner",
            Some(json!({ "mode": "create" })),
        )],
        Vec::new(),
    );
    let mut copied_author = serde_json::to_value(copied_author).expect("workflow serializes");
    copied_author["steps"][0]["copies"] = json!(2);

    vec![
        BrokenGraph {
            slug: "missing-create",
            file: missing_create,
            words: vec!["Consumer", "no step creates"],
        },
        BrokenGraph {
            slug: "second-create",
            file: second_create,
            words: vec!["Planner", "Other Planner", "exactly one Create"],
        },
        BrokenGraph {
            slug: "unordered-scopes",
            file: unordered_scopes,
            words: vec!["Implementation writer", "Design writer", "must run before"],
        },
        BrokenGraph {
            slug: "ambiguous-join",
            file: ambiguous_join,
            words: vec!["Join", "Planner", "Designer", "Same plan as"],
        },
        BrokenGraph {
            slug: "author-loop",
            file: author_loop,
            words: vec!["Loop Planner", "retry loop"],
        },
        BrokenGraph {
            slug: "copied-author",
            file: serde_json::from_value(copied_author).expect("copied workflow is valid"),
            words: vec!["Copied Planner", "more than one copy"],
        },
    ]
}

fn says_all(message: &str, words: &[&str]) -> bool {
    words.iter().all(|word| message.contains(word))
}

#[tokio::test]
async fn two_unordered_authors_are_named_before_the_run_starts() {
    let notes = loadout_lib::ipc::check_workflow_in(
        tempfile::tempdir()
            .expect("isolated library")
            .path()
            .to_path_buf(),
        two_unordered_authors(),
    )
    .await;

    assert!(
        notes.iter().any(|note| {
            note.level == Level::Problem
                && note.message.contains("Planner")
                && note.message.contains("Designer")
                && note.message.contains("must run before")
        }),
        "the window did not name both unordered plan authors: {notes:?}"
    );
}

#[test]
fn every_unresolved_graph_is_saved_with_a_warning() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    for broken in broken_graphs() {
        let path = home.path().join(format!("{}.json", broken.slug));
        let notes = loadout_lib::workflow::check::check(&broken.file);
        assert!(
            notes.iter().any(|note| {
                note.level == Level::Warning && says_all(&note.message, &broken.words)
            }),
            "{} did not produce its named save warning: {notes:?}",
            broken.slug
        );
        save(&broken.file, &path, None)
            .map_err(|error| format!("{} was not saved as a draft: {error}", broken.slug))?;
        assert!(
            path.exists(),
            "{} reported a save without a file",
            broken.slug
        );
    }
    Ok(())
}

#[tokio::test]
async fn every_unresolved_graph_is_a_named_problem_in_the_window() {
    for broken in broken_graphs() {
        let notes = loadout_lib::ipc::check_workflow_in(
            tempfile::tempdir()
                .expect("isolated library")
                .path()
                .to_path_buf(),
            broken.file,
        )
        .await;
        assert!(
            notes.iter().any(|note| {
                note.level == Level::Problem && says_all(&note.message, &broken.words)
            }),
            "{} did not reach the window as a named problem: {notes:?}",
            broken.slug
        );
    }
}

#[test]
fn every_unresolved_graph_is_refused_by_the_start_gate() {
    for broken in broken_graphs() {
        let refusal = plan_ready_to_start(&broken.file, &included(&broken.file))
            .expect_err("Start accepted an unresolved Plan graph");
        assert!(
            says_all(&refusal.message, &broken.words),
            "{} reached Start with the wrong refusal: {refusal:?}",
            broken.slug
        );
    }
}

#[test]
fn same_plan_as_distinguishes_two_otherwise_identical_use_choices() {
    let file = workflow(
        vec![
            step("planner", "Planner", Some(json!({ "mode": "create" }))),
            step("designer", "Designer", Some(json!({ "mode": "update" }))),
            step(
                "use-created",
                "Use created",
                Some(json!({ "mode": "use", "samePlanAs": "planner" })),
            ),
            step(
                "use-updated",
                "Use updated",
                Some(json!({ "mode": "use", "samePlanAs": "designer" })),
            ),
        ],
        vec![
            link("planner", "designer"),
            link("designer", "use-created"),
            link("designer", "use-updated"),
        ],
    );

    let resolved = authors(&file).expect("the explicit sources are ordered and valid");

    assert_eq!(resolved.sources["use-created"], vec!["planner"]);
    assert_eq!(resolved.sources["use-updated"], vec!["designer"]);
}

#[test]
fn panel_source_choices_only_name_plan_steps_that_really_run_earlier() {
    let file = workflow(
        vec![
            step("planner", "Planner", Some(json!({ "mode": "create" }))),
            step("before", "Before", Some(json!({ "mode": "use" }))),
            step("beside", "Beside", Some(json!({ "mode": "use" }))),
        ],
        vec![link("planner", "before")],
    );

    let view = resolve_workflow_plan_inner(&file);
    let before = view
        .steps
        .iter()
        .find(|step| step.step_id == "before")
        .expect("the panel lost the selected step");

    assert_eq!(
        before
            .earlier
            .iter()
            .map(|source| source.step_id.as_str())
            .collect::<Vec<_>>(),
        vec!["planner"]
    );
    assert!(
        before
            .source
            .as_ref()
            .is_some_and(|source| source.said == "Takes the plan from Planner.")
    );
    assert!(
        !before
            .earlier
            .iter()
            .any(|source| source.step_id == "beside")
    );
}

#[test]
fn command_round_trip_keeps_plan_and_stamps_only_documents_that_need_it()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let plain = workflow(
        vec![step("plain", "Plain", Some(json!({ "mode": "off" })))],
        Vec::new(),
    );
    save_workflow_inner(home.path(), None, "plain.json", &plain, None)?;
    let plain_back = load_workflow_inner(home.path(), None, "plain.json")?;
    let plain_bytes = fs::read(home.path().join("workflows/plain.json"))?;
    let plain_json: Value = serde_json::from_slice(&plain_bytes)?;
    assert_eq!(plain_back.workflow.format, 1);
    assert!(plain_json["steps"][0].get("plan").is_none());
    let loadout_lib::workflow::Step::Agent(plain_step) = &plain_back.workflow.steps[0] else {
        return Err("the plain step changed kind".into());
    };
    assert_eq!(
        plain_step.work_plan()?.mode(),
        loadout_lib::work_plan::Mode::Off
    );

    let planned = workflow(
        vec![step(
            "planner",
            "Planner",
            Some(json!({ "mode": "create" })),
        )],
        Vec::new(),
    );
    save_workflow_inner(home.path(), None, "planned.json", &planned, None)?;
    let planned_back = load_workflow_inner(home.path(), None, "planned.json")?;
    let planned_bytes = fs::read(home.path().join("workflows/planned.json"))?;
    let planned_json: Value = serde_json::from_slice(&planned_bytes)?;
    assert_eq!(planned_json["format"], 3);
    let loadout_lib::workflow::Step::Agent(planner) = &planned_back.workflow.steps[0] else {
        return Err("the saved planner changed kind".into());
    };
    let plan = planner.work_plan()?;
    assert_eq!(plan.mode(), loadout_lib::work_plan::Mode::Create);
    Ok(())
}

#[test]
fn plan_has_its_own_format_and_a_newer_snapshot_is_refused_before_its_shape_is_read()
-> Result<(), Box<dyn Error>> {
    // Wartości przechodzą przez zwykłe wiązania, bo bramka słusznie odrzuca asercje liczone
    // wyłącznie na stałych podczas kompilacji (2026-09-08, uwaga WP-02).
    let plain = CURRENT;
    let context = CONTEXT_FORMAT;
    let plan = PLAN_FORMAT;
    let supported = HIGHEST_SUPPORTED;
    assert_eq!(plain, 1);
    assert!(context > plain);
    assert!(plan > context);
    assert_eq!(supported, plan);

    let future = json!({ "format": u64::from(plan) + 1, "unknown": true });
    let refusal = load_snapshot(Path::new("future.json"), &serde_json::to_vec(&future)?)
        .expect_err("a future workflow was guessed open")
        .to_string();
    assert_eq!(
        refusal,
        "This workflow was saved by a newer Loadout. Update Loadout to open it."
    );
    Ok(())
}

#[tokio::test]
async fn an_unknown_plan_mode_is_refused_at_save_window_and_start() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let file = workflow(
        vec![step(
            "planner",
            "Future Planner",
            Some(json!({ "mode": "invent" })),
        )],
        Vec::new(),
    );

    let path = home.path().join("unknown.json");
    let save_refusal = save(&file, &path, None)
        .expect_err("an unknown mode was saved")
        .to_string();
    let notes = loadout_lib::ipc::check_workflow_in(
        tempfile::tempdir()
            .expect("isolated library")
            .path()
            .to_path_buf(),
        file.clone(),
    )
    .await;
    let start_refusal = plan_ready_to_start(&file, &included(&file))
        .expect_err("Start treated an unknown Plan mode as Off");

    assert!(save_refusal.contains("Future Planner") && save_refusal.contains("does not know"));
    assert!(!path.exists());
    assert!(notes.iter().any(|note| {
        note.level == Level::Problem
            && note.message.contains("Future Planner")
            && note.message.contains("does not know")
    }));
    assert!(
        start_refusal.message.contains("Future Planner")
            && start_refusal.message.contains("does not know")
    );
    Ok(())
}
