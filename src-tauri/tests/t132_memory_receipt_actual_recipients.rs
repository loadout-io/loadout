//! T-132 AC-1: receipt rozroznia uchwyt procesu od samej proby `AgentDriver::start`.
//!
//! Spec uruchamia prawdziwy workflow i czyta `run.json` jako `serde_json::Value`. Fake zapisuje
//! unikalny `node_key` z `RunSpec.cwd`, po czym test wiaze go z fizycznym UUID w `run.json`.
//! Dla trzeciej kopii fake zwraca kontrolowany `Err` bez uchwytu. Czwarty krok jest pominiety
//! przez graf, wiec jego UUID nie moze pojawic sie ani w ledgerze, ani w przyszlym receipt.

#![allow(clippy::too_many_lines)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::memory::project_notes_root;
use loadout_lib::commands::run::run_workflow_with_reflection;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, StepSettings, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::{EvidenceStreams, EvidenceTarget};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::Vendor;
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

const PATIENCE: Duration = Duration::from_secs(30);
const AGENT_NAME: &str = "Receipt agent";
const OPAQUE_ORIGIN: &str = "019b0131-aaaa-7bbb-8ccc-0123456789ab";

const IMPORTED_RULE: &str = "T132 imported global memory";
const AGENT_RULE: &str = "T132 only this repeated agent knows this";
const PROJECT_RULE: &str = "T132 this project remembers the proposing run";
const SHORT_RULE: &str = "T132 short memory still fits after the long one";

const AGENT: &str = r#"---
schema: 1
id: 019b0132-0000-7000-8000-000000000132
name: Receipt agent
summary: Proves the physical recipient boundary
color: moss
runsWith: claude-code
model: sonnet
thinking: balanced
fileAccess: look-only
giveUpAfterMinutes: 20
writeResultsTo: ""
tools: everything
skills: []
connections: []
---
Follow the step instructions.
"#;

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_t132_actual_memory_recipients",
  "name": "Actual memory recipients",
  "steps": [
    {
      "kind": "agent",
      "id": "delivered",
      "name": "Repeated worker",
      "agent": "019b0132-0000-7000-8000-000000000132",
      "overrides": {},
      "copies": 3,
      "instructions": "t132 delivered: copy {{copy}} of {{copies}}.",
      "folder": { "use": "fresh-copy" },
      "whenItFails": "stop",
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "skipped",
      "name": "Skipped worker",
      "agent": "019b0132-0000-7000-8000-000000000132",
      "overrides": {},
      "instructions": "t132 skipped: this driver must never start.",
      "folder": { "use": "project" },
      "at": { "x": 0, "y": 200 }
    }
  ],
  "links": [{ "from": "delivered", "to": "skipped" }]
}"#;

const REFLECTION_ANSWER: &str = "rule: T132 kept reflection\n\
because: this candidate proves a successful automatic write\n\n\
rule: T132 discarded again\n\
because: this exact candidate has a tombstone\n\n\
rule: T132 io failed reflection\n\
because: this candidate meets an unrelated IO refusal\n";

fn long_rule() -> String {
    "L".repeat(4_004)
}

fn seed_note(
    root: &Path,
    id: &str,
    scope: &str,
    rule: &str,
    extra: &[(&str, &str)],
) -> Result<PathBuf, Box<dyn Error>> {
    let notes = root.join("notes");
    fs::create_dir_all(&notes)?;
    let mut front = format!(
        "---\nscope: {scope}\nkind: rule\ntitle: {id}\nrule: {rule}\nbecause: T132 fixture reason\nstatus: in-use\noccurrences: 1\nmodified: 2026-08-26T10:00:00Z\nlast_used_at: null\n",
    );
    for (key, value) in extra {
        front.push_str(key);
        front.push_str(": ");
        front.push_str(value);
        front.push('\n');
    }
    front.push_str("---\n");
    let path = notes.join(format!("{id}.md"));
    fs::write(&path, front)?;
    Ok(path)
}

struct Bench {
    home: TempDir,
    project: TempDir,
    starts: Arc<Mutex<Vec<StartAttempt>>>,
    source_paths: Vec<PathBuf>,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = tempfile::tempdir()?;
        let project = tempfile::tempdir()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(home.path().join("memory/notes"))?;
        fs::create_dir_all(project_notes_root(project.path()).join("notes"))?;
        fs::write(home.path().join("agents/receipt-agent.md"), AGENT)?;
        fs::write(home.path().join("workflows/t132.json"), WORKFLOW)?;

        let library = home.path().join("memory");
        let project_memory = project_notes_root(project.path());
        let long = long_rule();
        let source_paths = vec![
            seed_note(&library, "a-too-long", "everywhere", &long, &[])?,
            seed_note(
                &library,
                "agent-only",
                "this-agent",
                AGENT_RULE,
                &[("agent", AGENT_NAME)],
            )?,
            seed_note(
                &library,
                "same",
                "everywhere",
                IMPORTED_RULE,
                &[("project", OPAQUE_ORIGIN)],
            )?,
            seed_note(&library, "z-short-after", "everywhere", SHORT_RULE, &[])?,
            seed_note(
                &project_memory,
                "same",
                "this-project",
                PROJECT_RULE,
                &[("from", OPAQUE_ORIGIN)],
            )?,
        ];

        let discarded = project_memory.join("discarded");
        fs::create_dir_all(&discarded)?;
        fs::write(
            discarded.join("t132-discarded-again__20260826T100000Z.md"),
            "tombstone",
        )?;

        Ok(Self {
            home,
            project,
            starts: Arc::new(Mutex::new(Vec::new())),
            source_paths,
        })
    }

    fn workflow(&self) -> PathBuf {
        self.home.path().join("workflows/t132.json")
    }

    fn database(&self) -> PathBuf {
        self.project.path().join(".loadout/loadout.db")
    }

    fn starts(&self) -> Result<Vec<StartAttempt>, Box<dyn Error>> {
        self.starts
            .lock()
            .map(|starts| starts.clone())
            .map_err(|_| "the start ledger was poisoned".into())
    }

    async fn run(&self) -> Result<RunReport, Box<dyn Error>> {
        let store = Store::open(&self.database())?;
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store: &store,
            drivers: fake_drivers(
                Arc::clone(&self.starts),
                project_notes_root(self.project.path()),
            ),
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow: self.workflow(),
            how_many_at_once: 4,
            task: None,
            part: None,
            handoffs_from: None,
        };
        let (sink, source) = line_channel(QUEUE_CAP);
        let pump = spawn_pump(source, Channel::new(|_| Ok(())));
        let report = tokio::time::timeout(
            PATIENCE,
            run_workflow_with_reflection(&deps, &request, sink, None, true),
        )
        .await??;
        tokio::time::timeout(PATIENCE, pump).await??;
        Ok(report)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartResult {
    Handle,
    Refused,
}

#[derive(Debug, Clone)]
struct StartAttempt {
    session: String,
    node_key: String,
    prompt: String,
    result: StartResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Step,
    Reflection,
}

#[derive(Clone)]
struct Fake {
    mode: Mode,
    starts: Arc<Mutex<Vec<StartAttempt>>>,
    project_memory: PathBuf,
    settings: Option<StepSettings>,
    evidence: Option<EvidenceTarget>,
    budget: Option<f64>,
}

impl Fake {
    fn step(starts: Arc<Mutex<Vec<StartAttempt>>>, project_memory: PathBuf) -> Self {
        Self {
            mode: Mode::Step,
            starts,
            project_memory,
            settings: None,
            evidence: None,
            budget: None,
        }
    }

    fn cloned(&self) -> Self {
        Self {
            mode: self.mode,
            starts: Arc::clone(&self.starts),
            project_memory: self.project_memory.clone(),
            settings: self.settings.clone(),
            evidence: self.evidence.clone(),
            budget: self.budget,
        }
    }

    async fn write_evidence(&self) -> anyhow::Result<()> {
        let target = self
            .evidence
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("the fake was started without its evidence target"))?;
        let EvidenceStreams {
            mut stdout,
            mut stderr,
        } = target.open().await?;
        stdout.write(b"{\"type\":\"t132-fixture\"}\n").await?;
        stderr.write(b"t132 fixture stderr\n").await?;
        stdout.close().await?;
        stderr.close().await?;
        Ok(())
    }
}

fn fake_drivers(starts: Arc<Mutex<Vec<StartAttempt>>>, project_memory: PathBuf) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake::step(starts, project_memory));
    Arc::new(move |_vendor: Vendor| Arc::clone(&driver))
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        "t132-fake"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("t132".to_owned()),
        })
    }

    fn reflecting(&self) -> Option<Arc<dyn AgentDriver>> {
        let mut reflection = self.cloned();
        reflection.mode = Mode::Reflection;
        reflection.settings = None;
        reflection.evidence = None;
        reflection.budget = None;
        Some(Arc::new(reflection))
    }

    fn with_settings(
        &self,
        settings: &StepSettings,
    ) -> Option<anyhow::Result<Arc<dyn AgentDriver>>> {
        let mut clone = self.cloned();
        clone.settings = Some(settings.clone());
        Some(Ok(Arc::new(clone)))
    }

    fn with_evidence(&self, target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        let mut clone = self.cloned();
        clone.evidence = Some(target);
        Some(Arc::new(clone))
    }

    fn with_budget(&self, dollars: f64) -> Option<Arc<dyn AgentDriver>> {
        let mut clone = self.cloned();
        clone.budget = Some(dollars);
        Some(Arc::new(clone))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.write_evidence().await?;

        if self.mode == Mode::Reflection {
            // Osobny blad IO powstaje dopiero po zamrozeniu katalogu dla krokow.
            fs::create_dir_all(
                self.project_memory
                    .join("notes")
                    .join("t132-io-failed-reflection.md"),
            )?;
        }

        let refused = self.mode == Mode::Step && spec.prompt.contains("copy 3 of 3");
        if self.mode == Mode::Step {
            self.starts
                .lock()
                .map_err(|_| anyhow::anyhow!("the start ledger was poisoned"))?
                .push(StartAttempt {
                    session: spec.run_id.to_string(),
                    node_key: spec
                        .cwd
                        .file_name()
                        .and_then(|name| name.to_str())
                        .ok_or_else(|| {
                            anyhow::anyhow!("the RunSpec cwd has no UTF-8 physical node key")
                        })?
                        .to_owned(),
                    prompt: spec.prompt.clone(),
                    result: if refused {
                        StartResult::Refused
                    } else {
                        StartResult::Handle
                    },
                });
        }
        if refused {
            return Err(anyhow::anyhow!(
                "T132 controlled refusal before returning an agent handle"
            ));
        }

        let session = SessionRef {
            vendor: "t132-fake",
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

        let (ok, text) = if self.mode == Mode::Reflection {
            (true, REFLECTION_ANSWER.to_owned())
        } else if spec.prompt.contains("copy 2 of 3") {
            (
                false,
                "## Answer\nThis copy started and then failed.\n\n## Evidence\nfixture\n\n## Open\nThe failure is intentional.\n"
                    .to_owned(),
            )
        } else {
            (
                true,
                "## Answer\nThis copy completed.\n\n## Evidence\nfixture\n\n## Open\nNothing.\n"
                    .to_owned(),
            )
        };

        Ok(Box::new(Turn {
            events,
            session,
            ok,
            text,
        }))
    }
}

struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    ok: bool,
    text: String,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        let outcome = TurnOutcome {
            ok: self.ok,
            reason: if self.ok {
                FinishReason::Completed
            } else {
                FinishReason::Failed("the fixture fails after start".to_owned())
            },
            text: self.text.clone(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(2),
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
        Ok(Some(i32::from(!self.ok)))
    }
}

fn read_json(path: &Path) -> Result<Value, Box<dyn Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn fingerprint(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[derive(Clone, Copy)]
struct ExpectedRecord<'a> {
    place: &'a str,
    id: &'a str,
    reference: &'a str,
    rule: &'a str,
    project: Option<&'a str>,
    from: Option<&'a str>,
    recipients: &'a [String],
    left_out_for: &'a [String],
}

fn expected_record(record: ExpectedRecord<'_>) -> Value {
    json!({
        "reference": record.reference,
        "hash": fingerprint(record.rule.as_bytes()),
        "bytes": record.rule.len(),
        "address": { "place": record.place, "id": record.id },
        "project": record.project,
        "from": record.from,
        "recipients": record.recipients,
        "leftOutFor": record.left_out_for
    })
}

fn step_rows(run: &Value) -> Result<&[Value], Box<dyn Error>> {
    run.get("steps")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| "run.json has no physical step rows".into())
}

fn attempt<'a>(
    starts: &'a [StartAttempt],
    marker: &str,
) -> Result<&'a StartAttempt, Box<dyn Error>> {
    starts
        .iter()
        .find(|start| start.prompt.contains(marker))
        .ok_or_else(|| format!("no AgentDriver::start attempt carried {marker:?}").into())
}

fn row_for_attempt<'a>(
    rows: &'a [Value],
    attempted: &StartAttempt,
) -> Result<&'a Value, Box<dyn Error>> {
    rows.iter()
        .find(|row| row.get("node_key").and_then(Value::as_str) == Some(&attempted.node_key))
        .ok_or_else(|| {
            format!(
                "AgentDriver::start received cwd node {:?}, but run.json has no matching physical row",
                attempted.node_key
            )
            .into()
        })
}

fn physical_id<'a>(row: &'a Value, path: &str) -> Result<&'a str, Box<dyn Error>> {
    row.get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("the {path} row has no physical UUID").into())
}

#[test]
fn an_old_flattened_memory_record_remains_readable() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let folder = "20260826-132000__019b0132-0000-7000-8000-000000000131";
    let dir = root.path().join(".loadout/runs").join(folder);
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join("run.json"),
        serde_json::to_vec_pretty(&json!({
            "id": "019b0132-0000-7000-8000-000000000131",
            "workflow_id": "legacy.json",
            "title": "Legacy receipt",
            "status": "succeeded",
            "steps": [{
                "id": "019b0132-0000-7000-8000-00000000013a",
                "node_key": "legacy",
                "name": "Legacy",
                "agent": "Receipt agent",
                "status": "succeeded"
            }],
            "memory": [{
                "reference": "memory/notes/legacy.md",
                "hash": "0123456789abcdef",
                "bytes": 12
            }]
        }))?,
    )?;

    let opened = read_run_inner(root.path(), folder)?;
    assert_eq!(opened.steps.len(), 1);
    assert_eq!(opened.steps[0].id, "019b0132-0000-7000-8000-00000000013a");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn run_json_names_only_steps_that_received_a_handle() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let report = bench.run().await?;
    let path = report.dir.join("run.json");
    let run = read_json(&path)?;
    let rows = step_rows(&run)?;
    assert_eq!(
        rows.len(),
        4,
        "three copies and their graph-skipped descendant must be recorded"
    );

    let starts = bench.starts()?;
    assert_eq!(
        starts.len(),
        3,
        "success, post-handle failure and controlled start refusal must all call start; the \
         graph-skipped descendant must not. Attempts were: {starts:?}"
    );
    let succeeded = attempt(&starts, "copy 1 of 3")?;
    let failed_after_handle = attempt(&starts, "copy 2 of 3")?;
    let refused_before_handle = attempt(&starts, "copy 3 of 3")?;
    assert_eq!(succeeded.result, StartResult::Handle);
    assert_eq!(failed_after_handle.result, StartResult::Handle);
    assert_eq!(refused_before_handle.result, StartResult::Refused);

    for started in [succeeded, failed_after_handle] {
        let row = row_for_attempt(rows, started)?;
        assert_eq!(
            row.get("agent_session_id").and_then(Value::as_str),
            Some(started.session.as_str()),
            "the physical row that received a handle is not tied to that handle's session"
        );
    }
    let failed_row = row_for_attempt(rows, failed_after_handle)?;
    assert_eq!(
        failed_row.get("status").and_then(Value::as_str),
        Some("failed"),
        "the fixture must really fail after the handle exists"
    );

    let refused_row = row_for_attempt(rows, refused_before_handle)?;
    // `agent_session_id` is allocated while planning, before `start`, so it is deliberately not
    // used as an oracle. The fake's typed `StartResult::Refused` above is the direct boundary
    // evidence; this row lookup binds that real call to the physical UUID written by the run.

    let skipped_row = rows
        .iter()
        .find(|row| {
            row.get("node_key")
                .and_then(Value::as_str)
                .is_some_and(|key| key.starts_with("skipped"))
        })
        .ok_or("run.json has no physical UUID for the graph-skipped descendant")?;
    let skipped = skipped_row
        .get("id")
        .and_then(Value::as_str)
        .ok_or("the graph-skipped descendant has no physical UUID")?;
    let skipped_node = skipped_row
        .get("node_key")
        .and_then(Value::as_str)
        .ok_or("the graph-skipped descendant has no node key")?;
    assert!(
        starts.iter().all(|start| start.node_key != skipped_node)
            && starts
                .iter()
                .all(|start| !start.prompt.contains("t132 skipped:")),
        "the graph-skipped descendant reached AgentDriver::start"
    );

    let succeeded_id = physical_id(row_for_attempt(rows, succeeded)?, "successful start")?;
    let failed_after_handle_id = physical_id(failed_row, "post-handle failure")?;
    let refused_before_handle_id = physical_id(refused_row, "start refusal")?;
    let mut recipients = vec![succeeded_id.to_owned(), failed_after_handle_id.to_owned()];
    recipients.sort();
    recipients.dedup();
    assert_eq!(
        recipients.len(),
        2,
        "the two returned handles must map to two distinct physical UUIDs"
    );

    let long = long_rule();
    let expected = json!([
        expected_record(ExpectedRecord {
            place: "library",
            id: "a-too-long",
            reference: "memory/notes/a-too-long.md",
            rule: &long,
            project: None,
            from: None,
            recipients: &[],
            left_out_for: &recipients,
        }),
        expected_record(ExpectedRecord {
            place: "library",
            id: "agent-only",
            reference: "memory/notes/agent-only.md",
            rule: AGENT_RULE,
            project: None,
            from: None,
            recipients: &recipients,
            left_out_for: &[],
        }),
        expected_record(ExpectedRecord {
            place: "library",
            id: "same",
            reference: "memory/notes/same.md",
            rule: IMPORTED_RULE,
            project: Some(OPAQUE_ORIGIN),
            from: None,
            recipients: &recipients,
            left_out_for: &[],
        }),
        expected_record(ExpectedRecord {
            place: "library",
            id: "z-short-after",
            reference: "memory/notes/z-short-after.md",
            rule: SHORT_RULE,
            project: None,
            from: None,
            recipients: &recipients,
            left_out_for: &[],
        }),
        expected_record(ExpectedRecord {
            place: "project",
            id: "same",
            reference: ".loadout/memory/notes/same.md",
            rule: PROJECT_RULE,
            project: None,
            from: Some(OPAQUE_ORIGIN),
            recipients: &recipients,
            left_out_for: &[],
        })
    ]);

    assert_eq!(
        run.get("memory"),
        Some(&expected),
        "the receipt must be address-sorted and literal: only UUIDs whose start returned a \
         handle appear exactly once, and the long note records those UUIDs only as deferrals"
    );
    let receipt_text = serde_json::to_string(&expected)?;
    assert!(
        !receipt_text.contains(refused_before_handle_id),
        "the UUID from a real AgentDriver::start call that returned Err leaked into the receipt"
    );
    assert!(
        !receipt_text.contains(skipped),
        "the graph-skipped UUID leaked into the receipt"
    );

    let receipt_bytes = fs::read(&path)?;
    fs::write(&bench.source_paths[2], "a later edit")?;
    fs::remove_file(&bench.source_paths[4])?;
    assert_eq!(
        fs::read(&path)?,
        receipt_bytes,
        "editing and deleting source notes after the run changed the bytes of its receipt"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reflection_counts_only_the_typed_previously_discarded_refusal()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let report = bench.run().await?;
    let run = read_json(&report.dir.join("run.json"))?;
    let reflection = run
        .get("reflection")
        .ok_or("run.json has no reflection receipt")?;

    assert_eq!(reflection.get("ran"), Some(&Value::Bool(true)));
    assert_eq!(reflection.get("kept").and_then(Value::as_u64), Some(1));
    assert!(
        project_notes_root(bench.project.path())
            .join("notes/t132-kept-reflection.md")
            .is_file(),
        "the control candidate was not kept successfully"
    );
    assert!(
        !project_notes_root(bench.project.path())
            .join("notes/t132-discarded-again.md")
            .exists(),
        "the exact tombstone was ignored and the discarded candidate came back"
    );
    assert!(
        project_notes_root(bench.project.path())
            .join("notes/t132-io-failed-reflection.md")
            .is_dir(),
        "the unrelated IO refusal was not planted, so the counter could pass without distinguishing errors"
    );
    assert_eq!(
        reflection.get("discardedAgain").and_then(Value::as_u64),
        Some(1),
        "PreviouslyDiscarded must increment discardedAgain exactly once; the unrelated IO \
         refusal stays a logged error and must not inflate that counter"
    );
    Ok(())
}
