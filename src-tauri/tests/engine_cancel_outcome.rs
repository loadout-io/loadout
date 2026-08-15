//! AC-6 dla T-02: anulowanie jest wartością, dociera do środka kroku, zostawia każdy węzeł
//! w stanie końcowym i **nie wycieka do następnego biegu**.
//!
//! Słaba wersja to samo `assert!(elapsed < 1 s)`. Przechodzi ją `JoinSet::abort_all`, czyli
//! zdjęcie zadań Rusta bez powiadomienia kroku — a w T-03 ten sam kształt zostawia żywy proces
//! systemowy, który dalej pali limit u dostawcy [T7 §3.1]. Rozróżnia je wpis `CancelSeen`:
//! token musiał wejść **do wnętrza** kroku.
//!
//! Drugi bieg na świeżym tokenie jest tu z powodu niezmiennika 7. Globalny `AtomicBool`
//! przecieka między biegami, więc bieg po anulowanym startuje jako już anulowany i kończy się
//! w milisekundach z samymi `Cancelled` — a to wygląda jak szybki bieg, nie jak awaria.
//! Oba biegi są w **jednej** funkcji i po kolei: rozdzielone na dwa `#[test]` poszłyby
//! równolegle i przeciek przestałby być widoczny.

use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use loadout_lib::engine::dag::Dag;
use loadout_lib::engine::fake::{Behaviour, FakeDriver, Recorder};
use loadout_lib::engine::scheduler::{execute, Outcome};
use loadout_lib::engine::step::StepState;
use tokio_util::sync::CancellationToken;

/// Po ilu milisekundach pada Stop.
const CANCEL_AFTER: Duration = Duration::from_millis(100);
/// Ile najdłużej wolno wracać z anulowanego biegu. Krok wisi 30 s, więc każda wartość poniżej
/// sekundy dowodzi, że nie czekano na jego naturalny koniec.
const PATIENCE: Duration = Duration::from_secs(1);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stop_reaches_the_step_and_does_not_leak() -> Result<(), Box<dyn Error>> {
    let dag = Dag::new(3, &[(0, 1), (1, 2)])?;

    // ── Bieg pierwszy: Stop w środku kroku 0 ───────────────────────────────────────────────
    let recorder = Arc::new(Recorder::new());
    let driver = FakeDriver::new(
        Arc::clone(&recorder),
        vec![Behaviour::Hang, Behaviour::Succeed, Behaviour::Succeed],
    );
    let token = CancellationToken::new();
    let trigger = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(CANCEL_AFTER).await;
        trigger.cancel();
    });

    let started = Instant::now();
    // Adnotacja typu jest tu asercją kompilatora, nie ozdobą: `Result<_, Cancelled>` nie
    // wpasuje się w `Outcome`. Anulowanie jest wartością, nie błędem (niezmiennik 7).
    let outcome: Outcome = execute(&dag, 1, token, move |step, cancel| {
        driver.clone().run(step, cancel)
    })
    .await;
    let elapsed = started.elapsed();

    assert!(
        elapsed < PATIENCE,
        "a cancelled run has to come back at once; this one took {elapsed:?} while step 0 was \
         sleeping for far longer, so the run waited the step out instead of stopping it"
    );
    assert!(
        outcome.cancelled,
        "the run was stopped by the user and has to say so; it came back as {outcome:?}"
    );
    assert!(
        !outcome.states.iter().any(|state| matches!(
            state,
            StepState::Pending | StepState::Ready | StepState::Running
        )),
        "every step has to be left in a state it can stay in; these came back as {:?}, and a \
         step still reading as Running after the run returned is a row the UI will show \
         spinning forever",
        outcome.states
    );
    assert!(
        recorder.saw_cancel(0),
        "the token has to reach INSIDE step 0. Dropping the task from the outside also returns \
         fast and also looks cancelled, and in T-03 that shape leaves a live process group \
         burning quota [T7 §3.1]"
    );

    // ── Bieg drugi: ten sam graf, świeży token ─────────────────────────────────────────────
    let recorder = Arc::new(Recorder::new());
    let driver = FakeDriver::new(Arc::clone(&recorder), vec![Behaviour::Succeed; 3]);
    let again: Outcome = execute(&dag, 1, CancellationToken::new(), move |step, cancel| {
        driver.clone().run(step, cancel)
    })
    .await;

    assert!(
        !again.cancelled,
        "a fresh token is a fresh run; this one came back cancelled without anybody stopping it"
    );
    assert_eq!(
        again.states,
        vec![StepState::Succeeded; 3],
        "the run after a cancelled one has to do the work. A global AtomicBool leaks the \
         previous Stop into this run, which then ends in milliseconds with nothing but \
         Cancelled — and that reads as a fast run, not as a failure (invariant 7)"
    );
    Ok(())
}
