//! Z-31: Lab keeps paid proposals bounded and merges them with a person's live decision.

// Kryteria wolno pisać `expect()` i `panic!`, a kod produkcyjny nie (`Cargo.toml`,
// `AGENTS.md` §4). Panika w teście JEST jego wynikiem.
#![allow(clippy::expect_used, clippy::panic)]

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::Drivers;
use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::commands::lab::{
    Proposing, apply_fix_inner, create_set_inner, decide_case_inner, propose_cases_inner,
    propose_fix_inner, put_case_inner, read_set_inner, workflow_id_for,
};
use loadout_lib::commands::settings::save_settings_inner;
use loadout_lib::durable_file::{
    FaultAction, FaultInjector, PublicationEvent, RecoveryEvent, RecoveryPoint, scoped_faults,
};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::lab::file::path_for;
use loadout_lib::lab::plan::{Half, key_for};
use loadout_lib::lab::{Case, CaseStatus, Expect, Subject};
use loadout_lib::library::agents::Agent;
use serde_json::{Map, Value, json};
use tempfile::TempDir;
use tokio::sync::mpsc;

const CASE_ANSWER: &str = "## Case\n\
name: Covers the paid proposal\n\
task: keep a paid suggestion after a live decision\n\
because: src/state.rs:31\n\
expect: answer = kept\n";

const FIX_ANSWER: &str = "## Why\n\
The old instructions missed the failing branch.\n\n\
## Instructions\n\
Read the branch and keep its result.\n";

#[test]
fn apply_without_a_proposal_revision_uses_the_agent_that_is_on_disk() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new(75.0)?;

    let revision = apply_fix_inner(
        bench.library.path(),
        &bench.agent_id,
        "Use the current instructions, then apply this fix.".to_owned(),
        None,
    )?;

    let definitions =
        loadout_lib::commands::agents::list_agent_definitions_inner(bench.library.path())?;
    let saved = definitions
        .into_iter()
        .find_map(|definition| match definition {
            loadout_lib::library::definition::Definition::Healthy { value, revision }
                if value.id.to_string() == bench.agent_id =>
            {
                Some((value, revision))
            }
            _ => None,
        });
    let (saved, on_disk) = saved.ok_or("the existing agent disappeared after Apply")?;
    assert_eq!(revision, on_disk);
    assert_eq!(
        saved.instructions,
        "Use the current instructions, then apply this fix."
    );
    Ok(())
}

#[tokio::test]
async fn both_paid_lab_turns_use_one_tenth_of_the_default_limit() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new(42.0)?;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let drivers = drivers(Fake::new(Arc::clone(&seen), true, None));
    let proposing = Proposing::new();

    propose_cases_inner(
        bench.library.path(),
        &drivers,
        &proposing,
        bench.project.path(),
        &bench.set_id,
        &bench.agent_id,
    )
    .await?;
    bench.add_case("measured", CaseStatus::InUse)?;
    bench.write_failed_run("measured")?;
    propose_fix_inner(
        bench.library.path(),
        &drivers,
        &proposing,
        bench.project.path(),
        &bench.set_id,
        &bench.agent_id,
    )
    .await?;

    assert_eq!(*lock(&seen), vec![4.2, 4.2]);
    Ok(())
}

#[tokio::test]
async fn a_driver_without_a_spending_limit_refuses_before_start() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new(75.0)?;
    let starts = Arc::new(AtomicUsize::new(0));
    let driver = CannotCarryBudget {
        starts: Arc::clone(&starts),
    };
    let drivers: Drivers = Arc::new(move |_| Arc::new(driver.clone()));

    let refused = propose_cases_inner(
        bench.library.path(),
        &drivers,
        &Proposing::new(),
        bench.project.path(),
        &bench.set_id,
        &bench.agent_id,
    )
    .await
    .expect_err("a paid turn without a hard spending limit started");

    assert!(
        refused.contains("spending limit"),
        "the refusal said: {refused}"
    );
    assert_eq!(starts.load(Ordering::Acquire), 0);
    Ok(())
}

#[tokio::test]
async fn a_case_decision_during_a_paid_proposal_keeps_the_decision_and_the_new_candidates()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new(75.0)?;
    bench.add_case("person-kept", CaseStatus::Suggested)?;
    let decision = DecisionDuringTurn {
        set_id: bench.set_id.clone(),
        case_id: "person-kept".to_owned(),
        made: Arc::new(AtomicBool::new(false)),
    };
    let drivers = drivers(Fake::new(
        Arc::new(Mutex::new(Vec::new())),
        false,
        Some(decision),
    ));

    let proposed = propose_cases_inner(
        bench.library.path(),
        &drivers,
        &Proposing::new(),
        bench.project.path(),
        &bench.set_id,
        &bench.agent_id,
    )
    .await?;

    let fresh = read_set_inner(bench.project.path(), &bench.set_id)?;
    assert_eq!(proposed.written, 1);
    assert_eq!(
        fresh
            .set
            .cases
            .iter()
            .find(|case| case.id == "person-kept")
            .map(|case| case.status),
        Some(CaseStatus::InUse),
        "retrying the paid proposal undid the person's Accept"
    );
    assert!(
        fresh
            .set
            .cases
            .iter()
            .any(|case| case.name == "Covers the paid proposal"),
        "the paid candidate disappeared when Accept changed the same file"
    );
    Ok(())
}

#[tokio::test]
async fn a_second_candidate_conflict_is_refused_without_overwriting_it()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new(75.0)?;
    let open = bench.add_case("person-kept", CaseStatus::Suggested)?;
    let mut changed_again = open.set;
    changed_again.cases[0].status = CaseStatus::InUse;
    changed_again
        .extra
        .insert("changedAgain".to_owned(), Value::Bool(true));
    let target = path_for(bench.project.path(), &bench.set_id);
    let root = target.parent().ok_or("the set file has no parent")?;
    let _faults = scoped_faults(
        root,
        Arc::new(ChangeAtRetry {
            target: target.clone(),
            replacement: set_bytes(&changed_again)?,
            changed: AtomicBool::new(false),
        }),
    )?;
    let decision = DecisionDuringTurn {
        set_id: bench.set_id.clone(),
        case_id: "person-kept".to_owned(),
        made: Arc::new(AtomicBool::new(false)),
    };
    let drivers = drivers(Fake::new(
        Arc::new(Mutex::new(Vec::new())),
        false,
        Some(decision),
    ));

    let refused = propose_cases_inner(
        bench.library.path(),
        &drivers,
        &Proposing::new(),
        bench.project.path(),
        &bench.set_id,
        &bench.agent_id,
    )
    .await
    .expect_err("the second conflict was overwritten or retried without a bound");

    assert!(
        refused.contains("changed on disk"),
        "the refusal said: {refused}"
    );
    let fresh = read_set_inner(bench.project.path(), &bench.set_id)?;
    assert_eq!(
        fresh.set.extra.get("changedAgain"),
        Some(&Value::Bool(true))
    );
    assert!(
        fresh
            .set
            .cases
            .iter()
            .all(|case| case.name != "Covers the paid proposal"),
        "the proposal overwrote the second concurrent change"
    );
    Ok(())
}

struct Bench {
    library: TempDir,
    project: TempDir,
    agent_id: String,
    set_id: String,
}

impl Bench {
    fn new(default_budget: f64) -> Result<Self, Box<dyn Error>> {
        let library = TempDir::new()?;
        let project = TempDir::new()?;
        let mut agent = Agent::example();
        "Z31 Writer".clone_into(&mut agent.name);
        "Writes Lab proposals".clone_into(&mut agent.summary);
        let agent_id = agent.id.to_string();
        save_agent_inner(library.path(), &agent, None)?;
        save_settings_inner(library.path(), "", default_budget, false, 0, true)?;
        let open = create_set_inner(
            project.path(),
            "Z31 paid proposals",
            &Subject::Agent {
                id: agent_id.clone(),
            },
            &agent_id,
        )?;
        Ok(Self {
            library,
            project,
            agent_id,
            set_id: open.set.id,
        })
    }

    fn add_case(
        &self,
        id: &str,
        status: CaseStatus,
    ) -> Result<loadout_lib::commands::lab::OpenSet, Box<dyn Error>> {
        let open = read_set_inner(self.project.path(), &self.set_id)?;
        Ok(put_case_inner(
            self.project.path(),
            &self.set_id,
            measured_case(id, status),
            Some(&open.revision),
        )?)
    }

    fn write_failed_run(&self, case_id: &str) -> Result<(), Box<dyn Error>> {
        let run = self
            .project
            .path()
            .join(".loadout/runs/20260904-120000__z31");
        fs::create_dir_all(&run)?;
        let description = json!({
            "workflow_id": workflow_id_for(&self.set_id),
            "status": "failed",
            "steps": [{
                "node_key": key_for(case_id, "as-it-is", Half::Work),
                "name": "Measured case",
                "status": "failed",
                "error": "The old answer was wrong."
            }]
        });
        fs::write(
            run.join("run.json"),
            serde_json::to_vec_pretty(&description)?,
        )?;
        Ok(())
    }
}

fn measured_case(id: &str, status: CaseStatus) -> Case {
    Case {
        id: id.to_owned(),
        name: format!("Measured {id}"),
        task: "Return the measured answer.".to_owned(),
        expect: vec![Expect {
            field: "answer".to_owned(),
            contains: "kept".to_owned(),
            describe: "the answer".to_owned(),
        }],
        command: String::new(),
        proof: String::new(),
        status,
        because: "src/state.rs:12".to_owned(),
        extra: Map::new(),
    }
}

fn drivers(fake: Fake) -> Drivers {
    Arc::new(move |_| Arc::new(fake.clone()))
}

#[derive(Clone)]
struct DecisionDuringTurn {
    set_id: String,
    case_id: String,
    made: Arc<AtomicBool>,
}

#[derive(Clone)]
struct Fake {
    seen: Arc<Mutex<Vec<f64>>>,
    requires_budget: bool,
    budget: Option<f64>,
    decision: Option<DecisionDuringTurn>,
}

impl Fake {
    fn new(
        seen: Arc<Mutex<Vec<f64>>>,
        requires_budget: bool,
        decision: Option<DecisionDuringTurn>,
    ) -> Self {
        Self {
            seen,
            requires_budget,
            budget: None,
            decision,
        }
    }
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        "z31-fake"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("z31".to_owned()),
        })
    }

    fn with_budget(&self, dollars: f64) -> Option<Arc<dyn AgentDriver>> {
        let mut clone = self.clone();
        clone.budget = Some(dollars);
        Some(Arc::new(clone))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        if self.requires_budget && self.budget.is_none() {
            return Err(anyhow::anyhow!(
                "the paid turn started without its spending limit"
            ));
        }
        if let Some(budget) = self.budget {
            lock(&self.seen).push(budget);
        }
        if let Some(decision) = &self.decision
            && decision
                .made
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            decide_case_inner(&spec.cwd, &decision.set_id, &decision.case_id, true, None)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        }
        let answer = if spec.prompt.contains("Write one block for each case") {
            CASE_ANSWER
        } else {
            FIX_ANSWER
        };
        let session = SessionRef {
            vendor: "z31-fake",
            id: spec.run_id.to_string(),
        };
        Ok(Box::new(Turn {
            events,
            session,
            answer: answer.to_owned(),
        }))
    }
}

#[derive(Clone)]
struct CannotCarryBudget {
    starts: Arc<AtomicUsize>,
}

#[async_trait]
impl AgentDriver for CannotCarryBudget {
    fn id(&self) -> &'static str {
        "z31-no-budget"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: None,
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.starts.fetch_add(1, Ordering::AcqRel);
        let session = SessionRef {
            vendor: "z31-no-budget",
            id: spec.run_id.to_string(),
        };
        Ok(Box::new(Turn {
            events,
            session,
            answer: CASE_ANSWER.to_owned(),
        }))
    }
}

struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    answer: String,
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
            text: self.answer.clone(),
            cost_usd: Some(0.01),
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
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

struct ChangeAtRetry {
    target: PathBuf,
    replacement: Vec<u8>,
    changed: AtomicBool,
}

impl FaultInjector for ChangeAtRetry {
    fn action(&self, _event: &PublicationEvent) -> FaultAction {
        FaultAction::Continue
    }

    fn recovery_action(&self, event: &RecoveryEvent) -> FaultAction {
        if event.point == RecoveryPoint::BeforeLock && !self.changed.swap(true, Ordering::AcqRel) {
            fs::write(&self.target, &self.replacement)
                .expect("the second concurrent set change must reach the fault fixture");
        }
        FaultAction::Continue
    }
}

fn set_bytes(set: &loadout_lib::lab::EvalSet) -> Result<Vec<u8>, serde_json::Error> {
    let mut bytes = serde_json::to_vec_pretty(set)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
