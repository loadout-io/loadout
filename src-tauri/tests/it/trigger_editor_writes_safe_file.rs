//! AC-3 dla T-74: formularz tworzy i zmienia prawdziwy plik bez ujawnienia sekretu.

#![allow(clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use loadout_lib::commands::triggers::{
    self, EditorStage, Secret, Source, TriggerDraft, TriggerError, TriggerSnapshot,
};
use serde_json::{Value, json};
use tempfile::TempDir;
use uuid::{Uuid, Version};

const KEY: &str = "lin_api_1234567890123456789012345678901234567890";
const REPLACEMENT: &str = "lin_api_abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMN";
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_trigger_editor",
  "name": "Handle Linear issue",
  "steps": [{
    "kind": "checkpoint",
    "id": "inspect",
    "name": "Inspect the issue",
    "at": { "x": 0, "y": 0 }
  }],
  "links": []
}"#;

fn fixed_id(suffix: u8) -> Uuid {
    Uuid::parse_str(&format!("0198a1f2-3b4c-7d5e-8f60-1122334455{suffix:02x}"))
        .expect("fixed UUID v7")
}

fn slug(id: Uuid) -> String {
    format!("linear-{id}")
}

fn home_with_workflow() -> Result<TempDir, Box<dyn Error>> {
    let home = TempDir::new()?;
    fs::create_dir_all(home.path().join("workflows"))?;
    fs::write(home.path().join("workflows/linear.json"), WORKFLOW)?;
    Ok(home)
}

fn draft(key: Option<&str>, cadence: u32) -> TriggerDraft {
    TriggerDraft {
        source: "linear".to_owned(),
        condition: "assigned-to-me".to_owned(),
        workflow: "linear.json".to_owned(),
        poll_every_minutes: cadence,
        api_key: key.map(Secret::new),
    }
}

fn snapshot(slug: &str, cadence: u32) -> TriggerSnapshot {
    TriggerSnapshot {
        slug: slug.to_owned(),
        source: Source::Linear,
        condition: "assigned-to-me".to_owned(),
        workflow: "linear.json".to_owned(),
        enabled: true,
        poll_every_minutes: cadence,
        key_saved: true,
    }
}

fn assert_redacted(text: &str) {
    for secret in [KEY, REPLACEMENT] {
        assert!(
            !text.contains(secret),
            "a redacted return, refusal or Debug string exposed {secret:?}: {text}"
        );
    }
}

#[test]
fn create_mints_a_private_complete_redacted_file() -> Result<(), Box<dyn Error>> {
    let home = home_with_workflow()?;
    let id = fixed_id(1);
    let expected_slug = slug(id);
    let mut saw_private_empty_file = false;

    let entry = triggers::create_with(
        home.path(),
        draft(Some(KEY), 5),
        || id,
        |stage, path| {
            if stage == EditorStage::BeforeContent {
                let metadata = fs::metadata(path)?;
                if metadata.len() != 0 || metadata.permissions().mode() & 0o777 != 0o600 {
                    return Err(std::io::Error::other(format!(
                        "secret temp had length {} and mode {:o} before content",
                        metadata.len(),
                        metadata.permissions().mode() & 0o777
                    )));
                }
                saw_private_empty_file = true;
            }
            Ok(())
        },
    )?;

    assert!(
        saw_private_empty_file,
        "the 0600-before-content seam never ran"
    );
    assert_eq!(entry.slug, expected_slug);
    let minted = entry
        .slug
        .strip_prefix("linear-")
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .ok_or("Rust did not mint a linear-<uuid> slug")?;
    assert_eq!(minted.get_version(), Some(Version::SortRand));
    assert_eq!(entry.source, Some(Source::Linear));
    assert_eq!(entry.condition.as_deref(), Some("assigned-to-me"));
    assert_eq!(entry.workflow.as_deref(), Some("linear.json"));
    assert_eq!(entry.enabled, Some(true));
    assert_eq!(entry.poll_every_minutes, Some(5));
    assert_eq!(entry.key_saved, Some(true));

    let path = home
        .path()
        .join(triggers::TRIGGERS_DIR)
        .join(format!("{expected_slug}.json"));
    let bytes = fs::read(&path)?;
    let file: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(
        file,
        json!({
            "schema": 1,
            "source": "linear",
            "enabled": true,
            "workflow": "linear.json",
            "condition": "assigned-to-me",
            "poll_every_minutes": 5,
            "api_key": KEY,
        }),
        "Create returned before a complete canonical config was durable"
    );
    assert_eq!(fs::metadata(path)?.permissions().mode() & 0o777, 0o600);
    assert_redacted(&format!("{} {:?}", serde_json::to_string(&entry)?, entry));
    assert_redacted(&format!("{:?}", draft(Some(KEY), 5)));
    Ok(())
}

#[test]
fn create_is_no_clobber_and_invalid_forms_touch_no_config() -> Result<(), Box<dyn Error>> {
    let home = home_with_workflow()?;
    let dir = home.path().join(triggers::TRIGGERS_DIR);
    fs::create_dir_all(&dir)?;
    let collision = fixed_id(2);
    let collision_path = dir.join(format!("{}.json", slug(collision)));
    let original = b"manual bytes must win";
    fs::write(&collision_path, original)?;
    let collision_result = triggers::create_with(
        home.path(),
        draft(Some(KEY), 1),
        || collision,
        |_, _| Ok(()),
    );
    assert!(
        matches!(collision_result, Err(TriggerError::AlreadyExists)),
        "Create did not refuse an occupied minted name: {collision_result:?}"
    );
    assert_eq!(fs::read(&collision_path)?, original);

    let racing = fixed_id(8);
    let racing_path = dir.join(format!("{}.json", slug(racing)));
    let manual_winner = b"manual writer won before publish";
    let racing_result = triggers::create_with(
        home.path(),
        draft(Some(KEY), 1),
        || racing,
        |stage, _| {
            if stage == EditorStage::BeforeCompare {
                fs::write(&racing_path, manual_winner)?;
            }
            Ok(())
        },
    );
    assert!(
        matches!(racing_result, Err(TriggerError::AlreadyExists)),
        "Create overwrote a name occupied immediately before publish: {racing_result:?}"
    );
    assert_eq!(fs::read(&racing_path)?, manual_winner);

    fs::write(home.path().join("victim.json"), WORKFLOW)?;

    let cases = [
        (draft(Some(KEY), 2), fixed_id(3), "cadence"),
        (
            TriggerDraft {
                source: REPLACEMENT.to_owned(),
                ..draft(Some(KEY), 1)
            },
            fixed_id(4),
            "source",
        ),
        (
            TriggerDraft {
                condition: "anything changed".to_owned(),
                ..draft(Some(KEY), 1)
            },
            fixed_id(5),
            "condition",
        ),
        (
            TriggerDraft {
                condition: "assigned to me".to_owned(),
                ..draft(Some(KEY), 1)
            },
            fixed_id(11),
            "legacy condition in a new form",
        ),
        (draft(Some("lin_api_too_short"), 1), fixed_id(6), "key"),
        (
            TriggerDraft {
                workflow: "missing.json".to_owned(),
                ..draft(Some(KEY), 1)
            },
            fixed_id(7),
            "workflow",
        ),
        (draft(None, 1), fixed_id(9), "missing key"),
        (
            TriggerDraft {
                workflow: "../victim.json".to_owned(),
                ..draft(Some(KEY), 1)
            },
            fixed_id(10),
            "workflow outside the library",
        ),
    ];
    for (invalid, id, label) in cases {
        let path = dir.join(format!("{}.json", slug(id)));
        assert_redacted(&format!("{invalid:?}"));
        let result = triggers::create_with(home.path(), invalid, || id, |_, _| Ok(()));
        let error = result.expect_err(label);
        assert!(!path.exists(), "invalid {label} left a config file behind");
        if label == "missing key" {
            let sentence = error.to_string();
            assert!(sentence.contains("Enter a Linear API key"));
            assert!(!sentence.contains("api_key") && !sentence.contains("trigger file"));
        }
        assert_redacted(&format!("{error} {error:?}"));
    }
    Ok(())
}

#[test]
fn old_files_default_to_one_and_all_four_cadences_round_trip() -> Result<(), Box<dyn Error>> {
    let home = home_with_workflow()?;
    let dir = home.path().join(triggers::TRIGGERS_DIR);
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join("legacy.json"),
        serde_json::to_vec_pretty(&json!({
            "schema": 1,
            "source": "linear",
            "enabled": true,
            "workflow": "linear.json",
            "condition": "assigned to me",
            "api_key": KEY,
        }))?,
    )?;
    let legacy = triggers::load(home.path(), "legacy")?;
    assert_eq!(
        legacy.poll_every_minutes,
        triggers::DEFAULT_POLL_EVERY_MINUTES,
        "a pre-T-74 file did not keep the old one-minute behavior"
    );
    assert_eq!(legacy.condition, "assigned-to-me");
    let legacy_entry = triggers::list(home.path())?
        .into_iter()
        .find(|entry| entry.slug == "legacy")
        .ok_or("the pre-T-74 trigger vanished from the library")?;
    assert_eq!(legacy_entry.problem, None);
    assert_eq!(legacy_entry.condition.as_deref(), Some("assigned-to-me"));
    assert_eq!(legacy_entry.poll_every_minutes, Some(1));

    for (slug, schema, condition, cadence) in [
        ("dishonest-schema", 99, "assigned-to-me", 1),
        ("dishonest-condition", 1, "anything changed", 1),
        ("dishonest-cadence", 1, "assigned-to-me", 2),
    ] {
        fs::write(
            dir.join(format!("{slug}.json")),
            serde_json::to_vec_pretty(&json!({
                "schema": schema,
                "source": "linear",
                "enabled": true,
                "workflow": "linear.json",
                "condition": condition,
                "poll_every_minutes": cadence,
                "api_key": KEY,
            }))?,
        )?;
        let entry = triggers::list(home.path())?
            .into_iter()
            .find(|entry| entry.slug == slug)
            .ok_or("an invalid manual config vanished from the real library")?;
        assert!(entry.problem.is_some());
        assert!(
            entry.source.is_none()
                && entry.condition.is_none()
                && entry.workflow.is_none()
                && entry.enabled.is_none()
                && entry.poll_every_minutes.is_none(),
            "an invalid manual config was presented as runnable: {entry:?}"
        );
        let mut fetched = false;
        assert!(
            triggers::poll_with(home.path(), slug, 1, |_| {
                fetched = true;
                Ok(Vec::new())
            })
            .is_err()
        );
        assert!(!fetched, "an invalid manual config reached Linear");
    }

    for (index, cadence) in [1, 5, 15, 60].into_iter().enumerate() {
        let id = fixed_id(20 + u8::try_from(index)?);
        let entry =
            triggers::create_with(home.path(), draft(Some(KEY), cadence), || id, |_, _| Ok(()))?;
        assert_eq!(entry.poll_every_minutes, Some(cadence));
        assert_eq!(
            triggers::load(home.path(), &entry.slug)?.poll_every_minutes,
            cadence
        );
    }
    Ok(())
}

#[test]
fn edit_preserves_or_replaces_the_key_and_refuses_invalid_drafts() -> Result<(), Box<dyn Error>> {
    let home = home_with_workflow()?;
    let id = fixed_id(40);
    let entry = triggers::create_with(home.path(), draft(Some(KEY), 1), || id, |_, _| Ok(()))?;
    let path = home
        .path()
        .join(triggers::TRIGGERS_DIR)
        .join(format!("{}.json", entry.slug));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640))?;

    let saved = triggers::update(
        home.path(),
        &entry.slug,
        &snapshot(&entry.slug, 1),
        draft(None, 5),
    )?;
    assert_eq!(saved.poll_every_minutes, Some(5));
    assert!(
        triggers::load(home.path(), &entry.slug)?
            .api_key
            .exposes(KEY)
    );
    assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o640);

    let replaced = triggers::update(
        home.path(),
        &entry.slug,
        &snapshot(&entry.slug, 5),
        draft(Some(REPLACEMENT), 15),
    )?;
    assert_eq!(replaced.poll_every_minutes, Some(15));
    assert!(
        triggers::load(home.path(), &entry.slug)?
            .api_key
            .exposes(REPLACEMENT)
    );
    let before_invalid = fs::read(&path)?;
    let invalid_updates = [
        TriggerDraft {
            source: "jira".to_owned(),
            ..draft(None, 15)
        },
        TriggerDraft {
            condition: "anything changed".to_owned(),
            ..draft(None, 15)
        },
        TriggerDraft {
            condition: "assigned to me".to_owned(),
            ..draft(None, 15)
        },
        draft(None, 2),
        TriggerDraft {
            workflow: "missing.json".to_owned(),
            ..draft(None, 15)
        },
        draft(Some("lin_api_too_short"), 15),
    ];
    for invalid in invalid_updates {
        let result = triggers::update(
            home.path(),
            &entry.slug,
            &snapshot(&entry.slug, 15),
            invalid,
        );
        assert!(result.is_err(), "an invalid Edit was accepted: {result:?}");
        assert_eq!(
            fs::read(&path)?,
            before_invalid,
            "an invalid Edit changed the config"
        );
    }
    assert_redacted(&format!("{saved:?} {replaced:?}"));
    Ok(())
}

#[test]
fn edit_preserves_a_fresh_manual_key_and_refuses_schema_change() -> Result<(), Box<dyn Error>> {
    let home = home_with_workflow()?;
    let id = fixed_id(41);
    let entry = triggers::create_with(
        home.path(),
        draft(Some(REPLACEMENT), 15),
        || id,
        |_, _| Ok(()),
    )?;
    let path = home
        .path()
        .join(triggers::TRIGGERS_DIR)
        .join(format!("{}.json", entry.slug));
    let before_invalid = fs::read(&path)?;
    let mut manual_schema: Value = serde_json::from_slice(&before_invalid)?;
    manual_schema["schema"] = json!(99);
    let manual_schema = serde_json::to_vec_pretty(&manual_schema)?;
    fs::write(&path, &manual_schema)?;
    let schema_conflict = triggers::update(
        home.path(),
        &entry.slug,
        &snapshot(&entry.slug, 15),
        draft(None, 5),
    );
    assert!(matches!(
        schema_conflict,
        Err(TriggerError::UnsupportedSchema)
    ));
    assert_eq!(
        fs::read(&path)?,
        manual_schema,
        "Edit silently accepted a manual format-version change"
    );
    fs::write(&path, &before_invalid)?;

    let key_only = serde_json::to_vec_pretty(&json!({
        "schema": 1,
        "source": "linear",
        "enabled": true,
        "workflow": "linear.json",
        "condition": "assigned-to-me",
        "poll_every_minutes": 15,
        "api_key": KEY,
    }))?;
    fs::write(&path, key_only)?;
    let preserved_manual_key = triggers::update(
        home.path(),
        &entry.slug,
        &snapshot(&entry.slug, 15),
        draft(None, 5),
    )?;
    assert_eq!(preserved_manual_key.poll_every_minutes, Some(5));
    assert!(
        triggers::load(home.path(), &entry.slug)?
            .api_key
            .exposes(KEY),
        "an empty Edit clobbered the key changed by hand after the redacted snapshot"
    );
    assert_redacted(&format!("{preserved_manual_key:?}"));
    Ok(())
}

#[test]
fn edit_refuses_a_stale_snapshot_symlink_and_directory() -> Result<(), Box<dyn Error>> {
    let home = home_with_workflow()?;
    let id = fixed_id(42);
    let entry = triggers::create_with(
        home.path(),
        draft(Some(REPLACEMENT), 5),
        || id,
        |_, _| Ok(()),
    )?;
    let path = home
        .path()
        .join(triggers::TRIGGERS_DIR)
        .join(format!("{}.json", entry.slug));
    let manual = serde_json::to_vec_pretty(&json!({
        "schema": 1,
        "source": "linear",
        "enabled": true,
        "workflow": "linear.json",
        "condition": "assigned-to-me",
        "poll_every_minutes": 60,
        "api_key": KEY,
    }))?;
    fs::write(&path, &manual)?;
    let conflict = triggers::update(
        home.path(),
        &entry.slug,
        &snapshot(&entry.slug, 5),
        draft(None, 15),
    );
    assert!(matches!(conflict, Err(TriggerError::ConfigChanged)));
    assert_eq!(
        fs::read(&path)?,
        manual,
        "a stale edit overwrote manual bytes"
    );

    let victim = home.path().join("victim.json");
    fs::write(&victim, b"outside")?;
    let linked_slug = "linked";
    symlink(
        &victim,
        home.path()
            .join(triggers::TRIGGERS_DIR)
            .join(format!("{linked_slug}.json")),
    )?;
    let linked = triggers::update(
        home.path(),
        linked_slug,
        &snapshot(linked_slug, 1),
        draft(None, 5),
    );
    assert!(matches!(linked, Err(TriggerError::NotRegularConfig)));
    assert_eq!(fs::read(&victim)?, b"outside");

    let directory_slug = "directory";
    fs::create_dir(
        home.path()
            .join(triggers::TRIGGERS_DIR)
            .join(format!("{directory_slug}.json")),
    )?;
    let directory = triggers::update(
        home.path(),
        directory_slug,
        &snapshot(directory_slug, 1),
        draft(None, 5),
    );
    assert!(matches!(directory, Err(TriggerError::NotRegularConfig)));
    Ok(())
}

#[test]
fn conflict_seam_keeps_a_manual_change_byte_for_byte() -> Result<(), Box<dyn Error>> {
    let home = home_with_workflow()?;
    let id = fixed_id(50);
    let entry = triggers::create_with(home.path(), draft(Some(KEY), 1), || id, |_, _| Ok(()))?;
    let path = home
        .path()
        .join(triggers::TRIGGERS_DIR)
        .join(format!("{}.json", entry.slug));
    let manual = b"manual writer won this race".to_vec();
    let result = triggers::update_with(
        home.path(),
        &entry.slug,
        &snapshot(&entry.slug, 1),
        draft(None, 5),
        |stage, _| {
            if stage == EditorStage::BeforeCompare {
                fs::write(&path, &manual)?;
            }
            Ok(())
        },
    );
    assert!(matches!(result, Err(TriggerError::ConfigChanged)));
    assert_eq!(fs::read(path)?, manual);
    Ok(())
}

#[test]
fn trigger_root_links_are_refused_and_crash_temps_have_one_safe_reader()
-> Result<(), Box<dyn Error>> {
    let linked_home = home_with_workflow()?;
    let external = TempDir::new()?;
    let victim = external.path().join("victim.txt");
    fs::write(&victim, b"external bytes")?;
    symlink(
        external.path(),
        linked_home.path().join(triggers::TRIGGERS_DIR),
    )?;
    let result = triggers::create_with(
        linked_home.path(),
        draft(Some(KEY), 1),
        || fixed_id(60),
        |_, _| Ok(()),
    );
    assert!(result.is_err(), "Create followed a linked trigger folder");
    assert_eq!(fs::read(&victim)?, b"external bytes");
    assert_eq!(
        fs::read_dir(external.path())?.count(),
        1,
        "Create wrote a secret-bearing file outside Loadout's trigger folder"
    );

    let home = home_with_workflow()?;
    let dir = home.path().join(triggers::TRIGGERS_DIR);
    fs::create_dir_all(&dir)?;
    let live = dir.join("mine.json");
    let live_bytes = serde_json::to_vec_pretty(&json!({
        "schema": 1,
        "source": "linear",
        "enabled": true,
        "workflow": "linear.json",
        "condition": "assigned-to-me",
        "poll_every_minutes": 1,
        "api_key": KEY,
    }))?;
    fs::write(&live, &live_bytes)?;
    let create_leftover = dir.join(format!(".{}-abcdef.writing", slug(fixed_id(61))));
    let update_leftover = dir.join(".mine.json-123-456.writing");
    let ledger_leftover = dir.join(".mine.ledger.json-123-457.writing");
    for leftover in [&create_leftover, &update_leftover, &ledger_leftover] {
        fs::write(leftover, KEY)?;
    }
    let unrelated = dir.join(".notes.writing");
    fs::write(&unrelated, b"human bytes")?;
    let outside = home.path().join("outside.txt");
    fs::write(&outside, b"outside bytes")?;
    let linked_leftover = dir.join(".linked.json-123-456.writing");
    symlink(&outside, &linked_leftover)?;
    let directory_leftover = dir.join(".directory.json-123-456.writing");
    fs::create_dir(&directory_leftover)?;

    let listed = triggers::list(home.path())?;
    assert!(listed.iter().any(|entry| entry.slug == "mine"));
    for leftover in [&create_leftover, &update_leftover, &ledger_leftover] {
        assert!(
            !leftover.exists(),
            "a recognized crash temp containing a key had no reader: {}",
            leftover.display()
        );
    }
    assert_eq!(
        fs::read(&live)?,
        live_bytes,
        "cleanup changed the live config"
    );
    assert_eq!(fs::read(&unrelated)?, b"human bytes");
    assert!(
        fs::symlink_metadata(&linked_leftover)?
            .file_type()
            .is_symlink(),
        "cleanup followed or removed a linked temp"
    );
    assert_eq!(fs::read(&outside)?, b"outside bytes");
    assert!(directory_leftover.is_dir());
    Ok(())
}

#[test]
fn recovery_waits_for_the_slug_that_owns_an_active_ledger_temp() -> Result<(), Box<dyn Error>> {
    let home = home_with_workflow()?;
    let entry = triggers::create_with(
        home.path(),
        draft(Some(KEY), 1),
        || fixed_id(62),
        |_, _| Ok(()),
    )?;
    let leftover = home
        .path()
        .join(triggers::TRIGGERS_DIR)
        .join(format!(".{}.ledger.json-123-999.writing", entry.slug));
    fs::write(&leftover, KEY)?;

    let (fetch_entered_tx, fetch_entered_rx) = mpsc::channel();
    let (release_fetch_tx, release_fetch_rx) = mpsc::channel();
    let poll_home = home.path().to_path_buf();
    let poll_slug = entry.slug.clone();
    let poll = thread::spawn(move || {
        triggers::poll_with(&poll_home, &poll_slug, 10, |_| {
            fetch_entered_tx
                .send(())
                .expect("cleanup oracle still receives");
            release_fetch_rx
                .recv()
                .expect("cleanup oracle releases fetch");
            Ok(br#"{"data":{"issues":{"nodes":[]}}}"#.to_vec())
        })
    });
    fetch_entered_rx.recv_timeout(Duration::from_secs(1))?;

    let (candidate_tx, candidate_rx) = mpsc::channel();
    let (removal_tx, removal_rx) = mpsc::channel();
    let list_home = home.path().to_path_buf();
    let expected_temp = leftover.clone();
    let listing = thread::spawn(move || {
        triggers::list_with_cleanup(&list_home, |stage, path| {
            if path == expected_temp {
                match stage {
                    triggers::CleanupStage::WritingCandidate => {
                        candidate_tx.send(()).map_err(std::io::Error::other)?;
                    }
                    triggers::CleanupStage::BeforeWritingRemoval => {
                        removal_tx.send(()).map_err(std::io::Error::other)?;
                    }
                    _ => {}
                }
            }
            Ok(())
        })
    });
    candidate_rx.recv_timeout(Duration::from_secs(1))?;
    assert!(leftover.exists());
    assert!(
        matches!(removal_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "list crossed the slug lock while its ledger temp was active"
    );

    release_fetch_tx.send(())?;
    poll.join().expect("poll thread")?;
    removal_rx.recv_timeout(Duration::from_secs(1))?;
    listing.join().expect("list thread")?;
    assert!(!leftover.exists());
    Ok(())
}

#[test]
fn recovery_accepts_a_ledger_temp_published_after_candidate_discovery() -> Result<(), Box<dyn Error>>
{
    let home = home_with_workflow()?;
    let entry = triggers::create_with(
        home.path(),
        draft(Some(KEY), 1),
        || fixed_id(63),
        |_, _| Ok(()),
    )?;
    let dir = home.path().join(triggers::TRIGGERS_DIR);
    let temporary = dir.join(format!(".{}.ledger.json-123-1000.writing", entry.slug));
    let published = dir.join(format!(".{}.ledger.json", entry.slug));
    let ledger = serde_json::to_vec_pretty(&json!({
        "schema": 1,
        "armed": false,
        "deliveries": [],
    }))?;
    fs::write(&temporary, &ledger)?;
    let mut crossed_publish_window = false;

    let listed = triggers::list_with_cleanup(home.path(), |stage, path| {
        if stage == triggers::CleanupStage::WritingCandidate && path == temporary {
            fs::rename(path, &published)?;
            crossed_publish_window = true;
        }
        Ok(())
    });
    assert!(
        listed.is_ok(),
        "a normal ledger publish made list_triggers refuse: {listed:?}"
    );
    assert!(crossed_publish_window, "the publish seam never ran");
    assert!(!temporary.exists());
    assert_eq!(fs::read(published)?, ledger);
    Ok(())
}
