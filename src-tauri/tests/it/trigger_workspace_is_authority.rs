//! Regresja incydentu 2026-08-21: trigger zapisany globalnie nie może pobrać aktywnego
//! workspace z okna w chwili Startu. Konfiguracja wybiera workspace, ledger go zamraża,
//! a Rust porównuje go przed zajęciem uchwytu biegu.

#![allow(clippy::expect_used)]

use std::cell::Cell;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use loadout_lib::commands::Drivers;
use loadout_lib::commands::triggers::{
    self, Secret, TriggerDelivery, TriggerDraft, TriggerEntry, TriggerPoll, TriggerSnapshot,
};
use loadout_lib::commands::workspaces;
use loadout_lib::engine::drivers::AgentDriver;
use loadout_lib::engine::drivers::absent::Absent;
use loadout_lib::ipc::{AppState, line_channel, run_workflow_from_window};
use loadout_lib::store::Store;
use serde_json::json;
use tempfile::TempDir;
use uuid::Uuid;

const KEY: &str = "lin_api_1234567890123456789012345678901234567890";
const CREATED: i64 = 1_777_777_777_000;
type TriggerTree = Vec<(PathBuf, Vec<u8>)>;
type FullTree = Vec<(PathBuf, Option<Vec<u8>>)>;
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_trigger_workspace",
  "name": "Workspace witness",
  "steps": [{
    "kind": "checkpoint",
    "id": "inspect",
    "name": "Inspect",
    "at": { "x": 0, "y": 0 }
  }],
  "links": []
}"#;

struct Bench {
    home: TempDir,
    workspace_a: PathBuf,
    workspace_b: PathBuf,
    entry: TriggerEntry,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let workspace_a = home.path().join("workspace-a");
        let workspace_b = home.path().join("workspace-b");
        fs::create_dir_all(workspace_a.join(".loadout"))?;
        fs::create_dir_all(&workspace_b)?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::write(home.path().join("workflows/ship.json"), WORKFLOW)?;
        workspaces::save_workspace_inner(home.path(), "Workspace A", path_text(&workspace_a)?)?;
        workspaces::save_workspace_inner(home.path(), "Workspace B", path_text(&workspace_b)?)?;
        let entry = triggers::create_with(
            home.path(),
            draft(&workspace_a),
            || fixed_id(1),
            |_, _| Ok(()),
        )?;
        Ok(Self {
            home,
            workspace_a,
            workspace_b,
            entry,
        })
    }

    fn slug(&self) -> &str {
        &self.entry.slug
    }

    fn delivery(&self) -> Result<TriggerDelivery, Box<dyn Error>> {
        assert_eq!(
            poll(self, "old", CREATED)?,
            TriggerPoll::Armed,
            "fixture did not arm before introducing new work"
        );
        let next = poll(self, "new", CREATED + 1)?;
        let TriggerPoll::Pending { delivery } = next else {
            return Err(format!("new issue did not become Pending: {next:?}").into());
        };
        Ok(*delivery)
    }

    fn edit_to_b(&mut self) -> Result<(), Box<dyn Error>> {
        let expected = snapshot(&self.entry)?;
        let slug = self.entry.slug.clone();
        self.entry =
            triggers::update(self.home.path(), &slug, &expected, draft(&self.workspace_b))?;
        Ok(())
    }

    fn state(&self) -> Result<AppState, Box<dyn Error>> {
        self.state_in(&self.workspace_a)
    }

    fn state_in(&self, project: &Path) -> Result<AppState, Box<dyn Error>> {
        fs::create_dir_all(project.join(".loadout"))?;
        let store = Store::open(&project.join(".loadout/loadout.db"))?;
        Ok(AppState::new(
            self.home.path().to_path_buf(),
            project.to_path_buf(),
            store,
            no_agents_needed(),
        ))
    }
}

#[test]
fn create_and_update_accept_only_a_registered_live_workspace() -> Result<(), Box<dyn Error>> {
    let mut bench = Bench::new()?;
    let saved = triggers::load(bench.home.path(), bench.slug())?;
    assert_eq!(
        saved.workspace.as_deref(),
        Some(path_text(&bench.workspace_a)?)
    );

    let missing = bench.home.path().join("not-there");
    let unregistered = bench.home.path().join("unregistered");
    fs::create_dir_all(&unregistered)?;
    let invalid = [
        (PathBuf::new(), "empty workspace"),
        (PathBuf::from("relative/project"), "relative workspace"),
        (unregistered.clone(), "unregistered workspace"),
        (missing.clone(), "missing workspace folder"),
    ];
    for (index, (workspace, label)) in invalid.iter().enumerate() {
        let suffix = 20 + u8::try_from(index)?;
        let create_home = TempDir::new()?;
        fs::create_dir_all(create_home.path().join("triggers"))?;
        fs::create_dir_all(create_home.path().join("workflows"))?;
        fs::write(create_home.path().join("workflows/ship.json"), WORKFLOW)?;
        workspaces::save_workspace_inner(
            create_home.path(),
            "Workspace A",
            path_text(&bench.workspace_a)?,
        )?;
        let before = trigger_tree(create_home.path())?;
        let refused = triggers::create_with(
            create_home.path(),
            draft(workspace),
            || fixed_id(suffix),
            |_, _| Ok(()),
        );
        assert!(refused.is_err(), "Create accepted {label}");
        assert_eq!(
            trigger_tree(create_home.path())?,
            before,
            "refusing {label} changed the trigger directory"
        );
    }

    for (workspace, label) in invalid {
        let before = fs::read(
            bench
                .home
                .path()
                .join("triggers")
                .join(format!("{}.json", bench.slug())),
        )?;
        let refused = triggers::update(
            bench.home.path(),
            bench.slug(),
            &snapshot(&bench.entry)?,
            draft(&workspace),
        );
        assert!(refused.is_err(), "Edit accepted {label}");
        assert_eq!(
            fs::read(
                bench
                    .home
                    .path()
                    .join("triggers")
                    .join(format!("{}.json", bench.slug()))
            )?,
            before,
            "refusing {label} changed the live config"
        );
    }

    bench.edit_to_b()?;
    assert_eq!(
        triggers::load(bench.home.path(), bench.slug())?
            .workspace
            .as_deref(),
        Some(path_text(&bench.workspace_b)?)
    );
    Ok(())
}

#[test]
fn legacy_trigger_refuses_before_fetch_cursor_or_ledger_write() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let workspace = home.path().join("workspace");
    fs::create_dir_all(&workspace)?;
    workspaces::save_workspace_inner(home.path(), "Workspace", path_text(&workspace)?)?;
    fs::create_dir_all(home.path().join("triggers"))?;
    fs::write(
        home.path().join("triggers/legacy.json"),
        serde_json::to_vec_pretty(&json!({
            "schema": 1,
            "source": "linear",
            "enabled": true,
            "workflow": "ship.json",
            "condition": "assigned-to-me",
            "poll_every_minutes": 1,
            "api_key": KEY
        }))?,
    )?;
    let before = trigger_tree(home.path())?;
    let fetched = Cell::new(0_u8);

    let refused = triggers::poll_with(home.path(), "legacy", CREATED, |_| {
        fetched.set(fetched.get() + 1);
        Ok(answer("must-not-fetch"))
    });

    assert!(
        refused.is_err(),
        "legacy trigger guessed a workspace: {refused:?}"
    );
    assert_eq!(fetched.get(), 0, "legacy trigger reached Linear");
    assert_eq!(
        trigger_tree(home.path())?,
        before,
        "legacy refusal wrote trigger state"
    );
    let listed = triggers::list(home.path())?;
    assert_eq!(
        listed[0].workspace, None,
        "legacy trigger disappeared instead of remaining editable"
    );
    Ok(())
}

#[tokio::test]
async fn legacy_ledger_claim_can_be_read_but_cannot_start_or_retry() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let delivery = bench.delivery()?;
    let ledger_file = bench
        .home
        .path()
        .join("triggers")
        .join(format!(".{}.ledger.json", bench.slug()));
    let mut ledger: serde_json::Value = serde_json::from_slice(&fs::read(&ledger_file)?)?;
    ledger["deliveries"][0]["delivery"]["claim"]
        .as_object_mut()
        .ok_or("legacy claim is not an object")?
        .remove("workspace");
    fs::write(&ledger_file, serde_json::to_vec_pretty(&ledger)?)?;
    let mut legacy_claim = delivery.claim;
    legacy_claim.workspace = None;
    let before = trigger_tree(bench.home.path())?;
    let state = bench.state()?;

    let start = state.begin_triggered_run(&bench.workspace_a, &legacy_claim);
    let retry = triggers::retry(bench.home.path(), bench.slug(), CREATED + 2);

    assert!(
        start.is_err(),
        "legacy claim started by guessing workspace A"
    );
    assert!(
        retry.is_err(),
        "legacy claim was retried into a guessed workspace"
    );
    assert_eq!(
        trigger_tree(bench.home.path())?,
        before,
        "legacy claim refusal rewrote its ledger"
    );
    let free = state.begin_run(&bench.workspace_a);
    assert!(
        free.is_ok(),
        "legacy claim refusal took the live latch: {free:?}"
    );
    Ok(())
}

#[test]
fn explicit_edit_repairs_only_pending_legacy_claim_without_fetching_or_reminting()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let delivery = bench.delivery()?;
    let config_file = bench
        .home
        .path()
        .join("triggers")
        .join(format!("{}.json", bench.slug()));
    let ledger_file = bench
        .home
        .path()
        .join("triggers")
        .join(format!(".{}.ledger.json", bench.slug()));
    let mut config: serde_json::Value = serde_json::from_slice(&fs::read(&config_file)?)?;
    config
        .as_object_mut()
        .ok_or("legacy config is not an object")?
        .remove("workspace");
    fs::write(&config_file, serde_json::to_vec_pretty(&config)?)?;
    let mut ledger: serde_json::Value = serde_json::from_slice(&fs::read(&ledger_file)?)?;
    ledger["deliveries"][0]["delivery"]["claim"]
        .as_object_mut()
        .ok_or("legacy claim is not an object")?
        .remove("workspace");
    fs::write(&ledger_file, serde_json::to_vec_pretty(&ledger)?)?;

    let mut expected = snapshot(&bench.entry)?;
    expected.workspace = None;
    triggers::update(
        bench.home.path(),
        bench.slug(),
        &expected,
        draft(&bench.workspace_a),
    )?;
    let before = trigger_tree(bench.home.path())?;
    let ledger_name = PathBuf::from(format!(".{}.ledger.json", bench.slug()));
    let fetched = Cell::new(0_u8);
    let repaired = triggers::poll_with(bench.home.path(), bench.slug(), CREATED + 2, |_| {
        fetched.set(fetched.get() + 1);
        Ok(answer("must-not-fetch"))
    })?;
    let TriggerPoll::Pending { delivery: repaired } = repaired else {
        return Err(format!("explicit edit did not repair Pending: {repaired:?}").into());
    };

    assert_eq!(
        fetched.get(),
        0,
        "repair fetched Linear before returning saved work"
    );
    assert_eq!(repaired.claim.delivery_id, delivery.claim.delivery_id);
    assert_eq!(repaired.claim.run_id, delivery.claim.run_id);
    assert_eq!(repaired.claim.workflow, delivery.claim.workflow);
    assert_eq!(repaired.issue, delivery.issue);
    assert_eq!(
        repaired.claim.workspace.as_deref(),
        Some(path_text(&bench.workspace_a)?),
        "explicitly saved workspace A did not repair legacy Pending"
    );
    let after = trigger_tree(bench.home.path())?;
    assert_eq!(
        without_file(&after, &ledger_name),
        without_file(&before, &ledger_name),
        "repair changed config, cursor or another trigger file"
    );
    assert_ne!(
        file_bytes(&after, &ledger_name),
        file_bytes(&before, &ledger_name),
        "repair returned workspace A without durably fixing the ledger"
    );
    Ok(())
}

#[test]
fn explicit_save_hydrates_only_pending_in_a_mixed_legacy_ledger() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let pending = bench.delivery()?;
    let _ = poll(&bench, "bound", CREATED + 2)?;
    let bound = ledger_deliveries(&ledger_json(&bench)?)?
        .last()
        .cloned()
        .ok_or("bound delivery was not written")?;
    let _ = poll(&bench, "accepted", CREATED + 3)?;
    let accepted = ledger_deliveries(&ledger_json(&bench)?)?
        .last()
        .cloned()
        .ok_or("accepted delivery was not written")?;
    let bound_file = bench.workspace_a.join(".loadout/runs/bound/run.json");
    triggers::bind_delivery(bench.home.path(), &bound.claim, &bound_file)?;
    accept(&bench, &accepted)?;

    let config_file = bench
        .home
        .path()
        .join("triggers")
        .join(format!("{}.json", bench.slug()));
    let ledger_file = bench
        .home
        .path()
        .join("triggers")
        .join(format!(".{}.ledger.json", bench.slug()));
    let mut config: serde_json::Value = serde_json::from_slice(&fs::read(&config_file)?)?;
    config
        .as_object_mut()
        .ok_or("legacy config is not an object")?
        .remove("workspace");
    fs::write(&config_file, serde_json::to_vec_pretty(&config)?)?;
    let mut legacy = ledger_json(&bench)?;
    for record in legacy["deliveries"]
        .as_array_mut()
        .ok_or("legacy deliveries are not an array")?
    {
        record["delivery"]["claim"]
            .as_object_mut()
            .ok_or("legacy claim is not an object")?
            .remove("workspace");
    }
    fs::write(&ledger_file, serde_json::to_vec_pretty(&legacy)?)?;

    let mut expected = snapshot(&bench.entry)?;
    expected.workspace = None;
    triggers::update(
        bench.home.path(),
        bench.slug(),
        &expected,
        draft(&bench.workspace_a),
    )?;
    let before_ledger = ledger_json(&bench)?;
    let before_triggers = trigger_tree(bench.home.path())?;
    let before_workspace = full_tree(&bench.workspace_a)?;
    let fetched = Cell::new(0_u8);

    let repaired = triggers::poll_with(bench.home.path(), bench.slug(), CREATED + 4, |_| {
        fetched.set(fetched.get() + 1);
        Ok(answer("must-not-fetch"))
    })?;

    let TriggerPoll::Pending { delivery: repaired } = repaired else {
        return Err(format!("mixed legacy ledger did not return Pending: {repaired:?}").into());
    };
    assert_eq!(fetched.get(), 0, "mixed legacy repair fetched Linear");
    assert_eq!(repaired.claim.delivery_id, pending.claim.delivery_id);
    assert_eq!(repaired.claim.run_id, pending.claim.run_id);
    let after_ledger = ledger_json(&bench)?;
    let before_deliveries = ledger_deliveries(&before_ledger)?;
    let after_deliveries = ledger_deliveries(&after_ledger)?;
    assert_eq!(before_deliveries.len(), 3);
    assert_eq!(after_deliveries.len(), 3);
    for (index, (before, after)) in before_deliveries
        .into_iter()
        .zip(after_deliveries)
        .enumerate()
    {
        let expected_workspace =
            (index == 0).then(|| bench.workspace_a.to_string_lossy().into_owned());
        assert_eq!(
            after.claim.workspace, expected_workspace,
            "repair hydrated a Bound/Accepted claim or missed Pending"
        );
        let mut after_without_workspace = after;
        after_without_workspace.claim.workspace = None;
        assert_eq!(
            after_without_workspace, before,
            "repair changed an ID, workflow, issue or receipt payload"
        );
    }
    for index in 0..3 {
        assert_eq!(
            after_ledger["deliveries"][index]["state"], before_ledger["deliveries"][index]["state"],
            "repair changed delivery state, run_file or accepted receipt"
        );
    }
    assert_mixed_repair_scope(&bench, &before_triggers, &before_workspace)?;
    Ok(())
}

fn assert_mixed_repair_scope(
    bench: &Bench,
    before_triggers: &TriggerTree,
    before_workspace: &FullTree,
) -> Result<(), Box<dyn Error>> {
    let ledger_name = PathBuf::from(format!(".{}.ledger.json", bench.slug()));
    assert_eq!(
        without_file(&trigger_tree(bench.home.path())?, &ledger_name),
        without_file(before_triggers, &ledger_name),
        "mixed repair changed config, cursor or another trigger file"
    );
    assert_eq!(
        &full_tree(&bench.workspace_a)?,
        before_workspace,
        "mixed repair changed a run receipt or workspace file"
    );
    Ok(())
}

#[test]
fn pending_and_retry_keep_workspace_a_after_config_changes_to_b() -> Result<(), Box<dyn Error>> {
    let mut bench = Bench::new()?;
    let accepted = bench.delivery()?;
    assert_eq!(
        accepted.claim.workspace.as_deref(),
        Some(path_text(&bench.workspace_a)?)
    );
    bench.edit_to_b()?;
    let pending = triggers::poll_with(bench.home.path(), bench.slug(), CREATED + 2, |_| {
        Err(triggers::TriggerError::EmptyAnswer)
    })?;
    let TriggerPoll::Pending { delivery: pending } = pending else {
        return Err(format!("edited config hid the earlier Pending: {pending:?}").into());
    };
    assert_eq!(
        pending.claim.workspace, accepted.claim.workspace,
        "editing config to B rewrote Pending A"
    );
    accept(&bench, &accepted)?;

    let retried = triggers::retry(bench.home.path(), bench.slug(), CREATED + 3)?;

    assert_eq!(
        retried.claim.workspace, accepted.claim.workspace,
        "Run again read the edited config instead of the accepted delivery"
    );
    assert_ne!(retried.claim.delivery_id, accepted.claim.delivery_id);
    Ok(())
}

#[test]
fn retry_of_a_legacy_accepted_run_uses_the_explicit_config_without_rewriting_history()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let accepted = bench.delivery()?;
    accept(&bench, &accepted)?;
    let ledger_file = bench
        .home
        .path()
        .join("triggers")
        .join(format!(".{}.ledger.json", bench.slug()));
    let mut legacy = ledger_json(&bench)?;
    legacy["deliveries"][0]["delivery"]["claim"]
        .as_object_mut()
        .ok_or("legacy accepted claim is not an object")?
        .remove("workspace");
    fs::write(&ledger_file, serde_json::to_vec_pretty(&legacy)?)?;
    let before = ledger_json(&bench)?;

    let retried = triggers::retry(bench.home.path(), bench.slug(), CREATED + 3)?;

    assert_eq!(
        retried.claim.workspace.as_deref(),
        Some(path_text(&bench.workspace_a)?),
        "Run again did not use the workspace explicitly saved in config"
    );
    assert_eq!(retried.claim.workflow, accepted.claim.workflow);
    assert_eq!(retried.issue, accepted.issue);
    assert_ne!(retried.claim.delivery_id, accepted.claim.delivery_id);
    assert_ne!(retried.claim.run_id, accepted.claim.run_id);
    let after = ledger_json(&bench)?;
    assert_eq!(
        after["deliveries"][0], before["deliveries"][0],
        "Run again rewrote the immutable legacy Accepted receipt"
    );
    assert_eq!(
        after["deliveries"].as_array().map(Vec::len),
        Some(2),
        "Run again did not append exactly one fresh delivery"
    );
    let pending = triggers::poll_with(bench.home.path(), bench.slug(), CREATED + 4, |_| {
        Err(triggers::TriggerError::EmptyAnswer)
    })?;
    let TriggerPoll::Pending { delivery: pending } = pending else {
        return Err(format!("legacy retry was not durably Pending: {pending:?}").into());
    };
    assert_eq!(*pending, retried);
    Ok(())
}

#[test]
fn retry_of_a_legacy_accepted_run_refuses_an_unavailable_config_workspace_without_writing()
-> Result<(), Box<dyn Error>> {
    for unregister in [true, false] {
        let bench = Bench::new()?;
        let accepted = bench.delivery()?;
        accept(&bench, &accepted)?;
        let ledger_file = bench
            .home
            .path()
            .join("triggers")
            .join(format!(".{}.ledger.json", bench.slug()));
        let mut legacy = ledger_json(&bench)?;
        legacy["deliveries"][0]["delivery"]["claim"]
            .as_object_mut()
            .ok_or("legacy accepted claim is not an object")?
            .remove("workspace");
        fs::write(&ledger_file, serde_json::to_vec_pretty(&legacy)?)?;
        if unregister {
            workspaces::delete_workspace_inner(bench.home.path(), path_text(&bench.workspace_a)?)?;
        } else {
            fs::remove_dir_all(&bench.workspace_a)?;
        }
        let before = fs::read(&ledger_file)?;

        let retry = triggers::retry(bench.home.path(), bench.slug(), CREATED + 3);

        assert!(
            retry.is_err(),
            "legacy Accepted used an unavailable workspace: {retry:?}"
        );
        assert_eq!(
            fs::read(&ledger_file)?,
            before,
            "unavailable legacy retry changed the immutable ledger"
        );
    }
    Ok(())
}

#[tokio::test]
async fn mismatched_window_folder_is_refused_before_the_live_latch_or_run_directory()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let delivery = bench.delivery()?;
    let state = bench.state()?;
    let run_root = bench.workspace_b.join(".loadout/runs");
    let before_a = full_tree(&bench.workspace_a)?;
    let before_b = full_tree(&bench.workspace_b)?;

    let refused_project =
        state.triggered_project(Some(path_text(&bench.workspace_b)?), &delivery.claim);

    let refused = state.begin_triggered_run(&bench.workspace_b, &delivery.claim);

    assert!(
        refused_project.is_err(),
        "production project resolver accepted active workspace B for delivery A"
    );
    assert!(refused.is_err(), "workspace B took a claim frozen for A");
    assert!(
        !run_root.exists(),
        "workspace mismatch touched a run directory"
    );
    assert_eq!(
        full_tree(&bench.workspace_a)?,
        before_a,
        "mismatch touched workspace A"
    );
    assert_eq!(
        full_tree(&bench.workspace_b)?,
        before_b,
        "mismatch touched workspace B"
    );
    let free = state.begin_run(&bench.workspace_a);
    assert!(
        free.is_ok(),
        "forged mismatch took the live-run latch: {free:?}"
    );
    Ok(())
}

#[tokio::test]
async fn production_run_command_refuses_window_workspace_b_for_delivery_a()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let delivery = bench.delivery()?;
    let state = bench.state_in(&bench.workspace_b)?;
    let before_a = full_tree(&bench.workspace_a)?;
    let before_b = full_tree(&bench.workspace_b)?;
    let (lines, _source) = line_channel(64);

    // Nieistniejący plik jest celowy. Poprawna krawędź odmawia na workspace przed planem;
    // mutant wracający do `project_for(folder)` idzie dalej i szybko odmawia na pliku zamiast
    // zawisnąć na checkpointowym workflow fixture'a.
    let refusal = run_workflow_from_window(
        &state,
        "missing.json",
        1,
        Some(path_text(&bench.workspace_b)?),
        None,
        Some(&delivery.claim),
        lines,
    )
    .await
    .expect_err("the production Run command accepted window workspace B for delivery A");

    assert!(
        refusal.contains("another workspace"),
        "the production command did not name its workspace refusal: {refusal}"
    );
    assert_eq!(full_tree(&bench.workspace_a)?, before_a);
    assert_eq!(full_tree(&bench.workspace_b)?, before_b);
    assert!(
        state.begin_run(&bench.workspace_b).is_ok(),
        "the refused production command took the live latch"
    );
    Ok(())
}

#[test]
fn removed_or_missing_workspace_refuses_poll_and_retry_without_writes() -> Result<(), Box<dyn Error>>
{
    for remove_from_list in [true, false] {
        let bench = Bench::new()?;
        let delivery = bench.delivery()?;
        accept(&bench, &delivery)?;
        if remove_from_list {
            workspaces::delete_workspace_inner(bench.home.path(), path_text(&bench.workspace_a)?)?;
        } else {
            fs::remove_dir_all(&bench.workspace_a)?;
        }
        let before = trigger_tree(bench.home.path())?;
        let fetched = Cell::new(0_u8);
        let poll = triggers::poll_with(bench.home.path(), bench.slug(), CREATED + 2, |_| {
            fetched.set(fetched.get() + 1);
            Ok(answer("must-not-fetch"))
        });
        let retry = triggers::retry(bench.home.path(), bench.slug(), CREATED + 3);

        assert!(
            poll.is_err(),
            "unavailable workspace still polled: {poll:?}"
        );
        assert!(
            retry.is_err(),
            "unavailable workspace still retried: {retry:?}"
        );
        assert_eq!(fetched.get(), 0, "unavailable workspace reached Linear");
        assert_eq!(
            trigger_tree(bench.home.path())?,
            before,
            "workspace refusal changed the ledger or cursor"
        );
    }
    Ok(())
}

#[tokio::test]
async fn production_start_refuses_unregistered_and_missing_workspace_before_latch_or_run_directory()
-> Result<(), Box<dyn Error>> {
    for unregister in [true, false] {
        let bench = Bench::new()?;
        let delivery = bench.delivery()?;
        let state = bench.state()?;
        let expected = if unregister {
            workspaces::delete_workspace_inner(bench.home.path(), path_text(&bench.workspace_a)?)?;
            "Choose a workspace from Loadout"
        } else {
            fs::remove_dir_all(&bench.workspace_a)?;
            "workspace folder is not there"
        };
        let before = full_tree(bench.home.path())?;

        let project =
            state.triggered_project(Some(path_text(&bench.workspace_a)?), &delivery.claim);
        let start = state.begin_triggered_run(&bench.workspace_a, &delivery.claim);
        let said = project.expect_err("production project resolver used an unavailable workspace");
        assert!(
            said.contains(expected) && said.contains("trigger"),
            "Start refusal did not name the workspace repair: {said}"
        );
        assert!(
            start.is_err(),
            "production Start reserved a run for an unavailable workspace"
        );
        assert_eq!(
            full_tree(bench.home.path())?,
            before,
            "unavailable-workspace Start touched config, ledger, cursor or a run directory"
        );
        assert!(
            !bench.workspace_b.join(".loadout/runs").exists(),
            "unavailable-workspace Start created a run in workspace B"
        );
        if unregister {
            assert!(
                !bench.workspace_a.join(".loadout/runs").exists(),
                "unregistered-workspace Start created a run in workspace A"
            );
        }
        let free = state.begin_run(&bench.workspace_b);
        assert!(
            free.is_ok(),
            "unavailable-workspace Start took the live latch: {free:?}"
        );
    }
    Ok(())
}

fn draft(workspace: &Path) -> TriggerDraft {
    TriggerDraft {
        source: "linear".to_owned(),
        condition: "assigned-to-me".to_owned(),
        workflow: "ship.json".to_owned(),
        workspace: workspace.to_string_lossy().into_owned(),
        poll_every_minutes: 1,
        api_key: Some(Secret::new(KEY)),
    }
}

fn snapshot(entry: &TriggerEntry) -> Result<TriggerSnapshot, Box<dyn Error>> {
    Ok(TriggerSnapshot {
        slug: entry.slug.clone(),
        source: entry.source.clone().ok_or("entry source")?,
        condition: entry.condition.clone().ok_or("entry condition")?,
        workflow: entry.workflow.clone().ok_or("entry workflow")?,
        workspace: entry.workspace.clone(),
        enabled: entry.enabled.ok_or("entry enabled")?,
        poll_every_minutes: entry.poll_every_minutes.ok_or("entry cadence")?,
        key_saved: entry.key_saved.ok_or("entry key status")?,
    })
}

fn poll(bench: &Bench, id: &str, created_at: i64) -> Result<TriggerPoll, triggers::TriggerError> {
    triggers::poll_with(bench.home.path(), bench.slug(), created_at, |_| {
        Ok(answer(id))
    })
}

fn ledger_json(bench: &Bench) -> Result<serde_json::Value, Box<dyn Error>> {
    let path = bench
        .home
        .path()
        .join("triggers")
        .join(format!(".{}.ledger.json", bench.slug()));
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn ledger_deliveries(ledger: &serde_json::Value) -> Result<Vec<TriggerDelivery>, Box<dyn Error>> {
    ledger["deliveries"]
        .as_array()
        .ok_or_else(|| -> Box<dyn Error> { "ledger deliveries are not an array".into() })?
        .iter()
        .map(|record| {
            serde_json::from_value(record["delivery"].clone())
                .map_err(|error| -> Box<dyn Error> { Box::new(error) })
        })
        .collect()
}

fn answer(id: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "data": { "issues": { "nodes": [{
            "id": id,
            "identifier": format!("LOAD-{id}"),
            "title": format!("Issue {id}"),
            "url": format!("https://linear.app/loadout/issue/{id}"),
            "description": "body",
            "updatedAt": if id == "old" { "2026-08-21T09:00:00.000Z" } else { "2026-08-21T09:01:00.000Z" }
        }] } }
    }))
    .expect("issue response")
}

fn accept(bench: &Bench, delivery: &TriggerDelivery) -> Result<(), Box<dyn Error>> {
    let run_file = bench
        .workspace_a
        .join(".loadout/runs")
        .join(&delivery.claim.run_id)
        .join("run.json");
    triggers::bind_delivery(bench.home.path(), &delivery.claim, &run_file)?;
    fs::create_dir_all(run_file.parent().ok_or("run file parent")?)?;
    fs::write(
        &run_file,
        serde_json::to_vec_pretty(&json!({
            "id": delivery.claim.run_id,
            "created_at": delivery.created_at,
            "trigger_origin": {
                "slug": delivery.claim.slug,
                "delivery_id": delivery.claim.delivery_id,
                "issue_id": delivery.issue.id
            }
        }))?,
    )?;
    triggers::accept_delivery(
        bench.home.path(),
        &delivery.claim,
        &run_file,
        delivery.created_at,
    )?;
    Ok(())
}

fn fixed_id(last: u8) -> Uuid {
    Uuid::parse_str(&format!("0198a1f2-3b4c-7d5e-8f60-1122334455{last:02x}"))
        .expect("fixed UUID v7")
}

fn path_text(path: &Path) -> Result<&str, Box<dyn Error>> {
    path.to_str().ok_or_else(|| "test path is not UTF-8".into())
}

fn trigger_tree(home: &Path) -> Result<TriggerTree, Box<dyn Error>> {
    let dir = home.join(triggers::TRIGGERS_DIR);
    let mut out = fs::read_dir(dir)?
        .map(|entry| {
            let entry = entry?;
            Ok((PathBuf::from(entry.file_name()), fs::read(entry.path())?))
        })
        .collect::<Result<Vec<_>, std::io::Error>>()?;
    out.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(out)
}

fn without_file(tree: &[(PathBuf, Vec<u8>)], skipped: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    tree.iter()
        .filter(|(path, _)| path.as_path() != skipped)
        .cloned()
        .collect()
}

fn file_bytes<'a>(tree: &'a [(PathBuf, Vec<u8>)], wanted: &Path) -> Option<&'a [u8]> {
    tree.iter()
        .find(|(path, _)| path.as_path() == wanted)
        .map(|(_, bytes)| bytes.as_slice())
}

fn full_tree(root: &Path) -> Result<FullTree, Box<dyn Error>> {
    fn visit(root: &Path, at: &Path, out: &mut FullTree) -> Result<(), Box<dyn Error>> {
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

fn no_agents_needed() -> Drivers {
    let absent: Arc<dyn AgentDriver> = Arc::new(Absent::new("nobody", "trigger workspace test"));
    Arc::new(move |_vendor| Arc::clone(&absent))
}
