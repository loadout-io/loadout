//! WF-23: źródła notatek jadą przez zwykły Start/replay, bez rekonstrukcji z dzisiejszej biblioteki.
//! Dubler mierzy `RunSpec`, nie składa promptu ani nie odtwarza selekcji pamięci.

#![allow(clippy::too_many_lines)]
#![allow(clippy::assigning_clones)]

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{AppState, line_channel};
use loadout_lib::library::agents::{Agent, write_agent_file};
use loadout_lib::store::Store;
use serde_json::{Value, json};
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::mpsc;

#[tokio::test]
async fn recorded_memory_reaches_each_physical_copy_after_sources_are_changed_or_deleted()
-> Result<(), Box<dyn Error>> {
    recorded_memory_case(false).await
}

#[tokio::test]
async fn a_run_from_the_shared_library_replays_matching_note_ids_after_project_isolation()
-> Result<(), Box<dyn Error>> {
    recorded_memory_case(true).await
}

async fn recorded_memory_case(legacy: bool) -> Result<(), Box<dyn Error>> {
    let bench = Bench::with_legacy_library(legacy)?;
    let global = bench.note(
        if legacy { "same" } else { "global" },
        ("everywhere", None),
        "ORIGINAL-GLOBAL",
        "in-use",
        "",
    )?;
    let project = bench.note(
        if legacy { "same" } else { "project" },
        ("this-project", None),
        "ORIGINAL-PROJECT",
        "in-use",
        "",
    )?;
    let alpha = bench.note(
        "alpha",
        ("this-agent", Some("Alpha")),
        "ALPHA-PRIVATE",
        "in-use",
        "",
    )?;
    let beta = bench.note(
        "beta",
        ("this-agent", Some("Beta")),
        "BETA-PRIVATE",
        "in-use",
        "",
    )?;
    bench.note(
        "suggested",
        ("everywhere", None),
        "SUGGESTED-MUST-NOT-LEAK",
        "suggested",
        "",
    )?;
    let source = bench.run().await?;
    let observed = bench.prompts();
    assert_eq!(observed.len(), 3);
    assert_scopes(&observed)?;
    let source_run = fs::read(source.dir.join("run.json"))?;
    let original_input = fs::read(source.dir.join("input/manifest.json"))?;
    let source_file: Value = serde_json::from_slice(&source_run)?;
    assert!(
        !source_file["memory"]
            .as_array()
            .ok_or("no memory receipt")?
            .is_empty()
    );

    // Nowe bajty mają zostać dokładnie nowe; Recorded nie może nawet dopisać do nich stempla.
    fs::write(
        &global,
        note_text(
            "everywhere",
            None,
            "CURRENT-MUST-NOT-LEAK",
            "in-use",
            "manual revision after the run",
        ),
    )?;
    let current_global = fs::read(&global)?;
    fs::remove_file(&project)?;
    fs::remove_file(&alpha)?;
    fs::remove_file(&beta)?;

    let folder = bench.project.to_str().ok_or("fixture path is not text")?;
    let preview = bench
        .state
        .prepare_replay_inner(folder, &source.id, json!({"kind":"all"}), "recorded")
        .await;
    assert!(
        preview.is_ok(),
        "saved source notes must be replayable without the library files: {preview:?}"
    );
    let preview = preview?;
    assert_eq!(preview["copies"], 3);
    let preview_id = preview["previewId"]
        .as_str()
        .ok_or("no replay preview ID")?;
    let request = bench
        .state
        .authorize_replay_inner(folder, preview_id, "Start replay".to_owned())
        .await?;
    let loadout_lib::engine::line::Line::RunRequested { request_id, .. } = request else {
        return Err("replay bypassed the normal Start request".into());
    };
    let (sink, _events) = line_channel(512);
    let replay = bench
        .state
        .accept_lead_start_inner(&request_id, 2, Some(8.0), false, sink)
        .await?;
    assert_ne!(replay.run.run_id, source.id);
    let observed = bench.prompts();
    assert_eq!(observed.len(), 6);
    assert_scopes(&observed[3..])?;
    assert_eq!(
        fs::read(&global)?,
        current_global,
        "Recorded stamped or replaced a current source note"
    );
    for deleted in [&project, &alpha, &beta] {
        assert!(!deleted.exists(), "Recorded recreated a deleted live note");
    }
    assert_eq!(
        fs::read(source.dir.join("run.json"))?,
        source_run,
        "Recorded changed its source receipt"
    );
    assert_eq!(
        fs::read(source.dir.join("input/manifest.json"))?,
        original_input,
        "Recorded changed the frozen project input"
    );
    bench.state.close_everything_down().await;
    Ok(())
}

#[tokio::test]
async fn source_note_budget_refuses_before_any_agent_starts() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.note(
        "large",
        ("everywhere", None),
        "a short accepted rule",
        "in-use",
        &"x".repeat(512 * 1024 + 1),
    )?;
    let report = bench.run().await;
    assert!(
        report.is_err(),
        "the complete source note exceeded 512 KiB but agents started: {report:?}"
    );
    let error = report.err().ok_or("asserted refusal")?.to_string();
    assert!(
        error.to_lowercase().contains("memory") && error.contains("512"),
        "the refusal must explain the source limit: {error}"
    );
    assert!(
        bench.prompts().is_empty(),
        "a refused source package still started a driver"
    );
    bench.state.close_everything_down().await;
    Ok(())
}

fn assert_scopes(prompts: &[String]) -> Result<(), Box<dyn Error>> {
    let mut alpha = 0;
    let mut beta = 0;
    for prompt in prompts {
        assert!(
            prompt.contains("ORIGINAL-GLOBAL") && prompt.contains("ORIGINAL-PROJECT"),
            "shared source notes did not reach a real RunSpec: {prompt}"
        );
        assert!(
            !prompt.contains("CURRENT-MUST-NOT-LEAK")
                && !prompt.contains("SUGGESTED-MUST-NOT-LEAK")
        );
        if prompt.contains("WF23-ALPHA") {
            alpha += 1;
            assert!(
                prompt.contains("ALPHA-PRIVATE") && !prompt.contains("BETA-PRIVATE"),
                "another agent's source leaked into Alpha"
            );
        } else if prompt.contains("WF23-BETA") {
            beta += 1;
            assert!(
                prompt.contains("BETA-PRIVATE") && !prompt.contains("ALPHA-PRIVATE"),
                "another agent's source leaked into Beta"
            );
        } else {
            return Err("unknown physical copy in the observation".into());
        }
    }
    assert_eq!((alpha, beta), (2, 1));
    Ok(())
}

fn note_text(scope: &str, agent: Option<&str>, rule: &str, status: &str, body: &str) -> String {
    let owner = agent
        .map(|agent| format!("agent: {agent}\n"))
        .unwrap_or_default();
    format!(
        "---\nscope: {scope}\n{owner}kind: fact\ntitle: Source for replay\nrule: {rule}\nbecause: Accepted by the person\nstatus: {status}\noccurrences: 1\nmodified: 2026-09-06T00:00:00Z\nlast_used_at: null\n---\n{body}"
    )
}

struct Bench {
    _root: TempDir,
    project: PathBuf,
    library: PathBuf,
    workflow: PathBuf,
    state: Arc<AppState>,
    seen: Arc<Mutex<Vec<String>>>,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        Self::with_legacy_library(false)
    }
    fn with_legacy_library(legacy: bool) -> Result<Self, Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let home = root.path().join("home");
        let project = root.path().join("project");
        fs::create_dir_all(project.join(".loadout/workflows"))?;
        fs::create_dir_all(project.join(".loadout"))?;
        fs::write(project.join("seed.txt"), "original project input")?;
        let mut alpha = Agent::example();
        alpha.name = "Alpha".to_owned();
        let mut beta = Agent::example();
        beta.id = uuid::Uuid::now_v7();
        beta.name = "Beta".to_owned();
        write_agent_file(&project.join(".loadout/agents"), &alpha, None)?;
        write_agent_file(&project.join(".loadout/agents"), &beta, None)?;
        let library = if legacy {
            home.clone()
        } else {
            project.join(".loadout")
        };
        if legacy {
            // Simulate a pre-isolation run; today's project independently owns both agents.
            write_agent_file(&library.join("agents"), &alpha, None)?;
            write_agent_file(&library.join("agents"), &beta, None)?;
        }
        let workflow = project.join(".loadout/workflows/memory-replay.json");
        fs::write(&workflow, json!({"format":1,"id":"memory-replay","name":"Replay selected source notes","links":[],"steps":[
            {"kind":"agent","id":"alpha","name":"Alpha copies","agent":alpha.id,"copies":2,"instructions":"WF23-ALPHA copy {{copy}}","folder":{"use":"fresh-copy"},"at":{"x":0,"y":0}},
            {"kind":"agent","id":"beta","name":"Beta copy","agent":beta.id,"instructions":"WF23-BETA","folder":{"use":"fresh-copy"},"at":{"x":0,"y":0}}
        ]}).to_string())?;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let driver: Arc<dyn AgentDriver> = Arc::new(Reader {
            seen: Arc::clone(&seen),
        });
        let drivers: Drivers = Arc::new(move |_| Arc::clone(&driver));
        let state = Arc::new(AppState::new(
            home.clone(),
            project.clone(),
            Store::open(&project.join(".loadout/loadout.db"))?,
            drivers,
        ));
        Ok(Self {
            _root: root,
            project,
            library,
            workflow,
            state,
            seen,
        })
    }
    fn note(
        &self,
        name: &str,
        scope: (&str, Option<&str>),
        rule: &str,
        status: &str,
        body: &str,
    ) -> Result<PathBuf, Box<dyn Error>> {
        let root = if scope.0 == "this-project" {
            loadout_lib::commands::memory::project_notes_root(&self.project)
        } else {
            loadout_lib::commands::memory::notes_root(&self.library)
        };
        let path = root.join("notes").join(format!("{name}.md"));
        fs::create_dir_all(path.parent().ok_or("no note parent")?)?;
        fs::write(&path, note_text(scope.0, scope.1, rule, status, body))?;
        Ok(path)
    }
    fn prompts(&self) -> Vec<String> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
    async fn run(&self) -> Result<loadout_lib::commands::RunReport, Box<dyn Error>> {
        let mut deps = self.state.begin_run(&self.project)?;
        deps.library.clone_from(&self.library);
        let (sink, _events) = line_channel(512);
        Ok(tokio::time::timeout(
            Duration::from_secs(30),
            run_workflow_inner(
                &deps,
                &RunRequest {
                    workflow: self.workflow.clone(),
                    how_many_at_once: 2,
                    task: None,
                    part: None,
                    handoffs_from: None,
                },
                sink,
            ),
        )
        .await??)
    }
}

struct Reader {
    seen: Arc<Mutex<Vec<String>>>,
}
#[async_trait]
impl AgentDriver for Reader {
    fn id(&self) -> &'static str {
        "claude-code"
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
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(spec.prompt);
        Ok(Box::new(Turn {
            events,
            session: SessionRef {
                vendor: self.id(),
                id: spec.run_id.to_string(),
            },
        }))
    }
}
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
}
#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }
    fn group(&self) -> Option<GroupId> {
        None
    }
    async fn send(&mut self, _: String) -> anyhow::Result<()> {
        Ok(())
    }
    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        let outcome = Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "Read the source notes.".to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session(),
        };
        let _ = self
            .events
            .send(AgentEvent::Finished(outcome.clone()).into())
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
