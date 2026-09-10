//! WF-16: output jest danymi stdin, nie interpolacją do powłoki ani wspólnym indeksem.

use std::error::Error;
use std::fs;
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::line_channel;
use loadout_lib::library::agents::Agent;
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tokio::sync::mpsc;

const SENTINEL: &str = "$(touch input-was-executed) `touch input-was-executed` \"quoted\"";

#[tokio::test]
async fn the_check_reads_exactly_its_predecessors_output_as_json_data() -> Result<(), Box<dyn Error>>
{
    run_bound_check("fresh-copy").await
}

#[tokio::test]
async fn freezing_a_project_result_does_not_import_unselected_private_inputs()
-> Result<(), Box<dyn Error>> {
    run_bound_check("project").await
}

async fn run_bound_check(folder: &str) -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let project = temp.path().join("project");
    let library = temp.path().join("library");
    fs::create_dir_all(project.join(".loadout"))?;
    fs::create_dir_all(library.join("agents"))?;
    fs::write(
        project.join(".env"),
        "PRIVATE_INPUT_NOT_SELECTED=local-only",
    )?;
    let mut agent = Agent::example();
    agent.skills.clear();
    agent.connections.clear();
    loadout_lib::commands::agents::save_agent_inner(&library, &agent, None)?;
    let executable = std::env::current_exe()?;
    let command = format!(
        "{} --ignored --exact check_reads_only_its_bound_input::check_input_fixture --nocapture",
        shell_word(&executable.to_string_lossy())
    );
    let workflow = json!({
        "format":1,"id":"wf_check_input","name":"Read the selected result",
        "steps":[
            {"kind":"agent","id":"s_wanted","name":"Wanted result","agent":agent.id.to_string(),"instructions":"wanted",
             "folder":{"use":folder},"skills":[],"handover":"notes","at":{"x":0,"y":0}},
            {"kind":"agent","id":"s_other","name":"Other result","agent":agent.id.to_string(),"instructions":"other",
             "folder":{"use":"fresh-copy"},"skills":[],"handover":"notes","at":{"x":240,"y":0}},
            {"kind":"agent","id":"s_mutate","name":"Later edit","agent":agent.id.to_string(),"instructions":"modify-saved-file",
             "folder":{"use":"same-copy"},"skills":[],"handover":"notes","at":{"x":0,"y":96}},
            {"kind":"check","id":"s_check","name":"Read selected input","command":command,"proof":"bound input: (\\d+) passed",
             "folder":{"use":"same-copy"},"at":{"x":0,"y":192}}
        ],
        "links":[{"from":"s_wanted","to":"s_mutate"},{"from":"s_mutate","to":"s_check"}],
        "executionInputs":{"schema":1,"checks":{"s_check":{"results":["s_wanted"]}}}
    });
    let path = project.join("workflow.json");
    fs::write(&path, serde_json::to_vec_pretty(&workflow)?)?;
    let store = Store::open(&project.join(".loadout/loadout.db"))?;
    let drivers: Drivers = Arc::new(|_| Arc::new(Fixture));
    let deps = RunDeps {
        home: &library,
        library: library.clone(),
        project: &project,
        store: &store,
        drivers,
        control: RunControl::new(),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
    };
    let (sink, _source) = line_channel(1024);
    let report = loadout_lib::commands::run::run_workflow_inner(
        &deps,
        &RunRequest {
            workflow: path,
            how_many_at_once: 2,
            task: None,
            part: None,
            handoffs_from: None,
        },
        sink,
    )
    .await?;
    let run: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    let check = run["steps"]
        .as_array()
        .ok_or("missing steps")?
        .iter()
        .find(|step| step["node_key"] == "s_check")
        .ok_or("missing check")?;
    assert_eq!(
        check["status"], "succeeded",
        "the real Check did not receive its bound JSON input: {check}"
    );
    assert!(!project.join("input-was-executed").exists());
    assert!(
        !report.dir.join("work/s_wanted/input-was-executed").exists(),
        "output was interpreted by the shell"
    );
    assert_eq!(check["death_proof"], true);
    Ok(())
}

#[test]
#[ignore = "real Check child fixture; invoked only by the workflow above"]
fn check_input_fixture() -> Result<(), Box<dyn Error>> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(256 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    assert!(bytes.len() <= 256 * 1024);
    let input: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(input["format"], 1);
    let results = input["results"].as_array().ok_or("missing results")?;
    assert_eq!(
        results.len(),
        1,
        "a sibling result leaked into the bound input"
    );
    assert_eq!(results[0]["nodeKey"], "s_wanted");
    assert_eq!(results[0]["status"], "succeeded");
    assert_eq!(
        results[0]["cause"], "completed",
        "the input omits the structured reason this producer ended"
    );
    assert!(
        results[0]["output"]
            .as_str()
            .is_some_and(|text| text.contains(SENTINEL))
    );
    let root = results[0]["files"]["root"]
        .as_str()
        .ok_or("missing frozen producer files")?;
    assert_eq!(
        fs::read_to_string(std::path::Path::new(root).join("producer.txt"))?,
        "wanted file"
    );
    assert!(
        !std::path::Path::new(root).join(".env").exists(),
        "freezing a result copied private host input without selection"
    );
    println!("bound input: 1 passed");
    Ok(())
}

fn shell_word(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

struct Fixture;
#[async_trait]
impl AgentDriver for Fixture {
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
        let marker = if spec.prompt.contains("wanted") {
            SENTINEL
        } else {
            "OTHER_SCOPE_SECRET"
        };
        fs::write(
            spec.cwd.join("producer.txt"),
            if spec.prompt.contains("modify-saved-file") {
                "changed after publication"
            } else if spec.prompt.contains("wanted") {
                "wanted file"
            } else {
                "other file"
            },
        )?;
        Ok(Box::new(Turn {
            events,
            text: format!("Answer: {marker}\nEvidence: fixture\nOpen: none"),
            session: SessionRef {
                vendor: "claude-code",
                id: spec.run_id.to_string(),
            },
        }))
    }
}
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    text: String,
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
        self.events
            .send(
                AgentEvent::Said {
                    text: self.text.clone(),
                }
                .into(),
            )
            .await?;
        let end = Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text: self.text.clone(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session.clone(),
        };
        self.events
            .send(AgentEvent::Finished(end.clone()).into())
            .await?;
        Ok(end)
    }
    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }
    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}
