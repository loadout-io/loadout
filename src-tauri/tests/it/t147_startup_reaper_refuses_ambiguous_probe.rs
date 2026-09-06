//! T-147 AC-1: an ambiguous signal result stops the startup reaper before KILL.

use std::collections::VecDeque;
use std::time::Duration;

use loadout_lib::engine::supervisor::{
    GroupProof, ReapAction, ReapResponse, reap_group_with_signaler,
};

#[test]
fn refused_term_returns_alive_without_escalating() {
    let (proof, trace) = scripted_reap(&[(ReapAction::Term, ReapResponse::Refused)]);

    assert!(
        matches!(proof, GroupProof::Alive { .. }),
        "a refused TERM is ambiguous and must keep the group Alive"
    );
    assert_eq!(
        trace,
        [ReapAction::Term],
        "a refused TERM must stop without guessing death or issuing another action"
    );
    assert!(
        !trace.contains(&ReapAction::Kill),
        "a refused TERM must never authorize KILL"
    );
}

#[test]
fn refused_probe_returns_alive_without_escalating() {
    let (proof, trace) = scripted_reap(&[
        (ReapAction::Term, ReapResponse::Delivered),
        (ReapAction::Probe, ReapResponse::Refused),
    ]);

    assert!(
        matches!(proof, GroupProof::Alive { .. }),
        "a refused probe is not proof of death and must keep the group Alive"
    );
    assert_eq!(
        trace,
        [ReapAction::Term, ReapAction::Probe],
        "a refused probe must stop before KILL or another probe"
    );
    assert!(
        !trace.contains(&ReapAction::Kill),
        "an ambiguous probe must never authorize KILL"
    );
}

#[test]
fn a_temporarily_refused_probe_can_observe_later_death_without_another_signal() {
    let mut trace = Vec::new();
    let proof = reap_group_with_signaler(Duration::from_millis(50), Duration::ZERO, |action| {
        trace.push(action);
        match trace.len() {
            1 => ReapResponse::Delivered,
            2 => ReapResponse::Refused,
            _ => ReapResponse::NoSuchGroup,
        }
    });
    assert!(
        matches!(proof, GroupProof::Dead { .. }),
        "a brief EPERM while the group exits must not hide a later real ESRCH: {trace:?}"
    );
    assert_eq!(
        trace,
        [ReapAction::Term, ReapAction::Probe, ReapAction::Probe]
    );
    assert!(
        !trace.contains(&ReapAction::Kill),
        "a refused probe must never authorize KILL"
    );
}

#[test]
fn probing_after_any_refusal_never_recovers_permission_to_escalate() {
    let mut trace = Vec::new();
    let proof = reap_group_with_signaler(Duration::from_millis(25), Duration::ZERO, |action| {
        trace.push(action);
        if trace.len() == 2 {
            ReapResponse::Refused
        } else {
            ReapResponse::Delivered
        }
    });
    assert!(matches!(proof, GroupProof::Alive { .. }));
    assert!(
        trace.len() >= 3,
        "the bounded observation stopped at the first temporary refusal"
    );
    assert!(
        !trace.contains(&ReapAction::Kill),
        "a later successful probe cannot erase the earlier refusal: {trace:?}"
    );
}

fn scripted_reap(script: &[(ReapAction, ReapResponse)]) -> (GroupProof, Vec<ReapAction>) {
    let mut remaining = script.iter().copied().collect::<VecDeque<_>>();
    let mut trace = Vec::new();
    let proof = reap_group_with_signaler(Duration::ZERO, Duration::ZERO, |action| {
        trace.push(action);
        let scripted_response = remaining.pop_front();
        assert!(
            scripted_response.is_some(),
            "the reaper issued an action after the scripted result"
        );
        let Some((expected, response)) = scripted_response else {
            return ReapResponse::Refused;
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
