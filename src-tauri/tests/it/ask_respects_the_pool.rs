//! AC-2 dla T-62: `/ask` nie omija ani puli, ani Stopu.
//!
//! To jest cicha porażka, przed którą stoi całe zadanie. Bieg jednokrokowy, który robi sobie
//! miejsce sam, wygląda jak wygoda („to tylko jeden agent") i znaczy, że `atOnce` przestaje być
//! prawdą o maszynie: człowiek ustawia trzech, a pracuje piątka, bo dwa `/ask` przeszły bokiem
//! (niezmiennik 11). Nic w tym nie krzyczy — po prostu laptop staje, a rachunek rośnie.
//!
//! # Słaba wersja tego kryterium: test wyłącznie na Stopie
//!
//! Stop działa także dla biegu uruchomionego POZA pulą: token jest tego biegu, dowód zejścia
//! grupy przychodzi z uchwytu, wszystko wygląda dobrze — i `atOnce` dalej jest nieprawdą.
//! Rozstrzyga pierwszy przypadek: `/ask` i bieg z pliku dostają **ten sam** uchwyt puli, a
//! okna ich kroków są liczone na JEDNEJ osi czasu. Miejsce zajęte przez plik ma zatrzymać
//! agenta z `/ask`, i odwrotnie.
//!
//! # Dlaczego drugi przypadek jest kontrolą dodatnią, a nie powtórką
//!
//! Sam próg „nie więcej niż jeden naraz" przechodzi też wtedy, kiedy nie nakłada się NIC — a
//! bieg, w którym nic nigdy nie idzie obok niczego, to dokładnie poprzedni prototyp z jego
//! `max_parallel`: cztery „równoległe" pasy w rozłącznych oknach po ~0,5 s. Więc ta sama
//! fikstura, ten sam kod i te same dwa biegi przy puli **dwóch** muszą pokazać szczyt większy
//! niż jeden. Jedna stała nie zaspokoi obu przypadków.
//!
//! # (c): drugie `/ask`, zanim pierwsze zeszło
//!
//! `AppState::begin_run` podmienia uchwyt żywego biegu **bezwarunkowo**, i dla Startu z płótna
//! jest to w porządku, bo okno ma zapadkę (`src/sections/run/io.ts`, `going`). `/ask` tej
//! zapadki nie ma i mieć nie może — to jedna linia w wierszu wejścia — więc podmiana w trakcie
//! biegu znaczy, że Stop sięga do biegu DRUGIEGO, a pierwszy pracuje dalej i dalej płaci.
//!
//! TASK.md dopuszcza dwie odpowiedzi („albo czeka, albo odmawia zdaniem"). Ten kontrakt wybiera
//! ODMOWĘ i wybór jest świadomy: czekanie w tym miejscu trzymałoby zamek na uchwycie przez cały
//! poprzedni bieg, czyli zawieszałoby Stop dokładnie wtedy, kiedy Stop jest do czegokolwiek
//! potrzebny. Kryterium na „czeka" musiałoby zawiesić się razem z nim, a bieg, który wisi, jest
//! dla bramki „nie uruchomiło się", nie czerwienią.
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
use loadout_lib::commands::run::{
    AskRequest, run_agent_with_slots, run_workflow_with_slots, stop_run_inner,
};
use loadout_lib::commands::{Drivers, Outcome, RunControl, RunDeps, RunRequest};
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

/// Jak długo krok trzyma miejsce. Rzędy wielkości ponad koszt wzięcia permitu, żeby próg nie
/// zależał od tego, jak szybko maszyna wystartuje kolejne zadanie.
const STEP: Duration = Duration::from_millis(200);

/// Ile kroków w tej fiksturze próbuje ruszyć: jeden z `/ask`, jeden z pliku.
const TRYING: usize = 2;

/// Pula w przypadku właściwym. Musi być MNIEJSZA niż [`TRYING`], inaczej mierzymy limiter,
/// który nie ma czego ograniczać — o to pyta asercja (d).
const TOGETHER: usize = 1;

/// Pula w kontroli dodatniej. Tyle, ile kroków próbuje ruszyć, żeby ta sama fikstura mogła
/// pokazać szczyt większy niż [`TOGETHER`].
const ROOMY: usize = TRYING;

/// Ile czekamy, zanim uznamy biegi za zawieszone. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(20);

/// Ile czekamy, aż krok w ogóle wejdzie do sterownika, zanim naciśniemy Stop.
///
/// HOJNE Z ROZMYSŁEM i niczego nie osłabia: ta bariera jest PRZYGOTOWANIEM, a przypadek mierzy
/// KOLEJNOŚĆ dwóch chwil. Krótki limit na barierze zamienia obciążoną maszynę w oskarżenie
/// poprawnego kodu.
const START_LIMIT: Duration = Duration::from_secs(10);

/// Odstęp między pytaniami bariery. Krótki, bo mierzymy kolejność, a nie czas.
const POLL: Duration = Duration::from_millis(5);

/// Tura, która nie kończy się sama. Stop jest jedyną drogą wyjścia, więc dowód nie ma jak
/// przyjść „przy okazji".
///
/// `from_hours`, nie `from_secs(3_600)`: ta sama wartość, a `clippy::duration_suboptimal_units`
/// pod `-D warnings` nie przepuszcza tej drugiej formy przez `full-clippy`. Zmiana zapisu
/// jednej stałej fikstury, ani jednej asercji.
const NEVER_ENDS: Duration = Duration::from_hours(1);

/// Identyfikator agenta w fiksturze.
const HAND_ID: &str = "01990000-0000-7000-8000-0000000000d1";

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

/// Jeden krok z pliku — druga strona pytania „czy to ta sama pula".
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_one_step",
  "name": "One step from a file",
  "steps": [
    {
      "kind": "agent",
      "id": "s_hand",
      "name": "Hand",
      "agent": "01990000-0000-7000-8000-0000000000d1",
      "overrides": {},
      "instructions": "do the work",
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

/* ASERCJA NA STAŁYCH JEST TU CAŁYM SENSEM ASERCJI (d), więc `clippy::assertions_on_constants`
 * dostaje wyjątek zamiast racji. Ta jedna linia pilnuje FIKSTURY, nie kodu produkcyjnego:
 * pytanie „czy pula naprawdę ma sufit mniejszy niż liczba kroków" jest pytaniem o dwie stałe
 * z tego pliku i o nic więcej. Podpowiadane `const { assert!(…) }` nie przyjmuje sformatowanego
 * komunikatu — formatowania nie da się zawołać w kontekście stałym — a komunikat jest tu
 * połową wartości: bez niego czerwień mówi „false", nie mówi, co w fiksturze przestało wiązać.
 *
 * Wyciszenie stoi w `src-tauri/tests/`, więc nie ma jak ukryć niczego w kodzie produkcyjnym
 * (`checks/quick-suppressions.sh` czyta `src/` i `src-tauri/src/`, i tylko je). Zapisane
 * 2026-08-20. */
#[allow(clippy::assertions_on_constants)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_ask_and_a_file_run_over_one_pool_never_go_beside_each_other()
-> Result<(), Box<dyn Error>> {
    // (d) KONTROLA NAD SAMĄ FIKSTURĄ, zanim cokolwiek zmierzymy: pula musi mieć sufit mniejszy
    //     niż liczba kroków, które próbują ruszyć. Inaczej „nie przekroczyło limitu" jest
    //     zdaniem o limicie, którego nikt nie dotknął.
    assert!(
        TOGETHER < TRYING,
        "this fixture hands out a pool of {TOGETHER} to {TRYING} step(s), so the ceiling is not \
         binding and the measurement below would pass for a run with no limiter at all"
    );

    let windows = an_ask_beside_a_file_run(TOGETHER).await?;
    let peak = most_at_once(&windows);

    // Obie strony naraz, bo każda sama w sobie przechodzi dla czegoś innego: górna dla biegów,
    // które dzielą pulę, dolna dla implementacji, która nie uruchamia niczego.
    assert_eq!(
        peak,
        TOGETHER,
        "{peak} step(s) were inside the driver at the same moment across the /ask run and the \
         run from a file, and the pool they were both handed says {TOGETHER}. More than that \
         means the one-step run made itself a place: `at once` stops being true about the \
         machine, because a person sets three and five are working (invariant 11). Fewer would \
         mean nothing ran at all. The windows were {:?}",
        spans(&windows)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_same_two_do_overlap_when_the_pool_is_bigger() -> Result<(), Box<dyn Error>> {
    let windows = an_ask_beside_a_file_run(ROOMY).await?;
    let peak = most_at_once(&windows);

    assert!(
        peak > TOGETHER,
        "at a shared pool of {ROOMY} the same two runs peaked at {peak} overlapping window(s), \
         so the ceiling asserted in the other case says nothing: a measurement that can only \
         ever report {TOGETHER} would be satisfied while nothing in this application ever ran \
         beside anything else — which is precisely what the earlier prototype's max_parallel did \
         (invariant 11). The windows were {:?}",
        spans(&windows)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stop_takes_an_ask_down_and_returns_only_with_the_proof() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::default());
    let deps = bench.deps(&store, &watch, NEVER_ENDS);
    let ask = AskRequest {
        agent: HAND_ID.to_owned(),
        task: "keep going until somebody says stop".to_owned(),
        how_many_at_once: ROOMY,
    };
    let (sink, drain) = the_pump_seam();

    let (ran, pressed, ()) = tokio::time::timeout(PATIENCE, async {
        tokio::join!(
            run_agent_with_slots(&deps, &ask, sink, Limiter::new(ROOMY)),
            async {
                // Stop naciśnięty przed startem kroku mierzyłby uchwyt biegu, który jeszcze nic
                // nie prowadzi — czyli nie to pytanie.
                watch.wait_until_working(START_LIMIT).await?;
                let outcome = stop_run_inner(&deps).await;
                Ok::<(Instant, Result<Outcome, _>), String>((Instant::now(), outcome))
            },
            drain,
        )
    })
    .await
    .map_err(|_| format!("the run and Stop did not both finish within {PATIENCE:?}"))?;

    let report = ran?;
    let (stop_returned_at, stopped) = pressed?;
    assert_eq!(
        stopped?,
        Outcome::Cancelled,
        "Stop has to come back with a VALUE, never `Err(Cancelled)` (invariant 7): a caller \
         forced to tell \"this failed\" from \"a person stopped it\" loses that difference once \
         and loses it everywhere"
    );

    assert_eq!(
        report.steps,
        vec![StepState::Cancelled],
        "a step taken down by Stop is `cancelled`, never `failed` and never `skipped`: \
         `skipped` means somebody above fell over and the screen would lie about the reason \
         (ARCHITECTURE §5). It ended as {:?}",
        report.steps
    );
    assert_eq!(
        report.outcome,
        Outcome::Cancelled,
        "stopping is a value, not an error (invariant 7), and this run reported {:?}",
        report.outcome
    );

    // DOWÓD, I DOPIERO POTEM POWRÓT. To jest asercja, której nie przechodzi `tokio::time::
    // timeout` wokół kroku: tamto anuluje zadanie Rusta i zostawia żywego agenta palącego
    // limit u dostawcy (niezmienniki 6 i 10).
    let proven_at = watch
        .proved()
        .ok_or("nothing ever asked the handle to go down, so no group death was ever proven")?;
    assert!(
        stop_returned_at >= proven_at,
        "Stop returned {:?} BEFORE the group was proven down. Until `kill(-pgid, 0)` answers \
         ESRCH the group is alive (invariant 6): the screen says \"stopped\", the agent keeps \
         writing and keeps paying",
        proven_at.saturating_duration_since(stop_returned_at)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_second_ask_before_the_first_is_down_does_not_orphan_it() -> Result<(), Box<dyn Error>> {
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
        .begin_a_run(bench.project.path())
        .map_err(|said| format!("the first /ask was turned down with nothing going: {said}"))?;
    assert!(
        !first.control.is_working(),
        "a new run has to get a FRESH handle. A handle that already reports working is one that \
         has already settled, and a settled handle can never prove anything down again — Stop \
         would return instantly while the agent keeps going"
    );
    first.control.begin();
    assert!(
        first.control.is_working(),
        "the handle handed to the first /ask does not report the run it is leading, so nothing \
         below can tell whose handle is live"
    );

    // `let … else`, nie `match`: ta sama asercja co do znaku (drugie `/ask`, które DOSTAŁO
    // uchwyt, przewraca ten przypadek zdaniem niżej), tylko w formie, którą `-D warnings`
    // przepuszcza — `clippy::manual_let_else` odrzuca `match` z ramieniem wychodzącym.
    let Err(said) = state.begin_a_run(bench.project.path()) else {
        return Err(
            "a second /ask took a handle while the first run was still going. That \
                        swap is silent and it costs money: Stop reaches the second run, the \
                        first keeps writing and keeps paying, and nobody holds its token any \
                        more (invariants 6 and 11)"
                .into(),
        );
    };
    assert!(
        said.trim().len() > 20,
        "the refusal has to be a sentence that names the next move (DESIGN §8), and it said: \
         {said:?}"
    );

    // TO JEST TA ASERCJA, KTÓREJ KLON W RĘKU NIE ZASTĄPI: implementacja, która odmawia i JEDNAK
    // podmienia uchwyt, przechodzi wszystko powyżej. Żywym uchwytem jest dalej ten pierwszy.
    assert!(
        state.deps().control.is_working(),
        "after the refused second /ask the live handle no longer belongs to the run that is \
         going, so Stop has nothing to stop and the first agent is orphaned"
    );

    // KONTROLA DODATNIA: odmowa, która odmawia zawsze, spełniłaby wszystko wyżej i zamieniłaby
    // `/ask` w komendę, której nie da się użyć dwa razy w jednej sesji pracy.
    first.control.settle();
    state.begin_a_run(bench.project.path()).map_err(|said| {
        format!("the first run is down and a new /ask was still refused: {said}")
    })?;
    Ok(())
}

/// Jedno `/ask` obok jednego biegu z pliku, nad **jedną** pulą miejsc; oddaje okna obu kroków.
///
/// Pula wchodzi do obu biegów argumentem, klonem tego samego uchwytu — to jest cały mechanizm
/// „jeden semafor na całą aplikację" (`engine::limits`). Każdy bieg dostaje własny folder, bo
/// dwa biegi w jednym katalogu kolidowałyby na plikach, a wtedy mierzylibyśmy kolizję, nie limit.
async fn an_ask_beside_a_file_run(
    at_once: usize,
) -> Result<Vec<(Instant, Instant)>, Box<dyn Error>> {
    let slots = Limiter::new(at_once);
    // JEDEN obserwator na oba biegi: okna liczone osobno w każdym z nich nie odpowiadają na
    // pytanie tego kryterium, choćby były policzone bezbłędnie.
    let watch = Arc::new(Watch::default());

    let asked = Bench::new()?;
    let from_file = Bench::new()?;
    let asked_store = Store::open(&asked.db())?;
    let file_store = Store::open(&from_file.db())?;
    let workflow = from_file.workflow(WORKFLOW)?;
    the_fixture_can_run(&workflow)?;

    let asked_deps = asked.deps(&asked_store, &watch, STEP);
    let file_deps = from_file.deps(&file_store, &watch, STEP);
    let ask = AskRequest {
        agent: HAND_ID.to_owned(),
        task: "say what this folder holds".to_owned(),
        how_many_at_once: at_once,
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: at_once,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (ask_sink, ask_drain) = the_pump_seam();
    let (file_sink, file_drain) = the_pump_seam();

    let (asked_ran, file_ran, (), ()) = tokio::time::timeout(PATIENCE, async {
        tokio::join!(
            run_agent_with_slots(&asked_deps, &ask, ask_sink, slots.clone()),
            run_workflow_with_slots(&file_deps, &request, file_sink, slots.clone()),
            ask_drain,
            file_drain,
        )
    })
    .await
    .map_err(|_| format!("the two runs did not both finish within {PATIENCE:?}"))?;

    for report in [asked_ran?, file_ran?] {
        assert_eq!(
            report.steps,
            vec![StepState::Succeeded],
            "both runs have to finish for the measured windows to mean anything; one ended as \
             {:?}",
            report.steps
        );
    }

    let windows = watch.windows();
    assert_eq!(
        windows.len(),
        TRYING,
        "the driver closed {} window(s) out of {TRYING}; an unclosed window silently lowers the \
         overlap count, so the measurement would understate exactly what it is here to catch",
        windows.len()
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

/// Fikstura pliku ma przejść walidator **bez ani jednego problemu**.
///
/// To nie jest część kryterium, tylko jego przesłanka. Czerwień w fazie kontraktu wygląda
/// identycznie dla „zachowania jeszcze nie ma" i dla „tego kryterium nie da się spełnić nigdy":
/// workflow, który `workflow::check` odrzuca, byłby odmową w KAŻDEJ implementacji.
fn the_fixture_can_run(workflow: &Path) -> Result<(), Box<dyn Error>> {
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
        let bench = Self { home, project };
        let hand = bench.home.path().join("agents").join("hand.md");
        fs::write(&hand, HAND_FILE)?;
        // PRZESŁANKA, NIE ASERCJA: definicja, której nie da się przeczytać, byłaby odmową
        // w KAŻDEJ implementacji.
        read_agent_file(&hand).map_err(|error| format!("{}: {error}", hand.display()))?;
        Ok(bench)
    }

    fn workflow(&self, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.home.path().join("workflows").join("one-step.json");
        fs::write(&path, text)?;
        Ok(path)
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }

    fn deps<'a>(&'a self, store: &'a Store, watch: &Arc<Watch>, hold: Duration) -> RunDeps<'a> {
        RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store,
            drivers: fake_drivers(Arc::clone(watch), hold),
            // Własny uchwyt Stop/Continue na bieg. Wspólny byłby drugim wspólnym stanem obok
            // puli, a wtedy nie dałoby się powiedzieć, który z nich ograniczył kroki.
            processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        }
    }
}

/// Szew, którym bieg mówi do okna: nadajnik dla biegu i czekanie na pompę.
///
/// Kanał jest tu czarną dziurą — to kryterium sądzi obserwatora sterownika, a nie wiersze.
fn the_pump_seam() -> (LineSink, impl Future<Output = ()>) {
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    (sink, async move {
        let _ = pump.await;
    })
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

/// Obserwator sterownika **obu biegów**: okno każdego uruchomienia, na jednej osi czasu, plus
/// chwila, w której uchwyt oddał dowód zejścia grupy.
///
/// Wejście zapisuje `start`, a wyjście — koniec tury, **przed** oddaniem miejsca do puli.
/// Zapisane okna leżą więc w środku okien miejsc, nigdy poza nimi: pomiar może zaniżyć
/// nakładanie się, ale nie może go zmyślić.
#[derive(Debug, Default)]
struct Watch {
    runs: Mutex<Vec<Ran>>,
    /// Kiedy `cancel()` oddało `GroupProof`. Jedna chwila, bo jeden krok schodzi raz.
    proof: Mutex<Option<Instant>>,
}

impl Watch {
    /// Krok wszedł do sterownika; oddaje numer wpisu, po którym zamknie się jego okno.
    ///
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym wywołaniu,
    /// więc nie ma wyrażenia, w którym dożyłby do `await`.
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

    /// Uchwyt oddał dowód, że grupa nie żyje.
    fn proof_came(&self) {
        self.proof
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_or_insert_with(Instant::now);
    }

    fn proved(&self) -> Option<Instant> {
        *self.proof.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Czeka, aż jakikolwiek krok wejdzie do sterownika.
    async fn wait_until_working(&self, patience: Duration) -> Result<(), String> {
        let until = Instant::now() + patience;
        while self.lock().is_empty() {
            if Instant::now() >= until {
                return Err(format!("no step reached the driver within {patience:?}"));
            }
            tokio::time::sleep(POLL).await;
        }
        Ok(())
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

/// Dubler sterownika: dwa zdarzenia na krok i tura o mierzalnej długości.
#[derive(Debug)]
struct Fake {
    watch: Arc<Watch>,
    hold: Duration,
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        "fake"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("fake".to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let entry = self.watch.entered();
        let session = SessionRef {
            vendor: "fake",
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
        // Dubler nie ma procesu, więc nie ma grupy. Zmyślony `pgid` byłby liczbą, po której
        // sprzątanie strzelałoby w cudzy proces.
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
        self.watch.proof_came();
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        self.watch.left(self.entry);
        Ok(Some(0))
    }
}
