//! AC-1 dla T-56: łańcuch trzech kroków pracuje w JEDNYM drzewie roboczym, a krok `same-copy`
//! bez poprzednika jest odmową, która nazywa go po nazwie z kafelka.
//!
//! Harness ma jedno drzewo robocze repo: pisze w nim implementacja, potem sprawdzenie, potem
//! druga opinia, potem poprawka. `Folder` nie umiał tego powiedzieć, więc wybór był między dwoma
//! kłamstwami: `project` (poprawka pisze po plikach człowieka) albo `fresh-copy` (każdy krok
//! dostaje WŁASNE drzewo, więc poprawka nie widzi kodu, który ma poprawić). Oba warianty
//! **kończą się sukcesem** — agent dostał folder, coś w nim napisał, krok jest zielony — więc
//! nikt nie zgłosi biegu, w którym recenzent czytał nie ten kod (`docs/harness-as-workflow.md`,
//! ustalenie U-2).
//!
//! # Słaba wersja tego kryterium
//!
//! Samo `assert_eq!(cwd_one, cwd_three)`. Przechodzi dla implementacji sprowadzającej `same-copy`
//! do `project`: trzy kroki w folderze projektu też są „jednym katalogiem", a plik napisany przez
//! pierwszy jest widoczny dla trzeciego — tylko z całkowicie złego powodu. To jest dokładnie ta
//! implementacja, którą to kryterium ma odrzucić, bo kafelek mówi „to samo drzewo", a krok pisze
//! po prawdziwych plikach człowieka. Rozróżniają ją dwie asercje: wspólny katalog musi leżeć pod
//! `work/` katalogu biegu, a katalog projektu nie ma prawa zobaczyć utworzonego pliku.
//!
//! Druga słaba wersja, w odmowie: `assert!(!notes.is_empty())`. Przechodzi na CUDZEJ uwadze, bo
//! krok bez wejść bywa wyspą i `islands()` mówi o nim swoje. Dlatego fikstura odmowy jest
//! **łańcuchem, nie wyspą**, a asercja stoi na wadze `Problem` **plus** `step_id` równym
//! identyfikatorowi tego kroku.
//!
//! Dubler sterownika CZYTA i PISZE w `spec.cwd` — dubler oddający same zdarzenia przeszedłby ten
//! test na implementacji, która nie zakłada żadnego drzewa (wzorzec z
//! `fresh_copy_isolates_steps.rs`). Katalog projektu jest tu zwykłym katalogiem, nie
//! repozytorium: AC-1 nie sądzi tego, JAK drzewo powstało — to robią kryteria T-52 — tylko ILU
//! ich jest i kto w którym pracuje.

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
use loadout_lib::workflow::Folder;
use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::check::{Level, Note, check};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera.
const VENDOR: &str = "claude-code";

/// Katalog, pod którym bieg zakłada drzewa robocze kroków.
const WORK: &str = "work";

/// Plik, który pierwszy krok ZMIENIA.
const EXISTING: &str = "notes.txt";
/// Treść, którą pierwszy krok ma zastać.
const ORIGINAL: &str = "written by the human";
/// Treść, którą pierwszy krok w to miejsce wpisuje.
const CHANGED: &str = "rewritten by the first step";
/// Plik, który pierwszy krok TWORZY.
const CREATED: &str = "made-by-step-one.txt";
/// I jego treść.
const MADE_HERE: &str = "the first step left this here";

/// Kroki po tym, czym się w tym teście przedstawiają.
const FIRST: &str = "s_one";
const SECOND: &str = "s_two";
const THIRD: &str = "s_three";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony.
const PATIENCE: Duration = Duration::from_secs(20);

/// Trzy kroki agenta w łańcuchu: pierwszy na własnym drzewie, dwa następne na tym samym.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_same_copy",
  "name": "Three steps, one working tree",
  "steps": [
    {
      "kind": "agent",
      "id": "s_one",
      "name": "First",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "step one: change and create",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_two",
      "name": "Second",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "step two: look only",
      "folder": { "use": "same-copy" },
      "at": { "x": 240, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_three",
      "name": "Third",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "step three: look only",
      "folder": { "use": "same-copy" },
      "at": { "x": 480, "y": 0 }
    }
  ],
  "links": [
    { "from": "s_one", "to": "s_two" },
    { "from": "s_two", "to": "s_three" }
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_chain_of_three_steps_works_in_one_tree() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let project = bench.project.path().to_path_buf();

    // Projekt z dwoma plikami. `EXISTING` ma treść, którą pierwszy krok ma ZASTAĆ.
    fs::write(project.join(EXISTING), ORIGINAL)?;
    fs::create_dir_all(project.join("src"))?;
    fs::write(project.join("src").join("main.rs"), "fn main() {}")?;

    let seen = Arc::new(Seen::default());
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: &project,
        store: &store,
        drivers: fake_drivers(Arc::clone(&seen)),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: bench.workflow("same-copy", WORKFLOW)?,
        // Trzy miejsca, choć łańcuch i tak idzie po kolei: limit, który wymusza kolejność,
        // schowałby implementację, w której kroki dzielą drzewo tylko dlatego, że nigdy nie
        // biegną razem.
        how_many_at_once: 3,
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
        vec![StepState::Succeeded; 3],
        "all three steps have to finish for the folder assertions to mean anything; they ended \
         as {:?}",
        report.steps
    );

    let looked = seen.snapshot();
    // (g) KONTROLA PRZECIW PUSTEMU CZYTANIU. Mniej niż trzy zameldowane katalogi znaczy, że
    //     poniższe asercje mówią o biegu mniejszym niż ten, o który poprosiliśmy — a zbiór
    //     dwóch równych ścieżek spełnia „jedno drzewo" bez uruchomienia trzeciego kroku.
    assert_eq!(
        looked.len(),
        3,
        "all three steps have to reach the driver, or this test is measuring a shorter chain \
         than the one it asked for. Saw: {:?}",
        looked.keys().collect::<Vec<_>>()
    );
    let one = looked
        .get(FIRST)
        .ok_or("the first step never reached the driver")?;
    let two = looked
        .get(SECOND)
        .ok_or("the second step never reached the driver")?;
    let three = looked
        .get(THIRD)
        .ok_or("the third step never reached the driver")?;

    // (a) JEDNO DRZEWO, NIE TRZY.
    assert_eq!(
        one.cwd, two.cwd,
        "the second step says it works in the same tree as the step before it, so it has to be \
         handed the very folder the first step worked in. It got {:?} while the first one had {:?}",
        two.cwd, one.cwd
    );
    assert_eq!(
        one.cwd, three.cwd,
        "the third step resolves through the step before it, whichever kind that step is, so \
         the whole chain lands in one folder. It got {:?} while the first one had {:?}",
        three.cwd, one.cwd
    );

    // (b) TO DRZEWO NIE JEST FOLDEREM PROJEKTU. Bez tej asercji przechodzi implementacja
    //     sprowadzająca `same-copy` do `project`: kafelek mówi „to samo drzewo", a krok pisze
    //     po prawdziwych plikach człowieka.
    assert_ne!(
        one.cwd, project,
        "the chain worked in the project folder itself. 'the same tree as the step before me' \
         has to mean the tree the run laid out, not the folder the human is editing — this is \
         the cheapest wrong implementation and the reason this assertion exists"
    );
    let trees = report.dir.join(WORK);
    assert!(
        one.cwd.starts_with(&trees),
        "the shared folder has to be the working tree this run laid out for the first step, so \
         it lives under {trees:?}. It was {:?}",
        one.cwd
    );

    // (c) PRACA PRZECHODZI DALEJ. Plik utworzony przez pierwszy krok trzeci ODCZYTUJE, a plik
    //     zmieniony ma w nim treść po zmianie. Bez pierwszej z tych trzech asercji drzewo mogłoby
    //     być puste, a wtedy dwie następne przechodzą na pliku, który powstał z niczego.
    assert_eq!(
        one.existing.as_deref(),
        Some(ORIGINAL),
        "the first step has to find the project's own files in its tree, or the two assertions \
         below would be reading files that came from nowhere. It found: {:?}",
        one.existing
    );
    assert_eq!(
        three.existing.as_deref(),
        Some(CHANGED),
        "the third step read {EXISTING} after the first step rewrote it. A step that works in \
         the same tree as the one before it has to see that text — a fresh copy each would hand \
         it the human's original, and the fix would be reviewing code nobody changed"
    );
    assert_eq!(
        three.created.as_deref(),
        Some(MADE_HERE),
        "the third step did not find {CREATED}, a file the FIRST step made. The chain is not \
         working in one tree, so a repair step would be looking at code that does not have the \
         change it is meant to repair"
    );

    // (d) KATALOG PROJEKTU TEGO NIE WIDZI.
    assert!(
        !project.join(CREATED).exists(),
        "{CREATED} appeared in the project folder. The chain shares ONE tree, and that tree is \
         not the folder the human works in — this file can only be here if the shared folder was \
         the project itself"
    );
    assert_eq!(
        fs::read_to_string(project.join(EXISTING))?,
        ORIGINAL,
        "the project file changed. Steps that share a tree still must not reach back into the \
         folder the human is editing"
    );

    Ok(())
}

// ── (e) krok „to samo drzewo" bez poprzednika ──────────────────────────────────────────────

/// Nazwa z kafelka kroku, który nie ma za sobą nikogo. Inna niż jego identyfikator, bo o to
/// w tej asercji chodzi.
const HEAD_NAME: &str = "Fix it again";
/// I jego identyfikator — ten trafia do `step_id`, a nie do zdania.
const HEAD_ID: &str = "s_head";

#[test]
fn a_same_copy_step_with_nothing_before_it_is_refused_by_name() -> Result<(), Box<dyn Error>> {
    // ŁAŃCUCH, NIE WYSPA. Krok odłączony od reszty grafu dostaje z `islands()` własne
    // ostrzeżenie i wtedy nie wiadomo, która reguła zaświeciła.
    let alone = workflow(
        &[
            step(HEAD_ID, HEAD_NAME, &same_copy()),
            step("s_tail", "Check it", &fresh_copy()),
        ],
        &[(HEAD_ID, "s_tail")],
    )?;

    let notes = check(&alone);
    let refused = problems(&notes);

    assert_eq!(
        refused.len(),
        1,
        "a step set to work in the same tree as the step before it, with nothing before it, has \
         no answer to 'which tree' — and one unanswerable step is one thing to fix. Got: {notes:?}"
    );
    assert_eq!(
        refused[0].step_id.as_deref(),
        Some(HEAD_ID),
        "the badge belongs on the step that cannot be resolved: without the id the human reads \
         a sentence and has no idea which tile it is about. Got: {:?}",
        refused[0].step_id
    );
    let said = &refused[0].message;
    assert!(
        said.contains(HEAD_NAME),
        "the refusal has to name the step the way the canvas names it. It reads: {said}"
    );
    assert!(
        !said.contains(HEAD_ID),
        "{HEAD_ID} is a key in a file, not anything the human sees on the canvas. It reads: {said}"
    );
    assert!(
        !said.contains("same-copy"),
        "'same-copy' is the key this option carries in the file; the sentence has to say what is \
         wrong in words, not quote our schema. It reads: {said}"
    );

    // TA SAMA FIKSTURA PO DOCIĄGNIĘCIU STRZAŁKI. Bez tej połowy przechodzi reguła, która odmawia
    // KAŻDEMU krokowi „to samo drzewo" — a wtedy wariant, który to zadanie dokłada, jest nie do
    // użycia i nikt się o tym nie dowie z zielonej bramki.
    let wired = workflow(
        &[
            step("s_before", "Write the code", &fresh_copy()),
            step(HEAD_ID, HEAD_NAME, &same_copy()),
            step("s_tail", "Check it", &fresh_copy()),
        ],
        &[("s_before", HEAD_ID), (HEAD_ID, "s_tail")],
    )?;

    let after = check(&wired);
    assert!(
        problems(&after).is_empty(),
        "with an arrow coming in, 'the same tree as the step before me' has an answer, so there \
         is nothing left to refuse. Got: {after:?}"
    );
    Ok(())
}

// ── (f) migracja jest addytywna ────────────────────────────────────────────────────────────

#[test]
fn a_file_from_before_this_change_still_loads() -> Result<(), Box<dyn Error>> {
    // Plik sprzed tej zmiany: trzy dotychczasowe warianty i krok BEZ klucza `folder`.
    let older: WorkflowFile = serde_json::from_value(json!({
        "format": 1,
        "id": "wf_older",
        "name": "Written by an older build",
        "steps": [
            step("s_a", "Plan", &json!({ "use": "project" })),
            step("s_b", "Build", &fresh_copy()),
            step("s_c", "Look", &json!({ "use": "pick", "path": "/Users/x/api" })),
            json!({
                "kind": "agent",
                "id": "s_d",
                "name": "No folder key at all",
                "agent": "a_forge",
                "instructions": "Do the work."
            }),
        ],
        "links": [
            { "from": "s_a", "to": "s_b" },
            { "from": "s_b", "to": "s_c" },
            { "from": "s_c", "to": "s_d" }
        ]
    }))?;

    assert_eq!(
        older.steps.len(),
        4,
        "a file written before this change has to load unchanged; a new variant that costs the \
         user their saved workflows is a migration, and migrations here are additive (invariant \
         25)"
    );
    assert_eq!(
        Folder::default(),
        Folder::Project,
        "the step with no folder key at all still means the project folder. Moving the default \
         would silently rehome every step of every saved file"
    );

    // I w drugą stronę: nowy wariant wraca na drut pod swoim kluczem, nie pod cudzym.
    assert_eq!(
        serde_json::to_value(Folder::SameCopy)?,
        json!({ "use": "same-copy" }),
        "the wire key is the file format; a variant that saves as anything else makes files this \
         build wrote unreadable to the next one"
    );
    assert_eq!(
        serde_json::from_value::<Folder>(json!({ "use": "same-copy" }))?,
        Folder::SameCopy,
        "and it has to come back as the same variant it went out as"
    );
    Ok(())
}

/// Krok o zadanym folderze. Wszystko poza folderem jest kompletne, żeby żadna inna reguła nie
/// dołożyła drugiej uwagi do fikstury, która mierzy tę jedną.
fn step(id: &str, name: &str, folder: &Value) -> Value {
    json!({
        "kind": "agent",
        "id": id,
        "name": name,
        "agent": "a_forge",
        "instructions": "Do the work.",
        "folder": folder
    })
}

fn same_copy() -> Value {
    json!({ "use": "same-copy" })
}

fn fresh_copy() -> Value {
    json!({ "use": "fresh-copy" })
}

fn workflow(steps: &[Value], links: &[(&str, &str)]) -> Result<WorkflowFile, Box<dyn Error>> {
    let links: Vec<Value> = links
        .iter()
        .map(|(from, to)| json!({ "from": from, "to": to }))
        .collect();
    Ok(serde_json::from_value(json!({
        "format": 1,
        "id": "wf_test",
        "name": "Test workflow",
        "steps": steps,
        "links": links
    }))?)
}

fn problems(notes: &[Note]) -> Vec<&Note> {
    notes
        .iter()
        .filter(|note| note.level == Level::Problem)
        .collect()
}

// ── co dubler zobaczył ─────────────────────────────────────────────────────────────────────

/// Co jeden krok zastał w swoim katalogu roboczym.
#[derive(Debug, Default, Clone)]
struct Look {
    /// Katalog, w którym ten krok naprawdę pracował.
    cwd: PathBuf,
    /// Treść `EXISTING`, jeśli plik tam był.
    existing: Option<String>,
    /// Treść `CREATED` — pliku, którego w projekcie nie ma i który zrobił pierwszy krok.
    created: Option<String>,
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
        cwd: cwd.to_path_buf(),
        existing: fs::read_to_string(cwd.join(EXISTING)).ok(),
        created: fs::read_to_string(cwd.join(CREATED)).ok(),
    }
}

/// Który krok tu wszedł. Prompt jest jedynym śladem: `RunSpec` nie niesie identyfikatora kroku,
/// a treść zadania jest tym, co ten krok naprawdę dostał.
fn which_step(prompt: &str) -> &'static str {
    if prompt.contains("step one") {
        FIRST
    } else if prompt.contains("step two") {
        SECOND
    } else if prompt.contains("step three") {
        THIRD
    } else {
        // Jeden klucz na wszystkie nierozpoznane: dwa takie kroki zejdą się w jeden wpis
        // i licznik z asercji (g) to zobaczy.
        "a step this test cannot name"
    }
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers(seen: Arc<Seen>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { seen });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler, który NAPRAWDĘ czyta i pisze w `spec.cwd`.
///
/// Dubler oddający same zdarzenia przeszedłby ten test na implementacji, która nie zakłada
/// żadnego drzewa: żeby asercja mówiła o dzieleniu katalogu, ktoś naprawdę musi tknąć dysk.
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
        // Odczyt PRZED zapisem: inaczej pierwszy krok meldowałby własną zmianę jako to, co zastał.
        self.seen.record(step, look_at(&spec.cwd));

        if step == FIRST {
            // Zmiana i utworzenie — obie w katalogu, który ten krok dostał.
            fs::write(spec.cwd.join(EXISTING), CHANGED)?;
            fs::write(spec.cwd.join(CREATED), MADE_HERE)?;
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
