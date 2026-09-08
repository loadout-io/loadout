//! CT-06: wymagania wygrywają z indeksem i nigdy nie są skracane do ścieżki.

// 2026-09-08 (CT-06) — jeden test przechodzi cały produkcyjny Start i asertuje na
// kopercie stdin, więc ma 103 linie przy sufiicie 100. Dzielenie go rozerwałoby jedną
// narrację dowodu na dwie połówki, z których żadna nie dowodzi kryterium. W `tests/`
// ten allow niczego nie wycisza w kodzie produktu: `checks/suppressions.sh` skanuje
// `src/` i `src-tauri/src`. Sąsiad `step_receives_selected_context.rs` ma go tak samo.
#![allow(clippy::too_many_lines)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use loadout_lib::commands::{RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::AgentDriver;
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::step::StepState;
use loadout_lib::ipc::line_channel;
use loadout_lib::store::Store;
use serde_json::json;

use super::step_receives_selected_context::{
    Bench, PATIENCE, Seen, drivers, pin, refusal_of, run, step,
};

const CLAUDE_OUTPUT: &str = concat!(
    r#"{"type":"system","subtype":"init","session_id":"01990000-0000-7000-8000-000000000606","model":"sonnet","tools":[]}"#,
    "\n",
    r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"The reference material was supplied."}]}}"#,
    "\n",
    r#"{"type":"result","subtype":"success","is_error":false,"num_turns":1,"duration_ms":7,"total_cost_usd":0.001,"result":"The reference material was supplied."}"#,
    "\n",
);

const CLAUDE_FAKE: &str = r#"#!/bin/sh
if [ "${1-}" = "--version" ]; then
  printf '%s\n' '2.1.263 (Claude Code)'
  exit 0
fi
here="$(dirname "$0")"
printf '%s\n' "$@" >> "$here/context.argv.log"
IFS= read -r first_turn
printf '%s\n' "$first_turn" >> "$here/context.stdin.log"
cat "$here/context.stdout.jsonl"
exit 0
"#;

type ReferenceFiles = Vec<(PathBuf, Vec<u8>)>;

fn controlled_claude(home: &Path) -> Result<loadout_lib::commands::Drivers, Box<dyn Error>> {
    let binary = home.join("context-claude");
    fs::write(&binary, CLAUDE_FAKE)?;
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))?;
    fs::write(home.join("context.stdout.jsonl"), CLAUDE_OUTPUT)?;
    let driver: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(binary));
    Ok(Arc::new(move |_vendor| Arc::clone(&driver)))
}

fn input_manifest(run: &Path) -> Result<serde_json::Value, Box<dyn Error>> {
    fs::read_dir(run.join("logs"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".input.json"))
        })
        .filter_map(|path| fs::read(path).ok())
        .filter_map(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .find(|manifest| {
            manifest["context"].as_array().is_some_and(|sources| {
                sources
                    .iter()
                    .any(|source| source["kind"] == "referenceMaterial")
            })
        })
        .ok_or_else(|| "the real step adapter did not preserve its reference inputs".into())
}

fn reference_files(
    run: &Path,
    manifest: &serde_json::Value,
) -> Result<ReferenceFiles, Box<dyn Error>> {
    manifest["context"]
        .as_array()
        .ok_or("the real adapter input has no context list")?
        .iter()
        .filter(|source| source["kind"] == "referenceMaterial")
        .map(|source| {
            let relative = source["reference"]
                .as_str()
                .ok_or("a reference material has no relative path")?;
            let bytes = source["bytes"]
                .as_u64()
                .ok_or("a reference material has no byte count")?;
            let path = run.join(relative);
            let actual = fs::read(&path)?;
            assert_eq!(bytes, actual.len() as u64);
            Ok::<_, Box<dyn Error>>((PathBuf::from(relative), actual))
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_final_prompt_keeps_exact_requirements_before_the_short_index()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let exact = "BUDGET-REQUIREMENT: keep 17.25% exactly; never round this number.";
    let material = bench.publish(
        "Budget brief",
        "Preserve the business limits exactly.",
        "budget",
        "Budget",
        exact,
        &format!("FULL-SOURCE-CONTENT {}", "detail ".repeat(5_000)),
    )?;
    let version = loadout_lib::context::files::folder_of(
        &loadout_lib::context::files::library_root(bench.home.path()),
        &material.set_id,
    )?
    .join("versions")
    .join(&material.revision);
    let manifest = version.join("manifest.json");
    let mut revision: serde_json::Value = serde_json::from_slice(&fs::read(&manifest)?)?;
    let mut second = revision["findings"][0].clone();
    second["id"] = json!("same-words-different-condition");
    second["condition"] = json!("while exporting the receipt");
    revision["findings"]
        .as_array_mut()
        .ok_or("the ready version has no findings array")?
        .push(second);
    fs::write(&manifest, serde_json::to_vec_pretty(&revision)?)?;
    fs::write(
        version.join("findings.json"),
        serde_json::to_vec_pretty(&revision["findings"])?,
    )?;
    let workflow = bench.workflow(
        "budget-context",
        &json!({
            "format": 2,
            "id": "ct-06-budget",
            "name": "Context budget",
            "steps": [step("budget", "CTX-ALPHA apply the exact limit.", pin(&material))],
            "links": []
        }),
    )?;
    let report = run(&bench, workflow, controlled_claude(bench.home.path())?, 1).await?;
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded],
        "{:?}",
        bench.run_errors()
    );
    let envelopes = fs::read_to_string(bench.home.path().join("context.stdin.log"))?
        .lines()
        .map(serde_json::from_str::<serde_json::Value>)
        .collect::<Result<Vec<_>, _>>()?;
    let prompt = envelopes
        .iter()
        .filter_map(|envelope| {
            envelope
                .pointer("/message/content/0/text")
                .and_then(serde_json::Value::as_str)
        })
        .find(|prompt| prompt.contains("CTX-ALPHA"))
        .ok_or("the real step adapter did not receive a text prompt through stdin")?;
    let requirement_at = prompt.find(exact).ok_or("the exact requirement was lost")?;
    let index_at = prompt
        .find("Available topics and sources")
        .ok_or("the short index was not labelled")?;
    assert!(
        requirement_at < index_at,
        "the index came before important requirements"
    );
    assert_eq!(
        prompt.matches(exact).count(),
        2,
        "requirements that differ only by condition were incorrectly deduplicated"
    );
    assert!(prompt.contains("while working on Budget"));
    assert!(prompt.contains("while exporting the receipt"));
    let reference_start = prompt
        .find("## Reference materials")
        .ok_or("the final prompt has no reference-material heading")?;
    let after_heading = &prompt[reference_start + "## Reference materials".len()..];
    let reference_end = after_heading.find("\n\n## ").map_or(prompt.len(), |at| {
        reference_start + "## Reference materials".len() + at
    });
    assert!(reference_end - reference_start <= 24 * 1024);
    assert!(!fs::read_to_string(bench.home.path().join("context.argv.log"))?.contains(exact));
    let manifest = input_manifest(&report.dir)?;
    let references = reference_files(&report.dir, &manifest)?;
    assert!(
        references
            .iter()
            .any(|(_, bytes)| String::from_utf8_lossy(bytes).contains("FULL-SOURCE-CONTENT")),
        "the full source was not left available behind the actual adapter"
    );
    assert!(references.iter().all(|(path, _)| {
        !path.is_absolute()
            && path
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_)))
            && path.starts_with("context-sources")
    }));
    let saved_run = fs::read_to_string(report.dir.join("run.json"))?;
    assert!(!saved_run.contains(exact));
    assert!(!saved_run.contains("FULL-SOURCE-CONTENT"));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_oversized_requirement_refuses_start_before_the_first_process()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let oversized = format!(
        "OVERSIZED-REQUIREMENT {}",
        "must remain exact ".repeat(1_600)
    );
    let material = bench.publish(
        "Oversized brief",
        "This requirement cannot be shortened.",
        "oversized",
        "Oversized",
        &oversized,
        "The source stays available, but the requirement cannot fit.",
    )?;
    let workflow = bench.workflow(
        "oversized-context",
        &json!({
            "format": 2,
            "id": "ct-06-oversized",
            "name": "Oversized requirement",
            "steps": [step("oversized", "CTX-ALPHA preserve it.", pin(&material))],
            "links": []
        }),
    )?;
    let seen = Arc::new(Seen::default());
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: drivers(Arc::clone(&seen), None, None, None),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let (sink, _lines) = line_channel(4096);
    let refused = refusal_of(
        tokio::time::timeout(
            PATIENCE,
            loadout_lib::commands::run::run_workflow_inner(
                &deps,
                &RunRequest {
                    workflow,
                    how_many_at_once: 1,
                    task: None,
                    part: None,
                    handoffs_from: None,
                },
                sink,
            ),
        )
        .await?,
        "an oversized important-requirements block started the run",
    )?;
    let said = refused.to_string();
    assert!(
        said.contains("important requirements") && said.contains("Oversized brief"),
        "the visible refusal did not name the oversized block and set: {said}"
    );
    assert_eq!(
        seen.started.load(Ordering::SeqCst),
        0,
        "an oversized important-requirements block reached a paid process"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nine_sets_after_inheritance_refuse_before_the_first_process() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let mut shared = Vec::new();
    let mut local = Vec::new();
    for index in 0..9 {
        let material = bench.publish(
            &format!("Set {index}"),
            "Count the effective selection.",
            &format!("topic-{index}"),
            &format!("Topic {index}"),
            &format!("REQUIREMENT-{index}"),
            &format!("SOURCE-{index}"),
        )?;
        let value = json!({
            "id": material.set_id,
            "revision": material.revision,
            "topics": [material.topic_id]
        });
        if index < 5 {
            shared.push(value);
        } else {
            local.push(value);
        }
    }
    let context = json!({
        "schema": 1,
        "inheritWorkflow": true,
        "exclude": [],
        "sets": local
    });
    let workflow = bench.workflow(
        "too-many-context-sets",
        &json!({
            "format": 2,
            "id": "ct-06-too-many-sets",
            "name": "Too many context sets",
            "context": {"schema":1,"sets":shared},
            "steps": [step("many", "CTX-ALPHA must not run.", context)],
            "links": []
        }),
    )?;
    let seen = Arc::new(Seen::default());
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: drivers(Arc::clone(&seen), None, None, None),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let (sink, _lines) = line_channel(4096);
    let refused = refusal_of(
        tokio::time::timeout(
            PATIENCE,
            loadout_lib::commands::run::run_workflow_inner(
                &deps,
                &RunRequest {
                    workflow,
                    how_many_at_once: 1,
                    task: None,
                    part: None,
                    handoffs_from: None,
                },
                sink,
            ),
        )
        .await?,
        "nine effective context sets started the run",
    )?;

    assert!(refused.to_string().contains("more than eight context sets"));
    assert_eq!(seen.started.load(Ordering::SeqCst), 0);
    Ok(())
}
