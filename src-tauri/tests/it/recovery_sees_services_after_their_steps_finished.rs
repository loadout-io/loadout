//! WF-25: skończony kafelek Serve nie jest dowodem śmierci jego usługi.
//! Rekord własności powstaje przez prawdziwe `start_owned`; reaper jest podstawiany tylko
//! w scenach niepewnego wyniku. Jeden przypadek idzie pełną produkcyjną eskalacją.

#![allow(clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::isolate;
use loadout_lib::commands::processes::{Processes, ServiceOwner, ServiceRef};
use loadout_lib::commands::reconcile::{reconcile_runs, with_reaper};
use loadout_lib::engine::drivers::command::StartSpec;
use loadout_lib::engine::supervisor::{self, GroupProof, StepTag};
use loadout_lib::recovery::ReapOutcome;
use loadout_lib::workflow::ServiceLifetime;
use serde_json::{Value, json};
use tempfile::TempDir;

const RUN: &str = "01950000-0000-7000-8000-000000002501";
const STEP: &str = "01950000-0000-7000-8000-000000002502";
const SERVICE: &str = "01950000-0000-7000-8000-000000002503";
const DIR: &str = "20260905-010000__01950000-0000-7000-8000-000000002501";
const FAKE_GROUP: i32 = 725_251;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_finished_serve_reaches_recovery_and_only_proven_death_is_saved()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::stopped().await?;
    bench.saved_as("running", Some(FAKE_GROUP))?;
    let calls = Mutex::new(Vec::new());
    let done = with_reaper(&bench.project, |target| {
        calls
            .lock()
            .expect("test observer")
            .push((target.run_id.clone(), target.pgid));
        ReapOutcome::ProvenDead
    });
    assert_eq!(
        *calls.lock().expect("test observer"),
        vec![(RUN.to_owned(), FAKE_GROUP)],
        "finished Serve was omitted even though its service record still names an owned group"
    );
    assert_eq!(done.reaped, 1);
    assert_eq!(bench.record()?["state"], "dead");
    assert_eq!(bench.run()?["status"], "succeeded");
    assert_eq!(bench.run()?["steps"][0]["status"], "succeeded");
    let again = Mutex::new(Vec::new());
    let _ = with_reaper(&bench.project, |target| {
        again.lock().expect("test observer").push(target.pgid);
        ReapOutcome::ProvenDead
    });
    assert!(
        again.lock().expect("test observer").is_empty(),
        "Dead service was reaped twice"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_service_that_survives_stays_unproven_and_history_says_so() -> Result<(), Box<dyn Error>>
{
    uncertain_service(ReapOutcome::StillAlive, "survived").await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_foreign_group_is_not_called_dead_and_history_explains_the_refusal()
-> Result<(), Box<dyn Error>> {
    uncertain_service(ReapOutcome::Foreign, "different run").await
}

async fn uncertain_service(outcome: ReapOutcome, sentence: &str) -> Result<(), Box<dyn Error>> {
    let bench = Bench::stopped().await?;
    bench.saved_as("running", Some(FAKE_GROUP))?;
    let calls = Mutex::new(Vec::new());
    let _ = with_reaper(&bench.project, |target| {
        calls.lock().expect("test observer").push(target.pgid);
        outcome
    });
    assert_eq!(*calls.lock().expect("test observer"), vec![FAKE_GROUP]);
    assert_eq!(
        bench.record()?["state"],
        "unproven",
        "no death proof may release copy ownership"
    );
    let history = read_run_inner(&bench.project, DIR)?;
    let step = history
        .steps
        .iter()
        .find(|one| one.id == STEP)
        .ok_or("missing history step")?;
    assert!(
        step.error.contains(sentence),
        "the real history seam hid the service warning: {}",
        step.error
    );
    assert!(step.error.contains(&FAKE_GROUP.to_string()));
    assert_eq!(bench.run()?["steps"][0]["status"], "succeeded");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recovery_really_ends_a_live_service_even_though_its_serve_step_succeeded()
-> Result<(), Box<dyn Error>> {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();
    let (bench, processes, pgid) = Bench::live()?;
    let was_alive = !supervisor::group_is_empty(pgid);
    let before = bench.record();
    let behind = supervisor::run_behind_group(pgid);
    let done = reconcile_runs(&bench.project);
    let dead_after_recovery = supervisor::group_is_empty(pgid);
    let saved = bench.record();
    let output = processes.said(pgid);
    let run_after = bench.run();
    let proofs = processes.close().await;
    assert_cleanup(&proofs, pgid);
    assert!(was_alive, "there was no actual orphan to recover");
    assert!(
        dead_after_recovery,
        "production recovery left the finished Serve's real service alive"
    );
    assert_eq!(
        done.reaped, 1,
        "recovery: {done:?}; run tag: {behind:?}; service before: {before:?}; service after: {saved:?}; output: {output:?}; run after: {run_after:?}"
    );
    assert_eq!(saved?["state"], "dead");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_missing_boot_proof_keeps_the_service_worktree() -> Result<(), Box<dyn Error>> {
    let bench = Bench::stopped().await?;
    bench.saved_as("running", Some(FAKE_GROUP))?;
    let mut run = bench.run()?;
    run["boot_id"] = Value::Null;
    fs::write(bench.dir.join("run.json"), serde_json::to_vec(&run)?)?;
    let done = reconcile_runs(&bench.project);
    assert_eq!(done.reaped, 0);
    assert!(
        bench.cwd.join("value.txt").is_file(),
        "recovery had no authority to reap the service but still deleted its working copy"
    );
    assert_ne!(bench.record()?["state"], "dead");
    assert_blocked_copy_is_explained(&bench)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_service_state_keeps_the_worktree_without_guessing() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::stopped().await?;
    bench.saved_as("a-future-state", None)?;
    let _ = reconcile_runs(&bench.project);
    assert!(
        bench.cwd.join("value.txt").is_file(),
        "unknown ownership was treated as free to remove"
    );
    assert_ne!(bench.record()?["state"], "dead");
    assert_blocked_copy_is_explained(&bench)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unreadable_service_record_blocks_cleanup_instead_of_looking_like_no_services()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::stopped().await?;
    fs::write(bench.service_file(), b"{ incomplete")?;
    let _ = reconcile_runs(&bench.project);
    assert!(
        bench.cwd.join("value.txt").is_file(),
        "an unreadable service record lost its only working folder"
    );
    assert_blocked_copy_is_explained(&bench)?;
    Ok(())
}

fn assert_blocked_copy_is_explained(bench: &Bench) -> Result<(), Box<dyn Error>> {
    let history = read_run_inner(&bench.project, DIR)?;
    let step = history
        .steps
        .iter()
        .find(|one| one.id == STEP)
        .ok_or("missing history step")?;
    assert!(
        step.error.contains(
            "Loadout could not prove that all services using this working folder have stopped"
        ),
        "the actual history omitted the reason this folder remains: {}",
        step.error
    );
    assert!(step.error.contains("the folder was kept"));
    assert!(
        step.error.contains(&bench.cwd.display().to_string()),
        "the person cannot locate the folder recovery kept: {}",
        step.error
    );
    Ok(())
}

struct Bench {
    _root: TempDir,
    project: PathBuf,
    dir: PathBuf,
    cwd: PathBuf,
}

impl Bench {
    fn live() -> Result<(Self, Processes, i32), Box<dyn Error>> {
        let root = TempDir::new()?;
        let project = fs::canonicalize(root.path())?;
        fs::write(project.join("value.txt"), "the preview's own input\n")?;
        fs::write(project.join(".gitignore"), ".loadout/\n")?;
        git(&project, &["init", "--quiet"])?;
        git(&project, &["add", "-A"])?;
        git(
            &project,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "--quiet",
                "-m",
                "input",
            ],
        )?;
        let dir = project.join(".loadout/runs").join(DIR);
        let cwd = dir.join("work/s_preview");
        fs::create_dir_all(dir.join("work"))?;
        let branch = isolate::branch_for(RUN, "s_preview");
        isolate::make(&project, &cwd, &branch)?;
        fs::create_dir_all(dir.join(".isolation"))?;
        fs::write(
            dir.join(".isolation/s_preview"),
            serde_json::to_vec(&json!({
                "state": "complete", "branch": branch, "head": git(&project, &["rev-parse", "HEAD"])?.trim()
            }))?,
        )?;
        fs::write(
            dir.join("run.json"),
            serde_json::to_vec(&json!({
                "id": RUN, "title": "Preview recovery", "status": "succeeded",
                "boot_id": supervisor::machine_booted_at().unwrap_or_default(),
                "created_at": 1, "started_at": 1, "ended_at": 2,
                "steps": [{ "id": STEP, "node_key": "s_preview", "name": "Preview", "kind": "serve",
                    "status": "succeeded", "pgid": null, "pgids": [], "death_proof": true,
                    "started_at": 1, "ended_at": 2 }]
            }))?,
        )?;
        let processes = Processes::new();
        let started = processes.start_owned(
            &StartSpec {
                command: "sleep 600".to_owned(),
                cwd: cwd.clone(),
            },
            StepTag::new(RUN, STEP),
            ServiceOwner {
                reference: ServiceRef {
                    workspace: project.clone(),
                    run_id: RUN.to_owned(),
                    node_key: "s_preview".to_owned(),
                    service_id: SERVICE.to_owned(),
                    generation: 1,
                },
                run_dir: dir.clone(),
                cwd: cwd.clone(),
                lifetime: ServiceLifetime::Window,
            },
        )?;
        // Po spawnie nie ma fallible kroku przed oddaniem właściciela do cleanupu sceny.
        Ok((
            Self {
                _root: root,
                project,
                dir,
                cwd,
            },
            processes,
            started.pgid,
        ))
    }

    async fn stopped() -> Result<Self, Box<dyn Error>> {
        let (bench, processes, pgid) = Self::live()?;
        let proofs = processes.close().await;
        assert_cleanup(&proofs, pgid);
        Ok(bench)
    }

    fn service_file(&self) -> PathBuf {
        self.dir.join("services").join(format!("{SERVICE}.json"))
    }
    fn record(&self) -> Result<Value, Box<dyn Error>> {
        Ok(serde_json::from_slice(&fs::read(self.service_file())?)?)
    }
    fn run(&self) -> Result<Value, Box<dyn Error>> {
        Ok(serde_json::from_slice(&fs::read(
            self.dir.join("run.json"),
        )?)?)
    }
    fn saved_as(&self, state: &str, pgid: Option<i32>) -> Result<(), Box<dyn Error>> {
        let mut record = self.record()?;
        record["state"] = json!(state);
        record["pgid"] = json!(pgid);
        fs::write(self.service_file(), serde_json::to_vec(&record)?)?;
        Ok(())
    }
}

fn assert_cleanup(proofs: &[GroupProof], pgid: i32) {
    assert!(
        proofs
            .iter()
            .all(|one| matches!(one, GroupProof::Dead { .. }))
    );
    assert!(
        supervisor::group_is_empty(pgid),
        "fixture left its owned group {pgid} alive"
    );
}

fn git(cwd: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = Command::new("git").args(args).current_dir(cwd).output()?;
    if !output.status.success() {
        return Err(format!("git {args:?}: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(String::from_utf8(output.stdout)?)
}
