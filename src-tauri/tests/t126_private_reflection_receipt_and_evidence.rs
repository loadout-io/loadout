//! T-126 AC-1: reflection has its own hard wrappers, physical evidence and durable receipt.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::memory::project_notes_root;
use loadout_lib::commands::run::run_workflow_with_reflection;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, StepSettings, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::{EvidenceIdentity, EvidenceStreams, EvidenceTarget};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::Vendor;
use loadout_lib::memory::notes::scan_notes;
use loadout_lib::store::Store;
use serde_json::Value;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;
use uuid::Uuid;

const PATIENCE: Duration = Duration::from_secs(20);
const QUIET_AFTER_RUN: Duration = Duration::from_millis(350);
const REFLECTION_COST: f64 = 0.037;
const SENTINEL: &str = "reflection evidence must stay inside its run";
const GOOD_RULE: &str = "T126-GROUNDED keep the queue owner in one place";

const AGENT: &str = r"---
schema: 1
id: 01990000-0000-7000-8000-000000000126
name: T126 Builder
summary: Leaves one useful result
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
  "id": "wf_t126_private_reflection",
  "name": "Private reflection fixture",
  "steps": [{
    "kind": "agent", "id": "build", "name": "Build", "agent":
    "01990000-0000-7000-8000-000000000126", "overrides": {},
    "instructions": "Return the fixture result.", "folder": { "use": "project" },
    "at": { "x": 0, "y": 0 }
  }],
  "links": []
}"#;

const STEP_ANSWER: &str = "## Answer\nQueue fixed.\n\n## Evidence\nqueue.rs:7\n\n## Open\nNone.\n";
const REFLECTION_ANSWER: &str = "rule: T126-GROUNDED keep the queue owner in one place\n\
because: run.json and the Build handoff show one owner\n\n\
rule: T126-UNGROUNDED never retry the queue\n";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn private_reflection_keeps_exact_evidence_receipt_and_history() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let seen = Arc::new(Seen::default());
    let report = run_fixture(&bench, Arc::clone(&seen), None, true).await?;
    tokio::time::sleep(QUIET_AFTER_RUN).await;

    assert_eq!(report.steps, vec![StepState::Succeeded]);
    assert_eq!(seen.step_starts.load(Ordering::Acquire), 1);
    assert_eq!(
        seen.reflection_starts.load(Ordering::Acquire),
        1,
        "reflection must still be exactly once after the delayed quiet window"
    );
    let mut run = read_json(&report.dir.join("run.json"))?;
    assert_receipt(&run)?;
    assert_exact_evidence(&report, &run)?;
    assert_notes(&bench)?;
    assert_eq!(fs::read_to_string(&bench.sentinel)?, SENTINEL);
    assert_old_history(&bench, &report, &mut run)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn each_missing_hard_wrapper_refuses_before_reflection_start() -> Result<(), Box<dyn Error>> {
    for missing in [Wrapper::Settings, Wrapper::Evidence, Wrapper::Budget] {
        let bench = Bench::new()?;
        let seen = Arc::new(Seen::default());
        let report = run_fixture(&bench, Arc::clone(&seen), Some(missing), true).await?;
        assert_eq!(report.steps, vec![StepState::Succeeded]);
        assert_eq!(
            seen.reflection_starts.load(Ordering::Acquire),
            0,
            "missing {missing:?} reached AgentDriver::start instead of refusing before spawn"
        );
        let run = read_json(&report.dir.join("run.json"))?;
        assert_eq!(run.pointer("/reflection/ran"), Some(&Value::Bool(false)));
        assert!(scan_notes(&bench.home.path().join("memory"))?.is_empty());
        assert!(scan_notes(&project_notes_root(bench.project.path()))?.is_empty());
    }
    Ok(())
}

fn assert_receipt(run: &Value) -> Result<(), Box<dyn Error>> {
    assert_eq!(run.pointer("/reflection/ran"), Some(&Value::Bool(true)));
    assert_eq!(
        run.pointer("/reflection/kept").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        run.pointer("/reflection/dropped_without_reason")
            .and_then(Value::as_u64),
        Some(1)
    );
    let cost = run
        .pointer("/reflection/cost_usd")
        .and_then(Value::as_f64)
        .ok_or("reflection receipt has no measured cost")?;
    assert!((cost - REFLECTION_COST).abs() < 0.000_001);
    Ok(())
}

fn assert_exact_evidence(report: &RunReport, run: &Value) -> Result<(), Box<dyn Error>> {
    let step = run
        .pointer("/steps/0")
        .and_then(Value::as_object)
        .ok_or("run.json has no physical step row")?;
    let id = step
        .get("id")
        .and_then(Value::as_str)
        .ok_or("step has no id")?;
    let _ = Uuid::parse_str(id)?;
    assert_eq!(step.get("node_key").and_then(Value::as_str), Some("build"));
    assert_ne!(
        id, "build",
        "logical key must not masquerade as the physical UUID"
    );
    let mut actual = file_names(&report.dir.join("logs"))?;
    let mut wanted = vec![
        format!("agent-{id}.input.json"),
        format!("agent-{id}.jsonl"),
        format!("agent-{id}.stderr.log"),
        "reflection.input.json".to_owned(),
        "reflection.jsonl".to_owned(),
        "reflection.stderr.log".to_owned(),
    ];
    actual.sort();
    wanted.sort();
    assert_eq!(
        actual, wanted,
        "logs/ contains a missing or extra reflection artifact"
    );
    Ok(())
}

fn assert_notes(bench: &Bench) -> Result<(), Box<dyn Error>> {
    let memory = project_notes_root(bench.project.path());
    let notes = scan_notes(&memory)?;
    assert_eq!(
        notes.len(),
        1,
        "one grounded and one ungrounded rule must keep exactly one"
    );
    assert_eq!(notes[0].rule, GOOD_RULE);
    assert!(!memory.join("notes").join("notes").exists());
    assert!(scan_notes(&bench.home.path().join("memory"))?.is_empty());
    Ok(())
}

fn assert_old_history(
    bench: &Bench,
    report: &RunReport,
    run: &mut Value,
) -> Result<(), Box<dyn Error>> {
    let id = run
        .get("id")
        .and_then(Value::as_str)
        .ok_or("run has no id")?
        .to_owned();
    let step_id = run
        .pointer("/steps/0/id")
        .and_then(Value::as_str)
        .ok_or("run has no physical step id")?
        .to_owned();
    run.as_object_mut()
        .ok_or("run.json is not an object")?
        .remove("reflection");
    fs::write(report.dir.join("run.json"), serde_json::to_vec_pretty(run)?)?;
    let folder = report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("run directory has no plain name")?;
    let past = read_run_inner(bench.project.path(), folder)?;
    assert_eq!(past.folder, folder);
    assert!(
        past.folder.ends_with(&id),
        "the public history address lost the run id"
    );
    assert_eq!(past.state, "succeeded");
    assert_eq!(past.steps.len(), 1);
    assert_eq!(
        past.steps[0].id, step_id,
        "the old run lost its complete step row"
    );
    Ok(())
}

async fn run_fixture(
    bench: &Bench,
    seen: Arc<Seen>,
    missing: Option<Wrapper>,
    reflection_enabled: bool,
) -> Result<RunReport, Box<dyn Error>> {
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(seen, missing),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: bench.workflow(),
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let report = tokio::time::timeout(
        PATIENCE,
        run_workflow_with_reflection(&deps, &request, sink, None, reflection_enabled),
    )
    .await??;
    tokio::time::timeout(PATIENCE, pump).await??;
    Ok(report)
}

fn fake_drivers(seen: Arc<Seen>, missing: Option<Wrapper>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake::step(seen, missing));
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

#[derive(Default)]
struct Seen {
    step_starts: AtomicUsize,
    reflection_starts: AtomicUsize,
}

#[derive(Clone)]
struct Fake {
    mode: Mode,
    seen: Arc<Seen>,
    missing: Option<Wrapper>,
    settings: Option<StepSettings>,
    evidence: Option<EvidenceTarget>,
    budget: Option<f64>,
}

impl Fake {
    fn step(seen: Arc<Seen>, missing: Option<Wrapper>) -> Self {
        Self {
            mode: Mode::Step,
            seen,
            missing,
            settings: None,
            evidence: None,
            budget: None,
        }
    }

    fn clone_with(&self) -> Self {
        Self {
            mode: self.mode,
            seen: Arc::clone(&self.seen),
            missing: self.missing,
            settings: self.settings.clone(),
            evidence: self.evidence.clone(),
            budget: self.budget,
        }
    }
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        "fake"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("t126".to_owned()),
        })
    }

    fn reflecting(&self) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            mode: Mode::Reflection,
            seen: Arc::clone(&self.seen),
            missing: self.missing,
            settings: None,
            evidence: None,
            budget: None,
        }))
    }

    fn with_settings(
        &self,
        settings: &StepSettings,
    ) -> Option<anyhow::Result<Arc<dyn AgentDriver>>> {
        if self.mode == Mode::Reflection && self.missing == Some(Wrapper::Settings) {
            return Some(Err(anyhow::anyhow!("reflection settings fixture failed")));
        }
        let mut clone = self.clone_with();
        clone.settings = Some(settings.clone());
        Some(Ok(Arc::new(clone)))
    }

    fn with_evidence(&self, target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        if self.mode == Mode::Reflection && self.missing == Some(Wrapper::Evidence) {
            return None;
        }
        let mut clone = self.clone_with();
        clone.evidence = Some(target);
        Some(Arc::new(clone))
    }

    fn with_budget(&self, dollars: f64) -> Option<Arc<dyn AgentDriver>> {
        if self.mode == Mode::Reflection && self.missing == Some(Wrapper::Budget) {
            return None;
        }
        let mut clone = self.clone_with();
        clone.budget = Some(dollars);
        Some(Arc::new(clone))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        match self.mode {
            Mode::Step => self.seen.step_starts.fetch_add(1, Ordering::AcqRel),
            Mode::Reflection => self.seen.reflection_starts.fetch_add(1, Ordering::AcqRel),
        };
        self.validate(&spec)?;
        self.write_evidence().await?;
        let session = SessionRef {
            vendor: "fake",
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
        let (text, cost) = match self.mode {
            Mode::Step => (STEP_ANSWER.to_owned(), Some(0.01)),
            Mode::Reflection => (REFLECTION_ANSWER.to_owned(), Some(REFLECTION_COST)),
        };
        Ok(Box::new(Turn {
            events,
            session,
            text,
            cost,
        }))
    }
}

impl Fake {
    fn validate(&self, spec: &RunSpec) -> anyhow::Result<()> {
        let target = self
            .evidence
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no evidence"))?;
        if self.mode == Mode::Step {
            return Ok(());
        }
        let settings = self
            .settings
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no settings"))?;
        if settings.dir != spec.cwd || settings.memory != spec.cwd.join("mem").join("_reflection") {
            return Err(anyhow::anyhow!(
                "reflection does not own <run>/mem/_reflection"
            ));
        }
        if !matches!(target.identity(), EvidenceIdentity::Reflection) || target.root() != spec.cwd {
            return Err(anyhow::anyhow!(
                "reflection does not own its evidence target"
            ));
        }
        if !self.budget.is_some_and(|budget| budget > 0.0) {
            return Err(anyhow::anyhow!("reflection has no positive budget"));
        }
        Ok(())
    }

    async fn write_evidence(&self) -> anyhow::Result<()> {
        let target = self
            .evidence
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no evidence"))?;
        let EvidenceStreams {
            mut stdout,
            mut stderr,
        } = target.open().await?;
        stdout.write(b"{\"type\":\"fixture\"}\n").await?;
        stderr.write(b"fixture stderr\n").await?;
        stdout.close().await?;
        stderr.close().await?;
        Ok(())
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

struct Bench {
    home: TempDir,
    project: TempDir,
    sentinel: PathBuf,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(home.path().join("memory").join("notes"))?;
        fs::create_dir_all(project_notes_root(project.path()).join("notes"))?;
        fs::write(home.path().join("agents").join("builder.md"), AGENT)?;
        fs::write(home.path().join("workflows").join("t126.json"), WORKFLOW)?;
        let sentinel = project.path().join("reflection-sentinel.txt");
        fs::write(&sentinel, SENTINEL)?;
        Ok(Self {
            home,
            project,
            sentinel,
        })
    }

    fn workflow(&self) -> PathBuf {
        self.home.path().join("workflows").join("t126.json")
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }
}

fn read_json(path: &Path) -> Result<Value, Box<dyn Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn file_names(dir: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    fs::read_dir(dir)?
        .map(|entry| {
            let name = entry?.file_name();
            name.into_string()
                .map_err(|_| std::io::Error::other("a log file has no UTF-8 name"))
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}
