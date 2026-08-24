//! T-111 AC-3: private MCP servers are disabled and approved Connections stay enabled.

use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Stdio};
use std::sync::Arc;
use std::time::Duration;

use loadout_lib::connections::runtime;
use loadout_lib::connections::{Connection, Transport};
use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::{AgentDriver, AgentHandle, Policy, RunSpec, ValidatedImages};
use loadout_lib::engine::supervisor::GroupProof;
use loadout_lib::evidence::{EvidenceTarget, SafeInputManifest};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

const LIMIT: Duration = Duration::from_secs(10);
const CHANNEL: usize = 32;
const CALLS: &str = "stdin.jsonl";
const PRIVATE_PLAIN: &str = "notion-private-t111";
const PRIVATE_TRICKY: &str = r#"team."private-t111"#;
const PRIVATE_CONTROL: &str = "control-\u{0001}-t111";
const PRIVATE_CONTROL_ESCAPED: &str = r"control-\u0001-t111";
const APPROVED: &str = r#"loadout."approved-t111"#;
const APPROVED_COMMAND: &str = "approved-command-t111";
const ENVIRONMENT_NAME: &str = "T111_CONNECTION_ENV";
const ENVIRONMENT_SECRET: &str = "PRIVATE_CONNECTION_VALUE_T111";

const PLAIN_OVERLAY: &str = "mcp_servers.notion-private-t111.enabled";
const TRICKY_OVERLAY: &str = r#"mcp_servers."team.\"private-t111".enabled"#;
const CONTROL_OVERLAY: &str = r#"mcp_servers."control-\u0001-t111".enabled"#;
const APPROVED_OVERLAY: &str = r#"mcp_servers."loadout.\"approved-t111".enabled"#;
const APPROVED_COMMAND_OVERRIDE: &str =
    r#"mcp_servers."loadout.\"approved-t111".command="approved-command-t111""#;
const APPROVED_ARGS_OVERRIDE: &str = r#"mcp_servers."loadout.\"approved-t111".args=["--stdio"]"#;

const CONFIG_CURATED: &str = r#"{"id":%s,"result":{"config":{"mcp_servers":{"notion-private-t111":{"command":"private-notion"},"team.\"private-t111":{"url":"https://private.invalid"},"control-\u0001-t111":{"url":"https://control.invalid"},"loadout.\"approved-t111":{"command":"private-shadow"}}},"origins":{}}}"#;
const CONFIG_EMPTY: &str = r#"{"id":%s,"result":{"config":{"mcp_servers":{}},"origins":{}}}"#;
const CONFIG_NO_MCP_KEY: &str = r#"{"id":%s,"result":{"config":{},"origins":{}}}"#;
const CONFIG_ERROR: &str =
    r#"{"id":%s,"error":{"code":-32111,"message":"private configuration unavailable"}}"#;
const CONFIG_MISSING: &str = r#"{"id":%s,"result":{"origins":{}}}"#;
const CONFIG_WRONG_MCP_TYPE: &str =
    r#"{"id":%s,"result":{"config":{"mcp_servers":[]},"origins":{}}}"#;

const APP_SERVER_FAKE: &str = r#"#!/bin/sh
here="$(dirname "$0")"
printf '%s\n' "$@" > "$here/argv.log"
printf '%s\n' "$$" > "$here/pid.log"
printf '%s\n' 'safe fixture complaint' >&2
if [ "${T111_CONNECTION_ENV+x}" = "x" ]; then
  printf '%s\n' 'present' > "$here/environment.log"
else
  printf '%s\n' 'missing' > "$here/environment.log"
fi
config_head=T111_CONFIG_RESPONSE_HEAD
config_tail=T111_CONFIG_RESPONSE_TAIL
turn=0
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$here/stdin.jsonl"
  id="$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')"
  case "$line" in
    *'"method":"initialize"'*)
      printf '{"id":%s,"result":{}}\n' "${id:-1}"
      ;;
    *'"method":"config/read"'*)
      printf '%s%s%s\n' "$config_head" "${id:-2}" "$config_tail"
      ;;
    *'"method":"thread/start"'*)
      printf '{"id":%s,"result":{"thread":{"id":"curated-thread","ephemeral":true,"path":null}}}\n' "${id:-3}"
      ;;
    *'"method":"turn/start"'*)
      turn=$((turn + 1))
      printf '{"id":%s,"result":{"turn":{"id":"curated-turn-%s","status":"inProgress"}}}\n' "${id:-4}" "$turn"
      printf '{"method":"turn/completed","params":{"threadId":"curated-thread","turn":{"id":"curated-turn-%s","status":"completed"}}}\n' "$turn"
      ;;
    *'interrupt'*)
      printf '{"id":%s,"result":{}}\n' "${id:-5}"
      ;;
  esac
done
"#;

fn shell_single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn executable(directory: &Path, config_response: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = directory.join("codex");
    let (head, tail) = config_response
        .split_once("%s")
        .ok_or("the App Server fixture response has no request-id slot")?;
    let body = APP_SERVER_FAKE
        .replace("T111_CONFIG_RESPONSE_HEAD", &shell_single_quoted(head))
        .replace("T111_CONFIG_RESPONSE_TAIL", &shell_single_quoted(tail));
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn spec(cwd: &Path) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: "Use only approved Connections.".to_owned(),
        model: None,
        system_append: None,
        policy: Policy::ReadOnly,
        reaches_the_web: false,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

fn calls(directory: &Path) -> Result<Vec<Value>, Box<dyn Error>> {
    Ok(fs::read_to_string(directory.join(CALLS))?
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<Result<Vec<_>, _>>()?)
}

fn method(call: &Value) -> Option<&str> {
    call.get("method").and_then(Value::as_str)
}

fn approved_connection() -> Connection {
    let mut connection = Connection::imported(
        "approved-t111".to_owned(),
        APPROVED.to_owned(),
        Transport::Stdio {
            command: APPROVED_COMMAND.to_owned(),
            args: vec!["--stdio".to_owned()],
            environment: vec![ENVIRONMENT_NAME.to_owned()],
        },
        PathBuf::from("approved-connection-t111.json"),
        "source-hash-t111".to_owned(),
    );
    connection.enabled = true;
    connection
}

fn evidence_target(workspace: &Path) -> EvidenceTarget {
    EvidenceTarget::lead(
        workspace,
        Uuid::now_v7(),
        SafeInputManifest {
            prompt_bytes: 30,
            context: Vec::new(),
            images: Vec::new(),
        },
    )
}

fn private_tree_contains(root: &Path, needle: &[u8]) -> Result<bool, Box<dyn Error>> {
    if !root.exists() {
        return Ok(false);
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if private_tree_contains(&entry.path(), needle)? {
                return Ok(true);
            }
        } else if kind.is_file() {
            let bytes = fs::read(entry.path())?;
            if bytes.windows(needle.len()).any(|window| window == needle) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn process_is_alive(pid: u32) -> Result<bool, Box<dyn Error>> {
    Ok(StdCommand::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?
        .success())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn private_servers_are_false_and_the_approved_connection_is_true()
-> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let configuration = runtime::for_driver(
        workspace.path(),
        "codex",
        &[approved_connection()],
        |name| (name == ENVIRONMENT_NAME).then(|| OsString::from(ENVIRONMENT_SECRET)),
    )?;
    assert_eq!(configuration.servers, [APPROVED]);

    let target = evidence_target(workspace.path());
    let evidence_root = target.root().to_path_buf();
    let binary = executable(fixture.path(), CONFIG_CURATED)?;
    let base: Arc<dyn AgentDriver> =
        Arc::new(CodexDriver::with_binary(binary).with_configuration(configuration));
    let driver = base
        .with_evidence(target)
        .ok_or("Codex has no production evidence seam")?;
    let (events, _inbox) = mpsc::channel(CHANNEL);
    let mut handle: Box<dyn AgentHandle> = timeout(
        LIMIT,
        driver.start_conversation(spec(workspace.path()), ValidatedImages::default(), events),
    )
    .await??;
    let proof = timeout(LIMIT, handle.cancel()).await?;
    assert!(
        matches!(proof, GroupProof::Dead { .. }),
        "the curated App Server remained alive after Stop: {proof:?}"
    );

    let calls = calls(fixture.path())?;
    let initialized = calls
        .iter()
        .position(|call| method(call) == Some("initialized"))
        .ok_or("the Lead never completed initialization")?;
    let config_read = calls
        .iter()
        .position(|call| method(call) == Some("config/read"))
        .ok_or("the Lead never read effective configuration")?;
    let thread_start = calls
        .iter()
        .position(|call| method(call) == Some("thread/start"))
        .ok_or("the Lead never attempted thread/start")?;
    assert!(
        initialized < config_read && config_read < thread_start,
        "config/read must happen after initialized and before thread/start: {calls:?}"
    );
    assert_eq!(
        calls[config_read].get("params"),
        Some(&json!({ "includeLayers": false })),
        "config/read must not request private layer metadata"
    );

    let config = calls[thread_start]
        .pointer("/params/config")
        .and_then(Value::as_object)
        .ok_or("thread/start carried no per-thread config overlays")?;
    let mut expected = serde_json::Map::new();
    expected.insert(PLAIN_OVERLAY.to_owned(), Value::Bool(false));
    expected.insert(TRICKY_OVERLAY.to_owned(), Value::Bool(false));
    expected.insert(CONTROL_OVERLAY.to_owned(), Value::Bool(false));
    expected.insert(APPROVED_OVERLAY.to_owned(), Value::Bool(true));
    assert_eq!(
        config, &expected,
        "private servers and the approved Connection were not curated exactly once: {config:?}"
    );

    let argv = fs::read_to_string(fixture.path().join("argv.log"))?
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(
        argv,
        [
            "-c",
            APPROVED_COMMAND_OVERRIDE,
            "-c",
            APPROVED_ARGS_OVERRIDE,
            "app-server",
            "--listen",
            "stdio://",
        ],
        "the Connection and thread overlay did not share one TOML-key encoding"
    );
    assert_eq!(
        fs::read_to_string(fixture.path().join("environment.log"))?.trim(),
        "present",
        "the App Server process did not receive the approved Connection environment name"
    );

    let escaped_private_tricky = PRIVATE_TRICKY.replace('"', "\\\"");
    assert!(
        TRICKY_OVERLAY.contains(&escaped_private_tricky),
        "the escaped private id must be the spelling exercised by the tricky overlay"
    );
    assert!(
        CONTROL_OVERLAY.contains(PRIVATE_CONTROL_ESCAPED),
        "the escaped control id must be the spelling exercised by the control overlay"
    );
    for private in [
        PRIVATE_PLAIN,
        PRIVATE_TRICKY,
        escaped_private_tricky.as_str(),
        PRIVATE_CONTROL,
        PRIVATE_CONTROL_ESCAPED,
        ENVIRONMENT_SECRET,
    ] {
        assert!(
            !argv.iter().any(|argument| argument.contains(private)),
            "private configuration escaped into argv through {private:?}: {argv:?}"
        );
        assert!(
            !private_tree_contains(&evidence_root, private.as_bytes())?,
            "private configuration escaped into evidence through {private:?}"
        );
    }
    Ok(())
}

async fn invalid_config_is_refused(config_response: &str) -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let binary = executable(fixture.path(), config_response)?;
    let target = evidence_target(workspace.path());
    let base: Arc<dyn AgentDriver> = Arc::new(CodexDriver::with_binary(binary));
    let driver = base
        .with_evidence(target.clone())
        .ok_or("Codex has no production evidence seam")?;
    let (events, _inbox) = mpsc::channel(CHANNEL);
    let started = timeout(
        LIMIT,
        driver.start_conversation(spec(workspace.path()), ValidatedImages::default(), events),
    )
    .await?;
    if let Ok(mut handle) = started {
        let proof = timeout(LIMIT, handle.cancel()).await?;
        return Err(format!(
            "Codex opened a thread after unsafe config/read; cleanup returned {proof:?}"
        )
        .into());
    }

    let calls = calls(fixture.path())?;
    assert!(
        calls.iter().any(|call| method(call) == Some("config/read")),
        "the Lead did not inspect effective configuration: {calls:?}"
    );
    assert!(
        calls
            .iter()
            .all(|call| method(call) != Some("thread/start")),
        "thread/start ran after config/read had already failed: {calls:?}"
    );
    let pid = fs::read_to_string(fixture.path().join("pid.log"))?
        .trim()
        .parse::<u32>()?;
    assert!(
        !process_is_alive(pid)?,
        "failed config/read left App Server pid {pid} alive"
    );
    assert!(
        !target.is_healthy(),
        "failed config/read left its conversation evidence eligible for completion"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bad_effective_config_fails_closed_before_thread_start() -> Result<(), Box<dyn Error>> {
    for response in [CONFIG_ERROR, CONFIG_MISSING, CONFIG_WRONG_MCP_TYPE] {
        invalid_config_is_refused(response).await?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_private_servers_leave_app_server_argv_byte_exact() -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let binary = executable(fixture.path(), CONFIG_EMPTY)?;
    let driver = CodexDriver::with_binary(binary);
    let (events, _inbox) = mpsc::channel(CHANNEL);
    let mut handle: Box<dyn AgentHandle> = timeout(
        LIMIT,
        driver.start_conversation(spec(workspace.path()), ValidatedImages::default(), events),
    )
    .await??;
    let proof = timeout(LIMIT, handle.cancel()).await?;
    assert!(matches!(proof, GroupProof::Dead { .. }));

    assert_eq!(
        fs::read_to_string(fixture.path().join("argv.log"))?
            .lines()
            .collect::<Vec<_>>(),
        ["app-server", "--listen", "stdio://"],
        "an empty private-server set changed today's App Server argv"
    );
    let calls = calls(fixture.path())?;
    assert!(
        calls.iter().any(|call| method(call) == Some("config/read")),
        "the byte-exact control skipped the safety read entirely: {calls:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn absent_private_server_key_keeps_the_approved_connection_enabled()
-> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let configuration = runtime::for_driver(
        workspace.path(),
        "codex",
        &[approved_connection()],
        |name| (name == ENVIRONMENT_NAME).then(|| OsString::from(ENVIRONMENT_SECRET)),
    )?;
    let binary = executable(fixture.path(), CONFIG_NO_MCP_KEY)?;
    let driver = CodexDriver::with_binary(binary).with_configuration(configuration);
    let (events, _inbox) = mpsc::channel(CHANNEL);
    let mut handle: Box<dyn AgentHandle> = timeout(
        LIMIT,
        driver.start_conversation(spec(workspace.path()), ValidatedImages::default(), events),
    )
    .await??;
    let proof = timeout(LIMIT, handle.cancel()).await?;
    assert!(matches!(proof, GroupProof::Dead { .. }));

    let calls = calls(fixture.path())?;
    let config = calls
        .iter()
        .find(|call| method(call) == Some("thread/start"))
        .and_then(|call| call.pointer("/params/config"))
        .and_then(Value::as_object)
        .ok_or("config without an MCP key was refused before thread/start")?;
    assert_eq!(
        config,
        &serde_json::Map::from_iter([(APPROVED_OVERLAY.to_owned(), Value::Bool(true))]),
        "an absent private-server key must produce only the approved Connection overlay"
    );
    Ok(())
}

#[test]
fn connection_control_characters_are_escaped_on_both_codex_configuration_paths()
-> Result<(), Box<dyn Error>> {
    const NAME: &str = "control\nserver";
    const URL: &str = "https://tools.invalid/\u{0001}\t";
    const TOKEN_ENVIRONMENT: &str = "TOKEN\r\u{007f}NAME";
    const TOKEN_VALUE: &str = "resolved-control-value-t111";

    let mut connection = Connection::imported(
        "control-t111".to_owned(),
        NAME.to_owned(),
        Transport::Http {
            url: URL.to_owned(),
            token_environment: Some(TOKEN_ENVIRONMENT.to_owned()),
        },
        PathBuf::from("control-connection-t111.json"),
        "control-source-hash-t111".to_owned(),
    );
    connection.enabled = true;

    let generated = runtime::for_connections(&[connection.clone()]);
    assert_eq!(
        generated.codex,
        "[mcp_servers.\"control\\nserver\"]\nurl = \"https://tools.invalid/\\u0001\\t\"\nbearer_token_env_var = \"TOKEN\\r\\u007FNAME\"\n\n",
        "the generated Codex document carried raw TOML control characters"
    );

    let workspace = tempfile::tempdir()?;
    let configuration = runtime::for_driver(workspace.path(), "codex", &[connection], |name| {
        (name == TOKEN_ENVIRONMENT).then(|| OsString::from(TOKEN_VALUE))
    })?;
    assert_eq!(
        configuration.arguments,
        [
            "-c",
            "mcp_servers.\"control\\nserver\".url=\"https://tools.invalid/\\u0001\\t\"",
            "-c",
            "mcp_servers.\"control\\nserver\".bearer_token_env_var=\"TOKEN\\r\\u007FNAME\"",
        ],
        "the per-process Codex overrides dropped or corrupted a control-bearing pair"
    );
    assert_eq!(
        configuration.environment,
        [(TOKEN_ENVIRONMENT.to_owned(), OsString::from(TOKEN_VALUE))],
        "escaping the public env name must not drop its resolved value"
    );
    Ok(())
}
