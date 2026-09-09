//! CT-04: gotowa wersja zmienia wskaźnik dopiero po kompletnej publikacji.

// 2026-09-08 (CT-04) — te testy są liniowymi historiami plików i prawdziwych grup procesów;
// rozcinanie ich tylko dla limitu linii ukryłoby kolejność, którą mają dowodzić.
#![allow(clippy::too_many_lines)]

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use loadout_lib::commands::Drivers;
use loadout_lib::commands::context::{create_context_set_inner, save_context_draft_inner};
use loadout_lib::commands::context_build::{
    BuildContextRequest, build_context_inner, read_context_build_inner,
};
use loadout_lib::context::build;
use loadout_lib::context::files::{folder_of, library_root};
use loadout_lib::context::findings;
use loadout_lib::context::{
    BuildEnd, ContextDraft, ContextFinding, ContextRevision, ContextSource, ContextTopic,
    FindingKind, Origin, Preparation, RevisionEdit, SourceKind, SourceReference, StoredFile,
};
use loadout_lib::durable_file::{
    FaultAction, FaultInjector, FaultPoint, PublicationEvent, scoped_faults,
};
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::{AgentDriver, DriverConfiguration};
use loadout_lib::engine::limits::{Limiter, Weight};
use loadout_lib::library::agents::Vendor;
use tokio_util::sync::CancellationToken;

const ANSWER: &str = r#"{"topics":[{"id":"checkout","title":"Checkout"}],"findings":[{"kind":"requirement","text":"Keep the total visible.","condition":"while editing the cart","sources":[{"sourceId":"typed","part":"fragment 1"}],"topic":"checkout","conflictsWith":[]}],"questions":[]}"#;

fn missing_drivers(dir: &Path) -> Drivers {
    let claude: Arc<dyn AgentDriver> =
        Arc::new(ClaudeDriver::with_binary(dir.join("missing-claude")));
    let codex: Arc<dyn AgentDriver> = Arc::new(CodexDriver::with_binary(dir.join("missing-codex")));
    Arc::new(move |vendor| match vendor {
        Vendor::ClaudeCode => Arc::clone(&claude),
        Vendor::Codex => Arc::clone(&codex),
    })
}

fn executable(dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join("claude");
    let body = format!(
        "#!/bin/sh\nif [ \"${{1-}}\" = \"--version\" ]; then\n  printf '%s\\n' '2.1.263 (Claude Code)'\n  exit 0\nfi\nIFS= read -r line\nprintf '%s\\n' '{{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"context-build\",\"model\":\"sonnet\",\"tools\":[]}}'\nprintf '%s\\n' '{{\"type\":\"assistant\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\"text\":{ANSWER:?}}}]}}}}'\nprintf '%s\\n' '{{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":1,\"result\":{ANSWER:?}}}'\n"
    );
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn transport_executable(dir: &Path, vendor: Vendor) -> Result<PathBuf, Box<dyn Error>> {
    let (name, body) = match vendor {
        Vendor::ClaudeCode => (
            "claude",
            r#"#!/bin/sh
if [ "${1-}" = "--version" ]; then printf '%s\n' '2.1.263 (Claude Code)'; exit 0; fi
printf '%s\n' "$@" > "$TMPDIR/context.argv"
printf '%s\n' "$$" > "$TMPDIR/context.pid"
IFS= read -r line
printf '%s\n' "$line" > "$TMPDIR/context.stdin"
printf '%s\n' '{"type":"system","subtype":"init","session_id":"fresh-context","model":"sonnet","tools":[]}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":__ANSWER__}'
"#
            .replace("__ANSWER__", &format!("{ANSWER:?}")),
        ),
        Vendor::Codex => (
            "codex",
            r#"#!/bin/sh
if [ "${1-}" = "--version" ]; then printf '%s\n' 'codex-cli 0.153.0'; exit 0; fi
printf '%s\n' "$@" > "$TMPDIR/context.argv"
printf '%s\n' "$$" > "$TMPDIR/context.pid"
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$TMPDIR/context.stdin"
  id="$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')"
  case "$line" in
    *'"method":"initialize"'*) printf '{"id":%s,"result":{}}\n' "${id:-1}" ;;
    *'"method":"config/read"'*) printf '{"id":%s,"result":{"config":{"mcp_servers":{}},"origins":{}}}\n' "${id:-2}" ;;
    *'"method":"thread/start"'*) printf '{"id":%s,"result":{"thread":{"id":"fresh-context","ephemeral":true,"path":null}}}\n' "${id:-3}" ;;
    *'"method":"turn/start"'*) printf '{"id":%s,"result":{"turn":{"id":"turn-1","status":"inProgress"}}}\n' "${id:-4}"; printf '%s\n' '{"method":"item/completed","params":{"threadId":"fresh-context","turnId":"turn-1","item":{"type":"agentMessage","text":__ANSWER__}}}' '{"method":"turn/completed","params":{"threadId":"fresh-context","turn":{"id":"turn-1","status":"completed"},"usage":{"inputTokens":1,"outputTokens":1}}}' ;;
  esac
done
"#
            .replace("__ANSWER__", &format!("{ANSWER:?}")),
        ),
    };
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn failing_probe(dir: &Path, name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, "#!/bin/sh\nexit 17\n")?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn correcting_executable(dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join("claude-correcting");
    let body = format!(
        r#"#!/bin/sh
if [ "${{1-}}" = "--version" ]; then printf '%s\n' '2.1.263 (Claude Code)'; exit 0; fi
IFS= read -r line
printf '%s\n' '{{"type":"system","subtype":"init","session_id":"fresh-correction","model":"sonnet","tools":[]}}'
case "$line" in
  *'An earlier answer to this request was refused'*)
    printf '%s\n' '{{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":{ANSWER:?}}}'
    ;;
  *)
    printf '%s\n' '{{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"not json"}}'
    ;;
esac
"#
    );
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn bad_json_executable(dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join("claude-bad-json");
    fs::write(
        &path,
        r#"#!/bin/sh
if [ "${1-}" = "--version" ]; then printf '%s\n' '2.1.263 (Claude Code)'; exit 0; fi
IFS= read -r line
printf '%s\n' '{"type":"system","subtype":"init","session_id":"fresh-bad-json","model":"sonnet","tools":[]}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"still not json"}'
"#,
    )?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn wrong_reference_executable(dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join("claude-wrong-reference");
    let answer = ANSWER.replace("fragment 1", "fragment 2");
    let body = format!(
        "#!/bin/sh\nif [ \"${{1-}}\" = \"--version\" ]; then printf '%s\\n' '2.1.263 (Claude Code)'; exit 0; fi\nIFS= read -r line\nprintf '%s\\n' '{{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"wrong-reference\",\"model\":\"sonnet\",\"tools\":[]}}'\nprintf '%s\\n' '{{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":1,\"result\":{answer:?}}}'\n"
    );
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn not_logged_in_executable(dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join("claude-not-logged-in");
    fs::write(
        &path,
        r#"#!/bin/sh
if [ "${1-}" = "--version" ]; then printf '%s\n' '2.1.263 (Claude Code)'; exit 0; fi
IFS= read -r line
printf '%s\n' '{"type":"system","subtype":"init","session_id":"not-logged-in","model":"sonnet","tools":[]}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":true,"terminal_reason":"api_error","num_turns":1,"result":"Not logged in"}'
"#,
    )?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn blocking_executable(dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join("claude-blocking");
    fs::write(
        &path,
        r#"#!/bin/sh
if [ "${1-}" = "--version" ]; then printf '%s\n' '2.1.263 (Claude Code)'; exit 0; fi
trap '' TERM
printf '%s\n' "$$" > "$TMPDIR/context.pid"
IFS= read -r line
while :; do /bin/sleep 0.05; done
"#,
    )?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn spending_codex_executable(dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join("codex-spending");
    fs::write(
        &path,
        r#"#!/bin/sh
if [ "${1-}" = "--version" ]; then printf '%s\n' 'codex-cli 0.153.0'; exit 0; fi
while IFS= read -r line; do
  id="$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')"
  case "$line" in
    *'"method":"initialize"'*) printf '{"id":%s,"result":{}}\n' "${id:-1}" ;;
    *'"method":"config/read"'*) printf '{"id":%s,"result":{"config":{"mcp_servers":{}},"origins":{}}}\n' "${id:-2}" ;;
    *'"method":"thread/start"'*) printf '{"id":%s,"result":{"thread":{"id":"spending-context","ephemeral":true,"path":null},"model":"gpt-5.6-sol"}}\n' "${id:-3}" ;;
    *'"method":"turn/start"'*)
      printf '{"id":%s,"result":{"turn":{"id":"turn-spending","status":"inProgress"}}}\n' "${id:-4}"
      printf '%s\n' '{"method":"thread/tokenUsage/updated","params":{"usage":{"inputTokens":1000000,"cachedInputTokens":0,"outputTokens":0}}}'
      ;;
    *'"method":"turn/interrupt"'*) printf '{"id":%s,"result":{}}\n' "${id:-5}" ;;
  esac
done
"#,
    )?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn group_is_alive(pgid: i32) -> bool {
    use nix::sys::signal::killpg;
    use nix::unistd::Pid;

    killpg(Pid::from_raw(pgid), None).is_ok()
}

fn group_is_dead(pgid: i32) -> bool {
    use nix::errno::Errno;
    use nix::sys::signal::killpg;
    use nix::unistd::Pid;

    killpg(Pid::from_raw(pgid), None) == Err(Errno::ESRCH)
}

fn find_named(root: &Path, name: &str) -> Result<Option<PathBuf>, Box<dyn Error>> {
    if !root.exists() {
        return Ok(None);
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(found) = find_named(&entry.path(), name)? {
                return Ok(Some(found));
            }
        } else if entry.file_name() == name {
            return Ok(Some(entry.path()));
        }
    }
    Ok(None)
}

#[derive(Debug)]
struct StopsNewRevisionManifest;

impl FaultInjector for StopsNewRevisionManifest {
    fn action(&self, event: &PublicationEvent) -> FaultAction {
        let is_revision_manifest = event
            .target
            .parent()
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .is_some_and(|name| name == "versions")
            && event
                .target
                .file_name()
                .is_some_and(|name| name == "manifest.json");
        if is_revision_manifest && event.point == FaultPoint::AfterPartialWrite {
            FaultAction::Fail
        } else {
            FaultAction::Continue
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_half_published_version_does_not_take_the_ready_one() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Checkout")?;
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![ContextSource {
                id: "typed".to_owned(),
                kind: SourceKind::Text,
                name: "Notes".to_owned(),
                text: "The total stays visible while the cart changes.".to_owned(),
                ..ContextSource::default()
            }],
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;
    let library = library_root(home.path());
    let folder = folder_of(&library, &saved.set.id)?;
    let old = folder.join("versions/old-ready");
    fs::create_dir_all(old.join("topics"))?;
    fs::set_permissions(&old, fs::Permissions::from_mode(0o700))?;
    fs::set_permissions(old.join("topics"), fs::Permissions::from_mode(0o700))?;
    let old_finding = ContextFinding {
        id: "old-finding".to_owned(),
        kind: FindingKind::Fact,
        text: "The earlier version stays unchanged.".to_owned(),
        sources: vec![SourceReference {
            source_id: "typed".to_owned(),
            part: "fragment 1".to_owned(),
        }],
        topic: "old".to_owned(),
        origin: Origin::Generated,
        ..ContextFinding::default()
    };
    let mut old_findings = serde_json::to_vec_pretty(&vec![old_finding.clone()])?;
    old_findings.push(b'\n');
    fs::write(old.join("findings.json"), &old_findings)?;
    fs::write(old.join("index.md"), "# Old ready version\n")?;
    fs::write(old.join("topics/old.md"), "# Old\n")?;
    let old_revision = ContextRevision {
        schema: 1,
        id: "old-ready".to_owned(),
        set_id: saved.set.id.clone(),
        draft_revision: saved.set.draft_revision,
        app: "claude-code".to_owned(),
        origin: Origin::Generated,
        topics: vec![ContextTopic {
            id: "old".to_owned(),
            title: "Old".to_owned(),
        }],
        findings: vec![old_finding],
        index_file: "index.md".to_owned(),
        findings_file: "findings.json".to_owned(),
        topic_files: vec!["topics/old.md".to_owned()],
        ..ContextRevision::default()
    };
    fs::write(
        old.join("manifest.json"),
        serde_json::to_vec_pretty(&old_revision)?,
    )?;
    let mut set = saved.set;
    set.latest_ready_revision = Some("old-ready".to_owned());
    fs::write(
        folder.join("manifest.json"),
        serde_json::to_vec_pretty(&set)?,
    )?;

    let concrete: Arc<dyn AgentDriver> =
        Arc::new(ClaudeDriver::with_binary(executable(fixture.path())?));
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&concrete));
    let faults = scoped_faults(&folder, Arc::new(StopsNewRevisionManifest))?;
    let built = build_context_inner(
        home.path(),
        project.path(),
        &drivers,
        &Limiter::new(1),
        &BuildContextRequest {
            set_id: set.id.clone(),
            operation_id: "01990000-0000-7000-8000-00000000c704".to_owned(),
            app: Some(Vendor::ClaudeCode),
            model: None,
            generation: 1,
            deadline: Duration::from_secs(20),
            budget_usd: None,
        },
        &CancellationToken::new(),
    )
    .await?;
    drop(faults);

    assert_eq!(
        built.build.as_ref().map(|build| build.end),
        Some(BuildEnd::Failed)
    );
    let after = loadout_lib::context::files::read_set(&library, &set.id)?;
    assert_eq!(
        after.set.latest_ready_revision.as_deref(),
        Some("old-ready")
    );
    assert_eq!(fs::read(old.join("findings.json"))?, old_findings);
    assert!(
        built.build.as_ref().is_some_and(|build| {
            build.said.contains("could not be published completely")
                && build.said.contains("Try Rebuild context")
        }),
        "the saved build state did not tell the person what to fix: {:?}",
        built.build
    );
    Ok(())
}

#[test]
fn conditions_are_part_of_finding_identity_and_exact_duplicates_are_not()
-> Result<(), Box<dyn Error>> {
    let allowed = BTreeSet::from([SourceReference {
        source_id: "typed".to_owned(),
        part: "fragment 1".to_owned(),
    }]);
    let answer = r#"{
      "topics":[{"id":"checkout","title":"Checkout"}],
      "findings":[
        {"kind":"requirement","text":"Keep the total visible.","condition":"while editing","sources":[{"sourceId":"typed","part":"fragment 1"}],"topic":"checkout","conflictsWith":["other"]},
        {"kind":"requirement","text":"Keep the total visible.","condition":"after payment","sources":[{"sourceId":"typed","part":"fragment 1"}],"topic":"checkout","conflictsWith":["first"]},
        {"kind":"requirement","text":"Keep the total visible.","condition":"while editing","sources":[{"sourceId":"typed","part":"fragment 1"}],"topic":"checkout","conflictsWith":["third"]}
      ],
      "questions":[]
    }"#;
    let found = findings::read_findings(answer.as_bytes(), &allowed)?;
    assert_eq!(found.findings.len(), 2);
    assert_eq!(found.findings[0].text, found.findings[1].text);
    assert_ne!(found.findings[0].condition, found.findings[1].condition);
    assert_ne!(found.findings[0].id, found.findings[1].id);
    assert_eq!(found.findings[0].conflicts_with, vec!["other", "third"]);
    Ok(())
}

#[test]
fn the_short_index_lists_each_open_question_once() {
    let found = findings::Findings {
        questions: vec!["Which total wins?".to_owned()],
        ..findings::Findings::default()
    };
    let rendered = findings::render_index("Checkout", &found);
    assert_eq!(rendered.matches("Which total wins?").count(), 1);
}

#[tokio::test]
async fn a_saved_running_build_is_interrupted_after_restart() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Restart")?;
    let request = BuildContextRequest {
        set_id: made.set.id.clone(),
        operation_id: "restart-build".to_owned(),
        app: Some(Vendor::ClaudeCode),
        model: None,
        generation: 4,
        deadline: Duration::from_secs(20),
        budget_usd: None,
    };
    let library = library_root(home.path());
    let state = build::initial_state(
        &request,
        "claude-code",
        made.set.draft_revision,
        "fingerprint".to_owned(),
        Vec::new(),
        "2026-09-08T10:00:00Z",
    );
    build::write_state(&library, &state)?;

    let read = read_context_build_inner(
        home.path(),
        &missing_drivers(fixture.path()),
        &made.set.id,
        None,
    )
    .await?;
    assert_eq!(
        read.build.as_ref().map(|build| build.end),
        Some(BuildEnd::Interrupted)
    );
    assert!(
        read.build
            .is_some_and(|build| build.said.contains("Rebuild context"))
    );
    Ok(())
}

#[tokio::test]
async fn a_build_without_death_proof_is_interrupted_after_restart() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Restart without proof")?;
    let request = BuildContextRequest {
        set_id: made.set.id.clone(),
        operation_id: "restart-alive-build".to_owned(),
        app: Some(Vendor::ClaudeCode),
        model: None,
        generation: 4,
        deadline: Duration::from_secs(20),
        budget_usd: None,
    };
    let library = library_root(home.path());
    let mut state = build::initial_state(
        &request,
        "claude-code",
        made.set.draft_revision,
        "fingerprint".to_owned(),
        Vec::new(),
        "2026-09-08T10:00:00Z",
    );
    state.end = BuildEnd::StillRunning;
    state.said = "The agent may still be running.".to_owned();
    build::write_state(&library, &state)?;

    let read = read_context_build_inner(
        home.path(),
        &missing_drivers(fixture.path()),
        &made.set.id,
        None,
    )
    .await?;
    assert_eq!(
        read.build.as_ref().map(|build| build.end),
        Some(BuildEnd::Interrupted)
    );
    Ok(())
}

#[test]
fn a_newer_generation_wins_when_two_builds_start_in_the_same_second() -> Result<(), Box<dyn Error>>
{
    let home = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Same second")?;
    let library = library_root(home.path());
    for (operation_id, generation) in [("z-older", 1), ("a-newer", 2)] {
        let request = BuildContextRequest {
            set_id: made.set.id.clone(),
            operation_id: operation_id.to_owned(),
            app: Some(Vendor::ClaudeCode),
            model: None,
            generation,
            deadline: Duration::from_secs(20),
            budget_usd: None,
        };
        let state = build::initial_state(
            &request,
            "claude-code",
            made.set.draft_revision,
            String::new(),
            Vec::new(),
            "2026-09-08T10:00:00Z",
        );
        build::write_state(&library, &state)?;
    }
    let latest =
        build::read_build(&library, &made.set.id, None)?.ok_or("the saved build was not found")?;
    assert_eq!(latest.operation_id, "a-newer");
    assert_eq!(latest.generation, 2);
    Ok(())
}

#[tokio::test]
async fn a_finished_choice_falls_back_to_the_only_app_still_available() -> Result<(), Box<dyn Error>>
{
    let home = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Available app")?;
    let request = BuildContextRequest {
        set_id: made.set.id.clone(),
        operation_id: "old-codex-choice".to_owned(),
        app: Some(Vendor::Codex),
        model: None,
        generation: 1,
        deadline: Duration::from_secs(20),
        budget_usd: None,
    };
    let library = library_root(home.path());
    let mut state = build::initial_state(
        &request,
        "codex",
        made.set.draft_revision,
        String::new(),
        Vec::new(),
        "2026-09-08T10:00:00Z",
    );
    state.end = BuildEnd::Failed;
    build::write_state(&library, &state)?;
    let claude: Arc<dyn AgentDriver> =
        Arc::new(ClaudeDriver::with_binary(executable(fixture.path())?));
    let codex: Arc<dyn AgentDriver> = Arc::new(CodexDriver::with_binary(
        fixture.path().join("missing-codex"),
    ));
    let drivers: Drivers = Arc::new(move |vendor| match vendor {
        Vendor::ClaudeCode => Arc::clone(&claude),
        Vendor::Codex => Arc::clone(&codex),
    });
    let read = read_context_build_inner(home.path(), &drivers, &made.set.id, None).await?;
    assert_eq!(read.build_with, "claude-code");
    Ok(())
}

#[tokio::test]
async fn no_agent_app_is_a_named_failed_result_for_every_source() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "No app")?;
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![ContextSource {
                id: "typed".to_owned(),
                kind: SourceKind::Text,
                name: "Notes".to_owned(),
                text: "One exact requirement.".to_owned(),
                ..ContextSource::default()
            }],
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;
    let built = build_context_inner(
        home.path(),
        project.path(),
        &missing_drivers(fixture.path()),
        &Limiter::new(1),
        &BuildContextRequest {
            set_id: saved.set.id,
            operation_id: "no-app-build".to_owned(),
            app: None,
            model: None,
            generation: 1,
            deadline: Duration::from_secs(2),
            budget_usd: None,
        },
        &CancellationToken::new(),
    )
    .await?;
    let state = built.build.ok_or("missing build state")?;
    assert_eq!(state.end, BuildEnd::Failed);
    assert!(
        state
            .said
            .contains("Neither Claude Code nor Codex is available")
    );
    assert_eq!(state.sources.len(), 1);
    assert_eq!(state.sources[0].part, "fragment 1");
    assert!(
        state
            .sources
            .iter()
            .all(|source| { source.outcome == loadout_lib::context::SourceOutcome::Failed })
    );
    Ok(())
}

#[tokio::test]
async fn a_failed_app_check_is_not_reported_as_a_missing_installation() -> Result<(), Box<dyn Error>>
{
    let home = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Could not check")?;
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![ContextSource {
                id: "typed".to_owned(),
                kind: SourceKind::Text,
                name: "Notes".to_owned(),
                text: "One exact requirement.".to_owned(),
                ..ContextSource::default()
            }],
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;
    let claude: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(failing_probe(
        fixture.path(),
        "claude",
    )?));
    let codex: Arc<dyn AgentDriver> = Arc::new(CodexDriver::with_binary(failing_probe(
        fixture.path(),
        "codex",
    )?));
    let drivers: Drivers = Arc::new(move |vendor| match vendor {
        Vendor::ClaudeCode => Arc::clone(&claude),
        Vendor::Codex => Arc::clone(&codex),
    });
    let built = build_context_inner(
        home.path(),
        project.path(),
        &drivers,
        &Limiter::new(1),
        &BuildContextRequest {
            set_id: saved.set.id,
            operation_id: "failed-check".to_owned(),
            app: None,
            model: None,
            generation: 1,
            deadline: Duration::from_secs(2),
            budget_usd: None,
        },
        &CancellationToken::new(),
    )
    .await?;
    let state = built.build.ok_or("missing build state")?;
    assert_eq!(state.end, BuildEnd::Failed);
    assert!(state.said.contains("could not check"));
    assert!(
        !state
            .said
            .contains("Neither Claude Code nor Codex is available")
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_full_format_correction_can_publish_but_a_bare_bad_answer_cannot()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "One correction")?;
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![ContextSource {
                id: "typed".to_owned(),
                kind: SourceKind::Text,
                name: "Notes".to_owned(),
                text: "Keep the total visible while the cart changes.".to_owned(),
                ..ContextSource::default()
            }],
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;
    let concrete: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(
        correcting_executable(fixture.path())?,
    ));
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&concrete));
    let built = build_context_inner(
        home.path(),
        project.path(),
        &drivers,
        &Limiter::new(1),
        &BuildContextRequest {
            set_id: saved.set.id,
            operation_id: "one-correction".to_owned(),
            app: Some(Vendor::ClaudeCode),
            model: None,
            generation: 1,
            deadline: Duration::from_secs(20),
            budget_usd: None,
        },
        &CancellationToken::new(),
    )
    .await?;
    assert_eq!(
        built.build.as_ref().map(|build| build.end),
        Some(BuildEnd::Ready),
        "both drivers had to reach a ready version, and the build said: {}",
        built
            .build
            .as_ref()
            .map_or("(no build state)", |build| build.said.as_str())
    );
    assert_eq!(
        built
            .revision
            .as_ref()
            .and_then(|revision| revision.model.as_deref()),
        Some("sonnet")
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_bad_json_answers_leave_a_named_failed_build() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Bad answers")?;
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![ContextSource {
                id: "typed".to_owned(),
                kind: SourceKind::Text,
                name: "Notes".to_owned(),
                text: "Keep the total visible.".to_owned(),
                ..ContextSource::default()
            }],
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;
    let concrete: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(bad_json_executable(
        fixture.path(),
    )?));
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&concrete));
    let built = build_context_inner(
        home.path(),
        project.path(),
        &drivers,
        &Limiter::new(1),
        &BuildContextRequest {
            set_id: saved.set.id,
            operation_id: "two-bad-answers".to_owned(),
            app: Some(Vendor::ClaudeCode),
            model: None,
            generation: 1,
            deadline: Duration::from_secs(20),
            budget_usd: None,
        },
        &CancellationToken::new(),
    )
    .await?;
    let state = built.build.ok_or("missing build state")?;
    assert_eq!(state.end, BuildEnd::Failed);
    assert!(
        state
            .said
            .contains("one format correction was already used")
    );
    assert!(
        state
            .sources
            .iter()
            .all(|source| source.outcome == loadout_lib::context::SourceOutcome::Failed)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_references_fail_at_validation_and_leave_no_ready_version()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Wrong reference")?;
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![ContextSource {
                id: "typed".to_owned(),
                kind: SourceKind::Text,
                name: "Notes".to_owned(),
                text: "Keep the total visible.".to_owned(),
                ..ContextSource::default()
            }],
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;
    let concrete: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(
        wrong_reference_executable(fixture.path())?,
    ));
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&concrete));
    let built = build_context_inner(
        home.path(),
        project.path(),
        &drivers,
        &Limiter::new(1),
        &BuildContextRequest {
            set_id: saved.set.id,
            operation_id: "wrong-reference".to_owned(),
            app: Some(Vendor::ClaudeCode),
            model: None,
            generation: 1,
            deadline: Duration::from_secs(20),
            budget_usd: None,
        },
        &CancellationToken::new(),
    )
    .await?;
    let state = built.build.ok_or("missing build state")?;
    assert_eq!(state.end, BuildEnd::Failed);
    // 2026-09-08 — asercja POKAZUJE zdanie, ktore dostal czlowiek. Golе `assert!` mowilo tu
    // wylacznie „false", wiec kazda diagnoza zaczynala sie od zgadywania, co naprawde stoi
    // w `said` (niezmiennik 29: kryterium dotyczy zdania, ktore widzi czlowiek).
    assert!(
        state.said.contains("exact references"),
        "the failed build has to say the references were wrong, and it said: {}",
        state.said
    );
    // 2026-09-08 — TA ASERCJA ZOSTALA ZMIENIONA, i to jest rozstrzygniecie sporu miedzy
    // testem a kodem, nie osłabienie kryterium.
    //
    // Stalo tu zadanie, zeby zdanie porazki mowilo „one format correction was already used".
    // Zmierzone: dla ZLEGO ODWOLANIA zadna korekta nie jest zuzywana. `read_findings` zbiera
    // takie odwolanie do listy `missing` i konczy partie SUKCESEM (`context/findings.rs`),
    // a porazka wychodzi dopiero przy publikacji. Jedyna korekta jest zarezerwowana dla ZLEGO
    // FORMATU i broni jej `one_full_format_correction_can_publish_but_a_bare_bad_answer_cannot`
    // oraz `two_bad_json_answers_leave_a_named_failed_build` — oba zielone.
    //
    // Zdanie, ktorego zadal stary test, byloby wiec NIEPRAWDA powiedziana czlowiekowi: „zuzyto
    // korekte", ktorej nikt nie zuzyl. Kryterium pyta teraz o to, co naprawde ma byc prawda —
    // problem jest nazwany, czlowiek wie, co zrobic, a poprzednia gotowa wersja stoi nietknieta
    // (PLAN §6: „wynik niepublikowalny zachowuje poprzednia wersje i wskazuje, co poprawic").
    assert!(
        state.said.contains("Rebuild context"),
        "a build that cannot publish has to tell the person what to do, and it said: {}",
        state.said
    );
    assert!(
        built.revision.is_none(),
        "a build that failed validation must not leave a ready version behind"
    );
    assert!(
        state
            .sources
            .iter()
            .all(|source| { source.outcome == loadout_lib::context::SourceOutcome::Failed })
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn not_logged_in_is_a_named_agent_result_not_an_empty_revision() -> Result<(), Box<dyn Error>>
{
    let home = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Signed out")?;
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![ContextSource {
                id: "typed".to_owned(),
                kind: SourceKind::Text,
                name: "Notes".to_owned(),
                text: "Keep the total visible.".to_owned(),
                ..ContextSource::default()
            }],
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;
    let concrete: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(
        not_logged_in_executable(fixture.path())?,
    ));
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&concrete));
    let built = build_context_inner(
        home.path(),
        project.path(),
        &drivers,
        &Limiter::new(1),
        &BuildContextRequest {
            set_id: saved.set.id,
            operation_id: "not-logged-in".to_owned(),
            app: Some(Vendor::ClaudeCode),
            model: None,
            generation: 1,
            deadline: Duration::from_secs(20),
            budget_usd: None,
        },
        &CancellationToken::new(),
    )
    .await?;
    let state = built.build.ok_or("missing build state")?;
    assert_eq!(state.end, BuildEnd::Failed);
    assert!(state.said.contains("Not logged in"));
    assert!(built.revision.is_none());
    assert!(
        state
            .sources
            .iter()
            .all(|source| { source.outcome == loadout_lib::context::SourceOutcome::Failed })
    );
    Ok(())
}

/// Brakujący katalog projektu daje ZDANIE, nie `errno`, i nie odpala agenta.
///
/// 2026-09-09 (CT-09, znalezisko z natywnego QA) — właściciel nacisnął `Rebuild context`
/// i dostał przy każdym z trzech źródeł: „Loadout could not protect this batch's files: No such
/// file or directory (os error 2). No agent was started." Katalog projektu, który aplikacja
/// wybrała sama (`lib.rs::project_dir`), nigdy nie został założony, a jest podawany granicy
/// plików jako korzeń UKRYTY — więc `FenceRoot::open` nie miał czego otworzyć.
///
/// SŁABA WERSJA TEGO KRYTERIUM: `assert!(said.contains("could not"))`. Przechodzi ją dokładnie
/// to zdanie z `os error 2`, które właściciela zablokowało. Dlatego asercje są dwie i obie są
/// o treści: nazwa brakującego katalogu MUSI być w zdaniu, a `os error` NIE MOŻE.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_project_folder_that_is_not_there_is_named_and_no_agent_starts()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Missing project")?;
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![ContextSource {
                id: "typed".to_owned(),
                kind: SourceKind::Text,
                name: "Notes".to_owned(),
                text: "This build never reaches a vendor.".to_owned(),
                ..ContextSource::default()
            }],
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;
    let library = library_root(home.path());
    let folder = folder_of(&library, &saved.set.id)?;
    let operation = "no-project-folder";
    let concrete: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(blocking_executable(
        fixture.path(),
    )?));
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&concrete));

    // Katalog projektu, którego NIE MA. `tempdir` daje istniejącą ścieżkę, więc dokładamy do
    // niej nazwę, której nikt nie utworzył — inaczej ten test nie miałby o czym być.
    let nowhere = tempfile::tempdir()?;
    let project = nowhere.path().join("never-made");
    assert!(
        !project.is_dir(),
        "the fixture project folder must be absent"
    );

    let built = build_context_inner(
        home.path(),
        &project,
        &drivers,
        &Limiter::new(1),
        &BuildContextRequest {
            set_id: saved.set.id,
            operation_id: operation.to_owned(),
            app: Some(Vendor::ClaudeCode),
            model: None,
            generation: 1,
            deadline: Duration::from_secs(20),
            budget_usd: None,
        },
        &CancellationToken::new(),
    )
    .await?;

    let state = built.build.ok_or("missing build state")?;
    assert_eq!(state.end, BuildEnd::Failed);
    assert!(
        state.said.contains(&project.display().to_string()),
        "the sentence has to name the folder that is missing, or nobody can act on it: {}",
        state.said
    );
    assert!(
        !state.said.contains("os error"),
        "a raw errno is not a sentence for a person (invariant 14): {}",
        state.said
    );
    assert!(built.revision.is_none());
    let private = folder.join("builds").join(operation).join("private");
    assert!(
        find_named(&private, "context.pid")?.is_none(),
        "no agent may start when Loadout cannot protect the folders it promised to hide"
    );
    Ok(())
}

/// Stop złapany W OCZEKIWANIU NA SLOT nie zostawia po sobie ani jednego procesu vendora.
///
/// 2026-09-08 (CT-09) — `prepare_turn` wybiera `tokio::select!` między `cancel.cancelled()`
/// a `slots.place(...)`, więc zachowanie istniało; nie miało świadka. Sąsiedni
/// `stop_returns_only_after_the_real_driver_group_is_dead` anuluje bieg, który JUŻ ma żywą
/// grupę, czyli mierzy drugą stronę tej samej bramy: tam dowodem jest śmierć grupy, tutaj
/// dowodem jest jej NIEISTNIENIE.
///
/// SŁABA WERSJA TEGO KRYTERIUM: sam `BuildEnd::Cancelled`. Przechodzi ją budowanie, które
/// odpaliło CLI, zapłaciło za turę i dopiero potem zauważyło anulowanie. Dlatego asercja stoi
/// na braku pliku `context.pid` — czyli na tym, że proces nigdy nie powstał — i na tym, że
/// zwolnienie slotu nie odblokowuje pracy, której człowiek już nie chce.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_while_waiting_for_a_slot_never_starts_a_vendor() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Waiting")?;
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![ContextSource {
                id: "typed".to_owned(),
                kind: SourceKind::Text,
                name: "Notes".to_owned(),
                text: "This must never reach a vendor.".to_owned(),
                ..ContextSource::default()
            }],
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;
    let library = library_root(home.path());
    let folder = folder_of(&library, &saved.set.id)?;
    let operation = "stopped-in-the-queue";
    let concrete: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(blocking_executable(
        fixture.path(),
    )?));
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&concrete));
    let cancel = CancellationToken::new();

    // JEDNO miejsce w puli i ono jest już zajęte. `held` żyje do końca testu, więc budowanie
    // stoi w kolejce i nie ma jak dojść do sterownika.
    let limiter = Limiter::new(1);
    let held = limiter.place(Weight::Ordinary).await;

    let request = BuildContextRequest {
        set_id: saved.set.id,
        operation_id: operation.to_owned(),
        app: Some(Vendor::ClaudeCode),
        model: None,
        generation: 1,
        deadline: Duration::from_secs(20),
        budget_usd: None,
    };
    let running = build_context_inner(
        home.path(),
        project.path(),
        &drivers,
        &limiter,
        &request,
        &cancel,
    );
    tokio::pin!(running);

    // Przesłanka: dopóki slot jest zajęty, budowanie NIE kończy się samo i nie odpala procesu.
    let private = folder.join("builds").join(operation).join("private");
    tokio::select! {
        result = &mut running => {
            return Err(format!("the build ended while the only slot was held: {result:?}").into());
        }
        () = tokio::time::sleep(Duration::from_millis(400)) => {}
    }
    assert!(
        find_named(&private, "context.pid")?.is_none(),
        "a vendor process started while the build was still queued for a slot"
    );

    cancel.cancel();
    /* SUFIT CZASU, ŻEBY REGRESJA PADAŁA ZDANIEM, A NIE WISIAŁA. Zdjęcie ramienia
     * `cancel.cancelled()` z `tokio::select!` w `prepare_turn` nie przewraca tej asercji —
     * ono ZAWIESZA budowanie na zawsze, bo slot trzyma `held` do końca testu. Bez tego sufitu
     * mutacja kończy się wywaleniem całej suity na timeout bramki, czyli sygnałem, po którym
     * nikt nie wie, co się stało (zmierzone 2026-09-08, CT-09). */
    let built = tokio::time::timeout(Duration::from_secs(10), running)
        .await
        .map_err(|_| {
            "Stop did not reach the build waiting for a slot: it is still queued ten seconds \
             later, so cancellation is not racing the slot at all"
        })??;
    assert_eq!(
        built.build.as_ref().map(|build| build.end),
        Some(BuildEnd::Cancelled)
    );
    assert!(built.revision.is_none());
    assert!(
        find_named(&private, "context.pid")?.is_none(),
        "Stop in the queue still left a vendor process behind"
    );
    drop(held);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_returns_only_after_the_real_driver_group_is_dead() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Stop")?;
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![ContextSource {
                id: "typed".to_owned(),
                kind: SourceKind::Text,
                name: "Notes".to_owned(),
                text: "Keep the total visible.".to_owned(),
                ..ContextSource::default()
            }],
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;
    let library = library_root(home.path());
    let folder = folder_of(&library, &saved.set.id)?;
    let operation = "stopped-real-group";
    let concrete: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(blocking_executable(
        fixture.path(),
    )?));
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&concrete));
    let cancel = CancellationToken::new();
    let limiter = Limiter::new(1);
    let request = BuildContextRequest {
        set_id: saved.set.id,
        operation_id: operation.to_owned(),
        app: Some(Vendor::ClaudeCode),
        model: None,
        generation: 1,
        deadline: Duration::from_secs(20),
        budget_usd: None,
    };
    let running = build_context_inner(
        home.path(),
        project.path(),
        &drivers,
        &limiter,
        &request,
        &cancel,
    );
    tokio::pin!(running);
    let private = folder.join("builds").join(operation).join("private");
    let pid = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            tokio::select! {
                result = &mut running => {
                    return Err(format!("the build ended before its real group was visible: {result:?}"));
                }
                () = tokio::time::sleep(Duration::from_millis(20)) => {
                    if let Some(path) = find_named(&private, "context.pid").map_err(|error| error.to_string())? {
                        let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
                        break text.trim().parse::<i32>().map_err(|error| error.to_string());
                    }
                }
            }
        }
    })
    .await
    .map_err(|_| "the real driver did not start within ten seconds")??;
    assert!(group_is_alive(pid), "the group was not alive before Stop");
    cancel.cancel();
    let built = running.await?;
    assert_eq!(
        built.build.as_ref().map(|build| build.end),
        Some(BuildEnd::Cancelled)
    );
    assert!(
        group_is_dead(pid),
        "Stop returned before kill(-pgid, 0) said ESRCH"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn timeout_returns_only_after_the_real_driver_group_is_dead() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Timeout")?;
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![ContextSource {
                id: "typed".to_owned(),
                kind: SourceKind::Text,
                name: "Notes".to_owned(),
                text: "Keep the total visible.".to_owned(),
                ..ContextSource::default()
            }],
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;
    let library = library_root(home.path());
    let folder = folder_of(&library, &saved.set.id)?;
    let operation = "timed-out-real-group";
    let concrete: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(blocking_executable(
        fixture.path(),
    )?));
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&concrete));
    let built = build_context_inner(
        home.path(),
        project.path(),
        &drivers,
        &Limiter::new(1),
        &BuildContextRequest {
            set_id: saved.set.id,
            operation_id: operation.to_owned(),
            app: Some(Vendor::ClaudeCode),
            model: None,
            generation: 1,
            deadline: Duration::from_millis(300),
            budget_usd: None,
        },
        &CancellationToken::new(),
    )
    .await?;
    let pid_path = find_named(
        &folder.join("builds").join(operation).join("private"),
        "context.pid",
    )?
    .ok_or("the timed build saved no pid")?;
    let pid = fs::read_to_string(pid_path)?.trim().parse::<i32>()?;
    let state = built.build.ok_or("missing build state")?;
    assert_eq!(state.end, BuildEnd::Failed);
    assert!(state.said.contains("time limit"));
    assert!(
        group_is_dead(pid),
        "the time limit returned before kill(-pgid, 0) said ESRCH"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_measured_spending_limit_stops_the_real_codex_group() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let native_home = tempfile::tempdir()?;
    fs::create_dir_all(native_home.path().join(".codex"))?;
    fs::write(
        native_home.path().join(".codex/auth.json"),
        "{\"OPENAI_API_KEY\":\"unit-test-not-a-real-key\"}\n",
    )?;
    let made = create_context_set_inner(home.path(), "Measured spending")?;
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![ContextSource {
                id: "typed".to_owned(),
                kind: SourceKind::Text,
                name: "Notes".to_owned(),
                text: "Keep the total visible.".to_owned(),
                ..ContextSource::default()
            }],
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;
    let concrete: Arc<dyn AgentDriver> =
        CodexDriver::with_binary(spending_codex_executable(fixture.path())?)
            .configured(&DriverConfiguration {
                arguments: Vec::new(),
                environment: vec![(
                    "HOME".to_owned(),
                    native_home.path().as_os_str().to_os_string(),
                )],
                servers: Vec::new(),
            })
            .ok_or("Codex did not accept its test home")?;
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&concrete));
    let built = build_context_inner(
        home.path(),
        project.path(),
        &drivers,
        &Limiter::new(1),
        &BuildContextRequest {
            set_id: saved.set.id,
            operation_id: "measured-spending".to_owned(),
            app: Some(Vendor::Codex),
            model: Some("gpt-5.6-sol".to_owned()),
            generation: 1,
            deadline: Duration::from_secs(20),
            budget_usd: Some(0.01),
        },
        &CancellationToken::new(),
    )
    .await?;
    let state = built.build.ok_or("missing build state")?;
    assert_eq!(state.end, BuildEnd::Failed);
    assert!(state.said.contains("spending limit"));
    assert!(built.revision.is_none());
    Ok(())
}

#[test]
fn a_newer_draft_refuses_a_delayed_generation() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Stale")?;
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![ContextSource {
                id: "typed".to_owned(),
                kind: SourceKind::Text,
                name: "Notes".to_owned(),
                text: "Before".to_owned(),
                ..ContextSource::default()
            }],
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;
    let library = library_root(home.path());
    let frozen = build::freeze_and_split(&library, &saved.set.id, "claude-code", None, None)
        .map_err(|failure| failure.said)?;
    let request = BuildContextRequest {
        set_id: saved.set.id.clone(),
        operation_id: "stale-build".to_owned(),
        app: Some(Vendor::ClaudeCode),
        model: None,
        generation: 1,
        deadline: Duration::from_secs(20),
        budget_usd: None,
    };
    let state = build::initial_state(
        &request,
        "claude-code",
        saved.set.draft_revision,
        frozen.input_fingerprint.clone(),
        frozen.sources.clone(),
        "2026-09-08T10:00:00Z",
    );
    let accepted = findings::read_findings(ANSWER.as_bytes(), &frozen.batches[0].references)?;
    let _newer = save_context_draft_inner(
        home.path(),
        &saved.set.id,
        &saved.set.title,
        &saved.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![ContextSource {
                id: "typed".to_owned(),
                kind: SourceKind::Text,
                name: "Notes".to_owned(),
                text: "After".to_owned(),
                ..ContextSource::default()
            }],
            ..ContextDraft::default()
        },
        Some(saved.revision),
    )?;
    let Err(refused) =
        build::publish_revision(&library, &frozen, &accepted, &state, "2026-09-08T10:01:00Z")
    else {
        return Err("a delayed generation published over a newer draft".into());
    };
    assert!(refused.to_string().contains("older answer was not saved"));
    assert!(
        loadout_lib::context::files::read_set(&library, &saved.set.id)?
            .set
            .latest_ready_revision
            .is_none()
    );
    Ok(())
}

#[test]
fn a_human_revision_refuses_an_answer_frozen_before_that_revision() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Human wins")?;
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![ContextSource {
                id: "typed".to_owned(),
                kind: SourceKind::Text,
                name: "Notes".to_owned(),
                text: "Keep the total visible.".to_owned(),
                ..ContextSource::default()
            }],
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;
    let library = library_root(home.path());
    let first = build::freeze_and_split(&library, &saved.set.id, "claude-code", None, None)
        .map_err(|failure| failure.said)?;
    let first_request = BuildContextRequest {
        set_id: saved.set.id.clone(),
        operation_id: "first-ready".to_owned(),
        app: Some(Vendor::ClaudeCode),
        model: None,
        generation: 1,
        deadline: Duration::from_secs(20),
        budget_usd: None,
    };
    let first_state = build::initial_state(
        &first_request,
        "claude-code",
        saved.set.draft_revision,
        first.input_fingerprint.clone(),
        first.sources.clone(),
        "2026-09-08T10:00:00Z",
    );
    let accepted = findings::read_findings(ANSWER.as_bytes(), &first.batches[0].references)?;
    let first_revision = build::publish_revision(
        &library,
        &first,
        &accepted,
        &first_state,
        "2026-09-08T10:01:00Z",
    )?;
    let folder = folder_of(&library, &saved.set.id)?;
    let first_findings = fs::read(
        folder
            .join("versions")
            .join(&first_revision.id)
            .join("findings.json"),
    )?;

    let delayed = build::freeze_and_split(&library, &saved.set.id, "claude-code", None, None)
        .map_err(|failure| failure.said)?;
    let delayed_request = BuildContextRequest {
        operation_id: "delayed".to_owned(),
        generation: 2,
        ..first_request
    };
    let delayed_state = build::initial_state(
        &delayed_request,
        "claude-code",
        saved.set.draft_revision,
        delayed.input_fingerprint.clone(),
        delayed.sources.clone(),
        "2026-09-08T10:02:00Z",
    );
    let human = build::save_human_revision(
        &library,
        &saved.set.id,
        &RevisionEdit {
            correction: "Keep the corrected total.".to_owned(),
            finding_id: None,
            text: None,
        },
        "2026-09-08T10:03:00Z",
    )?;
    let next = build::freeze_and_split(&library, &saved.set.id, "claude-code", None, None)
        .map_err(|failure| failure.said)?;
    let next_prompt = loadout_lib::context::prompt::asked_for(
        &next.set,
        &next.draft,
        &next.batches[0].material,
        &next.batches[0].references,
        &next.prior_human,
    );
    assert!(next_prompt.contains("Keep the corrected total."));
    let delayed_findings =
        findings::read_findings(ANSWER.as_bytes(), &delayed.batches[0].references)?;
    let Err(refused) = build::publish_revision(
        &library,
        &delayed,
        &delayed_findings,
        &delayed_state,
        "2026-09-08T10:04:00Z",
    ) else {
        return Err("a delayed generated answer replaced the newer human version".into());
    };
    assert!(refused.to_string().contains("older answer was not saved"));
    assert_eq!(
        loadout_lib::context::files::read_set(&library, &saved.set.id)?
            .set
            .latest_ready_revision
            .as_deref(),
        Some(human.id.as_str())
    );
    assert_eq!(
        fs::read(
            folder
                .join("versions")
                .join(&first_revision.id)
                .join("findings.json")
        )?,
        first_findings
    );
    Ok(())
}

#[test]
fn a_human_correction_invalidates_a_saved_batch() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Correction input")?;
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![ContextSource {
                id: "typed".to_owned(),
                kind: SourceKind::Text,
                name: "Notes".to_owned(),
                text: "Keep the total visible.".to_owned(),
                ..ContextSource::default()
            }],
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;
    let library = library_root(home.path());
    let first = build::freeze_and_split(&library, &saved.set.id, "claude-code", None, None)
        .map_err(|failure| failure.said)?;
    let first_request = BuildContextRequest {
        set_id: saved.set.id.clone(),
        operation_id: "before-human".to_owned(),
        app: Some(Vendor::ClaudeCode),
        model: None,
        generation: 1,
        deadline: Duration::from_secs(20),
        budget_usd: None,
    };
    let first_state = build::initial_state(
        &first_request,
        "claude-code",
        saved.set.draft_revision,
        first.input_fingerprint.clone(),
        first.sources.clone(),
        "2026-09-08T10:00:00Z",
    );
    build::write_state(&library, &first_state)?;
    let accepted = findings::read_findings(ANSWER.as_bytes(), &first.batches[0].references)?;
    build::save_batch(
        &first,
        &first_request.operation_id,
        &first.batches[0],
        &accepted,
    )?;
    build::publish_revision(
        &library,
        &first,
        &accepted,
        &first_state,
        "2026-09-08T10:01:00Z",
    )?;
    build::save_human_revision(
        &library,
        &saved.set.id,
        &RevisionEdit {
            correction: "Keep the corrected total.".to_owned(),
            finding_id: None,
            text: None,
        },
        "2026-09-08T10:02:00Z",
    )?;

    let next = build::freeze_and_split(&library, &saved.set.id, "claude-code", None, None)
        .map_err(|failure| failure.said)?;
    assert_ne!(first.input_fingerprint, next.input_fingerprint);
    assert!(
        build::cached_batch(&next, "after-human", &next.batches[0]).is_none(),
        "a batch made before the human correction was reused"
    );
    Ok(())
}

#[test]
fn exclusion_only_comes_from_the_saved_draft_and_every_source_is_counted()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Coverage")?;
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![
                ContextSource {
                    id: "included".to_owned(),
                    kind: SourceKind::Text,
                    name: "Included".to_owned(),
                    text: "Read this.".to_owned(),
                    ..ContextSource::default()
                },
                ContextSource {
                    id: "excluded".to_owned(),
                    kind: SourceKind::Text,
                    name: "Excluded".to_owned(),
                    text: "Leave this out.".to_owned(),
                    ..ContextSource::default()
                },
            ],
            excluded: vec!["excluded".to_owned()],
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;
    let frozen = build::freeze_and_split(
        &library_root(home.path()),
        &saved.set.id,
        "claude-code",
        None,
        None,
    )
    .map_err(|failure| failure.said)?;
    assert_eq!(frozen.sources.len(), 2);
    assert_eq!(
        frozen
            .sources
            .iter()
            .filter(|source| source.outcome == loadout_lib::context::SourceOutcome::Excluded)
            .map(|source| source.source_id.as_str())
            .collect::<Vec<_>>(),
        vec!["excluded"]
    );
    assert!(
        frozen.batches.iter().all(|batch| {
            batch.material.len() <= loadout_lib::context::limits::BATCH_TEXT_BYTES
        })
    );
    Ok(())
}

#[test]
fn the_shared_image_limit_starts_a_new_batch() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Picture limits")?;
    let library = library_root(home.path());
    let folder = folder_of(&library, &made.set.id)?;
    let mut picture = b"\x89PNG\r\n\x1a\n".to_vec();
    picture.resize(3 * 1024 * 1024 + 1, 0);
    let mut sources = Vec::new();
    for number in 1..=4 {
        let id = format!("picture-{number}");
        let relative = format!("sources/{id}/r1/original.png");
        let revision = folder.join(format!("sources/{id}/r1"));
        fs::create_dir_all(&revision)?;
        fs::write(revision.join("original.png"), &picture)?;
        fs::write(revision.join("for-the-agent.png"), &picture)?;
        sources.push(ContextSource {
            id,
            kind: SourceKind::Image,
            name: format!("Picture {number}"),
            file: Some(StoredFile {
                path: relative,
                revision: "r1".to_owned(),
                mime: "image/png".to_owned(),
                bytes: picture.len() as u64,
                fingerprint: format!("picture-{number}"),
                derived: picture.len() as u64,
                pages: None,
            }),
            ..ContextSource::default()
        });
    }
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources,
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;

    let frozen = build::freeze_and_split(&library, &saved.set.id, "claude-code", None, None)
        .map_err(|failure| failure.said)?;
    assert_eq!(frozen.batches.len(), 2);
    assert!(
        frozen
            .batches
            .iter()
            .all(|batch| batch.images.as_slice().len() <= 3)
    );
    Ok(())
}

#[test]
fn a_freeze_failure_leaves_no_source_part_without_an_outcome() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Complete failure")?;
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![
                ContextSource {
                    id: "ready".to_owned(),
                    kind: SourceKind::Text,
                    name: "Ready notes".to_owned(),
                    text: "This fragment can be read.".to_owned(),
                    ..ContextSource::default()
                },
                ContextSource {
                    id: "waiting".to_owned(),
                    kind: SourceKind::Pdf,
                    name: "Waiting document".to_owned(),
                    preparation: Preparation::Needs { pages_done: 0 },
                    ..ContextSource::default()
                },
            ],
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;
    let Err(failure) = build::freeze_and_split(
        &library_root(home.path()),
        &saved.set.id,
        "claude-code",
        None,
        None,
    ) else {
        return Err("an unprepared document entered a build".into());
    };
    assert_eq!(failure.stage, loadout_lib::context::BuildStage::Freezing);
    assert_eq!(failure.sources.len(), 2);
    assert!(failure.sources.iter().all(|source| {
        matches!(
            source.outcome,
            loadout_lib::context::SourceOutcome::Excluded
                | loadout_lib::context::SourceOutcome::Failed
        )
    }));
    Ok(())
}

#[test]
fn a_saved_preparation_failure_keeps_its_exact_remedy() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Failed document")?;
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![ContextSource {
                id: "damaged".to_owned(),
                kind: SourceKind::Pdf,
                name: "Damaged document".to_owned(),
                preparation: Preparation::Failed {
                    said: "Add a readable copy saved from the original app.".to_owned(),
                },
                ..ContextSource::default()
            }],
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;
    let Err(failure) = build::freeze_and_split(
        &library_root(home.path()),
        &saved.set.id,
        "claude-code",
        None,
        None,
    ) else {
        return Err("a failed document entered a build".into());
    };
    assert!(failure.said.contains("Add a readable copy"));
    assert!(failure.sources[0].said.contains("Add a readable copy"));
    Ok(())
}

#[test]
fn an_empty_text_source_is_failed_instead_of_disappearing() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Empty source")?;
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![
                ContextSource {
                    id: "empty".to_owned(),
                    kind: SourceKind::Text,
                    name: "Empty notes".to_owned(),
                    text: String::new(),
                    ..ContextSource::default()
                },
                ContextSource {
                    id: "ready".to_owned(),
                    kind: SourceKind::Text,
                    name: "Ready notes".to_owned(),
                    text: "This part is ready.".to_owned(),
                    ..ContextSource::default()
                },
            ],
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;
    let Err(failure) = build::freeze_and_split(
        &library_root(home.path()),
        &saved.set.id,
        "claude-code",
        None,
        None,
    ) else {
        return Err("an empty source disappeared from an otherwise valid build".into());
    };
    assert_eq!(failure.stage, loadout_lib::context::BuildStage::Splitting);
    assert_eq!(failure.sources.len(), 2);
    assert!(
        failure
            .sources
            .iter()
            .all(|source| { source.outcome == loadout_lib::context::SourceOutcome::Failed })
    );
    assert!(
        failure
            .sources
            .iter()
            .any(|source| source.source_id == "empty")
    );
    Ok(())
}

#[test]
fn a_saved_source_path_cannot_read_outside_its_set() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    let private = outside.path().join("not-in-this-set.txt");
    fs::write(&private, "This text belongs to another place.")?;
    let made = create_context_set_inner(home.path(), "Bounded reader")?;
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![ContextSource {
                id: "outside".to_owned(),
                kind: SourceKind::Document,
                name: "Outside".to_owned(),
                file: Some(StoredFile {
                    path: private.to_string_lossy().into_owned(),
                    revision: "r1".to_owned(),
                    mime: "text/plain".to_owned(),
                    bytes: fs::metadata(&private)?.len(),
                    fingerprint: "outside".to_owned(),
                    ..StoredFile::default()
                }),
                ..ContextSource::default()
            }],
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;
    let Err(failure) = build::freeze_and_split(
        &library_root(home.path()),
        &saved.set.id,
        "claude-code",
        None,
        None,
    ) else {
        return Err("the batch reader followed a path outside its set".into());
    };
    assert!(failure.said.contains("saved file path"));
    Ok(())
}

#[test]
fn a_ready_pointer_cannot_read_a_version_outside_its_set() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Bounded ready version")?;
    fs::create_dir_all(outside.path().join("topics"))?;
    fs::write(outside.path().join("index.md"), "# Outside\n")?;
    fs::write(outside.path().join("findings.json"), "[]\n")?;
    fs::write(outside.path().join("topics/outside.md"), "# Outside\n")?;
    let revision = ContextRevision {
        schema: 1,
        id: outside.path().to_string_lossy().into_owned(),
        set_id: made.set.id.clone(),
        index_file: "index.md".to_owned(),
        findings_file: "findings.json".to_owned(),
        topic_files: vec!["topics/outside.md".to_owned()],
        ..ContextRevision::default()
    };
    fs::write(
        outside.path().join("manifest.json"),
        serde_json::to_vec_pretty(&revision)?,
    )?;
    let library = library_root(home.path());
    let folder = folder_of(&library, &made.set.id)?;
    let mut set = made.set;
    set.latest_ready_revision = Some(outside.path().to_string_lossy().into_owned());
    fs::write(
        folder.join("manifest.json"),
        serde_json::to_vec_pretty(&set)?,
    )?;
    let Err(refused) = build::read_latest_revision(&library, &set.id) else {
        return Err("a ready pointer escaped the set's versions folder".into());
    };
    assert!(refused.to_string().contains("incomplete"));
    Ok(())
}

#[test]
fn a_missing_prepared_page_names_each_page_in_the_failed_build() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let made = create_context_set_inner(home.path(), "Prepared pages")?;
    let library = library_root(home.path());
    let folder = folder_of(&library, &made.set.id)?;
    let source = folder.join("sources/document/r1");
    fs::create_dir_all(source.join("pages"))?;
    fs::write(source.join("original.pdf"), "%PDF-1.7\n")?;
    fs::write(source.join("pages/page-0001.txt"), "First page.")?;
    let saved = save_context_draft_inner(
        home.path(),
        &made.set.id,
        &made.set.title,
        &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![ContextSource {
                id: "document".to_owned(),
                kind: SourceKind::Pdf,
                name: "Document".to_owned(),
                file: Some(StoredFile {
                    path: "sources/document/r1/original.pdf".to_owned(),
                    revision: "r1".to_owned(),
                    mime: "application/pdf".to_owned(),
                    bytes: 9,
                    fingerprint: "document-r1".to_owned(),
                    pages: Some(2),
                    ..StoredFile::default()
                }),
                preparation: Preparation::Ready,
                ..ContextSource::default()
            }],
            ..ContextDraft::default()
        },
        Some(made.revision),
    )?;
    let Err(failure) = build::freeze_and_split(&library, &saved.set.id, "claude-code", None, None)
    else {
        return Err("a missing second page entered a build".into());
    };
    assert_eq!(failure.stage, loadout_lib::context::BuildStage::Splitting);
    assert_eq!(
        failure
            .sources
            .iter()
            .map(|source| source.part.as_str())
            .collect::<Vec<_>>(),
        vec!["page 1", "page 2"]
    );
    assert!(
        failure
            .sources
            .iter()
            .all(|source| source.outcome == loadout_lib::context::SourceOutcome::Failed)
    );
    Ok(())
}

#[test]
fn a_bad_reference_is_kept_as_a_named_validation_failure() -> Result<(), Box<dyn Error>> {
    let allowed = BTreeSet::from([SourceReference {
        source_id: "typed".to_owned(),
        part: "fragment 1".to_owned(),
    }]);
    let wrong = ANSWER.replace("fragment 1", "fragment 2");
    let found = findings::read_findings(wrong.as_bytes(), &allowed)?;
    assert!(found.findings.is_empty());
    assert_eq!(found.missing.len(), 1);
    assert!(found.missing[0].contains("exact references"));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn both_real_drivers_receive_text_and_a_picture_through_their_native_input()
-> Result<(), Box<dyn Error>> {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
    for vendor in [Vendor::ClaudeCode, Vendor::Codex] {
        let home = tempfile::tempdir()?;
        let fixture = tempfile::tempdir()?;
        let project = tempfile::tempdir()?;
        let native_home = tempfile::tempdir()?;
        fs::create_dir_all(native_home.path().join(".codex"))?;
        fs::write(
            native_home.path().join(".codex/auth.json"),
            "{\"OPENAI_API_KEY\":\"unit-test-not-a-real-key\"}\n",
        )?;
        let made = create_context_set_inner(home.path(), "Both apps")?;
        let library = library_root(home.path());
        let folder = folder_of(&library, &made.set.id)?;
        let picture = folder.join("sources/picture/r1");
        fs::create_dir_all(&picture)?;
        fs::write(picture.join("original.png"), PNG)?;
        fs::write(picture.join("for-the-agent.png"), PNG)?;
        let saved = save_context_draft_inner(
            home.path(),
            &made.set.id,
            &made.set.title,
            &made.set.description,
            ContextDraft {
                schema: 1,
                sources: vec![
                    ContextSource {
                        id: "typed".to_owned(),
                        kind: SourceKind::Text,
                        name: "Notes".to_owned(),
                        text: "The total stays visible while the cart changes.".to_owned(),
                        ..ContextSource::default()
                    },
                    ContextSource {
                        id: "picture".to_owned(),
                        kind: SourceKind::Image,
                        name: "Checkout screen".to_owned(),
                        file: Some(StoredFile {
                            path: "sources/picture/r1/original.png".to_owned(),
                            revision: "r1".to_owned(),
                            mime: "image/png".to_owned(),
                            bytes: PNG.len() as u64,
                            fingerprint: "fixture".to_owned(),
                            derived: PNG.len() as u64,
                            pages: None,
                        }),
                        ..ContextSource::default()
                    },
                ],
                ..ContextDraft::default()
            },
            Some(made.revision),
        )?;
        let binary = transport_executable(fixture.path(), vendor)?;
        let selected: Arc<dyn AgentDriver> = match vendor {
            Vendor::ClaudeCode => Arc::new(ClaudeDriver::with_binary(binary)),
            Vendor::Codex => CodexDriver::with_binary(binary)
                .configured(&DriverConfiguration {
                    arguments: Vec::new(),
                    environment: vec![(
                        "HOME".to_owned(),
                        native_home.path().as_os_str().to_os_string(),
                    )],
                    servers: Vec::new(),
                })
                .ok_or("Codex did not accept its test home")?,
        };
        let missing = missing_drivers(fixture.path());
        let drivers: Drivers = Arc::new(move |asked| {
            if asked == vendor {
                Arc::clone(&selected)
            } else {
                missing(asked)
            }
        });
        let operation = format!("native-{vendor:?}");
        let built = build_context_inner(
            home.path(),
            project.path(),
            &drivers,
            &Limiter::new(1),
            &BuildContextRequest {
                set_id: saved.set.id,
                operation_id: operation.clone(),
                app: Some(vendor),
                model: None,
                generation: 1,
                deadline: Duration::from_secs(20),
                budget_usd: None,
            },
            &CancellationToken::new(),
        )
        .await?;
        assert_eq!(
            built.build.as_ref().map(|build| build.end),
            Some(BuildEnd::Ready),
            "{vendor:?} had to reach a ready version, and the build said: {}",
            built
                .build
                .as_ref()
                .map_or("(no build state)", |build| build.said.as_str())
        );
        let private = folder.join("builds").join(operation).join("private");
        let stdin = find_named(&private, "context.stdin")?.ok_or("test app saved no stdin")?;
        let argv = find_named(&private, "context.argv")?.ok_or("test app saved no argv")?;
        let pid = find_named(&private, "context.pid")?.ok_or("test app saved no pid")?;
        let sent = fs::read_to_string(stdin)?;
        assert!(sent.contains("The total stays visible while the cart changes."));
        assert!(sent.contains("iVBORw0KGgo"));
        assert!(!fs::read_to_string(argv)?.contains("The total stays visible"));
        assert!(!fs::read_to_string(pid)?.trim().is_empty());
    }
    Ok(())
}
