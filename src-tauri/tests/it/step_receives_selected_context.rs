//! CT-06: wybrane Context dociera do procesu przez finalny prompt i most tego kroku.

#![allow(clippy::too_many_lines, clippy::struct_field_names)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use loadout_lib::bridge::{Answer, Call, Greeting, Reply};
use loadout_lib::commands::context::{create_context_set_inner, save_context_draft_inner};
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::context::files::{folder_of, library_root};
use loadout_lib::context::{
    ContextDraft, ContextFinding, ContextRevision, ContextSource, ContextTopic, FindingKind,
    Origin, SourceKind, SourceReference,
};
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, DriverConfiguration, FinishReason, Outcome,
    PreparedProtectedStep, Probe, RunSpec, SessionRef, StepSettings, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{FilesystemFence, GroupId, GroupProof};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::line_channel;
use loadout_lib::library::agents::{Agent, Vendor, write_agent_file};
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{Barrier, mpsc};

pub(super) const PATIENCE: Duration = Duration::from_secs(45);
pub(super) const ANSWER: &str = "## Answer\nThe supplied material was read.\n\n## Evidence\nThe controlled process called the real bridge.\n\n## Open\nNone.\n";

const CLAUDE_BRIDGE_OUTPUT: &str = concat!(
    r#"{"type":"system","subtype":"init","session_id":"01990000-0000-7000-8000-000000000606","model":"sonnet","tools":[]}"#,
    "\n",
    r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"The frozen reference material was read through the bridge."}]}}"#,
    "\n",
    r#"{"type":"result","subtype":"success","is_error":false,"num_turns":1,"duration_ms":7,"total_cost_usd":0.001,"result":"The frozen reference material was read through the bridge."}"#,
    "\n",
);

const CLAUDE_BRIDGE_FAKE: &str = r#"#!/bin/sh
if [ "${1-}" = "--version" ]; then
  printf '%s\n' '2.1.263 (Claude Code)'
  exit 0
fi
here="$(dirname "$0")"
config=''
while [ "$#" -gt 0 ]; do
  if [ "$1" = '--mcp-config' ]; then
    shift
    config="$1"
  fi
  shift
done
IFS= read -r first_turn
printf '%s\n' "$first_turn" > "$here/bridge.stdin.log"
command="$(/usr/bin/awk -F'"' '/"command":/ { print $4; exit }' "$config")"
socket="$(/usr/bin/awk -F'"' 'found { print $2; exit } /"--bridge"/ { found=1 }' "$config")"
{
  printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
  printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}'
  printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
  printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_context","arguments":{}}}'
  printf '%s\n' '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"read_context","arguments":{"id":"__READ_ID__"}}}'
  printf '%s\n' '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"list_workflows","arguments":{}}}'
} | "$command" --bridge "$socket" > "$here/bridge.replies.jsonl"
cat "$here/bridge.stdout.jsonl"
exit 0
"#;

#[derive(Clone, Debug)]
pub(super) struct Published {
    pub set_id: String,
    pub revision: String,
    pub topic_id: String,
    pub source_id: String,
}

#[derive(Clone, Debug)]
pub(super) struct Visit {
    pub marker: String,
    pub prompt: String,
    pub system_append: Option<String>,
    pub tools: Vec<String>,
    pub item_ids: Vec<String>,
    pub addresses: Vec<String>,
    pub read: String,
    pub foreign: Option<Answer>,
    pub extra_dirs: Vec<PathBuf>,
    pub entered: Instant,
    pub left: Instant,
}

#[derive(Debug, Default)]
pub(super) struct Seen {
    visits: Mutex<Vec<Visit>>,
    errors: Mutex<Vec<String>>,
    pub started: AtomicUsize,
}

impl Seen {
    fn visit(&self, visit: Visit) {
        self.visits
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(visit);
    }

    fn error(&self, error: impl Into<String>) {
        self.errors
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(error.into());
    }

    pub fn visits(&self) -> Vec<Visit> {
        self.visits
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn errors(&self) -> Vec<String> {
        self.errors
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

pub(super) struct Bench {
    pub home: TempDir,
    pub project: TempDir,
}

impl Bench {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let home = tempfile::tempdir()?;
        let project = tempfile::tempdir()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(project.path().join("README.md"), "fixture\n")?;
        let mut agent = Agent::example();
        agent.id = uuid::Uuid::parse_str("01990000-0000-7000-8000-000000000606")?;
        "Context reader".clone_into(&mut agent.name);
        agent.runs_with = Vendor::ClaudeCode;
        agent.write_results_to.clear();
        write_agent_file(&home.path().join("agents"), &agent, None)?;
        Ok(Self { home, project })
    }

    pub fn publish(
        &self,
        title: &str,
        purpose: &str,
        topic_id: &str,
        topic_title: &str,
        requirement: &str,
        source_text: &str,
    ) -> Result<Published, Box<dyn Error>> {
        let made = create_context_set_inner(self.home.path(), title)?;
        let source_id = format!("source-{topic_id}");
        let saved = save_context_draft_inner(
            self.home.path(),
            &made.set.id,
            title,
            purpose,
            ContextDraft {
                schema: 1,
                sources: vec![ContextSource {
                    id: source_id.clone(),
                    kind: SourceKind::Text,
                    name: format!("{title} source"),
                    description: purpose.to_owned(),
                    text: source_text.to_owned(),
                    ..ContextSource::default()
                }],
                requirements: vec![requirement.to_owned()],
                ..ContextDraft::default()
            },
            Some(made.revision),
        )?;
        let revision = format!("revision-{topic_id}");
        let folder = folder_of(&library_root(self.home.path()), &saved.set.id)?;
        let version = folder.join("versions").join(&revision);
        fs::create_dir_all(version.join("topics"))?;
        let finding = ContextFinding {
            id: format!("requirement-{topic_id}"),
            kind: FindingKind::Requirement,
            text: requirement.to_owned(),
            condition: format!("while working on {topic_title}"),
            sources: vec![SourceReference {
                source_id: source_id.clone(),
                part: "fragment 1".to_owned(),
            }],
            topic: topic_id.to_owned(),
            origin: Origin::Human,
            ..ContextFinding::default()
        };
        fs::write(
            version.join("findings.json"),
            serde_json::to_vec_pretty(&vec![finding.clone()])?,
        )?;
        fs::write(
            version.join("index.md"),
            format!("# {title}\n\n- {topic_title}\n"),
        )?;
        fs::write(
            version.join("topics").join(format!("{topic_id}.md")),
            format!("# {topic_title}\n\n{requirement}\n\n{source_text}\n"),
        )?;
        let ready = ContextRevision {
            schema: 1,
            id: revision.clone(),
            set_id: saved.set.id.clone(),
            draft_revision: saved.set.draft_revision,
            origin: Origin::Human,
            topics: vec![ContextTopic {
                id: topic_id.to_owned(),
                title: topic_title.to_owned(),
            }],
            findings: vec![finding],
            index_file: "index.md".to_owned(),
            findings_file: "findings.json".to_owned(),
            topic_files: vec![format!("topics/{topic_id}.md")],
            ..ContextRevision::default()
        };
        fs::write(
            version.join("manifest.json"),
            serde_json::to_vec_pretty(&ready)?,
        )?;
        let mut definition = saved.set;
        definition.latest_ready_revision = Some(revision.clone());
        fs::write(
            folder.join("manifest.json"),
            serde_json::to_vec_pretty(&definition)?,
        )?;
        Ok(Published {
            set_id: definition.id,
            revision,
            topic_id: topic_id.to_owned(),
            source_id,
        })
    }

    pub fn workflow(&self, name: &str, value: &Value) -> Result<PathBuf, Box<dyn Error>> {
        let path = self
            .home
            .path()
            .join("workflows")
            .join(format!("{name}.json"));
        fs::write(&path, serde_json::to_vec_pretty(&value)?)?;
        Ok(path)
    }

    pub fn db(&self) -> PathBuf {
        self.project.path().join(".loadout/loadout.db")
    }

    pub fn run_errors(&self) -> Vec<String> {
        fs::read_dir(self.project.path().join(".loadout/runs"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| fs::read(entry.path().join("run.json")).ok())
            .filter_map(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .flat_map(|run| {
                run["steps"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|step| {
                        step["error"].as_str().map(|error| {
                            format!(
                                "status={} cause={} error={error}",
                                step["status"].as_str().unwrap_or("missing"),
                                step["endCause"].as_str().unwrap_or("missing")
                            )
                        })
                    })
            })
            .collect()
    }
}

pub(super) fn pin(material: &Published) -> Value {
    json!({
        "schema": 1,
        "inheritWorkflow": false,
        "exclude": [],
        "sets": [{
            "id": material.set_id,
            "revision": material.revision,
            "topics": [material.topic_id]
        }]
    })
}

pub(super) fn step(id: &str, marker: &str, context: Value) -> Value {
    let mut step = json!({
        "kind": "agent",
        "id": id,
        "name": id,
        "agent": "01990000-0000-7000-8000-000000000606",
        "overrides": {},
        "instructions": marker,
        "context": null,
        "folder": {"use": "fresh-copy"},
        "at": {"x": 0, "y": 0}
    });
    step["context"] = context;
    step
}

pub(super) fn refusal_of<T, E>(result: Result<T, E>, accepted: &str) -> Result<E, Box<dyn Error>> {
    match result {
        Ok(_) => Err(accepted.to_owned().into()),
        Err(error) => Ok(error),
    }
}

pub(super) async fn run(
    bench: &Bench,
    workflow: PathBuf,
    drivers: Drivers,
    how_many_at_once: usize,
) -> Result<loadout_lib::commands::RunReport, Box<dyn Error>> {
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers,
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let (sink, _lines) = line_channel(4096);
    Ok(tokio::time::timeout(
        PATIENCE,
        run_workflow_inner(
            &deps,
            &RunRequest {
                workflow,
                how_many_at_once,
                task: None,
                part: None,
                handoffs_from: None,
            },
            sink,
        ),
    )
    .await
    .map_err(|_| "the run did not finish")??)
}

pub(super) fn drivers(
    seen: Arc<Seen>,
    barrier: Option<Arc<Barrier>>,
    library_change: Option<(PathBuf, PathBuf)>,
    foreign_id: Option<String>,
) -> Drivers {
    Arc::new(move |_vendor| {
        Arc::new(Model {
            configuration: DriverConfiguration::default(),
            seen: Arc::clone(&seen),
            barrier: barrier.clone(),
            library_change: library_change.clone(),
            foreign_id: foreign_id.clone(),
        })
    })
}

fn controlled_bridge_claude(home: &Path, read_id: &str) -> Result<Drivers, Box<dyn Error>> {
    let binary = home.join("context-bridge-claude");
    fs::write(&binary, CLAUDE_BRIDGE_FAKE.replace("__READ_ID__", read_id))?;
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))?;
    fs::write(home.join("bridge.stdout.jsonl"), CLAUDE_BRIDGE_OUTPUT)?;
    let driver: Arc<dyn AgentDriver> = Arc::new(ProductBridgeClaude {
        inner: ClaudeDriver::with_binary(binary),
    });
    Ok(Arc::new(move |_vendor| Arc::clone(&driver)))
}

struct ProductBridgeClaude {
    inner: ClaudeDriver,
}

#[async_trait]
impl AgentDriver for ProductBridgeClaude {
    fn id(&self) -> &'static str {
        self.inner.id()
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        self.inner.probe().await
    }

    fn configured(&self, configuration: &DriverConfiguration) -> Option<Arc<dyn AgentDriver>> {
        (|| {
            let index = configuration
                .arguments
                .iter()
                .position(|argument| argument == "--mcp-config")?;
            let path = configuration.arguments.get(index + 1)?;
            let mut document: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
            // 2026-09-08 (CT-06) — `current_exe()` w celu integracyjnym wskazuje binarkę
            // testów, więc tylko fikstura podmienia ją na produkcyjną binarkę mostu.
            document["mcpServers"]["loadout"]["command"] =
                Value::String(env!("CARGO_BIN_EXE_loadout").to_owned());
            fs::write(path, serde_json::to_vec_pretty(&document).ok()?).ok()?;
            self.inner.configured(configuration)
        })()
    }

    fn effort_argv(&self, level: &str) -> Vec<String> {
        self.inner.effort_argv(level)
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.inner.start(spec, events).await
    }
}

#[derive(Clone, Debug)]
struct Model {
    configuration: DriverConfiguration,
    seen: Arc<Seen>,
    barrier: Option<Arc<Barrier>>,
    library_change: Option<(PathBuf, PathBuf)>,
    foreign_id: Option<String>,
}

impl Model {
    fn socket(&self) -> anyhow::Result<PathBuf> {
        let index = self
            .configuration
            .arguments
            .iter()
            .position(|arg| arg == "--mcp-config")
            .ok_or_else(|| anyhow::anyhow!("the context-only step did not receive a bridge"))?;
        let path = self
            .configuration
            .arguments
            .get(index + 1)
            .ok_or_else(|| anyhow::anyhow!("the bridge configuration path is missing"))?;
        let value: Value = serde_json::from_slice(&fs::read(path)?)?;
        value["mcpServers"]["loadout"]["args"]
            .as_array()
            .and_then(|args| args.last())
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("the context-only bridge has no socket"))
    }

    async fn inspect(&self, spec: &RunSpec, entered: Instant) -> anyhow::Result<Visit> {
        let marker = [
            "CTX-ALPHA",
            "CTX-BETA",
            "CTX-COPY",
            "CTX-RETRY",
            "CTX-JUDGE",
            "CTX-JOIN",
            "CTX-FROZEN",
        ]
        .into_iter()
        .find(|marker| spec.prompt.contains(marker))
        .unwrap_or("unknown")
        .to_owned();
        if let Some(barrier) = self.barrier.as_ref().filter(|_| marker != "CTX-JOIN") {
            barrier.wait().await;
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
        if matches!(marker.as_str(), "CTX-JOIN" | "CTX-JUDGE") {
            return Ok(Visit {
                marker,
                prompt: spec.prompt.clone(),
                system_append: spec.system_append.clone(),
                tools: Vec::new(),
                item_ids: Vec::new(),
                addresses: Vec::new(),
                read: String::new(),
                foreign: None,
                extra_dirs: spec.extra_dirs.clone(),
                entered,
                left: Instant::now(),
            });
        }
        let socket = self.socket()?;
        let (greeting, listed) = call(&socket, "list_context", json!({})).await?;
        let listed = ok(listed)?;
        // 2026-09-08 (CT-06) — `call` oddaje JUŻ samą tablicę `greeting.tools`, więc stało tu
        // `greeting["tools"]`, czyli indeks po kluczu na tablicy: serde_json daje na to `Null`,
        // lista wychodziła pusta i asercja `tools.len() == 4` była NIESPEŁNIALNA niezależnie od
        // produktu. Most naprawdę wystawia cztery czytelniki (`bridge::context::context_tools`),
        // a `allowed` w `bridge::host::talk` przepuszczało wywołania, więc odczyty w tym samym
        // teście przechodziły — defekt siedział wyłącznie w fiksturze.
        let tools = greeting
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|tool| tool["name"].as_str().map(ToOwned::to_owned))
            .collect::<Vec<_>>();
        let items = listed["items"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("the bridge did not return context items"))?;
        let item_ids = items
            .iter()
            .filter_map(|item| item["id"].as_str().map(ToOwned::to_owned))
            .collect::<Vec<_>>();
        let addresses = items
            .iter()
            .filter_map(|item| item["address"].as_str().map(ToOwned::to_owned))
            .collect::<Vec<_>>();
        let first = item_ids
            .first()
            .ok_or_else(|| anyhow::anyhow!("the context allocation is empty"))?;
        let read = if marker == "CTX-FROZEN"
            && let Some((library, changed)) = &self.library_change
        {
            // 2026-09-08 — obie mutacje następują dopiero po wejściu do adaptera i po liście
            // z mostu; wcześniejszy zapis mierzyłby wyłącznie preflight, nie aktywny bieg.
            fs::write(changed, "the live library changed after Start")?;
            let (_, after_edit) = call(&socket, "read_context", json!({"id": first})).await?;
            let after_edit = ok(after_edit)?["text"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            fs::remove_dir_all(library)?;
            let (_, after_delete) = call(&socket, "read_context", json!({"id": first})).await?;
            let after_delete = ok(after_delete)?["text"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            format!("{after_edit}\n{after_delete}")
        } else {
            let (_, read) = call(&socket, "read_context", json!({"id": first})).await?;
            ok(read)?["text"].as_str().unwrap_or_default().to_owned()
        };
        let foreign = if let Some(id) = &self.foreign_id {
            Some(call(&socket, "read_context", json!({"id": id})).await?.1)
        } else {
            None
        };
        let (_, lead) = call(&socket, "list_workflows", json!({})).await?;
        anyhow::ensure!(
            matches!(lead, Answer::Refused(_)),
            "context granted a Lead action: {lead:?}"
        );
        Ok(Visit {
            marker,
            prompt: spec.prompt.clone(),
            system_append: spec.system_append.clone(),
            tools,
            item_ids,
            addresses,
            read,
            foreign,
            extra_dirs: spec.extra_dirs.clone(),
            entered,
            left: Instant::now(),
        })
    }
}

#[async_trait]
impl AgentDriver for Model {
    fn id(&self) -> &'static str {
        "claude"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("ct-06-controlled-adapter".to_owned()),
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

    fn prepare_protected_step(
        &self,
        _: &StepSettings,
    ) -> Option<anyhow::Result<PreparedProtectedStep>> {
        Some(Ok(PreparedProtectedStep {
            driver: Arc::new(self.clone()),
            writable_roots: Vec::new(),
            readable_roots: Vec::new(),
            readable_files: Vec::new(),
        }))
    }

    fn with_filesystem_fence(&self, _: &FilesystemFence) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.seen.started.fetch_add(1, Ordering::SeqCst);
        let entered = Instant::now();
        let answer = if spec.prompt.contains("CTX-JUDGE") {
            "## Answer\nThe work needs another try.\n\noutcome: fail\n".to_owned()
        } else {
            ANSWER.to_owned()
        };
        match self.inspect(&spec, entered).await {
            Ok(visit) => self.seen.visit(visit),
            Err(error) => self.seen.error(error.to_string()),
        }
        let session = SessionRef {
            vendor: "claude",
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
            answer,
        }))
    }
}

#[derive(Debug)]
struct Turn {
    session: SessionRef,
    events: mpsc::Sender<DecodedEvent>,
    answer: String,
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
            text: self.answer.clone(),
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

fn ok(answer: Answer) -> anyhow::Result<Value> {
    match answer {
        Answer::Ok(value) => Ok(value),
        other => anyhow::bail!("the context call was refused: {other:?}"),
    }
}

async fn call(socket: &Path, name: &str, input: Value) -> anyhow::Result<(Value, Answer)> {
    tokio::time::timeout(Duration::from_secs(3), async {
        let stream = UnixStream::connect(socket).await?;
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        let greeting: Greeting = serde_json::from_str(&line)?;
        writer
            .write_all(
                format!(
                    "{}\n",
                    serde_json::to_string(&Call {
                        id: json!(606),
                        call: name.to_owned(),
                        input,
                    })?
                )
                .as_bytes(),
            )
            .await?;
        writer.flush().await?;
        line.clear();
        reader.read_line(&mut line).await?;
        let reply: Reply = serde_json::from_str(&line)?;
        Ok::<_, anyhow::Error>((greeting.tools, reply.answer))
    })
    .await?
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn different_parallel_steps_receive_only_their_selected_material()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let alpha = bench.publish(
        "Alpha brief",
        "Keep the alpha checkout legible.",
        "alpha",
        "Alpha checkout",
        "ALPHA-REQUIREMENT must stay exact.",
        "ALPHA-SOURCE-CONTENT is private to the alpha step.",
    )?;
    let beta = bench.publish(
        "Beta brief",
        "Keep the beta receipt complete.",
        "beta",
        "Beta receipt",
        "BETA-REQUIREMENT must stay exact.",
        "BETA-SOURCE-CONTENT is private to the beta step.",
    )?;
    let workflow = bench.workflow(
        "parallel-context",
        &json!({
            "format": 2,
            "id": "ct-06-parallel",
            "name": "Parallel context",
            "steps": [
                step("alpha", "CTX-ALPHA implement alpha.", pin(&alpha)),
                step("beta", "CTX-BETA implement beta.", pin(&beta))
            ],
            "links": []
        }),
    )?;
    let seen = Arc::new(Seen::default());
    let report = run(
        &bench,
        workflow,
        drivers(
            Arc::clone(&seen),
            Some(Arc::new(Barrier::new(2))),
            None,
            None,
        ),
        2,
    )
    .await?;

    assert!(
        seen.errors().is_empty(),
        "the controlled adapters failed: {:?}",
        seen.errors()
    );
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; 2],
        "run={:?} started={} adapter={:?}",
        bench.run_errors(),
        seen.started.load(Ordering::SeqCst),
        seen.errors()
    );
    assert_eq!(seen.started.load(Ordering::SeqCst), 2);
    let visits = seen.visits();
    assert_eq!(
        visits.len(),
        2,
        "both physical steps must reach the real adapter"
    );
    let by_marker = visits
        .iter()
        .map(|visit| (visit.marker.as_str(), visit))
        .collect::<BTreeMap<_, _>>();
    let alpha_visit = by_marker.get("CTX-ALPHA").ok_or("alpha did not run")?;
    let beta_visit = by_marker.get("CTX-BETA").ok_or("beta did not run")?;
    for (own, other, visit) in [
        ("ALPHA", "BETA", *alpha_visit),
        ("BETA", "ALPHA", *beta_visit),
    ] {
        assert!(visit.prompt.contains("## Reference materials"));
        assert!(visit.prompt.contains(&format!("{own}-REQUIREMENT")));
        assert!(!visit.prompt.contains(&format!("{other}-REQUIREMENT")));
        assert!(visit.read.contains(&format!("{own}-")));
        assert!(!visit.read.contains(&format!("{other}-")));
        assert_eq!(
            visit.tools.len(),
            4,
            "context alone must grant only its four readers"
        );
        assert!(
            visit
                .system_append
                .as_deref()
                .is_none_or(|text| !text.contains("Reference materials"))
        );
    }
    assert!(
        alpha_visit.entered < beta_visit.left && beta_visit.entered < alpha_visit.left,
        "the two context readers did not overlap in time: {visits:?}"
    );
    let folder = report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the run folder has no portable name")?;
    let past = read_run_inner(bench.project.path(), folder)?;
    let delivery = past
        .steps
        .iter()
        .map(|step| (&step.id, &step.reference_materials))
        .collect::<Vec<_>>();
    assert!(
        past.steps.iter().all(|step| {
            step.reference_materials
                .as_ref()
                .is_some_and(|lines| lines.iter().any(|line| line.contains("was opened")))
        }),
        "the visible delivery history did not report both reads: {delivery:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_step_with_only_context_reads_its_own_set_through_the_bridge()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let material = bench.publish(
        "Context-only brief",
        "Start the bridge without services or messages.",
        "only",
        "Only this step",
        "ONLY-REQUIREMENT reaches the final prompt.",
        "ONLY-SOURCE-CONTENT comes back through the real bridge.",
    )?;
    let workflow = bench.workflow(
        "context-only",
        &json!({
            "format": 2,
            "id": "ct-06-context-only",
            "name": "Context only",
            "steps": [step("only", "CTX-ALPHA use only this material.", pin(&material))],
            "links": []
        }),
    )?;
    let report = run(
        &bench,
        workflow,
        controlled_bridge_claude(
            bench.home.path(),
            &format!("{}--source--{}", material.set_id, material.source_id),
        )?,
        1,
    )
    .await?;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded],
        "{:?}",
        bench.run_errors()
    );
    let envelope: Value = serde_json::from_str(&fs::read_to_string(
        bench.home.path().join("bridge.stdin.log"),
    )?)?;
    let final_prompt = envelope
        .pointer("/message/content/0/text")
        .and_then(Value::as_str)
        .ok_or("the real Claude adapter did not send the final prompt through stdin")?;
    assert!(final_prompt.contains("ONLY-REQUIREMENT"));
    let replies = fs::read_to_string(bench.home.path().join("bridge.replies.jsonl"))?
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<Result<Vec<_>, _>>()?;
    let tools = replies
        .iter()
        .find(|reply| reply["id"] == 2)
        .and_then(|reply| reply.pointer("/result/tools"))
        .and_then(Value::as_array)
        .ok_or("the production MCP bridge did not return its tool list")?;
    assert_eq!(
        tools
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect::<Vec<_>>(),
        vec![
            "list_context",
            "search_context",
            "read_context",
            "view_context_image"
        ]
    );
    let answer = |id| {
        replies
            .iter()
            .find(|reply| reply["id"] == id)
            .and_then(|reply| reply.pointer("/result/content/0/text"))
            .and_then(Value::as_str)
    };
    let listed = answer(3).ok_or("the controlled process did not receive the context list")?;
    let read = answer(4).ok_or("the controlled process did not receive the source bytes")?;
    let refused = replies
        .iter()
        .find(|reply| reply["id"] == 5)
        .ok_or("the controlled process did not try the Lead-only tool")?;
    assert!(listed.contains(&material.source_id));
    assert!(read.contains("ONLY-SOURCE-CONTENT"));
    assert_eq!(refused.pointer("/result/isError"), Some(&json!(true)));
    assert!(
        refused
            .pointer("/result/content/0/text")
            .and_then(Value::as_str)
            .is_some_and(|text| text.contains("not available"))
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn copies_keep_the_selection_and_fan_in_keeps_the_response_contract()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let material = bench.publish(
        "Copy brief",
        "Give every physical copy the same brief.",
        "copies",
        "Copies",
        "COPY-REQUIREMENT remains exact for every try.",
        "COPY-SOURCE-CONTENT reaches each physical copy.",
    )?;
    let mut copy_step = step(
        "work",
        "CTX-COPY number {{copy}} of {{copies}}.",
        pin(&material),
    );
    copy_step["copies"] = json!(2);
    let joined = step(
        "join",
        "CTX-JOIN combine both results.",
        json!({"schema":1,"inheritWorkflow":false,"exclude":[],"sets":[]}),
    );
    let workflow = bench.workflow(
        "copies-context",
        &json!({
            "format": 2,
            "id": "ct-06-copies",
            "name": "Copies retain context",
            "steps": [copy_step, joined],
            "links": [{"from":"work","to":"join"}]
        }),
    )?;
    let seen = Arc::new(Seen::default());
    let report = run(
        &bench,
        workflow,
        drivers(
            Arc::clone(&seen),
            Some(Arc::new(Barrier::new(2))),
            None,
            None,
        ),
        2,
    )
    .await?;
    assert!(
        seen.errors().is_empty(),
        "the controlled adapters failed: {:?}",
        seen.errors()
    );
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; 3],
        "run={:?} started={} adapter={:?}",
        bench.run_errors(),
        seen.started.load(Ordering::SeqCst),
        seen.errors()
    );
    let visits = seen.visits();
    let copy_visits = visits
        .iter()
        .filter(|visit| visit.marker == "CTX-COPY")
        .collect::<Vec<_>>();
    assert_eq!(copy_visits.len(), 2);
    assert!(
        copy_visits
            .iter()
            .all(|visit| visit.prompt.contains("COPY-REQUIREMENT"))
    );
    assert_ne!(
        copy_visits[0].addresses, copy_visits[1].addresses,
        "each physical copy needs its own read address and delivery account"
    );
    let join = visits
        .iter()
        .find(|visit| visit.marker == "CTX-JOIN")
        .ok_or("fan-in did not run")?;
    // 2026-09-08 (CT-06) — stało tu zdanie "what the step before this one left", którego
    // produkt NIGDY nie emituje: to zlepek nagłówka indeksu ("Steps before this one left what
    // they found in these files:") i znacznika przy pozycji ("(what the step before left)").
    // Asercja była więc niespełnialna niezależnie od fan-inu, a fan-in działa — prompt wymienia
    // oba przekazania. Kotwiczę tak, jak robi to wyrocznia tego zachowania
    // (`handoff_index_for_fan_in.rs`): na WSKAŹNIKU `handoffs/`, nie na prozie, bo proza żyje
    // w jednym miejscu i wolno jej się zmienić, a wskaźnik jest kontraktem dla obu rodziców.
    assert_eq!(
        join.prompt.matches("handoffs/").count(),
        2,
        "the merging step has to be told about BOTH parents; it was told:\n{}",
        join.prompt
    );
    assert_eq!(
        join.prompt.matches("(what the step before left)").count(),
        2,
        "each listed handoff keeps its own marker"
    );
    assert!(
        join.prompt
            .contains("Your last message is what this step passes on.")
    );
    let folder = report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the run folder has no portable name")?;
    let past = read_run_inner(bench.project.path(), folder)?;
    let delivery = past
        .steps
        .iter()
        .map(|step| (&step.id, &step.reference_materials))
        .collect::<Vec<_>>();
    let accounts = past
        .steps
        .iter()
        .filter(|step| step.tile == "work")
        .filter(|step| {
            step.reference_materials
                .as_ref()
                .is_some_and(|lines| lines.iter().any(|line| line.contains("was opened")))
        })
        .count();
    assert_eq!(
        accounts, 2,
        "the copies did not keep separate delivery accounts: {delivery:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_retry_round_keeps_the_tile_selection_but_gets_its_own_read_address()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let material = bench.publish(
        "Retry brief",
        "Keep the same brief through every try.",
        "retry",
        "Retry",
        "RETRY-REQUIREMENT remains exact in every try.",
        "RETRY-SOURCE-CONTENT reaches both physical rounds.",
    )?;
    let workflow = bench.workflow(
        "retry-context",
        &json!({
            "format": 2,
            "id": "ct-06-retry",
            "name": "Retry retains context",
            "steps": [
                step("work", "CTX-RETRY do this try.", pin(&material)),
                step("judge", "CTX-JUDGE decide this try.", json!({"schema":1,"inheritWorkflow":false,"exclude":[],"sets":[]}))
            ],
            "links": [
                {"from":"work","to":"judge"},
                {"from":"judge","to":"work","max_turns":2}
            ]
        }),
    )?;
    let seen = Arc::new(Seen::default());
    let _report = run(
        &bench,
        workflow,
        drivers(Arc::clone(&seen), None, None, None),
        1,
    )
    .await?;

    assert!(
        seen.errors().is_empty(),
        "the controlled adapters failed: {:?}",
        seen.errors()
    );
    let visits = seen.visits();
    let tries = visits
        .iter()
        .filter(|visit| visit.marker == "CTX-RETRY")
        .collect::<Vec<_>>();
    assert_eq!(
        tries.len(),
        2,
        "the retry round did not reach a second physical agent"
    );
    assert!(
        tries
            .iter()
            .all(|visit| visit.prompt.contains("RETRY-REQUIREMENT"))
    );
    assert_ne!(
        tries[0].addresses, tries[1].addresses,
        "retry rounds shared one reader identity and delivery account"
    );
    Ok(())
}
