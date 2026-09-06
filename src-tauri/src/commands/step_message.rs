//! WF-08: wspólny wynik wiadomości do konkretnej sesji kroku, bez zmiany protokołu vendora.

use serde::Serialize;
use std::sync::PoisonError;

use super::RunControl;
use crate::engine::drivers::{ToAgent, Voice};
use crate::engine::line::Line;

#[derive(Debug, Clone)]
pub(crate) struct SessionChannel {
    pub name: String,
    pub voice: Option<Voice>,
    pub finished: bool,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StepMessageResult {
    AcceptedBySession,
    UnsupportedDuringRun,
    RecipientFinished,
    NoSuchStep,
    StaleRun,
    Disconnected,
    FixedInputs,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepMessageReply {
    pub run_id: String,
    pub node_key: String,
    pub result: StepMessageResult,
    pub said: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepRecipient {
    pub run_id: String,
    pub node_key: String,
    pub agent: String,
    pub can_receive: bool,
    pub finished: bool,
}

pub async fn send(
    control: &RunControl,
    run_id: &str,
    node_key: &str,
    text: &str,
) -> StepMessageReply {
    let address = control.run_address();
    if address.as_ref().is_some_and(|address| address.id == run_id)
        && let Some(reply) = fixed_input_refusal(control, run_id, node_key)
    {
        return reply;
    }
    let session = control
        .inner
        .voices
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(node_key)
        .cloned();
    let name = session
        .as_ref()
        .map_or("That step", |one| one.name.as_str());
    let result = if address.is_none_or(|address| address.id != run_id) {
        StepMessageResult::StaleRun
    } else {
        match &session {
            None => StepMessageResult::NoSuchStep,
            Some(one) if one.finished => StepMessageResult::RecipientFinished,
            Some(one) => match &one.voice {
                None => StepMessageResult::UnsupportedDuringRun,
                Some(voice) => {
                    // Klon sesji wzięty raz przed await. Następna próba/nowy bieg nie może
                    // zostać zastępczym odbiorcą, nawet kiedy ten kanał w międzyczasie zniknie.
                    if voice.send(ToAgent::Turn(text.to_owned())).await.is_ok() {
                        StepMessageResult::AcceptedBySession
                    } else {
                        StepMessageResult::Disconnected
                    }
                }
            },
        }
    };
    let said = match result {
        StepMessageResult::AcceptedBySession => format!("{name}'s session accepted the message."),
        StepMessageResult::UnsupportedDuringRun => {
            format!("{name} does not accept messages while it is running.")
        }
        StepMessageResult::RecipientFinished => format!("{name}'s addressed session has finished."),
        StepMessageResult::NoSuchStep => {
            "There is no created session at that exact step address in this run.".to_owned()
        }
        StepMessageResult::StaleRun => {
            "That run is no longer active here. Read its current status before trying again."
                .to_owned()
        }
        StepMessageResult::Disconnected => {
            format!("{name}'s message channel disconnected before accepting this message.")
        }
        StepMessageResult::FixedInputs => FIXED_INPUTS.to_owned(),
    };
    if result == StepMessageResult::AcceptedBySession {
        let _ = control.show_in_the_run(Line::Told {
            agent: name.to_owned(),
            text: text.to_owned(),
        });
    }
    // 2026-09-05 WF-08: odmowę pokazuje nadawca (Entry albo Desk), nie ten strumień.
    // Dwa echa dublowały fakt, a odmowa starego adresu mogła wpaść do nowego biegu.
    StepMessageReply {
        run_id: run_id.to_owned(),
        node_key: node_key.to_owned(),
        result,
        said,
    }
}

pub fn recipients(control: &RunControl) -> Vec<StepRecipient> {
    let Some(address) = control.run_address() else {
        return Vec::new();
    };
    let external_messages = control.external_messages_allowed();
    control
        .inner
        .voices
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .map(|(key, one)| StepRecipient {
            run_id: address.id.clone(),
            node_key: key.clone(),
            agent: one.name.clone(),
            can_receive: external_messages
                && !one.finished
                && one.voice.as_ref().is_some_and(|voice| !voice.is_closed()),
            finished: one.finished,
        })
        .collect()
}

const FIXED_INPUTS: &str = "This comparison uses fixed inputs. Start a new run to change them.";

/// Wspólna odmowa przed wyborem kanału — także dla dawnego wejścia po nazwie agenta.
pub(crate) fn fixed_input_refusal(
    control: &RunControl,
    run_id: &str,
    node_key: &str,
) -> Option<StepMessageReply> {
    (!control.external_messages_allowed()).then(|| StepMessageReply {
        run_id: run_id.to_owned(),
        node_key: node_key.to_owned(),
        result: StepMessageResult::FixedInputs,
        said: FIXED_INPUTS.to_owned(),
    })
}
