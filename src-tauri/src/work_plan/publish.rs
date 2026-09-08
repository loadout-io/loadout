//! Publikacja jest dwuczęściowa: niezmienna wersja pierwsza, ruchomy wskaźnik ostatni.
//!
//! Awaria między nimi może zostawić dodatkowy, nieosiągalny plik wersji, ale nie może zmienić
//! `current.json`. To celowe: poprzednia wersja pozostaje wtedy kompletna i nadal aktualna,
//! a ponowienie tej samej operacji może dokończyć sam wskaźnik bez wybicia kolejnej wersji.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::de::DeserializeOwned;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::durable_file::{
    DEFINITION_FILE_MODE, DurableFilePublisher, ModePolicy, PublishError, revision_of,
};

use super::{Error, LATEST_SCHEMA, PlanDocument, RENDERER, SCHEMA};

const VERSIONS: &str = "versions";
const CURRENT: &str = "current.json";
const VERSION_FILE_MODE: u32 = 0o444;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stamp {
    pub run_id: String,
    pub document_id: String,
    pub step_id: String,
    pub attempt: u32,
    pub operation: String,
    pub parent: Option<String>,
    pub at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlanVersion {
    pub schema: u32,
    pub renderer: u32,
    pub run_id: String,
    pub document_id: String,
    pub version: u64,
    pub version_id: String,
    pub parent: Option<String>,
    pub step_id: String,
    pub attempt: u32,
    pub operation: String,
    pub at: String,
    pub document: PlanDocument,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Publication {
    Published(PlanVersion),
    Unchanged(PlanVersion),
    AlreadyPublished(PlanVersion),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct CurrentPointer {
    schema: u32,
    version: u64,
    version_id: String,
    file: String,
}

#[must_use]
pub fn plan_root(home: &Path, run_id: &str, document_id: &str) -> PathBuf {
    home.join("runs")
        .join(run_id)
        .join("plans")
        .join(document_id)
}

pub fn publish_version(
    root: &Path,
    stamp: &Stamp,
    document: &PlanDocument,
) -> Result<Publication, Error> {
    if stamp.operation.is_empty() {
        return Err(Error::Malformed(
            "the application did not provide an operation identifier".to_owned(),
        ));
    }

    let versions = read_versions(root)?;
    let current = read_current_state(root)?;
    if let Some(existing) = versions
        .iter()
        .find(|version| version.operation == stamp.operation)
        .cloned()
    {
        if !same_submission(&existing, stamp, document) {
            return Err(Error::Malformed(format!(
                "operation {:?} already names a different plan version",
                stamp.operation
            )));
        }
        if current.as_ref().is_some_and(|state| {
            version_is_reachable(&state.version, &versions, &existing.version_id)
        }) {
            return Ok(Publication::AlreadyPublished(existing));
        }
        match current.as_ref() {
            Some(state)
                if existing.parent.as_deref() == Some(state.version.version_id.as_str()) =>
            {
                publish_pointer(root, &existing, Some(&state.revision))?;
            }
            // 2026-09-08 — pierwsza wersja też może zdążyć wylądować przed awarią pointera.
            // Jej legalnym rodzicem jest brak pliku, więc retry kończy tę samą operację.
            None if existing.parent.is_none() => publish_pointer(root, &existing, None)?,
            _ => return Err(Error::Stale),
        }
        return Ok(Publication::Published(existing));
    }

    let expected_parent = current
        .as_ref()
        .map(|state| state.version.version_id.as_str());
    if stamp.parent.as_deref() != expected_parent {
        return Err(Error::Stale);
    }
    if let Some(state) = current.as_ref()
        && state.version.document == *document
    {
        return Ok(Publication::Unchanged(state.version.clone()));
    }

    let version_number = match current.as_ref() {
        Some(state) => state.version.version.checked_add(1).ok_or_else(|| {
            Error::Malformed("the current plan has no representable next version".to_owned())
        })?,
        None => 1,
    };
    let version = PlanVersion {
        schema: SCHEMA,
        renderer: RENDERER,
        run_id: stamp.run_id.clone(),
        document_id: stamp.document_id.clone(),
        version: version_number,
        version_id: crate::commands::mint::new_id_inner().to_string(),
        parent: stamp.parent.clone(),
        step_id: stamp.step_id.clone(),
        attempt: stamp.attempt,
        operation: stamp.operation.clone(),
        at: stamp.at.clone(),
        document: document.clone(),
    };
    let file = version_file_name(version_number, &stamp.operation);
    let version_text = as_file(&version)?;
    let pointer_text = as_file(&CurrentPointer {
        schema: SCHEMA,
        version: version.version,
        version_id: version.version_id.clone(),
        file: file.clone(),
    })?;

    fs::create_dir_all(root.join(VERSIONS))?;
    DurableFilePublisher::new(root)
        .with_publication(|batch| {
            // 2026-09-08 — wersja idzie pierwsza, bo wskaźnik nigdy nie może nazywać pliku,
            // którego jeszcze nie ma. Awaria po tej linii zostawia stary wskaźnik nietknięty.
            batch.atomic_create_if_absent(
                &root.join(VERSIONS).join(&file),
                version_text.as_bytes(),
                ModePolicy::Exact(VERSION_FILE_MODE),
            )?;
            batch.publish_definition(
                &root.join(CURRENT),
                pointer_text.as_bytes(),
                ModePolicy::PreserveExistingOr(DEFINITION_FILE_MODE),
                current.as_ref().map(|state| state.revision.as_str()),
            )
        })
        .map_err(publication_error)?;
    Ok(Publication::Published(version))
}

pub fn read_versions(root: &Path) -> Result<Vec<PlanVersion>, Error> {
    let folder = root.join(VERSIONS);
    let entries = match fs::read_dir(&folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(Error::Unwritable(error)),
    };
    let mut versions = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type()?.is_file()
            || path.extension().and_then(|extension| extension.to_str()) != Some("json")
        {
            continue;
        }
        let version: PlanVersion = read_json(&path)?;
        validate_version(&version)?;
        versions.push(version);
    }
    versions.sort_by(|left, right| {
        left.version
            .cmp(&right.version)
            .then_with(|| left.version_id.cmp(&right.version_id))
    });
    Ok(versions)
}

pub fn read_current_version(root: &Path) -> Result<PlanVersion, Error> {
    read_current_state(root)?
        .map(|state| state.version)
        .ok_or(Error::NoSuchPlan)
}

#[derive(Debug)]
struct CurrentState {
    version: PlanVersion,
    revision: String,
}

fn read_current_state(root: &Path) -> Result<Option<CurrentState>, Error> {
    let path = root.join(CURRENT);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Error::Unwritable(error)),
    };
    let pointer: CurrentPointer = serde_json::from_slice(&bytes)?;
    if pointer.schema != LATEST_SCHEMA || !safe_file_name(&pointer.file) {
        return Err(Error::Malformed(
            "current.json names an unsupported schema or an unsafe version file".to_owned(),
        ));
    }
    let version: PlanVersion = read_json(&root.join(VERSIONS).join(&pointer.file))?;
    validate_version(&version)?;
    if version.version != pointer.version
        || version.version_id != pointer.version_id
        || pointer.file != version_file_name(version.version, &version.operation)
    {
        return Err(Error::Malformed(
            "current.json does not identify the version file it names".to_owned(),
        ));
    }
    Ok(Some(CurrentState {
        version,
        revision: revision_of(&bytes),
    }))
}

fn publish_pointer(
    root: &Path,
    version: &PlanVersion,
    expected: Option<&str>,
) -> Result<(), Error> {
    let file = version_file_name(version.version, &version.operation);
    let text = as_file(&CurrentPointer {
        schema: SCHEMA,
        version: version.version,
        version_id: version.version_id.clone(),
        file,
    })?;
    DurableFilePublisher::new(root)
        .publish_definition(
            &root.join(CURRENT),
            text.as_bytes(),
            ModePolicy::PreserveExistingOr(DEFINITION_FILE_MODE),
            expected,
        )
        .map_err(publication_error)
}

fn version_file_name(version: u64, operation: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(operation.as_bytes()));
    format!("{version:04}-{}.json", &digest[..8])
}

fn version_is_reachable(current: &PlanVersion, versions: &[PlanVersion], wanted: &str) -> bool {
    let mut cursor = Some(current);
    let mut visited = BTreeSet::new();
    while let Some(version) = cursor {
        if version.version_id == wanted {
            return true;
        }
        if !visited.insert(&version.version_id) {
            return false;
        }
        cursor = version.parent.as_ref().and_then(|parent| {
            versions
                .iter()
                .find(|candidate| candidate.version_id == *parent)
        });
    }
    false
}

fn same_submission(version: &PlanVersion, stamp: &Stamp, document: &PlanDocument) -> bool {
    version.run_id == stamp.run_id
        && version.document_id == stamp.document_id
        && version.step_id == stamp.step_id
        && version.attempt == stamp.attempt
        && version.operation == stamp.operation
        && version.parent == stamp.parent
        && version.at == stamp.at
        && version.document == *document
}

fn validate_version(version: &PlanVersion) -> Result<(), Error> {
    if version.schema != LATEST_SCHEMA || version.renderer != RENDERER {
        return Err(Error::Malformed(format!(
            "plan version {} uses an unsupported schema or renderer",
            version.version_id
        )));
    }
    Ok(())
}

fn safe_file_name(file: &str) -> bool {
    let mut components = Path::new(file).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn as_file<T: Serialize>(value: &T) -> Result<String, Error> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    Ok(text)
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, Error> {
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(Error::from)
}

fn publication_error(error: PublishError) -> Error {
    match error {
        PublishError::Changed { .. } | PublishError::Conflict { .. } => Error::Stale,
        other => Error::Unwritable(other.into_io()),
    }
}
