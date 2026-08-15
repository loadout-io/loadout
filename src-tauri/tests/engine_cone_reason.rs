//! AC-5 dla T-02: poniżej `failed` jest `skipped`, poniżej `cancelled` jest `cancelled`,
//! a status terminalny nie jest nadpisywany.
//!
//! To jest defekt z [T7 §2.4], znaleziony przez test i wypisany tam z wektorem stanów:
//! prototyp oznaczał **wszystko** poniżej anulowanego kroku jako `Skipped`, więc po świadomym
//! naciśnięciu Stop UI tłumaczyłoby ośmiu krokom, że „ktoś wyżej padł". To nie jest kosmetyka:
//! „pominięty" i „zatrzymany przez ciebie" prowadzą użytkownika do dwóch różnych następnych
//! ruchów.
//!
//! Słaba wersja — `assert!(matches!(states[2], Skipped | Cancelled))` albo
//! `assert_ne!(states[2], Succeeded)` — przechodzi dokładnie na tym defekcie. Rozróżnia je
//! `assert_eq!` na konkretnym wariancie w scenariuszu A **i** w B, w jednym pliku: jedna stała
//! nie zaspokoi obu naraz.
//!
//! Scenariusz C rozstrzyga remis. Reguła: **wygrywa powód, który wystąpił pierwszy; status
//! terminalny nigdy nie jest przepisywany.**

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use loadout_lib::engine::dag::Dag;
use loadout_lib::engine::fake::{Behaviour, FakeDriver, Recorder};
use loadout_lib::engine::scheduler::execute;
use loadout_lib::engine::step::StepState::{Cancelled, Failed, Skipped, Succeeded};
use tokio_util::sync::CancellationToken;

/// Po ilu milisekundach pada Stop. Kroki natychmiastowe zdążą się skończyć wcześniej, kroki
/// wiszące nie zdążą — i o tę różnicę w tych scenariuszach chodzi.
const CANCEL_AFTER: Duration = Duration::from_millis(150);

/// Ustawia anulowanie na później i oddaje token do biegu.
fn stop_after(delay: Duration) -> CancellationToken {
    let token = CancellationToken::new();
    let trigger = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        trigger.cancel();
    });
    token
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn below_a_failed_step_the_cone_is_skipped() -> Result<(), Box<dyn Error>> {
    // 0→1→2, 0→3, 4→5 — dwie rozłączne gałęzie, żeby było widać, że stożek nie sięga dalej,
    // niż powinien.
    let dag = Dag::new(6, &[(0, 1), (1, 2), (0, 3), (4, 5)])?;
    let mut behaviours = vec![Behaviour::Succeed; 6];
    behaviours[1] = Behaviour::Fail;

    let recorder = Arc::new(Recorder::new());
    let driver = FakeDriver::new(Arc::clone(&recorder), behaviours);
    let outcome = execute(&dag, 2, CancellationToken::new(), move |step, cancel| {
        driver.clone().run(step, cancel)
    })
    .await;

    assert_eq!(
        outcome.states,
        vec![Succeeded, Failed, Skipped, Succeeded, Succeeded, Succeeded],
        "step 2 is the only one below the failure, so it is the only one that gets skipped; \
         step 3 hangs off the same parent and step 5 is on a branch of its own"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn below_a_cancelled_step_the_cone_is_cancelled_not_skipped() -> Result<(), Box<dyn Error>> {
    // 0→1→2, 0→3. Krok 1 wisi, krok 3 kończy się natychmiast — więc w chwili Stopu gałąź 3
    // jest już zamknięta i widać, że anulowanie nie maluje na ślepo.
    let dag = Dag::new(4, &[(0, 1), (1, 2), (0, 3)])?;
    let mut behaviours = vec![Behaviour::Succeed; 4];
    behaviours[1] = Behaviour::Hang;

    let recorder = Arc::new(Recorder::new());
    let driver = FakeDriver::new(Arc::clone(&recorder), behaviours);
    let outcome = execute(&dag, 2, stop_after(CANCEL_AFTER), move |step, cancel| {
        driver.clone().run(step, cancel)
    })
    .await;

    assert_eq!(
        outcome.states,
        vec![Succeeded, Cancelled, Cancelled, Succeeded],
        "step 2 has to read as Cancelled, never as Skipped: the user pressed Stop, nobody \
         above it broke. Reporting Skipped here is the T7 §2.4 defect, where the UI explains \
         a deliberate stop to eight steps as somebody else's failure"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_reason_that_came_first_wins_a_tie() -> Result<(), Box<dyn Error>> {
    // 0→2, 1→2. Krok 0 pada od razu, krok 1 wisi do Stopu. Węzeł 2 ma więc dwa powody, żeby
    // się nie odbyć, i dzieli je 150 ms.
    let dag = Dag::new(3, &[(0, 2), (1, 2)])?;
    let behaviours = vec![Behaviour::Fail, Behaviour::Hang, Behaviour::Succeed];

    let recorder = Arc::new(Recorder::new());
    let driver = FakeDriver::new(Arc::clone(&recorder), behaviours);
    let outcome = execute(&dag, 2, stop_after(CANCEL_AFTER), move |step, cancel| {
        driver.clone().run(step, cancel)
    })
    .await;

    assert_eq!(
        outcome.states,
        vec![Failed, Cancelled, Skipped],
        "the failure landed on step 2 first and a terminal status is never rewritten, so the \
         later cancellation of step 1 may not repaint it. Anything else means the last writer \
         wins, and then the reason a step did not run depends on scheduling"
    );
    Ok(())
}
