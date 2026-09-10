//! Biblioteka użytkownika: rzeczy, które definiuje się raz i potem zapomina.
//!
//! Ten plik istnieje z jednego powodu i ma jedną linię. Mapa własności dała `mod.rs`
//! odpowiednikom w `memory/` i `skills/`, a tutaj go pominęła — a moduł bez deklaracji
//! nie jest częścią skrzyni, tylko plikiem, który leży obok. Test integracyjny linkujący
//! się z `loadout_lib` zobaczyłby wtedy „unresolved import", czyli czerwień, która niczego
//! nie sprawdziła (`AGENTS.md` §2a p. 5).

pub mod agent_generation;
pub mod agents;
pub mod definition;

/// 2026-09-10: zawartość projektu nigdy nie korzysta z domyślnej biblioteki
/// użytkownika. Stare pliki pozostają źródłem jawnego importu.
#[must_use]
pub fn project_root(project: &std::path::Path) -> std::path::PathBuf {
    project.join(".loadout")
}
