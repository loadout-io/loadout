//! T-111 AC-2: a JSON-RPC refusal reaches the same IPC result the window receives.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use loadout_lib::commands::Drivers;
use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::engine::drivers::AgentDriver;
use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::ipc::{AppState, line_channel, say_to_orchestrator_from_window};
use loadout_lib::library::agents::{Agent, Vendor};
use loadout_lib::store::Store;
use tempfile::TempDir;
use uuid::Uuid;

const TERMINAL: &str = "t-111-refusal-window";
const LINES: usize = 32;

const REFUSING_APP_SERVER: &str = r#"#!/bin/sh
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
      printf '{"id":%s,"error":{"code":T111_VENDOR_CODE,"message":"T111_VENDOR_MESSAGE"}}\n' "${id:-3}"
      ;;
  esac
done
"#;

const ACCEPTING_APP_SERVER: &str = r#"#!/bin/sh
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
      printf '{"id":%s,"result":{"thread":{"id":"accepted-thread","ephemeral":true,"path":null}}}\n' "${id:-3}"
      ;;
    *'"method":"turn/start"'*)
      printf '{"id":%s,"result":{"turn":{"id":"accepted-turn","status":"inProgress"}}}\n' "${id:-4}"
      printf '%s\n' '{"method":"turn/completed","params":{"threadId":"accepted-thread","turn":{"id":"accepted-turn","status":"completed"}}}'
      ;;
    *'interrupt'*)
      printf '{"id":%s,"result":{}}\n' "${id:-5}"
      ;;
  esac
done
"#;

fn executable(directory: &Path, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = directory.join("codex");
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

struct Bench {
    library: TempDir,
    workspace: TempDir,
    lead: String,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let library = tempfile::tempdir()?;
        let workspace = tempfile::tempdir()?;
        fs::create_dir_all(workspace.path().join(".loadout"))?;

        let mut lead = Agent::example();
        lead.id = Uuid::now_v7();
        "Codex Lead".clone_into(&mut lead.name);
        lead.runs_with = Vendor::Codex;
        save_agent_inner(library.path(), &lead)?;
        let lead = lead.id.to_string();

        Ok(Self {
            library,
            workspace,
            lead,
        })
    }

    fn state(&self, binary: PathBuf) -> Result<AppState, Box<dyn Error>> {
        let driver: Arc<dyn AgentDriver> = Arc::new(CodexDriver::with_binary(binary));
        let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));
        let store = Store::open(&self.workspace.path().join(".loadout/loadout.db"))?;
        Ok(AppState::new(
            self.library.path().to_path_buf(),
            self.workspace.path().to_path_buf(),
            store,
            drivers,
        ))
    }

    async fn say(&self, state: &AppState) -> Result<(), String> {
        let folder = self.workspace.path().to_string_lossy().into_owned();
        let (sink, _source) = line_channel(LINES);
        state.watching_the_lead(TERMINAL, Some(&folder), sink)?;
        say_to_orchestrator_from_window(
            state,
            TERMINAL,
            Some(&folder),
            Some(&self.lead),
            "Start this conversation.",
            Vec::new(),
        )
        .await
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_window_receives_the_vendors_runtime_code_and_message() -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let bench = Bench::new()?;
    let code = -32_000_i64 - i64::from(Uuid::now_v7().as_bytes()[15]);
    let message = format!("sandbox spelling rejected by vendor {}", Uuid::now_v7());
    let body = REFUSING_APP_SERVER
        .replace("T111_VENDOR_CODE", &code.to_string())
        .replace("T111_VENDOR_MESSAGE", &message);
    let state = bench.state(executable(fixture.path(), &body)?)?;

    let result = bench.say(&state).await;
    state.close_the_lead(TERMINAL).await;
    let Err(said) = result else {
        return Err("the window accepted a thread/start refusal".into());
    };

    assert!(
        said.contains(&code.to_string()),
        "the refusal shown to the window hid vendor code {code}: {said}"
    );
    assert!(
        said.contains(&message),
        "the refusal shown to the window hid the vendor's runtime message {message:?}: {said}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_response_without_error_still_opens_the_lead_conversation() -> Result<(), Box<dyn Error>>
{
    let fixture = tempfile::tempdir()?;
    let bench = Bench::new()?;
    let state = bench.state(executable(fixture.path(), ACCEPTING_APP_SERVER)?)?;

    let result = bench.say(&state).await;
    state.close_the_lead(TERMINAL).await;
    if let Err(said) = result {
        return Err(format!("the control response was turned down: {said}").into());
    }
    Ok(())
}
