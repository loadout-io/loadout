//! Z-9: folder wewnątrz repozytorium, który nie jest jego korzeniem, mówi o sobie przy Starcie.
//!
//! # Co było zepsute
//!
//! `isolate::is_a_repo` pyta o KORZEŃ, nie o „czy leży w jakimś", i to jest jej właściwa
//! odpowiedź: drzewo robocze założone w podkatalogu cudzego repozytorium leżałoby w jego
//! indeksie. Skutkiem było jednak coś, o czym nic nie mówiło — bieg w podkatalogu monorepo
//! wygląda DOKŁADNIE jak bieg w korzeniu: kafelki idą, kroki się kończą, a pracy nie ma na żadnej
//! gałęzi i nie będzie. Człowiek dowiadywał się o tym przez nieobecność czegoś, czego nie umiał
//! nazwać.
//!
//! # ZDANIE CZYTAMY Z KANAŁU, nie z wartości zwróconej (niezmiennik 29)
//!
//! Wartość zwrócona przez funkcję dowodzi, że mechanizm istnieje; wiersz, który naprawdę wyszedł
//! kanałem do okna, dowodzi, że produkt działa. Między jednym a drugim mieszka klasa wady, dla
//! której to repo powstało.
//!
//! # DWA BIEGI KONTROLNE, i one są tu połową kryterium
//!
//! Korzeń repozytorium i folder spoza jakiegokolwiek repozytorium NIE dostają tego zdania ani
//! razu. Bez nich całe kryterium spełnia jedna linia wysyłana zawsze — czyli ostrzeżenie, które
//! pada także tam, gdzie praca na gałąź trafia, i którego człowiek po tygodniu nie czyta.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::{INSIDE_A_REPOSITORY_BUT_NOT_ITS_ROOT, run_workflow_inner};
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

/// Nazwa kafelka. `Line::Problem` niesie nazwę tego, o kim zdanie mówi.
const STEP_NAME: &str = "Writes";

/// Podkatalog projektu wewnątrz repozytorium człowieka.
const INSIDE: &str = "packages/app";

const OWN_COPY: &str = r#"{
  "format": 1,
  "id": "wf_inside_a_repo",
  "name": "One step in its own copy",
  "steps": [
    {
      "kind": "agent",
      "id": "s_1",
      "name": "Writes",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "write something down",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

/// Ten sam bieg, tylko BEZ ani jednej własnej kopii: kafelek pracuje w samym folderze projektu.
///
/// 2026-09 (Z-9) — DRUGI KSZTAŁT JEST TU CAŁYM KRYTERIUM, nie symetrią dla ozdoby. Pierwsza
/// wersja tej poprawki pytała najpierw o to, czy jakiś krok dostał własną kopię plików, więc dla
/// tego workflow milczała — a to jest ten sam bieg z tą samą stratą: praca zostaje w cudzym
/// drzewie roboczym i nie ma jej na żadnej naszej gałęzi. `folder: project` jest przy tym
/// wariantem DOMYŚLNYM (`workflow::Folder`), czyli tym, który dostaje każdy kafelek, przy którym
/// nikt nie tknął tego wyboru.
const IN_THE_PROJECT_FOLDER: &str = r#"{
  "format": 1,
  "id": "wf_inside_a_repo_no_copy",
  "name": "One step in the project folder",
  "steps": [
    {
      "kind": "agent",
      "id": "s_1",
      "name": "Writes",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "write something down",
      "folder": { "use": "project" },
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

/// Oba kształty, dla biegów kontrolnych: warunkiem jest FOLDER, więc żaden z nich nie mówi nic.
const BOTH_SHAPES: [&str; 2] = [OWN_COPY, IN_THE_PROJECT_FOLDER];

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

// ── zdanie, którego to zadanie dotyczy ────────────────────────────────────────────────────

/// Zdanie z treści zlecenia, słowo w słowo. Stała po stronie Rusta ma mu być RÓWNA.
///
/// # 2026-09 (Z-9) — RÓWNOŚĆ, nie `contains`
///
/// Sądzone osobno od dostarczenia, bo to są dwie różne rzeczy: tamto pilnuje, że wiersz dojechał
/// do okna, to — że mówi to, co człowiek ma przeczytać. `contains` przepuszczało tu jednak każdy
/// prefiks, i przepuściło prawdziwy: „Loadout copied the files instead of branching:" mówiło
/// o kopii plików nawet wtedy, gdy bieg nie robił ani jednej. Zdanie nadmiarowe wygląda
/// w kryterium dokładnie jak zdanie właściwe, dopóki pyta się o zawieranie.
const WHAT_IT_HAS_TO_SAY: &str = "this folder is inside a git repository but is not its root, so \
                                  the work will not land on a branch";

#[test]
fn the_sentence_is_the_one_the_person_has_to_read() {
    assert_eq!(
        INSIDE_A_REPOSITORY_BUT_NOT_ITS_ROOT, WHAT_IT_HAS_TO_SAY,
        "the sentence Loadout puts on the stream is not the one this task asks for, word for \
         word. Both halves of it are the point — the cause tells the person what to change, the \
         loss tells them why they should care — and anything ADDED to it claims something the \
         run may not have done"
    );
}

// ── (a) podkatalog repozytorium: dokładnie jedno zdanie ───────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_run_in_a_subfolder_of_a_repository_says_so_exactly_once() -> Result<(), Box<dyn Error>> {
    let bench = Bench::inside_a_repo()?;
    let said = bench.run(OWN_COPY).await?;

    exactly_that_one_sentence(&said);
    Ok(())
}

/// TEN SAM FOLDER, workflow BEZ ani jednej własnej kopii — i to samo zdanie.
///
/// 2026-09 (Z-9) — POWSTAŁO Z ODRZUCENIA. Pierwsza wersja wysyłała ten wiersz dopiero wtedy, gdy
/// jakiś krok dostał kopię plików, więc workflow pracujący w samym folderze projektu — czyli ten
/// z DOMYŚLNYM wyborem folderu — nie dostawał ani słowa. Strata jest w obu kształtach ta sama:
/// praca zostaje w cudzym drzewie roboczym i nie ma jej na żadnej naszej gałęzi.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_run_in_a_subfolder_that_makes_no_copy_of_its_own_still_says_so()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::inside_a_repo()?;
    let said = bench.run(IN_THE_PROJECT_FOLDER).await?;

    exactly_that_one_sentence(&said);
    Ok(())
}

/// Jeden wiersz, ta treść CO DO ZNAKU, na wierszu kafelka.
///
/// Wspólne dla obu kształtów workflow, bo to jest jedno kryterium zadane dwa razy (niezmiennik
/// 13): dwie kopie tych asercji rozjechałyby się przy pierwszej poprawce jednej z nich.
fn exactly_that_one_sentence(said: &[Problem]) {
    assert_eq!(
        said.len(),
        1,
        "a run started in a folder that sits inside a repository but is not its root has to say \
         so exactly once. Nothing said it at all until this change, and saying it per step teaches \
         the person to scroll past it. It said: {said:?}"
    );
    assert_eq!(
        said[0].text, WHAT_IT_HAS_TO_SAY,
        "the line that reached the window is not, word for word, the sentence the person has to \
         read. A prefix of our own in front of it claims something this run may not have done"
    );
    assert_eq!(
        said[0].agent, STEP_NAME,
        "the line has to name the step it is about the way the canvas names it, or the person \
         reads a warning with nothing on the screen to attach it to (invariant 14)"
    );
}

// ── (b) korzeń repozytorium: ani razu ─────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_run_in_the_root_of_a_repository_says_nothing_of_the_kind() -> Result<(), Box<dyn Error>>
{
    // OBA KSZTAŁTY, bo warunkiem jest folder: kryterium sprawdzone na jednym z nich przechodzi
    // także dla implementacji, która pyta o kopie plików zamiast o folder.
    for shape in BOTH_SHAPES {
        let bench = Bench::at_the_root_of_a_repo()?;
        let said = bench.run(shape).await?;

        assert!(
            said.is_empty(),
            "a run in the root of a repository was told its work will not land on a branch — and \
             it will: that is exactly the folder where Loadout puts every step on its own branch. \
             A warning that also fires when nothing is wrong is one nobody reads by Friday. It \
             said: {said:?}"
        );
    }
    Ok(())
}

// ── (c) folder spoza jakiegokolwiek repozytorium: ani razu ────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_run_in_a_folder_that_is_no_repository_says_nothing_of_the_kind()
-> Result<(), Box<dyn Error>> {
    for shape in BOTH_SHAPES {
        let bench = Bench::outside_any_repo()?;
        let said = bench.run(shape).await?;

        assert!(
            said.is_empty(),
            "a folder that is no repository at all was told it is inside one. There is nothing to \
             move here and nothing to fix — the sentence would send the person looking for a root \
             that does not exist. It said: {said:?}"
        );
    }
    Ok(())
}

// ── dubler ────────────────────────────────────────────────────────────────────────────────

fn fake_drivers() -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake);
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct Fake;

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

// ── ławka ─────────────────────────────────────────────────────────────────────────────────

/// Jeden wiersz o kłopocie, tak jak go dostało okno.
#[derive(Debug)]
struct Problem {
    agent: String,
    text: String,
}

struct Bench {
    home: TempDir,
    /// Korzeń fikstury. Trzyma katalog projektu przy życiu, także gdy projekt leży pod nim.
    root: TempDir,
    project: PathBuf,
}

impl Bench {
    /// Projekt leżący WEWNĄTRZ repozytorium człowieka, ale nie w jego korzeniu.
    fn inside_a_repo() -> Result<Self, Box<dyn Error>> {
        let bench = Self::new(|root| root.join(INSIDE))?;
        fs::write(bench.root.path().join("notes.txt"), "the person's own file")?;
        make_a_repo(bench.root.path())?;
        Ok(bench)
    }

    /// Projekt, który JEST korzeniem repozytorium. Bieg kontrolny.
    fn at_the_root_of_a_repo() -> Result<Self, Box<dyn Error>> {
        let bench = Self::new(Path::to_path_buf)?;
        make_a_repo(&bench.project)?;
        Ok(bench)
    }

    /// Projekt spoza jakiegokolwiek repozytorium. Drugi bieg kontrolny.
    fn outside_any_repo() -> Result<Self, Box<dyn Error>> {
        Self::new(Path::to_path_buf)
    }

    fn new(where_the_project_is: impl Fn(&Path) -> PathBuf) -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let root = TempDir::new()?;
        let project = where_the_project_is(root.path());
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.join(".loadout"))?;
        fs::write(home.path().join("agents").join("scribe.md"), AGENT)?;
        fs::write(project.join("own-notes.txt"), "what this folder holds")?;
        Ok(Self {
            home,
            root,
            project,
        })
    }

    /// Jeden bieg tego workflow. Oddaje wiersze o kłopocie, które NAPRAWDĘ wyszły kanałem do okna.
    async fn run(&self, workflow: &str) -> Result<Vec<Problem>, Box<dyn Error>> {
        let store = Store::open(&self.project.join(".loadout").join("loadout.db"))?;
        let deps = RunDeps {
            home: self.home.path(),
            project: &self.project,
            store: &store,
            drivers: fake_drivers(),
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let path = self.home.path().join("workflows").join("inside.json");
        fs::write(&path, workflow)?;
        let request = RunRequest {
            workflow: path,
            how_many_at_once: 1,
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
        // ZDANIE CZYTAMY ZA POMPĄ. Kolejka linii jest osobnym zadaniem, więc wiersz wysłany
        // przed pierwszym krokiem dociera do kanału dopiero tutaj.
        let _ = tokio::time::timeout(PATIENCE, pump).await;

        assert_eq!(
            report.steps,
            vec![StepState::Succeeded],
            "the step has to finish, or nothing above is talking about a run that happened"
        );
        Ok(recorder.problems())
    }
}

/// Repozytorium z jednym commitem w tym katalogu.
fn make_a_repo(at: &Path) -> Result<(), Box<dyn Error>> {
    git_at(at, &["init", "--quiet"])?;
    git_at(at, &["add", "-A"])?;
    git_at(
        at,
        &["commit", "--quiet", "-m", "the person's first commit"],
    )
}

fn git_at(at: &Path, args: &[&str]) -> Result<(), Box<dyn Error>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(at)
        .args(["-c", "user.name=The Person"])
        .args(["-c", "user.email=person@loadout.invalid"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()?;
    if !out.status.success() {
        return Err(format!(
            "git {args:?} refused: {}",
            String::from_utf8_lossy(&out.stderr)
        )
        .into());
    }
    Ok(())
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

    /// Wszystkie wiersze o kłopocie, które okno dostało, w kolejności dostarczenia.
    ///
    /// WSZYSTKIE, a nie pierwszy pasujący: kryterium żąda DOKŁADNIE JEDNEGO, więc funkcja
    /// oddająca pierwsze trafienie przechodziłaby także dla zdania powtórzonego przy każdym kroku.
    fn problems(&self) -> Vec<Problem> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter_map(|batch| batch.as_array())
            .flatten()
            .filter(|line| line.get("kind").and_then(serde_json::Value::as_str) == Some("problem"))
            .map(|line| Problem {
                agent: line
                    .get("agent")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                text: line
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            })
            .collect()
    }
}
