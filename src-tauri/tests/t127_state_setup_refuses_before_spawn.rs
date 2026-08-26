//! T-127 AC-3: private-state setup failure is one visible refusal before any Claude spawn.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use loadout_lib::commands::run::{continue_run_inner, run_workflow_with_reflection};
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::{AgentDriver, AgentEvent, DriverConfiguration};
use loadout_lib::engine::line::{Curator, Line, Seen};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use serde_json::Value;
use tauri::ipc::{Channel, InvokeResponseBody};
use tempfile::TempDir;

const PATIENCE: Duration = Duration::from_secs(20);
const EVERY: Duration = Duration::from_millis(10);
const REFUSAL: &str =
    "Loadout could not create this agent's private state folder, so it did not start the step.";
const BLOCKER: &[u8] = b"a regular file deliberately occupies the Claude state parent\n";
const HOME_SENTINEL: &[u8] = b"the person's shared Claude state must not be used\n";

const AGENT: &str = r#"---
schema: 1
id: 01990000-0000-7000-8000-000000000127
name: Refusal witness
summary: Must never reach its executable
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
Return one short answer.
"#;

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_t127_setup_refusal",
  "name": "Private state refusal",
  "steps": [
    {
      "kind": "checkpoint",
      "id": "s_pause",
      "name": "Pause before the process",
      "question": "Continue to the process?",
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_blocked",
      "name": "Blocked Claude",
      "agent": "01990000-0000-7000-8000-000000000127",
      "overrides": {},
      "instructions": "Return the fixture answer.",
      "folder": { "use": "project" },
      "at": { "x": 240, "y": 0 }
    }
  ],
  "links": [{ "from": "s_pause", "to": "s_blocked" }]
}"#;

const FAKE_CLAUDE: &str = r#"#!/bin/sh
here=${0%/*}
if [ "${1-}" = "--version" ]; then
  printf '%s\n' '2.1.241 (Claude Code)'
  exit 0
fi
printf 'config=%s\n' "${CLAUDE_CONFIG_DIR-unset}" >> "$here/spawned"
if [ -z "${CLAUDE_CONFIG_DIR-}" ]; then
  printf '%s\n' 'shared state was reached' > "$HOME/.claude.json"
fi
IFS= read -r first_turn
printf '%s\n' '{"type":"system","subtype":"init","session_id":"01990000-0000-7000-8000-000000000127","model":"haiku","tools":[]}'
printf '%s\n' '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"done"}]}}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"duration_ms":2,"total_cost_usd":0.001,"result":"done"}'
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn setup_failure_is_visible_persisted_and_never_spawns() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    the_fixture_can_run(&bench.workflow, &bench.agent)?;
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: real_claude_factory(bench.binary.clone(), bench.fake_home.path()),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: bench.workflow.clone(),
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let delivered = Delivered::default();
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, delivered.channel());
    let plant_blocker = async {
        let run = wait_until_paused(bench.project.path()).await?;
        let claude = run.join("claude");
        fs::write(&claude, BLOCKER)?;
        assert!(
            claude.is_file(),
            "the refusal fixture did not plant a regular file"
        );
        continue_run_inner(&deps, Some("Continue".to_owned())).await?;
        Ok::<PathBuf, Box<dyn Error>>(run)
    };

    let (ran, blocked, pumped) = tokio::time::timeout(PATIENCE, async {
        tokio::join!(
            run_workflow_with_reflection(&deps, &request, sink, None, true),
            plant_blocker,
            pump
        )
    })
    .await
    .map_err(|_| "the checkpoint workflow did not settle before its explicit deadline")?;
    pumped?;
    let report = ran?;
    let planted_run = blocked?;
    assert_eq!(planted_run, report.dir);

    assert!(
        !bench.home.path().join("spawned").exists(),
        "Claude started despite the state setup refusal, or reflection retried without state"
    );
    assert_eq!(fs::read(&bench.home_state)?, HOME_SENTINEL);
    assert_eq!(fs::read(report.dir.join("claude"))?, BLOCKER);

    let problems = delivered
        .lines()?
        .into_iter()
        .filter(|line| line.get("kind").and_then(Value::as_str) == Some("problem"))
        .collect::<Vec<_>>();
    assert_eq!(
        problems.len(),
        1,
        "the visible stream must carry one and only one Problem row: {problems:?}"
    );
    assert_eq!(
        problems[0].get("text").and_then(Value::as_str),
        Some(REFUSAL),
        "the Notice was prefixed, suffixed, or replaced before reaching the visible Line::Problem"
    );
    assert_eq!(
        problems[0],
        notice_curated_as_problem("Blocked Claude")?,
        "the public row must have the exact shape produced by curating the refusal Notice"
    );

    let run: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    let step = run
        .get("steps")
        .and_then(Value::as_array)
        .and_then(|steps| {
            steps
                .iter()
                .find(|step| step.get("node_key").and_then(Value::as_str) == Some("s_blocked"))
        })
        .ok_or("run.json omitted the blocked Claude step")?;
    assert_eq!(step.get("status").and_then(Value::as_str), Some("failed"));
    assert_eq!(step.get("error").and_then(Value::as_str), Some(REFUSAL));
    assert_eq!(run.pointer("/reflection/ran"), Some(&Value::Bool(false)));
    Ok(())
}

fn notice_curated_as_problem(agent: &str) -> Result<Value, Box<dyn Error>> {
    let event = AgentEvent::Notice {
        text: REFUSAL.to_owned(),
    };
    let mut curator = Curator::new();
    let mut lines = curator.observe(Seen {
        agent,
        at_ms: 0,
        event: &event,
        tool: None,
    });
    lines.extend(curator.flush());
    let [
        Line::Problem {
            agent: seen_agent,
            text,
            resets_at,
        },
    ] = lines.as_slice()
    else {
        return Err(
            format!("the refusal Notice did not curate into one Problem: {lines:?}").into(),
        );
    };
    assert_eq!(seen_agent, agent);
    assert_eq!(text, REFUSAL);
    assert_eq!(*resets_at, None);
    Ok(serde_json::to_value(&lines[0])?)
}

#[derive(Clone, Debug, Default)]
struct Delivered(Arc<Mutex<Vec<InvokeResponseBody>>>);

impl Delivered {
    fn channel(&self) -> Channel<Vec<Line>> {
        let seen = Arc::clone(&self.0);
        Channel::new(move |body| {
            if let Ok(mut rows) = seen.lock() {
                rows.push(body);
            }
            Ok(())
        })
    }

    fn lines(&self) -> Result<Vec<Value>, Box<dyn Error>> {
        let seen = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let mut lines = Vec::new();
        for body in seen.iter().cloned() {
            lines.extend(body.deserialize::<Vec<Value>>()?);
        }
        Ok(lines)
    }
}

async fn wait_until_paused(project: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(run) = only_run_dir(project)
            && run_file(&run)
                .and_then(|value| {
                    value
                        .get("status")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .as_deref()
                == Some("paused")
        {
            return Ok(run);
        }
        if Instant::now() >= deadline {
            return Err("the real workflow never paused after creating its run directory".into());
        }
        tokio::time::sleep(EVERY).await;
    }
}

fn only_run_dir(project: &Path) -> Option<PathBuf> {
    let mut dirs = fs::read_dir(project.join(".loadout/runs"))
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    dirs.sort();
    match dirs.as_slice() {
        [only] => Some(only.clone()),
        _ => None,
    }
}

fn run_file(run: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(run.join("run.json")).ok()?).ok()
}

fn real_claude_factory(binary: PathBuf, fake_home: &Path) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(
        ClaudeDriver::with_binary(binary).with_configuration(DriverConfiguration {
            arguments: Vec::new(),
            environment: vec![("HOME".to_owned(), fake_home.as_os_str().to_os_string())],
            servers: Vec::new(),
        }),
    );
    Arc::new(move |_vendor| Arc::clone(&driver))
}

fn executable(dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join("fake-claude");
    fs::write(&path, FAKE_CLAUDE)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn the_fixture_can_run(workflow: &Path, agent: &Path) -> Result<(), Box<dyn Error>> {
    let problems = check(&load(workflow)?)
        .into_iter()
        .filter(|note| note.level == Level::Problem)
        .map(|note| note.message)
        .collect::<Vec<_>>();
    assert!(
        problems.is_empty(),
        "the fixture was refused before the state setup behavior: {problems:?}"
    );
    read_agent_file(agent)?;
    Ok(())
}

struct Bench {
    home: TempDir,
    project: TempDir,
    fake_home: TempDir,
    workflow: PathBuf,
    agent: PathBuf,
    binary: PathBuf,
    home_state: PathBuf,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        let fake_home = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        let agent = home.path().join("agents/t127.md");
        fs::write(&agent, AGENT)?;
        let workflow = home.path().join("workflows/t127.json");
        fs::write(&workflow, WORKFLOW)?;
        let binary = executable(home.path())?;
        let home_state = fake_home.path().join(".claude.json");
        fs::write(&home_state, HOME_SENTINEL)?;
        Ok(Self {
            home,
            project,
            fake_home,
            workflow,
            agent,
            binary,
            home_state,
        })
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout/loadout.db")
    }
}
