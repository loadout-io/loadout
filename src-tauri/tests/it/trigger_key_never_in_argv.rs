#![allow(clippy::expect_used)]

use loadout_lib::commands::triggers::{self, Secret, Source, Trigger};

const KEY: &str = "lin_api_1234567890abcdef1234567890abcdef";

fn trigger() -> Trigger {
    Trigger {
        schema: 1,
        source: Source::Linear,
        enabled: true,
        workflow: "ship.json".to_owned(),
        condition: "assigned-to-me".to_owned(),
        poll_every_minutes: triggers::DEFAULT_POLL_EVERY_MINUTES,
        api_key: Secret::new(KEY),
    }
}

#[test]
fn the_key_and_address_travel_in_curl_config_on_stdin_never_argv_or_environment() {
    let trigger = trigger();
    let command = triggers::build_curl_command(&trigger);
    let args = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        args,
        ["--config", "-"],
        "curl argv is not the stdin-only surface: {args:?}"
    );
    for fragment in KEY.as_bytes().windows(8) {
        let fragment = std::str::from_utf8(fragment).expect("ASCII key");
        assert!(
            !args.iter().any(|arg| arg.contains(fragment)),
            "key fragment leaked: {fragment}"
        );
    }
    assert!(
        !args.iter().any(|arg| arg.contains("api.linear.app")),
        "API URL leaked to argv"
    );

    let env = command
        .get_envs()
        .filter_map(|(name, value)| value.map(|value| (name, value)))
        .collect::<Vec<_>>();
    assert_eq!(
        env.len(),
        1,
        "env_clear did not leave exactly PATH: {env:?}"
    );
    assert_eq!(env[0].0, "PATH");

    let config = triggers::curl_config(&trigger);
    assert!(
        config.contains(KEY),
        "the key disappeared instead of moving to stdin"
    );
    assert!(config.contains("url = \"https://api.linear.app/graphql\""));
    assert!(config.contains("proto = \"=https\""));
    assert!(config.contains("max-time = \"20\""));
}
