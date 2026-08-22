//! Pętla z limitem tur na ŻYWYM biegu: dwa końce, wyjście po `pass` i odmowa po wyczerpaniu prób.
//!
//! # Dlaczego ten plik jest konieczny
//!
//! Wszystkie części pętli były dowiedzione osobno: rozwinięcie grafu (`workflow::unroll`), werdykt
//! z ciała przekazania (`memory::handoff::verdict_in`), zakres tur i reguła koła
//! (`workflow::check`), klucze węzłów (`commands::run::tests`). Ani jedno kryterium nie sprawdzało
//! ich SKLEJKI — a pętla to właśnie sklejka: planista rozwija, sterownik mówi, strażnik pomija,
//! planista zatrzymuje. Każdy z tych czterech może być poprawny osobno i nie spotkać się
//! z pozostałymi.
//!
//! # Co dokładnie mierzą te dwa kryteria
//!
//! Licznik startów sterownika **per prompt**, bo `RunSpec` nie niesie numeru kroku — jego
//! instrukcje są jedyną rzeczą, po której da się kroki rozróżnić (niezmiennik 9). Runda pominięta
//! nie woła sterownika w ogóle, więc licznik jest jedynym miejscem, w którym różnica między
//! „runda przeszła" i „rundy nie było" jest widoczna z zewnątrz.
//!
//! **SŁABĄ WERSJĄ pierwszego kryterium** jest sprawdzenie, że krok za pętlą się wykonał.
//! Przechodzi ją implementacja, która przepala WSZYSTKIE rundy i dopiero potem idzie dalej —
//! czyli ta, w której limit tur kosztuje trzy razy tyle, ile powinien, i nikt tego nie widzi,
//! bo wynik jest ten sam. Dlatego asercja stoi na LICZBIE startów sędziego.
//!
//! **SŁABĄ WERSJĄ drugiego** jest sprawdzenie, że bieg wrócił błędem. Przechodzi ją implementacja,
//! w której krok za pętlą już się wykonał, a bieg zameldował porażkę PO nim. Dlatego asercja stoi
//! na tym, że sterownik nigdy nie zobaczył promptu kroku za pętlą — bo to jest cały powód, dla
//! którego limit tur istnieje: zła robota nie ma pojechać dalej.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

const VENDOR: &str = "fake";

/// Sufit cierpliwości jednego biegu. Cztery kroki dublera nie mają jak trwać dłużej.
const PATIENCE: Duration = Duration::from_secs(20);

/// Prompt sędziego pętli — jedyna rzecz, po której dubler go rozpoznaje.
const JUDGE_PROMPT: &str = "Run the suite and say whether it passed.";

/// Prompt kroku ZA pętlą. Jego pojawienie się u dublera znaczy „praca pojechała dalej".
const AFTER_PROMPT: &str = "Ship it.";

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000c1
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

/// `implement → tester → ship`, i tester z powrotem do implementera, do trzech rund.
///
/// Każdy krok dostaje WŁASNĄ kopię plików: rundy jednego kroku dzielą katalog (i o to chodzi),
/// ale implementer i tester to dwa różne kroki, a te dwa nie mogą biec w jednym folderze przy
/// limicie dwóch naraz. Bez tego plik jest odmową z `one_folder_two_steps`, a nie fiksturą.
const LOOP_FILE: &str = r#"{
  "format": 1,
  "id": "wf_loop",
  "name": "Implement and test",
  "steps": [
    {
      "kind": "agent",
      "id": "s_impl",
      "name": "Implement",
      "agent": "01990000-0000-7000-8000-0000000000c1",
      "overrides": {},
      "copies": 1,
      "instructions": "Make the change.",
      "skills": "all",
      "folder": { "use": "fresh-copy" },
      "handover": "notes",
      "at": { "x": 24, "y": 24 }
    },
    {
      "kind": "agent",
      "id": "s_test",
      "name": "Tester",
      "agent": "01990000-0000-7000-8000-0000000000c1",
      "overrides": {},
      "copies": 1,
      "instructions": "Run the suite and say whether it passed.",
      "skills": "all",
      "folder": { "use": "fresh-copy" },
      "handover": "notes",
      "at": { "x": 24, "y": 168 }
    },
    {
      "kind": "agent",
      "id": "s_ship",
      "name": "Ship",
      "agent": "01990000-0000-7000-8000-0000000000c1",
      "overrides": {},
      "copies": 1,
      "instructions": "Ship it.",
      "skills": "all",
      "folder": { "use": "fresh-copy" },
      "handover": "notes",
      "at": { "x": 24, "y": 312 }
    }
  ],
  "links": [
    { "from": "s_impl", "to": "s_test" },
    { "from": "s_test", "to": "s_ship" },
    { "from": "s_test", "to": "s_impl", "max_turns": 3 }
  ]
}"#;

#[tokio::test]
async fn the_loop_stops_at_the_first_pass() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("loop", LOOP_FILE)?;
    let store = Store::open(&bench.db())?;
    /* Sędzia przepuszcza robotę w DRUGIEJ rundzie. Nie w pierwszej: pętla, która domyka się od
     * razu, nie odróżnia „wyszedł po `pass`" od „nigdy nie zawrócił". Nie w trzeciej: wtedy
     * wyjście po werdykcie jest nieodróżnialne od wyczerpania limitu. */
    let watch = Arc::new(Watch::passing_on_turn(2));

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch)),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 2,
        task: None,
        only: None,
        handoffs_from: None,
    };

    let report = one_run(&deps, &request).await??;

    assert_eq!(
        watch.times(JUDGE_PROMPT),
        2,
        "the tester passed on its second try, so a third must never have started. Three starts \
         mean the run burned a whole agent turn on work nobody needed, and the result looks \
         identical from the outside. The driver saw: {:?}",
        watch.seen()
    );
    assert_eq!(
        watch.times(AFTER_PROMPT),
        1,
        "and the step after the loop has to run exactly once — that is what passing IS for"
    );
    /* Sześć węzłów: trzy rundy implementera i trzy testera, plus krok za pętlą. Wszystkie
     * `Succeeded`, także te pominięte — planista zmniejsza stopień wejściowy dzieci WYŁĄCZNIE po
     * tym stanie, więc gdyby pominięta runda wróciła czymkolwiek innym, `Ship` nie ruszyłby
     * nigdy. Że runda nie biegła, widać po liczniku startów wyżej, nie po jej stanie. */
    assert_eq!(
        report.steps.len(),
        7,
        "three turns of two steps plus the step after the loop; the report has {:?}",
        report.steps
    );
    assert!(
        report.steps.iter().all(|one| *one == StepState::Succeeded),
        "a loop that passed leaves nothing failed behind; it left {:?}",
        report.steps
    );
    Ok(())
}

#[tokio::test]
async fn the_work_after_the_loop_never_starts_when_the_tries_run_out() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("loop", LOOP_FILE)?;
    let store = Store::open(&bench.db())?;
    // Sędzia nie przepuszcza nigdy: `passing_on_turn` większe niż liczba rund w pliku.
    let watch = Arc::new(Watch::passing_on_turn(99));

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch)),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 2,
        task: None,
        only: None,
        handoffs_from: None,
    };

    let report = one_run(&deps, &request).await??;

    assert_eq!(
        watch.times(AFTER_PROMPT),
        0,
        "THIS is the whole reason the limit exists: work that never passed must not go on. A run \
         that reports failure AFTER shipping has already shipped. The driver saw: {:?}",
        watch.seen()
    );
    assert_eq!(
        watch.times(JUDGE_PROMPT),
        3,
        "and all three tries have to be spent — a limit of three that gives up after two is a \
         different promise than the one on the arrow"
    );
    assert!(
        report.steps.contains(&StepState::Failed),
        "the run has to end red, or nothing tells the person their work did not pass; it ended \
         {:?}",
        report.steps
    );
    Ok(())
}

/// Jeden bieg z limitem cierpliwości. Zewnętrzny `Result` mówi „bieg wrócił", wewnętrzny — czym.
async fn one_run(
    deps: &RunDeps<'_>,
    request: &RunRequest,
) -> Result<Result<RunReport, loadout_lib::commands::RunError>, Box<dyn Error>> {
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let drain = async move {
        let _ = pump.await;
    };

    let both = tokio::time::timeout(PATIENCE, async {
        tokio::join!(run_workflow_inner(deps, request, sink), drain)
    })
    .await
    .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))?;
    Ok(both.0)
}

/// Biblioteka użytkownika i projekt na czas jednego kryterium.
struct Bench {
    home: TempDir,
    project: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        Ok(Self { home, project })
    }

    fn agent(&self, slug: &str, text: &str) -> Result<(), Box<dyn Error>> {
        fs::write(
            self.home.path().join("agents").join(format!("{slug}.md")),
            text,
        )?;
        Ok(())
    }

    fn workflow(&self, slug: &str, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self
            .home
            .path()
            .join("workflows")
            .join(format!("{slug}.json"));
        fs::write(&path, text)?;
        Ok(path)
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }
}

/// Fabryka, która dla każdego vendora oddaje ten sam dubler.
fn fake_drivers(watch: Arc<Watch>) -> Drivers {
    Arc::new(move |_| {
        Arc::new(Fake {
            watch: Arc::clone(&watch),
        }) as Arc<dyn AgentDriver>
    })
}

/// Co dubler widział i kiedy sędzia ma przepuścić robotę.
struct Watch {
    seen: Mutex<Vec<String>>,
    /// W której rundzie sędziego (licząc od jedynki) werdykt ma brzmieć `pass`.
    passing_on: usize,
}

impl Watch {
    fn passing_on_turn(turn: usize) -> Self {
        Self {
            seen: Mutex::new(Vec::new()),
            passing_on: turn,
        }
    }

    /// Zapisuje start i oddaje tekst, którym ta tura się skończy.
    ///
    /// Werdykt liczony z LICZBY startów sędziego, bo prompt jest w każdej rundzie identyczny —
    /// i to jest właściwa fikstura: sędzia nie wie, którą rundę biegnie, dokładnie jak prawdziwy
    /// agent w nowej sesji.
    fn entered(&self, prompt: &str) -> String {
        let mut seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
        seen.push(prompt.to_owned());
        if !prompt.contains(JUDGE_PROMPT) {
            return "Done.".to_owned();
        }
        let turn = seen.iter().filter(|one| one.contains(JUDGE_PROMPT)).count();
        if turn >= self.passing_on {
            return "All green.\n\nOUTCOME: PASS".to_owned();
        }
        format!("Two tests are red on try {turn}.\n\nOUTCOME: FAIL")
    }

    /// Ile razy dubler zobaczył prompt zawierający ten fragment.
    fn times(&self, needle: &str) -> usize {
        self.lock()
            .iter()
            .filter(|one| one.contains(needle))
            .count()
    }

    fn seen(&self) -> Vec<String> {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<String>> {
        self.seen.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

struct Fake {
    watch: Arc<Watch>,
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

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let said = self.watch.entered(&spec.prompt);
        let session = SessionRef {
            vendor: VENDOR,
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

        Ok(Box::new(Turn {
            events,
            session,
            said,
        }))
    }
}

/// Jedna tura dublera. `said` staje się ciałem przekazania — i to z niego czyta się werdykt.
#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    said: String,
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
            text: self.said.clone(),
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
