use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{
    AcceptanceCriterion, Error, Origin, PlanDocument, Requirement, SectionKey, SourceRef, Status,
};

/// Wymaganie człowieka wchodzi z aplikacji, nie z deserializowanego kandydata modelu.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HumanRequirement {
    pub id: String,
    pub text: String,
    pub acceptance: Vec<AcceptanceCriterion>,
}

/// Kandydat nie ma pól `origin` ani `status`, więc nie może sam sobie nadać autorytetu.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposedRequirement {
    pub id: String,
    pub text: String,
    pub acceptance: Vec<AcceptanceCriterion>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanCreate {
    pub goal: String,
    pub in_scope: Vec<String>,
    pub out_of_scope: Vec<String>,
    pub requirements: Vec<ProposedRequirement>,
    pub acceptance: Vec<AcceptanceCriterion>,
    pub decisions: Vec<String>,
    pub sections: BTreeMap<SectionKey, String>,
    pub proposals: Vec<String>,
    pub assumptions: Vec<String>,
    pub questions: Vec<String>,
    pub conflicts: Vec<String>,
    pub sources: Vec<SourceRef>,
}

impl TryFrom<&str> for PlanCreate {
    type Error = Error;

    fn try_from(candidate: &str) -> Result<Self, Self::Error> {
        serde_json::from_str(candidate).map_err(Error::from)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanUpdate {
    pub scope: Vec<SectionKey>,
    pub sections: BTreeMap<SectionKey, String>,
    pub requirements: Vec<ProposedRequirement>,
    pub proposals: Vec<String>,
    pub questions: Vec<String>,
    pub conflicts: Vec<String>,
}

impl TryFrom<&str> for PlanUpdate {
    type Error = Error;

    fn try_from(candidate: &str) -> Result<Self, Self::Error> {
        serde_json::from_str(candidate).map_err(Error::from)
    }
}

pub fn first_document(
    create: PlanCreate,
    human_requirements: Vec<HumanRequirement>,
) -> Result<PlanDocument, Error> {
    let mut seen = BTreeSet::new();
    let mut requirements = Vec::with_capacity(
        human_requirements
            .len()
            .saturating_add(create.requirements.len()),
    );
    for requirement in human_requirements {
        remember_requirement(&mut seen, &requirement.id)?;
        requirements.push(Requirement {
            id: requirement.id,
            text: requirement.text,
            acceptance: requirement.acceptance,
            origin: Origin::Human,
            status: Status::Agreed,
        });
    }
    for requirement in create.requirements {
        remember_requirement(&mut seen, &requirement.id)?;
        requirements.push(proposal(requirement));
    }

    Ok(PlanDocument {
        goal: create.goal,
        in_scope: create.in_scope,
        out_of_scope: create.out_of_scope,
        requirements,
        acceptance: create.acceptance,
        decisions: create.decisions,
        sections: create.sections,
        proposals: create.proposals,
        assumptions: create.assumptions,
        questions: create.questions,
        conflicts: create.conflicts,
        sources: create.sources,
    })
}

pub fn updated_document(parent: &PlanDocument, update: PlanUpdate) -> Result<PlanDocument, Error> {
    let scope: BTreeSet<&SectionKey> = update.scope.iter().collect();
    if update.sections.keys().any(|key| !scope.contains(key)) {
        return Err(Error::OutOfScope);
    }

    let mut seen = BTreeSet::new();
    for requirement in &update.requirements {
        remember_requirement(&mut seen, &requirement.id)?;
        if let Some(existing) = parent.requirement(&requirement.id) {
            return if existing.origin == Origin::Human {
                Err(Error::WouldWeakenHumanRequirement)
            } else {
                Err(Error::RepeatedRequirement)
            };
        }
    }

    let mut document = parent.clone();
    // 2026-09-08 — `sections` jest patchem, nie drugim dokumentem. Brak klucza znaczy
    // „nie dotykaj”, dzięki czemu ograniczona edycja nie musi przepisywać cudzych bajtów.
    for (key, text) in update.sections {
        document.sections.insert(key, text);
    }
    document
        .requirements
        .extend(update.requirements.into_iter().map(proposal));
    document.proposals.extend(update.proposals);
    document.questions.extend(update.questions);
    document.conflicts.extend(update.conflicts);
    Ok(document)
}

fn proposal(requirement: ProposedRequirement) -> Requirement {
    Requirement {
        id: requirement.id,
        text: requirement.text,
        acceptance: requirement.acceptance,
        origin: Origin::Generated,
        status: Status::Proposed,
    }
}

fn remember_requirement(seen: &mut BTreeSet<String>, id: &str) -> Result<(), Error> {
    let digits = id.strip_prefix('R').unwrap_or_default();
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::Malformed(format!(
            "requirement identifier {id:?} must be R followed by digits"
        )));
    }
    if !seen.insert(id.to_owned()) {
        return Err(Error::RepeatedRequirement);
    }
    Ok(())
}
