//! Natywne połączenia narzędziowe. Projekt jest źródłem propozycji, Loadout jest autorytetem.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub mod runtime;

/// Transport, który Loadout potrafi bezpiecznie przepisać do konfiguracji vendora.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum Transport {
    Stdio {
        command: String,
        args: Vec<String>,
        /// Wyłącznie nazwy wymaganych zmiennych, nigdy ich wartości.
        environment: Vec<String>,
    },
    Http {
        url: String,
        token_environment: Option<String>,
    },
}

/// Połączenie znalezione w repo. Zawsze wyłączone do jawnej decyzji człowieka.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub transport: Transport,
    pub source: PathBuf,
    pub source_hash: String,
}

impl Connection {
    #[must_use]
    pub fn imported(
        id: String,
        name: String,
        transport: Transport,
        source: PathBuf,
        source_hash: String,
    ) -> Self {
        Self {
            id,
            name,
            enabled: false,
            transport,
            source,
            source_hash,
        }
    }
}
