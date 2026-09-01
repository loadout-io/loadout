//! T-202 AC-1: workflow, agent i evidence publikują przez jeden trwały rdzeń.
//!
//! Spec celowo używa produkcyjnych save/load oraz produkcyjnego publishera. Fault injector
//! wybiera wyłącznie punkt przerwania; nie zapisuje pliku i nie ma własnego algorytmu.

#![allow(clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier, Mutex, MutexGuard, PoisonError};
use std::thread;

use loadout_lib::durable_file::{
    DEFINITION_FILE_MODE, DurableFilePublisher, FaultAction, FaultInjector, FaultPoint, ModePolicy,
    PRIVATE_FILE_MODE, PublicationEvent, PublicationOperation, PublishError, scoped_faults,
};
use loadout_lib::evidence::{EvidenceTarget, SafeInputManifest};
use loadout_lib::library::agents::{Agent, read_agent_file, write_agent_file};
use loadout_lib::memory::handoff::{BODY_CAP, Kind, MetaDraft, write_handoff};
use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::file::{load, save};
use serde_json::Map;
use tempfile::TempDir;

const FAULTS_AFTER_REAL_IO: [FaultPoint; 7] = [
    FaultPoint::AfterTempCreated,
    FaultPoint::AfterPartialWrite,
    FaultPoint::AfterWrite,
    FaultPoint::AfterFileSync,
    FaultPoint::BeforeCommit,
    FaultPoint::AfterCommit,
    FaultPoint::BeforeDirectorySync,
];

#[derive(Clone)]
struct Rule {
    point: FaultPoint,
    operation: PublicationOperation,
    target: PathBuf,
    action: FaultAction,
}

#[derive(Default)]
struct Faults {
    rule: Mutex<Option<Rule>>,
    events: Mutex<Vec<PublicationEvent>>,
}

impl Faults {
    fn arm(
        &self,
        point: FaultPoint,
        operation: PublicationOperation,
        target: &Path,
        action: FaultAction,
    ) {
        *lock(&self.rule) = Some(Rule {
            point,
            operation,
            target: target.to_owned(),
            action,
        });
    }

    fn events(&self) -> Vec<PublicationEvent> {
        lock(&self.events).clone()
    }
}

impl FaultInjector for Faults {
    fn action(&self, event: &PublicationEvent) -> FaultAction {
        lock(&self.events).push(event.clone());
        let mut rule = lock(&self.rule);
        if rule.as_ref().is_some_and(|armed| {
            armed.point == event.point
                && armed.operation == event.operation
                && armed.target == event.target
        }) {
            return rule
                .take()
                .map_or(FaultAction::Continue, |armed| armed.action);
        }
        FaultAction::Continue
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[test]
fn workflow_and_agent_stay_old_or_become_whole_at_every_fault_point() -> Result<(), Box<dyn Error>>
{
    for point in FAULTS_AFTER_REAL_IO {
        definition_fault_case(point, Definition::Workflow)?;
        definition_fault_case(point, Definition::Agent)?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Definition {
    Workflow,
    Agent,
}

fn definition_fault_case(point: FaultPoint, definition: Definition) -> Result<(), Box<dyn Error>> {
    let bench = TempDir::new()?;
    let faults = Arc::new(Faults::default());
    let hook: Arc<dyn FaultInjector> = faults.clone();
    let _scope = scoped_faults(bench.path(), hook)?;

    match definition {
        Definition::Workflow => {
            let target = bench.path().join("workflow.json");
            let old = workflow("old", "Old complete workflow");
            let new = workflow("new", "New complete workflow");
            save(&old, &target)?;
            let names_before = names(bench.path())?;

            faults.arm(
                point,
                PublicationOperation::Replace,
                &target,
                FaultAction::Fail,
            );
            let result = save(&new, &target);
            assert!(
                result.is_err(),
                "workflow save reported success when publication failed at {point:?}"
            );

            let reopened = load(&target)?;
            assert!(
                reopened == old || reopened == new,
                "after {point:?}, the workflow loader saw neither complete version: {reopened:?}"
            );
            assert_eq!(
                names(bench.path())?,
                names_before,
                "handled workflow failure at {point:?} left a publisher temp"
            );
        }
        Definition::Agent => {
            let dir = bench.path().join("agents");
            let old = agent("Old complete agent", "Old complete instructions.");
            let mut new = old.clone();
            "New complete agent".clone_into(&mut new.summary);
            "New complete instructions with a final byte.".clone_into(&mut new.instructions);
            let target = write_agent_file(&dir, &old)?;
            let names_before = names(&dir)?;

            faults.arm(
                point,
                PublicationOperation::Replace,
                &target,
                FaultAction::Fail,
            );
            let result = write_agent_file(&dir, &new);
            assert!(
                result.is_err(),
                "agent save reported success when publication failed at {point:?}"
            );

            let reopened = read_agent_file(&target)?;
            assert!(
                reopened == old || reopened == new,
                "after {point:?}, the agent loader saw neither complete version: {reopened:?}"
            );
            assert_eq!(
                names(&dir)?,
                names_before,
                "handled agent failure at {point:?} left a publisher temp"
            );
        }
    }
    Ok(())
}

#[test]
fn definition_modes_are_explicit_and_replace_preserves_the_existing_mode()
-> Result<(), Box<dyn Error>> {
    let bench = TempDir::new()?;
    let workflow_path = bench.path().join("existing.json");
    fs::write(&workflow_path, serde_json::to_vec(&workflow("old", "Old"))?)?;
    fs::set_permissions(&workflow_path, fs::Permissions::from_mode(0o640))?;
    save(&workflow("new", "New"), &workflow_path)?;
    assert_eq!(
        mode(&workflow_path)?,
        0o640,
        "replace changed an established definition mode"
    );

    let new_workflow = bench.path().join("new.json");
    save(&workflow("fresh", "Fresh"), &new_workflow)?;
    assert_eq!(mode(&new_workflow)?, DEFINITION_FILE_MODE);
    assert_ne!(
        DEFINITION_FILE_MODE & 0o200,
        0,
        "the documented definition default must be owner-writable"
    );
    assert_eq!(
        DEFINITION_FILE_MODE & 0o022,
        0,
        "the documented definition default may not be writable by group or others"
    );

    let agents = bench.path().join("agents");
    let agent_path = write_agent_file(&agents, &agent("Fresh agent", "Complete body."))?;
    assert_eq!(mode(&agent_path)?, DEFINITION_FILE_MODE);
    Ok(())
}

#[test]
fn unsafe_names_and_symlinks_are_refused_without_touching_the_outside() -> Result<(), Box<dyn Error>>
{
    let controlled = TempDir::new()?;
    let outside = TempDir::new()?;
    let outside_file = outside.path().join("outside.json");
    fs::write(&outside_file, b"outside stays old")?;

    let linked_dir = controlled.path().join("linked-dir");
    symlink(outside.path(), &linked_dir)?;
    let linked_target = controlled.path().join("linked-target.json");
    symlink(&outside_file, &linked_target)?;

    let publisher = DurableFilePublisher::new(controlled.path());
    let attempts = [
        linked_dir.join("escaped.json"),
        linked_target,
        PathBuf::from("../escaped.json"),
        outside_file.clone(),
    ];
    for target in attempts {
        assert!(
            publisher
                .atomic_replace(
                    &target,
                    b"new bytes must not escape",
                    ModePolicy::PreserveExistingOr(DEFINITION_FILE_MODE),
                )
                .is_err(),
            "publisher accepted unsafe target {}",
            target.display()
        );
    }

    assert_eq!(fs::read(&outside_file)?, b"outside stays old");
    assert_eq!(
        names(outside.path())?,
        vec!["outside.json"],
        "a refused path left a temp outside the controlled root"
    );
    Ok(())
}

#[test]
fn concurrent_create_has_one_typed_winner_and_replace_is_whole() -> Result<(), Box<dyn Error>> {
    let bench = TempDir::new()?;
    let target = bench.path().join("one-claim.md");
    let publisher = Arc::new(DurableFilePublisher::new(bench.path()));
    let barrier = Arc::new(Barrier::new(2));
    let first = spawn_claim(
        Arc::clone(&publisher),
        Arc::clone(&barrier),
        target.clone(),
        b"first complete claim\n",
    );
    let second = spawn_claim(
        Arc::clone(&publisher),
        Arc::clone(&barrier),
        target.clone(),
        b"second complete claim with a different tail\n",
    );
    let results = [join(first)?, join(second)?];

    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(PublishError::Conflict { .. })))
            .count(),
        1,
        "the losing claim must get the typed conflict: {results:?}"
    );
    let landed = fs::read(&target)?;
    assert!(
        landed == b"first complete claim\n"
            || landed == b"second complete claim with a different tail\n",
        "the winner is not one complete claimant: {landed:?}"
    );

    publisher.atomic_replace(
        &target,
        b"complete replacement after the claim\n",
        ModePolicy::PreserveExistingOr(DEFINITION_FILE_MODE),
    )?;
    assert_eq!(
        fs::read(&target)?,
        b"complete replacement after the claim\n"
    );
    Ok(())
}

#[test]
fn crash_recovery_keeps_the_committed_target_and_only_removes_owned_temps()
-> Result<(), Box<dyn Error>> {
    let bench = TempDir::new()?;
    let target = bench.path().join("committed.md");
    let foreign = bench.path().join(".loadout-writing-human-note");
    fs::write(&foreign, b"foreign")?;
    let faults = Arc::new(Faults::default());
    faults.arm(
        FaultPoint::AfterCommit,
        PublicationOperation::CreateIfAbsent,
        &target,
        FaultAction::Crash,
    );
    let hook: Arc<dyn FaultInjector> = faults.clone();
    let _scope = scoped_faults(bench.path(), hook)?;
    let publisher = DurableFilePublisher::new(bench.path());

    let result = publisher.atomic_create_if_absent(
        &target,
        b"complete before crash\n",
        ModePolicy::Exact(PRIVATE_FILE_MODE),
    );
    assert!(
        matches!(result, Err(PublishError::Injected { crashed: true, .. })),
        "the simulated crash must cross the real commit point: {result:?}"
    );
    assert_eq!(fs::read(&target)?, b"complete before crash\n");
    let before_recovery = names(bench.path())?;
    assert!(
        before_recovery.len() > 2,
        "the crash left no owned temp for recovery to prove it recognizes"
    );

    DurableFilePublisher::new(bench.path()).recover()?;
    assert_eq!(fs::read(&target)?, b"complete before crash\n");
    assert_eq!(fs::read(&foreign)?, b"foreign");
    assert_eq!(
        names(bench.path())?,
        vec![".loadout-writing-human-note", "committed.md"],
        "recovery did not remove exactly its own temp"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_production_caller_enters_the_same_instrumented_core() -> Result<(), Box<dyn Error>> {
    let bench = TempDir::new()?;
    let faults = Arc::new(Faults::default());
    let hook: Arc<dyn FaultInjector> = faults.clone();
    let _scope = scoped_faults(bench.path(), hook)?;

    let workflows = bench.path().join("workflows");
    fs::create_dir(&workflows)?;
    save(
        &workflow("shared", "Shared core workflow"),
        &workflows.join("shared.json"),
    )?;

    let agents = bench.path().join("agents");
    write_agent_file(&agents, &agent("Shared Core", "Use the shared publisher."))?;

    let run = bench.path().join("run");
    fs::create_dir(&run)?;
    let evidence = EvidenceTarget::workflow_step(
        run.clone(),
        "step-one".to_owned(),
        SafeInputManifest::default(),
    );
    evidence.prepare().await?;

    let body = format!(
        "## Answer\n{}\n\n## Evidence\nrun/file:1\n\n## Open\nNothing.\n",
        "A complete attachment line.\n".repeat(BODY_CAP / 8)
    );
    let handoff = write_handoff(&run, draft(), &body)?;

    let began: Vec<PathBuf> = faults
        .events()
        .into_iter()
        .filter(|event| event.point == FaultPoint::Begin)
        .map(|event| event.target)
        .collect();
    assert!(
        has_component(&began, "workflows"),
        "workflow bypassed the core: {began:?}"
    );
    assert!(
        has_component(&began, "agents"),
        "agent bypassed the core: {began:?}"
    );
    assert!(
        began.iter().any(|path| path == &evidence.input_path()),
        "the supervisor compatibility path left evidence on the old publisher: {began:?}"
    );
    assert!(
        has_component(&began, "handoffs"),
        "handoff bypassed the core: {began:?}"
    );
    assert!(
        has_component(&began, "attachments"),
        "attachment bypassed the core: {began:?}"
    );

    assert_eq!(mode(&evidence.input_path())?, PRIVATE_FILE_MODE);
    assert_eq!(mode(&handoff.path)?, PRIVATE_FILE_MODE);
    let attachment = handoff
        .attachment
        .ok_or("the oversized production handoff did not publish an attachment")?;
    assert_eq!(mode(&attachment)?, PRIVATE_FILE_MODE);
    Ok(())
}

fn workflow(id: &str, name: &str) -> WorkflowFile {
    WorkflowFile {
        format: 1,
        id: id.to_owned(),
        name: name.to_owned(),
        description: Some(format!("{name} with a distinct final field")),
        steps: Vec::new(),
        links: Vec::new(),
        extra: Map::new(),
    }
}

fn agent(summary: &str, instructions: &str) -> Agent {
    Agent {
        summary: summary.to_owned(),
        instructions: instructions.to_owned(),
        ..Agent::example()
    }
}

fn draft() -> MetaDraft {
    MetaDraft {
        run: "run-t202".to_owned(),
        step: 1,
        from: "Shared Core".to_owned(),
        to: vec!["Reader".to_owned()],
        kind: Kind::Findings,
        title: "Durable publication".to_owned(),
        reads: vec!["workflow.json".to_owned()],
    }
}

fn mode(path: &Path) -> Result<u32, Box<dyn Error>> {
    Ok(fs::metadata(path)?.permissions().mode() & 0o777)
}

fn names(dir: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let mut names = fs::read_dir(dir)?
        .map(|entry| entry.map(|item| item.file_name().to_string_lossy().into_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    names.sort();
    Ok(names)
}

fn has_component(paths: &[PathBuf], component: &str) -> bool {
    paths
        .iter()
        .any(|path| path.components().any(|part| part.as_os_str() == component))
}

fn spawn_claim(
    publisher: Arc<DurableFilePublisher>,
    barrier: Arc<Barrier>,
    target: PathBuf,
    bytes: &'static [u8],
) -> thread::JoinHandle<Result<(), PublishError>> {
    thread::spawn(move || {
        barrier.wait();
        publisher.atomic_create_if_absent(&target, bytes, ModePolicy::Exact(PRIVATE_FILE_MODE))
    })
}

fn join(
    handle: thread::JoinHandle<Result<(), PublishError>>,
) -> Result<Result<(), PublishError>, Box<dyn Error>> {
    handle
        .join()
        .map_err(|_| std::io::Error::other("a claim thread panicked").into())
}
