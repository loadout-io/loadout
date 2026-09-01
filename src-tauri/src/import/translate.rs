//! Złożenie inventory w raport zgodności i natywny graf.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::library::agents::Agent;
use crate::workflow::check::{Level, check_to_run};
use crate::workflow::{
    AgentStep, CheckStep, CheckpointStep, Folder, Handover, Link, PlainNotes, Point, Skills, Step,
    WhenItFails, WorkflowFile,
};

pub use crate::workflow::{CheckOutcome, Condition, ConditionalLink, RouteEvidence as Evidence};

use super::adapters::{self, WorkflowSource, adapt, take_connections};
use super::discover::{Inspection, scan};
use super::{
    ADAPTER_VERSION, Compatibility, CompatibilityReport, ImportItem, ImportPreview, ImportSource,
    ImportSourceRole, ImportStatus, ItemKind, Mapping, MigrationDraft, Result, SourceItem,
};

/// Pełny Scan: odczyt i translacja w jednym backendowym przebiegu, zanim dane trafią do okna.
pub fn preview(root: &Path) -> Result<ImportPreview> {
    let inspection = scan(root)?;
    Ok(from_inspection(inspection, None))
}

/// Ten sam Scan, plus **twoje własne** zakresy MCP z `~/.claude.json`.
///
/// 2026-08-22 — osobne wejście, a nie zmieniona [`preview`], i to jest wybór na rzecz kryteriów:
/// zestawy w `tests/it/` sądzą import na katalogu tymczasowym i nie mają prawa czytać konfiguracji
/// człowieka, który akurat uruchomił testy. Produkt woła tę funkcję, testy tamtą — a różnica
/// między nimi jest jednym argumentem, nie drugą ścieżką kodu.
pub fn preview_with_personal(root: &Path, home: &Path) -> Result<ImportPreview> {
    let inspection = scan(root)?;
    Ok(from_inspection(inspection, Some(home)))
}

fn from_inspection(inspection: Inspection, home: Option<&Path>) -> ImportPreview {
    let mut adapted = adapt(&inspection);

    /* TWOJE ZAKRESY DOCHODZĄ PO PROJEKTOWYCH, więc przy powtórzonej nazwie wygrywa plik projektu.
     * To jest ta sama reguła, co przy dwóch opisach jednego serwera w repo (`take_connections`),
     * i ten sam powód: dwa wpisy pod jedną nazwą dałyby w bibliotece dwa pliki, a człowiek
     * włączyłby jeden, podczas gdy bieg czyta drugi. */
    if let Some(home) = home {
        let mut mine = adapters::personal_connections(home, &inspection.snapshot.root);
        take_connections(&mut adapted.connections, &mut mine.connections);
    }
    let source_hashes = inspection
        .snapshot
        .items
        .iter()
        .map(|item| (item.path.clone(), item.hash.clone()))
        .collect();
    let mut imported_workflows = imported_workflows(&inspection, &adapted.agents, &adapted.skills);
    reconcile_workflow_targets(&mut imported_workflows);
    for imported in &imported_workflows {
        let Some((compatibility, message)) = &imported.mapping_override else {
            continue;
        };
        if let Some(mapping) = adapted
            .mappings
            .iter_mut()
            .find(|mapping| mapping.item_id == imported.item_id)
        {
            mapping.compatibility = *compatibility;
            message.clone_into(&mut mapping.message);
        }
    }
    let items = typed_items(&inspection, &adapted, &imported_workflows);
    let workflows = unique_workflow_outputs(&imported_workflows);
    let mut draft = MigrationDraft {
        root: inspection.snapshot.root.clone(),
        source_hashes,
        items,
        agents: adapted.agents,
        skills: adapted.skills,
        connections: adapted.connections,
        workflows,
        // Pamięć projektu jako notatki (2026-08-22, T-80). Składa je adapter, bo to on czyta
        // katalogi vendorów — tutaj mieszka wyłącznie polityka zgodności.
        notes: adapted.notes,
        report: CompatibilityReport {
            mappings: adapted.mappings,
        },
    };
    // Wektory przejściowe są materializacją wybranych pozycji. Dziś wszystkie są wybrane;
    // ta sama funkcja biegnie po decyzjach człowieka tuż przed `apply`.
    keep_selected_outputs(&mut draft);
    refresh_statuses(&mut draft);
    ImportPreview {
        snapshot: inspection.snapshot,
        draft,
    }
}

/// Przelicza gotowość po każdej zmianie planu — także po włączeniu Connections.
pub fn refresh_statuses(draft: &mut MigrationDraft) {
    let mappings: BTreeMap<_, _> = draft
        .report
        .mappings
        .iter()
        .map(|mapping| {
            (
                mapping.item_id.clone(),
                (mapping.compatibility, mapping.message.clone()),
            )
        })
        .collect();
    for item in &mut draft.items {
        let Some((compatibility, message)) = mappings.get(&item.id) else {
            item.status = ImportStatus::Unsupported;
            "Loadout has no compatibility result for this item."
                .clone_into(&mut item.status_message);
            continue;
        };
        item.status = match compatibility {
            Compatibility::Exact | Compatibility::Adjusted => ImportStatus::Ready,
            Compatibility::NeedsChoice => ImportStatus::NeedsChoice,
            Compatibility::Unsupported => ImportStatus::Unsupported,
        };
        message.clone_into(&mut item.status_message);
    }

    /* 2026-08-28, review T-78: sam obiekt w starym `draft.agents` nie jest dowodem, że jego
     * typowana pozycja została wybrana i domknięta. Startujemy wyłącznie od pozycji Ready,
     * a potem monotonicznie usuwamy te, których zależność nie należy do tego samego zbioru.
     * Dzięki temu A -> B -> brakujące C nie zależy od kolejności w inventory. */
    let mut ready: BTreeSet<String> = draft
        .items
        .iter()
        .filter(|item| item.status == ImportStatus::Ready)
        .map(|item| item.id.clone())
        .collect();
    loop {
        let unavailable: Vec<_> = draft
            .items
            .iter()
            .filter(|item| ready.contains(&item.id))
            .filter(|item| {
                item.dependencies
                    .iter()
                    .any(|dependency| !dependency_is_ready(draft, &ready, dependency))
            })
            .map(|item| item.id.clone())
            .collect();
        if unavailable.is_empty() {
            break;
        }
        for id in unavailable {
            ready.remove(&id);
        }
    }

    let missing: BTreeMap<_, Vec<_>> = draft
        .items
        .iter()
        .filter(|item| item.status == ImportStatus::Ready && !ready.contains(&item.id))
        .map(|item| {
            let dependencies = item
                .dependencies
                .iter()
                .filter(|dependency| !dependency_is_ready(draft, &ready, dependency))
                .cloned()
                .collect();
            (item.id.clone(), dependencies)
        })
        .collect();
    for item in &mut draft.items {
        let Some(dependencies) = missing.get(&item.id) else {
            continue;
        };
        item.status = ImportStatus::MissingDependencies;
        item.status_message = format!(
            "Blocked because {} will not be imported or enabled.",
            dependencies.join(", ")
        );
    }
}

fn dependency_is_ready(draft: &MigrationDraft, ready: &BTreeSet<String>, dependency: &str) -> bool {
    let Some((kind, name)) = dependency.split_once(':') else {
        return false;
    };
    match kind {
        "skill" => draft
            .skills
            .iter()
            .find(|skill| skill.name == name)
            .is_some_and(|skill| {
                let target = PathBuf::from("skills").join(&skill.name);
                draft.items.iter().any(|item| {
                    ready.contains(&item.id)
                        && item.kind == ItemKind::Skill
                        && item.target.as_deref().is_some_and(|item_target| {
                            item_target == target.as_path() || item_target.starts_with(&target)
                        })
                })
            }),
        "agent" => draft
            .agents
            .iter()
            .find(|agent| {
                agent.id.to_string().eq_ignore_ascii_case(name)
                    || agent.name.eq_ignore_ascii_case(name)
            })
            .is_some_and(|agent| {
                let target = agent_target(agent);
                draft.items.iter().any(|item| {
                    ready.contains(&item.id)
                        && item.kind == ItemKind::Agent
                        && item.target.as_deref() == Some(target.as_path())
                })
            }),
        "connection" => draft
            .connections
            .iter()
            .filter(|connection| connection.enabled)
            .find(|connection| {
                connection.id.eq_ignore_ascii_case(name)
                    || connection.name.eq_ignore_ascii_case(name)
            })
            .is_some_and(|connection| {
                // Osobiste połączenia są wybierane osobnym przełącznikiem i nie mają SourceItem.
                connection.source == Path::new(".claude.json")
                    || draft.items.iter().any(|item| {
                        ready.contains(&item.id)
                            && item.kind == ItemKind::Connection
                            && item
                                .sources
                                .iter()
                                .any(|source| source.path == connection.source)
                    })
            }),
        _ => false,
    }
}

/// Po odznaczeniu pozycji stare wektory nie mogą zachować pliku, którego w planie już nie ma.
pub fn keep_selected_outputs(draft: &mut MigrationDraft) {
    draft
        .agents
        .retain(|agent| owns_target(&draft.items, &agent_target(agent)));
    draft.skills.retain(|skill| {
        let target = PathBuf::from("skills").join(&skill.name);
        owns_target_or_child(&draft.items, &target)
    });
    draft.connections.retain(|connection| {
        // Osobiste zakresy nie mają SourceItem w repo. Są jawnie pokazane jako "yours" i ich
        // osobny przełącznik nadal jest decyzją człowieka, więc brak repo-itemu ich nie usuwa.
        connection.source == Path::new(".claude.json")
            || draft.items.iter().any(|item| {
                item.sources
                    .iter()
                    .any(|source| source.path == connection.source)
            })
    });
    draft.workflows.retain(|workflow| {
        let target = PathBuf::from("workflows").join(format!("{}.json", slug(&workflow.name)));
        owns_target(&draft.items, &target)
    });
    draft.notes.retain(|note| {
        draft.items.iter().any(|item| {
            item.kind == ItemKind::Memory
                && item.sources.iter().any(|source| {
                    source.path == note.source
                        || source.path.parent().is_some_and(|parent| {
                            !parent.as_os_str().is_empty() && note.source.starts_with(parent)
                        })
                })
        })
    });
}

fn typed_items(
    inspection: &Inspection,
    adapted: &adapters::AdapterOutput,
    workflows: &[ImportedWorkflow],
) -> Vec<ImportItem> {
    let mappings: BTreeMap<_, _> = adapted
        .mappings
        .iter()
        .map(|mapping| (mapping.item_id.as_str(), mapping))
        .collect();
    inspection
        .files
        .iter()
        .map(|file| {
            let mapping = mappings.get(file.item.id.as_str()).copied();
            let agent = agent_for(&file.item, &adapted.agents, &adapted.agent_sources);
            let skill = (file.item.kind == ItemKind::Skill)
                .then(|| {
                    adapted
                        .skills
                        .iter()
                        .find(|skill| skill.name.eq_ignore_ascii_case(&file.item.name))
                })
                .flatten();
            let connections: Vec<_> = if file.item.kind == ItemKind::Connection {
                adapted
                    .connections
                    .iter()
                    .filter(|connection| connection.source == file.item.path)
                    .collect()
            } else {
                Vec::new()
            };
            let imported_workflow = imported_workflow_for(file, workflows);
            let workflow = imported_workflow.and_then(|imported| imported.workflow.as_ref());
            let notes: Vec<_> = if file.item.kind == ItemKind::Memory {
                adapted
                    .notes
                    .iter()
                    .filter(|note| memory_item_covers(&file.item, &note.source))
                    .collect()
            } else {
                Vec::new()
            };
            let sources = sources_for(inspection, &file.item, skill, &notes);
            let target = target_for(&file.item, agent, skill, &connections, workflow, &notes);
            let dependencies = dependencies_for(&file.item, agent, workflow, imported_workflow);
            let generated_hash = generated_hash_for(agent, skill, &connections, workflow, &notes);
            let (status, status_message) = initial_status(mapping);
            ImportItem {
                id: file.item.id.clone(),
                kind: file.item.kind,
                sources,
                target,
                dependencies,
                status,
                status_message,
                generated_hash,
            }
        })
        .collect()
}

fn initial_status(mapping: Option<&Mapping>) -> (ImportStatus, String) {
    mapping.map_or_else(
        || {
            (
                ImportStatus::Unsupported,
                "Loadout has no compatibility result for this item.".to_owned(),
            )
        },
        |mapping| {
            let status = match mapping.compatibility {
                Compatibility::Exact | Compatibility::Adjusted => ImportStatus::Ready,
                Compatibility::NeedsChoice => ImportStatus::NeedsChoice,
                Compatibility::Unsupported => ImportStatus::Unsupported,
            };
            (status, mapping.message.clone())
        },
    )
}

fn sources_for(
    inspection: &Inspection,
    item: &SourceItem,
    skill: Option<&super::SkillDraft>,
    notes: &[&super::MemoryNote],
) -> Vec<ImportSource> {
    let mut sources = vec![ImportSource {
        provider: item.source,
        path: item.path.clone(),
        hash: item.hash.clone(),
        role: ImportSourceRole::Definition,
    }];
    if let Some(skill) = skill {
        for path in files_below(&skill.source_dir) {
            let Ok(relative) = path.strip_prefix(&inspection.snapshot.root) else {
                continue;
            };
            if relative == item.path {
                continue;
            }
            let Ok(bytes) = fs::read(&path) else {
                continue;
            };
            sources.push(ImportSource {
                provider: item.source,
                path: relative.to_path_buf(),
                hash: super::discover::content_hash(&bytes),
                role: ImportSourceRole::Dependency,
            });
        }
    }
    for note in notes {
        if note.source != item.path && !sources.iter().any(|source| source.path == note.source) {
            sources.push(ImportSource {
                provider: note.app,
                path: note.source.clone(),
                hash: note.source_hash.clone(),
                role: ImportSourceRole::Behavior,
            });
        }
    }
    sources.sort_by(|left, right| left.path.cmp(&right.path));
    sources
}

fn target_for(
    item: &SourceItem,
    agent: Option<&Agent>,
    skill: Option<&super::SkillDraft>,
    connections: &[&crate::connections::Connection],
    workflow: Option<&WorkflowFile>,
    notes: &[&super::MemoryNote],
) -> Option<PathBuf> {
    match item.kind {
        ItemKind::Agent => agent.map(agent_target),
        ItemKind::Skill => {
            skill.map(|skill| PathBuf::from("skills").join(&skill.name).join("SKILL.md"))
        }
        ItemKind::Connection if connections.len() == 1 => {
            Some(PathBuf::from("connections").join(format!("{}.json", connections[0].id)))
        }
        ItemKind::Connection if !connections.is_empty() => Some(PathBuf::from("connections")),
        ItemKind::Workflow => workflow.map(|workflow| {
            PathBuf::from("workflows").join(format!("{}.json", slug(&workflow.name)))
        }),
        ItemKind::Memory if notes.len() == 1 => Some(memory_target(&notes[0].title)),
        ItemKind::Memory if !notes.is_empty() => Some(PathBuf::from("memory/notes")),
        ItemKind::Hook
        | ItemKind::Rule
        | ItemKind::Unknown
        | ItemKind::Memory
        | ItemKind::Connection => None,
    }
}

fn dependencies_for(
    item: &SourceItem,
    agent: Option<&Agent>,
    workflow: Option<&WorkflowFile>,
    imported_workflow: Option<&ImportedWorkflow>,
) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(agent) = agent {
        out.extend(agent.skills.iter().map(|name| format!("skill:{name}")));
        out.extend(
            agent
                .connections
                .iter()
                .map(|name| format!("connection:{name}")),
        );
    }
    if let Some(workflow) = workflow {
        out.extend(workflow.steps.iter().filter_map(|step| match step {
            Step::Agent(step) => Some(format!("agent:{}", step.agent)),
            Step::Check(_) | Step::Checkpoint(_) | Step::Serve(_) => None,
        }));
    }
    if item.kind == ItemKind::Workflow
        && let Some(imported_workflow) = imported_workflow
    {
        // Źródłowe zależności obejmują także skille wybrane na kroku. Same natywne AgentStepy
        // niosą wyłącznie identyfikatory agentów, więc bez tego brak bundle skilla wyglądałby
        // jak kompletny workflow aż do Startu.
        out.extend(imported_workflow.dependencies.iter().cloned());
    }
    out.sort();
    out.dedup();
    out
}

fn generated_hash_for(
    agent: Option<&Agent>,
    skill: Option<&super::SkillDraft>,
    connections: &[&crate::connections::Connection],
    workflow: Option<&WorkflowFile>,
    notes: &[&super::MemoryNote],
) -> Option<String> {
    let mut files = Vec::new();
    if let Some(agent) = agent
        && let Some(bytes) = render_agent(agent)
    {
        files.push((agent_target(agent), bytes));
    }
    if let Some(skill) = skill {
        for source in files_below(&skill.source_dir) {
            let Ok(relative) = source.strip_prefix(&skill.source_dir) else {
                continue;
            };
            if let Ok(bytes) = fs::read(&source) {
                files.push((
                    PathBuf::from("skills").join(&skill.name).join(relative),
                    bytes,
                ));
            }
        }
    }
    for connection in connections {
        if let Some(bytes) = pretty_json_bytes(*connection) {
            files.push((
                PathBuf::from("connections").join(format!("{}.json", connection.id)),
                bytes,
            ));
        }
    }
    if let Some(workflow) = workflow
        && let Some(bytes) = pretty_json_bytes(workflow)
    {
        files.push((
            PathBuf::from("workflows").join(format!("{}.json", slug(&workflow.name))),
            bytes,
        ));
    }
    for note in notes {
        if let Ok(bytes) = serde_json::to_vec(note) {
            files.push((memory_target(&note.title), bytes));
        }
    }
    aggregate_hash(files)
}

fn aggregate_hash(mut files: Vec<(PathBuf, Vec<u8>)>) -> Option<String> {
    if files.is_empty() {
        return None;
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    if files.len() == 1 {
        return Some(super::discover::content_hash(&files[0].1));
    }
    let mut bytes = Vec::new();
    for (path, content) in files {
        bytes.extend_from_slice(path.to_string_lossy().as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&content);
        bytes.push(0);
    }
    Some(super::discover::content_hash(&bytes))
}

fn agent_for<'a>(
    item: &SourceItem,
    agents: &'a [Agent],
    sources: &BTreeMap<String, String>,
) -> Option<&'a Agent> {
    if item.kind != ItemKind::Agent {
        return None;
    }
    if let Some(id) = sources.get(&item.id)
        && let Some(agent) = agents.iter().find(|agent| agent.id.to_string() == *id)
    {
        return Some(agent);
    }
    agents
        .iter()
        .find(|agent| agent.name.eq_ignore_ascii_case(&item.name))
        .or_else(|| (agents.len() == 1).then(|| &agents[0]))
}

fn imported_workflow_for<'a>(
    file: &super::discover::InspectedFile,
    workflows: &'a [ImportedWorkflow],
) -> Option<&'a ImportedWorkflow> {
    (file.item.kind == ItemKind::Workflow)
        .then(|| {
            workflows
                .iter()
                .find(|workflow| workflow.item_id == file.item.id)
        })
        .flatten()
}

fn memory_item_covers(item: &SourceItem, source: &Path) -> bool {
    item.kind == ItemKind::Memory
        && (item.path == source
            || item.path.parent().is_some_and(|directory| {
                !directory.as_os_str().is_empty() && source.starts_with(directory)
            }))
}

fn files_below(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut out = Vec::new();
    while let Some(at) = pending.pop() {
        let Ok(read) = fs::read_dir(at) else {
            continue;
        };
        let mut entries: Vec<_> = read.flatten().collect();
        entries.sort_by_key(fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn owns_target(items: &[ImportItem], target: &Path) -> bool {
    items
        .iter()
        .any(|item| item.target.as_deref() == Some(target))
}

fn owns_target_or_child(items: &[ImportItem], parent: &Path) -> bool {
    items.iter().any(|item| {
        item.target
            .as_deref()
            .is_some_and(|target| target == parent || target.starts_with(parent))
    })
}

fn memory_target(title: &str) -> PathBuf {
    PathBuf::from("memory")
        .join("notes")
        .join(format!("{}.md", crate::memory::slugify(title)))
}

fn slug(value: &str) -> String {
    let mut out = String::new();
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

fn agent_target(agent: &Agent) -> PathBuf {
    let name = slug(&agent.name);
    PathBuf::from("agents").join(format!(
        "{}.md",
        if name.is_empty() {
            agent.id.to_string()
        } else {
            name
        }
    ))
}

fn pretty_json_bytes(value: &impl serde::Serialize) -> Option<Vec<u8>> {
    let mut text = serde_json::to_string_pretty(value).ok()?;
    text.push('\n');
    Some(text.into_bytes())
}

/// Ten renderer jest lustrzanym rachunkiem istniejącego `write_agent_file`; AC-1 porównuje jego
/// hash z plikiem po prawdziwym zapisie, więc każda przyszła zmiana formatu przewróci import,
/// zamiast zostawić cicho nieaktualny `generatedHash`.
fn render_agent(agent: &Agent) -> Option<Vec<u8>> {
    const FRONT: [&str; 15] = [
        "schema",
        "id",
        "name",
        "summary",
        "color",
        "runsWith",
        "model",
        "thinking",
        "fileAccess",
        "giveUpAfterMinutes",
        "writeResultsTo",
        "tools",
        "skills",
        "connections",
        "vendorOptions",
    ];
    let Value::Object(mut fields) = serde_json::to_value(agent).ok()? else {
        return None;
    };
    let body = fields
        .remove("instructions")
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default();
    let mut text = String::from("---\n");
    for name in FRONT {
        if let Some(value) = fields.remove(name) {
            text.push_str(&agent_setting(name, &value));
        }
    }
    for (name, value) in fields {
        text.push_str(&agent_setting(&name, &value));
    }
    text.push_str("---\n");
    text.push_str(&body);
    Some(text.into_bytes())
}

fn agent_setting(name: &str, value: &Value) -> String {
    match value {
        Value::String(text) if plain_agent_text(text) => format!("{name}: {text}\n"),
        Value::String(text) => format!("{name}: {}\n", Value::String(text.clone())),
        other => format!("{name}: {other}\n"),
    }
}

fn plain_agent_text(text: &str) -> bool {
    if text.is_empty()
        || text.trim() != text
        || text.contains(['\n', '\r', '\t', '#', '"', '\'', '\\'])
        || text.contains(": ")
        || text.ends_with(':')
        || text.starts_with([
            '-', '?', ':', ',', '[', ']', '{', '}', '&', '*', '!', '|', '>', '%', '@', '`',
        ])
    {
        return false;
    }
    agent_scalar(text) == Value::String(text.to_owned())
}

fn agent_scalar(text: &str) -> Value {
    if text.is_empty() || text == "null" || text == "~" {
        return Value::Null;
    }
    if text == "true" {
        return Value::Bool(true);
    }
    if text == "false" {
        return Value::Bool(false);
    }
    if text.starts_with(['{', '[', '"']) {
        if let Ok(value) = serde_json::from_str::<Value>(text) {
            return value;
        }
        if let Some(items) = agent_flow_list(text) {
            return Value::Array(items);
        }
        return Value::String(text.to_owned());
    }
    if let Ok(number) = text.parse::<u64>() {
        return Value::Number(number.into());
    }
    if let Ok(number) = text.parse::<i64>() {
        return Value::Number(number.into());
    }
    if let Ok(number) = text.parse::<f64>()
        && let Some(number) = serde_json::Number::from_f64(number)
    {
        return Value::Number(number);
    }
    Value::String(text.to_owned())
}

fn agent_flow_list(text: &str) -> Option<Vec<Value>> {
    let inner = text.strip_prefix('[')?.strip_suffix(']')?;
    if inner.trim().is_empty() {
        return Some(Vec::new());
    }
    Some(
        inner
            .split(',')
            .map(|item| agent_scalar(item.trim()))
            .collect(),
    )
}

#[derive(Debug, Clone)]
struct ImportedWorkflow {
    item_id: String,
    source_path: PathBuf,
    workflow: Option<WorkflowFile>,
    dependencies: Vec<String>,
    mapping_override: Option<(Compatibility, String)>,
}

fn imported_workflows(
    inspection: &Inspection,
    agents: &[Agent],
    skills: &[super::SkillDraft],
) -> Vec<ImportedWorkflow> {
    let mut out = Vec::new();
    for file in &inspection.files {
        if file.item.kind != ItemKind::Workflow {
            continue;
        }
        let (workflow, dependencies, mapping_override) = match adapters::workflow_source(file) {
            WorkflowSource::LegacyShipUi { command, proof } => {
                let workflow = ship_ui(agents, file, &command, &proof);
                let dependencies = workflow.as_ref().map_or_else(
                    || agent_dependencies(["frontend-dev", "design-qa", "code-reviewer"]),
                    |_| Vec::new(),
                );
                (workflow, dependencies, None)
            }
            WorkflowSource::Graph(graph) => match graph_workflow(file, agents, skills, &graph) {
                GraphWorkflowImport::Ready {
                    workflow,
                    dependencies,
                } => (Some(workflow), dependencies, None),
                GraphWorkflowImport::Missing { dependencies } => (None, dependencies, None),
                GraphWorkflowImport::NeedsChoice {
                    dependencies,
                    message,
                } => (
                    None,
                    dependencies,
                    Some((Compatibility::NeedsChoice, message)),
                ),
            },
            WorkflowSource::Routine(routine) => {
                (Some(routine_workflow(file, &routine)), Vec::new(), None)
            }
            WorkflowSource::NeedsChoice(choice) => {
                let dependencies =
                    agent_dependencies(choice.agent_roles.iter().map(String::as_str));
                (
                    None,
                    dependencies,
                    Some((Compatibility::NeedsChoice, choice.message)),
                )
            }
        };
        out.push(ImportedWorkflow {
            item_id: file.item.id.clone(),
            source_path: file.item.path.clone(),
            workflow,
            dependencies,
            mapping_override,
        });
    }
    out
}

/// Dwa narzędzia często przechowują ten sam workflow pod tą samą nazwą. Jeden plik docelowy
/// jest wtedy prawdą: identyczne definicje wskazują tę samą wartość, a różne definicje nie mogą
/// czekać z kolizją aż do Apply, kiedy człowiek właśnie zatwierdził cały plan.
fn reconcile_workflow_targets(imported: &mut [ImportedWorkflow]) {
    let mut by_target: BTreeMap<PathBuf, Vec<usize>> = BTreeMap::new();
    for (index, item) in imported.iter().enumerate() {
        let Some(workflow) = &item.workflow else {
            continue;
        };
        by_target
            .entry(workflow_target(workflow))
            .or_default()
            .push(index);
    }

    for (target, indices) in by_target {
        if indices.len() < 2 {
            continue;
        }
        let Some(canonical) = indices
            .first()
            .and_then(|index| imported.get(*index))
            .and_then(|item| item.workflow.clone())
        else {
            continue;
        };
        let same = indices.iter().all(|index| {
            imported
                .get(*index)
                .and_then(|item| item.workflow.as_ref())
                .is_some_and(|workflow| same_workflow_definition(&canonical, workflow))
        });
        if same {
            for index in indices.into_iter().skip(1) {
                if let Some(item) = imported.get_mut(index) {
                    item.workflow = Some(canonical.clone());
                    item.mapping_override = Some((
                        Compatibility::Adjusted,
                        format!(
                            "This is another app's copy of the same workflow. Both copies will use {}.",
                            target.display()
                        ),
                    ));
                }
            }
            continue;
        }

        let sources = indices
            .iter()
            .filter_map(|index| imported.get(*index))
            .map(|item| item.source_path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let message = format!(
            "These project workflows describe different behavior but would both become {}: {sources}. Choose which workflow to keep.",
            target.display()
        );
        for index in indices {
            if let Some(item) = imported.get_mut(index) {
                item.workflow = None;
                item.mapping_override = Some((Compatibility::NeedsChoice, message.clone()));
            }
        }
    }
}

fn same_workflow_definition(left: &WorkflowFile, right: &WorkflowFile) -> bool {
    let mut left = left.clone();
    let mut right = right.clone();
    left.id.clear();
    right.id.clear();
    left == right
}

fn unique_workflow_outputs(imported: &[ImportedWorkflow]) -> Vec<WorkflowFile> {
    let mut seen = BTreeSet::new();
    imported
        .iter()
        .filter_map(|item| item.workflow.clone())
        .filter(|workflow| seen.insert(workflow_target(workflow)))
        .collect()
}

fn workflow_target(workflow: &WorkflowFile) -> PathBuf {
    PathBuf::from("workflows").join(format!("{}.json", slug(&workflow.name)))
}

fn agent_dependencies<'a>(roles: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut dependencies: Vec<_> = roles
        .into_iter()
        .map(|role| format!("agent:{role}"))
        .collect();
    dependencies.sort();
    dependencies.dedup();
    dependencies
}

fn graph_dependencies(source: &adapters::GraphWorkflow) -> Vec<String> {
    let mut dependencies = agent_dependencies(source.steps.iter().map(|step| step.role.as_str()));
    dependencies.extend(source.steps.iter().flat_map(|step| {
        step.skills
            .iter()
            .flatten()
            .map(|skill| format!("skill:{skill}"))
    }));
    dependencies.sort();
    dependencies.dedup();
    dependencies
}

enum GraphWorkflowImport {
    Ready {
        workflow: WorkflowFile,
        dependencies: Vec<String>,
    },
    Missing {
        dependencies: Vec<String>,
    },
    NeedsChoice {
        dependencies: Vec<String>,
        message: String,
    },
}

enum DeclaredAgent<'a> {
    Found(&'a Agent),
    Missing,
    Ambiguous(Vec<&'a Agent>),
}

enum ImportedSkill<'a> {
    Found(&'a super::SkillDraft),
    Missing,
    Ambiguous(Vec<&'a super::SkillDraft>),
}

struct ResolvedGraph<'a> {
    agents: Vec<&'a Agent>,
    skills: Vec<Vec<String>>,
    dependencies: Vec<String>,
}

enum GraphInputs<'a> {
    Ready(ResolvedGraph<'a>),
    Missing(Vec<String>),
    NeedsChoice(String),
}

struct ResolvedStepSkills {
    names: Vec<String>,
    dependencies: Vec<String>,
    missing: bool,
}

fn graph_workflow(
    file: &super::discover::InspectedFile,
    agents: &[Agent],
    skills: &[super::SkillDraft],
    source: &adapters::GraphWorkflow,
) -> GraphWorkflowImport {
    let fallback_dependencies = graph_dependencies(source);
    let resolved = match resolve_graph_inputs(agents, skills, source) {
        GraphInputs::Ready(resolved) => resolved,
        GraphInputs::Missing(dependencies) => {
            return GraphWorkflowImport::Missing { dependencies };
        }
        GraphInputs::NeedsChoice(message) => {
            return GraphWorkflowImport::NeedsChoice {
                dependencies: fallback_dependencies,
                message,
            };
        }
    };
    let ids = graph_step_ids(source);
    let links = match graph_links(source, &ids) {
        Ok(links) => links,
        Err(message) => {
            return GraphWorkflowImport::NeedsChoice {
                dependencies: fallback_dependencies,
                message,
            };
        }
    };
    let workflow = WorkflowFile {
        format: 1,
        id: imported_workflow_id(file, &source.name),
        name: source.name.clone(),
        description: Some("Imported from a declared project workflow.".to_owned()),
        steps: graph_steps(source, &ids, &resolved),
        links,
        extra: imported_workflow_extra(),
    };
    let problems: Vec<_> = check_to_run(&workflow)
        .into_iter()
        .filter(|note| note.level == Level::Problem)
        .map(|note| note.message)
        .collect();
    if !problems.is_empty() {
        return GraphWorkflowImport::NeedsChoice {
            dependencies: resolved.dependencies,
            message: format!(
                "The {} workflow cannot run as imported. {}",
                source.name,
                problems.join(" ")
            ),
        };
    }
    GraphWorkflowImport::Ready {
        workflow,
        dependencies: resolved.dependencies,
    }
}

fn resolve_graph_inputs<'a>(
    agents: &'a [Agent],
    skills: &[super::SkillDraft],
    source: &adapters::GraphWorkflow,
) -> GraphInputs<'a> {
    let mut dependencies = Vec::new();
    let mut native_agents = Vec::with_capacity(source.steps.len());
    let mut selected_skills = Vec::with_capacity(source.steps.len());
    let mut missing = false;

    for step in &source.steps {
        let agent = match resolve_declared_agent(agents, &step.role) {
            DeclaredAgent::Found(agent) => {
                dependencies.push(format!("agent:{}", agent.id));
                Some(agent)
            }
            DeclaredAgent::Missing => {
                dependencies.push(format!("agent:{}", step.role));
                missing = true;
                None
            }
            DeclaredAgent::Ambiguous(candidates) => {
                let names = joined_names(candidates.iter().map(|agent| agent.name.as_str()));
                return GraphInputs::NeedsChoice(format!(
                    "The {} workflow uses role `{}`, but it matches more than one imported agent: {names}. Choose one agent for {}.",
                    source.name, step.role, step.name
                ));
            }
        };
        let selected = match resolve_graph_step_skills(skills, agent, step, &source.name) {
            Ok(selected) => selected,
            Err(message) => return GraphInputs::NeedsChoice(message),
        };
        dependencies.extend(selected.dependencies);
        missing |= selected.missing;
        native_agents.push(agent);
        selected_skills.push(selected.names);
    }
    dependencies.sort();
    dependencies.dedup();
    if missing {
        return GraphInputs::Missing(dependencies);
    }
    GraphInputs::Ready(ResolvedGraph {
        agents: native_agents.into_iter().flatten().collect(),
        skills: selected_skills,
        dependencies,
    })
}

fn resolve_graph_step_skills(
    skills: &[super::SkillDraft],
    agent: Option<&Agent>,
    step: &adapters::GraphAgentStep,
    workflow_name: &str,
) -> std::result::Result<ResolvedStepSkills, String> {
    let mut resolved = ResolvedStepSkills {
        names: Vec::new(),
        dependencies: Vec::new(),
        missing: false,
    };
    for selected in step.skills.iter().flatten() {
        match resolve_imported_skill(skills, selected) {
            ImportedSkill::Found(skill) => {
                if let Err(issue) = validate_imported_skill(skill) {
                    return Err(format!(
                        "The {workflow_name} workflow selects skill `{selected}`, but that imported skill cannot run: {issue}"
                    ));
                }
                if agent.is_some_and(|agent| {
                    !agent.skills.iter().any(|assigned| assigned == &skill.name)
                }) {
                    let agent_name =
                        agent.map_or("the selected agent", |agent| agent.name.as_str());
                    return Err(format!(
                        "The {workflow_name} workflow assigns skill `{selected}` to {}, but the imported skill is named `{}` and is not assigned to agent {agent_name}. Choose the agent's skills or change the step.",
                        step.name, skill.name
                    ));
                }
                resolved.dependencies.push(format!("skill:{}", skill.name));
                resolved.names.push(skill.name.clone());
            }
            ImportedSkill::Missing => {
                resolved.dependencies.push(format!("skill:{selected}"));
                resolved.names.push(selected.clone());
                resolved.missing = true;
            }
            ImportedSkill::Ambiguous(candidates) => {
                let names = joined_names(candidates.iter().map(|skill| skill.name.as_str()));
                return Err(format!(
                    "The {workflow_name} workflow selects skill `{selected}`, but it matches more than one imported skill: {names}. Choose one skill for {}.",
                    step.name
                ));
            }
        }
    }
    Ok(resolved)
}

fn graph_step_ids(source: &adapters::GraphWorkflow) -> BTreeMap<String, String> {
    let namespace = slug(&source.name);
    source
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| {
            (
                step.binding.clone(),
                format!("{}.{}-{}", namespace, slug(&step.binding), index + 1),
            )
        })
        .collect()
}

fn graph_steps(
    source: &adapters::GraphWorkflow,
    ids: &BTreeMap<String, String>,
    resolved: &ResolvedGraph<'_>,
) -> Vec<Step> {
    let namespace = slug(&source.name);
    let mut depths: BTreeMap<String, u32> = BTreeMap::new();
    let mut lanes: BTreeMap<u32, u32> = BTreeMap::new();
    let mut steps = Vec::with_capacity(source.steps.len());
    for (index, ((declared, agent), selected)) in source
        .steps
        .iter()
        .zip(&resolved.agents)
        .zip(&resolved.skills)
        .enumerate()
    {
        let depth = declared
            .after
            .iter()
            .filter_map(|binding| depths.get(binding))
            .copied()
            .max()
            .map_or(0_u32, |depth| depth.saturating_add(1));
        depths.insert(declared.binding.clone(), depth);
        let lane = lanes.entry(depth).or_default();
        let at = point(f64::from(depth) * 288.0, f64::from(*lane) * 144.0);
        *lane = lane.saturating_add(1);
        steps.push(Step::Agent(AgentStep {
            id: ids
                .get(&declared.binding)
                .cloned()
                .unwrap_or_else(|| format!("{namespace}.step-{}", index + 1)),
            name: declared.name.clone(),
            agent: agent.id.to_string(),
            overrides: Map::new(),
            vendor_options: BTreeMap::new(),
            copies: 1,
            instructions: declared.task.clone(),
            skills: if selected.is_empty() {
                Skills::default()
            } else {
                Skills::Only(selected.clone())
            },
            borrow: crate::workflow::Borrow::default(),
            folder: declared_folder(&declared.folder),
            handover: Handover::Plain(PlainNotes::Notes),
            when_it_fails: WhenItFails::default(),
            at,
            extra: Map::new(),
        }));
    }
    steps
}

fn graph_links(
    source: &adapters::GraphWorkflow,
    ids: &BTreeMap<String, String>,
) -> std::result::Result<Vec<Link>, String> {
    let mut links = Vec::new();
    for step in &source.steps {
        for predecessor in &step.after {
            let Some(from) = ids.get(predecessor) else {
                return Err(format!(
                    "The {} workflow says {} runs after `{predecessor}`, but that earlier step was not declared. Choose the intended order.",
                    source.name, step.name
                ));
            };
            let Some(to) = ids.get(&step.binding) else {
                return Err(format!(
                    "The {} workflow contains a step without a stable name. Choose how to represent {}.",
                    source.name, step.name
                ));
            };
            links.push(Link {
                from: from.clone(),
                to: to.clone(),
                max_turns: None,
            });
        }
    }
    Ok(links)
}

fn joined_names<'a>(names: impl IntoIterator<Item = &'a str>) -> String {
    names.into_iter().collect::<Vec<_>>().join(", ")
}

fn routine_workflow(
    file: &super::discover::InspectedFile,
    source: &adapters::RoutineWorkflow,
) -> WorkflowFile {
    let namespace = slug(&source.name);
    let mut steps = Vec::new();
    let mut links = Vec::new();
    if let Some((command, proof)) = &source.check {
        steps.push(Step::Check(CheckStep {
            id: format!("{namespace}.check"),
            name: "Run the checks".to_owned(),
            command: command.clone(),
            proof: proof.clone(),
            folder: Folder::Project,
            when_it_fails: WhenItFails::default(),
            at: point(0.0, 0.0),
            // Pochodzenie jest typowanym `ImportItem.sources` z T-78. Drugi, luźny klucz na
            // kroku mógłby wskazać inny plik niż ten, który importer naprawdę zapisał.
            extra: Map::new(),
        }));
    }
    if let Some(question) = &source.question {
        let checkpoint_id = format!("{namespace}.checkpoint");
        if source.check.is_some() {
            links.push(link(&format!("{namespace}.check"), &checkpoint_id));
        }
        steps.push(Step::Checkpoint(CheckpointStep {
            id: checkpoint_id,
            name: "Ask for approval".to_owned(),
            question: Some(question.clone()),
            at: point(if source.check.is_some() { 288.0 } else { 0.0 }, 0.0),
            extra: Map::new(),
        }));
    }
    WorkflowFile {
        format: 1,
        id: imported_workflow_id(file, &source.name),
        name: source.name.clone(),
        description: source.description.clone(),
        steps,
        links,
        extra: imported_workflow_extra(),
    }
}

fn declared_folder(folder: &str) -> Folder {
    match folder {
        "fresh-copy" => Folder::FreshCopy,
        "same-copy" => Folder::SameCopy,
        _ => Folder::Project,
    }
}

fn resolve_declared_agent<'a>(agents: &'a [Agent], role: &str) -> DeclaredAgent<'a> {
    let wanted = slug(role);
    let mut matches = agents.iter().filter(|agent| slug(&agent.name) == wanted);
    let Some(found) = matches.next() else {
        return DeclaredAgent::Missing;
    };
    let Some(second) = matches.next() else {
        return DeclaredAgent::Found(found);
    };
    let mut candidates = vec![found, second];
    candidates.extend(matches);
    DeclaredAgent::Ambiguous(candidates)
}

fn resolve_imported_skill<'a>(
    skills: &'a [super::SkillDraft],
    selected: &str,
) -> ImportedSkill<'a> {
    let mut matches = skills
        .iter()
        .filter(|skill| skill.name.eq_ignore_ascii_case(selected));
    let Some(found) = matches.next() else {
        return ImportedSkill::Missing;
    };
    let Some(second) = matches.next() else {
        return ImportedSkill::Found(found);
    };
    let mut candidates = vec![found, second];
    candidates.extend(matches);
    ImportedSkill::Ambiguous(candidates)
}

fn validate_imported_skill(skill: &super::SkillDraft) -> std::result::Result<(), String> {
    let source = skill.source_dir.join("SKILL.md");
    let text = fs::read_to_string(&source).map_err(|_| {
        format!(
            "Loadout could not read the definition for skill `{}`.",
            skill.name
        )
    })?;
    let document = crate::skills::place::read_doc(&text);
    crate::skills::place::validate_usable(&skill.name, &document).map_err(|issues| issues.join(" "))
}

fn imported_workflow_id(file: &super::discover::InspectedFile, name: &str) -> String {
    let identity = format!("{}\0{name}", file.item.id);
    format!(
        "imported-{}",
        super::discover::content_hash(identity.as_bytes())
    )
}

fn imported_workflow_extra() -> Map<String, Value> {
    let mut extra = Map::new();
    extra.insert(
        "importedBy".to_owned(),
        Value::String(format!("loadout-import-v{ADAPTER_VERSION}")),
    );
    extra
}

fn ship_ui(
    agents: &[Agent],
    file: &super::discover::InspectedFile,
    command: &str,
    proof: &str,
) -> Option<WorkflowFile> {
    let frontend = find_agent(agents, "frontend-dev")?;
    let design = find_agent(agents, "design-qa")?;
    let review = find_agent(agents, "code-reviewer")?;
    let review_template = review_subworkflow(design, review);
    let expanded_review = flatten(&review_template, "ship-ui");
    let expanded_review_id = expanded_review.id.clone();
    let mut steps = vec![
        Step::Agent({
            let mut plan = agent_step(
                "ship-ui.plan",
                "Plan the change",
                frontend,
                "Turn the person's request into an implementation plan and hand it to the approval step.",
                point(0.0, 0.0),
            );
            plan.folder = Folder::Project;
            plan
        }),
        Step::Checkpoint(CheckpointStep {
            id: "ship-ui.approve-plan".to_owned(),
            name: "Approve the plan".to_owned(),
            question: Some("Does the implementation plan match the task?".to_owned()),
            at: point(288.0, 0.0),
            extra: Map::new(),
        }),
        Step::Agent(agent_step(
            "ship-ui.implement",
            "Build the UI",
            frontend,
            "Implement the approved UI plan. Use the imported project skills and write a handoff for the checks.",
            point(576.0, 0.0),
        )),
        Step::Check(CheckStep {
            id: "ship-ui.check".to_owned(),
            name: "Run the project checks".to_owned(),
            command: command.to_owned(),
            proof: proof.to_owned(),
            folder: Folder::SameCopy,
            // Import nie zgaduje polityki: przeniesiona konfiguracja zachowuje się tak,
            // jak zachowywał się każdy krok do 2026-08-23.
            when_it_fails: WhenItFails::Stop,
            at: point(864.0, 0.0),
            extra: Map::new(),
        }),
    ];
    steps.extend(expanded_review.steps);
    steps.push(Step::Check(CheckStep {
        id: "ship-ui.final-check".to_owned(),
        name: "Run the final checks".to_owned(),
        command: command.to_owned(),
        proof: proof.to_owned(),
        folder: Folder::SameCopy,
        when_it_fails: WhenItFails::Stop,
        at: point(1440.0, 0.0),
        extra: Map::new(),
    }));
    let links = vec![
        link("ship-ui.plan", "ship-ui.approve-plan"),
        // Implementacja czeka także bezpośrednio na handoff planisty; sam Checkpoint niesie
        // decyzję człowieka, ale nie kopiuje cudzej odpowiedzi do własnego przekazania.
        link("ship-ui.plan", "ship-ui.implement"),
        link("ship-ui.approve-plan", "ship-ui.implement"),
        link("ship-ui.implement", "ship-ui.check"),
        link("ship-ui.check", "ship-ui.design-qa"),
        link("ship-ui.check", "ship-ui.code-review"),
        link("ship-ui.design-qa", "ship-ui.final-check"),
        link("ship-ui.code-review", "ship-ui.final-check"),
        Link {
            from: "ship-ui.code-review".to_owned(),
            to: "ship-ui.implement".to_owned(),
            max_turns: Some(1),
        },
    ];
    let mut extra = Map::new();
    extra.insert(
        "importedBy".to_owned(),
        Value::String(format!("loadout-import-v{ADAPTER_VERSION}")),
    );
    extra.insert(
        "expandedSubworkflows".to_owned(),
        Value::Array(vec![Value::String(expanded_review_id)]),
    );
    Some(WorkflowFile {
        format: 1,
        id: imported_workflow_id(file, "Ship UI"),
        name: "Ship UI".to_owned(),
        description: Some("Imported from the project's coordinating skill.".to_owned()),
        steps,
        links,
        extra,
    })
}

fn review_subworkflow(design: &Agent, review: &Agent) -> WorkflowFile {
    WorkflowFile {
        format: 1,
        id: "parallel-review".to_owned(),
        name: "Parallel review".to_owned(),
        description: None,
        steps: vec![
            Step::Agent(agent_step(
                "design-qa",
                "Review the UI",
                design,
                "Inspect the implementation and the check handoff. Report concrete visual concerns only.",
                point(1152.0, -144.0),
            )),
            Step::Agent(agent_step(
                "code-review",
                "Review the code",
                review,
                "Review the implementation and prior handoffs. End with outcome: pass or outcome: fail.",
                point(1152.0, 144.0),
            )),
        ],
        links: Vec::new(),
        extra: Map::new(),
    }
}

fn find_agent<'a>(agents: &'a [Agent], wanted: &str) -> Option<&'a Agent> {
    agents.iter().find(|agent| {
        agent.name.eq_ignore_ascii_case(wanted)
            || agent
                .name
                .to_ascii_lowercase()
                .replace(' ', "-")
                .contains(wanted)
    })
}

fn agent_step(id: &str, name: &str, agent: &Agent, instructions: &str, at: Point) -> AgentStep {
    AgentStep {
        id: id.to_owned(),
        name: name.to_owned(),
        agent: agent.id.to_string(),
        overrides: Map::new(),
        vendor_options: BTreeMap::new(),
        copies: 1,
        instructions: instructions.to_owned(),
        skills: Skills::default(),
        borrow: crate::workflow::Borrow::default(),
        folder: Folder::SameCopy,
        handover: Handover::Plain(PlainNotes::Notes),
        when_it_fails: WhenItFails::Stop,
        at,
        extra: Map::new(),
    }
}

fn point(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn link(from: &str, to: &str) -> Link {
    Link {
        from: from.to_owned(),
        to: to.to_owned(),
        max_turns: None,
    }
}

/// Rozwija szablon bez zmiany rodzajów kroków. Zagnieżdżenie jest faktem importera, nie silnika.
#[must_use]
pub fn flatten(template: &WorkflowFile, namespace: &str) -> WorkflowFile {
    let ids: BTreeSet<String> = template
        .steps
        .iter()
        .map(|step| step_id(step).to_owned())
        .collect();
    let rename = |id: &str| {
        if ids.contains(id) {
            format!("{namespace}.{id}")
        } else {
            id.to_owned()
        }
    };
    let steps = template
        .steps
        .iter()
        .cloned()
        .map(|mut step| {
            match &mut step {
                Step::Agent(step) => step.id = rename(&step.id),
                Step::Checkpoint(step) => step.id = rename(&step.id),
                Step::Check(step) => step.id = rename(&step.id),
                Step::Serve(step) => step.id = rename(&step.id),
            }
            step
        })
        .collect();
    let links = template
        .links
        .iter()
        .map(|link| Link {
            from: rename(&link.from),
            to: rename(&link.to),
            max_turns: link.max_turns,
        })
        .collect();
    let mut extra = template.extra.clone();
    extra.insert(
        "expandedFrom".to_owned(),
        Value::String(template.id.clone()),
    );
    WorkflowFile {
        format: template.format,
        id: format!("{namespace}.{}", template.id),
        name: template.name.clone(),
        description: template.description.clone(),
        steps,
        links,
        extra,
    }
}

fn step_id(step: &Step) -> &str {
    match step {
        Step::Agent(step) => &step.id,
        Step::Checkpoint(step) => &step.id,
        Step::Check(step) => &step.id,
        Step::Serve(step) => &step.id,
    }
}
