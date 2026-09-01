//! T-152 AC-3: receipt odróżnia wejście w próbę logiczną od przejęcia procesu.

#![allow(clippy::too_many_lines)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::processes::Processes;
use loadout_lib::commands::run::{continue_run_inner, run_workflow_inner};
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::line_channel;
use loadout_lib::library::agents::Vendor;
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::mpsc;

const PATIENCE: Duration = Duration::from_secs(30);
const AGENT_ID: &str = "019b0152-0000-7000-8000-000000000003";

const AGENT: &str = r#"---
schema: 1
id: 019b0152-0000-7000-8000-000000000003
name: Execution-fact worker
summary: Separates logical work from process ownership
color: moss
runsWith: claude-code
model: sonnet
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 20
writeResultsTo: ""
tools: everything
skills: []
connections: []
---
Follow the step instructions.
"#;

#[derive(Debug)]
struct Bench {
    _home: TempDir,
    _project: TempDir,
    _scripts: TempDir,
    home: PathBuf,
    project: PathBuf,
    scripts: PathBuf,
    starts: Arc<Mutex<Vec<String>>>,
}

impl Bench {
    fn new(git_project: bool) -> Result<Self, Box<dyn Error>> {
        let home = tempfile::tempdir()?;
        let project = tempfile::tempdir()?;
        let scripts = tempfile::tempdir()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(home.path().join("agents/execution-fact.md"), AGENT)?;
        if git_project {
            git_ok(project.path(), &["init", "--quiet"])?;
            git_ok(
                project.path(),
                &["config", "user.email", "t152@example.test"],
            )?;
            git_ok(project.path(), &["config", "user.name", "T152"])?;
            fs::write(project.path().join("tracked.txt"), b"baseline\n")?;
            git_ok(project.path(), &["add", "tracked.txt"])?;
            git_ok(project.path(), &["commit", "--quiet", "-m", "baseline"])?;
        }
        Ok(Self {
            home: home.path().to_path_buf(),
            project: project.path().to_path_buf(),
            scripts: scripts.path().to_path_buf(),
            _home: home,
            _project: project,
            _scripts: scripts,
            starts: Arc::new(Mutex::new(Vec::new())),
        })
    }

    fn database(&self) -> PathBuf {
        self.project.join(".loadout/loadout.db")
    }

    fn script(&self, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.scripts.join(name);
        fs::write(&path, body)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        Ok(path)
    }

    fn workflow(&self, name: &str, value: &Value) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.home.join("workflows").join(format!("{name}.json"));
        fs::write(&path, serde_json::to_vec_pretty(value)?)?;
        Ok(path)
    }

    async fn run(
        &self,
        workflow: PathBuf,
        answer_checkpoint: bool,
    ) -> Result<RunResult, Box<dyn Error>> {
        lock(&self.starts).clear();
        let store = Store::open(&self.database())?;
        let processes = Arc::new(Processes::new());
        let deps = RunDeps {
            home: &self.home,
            project: &self.project,
            store: &store,
            drivers: drivers(Arc::clone(&self.starts)),
            processes: Arc::clone(&processes),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow,
            how_many_at_once: 4,
            task: None,
            part: None,
            handoffs_from: None,
        };
        let (sink, _source) = line_channel(4_096);
        let run = run_workflow_inner(&deps, &request, sink);
        let answer = answer_if_asked(&deps, &self.project, answer_checkpoint);
        let (report, answered) =
            tokio::time::timeout(PATIENCE, async { tokio::join!(run, answer) })
                .await
                .map_err(|_| "the execution-fact workflow exceeded its bounded patience")?;
        let report = report?;
        let answered = answered?;
        let run_file = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
        Ok(RunResult {
            run_file,
            starts: lock(&self.starts).clone(),
            processes,
            answered,
        })
    }
}

#[derive(Debug)]
struct RunResult {
    run_file: Value,
    starts: Vec<String>,
    processes: Arc<Processes>,
    answered: bool,
}

async fn answer_if_asked(
    deps: &RunDeps<'_>,
    project: &Path,
    enabled: bool,
) -> Result<bool, loadout_lib::commands::RunError> {
    if !enabled {
        return Ok(false);
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if latest_run(project)
            .is_some_and(|run| run.get("status").and_then(Value::as_str) == Some("paused"))
        {
            continue_run_inner(deps, Some("Continue".to_owned())).await?;
            return Ok(true);
        }
        if !deps.control.is_working()
            && latest_run(project).is_some_and(|run| {
                !matches!(
                    run.get("status").and_then(Value::as_str),
                    Some("running" | "paused")
                )
            })
        {
            return Ok(false);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn receipt_records_every_physical_execution_path() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new(false)?;
    let check = bench.script("check.sh", "#!/bin/sh\necho '1 passed'\n")?;
    let serve = bench.script("serve.sh", "#!/bin/sh\ntouch \"$1\"\nsleep 600\n")?;
    let served = bench.project.join("serve-started");
    let workflow = json!({
        "format": 1,
        "id": "wf_t152_execution_paths",
        "name": "Execution paths",
        "steps": [
            agent("start_error", "Start error", "start-error", "fresh-copy", Some("carry-on")),
            {
                "kind": "checkpoint",
                "id": "checkpoint",
                "name": "Choose after refusal",
                "question": "Continue after the start refusal?",
                "at": { "x": 200, "y": 0 }
            },
            agent("agent_ok", "Agent process", "agent-success", "fresh-copy", None),
            agent("stop_error", "Stopping error", "stop-error", "fresh-copy", Some("stop")),
            agent("skipped", "Skipped child", "must-not-start", "fresh-copy", None),
            {
                "kind": "check",
                "id": "check",
                "name": "Check process",
                "command": check.to_string_lossy(),
                "proof": "(\\d+) passed",
                "folder": { "use": "fresh-copy" },
                "at": { "x": 0, "y": 300 }
            },
            {
                "kind": "serve",
                "id": "serve",
                "name": "Serve process",
                "command": format!("{} {}", serve.display(), served.display()),
                "folder": { "use": "fresh-copy" },
                "at": { "x": 200, "y": 300 }
            }
        ],
        "links": [
            { "from": "start_error", "to": "checkpoint" },
            { "from": "checkpoint", "to": "agent_ok" },
            { "from": "stop_error", "to": "skipped" }
        ]
    });
    let result = bench
        .run(bench.workflow("execution-paths", &workflow)?, true)
        .await?;

    let _ = result.processes.close().await;

    assert!(
        result.answered,
        "the start-error carry-on never reached the checkpoint"
    );
    assert!(served.exists(), "the Serve command did not really start");
    assert_facts(row(&result.run_file, "Start error")?, true, false)?;
    assert_facts(row(&result.run_file, "Choose after refusal")?, true, false)?;
    assert_facts(row(&result.run_file, "Agent process")?, true, true)?;
    assert_facts(row(&result.run_file, "Check process")?, true, true)?;
    assert_facts(row(&result.run_file, "Serve process")?, true, true)?;
    assert_facts(row(&result.run_file, "Skipped child")?, false, false)?;

    assert!(
        !result
            .starts
            .iter()
            .any(|prompt| prompt.contains("must-not-start")),
        "a child left behind by an earlier failure reached AgentDriver::start"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn no_work_and_already_settled_rounds_never_claim_execution() -> Result<(), Box<dyn Error>> {
    let no_work = Bench::new(true)?;
    let no_work_run = no_work
        .run(
            no_work.workflow("no-work", &loop_workflow("loop-no-work", "judge-no-work"))?,
            false,
        )
        .await?;
    assert_facts(row_key(&no_work_run.run_file, "implement")?, true, true)?;
    for key in ["judge", "implement#1", "judge#1", "implement#2", "judge#2"] {
        assert_facts(row_key(&no_work_run.run_file, key)?, false, false)?;
    }
    assert!(
        !no_work_run
            .starts
            .iter()
            .any(|prompt| prompt.contains("judge-no-work")),
        "nothing_to_judge still started a driver"
    );

    let settled = Bench::new(true)?;
    let settled_run = settled
        .run(
            settled.workflow("settled", &loop_workflow("loop-change", "judge-pass"))?,
            false,
        )
        .await?;
    assert_facts(row_key(&settled_run.run_file, "implement")?, true, true)?;
    assert_facts(row_key(&settled_run.run_file, "judge")?, true, true)?;
    for key in ["implement#1", "judge#1", "implement#2", "judge#2"] {
        assert_facts(row_key(&settled_run.run_file, key)?, false, false)?;
    }
    Ok(())
}

#[test]
fn legacy_history_reports_execution_as_unknown() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let folder = "20260828-120000__019b0152-0000-7000-8000-000000000004";
    let run_dir = project.path().join(".loadout/runs").join(folder);
    fs::create_dir_all(&run_dir)?;
    fs::write(
        run_dir.join("run.json"),
        serde_json::to_vec_pretty(&json!({
            "id": "019b0152-0000-7000-8000-000000000004",
            "workflow_id": "legacy.json",
            "workflow_hash": "legacy",
            "workflow_snapshot": { "format": 1 },
            "title": "Legacy run",
            "status": "succeeded",
            "concurrency": 1,
            "created_at": 1787918400000_i64,
            "started_at": 1787918400001_i64,
            "ended_at": 1787918400002_i64,
            "steps": [{
                "id": "019b0152-0000-7000-8000-000000000005",
                "node_key": "legacy",
                "name": "Legacy step",
                "agent": "claude",
                "kind": "agent",
                "depends_on": [],
                "status": "succeeded",
                "attempt": 0,
                "pid": 4242,
                "started_at": 1787918400001_i64,
                "ended_at": 1787918400002_i64,
                "summary": "Old receipt",
                "error": null
            }]
        }))?,
    )?;

    let wire = serde_json::to_value(read_run_inner(project.path(), folder)?)?;
    assert_eq!(
        wire.pointer("/steps/0/executed"),
        Some(&Value::Null),
        "history inferred execution from a legacy status, timestamp or PID instead of saying unknown: {wire:?}"
    );
    Ok(())
}

fn agent(id: &str, name: &str, instructions: &str, folder: &str, failure: Option<&str>) -> Value {
    let mut value = json!({
        "kind": "agent",
        "id": id,
        "name": name,
        "agent": AGENT_ID,
        "overrides": {},
        "instructions": instructions,
        "folder": { "use": folder },
        "at": { "x": 0, "y": 0 }
    });
    if let Some(failure) = failure {
        value["whenItFails"] = Value::String(failure.to_owned());
    }
    value
}

fn loop_workflow(implement: &str, judge: &str) -> Value {
    json!({
        "format": 1,
        "id": format!("wf_t152_{implement}"),
        "name": format!("Loop {implement}"),
        "steps": [
            agent("implement", "Implement", implement, "fresh-copy", None),
            agent("judge", "Judge", judge, "fresh-copy", None)
        ],
        "links": [
            { "from": "implement", "to": "judge" },
            { "from": "judge", "to": "implement", "max_turns": 3 }
        ]
    })
}

fn assert_facts(step: &Value, executed: bool, process_started: bool) -> Result<(), Box<dyn Error>> {
    assert_eq!(
        step.get("executed"),
        Some(&Value::Bool(executed)),
        "wrong executed fact for {}: {step:?}",
        step.get("name")
            .and_then(Value::as_str)
            .unwrap_or("unnamed step")
    );
    assert_eq!(
        step.get("process_started"),
        Some(&Value::Bool(process_started)),
        "wrong process_started fact for {}: {step:?}",
        step.get("name")
            .and_then(Value::as_str)
            .unwrap_or("unnamed step")
    );
    if !executed {
        for key in [
            "started_at",
            "pid",
            "pgid",
            "agent_session_id",
            "cost_usd",
            "turns",
            "input_tokens",
            "output_tokens",
            "cached_tokens",
        ] {
            assert!(
                step.get(key).is_none_or(Value::is_null),
                "a step that did not execute claimed {key}: {step:?}"
            );
        }
    }
    if !process_started {
        assert!(step.get("pid").is_none_or(Value::is_null));
        assert!(step.get("pgid").is_none_or(Value::is_null));
    }
    Ok(())
}

fn row<'a>(run: &'a Value, name: &str) -> Result<&'a Value, Box<dyn Error>> {
    run.get("steps")
        .and_then(Value::as_array)
        .and_then(|steps| {
            steps
                .iter()
                .find(|step| step.get("name").and_then(Value::as_str) == Some(name))
        })
        .ok_or_else(|| format!("run.json has no step named {name}").into())
}

fn row_key<'a>(run: &'a Value, key: &str) -> Result<&'a Value, Box<dyn Error>> {
    run.get("steps")
        .and_then(Value::as_array)
        .and_then(|steps| {
            steps
                .iter()
                .find(|step| step.get("node_key").and_then(Value::as_str) == Some(key))
        })
        .ok_or_else(|| format!("run.json has no physical node {key}").into())
}

fn latest_run(project: &Path) -> Option<Value> {
    fs::read_dir(project.join(".loadout/runs"))
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| fs::read(entry.path().join("run.json")).ok())
        .filter_map(|bytes| serde_json::from_slice(&bytes).ok())
        .max_by_key(|run: &Value| run.get("created_at").and_then(Value::as_i64).unwrap_or(0))
}

fn drivers(starts: Arc<Mutex<Vec<String>>>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(FakeDriver { starts });
    Arc::new(move |_vendor: Vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct FakeDriver {
    starts: Arc<Mutex<Vec<String>>>,
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
        lock(&self.starts).push(spec.prompt.clone());
        if spec.prompt.contains("start-error") || spec.prompt.contains("stop-error") {
            anyhow::bail!("T152 start failed before returning a handle");
        }
        if spec.prompt.contains("loop-change") {
            fs::write(spec.cwd.join("t152-change.txt"), b"changed\n")?;
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
        Ok(Box::new(FakeHandle {
            events,
            session,
            prompt: spec.prompt,
        }))
    }
}

#[derive(Debug)]
struct FakeHandle {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    prompt: String,
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
        let answer = if self.prompt.contains("judge-pass") {
            "## Answer\nThe work passed.\n\n## Evidence\ntracked.txt\n\n## Open\nNone.\n\noutcome: pass\n"
        } else {
            "## Answer\nDone.\n\n## Evidence\nrun.json\n\n## Open\nNone.\n"
        };
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: answer.to_owned(),
            cost_usd: Some(0.25),
            tokens: Tokens {
                input: 10,
                output: 5,
                cached: 2,
            },
            turns: 1,
            took: Duration::from_millis(1),
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

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
