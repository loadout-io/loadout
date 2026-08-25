//! T-124 AC-2: pełne ciało powstaje atomowo i retry nie narusza starego pliku.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use loadout_lib::memory::FrontMatter;
use loadout_lib::memory::notes::{
    Kind, NoteDraft, Scope, Status, record_candidate_for, record_candidate_for_with_body,
};
use tempfile::TempDir;

const AGENT: &str = "Atomic Keeper";
const OLD_BODY: &str = "# Atomic note\n\nThe old bytes are complete.\n\nOld tail.\n";
const NEW_BODY: &str = "# Atomic note\n\nThe new bytes are complete.\n\nNew tail.\n";

#[test]
fn denied_neighbor_creation_preserves_listing_and_old_bytes_until_retry()
-> Result<(), Box<dyn Error>> {
    let root = TempDir::new()?;
    let first = record_candidate_for_with_body(
        root.path(),
        draft("2026-08-25T20:00:00Z"),
        AGENT,
        OLD_BODY,
    )?;
    let notes_dir = first
        .path
        .parent()
        .ok_or("the note path has no parent")?
        .to_owned();
    let before_names = listing(&notes_dir)?;
    let before_raw = fs::read_to_string(&first.path)?;
    let (before_front, before_body_at) = FrontMatter::split(&before_raw)?;
    assert_front(&before_front, "2026-08-25T20:00:00Z", "1");
    assert_eq!(&before_raw[before_body_at..], OLD_BODY);
    let before_bytes = before_raw.into_bytes();
    assert_ne!(fs::metadata(&first.path)?.permissions().mode() & 0o200, 0);

    let locked = ModeGuard::set(&notes_dir, 0o555)?;
    let denied =
        record_candidate_for_with_body(root.path(), draft("2026-08-25T20:01:00Z"), AGENT, NEW_BODY);
    assert!(
        denied.is_err(),
        "replacement succeeded without room for its atomic neighbor"
    );
    assert_eq!(listing(&notes_dir)?, before_names);
    assert_eq!(fs::read(&first.path)?, before_bytes);
    drop(locked);

    let replaced = record_candidate_for_with_body(
        root.path(),
        draft("2026-08-25T20:01:00Z"),
        AGENT,
        NEW_BODY,
    )?;
    assert_eq!(listing(&notes_dir)?, before_names);
    assert_eq!(replaced.scope, Scope::ThisAgent);
    assert_eq!(replaced.agent.as_deref(), Some(AGENT));
    let raw = fs::read_to_string(&replaced.path)?;
    let (front, body_at) = FrontMatter::split(&raw)?;
    assert_front(&front, "2026-08-25T20:01:00Z", "2");
    assert_eq!(
        &raw[body_at..],
        NEW_BODY,
        "the retry must persist every new body byte"
    );
    Ok(())
}

#[test]
fn bodyless_api_keeps_its_existing_empty_body() -> Result<(), Box<dyn Error>> {
    let root = TempDir::new()?;
    let note = record_candidate_for(root.path(), bodyless_draft(), Some(AGENT))?;
    let raw = fs::read_to_string(note.path)?;
    let (_, body_at) = FrontMatter::split(&raw)?;
    assert_eq!(&raw[body_at..], "");
    Ok(())
}

fn draft(at: &str) -> NoteDraft {
    NoteDraft {
        title: "Atomic owned note".to_owned(),
        rule: "One stable atomic rule.".to_owned(),
        because: "The old file must survive a failed replacement.".to_owned(),
        scope: Scope::ThisAgent,
        kind: Kind::Fact,
        status: Status::InUse,
        at: at.to_owned(),
    }
}

fn bodyless_draft() -> NoteDraft {
    NoteDraft {
        title: "Bodyless compatibility".to_owned(),
        rule: "The existing API stays bodyless.".to_owned(),
        because: "Existing callers did not supply a body.".to_owned(),
        scope: Scope::ThisAgent,
        kind: Kind::Fact,
        status: Status::Suggested,
        at: "2026-08-25T20:02:00Z".to_owned(),
    }
}

fn assert_front(front: &FrontMatter, modified: &str, occurrences: &str) {
    assert_eq!(
        front.render(),
        format!(
            "---\nscope: this-agent\nagent: {AGENT}\nkind: fact\ntitle: Atomic owned note\nrule: One stable atomic rule.\nbecause: The old file must survive a failed replacement.\nstatus: suggested\noccurrences: {occurrences}\nmodified: {modified}\nlast_used_at: null\n---\n"
        ),
        "the complete front matter, including key order, must stay reviewable"
    );
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
