//! Prywatny pakiet planu związany z biegiem i jego fizycznymi odbiorcami.

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read as _, Write as _};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::durable_file::{
    DEFINITION_FILE_MODE, DurableFilePublisher, ModePolicy, PUBLISHED_HANDOFF_MODE,
};
use crate::engine::supervisor::{PrivateFileAccess, free_bytes, open_private_file};

use super::{
    DOCUMENT_ID, PlanVersion, read_current_version, read_versions, render_core, render_details,
};

const PLAN_ROOT: &str = "plans/workflow-plan";
const CURRENT: &str = "current.json";
const VERSIONS: &str = "versions";
const READER_LOCK: &str = "work-plan.reader.lock";
const READS: &str = "plan-reads";
const MAX_FILE_BYTES: usize = 512 * 1024;
const MAX_PACKAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_VERSIONS: usize = 4096;
const MAX_READ_LOG_BYTES: usize = 4 * 1024 * 1024;
const MAX_READ_LOG_ENTRIES: usize = 4096;

type PackageFiles = Vec<(String, Vec<u8>)>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub id: String,
    pub digest: String,
}

#[derive(Clone)]
pub struct Snapshot {
    binding: Binding,
    versions: BTreeMap<String, PlanVersion>,
    pinned: BTreeMap<String, String>,
    source_dir: Option<Arc<PathBuf>>,
    files: Option<Arc<PackageFiles>>,
    _source_holds: Option<Arc<Vec<Arc<File>>>>,
}

impl fmt::Debug for Snapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecordedWorkPlan")
            .field("id", &self.binding.id)
            .field("versions", &self.versions.len())
            .field("recipients", &self.pinned.len())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SavedNode {
    pub source_run_id: String,
    pub source_node_key: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Delivery {
    pub version: u64,
    pub version_id: String,
    pub core_bytes: usize,
    pub detail_bytes: usize,
    pub opened_bytes: usize,
    pub opened_count: usize,
}

#[derive(Debug)]
pub(crate) struct Recorder {
    run_dir: PathBuf,
    node_key: String,
    version_id: String,
    writes: Mutex<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Opened {
    schema: u32,
    version_id: String,
    bytes: usize,
}

#[derive(Debug, Deserialize)]
struct RunRecord {
    #[serde(default)]
    plan_sources: Option<Binding>,
    #[serde(default)]
    steps: Vec<StepRecord>,
}

#[derive(Debug, Deserialize)]
struct StepRecord {
    #[serde(default)]
    id: String,
    #[serde(default)]
    node_key: String,
    #[serde(default)]
    plan_version: Option<PlanReceipt>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanReceipt {
    document_id: String,
    version: u64,
    version_id: String,
}

impl Snapshot {
    #[must_use]
    pub fn binding(&self) -> Binding {
        self.binding.clone()
    }

    pub fn pinned_for(&self, node_key: &str) -> io::Result<PlanVersion> {
        let version_id = self
            .pinned
            .get(node_key)
            .ok_or_else(|| unavailable("the selected step has no saved plan version"))?;
        self.versions
            .get(version_id)
            .cloned()
            .ok_or_else(|| unavailable("the selected step points to a missing saved plan version"))
    }

    pub fn copy_to(&self, run_dir: &Path) -> io::Result<()> {
        let files = if let Some(source_dir) = &self.source_dir {
            let _hold = hold(source_dir)?;
            if binding(source_dir)?.as_ref() != Some(&self.binding) {
                return Err(unavailable(
                    "the saved plan changed before it could be copied",
                ));
            }
            package_files(source_dir)?
        } else {
            self.files
                .as_deref()
                .cloned()
                .ok_or_else(|| unavailable("the frozen plan has no publication files"))?
        };
        let needed = files.iter().try_fold(0_u64, |total, (_, bytes)| {
            total
                .checked_add(bytes.len() as u64)
                .ok_or_else(|| unavailable("the saved plan is too large to measure"))
        })?;
        if free_bytes(run_dir)? < needed {
            return Err(io::Error::other(
                "There is not enough free space to copy the recorded plan. Nothing started.",
            ));
        }
        copy_package(run_dir, &files)?;
        if binding(run_dir)?.as_ref() != Some(&self.binding) {
            return Err(unavailable("the plan changed while it was copied"));
        }
        Ok(())
    }

    pub(crate) fn delivery_for(&self, run_dir: &Path, node_key: &str) -> io::Result<Delivery> {
        let version = self.pinned_for(node_key)?;
        let opened = read_opened(run_dir, node_key)?
            .into_iter()
            .filter(|opened| opened.version_id == version.version_id)
            .collect::<Vec<_>>();
        let opened_bytes = opened.iter().try_fold(0_usize, |total, opened| {
            total
                .checked_add(opened.bytes)
                .ok_or_else(|| unavailable("the plan read total overflowed"))
        })?;
        Ok(Delivery {
            version: version.version,
            version_id: version.version_id.clone(),
            core_bytes: render_core(&version).len(),
            detail_bytes: render_details(&version.document, &[]).len(),
            opened_bytes,
            opened_count: opened.len(),
        })
    }

    pub fn diagnostic_counts(&self, run_dir: &Path, node_key: &str) -> io::Result<[usize; 3]> {
        let delivery = self.delivery_for(run_dir, node_key)?;
        Ok([
            self.versions.len(),
            usize::try_from(delivery.version).unwrap_or(usize::MAX),
            delivery.opened_count,
        ])
    }

    /// Lab rozwiązuje historyczne wersje raz na przypadek, zanim kolumny zmienią modele.
    pub(crate) fn from_saved_nodes(
        project: &Path,
        bindings: &BTreeMap<String, SavedNode>,
    ) -> io::Result<Option<Self>> {
        if bindings.is_empty() {
            return Ok(None);
        }
        let sources = held_saved_sources(project, bindings)?;
        let mut versions = BTreeMap::<String, PlanVersion>::new();
        let mut pinned = BTreeMap::new();
        for (target, saved) in bindings {
            one_component(target)?;
            let (snapshot, _) = sources
                .get(&saved.source_run_id)
                .ok_or_else(|| unavailable("a saved Lab case has no recorded plan package"))?;
            let version = snapshot.pinned_for(&saved.source_node_key)?;
            if let Some(known) = versions.get(&version.version_id)
                && known != &version
            {
                return Err(unavailable(
                    "two saved Lab cases disagree about one plan version",
                ));
            }
            pinned.insert(target.clone(), version.version_id.clone());
            versions.insert(version.version_id.clone(), version);
        }
        let current = versions
            .values()
            .max_by_key(|version| version.version)
            .ok_or_else(|| unavailable("the saved Lab cases have no plan version"))?;
        let mut version_files = BTreeMap::<String, Vec<u8>>::new();
        for version in versions.values() {
            let (relative, bytes) = super::publish::portable_version_file(version)
                .map_err(|error| plan_error(&error))?;
            if let Some(known) = version_files.insert(relative, bytes.clone())
                && known != bytes
            {
                return Err(unavailable(
                    "two saved Lab plan versions share one file address",
                ));
            }
        }
        let mut files = vec![(
            CURRENT.to_owned(),
            super::publish::portable_current_file(current).map_err(|error| plan_error(&error))?,
        )];
        files.extend(version_files);
        let binding = binding_for(&files, &current.version_id);
        Ok(Some(Self {
            binding,
            versions,
            pinned,
            source_dir: None,
            files: Some(Arc::new(files)),
            _source_holds: Some(Arc::new(
                sources.into_values().map(|(_, held)| held).collect(),
            )),
        }))
    }

    pub(crate) fn from_saved_cases(
        project: &Path,
        file: &crate::workflow::WorkflowFile,
    ) -> io::Result<Option<Self>> {
        let Some(saved) = file.extra.get("frozenPlanInputs") else {
            return Ok(None);
        };
        let bindings = serde_json::from_value(saved.clone())
            .map_err(|_| unavailable("the saved Lab plan addresses cannot be read"))?;
        Self::from_saved_nodes(project, &bindings)
    }
}

impl Recorder {
    /// 2026-09-08 (WP-06): zapis następuje dopiero po zbudowaniu odpowiedzi; odmowa nie
    /// dostarczyła agentowi bajtów i nie może wyglądać jak `Opened`.
    pub fn opened(&self, version_id: &str, bytes: usize) -> io::Result<()> {
        if version_id != self.version_id {
            return Err(unavailable(
                "the plan read names a version outside this step",
            ));
        }
        if bytes == 0 {
            return Ok(());
        }
        let mut writes = self.writes.lock().unwrap_or_else(PoisonError::into_inner);
        if *writes >= MAX_READ_LOG_ENTRIES {
            return Err(io::Error::other(
                "The plan read history is full. Start a new try.",
            ));
        }
        let relative = read_log(&self.node_key)?;
        ensure_reads(&self.run_dir)?;
        let mut file =
            match open_private_file(&self.run_dir, &relative, PrivateFileAccess::CreateAppend) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    open_private_file(&self.run_dir, &relative, PrivateFileAccess::Append)?
                }
                Err(error) => return Err(error),
            };
        let mut line = serde_json::to_vec(&Opened {
            schema: 1,
            version_id: version_id.to_owned(),
            bytes,
        })
        .map_err(io::Error::other)?;
        line.push(b'\n');
        if file.metadata()?.len().saturating_add(line.len() as u64) > MAX_READ_LOG_BYTES as u64 {
            return Err(io::Error::other(
                "The plan read history is full. Start a new try.",
            ));
        }
        file.write_all(&line)?;
        file.sync_data()?;
        *writes += 1;
        Ok(())
    }
}

pub(crate) fn recorder(
    run_dir: &Path,
    node_key: &str,
    version: &PlanVersion,
) -> io::Result<Recorder> {
    ensure_reads(run_dir)?;
    let written = read_opened(run_dir, node_key)?.len();
    Ok(Recorder {
        run_dir: run_dir.to_path_buf(),
        node_key: node_key.to_owned(),
        version_id: version.version_id.clone(),
        writes: Mutex::new(written),
    })
}

pub(crate) fn binding(run_dir: &Path) -> io::Result<Option<Binding>> {
    let root = run_dir.join(PLAN_ROOT);
    if !root.join(CURRENT).exists() {
        return Ok(None);
    }
    let current = read_current_version(&root).map_err(|error| plan_error(&error))?;
    let files = package_files(run_dir)?;
    Ok(Some(binding_for(&files, &current.version_id)))
}

pub(crate) fn read_bound(run_dir: &Path) -> io::Result<Option<Snapshot>> {
    let saved: RunRecord = serde_json::from_slice(&bounded_regular(
        &run_dir.join("run.json"),
        MAX_PACKAGE_BYTES,
    )?)
    .map_err(|_| unavailable("the saved run description cannot be read"))?;
    let Some(expected) = saved.plan_sources else {
        return Ok(None);
    };
    let source_hold = Arc::new(hold(run_dir)?);
    let actual =
        binding(run_dir)?.ok_or_else(|| unavailable("the saved plan package is missing"))?;
    if actual != expected {
        return Err(unavailable(
            "the plan package does not match this run's record",
        ));
    }
    let versions = read_versions(&run_dir.join(PLAN_ROOT))
        .map_err(|error| plan_error(&error))?
        .into_iter()
        .map(|version| (version.version_id.clone(), version))
        .collect::<BTreeMap<_, _>>();
    let pinned = pinned_versions(&saved.steps, &versions)?;
    Ok(Some(Snapshot {
        binding: actual,
        versions,
        pinned,
        source_dir: Some(Arc::new(run_dir.to_path_buf())),
        files: None,
        _source_holds: Some(Arc::new(vec![source_hold])),
    }))
}

fn held_saved_sources(
    project: &Path,
    bindings: &BTreeMap<String, SavedNode>,
) -> io::Result<BTreeMap<String, (Snapshot, Arc<File>)>> {
    let mut sources = BTreeMap::new();
    for saved in bindings.values() {
        if sources.contains_key(&saved.source_run_id) {
            continue;
        }
        let source = crate::commands::lead_history::source_for(project, &saved.source_run_id)
            .map_err(|error| unavailable(&error))?;
        let source_dir = project.join(".loadout/runs").join(source.run_folder);
        let before = read_bound(&source_dir)?
            .ok_or_else(|| unavailable("a saved Lab case has no plan package"))?;
        let held = Arc::new(hold(&source_dir)?);
        let snapshot = read_bound(&source_dir)?
            .ok_or_else(|| unavailable("a saved Lab plan disappeared while it was held"))?;
        if snapshot.binding() != before.binding() {
            return Err(unavailable(
                "a saved Lab plan changed while it was being held",
            ));
        }
        sources.insert(saved.source_run_id.clone(), (snapshot, held));
    }
    Ok(sources)
}

fn binding_for(files: &[(String, Vec<u8>)], id: &str) -> Binding {
    let mut digest = Sha256::new();
    for (name, bytes) in files {
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    Binding {
        id: id.to_owned(),
        digest: format!("{:x}", digest.finalize()),
    }
}

pub(crate) fn hold(run_dir: &Path) -> io::Result<File> {
    package_lock(run_dir, false)
}

pub(crate) fn is_held(run_dir: &Path) -> io::Result<bool> {
    if !run_dir.join(PLAN_ROOT).join(CURRENT).is_file() {
        return Ok(false);
    }
    match package_lock(run_dir, true) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(true),
        Err(error) => Err(error),
    }
}

pub(crate) fn removal_guard(run_dir: &Path) -> io::Result<Option<File>> {
    if !run_dir.join(PLAN_ROOT).join(CURRENT).is_file() {
        return Ok(None);
    }
    package_lock(run_dir, true).map(Some)
}

fn pinned_versions(
    steps: &[StepRecord],
    versions: &BTreeMap<String, PlanVersion>,
) -> io::Result<BTreeMap<String, String>> {
    let mut pinned = BTreeMap::new();
    for step in steps {
        let Some(receipt) = &step.plan_version else {
            continue;
        };
        let key = if step.node_key.is_empty() {
            &step.id
        } else {
            &step.node_key
        };
        one_component(key)?;
        let version = versions
            .get(&receipt.version_id)
            .ok_or_else(|| unavailable("a saved step points to a missing plan version"))?;
        if receipt.document_id != DOCUMENT_ID || receipt.version != version.version {
            return Err(unavailable(
                "a saved step's plan receipt does not match its version",
            ));
        }
        if pinned
            .insert(key.clone(), receipt.version_id.clone())
            .is_some()
        {
            return Err(unavailable("two saved steps share one physical address"));
        }
    }
    Ok(pinned)
}

fn package_files(run_dir: &Path) -> io::Result<PackageFiles> {
    let root = run_dir.join(PLAN_ROOT);
    let mut files = vec![(
        CURRENT.to_owned(),
        bounded_regular(&root.join(CURRENT), MAX_FILE_BYTES)?,
    )];
    let versions = root.join(VERSIONS);
    let mut names = fs::read_dir(&versions)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<Vec<_>, _>>()?;
    names.sort();
    if names.len() > MAX_VERSIONS {
        return Err(unavailable("the saved plan has too many versions"));
    }
    for name in names {
        let name = name
            .to_str()
            .ok_or_else(|| unavailable("a saved plan version has no portable name"))?;
        one_component(name)?;
        if Path::new(name).extension().and_then(|one| one.to_str()) != Some("json") {
            continue;
        }
        files.push((
            format!("{VERSIONS}/{name}"),
            bounded_regular(&versions.join(name), MAX_FILE_BYTES)?,
        ));
    }
    let total = files.iter().try_fold(0_usize, |total, (_, bytes)| {
        total
            .checked_add(bytes.len())
            .ok_or_else(|| unavailable("the saved plan package is too large"))
    })?;
    if total > MAX_PACKAGE_BYTES {
        return Err(unavailable("the saved plan package is too large"));
    }
    Ok(files)
}

fn copy_package(run_dir: &Path, files: &[(String, Vec<u8>)]) -> io::Result<()> {
    let Some(((current_name, current_bytes), versions)) = files.split_first() else {
        return Err(unavailable("the saved plan package is empty"));
    };
    if current_name != CURRENT {
        return Err(unavailable("the saved plan package has no current version"));
    }
    let root = run_dir.join(PLAN_ROOT);
    fs::create_dir_all(root.join(VERSIONS))?;
    let publisher = DurableFilePublisher::new(&root);
    publisher
        .with_publication(|batch| {
            for (relative, bytes) in versions {
                batch.atomic_create_if_absent(
                    &root.join(relative),
                    bytes,
                    ModePolicy::Exact(PUBLISHED_HANDOFF_MODE),
                )?;
            }
            batch.atomic_create_if_absent(
                &root.join(CURRENT),
                current_bytes,
                ModePolicy::Exact(DEFINITION_FILE_MODE),
            )
        })
        .map_err(crate::durable_file::PublishError::into_io)
}

fn package_lock(run_dir: &Path, exclusive: bool) -> io::Result<File> {
    let file = match open_private_file(
        run_dir,
        Path::new(READER_LOCK),
        PrivateFileAccess::CreateAppend,
    ) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            open_private_file(run_dir, Path::new(READER_LOCK), PrivateFileAccess::Append)?
        }
        Err(error) => return Err(error),
    };
    let locked = if exclusive {
        file.try_lock()
    } else {
        file.try_lock_shared()
    };
    locked.map_err(|_| {
        io::Error::new(
            io::ErrorKind::WouldBlock,
            "This run's plan is being read or copied. Keep the run until that finishes.",
        )
    })?;
    Ok(file)
}

fn ensure_reads(run_dir: &Path) -> io::Result<()> {
    crate::engine::supervisor::PublicationRoot::open(run_dir)?
        .ensure_directory(Path::new(READS), 0o700)
}

fn read_log(node_key: &str) -> io::Result<PathBuf> {
    one_component(node_key)?;
    Ok(Path::new(READS).join(format!("{node_key}.jsonl")))
}

fn read_opened(run_dir: &Path, node_key: &str) -> io::Result<Vec<Opened>> {
    let relative = read_log(node_key)?;
    let mut file = match open_private_file(run_dir, &relative, PrivateFileAccess::Read) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(MAX_READ_LOG_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_READ_LOG_BYTES {
        return Err(unavailable("the plan read history exceeds its byte limit"));
    }
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| {
            let opened: Opened = serde_json::from_slice(line)
                .map_err(|_| unavailable("the plan read history cannot be read"))?;
            if opened.schema != 1 {
                return Err(unavailable("the plan read history format is not supported"));
            }
            Ok(opened)
        })
        .take(MAX_READ_LOG_ENTRIES + 1)
        .collect::<io::Result<Vec<_>>>()
        .and_then(|records| {
            if records.len() > MAX_READ_LOG_ENTRIES {
                Err(unavailable("the plan read history exceeds its entry limit"))
            } else {
                Ok(records)
            }
        })
}

fn bounded_regular(path: &Path, limit: usize) -> io::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > limit as u64 {
        return Err(unavailable(
            "a saved plan file is not one bounded regular file",
        ));
    }
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(unavailable("a saved plan file exceeds its byte limit"));
    }
    let after = file.metadata()?;
    if after.len() != metadata.len() {
        return Err(unavailable("a saved plan file changed while it was read"));
    }
    Ok(bytes)
}

fn one_component(value: &str) -> io::Result<()> {
    let mut parts = Path::new(value).components();
    if value.is_empty()
        || !matches!(parts.next(), Some(Component::Normal(_)))
        || parts.next().is_some()
    {
        return Err(unavailable("a saved plan address is not safe"));
    }
    Ok(())
}

fn plan_error(error: &super::Error) -> io::Error {
    unavailable(&error.to_string())
}

fn unavailable(detail: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, detail.to_owned())
}
