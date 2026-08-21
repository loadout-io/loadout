#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_mcp_becomes_a_disabled_managed_connection()
-> Result<(), Box<dyn std::error::Error>> {
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;

    use loadout_lib::engine::drivers::claude::ClaudeDriver;
    use loadout_lib::engine::drivers::{AgentDriver, Policy, RunSpec};
    use tokio::sync::mpsc;
    use uuid::Uuid;

    let repo = tempfile::tempdir()?;
    std::fs::write(
        repo.path().join(".mcp.json"),
        r#"{"mcpServers":{"docs":{"command":"npx","args":["docs-mcp"],"env":{"DOCS_TOKEN":"${DOCS_TOKEN}"}},"remote":{"url":"https://tools.example.test/mcp","bearerTokenEnvVar":"REMOTE_TOKEN"}}}"#,
    )?;
    let preview = loadout_lib::import::translate::preview(repo.path())?;
    assert_eq!(preview.draft.connections.len(), 2);
    assert!(
        preview
            .draft
            .connections
            .iter()
            .all(|connection| !connection.enabled)
    );
    assert!(!serde_json::to_string(&preview)?.contains("secret-value"));
    let mut enabled = preview.draft.connections.clone();
    for connection in &mut enabled {
        connection.enabled = true;
    }
    let configurations = loadout_lib::connections::runtime::for_connections(&enabled);
    assert!(configurations.claude["mcpServers"].get("docs").is_some());
    assert!(configurations.codex.contains("[mcp_servers.docs]"));
    assert!(!configurations.codex.contains("DOCS_TOKEN="));

    let runtime = tempfile::tempdir()?;
    let proof = runtime.path().join("environment.txt");
    let binary = runtime.path().join("claude");
    std::fs::write(
        &binary,
        format!(
            "#!/bin/sh\nprintf '%s' \"$DOCS_TOKEN\" > '{}'\nwhile IFS= read -r line; do\n  printf '%s\\n' '{{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"terminal_reason\":\"completed\",\"session_id\":\"test\",\"num_turns\":1,\"total_cost_usd\":0,\"result\":\"ok\"}}'\ndone\n",
            proof.display()
        ),
    )?;
    std::fs::set_permissions(&binary, Permissions::from_mode(0o755))?;
    let configuration = loadout_lib::connections::runtime::for_driver(
        runtime.path(),
        "claude",
        &enabled,
        |name| match name {
            "DOCS_TOKEN" => Some("resolved-by-backend".into()),
            "REMOTE_TOKEN" => Some("remote-marker".into()),
            _ => None,
        },
    )?;
    assert!(!format!("{configuration:?}").contains("resolved-by-backend"));
    assert!(
        !std::fs::read_to_string(runtime.path().join("claude-mcp.json"))?
            .contains("resolved-by-backend")
    );
    let codex_configuration =
        loadout_lib::connections::runtime::for_driver(runtime.path(), "codex", &enabled, |name| {
            match name {
                "DOCS_TOKEN" => Some("resolved-by-backend".into()),
                "REMOTE_TOKEN" => Some("remote-marker".into()),
                _ => None,
            }
        })?;
    assert_eq!(
        codex_configuration.arguments.first().map(String::as_str),
        Some("-c")
    );
    assert!(
        codex_configuration
            .arguments
            .iter()
            .any(|argument| argument.starts_with("mcp_servers.docs.command="))
    );
    let driver = ClaudeDriver::with_binary(binary).with_configuration(configuration);
    let (events, _receive) = mpsc::channel(8);
    let mut handle = driver
        .start(
            RunSpec {
                run_id: Uuid::now_v7(),
                cwd: runtime.path().to_path_buf(),
                prompt: "test connection environment".to_owned(),
                model: None,
                system_append: None,
                policy: Policy::ReadOnly,
                tools: None,
                extra_dirs: Vec::new(),
                resume: None,
            },
            events,
        )
        .await?;
    let _outcome = handle.wait().await?;
    let _status = handle.close().await?;
    assert_eq!(
        std::fs::read_to_string(proof)?,
        "resolved-by-backend",
        "the approved environment name must cross the supervised env_clear boundary"
    );
    Ok(())
}
