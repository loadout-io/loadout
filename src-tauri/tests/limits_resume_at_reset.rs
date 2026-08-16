//! AC-4 dla T-21: wznowienie następuje o `resetsAt`, a `resetsAt` jest w **sekundach**.
//!
//! **Słaba wersja tego kryterium to `advance(Duration::from_secs(3600))` i sprawdzenie, że
//! bieg wznowiony.** Przechodzi ją każda zła jednostka i przechodzi ją implementacja, która
//! wznawia natychmiast. Rozstrzygająca asercja jest dwustronna i mieszka w jednym teście:
//! **czerwona na 299 s, zielona na 300 s.**
//!
//! Do tego warstwa czysta, bo jednostka jest tu całą stawką. `resetsAt` jest uniksowe
//! w sekundach `[T7 §7.2, V]`; ta sama para liczb potraktowana jak milisekundy daje albo
//! wznowienie za 300 000 s, albo natychmiastowe — i jedno, i drugie wygląda na „coś z zegarem",
//! więc szuka się tego godzinami zamiast przeczytać jedną linię.
//!
//! Zegar jest **wirtualny** (`start_paused`), a to wymaga cechy `test-util` tokio, której
//! `features = ["full"]` **nie** zawiera, i której brak daje mylący komunikat o prywatnym polu
//! zgłoszony przy niezwiązanej linii `[T7 §8.1, V]`.

use std::error::Error;
use std::time::Duration;

use loadout_lib::engine::limits::{
    Dispatch, Gate, Limiter, Refusal, Run, RunStatus, duration_until_reset,
};
use loadout_lib::engine::step::StepState;
use serde_json::json;

/// Kiedy limit wraca, dosłownie z fikstury.
const RESETS_AT: i64 = 1_786_800_600;

/// Kiedy bieg zobaczył zdarzenie: pięć minut wcześniej.
const NOW: i64 = 1_786_800_300;

/// Ile dzieli te dwie chwile: pięć minut, bo w sekundach są obie liczby z drutu.
///
/// Zapisane jako minuty, nie jako `from_secs(300)`, bo `clippy::duration_suboptimal_units`
/// nie przepuszcza tej drugiej formy, a bramka lintuje testy z `-D warnings` (2026-08-16).
/// Wartość jest ta sama co do nanosekundy, więc asercja niżej dalej pada dla implementacji,
/// która przeczyta `resetsAt` jako milisekundy — jednostki pilnuje porównanie, nie zapis.
const WAIT: Duration = Duration::from_mins(5);

/// O ile przesuwamy zegar za pierwszym razem: sekunda za mało.
const NEARLY: Duration = Duration::from_secs(299);

/// I ta jedna brakująca sekunda.
const LAST_SECOND: Duration = Duration::from_secs(1);

#[test]
fn the_reset_is_read_as_unix_seconds() {
    assert_eq!(
        duration_until_reset(RESETS_AT, NOW),
        WAIT,
        "these two numbers are Unix seconds five minutes apart. Read as milliseconds the same \
         pair gives 300 ms, which resumes the run immediately and burns the rest of the window \
         on refusals; read the other way round it gives 300000 s, which is three and a half \
         days of a run that looks hung"
    );
    assert_eq!(
        duration_until_reset(RESETS_AT, RESETS_AT),
        Duration::ZERO,
        "at the reset itself there is nothing left to wait for"
    );
    assert_eq!(
        duration_until_reset(RESETS_AT, RESETS_AT + 60),
        Duration::ZERO,
        "a limit that came back a minute ago is not a negative wait: Duration has no sign, so \
         subtracting the other way round is a panic in the engine, and a panic in an agent \
         runtime takes the whole run with it"
    );
}

#[tokio::test(start_paused = true)]
async fn the_run_comes_back_at_the_reset_and_not_a_second_earlier() -> Result<(), Box<dyn Error>> {
    let limiter = Limiter::new(2);
    let mut run = Run::new(limiter, &[StepState::Running, StepState::Ready]);
    let steps_before = run.step_states();
    let attempts_before = run.attempts();

    let gate = run.saw_rate_limit(&json!({"status": "rejected", "resetsAt": RESETS_AT}), NOW);
    assert_eq!(
        gate,
        Gate::PausedUntil(RESETS_AT),
        "the run has to know when it may send again, and that instant comes from the wire"
    );

    tokio::time::advance(NEARLY).await;
    assert_eq!(
        run.status(),
        RunStatus::Paused,
        "one second before the reset the run is still waiting. A run that is back here read \
         the unit wrong, or never really waited"
    );
    let early = run.dispatch().await;
    assert!(
        matches!(&early, Dispatch::Refused(Refusal::Paused)),
        "and while it waits it still refuses to send, because a status that says 'running' \
         while nothing may go out is the same lie in the other direction. It answered {early:?}"
    );

    tokio::time::advance(LAST_SECOND).await;
    assert_eq!(
        run.status(),
        RunStatus::Running,
        "at the reset the run comes back on its own — nobody presses anything"
    );
    let granted = run.dispatch().await;
    assert!(
        matches!(&granted, Dispatch::Granted(_)),
        "and the first request for a slot after the reset is met. It answered {granted:?}"
    );

    assert_eq!(
        run.step_states(),
        steps_before,
        "coming back from a pause is not an event in any step's life: dispatch resumes, and \
         that is the whole of it"
    );
    assert_eq!(
        run.attempts(),
        attempts_before,
        "and nothing is retried, so no step is on its second try. A pause counted as a failed \
         try spends the user's real retries on the provider's clock"
    );

    Ok(())
}
