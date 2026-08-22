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

#[test]
fn one_unsafe_server_does_not_hide_the_safe_connections() -> Result<(), Box<dyn std::error::Error>>
{
    use loadout_lib::import::Compatibility;

    let repo = tempfile::tempdir()?;
    std::fs::write(
        repo.path().join(".mcp.json"),
        r#"{"mcpServers":{"browser":{"command":"npx","args":["playwright-mcp"]},"docs":{"url":"https://docs.example.test/mcp"},"remote-design":{"type":"http","url":"http://design.example.test/mcp"}}}"#,
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
    assert!(preview.draft.report.mappings.iter().any(|mapping| {
        mapping.compatibility == Compatibility::NeedsChoice
            && mapping.message.contains("remote-design must use HTTPS")
    }));
    assert!(!preview.draft.runnable());
    Ok(())
}

/// 2026-08-22 — PĘTLA ZWROTNA JEST POPRAWNĄ KONFIGURACJĄ (T-81, AC-2).
///
/// Do tego dnia `http://127.0.0.1:3845/mcp` — czyli dokładnie to, co Figma instaluje jako swój
/// serwer Dev Mode — wylatywało z każdego skanu. Reguła HTTPS broni przed sekretem lecącym po
/// sieci bez szyfrowania, a ruch, który nie wychodzi z maszyny, nie ma gdzie zostać podsłuchany.
/// Kosztem odmowy było prawdziwe połączenie i zdanie `Connection figma does not exist in the
/// Loadout library.` przy starcie biegu — o skutku, którego przyczyna stała dwa ekrany wcześniej.
#[test]
fn a_server_on_this_machine_is_imported_without_https() -> Result<(), Box<dyn std::error::Error>> {
    let repo = tempfile::tempdir()?;
    std::fs::write(
        repo.path().join(".mcp.json"),
        r#"{"mcpServers":{"figma":{"type":"http","url":"http://127.0.0.1:3845/mcp"},"named":{"type":"http","url":"http://localhost:3845/mcp"},"six":{"type":"http","url":"http://[::1]:3845/mcp"}}}"#,
    )?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;

    let mut names: Vec<&str> = preview
        .draft
        .connections
        .iter()
        .map(|connection| connection.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec!["figma", "named", "six"],
        "all three spellings of this machine are the same place, and a person who runs Figma \
         desktop writes whichever one the tool handed them"
    );
    assert!(
        preview
            .draft
            .connections
            .iter()
            .all(|connection| !connection.enabled),
        "reaching this machine is still a tool connection, so it stays off until a person says \
         otherwise — the loopback is about eavesdropping, never about approval"
    );
    Ok(())
}

/// Nazwa hosta, nie prefiks napisu.
#[test]
fn a_hostname_that_merely_starts_like_loopback_is_still_refused()
-> Result<(), Box<dyn std::error::Error>> {
    let repo = tempfile::tempdir()?;
    std::fs::write(
        repo.path().join(".mcp.json"),
        r#"{"mcpServers":{"sneaky":{"type":"http","url":"http://127.0.0.1.example.test/mcp"}}}"#,
    )?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;

    assert!(
        preview.draft.connections.is_empty(),
        "this host is on somebody else's network and only spells the beginning of loopback; a \
         prefix test would have handed a token to it in the clear"
    );
    assert!(
        preview
            .draft
            .report
            .mappings
            .iter()
            .any(|mapping| mapping.message.contains("sneaky must use HTTPS")),
        "and the refusal still names the connection, so the person knows which line to fix. \
         Got: {:?}",
        preview.draft.report.mappings
    );
    assert!(
        !preview.draft.runnable(),
        "a file whose every connection was turned down is not something to import in silence"
    );
    Ok(())
}

#[test]
fn rulesync_jsonc_connections_are_imported_without_running_them()
-> Result<(), Box<dyn std::error::Error>> {
    let repo = tempfile::tempdir()?;
    std::fs::create_dir_all(repo.path().join(".rulesync"))?;
    std::fs::write(
        repo.path().join(".rulesync/mcp.jsonc"),
        r#"{
          // Rulesync permits comments and trailing commas.
          "mcpServers": {
            "docs": {
              "type": "stdio",
              "command": "npx",
              "args": ["-y", "@example/docs"],
              "env": {},
            },
          },
        }"#,
    )?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;
    assert_eq!(preview.draft.connections.len(), 1);
    assert_eq!(preview.draft.connections[0].name, "docs");
    assert!(!preview.draft.connections[0].enabled);
    Ok(())
}

/// 2026-08-22 — POŁĄCZENIE O NAZWIE `type` (T-81, pierwsza połowa).
///
/// Lista połączeń agenta powstawała skanerem linii z czarną listą czterech kluczy
/// (`command`, `args`, `env`, `url`). Serwer HTTP opisany w nagłówku agenta niesie `type: http`,
/// więc `type` wjeżdżało jako druga nazwa połączenia — a `connections::runtime::selected()`
/// odmawia startu przy nazwie, której nie ma w bibliotece. Zaimportowany agent z zagnieżdżonym
/// blokiem `mcpServers` nie dawał się przez to uruchomić w ogóle, a zdanie na ekranie mówiło
/// o połączeniu, którego nikt nigdy nie napisał.
#[test]
fn an_agent_asks_only_for_server_names_never_for_their_fields()
-> Result<(), Box<dyn std::error::Error>> {
    let repo = tempfile::tempdir()?;
    std::fs::create_dir_all(repo.path().join(".claude/agents"))?;
    std::fs::write(
        repo.path().join(".claude/agents/design.md"),
        "---\nname: design\ndescription: Reads a design.\nmcpServers:\n  figma:\n    type: http\n    url: http://127.0.0.1:3845/mcp\n    headers:\n      accept: application/json\n---\nRead the design.\n",
    )?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;
    let agent = preview
        .draft
        .agents
        .first()
        .ok_or("the agent was not imported at all")?;

    assert_eq!(
        agent.connections,
        vec!["figma".to_owned()],
        "only the server NAME is a connection; `type`, `url` and `headers` are its fields. A \
         hand-written list of exceptions loses to the first key nobody met yet, and every phantom \
         name here makes the imported agent refuse to start"
    );
    Ok(())
}

/// 2026-08-22 — SERWER Z NAGŁÓWKA AGENTA WCHODZI RAZEM Z NIM.
///
/// Zgłoszenie właściciela: „jak dziedziczymy agentów i skille, to tak samo wszystko MCP, żeby nie
/// było niespodzianek". Do tego dnia z takiego bloku brane były wyłącznie NAZWY, a transport
/// przepadał — agent lądował w bibliotece z nazwą połączenia, którego w niej nie było, i przewracał
/// bieg dopiero przy Starcie.
#[test]
fn a_server_declared_inside_an_agent_becomes_a_connection() -> Result<(), Box<dyn std::error::Error>>
{
    let repo = tempfile::tempdir()?;
    std::fs::create_dir_all(repo.path().join(".claude/agents"))?;
    std::fs::write(
        repo.path().join(".claude/agents/design.md"),
        "---\nname: design\ndescription: Reads a design.\nmcpServers:\n  playwright:\n    command: npx\n    args: [\"-y\", \"@playwright/mcp@0.0.75\"]\n  figma:\n    type: http\n    url: http://127.0.0.1:3845/mcp\n---\nRead it.\n",
    )?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;

    let mut names: Vec<&str> = preview
        .draft
        .connections
        .iter()
        .map(|one| one.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec!["figma", "playwright"],
        "the agent names both of these in its own header, so importing the agent without them \
         hands the person a role that cannot start"
    );
    assert!(
        preview.draft.connections.iter().all(|one| !one.enabled),
        "arriving is not the same as being switched on: a tool connection stays off until a \
         person says otherwise, wherever it was declared"
    );

    let agent = preview
        .draft
        .agents
        .first()
        .ok_or("the agent was not imported")?;
    assert_eq!(
        agent.connections,
        vec!["figma".to_owned(), "playwright".to_owned()],
        "and the names on the agent match the connections in the library, or the two halves \
         disagree about the same server again"
    );
    Ok(())
}

/// Ten sam serwer opisany dwa razy jest JEDNYM połączeniem.
#[test]
fn a_server_described_in_two_places_lands_once() -> Result<(), Box<dyn std::error::Error>> {
    let repo = tempfile::tempdir()?;
    std::fs::create_dir_all(repo.path().join(".claude/agents"))?;
    std::fs::write(
        repo.path().join(".mcp.json"),
        r#"{"mcpServers":{"playwright":{"command":"npx","args":["playwright-mcp"]}}}"#,
    )?;
    std::fs::write(
        repo.path().join(".claude/agents/design.md"),
        "---\nname: design\ndescription: Reads a design.\nmcpServers:\n  playwright:\n    command: npx\n    args: [\"-y\", \"@playwright/mcp@0.0.75\"]\n---\nRead it.\n",
    )?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;

    assert_eq!(
        preview.draft.connections.len(),
        1,
        "two files describing one server are not two servers. Two entries under one name would \
         put two files in the library, and a person who turns one on gets a run that reads the \
         other. Got: {:?}",
        preview
            .draft
            .connections
            .iter()
            .map(|one| one.name.as_str())
            .collect::<Vec<_>>()
    );
    Ok(())
}

/// 2026-08-22 — TWOJE WŁASNE ZAKRESY MCP TEŻ SIĘ IMPORTUJĄ, i widać, które są które.
///
/// Claude Code ma trzy zakresy, a import czytał jeden. `linear-server`, na którym stoi całe
/// `ship-task` w repo właściciela, siedział w zakresie LOKALNYM (`~/.claude.json`,
/// `projects["<katalog>"]`) — więc nie było go w `.mcp.json`, import go nie widział, a bieg
/// odmawiał startu na kroku, który miał przeczytać ticket.
#[test]
fn your_own_scopes_arrive_labelled_by_who_else_has_them() -> Result<(), Box<dyn std::error::Error>>
{
    use loadout_lib::connections::Origin;

    let repo = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    std::fs::write(
        repo.path().join(".mcp.json"),
        r#"{"mcpServers":{"context7":{"command":"npx","args":["-y","@upstash/context7-mcp"]}}}"#,
    )?;
    std::fs::write(
        home.path().join(".claude.json"),
        format!(
            r#"{{"mcpServers":{{"murmur":{{"type":"http","url":"http://127.0.0.1:8765/mcp"}}}},
                "projects":{{"{}":{{"mcpServers":{{"linear-server":{{"type":"http","url":"https://mcp.linear.app/mcp"}}}}}}}}}}"#,
            repo.path().display()
        ),
    )?;

    let preview = loadout_lib::import::translate::preview_with_personal(repo.path(), home.path())?;
    let mut seen: Vec<(&str, Origin)> = preview
        .draft
        .connections
        .iter()
        .map(|one| (one.name.as_str(), one.origin))
        .collect();
    seen.sort_by_key(|(name, _)| *name);

    assert_eq!(
        seen,
        vec![
            ("context7", Origin::Project),
            ("linear-server", Origin::YoursHere),
            ("murmur", Origin::YoursEverywhere),
        ],
        "all three scopes arrive, and each one remembers which it came from — a person weighing \
         whether to switch a tool server on is asking exactly that: is this the team's setting \
         or my own"
    );
    assert!(
        preview.draft.connections.iter().all(|one| !one.enabled),
        "arriving is not being switched on, wherever the server was written down"
    );
    Ok(())
}

/// Skan bez katalogu domowego nie zagląda do niczyjej konfiguracji.
#[test]
fn a_scan_without_a_home_reads_the_project_only() -> Result<(), Box<dyn std::error::Error>> {
    let repo = tempfile::tempdir()?;
    std::fs::write(
        repo.path().join(".mcp.json"),
        r#"{"mcpServers":{"context7":{"command":"npx","args":["-y","@upstash/context7-mcp"]}}}"#,
    )?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;

    assert_eq!(
        preview.draft.connections.len(),
        1,
        "this is the entry every criterion in this repo uses, and it may not start reading the \
         settings of whoever happens to be running the tests"
    );
    Ok(())
}

/// Ten sam serwer w projekcie i u ciebie jest JEDNYM połączeniem — wygrywa plik projektu.
#[test]
fn the_project_file_wins_over_your_own_copy() -> Result<(), Box<dyn std::error::Error>> {
    use loadout_lib::connections::Origin;

    let repo = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    std::fs::write(
        repo.path().join(".mcp.json"),
        r#"{"mcpServers":{"figma":{"type":"http","url":"https://mcp.figma.com/mcp"}}}"#,
    )?;
    std::fs::write(
        home.path().join(".claude.json"),
        format!(
            r#"{{"projects":{{"{}":{{"mcpServers":{{"figma":{{"type":"http","url":"http://127.0.0.1:3845/mcp"}}}}}}}}}}"#,
            repo.path().display()
        ),
    )?;

    let preview = loadout_lib::import::translate::preview_with_personal(repo.path(), home.path())?;

    assert_eq!(preview.draft.connections.len(), 1);
    assert_eq!(
        preview.draft.connections[0].origin,
        Origin::Project,
        "two entries under one name would put two files in the library, and a person who turns \
         one on gets a run that reads the other. The shared file wins, because that is the one \
         the team agreed on"
    );
    Ok(())
}
