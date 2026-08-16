//! AC-3 dla T-15: zatrzymanie kończy bieg jako anulowany i zabija to, co żyło.
//!
//! **Słaba wersja brzmi `assert!(stop_run_inner(...).is_ok())`** i przechodzi dla
//! `fn stop_run_inner() -> Ok(())`, które nie robi nic: bieg dobiega do końca sam, wszystko jest
//! zielone, a użytkownik patrzy na przycisk, który niczego nie zatrzymuje. Rozróżnia je (b) plus
//! (e) — **czas powrotu poniżej długości snu jest jedynym dowodem, że coś naprawdę przerwano**.
//!
//! Trzy rzeczy, które ten plik trzyma osobno, bo w kodzie łatwo je pomylić:
//!
//! - **Anulowanie jest wartością, nie błędem** (niezmiennik 7). `Err(Cancelled)` zmusza każdego
//!   wołającego do rozpakowywania błędu, który awarią nie jest, a stamtąd jest już tylko krok do
//!   potraktowania świadomego Stopu jak usterki.
//! - **Krok za anulowanym jest `cancelled`, nie `skipped`** (`docs/ARCHITECTURE.md` §5).
//!   `skipped` znaczy „ktoś wyżej padł" — pokazane po Stopie kłamie o powodzie.
//! - **Zdjęcie zadania Rusta to nie jest zabicie procesu** (niezmienniki 6 i 10). Dlatego (c)
//!   pyta o wywołanie **na sterowniku**, a nie o to, czy krok wrócił: `tokio::time::timeout`
//!   wokół kroku wygląda na limit czasu, jest o linijkę tańszy i zostawia żywego agenta palącego
//!   limit u dostawcy [T7 §3.1].

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use loadout_lib::commands::run::{run_workflow_inner, stop_run_inner};
use loadout_lib::commands::{Drivers, Outcome, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, FinishReason, Outcome as TurnOutcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Jak długo trwa długi krok. **Prawdziwy sen, nie czas wirtualny**: `start_paused` implikuje
/// runtime jednowątkowy i przeskakuje zegar do przodu, kiedy runtime staje bezczynny, więc
/// „wrócił szybciej niż sen" przestałoby cokolwiek znaczyć [T7 §8.1].
const LONG: Duration = Duration::from_secs(2);

/// Po ilu milisekundach naciskamy Stop.
const AFTER: Duration = Duration::from_millis(50);

/// Ile czekamy, zanim uznamy bieg za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(10);

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000d1
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

/// Jeden długi krok i jeden czekający za nim.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_long_then_after",
  "name": "One long step",
  "steps": [
    {
      "kind": "agent",
      "id": "s_long",
      "name": "Long",
      "agent": "01990000-0000-7000-8000-0000000000d1",
      "overrides": {},
      "instructions": "long",
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_after",
      "name": "After",
      "agent": "01990000-0000-7000-8000-0000000000d1",
      "overrides": {},
      "instructions": "after",
      "at": { "x": 240, "y": 0 }
    }
  ],
  "links": [{ "from": "s_long", "to": "s_after" }]
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stopping_a_run_cancels_it_and_kills_what_was_alive() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let hand = bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("long-then-after", WORKFLOW)?;
    the_fixture_can_run(&workflow, &[&hand])?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::default());

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch), LONG),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 2,
    };

    let (tx, mut rx) = mpsc::channel::<Vec<Line>>(64);
    let drain = async move { while rx.recv().await.is_some() {} };
    let stop = async {
        tokio::time::sleep(AFTER).await;
        stop_run_inner(&deps).await
    };

    let began = Instant::now();
    let (ran, stopped, ()) = tokio::time::timeout(PATIENCE, async {
        tokio::join!(run_workflow_inner(&deps, &request, tx), stop, drain)
    })
    .await
    .map_err(|_| format!("neither the run nor the stop came back within {PATIENCE:?}"))?;
    let took = began.elapsed();

    let report = ran?;

    // (e) najpierw, bo bez niego reszta asercji opisuje bieg, który po prostu doszedł do końca.
    assert!(
        took < LONG,
        "the run took {took:?}, and one step alone sleeps {LONG:?}: nothing was interrupted. \
         A Stop that lets the run finish on its own passes every assertion about final states \
         and stops nothing"
    );

    // (a) anulowanie jest WARTOŚCIĄ.
    assert_eq!(
        stopped?,
        Outcome::Cancelled,
        "stop_run_inner has to come back with Outcome::Cancelled — a value, never Err(Cancelled) \
         (invariant 7)"
    );
    assert_eq!(
        report.outcome,
        Outcome::Cancelled,
        "the run itself has to report that a person stopped it, not that it merely ended"
    );

    // (c) ktoś naprawdę zszedł po grupie procesów.
    assert!(
        watch.starts() >= 1,
        "the driver never started, so there was nothing alive to stop and this test measured \
         an empty run"
    );
    assert!(
        watch.kills() >= 1,
        "the driver's cancel was never called. Dropping the Rust task also returns quickly and \
         also looks cancelled — and leaves a live process group burning the vendor's limit, \
         because a step only comes down through the driver's own escalation \
         (invariants 6 and 10)"
    );

    // (b) i (d): dokładnie `cancelled`, nigdy `failed` ani `skipped`.
    assert_eq!(
        report.steps,
        vec![StepState::Cancelled, StepState::Cancelled],
        "the stopped step has to end as `cancelled`, and so does the one still waiting behind \
         it: `skipped` means \"someone upstream failed\" and would make the screen lie about \
         why nothing happened (docs/ARCHITECTURE.md §5). They ended as {:?}",
        report.steps
    );
    Ok(())
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
fn fake_drivers(watch: Arc<Watch>, hold: Duration) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { watch, hold });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Obserwator sterownika: ile razy ruszył i ile razy ktoś po nim zszedł.
#[derive(Debug, Default)]
struct Watch {
    starts: AtomicUsize,
    kills: AtomicUsize,
}

impl Watch {
    fn entered(&self) {
        self.starts.fetch_add(1, Ordering::SeqCst);
    }

    fn killed(&self) {
        self.kills.fetch_add(1, Ordering::SeqCst);
    }

    fn starts(&self) -> usize {
        self.starts.load(Ordering::SeqCst)
    }

    fn kills(&self) -> usize {
        self.kills.load(Ordering::SeqCst)
    }
}

/// Dubler sterownika: trzy zdarzenia na krok, długa tura i policzone zabicie.
#[derive(Debug)]
struct Fake {
    watch: Arc<Watch>,
    hold: Duration,
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
        events: mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.watch.entered();
        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };

        let _ = events
            .send(AgentEvent::Started {
                session: session.clone(),
                model: spec.model.clone().unwrap_or_default(),
                tools: Vec::new(),
                capabilities: Vec::new(),
            })
            .await;
        let _ = events
            .send(AgentEvent::Said {
                text: format!("working on {}", spec.prompt),
            })
            .await;

        Ok(Box::new(Turn {
            watch: Arc::clone(&self.watch),
            events,
            session,
            hold: self.hold,
        }))
    }
}

/// Jedna tura dublera.
#[derive(Debug)]
struct Turn {
    watch: Arc<Watch>,
    events: mpsc::Sender<AgentEvent>,
    session: SessionRef,
    hold: Duration,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        // Dubler nie ma procesu, więc nie ma grupy. Zmyślony `pgid` byłby liczbą, po której
        // sprzątanie z T-20 strzelałoby w cudzy proces.
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        // Sen nie ogląda się tu na żaden token i to jest cała treść tego dublera: krok kończy się
        // sam po `hold` albo dlatego, że ktoś zawołał `cancel`. Wersja zwijająca się na tokenie
        // sama w sobie zaliczałaby (e) i nie pytałaby o nic.
        tokio::time::sleep(self.hold).await;
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: String::new(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: self.hold,
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send(AgentEvent::Finished(outcome.clone()))
            .await;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> GroupProof {
        // Jedyny ślad odróżniający zejście po grupie procesów od zdjęcia zadania Rusta.
        self.watch.killed();
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}
