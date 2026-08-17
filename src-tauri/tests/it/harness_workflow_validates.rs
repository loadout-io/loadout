//! AC-1 dla T-23: graf harnessu przechodzi przez PRAWDZIWY walidator T-12, a walidator ma co
//! odrzucić.
//!
//! Kryterium mierzy dwie rzeczy naraz i żadna z nich sama nie wystarcza. `check()`, które nie ma
//! nic do powiedzenia o `.loadout/workflows/ship-task.json`, znaczy dokładnie tyle samo, co
//! walidator przyjmujący wszystko — w tym plik z wiszącą strzałką. Dlatego ten sam plik
//! z dopisaną strzałką w krok, którego nie ma, musi zostać odrzucony, a odmowa ma ten krok
//! **nazwać**: ogólne „this workflow is invalid" przeszłoby także wtedy, gdyby T-12 nie
//! sprawdzał niczego poza składnią JSON-a.
//!
//! Liczba kroków jest częścią kontraktu, nie szczegółem. Szósty kafelek znaczy, że któryś etap
//! harnessu dostał własny krok — a dopisanie brakującego rodzaju kafelka po cichu, żeby graf się
//! zmieścił, jest dokładnie tą cichą awarią, dla której T-23 powstało.
//!
//! Odmowa pada przy ZAPISIE i nic nie zostaje zapisane (niezmiennik 12). Samo `check()` byłoby
//! prawdą o funkcji, nie o granicy: implementacja, która zapisuje, a waliduje po zapisie, niszczy
//! poprzednią wersję pliku dokładnie w tej chwili, w której sprawdzenie miało jej bronić.
//!
//! Czerwień w warstwie `before` pada na WŁASNYM komunikacie. `fs::read_to_string` na
//! nieistniejącym pliku daje `No such file or directory`, a ten ciąg jest na liście fałszywych
//! czerwieni — bramka odrzuciłaby taką czerwień jako niebyłą.

use std::error::Error;
use std::path::{Path, PathBuf};

use loadout_lib::workflow::check::{Level, Note, check};
use loadout_lib::workflow::file::{self, CURRENT};
use loadout_lib::workflow::{Link, Step, WorkflowFile};

/// Kroki w kolejności z pliku — `steps` jest kolejnością wstawiania i nigdy nie jest sortowane
/// (T3 §8.2 reguła 2), więc porównanie jest deterministyczne.
const EXPECTED: [&str; 5] = ["s_implement", "s_gate", "s_review", "s_fix", "s_land"];

/// Krok, którego w pliku nie ma i mieć nie będzie. Strzałka w niego jest jedynym naruszeniem,
/// jakie ta fikstura wnosi.
const NOWHERE: &str = "s_nope";

fn graph_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../.loadout/workflows/ship-task.json")
}

/// Wczytuje graf publiczną powierzchnią T-12 — nie własnym `serde_json::from_str`. Drugi parser
/// w tym pliku dałby dwa źródła prawdy i kryterium, które nie certyfikuje niczego.
fn load() -> Result<WorkflowFile, Box<dyn Error>> {
    let path = graph_path();
    assert!(
        path.exists(),
        "the harness workflow has not been written yet: {}",
        path.display()
    );
    Ok(file::load(&path)?)
}

/// Identyfikator kroku, niezależnie od jego rodzaju.
fn id(step: &Step) -> &str {
    match step {
        Step::Agent(agent) => &agent.id,
        Step::Checkpoint(checkpoint) => &checkpoint.id,
    }
}

fn problems(notes: &[Note]) -> Vec<&Note> {
    notes
        .iter()
        .filter(|note| note.level == Level::Problem)
        .collect()
}

/// Ten sam graf ze strzałką w nieistniejący krok. Reszta pliku zostaje nietknięta, żeby odmowa
/// mogła mieć tylko jeden powód.
fn with_an_arrow_into_nowhere() -> Result<WorkflowFile, Box<dyn Error>> {
    let mut workflow = load()?;
    workflow.links.push(Link {
        from: EXPECTED[4].to_owned(),
        to: NOWHERE.to_owned(),
    });
    Ok(workflow)
}

#[test]
fn the_real_validator_has_nothing_to_say_about_this_file() -> Result<(), Box<dyn Error>> {
    let workflow = load()?;

    assert_eq!(
        workflow.format, CURRENT,
        "the graph has to be in the format this build writes; a file from another version is \
         not evidence about this schema"
    );

    let notes = check(&workflow);

    assert!(
        notes.is_empty(),
        "the harness graph is the heaviest workflow Loadout will ever be asked to hold, so it \
         has to be one the app would let a person save: no circle, no arrow into a step that is \
         not there, no step left unconnected, no two steps writing over each other. Not one \
         note, not even a warning. Got: {notes:?}"
    );
    Ok(())
}

#[test]
fn the_five_steps_are_the_contract() -> Result<(), Box<dyn Error>> {
    let workflow = load()?;

    let ids: Vec<&str> = workflow.steps.iter().map(id).collect();

    assert_eq!(
        ids, EXPECTED,
        "six stages of the harness map onto five steps, in this order. A sixth tile means a \
         stage quietly got one of its own — which is the answer to a question this task never \
         asked. A different order means the chain is a different chain."
    );
    Ok(())
}

#[test]
fn an_arrow_into_a_step_that_is_not_there_is_refused_by_name() -> Result<(), Box<dyn Error>> {
    let workflow = with_an_arrow_into_nowhere()?;

    let notes = check(&workflow);
    let problems = problems(&notes);

    assert_eq!(
        problems.len(),
        1,
        "one dangling arrow is one thing to fix. Zero of them means the validator this file \
         claims to pass accepts anything, and then passing it proves nothing. Got: {notes:?}"
    );
    assert!(
        problems[0].message.contains(NOWHERE),
        "the refusal has to name the step the arrow points at — a general 'this workflow is \
         invalid' would read the same for a validator that only parses JSON. It reads: {}",
        problems[0].message
    );
    Ok(())
}

#[test]
fn the_refusal_lands_at_save_time_and_nothing_is_written() -> Result<(), Box<dyn Error>> {
    let workflow = with_an_arrow_into_nowhere()?;
    let elsewhere = tempfile::tempdir()?;
    let path = elsewhere.path().join("ship-task.json");

    let Err(refusal) = file::save(&workflow, &path) else {
        return Err(format!(
            "invariant 12 puts the refusal at save time, and this save went through: {}",
            path.display()
        )
        .into());
    };

    assert!(
        refusal.to_string().contains(NOWHERE),
        "the sentence the user reads is the sentence of the first problem, word for word, so it \
         has to name the step too. It reads: {refusal}"
    );
    assert!(
        !path.exists(),
        "a refused save must not touch the disk: writing first and validating afterwards \
         destroys the previous file at exactly the moment the check was meant to defend it"
    );
    Ok(())
}
