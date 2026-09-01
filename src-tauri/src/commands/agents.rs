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

use std::borrow::Borrow;
use std::path::{Path, PathBuf};

use crate::library::agents::{
    Agent, AgentError, WrittenAgent, agent_file_name, read_agent_directory, write_agent_file,
};
use crate::library::definition::{Definition, Shelf, agent_problem, healthy_only};

/// Katalog agentów wewnątrz biblioteki: `~/.loadout/agents/` (`docs/ARCHITECTURE.md` §8).
///
/// Jedna stała na cały moduł, bo wszystkie trzy komendy muszą patrzeć w to samo miejsce.
/// Katalog doklejany osobno w każdej z nich to trzy odpowiedzi na pytanie „gdzie leżą
/// agenci", a rozjazd między nimi wygląda jak zniknięty agent, nie jak literówka.
const AGENTS_DIR: &str = "agents";

/// Odmowa, która nazywa plik — ten sam kształt, którym mówi `library::agents`.
///
/// T4 §10: „pokaż nazwę pliku i «Open in editor», nie połykaj". Warstwa komend nie ma własnego
/// typu błędu i mieć nie będzie: drugi enum opisujący te same awarie biblioteki byłby drugim
/// miejscem, w którym mieszka zdanie dla użytkownika (niezmiennik 23).
fn refused(path: &Path, detail: &str) -> AgentError {
    AgentError::Unreadable {
        file: path.display().to_string(),
        detail: detail.to_string(),
    }
}

/// Każdy zapisany agent razem ze ścieżką, spod której przyszedł.
///
/// Ścieżka jest tu, bo usunięcie po `id` musi wiedzieć, **który plik** zdjąć, a reguła nazwy
/// pliku mieszka w `write_agent_file` (T-11) i nie ma prawa zostać przepisana tutaj: slug
/// powstaje ze zmiennej nazwy agenta, więc agent przemianowany między zapisem a usunięciem
/// leżałby pod nazwą, której ta warstwa by nie zgadła. Odpowiedź daje **odczyt**, nie zgadywanie.
fn saved_definitions(home: &Path) -> Result<Vec<Definition<(PathBuf, Agent)>>, AgentError> {
    let dir = home.join(AGENTS_DIR);
    read_agent_directory(&dir)?
        .into_iter()
        .map(|(path, agent)| {
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| refused(&dir, "an agent file name could not be shown safely"))?
                .to_owned();
            Ok(match agent {
                Ok(read) => Definition::Healthy {
                    revision: read.revision,
                    value: (path, read.agent),
                },
                Err(error) => Definition::DefinitionProblem {
                    shelf: Shelf::Agents,
                    file_name,
                    problem: agent_problem(&error),
                },
            })
        })
        .collect()
}

fn saved(home: &Path) -> Result<Vec<(PathBuf, Agent)>, AgentError> {
    Ok(healthy_only(saved_definitions(home)?))
}

/// Wszyscy zapisani agenci, po jednym na plik.
///
/// `home` to `~/.loadout` i przychodzi **argumentem**, nigdy z `HOME` czytanego w środku —
/// katalog domowy odczytany tutaj znaczyłby, że każdy test pisze do prawdziwej biblioteki
/// (ten sam powód, co przy `RunDeps::home`).
pub fn list_agents_inner(home: &Path) -> Result<Vec<Agent>, AgentError> {
    Ok(saved(home)?.into_iter().map(|(_, agent)| agent).collect())
}

/// Unionowe wejście dla listy w oknie. Jeden wadliwy plik zostaje jednym problemem bez
/// odbierania zdrowych definicji callerom, którzy przechodzą przez [`healthy_only`].
pub fn list_agent_definitions_inner(home: &Path) -> Result<Vec<Definition<Agent>>, AgentError> {
    saved_definitions(home).map(|definitions| {
        definitions
            .into_iter()
            .map(|definition| match definition {
                Definition::Healthy {
                    value: (_, value),
                    revision,
                } => Definition::Healthy { value, revision },
                Definition::DefinitionProblem {
                    shelf,
                    file_name,
                    problem,
                } => Definition::DefinitionProblem {
                    shelf,
                    file_name,
                    problem,
                },
            })
            .collect()
    })
}

/// Zapisuje agenta i oddaje ścieżkę, pod którą wylądował.
///
/// Ścieżka wraca, bo bez niej wołający nie ma jak sprawdzić, że plik naprawdę powstał — a
/// „komenda zwróciła `Ok`" to jest dokładnie ta asercja, którą przechodzi implementacja
/// pisząca do `/dev/null`.
///
/// `impl Borrow<Agent>` zamiast `&Agent`, i to jest ustępstwo na rzecz jedynego wołającego,
/// który nie ma wyboru: skorupa `#[tauri::command]` dostaje agenta **wartością**, bo `serde`
/// musi go gdzieś zbudować, a struktury z polami `String` nie da się pożyczyć z bufora żądania.
/// Skorupa, która tę wartość tylko pożycza dalej, jest funkcją biorącą przez wartość i nie
/// konsumującą — czyli ostrzeżeniem clippy, którego w tym repo nie wolno wyciszyć. Wołający
/// z `&agent` w ręku nie zauważa różnicy.
///
/// `expected` jest rewizją pliku, którą okno przeczytało dla TEGO agenta — `None`, kiedy go
/// jeszcze nie widziało. Przelotka, bez ani jednej decyzji: co znaczy „ten sam plik" wie
/// `write_agent_file`, bo to ono zna regułę nazwy pliku.
pub fn save_agent_inner(
    home: &Path,
    agent: impl Borrow<Agent>,
    expected: Option<&str>,
) -> Result<WrittenAgent, AgentError> {
    let agent = agent.borrow();
    let file_name = agent_file_name(agent);
    if saved_definitions(home)?.iter().any(|definition| {
        // 2026-08-28: domyślny macOS traktuje te nazwy jako ten sam leaf. Guard musi robić
        // to samo także na case-sensitive CI, zanim writer otworzy kanoniczne `collision.md`.
        matches!(
            definition,
            Definition::DefinitionProblem {
                file_name: problem_file,
                ..
            } if problem_file.eq_ignore_ascii_case(&file_name)
        )
    }) {
        return Err(refused(
            &home.join(AGENTS_DIR).join(&file_name),
            "fix or delete this unreadable agent file before saving another agent here",
        ));
    }
    // Cała droga bajtów — nazwa pliku, kolejność wierszy front-mattera, `create_dir_all` —
    // jest w `write_agent_file` (T-11). Tutaj składa się wyłącznie katalog.
    write_agent_file(&home.join(AGENTS_DIR), agent, expected)
}

/// Usuwa agenta o tym identyfikatorze razem z jego plikiem.
///
/// Identyfikator, nie nazwa pliku: nazwa pliku powstaje ze zmiennej nazwy agenta, a `id` jest
/// stabilne przez zmianę nazwy (T4 §5.1). Front zna tylko `id` i tak ma zostać.
pub fn delete_agent_inner(home: &Path, id: &str) -> Result<(), AgentError> {
    let dir = home.join(AGENTS_DIR);
    let Some((path, _)) = saved(home)?
        .into_iter()
        .find(|(_, agent)| agent.id.to_string() == id)
    else {
        // Odmowa, nie ciche `Ok`. „Usunięte" o agencie, którego ta biblioteka nigdy nie
        // widziała, jest zdaniem, po którym lista odświeżona za sekundę pokazuje go dalej —
        // a człowiek czyta to jako niedziałający przycisk, nie jako pomyłkę w identyfikatorze.
        return Err(refused(
            &dir,
            &format!("there is no agent {id} here, so there was nothing to remove"),
        ));
    };

    // Plik, nie wiersz w indeksie: pliki są prawdą (niezmiennik 4), więc agent odfiltrowany
    // z listy i zostawiony na dysku wraca przy następnym starcie i wygląda jak nieudane
    // usunięcie, którego nikt nie umie powtórzyć.
    std::fs::remove_file(&path).map_err(|error| refused(&path, &error.to_string()))
}
