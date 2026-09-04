//! Dwa różne kształty liczników vendora muszą skończyć jako jeden słownik, a ten sam fakt
//! ma dojść do zdania czytanego przez człowieka i do bezpiecznego eksportu (niezmiennik 29).

use std::fs;
use std::path::Path;

use loadout_lib::commands::diagnostics::support_report;
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::engine::drivers::AgentEvent;
use loadout_lib::engine::drivers::claude::ClaudeDecoder;
use loadout_lib::engine::drivers::codex::CodexDecoder;
use loadout_lib::engine::line::{Curator, Seen};
use serde_json::{Value, json};

const RUN: &str = "20260904-120000__z48";

#[test]
fn two_vendor_shapes_land_in_one_dictionary_and_the_step_says_its_length_per_turn()
-> Result<(), Box<dyn std::error::Error>> {
    assert_vendor_shapes_land_in_one_dictionary()?;
    let workspace = fixture_workspace()?;
    assert_saved_history_explains_context_per_turn(workspace.path())?;
    assert_report_marks_not_run_and_counts_step_handoffs(workspace.path())?;
    Ok(())
}

fn assert_vendor_shapes_land_in_one_dictionary() -> Result<(), Box<dyn std::error::Error>> {
    let mut claude = ClaudeDecoder::new();
    let claude_done = done_line(
        &claude.push(
            r#"{"type":"result","subtype":"success","is_error":false,"num_turns":154,"duration_ms":387000,"total_cost_usd":2.33,"usage":{"input_tokens":302,"cache_read_input_tokens":25863748,"cache_creation_input_tokens":41,"output_tokens":900},"result":"done"}"#,
        ),
        "Claude",
    )?;
    let mut codex = CodexDecoder::new();
    let codex_done = done_line(
        &codex.push(
            r#"{"type":"turn.completed","usage":{"input_tokens":10005177,"cached_input_tokens":9387392,"cache_write_input_tokens":43,"output_tokens":901}}"#,
        ),
        "Codex",
    )?;
    let mut stopped_claude = ClaudeDecoder::new();
    let stopped_done = done_line(
        &stopped_claude.push(
            r#"{"type":"result","subtype":"error_during_execution","is_error":true,"terminal_reason":"cancelled","num_turns":2,"duration_ms":12,"usage":{"input_tokens":8,"cache_read_input_tokens":12,"output_tokens":1},"result":"interrupted"}"#,
        ),
        "Claude",
    )?;

    assert_eq!(claude_done.get("uncachedInput"), Some(&json!(302)));
    assert_eq!(claude_done.get("cacheRead"), Some(&json!(25_863_748)));
    assert_eq!(claude_done.get("cacheWrite"), Some(&json!(41)));
    assert_eq!(claude_done.get("output"), Some(&json!(900)));
    assert_eq!(claude_done.get("vendorTurns"), Some(&json!(154)));
    assert_eq!(
        claude_done.get("text"),
        Some(&json!(
            "Done · 154 turns · 168k length per turn on average · 6m 27s · $2.33"
        )),
        "the visible step card did not carry the requested per-turn sentence"
    );
    assert_eq!(codex_done.get("uncachedInput"), Some(&json!(617_785)));
    assert_eq!(codex_done.get("cacheRead"), Some(&json!(9_387_392)));
    assert_eq!(codex_done.get("cacheWrite"), Some(&json!(43)));
    assert_eq!(codex_done.get("output"), Some(&json!(901)));
    assert_eq!(codex_done.get("vendorTurns"), Some(&Value::Null));
    assert!(
        codex_done
            .get("text")
            .and_then(Value::as_str)
            .is_some_and(|text| !text.contains("turn")),
        "Codex must not turn one Loadout invocation into a made-up vendor turn"
    );
    assert!(
        stopped_done
            .get("text")
            .and_then(Value::as_str)
            .is_some_and(|text| {
                text.contains("Stopped · 2 turns · 10 length per turn on average")
            }),
        "a stopped Claude step must keep the counters it reported before cancellation"
    );
    for old in ["inputTokens", "cachedTokens", "outputTokens", "turns"] {
        assert!(
            claude_done.get(old).is_none() && codex_done.get(old).is_none(),
            "the old wire key {old} still escaped one of the vendor adapters"
        );
    }
    Ok(())
}

fn fixture_workspace() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let workspace = tempfile::tempdir()?;
    let run = workspace.path().join(".loadout/runs").join(RUN);
    fs::create_dir_all(run.join("handoffs"))?;
    fs::write(
        run.join("run.json"),
        serde_json::to_vec_pretty(&fixture_run())?,
    )?;
    fs::write(run.join("handoffs/01__build__findings.md"), b"safe fixture")?;
    Ok(workspace)
}

fn fixture_run() -> Value {
    json!({
        "id": "z48",
        "title": "Count one way",
        "status": "succeeded",
        "steps": [
            {
                "id": "claude-step",
                "node_key": "build",
                "name": "Build",
                "agent": "claude",
                "kind": "agent",
                "status": "succeeded",
                "executed": true,
                "uncached_input": 302,
                "cache_read": 25_863_748,
                "cache_write": 41,
                "output": 900,
                "vendor_turns": 154
            },
            {
                "id": "handoff-step",
                "node_key": "handoff",
                "name": "Hand off",
                "agent": "claude",
                "kind": "agent",
                "status": "succeeded",
                "executed": true
            },
            {
                "id": "loop-copy",
                "node_key": "build#2",
                "name": "Build",
                "agent": "claude",
                "kind": "agent",
                "status": "succeeded",
                "executed": false,
                "not_run_because": "loop settled at try 1"
            },
            {
                "id": "old-claude-step",
                "node_key": "old-build",
                "name": "Old build",
                "agent": "01990000-0000-7000-8000-000000000048",
                "kind": "agent",
                "status": "succeeded",
                "executed": true,
                "input_tokens": 302,
                "cached_tokens": 25_863_748,
                "output_tokens": 900,
                "turns": 154
            },
            {
                "id": "old-codex-step",
                "node_key": "old-code",
                "name": "Old code",
                "agent": "01990000-0000-7000-8000-000000000049",
                "kind": "agent",
                "status": "succeeded",
                "executed": true,
                "input_tokens": 10_005_177,
                "cached_tokens": 9_387_392,
                "output_tokens": 901,
                "turns": 1,
                "effective": { "runsWith": "codex" }
            }
        ]
    })
}

fn assert_saved_history_explains_context_per_turn(
    workspace: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let history = serde_json::to_value(read_run_inner(workspace, RUN)?)?;
    assert_eq!(
        history.pointer("/steps/0/contextPerTurn"),
        Some(&json!("154 turns · 168k length per turn on average")),
        "the sentence a person reads in saved history did not explain the context per turn"
    );
    assert_eq!(
        history.pointer("/steps/3/contextPerTurn"),
        Some(&json!("154 turns · 168k length per turn on average")),
        "a run.json written before Z-48 lost its context sentence instead of using aliases"
    );
    assert_eq!(
        history.pointer("/steps/4/contextPerTurn"),
        Some(&Value::Null),
        "an old Codex Loadout turn was mistaken for a vendor-reported internal turn"
    );
    Ok(())
}

fn assert_report_marks_not_run_and_counts_step_handoffs(
    workspace: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let report: Value = serde_json::from_str(support_report(workspace)?.text())?;
    assert_eq!(report.get("schemaVersion"), Some(&json!(3)));
    assert_eq!(
        report.pointer("/runs/0/steps/0/vendorTurns"),
        Some(&json!(154))
    );
    assert_eq!(
        report.pointer("/runs/0/steps/0/uncachedInput"),
        Some(&json!(302))
    );
    assert_eq!(
        report.pointer("/runs/0/steps/0/cacheRead"),
        Some(&json!(25_863_748))
    );
    assert_eq!(
        report.pointer("/runs/0/steps/0/cacheWrite"),
        Some(&json!(41))
    );
    assert_eq!(report.pointer("/runs/0/steps/0/output"), Some(&json!(900)));
    assert_eq!(
        report.pointer("/runs/0/steps/3/vendorTurns"),
        Some(&json!(154)),
        "an old Claude agent id without an effective snapshot lost its vendor turns"
    );
    assert_eq!(
        report.pointer("/runs/0/steps/2/state"),
        Some(&json!("notRun"))
    );
    assert_eq!(
        report.pointer("/runs/0/steps/2/reason"),
        Some(&json!("loop settled at try 1"))
    );
    assert_eq!(
        report.pointer("/runs/0/steps/1/artifacts/handoffs/total"),
        Some(&json!(1)),
        "the report counted handoffs for the run but not for the step that wrote one"
    );
    assert_eq!(
        report.pointer("/runs/0/steps/4/uncachedInput"),
        Some(&json!(617_785)),
        "the old Codex alias kept cache inside uncachedInput"
    );
    assert_eq!(
        report.pointer("/runs/0/steps/4/cacheRead"),
        Some(&json!(9_387_392))
    );
    assert!(report.pointer("/runs/0/steps/4/vendorTurns").is_none());
    Ok(())
}

fn done_line(events: &[AgentEvent], agent: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let mut curator = Curator::new();
    for event in events {
        for line in curator.observe(Seen {
            agent,
            at_ms: 0,
            event,
            tool: None,
        }) {
            let value = serde_json::to_value(line)?;
            if value.get("kind").and_then(Value::as_str) == Some("done") {
                return Ok(value);
            }
        }
    }
    Err("the vendor result did not reach the visible done line".into())
}
