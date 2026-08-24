//! T-111 AC-1: the App Server and `exec` roads use the same protocol sandbox words.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use loadout_lib::engine::drivers::codex::{CodexDriver, build_exec_argv};
use loadout_lib::engine::drivers::{AgentDriver, AgentHandle, Policy, RunSpec, ValidatedImages};
use loadout_lib::engine::supervisor::GroupProof;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

const LIMIT: Duration = Duration::from_secs(10);
const CHANNEL: usize = 32;

const APP_SERVER_FAKE: &str = r#"#!/bin/sh
here="$(dirname "$0")"
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$here/stdin.jsonl"
  id="$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')"
  case "$line" in
    *'"method":"initialize"'*)
      printf '{"id":%s,"result":{}}\n' "${id:-1}"
      ;;
    *'"method":"config/read"'*)
      printf '{"id":%s,"result":{"config":{"mcp_servers":{}},"origins":{}}}\n' "${id:-2}"
      ;;
    *'"method":"thread/start"'*)
      printf '{"id":%s,"result":{"thread":{"id":"sandbox-thread","ephemeral":true,"path":null}}}\n' "${id:-3}"
      ;;
    *'"method":"turn/start"'*)
      printf '{"id":%s,"result":{"turn":{"id":"sandbox-turn","status":"inProgress"}}}\n' "${id:-4}"
      printf '%s\n' '{"method":"turn/completed","params":{"threadId":"sandbox-thread","turn":{"id":"sandbox-turn","status":"completed"}}}'
      ;;
    *'interrupt'*)
      printf '{"id":%s,"result":{}}\n' "${id:-5}"
      ;;
  esac
done
"#;

fn executable(directory: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let path = directory.join("codex");
    fs::write(&path, APP_SERVER_FAKE)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn spec(cwd: &Path, policy: Policy) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: "Compare both sandbox roads.".to_owned(),
        model: None,
        system_append: None,
        reaches_the_web: false,
        policy,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

fn exec_sandbox(spec: &RunSpec) -> Result<String, Box<dyn Error>> {
    let argv = build_exec_argv(spec);
    let flag = argv
        .iter()
        .position(|argument| argument == "-s")
        .ok_or("the exec road omitted its sandbox flag")?;
    argv.get(flag + 1)
        .cloned()
        .ok_or_else(|| "the exec sandbox flag has no value".into())
}

async fn app_server_sandbox(
    fixture: &Path,
    workspace: &Path,
    policy: Policy,
) -> Result<String, Box<dyn Error>> {
    let binary = executable(fixture)?;
    let driver = CodexDriver::with_binary(binary);
    let (events, _inbox) = mpsc::channel(CHANNEL);
    let mut handle: Box<dyn AgentHandle> = timeout(
        LIMIT,
        driver.start_conversation(spec(workspace, policy), ValidatedImages::default(), events),
    )
    .await??;

    let proof = timeout(LIMIT, handle.cancel()).await?;
    assert!(
        matches!(proof, GroupProof::Dead { .. }),
        "the sandbox fixture remained alive after the measured request: {proof:?}"
    );

    let calls = fs::read_to_string(fixture.join("stdin.jsonl"))?
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<Result<Vec<_>, _>>()?;
    calls
        .iter()
        .find(|call| call.get("method").and_then(Value::as_str) == Some("thread/start"))
        .and_then(|call| call.pointer("/params/sandbox"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "the App Server road sent no sandbox in thread/start".into())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn all_three_policies_reach_both_codex_roads_with_the_protocol_spelling()
-> Result<(), Box<dyn Error>> {
    for (policy, expected) in [
        (Policy::ReadOnly, "read-only"),
        (Policy::EditInFolder, "workspace-write"),
        (Policy::Unrestricted, "danger-full-access"),
    ] {
        let fixture = tempfile::tempdir()?;
        let workspace = tempfile::tempdir()?;
        let run = spec(workspace.path(), policy);
        let from_exec = exec_sandbox(&run)?;
        let from_app_server = app_server_sandbox(fixture.path(), workspace.path(), policy).await?;

        assert_eq!(
            (from_exec.as_str(), from_app_server.as_str()),
            (expected, expected),
            "the two Codex roads disagree for {policy:?}; exec={from_exec:?}, App Server={from_app_server:?}"
        );
    }
    Ok(())
}
