//! Kandydatka bez pochodzenia jest odrzucana **i policzona**, a każda, która zostaje, czeka na
//! człowieka.
//!
//! # Dlaczego akurat to jest kryterium, a nie szczegół rozbioru
//!
//! Bo bez tego cała sekcja mierzy wyobraźnię modelu. Przypadek, który nie wskazuje pliku, jest
//! zwykle przypadkiem wymyślonym; człowiek nie ma jak go ocenić, więc klika „accept" na
//! wszystkim albo na niczym, a zestaw zaczyna mierzyć rzeczy, których w projekcie nie ma. Ta
//! sama reguła i z tego samego pomiaru stoi przy notatkach (`memory::notes`).
//!
//! # Słaba wersja
//!
//! Sprawdzenie, że odrzucona kandydatka nie jest na liście. Przechodzi ją implementacja, która
//! odrzuca po cichu — a wtedy człowiek widzi cztery pozycje po turze, która wypracowała sześć,
//! i uczy się, że licznik kłamie. Dlatego liczba odrzuconych jest częścią odpowiedzi.

// Kryteria wolno pisać `expect()` i `panic!`, a kod produkcyjny nie (`Cargo.toml`,
// `AGENTS.md` §4). Różnica jest treścią: panika w agentowym runtime zabiera cały bieg, a tutaj
// jest jedynym sposobem powiedzenia „ta fikstura jest zepsuta" bez udawania, że test przeszedł.
#![allow(clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use loadout_lib::lab::CaseStatus;
use loadout_lib::lab::Subject;
use loadout_lib::lab::cases::{ask_for_cases, read};

const THREE_BLOCKS: &str = "\
Here is what I found.

## Case
name: Reads the guard
task: Say which file resolves the tenant.
because: src/guard.rs:14
command: npm test
proof: 0 failed
expect: file = guard.rs

## Case
name: Made this one up
task: Do something clever.

## Case
name: Names the file
task: Name the middleware that runs first.
because: src/router.rs:3
";

#[test]
fn the_one_without_a_reason_is_dropped_and_counted() {
    let proposed = read(THREE_BLOCKS, &BTreeSet::new());
    let names: Vec<&str> = proposed.cases.iter().map(|one| one.name.as_str()).collect();
    assert_eq!(names, vec!["Reads the guard", "Names the file"]);
    assert_eq!(
        proposed.without_a_reason, 1,
        "the count is what lets a person read \"three came back, two were kept\" instead of \
         learning that the number on screen is smaller than the work they paid for"
    );
}

#[test]
fn everything_kept_waits_for_a_person() {
    let proposed = read(THREE_BLOCKS, &BTreeSet::new());
    for one in &proposed.cases {
        assert_eq!(
            one.status,
            CaseStatus::Suggested,
            "material that lets itself into a measurement makes the measurement about itself"
        );
    }
}

#[test]
fn a_command_without_its_proof_arrives_empty_rather_than_half_wired() {
    let proposed = read(THREE_BLOCKS, &BTreeSet::new());
    let second = proposed
        .cases
        .iter()
        .find(|one| one.name == "Names the file")
        .expect("the grounded case with no command");
    assert_eq!(second.command, "");
    assert_eq!(
        second.proof, "",
        "half of the mechanism looks wired and judges on the exit code alone, and a suite that \
         ran no tests exits with zero"
    );
}

#[test]
fn a_candidate_never_takes_an_address_a_person_already_accepted() {
    let taken: BTreeSet<String> = ["reads-the-guard".to_owned()].into_iter().collect();
    let proposed = read(THREE_BLOCKS, &taken);
    let first = proposed.cases.first().expect("the first grounded case");
    assert_ne!(
        first.id, "reads-the-guard",
        "a suggestion taking the address of an accepted row would make that row's results \
         disappear from the table without a word"
    );
}

#[test]
fn what_we_ask_for_never_points_at_the_thing_being_measured() {
    let asked = ask_for_cases(
        &Subject::Skill {
            name: "review-rubric".to_owned(),
        },
        6,
    );
    // TO JEST CAŁE TO KRYTERIUM: przypadek napisany z tekstu, który testuje, przechodzi, bo
    // z niego pochodzi. Pytanie musi więc odsyłać do PROJEKTU i mówić to wprost.
    assert!(
        asked.contains("this project"),
        "the question has to send the agent to the project, or the cases come from the thing \
         they measure: {asked}"
    );
    assert!(
        asked.contains("Do not read"),
        "without saying it out loud, the shortest path for the model is to open the skill it is \
         writing cases for: {asked}"
    );
    assert!(
        !asked.contains("review-rubric"),
        "naming the thing under test inside the question is an invitation to go read it: {asked}"
    );
}
