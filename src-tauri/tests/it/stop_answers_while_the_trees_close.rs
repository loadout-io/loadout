//! Z-10: ciężka praca biegu schodzi z wątku, na którym okno odpowiada człowiekowi.
//!
//! # Trzy pomiary, jeden defekt
//!
//! Bieg kończy się checkoutem, commitem i kasowaniem katalogów (`close_the_trees`), zamknięcie
//! okna kasuje foldery jeden po drugim, a pisarz indeksu woła synchroniczne `rusqlite` w zadaniu
//! tokio. Wszystkie trzy trzymają wątek, na którym żyje też Stop, pompa wierszy i każde inne
//! zadanie tej aplikacji — więc objaw jest jeden i ten sam: człowiek naciska Stop i nic się nie
//! dzieje.
//!
//! # Dlaczego JEDEN wątek runtime'u jest tu treścią, a nie oszczędnością
//!
//! `#[tokio::test(flavor = "current_thread")]` w pierwszym i trzecim pomiarze. Przy czterech
//! workerach tokio przeniosłoby Stop na wolny wątek i oba testy zazieleniłyby się NAD wadą:
//! zablokowany worker byłby wtedy tylko jednym z czterech. Aplikacja ma workerów tyle, ile rdzeni,
//! ale liczba równoległych biegów, pomp i pisarzy nie jest ograniczona niczym — więc „jest jeszcze
//! jeden wolny wątek" nie jest własnością, na której wolno oprzeć odpowiadające okno.
//!
//! # Skąd bierze się opóźnienie i dlaczego to nie jest `sleep` w kodzie produkcyjnym
//!
//! Z prawdziwego repozytorium, którego hak `post-commit` śpi dwie sekundy. `git commit` w izolacji
//! leci z `--no-verify` (`commands::isolate`), a ten pomija `pre-commit` i `commit-msg` — nie
//! pomija `post-commit`. Hak ogłasza plikiem, że domykanie się ZACZĘŁO, więc test wie, w której
//! chwili wątek powinien być wolny, i nie zgaduje jej z zegara.
//!
//! # Słaba wersja pierwszego pomiaru
//!
//! `assert!(stop_run_inner(&deps).await.is_ok())`. Przechodzi na każdym kodzie: Stop czeka na
//! dowód zejścia biegu (niezmiennik 6), więc wraca dopiero po całym domykaniu — i wraca tak samo
//! wtedy, gdy nie był odpytany ani razu. Mierzalną różnicą jest to, KIEDY okno w ogóle dostało
//! wątek: przy zablokowanym wątku pierwsze odpytanie po ogłoszeniu haka przychodzi po dwóch
//! sekundach, a nie po jednym tyknięciu.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w pozostałych modułach tego celu.
#![allow(clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::run::{
    PrestartFaultInjector, PrestartFaultPoint, RolledBackResource,
    run_workflow_with_prestart_faults, run_workflow_with_reflection, stop_run_inner,
};
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::ipc::{AppState, QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use rusqlite::Connection;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio::time::timeout;

const VENDOR: &str = "claude-code";

/// Ile czeka cały bieg razem z dwusekundowym hakiem i prawdziwym gitem.
const PATIENCE: Duration = Duration::from_secs(30);

/// Jak często „okno" tego testu prosi o wątek.
const TICK: Duration = Duration::from_millis(25);

/// W ile okno ma dostać wątek od chwili, w której domykanie drzewa się zaczęło.
///
/// Liczba jest z kryterium audytu z 2026-09-02, nie z pomiaru tej maszyny: pół sekundy jest
/// granicą, za którą człowiek przestaje wierzyć, że kliknięcie doszło.
const ANSWERS_WITHIN: Duration = Duration::from_millis(500);

/// Ile śpi hak `post-commit` — czyli ile trwa domykanie jednego drzewa w tym teście.
const HOOK_SLEEPS: Duration = Duration::from_secs(2);

/// Ile trwa zejście jednego biegu po Stopie w pomiarze dwóch folderów.
const COMES_DOWN_IN: Duration = Duration::from_secs(1);

/// Ile trzyma zamek pisarza obce połączenie w pomiarze indeksu.
///
/// Poniżej `store::BUSY_TIMEOUT_MS`, bo zapis ma się UDAĆ po odczekaniu, a nie odbić się od
/// sufitu zajętości: kryterium mówi, że wiersz ląduje w bazie, nie że pisarz się poddaje.
const LOCK_HELD_FOR: Duration = Duration::from_secs(2);

/// Plik, który pisze krok — jedyna zmiana w jego drzewie i cały powód, dla którego bieg
/// ma po sobie co commitować.
const MADE: &str = "the-work.txt";
const MADE_TEXT: &str = "this is what the agent produced";

/// Zdanie z zadania kroku. Dubler po nim rozpoznaje, że dostał ten bieg, a nie cudzy.
const TASK: &str = "write the file";

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_z10_close",
  "name": "One step, one tree",
  "steps": [
    {
      "kind": "agent",
      "id": "s_writes",
      "name": "Writes",
      "agent": "01990000-0000-7000-8000-0000000000b1",
      "overrides": {},
      "instructions": "write the file",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

const AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000b1
name: Scribe
summary: Writes things down
color: slate
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

// ── (a) STOP W TRAKCIE DOMYKANIA DRZEWA ────────────────────────────────────────────────────

#[tokio::test(flavor = "current_thread")]
async fn stop_answers_while_a_step_is_closing_its_tree() -> Result<(), Box<dyn Error>> {
    let bench = Bench::with_slow_git()?;
    let store = Store::open(&bench.db())?;
    let saw = Arc::new(AtomicUsize::new(0));
    let deps = bench.deps(&store, Arc::clone(&saw));
    let request = bench.request();

    let (lines, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, tauri::ipc::Channel::new(|_| Ok(())));
    let run = run_workflow_with_reflection(&deps, &request, lines, None, false);
    tokio::pin!(run);

    // Stop naciśnięty DOKŁADNIE wtedy, gdy hak commita ogłosił, że drzewo się domyka. Obie
    // przyszłości idą w jednym `join!`, więc Stop czekający na dowód zejścia (niezmiennik 6)
    // czeka na bieg, który dalej jest odpytywany — a nie na siebie.
    let ((answered, stopped), ran) = timeout(PATIENCE, async {
        tokio::join!(bench.press_stop_when_the_tree_closes(&deps), &mut run)
    })
    .await
    .map_err(|_| "the run and the Stop never both came back")?;
    let report = ran?;
    stopped?;
    timeout(PATIENCE, pump).await??;

    assert_eq!(
        saw.load(Ordering::Acquire),
        1,
        "the step never reached the driver, so this run had no work to close and everything \
         below would be talking about a run that never happened"
    );
    assert!(
        bench.closing_began().exists(),
        "the step's tree was never committed, so the delay this test is built on never happened: \
         nothing here measured the window while a tree was closing"
    );

    // TU PADA STARY KOD. Praca domykania stała w linii, więc jedyny wątek runtime'u był zajęty
    // przez cały dwusekundowy hak — a „okno" tego testu nie dostało go ani razu w tym czasie.
    let at_the_close = answered
        .at_the_close
        .expect("the window has to be given the thread at least once");
    assert!(
        at_the_close < ANSWERS_WITHIN,
        "the window waited {at_the_close:?} for its first turn after the step's tree began \
         closing, over the {ANSWERS_WITHIN:?} a person is willing to believe a click arrived. That \
         whole time Stop was on screen and did nothing, because closing the tree runs a commit \
         and a delete on the very thread that answers it"
    );
    // I TU PADA STARY KOD DRUGI RAZ, na drugiej połowie tej samej pracy: zakładanie drzewa kroku
    // to `git worktree add`, czyli pełny checkout, i stało w linii dokładnie tak samo. Hak
    // `post-checkout` tej ławki śpi tyle samo, co `post-commit`, więc gdyby układanie katalogu
    // biegu zostało na wątku okna, ta liczba dalej byłaby dwusekundowa.
    assert!(
        answered.longest < ANSWERS_WITHIN,
        "the longest the window went without a turn during this run was {:?}, over the \
         {ANSWERS_WITHIN:?} ceiling. Two things in a run are big enough to do that: laying out \
         the folders each step works in, and closing them again afterwards. Both are checkouts, \
         and neither one belongs on a thread anybody is waiting on",
        answered.longest
    );

    // I zdanie, które z tego Stopu powstaje, stoi tam, gdzie człowiek je czyta: w historii biegu,
    // czytanej tą samą drogą, którą czyta ją okno (niezmiennik 29).
    let past = read_run_inner(bench.project.path(), run_folder(&report)?)?;
    assert_eq!(
        past.state, "cancelled",
        "a person pressed Stop while this run was closing its last tree, and the run's own \
         history says it ended as {:?}. Without that word the history of a stopped run reads \
         exactly like the history of one nobody touched",
        past.state
    );
    assert_eq!(
        past.steps.first().map(|step| step.state.as_str()),
        Some("succeeded"),
        "the step had finished its work before that Stop, so its own row has to keep saying so"
    );
    Ok(())
}

// ── (b) DWA ŻYWE FOLDERY SCHODZĄ RAZEM ─────────────────────────────────────────────────────
//
// Zamknięcie okna zatrzymywało foldery JEDEN PO DRUGIM, a każdy z nich schodzi tyle, ile schodzą
// jego agenci (`engine::supervisor`: TERM, łaska, KILL, dowód). Przy dwóch folderach człowiek
// czekał dwa razy tyle, choć nic tych zejść nie łączy.
//
// Zegar jest ZATRZYMANY, więc ten pomiar nie czeka ani jednej prawdziwej sekundy i mierzy
// dokładnie te, które upłynęłyby. Sufit 30 s na folder i jego zdanie zostają tam, gdzie były
// (`close_stops_the_run.rs`) — tutaj mierzymy wyłącznie, czy foldery czekają na siebie.
#[tokio::test(start_paused = true)]
async fn two_folders_stop_their_runs_at_the_same_time() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let elsewhere = TempDir::new()?;
    let store = Store::open(&bench.db())?;
    let state = AppState::new(
        bench.home.path().to_path_buf(),
        bench.project.path().to_path_buf(),
        store,
        fake_drivers(Arc::new(AtomicUsize::new(0))),
    );

    let here = state
        .begin_run(bench.project.path())
        .map_err(io::Error::other)?;
    let there = state
        .begin_run(elsewhere.path())
        .map_err(io::Error::other)?;
    /* DWA ZNACZNIKI, NIE JEDEN: `is_working()` czyta i „ruszył", i „jeszcze nie zszedł", a bez
     * pierwszego z nich zamknięcie okna odpowiada „nie ma czego zatrzymywać" i nie naciska
     * niczego (`close_stops_the_run.rs` mówi to samo o uchwycie, którego nikt nie wziął). */
    here.control.begin();
    there.control.begin();
    // Dwa biegi w trakcie, każdy schodzący [`COMES_DOWN_IN`] po tym, jak dostanie Stop. Tyle
    // trwa dowód śmierci grupy i to jest jedyna rzecz, na którą zamknięcie okna czeka.
    let first = comes_down_after_stop(&here.control);
    let second = comes_down_after_stop(&there.control);

    let began = tokio::time::Instant::now();
    timeout(PATIENCE, state.stop_every_live_run_before_closing())
        .await
        .map_err(|_| "closing the window never came back")?
        .map_err(io::Error::other)?;
    let took = began.elapsed();
    timeout(PATIENCE, first).await??;
    timeout(PATIENCE, second).await??;

    assert!(
        here.control.cancel_token().is_cancelled() && there.control.cancel_token().is_cancelled(),
        "closing the window has to ask BOTH folders to stop. A folder left alone keeps its agents \
         alive under PID 1, spending money nobody is watching (invariant 6)"
    );
    // TU PADA STARY KOD: pętla czekała na zejście pierwszego folderu, zanim poprosiła drugi.
    assert!(
        took < COMES_DOWN_IN * 2,
        "closing the window took {took:?} for two folders that come down in {COMES_DOWN_IN:?} \
         each, so it waited for one before it even asked the other. Nothing connects those two \
         folders: the person is paying twice for a wait they could have had once"
    );
    Ok(())
}

// ── (c) BIEG DOCHODZI DO INDEKSU, CHOĆ JEDYNY WĄTEK JEST ZAJĘTY ────────────────────────────
//
// Pisarz indeksu woła synchroniczne `rusqlite` — a zapis czekający na zamek albo domykający
// dziennik stoi tyle, ile stoi `busy_timeout` (5 s). W zadaniu tokio ten czas jest czasem, przez
// który cała aplikacja przestaje odpowiadać: nie idzie ani jedna linia biegu, ani Stop.
//
// Zamek trzyma tu obce połączenie, bo to jest jedyny sposób, żeby zapis trwał mierzalnie długo
// bez zmiany kodu produkcyjnego. Zapis ma się przy tym UDAĆ — kryterium mówi, że wiersz ląduje
// w bazie, a nie że pisarz się poddaje.
//
// # Dwa odczyty na końcu i żadnego surowego `SELECT`
//
// `Store::rebuild_from` **czeka na odpowiedź pisarza** (`Writer::rows` czeka na `oneshot`), więc
// jego `Ok(())` jest zdaniem samego pisarza „ten bieg jest w bazie" — mocniejszym niż zapytanie
// napisane w teście. A to, co z tego widzi CZŁOWIEK, czyta `read_run_inner`, czyli dokładnie ta
// droga, którą historia w oknie otwiera bieg (niezmiennik 29).
//
// Że `read_run_inner` czyta PLIKI, a nie indeks, jest tu treścią, nie usterką: niezmiennik 4 mówi
// „pliki są prawdą, `loadout.db` jest indeksem", więc droga człowieka do biegu nie ma prawa
// zależeć od bazy. Ten test pilnuje obu połówek naraz — indeks przyjął wiersz, a bieg dalej
// otwiera się tą samą komendą, którą woła okno.
#[tokio::test(flavor = "current_thread")]
async fn a_run_reaches_the_index_while_the_only_thread_is_taken() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let store = Store::open(&bench.db())?;
    let saw = Arc::new(AtomicUsize::new(0));

    // Prawdziwy bieg, zwykłą drogą: zostawia `run.json` na dysku i swój wiersz w indeksie.
    let report = {
        let deps = bench.deps(&store, Arc::clone(&saw));
        let request = bench.request();
        let (lines, source) = line_channel(QUEUE_CAP);
        let pump = spawn_pump(source, tauri::ipc::Channel::new(|_| Ok(())));
        let report = timeout(
            PATIENCE,
            run_workflow_with_reflection(&deps, &request, lines, None, false),
        )
        .await
        .map_err(|_| "the run never came back")??;
        timeout(PATIENCE, pump).await??;
        report
    };
    assert_eq!(
        saw.load(Ordering::Acquire),
        1,
        "the step never reached the driver, so this run wrote nothing and everything below would \
         be talking about a run that never happened"
    );

    // Od tej chwili zapis do indeksu musi czekać na cudzy zamek — tyle, ile trwa [`LOCK_HELD_FOR`].
    let held = Connection::open(bench.db())?;
    held.execute_batch("BEGIN IMMEDIATE")?;
    let holder = std::thread::spawn(move || {
        std::thread::sleep(LOCK_HELD_FOR);
        held.execute_batch("ROLLBACK")
    });

    let longest = Arc::new(AtomicU64::new(0));
    let began = Instant::now();
    let watching = tokio::spawn(the_window_asking_for_the_thread(
        began,
        Arc::clone(&longest),
    ));
    // Ta sama droga, którą bieg oddaje swój wynik do indeksu (`commands::run::finish_planned_run`).
    timeout(PATIENCE, store.rebuild_from(&report.dir))
        .await
        .map_err(|_| "the run never reached the index at all")??;
    let wrote_in = began.elapsed();
    /* JEDNO TYKNIĘCIE PO ZAPISIE, i bez niego ten pomiar nie mierzy niczego. Sonda zapisuje
     * przerwę dopiero wtedy, gdy jej własny `sleep` się skończy — a zdjęta natychmiast po zapisie
     * nie dostaje na to ani jednej tury i oddaje zero, czyli liczbę, która przechodzi każdą
     * asercję. Zmierzone 2026-09 (Z-10): na starym pisarzu test był wtedy zielony. */
    tokio::time::sleep(TICK * 2).await;
    watching.abort();
    holder.join().map_err(|_| "the lock holder panicked")??;

    assert!(
        wrote_in >= LOCK_HELD_FOR / 2,
        "the index took the run after {wrote_in:?}, so it never waited for the lock this test \
         holds and the assertion below would pass on any code at all"
    );
    // TU PADA STARY KOD: pętla pisarza była zadaniem tokio, więc te dwie sekundy w `rusqlite`
    // były dwiema sekundami, w których nic innego w aplikacji nie dostało wątku.
    let waited = Duration::from_millis(longest.load(Ordering::Acquire));
    assert!(
        waited > Duration::ZERO,
        "the rest of the application never got a single turn during this write, so there is no \
         measurement here at all and the assertion below would pass on any code"
    );
    assert!(
        waited < ANSWERS_WITHIN,
        "while one run was waiting its turn in the index, the rest of the application went \
         {waited:?} without a single turn — over the {ANSWERS_WITHIN:?} ceiling. Writing to the \
         index is not allowed to stop the lines of a live run from reaching the screen"
    );

    // I to, co z tego widzi człowiek: bieg otwarty tą samą komendą, którą otwiera go historia.
    let past = read_run_inner(bench.project.path(), run_folder(&report)?)?;
    assert_eq!(
        past.folder,
        run_folder(&report)?,
        "the run that just went into the index does not open in the history any more"
    );
    assert_eq!(
        past.steps.len(),
        1,
        "the run opens, but without the step it ran — so what the person reads about it is not \
         what happened"
    );
    Ok(())
}

// ── (d) NIEUDANY START TEŻ SPRZĄTA POZA WĄTKIEM OKNA ──────────────────────────────────────
//
// Przygotowanie biegu trzyma gwardię ([`commands::run::ProvisionalRun`]), której `Drop` **sprząta
// gitem**: `git worktree remove`, `git branch -D`, `remove_dir_all` na katalogu biegu. Dopóki
// wszystko idzie dobrze, gwardia jest rozbrajana i nie robi nic — więc pomiar samego udanego
// Startu jest ślepy na to, gdzie ta praca biegnie. Widać ją dopiero na drodze ODMOWY.
//
// A odmów po założeniu drzewa jest w tym przygotowaniu sześć: nieudany seed przekazań, odmowa
// pożyczki z projektu, brakująca umiejętność, nieudany pierwszy `run.json`, odmowa akceptacji
// triggera i panika. Każda z nich schodzi tą samą gwardią, więc wystarczy jedna, żeby zmierzyć
// wątek — a wybieramy tę zaraz po drzewie, bo wtedy do cofnięcia jest najwięcej.
//
// MIERZYMY WĄTEK, NIE CZAS, i to jest tu mocniejsze: `git worktree remove` na pustym drzewie
// trwa milisekundy, więc zegar nie odróżniłby workera od puli. Tożsamość wątku odróżnia je
// zawsze — a `flavor = "current_thread"` znaczy, że wątek tego testu JEST wątkiem, na którym
// stoi całe okno.
#[tokio::test(flavor = "current_thread")]
async fn a_refused_start_rolls_its_folders_back_off_the_window_thread() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let store = Store::open(&bench.db())?;
    let saw = Arc::new(AtomicUsize::new(0));
    let deps = bench.deps(&store, Arc::clone(&saw));
    let request = bench.request();
    let (lines, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, tauri::ipc::Channel::new(|_| Ok(())));

    let window = std::thread::current().id();
    let faults = Arc::new(RefusedAfterTheTreeWasThere::default());
    let refused = timeout(
        PATIENCE,
        run_workflow_with_prestart_faults(&deps, &request, lines, Arc::clone(&faults) as Arc<_>),
    )
    .await
    .map_err(|_| "the refused start never came back")?
    .expect_err("this start is refused on purpose, so it cannot come back as a run");
    timeout(PATIENCE, pump).await??;

    assert_eq!(
        saw.load(Ordering::Acquire),
        0,
        "the refusal has to land before the first agent starts, or this run was not refused at \
         all and the folders below were rolled back from somewhere else: {refused}"
    );
    // Kontrola: cofnięcie NAPRAWDĘ się odbyło. Bez tego asercja niżej przechodzi na pustej liście,
    // czyli na przygotowaniu, które nie zdążyło niczego założyć.
    let rolled_back_on = faults.rolled_back_on();
    assert!(
        !rolled_back_on.is_empty(),
        "the refused start rolled nothing back, so this test measured a preparation that never \
         made a folder — and the assertion below would pass on any code at all"
    );
    // TU PADA STARY KOD. Gwardia wracała ze swojej puli uzbrojona, więc każde `?` po niej
    // sprzątało git-em na tym wątku, na którym okno czeka na odpowiedź.
    assert!(
        !rolled_back_on.contains(&window),
        "a Start that was refused took its folders down on the thread the window is drawn on. \
         That is a checkout being undone — `git worktree remove`, `git branch -D` and a recursive \
         delete — in front of a person who is waiting to be told the Start did not happen"
    );
    Ok(())
}

/// Odmawia przygotowania zaraz po tym, jak drzewo pracy kroku już stoi, i zapisuje, NA JAKIM
/// WĄTKU gwardia je potem cofnęła.
#[derive(Debug, Default)]
struct RefusedAfterTheTreeWasThere {
    rolled_back_on: Mutex<Vec<ThreadId>>,
}

impl RefusedAfterTheTreeWasThere {
    fn rolled_back_on(&self) -> Vec<ThreadId> {
        self.rolled_back_on
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl PrestartFaultInjector for RefusedAfterTheTreeWasThere {
    fn check(&self, point: PrestartFaultPoint) -> io::Result<()> {
        if point == PrestartFaultPoint::AfterHandoffSeed {
            return Err(io::Error::other("this Start is refused on purpose"));
        }
        Ok(())
    }

    fn rolled_back(&self, _resource: &RolledBackResource) {
        self.rolled_back_on
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(std::thread::current().id());
    }
}

// ── narzędzia pomiaru ──────────────────────────────────────────────────────────────────────

/// Co „okno" zmierzyło, czekając na swoją turę w trakcie biegu.
struct Answered {
    /// Przerwa przed tyknięciem, które zobaczyło początek domykania drzewa. `None` znaczy, że
    /// żadne tyknięcie tego nie zobaczyło — czyli że nie ma tu czego mierzyć.
    at_the_close: Option<Duration>,
    /// Najdłuższa przerwa od startu biegu. Obejmuje też ZAKŁADANIE drzew, czyli tę połowę
    /// ciężkiej pracy, która dzieje się przed pierwszym procesem agenta.
    longest: Duration,
}

/// „Okno", które co [`TICK`] prosi o wątek, i największa przerwa, jaką na to czekało.
///
/// Największa, nie średnia: średnia z osiemdziesięciu tyknięć rozpuszcza jedną dwusekundową
/// dziurę w liczbie, która wygląda zdrowo — a człowiek widzi właśnie tę dziurę.
///
/// CHWILA STARTU PRZYCHODZI ARGUMENTEM, nie z pierwszej linii tego ciała, i to jest treść, nie
/// styl. Zadanie zgłoszone do runtime'u nie jest odpytywane od razu: jeśli wątek zabiera zaraz
/// potem ktoś inny, pierwsza instrukcja tej funkcji wykonuje się DOPIERO PO tamtej pracy — więc
/// zegar założony tutaj nie widziałby właśnie tej przerwy, którą ma zmierzyć. Zmierzone
/// 2026-09 (Z-10): tak liczona sonda oddawała na starym pisarzu 25 ms i test był zielony.
async fn the_window_asking_for_the_thread(began: Instant, longest: Arc<AtomicU64>) {
    let mut last = began;
    loop {
        tokio::time::sleep(TICK).await;
        let now = Instant::now();
        let waited = u64::try_from(now.duration_since(last).as_millis()).unwrap_or(u64::MAX);
        last = now;
        longest.fetch_max(waited, Ordering::AcqRel);
    }
}

/// Bieg, który schodzi [`COMES_DOWN_IN`] po tym, jak ktoś go zatrzyma — i ani chwili wcześniej.
///
/// Tyle w produkcji trwa dowód śmierci grupy: TERM, łaska, KILL, `kill(-pgid, 0)`. Zejście
/// liczone od Stopu, a nie od startu zadania, jest tu treścią: pomiar ma odróżnić dwa zejścia
/// obok siebie od dwóch jedno po drugim.
fn comes_down_after_stop(control: &RunControl) -> tokio::task::JoinHandle<()> {
    let control = control.clone();
    tokio::spawn(async move {
        control.cancel_token().cancelled().await;
        tokio::time::sleep(COMES_DOWN_IN).await;
        control.settle();
    })
}

/// Hak gita, wykonywalny — bo nadany bez bitu `x` jest hakiem, którego git nie odpali i o którym
/// nie powie ani słowa.
fn write_hook(path: &Path, body: &str) -> Result<(), Box<dyn Error>> {
    fs::write(path, format!("#!/bin/sh\n{body}"))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

/// Nazwa katalogu biegu — to samo, czym adresuje go okno.
fn run_folder(report: &RunReport) -> Result<&str, Box<dyn Error>> {
    report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "the run directory has no UTF-8 folder name".into())
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers(saw: Arc<AtomicUsize>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { saw });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct Fake {
    /// Ile kroków naprawdę doszło do sterownika.
    saw: Arc<AtomicUsize>,
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
        if !spec.prompt.contains(TASK) {
            anyhow::bail!(
                "this run handed the driver a task it does not know: {}",
                spec.prompt
            );
        }
        // Jedna zmiana w drzewie kroku, i to jest cały powód, dla którego bieg ma potem co
        // commitować — a więc i cały powód, dla którego hak `post-commit` w ogóle się odpali.
        fs::write(spec.cwd.join(MADE), MADE_TEXT)?;
        self.saw.fetch_add(1, Ordering::AcqRel);

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
        Ok(Box::new(Turn { events, session }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<loadout_lib::engine::supervisor::GroupId> {
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
            .send((AgentEvent::Finished(outcome.clone())).into())
            .await;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> loadout_lib::engine::supervisor::GroupProof {
        loadout_lib::engine::supervisor::GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

// ── ławka ──────────────────────────────────────────────────────────────────────────────────

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
        fs::write(home.path().join("agents").join("scribe.md"), AGENT)?;
        fs::write(
            home.path().join("workflows").join("z10-close.json"),
            WORKFLOW,
        )?;
        let bench = Self { home, project };
        fs::write(bench.project.path().join("notes.txt"), "the human's file")?;
        bench.make_a_repo()?;
        Ok(bench)
    }

    /// Ta sama ławka, z gitem, który mierzalnie zwleka — powód przy [`Bench::make_the_git_work_slow`].
    ///
    /// Osobno od [`Bench::new`], bo pomiar indeksu potrzebuje biegu SZYBKIEGO: tam opóźnienie ma
    /// dawać zamek bazy, a nie hak commita, i cztery sekundy czekania niczego by tam nie dowiodły.
    fn with_slow_git() -> Result<Self, Box<dyn Error>> {
        let bench = Self::new()?;
        bench.make_the_git_work_slow()?;
        Ok(bench)
    }

    fn deps<'a>(&'a self, store: &'a Store, saw: Arc<AtomicUsize>) -> RunDeps<'a> {
        RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store,
            drivers: fake_drivers(saw),
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        }
    }

    fn request(&self) -> RunRequest {
        RunRequest {
            workflow: self.home.path().join("workflows").join("z10-close.json"),
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        }
    }

    /// Plik, którym hak `post-commit` ogłasza, że domykanie drzewa właśnie się zaczęło.
    fn closing_began(&self) -> PathBuf {
        self.project.path().join("closing-began")
    }

    /// Czeka na ogłoszenie haka, naciska Stop i oddaje to, co „okno" zmierzyło po drodze.
    async fn press_stop_when_the_tree_closes(
        &self,
        deps: &RunDeps<'_>,
    ) -> (
        Answered,
        Result<loadout_lib::commands::Outcome, loadout_lib::commands::RunError>,
    ) {
        let began = self.closing_began();
        let mut last = Instant::now();
        let mut longest = Duration::ZERO;
        loop {
            tokio::time::sleep(TICK).await;
            let now = Instant::now();
            let waited = now.duration_since(last);
            last = now;
            longest = longest.max(waited);
            if began.exists() {
                let seen = Answered {
                    at_the_close: Some(waited),
                    longest,
                };
                // Stop jedzie DOKŁADNIE tą drogą, którą wysyła go okno — nie samym tokenem.
                return (seen, stop_run_inner(deps).await);
            }
        }
    }

    fn make_a_repo(&self) -> Result<(), Box<dyn Error>> {
        self.git(&["init", "--quiet"])?;
        fs::write(self.project.path().join(".gitignore"), ".loadout/\n")?;
        self.git(&["add", "-A"])?;
        self.git(&["commit", "--quiet", "-m", "the human's first commit"])?;
        Ok(())
    }

    /// Zakłada dwa haki, po jednym na każdą połowę ciężkiej pracy biegu, i oba śpią
    /// [`HOOK_SLEEPS`].
    ///
    /// `post-checkout` odpala `git worktree add`, czyli zakładanie katalogu, w którym krok
    /// pracuje; `post-commit` odpala zapis pracy na gałąź, czyli domykanie tego katalogu.
    /// Ten drugi ogłasza przy tym plikiem, że domykanie się ZACZĘŁO — dzięki temu test wie,
    /// w której chwili wątek powinien być wolny, i nie zgaduje jej z zegara.
    ///
    /// PO pierwszym commicie ławki, nigdy przed: haki odpalają się przy każdym commicie
    /// i każdym checkoutcie w tym repozytorium, więc założone wcześniej opóźniłyby też
    /// przygotowanie samej ławki i pomiar mówiłby o czymś innym niż praca biegu.
    ///
    /// `--no-verify`, z którym bieg commituje (`commands::isolate`), pomija `pre-commit`
    /// i `commit-msg`. Żadnego z tych dwóch nie pomija — i to jest cały powód, dla którego
    /// opóźnienie jest tu prawdziwym gitem, a nie `sleep` wstawionym w kod produkcyjny.
    fn make_the_git_work_slow(&self) -> Result<(), Box<dyn Error>> {
        let hooks = self.project.path().join(".git").join("hooks");
        fs::create_dir_all(&hooks)?;
        write_hook(
            &hooks.join("post-checkout"),
            &format!("sleep {}\n", HOOK_SLEEPS.as_secs()),
        )?;
        write_hook(
            &hooks.join("post-commit"),
            &format!(
                ": > \"{}\"\nsleep {}\n",
                self.closing_began().display(),
                HOOK_SLEEPS.as_secs()
            ),
        )
    }

    fn git(&self, args: &[&str]) -> Result<String, Box<dyn Error>> {
        let out = Command::new("git")
            .arg("-C")
            .arg(self.project.path())
            .args(["-c", "user.name=Loadout Test"])
            .args(["-c", "user.email=test@loadout.invalid"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()?;
        if !out.status.success() {
            return Err(format!(
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            )
            .into());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }
}
