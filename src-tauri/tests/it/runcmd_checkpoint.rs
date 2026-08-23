//! AC-6 dla T-15: `Ask me first` zatrzymuje **bieg**, a nie krok, i nic za nim nie startuje.
//!
//! **Słaba wersja to asercja, że bieg z punktem kontrolnym w środku kończy się sukcesem.**
//! Przechodzi, kiedy punkt kontrolny jest ignorowany, a `build` startuje natychmiast — pytanie
//! do człowieka pojawia się wtedy na ekranie już po tym, jak agent zrobił swoje. Rozróżniają je
//! dwie rzeczy: **licznik uruchomień `build` równy zeru w momencie pauzy** oraz asercja, że pole
//! `paused` siedzi na **biegu**, a nie na kroku.
//!
//! Stan pauzy czytamy z `run.json`, a nie z wartości zwróconej przez funkcję, i to nie jest
//! wygoda. `paused` jest jedynym stanem biegu, którego nie ma w maszynie stanów **kroku**
//! (`docs/ARCHITECTURE.md` §5) — a typ `StepState` nie da się o niego zapytać, bo takiego
//! wariantu nie ma. Asercja o czymś, czego typ i tak zabrania, niczego nie dowodzi; plik na dysku
//! jest miejscem, w którym `"status": "paused"` da się wpisać przy kroku i dlatego jedynym, w
//! którym da się to złapać.
//!
//! Punkt kontrolny **nie jest etapem zaszytym w Ruście** (niezmiennik 27): to rodzaj kafelka
//! w pliku workflow (T3 §6.1 reguła 5, D6), więc kolejność dalej mieszka wyłącznie w grafie.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use loadout_lib::commands::run::{continue_run_inner, run_workflow_inner, stop_run_inner};
use loadout_lib::commands::{Drivers, Outcome, RunControl, RunDeps, RunRequest};
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
use serde_json::Value as Json;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Instrukcje kroku, który nie ma prawa ruszyć przed odpowiedzią człowieka.
/// Co człowiek napisał w okienku punktu kontrolnego. Zdanie, nie słowo: fragment, który da się
/// znaleźć w pliku i którego nic innego w tym biegu nie produkuje.
const ANSWER: &str = "Ship it, but keep the old endpoint for one release.";

const BUILD: &str = "build";

/// Ile czekamy na pauzę, a potem na cały bieg. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(5);

/// Jak często pytamy dysk o stan biegu.
const EVERY: Duration = Duration::from_millis(5);

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-00000000ab01
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

/// `plan → ask → build`, gdzie `ask` jest kafelkiem kontrolnym.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_ask_me_first",
  "name": "Ask me first",
  "steps": [
    {
      "kind": "agent",
      "id": "s_plan",
      "name": "Plan",
      "agent": "01990000-0000-7000-8000-00000000ab01",
      "overrides": {},
      "instructions": "plan",
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "checkpoint",
      "id": "s_ask",
      "name": "Ask me first",
      "question": "Does the plan look right?",
      "at": { "x": 240, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_build",
      "name": "Build",
      "agent": "01990000-0000-7000-8000-00000000ab01",
      "overrides": {},
      "instructions": "build",
      "at": { "x": 480, "y": 0 }
    }
  ],
  "links": [
    { "from": "s_plan", "to": "s_ask" },
    { "from": "s_ask", "to": "s_build" }
  ]
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn continue_lets_the_run_go_on_from_the_checkpoint() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let hand = bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("ask-me-first", WORKFLOW)?;
    the_fixture_can_run(&workflow, &[&hand])?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::default());

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
        how_many_at_once: 3,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let (sink, drain) = the_pump_seam();
    let answer = async {
        let paused = wait_until_paused(bench.project.path()).await?;
        the_pause_sits_on_the_run(&paused, &watch)?;
        // 2026-08-18 — ODPOWIEDŹ CZŁOWIEKA JEDZIE RAZEM ZE ZGODĄ, i to jest nowe. Do tego dnia
        // `continue_run` nie brało argumentu: zdanie napisane w oknie znikało razem z pytaniem
        // i nie trafiało nigdzie. Asercja na końcu tego pliku żąda, żeby zdanie wylądowało
        // w `handoffs/` — czyli tą samą drogą, którą wchodzi wynik agenta.
        continue_run_inner(&deps, Some(ANSWER.to_owned())).await?;
        Ok::<(), Box<dyn Error>>(())
    };

    let (ran, answered, ()) = tokio::time::timeout(PATIENCE.saturating_mul(3), async {
        tokio::join!(run_workflow_inner(&deps, &request, sink), answer, drain)
    })
    .await
    .map_err(|_| "the run never came back after Continue".to_owned())?;
    answered?;
    let report = ran?;

    assert_eq!(
        report.outcome,
        Outcome::Done,
        "a run that was answered and let go on ends on its own, not as cancelled"
    );
    assert_eq!(
        report.steps,
        vec![
            StepState::Succeeded,
            StepState::Succeeded,
            StepState::Succeeded
        ],
        "after Continue the checkpoint is answered and `build` runs: all three steps have to end \
         `succeeded`. They ended as {:?}",
        report.steps
    );
    assert_eq!(
        watch.times(BUILD),
        1,
        "`build` had to run exactly once after Continue; the driver started it {} time(s)",
        watch.times(BUILD)
    );

    /* ODPOWIEDŹ CZŁOWIEKA JEST NA DYSKU, w przekazaniach tego biegu.
     *
     * SŁABA WERSJA TEJ ASERCJI: `assert!(continue_run_inner(&deps, Some(..)).is_ok())`. Przechodzi
     * dla implementacji, która argument przyjmuje i wyrzuca — czyli dla dokładnie tego defektu,
     * który tu naprawiamy. Odróżnia je pytanie o TREŚĆ na dysku: zdanie człowieka ma dojechać
     * do pracy tą samą drogą, którą wchodzi wynik agenta, więc krok idący po punkcie kontrolnym
     * przeczyta je w indeksie przekazań.
     *
     * Katalog, nie prompt: prompt następnego kroku składa się z indeksu przekazań, a ten powstaje
     * z plików. Pytanie o pliki jest więc pytaniem o to samo, tylko odporniejszym na to, jak
     * dokładnie prompt jest sklejony. */
    let handoffs = report.dir.join("handoffs");
    let mut said = String::new();
    for entry in std::fs::read_dir(&handoffs)
        .map_err(|error| format!("{} could not be read: {error}", handoffs.display()))?
    {
        said.push_str(&std::fs::read_to_string(entry?.path())?);
    }
    assert!(
        said.contains(ANSWER),
        "what the person typed at the checkpoint has to reach the work. It is nowhere in {} — \
         so the answer was accepted, the run went on, and the sentence was thrown away. That is \
         the control that LIES, and it is the one this argument exists to end.",
        handoffs.display()
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stopping_at_the_checkpoint_cancels_what_was_behind_it() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let hand = bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("ask-me-first", WORKFLOW)?;
    the_fixture_can_run(&workflow, &[&hand])?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::default());

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
        how_many_at_once: 3,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let (sink, drain) = the_pump_seam();
    let answer = async {
        let paused = wait_until_paused(bench.project.path()).await?;
        the_pause_sits_on_the_run(&paused, &watch)?;
        Ok::<Outcome, Box<dyn Error>>(stop_run_inner(&deps).await?)
    };

    let (ran, stopped, ()) = tokio::time::timeout(PATIENCE.saturating_mul(3), async {
        tokio::join!(run_workflow_inner(&deps, &request, sink), answer, drain)
    })
    .await
    .map_err(|_| "the run never came back after Stop at the checkpoint".to_owned())?;

    assert_eq!(
        stopped?,
        Outcome::Cancelled,
        "Stop answers with a value, never with Err(Cancelled) (invariant 7)"
    );
    let report = ran?;
    assert_eq!(
        report.steps,
        vec![
            StepState::Succeeded,
            StepState::Cancelled,
            StepState::Cancelled
        ],
        "`build` never started and has to end `cancelled`, not `skipped`: `skipped` means \
         \"someone upstream failed\", and here nobody failed — a person said stop \
         (docs/ARCHITECTURE.md §5). They ended as {:?}",
        report.steps
    );
    assert_eq!(
        watch.times(BUILD),
        0,
        "`build` ran {} time(s) even though the run was stopped at the question in front of it",
        watch.times(BUILD)
    );
    Ok(())
}

/// Szew, którym bieg mówi do okna: nadajnik dla biegu i czekanie na pompę.
///
/// 2026-08-17 (T-30) — bieg oddaje linie POJEDYNCZO do `LineSink`, a sklejaniem zajmuje się
/// pompa po drugiej stronie, więc kanał zakłada się tutaj tak, jak zakłada go komenda:
/// `line_channel` + `spawn_pump`. Zmieniła się wyłącznie konstrukcja kanału przy wywołaniu —
/// ani jedna asercja tego kryterium nie wie o tej zmianie, bo sądzi ono `run.json` i obserwatora
/// sterownika, a nie wiersze. Kanał do okna jest czarną dziurą z tego samego powodu.
///
/// Czekanie oddajemy osobno, bo stoi w `join!` dokładnie tam, gdzie stało osuszanie kanału:
/// pompa kończy się sama, kiedy zniknie ostatni nadajnik, a ten ginie razem z powrotem biegu.
/// Slowo w instrukcji kroku, po ktorym dubler konczy ture PORAZKA.
///
/// Dubler tego pliku konczyl do 2026-08-23 kazda ture sukcesem, bo zadne kryterium tutaj nie
/// potrzebowalo porazki. Kryterium `ask-me` potrzebuje: bez kroku, ktory naprawde nie przeszedl,
/// nie da sie sprawdzic, czy bieg staje i pyta.
const FAILS_ON: &str = "THIS-ONE-DOES-NOT-PASS";

fn the_pump_seam() -> (LineSink, impl Future<Output = ()>) {
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    (sink, async move {
        let _ = pump.await;
    })
}

/// (a) i (b): pauza jest stanem **biegu**, żaden krok jej nie nosi, i nic za pytaniem nie ruszyło.
fn the_pause_sits_on_the_run(paused: &Json, watch: &Watch) -> Result<(), Box<dyn Error>> {
    let steps = paused
        .get("steps")
        .and_then(Json::as_array)
        .ok_or("run.json has no steps to look at")?;
    assert_eq!(
        steps.len(),
        3,
        "run.json has to describe all three steps while the run waits, not only the ones that \
         already ran"
    );

    let paused_steps: Vec<&Json> = steps
        .iter()
        .filter(|step| step.get("status").and_then(Json::as_str) == Some("paused"))
        .collect();
    assert!(
        paused_steps.is_empty(),
        "a step is carrying `\"status\": \"paused\"`: {paused_steps:?}. Pausing is a property of \
         the RUN and of nothing else — keeping it out of the step machine is what removes a whole \
         quadrant of states nobody needs (docs/ARCHITECTURE.md §5)"
    );

    assert_eq!(
        watch.times(BUILD),
        0,
        "the driver started `{BUILD}` {} time(s) while the run was still waiting for an answer. \
         A question that reaches the screen after the agent has done its work is not a question",
        watch.times(BUILD)
    );
    Ok(())
}

/// Czeka, aż `run.json` powie, że bieg stoi na punkcie kontrolnym; oddaje jego treść.
async fn wait_until_paused(project: &Path) -> Result<Json, Box<dyn Error>> {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(run) = only_run_dir(project).and_then(|dir| run_file(&dir))
            && run.get("status").and_then(Json::as_str) == Some("paused")
        {
            return Ok(run);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "within {PATIENCE:?} the run never wrote `\"status\": \"paused\"` into run.json. \
                 Either the checkpoint did not stop the run, or the run's state never reached \
                 disk — and a state that never reaches disk cannot be recovered after a crash \
                 either (invariant 4)"
            )
            .into());
        }
        tokio::time::sleep(EVERY).await;
    }
}

/// Jedyny katalog biegu pod `<projekt>/.loadout/runs/`, albo nic, kiedy jeszcze nie powstał.
fn only_run_dir(project: &Path) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(project.join(".loadout").join("runs"))
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    match dirs.as_slice() {
        [only] => Some(only.clone()),
        _ => None,
    }
}

/// `run.json` z katalogu biegu — albo nic, jeśli akurat nie da się go przeczytać w całości.
fn run_file(dir: &Path) -> Option<Json> {
    serde_json::from_str(&fs::read_to_string(dir.join("run.json")).ok()?).ok()
}

/// Fikstura ma przejść walidator **bez ani jednego problemu**, a jej plik agenta ma dać się
/// przeczytać.
///
/// To nie jest część kryterium, tylko jego przesłanka, i dlatego stoi przed biegiem. Czerwień
/// w fazie kontraktu wygląda identycznie dla „zachowania jeszcze nie ma" i dla „tego kryterium
/// nie da się spełnić nigdy": workflow, który `workflow::check` odrzuca, byłby odmową w KAŻDEJ
/// implementacji, a test nazywałby to brakiem zachowania.
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
        // `Store::open` zakłada plik bazy, ale nie katalog nad nim.
        fs::create_dir_all(project.path().join(".loadout"))?;
        Ok(Self { home, project })
    }

    fn agent(&self, slug: &str, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.home.path().join("agents").join(format!("{slug}.md"));
        fs::write(&path, text)?;
        Ok(path)
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
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { watch });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Obserwator sterownika: co ruszyło i ile razy.
#[derive(Debug, Default)]
struct Watch {
    runs: Mutex<Vec<String>>,
}

impl Watch {
    /// Krok wszedł do sterownika.
    ///
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym wywołaniu,
    /// więc nie ma wyrażenia, w którym dożyłby do `await`.
    fn entered(&self, prompt: &str) {
        self.lock().push(prompt.to_owned());
    }

    /// Ile razy sterownik ruszył krok, którego instrukcje niosą to słowo.
    ///
    /// `RunSpec` nie niesie numeru kroku — niesie jego instrukcje, i to jest jedyne pole, po
    /// którym da się kroki rozróżnić (niezmiennik 9: jadą tam jako **dane**).
    fn times(&self, word: &str) -> usize {
        self.lock()
            .iter()
            .filter(|prompt| prompt.contains(word))
            .count()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<String>> {
        self.runs.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Dubler sterownika: trzy zdarzenia na krok i wyjście zerem, natychmiast.
#[derive(Debug)]
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
        self.watch.entered(&spec.prompt);
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
        let _ = events
            .send(
                (AgentEvent::Said {
                    text: format!("working on {}", spec.prompt),
                })
                .into(),
            )
            .await;

        Ok(Box::new(Turn {
            events,
            session,
            fails: spec.prompt.contains(FAILS_ON),
        }))
    }
}

/// Jedna tura dublera.
#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    /// Czy ta tura ma skonczyc sie porazka — czytane z promptu przy starcie.
    fails: bool,
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
            ok: !self.fails,
            reason: if self.fails {
                FinishReason::Failed("the fixture was told to fail this one".to_owned())
            } else {
                FinishReason::Completed
            },
            text: String::new(),
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

/* 2026-08-23 — KROK, KTORY NIE PRZESZEDL, MOZE ZAPYTAC CZLOWIEKA.
 *
 * Zamowienie wlasciciela: „kafelek kontrolny gdzie moge zadecydowac czy ma przejsc z wynikiem do
 * kolejnego kroku np syntezy czy zapytac mnie co dalej". Do tego dnia nieudany krok kasowal caly
 * stozek potomkow — bez zdania i bez wyboru.
 *
 * SLABA WERSJA to „bieg sie zatrzymal". Przechodzi ja implementacja, ktora pyta i leci dalej
 * niezaleznie od odpowiedzi — czyli kontrolka bez skutku (niezmiennik 16). Rozroznia je punkt,
 * ze krok ZA nieudanym pobiegl DOPIERO PO odpowiedzi, sprawdzony po obu stronach tej chwili.
 *
 * DRUGI PUNKT jest rownie wazny: krok, ktory nie przeszedl, ma zostac CZERWONY takze wtedy, gdy
 * czlowiek kazal jechac dalej. Zielony blok nad robota, ktorej nikt nie przepuscil, jest ta jedna
 * rzecza, dla ktorej zapobiegania ten produkt powstal.
 */

/// Dwa kroki: pierwszy nie przechodzi i ma o to zapytac, drugi stoi za nim.
const ASK_WHEN_IT_FAILS: &str = r#"{
  "format": 1,
  "id": "01990000-0000-7000-8000-0000000000d1",
  "name": "Ask when it fails",
  "steps": [
    {
      "kind": "agent",
      "id": "s_try",
      "name": "Try it",
      "agent": "01990000-0000-7000-8000-00000000ab01",
      "overrides": {},
      "instructions": "THIS-ONE-DOES-NOT-PASS",
      "whenItFails": "ask-me",
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_after",
      "name": "After it",
      "agent": "01990000-0000-7000-8000-00000000ab01",
      "overrides": {},
      "instructions": "PICK-IT-UP-FROM-HERE",
      "at": { "x": 480, "y": 0 }
    }
  ],
  "links": [{ "from": "s_try", "to": "s_after" }]
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_step_set_to_ask_stops_the_run_and_the_answer_lets_it_through()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let hand = bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("ask-when-it-fails", ASK_WHEN_IT_FAILS)?;
    the_fixture_can_run(&workflow, &[&hand])?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::default());

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
        how_many_at_once: 3,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let (sink, drain) = the_pump_seam();
    let answer = async {
        let _paused = wait_until_paused(bench.project.path()).await?;
        /* PRZED ODPOWIEDZIA nastepny krok nie ma prawa byc uruchomiony. Bez tego punktu
         * kryterium przechodzi implementacja, ktora pyta i leci dalej, nie czekajac. */
        assert_eq!(
            watch.times("PICK-IT-UP-FROM-HERE"),
            0,
            "the step after the failed one started before anybody answered, so the question is \
             a control with no effect: it appears, and the run does what it would have done \
             anyway. The driver saw: {:?}",
            watch.lock().clone()
        );
        continue_run_inner(&deps, Some("carry on, ignore the third point".to_owned())).await?;
        Ok::<(), Box<dyn Error>>(())
    };

    let (ran, answered, ()) = tokio::time::timeout(PATIENCE.saturating_mul(3), async {
        tokio::join!(run_workflow_inner(&deps, &request, sink), answer, drain)
    })
    .await
    .map_err(|_| {
        "the run never came back. A step set to ask has to STOP the run and wait - a run that \
         sails past the question has no question"
            .to_owned()
    })?;
    answered?;
    let report = ran?;

    assert_eq!(
        watch.times("PICK-IT-UP-FROM-HERE"),
        1,
        "the step after the failed one never ran, even though the person answered and said carry \
         on. That is the dead end the whole setting exists to remove. The driver saw: {:?}",
        watch.lock().clone()
    );
    assert_eq!(
        report.steps.first().copied(),
        Some(StepState::Failed),
        "the step that did not pass came back as something other than failed. Carrying on is a \
         decision about the work AFTER it, never a claim that it succeeded - and a filled block \
         over work nobody passed is the one lie this product exists to prevent"
    );
    Ok(())
}
