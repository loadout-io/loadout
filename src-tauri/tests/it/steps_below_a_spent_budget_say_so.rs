//! AC-3 dla T-101: sufit wydatku nie udaje Stopu człowieka w stożku pod zatrzymanym krokiem.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;

use loadout_lib::commands::Outcome;
use loadout_lib::engine::step::StepState;

use super::every_failure_shares_one_door::{
    Bench, Intervention, budget_workflow, reason, status, stop_workflow,
};

const BUDGET: f64 = 10.0;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_budget_reason_reaches_the_step_below_the_one_it_stopped() -> Result<(), Box<dyn Error>>
{
    let result = Bench::new()?
        .run(
            "budget-cone-stop",
            &budget_workflow("stop"),
            Some(BUDGET),
            Intervention::None,
        )
        .await?;

    let stopped = reason(&result.run_file, "Budget stop")?;
    let below = reason(&result.run_file, "Below budget")?;
    assert!(
        stopped.contains("$12.00") && stopped.contains("$10.00"),
        "the stopped step does not name what was spent and what was allowed: {stopped:?}"
    );
    assert_eq!(
        below, stopped,
        "the child needs the same budget reason, not a sentence saying somebody pressed Stop"
    );
    assert_eq!(status(&result.run_file, "Below budget")?, "skipped");
    assert_eq!(
        result.report.outcome,
        Outcome::Done,
        "reaching a configured ceiling is not a user's cancellation"
    );
    assert!(
        !result.report.steps.contains(&StepState::Cancelled),
        "a budget cone contains cancelled rows, which the UI explains as a person pressing Stop: \
         {:?}",
        result.report.steps
    );
    for name in ["Costly", "Budget stop", "Below budget"] {
        let state = status(&result.run_file, name)?;
        let error = reason(&result.run_file, name)?;
        assert!(
            state != "cancelled" || !error.is_empty(),
            "{name} is a silent cancelled row, indistinguishable from a person pressing Stop"
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_real_stop_still_cancels_the_step_and_its_child() -> Result<(), Box<dyn Error>> {
    let result = Bench::new()?
        .run(
            "real-stop-control",
            &stop_workflow(),
            None,
            Intervention::StopWhenRunning("Running"),
        )
        .await?;

    assert!(
        result.acted,
        "the control never stopped a genuinely running step"
    );
    assert_eq!(result.report.outcome, Outcome::Cancelled);
    assert_eq!(
        result.report.steps,
        vec![StepState::Cancelled, StepState::Cancelled],
        "a real Stop remains cancelled all the way down its cone"
    );
    Ok(())
}
