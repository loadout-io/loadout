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
use loadout_lib::commands::RunControl;
use loadout_lib::commands::lead_history::LeadRunLookup;
use loadout_lib::engine::line::Line;
use loadout_lib::ipc::{LineSource, line_channel};
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
        /* 2026-09-08 (CT-03a) — `Answer` niesie od tego dnia także obraz. Ramię jest JAWNE,
         * a nie `_`, bo czwarty wariant ma przewrócić ten plik, a nie wpaść tu w ciszy. */
        Answer::Image { mime, .. } => panic!("history reads as facts, never as a {mime}"),
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

/// Ustawia zapisany bieg na `paused` — z pytaniem stojącym pod kafelkiem kontrolnym albo bez.
///
/// Fikstura różni się dokładnie tym, czym różnią się prawdziwe biegi: rodzajem kafelka, na
/// którym stoi krok w toku. Słowo na dysku jest w obu przypadkach to samo (`status: "paused"`),
/// bo powodu pauzy `run.json` celowo nie niesie.
fn paused(project: &Path, day: u8, id: &str, on_a_question: bool) -> Result<(), Box<dyn Error>> {
    let directory = project
        .join(".loadout/runs")
        .join(format!("202609{day:02}-100000__{id}"));
    let mut file: Value = serde_json::from_slice(&fs::read(directory.join("run.json"))?)?;
    let kind = if on_a_question { "checkpoint" } else { "agent" };
    file["status"] = json!("paused");
    file["steps"] = json!([{"id": "step-1", "node_key": "decision", "name": "Decision",
        "status": "running"}]);
    file["workflow_snapshot"] =
        json!({"steps": [{"id": "decision", "kind": kind, "question": "Ship it?"}]});
    fs::write(directory.join("run.json"), file.to_string())?;
    Ok(())
}

/// Wiersze „otwórz źródło biegu", które doszły do rozmowy od poprzedniego sprawdzenia.
fn sources(source: &mut LineSource) -> Vec<String> {
    let mut rows = Vec::new();
    while let Some(line) = source.try_next() {
        if let Line::RunSource { text, .. } = line {
            rows.push(text);
        }
    }
    rows
}

/// Zdanie o biegu, które most kładzie człowiekowi na ekran przy pytaniu o jego stan.
async fn shown_for(root: &Path, live: Option<&str>, run: &str) -> Option<String> {
    let (sink, mut source) = line_channel(32);
    let mut desk =
        Desk::at(Some(root.to_path_buf()), root.to_path_buf()).showing(Arc::new(Mutex::new(sink)));
    if let Some(live) = live {
        let control = RunControl::new();
        control.begin();
        control.set_run_address(live.to_owned(), root.join(".loadout/runs"));
        desk = desk.reading_runs_with(LeadRunLookup::new(move |_| Some(control.clone())));
    }
    let _ = value(ask(&desk, "get_run_status", json!({"run_id": run})).await);
    sources(&mut source).pop()
}

/// Bieg stoi z dwóch niezależnych powodów, a zdanie na ekranie zna tylko jeden z nich.
///
/// Bieg wstrzymany limitem dostawcy meldował człowiekowi „is waiting for an answer" — pytanie,
/// którego nikt nie zadał i którego nie da się na ekranie znaleźć.
#[tokio::test]
async fn a_pause_with_no_question_standing_is_not_announced_as_one() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let asked = record(root.path(), 1, 1, "Run standing on a question")?;
    let held = record(root.path(), 2, 2, "Run held by its agent app")?;
    paused(root.path(), 1, &asked, true)?;
    paused(root.path(), 2, &held, false)?;
    for (live, run, question_stands) in [
        (Some(held.as_str()), held.as_str(), false),
        (None, held.as_str(), false),
        (Some(asked.as_str()), asked.as_str(), true),
        (None, asked.as_str(), true),
    ] {
        let said = shown_for(root.path(), live, run)
            .await
            .ok_or("no openable run source reached the conversation")?;
        assert_eq!(
            said.contains("waiting for an answer"),
            question_stands,
            "a paused run told the person the wrong thing about why it stands: {said}"
        );
    }
    Ok(())
}

/// Wiersz o źródle biegu ma sens raz — z wyniku, który potrafi ten bieg nazwać.
///
/// Odczyt przekazania nie niesie ani tytułu, ani stanu, więc każdy taki odczyt dokładał do czatu
/// kolejne, identyczne „Saved work has saved source material." — a bieg ma ich do 27.
#[tokio::test]
async fn reading_saved_work_does_not_repeat_a_row_that_cannot_name_the_run()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let id = record(root.path(), 1, 1, "The parser fix")?;
    let (sink, mut source) = line_channel(32);
    let desk = Desk::at(Some(root.path().to_path_buf()), root.path().to_path_buf())
        .showing(Arc::new(Mutex::new(sink)));
    let _ = value(ask(&desk, "get_run_status", json!({"run_id": id})).await);
    assert_eq!(
        sources(&mut source).len(),
        1,
        "the result that knows this run stopped offering its source to the person"
    );
    let listed = value(ask(&desk, "list_handoffs", json!({"run_id": id})).await);
    assert_eq!(listed["handoffs"][0]["id"], "result-1");
    for _ in 0..3 {
        let read = value(
            ask(
                &desk,
                "read_handoff",
                json!({"run_id": id, "handoff_id": "result-1"}),
            )
            .await,
        );
        assert!(
            read["text"]
                .as_str()
                .is_some_and(|body| body.contains("Marker-1")),
            "the lead agent stopped receiving the saved text it asked for"
        );
    }
    let repeated = sources(&mut source);
    assert!(
        repeated.is_empty(),
        "reading saved work repeated a row that cannot even name the run: {repeated:?}"
    );
    Ok(())
}
