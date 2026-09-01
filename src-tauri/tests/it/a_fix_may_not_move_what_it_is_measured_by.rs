//! Poprawka przepisuje **instrukcję agenta** i nic poza nią — a odpowiedź bez nowego tekstu
//! nie jest poprawką.
//!
//! # Dlaczego to jest kryterium, a nie akapit w prompcie
//!
//! Bo mechanizm, któremu wolno przepisać własną miarę, zawsze dochodzi do stu procent.
//! Najkrótszą drogą do czystej tabeli jest zmiana tego, co ta tabela mierzy: skasować trudny
//! przypadek, poluzować komendę, wyjąć oczekiwane pole. Zdanie zakazujące tego musi więc
//! stać w pytaniu — i musi tam zostać, kiedy ktoś to pytanie za pół roku przepisze.
//!
//! # Słaba wersja
//!
//! `assert!(read_fix(said).is_some())` na jednej dobrej odpowiedzi. Przechodzi ją parser, który
//! oddaje poprawkę z **pustym** tekstem — a karta z pustym tekstem i przyciskiem Apply kasuje
//! instrukcję agenta jednym kliknięciem i wygląda dokładnie jak karta z poprawką.

// Kryteria wolno pisać `expect()` i `panic!`, a kod produkcyjny nie (`Cargo.toml`,
// `AGENTS.md` §4). Różnica jest treścią: panika w agentowym runtime zabiera cały bieg, a tutaj
// jest jedynym sposobem powiedzenia „ta fikstura jest zepsuta" bez udawania, że test przeszedł.
#![allow(clippy::expect_used, clippy::panic)]

use loadout_lib::lab::fix::{ask_for_a_fix, read_fix};

const NOW: &str = "Answer in two sentences. Never guess a file name.";

fn a_question() -> String {
    ask_for_a_fix(
        "Reviewer",
        NOW,
        &[
            "Reads the guard (Without): \"file\" came back as \"src/router.rs\".".to_owned(),
            "Names the file (With): The checks did not pass.".to_owned(),
        ],
    )
}

#[test]
fn the_question_carries_the_text_it_asks_to_rewrite() {
    let asked = a_question();
    assert!(
        asked.contains(NOW),
        "a model that cannot see the current text writes a new one from nothing and throws away \
         everything a person ever put there: {asked}"
    );
}

#[test]
fn the_question_carries_only_what_came_back_wrong() {
    let asked = a_question();
    assert!(asked.contains("came back as \"src/router.rs\""));
    assert!(asked.contains("The checks did not pass."));
    assert!(
        !asked.contains("passed"),
        "a hundred cells with three red ones is a question in which the three sentences that \
         matter are lost among ninety-seven that do not, and length costs on every turn: {asked}"
    );
}

#[test]
fn the_question_forbids_moving_the_thing_being_measured() {
    let asked = a_question();
    assert!(
        asked.contains("Do not propose changes to the work itself"),
        "without this sentence the shortest way to a clean table is to delete the hard case: \
         {asked}"
    );
    assert!(
        asked.contains("measures its own answer"),
        "the reason has to be in the question too: a rule with no reason is one a model talks \
         itself out of: {asked}"
    );
}

#[test]
fn an_answer_with_both_halves_becomes_a_fix() {
    let fixed = read_fix(
        "## Why\nIt guessed a path instead of opening the file.\n\n## Instructions\nOpen the \
         file before you name it.\n\nAnswer in two sentences.\n",
    )
    .expect("both headings were there");
    assert_eq!(
        fixed.because,
        "It guessed a path instead of opening the file."
    );
    assert_eq!(
        fixed.instructions, "Open the file before you name it.\n\nAnswer in two sentences.",
        "the blank line inside the text stays: an agent's instructions are paragraphs, and \
         gluing them into one is a change to the text nobody asked for"
    );
}

#[test]
fn an_answer_with_no_new_text_is_not_a_fix() {
    assert_eq!(
        read_fix("## Why\nIt guessed.\n\n## Instructions\n\n"),
        None,
        "an empty text behind an Apply button wipes the agent's instructions in one press and \
         looks exactly like a real fix"
    );
    assert_eq!(
        read_fix("## Instructions\nOpen the file first.\n"),
        None,
        "a wall of text with no sentence about what it fixes is accepted or refused without \
         being read, and this press changes how that agent behaves in every future run"
    );
    assert_eq!(read_fix("I could not do it, sorry."), None);
}

#[test]
fn a_heading_quoted_in_prose_does_not_open_a_section() {
    // Dopasowanie po CAŁYM wierszu, nie po podciągu: zdanie o nagłówku nie jest nagłówkiem.
    assert_eq!(
        read_fix("I put it under ## Instructions below.\n## Why\nBecause.\n"),
        None,
        "a parser matching a substring starts a section nobody wrote, and then Apply saves a \
         sentence of prose as the whole of an agent's instructions"
    );
}
