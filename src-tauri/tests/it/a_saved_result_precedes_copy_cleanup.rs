//! WF-25: wynik musi mieć trwały adres ZANIM znika jego katalog, także przy późnym EOF.

use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::commands::isolate;

#[test]
fn a_refused_result_write_keeps_the_committed_working_copy() -> Result<(), Box<dyn Error>> {
    check_publication(false, true)
}

#[test]
fn an_unchanged_result_is_recorded_before_its_branch_disappears() -> Result<(), Box<dyn Error>> {
    check_publication(true, false)
}

#[test]
fn a_saved_result_can_then_release_its_copy() -> Result<(), Box<dyn Error>> {
    check_publication(true, true)
}

fn check_publication(accept: bool, changed: bool) -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let project = root.path().join("project");
    let cwd = root.path().join("copy");
    fs::create_dir(&project)?;
    git(&project, &["init", "--quiet"])?;
    fs::write(project.join("value"), "before")?;
    git(&project, &["add", "value"])?;
    git(&project, &["commit", "--quiet", "-m", "input"])?;
    let base = git(&project, &["rev-parse", "HEAD"])?;
    let branch = "loadout/receipt-boundary/result";
    isolate::make(&project, &cwd, branch)?;
    if changed {
        fs::write(cwd.join("value"), "after")?;
    }
    let mut observed = None;
    let closed = isolate::finish_with_saved(
        &project,
        &cwd,
        branch,
        "save result",
        Some(base.trim()),
        |oid| {
            observed = Some((
                cwd.join("value").is_file(),
                git(&project, &["show", &format!("{oid}:value")]).ok(),
            ));
            if accept {
                Ok(())
            } else {
                Err("the result receipt could not be published".to_owned())
            }
        },
    );
    assert_eq!(
        observed,
        Some((
            true,
            Some(if changed { "after" } else { "before" }.to_owned())
        )),
        "cleanup ran without first publishing the exact, already saved result"
    );
    if accept {
        assert!(!cwd.exists());
        assert!(!matches!(closed.kept, isolate::Kept::LeftInPlace { .. }));
    } else {
        assert!(
            cwd.join("value").is_file(),
            "a failed receipt must keep the recoverable copy"
        );
        assert!(matches!(closed.kept, isolate::Kept::LeftInPlace { .. }));
        assert!(
            matches!(closed.kept, isolate::Kept::LeftInPlace { why, .. } if why.contains("receipt"))
        );
    }
    Ok(())
}

fn git(project: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(project)
        .args([
            "-c",
            "user.name=Loadout test",
            "-c",
            "user.email=test@loadout.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(String::from_utf8(output.stdout)?)
}
