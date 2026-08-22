//! AC-1 dla T-52: krok w trybie „własna kopia" pracuje we WŁASNYM DRZEWIE GITA, a projekt
//! tego nie widzi.
//!
//! T-33 dało temu trybowi kopiowanie plik po pliku. Zmierzone 2026-08-19 na `~/Projects/
//! meetnotes`: bieg odmówił na `.claude/worktrees/murmur-server`, czyli na dowiązaniu do
//! katalogu, po przepisaniu 13 MB i przed uruchomieniem czegokolwiek. Naprawa jednego kształtu
//! niczego nie rozstrzyga — `pnpm`, `python -m venv` i `git worktree` robią takie wpisy same.
//! Izolację oddajemy więc gitowi.
//!
//! **Słaba wersja tego kryterium:** sprawdzenie, że katalogi robocze obu kroków są RÓŻNE. Dwa
//! puste katalogi też są różne. Rozróżnia dopiero obecność plików projektu w obu, brak
//! przeciekania między nimi i to, że katalog kroku JEST drzewem gita.

use std::collections::BTreeMap;
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
const EXISTING: &str = "notes.txt";
const ORIGINAL: &str = "written by the human";
const CREATED: &str = "made-by-step-one.txt";
const PATIENCE: Duration = Duration::from_secs(20);

/// Dwa kroki BEZ strzałki, oba na własnym drzewie: izolacja ma działać także wtedy, gdy idą naraz.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_worktree_isolation",
  "name": "Two steps, each on its own tree",
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
async fn each_step_works_in_its_own_git_tree() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    fs::write(bench.project.path().join(EXISTING), ORIGINAL)?;
    fs::create_dir_all(bench.project.path().join("src"))?;
    fs::write(
        bench.project.path().join("src").join("main.rs"),
        "fn main() {}",
    )?;
    bench.make_a_repo()?;
    let before = bench.status()?;

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
        workflow: bench.workflow("worktree-isolation", WORKFLOW)?,
        how_many_at_once: 2,
        task: None,
        only: None,
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
    // (e) kontrola przeciw pustemu czytaniu.
    assert_eq!(
        looked.len(),
        2,
        "both steps have to reach the driver, or this test measures one step twice. Saw: {:?}",
        looked.keys().collect::<Vec<_>>()
    );

    for (step, look) in &looked {
        // (a) Pliki projektu są na miejscu w OBU drzewach.
        assert_eq!(
            look.existing.as_deref(),
            Some(ORIGINAL),
            "step {step} works on its own tree, so it has to find {EXISTING} with the human's \
             text in it. It found: {:?}",
            look.existing
        );
        assert!(
            look.nested,
            "step {step} did not see src/main.rs, so the tree is shallow: a project is a tree, \
             not a list of files in one folder"
        );
        // (d) I to naprawdę jest drzewo gita, nie kopia bajtów.
        assert!(
            look.a_git_tree,
            "step {step} did not get a git work tree. That is the whole point of this change: \
             a copy strands the work where nobody takes it from, while a tree has a branch"
        );
    }

    // (b) Zmiana pierwszego kroku nie jest widoczna dla drugiego.
    let second = looked
        .get("s_two")
        .ok_or("the second step never reached the driver")?;
    assert_eq!(
        second.existing.as_deref(),
        Some(ORIGINAL),
        "the second step read {EXISTING} AFTER the first one rewrote its own tree. Seeing the \
         first step's text here means both steps share one folder, which is exactly what \
         workflow::check refuses at save time (invariant 12)"
    );
    assert!(
        !second.created,
        "the second step found {CREATED}, a file the FIRST step made. Their trees are not \
         separate, so two steps without an arrow would overwrite each other's work"
    );

    // (c) Katalog oryginalny jest nietknięty — mierzone gitem, nie na oko.
    assert_eq!(
        fs::read_to_string(bench.project.path().join(EXISTING))?,
        ORIGINAL,
        "the project file changed. A step on its own tree must not reach back into the folder \
         the human is working in"
    );
    assert_eq!(
        bench.status()?,
        before,
        "git status in the project folder is not what it was before the run. Whatever the step \
         did, it did it in the human's checkout"
    );

    Ok(())
}

// ── co dubler zobaczył ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
struct Look {
    existing: Option<String>,
    nested: bool,
    created: bool,
    a_git_tree: bool,
}

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

fn look_at(cwd: &Path) -> Look {
    Look {
        existing: fs::read_to_string(cwd.join(EXISTING)).ok(),
        nested: cwd.join("src").join("main.rs").exists(),
        created: cwd.join(CREATED).exists(),
        a_git_tree: its_own_work_tree(cwd),
    }
}

/// Czy `cwd` jest KORZENIEM własnego drzewa roboczego.
///
/// `--is-inside-work-tree` tu nie wystarcza i to nie jest drobiazg: katalog biegu leży pod
/// `<projekt>/.loadout/runs/…`, czyli WEWNĄTRZ repozytorium człowieka, więc zwykła kopia plików
/// odpowiada na tamto pytanie „true" — mówiąc prawdę o cudzym drzewie. Rozstrzyga dopiero
/// `--show-toplevel`: własne drzewo ma korzeń w sobie.
fn its_own_work_tree(cwd: &Path) -> bool {
    let out = Command::new("git")
        .args([
            "-C",
            &cwd.display().to_string(),
            "rev-parse",
            "--show-toplevel",
        ])
        .output();
    let Ok(out) = out else { return false };
    if !out.status.success() {
        return false;
    }
    let top = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    match (fs::canonicalize(top), fs::canonicalize(cwd)) {
        (Ok(top), Ok(here)) => top == here,
        _ => false,
    }
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers(seen: Arc<Seen>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { seen });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

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
        let step = if spec.prompt.starts_with("change") {
            "s_one"
        } else {
            "s_two"
        };
        self.seen.record(step, look_at(&spec.cwd));

        if step == "s_one" {
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

    /// Repozytorium z jednym commitem — bez niego nie ma z czego założyć drzewa.
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

    /// `git status --porcelain` w projekcie: jedno zdanie o tym, czy ktoś tam sięgnął.
    fn status(&self) -> Result<String, Box<dyn Error>> {
        git(self.project.path(), &["status", "--porcelain"])
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

/// Git z tożsamością podaną na miejscu: maszyna, na której nikt jej nie ustawił, ma przejść
/// ten test tak samo jak Twoja.
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
