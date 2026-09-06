//! WF-16: wspólny czytnik pochodzenia kopii. Pliki i ich pełne odciski, nie `SQLite`.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::input_snapshot::{self, InputSnapshot};
use crate::engine::supervisor::PublicationRoot;
use crate::workflow::execution::RunInputs;

#[derive(Debug, Clone)]
pub(crate) struct FrozenSeed {
    pub input: InputSnapshot,
    pub record: SeedRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SeedRecord {
    pub id: String,
    pub directory: PathBuf,
}

pub(crate) type Bound = BTreeMap<String, FrozenSeed>;

/// Wywoływane po layout katalogu, przed pierwszym procesem. Każdy seed kopiuje bajty raz;
/// kilka komórek dostaje jeden odcisk wejścia, ale osobne zarządzane katalogi robocze.
pub(crate) fn prepare(
    project: &Path,
    run_dir: &Path,
    inputs: &RunInputs,
    needed: &BTreeSet<&str>,
    previous: Option<&Path>,
) -> io::Result<Bound> {
    let root = PublicationRoot::open(run_dir)?;
    let mut frozen = BTreeMap::new();
    for (name, wanted) in &inputs.workspace_seeds {
        if !needed.contains(name.as_str()) {
            continue;
        }
        let directory = PathBuf::from("seeds").join(name);
        let input = if let Some(previous) = previous {
            // Replay czy retry nie wraca do źródła seed po jego retencji. Własne bajty
            // poprzedniego biegu są jedynym źródłem, także gdy host już się zmienił.
            let records = records(previous)?;
            let record = records
                .values()
                .find(|record| record.directory == directory && record.id == wanted.snapshot_id)
                .ok_or_else(|| {
                    io::Error::other("The earlier run's saved workspace input is unavailable.")
                })?;
            read_record(previous, record)?
        } else {
            let source = super::lead_history::source_for(project, &wanted.source_run_id)
                .map_err(io::Error::other)?;
            let bytes =
                super::lead_history::source_bytes(project, &source).map_err(io::Error::other)?;
            let file: Value = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
            if file.pointer("/input_snapshot/id").and_then(Value::as_str)
                != Some(wanted.snapshot_id.as_str())
            {
                return Err(io::Error::other(
                    "The chosen input does not belong to that saved run.",
                ));
            }
            let input =
                input_snapshot::read(&project.join(".loadout/runs").join(&source.run_folder))?;
            if input.id() != wanted.snapshot_id {
                return Err(io::Error::other("The chosen input changed identity."));
            }
            input
        };
        root.ensure_directory(&directory, 0o700)?;
        let input = input.copy_to(&run_dir.join(&directory))?;
        frozen.insert(
            name.clone(),
            FrozenSeed {
                record: SeedRecord {
                    id: input.id().to_owned(),
                    directory,
                },
                input,
            },
        );
    }
    Ok(frozen)
}

fn description(run_dir: &Path) -> io::Result<Value> {
    let file = PublicationRoot::open(run_dir)?.open_regular_file(Path::new("run.json"))?;
    let mut bytes = Vec::new();
    file.take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(io::Error::other("The saved run description is too large."));
    }
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

pub(crate) fn records(run_dir: &Path) -> io::Result<BTreeMap<String, SeedRecord>> {
    let file = description(run_dir)?;
    file.get("workspace_inputs")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map(Option::unwrap_or_default)
        .map_err(io::Error::other)
}

fn read_record(run_dir: &Path, record: &SeedRecord) -> io::Result<InputSnapshot> {
    let parts: Vec<_> = record.directory.components().collect();
    if parts.len() != 2
        || parts[0] != Component::Normal("seeds".as_ref())
        || !matches!(parts[1], Component::Normal(_))
    {
        return Err(io::Error::other(
            "A saved input points outside this run's private input folders.",
        ));
    }
    let held = PublicationRoot::open(&run_dir.join(&record.directory))?;
    let input = input_snapshot::read(&run_dir.join(&record.directory))?;
    held.validate_path_identity(&run_dir.join(&record.directory))?;
    if input.id() != record.id {
        return Err(io::Error::other(
            "The saved workspace input changed identity.",
        ));
    }
    Ok(input)
}

pub(crate) fn for_copy(run_dir: &Path, key: &str) -> io::Result<InputSnapshot> {
    if let Some(record) = records(run_dir)?.get(key) {
        read_record(run_dir, record)
    } else {
        let file = description(run_dir)?;
        if let Some(graph) = file
            .get("workflow_snapshot")
            .filter(|graph| graph.get("executionInputs").is_some())
        {
            let graph: crate::workflow::WorkflowFile =
                serde_json::from_value(graph.clone()).map_err(io::Error::other)?;
            let inputs = RunInputs::from_graph(&graph).map_err(io::Error::other)?;
            if inputs
                .context_for(super::run::tile_key_of(key))
                .is_some_and(|scope| scope.workspace_seed.is_some())
            {
                return Err(io::Error::other(
                    "The recorded workspace input is missing. Today's project input was not substituted.",
                ));
            }
        }
        input_snapshot::read(run_dir)
    }
}

pub(crate) fn wire(bound: &Bound) -> BTreeMap<String, SeedRecord> {
    bound
        .iter()
        .map(|(key, seed)| (key.clone(), seed.record.clone()))
        .collect()
}

pub(crate) fn read_bound(run_dir: &Path) -> io::Result<Bound> {
    records(run_dir)?
        .into_iter()
        .map(|(key, record)| {
            let input = read_record(run_dir, &record)?;
            Ok((key, FrozenSeed { input, record }))
        })
        .collect()
}
