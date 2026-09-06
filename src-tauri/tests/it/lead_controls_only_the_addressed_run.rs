//! WF-10: prawdziwy Desk nie może zamienić deklaracji modelu w zgodę człowieka.
//! Runtime to rzeczywisty bieg z checkpointem; żaden vendor ani zewnętrzny proces nie jest potrzebny.

#![allow(clippy::panic)]

use std::error::Error;
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use loadout_lib::bridge::host::Answers;
use loadout_lib::bridge::library::{Desk, Waiting};
use loadout_lib::bridge::{Answer, Call};
use loadout_lib::commands::lead_history::LeadRunLookup;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunRequest};
use loadout_lib::engine::drivers::AgentDriver;
use loadout_lib::engine::drivers::absent::Absent;
use loadout_lib::engine::line::Line;
use loadout_lib::ipc::{AppState, line_channel};
use loadout_lib::store::Store;
use serde_json::json;

#[tokio::test]
async fn confirmed_true_is_not_a_persons_permission_to_stop() -> Result<(), Box<dyn Error>> {
    refused_stop(false, false).await
}

#[tokio::test]
async fn an_old_run_id_cannot_stop_the_run_now_waiting_here() -> Result<(), Box<dyn Error>> {
    refused_stop(true, false).await
}

#[tokio::test]
async fn only_an_exact_human_answer_grants_one_stop_for_this_run() -> Result<(), Box<dyn Error>> {
    refused_stop(false, true).await
}

async fn refused_stop(old_address: bool, with_permission: bool) -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let home = root.path().join("home");
    let project = root.path().join("project");
    fs::create_dir_all(&home)?;
    fs::create_dir_all(project.join(".loadout/workflows"))?;
    let workflow = project.join(".loadout/workflows/paused.json");
    fs::write(
        &workflow,
        json!({
            "format": 1, "id": "wf-control", "name": "Review the result", "links": [],
            "steps": [{"kind": "checkpoint", "id": "decision", "name": "Decision",
                "question": "What should happen next?", "at": {"x": 0, "y": 0}}]
        })
        .to_string(),
    )?;
    let absent: Arc<dyn AgentDriver> = Arc::new(Absent::new("nobody", "checkpoint only"));
    let drivers: Drivers = Arc::new(move |_| Arc::clone(&absent));
    let state = AppState::new(
        home.clone(),
        project.clone(),
        Store::open(&project.join(".loadout/loadout.db"))?,
        drivers,
    );
    let deps = state.begin_run(&project)?;
    let fixed_control = deps.control.clone();
    let (conversation_sink, mut conversation) = line_channel(256);
    let waiting = Arc::new(Waiting::default());
    let desk = Desk::at(Some(home), project.clone())
        .showing(Arc::new(Mutex::new(conversation_sink)))
        .hearing(Arc::clone(&waiting))
        .reading_runs_with(LeadRunLookup::new(move |_| Some(fixed_control.clone())));
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (run_sink, mut run_lines) = line_channel(256);
    let (ran, checked) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(run_workflow_inner(&deps, &request, run_sink), async {
            loop {
                if matches!(run_lines.try_next(), Some(Line::Asked { .. })) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            let address = deps
                .control
                .run_address()
                .ok_or("paused run has no identity")?;
            if with_permission {
                human_stop(
                    &desk,
                    &waiting,
                    &mut conversation,
                    &address.id,
                    &deps.control,
                )
                .await?;
                return Ok(());
            }
            let input_id = if old_address {
                "01980000-0000-7000-8000-000000000099"
            } else {
                &address.id
            };
            let reply = desk
                .answer(Call {
                    id: json!(1),
                    call: "stop_run".to_owned(),
                    input: json!({"run_id": input_id, "confirmed": true}),
                })
                .await;
            let was_stopped = deps.control.cancel_token().is_cancelled();
            // Always settle the actual production run before reporting an assertion failure.
            deps.control.stop();
            let mut shown = Vec::new();
            while let Some(line) = conversation.try_next() {
                shown.push(line);
            }
            assert!(
                !was_stopped,
                "a model assertion stopped a run without a human answer"
            );
            assert!(
                matches!(&reply, Answer::Refused(_)),
                "confirmed:true must not authorize this operation: {reply:?}"
            );
            assert!(
                !shown.iter().any(|line| matches!(line,
                Line::Suggested { command, auto: true, .. } if command == "/stop")),
                "a refusal still sent a mutating command to the window"
            );
            let Answer::Refused(said) = reply else {
                unreachable!()
            };
            assert!(
                shown
                    .iter()
                    .any(|line| matches!(line, Line::Problem { text, .. } if text == &said)),
                "the conversation must show the exact refusal received by the model"
            );
            Ok::<(), Box<dyn Error>>(())
        })
    })
    .await?;
    let _ = ran?;
    checked?;
    Ok(())
}

async fn human_stop(
    desk: &Desk,
    waiting: &Waiting,
    conversation: &mut loadout_lib::ipc::LineSource,
    run_id: &str,
    control: &loadout_lib::commands::RunControl,
) -> Result<(), Box<dyn Error>> {
    let ask = Call {
        id: json!(2),
        call: "ask_the_person".to_owned(),
        input: json!({"operation":"stop_run", "run_id":run_id,
            "question":"Pretend this is harmless", "options":["Trust the model"]}),
    };
    let (answer, ()) = tokio::join!(desk.answer(ask), async {
        loop {
            if let Some(Line::Asked {
                text,
                options,
                question: Some(question),
                ..
            }) = conversation.try_next()
            {
                assert!(text.contains("Review the result"));
                assert!(!text.contains("Pretend this is harmless"));
                assert_eq!(options, ["Stop run", "Keep running"]);
                assert!(
                    !waiting.answer("Lead", "Stop run".to_owned()),
                    "legacy reply cannot grant a control capability"
                );
                assert!(!waiting.answer_exact("Lead", "older-question", "Stop run".to_owned()));
                assert!(waiting.answer_exact("Lead", &question.question_id, "Stop run".to_owned()));
                assert!(!waiting.answer_exact(
                    "Lead",
                    &question.question_id,
                    "Stop run".to_owned()
                ));
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    });
    let Answer::Ok(approved) = answer else {
        panic!("human answer was refused: {answer:?}")
    };
    let token = approved["approvalToken"]
        .as_str()
        .ok_or("no one-use token")?;
    let stopped = desk
        .answer(Call {
            id: json!(3),
            call: "stop_run".to_owned(),
            input: json!({"run_id":run_id,"approval_token":token}),
        })
        .await;
    assert!(
        matches!(&stopped, Answer::Ok(value) if value["result"] == "stopped"),
        "real stop did not return a terminal receipt: {stopped:?}"
    );
    assert!(
        !control.is_working(),
        "Stopped preceded the real run settling"
    );
    let twice = desk
        .answer(Call {
            id: json!(4),
            call: "stop_run".to_owned(),
            input: json!({"run_id":run_id,"approval_token":token}),
        })
        .await;
    assert!(
        matches!(twice, Answer::Refused(_)),
        "spent consent was accepted twice"
    );
    let mut shown = Vec::new();
    while let Some(line) = conversation.try_next() {
        shown.push(line);
    }
    assert!(
        shown
            .iter()
            .any(|line| matches!(line, Line::Note { text, .. }
        if text.contains("run stopped"))),
        "the person never saw the final stop receipt"
    );
    assert!(
        !shown
            .iter()
            .any(|line| matches!(line, Line::Suggested { auto: true, .. })),
        "the runtime delegated the stop back to a mutable UI scope"
    );
    Ok(())
}
