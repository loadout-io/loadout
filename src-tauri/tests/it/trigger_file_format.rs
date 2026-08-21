#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;

use loadout_lib::commands::triggers::{self, Secret, Source, Trigger};

const KEY: &str = "lin_api_1234567890abcdef1234567890abcdef";

fn raw(extra: &str, key: Option<&str>, source: &str) -> String {
    let key = key.map_or_else(String::new, |value| format!(r#", "api_key": "{value}""#));
    format!(
        r#"{{"schema":1,"source":"{source}","enabled":true,"workflow":"ship.json","condition":"assigned-to-me"{key}{extra}}}"#
    )
}

#[test]
fn trigger_file_round_trips_and_every_refusal_keeps_the_key_secret() {
    assert!(
        KEY.starts_with("lin_api_") && KEY.len() >= 40,
        "the key fixture is too weak"
    );
    let temp = tempfile::tempdir().expect("temp home");
    let dir = temp.path().join(triggers::TRIGGERS_DIR);
    fs::create_dir_all(&dir).expect("trigger dir");
    fs::write(dir.join("mine.json"), raw("", Some(KEY), "linear")).expect("trigger file");

    let loaded = triggers::load(temp.path(), "mine").expect("valid trigger must load");
    assert_eq!(
        loaded,
        Trigger {
            schema: 1,
            source: Source::Linear,
            enabled: true,
            workflow: "ship.json".to_owned(),
            condition: "assigned-to-me".to_owned(),
            poll_every_minutes: triggers::DEFAULT_POLL_EVERY_MINUTES,
            api_key: Secret::new(KEY),
        }
    );
    assert!(loaded.api_key.exposes(KEY));
    assert!(
        !format!("{loaded:?}").contains(KEY),
        "Debug leaked the Linear API key"
    );

    fs::write(
        dir.join("typo.json"),
        raw(",\"workflo\":\"wrong.json\"", Some(KEY), "linear"),
    )
    .expect("typo fixture");
    let typo = triggers::load(temp.path(), "typo").expect_err("unknown key must be refused");
    assert!(
        typo.to_string().contains("workflo"),
        "the refusal did not name the typo: {typo}"
    );
    assert!(
        !typo.to_string().contains(KEY),
        "the typo refusal leaked the key"
    );

    fs::write(dir.join("missing.json"), raw("", None, "linear")).expect("missing-key fixture");
    let missing = triggers::load(temp.path(), "missing").expect_err("missing key must be refused");
    assert!(
        missing.to_string().contains("Linear API key"),
        "no next move in: {missing}"
    );
    assert!(
        !missing.to_string().contains("missing field"),
        "raw serde wording escaped: {missing}"
    );

    fs::write(dir.join("future.json"), raw("", Some(KEY), "clickup"))
        .expect("unknown source fixture");
    let future = triggers::load(temp.path(), "future").expect_err("unknown source must refuse");
    assert!(
        future.to_string().contains("clickup"),
        "source missing from refusal: {future}"
    );
    assert!(
        !future.to_string().contains(KEY),
        "source refusal leaked the key"
    );
}
