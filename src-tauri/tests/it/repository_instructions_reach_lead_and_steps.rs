//! WF-12: instrukcje repo docierają do rzeczywistej granicy start/send, nie tylko do skanera.
//! Ten sam dubler zastępuje wyłącznie CLI; plan, izolacja, actor Leada i prompt są produkcyjne.

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::chat::{Lead, Terminal, Threads};
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{line_channel, spawn_pump};
use loadout_lib::library::agents::{Agent, Vendor};
use loadout_lib::store::Store;
use serde_json::json;
use tempfile::TempDir;
use tokio::sync::mpsc;

const ROOT_RULE: &str = "WF12-ROOT-RULE-ORIGINAL";
const LATER_RULE: &str = "WF12-ROOT-RULE-NEXT-TURN";
const CLAUDE_RULE: &str = "WF12-ADDITIONAL-CLAUDE-RULE";
const WEB_RULE: &str = "WF12-WEB-ONLY-RULE";
const API_RULE: &str = "WF12-API-ONLY-RULE";
const LOCAL_RULE: &str = "WF12-PRIVATE-LOCAL-NOT-SELECTED";
const AGENT: &str = "01990000-0000-7000-8000-000000000012";

#[tokio::test]
async fn claude_steps_receive_one_frozen_scoped_package() -> Result<(), Box<dyn Error>> {
    steps_receive_instructions("claude-code").await
}

#[tokio::test]
async fn codex_steps_receive_one_frozen_scoped_package() -> Result<(), Box<dyn Error>> {
    steps_receive_instructions("codex").await
}

#[tokio::test]
async fn claude_lead_refreshes_instructions_between_real_turns() -> Result<(), Box<dyn Error>> {
    lead_receives_instructions("claude-code").await
}

#[tokio::test]
async fn codex_lead_refreshes_instructions_between_real_turns() -> Result<(), Box<dyn Error>> {
    lead_receives_instructions("codex").await
}

#[tokio::test]
async fn selected_instruction_refusal_has_one_owner_at_the_real_lead_ipc()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new("claude-code")?;
    bench.enable()?;
    fs::write(
        bench.project().join("AGENTS.md"),
        "@include missing-policy.md\n",
    )?;
    let store = Store::open(&bench.project().join(".loadout/loadout.db"))?;
    let state =
        loadout_lib::ipc::AppState::new(bench.home(), bench.project(), store, bench.drivers(false));
    let (sink, mut source) = line_channel(256);
    state.watching_the_lead("wf12-ipc", None, sink).await?;
    let error = loadout_lib::ipc::say_to_orchestrator_from_window(
        &state,
        "wf12-ipc",
        None,
        Some(AGENT),
        "Keep this unsent message",
        Vec::new(),
    )
    .await
    .err()
    .ok_or("selected missing instructions must refuse before the CLI starts")?;
    state.close_everything_down().await;
    assert!(
        error.contains("missing-policy.md"),
        "the real IPC hid the selected source: {error}"
    );
    assert!(
        bench
            .seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty()
    );
    let mut repeated = Vec::new();
    while let Some(line) = source.try_next() {
        if line.text().contains("missing-policy.md") {
            repeated.push(line);
        }
    }
    assert!(
        repeated.is_empty(),
        "Entry renders the rejected IPC; the actor sent the same refusal again: {repeated:?}"
    );
    Ok(())
}

async fn steps_receive_instructions(vendor: &'static str) -> Result<(), Box<dyn Error>> {
    let bench = Bench::new(vendor)?;
    bench.enable()?;
    bench.run_steps(true).await?;
    let seen = bench.seen.lock().unwrap_or_else(PoisonError::into_inner);
    assert_eq!(
        seen.len(),
        2,
        "both real workflow copies must reach the driver"
    );
    for prompt in seen.iter() {
        assert_package(prompt, ROOT_RULE);
        assert!(
            !prompt.contains(LATER_RULE),
            "the second copy read instructions edited after the first process started"
        );
    }
    drop(seen);
    let saved = bench.saved_run()?;
    let history = loadout_lib::commands::history::read_run_inner(
        &bench.project(),
        saved
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("invalid run folder")?,
    )?;
    for step in &history.steps {
        assert!(
            step.project_instructions
                .iter()
                .any(|source| source.path == std::path::Path::new("AGENTS.md"))
        );
        assert!(
            step.project_instructions
                .iter()
                .all(|source| source.digest.len() == 64)
        );
        assert!(
            step.loaded_by_the_app.is_none(),
            "supplied instructions must not pretend the vendor reported loading them"
        );
    }
    let wire = serde_json::to_string(&history)?;
    assert!(
        !wire.contains(ROOT_RULE),
        "the public history exported raw instructions"
    );
    assert!(!bench.project().join("HOOK-RAN").exists());
    Ok(())
}

async fn lead_receives_instructions(vendor: &'static str) -> Result<(), Box<dyn Error>> {
    let bench = Bench::new(vendor)?;
    bench.enable()?;
    let threads = Threads::new();
    threads.library_is(bench.home());
    let terminal = Terminal {
        id: "wf12-lead".to_owned(),
        folder: bench.project(),
    };
    let mut agent = Agent::example();
    agent.runs_with = if vendor == "codex" {
        Vendor::Codex
    } else {
        Vendor::ClaudeCode
    };
    let lead = Lead { agent };
    let (sink, _source) = line_channel(256);
    threads.terminal_lines_go_to(&terminal, sink);
    let drivers = bench.drivers(false);
    threads
        .say_in(&drivers, &lead, &terminal, "First question")
        .await?;
    fs::write(bench.project().join("AGENTS.md"), LATER_RULE)?;
    threads
        .say_in(&drivers, &lead, &terminal, "Next question")
        .await?;
    let _ = threads.close().await;
    let seen = bench.seen.lock().unwrap_or_else(PoisonError::into_inner);
    assert_eq!(
        seen.len(),
        2,
        "the first start and actual follow-up must both reach the driver"
    );
    assert_package(&seen[0], ROOT_RULE);
    assert_package(&seen[1], LATER_RULE);
    assert!(
        !seen[1].contains(ROOT_RULE),
        "the new turn received a stale instruction package"
    );
    assert!(!bench.project().join("HOOK-RAN").exists());
    Ok(())
}

#[tokio::test]
async fn old_projects_do_not_silently_enable_instruction_inheritance() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new("claude-code")?;
    bench.run_steps(false).await?;
    for prompt in bench
        .seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
    {
        assert!(!prompt.contains(ROOT_RULE));
        assert!(!prompt.contains(LOCAL_RULE));
    }
    Ok(())
}

#[tokio::test]
async fn explicit_step_choice_overrides_only_the_project_text_choice() -> Result<(), Box<dyn Error>>
{
    for (project_on, step_on) in [(true, false), (false, true)] {
        let bench = Bench::new("claude-code")?;
        if project_on {
            bench.enable()?;
        }
        bench.run_steps_with_choice(false, Some(step_on)).await?;
        for prompt in bench
            .seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
        {
            assert_eq!(prompt.contains(ROOT_RULE), step_on);
            assert!(!prompt.contains("WF12-ALLOW-ANYTHING"));
            assert!(!prompt.contains("WF12-ENV-VALUE"));
        }
    }
    Ok(())
}

#[tokio::test]
async fn includes_keep_scope_and_do_not_follow_markdown_or_code_examples()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new("claude-code")?;
    bench.enable()?;
    fs::create_dir_all(bench.project().join("web/policies"))?;
    fs::write(
        bench.project().join("web/AGENTS.md"),
        format!(
            "{WEB_RULE}\n@include policies/first.md\n[not an include](missing.md)\n```text\n@include absent.md\n```\n"
        ),
    )?;
    fs::write(
        bench.project().join("web/policies/first.md"),
        "WF12-INCLUDED-FIRST\n@include next.md\n",
    )?;
    fs::write(
        bench.project().join("web/policies/next.md"),
        "WF12-INCLUDED-NEXT\n",
    )?;
    bench.run_steps(false).await?;
    for prompt in bench
        .seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
    {
        assert!(prompt.contains("WF12-INCLUDED-FIRST"));
        assert!(prompt.contains("Source: web/policies/next.md\nScope directory: web\n"));
        assert!(prompt.contains("WF12-INCLUDED-NEXT"));
    }
    Ok(())
}

#[tokio::test]
async fn cyclic_missing_external_and_oversized_sources_refuse_before_any_step()
-> Result<(), Box<dyn Error>> {
    for (body, source) in [
        ("@include AGENTS.md\n".to_owned(), "AGENTS.md"),
        ("@include missing.md\n".to_owned(), "missing.md"),
        ("@include ../outside.md\n".to_owned(), "AGENTS.md"),
        ("x".repeat(64 * 1024 + 1), "AGENTS.md"),
    ] {
        let bench = Bench::new("claude-code")?;
        bench.enable()?;
        fs::write(bench.project().join("AGENTS.md"), body)?;
        let error = bench
            .run_steps(false)
            .await
            .err()
            .ok_or("selected invalid instructions were ignored")?
            .to_string();
        assert!(
            error.contains(source),
            "refusal did not name its source: {error}"
        );
        assert!(
            bench
                .seen
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_empty()
        );
    }
    Ok(())
}

#[test]
fn project_settings_preserve_unknown_fields_without_exporting_them() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new("claude-code")?;
    let file = bench.project().join(".loadout/project.json");
    fs::write(&file, json!({"future":{"private":"WF12-NOT-FOR-VIEW"}, "instructions":{"enabled":false,"futureInstruction":42}}).to_string())?;
    loadout_lib::inherit::instructions::save_settings(
        &bench.project(),
        &json!({"instructions":{"enabled":true},"leadInstructions":false}),
    )?;
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(file)?)?;
    assert_eq!(saved["future"]["private"], "WF12-NOT-FOR-VIEW");
    assert_eq!(saved["instructions"]["futureInstruction"], 42);
    assert_eq!(saved["leadInstructions"], false);
    let view = serde_json::to_string(&loadout_lib::inherit::instructions::settings_view(
        &bench.project(),
    )?)?;
    assert!(!view.contains("WF12-NOT-FOR-VIEW"));
    assert!(!view.contains(ROOT_RULE));
    let lead = loadout_lib::inherit::instructions::for_lead(&bench.project())?;
    assert!(
        lead.prompt(true).is_empty(),
        "explicit lead opt-out was ignored"
    );
    Ok(())
}

#[tokio::test]
async fn replay_refuses_a_missing_bound_instruction_package_instead_of_using_current_repo()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new("claude-code")?;
    bench.enable()?;
    bench.run_steps(false).await?;
    let previous = bench.saved_run()?;
    let manifest = previous.join("instructions/manifest.json");
    fs::rename(
        &manifest,
        previous.join("instructions/missing-manifest.json"),
    )?;
    let before = bench
        .seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .len();
    let error = bench
        .run_steps_from(false, None, Some(previous))
        .await
        .err()
        .ok_or("replay replaced the missing original instructions")?
        .to_string();
    assert!(
        error.contains("instructions"),
        "missing package was not the reported refusal: {error}"
    );
    assert_eq!(
        bench
            .seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len(),
        before
    );
    Ok(())
}

fn assert_package(prompt: &str, root: &str) {
    for marker in [root, CLAUDE_RULE, WEB_RULE, API_RULE] {
        assert!(
            prompt.contains(marker),
            "the actual driver did not receive source content {marker}"
        );
    }
    for source in [
        "AGENTS.md",
        "CLAUDE.md",
        "web/AGENTS.md",
        ".claude/rules/api.md",
    ] {
        assert!(
            prompt.contains(source),
            "the instruction text lost its source {source}"
        );
    }
    assert!(
        prompt.contains("Scope directory: web"),
        "web instructions became global"
    );
    assert!(
        prompt.contains("api/**"),
        "the rule lost its paths restriction"
    );
    assert!(prompt.contains("AGENTS.md takes precedence"));
    assert!(!prompt.contains(LOCAL_RULE));
    assert!(!prompt.contains("WF12-ALLOW-ANYTHING"));
    assert!(!prompt.contains("WF12-ENV-VALUE"));
}

struct Bench {
    root: TempDir,
    vendor: &'static str,
    seen: Arc<Mutex<Vec<String>>>,
}

impl Bench {
    fn new(vendor: &'static str) -> Result<Self, Box<dyn Error>> {
        let bench = Self {
            root: tempfile::tempdir()?,
            vendor,
            seen: Arc::new(Mutex::new(Vec::new())),
        };
        fs::create_dir_all(bench.home().join("agents"))?;
        fs::create_dir_all(bench.home().join("workflows"))?;
        fs::create_dir_all(bench.project().join(".loadout"))?;
        fs::create_dir_all(bench.project().join("web"))?;
        fs::create_dir_all(bench.project().join(".claude/rules"))?;
        fs::write(bench.project().join("AGENTS.md"), ROOT_RULE)?;
        fs::write(bench.project().join("CLAUDE.md"), CLAUDE_RULE)?;
        fs::write(bench.project().join("web/AGENTS.md"), WEB_RULE)?;
        fs::write(
            bench.project().join(".claude/rules/api.md"),
            format!("---\npaths:\n  - 'api/**'\n---\n{API_RULE}\n"),
        )?;
        fs::write(bench.project().join("CLAUDE.local.md"), LOCAL_RULE)?;
        fs::write(bench.project().join(".claude/settings.json"), json!({
            "permissions":{"allow":["WF12-ALLOW-ANYTHING"]}, "env":{"WF12_SECRET":"WF12-ENV-VALUE"},
            "hooks":{"PreToolUse":[{"hooks":[{"type":"command","command":"touch HOOK-RAN"}]}]}
        }).to_string())?;
        fs::write(
            bench.home().join("agents/reader.md"),
            format!(
                "---\nschema: 1\nid: {AGENT}\nname: Reader\nsummary: Reads instructions\ncolor: slate\nrunsWith: {vendor}\nmodel: opus\nthinking: balanced\nfileAccess: look-only\ngiveUpAfterMinutes: 20\nwriteResultsTo: \"\"\ntools: everything\nskills: []\nconnections: []\n---\nRead the project.\n"
            ),
        )?;
        Ok(bench)
    }
    fn home(&self) -> PathBuf {
        self.root.path().join("home")
    }
    fn project(&self) -> PathBuf {
        self.root.path().join("project")
    }
    fn saved_run(&self) -> Result<PathBuf, Box<dyn Error>> {
        fs::read_dir(self.project().join(".loadout/runs"))?
            .next()
            .ok_or_else(|| "missing run".into())
            .and_then(|entry| Ok(entry?.path()))
    }
    fn enable(&self) -> Result<(), Box<dyn Error>> {
        fs::write(
            self.project().join(".loadout/project.json"),
            json!({"instructions":{"enabled":true,"includeLocal":false}}).to_string(),
        )?;
        Ok(())
    }
    fn drivers(&self, mutate: bool) -> Drivers {
        let driver: Arc<dyn AgentDriver> = Arc::new(Recorder {
            vendor: self.vendor,
            project: self.project(),
            seen: Arc::clone(&self.seen),
            mutate,
        });
        Arc::new(move |_| Arc::clone(&driver))
    }
    async fn run_steps(&self, mutate: bool) -> Result<(), Box<dyn Error>> {
        self.run_steps_with_choice(mutate, None).await
    }
    async fn run_steps_with_choice(
        &self,
        mutate: bool,
        choice: Option<bool>,
    ) -> Result<(), Box<dyn Error>> {
        self.run_steps_from(mutate, choice, None).await
    }
    async fn run_steps_from(
        &self,
        mutate: bool,
        choice: Option<bool>,
        previous: Option<PathBuf>,
    ) -> Result<(), Box<dyn Error>> {
        let workflow = self.home().join("workflows/instructions.json");
        fs::write(&workflow, json!({"format":1,"id":"wf12","name":"Project instructions","links":[],"steps":[{
            "kind":"agent","id":"reader","name":"Read instructions","agent":AGENT,
            "copies":2,"instructions":"Read only","folder":{"use":"fresh-copy"},"projectInstructions":choice,"at":{"x":0,"y":0}
        }]}).to_string())?;
        let store = Store::open(&self.project().join(".loadout/loadout.db"))?;
        let deps = RunDeps {
            home: &self.home(),
            project: &self.project(),
            store: &store,
            drivers: self.drivers(mutate),
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow,
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: previous,
        };
        let (sink, source) = line_channel(256);
        let pump = spawn_pump(source, tauri::ipc::Channel::new(|_| Ok(())));
        let report = tokio::time::timeout(
            Duration::from_secs(30),
            run_workflow_inner(&deps, &request, sink),
        )
        .await??;
        tokio::time::timeout(Duration::from_secs(30), pump).await??;
        assert_eq!(report.steps.len(), 2);
        Ok(())
    }
}

#[derive(Clone)]
struct Recorder {
    vendor: &'static str,
    project: PathBuf,
    seen: Arc<Mutex<Vec<String>>>,
    mutate: bool,
}

#[async_trait]
impl AgentDriver for Recorder {
    fn id(&self) -> &'static str {
        self.vendor
    }
    fn with_evidence(
        &self,
        _target: loadout_lib::evidence::EvidenceTarget,
    ) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }
    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: None,
        })
    }
    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        assert!(
            !spec
                .system_append
                .as_deref()
                .unwrap_or_default()
                .contains(ROOT_RULE),
            "source content was placed in the system/argv append channel"
        );
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(spec.prompt);
        if self.mutate {
            fs::write(self.project.join("AGENTS.md"), LATER_RULE)?;
        }
        Ok(Box::new(Turn {
            events,
            seen: Arc::clone(&self.seen),
            session: SessionRef {
                vendor: self.vendor,
                id: spec.run_id.to_string(),
            },
        }))
    }
}

struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    seen: Arc<Mutex<Vec<String>>>,
    session: SessionRef,
}

impl Turn {
    fn outcome(&self) -> Outcome {
        Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "Read the instructions.".to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session.clone(),
        }
    }
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }
    fn group(&self) -> Option<GroupId> {
        None
    }
    async fn send(&mut self, text: String) -> anyhow::Result<()> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(text);
        self.events
            .send(AgentEvent::Finished(self.outcome()).into())
            .await?;
        Ok(())
    }
    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        let outcome = self.outcome();
        self.events
            .send(AgentEvent::Finished(outcome.clone()).into())
            .await?;
        Ok(outcome)
    }
    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }
    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}
