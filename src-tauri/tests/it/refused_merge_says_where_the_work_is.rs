//! Odmowa scalania podaje adres, pod którym praca każdej kopii naprawdę zostanie.
//!
//! 2026-09-10 — INCYDENT „Murmur-1". Zdanie odmowy obiecuje: „both of them still have their own
//! copy, so you can open each one and decide". W chwili powstania jest prawdziwe — scalanie
//! właśnie te katalogi czytało. Sekundy później bieg się kończy, `close_the_trees` commituje
//! każde izolowane drzewo na jego gałąź i katalog znika. Zmierzone: odmowa o 02:49:47, koniec
//! biegu o 02:49:47, a o 02:50 w `work/` nie ma już ani jednej kopii. Właściciel poszedł szukać
//! tam, gdzie kazało zdanie, i znalazł pusty katalog.
//!
//! CZEGO TEN ZESTAW PILNUJE Z DRUGIEJ STRONY: kopia, która gałęzi nie ma, NIE dostaje wiersza.
//! Zmyślony adres jest gorszy niż jego brak — człowiek szukałby gałęzi, której nigdy nie było.
//! Bez tej drugiej połowy przechodziłaby implementacja doklejająca wiersz każdemu rodzicowi.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use loadout_lib::commands::fan_in::Parent;
use loadout_lib::commands::run::and_where_each_copy_is_kept;

const RUN: &str = "20260909-232320__01a0887b-a674-7e62-96a3-066e2e956771";
const REFUSAL: &str = "\"Frontend\" and \"Design\" both changed .angular/x, and left different \
                       results there.";
const ON_A_BRANCH: &str = "s_6";
const NO_BRANCH: &str = "s_5";

struct Bench {
    _root: tempfile::TempDir,
    project: PathBuf,
    run_dir: PathBuf,
}

impl Bench {
    /// Jeden pas skończył izolowany i ma gałąź, drugi pracował bez izolacji i nie ma żadnej.
    /// Dokładnie taki był układ w incydencie.
    fn new() -> Result<Self, Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let project = root.path().join("project");
        let run_dir = project.join(".loadout/runs").join(RUN);
        fs::create_dir_all(run_dir.join("work").join(ON_A_BRANCH))?;
        fs::create_dir_all(run_dir.join("work").join(NO_BRANCH))?;
        fs::create_dir_all(run_dir.join(".isolation"))?;
        fs::write(
            run_dir.join(".isolation").join(ON_A_BRANCH),
            format!(
                r#"{{"state":"complete","branch":"loadout/{RUN}/{ON_A_BRANCH}","head":"178cc24a"}}"#
            ),
        )?;
        Ok(Self {
            _root: root,
            project,
            run_dir,
        })
    }

    fn work(&self, key: &str) -> PathBuf {
        self.run_dir.join("work").join(key)
    }

    fn said(&self, parents: &[Parent<'_>]) -> String {
        and_where_each_copy_is_kept(REFUSAL, &self.project, &self.run_dir, parents)
    }
}

#[test]
fn the_refusal_points_at_the_branch_the_work_was_saved_to() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let kept = bench.work(ON_A_BRANCH);

    let said = bench.said(&[Parent {
        name: "Frontend",
        cwd: &kept,
        born: None,
    }]);

    assert!(
        said.starts_with(REFUSAL),
        "the refusal itself must survive; the address is an addition, not a replacement: {said}"
    );
    assert!(
        said.contains(&format!("loadout/{RUN}/{ON_A_BRANCH}")),
        "the person was told to open a copy without being told where it will be: {said}"
    );
    Ok(())
}

#[test]
fn a_copy_with_no_branch_is_given_no_address() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let orphan = bench.work(NO_BRANCH);

    let said = bench.said(&[Parent {
        name: "Design",
        cwd: &orphan,
        born: None,
    }]);

    assert_eq!(
        said, REFUSAL,
        "a copy that git never received has no address, and inventing one sends the person \
         looking for a branch that was never written"
    );
    Ok(())
}

#[test]
fn only_the_copy_that_has_one_is_named() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let kept = bench.work(ON_A_BRANCH);
    let orphan = bench.work(NO_BRANCH);

    let said = bench.said(&[
        Parent {
            name: "Frontend",
            cwd: &kept,
            born: None,
        },
        Parent {
            name: "Design",
            cwd: &orphan,
            born: None,
        },
    ]);

    assert!(
        said.contains("\"Frontend\" on branch"),
        "the copy that has a branch has to be named: {said}"
    );
    assert!(
        !said.contains("\"Design\" on branch"),
        "the copy without a branch was given an address anyway: {said}"
    );
    Ok(())
}

/// Kontrola przeciw pustej scenie: bez tego cała asercja mogłaby stać na literówce w ścieżce.
#[test]
fn the_scene_really_seeds_a_marker() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let marker = bench.run_dir.join(".isolation").join(ON_A_BRANCH);
    assert!(
        Path::new(&marker).is_file(),
        "the marker this whole file reasons about was not written"
    );
    Ok(())
}
