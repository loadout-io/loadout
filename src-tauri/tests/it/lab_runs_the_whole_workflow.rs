//! WF-17: rzeczywisty wykonawca, równoległe sesje, pętla i zewnętrzny Check stdin.

#![allow(clippy::too_many_lines)]
#![allow(clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::{Drivers, RunControl, RunDeps};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::GroupProof;
use loadout_lib::ipc::line_channel;
use loadout_lib::library::agents::Agent;
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tokio::sync::{Barrier, mpsc};

#[test]
fn a_workflow_column_uses_its_selected_library_file_and_refuses_a_changed_revision()
-> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let project = temp.path().join("project");
    let home = temp.path().join("library");
    fs::create_dir_all(home.join("workflows"))?;
    fs::create_dir_all(project.join(".loadout/workflows"))?;
    fs::create_dir_all(project.join(".loadout/evals"))?;
    let workflow = |name: &str| {
        json!({"format":1,"id":"shared-id","name":name,"steps":[{
        "kind":"check","id":"out","name":name,"command":"printf '1 passed'","proof":"(\\d+) passed",
        "folder":{"use":"fresh-copy"},"at":{"x":0,"y":0}}],"links":[]})
    };
    fs::write(
        home.join("workflows/shared.json"),
        serde_json::to_vec(&workflow("Library version"))?,
    )?;
    fs::write(
        project.join(".loadout/workflows/shared.json"),
        serde_json::to_vec(&workflow("Project version"))?,
    )?;
    let original =
        loadout_lib::commands::workflows::load_workflow_inner(&home, None, "shared.json")?;
    let set = json!({"format":2,"id":"source-choice","name":"Source choice","subject":{"kind":"workflow","id":"shared-id"},
        "cases":[{"id":"case","name":"Case","task":"Task","status":"in-use","command":"printf '1 passed'","proof":"(\\d+) passed"}],
        "variants":[{"id":"variant","name":"Variant","workflow":{"id":"shared-id","place":"library","path":"shared.json","revision":original.revision,"outputStep":"out"}}]});
    fs::write(
        project.join(".loadout/evals/source-choice.json"),
        serde_json::to_vec(&set)?,
    )?;
    let planned =
        loadout_lib::commands::lab::plan_a_run_with_library(&home, &project, "source-choice", 2);
    assert!(
        planned.is_ok(),
        "an explicitly selected library workflow did not reach the actual Lab planner: {planned:?}"
    );
    let graph: Value = serde_json::from_slice(&fs::read(planned?.path)?)?;
    assert!(
        graph["steps"]
            .as_array()
            .ok_or("steps missing")?
            .iter()
            .any(|step| step["name"]
                .as_str()
                .is_some_and(|name| name.contains("Library version")))
    );
    assert!(
        !graph["steps"]
            .as_array()
            .ok_or("steps missing")?
            .iter()
            .any(|step| step["name"]
                .as_str()
                .is_some_and(|name| name.contains("Project version")))
    );
    fs::write(
        home.join("workflows/shared.json"),
        serde_json::to_vec(&workflow("Changed version"))?,
    )?;
    assert!(
        loadout_lib::commands::lab::plan_a_run_with_library(&home, &project, "source-choice", 2)
            .is_err(),
        "a pinned revision silently switched to today's graph"
    );
    Ok(())
}

#[test]
fn composing_a_workflow_preserves_its_typed_inputs_and_binds_each_case_seed()
-> Result<(), Box<dyn Error>> {
    let source: loadout_lib::workflow::WorkflowFile = serde_json::from_value(json!({
        "format":1,"id":"source","name":"Source","steps":[
            {"kind":"check","id":"work","name":"Work","command":"printf '1 passed'","proof":"(\\d+) passed","folder":{"use":"project"},"at":{"x":0,"y":0}},
            {"kind":"check","id":"out","name":"Output","command":"printf '1 passed'","proof":"(\\d+) passed","folder":{"use":"fresh-copy"},"at":{"x":0,"y":0}}],
        "links":[{"from":"work","to":"out"}],
        "executionInputs":{"schema":1,"contexts":{"build":{"task":"original","instructions":"keep source rules"},"inspect":{"task":"inspect"}},
            "stepContexts":{"work":"build","out":"inspect"},"checks":{"out":{"results":["work"]}}}
    }))?;
    let source_run = uuid::Uuid::now_v7().to_string();
    let snapshot = uuid::Uuid::now_v7().to_string();
    let set: loadout_lib::lab::EvalSet = serde_json::from_value(json!({
        "format":2,"id":"typed","name":"Typed inputs","subject":{"kind":"workflow","id":"source"},
        "cases":[{"id":"case","name":"Case","task":"CASE","status":"in-use","repeats":2,
            "input":{"sourceRunId":source_run,"snapshotId":snapshot},"command":"printf '1 passed'","proof":"(\\d+) passed"}],
        "variants":[{"id":"variant","name":"Variant","workflow":{"id":"source","outputStep":"out"}}]
    }))?;
    let graph = loadout_lib::lab::workflow_plan::compose(
        &[source],
        &set,
        "eval:typed".into(),
        "Typed".into(),
    )?;
    let inputs = loadout_lib::workflow::execution::RunInputs::from_graph(&graph)?;
    let bindings: Vec<loadout_lib::lab::workflow_plan::CellBinding> =
        serde_json::from_value(graph.extra["cellBindings"].clone())?;
    for binding in bindings {
        let work = &binding.nodes["work"];
        let out = &binding.nodes["out"];
        let input = inputs
            .checks
            .get(out)
            .expect("the source workflow's real check lost its exact CheckInput");
        assert_eq!(input.results, vec![work.clone()]);
        let context = inputs.context_for(work).ok_or("source scope missing")?;
        assert_eq!(context.instructions, "keep source rules");
        assert_eq!(context.task, "CASE");
        let seed =
            &inputs.workspace_seeds[context.workspace_seed.as_ref().ok_or("case seed missing")?];
        assert_eq!(seed.source_run_id, source_run);
        assert_eq!(seed.snapshot_id, snapshot);
        assert_ne!(inputs.context_key(work), inputs.context_key(out));
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_workflow_variant_executes_its_branches_loop_and_output_for_every_repeat()
-> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let project = temp.path().join("project");
    let home = temp.path().join("home");
    fs::create_dir_all(project.join(".loadout/workflows"))?;
    fs::create_dir_all(project.join(".loadout/evals"))?;
    fs::create_dir_all(home.join("agents"))?;
    fs::write(project.join("input.txt"), "same input for every cell")?;
    let mut agent = Agent::example();
    agent.skills.clear();
    agent.connections.clear();
    loadout_lib::commands::agents::save_agent_inner(&home, &agent, None)?;
    let step = |id: &str, folder: &str| {
        json!({"kind":"agent","id":id,"name":id,
        "agent":agent.id.to_string(),"instructions":format!("ROLE_{id} {{{{task}}}}"),
        "folder":{"use":folder},"skills":[],"handover":"notes","at":{"x":0,"y":0}})
    };
    for variant in ["a", "b"] {
        let graph = json!({"format":1,"id":format!("wf_{variant}"),"name":format!("Pipeline {variant}"),
            "steps":[step("left","fresh-copy"),step("right","fresh-copy"),
                {"kind":"check","id":"judge","name":"Judge iteration","command":"test \"$(cat left.txt)\" = 2 && printf '1 passed\\n'",
                 "proof":"(\\d+) passed","folder":{"use":"same-copy"},"whenItFails":"stop","at":{"x":0,"y":0}},
                step("join","same-copy")],
            "links":[{"from":"left","to":"judge"},{"from":"judge","to":"left","max_turns":2},
                {"from":"judge","to":"join"},{"from":"right","to":"join"}]});
        fs::write(
            project.join(format!(".loadout/workflows/{variant}.json")),
            serde_json::to_vec(&graph)?,
        )?;
    }
    let executable = std::env::current_exe()?;
    let command = format!(
        "'{}' --ignored --exact lab_runs_the_whole_workflow::external_check_fixture --nocapture",
        executable.to_string_lossy().replace('\'', "'\\''")
    );
    let set = json!({"format":2,"id":"whole-workflow","name":"Compare whole workflows",
        "subject":{"kind":"workflow","id":"wf_a"},
        "cases":[{"id":"case","name":"One problem","task":"CASE_TASK","status":"in-use","repeats":2,
            "command":command,"proof":"external assessment: (\\d+) passed"}],
        "variants":[{"id":"a","name":"First pipeline","workflow":{"id":"wf_a","outputStep":"join"}},
            {"id":"b","name":"Second pipeline","workflow":{"id":"wf_b","outputStep":"join"}}]});
    fs::write(
        project.join(".loadout/evals/whole-workflow.json"),
        serde_json::to_vec(&set)?,
    )?;
    let planned = loadout_lib::commands::lab::plan_a_run_inner(&project, "whole-workflow", 4);
    assert!(
        planned.is_ok(),
        "a full workflow variant cannot be prepared by the real Lab: {planned:?}"
    );
    let planned = planned?;
    let observed = Arc::new(Mutex::new(Vec::new()));
    let barrier = Arc::new(Barrier::new(2));
    let seen = observed.clone();
    let drivers: Drivers = Arc::new(move |_| {
        Arc::new(Fixture {
            seen: seen.clone(),
            barrier: barrier.clone(),
        })
    });
    let store = Store::open(&project.join(".loadout/loadout.db"))?;
    let deps = RunDeps {
        home: &home,
        project: &project,
        store: &store,
        drivers,
        control: RunControl::new(),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
    };
    let (sink, _source) = line_channel(4096);
    let report = tokio::time::timeout(
        Duration::from_secs(20),
        loadout_lib::commands::run::run_workflow_inner(&deps, &planned.request, sink),
    )
    .await??;
    let seen = observed.lock().map_err(|_| "observations poisoned")?;
    assert_eq!(
        seen.iter().filter(|role| *role == "left").count(),
        8,
        "loop work was replaced or did not repeat"
    );
    assert_eq!(seen.iter().filter(|role| *role == "right").count(), 4);
    assert_eq!(seen.iter().filter(|role| *role == "join").count(), 4);
    let file: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    let bindings = file["workflow_snapshot"]["cellBindings"]
        .as_array()
        .ok_or("missing explicit cell bindings")?;
    assert_eq!(bindings.len(), 4);
    for binding in bindings {
        let grader = binding["grader"]
            .as_str()
            .ok_or("missing bound external check")?;
        let step = file["steps"]
            .as_array()
            .ok_or("missing history")?
            .iter()
            .find(|step| step["node_key"] == grader)
            .ok_or("external check not in executed graph")?;
        assert_eq!(
            step["status"], "succeeded",
            "the independent check did not read its cell's final files: {step}"
        );
    }
    assert!(!project.join("left.txt").exists());
    assert!(!project.join("right.txt").exists());
    let board = loadout_lib::commands::lab::read_board_inner(&project, "whole-workflow", 5)?;
    let latest = board
        .runs
        .first()
        .ok_or("the completed comparison has no visible history")?;
    assert_eq!(
        (latest.passed, latest.judged, latest.cells.len()),
        (4, 4, 4),
        "the Lab board must grade every repeat through its saved cell binding, not split a step name"
    );
    Ok(())
}

#[test]
#[ignore = "real child of the production Check, never a substitute test runner"]
fn external_check_fixture() -> Result<(), Box<dyn Error>> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(256 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    let input: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(
        input["results"].as_array().ok_or("missing results")?.len(),
        1
    );
    let root = std::path::Path::new(
        input["results"][0]["files"]["root"]
            .as_str()
            .ok_or("missing frozen result")?,
    );
    assert_eq!(fs::read_to_string(root.join("left.txt"))?, "2");
    assert_eq!(fs::read_to_string(root.join("right.txt"))?, "CASE_TASK");
    println!("external assessment: 1 passed");
    Ok(())
}

struct Fixture {
    seen: Arc<Mutex<Vec<String>>>,
    barrier: Arc<Barrier>,
}
#[async_trait]
impl AgentDriver for Fixture {
    fn id(&self) -> &'static str {
        "claude-code"
    }
    fn carries_extra_dirs(&self) -> bool {
        true
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
        anyhow::ensure!(
            spec.prompt.contains("CASE_TASK"),
            "the case task did not reach its actual step"
        );
        anyhow::ensure!(
            fs::read_to_string(spec.cwd.join("input.txt"))? == "same input for every cell"
        );
        let role = ["left", "right", "join"]
            .into_iter()
            .find(|role| spec.prompt.contains(&format!("ROLE_{role}")))
            .ok_or_else(|| {
                anyhow::anyhow!("the Lab substituted the graph with an invented agent")
            })?;
        let first_left = role == "left" && !spec.cwd.join("left.txt").exists();
        if role == "left" {
            fs::write(
                spec.cwd.join("left.txt"),
                if first_left { "1" } else { "2" },
            )?;
        }
        if role == "right" {
            fs::write(spec.cwd.join("right.txt"), "CASE_TASK")?;
        }
        if role == "join" {
            anyhow::ensure!(fs::read_to_string(spec.cwd.join("left.txt"))? == "2");
            anyhow::ensure!(fs::read_to_string(spec.cwd.join("right.txt"))? == "CASE_TASK");
        }
        self.seen
            .lock()
            .map_err(|_| anyhow::anyhow!("observations poisoned"))?
            .push(role.to_owned());
        Ok(Box::new(Turn {
            events,
            barrier: (first_left || role == "right").then(|| self.barrier.clone()),
            session: SessionRef {
                vendor: "claude-code",
                id: spec.run_id.to_string(),
            },
        }))
    }
}
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    barrier: Option<Arc<Barrier>>,
    session: SessionRef,
}
#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }
    fn group(&self) -> Option<loadout_lib::engine::supervisor::GroupId> {
        None
    }
    async fn send(&mut self, _: String) -> anyhow::Result<()> {
        Ok(())
    }
    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        if let Some(barrier) = &self.barrier {
            tokio::time::timeout(Duration::from_secs(5), barrier.wait()).await?;
        }
        let text = "Answer: completed\nEvidence: files\nOpen: none".to_owned();
        self.events
            .send(AgentEvent::Said { text: text.clone() }.into())
            .await?;
        let outcome = Outcome {
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
