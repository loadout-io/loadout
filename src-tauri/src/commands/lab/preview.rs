//! WF-19: odczyt tego, co wystartuje, bez publikowania grafu ani zakładania biegu.
use super::{Drivers, LabError};
use crate::engine::supervisor::{FilesystemFence, PublicationRoot};
use crate::library::agents::{Agent, Overrides, Vendor, read_agent_directory_from_root, resolve};
use crate::workflow::{Step, execution::Examiner};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRun {
    pub set: String,
    pub revision: String,
    pub source_revision: Option<String>,
    pub protected: bool,
    pub size: Option<crate::lab::workflow_plan::ComparisonSize>,
    pub cannot_run: Option<String>,
}

pub async fn preview_run_inner(
    library: &Path,
    project: &Path,
    set: &str,
    expected_revision: Option<&str>,
    drivers: &Drivers,
) -> Result<PreviewRun, LabError> {
    let library = library.to_path_buf();
    let project = project.to_path_buf();
    let set = set.to_owned();
    let expected_revision = expected_revision.map(str::to_owned);
    let (mut preview, vendors) = tokio::task::spawn_blocking(move || {
        prepare(&library, &project, &set, expected_revision.as_deref())
    })
    .await
    .map_err(|error| {
        LabError::Unreadable(format!("The comparison preview could not finish: {error}"))
    })??;
    if preview.cannot_run.is_some() {
        return Ok(preview);
    }
    for vendor in vendors {
        let driver = drivers(vendor);
        let ready = async {
            let probe = driver.probe().await.map_err(|error| format!("The agent app could not be checked: {error}"))?;
            if !probe.found {
                return Err("An agent app needed by this comparison is unavailable. Install it or choose an available agent before running this comparison.".to_owned());
            }
            if preview.protected {
                driver.protected_readiness().ok_or_else(||
                    "An agent app in this comparison does not support restricted file access. This comparison cannot start with protection enabled.".to_owned())?
                    .map_err(|error| format!("Protected file access is not ready for an agent app in this comparison: {error}"))?;
            }
            Ok::<_, String>(())
        }.await;
        if let Err(reason) = ready {
            preview.cannot_run = Some(reason);
            return Ok(preview);
        }
    }
    if preview.protected {
        // Tylko dowód dostępności supervisora. prepare_protected_step tworzy prywatny
        // runtime i dlatego nie należy do podglądu; Start powtarza właściwe przygotowanie.
        let proof = async {
            FilesystemFence::new(vec![], vec![], vec![])?
                .prove_available()
                .await
        }
        .await;
        if let Err(error) = proof {
            preview.cannot_run = Some(format!(
                "Protected file access is unavailable on this computer: {error}"
            ));
        }
    }
    Ok(preview)
}

pub(super) fn check_revision(actual: &str, expected: Option<&str>) -> Result<(), LabError> {
    if expected.is_some_and(|expected| expected != actual) {
        return Err(LabError::NotReady(
            "This set changed after preview. Check this comparison again before running it."
                .to_owned(),
        ));
    }
    Ok(())
}

fn prepare(
    library: &Path,
    project: &Path,
    set: &str,
    expected: Option<&str>,
) -> Result<(PreviewRun, Vec<Vendor>), LabError> {
    let open = super::read_set_inner(project, set)?;
    check_revision(&open.revision, expected)?;
    let mut preview = PreviewRun {
        set: set.to_owned(),
        revision: open.revision,
        source_revision: None,
        protected: open
            .set
            .extra
            .get("protected")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        size: None,
        cannot_run: open.set.why_it_cannot_run(),
    };
    if preview.cannot_run.is_some() {
        return Ok((preview, Vec::new()));
    }
    let mut vendors = Vec::new();
    let checked = (|| -> Result<(), String> {
        if !matches!(open.set.subject, crate::lab::Subject::Workflow { .. }) {
            return Err("This preview is available for workflow comparisons.".to_owned());
        }
        let sources = super::workflow_sources::resolve(library, project, &open.set)
            .map_err(|error| error.to_string())?;
        preview.source_revision = Some(sources.revision);
        let (size, subjects) = crate::lab::workflow_plan::preflight_selected_with_subjects(
            &sources.graphs,
            &open.set,
        )?;
        preview.size = Some(size);
        check_examiners(&open.set)?;
        let agents = read_agents(library)?;
        for graph in subjects {
            for step in graph.steps {
                let Step::Agent(step) = step else { continue };
                let saved = agents.iter().find(|agent| agent.id.to_string() == step.agent)
                    .ok_or_else(|| format!("The agent selected for {} is unavailable. Select a saved agent before running this comparison.", step.name))?;
                let overrides: Overrides =
                    serde_json::from_value(serde_json::Value::Object(step.overrides))
                        .map_err(|error| error.to_string())?;
                let vendor = resolve(saved, &overrides)
                    .map_err(|error| error.to_string())?
                    .agent
                    .runs_with;
                if !vendors.contains(&vendor) {
                    vendors.push(vendor);
                }
            }
        }
        Ok(())
    })();
    if let Err(reason) = checked {
        preview.cannot_run = Some(reason);
    }
    Ok((preview, vendors))
}

/// WF-19 (2026-09-06): ta sama odmowa w podglądzie i przed płatnym planem. Sprawdzamy
/// dostępność pliku, nie poprawność Pythona ani kodu — żaden program tutaj nie biegnie.
/// Dowiązania systemowych interpreterów podążają tą samą drogą co późniejsze wykonanie.
pub(super) fn check_examiners(set: &crate::lab::EvalSet) -> Result<(), String> {
    for case in set.running_cases() {
        let Some(examiner) = Examiner::from_check(&case.extra)? else {
            continue;
        };
        let Examiner::Python { program, .. } = examiner else {
            return Err(
                "Choose a supported independent check interpreter before running this comparison."
                    .to_owned(),
            );
        };
        if !std::fs::metadata(&program).is_ok_and(|metadata| {
            metadata.is_file() && crate::engine::supervisor::executable_bits(&metadata)
        }) {
            return Err(format!(
                "The interpreter selected for \"{}\" is missing or cannot be executed. Choose an available interpreter before running this comparison.",
                case.name
            ));
        }
    }
    Ok(())
}

fn read_agents(library: &Path) -> Result<Vec<Agent>, String> {
    let dir = library.join("agents");
    let root = match PublicationRoot::open(&dir) {
        Ok(root) => root,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    let entries = read_agent_directory_from_root(&root, &dir).map_err(|error| error.to_string())?;
    root.validate_path_identity(&dir)
        .map_err(|error| error.to_string())?;
    Ok(entries
        .into_iter()
        .filter_map(|(_, read)| read.ok().map(|read| read.agent))
        .collect())
}
