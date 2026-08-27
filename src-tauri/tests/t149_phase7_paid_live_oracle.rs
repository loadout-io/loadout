//! T-149 AC-3: the paid oracle compiles but sleeps in the normal gate.

#[path = "support/t149_phase7.rs"]
pub mod phase7;

use loadout_lib::commands::run::run_workflow_with_reflection;
use loadout_lib::library::agents::Vendor;

#[test]
fn paid_oracle_is_armed_but_requires_the_exact_opt_in() {
    let entrypoint_type = std::any::type_name_of_val(&run_workflow_with_reflection);
    assert!(entrypoint_type.contains("run_workflow_with_reflection"));
    assert!(phase7::paid_opt_in_for(Some("phase7")));
    assert!(!phase7::paid_opt_in_for(None));
    assert!(!phase7::paid_opt_in_for(Some("true")));
    assert!(!phase7::paid_opt_in_for(Some("PHASE7")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "paid phase-7 oracle; requires LOADOUT_PAID_ORACLE=phase7"]
async fn claude_writer_and_codex_judge_complete_the_live_oracle() -> phase7::TestResult {
    phase7::require_paid_oracle();
    live_assignment(phase7::Assignment {
        writer: Vendor::ClaudeCode,
        judge: Vendor::Codex,
    })
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "paid phase-7 oracle; requires LOADOUT_PAID_ORACLE=phase7"]
async fn codex_writer_and_claude_judge_complete_the_live_oracle() -> phase7::TestResult {
    phase7::require_paid_oracle();
    live_assignment(phase7::Assignment {
        writer: Vendor::Codex,
        judge: Vendor::ClaudeCode,
    })
    .await
}

async fn live_assignment(assignment: phase7::Assignment) -> phase7::TestResult {
    let evidence = phase7::run_live_oracle(assignment).await?;
    phase7::assert_live_contract(&evidence, assignment);
    let run = &evidence.core.run_json;
    let spent = run
        .get("spent_usd")
        .and_then(serde_json::Value::as_f64)
        .ok_or("live run.json has no full spend")?;
    assert!(
        (spent
            - evidence.costs.claude_usd
            - evidence.costs.codex_usd
            - evidence.costs.reflection_usd)
            .abs()
            < 1e-9,
        "run.json did not include the private reflection in the full spend"
    );
    Ok(())
}
