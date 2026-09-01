//! Co Loadout robi domyślnie, kiedy człowiek nie powiedział inaczej. Dziś jedno pole: kto
//! prowadzi rozmowę.
//!
//! **Ani jednego `use tauri::` i ani jednego `#[tauri::command]`** — jak w całym tym katalogu.
//! Skorupy stoją w `src/ipc.rs` i mają po dwie linie (niezmiennik 1).
//!
//! # Po co to istnieje
//!
//! 2026-08-29. Do tego dnia wybór lidera żył WYŁĄCZNIE w oknie (`src/sections/run/lead.ts`):
//! zaczynał się pusty przy każdym uruchomieniu i człowiek wskazywał tę samą osobę przed każdą
//! pracą. To jest ta sama pomyłka, którą 2026-08-18 naprawił workspace — decyzja podejmowana
//! RAZ nie ma prawa być czynnością powtarzaną przed każdym biegiem.
//!
//! # Dlaczego plik, a nie kolumna w `loadout.db`
//!
//! Niezmiennik 4: pliki są prawdą, `loadout.db` jest indeksem i musi dać się skasować bez
//! utraty czegokolwiek. Wybór, którego nie da się odtworzyć z `~/.loadout`, byłby pierwszym
//! polem łamiącym tę regułę. Kształt i droga zapisu są jeden do jednego te, co w
//! [`super::workspaces`], i to jest celowe: dwa pliki listy w jednej bibliotece, zapisywane na
//! dwa różne sposoby, to dwie różne klasy awarii przy przerwanym zapisie.
//!
//! # Czego tu ŚWIADOMIE nie ma
//!
//! Vendora, modelu ani dialu bezpieczeństwa. „Kim jest lider" ma dokładnie jedno źródło —
//! zapisaną definicję agenta (`library::agents`) — a kopia któregokolwiek z tych pól trzymana
//! obok wskazania jest pierwszą rzeczą, która się rozjedzie (niezmiennik 13). Tutaj stoi
//! WSKAZANIE, czyli identyfikator; kto to jest, odpowiada biblioteka.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Nazwa pliku. Jeden plik, nie katalog: to jest garść wyborów, a nie biblioteka.
const FILE: &str = "settings.json";

/// Co Loadout robi domyślnie, na drucie.
///
/// Jedno pole, bo jeden wybór — i tak ma zostać, dopóki nie zajdzie potrzeba drugiego. Struktura
/// „na przyszłość" jest tu tym samym długiem, co migracja schematu „na przyszłość" (AGENTS.md §4).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsWire {
    /// Identyfikator zapisanego agenta, który prowadzi rozmowę, dopóki człowiek nie wskaże
    /// innego. Pusty napis znaczy „nikt jeszcze nie wybierał" i jest stanem normalnym.
    ///
    /// `serde(default)` z premedytacją: plik zapisany przez wcześniejszą wersję Loadouta nie ma
    /// tego klucza, a brakujące pole nie ma prawa wywalić odczytu całej biblioteki (niezmiennik 5).
    #[serde(default)]
    pub default_lead: String,
}

/// Dlaczego nie dało się przeczytać albo zapisać tego pliku.
///
/// Dwa warianty, dwa różne zdania dla człowieka, bo naprawia się je inaczej (niezmiennik 14:
/// „os error 2" nie mówi, co zrobić).
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    /// Pliku nie dało się przeczytać albo zapisać.
    #[error("Loadout could not save what it does by default: {0}")]
    Unwritable(#[source] io::Error),
    /// Plik jest uszkodzony.
    #[error(
        "The file that remembers what Loadout does by default could not be read: {0}. Loadout \
         left it alone rather than overwrite it."
    )]
    Malformed(#[source] serde_json::Error),
}

/// Ścieżka pliku w bibliotece.
#[must_use]
pub fn settings_path(home: &Path) -> PathBuf {
    home.join(FILE)
}

/// Co stoi w pliku.
///
/// **Brak pliku to pusty wybór, nie błąd.** Na świeżej maszynie tego pliku nie ma i to jest stan
/// normalny — dokładnie ta pomyłka (brakujący plik traktowany jako awaria dysku) kończyła każdy
/// bieg zdaniem „No such file or directory (os error 2)".
///
/// Uszkodzony plik jest za to ODMOWĄ, a nie cichym powrotem do pustki: pusty wybór po
/// uszkodzonym pliku wygląda na ekranie dokładnie tak, jakby człowiek nigdy nikogo nie wskazał,
/// i pierwszy zapis nadpisałby to, czego nie dało się przeczytać.
pub fn read_settings_inner(home: &Path) -> Result<SettingsWire, SettingsError> {
    let text = match fs::read_to_string(settings_path(home)) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(SettingsWire::default());
        }
        Err(error) => return Err(SettingsError::Unwritable(error)),
    };
    if text.trim().is_empty() {
        return Ok(SettingsWire::default());
    }
    serde_json::from_str(&text).map_err(SettingsError::Malformed)
}

/// Zapisuje domyślnego lidera i oddaje to, co ma teraz plik.
///
/// Oddaje CAŁY wpis, nie samo `()`, i to jest ta sama decyzja, co przy workspace'ach: okno ma
/// jedno źródło prawdy o tym wyborze i nie składa go sobie z argumentu, który wysłało. Stan
/// zbudowany po stronie okna rozjeżdża się przy pierwszym zapisie, który częściowo się nie udał.
///
/// Identyfikator jest przycinany, a puste wskazanie jest wartością, nie błędem: „nikt nie
/// prowadzi" jest wyborem, który człowiek ma prawo podjąć, a odmowa zostawiłaby go bez drogi
/// powrotnej z raz wskazanego agenta.
pub fn save_settings_inner(home: &Path, default_lead: &str) -> Result<SettingsWire, SettingsError> {
    let settings = SettingsWire {
        default_lead: default_lead.trim().to_owned(),
    };
    write(home, &settings)?;
    Ok(settings)
}

/// Wybór → plik, przez plik tymczasowy i `rename`.
///
/// `rename` w obrębie jednego katalogu jest atomowe: czytelnik widzi albo poprzedni wybór
/// w całości, albo nowy w całości, i nigdy zera bajtów w środku. Ta sama droga, którą zapisuje
/// się listę workspace'ów i `run.json` (`commands::run::spill`).
fn write(home: &Path, settings: &SettingsWire) -> Result<(), SettingsError> {
    fs::create_dir_all(home).map_err(SettingsError::Unwritable)?;
    // Blad serializacji idzie w `Unwritable`, nie w `Malformed`: zdanie „nie dalo sie
    // PRZECZYTAC" wypowiedziane przy zapisie wysyla czlowieka szukac uszkodzonego pliku,
    // ktorego nie ma.
    let mut text = serde_json::to_string_pretty(settings)
        .map_err(|error| SettingsError::Unwritable(io::Error::other(error)))?;
    // Znak nowej linii na koncu: bez niego kazda zmiana ostatniego wiersza niesie w diffie
    // dodatkowe „\ No newline at end of file", a plik przestaje byc zwyklym plikiem tekstowym.
    text.push('\n');
    let writing = settings_path(home).with_extension("json.writing");
    fs::write(&writing, text).map_err(SettingsError::Unwritable)?;
    fs::rename(&writing, settings_path(home)).map_err(SettingsError::Unwritable)
}
