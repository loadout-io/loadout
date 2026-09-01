//! Komórka przechodzi wtedy i tylko wtedy, gdy **wszystkie trzy** rzeczy się zgodziły: praca
//! się udała, pola mówią to, czego od nich chciano, a komenda przeszła.
//!
//! # Słaba wersja tego kryterium
//!
//! Jeden przebieg, w którym wszystko poszło, i asercja „przeszło". Przechodzi ją implementacja
//! pytająca wyłącznie o stan kroku pracy — czyli taka, dla której czerwona komenda i pole
//! mówiące co innego są niewidzialne. Dlatego każdy z trzech warunków ma tu własny przebieg,
//! w którym **tylko on** jest złamany.
//!
//! # I druga, subtelniejsza
//!
//! Traktowanie „nie zmierzono" jak porażki. Bieg zatrzymany w połowie macierzy zostawia
//! komórki, których nikt nie osądził; policzone jako czerwone obniżyłyby wynik zestawu o pracę,
//! której nikt nie zamówił — a wtedy „7 z 9" po zatrzymaniu znaczy co innego niż „7 z 9" po
//! pełnym przebiegu.

// Kryteria wolno pisać `expect()` i `panic!`, a kod produkcyjny nie (`Cargo.toml`,
// `AGENTS.md` §4). Różnica jest treścią: panika w agentowym runtime zabiera cały bieg, a tutaj
// jest jedynym sposobem powiedzenia „ta fikstura jest zepsuta" bez udawania, że test przeszedł.
#![allow(clippy::expect_used, clippy::panic)]

use loadout_lib::lab::plan::{Half, key_for};
use loadout_lib::lab::results::{Finished, Outcome, score};
use loadout_lib::lab::{Case, CaseStatus, EvalSet, Expect, Subject, Variant};
use serde_json::Map;

const CASE: &str = "one";
const COLUMN: &str = "without";

fn a_set(expect: Vec<Expect>, command: &str) -> EvalSet {
    EvalSet {
        format: loadout_lib::lab::CURRENT,
        id: "review-rubric".to_owned(),
        name: "Review rubric".to_owned(),
        subject: Subject::Agent {
            id: "0198a1f2-3b4c-7d5e-8f60-112233445566".to_owned(),
        },
        cases: vec![Case {
            id: CASE.to_owned(),
            name: "Reads the guard".to_owned(),
            task: "say which file resolves the tenant".to_owned(),
            expect,
            command: command.to_owned(),
            proof: if command.is_empty() {
                String::new()
            } else {
                "0 failed".to_owned()
            },
            status: CaseStatus::InUse,
            because: "src/guard.rs:14".to_owned(),
            extra: Map::new(),
        }],
        variants: vec![Variant {
            id: COLUMN.to_owned(),
            name: "Without".to_owned(),
            agent: "0198a1f2-3b4c-7d5e-8f60-112233445566".to_owned(),
            overrides: Map::new(),
            extra: Map::new(),
        }],
        extra: Map::new(),
    }
}

fn step(half: Half, state: &str, said: &str) -> Finished {
    Finished {
        tile: key_for(CASE, COLUMN, half),
        state: state.to_owned(),
        cost_usd: Some(0.5),
        error: String::new(),
        said: said.to_owned(),
    }
}

fn one_cell(set: &EvalSet, steps: &[Finished]) -> Outcome {
    let scored = score(set, steps);
    scored
        .cells
        .first()
        .map(|cell| cell.outcome)
        .expect("the one accepted case across the one column is one cell")
}

#[test]
fn all_three_agree_and_the_cell_passes() {
    let set = a_set(
        vec![Expect {
            field: "file".to_owned(),
            contains: "guard.rs".to_owned(),
            describe: String::new(),
        }],
        "npm test",
    );
    let outcome = one_cell(
        &set,
        &[
            step(Half::Work, "succeeded", "file: src/guard.rs\n"),
            step(Half::Checks, "succeeded", ""),
        ],
    );
    assert_eq!(outcome, Outcome::Passed);
}

#[test]
fn the_work_alone_is_not_enough_when_the_command_did_not_pass() {
    let set = a_set(Vec::new(), "npm test");
    let outcome = one_cell(
        &set,
        &[
            step(Half::Work, "succeeded", "all done"),
            step(Half::Checks, "failed", ""),
        ],
    );
    assert_eq!(
        outcome,
        Outcome::DidNotPass,
        "the agent said it was done and the project said otherwise — which of the two is right \
         is the only question this product exists to answer"
    );
}

#[test]
fn a_field_that_says_something_else_does_not_pass() {
    let set = a_set(
        vec![Expect {
            field: "file".to_owned(),
            contains: "guard.rs".to_owned(),
            describe: String::new(),
        }],
        "",
    );
    let outcome = one_cell(
        &set,
        &[step(Half::Work, "succeeded", "file: src/router.rs\n")],
    );
    assert_eq!(
        outcome,
        Outcome::DidNotPass,
        "the answer carried the field and filled it with something else; a case that only asks \
         whether the line exists measures formatting, not work"
    );

    let scored = score(
        &set,
        &[step(Half::Work, "succeeded", "file: src/router.rs\n")],
    );
    let said = scored
        .cells
        .first()
        .map(|cell| cell.said.clone())
        .unwrap_or_default();
    assert!(
        said.contains("guard.rs") && said.contains("router.rs"),
        "a person reads this sentence in the table and has to see both what was asked and what \
         came back, or the only way on is to open the transcript: {said:?}"
    );
}

#[test]
fn a_stopped_run_leaves_cells_unmeasured_rather_than_red() {
    let set = a_set(Vec::new(), "");
    let scored = score(&set, &[step(Half::Work, "cancelled", "")]);
    assert_eq!(
        scored.cells.first().map(|cell| cell.outcome),
        Some(Outcome::NotJudged)
    );
    assert_eq!(
        scored.judged, 0,
        "a cell nobody measured may not count against the set: the score after a stop would \
         then mean something different from the score after a whole run"
    );
    assert_eq!(scored.passed, 0);
}

#[test]
fn a_run_older_than_the_row_says_so_instead_of_failing_it() {
    let set = a_set(Vec::new(), "");
    // Ani jednego kroku tej komórki: ten przebieg pochodzi sprzed dopisania wiersza.
    let scored = score(&set, &[]);
    let cell = scored
        .cells
        .first()
        .expect("the row is in the set, so it is in the table");
    assert_eq!(cell.outcome, Outcome::NotJudged);
    assert!(
        !cell.said.trim().is_empty(),
        "an empty cell with no sentence sends a person looking for a run that never had this row"
    );
}
