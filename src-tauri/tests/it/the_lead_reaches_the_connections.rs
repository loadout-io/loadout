//! AC-5 dla T-97: rozmowa z Leadem dostaje zatwierdzone Connections — u obu vendorów.
//!
//! # Po co to istnieje
//!
//! Krok biegu składa fragment argv z zatwierdzonych Connections i podaje go sterownikowi
//! (`Live::vendor_arguments_for` → `AgentDriver::configured`). Rozmowa z Leadem wołała na
//! sterowniku **wyłącznie** `with_evidence`, a `configured` widziała jedynie szczebel „ile
//! myśleć" — więc lider z agentem mającym `connections: ["x"]` rozmawiał **bez ani jednego
//! serwera**, u każdego vendora. Człowiek zatwierdził połączenie, ekran je pokazuje, agent go
//! nie ma, i nic tego nie mówi: to jest kontrolka bez skutku (niezmiennik 16) schowana o warstwę
//! głębiej.
//!
//! # Kolejność opakowań jest treścią, nie stylem
//!
//! Każde opakowanie oddaje **klon** sterownika, więc opakowanie założone wcześniej ginie, jeśli
//! późniejsze klonuje sterownik sprzed niego. `Live::run_agent` ma tę kolejność zapisaną wprost:
//! Connections → dziedziczenie → dowody. Odwrócenie kompiluje się, rozmowa rusza, a znika albo
//! `--mcp-config`, albo plik dowodu — i nie widać tego po niczym. Dlatego asercja niżej pyta
//! o **oba naraz** na tym sterowniku, który naprawdę poszedł do `start_conversation`.
//!
//! # Trzy asercje, bo dwie kłamią
//!
//! (a) Claude dostaje `--mcp-config` ze ścieżką pliku, który **istnieje**. Sama flaga bez pliku
//!     jest argumentem, na którym CLI się wywraca — czyli liderem, który przestaje rozmawiać.
//!
//! (b) Codex dostaje `-c mcp_servers.…` i dostaje je **przed podkomendą**, także na ścieżce
//!     `app-server`. Opcje globalne postawione po podkomendzie nie są opcjami globalnymi;
//!     `exec_argv` składa je w tej samej kolejności i z tego samego powodu.
//!
//! (c) A agent **bez** połączeń ma argv co do bajtu takie jak dziś. Bez tej asercji zieleń
//!     przechodzi dla implementacji, która dokłada pustą flagę każdej rozmowie — a pusta flaga
//!     połyka następny argument jako swoją wartość.
//!
//! # Słaba wersja tego kryterium
//!
//! `assert!(arguments.iter().any(|a| a.contains("mcp")))` na sterowniku oddanym przez
//! `configured`. Przechodzi dla implementacji, która składa fragment i **gubi go** przy
//! nakładaniu dowodów — czyli dla tej, w której rozmowa startuje dokładnie tak jak dziś.
//! Rozróżnia to pytanie zadane sterownikowi, który dostał `start_conversation`.

// `expect()`/`unwrap()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam
// powód, co w `live_chat_goes_through_the_registry` i w pozostałych plikach tego celu.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::Drivers;
use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::engine::drivers::codex::app_server_argv;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, DriverConfiguration, FinishReason,
    Outcome as TurnOutcome, Probe, RunSpec, SessionRef, Tokens, ValidatedImages,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::{AppState, LineSource, line_channel};
use loadout_lib::library::agents::Agent;
use loadout_lib::store::Store;
use tempfile::TempDir;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Ile miejsca w strumieniu linii. Z zapasem — mierzymy drogę, nie przepustowość.
const LINES: usize = 32;

/// Terminal, w którym stoi rozmowa.
const TERMINAL: &str = "terminal-1";

/// Nazwa zatwierdzonego połączenia. Ta sama w pliku biblioteki i w definicji lidera.
const SERVER: &str = "x";

/// Komenda, którą to połączenie uruchamia. Wchodzi w `-c mcp_servers.x.command=…`, więc jest
/// tym, po czym poznajemy, że dojechało NIE PUSTE.
const SERVER_COMMAND: &str = "x-server";

/// Zatwierdzone połączenie tak, jak leży w bibliotece po imporcie.
fn connection_file() -> String {
    format!(
        r#"{{
  "id": "{SERVER}",
  "name": "{SERVER}",
  "enabled": true,
  "transport": {{ "kind": "stdio", "command": "{SERVER_COMMAND}", "args": ["--stdio"], "environment": [] }},
  "source": "/tmp/mcp.json",
  "sourceHash": "0000",
  "origin": "project"
}}
"#
    )
}

#[tokio::test]
async fn a_lead_with_a_connection_reaches_claude_with_the_file_of_servers()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let started = bench.one_sentence("claude", &[SERVER.to_owned()]).await?;

    let arguments = started.arguments;
    let at = arguments
        .iter()
        .position(|argument| argument == "--mcp-config")
        .ok_or_else(|| {
            format!(
                "the lead's agent has an approved connection and the conversation started \
                 without a file of servers: {arguments:?}. The human approved it, the screen \
                 shows it, and the agent never had it - which from the outside is the same as \
                 an agent that chose not to use it"
            )
        })?;
    let path = arguments.get(at + 1).ok_or_else(|| {
        format!(
            "--mcp-config came through with nothing behind it: {arguments:?}. A flag with an \
                 empty value swallows the next argument as its own"
        )
    })?;
    assert!(
        Path::new(path).is_file(),
        "the conversation was handed {path:?} as its file of servers and no such file exists. \
         The CLI stops on that argument, so the lead simply stops answering"
    );

    // KOLEJNOŚĆ OPAKOWAŃ, ZMIERZONA NA STEROWNIKU, KTÓRY NAPRAWDĘ POSZEDŁ DO ROZMOWY. Każde
    // opakowanie oddaje klon, więc to jedno pytanie rozstrzyga, czy któreś z nich nie zginęło.
    assert!(
        started.evidence,
        "the driver that started the conversation carries the servers and not its private \
         receipt. Each wrapper hands back a CLONE, so whichever goes on first is lost when the \
         next one clones the driver from before it - and nothing about that is visible"
    );

    Ok(())
}

#[tokio::test]
async fn a_lead_with_a_connection_reaches_codex_with_the_same_servers() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let started = bench.one_sentence("codex", &[SERVER.to_owned()]).await?;

    let arguments = started.arguments;
    let carried = arguments
        .iter()
        .any(|argument| argument.starts_with(&format!("mcp_servers.{SERVER}.")));
    assert!(
        carried,
        "this vendor takes its servers as global options, and the conversation started without \
         a single one: {arguments:?}"
    );
    assert!(
        arguments
            .iter()
            .any(|argument| argument.contains(SERVER_COMMAND)),
        "the option reached the conversation without naming what to run ({SERVER_COMMAND}), so \
         the server would never come up: {arguments:?}"
    );
    assert!(
        started.evidence,
        "the driver that started the conversation carries the servers and not its private \
         receipt - see the same assertion on the other vendor for why both have to be there"
    );

    Ok(())
}

#[test]
fn the_options_stand_before_the_subcommand_on_the_app_server_road() {
    let configuration = DriverConfiguration {
        arguments: vec![
            "-c".to_owned(),
            format!("mcp_servers.{SERVER}.command=\"{SERVER_COMMAND}\""),
        ],
        ..DriverConfiguration::default()
    };
    let argv = app_server_argv(&configuration);

    let subcommand = argv
        .iter()
        .position(|argument| argument == "app-server")
        .expect("the app server road has to still start the app server; it came out as {argv:?}");
    let option = argv
        .iter()
        .position(|argument| argument == "-c")
        .unwrap_or(usize::MAX);
    assert!(
        option < subcommand,
        "these are GLOBAL options and global options stand before the subcommand - the same \
         place they stand before exec. Behind it they are read as arguments of the subcommand, \
         so the servers quietly never arrive. It came out as {argv:?}"
    );
}

#[test]
fn a_conversation_without_connections_starts_exactly_as_it_does_today() {
    let argv = app_server_argv(&DriverConfiguration::default());
    assert_eq!(
        argv,
        vec![
            "app-server".to_owned(),
            "--listen".to_owned(),
            "stdio://".to_owned(),
        ],
        "an agent with no connections has to start byte for byte the way it starts today. A flag \
         added to every conversation 'just in case' is a flag with an empty value, and that one \
         swallows the next argument as its own"
    );
}

#[tokio::test]
async fn a_lead_without_connections_is_handed_nothing_extra() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let started = bench.one_sentence("codex", &[]).await?;
    assert!(
        !started
            .arguments
            .iter()
            .any(|argument| argument.contains("mcp_servers")),
        "this lead's agent has no connections at all, and the conversation was handed servers \
         anyway: {:?}",
        started.arguments
    );
    Ok(())
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

/// Co dubler zapamiętał o sterowniku, który NAPRAWDĘ zaczął rozmowę.
#[derive(Debug, Clone, Default)]
struct Started {
    /// Fragment argv, który ten sterownik niósł w chwili startu rozmowy.
    arguments: Vec<String>,
    /// Czy ten sam sterownik niósł też prywatny receipt tej rozmowy.
    evidence: bool,
}

#[derive(Debug, Default)]
struct Watch(Mutex<Option<Started>>);

fn lock<T>(what: &Mutex<T>) -> MutexGuard<'_, T> {
    what.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Dubler jednego vendora. `id()` rozstrzyga, którą postać przyjmie fragment argv — a to jest
/// jedyny powód, dla którego ten plik ma dwa wywołania zamiast jednego.
#[derive(Debug, Clone)]
struct Fake {
    watch: Arc<Watch>,
    vendor: &'static str,
    configuration: DriverConfiguration,
    evidence: bool,
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        self.vendor
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some(self.vendor.to_owned()),
        })
    }

    fn configured(&self, configuration: &DriverConfiguration) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            configuration: configuration.clone(),
            ..self.clone()
        }))
    }

    fn with_evidence(&self, _target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            evidence: true,
            ..self.clone()
        }))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        // TO JEST PUNKT POMIARU: sterownik, który dojechał tutaj, jest tym, który naprawdę
        // rozmawia. O opakowania pytamy jego, a nie tego, który wyszedł z fabryki.
        *lock(&self.watch.0) = Some(Started {
            arguments: self.configuration.arguments.clone(),
            evidence: self.evidence,
        });

        let session = SessionRef {
            vendor: self.vendor,
            id: spec.run_id.to_string(),
        };
        let _ = events
            .send(
                (AgentEvent::Started {
                    session: session.clone(),
                    model: spec.model.clone().unwrap_or_default(),
                    tools: Vec::new(),
                    capabilities: Vec::new(),
                })
                .into(),
            )
            .await;
        Ok(Box::new(Turn { events, session }))
    }

    async fn start_conversation(
        &self,
        spec: RunSpec,
        _images: ValidatedImages,
        tx: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.start(spec, tx).await
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
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
            text: "here is what I would do".to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::ZERO,
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

// ── ławka ──────────────────────────────────────────────────────────────────────────────────

struct Bench {
    home: TempDir,
    project: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        // `Store::open` zakłada plik bazy, ale nie katalog nad nim.
        fs::create_dir_all(project.path().join(".loadout"))?;
        let connections = home.path().join("connections");
        fs::create_dir_all(&connections)?;
        fs::write(connections.join("x.json"), connection_file())?;
        Ok(Self { home, project })
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }

    fn folder(&self) -> String {
        self.project.path().to_string_lossy().into_owned()
    }

    /// Zapisuje lidera przez produkcyjną drogę i oddaje jego identyfikator.
    fn saved_lead(&self, connections: &[String]) -> Result<String, Box<dyn Error>> {
        let agent = Agent {
            id: Uuid::from_u128(97),
            name: "Lead".to_owned(),
            connections: connections.to_vec(),
            ..Agent::example()
        };
        save_agent_inner(self.home.path(), &agent)?;
        Ok(agent.id.to_string())
    }

    /// Jedno zdanie do lidera i to, co z niego zobaczył sterownik.
    async fn one_sentence(
        &self,
        vendor: &'static str,
        connections: &[String],
    ) -> Result<Started, Box<dyn Error>> {
        let who = self.saved_lead(connections)?;
        let folder = self.folder();
        let watch = Arc::new(Watch::default());
        let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
            watch: Arc::clone(&watch),
            vendor,
            configuration: DriverConfiguration::default(),
            evidence: false,
        });
        let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));

        let store = Store::open(&self.db())?;
        let state = AppState::new(
            self.home.path().to_path_buf(),
            self.project.path().to_path_buf(),
            store,
            drivers,
        );
        let _watching: LineSource = {
            let (sink, source) = line_channel(LINES);
            state.watching_the_lead(TERMINAL, Some(&folder), sink)?;
            source
        };

        state
            .say_to_the_lead(
                TERMINAL,
                Some(&folder),
                Some(&who),
                "what should the checker look at?",
            )
            .await
            .map_err(|said| format!("the sentence to the lead was turned down: {said}"))?;

        lock(&watch.0)
            .clone()
            .ok_or_else(|| "the conversation never reached a driver at all".into())
    }
}
