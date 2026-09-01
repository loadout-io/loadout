//! T-160: Claude subagent `inherit` is a sentinel, not a top-level CLI model name.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::{AgentDriver, DriverConfiguration, Policy, RunSpec};
use tempfile::TempDir;
use tokio::sync::mpsc;
use uuid::Uuid;

const PATIENCE: Duration = Duration::from_secs(10);

const FAKE_CLAUDE: &str = r#"#!/bin/sh
if [ "${1-}" = "--version" ]; then
  printf '%s\n' '2.1.247 (Claude Code)'
  exit 0
fi
: > "$LOADOUT_T160_CAPTURE"
for argument in "$@"; do
  printf '%s\0' "$argument" >> "$LOADOUT_T160_CAPTURE"
done
IFS= read -r first_turn
printf '%s\n' '{"type":"system","subtype":"init","session_id":"01990000-0000-7000-8000-000000000160","model":"sonnet","tools":[]}'
printf '%s\n' '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"done"}]}}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"duration_ms":2,"total_cost_usd":0.001,"result":"done"}'
"#;

#[tokio::test]
async fn inherit_omits_model_while_a_real_selection_keeps_it() -> Result<(), Box<dyn Error>> {
    let inherited = argv_for("inherit").await?;
    assert_eq!(
        model_pairs(&inherited),
        Vec::<(OsString, OsString)>::new(),
        "the Claude subagent sentinel escaped as a top-level --model argument: {inherited:?}"
    );
    assert!(
        !inherited.iter().any(|argument| argument == "inherit"),
        "the sentinel still reached the child process: {inherited:?}"
    );

    let explicit = argv_for("opus").await?;
    assert_eq!(
        model_pairs(&explicit),
        vec![(OsString::from("--model"), OsString::from("opus"))],
        "a real Claude model selection no longer reaches the child exactly once: {explicit:?}"
    );
    Ok(())
}

async fn argv_for(model: &str) -> Result<Vec<OsString>, Box<dyn Error>> {
    let bench = TempDir::new()?;
    let project = TempDir::new()?;
    let binary = executable(bench.path())?;
    let capture = bench.path().join("argv.bin");
    let driver = ClaudeDriver::with_binary(binary).with_configuration(DriverConfiguration {
        arguments: Vec::new(),
        environment: vec![(
            "LOADOUT_T160_CAPTURE".to_owned(),
            capture.as_os_str().to_os_string(),
        )],
        servers: Vec::new(),
    });
    let (events, _inbox) = mpsc::channel(16);
    let mut handle = driver
        .start(
            RunSpec {
                run_id: Uuid::now_v7(),
                cwd: project.path().to_path_buf(),
                prompt: "Return one short answer.".to_owned(),
                model: Some(model.to_owned()),
                system_append: None,
                policy: Policy::ReadOnly,
                reaches_the_web: false,
                tools: None,
                extra_dirs: Vec::new(),
                resume: None,
            },
            events,
        )
        .await?;
    let outcome = tokio::time::timeout(PATIENCE, handle.wait()).await??;
    assert!(outcome.ok, "the executable fixture did not finish its turn");
    let _ = tokio::time::timeout(PATIENCE, handle.close()).await??;
    split_arguments(&fs::read(capture)?)
}

fn executable(dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join("fake-claude");
    fs::write(&path, FAKE_CLAUDE)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn split_arguments(bytes: &[u8]) -> Result<Vec<OsString>, Box<dyn Error>> {
    if bytes.last().is_some_and(|byte| *byte != 0) {
        return Err("the argv fixture did not terminate its final argument".into());
    }
    Ok(bytes
        .split(|byte| *byte == 0)
        .filter(|argument| !argument.is_empty())
        .map(|argument| OsString::from_vec(argument.to_vec()))
        .collect())
}

fn model_pairs(arguments: &[OsString]) -> Vec<(OsString, OsString)> {
    arguments
        .windows(2)
        .filter(|pair| pair[0] == OsStr::new("--model"))
        .map(|pair| (pair[0].clone(), pair[1].clone()))
        .collect()
}
