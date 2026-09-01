//! T-150: aplikacja uruchomiona przez `LaunchServices` nie dziedziczy `Homebrew` w `PATH`.

use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use loadout_lib::agent_drivers_with_search;
use loadout_lib::commands::run::run_workflow_with_reflection;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::supervisor::AgentCliSearch;
#[cfg(target_os = "macos")]
use loadout_lib::engine::supervisor::platform_agent_cli_dirs;
use loadout_lib::ipc::{QUEUE_CAP, line_channel};
use loadout_lib::library::agents::Vendor;
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tempfile::TempDir;

const PATIENCE: Duration = Duration::from_secs(20);
const GUI_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";
const CODEX_ID: &str = "01990000-0000-7000-8000-000000000150";
const CLAUDE_ID: &str = "01990000-0000-7000-8000-000000000151";

const CODEX_FAKE: &str = r#"#!/bin/sh
here="$(dirname "$0")"
cat > "$here/codex.stdin"
printf '%s\n' '{"type":"thread.started","thread_id":"thread-t150"}'
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"codex found"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":3,"cached_input_tokens":1,"output_tokens":2}}'
"#;

const CLAUDE_FAKE: &str = r#"#!/bin/sh
here="$(dirname "$0")"
IFS= read -r first_turn
printf '%s\n' "$first_turn" > "$here/claude.stdin"
printf '%s\n' '{"type":"system","subtype":"init","session_id":"01990000-0000-7000-8000-000000000151","model":"sonnet","tools":[]}'
printf '%s\n' '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"claude found"}]}}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"duration_ms":7,"total_cost_usd":0.001,"result":"claude found"}'
"#;

fn write_executable(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn agent(vendor: Vendor) -> String {
    let (id, name, runs_with, model) = match vendor {
        Vendor::Codex => (CODEX_ID, "Codex GUI witness", "codex", "gpt-5.6-sol"),
        Vendor::ClaudeCode => (CLAUDE_ID, "Claude GUI witness", "claude-code", "sonnet"),
    };
    format!(
        "---\nschema: 1\nid: {id}\nname: {name}\nsummary: Proves GUI CLI discovery\ncolor: moss\nrunsWith: {runs_with}\nmodel: {model}\nthinking: balanced\nfileAccess: look-only\ngiveUpAfterMinutes: 5\nwriteResultsTo: \"\"\ntools: everything\nskills: []\nconnections: []\n---\nReturn one short answer.\n"
    )
}

fn workflow(vendor: Vendor) -> Value {
    let (id, vendor_name) = match vendor {
        Vendor::Codex => (CODEX_ID, "codex"),
        Vendor::ClaudeCode => (CLAUDE_ID, "claude"),
    };
    json!({
        "format": 1,
        "id": format!("wf_t150_{vendor_name}"),
        "name": "GUI CLI discovery",
        "steps": [{
            "kind": "agent",
            "id": "s_1",
            "name": "Start the installed CLI",
            "agent": id,
            "overrides": {},
            "instructions": "prove the installed CLI started",
            "folder": { "use": "project" },
            "at": { "x": 0, "y": 0 }
        }],
        "links": []
    })
}

struct World {
    home: TempDir,
    project: TempDir,
}

impl World {
    fn new(vendor: Vendor) -> Result<Self, Box<dyn Error>> {
        let home = tempfile::tempdir()?;
        let project = tempfile::tempdir()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        let id = match vendor {
            Vendor::Codex => "codex",
            Vendor::ClaudeCode => "claude",
        };
        fs::write(home.path().join(format!("agents/{id}.md")), agent(vendor))?;
        fs::write(
            home.path().join("workflows/gui.json"),
            serde_json::to_vec_pretty(&workflow(vendor))?,
        )?;
        Ok(Self { home, project })
    }

    async fn run(&self, drivers: Drivers) -> Result<(Value, Vec<Line>), Box<dyn Error>> {
        let store = Store::open(&self.home.path().join("loadout.db"))?;
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store: &store,
            drivers,
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow: self.home.path().join("workflows/gui.json"),
            how_many_at_once: 1,
            task: Some("start the configured agent".to_owned()),
            part: None,
            handoffs_from: None,
        };
        let (sink, mut source) = line_channel(QUEUE_CAP);
        let report = tokio::time::timeout(
            PATIENCE,
            run_workflow_with_reflection(&deps, &request, sink, None, false),
        )
        .await
        .map_err(|_| "the product run did not finish")??;
        let mut lines = Vec::new();
        while let Some(line) = source.try_next() {
            lines.push(line);
        }
        let receipt = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
        Ok((receipt, lines))
    }
}

async fn installed_cli_runs(vendor: Vendor) -> Result<(), Box<dyn Error>> {
    let world = World::new(vendor)?;
    let bin = world.home.path().join("gui-bin");
    fs::create_dir_all(&bin)?;
    write_executable(&bin, "codex", CODEX_FAKE)?;
    write_executable(&bin, "claude", CLAUDE_FAKE)?;
    let search = AgentCliSearch::from_parts(Some(OsString::from(GUI_PATH)), vec![bin.clone()]);
    let (receipt, _) = world.run(agent_drivers_with_search(&search)).await?;

    assert_eq!(
        receipt["status"], "succeeded",
        "the GUI-only CLI did not run: {receipt:#}"
    );
    assert!(
        receipt["steps"][0]["pid"].as_u64().is_some()
            && receipt["steps"][0]["pgid"].as_i64().is_some()
            && receipt["steps"][0]["exit_code"] == 0,
        "the run did not leave process evidence: {receipt:#}"
    );
    let stdin = match vendor {
        Vendor::Codex => bin.join("codex.stdin"),
        Vendor::ClaudeCode => bin.join("claude.stdin"),
    };
    assert!(
        fs::read(&stdin).is_ok_and(|bytes| !bytes.is_empty()),
        "the absolute CLI started but did not receive its prompt on stdin"
    );
    Ok(())
}

async fn missing_cli_is_named(vendor: Vendor) -> Result<(), Box<dyn Error>> {
    let world = World::new(vendor)?;
    let empty_path = tempfile::tempdir()?;
    let empty_install_dir = tempfile::tempdir()?;
    let search = AgentCliSearch::from_parts(
        Some(empty_path.path().as_os_str().to_os_string()),
        vec![empty_install_dir.path().to_path_buf()],
    );
    let (receipt, lines) = world.run(agent_drivers_with_search(&search)).await?;
    let shown = lines
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()?
        .join("\n");
    let saved = receipt["steps"][0]["error"].as_str().unwrap_or("");
    let name = match vendor {
        Vendor::Codex => "Codex CLI",
        Vendor::ClaudeCode => "Claude Code CLI",
    };
    for public in [shown.as_str(), saved] {
        assert!(
            public.contains(name) && public.contains("install") && public.contains("restart"),
            "the refusal did not name {name} and the recovery action: {public}"
        );
        assert!(
            !public.contains("No such file") && !public.contains("os error 2"),
            "raw filesystem jargon reached the product"
        );
        assert!(
            !public.contains(&world.home.path().display().to_string()),
            "a private path reached the public refusal"
        );
    }
    assert!(receipt["steps"][0]["pid"].is_null());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn codex_installed_outside_the_gui_path_runs() -> Result<(), Box<dyn Error>> {
    installed_cli_runs(Vendor::Codex).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn claude_installed_outside_the_gui_path_runs() -> Result<(), Box<dyn Error>> {
    installed_cli_runs(Vendor::ClaudeCode).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_codex_is_a_human_refusal() -> Result<(), Box<dyn Error>> {
    missing_cli_is_named(Vendor::Codex).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_claude_is_a_human_refusal() -> Result<(), Box<dyn Error>> {
    missing_cli_is_named(Vendor::ClaudeCode).await
}

#[test]
fn path_candidate_wins_over_the_same_install_dir_name() -> Result<(), Box<dyn Error>> {
    let path_dir = tempfile::tempdir()?;
    let install_dir = tempfile::tempdir()?;
    let from_path = write_executable(path_dir.path(), "codex", CODEX_FAKE)?;
    write_executable(install_dir.path(), "codex", CODEX_FAKE)?;
    let search = AgentCliSearch::from_parts(
        Some(path_dir.path().as_os_str().to_os_string()),
        vec![install_dir.path().to_path_buf()],
    );

    assert_eq!(search.resolve("codex"), from_path);
    Ok(())
}

#[test]
fn non_executable_path_candidate_is_skipped() -> Result<(), Box<dyn Error>> {
    let path_dir = tempfile::tempdir()?;
    let install_dir = tempfile::tempdir()?;
    let blocked = path_dir.path().join("codex");
    fs::write(&blocked, CODEX_FAKE)?;
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o644))?;
    let executable = write_executable(install_dir.path(), "codex", CODEX_FAKE)?;
    let search = AgentCliSearch::from_parts(
        Some(path_dir.path().as_os_str().to_os_string()),
        vec![install_dir.path().to_path_buf()],
    );

    assert_eq!(search.resolve("codex"), executable);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn macos_search_includes_homebrew() {
    assert!(
        platform_agent_cli_dirs(None)
            .iter()
            .any(|directory| directory == Path::new("/opt/homebrew/bin")),
        "the GUI search omitted the standard Apple Silicon Homebrew bin directory"
    );
}
