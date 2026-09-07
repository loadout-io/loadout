//! Cztery komendy biblioteki Context: wypisz, przeczytaj, utwórz, zapisz.
//!
//! CO TA WARSTWA NAPRAWDĘ ROBI, skoro cała mechanika plików stoi w [`crate::context::files`]:
//! trzyma dwie rzeczy, których tamten moduł znać nie ma i których `ipc.rs` znać nie powinien.
//!
//! - **gdzie jest biblioteka.** Odwzorowanie `home → contexts/` mieszka tu w jednym miejscu, więc
//!   skorupa `#[tauri::command]` nie zna układu katalogów, a drugie okno nie ma jak zapytać
//!   o inny (ta sama umowa, co `memory::notes_root` w `list_notes`).
//! - **zegar.** `at` opisuje chwilę, w której kliknął CZŁOWIEK, a nie moment, w którym bajty
//!   dotarły na dysk — więc podaje go warstwa, która o kliknięciu wie, a nie funkcja zapisu.

use std::path::Path;

use crate::context::files::{self, DraftEdit};
use crate::context::{ContextDraft, ContextSet, ContextSetRead, Error};

/// Wszystkie gotowe zestawy tej biblioteki.
pub fn list_context_sets_inner(home: &Path) -> Result<Vec<ContextSet>, Error> {
    files::list_sets(&files::library_root(home))
}

/// Jeden zestaw w całości — manifest, szkic i rewizja, którą okno odda przy zapisie.
pub fn read_context_set_inner(home: &Path, id: &str) -> Result<ContextSetRead, Error> {
    files::read_set(&files::library_root(home), id)
}

/// Nowy zestaw pod nazwą, którą wpisał człowiek.
pub fn create_context_set_inner(home: &Path, title: &str) -> Result<ContextSetRead, Error> {
    files::create_set(&files::library_root(home), title, &super::now_utc())
}

/// Zapisuje tytuł, opis i szkic — albo odmawia, kiedy na dysku leży nowsza praca.
pub fn save_context_draft_inner(
    home: &Path,
    id: &str,
    title: &str,
    description: &str,
    draft: ContextDraft,
    expected_revision: Option<String>,
) -> Result<ContextSetRead, Error> {
    files::save_draft(
        &files::library_root(home),
        &DraftEdit {
            id: id.to_owned(),
            title: title.to_owned(),
            description: description.to_owned(),
            draft,
            expected_revision,
            at: super::now_utc(),
        },
    )
}
