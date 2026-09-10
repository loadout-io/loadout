#![allow(clippy::expect_used)]
// 2026-09-08 — pomocniki tego modułu biorą `Vec<Value>` z rozmysłu: budują drzewa JSON-a
// wprost z `json!` i przekazanie ich przez referencję kazałoby wołać je z `&` w kilkunastu
// miejscach, nie zmieniając ani jednej asercji. Bramka `suppressions` skanuje `src/`
// i `src-tauri/src/`, nie `tests/`, więc ten allow niczego nie wycisza w kodzie produktu.
#![allow(clippy::needless_pass_by_value)]
//! CT-05: dokładne przypięcia mają jeden resolver dla podglądu, walidacji i Startu.

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::commands::context::create_context_set_inner;
use loadout_lib::commands::workflow_context::{
    context_ready_to_start, resolve_workflow_context_inner,
};
use loadout_lib::context::files::{folder_of, library_root};
use loadout_lib::context::{ContextRevision, ContextTopic};
use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::context::{Topics, effective_for, validate};
use loadout_lib::workflow::execution::{ContextScope, RunInputs};
use loadout_lib::workflow::file::{CURRENT, HIGHEST_SUPPORTED, load_snapshot, save};
use serde_json::{Value, json};

fn workflow(workflow_context: Option<Value>, step_context: Option<Value>) -> WorkflowFile {
    let mut document = json!({
        "format": 1,
        "id": "checkout",
        "name": "Checkout",
        "steps": [{
            "kind": "agent",
            "id": "frontend",
            "name": "Frontend",
            "agent": "frontend",
            "instructions": "Build checkout."
        }],
        "links": []
    });
    if let Some(context) = workflow_context {
        document["context"] = context;
    }
    if let Some(context) = step_context {
        document["steps"][0]["context"] = context;
    }
    serde_json::from_value(document).expect("the test workflow is valid")
}

fn pin(id: &str, revision: &str, topics: Value) -> Value {
    json!({ "id": id, "revision": revision, "topics": topics })
}

fn shared(pins: Vec<Value>) -> Value {
    json!({ "schema": 1, "sets": pins })
}

fn local(inherit: Option<bool>, exclude: Vec<&str>, pins: Vec<Value>) -> Value {
    let mut context = json!({ "schema": 1, "exclude": exclude, "sets": pins });
    if let Some(inherit) = inherit {
        context["inheritWorkflow"] = json!(inherit);
    }
    context
}

fn included() -> BTreeSet<&'static str> {
    BTreeSet::from(["frontend"])
}

fn create_set(home: &Path, title: &str) -> Result<String, Box<dyn Error>> {
    Ok(create_context_set_inner(home, title)?.set.id)
}

fn publish_revision(
    home: &Path,
    set_id: &str,
    revision_id: &str,
    topics: &[(&str, &str)],
    index: &str,
) -> Result<(), Box<dyn Error>> {
    let library = library_root(home);
    let folder = folder_of(&library, set_id)?;
    let version = folder.join("versions").join(revision_id);
    fs::create_dir_all(version.join("topics"))?;
    fs::write(version.join("index.md"), index)?;
    fs::write(version.join("findings.json"), b"[]\n")?;
    for (id, title) in topics {
        fs::write(
            version.join(format!("topics/{id}.md")),
            format!("# {title}\n"),
        )?;
    }
    let revision = ContextRevision {
        schema: 1,
        id: revision_id.to_owned(),
        set_id: set_id.to_owned(),
        topics: topics
            .iter()
            .map(|(id, title)| ContextTopic {
                id: (*id).to_owned(),
                title: (*title).to_owned(),
            })
            .collect(),
        index_file: "index.md".to_owned(),
        findings_file: "findings.json".to_owned(),
        topic_files: topics
            .iter()
            .map(|(id, _)| format!("topics/{id}.md"))
            .collect(),
        ..ContextRevision::default()
    };
    fs::write(
        version.join("manifest.json"),
        serde_json::to_vec_pretty(&revision)?,
    )?;
    let manifest = folder.join("manifest.json");
    let mut definition: loadout_lib::context::ContextSet =
        serde_json::from_slice(&fs::read(&manifest)?)?;
    definition.latest_ready_revision = Some(revision_id.to_owned());
    fs::write(manifest, serde_json::to_vec_pretty(&definition)?)?;
    Ok(())
}

#[test]
fn a_step_that_narrows_a_shared_set_gets_only_the_topics_it_named() -> Result<(), Box<dyn Error>> {
    let workflow: WorkflowFile = serde_json::from_str(
        r#"{
          "format": 2,
          "id": "checkout",
          "name": "Checkout",
          "context": {
            "schema": 1,
            "sets": [{
              "id": "S",
              "revision": "r1",
              "topics": ["checkout", "returns"]
            }]
          },
          "steps": [{
            "kind": "agent",
            "id": "frontend",
            "name": "Frontend",
            "agent": "frontend",
            "context": {
              "schema": 1,
              "inheritWorkflow": true,
              "exclude": [],
              "sets": [{
                "id": "S",
                "revision": "r1",
                "topics": ["checkout"]
              }]
            }
          }],
          "links": []
        }"#,
    )?;

    let effective = effective_for(&workflow, "frontend", &RunInputs::default())?;

    assert_eq!(effective.sets.len(), 1);
    assert_eq!(
        effective.sets[0].topics,
        Topics::Only(vec!["checkout".to_owned()]),
        "the step's topic choice replaces the inherited choice instead of adding to it"
    );
    Ok(())
}

#[test]
fn shared_sets_are_inherited_then_excluded_and_local_sets_are_added() -> Result<(), Box<dyn Error>>
{
    let file = workflow(
        Some(shared(vec![
            pin("shared", "r1", json!("all")),
            pin("excluded", "r1", json!("all")),
        ])),
        Some(local(
            Some(true),
            vec!["excluded"],
            vec![pin("local", "r1", json!(["one"]))],
        )),
    );

    let effective = effective_for(&file, "frontend", &RunInputs::default())?;

    assert_eq!(
        effective
            .sets
            .iter()
            .map(|one| one.id.as_str())
            .collect::<Vec<_>>(),
        vec!["shared", "local"],
        "inheritance, exclusion and local selection must happen in the documented order"
    );
    Ok(())
}

#[test]
fn inheritance_can_be_turned_off_without_losing_local_context() -> Result<(), Box<dyn Error>> {
    let file = workflow(
        Some(shared(vec![pin("shared", "r1", json!("all"))])),
        Some(local(
            Some(false),
            Vec::new(),
            vec![pin("local", "r1", json!("all"))],
        )),
    );

    let effective = effective_for(&file, "frontend", &RunInputs::default())?;

    assert!(!effective.inherits_workflow);
    assert_eq!(effective.sets[0].id, "local");
    assert_eq!(effective.sets.len(), 1);
    Ok(())
}

#[test]
fn protected_scope_needs_explicit_inheritance_but_keeps_local_pins() -> Result<(), Box<dyn Error>> {
    let mut inputs = RunInputs::default();
    inputs.contexts.insert(
        "protected".to_owned(),
        ContextScope {
            task: "Build checkout".to_owned(),
            workspace_seed: None,
            instructions: String::new(),
            project_instructions: false,
            project_memory: false,
        },
    );
    inputs
        .step_contexts
        .insert("frontend".to_owned(), "protected".to_owned());
    let implicit = workflow(
        Some(shared(vec![pin("shared", "r1", json!("all"))])),
        Some(local(
            None,
            Vec::new(),
            vec![pin("local", "r1", json!("all"))],
        )),
    );
    let explicit = workflow(
        Some(shared(vec![pin("shared", "r1", json!("all"))])),
        Some(local(
            Some(true),
            Vec::new(),
            vec![pin("local", "r1", json!("all"))],
        )),
    );

    let implicit = effective_for(&implicit, "frontend", &inputs)?;
    let explicit = effective_for(&explicit, "frontend", &inputs)?;

    assert!(implicit.protected_scope);
    assert!(!implicit.inherits_workflow);
    assert_eq!(
        implicit
            .sets
            .iter()
            .map(|one| one.id.as_str())
            .collect::<Vec<_>>(),
        vec!["local"]
    );
    assert_eq!(
        explicit
            .sets
            .iter()
            .map(|one| one.id.as_str())
            .collect::<Vec<_>>(),
        vec!["shared", "local"],
        "explicit consent must add the shared set without dropping the protected step's local set"
    );
    Ok(())
}

#[test]
fn duplicate_ids_and_mixed_revisions_are_named_refusals() {
    let duplicate = workflow(
        Some(shared(vec![
            pin("S", "r1", json!("all")),
            pin("S", "r1", json!(["checkout"])),
        ])),
        None,
    );
    let conflict = workflow(
        Some(shared(vec![pin("S", "r1", json!("all"))])),
        Some(local(
            Some(true),
            Vec::new(),
            vec![pin("S", "r2", json!("all"))],
        )),
    );

    let duplicate = validate(&duplicate).expect_err("the duplicate was accepted");
    let conflict = validate(&conflict).expect_err("two versions were accepted");

    assert!(duplicate.contains('S') && duplicate.contains("more than once"));
    assert!(conflict.contains('S') && conflict.contains("r1") && conflict.contains("r2"));
}

#[test]
fn malformed_shape_and_unsafe_identifiers_are_refused_before_disk_lookup() {
    let malformed = workflow(
        Some(json!({ "schema": 1, "sets": [], "surprise": true })),
        None,
    );
    let unsafe_id = workflow(
        Some(shared(vec![pin("../outside", "r1", json!("all"))])),
        None,
    );

    assert!(
        validate(&malformed)
            .expect_err("an unknown Context field was accepted")
            .contains("cannot be read")
    );
    assert!(
        validate(&unsafe_id)
            .expect_err("a path-shaped set identifier was accepted")
            .contains("unsupported set or version identifier")
    );
}

#[tokio::test]
async fn a_graph_sent_directly_to_the_ipc_validator_gets_the_same_context_refusal() {
    let conflict = workflow(
        Some(shared(vec![pin("S", "r1", json!("all"))])),
        Some(local(
            Some(true),
            Vec::new(),
            vec![pin("S", "r2", json!("all"))],
        )),
    );

    let notes = loadout_lib::ipc::check_workflow_in(
        tempfile::tempdir()
            .expect("isolated library")
            .path()
            .to_path_buf(),
        conflict,
    )
    .await;

    assert!(
        notes.iter().any(|note| note
            .message
            .contains("Context set S uses versions r1 and r2")),
        "the IPC command accepted a graph that the file path refuses: {notes:?}"
    );
}

#[test]
fn non_agent_steps_never_inherit_context_and_cannot_carry_a_context_field()
-> Result<(), Box<dyn Error>> {
    for (kind, fields) in [
        ("checkpoint", json!({ "question": "Continue?" })),
        (
            "check",
            json!({ "command": "true", "expect": "It passes." }),
        ),
        ("serve", json!({ "command": "npm run dev" })),
    ] {
        let mut value = serde_json::to_value(workflow(
            Some(shared(vec![pin("S", "r1", json!("all"))])),
            None,
        ))?;
        value["steps"][0] = json!({
            "kind": kind,
            "id": "not-agent",
            "name": "Not an agent",
            "at": { "x": 0, "y": 0 }
        });
        for (key, field) in fields
            .as_object()
            .ok_or("fixture fields are not an object")?
        {
            value["steps"][0][key] = field.clone();
        }
        let file: WorkflowFile = serde_json::from_value(value.clone())?;
        assert!(
            effective_for(&file, "not-agent", &RunInputs::default())?
                .sets
                .is_empty()
        );

        value["steps"][0]["context"] = local(Some(true), Vec::new(), Vec::new());
        let invalid: WorkflowFile = serde_json::from_value(value)?;
        let said = validate(&invalid).expect_err("a non-agent context field was accepted");
        assert!(said.contains("Not an agent") && said.contains("only agent steps"));
    }
    Ok(())
}

#[test]
fn formats_one_and_two_both_load_from_a_snapshot_without_rewriting_it() -> Result<(), Box<dyn Error>>
{
    let one = serde_json::to_vec(&workflow(None, None))?;
    let mut two = workflow(Some(shared(vec![pin("S", "r1", json!("all"))])), None);
    two.format = 2;
    let two = serde_json::to_vec(&two)?;

    let loaded_one =
        load_snapshot(Path::new("one.json"), &one).map_err(|error| error.to_string())?;
    let loaded_two =
        load_snapshot(Path::new("two.json"), &two).map_err(|error| error.to_string())?;

    assert_eq!(loaded_one.format, 1);
    assert!(loaded_one.context()?.is_none());
    assert_eq!(loaded_two.format, 2);
    assert_eq!(
        loaded_two.context()?.expect("format 2 lost Context").sets[0].id,
        "S"
    );
    Ok(())
}

#[test]
fn saving_context_upgrades_once_with_a_backup_and_plain_documents_stay_at_one()
-> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let plain_path = dir.path().join("plain.json");
    let plain = workflow(
        Some(shared(Vec::new())),
        Some(local(None, Vec::new(), Vec::new())),
    );
    let plain_revision = save(&plain, &plain_path, None).map_err(|error| error.to_string())?;
    let plain_bytes = fs::read(&plain_path)?;
    let plain_written = serde_json::from_slice::<Value>(&plain_bytes)?;
    assert_eq!(plain_written["format"], 1);
    assert!(plain_written.get("context").is_none());
    assert!(plain_written["steps"][0].get("context").is_none());
    assert!(!plain_path.with_extension("json.bak").exists());

    let with_context = workflow(Some(shared(vec![pin("S", "r1", json!("all"))])), None);
    save(&with_context, &plain_path, Some(&plain_revision)).map_err(|error| error.to_string())?;
    let upgraded = fs::read(&plain_path)?;

    assert_eq!(serde_json::from_slice::<Value>(&upgraded)?["format"], 2);
    assert_eq!(
        fs::read(plain_path.with_extension("json.bak"))?,
        plain_bytes
    );
    assert_eq!(CURRENT, 1, "plain constructors must keep writing format 1");
    // 2026-09-08 — sprawdzenie zostaje, zmienia sie tylko forma: `const { assert!(…) }` nad
    // dwiema stalymi to `clippy::assertions_on_constants`, a bramka wola `-D warnings`.
    // Przez zwykle wiazania warunek jest liczony w czasie wykonania i dalej przewraca test,
    // gdy ktos zrowna oba formaty.
    let supported = HIGHEST_SUPPORTED;
    let baseline = CURRENT;
    assert!(
        supported > baseline,
        "a context document is not newer to the old reader unless supported and baseline formats are separate"
    );
    Ok(())
}

#[test]
fn an_empty_draft_still_refuses_a_malformed_workflow_context() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let mut empty = workflow(Some(json!({ "schema": 1, "sets": "not a list" })), None);
    empty.steps.clear();

    let refusal = save(&empty, &home.path().join("empty.json"), None)
        .expect_err("an empty draft saved malformed Context")
        .to_string();

    assert!(refusal.contains("context selection cannot be read"));
    assert!(!home.path().join("empty.json").exists());
    Ok(())
}

#[test]
fn a_missing_pin_is_a_saveable_warning_but_a_named_start_refusal() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let file = workflow(
        Some(shared(vec![pin("missing-set", "r1", json!("all"))])),
        None,
    );

    let view = resolve_workflow_context_inner(home.path(), &file)?;
    let saved = save(&file, &home.path().join("draft.json"), None);
    let refused = context_ready_to_start(home.path(), &file, &included())
        .expect_err("Start accepted a missing context set");

    assert!(
        view.warnings
            .iter()
            .any(|warning| warning.contains("draft can still be saved"))
    );
    assert!(
        view.catalog.iter().any(|choice| choice.id == "missing-set"),
        "the unavailable pin disappeared from the picker, so the warning has no removal path"
    );
    assert!(
        saved.is_ok(),
        "the same unavailable pin made the draft unsaveable: {saved:?}"
    );
    assert_eq!(refused.step_id, "frontend");
    assert!(refused.message.contains("Frontend") && refused.message.contains("missing-set"));
    assert!(refused.message.contains("rebuild") || refused.message.contains("choose"));
    Ok(())
}

#[test]
fn a_missing_pin_outside_the_requested_part_does_not_block_that_part() -> Result<(), Box<dyn Error>>
{
    let home = tempfile::tempdir()?;
    let file = workflow(
        Some(shared(vec![pin("missing-set", "r1", json!("all"))])),
        None,
    );

    context_ready_to_start(home.path(), &file, &BTreeSet::new()).map_err(|refusal| {
        format!(
            "an excluded step blocked a partial run: {}",
            refusal.message
        )
    })?;
    Ok(())
}

#[test]
fn unready_and_damaged_versions_each_refuse_before_start() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let unready_id = create_set(home.path(), "Unready set")?;
    let unready = workflow(
        Some(shared(vec![pin(&unready_id, "r1", json!("all"))])),
        None,
    );
    let unready_refusal = context_ready_to_start(home.path(), &unready, &included())
        .expect_err("a set with no built version started");

    let damaged_id = create_set(home.path(), "Damaged set")?;
    let damaged_folder = folder_of(&library_root(home.path()), &damaged_id)?;
    let damaged_version = damaged_folder.join("versions/r1");
    fs::create_dir_all(&damaged_version)?;
    fs::write(damaged_version.join("manifest.json"), b"{}")?;
    let damaged = workflow(
        Some(shared(vec![pin(&damaged_id, "r1", json!("all"))])),
        None,
    );
    let damaged_refusal = context_ready_to_start(home.path(), &damaged, &included())
        .expect_err("an incomplete version started");

    assert!(unready_refusal.message.contains(&unready_id));
    assert!(damaged_refusal.message.contains(&damaged_id));
    Ok(())
}

#[test]
fn removed_topics_need_a_new_choice_but_large_material_stays_available()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let changed_id = create_set(home.path(), "Changed topics")?;
    publish_revision(
        home.path(),
        &changed_id,
        "r1",
        &[("checkout", "Checkout")],
        "# Index\n",
    )?;
    let changed = workflow(
        Some(shared(vec![pin(&changed_id, "r1", json!(["returns"]))])),
        None,
    );
    let missing_topic = context_ready_to_start(home.path(), &changed, &included())
        .expect_err("a removed selected topic silently became all topics");

    let large_id = create_set(home.path(), "Large context")?;
    publish_revision(
        home.path(),
        &large_id,
        "r1",
        &[("checkout", "Checkout")],
        &"x".repeat(24 * 1024 + 1),
    )?;
    let large = workflow(Some(shared(vec![pin(&large_id, "r1", json!("all"))])), None);
    // 2026-09-08 (CT-06) — 24 KiB ogranicza dodatek do promptu, nie prywatny pakiet,
    // bo agent ma zawsze móc doczytać pełny materiał przez most.
    context_ready_to_start(home.path(), &large, &included()).map_err(|refusal| refusal.message)?;

    assert!(missing_topic.message.contains(&changed_id));
    assert!(missing_topic.message.contains("choose its topics again"));
    Ok(())
}

#[test]
fn more_than_eight_effective_sets_are_refused_before_any_of_them_is_opened()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let pins = (0..9)
        .map(|at| pin(&format!("set-{at}"), "r1", json!("all")))
        .collect();
    let file = workflow(Some(shared(pins)), None);

    let refusal = context_ready_to_start(home.path(), &file, &included())
        .expect_err("nine effective sets were accepted");

    assert_eq!(refusal.step_id, "frontend");
    assert!(refusal.message.contains("more than eight context sets"));
    Ok(())
}

#[test]
fn a_new_ready_version_is_only_an_offer_and_copying_keeps_exact_pins() -> Result<(), Box<dyn Error>>
{
    let home = tempfile::tempdir()?;
    let set_id = create_set(home.path(), "Store rules")?;
    publish_revision(
        home.path(),
        &set_id,
        "r1",
        &[("checkout", "Checkout")],
        "# One\n",
    )?;
    let file = workflow(
        Some(shared(vec![pin(&set_id, "r1", json!(["checkout"]))])),
        Some(local(
            Some(true),
            Vec::new(),
            vec![pin(&set_id, "r1", json!(["checkout"]))],
        )),
    );
    publish_revision(
        home.path(),
        &set_id,
        "r2",
        &[("payment", "Payment")],
        "# Two\n",
    )?;
    let before = serde_json::to_vec(&file)?;

    let view = resolve_workflow_context_inner(home.path(), &file)?;
    let workflow_copy: WorkflowFile = serde_json::from_slice(&serde_json::to_vec(&file)?)?;
    let step_copy = file.steps[0].clone();

    assert_eq!(view.workflow[0].update.as_deref(), Some("Update available"));
    assert_eq!(view.workflow[0].revision, "r1");
    assert_eq!(view.workflow[0].topics[0].id, "checkout");
    assert_eq!(
        serde_json::to_vec(&file)?,
        before,
        "looking for updates rewrote the workflow"
    );
    assert_eq!(workflow_copy.context()?, file.context()?);
    let loadout_lib::workflow::Step::Agent(step_copy) = step_copy else {
        return Err("the copied step changed kind".into());
    };
    let loadout_lib::workflow::Step::Agent(original_step) = &file.steps[0] else {
        return Err("the fixture step changed kind".into());
    };
    assert_eq!(step_copy.context()?, original_step.context()?);
    Ok(())
}
