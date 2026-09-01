//! AC-4 dla T-101: trzy pozostające boczne drogi porażki zostawiają przekazanie dokładnie tak,
//! jak wcześniejsze porażki objęte T-87.
//!
//! Ławka jest wspólna dla czterech kryteriów T-101. To nie jest test-only skrót do decyzji:
//! każdy scenariusz biegnie przez publiczne wejście biegu, prawdziwy plan, planistę, `run.json`
//! i kolejkę linii do okna. Dubel zastępuje wyłącznie płatną aplikację agenta.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::future::pending;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_with_budget;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::line_channel;
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::mpsc;

pub(super) const CONTEXT_REASON: &str =
    "Loadout could not prove the context files for this agent, so it did not start the step.";
pub(super) const NO_ROUTE_REASON: &str =
    "This result does not match any next step in the workflow.";
pub(super) const AMBIGUOUS_ROUTE_REASON: &str =
    "This result matches more than one next step in the workflow.";
pub(super) const LAST_WORDS: &str =
    "I finished the useful part before Loadout had to decide what happens next.";

const VENDOR: &str = "fake";
const AGENT_ID: &str = "01990000-0000-7000-8000-000000001101";
const PATIENCE: Duration = Duration::from_secs(10);
const EVERY: Duration = Duration::from_millis(5);
const BUDGET: f64 = 10.0;
const COST: f64 = 12.0;

const AGENT_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-000000001101
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

/// Co człowiek robi podczas biegu. Osobny wariant `None` pozwala temu samemu runnerowi
/// dowodzić, że `ask-me` naprawdę zaparkowało bieg, zamiast tylko kończyć go później.
#[derive(Debug, Clone, Copy)]
pub(super) enum Intervention<'a> {
    None,
    ContinueWhenPaused,
    StopWhenRunning(&'a str),
}

pub(super) struct RunResult {
    pub report: RunReport,
    pub run_file: Value,
    pub acted: bool,
    pub lines: Vec<Line>,
    pub watch: Arc<Watch>,
}

pub(super) struct Bench {
    home: TempDir,
    project: TempDir,
}

impl Bench {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(home.path().join("agents").join("hand.md"), AGENT_FILE)?;
        fs::write(project.path().join("notes.txt"), "written by the human")?;
        Ok(Self { home, project })
    }

    pub async fn run(
        &self,
        slug: &str,
        workflow: &Value,
        budget: Option<f64>,
        intervention: Intervention<'_>,
    ) -> Result<RunResult, Box<dyn Error>> {
        let workflow_path = self
            .home
            .path()
            .join("workflows")
            .join(format!("{slug}.json"));
        fs::write(&workflow_path, serde_json::to_vec_pretty(workflow)?)?;
        fixture_can_run(&workflow_path)?;

        let store = Store::open(&self.project.path().join(".loadout/loadout.db"))?;
        let watch = Arc::new(Watch::new(self.project.path().to_owned()));
        let control = RunControl::new();
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store: &store,
            drivers: fake_drivers(Arc::clone(&watch)),
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: control.clone(),
        };
        let request = RunRequest {
            workflow: workflow_path,
            how_many_at_once: 4,
            task: None,
            part: None,
            handoffs_from: None,
        };
        // Bez pompy: to kryterium pyta o pojedyncze linie przed serializacją do WKWebView.
        // Pojemność jest celowo dużo większa od kilkunastu linii tej ławki.
        let (sink, mut source) = line_channel(1_024);
        let run = run_workflow_with_budget(&deps, &request, sink, budget);
        let act = intervene(self.project.path(), &control, intervention);
        let (ran, acted) =
            tokio::time::timeout(PATIENCE.saturating_mul(3), async { tokio::join!(run, act) })
                .await
                .map_err(|_| format!("the {slug} run did not finish within the test's patience"))?;
        let report = ran?;
        let acted = acted?;
        let run_file = serde_json::from_str(&fs::read_to_string(report.dir.join("run.json"))?)?;
        let mut lines = Vec::new();
        while let Some(line) = source.try_next() {
            lines.push(line);
        }
        Ok(RunResult {
            report,
            run_file,
            acted,
            lines,
            watch,
        })
    }
}

/// `Source` oddaje prawdziwy plik. Równoległy `Saboteur` czeka na ten plik, usuwa go i dopiero
/// wtedy schodzi. `Context` zależy od obu, więc planista uruchamia go po sabotażu; składanie
/// promptu znajduje zarejestrowane przekazanie bez prawdziwego pliku i trafia dokładnie w
/// `CONTEXT_NOT_PROVEN` podczas wykonania.
pub(super) fn context_workflow(when: &str) -> Value {
    json!({
        "format": 1,
        "id": format!("wf_context_{when}"),
        "name": "A context file disappears",
        "steps": [
            agent("s_source", "Source", "source: leave a real result", "carry-on"),
            agent("s_saboteur", "Saboteur", "sabotage: remove Source's result", "carry-on"),
            agent("s_context", "Context", "context: this driver must never start", when),
            agent("s_after_context", "After context", "after-context: continue", "carry-on")
        ],
        "links": [
            { "from": "s_source", "to": "s_context" },
            { "from": "s_saboteur", "to": "s_context" },
            { "from": "s_context", "to": "s_after_context" }
        ]
    })
}

/// Warunkowe wyjścia, które nie mają jednej odpowiedzi. W pierwszym wariancie wynik nie pasuje
/// do żadnej drogi; w drugim dwie drogi mają ten sam warunek i pasują naraz.
pub(super) fn route_workflow(when: &str, ambiguous: bool) -> Value {
    let instruction = if ambiguous {
        "route-ambiguous: produce decision go"
    } else {
        "route-no-match: produce decision elsewhere"
    };
    let right = if ambiguous { "go" } else { "stop" };
    json!({
        "format": 1,
        "id": format!("wf_route_{when}_{ambiguous}"),
        "name": "A blocked way out",
        "steps": [
            {
                "kind": "agent",
                "id": "s_route",
                "name": "Route",
                "agent": AGENT_ID,
                "overrides": {},
                "instructions": instruction,
                "folder": { "use": "fresh-copy" },
                "handover": {
                    "fields": [{
                        "name": "decision",
                        "describe": "Which way to take",
                        "required": true
                    }]
                },
                "whenItFails": when,
                "at": { "x": 0, "y": 0 }
            },
            agent("s_route_left", "Route left", "after-route-left: continue", "carry-on"),
            agent("s_route_right", "Route right", "after-route-right: continue", "carry-on")
        ],
        "links": [
            { "from": "s_route", "to": "s_route_left" },
            { "from": "s_route", "to": "s_route_right" }
        ],
        "linkConditions": [
            {
                "from": "s_route",
                "to": "s_route_left",
                "when": { "source": "handoff", "field": "decision", "equals": "go" }
            },
            {
                "from": "s_route",
                "to": "s_route_right",
                "when": { "source": "handoff", "field": "decision", "equals": right }
            }
        ]
    })
}

/// `Costly` przekracza sufit. `Budget stop` jest pierwszym krokiem, który nie może ruszyć, a
/// `Below budget` stoi literalnie pod nim — tego potomka brakowało w fiksturze T-94.
pub(super) fn budget_workflow(when: &str) -> Value {
    json!({
        "format": 1,
        "id": format!("wf_budget_cone_{when}"),
        "name": "A spent budget has a cone",
        "steps": [
            agent("s_costly", "Costly", "costly: spend twelve dollars", "carry-on"),
            agent("s_budget", "Budget stop", "budget-stop: this driver must not start", when),
            agent("s_below_budget", "Below budget", "after-budget: continue", "carry-on")
        ],
        "links": [
            { "from": "s_costly", "to": "s_budget" },
            { "from": "s_budget", "to": "s_below_budget" }
        ]
    })
}

pub(super) fn stop_workflow() -> Value {
    json!({
        "format": 1,
        "id": "wf_real_stop_control",
        "name": "A person stops the run",
        "steps": [
            agent("s_running", "Running", "hang: wait for Stop", "carry-on"),
            agent("s_below_stop", "Below Stop", "after-stop: must not run", "carry-on")
        ],
        "links": [{ "from": "s_running", "to": "s_below_stop" }]
    })
}

fn agent(id: &str, name: &str, instructions: &str, when: &str) -> Value {
    json!({
        "kind": "agent",
        "id": id,
        "name": name,
        "agent": AGENT_ID,
        "overrides": {},
        "instructions": instructions,
        "folder": { "use": "fresh-copy" },
        "whenItFails": when,
        "at": { "x": 0, "y": 0 }
    })
}

pub(super) fn step<'a>(run_file: &'a Value, name: &str) -> Result<&'a Value, Box<dyn Error>> {
    run_file
        .get("steps")
        .and_then(Value::as_array)
        .ok_or("run.json has no steps")?
        .iter()
        .find(|row| row.get("name").and_then(Value::as_str) == Some(name))
        .ok_or_else(|| format!("run.json has no step named {name}").into())
}

pub(super) fn status(run_file: &Value, name: &str) -> Result<String, Box<dyn Error>> {
    Ok(step(run_file, name)?
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{name} has no status"))?
        .to_owned())
}

pub(super) fn reason(run_file: &Value, name: &str) -> Result<String, Box<dyn Error>> {
    Ok(step(run_file, name)?
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned())
}

pub(super) fn last_stream_state(lines: &[Line], step_id: &str) -> Option<String> {
    lines.iter().rev().find_map(|line| match line {
        Line::StepState {
            step_id: found,
            state,
            ..
        } if found == step_id => Some(state.clone()),
        _ => None,
    })
}

/// Dowodzi obu połówek przekazania: etykieta mówi, że poprzednik nie przeszedł, a wskazana
/// ścieżka prowadzi do prawdziwego pliku. Oddaje ciało, żeby scenariusz z ostatnimi słowami mógł
/// osądzić ich treść, a scenariusz bez słów mógł uczciwie zaakceptować pustkę.
pub(super) fn failed_handoff_in(prompt: &str) -> Result<String, Box<dyn Error>> {
    let row = prompt
        .lines()
        .find(|line| line.contains("handoffs/"))
        .ok_or("the next step received no handoff row")?;
    assert!(
        row.contains("did not pass"),
        "the next step got a file but its row does not say the step before did not pass: {row:?}"
    );
    let named = row
        .split_whitespace()
        .find(|word| word.contains("handoffs/"))
        .ok_or("the handoff row names no path")?;
    let path = PathBuf::from(named.trim_end_matches([',', ';', ':', ')']));
    Ok(fs::read_to_string(path)?)
}

fn fixture_can_run(workflow: &Path) -> Result<(), Box<dyn Error>> {
    let problems: Vec<String> = check(&load(workflow)?)
        .into_iter()
        .filter(|note| note.level == Level::Problem)
        .map(|note| note.message)
        .collect();
    assert!(
        problems.is_empty(),
        "the fixture would be refused before it ran, so it cannot prove runtime behavior: \
         {problems:?}"
    );
    Ok(())
}

async fn intervene(
    project: &Path,
    control: &RunControl,
    intervention: Intervention<'_>,
) -> Result<bool, Box<dyn Error>> {
    if matches!(intervention, Intervention::None) {
        control.wait_until_settled().await;
        return Ok(false);
    }
    let until = Instant::now() + PATIENCE;
    loop {
        if let Some(run_file) = current_run_file(project)? {
            let should_act = match intervention {
                Intervention::ContinueWhenPaused => {
                    // `asking` jest prywatnym faktem `RunControl` i celowo nie ma jego drugiej
                    // kopii w `run.json`. Ten dubel nie emituje limitu dostawcy, więc jedyny
                    // możliwy stan `paused` pochodzi z pytania, na które test ma odpowiedzieć.
                    run_file.get("status").and_then(Value::as_str) == Some("paused")
                }
                Intervention::StopWhenRunning(name) => {
                    status(&run_file, name).is_ok_and(|state| state == "running")
                }
                Intervention::None => false,
            };
            if should_act {
                match intervention {
                    Intervention::ContinueWhenPaused => control.go_on_with(Some(
                        "Carry on with the work, but keep the failed step visible.".to_owned(),
                    )),
                    Intervention::StopWhenRunning(_) => control.stop(),
                    Intervention::None => {}
                }
                return Ok(true);
            }
        }
        if Instant::now() >= until {
            return Err("the run never reached the state the intervention waits for".into());
        }
        tokio::select! {
            () = control.wait_until_settled() => return Ok(false),
            () = tokio::time::sleep(EVERY) => {}
        }
    }
}

fn current_run_file(project: &Path) -> Result<Option<Value>, Box<dyn Error>> {
    let runs = project.join(".loadout/runs");
    let Ok(entries) = fs::read_dir(runs) else {
        return Ok(None);
    };
    for entry in entries {
        let path = entry?.path().join("run.json");
        if let Ok(text) = fs::read_to_string(path) {
            return Ok(Some(serde_json::from_str(&text)?));
        }
    }
    Ok(None)
}

fn fake_drivers(watch: Arc<Watch>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { watch });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
pub(super) struct Watch {
    project: PathBuf,
    prompts: Mutex<Vec<String>>,
}

impl Watch {
    fn new(project: PathBuf) -> Self {
        Self {
            project,
            prompts: Mutex::new(Vec::new()),
        }
    }

    pub fn prompt_starting(&self, prefix: &str) -> Option<String> {
        self.lock()
            .iter()
            .find(|prompt| prompt.starts_with(prefix))
            .cloned()
    }

    fn saw(&self, prompt: &str) {
        self.lock().push(prompt.to_owned());
    }

    fn lock(&self) -> MutexGuard<'_, Vec<String>> {
        self.prompts.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Debug)]
struct Fake {
    watch: Arc<Watch>,
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
        self.watch.saw(&spec.prompt);
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
            prompt: spec.prompt,
            project: self.watch.project.clone(),
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    prompt: String,
    project: PathBuf,
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
        if self.prompt.starts_with("hang:") {
            pending::<()>().await;
            unreachable!("a hanging turn only ends through AgentHandle::cancel")
        }
        if self.prompt.starts_with("sabotage:") {
            remove_source_handoff(&self.project).await?;
        }
        let decision = if self.prompt.starts_with("route-ambiguous:") {
            "\ndecision: go"
        } else if self.prompt.starts_with("route-no-match:") {
            "\ndecision: elsewhere"
        } else {
            ""
        };
        let text = format!(
            "## Answer\n{LAST_WORDS}{decision}\n\n## Evidence\nfixture\n\n## Open\nnothing.\n"
        );
        let _ = self
            .events
            .send(
                (AgentEvent::Said {
                    text: LAST_WORDS.to_owned(),
                })
                .into(),
            )
            .await;
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text,
            cost_usd: self.prompt.starts_with("costly:").then_some(COST),
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

async fn remove_source_handoff(project: &Path) -> anyhow::Result<()> {
    let until = Instant::now() + PATIENCE;
    loop {
        let runs = project.join(".loadout/runs");
        if let Ok(run_dirs) = fs::read_dir(runs) {
            for run in run_dirs.flatten() {
                let handoffs = run.path().join("handoffs");
                let Ok(files) = fs::read_dir(handoffs) else {
                    continue;
                };
                for file in files.flatten() {
                    let path = file.path();
                    // 2026-08-28 — publisher zapisuje pełny named temp w tym samym katalogu.
                    // Sabotaż ma usunąć opublikowany wynik Source, nie stan przed commit pointem.
                    if path.extension().is_some_and(|extension| extension == "md")
                        && fs::read_to_string(&path)
                            .is_ok_and(|text| text.contains("\nfrom: Source\n"))
                    {
                        fs::remove_file(path)?;
                        return Ok(());
                    }
                }
            }
        }
        if Instant::now() >= until {
            anyhow::bail!("Source never left the handoff the context fixture must remove");
        }
        tokio::time::sleep(EVERY).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn carry_on_from_each_side_door_leaves_a_failed_handoff() -> Result<(), Box<dyn Error>> {
    // Każdy prompt niżej wskazuje plik pod katalogiem swojej ławki. Nazwane ławki trzymają te
    // katalogi przy życiu aż do asercji, zamiast kasować je na końcu wyrażenia z `.run()`.
    let context_bench = Bench::new()?;
    let context = context_bench
        .run(
            "one-door-context",
            &context_workflow("carry-on"),
            None,
            Intervention::None,
        )
        .await?;
    let prompt = context
        .watch
        .prompt_starting("after-context:")
        .ok_or("the step after the context failure never ran")?;
    let _possibly_empty = failed_handoff_in(&prompt)?;

    let route_bench = Bench::new()?;
    let route = route_bench
        .run(
            "one-door-route",
            &route_workflow("carry-on", false),
            None,
            Intervention::None,
        )
        .await?;
    let prompt = route
        .watch
        .prompt_starting("after-route-left:")
        .ok_or("the step after the blocked route never ran")?;
    let handed = failed_handoff_in(&prompt)?;
    assert!(
        handed.contains(LAST_WORDS),
        "the route failed after the agent had answered, but its handoff lost those last words: \
         {handed:?}"
    );

    let budget_bench = Bench::new()?;
    let budget = budget_bench
        .run(
            "one-door-budget",
            &budget_workflow("carry-on"),
            Some(BUDGET),
            Intervention::None,
        )
        .await?;
    let prompt = budget
        .watch
        .prompt_starting("after-budget:")
        .ok_or("the step after the spent budget never ran")?;
    let _possibly_empty = failed_handoff_in(&prompt)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stop_on_each_side_door_runs_nothing_after_it() -> Result<(), Box<dyn Error>> {
    let cases = [
        ("context", context_workflow("stop"), None, "after-context:"),
        (
            "route",
            route_workflow("stop", false),
            None,
            "after-route-left:",
        ),
        (
            "budget",
            budget_workflow("stop"),
            Some(BUDGET),
            "after-budget:",
        ),
    ];
    for (slug, workflow, budget, after) in cases {
        let result = Bench::new()?
            .run(slug, &workflow, budget, Intervention::None)
            .await?;
        assert!(
            result.watch.prompt_starting(after).is_none(),
            "{slug} was set to stop, but the step after it still started"
        );
    }
    Ok(())
}
