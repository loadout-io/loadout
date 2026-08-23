//! AC-3 dla T-94: krok Claude'a w biegu z sufitem dostaje `--max-budget-usd <reszta>`.
//!
//! PO CO, SKORO SUFIT PILNUJE JUŻ LOADOUT. Bo Loadout liczy dopiero **skończone** tury: sumę
//! zna po fakcie, a tura, która sama jedna przekroczy resztę, jest już wtedy opłacona. Vendor,
//! któremu powiemy, ile mu wolno, zatrzyma ją od środka. Jedno i drugie naraz, bo żadne z nich
//! samo nie wystarcza: Loadout bez flagi płaci za ostatnią turę, flaga bez Loadouta nie wie
//! o krokach, które biegły obok.
//!
//! ZMIERZONE 2026-08-23 na `claude --help` 2.1.241: flaga nazywa się `--max-budget-usd <amount>`
//! i „only works with --print". Loadout woła z `-p`, więc działa — i to rozstrzyga spike S-2,
//! który stał nierozstrzygnięty od T1 i którego dwa komentarze w `claude.rs` nazywały otwartym.
//!
//! **Dwie połowy, bo każda sama w sobie jest zielenią przy martwej drugiej.** Pierwsza pyta
//! adapter: czy z liczby dolarów robi parę argumentów, którą ten vendor rozumie. Druga pyta
//! bieg: czy krok naprawdę dostaje RESZTĘ, a nie cały sufit — bo implementacja podająca zawsze
//! pełną kwotę pozwala każdemu kolejnemu krokowi wydać tyle, ile wynosi sufit CAŁEGO biegu,
//! i przechodzi każdą asercję o samej obecności flagi.
//!
//! Trzeci przypadek jest kontrolą: bieg bez sufitu nie ma prawa nieść tej flagi w ogóle.
//! Domyślna kwota wpisana „na wszelki wypadek" byłaby limitem, którego nikt nie postawił.

use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_with_budget;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::claude::{ClaudeDriver, VENDOR, budget_argv};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, DriverConfiguration, FinishReason,
    Outcome as TurnOutcome, Policy, Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::{LineSink, QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Nazwa flagi — przepisana z `claude --help`, nie z naszego kodu.
const FLAG: &str = "--max-budget-usd";

/// Sufit wydatku tego biegu.
const BUDGET: f64 = 10.0;

/// Ile kosztuje pierwszy krok. Trzy miejsca po przecinku, bo reszta ma się zaokrąglić W DÓŁ
/// do centa: kwota zaokrąglona w górę oddaje vendorowi pół centa ponad to, co postawił człowiek.
const FIRST_COSTS: f64 = 3.339;

/// Co zostaje dla drugiego kroku: `10 - 3.339 = 6.661`, w dół do centa.
const LEFT_FOR_THE_SECOND: &str = "6.66";

/// Ile czekamy, zanim uznamy bieg za zawieszony.
const PATIENCE: Duration = Duration::from_secs(20);

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000f1
name: Hand
summary: Does the work
color: moss
runsWith: claude-code
model: opus
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 20
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Do the work.
";

/// Dwa kroki jeden po drugim — strzałka jest tu treścią: reszta sufitu ma zależeć od tego, co
/// wydał krok PRZED tym, a przy dwóch krokach naraz nie byłoby wiadomo, który był pierwszy.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_remaining",
  "name": "What is left",
  "steps": [
    {
      "kind": "agent",
      "id": "s_first",
      "name": "First",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "go first",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_second",
      "name": "Second",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "go second",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    }
  ],
  "links": [{ "from": "s_first", "to": "s_second" }]
}
"#;

#[test]
fn the_agent_app_turns_the_amount_into_the_flag_it_knows() -> Result<(), Box<dyn Error>> {
    let carried = DriverConfiguration {
        arguments: budget_argv(7.5),
        ..DriverConfiguration::default()
    };
    let command = ClaudeDriver::new()
        .with_configuration(carried)
        .command(&spec());
    let args: Vec<String> = command
        .as_std()
        .get_args()
        .map(OsStr::to_string_lossy)
        .map(std::borrow::Cow::into_owned)
        .collect();

    let at = args
        .iter()
        .position(|arg| arg == FLAG)
        .ok_or_else(|| format!("{FLAG} never reached the command line: {args:?}"))?;
    assert_eq!(
        args.get(at + 1).map(String::as_str),
        Some("7.50"),
        "the amount has to stand right after the flag, in dollars and cents. A flag with \
         nothing after it is either a start-up error or — worse — a flag that means something \
         else. The whole line was {args:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_second_step_is_told_what_is_left_not_the_whole_ceiling() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let handed = bench.run(Some(BUDGET)).await?;

    assert_eq!(
        handed.len(),
        2,
        "both steps have to reach the agent app for this measurement to mean anything; it saw \
         {handed:?}"
    );
    assert_eq!(
        amount_in(&handed[0]).as_deref(),
        Some("10.00"),
        "nothing had been spent when the first step started, so the whole ceiling was still \
         its to use. It was handed {:?}",
        handed[0]
    );
    assert_eq!(
        amount_in(&handed[1]).as_deref(),
        Some(LEFT_FOR_THE_SECOND),
        "the first step spent ${FIRST_COSTS}, so the second one may spend what is left and not \
         a cent more. Handing it the whole ceiling again lets every step spend the ceiling of \
         the entire run — and passes any check that only asks whether the flag is there. It was \
         handed {:?}",
        handed[1]
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_run_without_a_ceiling_carries_no_such_flag() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let handed = bench.run(None).await?;

    assert!(
        handed.iter().all(|fragment| amount_in(fragment).is_none()),
        "nobody capped this run, so no step may be capped either. A default amount put in \
         \"just in case\" is a limit nobody set, and the person who hits it has no way to tell \
         where it came from. The steps were handed {handed:?}"
    );
    Ok(())
}

/// Kwota stojąca zaraz za flagą, jeśli flaga w ogóle w tym fragmencie jest.
fn amount_in(fragment: &[String]) -> Option<String> {
    let at = fragment.iter().position(|arg| arg == FLAG)?;
    fragment.get(at + 1).cloned()
}

/// `RunSpec` do połowy adapterowej — różni się od produkcyjnego wyłącznie tym, że nic nie robi.
fn spec() -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: PathBuf::from("."),
        prompt: "rename the widget".to_owned(),
        model: None,
        system_append: None,
        policy: Policy::ReadOnly,
        reaches_the_web: false,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Biblioteka użytkownika, folder pracy i jeden bieg.
struct Bench {
    home: TempDir,
    project: TempDir,
    workflow: PathBuf,
    store: Store,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;

        let agent = home.path().join("agents").join("hand.md");
        fs::write(&agent, HAND_FILE)?;
        let workflow = home.path().join("workflows").join("remaining.json");
        fs::write(&workflow, WORKFLOW)?;
        the_fixture_can_run(&workflow, &[&agent])?;

        let store = Store::open(&project.path().join(".loadout").join("loadout.db"))?;
        Ok(Self {
            home,
            project,
            workflow,
            store,
        })
    }

    /// Jeden bieg; oddaje fragmenty argv, które dostały kolejne kroki.
    async fn run(&self, budget_usd: Option<f64>) -> Result<Vec<Vec<String>>, Box<dyn Error>> {
        let heard = Arc::new(Heard::default());
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store: &self.store,
            drivers: fake_drivers(Arc::clone(&heard)),
            processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow: self.workflow.clone(),
            how_many_at_once: 2,
            task: None,
            part: None,
            handoffs_from: None,
        };
        let (sink, drain) = the_pump_seam();

        let (ran, ()) = tokio::time::timeout(PATIENCE, async {
            tokio::join!(
                run_workflow_with_budget(&deps, &request, sink, budget_usd),
                drain
            )
        })
        .await
        .map_err(|_| format!("the run did not finish within {PATIENCE:?}"))?;

        let report = ran?;
        assert_eq!(
            report.steps,
            vec![StepState::Succeeded; 2],
            "both steps have to finish, or the fragments below belong to a run that fell over \
             for some other reason. It ended as {:?}",
            report.steps
        );
        Ok(heard.taken())
    }
}

/// Szew, którym bieg mówi do okna.
fn the_pump_seam() -> (LineSink, impl Future<Output = ()>) {
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    (sink, async move {
        let _ = pump.await;
    })
}

/// Fikstura ma przejść walidator bez ani jednego problemu. Przesłanka kryterium, nie kryterium.
fn the_fixture_can_run(workflow: &Path, agents: &[&Path]) -> Result<(), Box<dyn Error>> {
    let problems: Vec<String> = check(&load(workflow)?)
        .into_iter()
        .filter(|note| note.level == Level::Problem)
        .map(|note| note.message)
        .collect();
    assert!(
        problems.is_empty(),
        "the fixture would be refused before it ran, so this criterion could never pass: \
         {problems:?}"
    );
    for agent in agents {
        read_agent_file(agent).map_err(|error| format!("{}: {error}", agent.display()))?;
    }
    Ok(())
}

/// Co kolejne kroki dostały w swoim fragmencie argv, w kolejności wejścia.
#[derive(Debug, Default)]
struct Heard {
    fragments: Mutex<Vec<Vec<String>>>,
}

impl Heard {
    fn saw(&self, arguments: &[String]) {
        self.fragments
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(arguments.to_vec());
    }

    fn taken(&self) -> Vec<Vec<String>> {
        self.fragments
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// Fabryka oddająca dubler, który **nazywa się tak, jak Claude**.
///
/// Ta nazwa jest treścią: fragment argv z sufitem niesie flagę, którą zna dokładnie jeden
/// vendor, więc rdzeń pyta krok o to, czym on jest — dokładnie tak samo, jak przy zatwierdzonych
/// Connections (`connections::runtime::for_driver`). Dubler o cudzej nazwie nie dostałby tego
/// fragmentu i pomiar mierzyłby wtedy własną atrapę.
fn fake_drivers(heard: Arc<Heard>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        heard,
        arguments: Vec::new(),
    });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler: zapisuje fragment argv, który mu podano, i oddaje turę o zadanej cenie.
#[derive(Clone, Debug)]
struct Fake {
    heard: Arc<Heard>,
    arguments: Vec<String>,
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        VENDOR
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some(VENDOR.to_owned()),
        })
    }

    fn configured(&self, configuration: &DriverConfiguration) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            heard: Arc::clone(&self.heard),
            arguments: configuration.arguments.clone(),
        }))
    }

    /// Dubler nazywający się jak Claude MUSI umieć wziąć cel dowodów: bieg odmawia startu
    /// krokowi vendora, który tego szwu nie ma („cannot preserve its private run evidence").
    /// Sam plik dowodu nie jest przedmiotem tego pomiaru, więc zostaje tu nietknięty.
    fn with_evidence(&self, _target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        // Zapisujemy PRZY STARCIE, nie w `configured`: sterownik bez ani jednego argumentu nie
        // przechodzi przez tamtą drogę w ogóle (`commands::run`, `configured` woła się tylko dla
        // niepustego fragmentu), więc bieg bez sufitu nie zostawiłby tam ani jednego wpisu
        // i kontrola byłaby ślepa.
        self.heard.saw(&self.arguments);
        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };
        let cost = if spec.prompt.contains("go first") {
            FIRST_COSTS
        } else {
            0.0
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

        Ok(Box::new(Turn {
            events,
            session,
            cost,
        }))
    }
}

/// Jedna tura dublera.
#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    cost: f64,
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
            text: String::new(),
            cost_usd: Some(self.cost),
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
