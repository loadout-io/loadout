//! V-02: obowiązkowe zachowania nie znikają w jednym słowie podsumowania.
//!
//! Incydent I-05 (bieg z 2026-09-06): krok QA opisał w prozie brak obowiązkowych zachowań
//! i w tej samej odpowiedzi napisał `outcome: pass`. Nic w tym nie było sprzeczne z instrukcją,
//! którą dostał — pytano go, czy praca jest „good enough to build on". Pytanie było złe.
//!
//! Kryterium sądzi PRODUKCYJNĄ drogę: prawdziwy bieg, prawdziwa pętla, zapisany `run.json`.

#![allow(clippy::panic)]
#![allow(clippy::expect_used, clippy::too_many_lines, clippy::similar_names)]
#![allow(clippy::assigning_clones, clippy::duration_suboptimal_units)]
#![allow(clippy::struct_field_names, clippy::implicit_clone)]

use std::error::Error;
use std::fs;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens, Voice,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{AppState, line_channel};
use loadout_lib::library::agents::{Agent, write_agent_file};
use loadout_lib::store::Store;
use loadout_lib::workflow::criteria::{self, Criterion, Method, Outcome as Verdict};
use serde_json::{Value, json};
use tokio::sync::mpsc;

// --------------------------------------------------------------------------------------------
// Rdzeń rozstrzygnięcia
// --------------------------------------------------------------------------------------------

/// I-05 wprost: proza opisuje braki, ostatni wiersz mówi `pass`.
#[test]
fn a_pass_line_cannot_outvote_a_requirement_it_never_answered() {
    let judged = criteria::judge(
        &approved(),
        "The queue work is in good shape and good enough to build on.\n\
         Recording is not drained after a restart, which I would call non-blocking.\n\n\
         criterion c1: passed — Stop then Later kept the audio\n\
         outcome: pass\n",
    );
    assert_eq!(
        judged.outcome,
        Verdict::NotJudged,
        "a summary word passed work whose second requirement was never answered"
    );
    assert_eq!(judged.missing, vec!["c2".to_owned()]);
    assert!(
        judged.said().contains("said nothing about 1") && judged.said().contains("c2"),
        "the sentence does not name the requirement nobody answered: {:?}",
        judged.said()
    );
}

/// Prozy nie parsujemy. „non-blocking" w zdaniu nie zmienia wyniku kryterium.
#[test]
fn prose_about_severity_changes_nothing() {
    let judged = criteria::judge(
        &approved(),
        "criterion c1: passed\n\
         criterion c2: failed — the next recording never starts\n\
         I consider this non-blocking and minor.\n\
         outcome: pass\n",
    );
    assert_eq!(judged.outcome, Verdict::DidNotPass);
    assert_eq!(judged.failed, vec!["c2".to_owned()]);
}

/// Brak pomiaru nie jest ani zaliczeniem, ani dowodem wady.
#[test]
fn a_requirement_nobody_could_measure_is_not_a_pass_and_not_a_defect() {
    let judged = criteria::judge(
        &approved(),
        "criterion c1: passed\ncriterion c2: not tested — no way to drive the window\n",
    );
    assert_eq!(judged.outcome, Verdict::NotJudged);
    assert_eq!(judged.not_tested, vec!["c2".to_owned()]);
    assert!(
        judged.failed.is_empty(),
        "a missing measurement was recorded as a product defect"
    );
    assert!(
        judged.said().contains("could not measure 1"),
        "the sentence does not separate a missing measurement from a defect: {:?}",
        judged.said()
    );
}

/// Weryfikator nie może obniżyć metody z pełnego runtime'u do mocka.
#[test]
fn a_mocked_confirmation_does_not_satisfy_a_runtime_requirement() {
    let judged = criteria::judge(
        &approved(),
        "criterion c1: passed\ncriterion c2: passed via mocked ui — the button reacts\n",
    );
    assert_eq!(judged.outcome, Verdict::NotJudged);
    assert_eq!(judged.weaker_method, vec!["c2".to_owned()]);
    assert!(
        judged.said().contains("a weaker way"),
        "the sentence does not say the confirmation was weaker than agreed: {:?}",
        judged.said()
    );
}

/// Nieznane i podwójne identyfikatory nie mogą dać zieleni.
#[test]
fn unknown_and_duplicated_identifiers_never_go_green() {
    let unknown = criteria::judge(
        &approved(),
        "criterion c1: passed\ncriterion c2: passed via full runtime\ncriterion c9: passed\n",
    );
    assert_eq!(unknown.outcome, Verdict::NotJudged);
    assert_eq!(unknown.unknown, vec!["c9".to_owned()]);

    let twice = criteria::judge(
        &approved(),
        "criterion c1: passed\ncriterion c1: failed\ncriterion c2: passed via full runtime\n",
    );
    assert_eq!(twice.outcome, Verdict::NotJudged);
    assert_eq!(twice.duplicated, vec!["c1".to_owned()]);
}

/// Komplet wymaganych kryteriów, potwierdzony właściwą metodą, przechodzi.
#[test]
fn a_complete_report_with_the_agreed_methods_passes() {
    let judged = criteria::judge(
        &approved(),
        "criterion c1: passed — audio kept\n\
         criterion c2: passed via full runtime — the next recording started\n\
         outcome: pass\n",
    );
    assert_eq!(judged.outcome, Verdict::Passed, "{:?}", judged.said());
    assert!(judged.said().is_empty());
}

/// Kryterium nieobowiązkowe nie blokuje zaliczenia.
#[test]
fn an_optional_requirement_does_not_block_the_result() {
    let mut list = approved();
    list.push(Criterion {
        id: "c3".to_owned(),
        behaviour: "The window remembers its size".to_owned(),
        required: false,
        method: Method::HumanConfirmed,
    });
    let judged = criteria::judge(
        &list,
        "criterion c1: passed\ncriterion c2: passed via full runtime\n",
    );
    assert_eq!(judged.outcome, Verdict::Passed, "{:?}", judged.said());
}

/// Prośba skierowana do weryfikatora wymienia każde wymaganie i jego metodę.
#[test]
fn the_asked_for_block_names_every_requirement_and_its_method() {
    let asked = criteria::asked_for(&approved());
    assert!(asked.contains("c1: Stop then Later keeps the audio"));
    assert!(asked.contains("the running application with its real backend"));
    assert!(
        asked.contains("not tested"),
        "the verifier was never told it may answer that something could not be measured"
    );
    assert!(
        !asked.contains("good enough to build on"),
        "the strengthened block still asks the question that produced I-05"
    );
}

fn approved() -> Vec<Criterion> {
    vec![
        Criterion {
            id: "c1".to_owned(),
            behaviour: "Stop then Later keeps the audio and starts nothing".to_owned(),
            required: true,
            method: Method::AutomatedTest,
        },
        Criterion {
            id: "c2".to_owned(),
            behaviour: "The next recording starts after Later".to_owned(),
            required: true,
            method: Method::FullRuntime,
        },
    ]
}

// --------------------------------------------------------------------------------------------
// Droga produkcyjna: prawdziwy bieg, zapisany wynik
// --------------------------------------------------------------------------------------------

/// Ten sam fakt tam, gdzie widzi go CZŁOWIEK (niezmiennik 29): pętla nie domyka się na
/// `outcome: pass`, a zapisany bieg mówi, o czym weryfikator nie powiedział ani słowa.
#[tokio::test]
async fn the_saved_run_says_which_requirement_was_never_answered() -> Result<(), Box<dyn Error>> {
    let said = "Everything looks good enough to build on.\n\n\
                criterion c1: passed — audio kept\n\
                outcome: pass\n";
    let run = judged_run(said).await?;
    let tester = row(&run, "Tester")?;
    let error = tester
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        error.contains("said nothing about 1") && error.contains("c2"),
        "the saved run does not say which requirement the tester skipped: {error:?}"
    );
    assert_eq!(
        tester.get("round_outcome").and_then(Value::as_str),
        Some("fail"),
        "the loop closed on a summary word while a required behaviour was unanswered"
    );
    Ok(())
}

/// Komplet potwierdzeń domyka pętlę tą samą drogą, co dotąd.
#[tokio::test]
async fn a_complete_report_still_closes_the_loop() -> Result<(), Box<dyn Error>> {
    let said = "criterion c1: passed — audio kept\n\
                criterion c2: passed via full runtime — the next recording started\n\
                outcome: pass\n";
    let run = judged_run(said).await?;
    let tester = row(&run, "Tester")?;
    assert_eq!(
        tester.get("round_outcome").and_then(Value::as_str),
        Some("pass"),
        "a complete report did not close the loop: {:?}",
        tester.get("error")
    );
    Ok(())
}

/// Weryfikator naprawdę dostaje listę w swoim prompcie — inaczej wymagania są umową
/// podpisaną przez jedną stronę.
#[tokio::test]
async fn the_tester_is_given_the_approved_list() -> Result<(), Box<dyn Error>> {
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&prompts);
    run_with(
        "criterion c1: passed\ncriterion c2: passed via full runtime\n",
        Some(seen),
    )
    .await?;
    let asked = prompts
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let tester = asked
        .iter()
        .find(|one| one.contains("criterion <id>"))
        .ok_or("no step was asked to answer about the approved requirements")?;
    assert!(
        tester.contains("c2: The next recording starts after Later"),
        "the approved list never reached the verifier's prompt"
    );
    Ok(())
}

async fn judged_run(said: &str) -> Result<Value, Box<dyn Error>> {
    run_with(said, None).await
}

async fn run_with(
    said: &str,
    prompts: Option<Arc<Mutex<Vec<String>>>>,
) -> Result<Value, Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let project = root.path().canonicalize()?.join("project");
    let home = root.path().canonicalize()?.join("home");
    fs::create_dir_all(project.join(".loadout"))?;
    fs::create_dir_all(project.join(".loadout/workflows"))?;
    let agent = Agent::example();
    write_agent_file(&project.join(".loadout/agents"), &agent, None)?;
    let workflow = project.join(".loadout/workflows/verify.json");
    fs::write(
        &workflow,
        json!({"format":1,"id":"wf-criteria","name":"Criteria","steps":[
            {"kind":"agent","id":"build","name":"Builder","agent":agent.id,
             "instructions":"Do the work.","overrides":{},"folder":{"use":"project"},
             "at":{"x":0,"y":0}},
            {"kind":"agent","id":"test","name":"Tester","agent":agent.id,
             "instructions":"Check the work.","overrides":{},"folder":{"use":"project"},
             "criteria":[
                {"id":"c1","behaviour":"Stop then Later keeps the audio and starts nothing",
                 "required":true,"method":"automated-test"},
                {"id":"c2","behaviour":"The next recording starts after Later",
                 "required":true,"method":"full-runtime"}],
             "at":{"x":200,"y":0}}],
         "links":[{"from":"build","to":"test"},
                   {"from":"test","to":"build","max_turns":1}]})
        .to_string(),
    )?;
    let driver: Arc<dyn AgentDriver> = Arc::new(Driver {
        said: said.to_owned(),
        prompts,
    });
    let drivers: Drivers = Arc::new(move |_| Arc::clone(&driver));
    let state = AppState::new(
        home,
        project.clone(),
        Store::open(&project.join(".loadout/loadout.db"))?,
        drivers,
    );
    let deps = state.begin_run(&project)?;
    let (sink, _lines) = line_channel(512);
    let report = tokio::time::timeout(
        Duration::from_secs(30),
        run_workflow_inner(
            &deps,
            &RunRequest {
                workflow,
                how_many_at_once: 2,
                task: None,
                part: None,
                handoffs_from: None,
            },
            sink,
        ),
    )
    .await??;
    Ok(serde_json::from_slice(&fs::read(
        report.dir.join("run.json"),
    )?)?)
}

fn row<'a>(run: &'a Value, name: &str) -> Result<&'a Value, Box<dyn Error>> {
    run.get("steps")
        .and_then(Value::as_array)
        .and_then(|steps| {
            steps
                .iter()
                .find(|step| step.get("name").and_then(Value::as_str) == Some(name))
        })
        .ok_or_else(|| format!("run.json has no step named {name}").into())
}

struct Driver {
    said: String,
    prompts: Option<Arc<Mutex<Vec<String>>>>,
}

#[async_trait]
impl AgentDriver for Driver {
    fn id(&self) -> &'static str {
        "claude-code"
    }
    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: None,
        })
    }
    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        if let Some(prompts) = &self.prompts {
            prompts
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(spec.prompt.to_string());
        }
        let judging = spec.prompt.contains("criterion <id>");
        Ok(Box::new(Handle {
            text: if judging {
                self.said.clone()
            } else {
                "## Answer\nDone.\n\n## Evidence\nnone\n\n## Open\nNone.\n".to_owned()
            },
            events,
            session: SessionRef {
                vendor: "claude-code",
                id: spec.run_id.to_string(),
            },
        }))
    }
}

struct Handle {
    text: String,
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for Handle {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }
    fn group(&self) -> Option<GroupId> {
        None
    }
    fn voice(&self) -> Option<Voice> {
        None
    }
    async fn send(&mut self, _: String) -> anyhow::Result<()> {
        anyhow::bail!("this double takes no extra turns")
    }
    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        let result = Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text: self.text.clone(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send(AgentEvent::Finished(result.clone()).into())
            .await;
        Ok(result)
    }
    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }
    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}
