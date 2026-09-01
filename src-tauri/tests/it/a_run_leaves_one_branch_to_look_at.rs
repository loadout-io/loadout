//! Praca biegu ląduje na JEDNEJ gałęzi, a konflikt nie zostawia połowy.
//!
//! Zgłoszenie właściciela 2026-09-01: „powinnismy miec w workflow finalize step gdzie to wszystko
//! jest scalane". Do tego dnia bieg zostawiał po sobie kilka gałęzi `loadout/<bieg>/<krok>`,
//! o których produkt nie mówił ani słowa — żeby zobaczyć swoją pracę, trzeba było wiedzieć, że
//! istnieją, i znać schemat nazwy.
//!
//! SŁABA WERSJA tego kryterium pytałaby, czy gałąź wyniku powstała. Przeszłaby dla implementacji,
//! która scala do połowy i zatrzymuje się na konflikcie, zostawiając gałąź opisującą stan,
//! którego nie opisuje żaden krok. Dlatego punkt o zderzeniu pyta, czy po nieudanym składaniu
//! **nie ma po nas nic**.
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command;

use loadout_lib::commands::finalize::{Landing, fold_into_one};

fn git(at: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(at)
        .args(["-c", "user.name=Test"])
        .args(["-c", "user.email=test@localhost"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()?;
    if !out.status.success() {
        return Err(format!("git {args:?}: {}", String::from_utf8_lossy(&out.stderr)).into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Repozytorium z jednym commitem i gałęziami udającymi kroki biegu.
struct Repo {
    home: tempfile::TempDir,
}

impl Repo {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = tempfile::tempdir()?;
        git(home.path(), &["init", "--quiet", "--initial-branch=main"])?;
        fs::write(home.path().join("shared.txt"), "one\n")?;
        git(home.path(), &["add", "."])?;
        git(home.path(), &["commit", "--quiet", "-m", "start"])?;
        Ok(Self { home })
    }

    fn at(&self) -> &Path {
        self.home.path()
    }

    /// Gałąź kroku, która zmienia jeden plik.
    fn step(&self, branch: &str, file: &str, text: &str) -> Result<(), Box<dyn Error>> {
        git(self.at(), &["checkout", "--quiet", "-b", branch, "main"])?;
        fs::write(self.at().join(file), text)?;
        git(self.at(), &["add", "."])?;
        git(self.at(), &["commit", "--quiet", "-m", branch])?;
        git(self.at(), &["checkout", "--quiet", "main"])?;
        Ok(())
    }

    fn branches(&self) -> Result<Vec<String>, Box<dyn Error>> {
        Ok(git(
            self.at(),
            &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
        )?
        .lines()
        .map(str::to_owned)
        .collect())
    }
}

#[test]
fn two_steps_that_touched_different_files_land_on_one_branch() -> Result<(), Box<dyn Error>> {
    let repo = Repo::new()?;
    repo.step("loadout/r1/s_1", "first.txt", "a\n")?;
    repo.step("loadout/r1/s_2", "second.txt", "b\n")?;

    let landed = fold_into_one(
        repo.at(),
        "task-T-160",
        "main",
        &["loadout/r1/s_1".to_owned(), "loadout/r1/s_2".to_owned()],
    )?;

    assert_eq!(
        landed,
        Landing::Landed {
            branch: "task-T-160".to_owned(),
            steps: 2
        },
        "the work of two steps did not come together under one name, so a person still has to \
         know that loadout/<run>/<step> exists and go looking for each one"
    );
    assert!(
        repo.branches()?.iter().any(|one| one == "task-T-160"),
        "the branch was reported but is not in the repository"
    );

    let shown = git(
        repo.at(),
        &["show", "--stat", "--name-only", "task-T-160", "--", "."],
    )?;
    let _ = shown;
    let files = git(repo.at(), &["ls-tree", "-r", "--name-only", "task-T-160"])?;
    assert!(
        files.contains("first.txt") && files.contains("second.txt"),
        "the result branch is missing work from one of the steps. Found: {files}"
    );
    Ok(())
}

#[test]
fn the_person_working_tree_is_never_touched() -> Result<(), Box<dyn Error>> {
    let repo = Repo::new()?;
    repo.step("loadout/r1/s_1", "first.txt", "a\n")?;

    fold_into_one(
        repo.at(),
        "task-T-161",
        "main",
        &["loadout/r1/s_1".to_owned()],
    )?;

    assert_eq!(
        git(repo.at(), &["rev-parse", "--abbrev-ref", "HEAD"])?.trim(),
        "main",
        "folding moved the branch the person was standing on. Their working tree is the one \
         place where a machine's mistake costs them their own work."
    );
    assert!(
        !repo.at().join("first.txt").exists(),
        "the step's file appeared in the person's folder. The result belongs on a branch they \
         choose to take, not in the tree they are working in."
    );
    assert!(
        git(repo.at(), &["status", "--porcelain"])?
            .trim()
            .is_empty(),
        "folding left the person's working tree dirty"
    );
    Ok(())
}

#[test]
fn a_clash_leaves_nothing_behind_and_names_the_file() -> Result<(), Box<dyn Error>> {
    let repo = Repo::new()?;
    repo.step("loadout/r1/s_1", "shared.txt", "from the first step\n")?;
    repo.step("loadout/r1/s_2", "shared.txt", "from the second step\n")?;

    let landed = fold_into_one(
        repo.at(),
        "task-T-162",
        "main",
        &["loadout/r1/s_1".to_owned(), "loadout/r1/s_2".to_owned()],
    )?;

    assert_eq!(
        landed,
        Landing::Clash {
            with: "loadout/r1/s_2".to_owned(),
            files: vec!["shared.txt".to_owned()],
        },
        "two steps wrote the same file and the answer does not say so, or does not name the file \
         it happened on — and then a person is told that two steps disagreed without being told \
         where"
    );

    assert!(
        !repo.branches()?.iter().any(|one| one == "task-T-162"),
        "a half-folded branch was left behind. It describes a state no step produced, which is \
         exactly what fan_in refuses to do when it finds the same disagreement."
    );
    Ok(())
}

#[test]
fn a_run_where_nobody_wrote_anything_says_so() -> Result<(), Box<dyn Error>> {
    let repo = Repo::new()?;

    let landed = fold_into_one(
        repo.at(),
        "task-T-163",
        "main",
        &["loadout/r1/s_1".to_owned()],
    )?;

    assert_eq!(
        landed,
        Landing::Nothing,
        "a run in which no step changed a byte pretended to leave something behind"
    );
    assert!(!repo.branches()?.iter().any(|one| one == "task-T-163"));
    Ok(())
}

#[test]
fn a_name_already_in_use_refuses_before_anything_is_merged() -> Result<(), Box<dyn Error>> {
    let repo = Repo::new()?;
    repo.step("task-T-164", "taken.txt", "already here\n")?;
    repo.step("loadout/r1/s_1", "first.txt", "a\n")?;

    let answer = fold_into_one(
        repo.at(),
        "task-T-164",
        "main",
        &["loadout/r1/s_1".to_owned()],
    );

    assert!(
        answer.is_err(),
        "an existing branch was about to be written over. The refusal has to come before any \
         merging, or the work is done first and has nowhere to land second."
    );
    Ok(())
}
