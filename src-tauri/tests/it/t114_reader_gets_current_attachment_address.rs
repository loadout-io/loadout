//! AC-3 for T-114: a prompt names the current run's full copy without rewriting the durable file.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use loadout_lib::commands::{RunRequest, rerun};
use loadout_lib::memory::handoff;
use loadout_lib::store::Store;

use super::t114_copies_get_noncolliding_git_branches::support::{Rig, Spy, Started, run};

const WORKFLOW: &str = r#"{
  "format": 1, "id": "wf_t114_attachment_address", "name": "Writer then reader",
  "steps": [
    { "kind": "agent", "id": "s_writer", "name": "Writer",
      "agent": "01990000-0000-7000-8000-000000001114", "overrides": {},
      "instructions": "writer: leave the complete result", "folder": { "use": "fresh-copy" } },
    { "kind": "agent", "id": "s_reader", "name": "Reader",
      "agent": "01990000-0000-7000-8000-000000001114", "overrides": {},
      "instructions": "reader: use what came before", "folder": { "use": "fresh-copy" } }
  ],
  "links": [{ "from": "s_writer", "to": "s_reader" }]
}"#;

fn long_answer() -> String {
    format!(
        "## Answer\n{}\n## Evidence\nMeasured.\n\n## Open\nNone.\n",
        "The original answer stays byte for byte in the full copy.\n".repeat(180)
    )
}

fn answer(spec: &loadout_lib::engine::drivers::RunSpec) -> String {
    if spec.prompt.starts_with("writer:") {
        long_answer()
    } else {
        "Reader finished.\n".to_owned()
    }
}

fn request(workflow: PathBuf) -> RunRequest {
    RunRequest {
        workflow,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    }
}

fn reader(seen: &[Started]) -> Result<&Started, Box<dyn Error>> {
    seen.iter()
        .find(|one| one.prompt.starts_with("reader:"))
        .ok_or_else(|| "the reader never received a prompt".into())
}

fn one_handoff(run: &Path) -> Result<handoff::Handoff, Box<dyn Error>> {
    handoff::scan_run_dir(run)?
        .into_iter()
        .find(|one| one.meta.from == "Writer")
        .ok_or_else(|| "the writer's handoff is missing".into())
}

fn one_attachment(run: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let mut files: Vec<PathBuf> = fs::read_dir(run.join(handoff::ATTACHMENTS_DIR))?
        .flatten()
        .map(|entry| entry.path())
        .collect();
    files.sort();
    match files.as_slice() {
        [one] => Ok(one.clone()),
        other => Err(format!("expected one full copy, found {}", other.len()).into()),
    }
}

fn expected_line(label: &str, file: &Path, full: &Path) -> String {
    format!(
        "- Writer: {} ({label}; full text: {})",
        file.display(),
        full.display()
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ordinary_and_resumed_readers_get_current_absolute_full_copy_paths()
-> Result<(), Box<dyn Error>> {
    let rig = Rig::git()?;
    let workflow = rig.workflow("attachment-address", WORKFLOW)?;
    let store = Store::open(&rig.db())?;
    let first_spy = Arc::new(Spy::answering(answer));
    let first = run(&rig, &store, Arc::clone(&first_spy), request(workflow)).await??;
    let handoff = one_handoff(&first.dir)?;
    let full = one_attachment(&first.dir)?;
    let pointer = format!(
        "Moved to attachments/{}",
        full.file_name()
            .and_then(|name| name.to_str())
            .ok_or("bad full-copy name")?
    );
    let pointers: Vec<&str> = handoff
        .body
        .lines()
        .filter(|line| line.starts_with("Moved to "))
        .collect();
    assert!(
        !pointers.is_empty() && pointers.iter().all(|line| *line == pointer),
        "the durable handoff must keep only the portable relative pointer; got {pointers:?}"
    );
    assert!(full.is_file());
    assert_eq!(fs::read(&full)?, long_answer().as_bytes());
    let first_seen = first_spy.started();
    let ordinary = reader(&first_seen)?;
    assert!(
        ordinary
            .prompt
            .lines()
            .any(|line| line == expected_line("what the step before left", &handoff.path, &full))
    );
    let full_dir = full.parent().ok_or("full copy has no folder")?;
    assert!(ordinary.extra_dirs.iter().any(|dir| dir == full_dir));

    let old_name = first
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("bad run folder")?;
    let again = rerun::onward(rig.home.path(), rig.project(), old_name, "s_reader", 1)?;
    let resumed_spy = Arc::new(Spy::answering(answer));
    let second = run(&rig, &store, Arc::clone(&resumed_spy), again.request).await??;
    let carried = one_handoff(&second.dir)?;
    let current_full = one_attachment(&second.dir)?;
    let resumed_seen = resumed_spy.started();
    let resumed = reader(&resumed_seen)?;
    assert!(resumed.prompt.lines().any(|line| line
        == expected_line(
            "what an earlier run left here",
            &carried.path,
            &current_full
        )));
    let current_full_dir = current_full.parent().ok_or("full copy has no folder")?;
    assert!(resumed.extra_dirs.iter().any(|dir| dir == current_full_dir));

    fs::remove_dir_all(&first.dir)?;
    assert!(
        current_full.is_file(),
        "the resumed address still depended on the old run"
    );
    assert_eq!(fs::read(current_full)?, long_answer().as_bytes());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_short_body_gets_neither_an_address_nor_an_attachments_folder()
-> Result<(), Box<dyn Error>> {
    let rig = Rig::git()?;
    let workflow = rig.workflow("short-address", WORKFLOW)?;
    let store = Store::open(&rig.db())?;
    let spy = Arc::new(Spy::answering(|spec| {
        if spec.prompt.starts_with("writer:") {
            "Short answer.\n".to_owned()
        } else {
            String::new()
        }
    }));
    let report = run(&rig, &store, Arc::clone(&spy), request(workflow)).await??;
    assert!(!report.dir.join(handoff::ATTACHMENTS_DIR).exists());
    let seen = spy.started();
    let prompt = &reader(&seen)?.prompt;
    assert!(!prompt.contains("full text:"));
    assert!(
        prompt
            .lines()
            .any(|line| line.ends_with("(what the step before left)"))
    );
    Ok(())
}
