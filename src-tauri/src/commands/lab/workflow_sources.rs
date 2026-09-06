//! Jawne źródło kolumny: odczyt dokładnej półki, pliku i zaakceptowanej rewizji.
use super::LabError;
use crate::commands::workflows::{self, WorkflowPlace};
use crate::engine::supervisor::PublicationRoot;
use crate::{lab::EvalSet, workflow::WorkflowFile};
use sha2::{Digest, Sha256};
use std::path::Path;

pub(super) struct ResolvedSources {
    pub graphs: Vec<WorkflowFile>,
    pub revision: String,
}

pub(super) fn resolve(
    library: &Path,
    project: &Path,
    set: &EvalSet,
) -> Result<ResolvedSources, LabError> {
    let definitions = workflows::list_workflow_definitions_readonly(library, Some(project))
        .map_err(|error| LabError::NotReady(error.to_string()))?;
    let catalog = crate::library::definition::healthy_only(definitions);
    let mut bindings = Vec::new();
    let graphs = set.variants.iter().map(|variant| {
        let source = crate::lab::workflow_plan::source(variant).map_err(LabError::NotReady)?;
        let (place, path) = match (&source.place, &source.path) {
            (Some(place), Some(path)) => (match place.as_str() {
                "library" => WorkflowPlace::Library,
                "project" => WorkflowPlace::Project,
                _ => return Err(LabError::NotReady("Choose the project or library as the workflow source.".into())),
            }, path.clone()),
            (None, None) => {
                let mut matches = catalog.iter().filter(|entry| entry.workflow.id == source.id);
                let entry = matches.next().ok_or_else(|| LabError::NotReady(format!("The workflow {} is unavailable. Select its source again.", source.id)))?;
                if matches.next().is_some() {
                    return Err(LabError::NotReady("More than one workflow has this ID. Select its exact source.".into()));
                }
                (entry.place, entry.path.clone())
            }
            _ => return Err(LabError::NotReady("A workflow source needs both its file name and its project or library location.".into())),
        };
        // Odczyt tylko jednej wskazanej półki, bez fallbacku do pliku przesłaniającego ją
        // w projekcie. Istniejący parser dostaje jeden no-follow odczyt; podgląd nie
        // uruchamia recovery ani nie usuwa tempów pozostawionych przez inny zapis.
        let shelf = match place {
            WorkflowPlace::Library => library.to_path_buf(),
            WorkflowPlace::Project => project.join(".loadout"),
        };
        let address = workflows::where_it_lives(&shelf, None, &path)
            .map_err(|error| LabError::NotReady(error.to_string()))?;
        let dir = address.path.parent().ok_or_else(|| LabError::NotReady("The workflow source has no containing folder.".to_owned()))?;
        let root = PublicationRoot::open(dir)?;
        let bytes = root.read_regular(Path::new(&path), false)?;
        let workflow = crate::workflow::file::load_snapshot(&address.path, &bytes)
            .map_err(|error| LabError::NotReady(error.to_string()))?;
        root.validate_path_identity(dir)?;
        let open = workflows::OpenWorkflow { workflow, revision: crate::durable_file::revision_of(&bytes) };
        if open.workflow.id != source.id || source.revision.as_ref().is_some_and(|expected| expected != &open.revision) {
            return Err(LabError::NotReady("The selected workflow changed. Review and select its current revision before running the comparison.".into()));
        }
        // Digest liczymy z rewizji TEGO odczytu, którego graf trafia do compose, także
        // dla starszych kolumn bez przypiętej rewizji. Nie powstaje drugi rejestr źródeł.
        bindings.push(serde_json::json!({"variant":variant.id,"place":source.place.as_deref().unwrap_or(match place {
            WorkflowPlace::Library => "library", WorkflowPlace::Project => "project",
        }),"path":path,"revision":open.revision}));
        Ok(open.workflow)
    }).collect::<Result<Vec<_>, LabError>>()?;
    let bytes =
        serde_json::to_vec(&bindings).map_err(|error| LabError::NotReady(error.to_string()))?;
    Ok(ResolvedSources {
        graphs,
        revision: format!("{:x}", Sha256::digest(bytes)),
    })
}
