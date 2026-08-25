//! AC-1 dla T-115: znane modele Codeksa mają rozróżnialne kolumny cennika, a księga je sumuje.
//!
//! T-102 przeszło zielono, bo Terra i Luna dostawały po milionie tokenów KAŻDEGO rodzaju.
//! Zamiana wejścia, cache i wyjścia zachowywała wtedy sumę. Tutaj prawdziwy adapter czyta
//! 10 000 wejścia, 5 000 cache i 20 000 wyjścia dla każdego modelu, więc każda transpozycja
//! dwóch różnych stawek zmienia wynik.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_with_budget;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Policy, Probe,
    RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::Vendor;
use loadout_lib::store::Store;
use serde_json::Value;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;
use uuid::Uuid;

const INPUT: u64 = 10_000;
const CACHED: u64 = 5_000;
const OUTPUT: u64 = 20_000;
const SOL: f64 = 0.442;
const TERRA: f64 = 0.261;
const LUNA: f64 = 0.0261;
const CLAUDE_COST: f64 = 0.73;
const BUDGET: f64 = 10.0;
const PATIENCE: Duration = Duration::from_secs(30);

const CODEX_CLI: &str = r###"#!/bin/sh
printf '%s\n' '{"type":"thread.started","thread_id":"thread-t115-priced"}'
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","id":"item-1","text":"## Answer\nDone.\n\n## Evidence\nnone.\n\n## Open\nnothing."}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":10000,"cached_input_tokens":5000,"output_tokens":20000}}'
exit 0
"###;

const CODEX_AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-000000001151
name: Code
summary: Prices the Codex step
color: moss
runsWith: codex
model: gpt-5.6-sol
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

const CLAUDE_AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-000000001152
name: Check
summary: Supplies a measured vendor cost
color: plum
runsWith: claude-code
model: opus
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 20
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Check the work.
";

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_t115_both_vendor_costs",
  "name": "Both vendor costs",
  "steps": [
    {
      "kind": "agent",
      "id": "s_code",
      "name": "Code",
      "agent": "01990000-0000-7000-8000-000000001151",
      "overrides": {},
      "instructions": "Price the Codex work.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_check",
      "name": "Check",
      "agent": "01990000-0000-7000-8000-000000001152",
      "overrides": {},
      "instructions": "Measure the Claude work.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    }
  ],
  "links": []
}"#;

fn write_cli(dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join("codex-t115");
    fs::write(&path, CODEX_CLI)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn spec(cwd: &Path, model: &str) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: "price this turn".to_owned(),
        model: Some(model.to_owned()),
        system_append: None,
        reaches_the_web: false,
        policy: Policy::ReadOnly,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

async fn price_from_real_adapter(model: &str) -> Result<Outcome, Box<dyn Error>> {
    let dir = TempDir::new()?;
    let binary = write_cli(dir.path())?;
    let (tx, _rx) = mpsc::channel(64);
    let driver = CodexDriver::with_binary(binary);
    let mut handle =
        tokio::time::timeout(PATIENCE, driver.start_session(spec(dir.path(), model), tx)).await??;
    let outcome = tokio::time::timeout(PATIENCE, handle.wait()).await??;
    drop(handle);
    Ok(outcome)
}

fn assert_money(actual: f64, expected: f64, context: &str) {
    let difference = (actual - expected).abs();
    assert!(
        difference < 1e-12,
        "{context}: expected ${expected}, got ${actual} (difference {difference})"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_known_model_prices_three_unequal_token_columns() -> Result<(), Box<dyn Error>> {
    for (model, expected) in [
        ("gpt-5.6-sol", SOL),
        ("gpt-5.6-sol-2026-08-25", SOL),
        ("gpt-5.6-terra", TERRA),
        ("gpt-5.6-luna", LUNA),
    ] {
        let outcome = price_from_real_adapter(model).await?;
        assert_eq!(
            outcome.tokens,
            Tokens {
                input: INPUT,
                cached: CACHED,
                output: OUTPUT,
            },
            "{model}: the oracle is meaningful only if the real adapter kept the deliberately \
             unequal input/cache/output columns"
        );
        let cost = outcome.cost_usd.ok_or_else(|| {
            format!("{model}: this is a known model prefix, so its 10k/5k/20k usage needs a price")
        })?;
        assert_money(cost, expected, model);
    }
    Ok(())
}

fn estimate_markers(step: &Value) -> Vec<(&str, &Value)> {
    step.as_object()
        .into_iter()
        .flat_map(|fields| fields.iter())
        .filter(|(name, value)| {
            name.to_ascii_lowercase().contains("estimat")
                && (value.as_bool() == Some(true)
                    || value.as_str().is_some_and(|word| word == "estimate"))
        })
        .map(|(name, value)| (name.as_str(), value))
        .collect()
}

fn step_named<'a>(run: &'a Value, name: &str) -> Result<&'a Value, Box<dyn Error>> {
    run.get("steps")
        .and_then(Value::as_array)
        .ok_or("run.json has no steps array")?
        .iter()
        .find(|step| step.get("name").and_then(Value::as_str) == Some(name))
        .ok_or_else(|| format!("run.json has no step named {name}").into())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn run_json_marks_only_the_estimate_and_spent_in_sums_it() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let (report, run) = bench.run().await?;
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded, StepState::Succeeded],
        "both priced steps must really finish before their ledger entries prove anything. The \
         run file was {run}"
    );

    let codex = step_named(&run, "Code")?;
    assert_eq!(
        codex.get("input_tokens").and_then(Value::as_u64),
        Some(INPUT)
    );
    assert_eq!(
        codex.get("cached_tokens").and_then(Value::as_u64),
        Some(CACHED)
    );
    assert_eq!(
        codex.get("output_tokens").and_then(Value::as_u64),
        Some(OUTPUT)
    );
    let codex_cost = codex
        .get("cost_usd")
        .and_then(Value::as_f64)
        .ok_or("the real Codex step has no cost in run.json")?;
    assert_money(codex_cost, SOL, "the Codex step in run.json");
    assert_eq!(
        estimate_markers(codex).len(),
        1,
        "the Codex price is analytical, so its run.json entry needs exactly one explicit \
         estimate marker. The step was {codex}"
    );

    let claude = step_named(&run, "Check")?;
    assert_money(
        claude
            .get("cost_usd")
            .and_then(Value::as_f64)
            .ok_or("the measured Claude step lost its cost")?,
        CLAUDE_COST,
        "the Claude step in run.json",
    );
    assert!(
        estimate_markers(claude).is_empty(),
        "a measured Claude amount must not be labelled as an estimate: {claude}"
    );

    let spent = run
        .get("spent_usd")
        .and_then(Value::as_f64)
        .ok_or("a budgeted run has no spent_usd total")?;
    assert_money(
        spent,
        SOL + CLAUDE_COST,
        "spent_in over one estimated and one measured step",
    );
    Ok(())
}

#[derive(Debug)]
struct MeasuredClaude;

#[async_trait]
impl AgentDriver for MeasuredClaude {
    fn id(&self) -> &'static str {
        "claude"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("measured fixture".to_owned()),
        })
    }

    fn with_evidence(&self, _target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let session = SessionRef {
            vendor: "claude",
            id: spec.run_id.to_string(),
        };
        let _ = events
            .send(
                (AgentEvent::Started {
                    session: session.clone(),
                    model: spec.model.unwrap_or_default(),
                    tools: Vec::new(),
                    capabilities: Vec::new(),
                })
                .into(),
            )
            .await;
        Ok(Box::new(MeasuredTurn { events, session }))
    }
}

#[derive(Debug)]
struct MeasuredTurn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for MeasuredTurn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        let outcome = Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "## Answer\nChecked.\n\n## Evidence\nnone.\n\n## Open\nnothing.".to_owned(),
            cost_usd: Some(CLAUDE_COST),
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_secs(1),
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send((AgentEvent::Finished(outcome.clone())).into())
            .await;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
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
        fs::write(home.path().join("agents/code.md"), CODEX_AGENT)?;
        fs::write(home.path().join("agents/check.md"), CLAUDE_AGENT)?;
        let workflow = home.path().join("workflows/both-costs.json");
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

    fn drivers(&self) -> Drivers {
        let codex: Arc<dyn AgentDriver> = Arc::new(CodexDriver::with_binary(self.binary.clone()));
        let claude: Arc<dyn AgentDriver> = Arc::new(MeasuredClaude);
        Arc::new(move |vendor| match vendor {
            Vendor::Codex => Arc::clone(&codex),
            Vendor::ClaudeCode => Arc::clone(&claude),
        })
    }

    async fn run(&self) -> Result<(RunReport, Value), Box<dyn Error>> {
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store: &self.store,
            drivers: self.drivers(),
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow: self.workflow.clone(),
            how_many_at_once: 2,
            task: None,
            part: None,
            handoffs_from: None,
        };
        let (sink, source) = line_channel(QUEUE_CAP);
        let pump = spawn_pump(source, Channel::new(|_| Ok(())));
        let (report, _) = tokio::time::timeout(PATIENCE, async {
            tokio::join!(
                run_workflow_with_budget(&deps, &request, sink, Some(BUDGET)),
                pump
            )
        })
        .await
        .map_err(|_| format!("the priced run did not finish within {PATIENCE:?}"))?;
        let report = report?;
        let run = serde_json::from_str(&fs::read_to_string(report.dir.join("run.json"))?)?;
        Ok((report, run))
    }
}
