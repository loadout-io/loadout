//! WF-10: cienkie drzwi do istniejącego uchwytu, z tożsamością i zgodą po stronie hosta.

use super::{ApprovalScope, ApprovalSubject, ConfirmationBinding, Desk};
use crate::commands::RunControl;
use crate::commands::chat::LEAD;
use crate::commands::lead_start::RunRef;
use crate::engine::line::{Line, QuestionAddress};
use crate::ipc::Sent;
use serde_json::{Value, json};
use std::sync::PoisonError;

const NEED_PERMISSION: &str = "Ask this person with ask_the_person, operation stop_run and this exact run_id. Only their displayed confirmation can authorize Stop; confirmed:true is not permission.";

impl Desk {
    fn inactive_run(&self) -> String {
        let folder = self.project.file_name().map_or_else(
            || "this workspace".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        format!(
            "That run is no longer active in {folder}. Read its current status before trying again."
        )
    }

    fn addressed_run(&self, input: &Value) -> Result<(RunRef, RunControl), String> {
        if input.get("workspace").is_some() || input.get("folder").is_some() {
            return Err(
                "This operation only addresses the folder of this conversation.".to_owned(),
            );
        }
        let id = input
            .get("run_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "Name the exact run_id from its current status first.".to_owned())?;
        let control = self
            .runs
            .active(&self.project)
            .ok_or_else(|| self.inactive_run())?;
        let actual = control
            .run_address()
            .filter(|actual| actual.id == id)
            .ok_or_else(|| self.inactive_run())?;
        Ok((
            RunRef {
                workspace: self.project.clone(),
                run_id: actual.id,
            },
            control,
        ))
    }

    pub(super) fn scope_for(
        &self,
        run: RunRef,
        operation: &str,
        checkpoint_id: Option<String>,
    ) -> ApprovalScope {
        ApprovalScope {
            conversation: *self
                .conversation
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
            subject: ApprovalSubject::Run(run),
            operation: operation.to_owned(),
            checkpoint_id,
        }
    }

    /// Wspólny seam także dla usług: opis i przyciski muszą pochodzić z hosta wywołującego.
    /// `affirmative: None` zachowuje każdą oryginalną odpowiedź, w tym tekst własnymi słowami.
    pub(crate) async fn ask_for_approval(
        &self,
        scope: ApprovalScope,
        text: String,
        options: Vec<String>,
        affirmative: Option<String>,
        expires: impl std::future::Future<Output = ()>,
    ) -> Result<Value, String> {
        let Some(lines) = self.lines.as_ref() else {
            return Err(
                "Loadout cannot show that confirmation. Reopen this conversation and try again."
                    .to_owned(),
            );
        };
        let question_id = uuid::Uuid::now_v7().to_string();
        let run_id = match &scope.subject {
            ApprovalSubject::Replay { run, .. }
            | ApprovalSubject::Restore { run, .. }
            | ApprovalSubject::Run(run) => run.run_id.clone(),
            ApprovalSubject::Service(service) => service.run_id.clone(),
        };
        let question = QuestionAddress {
            question_id: question_id.clone(),
            run_id: Some(run_id),
            checkpoint_id: scope.checkpoint_id.clone(),
            operation: scope.operation.clone(),
        };
        let binding = ConfirmationBinding {
            question_id: question_id.clone(),
            scope,
            affirmative,
        };
        let (ticket, hear) = self.waiting.park_bound(LEAD.to_owned(), Some(binding));
        let sent = lines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .send(Line::Asked {
                agent: LEAD.to_owned(),
                text,
                options,
                question: Some(question),
            });
        if sent != Sent::Queued {
            self.waiting.withdraw(&ticket);
            return Err(
                "Loadout could not show the confirmation. Nothing was authorized.".to_owned(),
            );
        }
        let answer = tokio::select! {
            answer = hear => answer.map_err(|_| "That confirmation was withdrawn. Nothing was authorized.".to_owned()),
            () = expires => Err("That operation is no longer available. Its confirmation has expired.".to_owned()),
        };
        self.waiting.withdraw(&ticket);
        let original = answer?;
        let token = self.waiting.token_for(&question_id).ok_or_else(|| {
            "The person did not approve that operation. Nothing changed.".to_owned()
        })?;
        Ok(
            json!({ "questionId": question_id, "approvalToken": token, "answer": original,
            "said": "The person's answer was recorded for this exact operation. It can be used once." }),
        )
    }

    pub(super) async fn ask_control(&self, input: &Value) -> Result<Value, String> {
        let (run, control) = self.addressed_run(input)?;
        match input.get("operation").and_then(Value::as_str) {
            Some("stop_run") => {
                let status = crate::commands::lead_history::answer(
                    &self.project,
                    &self.runs,
                    "get_run_status",
                    &json!({"run_id": run.run_id}),
                )?;
                let title = status
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("this run");
                let text = format!(
                    "Stop {title}? Its unfinished work will end. Background work explicitly kept for this window stays open."
                );
                self.ask_for_approval(
                    self.scope_for(run, "stop_run", None),
                    text,
                    vec!["Stop run".to_owned(), "Keep running".to_owned()],
                    Some("Stop run".to_owned()),
                    control.wait_until_settled(),
                )
                .await
            }
            Some("continue_run") => {
                let checkpoint_id = input
                    .get("checkpoint_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        "Name the exact checkpoint_id from this run's current status.".to_owned()
                    })?;
                let question = crate::commands::checkpoint::list(&control).into_iter()
                    .find(|one| one.checkpoint_id == checkpoint_id)
                    .ok_or_else(|| "That question is no longer waiting for an answer. Read the current questions first.".to_owned())?;
                let lifetime = crate::commands::checkpoint::lifetime(&control, checkpoint_id)
                    .ok_or_else(|| "That question has already been answered.".to_owned())?;
                self.ask_for_approval(
                    self.scope_for(run, "continue_run", Some(checkpoint_id.to_owned())),
                    question.question,
                    question.options,
                    None,
                    async {
                        tokio::select! {
                            () = lifetime.cancelled() => {},
                            () = control.wait_until_settled() => {},
                        }
                    },
                )
                .await
            }
            _ => Err("That operation does not have a confirmation available here.".to_owned()),
        }
    }

    pub(super) fn continue_addressed(&self, input: &Value) -> Result<Value, String> {
        let (run, control) = self.addressed_run(input)?;
        let checkpoint_id = input
            .get("checkpoint_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                "Name the exact checkpoint_id from this run's current status.".to_owned()
            })?;
        if crate::commands::checkpoint::lifetime(&control, checkpoint_id).is_none() {
            return Err("That question is no longer waiting for an answer. The next question was not changed.".to_owned());
        }
        let token = input
            .get("approval_token")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let original = self.waiting.consume(
            &self.scope_for(run.clone(), "continue_run", Some(checkpoint_id.to_owned())), token,
        ).ok_or_else(|| "Ask this person with ask_the_person, operation continue_run, and this exact run_id and checkpoint_id. Only their original answer can continue this question.".to_owned())?;
        // Pole answer od modelu jest świadomie ignorowane: host ma już oryginał człowieka.
        let reply =
            crate::commands::checkpoint::answer(&control, &run.run_id, checkpoint_id, &original);
        serde_json::to_value(reply)
            .map_err(|_| "Loadout could not describe that answer.".to_owned())
    }

    pub(super) async fn stop_addressed(&self, input: &Value) -> Result<Value, String> {
        let (run, control) = self.addressed_run(input)?;
        if self.lines.is_none() {
            return Err("Reopen this conversation before stopping the run.".to_owned());
        }
        let scope = self.scope_for(run.clone(), "stop_run", None);
        let status = crate::commands::lead_history::answer(
            &self.project,
            &self.runs,
            "get_run_status",
            &json!({"run_id":run.run_id}),
        )?;
        let title = status
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("this run");
        let count = status
            .get("stepCount")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let token = input
            .get("approval_token")
            .and_then(Value::as_str)
            .unwrap_or_default();
        self.waiting
            .consume(&scope, token)
            .ok_or_else(|| NEED_PERMISSION.to_owned())?;
        // Klon uchwytu jest z pierwszego lookup, przed await. Nie wyszukujemy następcy A2.
        match crate::commands::run::stop_control(&control).await {
            crate::commands::StopProof::Stopped => Ok(json!({"result":"stopped","run":run,
                "said":format!("The run stopped: {title} ({count} step{}). Its run-owned processes were confirmed gone. Background work kept for this window stays open.", if count == 1 { "" } else { "s" })})),
            crate::commands::StopProof::StillAlive | crate::commands::StopProof::Unknown =>
                Err("Loadout could not confirm that everything owned by this run stopped. It is not reported as stopped; keep its working folders and inspect the remaining work.".to_owned()),
        }
    }

    pub(super) async fn send_addressed(&self, input: &Value) -> Result<Value, String> {
        let (run, control) = self.addressed_run(input)?;
        let node = input
            .get("node_key")
            .and_then(Value::as_str)
            .ok_or_else(|| "Name the exact node_key from the current run first.".to_owned())?;
        let text = input
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| "Write the message to send to that step.".to_owned())?;
        let reply = crate::commands::step_message::send(&control, &run.run_id, node, text);
        serde_json::to_value(reply)
            .map_err(|_| "Loadout could not describe that delivery.".to_owned())
    }

    pub(super) fn show_control_reply(
        &self,
        answer: Result<Value, String>,
    ) -> crate::bridge::Answer {
        let (line, answer) = match answer {
            Ok(value) => {
                let said = value.get("said").and_then(Value::as_str).map(str::to_owned);
                let line = said.map(|text| Line::Note {
                    agent: LEAD.to_owned(),
                    text,
                    body: Vec::new(),
                });
                (line, crate::bridge::Answer::Ok(value))
            }
            Err(text) => (
                Some(Line::Problem {
                    agent: LEAD.to_owned(),
                    text: text.clone(),
                    resets_at: None,
                }),
                crate::bridge::Answer::Refused(text),
            ),
        };
        if let (Some(lines), Some(line)) = (&self.lines, line) {
            let _ = lines
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .send(line);
        }
        answer
    }
}
