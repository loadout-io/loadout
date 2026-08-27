//! T-147 AC-2: KILL delivery is not death proof; a later ESRCH probe is.

use std::collections::VecDeque;
use std::time::Duration;

use loadout_lib::engine::supervisor::{
    GroupProof, ReapAction, ReapResponse, reap_group_with_signaler,
};

#[test]
fn post_kill_no_such_group_probe_is_the_death_proof() {
    let (proof, trace) = scripted_reap(&[
        (ReapAction::Term, ReapResponse::Delivered),
        (ReapAction::Probe, ReapResponse::Delivered),
        (ReapAction::Kill, ReapResponse::Delivered),
        (ReapAction::Probe, ReapResponse::NoSuchGroup),
    ]);

    assert!(
        matches!(proof, GroupProof::Dead { status: None }),
        "only the post-KILL NoSuchGroup probe proves the startup group dead"
    );
    assert_eq!(
        trace,
        [
            ReapAction::Term,
            ReapAction::Probe,
            ReapAction::Kill,
            ReapAction::Probe,
        ],
        "the reaper must probe once before each zero-duration deadline is evaluated"
    );
    assert_eq!(
        trace
            .iter()
            .filter(|&&action| action == ReapAction::Kill)
            .count(),
        1,
        "the proven-dead path must issue KILL exactly once"
    );
}

#[test]
fn delivered_kill_without_a_no_such_group_probe_stays_alive() {
    let (proof, trace) = scripted_reap(&[
        (ReapAction::Term, ReapResponse::Delivered),
        (ReapAction::Probe, ReapResponse::Delivered),
        (ReapAction::Kill, ReapResponse::Delivered),
        (ReapAction::Probe, ReapResponse::Delivered),
    ]);

    assert!(
        matches!(proof, GroupProof::Alive),
        "delivering KILL is not proof that every process in the group is dead"
    );
    assert_eq!(
        trace,
        [
            ReapAction::Term,
            ReapAction::Probe,
            ReapAction::Kill,
            ReapAction::Probe,
        ],
        "the reaper must ask for post-KILL proof even when the group still answers"
    );
    assert_eq!(
        trace
            .iter()
            .filter(|&&action| action == ReapAction::Kill)
            .count(),
        1,
        "an unproven path must not repeat KILL after the final probe"
    );
}

fn scripted_reap(script: &[(ReapAction, ReapResponse)]) -> (GroupProof, Vec<ReapAction>) {
    let mut remaining = script.iter().copied().collect::<VecDeque<_>>();
    let mut trace = Vec::new();
    let proof = reap_group_with_signaler(Duration::ZERO, Duration::ZERO, |action| {
        trace.push(action);
        let Some((expected, response)) = remaining.pop_front() else {
            panic!("the reaper issued an action after the scripted result");
        };
        assert_eq!(action, expected, "the reaper issued actions out of order");
        response
    });
    assert!(
        remaining.is_empty(),
        "the reaper returned before consuming the required action sequence: {remaining:?}"
    );
    (proof, trace)
}
