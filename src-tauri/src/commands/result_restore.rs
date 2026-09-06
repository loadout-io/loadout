//! WF-24: odczyt niezmiennego wyniku i nowy eksport. Bez checkoutu, hooków i modeli.
use std::collections::BTreeMap;
use std::fmt;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::input_snapshot::Entry;
use super::lead_start::RunRef;
use crate::engine::supervisor::{self, PrivateFileModePolicy, PublicationRoot};
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;

const MAX_BYTES: u64 = 128 * 1024 * 1024;
const MAX_FILES: usize = 100_000;
const LOCK: &str = ".result-reader.lock";
const RECEIPT: &str = ".loadout-restored.json";

pub async fn set_kept(
    project: &Path,
    run_id: &str,
    result_id: &str,
    kept: bool,
    original_intent: &str,
) -> Result<Value, String> {
    let expected = if kept {
        "Keep result"
    } else {
        "Allow cleanup of this result"
    };
    if original_intent != expected {
        return Err(
            "This change needs the person's exact choice in History. No result was changed."
                .to_owned(),
        );
    }
    if !safe_path(Path::new(result_id)) || Path::new(result_id).components().count() != 1 {
        return Err(unavailable("the result ID is invalid"));
    }
    let source = super::lead_history::source_for(project, run_id)?;
    let run_dir = project.join(".loadout/runs").join(&source.run_folder);
    let _exclusive = source_lock(&run_dir, true).map_err(unavailable)?;
    if super::run::copy_lifetime_blocker(&run_dir)
        .map_err(unavailable)?
        .is_some()
    {
        return Err(unavailable(
            "the run or a service is still using these files",
        ));
    }
    let bytes = super::lead_history::source_bytes(project, &source)?;
    let saved: Value = serde_json::from_slice(&bytes).map_err(unavailable)?;
    let result: super::run::SavedCopy = serde_json::from_value(
        saved
            .get("copy_results")
            .and_then(|all| all.get(result_id))
            .cloned()
            .ok_or_else(|| unavailable("that result was not saved"))?,
    )
    .map_err(unavailable)?;
    let held = PublicationRoot::open(&run_dir).map_err(unavailable)?;
    held.ensure_directory(Path::new(".kept-results"), 0o700)
        .map_err(unavailable)?;
    let relative = Path::new(".kept-results").join(format!("{result_id}.json"));
    let reference = format!("refs/loadout/kept/{run_id}/{result_id}");
    let existing = held
        .open_private_existing(&relative, PrivateFileModePolicy::ExactOwnerOnly)
        .map_err(unavailable)?;
    if kept {
        let oid = match &result {
            super::run::SavedCopy::Git { oid } => {
                tree(project, oid).await?;
                Some(oid.as_str())
            }
            super::run::SavedCopy::Folder { .. } => {
                super::run::read_saved_folder(&run_dir, result_id, &result).map_err(unavailable)?;
                None
            }
            _ => return Err(unavailable("there is no complete result to keep")),
        };
        let record = json!({"schema":1,"source":{"workspace":project,"runId":run_id},"resultId":result_id,"result":result,"reference":oid.map(|_| &reference)});
        let encoded = serde_json::to_vec(&record).map_err(unavailable)?;
        if let Some(existing) = &existing {
            let old: Value =
                serde_json::from_slice(&held.read_regular(&relative, true).map_err(unavailable)?)
                    .map_err(unavailable)?;
            if old != record
                || !held
                    .validate_private_identity(&relative, existing.identity())
                    .map_err(unavailable)?
            {
                return Err(unavailable(
                    "the kept-result record changed; nothing was overwritten",
                ));
            }
        } else {
            crate::durable_file::DurableFilePublisher::new(&run_dir)
                .atomic_create_if_absent(
                    &run_dir.join(&relative),
                    &encoded,
                    crate::durable_file::ModePolicy::Exact(crate::durable_file::PRIVATE_FILE_MODE),
                )
                .map_err(unavailable)?;
        }
        // Zapis pinu jest pierwszy: awaria refa ma blokować retencję, nie udawać sukcesu.
        if let Some(oid) = oid {
            let zero = "0".repeat(oid.len());
            let current = git(
                project,
                &["rev-parse", "--verify", &reference],
                Vec::new(),
                1024,
            )
            .await;
            match current {
                Ok(current) if std::str::from_utf8(&current).ok().map(str::trim) == Some(oid) => {}
                Ok(_) => {
                    return Err(unavailable(
                        "the protection reference points to a different result",
                    ));
                }
                Err(_) => {
                    git(
                        project,
                        &["update-ref", &reference, oid, &zero],
                        Vec::new(),
                        1024,
                    )
                    .await?;
                }
            }
            let checked = git(
                project,
                &["rev-parse", "--verify", &reference],
                Vec::new(),
                1024,
            )
            .await?;
            if std::str::from_utf8(&checked).ok().map(str::trim) != Some(oid) {
                return Err(unavailable("the result protection could not be verified"));
            }
        }
    } else if let Some(existing) = existing {
        let record: Value =
            serde_json::from_slice(&held.read_regular(&relative, true).map_err(unavailable)?)
                .map_err(unavailable)?;
        if record.pointer("/source/workspace") != Some(&json!(project))
            || record.pointer("/source/runId").and_then(Value::as_str) != Some(run_id)
            || record.get("resultId").and_then(Value::as_str) != Some(result_id)
            || record.get("result") != Some(&serde_json::to_value(&result).map_err(unavailable)?)
        {
            return Err(unavailable(
                "the kept-result identity changed; review it again",
            ));
        }
        if let super::run::SavedCopy::Git { oid } = &result {
            git(
                project,
                &["update-ref", "-d", &reference, oid],
                Vec::new(),
                1024,
            )
            .await?;
        }
        if !held
            .remove_regular_file_if_identity(&relative, existing.identity())
            .map_err(unavailable)?
        {
            return Err(unavailable(
                "the kept-result record changed during cleanup approval",
            ));
        }
    }
    held.validate_path_identity(&run_dir).map_err(unavailable)?;
    Ok(
        json!({"kept":kept,"said":if kept { "This result will be kept." } else { "Cleanup may remove this result. No files were removed now." }}),
    )
}

pub(super) fn has_pins(run_dir: &Path) -> io::Result<bool> {
    let root = PublicationRoot::open(run_dir)?;
    match root.list_directory(Path::new(".kept-results")) {
        Ok(entries) => Ok(!entries.is_empty()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

pub(super) fn removal_blocker(run_dir: &Path) -> io::Result<Option<String>> {
    if source_is_held(run_dir)? {
        return Ok(Some("This saved result is being read. Close its preview or wait for the restore to finish before removing it.".to_owned()));
    }
    if has_pins(run_dir)? {
        return Ok(Some("This run has a result marked Keep result. Allow cleanup of that exact result in History before removing this run.".to_owned()));
    }
    Ok(None)
}

/// Trzymany aż do końca usuwania. Pin oraz preview muszą zdobyć ten sam lock wcześniej.
pub(super) fn removal_guard(run_dir: &Path) -> io::Result<std::fs::File> {
    let guard = source_lock(run_dir, true)?;
    if has_pins(run_dir)? {
        return Err(io::Error::other(
            "This run has a kept result. Allow cleanup of that exact result first.",
        ));
    }
    Ok(guard)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GitFile {
    path: PathBuf,
    oid: String,
    mode: String,
    bytes: u64,
}

enum Contents {
    Git {
        oid: String,
        files: Vec<GitFile>,
    },
    Folder {
        path: PathBuf,
        entries: BTreeMap<PathBuf, Entry>,
    },
}

/// Desk trzyma ten sam obiekt w istniejącym rejestrze preview. Lock plikowy znika po crash.
pub struct ResultMaterial {
    pub source: RunRef,
    pub source_dir: PathBuf,
    pub result_id: String,
    pub destination: PathBuf,
    pub preview: Value,
    revision: String,
    contents: Contents,
    _source_hold: std::fs::File,
}

impl fmt::Debug for ResultMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResultMaterial")
            .field("source", &self.source)
            .field("result_id", &self.result_id)
            .finish_non_exhaustive()
    }
}

fn unavailable(detail: impl fmt::Display) -> String {
    format!(
        "The saved result is unavailable: {detail}. Nothing was restored or replaced with today's files."
    )
}

/// Shared/exclusive jest jedyną synchronizacją z istniejącą retencją, nie nowym registry.
pub(super) fn source_lock(run_dir: &Path, exclusive: bool) -> io::Result<std::fs::File> {
    let root = PublicationRoot::open(run_dir)?;
    let relative = Path::new(LOCK);
    let opened = match root
        .open_private_existing(relative, PrivateFileModePolicy::ExactOwnerOnly)
        .map_err(io::Error::other)?
    {
        Some(opened) => opened,
        None => match root.create_private(relative) {
            Ok(opened) => opened,
            Err(_) => root
                .open_private_existing(relative, PrivateFileModePolicy::ExactOwnerOnly)
                .map_err(io::Error::other)?
                .ok_or_else(|| io::Error::other("the result lock could not be created"))?,
        },
    };
    let file = opened.file().try_clone()?;
    let locked = if exclusive {
        file.try_lock()
    } else {
        file.try_lock_shared()
    };
    locked.map_err(|_| {
        io::Error::new(
            io::ErrorKind::WouldBlock,
            "This saved result is being read or removed. Keep viewing it, then try again.",
        )
    })?;
    root.validate_path_identity(run_dir)?;
    if !root
        .validate_private_identity(relative, opened.identity())
        .map_err(io::Error::other)?
    {
        return Err(io::Error::other("the result lock changed identity"));
    }
    Ok(file)
}

pub(super) fn source_is_held(run_dir: &Path) -> io::Result<bool> {
    match source_lock(run_dir, true) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(true),
        Err(error) => Err(error),
    }
}

pub async fn prepare(project: &Path, input: &Value) -> Result<Arc<ResultMaterial>, String> {
    if input.get("folder").is_some()
        || input.get("workspace").is_some()
        || input.get("destination").is_some()
    {
        return Err(
            "Restore only reads this conversation's saved result and creates its own new folder."
                .to_owned(),
        );
    }
    let id = input
        .get("source_run_id")
        .and_then(Value::as_str)
        .ok_or_else(|| unavailable("choose a saved run ID"))?;
    let key = input
        .get("result_id")
        .and_then(Value::as_str)
        .filter(|key| safe_path(Path::new(key)) && !key.contains('/'))
        .ok_or_else(|| unavailable("choose an exact saved result ID"))?;
    let source = super::lead_history::source_for(project, id)?;
    let source_dir = project.join(".loadout/runs").join(&source.run_folder);
    let hold = source_lock(&source_dir, false).map_err(unavailable)?;
    let bytes = super::lead_history::source_bytes(project, &source)?;
    let saved: Value = serde_json::from_slice(&bytes).map_err(unavailable)?;
    if super::run::copy_lifetime_blocker(&source_dir)
        .map_err(unavailable)?
        .is_some()
    {
        return Err(unavailable(
            "the run or one of its services is still using this result",
        ));
    }
    let result: super::run::SavedCopy = serde_json::from_value(
        saved
            .get("copy_results")
            .and_then(|all| all.get(key))
            .cloned()
            .ok_or_else(|| unavailable("that result was not saved"))?,
    )
    .map_err(unavailable)?;
    let contents = match &result {
        super::run::SavedCopy::Git { oid } => Contents::Git {
            oid: oid.clone(),
            files: tree(project, oid).await?,
        },
        super::run::SavedCopy::Folder { .. } => {
            let folder =
                super::run::read_saved_folder(&source_dir, key, &result).map_err(unavailable)?;
            Contents::Folder {
                path: folder.path,
                entries: folder.entries,
            }
        }
        _ => return Err(unavailable("this result has no complete saved file set")),
    };
    let (files, size, oid) = match &contents {
        Contents::Git { oid, files } => (
            files.iter().map(|one| one.path.clone()).collect::<Vec<_>>(),
            files.iter().map(|one| one.bytes).sum::<u64>(),
            Some(oid.clone()),
        ),
        Contents::Folder { entries, .. } => (
            entries.keys().cloned().collect(),
            entries
                .values()
                .map(|one| match one {
                    Entry::File { bytes, .. } => *bytes,
                    _ => 0,
                })
                .sum(),
            None,
        ),
    };
    if files.len() > MAX_FILES || size > MAX_BYTES {
        return Err(unavailable(
            "the saved result exceeds the 100,000-file or 128 MiB restore limit",
        ));
    }
    let destination = project
        .join(".loadout/restored")
        .join(uuid::Uuid::now_v7().to_string());
    let shown: Vec<_> = files.iter().take(100).collect();
    let preview = json!({"source":{"workspace":project,"runId":id},"resultId":key,"oid":oid,"files":shown,"fileCount":files.len(),"moreFiles":files.len()>shown.len(),
        "bytes":size,"folder":destination,"said":format!("Restore {} saved files to a new folder. The project will stay unchanged.", files.len())});
    Ok(Arc::new(ResultMaterial {
        source: RunRef {
            workspace: project.to_path_buf(),
            run_id: id.to_owned(),
        },
        source_dir,
        result_id: key.to_owned(),
        destination,
        preview,
        revision: crate::durable_file::revision_of(&bytes),
        contents,
        _source_hold: hold,
    }))
}

impl ResultMaterial {
    pub fn validate(&self) -> Result<(), String> {
        let source = super::lead_history::source_for(&self.source.workspace, &self.source.run_id)?;
        let bytes = super::lead_history::source_bytes(&self.source.workspace, &source)?;
        if crate::durable_file::revision_of(&bytes) != self.revision {
            return Err(unavailable(
                "the saved run changed after this preview; review it again",
            ));
        }
        if let Contents::Folder { path, entries } = &self.contents
            && super::input_snapshot::inspect(path).map_err(unavailable)? != *entries
        {
            return Err(unavailable(
                "the saved result files changed after this preview",
            ));
        }
        Ok(())
    }

    pub async fn restore(self: Arc<Self>) -> Result<Value, String> {
        self.validate()?;
        let project = PublicationRoot::open(&self.source.workspace).map_err(unavailable)?;
        project
            .ensure_directory(Path::new(".loadout/restored"), 0o700)
            .map_err(unavailable)?;
        let relative = self
            .destination
            .strip_prefix(&self.source.workspace)
            .map_err(unavailable)?;
        let output = project.create_directory_exclusive(relative, 0o700).map_err(|error| format!("The restore destination is occupied or unavailable: {error}. Nothing there was overwritten."))?;
        match &self.contents {
            Contents::Git { oid, files } => {
                if tree(&self.source.workspace, oid).await? != *files {
                    return Err(unavailable("the saved Git file set changed"));
                }
                let mut input = Vec::new();
                for file in files {
                    input.extend_from_slice(file.oid.as_bytes());
                    input.push(b'\n');
                }
                let bytes = git(
                    &self.source.workspace,
                    &["cat-file", "--batch"],
                    input,
                    MAX_BYTES + MAX_FILES as u64 * 100,
                )
                .await?;
                let mut cursor = bytes.as_slice();
                for file in files {
                    let newline = cursor
                        .iter()
                        .position(|byte| *byte == b'\n')
                        .ok_or_else(|| unavailable("a saved object is incomplete"))?;
                    let header = std::str::from_utf8(&cursor[..newline]).map_err(unavailable)?;
                    if header != format!("{} blob {}", file.oid, file.bytes) {
                        return Err(unavailable("a saved object is missing or has changed"));
                    }
                    cursor = &cursor[newline + 1..];
                    let length = usize::try_from(file.bytes).map_err(unavailable)?;
                    if cursor.len() <= length || cursor[length] != b'\n' {
                        return Err(unavailable("a saved object's bytes are incomplete"));
                    }
                    if let Some(parent) = file.path.parent() {
                        output
                            .ensure_directory(parent, 0o700)
                            .map_err(unavailable)?;
                    }
                    if file.mode == "120000" {
                        let link = std::str::from_utf8(&cursor[..length])
                            .map_err(|_| unavailable("a saved link is not a supported path"))?;
                        output
                            .create_link(&file.path, Path::new(link))
                            .map_err(unavailable)?;
                    } else {
                        let mut written = output.create_regular(&file.path).map_err(unavailable)?;
                        written.write_all(&cursor[..length]).map_err(unavailable)?;
                        supervisor::set_executable_file(&written, file.mode == "100755")
                            .map_err(unavailable)?;
                        written.sync_all().map_err(unavailable)?;
                    }
                    cursor = &cursor[length + 1..];
                }
                if !cursor.is_empty() {
                    return Err(unavailable("the saved object reply had unexpected data"));
                }
            }
            Contents::Folder { path, entries } => {
                output
                    .validate_path_identity(&self.destination)
                    .map_err(unavailable)?;
                super::input_snapshot::copy_entries(path, &self.destination, entries)
                    .map_err(unavailable)?;
            }
        }
        self.validate()?;
        output
            .validate_path_identity(&self.destination)
            .map_err(unavailable)?;
        let receipt = json!({"schema":1,"source":self.source,"resultId":self.result_id,"folder":self.destination,"identity":output.identity(),
            "entries":super::input_snapshot::inspect(&self.destination).map_err(unavailable)?});
        let mut file = output
            .create_regular(Path::new(RECEIPT))
            .map_err(unavailable)?;
        file.write_all(&serde_json::to_vec(&receipt).map_err(unavailable)?)
            .map_err(unavailable)?;
        file.sync_all().map_err(unavailable)?;
        output
            .validate_path_identity(&self.destination)
            .map_err(unavailable)?;
        Ok(json!({"folder":self.destination,"said":"The saved files are ready in a new folder."}))
    }
}

pub fn restored_folder(project: &Path, folder: &Path) -> Result<PathBuf, String> {
    let relative = folder
        .strip_prefix(project.join(".loadout/restored"))
        .map_err(|_| unavailable("the restored folder belongs to another project"))?;
    if relative.components().count() != 1
        || uuid::Uuid::parse_str(relative.to_str().unwrap_or_default()).is_err()
    {
        return Err(unavailable("that is not a Loadout restored folder"));
    }
    let root = PublicationRoot::open(folder).map_err(unavailable)?;
    let receipt: Value = serde_json::from_slice(
        &root
            .read_regular(Path::new(RECEIPT), true)
            .map_err(unavailable)?,
    )
    .map_err(unavailable)?;
    if receipt.get("folder") != Some(&json!(folder))
        || receipt.pointer("/source/workspace") != Some(&json!(project))
    {
        return Err(unavailable("the restored folder's identity does not match"));
    }
    if receipt.get("identity") != Some(&json!(root.identity())) {
        return Err(unavailable(
            "this folder is no longer the one created by the restore",
        ));
    }
    root.validate_path_identity(folder).map_err(unavailable)?;
    Ok(folder.to_path_buf())
}

pub(super) fn saved_results(project: &Path, run_id: &str) -> Result<Vec<Value>, String> {
    // Stare, nieadresowalne opisy pozostają czytelne, lecz nie dostają fałszywego Restore.
    if uuid::Uuid::parse_str(run_id).is_err() {
        return Ok(Vec::new());
    }
    let source = super::lead_history::source_for(project, run_id)?;
    let bytes = super::lead_history::source_bytes(project, &source)?;
    let saved: Value = serde_json::from_slice(&bytes).map_err(unavailable)?;
    let Some(results) = saved.get("copy_results").and_then(Value::as_object) else {
        return Ok(Vec::new());
    };
    let root = PublicationRoot::open(&project.join(".loadout/runs").join(&source.run_folder))
        .map_err(unavailable)?;
    let pins = match root.list_directory(Path::new(".kept-results")) {
        Ok(pins) => pins,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(unavailable(error)),
    };
    Ok(results.iter().take(100).map(|(key, result)| {
        let name = saved.get("steps").and_then(Value::as_array).and_then(|steps| steps.iter()
            .find(|one| one.get("node_key").and_then(Value::as_str) == Some(key)))
            .and_then(|one| one.get("name")).and_then(Value::as_str).unwrap_or("Saved result");
        let kind = result.get("kind").and_then(Value::as_str).unwrap_or("unavailable");
        let kept = pins.iter().any(|one| one.name == std::ffi::OsStr::new(&format!("{key}.json")));
        json!({"resultId":key,"name":name,"kind":kind,"available":matches!(kind,"git"|"folder"),"kept":kept,
            "cleanupWarning":format!("Allow cleanup to remove the saved {name} result? No files will be removed now.")})
    }).collect())
}

fn safe_path(path: &Path) -> bool {
    !path.as_os_str().is_empty() && path.components().all(|part| matches!(part, Component::Normal(name) if name != ".git" && name != ".loadout" && name != RECEIPT))
}

async fn tree(project: &Path, oid: &str) -> Result<Vec<GitFile>, String> {
    if !matches!(oid.len(), 40 | 64) || !oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(unavailable("the recorded commit ID is invalid"));
    }
    let bytes = git(
        project,
        &["ls-tree", "-r", "-z", "-l", oid],
        Vec::new(),
        32 * 1024 * 1024,
    )
    .await?;
    let mut files = Vec::new();
    let mut total = 0_u64;
    for row in bytes.split(|byte| *byte == 0).filter(|row| !row.is_empty()) {
        let tab = row
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or_else(|| unavailable("a saved tree entry is incomplete"))?;
        let fields: Vec<_> = std::str::from_utf8(&row[..tab])
            .map_err(unavailable)?
            .split_whitespace()
            .collect();
        let path = PathBuf::from(
            std::str::from_utf8(&row[tab + 1..])
                .map_err(|_| unavailable("a saved file name is not supported"))?,
        );
        if fields.len() != 4
            || fields[1] != "blob"
            || !matches!(fields[0], "100644" | "100755" | "120000")
            || !safe_path(&path)
        {
            return Err(unavailable(
                "a saved file, link or nested repository cannot be exported safely",
            ));
        }
        let size = fields[3].parse::<u64>().map_err(unavailable)?;
        total = total
            .checked_add(size)
            .ok_or_else(|| unavailable("the saved result is too large"))?;
        if total > MAX_BYTES || files.len() >= MAX_FILES {
            return Err(unavailable(
                "the saved result exceeds the 100,000-file or 128 MiB restore limit",
            ));
        }
        files.push(GitFile {
            path,
            oid: fields[2].to_owned(),
            mode: fields[0].to_owned(),
            bytes: size,
        });
    }
    Ok(files)
}

async fn git(
    project: &Path,
    arguments: &[&str],
    input: Vec<u8>,
    limit: u64,
) -> Result<Vec<u8>, String> {
    let mut command = tokio::process::Command::new("git");
    command
        .arg("--no-replace-objects")
        .arg("-C")
        .arg(project)
        .arg("-c")
        .arg(format!(
            "core.hooksPath={}",
            supervisor::disabled_git_hooks().display()
        ))
        .args(arguments);
    let input = String::from_utf8(input).map_err(unavailable)?;
    let mut child =
        supervisor::spawn(command, supervisor::StdinPlan::Write(input)).map_err(unavailable)?;
    let stdout = child
        .stdout()
        .ok_or_else(|| unavailable("Git did not provide its saved files"))?;
    let stderr = child
        .stderr()
        .ok_or_else(|| unavailable("Git did not provide its result"))?;
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        let read = async {
            let mut bytes = Vec::new();
            stdout.take(limit + 1).read_to_end(&mut bytes).await?;
            Ok::<_, io::Error>(bytes)
        };
        let errors = async {
            let mut bytes = Vec::new();
            stderr.take(64 * 1024 + 1).read_to_end(&mut bytes).await?;
            Ok::<_, io::Error>(bytes)
        };
        let (bytes, errors, status) = tokio::try_join!(read, errors, child.wait())?;
        if !status.success() || bytes.len() as u64 > limit || errors.len() > 64 * 1024 {
            return Err(io::Error::other(
                "Git could not read the complete saved result",
            ));
        }
        Ok(bytes)
    })
    .await;
    // Dowód śmierci także po naturalnym EOF; prywatna operacja nie pozostawia procesów.
    let proof = child.stop(supervisor::DEFAULT_GRACE).await;
    if !matches!(proof, supervisor::GroupProof::Dead { .. }) {
        return Err(unavailable("Git may still be reading this result"));
    }
    result
        .map_err(|_| unavailable("reading the saved result took too long"))?
        .map_err(unavailable)
}
