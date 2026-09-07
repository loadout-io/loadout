//! WF-17: kolumna jest pełnym grafem. Ten kompilator nie uruchamia żadnego procesu.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::{EvalSet, Variant};
use crate::workflow::{
    CheckStep, ConditionalLink, Folder, Link, Point, Step, WhenItFails, WorkflowFile,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowSource {
    pub id: String,
    #[serde(default)]
    pub place: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub revision: Option<String>,
    pub output_step: String,
    #[serde(default)]
    pub overrides: BTreeMap<String, Map<String, Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellBinding {
    pub case: String,
    pub variant: String,
    pub repeat: usize,
    pub nodes: BTreeMap<String, String>,
    pub output: String,
    pub grader: String,
    pub context: String,
}

pub fn source(variant: &Variant) -> Result<WorkflowSource, String> {
    let value = variant.extra.get("workflow").ok_or_else(|| {
        format!(
            "Column {} needs a workflow and its output step.",
            variant.name
        )
    })?;
    let source: WorkflowSource = serde_json::from_value(value.clone())
        .map_err(|error| format!("The workflow column could not be read: {error}"))?;
    if source.id.trim().is_empty() || source.output_step.trim().is_empty() {
        return Err("Choose a workflow and its output step.".to_owned());
    }
    Ok(source)
}

pub fn compose(
    catalog: &[WorkflowFile],
    set: &EvalSet,
    id: String,
    name: String,
) -> Result<WorkflowFile, String> {
    let selected: Vec<_> = set
        .variants
        .iter()
        .map(|variant| {
            let source = source(variant)?;
            let mut matches = catalog.iter().filter(|graph| graph.id == source.id);
            let graph = matches.next().ok_or_else(|| {
                format!(
                    "The workflow {} is not available in this project.",
                    source.id
                )
            })?;
            if matches.next().is_some() {
                return Err(format!(
                    "More than one workflow has ID {}. Choose one unambiguous source.",
                    source.id
                ));
            }
            Ok(graph.clone())
        })
        .collect::<Result<_, String>>()?;
    compose_selected(&selected, set, id, name)
}

/// Komenda rozwiązuje źródło po półce/pliku/rewizji. Kompilator zachowuje tę kolejność,
/// także gdy dwie jawnie wybrane półki zawierają grafy o tym samym ID.
pub fn compose_selected(
    selected: &[WorkflowFile],
    set: &EvalSet,
    id: String,
    name: String,
) -> Result<WorkflowFile, String> {
    if selected.len() != set.variants.len() {
        return Err("Every workflow column needs its own resolved source.".to_owned());
    }
    let cases = set.running_cases();
    let protected = super::workflow_inputs::protection_required(set)?;
    let (graphs, _size, additional_inputs) = prepare_subjects(selected, set)?;

    let mut graph = WorkflowFile {
        format: crate::workflow::file::CURRENT,
        id,
        name,
        description: None,
        steps: Vec::new(),
        links: Vec::new(),
        extra: Map::new(),
    };
    let mut bindings = Vec::new();
    let mut contexts = Map::new();
    let mut assignments = Map::new();
    let mut checks = Map::new();
    let mut seeds = Map::new();
    let mut conditions = Vec::new();
    for (column, (variant, source, subject)) in graphs.iter().enumerate() {
        for (row, case) in cases.iter().enumerate() {
            for repeat in 0..case.repeats()? {
                let scope = format!("cell_{row}_{column}_{repeat}");
                let judge_scope = format!("judge_{row}_{column}_{repeat}");
                let names = step_names(subject, &scope);
                let nodes = node_keys(subject, &names);
                let inputs =
                    super::workflow_inputs::for_cell(subject, case, &scope, &names, &nodes)?;
                append_json(&mut contexts, inputs.contexts)?;
                append_json(&mut assignments, inputs.step_contexts)?;
                append_json(&mut checks, inputs.checks)?;
                append_json(&mut seeds, inputs.workspace_seeds)?;
                let output = names
                    .get(&source.output_step)
                    .ok_or_else(|| "The output step is missing.".to_owned())?
                    .clone();
                let grader = format!("{judge_scope}__check");
                contexts.insert(judge_scope.clone(), json!({"task":""}));
                let prefix = format!("{} · {} · {}", case.name, variant.name, repeat + 1);
                append_subject(&mut graph, subject, &names, &prefix, &mut conditions)?;
                graph.steps.push(grader_step(case, &prefix, &grader));
                assignments.insert(grader.clone(), json!(judge_scope));
                checks.insert(grader.clone(), json!({"results":[output]}));
                graph.links.push(Link {
                    from: output.clone(),
                    to: grader.clone(),
                    max_turns: None,
                });
                bindings.push(CellBinding {
                    case: case.id.clone(),
                    variant: variant.id.clone(),
                    repeat,
                    nodes,
                    output,
                    grader,
                    context: scope,
                });
            }
        }
    }
    if !additional_inputs.is_empty() {
        graph
            .extra
            .insert("additionalInputs".to_owned(), json!(additional_inputs));
    }
    graph.extra.insert(
        "executionInputs".to_owned(),
        json!({"schema":1,"isolateContexts":protected,"contexts":contexts,"stepContexts":assignments,
        "checks":checks,"workspaceSeeds":seeds,"effects":{"publishMemory":false,"externalMessages":false}}),
    );
    graph.extra.insert(
        "cellBindings".to_owned(),
        serde_json::to_value(bindings).map_err(|error| error.to_string())?,
    );
    if !conditions.is_empty() {
        graph.extra.insert(
            "linkConditions".to_owned(),
            serde_json::to_value(conditions).map_err(|error| error.to_string())?,
        );
    }
    Ok(graph)
}

/// Każdy kafelek jest osobną kopią tego samego wzorca, więc jego kroki muszą mieć
/// własne ID w skompilowanym grafie. Ta mapa pilnuje, że prefiks kafelka powstaje w
/// jednym miejscu — nikt nie skleja tych ID ręcznie gdzie indziej.
fn step_names(subject: &WorkflowFile, scope: &str) -> BTreeMap<String, String> {
    subject
        .steps
        .iter()
        .map(|step| (step.id().to_owned(), format!("{scope}__{}", step.id())))
        .collect()
}

/// Wiązania komórek adresują węzły PO rozwinięciu — z turą i kopią w kluczu, a nie
/// same kroki wzorca. Ta mapa tłumaczy klucz węzła wzorca na klucz tego samego węzła
/// w kafelku, żeby odczyt wyników nie musiał znać reguł prefiksowania.
fn node_keys(
    subject: &WorkflowFile,
    names: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    crate::workflow::unroll::unroll(subject)
        .nodes
        .into_iter()
        .map(|node| {
            let original = subject.steps[node.step].id();
            (
                crate::workflow::check::node_key_for(original, node.turn, node.copy),
                crate::workflow::check::node_key_for(&names[original], node.turn, node.copy),
            )
        })
        .collect()
}

fn append_json<T: Serialize>(
    target: &mut Map<String, Value>,
    entries: impl IntoIterator<Item = (String, T)>,
) -> Result<(), String> {
    for (key, value) in entries {
        target.insert(
            key,
            serde_json::to_value(value).map_err(|error| error.to_string())?,
        );
    }
    Ok(())
}

fn append_subject(
    graph: &mut WorkflowFile,
    subject: &WorkflowFile,
    names: &BTreeMap<String, String>,
    prefix: &str,
    conditions: &mut Vec<ConditionalLink>,
) -> Result<(), String> {
    for step in &subject.steps {
        let mut step = step.clone();
        match &mut step {
                        Step::Agent(one) => {
                            one.id = names[&one.id].clone();
                            one.name = format!("{prefix} · {}", one.name);
                        }
                        Step::Check(one) => {
                            one.id = names[&one.id].clone();
                            one.name = format!("{prefix} · {}", one.name);
                        }
                        Step::Checkpoint(_) | Step::Serve(_) => return Err("This workflow needs an interactive or background step, which comparisons do not run.".to_owned()),
                    }
        graph.steps.push(step);
    }
    for link in &subject.links {
        graph.links.push(Link {
            from: names[&link.from].clone(),
            to: names[&link.to].clone(),
            max_turns: link.max_turns,
        });
    }
    if let Some(value) = subject.extra.get("linkConditions") {
        let original: Vec<ConditionalLink> =
            serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
        for mut condition in original {
            condition.from.clone_from(
                names
                    .get(&condition.from)
                    .ok_or_else(|| "A conditional link has no source.".to_owned())?,
            );
            condition.to.clone_from(
                names
                    .get(&condition.to)
                    .ok_or_else(|| "A conditional link has no target.".to_owned())?,
            );
            conditions.push(condition);
        }
    }
    Ok(())
}

fn grader_step(case: &super::Case, prefix: &str, grader: &str) -> Step {
    Step::Check(CheckStep {
        id: grader.to_owned(),
        name: format!("{prefix} · Independent checks"),
        command: case.command.clone(),
        proof: case.proof.clone(),
        required_tests: Vec::new(),
        folder: Folder::FreshCopy,
        when_it_fails: WhenItFails::Stop,
        at: Point::default(),
        extra: ["proofMode", "examiner"]
            .into_iter()
            .filter_map(|key| {
                case.extra
                    .get(key)
                    .map(|value| (key.to_owned(), value.clone()))
            })
            .collect(),
    })
}

fn limit() -> String {
    "This comparison exceeds 100 cells, 1000 executed steps, 5000 connections or 100 working folders. Reduce cases, repeats or workflow copies.".to_owned()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonSize {
    pub cells: usize,
    pub nodes: usize,
    pub edges: usize,
    pub trees: usize,
}

/// Read-only executor preview bierze te same nadpisane kroki, nie implementuje patchy ponownie.
pub fn preflight_selected_with_subjects(
    selected: &[WorkflowFile],
    set: &EvalSet,
) -> Result<(ComparisonSize, Vec<WorkflowFile>), String> {
    let (subjects, size, _) = prepare_subjects(selected, set)?;
    Ok((
        size,
        subjects.into_iter().map(|(_, _, graph)| graph).collect(),
    ))
}

type PreparedSubject<'a> = (&'a Variant, WorkflowSource, WorkflowFile);

fn prepare_subjects<'a>(
    selected: &[WorkflowFile],
    set: &'a EvalSet,
) -> Result<(Vec<PreparedSubject<'a>>, ComparisonSize, Vec<String>), String> {
    if selected.len() != set.variants.len() {
        return Err("Every workflow column needs its own resolved source.".to_owned());
    }
    super::workflow_inputs::validate_cases(set)?;
    let additional_inputs = super::workflow_inputs::shared_additional_inputs(selected, set)?;
    let repeats = set.running_cases().iter().try_fold(0usize, |sum, case| {
        sum.checked_add(case.repeats()?).ok_or_else(limit)
    })?;
    let mut size = ComparisonSize {
        cells: repeats.checked_mul(selected.len()).ok_or_else(limit)?,
        ..ComparisonSize::default()
    };
    if size.cells > 100 {
        return Err(limit());
    }
    let mut graphs = Vec::new();
    for (variant, graph) in set.variants.iter().zip(selected) {
        let source = source(variant)?;
        if graph.id != source.id {
            return Err("The selected workflow no longer has the expected identity.".to_owned());
        }
        let mut graph = graph.clone();
        for (key, patch) in &source.overrides {
            let step = graph
                .steps
                .iter_mut()
                .find(|step| step.id() == key)
                .ok_or_else(|| format!("The override names an unknown step: {key}."))?;
            let Step::Agent(agent) = step else {
                return Err(
                    "Only agent settings can be overridden by a workflow column.".to_owned(),
                );
            };
            for (key, value) in patch {
                agent.overrides.insert(key.clone(), value.clone());
            }
        }
        let expanded = crate::workflow::unroll::checked_size(&graph).map_err(|_| limit())?;
        // Grader dodaje jeden węzeł i jedną strzałkę od pojedynczego output.
        size.nodes = expanded
            .nodes
            .checked_add(1)
            .and_then(|count| count.checked_mul(repeats))
            .and_then(|count| size.nodes.checked_add(count))
            .ok_or_else(limit)?;
        size.edges = expanded
            .edges
            .checked_add(1)
            .and_then(|count| count.checked_mul(repeats))
            .and_then(|count| size.edges.checked_add(count))
            .ok_or_else(limit)?;
        if expanded.nodes > 1000 || expanded.edges > 5000 || size.nodes > 1000 || size.edges > 5000
        {
            return Err(limit());
        }
        graphs.push((variant, source, graph));
    }
    // 2026-09-06 — dopiero po sprawdzeniu całej macierzy wolno rozwinąć ograniczony
    // subject. SameCopy liczy faktyczne aliasy, a nie nowy katalog za każdy kafelek.
    for (_, source, graph) in &graphs {
        validate_subject(graph, &source.output_step)?;
        let expanded = crate::workflow::unroll::unroll(graph);
        let inputs = crate::workflow::execution::RunInputs::from_graph(graph)?;
        let folders =
            crate::workflow::unroll::folders::working_folders(graph, &expanded, &inputs, true)?;
        size.trees = folders
            .managed
            .len()
            .checked_add(1)
            .and_then(|count| count.checked_mul(repeats))
            .and_then(|count| size.trees.checked_add(count))
            .ok_or_else(limit)?;
        if size.trees > 100 {
            return Err(limit());
        }
    }
    Ok((graphs, size, additional_inputs))
}

fn validate_subject(graph: &WorkflowFile, output: &str) -> Result<(), String> {
    if graph.steps.len() > 1000 {
        return Err(limit());
    }
    let indices: BTreeMap<_, _> = graph
        .steps
        .iter()
        .enumerate()
        .map(|(at, step)| (step.id(), at))
        .collect();
    let out = *indices
        .get(output)
        .ok_or_else(|| "Choose an output step in the selected workflow.".to_owned())?;
    let forward: Vec<_> = graph
        .links
        .iter()
        .filter(|link| link.max_turns.is_none())
        .map(|link| {
            Ok((
                *indices
                    .get(link.from.as_str())
                    .ok_or_else(|| "A workflow link has no source.".to_owned())?,
                *indices
                    .get(link.to.as_str())
                    .ok_or_else(|| "A workflow link has no target.".to_owned())?,
            ))
        })
        .collect::<Result<_, String>>()?;
    if graph.links.iter().any(|link| link.from == output) {
        return Err("The output step must have no following steps or loop back.".to_owned());
    }
    for link in graph.links.iter().filter(|link| link.max_turns.is_some()) {
        let judge = *indices
            .get(link.from.as_str())
            .ok_or_else(|| "A loop has no judge.".to_owned())?;
        let entry = *indices
            .get(link.to.as_str())
            .ok_or_else(|| "A loop has no entry.".to_owned())?;
        let body = crate::workflow::unroll::body_of(judge, entry, &forward);
        if body.contains(&out) {
            return Err("The output step must be outside every loop.".to_owned());
        }
    }
    let mut reachable = BTreeSet::from([out]);
    loop {
        let before = reachable.len();
        for &(from, to) in &forward {
            if reachable.contains(&to) {
                reachable.insert(from);
            }
        }
        if before == reachable.len() {
            break;
        }
    }
    if reachable.len() != graph.steps.len() {
        return Err("Every workflow step must have a path to the selected output.".to_owned());
    }
    for (at, step) in graph.steps.iter().enumerate() {
        let (folder, copies) = match step {
            Step::Agent(one) => (&one.folder, usize::try_from(one.copies).map_err(|_| limit())?),
            Step::Check(one) => (&one.folder, 1),
            _ => return Err("Comparisons currently support automatic agent and check steps only. Remove questions and background apps from this variant.".to_owned()),
        };
        if at == out && copies != 1 {
            return Err(
                "The output step must have exactly one copy. Add an explicit synthesis step."
                    .to_owned(),
            );
        }
        if matches!(folder, Folder::Pick { .. }) {
            return Err("A comparison cannot write to a selected external folder. Use a managed working copy.".to_owned());
        }
    }
    if let Some(note) = crate::workflow::check::check_to_run(graph)
        .into_iter()
        .find(|note| note.level == crate::workflow::check::Level::Problem)
    {
        return Err(note.message);
    }
    Ok(())
}
