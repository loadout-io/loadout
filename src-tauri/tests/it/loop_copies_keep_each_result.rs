//! WF-05: kafelek, kopia i próba to trzy różne tożsamości przekazania.
//!
//! 2026-09-05: `.next_back()` per kafelek zostawiało jeden wynik z trzech, a `node_of`
//! podawało drugiej kopii historię pierwszej. Czytamy pliki wskazane przez rzeczywisty
//! `RunSpec` w chwili startu konsumenta, nie cały katalog wyników znaleziony po biegu.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::line_channel;
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::{Barrier, Notify, mpsc};

const VENDOR: &str = "claude-code";
const AGENT_ID: &str = "01990000-0000-7000-8000-000000000405";
const PATIENCE: Duration = Duration::from_secs(45);
const AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-000000000405
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
Return this step's result.
";

#[derive(Clone, Copy, Debug)]
struct Case {
    passes_on: usize,
    second_loop: bool,
    reverse: bool,
    failed_copy: bool,
    looped: bool,
}

impl Case {
    const fn passing_on(passes_on: usize) -> Self {
        Self {
            passes_on,
            second_loop: false,
            reverse: false,
            failed_copy: false,
            looped: true,
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn synthesis_gets_all_three_copies_when_the_first_try_passes() -> Result<(), Box<dyn Error>> {
    let ran = run(Case::passing_on(1)).await?;
    assert_synthesis(&ran, &[("left", 1)])?;
    assert_eq!(
        ran.calls("left", "research").len(),
        3,
        "future rounds should not run after pass"
    );
    assert_eq!(ran.calls("left", "judge").len(), 1);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_copy_receives_its_own_previous_try_not_the_first_copys() -> Result<(), Box<dyn Error>>
{
    let ran = run(Case::passing_on(2)).await?;
    let calls = ran.calls("left", "research");
    assert_eq!(
        calls.len(),
        6,
        "the fixture did not reach all copies of both rounds"
    );
    for copy in 1..=3 {
        let second = calls
            .iter()
            .find(|call| call.copy == copy && call.turn == 2)
            .ok_or("a copy did not enter its second turn")?;
        assert_eq!(
            second.markers(),
            vec![
                marker("left", "research", copy, 1),
                marker("left", "judge", 1, 1)
            ],
            "copy {copy} was given another copy's prior work: {second:?}"
        );
        assert!(
            second.inputs[0].label.contains("your own"),
            "the previous result lost its own-try label"
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn judge_receives_every_earlier_try_of_every_entry_copy_in_graph_order()
-> Result<(), Box<dyn Error>> {
    let ran = run(Case::passing_on(3)).await?;
    let judges = ran.calls("left", "judge");
    assert_eq!(judges.len(), 3);
    for judge in judges {
        let mut expected = Vec::new();
        for turn in 1..=judge.turn {
            expected.extend((1..=3).map(|copy| marker("left", "research", copy, turn)));
        }
        expected.extend((1..judge.turn).map(|turn| marker("left", "judge", 1, turn)));
        assert_eq!(
            judge.markers(),
            expected,
            "the judge cannot compare every copy against its earlier work: {judge:?}"
        );
    }
    assert_synthesis(&ran, &[("left", 3)])?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reverse_completion_keeps_all_results_in_the_same_context_order()
-> Result<(), Box<dyn Error>> {
    let mut case = Case::passing_on(2);
    case.reverse = true;
    let reversed = run(case).await?;
    assert_synthesis(&reversed, &[("left", 2)])?;
    for turn in 1..=2 {
        let ended: Vec<usize> = reversed
            .finished
            .iter()
            .filter(|(branch, which, _)| branch == "left" && *which == turn)
            .map(|(_, _, copy)| *copy)
            .collect();
        assert_eq!(
            ended,
            vec![3, 2, 1],
            "the barrier did not actually reverse completion"
        );
    }
    case.reverse = false;
    let ordinary = run(case).await?;
    assert_synthesis(&ordinary, &[("left", 2)])?;
    for role in ["research", "judge", "synthesis"] {
        let normal = ordinary.contexts_for(role);
        let reverse = reversed.contexts_for(role);
        assert_eq!(
            normal, reverse,
            "finishing order changed {role}'s context order"
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_disjoint_loops_keep_each_copy_at_their_own_last_published_turn()
-> Result<(), Box<dyn Error>> {
    let mut case = Case::passing_on(1);
    case.second_loop = true;
    let ran = run(case).await?;
    assert_synthesis(&ran, &[("left", 1), ("right", 2)])?;
    for call in ran.calls.iter().filter(|call| call.role != "synthesis") {
        assert!(
            call.inputs
                .iter()
                .all(|input| input.marker.contains(&format!("-{}-", call.branch))),
            "one loop received the other loop's history: {call:?}"
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_copy_keeps_its_last_words_and_failure_label_under_carry_on()
-> Result<(), Box<dyn Error>> {
    let mut case = Case::passing_on(2);
    case.failed_copy = true;
    let ran = run(case).await?;
    assert_synthesis(&ran, &[("left", 2)])?;
    let synthesis = ran.synthesis()?;
    let failed = synthesis
        .inputs
        .iter()
        .find(|input| input.marker == marker("left", "research", 2, 2))
        .ok_or("the failed copy was replaced by another result")?;
    assert!(
        failed.label.contains("did not pass"),
        "the failed copy was presented as accepted: {failed:?}"
    );
    assert!(
        !synthesis
            .inputs
            .iter()
            .any(|input| input.marker == marker("left", "research", 2, 1)),
        "the old good result hid the copy's later failure"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fan_in_without_a_loop_still_receives_all_copies_once() -> Result<(), Box<dyn Error>> {
    let mut case = Case::passing_on(1);
    case.looped = false;
    let ran = run(case).await?;
    assert_eq!(
        ran.synthesis()?.markers(),
        (1..=3)
            .map(|copy| marker("left", "research", copy, 1))
            .collect::<Vec<_>>()
    );
    Ok(())
}

fn marker(branch: &str, role: &str, copy: usize, turn: usize) -> String {
    format!("wf05-result-{branch}-{role}-copy{copy}-turn{turn}")
}

fn assert_synthesis(ran: &Ran, branches: &[(&str, usize)]) -> Result<(), Box<dyn Error>> {
    let mut expected = Vec::new();
    for &(branch, turn) in branches {
        expected.extend((1..=3).map(|copy| marker(branch, "research", copy, turn)));
        expected.push(marker(branch, "judge", 1, turn));
    }
    let synthesis = ran.synthesis()?;
    assert_eq!(
        synthesis.markers(),
        expected,
        "the loop must publish each copy's last actual result, not the last copy of the tile: {synthesis:?}"
    );
    Ok(())
}

fn agent(branch: &str, role: &str, copies: usize) -> Value {
    json!({
        "kind": "agent", "id": format!("s_{branch}_{role}"),
        "name": format!("{branch} {role}"), "agent": AGENT_ID, "overrides": {},
        "copies": copies, "instructions": format!("wf05:{branch}:{role}:copy{{{{copy}}}}"),
        "whenItFails": "carry-on", "folder": {"use": "fresh-copy"}, "at": {"x": 0, "y": 0}
    })
}

fn workflow(case: Case) -> Value {
    let mut steps = Vec::new();
    let mut links = Vec::new();
    for branch in if case.second_loop {
        vec!["left", "right"]
    } else {
        vec!["left"]
    } {
        let research = format!("s_{branch}_research");
        let judge = format!("s_{branch}_judge");
        steps.push(agent(branch, "research", 3));
        if case.looped {
            steps.push(agent(branch, "judge", 1));
            links.push(json!({"from": research, "to": judge}));
            links.push(json!({"from": judge, "to": research, "max_turns": 3}));
            links.push(json!({"from": judge, "to": "s_left_synthesis"}));
        } else {
            links.push(json!({"from": research, "to": "s_left_synthesis"}));
        }
    }
    steps.push(agent("left", "synthesis", 1));
    json!({"format": 1, "id": "wf_copy_history", "name": "Keep every copy", "steps": steps, "links": links})
}

#[derive(Clone, Debug)]
struct Input {
    marker: String,
    label: String,
}

#[derive(Clone, Debug)]
struct Call {
    branch: String,
    role: String,
    copy: usize,
    turn: usize,
    inputs: Vec<Input>,
}

impl Call {
    fn markers(&self) -> Vec<String> {
        self.inputs
            .iter()
            .map(|input| input.marker.clone())
            .collect()
    }
}

struct Ran {
    calls: Vec<Call>,
    finished: Vec<(String, usize, usize)>,
}

impl Ran {
    fn calls(&self, branch: &str, role: &str) -> Vec<&Call> {
        self.calls
            .iter()
            .filter(|call| call.branch == branch && call.role == role)
            .collect()
    }

    fn synthesis(&self) -> Result<&Call, Box<dyn Error>> {
        let calls = self.calls("left", "synthesis");
        match calls.as_slice() {
            [only] => Ok(only),
            _ => Err(format!("expected one synthesis, got {}", calls.len()).into()),
        }
    }

    fn contexts_for(&self, role: &str) -> BTreeMap<(String, usize, usize), Vec<String>> {
        self.calls
            .iter()
            .filter(|call| call.role == role)
            .map(|call| ((call.branch.clone(), call.copy, call.turn), call.markers()))
            .collect()
    }
}

#[derive(Debug, Default)]
struct Seen {
    // Oba zamki chronią krótkie próbki bez await; czekanie na innych ma osobny Barrier.
    calls: Mutex<Vec<Call>>,
    finished: Mutex<Vec<(String, usize, usize)>>,
}

#[derive(Debug)]
struct Reverse {
    meeting: Barrier,
    next: AtomicUsize,
    changed: Notify,
}

impl Reverse {
    fn new() -> Self {
        Self {
            meeting: Barrier::new(3),
            next: AtomicUsize::new(3),
            changed: Notify::new(),
        }
    }

    async fn wait_for_copy(&self, copy: usize) {
        self.meeting.wait().await;
        loop {
            // Rejestracja przed odczytem zapobiega zgubieniu wakeup pomiędzy nimi.
            let changed = self.changed.notified();
            if self.next.load(Ordering::Acquire) == copy {
                return;
            }
            changed.await;
        }
    }

    fn finished(&self) {
        self.next.fetch_sub(1, Ordering::AcqRel);
        self.changed.notify_waiters();
    }
}

async fn run(case: Case) -> Result<Ran, Box<dyn Error>> {
    let home = TempDir::new()?;
    let project = TempDir::new()?;
    fs::create_dir_all(home.path().join("agents"))?;
    fs::create_dir_all(home.path().join("workflows"))?;
    fs::write(home.path().join("agents/researcher.md"), AGENT)?;
    fs::write(project.path().join(".gitignore"), ".loadout/\n")?;
    git(project.path(), &["init", "--quiet"])?;
    git(project.path(), &["add", "-A"])?;
    git(
        project.path(),
        &["commit", "--quiet", "-m", "initial test input"],
    )?;
    fs::create_dir_all(project.path().join(".loadout"))?;
    let path = home.path().join("workflows/copy-history.json");
    fs::write(&path, serde_json::to_vec(&workflow(case))?)?;
    let store = Store::open(&project.path().join(".loadout/loadout.db"))?;
    let seen = Arc::new(Seen::default());
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        case,
        seen: Arc::clone(&seen),
        reverse: std::array::from_fn(|_| Arc::new(Reverse::new())),
    });
    let drivers: Drivers = Arc::new(move |_| Arc::clone(&driver));
    let deps = RunDeps {
        home: home.path(),
        project: project.path(),
        store: &store,
        drivers,
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: path,
        how_many_at_once: 6,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (sink, mut source) = line_channel(4096);
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| "copy-history run did not finish")??;
    assert!(
        !report.steps.is_empty(),
        "the fixture did not execute a graph"
    );
    while source.try_next().is_some() {}
    let calls = seen
        .calls
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let finished = seen
        .finished
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    Ok(Ran { calls, finished })
}

#[derive(Debug)]
struct Fake {
    case: Case,
    seen: Arc<Seen>,
    reverse: [Arc<Reverse>; 3],
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        VENDOR
    }
    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("wf05-test".to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let (branch, role, copy) = identity(&spec.prompt);
        let inputs = read_inputs(&spec.prompt)?;
        let turn = {
            let mut calls = self
                .seen
                .calls
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let turn = calls
                .iter()
                .filter(|call| call.branch == branch && call.role == role && call.copy == copy)
                .count()
                + 1;
            calls.push(Call {
                branch: branch.to_owned(),
                role: role.to_owned(),
                copy,
                turn,
                inputs,
            });
            turn
        };
        let passes_on = if branch == "right" {
            2
        } else {
            self.case.passes_on
        };
        let verdict = if role == "judge" {
            if turn >= passes_on {
                "outcome: pass\n"
            } else {
                "outcome: fail\n"
            }
        } else {
            ""
        };
        let text = format!(
            "## Answer\n{}\n\n## Evidence\nRead the supplied inputs.\n\n## Open questions\nNone.\n\n{verdict}",
            marker(branch, role, copy, turn)
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
        let reverse = if self.case.reverse && role == "research" {
            self.reverse.get(turn - 1).cloned()
        } else {
            None
        };
        let ok = !(self.case.failed_copy
            && branch == "left"
            && role == "research"
            && copy == 2
            && turn == 2);
        Ok(Box::new(Turn {
            events,
            session,
            text,
            ok,
            branch: branch.to_owned(),
            research: role == "research",
            copy,
            turn,
            reverse,
            seen: Arc::clone(&self.seen),
        }))
    }
}

fn identity(prompt: &str) -> (&'static str, &'static str, usize) {
    for branch in ["left", "right"] {
        for role in ["research", "judge", "synthesis"] {
            for copy in 1..=3 {
                if prompt.contains(&format!("wf05:{branch}:{role}:copy{copy}")) {
                    return (branch, role, copy);
                }
            }
        }
    }
    ("reflection", "reflection", 1)
}

fn read_inputs(prompt: &str) -> anyhow::Result<Vec<Input>> {
    let mut inputs = Vec::new();
    for line in prompt.lines() {
        let Some((_, addressed)) = line
            .strip_prefix("- ")
            .and_then(|line| line.split_once(": "))
        else {
            continue;
        };
        let Some((path, label)) = addressed.rsplit_once(" (") else {
            continue;
        };
        if !path.contains("/handoffs/") {
            continue;
        }
        let text = fs::read_to_string(path)?;
        let marker = text
            .lines()
            .find(|line| line.starts_with("wf05-result-"))
            .ok_or_else(|| anyhow::anyhow!("the addressed handoff has no result marker: {path}"))?;
        inputs.push(Input {
            marker: marker.to_owned(),
            label: label.trim_end_matches(')').to_owned(),
        });
    }
    Ok(inputs)
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    text: String,
    ok: bool,
    branch: String,
    research: bool,
    copy: usize,
    turn: usize,
    reverse: Option<Arc<Reverse>>,
    seen: Arc<Seen>,
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
        if let Some(reverse) = &self.reverse {
            reverse.wait_for_copy(self.copy).await;
        }
        // Nieudana tura zachowuje rzeczywiście wypowiedzianą prozę, nie pole text
        // w odrzuconym wyniku vendora (T-87). Fixture musi przejść tę samą drogę.
        let _ = self
            .events
            .send(
                AgentEvent::Said {
                    text: self.text.clone(),
                }
                .into(),
            )
            .await;
        let outcome = Outcome {
            ok: self.ok,
            reason: FinishReason::Completed,
            text: self.text.clone(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session.clone(),
        };
        if self.research {
            self.seen
                .finished
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((self.branch.clone(), self.turn, self.copy));
        }
        let _ = self
            .events
            .send(AgentEvent::Finished(outcome.clone()).into())
            .await;
        if let Some(reverse) = &self.reverse {
            reverse.finished();
        }
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
