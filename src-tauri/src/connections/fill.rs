//! Dopełnianie list połączeń u agentów, których o nie nikt nie zapytał.
//!
//! **Jedna reguła, jedno miejsce** (niezmiennik 23): wołają to obie drogi importu — „Import"
//! w sekcji Agents (`commands::import`) i „Import setup from project"
//! (`commands::project_setup`). Druga kopia tej reguły rozjechałaby się przy pierwszej zmianie
//! jednej z nich, a rozjazd byłoby widać dopiero jako agenta, który u jednego wejścia dostaje
//! połączenia, a u drugiego nie.
//!
//! 2026-09-11, rozstrzygnięcie właściciela: **zapisane `connections: []` znaczy „nikt nigdy nie
//! zapytał"**, a nie „żadnych". Pola Connections nie było w formularzu agenta, więc 26 z 32
//! agentów w jego bibliotece ma pustą listę bez ani jednej decyzji za sobą — i wolno ją
//! jednorazowo dopełnić. Agent, który coś wymienia, zostaje nietknięty: tam decyzja zapadła.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::library::agents::{Agent, read_agent_directory, write_agent_file};

/// Komu ten import dał połączenia i jakie — wpis paragonu i materiał na zdanie dla człowieka.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilledAgent {
    /// Nazwa agenta, czyli to, co człowiek widzi na kafelku — nigdy identyfikator pliku.
    pub agent: String,
    /// Nazwy połączeń, które ten agent dostał.
    pub connections: Vec<String>,
}

impl FilledAgent {
    /// Wpis o agencie, któremu właśnie dopisano listę.
    pub(crate) fn of(agent: &Agent) -> Self {
        Self {
            agent: agent.name.clone(),
            connections: agent.connections.clone(),
        }
    }
}

/// Dopisuje agentowi nazwy włączonych połączeń, jeśli sam nie wymienia żadnego. `true` = dopisano.
///
/// NAZWY, nie `id` (`runtime::enabled_names`): nazwą człowiek wpisuje połączenie w polu
/// Connections i nazwą szuka go Start. `runtime::selected` dopasowuje wprawdzie oba, ale jego
/// straż duplikatów patrzy na surowy napis wejściowy, więc mieszanka jednego i drugiego wpuściłaby
/// to samo połączenie dwa razy.
pub fn fill_one(agent: &mut Agent, enabled: &[String]) -> bool {
    if !agent.connections.is_empty() || enabled.is_empty() {
        return false;
    }
    agent.connections = enabled.to_vec();
    true
}

/// To samo dla agentów, którzy leżeli w bibliotece, zanim ten import się zaczął.
///
/// `brought` to nazwy połączeń, które ten przebieg ZAPISAŁ jako włączone. Pusto znaczy, że nie
/// ma czym dopełniać — i wtedy ta funkcja nie otwiera ani jednego pliku agenta, bo import, który
/// niczego nie włączył, nie ma prawa zostawić po sobie ani jednego zmienionego bajtu.
///
/// Lista, którą agenci dostają, jest czytana z dysku PO imporcie (`runtime::enabled_names`),
/// a nie sklejana z `brought`: biblioteka mogła mieć swoje włączone połączenia już wcześniej,
/// a agent dopełniony połową prawdy byłby drugą odpowiedzią na to samo pytanie (niezmiennik 13).
///
/// Odmowy są tu przemilczane z rozmysłem. Zapis idzie przez `write_agent_file` z rewizją, którą
/// ten przebieg przeczytał, więc agent zmieniony w tej chwili przez człowieka w sąsiednim oknie
/// zostaje POMINIĘTY, a nie nadpisany — a import, który już przeniósł swoje pliki, nie ma się
/// z czego wycofać (2026-09-14).
#[must_use]
pub fn fill_saved(library: &Path, brought: &[String]) -> Vec<FilledAgent> {
    if brought.is_empty() {
        return Vec::new();
    }
    let Ok(enabled) = super::runtime::enabled_names(&library.join("connections")) else {
        return Vec::new();
    };
    let agents = library.join("agents");
    let Ok(saved) = read_agent_directory(&agents) else {
        return Vec::new();
    };
    let mut filled = Vec::new();
    for (_path, read) in saved {
        let Ok(read) = read else {
            continue;
        };
        let mut agent = read.agent;
        if fill_one(&mut agent, &enabled)
            && write_agent_file(&agents, &agent, Some(&read.revision)).is_ok()
        {
            filled.push(FilledAgent::of(&agent));
        }
    }
    filled
}
