//! AC-1 dla T-95: po biegu praca stoi NA GAŁĘZI, a katalog, w którym powstała, znika.
//!
//! # Co to mierzy
//!
//! `isolate::finish` robił do dziś połowę roboty. Krok, który nic nie zmienił, tracił katalog
//! i gałąź; krok, który zmienił cokolwiek, dostawał commit — i katalog **zostawał na dysku**,
//! z pełnym checkoutem repozytorium w środku. Zmierzone u właściciela 2026-08-23: dziesięć
//! biegów na jednym monorepo zostawiło kilkadziesiąt katalogów `work/s_*`, każdy z osobną kopią
//! całego drzewa, dla zadania, które tego repozytorium nawet nie dotykało.
//!
//! Obietnica z T-52 brzmi: praca jest po biegu **osiągalna z gita**. Gałąź ją spełnia w całości.
//! Katalog nie dokłada do niej nic poza miejscem na dysku i wpisem na liście, której nikt nie
//! sprząta.
//!
//! # SŁABĄ WERSJĄ jest `assert!(!folder.exists())`
//!
//! Przechodzi ją `fs::remove_dir_all`, który kasuje katalog i **zostawia wpis** w rejestrze
//! gita — a taki wpis blokuje potem założenie drzewa pod tą samą ścieżką i psuje ponowne
//! odpalenie kroku. Rozstrzyga druga asercja: nazwy kroku nie ma też na liście drzew.
//!
//! Druga słaba wersja: skasowanie katalogu razem z pracą. Przechodzi obie asercje wyżej
//! i traci wszystko, po co ten produkt istnieje. Rozstrzyga ją `git diff` przez gałąź —
//! plik, którego krok nie zacommitował, nie jest w niej widoczny.
//!
//! Trzecia słaba wersja: sprzątanie ZAWSZE, także wtedy, gdy commit się nie udał. Wtedy jedyna
//! operacja w tym module, która umie stracić czyjąś pracę, właśnie ją traci. Rozstrzyga
//! przypadek (c): z zablokowanym indeksem gita katalog ma zostać, a bieg ma o tym napisać
//! jedno zdanie w swoim opisie.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::isolate;
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
use serde_json::Value;
use tempfile::TempDir;
use tokio::sync::mpsc;

const VENDOR: &str = "claude-code";
const PATIENCE: Duration = Duration::from_secs(20);

/// Katalog, pod którym bieg zakłada katalogi robocze kroków.
const WORK: &str = "work";
/// Opis biegu na dysku.
const RUN_FILE: &str = "run.json";

/// Plik, który pisze pierwszy krok — i tylko on.
const MADE: &str = "the-work.txt";
const MADE_TEXT: &str = "this is what the agent produced";

/// Klucze kafelków, po których poznajemy kroki w rejestrze dublera.
const WRITES: &str = "s_writes";
const IDLES: &str = "s_idles";

/// Zdania z zadań kroków. Dubler po nich rozpoznaje, który krok właśnie dostał.
const WRITE_TASK: &str = "write the file";
const IDLE_TASK: &str = "touch nothing";

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_no_leftovers",
  "name": "One writes, one does not",
  "steps": [
    {
      "kind": "agent",
      "id": "s_writes",
      "name": "Writes",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "write the file",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_idles",
      "name": "Idles",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "touch nothing",
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

// ── (a) praca ląduje na gałęzi, a katalog roboczy znika ────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_work_is_on_the_branch_and_the_folder_it_was_made_in_is_gone()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let base = bench.head()?;
    let store = Store::open(&bench.db())?;

    let seen = Arc::new(Seen::default());
    let report = bench.run(&store, Arc::clone(&seen), Jam::No, None).await?;

    // Gałąź niesie pracę — także plik, którego nie było w projekcie.
    let branch = isolate::branch_for(&report.id, WRITES);
    let changed = bench.git(&["diff", "--name-only", &format!("{base}..{branch}")])?;
    assert!(
        changed.contains(MADE),
        "`git diff {base}..{branch}` does not mention {MADE}. Tidying the folder away is only \
         allowed because the branch already carries everything that was in it — a branch \
         without the work turns this cleanup into the one operation in this module that loses \
         somebody's day. It listed: {changed}"
    );

    // A katalog, w którym ta praca powstała, już go nie ma.
    let folder = report.dir.join(WORK).join(WRITES);
    assert!(
        !folder.exists(),
        "the folder the step worked in is still on disk at {folder:?}. It holds a full checkout \
         of the repository and adds nothing the branch does not already have: ten runs on one \
         monorepo left tens of these behind, for a task that never touched that repository"
    );

    // I nie ma go też na liście, którą prowadzi git. Skasowanie samego katalogu zostawia tam
    // wpis, a taki wpis odmawia potem założenia drzewa pod tą samą ścieżką.
    let trees = bench.git(&["worktree", "list"])?;
    assert!(
        !trees.contains(WRITES),
        "the step is still registered with git as a place to work, so deleting the folder was \
         not enough: the entry stays, nothing lists it, and the next run under the same path is \
         refused. The list says: {trees}"
    );

    // Krok, który nic nie zmienił, zostawia dokładnie tyle, co dotąd: nic.
    let branches = bench.branches()?;
    assert!(
        !branches.iter().any(|name| name.contains(IDLES)),
        "the step that changed nothing left a branch behind, which is the state this module has \
         been avoiding since T-52. Branches: {branches:?}"
    );
    assert!(
        !report.dir.join(WORK).join(IDLES).exists(),
        "the step that changed nothing left its folder behind"
    );

    // Kontrola przeciw pustemu biegowi: obie asercje wyżej byłyby prawdziwe, gdyby żaden krok
    // nie ruszył.
    let looked = seen.snapshot();
    assert_eq!(
        looked.len(),
        2,
        "both steps have to reach the driver, or the assertions above are talking about a run \
         that never happened. Saw: {:?}",
        looked.keys().collect::<Vec<_>>()
    );

    Ok(())
}

// ── (b) po sprzątaniu krok wznowiony DALEJ widzi swoją pracę ───────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_step_picked_up_after_the_cleanup_still_starts_from_its_own_work()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let store = Store::open(&bench.db())?;

    let first = bench
        .run(&store, Arc::new(Seen::default()), Jam::No, None)
        .await?;

    // Punkt wyjścia dla tego przypadku: katalog naprawdę zniknął. Bez tej asercji drugi bieg
    // mógłby zobaczyć pracę z KATALOGU, a wtedy to kryterium nie mówi nic o wznowieniu.
    assert!(
        !first.dir.join(WORK).join(WRITES).exists(),
        "the first run kept its folder, so this case cannot say anything about picking a step \
         up AFTER the cleanup"
    );

    // Drugi bieg wznawia z opisu pierwszego. Punkt startu bierze się Z GAŁĘZI, więc sprzątnięty
    // katalog nie ma prawa niczego zabrać.
    let seen = Arc::new(Seen::default());
    let second = bench
        .run(&store, Arc::clone(&seen), Jam::No, Some(first.dir.clone()))
        .await?;
    assert_ne!(
        second.dir, first.dir,
        "picking a step up is a NEW run with its own folder; the fixture is wrong if both runs \
         share one"
    );

    let looked = seen.snapshot();
    let writes = looked
        .get(WRITES)
        .ok_or("the writing step never reached the driver in the second run")?;
    assert_eq!(
        writes.found.as_deref(),
        Some(MADE_TEXT),
        "the step picked up after the cleanup opened a folder without {MADE} in it. The work of \
         the previous run is reachable from git and nothing else was needed to hand it over — \
         if this is empty, the step starts rewriting from scratch what it already finished, \
         which is the defect this cleanup must never introduce. It found: {:?}",
        writes.found
    );

    Ok(())
}

// ── (c) commit, który się nie udał, NIE kosztuje pracy ─────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn work_that_could_not_be_saved_keeps_its_folder_and_the_run_says_so()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let store = Store::open(&bench.db())?;

    // Dubler blokuje indeks gita w swoim katalogu, więc zapis pracy na gałąź MUSI się nie udać.
    let report = bench
        .run(&store, Arc::new(Seen::default()), Jam::Yes, None)
        .await?;

    let folder = report.dir.join(WORK).join(WRITES);
    assert!(
        folder.join(MADE).is_file(),
        "the work could not be saved to a branch and the folder holding it was removed anyway, \
         so {MADE} is gone from both places. This is the one operation here that can lose \
         somebody's day, and it just did"
    );

    // I bieg mówi o tym jednym zdaniem, w swoim opisie, przy TYM kroku. Bez niego człowiek widzi
    // udany bieg i nie ma skąd wiedzieć, że jego praca leży poza gitem.
    let said = step_said(&report.dir, WRITES)?;
    assert!(
        !said.is_empty(),
        "the run's record says nothing about the step whose work never made it to a branch. A \
         green run over work that sits outside git is a silent loss: nobody looks for what \
         nothing mentioned"
    );
    assert!(
        said.contains(&folder.display().to_string()),
        "the sentence does not say WHERE the work was left, so the person is told something \
         went wrong and then has to go looking. It said: {said}"
    );

    Ok(())
}

// ── odczyt opisu biegu ─────────────────────────────────────────────────────────────────────

/// Zdanie, które bieg zapisał przy kroku o tym kluczu.
fn step_said(run_dir: &Path, node_key: &str) -> Result<String, Box<dyn Error>> {
    let text = fs::read_to_string(run_dir.join(RUN_FILE))?;
    let described: Value = serde_json::from_str(&text)?;
    let steps = described
        .get("steps")
        .and_then(Value::as_array)
        .ok_or("the run's record has no steps in it")?;
    let step = steps
        .iter()
        .find(|one| one.get("node_key").and_then(Value::as_str) == Some(node_key))
        .ok_or("the run's record does not mention that step")?;
    Ok(step
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned())
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

/// Czy dubler ma zablokować indeks gita w swoim katalogu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Jam {
    No,
    Yes,
}

/// Co jeden krok zastał w swoim katalogu.
#[derive(Debug, Clone)]
struct Look {
    /// Treść [`MADE`], jeśli plik już tam był.
    found: Option<String>,
}

/// Rejestr tego, co kroki zobaczyły — po kluczu kafelka.
#[derive(Debug, Default)]
struct Seen(Mutex<BTreeMap<String, Look>>);

impl Seen {
    fn note(&self, key: &str, look: Look) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key.to_owned(), look);
    }

    fn snapshot(&self) -> BTreeMap<String, Look> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

fn fake_drivers(seen: Arc<Seen>, jam: Jam) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { seen, jam });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct Fake {
    seen: Arc<Seen>,
    jam: Jam,
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
        // Krok rozpoznajemy po jego zadaniu: tylko jeden z dwóch ma cokolwiek napisać.
        let writing = spec.prompt.contains(WRITE_TASK);
        let key = if writing { WRITES } else { IDLES };
        if !writing && !spec.prompt.contains(IDLE_TASK) {
            anyhow::bail!(
                "this run handed the driver a task it does not know: {}",
                spec.prompt
            );
        }

        // ODCZYT PRZED ZAPISEM, bo o to pyta przypadek (b): krok wznowiony ma ZASTAĆ pracę
        // poprzedniego biegu, a nie tę, którą sam za chwilę położy.
        self.seen.note(
            key,
            Look {
                found: fs::read_to_string(spec.cwd.join(MADE)).ok(),
            },
        );

        if writing {
            fs::write(spec.cwd.join(MADE), MADE_TEXT)?;
            if self.jam == Jam::Yes {
                jam_the_index(&spec.cwd)?;
            }
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

/// Zabiera gitowi prawo zapisu do indeksu tego katalogu — tak, jak robi to przerwane `git
/// commit`, po którym plik blokady zostaje.
///
/// To jest jedyny znany nam sposób, żeby zapis pracy na gałąź nie udał się DETERMINISTYCZNIE,
/// a jednocześnie żeby katalog dalej wyglądał na zmieniony. Bez niego przypadek (c) mierzyłby
/// ścieżkę, której nie da się wywołać.
fn jam_the_index(cwd: &Path) -> anyhow::Result<()> {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["rev-parse", "--absolute-git-dir"])
        .output()?;
    if !out.status.success() {
        anyhow::bail!(
            "the step's folder is not a git work area: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let git_dir = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim().to_owned());
    fs::write(git_dir.join("index.lock"), "held by this test\n")?;
    Ok(())
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
        let bench = Self { home, project };
        fs::write(bench.project.path().join("notes.txt"), "the human's file")?;
        bench.make_a_repo()?;
        Ok(bench)
    }

    /// Jeden bieg tego workflow, z podanym dublerem i ewentualnym poprzednikiem.
    async fn run(
        &self,
        store: &Store,
        seen: Arc<Seen>,
        jam: Jam,
        after: Option<PathBuf>,
    ) -> Result<loadout_lib::commands::RunReport, Box<dyn Error>> {
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store,
            drivers: fake_drivers(seen, jam),
            processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow: self.workflow("no-leftovers", WORKFLOW)?,
            how_many_at_once: 2,
            task: None,
            part: None,
            handoffs_from: after,
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
            "both steps have to finish, or nothing below means anything; they ended as {:?}",
            report.steps
        );
        Ok(report)
    }

    fn make_a_repo(&self) -> Result<(), Box<dyn Error>> {
        self.git(&["init", "--quiet"])?;
        fs::write(self.project.path().join(".gitignore"), ".loadout/\n")?;
        self.git(&["add", "-A"])?;
        self.git(&["commit", "--quiet", "-m", "the human's first commit"])?;
        Ok(())
    }

    fn head(&self) -> Result<String, Box<dyn Error>> {
        Ok(self.git(&["rev-parse", "HEAD"])?.trim().to_owned())
    }

    /// Nazwy gałęzi, po jednej w wierszu.
    fn branches(&self) -> Result<Vec<String>, Box<dyn Error>> {
        Ok(self
            .git(&["branch", "--format=%(refname:short)"])?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect())
    }

    fn git(&self, args: &[&str]) -> Result<String, Box<dyn Error>> {
        git(self.project.path(), args)
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
