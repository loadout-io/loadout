//! WF-16: sprawdzamy rzeczywiste `RunSpec` i katalogi przekazań, nie izolację systemową.

#![allow(clippy::too_many_lines)]

use std::error::Error;
use std::fs;
use std::sync::{Arc, Mutex};
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

#[tokio::test]
async fn two_contexts_keep_their_task_instructions_and_published_directories_separate()
-> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let project = temp.path().join("project");
    let library = temp.path().join("library");
    fs::create_dir_all(project.join(".loadout"))?;
    fs::create_dir_all(library.join("agents"))?;
    fs::write(project.join("AGENTS.md"), "HOST_INSTRUCTION_MUST_NOT_LEAK")?;
    fs::write(
        project.join(".loadout/project.json"),
        r#"{"instructions":{"enabled":true}}"#,
    )?;
    let mut agent = Agent::example();
    agent.skills.clear();
    agent.connections.clear();
    loadout_lib::commands::agents::save_agent_inner(&library, &agent, None)?;
    let step = |id: &str, folder: &str, instructions: &str| {
        json!({
            "kind":"agent","id":id,"name":"Implement","agent":agent.id.to_string(),
            "instructions":instructions,"folder":{"use":folder},"skills":[],"handover":"notes","at":{"x":0,"y":0}
        })
    };
    let graph = json!({
        "format":1,"id":"wf_contexts","name":"Independent contexts",
        "steps":[step("s_a","project","publish {{task}}"),step("s_a_read","project","read {{task}}"),
                 step("s_b","project","publish {{task}}"),step("s_b_read","project","read {{task}}")],
        "links":[{"from":"s_a","to":"s_a_read"},{"from":"s_b","to":"s_b_read"}],
        "executionInputs":{"schema":1,
            "contexts":{"a":{"task":"TASK_A","instructions":"RULE_A"},"b":{"task":"TASK_B","instructions":"RULE_B"}},
            "stepContexts":{"s_a":"a","s_a_read":"a","s_b":"b","s_b_read":"b"},
            "effects":{"publishMemory":false,"externalMessages":false}}
    });
    let path = project.join("workflow.json");
    fs::write(&path, serde_json::to_vec(&graph)?)?;
    let calls = Arc::new(Mutex::new(Vec::new()));
    let seen = calls.clone();
    let drivers: Drivers = Arc::new(move |_| Arc::new(Fixture(seen.clone())));
    let store = Store::open(&project.join(".loadout/loadout.db"))?;
    let deps = RunDeps {
        home: &library,
        library: library.clone(),
        project: &project,
        store: &store,
        drivers,
        control: RunControl::new(),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
    };
    let (sink, _stream) = line_channel(1024);
    let result = loadout_lib::commands::run::run_workflow_inner(
        &deps,
        &RunRequest {
            workflow: path,
            how_many_at_once: 2,
            task: Some("HOST_TASK_MUST_NOT_LEAK".to_owned()),
            part: None,
            handoffs_from: None,
        },
        sink,
    )
    .await;
    assert!(
        result.is_ok(),
        "independent Project aliases were refused before they could run: {result:?}"
    );
    let report = result?;
    {
        let calls = calls.lock().map_err(|_| "poisoned")?;
        assert_eq!(calls.len(), 4);
        for spec in calls.iter() {
            let a = spec.prompt.contains("TASK_A");
            let (own, foreign, scope) = if a {
                ("RULE_A", "RULE_B", "a")
            } else {
                ("RULE_B", "RULE_A", "b")
            };
            assert!(spec.prompt.contains(own));
            assert!(!spec.prompt.contains(foreign));
            assert!(!spec.prompt.contains("HOST_TASK_MUST_NOT_LEAK"));
            assert!(!spec.prompt.contains("HOST_INSTRUCTION_MUST_NOT_LEAK"));
            assert_ne!(spec.cwd, project);
            if spec.prompt.contains("read TASK_") {
                let own_dir = report.dir.join("context").join(scope).join("handoffs");
                assert!(
                    spec.extra_dirs.contains(&own_dir),
                    "the real RunSpec omitted its own handoffs: {:?}",
                    spec.extra_dirs
                );
                assert!(
                    spec.extra_dirs
                        .iter()
                        .all(|dir| !report.dir.join("context").starts_with(dir)),
                    "a parent directory grants every context"
                );
                let files = fs::read_dir(&own_dir)?.collect::<Result<Vec<_>, _>>()?;
                assert!(!files.is_empty());
                for file in files {
                    let text = fs::read_to_string(file.path())?;
                    assert!(!text.contains(if a { "TASK_B" } else { "TASK_A" }));
                }
            }
        }
        let a_cwd = &calls
            .iter()
            .find(|spec| spec.prompt.contains("publish TASK_A"))
            .ok_or("missing A")?
            .cwd;
        let b_cwd = &calls
            .iter()
            .find(|spec| spec.prompt.contains("publish TASK_B"))
            .ok_or("missing B")?
            .cwd;
        assert_ne!(a_cwd, b_cwd);
        assert!(
            calls
                .iter()
                .filter(|spec| spec.prompt.contains("TASK_A"))
                .all(|spec| &spec.cwd == a_cwd)
        );
        let saved: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
        assert_eq!(saved["reflection"]["ran"], false);
        assert_eq!(saved["reflection"]["why"], "turned-off");
        let published = loadout_lib::memory::handoff::scan_run_dir(&report.dir)?;
        assert_eq!(
            published.len(),
            4,
            "the real history/recovery reader lost the scoped results"
        );
    }
    let retry_deps = RunDeps {
        home: &library,
        library: library.clone(),
        project: &project,
        store: &store,
        drivers: deps.drivers.clone(),
        control: RunControl::new(),
        processes: deps.processes.clone(),
    };
    let (sink, _source) = line_channel(1024);
    let retried = loadout_lib::commands::run::run_workflow_inner(
        &retry_deps,
        &RunRequest {
            workflow: project.join("workflow.json"),
            how_many_at_once: 2,
            task: Some("HOST_TASK_MUST_NOT_LEAK".to_owned()),
            part: Some(loadout_lib::commands::Part::Just(vec![
                "s_a_read".to_owned(),
            ])),
            handoffs_from: Some(report.dir.clone()),
        },
        sink,
    )
    .await?;
    assert!(
        retried
            .steps
            .iter()
            .all(|state| *state == loadout_lib::engine::step::StepState::Succeeded),
        "a scoped partial retry did not receive its own managed working folder: {:?}",
        retried.steps
    );
    let retry_handoffs = loadout_lib::memory::handoff::scan_run_dir(&retried.dir)?;
    assert!(
        retry_handoffs.iter().any(|hand| hand.meta.run == retried.id
            && hand
                .meta
                .reads
                .iter()
                .any(|read| read.contains("context/a/handoffs"))),
        "partial retry lost its scoped input history"
    );
    Ok(())
}

struct Fixture(Arc<Mutex<Vec<RunSpec>>>);

#[tokio::test]
async fn each_context_materializes_its_addressed_saved_input_not_todays_host_files()
-> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let project = temp.path().join("project");
    let library = temp.path().join("library");
    fs::create_dir_all(project.join(".loadout"))?;
    fs::create_dir_all(library.join("agents"))?;
    let mut agent = Agent::example();
    agent.skills.clear();
    agent.connections.clear();
    loadout_lib::commands::agents::save_agent_inner(&library, &agent, None)?;
    let step = |id: &str, folder: &str, instructions: &str| {
        json!({
            "kind":"agent","id":id,"name":"Implement","agent":agent.id.to_string(),
            "instructions":instructions,"folder":{"use":folder},"skills":[],"handover":"notes","at":{"x":0,"y":0}
        })
    };
    let source_graph = json!({"format":1,"id":"wf_seed_source","name":"Save an input",
        "steps":[step("s_source","fresh-copy","source")],"links":[],
        "executionInputs":{"schema":1,"effects":{"publishMemory":false}}});
    let path = project.join("workflow.json");
    fs::write(&path, serde_json::to_vec(&source_graph)?)?;
    let store = Store::open(&project.join(".loadout/loadout.db"))?;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let drivers: Drivers = Arc::new(move |_| Arc::new(Fixture(record.clone())));
    let processes = Arc::new(loadout_lib::commands::processes::Processes::new());
    let mut saved = Vec::new();
    for marker in ["SEED_A", "SEED_B"] {
        fs::write(project.join("seed.txt"), marker)?;
        let deps = RunDeps {
            home: &library,
            library: library.clone(),
            project: &project,
            store: &store,
            drivers: drivers.clone(),
            control: RunControl::new(),
            processes: processes.clone(),
        };
        let (sink, _source) = line_channel(1024);
        let report = loadout_lib::commands::run::run_workflow_inner(
            &deps,
            &RunRequest {
                workflow: path.clone(),
                how_many_at_once: 2,
                task: None,
                part: None,
                handoffs_from: None,
            },
            sink,
        )
        .await?;
        let snapshot = loadout_lib::commands::input_snapshot::read(&report.dir)?;
        saved.push(json!({"sourceRunId":report.id,"snapshotId":snapshot.id()}));
    }
    fs::write(
        project.join("seed.txt"),
        "TODAYS_HOST_MUST_NOT_REPLACE_SAVED_INPUT",
    )?;
    let graph = json!({"format":1,"id":"wf_separate_seeds","name":"Independent saved inputs",
        "steps":[step("s_a","project","CHECK_SEED_A"),step("s_a_fresh","fresh-copy","CHECK_SEED_A"),
                 step("s_b","project","CHECK_SEED_B"),step("s_b_fresh","fresh-copy","CHECK_SEED_B")],
        "links":[{"from":"s_a","to":"s_a_fresh"},{"from":"s_b","to":"s_b_fresh"}],
        "executionInputs":{"schema":1,"workspaceSeeds":{"a":saved[0],"b":saved[1]},
            "contexts":{"a":{"task":"TASK_A","workspaceSeed":"a"},"b":{"task":"TASK_B","workspaceSeed":"b"}},
            "stepContexts":{"s_a":"a","s_a_fresh":"a","s_b":"b","s_b_fresh":"b"},
            "effects":{"publishMemory":false}}});
    fs::write(&path, serde_json::to_vec(&graph)?)?;
    let deps = RunDeps {
        home: &library,
        library: library.clone(),
        project: &project,
        store: &store,
        drivers,
        control: RunControl::new(),
        processes,
    };
    let (sink, _source) = line_channel(1024);
    let result = loadout_lib::commands::run::run_workflow_inner(
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
    .await;
    assert!(
        result.is_ok(),
        "verified historical inputs must be usable without a Pick path: {result:?}"
    );
    let report = result?;
    assert_eq!(report.steps.len(), 4);
    assert!(
        report
            .steps
            .iter()
            .all(|state| *state == loadout_lib::engine::step::StepState::Succeeded),
        "a real driver received another context's input: {:?}",
        report.steps
    );
    let file: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    for key in ["s_a", "s_a_fresh", "s_b", "s_b_fresh"] {
        let index = usize::from(key.starts_with("s_b"));
        assert_eq!(
            file["workspace_inputs"][key]["id"],
            saved[index]["snapshotId"]
        );
        assert_eq!(
            file["copy_results"][key]["origin"],
            saved[index]["snapshotId"]
        );
    }
    assert_eq!(
        fs::read_to_string(project.join("seed.txt"))?,
        "TODAYS_HOST_MUST_NOT_REPLACE_SAVED_INPUT"
    );
    Ok(())
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
            fs::metadata(&spec.cwd)?.is_dir(),
            "the real driver needs an existing working folder"
        );
        for expected in ["SEED_A", "SEED_B"] {
            if spec.prompt.contains(&format!("CHECK_{expected}")) {
                anyhow::ensure!(
                    fs::read_to_string(spec.cwd.join("seed.txt"))? == expected,
                    "the managed copy received a different saved input"
                );
            }
        }
        let task = if spec.prompt.contains("TASK_A") {
            "TASK_A"
        } else {
            "TASK_B"
        };
        let session = SessionRef {
            vendor: "claude-code",
            id: spec.run_id.to_string(),
        };
        let text = format!("Answer: {task}\nEvidence: fixture\nOpen: none");
        self.0
            .lock()
            .map_err(|_| anyhow::anyhow!("poisoned"))?
            .push(spec);
        Ok(Box::new(Turn {
            events,
            text,
            session,
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
