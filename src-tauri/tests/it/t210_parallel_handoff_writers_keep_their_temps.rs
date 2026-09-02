//! T-210 AC-1: recovery jednego writera nie usuwa aktywnego tempu drugiego.
//!
//! Obaj writerzy wchodzą przez produkcyjne `write_handoff`. Fault seam zatrzymuje A po
//! utworzeniu jego tempu i potwierdza, że B przeszedł własny start publikacji, zanim A zostanie
//! zwolniony. Na implementacji T-202 `Begin` writera B następuje już po jego `recover()`, więc
//! ten interleaving deterministycznie wystawia usunięcie aktywnego tempu A.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use loadout_lib::durable_file::{
    FaultAction, FaultInjector, FaultPoint, PublicationEvent, scoped_faults,
};
use loadout_lib::memory::Error as MemoryError;
use loadout_lib::memory::handoff::{
    Handoff, Kind, MetaDraft, Written, scan_run_dir, write_handoff,
};
use tempfile::TempDir;

const SAFETY_TIMEOUT: Duration = Duration::from_secs(5);
const RECOVERY_PROBE: Duration = Duration::from_millis(500);
const BODY_A: &str =
    "## Answer\nWriter A kept every byte.\n\n## Evidence\na.rs:1\n\n## Open\nNothing.\n";
const BODY_B: &str =
    "## Answer\nWriter B kept every byte.\n\n## Evidence\nb.rs:2\n\n## Open\nNothing.\n";

struct Interleaving {
    writer_a_target: PathBuf,
    writer_b_target: PathBuf,
    writer_a_paused: SyncSender<()>,
    release_writer_a: Mutex<Receiver<()>>,
    writer_b_began: SyncSender<()>,
    paused_a_once: AtomicBool,
    observed_b_once: AtomicBool,
}

impl FaultInjector for Interleaving {
    fn action(&self, event: &PublicationEvent) -> FaultAction {
        if event.target == self.writer_a_target
            && event.point == FaultPoint::AfterTempCreated
            && !self.paused_a_once.swap(true, Ordering::SeqCst)
            && (self.writer_a_paused.send(()).is_err()
                || lock(&self.release_writer_a)
                    .recv_timeout(SAFETY_TIMEOUT)
                    .is_err())
        {
            return FaultAction::Fail;
        }

        if event.target == self.writer_b_target
            && event.point == FaultPoint::Begin
            && !self.observed_b_once.swap(true, Ordering::SeqCst)
            && self.writer_b_began.send(()).is_err()
        {
            return FaultAction::Fail;
        }

        FaultAction::Continue
    }
}

#[test]
fn parallel_handoff_writers_keep_both_complete_publications() -> Result<(), Box<dyn Error>> {
    let run = TempDir::new()?;
    let writer_a_target = run.path().join("handoffs/01__writer-a__findings.md");
    let writer_b_target = run.path().join("handoffs/02__writer-b__findings.md");
    let (writer_a_paused_tx, writer_a_paused_rx) = mpsc::sync_channel(0);
    let (release_writer_a_tx, release_writer_a_rx) = mpsc::sync_channel(0);
    let (writer_b_began_tx, writer_b_began_rx) = mpsc::sync_channel(0);

    let controls = Arc::new(Interleaving {
        writer_a_target: writer_a_target.clone(),
        writer_b_target: writer_b_target.clone(),
        writer_a_paused: writer_a_paused_tx,
        release_writer_a: Mutex::new(release_writer_a_rx),
        writer_b_began: writer_b_began_tx,
        paused_a_once: AtomicBool::new(false),
        observed_b_once: AtomicBool::new(false),
    });
    let hook: Arc<dyn FaultInjector> = controls.clone();
    let _scope = scoped_faults(run.path(), hook)?;

    let (writer_a_result_tx, writer_a_result_rx) = mpsc::channel();
    let first_run_root = run.path().to_owned();
    let writer_a = thread::spawn(move || {
        let result = write_handoff(&first_run_root, draft(1, "Writer A"), BODY_A);
        let _sent = writer_a_result_tx.send(result);
    });

    writer_a_paused_rx
        .recv_timeout(SAFETY_TIMEOUT)
        .map_err(|error| {
            std::io::Error::other(format!(
                "writer A did not reach AfterTempCreated before the safety timeout: {error}"
            ))
        })?;

    let (writer_b_result_tx, writer_b_result_rx) = mpsc::channel();
    let concurrent_run_root = run.path().to_owned();
    let writer_b = thread::spawn(move || {
        let result = write_handoff(&concurrent_run_root, draft(2, "Writer B"), BODY_B);
        let _sent = writer_b_result_tx.send(result);
    });

    // 2026-08-28: `Begin` jest pierwszym eventem publishera i następuje po recovery w
    // `write_handoff`. Stary kod dociera tu przed zwolnieniem A i usuwa jego temp; poprawny
    // wspólny guard może legalnie zatrzymać B jeszcze w recovery. Krótki probe rozróżnia te
    // interleavingi, ale nie jest asercją samą w sobie — kontraktem są oba kompletne wyniki.
    let writer_b_before_release = writer_b_began_rx.recv_timeout(RECOVERY_PROBE);
    let release_result = release_writer_a_tx.send(());
    let writer_b_began = match writer_b_before_release {
        Ok(()) => Ok(()),
        Err(RecvTimeoutError::Timeout) => writer_b_began_rx.recv_timeout(SAFETY_TIMEOUT),
        Err(error @ RecvTimeoutError::Disconnected) => Err(error),
    };
    assert!(
        writer_b_began.is_ok(),
        "writer B did not reach its publication start after the controlled interleaving: {writer_b_began:?}"
    );
    assert!(
        release_result.is_ok(),
        "writer A stopped waiting before the controlled interleaving released it"
    );

    let first_result = receive_writer("A", &writer_a_result_rx, writer_a)?;
    let competing_result = receive_writer("B", &writer_b_result_rx, writer_b)?;
    assert!(
        first_result.is_ok() && competing_result.is_ok(),
        "distinct handoff writers must both publish successfully; A={first_result:?}, B={competing_result:?}"
    );

    let written_a = first_result?;
    let written_b = competing_result?;
    assert_eq!(written_a.path, writer_a_target);
    assert_eq!(written_b.path, writer_b_target);

    let visible = scan_run_dir(run.path())?;
    assert_eq!(
        visible.len(),
        2,
        "the production scan must retain both complete handoffs: {visible:?}"
    );
    assert_eq!(
        bodies_by_step(&visible),
        BTreeMap::from([(1, BODY_A.to_owned()), (2, BODY_B.to_owned())]),
        "the two visible handoffs did not keep their writer's exact body"
    );
    assert_no_owned_temps(run.path())?;
    Ok(())
}

fn draft(step: u32, from: &str) -> MetaDraft {
    MetaDraft {
        run: "run-t210".to_owned(),
        step,
        from: from.to_owned(),
        to: vec!["Join".to_owned()],
        kind: Kind::Findings,
        title: format!("Parallel handoff {step}"),
        reads: vec![format!("{step}.rs")],
    }
}

fn receive_writer(
    name: &str,
    receiver: &Receiver<Result<Written, MemoryError>>,
    handle: thread::JoinHandle<()>,
) -> Result<Result<Written, MemoryError>, Box<dyn Error>> {
    let result = receiver.recv_timeout(SAFETY_TIMEOUT).map_err(|error| {
        std::io::Error::other(format!(
            "writer {name} did not finish before the safety timeout: {error}"
        ))
    })?;
    handle
        .join()
        .map_err(|_| std::io::Error::other(format!("writer {name} panicked")))?;
    Ok(result)
}

fn bodies_by_step(handoffs: &[Handoff]) -> BTreeMap<u32, String> {
    handoffs
        .iter()
        .map(|handoff| (handoff.meta.step, handoff.body.clone()))
        .collect()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn assert_no_owned_temps(root: &Path) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            assert_no_owned_temps(&entry.path())?;
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let has_temp_extension = Path::new(&name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("tmp"));
        assert!(
            !(name.starts_with(".loadout-writing-") && has_temp_extension),
            "publisher temp survived under {}: {name}",
            root.display()
        );
    }
    Ok(())
}
