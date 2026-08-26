//! T-127 AC-2: two real Claude copies overlap and reflection receives a third private state.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use loadout_lib::commands::run::run_workflow_with_reflection;
use loadout_lib::commands::{Drivers, Outcome, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::{AgentDriver, DriverConfiguration};
use loadout_lib::engine::step::StepState;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use serde_json::Value;
use tauri::ipc::Channel;
use tempfile::TempDir;

const PATIENCE: Duration = Duration::from_secs(30);
const HOME_SENTINEL: &[u8] = b"shared Claude state must remain untouched\n";
const HOSTILE_SENTINEL: &[u8] = b"hostile state must lose to the run-owned value\n";

const AGENT: &str = r#"---
schema: 1
id: 01990000-0000-7000-8000-000000000127
name: Parallel Claude witness
summary: Exercises two real process copies
color: slate
runsWith: claude-code
model: haiku
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 20
writeResultsTo: ""
tools: everything
skills: []
connections: []
---
Return the fixture result.
"#;

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_t127_parallel_state",
  "name": "Parallel private state",
  "steps": [{
    "kind": "agent",
    "id": "s_parallel",
    "name": "Parallel",
    "agent": "01990000-0000-7000-8000-000000000127",
    "overrides": {},
    "copies": 2,
    "instructions": "Return a useful handoff for copy {{copy}} of {{copies}}.",
    "folder": { "use": "fresh-copy" },
    "at": { "x": 0, "y": 0 }
  }],
  "links": []
}"#;

const FAKE_CLAUDE: &str = r###"#!/bin/sh
here=${0%/*}
if [ "${1-}" = "--version" ]; then
  printf 'config=%s\n' "${CLAUDE_CONFIG_DIR-unset}" > "$here/audit/probe-$$.env"
  printf '%s\n' '2.1.241 (Claude Code)'
  exit 0
fi
state=${CLAUDE_CONFIG_DIR-}
existed=no
if [ -d "$state" ]; then
  existed=yes
fi
printf 'config=%s\nexisted=%s\nhome=%s\n' "${state:-unset}" "$existed" "${HOME-unset}" > "$here/audit/spawn-$$.env"
if [ "$existed" != "yes" ]; then
  exit 31
fi
key=${state##*/}
case "$key" in
  s_parallel|s_parallel~2|_reflection) ;;
  *) exit 32 ;;
esac
claude_dir=${state%/*}
: > "$state/start"
printf 'config=%s\nhome=%s\n' "$state" "${HOME-unset}" > "$state/environment"
printf '%s\n' "marker for $key" > "$state/marker"
if [ "$key" != "_reflection" ]; then
  attempts=0
  while [ ! -f "$claude_dir/s_parallel/start" ] || [ ! -f "$claude_dir/s_parallel~2/start" ]; do
    attempts=$((attempts + 1))
    if [ "$attempts" -ge 400 ]; then
      exit 33
    fi
    sleep 0.01
  done
  sleep 0.08
fi
: > "$state/end"
IFS= read -r first_turn
printf '%s\n' '{"type":"system","subtype":"init","session_id":"01990000-0000-7000-8000-000000000127","model":"haiku","tools":[]}'
if [ "$key" = "_reflection" ]; then
  printf '%s\n' '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"rule: T127 keep process state private\nbecause: this run used three distinct folders"}]}}'
  printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"duration_ms":2,"total_cost_usd":0.001,"result":"rule: T127 keep process state private\nbecause: this run used three distinct folders"}'
else
  printf '%s\n' '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"## Answer\nThe copy finished.\n\n## Evidence\nnotes.txt:1\n\n## Open\nNone.\n"}]}}'
  printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"duration_ms":90,"total_cost_usd":0.001,"result":"## Answer\nThe copy finished.\n\n## Evidence\nnotes.txt:1\n\n## Open\nNone.\n"}'
fi
"###;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn copies_overlap_and_reflection_uses_a_third_private_spawn() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    the_fixture_can_run(&bench.workflow, &bench.agent)?;
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: real_claude_factory(bench.binary.clone(), bench.fake_home.path(), &bench.hostile),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: bench.workflow.clone(),
        how_many_at_once: 2,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    // T-126's public entry is the explicit-reflection sibling of run_workflow_inner.
    let report = tokio::time::timeout(
        PATIENCE,
        run_workflow_with_reflection(&deps, &request, sink, None, true),
    )
    .await
    .map_err(|_| "the two-copy workflow did not finish before its explicit deadline")??;
    tokio::time::timeout(PATIENCE, pump).await??;

    assert_eq!(report.outcome, Outcome::Done);
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded, StepState::Succeeded],
        "both physical copies must finish before reflection"
    );
    let expected = expected_states(&report.dir);
    assert_eq!(state_dirs(&report.dir)?, expected);
    let audited = audited_spawns(&bench.audit)?;
    assert_eq!(
        audited.len(),
        3,
        "the run started a retry, omitted a copy, or omitted reflection: {audited:?}"
    );
    assert_eq!(
        audited
            .iter()
            .map(|entry| entry.config.clone())
            .collect::<BTreeSet<_>>(),
        expected,
        "a child reported a shared, missing, or unexpected CLAUDE_CONFIG_DIR"
    );
    assert!(audited.iter().all(|entry| entry.existed));
    for state in &expected {
        assert_eq!(
            fs::read_to_string(state.join("environment"))?
                .lines()
                .next(),
            Some(format!("config={}", state.display()).as_str())
        );
        assert!(state.join("marker").is_file());
    }

    let first = interval(&report.dir.join("claude/s_parallel"))?;
    let second = interval(&report.dir.join("claude/s_parallel~2"))?;
    let shared_from = first.0.max(second.0);
    let shared_to = first.1.min(second.1);
    assert!(
        shared_to > shared_from,
        "the complete copy windows did not overlap: {first:?}, {second:?}"
    );
    let reflection = interval(&report.dir.join("claude/_reflection"))?;
    assert!(
        reflection.0 >= first.1.max(second.1),
        "reflection was counted as parallel copy work: {reflection:?} after {first:?}, {second:?}"
    );

    let run: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    assert_eq!(run.pointer("/reflection/ran"), Some(&Value::Bool(true)));
    assert_eq!(fs::read(&bench.home_state)?, HOME_SENTINEL);
    assert_eq!(fs::read(bench.hostile.join("sentinel"))?, HOSTILE_SENTINEL);
    assert_eq!(names_in(&bench.hostile)?, vec!["sentinel"]);

    later_round_keeps_copy_state_keys_stable().await?;
    Ok(())
}

async fn later_round_keeps_copy_state_keys_stable() -> Result<(), Box<dyn Error>> {
    let bench = Bench::looped()?;
    the_fixture_can_run(&bench.workflow, &bench.agent)?;
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: real_claude_factory(bench.binary.clone(), bench.fake_home.path(), &bench.hostile),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: bench.workflow.clone(),
        how_many_at_once: 2,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let report = tokio::time::timeout(
        PATIENCE,
        run_workflow_with_reflection(&deps, &request, sink, None, true),
    )
    .await
    .map_err(|_| "the looped two-copy workflow did not reach its later round in time")??;
    tokio::time::timeout(PATIENCE, pump).await??;

    assert_eq!(report.outcome, Outcome::Done);
    let run: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    let node_keys = run
        .get("steps")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|step| step.get("node_key").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    assert!(
        node_keys.contains("s_parallel#1") && node_keys.contains("s_parallel~2#1"),
        "the fixture never reached both copies in a later round: {node_keys:?}"
    );

    let expected = expected_states(&report.dir);
    assert_eq!(state_dirs(&report.dir)?, expected);
    let audited = audited_spawns(&bench.audit)?;
    assert_eq!(
        audited.len(),
        5,
        "two rounds of two copies plus reflection must start five Claude processes: {audited:?}"
    );
    assert_eq!(
        audited
            .iter()
            .map(|entry| entry.config.clone())
            .collect::<BTreeSet<_>>(),
        expected,
        "later-round #turn suffixes leaked into state paths, or copy suffixes were collapsed"
    );
    assert!(audited.iter().all(|entry| entry.existed));
    assert_eq!(fs::read(&bench.home_state)?, HOME_SENTINEL);
    assert_eq!(fs::read(bench.hostile.join("sentinel"))?, HOSTILE_SENTINEL);
    assert_eq!(names_in(&bench.hostile)?, vec!["sentinel"]);
    Ok(())
}

#[derive(Debug)]
struct SpawnAudit {
    config: PathBuf,
    existed: bool,
}

fn audited_spawns(dir: &Path) -> Result<Vec<SpawnAudit>, Box<dyn Error>> {
    let mut paths = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("spawn-"))
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let text = fs::read_to_string(path)?;
            let config = value_of(&text, "config=").ok_or("spawn audit has no config")?;
            let existed = value_of(&text, "existed=") == Some("yes");
            Ok(SpawnAudit {
                config: PathBuf::from(config),
                existed,
            })
        })
        .collect()
}

fn value_of<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.lines().find_map(|line| line.strip_prefix(prefix))
}

fn interval(state: &Path) -> Result<(SystemTime, SystemTime), Box<dyn Error>> {
    Ok((
        fs::metadata(state.join("start"))?.modified()?,
        fs::metadata(state.join("end"))?.modified()?,
    ))
}

fn expected_states(run: &Path) -> BTreeSet<PathBuf> {
    ["s_parallel", "s_parallel~2", "_reflection"]
        .map(|key| run.join("claude").join(key))
        .into_iter()
        .collect()
}

fn state_dirs(run: &Path) -> Result<BTreeSet<PathBuf>, Box<dyn Error>> {
    Ok(fs::read_dir(run.join("claude"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect())
}

fn real_claude_factory(binary: PathBuf, fake_home: &Path, hostile: &Path) -> Drivers {
    let configured = ClaudeDriver::with_binary(binary).with_configuration(DriverConfiguration {
        arguments: Vec::new(),
        environment: vec![
            (
                "CLAUDE_CONFIG_DIR".to_owned(),
                hostile.as_os_str().to_os_string(),
            ),
            ("HOME".to_owned(), fake_home.as_os_str().to_os_string()),
        ],
        servers: Vec::new(),
    });
    let driver: Arc<dyn AgentDriver> = Arc::new(configured);
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
        "the fixture was refused before reaching the process behavior: {problems:?}"
    );
    read_agent_file(agent)?;
    Ok(())
}

fn names_in(dir: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let mut names = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<_>>();
    names.sort();
    Ok(names)
}

struct Bench {
    home: TempDir,
    project: TempDir,
    fake_home: TempDir,
    workflow: PathBuf,
    agent: PathBuf,
    binary: PathBuf,
    audit: PathBuf,
    hostile: PathBuf,
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
        fs::write(
            project.path().join("notes.txt"),
            "the person's project file\n",
        )?;
        let audit = home.path().join("audit");
        fs::create_dir_all(&audit)?;
        let hostile = home.path().join("hostile-state");
        fs::create_dir_all(&hostile)?;
        fs::write(hostile.join("sentinel"), HOSTILE_SENTINEL)?;
        let home_state = fake_home.path().join(".claude.json");
        fs::write(&home_state, HOME_SENTINEL)?;
        let agent = home.path().join("agents/t127.md");
        fs::write(&agent, AGENT)?;
        let workflow = home.path().join("workflows/t127.json");
        fs::write(&workflow, WORKFLOW)?;
        let binary = executable(home.path())?;
        Ok(Self {
            home,
            project,
            fake_home,
            workflow,
            agent,
            binary,
            audit,
            hostile,
            home_state,
        })
    }

    fn looped() -> Result<Self, Box<dyn Error>> {
        let bench = Self::new()?;
        let counter = bench.home.path().join("loop-rounds");
        let checker = bench.home.path().join("loop-check");
        fs::write(
            &checker,
            r#"#!/bin/sh
counter=$1
printf '%s\n' round >> "$counter"
runs=$(wc -l < "$counter" | tr -d ' ')
if [ "$runs" -ge 2 ]; then
  printf '%s\n' 'test result: ok. 1 passed; 0 failed'
  exit 0
fi
printf '%s\n' 'test result: FAILED. 0 passed; 1 failed'
exit 1
"#,
        )?;
        fs::set_permissions(&checker, fs::Permissions::from_mode(0o755))?;
        let workflow = format!(
            r#"{{
  "format": 1,
  "id": "wf_t127_parallel_state_later_round",
  "name": "Parallel private state across rounds",
  "steps": [
    {{
      "kind": "agent",
      "id": "s_parallel",
      "name": "Parallel",
      "agent": "01990000-0000-7000-8000-000000000127",
      "overrides": {{}},
      "copies": 2,
      "instructions": "Return a useful handoff for copy {{{{copy}}}} of {{{{copies}}}}.",
      "folder": {{ "use": "fresh-copy" }},
      "at": {{ "x": 0, "y": 0 }}
    }},
    {{
      "kind": "check",
      "id": "s_check",
      "name": "Check both copies",
      "command": "{} {}",
      "proof": "(\\d+) passed",
      "folder": {{ "use": "project" }},
      "at": {{ "x": 0, "y": 180 }}
    }}
  ],
  "links": [
    {{ "from": "s_parallel", "to": "s_check" }},
    {{ "from": "s_check", "to": "s_parallel", "max_turns": 2 }}
  ]
}}"#,
            checker.display(),
            counter.display()
        );
        fs::write(&bench.workflow, workflow)?;
        Ok(bench)
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout/loadout.db")
    }
}
