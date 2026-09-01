//! Domyślne wybory przeżywają zamknięcie okna, bo leżą w PLIKU (niezmiennik 4).
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

/// Sufit wydatku, który człowiek wpisuje w Settings. Nie liczba wysyłkowa: taką łatwo dostać
/// przypadkiem od implementacji, która argumentu nie czyta.
const CEILING: f64 = 40.0;

/// Najmniejsza kwota, którą wolno postawić — ta sama podłoga, co po stronie okna.
const SMALLEST: f64 = 0.01;

/// Tryb bocznego menu, który człowiek wybrał. `true`, czyli NIE domyślny: wartość domyślna
/// przechodziłaby także dla zapisu, który tego argumentu nie czyta.
const FOLDED: bool = true;

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
    // 2026-08-29 (T-208) — I ODDAJE PRAWDZIWY SUFIT, NIE ZERO. `f64::default()` to zero, czyli
    // bieg, który nie ma prawa ruszyć; brak liczby w ogóle to bieg bez ograniczenia, na który
    // wpada się przez zapomnienie. Zmierzone koszty prawdziwych biegów właściciela z fazy 8:
    // od $11 do $67,78, a jeden bieg przerwał limit konta, nie aplikacja.
    assert!(
        settings.default_budget_usd >= SMALLEST,
        "a fresh library answers with {} dollars a run may spend. Zero is a run that may never \
         start, and anything below a cent is not a ceiling — either way the first ever run of \
         Loadout is one nobody capped",
        settings.default_budget_usd
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

    let saved = save_settings_inner(home.path(), LEAD, CEILING, FOLDED)?;
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
    // 2026-08-29 (T-208) — I DRUGI WYBÓR TĄ SAMĄ DROGĄ. Jeden plik, jeden zapis: kwota
    // zapisywana osobnym sposobem byłaby drugą klasą awarii przy przerwanym zapisie, dokładnie
    // tak, jak mówi nagłówek `commands::settings`.
    assert!(
        text.contains("defaultBudgetUsd") && text.contains("40"),
        "the ceiling a person set has to reach the same file under the key the window speaks. \
         Without it every window opens runs under a number nobody chose. It reads:\n{text}"
    );

    // 2026-08-31 — I TRZECI WYBÓR TĄ SAMĄ DROGĄ. Tryb bocznego menu jest wyborem człowieka,
    // a nie stanem okna: bez tego klucza w pliku menu wraca rozwinięte przy każdym uruchomieniu
    // i człowiek zwija je codziennie od nowa.
    assert!(
        text.contains("navCollapsed") && text.contains("true"),
        "the side nav mode a person chose has to reach the same file under the key the window \
         speaks. Without it the choice dies with the window. It reads:\n{text}"
    );

    // Świeży odczyt tego samego katalogu — to jest wszystko, co ma następne otwarcie okna.
    let read_again = read_settings_inner(home.path())?;
    assert_eq!(
        read_again.default_lead, LEAD,
        "a window opened later reads the library and nothing else, so the choice has to come \
         back out of the file. Without this the person picks the same lead before every run"
    );
    assert_eq!(
        read_again.nav_collapsed, FOLDED,
        "a window opened later reads the library and nothing else, so the side nav has to open \
         in the mode the person left it in"
    );
    assert_eq!(
        read_again, saved,
        "the whole entry has to come back, not the half of it somebody happened to look at. A \
         read that loses the ceiling starts every later run under a number the person never set"
    );
    Ok(())
}

/// Kwota, która nie jest sufitem, jest ODMOWĄ — nie liczbą poprawioną po cichu.
///
/// 2026-08-29 (T-208). Zero i kwota poniżej centa to bieg, który nie ma prawa ruszyć, a kwota
/// nieskończona to sufit, którego nie da się przekroczyć — czyli brak sufitu pod nazwą sufitu.
/// Podstawienie po cichu liczby, która ma sens, wygląda w Settings dokładnie tak, jakby człowiek
/// ją tak wpisał, a to jest ten jeden wybór, przy którym pomyłka kosztuje pieniądze.
#[test]
fn an_amount_that_is_not_a_ceiling_is_refused_and_leaves_the_file_alone()
-> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    save_settings_inner(home.path(), LEAD, CEILING, FOLDED)?;

    for amount in [0.0, -1.0, f64::INFINITY, f64::NAN] {
        let refused = save_settings_inner(home.path(), LEAD, amount, !FOLDED);
        assert!(
            refused.is_err(),
            "{amount} was accepted as how much a run may spend. A run allowed to spend nothing \
             never starts, and a run allowed to spend everything is the uncapped run this task \
             removes — under a name that says otherwise"
        );
    }

    assert_eq!(
        read_settings_inner(home.path())?,
        SettingsWire {
            default_lead: LEAD.to_owned(),
            default_budget_usd: CEILING,
            nav_collapsed: FOLDED,
        },
        "a refused amount overwrote what was already in the file, so one mistyped key leaves the \
         person with a default they never chose"
    );
    Ok(())
}
