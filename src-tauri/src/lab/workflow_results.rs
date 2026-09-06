//! WF-18: nie mylimy błędu zadania z niewykonanym lub uszkodzonym pomiarem.

use super::{
    EvalSet,
    results::{CellExecution, CellResult, Finished, Outcome, Scored},
    workflow_plan::CellBinding,
};
use crate::workflow::execution::EndCause;
use std::collections::{BTreeMap, BTreeSet};

#[must_use]
pub fn score(set: &EvalSet, bindings: &[CellBinding], finished: &[Finished]) -> Scored {
    let by_key: BTreeMap<_, _> = finished
        .iter()
        .map(|one| (one.tile.as_str(), one))
        .collect();
    let mut cells = Vec::new();
    for case in set.running_cases() {
        for variant in &set.variants {
            for repeat in 0..case.repeats().unwrap_or(1) {
                let mut found = bindings.iter().filter(|one| {
                    one.case == case.id && one.variant == variant.id && one.repeat == repeat
                });
                let binding = found.next().filter(|_| found.next().is_none());
                cells.push(judge(&case.id, &variant.id, repeat, binding, &by_key));
            }
        }
    }
    Scored {
        passed: cells
            .iter()
            .filter(|one| one.outcome == Outcome::Passed)
            .count(),
        judged: cells
            .iter()
            .filter(|one| one.outcome != Outcome::NotJudged)
            .count(),
        cost_usd: cells
            .iter()
            .filter_map(|one| one.cost_usd)
            .reduce(|a, b| a + b),
        cells,
    }
}

fn judge(
    case: &str,
    variant: &str,
    repeat: usize,
    binding: Option<&CellBinding>,
    steps: &BTreeMap<&str, &Finished>,
) -> CellResult {
    let mut cell = CellResult {
        case: case.to_owned(),
        variant: variant.to_owned(),
        outcome: Outcome::NotJudged,
        said: "The saved mapping for this repeat is unavailable. This was not measured.".to_owned(),
        cost_usd: None,
        execution: None,
    };
    let Some(binding) = binding else {
        return cell;
    };
    let keys: BTreeSet<_> = binding
        .nodes
        .values()
        .chain(std::iter::once(&binding.grader))
        .collect();
    let executed: Vec<_> = keys
        .iter()
        .filter_map(|key| steps.get(key.as_str()).copied())
        .filter(|one| one.executed)
        .collect();
    cell.cost_usd = executed
        .iter()
        .filter_map(|one| one.cost_usd)
        .reduce(|a, b| a + b);
    cell.execution = Some(CellExecution {
        repeat,
        nodes: keys.into_iter().cloned().collect(),
        output: binding.output.clone(),
        grader: binding.grader.clone(),
        elapsed_ms: executed
            .iter()
            .filter_map(|one| one.started_at.zip(one.ended_at))
            .filter_map(|(start, end)| u64::try_from(end.saturating_sub(start)).ok())
            .reduce(u64::saturating_add),
        cost_partial: executed
            .iter()
            .any(|one| one.kind == "agent" && one.cost_usd.is_none()),
    });
    let output = steps.get(binding.output.as_str()).copied();
    let grader = steps.get(binding.grader.as_str()).copied();
    let (outcome, said) = verdict(output, grader, &executed);
    cell.outcome = outcome;
    cell.said = said;
    cell
}

fn verdict(
    output: Option<&Finished>,
    grader: Option<&Finished>,
    executed: &[&Finished],
) -> (Outcome, String) {
    // Skończony output jest dowodem, że wcześniejsza nieudana runda/nieaktywna gałąź
    // nie zatrzymała tej komórki. Nie obniżamy wyniku za takie naprawione próby.
    if let Some(output) = output
        && output.executed
        && output.state == "succeeded"
        && output.cause == Some(EndCause::Completed)
    {
        return match grader {
            Some(one)
                if one.executed
                    && one.state == "succeeded"
                    && one.cause == Some(EndCause::Completed) =>
            {
                (Outcome::Passed, String::new())
            }
            Some(one) if one.executed && one.cause == Some(EndCause::TaskFailed) => (
                Outcome::DidNotPass,
                reason(
                    one,
                    "The independent checks found that the result did not meet the case.",
                ),
            ),
            Some(one) => (
                Outcome::NotJudged,
                reason(
                    one,
                    "The independent checks did not complete. This was not measured.",
                ),
            ),
            None => (
                Outcome::NotJudged,
                "The independent check result is missing. This was not measured.".to_owned(),
            ),
        };
    }
    if let Some(one) = executed.iter().copied().find(|one| {
        matches!(
            one.cause,
            Some(
                EndCause::InfrastructureFailed
                    | EndCause::Cancelled
                    | EndCause::UnprovenStop
                    | EndCause::Refused
                    | EndCause::LimitReached
            )
        ) || one.state == "failed" && matches!(one.cause, None | Some(EndCause::Unknown))
    }) {
        return (
            Outcome::NotJudged,
            reason(
                one,
                "The work could not be measured because execution did not complete.",
            ),
        );
    }
    if let Some(one) = executed
        .iter()
        .copied()
        .find(|one| one.cause == Some(EndCause::TaskFailed))
    {
        return (
            Outcome::DidNotPass,
            reason(
                one,
                "The workflow ran, but did not produce the required result.",
            ),
        );
    }
    (
        Outcome::NotJudged,
        output.map_or_else(
            || "The workflow did not produce a result to measure.".to_owned(),
            |one| reason(one, "The workflow did not finish a result to measure."),
        ),
    )
}

fn reason(one: &Finished, fallback: &str) -> String {
    if one.error.trim().is_empty() {
        fallback.to_owned()
    } else {
        one.error.clone()
    }
}
