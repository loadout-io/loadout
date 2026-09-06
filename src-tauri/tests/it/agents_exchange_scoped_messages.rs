//! WF-11: dwaj vendorzy, rzeczywiste konfiguracje MCP i host, nie bezpośredni mailbox-helper.
use async_trait::async_trait;
use loadout_lib::{
    bridge::{Answer, Call, Greeting, Reply},
    commands::{Drivers, RunControl, RunDeps, RunRequest},
    commands::{processes::Processes, run::run_workflow_inner},
    engine::{
        drivers::{
            AgentDriver, AgentEvent, AgentHandle, DecodedEvent, DriverConfiguration, FinishReason,
            Outcome, Probe, RunSpec, SessionRef, Tokens,
        },
        supervisor::{GroupId, GroupProof},
    },
    evidence::EvidenceTarget,
    ipc::line_channel,
    library::agents::{Agent, Vendor, agent_file_name, write_agent_file},
    store::Store,
};
use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    sync::{Barrier, mpsc},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_steps_store_once_and_read_only_their_own_scope() -> Result<(), Box<dyn Error>> {
    run_messages(1, 64, false).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn default_off_steps_run_without_a_message_host_or_saved_inbox() -> Result<(), Box<dyn Error>>
{
    run_messages(0, 64, false).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inbox_pages_are_bounded_and_reading_does_not_remove_entries() -> Result<(), Box<dyn Error>>
{
    run_messages(102, 64, false).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_thousandth_entry_is_the_last_one_this_run_can_store() -> Result<(), Box<dyn Error>> {
    run_messages(500, 64, true).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn eight_mib_of_message_text_is_a_real_storage_bound() -> Result<(), Box<dyn Error>> {
    run_messages(256, 16 * 1024, true).await
}

async fn run_messages(burst: usize, body_size: usize, full: bool) -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let library = tempfile::tempdir()?;
    fs::create_dir_all(project.path().join(".loadout"))?;
    let mut steps = Vec::new();
    for (key, vendor) in [
        ("builder", Vendor::ClaudeCode),
        ("reviewer", Vendor::Codex),
        ("isolated", Vendor::Codex),
    ] {
        let mut agent = Agent::example();
        agent.id = uuid::Uuid::now_v7();
        agent.name = key.to_owned();
        agent.runs_with = vendor;
        write_agent_file(&library.path().join("agents"), &agent, None)?;
        let path = library.path().join("agents").join(agent_file_name(&agent));
        let text = fs::read_to_string(&path)?;
        if burst > 0 {
            fs::write(
                &path,
                text.replacen("---\n", "---\nagentMessages: true\n", 1),
            )?;
        }
        steps.push(json!({"kind":"agent","id":key,"name":key,"agent":agent.id,"instructions":"Use the allowed messages.",
            "folder":{"use":"fresh-copy"},"overrides":{},"at":{"x":0,"y":0}}));
    }
    let workflow = library.path().join("messages.json");
    fs::write(
        &workflow,
        serde_json::to_vec(
            &json!({"format":1,"id":"wf-messages","name":"Messages are optional",
        "steps":steps,"links":[],"executionInputs":{"schema":1,"contexts":{"together":{"task":"shared input"},"apart":{"task":"other input"}},
        "stepContexts":{"builder":"together","reviewer":"together","isolated":"apart"}}}),
        )?,
    )?;
    let observed = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let started = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let starts = Arc::clone(&started);
    let ready = Arc::new(Barrier::new(3));
    let sent = Arc::new(Barrier::new(3));
    let checked = Arc::new(Barrier::new(3));
    let observations = Arc::clone(&observed);
    let drivers: Drivers = Arc::new(move |vendor| {
        Arc::new(Model {
            id: match vendor {
                Vendor::ClaudeCode => "claude",
                Vendor::Codex => "codex",
            },
            configuration: DriverConfiguration::default(),
            observed: Arc::clone(&observations),
            ready: Arc::clone(&ready),
            sent: Arc::clone(&sent),
            checked: Arc::clone(&checked),
            burst,
            body_size,
            full,
            started: Arc::clone(&starts),
        })
    });
    let store = Store::open(&project.path().join(".loadout/loadout.db"))?;
    let deps = RunDeps {
        home: library.path(),
        project: project.path(),
        store: &store,
        drivers,
        processes: Arc::new(Processes::new()),
        control: RunControl::new(),
    };
    let (sink, mut lines) = line_channel(4096);
    let report = tokio::time::timeout(
        Duration::from_mins(1),
        run_workflow_inner(
            &deps,
            &RunRequest {
                workflow,
                how_many_at_once: 3,
                task: None,
                part: None,
                handoffs_from: None,
            },
            sink,
        ),
    )
    .await??;
    assert_eq!(
        started.load(std::sync::atomic::Ordering::SeqCst),
        3,
        "the test did not execute all ordinary step sessions"
    );
    let errors = observed.lock().unwrap_or_else(PoisonError::into_inner);
    assert!(
        errors.is_empty(),
        "the real step host did not complete addressed messaging: {errors:?}"
    );
    if burst == 0 {
        assert!(
            !report.dir.join("messages/index.json").exists(),
            "default-off created a durable inbox"
        );
    } else {
        let durable: Value =
            serde_json::from_slice(&fs::read(report.dir.join("messages/index.json"))?)?;
        assert!(
            durable.get("messages").is_none(),
            "the header must not rewrite every message body"
        );
        let entries = fs::read_dir(report.dir.join("messages"))?.collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.file_name() != "index.json")
                .count(),
            2 * burst,
            "retries created extra stored messages"
        );
        for sequence in 1..=2 * burst {
            let saved: Value = serde_json::from_slice(&fs::read(
                report.dir.join(format!("messages/{sequence:06}.json")),
            )?)?;
            assert_eq!(saved["message"]["sequence"], sequence);
            assert_eq!(saved["run"], durable["run"]);
        }
    }
    let mut visible = 0;
    while let Some(line) = lines.try_next() {
        if serde_json::to_value(line)?["kind"] == "messageStored" {
            visible += 1;
        }
    }
    assert_eq!(
        visible,
        2 * burst,
        "one durable message must produce one human-visible fact, not a read receipt"
    );
    let run_name = report
        .dir
        .file_name()
        .and_then(|one| one.to_str())
        .ok_or("missing run folder")?;
    let reopened = loadout_lib::commands::history::read_run_inner(project.path(), run_name)?;
    assert_eq!(
        reopened
            .steps
            .iter()
            .flat_map(|step| &step.lines)
            .filter(
                |line| serde_json::to_value(line).is_ok_and(|one| one["kind"] == "messageStored")
            )
            .count(),
        2 * burst,
        "history must read stored messages after all runtime sessions have gone"
    );
    Ok(())
}

#[derive(Clone)]
struct Model {
    id: &'static str,
    configuration: DriverConfiguration,
    observed: Arc<Mutex<Vec<(String, String)>>>,
    ready: Arc<Barrier>,
    sent: Arc<Barrier>,
    checked: Arc<Barrier>,
    burst: usize,
    body_size: usize,
    full: bool,
    started: Arc<std::sync::atomic::AtomicUsize>,
}
impl Model {
    fn socket(&self) -> anyhow::Result<PathBuf> {
        if self.id == "claude" {
            let index = self
                .configuration
                .arguments
                .iter()
                .position(|arg| arg == "--mcp-config")
                .ok_or_else(|| {
                    anyhow::anyhow!("the Claude step did not receive its message host")
                })?;
            let value: Value =
                serde_json::from_slice(&fs::read(&self.configuration.arguments[index + 1])?)?;
            return value["mcpServers"]["loadout"]["args"]
                .as_array()
                .and_then(|args| args.last())
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!("no Claude message socket"));
        }
        let value = self
            .configuration
            .arguments
            .iter()
            .find_map(|arg| arg.strip_prefix("mcp_servers.loadout.args="))
            .ok_or_else(|| anyhow::anyhow!("the Codex step did not receive its message host"))?;
        let args: Vec<String> = serde_json::from_str(value)?;
        args.last()
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("no Codex message socket"))
    }
    async fn send(&self, name: &str) -> anyhow::Result<()> {
        let socket = self.socket()?;
        let peers = ok(call(&socket, "list_peers", json!({})).await?)?;
        let peers = peers["peers"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("missing peers: {peers}"))?;
        if name == "isolated" {
            anyhow::ensure!(peers.is_empty(), "another context became a recipient");
            return Ok(());
        }
        anyhow::ensure!(
            peers.len() == 1,
            "only a live peer from the same scope may be listed: {peers:?}"
        );
        let peer = peers[0]["peer"].clone();
        anyhow::ensure!(peer.is_object(), "host omitted the exact attempt identity");
        let mut body = format!("A private note from {name}.");
        body.push_str(&"x".repeat(self.body_size.saturating_sub(body.len())));
        let input = json!({"to":peer,"client_id":"retry-the-same-send","text":body});
        let first = ok(call(&socket, "send_message", input.clone()).await?)?;
        anyhow::ensure!(
            first["status"] == "stored",
            "storing must not claim reading: {first}"
        );
        let again = ok(call(&socket, "send_message", input.clone()).await?)?;
        anyhow::ensure!(
            again["sequence"] == first["sequence"],
            "retry did not preserve its durable identity"
        );
        for number in 1..self.burst {
            let mut next = input.clone();
            next["client_id"] = json!(format!("message-{number}"));
            let stored = ok(call(&socket, "send_message", next).await?)?;
            anyhow::ensure!(
                stored["status"] == "stored",
                "a message disappeared before the configured storage bound"
            );
        }
        let mut changed = input.clone();
        changed["text"] = json!("a different body");
        anyhow::ensure!(
            matches!(
                call(&socket, "send_message", changed).await?,
                Answer::Refused(_)
            ),
            "same client ID silently changed its body"
        );
        let mut oversized = input.clone();
        oversized["client_id"] = json!("too-large");
        oversized["text"] = json!("x".repeat(16 * 1024 + 1));
        anyhow::ensure!(
            matches!(
                call(&socket, "send_message", oversized).await?,
                Answer::Refused(_)
            ),
            "an oversized body was shortened or stored"
        );
        for (field, value) in [
            ("runId", "another-run"),
            ("nodeKey", "a-different-copy"),
            ("attempt", "a-later-attempt"),
        ] {
            let mut other = input.clone();
            other["client_id"] = json!(format!("wrong-{field}"));
            other["to"][field] = json!(value);
            anyhow::ensure!(
                matches!(
                    call(&socket, "send_message", other).await?,
                    Answer::Refused(_)
                ),
                "an exact recipient component was ignored: {field}"
            );
        }
        let mut forged = input;
        forged["from"] = json!("isolated");
        anyhow::ensure!(
            matches!(
                call(&socket, "send_message", forged).await?,
                Answer::Refused(_)
            ),
            "the model forged a sender"
        );
        anyhow::ensure!(
            matches!(
                call(&socket, "list_workflows", json!({})).await?,
                Answer::Refused(_)
            ),
            "Step acquired Lead rights"
        );
        Ok(())
    }
    async fn read(&self, name: &str) -> anyhow::Result<()> {
        let socket = self.socket()?;
        let mut messages = Vec::new();
        let mut after = 0_u64;
        let mut first_page = None;
        loop {
            let inbox = ok(call(&socket, "read_messages", json!({"after_sequence":after})).await?)?;
            let page = inbox["messages"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("missing inbox"))?;
            anyhow::ensure!(
                page.len() <= 100,
                "the host returned an unbounded inbox page"
            );
            if first_page.is_none() {
                first_page = Some(inbox["messages"].clone());
            }
            messages.extend(page.iter().cloned());
            let next = inbox["afterSequence"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("no continuation cursor"))?;
            if inbox["more"] == false {
                break;
            }
            anyhow::ensure!(
                next > after && page.len() == 100,
                "the page cannot make forward progress"
            );
            after = next;
        }
        let again = ok(call(&socket, "read_messages", json!({"after_sequence":0})).await?)?;
        anyhow::ensure!(
            Some(again["messages"].clone()) == first_page,
            "reading removed or changed stored entries"
        );
        anyhow::ensure!(
            messages.len() == if name == "isolated" { 0 } else { self.burst },
            "another inbox was read: {messages:?}"
        );
        for message in messages {
            anyhow::ensure!(
                message["to"]["nodeKey"].as_str() == Some(name),
                "foreign recipient: {message}"
            );
        }
        anyhow::ensure!(
            matches!(
                call(&socket, "read_messages", json!({"recipient":"isolated"})).await?,
                Answer::Refused(_)
            ),
            "scope membership granted another inbox"
        );
        Ok(())
    }
    async fn check_full(&self, name: &str) -> anyhow::Result<()> {
        if !self.full || name == "isolated" {
            return Ok(());
        }
        let socket = self.socket()?;
        let peers = ok(call(&socket, "list_peers", json!({})).await?)?;
        let answer=call(&socket,"send_message",json!({"to":peers["peers"][0]["peer"],"client_id":"past-the-storage-bound","text":"one more byte"})).await?;
        anyhow::ensure!(
            matches!(answer,Answer::Refused(ref text) if text.contains("storage is full")),
            "the host did not enforce its actual storage bound: {answer:?}"
        );
        Ok(())
    }
    async fn check_finished_recipient(&self, name: &str) -> anyhow::Result<()> {
        if self.burst != 1 || name != "builder" {
            return Ok(());
        }
        let socket = self.socket()?;
        let inbox = ok(call(&socket, "read_messages", json!({})).await?)?;
        let peer = inbox["messages"][0]["from"].clone();
        anyhow::ensure!(
            peer["nodeKey"] == "reviewer",
            "the live exchange did not preserve the reviewer's exact address"
        );
        // Reviewer kończy zwykłe AgentHandle::wait/close. Nie anulujemy tokenu w dublerze:
        // produkcyjny koniec kroku musi sam usunąć ten konkretny adres z listy.
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let peers = ok(call(&socket, "list_peers", json!({})).await?)?;
                if peers["peers"].as_array().is_some_and(Vec::is_empty) {
                    return Ok::<_, anyhow::Error>(());
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("the finished reviewer's exact address stayed available"))??;
        let answer = call(&socket, "send_message", json!({
            "to":peer,"client_id":"finished-recipient","text":"Do not deliver this to a future try."
        })).await?;
        anyhow::ensure!(
            matches!(answer, Answer::Refused(ref text) if text.contains("recipient is unavailable")),
            "a finished recipient accepted a new message: {answer:?}"
        );
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
            version: Some("message-model-double".to_owned()),
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
        self.started
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.burst == 0 {
            if self.socket().is_ok() {
                self.observed
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push((
                        "default-off".to_owned(),
                        "an ordinary step received message tools".to_owned(),
                    ));
            }
            return Ok(Box::new(Turn {
                session: SessionRef {
                    vendor: self.id,
                    id: spec.run_id.to_string(),
                },
                events,
            }));
        }
        let name = if spec.prompt.contains("other input") {
            "isolated"
        } else if self.id == "claude" {
            "builder"
        } else {
            "reviewer"
        };
        self.ready.wait().await;
        if let Err(error) = self.send(name).await {
            self.observed
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((name.to_owned(), error.to_string()));
        }
        self.sent.wait().await;
        if let Err(error) = self.check_full(name).await {
            self.observed
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((name.to_owned(), error.to_string()));
        }
        self.checked.wait().await;
        if let Err(error) = self.read(name).await {
            self.observed
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((name.to_owned(), error.to_string()));
        }
        if let Err(error) = self.check_finished_recipient(name).await {
            self.observed
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((name.to_owned(), error.to_string()));
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
        let outcome=Outcome{ok:true,reason:FinishReason::Completed,text:"## Answer\nMessage test completed.\n\n## Evidence\nThe host is checked by the test.\n\n## Open questions\nNone.".to_owned(),cost_usd:None,tokens:Tokens::default(),turns:1,took:Duration::from_millis(1),session:self.session.clone()};
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
fn ok(answer: Answer) -> anyhow::Result<Value> {
    match answer {
        Answer::Ok(value) => Ok(value),
        other => anyhow::bail!("message refused: {other:?}"),
    }
}
async fn call(socket: &Path, name: &str, input: Value) -> anyhow::Result<Answer> {
    tokio::time::timeout(Duration::from_secs(2), async {
        let stream = UnixStream::connect(socket).await?;
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        let _: Greeting = serde_json::from_str(&line)?;
        writer
            .write_all(
                format!(
                    "{}\n",
                    serde_json::to_string(&Call {
                        id: json!(11),
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
