//! AC-6 dla T-21: podpowiedź zależy od pamięci, a suwak nie przyjmuje wartości spoza 1–8.
//!
//! **Słaba wersja tego kryterium to `assert_eq!(DEFAULT_AT_ONCE, 3)`.** Przechodzi ją
//! implementacja, która zwraca trzy dla wszystkiego — łącznie z laptopem 8 GB, gdzie trzy
//! agenty to 1,7 GB samego RSS przy zmierzonych 583 MB na agenta `[T7 §7.1, V]`. Stała wartość
//! wygląda w kodzie dokładnie tak samo jak wzór i w recenzji przechodzi bez zatrzymania.
//!
//! Rozstrzygają **cztery punkty wzoru** plus **dwa punkty obcięcia settera**: wartość stała
//! przewraca się na pierwszym z nich. Obcięcie jest po stronie limitera, nie tylko w kontrolce,
//! bo ta liczba wraca też z pliku biegu (`runs.concurrency`) i z zapisanego workflow — a tamtędy
//! nie przechodzi przez żaden suwak.

use loadout_lib::engine::limits::{DEFAULT_AT_ONCE, Limiter, MAX_AT_ONCE, suggested_at_once};

#[test]
fn the_suggestion_is_computed_from_the_memory_the_machine_has() {
    assert_eq!(
        DEFAULT_AT_ONCE, 3,
        "three is what a 16 GB machine holds at 583 MB per agent, and 16 GB is the mainstream \
         case"
    );
    assert_eq!(
        MAX_AT_ONCE, 8,
        "above eight it is not memory that binds any more but the provider's own limit, and a \
         dial that promises ten researchers at once is a dial that promises a frozen laptop"
    );

    assert_eq!(
        suggested_at_once(2),
        1,
        "two gigabytes hold one agent and not two: 583 MB is a Node runtime, not a thin client"
    );
    assert_eq!(suggested_at_once(8), 2, "eight gigabytes hold two");
    assert_eq!(
        suggested_at_once(16),
        4,
        "sixteen hold four — this is the point where a constant three looks right and is not"
    );
    assert_eq!(
        suggested_at_once(64),
        8,
        "sixty-four would compute to sixteen, and sixteen is above the ceiling"
    );
    assert_eq!(
        suggested_at_once(0),
        1,
        "a machine that reports no memory at all still gets a usable answer, never zero agents"
    );
}

#[test]
fn the_dial_refuses_values_outside_one_to_eight() {
    let limiter = Limiter::new(DEFAULT_AT_ONCE);

    limiter.set_at_once(0);
    assert_eq!(
        limiter.at_once(),
        1,
        "zero agents is not a slower run, it is a run that never starts — and stopping a run is \
         what Stop is for, not the dial"
    );

    limiter.set_at_once(99);
    assert_eq!(
        limiter.at_once(),
        MAX_AT_ONCE,
        "and ninety-nine agents at 583 MB each is a machine that stops responding, so the \
         ceiling holds inside the limiter"
    );

    assert_eq!(
        Limiter::new(0).at_once(),
        1,
        "the same clamp has to hold on the way in: this number is read back from the run file \
         and from a saved workflow, where no control ever touched it"
    );
    assert_eq!(
        Limiter::new(99).at_once(),
        MAX_AT_ONCE,
        "a saved workflow with a value from a bigger machine is exactly how a nine-agent run \
         gets started by accident"
    );
}
