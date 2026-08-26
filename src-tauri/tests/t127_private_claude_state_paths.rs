//! T-127 AC-1: a configured Claude process receives one private state directory per work key.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::{
    AgentDriver, DriverConfiguration, Policy, RunSpec, StepSettings,
};
use tempfile::TempDir;
use tokio::sync::mpsc;
use uuid::Uuid;

const PATIENCE: Duration = Duration::from_secs(10);
const CONTROL: &str = "LOADOUT_T127_CONNECTION_SURVIVED";
const HOME_SENTINEL: &[u8] = b"the person's claude state must stay byte-for-byte unchanged\n";
const HOSTILE_SENTINEL: &[u8] = b"hostile connection directory must not be used\n";

const FAKE_CLAUDE: &str = r#"#!/bin/sh
here=${0%/*}
if [ "${1-}" = "--version" ]; then
  printf 'config=%s\nhome=%s\n' "${CLAUDE_CONFIG_DIR-unset}" "${HOME-unset}" > "$here/probe.env"
  printf '%s\n' '2.1.241 (Claude Code)'
  exit 0
fi
state=${CLAUDE_CONFIG_DIR-}
if [ -z "$state" ]; then
  printf '%s\n' 'shared state was reached' > "$HOME/.claude.json"
  exit 31
fi
if [ ! -d "$state" ]; then
  exit 32
fi
printf 'config=%s\ncontrol=%s\nhome=%s\n' "$state" "${LOADOUT_T127_CONTROL-unset}" "${HOME-unset}" > "$state/seen.env"
printf '%s\n' 'only this process may write here' > "$state/marker"
IFS= read -r first_turn
printf '%s\n' '{"type":"system","subtype":"init","session_id":"01990000-0000-7000-8000-000000000127","model":"haiku","tools":[]}'
printf '%s\n' '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"done"}]}}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"duration_ms":2,"total_cost_usd":0.001,"result":"done"}'
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn private_state_overrides_hostile_connection_environment() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let binary = executable(bench.root.path())?;
    let base: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(binary));
    let configured = base
        .configured(&bench.configuration())
        .ok_or("Claude refused its public DriverConfiguration seam")?;

    let probe = tokio::time::timeout(PATIENCE, configured.probe()).await??;
    assert!(probe.found, "the executable probe did not run");
    assert_eq!(
        fs::read_to_string(bench.root.path().join("probe.env"))?
            .lines()
            .next(),
        Some("config=unset"),
        "a probe without StepSettings received CLAUDE_CONFIG_DIR"
    );
    assert!(
        !bench.run.path().join("claude").exists(),
        "the probe created run-owned Claude state without StepSettings"
    );

    let first = configured_step(&configured, &bench, "s_1")?;
    let second = configured_step(&configured, &bench, "s_1~2")?;
    let first_state = bench.state("s_1");
    let second_state = bench.state("s_1~2");
    assert!(
        first_state.is_dir() && second_state.is_dir(),
        "with_settings must create both private folders before start: {first_state:?}, {second_state:?}"
    );

    run_one(first, bench.project.path()).await?;
    run_one(second, bench.project.path()).await?;
    assert_report(&first_state, CONTROL, bench.fake_home.path())?;
    assert_report(&second_state, CONTROL, bench.fake_home.path())?;
    assert_ne!(first_state, second_state);
    assert_eq!(
        fs::read(&bench.home_state)?,
        HOME_SENTINEL,
        "a normal spawn touched the person's shared ~/.claude.json"
    );
    assert_eq!(fs::read(bench.hostile.join("sentinel"))?, HOSTILE_SENTINEL);
    assert_eq!(
        names_in(&bench.hostile)?,
        vec!["sentinel"],
        "a configured spawn wrote into the hostile Connection directory"
    );
    assert_eq!(
        names_in(&bench.run.path().join("claude"))?,
        vec!["s_1", "s_1~2"],
        "the probe or a retry created an extra private state folder"
    );
    Ok(())
}

fn configured_step(
    driver: &Arc<dyn AgentDriver>,
    bench: &Bench,
    work_key: &str,
) -> Result<Arc<dyn AgentDriver>, Box<dyn Error>> {
    let settings = StepSettings {
        dir: bench.run.path().to_path_buf(),
        work_key: work_key.to_owned(),
        memory: bench.run.path().join("mem").join(work_key),
        deny: Vec::new(),
    };
    driver
        .with_settings(&settings)
        .ok_or("Claude refused its public StepSettings seam")?
        .map_err(Into::into)
}

async fn run_one(driver: Arc<dyn AgentDriver>, cwd: &Path) -> Result<(), Box<dyn Error>> {
    let (events, _inbox) = mpsc::channel(16);
    let mut handle = driver
        .start(
            RunSpec {
                run_id: Uuid::now_v7(),
                cwd: cwd.to_path_buf(),
                prompt: "Return one short answer.".to_owned(),
                model: Some("haiku".to_owned()),
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
    Ok(())
}

fn assert_report(state: &Path, control: &str, home: &Path) -> Result<(), Box<dyn Error>> {
    let report = fs::read_to_string(state.join("seen.env"))?;
    assert_eq!(
        report,
        format!(
            "config={}\ncontrol={control}\nhome={}\n",
            state.display(),
            home.display()
        ),
        "the child reported a different final environment"
    );
    assert!(state.join("marker").is_file());
    Ok(())
}

fn executable(dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join("fake-claude");
    fs::write(&path, FAKE_CLAUDE)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
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
    root: TempDir,
    run: TempDir,
    project: TempDir,
    fake_home: TempDir,
    hostile: PathBuf,
    home_state: PathBuf,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let root = TempDir::new()?;
        let run = TempDir::new()?;
        let project = TempDir::new()?;
        let fake_home = TempDir::new()?;
        let hostile = root.path().join("hostile-connection-state");
        fs::create_dir_all(&hostile)?;
        fs::write(hostile.join("sentinel"), HOSTILE_SENTINEL)?;
        let home_state = fake_home.path().join(".claude.json");
        fs::write(&home_state, HOME_SENTINEL)?;
        Ok(Self {
            root,
            run,
            project,
            fake_home,
            hostile,
            home_state,
        })
    }

    fn state(&self, work_key: &str) -> PathBuf {
        self.run.path().join("claude").join(work_key)
    }

    fn configuration(&self) -> DriverConfiguration {
        DriverConfiguration {
            arguments: Vec::new(),
            environment: vec![
                (
                    "CLAUDE_CONFIG_DIR".to_owned(),
                    self.hostile.as_os_str().to_os_string(),
                ),
                ("LOADOUT_T127_CONTROL".to_owned(), OsString::from(CONTROL)),
                (
                    "HOME".to_owned(),
                    self.fake_home.path().as_os_str().to_os_string(),
                ),
            ],
            servers: Vec::new(),
        }
    }
}
