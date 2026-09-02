//! Z-4: ocalały po **nieudanym starcie** zabiera ze sobą miejsce kroku i mówi o sobie człowiekowi.
//!
//! Start vendora potrafi paść nad grupą, która już wstała — App Server bez strumienia wyjściowego,
//! uzgodnienie, które nie doszło do skutku (`engine::drivers::codex`). Nikt na zewnątrz nie ma
//! wtedy uchwytu: `AgentDriver::start` oddaje `Err`, a nadzorowany proces ginie razem z ramką
//! sterownika. `Drop` posyła mu dziewiątkę, ale **nie dowodzi `ESRCH`** (niezmiennik 6).
//!
//! Do 2026-09 kosztowało to trzy rzeczy naraz, i wszystkie trzy mierzy ten plik:
//!
//! 1. **miejsce z puli** — permit należy do KROKU i jest brany PRZED `start` (`Live::step`), więc
//!    ocalały wjeżdżał do rejestru z `slot: None`, a miejsce wracało do puli razem z ramką kroku.
//!    Następny agent po ~583 MB startował obok grupy, która dalej pali limit (niezmiennik 11);
//! 2. **adres** — `run.json` tego kroku nie miał `pgid`, więc nie było czego szukać w `ps`
//!    ani po czym sprzątać przy następnym starcie [T7 §6.2];
//! 3. **zdanie** — historia kroku mówiła wyłącznie „nie wystartował". O żywej grupie ani słowa,
//!    czyli dokładnie ta klasa, dla której istnieje niezmiennik 29.
//!
//! # Droga jest PRODUKCYJNA, nie ręczna
//!
//! Kryterium nie woła `Processes::keep_leftover` samo. Idzie przez `run_workflow_inner` →
//! `Live::step` (bierze permit) → `configured_driver_for_agent` (wpina rejestr szwem
//! `AgentDriver::leaving_leftovers_with`) → `start` sterownika, który oddaje ocalałego i pada.
//! Dubler stoi w miejscu `CodexDriver` i robi dokładnie to samo, co on: `keep_leftover(owner,
//! None)` — miejsce ma dołożyć warstwa wyżej, bo sterownik żadnego nie trzyma. Ponowienie idzie
//! `Processes::close()`, czyli tą samą drogą, którą woła `lib.rs` przy zamykaniu okna.
//!
//! # Dlaczego to jest OSOBNY moduł
//!
//! Bo jako jedyny w tej fali nazywa API, którego na starym drzewie NIE MA
//! (`supervisor::{Leftover, KeepsLeftovers}`, `AgentDriver::leaving_leftovers_with`). Postawiony
//! obok kryteriów Z-4 przewracał cały cel integracyjny na etapie kompilacji, więc tamte dwa nie
//! uruchamiały się wcale — a test, który się nie skompilował, czyta się dokładnie jak zdany
//! (`AGENTS.md` §2a punkt 4). `a_run_always_settles.rs` stoi wyłącznie na API sprzed poprawki
//! i pada tam w wykonaniu; nowe typy sądzi ten moduł.
//!
//! # Czego tu nie ma i dlaczego
//!
//! Prawdziwego `GroupProof::Alive` od prawdziwego `Supervised` nie da się wywołać: wymaga procesu,
//! który przeżył SIGKILL, a `SIGKILL` jest nieprzechwytywalny i zombie zbiera sam
//! `Supervised::stop`. Fałszywy jest więc sam vendor — dokładnie tak, jak w każdym innym kryterium
//! tego drzewa, które sądzi zejście grupy (`t134_live_stop_has_a_ceiling.rs`). Cała droga między
//! krokiem a rejestrem jest prawdziwa.

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunReport, RunRequest};
use loadout_lib::engine::drivers::{AgentDriver, AgentHandle, DecodedEvent, Probe, RunSpec};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof, KeepsLeftovers, Leftover};
use loadout_lib::ipc::{AppState, QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use serde_json::Value;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Wyższa niż cokolwiek, co ta ława robi naprawdę; skończona, żeby zacięcie padło na asercji.
const PATIENCE: Duration = Duration::from_mins(1);

/// Adres grupy, którą nieudany start zostawił za sobą.
const SURVIVOR_PID: i32 = 913_579;
const SURVIVOR_PGID: i32 = 913_580;

/// Zdanie, które Rust zapisuje krokowi po grupie ocalałej z pełnej eskalacji.
///
/// Kopia `STEP_SURVIVOR_ERROR` z `commands/run.rs`, co do znaku: kryterium nazywa ZDANIE, nie
/// stałą. To samo zdanie pada po limicie czasu kroku i po turze, która wróciła błędem — na
/// wszystkich drogach z `death_proof: false` poza żywym Stopem stoi jedna odpowiedź.
const SURVIVOR_ERROR: &str = "\
This step finished its work, but Loadout could not make sure everything it started had stopped, \
so some of it may still be running.";

const AGENT: &str = r#"---
schema: 1
id: 01990000-0000-7000-8000-000000000905
name: Z4 Starter
summary: Never gets off the ground
color: slate
runsWith: claude-code
model: haiku
thinking: balanced
fileAccess: look-only
giveUpAfterMinutes: 20
writeResultsTo: ""
tools: everything
skills: []
connections: []
---
Try to start.
"#;

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_z4_leftover",
  "name": "Z4 leftover start",
  "steps": [{
    "kind": "agent",
    "id": "build",
    "name": "Build",
    "agent": "01990000-0000-7000-8000-000000000905",
    "overrides": {},
    "instructions": "Try to start.",
    "folder": { "use": "project" },
    "at": { "x": 0, "y": 0 }
  }],
  "links": []
}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_start_hands_its_group_and_the_seat_of_that_step_to_the_registry()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let asked = Arc::new(AtomicUsize::new(0));
    // `AppState`, nie ręczne `RunDeps`: zapadka folderu mieszka wyłącznie tutaj, a bez niej
    // „bieg osiadł" dowodziłoby się samym powrotem funkcji, nie przyjęciem następnego Startu.
    let app = bench.app(drivers_for(FailsToStart {
        asked: Arc::clone(&asked),
        keeper: None,
        held: Arc::new(Mutex::new(None)),
    }))?;
    let deps = app
        .begin_run(bench.project.path())
        .map_err(std::io::Error::other)?;
    let request = RunRequest {
        workflow: bench.workflow.clone(),
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (lines, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, lines))
        .await
        .map_err(|_| "the run never came back".to_owned())??;
    tokio::time::timeout(PATIENCE, pump).await??;

    assert_eq!(
        report.steps,
        vec![StepState::Failed],
        "a step whose agent app never started has to end failed"
    );
    assert_eq!(
        asked.load(Ordering::Acquire),
        0,
        "the fixture is wrong: nobody was supposed to ask this group for a proof during the run \
         itself, so the counts below would measure two things at once"
    );

    // ── Miejsce: ocalały zabrał je ze sobą ────────────────────────────────────────────────
    assert_eq!(
        deps.control.slots().running_now(),
        1,
        "the seat this step took before its agent app started went back to the pool, while the \
         group that start left behind is still answering signal zero. The next step takes that \
         seat, at ~583 MB, next to something nobody can account for (invariant 11)"
    );

    // ── Adres i zdanie: tam, gdzie czyta je człowiek ──────────────────────────────────────
    the_person_is_told_about_the_survivor(&bench, &report)?;

    // ── Bieg OSIADŁ: folder przyjmuje następny Start bez restartu Loadouta ─────────────────
    // To jest druga połowa „bieg zawsze osiada": sam powrót funkcji nie dowodzi, że `settle()`
    // zapadło — dowodzi tego dopiero zapadka, która puściła (`AppState::begin_run`).
    let next = app.begin_run(bench.project.path());
    assert!(
        next.is_ok(),
        "the folder still refuses every Start after a start that left a live group behind, so \
         the only way back into it is restarting Loadout. It said: {:?}",
        next.err()
    );

    // ── Ponowienie na TYM SAMYM właścicielu, i zwolnienie dokładnie raz ────────────────────
    // Produkcyjna droga: to `Processes::close` woła `lib.rs`, kiedy człowiek zamyka okno.
    let proofs = tokio::time::timeout(PATIENCE, deps.processes.close())
        .await
        .map_err(|_| "closing the window never came back".to_owned())?;
    assert!(
        matches!(proofs[..], [GroupProof::Alive { .. }]),
        "closing the window had to reach the group that start left behind and come back with the \
         honest answer about it: {proofs:?}"
    );
    assert_eq!(
        asked.load(Ordering::Acquire),
        1,
        "the owner was kept but never asked again, which is a leak with one extra step"
    );
    assert_eq!(
        deps.control.slots().running_now(),
        1,
        "the seat came back before the proof did"
    );

    let proofs = tokio::time::timeout(PATIENCE, deps.processes.close())
        .await
        .map_err(|_| "the second close never came back".to_owned())?;
    assert!(
        matches!(proofs[..], [GroupProof::Dead { .. }]),
        "the second close had to reach the same owner and get its proof: {proofs:?}"
    );
    assert_eq!(
        deps.control.slots().running_now(),
        0,
        "the seat comes back with the proof, not before it and not twice"
    );
    assert!(
        tokio::time::timeout(PATIENCE, deps.processes.close())
            .await
            .map_err(|_| "the third close never came back".to_owned())?
            .is_empty(),
        "the third close found something to stop again, so the second one released it without \
         taking it off the list"
    );
    Ok(())
}

/// Kryterium adresu i zdania: `run.json` oraz panel historii mówią to samo o ocalałej grupie.
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
        "the address of the group a failed start left behind is the one thing a person can act \
         on, and it is the one thing recovery looks for at the next start"
    );
    assert_eq!(
        step.get("death_proof").and_then(Value::as_bool),
        Some(false),
        "a step over a group nobody proved dead was written down as proven dead"
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
    assert_eq!(
        past.steps[0].error, SURVIVOR_ERROR,
        "the history of this run says only that the agent app would not start. A person reading \
         it never learns that something it did start is still going, and that is the half that \
         keeps costing money"
    );
    Ok(())
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
        fs::write(home.path().join("agents").join("z4-starter.md"), AGENT)?;
        let workflow = home.path().join("workflows").join("z4-leftover.json");
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
}

fn drivers_for(driver: FailsToStart) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(driver);
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Sterownik w miejscu `CodexDriver`: start pada, ale grupa już wstała i nie da się jej dowieść.
///
/// Klon z rejestrem, a nie pole ustawiane w miejscu — bo tak wygląda produkcyjny szew
/// [`AgentDriver::leaving_leftovers_with`]: bieg trzyma sterownik jako `Arc<dyn AgentDriver>`,
/// więc jedyną drogą podania mu czegokolwiek jest oddanie klona.
#[derive(Clone)]
struct FailsToStart {
    asked: Arc<AtomicUsize>,
    keeper: Option<Arc<dyn KeepsLeftovers>>,
    /// Nadajnik zdarzeń, którego ten sterownik **nie oddaje**, choć start padł.
    ///
    /// 2026-09 (Z-4) — to nie jest ozdoba fikstury, tylko odtworzenie produkcyjnego Codeksa:
    /// `start_app_conversation` wystawia czytnika (`app_server_actor`) PRZED uzgodnieniem, więc
    /// uzgodnienie, które padło, zostawia żywe zadanie trzymające `events`. Porzucenie
    /// `JoinHandle` go nie przerywa, a przy `Alive` czytniki zostają przy uchwycie z rozmysłu.
    /// Dubler, który upuszcza nadajnik od razu, zamyka kolejkę kuratora za produkcję i przechodzi
    /// kryterium, którego produkcja nie przechodzi.
    held: Arc<Mutex<Option<mpsc::Sender<DecodedEvent>>>>,
}

#[async_trait]
impl AgentDriver for FailsToStart {
    fn id(&self) -> &'static str {
        "z4-fails-to-start"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("z4".to_owned()),
        })
    }

    fn leaving_leftovers_with(
        &self,
        keeper: Arc<dyn KeepsLeftovers>,
    ) -> Option<Arc<dyn AgentDriver>> {
        let mut configured = self.clone();
        configured.keeper = Some(keeper);
        Some(Arc::new(configured))
    }

    async fn start(
        &self,
        _spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        // Nadajnik ZOSTAJE, dokładnie jak u Codeksa po nieudanym uzgodnieniu — powód przy polu.
        *self.held.lock().unwrap_or_else(PoisonError::into_inner) = Some(events);
        let keeper = self.keeper.as_ref().ok_or_else(|| {
            anyhow::anyhow!("the run never handed this driver a leftover registry")
        })?;
        /* `None` NA MIEJSCU Z PULI, i to nie jest uproszczenie fikstury: dokładnie to podaje
         * produkcyjny `codex.rs`, bo sterownik żadnego permitu nie trzyma — permit należy do kroku
         * i dołożyć go musi warstwa wyżej. Cała treść tego kryterium siedzi w tym `None`. */
        keeper.keep_leftover(
            Box::new(StubbornGroup {
                asked: Arc::clone(&self.asked),
                dead_after: 2,
            }),
            None,
        );
        Err(anyhow::anyhow!(
            "the Z-4 fixture could not start its agent app"
        ))
    }
}

/// Grupa, która na pierwszą eskalację odpowiada „nadal żyję", a dopiero na kolejną oddaje dowód.
struct StubbornGroup {
    asked: Arc<AtomicUsize>,
    dead_after: usize,
}

#[async_trait]
impl Leftover for StubbornGroup {
    async fn ask_again(&mut self) -> GroupProof {
        if self.asked.fetch_add(1, Ordering::AcqRel) + 1 >= self.dead_after {
            return GroupProof::Dead { status: None };
        }
        GroupProof::Alive {
            group: self.address(),
        }
    }

    fn address(&self) -> Option<GroupId> {
        Some(GroupId {
            pid: SURVIVOR_PID,
            pgid: SURVIVOR_PGID,
        })
    }
}
