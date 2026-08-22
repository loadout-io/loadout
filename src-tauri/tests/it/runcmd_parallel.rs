//! AC-4 dla T-15: liczba „ile naraz" z interfejsu dociera do semafora i przekłada się na
//! **nakładanie się w czasie**.
//!
//! Niezmiennik 11 łamie się cicho nie w planiście, tylko tutaj: liczba z UI jest wczytywana,
//! logowana i nigdzie nie podawana, a semafor dostaje `1`. poprzedni prototyp miał `max_parallel`, miał
//! zielone testy i **nigdy nie uruchomił dwóch agentów naraz** — cztery „równoległe" pasy
//! przebiegły w rozłącznych oknach po ~0,5 s, a każdy test przechodził, bo wszyscy agenci
//! rzeczywiście skończyli (`docs/handoff.md:144-165`).
//!
//! **Słaba wersja brzmi `assert_eq!(finished.len(), 4)` albo pomiar samego czasu całości.** Oba
//! przechodzą dla `run_ready(1)` w pętli. Rozróżnia je wyłącznie **przecięcie przedziałów**:
//! policzona na osi zdarzeń maksymalna liczba jednocześnie otwartych uruchomień, z asercją na
//! **obie** strony — górną (limit działa) i dolną (limit nie jest fikcją).
//!
//! Jedna fikstura, dwie wartości w żądaniu, dwa różne wyniki. To jest cały dowód, że liczba
//! przychodzi z żądania, a nie ze stałej w kodzie: stała nie zaspokoi obu biegów naraz.
//!
//! Runtime jest **wielowątkowy z prawdziwymi snami**, nigdy `start_paused`: czas wirtualny
//! implikuje runtime jednowątkowy i przeskakuje do przodu, kiedy runtime staje bezczynny, więc
//! „nakładanie się" przestaje cokolwiek znaczyć [T7 §8.1].

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
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Ile trwa każdy z czterech kroków.
const STEP: Duration = Duration::from_millis(200);

/// Ile kroków ma fikstura.
const STEPS: usize = 4;

/// Dolna granica czasu biegu jeden-po-drugim. W połowie między 200 ms (wszystko naraz)
/// a 800 ms (jeden po drugim), więc nie rozstrzyga się na styk i nie zależy od tego, jak szybko
/// maszyna wystartuje kolejne zadanie.
const ONE_AT_A_TIME_TAKES: Duration = Duration::from_millis(500);

/// Ile czekamy, zanim uznamy bieg za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(20);

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000e1
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

/// Cztery kroki i **ani jednej strzałki** — nic poza limitem nie ustala tu kolejności.
///
/// Każdy pracuje na własnej kopii plików nie dla ozdoby: cztery kroki, które mogą biec
/// równocześnie w folderze projektu, są odmową przy zapisie (niezmiennik 12), więc bez tego ta
/// fikstura nie doszłaby nawet do planisty.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_four_loose",
  "name": "Four loose steps",
  "steps": [
    {
      "kind": "agent",
      "id": "s_one",
      "name": "One",
      "agent": "01990000-0000-7000-8000-0000000000e1",
      "overrides": {},
      "instructions": "one",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_two",
      "name": "Two",
      "agent": "01990000-0000-7000-8000-0000000000e1",
      "overrides": {},
      "instructions": "two",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_three",
      "name": "Three",
      "agent": "01990000-0000-7000-8000-0000000000e1",
      "overrides": {},
      "instructions": "three",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 240 }
    },
    {
      "kind": "agent",
      "id": "s_four",
      "name": "Four",
      "agent": "01990000-0000-7000-8000-0000000000e1",
      "overrides": {},
      "instructions": "four",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 240 }
    }
  ],
  "links": []
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_at_once_means_the_windows_never_touch() -> Result<(), Box<dyn Error>> {
    let run = four_loose_steps(1).await?;

    assert_eq!(
        most_at_once(&run.windows),
        1,
        "the request asked for one step at a time and {} were inside the driver together; the \
         windows were {:?}",
        most_at_once(&run.windows),
        spans(&run.windows)
    );
    assert!(
        run.took >= ONE_AT_A_TIME_TAKES,
        "four steps of {STEP:?} one after another cannot take {:?}; that is the whole run \
         finishing in about the time of a single step, which means the limit bounded nothing",
        run.took
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_at_once_means_exactly_three_share_a_window() -> Result<(), Box<dyn Error>> {
    let run = four_loose_steps(3).await?;
    let peak = most_at_once(&run.windows);

    // Obie strony naraz, bo każda sama w sobie przechodzi dla czegoś innego: górna dla biegu,
    // który ignoruje limit, dolna dla biegu, który wysyła szeroko i wykonuje po jednym.
    assert_eq!(
        peak,
        3,
        "the request asked for three at once and the driver saw {peak} overlapping windows out \
         of {STEPS} steps. More than three means the number never reached the semaphore; fewer \
         means \"how many at once\" is only dispatch width — the defect that let poprzedni prototyp \
         report parallelism it never had (invariant 11). The windows were {:?}",
        spans(&run.windows)
    );
    Ok(())
}

/// Jeden bieg czterech niezależnych kroków przy zadanym limicie.
struct Measured {
    /// Okno każdego uruchomienia sterownika: wejście i wyjście.
    windows: Vec<(Instant, Instant)>,
    /// Ile trwał cały bieg.
    took: Duration,
}

/// Ten sam graf, ta sama fikstura — różni je **wyłącznie** liczba w żądaniu.
async fn four_loose_steps(how_many_at_once: usize) -> Result<Measured, Box<dyn Error>> {
    let bench = Bench::new()?;
    let hand = bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("four-loose", WORKFLOW)?;
    the_fixture_can_run(&workflow, &[&hand])?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::default());

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch), STEP),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once,
        task: None,
        only: None,
        handoffs_from: None,
    };

    // 2026-08-17 (T-30) — bieg oddaje linie POJEDYNCZO do `LineSink`, a sklejaniem zajmuje się
    // pompa po drugiej stronie, więc kanał zakłada się tutaj tak, jak zakłada go komenda:
    // `line_channel` + `spawn_pump`. Zmieniła się wyłącznie konstrukcja kanału przy wywołaniu;
    // ani jedna asercja tego kryterium nie wie o tej zmianie, bo nakładanie się kroków w czasie
    // mierzy obserwator sterownika, a nie wiersze. Kanał do okna jest czarną dziurą z tego
    // samego powodu.
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    // Pompa kończy się sama, kiedy zniknie ostatni nadajnik — a ten ginie razem z powrotem
    // biegu. Czekanie na nią zostaje w `join!` dokładnie tam, gdzie stało osuszanie kanału.
    let drain = async move {
        let _ = pump.await;
    };

    let began = Instant::now();
    let (ran, ()) = tokio::time::timeout(PATIENCE, async {
        tokio::join!(run_workflow_inner(&deps, &request, sink), drain)
    })
    .await
    .map_err(|_| format!("the run did not finish within {PATIENCE:?}"))?;
    let took = began.elapsed();
    let report = ran?;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; STEPS],
        "all four steps have to finish for the measured windows to mean anything; they ended \
         as {:?}",
        report.steps
    );

    let windows = watch.windows();
    assert_eq!(
        windows.len(),
        STEPS,
        "the driver closed {} window(s) out of {STEPS}; an unclosed window silently lowers the \
         overlap count, so the measurement would understate exactly what it is here to catch",
        windows.len()
    );
    Ok(Measured { windows, took })
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
    // zaczyna się następne, nie jest nakładaniem się — a bez tej reguły dwa sąsiednie kroki na
    // zegarze o rozdzielczości milisekundy potrafią wyglądać jak równoległe.
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

/// Fikstura ma przejść walidator **bez ani jednego problemu**, a jej plik agenta ma dać się
/// przeczytać.
///
/// To nie jest część kryterium, tylko jego przesłanka, i dlatego stoi przed biegiem. Czerwień
/// w fazie kontraktu wygląda identycznie dla „zachowania jeszcze nie ma" i dla „tego kryterium
/// nie da się spełnić nigdy": cztery kroki bez strzałek w folderze projektu są odmową przy
/// zapisie (niezmiennik 12), więc bez własnych kopii ta fikstura nie doszłaby nawet do planisty.
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

/// Biblioteka użytkownika i projekt na czas jednego biegu.
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

/// Jedno uruchomienie sterownika.
#[derive(Debug)]
struct Ran {
    from: Instant,
    to: Option<Instant>,
}

/// Obserwator sterownika: okno każdego uruchomienia.
///
/// Wejście zapisuje `start`, a wyjście — koniec tury, **przed** oddaniem permitu przez planistę.
/// Zapisane okna leżą więc w środku okien permitów, nigdy poza nimi: pomiar może zaniżyć
/// nakładanie się, ale nie może go zmyślić.
#[derive(Debug, Default)]
struct Watch {
    runs: Mutex<Vec<Ran>>,
}

impl Watch {
    /// Krok wszedł do sterownika; oddaje numer wpisu, po którym zamknie się jego okno.
    ///
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym wywołaniu,
    /// więc nie ma wyrażenia, w którym dożyłby do `await`. Wersja `log.lock().push(mark);
    /// sleep(d).await` zakleszcza bieg przy limicie większym niż jeden i wygląda jak zawieszony
    /// agent, nie jak błąd blokady.
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
        // Zatruty zamek nie ma prawa zgubić pomiaru: panika w jednym kroku oślepiłaby asercję,
        // która akurat dowodzi, że pozostałe biegły naraz.
        self.runs.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Dubler sterownika: trzy zdarzenia na krok i tura o mierzalnej długości.
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
