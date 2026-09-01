//! T-135 AC-1: startup cleanup leads with TERM, waits, escalates, and proves death.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use loadout_lib::engine::supervisor::{self, DEFAULT_GRACE, GroupProof};

/// Enough for the production grace plus its bounded proof-after-KILL window. The timeout lives
/// inside the target so a regression is an assertion failure, never harness rc 124.
const CALL_CEILING: Duration = Duration::from_secs(9);

/// A polite orphan should leave as soon as it handles TERM, not sit through the five-second
/// production grace. Two seconds leaves ample room for a loaded CI host without weakening that
/// distinction.
const POLITE_CEILING: Duration = Duration::from_secs(2);

/// Asking about a group already proven gone must be the immediate ESRCH path.
const ESRCH_CEILING: Duration = Duration::from_secs(1);

const POLITE: &str = r#"#!/bin/sh
MARKER_FILE="$1"
READY_FILE="$2"
CHILD_PID_FILE="$3"
(
  trap 'printf handled > "$MARKER_FILE"; exit 0' TERM
  : > "$READY_FILE"
  while :; do /bin/sleep 0.05; done
) &
printf '%s\n' "$!" > "$CHILD_PID_FILE"
/bin/sleep 0.1
"#;

const STUBBORN: &str = r#"#!/bin/sh
READY_FILE="$1"
CHILD_PID_FILE="$2"
(
  trap '' TERM
  : > "$READY_FILE"
  while :; do /bin/sleep 0.05; done
) &
printf '%s\n' "$!" > "$CHILD_PID_FILE"
/bin/sleep 0.1
"#;

#[test]
fn polite_orphan_handles_term_and_an_absent_group_returns_immediately() -> Result<(), Box<dyn Error>>
{
    let fixture = tempfile::tempdir()?;
    let marker = fixture.path().join("term-handled");
    let group = launch_orphan_group(
        fixture.path(),
        "polite.sh",
        POLITE,
        &[
            &marker,
            &fixture.path().join("ready"),
            &fixture.path().join("child-pid"),
        ],
    )?;
    let mut cleanup = GroupCleanup::new(group.pgid);

    let (proof, elapsed) = reap_with_ceiling(group.pgid, CALL_CEILING)?;
    assert_dead(
        &proof,
        "the polite orphan still answers after handling TERM",
    )?;
    assert!(
        marker.exists(),
        "the group disappeared without running its TERM handler; leading with KILL would pass a \
         test that only checked for disappearance"
    );
    assert!(
        elapsed < POLITE_CEILING,
        "the polite orphan handled TERM but reap_group waited {elapsed:?}; the grace is an \
         escalation ceiling, not a delay paid by every cooperative process"
    );
    assert!(
        !group_exists(group.pgid)?,
        "reap_group returned Dead while process group {} still answers signal zero",
        group.pgid
    );
    cleanup.disarm();

    let (second_proof, second_elapsed) = reap_with_ceiling(group.pgid, ESRCH_CEILING)?;
    assert_dead(
        &second_proof,
        "an immediate ESRCH was not accepted as proof that the group is gone",
    )?;
    assert!(
        second_elapsed < ESRCH_CEILING,
        "reaping an already absent group took {second_elapsed:?}; ESRCH must return without \
         waiting through the grace window"
    );
    Ok(())
}

#[test]
fn stubborn_orphan_gets_the_whole_grace_then_kill_and_a_real_death_probe()
-> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let group = launch_orphan_group(
        fixture.path(),
        "stubborn.sh",
        STUBBORN,
        &[
            &fixture.path().join("ready"),
            &fixture.path().join("child-pid"),
        ],
    )?;
    let mut cleanup = GroupCleanup::new(group.pgid);

    let (proof, elapsed) = reap_with_ceiling(group.pgid, CALL_CEILING)?;
    assert_dead(
        &proof,
        "the stubborn orphan survived TERM and reap_group returned Alive instead of escalating",
    )?;
    assert!(
        elapsed >= DEFAULT_GRACE,
        "the stubborn orphan was reported dead after {elapsed:?}, before the full \
         {DEFAULT_GRACE:?} grace elapsed; that leads with KILL wearing a timer"
    );
    assert!(
        elapsed < CALL_CEILING,
        "reap_group exceeded its explicit {CALL_CEILING:?} ceiling"
    );
    assert!(
        !group_exists(group.pgid)?,
        "reap_group returned Dead before signal zero stopped finding process group {}",
        group.pgid
    );
    cleanup.disarm();
    Ok(())
}

#[derive(Debug)]
struct OrphanGroup {
    pgid: i32,
}

/// Starts a launcher in its own process group. The launcher creates the actual long-lived child,
/// reports that child's PID, exits, and is reaped here before the production entry point runs.
/// That ordering prevents the launcher's zombie from keeping `killpg(pgid, 0)` artificially alive.
fn launch_orphan_group(
    dir: &Path,
    name: &str,
    body: &str,
    arguments: &[&Path],
) -> Result<OrphanGroup, Box<dyn Error>> {
    let script = write_script(dir, name, body)?;
    let mut command = Command::new(&script);
    command.args(arguments);
    command.process_group(0);
    command.stdin(Stdio::null());
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());

    let mut launcher = command.spawn()?;
    let pgid = i32::try_from(launcher.id())?;
    let mut cleanup = GroupCleanup::new(pgid);
    let status = launcher.wait()?;
    if !status.success() {
        return Err(format!("the {name} launcher exited as {status:?}").into());
    }

    let ready = arguments
        .iter()
        .find(|path| path.file_name().is_some_and(|file| file == "ready"))
        .ok_or("the fixture did not provide its ready file")?;
    if !wait_for_path(ready, Duration::from_secs(2)) {
        return Err(format!("the orphan in group {pgid} never reported readiness").into());
    }

    let child_pid_file = arguments
        .iter()
        .find(|path| path.file_name().is_some_and(|file| file == "child-pid"))
        .ok_or("the fixture did not provide its child PID file")?;
    let child_pid: i32 = fs::read_to_string(child_pid_file)?.trim().parse()?;
    if child_pid == pgid {
        return Err("the launcher remained the only process in the group".into());
    }
    if !group_exists(pgid)? {
        return Err("the launcher exited but left no live orphan for reap_group to stop".into());
    }

    // The caller installs its guard immediately after this returns. All earlier error paths retain
    // this local guard, while the successful path disarms it only after proving the handoff safe.
    cleanup.disarm();
    Ok(OrphanGroup { pgid })
}

fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    Ok(path)
}

fn wait_for_path(path: &Path, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

fn reap_with_ceiling(
    pgid: i32,
    ceiling: Duration,
) -> Result<(GroupProof, Duration), Box<dyn Error>> {
    let (send, receive) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let began = Instant::now();
        let proof = supervisor::reap_group(pgid);
        let _ = send.send((proof, began.elapsed()));
    });
    let answer = receive.recv_timeout(ceiling).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("reap_group({pgid}) exceeded {ceiling:?}: {error}"),
        )
    })?;
    worker
        .join()
        .map_err(|_| std::io::Error::other("reap_group panicked in its bounded worker"))?;
    Ok(answer)
}

fn assert_dead(proof: &GroupProof, context: &str) -> Result<(), Box<dyn Error>> {
    match proof {
        GroupProof::Dead { .. } => Ok(()),
        GroupProof::Alive { .. } => Err(context.to_owned().into()),
    }
}

/// A zero signal changes no process state. The group belongs to this test user, so a failed
/// probe here is ESRCH rather than the EPERM ambiguity that production must treat as alive.
fn group_exists(pgid: i32) -> Result<bool, Box<dyn Error>> {
    let status = Command::new("/bin/kill")
        .arg("-0")
        .arg(format!("-{pgid}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    Ok(status.success())
}

/// Test-only last resort. The production policy remains wholly in `supervisor.rs`; this guard
/// merely prevents a deliberately stubborn fixture from surviving a failed assertion.
#[derive(Debug)]
struct GroupCleanup {
    pgid: i32,
    armed: bool,
}

impl GroupCleanup {
    fn new(pgid: i32) -> Self {
        Self { pgid, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for GroupCleanup {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let _ = Command::new("/bin/kill")
            .arg("-KILL")
            .arg(format!("-{}", self.pgid))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}
