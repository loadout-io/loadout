//! AC-4 dla T-74: sprawdzenie klucza nie jest odpytywaniem triggera i nie pisze plikow.

#![allow(clippy::expect_used)]

use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use loadout_lib::commands::triggers::{self, Secret, Source, Trigger, TriggerError};
use serde_json::json;
use tempfile::TempDir;

const KEY: &str = "lin_api_1234567890123456789012345678901234567890";
const REPLACEMENT: &str = "lin_api_abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMN";
const VIEWER: &[u8] = br#"{"data":{"viewer":{"id":"viewer-1"}}}"#;

type DirectorySnapshot = Vec<(PathBuf, Option<Vec<u8>>)>;

fn write_saved_trigger(home: &Path) -> Result<(), Box<dyn Error>> {
    write_trigger(home, "mine")
}

fn write_trigger(home: &Path, slug: &str) -> Result<(), Box<dyn Error>> {
    let dir = home.join(triggers::TRIGGERS_DIR);
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join(format!("{slug}.json")),
        serde_json::to_vec_pretty(&json!({
            "schema": 1,
            "source": "linear",
            "enabled": true,
            "workflow": "linear.json",
            "condition": "assigned-to-me",
            "poll_every_minutes": 5,
            "api_key": KEY,
        }))?,
    )?;
    Ok(())
}

fn tree(root: &Path) -> Result<DirectorySnapshot, Box<dyn Error>> {
    fn visit(
        root: &Path,
        at: &Path,
        out: &mut Vec<(PathBuf, Option<Vec<u8>>)>,
    ) -> Result<(), Box<dyn Error>> {
        let mut entries = fs::read_dir(at)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let relative = path.strip_prefix(root)?.to_path_buf();
            if entry.file_type()?.is_dir() {
                out.push((relative, None));
                visit(root, &path, out)?;
            } else {
                out.push((relative, Some(fs::read(path)?)));
            }
        }
        Ok(())
    }

    let mut out = Vec::new();
    visit(root, root, &mut out)?;
    Ok(out)
}

fn assert_viewer_probe(key: &Secret) -> Result<(), TriggerError> {
    let mut fetched = 0;
    triggers::test_connection_with(key, |seen, query| {
        fetched += 1;
        assert!(seen.exposes(if key.exposes(KEY) { KEY } else { REPLACEMENT }));
        assert!(
            query.contains("viewer") && !query.contains("issues"),
            "the connection test reused the issue query: {query}"
        );
        Ok(VIEWER.to_vec())
    })?;
    assert_eq!(fetched, 1, "the connection test did not fetch exactly once");
    Ok(())
}

#[test]
fn new_replacement_and_saved_keys_leave_the_entire_home_unchanged() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    write_saved_trigger(home.path())?;
    fs::create_dir_all(home.path().join("workflows"))?;
    fs::write(home.path().join("workflows/linear.json"), b"workflow bytes")?;
    let before = tree(home.path())?;

    let new_key = triggers::connection_key(home.path(), None, Some(Secret::new(REPLACEMENT)))?;
    assert_viewer_probe(&new_key)?;
    let replacement =
        triggers::connection_key(home.path(), Some("mine"), Some(Secret::new(REPLACEMENT)))?;
    assert_viewer_probe(&replacement)?;
    let saved = triggers::connection_key(home.path(), Some("mine"), None)?;
    assert!(
        saved.exposes(KEY),
        "the edit probe did not use the saved key"
    );
    assert_viewer_probe(&saved)?;

    assert_eq!(
        tree(home.path())?,
        before,
        "Test connection changed config, cursor, ledger, delivery or run state"
    );
    assert!(
        !home
            .path()
            .join(triggers::TRIGGERS_DIR)
            .join(".mine.cursor")
            .exists()
    );
    assert!(
        !home
            .path()
            .join(triggers::TRIGGERS_DIR)
            .join(".mine.ledger.json")
            .exists()
    );
    Ok(())
}

#[test]
fn probe_and_watcher_share_the_stdin_only_curl_policy() {
    let key = Secret::new(KEY);
    let query = "query ConnectionTest { viewer { id } }";
    let command = triggers::build_linear_curl_command(&key, query);
    let args = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(args, ["--config", "-"]);
    assert!(
        !args
            .iter()
            .any(|arg| arg.contains(KEY) || arg.contains("api.linear.app")),
        "a secret or address escaped into argv: {args:?}"
    );
    let env = command
        .get_envs()
        .filter_map(|(name, value)| value.map(|value| (name.to_owned(), value.to_owned())))
        .collect::<Vec<_>>();
    assert_eq!(
        env.len(),
        1,
        "env_clear did not leave exactly PATH: {env:?}"
    );
    assert_eq!(env[0].0, OsStr::new("PATH"));

    let config = triggers::linear_curl_config(&key, query);
    assert!(config.contains(KEY));
    assert!(config.contains(query));
    assert!(config.contains("url = \"https://api.linear.app/graphql\""));
    assert!(config.contains("proto = \"=https\""));
    assert!(config.contains("max-time = \"20\""));

    let watcher = Trigger {
        schema: 1,
        source: Source::Linear,
        enabled: true,
        workflow: "linear.json".to_owned(),
        condition: "assigned-to-me".to_owned(),
        poll_every_minutes: 1,
        api_key: key.clone(),
    };
    let watcher_command = triggers::build_curl_command(&watcher);
    let watcher_args = watcher_command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let watcher_env = watcher_command
        .get_envs()
        .filter_map(|(name, value)| value.map(|value| (name.to_owned(), value.to_owned())))
        .collect::<Vec<_>>();
    assert_eq!(
        watcher_args, args,
        "the watcher escaped the shared stdin-only argv policy"
    );
    assert_eq!(
        watcher_env, env,
        "the watcher escaped the shared env_clear policy"
    );
    assert_eq!(
        triggers::curl_config(&watcher),
        triggers::linear_curl_config(&key, triggers::ISSUES_QUERY),
        "the watcher and connection probe no longer share one curl config policy"
    );
}

#[test]
fn production_watcher_edge_builds_the_shared_curl_request() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    write_trigger(home.path(), "runner")?;
    let mut ran = 0;
    let poll = triggers::poll_with_curl_runner(home.path(), "runner", 10, |command, config| {
        ran += 1;
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(args, ["--config", "-"]);
        let env = command
            .get_envs()
            .filter_map(|(name, value)| value.map(|value| (name.to_owned(), value.to_owned())))
            .collect::<Vec<_>>();
        assert_eq!(env.len(), 1);
        assert_eq!(env[0].0, OsStr::new("PATH"));
        assert!(config.contains(KEY));
        assert!(config.contains(triggers::ISSUES_QUERY));
        assert!(config.contains("proto = \"=https\""));
        assert!(config.contains("max-time = \"20\""));
        Ok(br#"{"data":{"issues":{"nodes":[]}}}"#.to_vec())
    })?;
    assert_eq!(ran, 1);
    assert_eq!(poll, triggers::TriggerPoll::Armed);
    Ok(())
}

fn spawn_blocked_poll(
    home: PathBuf,
    slug: &'static str,
    entered: mpsc::Sender<&'static str>,
    release: mpsc::Receiver<()>,
) -> thread::JoinHandle<Result<triggers::TriggerPoll, TriggerError>> {
    thread::spawn(move || {
        triggers::poll_with(&home, slug, 10, |_| {
            entered.send(slug).expect("test receiver remains alive");
            release.recv().expect("test releases the fetch");
            Ok(br#"{"data":{"issues":{"nodes":[]}}}"#.to_vec())
        })
    })
}

#[test]
fn different_triggers_fetch_together_but_one_trigger_never_overlaps() -> Result<(), Box<dyn Error>>
{
    let home = TempDir::new()?;
    for slug in ["first", "second", "same"] {
        write_trigger(home.path(), slug)?;
    }
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_first_tx, release_first_rx) = mpsc::channel();
    let first = spawn_blocked_poll(
        home.path().to_path_buf(),
        "first",
        entered_tx.clone(),
        release_first_rx,
    );
    assert_eq!(entered_rx.recv_timeout(Duration::from_secs(1))?, "first");
    let (release_second_tx, release_second_rx) = mpsc::channel();
    let second = spawn_blocked_poll(
        home.path().to_path_buf(),
        "second",
        entered_tx.clone(),
        release_second_rx,
    );
    let second_entered = entered_rx.recv_timeout(Duration::from_millis(250));
    release_first_tx.send(())?;
    release_second_tx.send(())?;
    first.join().expect("first poll thread")?;
    second.join().expect("second poll thread")?;
    assert_eq!(
        second_entered?, "second",
        "an unrelated trigger was serialized behind a blocked network request"
    );

    let (same_entered_tx, same_entered_rx) = mpsc::channel();
    let (same_release_one_tx, same_release_one_rx) = mpsc::channel();
    let same_one = spawn_blocked_poll(
        home.path().to_path_buf(),
        "same",
        same_entered_tx.clone(),
        same_release_one_rx,
    );
    assert_eq!(
        same_entered_rx.recv_timeout(Duration::from_secs(1))?,
        "same"
    );
    let (same_release_two_tx, same_release_two_rx) = mpsc::channel();
    let same_two = spawn_blocked_poll(
        home.path().to_path_buf(),
        "same",
        same_entered_tx,
        same_release_two_rx,
    );
    let overlapped = same_entered_rx.recv_timeout(Duration::from_millis(150));
    same_release_one_tx.send(())?;
    assert_eq!(
        same_entered_rx.recv_timeout(Duration::from_secs(1))?,
        "same"
    );
    same_release_two_tx.send(())?;
    same_one.join().expect("same first poll thread")?;
    same_two.join().expect("same second poll thread")?;
    assert!(
        matches!(overlapped, Err(mpsc::RecvTimeoutError::Timeout)),
        "two network requests for the same trigger overlapped"
    );
    Ok(())
}

#[test]
fn each_refusal_is_actionable_distinct_and_redacted() {
    let key = Secret::new(KEY);
    let cases: [(&str, Result<Vec<u8>, TriggerError>); 4] = [
        ("html", Ok(b"<html>sign in</html>".to_vec())),
        (
            "api",
            Ok(format!(r#"{{"errors":[{{"message":"{KEY} is not authorized"}}]}}"#).into_bytes()),
        ),
        ("empty", Ok(Vec::new())),
        (
            "process",
            Err(TriggerError::Start(std::io::Error::other(
                "curl was unavailable",
            ))),
        ),
    ];
    let mut said = Vec::new();
    for (label, answer) in cases {
        let mut fetched = 0;
        let error = triggers::test_connection_with(&key, |_, _| {
            fetched += 1;
            answer
        })
        .expect_err(label);
        assert_eq!(fetched, 1, "{label} did not fetch exactly once");
        let sentence = error.to_string();
        assert!(
            !sentence.trim().is_empty(),
            "{label} had no repair sentence"
        );
        assert!(
            !sentence.contains(KEY),
            "{label} exposed the key: {sentence}"
        );
        said.push(sentence);
    }
    said.sort();
    said.dedup();
    assert_eq!(
        said.len(),
        4,
        "different failures collapsed to one sentence"
    );

    let mut fetched = 0;
    let invalid = Secret::new("lin_api_too_short");
    let error = triggers::test_connection_with(&invalid, |_, _| {
        fetched += 1;
        Ok(VIEWER.to_vec())
    })
    .expect_err("bad key shape was accepted");
    assert_eq!(fetched, 0, "an invalid key reached the network");
    assert!(!format!("{error} {error:?}").contains("lin_api_too_short"));

    let oversized_text = format!("lin_api_{}", "a".repeat(32_768));
    let oversized = Secret::new(&oversized_text);
    let mut oversized_fetches = 0;
    let oversized_error = triggers::test_connection_with(&oversized, |_, _| {
        oversized_fetches += 1;
        Ok(VIEWER.to_vec())
    })
    .expect_err("an oversized key reached the pipe writer");
    assert_eq!(oversized_fetches, 0);
    assert!(!format!("{oversized_error:?}").contains(&oversized_text));

    let reflected = format!(r#"{{"errors":[{{"message":"{KEY}"}}]}}"#);
    let watcher_error = triggers::parse_response(reflected.as_bytes())
        .expect_err("the watcher accepted a reflected API error");
    assert!(
        !format!("{watcher_error} {watcher_error:?}").contains(KEY),
        "the issue watcher reflected the Authorization value"
    );

    let missing = triggers::connection_key(Path::new("unused"), None, None)
        .expect_err("a new probe accepted no key");
    let sentence = missing.to_string();
    assert!(sentence.contains("Enter a Linear API key"));
    assert!(!sentence.contains("api_key") && !sentence.contains("trigger file"));
}
