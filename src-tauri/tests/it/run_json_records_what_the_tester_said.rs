//! AC-4 dla T-100: `run.json` zapisuje rozstrzygnięcie każdej wykonanej rundy sędziego.
//!
//! Pole nazywa się `round_outcome`, bo `status` opisuje wykonanie kroku, a nie to, co sędzia
//! powiedział o cudzej pracy. Pierwsza runda poniżej mówi `fail`, lecz jej krok pozostaje
//! zakończony jak dziś; to właśnie przypadek, którego sam stan kroku nie potrafi zachować.

#![allow(clippy::expect_used, clippy::too_many_lines)]

use std::error::Error;
use std::fs;

use loadout_lib::store::Store;
use serde_json::Value as Json;
use tempfile::TempDir;

use super::the_tester_gets_an_outcome_field::{Script, run_fixture};

const LOOP: &str = r#"{
  "format": 1,
  "id": "wf_t100_run_json_outcomes",
  "name": "Three recorded round outcomes",
  "steps": [
    {
      "kind": "agent",
      "id": "s_work",
      "name": "Work",
      "agent": "01990000-0000-7000-8000-000000000100",
      "overrides": {},
      "instructions": "work: make the change.",
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_tester",
      "name": "Tester",
      "agent": "01990000-0000-7000-8000-000000000100",
      "overrides": {},
      "instructions": "tester: decide whether it is good enough.",
      "at": { "x": 0, "y": 200 }
    },
    {
      "kind": "agent",
      "id": "s_after",
      "name": "After",
      "agent": "01990000-0000-7000-8000-000000000100",
      "overrides": {},
      "instructions": "after: build on the accepted work.",
      "at": { "x": 0, "y": 400 }
    }
  ],
  "links": [
    { "from": "s_work", "to": "s_tester" },
    { "from": "s_tester", "to": "s_after" },
    { "from": "s_tester", "to": "s_work", "max_turns": 3 }
  ]
}"#;

const FAIL: &str = "## Answer
This try still has a problem.
outcome: fail

## Evidence
notes.txt:1

## Open
try again.
";

const PASS: &str = "## Answer
This try is good enough.
outcome: pass

## Evidence
notes.txt:1

## Open
nothing.
";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_executed_tester_step_records_its_round_outcome() -> Result<(), Box<dyn Error>> {
    let observed = run_fixture(
        "recorded-outcomes",
        LOOP,
        Script::new(&[("tester", &[FAIL, PASS])]),
    )
    .await?;
    assert_eq!(
        observed.calls_for("tester").len(),
        2,
        "the fixture must fail once and pass once before its third allowed round: {:?}",
        observed.calls
    );

    let run = observed.run_json()?;
    let steps = run
        .get("steps")
        .and_then(Json::as_array)
        .ok_or("run.json has no steps")?;
    let testers: Vec<&Json> = steps
        .iter()
        .filter(|step| step.get("name").and_then(Json::as_str) == Some("Tester"))
        .collect();
    assert_eq!(
        testers.len(),
        3,
        "the recorded graph no longer contains all three planned tester rounds: {testers:?}"
    );
    assert_eq!(
        testers
            .iter()
            .map(|step| step.get("round_outcome").and_then(Json::as_str))
            .collect::<Vec<Option<&str>>>(),
        vec![Some("fail"), Some("pass"), None],
        "run.json did not retain the refusal from the non-final round, the later pass, and no \
         invented result for the round that never ran: {testers:?}"
    );
    assert_eq!(
        testers[0].get("status").and_then(Json::as_str),
        Some("succeeded"),
        "recording a non-final refusal changed the loop state machine instead of adding a \
         durable fact: {:?}",
        testers[0]
    );

    for step in steps
        .iter()
        .filter(|step| step.get("name").and_then(Json::as_str) != Some("Tester"))
    {
        assert!(
            step.get("round_outcome").is_none(),
            "a step that is not the loop tester got an invented round result: {step:?}"
        );
    }

    // Odbudowa jest czytelnikiem trwałego pliku. Musi przyjąć nowy addytywny klucz bez
    // przepisania źródła, a plik starej wersji bez tego klucza ma pozostać poprawnym wejściem.
    let before_rebuild = fs::read_to_string(observed.report.dir.join("run.json"))?;
    let rebuilt = Store::open(&observed.project().join(".loadout/rebuilt.db"))?;
    rebuilt.rebuild_from(&observed.report.dir).await?;
    rebuilt.close().await?;
    assert_eq!(
        fs::read_to_string(observed.report.dir.join("run.json"))?,
        before_rebuild,
        "rebuilding the disposable index rewrote the durable run record"
    );

    let mut old_shape = run.clone();
    if let Some(steps) = old_shape.get_mut("steps").and_then(Json::as_array_mut) {
        for step in steps {
            let _ = step
                .as_object_mut()
                .map(|object| object.remove("round_outcome"));
        }
    }
    let legacy = TempDir::new()?;
    fs::write(
        legacy.path().join("run.json"),
        serde_json::to_vec_pretty(&old_shape)?,
    )?;
    let old_store = Store::open(&legacy.path().join("old.db"))?;
    old_store.rebuild_from(legacy.path()).await?;
    old_store.close().await?;
    Ok(())
}
