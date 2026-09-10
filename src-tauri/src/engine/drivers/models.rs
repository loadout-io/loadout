//! 2026-09-10: `gpt-6` odrzucone dopiero w ostatnim kroku. Katalog pochodzi z CLI,
//! nie z numerów wersji zaszytych w Loadout. Sonda nigdy nie wysyła tury użytkownika.
use super::DriverConfiguration;
use crate::engine::supervisor::{self, GroupProof, StdinPlan};
use anyhow::{Context, bail};
use serde::Serialize;
use serde_json::{Value, json};
use std::{path::Path, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelChoice {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub is_default: bool,
    pub hidden: bool,
    pub aliases: Vec<String>,
    pub efforts: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalog {
    pub models: Vec<ModelChoice>,
    pub configured_default: Option<String>,
}
impl ModelCatalog {
    pub fn check(&self, model: Option<&str>, effort: &str) -> anyhow::Result<()> {
        let model = model
            .filter(|m| !m.trim().is_empty())
            .or(self.configured_default.as_deref());
        let choice = match model.filter(|m| !m.trim().is_empty()) {
            Some(id) => self
                .models
                .iter()
                .find(|m| m.id == id || m.aliases.iter().any(|a| a == id)),
            None => self.models.iter().find(|m| m.is_default),
        };
        let Some(choice) = choice else {
            bail!(
                "Model ‘{}’ is not offered by this app. Refresh models and choose one from the list.",
                model.unwrap_or("default")
            );
        };
        if !choice.efforts.is_empty() && !choice.efforts.iter().any(|e| e == effort) {
            bail!(
                "Model ‘{}’ does not support the selected thinking level. Choose a different thinking level or model.",
                choice.id
            );
        }
        Ok(())
    }
}

pub fn parse_models(rows: &[Value], codex: bool) -> anyhow::Result<ModelCatalog> {
    let mut models = Vec::new();
    for row in rows {
        let Some(id) = row
            .get(if codex { "model" } else { "value" })
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let mut aliases = Vec::new();
        if !codex {
            // Importowane definicje Claude używają inherit: sterownik pomija wtedy --model.
            if id == "default" {
                aliases.push("inherit".to_owned());
            }
            if let Some(resolved) = row.get("resolvedModel").and_then(Value::as_str) {
                aliases.push(resolved.to_owned());
                // CLI publikuje wariant [1m], ale akceptuje też alias bez rozmiaru kontekstu.
                if let Some(base) = resolved.strip_suffix("[1m]") {
                    aliases.push(base.to_owned());
                }
            }
            if let Some(base) = id.strip_suffix("[1m]") {
                aliases.push(base.to_owned());
            }
        }
        let efforts = row
            .get(if codex {
                "supportedReasoningEfforts"
            } else {
                "supportedEffortLevels"
            })
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| {
                if codex {
                    v.get("reasoningEffort").and_then(Value::as_str)
                } else {
                    v.as_str()
                }
            })
            .map(str::to_owned)
            .collect();
        models.push(ModelChoice {
            id: id.to_owned(),
            display_name: row
                .get("displayName")
                .and_then(Value::as_str)
                .unwrap_or(id)
                .to_owned(),
            description: row
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            is_default: if codex {
                row.get("isDefault")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            } else {
                id == "default"
            },
            hidden: row.get("hidden").and_then(Value::as_bool).unwrap_or(false),
            aliases,
            efforts,
        });
    }
    if models.is_empty() {
        bail!("The app returned no models. Check sign-in and refresh models.");
    }
    Ok(ModelCatalog {
        models,
        configured_default: None,
    })
}

pub async fn discover(
    binary: &Path,
    configuration: &DriverConfiguration,
    codex: bool,
) -> anyhow::Result<ModelCatalog> {
    let mut command = Command::new(binary);
    command.current_dir(std::env::temp_dir());
    if codex {
        command.arg("app-server");
    } else {
        command.args([
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--strict-mcp-config",
            "--mcp-config",
            "{\"mcpServers\":{}}",
            "--setting-sources",
            "user",
            "--tools",
            "",
        ]);
    }
    let mut process = supervisor::spawn_with_environment(
        command,
        StdinPlan::Keep(String::new()),
        &configuration.environment,
    )?;
    let mut input = process
        .stdin()
        .await
        .context("The app did not open its input.")?;
    let output = process
        .stdout()
        .context("The app did not open its output.")?;
    let mut errors = process
        .stderr()
        .context("The app did not open its error output.")?;
    let drain =
        tokio::spawn(async move { tokio::io::copy(&mut errors, &mut tokio::io::sink()).await });
    let result =
        tokio::time::timeout(Duration::from_secs(20), query(&mut input, output, codex)).await;
    drop(input);
    // Jak sonda wersji: Alive wymaga dalszego sprzątania, nie oddania żywego procesu.
    while !matches!(
        process.stop(supervisor::DEFAULT_GRACE).await,
        GroupProof::Dead { .. }
    ) {}
    drain.abort();
    let _ = drain.await;
    result.context("The model check timed out. Check sign-in and refresh models.")?
}
async fn send(input: &mut tokio::process::ChildStdin, value: Value) -> anyhow::Result<()> {
    input.write_all(format!("{value}\n").as_bytes()).await?;
    input.flush().await?;
    Ok(())
}

async fn query(
    input: &mut tokio::process::ChildStdin,
    output: tokio::process::ChildStdout,
    codex: bool,
) -> anyhow::Result<ModelCatalog> {
    let hello = if codex {
        json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"loadout","version":env!("CARGO_PKG_VERSION")}}})
    } else {
        json!({"type":"control_request","request_id":"models","request":{"subtype":"initialize"}})
    };
    send(input, hello).await?;
    let mut output = BufReader::new(output);
    let mut all = Vec::new();
    let mut request_id = 3u64;
    let mut configured_default = None;
    let mut cursors = std::collections::HashSet::new();
    loop {
        // Limit pojedynczej linii chroni pamięć również przy uszkodzonym CLI.
        let mut bytes = Vec::new();
        let n = (&mut output)
            .take(1024 * 1024)
            .read_until(b'\n', &mut bytes)
            .await?;
        if n == 0 {
            bail!("The app closed before returning its models.");
        }
        if n >= 1024 * 1024 {
            bail!("The model response was too large.");
        }
        let Ok(v) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        if codex {
            if v.get("id").and_then(Value::as_u64) == Some(1) {
                if v.get("error").is_some() {
                    bail!("Codex could not initialize its model list.");
                }
                send(input, json!({"method":"initialized"})).await?;
                send(
                    input,
                    json!({"id":2,"method":"config/read","params":{"includeLayers":false}}),
                )
                .await?;
            } else if v.get("id").and_then(Value::as_u64) == Some(2) {
                let result = v
                    .get("result")
                    .context("Codex could not read its default model.")?;
                configured_default = result
                    .pointer("/config/model")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                send(input, json!({"id":request_id,"method":"model/list","params":{"limit":100,"includeHidden":true}})).await?;
            } else if v.get("id").and_then(Value::as_u64) == Some(request_id) {
                let result = v
                    .get("result")
                    .context("Codex could not return its models. Check sign-in and try again.")?;
                let rows = result
                    .get("data")
                    .and_then(Value::as_array)
                    .context("Codex returned an unreadable model list.")?;
                all.extend(rows.iter().cloned());
                if let Some(cursor) = result.get("nextCursor").and_then(Value::as_str) {
                    if !cursors.insert(cursor.to_owned()) || all.len() > 10000 {
                        bail!("Codex repeated its model list.");
                    }
                    request_id += 1;
                    send(input, json!({"id":request_id,"method":"model/list","params":{"limit":100,"includeHidden":true,"cursor":cursor}})).await?;
                } else {
                    let mut catalog = parse_models(&all, true)?;
                    catalog.configured_default = configured_default;
                    return Ok(catalog);
                }
            }
        } else if v.get("type").and_then(Value::as_str) == Some("control_response")
            && v.pointer("/response/request_id").and_then(Value::as_str) == Some("models")
        {
            let rows = v
                .pointer("/response/response/models")
                .and_then(Value::as_array)
                .context(
                    "Claude Code could not return its models. Check sign-in and update the app.",
                )?;
            return parse_models(rows, false);
        }
    }
}
