//! AC-5 dla T-12: krok, którego nikt nie podłączył, jest **ostrzeżeniem** — także wtedy, gdy
//! jest ich dwa.
//!
//! Obchód **ignoruje kierunek strzałek**, i to jest cała treść tego kryterium. T3 §5.2 napisał
//! wersję skierowaną, uruchomił ją i **nigdy nie wystrzeliła**: w grafie bez kół obchód
//! z każdego wierzchołka bez wejść dociera zawsze wszędzie. Reguła, która nie umie zaświecić,
//! jest gorsza niż jej brak, bo zajmuje miejsce reguły, która by zaświeciła.
//!
//! Słabą wersją jest jedna fixture z jednym samotnym krokiem. Przechodzi ją zarówno obchód
//! nieskierowany, jak i liczenie strzałek przy kroku („zero strzałek = samotny") — a to drugie
//! przepuszcza **całą wyspę**: dwa kroki połączone tylko ze sobą mają po jednej strzałce, więc
//! licznik ich nie widzi. Dlatego fixture (b) jest tu jedyną, która naprawdę rozróżnia.
//!
//! Poziom to `Warning`, nie `Problem`: taki workflow wolno uruchomić, a wyspa bywa świadoma —
//! ktoś odłączył krok na chwilę i wróci do niego.

use std::error::Error;

use serde_json::{Value, json};

use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::check::{Level, Note, check};

/// Zdanie z uruchomienia w T3 §5.2. Nazywa krok jego nazwą, nie identyfikatorem: „s_lonely"
/// nie jest niczym, co użytkownik widzi na płótnie.
const LONELY: &str = "\"Lonely step\" is not connected to the rest of the workflow.";

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

fn at_level(notes: &[Note], level: Level) -> Vec<&Note> {
    notes.iter().filter(|note| note.level == level).collect()
}

#[test]
fn a_step_nobody_wired_up_is_one_warning_that_names_it() -> Result<(), Box<dyn Error>> {
    let workflow = workflow(
        &[
            step("a", "Plan"),
            step("b", "Build"),
            step("c", "Ship"),
            step("s_lonely", "Lonely step"),
        ],
        &[("a", "b"), ("b", "c")],
    )?;

    let notes = check(&workflow);
    let warnings = at_level(&notes, Level::Warning);

    assert!(
        at_level(&notes, Level::Problem).is_empty(),
        "a step left unconnected is worth saying out loud, but the workflow still runs — a \
         Problem here would block Run over something that is not broken. Got: {notes:?}"
    );
    assert_eq!(
        warnings.len(),
        1,
        "one loose step is one warning. Got: {notes:?}"
    );
    assert_eq!(
        warnings[0].step_id.as_deref(),
        Some("s_lonely"),
        "the amber dot goes on the loose step, not on the chain that is fine"
    );
    assert_eq!(
        warnings[0].message, LONELY,
        "the sentence names the step the way the canvas does; 'orphan node' names neither the \
         step nor the fix"
    );
    Ok(())
}

#[test]
fn two_steps_connected_only_to_each_other_are_still_an_island() -> Result<(), Box<dyn Error>> {
    let workflow = workflow(
        &[
            step("a", "Plan"),
            step("b", "Build"),
            step("c", "Ship"),
            step("x", "Draft a memo"),
            step("y", "Send the memo"),
        ],
        &[("a", "b"), ("b", "c"), ("x", "y")],
    )?;

    let notes = check(&workflow);
    let warnings = at_level(&notes, Level::Warning);

    assert!(
        at_level(&notes, Level::Problem).is_empty(),
        "an island is still runnable — it just never gets its turn. Got: {notes:?}"
    );
    assert!(
        !warnings.is_empty(),
        "this is the case a directed walk cannot see: `x` has nothing pointing at it, so a walk \
         from every step-with-no-arrows-in reaches `x` and `y` and calls them connected. \
         Nothing said, and two steps quietly never run. Got: {notes:?}"
    );
    let named: Vec<Option<&str>> = warnings
        .iter()
        .map(|note| note.step_id.as_deref())
        .collect();
    assert!(
        named.iter().all(|id| matches!(*id, None | Some("x" | "y"))),
        "the warning belongs to the island — naming a step from the main chain sends the user to \
         a tile that is fine. Whether the note carries one tile or none is the implementation's \
         call; carrying the wrong one is not. Got: {named:?}"
    );
    assert!(
        warnings.iter().any(|note| {
            note.message.contains("Draft a memo") || note.message.contains("Send the memo")
        }),
        "and the island has to be named in words, because the message is the whole of what the \
         user gets to read. Got: {warnings:?}"
    );
    Ok(())
}

#[test]
fn a_workflow_of_one_step_is_quiet() -> Result<(), Box<dyn Error>> {
    let workflow = workflow(&[step("a", "Plan")], &[])?;

    let notes = check(&workflow);

    assert!(
        notes.is_empty(),
        "with one step there is nothing to be disconnected from, and a warning on the smallest \
         possible workflow is the fastest way to teach people to ignore warnings. Got: {notes:?}"
    );
    Ok(())
}
