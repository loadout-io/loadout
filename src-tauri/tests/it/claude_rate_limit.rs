//! AC-5 dla T-04: `rate_limit_event` jest czytany w swoim prawdziwym, **zagnieżdżonym**
//! kształcie.
//!
//! Raport T1 §4.5 opisał to zdarzenie jako płaskie i **to jest nieprawda** — korekta 3
//! z tego samego raportu podaje prawdziwą linię: pola siedzą w `rate_limit_info`. Parser
//! napisany pod płaski kształt „po cichu nie widzi nic": deserializacja się udaje, zdarzenia
//! nie ma, banner się nie pokazuje, bieg nie pauzuje — i dowiadujesz się o tym z rachunku.
//!
//! **Słaba wersja tego kryterium to `assert!(matches!(ev, AgentEvent::Notice { .. }))`.**
//! Przechodzi ją sterownik, który na każde nierozpoznane zdarzenie systemowe wypuszcza
//! `Notice` z pustym tekstem, i przechodzi ją parser napisany pod płaski kształt. Rozróżnia
//! asercja na **dokładnej** wartości `resets_at` oraz przypadek płaski, któremu nie wolno
//! wyprodukować zera: „limit wraca o 01:00 czasu uniksowego 1970" jest gorsze niż brak
//! bannera, bo wygląda na odpowiedź.

use std::error::Error;

use loadout_lib::engine::drivers::AgentEvent;
use loadout_lib::engine::drivers::claude::ClaudeDecoder;

/// Szesnaście prawdziwych linii z tej maszyny; jedna z nich jest linią limitu.
const FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../docs/research/fixtures/claude-stream.jsonl"
));

/// Po czym poznajemy linię limitu w fiksturze.
const RATE_LIMIT_TAG: &str = r#""type":"rate_limit_event""#;

/// Ten sam zestaw kluczy, tylko **bez** koperty `rate_limit_info` — czyli kształt, który
/// raport opisał, a którego CLI nigdy nie wysłało.
const FLAT_SHAPE: &str = r#"{"type":"rate_limit_event","status":"allowed","resetsAt":1786800600,"rateLimitType":"five_hour"}"#;

/// Kiedy limit wraca, według fikstury.
const RESETS_AT: i64 = 1_786_800_600;

/// Które okno limitu.
const WINDOW: &str = "five_hour";

/// Stan, przy którym bieg leci dalej.
const ALLOWED: &str = "allowed";

/// Stan, przy którym nie ma już czego wysyłać. Wartości spoza `allowed` nie są znane
/// z pomiaru — kontrakt brzmi „cokolwiek innego niż `allowed` zatrzymuje bieg", więc taka
/// jest też asercja.
const EXHAUSTED: &str = "exhausted";

/// Prawdziwa linia limitu z fikstury.
fn real_line() -> Result<&'static str, Box<dyn Error>> {
    let line = FIXTURE
        .lines()
        .find(|line| line.contains(RATE_LIMIT_TAG))
        .ok_or("the fixture holds no rate limit line, so this test proves nothing")?;
    Ok(line)
}

/// Wszystkie zdarzenia limitu, które dekoder wypuścił dla jednej linii.
fn limits_from(line: &str) -> Vec<AgentEvent> {
    let mut decoder = ClaudeDecoder::new();
    decoder
        .push(line)
        .into_iter()
        .filter(|event| matches!(event, AgentEvent::RateLimit { .. }))
        .collect()
}

#[test]
fn the_nested_shape_is_read_field_by_field() -> Result<(), Box<dyn Error>> {
    let limits = limits_from(real_line()?);

    assert_eq!(
        limits.len(),
        1,
        "the real line has to produce exactly one rate limit event; it produced {limits:?}"
    );
    let Some(AgentEvent::RateLimit {
        status,
        resets_at,
        rate_limit_type,
        ..
    }) = limits.first()
    else {
        return Err(format!("expected a rate limit event, got {limits:?}").into());
    };

    assert_eq!(
        status.as_str(),
        ALLOWED,
        "this run was still allowed to send, and that is the difference between a banner and \
         a paused run"
    );
    assert_eq!(
        *resets_at, RESETS_AT,
        "this exact number is the sentence the user reads - 'Claude limit reached, resets \
         5:30 AM'. A parser written for the flat shape deserializes happily and leaves it at \
         zero, which is 1970 and reads as an answer"
    );
    assert_eq!(
        rate_limit_type.as_str(),
        WINDOW,
        "which window ran out decides how long the wait is"
    );

    Ok(())
}

#[test]
fn the_flat_shape_never_answers_with_a_default() {
    let limits = limits_from(FLAT_SHAPE);

    // Świadomie szeroki kontrakt: ta linia może nie dać nic albo policzyć się jako
    // nierozpoznana. Czego nie wolno, to udać, że coś wiadomo.
    let defaulted: Vec<&AgentEvent> = limits
        .iter()
        .filter(|event| matches!(event, AgentEvent::RateLimit { resets_at: 0, .. }))
        .collect();
    assert!(
        defaulted.is_empty(),
        "the shape T1 described in section 4.5 is not the shape the CLI sends - correction 3 \
         has the real line, and the fields are nested. A decoder that answers this one with \
         zeroes is a decoder that would have answered the real line with zeroes too, and the \
         run would never pause. It produced {defaulted:?}"
    );
}

#[test]
fn a_status_other_than_allowed_asks_the_run_to_stop() -> Result<(), Box<dyn Error>> {
    let allowed = format!(r#""status":"{ALLOWED}""#);
    let spent = format!(r#""status":"{EXHAUSTED}""#);
    let line = real_line()?.replace(&allowed, &spent);
    assert_ne!(
        line,
        real_line()?,
        "the substitution has to actually change the line, otherwise this test measures the \
         allowed case twice"
    );

    let limits = limits_from(&line);
    let Some(AgentEvent::RateLimit {
        status, pause_run, ..
    }) = limits.first()
    else {
        return Err(format!("expected a rate limit event, got {limits:?}").into());
    };

    assert_eq!(
        status.as_str(),
        EXHAUSTED,
        "the status travels through unchanged"
    );
    assert!(
        *pause_run,
        "anything other than 'allowed' means there is nothing left to send, so the run has to \
         be told to stop rather than keep spending turns on refusals. Pausing itself is T-21's \
         job; saying so is this driver's"
    );

    Ok(())
}
