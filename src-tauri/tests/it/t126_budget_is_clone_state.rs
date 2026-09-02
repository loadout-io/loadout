//! T-126 AC-4: the reflection ceiling belongs to an explicitly configured driver clone.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use loadout_lib::commands::run::REFLECTION_MODEL;
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::{AgentDriver, Policy, RunSpec};
use tempfile::TempDir;
use tokio::sync::mpsc;
use uuid::Uuid;

const SESSION: &str = "01990000-0000-7000-8000-000000000126";
const PROMPT: &str = "Inspect the supplied facts and answer with one neutral sentence.";
const REFLECTION_BUDGET: f64 = 0.08;
const ORDINARY_BUDGET: f64 = 1.23;
const PATIENCE: Duration = Duration::from_secs(10);

const FAKE_CLAUDE: &str = r#"#!/bin/sh
here=${0%/*}
printf '%s\n' '--- spawn ---' >> "$here/argv.log"
printf '%s\n' "$@" >> "$here/argv.log"
IFS= read -r first
printf '%s\n' "$first" >> "$here/stdin.log"
printf '%s\n' '{"type":"system","subtype":"init","session_id":"01990000-0000-7000-8000-000000000126","model":"haiku","tools":[]}'
printf '%s\n' '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"done"}]}}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"duration_ms":2,"total_cost_usd":0.001,"result":"done"}'
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn budget_is_carried_by_the_clone_not_model_prompt_or_folder() -> Result<(), Box<dyn Error>> {
    let bench = TempDir::new()?;
    let binary = executable(bench.path())?;
    let base: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(binary));
    let plain = Arc::clone(&base);
    let own = budgeted(&base, ORDINARY_BUDGET, "ordinary driver")?;
    let reflected_a = reflected(&base)?;
    let reflected_b = reflected(&base)?;

    for (driver, folder) in [
        (plain, "ordinary-no-budget"),
        (own, "ordinary-own-budget"),
        (reflected_a, "reflection-a"),
        (reflected_b, "reflection-b"),
    ] {
        let cwd = bench.path().join(folder);
        fs::create_dir_all(&cwd)?;
        run_one(driver, cwd).await?;
    }

    let spawns = argv_spawns(&bench.path().join("argv.log"))?;
    assert_eq!(
        spawns.len(),
        4,
        "four configured clones must produce four real spawns"
    );
    assert_budget(&spawns[0], None)?;
    assert_budget(&spawns[1], Some(ORDINARY_BUDGET))?;
    assert_budget(&spawns[2], Some(REFLECTION_BUDGET))?;
    assert_budget(&spawns[3], Some(REFLECTION_BUDGET))?;
    for args in &spawns {
        assert_pair(args, "--model", REFLECTION_MODEL);
        assert_pair(args, "--permission-mode", "dontAsk");
    }
    assert_identical_stdin(&bench.path().join("stdin.log"))?;
    Ok(())
}

fn reflected(base: &Arc<dyn AgentDriver>) -> Result<Arc<dyn AgentDriver>, Box<dyn Error>> {
    let clone = base
        .reflecting()
        .ok_or("Claude did not expose its explicit reflection clone")?;
    budgeted(&clone, REFLECTION_BUDGET, "reflection clone")
}

fn budgeted(
    driver: &Arc<dyn AgentDriver>,
    dollars: f64,
    kind: &str,
) -> Result<Arc<dyn AgentDriver>, Box<dyn Error>> {
    driver
        .with_budget(dollars)
        .ok_or_else(|| format!("{kind} refused the budget wrapper before a real spawn").into())
}

async fn run_one(driver: Arc<dyn AgentDriver>, cwd: PathBuf) -> Result<(), Box<dyn Error>> {
    let run_id = Uuid::parse_str(SESSION)?;
    let spec = RunSpec {
        run_id,
        cwd,
        prompt: PROMPT.to_owned(),
        model: Some(REFLECTION_MODEL.to_owned()),
        system_append: None,
        policy: Policy::ReadOnly,
        reaches_the_web: false,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    };
    let (events, _inbox) = mpsc::channel(16);
    let mut handle = driver.start(spec, events).await?;
    let outcome = tokio::time::timeout(PATIENCE, handle.wait()).await??;
    assert!(
        outcome.ok,
        "the fixture process did not finish its neutral turn"
    );
    let _ = handle.close().await?;
    Ok(())
}

fn executable(dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join("fake-claude");
    fs::write(&path, FAKE_CLAUDE)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn argv_spawns(path: &Path) -> Result<Vec<Vec<String>>, Box<dyn Error>> {
    let text = fs::read_to_string(path)?;
    let mut out = Vec::new();
    for block in text
        .split("--- spawn ---\n")
        .filter(|block| !block.is_empty())
    {
        out.push(block.lines().map(str::to_owned).collect());
    }
    Ok(out)
}

fn assert_budget(args: &[String], wanted: Option<f64>) -> Result<(), Box<dyn Error>> {
    let found: Vec<f64> = args
        .windows(2)
        .filter(|pair| pair[0] == "--max-budget-usd")
        .map(|pair| pair[1].parse::<f64>())
        .collect::<Result<_, _>>()?;
    match wanted {
        None => assert!(
            found.is_empty(),
            "an ordinary clone without a budget got {found:?}"
        ),
        Some(wanted) => {
            assert_eq!(
                found.len(),
                1,
                "the spawn must carry exactly one budget flag"
            );
            assert!(
                (found[0] - wanted).abs() < 0.000_001,
                "the spawn carried ${} instead of ${wanted}",
                found[0]
            );
        }
    }
    Ok(())
}

fn assert_identical_stdin(path: &Path) -> Result<(), Box<dyn Error>> {
    let lines: Vec<String> = fs::read_to_string(path)?
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(
        lines.len(),
        4,
        "each real spawn must receive one stdin envelope"
    );
    assert!(
        lines.windows(2).all(|pair| pair[0] == pair[1]),
        "model, prompt and policy are neutral controls; only clone state may distinguish a turn"
    );
    assert!(lines.iter().all(|line| line.contains(PROMPT)));
    Ok(())
}

fn assert_pair(args: &[String], flag: &str, value: &str) {
    assert!(
        args.windows(2)
            .any(|pair| pair[0] == flag && pair[1] == value),
        "spawn lost the required {flag} {value} pair: {args:?}"
    );
}
