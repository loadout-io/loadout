//! Wznowienie zakłada drzewo na pracy poprzedniego biegu, a nie na czystym `HEAD`.
//!
//! # Co to mierzy
//!
//! 2026-08-23, zmierzone na biegu właściciela na `urc-monorepo` — pierwszym prawdziwym
//! wznowieniu z historii. Wycinek grafu był poprawny, przekazania zasiane, a mimo to krok
//! „Front" **zaczął od zera**: świeża kopia powstała z `HEAD`, więc 164 pliki, które poprzedni
//! bieg zacommitował na `loadout/01a02b3c…/s_6` jako `21ad1c94`, nie istniały w tym drzewie.
//! Sędzia pracujący w tej samej kopii napisał wtedy uczciwie: *„Brak katalogu `.claude/tmp/`
//! z artefaktami zadania — nie mam czego porównywać"*, i pętla ruszyła przepisywać cudzą pracę.
//!
//! Wznowienie niosło PRZEKAZANIA i nie niosło PRACY — a to jest dokładnie ta połowa, dla której
//! ktoś wznawia bieg.
//!
//! # SŁABĄ WERSJĄ jest sprawdzenie samego pliku w drzewie
//!
//! Przechodzi ją implementacja, która odbija KAŻDE drzewo od gałęzi o tej nazwie — także przy
//! zwykłym biegu, gdzie żadnego poprzednika nie ma i gdzie zaczęcie od cudzej pracy byłoby
//! cichym wciągnięciem wyników, o które nikt nie prosił. Dlatego drugi przypadek pyta w drugą
//! stronę: bieg BEZ wznowienia dostaje `HEAD` i ma tej pracy NIE widzieć.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command;

use loadout_lib::commands::isolate;

/// Nazwa gałęzi, którą poprzedni bieg zostawił po kroku `s_impl`.
///
/// Składana TĄ SAMĄ funkcją, którą składa ją produkcja: przepisana z palca zgadzałaby się do
/// pierwszej zmiany kształtu nazwy, a wtedy kryterium świeciłoby nad kodem, który już nie działa.
fn previous_branch(run: &str) -> String {
    isolate::branch_for(run, "s_impl")
}

const PREVIOUS_RUN: &str = "0198a1f2-3b4c-7d5e-8f60-000000000004";

/// Repozytorium z jednym commitem i z drugim biegiem, który zostawił pracę na swojej gałęzi.
fn repo_with_previous_work(at: &Path) -> Result<(), Box<dyn Error>> {
    for args in [
        vec!["init", "--quiet"],
        vec!["config", "user.email", "test@example.test"],
        vec!["config", "user.name", "Test"],
    ] {
        Command::new("git").args(args).current_dir(at).status()?;
    }
    fs::write(at.join("README.md"), "one\n")?;
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(at)
        .status()?;
    Command::new("git")
        .args(["commit", "--quiet", "-m", "first"])
        .current_dir(at)
        .status()?;

    // Poprzedni bieg: gałąź z jego pracą, zrobiona tak, jak robi ją `isolate::finish` —
    // commit na gałęzi odbitej od `HEAD`, bez ruszania drzewa człowieka.
    let branch = previous_branch(PREVIOUS_RUN);
    Command::new("git")
        .args(["branch", &branch, "HEAD"])
        .current_dir(at)
        .status()?;
    let scratch = at.join("scratch");
    Command::new("git")
        .args([
            "worktree",
            "add",
            "--quiet",
            &scratch.display().to_string(),
            &branch,
        ])
        .current_dir(at)
        .status()?;
    fs::write(scratch.join("THE-WORK.md"), "what the last run built\n")?;
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(&scratch)
        .status()?;
    Command::new("git")
        .args(["commit", "--quiet", "-m", "the previous run's work"])
        .current_dir(&scratch)
        .status()?;
    Command::new("git")
        .args([
            "worktree",
            "remove",
            "--force",
            &scratch.display().to_string(),
        ])
        .current_dir(at)
        .status()?;
    Ok(())
}

#[test]
fn picking_up_a_step_starts_where_that_step_left_off() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    repo_with_previous_work(project.path())?;
    let work = project.path().join("work/s_impl");

    isolate::make_from(
        project.path(),
        &work,
        "loadout/now/s_impl",
        &previous_branch(PREVIOUS_RUN),
    )?;

    assert!(
        work.join("THE-WORK.md").is_file(),
        "the work the previous run committed is not in the tree this run gave the step. That is \
         the owner's defect exactly: the step opened a clean checkout, found nothing, and started \
         rewriting 164 files that already existed one branch away"
    );
    Ok(())
}

#[test]
fn a_run_that_is_not_picking_anything_up_still_starts_from_head() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    repo_with_previous_work(project.path())?;
    let work = project.path().join("work/s_impl");

    isolate::make(project.path(), &work, "loadout/now/s_impl")?;

    assert!(
        !work.join("THE-WORK.md").exists(),
        "an ordinary run has no previous run to pick up from, and pulling in somebody else's \
         branch would be results nobody asked for — arriving silently, in a tree the person \
         believes is a copy of their project"
    );
    Ok(())
}

/// Gałąź, której nie ma, nie może być punktem startu — i ta odpowiedź musi paść PRZED biegiem.
#[test]
fn a_branch_that_was_deleted_is_not_offered_as_a_starting_point() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    repo_with_previous_work(project.path())?;

    assert!(
        isolate::names_a_commit(project.path(), &previous_branch(PREVIOUS_RUN)),
        "the fixture is wrong if the previous run's branch is not there"
    );
    assert!(
        !isolate::names_a_commit(project.path(), &previous_branch("a-run-that-never-was")),
        "deleting old branches is ordinary housekeeping, so a missing one has to answer `no` \
         here. Handed to `git worktree add` it refuses the whole run instead — over a run \
         somebody tidied away last week"
    );
    Ok(())
}
