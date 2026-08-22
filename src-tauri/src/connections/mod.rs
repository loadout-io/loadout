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

/// Skąd to połączenie się wzięło — i **kto je widzi poza tobą**.
///
/// 2026-08-22 — TYP JEST NOWY i istnieje dla jednego zdania na ekranie. Claude Code ma trzy
/// zakresy MCP, a import czytał jeden: `linear-server`, z którego korzysta całe `ship-task`
/// w repo właściciela, siedział w zakresie LOKALNYM (`~/.claude.json`, `projects[<katalog>]`),
/// więc nie było go w `.mcp.json` i żaden bieg go nie dostawał. Zdanie „Connection linear-server
/// does not exist in the Loadout library." padało wtedy przy Starcie, o serwerze, którego
/// człowiek używa u siebie codziennie.
///
/// Rozróżnienie jest tu, a nie w ścieżce pliku, bo o to pyta człowiek stojący nad listą przed
/// importem: **to jest ustawienie zespołu czy moje własne?** Ścieżka odpowiada na to okrężnie
/// i tylko komuś, kto zna te trzy pliki na pamięć.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Origin {
    /// Plik projektu — `.mcp.json` i jego odpowiedniki. Widzi to cały zespół.
    #[default]
    Project,
    /// `~/.claude.json`, `projects[<skanowany katalog>]`. Tylko ty, tylko w tym projekcie.
    YoursHere,
    /// `~/.claude.json`, `mcpServers` z najwyższego poziomu. Tylko ty, w każdym projekcie.
    YoursEverywhere,
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
    /// `#[serde(default)]`: połączenie zapisane przed 2026-08-22 nie niesie tego pola, a jego
    /// brak znaczy dokładnie to, czym wtedy było wszystko — plik projektu.
    #[serde(default)]
    pub origin: Origin,
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
            origin: Origin::Project,
        }
    }
}
