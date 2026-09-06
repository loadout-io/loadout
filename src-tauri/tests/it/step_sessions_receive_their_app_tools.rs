//! WF-28: zwykły graf konfiguruje usługę i przekazuje narzędzia obu sesjom.
//! Dubler zastępuje model, nie konfigurację vendora, host ani proces usługi.
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::bridge::{Answer, Call, Greeting, Reply};
use loadout_lib::commands::processes::Processes;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, DriverConfiguration, FinishReason, Outcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::supervisor::{self, GroupId, GroupProof};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::{AppState, line_channel};
use loadout_lib::library::agents::{
    Agent, ServiceGrant, ServiceOperation, Vendor, write_agent_file,
};
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_claude_step_uses_its_apps_through_the_configured_host() -> Result<(), Box<dyn Error>> {
    run(Vendor::ClaudeCode, "claude").await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_codex_step_uses_its_apps_through_the_configured_host() -> Result<(), Box<dyn Error>> {
    run(Vendor::Codex, "codex").await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_claude_lead_uses_the_apps_from_the_actual_application_registry()
-> Result<(), Box<dyn Error>> {
    run_lead(Vendor::ClaudeCode, "claude").await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_codex_lead_uses_the_apps_from_the_actual_application_registry()
-> Result<(), Box<dyn Error>> {
    run_lead(Vendor::Codex, "codex").await
}

#[derive(Default, Debug)]
struct Observed {
    calls: Vec<(String, Answer)>,
    missing: Option<String>,
    groups: Vec<i32>,
    reached_web: bool,
}

async fn run(vendor: Vendor, driver_id: &'static str) -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let library = tempfile::tempdir()?;
    fs::create_dir_all(project.path().join(".loadout"))?;
    let mut agent = Agent::example();
    agent.runs_with = vendor;
    agent.reaches_the_web = false;
    agent.service_access = vec![ServiceGrant {
        service: "s_preview".to_owned(),
        operations: vec![
            ServiceOperation::Read,
            ServiceOperation::Start,
            ServiceOperation::Restart,
            ServiceOperation::Stop,
        ],
    }];
    write_agent_file(&library.path().join("agents"), &agent, None)?;
    let workflow = library.path().join("apps.json");
    fs::write(
        &workflow,
        serde_json::to_vec(&json!({
            "format":1,"id":"wf-step-apps","name":"Use the prepared app",
            "links":[{"from":"s_preview","to":"s_qa"}],
            "steps":[
                {"kind":"serve","id":"s_preview","name":"Prepared app",
                 "command":"printf 'app marker\\n'; sleep 10", "folder":{"use":"fresh-copy"},
                 "startWhen":"asked","lifetime":"window","at":{"x":0,"y":0}},
                {"kind":"agent","id":"s_qa","name":"Use app tools","agent":agent.id,
                 "instructions":"Use the configured app tools.","overrides":{},
                 "folder":{"use":"fresh-copy"},"at":{"x":200,"y":0}}
            ]
        }))?,
    )?;
    let observed = Arc::new(Mutex::new(Observed::default()));
    let model: Arc<dyn AgentDriver> = Arc::new(Model {
        id: driver_id,
        lead: false,
        configuration: DriverConfiguration::default(),
        observed: Arc::clone(&observed),
    });
    let drivers: Drivers = Arc::new(move |_| Arc::clone(&model));
    let processes = Arc::new(Processes::new());
    let store = Store::open(&project.path().join(".loadout/loadout.db"))?;
    let deps = RunDeps {
        home: library.path(),
        project: project.path(),
        store: &store,
        drivers,
        processes: Arc::clone(&processes),
        control: RunControl::new(),
    };
    let (sink, _lines) = line_channel(512);
    let ran = run_workflow_inner(
        &deps,
        &RunRequest {
            workflow,
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        },
        sink,
    )
    .await;
    // Każda droga, również błąd modelowego protokołu, kończy prawdziwe procesy przed asercją.
    let active: Vec<_> = processes.list().iter().map(|one| one.pgid).collect();
    let cleanup = processes.close().await;
    assert!(
        cleanup
            .iter()
            .all(|one| matches!(one, GroupProof::Dead { .. })),
        "cleanup: {cleanup:?}"
    );
    assert!(active.iter().all(|pgid| supervisor::group_is_empty(*pgid)));
    let report = ran?;
    let book: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    let observed = observed.lock().unwrap_or_else(PoisonError::into_inner);
    assert!(
        observed
            .groups
            .iter()
            .all(|pgid| supervisor::group_is_empty(*pgid))
    );
    assert!(
        observed.missing.is_none(),
        "{driver_id}: {:?}; history: {book}",
        observed.missing
    );
    assert_eq!(
        observed.calls.len(),
        7,
        "{driver_id}: no complete app interaction: {observed:?}; {book}"
    );
    for (name, answer) in &observed.calls {
        if name == "list_workflows" {
            assert!(
                matches!(answer, Answer::Refused(_)),
                "Step received a Lead capability: {answer:?}"
            );
        } else {
            assert!(
                matches!(answer, Answer::Ok(_)),
                "{driver_id} {name}: {answer:?}"
            );
        }
    }
    assert!(
        !observed.reached_web,
        "app tools silently enabled access to the Internet"
    );
    assert_eq!(
        observed.groups.len(),
        2,
        "Start and Restart did not create their actual processes"
    );
    Ok(())
}

async fn run_lead(vendor: Vendor, driver_id: &'static str) -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let library = tempfile::tempdir()?;
    fs::create_dir_all(project.path().join(".loadout"))?;
    let mut agent = Agent::example();
    agent.runs_with = vendor;
    agent.reaches_the_web = false;
    agent.service_access = vec![ServiceGrant {
        service: "s_preview".to_owned(),
        operations: vec![ServiceOperation::Read],
    }];
    write_agent_file(&library.path().join("agents"), &agent, None)?;
    let workflow = library.path().join("lead-apps.json");
    fs::write(
        &workflow,
        serde_json::to_vec(&json!({
            "format":1,"id":"wf-lead-apps","name":"An app for the Lead","links":[],
            "steps":[{"kind":"serve","id":"s_preview","name":"Preview",
                "command":"printf 'app marker\\n'; sleep 10","folder":{"use":"fresh-copy"},
                "lifetime":"window","at":{"x":0,"y":0}}]
        }))?,
    )?;
    let observed = Arc::new(Mutex::new(Observed::default()));
    let model: Arc<dyn AgentDriver> = Arc::new(Model {
        id: driver_id,
        lead: true,
        configuration: DriverConfiguration::default(),
        observed: Arc::clone(&observed),
    });
    let drivers: Drivers = Arc::new(move |_| Arc::clone(&model));
    let store = Store::open(&project.path().join(".loadout/loadout.db"))?;
    let state = AppState::new(
        library.path().to_path_buf(),
        project.path().to_path_buf(),
        store,
        drivers,
    );
    let processes = state.deps().processes;
    let (sink, _run_lines) = line_channel(256);
    let (lead_sink, mut lead_lines) = line_channel(256);
    let folder = project.path().to_string_lossy().into_owned();
    let exercised = async {
        // The real window opens/reconciles this project before its first Start.
        // Doing that after a direct run would correctly reap it as a leftover.
        state
            .watching_the_lead("apps", Some(&folder), lead_sink)
            .await?;
        let report = run_workflow_inner(
            &state.deps(),
            &RunRequest {
                workflow,
                how_many_at_once: 1,
                task: None,
                part: None,
                handoffs_from: None,
            },
            sink,
        )
        .await?;
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if processes.list().iter().any(|one| {
                    processes
                        .said(one.pgid)
                        .is_some_and(|text| text.contains("app marker"))
                }) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await?;
        state
            .say_to_the_lead(
                "apps",
                Some(&folder),
                Some(&agent.id.to_string()),
                "Read this app's status and logs.",
            )
            .await?;
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let done = {
                    let observed = observed.lock().unwrap_or_else(PoisonError::into_inner);
                    observed.calls.len() >= 3 || observed.missing.is_some()
                };
                if done {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await?;
        Ok::<_, Box<dyn Error>>(report)
    }
    .await;
    // Closing a conversation must not stop a Window app owned by the actual run.
    let alive_before_close: Vec<_> = processes
        .list()
        .iter()
        .filter(|one| !supervisor::group_is_empty(one.pgid))
        .map(|one| one.pgid)
        .collect();
    state.close_the_lead("apps").await;
    let groups: Vec<_> = processes.list().iter().map(|one| one.pgid).collect();
    let proofs = state.close_started().await;
    assert!(
        proofs
            .iter()
            .all(|proof| matches!(proof, GroupProof::Dead { .. })),
        "fixture cleanup: {proofs:?}"
    );
    assert!(groups.iter().all(|pgid| supervisor::group_is_empty(*pgid)));
    let report = exercised?;
    let observed = observed.lock().unwrap_or_else(PoisonError::into_inner);
    assert!(
        observed.missing.is_none(),
        "{driver_id}: actual Lead host was missing: {observed:?}"
    );
    assert_eq!(
        observed.calls.len(),
        3,
        "{driver_id}: incomplete Lead host route: {observed:?}"
    );
    assert!(!observed.reached_web, "app tools enabled Internet access");
    assert_eq!(
        alive_before_close.len(),
        1,
        "the Window app was already absent before closing the Lead"
    );
    assert_eq!(groups.len(), 1, "closing the Lead stopped its Window app");
    assert_eq!(
        groups, alive_before_close,
        "closing the Lead changed the actual app process"
    );
    let (_, status) = &observed.calls[0];
    assert!(
        matches!(status, Answer::Ok(value) if value["services"][0]["service"]["run_id"] == report.id),
        "Lead read a registry other than the actual run's: {status:?}"
    );
    let (_, logs) = &observed.calls[1];
    assert!(
        matches!(logs, Answer::Ok(value) if value["text"].as_str().is_some_and(|text| text.contains("app marker"))),
        "Lead did not read real process output: {logs:?}"
    );
    let (_, mutation) = &observed.calls[2];
    let Answer::Refused(said) = mutation else {
        return Err("read-only Lead called an ungranted Start".into());
    };
    let mut lines = Vec::new();
    while let Some(line) = lead_lines.try_next() {
        lines.push(line);
    }
    // The host refuses hidden tools before dispatch; a direct malicious call need not
    // manufacture a model transcript row, but its exact wire refusal must reach the caller.
    assert!(!said.is_empty());
    assert!(
        lines.iter().any(|line| matches!(
            line,
            Line::Note { .. } | Line::Told { .. } | Line::Done { .. }
        )),
        "the actual conversation never appeared to the person"
    );
    Ok(())
}

#[derive(Clone)]
struct Model {
    id: &'static str,
    lead: bool,
    configuration: DriverConfiguration,
    observed: Arc<Mutex<Observed>>,
}

impl Model {
    fn socket(&self) -> anyhow::Result<Option<PathBuf>> {
        if self.id == "claude" {
            let Some(index) = self
                .configuration
                .arguments
                .iter()
                .position(|arg| arg == "--mcp-config")
            else {
                return Ok(None);
            };
            let path = self
                .configuration
                .arguments
                .get(index + 1)
                .ok_or_else(|| anyhow::anyhow!("missing MCP file argument"))?;
            let value: Value = serde_json::from_slice(&fs::read(path)?)?;
            return Ok(value["mcpServers"]["loadout"]["args"]
                .as_array()
                .and_then(|args| args.last())
                .and_then(Value::as_str)
                .map(PathBuf::from));
        }
        let Some(arg) = self
            .configuration
            .arguments
            .iter()
            .find(|arg| arg.starts_with("mcp_servers.loadout.args="))
        else {
            return Ok(None);
        };
        let (_, value) = arg
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("missing MCP args"))?;
        let args: Vec<String> = serde_json::from_str(value)?;
        Ok(args.last().map(PathBuf::from))
    }

    async fn call(&self, socket: &Path, name: &str, input: Value) -> anyhow::Result<Value> {
        let answer = socket_call(socket, name, input).await?;
        let value = match &answer {
            Answer::Ok(value) => value.clone(),
            _ => Value::Null,
        };
        self.observed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .calls
            .push((name.to_owned(), answer));
        Ok(value)
    }

    async fn use_apps(&self) -> anyhow::Result<()> {
        let socket = self
            .socket()?
            .ok_or_else(|| anyhow::anyhow!("the ordinary Step did not receive its app bridge"))?;
        let all = self.call(&socket, "service_status", json!({})).await?;
        let reference = all["services"][0]["service"].clone();
        if self.lead {
            self.call(
                &socket,
                "service_logs",
                json!({"service":reference,"limit":4096}),
            )
            .await?;
            self.call(
                &socket,
                "service_start",
                json!({"service":reference,"confirmed":true}),
            )
            .await?;
            return Ok(());
        }
        anyhow::ensure!(
            reference.is_object(),
            "the Step did not see its configured app: {all}"
        );
        let started = self
            .call(&socket, "service_start", json!({"service":reference}))
            .await?;
        if let Some(pgid) = started["pgid"].as_i64() {
            self.observed
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .groups
                .push(pgid as i32);
        }
        self.call(&socket, "service_status", json!({"service":reference}))
            .await?;
        self.call(
            &socket,
            "service_logs",
            json!({"service":reference,"limit":128}),
        )
        .await?;
        let restarted = self
            .call(&socket, "service_restart", json!({"service":reference}))
            .await?;
        if let Some(pgid) = restarted["pgid"].as_i64() {
            self.observed
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .groups
                .push(pgid as i32);
        }
        self.call(
            &socket,
            "service_stop",
            json!({"service":restarted["service"]}),
        )
        .await?;
        self.call(&socket, "list_workflows", json!({})).await?;
        Ok(())
    }
}

#[async_trait]
impl AgentDriver for Model {
    fn id(&self) -> &'static str {
        self.id
    }
    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("wf28-model-double".to_owned()),
        })
    }
    fn configured(&self, configuration: &DriverConfiguration) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            configuration: configuration.clone(),
            ..self.clone()
        }))
    }
    fn with_evidence(&self, _: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }
    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.observed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .reached_web = spec.reaches_the_web;
        if let Err(why) = self.use_apps().await {
            self.observed
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .missing = Some(why.to_string());
        }
        Ok(Box::new(Turn {
            session: SessionRef {
                vendor: self.id,
                id: spec.run_id.to_string(),
            },
            events,
        }))
    }
}

struct Turn {
    session: SessionRef,
    events: mpsc::Sender<DecodedEvent>,
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
            ok:true, reason:FinishReason::Completed,
            text:"## Answer\nThe app tools were exercised.\n\n## Evidence\nThe test checks the real host and process groups.\n\n## Open questions\nNone.\n".to_owned(),
            cost_usd:None, tokens:Tokens::default(), turns:1, took:Duration::from_millis(1), session:self.session.clone(),
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

async fn socket_call(socket: &Path, name: &str, input: Value) -> anyhow::Result<Answer> {
    tokio::time::timeout(Duration::from_secs(3), async {
        let connection = UnixStream::connect(socket).await?;
        let (reader, mut writer) = connection.into_split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        let _: Greeting = serde_json::from_str(&line)?;
        writer
            .write_all(
                format!(
                    "{}\n",
                    serde_json::to_string(&Call {
                        id: json!(28),
                        call: name.to_owned(),
                        input
                    })?
                )
                .as_bytes(),
            )
            .await?;
        writer.flush().await?;
        line.clear();
        reader.read_line(&mut line).await?;
        let reply: Reply = serde_json::from_str(&line)?;
        Ok::<_, anyhow::Error>(reply.answer)
    })
    .await?
}
