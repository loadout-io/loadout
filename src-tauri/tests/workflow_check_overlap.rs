//! AC-4 dla T-12: dwa kroki, które **mogą biec równocześnie**, nie piszą do jednego folderu —
//! i odmowa pada przy zapisie, nie w trakcie biegu (niezmiennik 12).
//!
//! „Mogą biec równocześnie" znaczy dokładnie jedno: **nie istnieje ścieżka po strzałkach** ani
//! z A do B, ani z B do A. Reguła, która porównuje folder na *wszystkich* parach kroków, jest
//! tą samą regułą pozbawioną tego zdania — i wtedy zwykły łańcuch `plan → build` jest odmową.
//! Ktoś zgłasza to jako błąd, ktoś inny „naprawia" regułę przez wyłączenie jej i zostaje martwy
//! kod. Dlatego przypadek (a) — łańcuch dzielący folder projektu — musi dać **zero** uwag.
//!
//! Słabą wersją jest porównanie pola `folder` przez `==`. Przechodzi przypadki (b) i (c),
//! a wykłada się na (d) i (e) — czyli dokładnie na tych dwóch, w których agenci naprawdę
//! nadpisują sobie pliki: zagnieżdżona ścieżka i jeden krok w kilku kopiach. Oba są w tym
//! samym pliku i to one nadają temu kryterium sens.
//!
//! Zagnieżdżenie porównujemy **po segmentach**, nie po prefiksie stringa: `/Users/x/api2` nie
//! leży w `/Users/x/api`, choć zaczyna się tymi samymi znakami. Fixture (d2) jest tu po to,
//! żeby najtańsza implementacja — `starts_with` na tekście — świeciła na czerwono.

use std::error::Error;

use serde_json::{Value, json};

use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::check::{Level, Note, check};

/// Zdanie z kryterium. Nazywa **oba** kroki — bez nich użytkownik wie, że coś koliduje, ale nie
/// wie z czym — i mówi, co zrobić.
const AT_THE_SAME_TIME: &str = "\"Research\" and \"Check\" can run at the same time and both \
     work in the project folder. Give one of them a fresh copy.";

/// Krok o zadanym folderze. Wszystko poza folderem jest kompletne, żeby żadna inna reguła nie
/// dołożyła drugiej uwagi do fixture, która mierzy tę jedną.
fn step(id: &str, name: &str, folder: Value) -> Value {
    json!({
        "kind": "agent",
        "id": id,
        "name": name,
        "agent": "a_forge",
        "instructions": "Do the work.",
        "folder": folder
    })
}

fn project() -> Value {
    json!({ "use": "project" })
}

fn fresh_copy() -> Value {
    json!({ "use": "fresh-copy" })
}

fn pick(path: &str) -> Value {
    json!({ "use": "pick", "path": path })
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
fn a_chain_may_share_the_project_folder() -> Result<(), Box<dyn Error>> {
    let workflow = workflow(
        &[
            step("a", "Research", project()),
            step("b", "Check", project()),
        ],
        &[("a", "b")],
    )?;

    let notes = check(&workflow);

    assert!(
        notes.is_empty(),
        "`b` starts after `a` finishes, so they never write at the same time — this is the \
         most ordinary workflow there is and refusing it makes the rule the first thing \
         somebody switches off. Got: {notes:?}"
    );
    Ok(())
}

#[test]
fn two_steps_with_no_arrow_between_them_may_not_share_the_project_folder()
-> Result<(), Box<dyn Error>> {
    let workflow = workflow(
        &[
            step("a", "Research", project()),
            step("b", "Check", project()),
        ],
        &[],
    )?;

    let notes = check(&workflow);
    let problems = problems(&notes);

    assert_eq!(
        problems.len(),
        1,
        "one collision between two steps is one thing to fix. Got: {notes:?}"
    );
    assert_eq!(
        problems[0].message, AT_THE_SAME_TIME,
        "the message has to name both steps and say what to do; 'path conflict' names neither"
    );
    Ok(())
}

#[test]
fn a_fresh_copy_takes_the_collision_away() -> Result<(), Box<dyn Error>> {
    let workflow = workflow(
        &[
            step("a", "Research", project()),
            step("b", "Check", fresh_copy()),
        ],
        &[],
    )?;

    let notes = check(&workflow);

    assert!(
        problems(&notes).is_empty(),
        "'a fresh copy just for this step' is the answer the message tells the user to pick, \
         so taking it has to actually solve the problem. Got: {notes:?}"
    );
    Ok(())
}

#[test]
fn a_folder_inside_the_other_folder_is_the_same_collision() -> Result<(), Box<dyn Error>> {
    let workflow = workflow(
        &[
            step("a", "Research", pick("/Users/x/api")),
            step("b", "Check", pick("/Users/x/api/src")),
        ],
        &[],
    )?;

    let notes = check(&workflow);
    let problems = problems(&notes);

    assert_eq!(
        problems.len(),
        1,
        "one step writing inside the other's folder is the same overwriting, so it is the same \
         one problem — comparing the two folders with `==` misses it entirely. Got: {notes:?}"
    );
    let message = &problems[0].message;
    assert!(
        message.contains("Research") && message.contains("Check"),
        "both steps have to be named: the user has to know which pair to separate. It reads: \
         {message}"
    );
    Ok(())
}

#[test]
fn a_folder_that_merely_starts_with_the_same_letters_is_not_a_collision()
-> Result<(), Box<dyn Error>> {
    let workflow = workflow(
        &[
            step("a", "Research", pick("/Users/x/api")),
            step("b", "Check", pick("/Users/x/api2")),
        ],
        &[],
    )?;

    let notes = check(&workflow);

    assert!(
        problems(&notes).is_empty(),
        "`/Users/x/api2` is a different folder that happens to share a prefix; refusing it \
         means the rule compares text instead of path segments, and then the user is told to \
         fix something that is not broken. Got: {notes:?}"
    );
    Ok(())
}

#[test]
fn a_step_in_several_copies_collides_with_itself() -> Result<(), Box<dyn Error>> {
    let mut step = step("a", "Research", project());
    step["copies"] = json!(3);
    let workflow = workflow(&[step], &[])?;

    let notes = check(&workflow);
    let problems = problems(&notes);

    assert_eq!(
        problems.len(),
        1,
        "three copies of one step run at the same time by definition, so one folder for all \
         three is three agents overwriting one another. T3 wanted a hint here; invariant 12 \
         says the refusal lands at save time, and the invariant wins. Got: {notes:?}"
    );
    assert_eq!(
        problems[0].step_id.as_deref(),
        Some("a"),
        "the badge belongs on the step that carries the copies"
    );
    Ok(())
}
