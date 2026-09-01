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
use std::fs;
use std::io;
use std::path::Path;

use serde_json::Value;

use crate::durable_file::{DEFINITION_FILE_MODE, DurableFilePublisher, ModePolicy};

use super::WorkflowFile;
use super::check::{Level, Note, check};

/// Wersja formatu, którą pisze ten build.
pub const CURRENT: u32 = 1;

/// `MIGRATIONS[i]` przenosi format `i + 1` na `i + 2`, więc długość tablicy jest zawsze
/// `CURRENT - 1`.
///
/// 2026-08-16 — numerujemy od jedynki, bo formatu 0 Loadout nigdy nie napisał; tablica
/// zaczynająca się od migracji „0 na 1" miałaby na pozycji zerowej wpis, którego nikt nigdy
/// nie zawoła, a taki wpis jest kłamstwem o tym, ile wersji pliku naprawdę było.
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
    /// `format` mniejszy niż [`CURRENT`], ale [`MIGRATIONS`] nie ma czym go podnieść.
    ///
    /// 2026-08-16 — wariant istnieje wyłącznie po to, żeby ta ścieżka nie kończyła się
    /// indeksowaniem tablicy poza końcem, czyli paniką (AGENTS.md §4: żadnej paniki w silniku).
    /// Dziś prowadzi tu jedna wartość — `"format": 0` — której Loadout nigdy nie napisał;
    /// prawdziwym powodem tego wariantu jest dzień, w którym `CURRENT` urośnie, a ktoś otworzy
    /// plik z wersji, do której migracji już nie ma.
    TooOld,
    /// Pliku nie dało się przeczytać.
    Unreadable(io::Error),
    /// Bajty są, ale to nie jest ten format.
    Malformed(serde_json::Error),
}

impl fmt::Display for LoadError {
    /// Po jednym zdaniu na wariant, każde mówiące, **co zrobić**. Każdy z tych błędów naprawia
    /// się inaczej, więc jedno wspólne „nie udało się wczytać workflow" byłoby zdaniem, po
    /// którym użytkownik i tak musi otworzyć plik w edytorze.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // „unsupported format version 2" nie mówi nic: wersja pliku nie jest niczym, co
            // użytkownik może naprawić sam, więc zdanie musi wskazać jedyne wyjście.
            Self::TooNew => formatter.write_str(
                "This workflow was saved by a newer Loadout. Update Loadout to open it.",
            ),
            Self::TooOld => formatter
                .write_str("This workflow was saved in a format Loadout can no longer open."),
            Self::NoFormat => formatter.write_str(
                "This file does not say which version of Loadout wrote it, so Loadout will not \
                 open it.",
            ),
            Self::Unreadable(error) => {
                write!(formatter, "This workflow file could not be read: {error}.")
            }
            Self::Malformed(error) => write!(
                formatter,
                "This file is not a workflow Loadout can read: {error}."
            ),
        }
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
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Odmowa mówi zdaniem pierwszego problemu, słowo w słowo. To jest ten sam tekst,
            // który stoi przy kafelku, a dwa różne zdania o jednej rzeczy to dwa miejsca, w
            // których ta rzecz jest opisana — i jedno z nich zawsze jest nieaktualne.
            Self::Refused(note) => formatter.write_str(&note.message),
            Self::Unwritable(error) => {
                write!(formatter, "This workflow could not be saved: {error}.")
            }
            Self::Malformed(error) => write!(
                formatter,
                "This workflow could not be turned into a file: {error}."
            ),
        }
    }
}

impl std::error::Error for SaveError {}

/// Wczytuje workflow: `format` czytany z surowego JSON-a **przed** deserializacją reszty,
/// migracje po kolei, `.bak` przed pierwszą z nich.
pub fn load(path: &Path) -> Result<WorkflowFile, LoadError> {
    let text = fs::read_to_string(path).map_err(LoadError::Unreadable)?;
    load_text(path, &text, true)
}

/// Parsuje bajty odczytane przez descriptor-bound loader biblioteki. Pierwsza przyszła
/// migracja musi dostać równie descriptor-bound publikację `.bak`; do tego czasu snapshot
/// odmawia migracji zamiast wracać do ścieżkowego `copy` po bezpiecznym odczycie.
pub(crate) fn load_snapshot(path: &Path, bytes: &[u8]) -> Result<WorkflowFile, LoadError> {
    let text = std::str::from_utf8(bytes).map_err(|error| {
        LoadError::Unreadable(io::Error::new(io::ErrorKind::InvalidData, error))
    })?;
    load_text(path, text, false)
}

fn load_text(
    path: &Path,
    text: &str,
    path_migration_is_safe: bool,
) -> Result<WorkflowFile, LoadError> {
    let document: Value = serde_json::from_str(text).map_err(LoadError::Malformed)?;

    // Wersja czytana z SUROWEGO dokumentu, zanim spróbujemy zrozumieć resztę. Nowszy build
    // mógł zmienić kształt kroku tak, że ten build go nie wczyta — i wtedy użytkownik
    // zobaczyłby „to nie jest workflow" zamiast „zaktualizuj Loadouta" [T3 §8.4].
    let format = document
        .get("format")
        .and_then(Value::as_u64)
        .ok_or(LoadError::NoFormat)?;

    // Nigdy nie zgaduj w przód. Zgadnięcie kończy się dokładnie tak: starszy build wczytuje
    // plik, którego połowy nie rozumie, zapisuje go z powrotem i kasuje pracę nowszego bez
    // jednego komunikatu. Plik zostaje na dysku nietknięty i bez `.bak`.
    if format > u64::from(CURRENT) {
        return Err(LoadError::TooNew);
    }

    let document = if format < u64::from(CURRENT) && path_migration_is_safe {
        migrated(path, document, format)?
    } else if format < u64::from(CURRENT) {
        return Err(LoadError::Unreadable(io::Error::other(
            "this workflow needs a safe migration before it can be listed",
        )));
    } else {
        document
    };

    serde_json::from_value(document).map_err(LoadError::Malformed)
}

/// Przenosi dokument na [`CURRENT`], robiąc `.bak` **przed pierwszą prawdziwą zmianą**.
///
/// Kopia nie powstaje przy każdym wczytaniu: kopia obok pliku, którego nikt nie tknął, to
/// śmieć, który po tygodniu jest nieodróżnialny od kopii, która kogoś kiedyś uratowała.
fn migrated(path: &Path, mut document: Value, format: u64) -> Result<Value, LoadError> {
    // Planem dla pliku w wersji `f` jest cały ogon [`MIGRATIONS`] od `f - 1`, bo `MIGRATIONS[i]`
    // przenosi `i + 1` na `i + 2`. Pusty plan przy `format < CURRENT` znaczy, że tablica jest
    // krótsza, niż mówi `CURRENT` — czyli że tego pliku nie ma czym podnieść.
    let plan = usize::try_from(format)
        .ok()
        .and_then(|version| version.checked_sub(1))
        .and_then(|first| MIGRATIONS.get(first..))
        .filter(|plan| !plan.is_empty())
        .ok_or(LoadError::TooOld)?;

    // Kopia zapasowa jest częścią wczytania: jeśli nie da się jej zrobić, plik jest dla nas
    // niedostępny i nie tykamy go — migracja bez kopii to jedyna operacja w tym module, która
    // umie stracić dane bezpowrotnie.
    fs::copy(path, path.with_extension("json.bak")).map_err(LoadError::Unreadable)?;

    for migration in plan {
        document = migration(document);
    }

    // Wersję ustawiamy raz, na końcu, zamiast wymagać od każdej migracji, żeby pamiętała
    // o podniesieniu licznika. Zapis tej samej wartości drugi raz nic nie zmienia, więc całość
    // zostaje idempotentna (niezmiennik 25).
    if let Some(object) = document.as_object_mut() {
        object.insert("format".to_owned(), Value::from(CURRENT));
    }

    Ok(document)
}

/// Zapisuje workflow — **jeżeli** [`super::check`] nie ma ani jednego problemu.
///
/// Kolejność jest całą treścią tej funkcji: sprawdź, dopiero potem dotknij dysku. Ostrzeżenie
/// nie blokuje niczego.
///
/// Tekst jest deterministyczny [T3 §8.2]: dwie spacje wcięcia, znak nowej linii na końcu,
/// `steps` w kolejności wstawiania i pozycje przyciągnięte do całkowitych wielokrotności
/// [`super::GRID`] — także wtedy, gdy przyciągnął je już frontend, bo plik można edytować
/// ręcznie i wtedy żadnego frontendu nie było. Przyciąganie robi serializacja
/// [`super::Point`], więc nie da się go tu pominąć.
pub fn save(workflow: &WorkflowFile, path: &Path) -> Result<(), SaveError> {
    // Kolejność jest całą treścią tej funkcji: najpierw sprawdź, dopiero potem dotknij dysku.
    // Implementacja, która zapisuje i waliduje po zapisie, niszczy poprzednią wersję pliku
    // dokładnie w tym momencie, w którym sprawdzenie miało jej bronić. Ostrzeżenie nie blokuje
    // niczego — gdyby blokowało, jeden niepodłączony krok zamykałby plik na klucz.
    //
    // PUSTY PLIK JEST WYJĄTKIEM I TO NIE JEST ZŁAGODZENIE REGUŁY.
    //
    // `check` daje dla zera kroków `Level::Problem`, a jego własny test podaje powód: „there is
    // nothing to run, so Run may not be offered" — ta odmowa broni URUCHOMIENIA. Komentarz obok
    // niej mówi wprost: „pusty workflow to nie jest błąd danych, tylko stan, w którym użytkownik
    // jeszcze nic nie zrobił".
    //
    // Zmierzone 2026-08-17 na prawdziwym oknie: bez tego wyjątku „＋ Create" nie umiał utworzyć
    // NICZEGO. Nowy workflow z definicji nie ma kroków, więc zapis padał zawsze — a ekran połykał
    // odmowę (`void actions.create(…)`), więc przycisk wyglądał na zepsuty i nie mówił dlaczego.
    // Katalog `~/.loadout/workflows/` powstawał (robi go `create_dir_all` piętro wyżej) i zostawał
    // pusty. Nie dało się przejść do edytora, bo nie było czego otworzyć.
    //
    // Zero kroków nie może niczego uszkodzić: nie ma cyklu, nie ma wyspy, nie ma nadpisanego
    // pola. Niezmiennik 12 („odmowa pada przy zapisie, nie w trakcie biegu") mówi o plikach
    // NIEPOPRAWNYCH, a szkic bez kroków jest poprawny — po prostu niegotowy. Run dalej go nie
    // przyjmie, bo `check` woła się drugi raz przed biegiem (dowodzi tego T-15).
    if !workflow.steps.is_empty()
        && let Some(refusal) = check(workflow)
            .into_iter()
            .find(|note| note.level == Level::Problem)
    {
        return Err(SaveError::Refused(refusal));
    }

    let mut text = serde_json::to_string_pretty(workflow).map_err(SaveError::Malformed)?;
    // Znak nowej linii na końcu: bez niego każda zmiana ostatniego wiersza niesie w diffie
    // dodatkowe „\ No newline at end of file", a plik przestaje być zwykłym plikiem tekstowym.
    text.push('\n');

    let root = path.parent().ok_or_else(|| {
        SaveError::Unwritable(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a workflow path has no controlled parent",
        ))
    })?;
    DurableFilePublisher::new(root)
        .atomic_replace(
            path,
            text.as_bytes(),
            ModePolicy::PreserveExistingOr(DEFINITION_FILE_MODE),
        )
        .map_err(|error| SaveError::Unwritable(error.into_io()))
}
