//! Powłoka aplikacji po stronie Rusta: dziennik, hak paniki, okno.
//!
//! Logowanie jest modułem *wewnątrz* tego pliku, bo `src-tauri/src/logging.rs` nie należy do
//! T-01. Nie zakładamy tu też `engine/` ani helperów, po które sięgnie T-02: niezmiennik 1
//! czyta się w tym zadaniu odwrotnie — silnik nie ma prawa zależeć od pliku, który zna Tauri.
//!
//! SZKIELET (faza kontraktowa T-01): same sygnatury. `todo!()` jest przejściowe — znika w fazie
//! implementacji, a `clippy::todo = deny` w Cargo.toml pilnuje, żeby żaden nie dożył pełnej bramki.

use std::io;
use std::path::{Path, PathBuf};

/// Wpina `tracing` w plik pod `dir` i zwraca ścieżkę tego pliku. Zdarzenia lecą jednocześnie
/// na wyjście diagnostyczne i do pliku, bo uruchomiona dwuklikiem aplikacja nie ma tego
/// pierwszego: LaunchServices je wyrzuca, więc release bez pliku jest niediagnozowalny.
///
/// Uchwyt pliku jest JEDEN na cały bieg (`Arc<File>` + `MakeWriterExt::and`), nigdy
/// `try_clone()` na linijkę: w Murmurze to był `dup(2)` na linijkę i panika z wyczerpania
/// deskryptorów wewnątrz samego logowania [T8 §9, 2026-08-15].
pub fn install_logging(dir: &Path) -> io::Result<PathBuf> {
    let _ = dir;
    todo!("T-01: tee tracing do pliku, jeden uchwyt na cały bieg")
}

/// Wpina hak paniki, który najpierw loguje przez `tracing`, a potem **woła poprzedni hak**.
///
/// Łańcuchowanie, nie zastąpienie: tokio połyka paniki na granicy zadania, a domyślny hak pisze
/// wyłącznie na wyjście diagnostyczne, które LaunchServices wyrzuca — hak, który zastępuje
/// poprzedni, kasuje jedyny ślad po pierwszej panice w release [T8 §9, 2026-08-15].
pub fn install_panic_hook() {
    todo!("T-01: zaloguj panikę przez tracing, potem zawołaj poprzedni hak")
}
