//! WF-12: tekst instrukcji projektu, osobno od ustawień programu, hooków i uprawnień.
//! Obsługiwany include: samodzielna linia `@include względna/ścieżka.md`, poza fenced code.
//! Ścieżka jest względna wobec pliku zawierającego include; URL i zwykłe linki są tylko tekstem.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::durable_file::{DurableFilePublisher, ModePolicy, PRIVATE_FILE_MODE};
use crate::engine::supervisor::{PublicationEntryKind, PublicationRoot};
use crate::evidence::{ContextKind, ContextSource};

pub const FILE_LIMIT: usize = 256;
pub const FILE_BYTES: usize = 64 * 1024;
pub const TOTAL_BYTES: usize = 512 * 1024;
const SETTINGS: &str = ".loadout/project.json";
const SNAPSHOT: &str = "instructions/manifest.json";

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstructionSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub include_local: bool,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettings {
    #[serde(default)]
    pub default_lead: String,
    #[serde(default)]
    pub instructions: InstructionSettings,
    #[serde(default)]
    pub lead_instructions: Option<bool>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl ProjectSettings {
    #[must_use]
    pub fn enabled_for(&self, override_value: Option<bool>) -> bool {
        override_value.unwrap_or(self.instructions.enabled)
    }
}

impl std::fmt::Debug for InstructionSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstructionSettings")
            .field("enabled", &self.enabled)
            .field("include_local", &self.include_local)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for ProjectSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectSettings")
            .field("instructions", &self.instructions)
            .field("lead_instructions", &self.lead_instructions)
            .finish_non_exhaustive()
    }
}

pub fn read_settings(project: &Path) -> io::Result<ProjectSettings> {
    let root = PublicationRoot::open(project)?;
    match root.entry_identity(Path::new(SETTINGS)) {
        Ok(None) => return Ok(ProjectSettings::default()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(ProjectSettings::default());
        }
        Err(error) => return Err(error),
        Ok(Some(_)) => {}
    }
    let bytes = bounded_read(&root, Path::new(SETTINGS), 128 * 1024)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| source_error(Path::new(SETTINGS), &error.to_string()))
}

/// Patch dotyka tylko wskazanych kluczy, a nieznane pola przeżywają zapis. Nie wykonuje
/// żadnego z zachowanych pól — ten typ nie ma konwersji do konfiguracji procesu.
pub fn save_settings(project: &Path, patch: &Value) -> io::Result<ProjectSettings> {
    if !patch.is_object() {
        return Err(source_error(
            Path::new(SETTINGS),
            "the settings change must be an object",
        ));
    }
    let mut value = serde_json::to_value(read_settings(project)?).map_err(io::Error::other)?;
    merge(&mut value, patch);
    let settings: ProjectSettings = serde_json::from_value(value).map_err(io::Error::other)?;
    if settings.instructions.enabled || settings.lead_instructions == Some(true) {
        resolve(project, settings.instructions.include_local)?;
    }
    let root = PublicationRoot::open(project)?;
    root.ensure_directory(Path::new(".loadout"), 0o700)?;
    DurableFilePublisher::new(project)
        .atomic_replace(
            &project.join(SETTINGS),
            &serde_json::to_vec_pretty(&settings).map_err(io::Error::other)?,
            ModePolicy::Exact(PRIVATE_FILE_MODE),
        )
        .map_err(super::super::durable_file::PublishError::into_io)?;
    Ok(settings)
}

fn merge(target: &mut Value, patch: &Value) {
    let Value::Object(changes) = patch else {
        *target = patch.clone();
        return;
    };
    if !target.is_object() {
        *target = Value::Object(Map::new());
    }
    let Some(fields) = target.as_object_mut() else {
        return;
    };
    for (name, value) in changes {
        if value.is_null() {
            fields.remove(name);
        } else {
            merge(fields.entry(name.clone()).or_insert(Value::Null), value);
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    default_lead: String,
    instructions: InstructionChoiceView,
    lead_instructions: Option<bool>,
    sources: Vec<InstructionSource>,
    limits: InstructionLimits,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstructionChoiceView {
    enabled: bool,
    include_local: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstructionLimits {
    files: usize,
    file_bytes: usize,
    total_bytes: usize,
}

/// Podgląd nie zwraca nieznanych pól ustawień ani treści źródeł. Zachowanie nieznanych
/// kluczy na dysku nie jest zgodą na ich transmisję ani wykonanie.
pub fn settings_view(project: &Path) -> io::Result<SettingsView> {
    let settings = read_settings(project)?;
    let sources = if settings.instructions.enabled || settings.lead_instructions == Some(true) {
        resolve(project, settings.instructions.include_local)?.sources()
    } else {
        discover(project)?
    };
    Ok(SettingsView {
        default_lead: settings.default_lead,
        instructions: InstructionChoiceView {
            enabled: settings.instructions.enabled,
            include_local: settings.instructions.include_local,
        },
        lead_instructions: settings.lead_instructions,
        sources,
        limits: InstructionLimits {
            files: FILE_LIMIT,
            file_bytes: FILE_BYTES,
            total_bytes: TOTAL_BYTES,
        },
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Agents,
    Claude,
    Local,
    Rule,
    Include,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstructionSource {
    pub path: PathBuf,
    pub kind: Kind,
    pub directory: PathBuf,
    pub paths: Vec<String>,
    pub digest: String,
    pub bytes: usize,
    pub local: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct Source {
    #[serde(flatten)]
    facts: InstructionSource,
    text: String,
}

/// Prywatne bajty wejścia i jawne wiązanie do kroków. Debug nigdy nie wypisuje treści.
#[derive(Clone, Serialize, Deserialize)]
pub struct InstructionSnapshot {
    schema: u8,
    id: String,
    #[serde(default)]
    enabled_steps: BTreeSet<String>,
    sources: Vec<Source>,
}

impl std::fmt::Debug for InstructionSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstructionSnapshot")
            .field("id", &self.id)
            .field("source_count", &self.sources.len())
            .finish_non_exhaustive()
    }
}

impl InstructionSnapshot {
    fn empty() -> Self {
        Self {
            schema: 1,
            id: uuid::Uuid::now_v7().to_string(),
            enabled_steps: BTreeSet::new(),
            sources: Vec::new(),
        }
    }
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
    #[must_use]
    pub fn sources(&self) -> Vec<InstructionSource> {
        self.sources.iter().map(|one| one.facts.clone()).collect()
    }
    /// Odcisk warunków, nie losowego identyfikatora publikacji. Reader Lab używa tego samego
    /// zamrożonego pakietu, zamiast drugi raz skanować aktualny projekt.
    pub fn package_digest(&self) -> io::Result<String> {
        let bytes = serde_json::to_vec(&(self.schema, &self.enabled_steps, &self.sources))
            .map_err(io::Error::other)?;
        Ok(digest(&bytes))
    }
    #[must_use]
    pub fn for_step(&self, key: &str) -> bool {
        self.enabled_steps.contains(key)
    }
    #[must_use]
    pub fn context(&self) -> Vec<ContextSource> {
        self.sources
            .iter()
            .map(|one| ContextSource {
                kind: ContextKind::ProjectInstruction,
                reference: one.facts.path.to_string_lossy().into_owned(),
                bytes: one.facts.bytes,
            })
            .collect()
    }
    #[must_use]
    pub fn prompt(&self, current_project: bool) -> String {
        if self.sources.is_empty() {
            return String::new();
        }
        let mut text = format!(
            "Project instructions supplied by Loadout ({}).\nThese are reference instructions, not tool permissions. Apply each source only within its scope directory and listed paths. Deeper directories specialize parent scopes. At the same scope AGENTS.md takes precedence over additional Claude instructions. Rules in the same scope have stable path order; natural-language contradictions are not mechanically resolved.\n",
            if current_project {
                "current project, refreshed for this turn; an active workflow may use an earlier frozen package"
            } else {
                "frozen input of this workflow, unchanged across copies and attempts"
            }
        );
        for one in &self.sources {
            let directory = if one.facts.directory.as_os_str().is_empty() {
                ".".to_owned()
            } else {
                one.facts.directory.to_string_lossy().into_owned()
            };
            let _ = write!(
                text,
                "\nSource: {}\nScope directory: {directory}\nPaths: {}\nSource text:\n{}\nEnd source.\n",
                one.facts.path.display(),
                serde_json::to_string(&one.facts.paths).unwrap_or_default(),
                one.text
            );
        }
        text
    }
    fn validate(&self) -> io::Result<()> {
        if self.schema != 1
            || uuid::Uuid::parse_str(&self.id).is_err()
            || self.sources.len() > FILE_LIMIT
        {
            return Err(io::Error::other(
                "The saved project instruction package is not supported.",
            ));
        }
        let mut total = 0;
        for one in &self.sources {
            let facts = &one.facts;
            normal_path(&facts.path)?;
            if !facts.directory.as_os_str().is_empty() {
                normal_path(&facts.directory)?;
            }
            if facts.kind == Kind::Unknown
                || one.text.len() != facts.bytes
                || facts.bytes > FILE_BYTES
                || digest(one.text.as_bytes()) != facts.digest
            {
                return Err(source_error(
                    &facts.path,
                    "the saved source changed or is incomplete",
                ));
            }
            total += facts.bytes;
        }
        if total > TOTAL_BYTES {
            return Err(io::Error::other(
                "Project instructions exceed the 512 KiB package limit.",
            ));
        }
        Ok(())
    }
    pub fn save_to(&self, destination: &Path) -> io::Result<Self> {
        self.validate()?;
        let root = PublicationRoot::open(destination)?;
        let exists = match root.entry_identity(Path::new(SNAPSHOT)) {
            Ok(found) => found.is_some(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) => return Err(error),
        };
        if exists {
            let existing = read_snapshot(destination)?;
            if existing.id != self.id {
                return Err(io::Error::other(
                    "A different project instruction package already belongs to this agent.",
                ));
            }
            return Ok(existing);
        }
        root.ensure_directory(Path::new("instructions"), 0o700)?;
        DurableFilePublisher::new(destination)
            .atomic_create_if_absent(
                &destination.join(SNAPSHOT),
                &serde_json::to_vec(self).map_err(io::Error::other)?,
                ModePolicy::Exact(PRIVATE_FILE_MODE),
            )
            .map_err(super::super::durable_file::PublishError::into_io)?;
        read_snapshot(destination)
    }
}

pub fn read_snapshot(directory: &Path) -> io::Result<InstructionSnapshot> {
    let root = PublicationRoot::open(directory)?;
    let bytes = bounded_read(&root, Path::new(SNAPSHOT), 4 * 1024 * 1024)?;
    let snapshot: InstructionSnapshot = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    snapshot.validate()?;
    if let Some(binding) = run_binding(directory)?
        && (binding.get("id").and_then(Value::as_str) != Some(snapshot.id())
            || binding.get("digest").and_then(Value::as_str)
                != Some(snapshot.package_digest()?.as_str()))
    {
        return Err(io::Error::other(
            "The saved project instruction package does not match this run's record.",
        ));
    }
    Ok(snapshot)
}

fn run_binding(directory: &Path) -> io::Result<Option<Value>> {
    let root = PublicationRoot::open(directory)?;
    if root.entry_identity(Path::new("run.json"))?.is_none() {
        return Ok(None);
    }
    let bytes = bounded_read(&root, Path::new("run.json"), 64 * 1024 * 1024)?;
    let file: Value = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    match file.get("project_instructions") {
        None | Some(Value::Null) => Ok(None),
        Some(value) if value.is_object() => Ok(Some(value.clone())),
        Some(_) => Err(io::Error::other(
            "The saved run's project instruction record cannot be read.",
        )),
    }
}

/// Snapshot wcześniejszego biegu jest nadrzędny wobec dzisiejszych ustawień projektu.
/// Stary bieg bez pakietu nie wciąga instrukcji, których przy swoim Starcie nie używał.
pub fn for_run(
    project: &Path,
    run_dir: &Path,
    choices: &[(String, Option<bool>)],
    previous: Option<&Path>,
) -> io::Result<InstructionSnapshot> {
    match fs::symlink_metadata(run_dir.join(SNAPSHOT)) {
        Ok(_) => return read_snapshot(run_dir),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if run_binding(run_dir)?.is_some() {
                return Err(io::Error::other(
                    "This run's saved project instructions are missing; Loadout did not use today's project instead.",
                ));
            }
        }
        Err(error) => return Err(error),
    }
    if let Some(previous) = previous {
        let snapshot = match fs::symlink_metadata(previous.join(SNAPSHOT)) {
            Ok(_) => read_snapshot(previous)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if run_binding(previous)?.is_some()
                    || choices.iter().any(|(_, value)| *value == Some(true))
                {
                    return Err(io::Error::other(
                        "The earlier run's selected project instructions are missing; nothing was read from today's project.",
                    ));
                }
                InstructionSnapshot::empty()
            }
            Err(error) => return Err(error),
        };
        return snapshot.save_to(run_dir);
    }
    let settings = read_settings(project)?;
    let enabled: BTreeSet<_> = choices
        .iter()
        .filter(|(_, choice)| settings.enabled_for(*choice))
        .map(|(key, _)| key.clone())
        .collect();
    let mut snapshot = if enabled.is_empty() {
        InstructionSnapshot::empty()
    } else {
        resolve(project, settings.instructions.include_local)?
    };
    snapshot.enabled_steps = enabled;
    snapshot.save_to(run_dir)
}

pub fn for_lead(project: &Path) -> io::Result<InstructionSnapshot> {
    let settings = read_settings(project)?;
    if settings.enabled_for(settings.lead_instructions) {
        resolve(project, settings.instructions.include_local)
    } else {
        Ok(InstructionSnapshot::empty())
    }
}

pub fn override_from(value: Option<&Value>) -> io::Result<Option<bool>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        _ => Err(io::Error::other(
            "Project instructions must be inherited, enabled, or disabled.",
        )),
    }
}

pub fn discover(project: &Path) -> io::Result<Vec<InstructionSource>> {
    let root = PublicationRoot::open(project)?;
    let mut sources = Vec::new();
    for (path, kind, directory) in candidates(&root)? {
        // Podgląd ujawnia tylko rozmiar; nie interpretuje include ani treści przed opt-in.
        let bytes = root
            .open_regular_file(&path)
            .and_then(|file| file.metadata())
            .map_err(|error| source_error(&path, &error.to_string()))?
            .len();
        sources.push(InstructionSource {
            local: kind == Kind::Local,
            path,
            kind,
            directory,
            paths: Vec::new(),
            digest: String::new(),
            bytes: usize::try_from(bytes).unwrap_or(usize::MAX),
        });
    }
    Ok(sources)
}

fn resolve(project: &Path, local: bool) -> io::Result<InstructionSnapshot> {
    let root = PublicationRoot::open(project)?;
    let mut resolver = Resolver {
        root: &root,
        local,
        total: 0,
        sources: Vec::new(),
        seen: BTreeSet::new(),
    };
    for (path, kind, directory) in candidates(&root)? {
        if kind == Kind::Local && !local {
            continue;
        }
        resolver.visit(&path, kind, &directory, Vec::new(), &mut Vec::new())?;
    }
    for source in &resolver.sources {
        if digest(&bounded_read(&root, &source.facts.path, FILE_BYTES)?) != source.facts.digest {
            return Err(source_error(
                &source.facts.path,
                "the file changed while instructions were being captured",
            ));
        }
    }
    root.validate_path_identity(project)?;
    let mut snapshot = InstructionSnapshot::empty();
    snapshot.sources = resolver.sources;
    snapshot.validate()?;
    Ok(snapshot)
}

struct Resolver<'a> {
    root: &'a PublicationRoot,
    local: bool,
    total: usize,
    sources: Vec<Source>,
    seen: BTreeSet<(PathBuf, PathBuf, Vec<String>)>,
}

impl Resolver<'_> {
    fn visit(
        &mut self,
        path: &Path,
        kind: Kind,
        directory: &Path,
        mut paths: Vec<String>,
        stack: &mut Vec<PathBuf>,
    ) -> io::Result<()> {
        if stack.iter().any(|one| one == path) {
            return Err(source_error(path, "an include cycle was found"));
        }
        if path
            .file_name()
            .is_some_and(|name| name == "CLAUDE.local.md")
            && !self.local
        {
            return Err(source_error(
                path,
                "CLAUDE.local.md requires a separate explicit choice",
            ));
        }
        normal_path(path)?;
        let raw = bounded_read(self.root, path, FILE_BYTES)?;
        let text = String::from_utf8(raw)
            .map_err(|_| source_error(path, "the source is not UTF-8 text"))?;
        if kind == Kind::Rule {
            paths = rule_paths(&text).map_err(|error| source_error(path, &error))?;
        }
        if !self
            .seen
            .insert((path.to_path_buf(), directory.to_path_buf(), paths.clone()))
        {
            return Ok(());
        }
        self.total = self.total.saturating_add(text.len());
        if self.sources.len() >= FILE_LIMIT || self.total > TOTAL_BYTES {
            return Err(source_error(
                path,
                "project instructions exceed 256 files or 512 KiB in total",
            ));
        }
        self.sources.push(Source {
            facts: InstructionSource {
                path: path.to_path_buf(),
                kind,
                directory: directory.to_path_buf(),
                paths: paths.clone(),
                digest: digest(text.as_bytes()),
                bytes: text.len(),
                local: kind == Kind::Local,
            },
            text: text.clone(),
        });
        stack.push(path.to_path_buf());
        let mut fence: Option<&str> = None;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with("```") || line.starts_with("~~~") {
                let marker = &line[..3];
                if fence == Some(marker) {
                    fence = None;
                } else if fence.is_none() {
                    fence = Some(marker);
                }
                continue;
            }
            if fence.is_some() {
                continue;
            }
            if let Some(include) = line.strip_prefix("@include ") {
                let target = relative_include(path, include.trim())
                    .map_err(|error| source_error(path, &error.to_string()))?;
                self.visit(&target, Kind::Include, directory, paths.clone(), stack)?;
            }
        }
        stack.pop();
        Ok(())
    }
}

fn candidates(root: &PublicationRoot) -> io::Result<Vec<(PathBuf, Kind, PathBuf)>> {
    let mut found = Vec::new();
    let mut todo = vec![PathBuf::new()];
    while let Some(directory) = todo.pop() {
        for one in root.list_directory(&directory)? {
            if [".git", ".loadout", "node_modules", "target"]
                .iter()
                .any(|skip| one.name == *skip)
            {
                continue;
            }
            let path = directory.join(&one.name);
            let kind = match one.name.to_str() {
                Some("AGENTS.md") => Some(Kind::Agents),
                Some("CLAUDE.md") => Some(Kind::Claude),
                Some("CLAUDE.local.md") => Some(Kind::Local),
                _ => None,
            };
            if let Some(kind) = kind {
                found.push((path.clone(), kind, directory.clone()));
            } else if path.extension().is_some_and(|extension| extension == "md")
                && let Some(scope) = rules_scope(&path)
            {
                found.push((path.clone(), Kind::Rule, scope));
            }
            if found.len() > FILE_LIMIT {
                return Err(source_error(
                    &path,
                    "more than 256 project instruction files were found",
                ));
            }
            if one.kind == PublicationEntryKind::Directory {
                todo.push(path);
            }
        }
    }
    found.sort_by_key(|(path, kind, directory)| {
        (
            directory.components().count(),
            directory.clone(),
            priority(*kind),
            path.clone(),
        )
    });
    Ok(found)
}

fn priority(kind: Kind) -> u8 {
    match kind {
        Kind::Claude => 0,
        Kind::Local => 1,
        Kind::Rule => 2,
        Kind::Include => 3,
        Kind::Agents => 4,
        Kind::Unknown => 5,
    }
}

fn rules_scope(path: &Path) -> Option<PathBuf> {
    let components: Vec<_> = path.components().collect();
    components
        .windows(2)
        .position(|parts| parts[0].as_os_str() == ".claude" && parts[1].as_os_str() == "rules")
        .map(|at| {
            components[..at]
                .iter()
                .map(|part| part.as_os_str())
                .collect()
        })
}

fn rule_paths(text: &str) -> Result<Vec<String>, String> {
    let mut lines = text.lines();
    if lines.next() != Some("---") {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    let mut listing = false;
    let mut closed = false;
    for line in lines {
        if line == "---" {
            closed = true;
            break;
        }
        if let Some(value) = line.strip_prefix("paths:") {
            let value = value.trim();
            if value.is_empty() {
                listing = true;
                continue;
            }
            paths = serde_json::from_str::<Vec<String>>(value).map_err(|_| {
                "Use paths as an indented list or a JSON array of strings.".to_owned()
            })?;
            listing = false;
        } else if listing && (line.starts_with(' ') || line.trim().is_empty()) {
            if line.trim().is_empty() {
                continue;
            }
            let value = line
                .trim()
                .strip_prefix("- ")
                .ok_or_else(|| "Use paths as an indented list of strings.".to_owned())?;
            paths.push(if value.starts_with('"') {
                serde_json::from_str(value)
                    .map_err(|_| "A paths entry has invalid quotes.".to_owned())?
            } else if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
                value[1..value.len() - 1].replace("''", "'")
            } else {
                value.to_owned()
            });
        } else {
            listing = false;
        }
    }
    if !closed {
        return Err("The rule's front matter is not closed.".to_owned());
    }
    crate::workflow::validate_additional_inputs(&paths)?;
    Ok(paths)
}

fn relative_include(source: &Path, include: &str) -> io::Result<PathBuf> {
    if include.is_empty() || Path::new(include).is_absolute() || include.contains(['\\', '\0']) {
        return Err(io::Error::other("the include must stay inside the project"));
    }
    let mut path = source
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .to_path_buf();
    for part in Path::new(include).components() {
        match part {
            Component::Normal(one) => path.push(one),
            Component::CurDir => {}
            Component::ParentDir if path.pop() => {}
            _ => return Err(io::Error::other("the include leaves the project")),
        }
    }
    normal_path(&path)?;
    Ok(path)
}

fn normal_path(path: &Path) -> io::Result<()> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|one| !matches!(one, Component::Normal(_)))
    {
        return Err(source_error(
            path,
            "the source path must stay inside the project",
        ));
    }
    Ok(())
}

fn bounded_read(root: &PublicationRoot, path: &Path, limit: usize) -> io::Result<Vec<u8>> {
    let file = root
        .open_regular_file(path)
        .map_err(|error| source_error(path, &error.to_string()))?;
    if file.metadata()?.len() > limit as u64 {
        return Err(source_error(
            path,
            &format!("the source exceeds its {limit}-byte limit"),
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| source_error(path, &error.to_string()))?;
    if bytes.len() > limit {
        return Err(source_error(path, "the source grew beyond its size limit"));
    }
    Ok(bytes)
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn source_error(path: &Path, why: &str) -> io::Error {
    io::Error::other(format!(
        "Project instructions in {} could not be used: {why}",
        path.display()
    ))
}
