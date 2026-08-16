//! AC-5 dla T-21: `status` to jedyne pole, które decyduje o pauzie.
//!
//! **Słaba wersja tego kryterium to test wyłącznie na `{"status":"rejected"}`.** Przechodzi ją
//! implementacja czytająca `overageStatus`, przechodzi czytająca `overageDisabledReason`,
//! a nawet taka, która pauzuje na samą **obecność** klucza `rate_limit_info` — a to
//! zatrzymałoby każdy bieg, bo to zdarzenie stanowi 1,3% normalnego strumienia `[T7 §4.3, V]`.
//! Objawia się to tak, że wszystkie testy przechodzą (pauza po limicie faktycznie działa),
//! a produkt nie uruchamia się nigdy.
//!
//! Rozstrzyga przypadek (a): **dosłowna linia z `docs/research/fixtures/claude-stream.jsonl`**,
//! wzięta z **udanego** biegu. Ma `"status":"allowed"` i jednocześnie `"overageStatus":
//! "rejected"` oraz `"overageDisabledReason":"out_of_credits"` — każde błędne pole daje tam
//! odpowiedź dokładnie przeciwną do wymaganej.

use std::error::Error;
use std::io::{self, Write};
use std::sync::{Arc, Mutex, PoisonError};

use loadout_lib::engine::limits::{Gate, read_gate};
use serde_json::{Value, json};

/// Szesnaście prawdziwych linii z tej maszyny; jedna z nich jest linią limitu.
const FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../docs/research/fixtures/claude-stream.jsonl"
));

/// Po czym poznajemy tę jedną linię.
const RATE_LIMIT_TAG: &str = r#""type":"rate_limit_event""#;

/// Kiedy limit wraca, według fikstury.
const RESETS_AT: i64 = 1_786_800_600;

/// Obiekt `rate_limit_info` wyjęty z prawdziwej linii — nigdy przepisany ręcznie.
///
/// Fikstury z rzeczywistości biją ręcznie pisany JSON, który zawsze dryfuje w stronę
/// optymistyczną: to raport opisał kiedyś to zdarzenie jako płaskie, a CLI nigdy takiego
/// nie wysłało.
fn real_rate_limit_info() -> Result<Value, Box<dyn Error>> {
    let line = FIXTURE
        .lines()
        .find(|line| line.contains(RATE_LIMIT_TAG))
        .ok_or("the fixture holds no rate limit line, so this test would prove nothing")?;
    let event: Value = serde_json::from_str(line)?;
    let info = event
        .get("rate_limit_info")
        .ok_or("the fixture line carries no rate_limit_info object")?;
    Ok(info.clone())
}

/// Zlewka na wpisy dziennika: `tracing` oddaje je wyłącznie przez `io::Write`.
#[derive(Debug)]
struct Scribe(Arc<Mutex<Vec<u8>>>);

impl Write for Scribe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn the_line_from_a_successful_run_keeps_the_run_going() -> Result<(), Box<dyn Error>> {
    let info = real_rate_limit_info()?;

    // Najpierw dowód, że to naprawdę jest pułapka, a nie zwykły przypadek pozytywny: trzy pola
    // obok siebie, dwa ze słowem "rejected", jedno mówiące "allowed".
    assert_eq!(
        info.get("status").and_then(Value::as_str),
        Some("allowed"),
        "the fixture stopped being the trap this criterion is built on"
    );
    assert_eq!(
        info.get("overageStatus").and_then(Value::as_str),
        Some("rejected"),
        "the field that reads like a refusal and is not one has to still be there"
    );
    assert_eq!(
        info.get("overageDisabledReason").and_then(Value::as_str),
        Some("out_of_credits"),
        "and so does the second one"
    );

    assert_eq!(
        read_gate(&info),
        Gate::Open,
        "this is a line from a run that went fine, so the run keeps sending. Reading either of \
         the two fields that say 'rejected' pauses every run at the first one of these events, \
         and these are 1.3% of a normal stream — the product then never starts, while every \
         test about pausing stays green"
    );

    Ok(())
}

#[test]
fn any_status_other_than_allowed_stops_dispatch() {
    let refused = json!({"status": "rejected", "resetsAt": RESETS_AT});
    assert_eq!(
        read_gate(&refused),
        Gate::PausedUntil(RESETS_AT),
        "this is the real refusal, and it carries the instant the run may resume"
    );

    let unheard_of = json!({"status": "soft_limited", "resetsAt": RESETS_AT});
    assert_eq!(
        read_gate(&unheard_of),
        Gate::PausedUntil(RESETS_AT),
        "a value nobody has measured yet has to stop dispatch too: the rule is 'anything other \
         than allowed', not 'equal to rejected'. Vendors add wire values quietly, and the \
         failure of guessing wrong here is a run that keeps spending turns on refusals"
    );
}

#[test]
fn a_shape_without_status_is_written_down_and_dropped() -> Result<(), Box<dyn Error>> {
    let notes = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&notes);
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .with_writer(move || Scribe(Arc::clone(&sink)))
        .finish();

    let gate = tracing::subscriber::with_default(subscriber, || read_gate(&json!({})));

    assert_eq!(
        gate,
        Gate::Open,
        "an object with no status at all is a shape we do not know. Unknown is dropped, never \
         fatal: vendors add fields every week and no run may fall over one (invariant 5)"
    );

    let written = {
        let held = notes.lock().unwrap_or_else(PoisonError::into_inner);
        String::from_utf8_lossy(&held).into_owned()
    };
    assert!(
        !written.is_empty(),
        "dropped is not the same as unnoticed: the line has to reach the debug log, because \
         that log is the only place anybody will look when the shape changes again"
    );
    assert!(
        written.contains("status"),
        "and the note has to name the field that was missing, otherwise it is a line that says \
         something happened. It said: {written:?}"
    );

    Ok(())
}
