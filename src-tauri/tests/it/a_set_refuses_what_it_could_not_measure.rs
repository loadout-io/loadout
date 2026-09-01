//! Zapis zestawu odmawia trzech rzeczy, których żaden bieg już nie naprawi — i **każda odmowa
//! jest zdaniem**, nie boolem.
//!
//! # Dlaczego to musi paść przy ZAPISIE
//!
//! Niezmiennik 12 mówi, kiedy: najpóźniej przy zapisie, nigdy w trakcie biegu. Każda z tych
//! trzech rzeczy gubi wynik **po cichu** — dwa przypadki o jednym identyfikatorze dają dwa kroki
//! o jednym kluczu i wynik jednego znika; przypadek, którego nie ma czym osądzić, przechodzi
//! zawsze i podnosi wynik, nie mierząc niczego; dwa przypadki o jednej nazwie mają dwa kroki
//! o jednej nazwie, a przekazanie zna swój krok wyłącznie po niej.
//!
//! # Słaba wersja tego kryterium
//!
//! `assert!(result.is_err())`. Przechodzi ją implementacja odmawiająca **zawsze** — czyli taka,
//! po której nie da się zapisać niczego. Dlatego każdy przypadek ma tu bliźniaka, który ma
//! przejść, i dlatego sądzone jest ZDANIE, a nie sam fakt odmowy: odmowa, która nie nazywa
//! rzeczy do poprawienia, wysyła człowieka szukać po dziewięciu polach.

// Kryteria wolno pisać `expect()` i `panic!`, a kod produkcyjny nie (`Cargo.toml`,
// `AGENTS.md` §4). Różnica jest treścią: panika w agentowym runtime zabiera cały bieg, a tutaj
// jest jedynym sposobem powiedzenia „ta fikstura jest zepsuta" bez udawania, że test przeszedł.
#![allow(clippy::expect_used, clippy::panic)]

use loadout_lib::lab::file::why_it_would_not_hold;
use loadout_lib::lab::{Case, CaseStatus, EvalSet, Expect, Subject, Variant};
use serde_json::Map;

fn a_case(id: &str, name: &str) -> Case {
    Case {
        id: id.to_owned(),
        name: name.to_owned(),
        task: "say which file resolves the tenant".to_owned(),
        expect: vec![Expect {
            field: "file".to_owned(),
            contains: String::new(),
            describe: String::new(),
        }],
        command: String::new(),
        proof: String::new(),
        status: CaseStatus::InUse,
        because: "src/guard.rs:14".to_owned(),
        extra: Map::new(),
    }
}

fn a_set(cases: Vec<Case>) -> EvalSet {
    EvalSet {
        format: loadout_lib::lab::CURRENT,
        id: "review-rubric".to_owned(),
        name: "Review rubric".to_owned(),
        subject: Subject::Agent {
            id: "0198a1f2-3b4c-7d5e-8f60-112233445566".to_owned(),
        },
        cases,
        variants: vec![Variant {
            id: "without".to_owned(),
            name: "Without".to_owned(),
            agent: "0198a1f2-3b4c-7d5e-8f60-112233445566".to_owned(),
            overrides: Map::new(),
            extra: Map::new(),
        }],
        extra: Map::new(),
    }
}

#[test]
fn a_set_a_person_could_actually_run_is_saved() {
    let set = a_set(vec![a_case("one", "Reads the guard")]);
    assert_eq!(
        why_it_would_not_hold(&set),
        None,
        "a set with one accepted case, one column and something to judge it by has to save; an \
         implementation that turns everything away passes every assertion below"
    );
}

#[test]
fn two_rows_with_one_name_are_turned_away_by_name() {
    let set = a_set(vec![
        a_case("one", "Reads the guard"),
        a_case("two", "Reads the guard"),
    ]);
    let said = why_it_would_not_hold(&set).unwrap_or_default();
    assert!(
        said.contains("Reads the guard"),
        "the sentence has to name the row a person has to rename, or the only way on is to \
         read the file: {said:?}"
    );
}

#[test]
fn two_rows_with_one_id_are_turned_away_too() {
    let set = a_set(vec![
        a_case("one", "Reads the guard"),
        a_case("one", "Names the file"),
    ]);
    let said = why_it_would_not_hold(&set).unwrap_or_default();
    assert!(
        said.contains("one"),
        "two rows at one address lose one set of results without a word: {said:?}"
    );
}

#[test]
fn an_accepted_case_with_nothing_to_judge_it_is_turned_away() {
    let mut only = a_case("one", "Reads the guard");
    only.expect = Vec::new();
    let said = why_it_would_not_hold(&a_set(vec![only.clone()])).unwrap_or_default();
    assert!(
        said.contains("Reads the guard"),
        "a case with no command and no field passes every time and measures nothing, and the \
         sentence has to say which one it is: {said:?}"
    );

    // A KANDYDATKA W TYM SAMYM STANIE PRZECHODZI, bo jest propozycją do uzupełnienia, nie
    // pomiarem. Odmowa jej zapisu skasowałaby całą turę, która ją wypracowała.
    let mut waiting = only;
    waiting.status = CaseStatus::Suggested;
    assert_eq!(
        why_it_would_not_hold(&a_set(vec![waiting])),
        None,
        "a suggestion is not yet measuring anything, so it has no business being judged like a \
         row that is"
    );
}

#[test]
fn a_command_with_nothing_that_proves_it_ran_is_turned_away() {
    let mut only = a_case("one", "Reads the guard");
    only.command = "npm test".to_owned();
    only.proof = String::new();
    let said = why_it_would_not_hold(&a_set(vec![only.clone()])).unwrap_or_default();
    assert!(
        said.contains("Reads the guard"),
        "a suite that ran no tests exits with zero, so a command with nothing that proves it \
         ran is a green cell over nothing: {said:?}"
    );

    only.proof = "0 failed".to_owned();
    assert_eq!(why_it_would_not_hold(&a_set(vec![only])), None);
}

#[test]
fn an_id_that_would_collide_in_a_run_is_turned_away() {
    // `a__b` w wierszu z kolumną `c` i `a` z kolumną `b__c` dają ten sam klucz kroku, więc jedna
    // z dwóch komórek czytałaby wynik drugiej.
    let set = a_set(vec![a_case("reads__the-guard", "Reads the guard")]);
    let said = why_it_would_not_hold(&set).unwrap_or_default();
    assert!(
        said.contains("reads__the-guard"),
        "the sentence has to name the id a person has to change: {said:?}"
    );
}
