//! P-01: zależność od sąsiedniego repozytorium nazwana, zanim ruszy pierwszy proces.
//!
//! Incydent I-08 (bieg z 2026-09-06): krok pracował we własnej kopii projektu, a `Cargo.toml`
//! tego projektu zależał od `../murmur-server`. Kopia leży w `.loadout/runs/<bieg>/work/<krok>`,
//! więc `../murmur-server` wskazuje tam katalog, którego nie ma i nie będzie. Agent dostał
//! wyłącznie błąd Cargo — prawdziwy i milczący o kopii — i palił tury na ratowanie środowiska.

#![allow(clippy::panic)]

use std::error::Error;
use std::fs;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens, Voice,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{AppState, line_channel};
use loadout_lib::library::agents::{Agent, write_agent_file};
use loadout_lib::store::Store;
use serde_json::json;
use tokio::sync::mpsc;

/// I-08 wprost: krok we własnej kopii, projekt zależny od katalogu wyżej.
#[tokio::test]
async fn a_step_in_its_own_copy_hears_which_folder_will_not_be_there()
-> Result<(), Box<dyn Error>> {
    let said = complaints("fresh-copy", CARGO_WITH_A_NEIGHBOUR).await?;
    let about = said
        .iter()
        .find(|text| text.contains("its own copy"))
        .ok_or("the run never said a neighbouring folder will be missing")?;
    assert!(
        about.contains("../murmur-server"),
        "the sentence does not name the folder that will be missing: {about:?}"
    );
    assert!(
        about.contains("src-tauri/Cargo.toml"),
        "the sentence does not name the file that asks for it: {about:?}"
    );
    assert!(
        about.contains("run this step in the project folder"),
        "the sentence names the gap without naming a way out of it: {about:?}"
    );
    Ok(())
}

/// Krok pracujący w folderze projektu widzi sąsiada normalnie i nie ma o czym słuchać.
#[tokio::test]
async fn a_step_in_the_project_folder_is_told_nothing() -> Result<(), Box<dyn Error>> {
    let said = complaints("project", CARGO_WITH_A_NEIGHBOUR).await?;
    assert!(
        !said.iter().any(|text| text.contains("its own copy")),
        "a step that works where the neighbour really is was warned about it anyway: {said:?}"
    );
    Ok(())
}

/// Zależność ścieżkowa, która zostaje w projekcie, nie jest brakiem.
#[tokio::test]
async fn a_path_dependency_inside_the_project_is_not_a_gap() -> Result<(), Box<dyn Error>> {
    let said = complaints("fresh-copy", CARGO_WITH_AN_INSIDE_PATH).await?;
    assert!(
        !said.iter().any(|text| text.contains("its own copy")),
        "a dependency that stays inside the project was reported as missing: {said:?}"
    );
    Ok(())
}

/// Ten sam fakt dla runnera pakietów węzłowych: `file:../` wychodzi z projektu tak samo.
#[tokio::test]
async fn a_node_file_dependency_outside_the_project_counts_too() -> Result<(), Box<dyn Error>> {
    let said = complaints("fresh-copy", NPM_WITH_A_NEIGHBOUR).await?;
    let about = said
        .iter()
        .find(|text| text.contains("its own copy"))
        .ok_or("a file: dependency pointing outside the project was not reported")?;
    assert!(
        about.contains("../shared-ui") && about.contains("package.json"),
        "the sentence does not name the node dependency that will be missing: {about:?}"
    );
    Ok(())
}

const CARGO_WITH_A_NEIGHBOUR: Manifest = Manifest {
    at: "src-tauri/Cargo.toml",
    text: "[package]\nname = \"app\"\n\n[dependencies]\nmurmur-server = { path = \"../murmur-server\" }\n",
};

const CARGO_WITH_AN_INSIDE_PATH: Manifest = Manifest {
    at: "src-tauri/Cargo.toml",
    text: "[package]\nname = \"app\"\n\n[dependencies]\nhelper = { path = \"helper\" }\n",
};

const NPM_WITH_A_NEIGHBOUR: Manifest = Manifest {
    at: "package.json",
    text: "{\"name\":\"app\",\"dependencies\":{\"shared-ui\":\"file:../shared-ui\"}}",
};

#[derive(Clone, Copy)]
struct Manifest {
    at: &'static str,
    text: &'static str,
}

/// Uruchamia prawdziwy bieg jednego kroku i oddaje zdania, które bieg pokazał człowiekowi.
async fn complaints(folder: &str, manifest: Manifest) -> Result<Vec<String>, Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let project = root.path().canonicalize()?.join("project");
    let home = root.path().canonicalize()?.join("home");
    fs::create_dir_all(project.join(".loadout"))?;
    fs::create_dir_all(home.join("workflows"))?;
    let at = project.join(manifest.at);
    if let Some(parent) = at.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&at, manifest.text)?;
    let agent = Agent::example();
    write_agent_file(&home.join("agents"), &agent, None)?;
    let workflow = home.join("workflows/build.json");
    fs::write(
        &workflow,
        json!({"format":1,"id":"wf-neighbour","name":"Neighbour","steps":[
            {"kind":"agent","id":"build","name":"Builder","agent":agent.id,
             "instructions":"Build it.","overrides":{},"folder":{"use":folder},
             "at":{"x":0,"y":0}}],"links":[]})
        .to_string(),
    )?;
    let driver: Arc<dyn AgentDriver> = Arc::new(Driver);
    let drivers: Drivers = Arc::new(move |_| Arc::clone(&driver));
    let state = AppState::new(
        home,
        project.clone(),
        Store::open(&project.join(".loadout/loadout.db"))?,
        drivers,
    );
    let deps = state.begin_run(&project)?;
    let (sink, mut output) = line_channel(512);
    tokio::time::timeout(
        Duration::from_secs(30),
        run_workflow_inner(
            &deps,
            &RunRequest {
                workflow,
                how_many_at_once: 1,
                task: None,
                part: None,
                handoffs_from: None,
            },
            sink,
        ),
    )
    .await??;
    let mut said = Vec::new();
    while let Some(line) = output.try_next() {
        if let Line::Problem { text, .. } = line {
            said.push(text);
        }
    }
    Ok(said)
}

struct Driver;

#[async_trait]
impl AgentDriver for Driver {
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
        Ok(Box::new(Handle {
            events,
            session: SessionRef {
                vendor: "claude-code",
                id: spec.run_id.to_string(),
            },
        }))
    }
}

struct Handle {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for Handle {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }
    fn group(&self) -> Option<GroupId> {
        None
    }
    fn voice(&self) -> Option<Voice> {
        None
    }
    async fn send(&mut self, _: String) -> anyhow::Result<()> {
        anyhow::bail!("this double takes no extra turns")
    }
    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        let result = Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "## Answer\nBuilt.\n\n## Evidence\nnone\n\n## Open\nNone.\n".to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send(AgentEvent::Finished(result.clone()).into())
            .await;
        Ok(result)
    }
    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }
    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}
