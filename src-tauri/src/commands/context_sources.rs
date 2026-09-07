//! Cztery komendy źródeł biblioteki Context: dodaj, dokończ stronę, pokaż, usuń.
//!
//! Ta warstwa trzyma dokładnie to samo, co [`super::context`], i nic ponadto: **gdzie jest
//! biblioteka** (odwzorowanie `home → contexts/` w jednym miejscu, więc skorupa
//! `#[tauri::command]` nie zna układu katalogów) oraz **zegar** (`at` opisuje chwilę, w której
//! kliknął CZŁOWIEK, więc podaje go warstwa, która o kliknięciu wie).
//!
//! Osobny plik od `context.rs`, bo to jest osobne pytanie: tamten zapisuje NAZWĘ i SZKIC
//! zestawu, ten kładzie w nim PLIKI. Wspólny plik rósłby w miejsce, w którym mieszkają dwie
//! niepowiązane odpowiedzi (PLAN §12 nazywa go zresztą po imieniu).

use std::path::Path;

use crate::context::sources::{
    self, ImportItem, ImportReport, ImportRequest, PreparedPage, SourcePart,
};
use crate::context::{ContextSetRead, Error, files};

/// Kładzie w zestawie wszystko, co człowiek wybrał albo wkleił — i oddaje wynik KAŻDEJ pozycji.
pub fn import_context_sources_inner(
    home: &Path,
    set_id: &str,
    operation_id: &str,
    expected_revision: Option<String>,
    items: Vec<ImportItem>,
) -> Result<ImportReport, Error> {
    sources::import(
        &files::library_root(home),
        &ImportRequest {
            set_id: set_id.to_owned(),
            operation_id: operation_id.to_owned(),
            expected_revision,
            items,
            at: super::now_utc(),
        },
    )
}

/// Zatwierdza jedną przygotowaną stronę dokumentu.
pub fn complete_context_source_preparation_inner(
    home: &Path,
    set_id: &str,
    source_id: &str,
    page: &PreparedPage,
    expected_revision: Option<&str>,
) -> Result<ContextSetRead, Error> {
    sources::complete_preparation(
        &files::library_root(home),
        set_id,
        source_id,
        page,
        expected_revision,
        &super::now_utc(),
    )
}

/// Kawałek zatwierdzonego źródła — po jego identyfikatorze, nigdy po ścieżce od okna.
pub fn read_context_source_inner(
    home: &Path,
    set_id: &str,
    source_id: &str,
    page: Option<u32>,
) -> Result<SourcePart, Error> {
    sources::read_source(&files::library_root(home), set_id, source_id, page)
}

/// Zdejmuje źródło z zestawu.
pub fn remove_context_source_inner(
    home: &Path,
    set_id: &str,
    source_id: &str,
    expected_revision: Option<&str>,
) -> Result<ContextSetRead, Error> {
    sources::remove_source(
        &files::library_root(home),
        set_id,
        source_id,
        expected_revision,
        &super::now_utc(),
    )
}
