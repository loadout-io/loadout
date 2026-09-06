//! WF-09 RED: rzeczywisty handler mostu czyta dokładny bieg z workspace rozmowy.

#![allow(clippy::panic)]
use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::bridge::host::Answers;
use loadout_lib::bridge::library::Desk;
use loadout_lib::bridge::{Answer, Call};
use serde_json::{Value, json};

const A1: &str = "01980000-0000-7000-8000-000000000001";
const A2: &str = "01980000-0000-7000-8000-000000000002";

fn recorded(
    project: &Path,
    date: &str,
    id: &str,
    title: &str,
    state: &str,
) -> Result<(), Box<dyn Error>> {
    let dir = project.join(".loadout/runs").join(format!("{date}__{id}"));
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join("run.json"),
        json!({
            "id": id, "title": title, "workflow_id": "workflow", "workflow_hash": "revision",
            "status": state, "created_at": 1777777777000_u64,
            "steps": [{"id": "step", "node_key": "step", "name": "Builder", "status": state,
                "summary": "A saved result", "error": "A saved error"}]
        })
        .to_string(),
    )?;
    Ok(())
}

async fn ask(desk: &Desk, input: Value) -> Answer {
    desk.answer(Call {
        id: json!(1),
        call: "get_run_status".to_owned(),
        input,
    })
    .await
}

fn value(answer: Answer) -> Value {
    match answer {
        Answer::Ok(value) => value,
        Answer::Refused(said) => panic!("status refused: {said}"),
    }
}

#[tokio::test]
async fn older_a1_is_not_replaced_by_a2_or_another_workspace() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let a = root.path().join("a");
    let b = root.path().join("b");
    recorded(&a, "20260904-100000", A1, "A1 exact result", "failed")?;
    recorded(&a, "20260905-100000", A2, "A2 newer result", "succeeded")?;
    recorded(&b, "20260904-100000", A1, "B private result", "succeeded")?;
    let desk = Desk::at(Some(root.path().to_path_buf()), a.clone());
    let actual = value(ask(&desk, json!({"run_id": A1})).await);
    assert_eq!(actual["run"]["runId"], A1);
    assert_eq!(actual["title"], "A1 exact result");
    assert_eq!(actual["state"], "failed");
    assert_eq!(actual["steps"][0]["error"], "A saved error");
    assert_eq!(actual["steps"][0]["state"], "failed");
    assert!(!actual.to_string().contains("B private result"));
    assert!(actual["observedAt"].is_string());
    assert_eq!(
        actual["source"]["runFolder"],
        format!("20260904-100000__{A1}")
    );
    Ok(())
}

#[tokio::test]
async fn stale_running_file_is_not_an_active_runtime_owner() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    recorded(
        project.path(),
        "20260904-100000",
        A1,
        "Abandoned",
        "running",
    )?;
    let desk = Desk::at(
        Some(project.path().to_path_buf()),
        project.path().to_path_buf(),
    );
    let actual = value(ask(&desk, json!({})).await);
    assert_eq!(actual["kind"], "noActiveRun");
    assert!(
        actual["said"]
            .as_str()
            .is_some_and(|said| said.contains("Nothing is running"))
    );
    Ok(())
}

#[tokio::test]
async fn status_rejects_workspace_and_arbitrary_path_arguments() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    recorded(project.path(), "20260904-100000", A1, "A", "succeeded")?;
    let desk = Desk::at(
        Some(project.path().to_path_buf()),
        project.path().to_path_buf(),
    );
    for input in [
        json!({"run_id": A1, "workspace": "/work/other"}),
        json!({"run_id": "../../secret"}),
    ] {
        assert!(matches!(ask(&desk, input).await, Answer::Refused(_)));
    }
    Ok(())
}
