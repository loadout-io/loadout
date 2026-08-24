//! AC-1 dla T-101: odmowa złożenia kontekstu nie omija ustawienia `whenItFails`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;

use loadout_lib::engine::step::StepState;

use super::every_failure_shares_one_door::{
    Bench, CONTEXT_REASON, Intervention, context_workflow, failed_handoff_in, reason, status,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn carry_on_reaches_the_next_step_with_the_same_refusal() -> Result<(), Box<dyn Error>> {
    // `prompt` wskazuje plik pod katalogiem ławki, więc ławka musi przeżyć jego odczyt.
    let bench = Bench::new()?;
    let result = bench
        .run(
            "context-carry-on",
            &context_workflow("carry-on"),
            None,
            Intervention::None,
        )
        .await?;

    assert_eq!(
        reason(&result.run_file, "Context")?,
        CONTEXT_REASON,
        "the durable reason remains today's precise refusal, independent of the chosen path"
    );
    assert_eq!(
        status(&result.run_file, "Context")?,
        "failed",
        "carry-on keeps the step red because its context was not proven"
    );
    assert_eq!(
        status(&result.run_file, "After context")?,
        "succeeded",
        "carry-on means the child really runs, not merely that its cone avoids one paint pass"
    );
    assert_eq!(
        result.report.steps,
        vec![
            StepState::Succeeded,
            StepState::Succeeded,
            StepState::Failed,
            StepState::Succeeded
        ]
    );
    let prompt = result
        .watch
        .prompt_starting("after-context:")
        .ok_or("the step after the context refusal never reached the driver")?;
    let _possibly_empty = failed_handoff_in(&prompt)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ask_me_really_pauses_before_it_carries_on() -> Result<(), Box<dyn Error>> {
    let result = Bench::new()?
        .run(
            "context-ask-me",
            &context_workflow("ask-me"),
            None,
            Intervention::ContinueWhenPaused,
        )
        .await?;

    assert!(
        result.acted,
        "the context path never exposed a paused run with asking=true, so ask-me was ignored"
    );
    assert_eq!(status(&result.run_file, "Context")?, "failed");
    assert_eq!(status(&result.run_file, "After context")?, "succeeded");
    assert!(
        reason(&result.run_file, "Context")?.starts_with(CONTEXT_REASON),
        "the refusal changed after the person answered"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stop_keeps_today_s_refusal_and_stops_the_cone() -> Result<(), Box<dyn Error>> {
    let result = Bench::new()?
        .run(
            "context-stop",
            &context_workflow("stop"),
            None,
            Intervention::None,
        )
        .await?;

    assert_eq!(reason(&result.run_file, "Context")?, CONTEXT_REASON);
    assert_eq!(status(&result.run_file, "Context")?, "failed");
    assert_eq!(status(&result.run_file, "After context")?, "skipped");
    assert!(
        result.watch.prompt_starting("after-context:").is_none(),
        "a child ran even though this failure was set to stop"
    );
    Ok(())
}
