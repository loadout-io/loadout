//! Kryteria planu są wejściem istniejącego sędziego, przypiętym do wersji i wyniku pracy.
//!
//! 2026-09-08 (WP-05) — ten moduł trzyma politykę razem, żeby cienkie szwy biegu nie
//! stworzyły drugiej reguły wyniku obok `workflow::criteria`.

use std::collections::BTreeMap;

use crate::workflow::criteria::{Criterion, Judgement, Method, Outcome};

use super::{Origin, PlanVersion, Status};

/// Dokładna podstawa, którą krok sprawdzający dostał przed uruchomieniem procesu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Basis {
    pub version_id: String,
    pub work: BTreeMap<String, String>,
}

impl Basis {
    /// Typowy sędzia sprawdza jedną kopię; ten odcisk trafia do jej trwałego paragonu.
    #[must_use]
    pub fn one_work_digest(&self) -> Option<&str> {
        (self.work.len() == 1)
            .then(|| self.work.values().next())
            .flatten()
            .map(String::as_str)
    }
}

/// Krótki stan otwartych kryteriów z poprzedniej próby, nigdy kopia starego raportu.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StillOpen {
    items: BTreeMap<String, String>,
}

impl StillOpen {
    #[must_use]
    pub fn from_judgement(judged: &Judgement) -> Self {
        let mut items = BTreeMap::new();
        remember(&mut items, &judged.missing, "was not answered");
        remember(&mut items, &judged.failed, "was found not met");
        remember(&mut items, &judged.not_tested, "could not be measured");
        remember(
            &mut items,
            &judged.weaker_method,
            "was confirmed a weaker way than agreed",
        );
        remember(
            &mut items,
            &judged.duplicated,
            "was answered more than once",
        );
        Self { items }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

fn remember(items: &mut BTreeMap<String, String>, ids: &[String], state: &str) {
    for id in ids {
        items.entry(id.clone()).or_insert_with(|| state.to_owned());
    }
}

/// Zamraża wyłącznie wymagania naprawdę zatwierdzone przez człowieka w tej wersji.
#[must_use]
pub fn frozen_criteria(version: &PlanVersion) -> Vec<Criterion> {
    version
        .document
        .requirements
        .iter()
        .filter(|requirement| {
            requirement.origin == Origin::Human && requirement.status == Status::Agreed
        })
        .map(|requirement| Criterion {
            id: requirement.id.clone(),
            behaviour: requirement.text.clone(),
            required: true,
            method: requirement
                .acceptance
                .first()
                .map_or(Method::Unknown, |acceptance| {
                    Method::from_said(&acceptance.verification)
                }),
        })
        .collect()
}

/// Konflikt jest widoczny z obu stron; plan nie może przykryć listy zatwierdzonej w workflow.
#[must_use]
pub fn conflicts(workflow: &[Criterion], from_plan: &[Criterion]) -> Vec<String> {
    let mut found = Vec::new();
    for agreed in workflow {
        let Some(planned) = from_plan.iter().find(|planned| planned.id == agreed.id) else {
            continue;
        };
        if agreed.behaviour == planned.behaviour && agreed.method == planned.method {
            continue;
        }
        found.push(format!(
            "Requirement {} differs between the approved workflow list and the plan. The approved workflow says \"{}\" and asks for {}; the plan says \"{}\" and asks for {}. The approved workflow requirement is the one this step checks.",
            agreed.id,
            agreed.behaviour,
            agreed.method.said(),
            planned.behaviour,
            planned.method.said(),
        ));
    }
    found
}

/// Jedna lista dla istniejącego `criteria::judge`; zatwierdzony workflow wygrywa kolizję ID.
#[must_use]
pub fn approved(workflow: &[Criterion], from_plan: &[Criterion]) -> Vec<Criterion> {
    let mut result = workflow.to_vec();
    result.extend(
        from_plan
            .iter()
            .filter(|planned| !workflow.iter().any(|agreed| agreed.id == planned.id))
            .cloned(),
    );
    result
}

/// Zdanie dla człowieka, gdy zmieniła się wyłącznie wersja, wyłącznie praca albo obie osie.
#[must_use]
pub fn about_other_work(frozen: &Basis, now: &Basis) -> Option<String> {
    match (frozen.version_id != now.version_id, frozen.work != now.work) {
        (false, false) => None,
        (true, false) => Some(format!(
            "The tester answered about plan version {} instead of the pinned version {}.",
            now.version_id, frozen.version_id
        )),
        (false, true) => Some(
            "The tester answered about different work than the work it was given to check."
                .to_owned(),
        ),
        (true, true) => Some(format!(
            "The tester answered about different work under plan version {} instead of the pinned version {}.",
            now.version_id, frozen.version_id
        )),
    }
}

/// Meldunek o innym produkcie nie opisuje wady ani zaliczenia tego produktu.
pub fn not_this_product(approved: &[Criterion], judged: &mut Judgement) {
    let required = approved
        .iter()
        .filter(|criterion| criterion.required)
        .map(|criterion| criterion.id.clone())
        .collect::<Vec<_>>();
    judged.missing.retain(|id| !required.contains(id));
    judged.failed.retain(|id| !required.contains(id));
    judged.weaker_method.retain(|id| !required.contains(id));
    judged.not_tested.extend(required);
    judged.not_tested.sort();
    judged.not_tested.dedup();
    judged.outcome = Outcome::NotJudged;
}

/// Milczenie może zachować starą uwagę jako brak odpowiedzi, ale nigdy nie cofa nowego `pass`.
#[must_use]
pub fn carry(previous: &StillOpen, judged: &mut Judgement) -> Option<String> {
    let mut silent = previous
        .items
        .keys()
        .filter(|id| judged.missing.contains(id))
        .cloned()
        .collect::<Vec<_>>();
    if silent.is_empty() {
        return None;
    }
    silent.sort();
    silent.dedup();
    judged.missing.sort();
    judged.missing.dedup();
    let count = silent.len();
    Some(format!(
        "The tester left {count} requirement(s) open from the previous try by saying nothing about them this time ({}).",
        silent.join(", ")
    ))
}

/// Następna próba dostaje ID i bieżący stan, bez narastających kopii szczegółowych raportów.
#[must_use]
pub fn told_what_is_still_open(previous: &StillOpen) -> String {
    if previous.is_empty() {
        return String::new();
    }
    let mut told = String::from(
        "These requirements were still open after the previous try. Answer each by its ID; silence does not close it:\n",
    );
    for (id, state) in &previous.items {
        told.push_str("\n- ");
        told.push_str(id);
        told.push_str(": it ");
        told.push_str(state);
        told.push('.');
    }
    told
}
