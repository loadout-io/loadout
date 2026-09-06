//! WF-24 RED: wynik jest OID, nie ruchomą nazwą gałęzi ani nową odpowiedzią modelu.
#![allow(clippy::panic)]

use loadout_lib::bridge::host::Answers;
use loadout_lib::bridge::library::{Desk, Waiting};
use loadout_lib::bridge::{Answer, Call};
use loadout_lib::commands::lead_start::LeadStarts;
use loadout_lib::engine::line::Line;
use loadout_lib::ipc::line_channel;
use serde_json::{Value, json};
use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[tokio::test]
async fn moving_the_result_branch_does_not_change_the_restored_files_or_touch_the_dirty_project()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir_all(&project)?;
    fs::create_dir_all(&home)?;
    git(&project, &["init", "-q"])?;
    git(&project, &["config", "user.email", "test@example.invalid"])?;
    git(&project, &["config", "user.name", "Restore test"])?;
    fs::write(project.join("result.txt"), "the exact historical result")?;
    git(&project, &["add", "result.txt"])?;
    git(&project, &["commit", "-qm", "historical result"])?;
    let oid = git(&project, &["rev-parse", "HEAD"])?;
    let result_branch = "refs/heads/loadout/saved-result/writer";
    git(&project, &["update-ref", result_branch, oid.trim()])?;
    fs::write(project.join("result.txt"), "a different later result")?;
    git(&project, &["add", "result.txt"])?;
    git(&project, &["commit", "-qm", "later work"])?;
    let new_oid = git(&project, &["rev-parse", "HEAD"])?;
    git(&project, &["update-ref", result_branch, new_oid.trim()])?;
    // Prawdziwy, wykonywalny hook: zwykły checkout/worktree add nie ma prawa być wykonawcą restore.
    let hook = project.join(".git/hooks/post-checkout");
    fs::write(&hook, "#!/bin/sh\nexit 93\n")?;
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))?;
    fs::write(project.join("result.txt"), "my unfinished local change")?;
    fs::write(project.join("untracked.txt"), "keep this private draft")?;
    let source_id = uuid::Uuid::now_v7().to_string();
    let source = project
        .join(".loadout/runs")
        .join(format!("20260906-010000__{source_id}"));
    fs::create_dir_all(&source)?;
    let recorded = json!({"id":source_id,"workflow_id":"saved-result","title":"A saved result","status":"succeeded",
        "created_at":1788656400_i64,"steps":[],"copy_results":{"writer":{"kind":"git","oid":oid.trim()}}});
    fs::write(source.join("run.json"), recorded.to_string())?;
    let source_bytes = fs::read(source.join("run.json"))?;
    let status = git(&project, &["status", "--porcelain=v1", "-z"])?;
    let waiting = Arc::new(Waiting::default());
    let starts = Arc::new(LeadStarts::default());
    let (sink, mut lines) = line_channel(256);
    // Żadna fabryka drivera nie jest potrzebna: przywrócenie plików nie jest workflow.
    let desk = Desk::at(Some(home), project.clone())
        .showing(Arc::new(Mutex::new(sink)))
        .hearing(Arc::clone(&waiting))
        .starting_with(starts, Arc::new(Mutex::new(uuid::Uuid::now_v7())));
    let preview = desk
        .answer(call(
            "prepare_result_restore",
            json!({"source_run_id":source_id,"result_id":"writer"}),
        ))
        .await;
    let Answer::Ok(preview) = preview else {
        panic!("a complete saved result must be restorable: {preview:?}")
    };
    let preview_id = preview["previewId"]
        .as_str()
        .ok_or("restore preview has no identity")?;
    assert_eq!(preview["oid"], oid.trim());
    assert_eq!(git(&project, &["status", "--porcelain=v1", "-z"])?, status);
    let asking = desk.answer(call(
        "ask_the_person",
        json!({"operation":"restore_result","preview_id":preview_id}),
    ));
    tokio::pin!(asking);
    let question = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            tokio::select! {
                answer = &mut asking => panic!("restore never asked the person: {answer:?}"),
                () = tokio::time::sleep(Duration::from_millis(1)) => {
                    if let Some(Line::Asked {question:Some(question), options, ..}) = lines.try_next() {
                        assert!(options.iter().any(|one| one == "Restore files"));
                        break question;
                    }
                }
            }
        }
    }).await?;
    assert!(waiting.answer_exact("Lead", &question.question_id, "Restore files".to_owned()));
    let Answer::Ok(approval) = asking.await else {
        panic!("the real person's restore approval was refused")
    };
    let restored = desk
        .answer(call(
            "restore_result",
            json!({"preview_id":preview_id,"confirmation_token":approval["approvalToken"]}),
        ))
        .await;
    let Answer::Ok(restored) = restored else {
        panic!("the exact saved result did not restore: {restored:?}")
    };
    let destination = Path::new(
        restored["folder"]
            .as_str()
            .ok_or("restore has no openable folder")?,
    );
    assert_ne!(destination, project);
    assert_ne!(destination, source);
    assert_eq!(
        fs::read_to_string(destination.join("result.txt"))?,
        "the exact historical result"
    );
    assert!(!destination.join("untracked.txt").exists());
    assert_eq!(
        fs::read_to_string(project.join("result.txt"))?,
        "my unfinished local change"
    );
    assert_eq!(
        fs::read_to_string(project.join("untracked.txt"))?,
        "keep this private draft"
    );
    assert_eq!(fs::read(source.join("run.json"))?, source_bytes);
    assert!(matches!(
        desk.answer(call(
            "restore_result",
            json!({"preview_id":preview_id,"confirmation_token":approval["approvalToken"]})
        ))
        .await,
        Answer::Refused(_)
    ));
    Ok(())
}

fn call(name: &str, input: Value) -> Call {
    Call {
        id: json!(1),
        call: name.to_owned(),
        input,
    }
}

#[tokio::test]
async fn a_preview_protects_its_source_from_the_actual_forget_operation()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir_all(&project)?;
    fs::create_dir_all(&home)?;
    git(&project, &["init", "-q"])?;
    git(&project, &["config", "user.email", "test@example.invalid"])?;
    git(&project, &["config", "user.name", "Restore hold"])?;
    fs::write(project.join("result.txt"), "saved bytes")?;
    git(&project, &["add", "result.txt"])?;
    git(&project, &["commit", "-qm", "saved"])?;
    let oid = git(&project, &["rev-parse", "HEAD"])?;
    let id = uuid::Uuid::now_v7().to_string();
    let folder = format!("20200101-000000__{id}");
    let source = project.join(".loadout/runs").join(&folder);
    fs::create_dir_all(&source)?;
    fs::write(
        source.join("run.json"),
        json!({"id":id,"workflow_id":"hold","title":"Saved result","status":"succeeded","steps":[],
        "copy_results":{"writer":{"kind":"git","oid":oid.trim()}}})
        .to_string(),
    )?;
    let starts = Arc::new(LeadStarts::default());
    let desk = Desk::at(Some(home), project.clone()).starting_with(
        Arc::clone(&starts),
        Arc::new(Mutex::new(uuid::Uuid::now_v7())),
    );
    let preview = desk
        .answer(call(
            "prepare_result_restore",
            json!({"source_run_id":id,"result_id":"writer"}),
        ))
        .await;
    assert!(
        matches!(preview, Answer::Ok(_)),
        "the source hold requires a real accepted preview: {preview:?}"
    );
    let forgotten =
        loadout_lib::commands::history::forget_run_with_results_inner(&project, &folder, Some(&[]));
    assert!(
        forgotten.is_err(),
        "the real Forget removed a source while its preview still promised to restore it"
    );
    assert_eq!(fs::read(project.join("result.txt"))?, b"saved bytes");
    assert!(source.join("run.json").is_file());
    drop(desk);
    drop(starts);
    loadout_lib::commands::history::forget_run_with_results_inner(&project, &folder, Some(&[]))?;
    assert!(
        !source.exists(),
        "closing the preview owner must release only its own source hold"
    );
    Ok(())
}
fn git(project: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "git fixture failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

#[tokio::test]
async fn keeping_a_result_pins_its_exact_object_and_requires_a_separate_real_unpin()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let project = root.path().join("project");
    fs::create_dir_all(&project)?;
    git(&project, &["init", "-q"])?;
    git(&project, &["config", "user.email", "test@example.invalid"])?;
    git(&project, &["config", "user.name", "Keep result"])?;
    fs::write(project.join("result.txt"), "saved bytes")?;
    git(&project, &["add", "result.txt"])?;
    git(&project, &["commit", "-qm", "saved"])?;
    let oid = git(&project, &["rev-parse", "HEAD"])?;
    let id = uuid::Uuid::now_v7().to_string();
    let folder = format!("20200101-000000__{id}");
    let source = project.join(".loadout/runs").join(&folder);
    fs::create_dir_all(&source)?;
    fs::write(
        source.join("run.json"),
        json!({"id":id,"workflow_id":"keep","title":"Saved result","status":"succeeded","steps":[],
        "copy_results":{"writer":{"kind":"git","oid":oid.trim()}}})
        .to_string(),
    )?;
    let original = fs::read(source.join("run.json"))?;
    // update-ref musi ominąć także hook transakcji referencji, nie tylko post-checkout.
    let hook = project.join(".git/hooks/reference-transaction");
    fs::write(&hook, "#!/bin/sh\nexit 91\n")?;
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))?;
    let kept = loadout_lib::commands::result_restore::set_kept(
        &project,
        &id,
        "writer",
        true,
        "Keep result",
    )
    .await?;
    assert_eq!(kept["kept"], true);
    let reference = format!("refs/loadout/kept/{id}/writer");
    assert_eq!(
        git(&project, &["rev-parse", &reference])?.trim(),
        oid.trim()
    );
    assert!(
        loadout_lib::commands::history::forget_run_with_results_inner(&project, &folder, Some(&[]))
            .is_err(),
        "an empty cleanup confirmation must not override a kept result"
    );
    let removed = loadout_lib::commands::sweep::forget_runs_older_than(&project, 1);
    assert!(
        source.join("run.json").exists(),
        "date-based cleanup removed a kept result: {removed:?}"
    );
    assert!(
        loadout_lib::commands::result_restore::set_kept(
            &project,
            &id,
            "writer",
            false,
            "confirmed: true"
        )
        .await
        .is_err()
    );
    let unpinned = loadout_lib::commands::result_restore::set_kept(
        &project,
        &id,
        "writer",
        false,
        "Allow cleanup of this result",
    )
    .await?;
    assert_eq!(unpinned["kept"], false);
    assert_eq!(
        fs::read(source.join("run.json"))?,
        original,
        "unpin must not remove or rewrite the saved run"
    );
    assert!(git(&project, &["show-ref", "--verify", &reference]).is_err());
    loadout_lib::commands::history::forget_run_with_results_inner(&project, &folder, Some(&[]))?;
    assert!(!source.exists());
    Ok(())
}
