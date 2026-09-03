//! Jedna migawka lokalnych aplikacji agentów, bez wpływu na wybór ani start kroku.

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
pub async fn check_agent_apps_inner(drivers: &Drivers) -> Vec<AgentAppWire> {
    let claude = drivers(Vendor::ClaudeCode);
    let codex = drivers(Vendor::Codex);
    let (claude, codex) = tokio::join!(claude.probe(), codex.probe());

    vec![
        public_result(CLAUDE_CODE, claude),
        public_result(CODEX, codex),
    ]
}
