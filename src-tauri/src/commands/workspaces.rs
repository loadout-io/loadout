//! Lista workspace'ów użytkownika: nazwa plus folder, w którym pracują agenci.
//!
//! **Ani jednego `use tauri::` i ani jednego `#[tauri::command]`** — jak w całym tym katalogu.
//! Skorupy stoją w `src/ipc.rs` i mają po dwie linie (niezmiennik 1).
//!
//! # Po co to istnieje
//!
//! 2026-08-18, decyzja Jakuba. Do tego dnia folder pracy wybierało się **systemowym oknem, przy
//! każdym uruchomieniu biegu**: `launchRun` pytał o katalog, jeśli żadna karta nie była otwarta.
//! Zdanie właściciela po zobaczeniu tego okna brzmiało „mega chujnia" i było trafne — wybór
//! folderu jest decyzją o PROJEKCIE, podejmowaną raz, a nie czynnością powtarzaną przed każdą
//! pracą. Workspace jest więc nazwanym zakresem, wybieranym w bocznym menu, i to on mówi,
//! gdzie pracują agenci.
//!
//! # Co workspace ZAKRESOWUJE, a czego nie
//!
//! Wyłącznie **folder pracy i żywą sesję** (strumień, karty biegów, stan biegu). Workflow,
//! agenci, umiejętności i pamięć zostają **globalne** w `~/.loadout` — to jest rozstrzygnięcie
//! z tego samego dnia i ma nazwany powód: umiejętności piszą do `~/.claude/skills`
//! i `~/.agents/skills` (`skills::DESTINATION_DIRS`), czyli do konfiguracji NARZĘDZI człowieka,
//! nie do jego projektu. Sekcja zakresowana per workspace i tak musiałaby się z tej reguły
//! wyłamać, a jedna sekcja łamiąca regułę kosztuje więcej niż brak reguły.
//!
//! Praktyczny skutek: agenta zapisanego raz widać w każdym workspace, a bieg zapisuje się pod
//! `<folder>/.loadout/runs/`, czyli w projekcie, którego dotyczy.
//!
//! # Czego tu ŚWIADOMIE nie ma, i to jest zgłoszenie
//!
//! Nie ma tu puli miejsc ani magazynu per folder. Oba mieszkają w `crate::workspace::Registry`
//! (546 linii, napisane 2026-08-16 i **do dziś bez ani jednego wołającego** — audyt nazwał to
//! wprost). Ten plik jest listą, którą widzi okno; tamten jest rejestrem, który powinien trzymać
//! `Store` na folder i JEDNĄ pulę miejsc na aplikację (`docs/ARCHITECTURE.md` §8, niezmiennik 11).
//! Wpięcie tamtego jest osobną zmianą: `AppState` trzyma dziś jeden `Store` otwarty na
//! `~/.loadout/loadout.db`, więc przeniesienie bazy pod folder projektu dotyka każdego biegu
//! i każdego kryterium, które go sądzi. Zgłoszone, nie zrobione po cichu.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Nazwa pliku z listą. Jeden plik, nie katalog: to jest lista kilku wierszy, a nie biblioteka.
const FILE: &str = "workspaces.json";

/// Ile workspace'ów przyjmujemy. Sufit istnieje, bo lista jedzie w całości do bocznego menu,
/// a menu, przez które trzeba przewijać, przestaje być przełącznikiem.
pub const MOST_WORKSPACES: usize = 24;

/// Jeden workspace na drucie.
///
/// `id` jest **ścieżką folderu** i to nie jest oszczędność: jeden folder = jeden workspace, więc
/// ścieżka jest naturalnym kluczem i nie da się zapisać dwóch workspace'ów o tym samym folderze
/// przez pomyłkę. Ten sam wybór robi `crate::workspace::WorkspaceId`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceWire {
    /// Ścieżka folderu — klucz i jedyna rzecz, którą ten wpis naprawdę niesie.
    pub id: String,
    /// Nazwa nadana przez człowieka. To ona stoi w przełączniku.
    pub name: String,
    /// Folder pracy. Równy `id`; osobne pole, bo okno nie ma prawa zakładać, że klucz jest
    /// ścieżką — dzień, w którym klucz przestanie nią być, ma zmienić jeden plik, nie sześć.
    pub folder: String,
}

/// Dlaczego nie dało się zapisać albo przeczytać listy.
///
/// Każdy wariant jest osobnym zdaniem dla człowieka, bo każdy naprawia się inaczej (niezmiennik 14:
/// zero żargonu — „os error 2" nie mówi, co zrobić).
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    /// Folder nie istnieje albo nie jest folderem.
    #[error(
        "The folder \"{folder}\" is not there, so Loadout did not add it. Pick a folder that \
         exists."
    )]
    NoSuchFolder {
        /// Ścieżka, którą podało okno.
        folder: String,
    },
    /// Ścieżka nie jest pełna.
    #[error(
        "Loadout needs the whole path to a folder, and \"{folder}\" is only part of one. Pick \
         the folder again."
    )]
    NotAWholePath {
        /// Ścieżka, którą podało okno.
        folder: String,
    },
    /// Nazwa jest pusta.
    #[error("Give this workspace a name first — that name is how you will pick it later.")]
    NoName,
    /// Lista jest pełna.
    #[error(
        "You already have {MOST_WORKSPACES} workspaces, which is as many as the switcher can \
         show. Remove one you no longer work in."
    )]
    TooMany,
    /// Pliku listy nie dało się przeczytać albo zapisać.
    #[error("Loadout could not save the list of workspaces: {0}")]
    Unwritable(#[source] io::Error),
    /// Plik listy jest uszkodzony.
    #[error(
        "The file that remembers your workspaces could not be read: {0}. Loadout left it alone \
         rather than overwrite it."
    )]
    Malformed(#[source] serde_json::Error),
}

/// Ścieżka pliku listy w bibliotece.
#[must_use]
pub fn list_path(home: &Path) -> PathBuf {
    home.join(FILE)
}

/// Workspace'y zapisane na dysku, w kolejności zapisu.
///
/// **Brak pliku to pusta lista, nie błąd.** Na świeżej maszynie tego pliku nie ma i to jest stan
/// normalny — dokładnie ta pomyłka (brakujący katalog traktowany jako awaria dysku) kończyła
/// każdy bieg zdaniem „No such file or directory (os error 2)".
///
/// Wpisy wskazujące na folder, którego już nie ma, **zostają na liście** i to jest wybór:
/// zniknięcie workspace'a z przełącznika, bo dysk zewnętrzny nie jest podłączony, wygląda jak
/// utrata pracy. Okno pokazuje je i mówi o nich; usuwa je człowiek.
pub fn list_workspaces_inner(home: &Path) -> Result<Vec<WorkspaceWire>, WorkspaceError> {
    let path = list_path(home);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(WorkspaceError::Unwritable(error)),
    };
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&text).map_err(WorkspaceError::Malformed)
}

/// Dokłada workspace albo zmienia nazwę istniejącego, i oddaje listę po zapisie.
///
/// Oddaje **całą listę**, nie sam zapisany wpis, i to jest ta sama decyzja, co przy notatkach:
/// okno ma jedno źródło prawdy o liście i nie składa jej sobie z odpowiedzi na pojedyncze
/// zapisy. Lista złożona po stronie okna rozjeżdża się przy pierwszym zapisie, który częściowo
/// się nie udał.
///
/// Klucz to folder, więc drugi zapis tego samego folderu **zmienia nazwę**, a nie dokłada
/// drugiego wiersza. Dwa workspace'y nad jednym folderem to dwa magazyny nad jedną bazą
/// (niezmiennik 2) — czyli zakleszczenie, nie „czasem wolniej".
pub fn save_workspace_inner(
    home: &Path,
    name: &str,
    folder: &str,
) -> Result<Vec<WorkspaceWire>, WorkspaceError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(WorkspaceError::NoName);
    }
    let path = PathBuf::from(folder);
    if !path.is_absolute() {
        return Err(WorkspaceError::NotAWholePath {
            folder: folder.to_owned(),
        });
    }
    // Sprawdzamy przy DOKŁADANIU, nie przy odczycie: folder, który zniknął po dodaniu, ma zostać
    // na liście (powód przy `list_workspaces_inner`), ale folder, którego nie ma w chwili
    // dodawania, jest pomyłką w oknie wyboru i mówimy o niej od razu.
    if !path.is_dir() {
        return Err(WorkspaceError::NoSuchFolder {
            folder: folder.to_owned(),
        });
    }

    let key = folder.to_owned();
    let mut all = list_workspaces_inner(home)?;
    if let Some(had) = all.iter_mut().find(|one| one.folder == key) {
        name.clone_into(&mut had.name);
    } else {
        if all.len() >= MOST_WORKSPACES {
            return Err(WorkspaceError::TooMany);
        }
        all.push(WorkspaceWire {
            id: key.clone(),
            name: name.to_owned(),
            folder: key,
        });
    }
    write(home, &all)?;
    Ok(all)
}

/// Zdejmuje workspace z listy i oddaje listę po zapisie.
///
/// **Folderu ani jego zawartości nie dotyka.** Usunięcie workspace'a jest zdjęciem zakresu
/// z przełącznika, a nie skasowaniem projektu — i to jest jedyna rzecz, którą ta komenda ma
/// prawo znaczyć. Katalog `.loadout/runs/` z historią biegów zostaje tam, gdzie był.
///
/// Nieznany identyfikator nie jest błędem: lista po tej operacji ma wyglądać tak samo,
/// niezależnie od tego, czy wpis był (idempotencja). Drugie kliknięcie „Remove" po odświeżeniu
/// listy w innym oknie nie ma prawa dać zdania o awarii.
pub fn delete_workspace_inner(home: &Path, id: &str) -> Result<Vec<WorkspaceWire>, WorkspaceError> {
    let mut all = list_workspaces_inner(home)?;
    all.retain(|one| one.id != id);
    write(home, &all)?;
    Ok(all)
}

/// Lista → plik, przez plik tymczasowy i `rename`.
///
/// `rename` w obrębie jednego katalogu jest atomowe: czytelnik widzi albo poprzednią listę
/// w całości, albo nową w całości, i nigdy zera bajtów w środku. Ta sama droga, którą zapisuje
/// się `run.json` (`commands::run::spill`) — bo ta sama klasa awarii kosztuje tu to samo:
/// przerwany zapis zabiera listę wszystkich projektów człowieka.
fn write(home: &Path, all: &[WorkspaceWire]) -> Result<(), WorkspaceError> {
    fs::create_dir_all(home).map_err(WorkspaceError::Unwritable)?;
    // Blad serializacji idzie w `Unwritable`, nie w `Malformed`: zdanie „nie dalo sie
    // PRZECZYTAC" wypowiedziane przy zapisie wysyla czlowieka szukac uszkodzonego pliku,
    // ktorego nie ma.
    let mut text = serde_json::to_string_pretty(all)
        .map_err(|error| WorkspaceError::Unwritable(io::Error::other(error)))?;
    // Znak nowej linii na końcu: bez niego każda zmiana ostatniego wiersza niesie w diffie
    // dodatkowe „\ No newline at end of file", a plik przestaje być zwykłym plikiem tekstowym.
    text.push('\n');
    let writing = list_path(home).with_extension("json.writing");
    fs::write(&writing, text).map_err(WorkspaceError::Unwritable)?;
    fs::rename(&writing, list_path(home)).map_err(WorkspaceError::Unwritable)
}
