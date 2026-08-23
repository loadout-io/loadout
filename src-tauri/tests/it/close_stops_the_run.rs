//! Zamknięcie okna: zatrzymuje bieg z dowodem, a przy braku biegu nie czeka na nic.
//!
//! # Po co to istnieje
//!
//! Zgłoszenie właściciela 2026-08-19, dwa zdania pod rząd: „co się dzieje jak zamykasz apkę
//! a leci jakiś workflow? on się wyłączy?", a po odpowiedzi „no to to napraw bo odpalałem kilka
//! workflow i apkę zamykałem w trakcie". Nie wyłączał się i nie był to niedopatrzenie w jednej
//! linii: w `src-tauri/src/lib.rs` nie było ANI JEDNEJ obsługi zamknięcia — ani
//! `on_window_event`, ani `CloseRequested`, ani `RunEvent`. Zamknięcie okna kończyło proces
//! Loadouta, a agenci przechodzili pod PID 1 i pracowali dalej: pisali po plikach projektu
//! i palili limit u dostawcy do następnego uruchomienia aplikacji (`recovery.rs`, nagłówek).
//!
//! # Czego ten plik NIE sądzi
//!
//! Samego okna. `harness/gate.py` słusznie nie uznaje „Failed to launch" za czerwień kodu, więc
//! kryterium wymagające żywego Tauri byłoby kryterium, które nigdy nie świeci. Sądzona jest
//! **decyzja**, która za tym stoi i która mieszka w rdzeniu: `stop_before_closing`. Że samo
//! zatrzymanie czeka na dowód śmierci grupy procesów, dowodzi `run_stop_waits_for_proof` na
//! prawdziwych procesach — tutaj chodzi o to, KIEDY wolno go w ogóle zażądać.
//!
//! # Słaba wersja tego kryterium
//!
//! `assert!(stop_before_closing(&deps).await.is_ok())` na uchwycie w trakcie biegu. Przechodzi
//! dla implementacji, która wraca natychmiast po wysłaniu sygnału — czyli dla tej, która zamyka
//! okno i zostawia żywe procesy, bo dokładnie to naprawiamy. Rozstrzyga pomiar w dwóch krokach:
//! wywołanie **nie ma prawa** wrócić, dopóki bieg nie zszedł, i **ma** wrócić, kiedy zszedł.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w `ipc_read_paths` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use loadout_lib::commands::run::stop_before_closing;
use loadout_lib::commands::{Drivers, Outcome, RunControl, RunDeps};
use loadout_lib::engine::drivers::AgentDriver;
use loadout_lib::engine::drivers::absent::Absent;
use loadout_lib::store::Store;
use tempfile::TempDir;

/// Ile czekamy, zanim uznamy wywołanie za zawieszone.
///
/// Krótko z premedytacją: to jest czas, po którym stwierdzamy „nie wróciło", a nie czas
/// schodzenia agentów. Wywołanie, które ma wrócić natychmiast, wraca w mikrosekundach —
/// margines rzędu wielkości wystarcza, żeby próg nie zależał od obciążenia maszyny.
const PATIENCE: Duration = Duration::from_secs(5);

/// Ile czekamy, ŻEBY SIĘ UPEWNIĆ, że wywołanie jeszcze nie wróciło.
///
/// Ten próg mierzy coś odwrotnego niż [`PATIENCE`] i dlatego jest osobną stałą: chcemy tu
/// najkrótszy czas, po którym „nie wróciło" znaczy „czeka", a nie „nie zdążyło".
const BRIEFLY: Duration = Duration::from_millis(300);

/// Sterownik, którego ten plik nigdy nie uruchamia — `RunDeps` wymaga fabryki, a żaden krok
/// tutaj nie startuje. `Absent` odmawia z nazwą zadania, więc gdyby jednak został zawołany,
/// zobaczylibyśmy to jako odmowę, a nie jako cichy sukces.
fn idle_drivers() -> Drivers {
    let absent: Arc<dyn AgentDriver> = Arc::new(Absent::new("fake", "no task"));
    Arc::new(move |_vendor| Arc::clone(&absent))
}

/// Katalogi i baza — tyle stanowiska, ile bierze `RunDeps`.
struct Bench {
    home: TempDir,
    project: TempDir,
}

impl Bench {
    fn new() -> Self {
        Self {
            home: TempDir::new().expect("a temporary library folder"),
            project: TempDir::new().expect("a temporary project folder"),
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_with_nothing_running_does_not_wait_for_anything() {
    let bench = Bench::new();
    let store = Store::open(&bench.home.path().join("loadout.db")).expect("a store");
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: idle_drivers(),
        // Uchwyt, którym NIKT nigdy nie uruchomił biegu — czyli stan aplikacji tuż po starcie
        // i najczęstszy stan w chwili zamykania okna.
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };

    /* TO JEST TA PUŁAPKA, i dlatego stoi jako pierwsze kryterium. `stop_run_inner` czeka na dowód
     * śmierci grupy procesów, a dowód zapala bieg, który przez siebie przeszedł. Zawołane na
     * uchwycie bez biegu czeka BEZ KOŃCA — czyli naiwna obsługa zamknięcia wieszałaby okno
     * dokładnie wtedy, gdy nic nie biegnie, i człowiek nie mógłby zamknąć aplikacji, w której
     * nic się nie dzieje. */
    let closed = tokio::time::timeout(PATIENCE, stop_before_closing(&deps)).await;
    let outcome = closed
        .expect(
            "closing an application in which nothing is running has to come back at once. It did \
             not, which means the close path waits for a proof that only a real run can give — \
             and the window can no longer be closed at all.",
        )
        .expect("there was nothing to stop, so there was nothing that could fail");
    assert_eq!(
        outcome,
        Outcome::Done,
        "with no run in flight nothing was left unfinished, so this must not report a run the \
         person never started as cancelled"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_mid_run_waits_until_the_run_is_really_down() {
    let bench = Bench::new();
    let store = Store::open(&bench.home.path().join("loadout.db")).expect("a store");
    let control = RunControl::new();
    /* Bieg w trakcie: `begin()` jest tym, co wołają prawdziwe biegi w `run_workflow_with_slots`,
     * a `settle()` jeszcze nie padło. Dwa znaczniki, bo jeden nie wystarcza — uchwyt świeży
     * i uchwyt w trakcie mają `settled` zgaszone oba. */
    control.begin();

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: idle_drivers(),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: control.clone(),
    };

    let mut closing = std::pin::pin!(stop_before_closing(&deps));

    // ── (a) NIE WRACA, DOPÓKI BIEG NIE ZSZEDŁ ───────────────────────────────────────────────
    //
    // To jest cała treść tej naprawy. Wersja, która wraca tutaj, zamyka okno w chwili wysłania
    // sygnału — a wtedy proces Loadouta kończy się, agenci przechodzą pod PID 1 i pracują dalej.
    let too_early = tokio::time::timeout(BRIEFLY, &mut closing).await;
    assert!(
        too_early.is_err(),
        "closing came back while the run had not yet proved it was down. That is the defect this \
         file exists for: the window disappears, the process ends, and the agents carry on under \
         PID 1 spending money nobody is watching (invariant 6)."
    );

    // ── (b) STOP DOSZEDŁ DO BIEGU ───────────────────────────────────────────────────────────
    //
    // Sprawdzane NA TOKENIE, nie po zwróconej wartości: „czeka" i „poprosiło o zatrzymanie" to
    // dwa różne fakty, a implementacja, która tylko czeka, przechodziłaby punkt (a).
    assert!(
        control.cancel_token().is_cancelled(),
        "the close path has to ask the run to stop, not merely wait for it to end on its own — \
         otherwise closing the window hangs until the agents finish by themselves"
    );

    // ── (c) WRACA, KIEDY BIEG NAPRAWDĘ ZSZEDŁ ───────────────────────────────────────────────
    //
    // `settle()` jest dowodem, który w produkcji zapala `run_workflow_with_slots` po ostatnim
    // kroku. Bez tej połowy kryterium punkt (a) przechodziłby dla implementacji, która nie wraca
    // NIGDY — czyli dla okna, którego nie da się zamknąć.
    control.settle();
    let closed = tokio::time::timeout(PATIENCE, closing).await;
    let outcome = closed
        .expect(
            "once the run is down, closing has to come back. It did not, which leaves the person \
             locked inside an application that is no longer doing anything.",
        )
        .expect("stopping a run that went down is not a failure");
    assert_eq!(
        outcome,
        Outcome::Cancelled,
        "a run that a person ended by closing the window did not finish on its own, and the \
         history has to say so"
    );
}

/* TRZECIE KRYTERIUM: bieg, ktory nie zejdzie NIGDY, nie zabiera czlowiekowi okna.
 *
 * Pierwsze kryterium tego pliku gasi czekanie bez konca pytaniem „czy w ogole cos biegnie" —
 * i gasi polowe. Druga polowa to bieg, ktory JEST w trakcie i ktorego zadanie sie zacielo:
 * `is_working()` mowi wtedy „tak", `wait_until_settled()` nie wraca, a `prevent_close` jest juz
 * podniesione. Czlowiek zostaje z oknem, ktorego nie da sie zamknac, i siega po jedyne wyjscie,
 * jakie mu zostalo — ubicie aplikacji z zewnatrz, czyli dokladnie te droge, ktora zostawia
 * sieroty. Zamowienie wlasciciela 2026-08-23: „zrob cos aby nie bylo takich sytuacji ze jakis
 * proces wisi".
 *
 * SLABA WERSJA: `assert!(closing.await.is_err())`. Przechodzi ja implementacja, ktora poddaje sie
 * po 200 ms — czyli ta, ktora przerywa UCZCIWE schodzenie agentow i zostawia je zywe w przypadku,
 * ktory dzialal. Dlatego nizej stoi obustronne ograniczenie: sufit ma byc wysoko nad tym, ile
 * trwa schodzenie (`supervisor::DEFAULT_GRACE` plus dowod po dziewiatce), i ma w ogole istniec.
 *
 * Zegar jest zatrzymany, wiec to kryterium nie czeka ani jednej prawdziwej sekundy, a mierzy
 * dokladnie te, ktore uplynelyby.
 */
#[tokio::test(start_paused = true)]
async fn a_run_that_will_never_come_down_still_lets_the_window_close() {
    let bench = Bench::new();
    let store = Store::open(&bench.home.path().join("loadout.db")).expect("a store");
    let control = RunControl::new();
    // Bieg w trakcie, ktorego `settle()` nie padnie nigdy: zacięte zadanie kroku wyglada
    // stad dokladnie tak.
    control.begin();
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: idle_drivers(),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: control.clone(),
    };

    let started = tokio::time::Instant::now();
    let refused = stop_before_closing(&deps)
        .await
        .expect_err("a run that never comes down cannot be reported as stopped");
    let waited = started.elapsed();

    assert!(
        waited >= loadout_lib::engine::supervisor::DEFAULT_GRACE * 2,
        "closing gave up after {waited:?}, which is inside the window agents are given to come \
         down on their own (SIGTERM, then the proof after SIGKILL). A ceiling this low does not \
         catch a stuck task - it cuts short the ordinary shutdown that was working, and leaves \
         the agents of a healthy run alive"
    );
    assert!(
        waited <= loadout_lib::commands::run::HOW_LONG_CLOSING_MAY_WAIT,
        "closing waited {waited:?}, past its own ceiling. With prevent_close already raised, \
         every second past it is a second in which the window cannot be closed at all"
    );

    let said = refused.to_string();
    assert!(
        said.contains("30 seconds"),
        "the sentence has to say how long it waited, because that is the one number that tells \
         a person this was a ceiling and not a crash. It said: {said}"
    );
    assert!(
        said.contains("next time you open Loadout"),
        "and it has to say what happens to what was left behind, or it is nothing but alarm. \
         It said: {said}"
    );
}
