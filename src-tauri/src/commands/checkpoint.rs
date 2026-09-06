//! WF-10: dokładne pytanie i oryginalna odpowiedź człowieka; brak drugiego właściciela biegu.
use super::RunControl;
use serde::Serialize;
use std::sync::PoisonError;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointInfo {
    pub checkpoint_id: String,
    pub node_key: String,
    pub question: String,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckpointResult {
    AnswerAccepted,
    StaleRun,
    StaleQuestion,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointReply {
    pub run_id: String,
    pub checkpoint_id: String,
    pub result: CheckpointResult,
    pub said: String,
}

#[derive(Debug)]
pub(super) struct Pending {
    info: CheckpointInfo,
    answer: oneshot::Sender<String>,
    finished: CancellationToken,
}

impl Drop for Pending {
    fn drop(&mut self) {
        self.finished.cancel();
    }
}

#[derive(Debug)]
pub(crate) struct Question {
    pub(crate) info: CheckpointInfo,
    control: RunControl,
    answer: Option<oneshot::Receiver<String>>,
}

impl Question {
    pub(crate) async fn wait(mut self) -> Option<String> {
        let hear = self.answer.take()?;
        let cancel = self.control.cancel_token();
        tokio::select! {
            biased;
            () = cancel.cancelled() => None,
            () = self.control.wait_until_settled() => None,
            answer = hear => answer.ok(),
        }
    }
}

impl Drop for Question {
    fn drop(&mut self) {
        self.control
            .inner
            .checkpoints
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.info.checkpoint_id);
    }
}

/// Wpis PRZED widoczną pauzą; pytanie i jego odpowiedź należą do jednego uchwytu biegu.
pub(crate) fn park(control: &RunControl, node_key: String, question: String) -> Question {
    let info = CheckpointInfo {
        checkpoint_id: uuid::Uuid::now_v7().to_string(),
        node_key,
        question,
        options: Vec::new(),
    };
    let (send, answer) = oneshot::channel();
    control
        .inner
        .checkpoints
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(
            info.checkpoint_id.clone(),
            Pending {
                info: info.clone(),
                answer: send,
                finished: CancellationToken::new(),
            },
        );
    Question {
        info,
        control: control.clone(),
        answer: Some(answer),
    }
}

pub fn answer(
    control: &RunControl,
    run_id: &str,
    checkpoint_id: &str,
    original: &str,
) -> CheckpointReply {
    let result = if control.run_address().is_none_or(|run| run.id != run_id)
        || control.cancel_token().is_cancelled()
    {
        CheckpointResult::StaleRun
    } else {
        // 2026-09-05: zabranie Sender pod jednym krótkim zamkiem jest jedyną akceptacją.
        // Duplikat, odpowiedź UI i zgoda Leada konkurują o TEN SAM wpis, nigdy o „bieżący”.
        let pending = control
            .inner
            .checkpoints
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(checkpoint_id);
        if let Some(mut pending) = pending {
            // Pending ma Drop (wygasza zgodę), dlatego przenosimy Sender przez podmianę.
            let (empty, _) = oneshot::channel();
            let send = std::mem::replace(&mut pending.answer, empty);
            if send.send(original.to_owned()).is_ok() {
                CheckpointResult::AnswerAccepted
            } else {
                CheckpointResult::StaleQuestion
            }
        } else {
            CheckpointResult::StaleQuestion
        }
    };
    let said = match result {
        CheckpointResult::AnswerAccepted => "Your answer was accepted for this question.",
        CheckpointResult::StaleRun => {
            "That run is no longer waiting here. Your answer was not sent to another run."
        }
        CheckpointResult::StaleQuestion => {
            "That question is no longer waiting for an answer. The next question was not changed."
        }
    };
    CheckpointReply {
        run_id: run_id.to_owned(),
        checkpoint_id: checkpoint_id.to_owned(),
        result,
        said: said.to_owned(),
    }
}

pub fn list(control: &RunControl) -> Vec<CheckpointInfo> {
    control
        .inner
        .checkpoints
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .values()
        .map(|pending| pending.info.clone())
        .collect()
}

pub(crate) fn lifetime(control: &RunControl, checkpoint_id: &str) -> Option<CancellationToken> {
    control
        .inner
        .checkpoints
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(checkpoint_id)
        .map(|pending| pending.finished.clone())
}
