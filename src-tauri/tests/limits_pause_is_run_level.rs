//! AC-3 dla T-21: limit dostawcy pauzuje **bieg**; żaden krok nie zmienia przez to stanu.
//!
//! **Słaba wersja tego kryterium to `assert_eq!(run.status, "paused")`.** Przechodzi ją
//! implementacja, która przy okazji ustawia `paused` na każdym trwającym kroku albo oznacza
//! je `failed` — a to jest dokładnie ten wariant, który `[T7 §7.2]` nazywa błędem: „a pause,
//! not a failure; do not mark steps failed". Na ekranie wygląda to jak bieg, który się wywrócił
//! na limicie, a nie jak bieg, który na niego czeka.
//!
//! Rozstrzygają trzy rzeczy. Porównanie **całych wektorów** statusów kroków przed i po. Asercja,
//! że oba trwające kroki mają przed sobą drogę do `succeeded` **już po wejściu pauzy** — bo
//! pauza wstrzymuje wysyłkę, a nie egzekucję `[T7 §9.3]`, a z `failed` czy `cancelled` tabela
//! przejść nie prowadzi nigdzie. I odmowa jako **wartość**: `Dispatch::Refused(Paused)`, nigdy
//! `Err` (niezmiennik 7).

use std::error::Error;

use loadout_lib::engine::limits::{Dispatch, Gate, Limiter, Refusal, Run, RunStatus};
use loadout_lib::engine::step::{StepEvent, StepState, next};
use serde_json::json;

/// Kiedy limit wraca, według prawdziwej linii z fikstury.
const RESETS_AT: i64 = 1_786_800_600;

/// Kiedy bieg zobaczył zdarzenie — pięć minut wcześniej.
const NOW: i64 = 1_786_800_300;

/// Ile agentów naraz wolno w tym biegu. Wartość bez znaczenia dla kryterium poza jednym:
/// miejsca SĄ wolne, więc odmowa niżej może wyjść wyłącznie z pauzy, nie z pełnej puli.
const AT_ONCE: usize = 3;

/// Ile kroków biegnie w chwili wejścia limitu.
const STILL_RUNNING: usize = 2;

#[tokio::test]
async fn a_provider_limit_pauses_the_run_and_leaves_every_step_where_it_was()
-> Result<(), Box<dyn Error>> {
    let limiter = Limiter::new(AT_ONCE);
    let mut run = Run::new(
        limiter,
        &[StepState::Running, StepState::Running, StepState::Ready],
    );
    let before = run.step_states();
    assert_eq!(
        run.status(),
        RunStatus::Running,
        "the run has to start out sending, otherwise the pause below proves nothing"
    );

    let gate = run.saw_rate_limit(&json!({"status": "rejected", "resetsAt": RESETS_AT}), NOW);
    assert_eq!(
        gate,
        Gate::PausedUntil(RESETS_AT),
        "a status other than 'allowed' means there is nothing left to send until the limit \
         comes back, and when it comes back is part of the answer"
    );

    assert_eq!(
        run.status(),
        RunStatus::Paused,
        "pausing is a property of the run: dispatch stops, running work does not"
    );
    assert_eq!(
        run.status().as_str(),
        "paused",
        "the run-level name travels to the file and to the screen unchanged"
    );

    let after = run.step_states();
    assert_eq!(
        before, after,
        "not one step may move because the provider said wait. Marking the running ones failed \
         reads on screen as a run that broke, and marking them anything else loses the only \
         record of what is actually on the machine"
    );

    for state in &after {
        let wire = serde_json::to_value(*state)?;
        assert_ne!(
            wire.as_str(),
            Some("paused"),
            "there is no paused step and there is not going to be one: keeping pause off the \
             step machine removes a whole quadrant of states nobody needs (ARCHITECTURE.md 5)"
        );
    }

    let refused = run.dispatch().await;
    assert!(
        matches!(&refused, Dispatch::Refused(Refusal::Paused)),
        "the next request for a slot has to come back refused, and refused is a value here, \
         not an error — a paused run is not a broken one. It answered {refused:?}"
    );

    let running_now: Vec<StepState> = after
        .iter()
        .copied()
        .filter(|state| *state == StepState::Running)
        .collect();
    assert_eq!(
        running_now.len(),
        STILL_RUNNING,
        "both steps that were running when the limit arrived have to still be running: the \
         pause holds back dispatch, it does not reach into work that already started"
    );
    for state in running_now {
        assert_eq!(
            next(state, StepEvent::ExitOk),
            Some(StepState::Succeeded),
            "and both have to be able to finish successfully from where the pause left them. \
             This is the assertion a run that marks steps failed cannot pass: from failed or \
             cancelled the transition table leads nowhere, so those steps could never report \
             the success they are about to have"
        );
    }

    Ok(())
}
