//! Z-46: katalog roboczy po biegu SPRZED znacznika izolacji jest nazwany w opisie tego biegu.
//!
//! # Co było zepsute
//!
//! Domykanie drzew przy otwarciu folderu (Z-9, `commands::reconcile::close_what_the_runs_left`)
//! chodzi po `<bieg>/.isolation/<klucz>` — po notatce, którą bieg pisze o każdym katalogu, jaki
//! sobie otworzył. Bieg sprzed tej notatki nie ma jej wcale, więc jego katalog roboczy jest dla
//! tamtej pętli **niewidzialny**: nie zamyka go i — co gorsze — nie mówi o nim ani słowa.
//!
//! Zmierzone u właściciela 2026-09-03 na `urc-monorepo`: dziennik zameldował „75 folder(s)
//! closed", a `git worktree list` dalej wymieniał dwanaście katalogów `work/s_*`, po 264 MB każdy.
//!
//! # DLACZEGO TO JEST OSOBNY MODUŁ (AGENTS.md §2a punkt 4)
//!
//! Bo ten jeden przypadek ma się skompilować i PAŚĆ na drzewie sprzed poprawki, a filtr nazwy
//! testu nie wyłącza kompilacji niczego: `cargo test --test it <moduł>::` buduje CAŁY cel `it`,
//! razem z każdym sąsiednim modułem. Moduł, w którym stoją pozostałe kryteria Z-46
//! (`old_runs_say_what_they_left`), woła nowe `commands::sweep` — więc gdyby ten przypadek stał
//! obok nich, na starym kodzie nie zbudowałby się w ogóle, a test, który się nie skompilował,
//! niczego nie uruchomił i niczego nie dowiódł.
//!
//! Ten plik używa więc **wyłącznie** dwóch funkcji, które istniały przed tą poprawką:
//! `reconcile::reconcile_runs` i `history::read_run_inner`. Ani jednego symbolu z `sweep`.
//!
//! # SŁABĄ WERSJĄ jest „liczba zwrócona z funkcji"
//!
//! Wartość dowodzi, że mechanizm istnieje; zdanie w opisie biegu — czytane TĄ SAMĄ komendą,
//! którą woła okno (`history::read_run_inner`) — dowodzi, że produkt działa (niezmiennik 29).
//! Dlatego ten przypadek nie pyta `reconcile_runs` o nic: pyta o to, co człowiek przeczyta.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::reconcile::reconcile_runs;

/// Katalog biegu i identyfikator w jego opisie. Nazwa gałęzi kroku bierze się z drugiego.
const RUN_FOLDER: &str = "20260823-090000__01a03200-0000-7000-8000-0000000000c1";
const RUN_ID: &str = "01a03200-0000-7000-8000-0000000000c1";

/// Klucz pracy kroku: nazwa katalogu w `work/`. Markera w `.isolation/` ten bieg NIE MA.
const KEY: &str = "s_2";

/// Nazwa kafelka, czyli to, po czym człowiek poznaje ten krok na ekranie (niezmiennik 14).
const STEP_NAME: &str = "Writes";

fn a_run() -> String {
    format!(
        r#"{{
  "id": "{RUN_ID}",
  "workflow_id": "sweep.json",
  "workflow_hash": "abc",
  "workflow_snapshot": {{ "format": 1 }},
  "title": "Sweep",
  "status": "succeeded",
  "concurrency": 1,
  "created_at": 1788000000000,
  "started_at": 1788000000000,
  "ended_at": 1788000060000,
  "error": null,
  "steps": [
    {{
      "id": "01a03200-0000-7000-8000-0000000000c2",
      "node_key": "{KEY}",
      "name": "{STEP_NAME}",
      "agent": "claude-code",
      "kind": "agent",
      "depends_on": [],
      "status": "succeeded",
      "started_at": 1788000000000,
      "ended_at": 1788000060000,
      "error": null
    }}
  ]
}}
"#
    )
}

#[test]
fn a_work_folder_from_before_the_marker_is_named_in_that_runs_record() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let head = bench.head()?;
    let branch = format!("loadout/{RUN_ID}/{KEY}");
    let work = bench.leave_a_tree_without_a_marker(&branch, &head)?;

    let done = reconcile_runs(bench.project());

    let past = read_run_inner(bench.project(), RUN_FOLDER)?;
    let step = past
        .steps
        .iter()
        .find(|one| one.name == STEP_NAME)
        .ok_or("the open run does not name that step at all")?;
    assert!(
        !step.error.is_empty(),
        "the run's record says nothing about the folder that step worked in, and that folder is \
         still on disk at {}. This run is older than the note Loadout now writes down for every \
         folder it opens, so the sweep at folder-open walks straight past it: it does not close \
         it and — this is the part that costs — it does not say it is there either. Measured on \
         the owner's monorepo: the log said 75 folders closed while twelve were still standing, \
         264 MB each. Settling the folder said: {done:?}",
        work.display()
    );
    assert!(
        step.error.contains(&work.display().to_string()),
        "the sentence does not say WHERE that folder is, so the person is told something is left \
         over and then has to find one folder among the ones every other run left. It said: {}",
        step.error
    );

    Ok(())
}

// ── ławka ─────────────────────────────────────────────────────────────────────────────────

struct Bench {
    project: tempfile::TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let bench = Self {
            project: tempfile::tempdir()?,
        };
        fs::write(bench.project().join("notes.txt"), "the person's own file")?;
        bench.git(&["init", "--quiet"])?;
        bench.git(&["add", "-A"])?;
        bench.git(&["commit", "--quiet", "-m", "the person's first commit"])?;
        Ok(bench)
    }

    fn project(&self) -> &Path {
        self.project.path()
    }

    /// Bieg skończony, którego katalog roboczy dalej stoi — i **bez** notatki w `.isolation/`.
    ///
    /// To jest cała różnica wobec ławki z `leftovers_are_swept_when_the_folder_opens.rs`: tamta
    /// pisze marker z palca, żeby domykanie miało co przeczytać, a tutaj markera nie ma, bo bieg,
    /// który ten katalog otworzył, jest starszy niż sam pomysł zapisywania takiej notatki.
    fn leave_a_tree_without_a_marker(
        &self,
        branch: &str,
        head: &str,
    ) -> Result<PathBuf, Box<dyn Error>> {
        let run_dir = self
            .project()
            .join(".loadout")
            .join("runs")
            .join(RUN_FOLDER);
        fs::create_dir_all(&run_dir)?;
        fs::write(run_dir.join("run.json"), a_run())?;

        let work = run_dir.join("work").join(KEY);
        self.git(&[
            "worktree",
            "add",
            "--quiet",
            "-b",
            branch,
            &work.display().to_string(),
            head,
        ])?;
        Ok(work)
    }

    fn head(&self) -> Result<String, Box<dyn Error>> {
        Ok(self.git(&["rev-parse", "HEAD"])?.trim().to_owned())
    }

    fn git(&self, args: &[&str]) -> Result<String, Box<dyn Error>> {
        let out = Command::new("git")
            .arg("-C")
            .arg(self.project())
            .args(["-c", "user.name=The Person"])
            .args(["-c", "user.email=person@loadout.invalid"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()?;
        if !out.status.success() {
            return Err(format!(
                "git {args:?} refused: {}",
                String::from_utf8_lossy(&out.stderr)
            )
            .into());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}
