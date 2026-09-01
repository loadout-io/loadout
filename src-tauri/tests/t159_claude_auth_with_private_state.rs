//! T-159: private writable Claude state must keep the person's secure login namespace.

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
const CREDENTIAL_SENTINEL: &[u8] = b"the person's login stays outside the run\n";

const FAKE_CLAUDE: &str = r#"#!/bin/sh
if [ "${1-}" = "--version" ]; then
  printf '%s\n' '2.1.247 (Claude Code)'
  exit 0
fi
state=${CLAUDE_CONFIG_DIR-}
if [ -z "$state" ] || [ ! -d "$state" ]; then
  exit 31
fi
claude_dir=${state%/*}
key=${state##*/}
printf 'config=%s\nsecure_set=%s\nsecure=%s\n' \
  "$state" "${CLAUDE_SECURESTORAGE_CONFIG_DIR+x}" \
  "${CLAUDE_SECURESTORAGE_CONFIG_DIR-unset}" > "$state/seen.env"
: > "$state/start"
attempts=0
while [ ! -f "$claude_dir/s_1/start" ] || [ ! -f "$claude_dir/s_2/start" ]; do
  attempts=$((attempts + 1))
  if [ "$attempts" -ge 400 ]; then
    exit 32
  fi
  sleep 0.01
done
sleep 0.05
: > "$state/end"
IFS= read -r first_turn
printf '%s\n' '{"type":"system","subtype":"init","session_id":"01990000-0000-7000-8000-000000000159","model":"haiku","tools":[]}'
printf '%s\n' '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"done"}]}}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"duration_ms":2,"total_cost_usd":0.001,"result":"done"}'
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn private_steps_share_only_the_default_secure_storage_namespace()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let binary = executable(bench.root.path())?;
    let configured: Arc<dyn AgentDriver> =
        Arc::new(ClaudeDriver::with_binary(binary).with_configuration(bench.configuration()));
    let first = configured_step(&configured, &bench, "s_1")?;
    let second = configured_step(&configured, &bench, "s_2")?;

    let (first_result, second_result) = tokio::join!(
        run_one(first, bench.project.path()),
        run_one(second, bench.project.path())
    );
    first_result?;
    second_result?;

    let first_state = bench.state("s_1");
    let second_state = bench.state("s_2");
    assert_ne!(first_state, second_state);
    assert_report(&first_state)?;
    assert_report(&second_state)?;
    assert!(overlapped(&first_state, &second_state)?);
    assert_eq!(
        fs::read(&bench.credentials)?,
        CREDENTIAL_SENTINEL,
        "the process touched the person's credential source"
    );
    assert!(!first_state.join(".credentials.json").exists());
    assert!(!second_state.join(".credentials.json").exists());
    assert_eq!(
        names_in(&bench.hostile)?,
        vec!["sentinel"],
        "a hostile secure-storage override escaped the driver policy"
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

fn assert_report(state: &Path) -> Result<(), Box<dyn Error>> {
    let report = fs::read_to_string(state.join("seen.env"))?;
    assert!(
        report.contains(format!("config={}", state.display()).as_str()),
        "the child did not receive its private writable state: {report}"
    );
    assert!(
        report.contains("secure_set=x\nsecure=\n"),
        "the child did not receive the explicit default secure-storage namespace: {report}"
    );
    Ok(())
}

fn overlapped(first: &Path, second: &Path) -> Result<bool, Box<dyn Error>> {
    let first_start = fs::metadata(first.join("start"))?.modified()?;
    let first_end = fs::metadata(first.join("end"))?.modified()?;
    let second_start = fs::metadata(second.join("start"))?.modified()?;
    let second_end = fs::metadata(second.join("end"))?.modified()?;
    Ok(first_start.max(second_start) < first_end.min(second_end))
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
    credentials: PathBuf,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let root = TempDir::new()?;
        let run = TempDir::new()?;
        let project = TempDir::new()?;
        let fake_home = TempDir::new()?;
        let hostile = root.path().join("hostile-secure-storage");
        fs::create_dir_all(&hostile)?;
        fs::write(hostile.join("sentinel"), b"must remain unused\n")?;
        let credentials = fake_home.path().join(".claude/.credentials.json");
        fs::create_dir_all(credentials.parent().expect("credentials have a parent"))?;
        fs::write(&credentials, CREDENTIAL_SENTINEL)?;
        Ok(Self {
            root,
            run,
            project,
            fake_home,
            hostile,
            credentials,
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
                    "CLAUDE_SECURESTORAGE_CONFIG_DIR".to_owned(),
                    self.hostile.as_os_str().to_os_string(),
                ),
                (
                    "HOME".to_owned(),
                    self.fake_home.path().as_os_str().to_os_string(),
                ),
                ("LOADOUT_T159_UNUSED".to_owned(), OsString::from("control")),
            ],
            servers: Vec::new(),
        }
    }
}
