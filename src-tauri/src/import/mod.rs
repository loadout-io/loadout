//! Bezpieczna migracja konfiguracji repo do natywnych plików Loadouta.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::connections::Connection;
use crate::library::agents::Agent;
use crate::workflow::WorkflowFile;

pub mod adapters;
pub mod apply;
pub mod discover;
pub mod translate;

/// Wersja mapowania zapisywana w receipt.
pub const ADAPTER_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Claude,
    Codex,
    AgentSkills,
    Rulesync,
    OpenStandard,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Agent,
    Skill,
    Connection,
    Workflow,
    Hook,
    Memory,
    Rule,
    Unknown,
}

/// Jeden znaleziony fakt. Nie niesie surowej treści ani wartości środowiska.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceItem {
    pub id: String,
    pub source: SourceKind,
    pub kind: ItemKind,
    pub path: PathBuf,
    pub hash: String,
    pub name: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverySnapshot {
    pub root: PathBuf,
    pub items: Vec<SourceItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compatibility {
    Exact,
    Adjusted,
    NeedsChoice,
    Unsupported,
}

impl Compatibility {
    #[must_use]
    pub const fn blocks(self) -> bool {
        matches!(self, Self::NeedsChoice | Self::Unsupported)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mapping {
    pub item_id: String,
    pub compatibility: Compatibility,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityReport {
    pub mappings: Vec<Mapping>,
}

impl CompatibilityReport {
    #[must_use]
    pub fn blockers(&self) -> usize {
        self.mappings
            .iter()
            .filter(|mapping| mapping.compatibility.blocks())
            .count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDraft {
    pub name: String,
    pub source_dir: PathBuf,
    pub source_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationDraft {
    pub root: PathBuf,
    pub source_hashes: BTreeMap<PathBuf, String>,
    pub agents: Vec<Agent>,
    pub skills: Vec<SkillDraft>,
    pub connections: Vec<Connection>,
    pub workflows: Vec<WorkflowFile>,
    pub report: CompatibilityReport,
}

impl MigrationDraft {
    #[must_use]
    pub fn runnable(&self) -> bool {
        self.report.blockers() == 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub snapshot: DiscoverySnapshot,
    pub draft: MigrationDraft,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("Loadout could not inspect {path}: {detail}")]
    Inspect { path: PathBuf, detail: String },
    #[error("This setup still has {0} unresolved item(s). Resolve them before saving.")]
    Blocked(usize),
    #[error("The project setup changed after Scan. Scan it again before saving.")]
    Changed,
    #[error("Loadout could not save the imported setup: {0}")]
    Save(String),
}

pub type Result<T> = std::result::Result<T, ImportError>;
