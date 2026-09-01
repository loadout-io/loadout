//! AC-3 dla T-97: lista narzędzi agenta Codeksa nie zabiera biegu.
//!
//! # Po co to istnieje
//!
//! `what_this_step_may_use` sądziło **każdą** listę `Tools::Only([…])` przeciw suficie Claude'a —
//! bo sufit był stałą tego jednego adaptera (`claude::tool_surface`), a nie pytaniem do vendora.
//! Skutek jest asymetrią, której nikt nie zamawiał: człowiek wpisuje `Read, Bash` agentowi
//! Codeksa na dialu „look only", `RunError::Refused` zabiera **cały bieg** — a `CodexDriver` tej
//! listy i tak nigdy nie czyta (`CAPABILITIES` mówi o niej `Unavailable`). Bieg pada o ustawienie,
//! które dla tego vendora nie robi nic.
//!
//! Naprawa jest jednym zdaniem i to zdanie jest niezmiennikiem 23: polityka mieszka w rdzeniu,
//! adapter odpowiada na pytanie o siebie. Sufit staje się pytaniem do [`AgentDriver`], a nie
//! drugą tabelą w `commands::run`.
//!
//! # Trzy asercje, bo dwie kłamią
//!
//! (a) Krok Codeksa **rusza** i jego lista jedzie do sterownika jako `None` — czyli dokładnie tak,
//!     jak ją traktuje sam sterownik. Lista przycięta po cichu do czegoś innego byłaby trzecim
//!     zachowaniem, o którym nikt nie wie.
//!
//! (b) I zostaje po niej **zdanie w `run.json`**. Bez niego naprawa jest cichym pominięciem:
//!     człowiek wpisał listę, ekran ją przyjął, bieg jej nie użył i nic tego nie mówi — czyli ta
//!     sama martwa kontrolka (niezmiennik 16), tylko przesunięta o jedno miejsce.
//!
//! (c) A vendor, który narzędzia **umie** zawęzić, jest sądzony jak dziś. Bez tej trzeciej
//!     asercji zieleń przechodzi dla implementacji, która przestała pilnować sufitu w ogóle —
//!     czyli dla tej, w której `tools` staje się drugą drogą do uprawnień obok diala
//!     bezpieczeństwa (`DECISIONS-LOCKED.md` D6).
//!
//! # Słaba wersja tego kryterium
//!
//! Test na samym (a). Przechodzi dla implementacji, która wyrzuciła `what_this_step_may_use`
//! do kosza — a wtedy agent Claude'a proszący o `Bash` na „look only" dostaje `Bash`, i to jest
//! dokładnie ta wada, przed którą stoi T-63. Rozróżnia to (c) na tym samym pliku workflow
//! i tym samym agencie: różni je **wyłącznie** odpowiedź sterownika.
//!
//! Druga słaba wersja: asercja o odpowiedzi samych dubli. Dubel może odpowiadać, co mu każemy,
//! więc szew byłby prawdziwy tylko w tym pliku. Rozróżnia to asercja o **prawdziwych** dwóch
//! sterownikach produkcyjnych na końcu — one są jedynym powodem, dla którego to kryterium mówi
//! cokolwiek o produkcie.

// `expect()`/`unwrap()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam
// powód, co w `skills_reach_codex` i w pozostałych plikach tego celu.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::{Agent, FileAccess, Tools, Vendor};
use loadout_lib::store::Store;
use serde_json::Value;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Ile czekamy na bieg, zanim uznamy, że nie wróci. Powód w całości przy tej samej stałej
/// w `skills_reach_codex.rs`.
const PATIENCE: Duration = Duration::from_secs(20);

/// Dwa narzędzia, o które prosi agent. `Bash` stoi **ponad** dialem „look only" u Claude'a i to
/// on jest całą różnicą: bez niego obie gałęzie tego pliku byłyby zielone od zawsze.
const WANTED: [&str; 2] = ["Read", "Bash"];

const STEP: &str = "Reads the code";

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_codex_tools",
  "name": "One narrowed step",
  "steps": [
    {
      "kind": "agent",
      "id": "s_only",
      "name": "Reads the code",
      "agent": "00000000-0000-0000-0000-000000000061",
      "overrides": {},
      "instructions": "Reads the code",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_narrowed_codex_step_starts_and_says_the_list_was_left_out() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    // Vendor, który narzędzi nie zawęża — dokładnie ta odpowiedź, którą daje `CodexDriver`.
    let done = bench.one_run(false).await?;

    let report = done.report.map_err(|said| {
        format!(
            "the step asked for {WANTED:?} and this agent app has no list of tools to narrow, so \
             there was nothing to refuse about - and the whole run was refused anyway: {said}"
        )
    })?;
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded],
        "the step has to finish, or every assertion below is true of a step that never ran. It \
         ended as {:?}",
        report.steps
    );

    // (a) LISTA JEDZIE DO STEROWNIKA JAKO „NIE ZAWĘŻAJ" — tak samo, jak czyta ją sam sterownik.
    //     Nie przycięta, nie pusta: `Some([])` jest u vendorów słowem znaczącym „żadnych
    //     narzędzi", więc krok wystartowałby agenta, który nie przeczyta ani jednego pliku.
    let asked = done
        .tools
        .first()
        .ok_or("the step never reached the driver at all")?;
    assert_eq!(
        *asked, None,
        "this agent app does not take a list of tools, so the step has to reach it the same way \
         it reaches it today - with nothing to narrow. It arrived as {asked:?}, and a list that \
         is trimmed on the way is a third behaviour nobody can see from the outside"
    );

    // (b) I ZOSTAJE PO NIEJ ZDANIE. Człowiek wpisał listę; jeśli bieg jej nie użył, ma o tym
    //     powiedzieć w jedynym pliku, który przeżywa skasowanie indeksu (niezmiennik 4).
    let effective = done
        .effective
        .ok_or("the run left no effective settings for the step")?;
    let said = sentence_in(&effective).ok_or_else(|| {
        format!(
            "the human narrowed this agent to {WANTED:?}, the run went ahead without narrowing \
             anything, and run.json says nothing about it. A setting the screen accepts and the \
             run drops in silence is a control with no effect (invariant 16). Effective settings \
             came out as {effective}"
        )
    })?;
    for word in ["tool", "Codex"] {
        assert!(
            said.contains(word),
            "the sentence in run.json has to say WHAT was left out and by WHICH app, or it sends \
             the human back to guessing. It is missing {word:?} and reads: {said:?}"
        );
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_same_step_on_a_vendor_that_narrows_is_judged_as_it_is_today()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    // Ten sam plik workflow i ten sam agent. Różni je WYŁĄCZNIE odpowiedź sterownika.
    let done = bench.one_run(true).await?;

    let said = match done.report {
        Err(said) => said,
        Ok(report) => {
            return Err(format!(
                "this agent app narrows tools, the human asked for {WANTED:?} on a 'look only' \
                 dial, and the run went ahead as {:?}. Then the list is a second road to \
                 permissions running past the safety dial - which is the one thing it may never \
                 be (DECISIONS-LOCKED D6)",
                report.steps
            )
            .into());
        }
    };
    assert!(
        said.contains("Bash"),
        "the refusal has to name the tool that got through, or the human does not know which \
         line to cross out. It said: {said:?}"
    );
    assert!(
        said.contains("look only"),
        "and it has to name the dial, or the human does not know that widening the access is the \
         alternative. It said: {said:?}"
    );

    Ok(())
}

#[test]
fn the_two_real_apps_answer_this_question_for_themselves() {
    // TO JEST JEDYNA ASERCJA W TYM PLIKU O PRODUKCIE, a nie o dublu. Dubel odpowiada, co mu
    // każemy, więc bez tych dwóch linii szew jest prawdziwy wyłącznie w tym pliku.
    assert!(
        !CodexDriver::new().narrows_its_tools(),
        "Codex has no list of tools to narrow - its CAPABILITIES row says so - so it has to say \
         so for itself. While it says otherwise, a list nobody reads takes down whole runs"
    );
    assert!(
        ClaudeDriver::new().narrows_its_tools(),
        "Claude Code does narrow tools, and the ceiling over that list is the safety dial. An \
         app that stops saying so quietly turns the list into a second road to permissions"
    );
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

/// Co dubler zobaczył: lista narzędzi z KAŻDEGO uruchomienia, w kolejności uruchomień.
///
/// Lista, a nie jedna wartość, bo to są dwa różne fakty: krok, który nie dojechał do sterownika
/// w ogóle (pusto), i krok, który dojechał bez zawężenia (jeden wpis, a w nim `None`). Zwinięte
/// w jedno, „brak listy" znaczyłoby raz sukces, a raz bieg, który nigdy nie ruszył.
#[derive(Debug, Default)]
struct Seen(Mutex<Vec<Option<Vec<String>>>>);

fn lock<T>(what: &Mutex<T>) -> MutexGuard<'_, T> {
    what.lock().unwrap_or_else(PoisonError::into_inner)
}

fn watching_drivers(seen: Arc<Seen>, narrows: bool) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { seen, narrows });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler, którego jedyną treścią jest odpowiedź na pytanie o zawężanie narzędzi.
#[derive(Debug)]
struct Fake {
    seen: Arc<Seen>,
    narrows: bool,
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        "fake"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("fake".to_owned()),
        })
    }

    fn narrows_its_tools(&self) -> bool {
        self.narrows
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        lock(&self.seen.0).push(spec.tools.clone());

        let session = SessionRef {
            vendor: "fake",
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
            text: "read it".to_owned(),
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

// ── ławka ──────────────────────────────────────────────────────────────────────────────────

/// Zdanie o narzędziach schowane gdziekolwiek w migawce ustawień efektywnych.
///
/// Po WARTOŚCIACH, nie po umówionym kluczu: kryterium mówi „jedno zdanie w `run.json`", a nie
/// „pole o tej nazwie". Asercja o nazwie klucza przechodziłaby dla pustego napisu pod właściwą
/// nazwą i padałaby dla prawdziwego zdania pod inną.
fn sentence_in(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if text.contains(' ') && text.to_lowercase().contains("tool") => {
            Some(text.clone())
        }
        Value::Object(fields) => fields.values().find_map(sentence_in),
        Value::Array(items) => items.iter().find_map(sentence_in),
        _ => None,
    }
}

struct Done {
    report: Result<loadout_lib::commands::RunReport, String>,
    tools: Vec<Option<Vec<String>>>,
    effective: Option<Value>,
}

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
        // PRZEZ PRODUKCYJNY ZAPIS, nie przez własny plik: agent zapisany inną drogą sprawdzałby
        // czytnik na bajtach, których produkcja nigdy nie produkuje — a `tools` jest właśnie tym
        // polem, którego kształt na dysku łatwo napisać inaczej, niż go potem czyta.
        let agent = Agent {
            id: Uuid::from_u128(0x61),
            name: "Hand".to_owned(),
            runs_with: Vendor::Codex,
            file_access: FileAccess::LookOnly,
            tools: Tools::Only(WANTED.iter().copied().map(str::to_owned).collect()),
            ..Agent::example()
        };
        save_agent_inner(home.path(), &agent, None)?;
        Ok(Self { home, project })
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }

    async fn one_run(&self, narrows: bool) -> Result<Done, Box<dyn Error>> {
        let path = self.home.path().join("workflows").join("narrowed.json");
        fs::write(&path, WORKFLOW)?;

        let store = Store::open(&self.db())?;
        let seen = Arc::new(Seen::default());
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store: &store,
            drivers: watching_drivers(Arc::clone(&seen), narrows),
            processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow: path,
            how_many_at_once: 2,
            task: None,
            part: None,
            handoffs_from: None,
        };

        let (sink, source) = line_channel(QUEUE_CAP);
        let pump = spawn_pump(source, Channel::new(|_| Ok(())));
        let outcome = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
            .await
            .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))?;
        let _ = tokio::time::timeout(PATIENCE, pump).await;

        let effective = outcome
            .as_ref()
            .ok()
            .and_then(|report| effective_of(&report.dir.join("run.json")));

        Ok(Done {
            report: outcome.map_err(|error| error.to_string()),
            tools: lock(&seen.0).clone(),
            effective,
        })
    }
}

/// Migawka ustawień efektywnych kroku, prosto z `run.json` — czyli z pliku, nie z indeksu.
fn effective_of(path: &std::path::Path) -> Option<Value> {
    let text = fs::read_to_string(path).ok()?;
    let book: Value = serde_json::from_str(&text).ok()?;
    let steps = book.get("steps")?.as_array()?;
    let step = steps
        .iter()
        .find(|step| step.get("name").and_then(Value::as_str) == Some(STEP))?;
    step.get("effective").cloned()
}
