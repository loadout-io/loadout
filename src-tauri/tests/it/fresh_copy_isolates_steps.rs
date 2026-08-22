//! AC-1 dla T-33: krok w trybie „własna kopia twoich plików" WIDZI pliki projektu, a to, co
//! zmieni, nie wychodzi poza jego kopię.
//!
//! `docs/ARCHITECTURE.md` §2 pyt. 4 obiecuje: „każdy krok dostaje własną kopię twoich plików".
//! Do 2026-08-17 `fresh-copy` tworzył **pusty katalog tymczasowy** — bez kopii, bez `git
//! worktree`, bez degradacji z ostrzeżeniem. To nie była brakująca wygoda: `workflow::check`
//! odmawia zapisu workflow, w którym dwa kroki piszą po tych samych ścieżkach (T-12), i ta
//! walidacja ZAKŁADA, że fresh-copy chroni. Nie chroniła, więc krok pracował na pustce zamiast
//! na projekcie — co jest gorsze od kolizji: agent nie widzi plików, które ma zmienić.
//!
//! **Słaba wersja tego kryterium:** sprawdzenie, że katalogi robocze obu kroków są RÓŻNE.
//! Przechodziła przed poprawką, bo dwa puste katalogi też są różne. Rozróżnia dopiero
//! **obecność plików projektu** w obu i brak przeciekania między nimi.
//!
//! Sterownik jest tu dublerem, który CZYTA i PISZE w `spec.cwd`. Dubler, który tylko oddaje
//! zdarzenia, przeszedłby ten test na implementacji kopiującej zero plików: żeby asercja mówiła
//! o izolacji, ktoś naprawdę musi tknąć dysk.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
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

/// Etykieta vendora dublera.
const VENDOR: &str = "claude-code";

/// Plik, który pierwszy krok ZMIENIA.
const EXISTING: &str = "notes.txt";
/// Treść, którą oba kroki mają zastać.
const ORIGINAL: &str = "written by the human";
/// Plik, który pierwszy krok TWORZY.
const CREATED: &str = "made-by-step-one.txt";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony.
const PATIENCE: Duration = Duration::from_secs(20);

/// Dwa kroki BEZ strzałki między nimi, oba na własnej kopii.
///
/// Bez strzałki, bo izolacja ma działać także wtedy, gdy kroki idą naraz — a przy limicie dwóch
/// naraz to jest właśnie ten przypadek. Strzałka schowałaby wyścig, który ten test bada.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_fresh_copy",
  "name": "Two steps, each on its own copy",
  "steps": [
    {
      "kind": "agent",
      "id": "s_one",
      "name": "First",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "change and create",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_two",
      "name": "Second",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "look only",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn each_step_sees_the_project_and_keeps_its_changes_to_itself() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;

    // Projekt z dwoma plikami. `EXISTING` ma treść, którą oba kroki mają ZASTAĆ.
    fs::write(bench.project.path().join(EXISTING), ORIGINAL)?;
    fs::create_dir_all(bench.project.path().join("src"))?;
    fs::write(
        bench.project.path().join("src").join("main.rs"),
        "fn main() {}",
    )?;

    let seen = Arc::new(Seen::default());
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&seen)),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: bench.workflow("fresh-copy", WORKFLOW)?,
        how_many_at_once: 2,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let recorder = Delivered::default();
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, recorder.channel());
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| "the run never came back")??;
    let _ = tokio::time::timeout(PATIENCE, pump).await;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded, StepState::Succeeded],
        "both steps have to finish for the file assertions to mean anything; they ended as {:?}",
        report.steps
    );

    let looked = seen.snapshot();
    assert_eq!(
        looked.len(),
        2,
        "both steps have to reach the driver, or this test is measuring one step twice. Saw: {:?}",
        looked.keys().collect::<Vec<_>>()
    );

    // (a) OBA kroki zastały pliki projektu. To jest ta asercja, której nie przechodzi pusty
    //     katalog — i cały powód, dla którego ten test istnieje.
    for (step, look) in &looked {
        assert_eq!(
            look.existing.as_deref(),
            Some(ORIGINAL),
            "step {step} was set to work on its own COPY of the project, so it has to find \
             {EXISTING} with the human's text in it. It found: {:?}",
            look.existing
        );
        assert!(
            look.nested,
            "step {step} did not see src/main.rs, so the copy is shallow: a project is a tree, \
             not a list of files in one folder"
        );
    }

    // (b) Zmiana pierwszego kroku NIE JEST widoczna dla drugiego.
    let second = looked
        .get("s_two")
        .ok_or("the second step never reached the driver")?;
    assert_eq!(
        second.existing.as_deref(),
        Some(ORIGINAL),
        "the second step read {EXISTING} AFTER the first one rewrote its own copy. Seeing the \
         first step's text here means both steps share one folder, which is exactly what \
         workflow::check refuses at save time (invariant 12)"
    );
    assert!(
        !second.created,
        "the second step found {CREATED}, a file the FIRST step made. Their copies are not \
         separate, so two steps without an arrow would overwrite each other's work"
    );

    // (c) Katalog ORYGINALNY jest nietknięty.
    assert_eq!(
        fs::read_to_string(bench.project.path().join(EXISTING))?,
        ORIGINAL,
        "the project file changed. A step on its own copy must not reach back into the folder \
         the human is working in"
    );
    assert!(
        !bench.project.path().join(CREATED).exists(),
        "{CREATED} appeared in the project folder. The step wrote into its own copy, so this \
         file can only be here if the copy was the project itself"
    );

    Ok(())
}

// ── co dubler zobaczył ─────────────────────────────────────────────────────────────────────

/// Co jeden krok zastał w swoim katalogu roboczym.
#[derive(Debug, Default, Clone)]
struct Look {
    /// Treść `EXISTING`, jeśli plik tam był.
    existing: Option<String>,
    /// Czy `src/main.rs` też dojechał — kopia płaska nie jest kopią projektu.
    nested: bool,
    /// Czy krok zastał plik zrobiony przez INNY krok.
    created: bool,
}

/// Co zobaczył każdy krok, po jego identyfikatorze.
#[derive(Debug, Default)]
struct Seen(Mutex<BTreeMap<String, Look>>);

impl Seen {
    fn record(&self, step: &str, look: Look) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(step.to_owned(), look);
    }

    fn snapshot(&self) -> BTreeMap<String, Look> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// Odczyt katalogu roboczego, zrobiony w chwili wejścia kroku do sterownika.
fn look_at(cwd: &Path) -> Look {
    Look {
        existing: fs::read_to_string(cwd.join(EXISTING)).ok(),
        nested: cwd.join("src").join("main.rs").exists(),
        created: cwd.join(CREATED).exists(),
    }
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers(seen: Arc<Seen>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { seen });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler, który NAPRAWDĘ czyta i pisze w `spec.cwd`.
///
/// Bez pisania ten test przechodziłby na implementacji, która kopiuje pliki, ale daje obu krokom
/// ten sam katalog: nikt by wtedy niczego nie nadpisał, więc izolacja wyglądałaby na działającą.
#[derive(Debug)]
struct Fake {
    seen: Arc<Seen>,
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
        // Krok rozpoznajemy po treści zadania: `RunSpec` nie niesie identyfikatora kroku,
        // a prompt jest tym, co ten krok naprawdę dostał.
        let step = if spec.prompt.starts_with("change") {
            "s_one"
        } else {
            "s_two"
        };
        self.seen.record(step, look_at(&spec.cwd));

        if step == "s_one" {
            // Zmiana i utworzenie — obie w SWOIM katalogu.
            fs::write(spec.cwd.join(EXISTING), "rewritten by the first step")?;
            fs::write(spec.cwd.join(CREATED), "made here")?;
        }

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
