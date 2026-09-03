//! Z-12: bieg nie uruchamia kroku, kiedy z jego sufitu zostało mniej niż jeden cent.
//!
//! Dwa kroki są połączone strzałką, więc drugi podejmuje decyzję dopiero po zaksięgowaniu
//! $9.996 pierwszego. Wyrocznia czyta odmowę z `run.json` i zapisuje prawdziwy fragment argv
//! w `configured`; sam test `budget_argv` nie dowiódłby, że bieg odmówił startu (niezmiennik 29).

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_with_budget;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::claude::VENDOR;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, DriverConfiguration, FinishReason,
    Outcome as TurnOutcome, Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::{LineSink, QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use serde_json::Value;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

const BUDGET_FLAG: &str = "--max-budget-usd";
const BUDGET: f64 = 10.0;
const FIRST_COSTS: f64 = 9.996;
const PATIENCE: Duration = Duration::from_secs(20);
const BUDGET_SENTENCE: &str = "Skipped: this run had spent $10.00 of the $10.00 it was allowed, so nothing new was started. Steps already working were left to finish.";

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-000000001212
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

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_sub_cent_budget",
  "name": "A sub-cent remainder",
  "steps": [
    {
      "kind": "agent",
      "id": "s_first",
      "name": "First",
      "agent": "01990000-0000-7000-8000-000000001212",
      "overrides": {},
      "instructions": "go first",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_second",
      "name": "Second",
      "agent": "01990000-0000-7000-8000-000000001212",
      "overrides": {},
      "instructions": "go second",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    }
  ],
  "links": [{ "from": "s_first", "to": "s_second" }]
}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_sub_cent_remainder_refuses_the_step_with_the_budget_sentence()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let heard = Arc::new(Heard::default());
    let (report, run) = bench.run(Arc::clone(&heard)).await?;
    let fragments = heard.taken();

    let unsafe_amounts: Vec<&str> = fragments
        .iter()
        .filter_map(|fragment| amount_in(fragment))
        .filter(|amount| amount.parse::<f64>().is_ok_and(|value| value <= 0.0))
        .collect();
    assert!(
        unsafe_amounts.is_empty(),
        "a sub-cent remainder reached Claude as an invalid zero or negative budget: \
         {unsafe_amounts:?}; all configured argv fragments were {fragments:?}"
    );
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded, StepState::Skipped],
        "the second step must be refused before start when only $0.004 remains; it ended as \
         {:?}, and configured saw {fragments:?}",
        report.steps
    );

    let second = step_named(&run, "Second")?;
    assert_eq!(
        second.get("status").and_then(Value::as_str),
        Some("skipped"),
        "run.json says the unaffordable step was not skipped: {second}"
    );
    assert_eq!(
        second.get("error").and_then(Value::as_str),
        Some(BUDGET_SENTENCE),
        "the history must use the same budget sentence as an exhausted run: {second}"
    );
    Ok(())
}

fn amount_in(fragment: &[String]) -> Option<&str> {
    let at = fragment.iter().position(|arg| arg == BUDGET_FLAG)?;
    fragment.get(at + 1).map(String::as_str)
}

fn step_named<'a>(run: &'a Value, name: &str) -> Result<&'a Value, Box<dyn Error>> {
    run.get("steps")
        .and_then(Value::as_array)
        .ok_or("run.json has no steps array")?
        .iter()
        .find(|step| step.get("name").and_then(Value::as_str) == Some(name))
        .ok_or_else(|| format!("run.json has no step named {name}").into())
}

struct Bench {
    home: TempDir,
    project: TempDir,
    workflow: PathBuf,
    store: Store,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;

        let agent = home.path().join("agents").join("hand.md");
        fs::write(&agent, HAND_FILE)?;
        let workflow = home.path().join("workflows").join("sub-cent.json");
        fs::write(&workflow, WORKFLOW)?;
        the_fixture_can_run(&workflow, &[&agent])?;
        let store = Store::open(&project.path().join(".loadout").join("loadout.db"))?;
        Ok(Self {
            home,
            project,
            workflow,
            store,
        })
    }

    async fn run(&self, heard: Arc<Heard>) -> Result<(RunReport, Value), Box<dyn Error>> {
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store: &self.store,
            drivers: fake_drivers(heard),
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow: self.workflow.clone(),
            how_many_at_once: 2,
            task: None,
            part: None,
            handoffs_from: None,
        };
        let (sink, drain) = the_pump_seam();
        let (ran, ()) = tokio::time::timeout(PATIENCE, async {
            tokio::join!(
                run_workflow_with_budget(&deps, &request, sink, Some(BUDGET)),
                drain
            )
        })
        .await
        .map_err(|_| format!("the run did not finish within {PATIENCE:?}"))?;
        let report = ran?;
        let run = serde_json::from_str(&fs::read_to_string(report.dir.join("run.json"))?)?;
        Ok((report, run))
    }
}

fn the_pump_seam() -> (LineSink, impl Future<Output = ()>) {
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    (sink, async move {
        let _ = pump.await;
    })
}

fn the_fixture_can_run(workflow: &Path, agents: &[&Path]) -> Result<(), Box<dyn Error>> {
    let problems: Vec<String> = check(&load(workflow)?)
        .into_iter()
        .filter(|note| note.level == Level::Problem)
        .map(|note| note.message)
        .collect();
    assert!(
        problems.is_empty(),
        "the fixture would be refused before it ran: {problems:?}"
    );
    for agent in agents {
        read_agent_file(agent).map_err(|error| format!("{}: {error}", agent.display()))?;
    }
    Ok(())
}

#[derive(Debug, Default)]
struct Heard {
    fragments: Mutex<Vec<Vec<String>>>,
}

impl Heard {
    fn saw(&self, arguments: &[String]) {
        self.fragments
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(arguments.to_vec());
    }

    fn taken(&self) -> Vec<Vec<String>> {
        self.fragments
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

fn fake_drivers(heard: Arc<Heard>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { heard });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Clone, Debug)]
struct Fake {
    heard: Arc<Heard>,
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

    fn configured(&self, configuration: &DriverConfiguration) -> Option<Arc<dyn AgentDriver>> {
        self.heard.saw(&configuration.arguments);
        Some(Arc::new(self.clone()))
    }

    fn with_evidence(&self, _target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
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
            cost: if spec.prompt.contains("go first") {
                FIRST_COSTS
            } else {
                0.0
            },
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    cost: f64,
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
            text: String::new(),
            cost_usd: Some(self.cost),
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
