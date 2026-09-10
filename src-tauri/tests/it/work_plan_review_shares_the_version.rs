//! WP-05: tester sądzi przypiętą wersję planu i dokładne bajty pracy, które dostał.

// 2026-09-08 (WP-05) — brak elementu w kontrolowanej fiksturze oznacza wadę samej sondy;
// `expect` nie trafia do kodu produkcyjnego.
#![allow(clippy::expect_used)]
// 2026-09-08 (WP-05) — pełny bieg jest jedną fiksturą tożsamości planu i pracy. Rozcięcie
// ukryłoby moment między złożeniem wejścia testera a jego odpowiedzią.
#![allow(clippy::too_many_lines)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
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
use loadout_lib::work_plan::{
    AcceptanceCriterion, Basis, Configuration, Origin, PlanVersion, Requirement, Status, StillOpen,
    about_other_work, approved, carry, conflicts, frozen_criteria, not_this_product,
    told_what_is_still_open,
};
use loadout_lib::workflow::criteria::{self, Criterion, Method};
use serde_json::{Value, json};
use tokio::sync::mpsc;

const CREATE: &str = "WP05-CREATE";
const IMPLEMENT: &str = "WP05-IMPLEMENT";
const CHECK: &str = "WP05-CHECK";
const UNCHANGED: &str = "WP05-UNCHANGED";

fn plan_candidate() -> Value {
    json!({
        "goal": "Keep implementation and review on one basis.",
        "inScope": ["The reviewed product"],
        "outOfScope": [],
        "requirements": [],
        "acceptance": [],
        "decisions": [],
        "sections": {
            "Implementation": "Build the requested product.",
            "Validation": "Check the same product bytes."
        },
        "proposals": [],
        "assumptions": [],
        "questions": [],
        "conflicts": [],
        "sources": []
    })
}

fn candidate_path(cwd: &Path, prompt: &str) -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
    let relative = prompt
        .lines()
        .find_map(|line| line.strip_prefix("LOADOUT_PLAN_CANDIDATE_PATH="))
        .ok_or("the Create prompt did not name its candidate path")?;
    Ok(cwd.join(relative))
}

fn write_candidate(spec: &RunSpec) -> Result<(), Box<dyn Error + Send + Sync>> {
    let path = candidate_path(&spec.cwd, &spec.prompt)?;
    fs::create_dir_all(path.parent().ok_or("the candidate has no parent")?)?;
    fs::write(path, serde_json::to_vec(&plan_candidate())?)?;
    Ok(())
}

fn change_the_work_after_it_was_given_to_the_tester(
    cwd: &Path,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    fs::write(
        cwd.join("changed-after-review-started.txt"),
        "different work\n",
    )?;
    Ok(())
}

#[derive(Clone)]
struct Driver {
    prompts: Option<Arc<Mutex<Vec<String>>>>,
}

#[async_trait]
impl AgentDriver for Driver {
    fn id(&self) -> &'static str {
        "claude"
    }

    // 2026-09-08 (WP-05) — krok z przypiętym planem dostaje most jako konfigurację Claude'a;
    // dubler nie uruchamia CLI, ale musi zachować produkcyjną drogę aż do `start`.
    fn configured(
        &self,
        _configuration: &loadout_lib::engine::drivers::DriverConfiguration,
    ) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }

    // 2026-09-08 (WP-05) — dubler nazywa się jak produkcyjny vendor, więc musi przyjąć cel
    // prywatnego dowodu; sam zapis dowodu nie jest przedmiotem testu podstawy oceny.
    fn with_evidence(
        &self,
        _target: loadout_lib::evidence::EvidenceTarget,
    ) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
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
                .push(spec.prompt.clone());
        }
        let text = if spec.prompt.contains(CREATE) {
            write_candidate(&spec).map_err(anyhow::Error::msg)?;
            "## Answer\nThe shared plan is ready.\n\n## Evidence\nThe candidate was written.\n\n## Open\nNone.\n"
                .to_owned()
        } else if spec.prompt.contains(IMPLEMENT) {
            fs::write(spec.cwd.join("product.txt"), "the reviewed product\n")?;
            "## Answer\nThe product is ready.\n\n## Evidence\nproduct.txt\n\n## Open\nNone.\n"
                .to_owned()
        } else if spec.prompt.contains(CHECK) {
            // 2026-09-08 (WP-05) — zmiana następuje po złożeniu promptu, czyli dokładnie
            // między zamrożeniem podstawy oceny i przyjęciem werdyktu.
            change_the_work_after_it_was_given_to_the_tester(&spec.cwd)
                .map_err(anyhow::Error::msg)?;
            "## Answer\nEverything passed.\n\n## Evidence\nThe running application worked.\n\n## Open\nNone.\n\ncriterion R1: passed via full runtime — observed the product\noutcome: pass\n"
                .to_owned()
        } else if spec.prompt.contains(UNCHANGED) {
            "## Answer\nThe unchanged path ran.\n\n## Evidence\nThe controlled test passed.\n\n## Open\nNone.\n\ncriterion R1: passed via automated test — observed the result\noutcome: pass\n"
                .to_owned()
        } else {
            anyhow::bail!("the fixture received an unknown step prompt")
        };
        Ok(Box::new(Handle {
            text,
            events,
            session: SessionRef {
                vendor: "claude",
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

    async fn send(&mut self, _message: String) -> anyhow::Result<()> {
        anyhow::bail!("this controlled driver takes no extra turns")
    }

    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        let outcome = Outcome {
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
            .send(AgentEvent::Finished(outcome.clone()).into())
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

fn row_for_node<'a>(run: &'a Value, node_key: &str) -> Result<&'a Value, Box<dyn Error>> {
    run.get("steps")
        .and_then(Value::as_array)
        .and_then(|steps| {
            steps
                .iter()
                .find(|step| step.get("node_key").and_then(Value::as_str) == Some(node_key))
        })
        .ok_or_else(|| format!("run.json has no step with node key {node_key}").into())
}

fn comparable_step_row(row: &Value) -> Value {
    let mut comparable = row.clone();
    let object = comparable
        .as_object_mut()
        .expect("a saved step row is a JSON object");
    // 2026-09-08 (WP-05) — dwa fizyczne węzły muszą mieć własną tożsamość i czas. Po ich
    // wyzerowaniu porównanie bajtów obejmuje wszystkie fakty produktu, także `plan_version`.
    for field in [
        "agent_session_id",
        "depends_on",
        "ended_at",
        "id",
        "node_key",
        "output",
        "started_at",
    ] {
        object.insert(field.to_owned(), Value::Null);
    }
    comparable
}

fn comparable_prompt(prompt: &str) -> String {
    prompt
        .lines()
        .map(|line| {
            let Some((label, address)) = line.split_once(": ") else {
                return line.to_owned();
            };
            let Some((_path, meaning)) = address.split_once(" (") else {
                return line.to_owned();
            };
            if !address.contains("/handoffs/") {
                return line.to_owned();
            }
            // 2026-09-08 (WP-05) — osobne gałęzie muszą wskazywać dwa fizyczne pliki. Tylko
            // ich adres jest zmienny; reszta promptu pozostaje porównywana bajt w bajt.
            format!("{label}: <handoff> ({meaning}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn criterion(id: &str, behaviour: &str, method: Method) -> Criterion {
    Criterion {
        id: id.to_owned(),
        behaviour: behaviour.to_owned(),
        required: true,
        method,
    }
}

fn basis(version: &str, node: &str, digest: &str) -> Basis {
    Basis {
        version_id: version.to_owned(),
        work: BTreeMap::from([(node.to_owned(), digest.to_owned())]),
    }
}

fn plan_with(requirement: Requirement) -> PlanVersion {
    let mut version = PlanVersion {
        version_id: "plan-v1".to_owned(),
        ..PlanVersion::default()
    };
    version.document.requirements.push(requirement);
    version
}

#[test]
fn use_does_not_check_the_plan_without_the_explicit_setting() {
    let configuration = Configuration::from_step(Some(&json!({ "mode": "use" })))
        .expect("the Use setting is valid");

    assert!(
        !configuration.check_plan(),
        "Plan Use silently became a request to judge the plan"
    );
}

#[test]
fn the_same_plan_id_does_not_cover_another_commit() {
    let frozen = basis("plan-v1", "implementation", "commit-a");
    let now = basis("plan-v1", "implementation", "commit-b");
    let approved = vec![criterion("R1", "The product works.", Method::FullRuntime)];
    let mut judged = criteria::judge(
        &approved,
        "criterion R1: passed via full runtime — observed the product",
    );
    let shown = about_other_work(&frozen, &now).expect("only the work digest changed");
    not_this_product(&approved, &mut judged);
    let history = format!("{shown} {}", judged.said());

    assert!(
        history.contains("different work"),
        "History did not identify the changed product: {history}"
    );
    assert!(
        history.contains("could not measure") && history.contains("R1"),
        "History counted an answer about another product as a measured pass: {history}"
    );
}

#[test]
fn the_same_work_does_not_cover_another_plan_version() {
    let frozen = basis("plan-v1", "implementation", "work-a");
    let now = basis("plan-v2", "implementation", "work-a");
    let shown = about_other_work(&frozen, &now).expect("only the plan version changed");

    assert!(
        shown.contains("plan-v1") && shown.contains("plan-v2"),
        "History did not name both plan versions: {shown}"
    );
}

#[test]
fn plan_requirements_keep_the_existing_method_and_not_tested_rules() {
    let version = plan_with(Requirement {
        id: "R1".to_owned(),
        text: "A person can finish the flow.".to_owned(),
        acceptance: vec![AcceptanceCriterion {
            text: "Finish it in the application.".to_owned(),
            verification: "the running application with its real backend".to_owned(),
        }],
        origin: Origin::Human,
        status: Status::Agreed,
    });
    let from_plan = frozen_criteria(&version);
    let weaker = criteria::judge(
        &from_plan,
        "criterion R1: passed via mocked ui — the stand-in screen changed",
    )
    .said();
    let unmeasured =
        criteria::judge(&from_plan, "criterion R1: not tested — no application").said();

    assert!(
        weaker.contains("weaker way than the one agreed") && weaker.contains("R1"),
        "The plan requirement bypassed the existing method rule: {weaker}"
    );
    assert!(
        unmeasured.contains("could not measure") && unmeasured.contains("R1"),
        "A report without a measurement was presented as a pass: {unmeasured}"
    );
}

#[test]
fn a_workflow_requirement_wins_a_visible_conflict_with_the_plan() {
    let workflow = vec![criterion(
        "R1",
        "The approved workflow wording.",
        Method::AutomatedTest,
    )];
    let from_plan = vec![criterion(
        "R1",
        "The different plan wording.",
        Method::FullRuntime,
    )];
    let shown = conflicts(&workflow, &from_plan).join(" ");
    let judged = approved(&workflow, &from_plan);

    assert!(
        shown.contains("The approved workflow wording.")
            && shown.contains("The different plan wording."),
        "The conflict did not show both sides to the person: {shown}"
    );
    assert_eq!(
        judged, workflow,
        "the plan silently replaced the requirement approved in the workflow"
    );
}

#[test]
fn silence_does_not_close_a_finding_from_the_previous_try() {
    let agreed = vec![criterion("R1", "The product works.", Method::AutomatedTest)];
    let first = criteria::judge(
        &agreed,
        "criterion R1: failed via automated test — the flow stopped",
    );
    let previous = StillOpen::from_judgement(&first);
    let reminder = told_what_is_still_open(&previous);
    let mut second = criteria::judge(&agreed, "outcome: pass");
    let shown = carry(&previous, &mut second).expect("R1 was omitted on the second try");
    let history = format!("{shown} {}", second.said());

    assert!(
        reminder.contains("R1") && !reminder.contains("the flow stopped"),
        "The next prompt lost the ID or copied the old detailed report: {reminder}"
    );
    assert!(
        history.contains("open from the previous try") && history.contains("R1"),
        "Silence closed the previous finding: {history}"
    );
}

#[tokio::test]
async fn check_plan_off_asks_and_judges_byte_for_byte_as_before() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let project = root.path().canonicalize()?.join("project");
    let home = root.path().canonicalize()?.join("home");
    fs::create_dir_all(project.join(".loadout"))?;
    fs::create_dir_all(project.join(".loadout/workflows"))?;

    let mut agent = Agent::example();
    agent.write_results_to.clear();
    write_agent_file(&project.join(".loadout/agents"), &agent, None)?;
    let workflow = project.join(".loadout/workflows/wp-05-off.json");
    fs::write(
        &workflow,
        serde_json::to_vec_pretty(&json!({
            "format": 1,
            "id": "wp-05-off",
            "name": "Keep the disabled path unchanged",
            "steps": [
                {
                    "kind": "agent", "id": "planner", "name": "Planner", "agent": agent.id,
                    "instructions": CREATE, "overrides": {}, "folder": {"use":"fresh-copy"},
                    "plan": {"mode":"create"},
                    "criteria": [{
                        "id": "R1", "behaviour": "The existing workflow still works.",
                        "required": true, "method": "automated-test"
                    }],
                    "at": {"x":0,"y":0}
                },
                {
                    "kind": "agent", "id": "implementation-without", "name": "Implementation",
                    "agent": agent.id, "instructions": IMPLEMENT, "overrides": {},
                    "folder": {"use":"fresh-copy"}, "plan": {"mode":"use"},
                    "at": {"x":200,"y":-100}
                },
                {
                    "kind": "agent", "id": "tester-without", "name": "Tester", "agent": agent.id,
                    "instructions": UNCHANGED, "overrides": {}, "folder": {"use":"same-copy"},
                    "plan": {"mode":"use"},
                    "criteria": [{
                        "id": "R1", "behaviour": "The existing workflow still works.",
                        "required": true, "method": "automated-test"
                    }],
                    "at": {"x":400,"y":-100}
                },
                {
                    "kind": "agent", "id": "implementation-explicit", "name": "Implementation",
                    "agent": agent.id, "instructions": IMPLEMENT, "overrides": {},
                    "folder": {"use":"fresh-copy"}, "plan": {"mode":"use"},
                    "at": {"x":200,"y":100}
                },
                {
                    "kind": "agent", "id": "tester-explicit", "name": "Tester", "agent": agent.id,
                    "instructions": UNCHANGED, "overrides": {}, "folder": {"use":"same-copy"},
                    "plan": {"mode":"use","checkPlan":false},
                    "criteria": [{
                        "id": "R1", "behaviour": "The existing workflow still works.",
                        "required": true, "method": "automated-test"
                    }],
                    "at": {"x":400,"y":100}
                }
            ],
            "links": [
                {"from":"planner","to":"implementation-without"},
                {"from":"implementation-without","to":"tester-without"},
                {"from":"tester-without","to":"implementation-without","max_turns":1},
                {"from":"planner","to":"implementation-explicit"},
                {"from":"implementation-explicit","to":"tester-explicit"},
                {"from":"tester-explicit","to":"implementation-explicit","max_turns":1}
            ]
        }))?,
    )?;

    let prompts = Arc::new(Mutex::new(Vec::new()));
    let driver: Arc<dyn AgentDriver> = Arc::new(Driver {
        prompts: Some(Arc::clone(&prompts)),
    });
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));
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
                how_many_at_once: 1,
                task: None,
                part: None,
                handoffs_from: None,
            },
            sink,
        ),
    )
    .await??;

    let use_prompts = prompts
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|prompt| prompt.contains(UNCHANGED))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        use_prompts.len(),
        2,
        "both disabled forms must reach the controlled driver: {use_prompts:?}"
    );
    assert_eq!(
        comparable_prompt(&use_prompts[0]),
        comparable_prompt(&use_prompts[1]),
        "omitting checkPlan and spelling checkPlan: false changed the prompt"
    );

    let run: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    let without = comparable_step_row(row_for_node(&run, "tester-without")?);
    let explicit = comparable_step_row(row_for_node(&run, "tester-explicit")?);
    assert_eq!(
        without.get("round_outcome").and_then(Value::as_str),
        Some("pass"),
        "the tester without checkPlan was not judged: {without}"
    );
    assert_eq!(
        explicit.get("round_outcome").and_then(Value::as_str),
        Some("pass"),
        "the tester with checkPlan: false was not judged: {explicit}"
    );
    assert_eq!(
        serde_json::to_vec(&without)?,
        serde_json::to_vec(&explicit)?,
        "the disabled setting changed the saved step row: without={without}, explicit={explicit}"
    );
    Ok(())
}

#[tokio::test]
async fn the_saved_run_says_the_tester_checked_other_work() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let project = root.path().canonicalize()?.join("project");
    let home = root.path().canonicalize()?.join("home");
    fs::create_dir_all(project.join(".loadout"))?;
    fs::create_dir_all(project.join(".loadout/workflows"))?;

    let mut agent = Agent::example();
    agent.write_results_to.clear();
    write_agent_file(&project.join(".loadout/agents"), &agent, None)?;
    let workflow = project.join(".loadout/workflows/wp-05.json");
    fs::write(
        &workflow,
        serde_json::to_vec_pretty(&json!({
            "format": 1,
            "id": "wp-05",
            "name": "Review the shared plan",
            "steps": [
                {
                    "kind": "agent", "id": "planner", "name": "Planner", "agent": agent.id,
                    "instructions": CREATE, "overrides": {}, "folder": {"use":"fresh-copy"},
                    "plan": {"mode":"create"},
                    "criteria": [{
                        "id": "R1", "behaviour": "The product works for a person.",
                        "required": true, "method": "full-runtime"
                    }],
                    "at": {"x":0,"y":0}
                },
                {
                    "kind": "agent", "id": "implementation", "name": "Implementation",
                    "agent": agent.id, "instructions": IMPLEMENT, "overrides": {},
                    "folder": {"use":"fresh-copy"}, "plan": {"mode":"use"},
                    "at": {"x":200,"y":0}
                },
                {
                    "kind": "agent", "id": "tester", "name": "Tester", "agent": agent.id,
                    "instructions": CHECK, "overrides": {}, "folder": {"use":"same-copy"},
                    "plan": {"mode":"use","checkPlan":true},
                    "at": {"x":400,"y":0}
                }
            ],
            "links": [
                {"from":"planner","to":"implementation"},
                {"from":"implementation","to":"tester"},
                {"from":"tester","to":"implementation","max_turns":1}
            ]
        }))?,
    )?;

    let driver: Arc<dyn AgentDriver> = Arc::new(Driver { prompts: None });
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));
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
                how_many_at_once: 1,
                task: None,
                part: None,
                handoffs_from: None,
            },
            sink,
        ),
    )
    .await??;
    let run: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    let tester = row_for_node(&run, "tester")?;
    let error = tester
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        error.contains("different work") || error.contains("other work"),
        "History does not say that the tester answered about work other than the frozen product: {error:?}"
    );
    assert_eq!(
        tester.get("round_outcome").and_then(Value::as_str),
        Some("fail"),
        "the loop accepted a report after the reviewed product changed"
    );
    assert!(
        tester
            .pointer("/plan_version/work")
            .and_then(Value::as_str)
            .is_some_and(|work| work.len() == 64),
        "run.json did not retain the reviewed work fingerprint: {tester}"
    );
    Ok(())
}
