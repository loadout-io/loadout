//! Nieznane stany z drutu są nazwane bez blokowania wierszy, które recovery umie rozstrzygnąć.
//!
//! Sesja adaptera i licznik prób nie są już wejściem recovery. Historyczne kontrolki braku
//! sesji, ujemnej próby i `i64::MAX` zachowują swoje nazwy, lecz teraz muszą przejść pełną
//! ścieżkę sprzątania i oznaczania. Wyłącznie nieznany status biegu lub kroku pozostaje jawną
//! odmową, a otaczające go czytelne wiersze nie mogą przez nią zniknąć.

use loadout_lib::recovery::{self, Machine, RecoveryPlan, RecoveryRow, reason};

const BOOT: &str = "1786900000";
const OWN_PGID: i32 = 501;
const RUN_MAIN: &str = "0199ab00-0000-7000-8000-000000000601";
const RUN_DRAINING: &str = "0199ab00-0000-7000-8000-000000000602";
const GOOD_PGIDS: [i32; 5] = [6011, 6012, 6013, 6014, 6015];

fn row(step_id: &str, run_id: &str, run_status: &str, step_status: &str, pgid: i32) -> RecoveryRow {
    RecoveryRow {
        step_id: step_id.to_owned(),
        run_id: run_id.to_owned(),
        run_status: run_status.to_owned(),
        step_status: step_status.to_owned(),
        run_boot_id: Some(BOOT.to_owned()),
        pid: Some(pgid),
        pgid: Some(pgid),
    }
}

fn good(step_id: &str, step_status: &str, pgid: i32) -> RecoveryRow {
    row(step_id, RUN_MAIN, "running", step_status, pgid)
}

fn rows() -> Vec<RecoveryRow> {
    vec![
        row(
            "row-unknown-step-status",
            RUN_MAIN,
            "running",
            "zombie",
            6001,
        ),
        good("good-1", "running", GOOD_PGIDS[0]),
        row(
            "row-unknown-run-status",
            RUN_DRAINING,
            "draining",
            "running",
            6002,
        ),
        good("good-2", "running", GOOD_PGIDS[1]),
        good("row-no-session", "running", GOOD_PGIDS[2]),
        good("row-negative-attempt", "running", GOOD_PGIDS[3]),
        good("good-3", "ready", GOOD_PGIDS[4]),
        good("row-huge-attempt", "ready", GOOD_PGIDS[4]),
    ]
}

fn unreadable_ids(plan: &RecoveryPlan) -> Vec<String> {
    let mut ids: Vec<String> = plan
        .unreadable
        .iter()
        .map(|entry| entry.step_id.clone())
        .collect();
    ids.sort();
    ids
}

fn reason_for<'plan>(plan: &'plan RecoveryPlan, step_id: &str) -> Option<&'plan str> {
    plan.unreadable
        .iter()
        .find(|entry| entry.step_id == step_id)
        .map(|entry| entry.reason.as_str())
}

fn changed_steps(plan: &RecoveryPlan) -> Vec<String> {
    let mut ids: Vec<String> = plan
        .step_status
        .iter()
        .map(|change| change.step_id.clone())
        .collect();
    ids.sort();
    ids
}

#[test]
fn unknown_states_are_named_and_recovery_metadata_cannot_block_cleanup() {
    let plan = recovery::decide(
        &rows(),
        &Machine {
            boot_id: BOOT.to_owned(),
            own_pgid: OWN_PGID,
        },
    );

    assert_eq!(
        plan.reap,
        GOOD_PGIDS.to_vec(),
        "five readable process groups are cleaned once; the duplicate group is deduplicated"
    );
    assert_eq!(
        changed_steps(&plan),
        vec![
            "good-1".to_owned(),
            "good-2".to_owned(),
            "good-3".to_owned(),
            "row-huge-attempt".to_owned(),
            "row-negative-attempt".to_owned(),
            "row-no-session".to_owned(),
        ],
        "all readable rows are marked, including the three historical metadata controls"
    );
    assert_eq!(
        plan.step_status.len(),
        6,
        "the shared process group is deduplicated only for signalling, not for status writes"
    );
    assert!(
        plan.step_status
            .iter()
            .all(|change| change.status == "failed" && change.reason == "interrupted"),
        "every readable cut-off row must be handled in full: {:?}",
        plan.step_status
    );
    assert_eq!(
        plan.run_status.len(),
        1,
        "all readable rows belong to one run, which is marked once"
    );
    assert_eq!(plan.run_status[0].run_id, RUN_MAIN);
    assert_eq!(plan.run_status[0].status, "interrupted");

    assert_eq!(
        unreadable_ids(&plan),
        vec![
            "row-unknown-run-status".to_owned(),
            "row-unknown-step-status".to_owned(),
        ],
        "only wire states this version cannot interpret belong in unreadable"
    );
    assert_eq!(
        reason_for(&plan, "row-unknown-run-status"),
        Some(reason::UNKNOWN_RUN)
    );
    assert_eq!(
        reason_for(&plan, "row-unknown-step-status"),
        Some(reason::UNKNOWN_STEP)
    );
    assert!(
        ["row-no-session", "row-negative-attempt", "row-huge-attempt",]
            .iter()
            .all(|step| reason_for(&plan, step).is_none()),
        "adapter session and attempt values cannot make recovery rows unreadable"
    );

    for entry in &plan.unreadable {
        assert!(
            !entry.reason.trim().is_empty(),
            "the entry for {} carries no reason",
            entry.step_id
        );
        assert!(
            !entry.reason.contains('\n'),
            "the reason for {} must be one sentence: {:?}",
            entry.step_id,
            entry.reason
        );
        assert!(
            entry.reason.is_ascii(),
            "the visible recovery reason must be English according to D5: {:?}",
            entry.reason
        );
    }
}
