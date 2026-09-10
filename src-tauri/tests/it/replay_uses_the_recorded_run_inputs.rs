//! WF-23 RED: zapisany graf i Agent wchodzą do tego samego produkcyjnego Startu.
//! Nie ma vendora; dubler obserwuje rzeczywiste `RunSpec` i bajty przygotowanej kopii.
#![allow(clippy::too_many_lines)]
#![allow(clippy::assigning_clones)]
#![allow(clippy::panic)]

use async_trait::async_trait;
use loadout_lib::bridge::host::Answers;
use loadout_lib::bridge::library::{Desk, Waiting};
use loadout_lib::bridge::{Answer, Call};
use loadout_lib::commands::{Drivers, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{AppState, LineSource, line_channel};
use loadout_lib::library::agents::{Agent, write_agent_file};
use loadout_lib::store::Store;
use serde_json::{Value, json};
use std::error::Error;
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug)]
struct Observed {
    model: Option<String>,
    prompt: String,
    input: String,
}

#[tokio::test]
async fn recorded_replay_executes_the_old_graph_model_and_files_after_the_library_changes()
-> Result<(), Box<dyn Error>> {
    recorded_case(false, false, false).await
}

#[tokio::test]
async fn a_historical_workflow_is_saved_under_a_new_identity_without_overwriting_today()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let home = root.path().join("home");
    let project = root.path().join("project");
    fs::create_dir_all(project.join(".loadout/workflows"))?;
    fs::create_dir_all(project.join(".loadout"))?;
    let old = json!({"format":1,"id":"historical-workflow-id","name":"Saved workflow","links":[],
        "steps":[{"kind":"agent","id":"build","name":"Build","agent":Agent::example().id,
            "instructions":"Historical step instructions","overrides":{},"folder":{"use":"fresh-copy"},"at":{"x":0,"y":0}}]});
    let mut current = old.clone();
    current["name"] = json!("Today's workflow");
    current["steps"][0]["instructions"] = json!("Today's instructions");
    let current_path = project.join(".loadout/workflows/saved.json");
    fs::write(&current_path, current.to_string())?;
    let current_bytes = fs::read(&current_path)?;
    let id = uuid::Uuid::now_v7().to_string();
    let source = project
        .join(".loadout/runs")
        .join(format!("20260906-010000__{id}"));
    fs::create_dir_all(&source)?;
    fs::write(
        source.join("run.json"),
        json!({"id":id,"status":"succeeded","workflow_snapshot":old}).to_string(),
    )?;
    let source_bytes = fs::read(source.join("run.json"))?;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let driver: Arc<dyn AgentDriver> = Arc::new(Driver {
        seen: Arc::clone(&seen),
    });
    let drivers: Drivers = Arc::new(move |_| Arc::clone(&driver));
    let state = AppState::new(
        home.clone(),
        project.clone(),
        Store::open(&project.join(".loadout/loadout.db"))?,
        drivers,
    );
    let copied = state
        .copy_recorded_workflow_inner(project.to_str().ok_or("project path")?, &id)
        .await?;
    let filename = copied["fileName"]
        .as_str()
        .ok_or("the saved copy has no file name")?;
    let loaded = loadout_lib::commands::workflows::load_workflow_inner(
        &project.join(".loadout"),
        None,
        filename,
    )?;
    assert_ne!(loaded.workflow.id, "historical-workflow-id");
    assert!(uuid::Uuid::parse_str(&loaded.workflow.id).is_ok());
    assert_eq!(loaded.workflow.id, copied["workflowId"]);
    let expected: loadout_lib::workflow::WorkflowFile = serde_json::from_value(old)?;
    assert_eq!(loaded.workflow.steps, expected.steps);
    assert_eq!(loaded.workflow.links, expected.links);
    assert_eq!(fs::read(&current_path)?, current_bytes);
    assert_eq!(fs::read(source.join("run.json"))?, source_bytes);
    assert!(
        seen.lock()
            .map_err(|_| "driver observations poisoned")?
            .is_empty(),
        "copying a definition ran a model"
    );
    let mut changed: Value = serde_json::from_slice(&source_bytes)?;
    changed["id"] = json!(uuid::Uuid::now_v7());
    fs::write(source.join("run.json"), changed.to_string())?;
    assert!(
        state
            .copy_recorded_workflow_inner(project.to_str().ok_or("project path")?, &id)
            .await
            .is_err(),
        "a directory cannot impersonate another saved run"
    );
    assert_eq!(fs::read_dir(project.join(".loadout/workflows"))?.count(), 2);
    Ok(())
}

#[tokio::test]
async fn the_person_sees_the_frozen_limit_and_settings_in_the_actual_replay_question()
-> Result<(), Box<dyn Error>> {
    recorded_case(true, false, false).await
}

#[tokio::test]
async fn rerun_step_addresses_the_source_and_returns_the_shared_current_preview()
-> Result<(), Box<dyn Error>> {
    recorded_case(false, true, false).await
}

#[tokio::test]
async fn current_rerun_uses_each_older_copy_result_even_after_a_newer_run()
-> Result<(), Box<dyn Error>> {
    recorded_case(false, false, true).await
}

async fn recorded_case(
    check_question: bool,
    check_rerun: bool,
    composed: bool,
) -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let home = root.path().join("home");
    let project = root.path().join("project");
    fs::create_dir_all(project.join(".loadout/workflows"))?;
    fs::create_dir_all(project.join(".loadout"))?;
    fs::write(project.join("seed.txt"), "original input")?;
    let mut agent = Agent::example();
    agent.model = "historical-model".to_owned();
    agent.instructions = "Historical agent instructions".to_owned();
    let written_agent = write_agent_file(&project.join(".loadout/agents"), &agent, None)?;
    let workflow = project.join(".loadout/workflows/replay.json");
    let graph = json!({"format":1,"id":"wf-replay","name":"Saved configuration","links":[],
        "steps":[{"kind":"agent","id":"build","name":"Build","agent":agent.id,
            "copies":2,"instructions":if composed {"Historical Copy marker {{copy}} of {{copies}}."} else {"Historical copy {{copy}} of {{copies}}."},"overrides":{},
            "folder":{"use":"fresh-copy"},"at":{"x":0,"y":0}}]});
    fs::write(&workflow, graph.to_string())?;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let driver: Arc<dyn AgentDriver> = Arc::new(Driver {
        seen: Arc::clone(&seen),
    });
    let drivers: Drivers = Arc::new(move |_| Arc::clone(&driver));
    let state = Arc::new(AppState::new(
        home.clone(),
        project.clone(),
        Store::open(&project.join(".loadout/loadout.db"))?,
        drivers,
    ));
    let deps = state.begin_run(&project)?;
    let (sink, _source) = line_channel(512);
    let source = loadout_lib::commands::run::run_workflow_with_budget(
        &deps,
        &RunRequest {
            workflow: workflow.clone(),
            how_many_at_once: 2,
            task: None,
            part: None,
            handoffs_from: None,
        },
        sink,
        Some(4.0),
    )
    .await?;
    assert_eq!(seen.lock().map_err(|_| "observations poisoned")?.len(), 2);
    let source_bytes = fs::read(source.dir.join("run.json"))?;
    let source_file: Value = serde_json::from_slice(&source_bytes)?;
    let source_id = source_file["id"].as_str().ok_or("source run has no ID")?;
    // Dwie niezależne zmiany: inny model oraz graf o innej liczbie sesji.
    agent.model = "current-model".to_owned();
    agent.instructions = "Current agent instructions".to_owned();
    write_agent_file(
        &project.join(".loadout/agents"),
        &agent,
        Some(&written_agent.revision),
    )?;
    let mut current = graph;
    current["steps"][0]["copies"] = json!(if composed { 2 } else { 1 });
    current["steps"][0]["instructions"] = json!(if composed {
        "Current Copy marker {{copy}} of {{copies}}."
    } else {
        "Current instructions"
    });
    fs::write(&workflow, current.to_string())?;
    // 2026-09-06: pełny Recorded odtwarza dawny input mimo zmian gospodarza;
    // Current/Just z wieloma kopiami wymaga tej samej bazy. A1/A2 różnią się
    // wynikami własnych kopii, nie niepowiązaną zmianą hostowego projektu.
    if !composed {
        fs::write(project.join("seed.txt"), "current input must not leak")?;
    }
    let newer = if composed {
        let deps = state.begin_run(&project)?;
        let (sink, _events) = line_channel(512);
        let report = loadout_lib::commands::run::run_workflow_with_budget(
            &deps,
            &RunRequest {
                workflow: workflow.clone(),
                how_many_at_once: 2,
                task: None,
                part: None,
                handoffs_from: None,
            },
            sink,
            Some(4.0),
        )
        .await?;
        let bytes = fs::read(report.dir.join("run.json"))?;
        let file: Value = serde_json::from_slice(&bytes)?;
        let id = file["id"]
            .as_str()
            .ok_or("newer run has no identity")?
            .to_owned();
        assert_ne!(id, source_id);
        assert_eq!(
            seen.lock().map_err(|_| "observations poisoned")?.len(),
            4,
            "A2 must really complete both copies before the older rerun"
        );
        assert_eq!(
            fs::read_to_string(project.join("seed.txt"))?,
            "original input",
            "independent copy results must leave the shared origin unchanged"
        );
        Some((id, report.dir, bytes))
    } else {
        None
    };
    let previous_starts = if composed { 4 } else { 2 };
    let waiting = Arc::new(Waiting::default());
    let (sink, mut conversation) = line_channel(512);
    let desk = Desk::at(Some(project.join(".loadout")), project.clone())
        .showing(Arc::new(Mutex::new(sink)))
        .hearing(Arc::clone(&waiting))
        .starting_with(
            state.lead_starts(),
            Arc::new(Mutex::new(uuid::Uuid::now_v7())),
        );
    let current_preview = desk
        .answer(call(
            "prepare_replay",
            json!({"source_run_id":source_id,
        "selection":{"kind":"all"},"mode":"current"}),
        ))
        .await;
    let Answer::Ok(current_preview) = current_preview else {
        panic!("current preview was refused: {current_preview:?}")
    };
    assert_eq!(
        current_preview["workflowChanged"], true,
        "current must disclose the difference from the historical graph, not compare itself with itself"
    );
    if check_rerun {
        let answer = desk
            .answer(call(
                "rerun_step",
                json!({"source_run_id":source_id,"step_id":"build"}),
            ))
            .await;
        let Answer::Ok(preview) = answer else {
            panic!("the addressed rerun did not prepare the shared preview: {answer:?}")
        };
        assert_eq!(preview["mode"], "current");
        assert_eq!(preview["source"]["runId"], source_id);
        assert_eq!(
            preview["copies"], 1,
            "the current tile has one copy; do not silently repeat the old graph"
        );
        assert_eq!(preview["workflowChanged"], true);
        assert!(
            state
                .lead_starts()
                .claim(preview["previewId"].as_str().ok_or("missing preview ID")?)
                .is_err(),
            "a rerun preview is not human approval"
        );
        assert!(
            matches!(
                desk.answer(call(
                    "rerun_step",
                    json!({"source_run_id":source_id,"step_id":"build~1"})
                ))
                .await,
                Answer::Refused(_)
            ),
            "one physical copy cannot silently select a tile"
        );
        assert_eq!(fs::read(source.dir.join("run.json"))?, source_bytes);
        return Ok(());
    }
    let preview = if composed {
        desk.answer(call(
            "rerun_step",
            json!({"source_run_id":source_id,"step_id":"build"}),
        ))
        .await
    } else {
        desk.answer(call(
            "prepare_replay",
            json!({"source_run_id":source_id,
        "selection":{"kind":"all"},"mode":"recorded"}),
        ))
        .await
    };
    let Answer::Ok(preview) = preview else {
        panic!("complete recorded inputs must produce a preview: {preview:?}")
    };
    let preview_id = preview["previewId"]
        .as_str()
        .ok_or("no replay preview identity")?;
    assert_eq!(
        preview["mode"],
        if composed { "current" } else { "recorded" }
    );
    assert_eq!(preview["source"]["runId"], source_id);
    assert_eq!(preview["copies"], 2);
    assert_eq!(
        preview["costBoundUsd"], 4.0,
        "the approved spending limit must be shown before starting"
    );
    assert_eq!(
        preview["budgetSaid"],
        "Spending limit: $4.00 for this new run."
    );
    assert_eq!(
        preview["configurationSaid"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        current_preview["costBoundUsd"], 4.0,
        "Current changes configuration, not the approved spending limit"
    );
    assert!(
        state.lead_starts().claim(preview_id).is_err(),
        "an unconfirmed preview must not become a run through the window acceptance endpoint"
    );
    assert!(
        matches!(
            desk.answer(call(
                "start_replay",
                json!({"preview_id":preview_id,"confirmed":true})
            ))
            .await,
            Answer::Refused(_)
        ),
        "a model cannot approve its own repeat"
    );
    assert_eq!(
        seen.lock().map_err(|_| "observations poisoned")?.len(),
        previous_starts,
        "preview started a model"
    );
    let asking = desk.answer(call(
        "ask_the_person",
        json!({"operation":"start_replay","preview_id":preview_id}),
    ));
    tokio::pin!(asking);
    let question = tokio::select! {
        answer = &mut asking => panic!("replay never asked the person: {answer:?}"),
        line = next(&mut conversation, "asked") => line,
    };
    let Line::Asked {
        question: Some(question),
        options,
        text,
        ..
    } = question
    else {
        panic!("confirmation has no host identity")
    };
    assert!(options.iter().any(|one| one == "Start replay"));
    if check_question {
        assert!(
            text.contains("Spending limit: $4.00 for this new run."),
            "the human saw no approved cost bound: {text}"
        );
        assert!(
            text.contains("historical-model")
                && text.contains("Claude Code")
                && text.contains("Work freely"),
            "the actual question hid the settings: {text}"
        );
        return Ok(());
    }
    assert!(waiting.answer_exact("Lead", &question.question_id, "Start replay".to_owned()));
    let Answer::Ok(approved) = asking.await else {
        panic!("human replay approval was refused")
    };
    let start = desk.answer(call(
        "start_replay",
        json!({"preview_id":preview_id,
        "confirmation_token":approved["approvalToken"]}),
    ));
    tokio::pin!(start);
    let request = tokio::select! {
        answer = &mut start => panic!("replay returned before the production Start accepted it: {answer:?}"),
        line = next(&mut conversation, "runRequested") => line,
    };
    let Line::RunRequested { request_id, .. } = request else {
        panic!("replay bypassed the normal Start transport")
    };
    let (sink, _output) = line_channel(512);
    let (accepted, answer) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(
            state.accept_lead_start_inner(&request_id, 2, Some(8.0), false, sink),
            start
        )
    })
    .await?;
    let accepted = accepted?;
    let description =
        loadout_lib::commands::lead_history::source_for(&project, &accepted.run.run_id)?;
    let new_run: Value = serde_json::from_slice(&fs::read(
        project
            .join(".loadout/runs")
            .join(description.run_folder)
            .join("run.json"),
    )?)?;
    assert_eq!(
        new_run["budget_usd"], preview["costBoundUsd"],
        "the adapter raised the confirmed repeat's spending limit from $4 to $8"
    );
    let Answer::Ok(answer) = answer else {
        panic!("recorded replay did not start: {answer:?}")
    };
    assert_ne!(answer["run"]["runId"], source_id);
    assert!(
        matches!(
            desk.answer(call(
                "start_replay",
                json!({"preview_id":preview_id,
        "confirmation_token":approved["approvalToken"]})
            ))
            .await,
            Answer::Refused(_)
        ),
        "the actual person's approval must be usable only once"
    );
    assert_eq!(
        fs::read(source.dir.join("run.json"))?,
        source_bytes,
        "replay changed its source history"
    );
    {
        let observed = seen.lock().map_err(|_| "observations poisoned")?;
        assert_eq!(
            observed.len(),
            previous_starts + 2,
            "the addressed tile must execute both copies"
        );
        for one in &observed[previous_starts..] {
            if composed {
                assert_eq!(one.model.as_deref(), Some("current-model"));
                assert!(one.prompt.contains("Current Copy marker"));
                let copy = if one.prompt.contains("Copy marker 1 of 2") {
                    1
                } else {
                    2
                };
                assert_eq!(
                    one.input,
                    format!("historical-model:copy{copy}"),
                    "the rerun read the newer run, host files or its neighboring copy"
                );
            } else {
                assert_eq!(one.model.as_deref(), Some("historical-model"));
                assert!(one.prompt.contains("Historical copy"));
                assert!(!one.prompt.contains("Current instructions"));
                assert_eq!(one.input, "original input");
            }
        }
        if composed {
            let mut inputs: Vec<_> = observed[previous_starts..]
                .iter()
                .map(|one| one.input.as_str())
                .collect();
            inputs.sort_unstable();
            assert_eq!(
                inputs,
                ["historical-model:copy1", "historical-model:copy2"],
                "both exact older copies must be represented once"
            );
        }
    }
    if let Some((newer_id, newer_dir, newer_bytes)) = newer {
        assert_ne!(accepted.run.run_id, newer_id);
        assert_eq!(fs::read(newer_dir.join("run.json"))?, newer_bytes);
        assert_eq!(fs::read(source.dir.join("run.json"))?, source_bytes);
        assert_eq!(
            fs::read_to_string(project.join("seed.txt"))?,
            "original input",
            "replaying older copies must not overwrite the host project"
        );
        return Ok(());
    }
    // Bez tury modelu: produkcyjny adapter historii dostaje zgodę wyłącznie z przycisku.
    let folder = project.to_str().ok_or("fixture path is not text")?;
    let ui_preview = state
        .prepare_replay_inner(folder, source_id, json!({"kind":"all"}), "recorded")
        .await?;
    let ui_id = ui_preview["previewId"]
        .as_str()
        .ok_or("UI preview has no identity")?;
    assert!(
        state
            .authorize_replay_inner(folder, ui_id, "Keep viewing history".to_owned())
            .await
            .is_err()
    );
    let authorized = state
        .authorize_replay_inner(folder, ui_id, "Start replay".to_owned())
        .await?;
    let Line::RunRequested { request_id, .. } = authorized else {
        panic!("history bypassed the shared Start request")
    };
    let (sink, _events) = line_channel(512);
    let receipt = state
        .accept_lead_start_inner(&request_id, 2, Some(8.0), false, sink)
        .await?;
    assert_ne!(receipt.run.run_id, source_id);
    assert_eq!(seen.lock().map_err(|_| "observations poisoned")?.len(), 6);
    assert!(
        state
            .authorize_replay_inner(folder, ui_id, "Start replay".to_owned())
            .await
            .is_err()
    );
    // Zmiana samego source po podglądzie unieważnia zgodę, mimo że graf nadal jest poprawny.
    let changed_preview = state
        .prepare_replay_inner(folder, source_id, json!({"kind":"all"}), "recorded")
        .await?;
    let mut changed_source = source_file;
    changed_source["title"] = json!("A different source after preview");
    fs::write(source.dir.join("run.json"), changed_source.to_string())?;
    let refusal = state
        .authorize_replay_inner(
            folder,
            changed_preview["previewId"]
                .as_str()
                .ok_or("missing preview")?,
            "Start replay".to_owned(),
        )
        .await
        .err()
        .ok_or("a source replacement cannot inherit approval")?;
    assert!(refusal.contains("saved run changed"), "{refusal}");
    assert_eq!(seen.lock().map_err(|_| "observations poisoned")?.len(), 6);
    fs::write(source.dir.join("run.json"), &source_bytes)?;
    Ok(())
}

#[tokio::test]
async fn replacing_the_current_tile_agent_cannot_restore_the_old_agents_broader_permissions()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let project = root.path().join("project");
    fs::create_dir_all(project.join(".loadout/workflows"))?;
    fs::create_dir_all(&project)?;
    fs::write(project.join("seed.txt"), "saved source")?;
    let old = Agent::example();
    write_agent_file(&project.join(".loadout/agents"), &old, None)?;
    let mut replacement = Agent::example();
    replacement.id = uuid::Uuid::now_v7();
    replacement.name = "Read-only replacement".to_owned();
    replacement.file_access = loadout_lib::library::agents::FileAccess::LookOnly;
    write_agent_file(&project.join(".loadout/agents"), &replacement, None)?;
    let recorded = json!({"format":1,"id":"permission-replay","name":"Saved permissions","links":[],
        "steps":[{"kind":"agent","id":"build","name":"Build","agent":old.id,"instructions":"Read the input", "overrides":{},"folder":{"use":"fresh-copy"},"at":{"x":0,"y":0}}]});
    let mut current = recorded.clone();
    current["steps"][0]["agent"] = json!(replacement.id);
    fs::write(
        project.join(".loadout/workflows/permission.json"),
        current.to_string(),
    )?;
    let id = uuid::Uuid::now_v7().to_string();
    let source = project
        .join(".loadout/runs")
        .join(format!("20260906-010500__{id}"));
    fs::create_dir_all(&source)?;
    let input = loadout_lib::commands::input_snapshot::capture(&project, &source)?;
    let instructions = loadout_lib::inherit::instructions::for_run(
        &project,
        &source,
        &[("build".to_owned(), Some(false))],
        None,
    )?;
    fs::write(source.join("run.json"), json!({"id":id,"workflow_snapshot":recorded,"status":"succeeded","task":"",
        "input_snapshot":{"id":input.id()},"project_instructions":{"id":instructions.id(),"digest":instructions.package_digest()?},
        "steps":[{"node_key":"build","effective":old}]}).to_string())?;
    let (sink, mut shown) = line_channel(32);
    let desk = Desk::at(Some(project.join(".loadout")), project)
        .showing(Arc::new(Mutex::new(sink)))
        .starting_with(
            Arc::new(loadout_lib::commands::lead_start::LeadStarts::default()),
            Arc::new(Mutex::new(uuid::Uuid::now_v7())),
        );
    let answer = desk
        .answer(call(
            "prepare_replay",
            json!({"source_run_id":id,"mode":"recorded","selection":{"kind":"all"}}),
        ))
        .await;
    let Answer::Refused(said) = answer else {
        panic!(
            "an agent no longer assigned to the tile still supplied the permission ceiling: {answer:?}"
        )
    };
    assert!(said.contains("more access"), "{said}");
    let mut visible = false;
    while let Some(line) = shown.try_next() {
        if let Line::Problem { text, .. } = line {
            visible |= text == said;
        }
    }
    assert!(
        visible,
        "the exact permission refusal must reach the real conversation"
    );
    Ok(())
}

fn call(name: &str, input: Value) -> Call {
    Call {
        id: json!(1),
        call: name.to_owned(),
        input,
    }
}

async fn next(source: &mut LineSource, kind: &str) -> Line {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(line) = source.try_next()
                && serde_json::to_value(&line).is_ok_and(|wire| wire["kind"] == kind)
            {
                return line;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no {kind} on the production conversation"))
}

struct Driver {
    seen: Arc<Mutex<Vec<Observed>>>,
}
#[async_trait]
impl AgentDriver for Driver {
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
            .map_err(|_| anyhow::anyhow!("observations poisoned"))?
            .push(Observed {
                model: spec.model.clone(),
                prompt: spec.prompt.clone(),
                input: fs::read_to_string(spec.cwd.join("seed.txt"))?,
            });
        if spec.prompt.contains("Copy marker") {
            let copy = if spec.prompt.contains("Copy marker 1 of 2") {
                1
            } else if spec.prompt.contains("Copy marker 2 of 2") {
                2
            } else {
                anyhow::bail!("the copy marker was not expanded")
            };
            // Tylko wariant złożony: prawdziwa finalizacja zapisuje odrębne wyniki A1/A2,
            // nie ręcznie zmontowane copy_results w fixture.
            fs::write(
                spec.cwd.join("seed.txt"),
                format!("{}:copy{copy}", spec.model.as_deref().unwrap_or("no-model")),
            )?;
        }
        Ok(Box::new(Handle {
            events,
            session: SessionRef {
                vendor: "claude-code",
                id: spec.run_id.to_string(),
            },
        }))
    }
}
struct Handle {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
}
#[async_trait]
impl AgentHandle for Handle {
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
            text: "Historical result".to_owned(),
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
