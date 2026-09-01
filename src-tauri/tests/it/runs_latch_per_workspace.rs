//! Zapadka „jeden bieg naraz" jest per FOLDER, a nie na całego Loadouta.
//!
//! # Co dokładnie było zepsute
//!
//! `AppState` trzymał JEDEN uchwyt biegu i pytał o niego bezwarunkowo, więc bieg w
//! `~/Projects/ledger` odmawiał startu w `~/Projects/atlas` zdaniem, które brzmiało tak, jakby
//! zajęty był cały produkt. Obietnicą tego produktu są agenci pracujący w TWOICH folderach —
//! w liczbie mnogiej — a jeden folder na całą aplikację jest tą obietnicą cofniętą w kodzie.
//!
//! # Dlaczego NAKŁADANIE SIĘ W CZASIE, a nie „drugi start został przyjęty"
//!
//! Niezmiennik 11: „ile naraz" musi znaczyć naraz. Repo źródłowe miało `max_parallel`, które
//! było wyłącznie szerokością wysyłki — cztery „równoległe" pasy w rozłącznych okienkach po
//! ~0,5 s. Kryterium pytające wyłącznie „czy drugi start wrócił bez odmowy" przechodzi dla
//! implementacji, która przyjmuje drugi start i **serializuje** oba biegi na jednym miejscu
//! w puli: z zewnątrz wygląda to jak naprawa, a jest tym samym defektem o warstwę niżej.
//! Dlatego pomiar leży na wspólnej osi czasu i pyta o CHWILĘ, w której pracują oba pasy naraz.
//!
//! # Dlaczego jeden krok na bieg w tym pomiarze
//!
//! Żeby nakładanie mogło być tylko MIĘDZY biegami, nigdy wewnątrz jednego. Bieg o trzech
//! luźnych krokach nakłada się sam ze sobą przy każdej puli szerszej niż jeden, więc szczyt
//! większy od jedynki nie odróżniałby dwóch biegów naraz od jednego biegu z dwoma krokami.
//!
//! # Zapadka to nie limiter i te dwie rzeczy nie mają prawa się zlać
//!
//! Zapadka odpowiada na „czyj uchwyt trzyma Stop", pula — na „ile kroków naraz na tej maszynie".
//! `the_one_pool_still_caps_both_folders` jest tu po to, żeby odklejenie zapadki od globalności
//! nie zabrało globalności puli po drodze: trzy kroki naraz przy suwaku na dwóch to dziewięć
//! agentów po ~583 MB przy trzech kartach, czyli zamrożony laptop, a nie szybsza praca
//! (`docs/ARCHITECTURE.md` §6a).
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
use loadout_lib::commands::{Drivers, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
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

/// Identyfikator zapisanego agenta, ten sam dla obu pasów.
const HAND: &str = "01990000-0000-7000-8000-0000000000f1";

/// Jak długo krok trzyma miejsce. Rzędy wielkości ponad koszt wzięcia miejsca w puli, żeby
/// nakładanie się nie zależało od tego, jak szybko maszyna wystartuje kolejne zadanie.
const STEP: Duration = Duration::from_millis(200);

/// Jak długo trzyma krok, który ma dożyć naciśnięcia Stopu.
const LONG_STEP: Duration = Duration::from_secs(5);

/// Suwak „ile naraz" w obu biegach. Dwa, bo dwa pasy po jednym kroku mają się zmieścić razem.
const TOGETHER: usize = 2;

/// Ile czekamy, zanim uznamy biegi za zawieszone. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(30);

/// Znak pasa pierwszego. Jedzie w instrukcji kroku, więc dubler czyta go z promptu i wie,
/// którego biegu okno właśnie zamyka.
const LEDGER: &str = "workledger";

/// Znak pasa drugiego.
const ATLAS: &str = "workatlas";

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

/// (AC-1) Dwa foldery mają swój bieg w tej samej chwili — i widać to na osi czasu.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_workspaces_run_at_the_same_time() -> Result<(), Box<dyn Error>> {
    let windows = two_folders_running(1, STEP, None).await?;

    let ledger = only_window(&windows, LEDGER)?;
    let atlas = only_window(&windows, ATLAS)?;

    // Przecięcie przedziałów, nie „drugi zaczął się przed końcem pierwszego": obie strony naraz,
    // bo pytanie brzmi „czy istniała chwila, w której pracowały oba", a nie „który był pierwszy".
    assert!(
        ledger.0 < atlas.1 && atlas.0 < ledger.1,
        "the two folders each closed a window, and those windows do not touch: one folder \
         worked from {:?} for {:?}, the other started {:?} later. Two folders taking turns in \
         two windows half a second apart is exactly the shape this application was built to \
         replace (invariant 11) — \"how many at once\" has to mean at once. The whole latch is \
         keyed by folder for this one reason.",
        ledger.0,
        ledger.1.saturating_duration_since(ledger.0),
        atlas.0.saturating_duration_since(ledger.0),
    );
    Ok(())
}

/// (AC-1) Ten sam suwak dalej trzyma sufit SUMY kroków obu folderów.
///
/// Kontrola przeciw naprawie, która odkleja zapadkę od globalności i zabiera po drodze
/// globalność puli. Trzy kroki na pas i suwak na dwóch: bez wspólnej puli szczyt wyszedłby
/// czterema, bo każdy bieg miałby własne dwa miejsca.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_one_pool_still_caps_both_folders() -> Result<(), Box<dyn Error>> {
    let windows = two_folders_running(3, STEP, None).await?;
    let peak = most_at_once(&windows);

    // Obie strony naraz: górna dla biegów, które dzielą pulę, dolna dla implementacji, która
    // nie zrównolegla w ogóle i przy której sufit nie ograniczałby niczego.
    assert_eq!(
        peak,
        TOGETHER,
        "{peak} steps of the two folders were inside the agent app at the same moment, and the \
         one pool this application hands out says {TOGETHER}. More than that means the latch \
         stopped being keyed and the pool stopped being shared in the same change: two folders \
         at two apiece is four agents at ~583 MB each, which is a frozen laptop rather than \
         faster work. Fewer than that would mean nothing ever ran beside anything else. The \
         windows were {:?}",
        spans(&windows)
    );
    Ok(())
}

/// (AC-4) Przy żywych biegach w dwóch folderach Stop zatrzymuje OBA i wraca z dowodem.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stop_reaches_a_run_in_every_folder() -> Result<(), Box<dyn Error>> {
    let stopped = two_folders_running(1, LONG_STEP, Some(Door::Stop)).await;
    assert!(
        stopped.is_ok(),
        "Stop pressed with a run going in each of two folders did not come back: {:?}. Stop \
         waits for proof that nothing is alive any more (invariant 6), so a folder it never \
         reached is a folder whose agent keeps working and keeps paying with nothing left on \
         the screen to press",
        stopped.err().map(|error| error.to_string())
    );
    Ok(())
}

/// (AC-4) Zamknięcie okna nie zostawia ani jednego żywego biegu.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn closing_the_window_leaves_no_run_going() -> Result<(), Box<dyn Error>> {
    let closed = two_folders_running(1, LONG_STEP, Some(Door::Close)).await;
    assert!(
        closed.is_ok(),
        "closing the window with a run going in each of two folders left one of them behind: \
         {:?}. Anything that outlives Loadout moves under PID 1 and keeps working, and nothing \
         cleans up after it: it has no entry in the index of runs",
        closed.err().map(|error| error.to_string())
    );
    Ok(())
}

/// Którą drogą zatrzymania idziemy w tym przebiegu.
#[derive(Debug, Clone, Copy)]
enum Door {
    /// Człowiek nacisnął Stop.
    Stop,
    /// Człowiek zamknął okno.
    Close,
}

/// Dwa biegi w dwóch folderach, oba z JEDNEGO `AppState`, i okna wszystkich ich kroków.
///
/// PRODUKCYJNE DRZWI, nie ręcznie sklejony `RunDeps`: uchwyt bierze się z `AppState::begin_run`,
/// czyli tak, jak bierze go skorupa komendy. Zapadka mieszka dokładnie tam i tylko tędy da się
/// zapytać, czy jest kluczowana.
async fn two_folders_running(
    steps_per_run: usize,
    hold: Duration,
    door: Option<Door>,
) -> Result<Vec<(&'static str, Instant, Instant)>, Box<dyn Error>> {
    let bench = Bench::new()?;
    let ledger = bench.folder("ledger")?;
    let atlas = bench.folder("atlas")?;
    bench.agent("hand", HAND_FILE)?;
    let ledger_file = bench.workflow(LEDGER, steps_per_run)?;
    let atlas_file = bench.workflow(ATLAS, steps_per_run)?;

    // JEDEN obserwator na oba biegi: okna liczone osobno w każdym biegu nie odpowiadają na
    // pytanie tego pliku, choćby były policzone bezbłędnie.
    let watch = Arc::new(Watch::default());
    let state = AppState::new(
        bench.home.path().to_path_buf(),
        bench.project.path().to_path_buf(),
        Store::open(&bench.db())?,
        fake_drivers(Arc::clone(&watch), hold),
    );

    let ledger_run = state
        .begin_run(&ledger)
        .map_err(|said| format!("the first Start was turned down with nothing going: {said}"))?;
    /* TU PADA CAŁA STARA IMPLEMENTACJA. Zapadka na jednym uchwycie odmawia temu startowi
     * zdaniem o biegu, który idzie w INNYM folderze — więc drugi pas nigdy nie rusza i nie ma
     * czego nałożyć. `two_folders_share_one_pool.rs` obchodził to jawnym `settle()` przed drugim
     * Startem i dlatego globalność zapadki była stamtąd niewidoczna. */
    let atlas_run = state.begin_run(&atlas).map_err(|said| {
        format!(
            "a Start in a second folder was turned down while a run was going in the first \
             one: {said:?}. The latch that answers \"is something already going\" is keyed by \
             folder, so a run in one workspace has no business refusing work in another — that \
             refusal makes the whole of Loadout look busy when one folder is"
        )
    })?;

    let asked_for = |file: &Path| RunRequest {
        workflow: file.to_path_buf(),
        how_many_at_once: TOGETHER,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let ledger_asks = asked_for(&ledger_file);
    let atlas_asks = asked_for(&atlas_file);
    let (ledger_sink, ledger_pump) = the_pump_seam();
    let (atlas_sink, atlas_pump) = the_pump_seam();

    let (ledger_ended, atlas_ended, door_answered, (), ()) =
        tokio::time::timeout(PATIENCE, async {
            tokio::join!(
                run_workflow_inner(&ledger_run, &ledger_asks, ledger_sink),
                run_workflow_inner(&atlas_run, &atlas_asks, atlas_sink),
                async {
                    match door {
                        None => Ok(()),
                        Some(door) => {
                            both_are_working(&ledger_run, &atlas_run).await?;
                            match door {
                                Door::Stop => state.stop_every_live_run().await.map(|_| ()),
                                Door::Close => state.stop_every_live_run_before_closing().await,
                            }
                            .map_err(|error| error.to_string())
                        }
                    }
                },
                ledger_pump,
                atlas_pump,
            )
        })
        .await
        .map_err(|_| format!("the two runs did not both finish within {PATIENCE:?}"))?;

    door_answered?;
    let reports = [ledger_ended?, atlas_ended?];
    if door.is_some() {
        // Nic więcej do zmierzenia: Stop wrócił, czyli oba biegi oddały dowód, że nic po nich
        // nie żyje. Okna kroków są tu bez znaczenia, bo żaden krok nie dobiegł końca.
        return Ok(Vec::new());
    }
    for report in reports {
        assert_eq!(
            report.steps,
            vec![StepState::Succeeded; steps_per_run],
            "every step of both folders has to finish for the measured windows to mean \
             anything; one run ended as {:?}",
            report.steps
        );
    }

    let windows = watch.windows();
    assert_eq!(
        windows.len(),
        2 * steps_per_run,
        "the agent app closed {} window(s) out of {}; an unclosed window silently lowers the \
         overlap count, so the measurement would understate exactly what it is here to catch",
        windows.len(),
        2 * steps_per_run,
    );
    Ok(windows)
}

/// Czeka, aż OBA foldery zameldują prowadzony bieg — i dopiero wtedy oddaje sterowanie.
///
/// Bez tego Stop trafiałby czasem w bieg, który jeszcze nie ruszył: `stop_if_anything_is_going`
/// odpowiada wtedy „nie było czego zatrzymywać" i wraca, a krok zaczyna się chwilę później.
/// Zielone byłoby wtedy przypadkiem, a nie z powodu.
/// Uchwyty są KLONAMI tych, które trzyma zapadka, więc widać przez nie ten sam bieg — to jest
/// ta sama droga, którą `no_start_orphans_the_previous.rs` pyta, czyj uchwyt jest żywy.
async fn both_are_working(here: &RunDeps<'_>, there: &RunDeps<'_>) -> Result<(), String> {
    let until = Instant::now() + PATIENCE;
    while Instant::now() < until {
        if here.control.is_working() && there.control.is_working() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err("neither folder reported a run of its own within the time this test waits".to_owned())
}

/// Jedno domknięte okno tego pasa — albo zdanie o tym, ile ich naprawdę było.
fn only_window(
    windows: &[(&'static str, Instant, Instant)],
    lane: &str,
) -> Result<(Instant, Instant), Box<dyn Error>> {
    let mine: Vec<(Instant, Instant)> = windows
        .iter()
        .filter(|&&(mark, _, _)| mark == lane)
        .map(|&(_, from, to)| (from, to))
        .collect();
    match mine.as_slice() {
        [only] => Ok(*only),
        many => Err(format!(
            "the folder marked {lane} closed {} window(s) instead of exactly one, so the \
             overlap below would be measured between things that are not one run each",
            many.len()
        )
        .into()),
    }
}

/// Największa liczba okien otwartych **naraz**, policzona na osi zdarzeń.
///
/// Nie liczba uruchomień i nie czas całości: obie te liczby wychodzą tak samo dla biegu, który
/// wysyła szeroko i wykonuje po jednym.
fn most_at_once(windows: &[(&'static str, Instant, Instant)]) -> usize {
    let mut marks: Vec<(Instant, i32)> = Vec::with_capacity(windows.len() * 2);
    for &(_, from, to) in windows {
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
fn spans(windows: &[(&'static str, Instant, Instant)]) -> Vec<(&'static str, Duration)> {
    windows
        .iter()
        .map(|&(mark, from, to)| (mark, to.saturating_duration_since(from)))
        .collect()
}

/// Szew, którym bieg mówi do okna: nadajnik dla biegu i czekanie na pompę.
fn the_pump_seam() -> (LineSink, impl Future<Output = ()>) {
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    (sink, async move {
        let _ = pump.await;
    })
}

/// Biblioteka użytkownika, folder startowy aplikacji i foldery obu pasów.
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

    /// Folder pracy jednego pasa. Pod folderem startowym aplikacji, bo `TempDir` na macOS leży
    /// pod dowiązaniem `/var` → `/private/var`: dwa różne zapisy tej samej ścieżki są dokładnie
    /// tym, co kanoniczna tożsamość ma skleić w jeden klucz.
    fn folder(&self, name: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.project.path().join(name);
        fs::create_dir_all(path.join(".loadout"))?;
        Ok(path)
    }

    fn agent(&self, slug: &str, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.home.path().join("agents").join(format!("{slug}.md"));
        fs::write(&path, text)?;
        Ok(path)
    }

    /// Plik workflow jednego pasa: `how_many` luźnych kroków, każdy ze znakiem pasa w instrukcji.
    ///
    /// **Ani jednej strzałki** — nic poza limitem nie ustala tu kolejności. Każdy krok pracuje na
    /// własnej kopii plików nie dla ozdoby: kroki mogące biec równocześnie w jednym folderze są
    /// odmową przy zapisie (niezmiennik 12), więc bez tego fikstura nie doszłaby do planisty.
    fn workflow(&self, mark: &str, how_many: usize) -> Result<PathBuf, Box<dyn Error>> {
        let steps: Vec<String> = (0..how_many)
            .map(|n| {
                format!(
                    "{{\"kind\":\"agent\",\"id\":\"s_{mark}_{n}\",\"name\":\"Step {n}\",\
                     \"agent\":\"{HAND}\",\"overrides\":{{}},\"instructions\":\"{mark}\",\
                     \"folder\":{{\"use\":\"fresh-copy\"}},\"at\":{{\"x\":{},\"y\":0}}}}",
                    n * 240
                )
            })
            .collect();
        let path = self
            .home
            .path()
            .join("workflows")
            .join(format!("{mark}.json"));
        fs::write(
            &path,
            format!(
                "{{\"format\":1,\"id\":\"wf_{mark}\",\"name\":\"Lane {mark}\",\"steps\":[{}],\
                 \"links\":[]}}",
                steps.join(",")
            ),
        )?;
        the_fixture_can_run(&path, &self.home.path().join("agents").join("hand.md"))?;
        Ok(path)
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }
}

/// Fikstura ma przejść walidator **bez ani jednego problemu**, a jej plik agenta ma dać się
/// przeczytać.
///
/// To nie jest część kryterium, tylko jego przesłanka, i dlatego stoi przed biegiem. Czerwień
/// w fazie kontraktu wygląda identycznie dla „zachowania jeszcze nie ma" i dla „tego kryterium
/// nie da się spełnić nigdy".
fn the_fixture_can_run(workflow: &Path, agent: &Path) -> Result<(), Box<dyn Error>> {
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
    read_agent_file(agent).map_err(|error| format!("{}: {error}", agent.display()))?;
    Ok(())
}

/// Fabryka, która dla każdego vendora oddaje ten sam dubler.
fn fake_drivers(watch: Arc<Watch>, hold: Duration) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { watch, hold });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Jedno uruchomienie dublera, ze znakiem pasa, z którego przyszło.
#[derive(Debug)]
struct Ran {
    mark: &'static str,
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
    fn entered(&self, mark: &'static str) -> usize {
        let mut runs = self.lock();
        runs.push(Ran {
            mark,
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
    fn windows(&self) -> Vec<(&'static str, Instant, Instant)> {
        self.lock()
            .iter()
            .filter_map(|ran| Some((ran.mark, ran.from, ran.to?)))
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

/// Znak pasa odczytany z promptu kroku. Nieznany prompt to wada fikstury, nie wynik pomiaru.
fn lane_of(prompt: &str) -> &'static str {
    if prompt.contains(LEDGER) {
        LEDGER
    } else {
        assert!(
            prompt.contains(ATLAS),
            "a step reached the agent app carrying neither folder's mark, so no window it opens \
             could be attributed to a folder: {prompt:?}"
        );
        ATLAS
    }
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
        let entry = self.watch.entered(lane_of(&spec.prompt));
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
