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

use std::path::{Path, PathBuf};

use crate::workflow::WorkflowFile;
use crate::workflow::check::Note;
use crate::workflow::file::{LoadError, SaveError};

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
    todo!(
        "save {} as {file_name} in the library under {}",
        workflow.name,
        home.display()
    )
}

/// Wczytuje workflow spod `file_name` w bibliotece.
pub fn load_workflow_inner(home: &Path, file_name: &str) -> Result<WorkflowFile, LoadError> {
    todo!(
        "read {file_name} back out of the library under {}",
        home.display()
    )
}

/// Uwagi walidatora o tym workflow — **te same**, które padają przy zapisie i przed Startem.
#[must_use]
pub fn check_workflow_inner(workflow: &WorkflowFile) -> Vec<Note> {
    todo!(
        "hand back everything the validator says about {}",
        workflow.name
    )
}
