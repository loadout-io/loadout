//! AC-4 dla T-27: workflow zapisany przez komendę daje się wczytać i sprawdzić.
//!
//! Na katalogu tymczasowym, przez funkcje `*_inner` — **bez Tauri**, z tego samego powodu, co
//! w `ipc_library_roundtrip.rs`: `State<'_, AppState>` nie da się zbudować w teście, a `&Path`
//! da się w jednym wierszu [04 §2.1].
//!
//! # Przypadek ujemny jest tu ważniejszy od dodatniego
//!
//! **Słaba wersja tego kryterium: sam zapis i odczyt.** Przechodzi, kiedy sprawdzenie zwraca
//! zawsze pustą listę — czyli kiedy komenda gubi uwagi walidatora z T-12. To jest gorsze niż
//! brak tej komendy: front rysuje wtedy zielono plik, który Rust odrzuci przy Starcie, a
//! człowiek dowiaduje się o tym od biegu, który nie ruszył.
//!
//! Rozróżnia je koło. Zdanie jest porównywane **słowo w słowo** z tym, które produkuje
//! `workflow::check` — nie dlatego, że interesuje nas napis, tylko dlatego, że to jedyny
//! sposób, żeby odróżnić uwagę walidatora od uwagi wymyślonej po drodze. Drugi walidator,
//! dopisany w warstwie komend, byłby drugim miejscem, w którym mieszka odpowiedź na pytanie
//! „co jest nie tak z tym plikiem", i jedno z nich zawsze byłoby nieaktualne (niezmiennik 13).

use std::error::Error;

use loadout_lib::commands::workflows::{
    check_workflow_inner, load_workflow_inner, save_workflow_inner,
};
use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::check::{Level, Note};
use serde_json::{Value, json};
use tempfile::TempDir;

/// Zdanie, którym `workflow::check` (T-12) nazywa koło. Żadnego „cycle detected in DAG": to
/// idzie wprost na ekran, więc mówi, co się stanie, a nie jak nazywa się algorytm, który to
/// znalazł (niezmiennik 14).
const CIRCLE: &str = "These steps point back at each other in a circle. Work would never finish.";

/// Nazwa pliku w bibliotece. Sama nazwa, nie ścieżka — katalog rozwiązuje Rust [T3 §8.3].
const FILE: &str = "ship-a-feature.json";

/// Krok kompletny poza tym, co bada dany przypadek.
///
/// Własna kopia folderu nie jest ozdobą: kroki bez strzałki między sobą, które celują w ten sam
/// folder, są osobną odmową walidatora (niezmiennik 12), a fikstura potrafiąca zapalić dwie
/// reguły naraz nie mierzy żadnej z nich.
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

/// Plik workflow złożony z surowego JSON-a, dokładnie tak, jak przyszedłby z okna.
fn workflow(steps: &[Value], links: &[(&str, &str)]) -> Result<WorkflowFile, Box<dyn Error>> {
    let links: Vec<Value> = links
        .iter()
        .map(|(from, to)| json!({ "from": from, "to": to }))
        .collect();
    let file = json!({
        "format": 1,
        "id": "wf_ship_a_feature",
        "name": "Ship a feature",
        "steps": steps,
        "links": links
    });
    Ok(serde_json::from_value(file)?)
}

/// Prosty łańcuch: dwa kroki i strzałka. Najzwyklejszy workflow, jaki istnieje.
fn a_plain_chain() -> Result<WorkflowFile, Box<dyn Error>> {
    workflow(
        &[step("plan", "Plan"), step("build", "Build")],
        &[("plan", "build")],
    )
}

#[test]
fn a_workflow_saved_by_the_command_loads_back_field_for_field() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let plan = a_plain_chain()?;

    let written = save_workflow_inner(home.path(), FILE, &plan, None)?.path;
    assert!(
        written.exists(),
        "the command said it saved the workflow and {} is not there. A save that reports \
         success without leaving a file is the one defect this whole roundtrip exists to catch",
        written.display()
    );

    let back = load_workflow_inner(home.path(), FILE)?;
    assert_eq!(
        back.workflow, plan,
        "the workflow comes back the way it went in — every step, every arrow, every setting. \
         This is the one thing in Loadout a person can lose, so 'roughly the same' is not a \
         state this comparison is allowed to have"
    );
    Ok(())
}

#[test]
fn the_check_command_stays_quiet_about_a_plain_chain() -> Result<(), Box<dyn Error>> {
    let plan = a_plain_chain()?;

    let notes = check_workflow_inner(TempDir::new()?.path(), &plan);

    assert!(
        notes.is_empty(),
        "two steps and one arrow is the most ordinary workflow there is; a note on it is a \
         false alarm, and false alarms are how a validator gets switched off. Got: {notes:?}"
    );
    Ok(())
}

#[test]
fn a_workflow_that_points_back_at_itself_comes_back_with_the_validator_s_note()
-> Result<(), Box<dyn Error>> {
    let circle = workflow(
        &[
            step("plan", "Plan"),
            step("build", "Build"),
            step("ship", "Ship"),
        ],
        &[("plan", "build"), ("build", "ship"), ("ship", "plan")],
    )?;

    let notes = check_workflow_inner(TempDir::new()?.path(), &circle);
    let problems: Vec<&Note> = notes
        .iter()
        .filter(|note| note.level == Level::Problem)
        .collect();

    assert_eq!(
        problems.len(),
        1,
        "three steps closed in a circle are one thing to fix. An empty list here is the whole \
         point of this case: a command that drops what the validator said is worse than no \
         command at all, because the window then draws in green what Rust will turn down. \
         Got: {notes:?}"
    );

    let refusal = problems[0];
    assert_eq!(
        refusal.message, CIRCLE,
        "the sentence is the validator's own, word for word. A second one, written inside the \
         command layer, is a second answer to 'what is wrong with this file' — and one of two \
         answers is always out of date (invariant 13)"
    );
    let named = refusal
        .step_id
        .as_deref()
        .ok_or("the note has to land on a step: a red dot with no step is nothing to click")?;
    assert!(
        ["plan", "build", "ship"].contains(&named),
        "the note belongs on one of the steps on the circle, not on some other one; got {named}"
    );
    Ok(())
}
