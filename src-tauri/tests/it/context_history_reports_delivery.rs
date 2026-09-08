//! CT-08: historia i raport wsparcia rozróżniają dostarczenie bez ujawniania treści.

#![allow(
    clippy::expect_used,
    clippy::too_many_lines,
    reason = "CT-08 keeps the real delivery path and its private fixture assertions together"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use loadout_lib::bridge::host::Answers;
use loadout_lib::bridge::{Answer, Call};
use loadout_lib::commands::context_sources::read_bound;
use loadout_lib::commands::diagnostics::support_report;
use loadout_lib::commands::history::read_run_inner;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use tokio_util::sync::CancellationToken;

use super::recorded_replay_uses_frozen_context::capturing_drivers;
use super::step_receives_selected_context::{Bench, run, step};

const PRIVATE_IMAGE: &[u8] = b"PRIVATE-CONTEXT-IMAGE-CT08";
const PRIVATE_TEXT: &[u8] = b"PRIVATE-CONTEXT-TEXT--CT08";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn available_stays_available_and_diagnostics_keep_only_counts() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let mut configured = step("history", "CTX-HISTORY records delivery.", Value::Null);
    configured
        .as_object_mut()
        .ok_or("the fixture step is not an object")?
        .remove("context");
    let workflow = bench.workflow(
        "context-history",
        &json!({
            "format": 2,
            "id": "ct-08-context-history",
            "name": "Context history",
            "steps": [configured],
            "links": []
        }),
    )?;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let available_report = run(&bench, workflow, capturing_drivers(seen), 1).await?;
    let _package_files = install_image_package(&available_report.dir)?;
    let folder = available_report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the run folder has no portable name")?;

    let available = delivery(&read_run_inner(bench.project.path(), folder)?)?;
    assert!(available.contains("was available to read"), "{available}");
    assert!(!available.contains("was opened"), "{available}");

    write_opened(&available_report.dir, 0)?;
    let empty = delivery(&read_run_inner(bench.project.path(), folder)?)?;
    assert!(empty.contains("was available to read"), "{empty}");
    assert!(!empty.contains("was opened"), "{empty}");

    let diagnostics = support_report(bench.project.path())?;
    let document: Value = serde_json::from_str(diagnostics.text())?;
    let counts = document["runs"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|run| run["steps"].as_array().into_iter().flatten())
        .filter_map(|step| step.get("referenceMaterials"))
        .collect::<Vec<_>>();
    let available_counts = json!([1, 1, 0]);
    assert!(
        counts.contains(&&available_counts),
        "diagnostic counts were {counts:?}"
    );
    assert!(!diagnostics.text().contains("PRIVATE-CONTEXT-IMAGE-CT08"));
    assert!(!diagnostics.text().contains("PRIVATE-CONTEXT-TEXT--CT08"));
    assert!(!diagnostics.text().contains("Reference picture"));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_image_is_opened_only_after_the_real_tool_returns_it() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let mut configured = step(
        "history",
        "CTX-IMAGE-HISTORY returns the picture.",
        Value::Null,
    );
    configured
        .as_object_mut()
        .ok_or("the fixture step is not an object")?
        .remove("context");
    let workflow = bench.workflow(
        "context-image-history",
        &json!({
            "format": 2,
            "id": "ct-08-context-image-history",
            "name": "Context image history",
            "steps": [configured],
            "links": []
        }),
    )?;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let opened_report = run(&bench, workflow, capturing_drivers(seen), 1).await?;
    let _package_files = install_image_package(&opened_report.dir)?;
    let snapshot = read_bound(&opened_report.dir)?
        .ok_or("the saved package did not retain its reference materials")?;
    let desk = snapshot
        .recording_desk_for(&opened_report.dir, "history", CancellationToken::new())?
        .ok_or("the saved package did not produce its recording context desk")?;
    let listed = match desk
        .answer(Call {
            id: json!(808),
            call: "list_context".to_owned(),
            input: json!({}),
        })
        .await
    {
        Answer::Ok(value) => value,
        other => return Err(format!("the real context desk listed {other:?}").into()),
    };
    let item = listed["items"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|item| item["hasImage"] == json!(true))
        .and_then(|item| item["id"].as_str())
        .map(str::to_owned)
        .ok_or_else(|| format!("the real context desk listed no image: {listed}"))?;
    let returned = desk
        .answer(Call {
            id: json!(809),
            call: "view_context_image".to_owned(),
            input: json!({"id": item}),
        })
        .await;
    let (data, mime) = match returned {
        Answer::Image { data, mime } => (data, mime),
        other => return Err(format!("the real context desk returned {other:?}").into()),
    };
    assert_eq!(mime, "image/png");
    let returned = base64::engine::general_purpose::STANDARD
        .decode(data)?
        .len();
    assert_eq!(returned, PRIVATE_IMAGE.len());
    let opened_folder = opened_report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the image run folder has no portable name")?;
    let opened = delivery(&read_run_inner(bench.project.path(), opened_folder)?)?;
    assert!(
        opened.contains(&format!("was opened ({returned} bytes returned)")),
        "{opened}"
    );

    let diagnostics = support_report(bench.project.path())?;
    let document: Value = serde_json::from_str(diagnostics.text())?;
    let counts = document["runs"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|run| run["steps"].as_array().into_iter().flatten())
        .filter_map(|step| step.get("referenceMaterials"))
        .collect::<Vec<_>>();
    let opened_counts = json!([1, 1, 1]);
    assert!(
        counts.contains(&&opened_counts),
        "diagnostic counts were {counts:?}"
    );
    assert!(!diagnostics.text().contains("Reference picture"));
    assert!(!diagnostics.text().contains("PRIVATE-CONTEXT-IMAGE-CT08"));
    Ok(())
}

fn delivery(run: &loadout_lib::commands::history::PastRunWire) -> Result<String, Box<dyn Error>> {
    run.steps
        .first()
        .and_then(|step| step.reference_materials.as_ref())
        .and_then(|lines| lines.first())
        .cloned()
        .ok_or_else(|| "the visible history has no reference-material row".into())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Address {
    set_id: String,
    version: String,
    item_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredFile {
    relative: String,
    digest: String,
    bytes: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    address: Address,
    source_id: String,
    name: String,
    description: String,
    kind: &'static str,
    page: Option<u32>,
    pages_total: Option<u32>,
    text: Option<StoredFile>,
    preview: Option<StoredFile>,
    agent_image: Option<StoredFile>,
    run_only: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Delivery {
    set_name: String,
    version: String,
    item: String,
    kind: String,
    bytes: usize,
    state: &'static str,
    run_only: bool,
    address: Option<Address>,
    reference: String,
}

#[derive(Serialize)]
struct Block {
    required: String,
    optional: String,
    required_name: Option<String>,
}

#[derive(Serialize)]
struct Manifest {
    schema: u32,
    id: String,
    items: Vec<Item>,
    nodes: BTreeMap<String, Vec<Address>>,
    delivery: BTreeMap<String, Vec<Delivery>>,
    blocks: BTreeMap<String, Block>,
}

pub(super) fn install_image_package(
    run_dir: &Path,
) -> Result<[std::path::PathBuf; 2], Box<dyn Error>> {
    let package_id = uuid::Uuid::now_v7().to_string();
    let address = Address {
        set_id: "set-history".to_owned(),
        version: "revision-history".to_owned(),
        item_id: "source--image".to_owned(),
    };
    let text_relative = "context-sources/set-history/revision-history/source--image/text";
    let image_relative =
        "context-sources/set-history/revision-history/source--image/for-the-agent.png";
    let text = StoredFile {
        relative: text_relative.to_owned(),
        digest: format!("{:x}", Sha256::digest(PRIVATE_TEXT)),
        bytes: PRIVATE_TEXT.len(),
    };
    let image = StoredFile {
        relative: image_relative.to_owned(),
        digest: format!("{:x}", Sha256::digest(PRIVATE_IMAGE)),
        bytes: PRIVATE_IMAGE.len(),
    };
    let item = Item {
        address: address.clone(),
        source_id: "image".to_owned(),
        name: "Reference picture".to_owned(),
        description: "Private description".to_owned(),
        kind: "image",
        page: None,
        pages_total: None,
        text: Some(text),
        preview: None,
        agent_image: Some(image),
        run_only: false,
    };
    let delivery = Delivery {
        set_name: "History set".to_owned(),
        version: "revision-history".to_owned(),
        item: "Reference picture".to_owned(),
        kind: "image".to_owned(),
        bytes: PRIVATE_IMAGE.len() + PRIVATE_TEXT.len(),
        state: "available",
        run_only: false,
        address: Some(address.clone()),
        reference: text_relative.to_owned(),
    };
    let mut nodes = BTreeMap::new();
    nodes.insert("history".to_owned(), vec![address]);
    let mut deliveries = BTreeMap::new();
    deliveries.insert("history".to_owned(), vec![delivery]);
    let mut blocks = BTreeMap::new();
    blocks.insert(
        "history".to_owned(),
        Block {
            required: String::new(),
            optional: String::new(),
            required_name: None,
        },
    );
    let manifest = Manifest {
        schema: 1,
        id: package_id.clone(),
        items: vec![item],
        nodes,
        delivery: deliveries,
        blocks,
    };
    let bytes = serde_json::to_vec(&manifest)?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let text_path = run_dir.join(text_relative);
    let image_path = run_dir.join(image_relative);
    write_private(&text_path, PRIVATE_TEXT)?;
    write_private(&image_path, PRIVATE_IMAGE)?;
    write_private(&run_dir.join("context-sources/manifest.json"), &bytes)?;
    fs::create_dir_all(run_dir.join("context-sources/reads"))?;
    fs::set_permissions(
        run_dir.join("context-sources/reads"),
        fs::Permissions::from_mode(0o700),
    )?;
    let mut run: Value = serde_json::from_slice(&fs::read(run_dir.join("run.json"))?)?;
    run["context_sources"] = json!({"id": package_id, "digest": digest});
    fs::write(run_dir.join("run.json"), serde_json::to_vec_pretty(&run)?)?;
    Ok([text_path, image_path])
}

fn write_opened(run_dir: &Path, bytes: usize) -> Result<(), Box<dyn Error>> {
    write_private(
        &run_dir.join("context-sources/reads/history.jsonl"),
        format!(
            "{{\"schema\":1,\"address\":{{\"setId\":\"set-history\",\"version\":\"revision-history\",\"itemId\":\"source--image\"}},\"bytes\":{bytes}}}\n"
        )
        .as_bytes(),
    )
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    fs::write(path, bytes)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}
