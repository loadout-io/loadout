//! AC-5 dla T-52: kiedy izolacji zrobić się nie da, nie rusza ANI JEDEN proces, a zdanie
//! nazywa krok i powód.
//!
//! To jest obietnica T-33 AC-2 przeniesiona na nowy mechanizm i ona się nie zmienia: cicha
//! degradacja do wspólnego katalogu jest groźniejsza niż odmowa, bo dwa kroki przepisują sobie
//! wtedy pliki nawzajem, a każdy kończy się „sukcesem".
//!
//! Warunek wymuszamy repozytorium **bez ani jednego commita**: git nie ma wtedy z czego założyć
//! drzewa. To celuje dokładnie w izolację — katalog biegu powstaje normalnie, plan przechodzi,
//! a dopiero drzewo nie ma jak powstać.
//!
//! **Słaba wersja tego kryterium:** `assert!(result.is_err())`. Przechodzi ją implementacja,
//! która wywala bieg bez powiedzenia dlaczego, i przechodzi ją `RunError::Io`, czyli zdanie
//! o tym, co nie udało się SYSTEMOWI.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunError, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use tempfile::TempDir;
use tokio::sync::mpsc;

const VENDOR: &str = "claude-code";
const PATIENCE: Duration = Duration::from_secs(20);
/// Nazwa kroku, której człowiek szuka na płótnie — i której ma szukać w zdaniu odmowy.
const STEP: &str = "Groundwork";

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_no_isolation",
  "name": "One step that cannot be isolated",
  "steps": [
    {
      "kind": "agent",
      "id": "s_one",
      "name": "Groundwork",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "do the thing",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

const AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000a1
name: Scribe
summary: Writes things down
color: slate
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_repo_with_no_commits_stops_the_run_with_a_sentence() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    fs::write(bench.project.path().join("notes.txt"), "not committed yet")?;
    // Repozytorium JEST, commita nie ma — `git worktree add` nie ma z czego wyjść.
    git(bench.project.path(), &["init", "--quiet"])?;
    assert!(
        bench.project.path().join(".git").exists(),
        "the repository was not created, so this test would measure the copy path instead"
    );
    assert!(
        git(bench.project.path(), &["rev-parse", "--verify", "HEAD"]).is_err(),
        "this repository has a commit, so `git worktree add` would work and the criterion \
         would measure nothing"
    );

    let started = Arc::new(AtomicUsize::new(0));
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&started)),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: bench.workflow("no-isolation", WORKFLOW)?,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let recorder = Delivered::default();
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, recorder.channel());
    let outcome = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| "the run never came back")?;
    let _ = tokio::time::timeout(PATIENCE, pump).await;

    let error = match outcome {
        Ok(report) => {
            return Err(format!(
                "the run reported {report:?} instead of refusing. Quietly falling back to a \
                 shared folder is worse than refusing: two steps would then write over each \
                 other and both would end as successes"
            )
            .into());
        }
        Err(error) => error,
    };

    // (a) Ani jeden proces nie ruszył.
    assert_eq!(
        started.load(Ordering::SeqCst),
        0,
        "an agent was started even though its tree could not be made. The refusal has to come \
         BEFORE anything runs, or the money is already spent and the isolation was never there"
    );

    // (d) …i to nie jest przezroczysty błąd systemu plików.
    assert!(
        !matches!(error, RunError::Io(_)),
        "the run failed with RunError::Io, which is transparent and hands the person a sentence \
         about the file system: {error}"
    );

    // (b) Zdanie nazywa KROK…
    let said = error.to_string();
    assert!(
        said.contains(STEP),
        "the refusal does not name the step. A person looks for a tile on the canvas, not for \
         an id: {said}"
    );
    // (c) …i mówi, co z tym zrobić.
    let actionable = said.contains("commit") || said.contains("project folder");
    assert!(
        actionable,
        "the refusal says what happened and stops there. A repository with no commits is fixed \
         in one of two ways — make the first commit, or run the step in the project folder — \
         and a message that names neither leaves the person exactly where they were: {said}"
    );

    Ok(())
}

// ── dubler, który LICZY starty ─────────────────────────────────────────────────────────────

fn fake_drivers(started: Arc<AtomicUsize>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { started });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct Fake {
    started: Arc<AtomicUsize>,
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
        self.started.fetch_add(1, Ordering::SeqCst);
        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };
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
            text: String::new(),
            cost_usd: None,
            tokens: Tokens::default(),
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

// ── ławka ──────────────────────────────────────────────────────────────────────────────────

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
        fs::write(home.path().join("agents").join("scribe.md"), AGENT)?;
        Ok(Self { home, project })
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

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }
}

fn git(at: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(at)
        .args(["-c", "user.name=Loadout Test"])
        .args(["-c", "user.email=test@loadout.invalid"])
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

#[derive(Debug, Clone, Default)]
struct Delivered(Arc<Mutex<Vec<serde_json::Value>>>);

impl Delivered {
    fn channel(&self) -> tauri::ipc::Channel<Vec<loadout_lib::engine::line::Line>> {
        let sink = Arc::clone(&self.0);
        tauri::ipc::Channel::new(move |body| {
            if let tauri::ipc::InvokeResponseBody::Json(text) = body
                && let Ok(value) = serde_json::from_str(&text)
            {
                sink.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(value);
            }
            Ok(())
        })
    }
}
