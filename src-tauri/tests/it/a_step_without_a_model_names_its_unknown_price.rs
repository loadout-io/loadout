//! Z-12: krok Codeksa bez nazwy modelu mówi człowiekowi, że jego ceny nie znamy.
//!
//! Wyrocznia przeprowadza zdarzenia prawdziwego adaptera przez produkcyjnego kuratora i czyta
//! zserializowany wiersz `Done`. Sama wartość `cost_usd: None` nie dowodzi, że zdanie dotarło
//! tam, gdzie człowiek je widzi (niezmiennik 29).

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::{AgentHandle, Policy, RunSpec};
use loadout_lib::engine::line::{Curator, LineKind, Seen};
use serde_json::Value;
use tempfile::TempDir;
use tokio::sync::mpsc;
use uuid::Uuid;

const PATIENCE: Duration = Duration::from_secs(30);
const UNKNOWN_PRICE: &str = "The price for the model this step used is not known.";

const CODEX_CLI: &str = r#"#!/bin/sh
printf '%s\n' '{"type":"thread.started","thread_id":"thread-z12-no-model"}'
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","id":"item-1","text":"Done."}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":10000,"cached_input_tokens":5000,"output_tokens":20000}}'
exit 0
"#;

fn write_cli(dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join("codex-z12-no-model");
    fs::write(&path, CODEX_CLI)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn spec(cwd: &Path) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: "finish without naming the model".to_owned(),
        model: None,
        system_append: None,
        reaches_the_web: false,
        policy: Policy::ReadOnly,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_final_row_says_the_price_is_unknown_even_without_a_model() -> Result<(), Box<dyn Error>>
{
    let dir = TempDir::new()?;
    let driver = CodexDriver::with_binary(write_cli(dir.path())?);
    let (tx, mut rx) = mpsc::channel(64);
    let mut handle =
        tokio::time::timeout(PATIENCE, driver.start_session(spec(dir.path()), tx)).await??;
    let outcome = tokio::time::timeout(PATIENCE, handle.wait()).await??;
    drop(handle);
    assert_eq!(
        outcome.cost_usd, None,
        "an unnamed model has no known price"
    );

    let mut decoded = Vec::new();
    while let Ok(event) = rx.try_recv() {
        decoded.push(event);
    }
    let mut curator = Curator::new();
    let mut lines = Vec::new();
    for (at_ms, event) in decoded.iter().enumerate() {
        lines.extend(curator.observe(Seen {
            agent: "Unnamed model",
            at_ms: u64::try_from(at_ms).unwrap_or_default(),
            event: &event.event,
            tool: None,
        }));
    }
    lines.extend(curator.flush());

    let row = lines
        .iter()
        .find(|line| line.kind() == LineKind::Done)
        .ok_or("the real adapter stream produced no final row for the UI")?;
    let wire = serde_json::to_value(row)?;
    let text = wire
        .get("text")
        .and_then(Value::as_str)
        .ok_or("the serialized final row has no sentence a person can read")?;
    assert!(
        text.contains(UNKNOWN_PRICE),
        "the final row hid the unknown price for an unnamed model. It said {text:?}; the full \
         serialized row was {wire}"
    );
    Ok(())
}
