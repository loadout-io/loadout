#![allow(clippy::expect_used)]

use loadout_lib::commands::triggers;
use serde_json::Value;

const FIXTURE: &[u8] = include_bytes!("../../../docs/research/fixtures/linear-assigned.json");

#[test]
fn linear_answer_is_permissive_but_bad_transport_shapes_are_distinct_refusals() {
    let raw: Value = serde_json::from_slice(FIXTURE).expect("golden fixture JSON");
    let nodes = raw["data"]["issues"]["nodes"]
        .as_array()
        .expect("nodes array");
    assert!(
        nodes.len() >= 3,
        "fixture has too few issues to exercise the decoder"
    );
    assert!(
        nodes
            .iter()
            .any(|one| one.get("addedByLinearLater").is_some())
    );
    assert!(
        nodes
            .iter()
            .any(|one| one.get("description").is_some_and(Value::is_null))
    );

    let issues = triggers::parse_response(FIXTURE).expect("golden response must parse");
    assert_eq!(issues.len(), 3);
    assert_eq!(issues[0].identifier, "LOAD-101");
    assert_eq!(issues[0].title, "Keep the cursor on disk");
    assert_eq!(issues[0].url, "https://linear.app/loadout/issue/LOAD-101");
    assert_eq!(issues[0].body, "Do not fire the same work twice.");
    assert_eq!(
        issues[1].body, "",
        "null description must become an empty body"
    );
    assert!(
        triggers::parse_response(br#"{"data":{"issues":{"nodes":[]}}}"#)
            .expect("empty list")
            .is_empty()
    );

    let empty = triggers::parse_response(b"")
        .expect_err("empty stdout must refuse")
        .to_string();
    let html = triggers::parse_response(b"<html>bad gateway</html>")
        .expect_err("HTML must refuse")
        .to_string();
    let api = triggers::parse_response(br#"{"errors":[{"message":"not allowed"}]}"#)
        .expect_err("GraphQL errors must refuse")
        .to_string();
    assert_ne!(empty, html);
    assert_ne!(html, api);
    assert_ne!(empty, api);
}
