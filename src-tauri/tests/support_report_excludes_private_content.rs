//! T-34 AC-3: support data is rebuilt from a closed allowlist, never by redacting private files.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::commands::diagnostics::{DiagnosticsError, copy_diagnostics_with, support_report};
use serde_json::{Value, json};

const SAFE_RUN: &str = "run-0199-safe";
const SAFE_STEP: &str = "step-0199-safe";
const SAFE_CONVERSATION: &str = "conversation-0199-safe";
const PRIVATE: [&str; 12] = [
    "PRIVATE_PROMPT_T34",
    "PRIVATE_HANDOFF_T34",
    "PRIVATE_STDOUT_T34",
    "PRIVATE_STDERR_T34",
    "PRIVATE_IMAGE_T34",
    "PRIVATE_ENV_T34",
    "PRIVATE_ARGV_T34",
    "PRIVATE_VENDOR_SESSION_T34",
    "PRIVATE_TRIGGER_KEY_T34",
    "PRIVATE_ERROR_T34",
    "PRIVATE_MODEL_T34",
    "PRIVATE_NAME_T34",
];

const ALLOWED_KEYS: [&str; 36] = [
    "agentTurns",
    "artifacts",
    "attempts",
    "cachedTokens",
    "complete",
    "conversations",
    "counts",
    "createdAt",
    "deathProof",
    /* 2026-08-30: dwa boole `ExecutionFacts` z T-207. Wchodzą na tę listę JAWNIE, bo lista jest
     * zamknięta z premedytacją — bez tego wiersza krok, który się nie wykonał, czytał się
     * w zrzucie identycznie jak krok, który padł bez śladu. Zdania `summary` tu nie ma i nie
     * ma go w zrzucie: pisze je agent, a ten plik człowiek wkleja obcym. */
    "executed",
    "processStarted",
    "endedAt",
    "exitCode",
    "failureKind",
    "handoffs",
    "id",
    "inputManifest",
    "inputTokens",
    "kind",
    "model",
    "modelConfigured",
    "outputTokens",
    "present",
    "receipt",
    "runs",
    "schemaVersion",
    "startedAt",
    "state",
    "stderr",
    "stdout",
    "steps",
    "total",
    "turnFiles",
    "turns",
    "vendor",
    "workspace",
];

fn write(path: &Path, bytes: impl AsRef<[u8]>) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    Ok(())
}

fn collect_keys(value: &Value, keys: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                keys.insert(key.clone());
                collect_keys(value, keys);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_keys(value, keys);
            }
        }
        _ => {}
    }
}

fn seed_active_run(active: &Path) -> Result<(), Box<dyn Error>> {
    let run = active.join(".loadout/runs").join(SAFE_RUN);
    write(
        &run.join("run.json"),
        serde_json::to_vec_pretty(&json!({
            "id": "PRIVATE_VENDOR_SESSION_T34",
            "name": "PRIVATE_NAME_T34",
            "model": "PRIVATE_MODEL_T34",
            "workflow_id": "wf-safe",
            "workflow_snapshot": {"format": 1, "id": "wf-safe", "steps": [], "links": []},
            "title": "PRIVATE_NAME_T34",
            "status": "running",
            "concurrency": 1,
            "created_at": 1_777_777_777_000_i64,
            "started_at": 1_777_777_777_001_i64,
            "ended_at": null,
            "prompt": "PRIVATE_PROMPT_T34",
            "argv": ["PRIVATE_ARGV_T34"],
            "env": {"TOKEN": "PRIVATE_ENV_T34"},
            "triggerKey": "PRIVATE_TRIGGER_KEY_T34",
            "error": "PRIVATE_ERROR_T34",
            "steps": [{
                "id": SAFE_STEP, "node_key": "safe-node", "name": "PRIVATE_NAME_T34",
                "agent": "codex", "depends_on": [], "status": "running", "attempt": 1,
                "agent_session_id": "PRIVATE_VENDOR_SESSION_T34",
                "effective": {"model": "PRIVATE_MODEL_T34"}, "turns": 1,
                "inputTokens": 13, "outputTokens": 8, "cachedTokens": 3, "exit_code": 17,
                "started_at": 1_777_777_777_002_i64, "ended_at": null
            }]
        }))?,
    )?;
    write(
        &run.join(format!("logs/agent-{SAFE_STEP}.jsonl")),
        [b"PRIVATE_STDOUT_T34".as_slice(), &[0xff, 0xfe]].concat(),
    )?;
    write(
        &run.join(format!("logs/agent-{SAFE_STEP}.stderr.log")),
        b"PRIVATE_STDERR_T34",
    )?;
    write(
        &run.join(format!("logs/agent-{SAFE_STEP}.input.json")),
        br#"{"promptBytes":34,"private":"PRIVATE_PROMPT_T34"}"#,
    )?;
    write(&run.join("handoffs/result.md"), b"PRIVATE_HANDOFF_T34")?;
    write(&run.join("private-image.png"), b"PRIVATE_IMAGE_T34")?;
    Ok(())
}

fn seed_failed_runs(active: &Path) -> Result<(), Box<dyn Error>> {
    let failed = active.join(".loadout/runs/run-0199-failed");
    write(
        &failed.join("run.json"),
        serde_json::to_vec_pretty(&json!({
            "status": "failed", "created_at": 1_777_777_777_100_i64,
            "started_at": 1_777_777_777_101_i64, "ended_at": 1_777_777_777_102_i64,
            "error": "PRIVATE_ERROR_T34", "steps": [{
                "id": "step-0199-failed", "status": "failed", "agent": "claude",
                "exit_code": 23, "death_proof": true, "error": "PRIVATE_ERROR_T34"
            }]
        }))?,
    )?;
    for suffix in ["jsonl", "stderr.log", "input.json"] {
        write(
            &failed.join(format!("logs/agent-step-0199-failed.{suffix}")),
            b"PRIVATE_ERROR_T34",
        )?;
    }
    let check = active.join(".loadout/runs/run-0199-check");
    write(
        &check.join("run.json"),
        serde_json::to_vec_pretty(&json!({
            "status": "failed", "created_at": 1_777_777_777_200_i64,
            "started_at": 1_777_777_777_201_i64, "ended_at": 1_777_777_777_202_i64,
            "steps": [{"id": "step-0199-check", "kind": "check", "status": "failed",
                "agent": "", "exit_code": 9, "error": "PRIVATE_ERROR_T34"}]
        }))?,
    )?;
    Ok(())
}

fn seed_conversations(active: &Path) -> Result<(), Box<dyn Error>> {
    let conversation = active
        .join(".loadout/conversations")
        .join(SAFE_CONVERSATION);
    write(
        &conversation.join("conversation.json"),
        serde_json::to_vec_pretty(&json!({
            "schemaVersion": 1, "id": "PRIVATE_VENDOR_SESSION_T34", "vendor": "codex",
            "modelConfigured": true, "model": "PRIVATE_MODEL_T34",
            "vendorSessionId": "PRIVATE_VENDOR_SESSION_T34", "state": "failed",
            "complete": true, "createdAt": 1_777_777_777_300_i64,
            "startedAt": 1_777_777_777_301_i64, "endedAt": 1_777_777_777_302_i64,
            "attempts": 2, "turns": 1, "failureKind": "deliveryFailed",
            "error": "PRIVATE_ERROR_T34", "exitCode": 17, "deathProof": true,
            "agentTurns": 1, "inputTokens": 21, "outputTokens": 13, "cachedTokens": 8
        }))?,
    )?;
    write(
        &conversation.join("turns/0001.json"),
        br#"{"text":"PRIVATE_PROMPT_T34","images":["PRIVATE_IMAGE_T34"]}"#,
    )?;
    write(&conversation.join("logs/lead.jsonl"), b"PRIVATE_STDOUT_T34")?;
    let live = active.join(".loadout/conversations/conversation-0199-active");
    write(
        &live.join("conversation.json"),
        serde_json::to_vec_pretty(&json!({
            "schemaVersion": 1, "vendor": "claude", "modelConfigured": false,
            "state": "active", "complete": false, "createdAt": 1_777_777_777_400_i64,
            "startedAt": 1_777_777_777_401_i64, "attempts": 1, "turns": 1,
            "agentTurns": 0, "inputTokens": 0, "outputTokens": 0, "cachedTokens": 0,
            "deathProof": false, "prompt": "PRIVATE_PROMPT_T34"
        }))?,
    )?;
    Ok(())
}

fn seed_neighbor(active: &Path, neighbor: &Path) -> Result<(), Box<dyn Error>> {
    let run = neighbor.join(".loadout/runs/neighbor-private");
    write(
        &run.join("run.json"),
        br#"{"id":"PRIVATE_NEIGHBOR_T34","state":"done"}"#,
    )?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(&run, active.join(".loadout/runs/symlink-escape"))?;
    Ok(())
}

#[test]
fn report_is_active_workspace_only_and_contains_no_private_bytes() -> Result<(), Box<dyn Error>> {
    let active = tempfile::tempdir()?;
    let neighbor = tempfile::tempdir()?;
    seed_active_run(active.path())?;

    seed_failed_runs(active.path())?;

    seed_conversations(active.path())?;

    seed_neighbor(active.path(), neighbor.path())?;

    let report = support_report(active.path())?;
    let text = report.text();
    let document: Value = serde_json::from_str(text)?;

    assert!(
        text.contains(SAFE_RUN),
        "the report omitted the active Loadout run id"
    );
    assert!(
        text.contains(SAFE_CONVERSATION),
        "the report omitted the active Loadout conversation id"
    );
    assert!(
        text.contains(SAFE_STEP),
        "the report omitted the Loadout step id"
    );
    assert!(
        text.contains("\"complete\":false") || text.contains("\"complete\": false"),
        "an active artifact was reported as complete: {text}"
    );
    assert!(
        text.contains("stdout") && text.contains("stderr") && text.contains("inputManifest"),
        "presence flags for private artifacts are missing: {text}"
    );
    assert_run_facts(&document)?;

    assert_conversation_facts(&document)?;

    assert_report_privacy(active.path(), text, &document);
    assert_eq!(report.receipt().runs, 3);
    assert_eq!(report.receipt().conversations, 2);
    Ok(())
}

fn fact_by_id<'a>(document: &'a Value, collection: &str, id: &str) -> Option<&'a Value> {
    document
        .get(collection)
        .and_then(Value::as_array)
        .and_then(|facts| {
            facts
                .iter()
                .find(|fact| fact.get("id").and_then(Value::as_str) == Some(id))
        })
}

fn assert_run_facts(document: &Value) -> Result<(), Box<dyn Error>> {
    let run = fact_by_id(document, "runs", SAFE_RUN).ok_or("the safe run has no report entry")?;
    assert_eq!(run.get("state").and_then(Value::as_str), Some("running"));
    assert_eq!(
        run.get("createdAt").and_then(Value::as_i64),
        Some(1_777_777_777_000_i64)
    );
    let step = run
        .get("steps")
        .and_then(Value::as_array)
        .and_then(|steps| steps.first())
        .ok_or("the safe step has no report entry")?;
    assert_eq!(step.get("vendor").and_then(Value::as_str), Some("codex"));
    assert_eq!(step.get("exitCode").and_then(Value::as_i64), Some(17));
    for (key, value) in [
        ("inputTokens", 13),
        ("outputTokens", 8),
        ("cachedTokens", 3),
    ] {
        assert_eq!(step.get(key).and_then(Value::as_u64), Some(value));
    }
    assert_eq!(
        step.pointer("/model/present").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        step.pointer("/deathProof/present").and_then(Value::as_bool),
        Some(false)
    );

    let failed = fact_by_id(document, "runs", "run-0199-failed")
        .ok_or("the failed run has no report entry")?;
    assert_eq!(
        failed.get("failureKind").and_then(Value::as_str),
        Some("processExit")
    );
    assert_eq!(
        failed
            .pointer("/steps/0/failureKind")
            .and_then(Value::as_str),
        Some("processExit")
    );
    assert_eq!(
        failed
            .pointer("/steps/0/deathProof/present")
            .and_then(Value::as_bool),
        Some(true)
    );
    let check = fact_by_id(document, "runs", "run-0199-check")
        .ok_or("the failed non-agent check has no report entry")?;
    assert_eq!(
        check
            .pointer("/steps/0/failureKind")
            .and_then(Value::as_str),
        Some("processExit")
    );
    assert_eq!(
        check.pointer("/steps/0/kind").and_then(Value::as_str),
        Some("check")
    );
    Ok(())
}

fn assert_conversation_facts(document: &Value) -> Result<(), Box<dyn Error>> {
    let failed = fact_by_id(document, "conversations", SAFE_CONVERSATION)
        .ok_or("the stopped Lead conversation has no report entry")?;
    assert_eq!(failed.get("vendor").and_then(Value::as_str), Some("codex"));
    assert_eq!(
        failed.get("modelConfigured").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(failed.get("state").and_then(Value::as_str), Some("failed"));
    assert_eq!(
        failed.get("failureKind").and_then(Value::as_str),
        Some("deliveryFailed")
    );
    assert_eq!(failed.get("attempts").and_then(Value::as_u64), Some(2));
    assert_eq!(failed.get("turns").and_then(Value::as_u64), Some(1));
    assert_eq!(
        failed
            .pointer("/deathProof/present")
            .and_then(Value::as_bool),
        Some(true)
    );
    for (key, value) in [
        ("inputTokens", 21),
        ("outputTokens", 13),
        ("cachedTokens", 8),
    ] {
        assert_eq!(failed.get(key).and_then(Value::as_u64), Some(value));
    }
    assert_eq!(failed.get("exitCode").and_then(Value::as_i64), Some(17));

    let active = fact_by_id(document, "conversations", "conversation-0199-active")
        .ok_or("the active Lead conversation has no report entry")?;
    assert_eq!(active.get("complete").and_then(Value::as_bool), Some(false));
    assert_eq!(active.get("state").and_then(Value::as_str), Some("active"));
    assert_eq!(
        active
            .pointer("/deathProof/present")
            .and_then(Value::as_bool),
        Some(false)
    );
    Ok(())
}

fn assert_report_privacy(active: &Path, text: &str, document: &Value) {
    assert!(!text.contains(active.to_string_lossy().as_ref()));
    assert!(!text.contains("PRIVATE_NEIGHBOR_T34"));
    for sentinel in PRIVATE {
        assert!(
            !text.contains(sentinel),
            "private sentinel {sentinel} escaped: {text}"
        );
    }
    let mut keys = BTreeSet::new();
    collect_keys(document, &mut keys);
    let allowed = ALLOWED_KEYS.into_iter().collect::<BTreeSet<_>>();
    let unexpected = keys
        .iter()
        .filter(|key| !allowed.contains(key.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        unexpected.is_empty(),
        "unexpected report keys: {unexpected:?}"
    );
}

#[test]
#[cfg(unix)]
fn report_refuses_an_intermediate_loadout_symlink() -> Result<(), Box<dyn Error>> {
    let active = tempfile::tempdir()?;
    let neighbor = tempfile::tempdir()?;
    write(
        &neighbor
            .path()
            .join(".loadout/runs/PRIVATE_NEIGHBOR_T34/run.json"),
        br#"{"status":"succeeded","steps":[]}"#,
    )?;
    std::os::unix::fs::symlink(
        neighbor.path().join(".loadout"),
        active.path().join(".loadout"),
    )?;

    assert!(
        support_report(active.path()).is_err(),
        "the active workspace followed its .loadout symlink into a neighboring workspace"
    );
    Ok(())
}

#[test]
fn rust_copies_the_report_but_returns_only_a_receipt_and_fixed_errors() -> Result<(), Box<dyn Error>>
{
    let workspace = tempfile::tempdir()?;
    let run = workspace.path().join(".loadout/runs/copy-safe");
    write(
        &run.join("run.json"),
        serde_json::to_vec_pretty(&json!({
            "id": "copy-safe",
            "workflow_id": "wf-copy",
            "workflow_snapshot": {"format": 1, "id": "wf-copy", "steps": [], "links": []},
            "title": "Copy",
            "status": "succeeded",
            "concurrency": 1,
            "created_at": 1_777_777_777_000_i64,
            "started_at": 1_777_777_777_001_i64,
            "ended_at": 1_777_777_777_002_i64,
            "error": null,
            "steps": []
        }))?,
    )?;
    write(
        &run.join("logs/private.log"),
        b"PRIVATE_CLIPBOARD_SOURCE_T34",
    )?;

    let copied = RefCell::new(String::new());
    let receipt = copy_diagnostics_with(workspace.path(), |safe| {
        copied.replace(safe.to_owned());
        Ok::<(), &'static str>(())
    })?;
    assert_eq!(receipt.runs, 1);
    assert!(
        !copied.borrow().contains("PRIVATE_CLIPBOARD_SOURCE_T34"),
        "the Rust clipboard boundary received private evidence instead of the safe report"
    );

    let refused = copy_diagnostics_with(workspace.path(), |_safe| {
        Err::<(), _>("PRIVATE_CLIPBOARD_PLUGIN_ERROR_T34")
    });
    assert_eq!(refused, Err(DiagnosticsError::Clipboard));
    let said = match refused {
        Err(error) => error.to_string(),
        Ok(_) => return Err("the rejecting clipboard unexpectedly succeeded".into()),
    };
    assert_eq!(said, "Loadout could not copy diagnostics.");
    assert!(!said.contains("PRIVATE_CLIPBOARD_PLUGIN_ERROR_T34"));

    let not_a_workspace = workspace.path().join("ordinary-file");
    fs::write(&not_a_workspace, b"PRIVATE_COLLECTOR_ERROR_T34")?;
    let refused = copy_diagnostics_with(&not_a_workspace, |_safe| Ok::<(), ()>(()));
    assert_eq!(refused, Err(DiagnosticsError::Collect));
    let said = match refused {
        Err(error) => error.to_string(),
        Ok(_) => return Err("a regular file became a workspace report".into()),
    };
    assert_eq!(said, "Loadout could not collect diagnostics.");
    Ok(())
}

/* KRYTERIUM NA JEDEN STATUS, i to nie jest drobiazg nazewniczy.
 *
 * Zgloszenie wlasciciela 2026-08-23: odczyt diagnostyczny pokazal trzy jego biegi jako
 * `"state": "unknown", "complete": false` — te same trzy, po ktorych sprzatanie wlasnie
 * posprzatalo i ktore niosly w plikach zdanie o tym, ze zginely razem z oknem. `unknown` plus
 * „niedokonczony" czyta sie jako „nie wiadomo, moze jeszcze trwa": dokladnie ten stan, ktory
 * zostal rozstrzygniety.
 *
 * `interrupted` pisze `recovery` od dawna (`RUN_INTERRUPTED`), ale pisalo je WYLACZNIE do bazy
 * biblioteki, ktorej ten odczyt nie czyta. Luka istniala i nie miala jak wyjsc, dopoki sprzatanie
 * nie zaczelo przepisywac `run.json`.
 *
 * SLABA WERSJA: sprawdzenie samego napisu. Przechodzi ja implementacja, ktora przepuszcza status
 * dalej, ale zostawia `complete: false` — czyli dalej mowi „poczekaj" o biegu, ktorego nikt nie
 * prowadzi. Oba punkty stoja nizej razem, bo razem sa jedna odpowiedzia.
 */
#[test]
fn a_run_cut_off_with_the_window_is_reported_as_over() -> Result<(), Box<dyn Error>> {
    let active = tempfile::tempdir()?;
    let cut_off = active.path().join(".loadout/runs/run-0199-interrupted");
    write(
        &cut_off.join("run.json"),
        serde_json::to_vec_pretty(&json!({
            "status": "interrupted", "created_at": 1_777_777_777_300_i64,
            "started_at": 1_777_777_777_301_i64, "ended_at": 1_777_777_777_302_i64,
            "steps": [{
                "id": "step-0199-interrupted", "status": "failed", "agent": "claude",
                "error": "Loadout closed while this step was still running."
            }]
        }))?,
    )?;

    let report = support_report(active.path())?;
    let described: Value = serde_json::from_str(report.text())?;
    let run = described["runs"]
        .as_array()
        .and_then(|all| all.first())
        .ok_or("the report carries no runs at all")?;

    assert_eq!(
        run["state"].as_str(),
        Some("interrupted"),
        "a run that was cut off with the window is reported as `unknown`. The person reading \
         this receipt cannot tell it apart from a run whose file this build cannot parse, and \
         the two call for opposite actions"
    );
    assert_eq!(
        run["complete"].as_bool(),
        Some(true),
        "the run is reported as still going. Nobody is carrying it and nobody ever will: saying \
         otherwise asks the person to wait for an answer that has already been given"
    );
    Ok(())
}
