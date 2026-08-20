#![allow(clippy::expect_used)]

use std::fs;

use loadout_lib::commands::triggers;
use serde_json::{Value, json};

fn issue(index: usize) -> Value {
    json!({
        "id": format!("issue-{index}"),
        "identifier": format!("LOAD-{index}"),
        "title": format!("Issue {index}"),
        "url": format!("https://linear.app/loadout/issue/LOAD-{index}"),
        "description": null,
        "updatedAt": format!("2026-08-20T{:02}:00:00.000Z", index % 24)
    })
}

fn answer(nodes: Vec<Value>) -> Vec<u8> {
    serde_json::to_vec(&json!({"data":{"issues":{"nodes":nodes}}})).expect("answer JSON")
}

#[test]
fn first_poll_arms_at_the_latest_issue_and_the_boundary_is_strict() {
    let temp = tempfile::tempdir().expect("temp home");
    fs::create_dir_all(temp.path().join(triggers::TRIGGERS_DIR)).expect("trigger dir");
    let mut nodes = (0..50).map(issue).collect::<Vec<_>>();
    nodes.rotate_left(17);
    assert_eq!(nodes.len(), 50);
    let stamps = nodes
        .iter()
        .map(|one| one["updatedAt"].as_str().expect("stamp"))
        .collect::<Vec<_>>();
    assert!(
        !stamps.windows(2).all(|pair| pair[0] <= pair[1]),
        "fixture accidentally sorted"
    );

    let armed = triggers::check_answer(temp.path(), "mine", &answer(nodes)).expect("arming poll");
    assert!(armed.is_none(), "first poll fired the existing backlog");
    let cursor =
        fs::read_to_string(triggers::cursor_path(temp.path(), "mine")).expect("armed cursor");
    assert_eq!(cursor.trim(), "2026-08-20T23:00:00.000Z");

    let newer = json!({"id":"new","identifier":"LOAD-NEW","title":"New","url":"https://linear.app/loadout/issue/LOAD-NEW","description":"body","updatedAt":"2026-08-21T00:00:00.000Z"});
    let hit = triggers::check_answer(temp.path(), "mine", &answer(vec![newer.clone()]))
        .expect("new poll")
        .expect("one new issue");
    assert_eq!(hit.identifier, "LOAD-NEW");
    assert!(
        triggers::check_answer(temp.path(), "mine", &answer(vec![newer]))
            .expect("equal poll")
            .is_none(),
        "updatedAt equal to the cursor fired again"
    );
}
