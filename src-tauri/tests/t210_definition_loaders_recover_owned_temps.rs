//! T-210 AC-2: prawdziwe listowanie biblioteki sprząta wyłącznie porzucone tempy publishera.
//!
//! Fault injector wybiera punkt crashu i kontroluje interleaving, ale zapis oraz późniejsze
//! recovery przechodzą wyłącznie przez produkcyjne komendy workflowów i agentów. Test nie woła
//! `DurableFilePublisher::recover` ani nie implementuje własnego sposobu publikacji.

use std::error::Error;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use loadout_lib::commands::agents::{list_agents_inner, save_agent_inner};
use loadout_lib::commands::workflows::{Saved, list_workflows_inner, save_workflow_inner};
use loadout_lib::durable_file::{
    FaultAction, FaultInjector, FaultPoint, PublicationEvent, RecoveryEvent, RecoveryPoint,
    scoped_faults,
};
use loadout_lib::library::agents::{Agent, read_agent_directory};
use loadout_lib::workflow::WorkflowFile;
use serde_json::Map;
use tempfile::TempDir;

const SAFETY_TIMEOUT: Duration = Duration::from_secs(5);
const LOADER_BLOCK_PROBE: Duration = Duration::from_secs(1);
const FOREIGN_NAME: &str = ".loadout-writing-human-note.tmp";
const FOREIGN_BYTES: &[u8] = b"a human-owned file must survive recovery\n";

#[derive(Clone)]
struct CrashRule {
    target: PathBuf,
    point: FaultPoint,
}

#[derive(Default)]
struct CrashFault {
    rule: Mutex<Option<CrashRule>>,
}

impl CrashFault {
    fn arm(&self, target: &Path, point: FaultPoint) {
        *lock(&self.rule) = Some(CrashRule {
            target: target.to_owned(),
            point,
        });
    }
}

impl FaultInjector for CrashFault {
    fn action(&self, event: &PublicationEvent) -> FaultAction {
        let mut rule = lock(&self.rule);
        if rule
            .as_ref()
            .is_some_and(|rule| rule.target == event.target && rule.point == event.point)
        {
            rule.take();
            return FaultAction::Crash;
        }
        FaultAction::Continue
    }
}

struct PausedSave {
    target: PathBuf,
    paused: SyncSender<()>,
    release: Mutex<Receiver<()>>,
    recovery_entered: SyncSender<()>,
    paused_once: AtomicBool,
    recovery_once: AtomicBool,
}

impl FaultInjector for PausedSave {
    fn action(&self, event: &PublicationEvent) -> FaultAction {
        if event.target == self.target
            && event.point == FaultPoint::AfterTempCreated
            && !self.paused_once.swap(true, Ordering::SeqCst)
            && (self.paused.send(()).is_err()
                || lock(&self.release).recv_timeout(SAFETY_TIMEOUT).is_err())
        {
            return FaultAction::Fail;
        }
        FaultAction::Continue
    }

    fn recovery_action(&self, event: &RecoveryEvent) -> FaultAction {
        if event.point == RecoveryPoint::BeforeLock
            && self
                .target
                .parent()
                .is_some_and(|root| root == event.root.as_path())
            && !self.recovery_once.swap(true, Ordering::SeqCst)
            && self.recovery_entered.send(()).is_err()
        {
            return FaultAction::Fail;
        }
        FaultAction::Continue
    }
}

struct SwapDefinitionRoot {
    root: PathBuf,
    parked: PathBuf,
    outside: PathBuf,
    swapped: AtomicBool,
}

impl FaultInjector for SwapDefinitionRoot {
    fn action(&self, _event: &PublicationEvent) -> FaultAction {
        FaultAction::Continue
    }

    fn recovery_action(&self, event: &RecoveryEvent) -> FaultAction {
        if event.root == self.root
            && event.point == RecoveryPoint::AfterRootOpened
            && !self.swapped.swap(true, Ordering::SeqCst)
            && (fs::rename(&self.root, &self.parked).is_err()
                || symlink(&self.outside, &self.root).is_err())
        {
            return FaultAction::Fail;
        }
        FaultAction::Continue
    }
}

#[test]
fn production_definition_lists_recover_crash_temps_without_racing_a_save()
-> Result<(), Box<dyn Error>> {
    workflow_list_recovers_a_crash_temp()?;
    agent_list_recovers_a_crash_temp()?;
    workflow_list_waits_for_an_active_save()?;
    workflow_list_refuses_a_linked_definition_root()?;
    workflow_recovery_does_not_follow_a_swapped_root()?;
    linked_agent_instructions_are_not_a_library_entry()?;
    Ok(())
}

fn linked_agent_instructions_are_not_a_library_entry() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let agents = home.path().join("agents");
    fs::create_dir(&agents)?;
    let outside = TempDir::new()?;
    // `None`: ten katalog jest świeży, więc plik ma tu POWSTAĆ, a nie kogokolwiek nadpisać.
    let outside_target = save_agent_inner(
        outside.path(),
        agent(
            "Outside agent must not enter Start",
            "These instructions are outside the controlled agent library.\n",
        ),
        None,
    )?
    .path;
    symlink(&outside_target, agents.join("linked.md"))?;

    assert!(
        read_agent_directory(&agents)?.is_empty(),
        "the shared Start/list loader followed an agent symlink"
    );
    assert!(
        list_agents_inner(home.path())?.is_empty(),
        "the screen and Start disagreed about the linked agent"
    );
    Ok(())
}

fn workflow_recovery_does_not_follow_a_swapped_root() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let local = workflow("local-root", "Local workflow before root swap");
    let local_target = save_workflow_inner(home.path(), "local.json", &local, None)?.path;
    let definitions = parent_of(&local_target)?.to_owned();
    let parked = home.path().join("workflows-held-by-recovery");

    let outside = TempDir::new()?;
    let outside_old = workflow("outside-sentinel", "Outside sentinel workflow");
    let outside_new = workflow("outside-sentinel", "Outside replacement workflow");
    let Saved {
        path: outside_target,
        revision: outside_revision,
    } = save_workflow_inner(outside.path(), "sentinel.json", &outside_old, None)?;
    let outside_definitions = parent_of(&outside_target)?;
    let outside_bytes = fs::read(&outside_target)?;
    let crash = Arc::new(CrashFault::default());
    let crash_hook: Arc<dyn FaultInjector> = crash.clone();
    let crash_scope = scoped_faults(outside.path(), crash_hook)?;
    crash.arm(&outside_target, FaultPoint::BeforeCommit);
    // Rewizja z pierwszego zapisu: ten zapis NAPRAWDĘ nadpisuje istniejący plik, więc musi
    // powiedzieć, co czytał — inaczej odmawia z powodu innego niż wstrzyknięty crash.
    assert!(
        save_workflow_inner(
            outside.path(),
            "sentinel.json",
            &outside_new,
            Some(&outside_revision)
        )
        .is_err()
    );
    assert_eq!(owned_temps(outside_definitions)?.len(), 1);
    drop(crash_scope);

    let swap: Arc<dyn FaultInjector> = Arc::new(SwapDefinitionRoot {
        root: definitions.clone(),
        parked,
        outside: outside_definitions.to_owned(),
        swapped: AtomicBool::new(false),
    });
    let _swap_scope = scoped_faults(home.path(), swap)?;

    let listed = list_workflows_inner(home.path());

    assert!(
        listed.is_err(),
        "listing trusted the definition-root name after recovery had opened a different directory"
    );
    assert_eq!(fs::read(&outside_target)?, outside_bytes);
    assert_eq!(
        owned_temps(outside_definitions)?.len(),
        1,
        "path-recursive recovery followed the swapped root and removed an outside temp"
    );
    Ok(())
}

fn workflow_list_refuses_a_linked_definition_root() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let outside = TempDir::new()?;
    let old = workflow("outside-recovery", "Outside complete workflow");
    let new = workflow("outside-recovery", "Outside replacement workflow");
    let Saved {
        path: target,
        revision,
    } = save_workflow_inner(outside.path(), "outside.json", &old, None)?;
    let old_bytes = fs::read(&target)?;
    let definitions = parent_of(&target)?;

    let faults = Arc::new(CrashFault::default());
    let hook: Arc<dyn FaultInjector> = faults.clone();
    let scope = scoped_faults(outside.path(), hook)?;
    faults.arm(&target, FaultPoint::BeforeCommit);
    assert!(save_workflow_inner(outside.path(), "outside.json", &new, Some(&revision)).is_err());
    assert_eq!(owned_temps(definitions)?.len(), 1);
    drop(scope);

    symlink(definitions, home.path().join("workflows"))?;
    let listed = list_workflows_inner(home.path());

    assert!(
        listed.is_err(),
        "the production loader followed a linked workflow root outside the library"
    );
    assert_eq!(
        owned_temps(definitions)?.len(),
        1,
        "recovery followed the linked root and deleted an outside crash temp"
    );
    assert_eq!(fs::read(&target)?, old_bytes);
    Ok(())
}

fn workflow_list_recovers_a_crash_temp() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let old = workflow("workflow-recovery", "Old complete workflow");
    let new = workflow("workflow-recovery", "New complete workflow");
    let Saved {
        path: target,
        revision,
    } = save_workflow_inner(home.path(), "recovery.json", &old, None)?;
    let definitions = parent_of(&target)?;
    let foreign = definitions.join(FOREIGN_NAME);
    fs::write(&foreign, FOREIGN_BYTES)?;

    let faults = Arc::new(CrashFault::default());
    let hook: Arc<dyn FaultInjector> = faults.clone();
    let _scope = scoped_faults(home.path(), hook)?;
    faults.arm(&target, FaultPoint::BeforeCommit);

    let crashed = save_workflow_inner(home.path(), "recovery.json", &new, Some(&revision));
    assert!(
        crashed.is_err(),
        "the controlled workflow crash unexpectedly reported a successful save"
    );
    assert_eq!(
        owned_temps(definitions)?.len(),
        1,
        "the workflow crash did not leave exactly one real publisher temp"
    );

    let first = list_workflows_inner(home.path())?;
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].path, "recovery.json");
    assert_eq!(first[0].workflow, old);
    assert!(
        owned_temps(definitions)?.is_empty(),
        "production workflow listing left its crash temp behind"
    );
    assert_eq!(fs::read(&foreign)?, FOREIGN_BYTES);

    let second = list_workflows_inner(home.path())?;
    assert_eq!(
        second, first,
        "reopening the workflow library was not idempotent"
    );
    assert!(owned_temps(definitions)?.is_empty());
    assert_eq!(fs::read(&foreign)?, FOREIGN_BYTES);
    Ok(())
}

fn agent_list_recovers_a_crash_temp() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let old = agent(
        "Old complete agent",
        "Keep the old complete instructions.\n",
    );
    let mut new = old.clone();
    "New complete agent".clone_into(&mut new.summary);
    "Use the new complete instructions.\n".clone_into(&mut new.instructions);
    let written = save_agent_inner(home.path(), &old, None)?;
    let target = written.path;
    let definitions = parent_of(&target)?;
    let foreign = definitions.join(FOREIGN_NAME);
    fs::write(&foreign, FOREIGN_BYTES)?;

    let faults = Arc::new(CrashFault::default());
    let hook: Arc<dyn FaultInjector> = faults.clone();
    let _scope = scoped_faults(home.path(), hook)?;
    faults.arm(&target, FaultPoint::BeforeCommit);

    let crashed = save_agent_inner(home.path(), &new, Some(&written.revision));
    assert!(
        crashed.is_err(),
        "the controlled agent crash unexpectedly reported a successful save"
    );
    assert_eq!(
        owned_temps(definitions)?.len(),
        1,
        "the agent crash did not leave exactly one real publisher temp"
    );

    let first = list_agents_inner(home.path())?;
    assert_eq!(first, vec![old]);
    assert!(
        owned_temps(definitions)?.is_empty(),
        "production agent listing left its crash temp behind"
    );
    assert_eq!(fs::read(&foreign)?, FOREIGN_BYTES);

    let second = list_agents_inner(home.path())?;
    assert_eq!(
        second, first,
        "reopening the agent library was not idempotent"
    );
    assert!(owned_temps(definitions)?.is_empty());
    assert_eq!(fs::read(&foreign)?, FOREIGN_BYTES);
    Ok(())
}

fn workflow_list_waits_for_an_active_save() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let old = workflow("active-save", "Old workflow before the active save");
    let new = workflow("active-save", "Complete workflow after the active save");
    let Saved {
        path: target,
        revision,
    } = save_workflow_inner(home.path(), "active.json", &old, None)?;
    let definitions = parent_of(&target)?;

    let (paused_tx, paused_rx) = mpsc::sync_channel(0);
    let (release_tx, release_rx) = mpsc::sync_channel(0);
    let (recovery_entered_tx, recovery_entered_rx) = mpsc::sync_channel(0);
    let controls = Arc::new(PausedSave {
        target: target.clone(),
        paused: paused_tx,
        release: Mutex::new(release_rx),
        recovery_entered: recovery_entered_tx,
        paused_once: AtomicBool::new(false),
        recovery_once: AtomicBool::new(false),
    });
    let hook: Arc<dyn FaultInjector> = controls;
    let _scope = scoped_faults(home.path(), hook)?;

    let (writer_result_tx, writer_result_rx) = mpsc::channel();
    let writer_home = home.path().to_owned();
    let writer = thread::spawn(move || {
        let result = save_workflow_inner(&writer_home, "active.json", &new, Some(&revision));
        let _sent = writer_result_tx.send(result);
    });
    paused_rx.recv_timeout(SAFETY_TIMEOUT).map_err(|error| {
        std::io::Error::other(format!(
            "the active save did not reach AfterTempCreated: {error}"
        ))
    })?;
    assert_eq!(
        owned_temps(definitions)?.len(),
        1,
        "the interleaving did not pause a real save with its temp still present"
    );

    let (loader_result_tx, loader_result_rx) = mpsc::channel();
    let loader_home = home.path().to_owned();
    let loader = thread::spawn(move || {
        let result = list_workflows_inner(&loader_home);
        let _sent = loader_result_tx.send(result);
    });
    recovery_entered_rx
        .recv_timeout(SAFETY_TIMEOUT)
        .map_err(|error| {
            std::io::Error::other(format!(
                "the production loader did not enter recovery before its lock: {error}"
            ))
        })?;

    // 2026-08-28: event pochodzi już z produkcyjnego recovery, więc brak wyniku dowodzi
    // oczekiwania na aktywnego writera, nie tego, że scheduler jeszcze nie uruchomił wątku.
    // Timeout jest wyłącznie bounded sondą przed zwolnieniem kontrolowanej publikacji.
    let early = loader_result_rx.recv_timeout(LOADER_BLOCK_PROBE);
    let loaded_before_release = early.is_ok();
    let release_result = release_tx.send(());

    let writer_result = receive_thread_result("writer", &writer_result_rx, writer)?;
    let loader_result = settle_loader(early, &loader_result_rx, loader)?;
    assert!(
        release_result.is_ok(),
        "the active writer stopped waiting before the test released it"
    );
    assert!(
        writer_result.is_ok(),
        "the controlled active save did not publish successfully: {writer_result:?}"
    );
    assert!(
        !loaded_before_release,
        "workflow listing returned while a save under the same root still owned its temp"
    );

    let listed = loader_result?;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].path, "active.json");
    assert_eq!(
        listed[0].workflow.name,
        "Complete workflow after the active save"
    );
    assert!(
        owned_temps(definitions)?.is_empty(),
        "active save/list interleaving left a publisher temp"
    );
    Ok(())
}

/// Wynik loadera — ten, który zdążył przed zwolnieniem publikacji, albo ten, na który czekamy po
/// niej — z wątkiem dołączonym dokładnie raz, którąkolwiek z trzech dróg test tu wszedł.
fn settle_loader<T, E>(
    early: Result<Result<T, E>, RecvTimeoutError>,
    receiver: &Receiver<Result<T, E>>,
    handle: thread::JoinHandle<()>,
) -> Result<Result<T, E>, Box<dyn Error>> {
    match early {
        Ok(result) => {
            handle
                .join()
                .map_err(|_| std::io::Error::other("the loader thread panicked"))?;
            Ok(result)
        }
        Err(RecvTimeoutError::Timeout) => receive_thread_result("loader", receiver, handle),
        Err(RecvTimeoutError::Disconnected) => {
            handle
                .join()
                .map_err(|_| std::io::Error::other("the loader thread panicked"))?;
            Err(std::io::Error::other("the loader returned no result").into())
        }
    }
}

fn receive_thread_result<T, E>(
    name: &str,
    receiver: &Receiver<Result<T, E>>,
    handle: thread::JoinHandle<()>,
) -> Result<Result<T, E>, Box<dyn Error>> {
    let result = receiver.recv_timeout(SAFETY_TIMEOUT).map_err(|error| {
        std::io::Error::other(format!(
            "the {name} thread did not finish before the safety timeout: {error}"
        ))
    })?;
    handle
        .join()
        .map_err(|_| std::io::Error::other(format!("the {name} thread panicked")))?;
    Ok(result)
}

fn workflow(id: &str, name: &str) -> WorkflowFile {
    WorkflowFile {
        format: 1,
        id: id.to_owned(),
        name: name.to_owned(),
        description: Some(format!("{name}; every final byte is significant")),
        steps: Vec::new(),
        links: Vec::new(),
        extra: Map::new(),
    }
}

fn agent(summary: &str, instructions: &str) -> Agent {
    Agent {
        name: "Recovery Agent".to_owned(),
        summary: summary.to_owned(),
        instructions: instructions.to_owned(),
        ..Agent::example()
    }
}

fn parent_of(path: &Path) -> Result<&Path, Box<dyn Error>> {
    path.parent()
        .ok_or_else(|| std::io::Error::other(format!("{} has no parent", path.display())).into())
}

fn owned_temps(root: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut temps = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = name
            .strip_prefix(".loadout-writing-")
            .and_then(|rest| rest.strip_suffix(".tmp"))
        else {
            continue;
        };
        if uuid::Uuid::parse_str(id).is_ok() {
            temps.push(entry.path());
        }
    }
    temps.sort();
    Ok(temps)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
