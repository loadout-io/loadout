//! AC-8 dla T-65: jedna sprawa dostaje jeden trwały claim i najwyżej jedną akceptację biegu.
//!
//! `SQLite` nie bierze udziału w deduplikacji. Każda asercja po restarcie buduje odpowiedź
//! wyłącznie z plików triggera i `run.json` (niezmiennik 4), a żywa część używa tej samej
//! `run_triggered_workflow_inner`, którą opakowuje istniejące `run_workflow`.

#![allow(clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::isolate;
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
use serde::Deserialize;
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
    let pending = pending_deliveries_on_disk(bench.home.path(), "mine")?;
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
    let pending = pending_deliveries_on_disk(bench.home.path(), "mine")?;
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
    let before_restart = pending_deliveries_on_disk(bench.home.path(), "mine")?;
    assert_eq!(
        issue_ids(&before_restart),
        vec!["issue-a"],
        "the cursor write failed before the pending receipt reached disk, so the issue was lost"
    );

    // Nowy odczyt bez żadnego stanu w pamięci procesu jest symulacją restartu.
    let after_restart = pending_deliveries_on_disk(bench.home.path(), "mine")?;
    assert_eq!(
        after_restart, before_restart,
        "restart minted a second delivery or run UUID"
    );
    let recovered = triggers::poll_with(bench.home.path(), "mine", CREATED + 1, |_| {
        Err(triggers::TriggerError::EmptyAnswer)
    })?;
    assert!(
        matches!(
            recovered,
            TriggerPoll::Pending { ref delivery }
                if Some(delivery.as_ref()) == after_restart.first()
        ),
        "the durable receipt stayed hidden behind the same broken cursor on the next tick: \
         {recovered:?}"
    );
    Ok(())
}

#[test]
fn a_clean_cursor_pending_survives_an_offline_poll_with_the_same_ids() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let delivery = bench.one_delivery()?;
    assert!(
        triggers::cursor_path(bench.home.path(), "mine").is_file(),
        "the fixture did not finish its cursor before going offline"
    );
    let mut fetched = false;
    let offline = triggers::poll_with(bench.home.path(), "mine", CREATED + 1, |_| {
        fetched = true;
        Err(triggers::TriggerError::EmptyAnswer)
    })?;
    assert!(fetched, "the clean-cursor fixture skipped its network path");
    assert_eq!(
        offline,
        TriggerPoll::Pending {
            delivery: Box::new(delivery.clone())
        },
        "offline polling hid or reminted the durable pending delivery"
    );
    assert_eq!(
        pending_deliveries_on_disk(bench.home.path(), "mine")?,
        vec![delivery],
        "the offline fallback changed the delivery or run UUID on disk"
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
        triggers::reconcile_delivery(
            bench.home.path(),
            &delivery.claim,
            read_and_sync_fixture_run,
        )?,
        DeliveryState::Accepted { .. }
    ));

    let next = poll(&bench, &[issue("issue-b", "LOAD-2", 10)])?;
    assert!(
        matches!(
            next,
            TriggerPoll::Pending { ref delivery } if delivery.issue.id == "issue-b"
        ),
        "an old Accepted receipt short-circuited the fetch and hid a later issue: {next:?}"
    );
    assert_eq!(
        issue_ids(&pending_deliveries_on_disk(bench.home.path(), "mine")?),
        vec!["issue-b"]
    );
    Ok(())
}

#[tokio::test]
async fn already_going_leaves_the_claim_pending_until_the_run_settles() -> Result<(), Box<dyn Error>>
{
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
        delivery_state_on_disk(bench.home.path(), &delivery.claim)?,
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
        delivery_state_on_disk(bench.home.path(), &delivery.claim)?,
        DeliveryState::Pending,
        "reserving AppState.live accepted the issue before run.json existed"
    );
    reserved.control.settle();
    Ok(())
}

#[tokio::test]
async fn a_forged_claim_is_refused_without_leaving_the_live_latch_taken()
-> Result<(), Box<dyn Error>> {
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
            delivery_state_on_disk(bench.home.path(), &delivery.claim)?,
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
        delivery_state_on_disk(bench.home.path(), &delivery.claim)?,
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
        delivery_state_on_disk(bench.home.path(), &delivery.claim)?,
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
        .join(".loadout/runs")
        .join(format!("20260503-030937__{}", delivery.claim.run_id));
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
        let reconciled = triggers::reconcile_delivery(
            bench.home.path(),
            &delivery.claim,
            read_and_sync_fixture_run,
        );
        assert!(
            !matches!(reconciled, Ok(DeliveryState::Accepted { .. })),
            "reconcile accepted a run.json with mismatched {field}"
        );
        assert!(
            matches!(
                delivery_state_on_disk(bench.home.path(), &delivery.claim)?,
                DeliveryState::Bound { run_file: ref found } if found == &run_file
            ),
            "a mismatched {field} changed the durable delivery state"
        );
    }

    fs::write(
        &run_file,
        serde_json::to_vec_pretty(&accepted_run_json(&delivery))?,
    )?;
    fs::remove_file(bench.home.path().join("workflows/ship-it.json"))?;

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
        "retry after the crash depended on the now-missing workflow instead of the receipt: {retried:?}"
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
async fn reconciliation_stays_bound_until_the_run_file_and_directory_are_durable()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let delivery = bench.one_delivery()?;
    let run_dir = bench
        .project
        .path()
        .join(".loadout/runs")
        .join(format!("20260503-030937__{}", delivery.claim.run_id));
    let run_file = run_dir.join("run.json");
    triggers::bind_delivery(bench.home.path(), &delivery.claim, &run_file)?;
    fs::create_dir_all(&run_dir)?;
    fs::write(
        &run_file,
        serde_json::to_vec_pretty(&accepted_run_json(&delivery))?,
    )?;
    let ledger_file = bench
        .home
        .path()
        .join(triggers::TRIGGERS_DIR)
        .join(".mine.ledger.json");
    let ledger_before = fs::read(&ledger_file)?;

    for attempt in 1..=2 {
        let mut called = false;
        let failed = triggers::reconcile_delivery(bench.home.path(), &delivery.claim, |seen| {
            called = true;
            assert_eq!(seen, run_file.as_path());
            Err(io::Error::other(format!(
                "injected durability failure {attempt}"
            )))
        });
        assert!(called, "durability callback {attempt} was skipped");
        assert!(
            matches!(failed, Err(triggers::TriggerError::RunDurability(_))),
            "durability failure {attempt} was hidden or changed kind: {failed:?}"
        );
        assert_eq!(
            delivery_state_on_disk(bench.home.path(), &delivery.claim)?,
            DeliveryState::Bound {
                run_file: run_file.clone()
            },
            "durability failure {attempt} accepted or released the bound crash receipt"
        );
        assert_eq!(
            fs::read(&ledger_file)?,
            ledger_before,
            "durability failure {attempt} rewrote the ledger before the run was safe"
        );
    }

    fs::remove_file(bench.home.path().join("workflows/ship-it.json"))?;
    let starts = Arc::new(AtomicUsize::new(0));
    let store = Store::open(&bench.db())?;
    let deps = bench.deps(&store, counting_drivers(Arc::clone(&starts)));
    let (sink, _source) = line_channel(QUEUE_CAP);
    let retried =
        run_triggered_workflow_inner(&deps, &bench.request(), &delivery.claim, sink).await?;
    assert!(
        matches!(retried, TriggerRunReport::AlreadyAccepted { ref id, run_file: ref found }
            if id == &delivery.claim.run_id && found == &run_file),
        "successful durability did not reconcile the same run without its workflow: {retried:?}"
    );
    assert!(matches!(
        delivery_state_on_disk(bench.home.path(), &delivery.claim)?,
        DeliveryState::Accepted { run_file: ref found, .. } if found == &run_file
    ));
    assert_eq!(
        starts.load(Ordering::Acquire),
        0,
        "durable reconciliation started a second driver"
    );
    assert_eq!(run_dirs(bench.project.path())?, 1);
    Ok(())
}

#[tokio::test]
async fn crash_before_run_json_reuses_the_bound_run_and_repairs_its_partial_copy()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let delivery = bench.one_delivery()?;
    let fresh_copy = WORKFLOW.replace(
        "\"overrides\": {},",
        "\"overrides\": {},\n    \"folder\": { \"use\": \"fresh-copy\" },",
    );
    fs::write(bench.home.path().join("workflows/ship-it.json"), fresh_copy)?;
    fs::write(bench.project.path().join("source.txt"), "the project")?;

    let run_dir = bench
        .project
        .path()
        .join(".loadout/runs")
        .join(format!("20260503-030937__{}", delivery.claim.run_id));
    let run_file = run_dir.join("run.json");
    let copy = run_dir.join("work/s_ship");
    triggers::bind_delivery(bench.home.path(), &delivery.claim, &run_file)?;
    fs::create_dir_all(&copy)?;
    fs::write(copy.join("half-written.txt"), "not a complete project")?;

    let store = Store::open(&bench.db())?;
    let starts = Arc::new(AtomicUsize::new(0));
    let deps = bench.deps(&store, counting_drivers(Arc::clone(&starts)));
    let (sink, _source) = line_channel(QUEUE_CAP);
    let report =
        run_triggered_workflow_inner(&deps, &bench.request(), &delivery.claim, sink).await?;
    let TriggerRunReport::Ran(report) = report else {
        return Err("bound without run.json was mistaken for an accepted run".into());
    };

    assert_eq!(
        report.dir, run_dir,
        "retry changed the preallocated run UUID"
    );
    assert_eq!(starts.load(Ordering::Acquire), 1);
    assert_eq!(fs::read_to_string(copy.join("source.txt"))?, "the project");
    assert!(
        !copy.join("half-written.txt").exists(),
        "retry reused a partial file copy instead of repairing it"
    );
    Ok(())
}

#[tokio::test]
async fn crash_after_worktree_add_rebuilds_the_dirty_human_diff_before_the_driver()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    git(bench.project.path(), &["init", "--quiet"])?;
    fs::write(bench.project.path().join(".gitignore"), ".loadout/\n")?;
    fs::write(bench.project.path().join("source.txt"), "committed source")?;
    git(bench.project.path(), &["add", "-A"])?;
    git(
        bench.project.path(),
        &["commit", "--quiet", "-m", "the committed base"],
    )?;
    fs::write(
        bench.project.path().join("source.txt"),
        "the human's dirty source",
    )?;

    let delivery = bench.one_delivery()?;
    let fresh_copy = WORKFLOW.replace(
        "\"overrides\": {},",
        "\"overrides\": {},\n    \"folder\": { \"use\": \"fresh-copy\" },",
    );
    fs::write(bench.home.path().join("workflows/ship-it.json"), fresh_copy)?;
    let run_dir = bench
        .project
        .path()
        .join(".loadout/runs")
        .join(format!("20260503-030937__{}", delivery.claim.run_id));
    let run_file = run_dir.join("run.json");
    let copy = run_dir.join("work/s_ship");
    let branch = isolate::branch_for(&delivery.claim.run_id, "s_ship");
    triggers::bind_delivery(bench.home.path(), &delivery.claim, &run_file)?;
    fs::create_dir_all(copy.parent().ok_or("worktree has no parent")?)?;
    let destination = copy.display().to_string();
    git(
        bench.project.path(),
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            &branch,
            &destination,
            "HEAD",
        ],
    )?;
    assert_eq!(
        fs::read_to_string(copy.join("source.txt"))?,
        "committed source",
        "the fixture accidentally applied the dirty diff before the simulated crash"
    );

    let starts = Arc::new(AtomicUsize::new(0));
    let seen_problem = Arc::new(Mutex::new(None));
    let drivers = inspecting_drivers_with_source(
        bench.home.path().to_path_buf(),
        delivery.clone(),
        Arc::clone(&starts),
        Arc::clone(&seen_problem),
        Some("the human's dirty source".to_owned()),
    );
    let store = Store::open(&bench.db())?;
    let deps = bench.deps(&store, drivers);
    let (sink, _source) = line_channel(QUEUE_CAP);
    let report =
        run_triggered_workflow_inner(&deps, &bench.request(), &delivery.claim, sink).await?;
    let TriggerRunReport::Ran(report) = report else {
        return Err("worktree-add without run.json was mistaken for acceptance".into());
    };

    assert_eq!(report.id, delivery.claim.run_id);
    assert_eq!(report.dir, run_dir, "retry minted a second run directory");
    assert_eq!(run_dirs(bench.project.path())?, 1);
    assert_eq!(starts.load(Ordering::Acquire), 1);
    if let Some(problem) = seen_problem
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
    {
        return Err(problem.into());
    }
    assert_eq!(
        fs::read_to_string(copy.join("source.txt"))?,
        "the human's dirty source",
        "retry preserved HEAD but lost the human's dirty tracked content"
    );
    let worktrees = git(bench.project.path(), &["worktree", "list", "--porcelain"])?;
    let named_copy = format!("worktree {}", fs::canonicalize(&copy)?.display());
    assert_eq!(
        worktrees.lines().filter(|line| *line == named_copy).count(),
        1,
        "retry registered zero or two worktrees for the preallocated run"
    );
    Ok(())
}

#[tokio::test]
async fn recovery_refuses_a_symlink_to_an_external_worktree_without_touching_it()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    git(bench.project.path(), &["init", "--quiet"])?;
    fs::write(bench.project.path().join(".gitignore"), ".loadout/\n")?;
    fs::write(
        bench.project.path().join("source.txt"),
        "owned by the victim",
    )?;
    git(bench.project.path(), &["add", "-A"])?;
    git(
        bench.project.path(),
        &["commit", "--quiet", "-m", "the victim base"],
    )?;

    let delivery = bench.one_delivery()?;
    let fresh_copy = WORKFLOW.replace(
        "\"overrides\": {},",
        "\"overrides\": {},\n    \"folder\": { \"use\": \"fresh-copy\" },",
    );
    fs::write(bench.home.path().join("workflows/ship-it.json"), fresh_copy)?;
    let run_dir = bench
        .project
        .path()
        .join(".loadout/runs")
        .join(format!("20260503-030937__{}", delivery.claim.run_id));
    let run_file = run_dir.join("run.json");
    let copy = run_dir.join("work/s_ship");
    let branch = isolate::branch_for(&delivery.claim.run_id, "s_ship");
    let victim = bench.home.path().join("external-victim-worktree");
    let victim_text = "owned by the victim";
    let destination = victim.display().to_string();
    git(
        bench.project.path(),
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            &branch,
            &destination,
            "HEAD",
        ],
    )?;
    let victim_before = snapshot_tree(&victim)?;

    triggers::bind_delivery(bench.home.path(), &delivery.claim, &run_file)?;
    fs::create_dir_all(copy.parent().ok_or("worktree path has no parent")?)?;
    std::os::unix::fs::symlink(&victim, &copy)?;

    let store = Store::open(&bench.db())?;
    let starts = Arc::new(AtomicUsize::new(0));
    let deps = bench.deps(&store, counting_drivers(Arc::clone(&starts)));
    let (sink, _source) = line_channel(QUEUE_CAP);
    let refused =
        run_triggered_workflow_inner(&deps, &bench.request(), &delivery.claim, sink).await;
    let said = refused
        .expect_err("a symlink at the reserved worktree path reached cleanup")
        .to_string();
    assert!(
        said.contains("file-copy path"),
        "the symlink was refused by an unrelated check: {said}"
    );
    assert_eq!(
        starts.load(Ordering::Acquire),
        0,
        "a driver started through the external worktree symlink"
    );
    assert!(victim.is_dir(), "recovery deleted the external worktree");
    assert_eq!(
        fs::read_to_string(victim.join("source.txt"))?,
        victim_text,
        "recovery changed the external worktree"
    );
    assert_eq!(
        snapshot_tree(&victim)?,
        victim_before,
        "recovery changed files in the external worktree"
    );
    assert!(
        !run_dir.join(".isolation/s_ship").exists(),
        "recovery authorized cleanup before rejecting the symlink"
    );
    assert_eq!(
        delivery_state_on_disk(bench.home.path(), &delivery.claim)?,
        DeliveryState::Pending,
        "rejecting the symlink consumed its delivery"
    );
    Ok(())
}

#[tokio::test]
async fn recovery_refuses_a_symlinked_work_root_without_touching_the_external_worktree()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    git(bench.project.path(), &["init", "--quiet"])?;
    fs::write(bench.project.path().join(".gitignore"), ".loadout/\n")?;
    fs::write(bench.project.path().join("source.txt"), "external parent")?;
    git(bench.project.path(), &["add", "-A"])?;
    git(
        bench.project.path(),
        &["commit", "--quiet", "-m", "the external parent base"],
    )?;

    let delivery = bench.one_delivery()?;
    let fresh_copy = WORKFLOW.replace(
        "\"overrides\": {},",
        "\"overrides\": {},\n    \"folder\": { \"use\": \"fresh-copy\" },",
    );
    fs::write(bench.home.path().join("workflows/ship-it.json"), fresh_copy)?;
    let run_dir = bench
        .project
        .path()
        .join(".loadout/runs")
        .join(format!("20260503-030937__{}", delivery.claim.run_id));
    let run_file = run_dir.join("run.json");
    let copy = run_dir.join("work/s_ship");
    let branch = isolate::branch_for(&delivery.claim.run_id, "s_ship");
    let external_parent = bench.home.path().join("external-work-root");
    let victim = external_parent.join("s_ship");
    fs::create_dir_all(&external_parent)?;
    let destination = victim.display().to_string();
    git(
        bench.project.path(),
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            &branch,
            &destination,
            "HEAD",
        ],
    )?;
    let victim_before = snapshot_tree(&victim)?;

    triggers::bind_delivery(bench.home.path(), &delivery.claim, &run_file)?;
    fs::create_dir_all(&run_dir)?;
    std::os::unix::fs::symlink(&external_parent, run_dir.join("work"))?;

    let store = Store::open(&bench.db())?;
    let starts = Arc::new(AtomicUsize::new(0));
    let deps = bench.deps(&store, counting_drivers(Arc::clone(&starts)));
    let (sink, _source) = line_channel(QUEUE_CAP);
    let refused =
        run_triggered_workflow_inner(&deps, &bench.request(), &delivery.claim, sink).await;
    let said = refused
        .expect_err("a symlinked work root reached recovery cleanup")
        .to_string();
    assert!(
        said.contains("run path"),
        "the symlinked work root was refused by an unrelated check: {said}"
    );
    assert_eq!(starts.load(Ordering::Acquire), 0);
    assert!(victim.is_dir(), "recovery deleted the external worktree");
    assert_eq!(
        snapshot_tree(&victim)?,
        victim_before,
        "recovery changed the external worktree"
    );
    assert!(
        !run_dir.join(".isolation/s_ship").exists(),
        "recovery wrote an authorization marker through the symlinked work root"
    );
    assert_eq!(
        delivery_state_on_disk(bench.home.path(), &delivery.claim)?,
        DeliveryState::Pending,
        "rejecting the symlinked work root consumed its delivery"
    );
    assert_eq!(copy.canonicalize()?, victim.canonicalize()?);
    Ok(())
}

#[tokio::test]
async fn recovery_never_reads_an_authorization_through_a_marker_link() -> Result<(), Box<dyn Error>>
{
    for attack in ["directory", "file"] {
        let bench = Bench::new()?;
        git(bench.project.path(), &["init", "--quiet"])?;
        fs::write(bench.project.path().join(".gitignore"), ".loadout/\n")?;
        fs::write(bench.project.path().join("source.txt"), "victim source")?;
        git(bench.project.path(), &["add", "-A"])?;
        git(
            bench.project.path(),
            &["commit", "--quiet", "-m", "the marker-link base"],
        )?;
        let delivery = bench.one_delivery()?;
        let fresh_copy = WORKFLOW.replace(
            "\"overrides\": {},",
            "\"overrides\": {},\n    \"folder\": { \"use\": \"fresh-copy\" },",
        );
        fs::write(bench.home.path().join("workflows/ship-it.json"), fresh_copy)?;
        let run_dir = bench
            .project
            .path()
            .join(".loadout/runs")
            .join(format!("20260503-030937__{}", delivery.claim.run_id));
        let run_file = run_dir.join("run.json");
        let copy = run_dir.join("work/s_ship");
        let branch = isolate::branch_for(&delivery.claim.run_id, "s_ship");
        triggers::bind_delivery(bench.home.path(), &delivery.claim, &run_file)?;
        fs::create_dir_all(copy.parent().ok_or("worktree path has no parent")?)?;
        let destination = copy.display().to_string();
        git(
            bench.project.path(),
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                &branch,
                &destination,
                "HEAD",
            ],
        )?;
        let head = git(
            bench.project.path(),
            &["rev-parse", "--verify", &format!("refs/heads/{branch}")],
        )?;
        let copy_before = snapshot_tree(&copy)?;
        let external = bench.home.path().join("external-marker");
        fs::create_dir_all(&external)?;
        fs::write(
            external.join("s_ship"),
            serde_json::to_vec(&json!({
                "state": "recovering",
                "branch": branch,
                "head": head.trim()
            }))?,
        )?;
        let marker_root = run_dir.join(".isolation");
        if attack == "directory" {
            std::os::unix::fs::symlink(&external, &marker_root)?;
        } else {
            fs::create_dir_all(&marker_root)?;
            std::os::unix::fs::symlink(external.join("s_ship"), marker_root.join("s_ship"))?;
        }

        let starts = Arc::new(AtomicUsize::new(0));
        let store = Store::open(&bench.db())?;
        let deps = bench.deps(&store, counting_drivers(Arc::clone(&starts)));
        let (sink, _source) = line_channel(QUEUE_CAP);
        let refused =
            run_triggered_workflow_inner(&deps, &bench.request(), &delivery.claim, sink).await;
        let said = refused
            .expect_err("a linked marker authorized recovery cleanup")
            .to_string();
        let expected = if attack == "directory" {
            "run path"
        } else {
            "file-copy path"
        };
        assert!(said.contains(expected), "{attack}: {said}");
        assert_eq!(starts.load(Ordering::Acquire), 0, "{attack}");
        assert_eq!(snapshot_tree(&copy)?, copy_before, "{attack}");
        assert_eq!(
            delivery_state_on_disk(bench.home.path(), &delivery.claim)?,
            DeliveryState::Pending,
            "{attack}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn recovery_recognizes_one_stale_admin_entry_without_a_synthetic_marker()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    git(bench.project.path(), &["init", "--quiet"])?;
    fs::write(bench.project.path().join(".gitignore"), ".loadout/\n")?;
    fs::write(bench.project.path().join("source.txt"), "committed source")?;
    git(bench.project.path(), &["add", "-A"])?;
    git(
        bench.project.path(),
        &["commit", "--quiet", "-m", "the recovery base"],
    )?;
    fs::write(
        bench.project.path().join("source.txt"),
        "the human's recovered source",
    )?;

    let delivery = bench.one_delivery()?;
    let fresh_copy = WORKFLOW.replace(
        "\"overrides\": {},",
        "\"overrides\": {},\n    \"folder\": { \"use\": \"fresh-copy\" },",
    );
    fs::write(bench.home.path().join("workflows/ship-it.json"), fresh_copy)?;
    let run_dir = bench
        .project
        .path()
        .join(".loadout/runs")
        .join(format!("20260503-030937__{}", delivery.claim.run_id));
    let run_file = run_dir.join("run.json");
    let copy = run_dir.join("work/s_ship");
    let branch = isolate::branch_for(&delivery.claim.run_id, "s_ship");
    triggers::bind_delivery(bench.home.path(), &delivery.claim, &run_file)?;
    fs::create_dir_all(copy.parent().ok_or("worktree path has no parent")?)?;
    leave_missing_worktree(bench.project.path(), &copy, &branch, false)?;
    let unrelated = bench.home.path().join("unrelated-stale-worktree");
    leave_missing_worktree(bench.project.path(), &unrelated, "unrelated-stale", false)?;
    assert!(
        !run_dir.join(".isolation/s_ship").exists(),
        "the crash fixture accidentally seeded a recovery marker"
    );
    let stale = git(bench.project.path(), &["worktree", "list", "--porcelain"])?;
    assert!(
        stale.contains(&format!("branch refs/heads/{branch}")) && stale.contains("prunable"),
        "the fixture did not leave the expected stale administrative entry: {stale}"
    );

    let starts = Arc::new(AtomicUsize::new(0));
    let store = Store::open(&bench.db())?;
    let deps = bench.deps(&store, counting_drivers(Arc::clone(&starts)));
    let (sink, _source) = line_channel(QUEUE_CAP);
    let report =
        run_triggered_workflow_inner(&deps, &bench.request(), &delivery.claim, sink).await?;
    let TriggerRunReport::Ran(report) = report else {
        return Err("stale administration was mistaken for an accepted run".into());
    };

    assert_eq!(report.id, delivery.claim.run_id);
    assert_eq!(report.dir, run_dir, "retry minted a second run directory");
    assert_eq!(run_dirs(bench.project.path())?, 1);
    assert_eq!(starts.load(Ordering::Acquire), 1);
    assert_eq!(
        fs::read_to_string(copy.join("source.txt"))?,
        "the human's recovered source"
    );
    let worktrees = git(bench.project.path(), &["worktree", "list", "--porcelain"])?;
    let named_copy = format!("worktree {}", fs::canonicalize(&copy)?.display());
    assert_eq!(
        worktrees.lines().filter(|line| *line == named_copy).count(),
        1,
        "retry left a stale entry beside the recreated worktree"
    );
    let named_branch = format!("branch refs/heads/{branch}");
    assert_eq!(
        worktrees
            .lines()
            .filter(|line| *line == named_branch)
            .count(),
        1,
        "the expected branch is registered by zero or two worktrees"
    );
    assert!(
        worktrees.contains("branch refs/heads/unrelated-stale") && worktrees.contains("prunable"),
        "targeted recovery removed an unrelated stale worktree: {worktrees}"
    );
    Ok(())
}

#[tokio::test]
async fn recovery_refuses_a_locked_missing_worktree_without_unlocking_or_removing_it()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    git(bench.project.path(), &["init", "--quiet"])?;
    fs::write(bench.project.path().join(".gitignore"), ".loadout/\n")?;
    fs::write(bench.project.path().join("source.txt"), "locked source")?;
    git(bench.project.path(), &["add", "-A"])?;
    git(
        bench.project.path(),
        &["commit", "--quiet", "-m", "the locked base"],
    )?;

    let delivery = bench.one_delivery()?;
    let fresh_copy = WORKFLOW.replace(
        "\"overrides\": {},",
        "\"overrides\": {},\n    \"folder\": { \"use\": \"fresh-copy\" },",
    );
    fs::write(bench.home.path().join("workflows/ship-it.json"), fresh_copy)?;
    let run_dir = bench
        .project
        .path()
        .join(".loadout/runs")
        .join(format!("20260503-030937__{}", delivery.claim.run_id));
    let run_file = run_dir.join("run.json");
    let copy = run_dir.join("work/s_ship");
    let branch = isolate::branch_for(&delivery.claim.run_id, "s_ship");
    triggers::bind_delivery(bench.home.path(), &delivery.claim, &run_file)?;
    fs::create_dir_all(copy.parent().ok_or("worktree path has no parent")?)?;
    let head = leave_missing_worktree(bench.project.path(), &copy, &branch, true)?;
    fs::create_dir_all(run_dir.join(".isolation"))?;
    fs::write(
        run_dir.join(".isolation/s_ship"),
        serde_json::to_vec(&json!({
            "state": "recovering",
            "branch": branch,
            "head": head
        }))?,
    )?;
    let before = git(bench.project.path(), &["worktree", "list", "--porcelain"])?;
    assert!(before.contains("locked owned by the fixture"));

    let starts = Arc::new(AtomicUsize::new(0));
    let store = Store::open(&bench.db())?;
    let deps = bench.deps(&store, counting_drivers(Arc::clone(&starts)));
    let (sink, _source) = line_channel(QUEUE_CAP);
    let refused =
        run_triggered_workflow_inner(&deps, &bench.request(), &delivery.claim, sink).await;
    let said = refused
        .expect_err("a locked worktree authorized recovery cleanup")
        .to_string();
    assert!(said.contains("nothing was removed"), "{said}");
    assert_eq!(starts.load(Ordering::Acquire), 0);
    assert_eq!(
        delivery_state_on_disk(bench.home.path(), &delivery.claim)?,
        DeliveryState::Pending
    );
    let after = git(bench.project.path(), &["worktree", "list", "--porcelain"])?;
    assert!(
        after.contains(&format!("branch refs/heads/{branch}"))
            && after.contains("locked owned by the fixture"),
        "recovery unlocked or removed the protected worktree: {after}"
    );
    assert!(run_dir.join(".isolation/s_ship").is_file());
    Ok(())
}

#[tokio::test]
async fn every_workflow_refuses_a_linked_run_path_before_external_write_or_binding()
-> Result<(), Box<dyn Error>> {
    for attack in ["runs", "run"] {
        let bench = Bench::new()?;
        let delivery = bench.one_delivery()?;
        let run_dir = bench
            .project
            .path()
            .join(".loadout/runs")
            .join(format!("20260503-030937__{}", delivery.claim.run_id));
        let external = bench.home.path().join(format!("external-{attack}-victim"));
        fs::create_dir_all(&external)?;
        fs::write(external.join("keep.txt"), format!("keep-{attack}"))?;
        let before = snapshot_tree(&external)?;
        if attack == "runs" {
            std::os::unix::fs::symlink(&external, bench.project.path().join(".loadout/runs"))?;
        } else {
            fs::create_dir_all(
                run_dir
                    .parent()
                    .ok_or("generated run directory has no parent")?,
            )?;
            std::os::unix::fs::symlink(&external, &run_dir)?;
        }

        let starts = Arc::new(AtomicUsize::new(0));
        let store = Store::open(&bench.db())?;
        let deps = bench.deps(&store, counting_drivers(Arc::clone(&starts)));
        let (sink, _source) = line_channel(QUEUE_CAP);
        let refused =
            run_triggered_workflow_inner(&deps, &bench.request(), &delivery.claim, sink).await;
        let said = refused
            .expect_err("a linked generated run path reached the driver")
            .to_string();
        assert!(said.contains("run path"), "{attack}: {said}");
        assert_eq!(starts.load(Ordering::Acquire), 0, "{attack}");
        assert_eq!(snapshot_tree(&external)?, before, "{attack}");
        assert_eq!(
            delivery_state_on_disk(bench.home.path(), &delivery.claim)?,
            DeliveryState::Pending,
            "{attack}: refusing before the run directory still bound the claim"
        );
    }
    Ok(())
}

#[tokio::test]
async fn manual_start_without_a_fresh_copy_uses_the_same_safe_run_root()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let external = bench.home.path().join("external-manual-runs");
    fs::create_dir_all(&external)?;
    fs::write(external.join("keep.txt"), "manual victim")?;
    let before = snapshot_tree(&external)?;
    std::os::unix::fs::symlink(&external, bench.project.path().join(".loadout/runs"))?;

    let starts = Arc::new(AtomicUsize::new(0));
    let store = Store::open(&bench.db())?;
    let deps = bench.deps(&store, counting_drivers(Arc::clone(&starts)));
    let (sink, _source) = line_channel(QUEUE_CAP);
    let refused = run_workflow_inner(&deps, &bench.request(), sink).await;
    let said = refused
        .expect_err("manual Start followed the linked runs root")
        .to_string();
    assert!(said.contains("run path"), "{said}");
    assert_eq!(starts.load(Ordering::Acquire), 0);
    assert_eq!(snapshot_tree(&external)?, before);
    Ok(())
}

#[tokio::test]
async fn a_project_opened_through_a_link_still_uses_its_real_generated_children()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let delivery = bench.one_delivery()?;
    let project_link = bench.home.path().join("project-link");
    std::os::unix::fs::symlink(bench.project.path(), &project_link)?;
    let store = Store::open(&bench.db())?;
    let starts = Arc::new(AtomicUsize::new(0));
    let deps = RunDeps {
        home: bench.home.path(),
        project: &project_link,
        store: &store,
        drivers: counting_drivers(Arc::clone(&starts)),
        control: RunControl::new(),
    };
    let (sink, _source) = line_channel(QUEUE_CAP);
    let report =
        run_triggered_workflow_inner(&deps, &bench.request(), &delivery.claim, sink).await?;
    let TriggerRunReport::Ran(report) = report else {
        return Err("a safe project-root link was mistaken for a replay".into());
    };
    assert_eq!(starts.load(Ordering::Acquire), 1);
    assert!(report.dir.starts_with(&project_link));
    assert!(report.dir.join("run.json").is_file());
    Ok(())
}

#[tokio::test]
async fn a_linked_logs_directory_is_refused_before_binding_or_writing() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let delivery = bench.one_delivery()?;
    let run_dir = bench
        .project
        .path()
        .join(".loadout/runs")
        .join(format!("20260503-030937__{}", delivery.claim.run_id));
    fs::create_dir_all(&run_dir)?;
    let external = bench.home.path().join("external-logs");
    fs::create_dir_all(&external)?;
    fs::write(external.join("keep.txt"), "logs victim")?;
    let before = snapshot_tree(&external)?;
    std::os::unix::fs::symlink(&external, run_dir.join("logs"))?;

    let starts = Arc::new(AtomicUsize::new(0));
    let store = Store::open(&bench.db())?;
    let deps = bench.deps(&store, counting_drivers(Arc::clone(&starts)));
    let (sink, _source) = line_channel(QUEUE_CAP);
    let refused =
        run_triggered_workflow_inner(&deps, &bench.request(), &delivery.claim, sink).await;
    let said = refused
        .expect_err("a linked logs directory reached the driver")
        .to_string();
    assert!(said.contains("run path"), "{said}");
    assert_eq!(starts.load(Ordering::Acquire), 0);
    assert_eq!(snapshot_tree(&external)?, before);
    assert_eq!(
        delivery_state_on_disk(bench.home.path(), &delivery.claim)?,
        DeliveryState::Pending,
        "the linked logs directory bound or accepted the claim"
    );
    Ok(())
}

#[tokio::test]
async fn a_bound_claim_never_reconciles_a_matching_run_file_through_a_link()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let delivery = bench.one_delivery()?;
    let run_dir = bench
        .project
        .path()
        .join(".loadout/runs")
        .join(format!("20260503-030937__{}", delivery.claim.run_id));
    let run_file = run_dir.join("run.json");
    triggers::bind_delivery(bench.home.path(), &delivery.claim, &run_file)?;
    fs::create_dir_all(
        run_dir
            .parent()
            .ok_or("generated run directory has no parent")?,
    )?;
    let external = bench.home.path().join("external-bound-run");
    fs::create_dir_all(&external)?;
    fs::write(
        external.join("run.json"),
        serde_json::to_vec_pretty(&accepted_run_json(&delivery))?,
    )?;
    std::os::unix::fs::symlink(&external, &run_dir)?;
    let before = snapshot_tree(&external)?;

    let polled = poll(&bench, &[])?;
    assert!(
        matches!(
            polled,
            TriggerPoll::Pending {
                delivery: ref pending
            }
                if pending.as_ref() == &delivery
        ),
        "polling reconciled an unproved external run file instead of returning the bound claim: \
         {polled:?}"
    );
    assert_eq!(
        delivery_state_on_disk(bench.home.path(), &delivery.claim)?,
        DeliveryState::Bound {
            run_file: run_file.clone()
        },
        "the polling path promoted an external run file without the run-directory proof"
    );
    assert_eq!(snapshot_tree(&external)?, before);

    let starts = Arc::new(AtomicUsize::new(0));
    let store = Store::open(&bench.db())?;
    let deps = bench.deps(&store, counting_drivers(Arc::clone(&starts)));
    let (sink, _source) = line_channel(QUEUE_CAP);
    let refused =
        run_triggered_workflow_inner(&deps, &bench.request(), &delivery.claim, sink).await;
    let said = refused
        .expect_err("a matching external run file was reconciled before path proof")
        .to_string();
    assert!(said.contains("run path"), "{said}");
    assert_eq!(starts.load(Ordering::Acquire), 0);
    assert_eq!(snapshot_tree(&external)?, before);
    assert_eq!(
        delivery_state_on_disk(bench.home.path(), &delivery.claim)?,
        DeliveryState::Pending,
        "the linked external file was accepted or stayed bound after a safe refusal"
    );
    Ok(())
}

#[tokio::test]
async fn a_nested_attachment_link_is_refused_before_a_long_result_can_follow_it()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let delivery = bench.one_delivery()?;
    let run_dir = bench
        .project
        .path()
        .join(".loadout/runs")
        .join(format!("20260503-030937__{}", delivery.claim.run_id));
    let attachments = run_dir.join("attachments");
    fs::create_dir_all(&attachments)?;
    let victim = bench.home.path().join("nested-attachment-victim.md");
    let victim_before = b"a result must never overwrite this external file";
    fs::write(&victim, victim_before)?;
    std::os::unix::fs::symlink(&victim, attachments.join("00__ship__findings__full.md"))?;

    let starts = Arc::new(AtomicUsize::new(0));
    let store = Store::open(&bench.db())?;
    let deps = bench.deps(
        &store,
        counting_drivers_with_text(Arc::clone(&starts), "x".repeat(16_384)),
    );
    let (sink, _source) = line_channel(QUEUE_CAP);
    let refused =
        run_triggered_workflow_inner(&deps, &bench.request(), &delivery.claim, sink).await;
    assert_eq!(
        fs::read(&victim)?,
        victim_before,
        "the long handoff followed the nested attachment link and overwrote the external file"
    );
    let said = refused
        .expect_err("a nested attachment link reached the driver")
        .to_string();
    assert!(said.contains("run path"), "{said}");
    assert_eq!(starts.load(Ordering::Acquire), 0);
    assert_eq!(
        delivery_state_on_disk(bench.home.path(), &delivery.claim)?,
        DeliveryState::Pending,
        "the nested artifact link was accepted before refusal"
    );
    Ok(())
}

#[tokio::test]
async fn a_linked_run_file_staging_path_never_truncates_or_accepts() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let delivery = bench.one_delivery()?;
    let run_dir = bench
        .project
        .path()
        .join(".loadout/runs")
        .join(format!("20260503-030937__{}", delivery.claim.run_id));
    fs::create_dir_all(&run_dir)?;
    let victim = bench.home.path().join("run-file-victim.txt");
    let victim_before = b"must stay byte-for-byte unchanged";
    fs::write(&victim, victim_before)?;
    std::os::unix::fs::symlink(&victim, run_dir.join("run.json.writing"))?;

    let starts = Arc::new(AtomicUsize::new(0));
    let store = Store::open(&bench.db())?;
    let deps = bench.deps(&store, counting_drivers(Arc::clone(&starts)));
    let (sink, _source) = line_channel(QUEUE_CAP);
    let refused =
        run_triggered_workflow_inner(&deps, &bench.request(), &delivery.claim, sink).await;
    let said = refused
        .expect_err("a linked run staging file reached the driver")
        .to_string();
    assert!(said.contains("staging path"), "{said}");
    assert_eq!(starts.load(Ordering::Acquire), 0);
    assert_eq!(fs::read(&victim)?, victim_before);
    assert_eq!(
        delivery_state_on_disk(bench.home.path(), &delivery.claim)?,
        DeliveryState::Pending,
        "refusing the staging link accepted or kept the delivery bound"
    );
    assert!(!run_dir.join("run.json").exists());
    Ok(())
}

#[tokio::test]
async fn a_fresh_copy_key_cannot_escape_or_nest_under_the_runs_work_folder()
-> Result<(), Box<dyn Error>> {
    for attack in ["parent", "nested", "absolute"] {
        let bench = Bench::new()?;
        let delivery = bench.one_delivery()?;
        let run_dir = bench
            .project
            .path()
            .join(".loadout/runs")
            .join(format!("20260503-030937__{}", delivery.claim.run_id));
        let outside = TempDir::new_in(
            bench
                .project
                .path()
                .parent()
                .ok_or("project temp directory has no parent")?,
        )?;
        // Rodzic `work` istnieje przed atakiem. Bez niego system nie potrafi rozwiazac
        // `work/..`, wiec wadliwa implementacja moglaby pasc na ENOENT, zanim dotknie ofiary.
        fs::create_dir_all(run_dir.join("work"))?;
        let (malicious, victim, existed) = match attack {
            "parent" => {
                let victim = outside.path().join("parent-victim");
                let outside_name = outside
                    .path()
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or("outside temp directory has no UTF-8 name")?;
                (
                    format!("../../../../../{outside_name}/parent-victim"),
                    victim,
                    true,
                )
            }
            "nested" => (
                "nested/victim".to_owned(),
                run_dir.join("work/nested/victim"),
                false,
            ),
            "absolute" => (
                outside.path().join("absolute-victim").display().to_string(),
                outside.path().join("absolute-victim"),
                false,
            ),
            _ => unreachable!("the attack matrix is exhaustive"),
        };
        let marker = victim.join("keep.txt");
        if existed {
            fs::create_dir_all(&victim)?;
            fs::write(&marker, format!("keep-{attack}"))?;
        }
        let victim_before = existed.then(|| snapshot_tree(&victim)).transpose()?;

        let mut workflow: Value = serde_json::from_str(WORKFLOW)?;
        workflow["steps"][0]["id"] = json!(malicious);
        workflow["steps"][0]["folder"] = json!({"use": "fresh-copy"});
        fs::write(
            bench.home.path().join("workflows/ship-it.json"),
            serde_json::to_vec_pretty(&workflow)?,
        )?;

        let store = Store::open(&bench.db())?;
        let starts = Arc::new(AtomicUsize::new(0));
        let deps = bench.deps(&store, counting_drivers(Arc::clone(&starts)));
        let (sink, _source) = line_channel(QUEUE_CAP);
        let refused =
            run_triggered_workflow_inner(&deps, &bench.request(), &delivery.claim, sink).await;
        let said = refused
            .expect_err("a malicious fresh-copy path reached the layout")
            .to_string();
        assert!(
            said.contains("file-copy path"),
            "{attack} was refused by an unrelated check instead of the path boundary: {said}"
        );
        assert_eq!(
            starts.load(Ordering::Acquire),
            0,
            "a driver started before the {attack} path was refused"
        );
        assert_eq!(
            delivery_state_on_disk(bench.home.path(), &delivery.claim)?,
            DeliveryState::Pending,
            "the {attack} refusal consumed its delivery"
        );
        if existed {
            assert_eq!(
                snapshot_tree(&victim)?,
                victim_before.ok_or("an existing victim had no before snapshot")?,
                "the {attack} path changed the victim before refusing"
            );
        } else {
            assert!(
                !victim.exists(),
                "the nested path created folders before refusing"
            );
        }
    }
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

fn read_and_sync_fixture_run(path: &Path) -> io::Result<Option<Vec<u8>>> {
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut raw = Vec::new();
    file.read_to_end(&mut raw)?;
    file.sync_all()?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("fixture run has no parent"))?;
    fs::File::open(parent)?.sync_all()?;
    Ok(Some(raw))
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

#[derive(Debug, Deserialize)]
struct LedgerView {
    deliveries: Vec<LedgerRecordView>,
}

#[derive(Debug, Deserialize)]
struct LedgerRecordView {
    delivery: TriggerDelivery,
    state: DeliveryState,
}

fn ledger_on_disk(home: &Path, slug: &str) -> Result<LedgerView, Box<dyn Error>> {
    let path = home
        .join(triggers::TRIGGERS_DIR)
        .join(format!(".{slug}.ledger.json"));
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn pending_deliveries_on_disk(
    home: &Path,
    slug: &str,
) -> Result<Vec<TriggerDelivery>, Box<dyn Error>> {
    Ok(ledger_on_disk(home, slug)?
        .deliveries
        .into_iter()
        .filter(|record| {
            matches!(
                &record.state,
                DeliveryState::Pending | DeliveryState::Bound { .. }
            )
        })
        .map(|record| record.delivery)
        .collect())
}

fn delivery_state_on_disk(
    home: &Path,
    claim: &triggers::TriggerClaim,
) -> Result<DeliveryState, Box<dyn Error>> {
    ledger_on_disk(home, &claim.slug)?
        .deliveries
        .into_iter()
        .find(|record| &record.delivery.claim == claim)
        .map(|record| record.state)
        .ok_or_else(|| "the delivery is missing from its durable ledger".into())
}

fn accepted_run_file(
    home: &Path,
    claim: &triggers::TriggerClaim,
) -> Result<PathBuf, Box<dyn Error>> {
    match delivery_state_on_disk(home, claim)? {
        DeliveryState::Accepted { run_file, .. } => Ok(run_file),
        state => Err(format!("the durable ledger contains {state:?}, not accepted").into()),
    }
}

type TreeSnapshot = Vec<(PathBuf, Option<Vec<u8>>)>;

fn snapshot_tree(root: &Path) -> Result<TreeSnapshot, Box<dyn Error>> {
    fn visit(root: &Path, at: &Path, out: &mut TreeSnapshot) -> Result<(), Box<dyn Error>> {
        let mut entries = fs::read_dir(at)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let relative = path.strip_prefix(root)?.to_path_buf();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                out.push((relative, None));
                visit(root, &path, out)?;
            } else if kind.is_file() {
                out.push((relative, Some(fs::read(path)?)));
            }
        }
        Ok(())
    }

    let mut snapshot = Vec::new();
    visit(root, root, &mut snapshot)?;
    Ok(snapshot)
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
                "workflow": "ship-it.json", "condition": "assigned-to-me", "api_key": KEY
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
        Ok(*delivery)
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
    expected_source: Option<String>,
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
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.starts.fetch_add(1, Ordering::AcqRel);
        let result = (|| -> Result<PathBuf, String> {
            if let Some(expected) = self.expected_source.as_deref() {
                let actual = fs::read_to_string(spec.cwd.join("source.txt"))
                    .map_err(|error| error.to_string())?;
                if actual != expected {
                    return Err(format!(
                        "the first driver saw {actual:?}, not the dirty source {expected:?}"
                    ));
                }
            }
            let state = triggers::accepted_while_busy(&self.home, &self.delivery.claim.slug)
                .map_err(|error| error.to_string())?;
            let Some(TriggerPoll::Accepted { receipt_at, .. }) = state else {
                return Err(format!("the first driver call saw {state:?}, not accepted"));
            };
            if receipt_at < self.delivery.created_at {
                return Err("the first driver call saw an older acceptance".to_owned());
            }
            let run_file = accepted_run_file(&self.home, &self.delivery.claim)
                .map_err(|error| error.to_string())?;
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
        Ok(Box::new(Turn {
            events,
            session,
            text: "done".to_owned(),
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    text: String,
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
            text: self.text.clone(),
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
    inspecting_drivers_with_source(home, delivery, starts, problem, None)
}

fn inspecting_drivers_with_source(
    home: PathBuf,
    delivery: TriggerDelivery,
    starts: Arc<AtomicUsize>,
    problem: Arc<Mutex<Option<String>>>,
    expected_source: Option<String>,
) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Witness {
        home,
        delivery,
        starts,
        problem,
        expected_source,
    });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

fn counting_drivers(starts: Arc<AtomicUsize>) -> Drivers {
    counting_drivers_with_text(starts, "done".to_owned())
}

fn counting_drivers_with_text(starts: Arc<AtomicUsize>, text: String) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Counting { starts, text });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

fn no_agents() -> Drivers {
    counting_drivers(Arc::new(AtomicUsize::new(0)))
}

fn git(at: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(at)
        .args(["-c", "user.name=Loadout Test"])
        .args(["-c", "user.email=test@loadout.invalid"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn leave_missing_worktree(
    project: &Path,
    path: &Path,
    branch: &str,
    locked: bool,
) -> Result<String, Box<dyn Error>> {
    let destination = path.display().to_string();
    git(
        project,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            branch,
            &destination,
            "HEAD",
        ],
    )?;
    let head = git(
        project,
        &["rev-parse", "--verify", &format!("refs/heads/{branch}")],
    )?;
    if locked {
        git(
            project,
            &[
                "worktree",
                "lock",
                "--reason",
                "owned by the fixture",
                &destination,
            ],
        )?;
    }
    fs::remove_dir_all(path)?;
    Ok(head.trim().to_owned())
}

#[derive(Debug)]
struct Counting {
    starts: Arc<AtomicUsize>,
    text: String,
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
        Ok(Box::new(Turn {
            events,
            session,
            text: self.text.clone(),
        }))
    }
}
