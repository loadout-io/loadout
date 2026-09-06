//! WF-02: pełne operacje plikowe przechodzą przez prawdziwy runner do prywatnej kopii.
//!
//! 2026-09-05: samo obejście istniejących plików gubi usunięcia, linki i zmianę trybu.
//! Dubler wykonuje pracę rodzica w RunSpec.cwd; dopiero uruchomiony konsument odczytuje
//! wynik składania. Bariera wymaga obu rodziców naraz, a kolejność ich zamknięcia jest
//! sterowana bez sleep. Nie implementujemy algorytmu fan-in w dublerze.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{self, GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use serde_json::json;
use tempfile::TempDir;
use tokio::sync::{Barrier, Semaphore, mpsc};

const LEFT: &str = "wf02_left";
const RIGHT: &str = "wf02_right";
const BELOW: &str = "wf02_below";
const LEFT_NAME: &str = "Remove retired files";
const RIGHT_NAME: &str = "Update current files";
const BELOW_NAME: &str = "Read the combined result";
const PATIENCE: Duration = Duration::from_secs(30);
const PATHS: &[&str] = &[
    "obsolete.txt",
    "shared.txt",
    "old-name.txt",
    "new-name.txt",
    "binary.dat",
    "run.sh",
    "alias",
    "feature",
    "feature/base.txt",
    "feature/new.txt",
    "added",
    "added/left.txt",
    "added/right.txt",
];

#[derive(Debug, Clone, Copy)]
enum Scenario {
    DeleteAndModify,
    RenameBinaryLinkAndMode,
    MatchingAndNewSiblings,
    DeleteAgainstModify,
    RemoveDirectoryAgainstChild,
    ReplaceDirectoryAgainstChild,
    LinkAgainstChild,
    DifferentLinks,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn deletion_and_modification_reach_the_consumers_real_cwd() -> Result<(), Box<dyn Error>> {
    for first in [LEFT, RIGHT] {
        let result = run_case(Scenario::DeleteAndModify, first).await?;
        let below = result.consumer()?;
        assert_eq!(
            below.entries["obsolete.txt"], None,
            "the consumer still sees the file its parent deleted: fan-in copied existing files but lost the deletion"
        );
        assert_file(&below, "shared.txt", b"changed by right", false);
        result.assert_private_and_unchanged(&below)?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rename_binary_symlink_and_executable_bit_reach_the_consumer() -> Result<(), Box<dyn Error>>
{
    for first in [LEFT, RIGHT] {
        let result = run_case(Scenario::RenameBinaryLinkAndMode, first).await?;
        let below = result.consumer()?;
        assert_eq!(
            below.entries["old-name.txt"], None,
            "rename must remove the original name"
        );
        assert_file(&below, "new-name.txt", b"rename me", false);
        assert_file(&below, "binary.dat", &[0, 255, 128, 0, 13, 10], false);
        assert_file(&below, "run.sh", b"#!/bin/sh\nexit 0\n", true);
        assert_eq!(
            below.entries["alias"],
            Some(FileFact::Link(PathBuf::from("binary.dat"))),
            "the consumer must see a link with the parent's target, not the target's bytes or the old link"
        );
        result.assert_private_and_unchanged(&below)?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn matching_changes_and_different_children_in_one_new_directory_are_compatible()
-> Result<(), Box<dyn Error>> {
    for first in [LEFT, RIGHT] {
        let result = run_case(Scenario::MatchingAndNewSiblings, first).await?;
        let below = result.consumer()?;
        assert_file(&below, "shared.txt", b"the same change", false);
        assert_eq!(below.entries["obsolete.txt"], None);
        assert_eq!(below.entries["added"], Some(FileFact::Directory));
        assert_file(&below, "added/left.txt", b"left child", false);
        assert_file(&below, "added/right.txt", b"right child", false);
        result.assert_private_and_unchanged(&below)?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn delete_modify_conflict_refuses_the_consumer_and_reaches_history_and_stream()
-> Result<(), Box<dyn Error>> {
    for first in [LEFT, RIGHT] {
        let result = run_case(Scenario::DeleteAgainstModify, first).await?;
        result.assert_refused("obsolete.txt")?;
        let prefix = format!("loadout/{}/", result.report.id);
        assert!(
            git(
                &result.project,
                &["show", &format!("{prefix}{LEFT}:obsolete.txt")]
            )
            .is_err(),
            "the left parent's saved result must still contain its deletion"
        );
        assert_eq!(
            git(
                &result.project,
                &["show", &format!("{prefix}{RIGHT}:obsolete.txt")]
            )?,
            "right still needs this",
            "refusing fan-in must preserve the other parent's own result"
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ancestor_operations_and_different_link_targets_refuse_before_consumer_start()
-> Result<(), Box<dyn Error>> {
    for (scenario, path) in [
        (Scenario::RemoveDirectoryAgainstChild, "feature"),
        (Scenario::ReplaceDirectoryAgainstChild, "feature"),
        (Scenario::LinkAgainstChild, "feature"),
        (Scenario::DifferentLinks, "alias"),
    ] {
        for first in [LEFT, RIGHT] {
            let result = run_case(scenario, first).await?;
            result.assert_refused(path)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FileFact {
    Directory,
    File(Vec<u8>, bool),
    Link(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Observation {
    cwd: PathBuf,
    entries: BTreeMap<String, Option<FileFact>>,
}

fn observe(cwd: &Path) -> io::Result<Observation> {
    let mut entries = BTreeMap::new();
    for relative in PATHS {
        let path = cwd.join(relative);
        let fact = match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                Some(FileFact::Link(fs::read_link(&path)?))
            }
            Ok(meta) if meta.is_dir() => Some(FileFact::Directory),
            Ok(meta) => Some(FileFact::File(
                fs::read(&path)?,
                meta.permissions().mode() & 0o111 != 0,
            )),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                ) =>
            {
                None
            }
            Err(error) => return Err(error),
        };
        entries.insert((*relative).to_owned(), fact);
    }
    Ok(Observation {
        cwd: cwd.to_path_buf(),
        entries,
    })
}

fn assert_file(observation: &Observation, path: &str, bytes: &[u8], executable: bool) {
    assert_eq!(
        observation.entries[path],
        Some(FileFact::File(bytes.to_vec(), executable)),
        "the actual consumer received the wrong content or mode for {path}"
    );
}

#[derive(Debug)]
struct Work {
    scenario: Scenario,
    first: &'static str,
    both: Barrier,
    first_closed: Semaphore,
    // Każdy zamek obejmuje tylko odczyt/zapis obserwacji, nigdy await.
    seen: Mutex<BTreeMap<&'static str, Observation>>,
    parents_at_consumer: Mutex<Vec<Observation>>,
    closed: Mutex<Vec<&'static str>>,
}

struct CaseResult {
    _home: TempDir,
    _project: TempDir,
    project: PathBuf,
    before: Observation,
    work: Arc<Work>,
    report: RunReport,
    delivered: Delivered,
}

impl CaseResult {
    fn consumer(&self) -> Result<Observation, Box<dyn Error>> {
        assert_eq!(
            self.report.steps,
            vec![StepState::Succeeded; 3],
            "{}",
            self.delivered.text()
        );
        let seen = self
            .work
            .seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        seen.get(BELOW)
            .cloned()
            .ok_or_else(|| "the consumer never reached the production driver boundary".into())
    }

    fn assert_private_and_unchanged(&self, below: &Observation) -> Result<(), Box<dyn Error>> {
        let seen = self
            .work
            .seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        assert_ne!(below.cwd, self.project);
        assert!(below.cwd.starts_with(self.report.dir.join("work")));
        for parent in [LEFT, RIGHT] {
            assert_ne!(
                below.cwd, seen[parent].cwd,
                "fan-in must not write into a parent's copy"
            );
        }
        let parents_now = self
            .work
            .parents_at_consumer
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        assert_eq!(
            *parents_now,
            vec![seen[LEFT].clone(), seen[RIGHT].clone()],
            "assembling the private result changed one of its sources"
        );
        assert_eq!(
            observe(&self.project)?,
            self.before,
            "fan-in modified the human's project"
        );
        let expected = if self.work.first == LEFT {
            vec![LEFT, RIGHT]
        } else {
            vec![RIGHT, LEFT]
        };
        assert_eq!(
            *self
                .work
                .closed
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
            expected,
            "the two executions must exercise opposite parent completion orders"
        );
        Ok(())
    }

    fn assert_refused(&self, path: &str) -> Result<(), Box<dyn Error>> {
        assert!(
            !self
                .work
                .seen
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .contains_key(BELOW),
            "the consumer started despite incompatible parent operations on {path}"
        );
        assert_eq!(
            self.report.steps,
            vec![
                StepState::Succeeded,
                StepState::Succeeded,
                StepState::Failed
            ]
        );
        let folder = self
            .report
            .dir
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("run has no folder name")?;
        let history = read_run_inner(&self.project, folder)?;
        let below = history
            .steps
            .iter()
            .find(|step| step.tile == BELOW)
            .ok_or("history lost the consumer")?;
        for said in [&below.error, &self.delivered.text()] {
            assert!(
                said.contains(path) && said.contains(LEFT_NAME) && said.contains(RIGHT_NAME),
                "the visible refusal must name the path and both steps, not just return an internal error: {said}"
            );
        }
        assert_eq!(observe(&self.project)?, self.before);
        // Ten konsument nie pracował. Własna gałąź ze zmianami oznaczałaby częściową publikację
        // przed wykryciem konfliktu, a nie odmowę przed pierwszym zapisem.
        let unexpected = format!("refs/heads/loadout/{}/{BELOW}", self.report.id);
        assert!(
            git(&self.project, &["show-ref", "--verify", &unexpected]).is_err(),
            "a refused merge must not save a partially changed destination as a result"
        );
        Ok(())
    }
}

async fn run_case(scenario: Scenario, first: &'static str) -> Result<CaseResult, Box<dyn Error>> {
    let home = TempDir::new()?;
    let project_dir = TempDir::new()?;
    let project = project_dir.path().to_path_buf();
    fs::create_dir_all(home.path().join("agents"))?;
    fs::create_dir_all(home.path().join("workflows"))?;
    fs::create_dir_all(project.join(".loadout"))?;
    fs::create_dir_all(project.join("feature"))?;
    for (path, bytes) in [
        ("obsolete.txt", "retired"),
        ("shared.txt", "original"),
        ("old-name.txt", "rename me"),
        ("binary.dat", "original bytes"),
        ("run.sh", "#!/bin/sh\nexit 0\n"),
        ("feature/base.txt", "base child"),
        (".gitignore", ".loadout/\n"),
    ] {
        fs::write(project.join(path), bytes)?;
    }
    fs::set_permissions(project.join("run.sh"), fs::Permissions::from_mode(0o644))?;
    supervisor::link(Path::new("obsolete.txt"), &project.join("alias"))?;
    git(&project, &["init", "--quiet"])?;
    git(&project, &["add", "-A"])?;
    git(&project, &["commit", "--quiet", "-m", "WF-02 fixture"])?;
    let before = observe(&project)?;
    let agent_id = "01990000-0000-7000-8000-0000000000a2";
    fs::write(
        home.path().join("agents/scribe.md"),
        format!(
            "---\nschema: 1\nid: {agent_id}\nname: Scribe\nsummary: Writes files\ncolor: slate\nrunsWith: claude-code\nmodel: opus\nthinking: balanced\nfileAccess: work-freely\ngiveUpAfterMinutes: 20\nwriteResultsTo: \"\"\ntools: everything\nskills: []\nconnections: []\n---\nDo the work.\n"
        ),
    )?;
    let workflow = home.path().join("workflows/fan-in.json");
    let steps: Vec<_> = [
        (LEFT, LEFT_NAME, "fresh-copy"),
        (RIGHT, RIGHT_NAME, "fresh-copy"),
        (BELOW, BELOW_NAME, "same-copy"),
    ]
    .into_iter()
    .map(|(id, name, folder)| {
        json!({
            "kind":"agent", "id":id, "name":name, "agent":agent_id,
            "instructions":format!("WF02_TASK={id}"), "folder":{"use":folder}, "at":{"x":0,"y":0}
        })
    })
    .collect();
    fs::write(
        &workflow,
        serde_json::to_vec(&json!({
            "format":1, "id":"wf02_operations", "name":"Preserve parent operations", "steps":steps,
            "links":[{"from":LEFT,"to":BELOW},{"from":RIGHT,"to":BELOW}]
        }))?,
    )?;
    let work = Arc::new(Work {
        scenario,
        first,
        both: Barrier::new(2),
        first_closed: Semaphore::new(0),
        seen: Mutex::new(BTreeMap::new()),
        parents_at_consumer: Mutex::new(Vec::new()),
        closed: Mutex::new(Vec::new()),
    });
    let fake: Arc<dyn AgentDriver> = Arc::new(Fake(Arc::clone(&work)));
    let drivers: Drivers = Arc::new(move |_| Arc::clone(&fake));
    let store = Store::open(&project.join(".loadout/loadout.db"))?;
    let deps = RunDeps {
        home: home.path(),
        project: &project,
        store: &store,
        drivers,
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 2,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let delivered = Delivered::default();
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, delivered.channel());
    let report =
        tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink)).await??;
    tokio::time::timeout(PATIENCE, pump).await??;
    Ok(CaseResult {
        _home: home,
        _project: project_dir,
        project,
        before,
        work,
        report,
        delivered,
    })
}

fn change(cwd: &Path, scenario: Scenario, step: &str) -> io::Result<()> {
    match (scenario, step == LEFT) {
        (Scenario::DeleteAndModify | Scenario::DeleteAgainstModify, true) => {
            fs::remove_file(cwd.join("obsolete.txt"))?;
        }
        (Scenario::DeleteAndModify, false) => {
            fs::write(cwd.join("shared.txt"), "changed by right")?;
        }
        (Scenario::DeleteAgainstModify, false) => {
            fs::write(cwd.join("obsolete.txt"), "right still needs this")?;
        }
        (Scenario::RenameBinaryLinkAndMode, true) => {
            fs::rename(cwd.join("old-name.txt"), cwd.join("new-name.txt"))?;
            fs::remove_file(cwd.join("alias"))?;
            supervisor::link(Path::new("binary.dat"), &cwd.join("alias"))?;
        }
        (Scenario::RenameBinaryLinkAndMode, false) => {
            fs::write(cwd.join("binary.dat"), [0, 255, 128, 0, 13, 10])?;
            fs::set_permissions(cwd.join("run.sh"), fs::Permissions::from_mode(0o755))?;
        }
        (Scenario::MatchingAndNewSiblings, left) => {
            fs::remove_file(cwd.join("obsolete.txt"))?;
            fs::write(cwd.join("shared.txt"), "the same change")?;
            fs::create_dir_all(cwd.join("added"))?;
            let (path, bytes) = if left {
                ("added/left.txt", "left child")
            } else {
                ("added/right.txt", "right child")
            };
            fs::write(cwd.join(path), bytes)?;
        }
        (
            Scenario::RemoveDirectoryAgainstChild
            | Scenario::ReplaceDirectoryAgainstChild
            | Scenario::LinkAgainstChild,
            true,
        ) => {
            fs::remove_dir_all(cwd.join("feature"))?;
            match scenario {
                Scenario::ReplaceDirectoryAgainstChild => {
                    fs::write(cwd.join("feature"), "now a file")?;
                }
                Scenario::LinkAgainstChild => {
                    supervisor::link(Path::new("shared.txt"), &cwd.join("feature"))?;
                }
                _ => {}
            }
        }
        (
            Scenario::RemoveDirectoryAgainstChild
            | Scenario::ReplaceDirectoryAgainstChild
            | Scenario::LinkAgainstChild,
            false,
        ) => fs::write(cwd.join("feature/new.txt"), "new child")?,
        (Scenario::DifferentLinks, left) => {
            fs::remove_file(cwd.join("alias"))?;
            supervisor::link(
                Path::new(if left { "shared.txt" } else { "binary.dat" }),
                &cwd.join("alias"),
            )?;
        }
    }
    Ok(())
}

#[derive(Debug)]
struct Fake(Arc<Work>);

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        "claude-code"
    }
    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("WF-02 fixture".to_owned()),
        })
    }
    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let step = [BELOW, LEFT, RIGHT]
            .into_iter()
            .find(|id| spec.prompt.contains(&format!("WF02_TASK={id}")))
            .ok_or_else(|| anyhow::anyhow!("unknown fixture step"))?;
        if step != BELOW {
            change(&spec.cwd, self.0.scenario, step)?;
        }
        self.0
            .seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(step, observe(&spec.cwd)?);
        if step == BELOW {
            let parent_dirs: Vec<_> = {
                let seen = self.0.seen.lock().unwrap_or_else(PoisonError::into_inner);
                [LEFT, RIGHT]
                    .into_iter()
                    .map(|id| seen[id].cwd.clone())
                    .collect()
            };
            let observed = parent_dirs
                .iter()
                .map(|path| observe(path))
                .collect::<io::Result<Vec<_>>>()?;
            *self
                .0
                .parents_at_consumer
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = observed;
        } else {
            self.0.both.wait().await;
        }
        let session = SessionRef {
            vendor: self.id(),
            id: spec.run_id.to_string(),
        };
        events
            .send(
                AgentEvent::Started {
                    session: session.clone(),
                    model: "opus".to_owned(),
                    tools: Vec::new(),
                    capabilities: Vec::new(),
                }
                .into(),
            )
            .await?;
        Ok(Box::new(Turn {
            work: Arc::clone(&self.0),
            step,
            events,
            session,
        }))
    }
}

#[derive(Debug)]
struct Turn {
    work: Arc<Work>,
    step: &'static str,
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
        if self.step != BELOW && self.step != self.work.first {
            self.work.first_closed.acquire().await?.forget();
        }
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "Completed the assigned work.".to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session.clone(),
        };
        self.events
            .send(AgentEvent::Finished(outcome.clone()).into())
            .await?;
        Ok(outcome)
    }
    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }
    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        if self.step != BELOW {
            self.work
                .closed
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(self.step);
            if self.step == self.work.first {
                self.work.first_closed.add_permits(1);
            }
        }
        Ok(Some(0))
    }
}

fn git(at: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(at)
        .args([
            "-c",
            "user.name=Loadout Test",
            "-c",
            "user.email=test@loadout.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(format!("git {args:?}: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[derive(Debug, Clone, Default)]
struct Delivered(Arc<Mutex<Vec<String>>>);

impl Delivered {
    fn channel(&self) -> tauri::ipc::Channel<Vec<loadout_lib::engine::line::Line>> {
        let seen = Arc::clone(&self.0);
        tauri::ipc::Channel::new(move |body| {
            if let tauri::ipc::InvokeResponseBody::Json(text) = body {
                seen.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(text);
            }
            Ok(())
        })
    }
    fn text(&self) -> String {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .join("\n")
    }
}
