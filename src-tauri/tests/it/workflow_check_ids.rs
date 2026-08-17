//! AC-6 dla T-12: plik wewnętrznie niespójny dostaje **po jednej uwadze na defekt**, ze
//! wskazaniem winnego.
//!
//! Cztery osobne fixture, każda z jednym defektem — i to jest cała konstrukcja tego kryterium.
//! Słabą wersją jest jedna fixture ze wszystkimi czterema defektami naraz i
//! `assert_eq!(notes.len(), 4)`: przechodzi ją implementacja, w której jedna reguła strzeliła
//! cztery razy, a trzech pozostałych nie ma. Rozdzielenie fixture plus asercja na `step_id`
//! przy każdej jest jedynym układem, który to rozróżnia.
//!
//! Liczymy uwagi poziomu `Problem`. Reguła spójności (AC-5) mówi o tych samych fixture co
//! najwyżej ostrzeżeniem, a ostrzeżenie nie jest defektem, o który pyta to kryterium — poza
//! przypadkiem pustego pliku, gdzie nie ma czego ostrzegać i liczymy wszystko.

use std::error::Error;

use serde_json::{Value, json};

use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::check::{Level, Note, check};

/// Zdanie z kryterium. Nie „steps array is empty": pusty workflow to nie jest błąd danych,
/// tylko stan, w którym użytkownik jeszcze nic nie zrobił.
const NO_STEPS: &str = "There are no steps yet.";

fn step(id: &str, name: &str) -> Value {
    json!({
        "kind": "agent",
        "id": id,
        "name": name,
        "agent": "a_forge",
        "instructions": "Do the work.",
        "folder": { "use": "fresh-copy" }
    })
}

fn workflow(steps: &[Value], links: &[(&str, &str)]) -> Result<WorkflowFile, Box<dyn Error>> {
    let links: Vec<Value> = links
        .iter()
        .map(|(from, to)| json!({ "from": from, "to": to }))
        .collect();
    let file = json!({
        "format": 1,
        "id": "wf_test",
        "name": "Test workflow",
        "steps": steps,
        "links": links
    });
    Ok(serde_json::from_value(file)?)
}

fn problems(notes: &[Note]) -> Vec<&Note> {
    notes
        .iter()
        .filter(|note| note.level == Level::Problem)
        .collect()
}

#[test]
fn two_steps_with_one_id_are_one_problem_naming_that_id() -> Result<(), Box<dyn Error>> {
    let workflow = workflow(&[step("s1", "Plan"), step("s1", "Build")], &[])?;

    let notes = check(&workflow);
    let problems = problems(&notes);

    assert_eq!(
        problems.len(),
        1,
        "two steps answering to one name is one defect; every arrow that points at `s1` \
         otherwise means two different things at once. Got: {notes:?}"
    );
    assert_eq!(
        problems[0].step_id.as_deref(),
        Some("s1"),
        "the note has to name the id that is doubled — without it the user is left searching"
    );
    Ok(())
}

#[test]
fn an_arrow_into_a_step_that_does_not_exist_is_one_problem_naming_it() -> Result<(), Box<dyn Error>>
{
    let workflow = workflow(
        &[step("s1", "Plan"), step("s2", "Build")],
        &[("s1", "s2"), ("s1", "s9")],
    )?;

    let notes = check(&workflow);
    let problems = problems(&notes);

    assert_eq!(
        problems.len(),
        1,
        "one arrow into nowhere is one defect, and the healthy arrow beside it stays silent. \
         Got: {notes:?}"
    );
    let problem = problems[0];
    assert!(
        problem.message.contains("s9"),
        "the message has to say which end is missing; 'invalid link' sends the user to read \
         the file by hand. It reads: {}",
        problem.message
    );
    assert_ne!(
        problem.step_id.as_deref(),
        Some("s9"),
        "clicking a note focuses that tile, so pointing it at a step that does not exist turns \
         the note into a dead link"
    );
    Ok(())
}

#[test]
fn a_workflow_with_no_steps_is_one_problem_with_no_tile() -> Result<(), Box<dyn Error>> {
    let workflow = workflow(&[], &[])?;

    let notes = check(&workflow);

    assert_eq!(
        notes.len(),
        1,
        "an empty workflow is one thing to say, once. Got: {notes:?}"
    );
    assert_eq!(
        notes[0].level,
        Level::Problem,
        "there is nothing to run, so Run may not be offered"
    );
    assert_eq!(
        notes[0].step_id, None,
        "there is no tile to put a dot on, and inventing one would focus a step that is not there"
    );
    assert_eq!(
        notes[0].message, NO_STEPS,
        "plain English about the state the user is actually in"
    );
    Ok(())
}

#[test]
fn more_copies_than_the_machine_can_carry_is_one_problem() -> Result<(), Box<dyn Error>> {
    let mut nine = step("s1", "Research");
    nine["copies"] = json!(9);
    let workflow = workflow(&[nine], &[])?;

    let notes = check(&workflow);
    let problems = problems(&notes);

    assert_eq!(
        problems.len(),
        1,
        "eight sessions at once on a real machine is already a lot; nine is one defect, not a \
         preference. Got: {notes:?}"
    );
    let problem = problems[0];
    assert_eq!(
        problem.step_id.as_deref(),
        Some("s1"),
        "the note lands on the step that carries the number"
    );
    assert!(
        problem.message.contains(' ') && problem.message.ends_with('.'),
        "this goes straight onto the screen, so it is a sentence and not a range check printed \
         as code. It reads: {}",
        problem.message
    );
    Ok(())
}

#[test]
fn a_step_that_runs_zero_times_is_one_problem() -> Result<(), Box<dyn Error>> {
    let mut none = step("s1", "Research");
    none["copies"] = json!(0);
    let workflow = workflow(&[none], &[])?;

    let notes = check(&workflow);
    let problems = problems(&notes);

    assert_eq!(
        problems.len(),
        1,
        "zero copies is a step that sits on the canvas and never runs — the same defect from \
         the other end of the range. Got: {notes:?}"
    );
    assert_eq!(
        problems[0].step_id.as_deref(),
        Some("s1"),
        "the note lands on the step that carries the number"
    );
    Ok(())
}
