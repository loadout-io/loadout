//! AC-2 dla T-33: kiedy własnej kopii zrobić się nie da, bieg mówi to WPROST i nie udaje izolacji.
//!
//! Cicha degradacja jest tu groźniejsza niż odmowa. Dwa kroki, które po cichu wylądują na jednym
//! katalogu, przepisują sobie pliki nawzajem — a każdy z nich kończy się „sukcesem", więc ani
//! bramka, ani człowiek nie mają jak tego zobaczyć. Dlatego bieg ma się **zatrzymać, zanim
//! ruszy jakikolwiek proces**, i powiedzieć, czego zabrakło.
//!
//! **Słaba wersja tego kryterium:** `assert!(result.is_err())`. Przechodzi ją implementacja,
//! która wywala bieg bez powiedzenia dlaczego — i przechodzi ją `RunError::Io`, czyli
//! „Permission denied (os error 13)": zdanie o tym, co się nie udało SYSTEMOWI, a nie
//! człowiekowi. Rozróżnia **treść** komunikatu i to, że ani jeden krok nie ruszył.
//!
//! Warunek wymuszamy odbierając prawo ODCZYTU jednemu plikowi projektu. To celuje dokładnie
//! w kopiowanie: katalog biegu powstaje normalnie, plan przechodzi, a dopiero kopia nie ma jak
//! się udać. Pierwsza wersja tego testu odbierała prawo zapisu do `runs/` — i wtedy padało
//! TWORZENIE KATALOGU BIEGU, gołym `RunError::Io`, zanim kopiowanie w ogóle ruszyło. Asercja
//! zaświeciła i miała rację: sprawdzała odmowę, której nie wywołał badany krok.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunError, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use tempfile::TempDir;
use tokio::sync::mpsc;

const VENDOR: &str = "claude-code";
const PATIENCE: Duration = Duration::from_secs(20);

/// Jeden krok na własnej kopii — do odmowy wystarczy jeden.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_no_copy",
  "name": "One step on its own copy",
  "steps": [
    {
      "kind": "agent",
      "id": "s_one",
      "name": "Scribe at work",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "do the work",
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_that_cannot_be_made_stops_the_run_and_says_why() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    fs::write(
        bench.project.path().join("notes.txt"),
        "written by the human",
    )?;

    let started = Arc::new(Mutex::new(Vec::<String>::new()));
    let store = Store::open(&bench.db())?;

    // Plik projektu, którego nie da się PRZECZYTAĆ. Katalog biegu powstanie normalnie, plan
    // przejdzie, a kopiowanie nie ma jak się udać — czyli psujemy dokładnie to, co badamy.
    let locked = bench.project.path().join("locked.txt");
    fs::write(&locked, "you may not read this")?;
    fs::set_permissions(&locked, PermissionsExt::from_mode(0o000))?;

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&started)),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: bench.workflow("no-copy", WORKFLOW)?,
        how_many_at_once: 1,
        task: None,
    };

    let recorder = Delivered::default();
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, recorder.channel());
    let outcome = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| "the run never came back")?;
    let _ = tokio::time::timeout(PATIENCE, pump).await;

    // Sprzątamy uprawnienia PRZED asercjami: `TempDir` nie usunie pliku, którego nie umie
    // przeczytać, a test zostawiający śmieci na dysku psuje następny bieg.
    fs::set_permissions(&locked, PermissionsExt::from_mode(0o644))?;

    // (a) ANI JEDEN krok nie ruszył. To jest ta połowa, której nie sprawdza `is_err()`.
    assert!(
        started
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty(),
        "a step started even though its own copy could not be made. Falling back to a shared \
         folder is silent: both steps would report success while overwriting each other"
    );

    // (b) Odmowa NAZYWA powód, i to po ludzku.
    // `err()`, a nie `panic!` w ramieniu `match`: `clippy::panic` nie przepuszcza paniki w tym
    // drzewie (`[workspace.lints]`), a `?` na `Option` mówi dokładnie to samo i czyta się tak samo.
    let error = outcome.err().ok_or(
        "the run finished instead of refusing to start: a step whose own copy could not be made must \
         not run at all",
    )?;
    assert!(
        matches!(error, RunError::NoFreshCopy { .. }),
        "the refusal has to be the one that means \"no copy\", not a bare io error. \
         RunError::Io is transparent, so the human reads \"Permission denied (os error 13)\" \
         — what failed for the SYSTEM, not what failed for them. Got: {error:?}"
    );

    let said = error.to_string();
    for expected in ["Scribe at work", "own copy of your files", "Nothing ran"] {
        assert!(
            said.contains(expected),
            "the refusal has to carry {expected:?}: the step by the name on its tile, what it \
             was promised, and that nothing started. It said: {said}"
        );
    }

    Ok(())
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers(started: Arc<Mutex<Vec<String>>>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { started });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler, który ZAPISUJE fakt swojego uruchomienia i nic poza tym.
///
/// Cała treść tego testu to zdanie „ani jeden krok nie ruszył", więc dubler ma tylko jedno
/// zadanie: zostawić ślad, jeśli mimo wszystko go zawołano.
#[derive(Debug)]
struct Fake {
    started: Arc<Mutex<Vec<String>>>,
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
        self.started
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(spec.prompt.clone());

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
