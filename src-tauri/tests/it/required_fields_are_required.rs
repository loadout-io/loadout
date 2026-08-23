//! AC-4 dla T-90: pola przekazania oznaczone jako wymagane są wymagane naprawdę.
//!
//! # Po co to istnieje
//!
//! `Handover::Form { fields }` jest w schemacie kroku od T3 §3.1, czyta go import, ma nawet
//! własne pole `required` — i jedyne użycie w całym drzewie to `Handover::default()`. Człowiek
//! opisuje, co ten krok ma oddać, plik to zapisuje, i **agent nigdy się o tym nie dowiaduje**.
//! Nie ma też jak zauważyć braku: odpowiedź bez umówionego pola wygląda dokładnie jak
//! odpowiedź, w której akurat nie było co w nie wpisać.
//!
//! # Dwie połowy, i żadna sama nie wystarcza
//!
//! **Powiedzieć.** Wymaganie, o którym agent nie wie, jest karą, nie umową — dokładnie jak limit
//! czasu, o którym do 2026-08-23 wiedział wyłącznie ten, kto zabija krok. Więc lista pól,
//! z opisami, wchodzi do bloku „jak odpowiadać" (T-86), czyli tam, gdzie stoi reszta tego, czego
//! Loadout od agenta oczekuje.
//!
//! **Wyegzekwować.** Prośba bez skutku jest poleceniem bez handlera (niezmiennik 16) i uczy
//! model, że tych wierszy można nie pisać. Odpowiedź bez wymaganego pola czyni krok
//! **nieprzeszłym** — tą samą drogą, którą przechodzi każda inna porażka kroku
//! (`Live::when_this_one_fails`), więc ustawienie „jedź dalej mimo wszystko" działa tu tak samo
//! jak wszędzie.
//!
//! # Trzy słabe wersje tego kryterium
//!
//! **„Prompt wymienia pola".** Przechodzi dla bloku, który wymienia je i niczego nie egzekwuje —
//! czyli dla dzisiejszego stanu z dopisanym akapitem. Dlatego niżej biegną obok siebie krok,
//! który pola nie oddał, i krok, który je oddał, i muszą skończyć **inaczej**.
//!
//! **„Krok padł".** Przechodzi dla implementacji, która obcina każdą odpowiedź bez `klucz:
//! wartość` — czyli także wtedy, gdy nikt żadnego pola nie zamawiał. Dlatego trzeci krok tej
//! ławki nie ma formularza, odpowiada bez ani jednego takiego wiersza i ma przejść.
//!
//! **„Odpowiedź ma wszystkie pola".** Pole bez `required` wolno pominąć i to jest cała różnica
//! między formularzem a listą życzeń. Krok, który oddał `risk` i nie oddał `notes`, przechodzi.
//!
//! # Jedna składnia, dwa mechanizmy
//!
//! To jest ten sam wiersz `klucz: wartość`, który czyta `remember_handoff_evidence`, wybierając
//! warunkową drogę za krokiem. Dlatego blok ma pokazać agentowi kształt, który tamten czytnik
//! naprawdę bierze: **cały wiersz**, zaczynający się od nazwy pola i dwukropka. Lista wypisana
//! myślnikami („- risk — największe ryzyko") jest dla tamtego czytnika niewidzialna, więc
//! kryterium pyta o wiersz, a nie o samo wystąpienie słowa.

// `expect()`/`unwrap()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::too_many_lines)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use serde_json::Value as Json;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera. Nie „claude" i nie „codex": tamte dwie mają w biegu własne
/// wymagania co do prywatnych dowodów, a to kryterium sądzi prompt i wynik kroku.
const VENDOR: &str = "fake";

/// Ile czekamy, zanim uznamy bieg za zawieszony. Bieg, który wisi, jest dla bramki „nie
/// uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(30);

/// Pole wymagane i jego opis, słowo w słowo z pliku workflow.
const REQUIRED: &str = "risk";
const REQUIRED_SAYS: &str = "the biggest thing that could still go wrong";

/// Pole, którego wolno nie oddać.
const OPTIONAL: &str = "notes";
const OPTIONAL_SAYS: &str = "anything else worth keeping";

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-00000000093a
name: Scout
summary: Does the work
color: moss
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

/// Trzy kroki w łańcuchu. Dwa mają ten sam formularz i oddają co innego, trzeci nie ma go wcale.
///
/// Łańcuch, nie luźne kafelki: dwa kroki, które mogą biec równocześnie w folderze projektu, są
/// odmową przed pierwszym procesem (niezmiennik 12).
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_required_fields_are_required",
  "name": "One step without a form and two with one",
  "steps": [
    {
      "kind": "agent",
      "id": "s_free",
      "name": "Free",
      "agent": "01990000-0000-7000-8000-00000000093a",
      "overrides": {},
      "instructions": "free: do the work and say what you found.",
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_missing",
      "name": "Missing",
      "agent": "01990000-0000-7000-8000-00000000093a",
      "overrides": {},
      "instructions": "missing: do the work and say what you found.",
      "handover": {
        "fields": [
          {
            "name": "risk",
            "describe": "the biggest thing that could still go wrong",
            "required": true
          },
          { "name": "notes", "describe": "anything else worth keeping" }
        ]
      },
      "at": { "x": 0, "y": 240 }
    },
    {
      "kind": "agent",
      "id": "s_present",
      "name": "Present",
      "agent": "01990000-0000-7000-8000-00000000093a",
      "overrides": {},
      "instructions": "present: do the work and say what you found.",
      "handover": {
        "fields": [
          {
            "name": "risk",
            "describe": "the biggest thing that could still go wrong",
            "required": true
          },
          { "name": "notes", "describe": "anything else worth keeping" }
        ]
      },
      "at": { "x": 0, "y": 480 }
    }
  ],
  "links": [
    { "from": "s_free", "to": "s_missing" },
    { "from": "s_missing", "to": "s_present" }
  ]
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_missing_required_field_makes_the_step_not_pass() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let hand = bench.agent("scout", HAND_FILE)?;
    let workflow = bench.workflow("required-fields", WORKFLOW)?;
    the_fixture_can_run(&workflow, &[&hand])?;

    let seen = Arc::new(Seen::default());
    let report = run_it(&bench, workflow, Arc::clone(&seen)).await?;
    let prompts = seen.snapshot();
    let told: Vec<&String> = prompts.keys().collect();
    assert_eq!(
        told.len(),
        3,
        "all three steps have to reach the agent app, or the assertions below are about work \
         that never happened. It entered for {told:?}"
    );

    // ── (a) AGENT DOWIADUJE SIĘ, CZEGO SIĘ OD NIEGO CHCE ────────────────────────────────────
    // Pierwsza asercja, bo wymaganie, o którym agent nie wie, jest karą, a nie umową — i wtedy
    // wszystko niżej mierzy wyłącznie to, jak dobrze model zgaduje.
    let asked = prompts
        .get("missing")
        .cloned()
        .ok_or("the step with a form never reached the agent app")?;
    for (field, says) in [(REQUIRED, REQUIRED_SAYS), (OPTIONAL, OPTIONAL_SAYS)] {
        assert!(
            asked.contains(says),
            "the step was never told what \"{field}\" is for. The person wrote that description \
             so the agent would fill the field with the right thing; a name with no description \
             is a question the agent has to guess at. Its prompt was: {asked:?}"
        );
    }

    // ── (b) I W KSZTAŁCIE, KTÓRY NASZ WŁASNY CZYTNIK NAPRAWDĘ BIERZE ───────────────────────
    // Cały wiersz, zaczynający się nazwą pola i dwukropkiem: to jest ta sama składnia, po której
    // warunkowa droga za krokiem wybiera gałąź. Lista wypisana myślnikami jest dla tamtego
    // czytnika niewidzialna, a wygląda w prompcie równie porządnie.
    for field in [REQUIRED, OPTIONAL] {
        assert!(
            starts_a_line(&asked, field),
            "the block never shows \"{field}\" the way it has to come back: on a line of its \
             own, beginning with the name and a colon. An agent copies the shape it is shown, \
             and a shape our own reader does not take is an agreement one side never signed. \
             Its prompt was: {asked:?}"
        );
    }

    // ── (c) BRAK WYMAGANEGO POLA CZYNI KROK NIEPRZESZŁYM ──────────────────────────────────
    let states = by_name(&report)?;
    assert_eq!(
        states.get("Missing").map(|one| one.0),
        Some(StepState::Failed),
        "a step whose form requires \"{REQUIRED}\" answered without it and was taken as done. \
         A request with no consequence is an instruction with nothing behind it (invariant 16), \
         and it teaches the model that these lines are optional everywhere. The run ended as {:?}",
        report.steps
    );
    let why = states
        .get("Missing")
        .and_then(|one| one.1.clone())
        .unwrap_or_default();
    assert!(
        why.contains(REQUIRED),
        "the step was marked as not passed without saying which field was missing: {why:?}. \
         With two fields on the form, a person reading that has to open the answer and compare \
         it against the workflow by eye — and the sentence is the only place that knows"
    );

    // ── (d) A POLE BEZ „WYMAGANE" WOLNO POMINĄĆ ───────────────────────────────────────────
    assert_eq!(
        states.get("Present").map(|one| one.0),
        Some(StepState::Succeeded),
        "a step that gave back \"{REQUIRED}\" and left out \"{OPTIONAL}\" did not pass. Only one \
         of those two was marked as needed, and a form that demands everything is a form on \
         which marking a field means nothing. The run ended as {:?}",
        report.steps
    );

    // ── (e) KONTROLA: KROK BEZ FORMULARZA NIE MA CZEGO ODDAWAĆ ───────────────────────────
    // Bez tej asercji wszystko wyżej przechodzi dla implementacji, która wymaga wiersza
    // `klucz: wartość` od KAŻDEGO kroku — czyli czerwieni połowę biegów, o które nikt nie prosił.
    assert_eq!(
        states.get("Free").map(|one| one.0),
        Some(StepState::Succeeded),
        "a step with no form at all was judged against fields nobody asked it for. The run ended \
         as {:?}",
        report.steps
    );
    Ok(())
}

/// Czy któryś wiersz tego tekstu zaczyna się od `<field>:`.
///
/// Wiersz, nie wystąpienie: to jest dokładnie ta różnica, po której `klucz: wartość` da się
/// przeczytać z odpowiedzi, a „patrz na risk: coś tam" w środku zdania — nie.
fn starts_a_line(text: &str, field: &str) -> bool {
    let head = format!("{field}:");
    text.lines()
        .any(|line| line.trim_start().starts_with(&head))
}

/// Stan i powód każdego kroku, po nazwie z pliku workflow.
///
/// Z `run.json`, nie z samego raportu: powód porażki jest tym, co człowiek czyta na karcie
/// kroku, a `run.json` jest jedynym zapisem biegu, który przeżywa skasowanie indeksu
/// (niezmiennik 4).
fn by_name(
    report: &RunReport,
) -> Result<BTreeMap<String, (StepState, Option<String>)>, Box<dyn Error>> {
    let text = fs::read_to_string(report.dir.join("run.json"))?;
    let run: Json = serde_json::from_str(&text)?;
    let steps = run
        .get("steps")
        .and_then(Json::as_array)
        .ok_or("the run's own record describes no steps at all")?;

    let mut out = BTreeMap::new();
    for step in steps {
        let Some(name) = step.get("name").and_then(Json::as_str) else {
            continue;
        };
        let status = step
            .get("status")
            .and_then(Json::as_str)
            .ok_or("a step in the run's own record has no state")?;
        let state = match status {
            "succeeded" => StepState::Succeeded,
            "failed" => StepState::Failed,
            "cancelled" => StepState::Cancelled,
            "skipped" => StepState::Skipped,
            "running" => StepState::Running,
            "ready" => StepState::Ready,
            _ => StepState::Pending,
        };
        let why = step.get("error").and_then(Json::as_str).map(str::to_owned);
        out.insert(name.to_owned(), (state, why));
    }
    Ok(out)
}

/// Fikstura ma przejść walidator **bez ani jednego problemu**, a jej plik agenta ma dać się
/// przeczytać.
///
/// To nie jest część kryterium, tylko jego przesłanka, i dlatego stoi przed biegiem. Czerwień
/// w fazie kontraktu wygląda identycznie dla „zachowania jeszcze nie ma" i dla „tego kryterium
/// nie da się spełnić nigdy".
fn the_fixture_can_run(workflow: &Path, agents: &[&Path]) -> Result<(), Box<dyn Error>> {
    let problems: Vec<String> = check(&load(workflow)?)
        .into_iter()
        .filter(|note| note.level == Level::Problem)
        .map(|note| note.message)
        .collect();
    assert!(
        problems.is_empty(),
        "the fixture would be refused before it ran, so this criterion could never pass: \
         {problems:?}"
    );
    for agent in agents {
        read_agent_file(agent).map_err(|error| format!("{}: {error}", agent.display()))?;
    }
    Ok(())
}

async fn run_it(
    bench: &Bench,
    workflow: PathBuf,
    seen: Arc<Seen>,
) -> Result<RunReport, Box<dyn Error>> {
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(seen),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };
    // Okno jest tu czarną dziurą: to kryterium sądzi prompt i wynik kroku, nie wiersze.
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))??;
    let _ = tokio::time::timeout(PATIENCE, pump).await;
    Ok(report)
}

// ── co dubler zobaczył ─────────────────────────────────────────────────────────────────────

/// Prompt, który dojechał do sterownika, po etykiecie kroku.
#[derive(Debug, Default)]
struct Seen(Mutex<BTreeMap<String, String>>);

impl Seen {
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym wywołaniu,
    /// więc nie ma wyrażenia, w którym dożyłby do `await`.
    fn record(&self, label: &str, prompt: String) {
        self.lock().entry(label.to_owned()).or_insert(prompt);
    }

    fn snapshot(&self) -> BTreeMap<String, String> {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, String>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Etykieta kroku: to, co stoi przed pierwszym dwukropkiem instrukcji — `RunSpec` nazwy kroku
/// nie niesie.
fn label_of(prompt: &str) -> String {
    prompt
        .split_once(':')
        .map_or_else(|| prompt.to_owned(), |(head, _)| head.trim().to_owned())
}

/// Co ten krok odpowiada.
///
/// Trzy odpowiedzi, po jednej na przypadek: bez wymaganego pola, z wymaganym i bez opcjonalnego,
/// oraz bez ani jednego wiersza `klucz: wartość` — ten ostatni od kroku, który o żadne pole nie
/// był proszony.
fn answer_from(label: &str) -> String {
    match label {
        "missing" => {
            "## Answer\nThe work is done.\n\n## Evidence\nsrc/main.rs:1\n\n## Open\nnothing.\n"
                .to_owned()
        }
        "present" => format!(
            "## Answer\nThe work is done.\n\n{REQUIRED}: the second row may be wrong.\n\n## Evidence\nsrc/main.rs:1\n\n## Open\nnothing.\n"
        ),
        other => format!("## Answer\n{other} did the work.\n\n## Open\nnothing.\n"),
    }
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers(seen: Arc<Seen>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { seen });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler sterownika: zatrzymuje prompt i oddaje odpowiedź pasującą do kroku.
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
        let label = label_of(&spec.prompt);
        self.seen.record(&label, spec.prompt.clone());

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
            said: answer_from(&label),
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    said: String,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<loadout_lib::engine::supervisor::GroupId> {
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        // `ok: true`, i to jest treść tej ławki: agent skończył turę bez błędu. Jedyne, czego
        // zabrakło, to umówione pole — a dziś nikt o nie nie pyta.
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: self.said.clone(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::ZERO,
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send((AgentEvent::Finished(outcome.clone())).into())
            .await;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> loadout_lib::engine::supervisor::GroupProof {
        loadout_lib::engine::supervisor::GroupProof::Dead { status: None }
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
        // `Store::open` zakłada plik bazy, ale nie katalog nad nim.
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(project.path().join("notes.txt"), "written by the human")?;
        Ok(Self { home, project })
    }

    fn agent(&self, slug: &str, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.home.path().join("agents").join(format!("{slug}.md"));
        fs::write(&path, text)?;
        Ok(path)
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
