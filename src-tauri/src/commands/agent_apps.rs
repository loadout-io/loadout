//! Jedna migawka lokalnych aplikacji agentów, bez wpływu na wybór ani na start kroku.
//!
//! **NIE JEST TO DRUGA WYROCZNIA STARTU** (2026-09, Z-34). Odmowę uruchomienia wydaje
//! `commands::run::check_to_run` i tylko ona; to tutaj mówi wyłącznie, co widziało okno
//! w chwili pytania. Dwie odpowiedzi na jedno pytanie rozjeżdżają się przy pierwszej
//! instalacji zrobionej przy otwartym oknie, a rozjazd czyta się jak awaria.

use serde::Serialize;

use super::Drivers;
use crate::engine::drivers::Probe;
use crate::library::agents::Vendor;

const CLAUDE_CODE: &str = "claude-code";
const CODEX: &str = "codex";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentAppState {
    Found,
    NotFound,
    CouldNotCheck,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentAppWire {
    pub app: &'static str,
    pub state: AgentAppState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Trzy stany, nie dwa, i to jest cała treść tej funkcji: „nie ma" jest odpowiedzią o świecie,
/// a „nie dało się sprawdzić" jest odpowiedzią o nas. Zlanie ich w jedno kazałoby oknu napisać
/// „zainstaluj to" komuś, kto ma to zainstalowane i tylko nie odpowiedziało w pięć sekund.
fn public_result(app: &'static str, result: anyhow::Result<Probe>) -> AgentAppWire {
    match result {
        Ok(Probe {
            found: true,
            version: Some(version),
        }) if !version.trim().is_empty() => AgentAppWire {
            app,
            state: AgentAppState::Found,
            version: Some(version),
        },
        Ok(Probe { found: false, .. }) => AgentAppWire {
            app,
            state: AgentAppState::NotFound,
            version: None,
        },
        Ok(_) | Err(_) => AgentAppWire {
            app,
            state: AgentAppState::CouldNotCheck,
            version: None,
        },
    }
}

/// Obie sondy ruszają razem, ale każda zajmuje własną pozycję w stałej kolejności odpowiedzi.
///
/// Rozliczane są **osobno**: awaria jednego vendora nie ma prawa skasować poprawnej odpowiedzi
/// drugiego, bo wtedy jedna zepsuta instalacja zabierałaby oknu wiedzę o obu.
pub async fn check_agent_apps_inner(drivers: &Drivers) -> Vec<AgentAppWire> {
    let claude = drivers(Vendor::ClaudeCode);
    let codex = drivers(Vendor::Codex);
    let (claude, codex) = tokio::join!(claude.probe(), codex.probe());

    vec![
        public_result(CLAUDE_CODE, claude),
        public_result(CODEX, codex),
    ]
}
