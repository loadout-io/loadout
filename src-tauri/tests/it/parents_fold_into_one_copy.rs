//! Dwa kroki pracujące NARAZ, każdy w swojej kopii, a krok pod nimi dostaje JEDNĄ kopię z pracą
//! obydwu — i zatrzymanie, kiedy obaj napisali w tym samym pliku co innego.
//!
//! Do 2026-08-29 ten kształt był odmową: *„…the steps before it work in 2 different folders"*.
//! Dwie równoległe gałęzie dało się narysować i nie dało się na nich pracować, choć „front
//! i backend osobno, potem ktoś to składa" jest tym, po co ten produkt powstał.
//!
//! # Słaba wersja tego kryterium
//!
//! Dwa kroki w rozłącznych okienkach czasu. „Ile naraz" musi znaczyć naraz (niezmiennik 11),
//! a bieg, w którym rodzice idą po kolei, spełnia każdą asercję o zawartości kopii i nie dowodzi
//! niczego o równoległości. Dlatego dubler stoi na **barierze**: obaj rodzice muszą do niej
//! dojść, zanim którykolwiek skończy, a kolejność zakończeń jest odwrotna do kolejności startów.
//! Bieg sekwencyjny wiesza się na tej barierze i test kończy się limitem czasu, nie zieloną
//! asercją.
//!
//! Druga słaba wersja: sprawdzenie, że plik ISTNIEJE. Plik z commita też istnieje. Rozróżnia
//! dopiero TREŚĆ, i to czytana w chwili wywołania sterownika.
//!
//! Trzecia, przy odmowie: asercja na wartości zwróconej przez funkcję składającą. Kryterium
//! o odmowie leży tam, gdzie zdanie widzi CZŁOWIEK (niezmiennik 29) — czyli w tym, co naprawdę
//! wyszło kanałem do okna.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
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
use tempfile::TempDir;
use tokio::sync::{Barrier, Semaphore, mpsc};

/// Etykieta vendora dublera.
const VENDOR: &str = "claude-code";

/// Katalog, pod którym bieg zakłada drzewa robocze kroków.
const WORK: &str = "work";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony. Bieg, w którym rodzice idą po kolei,
/// kończy się właśnie tutaj — i to jest zamierzone.
const PATIENCE: Duration = Duration::from_secs(30);

/// Plik ŚLEDZONY przez gita: jest w commicie, a lewy rodzic go nadpisuje.
const SHARED: &str = "shared.txt";
const COMMITTED: &str = "what the commit says";
const LEFT_WROTE: &str = "the left step rewrote this";
/// Co pisze w tym samym pliku prawy rodzic, kiedy fikstura mierzy niezgodę.
const RIGHT_WROTE: &str = "the right step rewrote it differently";

/// Plik NIEŚLEDZONY, w katalogu, którego w projekcie nie ma — zakłada go prawy rodzic.
const ADDED: &str = "docs/added.txt";
const RIGHT_ADDED: &str = "the right step made this file";

/// Nazwy z kafelków. Dobrane tak, żeby nie występowały w żadnym innym wierszu biegu: asercja
/// o zdaniu dla człowieka ma świecić na TYM zdaniu, a nie na cudzym.
const LEFT_NAME: &str = "Paint the walls";
const RIGHT_NAME: &str = "Wire the lights";
const BELOW_NAME: &str = "Sign it off";

/// Kroki po tym, czym się w tym teście przedstawiają.
const LEFT: &str = "s_left";
const RIGHT: &str = "s_right";
const BELOW: &str = "s_below";

/// Dwie własne kopie i jeden krok pod nimi, który mówi „to samo drzewo, co krok przede mną".
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_fan_in",
  "name": "Two branches, one copy below",
  "steps": [
    {
      "kind": "agent",
      "id": "s_left",
      "name": "Paint the walls",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "step left: change the shared file",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_right",
      "name": "Wire the lights",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "step right: add a file of its own",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 200 }
    },
    {
      "kind": "agent",
      "id": "s_below",
      "name": "Sign it off",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "step below: read what both of them did",
      "folder": { "use": "same-copy" },
      "at": { "x": 240, "y": 100 }
    }
  ],
  "links": [
    { "from": "s_left", "to": "s_below" },
    { "from": "s_right", "to": "s_below" }
  ]
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

// ── kryteria 1 i 2 ─────────────────────────────────────────────────────────────────────────

// `clippy::too_many_lines` liczy tu same ZDANIA: wszystkie asercje czytają jeden i ten sam bieg,
// a każda niesie komunikat mówiący, co poszło nie tak i dlaczego to jest wada. Rozbicie na
// funkcje pomocnicze znaczyłoby albo uruchomić ten bieg dwa razy, albo przewlec jego wynik przez
// cudzą sygnaturę — obie wersje są dłuższe, tylko rozłożone tak, żeby licznik ich nie widział.
#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn both_parents_work_at_once_and_the_step_below_sees_both_changes()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let project = bench.project.path().to_path_buf();

    fs::write(project.join(SHARED), COMMITTED)?;
    bench.make_a_repo()?;

    let seen = Arc::new(Seen::default());
    let meeting = Arc::new(Meeting::new());
    let recorder = Delivered::default();
    let report = bench
        .go(
            fake_drivers(Arc::clone(&seen), Arc::clone(&meeting), Writes::ItsOwnFile),
            &recorder,
        )
        .await?;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; 3],
        "all three steps have to finish for the folder assertions to mean anything; they ended \
         as {:?}. The run said: {}",
        report.steps,
        recorder.text()
    );

    let looked = seen.snapshot();
    let left = looked.get(LEFT).ok_or("the left step never ran")?;
    let right = looked.get(RIGHT).ok_or("the right step never ran")?;
    let below = looked.get(BELOW).ok_or(
        "the step below the two of them never reached the driver, so nothing was measured",
    )?;

    // (a) NARAZ, NIE PO KOLEI. Obaj rodzice doszli do punktu spotkania, zanim którykolwiek
    //     skończył — a skończyli w kolejności odwrotnej do startów. Dwa rozłączne okienka
    //     czasu nie umieją tego kształtu odtworzyć (niezmiennik 11).
    let started = meeting.started();
    let finished = meeting.finished();
    assert_eq!(
        started.len(),
        2,
        "both of the two steps above have to reach the meeting point; only {started:?} did"
    );
    assert_eq!(
        finished,
        started.iter().rev().copied().collect::<Vec<_>>(),
        "the step that started second has to finish first, or this test is not measuring an \
         overlap. Starts: {started:?}, finishes: {finished:?}"
    );

    // (b) KOPIA KROKU PONIŻEJ NIE JEST ŻADNĄ Z KOPII RODZICÓW. Wybranie jednej z nich jest
    //     najtańszą złą implementacją: krok kończy się sukcesem i po cichu nie widzi połowy
    //     pracy.
    assert_ne!(
        below.cwd, left.cwd,
        "the step below was handed the left step's own folder, so the right step's work is not \
         in it at all"
    );
    assert_ne!(
        below.cwd, right.cwd,
        "the step below was handed the right step's own folder, so the left step's work is not \
         in it at all"
    );
    assert_ne!(
        below.cwd, project,
        "the step below worked in the project folder itself. 'the same folder as the step before \
         me' has to mean a folder this run laid out, never the one the human is editing"
    );
    let trees = report.dir.join(WORK);
    assert!(
        below.cwd.starts_with(&trees),
        "the folded folder has to be a working copy this run laid out, so it lives under \
         {trees:?}. It was {:?}",
        below.cwd
    );

    // (c) I WIDZI OBIE ZMIANY CO DO BAJTA, w chwili wywołania sterownika. Plik śledzony przez
    //     gita przychodzi od jednego rodzica, plik nieśledzony — od drugiego.
    assert_eq!(
        below.shared.as_deref(),
        Some(LEFT_WROTE),
        "the step below read {SHARED} as {:?}. The step before it rewrote that file, and a step \
         that works on what came before it has to see the text that is really there — anything \
         else means it is reviewing code nobody changed",
        below.shared
    );
    assert_eq!(
        below.added.as_deref(),
        Some(RIGHT_ADDED),
        "the step below did not find {ADDED}, a file the other step above it made. Git does not \
         track that file and it sits in a folder that did not exist before, so an implementation \
         that carries only tracked changes loses it silently — and silently is the whole problem"
    );

    // (d) FOLDER CZŁOWIEKA JEST NIETKNIĘTY. Składanie czyta kopie, nigdy ich nie przenosi.
    assert_eq!(
        fs::read_to_string(project.join(SHARED))?,
        COMMITTED,
        "the project's own {SHARED} changed. Steps work in copies precisely so that the folder \
         the human is editing never moves under them"
    );
    assert!(
        !project.join(ADDED).exists(),
        "{ADDED} appeared in the project folder. Nothing a step wrote in its own copy has a way \
         back into the human's files without them asking for it"
    );

    Ok(())
}

// ── kryteria 3 i 4 ─────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_file_two_parents_changed_differently_stops_the_step_and_says_so()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let project = bench.project.path().to_path_buf();

    fs::write(project.join(SHARED), COMMITTED)?;
    bench.make_a_repo()?;

    let seen = Arc::new(Seen::default());
    let meeting = Arc::new(Meeting::new());
    let recorder = Delivered::default();
    let report = bench
        .go(
            fake_drivers(Arc::clone(&seen), Arc::clone(&meeting), Writes::TheSameFile),
            &recorder,
        )
        .await?;

    // (a) STEROWNIK KROKU PONIŻEJ NIE ZOSTAŁ WYWOŁANY ANI RAZU. Odmowa po starcie procesu jest
    //     już zapłacona — i, co gorsze, agent zdążyłby przeczytać kopię, w której jedna ze
    //     stron po cichu wygrała.
    assert_eq!(
        seen.times(BELOW),
        0,
        "the step below reached the driver even though the two steps above it wrote different \
         text into {SHARED}. Picking one of them is guessing: which agent finished first changes \
         from run to run, so the step below would be working on code nobody wrote"
    );

    // (b) I SKOŃCZYŁ SIĘ JAKO NIEUDANY, a nie po cichu pominięty.
    assert_eq!(
        report.steps,
        vec![
            StepState::Succeeded,
            StepState::Succeeded,
            StepState::Failed
        ],
        "the two steps above did their work and the one below could not start, so that is what \
         the run has to report. It reported {:?}",
        report.steps
    );

    // (c) CZŁOWIEK CZYTA, KTÓRY PLIK I KTÓRE DWA KAFELKI — w tym, co naprawdę wyszło kanałem do
    //     okna (niezmiennik 29). Zdanie oglądane wyłącznie w zwróconej wartości dowodziłoby, że
    //     mechanizm istnieje, a nie że produkt cokolwiek mówi.
    let said = recorder.text();
    assert!(
        said.contains(SHARED),
        "no line the window got names {SHARED}, the file the two steps disagree about. Without \
         it the human is told that something clashed and has to find out what. The run said: \
         {said}"
    );
    assert!(
        said.contains(BELOW_NAME),
        "the line has to say which tile could not start, or the human reads a sentence about two \
         other steps and has to work out whose row it belongs in. The run said: {said}"
    );
    assert!(
        said.contains(LEFT_NAME) && said.contains(RIGHT_NAME),
        "the line has to name BOTH steps the way the canvas names them, or the human knows there \
         is a clash and not who is in it. It named {LEFT_NAME}: {}, {RIGHT_NAME}: {}. The run \
         said: {said}",
        said.contains(LEFT_NAME),
        said.contains(RIGHT_NAME)
    );

    // (d) PRACA OBU RODZICÓW JEST DALEJ OSIĄGALNA — bo to jest jedyny powód, dla którego wolno
    //     zatrzymać krok zamiast wybrać jedną wersję: człowiek ma gdzie zajrzeć.
    let mine = format!("loadout/{}/", report.id);
    let branches = isolate::branches_under(&project, &mine);
    let left_branch = format!("{mine}{LEFT}");
    let right_branch = format!("{mine}{RIGHT}");
    assert!(
        branches.contains(&left_branch) && branches.contains(&right_branch),
        "both steps' work has to stay reachable after the refusal; that is the whole trade for \
         stopping the step below. Found: {branches:?}"
    );
    assert_eq!(
        git(&project, &["show", &format!("{left_branch}:{SHARED}")])?,
        LEFT_WROTE,
        "the left step's own version of {SHARED} is not on its branch, so there is nothing for \
         the human to compare"
    );
    assert_eq!(
        git(&project, &["show", &format!("{right_branch}:{SHARED}")])?,
        RIGHT_WROTE,
        "the right step's own version of {SHARED} is not on its branch"
    );

    Ok(())
}

// ── punkt spotkania ────────────────────────────────────────────────────────────────────────

/// Bariera dwóch rodziców plus odwrócona kolejność wyjścia.
///
/// Sam `Barrier` dowodzi tylko, że obaj byli w środku w tej samej chwili. Odwrócona kolejność
/// zakończeń dokłada drugą połowę: bieg, który puszcza rodziców po kolei, nie ma jak jej
/// odtworzyć nawet przypadkiem.
#[derive(Debug)]
struct Meeting {
    started: Mutex<Vec<&'static str>>,
    finished: Mutex<Vec<&'static str>>,
    both_here: Barrier,
    /// Pozwolenie dla tego, który wystartował PIERWSZY: dostaje je dopiero, gdy drugi wyszedł.
    second_is_out: Semaphore,
}

impl Meeting {
    fn new() -> Self {
        Self {
            started: Mutex::new(Vec::new()),
            finished: Mutex::new(Vec::new()),
            both_here: Barrier::new(2),
            second_is_out: Semaphore::new(0),
        }
    }

    /// Zapisuje start i mówi, czy ten krok był drugi.
    fn arrived(&self, step: &'static str) -> bool {
        let mut started = self.started.lock().unwrap_or_else(PoisonError::into_inner);
        started.push(step);
        started.len() == 2
    }

    fn left(&self, step: &'static str) {
        self.finished
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(step);
    }

    fn started(&self) -> Vec<&'static str> {
        self.started
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn finished(&self) -> Vec<&'static str> {
        self.finished
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

// ── co dubler zobaczył ─────────────────────────────────────────────────────────────────────

/// Co jeden krok zastał w swoim katalogu roboczym, odczytane w chwili wejścia do sterownika.
#[derive(Debug, Default, Clone)]
struct Look {
    cwd: PathBuf,
    shared: Option<String>,
    added: Option<String>,
}

fn look_at(cwd: &Path) -> Look {
    Look {
        cwd: cwd.to_path_buf(),
        shared: fs::read_to_string(cwd.join(SHARED)).ok(),
        added: fs::read_to_string(cwd.join(ADDED)).ok(),
    }
}

/// Co zobaczył każdy krok i ile razy tam wszedł.
#[derive(Debug, Default)]
struct Seen(Mutex<BTreeMap<String, (usize, Look)>>);

impl Seen {
    fn record(&self, step: &str, look: Look) {
        let mut rows = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let row = rows.entry(step.to_owned()).or_insert((0, look));
        row.0 += 1;
    }

    /// Ile razy sterownik dostał ten krok. Zero jest odpowiedzią, nie brakiem odpowiedzi.
    fn times(&self, step: &str) -> usize {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(step)
            .map_or(0, |row| row.0)
    }

    fn snapshot(&self) -> BTreeMap<String, Look> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|(step, row)| (step.clone(), row.1.clone()))
            .collect()
    }
}

/// Który krok tu wszedł. Prompt jest jedynym śladem: `RunSpec` nie niesie identyfikatora kroku.
fn which_step(prompt: &str) -> &'static str {
    if prompt.contains("step left") {
        LEFT
    } else if prompt.contains("step right") {
        RIGHT
    } else if prompt.contains("step below") {
        BELOW
    } else {
        "a step this test cannot name"
    }
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

/// Co pisze prawy rodzic: własny nowy plik, albo TEN SAM plik, co lewy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Writes {
    ItsOwnFile,
    TheSameFile,
}

fn fake_drivers(seen: Arc<Seen>, meeting: Arc<Meeting>, right: Writes) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        seen,
        meeting,
        right,
    });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler, który NAPRAWDĘ pisze i czyta w `spec.cwd`.
///
/// Dubler oddający same zdarzenia przeszedłby te asercje na implementacji, która nie zakłada
/// ani nie składa żadnego katalogu.
#[derive(Debug)]
struct Fake {
    seen: Arc<Seen>,
    meeting: Arc<Meeting>,
    right: Writes,
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
        let step = which_step(&spec.prompt);
        // Odczyt PRZED zapisem: inaczej rodzic meldowałby własną zmianę jako to, co zastał.
        self.seen.record(step, look_at(&spec.cwd));

        let mut second = false;
        match step {
            LEFT => {
                fs::write(spec.cwd.join(SHARED), LEFT_WROTE)?;
                second = self.meeting.arrived(LEFT);
                self.meeting.both_here.wait().await;
            }
            RIGHT => {
                if self.right == Writes::TheSameFile {
                    fs::write(spec.cwd.join(SHARED), RIGHT_WROTE)?;
                } else {
                    // Katalog, którego w projekcie nie ma, i plik, o którym git nie wie.
                    fs::create_dir_all(spec.cwd.join(ADDED).parent().unwrap_or(&spec.cwd))?;
                    fs::write(spec.cwd.join(ADDED), RIGHT_ADDED)?;
                }
                second = self.meeting.arrived(RIGHT);
                self.meeting.both_here.wait().await;
            }
            _ => {}
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
        Ok(Box::new(Turn {
            events,
            session,
            step,
            second,
            meeting: Arc::clone(&self.meeting),
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    step: &'static str,
    /// Czy ten krok doszedł do punktu spotkania jako drugi. Tylko rodzice.
    second: bool,
    meeting: Arc<Meeting>,
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
        // ODWROTNA KOLEJNOŚĆ WYJŚCIA. Ten, który wszedł drugi, wychodzi pierwszy i dopiero on
        // wypuszcza tamtego — więc oba okna czasu nakładają się na siebie w całości.
        if self.step == LEFT || self.step == RIGHT {
            if self.second {
                self.meeting.left(self.step);
                self.meeting.second_is_out.add_permits(1);
            } else {
                let _ = self.meeting.second_is_out.acquire().await;
                self.meeting.left(self.step);
            }
        }

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

    /// Cały bieg, od dyskiem po pompę. Dwa miejsca w puli, bo dwoje rodziców ma pracować NARAZ.
    async fn go(
        &self,
        drivers: Drivers,
        recorder: &Delivered,
    ) -> Result<RunReport, Box<dyn Error>> {
        let store = Store::open(&self.db())?;
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store: &store,
            drivers,
            processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow: self.workflow("fan-in", WORKFLOW)?,
            how_many_at_once: 2,
            task: None,
            part: None,
            handoffs_from: None,
        };

        let (sink, source) = line_channel(QUEUE_CAP);
        let pump = spawn_pump(source, recorder.channel());
        let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
            .await
            .map_err(|_| {
                "the run never came back. Two steps that are supposed to work at once wait for \
                 each other at a meeting point, so a run that starts them one after the other \
                 ends exactly here"
            })?
            .map_err(|why| format!("the run refused before a single step started: {why}"))?;
        let _ = tokio::time::timeout(PATIENCE, pump).await;
        Ok(report)
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
    let out = std::process::Command::new("git")
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

/// Paczki, które wyszły kanałem — czyli to, co naprawdę dostało okno.
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
