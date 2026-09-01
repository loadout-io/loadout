//! Trzy identyczne typowane porażki narzędzia zatrzymują żywego agenta, zamiast pozwolić mu
//! zużyć resztę tury na tę samą niedostępną zależność.
//!
//! Bezpiecznik celowo nie zna vendora, workflow ani nazwy narzędzia. Czyta istniejący kontrakt
//! `DecodedEvent`: sparowany start z `Action` i pełnym celem, a potem typowany koniec
//! `ok: false` z niepustym pełnym wynikiem. Samo podsumowanie nie jest dowodem.
//!
//! Słaba wersja sprawdzałaby tylko licznik. Ten test przechodzi przez produktową ścieżkę biegu
//! i dowodzi skutków: uchwyt jest anulowany, grupa ma dowód śmierci, krok kończy się porażką,
//! nie anulowaniem, a to samo bezpieczne zdanie dociera do `run.json` i żywego strumienia linii.
//! Kontrole rozcinają podpis i serię: dwa powtórzenia, inny cel, inny albo pusty pełny wynik,
//! sukces pośrodku oraz brak pełnego celu nie zatrzymują zdrowej tury. Normalizujemy wyłącznie
//! różnice końców linii wynikające z transportu; pozostała treść musi być dokładnie ta sama.

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::{REPEATED_TOOL_FAILURE_SENTENCE, run_workflow_inner};
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::line::{Action, Line, Tool};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel};
use loadout_lib::store::Store;
use serde_json::Value as Json;
use tempfile::TempDir;
use tokio::sync::mpsc;

const VENDOR: &str = "claude-code";
const TARGET: &str = "tools browser_navigate";
const OTHER_TARGET: &str = "tools browser_install";
const SAME_ERROR: &str = "browserType.launch: Executable doesn't exist";

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_repeated_tool_failure",
  "name": "Repeated tool failure",
  "steps": [{
    "kind": "agent",
    "id": "worker",
    "name": "Worker",
    "agent": "01990000-0000-7000-8000-0000000000b1",
    "overrides": {},
    "instructions": "Use the tool.",
    "whenItFails": "stop",
    "at": { "x": 0, "y": 0 }
  }],
  "links": []
}"#;

const CARRY_ON_WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_repeated_tool_failure_carries_on",
  "name": "Repeated tool failure carries on",
  "steps": [
    {
      "kind": "agent",
      "id": "worker",
      "name": "Worker",
      "agent": "01990000-0000-7000-8000-0000000000b1",
      "overrides": {},
      "instructions": "Use the tool.",
      "whenItFails": "carry-on",
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "after",
      "name": "After",
      "agent": "01990000-0000-7000-8000-0000000000b1",
      "overrides": {},
      "instructions": "Continue after the failed tool.",
      "whenItFails": "stop",
      "at": { "x": 240, "y": 0 }
    }
  ],
  "links": [{ "from": "worker", "to": "after" }]
}"#;

const AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000b1
name: Worker
summary: Uses a tool
color: slate
runsWith: claude-code
model: test
thinking: balanced
fileAccess: look-only
giveUpAfterMinutes: 20
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Use the tool.
";

#[tokio::test(flavor = "current_thread")]
async fn three_identical_typed_failures_stop_and_explain_the_step() -> Result<(), Box<dyn Error>> {
    let result = run_case(vec![failed(TARGET, SAME_ERROR); 3], true).await?;

    assert_eq!(result.reported, StepState::Failed);
    assert_eq!(
        result.cancels, 1,
        "the breaker must call AgentHandle::cancel exactly once; dropping the wait future leaves \
         the process group alive"
    );
    assert_eq!(
        result.death_proof,
        Some(true),
        "the failed step must record the Dead proof returned by cancellation"
    );
    assert_eq!(
        result.error.as_deref(),
        Some(REPEATED_TOOL_FAILURE_SENTENCE),
        "the durable reason must be the safe product sentence, never raw tool output"
    );
    assert!(
        result.visible.iter().any(|line| matches!(
            line,
            Line::Problem { text, .. } if text == REPEATED_TOOL_FAILURE_SENTENCE
        )),
        "the live view never received the sentence that explains why Loadout stopped the step: \
         {:?}",
        result.visible
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn two_identical_failures_do_not_trip_the_breaker() -> Result<(), Box<dyn Error>> {
    let result = run_case(vec![failed(TARGET, SAME_ERROR); 2], false).await?;
    assert_eq!(result.reported, StepState::Succeeded);
    assert_eq!(result.cancels, 0);
    assert!(result.visible.iter().all(
        |line| !matches!(line, Line::Problem { text, .. } if text == REPEATED_TOOL_FAILURE_SENTENCE)
    ));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn the_third_failure_wins_even_when_the_turn_result_is_already_ready()
-> Result<(), Box<dyn Error>> {
    let result = run_case(vec![failed(TARGET, SAME_ERROR); 3], false).await?;
    assert_eq!(
        result.reported,
        StepState::Failed,
        "Finished is ordered after the tool events; a ready wait result must not overtake the \
         third typed failure already queued before it"
    );
    assert_eq!(result.cancels, 1);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn three_different_full_outputs_do_not_trip_on_the_same_summary() -> Result<(), Box<dyn Error>>
{
    let result = run_case(
        vec![
            failed(TARGET, "attempt one"),
            failed(TARGET, "attempt two"),
            failed(TARGET, "attempt three"),
        ],
        false,
    )
    .await?;
    assert_eq!(result.reported, StepState::Succeeded);
    assert_eq!(
        result.cancels, 0,
        "a breaker using the one-line summary instead of full output stopped three different \
         failures"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn another_target_between_failures_breaks_the_series() -> Result<(), Box<dyn Error>> {
    let result = run_case(
        vec![
            failed(TARGET, SAME_ERROR),
            failed(OTHER_TARGET, SAME_ERROR),
            failed(TARGET, SAME_ERROR),
            failed(TARGET, SAME_ERROR),
        ],
        false,
    )
    .await?;
    assert_eq!(result.reported, StepState::Succeeded);
    assert_eq!(
        result.cancels, 0,
        "fail A, fail B, fail A, fail A is not three identical failures in a row"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn a_success_between_failures_breaks_the_series() -> Result<(), Box<dyn Error>> {
    let result = run_case(
        vec![
            failed(TARGET, SAME_ERROR),
            succeeded(TARGET, "ready"),
            failed(TARGET, SAME_ERROR),
            failed(TARGET, SAME_ERROR),
        ],
        false,
    )
    .await?;
    assert_eq!(result.reported, StepState::Succeeded);
    assert_eq!(
        result.cancels, 0,
        "a successful call resets the repeated-failure series"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn three_empty_failure_outputs_are_not_evidence_of_one_error() -> Result<(), Box<dyn Error>> {
    let result = run_case(vec![failed(TARGET, ""); 3], false).await?;
    assert_eq!(result.reported, StepState::Succeeded);
    assert_eq!(
        result.cancels, 0,
        "an empty full output is not evidence that three failures had the same cause"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn three_failures_without_a_real_target_do_not_form_one_call_series()
-> Result<(), Box<dyn Error>> {
    let result = run_case(vec![failed("   ", SAME_ERROR); 3], false).await?;
    assert_eq!(result.reported, StepState::Succeeded);
    assert_eq!(
        result.cancels, 0,
        "an empty or whitespace-only target is not the full identity of a tool call"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn transport_newlines_do_not_hide_the_same_full_failure() -> Result<(), Box<dyn Error>> {
    let result = run_case(
        vec![
            failed(TARGET, "launch failed\r\nmissing binary\r\n"),
            failed(TARGET, "launch failed\nmissing binary\n"),
            failed(TARGET, "launch failed\rmissing binary"),
        ],
        true,
    )
    .await?;
    assert_eq!(result.reported, StepState::Failed);
    assert_eq!(
        result.cancels, 1,
        "CRLF, CR and trailing newlines are transport differences, not different failures"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn surrounding_spaces_remain_part_of_the_exact_full_output() -> Result<(), Box<dyn Error>> {
    let result = run_case(
        vec![
            failed(TARGET, SAME_ERROR),
            failed(TARGET, " browserType.launch: Executable doesn't exist"),
            failed(TARGET, SAME_ERROR),
        ],
        false,
    )
    .await?;
    assert_eq!(result.reported, StepState::Succeeded);
    assert_eq!(
        result.cancels, 0,
        "only transport newlines may be normalized; meaningful spaces keep outputs distinct"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn the_breaker_uses_the_steps_existing_carry_on_policy() -> Result<(), Box<dyn Error>> {
    let result =
        run_case_with(CARRY_ON_WORKFLOW, vec![failed(TARGET, SAME_ERROR); 3], true).await?;
    assert_eq!(
        result.steps,
        vec![StepState::Failed, StepState::Succeeded],
        "the breaker must fail its own step and then use ordinary whenItFails: carry-on; it must \
         not synthesize a graph verdict or cancel the whole run"
    );
    assert_eq!(
        result.starts, 2,
        "the child never reached its real driver start"
    );
    assert_eq!(
        result.cancels, 1,
        "only the looping parent needed cancellation"
    );
    assert!(
        result.visible.iter().any(|line| matches!(
            line,
            Line::StepCarriedOn { step_id, .. } if step_id == "worker"
        )),
        "the ordinary carry-on result never emitted its explicit stepCarriedOn fact: {:?}",
        result.visible
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn an_alive_group_does_not_hold_the_forward_pump_open_and_remains_owned()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let watch = Arc::new(Watch::default());
    let processes = Arc::new(loadout_lib::commands::processes::Processes::new());
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers_with(
            vec![failed(TARGET, SAME_ERROR); 3],
            true,
            true,
            true,
            Arc::clone(&watch),
        ),
        processes: Arc::clone(&processes),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: bench.workflow(WORKFLOW)?,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (sink, _source) = line_channel(QUEUE_CAP);
    let report = tokio::time::timeout(
        Duration::from_secs(5),
        run_workflow_inner(&deps, &request, sink),
    )
    .await
    .map_err(|_| "the run waited for EOF from the sender retained by the live process handle")??;
    assert_eq!(report.steps, vec![StepState::Failed]);
    assert_eq!(
        watch.cancels(),
        3,
        "the bounded live Stop did not make all three attempts"
    );

    let receipt: Json = serde_json::from_str(&fs::read_to_string(report.dir.join("run.json"))?)?;
    let step = receipt
        .get("steps")
        .and_then(Json::as_array)
        .and_then(|steps| steps.first())
        .ok_or("run.json has no first step")?;
    assert_eq!(step.get("death_proof").and_then(Json::as_bool), Some(false));
    assert_eq!(
        step.get("error").and_then(Json::as_str),
        Some("This agent survived Loadout's three attempts to stop it and may still be running.")
    );

    let proofs = tokio::time::timeout(Duration::from_secs(1), processes.close())
        .await
        .map_err(|_| "the retained live handle could not be asked for proof during shutdown")?;
    assert_eq!(
        proofs.len(),
        1,
        "the live handle was dropped instead of retained exactly once"
    );
    assert!(matches!(proofs.first(), Some(GroupProof::Dead { .. })));
    assert_eq!(
        watch.proof_checks(),
        1,
        "Processes::close never reached the handle retained after the Alive proof"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn explicit_stop_also_closes_the_pump_and_retains_an_alive_group()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let watch = Arc::new(Watch::default());
    let processes = Arc::new(loadout_lib::commands::processes::Processes::new());
    let control = RunControl::new();
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers_with(Vec::new(), true, true, true, Arc::clone(&watch)),
        processes: Arc::clone(&processes),
        control: control.clone(),
    };
    let request = RunRequest {
        workflow: bench.workflow(WORKFLOW)?,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (sink, _source) = line_channel(QUEUE_CAP);
    let running = run_workflow_inner(&deps, &request, sink);
    let stopping = async {
        tokio::time::timeout(Duration::from_secs(1), async {
            while watch.starts() == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .map_err(|_| "the fake agent never started before Stop")?;
        control.stop();
        Ok::<(), Box<dyn Error>>(())
    };
    let (ran, stopped) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(running, stopping)
    })
    .await
    .map_err(|_| "explicit Stop waited for EOF from the sender retained by the live handle")?;
    stopped?;
    let report = ran?;
    assert_eq!(report.steps, vec![StepState::Failed]);
    assert_eq!(
        watch.cancels(),
        3,
        "explicit Stop did not complete the bounded three proof attempts"
    );

    let receipt: Json = serde_json::from_str(&fs::read_to_string(report.dir.join("run.json"))?)?;
    let step = receipt
        .get("steps")
        .and_then(Json::as_array)
        .and_then(|steps| steps.first())
        .ok_or("run.json has no first step")?;
    assert_eq!(step.get("death_proof").and_then(Json::as_bool), Some(false));
    assert_eq!(
        step.get("error").and_then(Json::as_str),
        Some("This agent survived Loadout's three attempts to stop it and may still be running.")
    );

    let proofs = tokio::time::timeout(Duration::from_secs(1), processes.close())
        .await
        .map_err(|_| "shutdown could not revisit the live handle retained after explicit Stop")?;
    assert_eq!(
        proofs.len(),
        1,
        "explicit Stop dropped the live handle instead of retaining it"
    );
    assert!(matches!(proofs.first(), Some(GroupProof::Dead { .. })));
    assert_eq!(
        watch.proof_checks(),
        1,
        "Processes::close never reached the handle retained after explicit Stop"
    );
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct Call {
    target: &'static str,
    output: &'static str,
    ok: bool,
}

const fn failed(target: &'static str, output: &'static str) -> Call {
    Call {
        target,
        output,
        ok: false,
    }
}

const fn succeeded(target: &'static str, output: &'static str) -> Call {
    Call {
        target,
        output,
        ok: true,
    }
}

struct ResultOfCase {
    reported: StepState,
    steps: Vec<StepState>,
    starts: usize,
    cancels: usize,
    death_proof: Option<bool>,
    error: Option<String>,
    visible: Vec<Line>,
}

async fn run_case(calls: Vec<Call>, hang: bool) -> Result<ResultOfCase, Box<dyn Error>> {
    run_case_with(WORKFLOW, calls, hang).await
}

async fn run_case_with(
    workflow: &str,
    calls: Vec<Call>,
    hang: bool,
) -> Result<ResultOfCase, Box<dyn Error>> {
    let bench = Bench::new()?;
    let watch = Arc::new(Watch::default());
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(calls, hang, Arc::clone(&watch)),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: bench.workflow(workflow)?,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (sink, mut source) = line_channel(QUEUE_CAP);
    let report = tokio::time::timeout(
        Duration::from_secs(5),
        run_workflow_inner(&deps, &request, sink),
    )
    .await
    .map_err(|_| "the repeated-failure run did not stop within five seconds")??;

    let mut visible = Vec::new();
    while let Some(line) = source.try_next() {
        visible.push(line);
    }
    let receipt: Json = serde_json::from_str(&fs::read_to_string(report.dir.join("run.json"))?)?;
    let step = receipt
        .get("steps")
        .and_then(Json::as_array)
        .and_then(|steps| steps.first())
        .ok_or("run.json has no first step")?;

    let reported = report.steps[0];
    Ok(ResultOfCase {
        reported,
        steps: report.steps,
        starts: watch.starts(),
        cancels: watch.cancels(),
        death_proof: step.get("death_proof").and_then(Json::as_bool),
        error: step.get("error").and_then(Json::as_str).map(str::to_owned),
        visible,
    })
}

#[derive(Debug, Default)]
struct Watch {
    cancels: Mutex<usize>,
    starts: Mutex<usize>,
    proof_checks: Mutex<usize>,
}

impl Watch {
    fn cancelled(&self) {
        *self.cancels.lock().unwrap_or_else(PoisonError::into_inner) += 1;
    }

    fn cancels(&self) -> usize {
        *self.cancels.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Fizyczny licznik startów od jedynki; tylko pierwszy start dostaje scenariusz porażek.
    fn started(&self) -> usize {
        let mut starts = self.starts.lock().unwrap_or_else(PoisonError::into_inner);
        *starts += 1;
        *starts
    }

    fn starts(&self) -> usize {
        *self.starts.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn proved(&self) {
        *self
            .proof_checks
            .lock()
            .unwrap_or_else(PoisonError::into_inner) += 1;
    }

    fn proof_checks(&self) -> usize {
        *self
            .proof_checks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

fn fake_drivers(calls: Vec<Call>, hang: bool, watch: Arc<Watch>) -> Drivers {
    fake_drivers_with(calls, hang, false, false, watch)
}

fn fake_drivers_with(
    calls: Vec<Call>,
    hang: bool,
    retains_sender: bool,
    cancel_alive: bool,
    watch: Arc<Watch>,
) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        calls,
        hang,
        retains_sender,
        cancel_alive,
        watch,
    });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct Fake {
    calls: Vec<Call>,
    hang: bool,
    retains_sender: bool,
    cancel_alive: bool,
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
        let first = self.watch.started() == 1;
        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };
        for (at, call) in self.calls.iter().enumerate().filter(|_| first) {
            let id = format!("call_{at}");
            events
                .send(DecodedEvent {
                    event: AgentEvent::ToolStart {
                        id: id.clone(),
                        label: "Using a connected tool".to_owned(),
                    },
                    tool: Some(Tool::Started {
                        action: Action::Ran,
                        target: call.target.to_owned(),
                    }),
                })
                .await?;
            events
                .send(DecodedEvent {
                    event: AgentEvent::ToolEnd {
                        id,
                        ok: call.ok,
                        // Każde wywołanie celowo ma to samo podsumowanie. Kontrola różnych
                        // pełnych wyników czerwienieje, jeśli produkcja omyłkowo podpisze to pole.
                        summary: "The connected tool failed".to_owned(),
                    },
                    tool: Some(Tool::Ended {
                        output: call.output.to_owned(),
                    }),
                })
                .await?;
        }
        let hang = self.hang && first;
        if !hang {
            // Kontrakt sterownika mówi dokładnie jeden `Finished` na turę. Nadajnik jest FIFO,
            // więc potwierdzenie tego zdarzenia przez pompę dowodzi też, że wszystkie wcześniejsze
            // końce narzędzi zostały już policzone, nawet gdy `wait()` jest gotowe od razu.
            events
                .send(DecodedEvent {
                    event: AgentEvent::Finished(completed(&session)),
                    tool: None,
                })
                .await?;
        }
        Ok(Box::new(Turn {
            hang,
            session,
            watch: Arc::clone(&self.watch),
            events: self.retains_sender.then_some(events),
            cancel_alive: self.cancel_alive,
        }))
    }
}

#[derive(Debug)]
struct Turn {
    hang: bool,
    session: SessionRef,
    watch: Arc<Watch>,
    events: Option<mpsc::Sender<DecodedEvent>>,
    cancel_alive: bool,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        Some(GroupId {
            pid: 4242,
            pgid: 4242,
        })
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        if self.hang {
            std::future::pending::<()>().await;
        }
        Ok(completed(&self.session))
    }

    async fn cancel(&mut self) -> GroupProof {
        self.watch.cancelled();
        if self.cancel_alive {
            GroupProof::Alive {
                group: self.group(),
            }
        } else {
            GroupProof::Dead { status: None }
        }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }

    async fn proof_of_death(&mut self) -> GroupProof {
        self.watch.proved();
        self.events.take();
        GroupProof::Dead { status: None }
    }
}

fn completed(session: &SessionRef) -> TurnOutcome {
    TurnOutcome {
        ok: true,
        reason: FinishReason::Completed,
        text: "done".to_owned(),
        cost_usd: None,
        tokens: Tokens::default(),
        turns: 1,
        took: Duration::ZERO,
        session: session.clone(),
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
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(home.path().join("agents").join("worker.md"), AGENT)?;
        Ok(Self { home, project })
    }

    fn workflow(&self, workflow: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.home.path().join("workflows").join("repeated.json");
        fs::write(&path, workflow)?;
        Ok(path)
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }
}
