//! Z-18: Stop kończy bieg przed prywatną turą, a trwały rachunek mówi, czemu jej nie było.

use std::error::Error;
use std::fs;
use std::future;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::run::{run_workflow_with_reflection, stop_run_inner};
use loadout_lib::commands::{Drivers, Outcome, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::Vendor;
use loadout_lib::store::Store;
use serde_json::Value;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::{Notify, mpsc};

const PATIENCE: Duration = Duration::from_secs(5);
const ANSWER: &str =
    "## Answer\nThe first step finished.\n\n## Evidence\nresult.md\n\n## Open\nNone.\n";

const AGENT: &str = r"---
schema: 1
id: 01990000-0000-7000-8000-000000000218
name: Z18 Builder
summary: Leaves one useful handoff before Stop
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
Exercise the reflection boundary.
";

const ONE_STEP: &str = r#"{
  "format": 1, "id": "wf_z18_one", "name": "One useful step",
  "steps": [{
    "kind": "agent", "id": "first", "name": "First",
    "agent": "01990000-0000-7000-8000-000000000218", "overrides": {},
    "instructions": "Leave a useful handoff.", "folder": { "use": "project" },
    "at": { "x": 0, "y": 0 }
  }],
  "links": []
}"#;

const TWO_STEPS: &str = r#"{
  "format": 1, "id": "wf_z18_stop", "name": "Stop after useful work",
  "steps": [
    {
      "kind": "agent", "id": "first", "name": "First",
      "agent": "01990000-0000-7000-8000-000000000218", "overrides": {},
      "instructions": "Leave a useful handoff.", "folder": { "use": "project" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent", "id": "second", "name": "Second",
      "agent": "01990000-0000-7000-8000-000000000218", "overrides": {},
      "instructions": "Wait for Stop.", "folder": { "use": "project" },
      "at": { "x": 200, "y": 0 }
    }
  ],
  "links": [{ "from": "first", "to": "second" }]
}"#;

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_stopped_run_never_asks_what_it_taught_us() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new(TWO_STEPS)?;
    let seen = Arc::new(Seen::default());
    let store = Store::open(&bench.db())?;
    let deps = bench.deps(&store, Arc::clone(&seen));
    let (lines, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let request = bench.request();
    let run = run_workflow_with_reflection(&deps, &request, lines, None, true);
    tokio::pin!(run);

    // 2026-09 (Z-18): `Notify` dowodzi, że Stop pada dopiero po udanym pierwszym kroku i po
    // starcie drugiego; zegar nie jest tu wyrocznią i może bezpiecznie stać.
    let second_started = seen.second_started.notified();
    tokio::pin!(second_started);
    tokio::select! {
        () = &mut second_started => {}
        result = &mut run => {
            return Err(format!("the run returned before its second step could be stopped: {result:?}").into());
        }
    }

    let (stopped, finished) = tokio::time::timeout(PATIENCE, async {
        tokio::join!(stop_run_inner(&deps), &mut run)
    })
    .await?;
    let report = finished?;
    tokio::time::timeout(PATIENCE, pump).await??;

    assert_eq!(stopped?, Outcome::Cancelled);
    assert_eq!(report.outcome, Outcome::Cancelled);
    assert_eq!(seen.steps.load(Ordering::Acquire), 2);
    assert_eq!(
        seen.reflections.load(Ordering::Acquire),
        0,
        "Stop reached AgentDriver::reflecting(), so Loadout started arranging a private turn for \
         a run the person had already cancelled"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_turned_off_reflection_says_why_in_the_run_a_person_opens() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new(ONE_STEP)?;
    let seen = Arc::new(Seen::default());
    let store = Store::open(&bench.db())?;
    let deps = bench.deps(&store, seen);
    let (lines, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let report = tokio::time::timeout(
        PATIENCE,
        run_workflow_with_reflection(&deps, &bench.request(), lines, None, false),
    )
    .await??;
    tokio::time::timeout(PATIENCE, pump).await??;

    let folder = run_folder(&report)?;
    // Droga tego samego odczytu, którego używa okno (niezmiennik 29), a `Value` utrzymuje test
    // kompilowalny przed dodaniem pola do drutu historii.
    let opened = serde_json::to_value(read_run_inner(bench.project.path(), folder)?)?;
    assert_eq!(opened.pointer("/reflection/ran"), Some(&Value::Bool(false)));
    assert_eq!(
        opened.pointer("/reflection/why"),
        Some(&Value::String("turned-off".to_owned())),
        "the run a person opens does not say that learning was turned off"
    );
    Ok(())
}

fn run_folder(report: &RunReport) -> Result<&str, Box<dyn Error>> {
    report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "the run directory has no UTF-8 folder name".into())
}

#[derive(Default)]
struct Seen {
    steps: AtomicUsize,
    reflections: AtomicUsize,
    second_started: Notify,
}

#[derive(Clone)]
struct Fake {
    seen: Arc<Seen>,
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        "z18-fake"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("z18".to_owned()),
        })
    }

    fn reflecting(&self) -> Option<Arc<dyn AgentDriver>> {
        self.seen.reflections.fetch_add(1, Ordering::AcqRel);
        None
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let index = self.seen.steps.fetch_add(1, Ordering::AcqRel);
        let session = announce(&events, &spec).await;
        if index == 0 {
            return Ok(Box::new(ReadyTurn { events, session }));
        }
        if index == 1 {
            self.seen.second_started.notify_one();
            return Ok(Box::new(WaitingTurn { session }));
        }
        Err(anyhow::anyhow!(
            "the Z-18 fixture started an unexpected step"
        ))
    }
}

async fn announce(events: &mpsc::Sender<DecodedEvent>, spec: &RunSpec) -> SessionRef {
    let session = SessionRef {
        vendor: "z18-fake",
        id: spec.run_id.to_string(),
    };
    let _ = events
        .send(
            AgentEvent::Started {
                session: session.clone(),
                model: spec.model.clone().unwrap_or_default(),
                tools: Vec::new(),
                capabilities: Vec::new(),
            }
            .into(),
        )
        .await;
    session
}

struct ReadyTurn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for ReadyTurn {
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
        let outcome = turn_outcome(&self.session, ANSWER);
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

struct WaitingTurn {
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for WaitingTurn {
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
        future::pending().await
    }

    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

fn turn_outcome(session: &SessionRef, text: &str) -> TurnOutcome {
    TurnOutcome {
        ok: true,
        reason: FinishReason::Completed,
        text: text.to_owned(),
        cost_usd: Some(0.01),
        tokens: Tokens::default(),
        turns: 1,
        took: Duration::ZERO,
        session: session.clone(),
    }
}

fn fake_drivers(seen: Arc<Seen>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { seen });
    Arc::new(move |_vendor: Vendor| Arc::clone(&driver))
}

struct Bench {
    home: TempDir,
    project: TempDir,
    workflow: PathBuf,
}

impl Bench {
    fn new(workflow: &str) -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(home.path().join("agents").join("z18-builder.md"), AGENT)?;
        let workflow_path = home.path().join("workflows").join("z18.json");
        fs::write(&workflow_path, workflow)?;
        Ok(Self {
            home,
            project,
            workflow: workflow_path,
        })
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
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

    fn deps<'a>(&'a self, store: &'a Store, seen: Arc<Seen>) -> RunDeps<'a> {
        RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store,
            drivers: fake_drivers(seen),
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        }
    }
}
