//! Wspólny kształt odpowiedzi obu półek biblioteki i izolacja problemów.

use serde::{Deserialize, Serialize};

use crate::library::agents::AgentError;
use crate::workflow::file::LoadError;

/// Półka, na której leży definicja. Zamknięta lista — nigdy ścieżka z maszyny człowieka.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Shelf {
    Agents,
    Workflows,
}

/// Bezpieczna kategoria problemu potrzebna do zdania na ekranie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DefinitionProblemKind {
    Unreadable,
    Malformed,
    NewerFormat,
    MissingFormat,
    OlderFormat,
}

/// Zdrowa definicja albo opis jednego pliku, którego nie dało się zinterpretować.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Definition<T> {
    Healthy {
        value: T,
    },
    DefinitionProblem {
        shelf: Shelf,
        file_name: String,
        problem: DefinitionProblemKind,
    },
}

/// Rustowi callerzy, którzy potrzebują wyłącznie poprawnych definicji, przechodzą tę jedną drogę.
#[must_use]
pub fn healthy_only<T>(definitions: Vec<Definition<T>>) -> Vec<T> {
    definitions
        .into_iter()
        .filter_map(|definition| match definition {
            Definition::Healthy { value } => Some(value),
            Definition::DefinitionProblem { .. } => None,
        })
        .collect()
}

/// Jeden normalizator błędu agenta na zamkniętą kategorię widoczną w oknie.
#[must_use]
pub const fn agent_problem(error: &AgentError) -> DefinitionProblemKind {
    match error {
        AgentError::Unreadable { .. } => DefinitionProblemKind::Unreadable,
        AgentError::Malformed { .. } | AgentError::EmptySetting { .. } => {
            DefinitionProblemKind::Malformed
        }
    }
}

/// Jeden normalizator błędu workflowu na zamkniętą kategorię widoczną w oknie.
#[must_use]
pub const fn workflow_problem(error: &LoadError) -> DefinitionProblemKind {
    match error {
        LoadError::TooNew => DefinitionProblemKind::NewerFormat,
        LoadError::NoFormat => DefinitionProblemKind::MissingFormat,
        LoadError::TooOld => DefinitionProblemKind::OlderFormat,
        LoadError::Unreadable(_) => DefinitionProblemKind::Unreadable,
        LoadError::Malformed(_) => DefinitionProblemKind::Malformed,
    }
}
