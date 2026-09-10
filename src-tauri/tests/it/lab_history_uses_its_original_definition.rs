//! WF-15: rzeczywisty pomiar → edycja zestawu → odczyt tablicy, bez nowej oceny według formularza.

#![allow(clippy::too_many_lines)]
#![allow(clippy::assigning_clones)]

use std::error::Error;
use std::fs;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::lab::{plan_a_run_inner, read_board_inner, read_set_inner};
use loadout_lib::commands::{Drivers, RunControl, RunDeps};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::line_channel;
use loadout_lib::lab::{EvalSet, file};
use loadout_lib::library::agents::Agent;
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tokio::sync::mpsc;

#[tokio::test]
async fn changing_the_form_does_not_regrade_the_finished_run() -> Result<(), Box<dyn Error>> {
    measure_then_change("edit").await
}

#[tokio::test]
async fn removing_the_current_set_does_not_remove_its_original_criteria()
-> Result<(), Box<dyn Error>> {
    measure_then_change("remove").await
}

#[tokio::test]
async fn a_legacy_run_is_not_regraded_with_current_criteria() -> Result<(), Box<dyn Error>> {
    measure_then_change("legacy").await
}

#[tokio::test]
async fn altered_criteria_bytes_are_not_used_to_regrade_history() -> Result<(), Box<dyn Error>> {
    measure_then_change("corrupt-definition").await
}

#[tokio::test]
async fn missing_original_input_makes_the_measurement_invalid() -> Result<(), Box<dyn Error>> {
    measure_then_change("corrupt-input").await
}

#[tokio::test]
async fn missing_instruction_package_does_not_silently_mean_no_instructions()
-> Result<(), Box<dyn Error>> {
    measure_then_change("missing-instructions").await
}

#[tokio::test]
async fn an_instruction_package_from_another_measurement_is_not_accepted()
-> Result<(), Box<dyn Error>> {
    measure_then_change("wrong-instructions").await
}

#[tokio::test]
async fn a_changed_case_starts_a_separate_comparison() -> Result<(), Box<dyn Error>> {
    measure_then_change("compare-criteria").await
}

#[tokio::test]
async fn a_changed_input_starts_a_separate_comparison() -> Result<(), Box<dyn Error>> {
    measure_then_change("compare-input").await
}

#[tokio::test]
async fn a_changed_variant_is_the_variable_being_measured() -> Result<(), Box<dyn Error>> {
    measure_then_change("compare-variant").await
}

async fn measure_then_change(change: &str) -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let project = root.path().join("project");
    let library = root.path().join("library");
    fs::create_dir_all(project.join(".loadout"))?;
    fs::create_dir_all(library.join("agents"))?;
    fs::write(project.join("input.txt"), "fixture")?;
    fs::write(
        project.join("AGENTS.md"),
        "Return the requested fixture answer.",
    )?;
    fs::write(
        project.join(".loadout/project.json"),
        r#"{"instructions":{"enabled":true}}"#,
    )?;
    let mut agent = Agent::example();
    agent.instructions = "Return a fixture answer.".to_owned();
    agent.skills.clear();
    agent.connections.clear();
    loadout_lib::commands::agents::save_agent_inner(&library, &agent, None)?;
    let set: EvalSet = serde_json::from_value(json!({
        "format":1,"id":"original-criteria","name":"Original criteria",
        "subject":{"kind":"agent","id":agent.id.to_string()},
        "cases":[{"id":"one","name":"The original case","task":"Read the fixture",
            "status":"in-use","expect":[{"field":"Answer","contains":"original marker"}]}],
        "variants":[{"id":"one","name":"Original variant","agent":agent.id.to_string()}]
    }))?;
    let path = file::path_for(&project, &set.id);
    file::save(&set, &path, None)?;
    /* FIKSTURA MA MOWIC PRAWDE O TYM, CO LEZY NA DYSKU.
     *
     * Powyzej deklarujemy `"format":1`, ale `file::save` podnosi format do biezacego przy
     * zapisie — wiec bez tej linii ten przypadek NIGDY nie sadzil zestawu zapisanego wczesniej,
     * tylko swiezy. A to jest dokladnie stan, ktory ma kazdy czlowiek na dysku: zestaw zapisany
     * zanim `CURRENT` poszlo w gore, czytany (odczyt odrzuca wylacznie format WIEKSZY niz
     * biezacy) i nigdy od tamtej pory nieprzepisany. Przy rownosci w `valid_for` taki zestaw
     * przestawal byc oceniany: pomiar melduje „Criteria snapshot unavailable" i ZERO zaliczonych,
     * mimo ze bieg przeszedl — a asercja `(passed, judged) == (1, 1)` nizej to lapie. */
    let raw = fs::read_to_string(&path)?;
    fs::write(
        &path,
        raw.replacen(
            &format!("\"format\": {}", loadout_lib::lab::CURRENT),
            "\"format\": 1",
            1,
        ),
    )?;
    let planned = plan_a_run_inner(&project, &set.id, 2)?;
    let store = Store::open(&project.join(".loadout/loadout.db"))?;
    let drivers: Drivers = Arc::new(|_| Arc::new(Fixture));
    let deps = RunDeps {
        home: &library,
        library: library.clone(),
        project: &project,
        store: &store,
        drivers,
        control: RunControl::new(),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
    };
    let (sink, _source) = line_channel(1024);
    let report =
        loadout_lib::commands::run::run_workflow_inner(&deps, &planned.request, sink).await?;
    let initial = read_board_inner(&project, &set.id, 10)?;
    assert_eq!(
        (initial.runs[0].passed, initial.runs[0].judged),
        (1, 1),
        "the fixture must really be measured first: {initial:?}"
    );
    match change {
        "edit" => {
            let mut open = read_set_inner(&project, &set.id)?;
            open.set.cases[0].expect[0].contains = "a different marker".to_owned();
            open.set.cases[0].command = "printf '1 passed\\n'".to_owned();
            open.set.cases[0].proof = "(\\d+) passed".to_owned();
            open.set.variants[0].name = "Today's changed variant".to_owned();
            file::save(&open.set, &path, Some(&open.revision))?;
        }
        "remove" => fs::remove_file(&path)?,
        "legacy" => {
            let run_path = report.dir.join("run.json");
            let mut saved: Value = serde_json::from_slice(&fs::read(&run_path)?)?;
            saved["workflow_snapshot"]
                .as_object_mut()
                .ok_or("missing workflow snapshot")?
                .remove("measurementDefinition");
            fs::write(run_path, serde_json::to_vec(&saved)?)?;
        }
        "corrupt-definition" => {
            let run_path = report.dir.join("run.json");
            let mut saved: Value = serde_json::from_slice(&fs::read(&run_path)?)?;
            saved["workflow_snapshot"]["measurementDefinition"]["set"]["cases"][0]["task"] =
                json!("changed after the run");
            fs::write(run_path, serde_json::to_vec(&saved)?)?;
        }
        "corrupt-input" => fs::write(report.dir.join("input/files/input.txt"), "changed")?,
        "missing-instructions" => fs::remove_file(report.dir.join("instructions/manifest.json"))?,
        "wrong-instructions" => {
            let path = report.dir.join("run.json");
            let mut saved: Value = serde_json::from_slice(&fs::read(&path)?)?;
            saved["project_instructions"] = json!({"id":"other-package", "digest":"other-digest"});
            fs::write(path, serde_json::to_vec(&saved)?)?;
        }
        "compare-criteria" | "compare-input" | "compare-variant" => {
            if change == "compare-input" {
                fs::write(project.join("input.txt"), "different input")?;
            } else {
                let mut open = read_set_inner(&project, &set.id)?;
                if change == "compare-criteria" {
                    open.set.cases[0].expect[0].contains = "different criterion".to_owned();
                } else {
                    open.set.variants[0].overrides.insert(
                        "instructions".to_owned(),
                        json!("A new variant, the same measurement conditions."),
                    );
                }
                file::save(&open.set, &path, Some(&open.revision))?;
            }
            let next = plan_a_run_inner(&project, &set.id, 2)?;
            let (sink, _source) = line_channel(1024);
            loadout_lib::commands::run::run_workflow_inner(&deps, &next.request, sink).await?;
        }
        _ => return Err("invalid test scenario".into()),
    }
    let later = read_board_inner(&project, &set.id, 10);
    assert!(later.is_ok(), "history lost its saved criteria: {later:?}");
    let later = later?;
    if matches!(change, "legacy" | "corrupt-definition") {
        assert_eq!(
            later.runs[0].judged, 0,
            "a legacy run was regraded using the live set"
        );
        assert!(
            later.runs[0]
                .cells
                .iter()
                .any(|cell| cell.said.contains("Criteria snapshot unavailable"))
        );
    } else if matches!(change, "missing-instructions" | "wrong-instructions") {
        assert_eq!(
            later.runs[0].judged, 0,
            "missing or mismatched instructions were counted as the measured input"
        );
        assert!(
            later.runs[0]
                .cells
                .iter()
                .any(|cell| cell.said.contains("Saved instructions unavailable"))
        );
    } else if change == "corrupt-input" {
        assert_eq!(
            later.runs[0].judged, 0,
            "an input with broken integrity was still measured"
        );
        assert!(
            later.runs[0]
                .cells
                .iter()
                .any(|cell| cell.said.contains("Saved input unavailable"))
        );
    } else if change.starts_with("compare-") {
        assert_eq!(later.runs.len(), 2);
        assert_eq!((later.runs[1].passed, later.runs[1].judged), (1, 1));
        if change == "compare-variant" {
            assert!(
                later.movement.is_some(),
                "changing the tested variable must not break comparability"
            );
        } else {
            assert!(
                later.movement.is_none(),
                "a change of input or criteria was shown as a performance change"
            );
        }
    } else {
        assert_eq!(
            (later.runs[0].passed, later.runs[0].judged),
            (1, 1),
            "editing the form changed what yesterday's run means"
        );
    }
    Ok(())
}

struct Fixture;
#[async_trait]
impl AgentDriver for Fixture {
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
        Ok(Box::new(Turn {
            events,
            session: SessionRef {
                vendor: "claude-code",
                id: spec.run_id.to_string(),
            },
        }))
    }
}
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
    async fn send(&mut self, _: String) -> anyhow::Result<()> {
        Ok(())
    }
    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        let text = "Answer: original marker\nEvidence: fixture\nOpen: none";
        self.events
            .send(
                AgentEvent::Said {
                    text: text.to_owned(),
                }
                .into(),
            )
            .await?;
        let outcome = Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text: text.to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session.clone(),
        };
        self.events
            .send(AgentEvent::Finished(outcome.clone()).into())
            .await?;
        Ok(outcome)
    }
    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }
    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}
