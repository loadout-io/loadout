//! AC-7 dla T-02: tabela przejść kroku odrzuca przejścia nielegalne, a `paused` nie jest
//! stanem kroku.
//!
//! Tabela wiążąca stoi w `docs/ARCHITECTURE.md` §5, jej wersja z efektami ubocznymi
//! w [T7 §9.3]. Ten plik jest jej drugą kopią z rozmysłem: tabela przepisana tutaj ręcznie
//! jest modelem referencyjnym, a nie odczytem z implementacji.
//!
//! Słaba wersja to `fn next(_from, ev) -> Option<StepState> { Some(target_of(ev)) }`, która
//! ignoruje stan wejściowy. Przechodzi każdą asercję na przejściach legalnych, a w biegu
//! **pozwala anulować krok, który już się udał** — i wtedy jego dzieci zostają policzone
//! drugi raz. Rozróżniają je cztery przypadki zwracające `None` i odrzucenie `"paused"`.
//!
//! Siedem nazw z drutu to te same siedem wartości, które niesie `CHECK` w kolumnie
//! `steps.status` [T7 §5.4]. Rozjazd między tym enumem a schematem bazy skończyłby się
//! wierszem, którego SQLite nie przyjmie, w trakcie biegu — dlatego sprawdzane są **obie**
//! strony, zapis i odczyt.

use std::error::Error;

use loadout_lib::engine::step::StepEvent::{
    ExitError, ExitOk, InDegreeZero, PermitAcquired, Retry, Timeout, UpstreamCancelled,
    UpstreamFailed, UserCancelled,
};
use loadout_lib::engine::step::StepState::{
    Cancelled, Failed, Pending, Ready, Running, Skipped, Succeeded,
};
use loadout_lib::engine::step::{StepEvent, StepState, next};

/// Każde przejście, które tabela dopuszcza.
const LEGAL: [(StepState, StepEvent, StepState); 11] = [
    (Pending, InDegreeZero, Ready),
    (Pending, UpstreamFailed, Skipped),
    (Pending, UpstreamCancelled, Cancelled),
    (Ready, PermitAcquired, Running),
    (Running, ExitOk, Succeeded),
    (Running, ExitError, Failed),
    (Running, Timeout, Failed),
    (Running, UserCancelled, Cancelled),
    (Failed, Retry, Pending),
    (Cancelled, Retry, Pending),
    (Skipped, Retry, Pending),
];

/// Przejścia, których w tabeli nie ma — i nie chodzi o to, że są rzadkie.
const ILLEGAL: [(StepState, StepEvent); 4] = [
    // Krok, który się udał, nie da się już zatrzymać ani powtórzyć: jego dzieci są policzone.
    (Succeeded, UserCancelled),
    (Succeeded, Retry),
    // Krok zamknięty nie wraca do kolejki przez zdarzenie z jej środka.
    (Cancelled, InDegreeZero),
    (Skipped, PermitAcquired),
];

/// Siedem nazw, które jadą do bazy i z powrotem.
const ON_THE_WIRE: [(&str, StepState); 7] = [
    ("pending", Pending),
    ("ready", Ready),
    ("running", Running),
    ("succeeded", Succeeded),
    ("failed", Failed),
    ("cancelled", Cancelled),
    ("skipped", Skipped),
];

#[test]
fn every_transition_the_table_allows_happens() {
    for (from, event, to) in LEGAL {
        assert_eq!(
            next(from, event),
            Some(to),
            "the table in ARCHITECTURE.md §5 takes {from:?} through {event:?} to {to:?}"
        );
    }
}

#[test]
fn every_transition_the_table_leaves_out_is_refused() {
    for (from, event) in ILLEGAL {
        assert_eq!(
            next(from, event),
            None,
            "{from:?} has nowhere to go on {event:?}. A transition function that reads only \
             the event lets a step that already succeeded be cancelled, and then its children \
             are released a second time"
        );
    }
}

#[test]
fn the_seven_names_round_trip_and_paused_is_refused() -> Result<(), Box<dyn Error>> {
    for (text, state) in ON_THE_WIRE {
        let parsed: StepState = serde_json::from_str(&format!("\"{text}\""))?;
        assert_eq!(
            parsed, state,
            "{text:?} is the name this state carries in the database, so it has to read back \
             as {state:?}"
        );
        assert_eq!(
            serde_json::to_string(&state)?,
            format!("\"{text}\""),
            "{state:?} has to write itself as {text:?}; anything else is a row the CHECK \
             constraint on steps.status will refuse mid-run"
        );
    }

    let paused = serde_json::from_str::<StepState>("\"paused\"");
    assert!(
        paused.is_err(),
        "paused is a state of the RUN, never of a step: pausing stops dispatch and lets \
         running steps finish. Accepting it here puts back the whole quadrant of states that \
         keeping pause off this machine removes [T7 §9.3]"
    );
    Ok(())
}
