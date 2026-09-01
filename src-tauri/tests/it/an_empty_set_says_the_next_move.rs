//! Zestaw bez ani jednego przypadku ma **własne zdanie**, nie zdanie o propozycjach, których
//! nie ma.
//!
//! # Zmierzone na żywym ekranie, 2026-08-31
//!
//! Właściciel zalozyl zestaw dla agenta, zobaczyl „Every case here is still a suggestion.
//! Accept at least one and it will be part of the next run." nad PUSTA lista i napisal „nie
//! kumam jak to dziala". Zdanie bylo prawdziwe logicznie — zero przypadkow w uzyciu — i
//! nieprawdziwe dla czlowieka: kazalo mu szukac propozycji, ktorych nikt nie napisal.
//!
//! # Dlaczego to jest kryterium, a nie poprawka brzmienia
//!
//! Bo to sa DWIE ROZNE RZECZY DO ZROBIENIA. Przy zerze przypadkow nastepny ruch to „Write
//! cases"; przy samych propozycjach — „Accept". Jedno zdanie na oba stany zawsze wysyla
//! polowe ludzi w zla strone, a ktora polowe, zalezy od tego, jak je napisano.
//!
//! # Slaba wersja
//!
//! `assert!(why_it_cannot_run(&set).is_some())` — „cos powiedzial". Przechodzi ja
//! implementacja, ktora dla obu stanow mowi to samo, czyli dokladnie ta, ktora byla.

// Kryteria wolno pisać `expect()` i `panic!`, a kod produkcyjny nie (`Cargo.toml`,
// `AGENTS.md` §4).
#![allow(clippy::expect_used, clippy::panic)]

use loadout_lib::lab::{Case, CaseStatus, EvalSet, Expect, Subject, Variant};
use serde_json::Map;

fn a_set(cases: Vec<Case>, variants: Vec<Variant>) -> EvalSet {
    EvalSet {
        format: loadout_lib::lab::CURRENT,
        id: "adversarial-verifier".to_owned(),
        name: "adversarial-verifier".to_owned(),
        subject: Subject::Agent {
            id: "01a04349-d19d-73b3-a71f-8287bcddacdc".to_owned(),
        },
        cases,
        variants,
        extra: Map::new(),
    }
}

fn a_case(status: CaseStatus) -> Case {
    Case {
        id: "one".to_owned(),
        name: "Reads the guard".to_owned(),
        task: "say which file resolves the tenant".to_owned(),
        expect: vec![Expect {
            field: "file".to_owned(),
            contains: String::new(),
            describe: String::new(),
        }],
        command: String::new(),
        proof: String::new(),
        status,
        because: "src/guard.rs:14".to_owned(),
        extra: Map::new(),
    }
}

fn a_column() -> Variant {
    Variant {
        id: "as-it-is".to_owned(),
        name: "As it is".to_owned(),
        agent: "01a04349-d19d-73b3-a71f-8287bcddacdc".to_owned(),
        overrides: Map::new(),
        extra: Map::new(),
    }
}

#[test]
fn a_set_with_no_cases_at_all_points_at_writing_some() {
    // DOKŁADNIE TEN PLIK, KTÓRY POWSTAŁ NA MASZYNIE WŁAŚCICIELA: jedna kolumna, zero
    // przypadków.
    let said = a_set(Vec::new(), vec![a_column()])
        .why_it_cannot_run()
        .unwrap_or_default();
    assert!(
        said.contains("Write cases"),
        "the sentence has to name the button a person presses next, or a fresh set is a screen \
         with nothing to do on it: {said:?}"
    );
    assert!(
        !said.contains("suggestion"),
        "there are no suggestions here, so a sentence about them sends a person looking for \
         something nobody wrote: {said:?}"
    );
}

#[test]
fn a_set_whose_cases_all_wait_says_something_else_entirely() {
    let said = a_set(vec![a_case(CaseStatus::Suggested)], vec![a_column()])
        .why_it_cannot_run()
        .unwrap_or_default();
    assert!(
        said.contains("Accept"),
        "with suggestions on screen the next move is accepting one, not writing more: {said:?}"
    );
    assert!(
        !said.contains("Write cases"),
        "and it may not send a person back to the button they already pressed: {said:?}"
    );
}

#[test]
fn the_two_sentences_are_not_the_same_sentence() {
    // KONTROLA PRZECIW IMPLEMENTACJI, KTÓRA BYŁA: jedno zdanie na oba stany przechodzi każde
    // „coś powiedział" i zawsze wysyła połowę ludzi w złą stronę.
    let empty = a_set(Vec::new(), vec![a_column()]).why_it_cannot_run();
    let waiting = a_set(vec![a_case(CaseStatus::Suggested)], vec![a_column()]).why_it_cannot_run();
    assert_ne!(empty, waiting);
}

#[test]
fn a_set_a_person_can_run_says_nothing_at_all() {
    assert_eq!(
        a_set(vec![a_case(CaseStatus::InUse)], vec![a_column()]).why_it_cannot_run(),
        None,
        "an implementation that always has something to complain about passes every assertion \
         above and never lets anybody press Run"
    );
}

#[test]
fn no_columns_is_still_its_own_answer() {
    let said = a_set(vec![a_case(CaseStatus::InUse)], Vec::new())
        .why_it_cannot_run()
        .unwrap_or_default();
    assert!(said.contains("columns"), "{said:?}");
}
