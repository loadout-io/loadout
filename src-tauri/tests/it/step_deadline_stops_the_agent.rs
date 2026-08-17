//! AC-1 dla T-35: krok ponad swój limit czasu zostaje zatrzymany, a jego grupa procesów
//! udowodniona martwa.
//!
//! `give_up_after_minutes` z definicji agenta nie miało do 2026-08-17 **ani jednego czytelnika**:
//! zaklinowany agent wisiał do ręcznego Stopu. Według taksonomii tego repo to błąd FINANSOWY,
//! nie higieniczny — proces pali limit u dostawcy tak długo, jak długo nikt nie patrzy.
//! `ARCHITECTURE.md` §11 zapowiada właśnie tę ochronę zamiast `--max-turns`.
//!
//! **Słaba asercja:** sprawdzenie, że funkcja wróciła po limicie. Przechodzi ją implementacja
//! owinięta w `tokio::time::timeout`, która anuluje ZADANIE RUSTA i zostawia żywy proces —
//! czyli dokładnie ten błąd finansowy, któremu to kryterium ma zapobiec (niezmiennik 10).
//! Rozróżnia **dowód zejścia grupy**: `cancel()` musi zostać zawołany na sterowniku i musi
//! oddać `GroupProof::Dead`.
//!
//! Zegar biegu przewijamy `tokio::time::pause()`: prawdziwe dwadzieścia minut w teście jest
//! niewykonalne, a limit liczony w minutach nie da się sensownie zejść niżej bez zmiany
//! jednostki w definicji agenta.

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, Outcome as TurnOutcome, Probe, RunSpec, SessionRef,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use tempfile::TempDir;
use tokio::sync::mpsc;

const VENDOR: &str = "claude-code";

/// Limit z definicji agenta, w minutach. Ta sama liczba musi pojawić się w komunikacie.
const GIVE_UP_MINUTES: u32 = 2;

/// Dwa kroki połączone strzałką: pierwszy się zaklinuje, drugi ma NIE WISIEĆ.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_deadline",
  "name": "One wedged step",
  "steps": [
    {
      "kind": "agent",
      "id": "s_wedged",
      "name": "Wedged",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "hang forever",
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_after",
      "name": "After",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "never reached",
      "at": { "x": 240, "y": 0 }
    }
  ],
  "links": [{ "from": "s_wedged", "to": "s_after" }]
}
"#;

const AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000a1
name: Scribe
summary: Writes things down
color: slate
runsWith: claude-code
model: opus
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 2
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Do the work.
";

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_step_over_its_limit_is_stopped_and_its_group_proven_dead() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let watch = Arc::new(Watch::default());
    let store = Store::open(&bench.db())?;

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch)),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: bench.workflow("deadline", WORKFLOW)?,
        how_many_at_once: 2,
    };

    let recorder = Delivered::default();
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, recorder.channel());

    // Zegar jest zatrzymany (`start_paused`), więc tokio przewinie go sam, gdy wszystko będzie
    // spało — a jedyną rzeczą, która wtedy śpi, jest limit kroku. Bez limitu ten bieg nigdy by
    // nie wrócił, i to jest cała treść tego testu.
    let report = tokio::time::timeout(
        Duration::from_hours(1),
        run_workflow_inner(&deps, &request, sink),
    )
    .await
    .map_err(|_| {
        "the run never came back. A wedged agent with no deadline hangs until a human presses \
         Stop -- which is the financial bug this criterion exists for"
    })??;
    let _ = tokio::time::timeout(Duration::from_mins(1), pump).await;

    // (a) Krok padł, a nie „udał się" i nie „został anulowany".
    assert_eq!(
        report.steps[0],
        StepState::Failed,
        "the wedged step has to end as failed: it did not finish its work, and nobody cancelled \
         it. It ended as {:?}",
        report.steps[0]
    );

    // (b) Powód nazywa LIMIT CZASU wraz z liczbą, a nie „coś poszło nie tak".
    // Powód czytamy Z BAZY, a nie z `RunReport`: to tam sięga ekran i tam sięga odzyskiwanie
    // po awarii. Reason żyjący wyłącznie w pamięci procesu znika razem z nim — czyli dokładnie
    // wtedy, kiedy człowiek najbardziej chce wiedzieć, co się stało.
    let why: String = store
        .reader()?
        .query_row(
            "SELECT error FROM steps WHERE run_id = ?1 AND node_key = 's_wedged'",
            [&report.id],
            |row| row.get::<_, Option<String>>(0),
        )?
        .ok_or("the failed step carries no reason in the store")?;
    for expected in [format!("{GIVE_UP_MINUTES} minute"), "limit".to_owned()] {
        assert!(
            why.contains(&expected),
            "the reason has to name the time limit and its number, so the human knows this was \
             Loadout's decision and which value to change. It said: {why}"
        );
    }

    // (c) DOWÓD zejścia grupy. To jest ta asercja, której nie przechodzi `tokio::time::timeout`:
    //     tamto anuluje zadanie Rusta i zostawia żywy proces.
    assert_eq!(
        watch.cancels(),
        1,
        "the deadline has to go through the driver's cancel(), exactly once. Dropping the Rust \
         task instead leaves the process group alive, burning the vendor's limit (invariant 10)"
    );

    // (d) Krok zależny nie wisi.
    assert_eq!(
        report.steps[1],
        StepState::Skipped,
        "the step after the wedged one has to be skipped, not left hanging: a run that never \
         comes back is the same defect one layer up. It ended as {:?}",
        report.steps[1]
    );

    Ok(())
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

/// Ile razy sterownik dostał `cancel()`. To jest dowód, o który chodzi w (c).
#[derive(Debug, Default)]
struct Watch {
    cancels: Mutex<usize>,
}

impl Watch {
    fn cancelled(&self) {
        *self.cancels.lock().unwrap_or_else(PoisonError::into_inner) += 1;
    }

    fn cancels(&self) -> usize {
        *self.cancels.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn fake_drivers(watch: Arc<Watch>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { watch });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler, którego tura NIGDY nie wraca sama.
#[derive(Debug)]
struct Fake {
    watch: Arc<Watch>,
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
        Ok(Box::new(Turn {
            watch: Arc::clone(&self.watch),
            session,
        }))
    }
}

#[derive(Debug)]
struct Turn {
    watch: Arc<Watch>,
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        Some(GroupId {
            pid: 4242,
            pgid: 4242,
        })
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    /// Nigdy nie wraca. Dokładnie tak wygląda zaklinowany agent z otwartym stdinem.
    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        std::future::pending::<()>().await;
        unreachable!("pending() never resolves")
    }

    async fn cancel(&mut self) -> GroupProof {
        self.watch.cancelled();
        GroupProof::Dead { status: None }
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
        Ok(Self { home, project })
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

/// Paczki, które wyszły kanałem. Ten test ich nie sądzi — pompa musi mieć dokąd oddawać.
#[derive(Debug, Clone, Default)]
struct Delivered(Arc<Mutex<Vec<serde_json::Value>>>);

impl Delivered {
    fn channel(&self) -> tauri::ipc::Channel<Vec<loadout_lib::engine::line::Line>> {
        let sink = Arc::clone(&self.0);
        tauri::ipc::Channel::new(move |body| {
            if let tauri::ipc::InvokeResponseBody::Json(text) = body
                && let Ok(value) = serde_json::from_str(&text)
            {
                sink.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(value);
            }
            Ok(())
        })
    }
}
