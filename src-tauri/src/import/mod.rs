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

/// Jedna notatka wyjęta z pamięci cudzego projektu.
///
/// 2026-08-22 (T-80) — PO CO TO ISTNIEJE. `.claude/agent-memory/` i `.claude/learnings/` były
/// dotąd pozycjami rodzaju [`ItemKind::Memory`], czyli **wyborem do rozstrzygnięcia**, i nic
/// poza tym: [`MigrationDraft`] nie miał pola na pamięć, więc `apply` nie zapisywał do
/// `memory/notes/` ani jednego pliku. Wiedza jednego agenta jechała wtedy w **stałej**
/// instrukcji w każdym jego promptcie, a ta sama treść potrafiła wejść drugi raz przez
/// learnings.
///
/// Pola są tym, co trzeba wiedzieć, żeby później powiedzieć **skąd to jest**: notatka bez
/// pochodzenia jest zdaniem, którego nie da się ani sprawdzić, ani wycofać [T6 §5.1].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryNote {
    /// Plik, z którego to przyszło — ścieżka **względna** wobec korzenia importu. Absolutna
    /// ścieżka gospodarza nie jest faktem o notatce, tylko o maszynie, na której skanowano.
    pub source: PathBuf,
    /// Odcisk tamtego pliku w chwili skanu — ten sam, który niesie [`SourceItem::hash`].
    pub source_hash: String,
    /// Z czyjego katalogu to wzięliśmy.
    pub app: SourceKind,
    /// Czyja to wiedza. `None` znaczy „niczyja" i nie ma udawać, że czyjaś.
    pub agent: Option<String>,
    /// Zakres, słowem z pliku notatki: `everywhere`, `this-project` albo `this-agent`.
    ///
    /// Słowo, nie `memory::notes::Scope`: import składa **plik**, a plik jest prawdą
    /// (niezmiennik 4). Drugi typ na tę samą wartość rozjechałby się przy pierwszej zmianie
    /// któregoś z nich.
    pub scope: String,
    /// Zdanie, które pojedzie do promptu — jedyna część notatki, która tam jedzie.
    pub rule: String,
    /// Nazwa, po której człowiek pozna to na liście.
    pub title: String,
    /// Dlaczego to jest prawda. „No because, no memory" [T6 §10.3] obowiązuje też notatkę,
    /// której nikt tutaj nie napisał ręcznie.
    pub because: String,
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
    /// Pamięć projektu jako notatki, nie jako akapity w instrukcjach agenta.
    #[serde(default)]
    pub notes: Vec<MemoryNote>,
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
