//! AC-7 dla T-74: Delete konczy Pending, ale odmawia pracy juz zwiazanej ze Startem.

#![allow(clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use loadout_lib::commands::triggers::{
    self, CleanupStage, DeleteStage, Source, TriggerClaim, TriggerError, TriggerPoll,
    TriggerSnapshot,
};
use serde_json::{Value, json};
use tempfile::TempDir;

const KEY: &str = "lin_api_1234567890123456789012345678901234567890";

type DirectorySnapshot = Vec<(PathBuf, Option<Vec<u8>>)>;

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
            "poll_every_minutes": 1,
            "api_key": KEY,
        }))?,
    )?;
    Ok(())
}

fn snapshot(slug: &str) -> TriggerSnapshot {
    TriggerSnapshot {
        slug: slug.to_owned(),
        source: Source::Linear,
        condition: "assigned-to-me".to_owned(),
        workflow: "linear.json".to_owned(),
        enabled: true,
        poll_every_minutes: 1,
        key_saved: true,
    }
}

fn answer(id: &str, updated_at: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "data": {
            "issues": {
                "nodes": [{
                    "id": id,
                    "identifier": format!("LOAD-{id}"),
                    "title": format!("Issue {id}"),
                    "url": format!("https://linear.app/loadout/issue/{id}"),
                    "description": "body",
                    "updatedAt": updated_at,
                }]
            }
        }
    }))
    .expect("issue response")
}

fn pending(home: &Path, slug: &str) -> Result<TriggerClaim, Box<dyn Error>> {
    write_trigger(home, slug)?;
    assert_eq!(
        triggers::poll_with(home, slug, 10, |_| {
            Ok(answer("old", "2026-08-21T09:00:00.000Z"))
        })?,
        TriggerPoll::Armed
    );
    let poll = triggers::poll_with(home, slug, 20, |_| {
        Ok(answer("new", "2026-08-21T09:01:00.000Z"))
    })?;
    let TriggerPoll::Pending { delivery } = poll else {
        return Err(format!("second poll did not create Pending: {poll:?}").into());
    };
    Ok(delivery.claim)
}

fn ledger(home: &Path, slug: &str) -> Result<Value, Box<dyn Error>> {
    let bytes = fs::read(
        home.join(triggers::TRIGGERS_DIR)
            .join(format!(".{slug}.ledger.json")),
    )?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn assert_cancelled(home: &Path, slug: &str) -> Result<(), Box<dyn Error>> {
    let ledger = ledger(home, slug)?;
    let deliveries = ledger["deliveries"]
        .as_array()
        .ok_or("ledger deliveries are not an array")?;
    assert!(!deliveries.is_empty(), "the delete oracle had no delivery");
    assert!(
        deliveries
            .iter()
            .all(|record| record["state"]["status"] == "cancelled"),
        "Delete hid the config above work which was not durably cancelled: {ledger}"
    );
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
            } else if entry.file_type()?.is_symlink() {
                out.push((
                    relative,
                    Some(fs::read_link(path)?.as_os_str().as_encoded_bytes().to_vec()),
                ));
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

#[test]
fn delete_cancels_pending_but_refuses_bound_before_any_mutation() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let pending_claim = pending(home.path(), "pending")?;
    let bound_claim = pending(home.path(), "bound")?;
    let bound_run = home.path().join("runs/bound/run.json");
    triggers::bind_delivery(home.path(), &bound_claim, &bound_run)?;

    triggers::delete(home.path(), "pending", &snapshot("pending"))?;
    assert_cancelled(home.path(), "pending")?;
    let listed = triggers::list(home.path())?;
    assert!(
        listed.iter().all(|entry| entry.slug != "pending"),
        "success returned while a deleted trigger was still in the real library: {listed:?}"
    );
    assert!(
        triggers::claimed_delivery(home.path(), &pending_claim).is_err(),
        "a cancelled claim still restored a runnable delivery"
    );
    assert!(
        triggers::bind_delivery(
            home.path(),
            &pending_claim,
            &home.path().join("runs/another/run.json")
        )
        .is_err(),
        "a cancelled claim could be bound again"
    );
    assert!(!triggers::tombstone_path(home.path(), "pending").exists());

    let before_bound = tree(home.path())?;
    let refusal = triggers::delete(home.path(), "bound", &snapshot("bound"))
        .expect_err("Delete must not race a delivery already bound to Start");
    assert_eq!(
        refusal.to_string(),
        "A run from this trigger is starting. Wait for it to start, then try deleting again."
    );
    assert_eq!(
        tree(home.path())?,
        before_bound,
        "Bound refusal changed config, ledger, cursor or run state"
    );
    assert_eq!(
        triggers::claimed_delivery(home.path(), &bound_claim)?.claim,
        bound_claim
    );
    triggers::bind_delivery(home.path(), &bound_claim, &bound_run)?;
    assert!(
        triggers::list(home.path())?
            .iter()
            .any(|entry| entry.slug == "bound"),
        "Bound refusal hid the trigger configuration"
    );
    Ok(())
}

#[test]
fn stale_missing_symlink_and_broken_ledger_refuse_without_changing_the_tree()
-> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;

    write_trigger(home.path(), "stale")?;
    let stale_path = home.path().join(triggers::TRIGGERS_DIR).join("stale.json");
    let mut stale_file: Value = serde_json::from_slice(&fs::read(&stale_path)?)?;
    stale_file["poll_every_minutes"] = json!(5);
    fs::write(&stale_path, serde_json::to_vec_pretty(&stale_file)?)?;
    let before_stale = tree(home.path())?;
    assert!(matches!(
        triggers::delete(home.path(), "stale", &snapshot("stale")),
        Err(TriggerError::ConfigChanged)
    ));
    assert_eq!(tree(home.path())?, before_stale);

    let before_missing = tree(home.path())?;
    assert!(matches!(
        triggers::delete(home.path(), "missing", &snapshot("missing")),
        Err(TriggerError::MissingConfig)
    ));
    assert_eq!(tree(home.path())?, before_missing);

    let victim = home.path().join("victim.json");
    fs::write(&victim, b"outside")?;
    symlink(
        &victim,
        home.path().join(triggers::TRIGGERS_DIR).join("linked.json"),
    )?;
    let before_link = tree(home.path())?;
    assert!(matches!(
        triggers::delete(home.path(), "linked", &snapshot("linked")),
        Err(TriggerError::NotRegularConfig)
    ));
    assert_eq!(tree(home.path())?, before_link);
    assert_eq!(fs::read(victim)?, b"outside");

    write_trigger(home.path(), "broken-ledger")?;
    fs::write(
        home.path()
            .join(triggers::TRIGGERS_DIR)
            .join(".broken-ledger.ledger.json"),
        b"{ broken",
    )?;
    let before_broken = tree(home.path())?;
    assert!(matches!(
        triggers::delete(home.path(), "broken-ledger", &snapshot("broken-ledger")),
        Err(TriggerError::InvalidLedger(_))
    ));
    assert_eq!(tree(home.path())?, before_broken);

    write_trigger(home.path(), "future-ledger")?;
    fs::write(
        home.path()
            .join(triggers::TRIGGERS_DIR)
            .join(".future-ledger.ledger.json"),
        serde_json::to_vec_pretty(&json!({
            "schema": 99,
            "armed": false,
            "deliveries": [],
        }))?,
    )?;
    let before_future = tree(home.path())?;
    assert!(matches!(
        triggers::delete(home.path(), "future-ledger", &snapshot("future-ledger")),
        Err(TriggerError::UnsupportedLedgerSchema)
    ));
    assert_eq!(
        tree(home.path())?,
        before_future,
        "Delete mutated a well-formed ledger from an unsupported schema"
    );
    Ok(())
}

#[test]
fn failures_on_both_sides_of_hide_leave_only_the_two_recoverable_states()
-> Result<(), Box<dyn Error>> {
    let visible = TempDir::new()?;
    let visible_claim = pending(visible.path(), "visible")?;
    let visible_result = triggers::delete_with(
        visible.path(),
        "visible",
        &snapshot("visible"),
        |stage, _| {
            if stage == DeleteStage::AfterCancellation {
                return Err(std::io::Error::other("injected before hide"));
            }
            Ok(())
        },
    );
    assert!(visible_result.is_err());
    assert_cancelled(visible.path(), "visible")?;
    assert!(
        triggers::list(visible.path())?
            .iter()
            .any(|entry| entry.slug == "visible"),
        "failure before hide lost the visible config"
    );
    assert!(triggers::claimed_delivery(visible.path(), &visible_claim).is_err());
    assert_eq!(
        triggers::poll_with(visible.path(), "visible", 30, |_| {
            Ok(answer("new", "2026-08-21T09:01:00.000Z"))
        })?,
        TriggerPoll::Armed,
        "the next poll presented a cancelled delivery as Pending"
    );

    let hidden = TempDir::new()?;
    let hidden_claim = pending(hidden.path(), "hidden")?;
    let hidden_result =
        triggers::delete_with(hidden.path(), "hidden", &snapshot("hidden"), |stage, _| {
            if stage == DeleteStage::AfterHide {
                return Err(std::io::Error::other("injected after hide"));
            }
            Ok(())
        });
    assert!(hidden_result.is_err());
    assert_cancelled(hidden.path(), "hidden")?;
    assert!(
        !hidden
            .path()
            .join(triggers::TRIGGERS_DIR)
            .join("hidden.json")
            .exists(),
        "failure after hide restored an active config"
    );
    assert!(
        triggers::tombstone_path(hidden.path(), "hidden").exists(),
        "failure after hide left no durable cleanup marker"
    );
    assert!(triggers::claimed_delivery(hidden.path(), &hidden_claim).is_err());

    let listed = triggers::list(hidden.path())?;
    assert!(listed.iter().all(|entry| entry.slug != "hidden"));
    assert!(
        !triggers::tombstone_path(hidden.path(), "hidden").exists(),
        "the next real list did not finish tombstone cleanup"
    );
    Ok(())
}

#[test]
fn forged_tombstones_never_erase_live_or_external_state() -> Result<(), Box<dyn Error>> {
    let live = TempDir::new()?;
    let claim = pending(live.path(), "live")?;
    fs::write(
        triggers::tombstone_path(live.path(), "live"),
        b"forged cleanup marker",
    )?;
    let before_live = tree(live.path())?;
    let listed = triggers::list(live.path())?;
    assert!(listed.iter().any(|entry| entry.slug == "live"));
    assert_eq!(
        tree(live.path())?,
        before_live,
        "a forged marker next to a live config erased exactly-once state"
    );
    assert_eq!(
        triggers::claimed_delivery(live.path(), &claim)?.claim,
        claim
    );

    let linked = TempDir::new()?;
    fs::create_dir_all(linked.path().join(triggers::TRIGGERS_DIR))?;
    let victim = linked.path().join("victim.json");
    fs::write(&victim, b"outside bytes")?;
    symlink(&victim, triggers::tombstone_path(linked.path(), "linked"))?;
    let before_linked = tree(linked.path())?;
    assert!(triggers::list(linked.path())?.is_empty());
    assert_eq!(tree(linked.path())?, before_linked);
    assert_eq!(fs::read(victim)?, b"outside bytes");

    let broken = TempDir::new()?;
    fs::create_dir_all(broken.path().join(triggers::TRIGGERS_DIR))?;
    fs::write(
        triggers::tombstone_path(broken.path(), "broken"),
        b"former config",
    )?;
    fs::write(
        broken
            .path()
            .join(triggers::TRIGGERS_DIR)
            .join(".broken.ledger.json"),
        b"{ broken",
    )?;
    let before_broken = tree(broken.path())?;
    assert!(matches!(
        triggers::list(broken.path()),
        Err(TriggerError::InvalidLedger(_))
    ));
    assert_eq!(tree(broken.path())?, before_broken);
    Ok(())
}

#[test]
fn cleanup_keeps_its_reader_until_sidecars_are_durable_and_retry_finishes()
-> Result<(), Box<dyn Error>> {
    for fault in [
        CleanupStage::AfterLedger,
        CleanupStage::AfterCursor,
        CleanupStage::BeforeCommit,
        CleanupStage::AfterCommit,
    ] {
        let home = TempDir::new()?;
        let claim = pending(home.path(), "cleanup")?;
        triggers::delete(home.path(), "cleanup", &snapshot("cleanup"))?;
        let dir = home.path().join(triggers::TRIGGERS_DIR);
        let ledger = dir.join(".cleanup.ledger.json");
        let cursor = triggers::cursor_path(home.path(), "cleanup");
        let tombstone = triggers::tombstone_path(home.path(), "cleanup");
        assert!(ledger.exists() && cursor.exists() && tombstone.exists());

        let mut injected = false;
        let result = triggers::list_with_cleanup(home.path(), |stage, _| {
            match stage {
                CleanupStage::WritingCandidate | CleanupStage::BeforeWritingRemoval => {}
                CleanupStage::AfterLedger => {
                    assert!(!ledger.exists());
                    assert!(cursor.exists() && tombstone.exists());
                }
                CleanupStage::AfterCursor | CleanupStage::BeforeCommit => {
                    assert!(!ledger.exists() && !cursor.exists());
                    assert!(tombstone.exists());
                }
                CleanupStage::AfterCommit => {
                    assert!(!ledger.exists() && !cursor.exists() && !tombstone.exists());
                }
            }
            if stage == fault {
                injected = true;
                return Err(std::io::Error::other("injected cleanup crash"));
            }
            Ok(())
        });
        assert!(result.is_err(), "{fault:?} fault was not returned");
        assert!(injected, "{fault:?} seam never ran");
        if fault == CleanupStage::AfterCommit {
            assert!(!tombstone.exists());
        } else {
            assert!(
                tombstone.exists(),
                "{fault:?} removed the only cleanup reader before commit"
            );
        }

        assert!(triggers::list(home.path())?.is_empty());
        assert!(!ledger.exists() && !cursor.exists() && !tombstone.exists());
        assert!(triggers::claimed_delivery(home.path(), &claim).is_err());
    }
    Ok(())
}

#[test]
fn every_delete_return_and_refusal_is_redacted() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    write_trigger(home.path(), "mine")?;
    let result = triggers::delete(home.path(), "mine", &snapshot("mine"));
    let text = format!("{result:?}");
    assert!(!text.contains(KEY), "Delete returned the API key: {text}");
    Ok(())
}

#[test]
fn delete_never_follows_a_linked_trigger_root() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let external = TempDir::new()?;
    write_trigger(external.path(), "outside")?;
    let external_dir = external.path().join(triggers::TRIGGERS_DIR);
    let sentinel = external_dir.join("sentinel.txt");
    fs::write(&sentinel, b"outside bytes")?;
    symlink(&external_dir, home.path().join(triggers::TRIGGERS_DIR))?;
    let before = tree(external.path())?;

    let result = triggers::delete(home.path(), "outside", &snapshot("outside"));
    assert!(result.is_err(), "Delete followed a linked trigger folder");
    assert_eq!(
        tree(external.path())?,
        before,
        "Delete changed state outside Loadout's trigger folder"
    );
    assert_eq!(fs::read(sentinel)?, b"outside bytes");
    Ok(())
}
