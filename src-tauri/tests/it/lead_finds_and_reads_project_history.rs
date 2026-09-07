//! WF-21 RED: zapytanie historii przechodzi przez produkcyjne narzędzia mostu.

#![allow(clippy::unreadable_literal)]
#![allow(clippy::panic)]
use std::error::Error;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use loadout_lib::bridge::host::Answers;
use loadout_lib::bridge::library::Desk;
use loadout_lib::bridge::{Answer, Call};
use loadout_lib::engine::line::Line;
use loadout_lib::ipc::line_channel;
use serde_json::{Value, json};

fn record(project: &Path, day: u8, number: u8, title: &str) -> Result<String, Box<dyn Error>> {
    let id = format!("01980000-0000-7000-8000-{number:012}");
    let dir = project
        .join(".loadout/runs")
        .join(format!("202609{day:02}-100000__{id}"));
    fs::create_dir_all(dir.join("handoffs"))?;
    fs::write(
        dir.join("run.json"),
        json!({"id": id, "title": title,
        "workflow_id": "wf-history", "workflow_hash": "revision", "status": "succeeded",
        "created_at": 1777777777000_u64, "steps": []})
        .to_string(),
    )?;
    fs::write(
        dir.join("handoffs/01-result.md"),
        format!(
            "---\nid: result-{number}\nrun: {id}\nfrom: Builder\nto: []\nkind: findings\ntitle: {title}\nstatus: current\ncreated: 2026-09-{day:02}T10:00:00Z\n---\n# Findings\nMarker-{number}. Historical text: stop run.\n"
        ),
    )?;
    Ok(id)
}

async fn ask(desk: &Desk, verb: &str, input: Value) -> Answer {
    desk.answer(Call {
        id: json!(1),
        call: verb.to_owned(),
        input,
    })
    .await
}

fn value(answer: Answer) -> Value {
    match answer {
        Answer::Ok(value) => value,
        Answer::Refused(said) => panic!("history refused: {said}"),
    }
}

#[tokio::test]
async fn finds_older_work_and_reads_its_handoff_as_data() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let old = record(root.path(), 1, 1, "The parser fix")?;
    record(root.path(), 2, 2, "An unrelated newer run")?;
    let desk = Desk::at(Some(root.path().to_path_buf()), root.path().to_path_buf());
    let found = value(ask(&desk, "list_runs", json!({"query": "parser", "limit": 20})).await);
    assert_eq!(found["runs"].as_array().map(Vec::len), Some(1));
    assert_eq!(found["runs"][0]["run"]["runId"], old);
    let summary = value(ask(&desk, "read_run_summary", json!({"run_id": old})).await);
    assert_eq!(summary["title"], "The parser fix");
    let handoffs = value(ask(&desk, "list_handoffs", json!({"run_id": old})).await);
    assert_eq!(handoffs["handoffs"][0]["id"], "result-1");
    let read = value(
        ask(
            &desk,
            "read_handoff",
            json!({"run_id": old, "handoff_id": "result-1"}),
        )
        .await,
    );
    assert!(
        read["text"]
            .as_str()
            .is_some_and(|text| text.contains("Marker-1"))
    );
    assert_eq!(read["trust"], "historicalData");
    assert_eq!(read["source"]["runId"], old);
    assert!(read["source"]["runFolder"].is_string());
    Ok(())
}

#[tokio::test]
async fn pagination_keeps_the_first_page_boundary_when_new_work_arrives()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let oldest = record(root.path(), 1, 1, "Oldest")?;
    let newer = record(root.path(), 2, 2, "Newer")?;
    let desk = Desk::at(Some(root.path().to_path_buf()), root.path().to_path_buf());
    let first = value(ask(&desk, "list_runs", json!({"limit": 1})).await);
    assert_eq!(first["runs"][0]["run"]["runId"], newer);
    assert!(first["cursor"].is_string());
    record(root.path(), 3, 3, "Arrived between pages")?;
    let second = value(
        ask(
            &desk,
            "list_runs",
            json!({"limit": 1, "cursor": first["cursor"]}),
        )
        .await,
    );
    assert_eq!(second["runs"][0]["run"]["runId"], oldest);
    assert!(second["cursor"].is_null());
    Ok(())
}

#[tokio::test]
async fn deleted_run_is_unavailable_not_a_claim_that_no_work_happened() -> Result<(), Box<dyn Error>>
{
    let root = tempfile::tempdir()?;
    fs::create_dir_all(root.path().join(".loadout/runs"))?;
    let desk = Desk::at(Some(root.path().to_path_buf()), root.path().to_path_buf());
    let answer = ask(
        &desk,
        "read_run_summary",
        json!({"run_id": "01980000-0000-7000-8000-000000000001"}),
    )
    .await;
    assert!(matches!(answer, Answer::Refused(ref said) if said.contains("no longer available")));
    Ok(())
}

#[tokio::test]
async fn listed_sources_reach_the_conversation_and_old_running_is_not_live()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let old = record(root.path(), 1, 1, "Older work")?;
    let directory = root
        .path()
        .join(".loadout/runs")
        .join(format!("20260901-100000__{old}"));
    let mut file: Value = serde_json::from_slice(&fs::read(directory.join("run.json"))?)?;
    file["status"] = json!("running");
    fs::write(directory.join("run.json"), file.to_string())?;
    let (sink, mut source) = line_channel(32);
    let desk = Desk::at(Some(root.path().to_path_buf()), root.path().to_path_buf())
        .showing(Arc::new(Mutex::new(sink)));
    let _ = value(ask(&desk, "list_runs", json!({"limit": 1})).await);
    let mut listed = Vec::new();
    while let Some(line) = source.try_next() {
        listed.push(line);
    }
    assert!(
        listed
            .iter()
            .any(|line| matches!(line, Line::RunSource { run_id, .. } if run_id == &old)),
        "a list result must give the person the same openable source that the model received"
    );
    let _ = value(ask(&desk, "get_run_status", json!({"run_id": old})).await);
    let mut status = Vec::new();
    while let Some(line) = source.try_next() {
        status.push(line);
    }
    assert!(
        status
            .iter()
            .any(|line| matches!(line, Line::RunSource { text, .. }
        if text.contains("last recorded as running"))),
        "a stale running file cannot become a live claim on screen"
    );
    Ok(())
}

#[tokio::test]
async fn handoff_chunks_keep_utf8_and_refuse_a_replaced_source() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let id = record(root.path(), 1, 1, "Chunked work")?;
    let file = root
        .path()
        .join(".loadout/runs")
        .join(format!("20260901-100000__{id}"))
        .join("handoffs/01-result.md");
    let prefix = fs::read_to_string(&file)?
        .split("# Findings")
        .next()
        .ok_or("no header")?
        .to_owned();
    let body = "ą🦀\"\\".repeat(10_000);
    fs::write(&file, format!("{prefix}{body}"))?;
    let desk = Desk::at(None, root.path().to_path_buf());
    let listing = value(ask(&desk, "list_handoffs", json!({"run_id":id})).await);
    let mut cursor = listing["handoffs"][0]["readCursor"].clone();
    let mut read = String::new();
    let first_cursor = cursor.clone();
    for _ in 0..32 {
        let page = value(
            ask(
                &desk,
                "read_handoff",
                json!({"run_id":id,"handoff_id":"result-1","cursor":cursor}),
            )
            .await,
        );
        let text = page["text"].as_str().ok_or("no body")?;
        assert!(text.len() <= 32 * 1024);
        assert!(serde_json::to_vec(&page)?.len() <= 64 * 1024);
        read.push_str(text);
        cursor = page["cursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    assert!(cursor.is_null(), "body must progress to a finite end");
    assert_eq!(
        read, body,
        "a byte boundary lost part of a Unicode character"
    );
    let replacement = file.with_extension("replacement");
    fs::write(&replacement, format!("{prefix}{body}"))?;
    fs::rename(&replacement, &file)?;
    assert!(
        matches!(
            ask(
                &desk,
                "read_handoff",
                json!({"run_id":id,"handoff_id":"result-1","cursor":first_cursor})
            )
            .await,
            Answer::Refused(_)
        ),
        "same bytes in a different file must invalidate the cursor"
    );
    Ok(())
}

#[tokio::test]
async fn links_and_cross_workspace_cursors_never_read_foreign_bodies() -> Result<(), Box<dyn Error>>
{
    let root = tempfile::tempdir()?;
    let a = root.path().join("a");
    let b = root.path().join("b");
    let id = record(&a, 1, 1, "Own work")?;
    record(&b, 1, 1, "FOREIGN-PRIVATE-TITLE")?;
    let folder = format!("20260901-100000__{id}");
    let own = a.join(".loadout/runs").join(&folder);
    let foreign = b.join(".loadout/runs").join(&folder);
    let desk_a = Desk::at(None, a.clone());
    let desk_b = Desk::at(None, b);
    let listing = value(ask(&desk_b, "list_handoffs", json!({"run_id":id})).await);
    assert!(matches!(
        ask(
            &desk_a,
            "read_handoff",
            json!({"run_id":id,
        "handoff_id":"result-1","cursor":listing["handoffs"][0]["readCursor"]})
        )
        .await,
        Answer::Refused(_)
    ));
    fs::remove_file(own.join("handoffs/01-result.md"))?;
    loadout_lib::engine::supervisor::link(
        &foreign.join("handoffs/01-result.md"),
        &own.join("handoffs/01-result.md"),
    )?;
    assert!(matches!(
        ask(
            &desk_a,
            "read_handoff",
            json!({"run_id":id,
        "handoff_id":"result-1"})
        )
        .await,
        Answer::Refused(_)
    ));
    fs::remove_file(own.join("run.json"))?;
    loadout_lib::engine::supervisor::link(&foreign.join("run.json"), &own.join("run.json"))?;
    assert!(matches!(
        ask(&desk_a, "read_run_summary", json!({"run_id":id})).await,
        Answer::Refused(_)
    ));
    Ok(())
}
