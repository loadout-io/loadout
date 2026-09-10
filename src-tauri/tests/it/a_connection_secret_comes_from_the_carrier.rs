//! Z-23: sekret zatwierdzonego Połączenia dojeżdża z nośnika Loadouta, kiedy okno wstało
//! bez środowiska.
//!
//! # Co to mierzy
//!
//! Audyt 2026-09-02 (C-6). Sekrety Połączeń miały do dziś JEDNO źródło — środowisko procesu
//! okna (`std::env::var_os` w `commands::run` i `commands::chat`). Aplikacja uruchomiona
//! z Docka nie dostaje niczego, co człowiek wyeksportował w swojej powłoce: `launchd` daje jej
//! kilkanaście zmiennych i ani jednego klucza. Efekt jest taki, że to samo zatwierdzone
//! Połączenie działa, gdy Loadout wstał z terminala, i odmawia, gdy wstał z ikony — a nic na
//! ekranie nie mówi, na czym polega różnica.
//!
//! Nośnikiem jest plik `~/.loadout/env` (`<biblioteka>/env`), czytany WYŁĄCZNIE przez resolver
//! i wyłącznie wtedy, gdy nazwy nie ma w środowisku okna.
//!
//! # Cztery rzeczy, i żadna z nich nie wystarcza sama
//!
//! * (a) rozmowa z liderem, którego agent ma to Połączenie, **rusza** i dostaje wartość
//!   z nośnika — bo to jest cała funkcja;
//! * (b) odmowa mówi, SKĄD Loadout tej wartości szukał. Odmowa „nie jest ustawiona" zostawia
//!   człowieka z pytaniem „ustawiona gdzie?", a odpowiedź zależy od tego, jak wstała aplikacja;
//! * (c) nośnik czytelny dla innych kont NIE jest używany — plik z kluczami do wszystkich
//!   narzędzi człowieka jest wart dokładnie tyle, ile jego prawa dostępu;
//! * (d) ta sama droga działa w BIEGU, nie tylko w rozmowie. To są dwa osobne wywołania
//!   resolvera (`commands::chat::connections_of` i `commands::run::Live::vendor_arguments_for`),
//!   więc naprawa jednego z nich zostawia drugi dokładnie tam, gdzie był;
//! * (e) wartość, której **nie ma nigdzie**, nie zatrzymuje serwera stdio pod Codeksem i nie
//!   tworzy pustego nadpisania. To jest druga strona decyzji z 2026-09-04: skoro ta jedna droga
//!   podaje wartość nadpisaniem w argv, to jej brak znaczy tyle, co przed tym zadaniem — nie ma
//!   czego nadpisać, a serwer sam powie, czego mu brak.
//! # Słabą wersją tego kryterium jest pytanie o funkcję resolvera
//!
//! `resolve(carrier, name) == Some(value)` przechodzi dla drzewa, w którym ani bieg, ani
//! rozmowa tej funkcji nie wołają — czyli dla dzisiejszego (niezmiennik 29). Dlatego wszystkie
//! asercje niżej idą przez tę samą komendę, której używa okno.
//!
//! # Gdzie stoi druga połowa tego zadania
//!
//! Że wartość dociera do serwera stdio **pod Codeksem**, dowodzi
//! `codex_lead_curates_mcp_servers::private_servers_are_false_and_the_approved_connection_is_true`:
//! nośnik 0600 przy pustej zmiennej procesu, a w argv App Servera dokładne nadpisanie
//! `mcp_servers.<nazwa>.env.<ZMIENNA>`. Tam też stoi zawężona wyrocznia niezmiennika 9 — wartość
//! wolno zobaczyć w argv dokładnie raz i tylko pod tym kluczem (wyjątek otwarty decyzją
//! właściciela 2026-09-04). Tutaj sądzimy drugą stronę tej samej decyzji: (e) niżej.

// `unwrap()`/`expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam
// powód, co w `the_lead_reaches_the_connections`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::commands::processes::Processes;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::connections::runtime;
use loadout_lib::connections::secrets::Carrier;
use loadout_lib::connections::{Connection, Transport};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, DriverConfiguration, FinishReason,
    Outcome as TurnOutcome, Probe, RunSpec, SessionRef, Tokens, ValidatedImages,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::{AppState, LineSource, QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::Agent;
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Ile miejsca w strumieniu linii. Z zapasem — mierzymy drogę, nie przepustowość.
const LINES: usize = 32;

/// Terminal, w którym stoi rozmowa.
const TERMINAL: &str = "terminal-1";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony.
const PATIENCE: Duration = Duration::from_secs(20);

/// Zatwierdzone Połączenie — to samo, o które chodzi w kryterium.
const SERVER: &str = "figma";

/// Komenda, którą to Połączenie uruchamia.
const SERVER_COMMAND: &str = "figma-mcp";

/// Nazwa wymaganej zmiennej.
///
/// To jest `FIGMA_TOKEN` z kryterium, z prefiksem zadania. Nazwa bez prefiksu bywa NAPRAWDĘ
/// ustawiona w środowisku człowieka, który to czyta — a wtedy resolver odpowiedziałby ze
/// środowiska, nośnika nikt by nie zapytał i test świeciłby na zielono nad martwą funkcją.
const NAME: &str = "Z23_FIGMA_TOKEN";

/// Wartość, która leży w nośniku i nigdzie indziej.
const VALUE: &str = "carried-by-loadout-not-by-the-shell";

/// Nazwa pliku nośnika w bibliotece.
const CARRIER: &str = "env";

/// Zatwierdzone Połączenie tak, jak leży w bibliotece po imporcie.
fn connection_file() -> String {
    format!(
        r#"{{
  "id": "{SERVER}",
  "name": "{SERVER}",
  "enabled": true,
  "transport": {{ "kind": "stdio", "command": "{SERVER_COMMAND}", "args": ["--stdio"], "environment": ["{NAME}"] }},
  "source": "/tmp/mcp.json",
  "sourceHash": "0000",
  "origin": "project"
}}
"#
    )
}

/// Bieg o jednym kroku agenta, który ma to jedno Połączenie.
fn workflow_file(agent: &Uuid) -> String {
    format!(
        r#"{{
  "format": 1,
  "id": "wf_z23",
  "name": "Read the design",
  "steps": [
    {{
      "kind": "agent",
      "id": "s_look",
      "name": "Look",
      "agent": "{agent}",
      "overrides": {{}},
      "instructions": "read the design",
      "folder": {{ "use": "project" }},
      "at": {{ "x": 0, "y": 0 }}
    }}
  ],
  "links": []
}}
"#
    )
}

// ── kryterium ─────────────────────────────────────────────────────────────────────────────

/// (a) Rozmowa rusza i niesie wartość, której w środowisku okna nie ma.
#[tokio::test]
async fn the_lead_gets_the_value_from_the_carrier_when_the_app_started_without_it()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.carrier(0o600)?;

    let started = bench.one_sentence().await?;

    assert!(
        started
            .environment
            .iter()
            .any(|(name, value)| name == NAME && value == OsString::from(VALUE).as_os_str()),
        "the lead's agent has an approved Connection that needs {NAME}, the value is in the \
         carrier Loadout keeps for exactly this, and the conversation started without it: {:?}. \
         Started from the Dock this app inherits no shell, so a resolver that only reads its own \
         environment answers nothing here - and the human sees a tool server that is approved, \
         listed and silent",
        started.environment
    );
    Ok(())
}

/// (b) Odmowa mówi, gdzie Loadout tego szukał.
#[tokio::test]
async fn the_refusal_says_where_loadout_reads_the_value_from() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    // Nośnika NIE MA. To jest stan człowieka, który pierwszy raz włączył Połączenie.
    let said = bench
        .one_sentence()
        .await
        .err()
        .ok_or("a Connection whose variable is set nowhere started the conversation anyway")?;

    assert!(
        said.contains(NAME),
        "the refusal does not name the variable, so there is nothing to go and set. It read: \
         {said:?}"
    );
    assert!(
        said.contains(&bench.carrier_path().display().to_string()),
        "the refusal does not say where Loadout looked, and that is the whole question this \
         message leaves the human with: set it WHERE? The answer differs between an app started \
         from a terminal and the same app started from the Dock, so it has to be written down. \
         It read: {said:?}"
    );
    assert!(
        !said.contains(VALUE),
        "the refusal quotes the value itself, which puts the secret into the window and into \
         everything that copies it. Name the place, never the value. It read: {said:?}"
    );
    Ok(())
}

/// (c) Nośnik, który mogą przeczytać inni, nie jest używany.
#[tokio::test]
async fn a_carrier_other_people_can_read_is_not_used() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.carrier(0o644)?;

    let said = bench.one_sentence().await.err().ok_or_else(|| {
        format!(
            "the value came out of a carrier every account on this machine can read. One \
                 file holds the keys to all of this person's tools, so it is worth exactly what \
                 its permissions are worth: {}",
            bench.carrier_path().display()
        )
    })?;

    assert!(
        said.contains(NAME),
        "the carrier was skipped and the refusal does not even name the variable: {said:?}"
    );
    Ok(())
}

/// (d) Ta sama droga w biegu — osobne wywołanie resolvera, osobna asercja.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_step_of_a_run_gets_the_value_from_the_carrier() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.carrier(0o600)?;
    let agent = bench.saved_agent(&[SERVER.to_owned()])?;
    let workflow = bench.workflow(&workflow_file(&agent))?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::default());

    let deps = RunDeps {
        home: bench.home.path(),
        library: bench.project.path().join(".loadout"),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch)),
        processes: Arc::new(Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))??;
    let _ = tokio::time::timeout(PATIENCE, pump).await;

    let started = lock(&watch.0)
        .clone()
        .ok_or("the step never reached a driver at all")?;
    assert!(
        started
            .environment
            .iter()
            .any(|(name, value)| name == NAME && value == OsString::from(VALUE).as_os_str()),
        "the step of a run reads its Connection secrets through its OWN call of the resolver, \
         and that one still sees only the environment this app was started with: {:?}. A fix \
         that lands in the conversation and not here leaves every run started from the Dock \
         exactly where it was",
        started.environment
    );
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded],
        "the step has to finish, or the assertion above is true of a step that never ran"
    );
    Ok(())
}

/// (e) Nierozwiązana zmienna: bez nadpisania, bez odmowy.
#[test]
fn a_value_nobody_can_find_neither_stops_codex_nor_reaches_its_argv() -> Result<(), Box<dyn Error>>
{
    // Nośnika NIE zakładamy, a `Z23_FIGMA_TOKEN` nie ma w środowisku tego procesu — czyli tej
    // wartości nie ma nigdzie.
    let bench = Bench::new()?;
    let run = TempDir::new()?;

    // SAM BRAK ODMOWY JEST POŁOWĄ KRYTERIUM: `?` niżej przewraca ten test, jeżeli Loadout
    // zatrzyma Start (2026-09-04, Z-23 — decyzja właściciela).
    let configuration = runtime::for_driver_with_secrets(
        run.path(),
        "codex",
        &[a_connection_that_needs_a_value()],
        &Carrier::in_library(Some(&bench.project.path().join(".loadout"))),
    )?;

    assert!(
        configuration
            .arguments
            .iter()
            .all(|argument| !argument.contains(".env.")),
        "a value nobody could find still produced an env override, so Codex would hand its \
         server an empty string where a token belongs - which fails later and further away than \
         no key at all: {:?}",
        configuration.arguments
    );
    assert!(
        configuration
            .arguments
            .iter()
            .any(|argument| argument.starts_with(&format!("mcp_servers.{SERVER}.command="))),
        "the missing value took the whole server down with it. The rest of this Connection is \
         known and has to reach Codex exactly as it did before: {:?}",
        configuration.arguments
    );
    Ok(())
}

/// Połączenie stdio, które wymaga wartości — tej samej, o którą chodzi wyżej.
fn a_connection_that_needs_a_value() -> Connection {
    let mut connection = Connection::imported(
        SERVER.to_owned(),
        SERVER.to_owned(),
        Transport::Stdio {
            command: SERVER_COMMAND.to_owned(),
            args: vec!["--stdio".to_owned()],
            environment: vec![NAME.to_owned()],
        },
        PathBuf::from("/tmp/mcp.json"),
        "0000".to_owned(),
    );
    connection.enabled = true;
    connection
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

/// Co dubler zapamiętał o sterowniku, który NAPRAWDĘ ruszył.
#[derive(Debug, Clone, Default)]
struct Started {
    /// Nazwy i wartości rozstrzygnięte przez resolver tuż przed startem.
    environment: Vec<(String, OsString)>,
}

#[derive(Debug, Default)]
struct Watch(Mutex<Option<Started>>);

fn lock<T>(what: &Mutex<T>) -> MutexGuard<'_, T> {
    what.lock().unwrap_or_else(PoisonError::into_inner)
}

fn fake_drivers(watch: Arc<Watch>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        watch,
        configuration: DriverConfiguration::default(),
    });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler jednego vendora.
///
/// `id()` oddaje „claude", bo tylko dwa identyfikatory umie dziś obsłużyć
/// `connections::runtime::for_driver` — a to jest funkcja, którą to kryterium mierzy.
#[derive(Debug, Clone)]
struct Fake {
    watch: Arc<Watch>,
    configuration: DriverConfiguration,
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        "claude"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("fake".to_owned()),
        })
    }

    fn configured(&self, configuration: &DriverConfiguration) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            configuration: configuration.clone(),
            ..self.clone()
        }))
    }

    /// Bez tego szwu bieg zatrzymuje krok sterownika o identyfikatorze „claude"
    /// (`Live::configured_driver_for_agent`), więc dubler musi go mieć, żeby w ogóle ruszyć.
    fn with_evidence(&self, _target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        // TO JEST PUNKT POMIARU: sterownik, który dojechał tutaj, jest tym, który naprawdę
        // pracuje. O rozstrzygnięte wartości pytamy jego, a nie tego, który wyszedł z fabryki.
        *lock(&self.watch.0) = Some(Started {
            environment: self.configuration.environment.clone(),
        });

        let session = SessionRef {
            vendor: "claude",
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
        fs::create_dir_all(project.path().join(".loadout/agents"))?;
        fs::create_dir_all(project.path().join(".loadout/workflows"))?;
        // `Store::open` zakłada plik bazy, ale nie katalog nad nim.
        fs::create_dir_all(project.path().join(".loadout"))?;
        let connections = project.path().join(".loadout/connections");
        fs::create_dir_all(&connections)?;
        fs::write(connections.join("figma.json"), connection_file())?;
        Ok(Self { home, project })
    }

    fn carrier_path(&self) -> PathBuf {
        self.project.path().join(".loadout").join(CARRIER)
    }

    /// Nośnik z jedną wartością, w podanych prawach dostępu.
    fn carrier(&self, mode: u32) -> Result<(), Box<dyn Error>> {
        let path = self.carrier_path();
        fs::write(
            &path,
            format!("# secrets for my Connections\n{NAME}={VALUE}\n"),
        )?;
        fs::set_permissions(&path, fs::Permissions::from_mode(mode))?;
        Ok(())
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }

    fn folder(&self) -> String {
        self.project.path().to_string_lossy().into_owned()
    }

    fn workflow(&self, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self
            .project
            .path()
            .join(".loadout/workflows")
            .join("z23.json");
        fs::write(&path, text)?;
        Ok(path)
    }

    /// Zapisuje agenta przez produkcyjną drogę i oddaje jego identyfikator.
    fn saved_agent(&self, connections: &[String]) -> Result<Uuid, Box<dyn Error>> {
        let agent = Agent {
            id: Uuid::from_u128(0x23),
            name: "Designer".to_owned(),
            connections: connections.to_vec(),
            ..Agent::example()
        };
        save_agent_inner(&self.project.path().join(".loadout"), &agent, None)?;
        Ok(agent.id)
    }

    /// Jedno zdanie do lidera i to, co z niego zobaczył sterownik.
    async fn one_sentence(&self) -> Result<Started, String> {
        let who = self
            .saved_agent(&[SERVER.to_owned()])
            .map_err(|error| error.to_string())?
            .to_string();
        let folder = self.folder();
        let watch = Arc::new(Watch::default());
        let store = Store::open(&self.db()).map_err(|error| error.to_string())?;
        let state = AppState::new(
            self.home.path().to_path_buf(),
            self.project.path().to_path_buf(),
            store,
            fake_drivers(Arc::clone(&watch)),
        );
        let _watching: LineSource = {
            let (sink, source) = line_channel(LINES);
            state
                .watching_the_lead(TERMINAL, Some(&folder), sink)
                .await?;
            source
        };

        state
            .say_to_the_lead(
                TERMINAL,
                Some(&folder),
                Some(&who),
                "what does this look like?",
            )
            .await?;

        lock(&watch.0)
            .clone()
            .ok_or_else(|| "the conversation never reached a driver at all".to_owned())
    }
}
