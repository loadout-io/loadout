//! T-124 AC-3: atomowa podmiana działa nad starym inode tylko do odczytu.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use loadout_lib::memory::FrontMatter;
use loadout_lib::memory::notes::{Kind, NoteDraft, Scope, Status, record_candidate_for_with_body};
use tempfile::TempDir;

const AGENT: &str = "Rename Keeper";
const OLD_BODY: &str = "# Read-only target\n\nOld complete body.\n";
const NEW_BODY: &str =
    "# Read-only target\n\nNew complete body.\n\nNothing was copied over the old inode.\n";

#[test]
fn writable_parent_replaces_a_read_only_target_without_changing_names() -> Result<(), Box<dyn Error>>
{
    let root = TempDir::new()?;
    let old = record_candidate_for_with_body(
        root.path(),
        draft("2026-08-25T21:00:00Z"),
        AGENT,
        OLD_BODY,
    )?;
    let parent = old
        .path
        .parent()
        .ok_or("the note path has no parent")?
        .to_owned();
    let names = listing(&parent)?;
    let old_raw = fs::read_to_string(&old.path)?;
    let (old_front, old_body_at) = FrontMatter::split(&old_raw)?;
    assert_front(&old_front, "2026-08-25T21:00:00Z", "1");
    assert_eq!(&old_raw[old_body_at..], OLD_BODY);
    assert_ne!(fs::metadata(&parent)?.permissions().mode() & 0o200, 0);

    let _read_only = ModeGuard::set(&old.path, 0o444)?;
    let attempt =
        record_candidate_for_with_body(root.path(), draft("2026-08-25T21:01:00Z"), AGENT, NEW_BODY);
    assert!(
        attempt.is_ok(),
        "a same-directory rename should replace a read-only target: {:?}",
        attempt.as_ref().err()
    );
    let replaced = attempt?;
    assert_eq!(
        listing(&parent)?,
        names,
        "atomic replacement must leave no neighbor"
    );
    assert_eq!(replaced.scope, Scope::ThisAgent);
    assert_eq!(replaced.agent.as_deref(), Some(AGENT));
    assert_eq!(replaced.occurrences, 2);

    let raw = fs::read_to_string(&replaced.path)?;
    let (front, body_at) = FrontMatter::split(&raw)?;
    assert_front(&front, "2026-08-25T21:01:00Z", "2");
    assert_eq!(
        &raw[body_at..],
        NEW_BODY,
        "replacement must keep every new byte"
    );
    Ok(())
}

fn assert_front(front: &FrontMatter, modified: &str, occurrences: &str) {
    assert_eq!(
        front.render(),
        format!(
            "---\nscope: this-agent\nagent: {AGENT}\nkind: fact\ntitle: Read only replacement\nrule: Replace the directory entry atomically.\nbecause: Copying over a read-only inode must be impossible.\nstatus: suggested\noccurrences: {occurrences}\nmodified: {modified}\nlast_used_at: null\n---\n"
        ),
        "the complete front matter, including its owner, must match"
    );
}

fn draft(at: &str) -> NoteDraft {
    NoteDraft {
        title: "Read only replacement".to_owned(),
        rule: "Replace the directory entry atomically.".to_owned(),
        because: "Copying over a read-only inode must be impossible.".to_owned(),
        scope: Scope::ThisAgent,
        kind: Kind::Fact,
        status: Status::Suggested,
        at: at.to_owned(),
    }
}

fn listing(dir: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let mut names = fs::read_dir(dir)?
        .map(|entry| entry.map(|item| item.file_name().to_string_lossy().into_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    names.sort();
    Ok(names)
}

struct ModeGuard {
    path: PathBuf,
    old_mode: u32,
}

impl ModeGuard {
    fn set(path: &Path, mode: u32) -> std::io::Result<Self> {
        let old_mode = fs::metadata(path)?.permissions().mode() & 0o777;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
        Ok(Self {
            path: path.to_owned(),
            old_mode,
        })
    }
}

impl Drop for ModeGuard {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(self.old_mode));
    }
}
