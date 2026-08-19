//! AC-2 dla T-52: niescommitowana praca jedzie do drzewa kroku, a nieśledzona jest NAZWANA.
//!
//! Drzewo zakładane z `HEAD` pokazuje agentowi stan sprzed Twoich zmian. Napisze wtedy kod
//! przeciwko plikowi, którego już nie ma — a konflikt zobaczysz dopiero przy scalaniu. Więc
//! różnica plików ŚLEDZONYCH jedzie z Tobą.
//!
//! Plików nieśledzonych git nie zna, więc do drzewa nie wchodzą. To jest strata i dlatego ma
//! być POWIEDZIANA: cicha jest gorsza niż brak funkcji, bo wygląda dokładnie tak samo jak
//! kompletna kopia.
//!
//! **Słaba wersja tego kryterium:** sprawdzenie, że plik istnieje. Plik z commita też istnieje
//! i różni się jedną linią. Rozróżnia dopiero TREŚĆ.

use std::error::Error;
use std::fs;
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

/// Plik śledzony: w commicie ma jedną treść, w katalogu człowieka drugą.
const TRACKED: &str = "notes.txt";
const COMMITTED: &str = "what the commit says";
const UNCOMMITTED: &str = "what the human is writing right now";
/// Plik, o którym git nie wie.
const UNTRACKED: &str = "scratch.txt";

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_carry_uncommitted",
  "name": "One step on its own tree",
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_step_sees_what_the_human_sees_and_hears_what_it_did_not_get()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;

    // Commit z jedną treścią…
    fs::write(bench.project.path().join(TRACKED), COMMITTED)?;
    bench.make_a_repo()?;
    // …a potem praca, której nikt nie zacommitował: zmiana śledzonego i nowy, nieśledzony.
    fs::write(bench.project.path().join(TRACKED), UNCOMMITTED)?;
    fs::write(bench.project.path().join(UNTRACKED), "notes to self")?;

    let seen = Arc::new(Mutex::new(None::<Look>));
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&seen)),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: bench.workflow("carry-uncommitted", WORKFLOW)?,
        how_many_at_once: 1,
        task: None,
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
        vec![StepState::Succeeded],
        "the step has to finish for the rest of this test to mean anything; it ended as {:?}",
        report.steps
    );

    let look = seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .ok_or("the step never reached the driver, so nothing was measured")?;

    // (a) Widzi TREŚĆ, którą ma człowiek — nie tę z commita.
    assert_eq!(
        look.tracked.as_deref(),
        Some(UNCOMMITTED),
        "the step read {TRACKED} as {:?}. A tree made from HEAD alone shows the agent the file \
         as it was before the human's edits, so it writes against a version that no longer \
         exists and the conflict shows up at merge time",
        look.tracked
    );

    // (b) Pliku nieśledzonego w drzewie nie ma — git go nie zna.
    assert!(
        !look.untracked,
        "{UNTRACKED} is in the step's tree. Git does not know this file, so a git-made tree \
         cannot honestly contain it; if it does, something copied it in behind git's back and \
         the tree is no longer what `git status` in it says it is"
    );

    // (c) …i bieg to POWIEDZIAŁ. Cicha strata wygląda jak kompletna kopia.
    let said = recorder.text();
    assert!(
        said.contains(UNTRACKED),
        "no line of the run names {UNTRACKED} as left behind. A file that silently does not \
         reach the agent is the worst shape of this feature: the run looks complete and the \
         agent is missing something the human can see on their screen. The run said: {said}"
    );

    // (d) Katalog człowieka ma dalej OBIE swoje rzeczy — czytanie różnicy niczego nie zabiera.
    assert_eq!(
        fs::read_to_string(bench.project.path().join(TRACKED))?,
        UNCOMMITTED,
        "the human's uncommitted edit is gone from their own folder. Carrying a diff means \
         reading it, never moving it"
    );
    assert!(
        bench.project.path().join(UNTRACKED).exists(),
        "the untracked file vanished from the human's folder"
    );

    Ok(())
}

// ── co dubler zobaczył ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
struct Look {
    tracked: Option<String>,
    untracked: bool,
}

fn look_at(cwd: &Path) -> Look {
    Look {
        tracked: fs::read_to_string(cwd.join(TRACKED)).ok(),
        untracked: cwd.join(UNTRACKED).exists(),
    }
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers(seen: Arc<Mutex<Option<Look>>>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { seen });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct Fake {
    seen: Arc<Mutex<Option<Look>>>,
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
        *self.seen.lock().unwrap_or_else(PoisonError::into_inner) = Some(look_at(&spec.cwd));

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

    fn make_a_repo(&self) -> Result<(), Box<dyn Error>> {
        git(self.project.path(), &["init", "--quiet"])?;
        fs::write(self.project.path().join(".gitignore"), ".loadout/\n")?;
        git(self.project.path(), &["add", "-A"])?;
        git(
            self.project.path(),
            &["commit", "--quiet", "-m", "the human's first commit"],
        )?;
        Ok(())
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

fn git(at: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(at)
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

/// Paczki, które wyszły kanałem — tu potrzebne, bo (c) pyta, co bieg POWIEDZIAŁ.
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

    /// Wszystko, co bieg powiedział, jednym tekstem.
    fn text(&self) -> String {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    }
}
