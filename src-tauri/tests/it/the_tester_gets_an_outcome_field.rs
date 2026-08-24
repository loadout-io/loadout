//! AC-1 dla T-100: sędzia pętli dostaje wymagane pole `outcome` w zmontowanym prompcie.
//!
//! To kryterium czyta prompt, który naprawdę dotarł do sterownika. Stała z właściwymi słowami
//! nie dowodzi, że sędzia ją dostał, a pole dodane do każdego kroku uczyłoby zwykłe kroki
//! odpowiadać czymś, czego nikt po nich nie czyta (niezmienniki 16 i 29).

#![allow(clippy::expect_used, clippy::too_many_lines, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use serde_json::Value as Json;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

const VENDOR: &str = "fake";
const PATIENCE: Duration = Duration::from_secs(30);

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-000000000100
name: Hand
summary: Does the work
color: moss
runsWith: claude-code
model: opus
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 20
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Do the work.
";

const WITHOUT_HANDOVER: &str = r#"{
  "format": 1,
  "id": "wf_t100_outcome_without_handover",
  "name": "An automatic outcome field",
  "steps": [
    {
      "kind": "agent",
      "id": "s_work",
      "name": "Work",
      "agent": "01990000-0000-7000-8000-000000000100",
      "overrides": {},
      "instructions": "work: make the change.",
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_tester",
      "name": "Tester",
      "agent": "01990000-0000-7000-8000-000000000100",
      "overrides": {},
      "instructions": "tester: decide whether it is good enough.",
      "at": { "x": 0, "y": 200 }
    },
    {
      "kind": "agent",
      "id": "s_after",
      "name": "After",
      "agent": "01990000-0000-7000-8000-000000000100",
      "overrides": {},
      "instructions": "after: build on the accepted work.",
      "at": { "x": 0, "y": 400 }
    }
  ],
  "links": [
    { "from": "s_work", "to": "s_tester" },
    { "from": "s_tester", "to": "s_after" },
    { "from": "s_tester", "to": "s_work", "max_turns": 2 }
  ]
}"#;

const WITH_HANDOVER: &str = r#"{
  "format": 1,
  "id": "wf_t100_outcome_with_handover",
  "name": "An outcome field beside a form",
  "steps": [
    {
      "kind": "agent",
      "id": "s_work",
      "name": "Work",
      "agent": "01990000-0000-7000-8000-000000000100",
      "overrides": {},
      "instructions": "work: make the change.",
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_tester",
      "name": "Tester",
      "agent": "01990000-0000-7000-8000-000000000100",
      "overrides": {},
      "instructions": "tester: decide whether it is good enough.",
      "handover": {
        "fields": [
          { "name": "notes", "describe": "anything else worth keeping" }
        ]
      },
      "at": { "x": 0, "y": 200 }
    },
    {
      "kind": "agent",
      "id": "s_after",
      "name": "After",
      "agent": "01990000-0000-7000-8000-000000000100",
      "overrides": {},
      "instructions": "after: build on the accepted work.",
      "at": { "x": 0, "y": 400 }
    }
  ],
  "links": [
    { "from": "s_work", "to": "s_tester" },
    { "from": "s_tester", "to": "s_after" },
    { "from": "s_tester", "to": "s_work", "max_turns": 2 }
  ]
}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn only_a_loop_tester_gets_the_required_outcome_field() -> Result<(), Box<dyn Error>> {
    let answers = Script::new(&[(
        "tester",
        &[
            "## Answer\nThe work is good.\n\noutcome: pass\n\n## Evidence\nnotes.txt:1\n\n## Open\nnothing.\n",
        ],
    )]);
    let without = run_fixture("without-handover", WITHOUT_HANDOVER, answers).await?;

    let answers = Script::new(&[(
        "tester",
        &[
            "## Answer\nThe work is good.\n\nnotes: nothing else\noutcome: pass\n\n## Evidence\nnotes.txt:1\n\n## Open\nnothing.\n",
        ],
    )]);
    let with = run_fixture("with-handover", WITH_HANDOVER, answers).await?;

    for (case, observed) in [("without a form", &without), ("with a form", &with)] {
        let tester = observed.only_prompt("tester")?;
        let fields: Vec<&str> = tester
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("outcome:"))
            .collect();
        assert_eq!(
            fields.len(),
            1,
            "the tester {case} did not get exactly one `outcome` field in the mounted prompt. \
             A mention inside the fallback prose is not a field the answer reader can take. \
             It got: {tester:?}"
        );
        let field = fields[0].to_ascii_lowercase();
        assert!(
            field.contains("pass") && field.contains("fail") && field.contains("needed"),
            "the tester {case} got an `outcome` line, but it does not name both allowed values \
             and mark the field as needed: {fields:?}"
        );

        let field_at = tester
            .find(fields[0])
            .ok_or("the outcome field disappeared from the prompt while it was inspected")?;
        let fallback_at = tester
            .find("End your answer with a line of its own")
            .ok_or("the existing outcome-line fallback disappeared from the tester prompt")?;
        assert!(
            field_at < fallback_at,
            "the automatic field has to be part of the fields agreement, with the old prose \
             line after it as a fallback. The prompt was: {tester:?}"
        );

        for ordinary in ["work", "after"] {
            let prompt = observed.only_prompt(ordinary)?;
            assert!(
                !prompt
                    .lines()
                    .map(str::trim)
                    .any(|line| line.starts_with("outcome:"))
                    && !prompt.contains("End your answer with a line of its own"),
                "the ordinary step {ordinary:?} was asked for a loop result nobody reads. \
                 Its prompt was: {prompt:?}"
            );
        }
    }

    let tester_with_form = with.only_prompt("tester")?;
    assert!(
        tester_with_form
            .lines()
            .map(str::trim)
            .any(|line| line.starts_with("notes:")),
        "adding the automatic outcome field replaced the form the person configured: \
         {tester_with_form:?}"
    );
    Ok(())
}

// ── wspólna ławka czterech kryteriów T-100 ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub(crate) struct Call {
    pub(crate) label: String,
    pub(crate) attempt: usize,
    pub(crate) prompt: String,
}

#[derive(Debug, Default)]
pub(crate) struct Script {
    answers: Mutex<BTreeMap<String, Vec<String>>>,
    calls: Mutex<Vec<Call>>,
}

impl Script {
    pub(crate) fn new(entries: &[(&str, &[&str])]) -> Self {
        let answers = entries
            .iter()
            .map(|(label, answers)| {
                (
                    (*label).to_owned(),
                    answers.iter().map(|answer| (*answer).to_owned()).collect(),
                )
            })
            .collect();
        Self {
            answers: Mutex::new(answers),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn entered(&self, prompt: &str) -> String {
        let label = label_of(prompt);
        let attempt = {
            let mut calls = self.calls();
            let attempt = calls.iter().filter(|call| call.label == label).count() + 1;
            calls.push(Call {
                label: label.clone(),
                attempt,
                prompt: prompt.to_owned(),
            });
            attempt
        };
        self.answers()
            .get(&label)
            .and_then(|answers| answers.get(attempt - 1).or_else(|| answers.last()))
            .cloned()
            .unwrap_or_else(|| ordinary_answer(&label, attempt))
    }

    fn snapshot(&self) -> Vec<Call> {
        self.calls().clone()
    }

    fn answers(&self) -> MutexGuard<'_, BTreeMap<String, Vec<String>>> {
        self.answers.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn calls(&self) -> MutexGuard<'_, Vec<Call>> {
        self.calls.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Debug)]
pub(crate) struct ObservedRun {
    pub(crate) report: RunReport,
    pub(crate) calls: Vec<Call>,
    _home: TempDir,
    project: TempDir,
}

impl ObservedRun {
    pub(crate) fn calls_for(&self, label: &str) -> Vec<&Call> {
        self.calls
            .iter()
            .filter(|call| call.label == label)
            .collect()
    }

    pub(crate) fn only_prompt(&self, label: &str) -> Result<&str, Box<dyn Error>> {
        let calls = self.calls_for(label);
        if calls.len() != 1 {
            return Err(format!(
                "expected exactly one call for {label:?}, got attempts {:?}",
                calls
                    .iter()
                    .map(|call| call.attempt)
                    .collect::<Vec<usize>>()
            )
            .into());
        }
        Ok(&calls[0].prompt)
    }

    pub(crate) fn run_json(&self) -> Result<Json, Box<dyn Error>> {
        let text = fs::read_to_string(self.report.dir.join("run.json"))?;
        Ok(serde_json::from_str(&text)?)
    }

    pub(crate) fn project(&self) -> &Path {
        self.project.path()
    }
}

pub(crate) async fn run_fixture(
    slug: &str,
    workflow_text: &str,
    script: Script,
) -> Result<ObservedRun, Box<dyn Error>> {
    let home = TempDir::new()?;
    let project = TempDir::new()?;
    fs::create_dir_all(home.path().join("agents"))?;
    fs::create_dir_all(home.path().join("workflows"))?;
    fs::create_dir_all(project.path().join(".loadout"))?;
    fs::write(project.path().join("notes.txt"), "written by the human")?;

    let agent = home.path().join("agents/hand.md");
    fs::write(&agent, HAND_FILE)?;
    read_agent_file(&agent).map_err(|error| format!("{}: {error}", agent.display()))?;

    let workflow = home.path().join("workflows").join(format!("{slug}.json"));
    fs::write(&workflow, workflow_text)?;
    let problems: Vec<String> = check(&load(&workflow)?)
        .into_iter()
        .filter(|note| note.level == Level::Problem)
        .map(|note| note.message)
        .collect();
    assert!(
        problems.is_empty(),
        "the fixture would be refused before an agent ran, so it cannot prove behavior: \
         {problems:?}"
    );

    let script = Arc::new(script);
    let store = Store::open(&project.path().join(".loadout/loadout.db"))?;
    let deps = RunDeps {
        home: home.path(),
        project: project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&script)),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))??;
    let _ = tokio::time::timeout(PATIENCE, pump).await;
    let calls = script.snapshot();
    Ok(ObservedRun {
        report,
        calls,
        _home: home,
        project,
    })
}

fn label_of(prompt: &str) -> String {
    prompt
        .split_once(':')
        .map_or_else(|| prompt.to_owned(), |(head, _)| head.trim().to_owned())
}

fn ordinary_answer(label: &str, attempt: usize) -> String {
    format!(
        "## Answer\n{label} try {attempt} is done.\n\n## Evidence\nnotes.txt:1\n\n## Open\n\
         nothing.\n"
    )
}

fn fake_drivers(script: Arc<Script>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { script });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct Fake {
    script: Arc<Script>,
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        VENDOR
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some(VENDOR.to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let said = self.script.entered(&spec.prompt);
        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };
        let _ = events
            .send(
                (AgentEvent::Started {
                    session: session.clone(),
                    model: spec.model.clone().unwrap_or_default(),
                    tools: Vec::new(),
                    capabilities: Vec::new(),
                })
                .into(),
            )
            .await;
        Ok(Box::new(Turn {
            events,
            session,
            said,
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    said: String,
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
            text: self.said.clone(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::ZERO,
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send((AgentEvent::Finished(outcome.clone())).into())
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
