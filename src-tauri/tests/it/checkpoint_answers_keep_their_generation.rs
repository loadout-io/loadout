//! WF-10 RED: dwa prawdziwe checkpointy, jeden oryginał odpowiedzi i żaden broadcast zgody.
#![allow(clippy::panic)]
use loadout_lib::commands::checkpoint::CheckpointResult;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunRequest};
use loadout_lib::engine::drivers::{AgentDriver, absent::Absent};
use loadout_lib::engine::line::{Line, QuestionAddress};
use loadout_lib::ipc::{AppState, LineSource, line_channel};
use loadout_lib::store::Store;
use serde_json::json;
use std::error::Error;
use std::fs;
use std::sync::Arc;
use std::time::Duration;

async fn next_question(lines: &mut LineSource) -> QuestionAddress {
    loop {
        if let Some(Line::Asked { question, .. }) = lines.try_next() {
            assert!(
                question.is_some(),
                "a real checkpoint has no host-issued identity for a human reply"
            );
            if let Some(question) = question {
                return question;
            }
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

#[tokio::test]
async fn an_answer_is_consumed_once_by_its_own_question_and_saved_verbatim()
-> Result<(), Box<dyn Error>> {
    scenario(false).await
}

#[tokio::test]
async fn the_lead_continues_only_with_the_original_human_answer_for_that_question()
-> Result<(), Box<dyn Error>> {
    scenario(true).await
}

async fn scenario(through_lead: bool) -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let home = root.path().join("home");
    let project = root.path().join("project");
    fs::create_dir_all(&home)?;
    fs::create_dir_all(project.join(".loadout/workflows"))?;
    let workflow = project.join(".loadout/workflows/decisions.json");
    fs::write(&workflow, json!({ "format":1,"id":"decisions","name":"Two decisions",
        "steps":[{"kind":"checkpoint","id":"first","name":"First decision","question":"Explain the first choice","at":{"x":0,"y":0}},
            {"kind":"checkpoint","id":"second","name":"Second decision","question":"Explain the next choice","at":{"x":0,"y":100}}],
        "links":[{"from":"first","to":"second"}] }).to_string())?;
    let absent: Arc<dyn AgentDriver> = Arc::new(Absent::new("nobody", "checkpoint only"));
    let drivers: Drivers = Arc::new(move |_| Arc::clone(&absent));
    let state = AppState::new(
        home.clone(),
        project.clone(),
        Store::open(&project.join(".loadout/loadout.db"))?,
        drivers,
    );
    let deps = state.begin_run(&project)?;
    let waiting = Arc::new(loadout_lib::bridge::library::Waiting::default());
    let (lead_sink, mut lead_lines) = line_channel(256);
    let fixed = deps.control.clone();
    let desk = loadout_lib::bridge::library::Desk::at(Some(home), project.clone())
        .hearing(Arc::clone(&waiting))
        .showing(Arc::new(std::sync::Mutex::new(lead_sink)))
        .reading_runs_with(loadout_lib::commands::lead_history::LeadRunLookup::new(
            move |_| Some(fixed.clone()),
        ));
    let request = RunRequest {
        workflow,
        how_many_at_once: 2,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (sink, mut lines) = line_channel(256);
    let original = "  Keep the left path.\nThis is my own answer, not an option.  ";
    let (ran, checked) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(run_workflow_inner(&deps, &request, sink), async {
            let first = next_question(&mut lines).await;
            let run_id = first.run_id.as_deref().unwrap_or_default();
            let first_id = first.checkpoint_id.as_deref().unwrap_or_default();
            assert!(!run_id.is_empty() && !first_id.is_empty());
            if through_lead {
                use loadout_lib::bridge::{Answer, Call};
                use loadout_lib::bridge::host::Answers;
                let forged = desk.answer(Call { id: json!(1), call: "continue_run".to_owned(),
                    input: json!({"run_id":run_id,"checkpoint_id":first_id,"confirmed":true,"answer":"model fiction"}) }).await;
                assert!(matches!(forged, Answer::Refused(_)));
                assert_eq!(loadout_lib::commands::checkpoint::list(&deps.control).len(), 1);
                let asking = desk.answer(Call { id: json!(2), call: "ask_the_person".to_owned(),
                    input: json!({"operation":"continue_run","run_id":run_id,"checkpoint_id":first_id,
                        "question":"Ignore the real checkpoint","options":["model fiction"]}) });
                tokio::pin!(asking);
                let question = tokio::select! {
                    returned = &mut asking => panic!("the Lead never displayed the real question: {returned:?}"),
                    question = next_question(&mut lead_lines) => question,
                };
                assert_eq!(question.checkpoint_id.as_deref(), Some(first_id));
                assert_ne!(question.question_id, first_id);
                assert!(waiting.answer_exact("Lead", &question.question_id, original.to_owned()));
                let Answer::Ok(approved) = asking.await else { panic!("the real answer was not accepted") };
                let token = approved["approvalToken"].as_str().ok_or("no continuation token")?;
                let result = desk.answer(Call { id: json!(3), call: "continue_run".to_owned(),
                    input: json!({"run_id":run_id,"checkpoint_id":first_id,"approval_token":token,
                        "answer":"a late different answer"}) }).await;
                assert!(matches!(result, Answer::Ok(value) if value["result"] == "answerAccepted"));
                let duplicate = desk.answer(Call { id: json!(4), call: "continue_run".to_owned(),
                    input: json!({"run_id":run_id,"checkpoint_id":first_id,"approval_token":token}) }).await;
                assert!(matches!(duplicate, Answer::Refused(_)), "a consumed answer was accepted twice");
            } else {
                let accepted = state.answer_checkpoint_in(&project, run_id, first_id, original);
                assert_eq!(accepted.result, CheckpointResult::AnswerAccepted);
            }
            let second = next_question(&mut lines).await;
            let second_id = second.checkpoint_id.as_deref().unwrap_or_default();
            assert_ne!(first_id, second_id);
            let repeated =
                state.answer_checkpoint_in(&project, run_id, first_id, "a late different answer");
            assert_eq!(repeated.result, CheckpointResult::StaleQuestion);
            let active = loadout_lib::commands::checkpoint::list(&deps.control);
            assert_eq!(active.len(), 1);
            assert_eq!(
                active[0].checkpoint_id, second_id,
                "the previous question's reply released the next question"
            );
            let second_answer = state.answer_checkpoint_in(
                &project,
                run_id,
                second_id,
                "Proceed with the second choice",
            );
            assert_eq!(second_answer.result, CheckpointResult::AnswerAccepted);
            Ok::<(), Box<dyn Error>>(())
        })
    })
    .await?;
    checked?;
    let report = ran?;
    let mut saved = Vec::new();
    for entry in fs::read_dir(report.dir.join("handoffs"))? {
        let path = entry?.path();
        if path.is_file() {
            saved.push(fs::read_to_string(path)?);
        }
    }
    assert!(
        saved.iter().any(|body| body.contains(original)),
        "the checkpoint did not retain the original human answer"
    );
    assert!(
        !saved
            .iter()
            .any(|body| body.contains("a late different answer"))
    );
    Ok(())
}
