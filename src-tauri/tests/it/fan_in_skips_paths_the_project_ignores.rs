//! Składanie nie sądzi plików, które projekt sam kazał gitowi pomijać.
//!
//! 2026-09-10 — INCYDENT „Murmur-1". Osiem kroków agentowych zeszło zielono przez 1 h 26 min,
//! po czym składanie odmówiło startu na `.angular/cache/22.0.8/meetnotes/.tsbuildinfo`: dwa
//! pasy odpaliły build i każdy zostawił swój cache kompilacji. Plik stoi w `.gitignore`
//! projektu i nie ma ani jednej kopii w gicie. Zabrał całą bramkę jakości i 32 USD.
//!
//! Kryterium jest sprawdzane na ZDANIU, które czyta człowiek (niezmiennik 29): `commands::run`
//! zamienia `Trouble` na `trouble.to_string()` i dokładnie ten napis ląduje w błędzie kroku
//! i na kafelku. Dlatego druga połowa testu wymaga, żeby kolizja na pliku ŚLEDZONYM nadal
//! mówiła człowiekowi to samo zdanie: wyrocznia ma się zawęzić, nie zniknąć.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use loadout_lib::commands::fan_in::{self, Parent, Trouble};
use loadout_lib::commands::input_snapshot::{self, InputSnapshot};

/// Ścieżka z incydentu, co do znaku. Nazwa katalogu jest treścią: `.angular` łapie ją
/// regułą ignorowania, a `NOT_COPIED` w `isolate` — nie, i to jest cała różnica.
const BUILD_CACHE: &str = ".angular/cache/22.0.8/app/.tsbuildinfo";
const SOURCE: &str = "app.ts";

struct Bench {
    _root: tempfile::TempDir,
    project: PathBuf,
    left: PathBuf,
    right: PathBuf,
    origin: InputSnapshot,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let project = root.path().join("project");
        let storage = root.path().join("run");
        fs::create_dir_all(&project)?;
        fs::create_dir_all(&storage)?;
        fs::write(project.join(".gitignore"), ".angular/\n.loadout/\n")?;
        fs::write(project.join(SOURCE), "original")?;
        git(&project, &["init", "--quiet"])?;
        git(&project, &["add", "-A"])?;
        git(&project, &["commit", "--quiet", "-m", "baseline"])?;
        let origin = input_snapshot::capture(&project, &storage)?;
        let left = root.path().join("left");
        let right = root.path().join("right");
        origin.materialize(&left)?;
        origin.materialize(&right)?;
        Ok(Self {
            _root: root,
            project,
            left,
            right,
            origin,
        })
    }

    /// Obie kopie piszą w tej samej ścieżce różne bajty — czyli dokładnie to, co robi build.
    fn both_write(&self, relative: &str, mine: &str, theirs: &str) -> Result<(), Box<dyn Error>> {
        for (copy, body) in [(&self.left, mine), (&self.right, theirs)] {
            let path = copy.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, body)?;
        }
        Ok(())
    }

    fn fold(&self) -> Result<Vec<PathBuf>, Trouble> {
        fan_in::plan_frozen(
            &[
                Parent {
                    name: "Frontend",
                    cwd: &self.left,
                    born: None,
                },
                Parent {
                    name: "Design",
                    cwd: &self.right,
                    born: None,
                },
            ],
            &self.origin,
            &self.project,
        )
        .map(|plan| plan.changes.into_iter().map(|one| one.path).collect())
    }
}

fn git(at: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let out = Command::new("git").arg("-C").arg(at).args(args).output()?;
    if !out.status.success() {
        return Err(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(out.stdout)?)
}

#[test]
fn two_copies_disagreeing_only_on_an_ignored_path_still_combine() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.both_write(BUILD_CACHE, "left build", "right build")?;

    let changed = bench.fold().map_err(|trouble| {
        format!("the step was refused over a file the project ignores: {trouble}")
    })?;

    assert!(
        !changed.iter().any(|path| path == Path::new(BUILD_CACHE)),
        "the ignored artifact was carried into the combined copy: {changed:?}"
    );
    Ok(())
}

#[test]
fn two_copies_disagreeing_on_a_real_file_still_say_so() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.both_write(BUILD_CACHE, "left build", "right build")?;
    bench.both_write(SOURCE, "left work", "right work")?;

    let Err(trouble) = bench.fold() else {
        return Err("two copies disagreed on a real file and were combined anyway".into());
    };
    let said = trouble.to_string();

    assert!(
        said.contains(SOURCE)
            && said.contains("Frontend")
            && said.contains("Design")
            && said.contains("both changed"),
        "the person was not told which two tiles disagreed about which file: {said}"
    );
    assert!(
        !said.contains(".tsbuildinfo"),
        "the person was pointed at a build artifact instead of their own file: {said}"
    );
    Ok(())
}
