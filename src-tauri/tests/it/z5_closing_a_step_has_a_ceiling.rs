//! Domknięcie kroku ma sufit, widoczną przyczynę i reaguje na Stop także po wyniku tury.
//!
//! Pierwszy test trzyma prawdziwe grupy obu sterowników nad procesami, które ignorują EOF.
//! Oba `close()` biegną w jednym oknie czasu; osobny dubler przepuszcza ten sam typ końca przez
//! historię używaną przez okno (niezmiennik 29). Drugi test zatrzymuje bieg dokładnie wtedy,
//! gdy `close()` już czeka bez końca, więc nie może zazielenić się na wcześniejszej gałęzi Stopu.

#![allow(clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::future;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::run::{run_workflow_with_reflection, stop_run_inner};
use loadout_lib::commands::{Drivers, Outcome, RunReport, RunRequest};
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, DidNotLetGo, FinishReason,
    Outcome as TurnOutcome, Policy, Probe, RunSpec, SessionRef, Tokens, ValidatedImages,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{CLOSE_CEILING, GroupId, GroupProof};
use loadout_lib::ipc::{AppState, QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::{Notify, mpsc};
use tokio::time::timeout;
use uuid::Uuid;

const PATIENCE: Duration = Duration::from_secs(18);
const QUICK: Duration = Duration::from_secs(5);
const STEP_WOULD_NOT_LET_GO_ERROR: &str = "This step finished its work, but the agent kept going \
after Loadout closed its input, so Loadout stopped it.";

/// Nie czyta stdin i nie wychodzi po EOF. TERM pozostaje domyślny, żeby test mierzył sufit
/// zamknięcia, a nie ponownie pięciosekundową eskalację supervisora (2026-09).
const IGNORES_EOF: &str = r"#!/bin/sh
while :; do
  sleep 0.2
done
";

/// App Server kończy pętlę protokołu po EOF, lecz sam proces zostaje żywy. To rozdziela jego
/// osobny uchwyt od drogi `codex exec`, choć oba należą do tego samego sterownika (2026-09).
const APP_SERVER_IGNORES_EOF: &str = r#"#!/bin/sh
while IFS= read -r line; do
  id="$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')"
  case "$line" in
    *'"method":"initialize"'*)
      printf '{"id":%s,"result":{}}\n' "${id:-1}"
      ;;
    *'"method":"config/read"'*)
      printf '{"id":%s,"result":{"config":{"mcp_servers":{}},"origins":{}}}\n' "${id:-2}"
      ;;
    *'"method":"thread/start"'*)
      printf '{"id":%s,"result":{"thread":{"id":"z5-thread","ephemeral":true,"path":null}}}\n' "${id:-3}"
      ;;
    *'"method":"turn/start"'*)
      printf '{"id":%s,"result":{"turn":{"id":"z5-turn","status":"inProgress"}}}\n' "${id:-4}"
      ;;
  esac
done
while :; do
  sleep 0.2
done
"#;

const AGENT: &str = r#"---
schema: 1
id: 01990000-0000-7000-8000-000000000005
name: Z5 Builder
summary: Exercises closing a finished step
color: slate
runsWith: claude-code
model: haiku
thinking: balanced
fileAccess: look-only
giveUpAfterMinutes: 20
writeResultsTo: ""
tools: everything
skills: []
connections: []
---
Exercise step closing.
"#;

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_z5_close",
  "name": "Z5 close",
  "steps": [{
    "kind": "agent",
    "id": "build",
    "name": "Build",
    "agent": "01990000-0000-7000-8000-000000000005",
    "overrides": {},
    "instructions": "Finish, then close.",
    "folder": { "use": "project" },
    "at": { "x": 0, "y": 0 }
  }],
  "links": []
}"#;

fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn spec(cwd: &Path) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: "finish once".to_owned(),
        model: None,
        system_append: None,
        reaches_the_web: false,
        policy: Policy::ReadOnly,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

#[allow(unsafe_code)]
fn group_probe(pgid: i32) -> io::Result<()> {
    // SAFETY: sygnał zero nie jest dostarczany; ujemny PGID adresuje grupę atrapy.
    let rc = unsafe { libc::kill(-pgid, 0) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn both_drivers_close_stubborn_processes_in_one_window_and_history_names_it()
-> Result<(), Box<dyn Error>> {
    let scripts = TempDir::new()?;
    let claude_binary = write_script(scripts.path(), "claude", IGNORES_EOF)?;
    let codex_binary = write_script(scripts.path(), "codex", IGNORES_EOF)?;
    let app_server_binary =
        write_script(scripts.path(), "codex-app-server", APP_SERVER_IGNORES_EOF)?;
    let (claude_tx, _claude_events) = mpsc::channel(16);
    let (codex_tx, _codex_events) = mpsc::channel(16);
    let (app_server_tx, _app_server_events) = mpsc::channel(16);
    let claude_driver = ClaudeDriver::with_binary(claude_binary);
    let codex_driver = CodexDriver::with_binary(codex_binary);
    let app_server_driver = CodexDriver::with_binary(app_server_binary);
    let mut claude = claude_driver.start(spec(scripts.path()), claude_tx).await?;
    let mut codex = codex_driver
        .start_session(spec(scripts.path()), codex_tx)
        .await?;
    let mut app_server = app_server_driver
        .start_conversation(
            spec(scripts.path()),
            ValidatedImages::default(),
            app_server_tx,
        )
        .await?;
    let claude_group = claude
        .group()
        .ok_or("the Claude fixture has no process group")?;
    let codex_group = codex
        .group()
        .ok_or("the Codex fixture has no process group")?;
    let app_server_group = app_server
        .group()
        .ok_or("the Codex App Server fixture has no process group")?;
    assert!(group_probe(claude_group.pgid).is_ok());
    assert!(group_probe(codex_group.pgid).is_ok());
    assert!(group_probe(app_server_group.pgid).is_ok());

    let began = Instant::now();
    let (claude_closed, codex_closed, app_server_closed) = timeout(PATIENCE, async {
        tokio::join!(claude.close(), codex.close(), app_server.close())
    })
    .await?;
    let took = began.elapsed();

    for (vendor, closed) in [
        ("Claude", claude_closed),
        ("Codex exec", codex_closed),
        ("Codex App Server", app_server_closed),
    ] {
        let error = closed.expect_err("a process ignoring EOF must not be accepted as closed");
        assert!(
            error.is::<DidNotLetGo>(),
            "{vendor} returned the wrong close failure after its ceiling: {error:#}"
        );
    }
    assert!(
        took >= CLOSE_CEILING && took < PATIENCE,
        "both closes should share one {CLOSE_CEILING:?} window; they took {took:?}"
    );
    for group in [claude_group, codex_group, app_server_group] {
        assert_eq!(
            group_probe(group.pgid)
                .err()
                .and_then(|error| error.raw_os_error()),
            Some(libc::ESRCH),
            "close() returned while kill(-{}, 0) still found the supervised group",
            group.pgid
        );
    }

    let bench = Bench::new()?;
    let fake = Arc::new(Fake::new(CloseBehavior::DidNotLetGo));
    let app = bench.app(fake_drivers(Arc::clone(&fake)))?;
    let deps = app
        .begin_run(bench.project.path())
        .map_err(io::Error::other)?;
    let (lines, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let request = bench.request();
    let report = timeout(
        QUICK,
        run_workflow_with_reflection(&deps, &request, lines, None, false),
    )
    .await??;
    timeout(QUICK, pump).await??;

    assert_eq!(report.steps, vec![StepState::Failed]);
    let past = read_run_inner(bench.project.path(), run_folder(&report)?)?;
    assert_eq!(
        past.steps[0].error, STEP_WOULD_NOT_LET_GO_ERROR,
        "the history command used by the window lost the reason this completed work was refused"
    );
    assert_eq!(fake.proof_calls.load(Ordering::Acquire), 1);
    assert_eq!(fake.cancel_calls.load(Ordering::Acquire), 0);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stop_during_close_returns_a_cancelled_report() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let fake = Arc::new(Fake::new(CloseBehavior::NeverReturns));
    let app = bench.app(fake_drivers(Arc::clone(&fake)))?;
    let deps = app
        .begin_run(bench.project.path())
        .map_err(io::Error::other)?;
    let (lines, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let request = bench.request();
    let run = run_workflow_with_reflection(&deps, &request, lines, None, false);
    tokio::pin!(run);

    let closing = fake.close_started.notified();
    tokio::pin!(closing);
    tokio::select! {
        () = &mut closing => {}
        result = &mut run => {
            return Err(format!("the run returned before its handle entered close(): {result:?}").into());
        }
    }

    let (stopped, ran) = timeout(QUICK, async {
        tokio::join!(stop_run_inner(&deps), &mut run)
    })
    .await?;
    let stopped = stopped?;
    let report = ran?;
    timeout(QUICK, pump).await??;

    assert_eq!(stopped, Outcome::Cancelled);
    assert_eq!(report.outcome, Outcome::Cancelled);
    assert_eq!(report.steps, vec![StepState::Cancelled]);
    assert_eq!(fake.proof_calls.load(Ordering::Acquire), 1);
    assert_eq!(
        fake.cancel_calls.load(Ordering::Acquire),
        0,
        "a Stop that arrived during close must take the bounded proof path, not restart live-turn cancellation"
    );
    let past = read_run_inner(bench.project.path(), run_folder(&report)?)?;
    assert!(past.steps[0].error.is_empty());
    Ok(())
}

fn run_folder(report: &RunReport) -> Result<&str, Box<dyn Error>> {
    report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "the run directory has no UTF-8 folder name".into())
}

struct Bench {
    home: TempDir,
    project: TempDir,
    workflow: PathBuf,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(project.path().join(".loadout/agents"))?;
        fs::create_dir_all(project.path().join(".loadout/workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(
            project.path().join(".loadout/agents").join("z5-builder.md"),
            AGENT,
        )?;
        let workflow = project
            .path()
            .join(".loadout/workflows")
            .join("z5-close.json");
        fs::write(&workflow, WORKFLOW)?;
        Ok(Self {
            home,
            project,
            workflow,
        })
    }

    fn app(&self, drivers: Drivers) -> Result<AppState, Box<dyn Error>> {
        let store = Store::open(&self.project.path().join(".loadout").join("loadout.db"))?;
        Ok(AppState::new(
            self.home.path().to_path_buf(),
            self.project.path().to_path_buf(),
            store,
            drivers,
        ))
    }

    fn request(&self) -> RunRequest {
        RunRequest {
            workflow: self.workflow.clone(),
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        }
    }
}

#[derive(Clone, Copy)]
enum CloseBehavior {
    DidNotLetGo,
    NeverReturns,
}

struct Fake {
    behavior: CloseBehavior,
    close_started: Arc<Notify>,
    proof_calls: Arc<AtomicUsize>,
    cancel_calls: Arc<AtomicUsize>,
}

impl Fake {
    fn new(behavior: CloseBehavior) -> Self {
        Self {
            behavior,
            close_started: Arc::new(Notify::new()),
            proof_calls: Arc::new(AtomicUsize::new(0)),
            cancel_calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

fn fake_drivers(fake: Arc<Fake>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = fake;
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        "z5-fake"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("z5".to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        Ok(Box::new(Turn {
            session: SessionRef {
                vendor: "z5-fake",
                id: spec.run_id.to_string(),
            },
            behavior: self.behavior,
            events,
            close_started: Arc::clone(&self.close_started),
            proof_calls: Arc::clone(&self.proof_calls),
            cancel_calls: Arc::clone(&self.cancel_calls),
        }))
    }
}

struct Turn {
    session: SessionRef,
    behavior: CloseBehavior,
    events: mpsc::Sender<DecodedEvent>,
    close_started: Arc<Notify>,
    proof_calls: Arc<AtomicUsize>,
    cancel_calls: Arc<AtomicUsize>,
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
            text: "Done.".to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::ZERO,
            session: self.session.clone(),
        };
        self.events
            .send(AgentEvent::Finished(outcome.clone()).into())
            .await
            .map_err(|_| anyhow::anyhow!("the Z-5 event receiver closed before Finished"))?;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> GroupProof {
        self.cancel_calls.fetch_add(1, Ordering::AcqRel);
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        self.close_started.notify_one();
        match self.behavior {
            CloseBehavior::DidNotLetGo => Err(DidNotLetGo.into()),
            CloseBehavior::NeverReturns => future::pending().await,
        }
    }

    async fn proof_of_death(&mut self) -> GroupProof {
        self.proof_calls.fetch_add(1, Ordering::AcqRel);
        GroupProof::Dead { status: None }
    }
}
