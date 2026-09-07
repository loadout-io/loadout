//! WF-27: model oddaje typowany opis, ale realne wykonanie/port/cwd mierzy host.
//! Ten sam graf w dwóch repo. Dubler czyta manifest repo zamiast otrzymywać
//! gotową komendę z testu wywołującego — symuluje tylko odpowiedź modelu.

#![allow(clippy::expect_used)]
use std::error::Error;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::processes::Processes;
use loadout_lib::commands::run::{run_workflow_inner, stop_run_inner};
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{self, GroupId, GroupProof};
use loadout_lib::ipc::line_channel;
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::mpsc;

const AGENT: &str = "---\nschema: 1\nid: 01990000-0000-7000-8000-000000002727\nname: Project launcher\nsummary: Reads how this project starts\ncolor: slate\nrunsWith: claude-code\nmodel: opus\nthinking: balanced\nfileAccess: work-freely\ngiveUpAfterMinutes: 2\nwriteResultsTo: \"\"\ntools: everything\nskills: []\nconnections: []\n---\nRead this repository and prepare its launch description.\n";

#[derive(Clone, Copy)]
enum Case {
    Normal,
    SecondCopy,
    Grandparent,
    Ambiguous,
    EscapingFolder,
    MissingEnvironment,
    MissingField,
    Secret,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_same_graph_starts_two_different_repositories_in_the_prepared_subdirectory()
-> Result<(), Box<dyn Error>> {
    for variant in ["one", "two"] {
        let ran = run(variant, Case::Normal).await?;
        assert_eq!(
            ran.mode, variant,
            "the actual process did not receive this repository's launch environment: {}",
            ran.book
        );
        assert_eq!(ran.prepared, "prepared by the repository agent\n");
        assert!(
            ran.cwd.ends_with("/packages/web"),
            "the actual server did not work in the requested subdirectory: {}",
            ran.cwd
        );
        assert_eq!(
            ran.consumers, 1,
            "the ready service did not reach its consumer"
        );
        assert!(!ran.manual, "the disabled manual command was executed");
        assert_eq!(step(&ran.book, "s_preview")["status"], "succeeded");
        assert!(
            ran.cwd_survived_graph,
            "the subdirectory process lost its parent working copy after the graph"
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_exact_second_copy_is_the_only_launch_description_producer()
-> Result<(), Box<dyn Error>> {
    let ran = run("one", Case::SecondCopy).await?;
    assert_eq!(
        ran.mode, "second-copy",
        "the service selected a different producer or the last matching field: {}",
        ran.book
    );
    assert!(!ran.manual);
    assert_eq!(ran.consumers, 1);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_exact_earlier_result_can_cross_an_intermediate_step() -> Result<(), Box<dyn Error>> {
    let ran = run("one", Case::Grandparent).await?;
    assert_eq!(
        ran.mode, "one",
        "the selected earlier description did not start its app: {}",
        ran.book
    );
    assert_eq!(ran.consumers, 1);
    assert_eq!(step(&ran.book, "s_preview")["status"], "succeeded");
    assert!(!ran.manual);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_matching_copies_without_an_exact_choice_are_refused() -> Result<(), Box<dyn Error>> {
    assert_refused(Case::Ambiguous, "More than one earlier result").await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_launch_description_cannot_escape_the_prepared_copy() -> Result<(), Box<dyn Error>> {
    assert_refused(Case::EscapingFolder, "folder").await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_required_environment_value_is_checked_before_spawn() -> Result<(), Box<dyn Error>> {
    assert_refused(Case::MissingEnvironment, "WF27_REQUIRED_BUT_ABSENT").await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_missing_agent_field_never_falls_back_to_the_saved_manual_command()
-> Result<(), Box<dyn Error>> {
    assert_refused(Case::MissingField, "launch").await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_secret_in_the_launch_command_is_refused_without_starting_any_service()
-> Result<(), Box<dyn Error>> {
    assert_refused(Case::Secret, "key").await
}

async fn assert_refused(case: Case, message: &str) -> Result<(), Box<dyn Error>> {
    let ran = run("one", case).await?;
    let preview = step(&ran.book, "s_preview");
    assert_eq!(
        preview["status"], "failed",
        "the service did not refuse its invalid launch description: {}",
        ran.book
    );
    assert_eq!(
        preview["process_started"], false,
        "invalid launch data reached the shell: {preview}"
    );
    assert_eq!(ran.consumers, 0);
    assert!(!ran.manual, "failure silently ran the old manual command");
    assert!(
        preview["error"]
            .as_str()
            .is_some_and(|error| error.to_lowercase().contains(&message.to_lowercase())),
        "the real history omitted what must be fixed: {preview}"
    );
    Ok(())
}

struct Ran {
    book: Value,
    mode: String,
    prepared: String,
    cwd: String,
    consumers: usize,
    manual: bool,
    cwd_survived_graph: bool,
}

async fn run(variant: &str, case: Case) -> Result<Ran, Box<dyn Error>> {
    let home = TempDir::new()?;
    let project = TempDir::new()?;
    let control = TempDir::new()?;
    fs::create_dir_all(home.path().join("agents"))?;
    fs::create_dir_all(home.path().join("workflows"))?;
    fs::write(home.path().join("agents/launcher.md"), AGENT)?;
    fs::create_dir_all(project.path().join("packages/web"))?;
    let binary = std::env::current_exe()?;
    let command = if variant == "one" {
        "sh start-one.sh"
    } else {
        "sh start-two.sh"
    };
    let script = format!(
        "test -f prepared.txt || exit 41\nprintf '%s' \"$WF27_MODE\" > {control}/mode\npwd -P > {control}/cwd\ncp prepared.txt {control}/prepared\nexport WF26_CONTROL={control}\nexport WF26_NAME=launch\nexport WF26_FIXED_PORT=0\nexport WF26_DELAY=0\nexec {binary} --ignored --exact a_preview_is_ready_before_its_consumer_starts::http_service_fixture --nocapture\n",
        control = quote(&control.path().to_string_lossy()),
        binary = quote(&binary.to_string_lossy())
    );
    let script_name = command
        .strip_prefix("sh ")
        .ok_or("invalid fixture command")?;
    fs::write(
        project.path().join("packages/web").join(script_name),
        script,
    )?;
    fs::write(
        project.path().join("project-start.json"),
        serde_json::to_vec(&json!({
            "command":command,"subdirectory":"packages/web","environment":{"WF27_MODE":variant},
            "endpoints":[{"name":"web","host":"127.0.0.1","port":0,"portEnv":"LOADOUT_WEB_PORT"}],
            "readiness":{"kind":"http","endpoint":"web","path":"/health","timeoutSeconds":3,"expectedStatus":200}
        }))?,
    )?;
    let workflow = home.path().join("workflows/launch.json");
    fs::write(
        &workflow,
        serde_json::to_vec(&workflow_file(case, control.path()))?,
    )?;
    fs::create_dir_all(project.path().join(".loadout"))?;
    let store = Store::open(&project.path().join(".loadout/loadout.db"))?;
    let seen = Arc::new(Mutex::new(0usize));
    let driver: Arc<dyn AgentDriver> = Arc::new(Discoverer {
        case,
        consumers: Arc::clone(&seen),
    });
    let drivers: Drivers = Arc::new(move |_| Arc::clone(&driver));
    let processes = Arc::new(Processes::new());
    let deps = RunDeps {
        home: home.path(),
        project: project.path(),
        store: &store,
        drivers,
        processes: Arc::clone(&processes),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 4,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (sink, mut source) = line_channel(4096);
    let report = tokio::time::timeout(
        Duration::from_secs(15),
        run_workflow_inner(&deps, &request, sink),
    )
    .await;
    if report.is_err() {
        let _ = stop_run_inner(&deps).await;
    }
    let cwd = fs::read_to_string(control.path().join("cwd"))
        .unwrap_or_default()
        .trim()
        .to_owned();
    let cwd_survived_graph = !cwd.is_empty() && Path::new(&cwd).join("prepared.txt").is_file();
    let groups: Vec<_> = processes.list().iter().map(|one| one.pgid).collect();
    let proofs = processes.close().await;
    assert!(
        proofs
            .iter()
            .all(|proof| matches!(proof, GroupProof::Dead { .. })),
        "fixture cleanup left an unproven process: {proofs:?}"
    );
    assert!(groups.into_iter().all(supervisor::group_is_empty));
    let report = report??;
    while source.try_next().is_some() {}
    let book = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    let consumers = *seen.lock().unwrap_or_else(PoisonError::into_inner);
    Ok(Ran {
        book,
        mode: fs::read_to_string(control.path().join("mode")).unwrap_or_default(),
        prepared: fs::read_to_string(control.path().join("prepared")).unwrap_or_default(),
        cwd,
        consumers,
        manual: control.path().join("manual").exists(),
        cwd_survived_graph,
    })
}

fn workflow_file(case: Case, control: &Path) -> Value {
    let mut prepare = json!({"kind":"agent","id":"s_prepare","name":"Prepare this repository","agent":"01990000-0000-7000-8000-000000002727","instructions":"wf27-discover copy{{copy}}","overrides":{},"folder":{"use":"fresh-copy"},"handover":{"kind":"form","fields":[{"name":"launch","describe":"the launch description as one JSON value","required":false}]},"at":{"x":0,"y":0}});
    if matches!(case, Case::SecondCopy | Case::Ambiguous) {
        prepare["copies"] = json!(2);
    }
    let mut file = json!({"format":1,"id":"wf_discover_launch","name":"Discover how this repository starts","steps":[prepare,
        {"kind":"serve","id":"s_preview","name":"Start the prepared app","command":format!("touch {}/manual",quote(&control.to_string_lossy())),
        "commandFrom":{"field":"launch","format":"launch-description","producer":if matches!(case,Case::SecondCopy) {"s_prepare~2"} else {"s_prepare"}},"folder":{"use":"same-copy"},"lifetime":"window","at":{"x":0,"y":200}},
        {"kind":"agent","id":"s_qa","name":"Read the app","agent":"01990000-0000-7000-8000-000000002727","instructions":"wf27-consume","overrides":{},"folder":{"use":"fresh-copy"},"at":{"x":0,"y":400}}],
        "links":[{"from":"s_prepare","to":"s_preview"},{"from":"s_preview","to":"s_qa"}]});
    if matches!(case, Case::Grandparent) {
        file["steps"].as_array_mut().expect("fixture has steps").push(json!({
            "kind":"agent","id":"s_middle","name":"Keep preparing","agent":"01990000-0000-7000-8000-000000002727",
            "instructions":"wf27-middle","overrides":{},"folder":{"use":"same-copy"},"at":{"x":200,"y":100}
        }));
        file["links"] = json!([{"from":"s_prepare","to":"s_middle"},{"from":"s_middle","to":"s_preview"},{"from":"s_preview","to":"s_qa"}]);
    }
    if matches!(case, Case::Ambiguous) {
        file["steps"][1]["commandFrom"]
            .as_object_mut()
            .expect("fixture has source")
            .remove("producer");
    }
    file
}

struct Discoverer {
    case: Case,
    consumers: Arc<Mutex<usize>>,
}
#[async_trait]
impl AgentDriver for Discoverer {
    fn id(&self) -> &'static str {
        "claude-code"
    }
    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("wf27-test".to_owned()),
        })
    }
    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let mut body = String::new();
        if spec.prompt.contains("wf27-discover") {
            let mut launch: Value =
                serde_json::from_slice(&fs::read(spec.cwd.join("project-start.json"))?)?;
            fs::write(
                spec.cwd.join("packages/web/prepared.txt"),
                "prepared by the repository agent\n",
            )?;
            if matches!(self.case, Case::SecondCopy) && spec.prompt.contains("copy2") {
                launch["environment"]["WF27_MODE"] = json!("second-copy");
            }
            match self.case {
                Case::EscapingFolder => launch["subdirectory"] = json!("../outside"),
                Case::MissingEnvironment => {
                    launch["requiredEnv"] = json!(["WF27_REQUIRED_BUT_ABSENT"]);
                }
                Case::Secret => {
                    launch["command"] =
                        json!("sh start-one.sh --token=ghp_0123456789abcdefghijklmnopqrstuvwxyzA");
                }
                _ => {}
            }
            if !matches!(self.case, Case::MissingField) {
                body = format!("launch: {launch}\n");
            }
        } else if spec.prompt.contains("wf27-consume") {
            *self
                .consumers
                .lock()
                .unwrap_or_else(PoisonError::into_inner) += 1;
        }
        body.push_str("## Answer\nRead the repository manifest and prepared its working folder.\n\n## Evidence\nThe actual process will be checked by Loadout.\n\n## Open questions\nNone.\n");
        let session = SessionRef {
            vendor: "claude-code",
            id: spec.run_id.to_string(),
        };
        let _ = events
            .send(
                AgentEvent::Started {
                    session: session.clone(),
                    model: spec.model.unwrap_or_default(),
                    tools: Vec::new(),
                    capabilities: Vec::new(),
                }
                .into(),
            )
            .await;
        Ok(Box::new(Turn {
            session,
            events,
            body,
        }))
    }
}
struct Turn {
    session: SessionRef,
    events: mpsc::Sender<DecodedEvent>,
    body: String,
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
            text: self.body.clone(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session.clone(),
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
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}
fn step<'a>(book: &'a Value, node: &str) -> &'a Value {
    book["steps"]
        .as_array()
        .and_then(|steps| steps.iter().find(|step| step["node_key"] == node))
        .unwrap_or(&Value::Null)
}
