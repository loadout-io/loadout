//! T-210 AC-4: skan i recovery handoffów nie przechodzą przez symlinki poza root biegu.
//!
//! Testy wchodzą wyłącznie przez produkcyjny `scan_run_dir`. Oprócz dwóch ucieczek przez
//! symlink rozróżniają niedostępny attachment, nieczytelny pointer i prawdziwą crash-sierotę,
//! aby naprawa bezpieczeństwa nie mogła po prostu wyłączyć potrzebnego recovery.

use std::error::Error;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use loadout_lib::durable_file::{
    FaultAction, FaultInjector, FaultPoint, PRIVATE_FILE_MODE, PublicationEvent, RecoveryEvent,
    RecoveryPoint, scoped_faults,
};
use loadout_lib::memory::handoff::{
    BODY_CAP, Handoff, Kind, MetaDraft, Written, scan_run_dir, write_handoff,
};

const HEALTHY_BODY: &str =
    "## Answer\nHealthy local handoff.\n\n## Evidence\nsrc/local.rs:1\n\n## Open\nNone.\n";
const OUTSIDE_BODY: &str =
    "## Answer\nOUTSIDE_LEAF_SENTINEL_T210\n\n## Evidence\noutside.rs:1\n\n## Open\nNone.\n";
const OUTSIDE_ATTACHMENT: &[u8] = b"OUTSIDE_ATTACHMENT_SENTINEL_T210";

struct CrashPointerBeforeCommit {
    target: PathBuf,
    fired: AtomicBool,
}

impl FaultInjector for CrashPointerBeforeCommit {
    fn action(&self, event: &PublicationEvent) -> FaultAction {
        if event.target == self.target
            && event.point == FaultPoint::BeforeCommit
            && !self.fired.swap(true, Ordering::SeqCst)
        {
            return FaultAction::Crash;
        }
        FaultAction::Continue
    }
}

struct SwapAttachmentsAfterRootOpen {
    root: PathBuf,
    attachments: PathBuf,
    parked: PathBuf,
    outside: PathBuf,
    swapped: AtomicBool,
}

impl FaultInjector for SwapAttachmentsAfterRootOpen {
    fn action(&self, _event: &PublicationEvent) -> FaultAction {
        FaultAction::Continue
    }

    fn recovery_action(&self, event: &RecoveryEvent) -> FaultAction {
        if event.root == self.root
            && event.point == RecoveryPoint::AfterRootOpened
            && !self.swapped.swap(true, Ordering::SeqCst)
            && (fs::rename(&self.attachments, &self.parked).is_err()
                || symlink(&self.outside, &self.attachments).is_err())
        {
            return FaultAction::Fail;
        }
        FaultAction::Continue
    }
}

#[test]
fn a_handoff_leaf_symlink_is_not_read_or_removed_and_local_history_survives()
-> Result<(), Box<dyn Error>> {
    let run = tempfile::tempdir()?;
    let healthy = write_handoff(run.path(), draft(1, "Local"), HEALTHY_BODY)?;
    let outside = tempfile::tempdir()?;
    let outside_handoff = write_handoff(outside.path(), draft(90, "Outside"), OUTSIDE_BODY)?;
    let outside_bytes = fs::read(&outside_handoff.path)?;
    let outside_mode = mode(&outside_handoff.path)?;
    let linked = run.path().join("handoffs/90__linked__findings.md");
    symlink(&outside_handoff.path, &linked)?;

    let visible = scan_run_dir(run.path())?;

    assert_only_healthy(&visible, &healthy);
    assert!(
        visible.iter().all(|handoff| handoff.body != OUTSIDE_BODY),
        "scan_run_dir read private Markdown through a handoff leaf symlink"
    );
    assert_eq!(
        fs::read(&outside_handoff.path)?,
        outside_bytes,
        "handoff recovery changed the outside leaf target"
    );
    assert_eq!(mode(&outside_handoff.path)?, outside_mode);
    assert!(
        fs::symlink_metadata(&linked)?.file_type().is_symlink(),
        "handoff recovery deleted the malicious leaf instead of safely ignoring it"
    );
    Ok(())
}

#[test]
fn an_attachments_directory_symlink_is_not_walked_or_cleaned() -> Result<(), Box<dyn Error>> {
    let run = tempfile::tempdir()?;
    let healthy = write_handoff(run.path(), draft(1, "Local"), HEALTHY_BODY)?;
    let outside = tempfile::tempdir()?;
    let victim = outside.path().join("victim__full.md");
    fs::write(&victim, OUTSIDE_ATTACHMENT)?;
    fs::set_permissions(&victim, fs::Permissions::from_mode(PRIVATE_FILE_MODE))?;
    let attachments_link = run.path().join("attachments");
    fs::create_dir_all(&attachments_link)?;
    let swap: Arc<dyn FaultInjector> = Arc::new(SwapAttachmentsAfterRootOpen {
        root: run.path().to_owned(),
        attachments: attachments_link.clone(),
        parked: run.path().join("attachments-held-by-recovery"),
        outside: outside.path().to_owned(),
        swapped: AtomicBool::new(false),
    });
    let _scope = scoped_faults(run.path(), swap)?;

    let visible = scan_run_dir(run.path())?;

    assert_only_healthy(&visible, &healthy);
    assert!(
        victim.exists(),
        "recovery followed attachments/ and deleted an outside victim"
    );
    assert_eq!(
        fs::read(&victim)?,
        OUTSIDE_ATTACHMENT,
        "recovery followed attachments/ and deleted or changed an outside victim"
    );
    assert_eq!(mode(&victim)?, PRIVATE_FILE_MODE);
    assert!(
        fs::symlink_metadata(&attachments_link)?
            .file_type()
            .is_symlink(),
        "recovery replaced or removed the linked attachments directory"
    );
    Ok(())
}

#[test]
fn a_pointer_with_a_missing_attachment_is_not_exposed_as_usable() -> Result<(), Box<dyn Error>> {
    let run = tempfile::tempdir()?;
    let healthy = write_handoff(run.path(), draft(1, "Local"), HEALTHY_BODY)?;
    let incomplete = write_handoff(run.path(), draft(2, "Missing"), &large_body("missing"))?;
    let missing = incomplete
        .attachment
        .as_deref()
        .ok_or("large handoff did not publish an attachment")?;
    fs::remove_file(missing)?;

    let visible = scan_run_dir(run.path())?;

    assert_healthy_is_visible(&visible, &healthy);
    assert_not_usable(&visible, &incomplete);
    assert!(!missing.exists());
    Ok(())
}

#[test]
fn a_pointer_with_a_symlink_attachment_is_not_exposed_as_usable() -> Result<(), Box<dyn Error>> {
    let run = tempfile::tempdir()?;
    let healthy = write_handoff(run.path(), draft(1, "Local"), HEALTHY_BODY)?;
    let linked = write_handoff(run.path(), draft(2, "Linked"), &large_body("linked"))?;
    let attachment = linked
        .attachment
        .as_deref()
        .ok_or("large handoff did not publish an attachment")?;
    fs::remove_file(attachment)?;
    let outside = tempfile::tempdir()?;
    let victim = outside.path().join("linked-private.md");
    fs::write(&victim, OUTSIDE_ATTACHMENT)?;
    fs::set_permissions(&victim, fs::Permissions::from_mode(PRIVATE_FILE_MODE))?;
    symlink(&victim, attachment)?;

    let visible = scan_run_dir(run.path())?;

    assert_healthy_is_visible(&visible, &healthy);
    assert_not_usable(&visible, &linked);
    assert_eq!(
        fs::read(&victim)?,
        OUTSIDE_ATTACHMENT,
        "scan_run_dir followed or changed a symlink attachment outside the run"
    );
    assert!(
        fs::symlink_metadata(attachment)?.file_type().is_symlink(),
        "recovery removed the symlink attachment instead of treating it as unusable"
    );
    Ok(())
}

#[test]
fn an_unreadable_existing_pointer_preserves_its_attachment_but_a_true_orphan_is_removed()
-> Result<(), Box<dyn Error>> {
    let run = tempfile::tempdir()?;
    let healthy = write_handoff(run.path(), draft(1, "Local"), HEALTHY_BODY)?;
    let handoffs = run.path().join("handoffs");
    let attachments = run.path().join("attachments");
    fs::create_dir_all(&attachments)?;

    let unreadable_pointer = handoffs.join("70__broken__findings.md");
    fs::write(&unreadable_pointer, b"not valid handoff front matter")?;
    fs::set_permissions(
        &unreadable_pointer,
        fs::Permissions::from_mode(PRIVATE_FILE_MODE),
    )?;
    let guarded_attachment = attachments.join("70__broken__findings__full.md");
    let guarded_bytes = b"FULL_ATTACHMENT_WITH_EXISTING_POINTER_T210";
    fs::write(&guarded_attachment, guarded_bytes)?;
    fs::set_permissions(
        &guarded_attachment,
        fs::Permissions::from_mode(PRIVATE_FILE_MODE),
    )?;

    let orphan = attachments.join("71__crashed__findings__full.md");
    let pointer = handoffs.join("71__crashed__findings.md");
    let faults: Arc<dyn FaultInjector> = Arc::new(CrashPointerBeforeCommit {
        target: pointer,
        fired: AtomicBool::new(false),
    });
    let _faults = scoped_faults(run.path(), faults)?;
    let crashed = write_handoff(run.path(), draft(71, "Crashed"), &large_body("crashed"));
    assert!(
        crashed.is_err(),
        "the controlled publication unexpectedly committed its pointer"
    );
    assert!(
        orphan.exists(),
        "the production crash did not leave its full attachment for recovery"
    );

    let visible = scan_run_dir(run.path())?;

    assert_only_healthy(&visible, &healthy);
    assert!(
        guarded_attachment.exists(),
        "recovery deleted a full attachment whose pointer exists but is unreadable"
    );
    assert_eq!(
        fs::read(&guarded_attachment)?,
        guarded_bytes,
        "recovery deleted a full attachment whose pointer exists but is unreadable"
    );
    assert_eq!(mode(&guarded_attachment)?, PRIVATE_FILE_MODE);
    assert!(
        !orphan.exists(),
        "hardening disabled cleanup of a genuine attachment without any pointer"
    );
    Ok(())
}

fn draft(step: u32, from: &str) -> MetaDraft {
    MetaDraft {
        run: "run-t210-security".to_owned(),
        step,
        from: from.to_owned(),
        to: vec!["Reader".to_owned()],
        kind: Kind::Findings,
        title: "Security boundary".to_owned(),
        reads: vec!["src/local.rs".to_owned()],
    }
}

fn large_body(label: &str) -> String {
    let repeated = format!("{label}-").repeat(BODY_CAP / (label.len() + 1) + 64);
    format!("## Answer\n{repeated}\n\n## Evidence\n{label}.rs:1\n\n## Open\nNone.\n")
}

fn assert_only_healthy(visible: &[Handoff], healthy: &Written) {
    assert_eq!(
        visible.len(),
        1,
        "a malicious or unreadable entry hid or joined the healthy local history: {visible:?}"
    );
    assert_healthy_is_visible(visible, healthy);
}

fn assert_healthy_is_visible(visible: &[Handoff], healthy: &Written) {
    assert!(
        visible
            .iter()
            .any(|handoff| handoff.path == healthy.path && handoff.body == HEALTHY_BODY),
        "the malicious neighbor hid the healthy local handoff: {visible:?}"
    );
}

fn assert_not_usable(visible: &[Handoff], written: &Written) {
    let exposed = visible
        .iter()
        .find(|handoff| handoff.path == written.path)
        .and_then(Handoff::attachment);
    assert!(
        exposed.is_none(),
        "scan_run_dir exposed an unavailable attachment as usable: {exposed:?}"
    );
}

fn mode(path: &Path) -> Result<u32, Box<dyn Error>> {
    Ok(fs::metadata(path)?.permissions().mode() & 0o777)
}
