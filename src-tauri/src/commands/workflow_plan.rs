//! Widok ustawienia Plan i cienka brama Startu nad jednym resolverem grafu.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::work_plan::Mode;
use crate::workflow::WorkflowFile;
use crate::workflow::check::node_key_for;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanSource {
    pub step_id: String,
    pub name: String,
    pub said: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepPlanView {
    pub step_id: String,
    pub mode: Mode,
    pub source: Option<PlanSource>,
    pub earlier: Vec<PlanSource>,
    pub said: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowPlanView {
    pub steps: Vec<StepPlanView>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanRefusal {
    pub step_id: String,
    pub message: String,
}

/// Panel dostaje nazwane kroki, ale żadnego zmyślonego numeru przyszłej wersji.
#[must_use]
pub fn resolve_workflow_plan_inner(file: &WorkflowFile) -> WorkflowPlanView {
    let resolved = crate::workflow::work_plan::resolution_for_panel(file);
    let names = file
        .steps
        .iter()
        .map(|step| (step.id(), step.name()))
        .collect::<BTreeMap<_, _>>();
    let steps = resolved
        .steps
        .iter()
        .map(|step| {
            let source = step.source_step_ids.first().and_then(|id| {
                names.get(id.as_str()).map(|name| PlanSource {
                    step_id: id.clone(),
                    name: (*name).to_owned(),
                    said: format!("Takes the plan from {name}."),
                })
            });
            let earlier = step
                .earlier_step_ids
                .iter()
                .filter_map(|id| {
                    names.get(id.as_str()).map(|name| PlanSource {
                        step_id: id.clone(),
                        name: (*name).to_owned(),
                        said: format!("Take the same plan as {name}."),
                    })
                })
                .collect();
            let said = resolved
                .notes
                .iter()
                .find(|note| note.step_id.as_deref() == Some(step.step_id.as_str()))
                .map(|note| note.message.clone());
            StepPlanView {
                step_id: step.step_id.clone(),
                mode: step.mode,
                source,
                earlier,
                said,
            }
        })
        .collect();
    let warnings = resolved
        .notes
        .iter()
        .filter(|note| note.step_id.is_none())
        .map(|note| note.message.clone())
        .collect();
    WorkflowPlanView { steps, warnings }
}

/// Odmowa przed pierwszym agentem oraz mapa, którą wykonanie niesie bez ponownego obchodu.
pub fn plan_ready_to_start(
    file: &WorkflowFile,
    included_steps: &BTreeSet<&str>,
) -> Result<crate::workflow::work_plan::AuthorMap, PlanRefusal> {
    let mut authors = crate::workflow::work_plan::authors(file).map_err(|note| PlanRefusal {
        step_id: note.step_id.unwrap_or_default(),
        message: note.message,
    })?;
    let graph = crate::workflow::unroll::unroll(file);
    let physical = graph
        .nodes
        .iter()
        .map(|node| {
            let step = &file.steps[node.step];
            (
                node_key_for(step.id(), node.turn, node.copy),
                (step.id(), step.name()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for (consumer, sources) in &authors.sources {
        let Some((consumer_id, consumer_name)) = physical.get(consumer) else {
            continue;
        };
        if !included_steps.contains(*consumer_id) {
            continue;
        }
        if sources.iter().any(|source| {
            physical
                .get(source)
                .is_some_and(|(source_id, _)| !included_steps.contains(*source_id))
        }) {
            return Err(PlanRefusal {
                step_id: (*consumer_id).to_owned(),
                message: format!(
                    "{consumer_name} cannot start because the earlier step that provides its plan is outside this run. Include that step too."
                ),
            });
        }
    }
    authors.sources.retain(|consumer, _| {
        physical
            .get(consumer)
            .is_some_and(|(consumer_id, _)| included_steps.contains(*consumer_id))
    });
    Ok(authors)
}
