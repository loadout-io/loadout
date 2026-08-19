//! AC-4 dla T-52: praca kroku jest po biegu OSIĄGALNA Z GITA, a krok, który nic nie zmienił,
//! nie zostawia po sobie śmiecia.
//!
//! To jest połowa, której nie miała stara kopia i której brak był większą dziurą niż dowiązanie:
//! `copy_project_into` był JEDYNYM transportem w `commands/run.rs` — drogi powrotnej nie było
//! żadnej. Cokolwiek agent napisał we własnej kopii, zostawało w
//! `.loadout/runs/<ts>/work/<krok>/` na zawsze i nie docierało do projektu nigdy.
//!
//! **Słaba wersja tego kryterium:** sprawdzenie, że katalog roboczy dalej istnieje. Istnieje
//! też dzisiaj — i jest dokładnie tym miejscem, z którego nikt nigdy pracy nie wyjął.
//! Rozstrzyga OSIĄGALNOŚĆ Z GITA.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use tempfile::TempDir;
use tokio::sync::mpsc;

const VENDOR: &str = "claude-code";
const PATIENCE: Duration = Duration::from_secs(20);

/// Plik, który pisze PIERWSZY krok — i tylko on.
const MADE: &str = "the-work.txt";
const MADE_TEXT: &str = "this is what the agent produced";

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_reachable",
  "name": "One writes, one does not",
  "steps": [
    {
      "kind": "agent",
      "id": "s_writes",
      "name": "Writes",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "write the file",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_idles",
      "name": "Idles",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "touch nothing",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
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
async fn the_work_lands_on_a_branch_and_an_idle_step_leaves_nothing() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    fs::write(bench.project.path().join("notes.txt"), "the human's file")?;
    bench.make_a_repo()?;
    let base = bench.head()?;

    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: bench.workflow("reachable", WORKFLOW)?,
        how_many_at_once: 2,
        task: None,
    };

    let recorder = Delivered::default();
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, recorder.channel());
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| "the run never came back")??;
    let _ = tokio::time::timeout(PATIENCE, pump).await;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded, StepState::Succeeded],
        "both steps have to finish; they ended as {:?}",
        report.steps
    );

    // (a) Gałąź niosąca pracę istnieje i jej nazwa wymienia krok.
    let branches = bench.branches()?;
    let carrying: Vec<&String> = branches
        .iter()
        .filter(|name| name.contains("s_writes"))
        .collect();
    assert_eq!(
        carrying.len(),
        1,
        "exactly one branch should carry the step that wrote something, and its name has to say \
         WHICH step, because a person reading `git branch` a day later has nothing else to go \
         on. Branches after the run: {branches:?}"
    );
    let branch = carrying[0];

    // (b) Zmiana jest na niej osiągalna Z GITA — nie „leży w jakimś katalogu".
    let log = bench.git(&["log", "--oneline", branch])?;
    assert!(
        !log.trim().is_empty(),
        "the branch {branch} has no commit on it, so the work is not reachable: the files sit in \
         a folder exactly as they did before this change"
    );
    let changed = bench.git(&["diff", "--name-only", &format!("{base}..{branch}")])?;
    assert!(
        changed.contains(MADE),
        "`git diff {base}..{branch}` does not mention {MADE}. A branch that does not carry the \
         work is the same dead end as the old copy, only with a nicer name. It listed: {changed}"
    );

    // (c) Krok, który nic nie zmienił, nie zostawia ani gałęzi, ani wpisu w liście drzew.
    assert!(
        !branches.iter().any(|name| name.contains("s_idles")),
        "the step that changed nothing left a branch behind. After a week of runs `git branch` \
         is unreadable and the branches that do carry work are lost among them. Branches: \
         {branches:?}"
    );
    let trees = bench.git(&["worktree", "list"])?;
    assert!(
        !trees.contains("s_idles"),
        "the idle step is still registered as a work tree. Every run would then add entries \
         nobody removes: {trees}"
    );

    Ok(())
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers() -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake);
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct Fake;

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
        // Krok rozpoznajemy po zadaniu: tylko jeden z dwóch ma cokolwiek napisać.
        if spec.prompt.starts_with("write") {
            fs::write(spec.cwd.join(MADE), MADE_TEXT)?;
        }
        let session = SessionRef {
            vendor: VENDOR,
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

    fn make_a_repo(&self) -> Result<(), Box<dyn Error>> {
        self.git(&["init", "--quiet"])?;
        fs::write(self.project.path().join(".gitignore"), ".loadout/\n")?;
        self.git(&["add", "-A"])?;
        self.git(&["commit", "--quiet", "-m", "the human's first commit"])?;
        Ok(())
    }

    fn head(&self) -> Result<String, Box<dyn Error>> {
        Ok(self.git(&["rev-parse", "HEAD"])?.trim().to_owned())
    }

    /// Nazwy gałęzi, po jednej w wierszu.
    fn branches(&self) -> Result<Vec<String>, Box<dyn Error>> {
        Ok(self
            .git(&["branch", "--format=%(refname:short)"])?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect())
    }

    fn git(&self, args: &[&str]) -> Result<String, Box<dyn Error>> {
        git(self.project.path(), args)
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
