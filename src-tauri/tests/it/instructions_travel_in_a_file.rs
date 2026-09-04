//! Z-17: instrukcje Claude'a jadą plikiem, a lider dostaje izolację i sufit jak zwykły krok.
//!
//! Pierwsze kryterium pyta o argv bez uruchamiania vendora. Znacznik ma dokładnie 40 znaków,
//! więc asercja łapie także rolę wklejoną do większego argumentu, nie tylko argument równy roli.
//! Drugie przechodzi prawdziwą drogą `Threads::say`: dubler zachowuje wszystkie opakowania,
//! składa z nich prawdziwy `ClaudeDriver` i zapisuje argv z jego `command`, zamiast uznawać
//! samo wywołanie szwu za dowód, że vendor dostał ustawienie (niezmiennik 29).

#![allow(clippy::expect_used)]

use std::error::Error;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::Drivers;
use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::commands::chat::{Lead, Threads};
use loadout_lib::commands::settings::save_settings_inner;
use loadout_lib::engine::drivers::claude::{ClaudeDriver, RunSettings, budget_argv};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, DriverConfiguration, FinishReason,
    Outcome as TurnOutcome, Policy, Probe, RunSpec, SessionRef, StepSettings, Tokens, Voice,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof, StepTag};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::line_channel;
use loadout_lib::library::agents::{Agent, Vendor};
use tokio::sync::mpsc;
use uuid::Uuid;

const ROLE_MARK: &str = "LOADOUT-Z17-ROLE-MARKER-0123456789ABCDEF";
const ROLE: &str =
    "Keep answers terse. LOADOUT-Z17-ROLE-MARKER-0123456789ABCDEF Never omit the reason.";
const CEILING: f64 = 31.25;
const LINES: usize = 32;

fn spec(cwd: PathBuf) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd,
        prompt: "map the release path".to_owned(),
        model: None,
        system_append: Some(ROLE.to_owned()),
        reaches_the_web: false,
        policy: Policy::ReadOnly,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

fn argv(driver: &ClaudeDriver, spec: &RunSpec) -> Vec<String> {
    driver
        .command(spec)
        .as_std()
        .get_args()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect()
}

fn value_after<'a>(arguments: &'a [String], flag: &str) -> Option<&'a str> {
    let at = arguments.iter().position(|argument| argument == flag)?;
    arguments.get(at + 1).map(String::as_str)
}

#[test]
fn the_role_never_shows_up_in_the_command_line() {
    let spec = spec(PathBuf::from("."));
    let arguments = argv(&ClaudeDriver::new(), &spec);
    let leaked: Vec<&String> = arguments
        .iter()
        .filter(|argument| argument.contains(ROLE_MARK))
        .collect();

    assert!(
        leaked.is_empty(),
        "the agent's role reached argv, which every user on this machine can read through ps. \
         It must travel in a private file. Leaking arguments: {leaked:?}"
    );
}

#[tokio::test]
async fn the_lead_runs_with_its_own_settings_and_a_ceiling() -> Result<(), Box<dyn Error>> {
    let library = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let agent = Agent {
        id: Uuid::from_u128(17),
        name: "Bounded Claude Lead".to_owned(),
        runs_with: Vendor::ClaudeCode,
        instructions: ROLE.to_owned(),
        ..Agent::example()
    };
    save_agent_inner(library.path(), &agent, None)?;
    save_settings_inner(
        library.path(),
        &agent.id.to_string(),
        CEILING,
        false,
        0,
        true,
    )?;
    let lead = Lead::pointed_at(library.path(), Some(&agent.id.to_string()))
        .map_err(|error| error.to_string())?;

    let seen = Arc::new(Mutex::new(Vec::new()));
    let driver: Arc<dyn AgentDriver> = Arc::new(CapturingClaude::new(Arc::clone(&seen)));
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));
    let (sink, _source) = line_channel(LINES);
    let threads = Threads::new();
    threads.library_is(library.path().to_path_buf());
    threads.lines_go_to(project.path().to_path_buf(), sink);

    threads
        .say(
            &drivers,
            &lead,
            project.path().to_path_buf(),
            "what should we change first?",
        )
        .await
        .map_err(|error| error.to_string())?;

    let arguments = seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .first()
        .cloned()
        .ok_or("the first sentence did not start the lead")?;
    assert!(
        arguments
            .iter()
            .all(|argument| !argument.contains(ROLE_MARK)),
        "the lead's role reached argv instead of a private file: {arguments:?}"
    );

    let settings = value_after(&arguments, "--settings")
        .map(PathBuf::from)
        .ok_or("the lead started without --settings")?;
    let conversation = settings
        .parent()
        .ok_or("the lead settings file has no conversation directory")?;
    assert_eq!(
        settings.file_name().and_then(|name| name.to_str()),
        Some("claude-settings-_lead.json"),
        "the lead settings do not carry their own work key: {settings:?}"
    );
    assert!(
        conversation.starts_with(project.path().join(".loadout/conversations")),
        "the lead settings escaped its private conversation directory: {settings:?}"
    );
    assert!(
        conversation.join("claude/_lead").is_dir(),
        "the lead has no private Claude state under its conversation: {conversation:?}"
    );
    assert!(
        conversation.join("mem/_lead").is_dir(),
        "the lead's automatic memory directory was not created: {conversation:?}"
    );
    assert_eq!(
        value_after(&arguments, "--max-budget-usd"),
        Some("31.25"),
        "the lead did not receive the ceiling from settings.json. argv was {arguments:?}"
    );

    let proofs = threads.close().await;
    assert!(
        proofs
            .iter()
            .all(|proof| matches!(proof, GroupProof::Dead { .. })),
        "closing the test conversation did not prove every lead handle dead: {proofs:?}"
    );
    Ok(())
}

#[derive(Clone)]
struct CapturingClaude {
    configuration: DriverConfiguration,
    settings: Option<RunSettings>,
    ceiling: Option<f64>,
    seen: Arc<Mutex<Vec<Vec<String>>>>,
}

impl CapturingClaude {
    fn new(seen: Arc<Mutex<Vec<Vec<String>>>>) -> Self {
        Self {
            configuration: DriverConfiguration::default(),
            settings: None,
            ceiling: None,
            seen,
        }
    }

    fn concrete(&self) -> ClaudeDriver {
        let mut configuration = self.configuration.clone();
        if let Some(ceiling) = self.ceiling {
            configuration.arguments.extend(budget_argv(ceiling));
        }
        let driver = ClaudeDriver::new().with_configuration(configuration);
        match &self.settings {
            Some(settings) => driver.with_settings(settings.clone()),
            None => driver,
        }
    }
}

#[async_trait]
impl AgentDriver for CapturingClaude {
    fn id(&self) -> &'static str {
        "claude"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("z17-capture".to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(argv(&self.concrete(), &spec));
        Ok(Box::new(Turn {
            session: SessionRef {
                vendor: "claude",
                id: spec.run_id.to_string(),
            },
            events,
        }))
    }

    fn effort_argv(&self, level: &str) -> Vec<String> {
        ClaudeDriver::new().effort_argv(level)
    }

    fn narrows_its_tools(&self) -> bool {
        ClaudeDriver::new().narrows_its_tools()
    }

    fn configured(&self, configuration: &DriverConfiguration) -> Option<Arc<dyn AgentDriver>> {
        let mut driver = self.clone();
        driver.configuration = configuration.clone();
        Some(Arc::new(driver))
    }

    fn with_evidence(&self, _target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }

    fn with_settings(
        &self,
        settings: &StepSettings,
    ) -> Option<anyhow::Result<Arc<dyn AgentDriver>>> {
        Some(RunSettings::for_step(settings).map(|written| {
            let mut driver = self.clone();
            driver.settings = Some(written);
            Arc::new(driver) as Arc<dyn AgentDriver>
        }))
    }

    fn with_budget(&self, dollars: f64) -> Option<Arc<dyn AgentDriver>> {
        let mut driver = self.clone();
        driver.ceiling = Some(dollars);
        Some(Arc::new(driver))
    }

    fn for_step(&self, _tag: &StepTag) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }
}

#[derive(Debug)]
struct Turn {
    session: SessionRef,
    events: mpsc::Sender<DecodedEvent>,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn voice(&self) -> Option<Voice> {
        None
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
