//! AC-2 dla T-94: bieg ma sufit wydatku i staje na nim.
//!
//! ZMIERZONE, PO CO TO JEST. Jedynym limitem biegu był `giveUpAfterMinutes`, a minuty nie są
//! ceną: 96-minutowy bieg właściciela kosztował ~$40 u Claude'a i nikt nie mógł powiedzieć
//! „stop po $20".
//!
//! **Sufit jedzie osobnym argumentem, nie polem `RunRequest`** — i to nie jest szczegół stylu.
//! Zmierzone 2026-08-24: literał `RunRequest { … }` stoi w tym drzewie w 55 plikach, a ten typ
//! nie ma `Default`, więc jedno nowe pole przewróciłoby każdy z nich naraz, w tym pliki, których
//! to zadanie nie posiada (AGENTS.md §7).
//!
//! **Trzy zdania kryterium mierzy JEDEN bieg, i to jest jego treść.** Fikstura ustawia je tak,
//! żeby żadne z nich nie dało się spełnić przypadkiem:
//!
//! - `Costly` kosztuje $12 przy sufcie $10 i kończy się szybko — po nim żaden nowy krok nie ma
//!   prawa ruszyć;
//! - `Slow` biegnie obok niego i w chwili przekroczenia sufitu **pracuje**. Ma się skończyć
//!   normalnie: nie zabijamy pracy, za którą już zapłacono, a bieg zabity w połowie tury płaci
//!   za nią tak samo i nie dostaje odpowiedzi;
//! - `Later` czeka na `Costly` i jest jedynym krokiem, który sufit zatrzymuje. Ma skończyć jako
//!   pominięty, **ze zdaniem nazywającym sufit i kwotę** — pominięcie bez powodu jest tym samym
//!   ślepym punktem, który 2026-08-23 skasował trzy kroki biegu właściciela bez ani jednego
//!   zdania o przyczynie.
//!
//! Słaba wersja tego kryterium sprawdza sam stan `skipped`. Przechodzi ją implementacja, która
//! kasuje wszystko, co jeszcze nie ruszyło, w chwili przekroczenia — razem z krokiem, który
//! w tej chwili pracuje, czyli tracąc pieniądze już wydane.
//!
//! Drugi przypadek jest kontrolą: ta sama fikstura **bez sufitu** ma dojść do końca w komplecie,
//! a plik biegu nie ma wtedy nieść ani słowa o sufcie, którego nikt nie postawił.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_with_budget;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{LineSink, QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use serde_json::Value;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Sufit wydatku tego biegu.
const BUDGET: f64 = 10.0;

/// Ile kosztuje krok, który sam jeden przekracza sufit.
const COSTLY: f64 = 12.0;

/// Ile kosztuje krok, który biegnie obok niego. Grosze, żeby o przekroczeniu decydował ten drugi.
const CHEAP: f64 = 1.0;

/// Ile naraz. Dwa, bo bez tego nie ma jak mieć kroku, który W CHWILI przekroczenia pracuje.
const AT_ONCE: usize = 2;

/// Ile czekamy, zanim uznamy bieg za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(20);

/// Jak długo trwa tura kroku, który ma zdążyć się skończyć pierwszy.
const QUICK_TURN: Duration = Duration::from_millis(50);

/// Jak długo trwa tura kroku, który ma jeszcze pracować, kiedy sufit zostanie przekroczony.
/// Rząd wielkości ponad [`QUICK_TURN`], żeby kolejność nie zależała od obciążenia maszyny.
const LONG_TURN: Duration = Duration::from_millis(700);

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

/// `Costly` i `Slow` ruszają razem; `Later` czeka na `Costly` i jest jedynym krokiem, który
/// dowiaduje się o sufcie.
///
/// Strzałka jest tu treścią, nie ozdobą: bez niej wszystkie trzy kroki są gotowe naraz,
/// a o tym, które dwa dostaną miejsce, decyduje wtedy przypadek — i pomiar mierzyłby wyścig,
/// nie sufit.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_budget",
  "name": "A run with a ceiling",
  "steps": [
    {
      "kind": "agent",
      "id": "s_costly",
      "name": "Costly",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "spend a lot",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_slow",
      "name": "Slow",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "take your time",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_later",
      "name": "Later",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "come after",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 240 }
    }
  ],
  "links": [{ "from": "s_costly", "to": "s_later" }]
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_step_that_had_not_started_is_skipped_and_says_why() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let (report, run_file) = bench.run(Some(BUDGET)).await?;

    assert_eq!(
        report.steps,
        vec![
            StepState::Succeeded,
            StepState::Succeeded,
            StepState::Skipped
        ],
        "with a ceiling of ${BUDGET} and a first step costing ${COSTLY}, the two steps that were \
         already working have to finish — Loadout does not throw away work it has already paid \
         for — and the one that had not started yet has to end as skipped. It came out as {:?}",
        report.steps
    );

    let later = step_named(&run_file, "Later")?;
    let said = later
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    assert!(
        said.contains("10") && said.contains("12"),
        "a step nobody ran has to say WHY, and the why is two numbers: what the run had spent \
         and what its ceiling was. Without both, the sentence is indistinguishable from a step \
         skipped because something above it did not pass — and a run that ends with silent \
         empty rows is the blind spot this repository exists to remove. It said {said:?}"
    );

    assert_eq!(
        run_file.get("budget_usd").and_then(Value::as_f64),
        Some(BUDGET),
        "the ceiling has to reach run.json: files are the truth and the index is rebuilt from \
         them (invariant 4), so a ceiling kept only in memory disappears the moment the window \
         closes"
    );
    assert_eq!(
        run_file.get("spent_usd").and_then(Value::as_f64),
        Some(COSTLY + CHEAP),
        "and so does what the run actually spent — both steps that finished, including the one \
         that was still working when the ceiling was passed"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_run_without_a_ceiling_is_untouched() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let (report, run_file) = bench.run(None).await?;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; 3],
        "the same three steps, the same costs, no ceiling: every one of them has to finish. A \
         run that stops at a limit nobody set is worse than no limit at all. It came out as {:?}",
        report.steps
    );
    assert_eq!(
        run_file.get("budget_usd"),
        None,
        "a run nobody capped has nothing to say about a cap, and a key saying \"no ceiling\" in \
         every run file in history is length paid for silence"
    );
    Ok(())
}

/// Krok o tej nazwie, tak jak zapisał go plik biegu.
fn step_named(run_file: &Value, name: &str) -> Result<Value, Box<dyn Error>> {
    run_file
        .get("steps")
        .and_then(Value::as_array)
        .ok_or("run.json carries no steps at all")?
        .iter()
        .find(|step| step.get("name").and_then(Value::as_str) == Some(name))
        .cloned()
        .ok_or_else(|| format!("run.json has no step named {name}").into())
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
        // `Store::open` zakłada plik bazy, ale nie katalog nad nim.
        fs::create_dir_all(project.path().join(".loadout"))?;

        let agent = home.path().join("agents").join("hand.md");
        fs::write(&agent, HAND_FILE)?;
        let workflow = home.path().join("workflows").join("budget.json");
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

    /// Jeden bieg z tym sufitem (albo bez niego) i jego plik na dysku.
    async fn run(&self, budget_usd: Option<f64>) -> Result<(RunReport, Value), Box<dyn Error>> {
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store: &self.store,
            drivers: fake_drivers(),
            processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow: self.workflow.clone(),
            how_many_at_once: AT_ONCE,
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
        let text = fs::read_to_string(report.dir.join("run.json"))?;
        let run_file: Value = serde_json::from_str(&text)?;
        Ok((report, run_file))
    }
}

/// Szew, którym bieg mówi do okna: nadajnik dla biegu i czekanie na pompę.
fn the_pump_seam() -> (LineSink, impl Future<Output = ()>) {
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    (sink, async move {
        let _ = pump.await;
    })
}

/// Fikstura ma przejść walidator **bez ani jednego problemu**, a jej plik agenta ma dać się
/// przeczytać. To nie jest część kryterium, tylko jego przesłanka.
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

/// Fabryka, która dla każdego vendora oddaje ten sam dubler.
fn fake_drivers() -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake);
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Ile ta tura kosztuje i jak długo trwa — rozstrzygnięte po zdaniu, które krok dostał.
///
/// Po treści polecenia, a nie po numerze wywołania: kolejność wejścia do dublera jest tym, czego
/// to kryterium nie kontroluje, więc fikstura oparta na niej mierzyłaby wyścig.
fn what_this_turn_costs(prompt: &str) -> (f64, Duration) {
    if prompt.contains("spend a lot") {
        (COSTLY, QUICK_TURN)
    } else if prompt.contains("take your time") {
        (CHEAP, LONG_TURN)
    } else {
        (CHEAP, QUICK_TURN)
    }
}

/// Dubler: jedna tura o zadanej cenie i długości.
#[derive(Debug)]
struct Fake;

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

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };
        let (cost, hold) = what_this_turn_costs(&spec.prompt);

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
            hold,
        }))
    }
}

/// Jedna tura dublera.
#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    cost: f64,
    hold: Duration,
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
        tokio::time::sleep(self.hold).await;
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: String::new(),
            cost_usd: Some(self.cost),
            tokens: Tokens::default(),
            turns: 1,
            took: self.hold,
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
