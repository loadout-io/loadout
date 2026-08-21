//! Małe adaptery formatów źródłowych. Polityka zgodności mieszka w [`translate`](super::translate).

use std::collections::BTreeMap;

use serde_json::Value;
use uuid::Uuid;

use crate::connections::{Connection, Transport};
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

    for file in &inspection.files {
        adapt_one(inspection, file, &mut output, &mut colours);
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
) {
    use super::ItemKind::{
        Agent as AgentItem, Connection as ConnectionItem, Hook, Skill, Unknown, Workflow,
    };
    match file.item.kind {
        AgentItem => match agent(file, colour(*colours)) {
            Ok(agent) => {
                *colours += 1;
                output.mappings.push(mapping(
                    file,
                    Compatibility::Exact,
                    "This agent will become a native Loadout agent.",
                ));
                output.agents.push(agent);
            }
            Err(message) => {
                output
                    .mappings
                    .push(mapping(file, Compatibility::Unsupported, &message));
            }
        },
        Skill => match skill(inspection, file) {
            Ok((skill, Compatibility::Exact)) => {
                output.mappings.push(mapping(
                    file,
                    Compatibility::Exact,
                    "This complete skill bundle can be imported.",
                ));
                output.skills.push(skill);
            }
            Ok((skill, Compatibility::Adjusted)) => {
                output.mappings.push(mapping(
                    file,
                    Compatibility::Adjusted,
                    "This skill was normalized and reviewed before import.",
                ));
                output.skills.push(skill);
            }
            Ok((_skill, compatibility)) => output.mappings.push(mapping(
                file,
                compatibility,
                "This skill contains instructions that must be resolved before import.",
            )),
            Err(message) => {
                output
                    .mappings
                    .push(mapping(file, Compatibility::Unsupported, &message));
            }
        },
        ConnectionItem => {
            let parsed = connections(file);
            match parsed {
                Ok(mut connections) => {
                    output.mappings.push(mapping(
                        file,
                        Compatibility::Adjusted,
                        "These project connections will be imported switched off.",
                    ));
                    output.connections.append(&mut connections);
                }
                Err(message) => {
                    output
                        .mappings
                        .push(mapping(file, Compatibility::Unsupported, &message));
                }
            }
        }
        Workflow => {
            let known = knows_ship_ui(&file.content);
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
        Hook => output.mappings.push(mapping(
            file,
            Compatibility::NeedsChoice,
            "This project hook will not run automatically. Choose a check or leave it out.",
        )),
        Unknown => output.mappings.push(mapping(
            file,
            Compatibility::Unsupported,
            "Loadout cannot reproduce this project setting yet.",
        )),
    }
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

fn agent(file: &InspectedFile, color: Color) -> std::result::Result<Agent, String> {
    match file.item.source {
        SourceKind::Claude => claude_agent(file, color),
        SourceKind::Codex => codex_agent(file, color),
        _ => Err("This agent format is not supported.".to_owned()),
    }
}

fn claude_agent(file: &InspectedFile, color: Color) -> std::result::Result<Agent, String> {
    let (fields, body) = markdown_frontmatter(&file.content)?;
    reject_unknown_fields(
        &fields,
        &[
            "name",
            "description",
            "model",
            "permissionMode",
            "tools",
            "skills",
            "mcpServers",
        ],
        "Claude agent",
    )?;
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
    let tools = list_field(fields.get("tools"));
    let skills = list_field(fields.get("skills"));
    let connections = nested_names(&file.content, "mcpServers");
    Ok(Agent {
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
        skills,
        connections,
        write_results_to: String::new(),
        vendor_options: VendorOptions::new(),
    })
}

fn codex_agent(file: &InspectedFile, color: Color) -> std::result::Result<Agent, String> {
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
    Ok(Agent {
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
        skills: Vec::new(),
        connections: nested_toml_tables(&file.content, "mcp_servers"),
        write_results_to: String::new(),
        vendor_options: VendorOptions::new(),
    })
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

fn connections(file: &InspectedFile) -> std::result::Result<Vec<Connection>, String> {
    match file.item.source {
        SourceKind::Claude => claude_connections(file),
        SourceKind::Codex => codex_connections(file),
        _ => Err("This connection format is not supported.".to_owned()),
    }
}

fn claude_connections(file: &InspectedFile) -> std::result::Result<Vec<Connection>, String> {
    let document: Value = serde_json::from_str(&file.content)
        .map_err(|error| format!("This project connection file is not valid JSON: {error}"))?;
    let servers = document
        .get("mcpServers")
        .and_then(Value::as_object)
        .ok_or_else(|| "This file does not contain project connections.".to_owned())?;
    let mut out = Vec::new();
    for (name, server) in servers {
        let object = server
            .as_object()
            .ok_or_else(|| format!("Connection {name} is not an object."))?;
        let transport = if let Some(url) = object.get("url").and_then(Value::as_str) {
            if !url.starts_with("https://") {
                return Err(format!("Connection {name} must use HTTPS."));
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
        out.push(Connection::imported(
            slug(name),
            name.clone(),
            transport,
            file.item.path.clone(),
            file.item.hash.clone(),
        ));
    }
    Ok(out)
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
            if !url.starts_with("https://") {
                return Err(format!("Connection {name} must use HTTPS."));
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
    for line in rest[..end].lines() {
        if line.starts_with(' ') || line.trim().is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            fields.insert(key.trim().to_owned(), unquote(value.trim()));
        }
    }
    Ok((fields, rest[end + 4..].trim_start_matches('\n')))
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

fn nested_names(content: &str, key: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut inside = false;
    for line in content.lines() {
        if line.trim() == format!("{key}:") {
            inside = true;
            continue;
        }
        if inside {
            let spaces = line.len() - line.trim_start().len();
            if spaces == 0 && !line.trim().is_empty() {
                break;
            }
            if spaces >= 2
                && let Some((name, _)) = line.trim().split_once(':')
                && !name.is_empty()
                && !["command", "args", "env", "url"].contains(&name)
            {
                names.push(name.to_owned());
            }
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
}
