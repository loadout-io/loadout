//! AC-4 dla T-02: `Dag::new` odmawia cyklu i krawędzi do nieistniejącego węzła — **przy
//! konstrukcji**, nie przy biegu.
//!
//! Słaba wersja tego kryterium to osobne `is_acyclic()`, które test woła wprost, podczas gdy
//! `Dag::new` przyjmuje wszystko. Wtedy planista dostaje graf bez ani jednego korzenia, jego
//! pętla kończy się przy `inflight == 0` w pierwszym obrocie i **melduje bieg, w którym nic nie
//! biegło**. Dlatego każda asercja niżej jest asercją na typie zwrotnym samego `Dag::new`.
//!
//! Drugi przypadek — `[(0,1),(1,2),(2,1)]` — jest tu po to, żeby odrzucić tańsze sprawdzenie
//! „czy istnieje węzeł o stopniu wejściowym 0". Węzeł 0 taki jest, więc to sprawdzenie
//! przechodzi; przewraca się dopiero liczenie przetworzonych węzłów (Kahn).
//!
//! Komunikaty dla użytkownika buduje T-12 przy zapisie workflow. Odmowa tutaj jest **ostatnią
//! linią obrony, nie pierwszą** — asercja na treści dotyczy tylko tego, żeby błąd nazywał
//! węzeł, bo „graf ma cykl" bez numeru jest zdaniem, z którym nie da się nic zrobić.

use std::error::Error;
use std::mem::discriminant;

use loadout_lib::engine::dag::{Dag, DagError};

/// Trzy węzły domknięte w koło.
const CLOSED_CIRCLE: [(usize, usize); 3] = [(0, 1), (1, 2), (2, 0)];
/// Koło niżej, ale z korzeniem nad nim: węzeł 0 ma stopień wejściowy 0, a 1 i 2 trzymają się
/// nawzajem.
const CIRCLE_BELOW_A_ROOT: [(usize, usize); 3] = [(0, 1), (1, 2), (2, 1)];
/// Diament: dwie ścieżki od 0 do 3.
const DIAMOND: [(usize, usize); 4] = [(0, 1), (0, 2), (1, 3), (2, 3)];

#[test]
fn a_closed_circle_is_refused_and_the_message_names_a_step() -> Result<(), Box<dyn Error>> {
    let error = Dag::new(3, &CLOSED_CIRCLE)
        .err()
        .ok_or("Dag::new accepted three steps depending on one another in a circle")?;

    let DagError::Cycle { nodes } = error.clone() else {
        return Err(format!("a circle has to come back as a cycle, not as: {error}").into());
    };
    assert!(
        !nodes.is_empty(),
        "a cycle error that names no step at all leaves the user with nothing to fix"
    );
    assert!(
        nodes.iter().all(|node| *node < 3),
        "every step named in the error has to be a step of this graph; got {nodes:?}"
    );

    let message = error.to_string();
    assert!(
        nodes.iter().any(|node| message.contains(&node.to_string())),
        "the message has to name at least one of the steps on the circle {nodes:?}; it reads: \
         {message}"
    );
    Ok(())
}

#[test]
fn a_circle_below_a_root_is_refused_too() -> Result<(), Box<dyn Error>> {
    // Węzeł 0 ma stopień wejściowy 0, więc sprawdzenie „czy istnieje korzeń" przechodzi ten
    // graf. Przewraca się dopiero liczenie przetworzonych węzłów.
    let error = Dag::new(3, &CIRCLE_BELOW_A_ROOT)
        .err()
        .ok_or("Dag::new accepted a graph whose steps 1 and 2 wait for one another")?;

    let DagError::Cycle { nodes } = error.clone() else {
        return Err(format!("a circle has to come back as a cycle, not as: {error}").into());
    };
    assert!(
        nodes.iter().all(|node| *node == 1 || *node == 2),
        "steps 1 and 2 are the ones waiting for each other; step 0 runs and must not be named. \
         Got {nodes:?}"
    );
    Ok(())
}

#[test]
fn a_step_that_waits_for_itself_is_refused() -> Result<(), Box<dyn Error>> {
    let error = Dag::new(2, &[(1, 1)])
        .err()
        .ok_or("Dag::new accepted a step that waits for itself, which can never start")?;
    assert!(
        matches!(error, DagError::Cycle { .. }),
        "a step waiting for itself is a circle of length one, not a different kind of mistake; \
         got: {error}"
    );
    Ok(())
}

#[test]
fn an_edge_into_a_missing_step_is_a_different_refusal() -> Result<(), Box<dyn Error>> {
    let circle = Dag::new(3, &CLOSED_CIRCLE)
        .err()
        .ok_or("Dag::new accepted three steps depending on one another in a circle")?;
    let missing = Dag::new(3, &[(0, 9)])
        .err()
        .ok_or("Dag::new accepted a link into step 9 of a three-step graph")?;

    assert!(
        matches!(missing, DagError::UnknownNode { .. }),
        "a link into a step that does not exist is a typo in the data, not an ordering problem; \
         got: {missing}"
    );
    assert_ne!(
        discriminant(&circle),
        discriminant(&missing),
        "these are two different mistakes and the user fixes them in two different ways, so \
         they may not come back as the same variant"
    );
    Ok(())
}

#[test]
fn a_diamond_is_accepted_and_carries_its_in_degrees() -> Result<(), Box<dyn Error>> {
    let dag = Dag::new(4, &DIAMOND)?;
    assert_eq!(
        dag.in_degree(),
        vec![0, 1, 1, 2],
        "the diamond has one root, two steps waiting for it and one step waiting for both"
    );
    Ok(())
}
