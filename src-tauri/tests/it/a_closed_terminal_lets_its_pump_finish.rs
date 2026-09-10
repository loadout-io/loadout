//! Zamknięta karta oddaje kanał linii, więc jej pompa może się skończyć.
//!
//! Słaba wersja tego kryterium pytałaby `Threads` zbudowane w teście. Ten przypadek idzie przez
//! `AppState::watching_the_lead` i `AppState::close_the_lead`, czyli te same dwie czynności, które
//! opakowują komendy okna. Nadajnik jest oddany rejestrowi bez kopii: koniec pompy dowodzi więc,
//! że zamknięcie zdjęło ostatniego właściciela kanału, a nie tylko zgubiło lokalny uchwyt.

use std::error::Error;
use std::fs;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::Drivers;
use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens, Voice,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{AppState, line_channel, spawn_pump};
use loadout_lib::library::agents::Agent;
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

/// Identyfikator dokładnie w kształcie wybijanym przez pasek terminali.
const TERMINAL: &str = "terminal-1";

/// Kanał ma zapas; kryterium mierzy jego życie, nie przepustowość.
const LINES: usize = 64;

#[derive(Debug)]
struct Fake;

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        "fake"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("fake".to_owned()),
        })
    }

    async fn start(
        &self,
        _spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let session = SessionRef {
            vendor: "fake",
            id: "terminal-close".to_owned(),
        };
        /* 2026-09: odbiornik głosu żyje razem z dublerem. Porzucenie go tutaj mierzyłoby
         * zamknięty kanał dublera zamiast zamknięcia terminalu przez produkcyjną drogę. */
        let (voice, mut heard) = mpsc::channel(4);
        tokio::spawn(async move { while heard.recv().await.is_some() {} });
        Ok(Box::new(Turn {
            events,
            session,
            voice,
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    voice: Voice,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn voice(&self) -> Option<Voice> {
        Some(self.voice.clone())
    }

    fn group(&self) -> Option<GroupId> {
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: String::new(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send((AgentEvent::Finished(outcome.clone())).into())
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

fn fake_drivers() -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake);
    Arc::new(move |_| Arc::clone(&driver))
}

#[tokio::test]
async fn a_terminal_that_was_only_watched_lets_its_pump_finish() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let project = TempDir::new()?;
    fs::create_dir_all(project.path().join(".loadout"))?;
    let store = Store::open(&project.path().join(".loadout").join("loadout.db"))?;
    /* 2026-09: ten przypadek tylko otwiera widok, więc dotknięcie fabryki oznaczałoby, że
     * fikstura uruchomiła płatną rozmowę, której człowiek nie zaczął. */
    let drivers: Drivers =
        Arc::new(|_| unreachable!("watching a terminal must not start an agent"));
    let state = AppState::new(
        home.path().to_path_buf(),
        project.path().to_path_buf(),
        store,
        drivers,
    );
    let (sink, source) = line_channel(LINES);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));

    state
        .watching_the_lead(TERMINAL, Some(&project.path().to_string_lossy()), sink)
        .await?;
    state.close_the_lead(TERMINAL).await;

    let finished = timeout(Duration::from_secs(2), pump).await;
    assert!(
        finished.is_ok(),
        "closing a terminal that nobody spoke in left its line sender in the registry. The pump \
         still wakes once per tick although its card is gone"
    );
    Ok(())
}

#[tokio::test]
async fn a_terminal_that_spoke_lets_its_pump_finish() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let project = TempDir::new()?;
    fs::create_dir_all(project.path().join(".loadout"))?;
    let lead = Agent {
        id: Uuid::from_u128(26),
        name: "Lead".to_owned(),
        ..Agent::example()
    };
    save_agent_inner(&project.path().join(".loadout"), &lead, None)?;
    let store = Store::open(&project.path().join(".loadout").join("loadout.db"))?;
    let state = AppState::new(
        home.path().to_path_buf(),
        project.path().to_path_buf(),
        store,
        fake_drivers(),
    );
    let (sink, source) = line_channel(LINES);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let folder = project.path().to_string_lossy();

    state
        .watching_the_lead(TERMINAL, Some(&folder), sink)
        .await?;
    state
        .say_to_the_lead(
            TERMINAL,
            Some(&folder),
            Some(&lead.id.to_string()),
            "What should happen next?",
        )
        .await?;
    state.close_the_lead(TERMINAL).await;

    let finished = timeout(Duration::from_secs(2), pump).await;
    assert!(
        finished.is_ok(),
        "closing a terminal after a conversation ended its actor but kept the line sender. The \
         pump still wakes once per tick although its card is gone"
    );
    Ok(())
}
