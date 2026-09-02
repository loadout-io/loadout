//! T-133 AC-1: the third reflection candidate reaches its independent IO failure.
//!
//! The directory at the third candidate's target is only the cause of the refusal. The
//! production warning is the observation that the real workflow actually attempted that write.

use std::error::Error;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::memory::project_notes_root;
use loadout_lib::commands::run::run_workflow_with_reflection;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, StepSettings, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::{EvidenceStreams, EvidenceTarget};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::Vendor;
use loadout_lib::store::Store;
use serde_json::Value;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

const PATIENCE: Duration = Duration::from_secs(20);
const WARNING: &str = "this run had something to remember and it could not be written down";
const KEPT_ID: &str = "t133-kept-reflection";
const DISCARDED_ID: &str = "t133-discarded-again";
const IO_ID: &str = "t133-independent-io-failure";

const AGENT: &str = r"---
schema: 1
id: 019b0133-0000-7000-8000-000000000133
name: T133 Builder
summary: Leaves one result before private reflection
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
Return the fixture result.
";

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_t133_reflection_io_attempt",
  "name": "Reflection IO attempt fixture",
  "steps": [{
    "kind": "agent",
    "id": "build",
    "name": "Build",
    "agent": "019b0133-0000-7000-8000-000000000133",
    "overrides": {},
    "instructions": "Return the fixture result.",
    "folder": { "use": "project" },
    "at": { "x": 0, "y": 0 }
  }],
  "links": []
}"#;

const STEP_ANSWER: &str =
    "## Answer\nFixture finished.\n\n## Evidence\nfixture\n\n## Open\nNone.\n";
const REFLECTION_ANSWER: &str = "rule: T133 kept reflection\n\
because: this candidate proves a successful automatic write\n\n\
rule: T133 discarded again\n\
because: this exact candidate has a tombstone\n\n\
rule: T133 independent IO failure\n\
because: this candidate must reach a separate filesystem refusal\n";

#[tokio::test(flavor = "current_thread")]
async fn completed_agent_reaches_the_private_reflection_seam() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let seen = Arc::new(Seen::default());
    let report = run_fixture(&bench, Arc::clone(&seen)).await?;

    assert_eq!(report.steps, vec![StepState::Succeeded]);
    assert_eq!(seen.step_starts.load(Ordering::Acquire), 1);
    assert_eq!(
        seen.reflection_starts.load(Ordering::Acquire),
        1,
        "the successful agent step must lead to exactly one private reflection turn"
    );
    let run = read_json(&report.dir.join("run.json"))?;
    assert_eq!(run.pointer("/reflection/ran"), Some(&Value::Bool(true)));
    assert_eq!(
        run.pointer("/reflection/kept").and_then(Value::as_u64),
        Some(1),
        "the first candidate proves that the real reflection writer ran"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn third_candidate_reaches_the_independent_io_warning() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let seen = Arc::new(Seen::default());
    let captured = Arc::new(std::sync::Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_target(false)
        .with_writer({
            let captured = Arc::clone(&captured);
            move || Scribe(Arc::clone(&captured))
        })
        .finish();

    // A thread-local guard covers every poll because this test deliberately uses current_thread.
    let guard = tracing::subscriber::set_default(subscriber);
    let report = run_fixture(&bench, Arc::clone(&seen)).await?;
    drop(guard);

    assert_eq!(report.steps, vec![StepState::Succeeded]);
    assert_eq!(seen.step_starts.load(Ordering::Acquire), 1);
    assert_eq!(
        seen.reflection_starts.load(Ordering::Acquire),
        1,
        "the successful agent step must lead to exactly one private reflection turn"
    );

    let run = read_json(&report.dir.join("run.json"))?;
    assert_eq!(run.pointer("/reflection/ran"), Some(&Value::Bool(true)));
    assert_eq!(
        run.pointer("/reflection/kept").and_then(Value::as_u64),
        Some(1),
        "only the first candidate is a successful write"
    );
    assert_eq!(
        run.pointer("/reflection/discardedAgain")
            .and_then(Value::as_u64),
        Some(1),
        "only the typed tombstone refusal belongs in discardedAgain"
    );
    assert_eq!(
        run.pointer("/reflection/dropped_without_reason")
            .and_then(Value::as_u64),
        Some(0),
        "all three reflection pairs have an explicit reason"
    );

    let notes = project_notes_root(bench.project.path()).join("notes");
    assert!(
        notes.join(format!("{KEPT_ID}.md")).is_file(),
        "the control candidate was not kept"
    );
    assert!(
        !notes.join(format!("{DISCARDED_ID}.md")).exists(),
        "the tombstoned candidate was written again"
    );
    let io_target = notes.join(format!("{IO_ID}.md"));
    assert!(
        !io_target.is_file(),
        "the third candidate unexpectedly became a file"
    );
    assert!(
        io_target.is_dir(),
        "the directory that caused the independent IO refusal was not preserved"
    );

    let logs = captured_text(&captured)?;
    let warnings: Vec<&str> = logs.lines().filter(|line| line.contains(WARNING)).collect();
    assert_eq!(
        warnings.len(),
        1,
        "the real write path must warn once for the independent IO failure and never for the \
         typed tombstone refusal; captured logs:\n{logs}"
    );
    let warning = warnings[0];
    assert!(
        warning.contains(&report.id),
        "the warning is not tied to the run that attempted the write: {warning}"
    );
    assert!(
        warning.contains("Is a directory") || warning.contains("os error"),
        "the warning does not carry the independent filesystem error: {warning}"
    );
    assert!(
        !logs.contains("PreviouslyDiscarded") && !logs.contains("was discarded before"),
        "the typed tombstone refusal leaked into the warning stream:\n{logs}"
    );
    Ok(())
}

async fn run_fixture(bench: &Bench, seen: Arc<Seen>) -> Result<RunReport, Box<dyn Error>> {
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(seen),
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
        run_workflow_with_reflection(&deps, &request, sink, None, true),
    )
    .await??;
    tokio::time::timeout(PATIENCE, pump).await??;
    Ok(report)
}

fn fake_drivers(seen: Arc<Seen>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake::step(seen));
    Arc::new(move |_vendor: Vendor| Arc::clone(&driver))
}

#[derive(Clone, Copy, PartialEq, Eq)]
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
    evidence: Option<EvidenceTarget>,
}

impl Fake {
    fn step(seen: Arc<Seen>) -> Self {
        Self {
            mode: Mode::Step,
            seen,
            evidence: None,
        }
    }

    async fn write_evidence(&self) -> anyhow::Result<()> {
        let target = self
            .evidence
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("the fake was started without its evidence target"))?;
        let EvidenceStreams {
            mut stdout,
            mut stderr,
        } = target.open().await?;
        stdout.write(b"{\"type\":\"t133-fixture\"}\n").await?;
        stderr.write(b"t133 fixture stderr\n").await?;
        stdout.close().await?;
        stderr.close().await?;
        Ok(())
    }
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        "t133-fake"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("t133".to_owned()),
        })
    }

    fn reflecting(&self) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            mode: Mode::Reflection,
            seen: Arc::clone(&self.seen),
            evidence: None,
        }))
    }

    fn with_settings(
        &self,
        _settings: &StepSettings,
    ) -> Option<anyhow::Result<Arc<dyn AgentDriver>>> {
        Some(Ok(Arc::new(self.clone())))
    }

    fn with_evidence(&self, target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        let mut clone = self.clone();
        clone.evidence = Some(target);
        Some(Arc::new(clone))
    }

    fn with_budget(&self, _dollars: f64) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
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
        self.write_evidence().await?;
        let session = SessionRef {
            vendor: "t133-fake",
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
        let text = match self.mode {
            Mode::Step => STEP_ANSWER,
            Mode::Reflection => REFLECTION_ANSWER,
        };
        Ok(Box::new(Turn {
            events,
            session,
            text: text.to_owned(),
        }))
    }
}

struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    text: String,
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
            cost_usd: None,
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
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(home.path().join("memory/notes"))?;
        fs::write(home.path().join("agents/t133-builder.md"), AGENT)?;
        fs::write(home.path().join("workflows/t133.json"), WORKFLOW)?;

        let project_memory = project_notes_root(project.path());
        fs::create_dir_all(project_memory.join("notes"))?;
        fs::create_dir_all(project_memory.join("discarded"))?;
        fs::write(
            project_memory
                .join("discarded")
                .join(format!("{DISCARDED_ID}__20260827T120000Z.md")),
            "tombstone",
        )?;
        // A pre-existing directory makes the third candidate fail only if the real writer tries it.
        fs::create_dir_all(project_memory.join("notes").join(format!("{IO_ID}.md")))?;

        Ok(Self { home, project })
    }

    fn workflow(&self) -> PathBuf {
        self.home.path().join("workflows/t133.json")
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout/loadout.db")
    }
}

#[derive(Clone)]
struct Scribe(Arc<std::sync::Mutex<Vec<u8>>>);

impl Write for Scribe {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut captured = self
            .0
            .lock()
            .map_err(|_| io::Error::other("the tracing capture was poisoned"))?;
        captured.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn captured_text(captured: &std::sync::Mutex<Vec<u8>>) -> Result<String, Box<dyn Error>> {
    let bytes = captured
        .lock()
        .map_err(|_| io::Error::other("the tracing capture was poisoned"))?
        .clone();
    Ok(String::from_utf8(bytes)?)
}

fn read_json(path: &Path) -> Result<Value, Box<dyn Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
