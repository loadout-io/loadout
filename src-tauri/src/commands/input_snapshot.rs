//! Jedno utrwalone wejście dla wszystkich kopii biegu (WF-01, 2026-09-05).
//!
//! Manifest jest czytany przez izolację i fan-in. Bajty leżą na dysku, nie w RAM;
//! odczyt źródła nie przechodzi przez linki i wykrywa zmianę w trakcie kopiowania.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::durable_file::{DurableFilePublisher, ModePolicy, PRIVATE_FILE_MODE};
use crate::engine::supervisor::{self, PublicationEntryKind, PublicationRoot};

use super::isolate::{self, NOT_COPIED};

pub const DIRECTORY: &str = "input";
const MANIFEST: &str = "manifest.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    Directory,
    File {
        digest: String,
        bytes: u64,
        executable: bool,
    },
    Symlink {
        target: PathBuf,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    schema: u8,
    id: String,
    git_oid: Option<String>,
    entries: BTreeMap<PathBuf, Entry>,
    left_behind: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    additional_inputs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct InputSnapshot {
    root: PathBuf,
    manifest: Manifest,
}

impl InputSnapshot {
    #[must_use]
    pub fn entries(&self) -> &BTreeMap<PathBuf, Entry> {
        &self.manifest.entries
    }
    #[must_use]
    pub fn files(&self) -> PathBuf {
        self.root.join("files")
    }
    #[must_use]
    pub fn git_oid(&self) -> Option<&str> {
        self.manifest.git_oid.as_deref()
    }
    #[must_use]
    pub fn id(&self) -> &str {
        &self.manifest.id
    }
    #[must_use]
    pub fn left_behind(&self) -> &[String] {
        &self.manifest.left_behind
    }
    #[must_use]
    pub fn additional_inputs(&self) -> &[String] {
        &self.manifest.additional_inputs
    }

    /// Weryfikacja także przy odczycie po restarcie; odcisk nie jest zgodą na brakujący plik.
    pub fn validate(&self) -> io::Result<()> {
        if self.manifest.schema != 1 || uuid::Uuid::parse_str(self.id()).is_err() {
            return Err(invalid("the saved input description is not supported"));
        }
        validate_paths(self.entries())?;
        if inspect(&self.files())? != *self.entries() {
            return Err(invalid("the saved input files changed or are missing"));
        }
        Ok(())
    }

    /// Tylko pusty prywatny folder albo świeży worktree. Nigdy katalog projektu człowieka.
    pub fn materialize(&self, destination: &Path) -> io::Result<()> {
        self.validate()?;
        fs::create_dir_all(destination)?;
        let held = PublicationRoot::open(destination)?;
        let present = inspect(destination)?;
        // Najpierw dzieci, potem rodzic. Linki pozostają liśćmi, więc usuwanie ich nie idzie
        // do obcego katalogu. Dokładna publikacja fan-in ma dodatkowy kontrakt WF-03.
        for (path, before) in present.iter().rev() {
            if self.entries().get(path) == Some(before) {
                continue;
            }
            if matches!(before, Entry::Directory)
                && matches!(self.entries().get(path), Some(Entry::Directory))
            {
                continue;
            }
            let identity = held
                .entry_identity(path)?
                .ok_or_else(|| invalid("the input destination changed during preparation"))?;
            if !held.remove_entry_if_identity(path, identity)? {
                return Err(invalid("the input destination changed during preparation"));
            }
        }
        held.validate_path_identity(destination)?;
        copy_entries(&self.files(), destination, self.entries())
    }

    /// Nowy bieg posiada własne bajty starego wejścia; retencja źródła nie zabiera wznowienia.
    pub fn copy_to(&self, run_dir: &Path) -> io::Result<Self> {
        self.validate()?;
        let root = run_dir.join(DIRECTORY);
        if fs::symlink_metadata(&root).is_ok() {
            let existing = read(run_dir)?;
            if existing.id() != self.id() || existing.entries() != self.entries() {
                return Err(invalid(
                    "the saved input belongs to a different earlier run",
                ));
            }
            return Ok(existing);
        }
        let staging = tempfile::Builder::new()
            .prefix(".input-")
            .tempdir_in(run_dir)?;
        self.materialize(&staging.path().join("files"))?;
        publish(staging, &root, &self.manifest)?;
        read(run_dir)
    }
}

pub fn read(run_dir: &Path) -> io::Result<InputSnapshot> {
    let root = run_dir.join(DIRECTORY);
    let held = PublicationRoot::open(&root)?;
    let file = held.open_regular_file(Path::new(MANIFEST))?;
    if file.metadata()?.len() > 64 * 1024 * 1024 {
        return Err(invalid("the saved input description is too large"));
    }
    let mut bytes = Vec::new();
    file.take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(invalid("the saved input description is too large"));
    }
    let manifest = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    let snapshot = InputSnapshot { root, manifest };
    snapshot.validate()?;
    Ok(snapshot)
}

/// Powtórzenie Bound prestart może odczytać własny gotowy obraz, ale nigdy go nadpisać.
pub fn capture(project: &Path, run_dir: &Path) -> io::Result<InputSnapshot> {
    capture_with_observer(project, run_dir, |_| Ok(()))
}

/// WF-14: jawny wybór jest rozstrzygany raz dla wspólnego obrazu, przed pierwszym procesem.
pub fn capture_selected(
    project: &Path,
    run_dir: &Path,
    additional: &[String],
) -> io::Result<InputSnapshot> {
    capture_input(project, run_dir, additional, false, |_| Ok(()))
}

/// Wynik już należy do biegu, nie jest nowym importem z projektu. Ponowne zastosowanie
/// filtrów prywatnego wejścia skasowałoby np. .env, który agent świadomie utworzył (WF-06).
pub(super) fn capture_saved_result(project: &Path, run_dir: &Path) -> io::Result<InputSnapshot> {
    capture_input(project, run_dir, &[], true, |_| Ok(()))
}

/// Obserwator służy deterministycznej reprodukcji edycji źródła, nie wykonuje capture za nas.
pub fn capture_with_observer(
    project: &Path,
    run_dir: &Path,
    observed: impl Fn(usize) -> io::Result<()>,
) -> io::Result<InputSnapshot> {
    capture_input(project, run_dir, &[], false, observed)
}

fn capture_input(
    project: &Path,
    run_dir: &Path,
    additional: &[String],
    complete_copy: bool,
    observed: impl Fn(usize) -> io::Result<()>,
) -> io::Result<InputSnapshot> {
    crate::workflow::validate_additional_inputs(additional).map_err(io::Error::other)?;
    let root = run_dir.join(DIRECTORY);
    if fs::symlink_metadata(&root).is_ok() {
        return read(run_dir);
    }
    let is_git = isolate::is_a_repo(project);
    // Najwyżej dwa powtórzenia po pierwszej próbie; ciągle edytowany projekt nie daje
    // pozornej atomowości. Każda próba ma własny losowy katalog staging.
    for attempt in 0..3 {
        let staging = tempfile::Builder::new()
            .prefix(".input-")
            .tempdir_in(run_dir)?;
        let files = staging.path().join("files");
        fs::create_dir(&files)?;
        let oid = head_oid(project, is_git);
        let paths = selected_paths(project, is_git, additional, complete_copy)?;
        let before = inspect_selected(project, paths.as_ref())?;
        observed(attempt)?;
        if let Err(error) = copy_entries(project, &files, &before) {
            if attempt < 2 && error.kind() == io::ErrorKind::InvalidData {
                continue;
            }
            return Err(error);
        }
        let after_paths = selected_paths(project, is_git, additional, complete_copy)?;
        let after = inspect_selected(project, after_paths.as_ref())?;
        let current_oid = head_oid(project, is_git);
        if before != after || paths != after_paths || oid != current_oid {
            if attempt < 2 {
                continue;
            }
            return Err(invalid(
                "the project kept changing while its input was being saved; try again",
            ));
        }
        let left_behind = if is_git {
            untracked_paths(project)?
                .into_iter()
                .filter(|name| !before.contains_key(Path::new(name)))
                .collect()
        } else {
            Vec::new()
        };
        let manifest = Manifest {
            schema: 1,
            id: uuid::Uuid::now_v7().to_string(),
            git_oid: oid,
            entries: before,
            left_behind,
            additional_inputs: additional.to_vec(),
        };
        publish(staging, &root, &manifest)?;
        return read(run_dir);
    }
    Err(invalid(
        "the project changed while its input was being saved",
    ))
}

pub const ADDITIONAL_FILE_LIMIT: usize = 10_000;
pub const ADDITIONAL_BYTE_LIMIT: u64 = 256 * 1024 * 1024;

/// Podgląd nie niesie bajtów prywatnych plików, tylko dokładne wybrane ścieżki i rozmiary.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedInput {
    pub path: PathBuf,
    pub bytes: u64,
    pub ignored: bool,
    pub private: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdditionalInputPreview {
    pub files: Vec<SelectedInput>,
    pub total_bytes: u64,
    pub file_limit: usize,
    pub byte_limit: u64,
}

/// Git dostaje tracked WIP i wybrane extras. Plain folder zachowuje zwykłe pliki,
/// lecz .env wymaga jawnego wyboru. Inspect wyników nie stosuje tego filtra ponownie.
fn selected_paths(
    project: &Path,
    is_git: bool,
    additional: &[String],
    complete_copy: bool,
) -> io::Result<Option<BTreeSet<PathBuf>>> {
    if complete_copy {
        return Ok(None);
    }
    let mut paths = if is_git {
        tracked_paths(project)?
    } else {
        available_paths(project)?.into_keys().collect()
    };
    paths.retain(|path| !private_input(path));
    for one in preview_additional_inputs(project, additional)?.files {
        paths.insert(one.path);
    }
    Ok(Some(paths))
}

fn private_input(path: &Path) -> bool {
    path.components().any(|part| {
        part.as_os_str()
            .to_str()
            .is_some_and(|name| name == ".env" || name.starts_with(".env."))
    })
}

/// Ten spacer nie otwiera plików i nie podąża za linkami. Glob służy tylko dopasowaniu
/// już odczytanych nazw; zewnętrzny link nie staje się katalogiem do przeszukania.
fn available_paths(project: &Path) -> io::Result<BTreeMap<PathBuf, PublicationEntryKind>> {
    let root = PublicationRoot::open(project)?;
    let mut found = BTreeMap::new();
    let mut todo = vec![PathBuf::new()];
    while let Some(directory) = todo.pop() {
        for one in root.list_directory(&directory)? {
            if NOT_COPIED.iter().any(|name| one.name == *name) {
                continue;
            }
            let path = directory.join(one.name);
            if one.kind == PublicationEntryKind::Directory {
                todo.push(path.clone());
            }
            found.insert(path, one.kind);
        }
    }
    root.validate_path_identity(project)?;
    Ok(found)
}

pub fn preview_additional_inputs(
    project: &Path,
    patterns: &[String],
) -> io::Result<AdditionalInputPreview> {
    crate::workflow::validate_additional_inputs(patterns).map_err(io::Error::other)?;
    let mut preview = AdditionalInputPreview {
        files: Vec::new(),
        total_bytes: 0,
        file_limit: ADDITIONAL_FILE_LIMIT,
        byte_limit: ADDITIONAL_BYTE_LIMIT,
    };
    if patterns.is_empty() {
        return Ok(preview);
    }
    let available = available_paths(project)?;
    let root = PublicationRoot::open(project)?;
    let mut picked = BTreeMap::<PathBuf, String>::new();
    let matching = glob::MatchOptions {
        case_sensitive: true,
        require_literal_separator: true,
        require_literal_leading_dot: false,
    };
    for pattern in patterns {
        let matcher =
            glob::Pattern::new(pattern).map_err(|why| io::Error::other(why.to_string()))?;
        let mut found = false;
        for path in available.keys() {
            if matcher.matches_path_with(path, matching)
                || path.ancestors().skip(1).any(|ancestor| {
                    !ancestor.as_os_str().is_empty()
                        && available.get(ancestor) == Some(&PublicationEntryKind::Directory)
                        && matcher.matches_path_with(ancestor, matching)
                })
            {
                found = true;
                picked
                    .entry(path.clone())
                    .or_insert_with(|| pattern.clone());
            }
        }
        if !found {
            return Err(invalid(&format!(
                "Additional input \"{pattern}\" matched no available file in this project."
            )));
        }
    }
    let ignored = ignored_selected_paths(project, &picked.keys().cloned().collect::<Vec<_>>())?;
    let mut counted_files = 0;
    for (path, pattern) in picked {
        let bytes = match available.get(&path) {
            Some(PublicationEntryKind::Regular) => root
                .open_regular_file(&path)
                .and_then(|file| file.metadata())
                .map(|metadata| metadata.len()),
            Some(PublicationEntryKind::Directory) => Ok(0),
            Some(PublicationEntryKind::Symlink) => {
                selected_link_is_contained(&root, &path).map(|()| 0)
            }
            _ => Err(invalid(
                "this is not a regular file, directory, or supported link",
            )),
        }
        .map_err(|error| {
            invalid(&format!(
                "Additional input \"{pattern}\" could not include {}: {error}",
                path.display()
            ))
        })?;
        if available.get(&path) != Some(&PublicationEntryKind::Directory) {
            counted_files += 1;
        }
        preview.total_bytes = preview.total_bytes.saturating_add(bytes);
        if counted_files > ADDITIONAL_FILE_LIMIT || preview.total_bytes > ADDITIONAL_BYTE_LIMIT {
            return Err(invalid(&format!(
                "Additional input \"{pattern}\" exceeds the limit of 10000 files or 256 MiB of selected input."
            )));
        }
        preview.files.push(SelectedInput {
            ignored: ignored.contains(&path),
            private: private_input(&path),
            path,
            bytes,
        });
    }
    root.validate_path_identity(project)?;
    Ok(preview)
}

fn selected_link_is_contained(root: &PublicationRoot, path: &Path) -> io::Result<()> {
    let mut current = path.to_path_buf();
    let mut seen = BTreeSet::new();
    while matches!(
        root.entry_identity(&current)?,
        Some((PublicationEntryKind::Symlink, _))
    ) {
        if !seen.insert(current.clone()) {
            return Err(invalid("the selected link contains a cycle"));
        }
        let target = root.read_link(&current)?;
        if target.is_absolute() {
            return Err(invalid(
                "the selected link does not stay inside the project",
            ));
        }
        let mut parts = current
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf();
        for part in target.components() {
            match part {
                Component::Normal(one) => parts.push(one),
                Component::CurDir => {}
                Component::ParentDir if parts.pop() => {}
                _ => {
                    return Err(invalid(
                        "the selected link does not stay inside the project",
                    ));
                }
            }
        }
        if parts.as_os_str().is_empty() {
            return Ok(());
        }
        current = parts;
    }
    Ok(())
}

fn ignored_selected_paths(project: &Path, paths: &[PathBuf]) -> io::Result<BTreeSet<PathBuf>> {
    if !isolate::is_a_repo(project) {
        return Ok(BTreeSet::new());
    }
    let mut ignored = BTreeSet::new();
    // Ścieżki jadą z NUL na stdin; newline w nazwie nie staje się drugim plikiem.
    // Pisarz i czytnik pracują równocześnie, więc dwa pełne pipe'y nie mogą się zakleszczyć.
    for chunk in paths.chunks(256) {
        let mut input = Vec::new();
        for path in chunk {
            let name = path
                .to_str()
                .ok_or_else(|| invalid("a selected input has an unreadable name"))?;
            input.extend_from_slice(name.as_bytes());
            input.push(0);
        }
        let mut child = Command::new("git")
            .arg("-C")
            .arg(project)
            .args(["check-ignore", "--no-index", "-z", "--stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let Some(mut stdin) = child.stdin.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(invalid("the selected inputs could not be sent to git"));
        };
        let writing = std::thread::spawn(move || stdin.write_all(&input));
        let output = child.wait_with_output();
        writing
            .join()
            .map_err(|_| invalid("the selected inputs could not be sent to git"))??;
        let output = output?;
        if !output.status.success() && output.status.code() != Some(1) {
            return Err(invalid(
                "the selected inputs could not be checked against this project's ignore rules",
            ));
        }
        let names = String::from_utf8(output.stdout)
            .map_err(|_| invalid("an ignored input has an unreadable name"))?;
        ignored.extend(
            names
                .split('\0')
                .filter(|name| !name.is_empty())
                .map(PathBuf::from),
        );
    }
    Ok(ignored)
}

fn publish(staging: tempfile::TempDir, root: &Path, manifest: &Manifest) -> io::Result<()> {
    DurableFilePublisher::new(staging.path())
        .atomic_create_if_absent(
            &staging.path().join(MANIFEST),
            &serde_json::to_vec_pretty(manifest).map_err(io::Error::other)?,
            ModePolicy::Exact(PRIVATE_FILE_MODE),
        )
        .map_err(super::super::durable_file::PublishError::into_io)?;
    if fs::symlink_metadata(root).is_ok() {
        return Err(invalid("the input destination was occupied during capture"));
    }
    // Po rename dawna nazwa staging nie należy już do nas; TempDir nie może przy
    // Drop usunąć obcego katalogu wstawionego później pod tę samą nazwę.
    let staged = staging.keep();
    fs::rename(&staged, root)?;
    let parent = root
        .parent()
        .ok_or_else(|| invalid("the saved input has no parent"))?;
    fs::File::open(parent)?.sync_all()
}

/// Ten sam spacer i wykluczenia dla capture, walidacji oraz rezultatów rodziców fan-in.
pub fn inspect(root: &Path) -> io::Result<BTreeMap<PathBuf, Entry>> {
    inspect_selected(root, None)
}

fn inspect_selected(
    root: &Path,
    selected: Option<&BTreeSet<PathBuf>>,
) -> io::Result<BTreeMap<PathBuf, Entry>> {
    let held = PublicationRoot::open(root)?;
    let mut found = BTreeMap::new();
    let mut todo = vec![PathBuf::new()];
    while let Some(directory) = todo.pop() {
        for entry in held.list_directory(&directory)? {
            if NOT_COPIED.iter().any(|skip| entry.name == *skip) {
                continue;
            }
            let relative = directory.join(&entry.name);
            if selected.is_some_and(|paths| {
                !paths.contains(&relative) && !paths.iter().any(|path| path.starts_with(&relative))
            }) {
                continue;
            }
            let value = match entry.kind {
                PublicationEntryKind::Directory => {
                    todo.push(relative.clone());
                    Entry::Directory
                }
                PublicationEntryKind::Symlink => Entry::Symlink {
                    target: held.read_link(&relative)?,
                },
                PublicationEntryKind::Regular => file_entry(&held, &relative)?,
                PublicationEntryKind::Other => continue,
            };
            found.insert(relative, value);
        }
    }
    held.validate_path_identity(root)?;
    Ok(found)
}

fn file_entry(held: &PublicationRoot, relative: &Path) -> io::Result<Entry> {
    let mut file = held.open_regular_file(relative)?;
    let before = file.metadata()?;
    let identity = supervisor::publication_identity(&file)?;
    let mut digest = Sha256::new();
    let mut limited = (&mut file).take(before.len().saturating_add(1));
    let mut bytes = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let count = limited.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
        bytes += count as u64;
    }
    let after = file.metadata()?;
    if bytes != before.len()
        || before.modified()? != after.modified()?
        || before.len() != after.len()
        || identity != supervisor::publication_identity(&held.open_regular_file(relative)?)?
    {
        return Err(invalid("a project file changed while it was being read"));
    }
    Ok(Entry::File {
        digest: format!("{:x}", digest.finalize()),
        bytes,
        executable: supervisor::executable_bits(&after),
    })
}

pub(super) fn copy_entries(
    source: &Path,
    destination: &Path,
    entries: &BTreeMap<PathBuf, Entry>,
) -> io::Result<()> {
    validate_paths(entries)?;
    let held = PublicationRoot::open(source)?;
    let output_root = PublicationRoot::open(destination)?;
    for (relative, entry) in entries {
        match entry {
            Entry::Directory => output_root.ensure_directory(relative, 0o700)?,
            Entry::Symlink { target: link } => {
                match output_root.entry_identity(relative)? {
                    Some((PublicationEntryKind::Symlink, _))
                        if output_root.read_link(relative)? == *link =>
                    {
                        continue;
                    }
                    Some(_) => {
                        return Err(invalid(
                            "the input destination already contains a different entry",
                        ));
                    }
                    None => {}
                }
                output_root.create_link(relative, link)?;
            }
            Entry::File {
                executable, bytes, ..
            } => {
                if output_root.entry_identity(relative)?.is_some() {
                    if file_entry(&output_root, relative)? == *entry {
                        continue;
                    }
                    return Err(invalid(
                        "the input destination already contains a different file",
                    ));
                }
                let mut file = held.open_regular_file(relative)?;
                let mut output = output_root.create_regular(relative)?;
                io::copy(&mut (&mut file).take(bytes.saturating_add(1)), &mut output)?;
                output.flush()?;
                supervisor::set_executable_file(&output, *executable)?;
                output.sync_all()?;
                if file_entry(&output_root, relative)? != *entry {
                    return Err(invalid(
                        "a project file changed while its input was being copied",
                    ));
                }
            }
        }
    }
    held.validate_path_identity(source)?;
    output_root.validate_path_identity(destination)
}

fn validate_paths(entries: &BTreeMap<PathBuf, Entry>) -> io::Result<()> {
    for path in entries.keys() {
        if path.as_os_str().is_empty()
            || path
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(invalid(
                "the saved input contains a path outside its folder",
            ));
        }
        for parent in path
            .ancestors()
            .skip(1)
            .filter(|path| !path.as_os_str().is_empty())
        {
            if !matches!(entries.get(parent), Some(Entry::Directory)) {
                return Err(invalid("the saved input traverses a non-directory entry"));
            }
        }
    }
    Ok(())
}

/// Repozytorium bez ani jednego commita nie ma `HEAD` — i to NIE jest awaria zapisu wejścia.
///
/// `git_text(…)?` robił z tego `RunError::Io`, czyli zdanie o tym, co nie udało się SYSTEMOWI
/// („fatal: ambiguous argument 'HEAD'"). Zdanie dla człowieka składa dopiero izolacja kroku
/// ([`super::isolate::Trouble::NoCommitYet`]), bo tylko ona zna nazwę kafelka i tylko ona wie,
/// że to jest powód, żeby biegu nie zaczynać. `git_oid` jest `Option` dokładnie po to: pusty
/// adres znaczy „nie ma z czego odbić drzewa", a nie „git przestał działać".
///
/// Porównanie `oid != current_oid` zostaje szczelne: dwa razy `None` to dalej ten sam stan.
fn head_oid(project: &Path, is_git: bool) -> Option<String> {
    if !is_git {
        return None;
    }
    git_text(project, &["rev-parse", "--verify", "HEAD"])
        .ok()
        .map(|text| text.trim().to_owned())
}

fn tracked_paths(project: &Path) -> io::Result<BTreeSet<PathBuf>> {
    Ok(git_text(project, &["ls-files", "--cached", "-z"])?
        .split('\0')
        .filter(|name| !name.is_empty())
        .map(PathBuf::from)
        .collect())
}

fn untracked_paths(project: &Path) -> io::Result<Vec<String>> {
    Ok(git_text(
        project,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?
    .split('\0')
    .filter(|name| !name.is_empty() && !name.starts_with(".loadout/"))
    .map(str::to_owned)
    .collect())
}

pub(super) fn git_text(project: &Path, args: &[&str]) -> io::Result<String> {
    let result = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(args)
        .output()?;
    if !result.status.success() {
        return Err(io::Error::other(
            String::from_utf8_lossy(&result.stderr).into_owned(),
        ));
    }
    String::from_utf8(result.stdout)
        .map_err(|_| invalid("this project's git paths are not valid UTF-8"))
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
