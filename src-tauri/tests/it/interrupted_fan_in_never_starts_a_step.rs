//! WF-03: częściowe wejście nie staje się ani pracą agenta, ani wynikiem do wznowienia.
//!
//! 2026-09-05: per-run fault zwraca błąd po prawdziwym zapisie pierwszego pliku. Nie
//! wykonuje rollbacku, nie stawia markerów i nie zmienia odpowiedzi sterownika. Baseline
//! odmawia konsumentowi, ale końcowe `isolate::finish` zapisuje częściowy cel na gałęzi.

use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::reconcile::reconcile_runs;
use loadout_lib::commands::run::{
    PrestartFaultInjector, PrestartFaultPoint, RolledBackResource, run_workflow_inner,
    run_workflow_with_prestart_faults,
};
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunError, RunReport, RunRequest, rerun};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::mpsc;

const LEFT: &str = "left";
const RIGHT: &str = "right";
const CONSUMER: &str = "consumer";
const AFTER: &str = "after";
const WRITTEN_LEFT: &str = "the complete left result";
const WRITTEN_RIGHT: &str = "the complete right result";
const FAULT: &str = "WF03: storage refused after the first applied file";
const PATIENCE: Duration = Duration::from_secs(30);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_resumed_consumer_keeps_its_own_edit_against_the_previous_import()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let mut workflow: Value = serde_json::from_slice(&fs::read(&bench.workflow)?)?;
    workflow["steps"][2]["instructions"] = json!("WF03_TASK=consumer WF03_OWN_EDIT");
    fs::write(&bench.workflow, serde_json::to_vec(&workflow)?)?;
    let first = bench.go(&bench.request(), None).await?.0?;
    assert_eq!(first.steps, vec![StepState::Succeeded; 3]);
    let original = fs::read(first.dir.join("run.json"))?;
    let mut request = bench.request();
    request.handoffs_from = Some(first.dir.clone());
    let (result, delivered) = bench.go(&request, None).await?;
    let resumed = result?;
    assert_eq!(
        resumed.steps,
        vec![StepState::Succeeded; 3],
        "{}",
        delivered.text()
    );
    // Niezmieniony wynik nie musi tworzyć nowej gałęzi. Mierzymy wejście prawdziwego
    // drugiego RunSpec przed pracą konsumenta, nie nazwę niepotrzebnego nowego commita.
    let observed = bench.seen.lock().unwrap_or_else(PoisonError::into_inner);
    assert_eq!(
        observed
            .iter()
            .rev()
            .find(|one| one.step == CONSUMER)
            .and_then(|one| one.a.as_deref()),
        Some("consumer's own edit"),
        "the consumer only rewrote an edit which the importer had lost"
    );
    drop(observed);
    assert_eq!(fs::read(first.dir.join("run.json"))?, original);
    assert_eq!(
        fs::read_to_string(bench.project.path().join("a.txt"))?,
        "original a"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_error_after_the_first_file_never_commits_a_partial_result() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let fault = Arc::new(AfterFirstFile::default());
    let (result, delivered) = bench.go(&bench.request(), Some(fault.clone())).await?;
    let report = result?;
    assert_injected(&fault);
    assert!(
        !bench.consumer_started(),
        "a partial input reached the consumer's real RunSpec.cwd"
    );
    assert_eq!(
        report.steps,
        vec![
            StepState::Succeeded,
            StepState::Succeeded,
            StepState::Failed
        ]
    );
    assert!(
        !input_ready(&report.dir)?,
        "failed publication must never claim inputReady"
    );
    bench.assert_complete_parents(&report)?;
    let history = read_run_inner(bench.project.path(), &run_name(&report)?)?;
    let consumer = history
        .steps
        .iter()
        .find(|step| step.tile == CONSUMER)
        .ok_or("consumer missing from history")?;
    assert!(
        consumer.error.contains(FAULT),
        "history lost the actual publication failure: {}",
        consumer.error
    );
    assert!(
        delivered.text().contains(FAULT),
        "the window never received the refusal"
    );
    bench.assert_not_committed_as_result(&report)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn recovery_and_resume_do_not_promote_the_partial_copy_to_a_completed_input()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let fault = Arc::new(AfterFirstFile::default());
    let (result, _) = bench.go(&bench.request(), Some(fault.clone())).await?;
    let first = result?;
    assert_injected(&fault);
    let _recovered = reconcile_runs(bench.project.path());
    assert!(!input_ready(&first.dir)?);
    bench.assert_complete_parents(&first)?;
    bench.assert_not_committed_as_result(&first)?;
    let before_resume = fs::read(first.dir.join("run.json"))?;

    let again = rerun::onward(
        bench.home.path(),
        bench.project.path(),
        &run_name(&first)?,
        CONSUMER,
        2,
    )?;
    let (result, _) = bench.go(&again.request, None).await?;
    // Bezpieczna implementacja może odmówić albo odbudować kompletne wejście z obu zachowanych
    // rodziców. Nie może wystartować na częściowej gałęzi zapisanej po poprzednim błędzie.
    let seen = bench.seen.lock().unwrap_or_else(PoisonError::into_inner);
    for observation in seen.iter().filter(|one| one.step == CONSUMER) {
        assert_eq!(observation.a.as_deref(), Some(WRITTEN_LEFT));
        assert_eq!(
            observation.b.as_deref(),
            Some(WRITTEN_RIGHT),
            "resume handed the consumer the partial destination from the failed publication"
        );
    }
    if seen.iter().any(|one| one.step == CONSUMER) {
        let resumed = result?;
        assert!(resumed.steps.contains(&StepState::Succeeded));
        assert!(
            input_ready(&resumed.dir)?,
            "reconstructed input needs its own durable ready fact"
        );
    }
    drop(seen);
    assert_eq!(
        fs::read(first.dir.join("run.json"))?,
        before_resume,
        "resume must not rewrite the source run into a successful run"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_ready_fact_is_written_only_for_a_fully_applied_input() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let (result, _) = bench.go(&bench.request(), None).await?;
    let report = result?;
    assert_eq!(report.steps, vec![StepState::Succeeded; 3]);
    assert!(bench.consumer_started());
    let seen = bench.seen.lock().unwrap_or_else(PoisonError::into_inner);
    let consumer = seen
        .iter()
        .find(|one| one.step == CONSUMER)
        .ok_or("consumer did not run")?;
    assert_eq!(consumer.a.as_deref(), Some(WRITTEN_LEFT));
    assert_eq!(consumer.b.as_deref(), Some(WRITTEN_RIGHT));
    assert!(
        input_ready(&report.dir)?,
        "the successful consumer has no durable inputReady fact"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn refusal_before_writing_and_before_ready_both_keep_the_input_unusable()
-> Result<(), Box<dyn Error>> {
    for applied in [0, 2] {
        let bench = Bench::new()?;
        let fault = Arc::new(BoundaryFault::new(applied, Mutation::Refuse));
        let (result, delivered) = bench.go(&bench.request(), Some(fault.clone())).await?;
        let report = result?;
        assert_eq!(fault.times(), 1);
        assert!(!bench.consumer_started());
        assert!(!input_ready(&report.dir)?);
        assert!(delivered.text().contains("WF03 boundary refusal"));
        bench.assert_complete_parents(&report)?;
        bench.assert_not_committed_as_result(&report)?;
        let copy = report.dir.join("work").join(CONSUMER);
        assert_eq!(
            fs::read_to_string(copy.join("a.txt"))?,
            if applied == 0 {
                "original a"
            } else {
                WRITTEN_LEFT
            }
        );
        assert_eq!(
            fs::read_to_string(copy.join("b.txt"))?,
            if applied == 0 {
                "original b"
            } else {
                WRITTEN_RIGHT
            }
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stop_after_a_file_is_cancellation_and_never_a_ready_input() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let control = RunControl::new();
    let fault = Arc::new(BoundaryFault::new(1, Mutation::Stop(control.clone())));
    let (result, _) = bench
        .go_with_control(&bench.request(), Some(fault.clone()), control)
        .await?;
    let report = result?;
    assert_eq!(fault.times(), 1);
    assert!(!bench.consumer_started());
    assert!(report.steps.contains(&StepState::Cancelled));
    assert!(!input_ready(&report.dir)?);
    bench.assert_complete_parents(&report)?;
    bench.assert_not_committed_as_result(&report)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_replaced_root_is_neither_followed_nor_removed_during_recovery()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let victim = TempDir::new()?;
    fs::write(victim.path().join("a.txt"), "outside a")?;
    fs::write(victim.path().join("b.txt"), "outside b")?;
    let fault = Arc::new(BoundaryFault::new(
        0,
        Mutation::ReplaceRoot(victim.path().to_path_buf()),
    ));
    let (result, _) = bench.go(&bench.request(), Some(fault.clone())).await?;
    let report = result?;
    assert_eq!(fault.times(), 1);
    assert!(!bench.consumer_started());
    assert!(!input_ready(&report.dir)?);
    let _recovered = reconcile_runs(bench.project.path());
    assert_eq!(
        fs::read_to_string(victim.path().join("a.txt"))?,
        "outside a"
    );
    assert_eq!(
        fs::read_to_string(victim.path().join("b.txt"))?,
        "outside b"
    );
    assert!(
        fs::symlink_metadata(report.dir.join("work").join(CONSUMER))?
            .file_type()
            .is_symlink()
    );
    bench.assert_complete_parents(&report)?;
    bench.assert_not_committed_as_result(&report)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_same_bytes_on_a_foreign_inode_do_not_authorize_overwriting_it()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let fault = Arc::new(BoundaryFault::new(0, Mutation::ReplaceFile));
    let (result, _) = bench.go(&bench.request(), Some(fault.clone())).await?;
    let report = result?;
    assert_eq!(fault.times(), 1);
    assert!(!bench.consumer_started());
    assert!(!input_ready(&report.dir)?);
    let _recovered = reconcile_runs(bench.project.path());
    let copy = report.dir.join("work").join(CONSUMER);
    assert_eq!(fs::read_to_string(copy.join("b.txt"))?, "original b");
    // Stary obiekt nadal istnieje pod inną nazwą: nowy plik o tych samych bajtach nie mógł
    // dostać jego inode. Test nie opiera się na heurystyce czasów modyfikacji.
    assert_eq!(
        fs::read_to_string(
            copy.parent()
                .ok_or("copy parent missing")?
                .join("owned-original-b")
        )?,
        "original b"
    );
    bench.assert_complete_parents(&report)?;
    bench.assert_not_committed_as_result(&report)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn carry_on_cannot_pass_a_partial_copy_to_a_same_copy_successor() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let mut workflow: Value = serde_json::from_slice(&fs::read(&bench.workflow)?)?;
    let mut next = workflow["steps"][2].clone();
    next["id"] = json!(AFTER);
    next["name"] = json!("Use the carried-on copy");
    next["instructions"] = json!(format!("WF03_TASK={AFTER}"));
    workflow["steps"][2]["whenItFails"] = json!("carry-on");
    workflow["steps"]
        .as_array_mut()
        .ok_or("steps missing")?
        .push(next);
    workflow["links"]
        .as_array_mut()
        .ok_or("links missing")?
        .push(json!({"from":CONSUMER,"to":AFTER}));
    fs::write(&bench.workflow, serde_json::to_vec(&workflow)?)?;
    let fault = Arc::new(AfterFirstFile::default());
    let (result, delivered) = bench.go(&bench.request(), Some(fault.clone())).await?;
    let report = result?;
    assert_injected(&fault);
    assert!(!bench.consumer_started());
    assert!(
        !bench
            .seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|one| one.step == AFTER),
        "a same-copy successor bypassed inputReady because it did not combine its own parents"
    );
    let history = read_run_inner(bench.project.path(), &run_name(&report)?)?;
    assert!(
        history
            .steps
            .iter()
            .find(|one| one.tile == AFTER)
            .ok_or("successor missing")?
            .error
            .contains("not completely prepared")
    );
    assert!(delivered.text().contains("not completely prepared"));
    bench.assert_not_committed_as_result(&report)?;
    Ok(())
}

enum Mutation {
    Refuse,
    Stop(RunControl),
    ReplaceRoot(PathBuf),
    ReplaceFile,
}

struct BoundaryFault {
    applied: usize,
    mutation: Mutation,
    times: Mutex<usize>,
}

impl BoundaryFault {
    fn new(applied: usize, mutation: Mutation) -> Self {
        Self {
            applied,
            mutation,
            times: Mutex::new(0),
        }
    }
    fn times(&self) -> usize {
        *self.times.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl PrestartFaultInjector for BoundaryFault {
    fn check(&self, _point: PrestartFaultPoint) -> io::Result<()> {
        Ok(())
    }
    fn rolled_back(&self, _resource: &RolledBackResource) {}
    fn after_fan_in_operation(&self, into: &Path, applied: usize) -> io::Result<()> {
        if applied != self.applied {
            return Ok(());
        }
        *self.times.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        let parent = into
            .parent()
            .ok_or_else(|| io::Error::other("copy parent missing"))?;
        match &self.mutation {
            Mutation::Refuse => return Err(io::Error::other("WF03 boundary refusal")),
            Mutation::Stop(control) => control.stop(),
            Mutation::ReplaceRoot(victim) => {
                fs::rename(into, parent.join("owned-original-consumer"))?;
                loadout_lib::engine::supervisor::link(victim, into)?;
            }
            Mutation::ReplaceFile => {
                fs::rename(into.join("b.txt"), parent.join("owned-original-b"))?;
                fs::write(into.join("b.txt"), "original b")?;
            }
        }
        Ok(())
    }
}

fn run_name(report: &RunReport) -> Result<String, Box<dyn Error>> {
    report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| "run folder has no name".into())
}

fn input_ready(run: &Path) -> Result<bool, Box<dyn Error>> {
    let marker = run.join(".isolation").join(CONSUMER);
    let description: Value = serde_json::from_slice(&fs::read(marker)?)?;
    Ok(description
        .get("fanIn")
        .and_then(|one| one.get("inputReady"))
        .and_then(Value::as_bool)
        == Some(true))
}

#[derive(Debug, Default)]
struct AfterFirstFile(Mutex<Vec<(PathBuf, Option<String>)>>);

impl PrestartFaultInjector for AfterFirstFile {
    fn check(&self, _point: PrestartFaultPoint) -> io::Result<()> {
        Ok(())
    }
    fn rolled_back(&self, _resource: &RolledBackResource) {}
    fn after_fan_in_operation(&self, into: &Path, applied: usize) -> io::Result<()> {
        if applied != 1 {
            return Ok(());
        }
        self.0.lock().unwrap_or_else(PoisonError::into_inner).push((
            into.to_path_buf(),
            fs::read_to_string(into.join("a.txt")).ok(),
        ));
        Err(io::Error::other(FAULT))
    }
}

fn assert_injected(fault: &AfterFirstFile) {
    let observations = fault.0.lock().unwrap_or_else(PoisonError::into_inner);
    assert_eq!(
        observations.len(),
        1,
        "the fault must run once after a real operation, not before the run"
    );
    assert_eq!(
        observations[0].1.as_deref(),
        Some(WRITTEN_LEFT),
        "the injection did not witness a completed write to the real destination"
    );
}

struct Bench {
    home: TempDir,
    project: TempDir,
    workflow: PathBuf,
    base: String,
    seen: Arc<Mutex<Vec<Observation>>>,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(project.path().join("a.txt"), "original a")?;
        fs::write(project.path().join("b.txt"), "original b")?;
        fs::write(project.path().join(".gitignore"), ".loadout/\n")?;
        git(project.path(), &["init", "--quiet"])?;
        git(project.path(), &["add", "-A"])?;
        git(
            project.path(),
            &["commit", "--quiet", "-m", "WF-03 baseline"],
        )?;
        let base = git(project.path(), &["rev-parse", "HEAD"])?;
        let agent = "01990000-0000-7000-8000-0000000000a3";
        fs::write(
            home.path().join("agents/scribe.md"),
            format!(
                "---\nschema: 1\nid: {agent}\nname: Scribe\nsummary: Writes files\ncolor: slate\nrunsWith: claude-code\nmodel: opus\nthinking: balanced\nfileAccess: work-freely\ngiveUpAfterMinutes: 20\nwriteResultsTo: \"\"\ntools: everything\nskills: []\nconnections: []\n---\nDo the work.\n"
            ),
        )?;
        let workflow = home.path().join("workflows/interrupted.json");
        let steps: Vec<_> = [(LEFT, "Produce left result", "fresh-copy"), (RIGHT, "Produce right result", "fresh-copy"), (CONSUMER, "Use both completed results", "same-copy")]
            .into_iter().map(|(id, name, folder)| json!({
                "kind":"agent", "id":id, "name":name, "agent":agent,
                "instructions":format!("WF03_TASK={id}"), "folder":{"use":folder}, "at":{"x":0,"y":0}
            })).collect();
        fs::write(
            &workflow,
            serde_json::to_vec(&json!({
                "format":1, "id":"wf03_interrupted", "name":"Do not accept partial inputs", "steps":steps,
                "links":[{"from":LEFT,"to":CONSUMER},{"from":RIGHT,"to":CONSUMER}]
            }))?,
        )?;
        Ok(Self {
            home,
            project,
            workflow,
            base,
            seen: Arc::new(Mutex::new(Vec::new())),
        })
    }

    fn request(&self) -> RunRequest {
        RunRequest {
            workflow: self.workflow.clone(),
            how_many_at_once: 2,
            task: None,
            part: None,
            handoffs_from: None,
        }
    }

    async fn go(
        &self,
        request: &RunRequest,
        faults: Option<Arc<dyn PrestartFaultInjector>>,
    ) -> Result<(Result<RunReport, RunError>, Delivered), Box<dyn Error>> {
        self.go_with_control(request, faults, RunControl::new())
            .await
    }

    async fn go_with_control(
        &self,
        request: &RunRequest,
        faults: Option<Arc<dyn PrestartFaultInjector>>,
        control: RunControl,
    ) -> Result<(Result<RunReport, RunError>, Delivered), Box<dyn Error>> {
        let driver: Arc<dyn AgentDriver> = Arc::new(Fake(Arc::clone(&self.seen)));
        let drivers: Drivers = Arc::new(move |_| Arc::clone(&driver));
        let store = Store::open(&self.project.path().join(".loadout/loadout.db"))?;
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store: &store,
            drivers,
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control,
        };
        let delivered = Delivered::default();
        let (sink, source) = line_channel(QUEUE_CAP);
        let pump = spawn_pump(source, delivered.channel());
        let result = match faults {
            Some(faults) => {
                tokio::time::timeout(
                    PATIENCE,
                    run_workflow_with_prestart_faults(&deps, request, sink, faults),
                )
                .await?
            }
            None => {
                tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, request, sink)).await?
            }
        };
        tokio::time::timeout(PATIENCE, pump).await??;
        Ok((result, delivered))
    }

    fn consumer_started(&self) -> bool {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|one| one.step == CONSUMER)
    }

    fn assert_complete_parents(&self, report: &RunReport) -> Result<(), Box<dyn Error>> {
        for (step, file, expected) in [
            (LEFT, "a.txt", WRITTEN_LEFT),
            (RIGHT, "b.txt", WRITTEN_RIGHT),
        ] {
            let source = format!("loadout/{}/{step}:{file}", report.id);
            assert_eq!(
                git(self.project.path(), &["show", &source])?,
                expected,
                "the failed merge lost the complete parent {step}"
            );
        }
        Ok(())
    }

    fn assert_not_committed_as_result(&self, report: &RunReport) -> Result<(), Box<dyn Error>> {
        let branch = format!("refs/heads/loadout/{}/{CONSUMER}", report.id);
        if let Ok(oid) = git(self.project.path(), &["rev-parse", "--verify", &branch]) {
            assert_eq!(
                oid.trim(),
                self.base.trim(),
                "finalization committed the partially prepared destination as the consumer's completed work"
            );
        }
        assert_eq!(
            fs::read_to_string(self.project.path().join("a.txt"))?,
            "original a"
        );
        assert_eq!(
            fs::read_to_string(self.project.path().join("b.txt"))?,
            "original b"
        );
        Ok(())
    }
}

#[derive(Debug)]
struct Observation {
    step: &'static str,
    a: Option<String>,
    b: Option<String>,
}

struct Fake(Arc<Mutex<Vec<Observation>>>);

#[async_trait]
impl AgentDriver for Fake {
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
        let step = [CONSUMER, AFTER, LEFT, RIGHT]
            .into_iter()
            .find(|id| spec.prompt.contains(&format!("WF03_TASK={id}")))
            .ok_or_else(|| anyhow::anyhow!("unrecognized fixture task"))?;
        match step {
            LEFT => fs::write(spec.cwd.join("a.txt"), WRITTEN_LEFT)?,
            RIGHT => fs::write(spec.cwd.join("b.txt"), WRITTEN_RIGHT)?,
            _ => {}
        }
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Observation {
                step,
                a: fs::read_to_string(spec.cwd.join("a.txt")).ok(),
                b: fs::read_to_string(spec.cwd.join("b.txt")).ok(),
            });
        if step == CONSUMER && spec.prompt.contains("WF03_OWN_EDIT") {
            fs::write(spec.cwd.join("a.txt"), "consumer's own edit")?;
        }
        Ok(Box::new(Turn {
            events,
            session: SessionRef {
                vendor: self.id(),
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
    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }
    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        let outcome = Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "The requested files are ready.".to_owned(),
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

#[derive(Clone, Default)]
struct Delivered(Arc<Mutex<Vec<String>>>);

impl Delivered {
    fn channel(&self) -> tauri::ipc::Channel<Vec<loadout_lib::engine::line::Line>> {
        let seen = Arc::clone(&self.0);
        tauri::ipc::Channel::new(move |body| {
            if let tauri::ipc::InvokeResponseBody::Json(text) = body {
                seen.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(text);
            }
            Ok(())
        })
    }
    fn text(&self) -> String {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .join("\n")
    }
}
