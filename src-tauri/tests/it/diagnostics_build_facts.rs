use std::cell::RefCell;
use std::collections::BTreeSet;
use std::error::Error;
use std::fs;

use loadout_lib::commands::diagnostics::{copy_diagnostics_with, support_report};
use serde_json::Value;

const PRIVATE_SENTINEL: &str = "PRIVATE_BUILD_FACTS_SENTINEL";

fn keys(value: &Value) -> Result<BTreeSet<&str>, Box<dyn Error>> {
    Ok(value
        .as_object()
        .ok_or("the report value is not an object")?
        .keys()
        .map(String::as_str)
        .collect())
}

fn assert_empty_report(workspace: &std::path::Path, text: &str) -> Result<(), Box<dyn Error>> {
    let document: Value = serde_json::from_str(text)?;

    assert_eq!(document["schemaVersion"].as_u64(), Some(2));
    assert_eq!(
        document["appVersion"].as_str(),
        Some(env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(document["targetOs"].as_str(), Some(std::env::consts::OS));
    assert_eq!(
        document["targetArch"].as_str(),
        Some(std::env::consts::ARCH)
    );
    assert_eq!(
        keys(&document)?,
        BTreeSet::from([
            "appVersion",
            "conversations",
            "receipt",
            "runs",
            "schemaVersion",
            "targetArch",
            "targetOs",
            "workspace",
        ])
    );
    assert_eq!(keys(&document["workspace"])?, BTreeSet::from(["counts"]));
    assert_eq!(
        keys(&document["workspace"]["counts"])?,
        BTreeSet::from(["artifacts", "conversations", "runs"])
    );
    assert_eq!(
        keys(&document["receipt"])?,
        BTreeSet::from(["artifacts", "conversations", "runs"])
    );
    for pointer in [
        "/workspace/counts/runs",
        "/workspace/counts/conversations",
        "/workspace/counts/artifacts",
        "/receipt/runs",
        "/receipt/conversations",
        "/receipt/artifacts",
    ] {
        assert_eq!(document.pointer(pointer).and_then(Value::as_u64), Some(0));
    }
    assert_eq!(document["runs"].as_array().map(Vec::len), Some(0));
    assert_eq!(document["conversations"].as_array().map(Vec::len), Some(0));
    assert!(!text.contains(workspace.to_string_lossy().as_ref()));
    assert!(!text.contains(PRIVATE_SENTINEL));
    Ok(())
}

#[test]
fn support_summary_carries_build_facts_without_expanding_the_receipt() -> Result<(), Box<dyn Error>>
{
    let without_loadout = tempfile::tempdir()?;
    fs::write(
        without_loadout.path().join(PRIVATE_SENTINEL),
        PRIVATE_SENTINEL,
    )?;
    let report = support_report(without_loadout.path())?;
    assert_empty_report(without_loadout.path(), report.text())?;

    let with_empty_loadout = tempfile::tempdir()?;
    fs::create_dir(with_empty_loadout.path().join(".loadout"))?;
    fs::write(
        with_empty_loadout.path().join(PRIVATE_SENTINEL),
        PRIVATE_SENTINEL,
    )?;
    let copied = RefCell::new(String::new());
    let receipt = copy_diagnostics_with(with_empty_loadout.path(), |text| {
        copied.replace(text.to_owned());
        Ok::<(), std::convert::Infallible>(())
    })?;
    assert_empty_report(with_empty_loadout.path(), &copied.borrow())?;

    let serialized_receipt = serde_json::to_value(receipt)?;
    assert_eq!(
        keys(&serialized_receipt)?,
        BTreeSet::from(["artifacts", "conversations", "runs"])
    );
    assert_eq!(serialized_receipt["runs"].as_u64(), Some(0));
    assert_eq!(serialized_receipt["conversations"].as_u64(), Some(0));
    assert_eq!(serialized_receipt["artifacts"].as_u64(), Some(0));
    let serialized_receipt = serde_json::to_string(&serialized_receipt)?;
    for report_only_key in [
        "schemaVersion",
        "appVersion",
        "targetOs",
        "targetArch",
        "workspace",
        "receipt",
    ] {
        assert!(!serialized_receipt.contains(report_only_key));
    }
    Ok(())
}
