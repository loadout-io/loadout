//! Z-4: bieg osiada na KAŻDEJ drodze, a folder przyjmuje następny Start bez restartu Loadouta.
//!
//! Dwie drogi, na których do 2026-09 nie osiadał nigdy, obie zgłoszone w audycie 2026-09-02
//! (E-4, E-5):
//!
//! 1. **Grupa, która nie odpowiada `ESRCH`.** Krok po limicie czasu szedł przez
//!    `Live::prove_agent_dead`, a ta pętla ponawiała pełną eskalację **bez sufitu** — choć żywy
//!    Stop miał swój sufit od T-134, a rejestr `Processes::unproven` czekał na ocalałych od
//!    T-201. Bieg nie wracał, `settle()` nie zapadało i zapadka folderu odmawiała każdego
//!    następnego Startu.
//! 2. **Panika w ciele biegu.** `settle()` stało ZA `await`, więc odwijanie stosu przechodziło
//!    obok niego i `stop_if_anything_is_going` czekało na dowód, którego nikt już nie zapali.
//!
//! Oba testy biegną pod zatrzymanym zegarem i oba mierzą to samo zdanie kryterium: bieg schodzi,
//! człowiek dostaje zdanie o ocalałym, a następny Start w tym samym folderze wchodzi.
//!
//! # Ten moduł stoi WYŁĄCZNIE na API sprzed poprawki, i to jest jego warunek istnienia
//!
//! `run_workflow_with_reflection`, `run_workflow_with_prestart_faults`, `stop_if_anything_is_going`,
//! `read_run_inner`, `AppState::begin_run` i `RunControl::slots` istnieją w tym drzewie od dawna —
//! ani jedna linia niżej nie nazywa typu ani metody, które dokłada Z-4. Dzięki temu ten cel
//! integracyjny **kompiluje się na starym kodzie** i oba testy padają tam w WYKONANIU, na
//! `tokio::time::timeout`, a nie na braku symbolu (`AGENTS.md` §2a punkt 4: test, który się nie
//! skompilował, niczego nie uruchomił i czyta się dokładnie jak zdany).
//!
//! Jedyne kryterium tej fali, które nowego API potrzebuje, mieszka osobno —
//! `a_leftover_that_is_not_an_agent_keeps_its_seat.rs` — i powód rozdzielenia stoi w jego
//! nagłówku. Dopisanie go tutaj przewracało kompilację CAŁEGO celu, więc te dwa testy nie
//! uruchamiały się wcale.

// `clippy::panic` jest w tym drzewie `deny` i sądzi także `--all-targets`. Wstrzykiwacz niżej
// panikuje CELOWO — panika jest tu materiałem kryterium, a nie niedopatrzeniem.
#![allow(clippy::panic)]

use std::error::Error;
use std::fs;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::run::{
    PrestartFaultInjector, PrestartFaultPoint, RolledBackResource,
    run_workflow_with_prestart_faults, run_workflow_with_reflection, stop_if_anything_is_going,
};
use loadout_lib::commands::{Drivers, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{AppState, QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use serde_json::Value;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Cierpliwość obu testów, liczona w czasie WIRTUALNYM (`start_paused`).
///
/// **Większa niż limit kroku**, i to jest cała treść tej liczby. Pod zatrzymanym zegarem tokio
/// przewija czas do najbliższego terminu, więc cierpliwość krótsza od minuty kroku wypaliłaby
/// pierwsza i test mierzyłby własny sufit zamiast sufitu produkcyjnego (ten sam powód stoi przy
/// `every_failure_leaves_its_last_words.rs`). Żadna prawdziwa sekunda tędy nie przechodzi.
const PATIENCE: Duration = Duration::from_mins(10);

/// Adres grupy, która na każdą eskalację odpowiada „nadal żyję".
const SURVIVOR_PID: i32 = 913_579;
const SURVIVOR_PGID: i32 = 913_580;

/// Zdanie, które ma zobaczyć CZŁOWIEK — w `run.json` i w panelu historii (niezmiennik 29).
/// Kopia stałej `STEP_SURVIVOR_ERROR` z produkcji: kryterium nazywa zdanie, a nie stałą.
const SURVIVOR_ERROR: &str = "\
This step finished its work, but Loadout could not make sure everything it started had stopped, \
so some of it may still be running.";

/// Odmowa zapadki folderu — ta, która po tej wadzie stała aż do restartu aplikacji.
const ALREADY_GOING: &str = "A run is already going";

const AGENT: &str = r#"---
schema: 1
id: 01990000-0000-7000-8000-000000000904
name: Z4 Builder
summary: Runs past its own limit
color: slate
runsWith: claude-code
model: haiku
thinking: balanced
fileAccess: look-only
giveUpAfterMinutes: 1
writeResultsTo: ""
tools: everything
skills: []
connections: []
---
Run past the limit.
"#;

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_z4_settles",
  "name": "Z4 always settles",
  "steps": [{
    "kind": "agent",
    "id": "build",
    "name": "Build",
    "agent": "01990000-0000-7000-8000-000000000904",
    "overrides": {},
    "instructions": "Run past the limit.",
    "folder": { "use": "project" },
    "at": { "x": 0, "y": 0 }
  }],
  "links": []
}"#;

/* KRYTERIUM 1-3. Krok przekracza swoją minutę w czasie wirtualnym i wchodzi w `stop_overdue_
 * agent`, a jego grupa odpowiada `Alive` na każdą eskalację.
 *
 * NA STARYM KODZIE `prove_agent_dead` kręci się po sekundzie wirtualnej BEZ KOŃCA, więc bieg
 * nie wraca, `settle()` nie zapada i wypala się `PATIENCE` niżej. To jest dokładnie ta wada:
 * jeden krok nad grupą, której nie da się dowieść, zabiera folder do restartu Loadouta. */
#[tokio::test(start_paused = true)]
async fn a_step_whose_group_never_answers_esrch_fails_and_lets_the_next_start_in()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let fake = Arc::new(Fake::new());
    let app = bench.app(fake_drivers(Arc::clone(&fake)))?;
    let deps = app
        .begin_run(bench.project.path())
        .map_err(std::io::Error::other)?;
    let (lines, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let request = bench.request();

    let finished = tokio::time::timeout(
        PATIENCE,
        run_workflow_with_reflection(&deps, &request, lines, None, false),
    )
    .await;
    assert!(
        finished.is_ok(),
        "the run did not come back within {PATIENCE:?} of its own clock. The fake agent answers \
         every escalation with GroupProof::Alive, so this is the missing ceiling on the proof \
         loop of a step that already finished its work, not a slow shutdown (escalations so far: \
         {})",
        fake.cancel_calls.load(Ordering::Acquire)
    );
    let Ok(ran) = finished else {
        return Err("the assertion above returned without a finished run".into());
    };
    let report = ran?;
    tokio::time::timeout(PATIENCE, pump).await??;

    assert_eq!(report.steps, vec![StepState::Failed]);
    assert_eq!(
        fake.cancel_calls.load(Ordering::Acquire),
        3,
        "the retry ceiling is a production policy: this test passes no attempt count"
    );
    the_person_is_told_about_the_survivor(&bench, &report)?;
    the_survivor_keeps_its_seat(&deps);
    the_next_start_in_this_folder_goes_through(&bench, &app, &fake).await
}

/// Kryterium 1: `run.json` i panel historii mówią OBA to samo zdanie o ocalałym.
fn the_person_is_told_about_the_survivor(
    bench: &Bench,
    report: &RunReport,
) -> Result<(), Box<dyn Error>> {
    let run: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    let step = run
        .pointer("/steps/0")
        .ok_or("run.json did not preserve its only step")?;
    assert_eq!(
        step.get("pgid").and_then(Value::as_i64),
        Some(i64::from(SURVIVOR_PGID)),
        "the address of the group nobody could prove dead has to survive in the file"
    );
    assert_eq!(
        step.get("death_proof").and_then(Value::as_bool),
        Some(false),
        "a step over a group that never answered ESRCH was written down as proven dead"
    );
    assert_eq!(
        step.get("error").and_then(Value::as_str),
        Some(SURVIVOR_ERROR)
    );

    // Ta sama treść tam, gdzie ją CZYTA człowiek (niezmiennik 29): zdanie w pliku, którego panel
    // historii nie pokazuje, jest zdaniem, którego nikt nie zobaczy.
    let folder = report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the run directory has no UTF-8 folder name")?;
    let past = read_run_inner(bench.project.path(), folder)?;
    assert_eq!(past.steps.len(), 1);
    assert_eq!(past.steps[0].error, SURVIVOR_ERROR);
    Ok(())
}

/// Kryterium 3: ocalały zachowuje miejsce we wspólnej puli — Loadout nie kłamie o tym, ile
/// naprawdę biegnie (niezmiennik 11).
fn the_survivor_keeps_its_seat(deps: &loadout_lib::commands::RunDeps<'_>) {
    assert_eq!(
        deps.control.slots().running_now(),
        1,
        "the seat of a group Loadout could not prove dead went back to the pool, so the next \
         agent starts next to something that is still burning the vendor's limit"
    );
}

/// Kryterium 2: zapadka folderu jest zwolniona, a drugi bieg dochodzi do końca.
async fn the_next_start_in_this_folder_goes_through(
    bench: &Bench,
    app: &AppState,
    fake: &Fake,
) -> Result<(), Box<dyn Error>> {
    let next = app.begin_run(bench.project.path()).map_err(|refused| {
        assert!(
            !refused.contains(ALREADY_GOING),
            "the folder still refuses every Start after a step whose group could not be proved \
             dead, so the only way back into it is restarting Loadout. It said: {refused}"
        );
        std::io::Error::other(refused)
    })?;
    let (lines, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let mut request = bench.request();
    // Drugie miejsce jest jawną decyzją wołającego: ocalały trzyma pierwsze i nie wolno go
    // odziedziczyć po grupie, którą Loadout nadal uczciwie liczy jako żywą (T-201).
    request.how_many_at_once = 2;
    let report = tokio::time::timeout(
        PATIENCE,
        run_workflow_with_reflection(&next, &request, lines, None, false),
    )
    .await??;
    tokio::time::timeout(PATIENCE, pump).await??;

    assert_eq!(report.steps, vec![StepState::Succeeded]);
    assert_eq!(fake.starts.load(Ordering::Acquire), 2);
    Ok(())
}

/* TA SAMA GRUPA, DRUGA DROGA WYJŚCIA: tura, która wróciła BŁĘDEM sterownika.
 *
 * `Ended::Overdue` wyżej i `Ended::Turn(Err)` tutaj schodzą tą samą pętlą dowodową
 * (`prove_agent_dead`), więc do 2026-09 wisiały identycznie — a po naprawie sufitu różniły się
 * jeszcze zdaniem: tamta mówiła o ocalałym, ta zostawiała człowiekowi wyłącznie awarię tury.
 * Obie odpowiedzi są prawdziwe, ale tylko jedna każe komuś sprawdzić maszynę, a `death_proof:
 * false` znaczy na obu drogach dokładnie to samo.
 *
 * Zdanie sprawdzamy tam, gdzie CZYTA je człowiek — przez `read_run_inner`, czyli tę samą komendę,
 * którą woła panel historii (niezmiennik 29). Wartość w polu struktury dowodziłaby wyłącznie tego,
 * że mechanizm istnieje.
 */
#[tokio::test(start_paused = true)]
async fn a_turn_that_broke_over_a_live_group_says_so_where_a_person_reads_it()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let fake = Arc::new(Fake::broken_turn());
    let app = bench.app(fake_drivers(Arc::clone(&fake)))?;
    let deps = app
        .begin_run(bench.project.path())
        .map_err(std::io::Error::other)?;
    let (lines, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let request = bench.request();

    let finished = tokio::time::timeout(
        PATIENCE,
        run_workflow_with_reflection(&deps, &request, lines, None, false),
    )
    .await;
    assert!(
        finished.is_ok(),
        "the run did not come back within {PATIENCE:?} of its own clock. A turn that came back \
         with an error goes through the very same proof loop as a step that ran out of time, so \
         this is that loop with no ceiling (escalations so far: {})",
        fake.cancel_calls.load(Ordering::Acquire)
    );
    let Ok(ran) = finished else {
        return Err("the assertion above returned without a finished run".into());
    };
    let report = ran?;
    tokio::time::timeout(PATIENCE, pump).await??;

    assert_eq!(report.steps, vec![StepState::Failed]);
    assert_eq!(
        fake.cancel_calls.load(Ordering::Acquire),
        3,
        "the retry ceiling is a production policy: this test passes no attempt count"
    );
    the_person_is_told_about_the_survivor(&bench, &report)?;
    the_survivor_keeps_its_seat(&deps);
    Ok(())
}

/* KRYTERIUM 4. Panika w ciele biegu też prowadzi do `settle()`.
 *
 * NOŚNIKIEM PANIKI JEST SZEW PRESTARTU, NIE STEROWNIK KROKU, i to jest zmierzone: planista łyka
 * panikę zadania kroku (`engine::scheduler`, `let Ok(…) = joined else { continue }`), więc panika
 * sterownika nigdy nie dociera do `the_whole_workflow_with_prestart` i nie odtwarza tej wady.
 * Prestart jest jedynym produkcyjnym, publicznym szwem na futurze samego biegu.
 *
 * Runtime jest ręczny, bo `#[tokio::test]` zjada panikę razem z całym testem — a tu trzeba ją
 * złapać i ZADAĆ PYTANIE po niej. `start_paused` zostaje: gdyby `stop_if_anything_is_going`
 * dalej czekało na dowód, którego nikt nie zapali, czekałoby prawdziwe dziesięć minut. */
#[test]
fn a_panic_in_the_run_body_still_settles_so_stop_answers() -> Result<(), Box<dyn Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .start_paused(true)
        .build()?;
    // Ławka powstaje WEWNĄTRZ tego runtime'u: `Store::open` zakłada pisarza jako zadanie tokio,
    // więc zbudowana obok przewraca test na `NoRuntime`, zanim bieg w ogóle ruszy.
    let (bench, app) = runtime.block_on(async {
        let bench = Bench::new()?;
        let app = bench.app(fake_drivers(Arc::new(Fake::new())))?;
        Ok::<_, Box<dyn Error>>((bench, app))
    })?;
    let deps = app
        .begin_run(bench.project.path())
        .map_err(std::io::Error::other)?;

    let broke = std::panic::catch_unwind(AssertUnwindSafe(|| {
        runtime.block_on(async {
            let (lines, source) = line_channel(QUEUE_CAP);
            let _pump = spawn_pump(source, Channel::new(|_| Ok(())));
            run_workflow_with_prestart_faults(
                &deps,
                &bench.request(),
                lines,
                Arc::new(PanicsInsideTheRun),
            )
            .await
        })
    }));
    assert!(
        broke.is_err(),
        "the fixture is wrong: the run body was supposed to fall over, and it came back with a \
         report instead. Nothing below is measuring the path this criterion is about"
    );

    let answered = runtime
        .block_on(async { tokio::time::timeout(PATIENCE, stop_if_anything_is_going(&deps)).await });
    let Ok(stopped) = answered else {
        return Err(
            "Stop never came back after the run body fell over. The person is left \
                    holding a button that does nothing, over a run that is no longer there, and \
                    the only way out is killing Loadout"
                .into(),
        );
    };
    assert!(
        !stopped?,
        "Stop answered `true` over a run that had already fallen over, so the person is told \
         something was stopped that nobody was running"
    );

    let refused = app.begin_run(bench.project.path()).err();
    assert!(
        refused.is_none(),
        "the folder still refuses a Start after the run body fell over: {refused:?}"
    );
    Ok(())
}

/// Wstrzykiwacz, który przewraca ciało biegu dokładnie tam, gdzie stoi jeszcze przed pierwszym
/// procesem — czyli w tym samym miejscu, w którym przewraca się `expect` albo indeks w prawdziwym
/// biegu.
#[derive(Debug)]
struct PanicsInsideTheRun;

impl PrestartFaultInjector for PanicsInsideTheRun {
    fn check(&self, point: PrestartFaultPoint) -> std::io::Result<()> {
        assert!(
            point != PrestartFaultPoint::BeforeFirstRunFile,
            "Z-4 fixture: the run body falls over here"
        );
        Ok(())
    }

    fn rolled_back(&self, _resource: &RolledBackResource) {}
}

struct Bench {
    home: TempDir,
    project: TempDir,
    workflow: PathBuf,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(home.path().join("agents").join("z4-builder.md"), AGENT)?;
        let workflow = home.path().join("workflows").join("z4-settles.json");
        fs::write(&workflow, WORKFLOW)?;
        Ok(Self {
            home,
            project,
            workflow,
        })
    }

    fn app(&self, drivers: Drivers) -> Result<AppState, Box<dyn Error>> {
        let store = Store::open(&self.project.path().join(".loadout").join("loadout.db"))?;
        Ok(AppState::new(
            self.home.path().to_path_buf(),
            self.project.path().to_path_buf(),
            store,
            drivers,
        ))
    }

    fn request(&self) -> RunRequest {
        RunRequest {
            workflow: self.workflow.clone(),
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        }
    }
}

/// Którą drogę wyjścia kroku odtwarza ten dubler. Obie kończą się grupą bez dowodu `ESRCH`.
#[derive(Clone, Copy)]
enum Scenario {
    /// Tura nie kończy się nigdy, więc krok wychodzi własnym limitem czasu; drugi bieg się udaje.
    OverdueThenSuccessful,
    /// Tura wraca BŁĘDEM sterownika — druga droga, na której `prove_agent_dead` kręciło się
    /// do 2026-09 bez sufitu, a krok mówił człowiekowi wyłącznie o awarii tury.
    BrokenTurn,
}

struct Fake {
    scenario: Scenario,
    starts: AtomicUsize,
    cancel_calls: Arc<AtomicUsize>,
}

impl Fake {
    fn new() -> Self {
        Self::of(Scenario::OverdueThenSuccessful)
    }

    fn broken_turn() -> Self {
        Self::of(Scenario::BrokenTurn)
    }

    fn of(scenario: Scenario) -> Self {
        Self {
            scenario,
            starts: AtomicUsize::new(0),
            cancel_calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

fn fake_drivers(fake: Arc<Fake>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = fake;
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        "z4-fake"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("z4".to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let index = self.starts.fetch_add(1, Ordering::AcqRel);
        let session = SessionRef {
            vendor: "z4-fake",
            id: spec.run_id.to_string(),
        };
        match (self.scenario, index) {
            // Pierwszy bieg: grupa, która nigdy nie odpowie `ESRCH`, i tura, która sama się nie
            // skończy — czyli krok, który wychodzi wyłącznie własnym limitem czasu.
            (Scenario::OverdueThenSuccessful, 0) => Ok(Box::new(WaitingHandle {
                session,
                cancel_calls: Arc::clone(&self.cancel_calls),
            })),
            // Drugi bieg: zwykły, udany krok. Bez niego kryterium 2 dowodziłoby tylko tego, że
            // zapadka puściła, a nie tego, że za nią jest bieg, który dochodzi do końca.
            (Scenario::OverdueThenSuccessful, 1) => {
                Ok(Box::new(SuccessfulHandle { session, events }))
            }
            (Scenario::BrokenTurn, 0) => Ok(Box::new(BrokenTurnHandle {
                session,
                cancel_calls: Arc::clone(&self.cancel_calls),
            })),
            _ => Err(anyhow::anyhow!(
                "the Z-4 fixture started an unexpected agent"
            )),
        }
    }
}

/// Uchwyt, którego tura wraca błędem sterownika nad grupą odpowiadającą „nadal żyję".
///
/// Adres ma ten sam, co [`WaitingHandle`], bo obie drogi kończą się tym samym pytaniem człowieka:
/// co jeszcze biegnie i pod jakim `pgid` tego szukać.
struct BrokenTurnHandle {
    session: SessionRef,
    cancel_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl AgentHandle for BrokenTurnHandle {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        Some(GroupId {
            pid: SURVIVOR_PID,
            pgid: SURVIVOR_PGID,
        })
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        Err(anyhow::anyhow!(
            "the Z-4 fixture's agent app dropped its connection mid-turn"
        ))
    }

    async fn cancel(&mut self) -> GroupProof {
        self.cancel_calls.fetch_add(1, Ordering::AcqRel);
        GroupProof::Alive { group: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(None)
    }
}

struct WaitingHandle {
    session: SessionRef,
    cancel_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl AgentHandle for WaitingHandle {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        Some(GroupId {
            pid: SURVIVOR_PID,
            pgid: SURVIVOR_PGID,
        })
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        std::future::pending().await
    }

    async fn cancel(&mut self) -> GroupProof {
        self.cancel_calls.fetch_add(1, Ordering::AcqRel);
        GroupProof::Alive { group: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(None)
    }
}

struct SuccessfulHandle {
    session: SessionRef,
    events: mpsc::Sender<DecodedEvent>,
}

#[async_trait]
impl AgentHandle for SuccessfulHandle {
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
            text: "Done.".to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::ZERO,
            session: self.session.clone(),
        };
        self.events
            .send(AgentEvent::Finished(outcome.clone()).into())
            .await
            .map_err(|_| anyhow::anyhow!("the Z-4 event receiver closed before Finished"))?;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}
