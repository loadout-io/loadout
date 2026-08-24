//! AC-2 dla T-101: `Route::Blocked` jest porażką kroku przez to samo wejście, co każda inna.
//! Ostatnia linia stanu jest porównana z `run.json`, bo do dziś okno zostaje na `succeeded`,
//! choć książka po zamknięciu mówi `failed`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;

use super::every_failure_shares_one_door::{
    AMBIGUOUS_ROUTE_REASON, Bench, Intervention, NO_ROUTE_REASON, last_stream_state, reason,
    route_workflow, status,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn no_matching_way_carries_on_and_corrects_the_stream() -> Result<(), Box<dyn Error>> {
    let result = Bench::new()?
        .run(
            "route-no-match-carry-on",
            &route_workflow("carry-on", false),
            None,
            Intervention::None,
        )
        .await?;

    assert_eq!(
        reason(&result.run_file, "Route")?,
        NO_ROUTE_REASON,
        "routing keeps today's precise reason while the shared failure door chooses the effect"
    );
    assert_eq!(status(&result.run_file, "Route")?, "failed");
    assert_eq!(status(&result.run_file, "Route left")?, "succeeded");
    assert_eq!(status(&result.run_file, "Route right")?, "succeeded");
    assert_eq!(
        last_stream_state(&result.lines, "s_route"),
        Some(status(&result.run_file, "Route")?),
        "the last state the window receives must be the same state the durable book keeps"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_matching_ways_pause_when_the_person_asked_to_choose() -> Result<(), Box<dyn Error>> {
    let result = Bench::new()?
        .run(
            "route-ambiguous-ask-me",
            &route_workflow("ask-me", true),
            None,
            Intervention::ContinueWhenPaused,
        )
        .await?;

    assert!(
        result.acted,
        "ask-me on an ambiguous route never exposed the run's paused state"
    );
    assert!(
        reason(&result.run_file, "Route")?.starts_with(AMBIGUOUS_ROUTE_REASON),
        "the route lost today's precise reason after the person answered"
    );
    assert_eq!(status(&result.run_file, "Route left")?, "succeeded");
    assert_eq!(status(&result.run_file, "Route right")?, "succeeded");
    assert_eq!(
        last_stream_state(&result.lines, "s_route"),
        Some(status(&result.run_file, "Route")?)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stop_keeps_the_blocked_reason_and_stops_both_ways() -> Result<(), Box<dyn Error>> {
    for (slug, ambiguous, expected) in [
        ("route-no-match-stop", false, NO_ROUTE_REASON),
        ("route-ambiguous-stop", true, AMBIGUOUS_ROUTE_REASON),
    ] {
        let result = Bench::new()?
            .run(
                slug,
                &route_workflow("stop", ambiguous),
                None,
                Intervention::None,
            )
            .await?;
        assert_eq!(reason(&result.run_file, "Route")?, expected);
        assert_eq!(status(&result.run_file, "Route")?, "failed");
        assert_eq!(status(&result.run_file, "Route left")?, "skipped");
        assert_eq!(status(&result.run_file, "Route right")?, "skipped");
        assert_eq!(
            last_stream_state(&result.lines, "s_route"),
            Some(status(&result.run_file, "Route")?),
            "the window and run.json disagree after {slug}"
        );
    }
    Ok(())
}
