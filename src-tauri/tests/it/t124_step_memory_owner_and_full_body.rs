//! T-124 AC-1: ukończony krok zachowuje właściciela i cały źródłowy Markdown.

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::memory::notes_root;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, StepSettings, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::memory::FrontMatter;
use loadout_lib::memory::notes::{Note, Scope, Status, scan_notes};
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

const PATIENCE: Duration = Duration::from_secs(20);
const VENDOR: &str = "fake";
const STEP_NAME: &str = "Remember the queue";
const AGENT_NAME: &str = "Backend Keeper";
const FIRST_MARKER: &str = "IBEX-T124-QUEUE";
const SECOND_MARKER: &str = "IBEX-T124-RETRY";
const INDEX_MARKER: &str = "IBEX-T124-INDEX";
const FIRST_RULE: &str = "IBEX-T124-QUEUE ownership stays with the scheduler, including every retry in that same transition.";
const SECOND_RULE: &str = "IBEX-T124-RETRY resumes from the last durable checkpoint.";
const WHY: &str = "The queue owner sees one exact transition.";

const FIRST_MARKDOWN: &str = "# Queue policy\n\nIBEX-T124-QUEUE ownership stays with the scheduler,\nincluding every retry in that same transition.\n\nKeep the acknowledgement after the transition.\n\n**Why:** The queue owner sees one exact transition.\n\nFinal checklist.\n";
const SECOND_MARKDOWN: &str = "# Retry policy\n\nIBEX-T124-RETRY resumes from the last durable checkpoint.\n\nKeep the original evidence attached.\n";

const AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-000000000124
name: Backend Keeper
summary: Keeps durable queue state
color: slate
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
Keep the queue durable.
";

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_t124_step_memory",
  "name": "One step remembers full markdown",
  "steps": [{
    "kind": "agent",
    "id": "s_memory",
    "name": "Remember the queue",
    "agent": "01990000-0000-7000-8000-000000000124",
    "overrides": {},
    "instructions": "Inspect the queue.",
    "folder": { "use": "project" },
    "at": { "x": 0, "y": 0 }
  }],
  "links": []
}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn completed_step_keeps_owner_reason_and_full_markdown() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let report = run_one_step(&bench).await?;
    assert_eq!(report.steps, vec![StepState::Succeeded]);

    let notes = scan_notes(&notes_root(bench.home.path()))?;
    assert_eq!(
        notes.len(),
        2,
        "MEMORY.md must not become a third candidate"
    );
    let first = note_with(&notes, FIRST_MARKER)?;
    let second = note_with(&notes, SECOND_MARKER)?;

    assert_candidate(first, FIRST_RULE);
    assert_candidate(second, SECOND_RULE);
    assert_eq!(first.because, WHY, "an explicit Why must win verbatim");
    assert_eq!(
        second.because,
        format!(
            "{AGENT_NAME} left this in its own notes while working on \"{STEP_NAME}\" in run {}.",
            report.id
        ),
        "a note without Why must name its agent, step, and run"
    );
    assert_eq!(body_of(first)?, FIRST_MARKDOWN);
    assert_eq!(body_of(second)?, SECOND_MARKDOWN);
    assert!(!first.rule.contains(INDEX_MARKER));
    assert!(!second.rule.contains(INDEX_MARKER));
    Ok(())
}

fn assert_candidate(note: &Note, rule: &str) {
    assert_eq!(
        note.rule, rule,
        "the rule must be the complete first paragraph"
    );
    assert_eq!(note.status, Status::Suggested);
    assert_eq!(note.scope, Scope::ThisAgent);
    assert_eq!(note.agent.as_deref(), Some(AGENT_NAME));
}

fn note_with<'a>(notes: &'a [Note], marker: &str) -> Result<&'a Note, Box<dyn Error>> {
    notes
        .iter()
        .find(|note| note.rule.contains(marker))
        .ok_or_else(|| format!("no candidate contains {marker}").into())
}

fn body_of(note: &Note) -> Result<String, Box<dyn Error>> {
    let raw = fs::read_to_string(&note.path)?;
    let (_, body_at) = FrontMatter::split(&raw)?;
    Ok(raw[body_at..].to_owned())
}

async fn run_one_step(bench: &Bench) -> Result<RunReport, Box<dyn Error>> {
    let workflow = bench.workflow();
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(),
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
        .map_err(|_| format!("the run did not finish within {PATIENCE:?}"))??;
    let _ = tokio::time::timeout(PATIENCE, pump).await;
    Ok(report)
}

fn fake_drivers() -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { memory: None });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct Fake {
    memory: Option<PathBuf>,
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

    fn with_settings(
        &self,
        settings: &StepSettings,
    ) -> Option<anyhow::Result<Arc<dyn AgentDriver>>> {
        Some(Ok(Arc::new(Self {
            memory: Some(settings.memory.clone()),
        })))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let dir = self
            .memory
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("the run did not provide a memory directory"))?;
        fs::create_dir_all(dir)?;
        fs::write(dir.join("queue.md"), FIRST_MARKDOWN)?;
        fs::write(dir.join("retry.md"), SECOND_MARKDOWN)?;
        fs::write(
            dir.join("MEMORY.md"),
            format!("# Index\n\n{INDEX_MARKER}\n"),
        )?;
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
        Ok(Box::new(Turn { events, session }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<loadout_lib::engine::supervisor::GroupId> {
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "Queue inspected.".to_owned(),
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

    async fn cancel(&mut self) -> loadout_lib::engine::supervisor::GroupProof {
        loadout_lib::engine::supervisor::GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

struct Bench {
    home: TempDir,
    project: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(home.path().join("memory").join("notes"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(home.path().join("agents").join("backend.md"), AGENT)?;
        fs::write(home.path().join("workflows").join("t124.json"), WORKFLOW)?;
        Ok(Self { home, project })
    }

    fn workflow(&self) -> PathBuf {
        self.home.path().join("workflows").join("t124.json")
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }
}
