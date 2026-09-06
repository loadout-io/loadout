//! Ten sam host-issued approval dla przycisku historii i narzędzia Leada.
use super::{ApprovalScope, ApprovalSubject, ConfirmationBinding, Desk};
use crate::commands::chat::LEAD;
use crate::commands::lead_start::StartRequest;
use serde_json::{Value, json};
use std::sync::{Arc, PoisonError};

impl Desk {
    pub(crate) async fn prepare_result_restore(&self, input: &Value) -> Result<Value, String> {
        let material = crate::commands::result_restore::prepare(&self.project, input).await?;
        let starts = self
            .starts
            .as_ref()
            .ok_or_else(|| "This window is not connected to saved results.".to_owned())?;
        let conversation = self
            .conversation
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .to_string();
        let mut preview = material.preview.clone();
        let request = starts.restore_preview(conversation, material)?;
        preview["previewId"] = json!(request.origin.request_id);
        Ok(preview)
    }

    fn restore_request(&self, input: &Value) -> Result<(StartRequest, ApprovalScope), String> {
        let id = input
            .get("preview_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "Review a saved-file preview first.".to_owned())?;
        let conversation = *self
            .conversation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let starts = self
            .starts
            .as_ref()
            .ok_or_else(|| "This window is not connected to saved results.".to_owned())?;
        let request = starts.preview_request(id, &conversation.to_string(), &self.project)?;
        let restore = request
            .restore
            .as_ref()
            .ok_or_else(|| "That request is not a saved-file preview.".to_owned())?;
        restore.validate()?;
        let scope = ApprovalScope {
            conversation,
            subject: ApprovalSubject::Restore {
                run: restore.source.clone(),
                preview_id: id.to_owned(),
            },
            operation: "restore_result".to_owned(),
            checkpoint_id: None,
        };
        Ok((request, scope))
    }

    pub(super) async fn ask_restore(&self, input: &Value) -> Result<Value, String> {
        let (request, scope) = self.restore_request(input)?;
        let restore = request
            .restore
            .as_ref()
            .ok_or_else(|| "That saved-file preview is unavailable.".to_owned())?;
        let said = restore
            .preview
            .get("said")
            .and_then(Value::as_str)
            .unwrap_or("Restore these saved files?");
        let starts = self
            .starts
            .as_ref()
            .ok_or_else(|| "This window is not connected to saved results.".to_owned())?;
        self.ask_for_approval(
            scope,
            said.to_owned(),
            vec![
                "Restore files".to_owned(),
                "Keep viewing history".to_owned(),
            ],
            Some("Restore files".to_owned()),
            starts.preview_expires(&request.origin.request_id),
        )
        .await
    }

    async fn consume_restore(
        &self,
        request: StartRequest,
        scope: ApprovalScope,
        token: &str,
    ) -> Result<Value, String> {
        if self.waiting.consume(&scope, token).is_none() {
            return Err("Ask this person to confirm this exact saved-file preview. A model's confirmed flag is not permission.".to_owned());
        }
        let starts = self
            .starts
            .as_ref()
            .ok_or_else(|| "This window is not connected to saved results.".to_owned())?;
        let request = starts.consume_restore(
            &request.origin.request_id,
            &request.origin.conversation_id,
            &self.project,
        )?;
        let material = Arc::clone(
            request
                .restore
                .as_ref()
                .ok_or_else(|| "That saved-file preview is unavailable.".to_owned())?,
        );
        // Zamknięcie okna nie zwalnia źródła podczas czytania procesu. Zadanie oddaje lock
        // dopiero po skończonym eksporcie i dowodzie śmierci jego procesów.
        tokio::spawn(async move { material.restore().await })
            .await
            .map_err(|_| {
                "The saved-file operation was interrupted. No project files were changed."
                    .to_owned()
            })?
    }

    pub(super) async fn restore_result(&self, input: &Value) -> Result<Value, String> {
        let (request, scope) = self.restore_request(input)?;
        self.consume_restore(
            request,
            scope,
            input
                .get("confirmation_token")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )
        .await
    }

    pub(crate) async fn restore_result_from_ui(
        &self,
        preview_id: &str,
        original_intent: String,
    ) -> Result<Value, String> {
        let (request, scope) = self.restore_request(&json!({"preview_id":preview_id}))?;
        let question_id = request.origin.request_id.clone();
        let (ticket, _answer) = self.waiting.park_bound(
            LEAD.to_owned(),
            Some(ConfirmationBinding {
                question_id: question_id.clone(),
                scope: scope.clone(),
                affirmative: Some("Restore files".to_owned()),
            }),
        );
        if !self
            .waiting
            .answer_exact(LEAD, &question_id, original_intent)
        {
            self.waiting.withdraw(&ticket);
            return Err("Those files were not confirmed. Nothing was restored.".to_owned());
        }
        let token = self
            .waiting
            .token_for(&question_id)
            .ok_or_else(|| "Those files were not confirmed. Nothing was restored.".to_owned())?;
        self.consume_restore(request, scope, &token).await
    }
}
