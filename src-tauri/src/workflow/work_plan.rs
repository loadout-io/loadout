//! Ustawienie Plan i jeden resolver jego zależności po rozwinięciu kopii oraz pętli.
//!
//! Zapis, panel i bieg pytają ten moduł o ten sam graf. Dzięki temu `Same plan as` nie może
//! wyglądać poprawnie w pickerze, a podczas wykonania po cichu wrócić do innego przodka.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;

use crate::work_plan::{Configuration, Mode};

use super::check::{Level, Note, node_key_for};
use super::unroll::{Unrolled, unroll};
use super::{AgentStep, Step, WorkflowFile};

/// Moment, w którym wynik resolvera trafi do człowieka. Szkic ostrzega; Start odmawia.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum When {
    Saving,
    Running,
}

/// Rozwiązanie jednego logicznego kroku dla panelu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepResolution {
    pub step_id: String,
    pub mode: Mode,
    pub inherited: bool,
    pub source_step_ids: Vec<String>,
    pub earlier_step_ids: Vec<String>,
}

/// Zamrożone źródła fizycznych kroków. Klucze mają rundę i kopię, tak jak `run.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthorMap {
    pub sources: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Resolution {
    pub authors: AuthorMap,
    pub steps: Vec<StepResolution>,
    pub notes: Vec<Note>,
}

struct Refusals<'a> {
    level: Level,
    notes: &'a mut Vec<Note>,
}

/// Typowany parser całego dokumentu. Błędny lub przyszły tryb nigdy nie staje się `Off`.
pub fn validate(file: &WorkflowFile) -> Result<(), String> {
    for step in &file.steps {
        match step {
            Step::Agent(agent) => {
                configuration_for(agent)?;
            }
            Step::Checkpoint(one) if one.extra.contains_key("plan") => {
                return Err(format!(
                    "{} cannot use Plan because only agent steps can create, update or use it.",
                    one.name
                ));
            }
            Step::Check(one) if one.extra.contains_key("plan") => {
                return Err(format!(
                    "{} cannot use Plan because only agent steps can create, update or use it.",
                    one.name
                ));
            }
            Step::Serve(one) if one.extra.contains_key("plan") => {
                return Err(format!(
                    "{} cannot use Plan because only agent steps can create, update or use it.",
                    one.name
                ));
            }
            Step::Checkpoint(_) | Step::Check(_) | Step::Serve(_) => {}
        }
    }
    Ok(())
}

fn configuration_for(agent: &AgentStep) -> Result<Configuration, String> {
    Configuration::from_step(agent.extra.get("plan")).map_err(|error| {
        let detail = error.to_string();
        if detail.contains("does not know that Plan setting") {
            format!(
                "{} uses a Plan setting this Loadout does not know. Update Loadout or choose another setting.",
                agent.name
            )
        } else {
            format!(
                "{} has a Plan setting that cannot be read. Open Plan and choose its settings again.",
                agent.name
            )
        }
    })
}

/// Uwagi pokazane przy zapisie albo sprawdzane przed Startem.
#[must_use]
pub(crate) fn notes(file: &WorkflowFile, when: When) -> Vec<Note> {
    resolve(file, when).notes
}

/// Źródła wykonania policzone raz przed utworzeniem katalogu biegu.
pub fn authors(file: &WorkflowFile) -> Result<AuthorMap, Note> {
    let resolved = resolve(file, When::Running);
    if let Some(problem) = resolved
        .notes
        .into_iter()
        .find(|note| note.level == Level::Problem)
    {
        Err(problem)
    } else {
        Ok(resolved.authors)
    }
}

#[must_use]
pub(crate) fn resolution_for_panel(file: &WorkflowFile) -> Resolution {
    let mut visible = file.clone();
    materialize(&mut visible);
    resolve(&visible, When::Running)
}

fn resolve(file: &WorkflowFile, when: When) -> Resolution {
    let configurations = match configurations(file) {
        Ok(configurations) => configurations,
        Err(message) => {
            return Resolution {
                notes: vec![note(Level::Problem, None, message)],
                ..Resolution::default()
            };
        }
    };
    let graph = unroll(file);
    let modes = graph
        .nodes
        .iter()
        .map(|node| configurations[node.step].mode())
        .collect::<Vec<_>>();
    let enabled = configurations
        .iter()
        .enumerate()
        .filter(|(_, configuration)| configuration.mode() != Mode::Off)
        .map(|(at, _)| at)
        .collect::<Vec<_>>();
    let mut resolved = Resolution::default();
    if enabled.is_empty() {
        resolved.steps = configurations
            .iter()
            .enumerate()
            .filter(|&(at, _)| matches!(file.steps.get(at), Some(Step::Agent(_))))
            .map(|(at, configuration)| StepResolution {
                step_id: file.steps[at].id().to_owned(),
                mode: configuration.mode(),
                inherited: configuration.inherited,
                source_step_ids: Vec::new(),
                earlier_step_ids: Vec::new(),
            })
            .collect();
        return resolved;
    }

    let level = match when {
        When::Saving => Level::Warning,
        When::Running => Level::Problem,
    };
    validate_creates(file, &configurations, &enabled, level, &mut resolved.notes);
    validate_authors(
        file,
        &graph,
        &configurations,
        &enabled,
        level,
        &mut resolved.notes,
    );
    resolve_dependencies(
        file,
        &graph,
        &configurations,
        &modes,
        &enabled,
        level,
        &mut resolved,
    );
    resolved
}

fn validate_creates(
    file: &WorkflowFile,
    configurations: &[Configuration],
    enabled: &[usize],
    level: Level,
    notes: &mut Vec<Note>,
) {
    let creates = enabled
        .iter()
        .copied()
        .filter(|&at| configurations[at].mode() == Mode::Create)
        .collect::<Vec<_>>();
    if creates.is_empty() {
        for &at in enabled {
            let step = &file.steps[at];
            push_once(
                notes,
                note(
                    level,
                    Some(step.id()),
                    format!(
                        "{} needs a plan, but no step creates one. Set one earlier step to Plan: Create.",
                        step.name()
                    ),
                ),
            );
        }
    } else if creates.len() > 1 {
        let names = creates
            .iter()
            .map(|&at| file.steps[at].name())
            .collect::<Vec<_>>()
            .join(" and ");
        let blamed = creates.get(1).copied().unwrap_or(creates[0]);
        push_once(
            notes,
            note(
                level,
                Some(file.steps[blamed].id()),
                format!("{names} both create a plan. Keep exactly one Create step."),
            ),
        );
    }
}

fn validate_authors(
    file: &WorkflowFile,
    graph: &Unrolled,
    configurations: &[Configuration],
    enabled: &[usize],
    level: Level,
    notes: &mut Vec<Note>,
) {
    let authors = enabled
        .iter()
        .copied()
        .filter(|&at| matches!(configurations[at].mode(), Mode::Create | Mode::Update))
        .collect::<Vec<_>>();
    for &at in &authors {
        let Step::Agent(agent) = &file.steps[at] else {
            continue;
        };
        if agent.copies != 1 {
            push_once(
                notes,
                note(
                    level,
                    Some(&agent.id),
                    format!(
                        "{} cannot create or update the plan in more than one copy. Set How many at once to 1.",
                        agent.name
                    ),
                ),
            );
        }
        if graph.loops.iter().any(|one| one.body.contains(&at)) {
            push_once(
                notes,
                note(
                    level,
                    Some(&agent.id),
                    format!(
                        "{} cannot create or update the plan inside a retry loop. Move it before or after the loop.",
                        agent.name
                    ),
                ),
            );
        }
    }
    for (position, &one) in authors.iter().enumerate() {
        for &other in authors.iter().skip(position + 1) {
            if !logical_before(one, other, graph) && !logical_before(other, one, graph) {
                push_once(
                    notes,
                    note(
                        level,
                        Some(file.steps[other].id()),
                        format!(
                            "{} and {} both change the plan, but neither must run before the other. Draw an arrow so one must run before the other.",
                            file.steps[one].name(),
                            file.steps[other].name()
                        ),
                    ),
                );
            }
        }
    }
}

fn resolve_dependencies(
    file: &WorkflowFile,
    graph: &Unrolled,
    configurations: &[Configuration],
    modes: &[Mode],
    enabled: &[usize],
    level: Level,
    resolved: &mut Resolution,
) {
    let origins = plan_origins(file, graph, configurations, modes);
    let incoming = incoming_to(graph.nodes.len(), &graph.arrows);
    let mut step_sources = vec![BTreeSet::<String>::new(); file.steps.len()];
    let mut earlier = vec![BTreeSet::<String>::new(); file.steps.len()];
    for (current, node) in graph.nodes.iter().enumerate() {
        if !matches!(modes[current], Mode::Update | Mode::Use) {
            continue;
        }
        let step = &file.steps[node.step];
        for &candidate in enabled {
            if candidate != node.step && logical_before(candidate, node.step, graph) {
                earlier[node.step].insert(file.steps[candidate].id().to_owned());
            }
        }
        let configuration = &configurations[node.step];
        let mut refusals = Refusals {
            level,
            notes: &mut resolved.notes,
        };
        let providers = if let Some(chosen) = configuration.same_plan_as.as_deref() {
            explicit_sources(
                file,
                graph,
                configurations,
                &origins,
                current,
                chosen,
                &mut refusals,
            )
        } else {
            automatic_sources(
                file,
                graph,
                &origins,
                &incoming[current],
                current,
                &mut refusals,
            )
        };
        if let Some((provider_keys, source_ids)) = providers {
            resolved
                .authors
                .sources
                .insert(node_key_for(step.id(), node.turn, node.copy), provider_keys);
            step_sources[node.step].extend(source_ids);
        }
    }
    resolved.steps = configurations
        .iter()
        .enumerate()
        .filter(|&(at, _)| matches!(file.steps.get(at), Some(Step::Agent(_))))
        .map(|(at, configuration)| StepResolution {
            step_id: file.steps[at].id().to_owned(),
            mode: configuration.mode(),
            inherited: configuration.inherited,
            source_step_ids: step_sources[at].iter().cloned().collect(),
            earlier_step_ids: earlier[at].iter().cloned().collect(),
        })
        .collect();
}

fn configurations(file: &WorkflowFile) -> Result<Vec<Configuration>, String> {
    validate(file)?;
    file.steps
        .iter()
        .map(|step| match step {
            Step::Agent(agent) => configuration_for(agent),
            Step::Checkpoint(_) | Step::Check(_) | Step::Serve(_) => Ok(Configuration::default()),
        })
        .collect()
}

fn incoming_to(nodes: usize, arrows: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut incoming = vec![Vec::new(); nodes];
    for &(from, to) in arrows {
        if let Some(parents) = incoming.get_mut(to) {
            parents.push(from);
        }
    }
    incoming
}

fn plan_origins(
    file: &WorkflowFile,
    graph: &Unrolled,
    configurations: &[Configuration],
    modes: &[Mode],
) -> Vec<BTreeSet<usize>> {
    let incoming = incoming_to(graph.nodes.len(), &graph.arrows);
    let mut origins = vec![BTreeSet::new(); graph.nodes.len()];
    for (at, mode) in modes.iter().enumerate() {
        if matches!(mode, Mode::Create | Mode::Update) {
            origins[at].insert(at);
        }
    }
    // 2026-09-08 (WP-02): stały punkt, nie kolejność pliku. Węzły `unroll` zachowują kolejność
    // kafelków dla nazw wyników, więc rodzic może stać w wektorze za dzieckiem mimo poprawnej strzałki.
    loop {
        let before = origins.clone();
        for (at, node) in graph.nodes.iter().enumerate() {
            if matches!(modes[at], Mode::Create | Mode::Update) {
                continue;
            }
            let selected = configurations[node.step]
                .same_plan_as
                .as_deref()
                .filter(|_| modes[at] == Mode::Use)
                .map_or_else(
                    || incoming[at].clone(),
                    |chosen| {
                        graph
                            .nodes
                            .iter()
                            .enumerate()
                            .filter(|(source, candidate)| {
                                file.steps[candidate.step].id() == chosen
                                    && comes_before(*source, at, &graph.arrows)
                            })
                            .map(|(source, _)| source)
                            .collect()
                    },
                );
            origins[at] = selected
                .iter()
                .flat_map(|&parent| before[parent].iter().copied())
                .collect();
        }
        if origins == before {
            return origins;
        }
    }
}

fn explicit_sources(
    file: &WorkflowFile,
    graph: &Unrolled,
    configurations: &[Configuration],
    origins: &[BTreeSet<usize>],
    current: usize,
    chosen: &str,
    refusals: &mut Refusals<'_>,
) -> Option<(Vec<String>, BTreeSet<String>)> {
    let current_step = &file.steps[graph.nodes[current].step];
    let Some(chosen_step) = file.steps.iter().position(|step| step.id() == chosen) else {
        push_once(
            refusals.notes,
            note(
                refusals.level,
                Some(current_step.id()),
                format!(
                    "{} names a Plan source that is no longer in this workflow. Choose Same plan as again.",
                    current_step.name()
                ),
            ),
        );
        return None;
    };
    if configurations[chosen_step].mode() == Mode::Off {
        push_once(
            refusals.notes,
            note(
                refusals.level,
                Some(current_step.id()),
                format!(
                    "{} says Same plan as {}, but {} does not create, update or use a plan. Choose another earlier step.",
                    current_step.name(),
                    file.steps[chosen_step].name(),
                    file.steps[chosen_step].name()
                ),
            ),
        );
        return None;
    }
    let physical = graph
        .nodes
        .iter()
        .enumerate()
        .filter(|(source, node)| {
            node.step == chosen_step && comes_before(*source, current, &graph.arrows)
        })
        .map(|(source, _)| source)
        .collect::<Vec<_>>();
    if physical.is_empty() {
        push_once(
            refusals.notes,
            note(
                refusals.level,
                Some(current_step.id()),
                format!(
                    "{} says Same plan as {}, but {} does not run before it. Draw a dependency from {} first.",
                    current_step.name(),
                    file.steps[chosen_step].name(),
                    file.steps[chosen_step].name(),
                    file.steps[chosen_step].name()
                ),
            ),
        );
        return None;
    }
    let inherited = physical
        .iter()
        .flat_map(|&source| origins[source].iter().copied())
        .collect::<BTreeSet<_>>();
    if inherited.len() > 1 {
        ambiguous(file, graph, current, &inherited, refusals);
        return None;
    }
    let provider_keys = physical
        .iter()
        .map(|&source| {
            let node = graph.nodes[source];
            node_key_for(file.steps[node.step].id(), node.turn, node.copy)
        })
        .collect();
    Some((
        provider_keys,
        BTreeSet::from([file.steps[chosen_step].id().to_owned()]),
    ))
}

fn automatic_sources(
    file: &WorkflowFile,
    graph: &Unrolled,
    origins: &[BTreeSet<usize>],
    parents: &[usize],
    current: usize,
    refusals: &mut Refusals<'_>,
) -> Option<(Vec<String>, BTreeSet<String>)> {
    let inherited = parents
        .iter()
        .flat_map(|&parent| origins[parent].iter().copied())
        .collect::<BTreeSet<_>>();
    let current_step = &file.steps[graph.nodes[current].step];
    if inherited.is_empty() {
        push_once(
            refusals.notes,
            note(
                refusals.level,
                Some(current_step.id()),
                format!(
                    "{} needs a plan, but none of its earlier steps provides one. Connect it after Create or Update.",
                    current_step.name()
                ),
            ),
        );
        return None;
    }
    if inherited.len() > 1 {
        ambiguous(file, graph, current, &inherited, refusals);
        return None;
    }
    let source = inherited.iter().next().copied()?;
    let node = graph.nodes[source];
    Some((
        vec![node_key_for(
            file.steps[node.step].id(),
            node.turn,
            node.copy,
        )],
        BTreeSet::from([file.steps[node.step].id().to_owned()]),
    ))
}

fn ambiguous(
    file: &WorkflowFile,
    graph: &Unrolled,
    current: usize,
    inherited: &BTreeSet<usize>,
    refusals: &mut Refusals<'_>,
) {
    let current_step = &file.steps[graph.nodes[current].step];
    let names = inherited
        .iter()
        .map(|&source| file.steps[graph.nodes[source].step].name())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(" and ");
    push_once(
        refusals.notes,
        note(
            refusals.level,
            Some(current_step.id()),
            format!(
                "{} can receive different plan versions from {}. Choose Same plan as after drawing a dependency to that step.",
                current_step.name(),
                names
            ),
        ),
    );
}

fn logical_before(from: usize, to: usize, graph: &Unrolled) -> bool {
    graph.nodes.iter().enumerate().any(|(source, one)| {
        one.step == from
            && graph.nodes.iter().enumerate().any(|(target, other)| {
                other.step == to && comes_before(source, target, &graph.arrows)
            })
    })
}

/// Materializuje intencję autora w pliku, bez zmiany semantyki resolvera używanego przy Starcie.
pub(super) fn materialize(file: &mut WorkflowFile) {
    let graph = unroll(file);
    let authors = file
        .steps
        .iter()
        .enumerate()
        .filter_map(|(at, step)| match step {
            Step::Agent(agent) => configuration_for(agent)
                .ok()
                .is_some_and(|configuration| configuration.writes_candidate())
                .then_some(at),
            Step::Checkpoint(_) | Step::Check(_) | Step::Serve(_) => None,
        })
        .collect::<Vec<_>>();
    let inheriting = file
        .steps
        .iter()
        .enumerate()
        .filter_map(|(at, step)| match step {
            Step::Agent(agent)
                if !agent.extra.contains_key("plan")
                    && authors
                        .iter()
                        .any(|&author| logical_before(author, at, &graph)) =>
            {
                Some(at)
            }
            Step::Agent(_) | Step::Checkpoint(_) | Step::Check(_) | Step::Serve(_) => None,
        })
        .collect::<Vec<_>>();

    for at in inheriting {
        if let Step::Agent(agent) = &mut file.steps[at] {
            // 2026-09-09 (WP-08): bajty muszą odróżniać automat od wyboru człowieka, bo panel
            // nazywa źródło, a ręczne Off nie może wrócić do Use przy kolejnym zapisie.
            agent.extra.insert(
                "plan".to_owned(),
                json!({ "mode": "use", "inherited": true }),
            );
        }
    }
}

fn comes_before(from: usize, to: usize, arrows: &[(usize, usize)]) -> bool {
    let mut seen = BTreeSet::new();
    let mut pending = vec![from];
    while let Some(at) = pending.pop() {
        if !seen.insert(at) {
            continue;
        }
        for &(_, next) in arrows.iter().filter(|(source, _)| *source == at) {
            if next == to {
                return true;
            }
            pending.push(next);
        }
    }
    false
}

fn note(level: Level, step_id: Option<&str>, message: String) -> Note {
    Note {
        level,
        step_id: step_id.map(str::to_owned),
        message,
        fix: None,
    }
}

fn push_once(notes: &mut Vec<Note>, candidate: Note) {
    if !notes
        .iter()
        .any(|known| known.step_id == candidate.step_id && known.message == candidate.message)
    {
        notes.push(candidate);
    }
}

/// Najniższy format, który nie pozwoli czytnikowi znającemu tylko Context pominąć Plan.
#[must_use]
pub fn format_needed_by(file: &WorkflowFile) -> u32 {
    if file.steps.iter().any(|step| match step {
        Step::Agent(one) => one.extra.contains_key("plan"),
        Step::Checkpoint(one) => one.extra.contains_key("plan"),
        Step::Check(one) => one.extra.contains_key("plan"),
        Step::Serve(one) => one.extra.contains_key("plan"),
    }) {
        super::file::PLAN_FORMAT
    } else {
        super::file::CURRENT
    }
}

/// Pusty `Off` znika tylko tam, gdzie nie może oznaczać odmowy dziedziczenia od autora.
pub(super) fn remove_empty(file: &mut WorkflowFile) {
    let has_author = file.steps.iter().any(|step| match step {
        Step::Agent(agent) => configuration_for(agent)
            .ok()
            .is_some_and(|configuration| configuration.writes_candidate()),
        Step::Checkpoint(_) | Step::Check(_) | Step::Serve(_) => false,
    });
    if has_author {
        return;
    }
    for step in &mut file.steps {
        let Step::Agent(agent) = step else {
            continue;
        };
        let empty = Configuration::from_step(agent.extra.get("plan"))
            .ok()
            .is_some_and(|configuration| configuration == Configuration::default());
        if empty {
            agent.extra.remove("plan");
        }
    }
}
