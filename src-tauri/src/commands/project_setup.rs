//! Jawny import wybranych kopii pomiędzy bibliotekami projektów.
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SetupItem {
    pub key: String,
    pub category: String,
    pub name: String,
    pub summary: String,
    pub preview: String,
    pub requires: Vec<String>,
    pub problems: Vec<String>,
    pub already_here: bool,
    #[serde(default)]
    pub reusable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupPreview {
    pub revision: String,
    pub items: Vec<SetupItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupReceipt {
    pub imported: Vec<String>,
}

use crate::library::agents::{Agent, agent_file_name, read_agent_directory, write_agent_file};
use crate::workflow::{Skills, Step};
use crate::{
    durable_file::{DurableFilePublisher, ModePolicy, revision_of},
    engine::supervisor::{PublicationEntryKind, PublicationIdentity, PublicationRoot},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::PathBuf,
};

const FILE_BYTES: u64 = 64 * 1024 * 1024;
const TOTAL_BYTES: usize = 256 * 1024 * 1024;
const FILES: usize = 10_000;

struct PreparedItem {
    view: SetupItem,
    files: BTreeMap<PathBuf, PreparedFile>,
    source_stamp: String,
}

#[derive(Clone)]
struct PreparedFile {
    bytes: Vec<u8>,
    executable: bool,
}

struct CopiedFile<'a> {
    path: &'a Path,
    identity: PublicationIdentity,
    bytes: &'a [u8],
}
impl From<Vec<u8>> for PreparedFile {
    fn from(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            executable: false,
        }
    }
}

pub fn preview(
    source: &Path,
    project: Option<&Path>,
    destination: &Path,
) -> Result<SetupPreview, String> {
    let items = prepare(source, project, destination)?;
    view_of(&items)
}

fn view_of(items: &[PreparedItem]) -> Result<SetupPreview, String> {
    // Odcisk zawiera CAŁE kopiowane bajty, także skrypty i źródła Context, a nie tylko opis karty.
    let stamp: Vec<_> = items
        .iter()
        .map(|item| {
            (
                &item.view,
                &item.source_stamp,
                item.files
                    .iter()
                    .map(|(path, bytes)| (path, revision_of(&bytes.bytes), bytes.executable))
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    Ok(SetupPreview {
        revision: revision_of(&serde_json::to_vec(&stamp).map_err(said)?),
        items: items.iter().map(|item| item.view.clone()).collect(),
    })
}

pub fn apply(
    source: &Path,
    project: Option<&Path>,
    destination: &Path,
    revision: &str,
    selected: &[String],
) -> Result<SetupReceipt, String> {
    apply_with_hook(source, project, destination, revision, selected, |_| Ok(()))
}

/// Hak sprawdza przerwany zapis. Nie zmienia wyboru ani zasad publikowania.
pub fn apply_with_hook(
    source: &Path,
    project: Option<&Path>,
    destination: &Path,
    revision: &str,
    selected: &[String],
    mut after_file: impl FnMut(usize) -> Result<(), String>,
) -> Result<SetupReceipt, String> {
    let items = prepare(source, project, destination)?;
    if view_of(&items)?.revision != revision {
        return Err(
            "The setup changed since you opened it. Refresh the preview and choose again."
                .to_owned(),
        );
    }
    let selected: BTreeSet<_> = selected.iter().cloned().collect();
    if selected.is_empty() {
        return Err("Choose at least one item to import.".to_owned());
    }
    for key in &selected {
        let item = items
            .iter()
            .find(|item| &item.view.key == key)
            .ok_or("An item is no longer available. Refresh the preview.")?;
        if (item.view.already_here && !item.view.reusable) || !item.view.problems.is_empty() {
            return Err(format!(
                "{} cannot be imported. {}",
                item.view.name,
                item.view.problems.join(" ")
            ));
        }
        if item.view.requires.iter().any(|key| !selected.contains(key)) {
            return Err(format!(
                "Include the items required by {} before importing.",
                item.view.name
            ));
        }
    }
    let mut files = BTreeMap::new();
    for item in items
        .iter()
        .filter(|item| selected.contains(&item.view.key) && !item.view.reusable)
    {
        for (path, bytes) in &item.files {
            if files.insert(path.clone(), bytes.clone()).is_some() {
                return Err(format!(
                    "Two items would use {}. Rename one in the source project first.",
                    path.display()
                ));
            }
        }
    }
    // Brak nadpisywania jest wymuszony przez publikator także przy wyścigu po podglądzie.
    fs::create_dir_all(destination).map_err(said)?;
    let root = PublicationRoot::open(destination).map_err(said)?;
    let publisher = DurableFilePublisher::new(destination);
    let mut files: Vec<_> = files.into_iter().collect();
    // Definicje pojawiają się dopiero z całym materiałem; workflow jest ostatnim odbiorcą.
    files.sort_by_key(|(path, _)| (publication_order(path), path.clone()));
    let mut written = Vec::new();
    let result = (|| {
        for (path, bytes) in &files {
            if let Some(parent) = path.parent() {
                root.ensure_directory(parent, 0o700).map_err(said)?;
            }
            let identity = publisher
                .with_publication(|batch| {
                    batch.atomic_create_if_absent_with_identity(
                        &destination.join(path),
                        &bytes.bytes,
                        ModePolicy::Exact(if bytes.executable { 0o700 } else { 0o600 }),
                    )
                })
                .map_err(said)?;
            written.push(CopiedFile {
                path,
                identity,
                bytes: &bytes.bytes,
            });
            after_file(written.len())?;
        }
        Ok::<_, String>(())
    })();
    if let Err(error) = result {
        return Err(undo_import(&root, &written, &error));
    }
    Ok(SetupReceipt {
        imported: items
            .iter()
            .filter(|item| selected.contains(&item.view.key) && !item.view.reusable)
            .map(|item| item.view.key.clone())
            .collect(),
    })
}

fn undo_import(root: &PublicationRoot, written: &[CopiedFile<'_>], error: &str) -> String {
    let mut retained = false;
    for copied in written.iter().rev() {
        // 2026-09-10: an editor can change bytes without changing the inode.
        // Keep those edits, but still clean every other unchanged file in the batch.
        let mut current = Vec::new();
        let unchanged = root
            .open_regular_file(copied.path)
            .and_then(|file| {
                file.take(copied.bytes.len() as u64 + 1)
                    .read_to_end(&mut current)
            })
            .is_ok()
            && current == copied.bytes;
        if !unchanged
            || !root
                .remove_entry_if_identity(
                    copied.path,
                    (PublicationEntryKind::Regular, copied.identity),
                )
                .unwrap_or(false)
        {
            retained = true;
        }
    }
    if retained {
        format!(
            "Import stopped: {error} Some copied files changed or could not be removed; they were kept."
        )
    } else {
        format!("Nothing was imported. {error}")
    }
}

fn publication_order(path: &Path) -> u8 {
    if path.starts_with("workflows") {
        4
    } else if path.starts_with("contexts")
        && path.components().count() == 3
        && path.file_name().is_some_and(|name| name == "manifest.json")
    {
        3
    } else if path.starts_with("agents")
        || path
            .file_name()
            .is_some_and(|name| name == "manifest.json" || name == "SKILL.md")
    {
        2
    } else {
        1
    }
}

fn prepare(
    source: &Path,
    project: Option<&Path>,
    destination: &Path,
) -> Result<Vec<PreparedItem>, String> {
    if source == destination
        || (source.exists()
            && destination.exists()
            && fs::canonicalize(source).map_err(said)?
                == fs::canonicalize(destination).map_err(said)?)
    {
        return Err("Choose a different project to import from.".to_owned());
    }
    let mut items = Vec::new();
    let stage = tempfile::tempdir().map_err(said)?;
    let agents = super::agents::list_agents_inner(source).map_err(said)?;
    collect_agents(source, project, destination, stage.path(), &mut items)?;
    collect_skills(source, project, &agents, stage.path(), &mut items)?;
    collect_workflows(source, project, destination, &mut items)?;
    collect_notes(source, &agents, &mut items)?;
    collect_contexts(source, destination, &mut items)?;
    collect_connections(source, project, &mut items)?;
    validate_items(destination, &mut items)?;
    Ok(items)
}

fn collect_agents(
    source: &Path,
    project: Option<&Path>,
    destination: &Path,
    stage: &Path,
    items: &mut Vec<PreparedItem>,
) -> Result<(), String> {
    let existing_agents = super::agents::list_agents_inner(destination).map_err(said)?;
    for (path, read) in read_agent_directory(&source.join("agents")).map_err(said)? {
        let mut agent = match read {
            Ok(read) => read.agent,
            Err(error) => {
                items.push(problem_item("agent", &path, &error.to_string()));
                continue;
            }
        };
        let mut item = item(
            "agent",
            &agent.id.to_string(),
            &agent.name,
            &agent.summary,
            &agent.instructions,
        );
        item.view
            .requires
            .extend(agent.skills.iter().map(|name| format!("skill:{name}")));
        item.view.requires.extend(
            agent
                .connections
                .iter()
                .map(|id| format!("connection:{id}")),
        );
        item.view.already_here = existing_agents.iter().any(|other| other.id == agent.id);
        retarget_skills(&mut agent.extra, destination);
        // Zapis do izolowanego bufora przechodzi zwykłą bramkę sekretów i serializator agenta.
        match write_agent_file(stage, &agent, None) {
            Ok(written) => {
                item.files.insert(
                    PathBuf::from("agents").join(agent_file_name(&agent)),
                    read_bytes(stage, &written.path)?.into(),
                );
            }
            Err(error) => item.view.problems.push(error.to_string()),
        }
        // Rewizja opiera się także na oryginale: zmiana wskazania źródła umiejętności unieważnia podgląd.
        let source_bytes = read_bytes(source, &path)?;
        item.view.preview.clone_from(&agent.instructions);
        item.view.summary = if agent.summary.is_empty() {
            format!("{} · {}", vendor(&agent), agent.model)
        } else {
            agent.summary.clone()
        };
        item.view.problems.extend(source_problems(&agent, project));
        item.source_stamp = revision_of(&source_bytes);
        items.push(item);
    }
    Ok(())
}

fn collect_workflows(
    source: &Path,
    project: Option<&Path>,
    destination: &Path,
    items: &mut Vec<PreparedItem>,
) -> Result<(), String> {
    for definition in
        super::workflows::list_workflow_definitions_inner(source, None).map_err(said)?
    {
        let entry = match definition {
            crate::library::definition::Definition::Healthy { value, .. } => value,
            crate::library::definition::Definition::DefinitionProblem { file_name, .. } => {
                items.push(problem_item(
                    "workflow",
                    Path::new(&file_name),
                    "Open this workflow in its project and fix its file before importing.",
                ));
                continue;
            }
        };
        let opened =
            super::workflows::load_workflow_inner(source, None, &entry.path).map_err(said)?;
        let mut flow = opened.workflow;
        let mut item = item(
            "workflow",
            &flow.id,
            &flow.name,
            flow.description.as_deref().unwrap_or(""),
            "",
        );
        if let Some(context) = flow.context()? {
            item.view
                .requires
                .extend(context.sets.iter().map(|set| format!("context:{}", set.id)));
        }
        let mut steps = Vec::new();
        for step in &mut flow.steps {
            if let Step::Agent(agent) = step {
                if agent
                    .overrides
                    .get("writeResultsTo")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|path| {
                        project.is_some_and(|root| Path::new(path).starts_with(root))
                    })
                {
                    item.view.problems.push(format!("{} saves results to an absolute folder in the source project. Use a relative folder there before importing.", agent.name));
                }
                item.view.requires.push(format!("agent:{}", agent.agent));
                if let Skills::Only(names) = &agent.skills {
                    item.view
                        .requires
                        .extend(names.iter().map(|name| format!("skill:{name}")));
                }
                if let Some(context) = agent.context()? {
                    item.view
                        .requires
                        .extend(context.sets.iter().map(|set| format!("context:{}", set.id)));
                }
                if let Some(names) = agent
                    .overrides
                    .get("connections")
                    .and_then(serde_json::Value::as_array)
                {
                    item.view.requires.extend(
                        names
                            .iter()
                            .filter_map(serde_json::Value::as_str)
                            .map(|id| format!("connection:{id}")),
                    );
                }
                retarget_skills(&mut agent.extra, destination);
                steps.push(agent.name.clone());
            }
        }
        item.view.preview = format!(
            "{}\n\n{} steps\n{}",
            flow.description.as_deref().unwrap_or(""),
            flow.steps.len(),
            steps.join("\n")
        );
        item.view.already_here = crate::library::definition::healthy_only(
            super::workflows::list_workflow_definitions_inner(destination, None).map_err(said)?,
        )
        .iter()
        .any(|other| other.workflow.id == flow.id);
        item.files.insert(
            PathBuf::from("workflows").join(&entry.path),
            serde_json::to_vec_pretty(&flow).map_err(said)?.into(),
        );
        items.push(item);
    }
    Ok(())
}

fn collect_notes(
    source: &Path,
    agents: &[Agent],
    items: &mut Vec<PreparedItem>,
) -> Result<(), String> {
    for note in
        crate::memory::notes::scan_notes(&super::memory::notes_root(source)).map_err(said)?
    {
        let mut item = item(
            "note",
            &note.id.to_string(),
            &note.title,
            &note.rule,
            &format!("{}\n\n{}", note.rule, note.because),
        );
        if let Some(owner) = note.agent.as_deref() {
            if let Some(agent) = agents
                .iter()
                .find(|agent| crate::memory::slugify(&agent.name) == crate::memory::slugify(owner))
            {
                item.view.requires.push(format!("agent:{}", agent.id));
            } else {
                item.view.problems.push(format!(
                    "The agent named {owner} is missing from this project."
                ));
            }
        }
        let relative = note.path.strip_prefix(source).map_err(said)?.to_path_buf();
        item.files
            .insert(relative, read_bytes(source, &note.path)?.into());
        items.push(item);
    }
    Ok(())
}

fn collect_contexts(
    source: &Path,
    destination: &Path,
    items: &mut Vec<PreparedItem>,
) -> Result<(), String> {
    for set in crate::context::files::list_sets(&source.join("contexts")).map_err(said)? {
        let folder =
            crate::context::files::folder_of(&source.join("contexts"), &set.id).map_err(said)?;
        let mut item = item(
            "context",
            &set.id,
            &set.title,
            &set.description,
            &set.description,
        );
        let read =
            crate::context::files::read_set(&source.join("contexts"), &set.id).map_err(said)?;
        item.view.preview = format!(
            "{}\n\n{} source materials",
            set.description,
            read.draft.sources.len()
        );
        match read_tree(source, &folder, true) {
            Ok(files) => item.files = files,
            Err(error) => item.view.problems.push(error),
        }
        item.view.already_here = crate::context::files::list_sets(&destination.join("contexts"))
            .map_err(said)?
            .iter()
            .any(|other| other.id == set.id);
        items.push(item);
    }
    Ok(())
}

fn collect_connections(
    source: &Path,
    project: Option<&Path>,
    items: &mut Vec<PreparedItem>,
) -> Result<(), String> {
    for connection in crate::connections::runtime::all(&source.join("connections")).map_err(said)? {
        let mut item = item(
            "connection",
            &connection.id,
            &connection.name,
            "Connection settings",
            "Copies its settings. Credentials stay in your environment and are not copied.",
        );
        if !safe_name(&connection.id) {
            item.view
                .problems
                .push("Give this connection a simple file name in its source project.".into());
        }
        let transport = serde_json::to_string(&connection.transport).map_err(said)?;
        if crate::workflow::check::a_command_carrying_a_secret_shape(&transport).is_some() {
            item.view.problems.push("This connection contains a credential. Replace it with an environment variable before importing.".into());
        }
        if project.is_some_and(|path| transport.contains(&path.to_string_lossy().to_string())) {
            item.view.problems.push("This connection points into the source project. Make its command portable before importing.".into());
        }
        item.files.insert(
            PathBuf::from("connections").join(format!("{}.json", connection.id)),
            serde_json::to_vec_pretty(&connection).map_err(said)?.into(),
        );
        items.push(item);
    }
    Ok(())
}

fn validate_items(destination: &Path, items: &mut [PreparedItem]) -> Result<(), String> {
    let keys: BTreeSet<_> = items.iter().map(|item| item.view.key.clone()).collect();
    let mut total = 0;
    let mut count = 0;
    for item in items.iter_mut() {
        item.view.requires.sort();
        item.view.requires.dedup();
        if !item.files.keys().all(|path| {
            path.components()
                .all(|part| matches!(part, std::path::Component::Normal(_)))
        }) {
            item.view
                .problems
                .push("This item contains a path outside its project.".into());
        }
        for dependency in &item.view.requires {
            if !keys.contains(dependency) {
                item.view.problems.push(format!(
                    "Required item {dependency} is missing. Add it in the source project first."
                ));
            }
        }
        item.view.already_here |= item
            .files
            .keys()
            .any(|path| fs::symlink_metadata(destination.join(path)).is_ok());
        item.view.reusable = item.view.already_here
            && item.view.problems.is_empty()
            && !item.files.is_empty()
            && item.files.iter().all(|(path, expected)| {
                let path = destination.join(path);
                read_bytes(destination, &path).is_ok_and(|bytes| bytes == expected.bytes)
                    && fs::symlink_metadata(path).is_ok_and(|metadata| {
                        crate::engine::supervisor::executable_bits(&metadata) == expected.executable
                    })
            });
        if item.view.already_here && !item.view.reusable {
            item.view.problems.push("An item with this identity or file name is already in this project. Existing items are kept.".to_owned());
        }
        total += item
            .files
            .values()
            .map(|file| file.bytes.len())
            .sum::<usize>();
        count += item.files.len();
        if total > TOTAL_BYTES || count > FILES {
            return Err("This setup is too large to import at once.".to_owned());
        }
    }
    items.sort_by(|a, b| {
        (&a.view.category, &a.view.name, &a.view.key).cmp(&(
            &b.view.category,
            &b.view.name,
            &b.view.key,
        ))
    });
    Ok(())
}

fn collect_skills(
    source: &Path,
    project: Option<&Path>,
    agents: &[Agent],
    stage: &Path,
    items: &mut Vec<PreparedItem>,
) -> Result<(), String> {
    let roots = crate::skills::Roots {
        home: source.parent().unwrap_or(source).to_path_buf(),
        project: project.map(Path::to_path_buf),
        data: source.to_path_buf(),
    };
    for skill in super::skills::list_skills_in(source, project).map_err(said)? {
        let mut item = item(
            "skill",
            &skill.name,
            &skill.name,
            &skill.summary,
            &skill.summary,
        );
        let choices: BTreeSet<_> = agents
            .iter()
            .filter_map(|agent| {
                crate::skills::bundle::choices(&agent.extra)
                    .ok()?
                    .remove(&skill.name)
            })
            .collect();
        if choices.len() > 1 {
            item.view.problems.push("Agents use different copies of this skill. Choose one source in that project first.".to_owned());
        }
        match crate::skills::bundle::resolve(
            &roots,
            &skill.name,
            choices.first().map(PathBuf::as_path),
        ) {
            Ok(resolved) => {
                resolved
                    .bundle
                    .skill_text()
                    .map_err(said)?
                    .clone_into(&mut item.view.preview);
                let folder = stage.join("skills").join(&skill.name);
                resolved.bundle.materialize(&folder).map_err(said)?;
                match read_tree(stage, &folder, false) {
                    Ok(files) => item.files = files,
                    Err(error) => item.view.problems.push(error),
                }
            }
            Err(error) => item.view.problems.push(error.to_string()),
        }
        items.push(item);
    }
    Ok(())
}

fn source_problems(agent: &Agent, project: Option<&Path>) -> Vec<String> {
    if project.is_some_and(|project| {
        !agent.write_results_to.is_empty()
            && Path::new(&agent.write_results_to).starts_with(project)
    }) {
        vec!["This agent saves results to an absolute folder in the source project. Use a relative folder there before importing.".to_owned()]
    } else {
        Vec::new()
    }
}

fn retarget_skills(extra: &mut serde_json::Map<String, serde_json::Value>, destination: &Path) {
    if let Some(sources) = extra
        .get_mut("skillSources")
        .and_then(serde_json::Value::as_object_mut)
    {
        for (name, path) in sources {
            if !safe_name(name) {
                continue;
            }
            *path = serde_json::Value::String(
                destination
                    .join("skills")
                    .join(name)
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
}

fn read_bytes(root: &Path, path: &Path) -> Result<Vec<u8>, String> {
    let root_handle = PublicationRoot::open(root).map_err(said)?;
    let relative = path.strip_prefix(root).map_err(said)?;
    let file = root_handle.open_regular_file(relative).map_err(said)?;
    let mut bytes = Vec::new();
    file.take(FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(said)?;
    if bytes.len() as u64 > FILE_BYTES {
        return Err("A setup file is too large to import.".to_owned());
    }
    Ok(bytes)
}

fn read_tree(
    root: &Path,
    folder: &Path,
    context: bool,
) -> Result<BTreeMap<PathBuf, PreparedFile>, String> {
    let mut files = BTreeMap::new();
    let mut pending = vec![folder.to_path_buf()];
    let mut size = 0;
    while let Some(dir) = pending.pop() {
        if fs::symlink_metadata(&dir)
            .map_err(said)?
            .file_type()
            .is_symlink()
        {
            return Err(
                "A setup folder is a link to another location. Import its original folder instead."
                    .to_owned(),
            );
        }
        for entry in fs::read_dir(&dir).map_err(said)? {
            let entry = entry.map_err(said)?;
            let path = entry.path();
            let name = entry.file_name();
            if context && (name.to_string_lossy().starts_with('.') || name == "builds") {
                continue;
            }
            let kind = entry.file_type().map_err(said)?;
            if kind.is_symlink() {
                return Err("A setup item contains a symbolic link. Replace it with a local file before importing.".to_owned());
            }
            if kind.is_dir() {
                pending.push(path);
                continue;
            }
            if !kind.is_file() {
                return Err("A setup item contains something other than a regular file.".to_owned());
            }
            let bytes = read_bytes(root, &path)?;
            size += bytes.len();
            files.insert(
                path.strip_prefix(root).map_err(said)?.to_path_buf(),
                PreparedFile {
                    bytes,
                    executable: crate::engine::supervisor::executable_bits(
                        &fs::metadata(&path).map_err(said)?,
                    ),
                },
            );
            if files.len() > FILES || size > TOTAL_BYTES {
                return Err("This setup item is too large to import.".to_owned());
            }
        }
    }
    Ok(files)
}

fn item(category: &str, id: &str, name: &str, summary: &str, preview: &str) -> PreparedItem {
    PreparedItem {
        view: SetupItem {
            key: format!("{category}:{id}"),
            category: category.to_owned(),
            name: name.to_owned(),
            summary: summary.to_owned(),
            preview: preview.chars().take(24_000).collect(),
            requires: Vec::new(),
            problems: Vec::new(),
            already_here: false,
            reusable: false,
        },
        files: BTreeMap::new(),
        source_stamp: String::new(),
    }
}

fn problem_item(category: &str, path: &Path, problem: &str) -> PreparedItem {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let mut item = item(category, &name, &name, "Needs attention", "");
    item.view.problems.push(problem.to_owned());
    item
}

fn vendor(agent: &Agent) -> &'static str {
    match agent.runs_with {
        crate::library::agents::Vendor::Codex => "Codex",
        crate::library::agents::Vendor::ClaudeCode => "Claude Code",
    }
}
fn said(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}
