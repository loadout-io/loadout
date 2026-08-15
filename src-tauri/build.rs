//! Skrypt budowania. Musi tu być, zanim `tauri::generate_context!()` w `lib.rs` ma co czytać:
//! to `tauri_build::build()` zbiera pliki z `capabilities/`, dokłada do nich uprawnienia wtyczek
//! i zapisuje jedną listę, którą makro potem wkleja do binarki. Bez tego kroku „uprawnienie,
//! którego nie ma" jest błędem czasu wykonania w webviewie zamiast błędu kompilacji tutaj.

fn main() {
    tauri_build::build();
}
