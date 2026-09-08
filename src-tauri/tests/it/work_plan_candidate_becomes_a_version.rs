//! WP-03: kandydat napisany przez zwykły krok staje się wersją dopiero po jego zakończeniu.
//!
//! Etapy porażki są rozdzielone: przed startem (brak wymaganego rodzica), w żywej sesji
//! (zły schemat i poprawka), przy zejściu procesu (Stop, timeout, błąd vendora), w kontrakcie
//! wyniku (brak pola), przy dostarczeniu (brak pliku) i w publikacji (stary rodzic, zakres,
//! chronione wymaganie). Każdy etap ma niżej własną asercję na pierwszy widoczny skutek.

#![allow(clippy::too_many_lines)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use loadout_lib::bridge::host::Answers;
use loadout_lib::bridge::work_plan::PlanDesk;
use loadout_lib::bridge::{Answer, Call};
use loadout_lib::commands::run::{run_workflow_with_reflection, stop_run_inner};
use loadout_lib::commands::{Drivers, Outcome, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::AgentDriver;
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::step::StepState;
use loadout_lib::library::agents::{Agent, Vendor, write_agent_file};
use loadout_lib::store::Store;
use loadout_lib::work_plan::{
    Configuration, Error as PlanError, HumanRequirement, Mode, PlanVersion, Publication,
    SectionKey, Stamp, read_current_version, read_versions,
};
use loadout_lib::{commands::processes::Processes, ipc::line_channel};
use serde_json::json;
use tokio_util::sync::CancellationToken;

const CREATE: &str = r#"{
  "goal": "Keep checkout editing predictable.",
  "inScope": ["Checkout editing"],
  "outOfScope": ["Payment authorization"],
  "requirements": [],
  "acceptance": [],
  "decisions": ["Amounts remain in PLN."],
  "sections": {
    "Implementation": "Keep the existing checkout state machine.",
    "Design": "Show the discount next to the total."
  },
  "proposals": [],
  "assumptions": [],
  "questions": [],
  "conflicts": [],
  "sources": []
}"#;

const UPDATE: &str = r#"{
  "baseVersion": 1,
  "result": "changes",
  "changes": {
    "sections": {
      "Design": "Show the discount directly below the total."
    }
  }
}"#;

const CLAUDE_STDOUT: &str = concat!(
    r#"{"type":"system","subtype":"init","session_id":"01990000-0000-7000-8000-000000000301","model":"sonnet","tools":[]}"#,
    "\n",
    r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"The plan candidate is ready."}]}}"#,
    "\n",
    r#"{"type":"result","subtype":"success","is_error":false,"num_turns":1,"duration_ms":7,"total_cost_usd":0.001,"result":"The plan candidate is ready."}"#,
    "\n",
);

const CLAUDE_CORRECTED_STDOUT: &str = concat!(
    r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"I corrected the plan candidate."}]}}"#,
    "\n",
    r#"{"type":"result","subtype":"success","is_error":false,"num_turns":1,"duration_ms":7,"total_cost_usd":0.001,"result":"I corrected the plan candidate."}"#,
    "\n",
);

const CODEX_STDOUT: &str = concat!(
    r#"{"type":"thread.started","thread_id":"wp-03-update"}"#,
    "\n",
    r#"{"type":"item.completed","item":{"type":"agent_message","text":"The plan update is ready."}}"#,
    "\n",
    r#"{"type":"turn.completed","usage":{"input_tokens":3,"cached_input_tokens":1,"output_tokens":2}}"#,
    "\n",
);

const CLAUDE_FAILED_STDOUT: &str = concat!(
    r#"{"type":"system","subtype":"init","session_id":"01990000-0000-7000-8000-000000000302","model":"sonnet","tools":[]}"#,
    "\n",
    r#"{"type":"result","subtype":"error","is_error":true,"num_turns":1,"duration_ms":7,"total_cost_usd":0.001,"result":"The agent could not finish the plan."}"#,
    "\n",
);

fn shell_fixture(candidate: Option<&str>, stdout: &str, log: &str, keeps_stdin: bool) -> String {
    let read = if keeps_stdin {
        "IFS= read -r sent"
    } else {
        "sent=\"$(cat)\""
    };
    let write_candidate = candidate.map_or_else(String::new, |candidate| {
        format!(
            "candidate_path=\"$(printf '%s\\n' \"$sent\" | sed -n \
             's/.*LOADOUT_PLAN_CANDIDATE_PATH=\\([^\\\\\\\" ]*\\).*/\\1/p')\"\n\
             if [ -n \"$candidate_path\" ]; then\n\
             mkdir -p \"$(dirname \"$candidate_path\")\"\n\
             printf '%s\\n' '{{\"goal\":7}}' > \"$candidate_path\"\n\
             printf '%s\\n' '{candidate}' > \"$candidate_path\"\n\
             fi"
        )
    });
    format!(
        r#"#!/bin/sh
if [ "${{1-}}" = "--version" ]; then
  printf '%s\n' 'fixture-cli 1.0'
  exit 0
fi
{read}
here="$(dirname "$0")"
printf '%s\n' "$sent" >> "$here/{log}"
{write_candidate}
printf '%s' '{stdout}'
exit 0
"#
    )
}

fn hanging_fixture(marker: &Path, write_before_wait: bool, write_after_signal: bool) -> String {
    let before = write_before_wait
        .then_some(format!("printf '%s\\n' '{CREATE}' > \"$candidate_path\""))
        .unwrap_or_default();
    let after = write_after_signal
        .then_some(format!(
            "late_candidate() {{\n  printf '%s\\n' '{CREATE}' > \"$candidate_path\"\n  exit 0\n}}\ntrap late_candidate TERM"
        ))
        .unwrap_or_default();
    format!(
        r#"#!/bin/sh
if [ "${{1-}}" = "--version" ]; then
  printf '%s\n' 'fixture-cli 1.0'
  exit 0
fi
IFS= read -r sent
candidate_path="$(printf '%s\n' "$sent" | sed -n 's/.*LOADOUT_PLAN_CANDIDATE_PATH=\([^\\\" ]*\).*/\1/p')"
mkdir -p "$(dirname "$candidate_path")"
{before}
{after}
printf '%s\n' ready > "{}"
printf '%s\n' '{{"type":"system","subtype":"init","session_id":"01990000-0000-7000-8000-000000000303","model":"sonnet","tools":[]}}'
while :; do sleep 60; done
"#,
        marker.display()
    )
}

fn correcting_claude_fixture(log: &str) -> String {
    format!(
        r#"#!/bin/sh
if [ "${{1-}}" = "--version" ]; then
  printf '%s\n' 'fixture-cli 1.0'
  exit 0
fi
IFS= read -r sent
here="$(dirname "$0")"
printf '%s\n' "$sent" >> "$here/{log}"
candidate_path="$(printf '%s\n' "$sent" | sed -n 's/.*LOADOUT_PLAN_CANDIDATE_PATH=\([^\" ]*\).*/\1/p')"
mkdir -p "$(dirname "$candidate_path")"
printf '%s\n' '{{"goal":7}}' > "$candidate_path"
printf '%s' '{CLAUDE_STDOUT}'
IFS= read -r correction
printf '%s\n' "$correction" >> "$here/{log}"
printf '%s\n' '{CREATE}' > "$candidate_path"
printf '%s' '{CLAUDE_CORRECTED_STDOUT}'
exit 0
"#
    )
}

fn executable(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn agent(home: &Path, name: &str, vendor: Vendor) -> Result<Agent, Box<dyn Error>> {
    let mut agent = Agent::example();
    agent.id = uuid::Uuid::now_v7();
    name.clone_into(&mut agent.name);
    agent.runs_with = vendor;
    agent.write_results_to.clear();
    write_agent_file(&home.join("agents"), &agent, None)?;
    Ok(agent)
}

fn timed_agent(home: &Path, name: &str, minutes: u32) -> Result<Agent, Box<dyn Error>> {
    let mut saved = Agent::example();
    saved.id = uuid::Uuid::now_v7();
    name.clone_into(&mut saved.name);
    saved.runs_with = Vendor::ClaudeCode;
    saved.write_results_to.clear();
    saved.give_up_after_minutes = minutes;
    write_agent_file(&home.join("agents"), &saved, None)?;
    Ok(saved)
}

fn one_step_workflow(home: &Path, saved: &Agent, name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let workflow = home.join(format!("workflows/{name}.json"));
    fs::create_dir_all(workflow.parent().ok_or("the workflow has no parent")?)?;
    fs::write(
        &workflow,
        serde_json::to_vec_pretty(&json!({
            "format": 1,
            "id": name,
            "name": "Create a plan",
            "steps": [{
                "kind": "agent",
                "id": "create",
                "name": "Create",
                "agent": saved.id,
                "instructions": "Create the shared plan.",
                "plan": {"mode":"create"},
                "overrides": {},
                "at": {"x":0,"y":0}
            }],
            "links": []
        }))?,
    )?;
    Ok(workflow)
}

async fn wait_for_file(path: &Path) -> Result<(), Box<dyn Error>> {
    for _ in 0..200 {
        if path.is_file() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(format!("the controlled process did not create {}", path.display()).into())
}

/// Kryteria, które człowiek naprawdę wypełnia w panelu kroku, mają identyfikatory `c1`, `c2`
/// (`src/sections/workflows/step-panel/criteria-row.tsx`) — nie `R1`. Nic po drodze ich nie
/// przepisuje: `job.criteria` to niezmieniona kopia z pliku workflow.
///
/// 2026-09-08 — DO TEGO DNIA KAŻDY taki krok `Create` kończył się `Malformed` i nie publikował
/// ANI JEDNEJ wersji, bo walidator żądał litery `R` i samych cyfr. Zlecenie mówi „stabilne
/// identyfikatory, **np.** R1" — przykład był zamieniony w regułę. Ten test pilnuje, żeby
/// kształt identyfikatora z panelu wystarczał, a jedyne, czego naprawdę wymagamy — stałość
/// i jedyność — dalej obowiązywało.
#[tokio::test]
async fn criteria_identifiers_from_the_panel_reach_a_published_version()
-> Result<(), Box<dyn Error>> {
    let storage = tempfile::tempdir()?;
    let root = storage.path().join("plan");
    let create = configuration(&json!({"mode":"create"}))?.prepare(
        root.clone(),
        PathBuf::from("candidate.json"),
        vec![
            HumanRequirement {
                id: "c1".to_owned(),
                text: "The promo code must survive a quantity change.".to_owned(),
                acceptance: Vec::new(),
            },
            HumanRequirement {
                id: "c2".to_owned(),
                text: "The total must stay in PLN.".to_owned(),
                acceptance: Vec::new(),
            },
        ],
    )?;

    let first = version(create.publish(&stamp("create", None), CREATE.as_bytes())?);
    assert_eq!(first.version, 1);
    let kept: Vec<&str> = first
        .document
        .requirements
        .iter()
        .map(|requirement| requirement.id.as_str())
        .collect();
    assert_eq!(
        kept,
        vec!["c1", "c2"],
        "identifiers written by the panel have to survive into the published version"
    );

    // Jedyność zostaje wymagana — to na niej stoi „aktualizacja jednej sekcji nie renumeruje
    // pozostałych", więc rozluźnienie kształtu nie ma prawa jej zdjąć.
    // WŁASNY korzeń: w tamtym stoi już wersja 1, a publikacja bez rodzica słusznie odbija się
    // wtedy o strażnika konfliktu (`Stale`) — czyli mierzyłaby co innego niż jedyność.
    let second_storage = tempfile::tempdir()?;
    let repeated = configuration(&json!({"mode":"create"}))?.prepare(
        second_storage.path().join("plan"),
        PathBuf::from("repeated.json"),
        vec![
            HumanRequirement {
                id: "c1".to_owned(),
                text: "One.".to_owned(),
                acceptance: Vec::new(),
            },
            HumanRequirement {
                id: "c1".to_owned(),
                text: "Two.".to_owned(),
                acceptance: Vec::new(),
            },
        ],
    )?;
    assert!(
        repeated
            .publish(&stamp("repeated", None), CREATE.as_bytes())
            .is_err(),
        "two requirements with the same identifier still have to be refused"
    );
    Ok(())
}

fn configuration(value: &serde_json::Value) -> Result<Configuration, Box<dyn Error>> {
    Ok(Configuration::from_step(Some(value))?)
}

fn stamp(operation: &str, parent: Option<&PlanVersion>) -> Stamp {
    Stamp {
        run_id: "run-wp-03".to_owned(),
        document_id: "workflow-plan".to_owned(),
        step_id: format!("step-{operation}"),
        attempt: 1,
        operation: operation.to_owned(),
        parent: parent.map(|version| version.version_id.clone()),
        at: "2026-09-08T12:00:00Z".to_owned(),
    }
}

fn version(publication: Publication) -> PlanVersion {
    match publication {
        Publication::Published(version)
        | Publication::Unchanged(version)
        | Publication::AlreadyPublished(version) => version,
    }
}

async fn one_create_run(
    candidate: Option<&str>,
    stdout: &str,
    required_field: bool,
) -> Result<(Vec<StepState>, String, usize), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    let fixtures = tempfile::tempdir()?;
    fs::create_dir_all(project.path().join(".loadout"))?;
    let saved = agent(home.path(), "Plan creator", Vendor::ClaudeCode)?;
    let claude = executable(
        fixtures.path(),
        "claude",
        &shell_fixture(candidate, stdout, "claude.stdin", true),
    )?;
    let driver: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(claude));
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));
    let workflow = home.path().join("workflows/create.json");
    fs::create_dir_all(workflow.parent().ok_or("the workflow has no parent")?)?;
    let mut step = json!({
        "kind": "agent",
        "id": "create",
        "name": "Create",
        "agent": saved.id,
        "instructions": "Create the shared plan.",
        "plan": {"mode":"create"},
        "overrides": {},
        "at": {"x":0,"y":0}
    });
    if required_field {
        step["handover"] = json!({"kind":"form","fields":[{
            "name":"decision",
            "describe":"The decision made.",
            "required":true
        }]});
    }
    fs::write(
        &workflow,
        serde_json::to_vec_pretty(&json!({
            "format": 1,
            "id": "create-plan",
            "name": "Create a plan",
            "steps": [step],
            "links": []
        }))?,
    )?;
    let store = Store::open(&project.path().join(".loadout/loadout.db"))?;
    let deps = RunDeps {
        home: home.path(),
        project: project.path(),
        store: &store,
        drivers,
        processes: Arc::new(Processes::new()),
        control: RunControl::new(),
    };
    let (sink, _lines) = line_channel(1024);
    let report = run_workflow_with_reflection(
        &deps,
        &RunRequest {
            workflow,
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        },
        sink,
        None,
        false,
    )
    .await?;
    let run_file = fs::read_to_string(report.dir.join("run.json"))?;
    let versions = read_versions(&report.dir.join("plans/workflow-plan"))?.len();
    Ok((report.steps, run_file, versions))
}

#[tokio::test]
async fn exit_zero_without_a_valid_candidate_is_a_named_step_failure() -> Result<(), Box<dyn Error>>
{
    let (valid_steps, valid_run, valid_versions) =
        one_create_run(Some(CREATE), CLAUDE_STDOUT, false).await?;
    assert_eq!(valid_steps, vec![StepState::Succeeded]);
    assert_eq!(valid_versions, 1);
    assert!(
        valid_run.contains(r#""plan_version""#)
            && valid_run.contains(r#""documentId": "workflow-plan""#),
        "the successful step did not record which complete plan it produced: {valid_run}"
    );

    let (missing_steps, missing_run, missing_versions) =
        one_create_run(None, CLAUDE_STDOUT, false).await?;
    assert_eq!(missing_steps, vec![StepState::Failed]);
    assert_eq!(missing_versions, 0);
    assert!(
        missing_run.contains(
            "This step did not deliver a valid plan document: the candidate file is missing. No plan was published."
        ),
        "the person cannot see why an exit-zero step failed: {missing_run}"
    );

    let (failed_steps, failed_run, failed_versions) =
        one_create_run(Some(CREATE), CLAUDE_FAILED_STDOUT, false).await?;
    assert_eq!(failed_steps, vec![StepState::Failed]);
    assert_eq!(failed_versions, 0);
    assert!(
        failed_run.contains("The agent could not finish the plan"),
        "the controlled process did not reach the failed-completion path: {failed_run}"
    );

    let (contract_steps, contract_run, contract_versions) =
        one_create_run(Some(CREATE), CLAUDE_STDOUT, true).await?;
    assert_eq!(contract_steps, vec![StepState::Failed]);
    assert_eq!(contract_versions, 0);
    assert!(
        contract_run.contains("was asked to hand back"),
        "the fixture did not reach the configured-result refusal: {contract_run}"
    );
    Ok(())
}

#[tokio::test]
async fn a_failed_plan_author_is_not_bypassed_by_carry_on() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    let fixtures = tempfile::tempdir()?;
    fs::create_dir_all(project.path().join(".loadout"))?;
    let saved = agent(home.path(), "Plan worker", Vendor::ClaudeCode)?;
    let binary = executable(
        fixtures.path(),
        "claude",
        &shell_fixture(None, CLAUDE_STDOUT, "claude.stdin", true),
    )?;
    let driver: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(binary));
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));
    let workflow = home.path().join("workflows/no-fallback.json");
    fs::create_dir_all(workflow.parent().ok_or("the workflow has no parent")?)?;
    fs::write(
        &workflow,
        serde_json::to_vec_pretty(&json!({
            "format":1,
            "id":"no-fallback",
            "name":"Do not substitute a plan",
            "steps":[
                {"kind":"agent","id":"create","name":"Create","agent":saved.id,"instructions":"Create it.","plan":{"mode":"create"},"overrides":{},"at":{"x":0,"y":0}},
                {"kind":"agent","id":"use","name":"Use","agent":saved.id,"instructions":"Use it.","plan":{"mode":"use"},"overrides":{},"at":{"x":200,"y":0}}
            ],
            "links":[{"from":"create","to":"use"}]
        }))?,
    )?;
    let store = Store::open(&project.path().join(".loadout/loadout.db"))?;
    let deps = RunDeps {
        home: home.path(),
        project: project.path(),
        store: &store,
        drivers,
        processes: Arc::new(Processes::new()),
        control: RunControl::new(),
    };
    let (sink, _lines) = line_channel(1024);
    let report = run_workflow_with_reflection(
        &deps,
        &RunRequest {
            workflow,
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        },
        sink,
        None,
        false,
    )
    .await?;
    assert_eq!(report.steps, vec![StepState::Failed, StepState::Failed]);
    let run_file = fs::read_to_string(report.dir.join("run.json"))?;
    assert!(
        run_file.contains("Create did not finish the plan version it was meant to provide"),
        "the Use step silently fell back to another plan: {run_file}"
    );
    assert_eq!(
        fs::read_to_string(fixtures.path().join("claude.stdin"))?
            .lines()
            .count(),
        1,
        "the Use process started even though its exact plan source failed"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_refuses_even_a_candidate_written_after_the_signal() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    let fixtures = tempfile::tempdir()?;
    fs::create_dir_all(project.path().join(".loadout"))?;
    let ready = fixtures.path().join("ready");
    let saved = timed_agent(home.path(), "Late plan creator", 0)?;
    let binary = executable(
        fixtures.path(),
        "claude",
        &hanging_fixture(&ready, false, true),
    )?;
    let driver: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(binary));
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));
    let workflow = one_step_workflow(home.path(), &saved, "late-stop")?;
    let store = Store::open(&project.path().join(".loadout/loadout.db"))?;
    let deps = RunDeps {
        home: home.path(),
        project: project.path(),
        store: &store,
        drivers,
        processes: Arc::new(Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (sink, _lines) = line_channel(1024);
    let run = run_workflow_with_reflection(&deps, &request, sink, None, false);
    tokio::pin!(run);
    let ready_to_stop = wait_for_file(&ready);
    tokio::pin!(ready_to_stop);
    tokio::select! {
        result = &mut ready_to_stop => result?,
        result = &mut run => return Err(format!("the run ended before Stop: {result:?}").into()),
    }
    let (stopped, finished) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(stop_run_inner(&deps), &mut run)
    })
    .await?;
    let report = finished?;
    assert_eq!(stopped?, Outcome::Cancelled);
    assert_eq!(report.steps, vec![StepState::Cancelled]);
    assert!(
        read_versions(&report.dir.join("plans/workflow-plan"))?.is_empty(),
        "the TERM trap wrote a late candidate and Stop published it"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn timeout_keeps_a_finished_candidate_unpublished() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    let fixtures = tempfile::tempdir()?;
    fs::create_dir_all(project.path().join(".loadout"))?;
    let ready = fixtures.path().join("ready");
    let saved = timed_agent(home.path(), "Timed plan creator", 1)?;
    let binary = executable(
        fixtures.path(),
        "claude",
        &hanging_fixture(&ready, true, false),
    )?;
    let driver: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(binary));
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));
    let workflow = one_step_workflow(home.path(), &saved, "plan-timeout")?;
    let store = Store::open(&project.path().join(".loadout/loadout.db"))?;
    let deps = RunDeps {
        home: home.path(),
        project: project.path(),
        store: &store,
        drivers,
        processes: Arc::new(Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (sink, _lines) = line_channel(1024);
    let run = run_workflow_with_reflection(&deps, &request, sink, None, false);
    tokio::pin!(run);
    let candidate_written = wait_for_file(&ready);
    tokio::pin!(candidate_written);
    tokio::select! {
        result = &mut candidate_written => result?,
        result = &mut run => return Err(format!("the run ended before its timeout: {result:?}").into()),
    }
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(61)).await;
    let report = (&mut run).await?;
    assert_eq!(report.steps, vec![StepState::Failed]);
    let run_file = fs::read_to_string(report.dir.join("run.json"))?;
    assert!(
        run_file.contains("This step ran longer than its 1 minute limit"),
        "the controlled process did not reach the timeout path: {run_file}"
    );
    assert!(read_versions(&report.dir.join("plans/workflow-plan"))?.is_empty());
    Ok(())
}

#[tokio::test]
async fn candidates_are_scoped_pinned_idempotent_and_read_only_where_required()
-> Result<(), Box<dyn Error>> {
    let storage = tempfile::tempdir()?;
    let root = storage.path().join("plan");
    let create = configuration(&json!({"mode":"create"}))?.prepare(
        root.clone(),
        PathBuf::from("candidate.json"),
        vec![HumanRequirement {
            id: "R1".to_owned(),
            text: "Keep this exact human requirement.".to_owned(),
            acceptance: Vec::new(),
        }],
    )?;
    let bad = create.publish(&stamp("bad-shape", None), br#"{"goal":7}"#);
    assert!(matches!(bad, Err(PlanError::Malformed(_))));
    assert!(read_versions(&root)?.is_empty());

    // Ten sam przydział reprezentuje jedną sesję: po złym zapisie agent poprawia plik,
    // a host publikuje dopiero ostatni, kompletny kształt.
    let first = version(create.publish(&stamp("create", None), CREATE.as_bytes())?);
    assert_eq!(first.version, 1);
    assert_eq!(
        first
            .document
            .requirement("R1")
            .map(|one| one.text.as_str()),
        Some("Keep this exact human requirement.")
    );

    let update_config = configuration(&json!({"mode":"update","canUpdate":["Design"]}))?;
    let stale = update_config.prepare(root.clone(), PathBuf::from("stale.json"), Vec::new())?;
    let update = update_config.prepare(root.clone(), PathBuf::from("update.json"), Vec::new())?;
    let update_stamp = stamp("update", update.current());
    let second = version(update.publish(&update_stamp, UPDATE.as_bytes())?);
    assert_eq!(second.version, 2);
    assert_eq!(
        second
            .document
            .requirement("R1")
            .map(|one| one.text.as_str()),
        Some("Keep this exact human requirement.")
    );
    assert!(matches!(
        update.publish(&update_stamp, UPDATE.as_bytes())?,
        Publication::AlreadyPublished(ref repeated) if repeated.version_id == second.version_id
    ));
    assert_eq!(read_versions(&root)?.len(), 2);

    let unchanged =
        update_config.prepare(root.clone(), PathBuf::from("unchanged.json"), Vec::new())?;
    let unchanged_json = format!(
        r#"{{"baseVersion":{},"result":"unchanged"}}"#,
        second.version
    );
    assert!(matches!(
        unchanged.publish(&stamp("unchanged", unchanged.current()), unchanged_json.as_bytes())?,
        Publication::Unchanged(ref current) if current.version_id == second.version_id
    ));
    assert_eq!(read_versions(&root)?.len(), 2);

    let stale_change =
        br#"{"baseVersion":1,"result":"changes","changes":{"sections":{"Design":"late"}}}"#;
    assert!(matches!(
        stale.publish(&stamp("stale", stale.current()), stale_change),
        Err(PlanError::Stale)
    ));
    let widened = update_config.prepare(root.clone(), PathBuf::from("widened.json"), Vec::new())?;
    let widened_change = format!(
        r#"{{"baseVersion":{},"result":"changes","changes":{{"scope":["Implementation"],"sections":{{"Implementation":"agent widened its own scope"}}}}}}"#,
        second.version
    );
    assert!(matches!(
        widened.publish(
            &stamp("widened", widened.current()),
            widened_change.as_bytes()
        ),
        Err(PlanError::OutOfScope)
    ));
    let protected =
        update_config.prepare(root.clone(), PathBuf::from("protected.json"), Vec::new())?;
    let protected_change = format!(
        r#"{{"baseVersion":{},"result":"changes","changes":{{"requirements":[{{"id":"R1","text":"changed"}}]}}}}"#,
        second.version
    );
    assert!(matches!(
        protected.publish(
            &stamp("protected", protected.current()),
            protected_change.as_bytes()
        ),
        Err(PlanError::WouldWeakenHumanRequirement)
    ));
    assert_eq!(read_current_version(&root)?.version_id, second.version_id);

    let use_config = configuration(&json!({"mode":"use"}))?;
    let off_config = configuration(&json!({"mode":"off"}))?;
    assert_eq!(use_config.mode(), Mode::Use);
    assert_eq!(off_config.mode(), Mode::Off);
    for (config, candidate) in [
        (use_config, PathBuf::from("use.json")),
        (off_config, PathBuf::from("off.json")),
    ] {
        let prepared = config.prepare(root.clone(), candidate, Vec::new())?;
        assert!(matches!(
            prepared.publish(&stamp("forbidden", prepared.current()), CREATE.as_bytes()),
            Err(PlanError::NotAllowed)
        ));
    }

    let desk = PlanDesk::new("use-step", &second, CancellationToken::new());
    assert_eq!(desk.tools()[0]["name"], "read_plan");
    assert_eq!(desk.tools()[0]["annotations"]["readOnlyHint"], true);
    assert!(matches!(
        desk.answer(Call {
            id: json!(1),
            call: "submit_plan".to_owned(),
            input: json!({"anything":"at all"}),
        })
        .await,
        Answer::Refused(_)
    ));
    assert!(matches!(
        desk.answer(Call {
            id: json!(2),
            call: "read_plan".to_owned(),
            input: json!({"versionId":format!("{}-different", second.version_id)}),
        })
        .await,
        Answer::Refused(_)
    ));
    let read = desk
        .answer(Call {
            id: json!(3),
            call: "read_plan".to_owned(),
            input: json!({"versionId":second.version_id}),
        })
        .await;
    assert!(matches!(
        read,
        Answer::Ok(ref value)
            if value["text"].as_str().is_some_and(|text| text.contains("Keep this exact human requirement."))
    ));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_create_and_an_update_become_two_versions_through_both_vendors()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    let fixtures = tempfile::tempdir()?;
    fs::create_dir_all(project.path().join(".loadout"))?;

    let claude_agent = agent(home.path(), "Plan creator", Vendor::ClaudeCode)?;
    let codex_agent = agent(home.path(), "Plan updater", Vendor::Codex)?;
    let claude = executable(
        fixtures.path(),
        "claude",
        &correcting_claude_fixture("claude.stdin"),
    )?;
    let codex = executable(
        fixtures.path(),
        "codex",
        &shell_fixture(Some(UPDATE), CODEX_STDOUT, "codex.stdin", false),
    )?;
    let claude_driver: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(claude));
    let codex_driver: Arc<dyn AgentDriver> = Arc::new(CodexDriver::with_binary(codex));
    let drivers: Drivers = Arc::new(move |vendor| match vendor {
        Vendor::ClaudeCode => Arc::clone(&claude_driver),
        Vendor::Codex => Arc::clone(&codex_driver),
    });

    let workflow = home.path().join("workflows/wp-03.json");
    fs::create_dir_all(workflow.parent().ok_or("the workflow has no parent")?)?;
    fs::write(
        &workflow,
        serde_json::to_vec_pretty(&json!({
            "format": 1,
            "id": "wp-03",
            "name": "Publish a shared plan",
            "steps": [
                {
                    "kind": "agent",
                    "id": "create",
                    "name": "Create the plan",
                    "agent": claude_agent.id,
                    "instructions": "Prepare the first shared plan.",
                    "criteria": [{
                        "id": "R1",
                        "behaviour": "The promo code must survive a quantity change.",
                        "method": "automated-test"
                    }],
                    "plan": {"mode": "create"},
                    "overrides": {},
                    "at": {"x": 0, "y": 0}
                },
                {
                    "kind": "agent",
                    "id": "update",
                    "name": "Update the design",
                    "agent": codex_agent.id,
                    "instructions": "Update only the entrusted design section.",
                    "plan": {"mode": "update", "canUpdate": ["Design"]},
                    "overrides": {},
                    "at": {"x": 240, "y": 0}
                },
                {
                    "kind": "agent",
                    "id": "use",
                    "name": "Use the plan",
                    "agent": codex_agent.id,
                    "instructions": "Build from the complete pinned plan.",
                    "plan": {"mode": "use"},
                    "overrides": {},
                    "at": {"x": 480, "y": 0}
                }
            ],
            "links": [
                {"from": "create", "to": "update"},
                {"from": "update", "to": "use"}
            ]
        }))?,
    )?;

    let store = Store::open(&project.path().join(".loadout/loadout.db"))?;
    let deps = RunDeps {
        home: home.path(),
        project: project.path(),
        store: &store,
        drivers,
        processes: Arc::new(Processes::new()),
        control: RunControl::new(),
    };
    let (sink, _lines) = line_channel(1024);
    let report = tokio::time::timeout(
        Duration::from_secs(20),
        run_workflow_with_reflection(
            &deps,
            &RunRequest {
                workflow,
                how_many_at_once: 1,
                task: None,
                part: None,
                handoffs_from: None,
            },
            sink,
            None,
            false,
        ),
    )
    .await??;
    let run_file = fs::read_to_string(report.dir.join("run.json"))?;
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; 3],
        "the runtime refused a plan candidate: {run_file}"
    );

    for log in ["claude.stdin", "codex.stdin"] {
        let received = fs::read_to_string(fixtures.path().join(log))?;
        assert!(
            received.contains(".loadout/plan-candidates/"),
            "the production {log} adapter did not receive Loadout's candidate path: {received:?}"
        );
        assert!(
            received.contains("separate") && received.contains("result"),
            "the candidate instruction contradicts the ordinary result-file rule: {received:?}"
        );
    }
    let claude_received = fs::read_to_string(fixtures.path().join("claude.stdin"))?;
    assert!(
        claude_received.contains("could not accept it")
            && claude_received.contains("shape is not valid"),
        "the malformed candidate did not get one correction in its existing session: {claude_received:?}"
    );
    let codex_received = fs::read_to_string(fixtures.path().join("codex.stdin"))?;
    assert!(
        codex_received.contains("Use the complete pinned plan version 2")
            && codex_received.contains("Read it with read_plan"),
        "the next Use step did not receive the complete current plan address: {codex_received:?}"
    );

    let root = report.dir.join("plans/workflow-plan");
    let versions = read_versions(&root)?;
    assert_eq!(
        versions.len(),
        2,
        "Create and Update did not publish two versions"
    );
    let current = read_current_version(&root)?;
    assert_eq!(current.version, 2);
    assert_eq!(
        current
            .document
            .requirement("R1")
            .map(|one| one.text.as_str()),
        Some("The promo code must survive a quantity change."),
        "the Design-only update changed a requirement outside its scope"
    );
    assert_eq!(
        current
            .document
            .sections
            .get(&SectionKey::Design)
            .map(String::as_str),
        Some("Show the discount directly below the total.")
    );
    let saved: serde_json::Value = serde_json::from_str(&run_file)?;
    let plan_versions = saved["steps"]
        .as_array()
        .ok_or("the saved run has no steps")?
        .iter()
        .map(|step| step["plan_version"]["version"].as_u64())
        .collect::<Vec<_>>();
    assert_eq!(
        plan_versions,
        vec![Some(1), Some(2), Some(2)],
        "the Use result did not retain the exact version it consumed"
    );
    Ok(())
}
