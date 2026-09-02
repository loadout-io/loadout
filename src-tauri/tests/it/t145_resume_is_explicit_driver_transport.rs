//! AC-2 dla T-145: resume jest jawnym transportem obu adapterów i nie wraca z recovery.
//!
//! Target rozdziela trzy granice: prawdziwy wiersz `SQLite` przechodzi przez `decide` bez sesji
//! w wyniku, `RunSpec.resume` dociera do pełnych kompozytorów argv Claude'a i Codeksa, a
//! nagłówek neutralnego modułu nazywa właściciela transportu. Sama obecność pola nie wystarcza.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use anyhow::{Context as _, Result};
use loadout_lib::engine::drivers::claude::{ClaudeDriver, VENDOR as CLAUDE};
use loadout_lib::engine::drivers::codex::{self, VENDOR as CODEX};
use loadout_lib::engine::drivers::{DriverConfiguration, Policy, RunSpec, SessionRef};
use loadout_lib::recovery::{self, Machine};
use rusqlite::Connection;
use serde_json::Value as Json;
use uuid::Uuid;

const BOOT: &str = "1787900000";
const OWN_PGID: i32 = 8145;
const RECOVERY_SESSION: &str = "session-that-must-not-leave-recovery";
const CLAUDE_SESSION: &str = "claude-session-from-runspec";
const CODEX_SESSION: &str = "codex-thread-from-runspec";
const CLAUDE_SENTINEL: &str = "configuration-reached-claude-command";
const CODEX_SENTINEL: &str = "t145_transport_sentinel=\"kept\"";
const CODEX_EFFORT: &str = "model_reasoning_effort=\"high\"";

fn recovery_database() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(
        r"CREATE TABLE runs (
              id TEXT PRIMARY KEY,
              status TEXT NOT NULL,
              boot_id TEXT
          );
          CREATE TABLE steps (
              id TEXT PRIMARY KEY,
              run_id TEXT NOT NULL,
              status TEXT NOT NULL,
              pid INTEGER,
              pgid INTEGER,
              agent_session_id TEXT,
              attempt INTEGER NOT NULL
          );
          INSERT INTO runs (id, status, boot_id)
          VALUES ('run-with-session', 'running', '1787900000');
          INSERT INTO steps
              (id, run_id, status, pid, pgid, agent_session_id, attempt)
          VALUES
              ('interrupted-with-session', 'run-with-session', 'running', 8301, 8301,
               'session-that-must-not-leave-recovery', 23);",
    )?;
    Ok(conn)
}

fn collect_keys(value: &Json, path: &str, found: &mut Vec<String>) {
    match value {
        Json::Object(fields) => {
            for (key, child) in fields {
                let here = format!("{path}.{key}");
                found.push(here.clone());
                collect_keys(child, &here, found);
            }
        }
        Json::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                collect_keys(child, &format!("{path}[{index}]"), found);
            }
        }
        _ => {}
    }
}

fn spec(resume: Option<SessionRef>) -> RunSpec {
    RunSpec {
        run_id: Uuid::from_u128(0x0199_ab00_0000_7000_8000_0000_0000_0145),
        cwd: PathBuf::from(".loadout/scratch/t145-driver-workspace"),
        prompt: "this stays on stdin".to_owned(),
        model: Some("model-from-runspec".to_owned()),
        system_append: None,
        policy: Policy::ReadOnly,
        reaches_the_web: false,
        tools: None,
        extra_dirs: Vec::new(),
        resume,
    }
}

fn claude_configuration() -> DriverConfiguration {
    DriverConfiguration {
        arguments: vec![
            "--t145-configuration-sentinel".to_owned(),
            CLAUDE_SENTINEL.to_owned(),
        ],
        environment: vec![(
            "T145_PRIVATE_CONFIGURATION".to_owned(),
            OsString::from("not-an-argv-value"),
        )],
        servers: vec!["t145-approved-server".to_owned()],
    }
}

fn codex_configuration() -> DriverConfiguration {
    DriverConfiguration {
        arguments: vec![
            "-c".to_owned(),
            CODEX_SENTINEL.to_owned(),
            "-c".to_owned(),
            CODEX_EFFORT.to_owned(),
        ],
        environment: vec![(
            "T145_PRIVATE_CONFIGURATION".to_owned(),
            OsString::from("not-an-argv-value"),
        )],
        servers: vec!["t145-approved-server".to_owned()],
    }
}

fn claude_args(driver: &ClaudeDriver, spec: &RunSpec) -> Vec<String> {
    driver
        .command(spec)
        .as_std()
        .get_args()
        .map(OsStr::to_string_lossy)
        .map(std::borrow::Cow::into_owned)
        .collect()
}

fn value_after<'args>(args: &'args [String], flag: &str) -> Option<&'args str> {
    let at = args.iter().position(|argument| argument == flag)?;
    args.get(at + 1).map(String::as_str)
}

fn has_pair(args: &[String], first: &str, second: &str) -> bool {
    args.windows(2)
        .any(|pair| pair[0] == first && pair[1] == second)
}

#[test]
fn a_recorded_session_never_becomes_a_recovery_question_or_resume_effect() -> Result<()> {
    let conn = recovery_database()?;
    let rows = recovery::rows_to_judge(&conn)?;
    assert_eq!(
        rows.len(),
        1,
        "the production query must deliver the interrupted control row before decide can prove \
         that it does not export its agent_session_id"
    );

    let plan = recovery::decide(
        &rows,
        &Machine {
            boot_id: BOOT.to_owned(),
            own_pgid: OWN_PGID,
        },
    );
    assert!(
        plan.step_status
            .iter()
            .any(|change| change.step_id == "interrupted-with-session"),
        "the control row was not handled, so an empty plan would make the transport boundary \
         below vacuous: {plan:?}"
    );

    let wire = serde_json::to_value(&plan)?;
    let text = serde_json::to_string(&wire)?;
    assert!(
        !text.contains(RECOVERY_SESSION),
        "decide returned the recorded agent_session_id. That value belongs to an explicit \
         RunSpec supplied by a caller, never to recovery output: {text}"
    );

    let mut keys = Vec::new();
    collect_keys(&wire, "plan", &mut keys);
    for forbidden in [
        "ask",
        "question",
        "resume",
        "session",
        "attempt",
        "option",
        "effect",
        "pick_up",
        "start_over",
    ] {
        assert!(
            !keys
                .iter()
                .any(|path| path.to_lowercase().contains(forbidden)),
            "recovery returned a nested {forbidden:?} field. It may reap and mark interrupted \
             rows, but it never constructs conversation transport; keys were {keys:?}"
        );
    }
    Ok(())
}

#[test]
fn none_selects_the_first_turn_in_both_full_production_argv_composers() {
    let claude = ClaudeDriver::new().with_configuration(claude_configuration());
    let fresh = spec(None);
    let fresh_claude = claude_args(&claude, &fresh);
    let minted = fresh.run_id.to_string();
    assert!(
        has_pair(
            &fresh_claude,
            "--t145-configuration-sentinel",
            CLAUDE_SENTINEL
        ),
        "the test must exercise ClaudeDriver::command with its real DriverConfiguration, not a \
         resume-only helper; argv was {fresh_claude:?}"
    );
    assert_eq!(
        value_after(&fresh_claude, "--session-id"),
        Some(minted.as_str()),
        "RunSpec.resume=None selects Claude's first turn and preassigns the run id; argv was \
         {fresh_claude:?}"
    );
    assert!(
        !fresh_claude.iter().any(|argument| argument == "--resume"),
        "a first Claude turn must not also resume a conversation: {fresh_claude:?}"
    );

    let codex_configuration = codex_configuration();
    let fresh_codex = codex::exec_argv(&codex_configuration, &fresh);
    assert!(
        fresh_codex.starts_with(&codex_configuration.arguments),
        "the full Codex composer must preserve configured global arguments before `exec`; argv \
         was {fresh_codex:?}"
    );
    assert!(
        has_pair(&fresh_codex, "-c", CODEX_SENTINEL),
        "the sentinel from a real DriverConfiguration never reached codex::exec_argv; argv was \
         {fresh_codex:?}"
    );
    assert_eq!(
        fresh_codex
            .get(codex_configuration.arguments.len())
            .map(String::as_str),
        Some("exec"),
        "RunSpec.resume=None must select the first-turn composer after global configuration: \
         {fresh_codex:?}"
    );
    assert!(
        !fresh_codex.iter().any(|argument| argument == "resume"),
        "a first Codex turn must not contain the resume subcommand: {fresh_codex:?}"
    );
    assert_eq!(
        fresh_codex.last().map(String::as_str),
        Some("-"),
        "the first-turn composer must keep the prompt on stdin: {fresh_codex:?}"
    );
}

#[test]
fn some_selects_resume_in_both_full_production_argv_composers() {
    let claude = ClaudeDriver::new().with_configuration(claude_configuration());
    let resumed_claude = spec(Some(SessionRef {
        vendor: CLAUDE,
        id: CLAUDE_SESSION.to_owned(),
    }));
    let resumed_claude_args = claude_args(&claude, &resumed_claude);
    assert!(
        has_pair(
            &resumed_claude_args,
            "--t145-configuration-sentinel",
            CLAUDE_SENTINEL
        ),
        "Claude resume must pass through the full command composer and retain its configured \
         arguments: {resumed_claude_args:?}"
    );
    assert_eq!(
        value_after(&resumed_claude_args, "--resume"),
        Some(CLAUDE_SESSION),
        "Claude must receive the exact identifier carried by RunSpec.resume; argv was \
         {resumed_claude_args:?}"
    );
    assert!(
        !resumed_claude_args
            .iter()
            .any(|argument| argument == "--session-id"),
        "a resumed Claude turn must not mint a competing session id: {resumed_claude_args:?}"
    );

    let codex_configuration = codex_configuration();
    let resumed_codex = spec(Some(SessionRef {
        vendor: CODEX,
        id: CODEX_SESSION.to_owned(),
    }));
    let resumed_codex_args = codex::exec_argv(&codex_configuration, &resumed_codex);
    assert!(
        has_pair(&resumed_codex_args, "-c", CODEX_SENTINEL),
        "the retained configuration sentinel proves this is codex::exec_argv rather than only \
         build_exec_argv; argv was {resumed_codex_args:?}"
    );
    assert!(
        !resumed_codex_args
            .iter()
            .any(|argument| argument == CODEX_EFFORT),
        "a resumed Codex thread keeps its original effort, so the full composer must remove \
         only that configuration pair: {resumed_codex_args:?}"
    );
    assert!(
        has_pair(&resumed_codex_args, "resume", CODEX_SESSION),
        "Codex must receive the resume subcommand immediately followed by the explicit thread \
         id from RunSpec.resume; argv was {resumed_codex_args:?}"
    );
    assert!(
        !resumed_codex_args
            .iter()
            .any(|argument| argument == "--resume"),
        "Codex resume is a subcommand, not Claude's flag: {resumed_codex_args:?}"
    );
}

#[test]
fn the_driver_header_names_transport_ownership_and_the_recovery_boundary() -> Result<()> {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/engine/drivers/mod.rs"
    ))?;
    let header = source
        .split_once("use std::ffi::OsString;")
        .map(|(header, _)| header)
        .context("drivers/mod.rs no longer has its documented module header")?;
    let paragraph = header
        .split("\n//!\n")
        .find(|paragraph| {
            paragraph.contains("RunSpec::resume") || paragraph.contains("RunSpec.resume")
        })
        .context(
            "the module header must contain its own RunSpec.resume paragraph; field docs alone \
             do not define ownership between adapters and recovery",
        )?;
    let meaning = paragraph.to_lowercase();
    assert!(
        meaning.contains("jawn")
            && meaning.contains("transport")
            && meaning.contains("adapter")
            && meaning.contains("recovery")
            && meaning.contains("nie")
            && meaning.contains("konstru"),
        "the header must state the complete boundary: RunSpec.resume is explicit adapter \
         transport and recovery does not construct it. Paragraph was {paragraph:?}"
    );
    Ok(())
}
