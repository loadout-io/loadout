//! Przypięcia gotowych wersji Context do workflow i kroków agentów.
//!
//! Resolver jest jeden, bo zapis, podgląd i Start muszą rozumieć `exclude`, chroniony zakres
//! i lokalne zawężenie identycznie. Druga implementacja zrobiłaby picker pokazujący inne wejście
//! niż to, które preflight za chwilę dopuści.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use super::execution::RunInputs;
use super::{AgentStep, Step, WorkflowFile};

/// Wszystkie tematy wersji albo dokładne, niezmienne identyfikatory wybrane przez człowieka.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Topics {
    All,
    Only(Vec<String>),
}

impl Serialize for Topics {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::All => serializer.serialize_str("all"),
            Self::Only(topics) => topics.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Topics {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        match value {
            Value::String(one) if one == "all" => Ok(Self::All),
            Value::Array(_) => serde_json::from_value(value)
                .map(Self::Only)
                .map_err(serde::de::Error::custom),
            _ => Err(serde::de::Error::custom(
                "topics must be \"all\" or a list of topic identifiers",
            )),
        }
    }
}

/// Jedno przypięcie wskazuje wersję dokładnie, nigdy `latest`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextPin {
    pub id: String,
    pub revision: String,
    pub topics: Topics,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowContext {
    pub schema: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sets: Vec<ContextPin>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StepContext {
    pub schema: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherit_workflow: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sets: Vec<ContextPin>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveContext {
    pub sets: Vec<ContextPin>,
    pub inherits_workflow: bool,
    pub protected_scope: bool,
    #[serde(skip)]
    pub(crate) excluded_workflow: BTreeSet<String>,
}

pub(super) fn workflow_from(value: Option<&Value>) -> Result<Option<WorkflowContext>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let parsed: WorkflowContext = serde_json::from_value(value.clone()).map_err(|_| {
        "The workflow context selection cannot be read. Open Context and choose the sets again."
            .to_owned()
    })?;
    validate_workflow(&parsed)?;
    Ok(Some(parsed))
}

pub(super) fn step_from(value: Option<&Value>) -> Result<Option<StepContext>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let parsed: StepContext = serde_json::from_value(value.clone()).map_err(|_| {
        "This step's context selection cannot be read. Open Context and choose the sets again."
            .to_owned()
    })?;
    validate_step(&parsed)?;
    Ok(Some(parsed))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn validate_pin(pin: &ContextPin) -> Result<(), String> {
    if !valid_id(&pin.id) || !valid_id(&pin.revision) {
        return Err(
            "A context selection contains an unsupported set or version identifier. Choose the set again."
                .to_owned(),
        );
    }
    if let Topics::Only(topics) = &pin.topics {
        if topics.is_empty() {
            return Err(format!(
                "Context set {} needs at least one topic, or all topics.",
                pin.id
            ));
        }
        let mut seen = BTreeSet::new();
        for topic in topics {
            if !valid_id(topic) || !seen.insert(topic) {
                return Err(format!(
                    "Context set {} contains a repeated or unsupported topic. Choose its topics again.",
                    pin.id
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_pins(pins: &[ContextPin]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for pin in pins {
        validate_pin(pin)?;
        if !seen.insert(pin.id.as_str()) {
            return Err(format!(
                "Context set {} is selected more than once in the same list. Keep one selection.",
                pin.id
            ));
        }
    }
    Ok(())
}

fn validate_workflow(context: &WorkflowContext) -> Result<(), String> {
    if context.schema != 1 {
        return Err(
            "The workflow context selection uses a version this Loadout cannot read. Update Loadout or choose the sets again."
                .to_owned(),
        );
    }
    validate_pins(&context.sets)
}

fn validate_step(context: &StepContext) -> Result<(), String> {
    if context.schema != 1 {
        return Err(
            "This step's context selection uses a version this Loadout cannot read. Update Loadout or choose the sets again."
                .to_owned(),
        );
    }
    validate_pins(&context.sets)?;
    let mut excluded = BTreeSet::new();
    for id in &context.exclude {
        if !valid_id(id) || !excluded.insert(id) {
            return Err(
                "This step's excluded context sets contain a repeated or unsupported identifier. Choose them again."
                    .to_owned(),
            );
        }
    }
    Ok(())
}

/// Sprawdza cały dokument, łącznie z konfliktem wersji widocznym dopiero między krokami.
pub fn validate(file: &WorkflowFile) -> Result<(), String> {
    let workflow = file.context()?;
    let mut revisions = BTreeMap::<String, String>::new();
    if let Some(context) = &workflow {
        remember_revisions(&context.sets, &mut revisions)?;
    }
    for step in &file.steps {
        match step {
            Step::Agent(agent) => {
                if let Some(context) = agent.context()? {
                    remember_revisions(&context.sets, &mut revisions)?;
                }
            }
            Step::Checkpoint(one) if one.extra.contains_key("context") => {
                return Err(format!(
                    "{} cannot receive workflow context because only agent steps can use it.",
                    one.name
                ));
            }
            Step::Check(one) if one.extra.contains_key("context") => {
                return Err(format!(
                    "{} cannot receive workflow context because only agent steps can use it.",
                    one.name
                ));
            }
            Step::Serve(one) if one.extra.contains_key("context") => {
                return Err(format!(
                    "{} cannot receive workflow context because only agent steps can use it.",
                    one.name
                ));
            }
            Step::Checkpoint(_) | Step::Check(_) | Step::Serve(_) => {}
        }
    }
    Ok(())
}

pub(crate) fn remember_revisions(
    pins: &[ContextPin],
    revisions: &mut BTreeMap<String, String>,
) -> Result<(), String> {
    for pin in pins {
        if let Some(known) = revisions.insert(pin.id.clone(), pin.revision.clone())
            && known != pin.revision
        {
            return Err(format!(
                "Context set {} uses versions {} and {} in the same workflow. Choose one version before starting.",
                pin.id, known, pin.revision
            ));
        }
    }
    Ok(())
}

/// Efektywny wybór jednego kroku: dziedziczenie, wykluczenia, potem lokalne zastąpienia.
pub fn effective_for(
    file: &WorkflowFile,
    step_id: &str,
    inputs: &RunInputs,
) -> Result<EffectiveContext, String> {
    validate(file)?;
    let step = file
        .steps
        .iter()
        .find(|step| step.id() == step_id)
        .ok_or_else(|| format!("There is no step named {step_id} in this workflow."))?;
    let Step::Agent(agent) = step else {
        return Ok(EffectiveContext::default());
    };
    resolve_agent(file, agent, inputs)
}

/// Pokazuje różnice na prawdziwych strzałkach, bez zgadywania semantyki nazwy kroku.
#[must_use]
pub fn differences_between_steps(file: &WorkflowFile) -> Vec<super::check::Note> {
    let Ok(inputs) = RunInputs::from_graph(file) else {
        return Vec::new();
    };
    let agents = file
        .steps
        .iter()
        .filter_map(|step| match step {
            Step::Agent(agent) => Some((agent.id.as_str(), agent.name.as_str())),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let mut notes = Vec::new();
    for link in &file.links {
        let (Some(from_name), Some(to_name)) =
            (agents.get(link.from.as_str()), agents.get(link.to.as_str()))
        else {
            continue;
        };
        let (Ok(from), Ok(to)) = (
            effective_for(file, &link.from, &inputs),
            effective_for(file, &link.to, &inputs),
        ) else {
            continue;
        };
        let from_ids = from
            .sets
            .iter()
            .map(|pin| pin.id.as_str())
            .collect::<BTreeSet<_>>();
        let to_ids = to
            .sets
            .iter()
            .map(|pin| pin.id.as_str())
            .collect::<BTreeSet<_>>();
        let only_from = from_ids.difference(&to_ids).copied().collect::<Vec<_>>();
        let only_to = to_ids.difference(&from_ids).copied().collect::<Vec<_>>();
        if only_from.is_empty() && only_to.is_empty() {
            continue;
        }
        let mut differences = Vec::new();
        if !only_from.is_empty() {
            differences.push(format!(
                "{} has Context {} that {} does not",
                from_name,
                only_from.join(", "),
                to_name
            ));
        }
        if !only_to.is_empty() {
            differences.push(format!(
                "{} has Context {} that {} does not",
                to_name,
                only_to.join(", "),
                from_name
            ));
        }
        notes.push(super::check::Note {
            level: super::check::Level::Warning,
            step_id: Some(link.to.clone()),
            message: format!("{}.", differences.join("; ")),
            fix: None,
        });
    }
    notes
}

fn resolve_agent(
    file: &WorkflowFile,
    agent: &AgentStep,
    inputs: &RunInputs,
) -> Result<EffectiveContext, String> {
    let local = agent.context()?;
    let excluded_workflow = local
        .as_ref()
        .map(|context| context.exclude.iter().cloned().collect())
        .unwrap_or_default();
    let protected_scope = inputs.context_for(&agent.id).is_some();
    // 2026-09-08 (CT-05) — chroniony zakres nie może dostać wspólnego materiału tylko dlatego,
    // że ktoś przypiął go później do workflow; jawne `true` jest zgodą dla tego jednego kroku.
    let inherits_workflow = local
        .as_ref()
        .and_then(|context| context.inherit_workflow)
        .unwrap_or(!protected_scope);
    let mut selected = Vec::<ContextPin>::new();
    if inherits_workflow && let Some(workflow) = file.context()? {
        selected = workflow.sets;
    }
    if let Some(local) = local {
        selected.retain(|pin| !local.exclude.contains(&pin.id));
        // 2026-09-08 (CT-05): lokalny wpis ZASTĘPUJE tematy odziedziczone. `extend` albo suma
        // list uniemożliwiałyby zawężenie zestawu do tego, czego naprawdę potrzebuje krok.
        for pin in local.sets {
            if let Some(at) = selected.iter().position(|known| known.id == pin.id) {
                selected[at] = pin;
            } else {
                selected.push(pin);
            }
        }
    }
    Ok(EffectiveContext {
        sets: selected,
        inherits_workflow,
        protected_scope,
        excluded_workflow,
    })
}

/// Najniższy format, który nie pozwoli starszemu buildowi wykonać przypięć jako pustych danych.
#[must_use]
pub fn format_needed_by(file: &WorkflowFile) -> u32 {
    let workflow_uses_context = file
        .context()
        .ok()
        .flatten()
        .is_some_and(|context| !context.sets.is_empty());
    let a_step_uses_context = file.steps.iter().any(|step| match step {
        Step::Agent(one) => one.context().ok().flatten().is_some_and(|context| {
            context.inherit_workflow.is_some()
                || !context.exclude.is_empty()
                || !context.sets.is_empty()
        }),
        Step::Checkpoint(one) => one.extra.contains_key("context"),
        Step::Check(one) => one.extra.contains_key("context"),
        Step::Serve(one) => one.extra.contains_key("context"),
    });
    if workflow_uses_context || a_step_uses_context {
        super::file::CONTEXT_FORMAT
    } else {
        super::file::CURRENT
    }
}

/// 2026-09-08 (CT-05): usuwa puste wybory, żeby samo ich wyczyszczenie wracało do formatu 1.
pub(super) fn remove_empty(file: &mut WorkflowFile) {
    if file
        .context()
        .ok()
        .flatten()
        .is_some_and(|context| context.sets.is_empty())
    {
        file.extra.remove("context");
    }
    for step in &mut file.steps {
        let Step::Agent(agent) = step else {
            continue;
        };
        let empty = agent.context().ok().flatten().is_some_and(|context| {
            context.inherit_workflow.is_none()
                && context.exclude.is_empty()
                && context.sets.is_empty()
        });
        if empty {
            agent.extra.remove("context");
        }
    }
}
