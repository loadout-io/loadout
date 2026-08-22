//! T-34 AC-1: every product door owns the same durable, rebuildable private evidence.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::diagnostics::support_report;
use loadout_lib::commands::run::{
    AskRequest, TriggerRunReport, run_agent_inner, run_triggered_workflow_inner, run_workflow_inner,
};
use loadout_lib::commands::triggers::{self, TriggerPoll};
use loadout_lib::commands::workspaces;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::claude::{ClaudeDriver, Transcript};
use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Policy, Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::{ContextKind, ContextSource, EvidenceTarget, SafeInputManifest};
use loadout_lib::inherit::wire::{self, Chosen, InheritedSourceKind};
use loadout_lib::ipc::{QUEUE_CAP, line_channel};
use loadout_lib::library::agents::Vendor;
use loadout_lib::store::Store;
use rusqlite::Connection;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use uuid::Uuid;

const PATIENCE: Duration = Duration::from_secs(20);
const CLAUDE_ID: &str = "01990000-0000-7000-8000-000000000034";
const CODEX_ID: &str = "01990000-0000-7000-8000-000000000035";
const TRIGGER_KEY: &str = "lin_api_1234567890123456789012345678901234567890";
const PRIVATE_TASK: &str = "PRIVATE_TASK_T34 must never be persisted verbatim";
const MEMORY_EVERYWHERE: &str = "MEMORY_EVERYWHERE_T34 is an approved global fact.";
const MEMORY_PROJECT: &str = "MEMORY_PROJECT_T34 is a longer approved project fact.";
const INHERITED_SKILL_ALPHA: &str = "# Alpha\n\nINHERITED_SKILL_ALPHA_T34\n";
const INHERITED_SKILL_BETA: &str = "# Beta\n\nINHERITED_SKILL_BETA_T34 is longer\n";
const INHERITED_LEARNING: &str = "INHERITED_LEARNING_T34 must be measured, never copied.";
const INHERITED_SUBAGENT: &str = "INHERITED_SUBAGENT_T34 is the selected role body.";
const UNKNOWN: &str = r#"{"type":"a_future_event","payload":{"kept":true}}"#;

const CLAUDE_STDOUT: &str = concat!(
    r#"{"type":"system","subtype":"init","session_id":"01990000-0000-7000-8000-000000000034","model":"sonnet","tools":[]}"#,
    "\n",
    r#"{"type":"a_future_event","payload":{"kept":true}}"#,
    "\n",
    r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"claude evidence"}]}}"#,
    "\n",
    r#"{"type":"result","subtype":"success","is_error":false,"num_turns":1,"duration_ms":7,"total_cost_usd":0.001,"result":"claude evidence"}"#,
    "\n",
);
const CODEX_STDOUT: &str = concat!(
    r#"{"type":"thread.started","thread_id":"thread-evidence"}"#,
    "\n",
    r#"{"type":"a_future_event","payload":{"kept":true}}"#,
    "\n",
    r#"{"type":"item.completed","item":{"type":"agent_message","text":"codex evidence"}}"#,
    "\n",
    r#"{"type":"turn.completed","usage":{"input_tokens":3,"cached_input_tokens":1,"output_tokens":2}}"#,
    "\n",
);
const CLAUDE_STDERR: &[u8] = b"claude delayed diagnostic\n\xffclaude non-utf8 tail\n";
const CODEX_STDERR: &[u8] = b"codex delayed diagnostic\n\xfecodex non-utf8 tail\n";

const CLAUDE_AGENT: &str = r"---
schema: 1
id: 01990000-0000-7000-8000-000000000034
name: Claude evidence witness
summary: Leaves a product-path receipt
color: clay
runsWith: claude-code
model: sonnet
thinking: balanced
fileAccess: look-only
giveUpAfterMinutes: 20
writeResultsTo: handoffs/claude.md
tools: everything
skills: []
connections: []
---
Return one short answer.
";

const CODEX_AGENT: &str = r"---
schema: 1
id: 01990000-0000-7000-8000-000000000035
name: Codex evidence witness
summary: Leaves a product-path receipt
color: moss
runsWith: codex
model: gpt-5-codex
thinking: balanced
fileAccess: look-only
giveUpAfterMinutes: 20
writeResultsTo: handoffs/codex.md
tools: everything
skills: []
connections: []
---
Return one short answer.
";

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_evidence",
  "name": "Evidence",
  "steps": [
    {
      "kind": "agent", "id": "s_claude", "name": "Claude evidence",
      "agent": "01990000-0000-7000-8000-000000000034", "overrides": {},
      "instructions": "leave the Claude evidence receipt", "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent", "id": "s_claude_two", "name": "Claude evidence two",
      "agent": "01990000-0000-7000-8000-000000000034", "overrides": {},
      "instructions": "leave the second Claude evidence receipt", "at": { "x": 0, "y": 200 }
    },
    {
      "kind": "agent", "id": "s_codex", "name": "Codex evidence",
      "agent": "01990000-0000-7000-8000-000000000035", "overrides": {},
      "instructions": "leave the Codex evidence receipt", "at": { "x": 320, "y": 0 }
    }
  ],
  "links": [
    { "from": "s_claude", "to": "s_claude_two" },
    { "from": "s_claude", "to": "s_codex" },
    { "from": "s_claude_two", "to": "s_codex" }
  ]
}"#;

const CLAUDE_FAKE: &str = r#"#!/bin/sh
if [ "${1-}" = "--version" ]; then
  printf '%s\n' '2.1.238 (Claude Code)'
  exit 0
fi
here="$(dirname "$0")"
IFS= read -r first_turn
printf '%s\n' "$first_turn" >> "$here/claude.stdin.log"
cat "$here/claude.stdout.jsonl"
sleep 0.05
cat "$here/claude.stderr.log" >&2
exit 0
"#;

const CODEX_FAKE: &str = r#"#!/bin/sh
if [ "${1-}" = "--version" ]; then
  printf '%s\n' 'codex-cli 0.148.0'
  exit 0
fi
here="$(dirname "$0")"
printf '%s\n' '--- process ---' >> "$here/codex.argv.log"
printf '%s\n' "$@" >> "$here/codex.argv.log"
cat >> "$here/codex.stdin.log"
cat "$here/codex.stdout.jsonl"
sleep 0.05
cat "$here/codex.stderr.log" >&2
exit 0
"#;

fn executable(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn driver_factory(claude: PathBuf, codex: PathBuf) -> Drivers {
    let claude: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(claude));
    let codex: Arc<dyn AgentDriver> = Arc::new(CodexDriver::with_binary(codex));
    Arc::new(move |vendor| match vendor {
        Vendor::ClaudeCode => Arc::clone(&claude),
        Vendor::Codex => Arc::clone(&codex),
    })
}

#[derive(Clone, Copy, Debug)]
enum EvidenceFakeMode {
    PoisonThenFinish,
    WaitForStop,
    AliveThenDead,
}

#[derive(Clone, Default)]
struct FakeProofs {
    cancel_calls: Arc<AtomicUsize>,
    dropped: Arc<AtomicBool>,
}

#[derive(Clone)]
struct EvidenceFake {
    mode: EvidenceFakeMode,
    evidence: Option<EvidenceTarget>,
    started: Arc<AtomicBool>,
    proofs: FakeProofs,
}

#[async_trait]
impl AgentDriver for EvidenceFake {
    fn id(&self) -> &'static str {
        "claude"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("evidence-fake".to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let evidence = self
            .evidence
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("the product omitted its evidence target"))?;
        if matches!(self.mode, EvidenceFakeMode::PoisonThenFinish) {
            evidence.mark_incomplete();
        }
        self.started.store(true, Ordering::Release);
        let session = SessionRef {
            vendor: "claude",
            id: spec.run_id.to_string(),
        };
        let _ = events
            .send(
                (AgentEvent::Started {
                    session: session.clone(),
                    model: String::new(),
                    tools: Vec::new(),
                    capabilities: Vec::new(),
                })
                .into(),
            )
            .await;
        Ok(Box::new(EvidenceFakeHandle {
            mode: self.mode,
            events,
            session,
            proofs: self.proofs.clone(),
        }))
    }

    fn with_evidence(&self, target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            mode: self.mode,
            evidence: Some(target),
            started: Arc::clone(&self.started),
            proofs: self.proofs.clone(),
        }))
    }
}

struct EvidenceFakeHandle {
    mode: EvidenceFakeMode,
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    proofs: FakeProofs,
}

#[async_trait]
impl AgentHandle for EvidenceFakeHandle {
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
        if matches!(
            self.mode,
            EvidenceFakeMode::WaitForStop | EvidenceFakeMode::AliveThenDead
        ) {
            std::future::pending::<()>().await;
        }
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "an apparently successful result".to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send((AgentEvent::Finished(outcome.clone())).into())
            .await;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> GroupProof {
        let call = self.proofs.cancel_calls.fetch_add(1, Ordering::AcqRel) + 1;
        if matches!(self.mode, EvidenceFakeMode::AliveThenDead) && call == 1 {
            return GroupProof::Alive;
        }
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

impl Drop for EvidenceFakeHandle {
    fn drop(&mut self) {
        self.proofs.dropped.store(true, Ordering::Release);
    }
}

fn fake_drivers(mode: EvidenceFakeMode, started: Arc<AtomicBool>, proofs: FakeProofs) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(EvidenceFake {
        mode,
        evidence: None,
        started,
        proofs,
    });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

fn evidence_ask() -> AskRequest {
    AskRequest {
        agent: CLAUDE_ID.to_owned(),
        task: PRIVATE_TASK.to_owned(),
        how_many_at_once: 1,
    }
}

fn prepare_fake_run(home: &Path, workspace: &Path) -> Result<(PathBuf, Store), Box<dyn Error>> {
    fs::create_dir_all(home.join("agents"))?;
    fs::create_dir_all(workspace.join(".loadout"))?;
    fs::write(home.join("agents/claude.md"), CLAUDE_AGENT)?;
    let database = workspace.join(".loadout/loadout.db");
    Ok((database.clone(), Store::open(&database)?))
}

fn physical_step(run_dir: &Path, node: &str) -> Result<String, Box<dyn Error>> {
    let run: Value = serde_json::from_slice(&fs::read(run_dir.join("run.json"))?)?;
    run.get("steps")
        .and_then(Value::as_array)
        .and_then(|steps| {
            steps.iter().find(|step| {
                step.get("node_key").and_then(Value::as_str) == Some(node)
                    || step.get("agent").and_then(Value::as_str) == Some(node)
            })
        })
        .and_then(|step| step.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("the product run.json has no physical step for {node}").into())
}

fn assert_step_evidence(
    run_dir: &Path,
    node: &str,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<(), Box<dyn Error>> {
    let step = physical_step(run_dir, node)?;
    let logs = run_dir.join("logs");
    let stdout_path = logs.join(format!("agent-{step}.jsonl"));
    let stderr_path = logs.join(format!("agent-{step}.stderr.log"));
    let manifest_path = logs.join(format!("agent-{step}.input.json"));
    let recorded = fs::read(&stdout_path).unwrap_or_default();
    assert_eq!(
        recorded,
        stdout,
        "the finished product run left no byte-exact stdout at {}; the stream included unknown \
         event {UNKNOWN}, so a tee after parsing would also fail",
        stdout_path.display()
    );
    assert_eq!(
        fs::read(&stderr_path).unwrap_or_default(),
        stderr,
        "stderr was not joined and flushed byte-exactly before completion at {}",
        stderr_path.display()
    );
    let manifest_bytes = fs::read(&manifest_path).unwrap_or_default();
    let manifest: Value = serde_json::from_slice(&manifest_bytes).map_err(|error| {
        format!(
            "safe input manifest missing or invalid at {}: {error}",
            manifest_path.display()
        )
    })?;
    assert!(
        manifest
            .get("promptBytes")
            .and_then(Value::as_u64)
            .is_some_and(|bytes| bytes > 0),
        "the manifest did not retain the safe prompt byte count: {manifest}"
    );
    let manifest_text = String::from_utf8(manifest_bytes)?;
    assert!(
        !manifest_text.contains(PRIVATE_TASK)
            && !manifest_text.contains("claude evidence")
            && !manifest_text.contains("codex evidence"),
        "the safe manifest persisted prompt or output content: {manifest_text}"
    );
    Ok(())
}

fn assert_handoff_context_is_measured_in_graph_order(
    run_dir: &Path,
    node: &str,
    expected_sources: usize,
) -> Result<(), Box<dyn Error>> {
    let step = physical_step(run_dir, node)?;
    let manifest: Value = serde_json::from_slice(&fs::read(
        run_dir
            .join("logs")
            .join(format!("agent-{step}.input.json")),
    )?)?;
    let context = manifest
        .get("context")
        .and_then(Value::as_array)
        .ok_or("the safe manifest has no context array")?;
    let context = context
        .iter()
        .filter(|source| source.get("kind").and_then(Value::as_str) == Some("handoff"))
        .collect::<Vec<_>>();
    assert_eq!(context.len(), expected_sources);

    let mut handoffs = fs::read_dir(run_dir.join("handoffs"))?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    handoffs.sort();
    let expected = handoffs
        .into_iter()
        .take(expected_sources)
        .map(|path| {
            path.strip_prefix(run_dir)
                .map(|relative| relative.display().to_string())
                .map_err(Into::into)
        })
        .collect::<Result<Vec<String>, Box<dyn Error>>>()?;
    let recorded = context
        .into_iter()
        .map(|source| {
            let reference = source
                .get("reference")
                .and_then(Value::as_str)
                .ok_or("a context source has no relative reference")?;
            let bytes = source
                .get("bytes")
                .and_then(Value::as_u64)
                .ok_or("a context source has no byte count")?;
            let path = run_dir.join(reference);
            let metadata = fs::symlink_metadata(&path)?;
            assert!(metadata.is_file() && !metadata.file_type().is_symlink());
            assert_eq!(bytes, metadata.len(), "wrong byte count for {reference}");
            assert!(
                bytes > 0,
                "{reference} was recorded as an unknown/zero size"
            );
            Ok::<_, Box<dyn Error>>(reference.to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(
        recorded, expected,
        "the context manifest did not preserve graph/handoff order"
    );
    Ok(())
}

fn context_of(run_dir: &Path, node: &str) -> Result<Vec<Value>, Box<dyn Error>> {
    let step = physical_step(run_dir, node)?;
    let manifest: Value = serde_json::from_slice(&fs::read(
        run_dir
            .join("logs")
            .join(format!("agent-{step}.input.json")),
    )?)?;
    manifest
        .get("context")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| "the safe manifest has no context array".into())
}

fn source(kind: &str, reference: &str, bytes: usize) -> Value {
    json!({ "kind": kind, "reference": reference, "bytes": bytes })
}

fn plant_memory(home: &Path) -> Result<(), Box<dyn Error>> {
    let notes = home.join("memory/notes");
    fs::create_dir_all(&notes)?;
    for (name, scope, rule) in [
        ("01-everywhere", "everywhere", MEMORY_EVERYWHERE),
        ("02-project", "this-project", MEMORY_PROJECT),
    ] {
        fs::write(
            notes.join(format!("{name}.md")),
            format!(
                "---\nscope: {scope}\nkind: fact\ntitle: Evidence provenance\nrule: {rule}\n\
                 because: T-34 must explain the exact context sources\nstatus: in-use\n\
                 occurrences: 1\nmodified: 2026-08-21T00:00:00Z\nlast_used_at: null\n---\n"
            ),
        )?;
    }
    Ok(())
}

fn trigger_delivery(home: &Path) -> Result<triggers::TriggerDelivery, Box<dyn Error>> {
    let first = serde_json::to_vec(&json!({"data":{"issues":{"nodes":[{
        "id":"old", "identifier":"LOAD-0", "title":"Old", "url":"https://linear/LOAD-0",
        "description":"old", "updatedAt":"2026-08-21T08:00:00.000Z"
    }]}}}))?;
    assert_eq!(
        triggers::poll_with(home, "evidence", 1_777_777_777_000, |_| Ok(first))?,
        TriggerPoll::Armed
    );
    let second = serde_json::to_vec(&json!({"data":{"issues":{"nodes":[{
        "id":"issue-evidence", "identifier":"LOAD-34", "title":"Evidence",
        "url":"https://linear/LOAD-34", "description":"private issue body",
        "updatedAt":"2026-08-21T09:00:00.000Z"
    }]}}}))?;
    match triggers::poll_with(home, "evidence", 1_777_777_777_001, |_| Ok(second))? {
        TriggerPoll::Pending { delivery } => Ok(*delivery),
        other => {
            Err(format!("the authoritative trigger path produced {other:?}, not Pending").into())
        }
    }
}

fn dump_events(conn: &Connection) -> Result<Vec<String>, Box<dyn Error>> {
    let mut statement = conn
        .prepare("SELECT seq, run_id, step_id, ts, kind, level, body FROM events ORDER BY seq")?;
    let rows = statement.query_map([], |row| {
        let seq: i64 = row.get(0)?;
        let run_id: String = row.get(1)?;
        let step: Option<String> = row.get(2)?;
        let ts: i64 = row.get(3)?;
        let kind: String = row.get(4)?;
        let level: String = row.get(5)?;
        let body: Option<String> = row.get(6)?;
        Ok(format!(
            "{seq} · {run_id} · {step:?} · {ts} · {kind} · {level} · {body:?}"
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn remove_database(path: &Path) -> Result<(), Box<dyn Error>> {
    for suffix in ["", "-wal", "-shm"] {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        let file = PathBuf::from(name);
        if file.exists() {
            fs::remove_file(file)?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn inherited_skill_learning_and_role_sources_are_ordered_and_content_free()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let run = tempfile::tempdir()?;
    seed_inherited_sources(project.path())?;

    let inherited = wire::from_the_host(
        project.path(),
        run.path(),
        &Chosen {
            // The selected order is intentionally not scan order. The receipt must describe
            // composition, not a later filesystem sort.
            skills: vec!["beta".to_owned(), "alpha".to_owned()],
            learnings: Some("evidence".to_owned()),
            subagent: Some("evidence".to_owned()),
        },
    )?;
    let sources = inherited.sources();
    assert_inherited_sources(sources);
    let context = sources
        .iter()
        .map(|source| ContextSource {
            kind: match source.kind {
                InheritedSourceKind::Skill => ContextKind::InheritedSkill,
                InheritedSourceKind::Learning => ContextKind::InheritedLearning,
            },
            reference: source.reference.clone(),
            bytes: source.bytes,
        })
        .collect::<Vec<_>>();
    let target = EvidenceTarget::workflow_step(
        run.path().to_path_buf(),
        "inherited-context".to_owned(),
        SafeInputManifest {
            prompt_bytes: 123,
            context,
            images: Vec::new(),
        },
    );
    target.prepare().await?;
    assert_inherited_manifest(&fs::read_to_string(target.input_path())?)?;
    Ok(())
}

fn seed_inherited_sources(project: &Path) -> Result<(), Box<dyn Error>> {
    for (name, body) in [
        ("alpha", INHERITED_SKILL_ALPHA),
        ("beta", INHERITED_SKILL_BETA),
    ] {
        let directory = project.join(".claude/skills").join(name);
        fs::create_dir_all(&directory)?;
        fs::write(directory.join("SKILL.md"), body)?;
    }
    fs::create_dir_all(project.join(".claude/learnings"))?;
    fs::write(
        project.join(".claude/learnings/evidence.md"),
        format!(
            "# Evidence\n\n## Recurring patterns (BINDING)\n\n{INHERITED_LEARNING}\n\n\
             ## Run journal\n\nPRIVATE_INHERITED_JOURNAL_T34\n"
        ),
    )?;
    fs::create_dir_all(project.join(".claude/agents"))?;
    fs::write(
        project.join(".claude/agents/evidence.md"),
        format!(
            "---\nname: private-front-matter\nmodel: PRIVATE_MODEL_T34\n---\n\n\
             {INHERITED_SUBAGENT}\n"
        ),
    )?;
    Ok(())
}

fn assert_inherited_sources(sources: &[wire::InheritedSource]) {
    assert_eq!(sources.len(), 4);
    assert_eq!(sources[0].kind, InheritedSourceKind::Skill);
    assert_eq!(sources[0].reference, "plugin/skills/beta/SKILL.md");
    assert_eq!(sources[0].bytes, INHERITED_SKILL_BETA.len());
    assert_eq!(sources[1].kind, InheritedSourceKind::Skill);
    assert_eq!(sources[1].reference, "plugin/skills/alpha/SKILL.md");
    assert_eq!(sources[1].bytes, INHERITED_SKILL_ALPHA.len());
    assert_eq!(sources[2].kind, InheritedSourceKind::Learning);
    assert_eq!(sources[2].reference, ".claude/learnings/evidence.md");
    assert_eq!(sources[2].bytes, INHERITED_LEARNING.len());
    assert_eq!(sources[3].kind, InheritedSourceKind::Learning);
    assert_eq!(sources[3].reference, ".claude/agents/evidence.md");
    assert_eq!(sources[3].bytes, INHERITED_SUBAGENT.len());
}

fn assert_inherited_manifest(receipt: &str) -> Result<(), Box<dyn Error>> {
    let manifest: Value = serde_json::from_str(receipt)?;
    let expected_context = vec![
        source(
            "inheritedSkill",
            "plugin/skills/beta/SKILL.md",
            INHERITED_SKILL_BETA.len(),
        ),
        source(
            "inheritedSkill",
            "plugin/skills/alpha/SKILL.md",
            INHERITED_SKILL_ALPHA.len(),
        ),
        source(
            "inheritedLearning",
            ".claude/learnings/evidence.md",
            INHERITED_LEARNING.len(),
        ),
        source(
            "inheritedLearning",
            ".claude/agents/evidence.md",
            INHERITED_SUBAGENT.len(),
        ),
    ];
    assert_eq!(
        manifest.get("context").and_then(Value::as_array),
        Some(&expected_context),
        "the persisted safe manifest changed inherited composition order or byte counts"
    );
    for private in [
        INHERITED_SKILL_ALPHA,
        INHERITED_SKILL_BETA,
        INHERITED_LEARNING,
        INHERITED_SUBAGENT,
        "PRIVATE_INHERITED_JOURNAL_T34",
        "PRIVATE_MODEL_T34",
    ] {
        assert!(
            !receipt.contains(private),
            "an inherited source receipt leaked its source content: {receipt}"
        );
    }
    Ok(())
}

#[test]
fn claude_driver_debug_exposes_only_safe_presence_and_counts() {
    const PRIVATE_DEBUG: &str = "PRIVATE_CLAUDE_DEBUG_T34";
    let (lines, _source) = mpsc::channel(QUEUE_CAP);
    let driver =
        ClaudeDriver::with_binary(PathBuf::from(format!("/private/{PRIVATE_DEBUG}/claude")))
            .with_transcript(Transcript {
                run_dir: PathBuf::from(format!("/private/{PRIVATE_DEBUG}/run")),
                step: PRIVATE_DEBUG.to_owned(),
                agent: PRIVATE_DEBUG.to_owned(),
                lines,
            })
            .with_inherited(vec![format!("--{PRIVATE_DEBUG}")]);
    let debug = format!("{driver:?}");
    assert!(debug.contains("inherited_arguments: 1"));
    assert!(
        !debug.contains(PRIVATE_DEBUG),
        "ClaudeDriver Debug exposed a binary, path, step, agent, or argv value: {debug}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workflow_trigger_and_ask_leave_both_vendor_streams_and_rebuild_exactly()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let (workflow, drivers) = prepare_product_inputs(home.path(), workspace.path())?;
    let database = workspace.path().join(".loadout/loadout.db");
    let store = Store::open(&database)?;
    let deps = RunDeps {
        home: home.path(),
        project: workspace.path(),
        store: &store,
        drivers,
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
        task: Some(PRIVATE_TASK.to_owned()),
    };
    let reports = run_product_doors(&deps, &request, home.path()).await?;
    assert_product_evidence(&reports)?;
    assert_product_counters(workspace.path(), &reports[0])?;
    drop(deps);
    rebuild_matches_live_index(store, &database, &reports).await?;
    Ok(())
}

fn prepare_product_inputs(
    home: &Path,
    workspace: &Path,
) -> Result<(PathBuf, Drivers), Box<dyn Error>> {
    for directory in ["agents", "workflows", "triggers"] {
        fs::create_dir_all(home.join(directory))?;
    }
    fs::create_dir_all(workspace.join(".loadout"))?;
    fs::write(home.join("agents/claude.md"), CLAUDE_AGENT)?;
    fs::write(home.join("agents/codex.md"), CODEX_AGENT)?;
    plant_memory(home)?;
    let workflow = home.join("workflows/evidence.json");
    fs::write(&workflow, WORKFLOW)?;
    let workspace_text = workspace.to_str().ok_or("workspace path is not UTF-8")?;
    workspaces::save_workspace_inner(home, "Evidence workspace", workspace_text)?;
    fs::write(
        home.join("triggers/evidence.json"),
        serde_json::to_vec_pretty(&json!({
            "schema": 1, "source": "linear", "enabled": true,
            "workflow": "evidence.json", "workspace": workspace_text,
            "condition": "assigned-to-me", "api_key": TRIGGER_KEY
        }))?,
    )?;
    let claude_binary = executable(home, "claude", CLAUDE_FAKE)?;
    let codex_binary = executable(home, "codex", CODEX_FAKE)?;
    fs::write(home.join("claude.stdout.jsonl"), CLAUDE_STDOUT)?;
    fs::write(home.join("claude.stderr.log"), CLAUDE_STDERR)?;
    fs::write(home.join("codex.stdout.jsonl"), CODEX_STDOUT)?;
    fs::write(home.join("codex.stderr.log"), CODEX_STDERR)?;
    Ok((workflow, driver_factory(claude_binary, codex_binary)))
}

async fn run_product_doors(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    home: &Path,
) -> Result<[RunReport; 3], Box<dyn Error>> {
    let (manual_lines, _manual_source) = line_channel(QUEUE_CAP);
    let manual = tokio::time::timeout(PATIENCE, run_workflow_inner(deps, request, manual_lines))
        .await
        .map_err(|_| "the manual workflow did not finish")??;

    let delivery = trigger_delivery(home)?;
    let (trigger_lines, _trigger_source) = line_channel(QUEUE_CAP);
    let triggered = tokio::time::timeout(
        PATIENCE,
        run_triggered_workflow_inner(deps, request, &delivery.claim, trigger_lines),
    )
    .await
    .map_err(|_| "the authoritative trigger workflow did not finish")??;
    let TriggerRunReport::Ran(triggered) = triggered else {
        return Err("the new trigger delivery was treated as an earlier accepted run".into());
    };

    let ask = AskRequest {
        agent: CODEX_ID.to_owned(),
        task: PRIVATE_TASK.to_owned(),
        how_many_at_once: 1,
    };
    let (ask_lines, _ask_source) = line_channel(QUEUE_CAP);
    let asked = tokio::time::timeout(PATIENCE, run_agent_inner(deps, &ask, ask_lines))
        .await
        .map_err(|_| "the /ask product path did not finish")??;
    Ok([manual, triggered, asked])
}

fn assert_product_evidence(reports: &[RunReport; 3]) -> Result<(), Box<dyn Error>> {
    for report in &reports[..2] {
        assert_step_evidence(
            &report.dir,
            "s_claude",
            CLAUDE_STDOUT.as_bytes(),
            CLAUDE_STDERR,
        )?;
        assert_step_evidence(
            &report.dir,
            "s_claude_two",
            CLAUDE_STDOUT.as_bytes(),
            CLAUDE_STDERR,
        )?;
        assert_step_evidence(
            &report.dir,
            "s_codex",
            CODEX_STDOUT.as_bytes(),
            CODEX_STDERR,
        )?;
        assert_handoff_context_is_measured_in_graph_order(&report.dir, "s_codex", 2)?;

        let memory = [
            source(
                "memoryNote",
                "memory/notes/01-everywhere.md",
                MEMORY_EVERYWHERE.len(),
            ),
            source(
                "memoryNote",
                "memory/notes/02-project.md",
                MEMORY_PROJECT.len(),
            ),
        ];
        let first = context_of(&report.dir, "s_claude")?;
        assert_eq!(
            first,
            [
                memory[0].clone(),
                memory[1].clone(),
                source("runTask", "run/task", PRIVATE_TASK.len()),
                source(
                    "workflowStep",
                    "workflow/steps/0",
                    "leave the Claude evidence receipt".len(),
                ),
            ],
            "the first step manifest did not mirror the exact context composition order"
        );
        let second = context_of(&report.dir, "s_codex")?;
        let handoffs = second
            .iter()
            .filter(|item| item.get("kind").and_then(Value::as_str) == Some("handoff"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(handoffs.len(), 2, "both typed handoffs must be present");
        assert_eq!(
            &second[..4],
            &[
                memory[0].clone(),
                memory[1].clone(),
                source("runTask", "run/task", PRIVATE_TASK.len()),
                source(
                    "workflowStep",
                    "workflow/steps/2",
                    "leave the Codex evidence receipt".len(),
                ),
            ],
            "memory, run task and workflow instruction lost their typed order"
        );
    }
    assert_step_evidence(
        &reports[2].dir,
        CODEX_ID,
        CODEX_STDOUT.as_bytes(),
        CODEX_STDERR,
    )?;
    assert_eq!(
        context_of(&reports[2].dir, CODEX_ID)?,
        vec![
            source(
                "memoryNote",
                "memory/notes/01-everywhere.md",
                MEMORY_EVERYWHERE.len(),
            ),
            source(
                "memoryNote",
                "memory/notes/02-project.md",
                MEMORY_PROJECT.len(),
            ),
            source("runTask", "ask/task", PRIVATE_TASK.len()),
        ],
        "/ask duplicated its sentence as both a task and workflow instruction"
    );
    Ok(())
}

fn assert_product_counters(workspace: &Path, manual: &RunReport) -> Result<(), Box<dyn Error>> {
    let manual_run: Value = serde_json::from_slice(&fs::read(manual.dir.join("run.json"))?)?;
    let codex_step = manual_run
        .get("steps")
        .and_then(Value::as_array)
        .and_then(|steps| {
            steps
                .iter()
                .find(|step| step.get("node_key").and_then(Value::as_str) == Some("s_codex"))
        })
        .ok_or("the product run omitted its Codex step")?;
    assert_eq!(codex_step.get("turns").and_then(Value::as_u64), Some(1));
    assert_eq!(
        codex_step.get("input_tokens").and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(
        codex_step.get("output_tokens").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        codex_step.get("cached_tokens").and_then(Value::as_u64),
        Some(1)
    );
    let report_document: Value = serde_json::from_str(support_report(workspace)?.text())?;
    let safe_run_id = manual
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the product run directory has no safe Loadout identity")?;
    let safe_codex = report_document
        .get("runs")
        .and_then(Value::as_array)
        .and_then(|runs| {
            runs.iter()
                .find(|run| run.get("id").and_then(Value::as_str) == Some(safe_run_id))
                .and_then(|run| run.get("steps"))
                .and_then(Value::as_array)
                .and_then(|steps| {
                    steps
                        .iter()
                        .find(|step| step.get("id") == codex_step.get("id"))
                })
        })
        .ok_or("the support report omitted the product Codex step")?;
    assert_eq!(safe_codex.get("turns"), codex_step.get("turns"));
    assert_eq!(
        safe_codex.get("inputTokens").and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(
        safe_codex.get("outputTokens").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        safe_codex.get("cachedTokens").and_then(Value::as_u64),
        Some(1)
    );
    Ok(())
}

async fn rebuild_matches_live_index(
    store: Store,
    database: &Path,
    reports: &[RunReport; 3],
) -> Result<(), Box<dyn Error>> {
    let before = dump_events(&store.reader()?)?;
    assert!(
        !before.is_empty(),
        "the live product index contained no vendor events"
    );
    store.close().await?;
    remove_database(database)?;
    assert!(
        !database.exists(),
        "loadout.db survived the deletion fixture"
    );
    let rebuilt = Store::open(database)?;
    for report in reports {
        rebuilt.rebuild_from(&report.dir).await?;
    }
    let after = dump_events(&rebuilt.reader()?)?;
    assert_eq!(
        after, before,
        "deleting loadout.db changed the order or content reconstructed from private evidence"
    );
    rebuilt.close().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn codex_follow_up_process_appends_instead_of_truncating_the_first_turn()
-> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    fs::create_dir_all(dir.path().join("logs"))?;
    let binary = executable(dir.path(), "codex", CODEX_FAKE)?;
    fs::write(dir.path().join("codex.stdout.jsonl"), CODEX_STDOUT)?;
    fs::write(dir.path().join("codex.stderr.log"), CODEX_STDERR)?;
    let base: Arc<dyn AgentDriver> = Arc::new(CodexDriver::with_binary(binary));
    let target = EvidenceTarget::workflow_step(
        dir.path().to_path_buf(),
        "codex-append".to_owned(),
        SafeInputManifest::default(),
    );
    let driver = base
        .with_evidence(target)
        .ok_or("Codex has no production evidence seam")?;
    let (tx, _events) = mpsc::channel(QUEUE_CAP);
    let mut handle: Box<dyn AgentHandle> = driver
        .start(
            RunSpec {
                run_id: Uuid::now_v7(),
                cwd: dir.path().to_path_buf(),
                prompt: "first private turn".to_owned(),
                model: None,
                system_append: None,
                policy: Policy::ReadOnly,
                tools: None,
                extra_dirs: Vec::new(),
                resume: None,
            },
            tx,
        )
        .await?;
    let _first = tokio::time::timeout(PATIENCE, handle.wait()).await??;
    handle.send("second private turn".to_owned()).await?;
    let _second = tokio::time::timeout(PATIENCE, handle.wait()).await??;
    let _closed = handle.close().await?;
    let mut twice = CODEX_STDOUT.as_bytes().to_vec();
    twice.extend_from_slice(CODEX_STDOUT.as_bytes());
    assert_eq!(
        fs::read(dir.path().join("logs/agent-codex-append.jsonl"))?,
        twice,
        "the resumed Codex process truncated the first turn or reordered the append"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn poisoned_evidence_turns_an_apparent_success_into_a_failed_product_run()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let (_database, store) = prepare_fake_run(home.path(), workspace.path())?;
    let started = Arc::new(AtomicBool::new(false));
    let deps = RunDeps {
        home: home.path(),
        project: workspace.path(),
        store: &store,
        drivers: fake_drivers(
            EvidenceFakeMode::PoisonThenFinish,
            started,
            FakeProofs::default(),
        ),
        control: RunControl::new(),
    };
    let (lines, _source) = line_channel(QUEUE_CAP);
    let report = tokio::time::timeout(PATIENCE, run_agent_inner(&deps, &evidence_ask(), lines))
        .await
        .map_err(|_| "the poisoned evidence run did not finish")??;

    assert_eq!(
        report.steps,
        vec![StepState::Failed],
        "a successful model answer was accepted after its evidence writer failed"
    );
    let run: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    assert_eq!(run.get("status").and_then(Value::as_str), Some("failed"));
    assert_eq!(
        run.pointer("/steps/0/error").and_then(Value::as_str),
        Some(
            "Loadout could not preserve this agent's private run evidence. The step was not \
             accepted as complete."
        )
    );
    let handoffs = match fs::read_dir(report.dir.join("handoffs")) {
        Ok(entries) => entries.count(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => return Err(error.into()),
    };
    assert_eq!(
        handoffs, 0,
        "a result with incomplete private evidence was handed to a downstream step"
    );
    store.close().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_actual_dead_group_proof_is_persisted_on_the_cancelled_product_step()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let (_database, store) = prepare_fake_run(home.path(), workspace.path())?;
    let started = Arc::new(AtomicBool::new(false));
    let control = RunControl::new();
    let deps = RunDeps {
        home: home.path(),
        project: workspace.path(),
        store: &store,
        drivers: fake_drivers(
            EvidenceFakeMode::WaitForStop,
            Arc::clone(&started),
            FakeProofs::default(),
        ),
        control: control.clone(),
    };
    let (lines, _source) = line_channel(QUEUE_CAP);
    let ask = evidence_ask();
    let stopped = async {
        while !started.load(Ordering::Acquire) {
            tokio::task::yield_now().await;
        }
        control.stop();
    };
    let (ran, ()) = tokio::time::timeout(PATIENCE, async {
        tokio::join!(run_agent_inner(&deps, &ask, lines), stopped)
    })
    .await
    .map_err(|_| "the cancelled evidence run did not finish")?;
    let report = ran?;

    assert_eq!(report.steps, vec![StepState::Cancelled]);
    let run: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    assert_eq!(run.get("status").and_then(Value::as_str), Some("cancelled"));
    assert_eq!(
        run.pointer("/steps/0/death_proof").and_then(Value::as_bool),
        Some(true),
        "the real GroupProof::Dead vanished before the durable run document"
    );
    store.close().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_alive_cancel_proof_keeps_the_handle_owned_until_a_retry_proves_dead()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let (_database, store) = prepare_fake_run(home.path(), workspace.path())?;
    let started = Arc::new(AtomicBool::new(false));
    let proofs = FakeProofs::default();
    let control = RunControl::new();
    let deps = RunDeps {
        home: home.path(),
        project: workspace.path(),
        store: &store,
        drivers: fake_drivers(
            EvidenceFakeMode::AliveThenDead,
            Arc::clone(&started),
            proofs.clone(),
        ),
        control: control.clone(),
    };
    let (lines, _source) = line_channel(QUEUE_CAP);
    let stopped = async {
        while !started.load(Ordering::Acquire) {
            tokio::task::yield_now().await;
        }
        control.stop();
    };
    let ask = evidence_ask();
    let (ran, ()) = tokio::time::timeout(PATIENCE, async {
        tokio::join!(run_agent_inner(&deps, &ask, lines), stopped)
    })
    .await
    .map_err(|_| "the Alive-then-Dead cleanup did not finish")?;
    let report = ran?;

    assert_eq!(
        proofs.cancel_calls.load(Ordering::Acquire),
        2,
        "Loadout did not retry the same owned handle after GroupProof::Alive"
    );
    assert!(
        proofs.dropped.load(Ordering::Acquire),
        "the handle was not released after the second cancel proved Dead"
    );
    let run: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    assert_eq!(
        run.pointer("/steps/0/death_proof").and_then(Value::as_bool),
        Some(true)
    );
    store.close().await?;
    Ok(())
}
