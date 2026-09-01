//! T-210 AC-3: prywatny replace odmawia przed przykryciem słabego celu lub starego guarda.
//!
//! Każda scena wchodzi przez produkcyjny `PrivateFilePublisher`. Bajty terminalnego stanu są
//! rozróżnialne, aby sama odmowa bez zachowania istniejącej prawdy nie mogła zaliczyć testu.

use std::error::Error;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use loadout_lib::durable_file::{
    FaultAction, FaultInjector, FaultPoint, PRIVATE_FILE_MODE, PublicationEvent, scoped_faults,
};
use loadout_lib::engine::supervisor::PrivateFilePublisher;

const TERMINAL_UNHEALTHY: &[u8] = br#"{"complete":false,"healthy":false,"state":"cleanup-needed"}"#;
const TERMINAL_COMPLETE: &[u8] = br#"{"complete":true,"healthy":true,"state":"succeeded"}"#;
const LEGACY_GUARD: &[u8] = b"unfinished terminal publication";
const SWAPPED_UNHEALTHY: &[u8] =
    br#"{"complete":false,"healthy":false,"state":"swapped-after-validation"}"#;

struct SwapTargetAtBegin {
    target: PathBuf,
    original: PathBuf,
    swapped: AtomicBool,
}

impl FaultInjector for SwapTargetAtBegin {
    fn action(&self, event: &PublicationEvent) -> FaultAction {
        if event.target == self.target
            && event.point == FaultPoint::Begin
            && !self.swapped.swap(true, Ordering::SeqCst)
            && (fs::rename(&self.target, &self.original).is_err()
                || fs::write(&self.target, SWAPPED_UNHEALTHY).is_err()
                || fs::set_permissions(&self.target, fs::Permissions::from_mode(0o644)).is_err())
        {
            return FaultAction::Fail;
        }
        FaultAction::Continue
    }
}

#[test]
fn a_mode_0644_private_target_is_refused_without_changing_terminal_truth()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let relative = Path::new("evidence/conversation.json");
    let target = root.path().join(relative);
    fs::create_dir_all(target.parent().ok_or("target has no parent")?)?;
    fs::write(&target, TERMINAL_UNHEALTHY)?;
    fs::set_permissions(&target, fs::Permissions::from_mode(0o644))?;

    let result = private_replace(root.path(), relative, TERMINAL_COMPLETE);

    assert!(
        result.is_err(),
        "a private replace accepted an existing mode-0644 target"
    );
    assert_eq!(
        fs::read(&target)?,
        TERMINAL_UNHEALTHY,
        "refusal replaced the existing incomplete/unhealthy terminal state"
    );
    assert_eq!(
        mode(&target)?,
        0o644,
        "refusal silently tightened the target instead of failing closed"
    );
    Ok(())
}

#[test]
fn a_legacy_writing_guard_blocks_private_replace_without_hiding_the_failed_attempt()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let relative = Path::new("evidence/conversation.json");
    let target = root.path().join(relative);
    fs::create_dir_all(target.parent().ok_or("target has no parent")?)?;
    fs::write(&target, TERMINAL_UNHEALTHY)?;
    fs::set_permissions(&target, fs::Permissions::from_mode(PRIVATE_FILE_MODE))?;
    let guard = writing_guard(&target);
    fs::write(&guard, LEGACY_GUARD)?;
    fs::set_permissions(&guard, fs::Permissions::from_mode(PRIVATE_FILE_MODE))?;

    let result = private_replace(root.path(), relative, TERMINAL_COMPLETE);

    assert!(
        result.is_err(),
        "a private replace ignored the legacy <target>.writing poison guard"
    );
    assert_eq!(
        fs::read(&target)?,
        TERMINAL_UNHEALTHY,
        "the guarded target was promoted from incomplete/unhealthy to complete"
    );
    assert_eq!(mode(&target)?, PRIVATE_FILE_MODE);
    assert_eq!(
        fs::read(&guard)?,
        LEGACY_GUARD,
        "refusal erased the evidence of the unfinished terminal commit"
    );
    assert_eq!(mode(&guard)?, PRIVATE_FILE_MODE);
    Ok(())
}

#[test]
fn an_owner_only_private_target_without_a_guard_is_replaced_normally() -> Result<(), Box<dyn Error>>
{
    let root = tempfile::tempdir()?;
    let relative = Path::new("evidence/conversation.json");
    let target = root.path().join(relative);
    fs::create_dir_all(target.parent().ok_or("target has no parent")?)?;
    fs::write(&target, TERMINAL_UNHEALTHY)?;
    fs::set_permissions(&target, fs::Permissions::from_mode(PRIVATE_FILE_MODE))?;

    private_replace(root.path(), relative, TERMINAL_COMPLETE)?;

    assert_eq!(fs::read(&target)?, TERMINAL_COMPLETE);
    assert_eq!(mode(&target)?, PRIVATE_FILE_MODE);
    assert!(
        !writing_guard(&target).exists(),
        "a successful replace manufactured a legacy poison guard"
    );
    Ok(())
}

#[test]
fn replacing_private_truth_refuses_a_leaf_swapped_after_open() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let relative = Path::new("evidence/conversation.json");
    let target = root.path().join(relative);
    let original = root.path().join("evidence/conversation.original");
    fs::create_dir_all(target.parent().ok_or("target has no parent")?)?;
    fs::write(&target, TERMINAL_UNHEALTHY)?;
    fs::set_permissions(&target, fs::Permissions::from_mode(PRIVATE_FILE_MODE))?;

    let publisher = PrivateFilePublisher::open(root.path(), relative)?;
    let hook: Arc<dyn FaultInjector> = Arc::new(SwapTargetAtBegin {
        target: target.clone(),
        original: original.clone(),
        swapped: AtomicBool::new(false),
    });
    let _scope = scoped_faults(root.path(), hook)?;

    let result = publisher.publish(TERMINAL_COMPLETE, true);

    assert!(
        result.is_err(),
        "private publication trusted a target name after its validated leaf was replaced"
    );
    assert_eq!(fs::read(&target)?, SWAPPED_UNHEALTHY);
    assert_eq!(mode(&target)?, 0o644);
    assert_eq!(fs::read(&original)?, TERMINAL_UNHEALTHY);
    assert_eq!(mode(&original)?, PRIVATE_FILE_MODE);
    Ok(())
}

#[test]
fn replacing_private_truth_refuses_a_parent_swapped_after_open() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let relative = Path::new("evidence/conversation.json");
    let target = root.path().join(relative);
    let original_parent = root.path().join("evidence-original");
    fs::create_dir_all(target.parent().ok_or("target has no parent")?)?;
    fs::write(&target, TERMINAL_UNHEALTHY)?;
    fs::set_permissions(&target, fs::Permissions::from_mode(PRIVATE_FILE_MODE))?;

    let publisher = PrivateFilePublisher::open(root.path(), relative)?;
    fs::rename(root.path().join("evidence"), &original_parent)?;
    fs::create_dir(root.path().join("evidence"))?;
    fs::hard_link(original_parent.join("conversation.json"), &target)?;
    let foreign_temp = root
        .path()
        .join("evidence/.loadout-writing-018f47c0-9b32-7cc1-98ae-0242ac120002.tmp");
    fs::write(
        &foreign_temp,
        b"the substituted parent is not ours to clean",
    )?;

    let result = publisher.publish(TERMINAL_COMPLETE, true);

    assert!(
        result.is_err(),
        "private publication trusted a different parent containing the same leaf inode"
    );
    assert_eq!(fs::read(&target)?, TERMINAL_UNHEALTHY);
    assert_eq!(
        fs::read(original_parent.join("conversation.json"))?,
        TERMINAL_UNHEALTHY
    );
    assert_eq!(
        fs::read(foreign_temp)?,
        b"the substituted parent is not ours to clean"
    );
    Ok(())
}

fn private_replace(root: &Path, relative: &Path, bytes: &[u8]) -> io::Result<()> {
    PrivateFilePublisher::open(root, relative)?.publish(bytes, true)
}

fn writing_guard(target: &Path) -> PathBuf {
    let mut guarded = target.as_os_str().to_os_string();
    guarded.push(".writing");
    PathBuf::from(guarded)
}

fn mode(path: &Path) -> Result<u32, Box<dyn Error>> {
    Ok(fs::metadata(path)?.permissions().mode() & 0o777)
}
