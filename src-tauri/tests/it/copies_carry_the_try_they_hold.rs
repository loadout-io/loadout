//! Kopia niesie znacznik tego, KTÓRĄ PRÓBĘ w sobie ma — i składanie dwóch kopii z dwóch różnych
//! prób jest odmawiane, zanim sterownik zobaczy cokolwiek.
//!
//! # Stan bez tego znacznika
//!
//! Dwie kopie tej samej pętli mają dziś rozłączne katalogi i nic ponadto. Rozłączny katalog mówi
//! „to nie jest ta sama kopia" i ani słowa o tym, z której rundy jest praca w środku — a rundy
//! **dzielą** folder (`commands::run::work_key_for`), więc gałąź pominięta w próbie drugiej
//! zostaje z pracą próby pierwszej na zawsze. Złożenie jej z gałęzią, która w próbie drugiej
//! naprawdę pracowała, daje kopię nie do odróżnienia od poprawnej: krok poniżej kończy się
//! sukcesem nad kodem, którego nikt nie napisał w jednym kawałku.
//!
//! # Słabe wersje tego kryterium
//!
//! Pierwsza: asercja na wartości zwróconej przez funkcję składającą. Kryterium o odmowie leży
//! tam, gdzie zdanie widzi CZŁOWIEK (niezmiennik 29) — czyli w wierszu, który naprawdę wyszedł
//! kanałem do okna, na wierszu tego kafelka.
//!
//! Druga: sprawdzenie, że krok poniżej „nie przeszedł". Krok, który wystartował i dopiero potem
//! odmówił, jest już zapłacony, a agent zdążył przeczytać kopię złożoną z dwóch prób. Dlatego
//! kryterium mierzy liczbę wywołań sterownika, a nie stan kroku.
//!
//! Trzecia, i najgroźniejsza: sam bieg z odmową. Implementacja odmawiająca ZAWSZE przechodzi go
//! w całości i kasuje przy tym fizyczne składanie. Dlatego drugi bieg jest tym samym grafem bez
//! warunków na strzałkach: obie gałęzie pracują w każdej próbie, składanie idzie jak dotąd,
//! a krok poniżej czyta obie zmiany co do bajta.

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
use tokio::sync::mpsc;

/// Etykieta vendora dublera.
const VENDOR: &str = "claude-code";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony. Ten graf zakłada pięć drzew roboczych
/// i przechodzi dwie próby pętli, więc sufit jest szerszy niż przy zwykłym rozwidleniu.
const PATIENCE: Duration = Duration::from_mins(1);

/// Pliki, które piszą obie gałęzie. RÓŻNE ścieżki z rozmysłu: dwie gałęzie piszące w jednym pliku
/// odmówiłyby już dziś (`fan_in::Trouble::TwoAnswers`), więc taki bieg nie powiedziałby ani słowa
/// o pochodzeniu kopii.
const LEFT_FILE: &str = "left.txt";
const LEFT_WROTE: &str = "the left branch wrote this";
const RIGHT_FILE: &str = "docs/right.txt";
const RIGHT_WROTE: &str = "the right branch wrote this";

/// Plik, którym wejście pętli dowodzi, że jest co sprawdzać — bez niego sędzia domyka pętlę
/// w pierwszej próbie (`Live::nothing_to_judge`) i drugiej próby nie ma wcale.
const PICKED_FILE: &str = "picked.txt";

/// Nazwy z kafelków. Dobrane tak, żeby nie występowały w żadnym innym wierszu biegu: asercja
/// o zdaniu dla człowieka ma świecić na TYM zdaniu, a nie na cudzym.
const LEFT_NAME: &str = "Paint the walls";
const RIGHT_NAME: &str = "Wire the lights";
const JOIN_NAME: &str = "Sign it off";

/// Kroki po tym, czym się w tym teście przedstawiają.
const PICK: &str = "s_pick";
const LEFT: &str = "s_left";
const RIGHT: &str = "s_right";
const JUDGE: &str = "s_judge";
const JOIN: &str = "s_join";

/// Miejsce, w które wchodzą warunki na strzałkach — jedyna różnica między dwoma biegami.
const HOLE: &str = "__CONDITIONS__";

/// Gałąź wybierana warunkiem: lewa w próbie pierwszej, prawa w drugiej.
const BRANCHING: &str = r#"[
    { "from": "s_pick", "to": "s_left",  "when": { "source": "handoff", "field": "branch", "equals": "left" } },
    { "from": "s_pick", "to": "s_right", "when": { "source": "handoff", "field": "branch", "equals": "right" } }
  ]"#;

/// Pętla o dwóch próbach z rozwidleniem w środku i krokiem składającym pod nim.
///
/// `s_join` stoi POZA ciałem pętli (nie da się z niego dojść do sędziego), więc wchodzą do niego
/// strzałki z próby OSTATNIEJ obu gałęzi — a to jest dokładnie ten kształt, w którym gałąź
/// pominięta w tej próbie oddaje pracę z próby wcześniejszej.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_two_tries",
  "name": "One loop, two branches, one copy below",
  "steps": [
    {
      "kind": "agent",
      "id": "s_pick",
      "name": "Pick a side",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "step pick: say which branch this try goes down",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 100 }
    },
    {
      "kind": "agent",
      "id": "s_left",
      "name": "Paint the walls",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "step left: write the left file",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_right",
      "name": "Wire the lights",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "step right: write the right file",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 200 }
    },
    {
      "kind": "agent",
      "id": "s_judge",
      "name": "Read it back",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "step judge: say whether this try is good enough",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 480, "y": 100 }
    },
    {
      "kind": "agent",
      "id": "s_join",
      "name": "Sign it off",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "step join: read what both branches did",
      "folder": { "use": "same-copy" },
      "at": { "x": 720, "y": 100 }
    }
  ],
  "links": [
    { "from": "s_pick", "to": "s_left" },
    { "from": "s_pick", "to": "s_right" },
    { "from": "s_left", "to": "s_judge" },
    { "from": "s_right", "to": "s_judge" },
    { "from": "s_left", "to": "s_join" },
    { "from": "s_right", "to": "s_join" },
    { "from": "s_judge", "to": "s_pick", "max_turns": 2 }
  ],
  "linkConditions": __CONDITIONS__
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

// ── kryteria 1, 2 i 3 ──────────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_copy_left_on_an_older_try_stops_the_join_and_says_which_tries()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let project = bench.project.path().to_path_buf();
    bench.make_a_repo()?;

    let seen = Arc::new(Seen::default());
    let recorder = Delivered::default();
    let report = bench
        .go(
            &WORKFLOW.replace(HOLE, BRANCHING),
            fake_drivers(Arc::clone(&seen)),
            &recorder,
        )
        .await?;

    // Bieg musi naprawdę pójść lewą gałęzią w pierwszej próbie i prawą w drugiej — bez tego
    // wszystko niżej mierzy zupełnie inny kształt niż ten, o który chodzi.
    assert_eq!(
        (seen.times(LEFT), seen.times(RIGHT)),
        (1, 1),
        "each branch has to work in exactly one try for this to be the shape under test: the \
         left one in try 1, the right one in try 2. It was left {} and right {}. The run said: \
         {}",
        seen.times(LEFT),
        seen.times(RIGHT),
        recorder.text()
    );

    // (a) STEROWNIK KROKU SKŁADAJĄCEGO NIE ZOSTAŁ WYWOŁANY ANI RAZU. Odmowa po starcie procesu
    //     jest już zapłacona, a agent zdążyłby przeczytać kopię złożoną z dwóch różnych prób —
    //     czyli z kodu, którego nikt nie napisał razem.
    assert_eq!(
        seen.times(JOIN),
        0,
        "the step below reached the driver even though one branch above it is still holding the \
         work of the first try while the other did its work in the second. A folder folded out \
         of two different tries reads exactly like a folder everybody agreed on, and the step \
         below finishes green over it"
    );

    // (b) I SKOŃCZYŁ SIĘ JAKO NIEUDANY, a nie po cichu pominięty.
    assert_eq!(
        report.steps.last(),
        Some(&StepState::Failed),
        "the step that folds the two branches could not start, so that is what the run has to \
         report for it. It reported {:?}",
        report.steps
    );

    // (c) CZŁOWIEK CZYTA, KTÓRE DWA KAFELKI I KTÓRE DWIE PRÓBY — w wierszu, który naprawdę
    //     wyszedł kanałem do okna, na wierszu TEGO kafelka (niezmiennik 29).
    let said = recorder.problem_from(JOIN_NAME).ok_or_else(|| {
        format!(
            "nothing the window got is a problem on the row of \"{JOIN_NAME}\". A refusal only \
             the returned value knows about leaves the person with a step that stopped and no \
             sentence anywhere. The run said: {}",
            recorder.text()
        )
    })?;
    assert!(
        said.contains(LEFT_NAME) && said.contains(RIGHT_NAME),
        "the sentence has to name BOTH copies the way the canvas names them, or the person is \
         told two folders do not match and has to work out which two. It said: {said}"
    );
    assert!(
        said.contains("try 1 of 2") && said.contains("try 2 of 2"),
        "the sentence has to say which try each copy is holding. Without the two numbers the \
         person reads that something is out of step and has nothing to go and look at. It said: \
         {said}"
    );
    assert!(
        said.contains("Nothing was overwritten"),
        "stopping the step is only allowed because both copies stay exactly where they were, so \
         the sentence has to say it — otherwise the person's first move is to look for what was \
         lost. It said: {said}"
    );

    // (d) OBIE KOPIE SĄ DALEJ OSIĄGALNE, i to jest cała cena za zatrzymanie kroku poniżej.
    let mine = format!("loadout/{}/", report.id);
    let branches = isolate::branches_under(&project, &mine);
    let left_branch = format!("{mine}{LEFT}");
    let right_branch = format!("{mine}{RIGHT}");
    assert!(
        branches.contains(&left_branch) && branches.contains(&right_branch),
        "both branches' work has to stay reachable after the refusal; that is the whole trade \
         for stopping the step below. Found: {branches:?}"
    );
    assert_eq!(
        git(&project, &["show", &format!("{left_branch}:{LEFT_FILE}")])?,
        LEFT_WROTE,
        "the left branch's own {LEFT_FILE} is not on its branch, so there is nothing for the \
         person to open"
    );
    assert_eq!(
        git(&project, &["show", &format!("{right_branch}:{RIGHT_FILE}")])?,
        RIGHT_WROTE,
        "the right branch's own {RIGHT_FILE} is not on its branch"
    );

    // (e) A DO KOPII SKŁADANEJ NIE POSZEDŁ ANI JEDEN BAJT PRACY RODZICÓW. Folder, w którym coś
    //     stanęło, wychodzi z biegu jako gałąź; folder nietknięty nie zostawia po sobie nic
    //     (`isolate::finish`), więc gałąź kroku składającego jest tu miarą zapisu.
    assert!(
        !branches.contains(&format!("{mine}{JOIN}")),
        "the folded folder came out of the run carrying work, so the refusal happened after the \
         first write. The whole point of answering before the first read is that this folder \
         stays exactly as the run laid it out. Found: {branches:?}"
    );

    Ok(())
}

// ── kryterium 4 ────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_copies_from_the_same_try_still_fold_into_one() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.make_a_repo()?;

    let seen = Arc::new(Seen::default());
    let recorder = Delivered::default();
    let report = bench
        .go(
            &WORKFLOW.replace(HOLE, "[]"),
            fake_drivers(Arc::clone(&seen)),
            &recorder,
        )
        .await?;

    // Ten sam graf, tylko bez warunków na strzałkach: obie gałęzie pracują w KAŻDEJ próbie, więc
    // obie kopie trzymają próbę drugą i nie ma o co się nie zgadzać.
    let looked = seen.snapshot();
    let join = looked.get(JOIN).ok_or(
        "the step below the two branches never reached the driver, even though both of them did \
         their work in the same try. An implementation that refuses whatever it is given passes \
         the first test in this file and deletes physical fan-in on the way",
    )?;

    assert_eq!(
        join.left.as_deref(),
        Some(LEFT_WROTE),
        "the step below read {LEFT_FILE} as {:?}. It has to see what the branch above it really \
         wrote, or it is signing off on code nobody changed",
        join.left
    );
    assert_eq!(
        join.right.as_deref(),
        Some(RIGHT_WROTE),
        "the step below did not find {RIGHT_FILE}, a file the other branch made in a folder that \
         did not exist before. Git does not track it, so an implementation that carries only \
         tracked changes loses it in silence. It read {:?}",
        join.right
    );
    assert_eq!(
        report.steps.last(),
        Some(&StepState::Succeeded),
        "the folding step had two copies from the same try, so it had to run and finish. The run \
         reported {:?} and said: {}",
        report.steps,
        recorder.text()
    );

    Ok(())
}

// ── co dubler zobaczył ─────────────────────────────────────────────────────────────────────

/// Co jeden krok zastał w swoim katalogu roboczym, odczytane w chwili wejścia do sterownika.
#[derive(Debug, Default, Clone)]
struct Look {
    left: Option<String>,
    right: Option<String>,
}

fn look_at(cwd: &Path) -> Look {
    Look {
        left: fs::read_to_string(cwd.join(LEFT_FILE)).ok(),
        right: fs::read_to_string(cwd.join(RIGHT_FILE)).ok(),
    }
}

/// Co zobaczył każdy krok i ile razy tam wszedł.
#[derive(Debug, Default)]
struct Seen(Mutex<BTreeMap<String, (usize, Look)>>);

impl Seen {
    /// Zapisuje wejście i oddaje, KTÓRE to było z kolei. Numer próby jest jedyną rzeczą, po
    /// której dubler odróżnia pierwszą rundę pętli od drugiej: `RunSpec` rundy nie niesie.
    fn record(&self, step: &str, look: Look) -> usize {
        let mut rows = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let row = rows.entry(step.to_owned()).or_insert((0, look));
        row.0 += 1;
        row.0
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
    if prompt.contains("step pick") {
        PICK
    } else if prompt.contains("step left") {
        LEFT
    } else if prompt.contains("step right") {
        RIGHT
    } else if prompt.contains("step judge") {
        JUDGE
    } else if prompt.contains("step join") {
        JOIN
    } else {
        "a step this test cannot name"
    }
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers(seen: Arc<Seen>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { seen });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler, który NAPRAWDĘ pisze i czyta w `spec.cwd`.
///
/// Dubler oddający same zdarzenia przeszedłby te asercje na implementacji, która nie zakłada ani
/// nie składa żadnego katalogu.
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
        let step = which_step(&spec.prompt);
        // Odczyt PRZED zapisem: inaczej krok meldowałby własną zmianę jako to, co zastał.
        let nth = self.seen.record(step, look_at(&spec.cwd));

        let said = match step {
            // Wejście pętli zostawia po sobie ślad, żeby sędzia miał co sądzić — bez tego pętla
            // domyka się w pierwszej próbie i drugiej nie ma wcale.
            PICK => {
                fs::write(spec.cwd.join(PICKED_FILE), format!("try {nth}"))?;
                if nth == 1 {
                    "branch: left"
                } else {
                    "branch: right"
                }
            }
            LEFT => {
                fs::write(spec.cwd.join(LEFT_FILE), LEFT_WROTE)?;
                ""
            }
            RIGHT => {
                // Katalog, którego w projekcie nie ma, i plik, o którym git nie wie.
                fs::create_dir_all(spec.cwd.join(RIGHT_FILE).parent().unwrap_or(&spec.cwd))?;
                fs::write(spec.cwd.join(RIGHT_FILE), RIGHT_WROTE)?;
                ""
            }
            // Pierwsza próba nie przechodzi, więc pętla idzie po drugą; druga przechodzi, żeby
            // bieg skończył się na pracy, a nie na wyczerpaniu prób.
            JUDGE => {
                if nth == 1 {
                    "outcome: fail"
                } else {
                    "outcome: pass"
                }
            }
            _ => "",
        };

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
            said: said.to_owned(),
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    /// Co ten krok oddaje. Stąd bierze się i wybór gałęzi, i werdykt pętli.
    said: String,
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
            text: self.said.clone(),
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

    /// Cały bieg, od dysku po pompę.
    async fn go(
        &self,
        workflow: &str,
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
            workflow: self.workflow("two-tries", workflow)?,
            how_many_at_once: 2,
            task: None,
            part: None,
            handoffs_from: None,
        };

        let (sink, source) = line_channel(QUEUE_CAP);
        let pump = spawn_pump(source, recorder.channel());
        let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
            .await
            .map_err(|_| "the run never came back")?
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

    /// Zdanie o kłopocie, które okno dostało NA WIERSZU tego kafelka.
    ///
    /// Po `agent`, nie po treści: kryterium ma świecić na wierszu, który człowiek otworzy, a nie
    /// na dowolnej linii biegu, w której akurat padły te same słowa.
    fn problem_from(&self, agent: &str) -> Option<String> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter_map(|batch| batch.as_array())
            .flatten()
            .filter(|line| line.get("kind").and_then(serde_json::Value::as_str) == Some("problem"))
            .filter(|line| line.get("agent").and_then(serde_json::Value::as_str) == Some(agent))
            .find_map(|line| {
                line.get("text")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
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
