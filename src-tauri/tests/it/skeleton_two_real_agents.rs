//! AC-1 dla T-28: dwa **prawdziwe** procesy `claude` zajmują nachodzące na siebie okna czasu,
//! każdy w swojej kopii katalogu.
//!
//! To jest zdanie z `docs/PLAN.md` §1 — „naciskam Start i dwa prawdziwe procesy `claude`
//! pracują jednocześnie, każdy w swojej kopii repo" — zamienione w kryterium. Do dziś dowodził
//! go wyłącznie `engine_overlap.rs`, czyli **dwie atrapy**: ten sam kształt asercji, tylko na
//! `FakeDriver`, który śpi zadany czas i nie odpala niczego. Atrapa dowodzi planisty; nie
//! dowodzi, że planista, sterownik i nadzór składają się na dwa żywe procesy.
//!
//! **Słaba wersja tego kryterium:** `assert!(oba.is_ok())` plus pomiar, że całość trwała krócej
//! niż suma. Przechodzi na planiście, który odpala kroki jeden po drugim, kiedy drugi jest
//! szybszy — a poprzedni prototyp miał dokładnie to: `max_parallel` było wyłącznie szerokością wysyłki,
//! cztery „równoległe" pasy w rozłącznych oknach po ~0,5 s, i ani jednej sekundy, w której
//! działały dwa agenty (niezmiennik 11). Dyskryminuje **część wspólna przedziałów**.
//!
//! Okno kroku kończy się na `wait()`, czyli na końcu tury, a **nie** na `close()`. To nie jest
//! drobiazg: `close()` czeka na wyjście procesu i przy wolnym czytaniu stdoutu potrafi trwać do
//! 30 s [T1 „Worth adding"], więc okno liczone do niego nadmuchiwałoby się o czas sprzątania
//! i część wspólna robiłaby się prawdziwa sama z siebie. Mierzymy okno, w którym agent
//! **pracował**.
//!
//! Runtime jest wielowątkowy z prawdziwymi procesami i nigdy `start_paused`: czas wirtualny
//! przeskakuje do przodu, kiedy runtime staje bezczynny, więc „nakładanie się" przestałoby
//! cokolwiek znaczyć [T7 §8.1].
//!
//! **Ten test kosztuje pieniądze i na maszynie bez `claude` na PATH jest czerwony.** Jedno
//! i drugie jest decyzją człowieka z 2026-08-16, spisaną w `TASK.md`: zdania z §1 nie da się
//! udowodnić bez agenta, a bramka, która udaje, że da się, jest gorsza niż jej brak.

use std::error::Error;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use loadout_lib::engine::StepId;
use loadout_lib::engine::dag::Dag;
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::{AgentDriver, Policy, RunSpec};
use loadout_lib::engine::scheduler::execute;
use loadout_lib::engine::step::{StepReport, StepState};
use loadout_lib::engine::supervisor::GroupId;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Ile kroków ma naprawdę działać naraz. Dwa — bo pytanie brzmi „czy dwa naraz", a nie „czy
/// szybciej".
const AT_ONCE: usize = 2;

/// Sufit na start procesu. Każde oczekiwanie ma swój, bo regresja ma się objawić jako
/// **czerwony test**, a nie jako zawieszenie: bramka zwraca wtedy rc 124, a to jest fałszywa
/// czerwień, nie dowód.
const START_LIMIT: Duration = Duration::from_mins(1);

/// Sufit na jedną turę. Prompt jest na jedno słowo, więc to jest zapas rzędu wielkości,
/// a nie oczekiwany czas.
const TURN_LIMIT: Duration = Duration::from_mins(3);

/// Sufit na zamknięcie sesji. `claude` z otwartym stdinem czeka w nieskończoność, więc bez
/// `close()` krok zostawiałby żywy proces [T1 §2] — ale samo czekanie na jego wyjście też ma
/// mieć koniec.
const CLOSE_LIMIT: Duration = Duration::from_secs(30);

/// Ile zdarzeń mieści się w kanale sterownika, zanim ten zaczeka.
const EVENTS: usize = 256;

/// Treść zadania. Jedno słowo w odpowiedzi i **ani jednego narzędzia**: to jest jeden obrót,
/// za grosze, i tyle wystarczy, żeby proces naprawdę wstał i naprawdę zszedł.
///
/// Prompt jedzie **wyłącznie stdinem** (niezmiennik 9) — tędy wkłada go `RunSpec::prompt`,
/// a sterownik nie ma w argv ani jednego znaku tej treści.
const PROMPT: &str = "Reply with the single word: ready. Do not use any tools.";

/// Okno czasu, w którym krok pracował, plus to, czym go pracował.
///
/// `pid` i `pgid` przychodzą z `AgentHandle::group()`, czyli od nadzoru, a nie z naszego
/// liczenia: to są liczby, które zna jądro.
#[derive(Debug, Clone)]
struct Ran {
    /// Kiedy krok zaczął — chwila tuż przed `start()`.
    started_at: Instant,
    /// Kiedy skończył — chwila, w której wróciła tura.
    ended_at: Instant,
    /// Proces i jego grupa.
    group: Option<GroupId>,
    /// Katalog roboczy, który dostał ten krok.
    cwd: PathBuf,
}

/// Dwa kroki, dwa katalogi i miejsce, w którym zapisują, co im się przydarzyło.
///
/// Wspólny stan jest za `std::sync::Mutex`, a każde jego wzięcie mieści się w jednym bloku bez
/// `await` (niezmiennik 8, `clippy::await_holding_lock` = deny).
#[derive(Debug)]
struct Bench {
    /// Sterownik prawdziwego CLI. `ClaudeDriver::new()`, czyli gołe „claude" z `PATH` — bez
    /// atrapy i bez szwu, bo atrapa jest dokładnie tym, czego to kryterium nie przyjmuje.
    driver: ClaudeDriver,
    /// Katalog roboczy każdego kroku, po jednym na krok.
    dirs: Vec<PathBuf>,
    /// Co krok zapisał o sobie. `None`, dopóki nie ruszył.
    ran: Mutex<Vec<Option<Ran>>>,
    /// Powody, dla których krok nie doszedł do końca. Zbierane po to, żeby czerwony test mówił,
    /// **co** się stało, zamiast samego „nie oba się udały".
    why: Mutex<Vec<String>>,
}

impl Bench {
    /// Ławka na tyle kroków, ile katalogów.
    fn new(dirs: Vec<PathBuf>) -> Self {
        let steps = dirs.len();
        Self {
            driver: ClaudeDriver::new(),
            dirs,
            ran: Mutex::new(vec![None; steps]),
            why: Mutex::new(Vec::new()),
        }
    }

    /// Zamek, który nie panikuje. `panic!` w zadaniu planisty nie wraca ze swoim numerem kroku
    /// i zamienia czerwień z powodem w czerwień bez powodu.
    fn lock<T>(guarded: &Mutex<T>) -> MutexGuard<'_, T> {
        guarded.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Zapisuje, dlaczego krok nie doszedł do końca.
    fn blame(&self, id: StepId, reason: &str) {
        Self::lock(&self.why).push(format!("step {id}: {reason}"));
    }

    /// Wszystkie powody, jednym zdaniem.
    fn complaints(&self) -> String {
        let why = Self::lock(&self.why);
        if why.is_empty() {
            "no step said why".to_owned()
        } else {
            why.join(" · ")
        }
    }

    /// Co krok zapisał o sobie.
    fn observed(&self, id: StepId) -> Option<Ran> {
        Self::lock(&self.ran)[id].clone()
    }

    /// Jeden krok: własny katalog, własny proces, własne okno czasu.
    ///
    /// Anulowania ten krok nie obsługuje i nie ma po co: AC-1 pyta o **nakładanie się**,
    /// a zejście po grupie procesów jest osobnym kryterium (AC-2, `skeleton_group_death.rs`).
    async fn step(self: Arc<Self>, id: StepId, _cancel: CancellationToken) -> StepReport {
        let cwd = self.dirs[id].clone();

        let (tx, mut inbox) = mpsc::channel(EVENTS);
        // Kanał musi być OPRÓŻNIANY, nie tylko trzymany przy życiu: pętla czytająca sterownika
        // zatrzymuje się na pełnym buforze, a zatrzymana pętla nie dojdzie do EOF-u i `close()`
        // nie wróci nigdy. Odbiornik, który tylko istnieje, wystarcza atrapie i nie wystarcza
        // agentowi, który naprawdę mówi.
        let _drain = tokio::spawn(async move { while inbox.recv().await.is_some() {} });

        let spec = RunSpec {
            // Sesję nadajemy MY, przed startem procesu [T7 §6.2].
            run_id: Uuid::now_v7(),
            cwd: cwd.clone(),
            prompt: PROMPT.to_owned(),
            model: None,
            system_append: None,
            // Czyta i szuka, nie zapisuje niczego. Najtańsza polityka, jaka wystarcza.
            policy: Policy::ReadOnly,
            extra_dirs: Vec::new(),
            resume: None,
        };

        let started_at = Instant::now();
        let mut handle = match timeout(START_LIMIT, self.driver.start(spec, tx)).await {
            Ok(Ok(handle)) => handle,
            Ok(Err(error)) => {
                self.blame(id, &format!("the agent would not start: {error}"));
                return StepReport::Failed;
            }
            Err(_elapsed) => {
                self.blame(
                    id,
                    &format!("the agent did not start within {START_LIMIT:?}"),
                );
                return StepReport::Failed;
            }
        };

        // `pid` i `pgid` są znane ZANIM cokolwiek popłynie ze stdout [T7 §6.2] — bierzemy je
        // od razu, bo po anulowaniu albo po awarii nie ma już kogo o nie zapytać.
        let group = handle.group();

        let turn = match timeout(TURN_LIMIT, handle.wait()).await {
            Ok(Ok(turn)) => turn,
            Ok(Err(error)) => {
                self.blame(id, &format!("the turn ended without a result: {error}"));
                return StepReport::Failed;
            }
            Err(_elapsed) => {
                self.blame(id, &format!("the turn did not end within {TURN_LIMIT:?}"));
                return StepReport::Failed;
            }
        };
        let ended_at = Instant::now();

        {
            let mut ran = Self::lock(&self.ran);
            ran[id] = Some(Ran {
                started_at,
                ended_at,
                group,
                cwd,
            });
        }

        // Koniec sesji, nie koniec tury: bez EOF-u `claude` czeka w nieskończoność, a krok
        // zostawia żywy proces [T1 §2, §4.6].
        let code = match timeout(CLOSE_LIMIT, handle.close()).await {
            Ok(Ok(code)) => code,
            Ok(Err(error)) => {
                self.blame(id, &format!("the session would not close: {error}"));
                None
            }
            Err(_elapsed) => {
                self.blame(
                    id,
                    &format!("the session did not close within {CLOSE_LIMIT:?}"),
                );
                None
            }
        };

        // Sukces to `is_error == false` **i** czyste wyjście (niezmiennik 19): agent, który
        // wypisał „nie dam rady" i wyszedł zerem, nie zrobił tego, o co go proszono.
        if turn.ok && matches!(code, None | Some(0)) {
            StepReport::Succeeded
        } else {
            self.blame(
                id,
                &format!(
                    "the turn came back unsuccessful: reason {:?}, exit code {code:?}",
                    turn.reason
                ),
            );
            StepReport::Failed
        }
    }
}

/// Część wspólna dwóch przedziałów. Zero, kiedy przecięcie jest puste — czyli dokładnie wtedy,
/// kiedy kroki biegły jeden po drugim.
fn shared(first: &Ran, second: &Ran) -> Duration {
    first
        .ended_at
        .min(second.ended_at)
        .saturating_duration_since(first.started_at.max(second.started_at))
}

/* 2026-08-17 — DLACZEGO `#[ignore]` NA TEŚCIE, KTÓRY JEST KRYTERIUM.
 * `checks/full-test.sh` odpala `cargo test --tests` BEZ filtra, więc bierze każdy cel
 * z `src-tauri/tests/` — także ten. A ten cel uruchamia PRAWDZIWE procesy `claude`:
 * kosztuje pieniądze przy każdym przebiegu pełnej bramki, a z otwartym stdinem czeka
 * w nieskończoność (patrz nagłówek pliku). Zmierzone: T-29 i T-32 miały wszystkie
 * własne kryteria zielone i 15/16 sprawdzeń, i oba padły WYŁĄCZNIE na `full-test`,
 * każdy dokładnie w 3600 s = 2 × budżet 1800 s. Od 2026-08-17 nie wylądowałaby żadna
 * gałąź. Poszerzenie bramki do `--tests` (f553404, 2026-08-16) było słuszne; błędem
 * było wpuszczenie w jej zakres celu bez ograniczenia czasu (a7a2d87, 2026-08-17).
 *
 * Cel NIE jest schowany przed bramką: kryterium T-28 woła go wprost przez
 * `-- --ignored`, więc dalej musi być zielony, tylko nigdy hurtem. */
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "uruchamia prawdziwe procesy claude: kosztuje pieniadze i czeka bez konca; wolany wprost przez kryterium T-28"]
async fn two_real_agents_overlap_in_time_each_in_its_own_folder() -> Result<(), Box<dyn Error>> {
    // Własna kopia katalogu na krok. `tempdir()` żyje do końca testu — skasowany w połowie
    // katalog roboczy byłby awarią agenta, nie pomiarem planisty.
    let first_dir = tempfile::tempdir()?;
    let second_dir = tempfile::tempdir()?;
    let bench = Arc::new(Bench::new(vec![
        first_dir.path().to_path_buf(),
        second_dir.path().to_path_buf(),
    ]));

    // Dwa węzły, ani jednej strzałki: kroki są niezależne, więc jedyne, co może je rozdzielić
    // w czasie, to planista.
    let dag = Dag::new(2, &[])?;
    let run = Arc::clone(&bench);
    let outcome = execute(
        &dag,
        AT_ONCE,
        CancellationToken::new(),
        move |id, cancel| Arc::clone(&run).step(id, cancel),
    )
    .await;

    assert!(
        outcome
            .states
            .iter()
            .all(|state| *state == StepState::Succeeded),
        "both real agents have to finish successfully for the measured windows to mean \
         anything; they ended as {:?} — {}",
        outcome.states,
        bench.complaints()
    );

    let first = bench
        .observed(0)
        .ok_or("step 0 never recorded a window, so there is nothing to compare")?;
    let second = bench
        .observed(1)
        .ok_or("step 1 never recorded a window, so there is nothing to compare")?;

    // ── Dowód: przedziały mają część wspólną ──────────────────────────────────────────────
    let overlap = shared(&first, &second);
    assert!(
        overlap > Duration::ZERO,
        "two independent steps have to occupy overlapping windows when two may run at once. \
         These shared {overlap:?}: step 0 ran for {:?}, step 1 for {:?}, and anything at zero \
         is a scheduler that dispatches wide and runs one at a time — the defect this whole \
         task exists to rule out (invariant 11)",
        first.ended_at.saturating_duration_since(first.started_at),
        second.ended_at.saturating_duration_since(second.started_at)
    );

    // ── Dowód: to były dwa procesy, nie jeden ─────────────────────────────────────────────
    let first_group = first
        .group
        .ok_or("step 0 ran without a process group, so there is no process to tell apart")?;
    let second_group = second
        .group
        .ok_or("step 1 ran without a process group, so there is no process to tell apart")?;
    assert_ne!(
        first_group.pid, second_group.pid,
        "the two steps have to be two processes; one pid means one agent answered twice"
    );
    assert_ne!(
        first_group.pgid, second_group.pgid,
        "each agent has to lead its own process group, otherwise stopping one of them signals \
         the other as well (invariant 6)"
    );

    // ── Dowód: każdy w swojej kopii katalogu ──────────────────────────────────────────────
    assert_ne!(
        first.cwd, second.cwd,
        "each agent works in its own folder; one folder for both is two steps writing over the \
         same paths, which is what the workflow validator refuses at save time (invariant 12)"
    );

    Ok(())
}
