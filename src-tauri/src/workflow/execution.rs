//! WF-16: dane przygotowania grafu. Walidator i wykonawca czytają ten sam kontrakt.

use super::{Folder, Step, WorkflowFile};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Fakt końca wykonania. Historia i pomiary czytają wspólny typ bez zależności od commands.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EndCause {
    Completed,
    TaskFailed,
    Refused,
    InfrastructureFailed,
    LimitReached,
    Cancelled,
    DependencySkipped,
    BranchNotSelected,
    LoopSettled,
    UnprovenStop,
    #[serde(other)]
    Unknown,
}

/// Kod z zaakceptowanego przypadku, nie ścieżka do testu modyfikowanego przez subject.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Examiner {
    Python {
        program: std::path::PathBuf,
        source: String,
    },
    #[serde(other)]
    Unknown,
}

impl std::fmt::Debug for Examiner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Examiner").finish_non_exhaustive()
    }
}

impl Examiner {
    pub fn from_check(
        extra: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<Option<Self>, String> {
        let Some(value) = extra.get("examiner") else {
            return Ok(None);
        };
        if extra.get("proofMode").and_then(serde_json::Value::as_str)
            != Some("external-assessment-v1")
        {
            return Err("A frozen examiner must return the independent assessment result, not an output pattern.".to_owned());
        }
        Self::read(value).map(Some)
    }

    pub fn read(value: &serde_json::Value) -> Result<Self, String> {
        let definition: Self = serde_json::from_value(value.clone())
            .map_err(|_| "The independent examiner needs a supported kind, an absolute interpreter path and its accepted source code.".to_owned())?;
        match &definition {
            Self::Python { program, source } if program.is_absolute()
                && !source.trim().is_empty() && source.len() <= 256 * 1024 => Ok(definition),
            _ => Err("Choose a supported independent examiner with an absolute interpreter path and source code within 256 KiB.".to_owned()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunInputs {
    pub schema: u32,
    /// WF-18: egzekwowana granica procesu, nie tylko routing przekazań.
    #[serde(default)]
    pub isolate_contexts: bool,
    #[serde(default)]
    pub checks: BTreeMap<String, CheckInput>,
    #[serde(default)]
    pub workspace_seeds: BTreeMap<String, WorkspaceSeed>,
    #[serde(default)]
    pub contexts: BTreeMap<String, ContextScope>,
    #[serde(default)]
    pub step_contexts: BTreeMap<String, String>,
    #[serde(default)]
    pub effects: RunEffects,
}

impl Default for RunInputs {
    fn default() -> Self {
        Self {
            schema: 1,
            isolate_contexts: false,
            checks: BTreeMap::new(),
            workspace_seeds: BTreeMap::new(),
            contexts: BTreeMap::new(),
            step_contexts: BTreeMap::new(),
            effects: RunEffects::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckInput {
    pub results: Vec<String>,
}

/// Adres plików już utrwalonych przez gospodarza. Nie jest dowolną ścieżką Pick.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceSeed {
    pub source_run_id: String,
    pub snapshot_id: String,
}

/// Jawny kontekst jest domyślnie pusty poza zadaniem; nic nie dziedziczy po gospodarzu.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextScope {
    pub task: String,
    #[serde(default)]
    pub workspace_seed: Option<String>,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub project_instructions: bool,
    #[serde(default)]
    pub project_memory: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunEffects {
    #[serde(default = "yes")]
    pub publish_memory: bool,
    #[serde(default = "yes")]
    pub external_messages: bool,
}
fn yes() -> bool {
    true
}
impl Default for RunEffects {
    fn default() -> Self {
        Self {
            publish_memory: true,
            external_messages: true,
        }
    }
}

impl RunInputs {
    pub fn from_graph(file: &WorkflowFile) -> Result<Self, String> {
        let inputs: Self = match file.extra.get("executionInputs") {
            None => Self::default(),
            Some(value) => serde_json::from_value(value.clone()).map_err(|error| {
                format!("The workflow's execution inputs could not be read: {error}")
            })?,
        };
        if inputs.schema != 1 {
            return Err("This version of execution inputs is not supported.".to_owned());
        }
        for step in &file.steps {
            if let Step::Check(check) = step
                && Examiner::from_check(&check.extra)?.is_some()
                && !inputs.isolate_contexts
            {
                return Err("Frozen independent examiners require protected file access. Enable protection before starting this workflow.".to_owned());
            }
        }
        if inputs.contexts.len() > 100 {
            return Err("A run can contain at most 100 independent contexts.".to_owned());
        }
        if inputs.workspace_seeds.len() > 100 {
            return Err("A run can contain at most 100 saved inputs.".to_owned());
        }
        for (key, seed) in &inputs.workspace_seeds {
            if !safe_key(key)
                || uuid::Uuid::parse_str(&seed.source_run_id).is_err()
                || uuid::Uuid::parse_str(&seed.snapshot_id).is_err()
            {
                return Err(
                    "A saved input needs a valid name, run ID and snapshot ID, not a folder path."
                        .to_owned(),
                );
            }
        }
        let mut bytes = 0usize;
        for (key, context) in &inputs.contexts {
            if !safe_key(key) {
                return Err("A context name must use only letters, numbers, underscores or hyphens (1–80 characters).".to_owned());
            }
            if context
                .workspace_seed
                .as_ref()
                .is_some_and(|seed| !inputs.workspace_seeds.contains_key(seed))
            {
                return Err(format!(
                    "Context {key} names an input that is not part of this run."
                ));
            }
            bytes = bytes
                .saturating_add(context.task.len())
                .saturating_add(context.instructions.len());
        }
        if bytes > 512 * 1024 {
            return Err("The run's context inputs exceed 512 KiB.".to_owned());
        }
        for (step, scope) in &inputs.step_contexts {
            if !file.steps.iter().any(|one| one.id() == step)
                || !inputs.contexts.contains_key(scope)
            {
                return Err(format!(
                    "The context assignment for {step} does not name a step and a context in this workflow."
                ));
            }
        }
        for link in &file.links {
            if inputs.context_key(&link.from) == inputs.context_key(&link.to) {
                continue;
            }
            // Przekazanie między zakresami jest możliwe jedynie jako jawne dane Check stdin.
            // Zwykła strzałka nie przyznaje dostępu do katalogu cudzych wyników.
            if !inputs.checks.contains_key(&link.to) {
                return Err("Steps in different contexts need an explicit check input; their working folders and notes are not shared.".to_owned());
            }
            if file.steps.iter().find(|step| step.id() == link.to)
                .is_some_and(|step| matches!(step, Step::Check(check) if matches!(check.folder, Folder::SameCopy))) {
                return Err("A check receiving another context's result must use its own working folder.".to_owned());
            }
        }
        Ok(inputs)
    }

    #[must_use]
    pub fn context_key(&self, tile: &str) -> Option<&str> {
        self.step_contexts.get(tile).map(String::as_str)
    }
    #[must_use]
    pub fn context_for(&self, tile: &str) -> Option<&ContextScope> {
        self.contexts.get(self.context_key(tile)?)
    }

    /// Pierwszy Project jest właścicielem zarządzanej kopii tego zakresu. Tożsamość zależy
    /// od grafu, nie kolejności kończenia ani nazwy „Implement”.
    #[must_use]
    pub fn project_owner<'a>(&self, file: &'a WorkflowFile, tile: &str) -> Option<&'a str> {
        let scope = self.context_key(tile)?;
        file.steps
            .iter()
            .find(|step| {
                self.context_key(step.id()) == Some(scope)
                    && match step {
                        Step::Agent(one) => matches!(one.folder, Folder::Project),
                        Step::Check(one) => matches!(one.folder, Folder::Project),
                        Step::Serve(one) => matches!(one.folder, Folder::Project),
                        Step::Checkpoint(_) => false,
                    }
            })
            .map(Step::id)
    }
}

fn safe_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 80
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}
