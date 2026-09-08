//! CT-08: Recorded odtwarza prywatny pakiet Context zamiast dzisiejszej biblioteki.

#![allow(
    clippy::expect_used,
    clippy::too_many_lines,
    reason = "CT-08 keeps each end-to-end acceptance story and its fixture failures together"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::context::{archive_context_set_inner, delete_context_set_inner};
use loadout_lib::commands::processes::Processes;
use loadout_lib::commands::run::run_workflow_with_before_stamp;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{AppState, line_channel};
use loadout_lib::store::Store;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use tokio::sync::mpsc;

use super::context_history_reports_delivery::install_image_package;
use super::step_receives_selected_context::{Bench, pin, run, step};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_saved_material_replays_after_the_whole_library_is_deleted()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let material = bench.publish(
        "Recorded brief",
        "Keep the exact historical material.",
        "recorded",
        "Recorded topic",
        "RECORDED-REQUIREMENT stays byte-for-byte stable.",
        "RECORDED-SOURCE-CONTENT survives deletion.",
    )?;
    let mut configured_step = step(
        "recorded",
        "CTX-FROZEN replay the recorded material.",
        Value::Null,
    );
    configured_step
        .as_object_mut()
        .ok_or("the fixture step is not an object")?
        .remove("context");
    let workflow = bench.workflow(
        "recorded-context",
        &json!({
            "format": 2,
            "id": "ct-08-recorded-context",
            "name": "Replay recorded context",
            "steps": [configured_step],
            "links": []
        }),
    )?;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let selected_drivers = capturing_drivers(Arc::clone(&seen));
    let source = run(&bench, workflow.clone(), Arc::clone(&selected_drivers), 1).await?;
    assert_eq!(seen.lock().unwrap_or_else(PoisonError::into_inner).len(), 1);
    configured_step["context"] = pin(&material);
    install_frozen_context(&source.dir, &workflow, &configured_step, &material)?;
    let source_rows = visible_material_rows(bench.project.path(), &source.dir)?;
    seen.lock().unwrap_or_else(PoisonError::into_inner).clear();

    fs::remove_dir_all(loadout_lib::context::files::library_root(bench.home.path()))?;
    let state = AppState::new(
        bench.home.path().to_path_buf(),
        bench.project.path().to_path_buf(),
        Store::open(&bench.db())?,
        selected_drivers,
    );
    let folder = bench
        .project
        .path()
        .to_str()
        .ok_or("the fixture project path is not text")?;
    let preview = state
        .prepare_replay_inner(folder, &source.id, json!({"kind":"all"}), "recorded")
        .await?;
    let preview_id = preview["previewId"]
        .as_str()
        .ok_or("the replay preview has no ID")?;
    let requested = state
        .authorize_replay_inner(folder, preview_id, "Start replay".to_owned())
        .await?;
    let Line::RunRequested { request_id, .. } = requested else {
        return Err("Recorded replay bypassed the normal Start request".into());
    };
    let (sink, _events) = line_channel(512);
    let replay = state
        .accept_lead_start_inner(&request_id, 1, Some(8.0), false, sink)
        .await?;
    let replay_prompt = seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .pop()
        .ok_or("the replay step did not run")?;
    assert!(replay_prompt.contains(FROZEN_REQUIREMENT));
    assert!(replay_prompt.contains(FROZEN_OPTIONAL));
    let replay_source =
        loadout_lib::commands::lead_history::source_for(bench.project.path(), &replay.run.run_id)?;
    let replay_dir = bench
        .project
        .path()
        .join(".loadout/runs")
        .join(replay_source.run_folder);
    assert_eq!(
        visible_material_rows(bench.project.path(), &replay_dir)?,
        source_rows,
        "the replay history described different recorded material"
    );

    let mut missing: Value = serde_json::from_slice(&fs::read(source.dir.join("run.json"))?)?;
    missing
        .as_object_mut()
        .ok_or("the saved run is not an object")?
        .remove("context_sources");
    fs::write(
        source.dir.join("run.json"),
        serde_json::to_vec_pretty(&missing)?,
    )?;
    let without_binding = state
        .prepare_replay_inner(folder, &source.id, json!({"kind":"all"}), "recorded")
        .await;
    assert_recorded_refusal(without_binding, "a missing package became empty Context")?;
    state.close_everything_down().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn current_replay_keeps_todays_explicit_pin_instead_of_using_latest()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let material = bench.publish(
        "Pinned today",
        "Keep the selected version exact.",
        "pinned-today",
        "Pinned topic",
        "PINNED-TODAY-REQUIREMENT stays selected.",
        "PINNED-TODAY-SOURCE stays selected.",
    )?;
    let mut configured = step("current", "CTX-CURRENT uses today's pin.", Value::Null);
    configured
        .as_object_mut()
        .ok_or("the fixture step is not an object")?
        .remove("context");
    let workflow = bench.workflow(
        "current-context",
        &json!({
            "format": 2,
            "id": "ct-08-current-context",
            "name": "Replay current context",
            "steps": [configured],
            "links": []
        }),
    )?;
    let source = run(
        &bench,
        workflow.clone(),
        capturing_drivers(Arc::new(Mutex::new(Vec::new()))),
        1,
    )
    .await?;
    let latest = "revision-newer-than-the-pin";
    install_newer_revision(&bench, &material, latest)?;
    configured["context"] = pin(&material);
    let mut current: Value = serde_json::from_slice(&fs::read(&workflow)?)?;
    current["steps"][0] = configured;
    fs::write(&workflow, serde_json::to_vec_pretty(&current)?)?;

    let state = AppState::new(
        bench.home.path().to_path_buf(),
        bench.project.path().to_path_buf(),
        Store::open(&bench.db())?,
        capturing_drivers(Arc::new(Mutex::new(Vec::new()))),
    );
    let folder = bench
        .project
        .path()
        .to_str()
        .ok_or("the fixture project path is not text")?;
    let preview = state
        .prepare_replay_inner(folder, &source.id, json!({"kind":"all"}), "current")
        .await?;
    let requested = authorize(&state, folder, &preview).await?;
    let (sink, _events) = line_channel(128);
    let replay = state
        .accept_lead_start_inner(&requested, 1, Some(8.0), false, sink)
        .await?;
    let replay_source =
        loadout_lib::commands::lead_history::source_for(bench.project.path(), &replay.run.run_id)?;
    let replay_dir = bench
        .project
        .path()
        .join(".loadout/runs")
        .join(replay_source.run_folder);
    let rows = visible_material_rows(bench.project.path(), &replay_dir)?.join("\n");
    assert!(rows.contains(&material.revision), "{rows}");
    assert!(!rows.contains(latest), "{rows}");
    state.close_everything_down().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_partial_replay_keeps_the_selected_physical_steps_frozen_block()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let mut first = step("first", "first source step", Value::Null);
    let mut second = step("second", "second source step", Value::Null);
    first
        .as_object_mut()
        .ok_or("the first fixture step is not an object")?
        .remove("context");
    second
        .as_object_mut()
        .ok_or("the second fixture step is not an object")?
        .remove("context");
    let workflow = bench.workflow(
        "partial-recorded-context",
        &json!({
            "format": 2,
            "id": "ct-08-partial-recorded-context",
            "name": "Partial recorded context",
            "steps": [first, second],
            "links": []
        }),
    )?;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let selected_drivers = capturing_drivers(Arc::clone(&seen));
    let source = run(&bench, workflow, Arc::clone(&selected_drivers), 2).await?;
    seen.lock().map_err(|_| "observations poisoned")?.clear();
    install_two_frozen_blocks(&source.dir)?;
    let state = AppState::new(
        bench.home.path().to_path_buf(),
        bench.project.path().to_path_buf(),
        Store::open(&bench.db())?,
        selected_drivers,
    );
    let folder = bench
        .project
        .path()
        .to_str()
        .ok_or("the fixture project path is not text")?;
    let all = state
        .prepare_replay_inner(folder, &source.id, json!({"kind":"all"}), "recorded")
        .await?;
    let requested = authorize(&state, folder, &all).await?;
    let (sink, _events) = line_channel(128);
    state
        .accept_lead_start_inner(&requested, 1, Some(8.0), false, sink)
        .await?;
    {
        let prompts = seen.lock().map_err(|_| "observations poisoned")?;
        let first_prompt = prompts
            .iter()
            .find(|prompt| prompt.contains("first source step"))
            .ok_or("the first physical step did not replay")?;
        let second_prompt = prompts
            .iter()
            .find(|prompt| prompt.contains("second source step"))
            .ok_or("the second physical step did not replay")?;
        assert!(first_prompt.contains(FIRST_FROZEN_BLOCK), "{first_prompt}");
        assert!(
            !first_prompt.contains(SECOND_FROZEN_BLOCK),
            "{first_prompt}"
        );
        assert!(
            second_prompt.contains(SECOND_FROZEN_BLOCK),
            "{second_prompt}"
        );
        assert!(
            !second_prompt.contains(FIRST_FROZEN_BLOCK),
            "{second_prompt}"
        );
    }
    seen.lock().map_err(|_| "observations poisoned")?.clear();
    let preview = state
        .prepare_replay_inner(
            folder,
            &source.id,
            json!({"kind":"step", "step_id":"second"}),
            "recorded",
        )
        .await?;
    let requested = authorize(&state, folder, &preview).await?;
    let (sink, _events) = line_channel(128);
    state
        .accept_lead_start_inner(&requested, 1, Some(8.0), false, sink)
        .await?;
    {
        let prompts = seen.lock().map_err(|_| "observations poisoned")?;
        assert_eq!(
            prompts.len(),
            1,
            "a tile replay ran a different physical step"
        );
        assert!(prompts[0].contains(SECOND_FROZEN_BLOCK), "{}", prompts[0]);
        assert!(!prompts[0].contains(FIRST_FROZEN_BLOCK), "{}", prompts[0]);
    }
    state.close_everything_down().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn archive_keeps_the_pin_and_delete_names_uses_before_future_start_refuses()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let material = bench.publish(
        "Pinned archive",
        "Keep this exact pin usable while archived.",
        "archive",
        "Archived topic",
        "ARCHIVED-REQUIREMENT stays pinned.",
        "ARCHIVED-SOURCE stays available.",
    )?;
    let workflow = bench.workflow(
        "archived-pin",
        &json!({
            "format": 2,
            "id": "ct-08-archived-pin",
            "name": "Workflow using the archived pin",
            "steps": [step(
                "archived-step",
                "CTX-ARCHIVE uses the pinned set.",
                json!({
                    "schema": 1,
                    "inheritWorkflow": false,
                    "exclude": [],
                    "sets": [{
                        "id": material.set_id,
                        "revision": material.revision,
                        "topics": "all"
                    }]
                }),
            )],
            "links": []
        }),
    )?;
    let archived = archive_context_set_inner(bench.home.path(), &material.set_id, true)?;
    assert!(archived.archived);
    let library = loadout_lib::context::files::library_root(bench.home.path());
    assert!(
        loadout_lib::context::files::list_sets_by_archive(&library, false)?.is_empty(),
        "Archive left the set on the default shelf"
    );
    assert_eq!(
        loadout_lib::context::files::list_sets_by_archive(&library, true)?[0]
            .latest_ready_revision
            .as_deref(),
        Some(material.revision.as_str())
    );
    assert_eq!(
        loadout_lib::context::files::read_set(&library, &material.set_id)?
            .set
            .latest_ready_revision
            .as_deref(),
        Some(material.revision.as_str()),
        "Archive broke the exact version the workflow pins"
    );
    let archived_prompt = prompt_before_first_process(&bench, workflow.clone()).await?;
    assert!(
        archived_prompt.contains("ARCHIVED-REQUIREMENT stays pinned."),
        "{archived_prompt}"
    );

    let preview = delete_context_set_inner(
        bench.home.path(),
        bench.project.path(),
        &material.set_id,
        false,
    )?;
    assert!(!preview.deleted);
    assert_eq!(preview.uses, ["Workflow using the archived pin"]);
    assert!(
        preview
            .said
            .contains("Saved runs keep their historical copies")
    );
    assert!(library.exists(), "the preview deleted before confirmation");
    let deleted = delete_context_set_inner(
        bench.home.path(),
        bench.project.path(),
        &material.set_id,
        true,
    )?;
    assert!(deleted.deleted);
    assert!(
        loadout_lib::context::files::read_set(&library, &material.set_id).is_err(),
        "Delete left a live library set behind"
    );

    let starts = Arc::new(Mutex::new(Vec::new()));
    let refused = run(&bench, workflow, capturing_drivers(Arc::clone(&starts)), 1).await;
    assert!(
        refused.is_err(),
        "a deleted future pin started: {refused:?}"
    );
    let said = refused.err().ok_or("the refusal vanished")?.to_string();
    assert!(said.contains("archived-step"), "{said}");
    assert!(said.contains(&material.set_id), "{said}");
    assert!(
        said.contains("Restore it or choose another ready version"),
        "{said}"
    );
    assert!(
        starts
            .lock()
            .map_err(|_| "observations poisoned")?
            .is_empty()
    );
    Ok(())
}

async fn prompt_before_first_process(
    bench: &Bench,
    workflow: std::path::PathBuf,
) -> Result<String, Box<dyn Error>> {
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let hook_prompts = Arc::clone(&prompts);
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: capturing_drivers(Arc::new(Mutex::new(Vec::new()))),
        processes: Arc::new(Processes::new()),
        control: RunControl::new(),
    };
    let (sink, _events) = line_channel(512);
    // 2026-09-08 (CT-08): ten produkcyjny szew widzi finalny `RunSpec` przed procesem;
    // dzięki temu archiwizację mierzymy bez uzależniania jej od gniazd Unix sandboxa testu.
    let hook = Arc::new(move |prompt: &str| {
        hook_prompts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(prompt.to_owned());
    });
    let _report = run_workflow_with_before_stamp(
        &deps,
        &RunRequest {
            workflow,
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        },
        sink,
        Some(hook),
    )
    .await?;
    let prompt = prompts
        .lock()
        .map_err(|_| "archived prompt observation poisoned".to_owned())?
        .pop();
    prompt.ok_or_else(|| {
        Box::<dyn Error>::from("the archived pin did not reach the final run prompt")
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn damaged_and_swapped_package_files_refuse_the_preview_instead_of_becoming_empty()
-> Result<(), Box<dyn Error>> {
    let damaged = package_fixture().await?;
    fs::write(&damaged.files[0], b"damaged historical input")?;
    let damaged_answer = damaged
        .state
        .prepare_replay_inner(
            damaged.folder()?,
            &damaged.source.id,
            json!({"kind":"all"}),
            "recorded",
        )
        .await;
    assert_recorded_refusal(damaged_answer, "a damaged file produced a preview")?;
    assert!(damaged.starts()?.is_empty());
    damaged.state.close_everything_down().await;

    let swapped = package_fixture().await?;
    let first = fs::read(&swapped.files[0])?;
    let second = fs::read(&swapped.files[1])?;
    fs::write(&swapped.files[0], second)?;
    fs::write(&swapped.files[1], first)?;
    let swapped_answer = swapped
        .state
        .prepare_replay_inner(
            swapped.folder()?,
            &swapped.source.id,
            json!({"kind":"all"}),
            "recorded",
        )
        .await;
    assert_recorded_refusal(swapped_answer, "two swapped files produced a preview")?;
    assert!(swapped.starts()?.is_empty());
    swapped.state.close_everything_down().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn package_changes_after_preview_or_confirmation_refuse_before_a_process()
-> Result<(), Box<dyn Error>> {
    let after_preview = package_fixture().await?;
    let preview = after_preview
        .state
        .prepare_replay_inner(
            after_preview.folder()?,
            &after_preview.source.id,
            json!({"kind":"all"}),
            "recorded",
        )
        .await?;
    flip_manifest_byte(&after_preview.source.dir)?;
    let preview_id = preview["previewId"]
        .as_str()
        .ok_or("the replay preview has no ID")?;
    let answer = after_preview
        .state
        .authorize_replay_inner(
            after_preview.folder()?,
            preview_id,
            "Start replay".to_owned(),
        )
        .await;
    assert_recorded_refusal(answer, "a changed post-preview package was confirmed")?;
    assert!(after_preview.starts()?.is_empty());
    after_preview.state.close_everything_down().await;

    let after_confirmation = package_fixture().await?;
    let preview = after_confirmation
        .state
        .prepare_replay_inner(
            after_confirmation.folder()?,
            &after_confirmation.source.id,
            json!({"kind":"all"}),
            "recorded",
        )
        .await?;
    let requested = authorize(
        &after_confirmation.state,
        after_confirmation.folder()?,
        &preview,
    )
    .await?;
    flip_manifest_byte(&after_confirmation.source.dir)?;
    let (sink, _events) = line_channel(128);
    let answer = after_confirmation
        .state
        .accept_lead_start_inner(&requested, 1, Some(8.0), false, sink)
        .await;
    assert_recorded_refusal(answer, "a changed confirmed package started")?;
    assert!(after_confirmation.starts()?.is_empty());
    after_confirmation.state.close_everything_down().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_replay_preview_holds_its_source_against_retention() -> Result<(), Box<dyn Error>> {
    let fixture = package_fixture().await?;
    let preview = fixture
        .state
        .prepare_replay_inner(
            fixture.folder()?,
            &fixture.source.id,
            json!({"kind":"all"}),
            "recorded",
        )
        .await?;
    let run_folder = fixture
        .source
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the source run has no folder name")?;
    let removal =
        loadout_lib::commands::history::forget_run_inner(fixture.bench.project.path(), run_folder);
    let said = removal
        .err()
        .ok_or("retention removed material held by a replay preview")?
        .to_string();
    assert!(said.contains("being read or copied"), "{said}");
    assert!(fixture.source.dir.exists());
    let requested = authorize(&fixture.state, fixture.folder()?, &preview).await?;
    drop(requested);
    fixture.state.close_everything_down().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lab_copies_one_saved_case_package_for_every_variant_before_retention()
-> Result<(), Box<dyn Error>> {
    let fixture = package_fixture().await?;
    let input = loadout_lib::commands::input_snapshot::capture_selected(
        fixture.bench.project.path(),
        &fixture.source.dir,
        &["README.md".to_owned()],
    )?;
    let mut source_run: Value =
        serde_json::from_slice(&fs::read(fixture.source.dir.join("run.json"))?)?;
    source_run["input_snapshot"] = json!({"id": input.id(), "manifest": "input/manifest.json"});
    fs::write(
        fixture.source.dir.join("run.json"),
        serde_json::to_vec_pretty(&source_run)?,
    )?;
    let source_graph = loadout_lib::workflow::file::load(&fixture.workflow)?;
    let set: loadout_lib::lab::EvalSet = serde_json::from_value(json!({
        "format": 2,
        "id": "frozen-context",
        "name": "Frozen context",
        "subject": {"kind": "workflow", "id": source_graph.id},
        "cases": [{
            "id": "saved",
            "name": "Saved case",
            "task": "Use the saved case material",
            "status": "in-use",
            "command": "printf 'lab context: 1 passed\\n'",
            "proof": "lab context: (\\d+) passed",
            "input": {"sourceRunId": fixture.source.id, "snapshotId": input.id()}
        }],
        "variants": [
            {"id": "first", "name": "First model", "workflow": {
                "id": source_graph.id, "outputStep": "history",
                "overrides": {"history": {"model": "model-one"}}
            }},
            {"id": "second", "name": "Second model", "workflow": {
                "id": source_graph.id, "outputStep": "history",
                "overrides": {"history": {"model": "model-two"}}
            }}
        ]
    }))?;
    let compiled = loadout_lib::lab::workflow_plan::compose_selected(
        &[source_graph.clone(), source_graph],
        &set,
        "eval:frozen-context".to_owned(),
        "Frozen context - Lab".to_owned(),
    )?;
    let workflow = fixture
        .bench
        .workflow("frozen-context-lab", &serde_json::to_value(compiled)?)?;
    let report = run(
        &fixture.bench,
        workflow,
        capturing_drivers(Arc::clone(&fixture.seen)),
        2,
    )
    .await?;

    let source_folder = fixture
        .source
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the source run has no folder name")?;
    loadout_lib::commands::history::forget_run_inner(fixture.bench.project.path(), source_folder)?;
    assert!(!fixture.source.dir.exists());
    let lab_folder = report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the Lab run has no folder name")?;
    let history =
        loadout_lib::commands::history::read_run_inner(fixture.bench.project.path(), lab_folder)?;
    let delivered = history
        .steps
        .iter()
        .filter_map(|step| step.reference_materials.as_ref())
        .flatten()
        .filter(|line| line.contains("Reference picture") && line.contains("available to read"))
        .count();
    assert_eq!(
        delivered, 2,
        "both model variants must retain the same saved case material"
    );
    assert_eq!(
        fs::read(
            report.dir.join(
                "context-sources/set-history/revision-history/source--image/for-the-agent.png"
            )
        )?,
        b"PRIVATE-CONTEXT-IMAGE-CT08"
    );
    fixture.state.close_everything_down().await;
    Ok(())
}

fn visible_material_rows(project: &Path, run_dir: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let folder = run_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the run has no portable folder name")?;
    Ok(
        loadout_lib::commands::history::read_run_inner(project, folder)?
            .steps
            .first()
            .and_then(|step| step.reference_materials.clone())
            .ok_or("the run history has no reference-material rows")?,
    )
}

fn install_newer_revision(
    bench: &Bench,
    material: &super::step_receives_selected_context::Published,
    latest: &str,
) -> Result<(), Box<dyn Error>> {
    let folder = loadout_lib::context::files::folder_of(
        &loadout_lib::context::files::library_root(bench.home.path()),
        &material.set_id,
    )?;
    let source = folder.join("versions").join(&material.revision);
    let destination = folder.join("versions").join(latest);
    fs::create_dir_all(destination.join("topics"))?;
    let mut manifest: loadout_lib::context::ContextRevision =
        serde_json::from_slice(&fs::read(source.join("manifest.json"))?)?;
    latest.clone_into(&mut manifest.id);
    fs::write(
        destination.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    fs::write(
        destination.join("index.md"),
        fs::read(source.join("index.md"))?,
    )?;
    let topic = format!("{}.md", material.topic_id);
    fs::write(
        destination.join("topics").join(&topic),
        "LATEST-REVISION-MUST-NOT-BE-USED\n",
    )?;
    let mut findings: Vec<loadout_lib::context::ContextFinding> =
        serde_json::from_slice(&fs::read(source.join("findings.json"))?)?;
    for finding in &mut findings {
        "LATEST-REVISION-MUST-NOT-BE-USED".clone_into(&mut finding.text);
    }
    fs::write(
        destination.join("findings.json"),
        serde_json::to_vec_pretty(&findings)?,
    )?;
    let manifest_path = folder.join("manifest.json");
    let mut set: loadout_lib::context::ContextSet =
        serde_json::from_slice(&fs::read(&manifest_path)?)?;
    set.latest_ready_revision = Some(latest.to_owned());
    fs::write(manifest_path, serde_json::to_vec_pretty(&set)?)?;
    Ok(())
}

struct PackageFixture {
    bench: Bench,
    source: loadout_lib::commands::RunReport,
    workflow: std::path::PathBuf,
    state: AppState,
    files: [std::path::PathBuf; 2],
    seen: Arc<Mutex<Vec<String>>>,
}

impl PackageFixture {
    fn folder(&self) -> Result<&str, Box<dyn Error>> {
        self.bench
            .project
            .path()
            .to_str()
            .ok_or_else(|| "the fixture project path is not text".into())
    }

    fn starts(&self) -> Result<Vec<String>, Box<dyn Error>> {
        self.seen
            .lock()
            .map(|seen| seen.clone())
            .map_err(|_| "observations poisoned".into())
    }
}

async fn package_fixture() -> Result<PackageFixture, Box<dyn Error>> {
    let bench = Bench::new()?;
    let mut configured = step("history", "CTX-PACKAGE must not start.", Value::Null);
    configured
        .as_object_mut()
        .ok_or("the fixture step is not an object")?
        .remove("context");
    let workflow = bench.workflow(
        "recorded-package",
        &json!({
            "format": 2,
            "id": "ct-08-recorded-package",
            "name": "Recorded package",
            "steps": [configured],
            "links": []
        }),
    )?;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let selected_drivers = capturing_drivers(Arc::clone(&seen));
    let source = run(&bench, workflow.clone(), Arc::clone(&selected_drivers), 1).await?;
    seen.lock().map_err(|_| "observations poisoned")?.clear();
    let files = install_image_package(&source.dir)?;
    let state = AppState::new(
        bench.home.path().to_path_buf(),
        bench.project.path().to_path_buf(),
        Store::open(&bench.db())?,
        selected_drivers,
    );
    Ok(PackageFixture {
        bench,
        source,
        workflow,
        state,
        files,
        seen,
    })
}

async fn authorize(
    state: &AppState,
    folder: &str,
    preview: &Value,
) -> Result<String, Box<dyn Error>> {
    let preview_id = preview["previewId"]
        .as_str()
        .ok_or("the replay preview has no ID")?;
    let line = state
        .authorize_replay_inner(folder, preview_id, "Start replay".to_owned())
        .await?;
    let Line::RunRequested { request_id, .. } = line else {
        return Err("replay bypassed the normal Start request".into());
    };
    Ok(request_id)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_whole_foreign_package_refuses_even_though_every_digest_matches()
-> Result<(), Box<dyn Error>> {
    let swapped = package_fixture().await?;

    // Przesłanka: zanim cokolwiek podmienimy, TEN SAM wywołanie musi się udać. Bez tego
    // asercja przechodzi także dla `prepare_replay_inner`, które odmawia zawsze.
    let honest = swapped
        .state
        .prepare_replay_inner(
            swapped.folder()?,
            &swapped.source.id,
            json!({"kind":"all"}),
            "recorded",
        )
        .await;
    assert!(
        honest.is_ok(),
        "the fixture cannot preview its own untouched package: {honest:?}"
    );

    swap_in_a_foreign_package(&swapped.source.dir)?;
    let answer = swapped
        .state
        .prepare_replay_inner(
            swapped.folder()?,
            &swapped.source.id,
            json!({"kind":"all"}),
            "recorded",
        )
        .await;
    assert_recorded_refusal(
        answer,
        "a package belonging to another run produced a preview",
    )?;
    assert!(swapped.starts()?.is_empty());
    swapped.state.close_everything_down().await;
    Ok(())
}

fn flip_manifest_byte(run_dir: &Path) -> Result<(), Box<dyn Error>> {
    let path = run_dir.join("context-sources/manifest.json");
    let mut bytes = fs::read(&path)?;
    let at = bytes
        .windows("Reference picture".len())
        .position(|window| window == b"Reference picture")
        .ok_or("the manifest has no stable text to change")?;
    bytes[at] = b'X';
    fs::write(path, bytes)?;
    Ok(())
}

/// Podmienia manifest na WEWNĘTRZNIE POPRAWNY, ale należący do innego pakietu.
///
/// 2026-09-08 (CT-08, luka wyroczni) — `flip_manifest_byte` i przepisanie pliku pakietu psują
/// ODCISKI, więc obie te drogi wpadają w kontrolę sum kontrolnych wewnątrz `read_package`.
/// Wiązanie z `run.json` — czyli jedyna kontrola, która odróżnia „to jest CUDZY pakiet" od
/// „ten pakiet jest uszkodzony" — nie miała ani jednego świadka: mutacja zamieniająca jej
/// `Err` na `Ok(None)` przechodziła całą suitę na zielono. Tu zmienia się wyłącznie
/// identyfikator manifestu, a wszystkie odciski plików zostają zgodne.
fn swap_in_a_foreign_package(run_dir: &Path) -> Result<(), Box<dyn Error>> {
    let path = run_dir.join("context-sources/manifest.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&path)?)?;
    manifest["id"] = json!(uuid::Uuid::now_v7().to_string());
    let bytes = serde_json::to_vec(&manifest)?;
    fs::write(&path, bytes)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

fn assert_recorded_refusal<T: std::fmt::Debug>(
    answer: Result<T, String>,
    accepted: &str,
) -> Result<(), Box<dyn Error>> {
    let said = answer.err().ok_or_else(|| accepted.to_owned())?;
    assert!(
        said.starts_with("Recorded inputs are unavailable:"),
        "{said}"
    );
    assert!(
        said.contains("Nothing was replaced with today's files"),
        "{said}"
    );
    Ok(())
}

const FROZEN_REQUIREMENT: &str = "RECORDED-REQUIREMENT stays byte-for-byte stable.";
const FROZEN_OPTIONAL: &str = "HISTORICAL-OPTIONAL-INDEX stays byte-for-byte stable.";
const FIRST_FROZEN_BLOCK: &str = "PHYSICAL-FIRST-CONTEXT";
const SECOND_FROZEN_BLOCK: &str = "PHYSICAL-SECOND-CONTEXT";

#[derive(Serialize)]
struct SeedManifest {
    schema: u32,
    id: String,
    items: Vec<Value>,
    nodes: BTreeMap<String, Vec<Value>>,
    delivery: BTreeMap<String, Vec<SeedDelivery>>,
    blocks: BTreeMap<String, SeedBlock>,
}

#[derive(Serialize)]
struct SeedBlock {
    required: String,
    optional: String,
    required_name: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SeedDelivery {
    set_name: String,
    version: String,
    item: String,
    kind: String,
    bytes: usize,
    state: &'static str,
    run_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    address: Option<Value>,
    reference: String,
}

fn install_frozen_context(
    run_dir: &Path,
    workflow: &Path,
    configured_step: &Value,
    material: &super::step_receives_selected_context::Published,
) -> Result<(), Box<dyn Error>> {
    let package_id = uuid::Uuid::now_v7().to_string();
    let mut nodes = BTreeMap::new();
    nodes.insert("recorded".to_owned(), Vec::new());
    let mut delivery = BTreeMap::new();
    delivery.insert(
        "recorded".to_owned(),
        vec![SeedDelivery {
            set_name: "Recorded brief".to_owned(),
            version: material.revision.clone(),
            item: "Set requirements".to_owned(),
            kind: "requirement".to_owned(),
            bytes: FROZEN_REQUIREMENT.len(),
            state: "included",
            run_only: false,
            address: None,
            reference: format!(
                "context-sources/{}/{}/requirements",
                material.set_id, material.revision
            ),
        }],
    );
    let mut blocks = BTreeMap::new();
    blocks.insert(
        "recorded".to_owned(),
        SeedBlock {
            required: format!(
                "## Reference materials\nImportant requirements:\n\n### Recorded brief ({})\nPurpose: Keep the exact historical material.\n- {FROZEN_REQUIREMENT}\n",
                material.revision
            ),
            optional: FROZEN_OPTIONAL.to_owned(),
            required_name: Some("Recorded brief".to_owned()),
        },
    );
    let manifest = SeedManifest {
        schema: 1,
        id: package_id.clone(),
        items: Vec::new(),
        nodes,
        delivery,
        blocks,
    };
    let manifest_bytes = serde_json::to_vec(&manifest)?;
    let digest = format!("{:x}", Sha256::digest(&manifest_bytes));
    let package = run_dir.join("context-sources");
    fs::create_dir_all(package.join("reads"))?;
    fs::set_permissions(&package, fs::Permissions::from_mode(0o700))?;
    fs::set_permissions(package.join("reads"), fs::Permissions::from_mode(0o700))?;
    fs::write(package.join("manifest.json"), manifest_bytes)?;
    fs::set_permissions(
        package.join("manifest.json"),
        fs::Permissions::from_mode(0o600),
    )?;

    let mut source: Value = serde_json::from_slice(&fs::read(run_dir.join("run.json"))?)?;
    source["context_sources"] = json!({"id": package_id, "digest": digest});
    source["workflow_snapshot"]["steps"][0] = configured_step.clone();
    fs::write(
        run_dir.join("run.json"),
        serde_json::to_vec_pretty(&source)?,
    )?;
    let mut current: Value = serde_json::from_slice(&fs::read(workflow)?)?;
    current["steps"][0] = configured_step.clone();
    fs::write(workflow, serde_json::to_vec_pretty(&current)?)?;
    Ok(())
}

fn install_two_frozen_blocks(run_dir: &Path) -> Result<(), Box<dyn Error>> {
    let package_id = uuid::Uuid::now_v7().to_string();
    let mut nodes = BTreeMap::new();
    let mut delivery = BTreeMap::new();
    let mut blocks = BTreeMap::new();
    for (node, version, text) in [
        ("first", "revision-first", FIRST_FROZEN_BLOCK),
        ("second", "revision-second", SECOND_FROZEN_BLOCK),
    ] {
        nodes.insert(node.to_owned(), Vec::new());
        delivery.insert(
            node.to_owned(),
            vec![SeedDelivery {
                set_name: "Physical address".to_owned(),
                version: version.to_owned(),
                item: "Set requirements".to_owned(),
                kind: "requirement".to_owned(),
                bytes: text.len(),
                state: "included",
                run_only: false,
                address: None,
                reference: format!("context-sources/set-physical/{version}/requirements"),
            }],
        );
        blocks.insert(
            node.to_owned(),
            SeedBlock {
                required: text.to_owned(),
                optional: String::new(),
                required_name: Some("Physical address".to_owned()),
            },
        );
    }
    let manifest = SeedManifest {
        schema: 1,
        id: package_id.clone(),
        items: Vec::new(),
        nodes,
        delivery,
        blocks,
    };
    let bytes = serde_json::to_vec(&manifest)?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let package = run_dir.join("context-sources");
    fs::create_dir_all(package.join("reads"))?;
    fs::set_permissions(&package, fs::Permissions::from_mode(0o700))?;
    fs::set_permissions(package.join("reads"), fs::Permissions::from_mode(0o700))?;
    fs::write(package.join("manifest.json"), bytes)?;
    fs::set_permissions(
        package.join("manifest.json"),
        fs::Permissions::from_mode(0o600),
    )?;
    let mut source: Value = serde_json::from_slice(&fs::read(run_dir.join("run.json"))?)?;
    source["context_sources"] = json!({"id": package_id, "digest": digest});
    fs::write(
        run_dir.join("run.json"),
        serde_json::to_vec_pretty(&source)?,
    )?;
    Ok(())
}

pub(super) fn capturing_drivers(seen: Arc<Mutex<Vec<String>>>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Reader { seen });
    Arc::new(move |_| Arc::clone(&driver))
}

struct Reader {
    seen: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl AgentDriver for Reader {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: None,
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(spec.prompt);
        Ok(Box::new(Turn {
            events,
            session: SessionRef {
                vendor: self.id(),
                id: spec.run_id.to_string(),
            },
        }))
    }
}

struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        None
    }

    async fn send(&mut self, _: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        let outcome = Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "Read the recorded reference material.".to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session(),
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
