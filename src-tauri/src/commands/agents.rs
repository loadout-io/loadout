//! Komendy biblioteki agentów: wypisz, zapisz, usuń.
//!
//! **Ani jednego `use tauri::` i ani jednego `#[tauri::command]`** — jak w całym tym katalogu
//! (`docs/ARCHITECTURE.md` §3). Tutaj mieszka to, co da się uruchomić w teście: funkcje biorące
//! katalog biblioteki **argumentem**. Dwuliniowe skorupy nad nimi stoją w `ipc.rs`, bo
//! `State<'_, AppState>` nie da się zbudować w teście jednostkowym, a `&Path` da się w jednym
//! wierszu [04 §2.1].
//!
//! Cała droga bajtów jest już wylądowana w `library::agents` (T-11): `read_agent_file` zna
//! format pliku, `write_agent_file` zna regułę nazwy pliku. Te funkcje nie powtarzają ani
//! jednej z tych decyzji — składają katalog, wołają tamto i oddają wynik. Druga reguła nazwy
//! pliku, dopisana tutaj „bo wygodniej", byłaby drugim miejscem, w którym mieszka odpowiedź na
//! pytanie „gdzie leży ten agent" (niezmiennik 23).

use std::path::{Path, PathBuf};

use crate::library::agents::{Agent, AgentError};

/// Wszyscy zapisani agenci, po jednym na plik.
///
/// `home` to `~/.loadout` i przychodzi **argumentem**, nigdy z `HOME` czytanego w środku —
/// katalog domowy odczytany tutaj znaczyłby, że każdy test pisze do prawdziwej biblioteki
/// (ten sam powód, co przy `RunDeps::home`).
pub fn list_agents_inner(home: &Path) -> Result<Vec<Agent>, AgentError> {
    todo!(
        "read every saved agent out of the library under {}",
        home.display()
    )
}

/// Zapisuje agenta i oddaje ścieżkę, pod którą wylądował.
///
/// Ścieżka wraca, bo bez niej wołający nie ma jak sprawdzić, że plik naprawdę powstał — a
/// „komenda zwróciła `Ok`" to jest dokładnie ta asercja, którą przechodzi implementacja
/// pisząca do `/dev/null`.
pub fn save_agent_inner(home: &Path, agent: &Agent) -> Result<PathBuf, AgentError> {
    todo!(
        "write {} into the library under {}",
        agent.name,
        home.display()
    )
}

/// Usuwa agenta o tym identyfikatorze razem z jego plikiem.
///
/// Identyfikator, nie nazwa pliku: nazwa pliku powstaje ze zmiennej nazwy agenta, a `id` jest
/// stabilne przez zmianę nazwy (T4 §5.1). Front zna tylko `id` i tak ma zostać.
pub fn delete_agent_inner(home: &Path, id: &str) -> Result<(), AgentError> {
    todo!(
        "drop the agent {id} and its file from the library under {}",
        home.display()
    )
}
