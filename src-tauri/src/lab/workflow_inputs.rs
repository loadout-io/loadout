//! Typowane referencje subjectu są remapowane, nigdy wyszukiwane w prozie lub vendorOptions.
use super::{Case, EvalSet};
use crate::workflow::{
    WorkflowFile,
    execution::{ContextScope, RunInputs, WorkspaceSeed},
};
use std::collections::{BTreeMap, BTreeSet};

/// Jedna decyzja dla planera i podglądu; nie jest skanerem plików ani drugim snapshotem.
pub(super) fn shared_additional_inputs(
    selected: &[WorkflowFile],
    set: &EvalSet,
) -> Result<Vec<String>, String> {
    let protected = protection_required(set)?;
    let saved_for_every_case = set.running_cases().iter().try_fold(true, |all, case| {
        Ok::<_, String>(case_seed(case)?.is_some() && all)
    })?;
    let mut common: Option<BTreeSet<String>> = None;
    for source in selected {
        if RunInputs::from_graph(source)?.isolate_contexts && !protected {
            return Err(format!(
                "The workflow {} requires restricted file access. Enable protection for this comparison before running it.",
                source.name
            ));
        }
        let patterns: BTreeSet<_> = source.additional_inputs()?.into_iter().collect();
        // 2026-09-06: case seed ma własne wybrane bajty. Przechwycenie hosta jeszcze raz
        // psuło replay po usunięciu pliku, a union globów udzielał dostępu innej kolumnie.
        if saved_for_every_case {
            continue;
        }
        if common.as_ref().is_some_and(|chosen| chosen != &patterns) {
            return Err("These workflows select different additional starting files. Choose a shared saved input for every case, or make the workflows' selected starting files identical. Their file selections were not combined.".to_owned());
        }
        common = Some(patterns);
    }
    Ok(common.unwrap_or_default().into_iter().collect())
}

pub(super) fn protection_required(set: &EvalSet) -> Result<bool, String> {
    match set.extra.get("protected") {
        None => Ok(false),
        Some(serde_json::Value::Bool(value)) => Ok(*value),
        Some(_) => Err("Choose whether this comparison uses protected file access.".to_owned()),
    }
}

/// Odmowy tej samej definicji oceny obowiązują podgląd i compose, nie tylko płatny Start.
pub(super) fn validate_cases(set: &EvalSet) -> Result<(), String> {
    let protected = protection_required(set)?;
    for case in set.running_cases() {
        if protected {
            if case
                .extra
                .get("proofMode")
                .and_then(serde_json::Value::as_str)
                != Some("external-assessment-v1")
            {
                return Err("Protected comparisons need the independent examiner result, not an output pattern.".to_owned());
            }
            let examiner = case.extra.get("examiner").ok_or_else(|| {
                "Protected comparisons need accepted independent examiner source code.".to_owned()
            })?;
            crate::workflow::execution::Examiner::read(examiner)?;
        }
        let examiner = crate::workflow::execution::Examiner::from_check(&case.extra)?;
        if examiner.is_some() && !protected {
            return Err("Frozen independent examiners require protected file access. Enable protection before starting this workflow.".to_owned());
        }
        if examiner.is_none() && (case.command.trim().is_empty() || case.proof.trim().is_empty()) {
            return Err("A workflow comparison needs an independent command and a positive pass-count proof.".to_owned());
        }
    }
    Ok(())
}

fn case_seed(case: &Case) -> Result<Option<WorkspaceSeed>, String> {
    case.extra
        .get("input")
        .map(|value| {
            serde_json::from_value::<WorkspaceSeed>(value.clone()).map_err(|_| {
                "A case input needs the exact saved run ID and input snapshot ID.".to_owned()
            })
        })
        .transpose()
}

pub(super) fn for_cell(
    graph: &WorkflowFile,
    case: &Case,
    scope: &str,
    names: &BTreeMap<String, String>,
    physical: &BTreeMap<String, String>,
) -> Result<RunInputs, String> {
    let original = RunInputs::from_graph(graph)?;
    let mut inputs = RunInputs::default();
    let seed_names: BTreeMap<_, _> = original
        .workspace_seeds
        .keys()
        .enumerate()
        .map(|(at, key)| (key.clone(), format!("{scope}_seed_{at}")))
        .collect();
    for (key, seed) in &original.workspace_seeds {
        inputs
            .workspace_seeds
            .insert(seed_names[key].clone(), seed.clone());
    }
    let case_seed = case_seed(case)?;
    let case_seed = case_seed.map(|seed| {
        let key = format!("{scope}_case_input");
        inputs.workspace_seeds.insert(key.clone(), seed);
        key
    });
    inputs.contexts.insert(
        scope.into(),
        ContextScope {
            task: case.task.clone(),
            workspace_seed: case_seed.clone(),
            instructions: String::new(),
            project_instructions: false,
            project_memory: false,
        },
    );
    let scope_names: BTreeMap<_, _> = original
        .contexts
        .keys()
        .enumerate()
        .map(|(at, key)| (key.clone(), format!("{scope}_context_{at}")))
        .collect();
    for (key, context) in &original.contexts {
        let mut context = context.clone();
        // Zadanie jest parametrem przypadku; jawne instrukcje i zakresy źródła pozostają.
        context.task.clone_from(&case.task);
        context.workspace_seed = case_seed.clone().or_else(|| {
            context
                .workspace_seed
                .as_ref()
                .and_then(|key| seed_names.get(key))
                .cloned()
        });
        inputs.contexts.insert(scope_names[key].clone(), context);
    }
    for (original_key, new_key) in names {
        let context = original
            .step_contexts
            .get(original_key)
            .and_then(|key| scope_names.get(key));
        inputs.step_contexts.insert(
            new_key.clone(),
            context.cloned().unwrap_or_else(|| scope.into()),
        );
    }
    for (key, check) in &original.checks {
        let new_key = physical
            .get(key)
            .ok_or_else(|| format!("The source check {key} has no exact executed step."))?;
        let results = check
            .results
            .iter()
            .map(|key| {
                physical.get(key).cloned().ok_or_else(|| {
                    format!("The source check input {key} has no exact executed step.")
                })
            })
            .collect::<Result<_, String>>()?;
        inputs.checks.insert(
            new_key.clone(),
            crate::workflow::execution::CheckInput { results },
        );
    }
    Ok(inputs)
}
