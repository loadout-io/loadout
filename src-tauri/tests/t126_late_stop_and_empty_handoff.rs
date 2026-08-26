//! T-126 AC-3: reflection is opt-in, semantic emptiness skips it and late Stop reaps it.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use loadout_lib::commands::memory::project_notes_root;
use loadout_lib::commands::run::{run_workflow_with_reflection, stop_run_inner};
use loadout_lib::commands::{Drivers, Outcome, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, StepSettings, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{
    GroupId, GroupProof, StdinPlan, Supervised, reap_group, spawn,
};
use loadout_lib::evidence::{EvidenceIdentity, EvidenceTarget};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::Vendor;
use loadout_lib::memory::handoff::{Section, read_handoff, scan_run_dir};
use loadout_lib::memory::notes::scan_notes;
use loadout_lib::store::Store;
use serde_json::Value;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::process::Command;
use tokio::sync::mpsc;

const PATIENCE: Duration = Duration::from_secs(20);
const PROCESS_GRACE: Duration = Duration::from_millis(120);
const STEP_ANSWER: &str = "## Answer\nQueue fixed.\n\n## Evidence\nqueue.rs:7\n\n## Open\nNone.\n";
const REFLECTION_ANSWER: &str = "rule: T126-LATE keep the queue owner in one place\n\
because: the Build handoff and run.json show one owner\n";

const AGENT: &str = r"---
schema: 1
id: 01990000-0000-7000-8000-000000000127
name: T126 Builder
summary: Exercises late reflection
color: slate
runsWith: claude-code
model: haiku
thinking: balanced
fileAccess: look-only
giveUpAfterMinutes: 20
writeResultsTo: handoffs/result.md
tools: everything
skills: []
connections: []
---
Build the requested change.
";

const ONE_STEP: &str = r#"{
  "format": 1, "id": "wf_t126_one", "name": "One useful step",
  "steps": [{
    "kind": "agent", "id": "build", "name": "Build",
    "agent": "01990000-0000-7000-8000-000000000127", "overrides": {},
    "instructions": "Return the fixture result.", "folder": { "use": "project" },
    "at": { "x": 0, "y": 0 }
  }],
  "links": []
}"#;

const TWO_STEPS: &str = r#"{
  "format": 1, "id": "wf_t126_empty", "name": "Empty handoff",
  "steps": [
    {"kind":"agent","id":"first","name":"First","agent":"01990000-0000-7000-8000-000000000127","overrides":{},"instructions":"Leave nothing.","folder":{"use":"project"},"at":{"x":0,"y":0}},
    {"kind":"agent","id":"second","name":"Second","agent":"01990000-0000-7000-8000-000000000127","overrides":{},"instructions":"Read the handoff.","folder":{"use":"project"},"at":{"x":200,"y":0}}
  ],
  "links": [{"from":"first","to":"second"}]
}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disabled_and_useful_runs_start_zero_then_one_reflection() -> Result<(), Box<dyn Error>> {
    let disabled = Bench::new(ONE_STEP)?;
    let (plain, plain_seen) = run_fixture(&disabled, Scenario::Useful, false).await?;
    assert_eq!(plain.steps, vec![StepState::Succeeded]);
    assert_eq!(plain_seen.reflections.load(Ordering::Acquire), 0);
    assert_reflection_ran(&plain, false)?;

    let enabled = Bench::new(ONE_STEP)?;
    let (learned, learned_seen) = run_fixture(&enabled, Scenario::Useful, true).await?;
    assert_eq!(learned.steps, vec![StepState::Succeeded]);
    assert_eq!(learned_seen.reflections.load(Ordering::Acquire), 1);
    assert_reflection_ran(&learned, true)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn empty_successful_handoff_stays_successful_and_skips_reflection()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new(TWO_STEPS)?;
    let (report, seen) = run_fixture(&bench, Scenario::Empty, true).await?;
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded, StepState::Succeeded]
    );
    assert_eq!(seen.reflections.load(Ordering::Acquire), 0);
    let handoffs = scan_run_dir(&report.dir)?;
    assert_eq!(handoffs.len(), 2);
    assert!(handoffs[0].left_nothing());
    let visible = lock(&seen.visible_to_second)?
        .clone()
        .ok_or("second reader saw no handoff")?;
    let headings: Vec<&str> = visible
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let expected: Vec<String> = [Section::Answer, Section::Evidence, Section::Open]
        .map(|section| format!("## {}", section.name()))
        .into_iter()
        .collect();
    assert_eq!(headings, expected);
    assert!(scan_notes(&bench.home.path().join("memory"))?.is_empty());
    assert!(scan_notes(&project_notes_root(bench.project.path()))?.is_empty());
    assert_reflection_ran(&report, false)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn late_stop_waits_until_the_real_reflection_group_is_dead() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new(ONE_STEP)?;
    let seen = Arc::new(Seen::new(Scenario::Late, bench.ready.clone()));
    let store = Store::open(&bench.db())?;
    let control = RunControl::new();
    let deps = deps(&bench, &store, Arc::clone(&seen), control);
    let request = request(&bench);
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let run = run_workflow_with_reflection(&deps, &request, sink, None, true);
    tokio::pin!(run);

    let group = wait_until_reflection_is_alive(&seen, &bench.ready, &mut run).await?;
    assert!(matches!(reap_group(group.pgid), GroupProof::Alive));
    let (stopped, completed) = tokio::time::timeout(PATIENCE, async {
        tokio::join!(stop_run_inner(&deps), &mut run)
    })
    .await?;
    let stopped = stopped?;
    let report = completed?;
    tokio::time::timeout(PATIENCE, pump).await??;

    assert_eq!(stopped, Outcome::Cancelled);
    assert_eq!(report.outcome, Outcome::Cancelled);
    assert!(matches!(reap_group(group.pgid), GroupProof::Dead { .. }));
    assert_eq!(seen.reflections.load(Ordering::Acquire), 1);
    assert!(scan_notes(&bench.home.path().join("memory"))?.is_empty());
    assert!(scan_notes(&project_notes_root(bench.project.path()))?.is_empty());
    assert_reflection_ran(&report, false)?;
    assert_no_reflection_cost(&report)?;
    Ok(())
}

async fn wait_until_reflection_is_alive<F>(
    seen: &Seen,
    ready: &Path,
    run: &mut std::pin::Pin<&mut F>,
) -> Result<GroupId, Box<dyn Error>>
where
    F: std::future::Future<Output = Result<RunReport, loadout_lib::commands::RunError>>,
{
    let waiting = wait_for_group_and_marker(seen, ready);
    tokio::pin!(waiting);
    tokio::select! {
        group = &mut waiting => group,
        result = run => Err(format!("run returned before late Stop: {result:?}").into()),
    }
}

async fn wait_for_group_and_marker(seen: &Seen, ready: &Path) -> Result<GroupId, Box<dyn Error>> {
    let deadline = Instant::now() + PATIENCE;
    while Instant::now() < deadline {
        let group = *lock(&seen.group)?;
        if let Some(group) = group.filter(|_| ready.exists()) {
            return Ok(group);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("reflection did not expose a live process group".into())
}

async fn run_fixture(
    bench: &Bench,
    scenario: Scenario,
    reflection_enabled: bool,
) -> Result<(RunReport, Arc<Seen>), Box<dyn Error>> {
    let seen = Arc::new(Seen::new(scenario, bench.ready.clone()));
    let store = Store::open(&bench.db())?;
    let deps = deps(bench, &store, Arc::clone(&seen), RunControl::new());
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let report = tokio::time::timeout(
        PATIENCE,
        run_workflow_with_reflection(&deps, &request(bench), sink, None, reflection_enabled),
    )
    .await??;
    tokio::time::timeout(PATIENCE, pump).await??;
    Ok((report, seen))
}

fn deps<'a>(
    bench: &'a Bench,
    store: &'a Store,
    seen: Arc<Seen>,
    control: RunControl,
) -> RunDeps<'a> {
    RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store,
        drivers: fake_drivers(seen),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control,
    }
}

fn request(bench: &Bench) -> RunRequest {
    RunRequest {
        workflow: bench.workflow(),
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    }
}

fn assert_reflection_ran(report: &RunReport, expected: bool) -> Result<(), Box<dyn Error>> {
    let run: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    assert_eq!(run.pointer("/reflection/ran"), Some(&Value::Bool(expected)));
    Ok(())
}

fn assert_no_reflection_cost(report: &RunReport) -> Result<(), Box<dyn Error>> {
    let run: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    assert!(
        run.pointer("/reflection/cost_usd")
            .is_none_or(Value::is_null)
    );
    Ok(())
}

fn fake_drivers(seen: Arc<Seen>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake::step(seen));
    Arc::new(move |_vendor: Vendor| Arc::clone(&driver))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scenario {
    Useful,
    Empty,
    Late,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Step,
    Reflection,
}

struct Seen {
    scenario: Scenario,
    steps: AtomicUsize,
    reflections: AtomicUsize,
    visible_to_second: Mutex<Option<String>>,
    group: Mutex<Option<GroupId>>,
    ready: PathBuf,
}

impl Seen {
    fn new(scenario: Scenario, ready: PathBuf) -> Self {
        Self {
            scenario,
            steps: AtomicUsize::new(0),
            reflections: AtomicUsize::new(0),
            visible_to_second: Mutex::new(None),
            group: Mutex::new(None),
            ready,
        }
    }
}

#[derive(Clone)]
struct Fake {
    mode: Mode,
    seen: Arc<Seen>,
    settings: Option<StepSettings>,
    evidence: Option<EvidenceTarget>,
    budget: Option<f64>,
}

impl Fake {
    fn step(seen: Arc<Seen>) -> Self {
        Self {
            mode: Mode::Step,
            seen,
            settings: None,
            evidence: None,
            budget: None,
        }
    }

    fn clone_with(&self) -> Self {
        Self {
            mode: self.mode,
            seen: Arc::clone(&self.seen),
            settings: self.settings.clone(),
            evidence: self.evidence.clone(),
            budget: self.budget,
        }
    }
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        "fake"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("t126".to_owned()),
        })
    }

    fn reflecting(&self) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            mode: Mode::Reflection,
            seen: Arc::clone(&self.seen),
            settings: None,
            evidence: None,
            budget: None,
        }))
    }

    fn with_settings(
        &self,
        settings: &StepSettings,
    ) -> Option<anyhow::Result<Arc<dyn AgentDriver>>> {
        let mut clone = self.clone_with();
        clone.settings = Some(settings.clone());
        Some(Ok(Arc::new(clone)))
    }

    fn with_evidence(&self, target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        let mut clone = self.clone_with();
        clone.evidence = Some(target);
        Some(Arc::new(clone))
    }

    fn with_budget(&self, dollars: f64) -> Option<Arc<dyn AgentDriver>> {
        let mut clone = self.clone_with();
        clone.budget = Some(dollars);
        Some(Arc::new(clone))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        match self.mode {
            Mode::Step => self.start_step(spec, events).await,
            Mode::Reflection => self.start_reflection(spec, events).await,
        }
    }
}

impl Fake {
    async fn start_step(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let index = self.seen.steps.fetch_add(1, Ordering::AcqRel);
        if index == 1 {
            let visible = first_markdown_in(&spec.extra_dirs)?;
            *lock(&self.seen.visible_to_second)? = visible;
        }
        let text = if self.seen.scenario == Scenario::Empty {
            ""
        } else {
            STEP_ANSWER
        };
        ready_turn(events, spec, text.to_owned(), Some(0.01)).await
    }

    async fn start_reflection(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.validate_reflection(&spec)?;
        self.seen.reflections.fetch_add(1, Ordering::AcqRel);
        if self.seen.scenario != Scenario::Late {
            return ready_turn(events, spec, REFLECTION_ANSWER.to_owned(), Some(0.02)).await;
        }
        let script = self
            .seen
            .ready
            .parent()
            .ok_or_else(|| anyhow::anyhow!("reflection marker has no parent"))?
            .join("reflection.sh");
        let command = Command::new(script);
        let process = spawn(command, StdinPlan::Null)?;
        let group = process.group();
        *lock(&self.seen.group)? = Some(group);
        let session = announce(&events, &spec).await;
        Ok(Box::new(ProcessTurn {
            process,
            group,
            session,
        }))
    }

    fn validate_reflection(&self, spec: &RunSpec) -> anyhow::Result<()> {
        let settings = self
            .settings
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no settings"))?;
        let target = self
            .evidence
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no evidence"))?;
        if settings.dir != spec.cwd || !matches!(target.identity(), EvidenceIdentity::Reflection) {
            return Err(anyhow::anyhow!(
                "reflection wrappers do not describe this private turn"
            ));
        }
        if !self.budget.is_some_and(|budget| budget > 0.0) {
            return Err(anyhow::anyhow!("reflection has no positive budget"));
        }
        Ok(())
    }
}

async fn ready_turn(
    events: mpsc::Sender<DecodedEvent>,
    spec: RunSpec,
    text: String,
    cost: Option<f64>,
) -> anyhow::Result<Box<dyn AgentHandle>> {
    let session = announce(&events, &spec).await;
    Ok(Box::new(Turn {
        events,
        session,
        text,
        cost,
    }))
}

async fn announce(events: &mpsc::Sender<DecodedEvent>, spec: &RunSpec) -> SessionRef {
    let session = SessionRef {
        vendor: "fake",
        id: spec.run_id.to_string(),
    };
    let _ = events
        .send(
            AgentEvent::Started {
                session: session.clone(),
                model: spec.model.clone().unwrap_or_default(),
                tools: Vec::new(),
                capabilities: Vec::new(),
            }
            .into(),
        )
        .await;
    session
}

struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    text: String,
    cost: Option<f64>,
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
        let outcome = turn_outcome(&self.session, true, self.text.clone(), self.cost);
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

struct ProcessTurn {
    process: Supervised,
    group: GroupId,
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for ProcessTurn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        Some(self.group)
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        let status = self.process.wait().await?;
        Ok(turn_outcome(
            &self.session,
            status.success(),
            String::new(),
            None,
        ))
    }

    async fn cancel(&mut self) -> GroupProof {
        self.process.stop(PROCESS_GRACE).await
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(self.process.wait().await?.code())
    }
}

fn turn_outcome(session: &SessionRef, ok: bool, text: String, cost: Option<f64>) -> TurnOutcome {
    TurnOutcome {
        ok,
        reason: FinishReason::Completed,
        text,
        cost_usd: cost,
        tokens: Tokens::default(),
        turns: 1,
        took: Duration::from_millis(2),
        session: session.clone(),
    }
}

struct Bench {
    home: TempDir,
    project: TempDir,
    ready: PathBuf,
}

impl Bench {
    fn new(workflow: &str) -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(home.path().join("memory").join("notes"))?;
        fs::create_dir_all(project_notes_root(project.path()).join("notes"))?;
        fs::write(home.path().join("agents").join("builder.md"), AGENT)?;
        fs::write(home.path().join("workflows").join("t126.json"), workflow)?;
        let ready = project.path().join("reflection-ready");
        let script = project.path().join("reflection.sh");
        fs::write(&script, reflection_script(&ready))?;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700))?;
        Ok(Self {
            home,
            project,
            ready,
        })
    }

    fn workflow(&self) -> PathBuf {
        self.home.path().join("workflows").join("t126.json")
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }
}

fn reflection_script(ready: &Path) -> String {
    format!(
        "#!/bin/sh\ntrap '' TERM\nprintf ready > '{}'\nwhile :; do /bin/sleep 1; done\n",
        ready.display()
    )
}

fn first_markdown_in(extra_dirs: &[PathBuf]) -> anyhow::Result<Option<String>> {
    for dir in extra_dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|extension| extension == "md") {
                return Ok(Some(read_handoff(&path)?.body));
            }
        }
    }
    Ok(None)
}

fn lock<T>(mutex: &Mutex<T>) -> std::io::Result<std::sync::MutexGuard<'_, T>> {
    mutex
        .lock()
        .map_err(|_| std::io::Error::other("T-126 fixture mutex was poisoned"))
}
