//! WF-23: wskazany zapis jest wejściem zwykłego wykonawcy, nie drugim wykonawcą.
//! Podgląd nie uruchamia procesów i nie przechowuje złożonego promptu.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::Part;
use super::lead_start::RunRef;
use super::workflows::{
    WorkflowPlace, library_workflows, list_workflow_definitions_inner, project_workflows,
};
use crate::library::agents::{Agent, Overrides, Tools, policy_of, read_agent_directory, resolve};
use crate::library::definition::Definition;
use crate::workflow::{Step, WorkflowFile};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReplayMode {
    Recorded,
    Current,
}

/// Prywatne wejścia żyją wyłącznie do startu; Debug nie wypisuje zadań ani instrukcji.
pub struct ReplayMaterial {
    pub source: RunRef,
    pub source_dir: PathBuf,
    pub mode: ReplayMode,
    pub workflow: PathBuf,
    pub workflow_revision: String,
    pub graph: WorkflowFile,
    pub graph_bytes: Vec<u8>,
    pub task: Option<String>,
    /// Sufit zatwierdzony razem z podglądem, także kiedy wynosi jawne „bez limitu”.
    pub budget_usd: Option<f64>,
    pub part: Option<Part>,
    pub input: Option<super::input_snapshot::InputSnapshot>,
    pub effective: BTreeMap<String, Agent>,
    pub skills: BTreeMap<String, super::run::SavedSkillBundles>,
    pub instructions: Option<crate::inherit::instructions::InstructionSnapshot>,
    pub(crate) memory_sources: Option<super::memory_sources::Snapshot>,
    pub preview: Value,
    source_revision: String,
    policy_revisions: BTreeMap<PathBuf, String>,
}

impl fmt::Debug for ReplayMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReplayMaterial")
            .field("source", &self.source)
            .field("mode", &self.mode)
            .field("agents", &self.effective.len())
            .finish_non_exhaustive()
    }
}

impl ReplayMaterial {
    /// Przed zgodą, aktywacją i planowaniem: zmiana źródła/prawa nigdy nie dziedziczy tokenu.
    pub fn validate(&self) -> Result<(), String> {
        let source = super::lead_history::source_for(&self.source.workspace, &self.source.run_id)?;
        let bytes = super::lead_history::source_bytes(&self.source.workspace, &source)?;
        if crate::durable_file::revision_of(&bytes) != self.source_revision {
            return Err(
                "The saved run changed after this preview. Review it again; nothing started."
                    .to_owned(),
            );
        }
        let source: Value = serde_json::from_slice(&bytes)
            .map_err(|_| unavailable("the saved description cannot be read"))?;
        if source.get("workspace_inputs").is_some() {
            super::workspace_inputs::read_bound(&self.source_dir)
                .map_err(|error| unavailable(&error.to_string()))?;
        }
        for (path, expected) in &self.policy_revisions {
            let bytes = std::fs::read(path).map_err(|_| {
                "Today's agent permissions are no longer available. Review the repeat again."
                    .to_owned()
            })?;
            if crate::durable_file::revision_of(&bytes) != *expected {
                return Err("Today's workflow or agent permissions changed after this preview. Review it again; nothing started.".to_owned());
            }
        }
        if let Some(input) = &self.input {
            input
                .validate()
                .map_err(|error| unavailable(&error.to_string()))?;
        }
        if self.mode == ReplayMode::Recorded {
            let actual = super::memory_sources::read_bound(&self.source_dir)
                .map_err(|error| unavailable(&error.to_string()))?;
            let actual = actual
                .as_ref()
                .map(super::memory_sources::Snapshot::binding)
                .transpose()
                .map_err(|error| unavailable(&error.to_string()))?;
            let expected = self
                .memory_sources
                .as_ref()
                .map(super::memory_sources::Snapshot::binding)
                .transpose()
                .map_err(|error| unavailable(&error.to_string()))?;
            if actual != expected {
                return Err(unavailable(
                    "the saved memory sources changed after this preview",
                ));
            }
            for (key, expected) in &self.skills {
                let actual = super::run::saved_skill_bundles(&self.source_dir, key)
                    .map_err(|error| unavailable(&error.to_string()))?;
                if expected.agent != actual.agent || expected.borrowed != actual.borrowed {
                    return Err(unavailable(
                        "the saved skill package changed after this preview",
                    ));
                }
            }
            if self.instructions.is_some() {
                crate::inherit::instructions::read_snapshot(&self.source_dir)
                    .map_err(|error| unavailable(&error.to_string()))?;
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn handoffs_from(&self) -> Option<PathBuf> {
        self.part.as_ref().map(|_| self.source_dir.clone())
    }
}

fn unavailable(detail: &str) -> String {
    format!("Recorded inputs are unavailable: {detail}. Nothing was replaced with today's files.")
}

/// Zapis historycznego grafu jest nową definicją, nigdy ponowieniem ani nadpisaniem źródła.
pub fn copy_recorded_workflow(
    home: &Path,
    project: &Path,
    source_run_id: &str,
) -> Result<Value, String> {
    let source = super::lead_history::source_for(project, source_run_id)?;
    let bytes = super::lead_history::source_bytes(project, &source)?;
    let saved: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "The saved workflow cannot be read. Nothing was copied.".to_owned())?;
    if saved.get("id").and_then(Value::as_str) != Some(source_run_id) {
        return Err("The saved run does not match its folder. Nothing was copied.".to_owned());
    }
    let graph_bytes = serde_json::to_vec(
        saved
            .get("workflow_snapshot")
            .ok_or_else(|| "This run has no saved workflow. Nothing was copied.".to_owned())?,
    )
    .map_err(|error| error.to_string())?;
    let mut graph = crate::workflow::file::load_snapshot(
        &project
            .join(".loadout/runs")
            .join(&source.run_folder)
            .join("run.json"),
        &graph_bytes,
    )
    .map_err(|error| error.to_string())?;
    graph.id = uuid::Uuid::now_v7().to_string();
    graph.name = format!("{} (copy)", graph.name);
    let filename = format!("saved-workflow-{}.json", graph.id);
    // Osobna definicja w bibliotece, nie nadpisanie aktualnego grafu ani przypięcie do
    // aktywnego workspace. CAS None pozostaje w tym samym writerze co zwykłe Create.
    if super::lead_history::source_bytes(project, &source)? != bytes {
        return Err("The saved run changed while it was read. Nothing was copied.".to_owned());
    }
    super::workflows::save_workflow_inner(home, None, &filename, &graph, None)
        .map_err(|error| error.to_string())?;
    Ok(json!({"fileName":filename,"workflowId":graph.id,
        "said":format!("Saved {} as a new workflow. Nothing started.", graph.name)}))
}

pub fn prepare(home: &Path, project: &Path, input: &Value) -> Result<Arc<ReplayMaterial>, String> {
    if input.get("folder").is_some() || input.get("workspace").is_some() {
        return Err("Repeat only addresses the folder of this conversation.".to_owned());
    }
    let id = input
        .get("source_run_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "Choose the exact saved run ID first.".to_owned())?;
    let mode: ReplayMode =
        serde_json::from_value(input.get("mode").cloned().unwrap_or(Value::Null))
            .map_err(|_| "Choose recorded or current setup explicitly.".to_owned())?;
    let source = super::lead_history::source_for(project, id)?;
    let source_bytes = super::lead_history::source_bytes(project, &source)?;
    let saved: Value = serde_json::from_slice(&source_bytes)
        .map_err(|_| unavailable("the saved description cannot be read"))?;
    let budget_usd = match saved.get("budget_usd") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            value
                .as_f64()
                .filter(|value| value.is_finite() && *value > 0.0)
                .ok_or_else(|| {
                    "The saved spending limit cannot be read. Nothing started.".to_owned()
                })?,
        ),
    };
    if matches!(
        saved.get("status").and_then(Value::as_str),
        Some("running" | "paused")
    ) {
        return Err(
            "This saved run has not settled. Read its current status before repeating it."
                .to_owned(),
        );
    }
    let source_dir = project.join(".loadout/runs").join(source.run_folder);
    if saved.get("workspace_inputs").is_some() {
        super::workspace_inputs::read_bound(&source_dir)
            .map_err(|error| unavailable(&error.to_string()))?;
    }
    let recorded_graph: WorkflowFile = serde_json::from_value(
        saved
            .get("workflow_snapshot")
            .cloned()
            .ok_or_else(|| unavailable("the workflow was not saved"))?,
    )
    .map_err(|_| unavailable("the saved workflow cannot be read"))?;
    let catalog =
        list_workflow_definitions_inner(home, Some(project)).map_err(|error| error.to_string())?;
    let (current, revision) = catalog.into_iter().find_map(|one| match one {
        Definition::Healthy { value, revision } if value.workflow.id == recorded_graph.id => Some((value, revision)),
        _ => None,
    }).ok_or_else(|| "The current workflow is unavailable, so its current permissions cannot be checked. You can save the old workflow as a separate copy, but that does not authorize repeating this run.".to_owned())?;
    let workflow = match current.place {
        WorkflowPlace::Project => project_workflows(project),
        WorkflowPlace::Library => library_workflows(home),
    }
    .join(&current.path);
    let changed =
        serde_json::to_value(&recorded_graph).ok() != serde_json::to_value(&current.workflow).ok();
    let graph = if mode == ReplayMode::Recorded {
        recorded_graph
    } else {
        current.workflow.clone()
    };
    let selection = input
        .get("selection")
        .ok_or_else(|| "Choose all steps or an exact step tile.".to_owned())?;
    let part = match selection.get("kind").and_then(Value::as_str) {
        Some("all") => None,
        Some(kind @ ("step" | "onward")) => {
            let tile = selection
                .get("step_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "Choose a step tile ID, not a session or attempt.".to_owned())?;
            if !graph.steps.iter().any(|step| step.id() == tile) {
                return Err("That is not a step tile in this workflow. A tile repeat includes all its copies; a session or attempt cannot silently select the tile.".to_owned());
            }
            Some(if kind == "step" {
                Part::Just(vec![tile.to_owned()])
            } else {
                Part::Onward(tile.to_owned())
            })
        }
        _ => return Err("Choose all, step or onward explicitly.".to_owned()),
    };
    let unrolled = crate::workflow::unroll::unroll(&graph);
    let wanted = super::run::which_nodes(&unrolled, &graph, part.as_ref());
    let mut effective = BTreeMap::new();
    let mut skills = BTreeMap::new();
    let memory_sources = if mode == ReplayMode::Recorded {
        super::memory_sources::read_bound(&source_dir)
            .map_err(|error| unavailable(&error.to_string()))?
    } else {
        None
    };
    let mut policy_revisions = BTreeMap::from([(workflow.clone(), revision.clone())]);
    let agents = read_agent_directory(&home.join("agents")).map_err(|error| error.to_string())?;
    let mut configurations = Vec::new();
    for (node, selected) in unrolled.nodes.iter().zip(&wanted) {
        if !selected {
            continue;
        }
        let Step::Agent(step) = &graph.steps[node.step] else {
            continue;
        };
        let key = super::run::node_key_for(&step.id, node.turn, node.copy);
        let current_step = current.workflow.steps.iter().find_map(|one| match one {
            Step::Agent(one) if one.id == step.id => Some(one), _ => None,
        }).ok_or_else(|| format!("{} is no longer in the current workflow. Review its permissions before repeating it.", step.name))?;
        // 2026-09: sufit należy do DZISIEJSZEGO kafelka. Stary agent pozostawiony w bibliotece
        // nie może przywrócić praw odebranych przez zmianę agenta na tym kafelku.
        let (path, today) = agents
            .iter()
            .find_map(|(path, one)| {
                one.as_ref()
                    .ok()
                    .filter(|one| one.agent.id.to_string() == current_step.agent)
                    .map(|one| (path, one))
            })
            .ok_or_else(|| {
                format!(
                    "Today's permissions for {} are unavailable; nothing started.",
                    step.name
                )
            })?;
        policy_revisions.insert(path.clone(), today.revision.clone());
        let overrides: Overrides =
            serde_json::from_value(Value::Object(current_step.overrides.clone()))
                .map_err(|error| error.to_string())?;
        let ceiling = resolve(&today.agent, &overrides)
            .map_err(|error| error.to_string())?
            .agent;
        if mode == ReplayMode::Recorded {
            if let Some(snapshot) = &memory_sources {
                snapshot
                    .selected_for(&key)
                    .map_err(|error| unavailable(&error.to_string()))?;
            }
            let mut frozen = saved
                .get("steps")
                .and_then(Value::as_array)
                .and_then(|steps| {
                    steps.iter().find(|one| {
                        one.get("node_key").and_then(Value::as_str) == Some(key.as_str())
                    })
                })
                .and_then(|one| one.get("effective"))
                .cloned()
                .ok_or_else(|| {
                    unavailable(&format!("{} has no saved agent settings", step.name))
                })?;
            if let Some(object) = frozen.as_object_mut() {
                object.remove("toolsNote");
            }
            let agent: Agent = serde_json::from_value(frozen)
                .map_err(|_| unavailable("saved agent settings cannot be read"))?;
            permissions_within(&agent, &ceiling)?;
            let bundles = super::run::saved_skill_bundles(&source_dir, &key)
                .map_err(|error| unavailable(&error.to_string()))?;
            if !agent.skills.is_empty() && bundles.agent.is_none() {
                return Err(unavailable(&format!(
                    "{} has no saved skill files",
                    step.name
                )));
            }
            if !step.borrow.skills.is_empty() && bundles.borrowed.is_none() {
                return Err(unavailable(&format!(
                    "{} has no saved borrowed skills",
                    step.name
                )));
            }
            if step.borrow.agent.is_some() || step.borrow.learnings.is_some() {
                return Err(unavailable(
                    "selected native roles or learning notes were not saved as replayable source files",
                ));
            }
            configurations.push(json!({"nodeKey":key,"name":step.name,"model":agent.model,"runsWith":agent.runs_with,"fileAccess":agent.file_access}));
            skills.insert(key.clone(), bundles);
            effective.insert(key, agent);
        } else {
            configurations.push(json!({"nodeKey":key,"name":step.name,"model":ceiling.model,"runsWith":ceiling.runs_with,"fileAccess":ceiling.file_access}));
        }
    }
    let input_snapshot = if mode == ReplayMode::Recorded {
        let snapshot = super::input_snapshot::read(&source_dir)
            .map_err(|error| unavailable(&error.to_string()))?;
        if saved.pointer("/input_snapshot/id").and_then(Value::as_str) != Some(snapshot.id()) {
            return Err(unavailable(
                "the starting files do not belong to this saved run",
            ));
        }
        Some(snapshot)
    } else {
        None
    };
    let instructions = if mode == ReplayMode::Recorded {
        Some(
            crate::inherit::instructions::read_snapshot(&source_dir)
                .map_err(|error| unavailable(&error.to_string()))?,
        )
    } else {
        None
    };
    let copies = wanted.iter().filter(|&&one| one).count();
    let said = match mode {
        ReplayMode::Recorded => format!(
            "Repeat the saved setup: {copies} copies, using the saved starting files. Today's permissions still apply."
        ),
        ReplayMode::Current => format!(
            "Repeat the current setup: {copies} copies. This uses today's workflow and models, not a reconstruction of the saved setup."
        ),
    };
    let budget_said = budget_usd.map_or_else(
        || "No spending limit is set for this repeat.".to_owned(),
        |limit| {
            let amount = if (limit * 100.0).fract() == 0.0 {
                format!("{limit:.2}")
            } else {
                limit.to_string()
            };
            format!("Spending limit: ${amount} for this new run.")
        },
    );
    let configuration_said: Vec<String> = configurations
        .iter()
        .map(|one| {
            let text = |key| {
                one.get(key)
                    .and_then(Value::as_str)
                    .unwrap_or("Unavailable")
            };
            let vendor = match text("runsWith") {
                "claude-code" => "Claude Code",
                "codex" => "Codex",
                _ => "Unavailable app",
            };
            let access = match text("fileAccess") {
                "look-only" => "Look only",
                "ask-first" => "Ask first",
                "work-freely" => "Work freely",
                _ => "Unavailable access",
            };
            format!(
                "{} ({}): {}, {}, {}.",
                text("name"),
                text("nodeKey"),
                text("model"),
                vendor,
                access
            )
        })
        .collect();
    let differences_said = if changed {
        vec!["Today's workflow differs from this saved run. The setup selected above determines which steps will run.".to_owned()]
    } else {
        vec!["The workflow has not changed. The agent settings above are the settings selected for this repeat.".to_owned()]
    };
    let preview = json!({"mode":mode,"source":{"workspace":project,"runId":id},"title":graph.name,
        "copies":copies,"settings":configurations,"workflowChanged":changed,"costBoundUsd":budget_usd,
        "budgetSaid":budget_said,"configurationSaid":configuration_said,"differencesSaid":differences_said,
        "said":said,"limitations":["External services and model replies may differ.","The earlier native agent app context and version may not be available."]});
    Ok(Arc::new(ReplayMaterial {
        source: RunRef {
            workspace: project.to_path_buf(),
            run_id: id.to_owned(),
        },
        source_dir,
        mode,
        workflow,
        workflow_revision: revision,
        graph_bytes: serde_json::to_vec(&graph).map_err(|error| error.to_string())?,
        graph,
        task: saved
            .get("task")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_owned),
        budget_usd,
        part,
        input: input_snapshot,
        effective,
        skills,
        instructions,
        memory_sources,
        preview,
        source_revision: crate::durable_file::revision_of(&source_bytes),
        policy_revisions,
    }))
}

fn permissions_within(requested: &Agent, ceiling: &Agent) -> Result<(), String> {
    use crate::engine::drivers::Policy;
    let files = policy_of(requested.file_access);
    let allowed = policy_of(ceiling.file_access);
    let tools = match (&requested.tools, &ceiling.tools) {
        (_, Tools::Everything) => true,
        (Tools::Only(wanted), Tools::Only(allowed)) => {
            wanted.iter().all(|one| allowed.contains(one))
        }
        _ => false,
    };
    if !(files == allowed || files == Policy::ReadOnly || allowed == Policy::Unrestricted)
        || (requested.reaches_the_web && !ceiling.reaches_the_web)
        || !tools
        || requested
            .connections
            .iter()
            .any(|one| !ceiling.connections.contains(one))
        || (requested.agent_messages && !ceiling.agent_messages)
        || !crate::library::agents::service_access_within(
            &requested.service_access,
            &ceiling.service_access,
        )
    {
        return Err("The saved setup asks for more access than today's agent settings allow. Review those permissions first; nothing started.".to_owned());
    }
    Ok(())
}
