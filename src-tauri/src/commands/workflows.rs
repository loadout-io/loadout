//! Komendy plików workflow: wczytaj, zapisz, sprawdź.
//!
//! **Ani jednego `use tauri::`** — jak w całym tym katalogu (`docs/ARCHITECTURE.md` §3).
//!
//! Format pliku, migracje w przód i odmowa przy zapisie mieszkają w `workflow::file` (T-12),
//! a wszystko, co da się o pliku powiedzieć bez uruchamiania go, w `workflow::check`. Te trzy
//! funkcje nie powtarzają ani jednej z tych reguł: składają ścieżkę i oddają to, co powiedział
//! walidator — **co do uwagi**.
//!
//! Ostatnie zdanie jest całym powodem, dla którego `check_workflow_inner` w ogóle istnieje jako
//! osobna komenda. Komenda, która gubi uwagi walidatora, jest gorsza niż jej brak: front
//! narysuje wtedy zielono plik, który Rust odrzuci przy Starcie, i człowiek dowie się o tym
//! dopiero od biegu, który nie ruszył.

use std::io;
use std::path::{Path, PathBuf};

use crate::workflow::WorkflowFile;
use crate::workflow::check::Note;
use crate::workflow::file::{LoadError, SaveError};

/// Katalog workflow wewnątrz biblioteki: `~/.loadout/workflows/` (`docs/ARCHITECTURE.md` §8).
const WORKFLOWS_DIR: &str = "workflows";

/// Jedna pozycja listy: nazwa pliku i to, co w nim leży.
///
/// Lustro `WorkflowEntry` z `src/sections/workflows/list/store.ts`, pole w pole. `path` to
/// **sama nazwa pliku**, nigdy pełna ścieżka: katalog rozwiązuje ta warstwa, a front, który
/// doklejałby go sam, byłby drugim miejscem, w którym mieszka odpowiedź na pytanie „gdzie to
/// leży" [T3 §8.3].
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WorkflowEntry {
    /// `ship-a-feature.json` — bez katalogu i bez `~`.
    pub path: String,
    pub workflow: WorkflowFile,
}

/// `home/workflows/<file_name>`, albo odmowa, kiedy `file_name` nie jest nazwą pliku.
///
/// Nazwa przychodzi z okna, więc jest wejściem, któremu nie ufamy (T3 §5.2). `Path::join`
/// z `../../.ssh/config` wychodzi poza bibliotekę bez jednego ostrzeżenia, a `join("/etc/x")`
/// **odrzuca cały prefiks** i zwraca `/etc/x` — czyli komenda „zapisz workflow" pisałaby
/// dokładnie tam, gdzie każe jej webview. Zapora jest tutaj, bo to jedyna warstwa, która wie,
/// że ten napis ma być nazwą, a nie ścieżką.
fn in_library(home: &Path, file_name: &str) -> Result<PathBuf, io::Error> {
    // `Path::file_name` zamiast ręcznego szukania separatorów: reguła „czym jest nazwa pliku"
    // należy do systemu plików, a nie do naszej listy zakazanych znaków. Nazwa, która nie jest
    // swoją własną nazwą pliku, niesie katalog — i o to właśnie pytamy.
    if Path::new(file_name)
        .file_name()
        .is_none_or(|name| name != file_name)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{file_name} is not the name of a file in the workflow folder"),
        ));
    }
    Ok(home.join(WORKFLOWS_DIR).join(file_name))
}

/// Zapisuje workflow pod `file_name` w bibliotece i oddaje ścieżkę, pod którą wylądował.
///
/// `file_name` to **sama nazwa pliku** (`ship-a-feature.json`), nigdy pełna ścieżka: katalog
/// rozwiązuje ta warstwa, po stronie Rusta. Front, który dokleja katalog sam, jest drugim
/// miejscem, w którym mieszka odpowiedź na pytanie „gdzie to leży" [T3 §8.3].
pub fn save_workflow_inner(
    home: &Path,
    file_name: &str,
    workflow: &WorkflowFile,
) -> Result<PathBuf, SaveError> {
    let path = in_library(home, file_name).map_err(SaveError::Unwritable)?;
    // Katalog powstaje tutaj, bo `file::save` (T-12) pisze plik i nie zakłada katalogów —
    // celowo, bo tam jego brak jest awarią, a tutaj jest normalnym stanem biblioteki, w której
    // nikt jeszcze niczego nie zapisał.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(SaveError::Unwritable)?;
    }

    // Odmowa walidatora, kolejność „sprawdź, potem dotknij dysku" i deterministyczny tekst
    // mieszkają w `file::save`. Ta funkcja nie powtarza ani jednej z tych decyzji.
    crate::workflow::file::save(workflow, &path)?;
    Ok(path)
}

/// Wczytuje workflow spod `file_name` w bibliotece.
pub fn load_workflow_inner(home: &Path, file_name: &str) -> Result<WorkflowFile, LoadError> {
    let path = in_library(home, file_name).map_err(LoadError::Unreadable)?;
    crate::workflow::file::load(&path)
}

/// Wszystko, co leży w katalogu workflow, każdy plik ze swoją nazwą.
///
/// Plik, którego nie da się wczytać, **przewraca listę** i robi to z podaniem powodu:
/// `LoadError::TooNew` znaczy „zaktualizuj Loadouta", a nie „ten plik zniknął" [T3 §8.4].
/// Lista, która po cichu pomija plik z przyszłej wersji, jest listą, na której użytkownik
/// widzi, że jego workflow przepadł — i tworzy go od nowa obok tego, który leży na dysku.
pub fn list_workflows_inner(home: &Path) -> Result<Vec<WorkflowEntry>, LoadError> {
    let dir = home.join(WORKFLOWS_DIR);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        // Biblioteka bez ani jednego zapisanego workflow ma zero pozycji, a nie błąd.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(LoadError::Unreadable(error)),
    };

    // Wyłącznie `.json`, więc `.json.bak` odłożony przez migrację (`file::migrated`) nie wraca
    // na listę jako drugi, starszy workflow o tej samej nazwie.
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    // Kolejność, w jakiej system plików oddaje wpisy, nie jest niczyją obietnicą. Sortowanie na
    // ekranie robi magazyn listy (po nazwie, bez wielkości liter); tutaj chodzi tylko o to, żeby
    // dwa wywołania na tym samym katalogu dały tę samą kolejność.
    paths.sort();

    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        let workflow = crate::workflow::file::load(&path)?;
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            // Nazwa pliku, która nie jest tekstem w UTF-8, nie da się odesłać do okna jako
            // `path` — a pozycja bez nazwy pliku jest pozycją, której nie da się ani zapisać,
            // ani usunąć.
            return Err(LoadError::Unreadable(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} is not a name Loadout can read", path.display()),
            )));
        };
        out.push(WorkflowEntry {
            path: name.to_owned(),
            workflow,
        });
    }
    Ok(out)
}

/// Usuwa plik workflow z biblioteki.
pub fn delete_workflow_inner(home: &Path, file_name: &str) -> Result<(), io::Error> {
    std::fs::remove_file(in_library(home, file_name)?)
}

/// Uwagi walidatora o tym workflow — **te same**, które padają przy zapisie i przed Startem.
#[must_use]
pub fn check_workflow_inner(workflow: &WorkflowFile) -> Vec<Note> {
    // Przelotka i nic więcej. Drugi walidator, dopisany tutaj „bo front potrzebuje jeszcze
    // jednej uwagi", byłby drugim miejscem, w którym mieszka odpowiedź na pytanie „co jest nie
    // tak z tym plikiem", i jedno z dwóch zawsze byłoby nieaktualne (niezmiennik 13).
    crate::workflow::check::check(workflow)
}
