//! Wczytanie i zapis: odmowa-w-przód, kopia `.bak` przed pierwszą migracją, deterministyczny tekst.
//!
//! Trzy własności, które trzeba mieć **naraz** [T3 §8.4]:
//!
//! - **odmowa zamiast zgadywania w przód.** Plik z `format` większym niż [`CURRENT`] nie jest
//!   wczytywany ani dotykany. Zgadnięcie kończy się tak: starszy build zapisuje plik z powrotem
//!   i kasuje pracę nowszego bez jednego komunikatu.
//! - **`.bak` przed pierwszą prawdziwą zmianą** — nie przy każdym wczytaniu. Kopia po nieudanym
//!   wczytaniu jest śmieciem obok pliku, którego nikt nie tknął.
//! - **każda migracja to czysta funkcja `Value -> Value`** z jednym plikiem złotym, więc
//!   `v1 -> v3` jest po prostu `v1 -> v2 -> v3`.

use std::fmt;
use std::io;
use std::path::Path;

use serde_json::Value;

use super::WorkflowFile;
use super::check::Note;

/// Wersja formatu, którą pisze ten build.
pub const CURRENT: u32 = 1;

/// `MIGRATIONS[i]` przenosi format `i` na `i + 1`.
///
/// Pusta tablica jest **poprawnym** stanem, nie brakiem: jedna wersja, dopóki nie ma drugiej
/// (AGENTS.md §4, niezmiennik 25). Pierwszy wpis przychodzi razem z pierwszą zmianą łamiącą
/// i przynosi ze sobą swój plik złoty. Migracja „na przyszłość" jest tu zakazana.
pub static MIGRATIONS: &[fn(Value) -> Value] = &[];

/// Dlaczego pliku nie da się wczytać.
///
/// Każdy wariant ma być osobnym zdaniem dla użytkownika, bo każdy naprawia się inaczej.
#[derive(Debug)]
pub enum LoadError {
    /// `format` większy niż [`CURRENT`]. Plik zostaje na dysku bez zmian i bez `.bak`.
    ///
    /// Zdanie wymagane przez AC-1 brzmi dokładnie:
    /// `This workflow was saved by a newer Loadout. Update Loadout to open it.`
    TooNew,
    /// Brak klucza `format`. Osobny wariant, bo potraktowanie tego jak wersji 0 jest cichym
    /// zgadywaniem — a plik bez wersji równie dobrze może być czymś, co workflowem nie jest.
    NoFormat,
    /// Pliku nie dało się przeczytać.
    Unreadable(io::Error),
    /// Bajty są, ale to nie jest ten format.
    Malformed(serde_json::Error),
}

impl fmt::Display for LoadError {
    fn fmt(&self, _formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("T-12: po jednym angielskim zdaniu na wariant, każde mówiące, co zrobić")
    }
}

impl std::error::Error for LoadError {}

/// Dlaczego pliku nie zapisano.
#[derive(Debug)]
pub enum SaveError {
    /// [`super::check`] znalazło problem. **Nic nie zostało zapisane** — poprzedni plik leży
    /// nietknięty. Implementacja, która zapisuje i dopiero potem waliduje, niszczy dane
    /// dokładnie w tym momencie, w którym miała ich bronić.
    Refused(Note),
    /// Sprawdzenia przeszły, ale zapis się nie udał.
    Unwritable(io::Error),
    /// Nie dało się zserializować — nie powinno się zdarzyć i dlatego ma własny wariant.
    Malformed(serde_json::Error),
}

impl fmt::Display for SaveError {
    fn fmt(&self, _formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("T-12: `Refused` mówi komunikatem pierwszego problemu, słowo w słowo")
    }
}

impl std::error::Error for SaveError {}

/// Wczytuje workflow: `format` czytany z surowego JSON-a **przed** deserializacją reszty,
/// migracje po kolei, `.bak` przed pierwszą z nich.
pub fn load(_path: &Path) -> Result<WorkflowFile, LoadError> {
    todo!("T-12: odmowa-w-przód, brak `format` jako osobna odmowa, `.bak` przed pierwszą migracją")
}

/// Zapisuje workflow — **jeżeli** [`super::check`] nie ma ani jednego problemu.
///
/// Kolejność jest całą treścią tej funkcji: sprawdź, dopiero potem dotknij dysku. Ostrzeżenie
/// nie blokuje niczego.
///
/// Tekst jest deterministyczny [T3 §8.2]: dwie spacje wcięcia, znak nowej linii na końcu,
/// `steps` w kolejności wstawiania i pozycje przyciągnięte do całkowitych wielokrotności
/// [`super::GRID`] — także wtedy, gdy przyciągnął je już frontend, bo plik można edytować ręcznie.
pub fn save(_workflow: &WorkflowFile, _path: &Path) -> Result<(), SaveError> {
    todo!("T-12: check() przed dyskiem, `to_string_pretty` + `\\n`, przyciąganie pozycji do siatki")
}
