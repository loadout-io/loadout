//! AC-1 dla T-94: dwa biegi w dwóch folderach biorą miejsca z **jednej** puli aplikacji.
//!
//! Czym to NIE jest, choć wygląda tak samo z daleka. `limits_are_global_across_runs.rs`
//! (T-31 AC-1) dowodzi tej własności dla `run_workflow_with_slots` — funkcji, której pulę
//! podaje się argumentem i która **nie ma produkcyjnego wołającego**. Zmierzone 2026-08-24 na
//! wyładowanym trunku: jedyną drogą, którą wchodzi tu okno, jest `run_workflow_inner`, a ta
//! robiła `Limiter::new(request.how_many_at_once)` na KAŻDY bieg. Kryterium tamtego zadania
//! przechodziło więc na typie, którego produkt nie używał — a dwie karty dawały `2 × limit`
//! agentów po ~583 MB, czyli zamrożony laptop, a nie szybszą pracę
//! (`docs/ARCHITECTURE.md` §6a, niezmiennik 11).
//!
//! **Rozstrzyga suma po OBU biegach w jednym oknie czasu**, policzona na wspólnej osi zdarzeń:
//! maksimum nachodzących na siebie przedziałów. Słaba wersja tego kryterium — sufit sprawdzony
//! w jednym biegu — przechodzi także dla puli zakładanej per bieg, bo tamta w jednym biegu
//! działa bez zarzutu.
//!
//! **Drugi przypadek jest kontrolą dodatnią.** Sam próg „nie więcej niż dwa" przechodzi także
//! wtedy, kiedy nie nakłada się nic, a bieg, w którym nic nigdy nie idzie obok niczego, to
//! dokładnie `max_parallel` z repo źródłowego: cztery „równoległe" pasy w rozłącznych oknach
//! po ~0,5 s (niezmiennik 11). Ta sama fikstura przy puli **4** musi pokazać szczyt większy
//! niż dwa.
//!
//! **Trzeci przypadek pyta o samą aplikację**, bo dwa poprzednie mogłyby przejść dla kodu,
//! który bierze pulę z uchwytu biegu, a aplikacja i tak wręczałaby każdemu Startowi świeżą.
//! Pula ma być JEDNA na `AppState` i przeżywać bieg — po to, żeby suwak z drugiego Startu
//! przestawiał ten sam limit, a nie zakładał drugą pulę obok pierwszej.
//!
//! Runtime jest **wielowątkowy z prawdziwymi snami**, nigdy `start_paused`: czas wirtualny
//! przeskakuje do przodu, kiedy runtime staje bezczynny, więc „nakładanie się" przestaje
//! cokolwiek znaczyć [T7 §8.1].

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::limits::Limiter;
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{AppState, LineSink, QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Jak długo krok trzyma miejsce. Rzędy wielkości ponad koszt wzięcia permitu, żeby próg nie
/// zależał od tego, jak szybko maszyna wystartuje kolejne zadanie.
const STEP: Duration = Duration::from_millis(200);

/// Ile kroków ma jeden bieg.
const STEPS_PER_RUN: usize = 3;

/// Ile biegów odpalamy naraz.
const RUNS: usize = 2;

/// Limit wspólny dla całej aplikacji w przypadku właściwym.
const TOGETHER: usize = 2;

/// Limit wspólny w kontroli dodatniej. Cztery, żeby ta sama fikstura mogła pokazać szczyt
/// większy niż [`TOGETHER`].
const ROOMY: usize = 4;

/// Liczba, na którą przestawiamy wspólny limit z drugiego Startu. Piątka, bo różni się i od
/// domyślnej trójki, i od obu wartości wyżej — wpisana przez pomyłkę byłaby nie do odróżnienia.
const MOVED_TO: usize = 5;

/// Ile czekamy, zanim uznamy biegi za zawieszone. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
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

/// Trzy kroki i **ani jednej strzałki** — nic poza limitem nie ustala tu kolejności.
///
/// Każdy pracuje na własnej kopii plików nie dla ozdoby: trzy kroki, które mogą biec równocześnie
/// w folderze projektu, są odmową przy zapisie (niezmiennik 12), więc bez tego ta fikstura nie
/// doszłaby nawet do planisty.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_three_loose",
  "name": "Three loose steps",
  "steps": [
    {
      "kind": "agent",
      "id": "s_one",
      "name": "One",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "one",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_two",
      "name": "Two",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "two",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_three",
      "name": "Three",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "three",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 240 }
    }
  ],
  "links": []
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_runs_in_two_folders_never_exceed_the_one_pool() -> Result<(), Box<dyn Error>> {
    let windows = two_runs_sharing(TOGETHER).await?;
    let peak = most_at_once(&windows);

    // Obie strony naraz, bo każda sama w sobie przechodzi dla czegoś innego: górna dla biegów,
    // które dzielą pulę, dolna dla implementacji, która nie zrównolegla w ogóle.
    assert_eq!(
        peak,
        TOGETHER,
        "{peak} steps were inside the agent app at the same moment across BOTH runs, and the \
         one pool this application hands out says {TOGETHER}. More than that means each run \
         built a pool of its own: two folders at two apiece is four agents at ~583 MB each, \
         which is a frozen laptop rather than faster work (ARCHITECTURE.md 6a, invariant 11). \
         Fewer than that would mean the limit bounded nothing, because nothing ever ran beside \
         anything else. The windows were {:?}",
        spans(&windows)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_same_two_runs_overlap_more_when_the_pool_is_bigger() -> Result<(), Box<dyn Error>> {
    let windows = two_runs_sharing(ROOMY).await?;
    let peak = most_at_once(&windows);

    assert!(
        peak > TOGETHER,
        "at a shared pool of {ROOMY} the same two runs peaked at {peak} overlapping windows, so \
         the ceiling asserted in the other case says nothing: a measurement that can only ever \
         report {TOGETHER} would be satisfied while nothing in this application ever ran beside \
         anything else. The windows were {:?}",
        spans(&windows)
    );
    Ok(())
}

/// Aplikacja wręcza KAŻDEMU startowi tę samą pulę, a nie świeżą na bieg.
///
/// Bez tego przypadku oba pomiary wyżej przechodzą dla kodu, który bierze pulę z uchwytu biegu,
/// podczas gdy aplikacja i tak zakłada nową przy każdym Starcie — czyli dla wady, która wygląda
/// na naprawioną wszędzie poza jedynym miejscem, w którym żyje.
#[tokio::test]
async fn the_application_hands_every_start_the_same_pool() -> Result<(), Box<dyn Error>> {
    let first_folder = TempDir::new()?;
    let second_folder = TempDir::new()?;
    let bench = Bench::new()?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::default());
    let state = AppState::new(
        bench.home.path().to_path_buf(),
        bench.project.path().to_path_buf(),
        store,
        fake_drivers(Arc::clone(&watch), STEP),
    );

    let first = state
        .begin_run(first_folder.path())
        .map_err(|said| format!("the first Start was turned down with nothing going: {said}"))?;
    let of_the_first = first.control.slots();
    // Suwak przestawiony w trakcie pierwszego biegu. Ta droga istnieje dziś i nie zmienia się
    // tutaj: obniżenie w dół nie zabija kroków, tylko zostaje długiem do spłacenia przy
    // zwalnianiu (`engine::limits::Pool::take_back`).
    of_the_first.set_at_once(MOVED_TO);
    first.control.settle();

    let second = state.begin_run(second_folder.path()).map_err(|said| {
        format!("the second Start was turned down after the first went down: {said}")
    })?;

    assert_eq!(
        second.control.slots().at_once(),
        MOVED_TO,
        "the second Start in a second folder was handed a pool that stands at {}, while the \
         pool the first Start moved stands at {MOVED_TO}. Two numbers means two pools, and two \
         pools is the whole defect: \"how many at once\" is a number about this MACHINE, so it \
         cannot start over every time somebody opens another folder (invariant 11)",
        second.control.slots().at_once()
    );
    Ok(())
}

/// Dwa biegi naraz nad **jedną** pulą aplikacji; oddaje okna wszystkich kroków obu biegów.
///
/// Ta sama fikstura i ten sam kod dla obu przypadków — różni je wyłącznie wielkość puli.
async fn two_runs_sharing(at_once: usize) -> Result<Vec<(Instant, Instant)>, Box<dyn Error>> {
    // JEDNA pula, dwa uchwyty biegu — dokładnie tak, jak wręcza je `AppState::begin_run`.
    // Klon dzieli tę samą pulę i to jest cały mechanizm; dwa razy `Limiter::new(at_once)` to
    // dwie pule, czyli dokładnie defekt, który tu mierzymy.
    let slots = Limiter::new(at_once);
    // JEDEN obserwator na oba biegi: okna liczone osobno w każdym biegu nie odpowiadają na
    // pytanie tego kryterium, choćby były policzone bezbłędnie.
    let watch = Arc::new(Watch::default());

    let first = Lane::new()?;
    let second = Lane::new()?;
    let deps_one = first.deps(&watch, &slots);
    let deps_two = second.deps(&watch, &slots);
    let request_one = first.request(at_once);
    let request_two = second.request(at_once);
    let (sink_one, drain_one) = the_pump_seam();
    let (sink_two, drain_two) = the_pump_seam();

    let (ran_one, ran_two, (), ()) = tokio::time::timeout(PATIENCE, async {
        tokio::join!(
            // PRODUKCYJNE DRZWI, nie `run_workflow_with_slots`: okno wchodzi tędy i tylko tędy,
            // więc pula podana bokiem dowiodłaby własności typu, a nie własności produktu.
            run_workflow_inner(&deps_one, &request_one, sink_one),
            run_workflow_inner(&deps_two, &request_two, sink_two),
            drain_one,
            drain_two,
        )
    })
    .await
    .map_err(|_| format!("the two runs did not both finish within {PATIENCE:?}"))?;

    for report in [ran_one?, ran_two?] {
        assert_eq!(
            report.steps,
            vec![StepState::Succeeded; STEPS_PER_RUN],
            "every step of both runs has to finish for the measured windows to mean anything; \
             one run ended as {:?}",
            report.steps
        );
    }

    let windows = watch.windows();
    assert_eq!(
        windows.len(),
        RUNS * STEPS_PER_RUN,
        "the agent app closed {} window(s) out of {}; an unclosed window silently lowers the \
         overlap count, so the measurement would understate exactly what it is here to catch",
        windows.len(),
        RUNS * STEPS_PER_RUN
    );
    Ok(windows)
}

/// Największa liczba okien otwartych **naraz**, policzona na osi zdarzeń.
///
/// Nie liczba uruchomień i nie czas całości: obie te liczby wychodzą tak samo dla biegu, który
/// wysyła szeroko i wykonuje po jednym.
fn most_at_once(windows: &[(Instant, Instant)]) -> usize {
    let mut marks: Vec<(Instant, i32)> = Vec::with_capacity(windows.len() * 2);
    for &(from, to) in windows {
        marks.push((from, 1));
        marks.push((to, -1));
    }
    // Zamknięcie przed otwarciem przy równym znaczniku: okno kończące się dokładnie wtedy, kiedy
    // zaczyna się następne, oddało mu swoje miejsce, a nie zajęło drugie.
    marks.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));

    let mut open = 0i32;
    let mut most = 0i32;
    for (_, delta) in marks {
        open += delta;
        most = most.max(open);
    }
    usize::try_from(most).unwrap_or(0)
}

/// Okna jako czasy trwania — czytelne w komunikacie asercji.
fn spans(windows: &[(Instant, Instant)]) -> Vec<Duration> {
    windows
        .iter()
        .map(|&(from, to)| to.saturating_duration_since(from))
        .collect()
}

/// Jeden bieg: własna biblioteka, własny folder, własny magazyn.
///
/// Osobny folder na bieg nie jest ozdobą: otwarcie folderu, który już ma kartę, przełącza na nią
/// zamiast zakładać drugą (`docs/ARCHITECTURE.md` §6a reguła 1), więc dwa biegi naraz to z
/// definicji dwa foldery. Wspólna jest **wyłącznie** pula miejsc i obserwator.
struct Lane {
    bench: Bench,
    store: Store,
    workflow: PathBuf,
}

impl Lane {
    fn new() -> Result<Self, Box<dyn Error>> {
        let bench = Bench::new()?;
        let hand = bench.agent("hand", HAND_FILE)?;
        let workflow = bench.workflow("three-loose", WORKFLOW)?;
        the_fixture_can_run(&workflow, &[&hand])?;
        let store = Store::open(&bench.db())?;
        Ok(Self {
            bench,
            store,
            workflow,
        })
    }

    fn deps<'a>(&'a self, watch: &Arc<Watch>, slots: &Limiter) -> RunDeps<'a> {
        RunDeps {
            home: self.bench.home.path(),
            project: self.bench.project.path(),
            store: &self.store,
            drivers: fake_drivers(Arc::clone(watch), STEP),
            processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
            /* UCHWYT BIEGU NIESIE PULĘ APLIKACJI — to jest ta jedna droga, którą pula dojeżdża
             * do biegu, i dokładnie ta, którą karmi ją `AppState::begin_run`. Uchwyt jest własny
             * na bieg, bo Stop sięga nim do środka JEDNEGO biegu; wspólna jest pula w środku. */
            control: RunControl::sharing(slots.clone()),
        }
    }

    fn request(&self, how_many_at_once: usize) -> RunRequest {
        RunRequest {
            workflow: self.workflow.clone(),
            how_many_at_once,
            task: None,
            part: None,
            handoffs_from: None,
        }
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
/// przeczytać.
///
/// To nie jest część kryterium, tylko jego przesłanka, i dlatego stoi przed biegiem. Czerwień
/// w fazie kontraktu wygląda identycznie dla „zachowania jeszcze nie ma" i dla „tego kryterium
/// nie da się spełnić nigdy".
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

/// Biblioteka użytkownika i folder pracy jednego biegu.
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

/// Jedno uruchomienie dublera.
#[derive(Debug)]
struct Ran {
    from: Instant,
    to: Option<Instant>,
}

/// Obserwator **obu biegów**: okno każdego uruchomienia, na jednej osi czasu.
///
/// Wejście zapisuje start, a wyjście — koniec tury, **przed** oddaniem miejsca do puli.
/// Zapisane okna leżą więc w środku okien miejsc, nigdy poza nimi: pomiar może zaniżyć
/// nakładanie się, ale nie może go zmyślić.
#[derive(Debug, Default)]
struct Watch {
    runs: Mutex<Vec<Ran>>,
}

impl Watch {
    /// Krok wszedł do dublera; oddaje numer wpisu, po którym zamknie się jego okno.
    fn entered(&self) -> usize {
        let mut runs = self.lock();
        runs.push(Ran {
            from: Instant::now(),
            to: None,
        });
        runs.len() - 1
    }

    /// Krok wyszedł, jakkolwiek się skończył. Pierwsze wyjście wygrywa.
    fn left(&self, entry: usize) {
        let mut runs = self.lock();
        if let Some(ran) = runs.get_mut(entry) {
            ran.to.get_or_insert_with(Instant::now);
        }
    }

    /// Domknięte okna. Okno bez końca nie wchodzi — i dlatego wołający sprawdza ich liczbę.
    fn windows(&self) -> Vec<(Instant, Instant)> {
        self.lock()
            .iter()
            .filter_map(|ran| Some((ran.from, ran.to?)))
            .collect()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Ran>> {
        self.runs.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Dubler: dwa zdarzenia na krok i tura o mierzalnej długości.
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
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let entry = self.watch.entered();
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
            watch: Arc::clone(&self.watch),
            events,
            session,
            entry,
            hold: self.hold,
        }))
    }
}

/// Jedna tura dublera.
#[derive(Debug)]
struct Turn {
    watch: Arc<Watch>,
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    entry: usize,
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
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: self.hold,
            session: self.session.clone(),
        };
        self.watch.left(self.entry);
        let _ = self
            .events
            .send((AgentEvent::Finished(outcome.clone())).into())
            .await;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> GroupProof {
        self.watch.left(self.entry);
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        self.watch.left(self.entry);
        Ok(Some(0))
    }
}
