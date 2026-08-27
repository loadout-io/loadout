//! Historyczna regresja pyta po T-145 o jedyne dozwolone wyjście recovery.
//!
//! Dawny kontrakt budował pytania i efekty wznowienia z wiersza recovery. Dziś ten sam moduł
//! pilnuje konkretnego odpowiednika każdej tamtej własności: dokładnie przerwane kroki są
//! sprzątane i oznaczane, skończone pozostają nietknięte, a serializowany plan nie może odzyskać
//! żadnego pola transportu rozmowy ani domyślnie wybranego następnego działania.

use anyhow::{Context as _, Result};
use loadout_lib::recovery::{self, Machine, RecoveryPlan, RecoveryRow};
use serde_json::Value as Json;

const BOOT: &str = "1786900000";
const OWN_PGID: i32 = 501;
const RUN: &str = "0199ab00-0000-7000-8000-000000000401";
const RUN_FINISHED: &str = "0199ab00-0000-7000-8000-000000000402";
const STEP_RUNNING: &str = "step-running";
const STEP_READY: &str = "step-ready";

const BANNED_KEY_FRAGMENTS: &[&str] = &[
    "ask",
    "question",
    "resume",
    "session",
    "attempt",
    "option",
    "effect",
    "pick_up",
    "start_over",
    "auto",
    "default",
    "select",
    "chosen",
    "preferred",
    "recommended",
    "primary",
];

fn row(step_id: &str, run_id: &str, step_status: &str, pgid: Option<i32>) -> RecoveryRow {
    RecoveryRow {
        step_id: step_id.to_owned(),
        run_id: run_id.to_owned(),
        run_status: "running".to_owned(),
        step_status: step_status.to_owned(),
        run_boot_id: Some(BOOT.to_owned()),
        pid: pgid,
        pgid,
    }
}

fn interrupted_and_settled_rows() -> Vec<RecoveryRow> {
    vec![
        row(STEP_RUNNING, RUN, "running", Some(5001)),
        row(STEP_READY, RUN, "ready", Some(5002)),
        row("step-succeeded", RUN, "succeeded", Some(5003)),
        row("step-pending", RUN, "pending", None),
        row("step-skipped", RUN, "skipped", None),
    ]
}

fn finished_rows() -> Vec<RecoveryRow> {
    vec![
        row("done-succeeded", RUN_FINISHED, "succeeded", Some(5101)),
        row("done-failed", RUN_FINISHED, "failed", Some(5102)),
        row("done-cancelled", RUN_FINISHED, "cancelled", Some(5103)),
    ]
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

fn status_lines(plan: &RecoveryPlan) -> Vec<String> {
    let mut lines: Vec<String> = plan
        .step_status
        .iter()
        .map(|change| {
            format!(
                "{} -> {} / {}",
                change.step_id, change.status, change.reason
            )
        })
        .collect();
    lines.sort();
    lines
}

fn keys_in(value: &Json, path: &str, found: &mut Vec<String>) {
    match value {
        Json::Object(fields) => {
            for (key, child) in fields {
                let here = format!("{path}.{key}");
                found.push(here.clone());
                keys_in(child, &here, found);
            }
        }
        Json::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                keys_in(child, &format!("{path}[{index}]"), found);
            }
        }
        _ => {}
    }
}

#[test]
fn interrupted_steps_leave_only_cleanup_and_status_facts() -> Result<()> {
    let machine = Machine {
        boot_id: BOOT.to_owned(),
        own_pgid: OWN_PGID,
    };

    let plan = recovery::decide(&interrupted_and_settled_rows(), &machine);

    assert_eq!(
        changed_steps(&plan),
        vec![STEP_READY.to_owned(), STEP_RUNNING.to_owned()],
        "only ready and running steps were cut off; settled and pending rows stay untouched"
    );
    assert_eq!(
        plan.step_status.len(),
        2,
        "one status write is emitted for each of the two cut-off steps"
    );
    assert_eq!(
        status_lines(&plan),
        vec![
            format!("{STEP_READY} -> failed / interrupted"),
            format!("{STEP_RUNNING} -> failed / interrupted"),
        ],
        "failed is the step status and interrupted remains its separate reason"
    );
    assert_eq!(
        plan.reap,
        vec![5001, 5002],
        "only process groups belonging to cut-off steps may be reaped"
    );
    assert_eq!(
        plan.run_status.len(),
        1,
        "both cut-off steps belong to one run, which must be marked once"
    );
    assert_eq!(plan.run_status[0].run_id, RUN);
    assert_eq!(plan.run_status[0].status, "interrupted");
    assert!(
        plan.unreadable.is_empty(),
        "every live row has a known state, boot marker, and safe process group"
    );

    let finished = recovery::decide(&finished_rows(), &machine);
    assert!(
        finished.reap.is_empty(),
        "leftover process groups from finished steps must not be signalled"
    );
    assert!(
        finished.run_status.is_empty(),
        "a running label alone does not prove that a run was cut off"
    );
    assert!(
        finished.step_status.is_empty(),
        "finished steps keep the state they reached before the crash"
    );
    assert!(
        finished.unreadable.is_empty(),
        "settled rows are understood rather than refused"
    );

    let wire = serde_json::to_value(&plan)?;
    let object = wire
        .as_object()
        .context("RecoveryPlan must serialize as the startup consumer's object")?;
    let mut top_level: Vec<&str> = object.keys().map(String::as_str).collect();
    top_level.sort_unstable();
    assert_eq!(
        top_level,
        vec!["reap", "run_status", "step_status", "unreadable"],
        "cleanup has exactly one output shape"
    );

    let text = serde_json::to_string(&wire)?;
    assert!(
        text.contains(STEP_RUNNING) && text.contains(STEP_READY),
        "the serialized plan must carry both real status changes, or the shape sweep is vacuous"
    );
    let mut keys = Vec::new();
    keys_in(&wire, "plan", &mut keys);
    assert!(
        !keys.is_empty(),
        "the serialized plan has no fields, so the forbidden-key sweep would prove nothing"
    );
    for key in &keys {
        let name = key.rsplit('.').next().unwrap_or(key).to_lowercase();
        for fragment in BANNED_KEY_FRAGMENTS {
            assert!(
                !name.contains(fragment),
                "the recovery plan carries forbidden field {key:?}; {fragment:?} would put \
                 conversation transport or a preselected next action back into recovery"
            );
        }
    }

    Ok(())
}
