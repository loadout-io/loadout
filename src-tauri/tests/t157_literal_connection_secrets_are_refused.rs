//! T-157 AC-1: literal connection credentials stop before the imported library is written.
//!
//! Each marker below is deliberately fake and exists only in a `tempfile` project. The target
//! reaches the public `preview -> apply` path for the Claude and Codex adapters; it never reads a
//! person's connection library, home directory, environment, or vendor configuration.

use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::import::{adapters::literal_connection_secret_issue, apply, translate};

const CLAUDE_ARGUMENT: &str = "t157-not-a-real-secret-claude-argument";
const CLAUDE_URL: &str = "t157-not-a-real-secret-claude-url";
const CODEX_ARGUMENT: &str = "t157-not-a-real-secret-codex-argument";
const CODEX_URL: &str = "t157-not-a-real-secret-codex-url";

#[test]
fn literal_connection_secrets_are_refused_before_they_reach_a_library() -> Result<(), Box<dyn Error>>
{
    // The direct smoke keeps the shared policy seam observable, while the loops below remain the
    // product oracle: both real adapters must surface the refusal and `apply` must write nothing.
    let issue =
        literal_connection_secret_issue(&["--token".to_owned(), CLAUDE_ARGUMENT.to_owned()], None)
            .ok_or("the shared connection policy accepted a literal --token value")?;
    assert!(issue.contains("--token"));
    assert!(issue.contains("environment variable"));
    assert!(!issue.contains(CLAUDE_ARGUMENT));

    for form in SecretForm::ALL {
        assert_refused_before_write(
            "Claude",
            form.safe_name(),
            |root| claude_fixture(root, form),
            &[CLAUDE_ARGUMENT, CLAUDE_URL],
        )?;
        assert_refused_before_write(
            "Codex",
            form.safe_name(),
            |root| codex_fixture(root, form),
            &[CODEX_ARGUMENT, CODEX_URL],
        )?;
    }

    assert_safe_connections_still_land("Claude", claude_safe_fixture)?;
    assert_safe_connections_still_land("Codex", codex_safe_fixture)?;
    Ok(())
}

fn assert_refused_before_write<F>(
    vendor: &str,
    safe_name: &str,
    fixture: F,
    literal_markers: &[&str],
) -> Result<(), Box<dyn Error>>
where
    F: FnOnce(&Path) -> Result<(), Box<dyn Error>>,
{
    let project = tempfile::tempdir()?;
    let library = tempfile::tempdir()?;
    fixture(project.path())?;

    let preview = translate::preview(project.path())?;
    // `apply` repeats the production scan before it writes. Before T-157 it reaches the real
    // temporary library; after T-157 it stops here, before staging a Connection file.
    let result = apply::apply(library.path(), &preview.draft);
    assert!(
        result.is_err(),
        "{vendor} accepted a fixture with a literal credential and wrote it through the real \
         import path: {result:?}"
    );
    assert!(
        !library.path().join("connections").exists(),
        "{vendor} left a Connection file behind after refusing the import"
    );

    let report = preview
        .draft
        .report
        .mappings
        .iter()
        .map(|mapping| mapping.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        report.contains("environment variable"),
        "{vendor} did not tell the person to use a named environment variable: {report:?}"
    );
    assert!(
        report.contains(safe_name),
        "{vendor} did not name the safe flag or key {safe_name:?} in its visible refusal: \
         {report:?}"
    );
    for marker in literal_markers {
        assert!(
            !report.contains(marker),
            "{vendor} repeated a literal credential in its visible import message: {report:?}"
        );
    }

    Ok(())
}

fn assert_safe_connections_still_land(
    vendor: &str,
    fixture: fn(&Path) -> Result<(), Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let library = tempfile::tempdir()?;
    fixture(project.path())?;

    let preview = translate::preview(project.path())?;
    assert!(
        preview.draft.runnable(),
        "{vendor} rejected package arguments, ordinary URL query parameters, a URL path with \
         `token`, or a named environment variable: {:?}",
        preview.draft.report.mappings
    );
    let receipt = apply::apply(library.path(), &preview.draft)?;
    assert!(
        receipt
            .written
            .iter()
            .any(|path| path.starts_with("connections")),
        "{vendor} accepted the safe fixture but did not write its Connection into the temporary \
         library: {:?}",
        receipt.written
    );
    Ok(())
}

fn claude_fixture(root: &Path, form: SecretForm) -> Result<(), Box<dyn Error>> {
    fs::write(
        root.join(".mcp.json"),
        format!(r#"{{"mcpServers":{{"fixture":{}}}}}"#, form.claude_server()),
    )?;
    Ok(())
}

fn codex_fixture(root: &Path, form: SecretForm) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(root.join(".codex"))?;
    fs::write(
        root.join(".codex/config.toml"),
        format!("[mcp_servers.fixture]\n{}", form.codex_server()),
    )?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum SecretForm {
    TokenArgument,
    TokenArgumentLeadingDash,
    ApiKeyArgument,
    UrlUserinfo,
    ApiKeyQuery,
    ApiDashKeyQuery,
    AccessTokenQuery,
    TokenQuery,
    SecretQuery,
    AuthorizationQuery,
    EncodedMixedCaseAccessTokenQuery,
}

impl SecretForm {
    const ALL: [Self; 11] = [
        Self::TokenArgument,
        Self::TokenArgumentLeadingDash,
        Self::ApiKeyArgument,
        Self::UrlUserinfo,
        Self::ApiKeyQuery,
        Self::ApiDashKeyQuery,
        Self::AccessTokenQuery,
        Self::TokenQuery,
        Self::SecretQuery,
        Self::AuthorizationQuery,
        Self::EncodedMixedCaseAccessTokenQuery,
    ];

    const fn safe_name(self) -> &'static str {
        match self {
            Self::TokenArgument | Self::TokenArgumentLeadingDash => "--token",
            Self::ApiKeyArgument => "--api-key",
            Self::UrlUserinfo => "URL",
            Self::ApiKeyQuery => "api_key",
            Self::ApiDashKeyQuery => "api-key",
            Self::AccessTokenQuery | Self::EncodedMixedCaseAccessTokenQuery => "access_token",
            Self::TokenQuery => "token",
            Self::SecretQuery => "secret",
            Self::AuthorizationQuery => "authorization",
        }
    }

    fn claude_server(self) -> String {
        match self {
            Self::TokenArgument => {
                format!(r#"{{"command":"tool","args":["--token","{CLAUDE_ARGUMENT}"]}}"#)
            }
            Self::TokenArgumentLeadingDash => {
                format!(r#"{{"command":"tool","args":["--token","-{CLAUDE_ARGUMENT}"]}}"#)
            }
            Self::ApiKeyArgument => {
                format!(r#"{{"command":"tool","args":["--api-key={CLAUDE_ARGUMENT}"]}}"#)
            }
            Self::UrlUserinfo => {
                format!(r#"{{"url":"https://user:{CLAUDE_URL}@tools.example.test/mcp"}}"#)
            }
            form => format!(
                r#"{{"url":"https://tools.example.test/mcp?{}={CLAUDE_URL}"}}"#,
                form.query_key()
            ),
        }
    }

    fn codex_server(self) -> String {
        match self {
            Self::TokenArgument => {
                format!("command = \"tool\"\nargs = [\"--token\", \"{CODEX_ARGUMENT}\"]\n")
            }
            Self::TokenArgumentLeadingDash => {
                format!("command = \"tool\"\nargs = [\"--token\", \"-{CODEX_ARGUMENT}\"]\n")
            }
            Self::ApiKeyArgument => {
                format!("command = \"tool\"\nargs = [\"--api-key={CODEX_ARGUMENT}\"]\n")
            }
            Self::UrlUserinfo => {
                format!("url = \"https://user:{CODEX_URL}@tools.example.test/mcp\"\n")
            }
            form => format!(
                "url = \"https://tools.example.test/mcp?{}={CODEX_URL}\"\n",
                form.query_key()
            ),
        }
    }

    const fn query_key(self) -> &'static str {
        match self {
            Self::ApiKeyQuery => "api_key",
            Self::ApiDashKeyQuery => "api-key",
            Self::AccessTokenQuery => "access_token",
            Self::TokenQuery => "token",
            Self::SecretQuery => "secret",
            Self::AuthorizationQuery => "authorization",
            Self::EncodedMixedCaseAccessTokenQuery => "AcCeSs%5FtOkEn",
            Self::TokenArgument
            | Self::TokenArgumentLeadingDash
            | Self::ApiKeyArgument
            | Self::UrlUserinfo => "",
        }
    }
}

fn claude_safe_fixture(root: &Path) -> Result<(), Box<dyn Error>> {
    fs::write(
        root.join(".mcp.json"),
        r#"{"mcpServers":{
          "package":{
            "command":"npx",
            "args":["--tokenizer","@example/token-tool"],
            "env":{"T157_NAMED_ENV":"${T157_NAMED_ENV}"}
          },
          "ordinary-url":{"url":"https://tools.example.test/token/catalog?page=1&timeout=5"},
          "fragment":{"url":"https://tools.example.test/mcp#note?token=not-a-query"}
        }}"#,
    )?;
    Ok(())
}

fn codex_safe_fixture(root: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(root.join(".codex"))?;
    fs::write(
        root.join(".codex/config.toml"),
        r#"[mcp_servers.package]
command = "npx"
args = ["--tokenizer", "@example/token-tool"]
required_env = ["T157_NAMED_ENV"]

[mcp_servers.ordinary-url]
url = "https://tools.example.test/token/catalog?page=1&timeout=5"

[mcp_servers.fragment]
url = "https://tools.example.test/mcp#note?token=not-a-query"
"#,
    )?;
    Ok(())
}
