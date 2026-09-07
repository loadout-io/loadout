//! Definicja oceny zapisana w grafie i run.json, nigdy odczytywana z dzisiejszego formularza.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{CaseStatus, EvalSet, file};

/// Tożsamość kryteriów, nie druga tabela wyników. Fakty wykonania pozostają w run.json.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasurementDefinition {
    pub schema: u8,
    pub revision: String,
    pub set: EvalSet,
    pub digest: String,
    #[serde(default)]
    pub graph_digest: String,
}

impl MeasurementDefinition {
    /// Wariant jest badaną zmienną. Zmiana jego instrukcji/modelu nie zmienia pytania,
    /// ale zmiana taska, kryterium albo wspólnego wejścia rozpoczyna nową serię.
    pub fn comparison_fingerprint(
        &self,
        input: &serde_json::Value,
        conditions: &serde_json::Value,
    ) -> Result<String, serde_json::Error> {
        let bytes = serde_json::to_vec(&(1_u8, &self.set.cases, input, conditions))?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }

    pub fn freeze(
        set: &EvalSet,
        revision: String,
        graph: &crate::workflow::WorkflowFile,
    ) -> Result<Self, serde_json::Error> {
        let mut set = set.clone();
        set.cases.retain(|case| case.status == CaseStatus::InUse);
        let graph_digest = graph_digest_of(&serde_json::to_value(graph)?)?;
        let digest = digest_of(&set, &revision, &graph_digest)?;
        Ok(Self {
            schema: 1,
            revision,
            set,
            digest,
            graph_digest,
        })
    }

    /// Uszkodzony zapis nie staje się zgodą na ocenę według aktualnego pliku.
    #[must_use]
    pub fn valid_for(&self, workflow_id: &str) -> bool {
        self.schema == 1
            && format!("eval:{}", self.set.id) == workflow_id
            // ZAKRES, NIE ROWNOSC — tak, jak sadzi to `file::why_it_would_not_hold` (file.rs:217)
            // i odczyt pliku (file.rs:129, odrzuca wylacznie format WIEKSZY niz biezacy).
            // Rownosc znaczyla, ze kazdy zestaw zapisany przed podniesieniem `CURRENT` z 1 na 2
            // przestaje byc oceniany: swiezo zakonczony pomiar melduje na ekranie „Criteria
            // snapshot unavailable" i ZERO zaliczonych, mimo ze bieg przeszedl. Format 2 dolozyl
            // wylacznie pole z `#[serde(default)]`, wiec zestaw w formacie 1 jest kompletny.
            && (1..=super::CURRENT).contains(&self.set.format)
            && self.set.why_it_cannot_run().is_none()
            && file::why_it_would_not_hold(&self.set).is_none()
            && !self.graph_digest.is_empty()
            && digest_of(&self.set, &self.revision, &self.graph_digest)
                .is_ok_and(|digest| digest == self.digest)
    }

    /// 2026-09-06: podmieniona mapa komórek przypisywała sukces innego egzaminatora.
    /// Cały zamrożony graf jest wejściem pomiaru, także jego mapa, połączenia i izolacja.
    #[must_use]
    pub fn matches_graph(&self, graph: &serde_json::Value) -> bool {
        graph_digest_of(graph).is_ok_and(|digest| digest == self.graph_digest)
    }
}

fn digest_of(
    set: &EvalSet,
    revision: &str,
    graph_digest: &str,
) -> Result<String, serde_json::Error> {
    let bytes = serde_json::to_vec(&(1_u8, revision, set, graph_digest))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn graph_digest_of(graph: &serde_json::Value) -> Result<String, serde_json::Error> {
    let mut graph = graph.clone();
    if let Some(fields) = graph.as_object_mut() {
        // Definicja zawiera ten skrót: wyłączamy wyłącznie cykliczne odwołanie do siebie.
        fields.remove("measurementDefinition");
    }
    let bytes = serde_json::to_vec(&(1_u8, graph))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
