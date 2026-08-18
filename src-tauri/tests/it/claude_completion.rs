//! AC-4 dla T-04: zakończenie czyta `is_error` i `terminal_reason`, nigdy `subtype`.
//!
//! **Słaba wersja tego kryterium to `match subtype { "success" => Completed, _ => Failed(_) }`
//! z testem, w którym przypadek nieudany ma `subtype != "success"`.** Przechodzi wszystko
//! i odwraca wynik dokładnie tam, gdzie to boli: krok, który padł na błędzie API, jedzie dalej
//! jako udany, a jego przekazanie jest puste — stożek poniżej rusza na pustce i nikt nie wie
//! dlaczego.
//!
//! Rozróżnia pierwszy przypadek z listy. To jest **prawdziwa linia z nieudanego biegu**
//! `--bare` na tej maszynie: `"subtype":"success"` przy `"is_error":true`
//! i `"terminal_reason":"api_error"` [T1 §4.4, potwierdzone ponownie]. `subtype` mówi
//! „success", a kryterium żąda `ok == false`.
//!
//! Piąty przypadek jest o czymś innym: strumień, który skończył się **bez** linii `result`.
//! Wyjście procesu jest sygnałem drugorzędnym [T1 §8.5] — proces, który wyszedł czysto i nie
//! powiedział, co zrobił, nie ma czego przekazać dalej, więc kod 0 nie czyni z tego sukcesu.

use std::error::Error;

use loadout_lib::engine::drivers::claude::ClaudeDecoder;
use loadout_lib::engine::drivers::{AgentEvent, FinishReason, Outcome};

/// Szesnaście prawdziwych linii z tej maszyny.
const FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../docs/research/fixtures/claude-stream.jsonl"
));

/// Po czym poznajemy linię wyniku: klucz `type` stoi w niej na końcu obiektu.
const RESULT_TAG: &str = r#""type":"result""#;

/// Nieudany bieg, który melduje się jako udany. Linia w tym kształcie przyszła z prawdziwego
/// biegu `--bare`.
const LIED_ABOUT_SUCCESS: &str = r#"{"type":"result","subtype":"success","is_error":true,"terminal_reason":"api_error","result":"Not logged in"}"#;

/// Bieg, który naprawdę się udał.
const REALLY_DONE: &str = r#"{"type":"result","subtype":"success","is_error":false,"terminal_reason":"completed","result":"done"}"#;

/// Bieg zdjęty przerwaniem w paśmie — dokładnie ten kształt oddaje CLI po `control_request`.
const INTERRUPTED: &str = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"terminal_reason":"cancelled","result":"interrupted"}"#;

/// Bieg, który uderzył w sufit tur. `terminal_reason` **celowo nie ma**: pola nieistotne są
/// opcjonalne, a wynik nie ma prawa zależeć od tego, czy vendor akurat je dosłał
/// (niezmiennik 5).
const HIT_THE_CEILING: &str =
    r#"{"type":"result","subtype":"error_max_turns","is_error":true,"result":"turn limit"}"#;

/// Sesja z fikstury.
const FIXTURE_SESSION: &str = "d24ee572-640c-4442-9c15-587dff952b98";

/// Koszt z fikstury. Na drucie stoi `0.14836290000000002`, więc porównanie idzie przez
/// tolerancję — dwie różne dziesiętne reprezentacje tej samej kwoty to nie jest regresja.
const FIXTURE_COST: f64 = 0.148_362_9;

/// Ile wolno się rozjechać kosztowi. Rzędy wielkości poniżej najmniejszej kwoty, jaką
/// vendor kiedykolwiek wystawi, i rzędy wielkości powyżej szumu binarnego.
const COST_EPSILON: f64 = 1e-9;

/// Przepuszcza jedną linię przez świeży dekoder i wyjmuje z niej wynik tury.
fn outcome_of(line: &str) -> Result<Outcome, Box<dyn Error>> {
    let mut decoder = ClaudeDecoder::new();
    for event in decoder.push(line) {
        if let AgentEvent::Finished(outcome) = event {
            return Ok(outcome);
        }
    }
    Err(format!("this line ended no turn at all: {line}").into())
}

#[test]
fn a_failed_run_that_calls_itself_success_is_still_a_failure() -> Result<(), Box<dyn Error>> {
    let outcome = outcome_of(LIED_ABOUT_SUCCESS)?;

    assert!(
        !outcome.ok,
        "this is the real line from a failed run: subtype says success, is_error says true, \
         terminal_reason says api_error. A driver that branches on subtype reports a step that \
         did nothing as done, and everything downstream starts from an empty handoff. \
         It came out as {outcome:?}"
    );
    assert!(
        matches!(outcome.reason, FinishReason::Failed(_)),
        "the reason has to carry something a person can read, because this is the case where \
         somebody will ask why. It came out as {:?}",
        outcome.reason
    );

    Ok(())
}

#[test]
fn the_three_other_endings_each_get_their_own_reason() -> Result<(), Box<dyn Error>> {
    let done = outcome_of(REALLY_DONE)?;
    assert!(done.ok, "is_error false is the only thing that means done");
    assert_eq!(
        done.reason,
        FinishReason::Completed,
        "a run that really finished has to read as finished"
    );

    let stopped = outcome_of(INTERRUPTED)?;
    assert_eq!(
        stopped.reason,
        FinishReason::Cancelled,
        "cancelling is a value, never an error: a step somebody stopped on purpose must not \
         read the same as a step that broke. It came out as {:?}",
        stopped.reason
    );

    let ceiling = outcome_of(HIT_THE_CEILING)?;
    assert_eq!(
        ceiling.reason,
        FinishReason::LimitReached,
        "hitting a ceiling is its own answer - and this line carries no terminal_reason at \
         all, so a driver that treats that field as mandatory gets it wrong. It came out as \
         {:?}",
        ceiling.reason
    );

    Ok(())
}

#[test]
fn a_stream_that_ends_without_a_result_is_a_failure_even_on_exit_zero() -> Result<(), Box<dyn Error>>
{
    let lines: Vec<&str> = FIXTURE.lines().collect();
    let cut = lines
        .iter()
        .position(|line| line.contains(RESULT_TAG))
        .ok_or("the fixture holds no result line")?;

    let mut decoder = ClaudeDecoder::new();
    for line in &lines[..cut] {
        decoder.push(line);
    }

    let ending = decoder
        // Skarga pusta: ten strumien skonczyl sie czysto, agent nie narzekal na nic.
        .end_of_stream(Some(0), "")
        .ok_or("a stream that never said how it went still has to end the turn somehow")?;
    let AgentEvent::Finished(outcome) = &ending else {
        return Err(format!("the end of a stream has to end the turn; it gave {ending:?}").into());
    };

    assert!(
        !outcome.ok,
        "a process that exited cleanly and never said what it did has nothing to hand on. \
         Process exit is the secondary signal here, the result event is the primary one. \
         It came out as {outcome:?}"
    );
    let FinishReason::Failed(why) = &outcome.reason else {
        return Err(format!(
            "expected a failure with a readable reason, got {:?}",
            outcome.reason
        )
        .into());
    };
    assert!(
        why.to_lowercase().contains("result"),
        "the reason has to name what was missing - the result event - because that is the one \
         sentence that tells whoever reads it where to look. It said {why:?}"
    );

    Ok(())
}

#[test]
fn the_fixture_hands_over_its_own_session_cost_and_tokens() -> Result<(), Box<dyn Error>> {
    let line = FIXTURE
        .lines()
        .find(|line| line.contains(RESULT_TAG))
        .ok_or("the fixture holds no result line")?;
    let outcome = outcome_of(line)?;

    assert!(outcome.ok, "the fixture run really did succeed");
    assert_eq!(
        outcome.session.id, FIXTURE_SESSION,
        "the session id is what the next turn resumes and what T-06 stores next to the step"
    );
    assert_eq!(outcome.turns, 2, "the fixture run took two turns");

    let cost = outcome
        .cost_usd
        .ok_or("the fixture carries a cost, so None here means it was dropped on the floor")?;
    assert!(
        (cost - FIXTURE_COST).abs() < COST_EPSILON,
        "the run cost {FIXTURE_COST} and this is the number the user is shown and billed for; \
         it came out as {cost}"
    );

    assert_eq!(outcome.tokens.input, 4, "fresh input from the wire");
    assert_eq!(
        outcome.tokens.cached, 65_403,
        "cached input is the number that says whether context isolation is working at all; \
         reading the wrong usage field here makes that measurement silently meaningless"
    );
    assert_eq!(outcome.tokens.output, 336, "output from the wire");

    Ok(())
}
