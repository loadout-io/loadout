//! AC-3 dla T-52: folder, który repozytorium NIE jest, dostaje izolację mimo egzotycznych
//! kształtów plików.
//!
//! Zmierzone 2026-08-19: `copy_project_into` pytał `entry.file_type()`, a ta **nie podąża** za
//! dowiązaniem. Dowiązanie do katalogu wyglądało więc na „nie katalog", szło do `fs::copy`,
//! a `fs::copy` za nim podążało i odmawiało — cały bieg stawał na jednym wpisie
//! (`meetnotes/.claude/worktrees/murmur-server`). Takie wpisy robią same `pnpm`, `python -m
//! venv`, `git worktree` i worktree Claude Code.
//!
//! **Słaba wersja tego kryterium:** test na samo dowiązanie do katalogu. To jest jeden zmierzony
//! kształt, a klasa błędu jest szersza niż jej pierwszy przedstawiciel.

use std::error::Error;
use std::fs;
use std::os::unix::fs::{FileTypeExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

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
use loadout_lib::store::Store;
use tempfile::TempDir;
use tokio::sync::mpsc;

const VENDOR: &str = "claude-code";
const PATIENCE: Duration = Duration::from_secs(20);

const PLAIN: &str = "notes.txt";
const PLAIN_TEXT: &str = "an ordinary file";
const LINK_TO_DIR: &str = "link-to-a-folder";
const BROKEN_LINK: &str = "link-to-nowhere";
const FIFO: &str = "a-queue";

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_every_shape",
  "name": "One step on its own copy",
  "steps": [
    {
      "kind": "agent",
      "id": "s_one",
      "name": "Only",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "read what is there",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
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
giveUpAfterMinutes: 20
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Do the work.
";

/// **Bieg jedzie w OSOBNYM WĄTKU i mierzymy go zegarem, a nie `tokio::time::timeout`.**
///
/// Zmierzone 2026-08-19: `fs::copy` na kolejce FIFO nie odmawia — **blokuje się na zawsze**,
/// bo otwarcie kolejki do odczytu czeka na piszącego, który nigdy nie przyjdzie. To jest gorsze
/// niż odmowa, ale dla kryterium jest bezużyteczne: zawieszenie nie jest czerwienią, tylko
/// przekroczonym czasem (`NOT_A_REAL_RED`). Blokada siedzi w kodzie synchronicznym, więc żaden
/// asynchroniczny limit czasu jej nie przerwie — jedyne, co zostaje, to zmierzyć ją z zewnątrz
/// i POWIEDZIEĆ, co się stało. Zablokowany wątek zostaje; kończy go wyjście procesu testowego.
#[test]
fn a_folder_that_is_not_a_repo_still_gets_its_own_copy() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let project = bench.project.path().to_path_buf();

    // Fikstura i jej kontrola stoją osobno: to jest OSOBNE pytanie („czy w źródle naprawdę
    // są te kształty") od pytania, które zadaje ten test („czy bieg je przeżywa").
    let _elsewhere = lay_out_every_shape(&project)?;

    let seen = Arc::new(Mutex::new(None::<PathBuf>));
    let db = bench.db();
    let workflow = bench.workflow("every-shape", WORKFLOW)?;

    let waiting = run_in_its_own_thread(Setup {
        home: bench.home.path().to_path_buf(),
        project: project.clone(),
        db,
        workflow,
        seen: Arc::clone(&seen),
    });

    // (a) Bieg RUSZA i WRACA. Do 2026-08-19 kończył się tu odmową na pierwszym dowiązaniu —
    //     albo, kiedy pierwsza w kolejności okazała się kolejka, nie kończył się wcale.
    let steps = match waiting.recv_timeout(PATIENCE) {
        Ok(Ok(steps)) => steps,
        Ok(Err(said)) => {
            return Err(format!(
                "the run refused instead of running: {said}. None of these file shapes is a \
                 reason to stop a run — every one of them turns up in a real project"
            )
            .into());
        }
        Err(_) => {
            return Err(format!(
                "the run did not come back within {PATIENCE:?}. Copying a fifo byte by byte \
                 blocks for ever: opening it for reading waits for a writer that never arrives. \
                 A step that never starts and never fails is the worst of the three outcomes, \
                 because nothing on the screen says why"
            )
            .into());
        }
    };

    assert_eq!(
        steps,
        vec![StepState::Succeeded],
        "the step ended as {steps:?}"
    );

    let cwd = seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .ok_or("the step never reached the driver")?;

    // (b) Zwykły plik jest, z tą samą treścią.
    assert_eq!(
        fs::read_to_string(cwd.join(PLAIN))?,
        PLAIN_TEXT,
        "the ordinary file did not arrive with its content"
    );
    assert!(
        cwd.join("src").join("main.rs").exists(),
        "the nested file did not arrive: a project is a tree, not a list"
    );

    // (c) Dowiązania są DOWIĄZANIAMI, nie kopiami swojego celu.
    let link = fs::symlink_metadata(cwd.join(LINK_TO_DIR))
        .map_err(|error| format!("{LINK_TO_DIR} is missing from the step's copy: {error}"))?;
    assert!(
        link.file_type().is_symlink(),
        "{LINK_TO_DIR} arrived as something other than a link. Following it copies a whole \
         unrelated tree into every step of every run — here that is a second repository"
    );
    let broken = fs::symlink_metadata(cwd.join(BROKEN_LINK))
        .map_err(|error| format!("{BROKEN_LINK} is missing from the step's copy: {error}"))?;
    assert!(
        broken.file_type().is_symlink(),
        "the broken link did not survive as a link"
    );

    Ok(())
}

// ── fikstura i bieg, każde pod swoją nazwą ─────────────────────────────────────────────────

/// Układa w projekcie pięć kształtów i **sprawdza, że naprawdę tam są**.
///
/// Kontrola jest tu, a nie w teście, bo bez niej całe kryterium może zmierzyć pusty katalog:
/// `mkfifo` albo dowiązanie mogło się nie udać, a wtedy zieleń nie mówi nic. Oddaje katalog,
/// na który wskazuje dowiązanie — musi przeżyć do końca testu, inaczej dowiązanie zrywa się
/// samo i test bada inny przypadek, niż sądzi.
fn lay_out_every_shape(project: &Path) -> Result<TempDir, Box<dyn Error>> {
    // Pięć kształtów. Żaden nie jest wymyślony: pierwsze trzy zmierzono w prawdziwym repo.
    fs::write(project.join(PLAIN), PLAIN_TEXT)?;
    fs::create_dir_all(project.join("src"))?;
    fs::write(project.join("src").join("main.rs"), "fn main() {}")?;
    let elsewhere = TempDir::new()?;
    fs::write(elsewhere.path().join("inside.txt"), "over here")?;
    symlink(elsewhere.path(), project.join(LINK_TO_DIR))?;
    symlink("/no/such/path/anywhere", project.join(BROKEN_LINK))?;
    let made = Command::new("mkfifo").arg(project.join(FIFO)).status()?;
    assert!(
        made.success(),
        "mkfifo did not run, so this test cannot measure a fifo"
    );

    // (d) Kontrola przeciw pustemu czytaniu: wszystkie cztery egzotyczne kształty NAPRAWDĘ są.
    assert!(
        fs::symlink_metadata(project.join(LINK_TO_DIR))?
            .file_type()
            .is_symlink(),
        "the link to a directory was not created, so this test would measure an empty folder"
    );
    assert!(
        fs::symlink_metadata(project.join(BROKEN_LINK))?
            .file_type()
            .is_symlink(),
        "the broken link was not created"
    );
    assert!(
        fs::symlink_metadata(project.join(FIFO))?
            .file_type()
            .is_fifo(),
        "the fifo was not created"
    );
    assert!(
        project.join("src").is_dir(),
        "the nested directory was not created"
    );
    assert!(
        !project.join(".git").exists(),
        "this criterion is about a folder that is NOT a repository, and this one is"
    );
    Ok(elsewhere)
}

/// Wszystko, czego bieg potrzebuje po drugiej stronie granicy wątku.
struct Setup {
    home: PathBuf,
    project: PathBuf,
    db: PathBuf,
    workflow: PathBuf,
    seen: Arc<Mutex<Option<PathBuf>>>,
}

/// Puszcza bieg w OSOBNYM wątku i oddaje odbiornik, na którym da się czekać z zegarem.
///
/// Powód stoi przy teście: blokada siedzi w kodzie synchronicznym, więc żaden asynchroniczny
/// limit czasu jej nie przerwie.
fn run_in_its_own_thread(
    setup: Setup,
) -> std::sync::mpsc::Receiver<Result<Vec<StepState>, String>> {
    let (done, waiting) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                // Odbiornik po drugiej stronie czeka z zegarem, więc milczenie tutaj wyglądałoby
                // jak zawieszony bieg — czyli jak dokładnie ten defekt, który ten test mierzy.
                let _ = done.send(Err(format!("the test runtime would not start: {error}")));
                return;
            }
        };
        let outcome = runtime.block_on(async move {
            let store = Store::open(&setup.db).map_err(|error| error.to_string())?;
            let deps = RunDeps {
                home: &setup.home,
                project: &setup.project,
                store: &store,
                drivers: fake_drivers(setup.seen),
                processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
                control: RunControl::new(),
            };
            let request = RunRequest {
                workflow: setup.workflow,
                how_many_at_once: 1,
                task: None,
                only: None,
                handoffs_from: None,
            };
            let recorder = Delivered::default();
            let (sink, source) = line_channel(QUEUE_CAP);
            let pump = spawn_pump(source, recorder.channel());
            let report = run_workflow_inner(&deps, &request, sink)
                .await
                .map_err(|error| error.to_string())?;
            let _ = pump.await;
            Ok::<_, String>(report.steps)
        });
        let _ = done.send(outcome);
    });
    waiting
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers(seen: Arc<Mutex<Option<PathBuf>>>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { seen });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct Fake {
    seen: Arc<Mutex<Option<PathBuf>>>,
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
        *self.seen.lock().unwrap_or_else(PoisonError::into_inner) = Some(spec.cwd.clone());
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

    async fn cancel(&mut self) -> GroupProof {
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
