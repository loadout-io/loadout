//! CT-07: wybór rozmowy dociera tą samą drogą do Leada i uruchomionego kroku.

// `expect` oraz długie scenariusze są rachunkiem integracyjnym, nie kodem produkcyjnym.
#![allow(clippy::expect_used, clippy::too_many_lines)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::bridge::host::Answers;
use loadout_lib::bridge::library::Desk;
use loadout_lib::bridge::{Answer, Call};
use loadout_lib::commands::Drivers;
use loadout_lib::commands::lead_start::{LeadContextChoice, LeadContextTarget};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, DriverConfiguration, FinishReason, Outcome,
    Probe, RunSpec, SessionRef, StepSettings, Tokens,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::supervisor::{FilesystemFence, GroupId, GroupProof, StepTag};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::{AppState, LineSource, line_channel};
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::step_receives_selected_context::{Bench, Published, pin, step};

const TERMINAL: &str = "terminal-lead-context";
const LEAD_ID: &str = "01990000-0000-7000-8000-000000000606";
const PATIENCE: Duration = Duration::from_secs(20);

#[derive(Debug, Default)]
struct Seen {
    lead_prompt: Mutex<Option<String>>,
    step_prompts: Mutex<Vec<String>>,
}

impl Seen {
    fn remember<T>(slot: &Mutex<Option<T>>, value: T) {
        *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(value);
    }
}

#[derive(Clone, Debug)]
struct Driver {
    seen: Arc<Seen>,
}

#[async_trait]
impl AgentDriver for Driver {
    fn id(&self) -> &'static str {
        "claude"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("ct-07-controlled-driver".to_owned()),
        })
    }

    fn configured(&self, _configuration: &DriverConfiguration) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }

    fn with_settings(
        &self,
        _settings: &StepSettings,
    ) -> Option<anyhow::Result<Arc<dyn AgentDriver>>> {
        Some(Ok(Arc::new(self.clone())))
    }

    fn with_evidence(&self, _target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }

    fn with_filesystem_fence(&self, _fence: &FilesystemFence) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }

    fn for_step(&self, _tag: &StepTag) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let session = SessionRef {
            vendor: "claude",
            id: spec.run_id.to_string(),
        };
        if spec.prompt.contains("PLAN-WITH-CONTEXT") {
            Seen::remember(&self.seen.lead_prompt, spec.prompt);
        } else {
            self.seen
                .step_prompts
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(spec.prompt);
        }
        Ok(Box::new(Turn { events, session }))
    }
}

struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
}

impl std::fmt::Debug for Turn {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Ct07Turn")
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
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

    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        let outcome = Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "The selected Context was used.".to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
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

fn app(bench: &Bench, seen: Arc<Seen>) -> Result<AppState, Box<dyn Error>> {
    let driver = Driver { seen };
    let drivers: Drivers = Arc::new(move |_vendor| Arc::new(driver.clone()));
    Ok(AppState::new(
        bench.home.path().to_path_buf(),
        bench.project.path().to_path_buf(),
        Store::open(&bench.db())?,
        drivers,
    ))
}

fn selected(
    material: &Published,
) -> Result<Vec<loadout_lib::workflow::context::ContextPin>, Box<dyn Error>> {
    Ok(serde_json::from_value(pin(material)["sets"].clone())?)
}

fn workflow(bench: &Bench) -> Result<PathBuf, Box<dyn Error>> {
    bench.workflow(
        "lead-context-run",
        &json!({
            "format": 2,
            "id": "ct-07-lead-context-run",
            "name": "Lead context run",
            "steps": [
                step("chosen", "CTX-LEAD-CHOSEN execute the plan.", json!({
                    "schema":1,"inheritWorkflow":false,"exclude":[],"sets":[]
                })),
                step("other", "CTX-LEAD-OTHER do unrelated work.", json!({
                    "schema":1,"inheritWorkflow":false,"exclude":[],"sets":[]
                }))
            ],
            "links": [{"from":"chosen","to":"other"}]
        }),
    )
}

async fn request_from(source: &mut LineSource) -> Result<Line, Box<dyn Error>> {
    Ok(tokio::time::timeout(PATIENCE, async {
        loop {
            if let Some(line @ Line::RunRequested { .. }) = source.try_next() {
                return line;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await?)
}

fn request_id(line: &Line) -> Result<String, Box<dyn Error>> {
    match line {
        Line::RunRequested { request_id, .. } => Ok(request_id.clone()),
        other => Err(format!("expected RunRequested, got {other:?}").into()),
    }
}

async fn begin_plan(
    state: &AppState,
    bench: &Bench,
    material: &Published,
) -> Result<LineSource, Box<dyn Error>> {
    let folder = bench.project.path().to_string_lossy();
    let (sink, source) = line_channel(256);
    state
        .watching_the_lead(TERMINAL, Some(&folder), sink)
        .await?;
    state
        .pin_context_to_chat_inner(TERMINAL, &folder, selected(material)?)
        .await?;
    state
        .say_to_the_lead(
            TERMINAL,
            Some(&folder),
            Some(LEAD_ID),
            "PLAN-WITH-CONTEXT prepare and start this workflow.",
        )
        .await?;
    Ok(source)
}

async fn lead_desk(
    state: &AppState,
    bench: &Bench,
) -> Result<(Arc<Desk>, LineSource), Box<dyn Error>> {
    let folder = bench.project.path().to_string_lossy();
    let selected = state
        .chat_context_selection_inner(TERMINAL, &folder)
        .await?;
    let (sink, source) = line_channel(256);
    let desk = Desk::at(
        Some(bench.home.path().to_path_buf()),
        bench.project.path().to_path_buf(),
    )
    .showing(Arc::new(Mutex::new(sink)))
    .starting_with(state.lead_starts(), Arc::new(Mutex::new(Uuid::now_v7())))
    .using_context(selected, CancellationToken::new());
    Ok((Arc::new(desk), source))
}

async fn ask(desk: &Desk, name: &str, input: Value) -> Answer {
    desk.answer(Call {
        id: json!(707),
        call: name.to_owned(),
        input,
    })
    .await
}

fn start_from(desk: Arc<Desk>) -> tokio::task::JoinHandle<Answer> {
    tokio::spawn(async move {
        ask(
            &desk,
            "start_workflow",
            json!({"workflow":"lead-context-run","task":"Use the approved plan."}),
        )
        .await
    })
}

fn ok(answer: Answer) -> anyhow::Result<Value> {
    match answer {
        Answer::Ok(value) => Ok(value),
        other => anyhow::bail!("the bridge call was refused: {other:?}"),
    }
}

fn run_folder(project: &Path, run_id: &str) -> Result<PathBuf, Box<dyn Error>> {
    fs::read_dir(project.join(".loadout/runs"))?
        .filter_map(Result::ok)
        .find_map(|entry| {
            let value: Value =
                serde_json::from_slice(&fs::read(entry.path().join("run.json")).ok()?).ok()?;
            (value["id"] == run_id).then(|| entry.path())
        })
        .ok_or_else(|| format!("run {run_id} was not published").into())
}

fn publish_new_latest(
    bench: &Bench,
    material: &Published,
    revision: &str,
) -> Result<(), Box<dyn Error>> {
    let set = loadout_lib::context::files::folder_of(
        &loadout_lib::context::files::library_root(bench.home.path()),
        &material.set_id,
    )?;
    let from = set.join("versions").join(&material.revision);
    let to = set.join("versions").join(revision);
    fs::create_dir_all(to.join("topics"))?;
    for relative in [
        "findings.json".to_owned(),
        "index.md".to_owned(),
        format!("topics/{}.md", material.topic_id),
    ] {
        fs::copy(from.join(&relative), to.join(relative))?;
    }
    let mut version: Value = serde_json::from_slice(&fs::read(from.join("manifest.json"))?)?;
    version["id"] = json!(revision);
    fs::write(
        to.join("manifest.json"),
        serde_json::to_vec_pretty(&version)?,
    )?;
    let mut definition: Value = serde_json::from_slice(&fs::read(set.join("manifest.json"))?)?;
    definition["latestReadyRevision"] = json!(revision);
    fs::write(
        set.join("manifest.json"),
        serde_json::to_vec_pretty(&definition)?,
    )?;
    Ok(())
}

#[test]
fn adjacent_agent_steps_show_their_context_difference_without_a_plan_name()
-> Result<(), Box<dyn Error>> {
    let file: loadout_lib::workflow::WorkflowFile = serde_json::from_value(json!({
        "format": 2,
        "id": "ct-07-context-difference",
        "name": "Ordinary graph",
        "steps": [
            step("research", "Research", json!({
                "schema": 1,
                "inheritWorkflow": false,
                "exclude": [],
                "sets": [{"id":"set-alpha","revision":"revision-1","topics":"all"}]
            })),
            step("build", "Build", json!({
                "schema": 1,
                "inheritWorkflow": false,
                "exclude": [],
                "sets": []
            }))
        ],
        "links": [{"from":"research","to":"build"}]
    }))?;
    let notes = loadout_lib::workflow::context::differences_between_steps(&file);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].message.contains("set-alpha"), "{notes:?}");
    assert!(notes[0].message.contains("research"), "{notes:?}");
    assert!(notes[0].message.contains("build"), "{notes:?}");
    Ok(())
}

#[tokio::test]
async fn a_foreign_context_id_is_refused_and_shown_in_the_conversation()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let material = bench.publish(
        "Private", "Private", "private", "Private", "PRIVATE", "PRIVATE",
    )?;
    let state = app(&bench, Arc::new(Seen::default()))?;
    let _conversation = begin_plan(&state, &bench, &material).await?;
    let (desk, mut source) = lead_desk(&state, &bench).await?;
    let refused = ask(
        &desk,
        "read_context",
        json!({"id":"a-context-item-from-another-conversation"}),
    )
    .await;
    let Answer::Refused(refused) = refused else {
        return Err(format!("another conversation's Context was exposed: {refused:?}").into());
    };
    let Some(Line::Problem { text, .. }) = source.try_next() else {
        return Err("the foreign Context refusal was not shown in the conversation".into());
    };
    assert_eq!(text, refused);
    assert!(
        refused.contains("not given to this conversation"),
        "{refused}"
    );
    Ok(())
}

#[tokio::test]
async fn a_shared_run_only_set_respects_a_steps_explicit_exclusion() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let material = bench.publish(
        "Excluded",
        "Excluded",
        "excluded",
        "Excluded",
        "EXCLUDED-REQUIREMENT",
        "EXCLUDED-SOURCE",
    )?;
    bench.workflow(
        "lead-context-run",
        &json!({
            "format": 2,
            "id": "ct-07-shared-exclusion",
            "name": "Lead context run",
            "steps": [step("build", "BUILD-WITHOUT-EXCLUDED", json!({
                "schema": 1,
                "inheritWorkflow": true,
                "exclude": [material.set_id],
                "sets": []
            }))],
            "links": []
        }),
    )?;
    let seen = Arc::new(Seen::default());
    let state = app(&bench, Arc::clone(&seen))?;
    let _conversation = begin_plan(&state, &bench, &material).await?;
    let (desk, mut source) = lead_desk(&state, &bench).await?;
    let starting = start_from(desk);
    let request = request_from(&mut source).await?;
    let (sink, _lines) = line_channel(256);
    state
        .accept_lead_start_with_context_inner(
            &request_id(&request)?,
            1,
            None,
            false,
            Some(LeadContextChoice {
                target: LeadContextTarget::Workflow,
                keep_previous: false,
            }),
            sink,
        )
        .await?;
    assert_eq!(ok(starting.await?)?["started"], true);
    let prompt = tokio::time::timeout(PATIENCE, async {
        loop {
            if let Some(prompt) = seen
                .step_prompts
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .first()
                .cloned()
            {
                return prompt;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await?;
    assert!(prompt.contains("BUILD-WITHOUT-EXCLUDED"), "{prompt}");
    assert!(!prompt.contains("EXCLUDED-REQUIREMENT"), "{prompt}");
    Ok(())
}

#[tokio::test]
async fn a_set_pinned_in_the_chat_reaches_the_step_the_lead_started() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let material = bench.publish(
        "Lead brief",
        "Plan and run from the same selected version.",
        "lead",
        "Lead selection",
        "LEAD-REQUIREMENT reaches both prompts.",
        "LEAD-SOURCE-CONTENT reaches only the selected step.",
    )?;
    let workflow = workflow(&bench)?;
    let workflow_before = fs::read(&workflow)?;
    let seen = Arc::new(Seen::default());
    let state = app(&bench, Arc::clone(&seen))?;
    let _conversation = begin_plan(&state, &bench, &material).await?;
    let (desk, mut source) = lead_desk(&state, &bench).await?;
    let listed = ok(ask(&desk, "list_context", json!({})).await)?;
    let id = listed["items"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item["id"].as_str())
        .ok_or("the Lead received an empty Context list")?;
    let lead_read = ok(ask(&desk, "read_context", json!({"id": id})).await)?;
    let lead_prompt = seen
        .lead_prompt
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .ok_or("the conversation driver did not receive a prompt")?;
    assert!(lead_prompt.contains("LEAD-REQUIREMENT"), "{lead_prompt}");
    assert!(lead_read.to_string().contains("LEAD-SOURCE-CONTENT"));
    let starting = start_from(Arc::clone(&desk));
    let line = request_from(&mut source).await?;
    let request_id = request_id(&line)?;
    let wire = serde_json::to_value(&line)?;
    assert_eq!(wire["context"][0]["id"], material.set_id);
    assert_eq!(wire["context"][0]["revision"], material.revision);

    // 2026-09-08 (CT-07) — nowe latest nie unieważnia starej, nadal czytelnej wersji planu.
    publish_new_latest(&bench, &material, "revision-after-plan")?;
    let (sink, _run_lines) = line_channel(4096);
    let receipt = tokio::time::timeout(
        PATIENCE,
        state.accept_lead_start_with_context_inner(
            &request_id,
            1,
            None,
            false,
            Some(LeadContextChoice {
                target: LeadContextTarget::Steps {
                    step_ids: vec!["chosen".to_owned()],
                },
                keep_previous: false,
            }),
            sink,
        ),
    )
    .await??;
    let started = ok(starting.await?)?;
    assert_eq!(started["started"], true);

    assert_eq!(fs::read(&workflow)?, workflow_before);
    let prompts = tokio::time::timeout(PATIENCE, async {
        loop {
            let prompts = seen
                .step_prompts
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            if prompts.len() == 2 {
                return prompts;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .map_err(|_| {
        format!(
            "steps did not finish: prompts={:?}, errors={:?}",
            seen.step_prompts
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
            bench.run_errors()
        )
    })?;
    let chosen = prompts
        .iter()
        .find(|prompt| prompt.contains("CTX-LEAD-CHOSEN"))
        .ok_or("the chosen step did not start")?;
    let other = prompts
        .iter()
        .find(|prompt| prompt.contains("CTX-LEAD-OTHER"))
        .ok_or("the other step did not start")?;
    assert!(chosen.contains("LEAD-REQUIREMENT"), "{chosen}");
    assert!(!other.contains("LEAD-REQUIREMENT"), "{other}");

    let run = run_folder(bench.project.path(), &receipt.run.run_id)?;
    let manifest: Value =
        serde_json::from_slice(&fs::read(run.join("context-sources/manifest.json"))?)?;
    let items = manifest["items"]
        .as_array()
        .ok_or("the run Context manifest did not contain an item list")?;
    assert!(!items.is_empty(), "{manifest}");
    assert!(
        items.iter().all(|item| item["runOnly"] == true),
        "{manifest}"
    );
    let record: Value = serde_json::from_slice(&fs::read(run.join("run.json"))?)?;
    assert!(!record["context_sources"].is_null(), "{record}");
    Ok(())
}

#[tokio::test]
async fn conversations_and_workspaces_do_not_share_their_selection() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let first = bench.publish("First", "First", "first", "First", "FIRST", "FIRST")?;
    let second = bench.publish("Second", "Second", "second", "Second", "SECOND", "SECOND")?;
    let state = app(&bench, Arc::new(Seen::default()))?;
    let folder = bench.project.path().to_string_lossy();
    state
        .pin_context_to_chat_inner("conversation-a", &folder, selected(&first)?)
        .await?;
    state
        .pin_context_to_chat_inner("conversation-b", &folder, selected(&second)?)
        .await?;
    let a = state
        .what_this_chat_pinned_inner("conversation-a", &folder)
        .await?;
    let b = state
        .what_this_chat_pinned_inner("conversation-b", &folder)
        .await?;
    assert_eq!(a.pins.sets[0].id, first.set_id);
    assert_eq!(b.pins.sets[0].id, second.set_id);
    assert!(!a.pins.transcript_keeps_previous_context);
    assert!(!b.pins.transcript_keeps_previous_context);

    let elsewhere = tempfile::tempdir()?;
    fs::create_dir_all(elsewhere.path().join(".loadout"))?;
    let refusal = state
        .pin_context_to_chat_inner(
            "conversation-a",
            &elsewhere.path().to_string_lossy(),
            selected(&second)?,
        )
        .await
        .expect_err("the same conversation identity must not move to another workspace");
    assert!(refusal.contains("different workspace"), "{refusal}");
    let elsewhere_folder = elsewhere.path().to_string_lossy();
    state
        .pin_context_to_chat_inner("conversation-c", &elsewhere_folder, selected(&second)?)
        .await?;
    let c = state
        .what_this_chat_pinned_inner("conversation-c", &elsewhere_folder)
        .await?;
    let unchanged_a = state
        .what_this_chat_pinned_inner("conversation-a", &folder)
        .await?;
    assert_eq!(c.pins.sets[0].id, second.set_id);
    assert_eq!(unchanged_a.pins.sets[0].id, first.set_id);
    Ok(())
}

#[tokio::test]
async fn changing_the_selection_after_preview_refuses_until_the_person_keeps_it()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let material = bench.publish("Plan", "Plan", "plan", "Plan", "PLAN", "PLAN")?;
    workflow(&bench)?;
    let state = app(&bench, Arc::new(Seen::default()))?;
    let _conversation = begin_plan(&state, &bench, &material).await?;
    let (desk, mut source) = lead_desk(&state, &bench).await?;
    let starting = start_from(desk);
    let line = request_from(&mut source).await?;
    let id = request_id(&line)?;
    let folder = bench.project.path().to_string_lossy();
    state
        .pin_context_to_chat_inner(TERMINAL, &folder, Vec::new())
        .await?;
    let changed = state.what_this_chat_pinned_inner(TERMINAL, &folder).await?;
    assert!(changed.pins.transcript_keeps_previous_context);
    let (sink, _lines) = line_channel(32);
    let refusal = state
        .accept_lead_start_with_context_inner(
            &id,
            1,
            None,
            false,
            Some(LeadContextChoice {
                target: LeadContextTarget::Workflow,
                keep_previous: false,
            }),
            sink,
        )
        .await
        .expect_err("a changed selection must invalidate the preview");
    assert_eq!(refusal, "Context changed since this plan");
    let (sink, _lines) = line_channel(4096);
    state
        .accept_lead_start_with_context_inner(
            &id,
            1,
            None,
            false,
            Some(LeadContextChoice {
                target: LeadContextTarget::Steps {
                    step_ids: vec!["chosen".to_owned()],
                },
                keep_previous: true,
            }),
            sink,
        )
        .await?;
    assert_eq!(ok(starting.await?)?["started"], true);
    Ok(())
}

#[tokio::test]
async fn a_set_that_vanishes_is_refused_when_pinned() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let material = bench.publish("Exact", "Exact", "exact", "Exact", "EXACT", "EXACT")?;
    let state = app(&bench, Arc::new(Seen::default()))?;
    let set = loadout_lib::context::files::folder_of(
        &loadout_lib::context::files::library_root(bench.home.path()),
        &material.set_id,
    )?;
    fs::remove_dir_all(set)?;
    let folder = bench.project.path().to_string_lossy();
    let pin_refusal = state
        .pin_context_to_chat_inner(TERMINAL, &folder, selected(&material)?)
        .await
        .expect_err("pinning a vanished set must be refused");
    assert!(pin_refusal.contains(&material.set_id), "{pin_refusal}");
    assert!(pin_refusal.contains(&material.revision), "{pin_refusal}");
    assert!(
        pin_refusal.contains("choose another ready version"),
        "{pin_refusal}"
    );
    Ok(())
}

#[tokio::test]
async fn a_version_that_vanishes_after_preview_is_refused_before_start()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let readable = bench.publish(
        "Readable", "Readable", "readable", "Readable", "READABLE", "READABLE",
    )?;
    workflow(&bench)?;
    let state = app(&bench, Arc::new(Seen::default()))?;
    let _conversation = begin_plan(&state, &bench, &readable).await?;
    let (desk, mut source) = lead_desk(&state, &bench).await?;
    let starting = start_from(desk);
    let line = request_from(&mut source).await?;
    let id = request_id(&line)?;
    let selected_version = loadout_lib::context::files::folder_of(
        &loadout_lib::context::files::library_root(bench.home.path()),
        &readable.set_id,
    )?
    .join("versions")
    .join(&readable.revision);
    fs::remove_dir_all(selected_version)?;
    let (sink, _lines) = line_channel(64);
    let start_refusal = state
        .accept_lead_start_with_context_inner(
            &id,
            1,
            None,
            false,
            Some(LeadContextChoice {
                target: LeadContextTarget::Steps {
                    step_ids: vec!["chosen".to_owned()],
                },
                keep_previous: false,
            }),
            sink,
        )
        .await
        .expect_err("Start must not replace a missing selected version with latest");
    assert!(start_refusal.contains("Readable"), "{start_refusal}");
    assert!(
        start_refusal.contains(&readable.revision),
        "{start_refusal}"
    );
    assert!(
        start_refusal.contains("choose another ready version"),
        "{start_refusal}"
    );
    let Answer::Refused(bridge_refusal) = starting.await? else {
        return Err("the Lead's Start call was not refused".into());
    };
    assert_eq!(bridge_refusal, start_refusal);
    Ok(())
}

#[tokio::test]
async fn a_chat_version_conflicting_with_the_workflow_is_refused_before_a_step_starts()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let original = bench.publish(
        "Conflict", "Conflict", "conflict", "Conflict", "ORIGINAL", "ORIGINAL",
    )?;
    let workflow = workflow(&bench)?;
    let mut file: Value = serde_json::from_slice(&fs::read(&workflow)?)?;
    file["steps"][0]["context"] = pin(&original);
    fs::write(&workflow, serde_json::to_vec_pretty(&file)?)?;
    let newer_revision = "revision-conflict-newer";
    publish_new_latest(&bench, &original, newer_revision)?;
    let newer = Published {
        revision: newer_revision.to_owned(),
        ..original.clone()
    };
    let state = app(&bench, Arc::new(Seen::default()))?;
    let _conversation = begin_plan(&state, &bench, &newer).await?;
    let (desk, mut source) = lead_desk(&state, &bench).await?;
    let starting = start_from(desk);
    let request = request_from(&mut source).await?;
    let id = request_id(&request)?;
    let (sink, _lines) = line_channel(64);
    let refusal = state
        .accept_lead_start_with_context_inner(
            &id,
            1,
            None,
            false,
            Some(LeadContextChoice {
                target: LeadContextTarget::Steps {
                    step_ids: vec!["chosen".to_owned()],
                },
                keep_previous: false,
            }),
            sink,
        )
        .await
        .expect_err("two versions of one Context set must not enter the same step");
    assert!(refusal.contains(&original.revision), "{refusal}");
    assert!(refusal.contains(newer_revision), "{refusal}");
    assert!(
        refusal.contains("Choose one version before starting"),
        "{refusal}"
    );
    let Answer::Refused(bridge_refusal) = starting.await? else {
        return Err("the Lead's conflicting Start call was not refused".into());
    };
    assert_eq!(bridge_refusal, refusal);
    Ok(())
}
