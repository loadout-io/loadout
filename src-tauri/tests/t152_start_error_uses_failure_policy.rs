//! T-152 AC-2: `AgentDriver::start -> Err` przechodzi przez ten sam `whenItFails` co żywa tura.

#![allow(clippy::too_many_lines)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use loadout_lib::commands::run::{continue_run_inner, run_workflow_inner, stop_run_inner};
use loadout_lib::commands::{Drivers, Outcome, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::line_channel;
use loadout_lib::library::agents::Vendor;
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::mpsc;

const PATIENCE: Duration = Duration::from_secs(20);
const REFUSAL: &str = "T152 controlled start refusal";
const PUBLIC_REFUSAL: &str = "Loadout could not start this agent: T152 controlled start refusal";

const AGENT: &str = r#"---
schema: 1
id: 019b0152-0000-7000-8000-000000000002
name: Failure-policy worker
summary: Proves one failure funnel
color: slate
runsWith: claude-code
model: sonnet
thinking: balanced
fileAccess: look-only
giveUpAfterMinutes: 20
writeResultsTo: ""
tools: everything
skills: []
connections: []
---
Follow the step instructions.
"#;

#[derive(Debug)]
struct Bench {
    _home: TempDir,
    _project: TempDir,
    home: PathBuf,
    project: PathBuf,
    starts: Arc<Mutex<Vec<Started>>>,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = tempfile::tempdir()?;
        let project = tempfile::tempdir()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(home.path().join("agents/failure-policy.md"), AGENT)?;
        Ok(Self {
            home: home.path().to_path_buf(),
            project: project.path().to_path_buf(),
            _home: home,
            _project: project,
            starts: Arc::new(Mutex::new(Vec::new())),
        })
    }

    fn database(&self) -> PathBuf {
        self.project.join(".loadout/loadout.db")
    }

    fn workflow(&self, policy: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.home.join("workflows").join(format!("{policy}.json"));
        let workflow = json!({
            "format": 1,
            "id": format!("wf_t152_{policy}"),
            "name": format!("Start refusal uses {policy}"),
            "steps": [
                {
                    "kind": "agent",
                    "id": "parent",
                    "name": "Parent step",
                    "agent": "019b0152-0000-7000-8000-000000000002",
                    "overrides": {},
                    "instructions": "refuse-before-handle",
                    "folder": { "use": "project" },
                    "whenItFails": policy,
                    "at": { "x": 0, "y": 0 }
                },
                {
                    "kind": "agent",
                    "id": "child",
                    "name": "Child step",
                    "agent": "019b0152-0000-7000-8000-000000000002",
                    "overrides": {},
                    "instructions": "child-after-refusal",
                    "folder": { "use": "project" },
                    "at": { "x": 240, "y": 0 }
                }
            ],
            "links": [{ "from": "parent", "to": "child" }]
        });
        fs::write(&path, serde_json::to_vec_pretty(&workflow)?)?;
        Ok(path)
    }

    async fn run(&self, policy: &str, decision: Decision) -> Result<Observed, Box<dyn Error>> {
        lock(&self.starts).clear();
        let store = Store::open(&self.database())?;
        let control = RunControl::new();
        let deps = RunDeps {
            home: &self.home,
            project: &self.project,
            store: &store,
            drivers: drivers(Arc::clone(&self.starts)),
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control,
        };
        let request = RunRequest {
            workflow: self.workflow(policy)?,
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        };
        let (sink, mut source) = line_channel(2_048);
        let run = run_workflow_inner(&deps, &request, sink);
        let intervention = intervene(&deps, &self.project, decision);
        let (report, intervention) =
            tokio::time::timeout(PATIENCE, async { tokio::join!(run, intervention) })
                .await
                .map_err(|_| format!("the {policy} scenario exceeded {PATIENCE:?}"))?;
        let report = report?;
        let intervention = intervention?;
        let mut lines = Vec::new();
        while let Some(line) = source.try_next() {
            lines.push(line);
        }
        let run_file: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
        Ok(Observed {
            report,
            run_file,
            lines,
            starts: lock(&self.starts).clone(),
            intervention,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Decision {
    None,
    Continue,
    Stop,
}

#[derive(Debug)]
struct Intervention {
    paused: Option<Value>,
    outcome: Option<Outcome>,
}

#[derive(Debug)]
struct Observed {
    report: RunReport,
    run_file: Value,
    lines: Vec<Line>,
    starts: Vec<Started>,
    intervention: Intervention,
}

#[derive(Clone, Debug)]
struct Started {
    prompt: String,
    returned_handle: bool,
}

async fn intervene(
    deps: &RunDeps<'_>,
    project: &Path,
    decision: Decision,
) -> Result<Intervention, loadout_lib::commands::RunError> {
    if decision == Decision::None {
        return Ok(Intervention {
            paused: None,
            outcome: None,
        });
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(run) = latest_run(project)
            && run.get("status").and_then(Value::as_str) == Some("paused")
        {
            let paused = Some(run);
            let outcome = match decision {
                Decision::Continue => {
                    continue_run_inner(deps, Some("Continue".to_owned())).await?;
                    None
                }
                Decision::Stop => Some(stop_run_inner(deps).await?),
                Decision::None => None,
            };
            return Ok(Intervention { paused, outcome });
        }
        if !deps.control.is_working()
            && latest_run(project).is_some_and(|run| {
                !matches!(
                    run.get("status").and_then(Value::as_str),
                    Some("running" | "paused")
                )
            })
        {
            return Ok(Intervention {
                paused: None,
                outcome: None,
            });
        }
        if Instant::now() >= deadline {
            return Ok(Intervention {
                paused: None,
                outcome: None,
            });
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn start_refusal_obeys_stop_carry_on_and_both_ask_me_answers() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let stopped = bench.run("stop", Decision::None).await?;
    let carried = bench.run("carry-on", Decision::None).await?;
    let continued = bench.run("ask-me", Decision::Continue).await?;
    let cancelled = bench.run("ask-me", Decision::Stop).await?;

    assert_eq!(
        stopped.report.steps,
        vec![StepState::Failed, StepState::Skipped]
    );
    assert_eq!(stopped.starts.len(), 1);

    assert_eq!(
        carried.report.steps,
        vec![StepState::Failed, StepState::Succeeded],
        "carry-on did not release the child after AgentDriver::start returned Err"
    );
    let child = carried
        .starts
        .iter()
        .find(|start| start.returned_handle)
        .ok_or("carry-on never started the child")?;
    assert!(
        child.prompt.contains("Parent step"),
        "the child did not receive a named failed handoff: {:?}",
        child.prompt
    );
    let handed = failed_handoff_in(&child.prompt)?;
    assert!(
        handed.contains(PUBLIC_REFUSAL),
        "the named failed handoff lost the public refusal: {handed:?}"
    );

    assert_asked(&continued)?;
    assert_eq!(
        continued.report.steps,
        vec![StepState::Failed, StepState::Succeeded]
    );
    assert_eq!(
        continued
            .intervention
            .paused
            .as_ref()
            .and_then(|run| step(run, "Parent step"))
            .and_then(|row| row.get("error"))
            .and_then(Value::as_str),
        Some(PUBLIC_REFUSAL),
        "the paused receipt and the visible question disagreed about the refusal"
    );

    assert_asked(&cancelled)?;
    assert_eq!(cancelled.intervention.outcome, Some(Outcome::Cancelled));
    assert_eq!(cancelled.report.outcome, Outcome::Cancelled);
    assert_eq!(
        cancelled.report.steps,
        vec![StepState::Cancelled, StepState::Cancelled],
        "Stop at the real question must remain Outcome::Cancelled rather than an error"
    );

    for observed in [&stopped, &carried, &continued, &cancelled] {
        let parent = step(&observed.run_file, "Parent step").ok_or("run.json lost Parent step")?;
        assert_eq!(parent.get("pid"), Some(&Value::Null));
        assert_eq!(parent.get("pgid"), Some(&Value::Null));
        assert_eq!(parent.get("death_proof"), Some(&Value::Bool(false)));
        assert!(
            parent
                .get("error")
                .and_then(Value::as_str)
                .is_some_and(|error| error.starts_with(PUBLIC_REFUSAL)),
            "run.json did not keep the public start refusal: {parent:?}"
        );
    }
    Ok(())
}

fn assert_asked(observed: &Observed) -> Result<(), Box<dyn Error>> {
    let asked = observed
        .lines
        .iter()
        .find_map(|line| match line {
            Line::Asked {
                agent,
                text,
                options,
            } => Some((agent, text, options)),
            _ => None,
        })
        .ok_or("start -> Err never published a real Asked line")?;
    assert_eq!(asked.0, "Parent step");
    assert!(
        asked.1.contains(PUBLIC_REFUSAL),
        "question was: {}",
        asked.1
    );
    assert!(
        asked.2.is_empty(),
        "the existing checkpoint screen owns Continue and Stop; backend options drifted to {:?}",
        asked.2
    );
    assert!(observed.intervention.paused.is_some());
    Ok(())
}

fn step<'a>(run: &'a Value, name: &str) -> Option<&'a Value> {
    run.get("steps")?
        .as_array()?
        .iter()
        .find(|row| row.get("name").and_then(Value::as_str) == Some(name))
}

/// Odczytuje ciało pliku wskazanego przez wiersz nieudanego rodzica w indeksie handoffów.
/// Powód pozostaje w pliku; etykieta relacji jest celowo krótka, żeby nie łamać limitu T-87.
fn failed_handoff_in(prompt: &str) -> Result<String, Box<dyn Error>> {
    let row = prompt
        .lines()
        .find(|line| line.contains("handoffs/"))
        .ok_or("the next step received no handoff row")?;
    assert!(
        row.contains("did not pass"),
        "the handoff row did not identify the failed parent: {row:?}"
    );
    let named = row
        .split_whitespace()
        .find(|word| word.contains("handoffs/"))
        .ok_or("the handoff row names no path")?;
    let path = PathBuf::from(named.trim_end_matches([',', ';', ':', ')']));
    Ok(fs::read_to_string(path)?)
}

fn latest_run(project: &Path) -> Option<Value> {
    let entries = fs::read_dir(project.join(".loadout/runs")).ok()?;
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| fs::read(entry.path().join("run.json")).ok())
        .filter_map(|bytes| serde_json::from_slice(&bytes).ok())
        .max_by_key(|run: &Value| run.get("created_at").and_then(Value::as_i64).unwrap_or(0))
}

fn drivers(starts: Arc<Mutex<Vec<Started>>>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(FakeDriver { starts });
    Arc::new(move |_vendor: Vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct FakeDriver {
    starts: Arc<Mutex<Vec<Started>>>,
}

#[async_trait]
impl AgentDriver for FakeDriver {
    fn id(&self) -> &'static str {
        "t152-fake"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("t152-fake".to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let refused = spec.prompt.contains("refuse-before-handle");
        lock(&self.starts).push(Started {
            prompt: spec.prompt.clone(),
            returned_handle: !refused,
        });
        if refused {
            anyhow::bail!(REFUSAL);
        }
        let session = SessionRef {
            vendor: "t152-fake",
            id: spec.run_id.to_string(),
        };
        let _ = events
            .send(
                AgentEvent::Started {
                    session: session.clone(),
                    model: "fixture".to_owned(),
                    tools: Vec::new(),
                    capabilities: Vec::new(),
                }
                .into(),
            )
            .await;
        Ok(Box::new(FakeHandle { events, session }))
    }
}

#[derive(Debug)]
struct FakeHandle {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for FakeHandle {
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
            text: "## Answer\nThe child ran.\n\n## Evidence\nrun.json\n\n## Open\nNone.\n"
                .to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::ZERO,
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

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
