//! Odczyt repo bez zapisu, sieci i uruchamiania znalezionego kodu.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::{DiscoverySnapshot, ImportError, ItemKind, Result, SourceItem, SourceKind};

const FILE_CAP: u64 = 1_048_576;
const TOTAL_CAP: u64 = 8_388_608;
const COUNT_CAP: usize = 512;

#[derive(Debug, Clone)]
pub(crate) struct InspectedFile {
    pub item: SourceItem,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct Inspection {
    pub snapshot: DiscoverySnapshot,
    pub(crate) files: Vec<InspectedFile>,
}

/// Skanuje wyłącznie katalogi konfiguracji. Kod projektu nie jest wejściem importera.
pub fn scan(root: &Path) -> Result<Inspection> {
    let root = canonical_root(root)?;
    let candidates = configuration_files(&root)?;
    let mut files = inspect_files(&root, &candidates)?;

    // Jeden plik skilla może równocześnie opisywać proceduralny workflow. Druga pozycja jest
    // jawna, bo inaczej raport powiedziałby tylko „skill imported" i zgubił ceremonię.
    let mut workflow_items = Vec::new();
    for file in &files {
        if file.item.kind == ItemKind::Skill && looks_like_workflow(&file.content) {
            let mut item = file.item.clone();
            item.id = format!("{}:workflow", item.id);
            item.kind = ItemKind::Workflow;
            "This skill also coordinates multiple roles.".clone_into(&mut item.summary);
            workflow_items.push(InspectedFile {
                item,
                content: file.content.clone(),
            });
        }
    }
    files.extend(workflow_items);
    files.sort_by(|left, right| {
        (&left.item.path, left.item.kind as u8, &left.item.id).cmp(&(
            &right.item.path,
            right.item.kind as u8,
            &right.item.id,
        ))
    });

    let items = files.iter().map(|file| file.item.clone()).collect();
    Ok(Inspection {
        snapshot: DiscoverySnapshot {
            root: root.clone(),
            items,
        },
        files,
    })
}

fn canonical_root(root: &Path) -> Result<PathBuf> {
    let selected = root.canonicalize().map_err(|error| ImportError::Inspect {
        path: root.to_path_buf(),
        detail: error.to_string(),
    })?;
    // Człowiek często wskazuje widoczny katalog `.claude`, nie jego rodzica. Import nadal
    // dotyczy repo: inaczej szukalibyśmy `.claude/.claude` i uczciwy projekt wyglądałby jak
    // pusta konfiguracja. `.codex` i `.agents` mają dokładnie tę samą pułapkę.
    let root = if selected
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| [".claude", ".codex", ".agents", ".rulesync"].contains(&name))
    {
        selected.parent().unwrap_or(&selected).to_path_buf()
    } else {
        selected
    };
    if !root.is_dir() {
        return Err(ImportError::Inspect {
            path: root,
            detail: "That workspace is not a folder.".to_owned(),
        });
    }

    Ok(root)
}

fn configuration_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut candidates = Vec::new();
    for relative in [".claude", ".codex", ".rulesync"] {
        let path = root.join(relative);
        if path.exists() {
            walk(root, &path, &mut candidates)?;
        }
    }
    let agents = root.join(".agents");
    if agents.exists() {
        walk_agents(root, &agents, &mut candidates)?;
    }
    let mcp = root.join(".mcp.json");
    if mcp.exists() {
        candidates.push(mcp);
    }
    for relative in ["AGENTS.md", "CLAUDE.md", "CLAUDE.local.md"] {
        let path = root.join(relative);
        if path.exists() {
            candidates.push(path);
        }
    }
    candidates.sort();
    candidates.dedup();
    if candidates.len() > COUNT_CAP {
        return Err(ImportError::Inspect {
            path: root.to_path_buf(),
            detail: format!("This setup has more than {COUNT_CAP} configuration files."),
        });
    }
    let total = candidates.iter().try_fold(0_u64, |total, path| {
        let metadata = fs::symlink_metadata(path).map_err(|error| ImportError::Inspect {
            path: path.strip_prefix(root).unwrap_or(path).to_path_buf(),
            detail: error.to_string(),
        })?;
        Ok::<u64, ImportError>(total.saturating_add(metadata.len()))
    })?;
    if total > TOTAL_CAP {
        return Err(ImportError::Inspect {
            path: root.to_path_buf(),
            detail: format!("This setup is larger than {TOTAL_CAP} bytes."),
        });
    }
    Ok(candidates)
}

fn walk_agents(root: &Path, agents: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(agents)
        .map_err(|error| ImportError::Inspect {
            path: agents.to_path_buf(),
            detail: error.to_string(),
        })?
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|error| ImportError::Inspect {
            path: agents.to_path_buf(),
            detail: error.to_string(),
        })?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        if ["agents", "skills", "rules", "commands", "checks", "hooks"]
            .iter()
            .any(|known| name == *known)
            || path.is_file()
        {
            walk(root, &path, out)?;
        } else if let Some(representative) = representative_file(root, &path)? {
            // Niestandardowy harness jest jednym źródłem semantycznym. Lista każdego skryptu,
            // schematu i promptu osobno w Murmur dawała 30 identycznych odmów i ukrywała fakt,
            // że wszystkie razem opisują jeden system, który ma przeanalizować agent.
            out.push(representative);
        }
    }
    Ok(())
}

fn representative_file(root: &Path, directory: &Path) -> Result<Option<PathBuf>> {
    let mut files = Vec::new();
    walk(root, directory, &mut files)?;
    files.sort_by_key(|path| {
        let preferred = path.file_name().is_none_or(|name| {
            ![
                "config.json",
                "config.jsonc",
                "config.toml",
                "manifest.json",
            ]
            .iter()
            .any(|candidate| name == *candidate)
        });
        (preferred, path.clone())
    });
    Ok(files
        .into_iter()
        .find(|path| !is_documentation(path.strip_prefix(root).unwrap_or(path))))
}

fn inspect_files(root: &Path, candidates: &[PathBuf]) -> Result<Vec<InspectedFile>> {
    let mut files = Vec::new();
    for path in candidates {
        if let Some(file) = inspect_file(root, path, candidates)? {
            files.push(file);
        }
    }
    Ok(files)
}

fn inspect_file(root: &Path, path: &Path, candidates: &[PathBuf]) -> Result<Option<InspectedFile>> {
    let relative = path.strip_prefix(root).map_err(|_| ImportError::Inspect {
        path: path.to_path_buf(),
        detail: "A configuration path leaves the workspace.".to_owned(),
    })?;
    let metadata = fs::symlink_metadata(path).map_err(|error| ImportError::Inspect {
        path: relative.to_path_buf(),
        detail: error.to_string(),
    })?;
    if metadata.file_type().is_symlink() {
        return Ok(Some(InspectedFile {
            item: SourceItem {
                id: identity(relative, "symlink"),
                source: source_of(relative),
                kind: ItemKind::Unknown,
                path: relative.to_path_buf(),
                hash: "symlink".to_owned(),
                name: display_name(relative, ItemKind::Unknown),
                summary: "This configuration is a link and was not followed.".to_owned(),
            },
            content: String::new(),
        }));
    }
    if !metadata.is_file() || is_skill_bundle_child(relative) || is_documentation(relative) {
        return Ok(None);
    }
    if metadata.len() > FILE_CAP {
        return Err(ImportError::Inspect {
            path: relative.to_path_buf(),
            detail: format!("This configuration file is larger than {FILE_CAP} bytes."),
        });
    }
    let bytes = fs::read(path).map_err(|error| ImportError::Inspect {
        path: relative.to_path_buf(),
        detail: error.to_string(),
    })?;
    let content = String::from_utf8(bytes.clone()).map_err(|_| ImportError::Inspect {
        path: relative.to_path_buf(),
        detail: "Configuration files must be UTF-8 text.".to_owned(),
    })?;
    let hash = if is_bundle_entry(relative) {
        bundle_hash(root, relative, candidates)?
    } else {
        content_hash(&bytes)
    };
    let (kind, summary) = classify(relative, &content);
    Ok(Some(InspectedFile {
        item: SourceItem {
            id: identity(relative, &hash),
            source: source_of(relative),
            kind,
            path: relative.to_path_buf(),
            hash,
            name: display_name(relative, kind),
            summary,
        },
        content,
    }))
}

fn is_documentation(path: &Path) -> bool {
    path.file_name().is_some_and(|name| {
        name.eq_ignore_ascii_case("README.md") || name.eq_ignore_ascii_case("README")
    })
}

fn is_skill_bundle_child(path: &Path) -> bool {
    let components: Vec<_> = path.components().collect();
    let skill_child = components
        .iter()
        .position(|part| part.as_os_str() == "skills")
        .is_some_and(|index| {
            components.len() > index + 2 && path.file_name().is_some_and(|name| name != "SKILL.md")
        });
    let memory_child = components
        .first()
        .is_some_and(|part| part.as_os_str() == ".claude")
        && components
            .get(1)
            .is_some_and(|part| part.as_os_str() == "agent-memory")
        && components.len() >= 4
        && path.file_name().is_some_and(|name| name != "MEMORY.md");
    skill_child || memory_child
}

fn is_bundle_entry(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "SKILL.md")
        || path.file_name().is_some_and(|name| name == "MEMORY.md")
            && path.starts_with(".claude/agent-memory")
}

fn bundle_hash(root: &Path, skill_file: &Path, candidates: &[PathBuf]) -> Result<String> {
    let Some(skill_dir) = skill_file.parent() else {
        return Ok(content_hash(&[]));
    };
    let mut bytes = Vec::new();
    for path in candidates {
        let relative = path.strip_prefix(root).map_err(|_| ImportError::Inspect {
            path: path.clone(),
            detail: "A skill path leaves the workspace.".to_owned(),
        })?;
        if !relative.starts_with(skill_dir) {
            continue;
        }
        let metadata = fs::symlink_metadata(path).map_err(|error| ImportError::Inspect {
            path: relative.to_path_buf(),
            detail: error.to_string(),
        })?;
        if metadata.file_type().is_symlink() {
            bytes.extend_from_slice(b"symlink\0");
        } else if metadata.is_file() {
            bytes.extend_from_slice(relative.to_string_lossy().as_bytes());
            bytes.push(0);
            bytes.extend(fs::read(path).map_err(|error| ImportError::Inspect {
                path: relative.to_path_buf(),
                detail: error.to_string(),
            })?);
            bytes.push(0);
        }
    }
    Ok(content_hash(&bytes))
}

fn walk(root: &Path, at: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    // `traces` jest wyjściem wykonanych workflow (zrzuty, filmy, logi), nie konfiguracją,
    // która steruje następnym biegiem. Czytanie go jako setupu w realnym URC mieszało setki
    // binarnych artefaktów z trzynastoma agentami i kończyło skan na pierwszym PNG.
    if at.strip_prefix(root).is_ok_and(|relative| {
        [
            ".claude/traces",
            ".claude/tmp",
            ".claude/plans",
            ".claude/d2c",
            ".claude/worktrees",
            ".codex/traces",
        ]
        .iter()
        .any(|generated| relative.starts_with(generated))
    }) {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(at).map_err(|error| ImportError::Inspect {
        path: at.to_path_buf(),
        detail: error.to_string(),
    })?;
    if metadata.file_type().is_symlink() || metadata.is_file() {
        out.push(at.to_path_buf());
        return Ok(());
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    let mut entries: Vec<_> = fs::read_dir(at)
        .map_err(|error| ImportError::Inspect {
            path: at.to_path_buf(),
            detail: error.to_string(),
        })?
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|error| ImportError::Inspect {
            path: at.to_path_buf(),
            detail: error.to_string(),
        })?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if !path.starts_with(root) {
            return Err(ImportError::Inspect {
                path,
                detail: "A configuration path leaves the workspace.".to_owned(),
            });
        }
        walk(root, &path, out)?;
        if out.len() > COUNT_CAP {
            return Err(ImportError::Inspect {
                path: at.to_path_buf(),
                detail: format!("This setup has more than {COUNT_CAP} configuration files."),
            });
        }
    }
    Ok(())
}

fn source_of(path: &Path) -> SourceKind {
    match path
        .components()
        .next()
        .and_then(|part| part.as_os_str().to_str())
    {
        Some(".claude") => SourceKind::Claude,
        Some(".codex") => SourceKind::Codex,
        Some(".agents") => SourceKind::AgentSkills,
        Some(".rulesync") => SourceKind::Rulesync,
        _ if path == Path::new(".mcp.json") => SourceKind::Claude,
        _ if [
            Path::new("AGENTS.md"),
            Path::new("CLAUDE.md"),
            Path::new("CLAUDE.local.md"),
        ]
        .contains(&path) =>
        {
            SourceKind::OpenStandard
        }
        _ => SourceKind::Unknown,
    }
}

fn classify(path: &Path, content: &str) -> (ItemKind, String) {
    let text = path.to_string_lossy();
    if text.starts_with(".claude/agents/") && path.extension().is_some_and(|ext| ext == "md")
        || text.starts_with(".codex/agents/") && path.extension().is_some_and(|ext| ext == "toml")
        || text.starts_with(".rulesync/subagents/")
            && path.extension().is_some_and(|ext| ext == "md")
        || text.starts_with(".agents/agents/") && path.extension().is_some_and(|ext| ext == "md")
    {
        return (ItemKind::Agent, "An agent definition was found.".to_owned());
    }
    if text.starts_with(".claude/workflows/") && path.extension().is_some_and(|ext| ext == "js")
        || text.starts_with(".rulesync/commands/")
            && path.extension().is_some_and(|ext| ext == "md")
        || text.starts_with(".agents/commands/") && path.extension().is_some_and(|ext| ext == "md")
    {
        return (
            ItemKind::Workflow,
            "A project workflow definition was found.".to_owned(),
        );
    }
    if text.starts_with(".agents/")
        && ![
            ".agents/agents/",
            ".agents/skills/",
            ".agents/rules/",
            ".agents/commands/",
            ".agents/checks/",
            ".agents/hooks/",
        ]
        .iter()
        .any(|known| text.starts_with(known))
    {
        return (
            ItemKind::Workflow,
            "A custom project automation bundle was found for agent analysis.".to_owned(),
        );
    }
    if text.starts_with(".claude/commands/") && path.extension().is_some_and(|ext| ext == "md")
        || text.starts_with(".claude/lib/")
        || text.starts_with(".codex/lib/")
    {
        return (
            ItemKind::Workflow,
            "A procedural project routine was found.".to_owned(),
        );
    }
    if path.file_name().is_some_and(|name| name == "SKILL.md")
        && (text.contains("/skills/") || text.starts_with(".agents/skills/"))
    {
        return (
            ItemKind::Skill,
            "A complete skill bundle was found.".to_owned(),
        );
    }
    classify_project_support(path, content, &text).unwrap_or_else(|| {
        (
            ItemKind::Unknown,
            "Loadout does not recognize this project setting yet.".to_owned(),
        )
    })
}

fn classify_project_support(path: &Path, content: &str, text: &str) -> Option<(ItemKind, String)> {
    if path == Path::new(".mcp.json")
        || path == Path::new(".rulesync/mcp.json")
        || path == Path::new(".rulesync/mcp.jsonc")
        || path == Path::new(".agents/mcp.json")
        || path == Path::new(".agents/mcp.jsonc")
        || path == Path::new(".codex/config.toml") && content.contains("mcp_servers")
    {
        return Some((
            ItemKind::Connection,
            "Project tool connections were found and will stay off until approved.".to_owned(),
        ));
    }
    if text.starts_with(".claude/settings") && content.contains("hooks")
        || path == Path::new(".codex/hooks.json")
        || path == Path::new(".rulesync/hooks.json")
        || path == Path::new(".rulesync/hooks.jsonc")
    {
        return Some((ItemKind::Hook, "A project hook was found.".to_owned()));
    }
    if text.starts_with(".claude/hooks/")
        || text.starts_with(".codex/hooks/")
        || text.starts_with(".agents/hooks/")
    {
        return Some((
            ItemKind::Hook,
            "A project hook script was found.".to_owned(),
        ));
    }
    if text.starts_with(".claude/agent-memory/")
        && path.file_name().is_some_and(|name| name == "MEMORY.md")
        || text.starts_with(".claude/learnings/") && path.extension().is_some_and(|ext| ext == "md")
        || text.starts_with(".codex/learnings/") && path.extension().is_some_and(|ext| ext == "md")
    {
        return Some((
            ItemKind::Memory,
            "Project guidance for an agent was found.".to_owned(),
        ));
    }
    if text.starts_with(".claude/rules/")
        || text.starts_with(".claude/automation/")
        || text.starts_with(".codex/rules/")
        || text.starts_with(".agents/rules/")
        || text.starts_with(".agents/checks/")
        || text.starts_with(".rulesync/rules/")
        || text.starts_with(".rulesync/checks/")
        || path == Path::new(".rulesync/permissions.json")
        || path == Path::new(".rulesync/permissions.jsonc")
        || path == Path::new(".codex/config.toml")
        || path == Path::new(".claude/settings.json")
        || path == Path::new(".claude/settings.local.json")
        || path == Path::new("AGENTS.md")
        || path == Path::new("CLAUDE.md")
        || path == Path::new("CLAUDE.local.md")
    {
        return Some((ItemKind::Rule, "A project rule was found.".to_owned()));
    }
    None
}

fn looks_like_workflow(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    lower.contains("ship-task")
        || lower.contains("subagent") && (lower.contains("review") || lower.contains("parallel"))
        || lower.contains("agent(")
}

fn display_name(path: &Path, kind: ItemKind) -> String {
    if path.starts_with(".agents")
        && let Some(area) = path.components().nth(1)
        && !["agents", "skills", "rules", "commands", "checks", "hooks"]
            .iter()
            .any(|known| area.as_os_str() == *known)
    {
        return area.as_os_str().to_string_lossy().into_owned();
    }
    let name = match kind {
        ItemKind::Skill | ItemKind::Memory
            if path
                .file_name()
                .is_some_and(|name| name == "MEMORY.md" || name == "SKILL.md") =>
        {
            path.parent().and_then(Path::file_name)
        }
        _ => path.file_stem(),
    };
    name.and_then(|name| name.to_str())
        .unwrap_or("Project setting")
        .to_owned()
}

fn identity(path: &Path, hash: &str) -> String {
    format!("{}:{hash}", path.to_string_lossy())
}

/// Stabilny FNV-1a wystarcza do odświeżenia migawki; nie jest podpisem bezpieczeństwa.
#[must_use]
pub fn content_hash(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Nazwy plików w migawce, przydatne do kontroli pełnego pokrycia.
#[must_use]
pub fn paths(snapshot: &DiscoverySnapshot) -> BTreeSet<PathBuf> {
    snapshot
        .items
        .iter()
        .map(|item| item.path.clone())
        .collect()
}
