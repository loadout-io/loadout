//! Strażnik z 2026-09-13: włączone połączenia biblioteki są JEDNĄ listą (niezmienniki 13 i 23).
//!
//! # Po co to istnieje
//!
//! Tej samej listy pytają dziś trzy miejsca: nowy agent z `＋ Create` i trzej agenci z pierwszego
//! ekranu startują z nią (`list_connections`), generator agenta z opisu proponuje ją modelowi,
//! a Start przyjmuje wyłącznie jej nazwy (`runtime::selected` odmawia nazwy nieznanej
//! i połączenia wyłączonego). Do tego dnia generator dostawał `runtime::all(…)` po `id` — więc
//! proponował także połączenia WYŁĄCZONE, i to pod identyfikatorem zamiast nazwy. Szkic prosił
//! wtedy o `playwright`, dostawał go, a Start takiego agenta odmawiał zdaniem „Connection
//! playwright is not enabled in Loadout.", już po zapisie.
//!
//! # Dlaczego id różny od nazwy
//!
//! Połączenie z `id` równym nazwie nie odróżnia listy nazw od listy identyfikatorów, bo
//! `selected()` przyjmuje jedno i drugie. `Figma Design` pod `figma-design` jest więc całą
//! treścią fikstury, a nie ozdobą.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::Drivers;
use loadout_lib::connections::runtime::{enabled_names, selected};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::AppState;
use loadout_lib::library::agents::Vendor;
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Nazwa włączonego połączenia. Różna od jego `id` — powód w nagłówku pliku.
const ON: &str = "Figma Design";

/// Połączenie wyłączone. Tu `id` i nazwa są jednym napisem, bo sądzimy włączenie, nie pisownię.
const OFF: &str = "playwright";

/// Jeden plik połączenia w kształcie, w jakim zostawia go import.
fn connection(id: &str, name: &str, enabled: bool) -> String {
    json!({
        "id": id,
        "name": name,
        "enabled": enabled,
        "transport": { "kind": "stdio", "command": "server", "args": [], "environment": [] },
        "source": "/tmp/mcp.json",
        "sourceHash": "0000",
        "origin": "project"
    })
    .to_string()
}

/// Biblioteka projektu z jednym połączeniem włączonym i jednym wyłączonym.
fn library_in(project: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let connections = project.join(".loadout").join("connections");
    fs::create_dir_all(&connections)?;
    fs::write(
        connections.join("figma-design.json"),
        connection("figma-design", ON, true),
    )?;
    fs::write(
        connections.join("playwright.json"),
        connection(OFF, OFF, false),
    )?;
    Ok(connections)
}

#[test]
fn only_the_connections_turned_on_are_listed_and_start_takes_every_one_of_them()
-> Result<(), Box<dyn Error>> {
    let project = TempDir::new()?;
    let connections = library_in(project.path())?;

    let names = enabled_names(&connections)?;
    assert_eq!(
        names,
        vec![ON.to_owned()],
        "a new agent starts with exactly this list, so a connection turned off in it is an agent \
         that Start refuses, and an id instead of a name is a word the person never chose"
    );
    selected(&connections, &names).map_err(|why| {
        format!(
            "every name a new agent starts with has to be one Start takes, and this one was \
             refused: {why}"
        )
    })?;
    Ok(())
}

#[tokio::test]
async fn the_generator_is_offered_only_the_names_of_the_connections_turned_on()
-> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let project = TempDir::new()?;
    library_in(project.path())?;

    let driver: Arc<dyn AgentDriver> = Arc::new(Writer {
        answer: json!({
            "name": "Designer",
            "summary": "Reads the design",
            "instructions": "Read the frames before anything else.",
            "connections": [ON, OFF]
        })
        .to_string(),
    });
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));
    let store = Store::open(&project.path().join(".loadout").join("loadout.db"))?;
    let state = AppState::new(
        home.path().to_path_buf(),
        project.path().to_path_buf(),
        store,
        drivers,
    );

    let draft = state
        .generate_agent_in(
            project.path(),
            "connections-one-list",
            "A designer that reads the frames",
            Vendor::ClaudeCode,
        )
        .await?;

    assert_eq!(
        draft["agent"]["connections"],
        json!([ON]),
        "the draft kept a connection that is turned off, or dropped the one that is on — the \
         generator was offered a different list than the one Start takes: {draft}"
    );
    let missing: Vec<&str> = draft["missing"]
        .as_array()
        .map(|all| all.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    assert!(
        missing.iter().any(|one| one.contains(OFF)),
        "the draft asked for {OFF}, which is turned off here, and the note before saving never \
         says it is not available: {missing:?}"
    );
    assert!(
        !missing.iter().any(|one| one.contains(ON)),
        "{ON} is turned on here and the note before saving calls it not available: {missing:?}"
    );
    Ok(())
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

/// Vendor, który na każdą prośbę oddaje ten sam szkic. Walidacja i dopasowanie są produkcyjne.
struct Writer {
    answer: String,
}

#[async_trait]
impl AgentDriver for Writer {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: None,
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        Ok(Box::new(Wrote {
            text: self.answer.clone(),
            events,
            session: SessionRef {
                vendor: "claude-code",
                id: spec.run_id.to_string(),
            },
        }))
    }
}

struct Wrote {
    text: String,
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for Wrote {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        None
    }

    async fn send(&mut self, _: String) -> anyhow::Result<()> {
        anyhow::bail!("generation is one turn")
    }

    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        let outcome = Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text: self.text.clone(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send(AgentEvent::Finished(outcome.clone()).into())
            .await;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}
