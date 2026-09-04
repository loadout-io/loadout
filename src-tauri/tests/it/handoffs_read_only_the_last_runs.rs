//! Z-49: sekcja przekazań otwiera katalogi biegów paczkami, od najnowszego.
//!
//! Słaba wersja sprawdzałaby tylko liczbę zwróconych plików. Przeszłaby na kodzie, który
//! najpierw czyta całe wielogigabajtowe archiwum, a dopiero potem ucina odpowiedź. Pole
//! `runs_read` jest licznikiem katalogów przejętych przez komendę; razem z dokładnym zbiorem
//! nazw dowodzi granicy pierwszej i drugiej paczki.

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use loadout_lib::commands::handoffs::{HandoffPageWire, list_handoffs_inner};

fn handoff(run: &str, number: usize) -> String {
    format!(
        "---\n\
         id: h_{number:02}\n\
         run: {run}\n\
         step: 1\n\
         from: Agent-{number:02}\n\
         to: []\n\
         kind: findings\n\
         title: What run {number:02} found\n\
         status: current\n\
         supersedes: \n\
         reads: []\n\
         created: 2026-09-04T01:00:00Z\n\
         bytes: 7\n\
         est_tokens: 2\n\
         ---\n\n\
         Answer.\n"
    )
}

fn project_with_runs(root: &Path, count: usize) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut runs = Vec::new();
    for number in 0..count {
        let name = format!("20260904-{number:06}__wf_{number:02}");
        let dir = root.join(".loadout").join("runs").join(&name);
        let handoffs = dir.join("handoffs");
        fs::create_dir_all(&handoffs)?;
        fs::write(
            handoffs.join("01__agent__findings.md"),
            handoff(&name, number),
        )?;
        runs.push(dir);
    }
    Ok(runs)
}

fn run_names(page: &HandoffPageWire) -> BTreeSet<&str> {
    page.handoffs
        .iter()
        .map(|handoff| handoff.run.as_str())
        .collect()
}

fn expected_names(from: usize, to: usize) -> BTreeSet<String> {
    (from..to)
        .rev()
        .map(|number| format!("20260904-{number:06}__wf_{number:02}"))
        .collect()
}

#[test]
fn it_reads_only_the_last_ten_run_directories_and_says_how_many_are_left()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    project_with_runs(project.path(), 25)?;

    let first = list_handoffs_inner(project.path(), 0, 10)?;
    assert_eq!(
        first.runs_read, 10,
        "the first page must spend exactly ten run slots"
    );
    assert_eq!(
        first.more_runs, 15,
        "the page must say how many older runs remain"
    );
    assert_eq!(
        run_names(&first),
        expected_names(15, 25).iter().map(String::as_str).collect(),
        "the first page must contain the ten newest runs and none of the fifteen older ones"
    );

    let second = list_handoffs_inner(project.path(), 10, 10)?;
    assert_eq!(
        second.runs_read, 10,
        "the next ask must spend one more ten-run batch"
    );
    assert_eq!(
        second.more_runs, 5,
        "five runs must remain after two batches"
    );
    assert_eq!(
        run_names(&second),
        expected_names(5, 15).iter().map(String::as_str).collect(),
        "the second page must be runs eleven through twenty, not the rest of history"
    );
    Ok(())
}

#[test]
fn a_run_it_cannot_read_still_costs_its_place_in_the_batch() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let runs = project_with_runs(project.path(), 25)?;
    let unreadable = runs[20].join("handoffs");
    let readable_permissions = fs::metadata(&unreadable)?.permissions();
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000))?;

    let cannot_read = fs::read_dir(&unreadable).is_err();
    if !cannot_read {
        fs::set_permissions(&unreadable, readable_permissions.clone())?;
        assert!(
            cannot_read,
            "this filesystem ignored mode 000, so the test did not create an unreadable run"
        );
    }
    let answer = list_handoffs_inner(project.path(), 0, 10);
    fs::set_permissions(&unreadable, readable_permissions)?;
    let page = answer?;

    assert_eq!(
        page.runs_read, 10,
        "an unreadable run still consumes its page slot"
    );
    assert_eq!(
        page.more_runs, 15,
        "the unreadable slot must not pull run eleven forward"
    );
    assert_eq!(
        page.handoffs.len(),
        9,
        "one unreadable directory among the ten newest leaves nine visible handoffs"
    );
    Ok(())
}

#[test]
fn exactly_one_batch_has_nothing_more() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    project_with_runs(project.path(), 10)?;

    let page = list_handoffs_inner(project.path(), 0, 10)?;
    assert_eq!(page.runs_read, 10);
    assert_eq!(page.more_runs, 0);
    Ok(())
}

#[test]
fn fewer_than_one_batch_reads_only_what_exists() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    project_with_runs(project.path(), 3)?;

    let page = list_handoffs_inner(project.path(), 0, 10)?;
    assert_eq!(page.runs_read, 3);
    assert_eq!(page.more_runs, 0);
    Ok(())
}
