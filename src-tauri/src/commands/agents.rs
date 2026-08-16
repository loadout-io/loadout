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

use crate::library::agents::{Agent, AgentError, read_agent_file, write_agent_file};

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
fn saved(home: &Path) -> Result<Vec<(PathBuf, Agent)>, AgentError> {
    let dir = home.join(AGENTS_DIR);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        // Biblioteka, w której nikt jeszcze nikogo nie zapisał, ma zero agentów, a nie błąd.
        // Pusta sekcja Agenci przy pierwszym uruchomieniu jest prawdą; czerwony pasek nie jest.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(refused(&dir, &error.to_string())),
    };

    // Płasko i wyłącznie `.md`: w katalogu biblioteki potrafi wylądować `.DS_Store` albo kopia
    // zapasowa z edytora, a spacer po drzewie zwróciłby je jako agentów.
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    // Sortujemy ŚCIEŻKI, nie wynik: kolejność, w jakiej system plików oddaje wpisy, nie jest
    // niczyją obietnicą, a lista, która przy każdym otwarciu sekcji układa się inaczej, wygląda
    // jak lista, która się zmieniła.
    paths.sort();

    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        // Nieczytelny plik agenta **przewraca całą listę**, i to jest różnica wobec notatek
        // (`memory::notes::scan_notes` pomija je z wpisem w dzienniku). Plik agenta pisze
        // człowiek i literówka ma zaboleć od razu, z nazwą pliku w zdaniu [T4 §10] — cicho
        // pominięty agent znika z listy i z workflow, który go woła, a jedyny ślad zostaje
        // w dzienniku, którego nikt nie czyta.
        let agent = read_agent_file(&path)?;
        out.push((path, agent));
    }
    Ok(out)
}

/// Wszyscy zapisani agenci, po jednym na plik.
///
/// `home` to `~/.loadout` i przychodzi **argumentem**, nigdy z `HOME` czytanego w środku —
/// katalog domowy odczytany tutaj znaczyłby, że każdy test pisze do prawdziwej biblioteki
/// (ten sam powód, co przy `RunDeps::home`).
pub fn list_agents_inner(home: &Path) -> Result<Vec<Agent>, AgentError> {
    Ok(saved(home)?.into_iter().map(|(_, agent)| agent).collect())
}

/// Zapisuje agenta i oddaje ścieżkę, pod którą wylądował.
///
/// Ścieżka wraca, bo bez niej wołający nie ma jak sprawdzić, że plik naprawdę powstał — a
/// „komenda zwróciła `Ok`" to jest dokładnie ta asercja, którą przechodzi implementacja
/// pisząca do `/dev/null`.
pub fn save_agent_inner(home: &Path, agent: &Agent) -> Result<PathBuf, AgentError> {
    // Cała droga bajtów — nazwa pliku, kolejność wierszy front-mattera, `create_dir_all` —
    // jest w `write_agent_file` (T-11). Tutaj składa się wyłącznie katalog.
    write_agent_file(&home.join(AGENTS_DIR), agent)
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
