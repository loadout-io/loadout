//! T-149 AC-1: the shared phase-7 graph runs through production without network access.

#[path = "support/t149_phase7.rs"]
pub mod phase7;

use loadout_lib::library::agents::Vendor;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claude_writer_and_codex_judge_follow_the_complete_offline_contract() -> phase7::TestResult
{
    offline_assignment(phase7::Assignment {
        writer: Vendor::ClaudeCode,
        judge: Vendor::Codex,
    })
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn codex_writer_and_claude_judge_follow_the_complete_offline_contract() -> phase7::TestResult
{
    offline_assignment(phase7::Assignment {
        writer: Vendor::Codex,
        judge: Vendor::ClaudeCode,
    })
    .await
}

async fn offline_assignment(assignment: phase7::Assignment) -> phase7::TestResult {
    let evidence = phase7::run_offline_oracle(assignment).await?;
    phase7::assert_core_contract(&evidence.core, assignment);
    phase7::assert_budget_contract(&evidence.budget);
    let run = &evidence.core.run_json;
    assert_eq!(
        run.get("budget_usd").and_then(serde_json::Value::as_f64),
        Some(phase7::MAX_COST_USD),
        "run.json did not preserve the eight-dollar budget"
    );
    let step_spend: f64 = run["steps"]
        .as_array()
        .ok_or("run.json has no steps")?
        .iter()
        .filter_map(|step| step.get("cost_usd").and_then(serde_json::Value::as_f64))
        .sum();
    let reflection = run
        .pointer("/reflection/cost_usd")
        .and_then(serde_json::Value::as_f64)
        .ok_or("run.json has no separately measured reflection cost")?;
    let spent = run
        .get("spent_usd")
        .and_then(serde_json::Value::as_f64)
        .ok_or("run.json has no full spend")?;
    assert!((spent - step_spend - reflection).abs() < 1e-9);
    Ok(())
}
