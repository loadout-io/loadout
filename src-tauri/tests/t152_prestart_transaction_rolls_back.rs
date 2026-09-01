//! T-152 AC-1: przygotowanie biegu ma jednego provisional właściciela aż do pierwszego lifecycle.

#![allow(clippy::too_many_lines)]

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::{
    PrestartFaultInjector, PrestartFaultPoint, RolledBackResource,
    run_triggered_workflow_with_prestart_faults, run_workflow_with_prestart_faults,
};
use loadout_lib::commands::triggers::{self, DeliveryState, TriggerDelivery, TriggerPoll};
use loadout_lib::commands::workspaces;
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
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::mpsc;

const PATIENCE: Duration = Duration::from_secs(30);
const CREATED: i64 = 1_788_000_000_000;
const KEY: &str = "lin_api_1234567890123456789012345678901234567890";

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

const SKILL: &str = r"---
name: alpha
description: Leaves the T-152 ownership proof in the selected work tree.
---

Write one small proof file.
";

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
        fs::create_dir_all(home.join("triggers"))?;
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
        let workspace = project.to_str().ok_or("the test workspace is not UTF-8")?;
        workspaces::save_workspace_inner(&home, "T-152 trigger", workspace)?;
        fs::write(
            home.join("triggers/t152.json"),
            serde_json::to_vec_pretty(&json!({
                "schema": 1,
                "source": "linear",
                "enabled": true,
                "workflow": "t152-prestart.json",
                "workspace": workspace,
                "condition": "assigned-to-me",
                "api_key": KEY
            }))?,
        )?;
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
            run_directories(&self.project).is_empty(),
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
        let hook: Arc<dyn PrestartFaultInjector> = faults;
        self.attempt_with(hook).await
    }

    async fn attempt_with(
        &self,
        hook: Arc<dyn PrestartFaultInjector>,
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
        let result = tokio::time::timeout(
            PATIENCE,
            run_workflow_with_prestart_faults(&deps, &self.request(), lines, hook),
        )
        .await
        .map_err(|_| "pre-start preparation exceeded the test's bounded patience")?;
        Ok(result)
    }

    fn one_delivery(&self) -> Result<TriggerDelivery, Box<dyn Error>> {
        assert_eq!(
            poll(self, &[issue("old", "LOAD-0", 8)])?,
            TriggerPoll::Armed
        );
        let pending = poll(self, &[issue("t152", "LOAD-152", 9)])?;
        let TriggerPoll::Pending { delivery } = pending else {
            return Err(format!("the T-152 issue did not become pending: {pending:?}").into());
        };
        Ok(*delivery)
    }

    async fn triggered_attempt_with(
        &self,
        delivery: &TriggerDelivery,
        hook: Arc<dyn PrestartFaultInjector>,
    ) -> Result<
        Result<loadout_lib::commands::run::TriggerRunReport, loadout_lib::commands::RunError>,
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
        let result = tokio::time::timeout(
            PATIENCE,
            run_triggered_workflow_with_prestart_faults(
                &deps,
                &self.request(),
                &delivery.claim,
                lines,
                hook,
            ),
        )
        .await
        .map_err(|_| "triggered pre-start preparation exceeded the test's bounded patience")?;
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

#[derive(Debug)]
struct RecoveredGitMarker {
    project: PathBuf,
    run_dir: Mutex<Option<PathBuf>>,
    rolled_back: Mutex<Vec<RolledBackResource>>,
}

impl RecoveredGitMarker {
    fn new(project: PathBuf) -> Self {
        Self {
            project,
            run_dir: Mutex::new(None),
            rolled_back: Mutex::new(Vec::new()),
        }
    }

    fn run_dir(&self) -> Option<PathBuf> {
        lock(&self.run_dir).clone()
    }

    fn rollback(&self) -> Vec<RolledBackResource> {
        lock(&self.rolled_back).clone()
    }
}

impl PrestartFaultInjector for RecoveredGitMarker {
    fn before_run_directory(&self, run_dir: &Path) -> std::io::Result<()> {
        let cwd = run_dir.join("work/work");
        let marker_root = run_dir.join(".isolation");
        fs::create_dir_all(&cwd)?;
        fs::create_dir_all(&marker_root)?;
        fs::write(cwd.join("human-wip.txt"), b"do not delete recovered WIP\n")?;
        fs::write(
            marker_root.join("work"),
            br#"{"state":"complete","branch":"foreign/t152","head":"deadbeef"}"#,
        )?;
        fs::rename(
            self.project.join(".git"),
            self.project.join(".git-before-retry"),
        )?;
        *lock(&self.run_dir) = Some(run_dir.to_path_buf());
        Ok(())
    }

    fn check(&self, _point: PrestartFaultPoint) -> std::io::Result<()> {
        Ok(())
    }

    fn rolled_back(&self, resource: &RolledBackResource) {
        lock(&self.rolled_back).push(resource.clone());
    }
}

#[derive(Debug)]
struct ExistingManualRun {
    run_dir: Mutex<Option<PathBuf>>,
    rolled_back: Mutex<Vec<RolledBackResource>>,
}

impl ExistingManualRun {
    fn new() -> Self {
        Self {
            run_dir: Mutex::new(None),
            rolled_back: Mutex::new(Vec::new()),
        }
    }

    fn run_dir(&self) -> Option<PathBuf> {
        lock(&self.run_dir).clone()
    }

    fn rollback(&self) -> Vec<RolledBackResource> {
        lock(&self.rolled_back).clone()
    }
}

impl PrestartFaultInjector for ExistingManualRun {
    fn before_run_directory(&self, run_dir: &Path) -> io::Result<()> {
        fs::create_dir_all(run_dir)?;
        fs::write(
            run_dir.join("manual-sentinel.txt"),
            b"this directory was not created by Loadout\n",
        )?;
        *lock(&self.run_dir) = Some(run_dir.to_path_buf());
        Ok(())
    }

    fn check(&self, _point: PrestartFaultPoint) -> io::Result<()> {
        Ok(())
    }

    fn rolled_back(&self, resource: &RolledBackResource) {
        lock(&self.rolled_back).push(resource.clone());
    }
}

#[derive(Debug)]
struct ExistingBoundRun {
    refuse: PrestartFaultPoint,
    run_dir: Mutex<Option<PathBuf>>,
    rolled_back: Mutex<Vec<RolledBackResource>>,
}

impl ExistingBoundRun {
    fn new(refuse: PrestartFaultPoint) -> Self {
        Self {
            refuse,
            run_dir: Mutex::new(None),
            rolled_back: Mutex::new(Vec::new()),
        }
    }

    fn run_dir(&self) -> Option<PathBuf> {
        lock(&self.run_dir).clone()
    }

    fn rollback(&self) -> Vec<RolledBackResource> {
        lock(&self.rolled_back).clone()
    }
}

impl PrestartFaultInjector for ExistingBoundRun {
    fn before_run_directory(&self, run_dir: &Path) -> io::Result<()> {
        fs::create_dir_all(run_dir)?;
        fs::write(
            run_dir.join("same-attempt.txt"),
            b"this directory belongs to the exact Bound claim\n",
        )?;
        *lock(&self.run_dir) = Some(run_dir.to_path_buf());
        Ok(())
    }

    fn check(&self, point: PrestartFaultPoint) -> io::Result<()> {
        if point == self.refuse {
            return Err(io::Error::other(format!(
                "T-152 rejected the Bound retry at {point:?}"
            )));
        }
        Ok(())
    }

    fn rolled_back(&self, resource: &RolledBackResource) {
        lock(&self.rolled_back).push(resource.clone());
    }
}

#[derive(Debug)]
struct ReplacePublishedRunFile {
    run_dir: Mutex<Option<PathBuf>>,
    replacement: Mutex<Option<Vec<u8>>>,
    rolled_back: Mutex<Vec<RolledBackResource>>,
}

impl ReplacePublishedRunFile {
    fn new() -> Self {
        Self {
            run_dir: Mutex::new(None),
            replacement: Mutex::new(None),
            rolled_back: Mutex::new(Vec::new()),
        }
    }

    fn run_dir(&self) -> Option<PathBuf> {
        lock(&self.run_dir).clone()
    }

    fn replacement(&self) -> Option<Vec<u8>> {
        lock(&self.replacement).clone()
    }

    fn rollback(&self) -> Vec<RolledBackResource> {
        lock(&self.rolled_back).clone()
    }
}

impl PrestartFaultInjector for ReplacePublishedRunFile {
    fn before_run_directory(&self, run_dir: &Path) -> io::Result<()> {
        *lock(&self.run_dir) = Some(run_dir.to_path_buf());
        Ok(())
    }

    fn check(&self, point: PrestartFaultPoint) -> io::Result<()> {
        if point != PrestartFaultPoint::AfterFirstRunFile {
            return Ok(());
        }
        let run_dir = self.run_dir().ok_or_else(|| {
            io::Error::other("the replacement hook did not see the run directory")
        })?;
        let run_file = run_dir.join("run.json");
        // Te same bajty i to samo ID celowo nie odróżniają kopii treścią. Jedynym dowodem, że
        // próba nie utworzyła replacementu, jest opaque identity pierwszej publikacji.
        let replacement = fs::read(&run_file)?;
        let incoming = run_dir.join("foreign-run.json");
        fs::write(&incoming, &replacement)?;
        fs::rename(incoming, &run_file)?;
        *lock(&self.replacement) = Some(replacement);
        Err(io::Error::other(
            "T-152 replaced the published receipt while preserving its run id",
        ))
    }

    fn rolled_back(&self, resource: &RolledBackResource) {
        lock(&self.rolled_back).push(resource.clone());
    }
}

#[derive(Clone, Copy, Debug)]
enum GitMutation {
    RemovePath,
    ReplaceIdentity,
}

#[derive(Clone, Debug)]
struct MutatedTree {
    cwd: PathBuf,
    branch: String,
    head: String,
}

#[derive(Debug)]
struct MutateRegisteredGitTree {
    project: PathBuf,
    mutation: GitMutation,
    run_dir: Mutex<Option<PathBuf>>,
    changed: Mutex<Option<MutatedTree>>,
    rolled_back: Mutex<Vec<RolledBackResource>>,
}

impl MutateRegisteredGitTree {
    fn new(project: PathBuf, mutation: GitMutation) -> Self {
        Self {
            project,
            mutation,
            run_dir: Mutex::new(None),
            changed: Mutex::new(None),
            rolled_back: Mutex::new(Vec::new()),
        }
    }

    fn changed(&self) -> Option<MutatedTree> {
        lock(&self.changed).clone()
    }

    fn run_dir(&self) -> Option<PathBuf> {
        lock(&self.run_dir).clone()
    }

    fn rollback(&self) -> Vec<RolledBackResource> {
        lock(&self.rolled_back).clone()
    }
}

impl PrestartFaultInjector for MutateRegisteredGitTree {
    fn before_run_directory(&self, run_dir: &Path) -> io::Result<()> {
        *lock(&self.run_dir) = Some(run_dir.to_path_buf());
        Ok(())
    }

    fn check(&self, point: PrestartFaultPoint) -> io::Result<()> {
        if point != PrestartFaultPoint::AfterWorktreeAdd {
            return Ok(());
        }
        let run_dir = self
            .run_dir()
            .ok_or_else(|| io::Error::other("the mutation hook did not see the run directory"))?;
        let cwd = run_dir.join("work/work");
        let branch = git_text_io(&cwd, &["symbolic-ref", "--short", "HEAD"])?
            .trim()
            .to_owned();
        let original_head = git_text_io(&self.project, &["rev-parse", "--verify", &branch])?
            .trim()
            .to_owned();
        let head = match self.mutation {
            GitMutation::RemovePath => {
                fs::remove_dir_all(&cwd)?;
                original_head
            }
            GitMutation::ReplaceIdentity => {
                let destination = cwd.display().to_string();
                git_ok_io(
                    &self.project,
                    &["worktree", "remove", "--force", "--", &destination],
                )?;
                let tree = git_text_io(&self.project, &["rev-parse", "HEAD^{tree}"])?;
                let replacement = git_text_io(
                    &self.project,
                    &[
                        "commit-tree",
                        tree.trim(),
                        "-p",
                        &original_head,
                        "-m",
                        "foreign replacement",
                    ],
                )?
                .trim()
                .to_owned();
                let reference = format!("refs/heads/{branch}");
                git_ok_io(
                    &self.project,
                    &["update-ref", &reference, &replacement, &original_head],
                )?;
                git_ok_io(
                    &self.project,
                    &["worktree", "add", "--quiet", &destination, &branch],
                )?;
                fs::write(cwd.join("foreign-wip.txt"), b"replacement must survive\n")?;
                replacement
            }
        };
        *lock(&self.changed) = Some(MutatedTree { cwd, branch, head });
        Err(io::Error::other(format!(
            "T-152 changed the registered Git tree with {:?}",
            self.mutation
        )))
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
                rollback.len() <= 5,
                "three exact Git trees need at most three guarded retirements, one run-file removal and one run-directory removal; got {rollback:?}"
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_manual_run_directory_is_refused_without_any_mutation() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let manual = Arc::new(ExistingManualRun::new());
    let hook: Arc<dyn PrestartFaultInjector> = manual.clone();
    let result = bench.attempt_with(hook).await?;

    assert!(
        result.is_err(),
        "an unclaimed existing run directory was accepted"
    );
    let run_dir = manual
        .run_dir()
        .ok_or("the manual-directory fixture never observed the planned path")?;
    assert_eq!(
        fs::read(run_dir.join("manual-sentinel.txt"))?,
        b"this directory was not created by Loadout\n"
    );
    assert_eq!(
        fs::read_dir(&run_dir)?.count(),
        1,
        "preparation wrote into an unclaimed existing run directory"
    );
    assert!(
        manual.rollback().is_empty(),
        "the guard claimed resources inside a directory it did not create: {:?}",
        manual.rollback()
    );
    assert_eq!(
        git_text(&bench.project, &["worktree", "list", "--porcelain"])?,
        bench.baseline_worktrees
    );
    assert_eq!(branches(&bench.project)?, bench.baseline_branches);
    assert_eq!(bench.starts.load(Ordering::Acquire), 0);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_git_marker_is_not_claimed_as_a_new_file_copy() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let delivery = bench.one_delivery()?;
    let marker = Arc::new(RecoveredGitMarker::new(bench.project.clone()));
    let hook: Arc<dyn PrestartFaultInjector> = marker.clone();
    let result = bench.triggered_attempt_with(&delivery, hook).await?;

    assert!(
        result.is_err(),
        "a retry accepted a git marker after the project stopped being a git repository"
    );
    let run_dir = marker
        .run_dir()
        .ok_or("the recovery fixture never observed the planned run directory")?;
    let recovered = run_dir.join("work/work");
    assert_eq!(
        fs::read(recovered.join("human-wip.txt"))?,
        b"do not delete recovered WIP\n",
        "the provisional guard deleted WIP from a work tree owned by an earlier attempt"
    );
    assert!(
        run_dir.join(".isolation/work").is_file(),
        "the refusal removed the marker that proves the recovered tree's origin"
    );
    assert!(
        !marker.rollback().iter().any(
            |resource| matches!(resource, RolledBackResource::GitTree { path, .. } if path == &recovered)
        ),
        "the current attempt claimed a recovered git work tree as its own file copy"
    );
    assert_eq!(
        bench.starts.load(Ordering::Acquire),
        0,
        "a driver started after the isolation mismatch"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_same_bound_claim_can_retry_before_and_after_its_first_run_file()
-> Result<(), Box<dyn Error>> {
    for point in [
        PrestartFaultPoint::BeforeFirstRunFile,
        PrestartFaultPoint::AfterFirstRunFile,
        PrestartFaultPoint::OwnershipTransferred,
    ] {
        let bench = Bench::new()?;
        let delivery = bench.one_delivery()?;
        for repetition in 1..=2 {
            let faults = Arc::new(ExistingBoundRun::new(point));
            let hook: Arc<dyn PrestartFaultInjector> = faults.clone();
            let result = bench.triggered_attempt_with(&delivery, hook).await?;
            assert!(
                result.is_err(),
                "the Bound claim reported success at {point:?}, repetition {repetition}"
            );
            let run_dir = faults
                .run_dir()
                .ok_or("the Bound fixture did not see its stable run directory")?;
            assert!(
                !run_dir.join("run.json").exists(),
                "a zero-start receipt survived {point:?}, repetition {repetition}"
            );
            assert!(
                !run_dir.exists(),
                "the exact Bound attempt kept its Complete markers or partial directory at {point:?}, repetition {repetition}"
            );
            assert_eq!(
                delivery_state(&bench.home, &delivery)?,
                DeliveryState::Pending,
                "the failed Bound claim was not released for a safe retry"
            );
            let rollback = faults.rollback();
            assert!(
                rollback.iter().any(
                    |resource| matches!(resource, RolledBackResource::RunDirectory(path) if path == &run_dir)
                ),
                "the stable run directory never entered the production guard: {rollback:?}"
            );
            if point != PrestartFaultPoint::BeforeFirstRunFile {
                assert!(
                    rollback.iter().any(
                        |resource| matches!(resource, RolledBackResource::RunFile(path) if path == &run_dir.join("run.json"))
                    ),
                    "the first run.json was not a separately guarded resource: {rollback:?}"
                );
            }
            assert_eq!(bench.starts.load(Ordering::Acquire), 0);
            assert_eq!(
                git_text(&bench.project, &["worktree", "list", "--porcelain"])?,
                bench.baseline_worktrees,
                "a Bound retry left a Complete+missing tree state"
            );
            assert_eq!(branches(&bench.project)?, bench.baseline_branches);
        }
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_same_id_replacement_receipt_is_preserved_with_its_parent() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let faults = Arc::new(ReplacePublishedRunFile::new());
    let hook: Arc<dyn PrestartFaultInjector> = faults.clone();
    let result = bench.attempt_with(hook).await?;
    assert!(result.is_err(), "the replacement fault reported success");

    let run_dir = faults
        .run_dir()
        .ok_or("the replacement fixture did not see its run directory")?;
    let replacement = faults
        .replacement()
        .ok_or("the replacement fixture did not publish its foreign receipt")?;
    assert_eq!(
        fs::read(run_dir.join("run.json"))?,
        replacement,
        "rollback removed or rewrote a foreign receipt that retained the preallocated run id"
    );
    assert!(
        run_dir.is_dir(),
        "parent cleanup bypassed the exact receipt provenance mismatch"
    );
    let rollback = faults.rollback();
    assert!(
        rollback.iter().any(
            |resource| matches!(resource, RolledBackResource::RunFile(path) if path == &run_dir.join("run.json"))
        ),
        "the replacement receipt never passed through guarded cleanup: {rollback:?}"
    );
    assert!(
        rollback.iter().any(
            |resource| matches!(resource, RolledBackResource::RunDirectory(path) if path == &run_dir)
        ),
        "the parent directory never passed through guarded cleanup: {rollback:?}"
    );
    assert_eq!(
        git_text(&bench.project, &["worktree", "list", "--porcelain"])?,
        bench.baseline_worktrees,
        "preserving the foreign receipt left a provisional Git worktree behind"
    );
    assert_eq!(branches(&bench.project)?, bench.baseline_branches);
    assert_eq!(bench.starts.load(Ordering::Acquire), 0);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_missing_registered_path_still_retires_its_exact_git_admin_and_branch()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let faults = Arc::new(MutateRegisteredGitTree::new(
        bench.project.clone(),
        GitMutation::RemovePath,
    ));
    let hook: Arc<dyn PrestartFaultInjector> = faults.clone();
    let result = bench.attempt_with(hook).await?;
    assert!(result.is_err(), "the missing-path fault reported success");
    let changed = faults
        .changed()
        .ok_or("the missing-path hook never changed the registered work tree")?;
    assert!(
        !changed.cwd.exists(),
        "the provisional checkout path came back after guarded cleanup"
    );
    assert!(
        !branches(&bench.project)?.contains(&changed.branch),
        "the exact provisional branch survived after its missing cwd was retired"
    );
    assert!(
        faults.rollback().iter().any(
            |resource| matches!(resource, RolledBackResource::GitTree { path, branch } if path == &changed.cwd && branch == &changed.branch)
        ),
        "the exact Git tuple was not observed as one bounded resource"
    );
    bench.assert_baseline()?;
    assert_eq!(bench.starts.load(Ordering::Acquire), 0);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_replaced_git_identity_and_its_wip_are_never_removed_by_parent_cleanup()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let faults = Arc::new(MutateRegisteredGitTree::new(
        bench.project.clone(),
        GitMutation::ReplaceIdentity,
    ));
    let hook: Arc<dyn PrestartFaultInjector> = faults.clone();
    let result = bench.attempt_with(hook).await?;
    assert!(result.is_err(), "the replacement fault reported success");
    let changed = faults
        .changed()
        .ok_or("the replacement hook never changed the registered work tree")?;
    assert_eq!(
        fs::read(changed.cwd.join("foreign-wip.txt"))?,
        b"replacement must survive\n",
        "the provisional guard deleted WIP from the replacement work tree"
    );
    assert_eq!(
        git_text(&bench.project, &["rev-parse", "--verify", &changed.branch])?.trim(),
        changed.head,
        "the guard moved or deleted the replacement branch"
    );
    let run_dir = faults
        .run_dir()
        .ok_or("the replacement fixture did not see its run directory")?;
    assert!(
        run_dir.is_dir(),
        "parent cleanup bypassed the Git identity mismatch with remove_dir_all"
    );
    assert_eq!(
        fs::read(bench.foreign.join("foreign-wip.txt"))?,
        bench.foreign_wip
    );
    assert_eq!(bench.starts.load(Ordering::Acquire), 0);
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

fn poll(bench: &Bench, issues: &[Value]) -> Result<TriggerPoll, triggers::TriggerError> {
    let bytes = serde_json::to_vec(&json!({"data":{"issues":{"nodes":issues}}}))
        .map_err(triggers::TriggerError::InvalidAnswer)?;
    triggers::poll_with(&bench.home, "t152", CREATED, |_| Ok(bytes))
}

fn issue(id: &str, identifier: &str, hour: u8) -> Value {
    json!({
        "id": id,
        "identifier": identifier,
        "title": format!("Issue {identifier}"),
        "url": format!("https://linear.app/loadout/issue/{identifier}"),
        "description": "body",
        "updatedAt": format!("2026-08-28T{hour:02}:00:00.000Z")
    })
}

fn delivery_state(
    home: &Path,
    delivery: &TriggerDelivery,
) -> Result<DeliveryState, Box<dyn Error>> {
    let ledger: Value =
        serde_json::from_slice(&fs::read(home.join("triggers/.t152.ledger.json"))?)?;
    let records = ledger
        .get("deliveries")
        .and_then(Value::as_array)
        .ok_or("the trigger ledger has no deliveries")?;
    for record in records {
        let found: TriggerDelivery = serde_json::from_value(
            record
                .get("delivery")
                .cloned()
                .ok_or("a ledger record has no delivery")?,
        )?;
        if found == *delivery {
            return Ok(serde_json::from_value(
                record
                    .get("state")
                    .cloned()
                    .ok_or("a ledger record has no state")?,
            )?);
        }
    }
    Err("the delivery is missing from its ledger".into())
}

fn git_ok_io(at: &Path, args: &[&str]) -> io::Result<()> {
    git_ok(at, args).map_err(|error| io::Error::other(error.to_string()))
}

fn git_text_io(at: &Path, args: &[&str]) -> io::Result<String> {
    git_text(at, args).map_err(|error| io::Error::other(error.to_string()))
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

fn run_directories(project: &Path) -> Vec<String> {
    let root = project.join(".loadout/runs");
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut names = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
