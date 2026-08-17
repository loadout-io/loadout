//! AC-3 dla T-02: każdy węzeł biegnie **dokładnie raz** i żaden nie startuje przed końcem
//! wszystkich rodziców.
//!
//! Sama asercja „wszystkie stany to `Succeeded`" jest za słaba dwa razy naraz: przechodzi ją
//! implementacja, która nie woła kroku ani razu i od razu maluje wektor stanów, oraz taka,
//! która uruchamia dzieci przed rodzicami. Rozróżniają je licznik uruchomień równy 1
//! i porównanie numerów sekwencji na każdej krawędzi.
//!
//! **Model referencyjny liczony jest tutaj, w pliku testu.** Ten test nie ma prawa zawołać
//! `dag.children()` ani `dag.in_degree()`: gdyby pytał graf o to, czego sam ma dowieść, błąd
//! w odwracaniu krawędzi byłby niewidoczny dla obu stron [T7 §8.2]. Zbiór krawędzi jest tu
//! tym samym wektorem, który poszedł do `Dag::new` — i niczym więcej.
//!
//! Generator jest **przepisany z pliku AC-2 dosłownie**, a nie zaimportowany: wspólny moduł
//! musiałby stanąć pod `tests/common/`, a ta ścieżka nie należy do T-02 (mapa własności,
//! `AGENTS.md` §7). Dwa pliki testowe to dwie osobne skrzynie.

use std::sync::Arc;

use loadout_lib::engine::dag::Dag;
use loadout_lib::engine::fake::{Behaviour, FakeDriver, Recorder};
use loadout_lib::engine::scheduler::execute;
use loadout_lib::engine::step::StepState;
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;
use tokio_util::sync::CancellationToken;

/// Co jeden bieg zostawił po sobie, w postaci, którą da się porównać z modelem.
#[derive(Debug)]
struct Observed {
    /// Ile razy każdy węzeł wszedł do sterownika.
    run_count: Vec<usize>,
    /// Numer sekwencji wejścia każdego węzła.
    start_seq: Vec<Option<u64>>,
    /// Numer sekwencji wyjścia każdego węzła.
    finish_seq: Vec<Option<u64>>,
    /// Stany końcowe.
    states: Vec<StepState>,
}

/// Losowy DAG i losowy limit — krawędzie wyłącznie z niższego indeksu do wyższego, więc graf
/// jest acykliczny z konstrukcji [T7 §8.2].
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

fn run_and_observe(
    n: usize,
    edges: &[(usize, usize)],
    limit: usize,
) -> Result<Observed, TestCaseError> {
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
    let outcome = runtime.block_on(async {
        execute(
            &dag,
            limit,
            CancellationToken::new(),
            move |step, cancel| driver.clone().run(step, cancel),
        )
        .await
    });

    Ok(Observed {
        run_count: (0..n).map(|step| recorder.run_count(step)).collect(),
        start_seq: (0..n).map(|step| recorder.enter_seq(step)).collect(),
        finish_seq: (0..n).map(|step| recorder.exit_seq(step)).collect(),
        states: outcome.states,
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn every_node_runs_once_and_never_before_its_parents(
        (n, edges, limit) in dag_and_limit()
    ) {
        let observed = run_and_observe(n, &edges, limit)?;

        for step in 0..n {
            let count = observed.run_count[step];
            // Argumenty nazwane WPROST, choć w komunikacie wyglądają na przechwycone w miejscu.
            // `prop_assert_eq!` skleja swój format przez `concat!`, a `format_args!` nie
            // przechwytuje zmiennych, kiedy łańcuch formatu powstał z rozwinięcia makra —
            // bez tej listy plik się NIE KOMPILUJE (`error: there is no argument named 'step'`),
            // a test, który się nie kompiluje, niczego nie uruchomił (AGENTS.md §2a p. 5).
            // Sąsiednie `prop_assert!` przechwytują w miejscu i tego nie potrzebują: tamto makro
            // podaje literał formatu wprost, bez `concat!`.
            prop_assert_eq!(
                count, 1,
                "step {step} has to run exactly once; it ran {count} times on a graph of {n} \
                 nodes with edges {edges:?}",
                step = step,
                count = count,
                n = n,
                edges = edges
            );
        }

        // Model referencyjny: te same krawędzie, które poszły do konstruktora. Bez pytania
        // grafu o cokolwiek — o to chodzi.
        for &(parent, child) in &edges {
            let finished = observed.finish_seq[parent].ok_or_else(|| TestCaseError::fail(
                format!("step {parent} never left the driver, so nothing can depend on it")
            ))?;
            let started = observed.start_seq[child].ok_or_else(|| TestCaseError::fail(
                format!("step {child} never entered the driver")
            ))?;
            prop_assert!(
                finished < started,
                "step {child} may not begin before step {parent} has finished; the parent left \
                 at {finished} and the child entered at {started} on a graph of {n} nodes with \
                 edges {edges:?}"
            );
        }

        prop_assert!(
            observed.states.iter().all(|state| *state == StepState::Succeeded),
            "with no failing steps every node has to end as Succeeded; they ended as {:?}",
            observed.states
        );
    }
}
