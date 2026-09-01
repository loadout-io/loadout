//! T-152 AC-1: przygotowanie biegu ma jednego provisional właściciela aż do pierwszego lifecycle.

#![allow(clippy::too_many_lines)]

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::{
    PrestartFaultInjector, PrestartFaultPoint, RolledBackResource,
    run_workflow_with_prestart_faults,
};
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::line_channel;
use loadout_lib::library::agents::Vendor;
use loadout_lib::memory::handoff::{Kind, MetaDraft, write_handoff};
use loadout_lib::store::Store;
use tempfile::TempDir;
use tokio::sync::mpsc;

const PATIENCE: Duration = Duration::from_secs(30);

const AGENT: &str = r#"---
schema: 1
id: 019b0152-0000-7000-8000-000000000001
name: Transaction worker
summary: Exercises pre-start ownership
color: moss
runsWith: claude-code
model: sonnet
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 20
writeResultsTo: ""
tools: everything
skills: [alpha]
connections: []
---
Write the requested proof.
"#;

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_t152_prestart",
  "name": "Pre-start transaction",
  "steps": [{
    "kind": "agent",
    "id": "work",
    "name": "Prepare safely",
    "agent": "019b0152-0000-7000-8000-000000000001",
    "overrides": {},
    "copies": 3,
    "instructions": "T152 write the guard proof.",
    "borrow": { "learnings": "t152-prestart" },
    "folder": { "use": "fresh-copy" },
    "at": { "x": 0, "y": 0 }
  }],
  "links": []
}"#;

const SKILL: &str = r#"---
name: alpha
description: Leaves the T-152 ownership proof in the selected work tree.
---

Write one small proof file.
"#;

#[derive(Debug)]
struct Bench {
    _root: TempDir,
    home: PathBuf,
    project: PathBuf,
    foreign: PathBuf,
    previous: PathBuf,
    workflow: PathBuf,
    starts: Arc<AtomicUsize>,
    baseline_worktrees: String,
    baseline_branches: Vec<String>,
    tracked_wip: Vec<u8>,
    untracked_wip: Vec<u8>,
    foreign_wip: Vec<u8>,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let home = root.path().join("home");
        let project = root.path().join("project");
        let foreign = root.path().join("foreign-worktree");
        let previous = root.path().join("previous-run");
        fs::create_dir_all(home.join("agents"))?;
        fs::create_dir_all(home.join("workflows"))?;
        fs::create_dir_all(home.join("skills/alpha"))?;
        fs::create_dir_all(&project)?;
        fs::create_dir_all(&previous)?;
        fs::create_dir_all(project.join(".loadout"))?;
        fs::create_dir_all(project.join(".claude/learnings"))?;
        fs::write(
            project.join(".claude/learnings/t152-prestart.md"),
            "# Learnings\n\n## Recurring patterns\n\n- Keep one owner for pre-start resources.\n",
        )?;

        git_ok(&project, &["init", "--quiet"])?;
        git_ok(&project, &["config", "user.email", "t152@example.test"])?;
        git_ok(&project, &["config", "user.name", "T152"])?;
        fs::write(project.join("tracked.txt"), b"committed\n")?;
        git_ok(&project, &["add", "tracked.txt"])?;
        git_ok(&project, &["commit", "--quiet", "-m", "baseline"])?;

        git_ok(
            &project,
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                "foreign/t152",
                foreign.to_string_lossy().as_ref(),
                "HEAD",
            ],
        )?;
        let foreign_wip = b"foreign work must survive\n".to_vec();
        fs::write(foreign.join("foreign-wip.txt"), &foreign_wip)?;

        let tracked_wip = b"committed\nhuman tracked WIP\n".to_vec();
        let untracked_wip = b"human untracked WIP\n".to_vec();
        fs::write(project.join("tracked.txt"), &tracked_wip)?;
        fs::write(project.join("untracked.txt"), &untracked_wip)?;

        fs::write(home.join("agents/transaction-worker.md"), AGENT)?;
        fs::write(home.join("skills/alpha/SKILL.md"), SKILL)?;
        let workflow = home.join("workflows/t152-prestart.json");
        fs::write(&workflow, WORKFLOW)?;
        write_handoff(
            &previous,
            MetaDraft {
                run: "t152-before".to_owned(),
                step: 1,
                from: "Earlier step".to_owned(),
                to: vec!["Prepare safely".to_owned()],
                kind: Kind::Brief,
                title: "Seeded input".to_owned(),
                reads: Vec::new(),
            },
            "## Answer\nKeep the human WIP.\n\n## Evidence\ntracked.txt\n\n## Open\nNone.\n",
        )?;

        let baseline_worktrees = git_text(&project, &["worktree", "list", "--porcelain"])?;
        let baseline_branches = branches(&project)?;
        Ok(Self {
            _root: root,
            home,
            project,
            foreign,
            previous,
            workflow,
            starts: Arc::new(AtomicUsize::new(0)),
            baseline_worktrees,
            baseline_branches,
            tracked_wip,
            untracked_wip,
            foreign_wip,
        })
    }

    fn database(&self) -> PathBuf {
        self.project.join(".loadout/loadout.db")
    }

    fn request(&self) -> RunRequest {
        RunRequest {
            workflow: self.workflow.clone(),
            how_many_at_once: 3,
            task: None,
            part: None,
            handoffs_from: Some(self.previous.clone()),
        }
    }

    fn assert_baseline(&self) -> Result<(), Box<dyn Error>> {
        assert_eq!(
            git_text(&self.project, &["worktree", "list", "--porcelain"])?,
            self.baseline_worktrees,
            "a failed preparation changed the set of work trees, including the foreign one"
        );
        assert_eq!(
            branches(&self.project)?,
            self.baseline_branches,
            "a failed preparation left a branch or removed the foreign branch"
        );
        assert_eq!(
            fs::read(self.project.join("tracked.txt"))?,
            self.tracked_wip
        );
        assert_eq!(
            fs::read(self.project.join("untracked.txt"))?,
            self.untracked_wip
        );
        assert_eq!(
            fs::read(self.foreign.join("foreign-wip.txt"))?,
            self.foreign_wip
        );
        assert!(
            run_directories(&self.project)?.is_empty(),
            "a refused preparation left a run directory behind"
        );
        Ok(())
    }

    async fn attempt(
        &self,
        faults: Arc<Faults>,
    ) -> Result<
        Result<loadout_lib::commands::RunReport, loadout_lib::commands::RunError>,
        Box<dyn Error>,
    > {
        let store = Store::open(&self.database())?;
        let deps = RunDeps {
            home: &self.home,
            project: &self.project,
            store: &store,
            drivers: drivers(Arc::clone(&self.starts), false),
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let (lines, _source) = line_channel(4_096);
        let hook: Arc<dyn PrestartFaultInjector> = faults;
        let result = tokio::time::timeout(
            PATIENCE,
            run_workflow_with_prestart_faults(&deps, &self.request(), lines, hook),
        )
        .await
        .map_err(|_| "pre-start preparation exceeded the test's bounded patience")?;
        Ok(result)
    }
}

#[derive(Debug)]
struct Faults {
    refuse: Option<PrestartFaultPoint>,
    seen: Mutex<Vec<PrestartFaultPoint>>,
    rolled_back: Mutex<Vec<RolledBackResource>>,
}

impl Faults {
    fn refusing(point: PrestartFaultPoint) -> Self {
        Self {
            refuse: Some(point),
            seen: Mutex::new(Vec::new()),
            rolled_back: Mutex::new(Vec::new()),
        }
    }

    fn observing() -> Self {
        Self {
            refuse: None,
            seen: Mutex::new(Vec::new()),
            rolled_back: Mutex::new(Vec::new()),
        }
    }

    fn seen(&self) -> Vec<PrestartFaultPoint> {
        lock(&self.seen).clone()
    }

    fn rollback(&self) -> Vec<RolledBackResource> {
        lock(&self.rolled_back).clone()
    }
}

impl PrestartFaultInjector for Faults {
    fn check(&self, point: PrestartFaultPoint) -> std::io::Result<()> {
        lock(&self.seen).push(point);
        if self.refuse == Some(point) {
            return Err(std::io::Error::other(format!(
                "T-152 injected refusal at {point:?}"
            )));
        }
        Ok(())
    }

    fn rolled_back(&self, resource: &RolledBackResource) {
        lock(&self.rolled_back).push(resource.clone());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_prestart_refusal_restores_the_exact_baseline() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let points = [
        PrestartFaultPoint::AfterWorktreeAdd,
        PrestartFaultPoint::AfterSecondIsolation,
        PrestartFaultPoint::AfterHandoffSeed,
        PrestartFaultPoint::AfterBorrow,
        PrestartFaultPoint::AfterSkills,
        PrestartFaultPoint::BeforeFirstRunFile,
        PrestartFaultPoint::AfterFirstRunFile,
    ];

    for point in points {
        for repetition in 1..=2 {
            let faults = Arc::new(Faults::refusing(point));
            let result = bench.attempt(Arc::clone(&faults)).await?;
            assert!(
                result.is_err(),
                "preparation reported success at {point:?}, repetition {repetition}"
            );
            assert!(
                faults.seen().contains(&point),
                "the production preparation never reached {point:?}"
            );
            let rollback = faults.rollback();
            let distinct: BTreeSet<String> = rollback
                .iter()
                .map(|resource| format!("{resource:?}"))
                .collect();
            assert_eq!(
                distinct.len(),
                rollback.len(),
                "rollback retried one resource in a loop at {point:?}: {rollback:?}"
            );
            assert!(
                rollback.len() <= 7,
                "three trial work trees need at most three removals, three branch drops and one run-directory removal; got {rollback:?}"
            );
            bench.assert_baseline()?;
        }
    }
    assert_eq!(
        bench.starts.load(Ordering::Acquire),
        0,
        "a driver started while preparation still belonged to the provisional guard"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ownership_transfer_disarms_the_provisional_guard() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let store = Store::open(&bench.database())?;
    let faults = Arc::new(Faults::observing());
    let deps = RunDeps {
        home: &bench.home,
        project: &bench.project,
        store: &store,
        drivers: drivers(Arc::clone(&bench.starts), true),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let (lines, _source) = line_channel(4_096);
    let hook: Arc<dyn PrestartFaultInjector> = faults.clone();
    let report = tokio::time::timeout(
        PATIENCE,
        run_workflow_with_prestart_faults(&deps, &bench.request(), lines, hook),
    )
    .await
    .map_err(|_| "the successful control run exceeded the test's bounded patience")??;

    assert!(
        faults
            .seen()
            .contains(&PrestartFaultPoint::OwnershipTransferred),
        "the production path never exposed the single ownership-transfer boundary"
    );
    assert!(
        faults.rollback().is_empty(),
        "dropping a disarmed provisional guard attempted rollback: {:?}",
        faults.rollback()
    );
    assert_eq!(bench.starts.load(Ordering::Acquire), 3);
    assert_eq!(
        git_text(&bench.project, &["worktree", "list", "--porcelain"])?,
        bench.baseline_worktrees,
        "normal lifecycle did not remove only the three trial work trees"
    );

    let prefix = format!("loadout/{}/", report.id);
    let trial_branches: Vec<String> = branches(&bench.project)?
        .into_iter()
        .filter(|branch| branch.starts_with(&prefix))
        .collect();
    assert_eq!(trial_branches.len(), 3);
    for branch in trial_branches {
        assert_eq!(
            git_text(
                &bench.project,
                &["show", &format!("{branch}:guard-proof.txt")]
            )?,
            "owned after transfer\n",
            "the provisional guard removed work written after ownership transfer"
        );
    }
    Ok(())
}

fn drivers(starts: Arc<AtomicUsize>, writes: bool) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(FakeDriver { starts, writes });
    Arc::new(move |_vendor: Vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct FakeDriver {
    starts: Arc<AtomicUsize>,
    writes: bool,
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
        self.starts.fetch_add(1, Ordering::AcqRel);
        if self.writes {
            fs::write(spec.cwd.join("guard-proof.txt"), b"owned after transfer\n")?;
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
            text: "## Answer\nDone.\n\n## Evidence\nguard-proof.txt\n\n## Open\nNone.\n".to_owned(),
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

fn git_ok(at: &Path, args: &[&str]) -> Result<(), Box<dyn Error>> {
    let output = Command::new("git").args(args).current_dir(at).output()?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    )
    .into())
}

fn git_text(at: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = Command::new("git").args(args).current_dir(at).output()?;
    if !output.status.success() {
        return Err(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim_end().to_owned() + "\n")
}

fn branches(project: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let text = git_text(
        project,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )?;
    Ok(text.lines().map(str::to_owned).collect())
}

fn run_directories(project: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let root = project.join(".loadout/runs");
    let Ok(entries) = fs::read_dir(root) else {
        return Ok(Vec::new());
    };
    let mut names = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<_>>();
    names.sort();
    Ok(names)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
