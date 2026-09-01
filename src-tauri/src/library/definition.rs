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
        /// Rewizja pliku, z którego ta definicja powstała — dokładnie te bajty, nie odczyt
        /// zrobiony sekundę później.
        ///
        /// 2026-08-28 — stoi przy definicji, a nie osobną komendą „daj rewizję", bo okno musi
        /// wysłać ją z powrotem przy zapisie: rewizja pobrana drugim wywołaniem opisywałaby
        /// inną chwilę niż to, co człowiek ma przed sobą, i zapis dalej mógłby cofnąć cudzą pracę.
        revision: String,
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
            Definition::Healthy { value, .. } => Some(value),
            Definition::DefinitionProblem { .. } => None,
        })
        .collect()
}

/// Jeden normalizator błędu agenta na zamkniętą kategorię widoczną w oknie.
#[must_use]
pub const fn agent_problem(error: &AgentError) -> DefinitionProblemKind {
    match error {
        // `Changed` powstaje wyłącznie przy ZAPISIE i nigdy nie przyjeżdża z listowania —
        // ramię jest tu po to, żeby wyczerpać typ, a nie żeby opisywać stan półki. Ekran
        // dostaje wtedy najbliższą prawdę: tego pliku nie udało się użyć.
        AgentError::Unreadable { .. } | AgentError::Changed => DefinitionProblemKind::Unreadable,
        // `CarriesASecret` powstaje — jak `Changed` wyżej — WYŁĄCZNIE przy zapisie i nigdy nie
        // przyjeżdża z listowania półki: plik z sekretem nie ma jak na niej wylądować, bo brama
        // stoi przed pierwszym bajtem. Ramię jest tu po to, żeby wyczerpać typ.
        AgentError::Malformed { .. }
        | AgentError::EmptySetting { .. }
        | AgentError::CarriesASecret { .. } => DefinitionProblemKind::Malformed,
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
