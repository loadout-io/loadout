//! WF-08: wspólny wynik wiadomości do konkretnej sesji kroku, bez zmiany protokołu vendora.

use serde::Serialize;
use std::collections::VecDeque;
use std::sync::PoisonError;

use super::RunControl;
use crate::engine::drivers::Voice;
use crate::engine::line::Line;

#[derive(Debug, Clone)]
pub(crate) struct SessionChannel {
    pub name: String,
    pub voice: Option<Voice>,
    pub finished: bool,
    pub generation: u64,
    /// L-01: doprecyzowania przyjęte przez **Loadout**, jeszcze niepodane sesji.
    ///
    /// 2026-09-06 (I-01) — do tego dnia wiadomość szła prosto w `Voice`, czyli w kolejkę
    /// vendora, o której bieg nic nie wiedział. Claude zaczynał po pierwszym `result` kolejną
    /// turę, a Loadout właśnie zamykał wejście i zabijał grupę: wiadomość człowieka ginęła
    /// razem z turą, która miała ją obsłużyć. Kolejka po tej stronie jest jedynym miejscem,
    /// z którego bieg **wie**, że ma jeszcze czego oddać, zanim zamknie sesję.
    pub waiting: VecDeque<String>,
    /// L-01: czy ta sesja jeszcze przyjmuje pracę.
    ///
    /// Zdejmowane wyłącznie w [`crate::commands::RunControl::next_turn_or_stop_accepting`],
    /// **w tym samym wzięciu zamka**, w którym stwierdzono pustą kolejkę. Dwie osobne operacje
    /// zostawiłyby okno, w którym wiadomość wchodzi między „kolejka pusta" a „już nie przyjmuję".
    pub accepting: bool,
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
    /// L-01: kolejka tej sesji jest pełna. Jawna odmowa zamiast czekania pod zamkiem.
    QueueFull,
    /// L-01: wiadomość jest dłuższa, niż ta kolejka przyjmuje.
    TooLong,
}

/// Ile doprecyzowań mieści się w kolejce jednej sesji kroku.
///
/// L-01: kolejka jest **Loadouta**, nie vendora, więc musi mieć własny sufit — inaczej okno
/// albo Lead wpisuje nieograniczoną pracę do kroku, który jej nie zobaczy przed końcem biegu.
pub const QUEUE_LIMIT: usize = 8;

/// Sufit rozmiaru jednej wiadomości do żywej sesji, w bajtach.
pub const MESSAGE_LIMIT_BYTES: usize = 8 * 1024;

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

/// L-01: bez `async`, bo od dnia, w którym kolejka jest po stronie Loadouta, przyjęcie
/// wiadomości nie dotyka już transportu. Podaje ją sesji `Live::one_turn`, między turami.
pub fn send(
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
    // L-01: całe rozstrzygnięcie mieści się w JEDNYM wzięciu zamka i nie ma w sobie `await`
    // (niezmiennik 8). Wiadomość ląduje w kolejce biegu; poda ją sesji `Live::one_turn`,
    // dopiero po wyniku bieżącej tury i przed zamknięciem przyjmowania.
    let (result, name) = if address.is_none_or(|address| address.id != run_id) {
        let name = control
            .inner
            .voices
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(node_key)
            .map_or_else(|| "That step".to_owned(), |one| one.name.clone());
        (StepMessageResult::StaleRun, name)
    } else {
        accept_into_the_queue(control, node_key, text)
    };
    let name = name.as_str();
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
        StepMessageResult::QueueFull => {
            format!("{name} already has {QUEUE_LIMIT} messages waiting. Wait for it to answer.")
        }
        StepMessageResult::TooLong => {
            format!("That message is too long to send. Keep it under {MESSAGE_LIMIT_BYTES} bytes.")
        }
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

/// Jedyne miejsce, w którym wiadomość wchodzi do kolejki sesji.
///
/// Zwraca też nazwę kroku, bo zdanie dla człowieka powstaje z tej samej migawki, z której
/// powstał wynik — nazwa odczytana drugim wzięciem zamka mogłaby należeć już do innej próby.
fn accept_into_the_queue(
    control: &RunControl,
    node_key: &str,
    text: &str,
) -> (StepMessageResult, String) {
    let mut sessions = control
        .inner
        .voices
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let Some(session) = sessions.get_mut(node_key) else {
        return (StepMessageResult::NoSuchStep, "That step".to_owned());
    };
    let name = session.name.clone();
    let result = if session.finished || !session.accepting {
        StepMessageResult::RecipientFinished
    } else {
        match &session.voice {
            None => StepMessageResult::UnsupportedDuringRun,
            Some(voice) if voice.is_closed() => StepMessageResult::Disconnected,
            Some(_) if text.len() > MESSAGE_LIMIT_BYTES => StepMessageResult::TooLong,
            Some(_) if session.waiting.len() >= QUEUE_LIMIT => StepMessageResult::QueueFull,
            Some(_) => {
                session.waiting.push_back(text.to_owned());
                StepMessageResult::AcceptedBySession
            }
        }
    };
    (result, name)
}
