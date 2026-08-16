//! AC-3 dla T-12: koło jest odmową i wskazuje kafelek; **romb nią nie jest**.
//!
//! Słaba wersja to `assert!(!check(&wf).is_empty())` na pliku z kołem. Przechodzi ją walidator,
//! który zgłasza cokolwiek na czymkolwiek — na przykład „ten krok ma więcej niż jednego
//! rodzica". Rozróżnia je wyłącznie **romb**: cztery kroki, gdzie `d` czeka na `b` i na `c`,
//! muszą dać **zero** problemów, bo „poczekaj na wszystkich" to jedna z pięciu rzeczy, które
//! silnik w ogóle umie (T3 §6.1 punkt 3). Walidator, który odmawia rombu, kasuje syntezę
//! wyników — czyli jedno z pięciu zadań edytora (D6).
//!
//! Wszystkie kroki w tym pliku pracują we własnej kopii folderu. To nie jest ozdoba: kroki
//! bez strzałki między sobą, które celują w ten sam folder, są osobną odmową (AC-4), a fixture,
//! która potrafi zapalić dwie reguły naraz, nie mierzy żadnej z nich.

use std::error::Error;

use serde_json::{Value, json};

use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::check::{Level, Note, check};

/// Zdanie z uruchomienia w T3 §5.2. Żadnego „cycle detected in DAG": `cycle`, `DAG`, `node`
/// i `in-degree` są w tekście dla użytkownika zakazane tak samo, jak w komponencie Reacta
/// (niezmiennik 14).
const CIRCLE: &str = "These steps point back at each other in a circle. Work would never finish.";

/// Krok kompletny poza tym, co bada dany przypadek: ma nazwę, instrukcje i własny folder.
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

/// Uwagi, które blokują Run i zapis.
fn problems(notes: &[Note]) -> Vec<&Note> {
    notes
        .iter()
        .filter(|note| note.level == Level::Problem)
        .collect()
}

#[test]
fn three_steps_closed_in_a_circle_are_one_problem_that_names_a_tile() -> Result<(), Box<dyn Error>>
{
    let workflow = workflow(
        &[step("a", "Plan"), step("b", "Build"), step("c", "Ship")],
        &[("a", "b"), ("b", "c"), ("c", "a")],
    )?;

    let notes = check(&workflow);
    let problems = problems(&notes);

    assert_eq!(
        problems.len(),
        1,
        "one circle is one thing to fix; three notes for one mistake read as three mistakes. \
         Got: {notes:?}"
    );
    let problem = problems[0];
    assert_eq!(
        problem.message, CIRCLE,
        "this sentence goes straight onto the screen, so it says what happens instead of \
         naming the algorithm that found it"
    );
    let named = problem
        .step_id
        .as_deref()
        .ok_or("the note has to land on a tile: a red dot with no tile is nothing to click")?;
    assert!(
        ["a", "b", "c"].contains(&named),
        "the badge belongs on one of the steps on the circle, not on some other tile; got \
         {named}"
    );
    Ok(())
}

#[test]
fn a_step_that_waits_for_itself_is_a_problem() -> Result<(), Box<dyn Error>> {
    let workflow = workflow(&[step("a", "Plan")], &[("a", "a")])?;

    let notes = check(&workflow);
    let problems = problems(&notes);

    assert_eq!(
        problems.len(),
        1,
        "a step waiting for itself can never start, so it is exactly one problem. Got: {notes:?}"
    );
    assert_eq!(
        problems[0].step_id.as_deref(),
        Some("a"),
        "there is one step in this workflow, so there is one tile the note can point at"
    );
    Ok(())
}

#[test]
fn a_diamond_is_not_a_circle() -> Result<(), Box<dyn Error>> {
    let workflow = workflow(
        &[
            step("a", "Plan"),
            step("b", "Research"),
            step("c", "Check"),
            step("d", "Ship"),
        ],
        &[("a", "b"), ("a", "c"), ("b", "d"), ("c", "d")],
    )?;

    let notes = check(&workflow);

    assert!(
        problems(&notes).is_empty(),
        "`d` waits for two steps and that is 'wait for all', one of the five things this \
         product does — refusing it deletes the whole idea of putting results together. \
         Got: {notes:?}"
    );
    Ok(())
}

#[test]
fn twenty_steps_in_a_row_are_quiet() -> Result<(), Box<dyn Error>> {
    let names: Vec<String> = (0..20).map(|index| format!("s{index}")).collect();
    let steps: Vec<Value> = names
        .iter()
        .map(|id| step(id, "Do a piece of the work"))
        .collect();
    let links: Vec<(&str, &str)> = names
        .windows(2)
        .map(|pair| (pair[0].as_str(), pair[1].as_str()))
        .collect();

    let workflow = workflow(&steps, &links)?;

    let notes = check(&workflow);

    assert!(
        notes.is_empty(),
        "a plain chain is the most ordinary workflow there is; a note on it is a false alarm, \
         and false alarms are how a validator gets switched off. Got: {notes:?}"
    );
    Ok(())
}
