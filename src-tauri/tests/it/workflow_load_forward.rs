//! AC-1 dla T-12: plik z przyszłości nie jest wczytywany **ani dotykany**.
//!
//! Cicha porażka, przed którą stoi to kryterium, wygląda tak: starszy build wczytuje plik
//! zapisany przez nowszy, nie rozumie połowy pól, zapisuje go z powrotem i **kasuje pracę
//! nowszego builda bez jednego komunikatu**. Dlatego odmowa jest w przód, a nie zgadywanie
//! [T3 §8.4].
//!
//! Słaba wersja tego kryterium to `assert!(load(p).is_err())` dla wersji 2. Przechodzi ją
//! `load()`, które zwraca `Err` **zawsze** — a wtedy nie da się otworzyć żadnego workflow
//! i dowie się o tym dopiero użytkownik. Dlatego para wersji jest w jednym pliku: wersja 1
//! musi wczytać się na `Ok` z poprawną liczbą kroków, i dopiero to nadaje odmowie wersji 2
//! jakiekolwiek znaczenie.
//!
//! Druga asercja jest o dysku, nie o wartości zwrotnej: bajty pliku przed i po nieudanym
//! wczytaniu muszą być identyczne, a `.json.bak` ma **nie** powstać. Kopia zapasowa powstaje
//! wyłącznie przed pierwszą prawdziwą migracją; kopia po nieudanym wczytaniu to śmieć obok
//! pliku, którego nikt nie tknął.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use loadout_lib::workflow::file::{LoadError, load};

/// Zdanie, które użytkownik ma zobaczyć zamiast pliku. Mówi, co zrobić — „unsupported format
/// version 2" nie mówi nic, a wersja pliku nie jest niczym, co użytkownik może naprawić sam.
const TOO_NEW: &str = "This workflow was saved by a newer Loadout. Update Loadout to open it.";

/// Workflow zapisany przez build, którego jeszcze nie ma: `format: 2` i pole, którego ta
/// wersja nie zna.
const FROM_THE_FUTURE: &str = r#"{
  "format": 2,
  "id": "wf_ship",
  "name": "Ship a feature",
  "steps": [
    {
      "kind": "agent",
      "id": "s_plan",
      "name": "Plan the work",
      "agent": "a_planner",
      "instructions": "Sketch the steps.",
      "budget": { "dollars": 5 }
    }
  ],
  "links": []
}
"#;

/// Ten sam workflow w wersji, którą ten build pisze sam — dwa kroki i strzałka między nimi.
const OF_THIS_VERSION: &str = r#"{
  "format": 1,
  "id": "wf_ship",
  "name": "Ship a feature",
  "steps": [
    {
      "kind": "agent",
      "id": "s_plan",
      "name": "Plan the work",
      "agent": "a_planner",
      "instructions": "Sketch the steps."
    },
    {
      "kind": "agent",
      "id": "s_build",
      "name": "Build",
      "agent": "a_forge",
      "instructions": "Do the work."
    }
  ],
  "links": [{ "from": "s_plan", "to": "s_build" }]
}
"#;

/// Plik, który wygląda jak workflow, ale nie mówi, którą jest wersją.
const WITHOUT_A_VERSION: &str = r#"{
  "id": "wf_ship",
  "name": "Ship a feature",
  "steps": [],
  "links": []
}
"#;

/// Zapisuje `source` jako `<tmp>/<name>.json` i zwraca ścieżkę.
fn written(dir: &Path, name: &str, source: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(format!("{name}.json"));
    fs::write(&path, source)?;
    Ok(path)
}

#[test]
fn a_file_from_a_newer_loadout_is_refused_by_name() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = written(dir.path(), "future", FROM_THE_FUTURE)?;

    let error = load(&path)
        .err()
        .ok_or("load() accepted a workflow written by a newer Loadout")?;

    assert!(
        matches!(error, LoadError::TooNew),
        "a file from the future is its own refusal, not a parse error; got: {error:?}"
    );
    assert_eq!(
        error.to_string(),
        TOO_NEW,
        "this sentence is the whole of what the user gets to see, and it has to say what to do"
    );
    Ok(())
}

#[test]
fn refusing_a_file_from_the_future_leaves_the_disk_alone() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = written(dir.path(), "future", FROM_THE_FUTURE)?;
    let before = fs::read(&path)?;

    let refusal = load(&path);
    assert!(
        refusal.is_err(),
        "this test only means something while the load is the one that failed"
    );

    assert_eq!(
        fs::read(&path)?,
        before,
        "the file a newer Loadout wrote may not change by one byte when an older one fails to \
         open it — that byte is the user's work"
    );
    assert!(
        !path.with_extension("json.bak").exists(),
        "a backup belongs before the first real migration, not next to a file nobody touched"
    );
    Ok(())
}

#[test]
fn a_file_of_this_version_loads_with_all_of_its_steps() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = written(dir.path(), "current", OF_THIS_VERSION)?;

    let workflow = load(&path).map_err(|error| format!("{error:?}"))?;

    assert_eq!(
        workflow.steps.len(),
        2,
        "a load() that refuses everything also passes 'version 2 is refused' — this is the \
         assertion that tells the two apart"
    );
    assert_eq!(
        workflow.links.len(),
        1,
        "the one arrow in the file is a step's dependency and \
         losing it silently reorders the run"
    );
    Ok(())
}

#[test]
fn a_file_without_a_version_is_its_own_refusal() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = written(dir.path(), "versionless", WITHOUT_A_VERSION)?;

    let error = load(&path)
        .err()
        .ok_or("load() accepted a file that never said which version it is")?;

    assert!(
        !matches!(error, LoadError::TooNew),
        "a missing version is not a version from the future; treating it as one sends the user \
         to update Loadout over a file that needs a different fix"
    );
    let message = error.to_string();
    assert_ne!(
        message, TOO_NEW,
        "two different mistakes may not arrive as the same sentence"
    );
    assert!(
        message.contains(' ') && message.ends_with('.'),
        "this goes straight onto the screen, so it is an English sentence and not a code; it \
         reads: {message}"
    );
    Ok(())
}
