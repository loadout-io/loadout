//! WF-20: jeden realny bieg łączy przygotowanie, repo context, równoległość,
//! operacje plikowe, poprawkę pętli i odczyt Leada. Dubel zastępuje wyłącznie model.

#![allow(clippy::too_many_lines)]

use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use loadout_lib::bridge::{Answer, Call, host::Answers, library::Desk};
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{self, GroupProof};
use loadout_lib::ipc::line_channel;
use loadout_lib::library::agents::Agent;
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tokio::sync::{Barrier, Notify, Semaphore, mpsc};

const SKILL: &str = "acceptance-reader";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_portable_parallel_repair_preserves_files_and_the_lead_addresses_that_live_run()
-> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let project = temp.path().join("project");
    let home = temp.path().join("library");
    fs::create_dir_all(project.join(".loadout"))?;
    fs::create_dir_all(home.join("agents"))?;
    git(&project, &["init", "-q"])?;
    for (name, body) in [
        ("tracked.txt", "committed"),
        ("delete.txt", "remove me"),
        ("old-name.txt", "rename me"),
        (".gitignore", "*.local\n.loadout/\n"),
        (
            "AGENTS.md",
            "REPO_ACCEPTANCE_RULE: preserve the requested files.",
        ),
    ] {
        fs::write(project.join(name), body)?;
    }
    git(&project, &["add", "."])?;
    git(
        &project,
        &["commit", "-qm", "disposable acceptance fixture"],
    )?;
    fs::write(project.join("tracked.txt"), "captured WIP")?;
    fs::write(project.join("chosen.local"), "chosen input")?;
    fs::write(project.join("not-chosen.local"), "not an input")?;
    fs::write(
        project.join(".loadout/project.json"),
        r#"{"instructions":{"enabled":true}}"#,
    )?;
    let skill = project.join(".claude/skills").join(SKILL);
    fs::create_dir_all(skill.join("references"))?;
    fs::create_dir_all(skill.join("scripts"))?;
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: acceptance-reader\ndescription: Reads acceptance data\n---\nRead references/marker.txt and scripts/helper.sh.\n",
    )?;
    fs::write(
        skill.join("references/marker.txt"),
        "complete skill reference",
    )?;
    fs::write(
        skill.join("scripts/helper.sh"),
        "#!/bin/sh\nprintf 'complete skill helper\\n'\n",
    )?;
    supervisor::set_executable_file(&std::fs::File::open(skill.join("scripts/helper.sh"))?, true)?;
    let mut agent = Agent::example();
    agent.skills.clear();
    agent.connections.clear();
    agent.instructions = "Execute the requested acceptance role.".to_owned();
    loadout_lib::commands::agents::save_agent_inner(&home, &agent, None)?;
    let worker = |id: &str, folder: &str| {
        json!({"kind":"agent","id":id,"name":id,
        "agent":agent.id.to_string(),"instructions":format!("AC_ROLE_{id}"),
        "folder":{"use":folder},"borrow":{"skills":[SKILL]},"handover":"notes"})
    };
    let check = |id: &str, folder: &str, command: &str| {
        json!({"kind":"check","id":id,"name":id,
        "folder":{"use":folder},"command":command,"proof":"(\\d+) passed"})
    };
    let graph = json!({"format":1,"id":"product-acceptance","name":"Portable parallel repair",
        "additionalInputs":["chosen.local"],
        "steps":[check("prepare","fresh-copy","test \"$(cat chosen.local)\" = 'chosen input' && printf '1 passed\\n'"),
            worker("left","fresh-copy"),worker("right","fresh-copy"),
            check("merge","same-copy","test ! -e delete.txt && test ! -e old-name.txt && test -f new-name.txt && test -f left.txt && test -f right.txt && printf '1 passed\\n'"),
            check("judge","same-copy","test \"$(cat left.txt)\" = 2 && printf '1 passed\\n'"),
            worker("final","same-copy")],
        "links":[{"from":"prepare","to":"left"},{"from":"prepare","to":"right"},
            {"from":"left","to":"merge"},{"from":"right","to":"merge"},
            {"from":"merge","to":"judge"},{"from":"judge","to":"prepare","max_turns":2},
            {"from":"judge","to":"final"}]});
    let workflow = temp.path().join("acceptance.json");
    fs::write(&workflow, serde_json::to_vec(&graph)?)?;
    let seen = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let ready = Arc::new(Notify::new());
    let release = Arc::new(Semaphore::new(0));
    let fixture = Arc::new(Worker {
        seen: seen.clone(),
        flags: Vec::new(),
        barrier: Arc::new(Barrier::new(2)),
        ready: ready.clone(),
        release: release.clone(),
    });
    let drivers: Drivers = Arc::new(move |_| fixture.clone());
    let store = Store::open(&project.join(".loadout/loadout.db"))?;
    let deps = RunDeps {
        home: &home,
        library: home.clone(),
        project: &project,
        store: &store,
        drivers,
        control: RunControl::new(),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 2,
        task: Some("Deliver the portable result".into()),
        part: None,
        handoffs_from: None,
    };
    let (sink, _source) = line_channel(4096);
    let running = loadout_lib::commands::run::run_workflow_inner(&deps, &request, sink);
    tokio::pin!(running);
    tokio::time::timeout(Duration::from_secs(15),async {
        tokio::select! {
            () = ready.notified() => Ok(()),
            ended = &mut running => {
                let steps = ended.as_ref().ok().and_then(|report|fs::read(report.dir.join("run.json")).ok())
                    .and_then(|bytes|serde_json::from_slice::<Value>(&bytes).ok())
                    .and_then(|receipt|receipt["steps"].as_array().cloned())
                    .map(|steps|steps.iter().map(|step|json!({"node":step["node_key"],"state":step["status"],"error":step["error"]})).collect::<Vec<_>>());
                Err(format!("the workflow ended before two workers overlapped: {ended:?}; execution facts: {steps:?}"))
            },
        }
    }).await??;
    let source_id = seen
        .lock()
        .map_err(|_| "observations poisoned")?
        .first()
        .ok_or("no started worker")?
        .1
        .clone();
    // Gospodarz może dalej pracować: kolejne próby nadal mają wejście uchwycone przed Startem.
    fs::write(project.join("tracked.txt"), "host changed during the run")?;
    let other = temp.path().join("other-project");
    let other_run = other
        .join(".loadout/runs")
        .join(format!("20260906-010000__{source_id}"));
    fs::create_dir_all(&other_run)?;
    fs::write(
        other_run.join("run.json"),
        serde_json::to_vec(
            &json!({"id":source_id,"title":"OTHER PRIVATE PROJECT","status":"failed","steps":[]}),
        )?,
    )?;
    let desk = Desk::at(Some(home.clone()), project.clone());
    let answer = desk
        .answer(Call {
            id: json!(1),
            call: "get_run_status".into(),
            input: json!({"run_id":source_id}),
        })
        .await;
    let Answer::Ok(status) = answer else {
        return Err(format!("Lead could not read the active addressed run: {answer:?}").into());
    };
    assert_eq!(status["run"]["runId"], source_id);
    assert_eq!(status["title"], "Portable parallel repair");
    assert!(!status.to_string().contains("OTHER PRIVATE PROJECT"));
    release.add_permits(2);
    let report = tokio::time::timeout(Duration::from_secs(30), running).await??;
    let receipt: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    let steps = receipt["steps"]
        .as_array()
        .ok_or("missing executed steps")?;
    let final_step = steps
        .iter()
        .find(|step| step["node_key"] == "final")
        .ok_or("missing synthesis")?;
    assert_eq!(
        final_step["status"], "succeeded",
        "the complete graph did not deliver its result: {receipt}"
    );
    let judges: Vec<_> = steps
        .iter()
        .filter(|step| {
            step["node_key"]
                .as_str()
                .is_some_and(|key| key.starts_with("judge"))
        })
        .map(|step| step["round_outcome"].clone())
        .collect();
    assert_eq!(
        judges,
        vec![json!("fail"), json!("pass")],
        "a real failing judge must trigger one actual repair"
    );
    let seen = seen.lock().map_err(|_| "observations poisoned")?;
    assert_eq!(seen.iter().filter(|(role, _)| role == "left").count(), 2);
    assert_eq!(seen.iter().filter(|(role, _)| role == "right").count(), 2);
    assert_eq!(seen.iter().filter(|(role, _)| role == "final").count(), 1);
    assert_eq!(
        fs::read_to_string(project.join("tracked.txt"))?,
        "host changed during the run"
    );
    assert_eq!(fs::read_to_string(project.join("delete.txt"))?, "remove me");
    assert_eq!(
        fs::read_to_string(project.join("old-name.txt"))?,
        "rename me"
    );
    assert!(!project.join("new-name.txt").exists());
    assert!(
        steps
            .iter()
            .filter(|step| step["process_started"] == true)
            .all(|step| step["death_proof"] == true)
    );
    Ok(())
}

fn git(project: &Path, args: &[&str]) -> Result<(), Box<dyn Error>> {
    let output = std::process::Command::new("git")
        .current_dir(project)
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "user.name=Loadout acceptance",
            "-c",
            "user.email=acceptance@example.invalid",
        ])
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(())
}

#[derive(Clone)]
struct Worker {
    // Wyłącznie krótkie push/odczyty; guard nigdy nie przechodzi przez await.
    seen: Arc<Mutex<Vec<(String, String)>>>,
    flags: Vec<String>,
    barrier: Arc<Barrier>,
    ready: Arc<Notify>,
    release: Arc<Semaphore>,
}
#[async_trait]
impl AgentDriver for Worker {
    fn id(&self) -> &'static str {
        "claude-code"
    }
    fn inheriting(&self, flags: &[String]) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            flags: flags.to_vec(),
            ..self.clone()
        }))
    }
    fn with_evidence(
        &self,
        _: loadout_lib::evidence::EvidenceTarget,
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
        anyhow::ensure!(
            spec.prompt.contains("REPO_ACCEPTANCE_RULE"),
            "the opted-in repository instructions did not reach the actual worker"
        );
        anyhow::ensure!(fs::read_to_string(spec.cwd.join("tracked.txt"))? == "captured WIP");
        anyhow::ensure!(fs::read_to_string(spec.cwd.join("chosen.local"))? == "chosen input");
        anyhow::ensure!(!spec.cwd.join("not-chosen.local").exists());
        // FreshCopy ma wspólną bazę wejścia, nie filesystem rodzica. Przygotowanie tutaj
        // sprawdza wejście; jawne składanie pracy agentów jest późniejszym SameCopy.
        let package = self
            .flags
            .windows(2)
            .find(|pair| pair[0] == "--plugin-dir")
            .map(|pair| PathBuf::from(&pair[1]).join("skills").join(SKILL))
            .ok_or_else(|| anyhow::anyhow!("native skill path missing"))?;
        anyhow::ensure!(
            fs::read_to_string(package.join("references/marker.txt"))?
                == "complete skill reference"
        );
        anyhow::ensure!(
            fs::read_to_string(package.join("scripts/helper.sh"))?
                .contains("complete skill helper")
        );
        anyhow::ensure!(supervisor::executable_bits(&fs::metadata(
            package.join("scripts/helper.sh")
        )?));
        let role = ["left", "right", "final"]
            .into_iter()
            .find(|role| spec.prompt.contains(&format!("AC_ROLE_{role}")))
            .ok_or_else(|| anyhow::anyhow!("unknown actual graph role"))?;
        let run_dir = spec
            .cwd
            .ancestors()
            .find(|path| path.join("run.json").is_file())
            .ok_or_else(|| anyhow::anyhow!("the fixture worker has no containing run"))?;
        let run: Value = serde_json::from_slice(&fs::read(run_dir.join("run.json"))?)?;
        let run_id = run["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("the run has no identity"))?;
        let round = {
            let mut seen = self
                .seen
                .lock()
                .map_err(|_| anyhow::anyhow!("observations poisoned"))?;
            let round = seen.iter().filter(|(previous, _)| previous == role).count() + 1;
            // RunSpec.run_id adresuje sesję kroku, nie UUID całego workflow.
            seen.push((role.to_owned(), run_id.to_owned()));
            round
        };
        match role {
            "left" => {
                if spec.cwd.join("delete.txt").exists() {
                    fs::remove_file(spec.cwd.join("delete.txt"))?;
                }
                fs::write(spec.cwd.join("left.txt"), round.to_string())?;
            }
            "right" => {
                if spec.cwd.join("old-name.txt").exists() {
                    fs::rename(spec.cwd.join("old-name.txt"), spec.cwd.join("new-name.txt"))?;
                }
                fs::write(spec.cwd.join("right.txt"), round.to_string())?;
            }
            _ => {
                anyhow::ensure!(
                    !spec.cwd.join("delete.txt").exists()
                        && !spec.cwd.join("old-name.txt").exists()
                );
                anyhow::ensure!(fs::read_to_string(spec.cwd.join("new-name.txt"))? == "rename me");
                anyhow::ensure!(fs::read_to_string(spec.cwd.join("left.txt"))? == "2");
                anyhow::ensure!(fs::read_to_string(spec.cwd.join("right.txt"))? == "2");
                fs::write(spec.cwd.join("delivered.txt"), "complete product result")?;
            }
        }
        Ok(Box::new(Turn {
            events,
            session: SessionRef {
                vendor: "claude-code",
                id: spec.run_id.to_string(),
            },
            barrier: (role != "final").then(|| self.barrier.clone()),
            hold: (role != "final" && round == 1)
                .then(|| (self.ready.clone(), self.release.clone())),
        }))
    }
}
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    barrier: Option<Arc<Barrier>>,
    hold: Option<(Arc<Notify>, Arc<Semaphore>)>,
}
#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }
    fn group(&self) -> Option<supervisor::GroupId> {
        None
    }
    async fn send(&mut self, _: String) -> anyhow::Result<()> {
        Ok(())
    }
    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        if let Some(barrier) = &self.barrier {
            tokio::time::timeout(Duration::from_secs(5), barrier.wait()).await?;
        }
        if let Some((ready, release)) = &self.hold {
            ready.notify_one();
            tokio::time::timeout(Duration::from_secs(10), release.acquire())
                .await??
                .forget();
        }
        let text = "Answer: delivered\nEvidence: actual files\nOpen: none".to_owned();
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
