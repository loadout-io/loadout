//! AC-7 dla T-12: `save()` odmawia przy problemie i **zostawia poprzedni plik nietknięty**.
//!
//! Słabą wersją jest `assert!(save(&cyclic, p).is_err())`. Przechodzi ją implementacja, która
//! zapisuje plik, potem waliduje i dopiero wtedy zwraca `Err` — czyli ta, która niszczy dane
//! dokładnie w tym momencie, w którym miała ich bronić. Rozróżniają dwie rzeczy i obie są
//! niżej: odczyt bajtów pliku po nieudanym zapisie i porównanie ich z bajtami sprzed, oraz
//! porównanie **całego** tekstu udanego zapisu z literałem, razem z końcowym `\n`.
//!
//! Plik leżący na dysku jest zapisany w jednej linii, czyli formatem, którego `save()` nigdy
//! nie produkuje. To celowe: gdyby był sformatowany tak samo, porównanie bajtów przechodziłoby
//! także dla implementacji, która plik nadpisała — przypadkiem tą samą treścią.
//!
//! Literał niżej jest jednocześnie asercją na cztery rzeczy z T3 §8.2: dwie spacje wcięcia,
//! znak nowej linii na końcu, `steps` w kolejności **wstawiania** (`s_plan`, `s_build`,
//! `s_lonely` — posortowane alfabetycznie wyglądałyby inaczej) i pozycje przyciągnięte do
//! całkowitych wielokrotności 24. `241.4 / 95.2` wchodzi, `240 / 96` wychodzi — przyciąganie
//! dzieje się w Ruście **także** wtedy, gdy zrobił je już frontend, bo plik można edytować
//! ręcznie i wtedy nie ma żadnego frontendu.

use std::error::Error;
use std::fs;

use serde_json::{Value, json};

use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::check::Level;
use loadout_lib::workflow::file::{SaveError, save};

/// Zdanie z uruchomienia w T3 §5.2.
const CIRCLE: &str = "These steps point back at each other in a circle. Work would never finish.";

/// Poprawny workflow, tak jak leży na dysku przed każdą próbą zapisu — jedna linia.
const ON_DISK: &str = "{\"format\":1,\"id\":\"wf_ship\",\"name\":\"Ship a feature\",\"steps\":[{\"kind\":\"agent\",\"id\":\"s_plan\",\"name\":\"Plan the work\",\"agent\":\"a_planner\",\"instructions\":\"Sketch the steps.\"}],\"links\":[]}";

/// Tekst, który `save()` ma napisać dla [`with_a_warning`] — cały, co do bajtu.
const GOLDEN: &str = r#"{
  "format": 1,
  "id": "wf_ship",
  "name": "Ship a feature",
  "steps": [
    {
      "kind": "agent",
      "id": "s_plan",
      "name": "Plan the work",
      "agent": "a_planner",
      "overrides": {},
      "copies": 1,
      "instructions": "Sketch the steps.",
      "skills": "all",
      "folder": {
        "use": "project"
      },
      "handover": "notes",
      "at": {
        "x": 0,
        "y": 0
      }
    },
    {
      "kind": "agent",
      "id": "s_build",
      "name": "Build",
      "agent": "a_forge",
      "overrides": {},
      "copies": 1,
      "instructions": "Do the work.",
      "skills": "all",
      "folder": {
        "use": "project"
      },
      "handover": "notes",
      "at": {
        "x": 240,
        "y": 96
      }
    },
    {
      "kind": "agent",
      "id": "s_lonely",
      "name": "Lonely step",
      "agent": "a_forge",
      "overrides": {},
      "copies": 1,
      "instructions": "Nobody wired this one up.",
      "skills": "all",
      "folder": {
        "use": "fresh-copy"
      },
      "handover": "notes",
      "at": {
        "x": 480,
        "y": 288
      }
    }
  ],
  "links": [
    {
      "from": "s_plan",
      "to": "s_build"
    }
  ]
}
"#;

fn parsed(file: Value) -> Result<WorkflowFile, Box<dyn Error>> {
    Ok(serde_json::from_value(file)?)
}

/// Trzy kroki domknięte w koło. Każdy pracuje we własnej kopii folderu, więc jedyną rzeczą,
/// jaką ten plik może zgłosić, jest koło.
fn cyclic() -> Result<WorkflowFile, Box<dyn Error>> {
    parsed(json!({
        "format": 1,
        "id": "wf_ship",
        "name": "Ship a feature",
        "steps": [
            { "kind": "agent", "id": "a", "name": "Plan the work", "agent": "a_planner",
              "instructions": "Sketch the steps.", "folder": { "use": "fresh-copy" } },
            { "kind": "agent", "id": "b", "name": "Build", "agent": "a_forge",
              "instructions": "Do the work.", "folder": { "use": "fresh-copy" } },
            { "kind": "agent", "id": "c", "name": "Ship", "agent": "a_forge",
              "instructions": "Ship it.", "folder": { "use": "fresh-copy" } }
        ],
        "links": [
            { "from": "a", "to": "b" },
            { "from": "b", "to": "c" },
            { "from": "c", "to": "a" }
        ]
    }))
}

/// Ten sam workflow z jednym krokiem, którego nikt nie podłączył — czyli z ostrzeżeniem
/// i bez ani jednego problemu. Pozycja `s_build` przychodzi nieprzyciągnięta.
fn with_a_warning() -> Result<WorkflowFile, Box<dyn Error>> {
    parsed(json!({
        "format": 1,
        "id": "wf_ship",
        "name": "Ship a feature",
        "steps": [
            { "kind": "agent", "id": "s_plan", "name": "Plan the work", "agent": "a_planner",
              "instructions": "Sketch the steps." },
            { "kind": "agent", "id": "s_build", "name": "Build", "agent": "a_forge",
              "instructions": "Do the work.", "at": { "x": 241.4, "y": 95.2 } },
            { "kind": "agent", "id": "s_lonely", "name": "Lonely step", "agent": "a_forge",
              "instructions": "Nobody wired this one up.",
              "folder": { "use": "fresh-copy" }, "at": { "x": 480, "y": 288 } }
        ],
        "links": [{ "from": "s_plan", "to": "s_build" }]
    }))
}

#[test]
fn a_problem_refuses_the_save_and_the_previous_file_does_not_move() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("ship-a-feature.json");
    fs::write(&path, ON_DISK)?;
    let before = fs::read(&path)?;

    let error = save(&cyclic()?, &path)
        .err()
        .ok_or("save() wrote a workflow whose steps wait for one another in a circle")?;

    // Opis bierzemy PRZED dopasowaniem: wzorzec przenosi `error`, a wtedy gałąź `else` nie ma
    // już czego wypisać. `SaveError` nie jest `Clone`, bo niesie `io::Error`.
    let description = format!("{error:?}");
    let SaveError::Refused(note) = error else {
        return Err(format!("a refusal is not an I/O failure; got: {description}").into());
    };
    assert_eq!(
        note.level,
        Level::Problem,
        "only a Problem may stop a save; if a Warning could, an unconnected step would lock the \
         file"
    );
    assert_eq!(
        note.message, CIRCLE,
        "the error carries the first problem's own sentence, because that is what the user is \
         shown and it is already plain English"
    );
    assert_eq!(
        fs::read(&path)?,
        before,
        "the workflow that was on disk is the user's last good version — writing first and \
         validating afterwards destroys it at exactly the moment the check was meant to save it"
    );
    Ok(())
}

#[test]
fn a_warning_still_saves_and_the_text_is_the_one_we_promised() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("ship-a-feature.json");
    fs::write(&path, ON_DISK)?;

    save(&with_a_warning()?, &path)?;

    assert_eq!(
        fs::read_to_string(&path)?,
        GOLDEN,
        "two spaces, a trailing newline, steps in the order they were inserted and positions \
         snapped to whole multiples of 24 — every one of them is what keeps a one-tile edit to \
         a three-line diff instead of a rewritten file"
    );
    Ok(())
}
