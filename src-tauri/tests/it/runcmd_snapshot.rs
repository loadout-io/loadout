//! AC-5 dla T-15: `run.json` pamięta, co **naprawdę** biegło, także po późniejszej edycji agenta.
//!
//! To jest jedyna odpowiedź na pytanie „dlaczego zeszłotygodniowy bieg zachował się inaczej"
//! [T4 §5.2 p. 3, §10 ryzyko 1]. Migawka konfiguracji **efektywnej** — agent złożony z nadpisaniami
//! kroku (`library::agents::resolve`) — zamrożona w chwili startu jest jedynym miejscem, w którym
//! ta odpowiedź może stać.
//!
//! **Słaba wersja brzmi `assert!(run_json.contains("Forge"))` albo asercja, że jest tam `agentId`.**
//! Obie przechodzą dla migawki będącej **referencją**, a wtedy po każdej edycji szablonu historia
//! biegów po cichu zaczyna opowiadać o sobie coś innego. Rozróżnia je edycja pliku agenta **po**
//! biegu i asercja, że `run.json` dalej pokazuje starą wartość `model`.
//!
//! Do tego druga strona tej samej monety, bez której „stara wartość" da się przejść na sztywno
//! wpisanym `opus`: **kolejny bieg, po edycji, ma pokazać nową**. Migawka ma być czytana przy
//! starcie, a nie zapamiętana raz w kodzie.
//!
//! `workflow_hash` odpowiada na drugą połowę pytania — „czy to był ten sam plan". Testujemy go
//! zachowaniem, nie kształtem: dwa biegi tego samego pliku mają dać ten sam hash, a bieg pliku
//! poprawionego — inny. Asercja na konkretną wartość wymagałaby przepisania algorytmu do tego
//! pliku, czyli drugiej jego kopii (niezmiennik 20).

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::{Overrides, Thinking, read_agent_file, resolve};
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use serde_json::Value as Json;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony.
const PATIENCE: Duration = Duration::from_secs(10);

/// Identyfikator agenta — stabilny przez zmianę nazwy i dlatego to **on**, a nie nazwa, stoi
/// w kroku workflow (T3 §3.1: `agent: id of a saved Agent`).
const FORGE_ID: &str = "01990000-0000-7000-8000-0000000000f0";

/// Forge, tak jak wygląda **w chwili biegu**: `model: opus`, `thinking: balanced`.
const FORGE_OPUS: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000f0
name: Forge
summary: Writes code
color: clay
runsWith: claude-code
model: opus
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 20
writeResultsTo: handoffs/build.md
tools: everything
skills: []
connections: []
---
Write the smallest change that makes the checks pass.
";

/// Ten sam agent po edycji szablonu: `model: sonnet`. Nic poza modelem się nie zmienia.
const FORGE_SONNET: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000f0
name: Forge
summary: Writes code
color: clay
runsWith: claude-code
model: sonnet
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 20
writeResultsTo: handoffs/build.md
tools: everything
skills: []
connections: []
---
Write the smallest change that makes the checks pass.
";

/// Jeden krok, który nadpisuje agentowi **wyłącznie** `thinking`.
const PLAN: &str = r#"{
  "format": 1,
  "id": "wf_ship_it",
  "name": "Ship it",
  "steps": [
    {
      "kind": "agent",
      "id": "s_forge",
      "name": "Forge",
      "agent": "01990000-0000-7000-8000-0000000000f0",
      "overrides": { "thinking": "deep" },
      "instructions": "forge",
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

/// Ten sam graf po ręcznej poprawce. Różni się jednym słowem — i to wystarcza, żeby przestał
/// być tym samym planem.
const PLAN_EDITED: &str = r#"{
  "format": 1,
  "id": "wf_ship_it",
  "name": "Ship it carefully",
  "steps": [
    {
      "kind": "agent",
      "id": "s_forge",
      "name": "Forge",
      "agent": "01990000-0000-7000-8000-0000000000f0",
      "overrides": { "thinking": "deep" },
      "instructions": "forge",
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_run_file_freezes_the_config_that_actually_ran() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let forge = bench.agent("forge", FORGE_OPUS)?;
    let workflow = bench.workflow("ship-it", PLAN)?;
    the_fixture_can_run(&workflow, &forge)?;
    let store = Store::open(&bench.db())?;

    let first = one_run(&bench, &store, &workflow).await?;

    // Szablon zmienia się PO biegu — dokładnie tak, jak zmienia się w życiu.
    bench.agent("forge", FORGE_SONNET)?;
    let second = one_run(&bench, &store, &workflow).await?;

    // A teraz zmienia się sam plan.
    fs::write(&workflow, PLAN_EDITED)?;
    let third = one_run(&bench, &store, &workflow).await?;

    let frozen = run_file(&first.dir)?;
    let effective = snapshot_of(&frozen)?;

    assert_eq!(
        text_at(effective, "model")?,
        "opus",
        "the agent was edited to `sonnet` after the run, and run.json now tells a different story \
         about what already happened. A snapshot that is a reference leaves \"why did last week's \
         run behave differently?\" unanswerable after any template edit [T4 §10, risk 1]"
    );
    assert_eq!(
        text_at(effective, "thinking")?,
        "deep",
        "the step overrode `thinking`, so the frozen config is the EFFECTIVE one — agent plus the \
         step's overrides (library::agents::resolve), not the agent as saved"
    );
    assert_eq!(
        text_at(effective, "id")?,
        FORGE_ID,
        "the snapshot has to name which agent this was; a name alone changes when someone renames \
         the agent, and then the run points at nothing"
    );

    let after_edit = run_file(&second.dir)?;
    assert_eq!(
        text_at(snapshot_of(&after_edit)?, "model")?,
        "sonnet",
        "the run that started after the edit still reports `opus`, so the snapshot is not read at \
         start — it is written into the code. Then it freezes the wrong thing forever"
    );

    the_hash_answers_whether_it_was_the_same_plan(&frozen, &after_edit, &run_file(&third.dir)?)?;
    Ok(())
}

/// `workflow_hash`: ten sam plan → ten sam hash, poprawiony plan → inny.
fn the_hash_answers_whether_it_was_the_same_plan(
    first: &Json,
    second: &Json,
    third: &Json,
) -> Result<(), Box<dyn Error>> {
    let (before, again, edited) = (hash_of(first)?, hash_of(second)?, hash_of(third)?);

    assert!(
        !before.is_empty(),
        "run.json carries an empty workflow_hash, which answers \"was it the same plan?\" with \
         silence"
    );
    assert_eq!(
        before, again,
        "two runs of the SAME workflow file came back with different hashes, so the hash cannot \
         tell \"same plan\" from \"different plan\" — it is a fresh number per run"
    );
    assert_ne!(
        before, edited,
        "the workflow file was edited between the runs and the hash did not move; a hash that \
         never changes is a constant with a hash's name"
    );
    Ok(())
}

/// `run.json` z katalogu biegu.
fn run_file(dir: &Path) -> Result<Json, Box<dyn Error>> {
    let text = fs::read_to_string(dir.join("run.json"))
        .map_err(|error| format!("{}/run.json could not be read: {error}", dir.display()))?;
    Ok(serde_json::from_str(&text)?)
}

/// Migawka konfiguracji efektywnej pierwszego kroku.
fn snapshot_of(run: &Json) -> Result<&Json, Box<dyn Error>> {
    run.get("steps")
        .and_then(Json::as_array)
        .and_then(|steps| steps.first())
        .and_then(|step| step.get("effective"))
        .ok_or_else(|| {
            "run.json has no steps[0].effective — the effective config frozen at start is the one \
             place a copy is correct [T4 §5.2 p. 3]"
                .into()
        })
}

/// Hash pliku workflow, którym ten bieg poszedł.
fn hash_of(run: &Json) -> Result<&str, Box<dyn Error>> {
    run.get("workflow_hash")
        .and_then(Json::as_str)
        .ok_or_else(|| {
            "run.json has no workflow_hash, so \"was it the same plan?\" has no answer".into()
        })
}

/// Tekstowa wartość klucza migawki.
fn text_at<'a>(value: &'a Json, key: &str) -> Result<&'a str, Box<dyn Error>> {
    value
        .get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| format!("the snapshot has no `{key}` to read; it says {value}").into())
}

/// Jeden bieg jednokrokowego planu.
async fn one_run(
    bench: &Bench,
    store: &Store,
    workflow: &Path,
) -> Result<RunReport, Box<dyn Error>> {
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store,
        drivers: fake_drivers(),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: workflow.to_path_buf(),
        how_many_at_once: 1,
        task: None,
    };

    // 2026-08-17 (T-30) — bieg oddaje linie POJEDYNCZO do `LineSink`, a sklejaniem zajmuje się
    // pompa po drugiej stronie, więc kanał zakłada się tutaj tak, jak zakłada go komenda:
    // `line_channel` + `spawn_pump`. Zmieniła się wyłącznie konstrukcja kanału przy wywołaniu;
    // ani jedna asercja tego kryterium nie wie o tej zmianie, bo sądzi ono `run.json`, a nie
    // wiersze. Kanał do okna jest czarną dziurą z tego samego powodu.
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    // Pompa kończy się sama, kiedy zniknie ostatni nadajnik — a ten ginie razem z powrotem
    // biegu. Czekanie na nią zostaje w `join!`, bo poprzednie osuszanie kanału stało dokładnie
    // tutaj i tak samo domykało bieg.
    let drain = async move {
        let _ = pump.await;
    };

    let (ran, ()) = tokio::time::timeout(PATIENCE, async {
        tokio::join!(run_workflow_inner(&deps, &request, sink), drain)
    })
    .await
    .map_err(|_| format!("the run did not finish within {PATIENCE:?}"))?;

    let report = ran?;
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded],
        "the one step has to finish for its snapshot to describe anything; it ended as {:?}",
        report.steps
    );
    Ok(report)
}

/// Przesłanka kryterium, nie kryterium: fikstura ma przejść walidator, jej plik agenta ma dać się
/// przeczytać, a złożenie agenta z nadpisaniem kroku ma **dawać to, czego kryterium wymaga od
/// migawki**.
///
/// Czerwień w fazie kontraktu wygląda identycznie dla „zachowania jeszcze nie ma" i dla „tego
/// kryterium nie da się spełnić nigdy". Gdyby `resolve` nie dawało tu `opus` i `deep`, żadna
/// implementacja nie miałaby czego zapisać do `run.json`, a test nazywałby to brakiem zachowania.
fn the_fixture_can_run(workflow: &Path, agent: &Path) -> Result<(), Box<dyn Error>> {
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

    let forge = read_agent_file(agent).map_err(|error| format!("{}: {error}", agent.display()))?;
    let overrides: Overrides = serde_json::from_value(serde_json::json!({ "thinking": "deep" }))?;
    let resolved = resolve(&forge, &overrides)?;
    assert_eq!(
        resolved.agent.model, "opus",
        "the fixture's agent has to run on `opus`, or the frozen value proves nothing"
    );
    assert_eq!(
        resolved.agent.thinking,
        Thinking::Deep,
        "the step's override has to win over the agent's `balanced`, or the criterion is asking \
         for a value nothing produces"
    );
    Ok(())
}

/// Biblioteka użytkownika i projekt na czas jednego kryterium.
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

/// Fabryka, która dla każdego vendora oddaje ten sam dubler.
fn fake_drivers() -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake);
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler sterownika: trzy zdarzenia na krok i wyjście zerem, natychmiast.
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
        let _ = events
            .send(
                (AgentEvent::Said {
                    text: format!("working on {}", spec.prompt),
                })
                .into(),
            )
            .await;

        Ok(Box::new(Turn { events, session }))
    }
}

/// Jedna tura dublera.
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
            took: Duration::ZERO,
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
