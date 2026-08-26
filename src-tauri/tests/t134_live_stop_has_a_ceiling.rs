//! T-134 AC-1: live Stop has a finite, honest ceiling and releases the next Start.

use std::error::Error;
use std::fs;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::run::{run_workflow_with_reflection, stop_run_inner};
use loadout_lib::commands::{Drivers, Outcome, RunError, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{AppState, QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use serde_json::Value;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::{Notify, mpsc};

/// Wyższy niż trzy produkcyjne próby z odstępami, ale skończony wewnątrz testu: obecna pętla
/// ma paść na asercji, nie na zewnętrznym rc 124 bramki.
const PATIENCE: Duration = Duration::from_secs(15);
const STUBBORN_PID: i32 = 813_579;
const STUBBORN_PGID: i32 = 813_580;
const CONTROL_PID: i32 = 824_680;
const CONTROL_PGID: i32 = 824_681;
const SURVIVOR_ERROR: &str =
    "This agent survived Loadout's three attempts to stop it and may still be running.";

const AGENT: &str = r#"---
schema: 1
id: 01990000-0000-7000-8000-000000000134
name: T134 Builder
summary: Exercises live Stop
color: slate
runsWith: claude-code
model: haiku
thinking: balanced
fileAccess: look-only
giveUpAfterMinutes: 20
writeResultsTo: ""
tools: everything
skills: []
connections: []
---
Exercise live Stop.
"#;

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_t134_stop",
  "name": "T134 live Stop",
  "steps": [{
    "kind": "agent",
    "id": "build",
    "name": "Build",
    "agent": "01990000-0000-7000-8000-000000000134",
    "overrides": {},
    "instructions": "Wait for Stop.",
    "folder": { "use": "project" },
    "at": { "x": 0, "y": 0 }
  }],
  "links": []
}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stubborn_agent_stops_after_three_attempts_and_releases_the_next_start()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let fake = Arc::new(Fake::stubborn_then_successful());
    let app = bench.app(fake_drivers(Arc::clone(&fake)))?;
    let deps = app
        .begin_run(bench.project.path())
        .map_err(std::io::Error::other)?;
    let (lines, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let request = bench.request();
    let run = run_workflow_with_reflection(&deps, &request, lines, None, false);
    tokio::pin!(run);

    wait_until_the_handle_is_live(&fake, &mut run).await?;
    let finished = tokio::time::timeout(PATIENCE, async {
        tokio::join!(stop_run_inner(&deps), &mut run)
    })
    .await;
    assert!(
        finished.is_ok(),
        "Stop and its run did not come back within {PATIENCE:?}. The fake agent has no system \
         process and returns GroupProof::Alive immediately, so this is the missing retry \
         ceiling, not a slow escalation (cancel calls: {})",
        fake.cancel_calls.load(Ordering::Acquire)
    );
    let (stopped, ran) = finished.expect("the timeout assertion above checked this result");
    let stopped = stopped?;
    let report = ran?;
    tokio::time::timeout(PATIENCE, pump).await??;

    assert_eq!(stopped, Outcome::Cancelled);
    assert_eq!(report.outcome, Outcome::Cancelled);
    assert_eq!(report.steps, vec![StepState::Failed]);
    assert_eq!(
        fake.cancel_calls.load(Ordering::Acquire),
        3,
        "the retry ceiling is a production policy: the test passes no attempt count"
    );
    assert_stubborn_receipt(&bench, &report)?;

    // Ten sam AppState jest częścią kryterium. Nowy, niezależny stan ominąłby zapadkę, której
    // zablokowanie po nieudowodnionym Stopie jest wadą T-134.
    let next = app
        .begin_run(bench.project.path())
        .map_err(std::io::Error::other)?;
    let (next_lines, next_source) = line_channel(QUEUE_CAP);
    let next_pump = spawn_pump(next_source, Channel::new(|_| Ok(())));
    let next_request = bench.request();
    let next_report = tokio::time::timeout(
        PATIENCE,
        run_workflow_with_reflection(&next, &next_request, next_lines, None, false),
    )
    .await??;
    tokio::time::timeout(PATIENCE, next_pump).await??;

    assert_eq!(next_report.outcome, Outcome::Done);
    assert_eq!(next_report.steps, vec![StepState::Succeeded]);
    assert_eq!(fake.starts.load(Ordering::Acquire), 2);
    assert_eq!(fake.successful_waits.load(Ordering::Acquire), 1);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn first_dead_proof_keeps_the_existing_honest_stop_path() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let fake = Arc::new(Fake::dead_on_first_cancel());
    let app = bench.app(fake_drivers(Arc::clone(&fake)))?;
    let deps = app
        .begin_run(bench.project.path())
        .map_err(std::io::Error::other)?;
    let (lines, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let request = bench.request();
    let run = run_workflow_with_reflection(&deps, &request, lines, None, false);
    tokio::pin!(run);

    wait_until_the_handle_is_live(&fake, &mut run).await?;
    let (stopped, ran) = tokio::time::timeout(PATIENCE, async {
        tokio::join!(stop_run_inner(&deps), &mut run)
    })
    .await?;
    let stopped = stopped?;
    let report = ran?;
    tokio::time::timeout(PATIENCE, pump).await??;

    assert_eq!(stopped, Outcome::Cancelled);
    assert_eq!(report.outcome, Outcome::Cancelled);
    assert_eq!(report.steps, vec![StepState::Cancelled]);
    assert_eq!(fake.cancel_calls.load(Ordering::Acquire), 1);

    let run = read_run_json(&report)?;
    let step = one_step(&run)?;
    assert_eq!(
        step.get("pid").and_then(Value::as_i64),
        Some(i64::from(CONTROL_PID))
    );
    assert_eq!(
        step.get("pgid").and_then(Value::as_i64),
        Some(i64::from(CONTROL_PGID))
    );
    assert_eq!(step.get("death_proof").and_then(Value::as_bool), Some(true));
    assert!(step.get("error").is_none_or(Value::is_null));

    let past = read_run_inner(bench.project.path(), run_folder(&report)?)?;
    assert_eq!(past.steps.len(), 1);
    assert!(past.steps[0].error.is_empty());
    Ok(())
}

fn assert_stubborn_receipt(bench: &Bench, report: &RunReport) -> Result<(), Box<dyn Error>> {
    let run = read_run_json(report)?;
    let step = one_step(&run)?;
    assert_eq!(
        step.get("pid").and_then(Value::as_i64),
        Some(i64::from(STUBBORN_PID))
    );
    assert_eq!(
        step.get("pgid").and_then(Value::as_i64),
        Some(i64::from(STUBBORN_PGID))
    );
    match step.get("death_proof") {
        // `run.json` historically omits false booleans; both shapes mean exactly "not proved".
        None | Some(Value::Bool(false)) => {}
        other => panic!("the stubborn agent was falsely recorded as dead: {other:?}"),
    }
    assert_eq!(
        step.get("error").and_then(Value::as_str),
        Some(SURVIVOR_ERROR)
    );

    let past = read_run_inner(bench.project.path(), run_folder(report)?)?;
    assert_eq!(past.steps.len(), 1);
    assert_eq!(past.steps[0].error, SURVIVOR_ERROR);
    Ok(())
}

fn read_run_json(report: &RunReport) -> Result<Value, Box<dyn Error>> {
    Ok(serde_json::from_slice(&fs::read(
        report.dir.join("run.json"),
    )?)?)
}

fn one_step(run: &Value) -> Result<&Value, Box<dyn Error>> {
    run.pointer("/steps/0")
        .ok_or_else(|| "run.json did not preserve its only step".into())
}

fn run_folder(report: &RunReport) -> Result<&str, Box<dyn Error>> {
    report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "the run directory has no UTF-8 folder name".into())
}

async fn wait_until_the_handle_is_live<F>(
    fake: &Fake,
    run: &mut Pin<&mut F>,
) -> Result<(), Box<dyn Error>>
where
    F: Future<Output = Result<RunReport, RunError>>,
{
    let started = fake.started.notified();
    tokio::pin!(started);
    tokio::select! {
        () = &mut started => Ok(()),
        result = run => Err(format!(
            "the run returned before its fake AgentHandle entered the turn: {result:?}"
        ).into()),
    }
}

struct Bench {
    home: TempDir,
    project: TempDir,
    workflow: PathBuf,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(home.path().join("agents").join("t134-builder.md"), AGENT)?;
        let workflow = home.path().join("workflows").join("t134-stop.json");
        fs::write(&workflow, WORKFLOW)?;
        Ok(Self {
            home,
            project,
            workflow,
        })
    }

    fn app(&self, drivers: Drivers) -> Result<AppState, Box<dyn Error>> {
        let store = Store::open(&self.project.path().join(".loadout").join("loadout.db"))?;
        Ok(AppState::new(
            self.home.path().to_path_buf(),
            self.project.path().to_path_buf(),
            store,
            drivers,
        ))
    }

    fn request(&self) -> RunRequest {
        RunRequest {
            workflow: self.workflow.clone(),
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        }
    }
}

#[derive(Clone, Copy)]
enum Scenario {
    StubbornThenSuccessful,
    DeadOnFirstCancel,
}

struct Fake {
    scenario: Scenario,
    starts: AtomicUsize,
    cancel_calls: Arc<AtomicUsize>,
    successful_waits: Arc<AtomicUsize>,
    started: Notify,
}

impl Fake {
    fn stubborn_then_successful() -> Self {
        Self::new(Scenario::StubbornThenSuccessful)
    }

    fn dead_on_first_cancel() -> Self {
        Self::new(Scenario::DeadOnFirstCancel)
    }

    fn new(scenario: Scenario) -> Self {
        Self {
            scenario,
            starts: AtomicUsize::new(0),
            cancel_calls: Arc::new(AtomicUsize::new(0)),
            successful_waits: Arc::new(AtomicUsize::new(0)),
            started: Notify::new(),
        }
    }
}

fn fake_drivers(fake: Arc<Fake>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = fake;
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        "t134-fake"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("t134".to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        _events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let index = self.starts.fetch_add(1, Ordering::AcqRel);
        self.started.notify_one();
        let session = SessionRef {
            vendor: "t134-fake",
            id: spec.run_id.to_string(),
        };
        match (self.scenario, index) {
            (Scenario::StubbornThenSuccessful, 0) => Ok(Box::new(WaitingHandle {
                session,
                group: GroupId {
                    pid: STUBBORN_PID,
                    pgid: STUBBORN_PGID,
                },
                answer: CancelAnswer::Alive,
                cancel_calls: Arc::clone(&self.cancel_calls),
            })),
            (Scenario::StubbornThenSuccessful, 1) => Ok(Box::new(SuccessfulHandle {
                session,
                waits: Arc::clone(&self.successful_waits),
            })),
            (Scenario::DeadOnFirstCancel, 0) => Ok(Box::new(WaitingHandle {
                session,
                group: GroupId {
                    pid: CONTROL_PID,
                    pgid: CONTROL_PGID,
                },
                answer: CancelAnswer::Dead,
                cancel_calls: Arc::clone(&self.cancel_calls),
            })),
            _ => Err(anyhow::anyhow!(
                "the T-134 fixture started an unexpected agent"
            )),
        }
    }
}

#[derive(Clone, Copy)]
enum CancelAnswer {
    Alive,
    Dead,
}

struct WaitingHandle {
    session: SessionRef,
    group: GroupId,
    answer: CancelAnswer,
    cancel_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl AgentHandle for WaitingHandle {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        Some(self.group)
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        std::future::pending().await
    }

    async fn cancel(&mut self) -> GroupProof {
        self.cancel_calls.fetch_add(1, Ordering::AcqRel);
        match self.answer {
            CancelAnswer::Alive => GroupProof::Alive,
            CancelAnswer::Dead => GroupProof::Dead { status: None },
        }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(None)
    }
}

struct SuccessfulHandle {
    session: SessionRef,
    waits: Arc<AtomicUsize>,
}

#[async_trait]
impl AgentHandle for SuccessfulHandle {
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
        self.waits.fetch_add(1, Ordering::AcqRel);
        Ok(TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "Done.".to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::ZERO,
            session: self.session.clone(),
        })
    }

    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}
