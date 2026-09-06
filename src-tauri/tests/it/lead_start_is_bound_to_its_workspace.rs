//! WF-07: rzeczywisty Desk → `AppState` → prepare. Checkpoint utrzymuje graf przy życiu,
//! więc odpowiedź narzędzia nie może przypadkiem być odpowiedzią dopiero po całym biegu.

#![allow(clippy::expect_used, clippy::panic)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn debugging_a_start_does_not_archive_encoded_workflow_instructions()
-> Result<(), Box<dyn std::error::Error>> {
    let secret_source = br#"{"instructions":"private source instructions"}"#;
    let revision = loadout_lib::durable_file::revision_of(secret_source);
    let starts = std::sync::Arc::new(loadout_lib::commands::lead_start::LeadStarts::default());
    let request = starts.register(
        "conversation".to_owned(),
        PathBuf::from("/work/project"),
        PathBuf::from("/work/ship.json"),
        revision.clone(),
        Some("private live task".to_owned()),
    )?;
    let debug = format!("{request:?}");
    assert!(
        !debug.contains(&revision),
        "Debug archives the complete source as reversible base64"
    );
    assert!(!debug.contains("private live task"));
    assert!(!debug.contains("private source instructions"));
    let accepting =
        loadout_lib::commands::lead_start::LeadStart::new(starts, request.origin, revision.clone());
    assert!(
        !format!("{accepting:?}").contains(&revision),
        "the accepting handle also archives the source revision"
    );
    Ok(())
}
use std::sync::{Arc, Mutex};
use std::time::Duration;

use loadout_lib::bridge::host::Answers;
use loadout_lib::bridge::library::Desk;
use loadout_lib::bridge::{Answer, Call};
use loadout_lib::commands::Drivers;
use loadout_lib::commands::lead_start::LeadStarts;
use loadout_lib::engine::drivers::AgentDriver;
use loadout_lib::engine::drivers::absent::Absent;
use loadout_lib::engine::line::Line;
use loadout_lib::ipc::{AppState, LineSource, line_channel};
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tokio::task::JoinHandle;
use uuid::Uuid;

const TASK: &str = "keep this task only in the backend start request";

struct Bench {
    _root: tempfile::TempDir,
    home: PathBuf,
    a: PathBuf,
    b: PathBuf,
    state: Arc<AppState>,
    conversation: Uuid,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let root_path = root.path().canonicalize()?;
        let home = root_path.join("home");
        let a = root_path.join("a");
        let b = root_path.join("b");
        fs::create_dir_all(&home)?;
        for (project, step) in [(&a, "only-a"), (&b, "only-b")] {
            fs::create_dir_all(project.join(".loadout/workflows"))?;
            fs::write(project.join(".loadout/workflows/ship.json"), workflow(step))?;
        }
        let absent: Arc<dyn AgentDriver> = Arc::new(Absent::new("nobody", "checkpoint only"));
        let drivers: Drivers = Arc::new(move |_| Arc::clone(&absent));
        let store = Store::open(&b.join(".loadout/loadout.db"))?;
        // Domyślny projekt jest B. Start musi zachować A zapisane w rozmowie.
        let state = Arc::new(AppState::new(home.clone(), b.clone(), store, drivers));
        Ok(Self {
            _root: root,
            home,
            a,
            b,
            state,
            conversation: Uuid::now_v7(),
        })
    }

    fn desk(&self) -> (Desk, LineSource) {
        let (sink, source) = line_channel(128);
        let desk = Desk::at(Some(self.home.clone()), self.a.clone())
            .showing(Arc::new(Mutex::new(sink)))
            .starting_with(
                self.state.lead_starts(),
                Arc::new(Mutex::new(self.conversation)),
            );
        (desk, source)
    }

    fn ask(&self) -> (JoinHandle<Answer>, LineSource) {
        let (desk, source) = self.desk();
        (
            tokio::spawn(async move { desk.answer(start_call()).await }),
            source,
        )
    }
}

fn workflow(step: &str) -> String {
    json!({
        "format": 1, "id": format!("wf-{step}"), "name": "Ship it", "links": [],
        "steps": [{"kind": "checkpoint", "id": step, "name": step,
            "question": "May this run continue?", "at": {"x": 0, "y": 0}}]
    })
    .to_string()
}

fn start_call() -> Call {
    Call {
        id: json!(1),
        call: "start_workflow".to_owned(),
        input: json!({"workflow": "ship-it", "task": TASK}),
    }
}

async fn request_from(source: &mut LineSource) -> Line {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(line @ Line::RunRequested { .. }) = source.try_next() {
                return line;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("the production Desk did not send its addressed request")
}

fn request_id(line: &Line) -> String {
    match line {
        Line::RunRequested { request_id, .. } => request_id.clone(),
        other => panic!("expected the addressed request, got {other:?}"),
    }
}

fn runs(project: &Path) -> Vec<Value> {
    fs::read_dir(project.join(".loadout/runs"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| fs::read(entry.path().join("run.json")).ok())
        .map(|bytes| serde_json::from_slice(&bytes).expect("published run.json must be readable"))
        .collect()
}

#[tokio::test]
async fn acknowledged_start_has_a_durable_identity_before_the_graph_finishes()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let (asked, mut conversation) = bench.ask();
    let line = request_from(&mut conversation).await;
    let id = request_id(&line);
    assert!(
        !asked.is_finished(),
        "sending a row is not acceptance of a run"
    );
    assert!(runs(&bench.a).is_empty());
    let wire = serde_json::to_value(&line)?;
    assert_eq!(wire["workspace"], bench.a.to_string_lossy().as_ref());
    assert_eq!(wire["conversationId"], bench.conversation.to_string());
    assert_eq!(wire["steps"][0]["id"], "only-a");
    assert!(
        !wire.to_string().contains(TASK),
        "the UI transport archived the task"
    );
    let state = Arc::clone(&bench.state);
    let accepted_id = id.clone();
    let (sink, mut output) = line_channel(512);
    let running = tokio::spawn(async move {
        state
            .accept_lead_start_inner(&accepted_id, 4, Some(8.0), false, sink)
            .await
    });
    let answer = tokio::time::timeout(Duration::from_secs(5), asked).await??;
    let Answer::Ok(answer) = answer else {
        panic!("real prestart was refused: {answer:?}")
    };
    assert_eq!(answer["started"], true);
    assert_eq!(answer["requestId"], id);
    assert_eq!(
        answer["run"]["workspace"],
        bench.a.to_string_lossy().as_ref()
    );
    assert!(
        !running.is_finished(),
        "ack waited for the parked graph to finish"
    );
    let files = runs(&bench.a);
    assert_eq!(files.len(), 1, "ack preceded publication of run.json");
    assert_eq!(files[0]["id"], answer["run"]["runId"]);
    assert_eq!(files[0]["lead_origin"]["request_id"], id);
    assert_eq!(
        files[0]["lead_origin"]["conversation_id"],
        bench.conversation.to_string()
    );
    assert_eq!(files[0]["workflow_id"], "wf-only-a");
    assert_eq!(files[0]["concurrency"], 4);
    assert_eq!(files[0]["budget_usd"], 8.0);
    assert!(
        runs(&bench.b).is_empty(),
        "the current workspace stole the Lead's start"
    );

    let (duplicate, _unused) = line_channel(32);
    let receipt = bench
        .state
        .accept_lead_start_inner(&id, 1, None, true, duplicate)
        .await?;
    assert_eq!(json!(receipt.run.run_id), answer["run"]["runId"]);
    assert_eq!(
        runs(&bench.a).len(),
        1,
        "duplicate transport started a second graph"
    );

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if matches!(output.try_next(), Some(Line::Asked { .. })) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await?;
    assert!(bench.state.stop_the_run_in(&bench.a).await?);
    tokio::time::timeout(Duration::from_secs(5), running).await???;
    assert_eq!(
        runs(&bench.a)[0]["lead_origin"]["request_id"],
        id,
        "later serialization lost start correlation"
    );
    Ok(())
}

#[tokio::test]
async fn changed_workflow_refuses_the_same_request_before_durable_acceptance()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let (asked, mut source) = bench.ask();
    let id = request_id(&request_from(&mut source).await);
    fs::write(
        bench.a.join(".loadout/workflows/ship.json"),
        workflow("changed"),
    )?;
    let (sink, _source) = line_channel(32);
    let refusal = bench
        .state
        .accept_lead_start_inner(&id, 4, None, false, sink)
        .await
        .expect_err("an edited graph must not reuse the earlier decision");
    assert!(
        refusal.contains("changed after the lead selected it"),
        "{refusal}"
    );
    let answer = asked.await?;
    assert!(matches!(answer, Answer::Refused(ref said) if said == &refusal));
    assert!(runs(&bench.a).is_empty());
    let (sink, _source) = line_channel(32);
    assert_eq!(
        bench
            .state
            .accept_lead_start_inner(&id, 4, None, false, sink)
            .await
            .expect_err("a refused request cannot be retried under the same id"),
        refusal
    );
    Ok(())
}

#[tokio::test]
async fn an_unrunnable_graph_never_returns_started() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let mut invalid: Value = serde_json::from_str(&workflow("only-a"))?;
    invalid["steps"] = json!([]);
    fs::write(
        bench.a.join(".loadout/workflows/ship.json"),
        invalid.to_string(),
    )?;
    let (asked, mut source) = bench.ask();
    let id = request_id(&request_from(&mut source).await);
    let (sink, _source) = line_channel(32);
    let refusal = bench
        .state
        .accept_lead_start_inner(&id, 4, None, false, sink)
        .await
        .expect_err("an empty graph cannot be accepted as a run");
    assert!(matches!(asked.await?, Answer::Refused(ref said) if said == &refusal));
    assert!(runs(&bench.a).is_empty());
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn late_window_delivery_cannot_start_an_expired_request() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let (asked, mut source) = bench.ask();
    let id = request_id(&request_from(&mut source).await);
    tokio::time::advance(Duration::from_secs(31)).await;
    let answer = asked.await?;
    assert!(matches!(answer, Answer::Refused(ref said) if said.contains("in time")));
    let (sink, _source) = line_channel(32);
    let refusal = bench
        .state
        .accept_lead_start_inner(&id, 4, None, false, sink)
        .await
        .expect_err("late frontend revived an expired request");
    assert!(refusal.contains("in time"), "{refusal}");
    assert!(runs(&bench.a).is_empty());
    Ok(())
}

#[tokio::test]
async fn missing_window_and_old_app_instance_refuse_without_starting() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let (desk, source) = bench.desk();
    drop(source);
    let answer = tokio::time::timeout(Duration::from_secs(1), desk.answer(start_call())).await?;
    assert!(matches!(answer, Answer::Refused(ref said) if said.contains("window")));
    assert!(runs(&bench.a).is_empty());

    let request = bench.state.lead_starts().register(
        bench.conversation.to_string(),
        bench.a.clone(),
        bench.a.join(".loadout/workflows/ship.json"),
        "revision".to_owned(),
        None,
    )?;
    let new_instance = LeadStarts::default();
    let refused = new_instance
        .claim(&request.origin.request_id)
        .expect_err("a different app instance accepted an old capability");
    assert!(refused.contains("earlier app session"), "{refused}");
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn claiming_removes_only_the_transport_deadline() -> Result<(), Box<dyn Error>> {
    let starts = Arc::new(LeadStarts::default());
    let request = starts.register(
        Uuid::now_v7().to_string(),
        PathBuf::from("/work/a"),
        PathBuf::from("/work/a/ship.json"),
        "revision".to_owned(),
        None,
    )?;
    assert!(starts.claim(&request.origin.request_id)?.is_some());
    tokio::time::advance(Duration::from_secs(31)).await;
    assert!(
        starts.claim(&request.origin.request_id)?.is_none(),
        "a duplicate re-claimed it"
    );
    let accepting = loadout_lib::commands::lead_start::LeadStart::new(
        Arc::clone(&starts),
        request.origin.clone(),
        request.revision,
    );
    accepting.prepared(Path::new("/work/a"), "accepted-run");
    assert_eq!(
        starts.wait(&request.origin.request_id).await?.run.run_id,
        "accepted-run"
    );
    Ok(())
}
