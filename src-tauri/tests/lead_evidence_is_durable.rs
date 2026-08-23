//! T-34 AC-2: real Lead conversations append private evidence and complete only after `Dead`.

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use loadout_lib::commands::Drivers;
use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::commands::chat::{Lead, Terminal, Threads};
use loadout_lib::commands::diagnostics::support_report;
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as AgentOutcome,
    Policy, Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{
    GroupId, GroupProof, PrivateFileAccess, PrivateFilePublisher, open_private_file,
};
use loadout_lib::evidence::{
    ConversationMetadata, ConversationVendor, EvidenceFailureKind, EvidenceTarget,
    SafeInputManifest, TurnCounters,
};
use loadout_lib::ipc::{QUEUE_CAP, line_channel};
use loadout_lib::library::agents::{Agent, Vendor};
use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

const PATIENCE: Duration = Duration::from_secs(12);
const FIRST: &str = "PRIVATE_FIRST_LEAD_T34";
const SECOND: &str = "PRIVATE_SECOND_LEAD_T34";
const CODEX_ECHO: &str = "PRIVATE_CODEX_ECHO_T34";

const CLAUDE_INIT: &str = "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"lead-claude\",\"model\":\"sonnet\",\"tools\":[]}\n";
const CLAUDE_ONE: &str = concat!(
    "{\"type\":\"a_future_event\",\"turn\":1}\n",
    "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"CLAUDE_SAFE_ONE\"}]}}\n",
    "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":1,\"result\":\"CLAUDE_SAFE_ONE\"}\n",
);
const CLAUDE_TWO: &str = concat!(
    "{\"type\":\"a_future_event\",\"turn\":2}\n",
    "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"CLAUDE_SAFE_TWO\"}]}}\n",
    "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":1,\"result\":\"CLAUDE_SAFE_TWO\"}\n",
);
const CLAUDE_ERR_ONE: &[u8] = b"claude stderr one\n\xff\n";
const CLAUDE_ERR_TWO: &[u8] = b"claude stderr two\n\xfe\n";
const CODEX_ERR_ONE: &[u8] = b"codex stderr one\n\xff\n";
const CODEX_ERR_TWO: &[u8] = b"codex stderr two\n\xfe\n";

const CLAUDE_FAKE: &str = r#"#!/bin/sh
if [ "${1-}" = "--version" ]; then
  printf '%s\n' '2.1.238 (Claude Code)'
  exit 0
fi
here="$(dirname "$0")"
printf '%s\n' "$$" >> "$here/claude.pid.log"
cat "$here/claude.init.jsonl"
turn=0
while IFS= read -r line; do
  turn=$((turn + 1))
  printf '%s\n' "$line" >> "$here/claude.stdin.jsonl"
  cat "$here/claude.turn.$turn.jsonl"
  sleep 0.03
  cat "$here/claude.stderr.$turn.log" >&2
done
exit 0
"#;

// The same binary is parser-faithful for the old exec path and the required Lead app-server.
// That makes the before failure about missing product evidence, not a dummy that cannot start.
const CODEX_FAKE: &str = r#"#!/bin/sh
if [ "${1-}" = "--version" ]; then
  printf '%s\n' 'codex-cli 0.148.0'
  exit 0
fi
here="$(dirname "$0")"
printf '%s\n' "$@" > "$here/codex.argv.log"
case " $* " in
  *" app-server "*)
    turn=0
    while IFS= read -r line; do
      printf '%s\n' "$line" >> "$here/codex.stdin.jsonl"
      id="$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')"
      case "$line" in
        *'"method":"initialize"'*)
          printf '{"id":%s,"result":{}}\n' "${id:-1}"
          ;;
        *'"method":"thread/start"'*)
          printf '{"id":%s,"result":{"thread":{"id":"thread-lead","ephemeral":true,"path":null}}}\n' "${id:-2}"
          ;;
        *'"method":"turn/start"'*)
          turn=$((turn + 1))
          printf '{"id":%s,"result":{"turn":{"id":"turn-%s","status":"inProgress"}}}\n' "${id:-3}" "$turn"
          printf '{"method":"item/completed","params":{"threadId":"thread-lead","turnId":"turn-%s","item":{"type":"agentMessage","text":"CODEX_SAFE_%s"}}}\n' "$turn" "$turn"
          # app-server echoes user input. This line must be parsed and omitted before evidence.
          printf '{"method":"codex/event/user_message","params":{"text":"PRIVATE_CODEX_ECHO_T34"}}\n'
          printf '{"method":"turn/completed","params":{"threadId":"thread-lead","turn":{"id":"turn-%s","status":"completed"},"usage":{"inputTokens":%s,"outputTokens":%s}}}\n' "$turn" "$turn" "$turn"
          cat "$here/codex.stderr.$turn.log" >&2
          ;;
        *'interrupt"'*)
          printf '{"id":%s,"result":{}}\n' "${id:-9}"
          ;;
      esac
    done
    exit 0
    ;;
esac

# Compatibility branch: before the Lead-only seam exists, old Threads calls `codex exec`.
cat >> "$here/codex.exec.stdin.log"
printf '%s\n' '{"type":"thread.started","thread_id":"thread-lead"}'
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"legacy"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":1,"cached_input_tokens":0,"output_tokens":1}}'
exit 0
"#;

fn executable(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn saved_lead(library: &Path, name: &str, vendor: Vendor) -> Result<Agent, Box<dyn Error>> {
    let mut agent = Agent::example();
    agent.id = Uuid::now_v7();
    name.clone_into(&mut agent.name);
    agent.runs_with = vendor;
    let _path = save_agent_inner(library, &agent)?;
    Ok(agent)
}

fn conversations(root: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut paths = match fs::read_dir(root) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                entry
                    .file_type()
                    .ok()
                    .filter(std::fs::FileType::is_dir)
                    .map(|_| entry.path())
            })
            .collect::<Vec<_>>(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    paths.sort();
    Ok(paths)
}

async fn wait_for_conversation(
    root: &Path,
    known: &BTreeSet<PathBuf>,
) -> Result<PathBuf, Box<dyn Error>> {
    let began = Instant::now();
    loop {
        if let Some(path) = conversations(root)?
            .into_iter()
            .find(|path| !known.contains(path))
        {
            return Ok(path);
        }
        if began.elapsed() >= PATIENCE {
            return Err(format!(
                "the production Threads path left no new conversation directory at {}",
                root.display()
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn wait_for_file_with(path: &Path, needle: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let began = Instant::now();
    loop {
        if let Ok(bytes) = fs::read(path)
            && (needle.is_empty() || bytes.windows(needle.len()).any(|window| window == needle))
        {
            return Ok(bytes);
        }
        if began.elapsed() >= PATIENCE {
            return Err(format!(
                "{} never contained the second completed turn marker",
                path.display()
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn complete_at(dir: &Path) -> Result<bool, Box<dyn Error>> {
    let document: Value = serde_json::from_slice(&fs::read(dir.join("conversation.json"))?)?;
    document
        .get("complete")
        .and_then(Value::as_bool)
        .ok_or_else(|| "conversation.json has no boolean complete field".into())
}

async fn wait_for_state(path: &Path, state: &str) -> Result<Value, Box<dyn Error>> {
    let began = Instant::now();
    loop {
        if let Ok(bytes) = fs::read(path)
            && let Ok(document) = serde_json::from_slice::<Value>(&bytes)
            && document.get("state").and_then(Value::as_str) == Some(state)
        {
            return Ok(document);
        }
        if began.elapsed() >= PATIENCE {
            return Err(format!("{} never reached state {state}", path.display()).into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn assert_no_private_bytes(root: &Path) -> Result<(), Box<dyn Error>> {
    fn visit(root: &Path, path: &Path) -> Result<(), Box<dyn Error>> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                visit(root, &entry.path())?;
            } else if kind.is_file() {
                let bytes = fs::read(entry.path())?;
                for private in [FIRST, SECOND, CODEX_ECHO] {
                    assert!(
                        !bytes
                            .windows(private.len())
                            .any(|window| window == private.as_bytes()),
                        "private prompt/image echo escaped into {} under {}",
                        entry.path().display(),
                        root.display()
                    );
                }
            }
        }
        Ok(())
    }
    visit(root, root)
}

async fn exercise_lead_case(
    threads: &Threads,
    drivers: &Drivers,
    lead: &Lead,
    terminal_id: &str,
    workspace: &Path,
    fixture: &Path,
    known: &BTreeSet<PathBuf>,
) -> Result<PathBuf, Box<dyn Error>> {
    let terminal = Terminal {
        id: terminal_id.to_owned(),
        folder: workspace.to_path_buf(),
    };
    let (lines, _source) = line_channel(QUEUE_CAP);
    threads.terminal_lines_go_to(&terminal, lines);
    threads.say_in(drivers, lead, &terminal, FIRST).await?;
    let dir = wait_for_conversation(&workspace.join(".loadout/conversations"), known).await?;
    let is_claude = terminal_id.contains("claude");
    let second_marker = if is_claude {
        b"CLAUDE_SAFE_TWO".as_slice()
    } else {
        b"CODEX_SAFE_2".as_slice()
    };
    assert_first_turn(&dir, is_claude).await?;

    threads.say_in(drivers, lead, &terminal, SECOND).await?;
    let log = wait_for_file_with(&dir.join("logs/lead.jsonl"), second_marker).await?;
    let stderr_marker = if is_claude {
        b"claude stderr two".as_slice()
    } else {
        b"codex stderr two".as_slice()
    };
    let _stderr = wait_for_file_with(&dir.join("logs/lead.stderr.log"), stderr_marker).await?;
    assert_second_turn(&dir, is_claude).await?;
    assert_vendor_transport(fixture, &log, is_claude)?;

    let proof = threads.close_at(terminal_id).await;
    assert!(
        matches!(proof, Some(GroupProof::Dead { .. })),
        "close returned {proof:?}"
    );
    assert!(complete_at(&dir)?);
    let closed: Value = serde_json::from_slice(&fs::read(dir.join("conversation.json"))?)?;
    assert_eq!(closed.get("state").and_then(Value::as_str), Some("closed"));
    assert_eq!(
        closed.get("deathProof").and_then(Value::as_bool),
        Some(true)
    );
    Ok(dir)
}

async fn assert_first_turn(dir: &Path, is_claude: bool) -> Result<(), Box<dyn Error>> {
    assert!(!complete_at(dir)?);
    let first = wait_for_state(&dir.join("turns/0001.json"), "succeeded").await?;
    let live: Value = serde_json::from_slice(&fs::read(dir.join("conversation.json"))?)?;
    for key in ["attempts", "turns", "agentTurns"] {
        assert_eq!(live.get(key).and_then(Value::as_u64), Some(1));
    }
    assert_eq!(live.get("complete").and_then(Value::as_bool), Some(false));
    assert_eq!(live.get("state").and_then(Value::as_str), Some("active"));
    assert_eq!(
        live.get("modelConfigured").and_then(Value::as_bool),
        Some(true)
    );
    let vendor = if is_claude { "claude" } else { "codex" };
    assert_eq!(live.get("vendor").and_then(Value::as_str), Some(vendor));
    assert_eq!(first.get("turns").and_then(Value::as_u64), Some(1));
    let tokens = u64::from(!is_claude);
    assert_eq!(
        first.get("inputTokens").and_then(Value::as_u64),
        Some(tokens)
    );
    assert_eq!(
        first.get("outputTokens").and_then(Value::as_u64),
        Some(tokens)
    );
    Ok(())
}

async fn assert_second_turn(dir: &Path, is_claude: bool) -> Result<(), Box<dyn Error>> {
    let second = wait_for_state(&dir.join("turns/0002.json"), "succeeded").await?;
    let live: Value = serde_json::from_slice(&fs::read(dir.join("conversation.json"))?)?;
    for key in ["attempts", "turns", "agentTurns"] {
        assert_eq!(live.get(key).and_then(Value::as_u64), Some(2));
    }
    let tokens = u64::from(!is_claude);
    assert_eq!(
        live.get("inputTokens").and_then(Value::as_u64),
        Some(tokens * 2)
    );
    assert_eq!(
        live.get("outputTokens").and_then(Value::as_u64),
        Some(tokens * 2)
    );
    assert_eq!(second.get("turns").and_then(Value::as_u64), Some(1));
    assert_eq!(
        second.get("inputTokens").and_then(Value::as_u64),
        Some(tokens)
    );
    assert_eq!(
        second.get("outputTokens").and_then(Value::as_u64),
        Some(tokens)
    );
    assert!(!complete_at(dir)?);
    Ok(())
}

fn assert_vendor_transport(
    fixture: &Path,
    log: &[u8],
    is_claude: bool,
) -> Result<(), Box<dyn Error>> {
    if is_claude {
        let mut expected = CLAUDE_INIT.as_bytes().to_vec();
        expected.extend_from_slice(CLAUDE_ONE.as_bytes());
        expected.extend_from_slice(CLAUDE_TWO.as_bytes());
        assert_eq!(
            log, expected,
            "Claude stdout was not byte-exact and append-only"
        );
        return Ok(());
    }
    let argv = fs::read_to_string(fixture.join("codex.argv.log"))?;
    assert_eq!(
        argv.lines().collect::<Vec<_>>(),
        ["app-server", "--listen", "stdio://"]
    );
    let stdin = fs::read_to_string(fixture.join("codex.stdin.jsonl"))?;
    assert_eq!(stdin.matches("\"method\":\"thread/start\"").count(), 1);
    assert_eq!(stdin.matches("\"method\":\"turn/start\"").count(), 2);
    assert!(stdin.contains("\"ephemeral\":true"));
    let log = String::from_utf8_lossy(log);
    assert!(log.contains("CODEX_SAFE_1") && log.contains("CODEX_SAFE_2"));
    assert!(!log.contains(CODEX_ECHO));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concrete_claude_and_codex_append_two_turns_and_complete_only_after_dead()
-> Result<(), Box<dyn Error>> {
    let library = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    let threads = Threads::new();
    threads.library_is(library.path().to_path_buf());

    fs::write(fixture.path().join("claude.init.jsonl"), CLAUDE_INIT)?;
    fs::write(fixture.path().join("claude.turn.1.jsonl"), CLAUDE_ONE)?;
    fs::write(fixture.path().join("claude.turn.2.jsonl"), CLAUDE_TWO)?;
    fs::write(fixture.path().join("claude.stderr.1.log"), CLAUDE_ERR_ONE)?;
    fs::write(fixture.path().join("claude.stderr.2.log"), CLAUDE_ERR_TWO)?;
    fs::write(fixture.path().join("codex.stderr.1.log"), CODEX_ERR_ONE)?;
    fs::write(fixture.path().join("codex.stderr.2.log"), CODEX_ERR_TWO)?;
    let claude_binary = executable(fixture.path(), "claude", CLAUDE_FAKE)?;
    let codex_binary = executable(fixture.path(), "codex", CODEX_FAKE)?;
    let claude_driver: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(claude_binary));
    let codex_driver: Arc<dyn AgentDriver> = Arc::new(CodexDriver::with_binary(codex_binary));
    let drivers: Drivers = Arc::new(move |vendor| match vendor {
        Vendor::ClaudeCode => Arc::clone(&claude_driver),
        Vendor::Codex => Arc::clone(&codex_driver),
    });

    let claude = saved_lead(library.path(), "Claude Lead", Vendor::ClaudeCode)?;
    let codex = saved_lead(library.path(), "Codex Lead", Vendor::Codex)?;
    let cases = [
        (
            "terminal-claude",
            Lead::pointed_at(library.path(), Some(&claude.id.to_string()))
                .map_err(|error| error.to_string())?,
        ),
        (
            "terminal-codex",
            Lead::pointed_at(library.path(), Some(&codex.id.to_string()))
                .map_err(|error| error.to_string())?,
        ),
    ];
    let root = workspace.path().join(".loadout/conversations");
    let mut known = BTreeSet::new();
    let mut claude_dir = None;
    let mut codex_dir = None;

    for (terminal_id, lead) in &cases {
        let dir = exercise_lead_case(
            &threads,
            &drivers,
            lead,
            terminal_id,
            workspace.path(),
            fixture.path(),
            &known,
        )
        .await?;
        known.insert(dir.clone());
        if terminal_id.contains("claude") {
            claude_dir = Some(dir.clone());
        } else {
            codex_dir = Some(dir.clone());
        }
    }

    assert_eq!(conversations(&root)?.len(), 2);
    assert!(
        !workspace.path().join(".loadout/runs").exists(),
        "talking to Lead created a fake workflow run"
    );
    assert_no_private_bytes(workspace.path().join(".loadout").as_path())?;

    assert_safe_conversation_report(workspace.path())?;
    assert_stderr_append(claude_dir, codex_dir)?;
    Ok(())
}

fn assert_safe_conversation_report(workspace: &Path) -> Result<(), Box<dyn Error>> {
    let safe_text = support_report(workspace)?.text().to_owned();
    let safe: Value = serde_json::from_str(&safe_text)?;
    let conversations = safe
        .get("conversations")
        .and_then(Value::as_array)
        .ok_or("the support report omitted real Lead conversations")?;
    assert_eq!(conversations.len(), 2);
    for vendor in ["claude", "codex"] {
        let facts = conversations
            .iter()
            .find(|facts| facts.get("vendor").and_then(Value::as_str) == Some(vendor))
            .ok_or("the support report lost a real Lead vendor")?;
        assert_eq!(facts.get("state").and_then(Value::as_str), Some("closed"));
        assert_eq!(facts.get("complete").and_then(Value::as_bool), Some(true));
        assert_eq!(
            facts
                .pointer("/deathProof/present")
                .and_then(Value::as_bool),
            Some(true)
        );
        for key in ["attempts", "turns", "agentTurns"] {
            assert_eq!(facts.get(key).and_then(Value::as_u64), Some(2));
        }
        for key in ["createdAt", "startedAt", "endedAt"] {
            assert!(facts.get(key).and_then(Value::as_i64).is_some());
        }
        assert!(
            facts
                .as_object()
                .is_some_and(|facts| facts.contains_key("exitCode"))
        );
    }
    for private in [FIRST, SECOND, CODEX_ECHO] {
        assert!(!safe_text.contains(private));
    }
    Ok(())
}

fn assert_stderr_append(
    claude_dir: Option<PathBuf>,
    codex_dir: Option<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    let claude = fs::read(
        claude_dir
            .ok_or("missing Claude conversation")?
            .join("logs/lead.stderr.log"),
    )?;
    let mut expected = CLAUDE_ERR_ONE.to_vec();
    expected.extend_from_slice(CLAUDE_ERR_TWO);
    assert_eq!(
        claude, expected,
        "Claude stderr was not byte-exact across turns"
    );

    let codex = fs::read(
        codex_dir
            .ok_or("missing Codex conversation")?
            .join("logs/lead.stderr.log"),
    )?;
    let mut expected = CODEX_ERR_ONE.to_vec();
    expected.extend_from_slice(CODEX_ERR_TWO);
    assert_eq!(
        codex, expected,
        "Codex stderr was not byte-exact across turns"
    );
    Ok(())
}

#[derive(Clone)]
struct RefusesFollowUp {
    evidence: Option<EvidenceTarget>,
}

#[async_trait]
impl AgentDriver for RefusesFollowUp {
    fn id(&self) -> &'static str {
        "claude"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("receipt-regression".to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let _evidence = self
            .evidence
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("the product omitted conversation evidence"))?;
        let session = SessionRef {
            vendor: "claude",
            id: spec.run_id.to_string(),
        };
        let outcome = AgentOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "safe first response".to_owned(),
            cost_usd: None,
            tokens: Tokens {
                input: 7,
                output: 5,
                cached: 3,
            },
            turns: 2,
            took: Duration::from_millis(1),
            session: session.clone(),
        };
        events
            .send(AgentEvent::Finished(outcome.clone()).into())
            .await
            .map_err(|_| anyhow::anyhow!("the product dropped its Lead event reader"))?;
        Ok(Box::new(RefusingHandle {
            events,
            session,
            outcome: Some(outcome),
        }))
    }

    fn with_evidence(&self, target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            evidence: Some(target),
        }))
    }
}

struct RefusingHandle {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    outcome: Option<AgentOutcome>,
}

#[async_trait]
impl AgentHandle for RefusingHandle {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        anyhow::bail!("PRIVATE_FOLLOW_UP_TRANSPORT_ERROR_T34")
    }

    async fn wait(&mut self) -> anyhow::Result<AgentOutcome> {
        self.outcome
            .take()
            .ok_or_else(|| anyhow::anyhow!("the first turn was already collected"))
    }

    async fn cancel(&mut self) -> GroupProof {
        // Nadajnik zostaje do `Drop`: `finish_dead_session` ma wtedy realny EOF i dowodzi,
        // że zakolejkowany `Finished` został przeniesiony do receiptu przed `complete: true`.
        let _still_owned = &self.events;
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failed_follow_up_is_an_explicit_attempt_and_never_a_phantom_success()
-> Result<(), Box<dyn Error>> {
    let library = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let threads = Threads::new();
    threads.library_is(library.path().to_path_buf());
    let agent = saved_lead(library.path(), "Receipt Lead", Vendor::ClaudeCode)?;
    let lead = Lead::pointed_at(library.path(), Some(&agent.id.to_string()))
        .map_err(|error| error.to_string())?;
    let driver: Arc<dyn AgentDriver> = Arc::new(RefusesFollowUp { evidence: None });
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));
    let terminal = Terminal {
        id: "terminal-refused-follow-up".to_owned(),
        folder: workspace.path().to_path_buf(),
    };
    let (lines, _source) = line_channel(QUEUE_CAP);
    threads.terminal_lines_go_to(&terminal, lines);

    threads.say_in(&drivers, &lead, &terminal, FIRST).await?;
    let root = workspace.path().join(".loadout/conversations");
    let dir = wait_for_conversation(&root, &BTreeSet::new()).await?;
    let _first = wait_for_state(&dir.join("turns/0001.json"), "succeeded").await?;

    let refused = threads.say_in(&drivers, &lead, &terminal, SECOND).await;
    assert!(
        refused.is_err(),
        "the refusing transport accepted a follow-up"
    );
    let conversation = wait_for_state(&dir.join("conversation.json"), "failed").await?;
    let second = wait_for_state(&dir.join("turns/0002.json"), "failed").await?;

    assert_eq!(
        conversation.get("complete").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        conversation.get("attempts").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        conversation.get("turns").and_then(Value::as_u64),
        Some(1),
        "the failed attempt became a delivered turn"
    );
    assert_eq!(
        conversation.get("failureKind").and_then(Value::as_str),
        Some("deliveryFailed")
    );
    assert_eq!(
        conversation.get("deathProof").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        conversation.get("agentTurns").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        conversation.get("inputTokens").and_then(Value::as_u64),
        Some(7)
    );
    assert_eq!(
        conversation.get("outputTokens").and_then(Value::as_u64),
        Some(5)
    );
    assert_eq!(
        conversation.get("cachedTokens").and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(
        second.get("failureKind").and_then(Value::as_str),
        Some("deliveryFailed")
    );
    assert_eq!(second.get("deliveredAt"), Some(&Value::Null));
    assert!(
        !serde_json::to_string(&conversation)?.contains("PRIVATE_FOLLOW_UP_TRANSPORT_ERROR_T34"),
        "a raw transport error escaped into the conversation receipt"
    );
    Ok(())
}

fn evidence_spec(cwd: &Path) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: "PRIVATE_SYMLINK_PROMPT_T34".to_owned(),
        model: None,
        system_append: None,
        reaches_the_web: false,
        policy: Policy::ReadOnly,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

fn empty_manifest() -> SafeInputManifest {
    SafeInputManifest {
        prompt_bytes: "PRIVATE_SYMLINK_PROMPT_T34".len(),
        context: Vec::new(),
        images: Vec::new(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn private_evidence_refuses_preseeded_symlinks_and_unsafe_step_names_before_spawn()
-> Result<(), Box<dyn Error>> {
    for poisoned in ["input", "input-writing", "stdout", "stderr"] {
        let workspace = tempfile::tempdir()?;
        let fixture = tempfile::tempdir()?;
        let binary = executable(fixture.path(), "claude", CLAUDE_FAKE)?;
        let victim = fixture.path().join(format!("victim-{poisoned}"));
        fs::write(&victim, b"VICTIM_UNCHANGED_T34")?;
        let conversation = Uuid::now_v7();
        let root = workspace
            .path()
            .join(".loadout/conversations")
            .join(conversation.to_string());
        fs::create_dir_all(root.join("logs"))?;
        let poisoned_path = match poisoned {
            "input" => root.join("input.json"),
            "input-writing" => root.join("input.json.writing"),
            "stdout" => root.join("logs/lead.jsonl"),
            "stderr" => root.join("logs/lead.stderr.log"),
            _ => return Err("unknown poison case".into()),
        };
        std::os::unix::fs::symlink(&victim, poisoned_path)?;

        let base: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(binary));
        let driver = base
            .with_evidence(EvidenceTarget::lead(
                workspace.path(),
                conversation,
                empty_manifest(),
            ))
            .ok_or("Claude has no evidence seam")?;
        let (events, _inbox) = mpsc::channel(8);
        let started = driver.start(evidence_spec(workspace.path()), events).await;
        assert!(
            started.is_err(),
            "the vendor process started through a preseeded {poisoned} symlink"
        );
        assert!(
            !fixture.path().join("claude.pid.log").exists(),
            "the vendor was spawned before the evidence target was proven safe"
        );
        assert_eq!(fs::read(&victim)?, b"VICTIM_UNCHANGED_T34");
    }

    let workspace = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    let neighbor = tempfile::tempdir()?;
    let binary = executable(fixture.path(), "claude", CLAUDE_FAKE)?;
    fs::create_dir_all(workspace.path().join(".loadout"))?;
    let victim = neighbor.path().join("VICTIM_UNCHANGED_T34");
    fs::write(&victim, b"VICTIM_UNCHANGED_T34")?;
    std::os::unix::fs::symlink(
        neighbor.path(),
        workspace.path().join(".loadout/conversations"),
    )?;
    let conversation = Uuid::now_v7();
    let base: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(binary));
    let driver = base
        .with_evidence(EvidenceTarget::lead(
            workspace.path(),
            conversation,
            empty_manifest(),
        ))
        .ok_or("Claude has no evidence seam")?;
    let (events, _inbox) = mpsc::channel(8);
    assert!(
        driver
            .start(evidence_spec(workspace.path()), events)
            .await
            .is_err(),
        "the vendor process followed a preseeded conversations directory symlink"
    );
    assert!(!fixture.path().join("claude.pid.log").exists());
    assert_eq!(fs::read(&victim)?, b"VICTIM_UNCHANGED_T34");
    assert!(!neighbor.path().join(conversation.to_string()).exists());

    let workspace = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    let binary = executable(fixture.path(), "claude", CLAUDE_FAKE)?;
    fs::create_dir_all(workspace.path().join("logs"))?;
    let base: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(binary));
    let driver = base
        .with_evidence(EvidenceTarget::workflow_step(
            workspace.path().to_path_buf(),
            "../../../PRIVATE_ESCAPE_T34".to_owned(),
            empty_manifest(),
        ))
        .ok_or("Claude has no evidence seam")?;
    let (events, _inbox) = mpsc::channel(8);
    assert!(
        driver
            .start(evidence_spec(workspace.path()), events)
            .await
            .is_err(),
        "an unsafe workflow step name reached the process boundary"
    );
    assert!(!fixture.path().join("claude.pid.log").exists());
    assert!(!workspace.path().join("PRIVATE_ESCAPE_T34").exists());
    Ok(())
}

#[tokio::test]
#[cfg(unix)]
async fn every_new_private_evidence_file_is_owner_only_before_its_first_byte()
-> Result<(), Box<dyn Error>> {
    let workspace = tempfile::tempdir()?;
    let conversation = Uuid::now_v7();
    let target = EvidenceTarget::lead(workspace.path(), conversation, empty_manifest());
    target
        .begin_conversation(ConversationMetadata {
            vendor: ConversationVendor::Claude,
            model_configured: false,
        })
        .await?;
    target.begin_turn(1, &empty_manifest()).await?;
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(target.root().join("turns/0001.json"))?)?
            .get("state")
            .and_then(Value::as_str),
        Some("sending")
    );
    target.accept_turn(1).await?;
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(target.root().join("turns/0001.json"))?)?
            .get("state")
            .and_then(Value::as_str),
        Some("delivered")
    );
    target
        .finish_turn(1, TurnCounters::default(), true, false)
        .await?;
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(target.root().join("turns/0001.json"))?)?
            .get("state")
            .and_then(Value::as_str),
        Some("succeeded")
    );
    target.begin_turn(2, &empty_manifest()).await?;
    target.fail_turn(2, EvidenceFailureKind::Cancelled).await?;
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(target.root().join("turns/0002.json"))?)?
            .get("state")
            .and_then(Value::as_str),
        Some("cancelled")
    );
    let mut streams = target.open().await?;
    streams.stdout.write(b"safe stdout\n").await?;
    streams.stderr.write(b"safe stderr\n").await?;
    streams.stdout.close().await?;
    streams.stderr.close().await?;

    for path in [
        target.input_path(),
        target.root().join("conversation.json"),
        target.root().join("turns/0001.json"),
        target.root().join("turns/0002.json"),
        target.stdout_path(),
        target.stderr_path(),
    ] {
        let mode = fs::metadata(&path)?.permissions().mode() & 0o777;
        assert_eq!(
            mode,
            0o600,
            "{} was created with mode {mode:o}",
            path.display()
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn preexisting_private_files_must_already_be_owner_only_before_any_append_or_reuse()
-> Result<(), Box<dyn Error>> {
    for poisoned in ["input", "stdout", "stderr"] {
        let workspace = tempfile::tempdir()?;
        let fixture = tempfile::tempdir()?;
        let binary = executable(fixture.path(), "claude", CLAUDE_FAKE)?;
        let conversation = Uuid::now_v7();
        let root = workspace
            .path()
            .join(".loadout/conversations")
            .join(conversation.to_string());
        fs::create_dir_all(root.join("logs"))?;
        let (path, original) = match poisoned {
            "input" => (
                root.join("input.json"),
                serde_json::to_vec_pretty(&empty_manifest())?,
            ),
            "stdout" => (
                root.join("logs/lead.jsonl"),
                b"PUBLIC_MODE_STDOUT_T34".to_vec(),
            ),
            "stderr" => (
                root.join("logs/lead.stderr.log"),
                b"PUBLIC_MODE_STDERR_T34".to_vec(),
            ),
            _ => return Err("unknown owner-only fixture".into()),
        };
        fs::write(&path, &original)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644))?;

        let base: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(binary));
        let driver = base
            .with_evidence(EvidenceTarget::lead(
                workspace.path(),
                conversation,
                empty_manifest(),
            ))
            .ok_or("Claude has no evidence seam")?;
        let (events, _inbox) = mpsc::channel(8);
        assert!(
            driver
                .start(evidence_spec(workspace.path()), events)
                .await
                .is_err(),
            "a mode-0644 {poisoned} target was reused for private evidence"
        );
        assert!(!fixture.path().join("claude.pid.log").exists());
        assert_eq!(
            fs::read(&path)?,
            original,
            "{poisoned} changed before refusal"
        );
        assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o644);
    }
    Ok(())
}

#[tokio::test]
async fn failed_terminal_attempt_commit_poison_prevents_a_complete_conversation()
-> Result<(), Box<dyn Error>> {
    let workspace = tempfile::tempdir()?;
    let target = EvidenceTarget::lead(workspace.path(), Uuid::now_v7(), empty_manifest());
    target
        .begin_conversation(ConversationMetadata {
            vendor: ConversationVendor::Codex,
            model_configured: true,
        })
        .await?;
    target.begin_turn(1, &empty_manifest()).await?;
    target.accept_turn(1).await?;

    let turn = target.root().join("turns/0001.json");
    fs::write(turn.with_extension("json.writing"), b"FAULT_GUARD_T34")?;
    assert!(
        target
            .finish_turn(
                1,
                TurnCounters {
                    turns: 2,
                    input_tokens: 11,
                    output_tokens: 7,
                    cached_tokens: 3,
                },
                true,
                false,
            )
            .await
            .is_err(),
        "the injected terminal-receipt fault did not reach the production publisher"
    );
    assert!(
        !target.is_healthy(),
        "a failed terminal commit left the conversation evidence healthy"
    );
    let attempt: Value = serde_json::from_slice(&fs::read(&turn)?)?;
    assert_eq!(
        attempt.get("state").and_then(Value::as_str),
        Some("delivered"),
        "the failed terminal commit exposed a phantom succeeded attempt"
    );
    assert!(
        target.finish_conversation(Some(0), true).await.is_err(),
        "an unhealthy attempt was later promoted to complete conversation evidence"
    );
    let conversation: Value =
        serde_json::from_slice(&fs::read(target.root().join("conversation.json"))?)?;
    assert_eq!(
        conversation.get("complete").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        conversation.get("agentTurns").and_then(Value::as_u64),
        Some(2),
        "the aggregate was not durably current before the terminal commit point"
    );
    Ok(())
}

#[test]
#[cfg(unix)]
fn an_opened_private_parent_cannot_be_swapped_to_a_neighbor_before_publication()
-> Result<(), Box<dyn Error>> {
    const PRIVATE_BYTES: &[u8] = b"PRIVATE_PARENT_SWAP_T34";
    const VICTIM_BYTES: &[u8] = b"VICTIM_UNCHANGED_T34";

    let anchor = tempfile::tempdir()?;
    let neighbor = tempfile::tempdir()?;
    let live = anchor.path().join("evidence/live");
    let parked = anchor.path().join("evidence/parked");
    fs::create_dir_all(&live)?;
    let publisher =
        PrivateFilePublisher::open(anchor.path(), Path::new("evidence/live/receipt.json"))?;

    // This is the exact TOCTOU window that a path-based `metadata -> rename` reopened. The
    // publisher already owns the real directory; changing its former name must not redirect a
    // single private byte into the neighboring workspace.
    fs::rename(&live, &parked)?;
    let victim = neighbor.path().join("receipt.json");
    fs::write(&victim, VICTIM_BYTES)?;
    std::os::unix::fs::symlink(neighbor.path(), &live)?;

    publisher.publish(PRIVATE_BYTES, false)?;
    assert_eq!(fs::read(&victim)?, VICTIM_BYTES);
    assert_eq!(fs::read(parked.join("receipt.json"))?, PRIVATE_BYTES);
    assert_eq!(
        fs::metadata(parked.join("receipt.json"))?
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    Ok(())
}

#[test]
#[cfg(unix)]
fn replacement_publication_stays_on_the_held_parent_after_a_swap() -> Result<(), Box<dyn Error>> {
    const PRIVATE_BYTES: &[u8] = b"PRIVATE_REPLACE_PARENT_SWAP_T34";
    const ORIGINAL_BYTES: &[u8] = b"ORIGINAL_PRIVATE_T34";
    const VICTIM_BYTES: &[u8] = b"VICTIM_REPLACE_UNCHANGED_T34";

    let anchor = tempfile::tempdir()?;
    let neighbor = tempfile::tempdir()?;
    let live = anchor.path().join("evidence/live");
    let parked = anchor.path().join("evidence/parked");
    fs::create_dir_all(&live)?;
    let original = live.join("receipt.json");
    fs::write(&original, ORIGINAL_BYTES)?;
    fs::set_permissions(&original, fs::Permissions::from_mode(0o600))?;
    let publisher =
        PrivateFilePublisher::open(anchor.path(), Path::new("evidence/live/receipt.json"))?;

    fs::rename(&live, &parked)?;
    let victim = neighbor.path().join("receipt.json");
    fs::write(&victim, VICTIM_BYTES)?;
    fs::set_permissions(&victim, fs::Permissions::from_mode(0o600))?;
    std::os::unix::fs::symlink(neighbor.path(), &live)?;

    publisher.publish(PRIVATE_BYTES, true)?;
    assert_eq!(fs::read(&victim)?, VICTIM_BYTES);
    assert_eq!(fs::read(parked.join("receipt.json"))?, PRIVATE_BYTES);
    Ok(())
}

#[test]
#[cfg(unix)]
fn an_intermediate_symlink_in_the_anchor_is_refused() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let neighbor = tempfile::tempdir()?;
    fs::create_dir(neighbor.path().join("run"))?;
    std::os::unix::fs::symlink(neighbor.path(), root.path().join("link"))?;
    let redirected_anchor = root.path().join("link/run");

    assert!(
        PrivateFilePublisher::open(&redirected_anchor, Path::new("receipt.json")).is_err(),
        "an intermediate anchor symlink redirected private evidence"
    );
    assert!(!neighbor.path().join("run/receipt.json").exists());
    Ok(())
}

#[test]
#[cfg(unix)]
fn a_private_leaf_symlink_is_never_opened_for_reading() -> Result<(), Box<dyn Error>> {
    let anchor = tempfile::tempdir()?;
    let target = anchor.path().join("target.json");
    fs::write(&target, b"PRIVATE_LEAF_TARGET_T34")?;
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600))?;
    std::os::unix::fs::symlink(&target, anchor.path().join("link.json"))?;

    assert!(
        open_private_file(
            anchor.path(),
            Path::new("link.json"),
            PrivateFileAccess::Read
        )
        .is_err(),
        "a private read followed its leaf symlink"
    );
    Ok(())
}

#[test]
#[cfg(unix)]
fn a_new_append_target_is_owner_only_before_the_caller_can_write() -> Result<(), Box<dyn Error>> {
    let anchor = tempfile::tempdir()?;
    let relative = Path::new("fresh.log");
    let file = open_private_file(anchor.path(), relative, PrivateFileAccess::CreateAppend)?;

    let metadata = file.metadata()?;
    assert_eq!(
        metadata.len(),
        0,
        "the fresh target already contained bytes"
    );
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    assert_eq!(fs::metadata(anchor.path().join(relative))?.len(), 0);
    Ok(())
}
