//! Widok przypięć dla edytora i preflight tych samych przypięć przed Startem.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Serialize;

use crate::context::build;
use crate::context::files;
use crate::context::{ContextRevision, ContextSet, ContextTopic};
use crate::workflow::context::{ContextPin, Topics, effective_for};
use crate::workflow::execution::RunInputs;
use crate::workflow::{Step, WorkflowFile};

#[derive(Debug, PartialEq, Eq)]
pub struct ContextRefusal {
    pub step_id: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PinSource {
    Workflow,
    Step,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextChoice {
    pub id: String,
    pub title: String,
    pub description: String,
    pub revision: Option<String>,
    pub topics: Vec<ContextTopic>,
    pub said: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedContext {
    pub id: String,
    pub title: String,
    pub revision: String,
    pub selected_topics: Topics,
    pub topics: Vec<ContextTopic>,
    pub source: PinSource,
    pub update: Option<String>,
    pub said: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OmittedContext {
    pub id: String,
    pub title: String,
    pub said: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepContextView {
    pub step_id: String,
    pub sets: Vec<SelectedContext>,
    pub omitted: Vec<OmittedContext>,
    pub inherits_workflow: bool,
    pub protected_scope: bool,
    pub said: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowContextView {
    pub catalog: Vec<ContextChoice>,
    pub workflow: Vec<SelectedContext>,
    pub steps: Vec<StepContextView>,
    pub warnings: Vec<String>,
}

/// Wszystko, czego oba pickery potrzebują z biblioteki, bez zmiany dokumentu wejściowego.
pub fn resolve_workflow_context_inner(
    home: &Path,
    file: &WorkflowFile,
) -> Result<WorkflowContextView, String> {
    crate::workflow::context::validate(file)?;
    let inputs = RunInputs::from_graph(file)?;
    let library = files::library_root(home);
    let sets = files::list_sets(&library).map_err(|error| error.to_string())?;
    let by_id = sets
        .iter()
        .map(|set| (set.id.as_str(), set))
        .collect::<BTreeMap<_, _>>();
    let mut warnings = BTreeSet::new();
    let catalog = catalog_for(&library, &sets, &by_id, file)?;
    let workflow = file.context()?.map_or_else(Vec::new, |context| {
        context
            .sets
            .iter()
            .map(|pin| inspect_pin(&library, &by_id, pin, PinSource::Workflow, &mut warnings))
            .collect()
    });
    let steps = step_views(&library, &by_id, file, &inputs, &mut warnings)?;
    Ok(WorkflowContextView {
        catalog,
        workflow,
        steps,
        warnings: warnings.into_iter().collect(),
    })
}

fn catalog_for(
    library: &Path,
    sets: &[ContextSet],
    by_id: &BTreeMap<&str, &ContextSet>,
    file: &WorkflowFile,
) -> Result<Vec<ContextChoice>, String> {
    let mut pinned = file
        .context()?
        .map(|context| {
            context
                .sets
                .into_iter()
                .map(|pin| (pin.id, pin.revision))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    for step in &file.steps {
        if let Step::Agent(agent) = step
            && let Some(context) = agent.context()?
        {
            for pin in context.sets {
                pinned.entry(pin.id).or_insert(pin.revision);
            }
        }
    }
    let mut catalog = sets
        .iter()
        // 2026-09-08 (CT-05): archiwum znika z nowych wyborów, ale istniejące przypięcie musi
        // zostać widoczne i usuwalne; ukrycie go zamieniłoby „preserve pins” w ślepy zapis.
        .filter(|set| !set.archived || pinned.contains_key(&set.id))
        .map(|set| catalog_choice(library, set))
        .collect::<Vec<_>>();
    for (id, revision) in &pinned {
        if by_id.contains_key(id.as_str()) {
            continue;
        }
        catalog.push(ContextChoice {
            id: id.clone(),
            title: id.clone(),
            description: String::new(),
            revision: Some(revision.clone()),
            topics: Vec::new(),
            said: Some(
                "This context set is not available now. Remove it or restore the set before starting."
                    .to_owned(),
            ),
        });
    }
    Ok(catalog)
}

fn step_views(
    library: &Path,
    by_id: &BTreeMap<&str, &ContextSet>,
    file: &WorkflowFile,
    inputs: &RunInputs,
    warnings: &mut BTreeSet<String>,
) -> Result<Vec<StepContextView>, String> {
    let mut steps = Vec::new();
    for step in &file.steps {
        let Step::Agent(agent) = step else {
            continue;
        };
        let local = agent.context()?;
        let local_ids = local
            .as_ref()
            .map(|context| {
                context
                    .sets
                    .iter()
                    .map(|pin| pin.id.as_str())
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        let effective = effective_for(file, &agent.id, inputs)?;
        let selected = effective
            .sets
            .iter()
            .map(|pin| {
                let source = if local_ids.contains(pin.id.as_str()) {
                    PinSource::Step
                } else {
                    PinSource::Workflow
                };
                inspect_pin(library, by_id, pin, source, warnings)
            })
            .collect();
        let omitted = omitted_workflow_sets(file, by_id, local.as_ref(), &effective);
        let said = why_inheritance_differs(local.as_ref(), &effective);
        steps.push(StepContextView {
            step_id: agent.id.clone(),
            sets: selected,
            omitted,
            inherits_workflow: effective.inherits_workflow,
            protected_scope: effective.protected_scope,
            said,
        });
    }
    Ok(steps)
}

fn catalog_choice(library: &Path, set: &ContextSet) -> ContextChoice {
    let (revision, topics, said) = match set.latest_ready_revision.as_deref() {
        None => (
            None,
            Vec::new(),
            Some("Build this context before adding it to a workflow.".to_owned()),
        ),
        Some(id) => match build::read_revision(library, &set.id, id) {
            Ok(revision) => (Some(id.to_owned()), revision.topics, None),
            Err(_) => (
                Some(id.to_owned()),
                Vec::new(),
                Some(
                    "This ready version cannot be read. Rebuild the context before using it."
                        .to_owned(),
                ),
            ),
        },
    };
    ContextChoice {
        id: set.id.clone(),
        title: set.title.clone(),
        description: set.description.clone(),
        revision,
        topics,
        said,
    }
}

fn inspect_pin(
    library: &Path,
    by_id: &BTreeMap<&str, &ContextSet>,
    pin: &ContextPin,
    source: PinSource,
    warnings: &mut BTreeSet<String>,
) -> SelectedContext {
    let title = by_id
        .get(pin.id.as_str())
        .map_or_else(|| pin.id.clone(), |set| set.title.clone());
    let update = by_id
        .get(pin.id.as_str())
        .and_then(|set| set.latest_ready_revision.as_deref())
        .filter(|latest| *latest != pin.revision)
        .map(|_| "Update available".to_owned());
    let (topics, said) = if let Ok(revision) = build::read_revision(library, &pin.id, &pin.revision)
    {
        let missing = missing_topics(pin, &revision);
        if missing.is_empty() {
            (revision.topics, None)
        } else {
            let said = format!(
                "Context set {} no longer contains every chosen topic. Choose its topics again before starting.",
                pin.id
            );
            warnings.insert(said.clone());
            (revision.topics, Some(said))
        }
    } else {
        let said = format!(
            "Context set {} version {} is not available now. This draft can still be saved; open Context and choose or build a ready version before starting.",
            pin.id, pin.revision
        );
        warnings.insert(said.clone());
        (Vec::new(), Some(said))
    };
    SelectedContext {
        id: pin.id.clone(),
        title,
        revision: pin.revision.clone(),
        selected_topics: pin.topics.clone(),
        topics,
        source,
        update,
        said,
    }
}

fn missing_topics(pin: &ContextPin, revision: &ContextRevision) -> Vec<String> {
    let Topics::Only(selected) = &pin.topics else {
        return Vec::new();
    };
    let existing = revision
        .topics
        .iter()
        .map(|topic| topic.id.as_str())
        .collect::<BTreeSet<_>>();
    selected
        .iter()
        .filter(|topic| !existing.contains(topic.as_str()))
        .cloned()
        .collect()
}

fn omitted_workflow_sets(
    file: &WorkflowFile,
    by_id: &BTreeMap<&str, &ContextSet>,
    local: Option<&crate::workflow::context::StepContext>,
    effective: &crate::workflow::context::EffectiveContext,
) -> Vec<OmittedContext> {
    let selected = effective
        .sets
        .iter()
        .map(|pin| pin.id.as_str())
        .collect::<BTreeSet<_>>();
    file.context()
        .ok()
        .flatten()
        .map_or_else(Vec::new, |context| {
            context
                .sets
                .into_iter()
                .filter(|pin| !selected.contains(pin.id.as_str()))
                .map(|pin| {
                    let title = by_id
                        .get(pin.id.as_str())
                        .map_or_else(|| pin.id.clone(), |set| set.title.clone());
                    let reason = if local.is_some_and(|one| one.exclude.contains(&pin.id)) {
                        "it was excluded here"
                    } else if effective.protected_scope {
                        "protected steps require an explicit choice"
                    } else {
                        "workflow context is turned off here"
                    };
                    OmittedContext {
                        id: pin.id,
                        title,
                        said: format!("Not used in this step — {reason}."),
                    }
                })
                .collect()
        })
}

fn why_inheritance_differs(
    local: Option<&crate::workflow::context::StepContext>,
    effective: &crate::workflow::context::EffectiveContext,
) -> Option<String> {
    if effective.inherits_workflow {
        return None;
    }
    if effective.protected_scope && local.and_then(|one| one.inherit_workflow).is_none() {
        Some(
            "Workflow context is off because this step uses a protected context. Turn it on here to include shared sets."
                .to_owned(),
        )
    } else {
        Some("Workflow context is turned off for this step.".to_owned())
    }
}

/// Odmowa przed pierwszym procesem, ograniczona do kroków, które naprawdę mają ruszyć.
pub fn context_ready_to_start(
    home: &Path,
    file: &WorkflowFile,
    included_steps: &BTreeSet<&str>,
) -> Result<(), ContextRefusal> {
    let inputs = RunInputs::from_graph(file).map_err(|message| ContextRefusal {
        step_id: String::new(),
        message,
    })?;
    let library = files::library_root(home);
    for step in &file.steps {
        let Step::Agent(agent) = step else {
            continue;
        };
        if !included_steps.contains(agent.id.as_str()) {
            continue;
        }
        let effective =
            effective_for(file, &agent.id, &inputs).map_err(|message| ContextRefusal {
                step_id: agent.id.clone(),
                message,
            })?;
        if effective.sets.len() > crate::context::limits::STEP_CONTEXT_SETS {
            return Err(ContextRefusal {
                step_id: agent.id.clone(),
                message: format!(
                    "{} cannot start because it would receive more than eight context sets. Remove a set from this step.",
                    agent.name
                ),
            });
        }
        for pin in &effective.sets {
            let revision = build::read_revision(&library, &pin.id, &pin.revision).map_err(|_| {
                ContextRefusal {
                    step_id: agent.id.clone(),
                    message: format!(
                        "{} cannot start because context set {} version {} is missing, damaged or not ready. Open Context and rebuild it or choose another ready version.",
                        agent.name, pin.id, pin.revision
                    ),
                }
            })?;
            if !missing_topics(pin, &revision).is_empty() {
                return Err(ContextRefusal {
                    step_id: agent.id.clone(),
                    message: format!(
                        "{} cannot start because context set {} no longer contains every chosen topic. Open Context and choose its topics again.",
                        agent.name, pin.id
                    ),
                });
            }
        }
    }
    Ok(())
}
