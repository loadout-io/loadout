#![allow(clippy::expect_used)]

use std::fs;

use loadout_lib::commands::triggers;
use serde_json::json;

fn answer(updated_at: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({"data":{"issues":{"nodes":[{
        "id":"issue-1","identifier":"LOAD-101","title":"Ship it",
        "url":"https://linear.app/loadout/issue/LOAD-101","description":"body",
        "updatedAt":updated_at
    }]}}}))
    .expect("answer JSON")
}

#[test]
fn cursor_is_durable_before_a_hit_returns_and_write_failure_never_returns_the_hit() {
    let temp = tempfile::tempdir().expect("temp home");
    let dir = temp.path().join(triggers::TRIGGERS_DIR);
    fs::create_dir_all(&dir).expect("trigger dir");
    let cursor = triggers::cursor_path(temp.path(), "mine");
    fs::write(&cursor, "2026-08-20T08:00:00.000Z\n").expect("old cursor");

    let bytes = answer("2026-08-20T09:00:00.000Z");
    let first = triggers::check_answer(temp.path(), "mine", &bytes)
        .expect("first poll")
        .expect("new issue");
    assert_eq!(first.identifier, "LOAD-101");
    assert_eq!(
        fs::read_to_string(&cursor).expect("cursor on disk").trim(),
        "2026-08-20T09:00:00.000Z"
    );
    assert!(
        triggers::check_answer(temp.path(), "mine", &bytes)
            .expect("second poll")
            .is_none(),
        "the same issue returned twice; an in-memory dedupe would also fail after restart"
    );

    fs::remove_file(&cursor).expect("remove cursor");
    fs::create_dir(&cursor).expect("make the cursor path unwritable as a file");
    let failed = triggers::check_answer(temp.path(), "mine", &bytes);
    assert!(
        failed.is_err(),
        "a cursor write failure returned a hit and can charge twice"
    );
}
