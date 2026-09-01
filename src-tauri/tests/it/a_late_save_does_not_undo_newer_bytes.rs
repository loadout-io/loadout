//! Spóźniony zapis odbija się od pliku, a bajty, które leżą tam teraz, zostają co do bajtu.
//!
//! `workflow_save_refuses` woła `save()` z rewizją, która **zgadza się** z dyskiem — to celowe,
//! bo tamten plik sądzi odmowę `check`, więc niezgodność rewizji tylko zasłaniałaby jego własne
//! asercje. Skutek uboczny: obie jego ścieżki jadą przez szczęśliwe ramię `publish_definition`
//! i żadna z nich nie zauważyłaby ramienia, które przyjmuje NIEAKTUALNĄ rewizję. Zmierzone
//! 2026-08-28: po podmianie warunku tego ramienia na tautologię cała rustowa suita jest zielona.
//!
//! Dlatego ten plik. Słabą wersją jest `assert!(save(…).is_err())`: przechodzi ją implementacja,
//! która najpierw nadpisuje plik, a potem zwraca `Err` — czyli ta, która kasuje cudzą pracę
//! dokładnie w chwili, w której miała jej bronić. Odmowa jest więc dopiero pierwszą połową
//! asercji, a drugą jest odczyt pliku i porównanie go z bajtami, które ktoś tam zapisał
//! po nas. Drugi test stoi po przeciwnej stronie, żeby ta brama nie mogła być zawsze-odmową:
//! z AKTUALNĄ rewizją zapis wchodzi i bajty na dysku naprawdę się zmieniają.

use std::error::Error;
use std::fs;

use serde_json::{Value, json};

use loadout_lib::durable_file::revision_of;
use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::file::{SaveError, save};

/// Bajty, które ktoś inny zapisał już PO tym, jak nasze okno przeczytało plik. Jedna linia,
/// czyli format, którego `save()` nigdy nie produkuje — gdyby były sformatowane tak samo,
/// porównanie bajtów przechodziłoby także dla implementacji, która plik nadpisała.
const NEWER_ON_DISK: &str = "{\"format\":1,\"id\":\"wf_ship\",\"name\":\"Somebody else got here first\",\"steps\":[],\"links\":[]}";

fn parsed(file: Value) -> Result<WorkflowFile, Box<dyn Error>> {
    Ok(serde_json::from_value(file)?)
}

/// Workflow, który leży na dysku, zanim ktokolwiek zacznie pisać.
fn opened() -> Result<WorkflowFile, Box<dyn Error>> {
    parsed(json!({
        "format": 1,
        "id": "wf_ship",
        "name": "Ship a feature",
        "steps": [
            { "kind": "agent", "id": "s_plan", "name": "Plan the work", "agent": "a_planner",
              "instructions": "Sketch the steps." }
        ],
        "links": []
    }))
}

/// Ta sama praca po jednej edycji w oknie — to ona przychodzi spóźniona.
fn edited() -> Result<WorkflowFile, Box<dyn Error>> {
    parsed(json!({
        "format": 1,
        "id": "wf_ship",
        "name": "Ship a feature, renamed in a window that had been open a while",
        "steps": [
            { "kind": "agent", "id": "s_plan", "name": "Plan the work", "agent": "a_planner",
              "instructions": "Sketch the steps, but differently." }
        ],
        "links": []
    }))
}

#[test]
fn a_stale_revision_is_refused_and_the_newer_bytes_stay() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("ship-a-feature.json");

    // Rewizja, którą okno przeczytało: ta, którą oddał jego własny zapis.
    let read_by_the_window = save(&opened()?, &path, None)?;

    // Ktoś inny zapisuje ten plik w międzyczasie — poza oknem, którego rewizję trzymamy wyżej.
    fs::write(&path, NEWER_ON_DISK)?;

    let error = save(&edited()?, &path, Some(&read_by_the_window))
        .err()
        .ok_or("save() accepted a revision that is no longer what the file says")?;

    let description = format!("{error:?}");
    assert!(
        matches!(error, SaveError::Changed),
        "a file that is not the one that was read has its own sentence for the person, because \
         it is the only refusal after which they have something to do; got: {description}"
    );
    assert_eq!(
        fs::read(&path)?,
        NEWER_ON_DISK.as_bytes(),
        "the bytes on disk are somebody's newer work, and newer work never disappears without a \
         word — an implementation that writes first and compares afterwards destroys it here"
    );
    Ok(())
}

#[test]
fn the_current_revision_still_writes_the_file() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("ship-a-feature.json");

    save(&opened()?, &path, None)?;
    fs::write(&path, NEWER_ON_DISK)?;

    // Okno przeczytało plik PONOWNIE, więc niesie rewizję tego, co leży tam teraz.
    let written = save(
        &edited()?,
        &path,
        Some(&revision_of(NEWER_ON_DISK.as_bytes())),
    )?;

    let after = fs::read(&path)?;
    assert_ne!(
        after,
        NEWER_ON_DISK.as_bytes(),
        "a save carrying the revision the file actually has must go through; a door that never \
         opens keeps the bytes safe and makes the editor useless"
    );
    assert_eq!(
        revision_of(&after),
        written,
        "the revision save() hands back describes the bytes that just landed, because that is \
         what the window carries into its next save"
    );
    Ok(())
}
