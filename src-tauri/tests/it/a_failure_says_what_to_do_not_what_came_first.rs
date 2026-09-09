//! Znany powód porażki wygrywa z pierwszą linią hałasu na ekranie.
//!
//! 2026-09-09 — wygasła sesja Claude Code, ale przed prawdziwą przyczyną stderr wypisał
//! `shell-init: ... getcwd ...`. Loadout pokazał tę pierwszą linię przy każdym źródle i skierował
//! właściciela w stronę katalogów oraz uprawnień, choć naprawą było ponowne logowanie poza
//! aplikacją. Cztery skargi osobno bronią kolejności i jej przesłanek; piąta prowadzi tę samą
//! rodzinę przez drugi adapter i jedyną kurację widoczną w strumieniu.

#![allow(clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use loadout_lib::engine::drivers::claude::ClaudeDecoder;
use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::{AgentHandle, Policy, RunSpec};
use loadout_lib::engine::line::{Curator, Line, Seen};
use tokio::sync::mpsc;
use uuid::Uuid;

const SIGN_IN: &str =
    "The agent app is not signed in. Sign in to it outside Loadout, then try again.";
const CLAUDE_PREFIX: &str = "The agent stopped without ever sending its result. ";
const CODEX_PREFIX: &str = "The agent stopped without ever finishing its turn. ";
const WAIT: Duration = Duration::from_secs(20);

fn claude_row_for(complaint: &str) -> Line {
    let mut decoder = ClaudeDecoder::default();
    let event = decoder
        .end_of_stream(None, complaint)
        .expect("a stream without result has to end the visible turn");
    let mut curator = Curator::new();
    let mut rows = curator.observe(Seen {
        agent: "builder",
        at_ms: 0,
        event: &event,
        tool: None,
    });
    rows.extend(curator.flush());
    rows.into_iter()
        .find(|row| matches!(row, Line::Done { .. }))
        .expect("the failed outcome has to reach the terminal row a person reads")
}

fn assert_claude_row_says(row: &Line, expected: &str) {
    assert!(
        matches!(row, Line::Done { .. }),
        "the sentence has to arrive through the terminal row a person reads: {row:?}"
    );
    assert!(
        row.text().ends_with(expected),
        "the visible row must end with the selected reason {expected:?}. It showed: {:?}",
        row.text()
    );
}

fn codex_spec(cwd: &Path) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: "read the material".to_owned(),
        model: None,
        system_append: None,
        reaches_the_web: false,
        policy: Policy::ReadOnly,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

async fn codex_rows_for(complaint: &str) -> Result<Vec<Line>, Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let path = fixture.path().join("codex");
    let body = format!("#!/bin/sh\nIFS= read -r line\nprintf '%b\\n' {complaint:?} >&2\nexit 3\n");
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;

    let driver = CodexDriver::with_binary(path);
    let (events, mut received) = mpsc::channel(64);
    let mut handle = tokio::time::timeout(
        WAIT,
        driver.start_session(codex_spec(fixture.path()), events),
    )
    .await??;
    let _outcome = tokio::time::timeout(WAIT, handle.wait()).await??;
    drop(handle);

    let mut decoded = Vec::new();
    while let Ok(event) = received.try_recv() {
        decoded.push(event);
    }
    let mut curator = Curator::new();
    let mut rows = Vec::new();
    for (at_ms, event) in decoded.iter().enumerate() {
        rows.extend(curator.observe(Seen {
            agent: "builder",
            at_ms: u64::try_from(at_ms).unwrap_or_default(),
            event: &event.event,
            tool: None,
        }));
    }
    rows.extend(curator.flush());
    Ok(rows)
}

#[test]
fn noise_before_the_reason_still_says_what_to_do() {
    let complaint = "shell-init: error retrieving current directory: getcwd: cannot access \
                     parent directories: Operation not permitted\nFailed to authenticate: OAuth \
                     session expired and could not be refreshed";
    let row = claude_row_for(complaint);
    let expected = format!("{CLAUDE_PREFIX}{SIGN_IN}");

    assert_claude_row_says(&row, &expected);
    assert!(
        !row.text().contains("getcwd")
            && !row.text().contains("OAuth")
            && !row.text().contains("401"),
        "noise and vendor jargon must not survive in the sentence a person sees: {row:?}",
    );
}

#[test]
fn the_same_sentence_when_the_reason_came_first() {
    let complaint = "Failed to authenticate: OAuth session expired and could not be \
                     refreshed\nshell-init: error retrieving current directory: getcwd failed";
    let row = claude_row_for(complaint);

    assert_claude_row_says(&row, &format!("{CLAUDE_PREFIX}{SIGN_IN}"));
}

#[test]
fn a_complaint_we_do_not_know_keeps_its_first_line() {
    let first = "the agent could not reach the model after error 4010";
    let row = claude_row_for(&format!("{first}\na later diagnostic detail"));

    assert_claude_row_says(&row, &format!("{CLAUDE_PREFIX}{first}"));
}

#[test]
fn a_stack_tail_still_loses_to_the_first_line() {
    let first = "the provider closed the connection unexpectedly";
    let row = claude_row_for(&format!(
        "{first}\n    at Session.run (client.js:88:14)\n    at async main (cli.js:12:3)"
    ));

    assert_claude_row_says(&row, &format!("{CLAUDE_PREFIX}{first}"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn codex_says_the_same_thing_about_the_same_complaint() -> Result<(), Box<dyn Error>> {
    let rows = codex_rows_for(
        "a setup warning that does not explain the failure\nPlease run /login before continuing",
    )
    .await?;
    let expected = format!("{CODEX_PREFIX}{SIGN_IN}");

    assert!(
        rows.iter()
            .any(|row| matches!(row, Line::Problem { text, .. } if text == &expected)),
        "the Codex adapter must send the same advice through the one row a person reads: \
         {rows:?}"
    );
    Ok(())
}
