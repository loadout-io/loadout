//! Małe adaptery formatów źródłowych. Polityka zgodności mieszka w [`translate`](super::translate).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};
use uuid::Uuid;

use crate::connections::{Connection, Origin, Transport};
use crate::library::agents::{
    Agent, Color, FileAccess, SCHEMA, Thinking, Tools, Vendor, VendorOptions,
};
use crate::skills::ingest::{Verdict, from_folder};

use super::discover::{InspectedFile, Inspection};
use super::{Compatibility, Mapping, SkillDraft, SourceKind};

pub(crate) struct AdapterOutput {
    pub agents: Vec<Agent>,
    pub skills: Vec<SkillDraft>,
    pub connections: Vec<Connection>,
    pub mappings: Vec<Mapping>,
}

pub(crate) fn adapt(inspection: &Inspection) -> AdapterOutput {
    let mut output = AdapterOutput {
        agents: Vec::new(),
        skills: Vec::new(),
        connections: Vec::new(),
        mappings: Vec::new(),
    };
    let mut colours = 0_usize;
    let mut seen = BTreeMap::new();

    for file in &inspection.files {
        adapt_one(inspection, file, &mut output, &mut colours, &mut seen);
    }

    output
        .agents
        .sort_by(|left, right| left.name.cmp(&right.name));
    output
        .skills
        .sort_by(|left, right| left.name.cmp(&right.name));
    output
        .connections
        .sort_by(|left, right| left.name.cmp(&right.name));
    output
        .mappings
        .sort_by(|left, right| left.item_id.cmp(&right.item_id));
    output
}

fn adapt_one(
    inspection: &Inspection,
    file: &InspectedFile,
    output: &mut AdapterOutput,
    colours: &mut usize,
    seen: &mut BTreeMap<(super::ItemKind, String), String>,
) {
    use super::ItemKind::{
        Agent as AgentItem, Connection as ConnectionItem, Hook, Memory, Rule, Skill, Unknown,
        Workflow,
    };
    let key = (file.item.kind, file.item.name.to_ascii_lowercase());
    if matches!(file.item.kind, Hook | Memory | Rule | Workflow)
        && seen
            .get(&key)
            .is_some_and(|content| normalized(content) == normalized(&file.content))
    {
        output.mappings.push(mapping(
            file,
            Compatibility::Adjusted,
            "This is another app's copy of the same project behavior.",
        ));
        return;
    }
    seen.entry(key).or_insert_with(|| file.content.clone());

    match file.item.kind {
        AgentItem => adapt_agent(file, output, colours),
        Skill => adapt_skill(inspection, file, output),
        ConnectionItem => adapt_connections(file, output),
        Workflow => adapt_workflow(file, output),
        Hook => output.mappings.push(mapping(
            file,
            Compatibility::NeedsChoice,
            "This project hook will not run automatically. Choose a check or leave it out.",
        )),
        Memory => output.mappings.push(mapping(
            file,
            Compatibility::NeedsChoice,
            "Choose whether to turn this project guidance into Loadout Memory.",
        )),
        Rule => output.mappings.push(mapping(
            file,
            Compatibility::NeedsChoice,
            "Choose whether to turn this project rule into agent instructions or a check.",
        )),
        Unknown => output.mappings.push(mapping(
            file,
            Compatibility::Unsupported,
            "Loadout cannot reproduce this project setting yet.",
        )),
    }
}

fn adapt_agent(file: &InspectedFile, output: &mut AdapterOutput, colours: &mut usize) {
    match agent(file, colour(*colours)) {
        Ok((mut agent, compatibility, message)) => {
            if let Some(existing) = output
                .agents
                .iter_mut()
                .find(|existing| existing.name.eq_ignore_ascii_case(&agent.name))
            {
                let same = normalized(&existing.instructions) == normalized(&agent.instructions);
                merge_names(&mut existing.skills, &agent.skills);
                merge_names(&mut existing.connections, &agent.connections);
                output.mappings.push(mapping(
                    file,
                    if same {
                        Compatibility::Adjusted
                    } else {
                        Compatibility::NeedsChoice
                    },
                    if same {
                        "This is another app's copy of the same native agent."
                    } else {
                        "This app's copy differs from the native agent. Let an agent compare the two versions."
                    },
                ));
                return;
            }
            *colours += 1;
            /* SERWERY Z NAGŁÓWKA TEGO AGENTA WCHODZĄ RAZEM Z NIM. Bez tej linii agent lądował
             * w bibliotece z nazwą połączenia, którego w niej nie było — i przewracał bieg dopiero
             * przy Starcie. Powód w całości stoi przy `servers_in_the_agent`. */
            let mut mine = servers_in_the_agent(file);
            let message = if mine.issues.is_empty() {
                message
            } else {
                format!("{message} {}", mine.issues.join(" "))
            };
            output.mappings.push(mapping(file, compatibility, &message));
            take_connections(&mut output.connections, &mut mine.connections);
            agent.skills.sort();
            agent.connections.sort();
            output.agents.push(agent);
        }
        Err(message) => output
            .mappings
            .push(mapping(file, Compatibility::Unsupported, &message)),
    }
}

fn adapt_skill(inspection: &Inspection, file: &InspectedFile, output: &mut AdapterOutput) {
    match skill(inspection, file) {
        Ok((skill, compatibility @ (Compatibility::Exact | Compatibility::Adjusted))) => {
            if let Some(existing) = output
                .skills
                .iter()
                .find(|existing| existing.name.eq_ignore_ascii_case(&skill.name))
            {
                let same = same_skill_bundle(&existing.source_dir, &skill.source_dir);
                output.mappings.push(mapping(
                    file,
                    if same {
                        Compatibility::Adjusted
                    } else {
                        Compatibility::NeedsChoice
                    },
                    if same {
                        "This is another app's copy of the same portable skill."
                    } else {
                        "This skill has different copies. Let an agent compare them before import."
                    },
                ));
                return;
            }
            let message = if compatibility == Compatibility::Exact {
                "This complete skill bundle can be imported."
            } else {
                "This skill was normalized and reviewed before import."
            };
            output.mappings.push(mapping(file, compatibility, message));
            output.skills.push(skill);
        }
        Ok((_skill, compatibility)) => output.mappings.push(mapping(
            file,
            compatibility,
            "This skill contains instructions that must be resolved before import.",
        )),
        Err(message) => output
            .mappings
            .push(mapping(file, Compatibility::Unsupported, &message)),
    }
}

fn normalized(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn merge_names(existing: &mut Vec<String>, additional: &[String]) {
    for name in additional {
        if !existing.iter().any(|one| one.eq_ignore_ascii_case(name)) {
            existing.push(name.clone());
        }
    }
    existing.sort();
}

fn same_skill_bundle(left: &std::path::Path, right: &std::path::Path) -> bool {
    fn files(root: &std::path::Path) -> Option<BTreeMap<std::path::PathBuf, Vec<u8>>> {
        let mut pending = vec![root.to_path_buf()];
        let mut found = BTreeMap::new();
        while let Some(directory) = pending.pop() {
            let mut entries: Vec<_> = std::fs::read_dir(&directory).ok()?.flatten().collect();
            entries.sort_by_key(std::fs::DirEntry::file_name);
            for entry in entries {
                let path = entry.path();
                let kind = entry.file_type().ok()?;
                if kind.is_symlink() {
                    return None;
                }
                if kind.is_dir() {
                    pending.push(path);
                } else if kind.is_file() {
                    found.insert(
                        path.strip_prefix(root).ok()?.to_path_buf(),
                        std::fs::read(path).ok()?,
                    );
                }
            }
        }
        Some(found)
    }
    files(left)
        .zip(files(right))
        .is_some_and(|(left, right)| left == right)
}

fn adapt_connections(file: &InspectedFile, output: &mut AdapterOutput) {
    match connections(file) {
        Ok(mut parsed) => {
            let compatibility = if parsed.issues.is_empty() {
                Compatibility::Adjusted
            } else if parsed.connections.is_empty() {
                Compatibility::Unsupported
            } else {
                Compatibility::NeedsChoice
            };
            let message = if parsed.issues.is_empty() {
                "These project connections will be imported switched off.".to_owned()
            } else {
                parsed.issues.join(" ")
            };
            output.mappings.push(mapping(file, compatibility, &message));
            take_connections(&mut output.connections, &mut parsed.connections);
        }
        Err(message) => output
            .mappings
            .push(mapping(file, Compatibility::Unsupported, &message)),
    }
}

/// Dokłada połączenia, których jeszcze nie ma — po NAZWIE, bo to ona jest tożsamością.
///
/// Ten sam serwer bywa opisany dwa razy: raz w `.mcp.json` projektu, raz w nagłówku agenta,
/// który go używa. Dwa wpisy o jednej nazwie dałyby w bibliotece dwa pliki i człowieka, który
/// włącza jeden, a bieg czyta drugi. Wygrywa PIERWSZY napotkany — bo importer idzie po plikach
/// w kolejności skanu, a ta jest stabilna.
pub(super) fn take_connections(into: &mut Vec<Connection>, found: &mut Vec<Connection>) {
    for connection in found.drain(..) {
        if !into
            .iter()
            .any(|have| have.name.eq_ignore_ascii_case(&connection.name))
        {
            into.push(connection);
        }
    }
}

fn adapt_workflow(file: &InspectedFile, output: &mut AdapterOutput) {
    // Jawny plik workflow jest programem vendora, więc samo znalezienie znanych nazw ról nie
    // dowodzi, że umieliśmy odtworzyć jego graf. Znany szablon musi pochodzić z bundle skilla.
    let known = file
        .item
        .path
        .file_name()
        .is_some_and(|name| name == "SKILL.md")
        && knows_ship_ui(&file.content);
    output.mappings.push(mapping(
        file,
        if known {
            Compatibility::Adjusted
        } else {
            Compatibility::NeedsChoice
        },
        if known {
            "This coordinating skill will become a visible Ship UI workflow."
        } else {
            "Choose how this coordinating skill should be represented as a workflow."
        },
    ));
}

fn mapping(file: &InspectedFile, compatibility: Compatibility, message: &str) -> Mapping {
    Mapping {
        item_id: file.item.id.clone(),
        compatibility,
        message: message.to_owned(),
    }
}

fn colour(index: usize) -> Color {
    [
        Color::Slate,
        Color::Plum,
        Color::Clay,
        Color::Moss,
        Color::Rose,
    ][index % 5]
}

fn agent(
    file: &InspectedFile,
    color: Color,
) -> std::result::Result<(Agent, Compatibility, String), String> {
    match file.item.source {
        SourceKind::Claude => claude_agent(file, color),
        SourceKind::Codex => codex_agent(file, color),
        SourceKind::Rulesync | SourceKind::AgentSkills => rulesync_agent(file, color),
        _ => Err("This agent format is not supported.".to_owned()),
    }
}

fn claude_agent(
    file: &InspectedFile,
    fallback_color: Color,
) -> std::result::Result<(Agent, Compatibility, String), String> {
    let (fields, body) = markdown_frontmatter(&file.content)?;
    reject_unknown_fields(
        &fields,
        &[
            "name",
            "description",
            "model",
            "permissionMode",
            "tools",
            "disallowedTools",
            "skills",
            "mcpServers",
            "maxTurns",
            "memory",
            "color",
            "type",
        ],
        "Claude agent",
    )?;
    claude_agent_from_fields(file, &fields, body, fallback_color, Vec::new())
}

fn rulesync_agent(
    file: &InspectedFile,
    fallback_color: Color,
) -> std::result::Result<(Agent, Compatibility, String), String> {
    let (root, body) = markdown_frontmatter(&file.content)?;
    let mut fields = nested_yaml_fields(&file.content, "claudecode");
    for key in ["name", "description"] {
        if let Some(value) = root.get(key) {
            fields.insert(key.to_owned(), value.clone());
        }
    }
    claude_agent_from_fields(file, &fields, body, fallback_color, vec!["target app"])
}

fn claude_agent_from_fields(
    file: &InspectedFile,
    fields: &BTreeMap<String, String>,
    body: &str,
    fallback_color: Color,
    mut choices: Vec<&'static str>,
) -> std::result::Result<(Agent, Compatibility, String), String> {
    let name = fields
        .get("name")
        .cloned()
        .unwrap_or_else(|| file.item.name.clone());
    if body.trim().is_empty() {
        return Err("This agent has no instructions.".to_owned());
    }
    let access = match fields.get("permissionMode").map(String::as_str) {
        Some("bypassPermissions" | "dontAsk") => {
            return Err(
                "This agent bypasses permission checks. Choose a Loadout file access policy before importing it."
                    .to_owned(),
            );
        }
        Some("acceptEdits" | "auto") => FileAccess::AskFirst,
        _ => FileAccess::LookOnly,
    };
    let denied_tools = list_field(fields.get("disallowedTools"));
    let tools: Vec<_> = list_field(fields.get("tools"))
        .into_iter()
        .filter(|tool| !denied_tools.contains(tool))
        .collect();
    let skills = list_field(fields.get("skills"));
    let connections = nested_names(&file.content, "mcpServers");
    if fields.contains_key("memory") {
        choices.push("project memory");
    }
    if fields.contains_key("maxTurns") {
        choices.push("turn limit");
    }
    if fields.contains_key("type") {
        choices.push("agent type");
    }
    let color = fields
        .get("color")
        .map_or(fallback_color, |value| claude_color(value, fallback_color));
    let agent = Agent {
        schema: SCHEMA,
        id: Uuid::now_v7(),
        name,
        summary: fields
            .get("description")
            .cloned()
            .unwrap_or_else(|| "Imported project role".to_owned()),
        color,
        instructions: body.trim().to_owned(),
        runs_with: Vendor::ClaudeCode,
        model: fields
            .get("model")
            .cloned()
            .unwrap_or_else(|| "sonnet".to_owned()),
        thinking: Thinking::Balanced,
        file_access: access,
        give_up_after_minutes: 20,
        tools: if tools.is_empty() {
            Tools::Everything
        } else {
            Tools::Only(tools)
        },
        reaches_the_web: crate::library::agents::reaching_the_web(),
        skills,
        connections,
        write_results_to: String::new(),
        vendor_options: VendorOptions::new(),
    };
    if choices.is_empty() {
        Ok((
            agent,
            Compatibility::Exact,
            "This agent will become a native Loadout agent.".to_owned(),
        ))
    } else {
        Ok((
            agent,
            Compatibility::NeedsChoice,
            format!(
                "Choose how to reproduce this agent's {}.",
                choices.join(" and ")
            ),
        ))
    }
}

fn codex_agent(
    file: &InspectedFile,
    color: Color,
) -> std::result::Result<(Agent, Compatibility, String), String> {
    let fields = flat_toml(&file.content);
    reject_unknown_fields(
        &fields,
        &[
            "name",
            "description",
            "model",
            "model_reasoning_effort",
            "sandbox_mode",
            "developer_instructions",
        ],
        "Codex agent",
    )?;
    let name = fields
        .get("name")
        .cloned()
        .unwrap_or_else(|| file.item.name.clone());
    let instructions = fields
        .get("developer_instructions")
        .cloned()
        .ok_or_else(|| "This Codex agent has no developer instructions.".to_owned())?;
    let thinking = match fields.get("model_reasoning_effort").map(String::as_str) {
        Some("low") => Thinking::Quick,
        Some("high") => Thinking::Deep,
        Some("xhigh" | "max") => Thinking::Deepest,
        _ => Thinking::Balanced,
    };
    let file_access = match fields.get("sandbox_mode").map(String::as_str) {
        Some("danger-full-access") => FileAccess::WorkFreely,
        Some("workspace-write") => FileAccess::AskFirst,
        _ => FileAccess::LookOnly,
    };
    Ok((
        Agent {
            schema: SCHEMA,
            id: Uuid::now_v7(),
            name,
            summary: fields
                .get("description")
                .cloned()
                .unwrap_or_else(|| "Imported project role".to_owned()),
            color,
            instructions,
            runs_with: Vendor::Codex,
            model: fields
                .get("model")
                .cloned()
                .unwrap_or_else(|| "gpt-5.6-sol".to_owned()),
            thinking,
            file_access,
            give_up_after_minutes: 20,
            tools: Tools::Everything,
            reaches_the_web: crate::library::agents::reaching_the_web(),
            skills: Vec::new(),
            connections: nested_toml_tables(&file.content, "mcp_servers"),
            write_results_to: String::new(),
            vendor_options: VendorOptions::new(),
        },
        Compatibility::Exact,
        "This agent will become a native Loadout agent.".to_owned(),
    ))
}

fn claude_color(value: &str, fallback: Color) -> Color {
    match value.to_ascii_lowercase().as_str() {
        "purple" => Color::Plum,
        "red" | "pink" => Color::Rose,
        "green" => Color::Moss,
        "yellow" | "orange" => Color::Clay,
        "blue" | "cyan" => Color::Slate,
        _ => fallback,
    }
}

fn reject_unknown_fields(
    fields: &BTreeMap<String, String>,
    allowed: &[&str],
    kind: &str,
) -> std::result::Result<(), String> {
    if let Some(field) = fields
        .keys()
        .find(|field| !allowed.contains(&field.as_str()))
    {
        return Err(format!(
            "This {kind} uses {field}, which Loadout cannot reproduce yet."
        ));
    }
    Ok(())
}

fn skill(
    inspection: &Inspection,
    file: &InspectedFile,
) -> std::result::Result<(SkillDraft, Compatibility), String> {
    let source = inspection.snapshot.root.join(&file.item.path);
    let Some(dir) = source.parent() else {
        return Err("This skill has no folder.".to_owned());
    };
    let imported = from_folder(dir).map_err(|error| error.to_string())?;
    let compatibility = match imported.reviewed.verdict {
        Verdict::Clean if imported.reviewed.body == file.content => Compatibility::Exact,
        Verdict::Clean | Verdict::Concerns => Compatibility::Adjusted,
        Verdict::Blocked => Compatibility::Unsupported,
    };
    Ok((
        SkillDraft {
            name: imported.skill.name,
            source_dir: dir.to_path_buf(),
            source_hash: file.item.hash.clone(),
        },
        compatibility,
    ))
}

pub(super) struct AdaptedConnections {
    pub(super) connections: Vec<Connection>,
    issues: Vec<String>,
}

fn connections(file: &InspectedFile) -> std::result::Result<AdaptedConnections, String> {
    match file.item.source {
        SourceKind::Claude => claude_connections(file),
        SourceKind::Codex => codex_connections(file).map(|connections| AdaptedConnections {
            connections,
            issues: Vec::new(),
        }),
        SourceKind::Rulesync | SourceKind::AgentSkills => rulesync_connections(file),
        _ => Err("This connection format is not supported.".to_owned()),
    }
}

fn rulesync_connections(file: &InspectedFile) -> std::result::Result<AdaptedConnections, String> {
    let json = jsonc_to_json(&file.content)?;
    let document: Value = serde_json::from_str(&json)
        .map_err(|error| format!("This Rulesync connection file is not valid JSONC: {error}"))?;
    let servers = document
        .get("mcpServers")
        .and_then(Value::as_object)
        .ok_or_else(|| "This file does not contain project connections.".to_owned())?;
    let mut connections = Vec::new();
    let mut issues = Vec::new();
    for (name, server) in servers {
        if name == "$schema" {
            continue;
        }
        match claude_connection(&file.item.path, &file.item.hash, name, server) {
            Ok(connection) => connections.push(connection),
            Err(issue) => issues.push(issue),
        }
    }
    if document.as_object().is_some_and(|root| {
        root.keys()
            .any(|key| key != "$schema" && key != "mcpServers")
    }) {
        issues.push(
            "Tool-specific connection overrides need a choice before they can be reproduced."
                .to_owned(),
        );
    }
    Ok(AdaptedConnections {
        connections,
        issues,
    })
}

/// Czy ten adres wolno wpuścić bez HTTPS.
///
/// **Pętla zwrotna tak, wszystko inne nie**, i to jest cała treść tej funkcji (T-81, AC-2).
/// Reguła `starts_with("https://")` broniła przed sekretem lecącym po sieci bez szyfrowania —
/// a ruch, który nie wychodzi z maszyny, tej ochrony nie potrzebuje, bo nie ma go gdzie podsłuchać.
///
/// 2026-08-22 — KOSZT TEJ ODMOWY BYŁ PRAWDZIWY, nie teoretyczny. Figma instaluje swój serwer
/// Dev Mode jako `http://127.0.0.1:3845/mcp`; import wyrzucał go z każdego skanu, a właściciel
/// dostawał przy biegu `Connection figma does not exist in the Loadout library.` — zdanie
/// o skutku, którego przyczyna stała dwa ekrany wcześniej.
///
/// Nazwa hosta, nie prefiks napisu: `http://127.0.0.1.evil.test/` zaczyna się od `http://127.0.0.1`
/// i pętlą zwrotną nie jest. Dlatego host jest wycinany do najbliższego `/`, `:` albo `?`
/// i porównywany W CAŁOŚCI.
fn is_loopback(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    // Port odcinamy TYLKO wtedy, gdy naprawdę jest portem: `[::1]` niesie dwukropki w samym
    // adresie i bez tego warunku zostałby przycięty do `[:`.
    let host = match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.chars().all(|one| one.is_ascii_digit()) => {
            host
        }
        _ => authority,
    };
    matches!(host, "127.0.0.1" | "localhost" | "[::1]")
}

/// Serwery narzędziowe zadeklarowane WPROST W NAGŁÓWKU AGENTA — jako gotowe połączenia.
///
/// 2026-08-22 — TO ZAMYKA CICHĄ DZIURĘ W IMPORCIE, zgłoszoną zdaniem „jak dziedziczymy agentów
/// i skille, to tak samo wszystko MCP, żeby nie było niespodzianek". Do tego dnia z takiego bloku
/// brane były wyłącznie NAZWY (do `Agent::connections`), a transport przepadał. Agent lądował
/// więc w bibliotece z nazwą połączenia, którego w niej nie było, i przewracał się dopiero przy
/// Starcie zdaniem `Connection <nazwa> does not exist in the Loadout library.` — o kroku, którego
/// człowiek w tej chwili nie oglądał.
///
/// Reguły bezpieczeństwa są POŻYCZONE, nie przepisane: blok zamienia się na ten sam kształt JSON,
/// który niesie `.mcp.json`, i idzie przez [`claude_connection`]. Dzięki temu HTTPS-albo-pętla-
/// zwrotna, zakaz wartości sekretów i wszystko inne obowiązuje tu co do znaku, a nowa reguła
/// dopisana tam obowiązuje tu od razu.
///
/// Czytanie jest wcięciowe, nie parserem YAML — ta sama technika i to samo zastrzeżenie, co przy
/// [`nested_names`]: prawdziwy parser należy do reszty T-81. Do tego czasu lepiej wnieść serwer
/// czytelny z dwóch poziomów wcięcia niż zgubić go w całości.
fn servers_in_the_agent(file: &InspectedFile) -> AdaptedConnections {
    let mut connections = Vec::new();
    let mut issues = Vec::new();
    let mut inside = false;
    let mut level: Option<usize> = None;
    let mut open: Option<(String, Map<String, Value>)> = None;

    let close = |open: Option<(String, Map<String, Value>)>,
                 connections: &mut Vec<Connection>,
                 issues: &mut Vec<String>| {
        let Some((name, fields)) = open else { return };
        match claude_connection(
            &file.item.path,
            &file.item.hash,
            &name,
            &Value::Object(fields),
        ) {
            Ok(connection) => connections.push(connection),
            Err(issue) => issues.push(issue),
        }
    };

    for line in file.content.lines() {
        if line.trim() == "mcpServers:" {
            inside = true;
            continue;
        }
        if !inside || line.trim().is_empty() {
            continue;
        }
        let spaces = line.len() - line.trim_start().len();
        if spaces == 0 {
            break;
        }
        let first = *level.get_or_insert(spaces);
        let Some((key, value)) = line.trim().split_once(':') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        if spaces == first {
            close(open.take(), &mut connections, &mut issues);
            open = Some((key.to_owned(), Map::new()));
            continue;
        }
        let Some((_, fields)) = open.as_mut() else {
            continue;
        };
        /* `args` jest w nagłówku tablicą w zapisie wbudowanym, a `env`/`headers` schodzą jeszcze
         * głębiej i są odmową w tym samym brzmieniu, co przy Codeksie: wartość sekretu w pliku
         * projektu nie ma prawa wjechać do biblioteki, także w bloku agenta. */
        if key == "args" {
            fields.insert(
                key.to_owned(),
                Value::Array(
                    parse_array(Some(&value.to_owned()))
                        .into_iter()
                        .map(Value::String)
                        .collect(),
                ),
            );
        } else if value.is_empty() {
            issues.push(format!(
                "Connection {} contains inline environment or header values. Replace them with \
                 environment references.",
                open.as_ref().map_or("", |(name, _)| name.as_str())
            ));
        } else {
            fields.insert(key.to_owned(), Value::String(unquote(value)));
        }
    }
    close(open.take(), &mut connections, &mut issues);

    AdaptedConnections {
        connections,
        issues,
    }
}

/// Serwery z TWOJEJ konfiguracji Claude Code — dwa zakresy, których plik projektu nie zna.
///
/// 2026-08-22 — ZGŁOSZENIE WŁAŚCICIELA: „chodzi mi o to, żeby tego typu MCP też sobie
/// importować". Claude Code ma trzy zakresy, a import czytał jeden:
///
/// | zakres | gdzie | kto to widzi |
/// |---|---|---|
/// | project | `.mcp.json` | cały zespół |
/// | local | `~/.claude.json` → `projects["<katalog>"]` | tylko ty, w tym projekcie |
/// | user | `~/.claude.json` → `mcpServers` | tylko ty, wszędzie |
///
/// `linear-server`, na którym stoi całe `ship-task` w repo właściciela, siedział w LOKALNYM.
/// Dlatego nie było go w `.mcp.json`, import go nie widział, a bieg odmawiał startu na kroku,
/// który miał przeczytać ticket.
///
/// **CZYTAMY DWA KLUCZE I NIC WIĘCEJ.** `~/.claude.json` niesie u właściciela 231 projektów
/// z historią rozmów i sporo prywatnego stanu; wchodzimy po `mcpServers`, wychodzimy. Reszta
/// pliku nie ma prawa dotknąć ani draftu, ani odcisków źródeł.
///
/// **`projects` dopasowane po ŚCIEŻCE SKANOWANEGO KATALOGU**, nie „weź wszystkie": import
/// zostaje projektowy, więc skan `urc-monorepo` daje serwery `urc-monorepo`, a nie wszystkich
/// 231 pozycji z tamtego pliku.
///
/// Reguły bezpieczeństwa są POŻYCZONE od [`claude_connection`]: HTTPS-albo-pętla-zwrotna, zakaz
/// wartości sekretów, wszystko obowiązuje tu co do znaku. Serwer z twojego pliku nie jest
/// bezpieczniejszy od serwera z repo tylko dlatego, że jest twój.
pub(super) fn personal_connections(home: &Path, root: &Path) -> AdaptedConnections {
    let mut connections = Vec::new();
    let mut issues = Vec::new();

    let path = home.join(".claude.json");
    let Ok(text) = fs::read_to_string(&path) else {
        return AdaptedConnections {
            connections,
            issues,
        };
    };
    let Ok(document) = serde_json::from_str::<Value>(&text) else {
        issues.push(
            "Your own Claude Code settings are not valid JSON, so Loadout left them alone."
                .to_owned(),
        );
        return AdaptedConnections {
            connections,
            issues,
        };
    };

    // Kolejność: NAJPIERW ten projekt, potem „wszędzie". Przy tej samej nazwie w obu zakresach
    // wygrywa węższy — bo to on opisuje serwer, którego ten projekt naprawdę używa.
    /* KLUCZ DOPASOWANY PO ŚCIEŻCE KANONICZNEJ, nie po napisie. macOS podaje `/var/folders/…`
     * i `/private/var/folders/…` jako dwie nazwy jednego katalogu, a `~/.claude.json` niesie tę,
     * którą akurat miał terminal. Porównanie napisów gubi wtedy cały zakres lokalny i wygląda
     * dokładnie jak „nie masz tam żadnych serwerów". */
    let wanted = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let here = document
        .get("projects")
        .and_then(Value::as_object)
        .and_then(|projects| {
            projects.iter().find_map(|(key, value)| {
                let key_path = Path::new(key);
                let same = fs::canonicalize(key_path)
                    .map_or_else(|_| key_path == root, |canonical| canonical == wanted);
                same.then_some(value)
            })
        })
        .and_then(|project| project.get("mcpServers"));
    let everywhere = document.get("mcpServers");

    for (servers, origin) in [
        (here, Origin::YoursHere),
        (everywhere, Origin::YoursEverywhere),
    ] {
        let Some(servers) = servers.and_then(Value::as_object) else {
            continue;
        };
        for (name, server) in servers {
            /* Pusty odcisk i ścieżka bez katalogu domowego: `source_hash` służy wykrywaniu zmiany
             * w plikach PROJEKTU między Scanem a Importem, a te dwa zakresy do `source_hashes`
             * nie wchodzą. Ścieżka jest etykietą dla człowieka, nie kluczem — twojego katalogu
             * domowego nie ma po co wpisywać do pliku połączenia. */
            match claude_connection(Path::new(".claude.json"), "", name, server) {
                Ok(mut connection) => {
                    connection.origin = origin;
                    connections.push(connection);
                }
                Err(issue) => issues.push(issue),
            }
        }
    }

    AdaptedConnections {
        connections,
        issues,
    }
}

fn claude_connections(file: &InspectedFile) -> std::result::Result<AdaptedConnections, String> {
    let document: Value = serde_json::from_str(&file.content)
        .map_err(|error| format!("This project connection file is not valid JSON: {error}"))?;
    let servers = document
        .get("mcpServers")
        .and_then(Value::as_object)
        .ok_or_else(|| "This file does not contain project connections.".to_owned())?;
    let mut out = Vec::new();
    let mut issues = Vec::new();
    for (name, server) in servers {
        match claude_connection(&file.item.path, &file.item.hash, name, server) {
            Ok(connection) => out.push(connection),
            Err(issue) => issues.push(issue),
        }
    }
    Ok(AdaptedConnections {
        connections: out,
        issues,
    })
}

fn claude_connection(
    source: &Path,
    hash: &str,
    name: &str,
    server: &Value,
) -> std::result::Result<Connection, String> {
    let object = server
        .as_object()
        .ok_or_else(|| format!("Connection {name} is not an object."))?;
    let transport = if let Some(url) = object.get("url").and_then(Value::as_str) {
        if !url.starts_with("https://") && !is_loopback(url) {
            return Err(format!(
                "Connection {name} must use HTTPS, or run on this machine."
            ));
        }
        let token_environment = object
            .get("bearerTokenEnvVar")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if token_environment
            .as_deref()
            .is_some_and(|name| !environment_name(name))
        {
            return Err(format!(
                "Connection {name} must name an environment variable instead of storing a token."
            ));
        }
        Transport::Http {
            url: url.to_owned(),
            token_environment,
        }
    } else {
        let command = object
            .get("command")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Connection {name} has no command or HTTPS URL."))?;
        let args = string_array(object.get("args"))?;
        let mut environment = Vec::new();
        if let Some(env) = object.get("env").and_then(Value::as_object) {
            for (key, value) in env {
                let Some(value) = value.as_str() else {
                    return Err(format!(
                        "Connection {name} has a non-text environment value."
                    ));
                };
                if value != format!("${{{key}}}") && value != format!("${key}") {
                    return Err(format!(
                        "Connection {name} contains a literal secret. Replace it with an environment reference."
                    ));
                }
                environment.push(key.clone());
            }
        }
        environment.sort();
        Transport::Stdio {
            command: command.to_owned(),
            args,
            environment,
        }
    };
    Ok(Connection::imported(
        slug(name),
        name.to_owned(),
        transport,
        source.to_path_buf(),
        hash.to_owned(),
    ))
}

fn codex_connections(file: &InspectedFile) -> std::result::Result<Vec<Connection>, String> {
    let tables = toml_tables(&file.content, "mcp_servers");
    if tables.is_empty() {
        return Err("This file does not contain project connections.".to_owned());
    }
    let mut out = Vec::new();
    for (name, fields) in tables {
        if fields.contains_key("env") || fields.contains_key("http_headers") {
            return Err(format!(
                "Connection {name} contains inline environment or header values. Replace them with environment references."
            ));
        }
        let transport = if let Some(url) = fields.get("url") {
            if !url.starts_with("https://") && !is_loopback(url) {
                return Err(format!(
                    "Connection {name} must use HTTPS, or run on this machine."
                ));
            }
            let token_environment = fields.get("bearer_token_env_var").cloned();
            if token_environment
                .as_deref()
                .is_some_and(|value| !environment_name(value))
            {
                return Err(format!(
                    "Connection {name} must name an environment variable instead of storing a token."
                ));
            }
            Transport::Http {
                url: url.clone(),
                token_environment,
            }
        } else {
            let command = fields
                .get("command")
                .cloned()
                .ok_or_else(|| format!("Connection {name} has no command or HTTPS URL."))?;
            let environment = parse_array(fields.get("required_env"));
            if environment.iter().any(|value| !environment_name(value)) {
                return Err(format!(
                    "Connection {name} has an invalid required environment variable name."
                ));
            }
            Transport::Stdio {
                command,
                args: parse_array(fields.get("args")),
                environment,
            }
        };
        out.push(Connection::imported(
            slug(&name),
            name,
            transport,
            file.item.path.clone(),
            file.item.hash.clone(),
        ));
    }
    Ok(out)
}

fn markdown_frontmatter(
    content: &str,
) -> std::result::Result<(BTreeMap<String, String>, &str), String> {
    if !content.starts_with("---\n") {
        return Ok((BTreeMap::new(), content));
    }
    let rest = &content[4..];
    let Some(end) = rest.find("\n---") else {
        return Err("This agent has an unfinished front matter block.".to_owned());
    };
    let mut fields = BTreeMap::new();
    let lines: Vec<_> = rest[..end].lines().collect();
    let mut index = 0_usize;
    while index < lines.len() {
        let line = lines[index];
        index += 1;
        if line.starts_with(' ') || line.trim().is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let raw = value.trim();
        if [">", ">-", "|", "|-"].contains(&raw) {
            let mut parts = Vec::new();
            while index < lines.len() && (lines[index].starts_with(' ') || lines[index].is_empty())
            {
                let next = lines[index].trim();
                if !next.is_empty() {
                    parts.push(next);
                }
                index += 1;
            }
            fields.insert(
                key.trim().to_owned(),
                if raw.starts_with('>') {
                    parts.join(" ")
                } else {
                    parts.join("\n")
                },
            );
        } else {
            fields.insert(key.trim().to_owned(), unquote(raw));
        }
    }
    Ok((fields, rest[end + 4..].trim_start_matches('\n')))
}

fn nested_yaml_fields(content: &str, section: &str) -> BTreeMap<String, String> {
    let Some(front) = frontmatter_text(content) else {
        return BTreeMap::new();
    };
    let mut fields = BTreeMap::new();
    let mut inside = false;
    for line in front.lines() {
        let indent = line.len() - line.trim_start().len();
        if indent == 0 {
            inside = line.trim() == format!("{section}:");
            continue;
        }
        if !inside || indent != 2 {
            continue;
        }
        if let Some((key, value)) = line.trim().split_once(':') {
            fields.insert(key.trim().to_owned(), unquote(value.trim()));
        }
    }
    fields
}

fn frontmatter_text(content: &str) -> Option<&str> {
    let rest = content.strip_prefix("---\n")?;
    let end = rest.find("\n---")?;
    Some(&rest[..end])
}

fn jsonc_to_json(content: &str) -> std::result::Result<String, String> {
    let mut without_comments = String::with_capacity(content.len());
    let mut characters = content.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(character) = characters.next() {
        if in_string {
            without_comments.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        if character == '"' {
            in_string = true;
            without_comments.push(character);
        } else if character == '/' && characters.peek() == Some(&'/') {
            characters.next();
            for next in characters.by_ref() {
                if next == '\n' {
                    without_comments.push('\n');
                    break;
                }
            }
        } else if character == '/' && characters.peek() == Some(&'*') {
            characters.next();
            let mut previous = '\0';
            for next in characters.by_ref() {
                if previous == '*' && next == '/' {
                    break;
                }
                previous = next;
            }
        } else {
            without_comments.push(character);
        }
    }
    if in_string {
        return Err("This Rulesync connection file has an unfinished string.".to_owned());
    }

    let mut out = String::with_capacity(without_comments.len());
    let mut chars = without_comments.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(character) = chars.next() {
        if in_string {
            out.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        if character == '"' {
            in_string = true;
            out.push(character);
        } else if character == ',' {
            let mut lookahead = chars.clone();
            if lookahead
                .find(|next| !next.is_whitespace())
                .is_some_and(|next| next == '}' || next == ']')
            {
                continue;
            }
            out.push(character);
        } else {
            out.push(character);
        }
    }
    Ok(out)
}

fn flat_toml(content: &str) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    let mut lines = content.lines().peekable();
    while let Some(line) = lines.next() {
        let line = line.trim();
        if line.starts_with('[') || line.starts_with('#') || line.is_empty() {
            continue;
        }
        let Some((key, raw)) = line.split_once('=') else {
            continue;
        };
        let mut value = raw.trim().to_owned();
        if value.starts_with("\"\"\"") {
            let trimmed = value.trim_start_matches("\"\"\"").to_owned();
            value = trimmed;
            while !value.ends_with("\"\"\"") {
                let Some(next) = lines.next() else { break };
                value.push('\n');
                value.push_str(next);
            }
            let trimmed = value.trim_end_matches("\"\"\"").to_owned();
            value = trimmed;
        } else {
            value = unquote(&value);
        }
        fields.insert(key.trim().to_owned(), value);
    }
    fields
}

fn toml_tables(content: &str, prefix: &str) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in content.lines() {
        let line = line.trim();
        if let Some(table) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            current = table
                .strip_prefix(&format!("{prefix}."))
                .map(|name| unquote(name.trim()));
            continue;
        }
        let Some(name) = current.as_ref() else {
            continue;
        };
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        out.entry(name.clone())
            .or_insert_with(BTreeMap::new)
            .insert(key.trim().to_owned(), unquote(value.trim()));
    }
    out
}

fn nested_toml_tables(content: &str, prefix: &str) -> Vec<String> {
    toml_tables(content, prefix).into_keys().collect()
}

/// Nazwy z PIERWSZEGO poziomu zagnieżdżenia pod `key:` w nagłówku pliku agenta.
///
/// 2026-08-22 — POZIOM, NIE CZARNA LISTA, i to jest naprawa wady zgłoszonej z ekranu (T-81,
/// pierwsza połowa). Do tego dnia ta funkcja brała KAŻDY klucz wcięty o co najmniej dwie spacje,
/// odejmując cztery nazwy wypisane z ręki (`command`, `args`, `env`, `url`). Serwer opisany tak:
///
/// ```yaml
/// mcpServers:
///   figma:
///     type: http
///     url: http://127.0.0.1:3845/mcp
/// ```
///
/// dawał więc DWA połączenia: `figma` i `type` — bo `type` nie stało na tamtej liście.
/// Nie była to kosmetyka: `connections::runtime::selected()` odmawia startu przy nieznanej
/// nazwie, więc **zaimportowany agent z zagnieżdżonym blokiem `mcpServers` nie dawał się
/// uruchomić w ogóle**, a zdanie, które człowiek widział, mówiło o połączeniu `type`, którego
/// nikt nigdy nie napisał.
///
/// Czarna lista jest naprawą objawu i przegrywa z każdym kluczem, którego jeszcze nikt nie
/// napotkał (`headers`, `timeout`, `disabled`…). Poziom wcięcia jest tym, co naprawdę odróżnia
/// nazwę serwera od jego pola. Prawdziwy parser YAML należy do reszty T-81 i tej funkcji nie
/// zastępuje niniejsza poprawka — zmniejsza tylko szkodę do czasu, aż tamten wejdzie.
fn nested_names(content: &str, key: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut inside = false;
    // Wcięcie pierwszego dziecka. Ustala je PIERWSZY niepusty wiersz po `key:`, bo YAML nie
    // narzuca dwóch spacji — pliki z czterema są równie poprawne.
    let mut level: Option<usize> = None;
    for line in content.lines() {
        if line.trim() == format!("{key}:") {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let spaces = line.len() - line.trim_start().len();
        if spaces == 0 {
            break;
        }
        let first = *level.get_or_insert(spaces);
        if spaces != first {
            // Głębiej: to pole serwera (`type`, `url`, `headers`…), nie jego nazwa. Płycej niż
            // pierwsze dziecko przy niezerowym wcięciu nie powinno się zdarzyć, ale gdyby plik
            // był poprawiony ręcznie, wolimy pominąć niż zmyślić nazwę.
            continue;
        }
        if let Some((name, _)) = line.trim().split_once(':')
            && !name.is_empty()
        {
            names.push(name.to_owned());
        }
    }
    names.sort();
    names.dedup();
    names
}

fn list_field(value: Option<&String>) -> Vec<String> {
    value.map_or_else(Vec::new, |value| parse_array(Some(value)))
}

fn parse_array(value: Option<&String>) -> Vec<String> {
    value.map_or_else(Vec::new, |value| {
        value
            .trim_matches(['[', ']'])
            .split(',')
            .map(|part| unquote(part.trim()))
            .filter(|part| !part.is_empty())
            .collect()
    })
}

fn string_array(value: Option<&Value>) -> std::result::Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .ok_or_else(|| "Connection arguments must be a list.".to_owned())?
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or_else(|| "Connection arguments must be text.".to_owned())
        })
        .collect()
}

fn unquote(value: &str) -> String {
    value.trim().trim_matches('"').trim_matches('\'').to_owned()
}

fn slug(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn environment_name(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

pub(crate) fn knows_ship_ui(content: &str) -> bool {
    ["frontend-dev", "design-qa", "code-reviewer"]
        .iter()
        .all(|role| content.contains(role))
        && check_command(content).is_some()
}

pub(crate) fn check_command(source: &str) -> Option<String> {
    for (index, piece) in source.split('`').enumerate() {
        if index % 2 == 0 {
            continue;
        }
        let command = piece.trim();
        if command.starts_with("./")
            && (command.contains("verify") || command.contains("test") || command.contains("ci"))
            && !command.contains('\n')
        {
            return Some(command.to_owned());
        }
    }
    None
}
