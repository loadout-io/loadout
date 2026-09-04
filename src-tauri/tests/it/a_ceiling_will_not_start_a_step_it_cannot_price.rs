//! Z-44: sufit biegu nie startuje kroku Codeksa, którego tury nie da się wycenić.
//!
//! Pięć dróg z jednego stanowiska, bo to jeden mechanizm o pięciu wyjściach: sufit i model
//! spoza tabeli, sufit i model z tabeli, brak sufitu i model spoza tabeli, plik cen z błędem
//! składni oraz stawka z pliku nadpisująca wbudowaną.
//!
//! Wyrocznia czyta ZDANIE tam, skąd bierze je okno (niezmiennik 29): odmowę przez
//! `read_run_inner`, czyli tę samą komendę, którą woła historia biegu, a wiersz „Done" z linii
//! wysłanych pompą do kanału okna. Kwota, którą oddaje funkcja wyceniająca, nie dowodzi niczego:
//! leży wyłącznie w pamięci procesu, który już zszedł.
//!
//! CLI jest prawdziwym procesem: skrypt-atrapa dopisuje wiersz do pliku-świadka przy każdym
//! uruchomieniu, więc „krok nie ruszył" jest tu faktem o systemie plików, a nie o stanie kroku.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::run::run_workflow_with_budget;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::AgentDriver;
use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::line::Line;
use loadout_lib::engine::step::StepState;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use serde_json::Value;
use tauri::ipc::{Channel, InvokeResponseBody};
use tempfile::TempDir;

/// Model, którego wbudowana tabela nie zna i znać nie ma prawa.
const UNPRICED: &str = "gpt-9.9-nebula";
/// Model z wbudowanej tabeli — stawki 0,1 / 0,02 / 1,25 za milion tokenów.
const PRICED: &str = "gpt-5.6-luna";
/// Sufit z opisu zadania. Kwota jedzie do zdania odmowy, więc jest częścią wyroczni.
const CEILING: f64 = 275.0;
/// Ścieżka, którą zdanie odmowy ma podać człowiekowi.
const WHERE_PRICES_LIVE: &str = "~/.loadout/prices.json";
const PATIENCE: Duration = Duration::from_secs(30);

/// Liczniki tury, te same we wszystkich drogach: 10 000 wejścia (w tym 5 000 z cache'u)
/// i 20 000 wyjścia. Świeżego wejścia jest więc 5 000 (poprawka Z-12).
const CODEX_CLI: &str = r###"#!/bin/sh
printf 'ran\n' >> "MARKER"
printf '%s\n' '{"type":"thread.started","thread_id":"thread-z44"}'
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","id":"item-1","text":"## Answer\nDone.\n\n## Evidence\nnone.\n\n## Open\nnothing."}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":10000,"cached_input_tokens":5000,"output_tokens":20000}}'
exit 0
"###;

const AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-000000004444
name: Priced Or Not
summary: Runs one Codex model
color: moss
runsWith: codex
model: MODEL
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

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_z44_unknown_price",
  "name": "A model with no price",
  "steps": [
    {
      "kind": "agent",
      "id": "s_priced",
      "name": "Priced Or Not",
      "agent": "01990000-0000-7000-8000-000000004444",
      "overrides": {},
      "instructions": "Run the model.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}"#;

/// Dwa kroki na jednej strzałce, oba tym samym nieznanym modelem, i ani jednego `whenItFails`
/// w pliku — bo `carry-on` JEST wartością domyślną (`workflow::WhenItFails`, decyzja właściciela
/// 2026-08-23). Odmowa pierwszego kroku wpisuje go do `stopped_by_the_budget` i do `did_not_pass`,
/// więc drugi leży w stożku „jedź mimo sufitu" — i to jest ta droga, którą trzeba przejść.
const CHAINED_WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_z44_unknown_price_chain",
  "name": "Two models with no price",
  "steps": [
    {
      "kind": "agent",
      "id": "s_first",
      "name": "First",
      "agent": "01990000-0000-7000-8000-000000004444",
      "overrides": {},
      "instructions": "Run the model.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_second",
      "name": "Second",
      "agent": "01990000-0000-7000-8000-000000004444",
      "overrides": {},
      "instructions": "Run the model again.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    }
  ],
  "links": [{ "from": "s_first", "to": "s_second" }]
}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_ceiling_refuses_the_codex_step_whose_model_has_no_price() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new(UNPRICED)?;
    let Ran {
        report, history, ..
    } = bench.run(Some(CEILING)).await?;

    assert_eq!(
        report.steps,
        vec![StepState::Skipped],
        "a run with a ceiling must refuse the step it cannot price instead of paying for it"
    );
    assert!(
        !bench.the_agent_app_ran(),
        "the agent app was started for a model this run had no way to pay for"
    );

    let step = the_only_step_of(&history)?;
    let said = step
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        said.contains(UNPRICED),
        "the refusal must name the model it could not price. The row said {step}"
    );
    assert!(
        said.contains(WHERE_PRICES_LIVE),
        "the refusal must say where the price can be written down. The row said {step}"
    );
    assert!(
        said.contains("$275.00"),
        "the refusal must name the ceiling it could not keep. The row said {step}"
    );
    Ok(())
}

/// Ta sama odmowa dla kroku, który stoi ZA odmówionym — czyli w stożku „jedź mimo sufitu".
///
/// Osobna droga, bo bieg o jednym kroku jej nie widzi: `carry-on` jest domyślne, więc pierwszy
/// odmówiony krok robi ze swojego następnika stożek, któremu wolno przekroczyć kwotę. Wolno mu
/// przekroczyć KWOTĘ, której nie wolno mu przestać mierzyć — a tura bez ceny jest właśnie
/// przestaniem mierzyć.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_step_after_a_refused_one_is_refused_too_when_its_price_is_unknown()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::chained(UNPRICED)?;
    let Ran {
        report, history, ..
    } = bench.run(Some(CEILING)).await?;

    assert_eq!(
        report.steps,
        vec![StepState::Skipped, StepState::Skipped],
        "the step below a refused one carries on past the amount, not past the measuring: \
         it has no price either, so the ceiling has to stop it too"
    );
    assert!(
        !bench.the_agent_app_ran(),
        "a run with a ceiling started the agent app for a turn nobody could price, because \
         the step before it was allowed to carry on"
    );

    for name in ["First", "Second"] {
        let step = the_step_named(&history, name)?;
        let said = step
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert!(
            said.contains(UNPRICED) && said.contains(WHERE_PRICES_LIVE),
            "the row for {name} must carry the same refusal as the one before it, naming the \
             model and the file. The row said {step}"
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn without_a_ceiling_the_unpriced_model_runs_and_says_the_price_is_not_known()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new(UNPRICED)?;
    let Ran {
        report,
        history,
        on_screen,
    } = bench.run(None).await?;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded],
        "a run with no ceiling keeps today's behaviour: the unpriced model still works"
    );
    assert!(
        bench.the_agent_app_ran(),
        "a run with no ceiling has nothing to refuse, so the agent app must have started"
    );
    let step = the_only_step_of(&history)?;
    assert!(
        step.get("costUsd").is_some_and(Value::is_null),
        "a model with no price keeps a null amount, never zero: {step}"
    );
    assert!(
        on_screen
            .iter()
            .any(|text| text.contains(UNPRICED) && text.contains("is not known")),
        "the row a person reads must still name the model whose price is not known. \
         The window was sent {on_screen:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_ceiling_starts_the_step_whose_model_the_built_in_table_knows()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new(PRICED)?;
    let Ran {
        report, history, ..
    } = bench.run(Some(CEILING)).await?;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded],
        "a model the built-in table prices must run under the same ceiling"
    );
    assert!(bench.the_agent_app_ran(), "the priced model never started");
    let step = the_only_step_of(&history)?;
    assert_eq!(
        the_amount_of(step),
        Some(0.0256),
        "the built-in rate for {PRICED} is 0.1 / 0.02 / 1.25 per million; the row said {step}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_price_written_down_by_hand_starts_the_step_and_pays_its_own_rate()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new(UNPRICED)?;
    bench.the_person_wrote_down(&format!(
        r#"{{ "{UNPRICED}": {{ "input": 2.0, "cached": 0.4, "output": 21.0 }} }}"#
    ))?;
    let Ran {
        report, history, ..
    } = bench.run(Some(CEILING)).await?;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded],
        "a price written down by hand must let the same step run under the same ceiling"
    );
    let step = the_only_step_of(&history)?;
    assert_eq!(
        the_amount_of(step),
        Some(0.432),
        "the step must be charged the rate the person wrote down; the row said {step}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_price_written_down_by_hand_wins_over_the_built_in_one() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new(PRICED)?;
    bench.the_person_wrote_down(&format!(
        r#"{{ "{PRICED}": {{ "input": 10.0, "cached": 1.0, "output": 100.0 }} }}"#
    ))?;
    let Ran { history, .. } = bench.run(Some(CEILING)).await?;

    let step = the_only_step_of(&history)?;
    assert_eq!(
        the_amount_of(step),
        Some(2.055),
        "the rate written down by hand must beat the built-in rate for the same model; \
         the row said {step}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_price_file_that_cannot_be_read_refuses_the_run_by_name() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new(PRICED)?;
    bench.the_person_wrote_down("{ this is not json")?;

    let refusal = match bench.run(Some(CEILING)).await {
        Ok(ran) => {
            return Err(format!(
                "a price file nobody can read must stop the run before it starts; it ended as {:?}",
                ran.report.steps
            )
            .into());
        }
        Err(refusal) => refusal.to_string(),
    };
    assert!(
        refusal.contains("prices.json"),
        "the refusal must name the file a person has to fix. It said {refusal:?}"
    );
    assert!(
        !bench.the_agent_app_ran(),
        "a run refused over its price file must not start a single agent app"
    );
    Ok(())
}

/// Kwota z wiersza historii, zaokrąglona do dziesiątej części centa — porównanie `f64` co do
/// bitu byłoby asercją o arytmetyce zmiennoprzecinkowej, a nie o cenie.
fn the_amount_of(step: &Value) -> Option<f64> {
    let amount = step.get("costUsd").and_then(Value::as_f64)?;
    Some((amount * 10_000.0).round() / 10_000.0)
}

fn the_only_step_of(history: &Value) -> Result<&Value, Box<dyn Error>> {
    history
        .get("steps")
        .and_then(Value::as_array)
        .and_then(|steps| steps.first())
        .ok_or_else(|| "the history of this run has no step at all".into())
}

fn the_step_named<'a>(history: &'a Value, name: &str) -> Result<&'a Value, Box<dyn Error>> {
    history
        .get("steps")
        .and_then(Value::as_array)
        .ok_or("the history of this run has no steps at all")?
        .iter()
        .find(|step| step.get("name").and_then(Value::as_str) == Some(name))
        .ok_or_else(|| format!("the history of this run has no step named {name}").into())
}

/// Co ten bieg po sobie zostawił: stany kroków, historia otwarta tą samą komendą, co okno,
/// i wszystko, co pompa zdążyła wysłać na ekran.
struct Ran {
    report: RunReport,
    history: Value,
    on_screen: Vec<String>,
}

struct Bench {
    home: TempDir,
    project: TempDir,
    workflow: PathBuf,
    binary: PathBuf,
    marker: PathBuf,
    store: Store,
}

impl Bench {
    /// Bieg o jednym kroku — tyle wystarczy każdej drodze poza stożkiem `carry-on`.
    fn new(model: &str) -> Result<Self, Box<dyn Error>> {
        Self::running(model, WORKFLOW)
    }

    /// Bieg o dwóch krokach na strzałce, czyli jedyny kształt, w którym istnieje stożek
    /// „jedź mimo sufitu".
    fn chained(model: &str) -> Result<Self, Box<dyn Error>> {
        Self::running(model, CHAINED_WORKFLOW)
    }

    fn running(model: &str, workflow_file: &str) -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(
            home.path().join("agents/priced-or-not.md"),
            AGENT.replace("MODEL", model),
        )?;
        let workflow = home.path().join("workflows/priced-or-not.json");
        fs::write(&workflow, workflow_file)?;

        // Świadek leży POZA katalogiem biegu: krok z własną kopią plików pracuje w drzewie,
        // które powstaje dopiero po planowaniu, a odmowa ma paść wcześniej.
        let marker = home.path().join("the-agent-app-ran");
        let binary = home.path().join("codex-z44");
        fs::write(
            &binary,
            CODEX_CLI.replace("MARKER", &marker.display().to_string()),
        )?;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))?;

        let store = Store::open(&project.path().join(".loadout/loadout.db"))?;
        Ok(Self {
            home,
            project,
            workflow,
            binary,
            marker,
            store,
        })
    }

    /// Cennik dopisany ręką człowieka, dokładnie tam, gdzie zdanie odmowy każe go szukać.
    fn the_person_wrote_down(&self, text: &str) -> Result<(), Box<dyn Error>> {
        fs::write(self.home.path().join("prices.json"), text)?;
        Ok(())
    }

    /// Czy prawdziwy proces CLI ruszył choć raz.
    fn the_agent_app_ran(&self) -> bool {
        self.marker.is_file()
    }

    async fn run(&self, ceiling: Option<f64>) -> Result<Ran, Box<dyn Error>> {
        let driver: Arc<dyn AgentDriver> = Arc::new(CodexDriver::with_binary(self.binary.clone()));
        let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store: &self.store,
            drivers,
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow: self.workflow.clone(),
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        };
        let delivered = Delivered::default();
        let (sink, source) = line_channel(QUEUE_CAP);
        let pump = spawn_pump(source, delivered.channel());
        let (report, _) = tokio::time::timeout(PATIENCE, async {
            tokio::join!(
                run_workflow_with_budget(&deps, &request, sink, ceiling),
                pump
            )
        })
        .await
        .map_err(|_| format!("the run did not finish within {PATIENCE:?}"))?;
        let report = report?;
        let folder = report
            .dir
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("the run directory has no name history could open")?;
        Ok(Ran {
            history: serde_json::to_value(read_run_inner(self.project.path(), folder)?)?,
            on_screen: delivered.taken(),
            report,
        })
    }
}

/// Wiersze, które pompa wysłała kanałem okna — czyli dokładnie to, co człowiek zobaczył.
#[derive(Default)]
struct Delivered(Arc<Mutex<Vec<String>>>);

impl Delivered {
    fn channel(&self) -> Channel<Vec<Line>> {
        let seen = Arc::clone(&self.0);
        Channel::new(move |body| {
            if let InvokeResponseBody::Json(text) = body {
                seen.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(text);
            }
            Ok(())
        })
    }

    fn taken(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}
