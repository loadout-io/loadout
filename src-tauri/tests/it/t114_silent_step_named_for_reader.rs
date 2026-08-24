//! AC-5 for T-114: the real recipient index says when a successful predecessor left no content.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;

use loadout_lib::commands::RunRequest;
use loadout_lib::memory::handoff;
use loadout_lib::store::Store;

use super::t114_copies_get_noncolliding_git_branches::support::{Rig, Spy, Started, run};

const WORKFLOW: &str = r#"{
  "format": 1, "id": "wf_t114_silent", "name": "Silent then reader",
  "steps": [
    { "kind": "agent", "id": "s_silent", "name": "Silent",
      "agent": "01990000-0000-7000-8000-000000001114", "overrides": {},
      "instructions": "silent: answer", "folder": { "use": "fresh-copy" } },
    { "kind": "agent", "id": "s_reader", "name": "Reader",
      "agent": "01990000-0000-7000-8000-000000001114", "overrides": {},
      "instructions": "reader: consume", "folder": { "use": "fresh-copy" } }
  ],
  "links": [{ "from": "s_silent", "to": "s_reader" }]
}"#;

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
        .ok_or_else(|| "the reader never ran".into())
}

fn index_line(prompt: &str) -> Result<&str, Box<dyn Error>> {
    prompt
        .lines()
        .find(|line| line.starts_with("- Silent: "))
        .ok_or_else(|| "the reader's real prompt did not list Silent".into())
}

async fn fixture(
    silent_answer: &'static str,
) -> Result<(Rig, loadout_lib::commands::RunReport, Arc<Spy>), Box<dyn Error>> {
    let rig = Rig::git()?;
    let workflow = rig.workflow("silent", WORKFLOW)?;
    let store = Store::open(&rig.db())?;
    let spy = Arc::new(Spy::answering(move |spec| {
        if spec.prompt.starts_with("silent:") {
            silent_answer.to_owned()
        } else {
            "read\n".to_owned()
        }
    }));
    let report = run(&rig, &store, Arc::clone(&spy), request(workflow)).await??;
    Ok((rig, report, spy))
}

fn silent_handoff(run: &std::path::Path) -> Result<handoff::Handoff, Box<dyn Error>> {
    handoff::scan_run_dir(run)?
        .into_iter()
        .find(|one| one.meta.from == "Silent")
        .ok_or_else(|| "the successful Silent step left no handoff file".into())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn empty_sections_are_named_but_nonempty_rows_remain_byte_for_byte_as_before()
-> Result<(), Box<dyn Error>> {
    let (_empty_rig, empty_report, empty_spy) = fixture("").await?;
    let empty_file = silent_handoff(&empty_report.dir)?;
    let empty_seen = empty_spy.started();
    assert_eq!(
        index_line(&reader(&empty_seen)?.prompt)?,
        format!(
            "- Silent: {} (what the step before left; left nothing)",
            empty_file.path.display()
        )
    );

    let (_text_rig, text_report, text_spy) = fixture("x").await?;
    let text_file = silent_handoff(&text_report.dir)?;
    let text_seen = text_spy.started();
    assert_eq!(
        index_line(&reader(&text_seen)?.prompt)?,
        format!(
            "- Silent: {} (what the step before left)",
            text_file.path.display()
        ),
        "one character of content must preserve today's index row exactly"
    );
    Ok(())
}
