//! L-02: zapisany powód końca kroku wygrywa z heurystyką czytelnika.
//!
//! Incydent I-02 (bieg z 2026-09-06): `run.json` trzymał `end_cause: "infrastructure-failed"`,
//! a eksport diagnostyczny meldował `unknown`. Powód nie znikał po drodze — nikt go nie czytał.
//! Czytelnik zgadywał z kodu wyjścia i obecności artefaktów, a przy zerowym kodzie i komplecie
//! plików każdy powód wygląda tak samo.
//!
//! Fixture jest syntetyczny. Prywatnych transkryptów z badanego biegu nie kopiujemy.

#![allow(clippy::panic)]

use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::bridge::host::Answers;
use loadout_lib::bridge::library::Desk;
use loadout_lib::bridge::{Answer, Call};
use loadout_lib::commands::diagnostics::support_report;
use serde_json::{Value, json};

const RUN: &str = "01980000-0000-7000-8000-000000000001";

/// I-02 wprost: krok, który padł na infrastrukturze, z zerowym kodem wyjścia i kompletem
/// artefaktów. Zgadywanie nie ma z czego odróżnić go od czegokolwiek innego.
#[test]
fn the_export_reads_the_saved_cause_instead_of_guessing_from_the_exit_code()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    write_run(root.path(), Some("infrastructure-failed"))?;
    let report: Value = serde_json::from_str(&support_report(root.path())?.text())?;
    let step = &report["runs"][0]["steps"][0];
    assert_eq!(
        step["failureKind"].as_str(),
        Some("infrastructureFailed"),
        "the export answered with a guess while the run file held the real cause"
    );
    Ok(())
}

/// Każdy zapisany powód ma własną odpowiedź. Jeden wspólny „unknown" jest gorszy niż brak
/// zdania: czyta się jak stwierdzenie, że Loadout sprawdził i nie wie.
#[test]
fn every_saved_cause_keeps_its_own_answer() -> Result<(), Box<dyn Error>> {
    for (saved, expected) in [
        ("infrastructure-failed", "infrastructureFailed"),
        ("task-failed", "taskFailed"),
        ("cancelled", "cancelled"),
        ("limit-reached", "limitReached"),
        ("refused", "refused"),
        ("unproven-stop", "unprovenStop"),
    ] {
        let root = tempfile::tempdir()?;
        write_run(root.path(), Some(saved))?;
        let report: Value = serde_json::from_str(&support_report(root.path())?.text())?;
        assert_eq!(
            report["runs"][0]["steps"][0]["failureKind"].as_str(),
            Some(expected),
            "the saved cause {saved} did not reach the export as its own answer"
        );
    }
    Ok(())
}

/// Stary raport bez zapisanego powodu nadal się otwiera i nadal dostaje dotychczasową
/// odpowiedź. Nowe pole nie unieważnia plików, które powstały przed nim.
#[test]
fn a_report_without_a_saved_cause_keeps_the_old_answer() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    write_run(root.path(), None)?;
    let report: Value = serde_json::from_str(&support_report(root.path())?.text())?;
    assert_eq!(
        report["runs"][0]["steps"][0]["failureKind"].as_str(),
        Some("unknown"),
        "an older run file without the field lost its previous reading"
    );
    Ok(())
}

/// Lead czyta ten sam fakt z historii. Bez tego nowo otwarta rozmowa widzi krok „failed"
/// i nie ma jak odróżnić awarii Loadouta od oceny agenta.
#[tokio::test]
async fn the_lead_sees_the_saved_cause_in_its_history_lookup() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    write_run(root.path(), Some("infrastructure-failed"))?;
    let desk = Desk::at(Some(root.path().to_path_buf()), root.path().to_path_buf());
    let answer = Answers::answer(&desk, Call {
            id: json!(1),
            call: "read_run_summary".to_owned(),
            input: json!({"run_id": RUN}),
        })
        .await;
    let summary = match answer {
        Answer::Ok(value) => value,
        Answer::Refused(said) => panic!("the Lead could not read the run: {said}"),
    };
    assert_eq!(
        summary["steps"][0]["cause"].as_str(),
        Some("infrastructure-failed"),
        "the Lead's history lookup does not carry why the step ended"
    );
    assert_eq!(
        summary["steps"][0]["state"].as_str(),
        Some("failed"),
        "the state the Lead reads changed shape"
    );
    Ok(())
}

/// Krok, który nigdy nie ruszył, nie ma powodu końca — i Lead nie może zobaczyć tam
/// zmyślonego. Brak pomiaru jest osobnym faktem od porażki.
#[tokio::test]
async fn a_step_that_never_ran_has_no_invented_cause() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let dir = run_dir(root.path());
    fs::create_dir_all(dir.join("handoffs"))?;
    fs::write(
        dir.join("run.json"),
        json!({"id": RUN, "title": "A run", "workflow_id": "wf", "workflow_hash": "rev",
        "status": "failed", "created_at": 1_777_777_777_000_u64,
        "steps": [{"id": "s_late", "node_key": "late", "name": "Late", "agent": "claude-code",
        "kind": "agent", "status": "skipped", "not_run_because": "dependency-failed",
        "executed": false, "process_started": false, "death_proof": false}]})
        .to_string(),
    )?;
    let desk = Desk::at(Some(root.path().to_path_buf()), root.path().to_path_buf());
    let answer = Answers::answer(&desk, Call {
            id: json!(1),
            call: "read_run_summary".to_owned(),
            input: json!({"run_id": RUN}),
        })
        .await;
    let summary = match answer {
        Answer::Ok(value) => value,
        Answer::Refused(said) => panic!("the Lead could not read the run: {said}"),
    };
    assert!(
        summary["steps"][0]["cause"].is_null(),
        "a step that never ran was given a reason for ending"
    );
    Ok(())
}

fn run_dir(project: &Path) -> std::path::PathBuf {
    project
        .join(".loadout/runs")
        .join(format!("20260906-100000__{RUN}"))
}

/// Krok, który padł, z zerowym kodem wyjścia, dowodem zejścia i kompletem artefaktów —
/// czyli dokładnie ten kształt, w którym zgadywanie nie ma z czego wybrać.
fn write_run(project: &Path, cause: Option<&str>) -> Result<(), Box<dyn Error>> {
    let dir = run_dir(project);
    let logs = dir.join("logs");
    fs::create_dir_all(&logs)?;
    fs::create_dir_all(dir.join("handoffs"))?;
    for name in [
        "agent-s_first.jsonl",
        "agent-s_first.stderr.log",
        "agent-s_first.input.json",
    ] {
        fs::write(logs.join(name), "kept out of the report on purpose")?;
    }
    let mut step = json!({
        "id": "s_first", "node_key": "first", "name": "Builder", "agent": "claude-code",
        "kind": "agent", "status": "failed", "executed": true, "process_started": true,
        "exit_code": 0, "death_proof": true
    });
    if let Some(cause) = cause {
        step["end_cause"] = json!(cause);
    }
    fs::write(
        dir.join("run.json"),
        json!({"id": RUN, "title": "A run", "workflow_id": "wf", "workflow_hash": "rev",
        "status": "failed", "created_at": 1_777_777_777_000_u64, "steps": [step]})
        .to_string(),
    )?;
    Ok(())
}
