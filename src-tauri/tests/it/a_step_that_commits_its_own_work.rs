//! Z-7: krok, który sam zacommitował swoją pracę, ZOSTAJE ze swoją gałęzią.
//!
//! # Co to mierzy
//!
//! `isolate::finish` pytał do dziś jednym pytaniem — `git status --porcelain` — i pustą
//! odpowiedź czytał jako „ten krok nic nie zmienił": zdejmował katalog i robił `branch -D`.
//! Agent, który commituje swoją pracę sam, a to jest normalny tryb pracy implementera, zostawia
//! drzewo dokładnie takie: czyste, bo wszystko już zapisał. Kasowaliśmy mu wtedy gałąź razem
//! z commitami, czyli jedyną kopię jego pracy — po `branch -D` nie widać jej ani w `git log`,
//! ani w `git branch`, i nie sięga do niej nic poza `git fsck`.
//!
//! Obok, w tym samym module, `touched()` zadaje od 2026-08-22 właściwe pytanie: status LUB
//! commity ponad punktem startu. Tu chodzi o to, żeby zamknięcie biegu pytało tak samo.
//!
//! # SŁABĄ WERSJĄ jest samo `assert!(branch_exists)`
//!
//! Przechodzi ją „nigdy nie kasuj gałęzi", czyli obejście, które przewraca obietnicę T-52:
//! po tygodniu biegów `git branch` przestaje być do przeczytania. Rozstrzyga drugi kafelek,
//! który naprawdę nic nie robi i dalej nie ma prawa zostawić ani gałęzi, ani katalogu.
//!
//! Druga słaba wersja: zostawić gałąź i dołożyć na jej wierzchu własny commit „Tytuł: Krok".
//! Praca byłaby wtedy osiągalna, ale historia kroku kłamałaby o tym, kto ją zapisał, a pusty
//! commit stałby na każdej takiej gałęzi. Rozstrzyga `git log --format=%s`: temat ma być JEDEN
//! i ma być tym, który napisał agent.
//!
//! Trzecia: zostawić gałąź i nie pokazać jej nigdzie. Człowiek widzi swoją pracę wyłącznie
//! w opisie otwartego biegu (niezmiennik 29), więc pyta o nią ta sama komenda, którą woła okno.

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::isolate;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
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
/// Katalog, w którym bieg trzyma zapis o tym, z czego powstało drzewo każdego kroku.
const MARKERS: &str = ".isolation";
/// Opis biegu na dysku.
const RUN_FILE: &str = "run.json";

/// Plik, który pisze pierwszy krok — i tylko on.
const MADE: &str = "the-work.txt";
const MADE_TEXT: &str = "this is what the agent produced";
/// Temat commita, który agent robi WŁASNĄ RĘKĄ. Po nim poznajemy, czyja to praca.
const AGENT_SAID: &str = "the agent saved this itself";

/// Klucze kafelków, po których poznajemy kroki w rejestrze dublera.
const COMMITS: &str = "s_commits";
const IDLES: &str = "s_idles";
/// Nazwa kafelka, który commituje — tę widzi człowiek przy gałęzi.
const COMMITS_NAME: &str = "Commits";

/// Zdania z zadań kroków. Dubler po nich rozpoznaje, który krok właśnie dostał.
const COMMIT_TASK: &str = "save the work yourself";
const IDLE_TASK: &str = "touch nothing";

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_commits_its_own_work",
  "name": "One saves its own work, one does not",
  "steps": [
    {
      "kind": "agent",
      "id": "s_commits",
      "name": "Commits",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "save the work yourself",
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

// ── (a) praca zacommitowana ręką agenta zostaje na gałęzi, a bieg mówi gdzie ───────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_step_that_commits_its_own_work_keeps_its_branch_and_the_run_says_where()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let base = bench.head()?;
    let store = Store::open(&bench.db())?;

    let seen = Arc::new(Seen::default());
    let report = bench
        .run(&store, Arc::clone(&seen), Habit::Nothing, None)
        .await?;

    // Gałąź istnieje. Bez niej praca agenta jest nieosiągalna z gita w ogóle.
    let branch = isolate::branch_for(&report.id, COMMITS);
    let branches = bench.branches()?;
    assert!(
        branches.iter().any(|name| name == &branch),
        "the branch {branch} is gone, so the commit the agent made itself is reachable from \
         nothing: not from `git log`, not from `git branch`, only from `git fsck`. This step \
         left a clean folder because it had already saved everything, and a clean folder was \
         read as \"changed nothing\". Branches: {branches:?}"
    );

    // I niesie DOKŁADNIE ten commit, który zrobił agent. Ani mniej — praca jest osiągalna —
    // ani więcej: własny commit Loadouta na wierzchu kłamałby o tym, kto ją zapisał.
    let subjects = bench.git(&["log", "--format=%s", &format!("{base}..{branch}")])?;
    let subjects: Vec<&str> = subjects
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(
        subjects,
        vec![AGENT_SAID],
        "`git log {base}..{branch}` does not say what the agent said, and only that"
    );
    let saved = bench.git(&["show", &format!("{branch}:{MADE}")])?;
    assert_eq!(
        saved.trim(),
        MADE_TEXT,
        "the branch is there and the work is not in it"
    );

    // A człowiek widzi tę gałąź tam, gdzie w ogóle o niej czyta: w opisie otwartego biegu.
    let past = read_run_inner(bench.project.path(), run_folder(&report)?)?;
    let named = past
        .branches
        .iter()
        .find(|one| one.name == branch)
        .ok_or_else(|| {
            format!(
                "the open run does not name {branch} among the branches it left, so the only \
                 place a person is told where their work went says nothing about it. It named: \
                 {:?}",
                past.branches
            )
        })?;
    assert_eq!(
        named.step, COMMITS_NAME,
        "the branch is named after nothing a person can recognise; the key from the file is not \
         on any screen (invariant 14)"
    );

    the_rest_is_tidy(&bench, &report)?;

    // Kontrola przeciw pustemu biegowi: asercje wyżej byłyby prawdziwe, gdyby żaden krok nie
    // ruszył.
    let looked = seen.snapshot();
    assert_eq!(
        looked.len(),
        2,
        "both steps have to reach the driver, or the assertions above are talking about a run \
         that never happened. Saw: {looked:?}"
    );

    Ok(())
}

/// Druga połowa przypadku (a): katalog po tamtym kroku znika, a krok, który NAPRAWDĘ nic nie
/// zrobił, dalej nie zostawia niczego.
///
/// Osobno, bo to jest kontrola przeciw obejściu „nigdy nie kasuj" — bez niej całe kryterium
/// spełnia jedna skasowana linia, a obietnica T-95 przestaje obowiązywać po cichu.
fn the_rest_is_tidy(bench: &Bench, report: &RunReport) -> Result<(), Box<dyn Error>> {
    let folder = report.dir.join(WORK).join(COMMITS);
    assert!(
        !folder.exists(),
        "the folder the step worked in is still on disk at {}. The branch already carries \
         everything that was in it, so the folder adds only a full checkout of the repository \
         (T-95)",
        folder.display()
    );
    let trees = bench.git(&["worktree", "list"])?;
    assert!(
        !trees.contains(COMMITS),
        "the step is still registered with git as a place to work, so deleting the folder was \
         not enough: the next run under the same path is refused. The list says: {trees}"
    );

    let branches = bench.branches()?;
    assert!(
        !branches.iter().any(|name| name.contains(IDLES)),
        "the step that changed nothing left a branch behind. Keeping every branch is not the \
         fix here: after a week of runs `git branch` stops being readable and the one branch \
         that carries something disappears among the empty ones (T-52). Branches: {branches:?}"
    );
    assert!(
        !report.dir.join(WORK).join(IDLES).exists(),
        "the step that changed nothing left its folder behind"
    );
    Ok(())
}

// ── (b) katalog, którego nie dało się sprzątnąć, jest NAZWANY w opisie biegu ───────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_folder_that_could_not_be_tidied_is_named_in_the_run_record() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let store = Store::open(&bench.db())?;

    // Dubler zamyka swoje drzewo na klucz, więc `worktree remove --force` odmawia (jedno
    // `--force` zamka nie zdejmuje), a po nim odmawia też `branch -D`: gałąź jest wyjęta do
    // pracy w drzewie, którego nie udało się zdjąć.
    let report = bench
        .run(&store, Arc::new(Seen::default()), Habit::LockTheTree, None)
        .await?;

    let folder = report.dir.join(WORK).join(IDLES);
    let said = step_said(&report.dir, IDLES)?;
    assert!(
        !said.is_empty(),
        "the run's record says nothing about the folder that could not be cleared away. A green \
         run over a folder holding a full checkout of the repository is a leftover nobody looks \
         for: nothing mentioned it, and the next run under that path is refused"
    );
    assert!(
        said.contains(&folder.display().to_string()),
        "the sentence does not say WHERE the leftover is, so the person is told something went \
         wrong and then has to go looking for one folder among the ones every other run left. \
         It said: {said}"
    );

    Ok(())
}

// ── (c) nieznany punkt startu ZOSTAWIA gałąź ──────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_step_whose_starting_point_is_unknown_keeps_its_branch() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let store = Store::open(&bench.db())?;

    // Bez zapisu o punkcie startu nie da się odpowiedzieć na pytanie „czy ten krok coś
    // zacommitował". Wybór w wątpliwości jest tu jedyny możliwy: gałąź zostaje. Odwrotny
    // kasuje pracę zawsze wtedy, kiedy nie umiemy jej policzyć — czyli dokładnie wtedy, kiedy
    // najbardziej trzeba jej pilnować.
    let report = bench
        .run(
            &store,
            Arc::new(Seen::default()),
            Habit::LoseTheMarker,
            None,
        )
        .await?;

    let branch = isolate::branch_for(&report.id, IDLES);
    let branches = bench.branches()?;
    assert!(
        branches.iter().any(|name| name == &branch),
        "the branch {branch} was removed over a step whose starting point Loadout could not \
         read. A branch too many costs one line in `git branch`; a branch too few costs \
         somebody's day. Branches: {branches:?}"
    );
    assert!(
        !report.dir.join(WORK).join(IDLES).exists(),
        "the folder stayed as well, so this is not the careful choice — it is no cleanup at all"
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

/// Nazwa katalogu biegu — tą samą wartością posługuje się okno, otwierając bieg.
fn run_folder(report: &RunReport) -> Result<&str, Box<dyn Error>> {
    report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "the run directory has no UTF-8 folder name".into())
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

/// Co poza samą pracą robi w swoim katalogu krok, który niczego nie zmienia.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Habit {
    /// Nic. Tak wygląda krok, po którym nie ma prawa zostać ani gałąź, ani katalog.
    Nothing,
    /// Zamyka swoje drzewo na klucz gita, przez co sprzątanie po nim musi odmówić.
    LockTheTree,
    /// Kasuje zapis o tym, z czego jego drzewo powstało.
    LoseTheMarker,
}

/// Klucze kafelków, które naprawdę doszły do dublera.
///
/// Bez tego rejestru asercje o gałęziach są prawdziwe także dla biegu, w którym nie ruszył ani
/// jeden krok — a wtedy nie mówią o niczym.
#[derive(Debug, Default)]
struct Seen(Mutex<BTreeSet<String>>);

impl Seen {
    fn note(&self, key: &str) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key.to_owned());
    }

    fn snapshot(&self) -> BTreeSet<String> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

fn fake_drivers(seen: Arc<Seen>, habit: Habit) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { seen, habit });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct Fake {
    seen: Arc<Seen>,
    habit: Habit,
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
        let saving = spec.prompt.contains(COMMIT_TASK);
        let key = if saving { COMMITS } else { IDLES };
        if !saving && !spec.prompt.contains(IDLE_TASK) {
            anyhow::bail!(
                "this run handed the driver a task it does not know: {}",
                spec.prompt
            );
        }

        self.seen.note(key);

        if saving {
            fs::write(spec.cwd.join(MADE), MADE_TEXT)?;
            // CAŁA PRACA IDZIE NA GAŁĄŹ RĘKĄ AGENTA, tak jak robi to implementer, który sam
            // pilnuje swoich commitów. Po tym drzewo jest czyste, a praca zrobiona.
            git(&spec.cwd, &["add", "-A"])?;
            git(&spec.cwd, &["commit", "--quiet", "-m", AGENT_SAID])?;
        } else {
            match self.habit {
                Habit::Nothing => {}
                Habit::LockTheTree => lock_the_tree(&spec.cwd)?,
                Habit::LoseTheMarker => lose_the_marker(&spec.cwd)?,
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

/// Zamyka drzewo kroku na klucz gita.
///
/// To jest jedyny znany nam sposób, żeby `worktree remove --force` odmówił DETERMINISTYCZNIE:
/// zamek zdejmuje dopiero drugie `--force`, a `isolate::remove_tree` podaje jedno. Bez niego
/// przypadek (b) mierzyłby ścieżkę, której nie da się wywołać.
fn lock_the_tree(cwd: &Path) -> anyhow::Result<()> {
    git(cwd, &["worktree", "lock", &cwd.display().to_string()])?;
    Ok(())
}

/// Kasuje zapis o punkcie startu tego drzewa — ten sam plik, który zakłada bieg.
///
/// Nie da się go zgubić przez okno ani przez agenta; kasujemy go tutaj, bo to jedyny sposób,
/// żeby zapytać zamknięcie biegu o wybór, który robi **w wątpliwości**.
fn lose_the_marker(cwd: &Path) -> anyhow::Result<()> {
    let run_dir = cwd
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| anyhow::anyhow!("the step's folder does not sit under a run"))?;
    let name = cwd
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("the step's folder has no name"))?;
    fs::remove_file(run_dir.join(MARKERS).join(name))?;
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
        habit: Habit,
        after: Option<PathBuf>,
    ) -> Result<RunReport, Box<dyn Error>> {
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store,
            drivers: fake_drivers(seen, habit),
            processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow: self.workflow("commits-its-own-work", WORKFLOW)?,
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
        git(self.project.path(), args).map_err(|said| said.to_string().into())
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

/// Wołanie gita z tożsamością podaną na miejscu — ta sama dla ławki i dla dublera.
fn git(at: &Path, args: &[&str]) -> anyhow::Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(at)
        .args(["-c", "user.name=The Agent"])
        .args(["-c", "user.email=agent@loadout.invalid"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()?;
    if !out.status.success() {
        anyhow::bail!(
            "git {args:?} refused: {}",
            String::from_utf8_lossy(&out.stderr)
        );
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
