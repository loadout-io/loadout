//! Dwa wejścia do jednego podglądu: narzędzie Leada i zaufany klik historii.
//! Model nie ma dostępu do metody potwierdzającej intencję interfejsu.

use super::{ApprovalScope, ApprovalSubject, ConfirmationBinding, Desk};
use crate::commands::chat::LEAD;
use crate::commands::lead_start::StartRequest;
use crate::engine::line::{Line, RequestedLink, RequestedStep};
use crate::ipc::Sent;
use serde_json::{Value, json};
use std::sync::PoisonError;

impl Desk {
    pub(super) async fn rerun_step(&self, input: &Value) -> Result<Value, String> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Repeat {
            source_run_id: String,
            step_id: String,
        }
        let input:Repeat=serde_json::from_value(input.clone())
            .map_err(|_|"Choose an exact saved run and a whole step tile. A model's confirmed flag is not approval.".to_owned())?;
        // Ten sam podgląd Current zachowuje Part::Just i źródłowy RunRef. Niczego nie
        // uruchamia bez wspólnego ask_the_person → start_replay → trwałego WF-07 ACK.
        self.prepare_replay(
            &json!({"source_run_id":input.source_run_id,"mode":"current",
            "selection":{"kind":"step","step_id":input.step_id}}),
        )
        .await
    }
    pub(crate) async fn prepare_replay(&self, input: &Value) -> Result<Value, String> {
        let home = self
            .home
            .clone()
            .ok_or_else(|| "The current agent library is unavailable.".to_owned())?;
        let project = self.project.clone();
        let input = input.clone();
        let material = tokio::task::spawn_blocking(move || {
            crate::commands::replay::prepare(&home, &project, &input)
        })
        .await
        .map_err(|_| "The saved setup could not be read.".to_owned())??;
        let starts = self
            .starts
            .as_ref()
            .ok_or_else(|| "This window is not connected to the run manager.".to_owned())?;
        let conversation = self
            .conversation
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .to_string();
        let mut preview = material.preview.clone();
        let request = starts.preview(conversation, material)?;
        preview["previewId"] = json!(request.origin.request_id);
        Ok(preview)
    }

    fn replay_request(&self, input: &Value) -> Result<(StartRequest, ApprovalScope), String> {
        let id = input
            .get("preview_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "Review a repeat preview first.".to_owned())?;
        let conversation = *self
            .conversation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let starts = self
            .starts
            .as_ref()
            .ok_or_else(|| "This window is not connected to the run manager.".to_owned())?;
        let request = starts.preview_request(id, &conversation.to_string(), &self.project)?;
        let replay = request
            .replay
            .as_ref()
            .ok_or_else(|| "That request is not a repeat preview.".to_owned())?;
        replay.validate()?;
        let scope = ApprovalScope {
            conversation,
            subject: ApprovalSubject::Replay {
                run: replay.source.clone(),
                preview_id: id.to_owned(),
            },
            operation: "start_replay".to_owned(),
            checkpoint_id: None,
        };
        Ok((request, scope))
    }

    pub(super) async fn ask_replay(&self, input: &Value) -> Result<Value, String> {
        let (request, scope) = self.replay_request(input)?;
        let replay = request
            .replay
            .as_ref()
            .ok_or_else(|| "That repeat is unavailable.".to_owned())?;
        let said = replay
            .preview
            .get("said")
            .and_then(Value::as_str)
            .unwrap_or("Repeat this setup?");
        let budget = replay
            .preview
            .get("budgetSaid")
            .and_then(Value::as_str)
            .unwrap_or("The spending limit is unavailable; review the preview again.");
        let settings = replay
            .preview
            .get("configurationSaid")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("\n");
        let differences = replay
            .preview
            .get("differencesSaid")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("\n");
        let starts = self
            .starts
            .as_ref()
            .ok_or_else(|| "This window is not connected to the run manager.".to_owned())?;
        self.ask_for_approval(scope, format!("{said}\n{budget}\n{differences}\n{settings}\nStart a new run? The original run will stay unchanged."),
            vec!["Start replay".to_owned(), "Keep viewing history".to_owned()], Some("Start replay".to_owned()),
            starts.preview_expires(&request.origin.request_id)).await
    }

    fn activate_replay(
        &self,
        request: &StartRequest,
        scope: &ApprovalScope,
        token: &str,
    ) -> Result<Line, String> {
        if self.waiting.consume(scope, token).is_none() {
            return Err("Ask this person to confirm this exact repeat preview. A model's confirmed flag is not permission.".to_owned());
        }
        let starts = self
            .starts
            .as_ref()
            .ok_or_else(|| "This window is not connected to the run manager.".to_owned())?;
        let active = starts.activate_preview(
            &request.origin.request_id,
            &request.origin.conversation_id,
            &self.project,
        )?;
        let material = active
            .replay
            .as_ref()
            .ok_or_else(|| "That repeat is unavailable.".to_owned())?;
        let workflow = &material.graph;
        Ok(Line::RunRequested {
            agent: LEAD.to_owned(),
            text: format!("Starting {}", workflow.name),
            request_id: active.origin.request_id.clone(),
            conversation_id: active.origin.conversation_id.clone(),
            workspace: active.workspace.to_string_lossy().into_owned(),
            title: workflow.name.clone(),
            file_name: active
                .workflow
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            steps: workflow
                .steps
                .iter()
                .map(|step| {
                    use crate::workflow::Step;
                    let (kind, at, weight) = match step {
                        Step::Agent(one) => (
                            "agent",
                            one.at,
                            if one.weight == crate::workflow::Weight::Heavy {
                                "heavy"
                            } else {
                                "ordinary"
                            },
                        ),
                        Step::Checkpoint(one) => ("checkpoint", one.at, "ordinary"),
                        Step::Check(one) => ("check", one.at, "heavy"),
                        Step::Serve(one) => ("serve", one.at, "ordinary"),
                    };
                    RequestedStep {
                        id: step.id().to_owned(),
                        name: step.name().to_owned(),
                        kind,
                        at,
                        weight,
                    }
                })
                .collect(),
            links: workflow
                .links
                .iter()
                .map(|link| RequestedLink {
                    from: link.from.clone(),
                    to: link.to.clone(),
                    max_turns: link.max_turns,
                })
                .collect(),
            context: Vec::new(),
            context_generation: 0,
        })
    }

    pub(super) async fn start_replay(&self, input: &Value) -> Result<Value, String> {
        let lines = self.lines.as_ref().ok_or_else(|| {
            "Loadout cannot show this run. Reopen the work screen before starting it.".to_owned()
        })?;
        let (request, scope) = self.replay_request(input)?;
        let token = input
            .get("confirmation_token")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let line = self.activate_replay(&request, &scope, token)?;
        let sent = lines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .send(line);
        let starts = self
            .starts
            .as_ref()
            .ok_or_else(|| "This window is not connected to the run manager.".to_owned())?;
        if sent != Sent::Queued {
            starts.refuse(
                &request.origin.request_id,
                "Nothing started: the window could not receive this repeat request.".to_owned(),
            );
        }
        let receipt = starts.wait(&request.origin.request_id).await?;
        Ok(
            json!({"started":true,"requestId":receipt.request_id,"run":receipt.run,
            "said":"Loadout accepted the new run. This is not a completed result."}),
        )
    }

    /// Wyłącznie adapter Tauri wywołany kliknięciem, nigdy dispatch narzędzi Leada.
    /// Ten sam Waiting porównuje pierwotną intencję i wydaje jednorazowy, związany token.
    pub(crate) fn start_replay_from_ui(
        &self,
        preview_id: &str,
        original_intent: String,
    ) -> Result<Line, String> {
        let (request, scope) = self.replay_request(&json!({"preview_id":preview_id}))?;
        let question_id = request.origin.request_id.clone();
        let (ticket, _answer) = self.waiting.park_bound(
            LEAD.to_owned(),
            Some(ConfirmationBinding {
                question_id: question_id.clone(),
                scope: scope.clone(),
                affirmative: Some("Start replay".to_owned()),
            }),
        );
        if !self
            .waiting
            .answer_exact(LEAD, &question_id, original_intent)
        {
            self.waiting.withdraw(&ticket);
            return Err("That repeat was not confirmed. Nothing started.".to_owned());
        }
        let token = self
            .waiting
            .token_for(&question_id)
            .ok_or_else(|| "That repeat was not confirmed. Nothing started.".to_owned())?;
        self.activate_replay(&request, &scope, &token)
    }
}
