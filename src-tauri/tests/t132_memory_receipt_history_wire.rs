//! T-132 AC-2: historia przypina zamrozony receipt do fizycznego UUID kroku.
//!
//! Fixture jest przyszlym, addytywnym JSON-em zapisanym literalnie. Spec nie uzywa pisarza
//! biegu jako wlasnej wyroczni i nie importuje przyszlego typu receipt: obecne
//! `read_run_inner` otwiera plik, a brakujace zachowanie ujawnia dopiero asercja na drucie.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use loadout_lib::commands::history::read_run_inner;
use loadout_lib::store::Store;
use serde_json::{Value, json};

const RUN: &str = "20260826-132000__019b0132-0000-7000-8000-000000000132";
const STEP_A: &str = "019b0132-0000-7000-8000-00000000013a";
const STEP_B: &str = "019b0132-0000-7000-8000-00000000013b";
const OPAQUE_ORIGIN: &str = "019b0131-aaaa-7bbb-8ccc-0123456789ab";

fn run_dir(project: &Path, folder: &str) -> PathBuf {
    project.join(".loadout").join("runs").join(folder)
}

fn step(id: &str, node_key: &str) -> Value {
    json!({
        "id": id,
        "node_key": node_key,
        "name": "Repeated worker",
        "agent": "Receipt agent",
        "kind": "agent",
        "depends_on": [],
        "status": "succeeded",
        "attempt": 0,
        "cost_usd": null,
        "summary": null,
        "error": null,
        "effective": { "runsWith": "claude-code" }
    })
}

fn write_run(
    project: &Path,
    folder: &str,
    memory: Option<Value>,
) -> Result<PathBuf, Box<dyn Error>> {
    let dir = run_dir(project, folder);
    fs::create_dir_all(dir.join("logs"))?;
    let mut description = json!({
        "id": "019b0132-0000-7000-8000-000000000132",
        "workflow_id": "receipt-history.json",
        "workflow_hash": "feedfacefeedface",
        "workflow_snapshot": { "format": 1 },
        "title": "Receipt history fixture",
        "status": "succeeded",
        "concurrency": 2,
        "created_at": 1_787_732_400_000_i64,
        "started_at": 1_787_732_400_100_i64,
        "ended_at": 1_787_732_400_200_i64,
        "error": null,
        "steps": [step(STEP_A, "worker#1"), step(STEP_B, "worker#2")],
        "futureTopLevelKey": { "kept": true }
    });
    if let Some(memory) = memory {
        description
            .as_object_mut()
            .ok_or("the literal description stopped being an object")?
            .insert("memory".to_owned(), memory);
    }
    fs::write(
        dir.join("run.json"),
        serde_json::to_vec_pretty(&description)?,
    )?;
    Ok(dir)
}

fn future_memory() -> Value {
    json!([
        {
            "reference": "memory/notes/imported.md",
            "hash": "1111222233334444",
            "bytes": 41,
            "address": { "place": "library", "id": "imported" },
            "project": OPAQUE_ORIGIN,
            "from": null,
            "recipients": [STEP_A],
            "leftOutFor": [],
            "futureRecordKey": "ignored"
        },
        {
            "reference": ".loadout/memory/notes/suggested.md",
            "hash": "aaaabbbbccccdddd",
            "bytes": 73,
            "address": { "place": "project", "id": "suggested" },
            "project": null,
            "from": OPAQUE_ORIGIN,
            "recipients": [],
            "leftOutFor": [STEP_B]
        },
        {
            "reference": "memory/notes/nobody.md",
            "hash": "99990000aaaabbbb",
            "bytes": 17,
            "address": { "place": "library", "id": "nobody" },
            "project": null,
            "from": null,
            "recipients": ["019b0132-0000-7000-8000-00000000013c"],
            "leftOutFor": []
        }
    ])
}

fn memory_of<'a>(wire: &'a Value, id: &str) -> Result<Option<&'a Value>, Box<dyn Error>> {
    let step = wire
        .get("steps")
        .and_then(Value::as_array)
        .ok_or("history wire has no steps")?
        .iter()
        .find(|step| step.get("id").and_then(Value::as_str) == Some(id))
        .ok_or_else(|| format!("history wire has no step {id}"))?;
    Ok(step.get("memory"))
}

#[test]
fn additive_and_legacy_files_are_readable_before_the_new_wire_is_inspected()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let legacy = "20260826-131800__019b0132-0000-7000-8000-000000000128";
    let no_lists = "20260826-131900__019b0132-0000-7000-8000-000000000129";

    write_run(root.path(), legacy, None)?;
    write_run(
        root.path(),
        no_lists,
        Some(json!([{
            "reference": "memory/notes/old.md",
            "hash": "0123456789abcdef",
            "bytes": 12,
            "unknownLegacyKey": true
        }])),
    )?;

    for folder in [legacy, no_lists] {
        let opened = read_run_inner(root.path(), folder)?;
        assert_eq!(
            opened.steps.len(),
            2,
            "{folder} lost its two physical steps"
        );
        assert_eq!(opened.steps[0].id, STEP_A);
        assert_eq!(opened.steps[1].id, STEP_B);
    }
    Ok(())
}

#[test]
fn legacy_receipts_reach_each_step_as_an_empty_memory_list() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let legacy = "20260826-131800__019b0132-0000-7000-8000-000000000128";
    let no_lists = "20260826-131900__019b0132-0000-7000-8000-000000000129";

    write_run(root.path(), legacy, None)?;
    write_run(
        root.path(),
        no_lists,
        Some(json!([{
            "reference": "memory/notes/old.md",
            "hash": "0123456789abcdef",
            "bytes": 12,
            "unknownLegacyKey": true
        }])),
    )?;

    for folder in [legacy, no_lists] {
        let wire = serde_json::to_value(read_run_inner(root.path(), folder)?)?;
        assert_eq!(
            memory_of(&wire, STEP_A)?,
            Some(&json!([])),
            "{folder} must expose an explicit empty memory list for a legacy step"
        );
        assert_eq!(memory_of(&wire, STEP_B)?, Some(&json!([])));
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn receipt_is_filtered_by_step_uuid_and_survives_catalog_and_index_rebuilds()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let project = root.path();
    let dir = write_run(project, RUN, Some(future_memory()))?;

    // Dzisiejszy katalog celowo przeczy zamrozonemu receiptowi. Historia nie moze go otworzyc.
    let current = project.join(".loadout/memory/notes");
    fs::create_dir_all(&current)?;
    fs::write(
        current.join("imported.md"),
        "---\nrule: CURRENT CATALOG IS NOT HISTORY\nproject: newer-project\n---\n",
    )?;

    let database = project.join(".loadout/loadout.db");
    let store = Store::open(&database)?;
    store.rebuild_from(&dir).await?;
    drop(store);

    let before = serde_json::to_value(read_run_inner(project, RUN)?)?;

    fs::remove_dir_all(project.join(".loadout/memory"))?;
    fs::remove_file(&database)?;
    let rebuilt = Store::open(&database)?;
    rebuilt.rebuild_from(&dir).await?;
    drop(rebuilt);

    let after = serde_json::to_value(read_run_inner(project, RUN)?)?;
    assert_eq!(
        after, before,
        "deleting every current note and rebuilding the disposable SQLite index changed the \
         history wire; read_run_inner must get this fact only from run.json"
    );

    assert_eq!(
        memory_of(&after, STEP_A)?,
        Some(&json!([{
            "reference": "memory/notes/imported.md",
            "hash": "1111222233334444",
            "bytes": 41,
            "address": { "place": "library", "id": "imported" },
            "project": OPAQUE_ORIGIN,
            "from": null,
            "leftOut": false
        }])),
        "the first physical UUID must receive only the record delivered to that process"
    );
    assert_eq!(
        memory_of(&after, STEP_B)?,
        Some(&json!([{
            "reference": ".loadout/memory/notes/suggested.md",
            "hash": "aaaabbbbccccdddd",
            "bytes": 73,
            "address": { "place": "project", "id": "suggested" },
            "project": null,
            "from": OPAQUE_ORIGIN,
            "leftOut": true
        }])),
        "the second same-named step must receive only its own deferred record, marked as such"
    );
    Ok(())
}
