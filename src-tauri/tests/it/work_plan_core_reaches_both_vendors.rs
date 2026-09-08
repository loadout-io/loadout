//! WP-04b: jeden przydział niesie wspólny rdzeń planu przez oba prawdziwe adaptery.

// 2026-09-08 (WP-04b) — fikstury procesu używają `expect` tylko wtedy, gdy brak elementu
// oznacza wadę samej sondy. Bez tego cały cel `it` nie biegnie pod `-D warnings`.
#![allow(clippy::expect_used)]
// 2026-09-08 (WP-04b) — scenariusze porównują cztery fizyczne dostawy i trzy momenty odmowy;
// podział ukryłby wspólną tożsamość wersji albo etap, na którym proces nie może wystartować.
#![allow(clippy::too_many_lines)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use loadout_lib::commands::run::run_workflow_with_reflection;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::context::limits::STEP_PROMPT_BYTES;
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::{AgentDriver, Policy, RunSpec, ToAgent};
use loadout_lib::engine::step::StepState;
use loadout_lib::ipc::line_channel;
use loadout_lib::library::agents::{Agent, Vendor, write_agent_file};
use loadout_lib::store::Store;
use loadout_lib::work_plan::{
    AcceptanceCriterion, Configuration, ContextBlock, HumanRequirement, PlanVersion, Publication,
    Stamp, WorkPlanCore, compose,
};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

const AGREED: &str = "The final requirement must reach every consumer unchanged.";
const LIMIT: Duration = Duration::from_secs(20);
const PLAN_OPEN: &str = "## Required plan core\n";
const PLAN_CLOSE: &str = "\n## End required plan core";

const CLAUDE_STDOUT: &str = concat!(
    r#"{"type":"system","subtype":"init","session_id":"01990000-0000-7000-8000-000000000401","model":"sonnet","tools":[]}"#,
    "\n",
    r#"{"type":"result","subtype":"success","is_error":false,"num_turns":1,"duration_ms":7,"total_cost_usd":0.001,"result":"ok"}"#,
    "\n",
);

const CODEX_STDOUT: &str = concat!(
    r#"{"type":"thread.started","thread_id":"wp-04b-use"}"#,
    "\n",
    r#"{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}"#,
    "\n",
    r#"{"type":"turn.completed","usage":{"input_tokens":3,"cached_input_tokens":1,"output_tokens":2}}"#,
    "\n",
);

struct PublishedPlan {
    _storage: TempDir,
    root: PathBuf,
    version: PlanVersion,
}

struct OversizedRun {
    _project: TempDir,
    _home: TempDir,
    fixtures: TempDir,
    steps: Vec<StepState>,
    run_file: String,
}

fn executable(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn transport_fixture(claude: bool) -> String {
    let read = if claude {
        "IFS= read -r sent"
    } else {
        "sent=\"$(cat)\""
    };
    let stdout = if claude { CLAUDE_STDOUT } else { CODEX_STDOUT };
    format!(
        r#"#!/bin/sh
if [ "${{1-}}" = "--version" ]; then
  printf '%s\n' 'fixture-cli 1.0'
  exit 0
fi
{read}
here="$(dirname "$0")"
case "$sent" in
  *CLAUDE-IMPLEMENTER*) log='claude-implementer.stdin' ;;
  *CLAUDE-REVIEWER*) log='claude-reviewer.stdin' ;;
  *CODEX-IMPLEMENTER*) log='codex-implementer.stdin' ;;
  *CODEX-REVIEWER*) log='codex-reviewer.stdin' ;;
  *LATE-REQUIREMENT*) log='late-requirement.stdin' ;;
  *SHARED-ALLOWANCE*) log='shared-allowance.stdin' ;;
  *) log='unexpected.stdin' ;;
esac
printf '%s' "$sent" > "$here/$log"
printf '%s' '{stdout}'
exit 0
"#
    )
}

fn two_turn_fixture() -> &'static str {
    r#"#!/bin/sh
if [ "${1-}" = "--version" ]; then
  printf '%s\n' 'fixture-cli 1.0'
  exit 0
fi
here="$(dirname "$0")"
session=''
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--session-id" ]; then session="$2"; fi
  shift
done
printf '{"type":"system","subtype":"init","session_id":"%s","model":"sonnet","tools":[]}\n' "$session"
while IFS= read -r sent; do
  printf '%s\n' "$sent" >> "$here/resumed.stdin"
  printf '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"duration_ms":7,"total_cost_usd":0.001,"result":"ok"}\n'
done
exit 0
"#
}

fn candidate(goal: &str, implementation: &str, validation: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "goal": goal,
        "inScope": ["Implementation and review"],
        "outOfScope": ["Unrelated workflows"],
        "requirements": [],
        "acceptance": [{
            "text": "Both roles receive the same core.",
            "verification": "Compare captured stdin bytes."
        }],
        "decisions": ["The application owns the shared allowance."],
        "sections": {
            "Implementation": implementation,
            "Validation": validation
        },
        "proposals": [],
        "assumptions": [],
        "questions": [],
        "conflicts": ["None remain."],
        "sources": []
    }))
    .expect("the test candidate is serializable")
}

fn version(publication: Publication) -> PlanVersion {
    match publication {
        Publication::Published(version)
        | Publication::Unchanged(version)
        | Publication::AlreadyPublished(version) => version,
    }
}

fn published_plan(
    goal: &str,
    implementation: &str,
    requirements: Vec<HumanRequirement>,
) -> Result<PublishedPlan, Box<dyn Error>> {
    let storage = tempfile::tempdir()?;
    let root = storage.path().join("plan");
    let configuration = Configuration::from_step(Some(&json!({"mode":"create"})))?;
    let prepared =
        configuration.prepare(root.clone(), PathBuf::from("candidate.json"), requirements)?;
    let stamp = Stamp {
        run_id: "run-wp-04b".to_owned(),
        document_id: "workflow-plan".to_owned(),
        step_id: "author".to_owned(),
        attempt: 1,
        operation: "create".to_owned(),
        parent: None,
        at: "2026-09-08T12:00:00Z".to_owned(),
    };
    let version = version(prepared.publish(
        &stamp,
        &candidate(goal, implementation, "Compare the received bytes."),
    )?);
    Ok(PublishedPlan {
        _storage: storage,
        root,
        version,
    })
}

fn prepared_core(
    plan: &PublishedPlan,
    role_requirement: &str,
) -> Result<WorkPlanCore, Box<dyn Error>> {
    let configuration = Configuration::from_step(Some(&json!({"mode":"use"})))?;
    let prepared = configuration.prepare(
        plan.root.clone(),
        PathBuf::from("unused.json"),
        vec![HumanRequirement {
            id: "role-only".to_owned(),
            text: role_requirement.to_owned(),
            acceptance: Vec::new(),
        }],
    )?;
    prepared
        .core_for_prompt()
        .ok_or_else(|| "Plan: Use did not expose its pinned core".into())
}

fn compose_prompt(
    marker: &str,
    plan: &WorkPlanCore,
    context: &ContextBlock,
    handoff: &str,
) -> Result<String, Box<dyn Error>> {
    let composed = compose(marker, marker, Some(plan), context, handoff)
        .map_err(|refusal| std::io::Error::other(refusal.message))?;
    Ok(format!("{marker}\n\n{}", composed.prompt))
}

fn run_spec(cwd: &Path, prompt: String) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt,
        model: None,
        system_append: None,
        reaches_the_web: false,
        policy: Policy::ReadOnly,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

async fn deliver(
    driver: &dyn AgentDriver,
    cwd: &Path,
    prompt: String,
) -> Result<(), Box<dyn Error>> {
    let (events, _received) = mpsc::channel(128);
    let mut handle = timeout(LIMIT, driver.start(run_spec(cwd, prompt), events)).await??;
    let outcome = timeout(LIMIT, handle.wait()).await??;
    assert!(outcome.ok, "the controlled adapter rejected its turn");
    Ok(())
}

fn claude_prompt(path: &Path) -> Result<String, Box<dyn Error>> {
    let envelope: Value = serde_json::from_str(fs::read_to_string(path)?.trim_end())?;
    envelope
        .pointer("/message/content/0/text")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "the Claude adapter did not put the prompt in its stdin envelope".into())
}

fn core(prompt: &str) -> Option<&str> {
    let after = prompt.split_once(PLAN_OPEN)?.1;
    Some(after.split_once(PLAN_CLOSE)?.0)
}

fn agreed_requirement(id: &str, text: &str) -> HumanRequirement {
    HumanRequirement {
        id: id.to_owned(),
        text: text.to_owned(),
        acceptance: vec![AcceptanceCriterion {
            text: format!("{text} is present."),
            verification: "Inspect the received prompt.".to_owned(),
        }],
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_same_core_reaches_an_implementer_and_a_reviewer_byte_for_byte()
-> Result<(), Box<dyn Error>> {
    let plan = published_plan(
        "Keep every consumer aligned.",
        "Apply the complete pinned plan.",
        vec![agreed_requirement("R-last", AGREED)],
    )?;
    let implementer = prepared_core(&plan, "Implement the plan.")?;
    let reviewer = prepared_core(&plan, "Review the result.")?;
    assert_eq!(implementer.version_id, plan.version.version_id);
    assert_eq!(reviewer.version_id, plan.version.version_id);

    let fixtures = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let claude = ClaudeDriver::with_binary(executable(
        fixtures.path(),
        "claude",
        &transport_fixture(true),
    )?);
    let codex = CodexDriver::with_binary(executable(
        fixtures.path(),
        "codex",
        &transport_fixture(false),
    )?);
    for (driver, marker, prepared) in [
        (
            &claude as &dyn AgentDriver,
            "CLAUDE-IMPLEMENTER",
            &implementer,
        ),
        (&claude, "CLAUDE-REVIEWER", &reviewer),
        (&codex, "CODEX-IMPLEMENTER", &implementer),
        (&codex, "CODEX-REVIEWER", &reviewer),
    ] {
        deliver(
            driver,
            workspace.path(),
            compose_prompt(marker, prepared, &ContextBlock::default(), "")?,
        )
        .await?;
    }

    let prompts = [
        claude_prompt(&fixtures.path().join("claude-implementer.stdin"))?,
        claude_prompt(&fixtures.path().join("claude-reviewer.stdin"))?,
        fs::read_to_string(fixtures.path().join("codex-implementer.stdin"))?,
        fs::read_to_string(fixtures.path().join("codex-reviewer.stdin"))?,
    ];
    let cores = prompts
        .iter()
        .map(|prompt| core(prompt))
        .collect::<Vec<_>>();
    assert!(
        cores.iter().all(Option::is_some),
        "a production adapter did not deliver the required plan block: {prompts:#?}"
    );
    assert!(cores.iter().flatten().all(|one| one.contains(AGREED)));
    assert!(cores.windows(2).all(|pair| pair[0] == pair[1]));
    assert!(prompts.iter().all(|prompt| {
        prompt.contains("Apply the complete pinned plan.")
            && prompt.contains("Compare the received bytes.")
    }));
    Ok(())
}

#[tokio::test]
async fn a_requirement_late_in_the_plan_is_never_only_an_address() -> Result<(), Box<dyn Error>> {
    let late = "LATE-REQUIREMENT must remain in the required bytes.";
    let mut requirements = (0..40)
        .map(|at| agreed_requirement(&format!("R{at}"), &format!("Requirement number {at}.")))
        .collect::<Vec<_>>();
    requirements.push(agreed_requirement("R-last", late));
    let plan = published_plan("Keep all requirements.", "Apply them.", requirements)?;
    let prepared = prepared_core(&plan, "Implement all requirements.")?;
    let fixtures = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let driver = ClaudeDriver::with_binary(executable(
        fixtures.path(),
        "claude",
        &transport_fixture(true),
    )?);
    deliver(
        &driver,
        workspace.path(),
        compose_prompt("LATE-REQUIREMENT", &prepared, &ContextBlock::default(), "")?,
    )
    .await?;
    let prompt = claude_prompt(&fixtures.path().join("late-requirement.stdin"))?;
    let received = core(&prompt).ok_or("the required core was absent")?;
    assert!(
        received.contains(late),
        "the last requirement became index-only"
    );
    assert!(received.contains("R-last"));
    Ok(())
}

#[tokio::test]
async fn plan_context_and_the_handoff_index_share_one_allowance() -> Result<(), Box<dyn Error>> {
    let plan = published_plan(
        &format!("SHARED-ALLOWANCE {}", "plan ".repeat(1_100)),
        &"detail ".repeat(450),
        vec![agreed_requirement("R1", "Keep the one combined allowance.")],
    )?;
    let prepared = prepared_core(&plan, "Use every supplied input.")?;
    let context = ContextBlock {
        required: format!(
            "## Reference materials\nImportant requirements:\n{}",
            "context ".repeat(700)
        ),
        optional: "Available topics and sources (optional):\n- Source: Brief — `brief`".to_owned(),
        required_name: Some("Brief".to_owned()),
    };
    let handoff = format!(
        "What the steps before this one left (optional):\n- `handoffs/previous.md` — {}",
        "handoff ".repeat(1_200)
    );
    assert!(prepared.required.len() < STEP_PROMPT_BYTES);
    assert!(context.required.len() + context.optional.len() < STEP_PROMPT_BYTES);
    assert!(handoff.len() < STEP_PROMPT_BYTES);
    let composed = compose(
        "SHARED-ALLOWANCE",
        "shared-allowance",
        Some(&prepared),
        &context,
        &handoff,
    )
    .map_err(|refusal| std::io::Error::other(refusal.message))?;
    let addition = composed.prompt;

    let fixtures = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let driver = CodexDriver::with_binary(executable(
        fixtures.path(),
        "codex",
        &transport_fixture(false),
    )?);
    deliver(
        &driver,
        workspace.path(),
        format!("SHARED-ALLOWANCE\n\n{addition}"),
    )
    .await?;
    let prompt = fs::read_to_string(fixtures.path().join("shared-allowance.stdin"))?;
    let received = prompt
        .strip_prefix("SHARED-ALLOWANCE\n\n")
        .ok_or("the adapter changed the composed prefix")?;
    assert!(received.len() <= STEP_PROMPT_BYTES);
    assert!(received.contains("Keep the one combined allowance."));
    assert!(received.contains("Important requirements:"));
    assert!(received.contains("handoffs/previous.md"));
    Ok(())
}

fn runtime_fixture(candidate: &str) -> String {
    format!(
        r#"#!/bin/sh
if [ "${{1-}}" = "--version" ]; then
  printf '%s\n' 'fixture-cli 1.0'
  exit 0
fi
IFS= read -r sent
here="$(dirname "$0")"
candidate_path="$(printf '%s\n' "$sent" | sed -n 's/.*LOADOUT_PLAN_CANDIDATE_PATH=\([^\\\" ]*\).*/\1/p')"
if [ -n "$candidate_path" ]; then
  printf '%s\n' "$sent" > "$here/author.stdin"
  mkdir -p "$(dirname "$candidate_path")"
  printf '%s\n' '{candidate}' > "$candidate_path"
else
  printf '%s\n' "$sent" > "$here/consumer.stdin"
fi
printf '%s' '{CLAUDE_STDOUT}'
exit 0
"#
    )
}

fn saved_agent(home: &Path) -> Result<Agent, Box<dyn Error>> {
    let mut saved = Agent::example();
    saved.id = Uuid::now_v7();
    "Plan worker".clone_into(&mut saved.name);
    saved.runs_with = Vendor::ClaudeCode;
    saved.write_results_to.clear();
    write_agent_file(&home.join("agents"), &saved, None)?;
    Ok(saved)
}

async fn oversized_runtime() -> Result<OversizedRun, Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    let fixtures = tempfile::tempdir()?;
    fs::create_dir_all(project.path().join(".loadout"))?;
    let saved = saved_agent(home.path())?;
    let oversized = candidate(
        &format!("Oversized required core {}", "scope ".repeat(4_500)),
        "This detail is optional.",
        "Validate before use.",
    );
    let oversized = String::from_utf8(oversized)?;
    let binary = executable(fixtures.path(), "claude", &runtime_fixture(&oversized))?;
    let driver: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(binary));
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));
    let workflow = home.path().join("workflows/oversized.json");
    fs::create_dir_all(workflow.parent().ok_or("the workflow has no parent")?)?;
    fs::write(
        &workflow,
        serde_json::to_vec_pretty(&json!({
            "format": 1,
            "id": "wp-04b-oversized",
            "name": "Refuse an oversized core",
            "steps": [
                {
                    "kind": "agent",
                    "id": "author",
                    "name": "Author",
                    "agent": saved.id,
                    "instructions": "Create the shared plan.",
                    "plan": {"mode":"create"},
                    "overrides": {},
                    "at": {"x":0,"y":0}
                },
                {
                    "kind": "agent",
                    "id": "consumer",
                    "name": "Consumer",
                    "agent": saved.id,
                    "instructions": "Use the shared plan.",
                    "plan": {"mode":"use"},
                    "overrides": {},
                    "at": {"x":240,"y":0}
                }
            ],
            "links": [{"from":"author","to":"consumer"}]
        }))?,
    )?;
    let store = Store::open(&project.path().join(".loadout/loadout.db"))?;
    let deps = RunDeps {
        home: home.path(),
        project: project.path(),
        store: &store,
        drivers,
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let (sink, _lines) = line_channel(4096);
    let report = timeout(
        LIMIT,
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
    Ok(OversizedRun {
        _project: project,
        _home: home,
        fixtures,
        steps: report.steps,
        run_file,
    })
}

#[tokio::test]
async fn a_core_too_large_stops_the_consumer_before_the_first_process() -> Result<(), Box<dyn Error>>
{
    let run = oversized_runtime().await?;
    assert!(run.fixtures.path().join("author.stdin").is_file());
    assert!(
        !run.fixtures.path().join("consumer.stdin").exists(),
        "the oversized required core reached the consumer process"
    );
    assert!(run.run_file.contains("required plan core"));
    assert!(run.run_file.contains("Reduce or split the scope"));
    Ok(())
}

#[tokio::test]
async fn a_core_too_large_stops_only_the_step_that_uses_it() -> Result<(), Box<dyn Error>> {
    let run = oversized_runtime().await?;
    assert_eq!(run.steps, vec![StepState::Succeeded, StepState::Failed]);
    let author = fs::read_to_string(run.fixtures.path().join("author.stdin"))?;
    assert!(!author.contains("Moved to"));
    assert!(!run.run_file.contains("Moved to"));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_turn_in_the_same_session_carries_the_core_again() -> Result<(), Box<dyn Error>> {
    let plan = published_plan(
        "Repeat the required input after resume.",
        "Keep the session alive.",
        vec![agreed_requirement("R1", AGREED)],
    )?;
    let prepared = prepared_core(&plan, "Continue the same role.")?;
    let first = compose_prompt("FIRST-TURN", &prepared, &ContextBlock::default(), "")?;
    let second = compose_prompt("SECOND-TURN", &prepared, &ContextBlock::default(), "")?;
    let fixtures = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let binary = executable(fixtures.path(), "claude", two_turn_fixture())?;
    let driver = ClaudeDriver::with_binary(binary);
    let (events, _received) = mpsc::channel(128);
    let mut handle = timeout(
        LIMIT,
        driver.start(run_spec(workspace.path(), first), events),
    )
    .await??;
    let first_outcome = timeout(LIMIT, handle.wait()).await??;
    assert!(first_outcome.ok);
    let voice = handle
        .voice()
        .ok_or("Claude exposed no live-session voice")?;
    voice.send(ToAgent::Turn(second)).await?;
    let second_outcome = timeout(LIMIT, handle.wait()).await??;
    assert!(second_outcome.ok);
    let _code = timeout(LIMIT, handle.close()).await??;

    let envelopes = fs::read_to_string(fixtures.path().join("resumed.stdin"))?
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(envelopes.len(), 2);
    let prompts = envelopes
        .iter()
        .map(|envelope| {
            envelope
                .pointer("/message/content/0/text")
                .and_then(Value::as_str)
                .ok_or("a resumed Claude envelope has no prompt text")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let cores = prompts
        .iter()
        .map(|prompt| core(prompt).ok_or("a turn omitted its required plan core"))
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(cores.len(), 2);
    assert_eq!(cores[0], cores[1]);
    assert!(cores.iter().all(|received| received.contains(AGREED)));
    Ok(())
}
