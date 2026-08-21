//! AC-7 dla T-65: biblioteka triggerów jest zredagowana, a przełącznik edytuje prawdziwy plik.
//!
//! Wyrocznia nie pyta o helper serializacji. Czyta katalog tak jak ekran, następnie woła tę
//! samą funkcję, którą opakowuje komenda `set_trigger_enabled`, i na końcu ładuje plik od nowa.

#![allow(clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use loadout_lib::commands::triggers::{self, Source};
use serde_json::{Value, json};
use tempfile::TempDir;

const SECRET: &str = "lin_api_1234567890123456789012345678901234567890";

#[test]
fn accepted_poll_uses_the_frontend_wire_names() -> Result<(), Box<dyn Error>> {
    let wire = serde_json::to_value(triggers::TriggerPoll::Accepted {
        workflow: "ship-it.json".to_owned(),
        receipt_at: 17,
    })?;
    assert_eq!(wire["status"], "accepted");
    assert_eq!(wire["workflow"], "ship-it.json");
    assert_eq!(wire["receiptAt"], 17);
    assert!(
        wire.get("receipt_at").is_none(),
        "Rust exposed a field name the frontend never reads"
    );
    Ok(())
}

#[test]
fn boxed_pending_keeps_the_same_frontend_object() -> Result<(), Box<dyn Error>> {
    let delivery = triggers::TriggerDelivery {
        claim: triggers::TriggerClaim {
            slug: "mine".to_owned(),
            delivery_id: "delivery-1".to_owned(),
            workflow: "ship-it.json".to_owned(),
            run_id: "run-1".to_owned(),
        },
        issue: triggers::Issue {
            id: "issue-1".to_owned(),
            identifier: "LOAD-1".to_owned(),
            title: "Ship it".to_owned(),
            url: "https://linear.app/loadout/issue/LOAD-1".to_owned(),
            body: "body".to_owned(),
            updated_at: "2026-08-21T09:00:00.000Z".to_owned(),
        },
        created_at: 17,
    };
    let wire = serde_json::to_value(triggers::TriggerPoll::Pending {
        delivery: Box::new(delivery.clone()),
    })?;
    assert_eq!(wire["status"], "pending");
    assert_eq!(wire["delivery"], serde_json::to_value(delivery)?);
    assert!(
        wire["delivery"].is_object(),
        "boxing the Rust payload changed the IPC object into a wrapper"
    );
    Ok(())
}

#[test]
fn listing_names_every_file_without_ever_exposing_a_key() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let dir = home.path().join(triggers::TRIGGERS_DIR);
    fs::create_dir_all(&dir)?;
    write_trigger(&dir.join("mine.json"), true, "ship-it")?;
    write_trigger(&dir.join("nightly.json"), false, "verify")?;
    fs::write(dir.join("broken.json"), b"{ definitely not json")?;
    // Ukryte pliki są ledgerem/kursorem, nie konfiguracją. JSON-owa treść celowo wygląda
    // wiarygodnie, żeby filtr oparty na samym rozszerzeniu tego nie przeoczył.
    fs::write(dir.join(".mine.ledger.json"), b"{}")?;

    let mut entries = triggers::list(home.path())?;
    entries.sort_by(|left, right| left.slug.cmp(&right.slug));
    let slugs = entries
        .iter()
        .map(|entry| entry.slug.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        slugs,
        vec!["broken", "mine", "nightly"],
        "hidden cursor/ledger files were presented as triggers, or a broken named file vanished"
    );

    let mine = entries
        .iter()
        .find(|entry| entry.slug == "mine")
        .ok_or("the healthy trigger disappeared from the library")?;
    assert_eq!(mine.source, Some(Source::Linear));
    assert_eq!(mine.condition.as_deref(), Some("assigned to me"));
    assert_eq!(mine.workflow.as_deref(), Some("ship-it"));
    assert_eq!(mine.enabled, Some(true));
    assert!(
        mine.problem.is_none(),
        "a healthy file was reported as broken"
    );

    let broken = entries
        .iter()
        .find(|entry| entry.slug == "broken")
        .ok_or("the broken file disappeared instead of becoming a named problem")?;
    assert!(
        broken
            .problem
            .as_deref()
            .is_some_and(|problem| !problem.trim().is_empty()),
        "the broken file is present but says no actionable problem"
    );
    assert!(
        broken.source.is_none()
            && broken.condition.is_none()
            && broken.workflow.is_none()
            && broken.enabled.is_none(),
        "the invalid JSON was filled with invented configuration values"
    );

    let wire = serde_json::to_string(&entries)?;
    let debug = format!("{entries:?}");
    for exposed in [SECRET, "api_key", "apiKey"] {
        assert!(
            !wire.contains(exposed) && !debug.contains(exposed),
            "the redacted library exposed {exposed:?}; a serde skip on one path is not enough \
             because Debug/logging is a second path"
        );
    }
    Ok(())
}

#[test]
fn an_unknown_source_never_echoes_a_value_that_may_be_a_key() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let dir = home.path().join(triggers::TRIGGERS_DIR);
    fs::create_dir_all(&dir)?;
    for secret_shaped_source in [SECRET, "d7c44d8f5f0a4a069bcb86279301fd29"] {
        fs::write(
            dir.join("mine.json"),
            serde_json::to_vec_pretty(&json!({
                "schema": 1,
                "source": secret_shaped_source,
                "enabled": true,
                "workflow": "ship-it",
                "condition": "assigned to me",
                "api_key": SECRET
            }))?,
        )?;

        let mut fetched = false;
        let error = triggers::poll_with(home.path(), "mine", 17, |_| {
            fetched = true;
            Err(triggers::TriggerError::EmptyAnswer)
        })
        .expect_err("a secret-shaped source was accepted");
        assert!(!fetched, "an invalid source reached the fetcher");
        let exposed = format!("{error} {error:?}");
        assert!(
            !exposed.contains(secret_shaped_source) && !exposed.contains("lin_api_"),
            "the error crossing IPC echoed the unknown source: {exposed}"
        );
        assert!(exposed.contains("Choose `linear`"));
    }
    Ok(())
}

#[test]
fn switching_is_atomic_and_preserves_every_other_byte_of_meaning() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let dir = home.path().join(triggers::TRIGGERS_DIR);
    fs::create_dir_all(&dir)?;
    let path = dir.join("mine.json");
    write_trigger(&path, true, "ship-it")?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640))?;
    let before: Value = serde_json::from_slice(&fs::read(&path)?)?;

    // Czytelnik pracuje równolegle z wieloma przełączeniami. Przy zapisie wprost do pliku
    // zobaczy czasem zero bajtów albo pół JSON-a; rename w tym samym katalogu pokazuje zawsze
    // pełną starą albo pełną nową wersję.
    let reader = Reader::start(path.clone());
    for index in 0..64 {
        let wanted = index % 2 == 0;
        let shown = triggers::set_enabled(home.path(), "mine", wanted)?;
        assert_eq!(
            shown.enabled,
            Some(wanted),
            "the command returned before the requested value was durable"
        );
    }
    let failures = reader.finish();
    assert!(
        failures.is_empty(),
        "a concurrent reader observed a missing or partial trigger during the switch: {failures:?}"
    );

    let after_bytes = fs::read(&path)?;
    let after: Value = serde_json::from_slice(&after_bytes)?;
    assert_eq!(after["enabled"], json!(false));
    for key in ["schema", "source", "workflow", "condition", "api_key"] {
        assert_eq!(
            after[key], before[key],
            "switching `enabled` changed the unrelated field {key}"
        );
    }
    assert_eq!(
        fs::metadata(&path)?.permissions().mode() & 0o777,
        0o640,
        "atomic replacement changed the trigger file's permissions"
    );
    let loaded = triggers::load(home.path(), "mine")?;
    assert!(
        !loaded.enabled,
        "a fresh load did not see the persisted switch"
    );
    assert!(
        loaded.api_key.exposes(SECRET),
        "the switch replaced or erased the secret"
    );
    Ok(())
}

#[test]
fn temp_is_restricted_before_content_and_a_manual_edit_wins_the_compare()
-> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let dir = home.path().join(triggers::TRIGGERS_DIR);
    fs::create_dir_all(&dir)?;
    let path = dir.join("mine.json");
    write_trigger(&path, true, "ship-it")?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    let manual = serde_json::to_vec_pretty(&json!({
        "schema": 1,
        "source": "linear",
        "enabled": true,
        "workflow": "edited-by-hand",
        "condition": "keep this manual edit",
        "api_key": SECRET
    }))?;
    let mut saw_empty_restricted_temp = false;
    let mut saw_complete_temp = false;

    let result = triggers::set_enabled_with(home.path(), "mine", false, |stage, temp| {
        let metadata = fs::metadata(temp)?;
        match stage {
            triggers::ToggleStage::BeforeContent => {
                if metadata.len() != 0 || metadata.permissions().mode() & 0o777 != 0o600 {
                    return Err(std::io::Error::other(format!(
                        "temp had length {} and mode {:o} before content",
                        metadata.len(),
                        metadata.permissions().mode() & 0o777
                    )));
                }
                saw_empty_restricted_temp = true;
            }
            triggers::ToggleStage::BeforeCompare => {
                let staged = fs::read(temp)?;
                if !staged
                    .windows(SECRET.len())
                    .any(|window| window == SECRET.as_bytes())
                {
                    return Err(std::io::Error::other(
                        "the compare seam ran before the complete secret-bearing file was staged",
                    ));
                }
                saw_complete_temp = true;
                fs::write(&path, &manual)?;
            }
        }
        Ok(())
    });

    assert!(
        matches!(result, Err(triggers::TriggerError::ConfigChanged)),
        "a manual edit after the snapshot was overwritten: {result:?}"
    );
    assert!(saw_empty_restricted_temp && saw_complete_temp);
    assert_eq!(
        fs::read(&path)?,
        manual,
        "the conflict refusal did not preserve the manual file byte for byte"
    );
    assert!(
        fs::read_dir(&dir)?
            .filter_map(Result::ok)
            .all(|entry| { !entry.file_name().to_string_lossy().ends_with(".writing") }),
        "the conflict left a secret-bearing temp file behind"
    );
    Ok(())
}

#[test]
fn two_app_toggles_are_serialized_around_the_snapshot() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let dir = home.path().join(triggers::TRIGGERS_DIR);
    fs::create_dir_all(&dir)?;
    write_trigger(&dir.join("mine.json"), true, "ship-it")?;
    let home = Arc::new(home);
    let (inside_tx, inside_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let first_home = Arc::clone(&home);
    let first = thread::spawn(move || {
        triggers::set_enabled_with(first_home.path(), "mine", false, |stage, _| {
            if stage == triggers::ToggleStage::BeforeCompare {
                inside_tx
                    .send(())
                    .map_err(|error| std::io::Error::other(error.to_string()))?;
                release_rx
                    .recv()
                    .map_err(|error| std::io::Error::other(error.to_string()))?;
            }
            Ok(())
        })
    });
    inside_rx.recv()?;

    let (started_tx, started_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let second_home = Arc::clone(&home);
    let second = thread::spawn(move || {
        let _ = started_tx.send(());
        let result = triggers::set_enabled(second_home.path(), "mine", true);
        let _ = done_tx.send(());
        result
    });
    started_rx.recv()?;
    assert!(
        done_rx.recv_timeout(Duration::from_millis(50)).is_err(),
        "the second app toggle read a snapshot while the first still owned it"
    );
    release_tx.send(())?;
    first.join().map_err(|_| "first toggle panicked")??;
    second.join().map_err(|_| "second toggle panicked")??;
    assert!(triggers::load(home.path(), "mine")?.enabled);
    Ok(())
}

fn write_trigger(path: &Path, enabled: bool, workflow: &str) -> Result<(), Box<dyn Error>> {
    fs::write(
        path,
        serde_json::to_vec_pretty(&json!({
            "schema": 1,
            "source": "linear",
            "enabled": enabled,
            "workflow": workflow,
            "condition": "assigned to me",
            "api_key": SECRET
        }))?,
    )?;
    Ok(())
}

#[derive(Debug)]
struct Reader {
    stop: Arc<AtomicBool>,
    failures: Arc<Mutex<Vec<String>>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Reader {
    fn start(path: PathBuf) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let failures = Arc::new(Mutex::new(Vec::new()));
        let stop_in_thread = Arc::clone(&stop);
        let failures_in_thread = Arc::clone(&failures);
        let thread = thread::spawn(move || {
            while !stop_in_thread.load(Ordering::Acquire) {
                match fs::read(&path)
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| {
                        serde_json::from_slice::<Value>(&bytes).map_err(|e| e.to_string())
                    }) {
                    Ok(value) if value.get("enabled").and_then(Value::as_bool).is_some() => {}
                    Ok(value) => failures_in_thread
                        .lock()
                        .expect("reader failures lock")
                        .push(format!("complete JSON without enabled: {value}")),
                    Err(error) => failures_in_thread
                        .lock()
                        .expect("reader failures lock")
                        .push(error),
                }
                thread::yield_now();
            }
        });
        Self {
            stop,
            failures,
            thread: Some(thread),
        }
    }

    fn finish(mut self) -> Vec<String> {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.join().expect("reader thread");
        }
        self.failures.lock().expect("reader failures lock").clone()
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
