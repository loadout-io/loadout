//! AC-2 dla T-02: szczyt równoczesności nigdy nie przekracza limitu, a przy nadmiarze
//! gotowych kroków **dochodzi do limitu**.
//!
//! Dwie połowy, bo każda z osobna jest do przejścia w sposób, który nic nie znaczy.
//!
//! Samo `peak <= limit` przechodzi implementacja z jednym workerem: `peak == 1` przy
//! `limit == 4` spełnia nierówność i jest dokładnie defektem poprzedniego prototypu, którego to zadanie ma
//! nie powtórzyć. Dlatego druga połowa asertuje **równość** przy ośmiu gotowych krokach
//! i limicie 3 — nie „co najmniej 1".
//!
//! Samo `peak == 3` przy jednym kształcie grafu nie mówi nic o innych kształtach, więc pierwsza
//! połowa jest własnościowa: 300 losowych DAG-ów. Krawędzie wyłącznie z niższego indeksu do
//! wyższego, czyli acykliczne z konstrukcji [T7 §8.2] — to usuwa całą klasę bezużytecznych
//! przypadków przy zwężaniu.
//!
//! Runtime w ciele przypadku musi być **wielowątkowy**: na jednowątkowym `peak` bywa 1 zawsze
//! i własność przechodzi, nie mierząc niczego.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use loadout_lib::engine::dag::Dag;
use loadout_lib::engine::fake::{Behaviour, FakeDriver, Recorder};
use loadout_lib::engine::scheduler::execute;
use loadout_lib::engine::step::StepState;
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;
use tokio_util::sync::CancellationToken;

/// Ile kroków jest gotowych naraz w części deterministycznej.
const READY: usize = 8;
/// Ilu z nich wolno biec naraz.
const LIMIT: usize = 3;
/// Jak długo trwa każdy z nich. Musi wystarczyć, żeby trzeci zdążył wejść, zanim pierwszy
/// wyjdzie — 120 ms to około dwóch rzędów wielkości ponad koszt wysłania zadania.
const BUSY: Duration = Duration::from_millis(120);

/// Losowy DAG i losowy limit.
///
/// Krawędzie idą wyłącznie „w prawo" (`parent < child`), więc każdy wygenerowany graf jest
/// acykliczny **z konstrukcji**. Maska po wszystkich dopuszczalnych parach daje pełny zakres
/// gęstości: od grafu bez krawędzi (wszystko gotowe od razu, najostrzejszy test na limit) po
/// łańcuch, w którym nic nie może biec równolegle.
fn dag_and_limit() -> impl Strategy<Value = (usize, Vec<(usize, usize)>, usize)> {
    (1usize..=10, 1usize..=4).prop_flat_map(|(n, limit)| {
        let pairs: Vec<(usize, usize)> = (0..n)
            .flat_map(|parent| ((parent + 1)..n).map(move |child| (parent, child)))
            .collect();
        let width = pairs.len();
        proptest::collection::vec(any::<bool>(), width).prop_map(move |mask| {
            let edges = pairs
                .iter()
                .copied()
                .zip(mask)
                .filter_map(|(edge, keep)| keep.then_some(edge))
                .collect();
            (n, edges, limit)
        })
    })
}

/// Uruchamia graf i zwraca zmierzony szczyt równoczesności.
fn peak_of(n: usize, edges: &[(usize, usize)], limit: usize) -> Result<usize, TestCaseError> {
    let dag = Dag::new(n, edges).map_err(|error| {
        TestCaseError::fail(format!(
            "the generator only makes edges from a lower index to a higher one, so Dag::new \
             had nothing to refuse here: {error}"
        ))
    })?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .map_err(|error| {
            TestCaseError::fail(format!("could not build a multi-thread runtime: {error}"))
        })?;

    let recorder = Arc::new(Recorder::new());
    let driver = FakeDriver::new(Arc::clone(&recorder), vec![Behaviour::Succeed; n]);
    runtime.block_on(async {
        execute(&dag, limit, CancellationToken::new(), move |step, cancel| {
            driver.clone().run(step, cancel)
        })
        .await
    });

    Ok(recorder.peak())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Górne ograniczenie, na trzystu losowych grafach.
    #[test]
    fn no_run_ever_has_more_steps_inside_than_the_limit((n, edges, limit) in dag_and_limit()) {
        let peak = peak_of(n, &edges, limit)?;
        prop_assert!(
            peak <= limit,
            "a limit of {limit} may never be exceeded; {peak} steps were inside at once on a \
             graph of {n} nodes with edges {edges:?}"
        );
    }
}

/// Dolne ograniczenie: przy nadmiarze gotowych kroków limit ma być **osiągnięty**.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn eight_ready_steps_reach_the_limit_exactly() -> Result<(), Box<dyn Error>> {
    let dag = Dag::new(READY, &[])?;
    let recorder = Arc::new(Recorder::new());
    let driver = FakeDriver::new(Arc::clone(&recorder), vec![Behaviour::Busy(BUSY); READY]);

    let outcome = execute(&dag, LIMIT, CancellationToken::new(), move |step, cancel| {
        driver.clone().run(step, cancel)
    })
    .await;

    assert_eq!(
        recorder.peak(),
        LIMIT,
        "with {READY} steps ready at once and a limit of {LIMIT}, exactly {LIMIT} of them have \
         to be inside at the same moment. A peak of 1 satisfies `peak <= limit` and is the \
         one-worker scheduler this criterion exists to reject"
    );
    assert!(
        outcome.states.iter().all(|state| *state == StepState::Succeeded),
        "all {READY} steps have to finish; they ended as {:?}",
        outcome.states
    );
    Ok(())
}
