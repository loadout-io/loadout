#[test]
fn every_typed_source_selects_exactly_one_persisted_branch() {
    use std::collections::BTreeMap;

    use loadout_lib::workflow::{
        CheckOutcome, Condition, ConditionalLink, RouteError, RouteEvidence, select_branch,
    };

    let links = vec![
        ConditionalLink {
            from: "check".into(),
            to: "pass".into(),
            when: Condition::Check {
                outcome: CheckOutcome::Passed,
            },
        },
        ConditionalLink {
            from: "check".into(),
            to: "fail".into(),
            when: Condition::Check {
                outcome: CheckOutcome::Failed,
            },
        },
        ConditionalLink {
            from: "choice".into(),
            to: "approved".into(),
            when: Condition::Checkpoint {
                choice: "approve".into(),
            },
        },
        ConditionalLink {
            from: "review".into(),
            to: "repair".into(),
            when: Condition::Handoff {
                field: "outcome".into(),
                equals: "fail".into(),
            },
        },
    ];
    assert_eq!(
        select_branch(
            &links,
            "check",
            Some(&RouteEvidence::Check(CheckOutcome::Failed))
        )
        .map(|selected| selected.map(|link| link.to.as_str())),
        Ok(Some("fail"))
    );
    assert_eq!(
        select_branch(
            &links,
            "choice",
            Some(&RouteEvidence::Checkpoint("approve".into()))
        )
        .map(|selected| selected.map(|link| link.to.as_str())),
        Ok(Some("approved"))
    );
    assert_eq!(
        select_branch(
            &links,
            "review",
            Some(&RouteEvidence::Handoff(BTreeMap::from([(
                "outcome".into(),
                "fail".into()
            )])))
        )
        .map(|selected| selected.map(|link| link.to.as_str())),
        Ok(Some("repair"))
    );
    assert_eq!(
        select_branch(&links, "check", None),
        Err(RouteError::MissingEvidence)
    );
    assert_eq!(
        select_branch(
            &links,
            "choice",
            Some(&RouteEvidence::Checkpoint("unknown".into()))
        ),
        Err(RouteError::NoMatch)
    );
    let mut ambiguous = links.clone();
    ambiguous.push(links[0].clone());
    assert_eq!(
        select_branch(
            &ambiguous,
            "check",
            Some(&RouteEvidence::Check(CheckOutcome::Passed))
        ),
        Err(RouteError::Ambiguous)
    );
    assert_eq!(select_branch(&links, "plain", None), Ok(None));
}

#[tokio::test]
async fn scheduler_runs_only_the_selected_branch_and_preserves_fan_in()
-> Result<(), Box<dyn std::error::Error>> {
    use loadout_lib::engine::dag::Dag;
    use loadout_lib::engine::scheduler::{Route, execute_routed};
    use loadout_lib::engine::step::{StepReport, StepState};
    use std::sync::{Arc, Mutex, PoisonError};
    use tokio_util::sync::CancellationToken;

    let dag = Dag::new(4, &[(0, 1), (0, 2), (1, 3), (2, 3)])?;
    let ran = Arc::new(Mutex::new(Vec::new()));
    let run_step = {
        let ran = Arc::clone(&ran);
        move |id, _cancel| {
            let ran = Arc::clone(&ran);
            async move {
                ran.lock().unwrap_or_else(PoisonError::into_inner).push(id);
                StepReport::Succeeded
            }
        }
    };
    let outcome = execute_routed(&dag, 4, CancellationToken::new(), run_step, |id, _| {
        if id == 0 {
            Route::Only(vec![2])
        } else {
            Route::All
        }
    })
    .await;
    assert_eq!(
        outcome.states,
        vec![
            StepState::Succeeded,
            StepState::Skipped,
            StepState::Succeeded,
            StepState::Succeeded
        ]
    );
    let ran = ran.lock().unwrap_or_else(PoisonError::into_inner);
    assert!(!ran.contains(&1));
    assert!(ran.contains(&2));
    assert!(ran.contains(&3));
    Ok(())
}
