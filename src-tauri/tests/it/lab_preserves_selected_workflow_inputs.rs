//! WF-17/14: prawdziwy Lab planner i runner, plik czytany w roboczej kopii agenta.
use std::{
    error::Error,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use loadout_lib::{
    commands::{self, Drivers, RunControl, RunDeps},
    engine::{
        drivers::{
            AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe,
            RunSpec, SessionRef, Tokens,
        },
        supervisor::{GroupId, GroupProof},
    },
    ipc::line_channel,
    library::agents::Agent,
    store::Store,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

const SELECTED: &str = "the explicitly selected original fixture";

#[tokio::test]
async fn both_real_workflow_columns_receive_their_common_selected_untracked_file()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.define(
        [
            json!(["fixtures/local.json"]),
            json!(["fixtures/local.json"]),
        ],
        None,
        false,
    )?;
    let report = bench.run().await?;
    let seen = bench.seen.lock().map_err(|_| "observations poisoned")?;
    assert_eq!(seen.len(), 2, "the actual workflow workers did not run");
    for one in seen.iter() {
        assert_eq!(
            one.selected.as_deref(),
            Some(SELECTED),
            "the Lab compiler dropped the chosen untracked input from the actual worker's cwd"
        );
        assert_eq!(one.tracked.as_deref(), Some("tracked WIP"));
        assert!(
            !one.secret,
            "the Lab broadened selected inputs to an unselected .env"
        );
    }
    let board = commands::lab::read_board_inner(&bench.project, "input-comparison", 3)?;
    let run = board.runs.first().ok_or("missing completed comparison")?;
    assert_eq!(
        (run.passed, run.judged),
        (2, 2),
        "real independent checks did not read both frozen results: {}",
        fs::read_to_string(report.dir.join("run.json"))?
    );
    Ok(())
}

#[test]
fn different_host_selections_refuse_in_the_actual_planner_instead_of_silently_unioning()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.define(
        [json!(["fixtures/local.json"]), json!([".env"])],
        None,
        false,
    )?;
    let answer =
        commands::lab::plan_a_run_with_library(&bench.home, &bench.project, "input-comparison", 2);
    assert!(
        answer.is_err(),
        "different explicit selections were silently discarded or combined by the actual Lab planner"
    );
    let said = answer.err().ok_or("expected refusal")?.to_string();
    assert!(
        said.contains("saved") && said.contains("input"),
        "the refusal does not explain the explicit saved-input alternative: {said}"
    );
    assert!(
        bench
            .seen
            .lock()
            .map_err(|_| "observations poisoned")?
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn an_explicit_saved_case_input_supersedes_host_selections_without_recapturing_todays_files()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let id = uuid::Uuid::now_v7().to_string();
    let dir = bench
        .project
        .join(".loadout/runs")
        .join(format!("20260906-030000__{id}"));
    fs::create_dir_all(&dir)?;
    let input = commands::input_snapshot::capture_selected(
        &bench.project,
        &dir,
        &["fixtures/*.json".to_owned()],
    )?;
    fs::write(
        dir.join("run.json"),
        serde_json::to_vec(
            &json!({"id":id,"title":"Shared starting files","status":"succeeded","steps":[],
        "input_snapshot":{"id":input.id(),"manifest":"input/manifest.json"}}),
        )?,
    )?;
    bench.define(
        [
            json!(["fixtures/local.json"]),
            json!(["fixtures/other.json"]),
        ],
        Some(json!({"sourceRunId":id,"snapshotId":input.id()})),
        false,
    )?;
    // Te wzorce nie mają już dopasowań na gospodarzu. Seed jest świadomym wyborem bajtów.
    fs::remove_file(bench.project.join("fixtures/local.json"))?;
    fs::remove_file(bench.project.join("fixtures/other.json"))?;
    fs::write(
        bench.project.join("tracked.txt"),
        "today is not the saved input",
    )?;
    bench.run().await?;
    let seen = bench.seen.lock().map_err(|_| "observations poisoned")?;
    assert_eq!(seen.len(), 2);
    for one in seen.iter() {
        assert_eq!(one.selected.as_deref(), Some(SELECTED));
        assert_eq!(one.tracked.as_deref(), Some("tracked WIP"));
        assert!(!one.secret);
    }
    assert_eq!(
        fs::read_to_string(input.files().join("fixtures/local.json"))?,
        SELECTED
    );
    Ok(())
}

#[test]
fn a_source_workflows_required_file_restrictions_are_not_downgraded_by_a_diagnostic_set()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.define([json!([]), json!([])], None, true)?;
    let answer =
        commands::lab::plan_a_run_with_library(&bench.home, &bench.project, "input-comparison", 2);
    assert!(
        answer.is_err(),
        "Lab silently removed executionInputs.isolateContexts from its source workflow"
    );
    let said = answer.err().ok_or("expected refusal")?.to_string();
    assert!(
        said.to_lowercase().contains("protect") || said.contains("restrict"),
        "the refusal does not name the required protection: {said}"
    );
    Ok(())
}

#[test]
#[ignore = "the real child process of the production Check"]
fn external_file_reader() -> Result<(), Box<dyn Error>> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(256 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    let input: Value = serde_json::from_slice(&bytes)?;
    let root = Path::new(
        input["results"][0]["files"]["root"]
            .as_str()
            .ok_or("missing result files")?,
    );
    assert_eq!(
        fs::read_to_string(root.join("fixtures/local.json"))?,
        SELECTED
    );
    println!("selected input: 1 passed");
    Ok(())
}

struct Bench {
    _temp: tempfile::TempDir,
    home: PathBuf,
    project: PathBuf,
    agent: Agent,
    seen: Arc<Mutex<Vec<Observed>>>,
}
impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let temp = tempfile::tempdir()?;
        let home = temp.path().join("home");
        let project = temp.path().join("project");
        fs::create_dir_all(home.join("agents"))?;
        fs::create_dir_all(project.join(".loadout/workflows"))?;
        fs::create_dir_all(project.join(".loadout/evals"))?;
        fs::create_dir_all(project.join("fixtures"))?;
        fs::write(project.join("tracked.txt"), "committed")?;
        fs::write(project.join(".gitignore"), ".loadout/\n.env\n")?;
        git(&project, &["init", "--quiet"])?;
        git(&project, &["add", "tracked.txt", ".gitignore"])?;
        git(
            &project,
            &[
                "-c",
                "user.name=Lab input fixture",
                "-c",
                "user.email=lab@example.invalid",
                "commit",
                "--quiet",
                "-m",
                "input",
            ],
        )?;
        fs::write(project.join("tracked.txt"), "tracked WIP")?;
        fs::write(project.join("fixtures/local.json"), SELECTED)?;
        fs::write(
            project.join("fixtures/other.json"),
            "another explicitly saved input",
        )?;
        fs::write(project.join(".env"), "PRIVATE_NOT_SELECTED=1")?;
        let mut agent = Agent::example();
        agent.skills.clear();
        agent.connections.clear();
        commands::agents::save_agent_inner(&home, &agent, None)?;
        Ok(Self {
            _temp: temp,
            home,
            project,
            agent,
            seen: Arc::new(Mutex::new(Vec::new())),
        })
    }
    fn define(
        &self,
        selections: [Value; 2],
        input: Option<Value>,
        protected_source: bool,
    ) -> Result<(), Box<dyn Error>> {
        for (id, additional) in ["a", "b"].into_iter().zip(selections) {
            let mut graph = json!({"format":1,"id":id,"name":id,"additionalInputs":additional,
                "steps":[{"kind":"agent","id":"out","name":"Read selected input","agent":self.agent.id.to_string(),
                    "instructions":"Read the selected local fixture for {{task}}","folder":{"use":"fresh-copy"},"skills":[],"at":{"x":0,"y":0}}],"links":[]});
            if protected_source {
                graph["executionInputs"] = json!({"schema":1,"isolateContexts":true});
            }
            fs::write(
                self.project.join(format!(".loadout/workflows/{id}.json")),
                serde_json::to_vec(&graph)?,
            )?;
        }
        let executable = std::env::current_exe()?;
        let command = format!(
            "'{}' --ignored --exact lab_preserves_selected_workflow_inputs::external_file_reader --nocapture",
            executable.to_string_lossy().replace('\'', "'\\''")
        );
        let mut case = json!({"id":"case","name":"Case","task":"Read the fixture","status":"in-use","command":command,"proof":"selected input: (\\d+) passed"});
        if let Some(input) = input {
            case["input"] = input;
        }
        fs::write(
            self.project.join(".loadout/evals/input-comparison.json"),
            serde_json::to_vec(&json!({
            "format":2,"id":"input-comparison","name":"Input comparison","subject":{"kind":"workflow","id":"a"},"protected":false,
            "cases":[case],"variants":[
                {"id":"a","name":"A","workflow":{"id":"a","outputStep":"out"}},
                {"id":"b","name":"B","workflow":{"id":"b","outputStep":"out"}}]}))?,
        )?;
        Ok(())
    }
    async fn run(&self) -> Result<commands::RunReport, Box<dyn Error>> {
        let plan = commands::lab::plan_a_run_with_library(
            &self.home,
            &self.project,
            "input-comparison",
            2,
        )?;
        let seen = self.seen.clone();
        let drivers: Drivers = Arc::new(move |_| Arc::new(Reader { seen: seen.clone() }));
        let store = Store::open(&self.project.join(".loadout/loadout.db"))?;
        let deps = RunDeps {
            home: &self.home,
            project: &self.project,
            store: &store,
            drivers,
            control: RunControl::new(),
            processes: Arc::new(commands::processes::Processes::new()),
        };
        let (sink, _source) = line_channel(4096);
        Ok(tokio::time::timeout(
            Duration::from_secs(20),
            commands::run::run_workflow_inner(&deps, &plan.request, sink),
        )
        .await??)
    }
}
fn git(project: &Path, args: &[&str]) -> Result<(), Box<dyn Error>> {
    let result = Command::new("git")
        .current_dir(project)
        .args(args)
        .output()?;
    if !result.status.success() {
        return Err(String::from_utf8_lossy(&result.stderr).into_owned().into());
    }
    Ok(())
}
struct Observed {
    selected: Option<String>,
    tracked: Option<String>,
    secret: bool,
}
struct Reader {
    seen: Arc<Mutex<Vec<Observed>>>,
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
            .map_err(|_| anyhow::anyhow!("observations poisoned"))?
            .push(Observed {
                selected: fs::read_to_string(spec.cwd.join("fixtures/local.json")).ok(),
                tracked: fs::read_to_string(spec.cwd.join("tracked.txt")).ok(),
                secret: spec.cwd.join(".env").exists(),
            });
        Ok(Box::new(Turn {
            events,
            session: SessionRef {
                vendor: "claude-code",
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
        let text = "Answer: read the selected files\nEvidence: working copy\nOpen: none".to_owned();
        self.events
            .send(AgentEvent::Said { text: text.clone() }.into())
            .await?;
        let result = Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text,
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session.clone(),
        };
        self.events
            .send(AgentEvent::Finished(result.clone()).into())
            .await?;
        Ok(result)
    }
    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }
    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}
