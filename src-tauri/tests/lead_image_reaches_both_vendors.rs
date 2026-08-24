//! T-34 AC-5: Lead images use stdin-native transports and never a path or persisted session.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Stdio};
use std::sync::Arc;
use std::time::Duration;

use loadout_lib::commands::Drivers;
use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentHandle, ImageInput, ImageMime, Policy, RunSpec, ValidatedImages,
};
use loadout_lib::engine::supervisor::GroupProof;
use loadout_lib::evidence::{EvidenceTarget, ImageFact, SafeInputManifest};
use loadout_lib::ipc::{
    AppState, PastedImage, QUEUE_CAP, line_channel, say_to_orchestrator_from_window,
};
use loadout_lib::library::agents::{Agent, Vendor};
use loadout_lib::store::Store;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

const LIMIT: Duration = Duration::from_secs(10);
const CHANNEL: usize = 128;
const FIRST_TEXT: &str = "PRIVATE_IMAGE_\"FIRST\"_T34\nBACK\\SLASH";
const SECOND_TEXT: &str = "PRIVATE_IMAGE_SECOND_T34";
const DATA_URL: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUg==";
const APP_SERVER_ECHO: &str = "PRIVATE_APP_SERVER_IMAGE_ECHO_T34";

// Small parser-faithful fixtures: validation owns magic/MIME matching, not full image decoding.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
const JPEG: &[u8] = b"\xff\xd8\xff\xe0\0\x10JFIF";
const GIF: &[u8] = b"GIF89a\x01\0\x01\0";
const WEBP: &[u8] = b"RIFF\x08\0\0\0WEBPVP8 ";

const CLAUDE_FAKE: &str = r#"#!/bin/sh
if [ "${1-}" = "--version" ]; then
  printf '%s\n' '2.1.238 (Claude Code)'
  exit 0
fi
here="$(dirname "$0")"
printf '%s\n' "$@" > "$here/claude.argv.log"
printf '%s\n' "$$" >> "$here/claude.pid.log"
printf '%s\n' '{"type":"system","subtype":"init","session_id":"lead-image","model":"sonnet","tools":[]}'
turn=0
while IFS= read -r line; do
  turn=$((turn + 1))
  printf '%s\n' "$line" >> "$here/claude.stdin.jsonl"
  # Claude may echo the JSON-escaped user envelope. It is input, never raw evidence.
  printf '%s\n' "$line"
  printf '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"CLAUDE_IMAGE_SAFE_%s"}]}}\n' "$turn"
  printf '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"CLAUDE_IMAGE_SAFE_%s"}\n' "$turn"
done
exit 0
"#;

const CODEX_APP_SERVER_FAKE: &str = r#"#!/bin/sh
if [ "${1-}" = "--version" ]; then
  printf '%s\n' 'codex-cli 0.148.0'
  exit 0
fi
here="$(dirname "$0")"
printf '%s\n' "$@" > "$here/codex.argv.log"
printf '%s\n' "$$" >> "$here/codex.pid.log"
case " $* " in
  *" app-server "*) ;;
  *) printf '%s\n' 'Lead images must not use codex exec or --image.' >&2; exit 64 ;;
esac
turn=0
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$here/codex.stdin.jsonl"
  id="$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')"
  case "$line" in
    *'"method":"initialize"'*)
      printf '{"id":%s,"result":{}}\n' "${id:-1}"
      ;;
    *'"method":"config/read"'*)
      printf '{"id":%s,"result":{"config":{"mcp_servers":{}},"origins":{}}}\n' "${id:-2}"
      ;;
    *'"method":"thread/start"'*)
      printf '{"id":%s,"result":{"thread":{"id":"thread-image","ephemeral":true,"path":null}}}\n' "${id:-2}"
      ;;
    *'"method":"turn/start"'*)
      turn=$((turn + 1))
      printf '{"id":%s,"result":{"turn":{"id":"turn-%s","status":"inProgress"}}}\n' "${id:-3}" "$turn"
      printf '{"method":"item/completed","params":{"threadId":"thread-image","turnId":"turn-%s","item":{"type":"agentMessage","text":"CODEX_IMAGE_SAFE_%s"}}}\n' "$turn" "$turn"
      printf '{"method":"codex/event/user_message","params":{"text":"PRIVATE_APP_SERVER_IMAGE_ECHO_T34"}}\n'
      printf '{"method":"turn/completed","params":{"threadId":"thread-image","turn":{"id":"turn-%s","status":"completed"},"usage":{"inputTokens":%s,"outputTokens":%s}}}\n' "$turn" "$turn" "$turn"
      ;;
    *'interrupt"'*)
      printf '{"id":%s,"result":{}}\n' "${id:-9}"
      ;;
  esac
done
exit 0
"#;

fn executable(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn spec(cwd: &Path, prompt: &str) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: prompt.to_owned(),
        model: None,
        system_append: None,
        reaches_the_web: false,
        policy: Policy::ReadOnly,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

fn image(mime: ImageMime, bytes: &[u8]) -> ImageInput {
    ImageInput::new(mime, bytes.to_vec())
}

fn target(workspace: &Path, conversation: Uuid) -> EvidenceTarget {
    EvidenceTarget::lead(
        workspace,
        conversation,
        SafeInputManifest {
            prompt_bytes: FIRST_TEXT.len(),
            context: Vec::new(),
            images: vec![ImageFact {
                mime: "image/png".to_owned(),
                bytes: PNG.len(),
            }],
        },
    )
}

fn private_tree_contains(root: &Path, needle: &[u8]) -> Result<bool, Box<dyn Error>> {
    if !root.exists() {
        return Ok(false);
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if private_tree_contains(&entry.path(), needle)? {
                return Ok(true);
            }
        } else if kind.is_file() {
            let bytes = fs::read(entry.path())?;
            if bytes.windows(needle.len()).any(|window| window == needle) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[test]
fn shared_validation_accepts_four_formats_and_rejects_spoofing_and_limits()
-> Result<(), Box<dyn Error>> {
    let accepted = ValidatedImages::validate(vec![
        image(ImageMime::Png, PNG),
        image(ImageMime::Jpeg, JPEG),
        image(ImageMime::Gif, GIF),
        image(ImageMime::Webp, WEBP),
    ])?;
    assert_eq!(accepted.as_slice().len(), 4);

    assert!(
        ImageInput::from_wire("image/svg+xml", b"<svg>PRIVATE</svg>".to_vec()).is_err(),
        "SVG crossed the closed wire allowlist"
    );
    assert!(
        ValidatedImages::validate(vec![image(ImageMime::Png, JPEG)]).is_err(),
        "declared PNG with JPEG bytes reached a vendor process"
    );
    assert!(
        ValidatedImages::validate(vec![image(ImageMime::Gif, GIF); 5]).is_err(),
        "five images passed the four-image limit"
    );
    assert!(
        ValidatedImages::validate(vec![image(
            ImageMime::Png,
            &vec![0_u8; 5 * 1024 * 1024 + 1]
        )])
        .is_err(),
        "one image larger than 5 MiB was accepted"
    );
    let each = vec![0_u8; 4 * 1024 * 1024];
    assert!(
        ValidatedImages::validate(vec![
            image(ImageMime::Png, &each),
            image(ImageMime::Png, &each),
            image(ImageMime::Png, &each),
            image(ImageMime::Png, PNG),
        ])
        .is_err(),
        "more than 12 MiB in one message passed the total limit"
    );

    let mut redacted = spec(Path::new("/PRIVATE_CWD_T34"), "PRIVATE_DEBUG_PROMPT_T34");
    redacted.model = Some("PRIVATE_DEBUG_MODEL_T34".to_owned());
    redacted.system_append = Some("PRIVATE_DEBUG_SYSTEM_T34".to_owned());
    let debug = format!("{redacted:?}");
    for private in [
        "PRIVATE_CWD_T34",
        "PRIVATE_DEBUG_PROMPT_T34",
        "PRIVATE_DEBUG_MODEL_T34",
        "PRIVATE_DEBUG_SYSTEM_T34",
    ] {
        assert!(
            !debug.contains(private),
            "RunSpec Debug leaked {private}: {debug}"
        );
    }
    Ok(())
}

fn assert_codex_calls(calls: &[Value], workspace: &Path) -> Result<(), Box<dyn Error>> {
    let thread = calls
        .iter()
        .find(|call| call.get("method").and_then(Value::as_str) == Some("thread/start"))
        .ok_or("Codex received no thread/start")?;
    assert_eq!(
        thread.pointer("/params/ephemeral").and_then(Value::as_bool),
        Some(true)
    );
    assert!(
        thread.pointer("/params/cwd").is_none(),
        "thread/start must omit the cwd key entirely"
    );
    assert!(
        !thread
            .to_string()
            .contains(workspace.to_string_lossy().as_ref()),
        "cwd crossed JSON-RPC instead of command.current_dir"
    );
    let turns = calls
        .iter()
        .filter(|call| call.get("method").and_then(Value::as_str) == Some("turn/start"))
        .collect::<Vec<_>>();
    assert_eq!(turns.len(), 2);
    for (turn, text) in turns.iter().zip([FIRST_TEXT, SECOND_TEXT]) {
        assert_eq!(
            turn.pointer("/params/threadId").and_then(Value::as_str),
            Some("thread-image")
        );
        let input = turn
            .pointer("/params/input")
            .and_then(Value::as_array)
            .ok_or("turn/start has no native input array")?;
        assert!(input.iter().any(|item| {
            item.get("type").and_then(Value::as_str) == Some("text")
                && item.get("text").and_then(Value::as_str) == Some(text)
        }));
        assert!(input.iter().any(|item| {
            item.get("type").and_then(Value::as_str) == Some("image")
                && item.get("url").and_then(Value::as_str) == Some(DATA_URL)
        }));
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn claude_receives_first_and_follow_up_images_only_as_native_stdin_blocks()
-> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let binary = executable(fixture.path(), "claude", CLAUDE_FAKE)?;
    let conversation = Uuid::now_v7();
    let base: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(binary));
    let driver = base
        .with_evidence(target(workspace.path(), conversation))
        .ok_or("Claude has no production evidence seam")?;
    let first_images = ValidatedImages::validate(vec![image(ImageMime::Png, PNG)])?;
    let second_images = ValidatedImages::validate(vec![image(ImageMime::Png, PNG)])?;
    let (tx, _events) = mpsc::channel(CHANNEL);

    let mut handle: Box<dyn AgentHandle> = timeout(
        LIMIT,
        driver.start_conversation(spec(workspace.path(), FIRST_TEXT), first_images, tx),
    )
    .await??;
    let _first = timeout(LIMIT, handle.wait()).await??;
    handle
        .send_with_images(SECOND_TEXT.to_owned(), second_images)
        .await?;
    let _second = timeout(LIMIT, handle.wait()).await??;
    let _closed = timeout(LIMIT, handle.close()).await??;

    let stdin = fs::read_to_string(fixture.path().join("claude.stdin.jsonl"))?;
    let turns = stdin
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(turns.len(), 2);
    for (turn, text) in turns.iter().zip([FIRST_TEXT, SECOND_TEXT]) {
        let content = turn
            .pointer("/message/content")
            .and_then(Value::as_array)
            .ok_or("Claude did not receive native content blocks")?;
        assert!(content.iter().any(|block| {
            block.get("type").and_then(Value::as_str) == Some("text")
                && block.get("text").and_then(Value::as_str) == Some(text)
        }));
        assert!(content.iter().any(|block| {
            block.get("type").and_then(Value::as_str) == Some("image")
                && block.pointer("/source/media_type").and_then(Value::as_str) == Some("image/png")
                && block.pointer("/source/data").and_then(Value::as_str)
                    == Some("iVBORw0KGgoAAAANSUhEUg==")
        }));
    }
    let argv = fs::read_to_string(fixture.path().join("claude.argv.log"))?;
    assert!(!argv.contains(FIRST_TEXT) && !argv.contains(DATA_URL));
    assert_eq!(
        fs::read_to_string(fixture.path().join("claude.pid.log"))?
            .lines()
            .count(),
        1
    );
    let escaped_first = serde_json::to_string(FIRST_TEXT)?;
    for private in [
        FIRST_TEXT.as_bytes(),
        SECOND_TEXT.as_bytes(),
        DATA_URL.as_bytes(),
        escaped_first.as_bytes(),
    ] {
        assert!(!private_tree_contains(
            &workspace.path().join(".loadout"),
            private
        )?);
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn codex_lead_uses_one_ephemeral_stdio_app_server_for_both_image_turns()
-> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let binary = executable(fixture.path(), "codex", CODEX_APP_SERVER_FAKE)?;
    let conversation = Uuid::now_v7();
    let base: Arc<dyn AgentDriver> = Arc::new(CodexDriver::with_binary(binary));
    let driver = base
        .with_evidence(target(workspace.path(), conversation))
        .ok_or("Codex has no production evidence seam")?;
    let first_images = ValidatedImages::validate(vec![image(ImageMime::Png, PNG)])?;
    let second_images = ValidatedImages::validate(vec![image(ImageMime::Png, PNG)])?;
    let (tx, _events) = mpsc::channel(CHANNEL);

    let mut handle: Box<dyn AgentHandle> = timeout(
        LIMIT,
        driver.start_conversation(spec(workspace.path(), FIRST_TEXT), first_images, tx),
    )
    .await??;
    let first = timeout(LIMIT, handle.wait()).await??;
    handle
        .send_with_images(SECOND_TEXT.to_owned(), second_images)
        .await?;
    let second = timeout(LIMIT, handle.wait()).await??;
    assert_eq!(
        first.session.id, second.session.id,
        "the second image minted another thread"
    );
    let proof = timeout(LIMIT, handle.cancel()).await?;
    assert!(
        matches!(proof, GroupProof::Dead { .. }),
        "Stop returned {proof:?}"
    );

    let argv = fs::read_to_string(fixture.path().join("codex.argv.log"))?;
    assert_eq!(
        argv.lines().collect::<Vec<_>>(),
        vec!["app-server", "--listen", "stdio://"],
        "Codex Lead used exec/--image or exposed another argument: {argv:?}"
    );
    assert_eq!(
        fs::read_to_string(fixture.path().join("codex.pid.log"))?
            .lines()
            .count(),
        1
    );
    let stdin = fs::read_to_string(fixture.path().join("codex.stdin.jsonl"))?;
    let calls = stdin
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<Result<Vec<_>, _>>()?;
    assert_codex_calls(&calls, workspace.path())?;
    for private in [
        FIRST_TEXT.as_bytes(),
        SECOND_TEXT.as_bytes(),
        DATA_URL.as_bytes(),
        APP_SERVER_ECHO.as_bytes(),
    ] {
        assert!(
            !private_tree_contains(&workspace.path().join(".loadout"), private)?,
            "Codex prompt/image echo persisted under .loadout"
        );
    }
    assert!(!workspace.path().join(".codex").exists());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn window_wire_reaches_app_state_and_the_same_thread_on_first_and_follow_up_turns()
-> Result<(), Box<dyn Error>> {
    let library = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    fs::create_dir_all(workspace.path().join(".loadout"))?;
    let binary = executable(fixture.path(), "claude", CLAUDE_FAKE)?;
    let concrete: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(binary));
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&concrete));
    let store = Store::open(&workspace.path().join(".loadout/loadout.db"))?;
    let state = AppState::new(
        library.path().to_path_buf(),
        workspace.path().to_path_buf(),
        store,
        drivers,
    );
    let mut lead = Agent::example();
    lead.id = Uuid::now_v7();
    lead.name = "Image Lead".to_owned();
    lead.runs_with = Vendor::ClaudeCode;
    save_agent_inner(library.path(), &lead)?;
    let folder = workspace.path().to_string_lossy().into_owned();
    let (lines, _source) = line_channel(QUEUE_CAP);
    state.watching_the_lead("product-image", Some(&folder), lines)?;
    let wire_image = || PastedImage {
        mime: "image/png".to_owned(),
        base64: "iVBORw0KGgoAAAANSUhEUg==".to_owned(),
    };

    say_to_orchestrator_from_window(
        &state,
        "product-image",
        Some(&folder),
        Some(&lead.id.to_string()),
        FIRST_TEXT,
        vec![wire_image()],
    )
    .await?;
    say_to_orchestrator_from_window(
        &state,
        "product-image",
        Some(&folder),
        Some(&lead.id.to_string()),
        SECOND_TEXT,
        vec![wire_image()],
    )
    .await?;
    say_to_orchestrator_from_window(
        &state,
        "product-image",
        Some(&folder),
        Some(&lead.id.to_string()),
        "PRIVATE_PLAIN_THIRD_T34",
        Vec::new(),
    )
    .await?;

    let stdin_path = fixture.path().join("claude.stdin.jsonl");
    let stdin = timeout(LIMIT, async {
        loop {
            if let Ok(stdin) = fs::read_to_string(&stdin_path)
                && stdin.lines().count() >= 3
            {
                break stdin;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    let turns = stdin
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<Result<Vec<_>, _>>()?;
    assert_product_turns(&turns);
    assert_eq!(
        fs::read_to_string(fixture.path().join("claude.pid.log"))?
            .lines()
            .count(),
        1,
        "the product IPC edge minted another Lead process for the follow-up image"
    );
    state.close_the_lead("product-image").await;
    let escaped_first = serde_json::to_string(FIRST_TEXT)?;
    for private in [
        FIRST_TEXT.as_bytes(),
        SECOND_TEXT.as_bytes(),
        b"PRIVATE_PLAIN_THIRD_T34".as_slice(),
        DATA_URL.as_bytes(),
        escaped_first.as_bytes(),
    ] {
        assert!(
            !private_tree_contains(&workspace.path().join(".loadout"), private)?,
            "the production IPC/Threads edge persisted a private image turn echo"
        );
    }
    Ok(())
}

fn assert_product_turns(turns: &[Value]) {
    assert_eq!(turns.len(), 3);
    assert_eq!(
        turns[0]
            .pointer("/message/content/0/text")
            .and_then(Value::as_str),
        Some(FIRST_TEXT)
    );
    assert_eq!(
        turns[1]
            .pointer("/message/content/0/text")
            .and_then(Value::as_str),
        Some(SECOND_TEXT)
    );
    assert_eq!(
        turns[2]
            .pointer("/message/content/0/text")
            .and_then(Value::as_str),
        Some("PRIVATE_PLAIN_THIRD_T34"),
        "the single FIFO reordered a plain turn behind an image turn"
    );
    for turn in &turns[..2] {
        assert_eq!(
            turn.pointer("/message/content/1/source/data")
                .and_then(Value::as_str),
            Some("iVBORw0KGgoAAAANSUhEUg==")
        );
    }
}

async fn assert_invalid_thread_is_refused_and_dead(
    replacement: &str,
) -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let fake = CODEX_APP_SERVER_FAKE.replace("\"ephemeral\":true,\"path\":null", replacement);
    assert_ne!(
        fake, CODEX_APP_SERVER_FAKE,
        "the negative fixture did not alter thread/start"
    );
    let binary = executable(fixture.path(), "codex-invalid-thread", &fake)?;
    let driver: Arc<dyn AgentDriver> = Arc::new(CodexDriver::with_binary(binary));
    let (tx, _events) = mpsc::channel(CHANNEL);

    let started = timeout(
        LIMIT,
        driver.start_conversation(
            spec(workspace.path(), FIRST_TEXT),
            ValidatedImages::default(),
            tx,
        ),
    )
    .await?;
    let error = match started {
        Err(error) => error,
        Ok(mut handle) => {
            let proof = timeout(LIMIT, handle.cancel()).await?;
            return Err(format!(
                "Codex accepted non-ephemeral thread metadata; cleanup returned {proof:?}"
            )
            .into());
        }
    };
    assert!(
        error.to_string().contains("ephemeral thread"),
        "the refusal hid its fixed reason: {error}"
    );

    let calls = fs::read_to_string(fixture.path().join("codex.stdin.jsonl"))?
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<Result<Vec<_>, _>>()?;
    let thread = calls
        .iter()
        .find(|call| call.get("method").and_then(Value::as_str) == Some("thread/start"))
        .ok_or("the refusing fixture received no thread/start")?;
    assert!(
        thread.pointer("/params/cwd").is_none(),
        "the rejected thread/start still carried a cwd key"
    );

    let pid = fs::read_to_string(fixture.path().join("codex.pid.log"))?
        .trim()
        .parse::<u32>()?;
    let still_alive = StdCommand::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?
        .success();
    assert!(
        !still_alive,
        "thread/start was refused but App Server pid {pid} was left alive"
    );
    assert!(!workspace.path().join(".codex").exists());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn codex_refuses_persisted_thread_metadata_and_proves_the_server_dead()
-> Result<(), Box<dyn Error>> {
    assert_invalid_thread_is_refused_and_dead("\"ephemeral\":false,\"path\":null").await?;
    assert_invalid_thread_is_refused_and_dead("\"ephemeral\":true,\"path\":\"persisted.jsonl\"")
        .await
}
