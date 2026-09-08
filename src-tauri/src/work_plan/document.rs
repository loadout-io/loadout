use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Kryterium i jawna metoda, którą późniejszy etap ma je sprawdzić.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceCriterion {
    pub text: String,
    pub verification: String,
}

/// Pochodzenie i status są osobnymi osiami: zapis nie może zmienić jednego w drugie.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Human,
    Generated,
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Proposed,
    Agreed,
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Requirement {
    pub id: String,
    pub text: String,
    pub acceptance: Vec<AcceptanceCriterion>,
    pub origin: Origin,
    pub status: Status,
}

/// Znane sekcje mają stabilne nazwy, a nowe pozostają danymi zamiast nowym parserem Markdown.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SectionKey {
    Implementation,
    Design,
    Validation,
    Other(String),
}

impl SectionKey {
    pub(crate) fn label(&self) -> &str {
        match self {
            Self::Implementation => "Implementation",
            Self::Design => "Design",
            Self::Validation => "Validation",
            Self::Other(label) => label,
        }
    }
}

impl Serialize for SectionKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.label())
    }
}

impl<'de> Deserialize<'de> for SectionKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let label = String::deserialize(deserializer)?;
        Ok(match label.as_str() {
            "Implementation" => Self::Implementation,
            "Design" => Self::Design,
            "Validation" => Self::Validation,
            _ => Self::Other(label),
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    pub name: String,
    pub locator: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlanDocument {
    pub goal: String,
    pub in_scope: Vec<String>,
    pub out_of_scope: Vec<String>,
    pub requirements: Vec<Requirement>,
    pub acceptance: Vec<AcceptanceCriterion>,
    pub decisions: Vec<String>,
    pub sections: BTreeMap<SectionKey, String>,
    pub proposals: Vec<String>,
    pub assumptions: Vec<String>,
    pub questions: Vec<String>,
    pub conflicts: Vec<String>,
    pub sources: Vec<SourceRef>,
}

impl PlanDocument {
    #[must_use]
    pub fn requirement(&self, id: &str) -> Option<&Requirement> {
        self.requirements
            .iter()
            .find(|requirement| requirement.id == id)
    }
}
