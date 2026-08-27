//! Wspolny fixture T-149: ten sam graf i te same oczekiwania dla oracle offline i live.
//!
//! Plik jest dolaczany jawnie przez oba standalone targety. Nie ma `support/mod.rs`, bo wtedy
//! wspolna lawka stalaby sie niezamierzonym trzecim targetem albo globalnym miejscem na kolejne
//! wyjatki.

use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fs;
use std::fs::OpenOptions;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::commands::run::{run_workflow_with_reflection, stop_run_inner};
use loadout_lib::commands::workflows::save_workflow_inner;
use loadout_lib::commands::{Drivers, Outcome, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, StepSettings, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::{Agent, FileAccess, Vendor};
use loadout_lib::store::Store;
use loadout_lib::workflow::{
    AgentStep, Borrow, Folder, Handover, Link, Point, Skills, Step, WhenItFails, WorkflowFile,
};
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;
use uuid::Uuid;

pub const MAX_COST_USD: f64 = 8.0;
pub const MAX_TURNS: u8 = 3;
pub const PAID_ENV: &str = "LOADOUT_PAID_ORACLE";
pub const PAID_VALUE: &str = "phase7";
pub const CODEX_MODEL: &str = "gpt-5.6-terra";
pub const SENTINEL: &str = "T149-UPSTREAM-HANDOFF-ONLY-6C7E";

const PATIENCE: Duration = Duration::from_secs(30);
const LIVE_PATIENCE: Duration = Duration::from_mins(20);
const WRITER_ID: Uuid = Uuid::from_u128(0x0199_0000_0000_7000_8000_0000_0000_0148);
const JUDGE_ID: Uuid = Uuid::from_u128(0x0199_0000_0000_7000_8000_0000_0000_0149);

const PLAN: &str = "plan: hand a concrete plan to the worker. Include the exact marker T149-UPSTREAM-HANDOFF-ONLY-6C7E in your Answer.";
const WORK: &str = "work: read every indexed handoff before answering. Copy the upstream marker into your Answer. Create or update phase7-oracle.txt with one short line proving this turn edited the fresh copy.";
const JUDGE: &str = "judge: read every indexed handoff. On the first try return outcome: fail. When the index contains the earlier Judge decision and the second Work answer, return outcome: pass. Always place the outcome field on its own line.";
const SYNTHESIS: &str = "synthesis: read every indexed handoff and combine the Work answer and Judge decision from the round that passed.";
const REFLECTION: &str = "rule: T149 keeps the oracle grounded\n\
because: the completed phase-7 handoff is nonempty and measurable\n";

pub type TestResult<T = ()> = Result<T, Box<dyn Error>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Assignment {
    pub writer: Vendor,
    pub judge: Vendor,
}

#[must_use]
pub const fn assignments() -> [Assignment; 2] {
    [
        Assignment {
            writer: Vendor::ClaudeCode,
            judge: Vendor::Codex,
        },
        Assignment {
            writer: Vendor::Codex,
            judge: Vendor::ClaudeCode,
        },
    ]
}

#[derive(Debug)]
pub struct RoundEvidence {
    pub number: u8,
    pub work_started: bool,
    pub judge_started: bool,
    pub outcome: Option<String>,
    pub artifacts: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct CoreEvidence {
    pub assignment: Assignment,
    pub expanded_steps: Vec<String>,
    pub max_turns: u8,
    pub rounds: Vec<RoundEvidence>,
    pub work_round_two_index: Vec<String>,
    pub work_round_two_paths: Vec<PathBuf>,
    pub synthesis_index: Vec<String>,
    pub synthesis_paths: Vec<PathBuf>,
    pub sentinel_source: PathBuf,
    pub sentinel_read_from: PathBuf,
    pub downstream_handoff: String,
    pub run_json: serde_json::Value,
    pub vendor_starts: Vec<(String, Vendor)>,
    pub reflection_wrapped: bool,
    pub reflection_saw_nonempty_handoff: bool,
}

#[derive(Debug)]
pub struct BudgetEvidence {
    pub limit_usd: f64,
    pub spent_usd: f64,
    pub crossed_after: String,
    pub started_after_crossing: Vec<String>,
}

#[derive(Debug)]
pub struct OfflineEvidence {
    pub core: CoreEvidence,
    pub budget: BudgetEvidence,
}

#[derive(Debug)]
pub struct CostEvidence {
    pub claude_usd: f64,
    pub codex_usd: f64,
    pub reflection_usd: f64,
    pub codex_model: String,
    pub codex_input_tokens: u64,
    pub codex_output_tokens: u64,
    pub codex_is_estimate: bool,
    pub largest_turn_usd: f64,
}

#[derive(Debug)]
pub struct ReflectionEvidence {
    pub ran: bool,
    pub saw_nonempty_handoff: bool,
    pub kept: usize,
    pub receipt_in_run_json: bool,
}

#[derive(Debug)]
pub struct HostStateEvidence {
    pub exclusive: bool,
    pub scanned_processes: usize,
    pub claude_json_before: Option<Vec<u8>>,
    pub claude_json_after: Option<Vec<u8>>,
    pub claude_projects_before: Option<Vec<u8>>,
    pub claude_projects_after: Option<Vec<u8>>,
    pub private_state_under_run: bool,
}

#[derive(Debug)]
pub struct StopEvidence {
    pub stopped_groups: usize,
    pub groups_proven_dead: usize,
    pub registered_worktrees_after: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct GitEvidence {
    pub base: String,
    pub branch: String,
    pub changed_files: Vec<String>,
    pub diff: String,
}

#[derive(Debug)]
pub struct LiveEvidence {
    pub core: CoreEvidence,
    pub costs: CostEvidence,
    pub reflection: ReflectionEvidence,
    pub host: HostStateEvidence,
    pub stop: StopEvidence,
    pub git: GitEvidence,
    pub final_success: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiveCallKind {
    Step,
    Reflection,
}

#[derive(Debug, Clone)]
struct LiveStart {
    role: String,
    turn: usize,
    vendor: Vendor,
    model: Option<String>,
    prompt: String,
    cwd: PathBuf,
    call_kind: LiveCallKind,
}

#[derive(Debug, Default)]
struct LiveWatch(Mutex<Vec<LiveStart>>);

impl LiveWatch {
    fn record(&self, mut start: LiveStart) {
        let mut starts = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        start.turn = starts
            .iter()
            .filter(|seen| seen.role == start.role && seen.call_kind == start.call_kind)
            .count()
            + 1;
        starts.push(start);
    }

    fn snapshot(&self) -> Vec<LiveStart> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

pub fn assert_core_contract(evidence: &CoreEvidence, assignment: Assignment) {
    assert_eq!(
        evidence.assignment, assignment,
        "the run used the wrong vendor assignment"
    );
    assert_eq!(
        evidence.expanded_steps,
        [
            "plan",
            "work-1",
            "work-2",
            "work-3",
            "judge-1",
            "judge-2",
            "judge-3",
            "synthesis",
        ],
        "the production run did not expand the shared three-turn graph"
    );
    assert_eq!(
        evidence.max_turns, MAX_TURNS,
        "the back edge lost the three-turn ceiling"
    );
    assert_rounds(&evidence.rounds);
    assert_prompt_indexes(evidence);
    assert_sentinel(evidence);
    let expected = [
        ("plan", assignment.writer),
        ("work", assignment.writer),
        ("work", assignment.writer),
        ("judge", assignment.judge),
        ("judge", assignment.judge),
        ("synthesis", assignment.writer),
    ];
    for wanted in expected {
        assert!(
            evidence
                .vendor_starts
                .contains(&(wanted.0.to_owned(), wanted.1)),
            "the factory did not route {} through {:?}: {:?}",
            wanted.0,
            wanted.1,
            evidence.vendor_starts
        );
    }
    assert!(
        evidence.reflection_wrapped,
        "reflection skipped one of its hard wrappers"
    );
    assert!(
        evidence.reflection_saw_nonempty_handoff,
        "reflection started without observing a real nonempty handoff"
    );
}

fn assert_rounds(rounds: &[RoundEvidence]) {
    assert_eq!(rounds.len(), usize::from(MAX_TURNS));
    assert_eq!(rounds[0].number, 1);
    assert!(rounds[0].work_started && rounds[0].judge_started);
    assert_eq!(rounds[0].outcome.as_deref(), Some("fail"));
    assert_eq!(rounds[1].number, 2);
    assert!(rounds[1].work_started && rounds[1].judge_started);
    assert_eq!(rounds[1].outcome.as_deref(), Some("pass"));
    assert_eq!(rounds[2].number, 3);
    assert!(
        !rounds[2].work_started && !rounds[2].judge_started,
        "third round unexpectedly started: {rounds:?}"
    );
    assert!(rounds[2].outcome.is_none());
    assert!(
        rounds[2].artifacts.is_empty(),
        "a skipped third round left files behind: {:?}",
        rounds[2].artifacts
    );
}

fn assert_prompt_indexes(evidence: &CoreEvidence) {
    assert_eq!(
        evidence.work_round_two_index,
        ["plan", "work-1", "judge-1"],
        "round two must receive the loop input and its own stable history"
    );
    assert!(
        evidence
            .work_round_two_paths
            .iter()
            .all(|path| path.is_absolute())
    );
    assert_eq!(
        evidence.synthesis_index,
        ["work-2", "judge-2"],
        "synthesis must receive the round that passed, not a fictional third round"
    );
    assert!(
        evidence
            .synthesis_paths
            .iter()
            .all(|path| path.is_absolute())
    );
}

fn assert_sentinel(evidence: &CoreEvidence) {
    assert!(evidence.sentinel_source.is_absolute());
    assert_eq!(
        evidence.sentinel_read_from, evidence.sentinel_source,
        "downstream did not read the exact absolute upstream handoff path"
    );
    assert!(
        evidence.downstream_handoff.contains(SENTINEL),
        "the value read from the upstream handoff did not reach downstream's real handoff"
    );
}

pub fn assert_budget_contract(evidence: &BudgetEvidence) {
    assert!((evidence.limit_usd - MAX_COST_USD).abs() < f64::EPSILON);
    assert!(
        evidence.spent_usd > evidence.limit_usd,
        "the probe must cross the budget before it can prove the next step is refused"
    );
    assert!(!evidence.crossed_after.is_empty());
    assert!(
        evidence.started_after_crossing.is_empty(),
        "the next work step started after the recorded budget crossing: {:?}",
        evidence.started_after_crossing
    );
}

pub fn assert_live_contract(evidence: &LiveEvidence, assignment: Assignment) {
    assert_core_contract(&evidence.core, assignment);
    assert_costs(&evidence.costs);
    assert_reflection(&evidence.reflection);
    assert_host_state(&evidence.host);
    assert_stop_and_git(&evidence.stop, &evidence.git);
    assert!(
        evidence.final_success,
        "run.json did not record final success"
    );
    assert_eq!(
        evidence
            .core
            .run_json
            .get("budget_usd")
            .and_then(serde_json::Value::as_f64),
        Some(MAX_COST_USD),
        "the live run.json lost its soft budget"
    );
}

fn assert_costs(costs: &CostEvidence) {
    assert!(costs.claude_usd > 0.0);
    assert!(costs.codex_usd > 0.0);
    assert!(costs.reflection_usd > 0.0);
    assert_eq!(costs.codex_model, CODEX_MODEL);
    assert!(costs.codex_input_tokens > 0 && costs.codex_output_tokens > 0);
    assert!(costs.codex_is_estimate);
    // Jedna tura Codeksa moze przekroczyc granice: zapisujemy koszt, nie udajemy twardego capu.
    assert!(costs.largest_turn_usd > 0.0);
}

fn assert_reflection(reflection: &ReflectionEvidence) {
    assert!(reflection.ran, "the production reflection did not run");
    assert!(
        reflection.saw_nonempty_handoff,
        "reflection was marked as run without any handoff to reflect on"
    );
    assert!(
        reflection.kept <= 3,
        "reflection kept more than three notes"
    );
    assert!(reflection.receipt_in_run_json);
}

fn assert_host_state(host: &HostStateEvidence) {
    assert!(
        host.exclusive,
        "the oracle ran without exclusive host-state fingerprinting"
    );
    assert!(
        host.scanned_processes > 0,
        "the exclusivity preflight did not inspect the host process table"
    );
    assert_eq!(host.claude_json_after, host.claude_json_before);
    assert_eq!(host.claude_projects_after, host.claude_projects_before);
    assert!(host.private_state_under_run);
}

fn assert_stop_and_git(stop: &StopEvidence, git: &GitEvidence) {
    assert_eq!(
        stop.groups_proven_dead, stop.stopped_groups,
        "at least one stopped process group lacks death proof"
    );
    assert!(stop.registered_worktrees_after.is_empty());
    assert!(!git.base.is_empty() && !git.branch.is_empty());
    assert!(!git.changed_files.is_empty());
    assert!(!git.diff.trim().is_empty());
}

pub async fn run_offline_oracle(assignment: Assignment) -> TestResult<OfflineEvidence> {
    let core = run_fake_graph(assignment, 0.25, true).await?.0;
    let (budget_run, started) = run_budget_probe(assignment).await?;
    let spent_usd = budget_run
        .get("spent_usd")
        .and_then(serde_json::Value::as_f64)
        .ok_or("the budget probe has no durable spend")?;
    let crossed_after = started
        .first()
        .map(|call| call.role.clone())
        .ok_or("the budget probe started no step")?;
    let started_after_crossing = started
        .iter()
        .skip(1)
        .filter(|call| call.role == "work")
        .map(|call| call.role.clone())
        .collect();
    Ok(OfflineEvidence {
        core,
        budget: BudgetEvidence {
            limit_usd: MAX_COST_USD,
            spent_usd,
            crossed_after,
            started_after_crossing,
        },
    })
}

async fn run_budget_probe(
    assignment: Assignment,
) -> TestResult<(serde_json::Value, Vec<StartCall>)> {
    let bench = Bench::new()?;
    let workflow = save_fixture(&bench, assignment)?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::default());
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch), 9.25),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: workflow.clone(),
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let report = tokio::time::timeout(
        PATIENCE,
        run_workflow_with_reflection(&deps, &request, sink, Some(MAX_COST_USD), false),
    )
    .await
    .map_err(|_| "T-149 budget probe did not return through commands::run")??;
    tokio::time::timeout(PATIENCE, pump).await??;
    let run = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    Ok((run, watch.snapshot()))
}

#[derive(Clone)]
struct ObservedDriver {
    inner: Arc<dyn AgentDriver>,
    vendor: Vendor,
    call_kind: LiveCallKind,
    watch: Arc<LiveWatch>,
}

impl ObservedDriver {
    fn around(
        inner: Arc<dyn AgentDriver>,
        vendor: Vendor,
        call_kind: LiveCallKind,
        watch: Arc<LiveWatch>,
    ) -> Arc<dyn AgentDriver> {
        Arc::new(Self {
            inner,
            vendor,
            call_kind,
            watch,
        })
    }

    fn around_like(&self, inner: Arc<dyn AgentDriver>) -> Arc<dyn AgentDriver> {
        Self::around(inner, self.vendor, self.call_kind, Arc::clone(&self.watch))
    }
}

#[async_trait]
impl AgentDriver for ObservedDriver {
    fn id(&self) -> &'static str {
        self.inner.id()
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        self.inner.probe().await
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let role = if self.call_kind == LiveCallKind::Reflection {
            "reflection".to_owned()
        } else {
            role_in(&spec.prompt).to_owned()
        };
        let observed = LiveStart {
            role,
            turn: 0,
            vendor: self.vendor,
            model: spec.model.clone(),
            prompt: spec.prompt.clone(),
            cwd: spec.cwd.clone(),
            call_kind: self.call_kind,
        };
        let handle = self.inner.start(spec, events).await?;
        // Dopiero uchwyt znaczy, ze prawdziwy adapter wystartowal. Wybor z agenta albo grafu
        // bylby tylko deklaracja, wiec obserwacja trafia do oracle dopiero po tym `await`.
        self.watch.record(observed);
        Ok(handle)
    }

    fn with_evidence(&self, target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        self.inner
            .with_evidence(target)
            .map(|inner| self.around_like(inner))
    }

    fn with_settings(
        &self,
        settings: &StepSettings,
    ) -> Option<anyhow::Result<Arc<dyn AgentDriver>>> {
        self.inner
            .with_settings(settings)
            .map(|configured| configured.map(|inner| self.around_like(inner)))
    }

    fn with_budget(&self, dollars: f64) -> Option<Arc<dyn AgentDriver>> {
        self.inner
            .with_budget(dollars)
            .map(|inner| self.around_like(inner))
    }

    fn reflecting(&self) -> Option<Arc<dyn AgentDriver>> {
        self.inner.reflecting().map(|inner| {
            Self::around(
                inner,
                self.vendor,
                LiveCallKind::Reflection,
                Arc::clone(&self.watch),
            )
        })
    }
}

fn observed_live_drivers(watch: Arc<LiveWatch>) -> Drivers {
    let claude = ObservedDriver::around(
        Arc::new(ClaudeDriver::new()),
        Vendor::ClaudeCode,
        LiveCallKind::Step,
        Arc::clone(&watch),
    );
    let codex = ObservedDriver::around(
        Arc::new(CodexDriver::new()),
        Vendor::Codex,
        LiveCallKind::Step,
        watch,
    );
    Arc::new(move |vendor| match vendor {
        Vendor::ClaudeCode => Arc::clone(&claude),
        Vendor::Codex => Arc::clone(&codex),
    })
}

pub async fn run_live_oracle(assignment: Assignment) -> TestResult<LiveEvidence> {
    // Obrona jest takze tutaj, nie tylko w `#[ignore]` targecie: zadne wywolanie helpera nie
    // moze dojsc do inspekcji HOME ani konstrukcji platnego sterownika bez dokladnego opt-inu.
    require_paid_oracle();
    let _lease = HostLease::acquire()?;
    let exclusivity = ensure_host_exclusive()?;
    let user_home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is unavailable")?);
    let claude_json_before = fingerprint(&user_home.join(".claude.json"))?;
    let claude_projects_before = fingerprint(&user_home.join(".claude/projects"))?;
    let bench = Bench::new()?;
    let workflow = save_fixture(&bench, assignment)?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(LiveWatch::default());
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: observed_live_drivers(Arc::clone(&watch)),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: workflow.clone(),
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let run = run_workflow_with_reflection(&deps, &request, sink, Some(MAX_COST_USD), true);
    tokio::pin!(run);
    let report = match tokio::time::timeout(LIVE_PATIENCE, &mut run).await {
        Ok(Ok(report)) => report,
        Ok(Err(error)) => {
            let stopped = bounded_live_stop(&deps).await?;
            assert_eq!(stopped, Outcome::Cancelled);
            tokio::time::timeout(PATIENCE, pump)
                .await
                .map_err(|_| "paid run event pump did not settle after a run error")??;
            let run_dir = newest_run_dir(bench.project.path())?;
            assert_all_live_groups_dead(&run_dir)?;
            return Err(format!("paid phase-7 run failed after cleanup: {error}").into());
        }
        Err(_) => {
            let stopped = bounded_live_stop(&deps).await?;
            let report = tokio::time::timeout(PATIENCE, &mut run)
                .await
                .map_err(|_| "paid run future did not settle after bounded production Stop")??;
            assert_eq!(stopped, Outcome::Cancelled);
            tokio::time::timeout(PATIENCE, pump)
                .await
                .map_err(|_| "paid run event pump did not settle after timeout cleanup")??;
            assert_all_live_groups_dead(&report.dir)?;
            return Err(
                format!("paid phase-7 run exceeded {LIVE_PATIENCE:?} after cleanup").into(),
            );
        }
    };
    tokio::time::timeout(PATIENCE, pump).await??;
    let run_json: serde_json::Value =
        serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    let starts = watch.snapshot();
    let core = durable_core_evidence(assignment, &workflow, &report.dir, run_json, &starts)?;
    let costs = cost_evidence(&core.run_json, &report.dir, &starts)?;
    let reflection = reflection_evidence(&core.run_json, &report.dir, &starts)?;
    let claude_json_after = fingerprint(&user_home.join(".claude.json"))?;
    let claude_projects_after = fingerprint(&user_home.join(".claude/projects"))?;
    let git = git_evidence(&bench)?;
    let registered_worktrees_after = registered_worktrees(&bench)?;
    let final_success = core
        .run_json
        .get("status")
        .and_then(serde_json::Value::as_str)
        == Some("succeeded");
    Ok(LiveEvidence {
        core,
        costs,
        reflection,
        host: HostStateEvidence {
            exclusive: exclusivity.conflicts.is_empty(),
            scanned_processes: exclusivity.scanned_processes,
            claude_json_before,
            claude_json_after,
            claude_projects_before,
            claude_projects_after,
            private_state_under_run: has_private_claude_state(&report.dir)?,
        },
        stop: StopEvidence {
            stopped_groups: 0,
            groups_proven_dead: 0,
            registered_worktrees_after,
        },
        git,
        final_success,
    })
}

#[must_use]
pub fn paid_opt_in_for(value: Option<&str>) -> bool {
    value == Some(PAID_VALUE)
}

/// Kazdy test live wola ten guard sam, zanim skonstruuje fingerprint albo sterownik.
pub fn require_paid_oracle() {
    let value = std::env::var(PAID_ENV).ok();
    assert!(
        paid_opt_in_for(value.as_deref()),
        "paid phase-7 oracle refused: set {PAID_ENV}={PAID_VALUE} explicitly"
    );
}

fn durable_core_evidence(
    assignment: Assignment,
    workflow: &Path,
    run_dir: &Path,
    run_json: serde_json::Value,
    starts: &[LiveStart],
) -> TestResult<CoreEvidence> {
    let workflow_json: serde_json::Value = serde_json::from_slice(&fs::read(workflow)?)?;
    let max_turns = workflow_json["links"]
        .as_array()
        .and_then(|links| links.iter().find_map(|link| link.get("max_turns")))
        .and_then(serde_json::Value::as_u64)
        .and_then(|turns| u8::try_from(turns).ok())
        .ok_or("the saved live workflow lost max_turns")?;
    let steps = run_json["steps"]
        .as_array()
        .ok_or("live run.json has no expanded steps")?;
    let expanded_steps = expanded_step_names(steps);
    let artifacts = fs::read_dir(run_dir.join("handoffs"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    let rounds = (1_u8..=MAX_TURNS)
        .map(|number| {
            let work = nth_named_step(steps, "Work", number);
            let judge = nth_named_step(steps, "Judge", number);
            RoundEvidence {
                number,
                work_started: durable_step_started(run_dir, work),
                judge_started: durable_step_started(run_dir, judge),
                outcome: judge
                    .and_then(|step| step.get("round_outcome"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                artifacts: artifacts
                    .iter()
                    .filter(|path| {
                        artifact_step(path, &expanded_steps).is_some_and(|key| {
                            key == format!("work-{number}") || key == format!("judge-{number}")
                        })
                    })
                    .cloned()
                    .collect(),
            }
        })
        .collect::<Vec<_>>();
    let work_two = nth_named_step(steps, "Work", 2).ok_or("live run lost Work round two")?;
    let synthesis = nth_named_step(steps, "Synthesis", 1).ok_or("live run lost Synthesis")?;
    let (work_round_two_index, work_round_two_paths) =
        manifest_index(run_dir, physical_id(work_two)?, &expanded_steps)?;
    let (synthesis_index, synthesis_paths) =
        manifest_index(run_dir, physical_id(synthesis)?, &expanded_steps)?;
    let work_one = nth_named_step(steps, "Work", 1).ok_or("live run lost Work round one")?;
    let (_, work_one_paths) = manifest_index(run_dir, physical_id(work_one)?, &expanded_steps)?;
    let sentinel_source = work_one_paths
        .iter()
        .find(|path| fs::read_to_string(path).is_ok_and(|text| text.contains(SENTINEL)))
        .cloned()
        .ok_or("live Work round one did not receive the sentinel")?;
    let downstream = artifacts
        .iter()
        .find(|path| artifact_step(path, &expanded_steps).as_deref() == Some("work-1"))
        .ok_or("live Work round one left no handoff")?;
    let reflection_ran = run_json
        .pointer("/reflection/ran")
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    Ok(CoreEvidence {
        assignment,
        expanded_steps,
        max_turns,
        rounds,
        work_round_two_index,
        work_round_two_paths,
        synthesis_index,
        synthesis_paths,
        sentinel_read_from: sentinel_source.clone(),
        sentinel_source,
        downstream_handoff: fs::read_to_string(downstream)?,
        vendor_starts: starts
            .iter()
            .filter(|start| start.call_kind == LiveCallKind::Step)
            .map(|start| (start.role.clone(), start.vendor))
            .collect(),
        reflection_wrapped: reflection_ran
            && run_dir.join("logs/reflection.input.json").is_file()
            && run_dir.join("logs/reflection.jsonl").is_file(),
        reflection_saw_nonempty_handoff: observed_reflection_saw_handoff(starts, run_dir)?,
        run_json,
    })
}

fn expanded_step_names(steps: &[serde_json::Value]) -> Vec<String> {
    let mut work = 0_u8;
    let mut judge = 0_u8;
    steps
        .iter()
        .map(|step| match step["name"].as_str().unwrap_or_default() {
            "Plan" => "plan".to_owned(),
            "Work" => {
                work += 1;
                format!("work-{work}")
            }
            "Judge" => {
                judge += 1;
                format!("judge-{judge}")
            }
            "Synthesis" => "synthesis".to_owned(),
            other => format!("unknown-{other}"),
        })
        .collect()
}

fn durable_step_started(run_dir: &Path, step: Option<&serde_json::Value>) -> bool {
    let Some(step) = step else {
        return false;
    };
    let Some(id) = step.get("id").and_then(serde_json::Value::as_str) else {
        return false;
    };
    step.get("started_at")
        .and_then(serde_json::Value::as_i64)
        .is_some()
        && run_dir
            .join("logs")
            .join(format!("agent-{id}.input.json"))
            .is_file()
}

fn physical_id(step: &serde_json::Value) -> TestResult<&str> {
    step.get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "a live physical step has no id".into())
}

fn manifest_index(
    run_dir: &Path,
    step_id: &str,
    expanded_steps: &[String],
) -> TestResult<(Vec<String>, Vec<PathBuf>)> {
    let manifest: serde_json::Value = serde_json::from_slice(&fs::read(
        run_dir
            .join("logs")
            .join(format!("agent-{step_id}.input.json")),
    )?)?;
    let mut keys = Vec::new();
    let mut paths = Vec::new();
    for source in manifest["context"].as_array().into_iter().flatten() {
        if source.get("kind").and_then(serde_json::Value::as_str) != Some("handoff") {
            continue;
        }
        let reference = source
            .get("reference")
            .and_then(serde_json::Value::as_str)
            .ok_or("a handoff context source has no reference")?;
        let relative = PathBuf::from(reference);
        let path = if relative.is_absolute() {
            relative
        } else {
            run_dir.join(relative)
        };
        let key = artifact_step(&path, expanded_steps)
            .ok_or_else(|| format!("cannot map handoff context {}", path.display()))?;
        keys.push(key);
        paths.push(path);
    }
    Ok((keys, paths))
}

fn cost_evidence(
    run: &serde_json::Value,
    run_dir: &Path,
    starts: &[LiveStart],
) -> TestResult<CostEvidence> {
    let steps = run["steps"].as_array().ok_or("live run has no steps")?;
    let mut claude_usd = 0.0;
    let mut codex_usd = 0.0;
    let mut codex_input_tokens = 0_u64;
    let mut codex_output_tokens = 0_u64;
    let mut codex_model = None;
    let mut codex_is_estimate = false;
    let mut largest_turn_usd = 0.0_f64;
    let mut turns_by_role = HashMap::<String, usize>::new();
    for step in steps
        .iter()
        .filter(|step| durable_step_started(run_dir, Some(step)))
    {
        let role = step
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or("a started live step has no name")?
            .to_ascii_lowercase();
        let turn = turns_by_role.entry(role.clone()).or_default();
        *turn += 1;
        let observed = starts
            .iter()
            .find(|start| {
                start.call_kind == LiveCallKind::Step && start.role == role && start.turn == *turn
            })
            .ok_or_else(|| {
                format!(
                    "durable {role} turn {} has no successful real-driver start observation",
                    *turn
                )
            })?;
        let cost = step
            .get("cost_usd")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        largest_turn_usd = largest_turn_usd.max(cost);
        match observed.vendor {
            Vendor::ClaudeCode => claude_usd += cost,
            Vendor::Codex => {
                codex_usd += cost;
                codex_input_tokens += step
                    .get("input_tokens")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                codex_output_tokens += step
                    .get("output_tokens")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                codex_is_estimate |= step
                    .get("cost_estimate")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true);
                codex_model = observed.model.clone().or(codex_model);
            }
        }
    }
    let reflection_usd = run
        .pointer("/reflection/cost_usd")
        .and_then(serde_json::Value::as_f64)
        .ok_or("live reflection has no cost")?;
    Ok(CostEvidence {
        claude_usd,
        codex_usd,
        reflection_usd,
        codex_model: codex_model.ok_or("live Codex steps have no model")?,
        codex_input_tokens,
        codex_output_tokens,
        codex_is_estimate,
        largest_turn_usd,
    })
}

fn reflection_evidence(
    run: &serde_json::Value,
    run_dir: &Path,
    starts: &[LiveStart],
) -> TestResult<ReflectionEvidence> {
    Ok(ReflectionEvidence {
        ran: run
            .pointer("/reflection/ran")
            .and_then(serde_json::Value::as_bool)
            == Some(true),
        saw_nonempty_handoff: observed_reflection_saw_handoff(starts, run_dir)?,
        kept: run
            .pointer("/reflection/kept")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(0),
        receipt_in_run_json: run.get("reflection").is_some(),
    })
}

fn observed_reflection_saw_handoff(starts: &[LiveStart], run_dir: &Path) -> TestResult<bool> {
    let reflections = starts
        .iter()
        .filter(|start| start.call_kind == LiveCallKind::Reflection)
        .collect::<Vec<_>>();
    if reflections.len() != 1 {
        return Err(format!(
            "expected one successful observed reflection start, got {}",
            reflections.len()
        )
        .into());
    }
    let reflection = reflections[0];
    if reflection.cwd != run_dir {
        return Err(format!(
            "reflection RunSpec cwd {} is not the durable run directory {}",
            reflection.cwd.display(),
            run_dir.display()
        )
        .into());
    }
    if !reflection.prompt.contains("handoffs/") {
        return Ok(false);
    }
    // To nie jest juz pytanie „czy katalog przypadkiem ma plik". Nazwany przez prawdziwy
    // `RunSpec.prompt` input rozwiazujemy wzgledem obserwowanego `RunSpec.cwd`, czyli dokladnie
    // tego wejscia, ktore wrapper przekazal realnemu adapterowi przy udanym `start`.
    Ok(files_below(&reflection.cwd.join("handoffs"))?
        .into_iter()
        .any(|path| fs::read(path).is_ok_and(|body| !body.is_empty())))
}

fn git_evidence(bench: &Bench) -> TestResult<GitEvidence> {
    let base = bench.git(&["rev-parse", "HEAD"])?.trim().to_owned();
    let branches = bench.git(&[
        "for-each-ref",
        "--format=%(refname:short)",
        "refs/heads/loadout",
    ])?;
    for branch in branches.lines() {
        let range = format!("HEAD..{branch}");
        let diff = bench.git(&["diff", &range])?;
        if diff.trim().is_empty() {
            continue;
        }
        let changed_files = bench
            .git(&["diff", "--name-only", &range])?
            .lines()
            .map(str::to_owned)
            .collect();
        return Ok(GitEvidence {
            base,
            branch: branch.to_owned(),
            changed_files,
            diff,
        });
    }
    Err("the paid run left no nonempty reachable work branch".into())
}

fn registered_worktrees(bench: &Bench) -> TestResult<Vec<PathBuf>> {
    let listed = bench.git(&["worktree", "list", "--porcelain"])?;
    Ok(listed
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)
        .filter(|path| path != bench.project.path())
        .collect())
}

fn has_private_claude_state(run_dir: &Path) -> TestResult<bool> {
    Ok(files_below(&run_dir.join("claude"))?
        .into_iter()
        .any(|path| {
            path.strip_prefix(run_dir)
                .is_ok_and(|relative| relative.components().count() >= 3)
        }))
}

fn fingerprint(path: &Path) -> TestResult<Option<Vec<u8>>> {
    if !path.exists() {
        return Ok(None);
    }
    let mut hasher = DefaultHasher::new();
    for file in files_below(path)? {
        file.strip_prefix(path)
            .unwrap_or(file.as_path())
            .hash(&mut hasher);
        fs::read(&file)?.hash(&mut hasher);
    }
    Ok(Some(hasher.finish().to_le_bytes().to_vec()))
}

fn files_below(path: &Path) -> TestResult<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    if !path.is_dir() {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    let mut pending = vec![path.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                found.push(entry.path());
            }
        }
    }
    found.sort();
    Ok(found)
}

async fn bounded_live_stop(deps: &RunDeps<'_>) -> TestResult<Outcome> {
    tokio::time::timeout(PATIENCE, stop_run_inner(deps))
        .await
        .map_err(|_| "production Stop did not settle within the paid cleanup deadline")?
        .map_err(Into::into)
}

fn newest_run_dir(project: &Path) -> TestResult<PathBuf> {
    let runs = project.join(".loadout/runs");
    let mut dirs = fs::read_dir(&runs)?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_type().ok()?.is_dir().then_some(entry.path()))
        .collect::<Vec<_>>();
    dirs.sort();
    dirs.pop()
        .ok_or_else(|| format!("paid failure left no durable run under {}", runs.display()).into())
}

fn assert_all_live_groups_dead(run_dir: &Path) -> TestResult {
    let run: serde_json::Value = serde_json::from_slice(&fs::read(run_dir.join("run.json"))?)?;
    for step in run["steps"].as_array().into_iter().flatten() {
        let Some(pgid) = step
            .get("pgid")
            .and_then(serde_json::Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
        else {
            continue;
        };
        assert_eq!(
            step.get("death_proof").and_then(serde_json::Value::as_bool),
            Some(true),
            "paid cleanup left group {pgid} without durable death proof"
        );
        let proof = group_probe(pgid);
        assert_eq!(
            proof.err().and_then(|error| error.raw_os_error()),
            Some(libc::ESRCH),
            "paid cleanup returned while pure kill(-{pgid}, 0) still sees the process group"
        );
    }
    Ok(())
}

#[allow(unsafe_code)]
fn group_probe(pgid: i32) -> io::Result<()> {
    // SAFETY: sygnal 0 nie dostarcza sygnalu. Pyta jadro o istnienie grupy i niczego nie zbiera,
    // w przeciwienstwie do `reap_group`, wiec sam oracle nie moze wytworzyc swojego dowodu.
    let rc = unsafe { libc::kill(-pgid, 0) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[derive(Debug)]
struct HostExclusivity {
    scanned_processes: usize,
    conflicts: Vec<String>,
}

fn ensure_host_exclusive() -> TestResult<HostExclusivity> {
    let output = Command::new("ps").args(["-axo", "pid=,comm="]).output()?;
    if !output.status.success() {
        return Err(format!(
            "paid oracle refused: host process exclusivity could not be checked (ps exited {})",
            output.status
        )
        .into());
    }
    let table = String::from_utf8(output.stdout)
        .map_err(|_| "paid oracle refused: host process table was not UTF-8")?;
    let own_pid = std::process::id();
    let mut scanned_processes = 0;
    let mut conflicts = Vec::new();
    for line in table.lines() {
        let Some((pid, command)) = line.trim().split_once(char::is_whitespace) else {
            continue;
        };
        let Ok(pid) = pid.parse::<u32>() else {
            continue;
        };
        scanned_processes += 1;
        if pid == own_pid {
            continue;
        }
        let executable = Path::new(command.trim())
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(command.trim())
            .to_ascii_lowercase();
        if is_host_state_writer(&executable) {
            // Tylko PID i basename, nigdy argv: prompt oraz sekrety nie trafiaja do odmowy.
            conflicts.push(format!("pid {pid} ({executable})"));
        }
    }
    let evidence = HostExclusivity {
        scanned_processes,
        conflicts,
    };
    if !evidence.conflicts.is_empty() {
        return Err(format!(
            "paid oracle refused before touching host state: external vendor processes are active: {}",
            evidence.conflicts.join(", ")
        )
        .into());
    }
    Ok(evidence)
}

fn is_host_state_writer(executable: &str) -> bool {
    ["claude", "codex", "loadout"].iter().any(|name| {
        executable == *name
            || executable.starts_with(&format!("{name}-"))
            || executable.starts_with(&format!("{name}_"))
    })
}

struct HostLease {
    _file: fs::File,
    path: PathBuf,
}

impl HostLease {
    fn acquire() -> TestResult<Self> {
        let path = std::env::temp_dir().join("loadout-phase7-paid-oracle.lock");
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                format!(
                    "paid oracle cannot guarantee exclusive host fingerprinting at {}: {error}",
                    path.display()
                )
            })?;
        Ok(Self { _file: file, path })
    }
}

impl Drop for HostLease {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn oracle_not_authored() -> TestResult {
    assert!(std::hint::black_box(false), "T-149 oracle not authored");
    Err("T-149 oracle not authored".into())
}

async fn run_fake_graph(
    assignment: Assignment,
    turn_cost: f64,
    reflection_enabled: bool,
) -> TestResult<(CoreEvidence, Vec<StartCall>)> {
    let bench = Bench::new()?;
    let workflow = save_fixture(&bench, assignment)?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::default());
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch), turn_cost),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: workflow.clone(),
        how_many_at_once: 2,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let report = tokio::time::timeout(
        PATIENCE,
        run_workflow_with_reflection(
            &deps,
            &request,
            sink,
            Some(MAX_COST_USD),
            reflection_enabled,
        ),
    )
    .await
    .map_err(|_| "T-149 offline oracle did not return through commands::run")??;
    tokio::time::timeout(PATIENCE, pump).await??;
    assert_eq!(report.outcome, Outcome::Done);
    let run: serde_json::Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    let calls = watch.snapshot();
    let core = core_evidence(assignment, &workflow, &report.dir, run, &calls)?;
    Ok((core, calls))
}

fn core_evidence(
    assignment: Assignment,
    workflow: &Path,
    run_dir: &Path,
    run_json: serde_json::Value,
    calls: &[StartCall],
) -> TestResult<CoreEvidence> {
    let workflow_json: serde_json::Value = serde_json::from_slice(&fs::read(workflow)?)?;
    let max_turns = workflow_json["links"]
        .as_array()
        .and_then(|links| links.iter().find_map(|link| link.get("max_turns")))
        .and_then(serde_json::Value::as_u64)
        .and_then(|turns| u8::try_from(turns).ok())
        .ok_or("the saved workflow lost max_turns")?;
    let steps = run_json["steps"]
        .as_array()
        .ok_or("run.json has no expanded steps")?;
    let mut work_number = 0_u8;
    let mut judge_number = 0_u8;
    let expanded_steps = steps
        .iter()
        .map(|step| match step["name"].as_str().unwrap_or_default() {
            "Plan" => "plan".to_owned(),
            "Work" => {
                work_number += 1;
                format!("work-{work_number}")
            }
            "Judge" => {
                judge_number += 1;
                format!("judge-{judge_number}")
            }
            "Synthesis" => "synthesis".to_owned(),
            other => format!("unknown-{other}"),
        })
        .collect::<Vec<_>>();
    let artifacts = fs::read_dir(run_dir.join("handoffs"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    let rounds = (1_u8..=MAX_TURNS)
        .map(|number| {
            let judge = nth_named_step(steps, "Judge", number);
            RoundEvidence {
                number,
                work_started: call(calls, "work", usize::from(number)).is_ok(),
                judge_started: call(calls, "judge", usize::from(number)).is_ok(),
                outcome: judge
                    .and_then(|step| step.get("round_outcome"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                artifacts: artifacts
                    .iter()
                    .filter(|path| {
                        artifact_step(path, &expanded_steps).is_some_and(|key| {
                            key == format!("work-{number}") || key == format!("judge-{number}")
                        })
                    })
                    .cloned()
                    .collect(),
            }
        })
        .collect();
    let work_two = call(calls, "work", 2)?;
    let synthesis = call(calls, "synthesis", 1)?;
    let (work_round_two_index, work_round_two_paths) =
        prompt_index(&work_two.prompt, &expanded_steps);
    let (synthesis_index, synthesis_paths) = prompt_index(&synthesis.prompt, &expanded_steps);
    let work_one = call(calls, "work", 1)?;
    let sentinel_read_from = work_one
        .reads
        .iter()
        .find(|path| fs::read_to_string(path).is_ok_and(|text| text.contains(SENTINEL)))
        .cloned()
        .ok_or("work round one did not read the upstream sentinel")?;
    let sentinel_source = sentinel_read_from.clone();
    let downstream = artifacts
        .iter()
        .find(|path| artifact_step(path, &expanded_steps).as_deref() == Some("work-1"))
        .ok_or_else(|| format!("work round one left no real handoff: {artifacts:?}"))?;
    let downstream_handoff = fs::read_to_string(downstream)?;
    let reflection = call(calls, "reflection", 1)?;
    Ok(CoreEvidence {
        assignment,
        expanded_steps,
        max_turns,
        rounds,
        work_round_two_index,
        work_round_two_paths,
        synthesis_index,
        synthesis_paths,
        sentinel_source,
        sentinel_read_from,
        downstream_handoff,
        run_json,
        vendor_starts: calls
            .iter()
            .filter(|call| call.role != "reflection")
            .map(|call| (call.role.clone(), call.vendor))
            .collect(),
        reflection_wrapped: reflection.reflection_wrapped,
        reflection_saw_nonempty_handoff: reflection.reflection_saw_nonempty_handoff,
    })
}

fn nth_named_step<'a>(
    steps: &'a [serde_json::Value],
    name: &str,
    number: u8,
) -> Option<&'a serde_json::Value> {
    steps
        .iter()
        .filter(|step| step["name"].as_str() == Some(name))
        .nth(usize::from(number - 1))
}

fn call<'a>(calls: &'a [StartCall], role: &str, turn: usize) -> TestResult<&'a StartCall> {
    calls
        .iter()
        .find(|call| call.role == role && call.turn == turn)
        .ok_or_else(|| format!("no {role} call for turn {turn}: {calls:?}").into())
}

fn prompt_index(prompt: &str, expanded_steps: &[String]) -> (Vec<String>, Vec<PathBuf>) {
    let rows = prompt.lines().filter_map(|line| {
        let (_, rest) = line.strip_prefix("- ")?.split_once(": ")?;
        let path = rest.split_once(" (").map_or(rest, |(path, _)| path);
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return None;
        }
        artifact_step(&path, expanded_steps).map(|key| (key, path))
    });
    rows.unzip()
}

fn artifact_step(path: &Path, expanded_steps: &[String]) -> Option<String> {
    let step = path
        .file_stem()
        .and_then(|name| name.to_str())
        .and_then(|name| name.split_once("__"))?
        .0
        .parse::<usize>()
        .ok()?;
    expanded_steps.get(step).cloned()
}

fn save_fixture(bench: &Bench, assignment: Assignment) -> TestResult<PathBuf> {
    let writer = saved_agent(bench.home.path(), WRITER_ID, "Writer", assignment.writer)?;
    let judge = saved_agent(bench.home.path(), JUDGE_ID, "Judge", assignment.judge)?;
    let workflow = fixture(&writer, &judge);
    Ok(save_workflow_inner(
        bench.home.path(),
        "t149-phase7.json",
        &workflow,
    )?)
}

fn saved_agent(home: &Path, id: Uuid, name: &str, vendor: Vendor) -> TestResult<Agent> {
    let mut agent = Agent::example();
    agent.id = id;
    name.clone_into(&mut agent.name);
    agent.runs_with = vendor;
    agent.model = match vendor {
        Vendor::ClaudeCode => "opus".to_owned(),
        Vendor::Codex => CODEX_MODEL.to_owned(),
    };
    agent.file_access = FileAccess::WorkFreely;
    agent.write_results_to.clear();
    save_agent_inner(home, &agent)?;
    Ok(agent)
}

#[must_use]
pub fn fixture(writer: &Agent, judge: &Agent) -> WorkflowFile {
    WorkflowFile {
        format: 1,
        id: "wf_t149_phase7_oracle".to_owned(),
        name: "Phase 7 oracle".to_owned(),
        description: None,
        steps: vec![
            agent_step("s_plan", "Plan", writer, PLAN),
            agent_step("s_work", "Work", writer, WORK),
            agent_step("s_judge", "Judge", judge, JUDGE),
            agent_step("s_synthesis", "Synthesis", writer, SYNTHESIS),
        ],
        links: vec![
            link("s_plan", "s_work", None),
            link("s_work", "s_judge", None),
            link("s_judge", "s_work", Some(MAX_TURNS)),
            link("s_judge", "s_synthesis", None),
        ],
        extra: serde_json::Map::new(),
    }
}

fn agent_step(id: &str, name: &str, agent: &Agent, instructions: &str) -> Step {
    Step::Agent(AgentStep {
        id: id.to_owned(),
        name: name.to_owned(),
        agent: agent.id.to_string(),
        overrides: serde_json::Map::new(),
        vendor_options: BTreeMap::new(),
        copies: 1,
        instructions: instructions.to_owned(),
        skills: Skills::default(),
        borrow: Borrow::default(),
        folder: Folder::FreshCopy,
        handover: Handover::default(),
        when_it_fails: WhenItFails::default(),
        at: Point::default(),
        extra: serde_json::Map::new(),
    })
}

fn link(from: &str, to: &str, max_turns: Option<u8>) -> Link {
    Link {
        from: from.to_owned(),
        to: to.to_owned(),
        max_turns,
    }
}

#[derive(Debug, Clone)]
struct StartCall {
    role: String,
    turn: usize,
    vendor: Vendor,
    prompt: String,
    reads: Vec<PathBuf>,
    reflection_wrapped: bool,
    reflection_saw_nonempty_handoff: bool,
}

#[derive(Debug, Default)]
struct Watch(Mutex<Vec<StartCall>>);

impl Watch {
    fn next_turn(&self, role: &str) -> usize {
        self.lock().iter().filter(|one| one.role == role).count() + 1
    }

    fn record(&self, call: StartCall) {
        self.lock().push(call);
    }

    fn entered(&self, prompt: &str) -> (String, usize) {
        let role = role_in(prompt).to_owned();
        let turn = self.next_turn(&role);
        (role, turn)
    }

    fn entered_as(&self, role: &str) -> (String, usize) {
        let role = role.to_owned();
        let turn = self.next_turn(&role);
        (role, turn)
    }

    fn snapshot(&self) -> Vec<StartCall> {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<StartCall>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn role_in(prompt: &str) -> &str {
    [
        (PLAN, "plan"),
        (WORK, "work"),
        (JUDGE, "judge"),
        (SYNTHESIS, "synthesis"),
    ]
    .into_iter()
    .find(|(opening, _)| prompt.starts_with(opening))
    .map_or("unknown", |(_, role)| role)
}

fn fake_drivers(watch: Arc<Watch>, turn_cost: f64) -> Drivers {
    Arc::new(move |vendor| {
        Arc::new(Fake {
            watch: Arc::clone(&watch),
            vendor,
            call_kind: CallKind::Step,
            wrappers: ReflectionWrappers::default(),
            turn_cost,
        })
    })
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
enum CallKind {
    #[default]
    Step,
    Reflection,
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
struct ReflectionWrappers(u8);

impl ReflectionWrappers {
    const SETTINGS: u8 = 1;
    const EVIDENCE: u8 = 2;
    const BUDGET: u8 = 4;
    const ALL: u8 = Self::SETTINGS | Self::EVIDENCE | Self::BUDGET;

    fn including(mut self, wrapper: u8) -> Self {
        self.0 |= wrapper;
        self
    }

    fn complete(self) -> bool {
        self.0 == Self::ALL
    }
}

#[derive(Debug, Clone)]
struct Fake {
    watch: Arc<Watch>,
    vendor: Vendor,
    call_kind: CallKind,
    wrappers: ReflectionWrappers,
    turn_cost: f64,
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        match self.vendor {
            Vendor::ClaudeCode => "t149-claude-fake",
            Vendor::Codex => "t149-codex-fake",
        }
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("t149-fake".to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let (role, turn) = if self.call_kind == CallKind::Reflection {
            self.watch.entered_as("reflection")
        } else {
            self.watch.entered(&spec.prompt)
        };
        if role == "work" {
            fs::write(
                spec.cwd.join("t149-work.txt"),
                format!("work round {turn}\n"),
            )?;
        }
        let read = read_indexed_handoffs(&spec.prompt)?;
        let reflection_saw_nonempty_handoff = self.call_kind == CallKind::Reflection
            && fs::read_dir(spec.cwd.join("handoffs"))?
                .filter_map(Result::ok)
                .any(|entry| fs::read(entry.path()).is_ok_and(|body| !body.is_empty()));
        self.watch.record(StartCall {
            role: role.clone(),
            turn,
            vendor: self.vendor,
            prompt: spec.prompt.clone(),
            reads: read.keys().cloned().collect(),
            reflection_wrapped: self.wrappers.complete(),
            reflection_saw_nonempty_handoff,
        });
        let said = if self.call_kind == CallKind::Reflection {
            REFLECTION.to_owned()
        } else {
            fake_answer(&role, turn, &read)
        };
        let session = SessionRef {
            vendor: "t149-fake",
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
            said,
            cost_usd: self.turn_cost,
        }))
    }

    fn reflecting(&self) -> Option<Arc<dyn AgentDriver>> {
        let mut configured = self.clone();
        configured.call_kind = CallKind::Reflection;
        Some(Arc::new(configured))
    }

    fn with_settings(
        &self,
        _settings: &StepSettings,
    ) -> Option<anyhow::Result<Arc<dyn AgentDriver>>> {
        let mut configured = self.clone();
        configured.wrappers = configured.wrappers.including(ReflectionWrappers::SETTINGS);
        Some(Ok(Arc::new(configured)))
    }

    fn with_evidence(&self, _target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        let mut configured = self.clone();
        configured.wrappers = configured.wrappers.including(ReflectionWrappers::EVIDENCE);
        Some(Arc::new(configured))
    }

    fn with_budget(&self, _dollars: f64) -> Option<Arc<dyn AgentDriver>> {
        let mut configured = self.clone();
        configured.wrappers = configured.wrappers.including(ReflectionWrappers::BUDGET);
        Some(Arc::new(configured))
    }
}

fn read_indexed_handoffs(prompt: &str) -> anyhow::Result<HashMap<PathBuf, String>> {
    let mut read = HashMap::new();
    for word in prompt.split_whitespace() {
        let candidate = word.trim_end_matches([',', ';', ':', ')']);
        let path = PathBuf::from(candidate);
        if path.is_absolute() && candidate.contains("handoffs/") && path.is_file() {
            read.insert(path.clone(), fs::read_to_string(path)?);
        }
    }
    Ok(read)
}

fn fake_answer(role: &str, turn: usize, read: &HashMap<PathBuf, String>) -> String {
    let inherited = read.values().cloned().collect::<Vec<String>>().join("\n");
    let answer = match role {
        "plan" => format!("plan-1 carries {SENTINEL}"),
        "work" => format!("work-{turn} read:\n{inherited}"),
        "judge" => format!("judge-{turn} read:\n{inherited}"),
        "synthesis" => format!("synthesis read:\n{inherited}"),
        _ => format!("unrecognised prompt role: {role}"),
    };
    let outcome = match (role, turn) {
        ("judge", 1) => "\noutcome: fail\n",
        ("judge", 2) => "\noutcome: pass\n",
        _ => "",
    };
    format!("## Answer\n{answer}\n{outcome}\n## Evidence\nt149-work.txt:1\n\n## Open\nnothing\n")
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    said: String,
    cost_usd: f64,
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
            text: self.said.clone(),
            cost_usd: Some(self.cost_usd),
            tokens: Tokens {
                input: 20,
                output: 10,
                cached: 0,
            },
            turns: 1,
            took: Duration::from_millis(1),
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

struct Bench {
    home: TempDir,
    project: TempDir,
}

impl Bench {
    fn new() -> TestResult<Self> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path())?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(project.path().join("seed.txt"), "human-owned seed\n")?;
        fs::write(project.path().join(".gitignore"), ".loadout/\n")?;
        let bench = Self { home, project };
        bench.git(&["init", "--quiet"])?;
        bench.git(&["add", "-A"])?;
        bench.git(&["commit", "--quiet", "-m", "oracle seed"])?;
        Ok(bench)
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }

    fn git(&self, args: &[&str]) -> TestResult<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(self.project.path())
            .args(["-c", "user.name=Loadout Oracle"])
            .args(["-c", "user.email=oracle@loadout.invalid"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()?;
        if !out.status.success() {
            return Err(format!(
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            )
            .into());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}
