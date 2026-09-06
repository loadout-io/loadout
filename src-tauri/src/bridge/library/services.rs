//! WF-28: Lead używa tego samego `ServiceAccess`; wyłącznie human Waiting autoryzuje zmianę.
use super::{ApprovalScope, ApprovalSubject, Desk};
use crate::bridge::Call;
use crate::commands::processes::ServiceRef;
use crate::library::agents::ServiceOperation;
use serde_json::{Value, json};
use std::sync::PoisonError;

pub(super) fn accepts(name: &str) -> bool {
    matches!(
        name,
        "service_status" | "service_logs" | "service_start" | "service_restart" | "service_stop"
    )
}

fn operation(name: &str) -> Result<ServiceOperation, String> {
    match name {
        "service_start" => Ok(ServiceOperation::Start),
        "service_restart" => Ok(ServiceOperation::Restart),
        "service_stop" => Ok(ServiceOperation::Stop),
        _ => Err("That app operation has no confirmation available.".to_owned()),
    }
}

impl Desk {
    fn service_scope(&self, service: ServiceRef, operation: &str) -> ApprovalScope {
        ApprovalScope {
            conversation: *self
                .conversation
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
            subject: ApprovalSubject::Service(service),
            operation: operation.to_owned(),
            checkpoint_id: None,
        }
    }

    pub(super) async fn ask_service(&self, input: &Value) -> Result<Value, String> {
        let access = self
            .services
            .as_ref()
            .ok_or_else(|| "This conversation has not been given access to apps.".to_owned())?;
        let name = input
            .get("operation")
            .and_then(Value::as_str)
            .ok_or_else(|| "Name the app operation first.".to_owned())?;
        let (reference, generation) = access
            .approval_subject(input, operation(name)?)
            .map_err(|why| why.to_string())?;
        let (verb, affirmative, keep) = match name {
            "service_start" => ("Start", "Start app", "Leave stopped"),
            "service_restart" => ("Restart", "Restart app", "Keep running"),
            "service_stop" => ("Stop", "Stop app", "Keep running"),
            _ => return Err("That app operation has no confirmation available.".to_owned()),
        };
        let text = format!(
            "{verb} app {} from run {} (instance {})? Only this app will change; the workflow graph will not restart.",
            reference.node_key, reference.run_id, reference.generation
        );
        self.ask_for_approval(
            self.service_scope(reference, name),
            text,
            vec![affirmative.to_owned(), keep.to_owned()],
            Some(affirmative.to_owned()),
            access.approval_expired(&generation),
        )
        .await
    }

    pub(super) async fn service_call(&self, call: &Call) -> Result<Value, String> {
        let access = self
            .services
            .as_ref()
            .ok_or_else(|| "This conversation has not been given access to apps.".to_owned())?;
        if matches!(call.call.as_str(), "service_status" | "service_logs") {
            return access.dispatch(call).await.map_err(|why| why.to_string());
        }
        let input = call
            .input
            .as_object()
            .ok_or_else(|| "Choose one exact configured app first.".to_owned())?;
        if input
            .keys()
            .any(|key| !matches!(key.as_str(), "service" | "approval_token"))
        {
            return Err("An app change needs the person's exact displayed approval, not a model's confirmation flag, command or folder.".to_owned());
        }
        let action = operation(&call.call)?;
        let (reference, _) = access
            .approval_subject(&call.input, action)
            .map_err(|why| why.to_string())?;
        let token = input
            .get("approval_token")
            .and_then(Value::as_str)
            .unwrap_or_default();
        self.waiting.consume(&self.service_scope(reference.clone(), &call.call), token)
            .ok_or_else(|| format!("Ask this person with ask_the_person, operation {} and this exact service reference. Only their displayed confirmation can authorize the change; it can be used once.", call.call))?;
        let mut value = access
            .approved_mutation(&call.input, action)
            .await
            .map_err(|why| why.to_string())?;
        let status = value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let said = if status == "Cancelled" {
            format!(
                "The request for app {} was cancelled. No ready app was confirmed.",
                reference.node_key
            )
        } else {
            let verb = match action {
                ServiceOperation::Start => "started",
                ServiceOperation::Restart => "restarted",
                ServiceOperation::Stop => "stopped",
                _ => "changed",
            };
            format!(
                "App {} from run {} {verb}. Its current status is {status}.",
                reference.node_key, reference.run_id
            )
        };
        value["said"] = json!(said);
        Ok(value)
    }
}
