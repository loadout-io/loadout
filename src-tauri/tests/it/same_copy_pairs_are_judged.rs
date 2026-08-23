//! AC-3 dla T-95: dwa kroki „to samo drzewo, w którym pracował krok przede mną", gotowe naraz,
//! są kolizją — i mówi to walidator, a nie dopiero bieg.
//!
//! # Co to mierzy
//!
//! `one_folder_two_steps` sądzi parę kroków po ich FOLDERACH, a `same-copy` folderu sam z siebie
//! nie zna: „to samo drzewo, co krok przede mną" jest zdaniem o grafie. Do dziś ta para wpadała
//! więc do reguły bez odpowiedzi i wychodziła z niej bez uwagi — przyznane wprost w komentarzu
//! przy `the_same_files`. Skutek jest dokładnie tą kolizją, przed którą stoi niezmiennik 12: dwa
//! kroki po jednej `fresh-copy`, bez strzałki między sobą, dostają JEDEN katalog i piszą po
//! sobie nawzajem, a oba kończą się sukcesem.
//!
//! Rozwiązanie jest jedno i już istnieje w biegu: `same-copy` schodzi do drzewa najbliższego
//! poprzednika (`commands::run::trees_before`). Walidator ma pytać tak samo — druga reguła
//! odpowiadałaby na to samo pytanie inaczej i rozjechałaby się przy pierwszej poprawce jednej
//! z nich.
//!
//! # SŁABE WERSJE, i po jednej asercji na każdą
//!
//! - **`same-copy` sprowadzone do folderu projektu.** Wtedy para też dostaje uwagę — tylko
//!   zdaniem, które kłamie: te kroki nie pracują w folderze człowieka i naprawa „dajcie jednemu
//!   własną kopię" brzmi nad nim absurdalnie. Rozstrzyga asercja o treści zdania.
//! - **„każde dwa `same-copy` kolidują".** Przechodzi przypadek (a) i przewraca (c): dwa
//!   łańcuchy obok siebie, każdy na swojej kopii, nie mają o co kolidować.
//! - **„`same-copy` nie koliduje nigdy", czyli dzisiejszy stan.** Przechodzi (b) i (c).

use std::error::Error;

use serde_json::{Value, json};

use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::check::{Level, Note, check, check_to_run};

/// Krok agenta o zadanym folderze. Wszystko poza folderem jest kompletne, żeby żadna inna
/// reguła nie dołożyła drugiej uwagi do fixture, która mierzy tę jedną.
fn step(id: &str, name: &str, folder: &Value) -> Value {
    json!({
        "kind": "agent",
        "id": id,
        "name": name,
        "agent": "a_forge",
        "instructions": "Do the work.",
        "folder": folder
    })
}

fn fresh_copy() -> Value {
    json!({ "use": "fresh-copy" })
}

fn same_copy() -> Value {
    json!({ "use": "same-copy" })
}

fn workflow(steps: &[Value], links: &[(&str, &str)]) -> Result<WorkflowFile, Box<dyn Error>> {
    let links: Vec<Value> = links
        .iter()
        .map(|(from, to)| json!({ "from": from, "to": to }))
        .collect();
    let file = json!({
        "format": 1,
        "id": "wf_same_copy_pairs",
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

fn warnings(notes: &[Note]) -> Vec<&Note> {
    notes
        .iter()
        .filter(|note| note.level == Level::Warning)
        .collect()
}

/// Dwa kroki na jednej kopii, bez strzałki między sobą.
fn a_fork_over_one_copy() -> Result<WorkflowFile, Box<dyn Error>> {
    workflow(
        &[
            step("s_make", "Build", &fresh_copy()),
            step("s_left", "Review", &same_copy()),
            step("s_right", "Docs", &same_copy()),
        ],
        &[("s_make", "s_left"), ("s_make", "s_right")],
    )
}

// ── (a) para na jednej kopii: ostrzeżenie przy zapisie, problem przy Run ────────────────────

#[test]
fn two_steps_on_one_copy_warn_at_save_and_refuse_at_run() -> Result<(), Box<dyn Error>> {
    let file = a_fork_over_one_copy()?;

    // Przy zapisie: OSTRZEŻENIE. Szkic, w którym kafelki leżą, zanim człowiek pociągnie
    // strzałki, ma się zapisać — ta sama waga, co przy parze `project`/`project`.
    let saving = check(&file);
    assert!(
        problems(&saving).is_empty(),
        "saving this workflow was refused. Two tiles dropped on the canvas before the arrows \
         are drawn is the ordinary shape of building one, and a save that refuses deletes the \
         person's work while they are doing it. Got: {:?}",
        problems(&saving)
    );
    let warned = warnings(&saving);
    assert_eq!(
        warned.len(),
        1,
        "\"Review\" and \"Docs\" both work in the copy \"Build\" made, and no arrow orders them, \
         so they can run at the same time and write over each other. That is the collision this \
         validator exists to name, and it says nothing about it. Notes: {saving:?}"
    );

    // Przy Run: PROBLEM. Odmowa pada, zanim ruszy pierwszy krok (niezmiennik 12).
    let running = check_to_run(&file);
    let refused = problems(&running);
    assert_eq!(
        refused.len(),
        1,
        "pressing Run over this file has to be refused before anything starts. Got: {running:?}"
    );

    // Zdanie jest to samo w obu wagach i NAZYWA OBA KROKI: uwaga, która mówi „coś koliduje",
    // zostawia szukanie po całym płótnie.
    let message = &refused[0].message;
    assert_eq!(
        message, &warned[0].message,
        "the same collision has to read the same way at save and at Run; two descriptions of \
         one problem send the person looking for two"
    );
    for name in ["Review", "Docs"] {
        assert!(
            message.contains(name),
            "the sentence does not name \"{name}\", so the person is told that two steps collide \
             and has to work out which. It said: {message}"
        );
    }

    // I NIE mówi o folderze projektu. Najtańsza zła implementacja sprowadza `same-copy` do
    // folderu człowieka — dostaje wtedy uwagę o tej parze i opisuje ją zdaniem, które jest
    // nieprawdą: te kroki pracują w kopii, którą założył bieg, a nie w plikach człowieka.
    assert!(
        !message.contains("project folder"),
        "the sentence says these two work in the project folder. They do not: they share the \
         copy the run laid out for the step before them, and reading it this way sends the \
         person to look at files nothing is touching. It said: {message}"
    );

    // Kropka ląduje na jednym z tych dwóch kafelków, nie gdzie indziej.
    assert!(
        matches!(refused[0].step_id.as_deref(), Some("s_left" | "s_right")),
        "the note points at {:?}, which is not one of the two steps it is about",
        refused[0].step_id
    );

    Ok(())
}

// ── (b) łańcuch na jednej kopii zostaje poprawny ────────────────────────────────────────────

#[test]
fn a_chain_of_steps_sharing_one_copy_is_still_fine() -> Result<(), Box<dyn Error>> {
    let file = workflow(
        &[
            step("s_make", "Build", &fresh_copy()),
            step("s_check", "Check", &same_copy()),
            step("s_fix", "Fix", &same_copy()),
        ],
        &[("s_make", "s_check"), ("s_check", "s_fix")],
    )?;

    // Sądzone `check_to_run`, czyli najsurowszym pytaniem, jakie ten walidator zna: wersja
    // pytająca tylko `check` przechodziłaby także po wyłączeniu reguły.
    let notes = check_to_run(&file);
    assert!(
        notes.is_empty(),
        "the chain \"one tree, one step after another\" is the shape this option was added for \
         (T-56): implementation, then the check, then the fix, all in one folder. A rule that \
         refuses it is a rule somebody will switch off. Got: {notes:?}"
    );
    Ok(())
}

// ── (c) dwa łańcuchy obok siebie nie kolidują ───────────────────────────────────────────────

#[test]
fn two_chains_each_on_its_own_copy_do_not_collide() -> Result<(), Box<dyn Error>> {
    // Jeden plan, dwie gałęzie, każda ze swoją kopią i swoją poprawką. Wszystko spięte
    // strzałkami, żeby to kryterium mierzyło kolizje, a nie regułę o wyspach.
    let file = workflow(
        &[
            step("s_head", "Plan", &fresh_copy()),
            step("s_one", "Front", &fresh_copy()),
            step("s_one_after", "Front review", &same_copy()),
            step("s_two", "Back", &fresh_copy()),
            step("s_two_after", "Back review", &same_copy()),
        ],
        &[
            ("s_head", "s_one"),
            ("s_head", "s_two"),
            ("s_one", "s_one_after"),
            ("s_two", "s_two_after"),
        ],
    )?;

    let notes = check_to_run(&file);
    assert!(
        notes.is_empty(),
        "\"Front review\" and \"Back review\" work in two different copies, so they have nothing \
         to collide over. A rule that answers \"yes\" for every pair of same-copy steps refuses \
         the ordinary shape of two branches side by side. Got: {notes:?}"
    );
    Ok(())
}
