//! Z-45: dwa ciężkie kroki agenta dzielą jedno miejsce ciężkie, a zwykły dalej biegnie obok.
//!
//! `weight` jedzie w tekście pliku, nie przez nowy typ, więc ten test kompiluje się na kodzie
//! sprzed poprawki i pada dopiero w wykonaniu: stary `AgentStep::extra` zachowuje nieznany klucz,
//! ale `weight_of` nadal traktuje oba kroki jak zwykłe.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::limits::Limiter;
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{LineSink, QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio::time::Instant;

const VENDOR: &str = "fake";
const WORK: Duration = Duration::from_secs(1);
const PATIENCE: Duration = Duration::from_secs(10);

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-000000000045
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

/// Trzy kroki gotowe naraz. Osobne kopie są przesłanką pomiaru: bez nich walidator słusznie
/// odmówiłby trzem równoległym pisarzom do jednego folderu (niezmiennik 12).
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_heavy_agents",
  "name": "Heavy agents",
  "steps": [
    {
      "kind": "agent",
      "id": "s_heavy_one",
      "name": "Heavy one",
      "agent": "01990000-0000-7000-8000-000000000045",
      "overrides": {},
      "instructions": "heavy-one: work",
      "weight": "heavy",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_heavy_two",
      "name": "Heavy two",
      "agent": "01990000-0000-7000-8000-000000000045",
      "overrides": {},
      "instructions": "heavy-two: work",
      "weight": "heavy",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_ordinary",
      "name": "Ordinary",
      "agent": "01990000-0000-7000-8000-000000000045",
      "overrides": {},
      "instructions": "ordinary: work",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 240 }
    }
  ],
  "links": []
}
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    Entered,
    Left,
}

#[derive(Debug)]
struct Mark {
    label: String,
    edge: Edge,
    at: Instant,
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn two_heavy_agent_steps_never_share_a_moment() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let store = Store::open(&bench.db())?;
    let (marks, mut recorded) = mpsc::unbounded_channel();
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(marks),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        // 2026-09 (Z-45) — trzy zwykłe miejsca usuwają zwykłą pulę z pomiaru; jedynym
        // zasobem zdolnym uszeregować dwa oznaczone kroki zostaje miejsce ciężkie.
        control: RunControl::sharing(Limiter::with_heavy(3, 1)),
    };
    let request = RunRequest {
        workflow: bench.workflow.clone(),
        how_many_at_once: 3,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (sink, drain) = pump_seam();

    let (ran, ()) = tokio::time::timeout(PATIENCE, async {
        tokio::join!(run_workflow_inner(&deps, &request, sink), drain)
    })
    .await
    .map_err(|_| format!("the run did not finish within {PATIENCE:?}"))?;
    let report = ran?;
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; 3],
        "all three turns must finish before their windows say anything; got {:?}",
        report.steps
    );

    let mut windows: BTreeMap<String, (Option<Instant>, Option<Instant>)> = BTreeMap::new();
    while let Ok(mark) = recorded.try_recv() {
        let window = windows.entry(mark.label).or_default();
        match mark.edge {
            Edge::Entered => window.0 = Some(mark.at),
            Edge::Left => window.1 = Some(mark.at),
        }
    }
    let heavy_one = complete_window(&windows, "heavy-one")?;
    let heavy_two = complete_window(&windows, "heavy-two")?;
    let ordinary = complete_window(&windows, "ordinary")?;

    for (label, (from, to)) in [
        ("heavy-one", heavy_one),
        ("heavy-two", heavy_two),
        ("ordinary", ordinary),
    ] {
        assert_eq!(
            to.saturating_duration_since(from),
            WORK,
            "{label} held a measured window for {:?}, not exactly {WORK:?}; a zero-length turn \
             would make every pair look safely disjoint",
            to.saturating_duration_since(from)
        );
    }

    assert!(
        !overlap(heavy_one, heavy_two),
        "the two agent steps marked heavy shared a moment: {heavy_one:?} and {heavy_two:?}. \
         Then two builds can still pin the memory compressor together (invariant 26)"
    );
    assert!(
        overlap(ordinary, heavy_one) || overlap(ordinary, heavy_two),
        "the ordinary step overlapped neither heavy turn. Serializing every agent would satisfy \
         the safety assertion by removing the parallelism this product promises (invariant 11)"
    );
    Ok(())
}

fn complete_window(
    windows: &BTreeMap<String, (Option<Instant>, Option<Instant>)>,
    label: &str,
) -> Result<(Instant, Instant), Box<dyn Error>> {
    windows
        .get(label)
        .and_then(|&(from, to)| Some((from?, to?)))
        .ok_or_else(|| {
            format!("{label} did not open and close exactly one window: {windows:?}").into()
        })
}

fn overlap(one: (Instant, Instant), other: (Instant, Instant)) -> bool {
    one.0 < other.1 && other.0 < one.1
}

fn pump_seam() -> (LineSink, impl Future<Output = ()>) {
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    (sink, async move {
        let _ = pump.await;
    })
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
        fs::write(home.path().join("agents/hand.md"), HAND_FILE)?;
        fs::write(project.path().join("notes.txt"), "work")?;
        let workflow = home.path().join("workflows/heavy-agents.json");
        fs::write(&workflow, WORKFLOW)?;
        Ok(Self {
            home,
            project,
            workflow,
        })
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout/loadout.db")
    }
}

fn fake_drivers(marks: mpsc::UnboundedSender<Mark>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { marks });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct Fake {
    marks: mpsc::UnboundedSender<Mark>,
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
        let label = spec
            .prompt
            .split_once(':')
            .map_or_else(|| spec.prompt.clone(), |(head, _)| head.trim().to_owned());
        let _ = self.marks.send(Mark {
            label: label.clone(),
            edge: Edge::Entered,
            at: Instant::now(),
        });
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
            marks: self.marks.clone(),
            events,
            session,
            label,
        }))
    }
}

#[derive(Debug)]
struct Turn {
    marks: mpsc::UnboundedSender<Mark>,
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    label: String,
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
        tokio::time::sleep(WORK).await;
        let _ = self.marks.send(Mark {
            label: self.label.clone(),
            edge: Edge::Left,
            at: Instant::now(),
        });
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: String::new(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: WORK,
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
