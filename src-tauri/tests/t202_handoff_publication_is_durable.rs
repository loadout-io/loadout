//! T-202 AC-2: attachment jest trwały przed widocznym pointerem, a claim nie nadpisuje.
//!
//! Czytniki są produkcyjne (`scan_run_dir`, `read_handoff`). Fault injector zatrzymuje ten sam
//! publisher, którego używa zapis; nie składa pliku ani pointera po stronie testu.

use std::error::Error;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Barrier, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use loadout_lib::durable_file::{
    FaultAction, FaultInjector, FaultPoint, PRIVATE_FILE_MODE, PublicationEvent, scoped_faults,
};
use loadout_lib::memory::Error as MemoryError;
use loadout_lib::memory::handoff::{
    BODY_CAP, Handoff, Kind, MetaDraft, Written, read_handoff, scan_run_dir, write_handoff,
};
use tempfile::TempDir;

const FULL_BODY_LINE: &str = "Every byte of the attachment belongs to the durable handoff.\n";

#[derive(Clone)]
struct Rule {
    point: FaultPoint,
    target: PathBuf,
    action: FaultAction,
}

#[derive(Default)]
struct Faults {
    rule: Mutex<Option<Rule>>,
    events: Mutex<Vec<PublicationEvent>>,
}

impl Faults {
    fn arm(&self, point: FaultPoint, target: &Path, action: FaultAction) {
        *lock(&self.rule) = Some(Rule {
            point,
            target: target.to_owned(),
            action,
        });
    }
}

impl FaultInjector for Faults {
    fn action(&self, event: &PublicationEvent) -> FaultAction {
        lock(&self.events).push(event.clone());
        let mut rule = lock(&self.rule);
        if rule
            .as_ref()
            .is_some_and(|armed| armed.point == event.point && armed.target == event.target)
        {
            return rule
                .take()
                .map_or(FaultAction::Continue, |armed| armed.action);
        }
        FaultAction::Continue
    }
}

struct PauseAfterAttachmentCommit {
    target: PathBuf,
    entered: Barrier,
    release: Barrier,
}

impl FaultInjector for PauseAfterAttachmentCommit {
    fn action(&self, event: &PublicationEvent) -> FaultAction {
        if event.target == self.target && event.point == FaultPoint::AfterCommit {
            self.entered.wait();
            self.release.wait();
        }
        FaultAction::Continue
    }
}

struct MeetBeforePointerCommit {
    target: PathBuf,
    writers: Barrier,
}

impl FaultInjector for MeetBeforePointerCommit {
    fn action(&self, event: &PublicationEvent) -> FaultAction {
        if event.target == self.target && event.point == FaultPoint::BeforeCommit {
            // 2026-08-28: bariera startowa w teście nie dowodzi overlapu w publisherze.
            // Tutaj oba pełne tempy już istnieją, a nazwa końcowa nadal jest wolna, więc
            // oba writery muszą rozstrzygnąć ten sam claim atomowym `linkat`.
            self.writers.wait();
        }
        FaultAction::Continue
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[test]
fn a_small_handoff_is_complete_private_and_claimed_without_an_empty_reservation()
-> Result<(), Box<dyn Error>> {
    let run = TempDir::new()?;
    let body = "## Answer\nSmall and complete.\n\n## Evidence\nfile.rs:1\n\n## Open\nNothing.\n";
    let written = write_handoff(run.path(), draft(1, "Writer"), body)?;

    assert!(fs::read_to_string(&written.path)?.ends_with(body));
    assert_eq!(read_handoff(&written.path)?.body, body);
    assert_eq!(mode(&written.path)?, PRIVATE_FILE_MODE);
    assert!(written.attachment.is_none());
    assert_no_transient_artifacts(run.path())?;
    Ok(())
}

#[test]
fn attachment_and_pointer_faults_reopen_as_the_previous_state_or_one_complete_handoff()
-> Result<(), Box<dyn Error>> {
    let cases = [
        FaultCase::attachment(FaultPoint::AfterWrite, false),
        FaultCase::attachment(FaultPoint::AfterCommit, false),
        FaultCase::pointer(FaultPoint::Begin, false),
        FaultCase::pointer(FaultPoint::BeforeCommit, false),
        FaultCase::pointer(FaultPoint::AfterCommit, true),
    ];
    for case in cases {
        handoff_fault_case(case)?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct FaultCase {
    point: FaultPoint,
    target: Target,
    new_handoff_visible: bool,
}

impl FaultCase {
    const fn attachment(point: FaultPoint, new_handoff_visible: bool) -> Self {
        Self {
            point,
            target: Target::Attachment,
            new_handoff_visible,
        }
    }

    const fn pointer(point: FaultPoint, new_handoff_visible: bool) -> Self {
        Self {
            point,
            target: Target::Pointer,
            new_handoff_visible,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Target {
    Attachment,
    Pointer,
}

fn handoff_fault_case(case: FaultCase) -> Result<(), Box<dyn Error>> {
    let run = TempDir::new()?;
    let stable_body =
        "## Answer\nPrevious complete state.\n\n## Evidence\nold.rs:1\n\n## Open\nNone.\n";
    let stable = write_handoff(run.path(), draft(1, "Stable"), stable_body)?;
    let stable_bytes = fs::read(&stable.path)?;

    let faults = Arc::new(Faults::default());
    let hook: Arc<dyn FaultInjector> = faults.clone();
    let _scope = scoped_faults(run.path(), hook)?;
    let pointer = run.path().join("handoffs/02__writer__findings.md");
    let attachment = run.path().join("attachments/02__writer__findings__full.md");
    let fault_target = match case.target {
        Target::Attachment => &attachment,
        Target::Pointer => &pointer,
    };
    faults.arm(case.point, fault_target, FaultAction::Fail);

    let body = large_body();
    let result = write_handoff(run.path(), draft(2, "Writer"), &body);
    assert!(
        result.is_err(),
        "handoff reported success when {case:?} stopped publication"
    );

    let first_reopen = scan_run_dir(run.path())?;
    let second_reopen = scan_run_dir(run.path())?;
    assert_eq!(
        ids(&first_reopen),
        ids(&second_reopen),
        "handoff recovery is not idempotent for {case:?}"
    );
    assert_eq!(fs::read(&stable.path)?, stable_bytes);
    assert_eq!(read_handoff(&stable.path)?.body, stable_body);

    let new = first_reopen.iter().find(|handoff| handoff.path == pointer);
    assert_eq!(
        new.is_some(),
        case.new_handoff_visible,
        "reopen exposed the wrong pointer state for {case:?}: {first_reopen:?}"
    );
    if let Some(handoff) = new {
        assert_complete_attachment(handoff, &body)?;
    } else {
        assert!(
            !attachment.exists(),
            "recovery left an orphan attachment visible without its handoff for {case:?}"
        );
    }
    assert_no_transient_artifacts(run.path())?;
    Ok(())
}

#[test]
fn recovery_waits_until_an_active_attachment_has_its_pointer() -> Result<(), Box<dyn Error>> {
    let run = TempDir::new()?;
    let pointer = run.path().join("handoffs/06__writer__findings.md");
    let attachment = run.path().join("attachments/06__writer__findings__full.md");
    let pause = Arc::new(PauseAfterAttachmentCommit {
        target: attachment.clone(),
        entered: Barrier::new(2),
        release: Barrier::new(2),
    });
    let hook: Arc<dyn FaultInjector> = pause.clone();
    let _scope = scoped_faults(run.path(), hook)?;

    let writer_run = run.path().to_owned();
    let writer =
        thread::spawn(move || write_handoff(&writer_run, draft(6, "Writer"), &large_body()));
    pause.entered.wait();

    let reader_run = run.path().to_owned();
    let (started_tx, started_rx) = mpsc::sync_channel(0);
    let (finished_tx, finished_rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        let _started = started_tx.send(());
        let result = scan_run_dir(&reader_run);
        let _finished = finished_tx.send(());
        result
    });
    started_rx.recv()?;
    let premature = finished_rx.recv_timeout(Duration::from_millis(250));

    pause.release.wait();
    let written = join(writer)??;
    let visible = reader
        .join()
        .map_err(|_| std::io::Error::other("handoff reader panicked"))??;

    assert!(
        matches!(premature, Err(RecvTimeoutError::Timeout)),
        "recovery finished while an attachment had no pointer: {premature:?}"
    );
    assert_eq!(written.path, pointer);
    assert_eq!(written.attachment.as_deref(), Some(attachment.as_path()));
    assert_eq!(
        visible.len(),
        1,
        "reader saw the wrong handoffs: {visible:?}"
    );
    assert_complete_attachment(&visible[0], &large_body())?;
    assert_no_transient_artifacts(run.path())?;
    Ok(())
}

#[test]
fn concurrent_same_name_claims_leave_one_readable_handoff_and_one_conflict()
-> Result<(), Box<dyn Error>> {
    let run = TempDir::new()?;
    let pointer = run
        .path()
        .join("handoffs/05__concurrent-writer__findings.md");
    let hook: Arc<dyn FaultInjector> = Arc::new(MeetBeforePointerCommit {
        target: pointer,
        writers: Barrier::new(2),
    });
    let _scope = scoped_faults(run.path(), hook)?;
    let barrier = Arc::new(Barrier::new(2));
    let first = spawn_handoff(
        run.path().to_owned(),
        Arc::clone(&barrier),
        "## Answer\nFirst complete claimant.\n\n## Evidence\na:1\n\n## Open\nNone.\n",
    );
    let second = spawn_handoff(
        run.path().to_owned(),
        Arc::clone(&barrier),
        "## Answer\nSecond complete claimant with a tail.\n\n## Evidence\nb:2\n\n## Open\nNone.\n",
    );
    let results = [join(first)?, join(second)?];

    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| {
                matches!(result, Err(MemoryError::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists)
            })
            .count(),
        1,
        "the losing handoff needs one explicit conflict: {results:?}"
    );

    let visible = scan_run_dir(run.path())?;
    assert_eq!(visible.len(), 1, "two claims produced {visible:?}");
    assert!(
        visible[0].body.contains("First complete claimant")
            || visible[0]
                .body
                .contains("Second complete claimant with a tail"),
        "the winner contains neither complete body: {:?}",
        visible[0].body
    );
    assert_eq!(mode(&visible[0].path)?, PRIVATE_FILE_MODE);
    assert_no_transient_artifacts(run.path())?;
    Ok(())
}

#[test]
fn handoff_root_and_target_symlinks_never_publish_outside_the_run() -> Result<(), Box<dyn Error>> {
    let controlled = TempDir::new()?;
    let outside = TempDir::new()?;
    let outside_marker = outside.path().join("marker.md");
    fs::write(&outside_marker, b"outside remains unchanged")?;

    let linked_run = controlled.path().join("linked-run");
    symlink(outside.path(), &linked_run)?;
    let through_root = write_handoff(
        &linked_run,
        draft(3, "Escaping Root"),
        "## Answer\nNo escape.\n\n## Evidence\nNone.\n\n## Open\nNone.\n",
    );
    assert!(through_root.is_err(), "a symlink run root was accepted");
    assert_eq!(fs::read(&outside_marker)?, b"outside remains unchanged");
    assert_eq!(names(outside.path())?, vec!["marker.md"]);

    let run = controlled.path().join("real-run");
    let handoffs = run.join("handoffs");
    fs::create_dir_all(&handoffs)?;
    let outside_target = outside.path().join("outside-target.md");
    fs::write(&outside_target, b"outside target remains old")?;
    symlink(&outside_target, handoffs.join("04__writer__findings.md"))?;
    let through_target = write_handoff(
        &run,
        draft(4, "Writer"),
        "## Answer\nNo target escape.\n\n## Evidence\nNone.\n\n## Open\nNone.\n",
    );
    assert!(through_target.is_err(), "a symlink final name was accepted");
    assert_eq!(fs::read(&outside_target)?, b"outside target remains old");
    assert_no_transient_artifacts(&run)?;
    Ok(())
}

fn assert_complete_attachment(handoff: &Handoff, original: &str) -> Result<(), Box<dyn Error>> {
    let attachment = handoff
        .attachment()
        .ok_or("a visible truncated handoff has no production attachment pointer")?;
    assert_eq!(fs::read_to_string(&attachment)?, original);
    assert_eq!(mode(&handoff.path)?, PRIVATE_FILE_MODE);
    assert_eq!(mode(&attachment)?, PRIVATE_FILE_MODE);
    Ok(())
}

fn large_body() -> String {
    format!(
        "## Answer\n{}\n## Evidence\nsrc/file.rs:1\n\n## Open\nNothing.\n",
        FULL_BODY_LINE.repeat(BODY_CAP / FULL_BODY_LINE.len() + 40)
    )
}

fn draft(step: u32, from: &str) -> MetaDraft {
    MetaDraft {
        run: "run-t202".to_owned(),
        step,
        from: from.to_owned(),
        to: vec!["Reader".to_owned()],
        kind: Kind::Findings,
        title: "Durable handoff".to_owned(),
        reads: vec!["src/file.rs".to_owned()],
    }
}

fn spawn_handoff(
    run: PathBuf,
    barrier: Arc<Barrier>,
    body: &'static str,
) -> thread::JoinHandle<Result<Written, MemoryError>> {
    thread::spawn(move || {
        barrier.wait();
        write_handoff(&run, draft(5, "Concurrent Writer"), body)
    })
}

fn join(
    handle: thread::JoinHandle<Result<Written, MemoryError>>,
) -> Result<Result<Written, MemoryError>, Box<dyn Error>> {
    handle
        .join()
        .map_err(|_| std::io::Error::other("a handoff claimant panicked").into())
}

fn ids(handoffs: &[Handoff]) -> Vec<String> {
    handoffs.iter().map(|one| one.meta.id.clone()).collect()
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

fn assert_no_transient_artifacts(root: &Path) -> Result<(), Box<dyn Error>> {
    fn visit(root: &Path, path: &Path) -> Result<(), Box<dyn Error>> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                visit(root, &entry.path())?;
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            assert!(
                !name.contains("writing")
                    && !name.contains("journal")
                    && !Path::new(&name)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("tmp")),
                "transient artifact {name} survived under {}",
                root.display()
            );
        }
        Ok(())
    }
    visit(root, root)
}
