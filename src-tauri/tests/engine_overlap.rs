//! AC-1 dla T-02: dwa niezależne kroki zajmują **nachodzące na siebie** okna czasu, a przy
//! limicie 1 nie zachodzą wcale.
//!
//! To jest kryterium, dla którego całe zadanie istnieje. poprzedni prototyp miał `max_parallel`, miał
//! zielone testy i **nigdy nie uruchomił dwóch agentów naraz**: `max_parallel` było tylko
//! szerokością wysyłki — jeden worker, cztery „równoległe" pasy w rozłącznych oknach po ~0,5 s
//! [raport 01 §7.3]. Żaden test tego nie złapał, bo każdy pytał „czy oba się skończyły",
//! a oba się skończyły.
//!
//! Dlatego mierzone są **przedziały**, nie czas trwania biegu. Słaba wersja tego kryterium —
//! `assert!(wszystkie Succeeded)` plus `assert!(elapsed < 2 * STEP)` — przechodzi dokładnie na
//! tej implementacji, którą to kryterium ma odrzucić: oba kroki naprawdę się skończyły, a suma
//! dwóch snów po 300 ms bywa poniżej progu na obciążonej maszynie. Rozróżniają je zapisane
//! przedziały **i bieg kontrolny z limitem 1 w tym samym pliku** — jedna stała nie zaspokoi
//! obu naraz.
//!
//! Runtime jest **wielowątkowy z prawdziwymi snami**, nigdy `start_paused`: czas wirtualny
//! implikuje runtime jednowątkowy i przeskakuje do przodu, kiedy runtime staje bezczynny,
//! więc „nakładanie się" przestaje cokolwiek znaczyć [T7 §8.1].

use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use loadout_lib::engine::dag::Dag;
use loadout_lib::engine::fake::{Behaviour, FakeDriver, Recorder};
use loadout_lib::engine::scheduler::execute;
use loadout_lib::engine::step::StepState;
use tokio_util::sync::CancellationToken;

/// Ile trwa każdy z dwóch kroków.
const STEP: Duration = Duration::from_millis(300);

/// Ile z tego musi być wspólne. Połowa kroku: prawdziwa równoległość daje tu prawie całe
/// 300 ms, a wysyłka szeregowa daje zero — próg w połowie nie rozstrzyga niczego na styk
/// i nie zależy od tego, jak szybko maszyna wystartuje drugie zadanie.
const MIN_OVERLAP: Duration = Duration::from_millis(150);

/// Okno czasu jednego kroku.
type Span = (Instant, Instant);

/// Jeden bieg dwóch niezależnych kroków przy zadanym limicie.
///
/// Zwraca oba okna i stany końcowe. Graf jest ten sam w obu biegach — różni je **wyłącznie**
/// limit, więc każda różnica w wyniku jest różnicą w planiście, a nie w danych.
async fn two_independent_steps(
    limit: usize,
) -> Result<(Span, Span, Vec<StepState>), Box<dyn Error>> {
    let dag = Dag::new(2, &[])?;
    let recorder = Arc::new(Recorder::new());
    let driver = FakeDriver::new(Arc::clone(&recorder), vec![Behaviour::Busy(STEP); 2]);

    let outcome = execute(&dag, limit, CancellationToken::new(), move |step, cancel| {
        driver.clone().run(step, cancel)
    })
    .await;

    let first = recorder
        .span(0)
        .ok_or("step 0 never entered and left the driver, so there is no window to compare")?;
    let second = recorder
        .span(1)
        .ok_or("step 1 never entered and left the driver, so there is no window to compare")?;
    Ok((first, second, outcome.states))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_ready_steps_share_a_window_when_two_may_run_at_once() -> Result<(), Box<dyn Error>> {
    let ((start_a, end_a), (start_b, end_b), states) = two_independent_steps(2).await?;

    // Przecięcie dwóch przedziałów: od późniejszego startu do wcześniejszego końca.
    // `saturating_duration_since` daje zero, kiedy przecięcie jest puste — czyli dokładnie
    // wtedy, kiedy kroki biegły jeden po drugim.
    let shared = end_a.min(end_b).saturating_duration_since(start_a.max(start_b));

    assert!(
        shared >= MIN_OVERLAP,
        "two independent steps have to occupy overlapping windows when the limit is two; \
         these two shared {shared:?} of a {STEP:?} step, and anything near zero is a scheduler \
         that dispatches wide but runs one at a time — the defect this whole task exists to \
         rule out"
    );
    assert!(
        states.iter().all(|state| *state == StepState::Succeeded),
        "both steps have to finish successfully for the measured windows to mean anything; \
         they ended as {states:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_same_steps_never_share_a_window_at_a_limit_of_one() -> Result<(), Box<dyn Error>> {
    let ((start_a, end_a), (start_b, end_b), states) = two_independent_steps(1).await?;

    let latest_start = start_a.max(start_b);
    let earliest_end = end_a.min(end_b);

    assert!(
        latest_start >= earliest_end,
        "at a limit of one the two windows have to be disjoint: the second step may not begin \
         before the first one ends. It began {:?} before — which is a limit that bounds \
         dispatch rather than execution",
        earliest_end.saturating_duration_since(latest_start)
    );
    assert!(
        states.iter().all(|state| *state == StepState::Succeeded),
        "a limit of one slows the run down; it does not change how it ends. These ended as \
         {states:?}"
    );
    Ok(())
}
