//! AC-3 dla T-115: nieznany model zachowuje tokeny i mówi człowiekowi, że ceny nie zna.
//!
//! Wyrocznia czyta dwa prawdziwe artefakty: `run.json` po pełnym biegu oraz zserializowany
//! końcowy wiersz z adaptera przez produkcyjnego kuratora. Prywatny wynik funkcji wyceniającej
//! nie wystarcza — nie ma go ani na dysku, ani na ekranie (niezmiennik 29).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::{AgentDriver, AgentHandle, Outcome, Policy, RunSpec, Tokens};
use loadout_lib::engine::line::{Curator, Line, LineKind, Seen};
use loadout_lib::engine::step::StepState;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use serde_json::Value;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;
use uuid::Uuid;

const MODEL: &str = "gpt-9.9-nebula";
const INPUT: u64 = 10_000;
const CACHED: u64 = 5_000;
const OUTPUT: u64 = 20_000;
const PATIENCE: Duration = Duration::from_secs(30);

const CODEX_CLI: &str = r###"#!/bin/sh
printf '%s\n' '{"type":"thread.started","thread_id":"thread-t115-unknown"}'
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","id":"item-1","text":"## Answer\nDone.\n\n## Evidence\nnone.\n\n## Open\nnothing."}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":10000,"cached_input_tokens":5000,"output_tokens":20000}}'
exit 0
"###;

const AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-000000001153
name: Unknown Price
summary: Runs an unknown Codex model
color: moss
runsWith: codex
model: gpt-9.9-nebula
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 20
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Do the work.
";

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_t115_unknown_price",
  "name": "Unknown Codex price",
  "steps": [
    {
      "kind": "agent",
      "id": "s_unknown",
      "name": "Unknown Price",
      "agent": "01990000-0000-7000-8000-000000001153",
      "overrides": {},
      "instructions": "Run the unknown model.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}"#;

fn write_cli(dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join("codex-t115-unknown");
    fs::write(&path, CODEX_CLI)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn spec(cwd: &Path) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: "run the unknown model".to_owned(),
        model: Some(MODEL.to_owned()),
        system_append: None,
        reaches_the_web: false,
        policy: Policy::ReadOnly,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

async fn adapter_outcome_and_visible_lines() -> Result<(Outcome, Vec<Line>), Box<dyn Error>> {
    let dir = TempDir::new()?;
    let binary = write_cli(dir.path())?;
    let driver = CodexDriver::with_binary(binary);
    let (tx, mut rx) = mpsc::channel(64);
    let mut handle =
        tokio::time::timeout(PATIENCE, driver.start_session(spec(dir.path()), tx)).await??;
    let outcome = tokio::time::timeout(PATIENCE, handle.wait()).await??;
    drop(handle);

    let mut decoded = Vec::new();
    while let Ok(event) = rx.try_recv() {
        decoded.push(event);
    }
    let mut curator = Curator::new();
    let mut lines = Vec::new();
    for (at_ms, event) in decoded.iter().enumerate() {
        lines.extend(curator.observe(Seen {
            agent: "Unknown Price",
            at_ms: u64::try_from(at_ms).unwrap_or_default(),
            event: &event.event,
            tool: None,
        }));
    }
    lines.extend(curator.flush());
    Ok((outcome, lines))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_serialized_final_row_names_the_model_whose_price_is_unknown()
-> Result<(), Box<dyn Error>> {
    let (outcome, lines) = adapter_outcome_and_visible_lines().await?;
    assert_eq!(
        outcome.tokens,
        Tokens {
            input: INPUT,
            cached: CACHED,
            output: OUTPUT,
        },
        "an unknown price must not discard the usage the vendor did report"
    );
    assert_eq!(
        outcome.cost_usd, None,
        "unknown is not zero and not a guessed amount"
    );

    let row = lines
        .iter()
        .find(|line| line.kind() == LineKind::Done)
        .ok_or("the real adapter stream produced no final row for the UI")?;
    let wire = serde_json::to_value(row)?;
    let text = wire
        .get("text")
        .and_then(Value::as_str)
        .ok_or("the serialized final row has no sentence a person can read")?;
    let lower = text.to_ascii_lowercase();
    assert!(
        lower.contains("price")
            && (lower.contains("not known") || lower.contains("unknown"))
            && text.contains(MODEL),
        "the final row must tell the person that the price for {MODEL} is not known. It said \
         {text:?}; the full serialized row was {wire}"
    );
    assert!(
        !text.contains("$0.00"),
        "a model with no known price must never look free: {text:?}"
    );
    Ok(())
}

fn provenance_fields(step: &Value) -> Vec<&str> {
    step.as_object()
        .into_iter()
        .flat_map(|fields| fields.keys())
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            lower.contains("estimat") || lower.contains("measur")
        })
        .map(String::as_str)
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn run_json_keeps_usage_without_money_or_false_provenance() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let (report, run) = bench.run().await?;
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded],
        "the step must finish before its run.json row proves anything"
    );
    let step = run
        .get("steps")
        .and_then(Value::as_array)
        .and_then(|steps| steps.first())
        .ok_or("run.json has no first step")?;
    assert_eq!(
        step.get("input_tokens").and_then(Value::as_u64),
        Some(INPUT)
    );
    assert_eq!(
        step.get("cached_tokens").and_then(Value::as_u64),
        Some(CACHED)
    );
    assert_eq!(
        step.get("output_tokens").and_then(Value::as_u64),
        Some(OUTPUT)
    );
    assert!(
        step.get("cost_usd").is_some_and(Value::is_null),
        "the unknown model must keep a null price, never zero or an omitted token ledger: {step}"
    );
    assert!(
        provenance_fields(step).is_empty(),
        "a price that does not exist is neither measured nor estimated, so it gets no false \
         provenance field. Found {:?} in {step}",
        provenance_fields(step)
    );
    assert!(
        !run.to_string().contains("$0.00"),
        "run.json must not manufacture the display string $0.00 for missing money"
    );
    Ok(())
}

struct Bench {
    home: TempDir,
    project: TempDir,
    workflow: PathBuf,
    binary: PathBuf,
    store: Store,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(home.path().join("agents/unknown.md"), AGENT)?;
        let workflow = home.path().join("workflows/unknown.json");
        fs::write(&workflow, WORKFLOW)?;
        let binary = write_cli(home.path())?;
        let store = Store::open(&project.path().join(".loadout/loadout.db"))?;
        Ok(Self {
            home,
            project,
            workflow,
            binary,
            store,
        })
    }

    async fn run(&self) -> Result<(loadout_lib::commands::RunReport, Value), Box<dyn Error>> {
        let driver: Arc<dyn AgentDriver> = Arc::new(CodexDriver::with_binary(self.binary.clone()));
        let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store: &self.store,
            drivers,
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow: self.workflow.clone(),
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        };
        let (sink, source) = line_channel(QUEUE_CAP);
        let pump = spawn_pump(source, Channel::new(|_| Ok(())));
        let (report, _) = tokio::time::timeout(PATIENCE, async {
            tokio::join!(run_workflow_inner(&deps, &request, sink), pump)
        })
        .await
        .map_err(|_| format!("the unknown-model run did not finish within {PATIENCE:?}"))?;
        let report = report?;
        let run = serde_json::from_str(&fs::read_to_string(report.dir.join("run.json"))?)?;
        Ok((report, run))
    }
}
