//! Zestaw zamienia się w **zwykły plik workflow** — taki, który silnik wykona bez ani jednego
//! słowa o Labie.
//!
//! # Słaba wersja tego kryterium
//!
//! `assert!(!plan.steps.is_empty())` — „coś powstało". Przechodzi ją implementacja, która
//! stawia jeden krok na cały zestaw, stawia je wszystkie w katalogu projektu (a wtedy odmowa
//! kolizji zapisu ubija bieg przed pierwszym procesem, niezmiennik 12), albo wpuszcza do
//! pomiaru kandydatki, których człowiek nie zaakceptował. Rozstrzygają liczby i pola, nie sam
//! fakt istnienia.
//!
//! # Czego to kryterium pilnuje ponad kształt
//!
//! Że plan **da się uruchomić**: `workflow::check::check_to_run` nie ma o nim ani jednego
//! zdania rangi `Problem`. Bez tego graf rozłącznych komórek mógłby powstawać poprawnie
//! i odbijać się przy Starcie — a wtedy człowiek dowiaduje się o wadzie zestawu z biegu,
//! który nie ruszył.

// Kryteria wolno pisać `expect()` i `panic!`, a kod produkcyjny nie (`Cargo.toml`,
// `AGENTS.md` §4). Różnica jest treścią: panika w agentowym runtime zabiera cały bieg, a tutaj
// jest jedynym sposobem powiedzenia „ta fikstura jest zepsuta" bez udawania, że test przeszedł.
#![allow(clippy::expect_used, clippy::panic)]

use loadout_lib::lab::plan::{Half, key_for};
use loadout_lib::lab::{Case, CaseStatus, EvalSet, Expect, Subject, Variant, plan};
use loadout_lib::workflow::check::{Level, check_to_run};
use loadout_lib::workflow::{Folder, Handover, Step, WhenItFails};
use serde_json::Map;

fn a_case(id: &str, name: &str, command: &str, status: CaseStatus) -> Case {
    Case {
        id: id.to_owned(),
        name: name.to_owned(),
        task: format!("do the work of {name}"),
        expect: Vec::new(),
        command: command.to_owned(),
        proof: if command.is_empty() {
            String::new()
        } else {
            "0 failed".to_owned()
        },
        status,
        because: "src/guard.rs:14".to_owned(),
        extra: Map::new(),
    }
}

fn a_variant(id: &str, name: &str) -> Variant {
    Variant {
        id: id.to_owned(),
        name: name.to_owned(),
        agent: "0198a1f2-3b4c-7d5e-8f60-112233445566".to_owned(),
        overrides: Map::new(),
        extra: Map::new(),
    }
}

fn a_set(cases: Vec<Case>, variants: Vec<Variant>) -> EvalSet {
    EvalSet {
        format: loadout_lib::lab::CURRENT,
        id: "review-rubric".to_owned(),
        name: "Review rubric".to_owned(),
        subject: Subject::Agent {
            id: "0198a1f2-3b4c-7d5e-8f60-112233445566".to_owned(),
        },
        cases,
        variants,
        extra: Map::new(),
    }
}

#[test]
fn every_accepted_case_meets_every_column_and_no_suggestion_does() {
    let set = a_set(
        vec![
            a_case("one", "Reads the guard", "npm test", CaseStatus::InUse),
            a_case("two", "Names the file", "", CaseStatus::InUse),
            a_case("draft", "Still a draft", "npm test", CaseStatus::Suggested),
        ],
        vec![a_variant("without", "Without"), a_variant("with", "With")],
    );

    let file = plan::compose(
        &set,
        "eval:review-rubric".to_owned(),
        "Review rubric".to_owned(),
    );
    let keys: Vec<&str> = file
        .steps
        .iter()
        .map(loadout_lib::workflow::Step::id)
        .collect();

    for variant in ["without", "with"] {
        for one in ["one", "two"] {
            let work = key_for(one, variant, Half::Work);
            assert!(
                keys.contains(&work.as_str()),
                "the accepted case {one} has no work step in column {variant}, so that cell \
                 could never be measured. What is there: {keys:?}"
            );
        }
        // Przypadek bez komendy nie ma czego uruchomić poza samą pracą: krok „sprawdź" nad
        // pustym poleceniem byłby krokiem, który zawsze przechodzi.
        assert!(
            keys.contains(&key_for("one", variant, Half::Checks).as_str()),
            "the case with a command has no step that runs it in column {variant}"
        );
        assert!(
            !keys.contains(&key_for("two", variant, Half::Checks).as_str()),
            "a case with no command got a step that runs nothing, and a step that runs nothing \
             passes every time"
        );
    }

    for variant in ["without", "with"] {
        assert!(
            !keys.contains(&key_for("draft", variant, Half::Work).as_str()),
            "a case still waiting for a person reached the run in column {variant}. Material \
             that lets itself into a measurement makes the measurement about itself."
        );
    }
}

#[test]
fn every_cell_works_in_its_own_copy_and_stops_its_own_checks() {
    let set = a_set(
        vec![a_case(
            "one",
            "Reads the guard",
            "npm test",
            CaseStatus::InUse,
        )],
        vec![a_variant("without", "Without"), a_variant("with", "With")],
    );
    let file = plan::compose(
        &set,
        "eval:review-rubric".to_owned(),
        "Review rubric".to_owned(),
    );

    let mut work_steps = 0;
    for step in &file.steps {
        match step {
            Step::Agent(one) => {
                work_steps += 1;
                assert_eq!(
                    one.folder,
                    Folder::FreshCopy,
                    "two cells writing in one folder are refused before the first process \
                     starts, and a measurement that changes the project it measures is not a \
                     measurement"
                );
                assert_eq!(
                    one.when_it_fails,
                    WhenItFails::Stop,
                    "work that did not finish has nothing to check, and paying for that check \
                     buys nothing"
                );
            }
            Step::Check(one) => {
                assert_eq!(
                    one.folder,
                    Folder::SameCopy,
                    "the command has to run where the work happened, or it looks at a folder \
                     where nothing was done"
                );
                assert!(
                    !one.proof.trim().is_empty(),
                    "a command with nothing that proves it ran falls back to the exit code, and \
                     a suite that ran no tests exits with zero"
                );
            }
            _ => panic!("a plan may only hold work and the commands that judge it"),
        }
    }
    assert_eq!(
        work_steps, 2,
        "one accepted case across two columns is two cells"
    );
}

#[test]
fn the_plan_is_something_the_engine_agrees_to_run() {
    let set = a_set(
        vec![
            a_case("one", "Reads the guard", "npm test", CaseStatus::InUse),
            a_case("two", "Names the file", "", CaseStatus::InUse),
        ],
        vec![a_variant("without", "Without"), a_variant("with", "With")],
    );
    let file = plan::compose(
        &set,
        "eval:review-rubric".to_owned(),
        "Review rubric".to_owned(),
    );

    let stopping: Vec<String> = check_to_run(&file)
        .into_iter()
        .filter(|note| note.level == Level::Problem)
        .map(|note| note.message)
        .collect();
    assert!(
        stopping.is_empty(),
        "the composed plan would be turned away at Start, so a person would learn about it from \
         a run that never began: {stopping:?}"
    );
}

#[test]
fn what_the_answer_has_to_carry_never_says_what_it_should_say() {
    let mut case = a_case("one", "Reads the guard", "", CaseStatus::InUse);
    case.expect = vec![Expect {
        field: "file".to_owned(),
        contains: "guard.rs".to_owned(),
        describe: String::new(),
    }];
    let set = a_set(vec![case], vec![a_variant("without", "Without")]);
    let file = plan::compose(
        &set,
        "eval:review-rubric".to_owned(),
        "Review rubric".to_owned(),
    );

    let Some(Step::Agent(step)) = file.steps.first() else {
        panic!("the one accepted case has to be there");
    };
    let Handover::Form { fields } = &step.handover else {
        panic!("a case that asks for fields has to ask for them in the shape the run enforces");
    };
    let field = fields.first().expect("the one field it asked for");
    assert_eq!(field.name, "file");
    assert_eq!(
        field.required,
        Some(true),
        "a field that is not needed is a field the run will not miss, so nothing would judge it"
    );
    // TO JEST CAŁE TO KRYTERIUM. Prompt mówiący „w tym polu ma paść guard.rs" mierzy, czy model
    // umie przepisać `guard.rs` — a nie to, czy potrafi wykonać pracę, po której to pada.
    let whole = format!("{} {}", step.instructions, field.describe);
    assert!(
        !whole.contains("guard.rs"),
        "what we expect to read leaked into what the agent is told, so the case now measures \
         whether it can copy that word back: {whole:?}"
    );
}
