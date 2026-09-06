//! WF-04: tekst jest wynikiem pętli także wtedy, gdy Git nie widzi żadnej zmiany.
//!
//! 2026-09-05: `nothing_to_judge` pomijało obu sędziów na podstawie pierwszej kopii
//! wejścia. Te testy przechodzą przez normalny Start, realne worktree i pliki przekazań;
//! dubler zastępuje wyłącznie model, a Check uruchamia prawdziwą komendę.

use std::error::Error;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::history::{PastRunWire, read_run_inner};
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::line_channel;
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::mpsc;

const VENDOR: &str = "claude-code";
const AGENT_ID: &str = "01990000-0000-7000-8000-000000000404";
const PATIENCE: Duration = Duration::from_secs(45);
const AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-000000000404
name: Researcher
summary: Returns observations
color: slate
runsWith: claude-code
model: opus
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 2
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Return the result requested by this step.
";

// Licznik jest w OSOBNEJ kopii Check, nigdy w kopii research. Gdyby leżał w entry,
// sam test naprawiałby heurystykę, którą ma obalić. Skrypt jest tracked wejściem repo.
const CHECK: &str = "turn=0
if test -f .judge-turn; then
  read -r turn < .judge-turn
fi
turn=$((turn + 1))
printf '%s\\n' \"$turn\" > .judge-turn
printf 'wf04-check-turn%s\\n' \"$turn\"
if test \"$turn\" -ge \"$1\"; then
  printf '1 passed\\n'
  exit 0
fi
printf '0 passed\\n'
exit 1
";

#[derive(Clone, Copy, Debug)]
enum Judge {
    Agent,
    Check,
}

#[derive(Clone, Copy, Debug)]
enum Writes {
    Nothing,
    LaterStep,
    SecondCopy,
}

#[derive(Clone, Copy, Debug)]
struct Case {
    judge: Judge,
    passes_on: usize,
    writes: Writes,
    when: &'static str,
    stop_when_asked: bool,
}

impl Case {
    const fn text(judge: Judge, passes_on: usize, when: &'static str) -> Self {
        Self {
            judge,
            passes_on,
            writes: Writes::Nothing,
            when,
            stop_when_asked: false,
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn text_only_result_is_judged_failed_then_passed_and_reaches_synthesis()
-> Result<(), Box<dyn Error>> {
    let ran = run(Case::text(Judge::Agent, 2, "stop")).await?;
    assert_judged(&ran, 2)?;
    let synthesis = ran.seen.for_role("synthesis");
    assert_eq!(
        synthesis.len(),
        1,
        "the accepted result never reached synthesis"
    );
    let inputs = synthesis[0].handoffs.join("\n");
    assert!(
        inputs.contains("wf04-result-research-copy1-turn2"),
        "synthesis did not read the actual accepted research file: {inputs}"
    );
    assert!(
        inputs.contains("wf04-result-judge-copy1-turn2"),
        "synthesis did not read the actual accepting judge file: {inputs}"
    );
    assert!(
        !inputs.contains("wf04-result-research-copy1-turn1"),
        "synthesis received the old research instead of the last result"
    );
    assert_future_not_executed(&ran, 2)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_real_check_judges_text_only_work_and_ends_on_its_second_pass()
-> Result<(), Box<dyn Error>> {
    let ran = run(Case::text(Judge::Check, 2, "stop")).await?;
    assert_judged(&ran, 2)?;
    assert_eq!(ran.seen.for_role("synthesis").len(), 1);
    let rows = steps(&ran.book)?;
    for (key, marker) in [
        ("s_judge", "wf04-check-turn1"),
        ("s_judge#1", "wf04-check-turn2"),
    ] {
        let row = rows
            .iter()
            .find(|row| row["node_key"] == key)
            .ok_or("missing Check row")?;
        let id = row["id"].as_str().ok_or("Check row has no id")?;
        let history = ran
            .history
            .steps
            .iter()
            .find(|step| step.id == id)
            .ok_or("Check absent from history")?;
        assert!(
            history.summary.contains(marker),
            "the real Check never produced {marker}"
        );
        assert_eq!(row["process_started"], true);
        assert_eq!(row["death_proof"], true);
        assert_eq!(row["exit_code"], i32::from(key == "s_judge"));
    }
    assert_future_not_executed(&ran, 2)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_change_in_a_later_body_step_does_not_skip_the_judge() -> Result<(), Box<dyn Error>> {
    let mut case = Case::text(Judge::Agent, 1, "stop");
    case.writes = Writes::LaterStep;
    let ran = run(case).await?;
    assert_judged(&ran, 1)?;
    assert_eq!(ran.seen.for_role("later").len(), 1);
    assert_future_not_executed(&ran, 1)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_change_only_in_the_second_copy_does_not_skip_the_judge() -> Result<(), Box<dyn Error>> {
    let mut case = Case::text(Judge::Agent, 1, "stop");
    case.writes = Writes::SecondCopy;
    let ran = run(case).await?;
    assert_judged(&ran, 1)?;
    let research = ran.seen.for_role("research");
    assert_eq!(research.len(), 2);
    assert!(research.iter().any(|start| start.copy == 2 && start.wrote));
    assert_future_not_executed(&ran, 1)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exhausted_text_only_loops_keep_stop_for_agent_and_check() -> Result<(), Box<dyn Error>> {
    for judge in [Judge::Agent, Judge::Check] {
        let ran = run(Case::text(judge, 99, "stop")).await?;
        assert_judged(&ran, 3)?;
        assert_eq!(
            ran.seen.for_role("synthesis").len(),
            0,
            "{judge:?} ignored Stop"
        );
        assert_eq!(ran.report.steps.last(), Some(&StepState::Skipped));
        assert_last_judge_failed(&ran)?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exhausted_text_only_loops_keep_carry_on_without_hiding_failure()
-> Result<(), Box<dyn Error>> {
    for judge in [Judge::Agent, Judge::Check] {
        let ran = run(Case::text(judge, 99, "carry-on")).await?;
        assert_judged(&ran, 3)?;
        assert_eq!(
            ran.seen.for_role("synthesis").len(),
            1,
            "{judge:?} ignored CarryOn"
        );
        assert_last_judge_failed(&ran)?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exhausted_text_only_loops_really_ask_before_carrying_on() -> Result<(), Box<dyn Error>> {
    for judge in [Judge::Agent, Judge::Check] {
        let ran = run(Case::text(judge, 99, "ask-me")).await?;
        assert_judged(&ran, 3)?;
        assert!(ran.answered, "{judge:?} never exposed the paused run");
        assert_eq!(ran.seen.for_role("synthesis").len(), 1);
        assert_last_judge_failed(&ran)?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stopping_at_the_exhausted_loop_question_never_starts_synthesis()
-> Result<(), Box<dyn Error>> {
    for judge in [Judge::Agent, Judge::Check] {
        let mut case = Case::text(judge, 99, "ask-me");
        case.stop_when_asked = true;
        let ran = run(case).await?;
        assert_judged(&ran, 3)?;
        assert!(ran.answered, "{judge:?} never exposed the paused run");
        assert_eq!(ran.seen.for_role("synthesis").len(), 0);
        assert_eq!(ran.history.state, "cancelled");
    }
    Ok(())
}

fn steps(book: &Value) -> Result<&Vec<Value>, Box<dyn Error>> {
    book["steps"]
        .as_array()
        .ok_or_else(|| "run.json has no steps".into())
}

fn assert_judged(ran: &Ran, expected: usize) -> Result<(), Box<dyn Error>> {
    let judges: Vec<&Value> = steps(&ran.book)?
        .iter()
        .filter(|row| {
            row["node_key"]
                .as_str()
                .is_some_and(|key| key == "s_judge" || key.starts_with("s_judge#"))
        })
        .collect();
    let executed = judges.iter().filter(|row| row["executed"] == true).count();
    assert_eq!(
        executed, expected,
        "a clean Git tree is not permission to skip the judge; expected {expected} actual executions, got {executed}: {judges:?}"
    );
    let research = ran.seen.for_role("research");
    assert!(!research.is_empty(), "the entry never reached the driver");
    assert!(
        research
            .iter()
            .filter(|start| start.copy == 1)
            .all(|start| start.clean),
        "the first entry copy was not really clean, so this is not the regression fixture"
    );
    Ok(())
}

fn assert_future_not_executed(ran: &Ran, accepted_on: usize) -> Result<(), Box<dyn Error>> {
    let mut omitted = 0;
    for row in steps(&ran.book)? {
        let Some(key) = row["node_key"].as_str() else {
            continue;
        };
        let Some((_, turn)) = key.rsplit_once('#') else {
            continue;
        };
        if turn.parse::<usize>()? < accepted_on {
            continue;
        }
        omitted += 1;
        assert_eq!(
            row["executed"], false,
            "future round claimed execution: {row}"
        );
        assert_eq!(
            row["not_run_because"],
            format!("loop settled at try {accepted_on}")
        );
        let id = row["id"].as_str().ok_or("omitted row has no id")?;
        let shown = ran
            .history
            .steps
            .iter()
            .find(|step| step.id == id)
            .ok_or("omitted round is missing from history")?;
        assert_eq!(shown.executed, Some(false));
        assert_eq!(
            shown.state, "not_run",
            "history presents an omitted round as an executed check"
        );
    }
    assert!(
        omitted >= 2,
        "the test did not leave a future round to inspect"
    );
    Ok(())
}

fn assert_last_judge_failed(ran: &Ran) -> Result<(), Box<dyn Error>> {
    let last = ran
        .history
        .steps
        .iter()
        .rfind(|step| step.tile == "s_judge")
        .ok_or("history has no judge")?;
    assert_eq!(last.state, "failed", "exhaustion was hidden by carrying on");
    assert!(
        !last.error.is_empty(),
        "history omitted why the loop was not accepted"
    );
    Ok(())
}

fn agent(id: &str, role: &str, name: &str) -> Value {
    json!({
        "kind": "agent", "id": id, "name": name, "agent": AGENT_ID,
        "overrides": {}, "instructions": format!("wf04:{role} copy{{{{copy}}}}"),
        "folder": {"use": "fresh-copy"}, "at": {"x": 0, "y": 0}
    })
}

fn workflow(case: Case) -> Value {
    let mut research = agent("s_research", "research", "Research");
    research["copies"] = json!(if matches!(case.writes, Writes::SecondCopy) {
        2
    } else {
        1
    });
    let mut judge = match case.judge {
        Judge::Agent => agent("s_judge", "judge", "Judge"),
        Judge::Check => json!({
            "kind": "check", "id": "s_judge", "name": "Judge",
            "command": format!("sh judge.sh {}", case.passes_on), "proof": "(\\d+) passed",
            "folder": {"use": "fresh-copy"}, "at": {"x": 0, "y": 200}
        }),
    };
    judge["whenItFails"] = json!(case.when);
    let mut steps = vec![research];
    let mut links = vec![
        json!({"from": "s_judge", "to": "s_research", "max_turns": 3}),
        json!({"from": "s_judge", "to": "s_synthesis"}),
    ];
    if matches!(case.writes, Writes::LaterStep) {
        steps.push(agent("s_later", "later", "Later work"));
        links.push(json!({"from": "s_research", "to": "s_later"}));
        links.push(json!({"from": "s_later", "to": "s_judge"}));
    } else {
        links.push(json!({"from": "s_research", "to": "s_judge"}));
    }
    steps.extend([judge, agent("s_synthesis", "synthesis", "Synthesis")]);
    json!({"format": 1, "id": "wf_text_results", "name": "Judge the result", "steps": steps, "links": links})
}

struct Ran {
    report: RunReport,
    book: Value,
    history: PastRunWire,
    seen: Arc<Seen>,
    answered: bool,
}

async fn run(case: Case) -> Result<Ran, Box<dyn Error>> {
    let home = TempDir::new()?;
    let project = TempDir::new()?;
    fs::create_dir_all(home.path().join("agents"))?;
    fs::create_dir_all(home.path().join("workflows"))?;
    fs::write(home.path().join("agents/researcher.md"), AGENT)?;
    fs::write(project.path().join(".gitignore"), ".loadout/\n")?;
    fs::write(project.path().join("judge.sh"), CHECK)?;
    git(project.path(), &["init", "--quiet"])?;
    git(project.path(), &["add", "-A"])?;
    git(
        project.path(),
        &["commit", "--quiet", "-m", "initial test input"],
    )?;
    fs::create_dir_all(project.path().join(".loadout"))?;
    let workflow_path = home.path().join("workflows/text-results.json");
    fs::write(&workflow_path, serde_json::to_vec(&workflow(case))?)?;
    let store = Store::open(&project.path().join(".loadout/loadout.db"))?;
    let seen = Arc::new(Seen::default());
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        case,
        seen: Arc::clone(&seen),
    });
    let drivers: Drivers = Arc::new(move |_| Arc::clone(&driver));
    let control = RunControl::new();
    let deps = RunDeps {
        home: home.path(),
        project: project.path(),
        store: &store,
        drivers,
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: control.clone(),
    };
    let request = RunRequest {
        workflow: workflow_path,
        how_many_at_once: 3,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (sink, mut source) = line_channel(4096);
    let execution = run_workflow_inner(&deps, &request, sink);
    let intervention = answer_when_paused(project.path(), &control, case);
    let (report, answered) =
        tokio::time::timeout(PATIENCE, async { tokio::join!(execution, intervention) })
            .await
            .map_err(|_| "the loop did not finish")?;
    let report = report?;
    let book = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    let folder = report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("run has no folder")?;
    let history = read_run_inner(project.path(), folder)?;
    while source.try_next().is_some() {}
    Ok(Ran {
        report,
        book,
        history,
        seen,
        answered: answered?,
    })
}

async fn answer_when_paused(
    project: &Path,
    control: &RunControl,
    case: Case,
) -> Result<bool, Box<dyn Error>> {
    if case.when != "ask-me" {
        control.wait_until_settled().await;
        return Ok(false);
    }
    loop {
        if let Ok(entries) = fs::read_dir(project.join(".loadout/runs")) {
            for entry in entries {
                let path = entry?.path().join("run.json");
                if let Ok(bytes) = fs::read(path) {
                    let book: Value = serde_json::from_slice(&bytes)?;
                    if book["status"] == "paused" {
                        if case.stop_when_asked {
                            control.stop();
                        } else {
                            control.go_on_with(Some(
                                "Continue with the rejected result clearly marked.".to_owned(),
                            ));
                        }
                        return Ok(true);
                    }
                }
            }
        }
        tokio::select! {
            () = control.wait_until_settled() => return Ok(false),
            () = tokio::time::sleep(Duration::from_millis(5)) => {}
        }
    }
}

#[derive(Clone, Debug)]
struct Started {
    role: String,
    copy: usize,
    clean: bool,
    wrote: bool,
    handoffs: Vec<String>,
}

#[derive(Default, Debug)]
struct Seen {
    // Zamek chroni wyłącznie krótki zapis/odczyt próbek; nigdy nie przechodzi przez await.
    starts: Mutex<Vec<Started>>,
}

impl Seen {
    fn for_role(&self, role: &str) -> Vec<Started> {
        self.starts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|start| start.role == role)
            .cloned()
            .collect()
    }
}

#[derive(Debug)]
struct Fake {
    case: Case,
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
            version: Some("wf04-test".to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let role = ["research", "later", "judge", "synthesis"]
            .into_iter()
            .find(|role| spec.prompt.contains(&format!("wf04:{role} ")))
            .unwrap_or("reflection");
        let copy = if spec.prompt.contains(&format!("wf04:{role} copy2")) {
            2
        } else {
            1
        };
        let handoffs = read_inputs(&spec.prompt)?;
        let wrote = (role == "later" && matches!(self.case.writes, Writes::LaterStep))
            || (role == "research" && copy == 2 && matches!(self.case.writes, Writes::SecondCopy));
        if wrote {
            fs::write(
                spec.cwd.join("actual-work.txt"),
                "the other part of the loop changed this",
            )?;
        }
        let clean = git(&spec.cwd, &["status", "--porcelain"])
            .map_err(|error| anyhow::anyhow!(error.to_string()))?
            .trim()
            .is_empty();
        let turn = {
            let mut starts = self
                .seen
                .starts
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let turn = starts
                .iter()
                .filter(|start| start.role == role && start.copy == copy)
                .count()
                + 1;
            starts.push(Started {
                role: role.to_owned(),
                copy,
                clean,
                wrote,
                handoffs,
            });
            turn
        };
        let verdict = if role == "judge" {
            if turn >= self.case.passes_on {
                "outcome: pass\n"
            } else {
                "outcome: fail\n"
            }
        } else {
            ""
        };
        let text = format!(
            "{verdict}## Answer\nwf04-result-{role}-copy{copy}-turn{turn}\n\n## Evidence\nObserved the supplied material.\n\n## Open questions\nNone.\n"
        );
        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };
        let _ = events
            .send(
                AgentEvent::Started {
                    session: session.clone(),
                    model: spec.model.unwrap_or_default(),
                    tools: Vec::new(),
                    capabilities: Vec::new(),
                }
                .into(),
            )
            .await;
        Ok(Box::new(Turn {
            events,
            session,
            text,
        }))
    }
}

// Nie szukamy plików po dysku: otwieramy wyłącznie adresy, które produkcyjny RunSpec
// podał temu konsumentowi. Folder pełny wyników nie dowodzi kompletnego kontekstu.
fn read_inputs(prompt: &str) -> anyhow::Result<Vec<String>> {
    let mut contents = Vec::new();
    for line in prompt.lines() {
        let Some((_, addressed)) = line
            .strip_prefix("- ")
            .and_then(|line| line.split_once(": "))
        else {
            continue;
        };
        let Some((path, _)) = addressed.rsplit_once(" (") else {
            continue;
        };
        if path.contains("/handoffs/") {
            contents.push(fs::read_to_string(path)?);
        }
    }
    Ok(contents)
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    text: String,
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

fn git(at: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(at)
        .args([
            "-c",
            "user.name=Loadout Test",
            "-c",
            "user.email=test@loadout.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(format!("git {args:?}: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
