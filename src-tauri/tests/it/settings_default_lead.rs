//! Domyślny lider przeżywa zamknięcie okna, bo leży w PLIKU (niezmiennik 4).
//!
//! Słaba wersja tego zestawu zapisuje i od razu czyta przez te same dwie funkcje. Ona przechodzi
//! dla pary, która trzyma wybór w pamięci procesu i nie dotyka dysku ani razu — czyli dla
//! dokładnie tego stanu, który to zadanie kończy: wskazanie żyjące w oknie i ginące razem z nim.
//! Dlatego pierwszy test czyta BAJTY z dysku, a drugi udaje świeże uruchomienie, czytając
//! katalog, do którego nikt nic nie zapisał w tym wywołaniu.
//!
//! Brak pliku jest osobnym przypadkiem i nie jest ozdobą: na świeżej maszynie tego pliku nie ma,
//! a `Err` w tym miejscu zamieniłby pierwsze uruchomienie Loadouta w zdanie o awarii dysku.

use std::error::Error;

use loadout_lib::commands::settings::{
    SettingsWire, read_settings_inner, save_settings_inner, settings_path,
};
use tempfile::TempDir;

/// Identyfikator zapisanego agenta — uuid v7, taki, jaki wybija `new_id` (T4 §5.1).
const LEAD: &str = "0198a1f2-3b4c-7d5e-8f60-112233445566";

#[test]
fn a_fresh_library_has_nobody_leading_and_says_so_without_failing() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;

    let settings = read_settings_inner(home.path())?;

    assert_eq!(
        settings,
        SettingsWire::default(),
        "a library with no settings file yet has to answer with an empty choice. A refusal here \
         turns the first ever start of Loadout into a sentence about a broken disk, which is the \
         same mistake that ended every run with \"No such file or directory (os error 2)\""
    );
    assert!(
        !settings_path(home.path()).exists(),
        "reading must not create the file. A read that writes turns \"nobody chose yet\" into a \
         choice nobody made"
    );
    Ok(())
}

#[test]
fn the_chosen_lead_is_on_disk_and_a_later_read_finds_it() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;

    let saved = save_settings_inner(home.path(), LEAD)?;
    assert_eq!(
        saved.default_lead, LEAD,
        "saving has to answer with what the file now holds, so the window has one source of \
         truth for this choice instead of trusting the value it just sent"
    );

    // BAJTY, nie druga podróż tą samą funkcją. Para, która trzyma wybór w pamięci procesu,
    // przechodzi każde porównanie zapisu z odczytem i ginie razem z oknem.
    let text = std::fs::read_to_string(settings_path(home.path()))?;
    assert!(
        text.contains(LEAD),
        "the file that survives the window has to carry the chosen id. It reads:\n{text}"
    );
    assert!(
        text.contains("defaultLead"),
        "the key on disk is the one the window speaks, character for character. It reads:\n{text}"
    );

    // Świeży odczyt tego samego katalogu — to jest wszystko, co ma następne otwarcie okna.
    let read_again = read_settings_inner(home.path())?;
    assert_eq!(
        read_again.default_lead, LEAD,
        "a window opened later reads the library and nothing else, so the choice has to come \
         back out of the file. Without this the person picks the same lead before every run"
    );
    Ok(())
}
