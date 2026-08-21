//! AC-8 dla T-65: jedna sprawa dostaje jeden trwały claim i najwyżej jedną akceptację biegu.
//!
//! SQLite nie bierze udziału w deduplikacji. Każda asercja po restarcie buduje odpowiedź
//! wyłącznie z plików triggera i `run.json` (niezmiennik 4), a żywa część używa tej samej
//! `run_triggered_workflow_inner`, którą opakowuje istniejące `run_workflow`.

#![allow(clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::{
    TriggerRunReport, run_triggered_workflow_inner, run_workflow_inner,
};
use loadout_lib::commands::triggers::{self, DeliveryState, TriggerDelivery, TriggerPoll};
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{AppState, QUEUE_CAP, line_channel};
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::mpsc;
use uuid::{Uuid, Version};

const KEY: &str = "lin_api_1234567890123456789012345678901234567890";
const CREATED: i64 = 1_777_777_777_000;
const PATIENCE: Duration = Duration::from_secs(10);

const AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-000000000065
name: Trigger witness
summary: Proves acceptance order
color: clay
runsWith: claude-code
model: sonnet
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 20
writeResultsTo: handoffs/result.md
tools: everything
skills: []
connections: []
---
Finish the delivered issue.
";

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_ship_it",
  "name": "Ship it",
  "steps": [{
    "kind": "agent",
    "id": "s_ship",
    "name": "Ship",
    "agent": "01990000-0000-7000-8000-000000000065",
    "overrides": {},
    "instructions": "ship",
    "at": { "x": 0, "y": 0 }
  }],
  "links": []
}"#;

#[test]
fn every_new_issue_is_queued_once_by_id_not_updated_at() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    assert_eq!(
        poll(&bench, &[issue("old", "LOAD-0", 8)])?,
        TriggerPoll::Armed
    );

    let first = poll(
        &bench,
        &[
            issue("issue-a", "LOAD-1", 9),
            issue("issue-b", "LOAD-2", 10),
        ],
    )?;
    assert!(matches!(first, TriggerPoll::Pending { .. }));
    let pending = triggers::pending_deliveries(bench.home.path(), "mine")?;
    assert_eq!(
        issue_ids(&pending),
        vec!["issue-a", "issue-b"],
        "two issues returned between ticks were collapsed into one newest cursor"
    );
    assert_unique_v7_ids(&pending)?;

    // To samo ID z nowszym `updatedAt` nie jest nową dostawą; nowe ID obok niego jest.
    let _ = poll(
        &bench,
        &[
            issue("issue-a", "LOAD-1", 11),
            issue("issue-c", "LOAD-3", 12),
        ],
    )?;
    let pending = triggers::pending_deliveries(bench.home.path(), "mine")?;
    assert_eq!(
        issue_ids(&pending),
        vec!["issue-a", "issue-b", "issue-c"],
        "updatedAt was used as identity: an edited issue duplicated or a neighbouring issue vanished"
    );
    let cursor = fs::read_to_string(triggers::cursor_path(bench.home.path(), "mine"))?;
    assert_eq!(cursor.trim(), "2026-08-21T12:00:00.000Z");
    Ok(())
}

#[test]
fn pending_is_durable_before_the_cursor_and_restart_keeps_its_ids() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    assert_eq!(
        poll(&bench, &[issue("old", "LOAD-0", 8)])?,
        TriggerPoll::Armed
    );
    let cursor = triggers::cursor_path(bench.home.path(), "mine");
    fs::remove_file(&cursor)?;
    fs::create_dir(&cursor)?; // deterministyczna odmowa zapisu pliku kursora

    let failed = poll(&bench, &[issue("issue-a", "LOAD-1", 9)]);
    assert!(
        failed.is_err(),
        "an unwritable cursor was silently reported as a complete poll"
    );
    let before_restart = triggers::pending_deliveries(bench.home.path(), "mine")?;
    assert_eq!(
        issue_ids(&before_restart),
        vec!["issue-a"],
        "the cursor write failed before the pending receipt reached disk, so the issue was lost"
    );

    // Nowy odczyt bez żadnego stanu w pamięci procesu jest symulacją restartu.
    let after_restart = triggers::pending_deliveries(bench.home.path(), "mine")?;
    assert_eq!(
        after_restart, before_restart,
        "restart minted a second delivery or run UUID"
    );
    Ok(())
}

#[test]
fn an_accepted_receipt_does_not_hide_later_issues() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let delivery = bench.one_delivery()?;
    let run_file = bench
        .project
        .path()
        .join(".loadout/runs/already-accepted/run.json");
    triggers::bind_delivery(bench.home.path(), &delivery.claim, &run_file)?;
    fs::create_dir_all(run_file.parent().ok_or("run.json has no parent")?)?;
    fs::write(
        &run_file,
        serde_json::to_vec_pretty(&accepted_run_json(&delivery))?,
    )?;
    assert!(matches!(
        triggers::reconcile_delivery(bench.home.path(), &delivery.claim)?,
        DeliveryState::Accepted { .. }
    ));

    let next = poll(&bench, &[issue("issue-b", "LOAD-2", 10)])?;
    assert!(
        matches!(
            next,
            TriggerPoll::Pending {
                delivery: TriggerDelivery { ref issue, .. }
            } if issue.id == "issue-b"
        ),
        "an old Accepted receipt short-circuited the fetch and hid a later issue: {next:?}"
    );
    assert_eq!(
        issue_ids(&triggers::pending_deliveries(bench.home.path(), "mine")?),
        vec!["issue-b"]
    );
    Ok(())
}

#[test]
fn already_going_leaves_the_claim_pending_until_the_run_settles() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let delivery = bench.one_delivery()?;
    let state = bench.app_state(no_agents())?;
    let live = state.begin_a_run(bench.project.path())?;
    live.control.begin();

    let refused = state.begin_triggered_run(bench.project.path(), &delivery.claim);
    assert!(
        refused
            .as_ref()
            .is_err_and(|said| said.contains("already") || said.contains("Stop")),
        "the trigger was not refused by the same Rust-owned one-run gate: {refused:?}"
    );
    assert_eq!(
        triggers::delivery_state(bench.home.path(), &delivery.claim)?,
        DeliveryState::Pending,
        "ALREADY_GOING consumed or bound the claim before a run could own it"
    );
    assert!(
        state.deps().control.is_working(),
        "refusing the trigger replaced the live handle, so Stop can no longer reach the first run"
    );

    live.control.settle();
    let reserved = state
        .begin_triggered_run(bench.project.path(), &delivery.claim)
        .map_err(|said| format!("the settled run kept the claim blocked: {said}"))?;
    assert_eq!(
        triggers::delivery_state(bench.home.path(), &delivery.claim)?,
        DeliveryState::Pending,
        "reserving AppState.live accepted the issue before run.json existed"
    );
    reserved.control.settle();
    Ok(())
}

#[test]
fn a_forged_claim_is_refused_without_leaving_the_live_latch_taken() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let delivery = bench.one_delivery()?;
    let state = bench.app_state(no_agents())?;
    let mut forged = Vec::new();

    let mut wrong = delivery.claim.clone();
    wrong.slug = "somebody-elses-trigger".to_owned();
    forged.push(wrong);
    let mut wrong = delivery.claim.clone();
    wrong.delivery_id = "somebody-elses-delivery".to_owned();
    forged.push(wrong);
    let mut wrong = delivery.claim.clone();
    wrong.workflow = "somebody-elses-workflow.json".to_owned();
    forged.push(wrong);
    let mut wrong = delivery.claim.clone();
    wrong.run_id = Uuid::now_v7().to_string();
    forged.push(wrong);

    for claim in forged {
        let refused = state.begin_triggered_run(bench.project.path(), &claim);
        assert!(refused.is_err(), "a forged trigger claim was accepted");
        assert_eq!(
            triggers::delivery_state(bench.home.path(), &delivery.claim)?,
            DeliveryState::Pending,
            "rejecting a forged claim changed the real delivery"
        );
    }

    let reserved = state
        .begin_triggered_run(bench.project.path(), &delivery.claim)
        .map_err(|said| format!("a forged claim took the live latch: {said}"))?;
    reserved.control.settle();
    Ok(())
}

#[tokio::test]
async fn an_authentic_claim_cannot_start_a_different_workflow() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let delivery = bench.one_delivery()?;
    let other = bench.workflow("other", WORKFLOW)?;
    let starts = Arc::new(AtomicUsize::new(0));
    let state = bench.app_state(counting_drivers(Arc::clone(&starts)))?;
    let request = RunRequest {
        workflow: other,
        how_many_at_once: 1,
        task: Some("forged task".to_owned()),
    };
    let (sink, _source) = line_channel(QUEUE_CAP);

    let refused = {
        let deps = state
            .begin_triggered_run(bench.project.path(), &delivery.claim)
            .map_err(|said| format!("the authentic claim could not reserve the run: {said}"))?;
        run_triggered_workflow_inner(&deps, &request, &delivery.claim, sink).await
    };
    assert!(
        refused.is_err(),
        "an authentic claim was replayed against a workflow other than the one in its receipt"
    );
    assert_eq!(starts.load(Ordering::Acquire), 0);
    assert_eq!(
        triggers::delivery_state(bench.home.path(), &delivery.claim)?,
        DeliveryState::Pending,
        "a mismatched workflow consumed the authentic delivery"
    );
    assert_eq!(run_dirs(bench.project.path())?, 0);
    let next = state
        .begin_run(bench.project.path())
        .map_err(|said| format!("workflow mismatch left AppState.live latched: {said}"))?;
    next.control.settle();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_json_accepts_the_claim_before_the_first_driver_call() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let delivery = bench.one_delivery()?;
    let starts = Arc::new(AtomicUsize::new(0));
    let seen_problem = Arc::new(Mutex::new(None));
    let drivers = inspecting_drivers(
        bench.home.path().to_path_buf(),
        delivery.clone(),
        Arc::clone(&starts),
        Arc::clone(&seen_problem),
    );
    let store = Store::open(&bench.db())?;
    let deps = bench.deps(&store, drivers);
    let request = bench.request();
    let (sink, _source) = line_channel(QUEUE_CAP);

    let result = tokio::time::timeout(
        PATIENCE,
        run_triggered_workflow_inner(&deps, &request, &delivery.claim, sink),
    )
    .await
    .map_err(|_| "the triggered run did not finish")??;
    let TriggerRunReport::Ran(report) = result else {
        return Err("a fresh pending delivery was treated as already accepted".into());
    };

    assert_eq!(
        report.id, delivery.claim.run_id,
        "the plan minted a new UUID after claiming"
    );
    assert_eq!(
        starts.load(Ordering::Acquire),
        1,
        "the one-step workflow did not start exactly once"
    );
    if let Some(problem) = seen_problem
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
    {
        return Err(problem.into());
    }
    assert_accepted_run_file(&report.dir.join("run.json"), &delivery)?;
    Ok(())
}

#[tokio::test]
async fn a_refused_workflow_stays_pending_and_starts_nothing() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let delivery = bench.one_delivery()?;
    let invalid = bench.workflow(
        "ship-it",
        r#"{"format":1,"id":"bad","name":"Bad","steps":[],"links":[]}"#,
    )?;
    let starts = Arc::new(AtomicUsize::new(0));
    let state = bench.app_state(counting_drivers(Arc::clone(&starts)))?;
    let request = RunRequest {
        workflow: invalid,
        how_many_at_once: 1,
        task: Some("LOAD-1: Do the work".to_owned()),
    };
    let (sink, _source) = line_channel(QUEUE_CAP);

    let refused = {
        let deps = state
            .begin_triggered_run(bench.project.path(), &delivery.claim)
            .map_err(|said| format!("a pending delivery could not reserve the run: {said}"))?;
        run_triggered_workflow_inner(&deps, &request, &delivery.claim, sink).await
    };
    assert!(refused.is_err(), "an empty workflow unexpectedly started");
    assert_eq!(
        starts.load(Ordering::Acquire),
        0,
        "a driver ran before workflow refusal"
    );
    assert_eq!(
        triggers::delivery_state(bench.home.path(), &delivery.claim)?,
        DeliveryState::Pending,
        "a workflow refusal consumed the issue instead of leaving it actionable"
    );
    assert_eq!(
        run_dirs(bench.project.path())?,
        0,
        "a refused workflow left a run directory"
    );
    let next = state
        .begin_run(bench.project.path())
        .map_err(|said| format!("workflow refusal left AppState.live latched: {said}"))?;
    next.control.settle();
    Ok(())
}

#[tokio::test]
async fn crash_after_run_json_reconciles_without_a_second_directory_or_start()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let delivery = bench.one_delivery()?;
    let run_dir = bench
        .project
        .path()
        .join(".loadout/runs/crash-window__")
        .join(&delivery.claim.run_id);
    let run_file = run_dir.join("run.json");
    triggers::bind_delivery(bench.home.path(), &delivery.claim, &run_file)?;
    fs::create_dir_all(&run_dir)?;

    let mismatches = [
        ("id", "somebody-elses-run"),
        ("trigger_origin.slug", "somebody-elses-trigger"),
        ("trigger_origin.delivery_id", "somebody-elses-delivery"),
        ("trigger_origin.issue_id", "somebody-elses-issue"),
    ];
    for (field, replacement) in mismatches {
        let mut candidate = accepted_run_json(&delivery);
        let target = match field {
            "id" => &mut candidate["id"],
            "trigger_origin.slug" => &mut candidate["trigger_origin"]["slug"],
            "trigger_origin.delivery_id" => &mut candidate["trigger_origin"]["delivery_id"],
            "trigger_origin.issue_id" => &mut candidate["trigger_origin"]["issue_id"],
            _ => unreachable!("the mismatch matrix is exhaustive"),
        };
        *target = json!(replacement);
        fs::write(&run_file, serde_json::to_vec_pretty(&candidate)?)?;
        let reconciled = triggers::reconcile_delivery(bench.home.path(), &delivery.claim);
        assert!(
            !matches!(reconciled, Ok(DeliveryState::Accepted { .. })),
            "reconcile accepted a run.json with mismatched {field}"
        );
        assert!(
            matches!(
                triggers::delivery_state(bench.home.path(), &delivery.claim)?,
                DeliveryState::Bound { run_file: ref found } if found == &run_file
            ),
            "a mismatched {field} changed the durable delivery state"
        );
    }

    fs::write(
        &run_file,
        serde_json::to_vec_pretty(&accepted_run_json(&delivery))?,
    )?;

    let state = triggers::reconcile_delivery(bench.home.path(), &delivery.claim)?;
    assert!(
        matches!(state, DeliveryState::Accepted { run_file: ref accepted, .. } if accepted == &run_file),
        "bound plus matching run.json was not reconciled as accepted: {state:?}"
    );

    let starts = Arc::new(AtomicUsize::new(0));
    let state = bench.app_state(counting_drivers(Arc::clone(&starts)))?;
    let (sink, _source) = line_channel(QUEUE_CAP);
    let retried = {
        let deps = state
            .begin_triggered_run(bench.project.path(), &delivery.claim)
            .map_err(|said| format!("an accepted delivery could not reconcile: {said}"))?;
        run_triggered_workflow_inner(&deps, &bench.request(), &delivery.claim, sink).await?
    };
    assert!(
        matches!(retried, TriggerRunReport::AlreadyAccepted { ref id, run_file: ref found }
            if id == &delivery.claim.run_id && found == &run_file),
        "retry after the crash did not return the existing acceptance: {retried:?}"
    );
    assert_eq!(
        starts.load(Ordering::Acquire),
        0,
        "reconciliation started a second external agent"
    );
    assert_eq!(
        run_dirs(bench.project.path())?,
        1,
        "reconciliation created a second run directory"
    );
    let next = state
        .begin_run(bench.project.path())
        .map_err(|said| format!("AlreadyAccepted left AppState.live latched: {said}"))?;
    next.control.settle();
    Ok(())
}

#[tokio::test]
async fn a_manual_start_still_mints_its_own_uuid() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let delivery = bench.one_delivery()?;
    let store = Store::open(&bench.db())?;
    let deps = bench.deps(&store, counting_drivers(Arc::new(AtomicUsize::new(0))));
    let (sink, _source) = line_channel(QUEUE_CAP);
    let report = run_workflow_inner(&deps, &bench.request(), sink).await?;
    let parsed = Uuid::parse_str(&report.id)?;
    assert_eq!(parsed.get_version(), Some(Version::SortRand));
    assert_ne!(
        report.id, delivery.claim.run_id,
        "manual Start accidentally reused a trigger's preallocated identity"
    );
    Ok(())
}

fn poll(bench: &Bench, issues: &[Value]) -> Result<TriggerPoll, triggers::TriggerError> {
    let bytes =
        serde_json::to_vec(&json!({"data":{"issues":{"nodes":issues}}})).expect("answer JSON");
    triggers::poll_with(bench.home.path(), "mine", CREATED, |_| Ok(bytes))
}

fn issue(id: &str, identifier: &str, hour: u8) -> Value {
    json!({
        "id": id,
        "identifier": identifier,
        "title": format!("Issue {identifier}"),
        "url": format!("https://linear.app/loadout/issue/{identifier}"),
        "description": "body",
        "updatedAt": format!("2026-08-21T{hour:02}:00:00.000Z")
    })
}

fn issue_ids(deliveries: &[TriggerDelivery]) -> Vec<&str> {
    deliveries
        .iter()
        .map(|delivery| delivery.issue.id.as_str())
        .collect()
}

fn assert_unique_v7_ids(deliveries: &[TriggerDelivery]) -> Result<(), Box<dyn Error>> {
    let mut delivery_ids = deliveries
        .iter()
        .map(|delivery| delivery.claim.delivery_id.as_str())
        .collect::<Vec<_>>();
    delivery_ids.sort_unstable();
    delivery_ids.dedup();
    assert_eq!(
        delivery_ids.len(),
        deliveries.len(),
        "two issues share one delivery id"
    );
    for delivery in deliveries {
        let run = Uuid::parse_str(&delivery.claim.run_id)?;
        assert_eq!(
            run.get_version(),
            Some(Version::SortRand),
            "run id is not UUID v7"
        );
    }
    Ok(())
}

fn assert_accepted_run_file(path: &Path, delivery: &TriggerDelivery) -> Result<(), Box<dyn Error>> {
    let run: Value = serde_json::from_slice(&fs::read(path)?)?;
    assert_eq!(run["id"], delivery.claim.run_id);
    assert_eq!(run["created_at"], delivery.created_at);
    assert_eq!(run["trigger_origin"]["slug"], delivery.claim.slug);
    assert_eq!(
        run["trigger_origin"]["delivery_id"],
        delivery.claim.delivery_id
    );
    assert_eq!(run["trigger_origin"]["issue_id"], delivery.issue.id);
    let text = serde_json::to_string(&run)?;
    assert!(
        !text.contains(KEY) && !text.contains("api_key"),
        "run.json copied the trigger secret"
    );
    Ok(())
}

fn accepted_run_json(delivery: &TriggerDelivery) -> Value {
    json!({
        "id": delivery.claim.run_id,
        "created_at": delivery.created_at,
        "trigger_origin": {
            "slug": delivery.claim.slug,
            "delivery_id": delivery.claim.delivery_id,
            "issue_id": delivery.issue.id
        }
    })
}

fn run_dirs(project: &Path) -> Result<usize, Box<dyn Error>> {
    let path = project.join(".loadout/runs");
    match fs::read_dir(path) {
        Ok(entries) => Ok(entries.filter_map(Result::ok).count()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
}

#[derive(Debug)]
struct Bench {
    home: TempDir,
    project: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("triggers"))?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(home.path().join("agents/witness.md"), AGENT)?;
        fs::write(home.path().join("workflows/ship-it.json"), WORKFLOW)?;
        fs::write(
            home.path().join("triggers/mine.json"),
            serde_json::to_vec_pretty(&json!({
                "schema": 1, "source": "linear", "enabled": true,
                "workflow": "ship-it.json", "condition": "assigned to me", "api_key": KEY
            }))?,
        )?;
        Ok(Self { home, project })
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout/loadout.db")
    }

    fn workflow(&self, slug: &str, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self
            .home
            .path()
            .join("workflows")
            .join(format!("{slug}.json"));
        fs::write(&path, text)?;
        Ok(path)
    }

    fn request(&self) -> RunRequest {
        RunRequest {
            workflow: self.home.path().join("workflows/ship-it.json"),
            how_many_at_once: 1,
            task: Some("LOAD-1: Do the work\n\nbody".to_owned()),
        }
    }

    fn one_delivery(&self) -> Result<TriggerDelivery, Box<dyn Error>> {
        assert_eq!(
            poll(self, &[issue("old", "LOAD-0", 8)])?,
            TriggerPoll::Armed
        );
        let polled = poll(self, &[issue("issue-a", "LOAD-1", 9)])?;
        let TriggerPoll::Pending { delivery } = polled else {
            return Err(format!("the new issue did not become pending: {polled:?}").into());
        };
        Ok(delivery)
    }

    fn deps<'a>(&'a self, store: &'a Store, drivers: Drivers) -> RunDeps<'a> {
        RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store,
            drivers,
            control: RunControl::new(),
        }
    }

    fn app_state(&self, drivers: Drivers) -> Result<AppState, Box<dyn Error>> {
        Ok(AppState::new(
            self.home.path().to_path_buf(),
            self.project.path().to_path_buf(),
            Store::open(&self.db())?,
            drivers,
        ))
    }
}

#[derive(Debug)]
struct Witness {
    home: PathBuf,
    delivery: TriggerDelivery,
    starts: Arc<AtomicUsize>,
    problem: Arc<Mutex<Option<String>>>,
}

#[async_trait]
impl AgentDriver for Witness {
    fn id(&self) -> &'static str {
        "witness"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("test".to_owned()),
        })
    }

    async fn start(
        &self,
        _spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.starts.fetch_add(1, Ordering::AcqRel);
        let result = (|| -> Result<PathBuf, String> {
            let state = triggers::delivery_state(&self.home, &self.delivery.claim)
                .map_err(|error| error.to_string())?;
            let DeliveryState::Accepted { run_file, .. } = state else {
                return Err(format!("the first driver call saw {state:?}, not accepted"));
            };
            assert_accepted_run_file(&run_file, &self.delivery)
                .map_err(|error| error.to_string())?;
            Ok(run_file)
        })();
        if let Err(problem) = result {
            *self.problem.lock().unwrap_or_else(PoisonError::into_inner) = Some(problem);
        }
        let session = SessionRef {
            vendor: self.id(),
            id: "trigger-witness".to_owned(),
        };
        let _ = events
            .send(
                (AgentEvent::Started {
                    session: session.clone(),
                    model: "test".to_owned(),
                    tools: Vec::new(),
                    capabilities: Vec::new(),
                })
                .into(),
            )
            .await;
        Ok(Box::new(Turn { events, session }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
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
            text: "done".to_owned(),
            cost_usd: None,
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

fn inspecting_drivers(
    home: PathBuf,
    delivery: TriggerDelivery,
    starts: Arc<AtomicUsize>,
    problem: Arc<Mutex<Option<String>>>,
) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Witness {
        home,
        delivery,
        starts,
        problem,
    });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

fn counting_drivers(starts: Arc<AtomicUsize>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Counting { starts });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

fn no_agents() -> Drivers {
    counting_drivers(Arc::new(AtomicUsize::new(0)))
}

#[derive(Debug)]
struct Counting {
    starts: Arc<AtomicUsize>,
}

#[async_trait]
impl AgentDriver for Counting {
    fn id(&self) -> &'static str {
        "counting"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: None,
        })
    }

    async fn start(
        &self,
        _spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.starts.fetch_add(1, Ordering::AcqRel);
        let session = SessionRef {
            vendor: self.id(),
            id: "counting".to_owned(),
        };
        let _ = events
            .send(
                (AgentEvent::Started {
                    session: session.clone(),
                    model: String::new(),
                    tools: Vec::new(),
                    capabilities: Vec::new(),
                })
                .into(),
            )
            .await;
        Ok(Box::new(Turn { events, session }))
    }
}
