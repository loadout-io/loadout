//! AC-2 dla T-100: pole `outcome` steruje pętlą, a stara linia zostaje fallbackiem.
//!
//! Dwie sprzeczne pary są celowe. Pole ma dokładną, umówioną nazwę `outcome`; fallback jest
//! czytany tak jak dotąd, bez względu na wielkość liter. Dzięki temu test potrafi dowieść
//! preferencji pola, zamiast dwukrotnie podać ten sam wiersz i nazwać go dwoma nośnikami.

#![allow(clippy::too_many_lines)]

use std::error::Error;

use super::the_tester_gets_an_outcome_field::{Script, run_fixture};

const LOOP: &str = r#"{
  "format": 1,
  "id": "wf_t100_outcome_settles_rounds",
  "name": "A field that settles two rounds",
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
      "whenItFails": "stop",
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
    { "from": "s_tester", "to": "s_work", "max_turns": 2 }
  ]
}"#;

const FIELD_PASS_FALLBACK_FAIL: &str = "## Answer
The work is good enough.
outcome: pass

## Evidence
notes.txt:1

## Open
nothing.

OUTCOME: fail
";

const NO_OUTCOME: &str = "## Answer
The work still has a problem.

## Evidence
notes.txt:1

## Open
fix it in another try.
";

const FIELD_FAIL_FALLBACK_PASS: &str = "## Answer
The work still has a problem.
outcome: fail

## Evidence
notes.txt:1

## Open
fix it before building on this.

OUTCOME: pass
";

const FALLBACK_PASS: &str = "## Answer
The work is good enough.

## Evidence
notes.txt:1

## Open
nothing.

OUTCOME: pass
";

const FALLBACK_FAIL: &str = "## Answer
The work still has a problem.

## Evidence
notes.txt:1

## Open
fix it in another try.

OUTCOME: fail
";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_field_wins_and_the_old_line_still_works() -> Result<(), Box<dyn Error>> {
    // Pole `pass` wygrywa z późniejszą, sprzeczną linią fallbacku. Gdyby nadal czytać tylko
    // ostatnią linię prozy, pętla zużyłaby obie rundy i krok After nigdy by nie ruszył.
    let field_pass = run_fixture(
        "field-pass",
        LOOP,
        Script::new(&[("tester", &[FIELD_PASS_FALLBACK_FAIL])]),
    )
    .await?;
    assert_eq!(
        field_pass.calls_for("tester").len(),
        1,
        "`outcome: pass` from the agreed field did not settle the loop before the second try. \
         The calls were: {:?}",
        field_pass.calls
    );
    assert_eq!(
        field_pass.calls_for("after").len(),
        1,
        "the work after a field-level pass never ran; the conflicting fallback line won"
    );

    // Brak obu nośników w rundzie nieostatniej nadal znaczy fail. W ostatniej rundzie pole
    // `fail` ma wygrać z prozowym `PASS` i pójść przez ustawienie `whenItFails: stop`.
    let field_fail = run_fixture(
        "field-fail",
        LOOP,
        Script::new(&[("tester", &[NO_OUTCOME, FIELD_FAIL_FALLBACK_PASS])]),
    )
    .await?;
    assert_eq!(
        field_fail.calls_for("tester").len(),
        2,
        "a round with neither an outcome field nor a fallback line did not count as fail, or \
         the final field-level refusal did not run. The calls were: {:?}",
        field_fail.calls
    );
    assert!(
        field_fail.calls_for("after").is_empty(),
        "the final `outcome: fail` field was overridden by the later prose fallback and work \
         after a step configured to stop was started"
    );
    let run = field_fail.run_json()?;
    let testers: Vec<&serde_json::Value> = run
        .get("steps")
        .and_then(serde_json::Value::as_array)
        .ok_or("run.json has no steps")?
        .iter()
        .filter(|step| step.get("name").and_then(serde_json::Value::as_str) == Some("Tester"))
        .collect();
    assert_eq!(
        testers
            .get(1)
            .and_then(|step| step.get("status"))
            .and_then(serde_json::Value::as_str),
        Some("failed"),
        "the final refusal did not go through the normal failure path: {testers:?}"
    );

    // Kontrole zgodności: stare workflow nie umawiały pola, więc dotychczasowy wiersz musi
    // rozstrzygać oba kierunki również po przeprowadzce werdyktu.
    let fallback_pass = run_fixture(
        "fallback-pass",
        LOOP,
        Script::new(&[("tester", &[FALLBACK_PASS])]),
    )
    .await?;
    assert_eq!(
        fallback_pass.calls_for("tester").len(),
        1,
        "the legacy pass line no longer settles the loop"
    );
    assert_eq!(
        fallback_pass.calls_for("after").len(),
        1,
        "the step after a legacy pass no longer runs"
    );

    let fallback_fail = run_fixture(
        "fallback-fail",
        LOOP,
        Script::new(&[("tester", &[FALLBACK_FAIL, FALLBACK_PASS])]),
    )
    .await?;
    assert_eq!(
        fallback_fail.calls_for("tester").len(),
        2,
        "the legacy fail line did not send the work around once: {:?}",
        fallback_fail.calls
    );
    assert_eq!(
        fallback_fail.calls_for("after").len(),
        1,
        "the legacy pass after a legacy fail did not let the work continue"
    );
    Ok(())
}
