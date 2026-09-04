//! Z-9: „forget this run" zdejmuje gałęzie biegu **i jego katalog** — albo nie zdejmuje nic.
//!
//! # Co było zepsute
//!
//! Gałęzie biegu dało się zdjąć od T-95 (`history::forget_run_branches_inner`). Jego katalog —
//! ze strumieniami agentów, przekazaniami i kopiami notatek — nie schodził **niczym**: ani po
//! biegu, ani przy otwarciu folderu, ani przyciskiem. Zmierzone u właściciela 2026-09-02 na
//! `urc-monorepo`: 87 katalogów biegów, 3,8 GB, a jedyną drogą był `rm -rf` z terminala.
//!
//! # Trzy przypadki, bo to są trzy różne obietnice
//!
//! 1. **Skutek.** Bieg nie jest już wymieniany przez `list_runs_inner`, jego katalogu nie ma,
//!    a `git branch` nie zna ani jednej `loadout/<bieg>/*`.
//! 2. **Odmowa jest CAŁOŚCIOWA.** Kiedy którakolwiek gałąź tego biegu jest w tej chwili wyjęta do
//!    pracy w innym drzewie, nie znika ANI JEDNA rzecz — ani gałąź, ani katalog — a zdanie odmowy
//!    nazywa tę gałąź. Słabą wersją jest tu „katalog zniknął, gałęzie zostały": człowiek czyta
//!    wtedy „nic nie ruszyłem" nad projektem, w którym zniknęła już historia biegu.
//! 3. **Retencja idzie tą samą drogą.** „Zostaw N ostatnich" nie jest drugim sprzątaniem obok
//!    przycisku — to ta sama funkcja zawołana bez człowieka, więc dziedziczy tę samą ostrożność.
//!    Kryterium woła szew, przez który przechodzi każda komenda dotykająca projektu
//!    (`AppState::project_for`), bo naprawa wpięta w kod bez wołających raz już w tym repo
//!    wylądowała i wyglądała na zrobioną.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use loadout_lib::commands::Drivers;
use loadout_lib::commands::history::{forget_run_inner, list_runs_inner};
use loadout_lib::engine::drivers::AgentDriver;
use loadout_lib::ipc::AppState;
use loadout_lib::store::Store;

/// Bieg, o którym człowiek każe zapomnieć.
const RUN_FOLDER: &str = "20260901-120000__01a03100-0000-7000-8000-0000000000e1";
const RUN_ID: &str = "01a03100-0000-7000-8000-0000000000e1";

/// Klucz kafelka. Ostatni człon nazwy gałęzi tego biegu.
const KEY: &str = "s_1";

/// Plik w katalogu biegu, którego po zapomnieniu nie ma razem z katalogiem.
const KEPT: &str = "handoffs/build.md";

fn a_run(folder: &str, id: &str) -> String {
    format!(
        r#"{{
  "id": "{id}",
  "workflow_id": "sweep.json",
  "workflow_hash": "abc",
  "workflow_snapshot": {{ "format": 1 }},
  "title": "Sweep {folder}",
  "status": "succeeded",
  "concurrency": 1,
  "created_at": 1788000000000,
  "started_at": 1788000000000,
  "ended_at": 1788000060000,
  "error": null,
  "steps": [
    {{
      "id": "01a03100-0000-7000-8000-0000000000e2",
      "node_key": "{KEY}",
      "name": "Writes",
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

// ── (1) skutek ────────────────────────────────────────────────────────────────────────────

#[test]
fn forgetting_a_run_takes_its_branches_and_its_folder() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let dir = bench.put_a_run(RUN_FOLDER, RUN_ID)?;
    let branch = format!("loadout/{RUN_ID}/{KEY}");
    bench.leave_a_branch(&branch)?;

    let gone = forget_run_inner(bench.project(), RUN_FOLDER)?;

    assert_eq!(
        gone,
        vec![branch.clone()],
        "forgetting the run has to answer with the branches it really took away, because that \
         list is the only thing the window can put on the screen instead of guessing"
    );
    assert!(
        !dir.exists(),
        "the folder of the run is still on disk at {}. Until this change nothing in the whole app \
         could take it away, and it carries every stream, every handover and every note copy of \
         that run",
        dir.display()
    );
    assert!(
        !bench.branches()?.iter().any(|name| name == &branch),
        "the branch {branch} is still here, so half of the run was forgotten and the other half \
         stayed — in `git branch`, where nothing points at it any more"
    );

    // I CZŁOWIEK TEGO BIEGU JUŻ NIE WIDZI, zapytany tą samą komendą, którą pyta okno.
    let listed = list_runs_inner(bench.project());
    assert!(
        !listed.iter().any(|one| one.folder == RUN_FOLDER),
        "the history still lists a run whose folder is gone, so the person clicks a row that \
         cannot open. It listed: {:?}",
        listed.iter().map(|one| &one.folder).collect::<Vec<_>>()
    );

    Ok(())
}

// ── (2) odmowa, kiedy ktoś na tej gałęzi pracuje ─────────────────────────────────────────

#[test]
fn a_run_whose_branch_is_open_somewhere_else_loses_nothing() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let dir = bench.put_a_run(RUN_FOLDER, RUN_ID)?;
    let branch = format!("loadout/{RUN_ID}/{KEY}");
    // Gałąź WYJĘTA DO PRACY w innym drzewie: dokładnie ten stan, w którym zdjęcie jej spod
    // czyjejś ręki jest jedyną rzeczą, którą ta droga mogłaby zepsuć nieodwracalnie.
    bench.check_the_branch_out_elsewhere(&branch)?;

    let refused = forget_run_inner(bench.project(), RUN_FOLDER)
        .err()
        .ok_or("forgetting a run with a branch open in another folder was allowed")?;

    let said = refused.to_string();
    assert!(
        said.contains(&branch),
        "the refusal does not name the branch somebody is working on, so the person is told no \
         and left to find out which of the branches it was. It said: {said}"
    );
    assert!(
        bench.branches()?.iter().any(|name| name == &branch),
        "the refusal took the branch away anyway"
    );
    assert!(
        dir.exists(),
        "the refusal took the folder of the run away, so the person reads \"I touched nothing\" \
         over a project where the whole history of that run is already gone"
    );
    assert!(
        dir.join(KEPT).exists(),
        "what the run left inside its folder is gone, even though nothing was supposed to happen"
    );
    assert!(
        list_runs_inner(bench.project())
            .iter()
            .any(|one| one.folder == RUN_FOLDER),
        "the run is no longer listed after a refusal that changed nothing"
    );

    Ok(())
}

// ── (3) retencja przy otwarciu folderu ──────────────────────────────────────────────────

/// Ile biegów człowiek prosił zostawić. Cztery na dysku, dwa najświeższe zostają.
const KEEP: usize = 2;

/// Biegi od najstarszego. Nazwa katalogu otwiera się znacznikiem czasu UTC, więc ten porządek
/// jest porządkiem czasu.
const FOUR_RUNS: [(&str, &str); 4] = [
    (
        "20260901-100000__01a03100-0000-7000-8000-0000000000a1",
        "01a03100-0000-7000-8000-0000000000a1",
    ),
    (
        "20260901-110000__01a03100-0000-7000-8000-0000000000a2",
        "01a03100-0000-7000-8000-0000000000a2",
    ),
    (
        "20260901-120000__01a03100-0000-7000-8000-0000000000a3",
        "01a03100-0000-7000-8000-0000000000a3",
    ),
    (
        "20260901-130000__01a03100-0000-7000-8000-0000000000a4",
        "01a03100-0000-7000-8000-0000000000a4",
    ),
];

#[tokio::test]
async fn opening_a_folder_keeps_only_the_last_runs_a_person_asked_for() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    for (folder, id) in FOUR_RUNS {
        bench.put_a_run(folder, id)?;
        // 2026-09 (Z-46) — KAŻDY BIEG ZOSTAWIA TU GAŁĄŹ, bo bez niej to kryterium mierzy połowę
        // retencji. Zmierzone u właściciela na `urc-monorepo`: 99 gałęzi `loadout/*` przy
        // czternastu biegach — katalogi schodziły, a gałęzie zostawały na zawsze.
        bench.leave_a_branch(&format!("loadout/{id}/{KEY}"))?;
    }
    // WYBÓR CZŁOWIEKA CZYTAMY Z PLIKU, tą samą drogą, którą go czyta aplikacja: `read_settings`
    // patrzy w bibliotekę, nie w stan okna.
    bench.ask_to_keep(KEEP)?;

    let state = AppState::new(
        bench.home.path().to_path_buf(),
        bench.project().to_path_buf(),
        Store::open(&bench.home.path().join("index.db"))?,
        no_drivers(),
    );
    // SZEW, PRZEZ KTÓRY IDZIE KAŻDA KOMENDA DOTYKAJĄCA PROJEKTU. Wpięcie retencji w kod bez
    // wołających wyglądałoby dokładnie tak samo z zewnątrz — i raz już tak w tym repo wylądowało.
    state
        .project_for(None)
        .await
        .map_err(|said| format!("the window could not even name its own folder: {said}"))?;

    let left: Vec<String> = list_runs_inner(bench.project())
        .into_iter()
        .map(|one| one.folder)
        .collect();
    let newest: Vec<&str> = FOUR_RUNS
        .iter()
        .rev()
        .take(KEEP)
        .map(|(folder, _)| *folder)
        .collect();
    assert_eq!(
        left, newest,
        "the folder has to keep exactly the {KEEP} newest runs the person asked for, and the \
         newest ones. Keeping the oldest is the same size on disk and the wrong history"
    );
    for (folder, _) in FOUR_RUNS.iter().take(FOUR_RUNS.len() - KEEP) {
        assert!(
            !bench.run_dir(folder).exists(),
            "{folder} is off the list and still on disk, so the history says one thing and the \
             disk another — and nothing will ever come back for it"
        );
    }

    /* GAŁĘZIE SCHODZĄ RAZEM Z KATALOGIEM, i to jest druga połowa tego, o co człowiek prosi
     * suwakiem (2026-09, Z-46). Katalog zdjęty bez gałęzi jest stanem, o którym człowiek
     * dowiaduje się dopiero z `git branch`: historia mówi „zostały dwa biegi", a repozytorium
     * niesie gałęzie wszystkich czterech i nic już nie umie ich nazwać — bo przedrostek liczy
     * się z `run.json`, którego właśnie nie ma. */
    let branches = bench.branches()?;
    for (folder, id) in FOUR_RUNS.iter().take(FOUR_RUNS.len() - KEEP) {
        let branch = format!("loadout/{id}/{KEY}");
        assert!(
            !branches.contains(&branch),
            "{folder} was forgotten and its branch {branch} is still in this repository. Nothing \
             names it any more: the prefix that identifies a run's branches is read out of the \
             run's own record, and that record is gone. Branches left: {branches:?}"
        );
    }
    for (_, id) in FOUR_RUNS.iter().skip(FOUR_RUNS.len() - KEEP) {
        let branch = format!("loadout/{id}/{KEY}");
        assert!(
            branches.contains(&branch),
            "the branch {branch} of a run the person asked to KEEP was taken away as well, so \
             retention reaches past what it was asked for. Branches left: {branches:?}"
        );
    }

    Ok(())
}

// ── (4) zero znaczy „wszystkie", więc nie schodzi ani katalog, ani gałąź ───────────────────

/// Wartość, którą `serde` wstawia KAŻDEMU dzisiejszemu plikowi wyborów, czyli stan zastany
/// u każdego, kto tego suwaka nigdy nie dotknął.
const KEEP_EVERYTHING: usize = 0;

#[tokio::test]
async fn keeping_everything_takes_neither_a_folder_nor_a_branch() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    for (folder, id) in FOUR_RUNS {
        bench.put_a_run(folder, id)?;
        bench.leave_a_branch(&format!("loadout/{id}/{KEY}"))?;
    }
    bench.ask_to_keep(KEEP_EVERYTHING)?;

    let state = AppState::new(
        bench.home.path().to_path_buf(),
        bench.project().to_path_buf(),
        Store::open(&bench.home.path().join("index.db"))?,
        no_drivers(),
    );
    state
        .project_for(None)
        .await
        .map_err(|said| format!("the window could not even name its own folder: {said}"))?;

    let left: Vec<String> = list_runs_inner(bench.project())
        .into_iter()
        .map(|one| one.folder)
        .collect();
    assert_eq!(
        left.len(),
        FOUR_RUNS.len(),
        "opening the folder forgot runs nobody asked it to forget. Zero in that setting is the \
         value serde writes into every file today, so reading it as \"keep none\" would take the \
         whole history of every project on this machine on the next start. It left: {left:?}"
    );
    let branches = bench.branches()?;
    for (_, id) in FOUR_RUNS {
        let branch = format!("loadout/{id}/{KEY}");
        assert!(
            branches.contains(&branch),
            "and the branch {branch} was taken away even though no run was. Branches left: \
             {branches:?}"
        );
    }

    Ok(())
}

/// Sterownik, którego to kryterium nie woła. Musi istnieć, bo [`AppState`] go trzyma.
fn no_drivers() -> Drivers {
    std::sync::Arc::new(|_vendor| -> std::sync::Arc<dyn AgentDriver> {
        unreachable!("asking a folder for its path never starts an agent")
    })
}

// ── ławka ─────────────────────────────────────────────────────────────────────────────────

struct Bench {
    home: tempfile::TempDir,
    project: tempfile::TempDir,
    /// Drzewo obok, w którym da się wyjąć gałąź do pracy. Poza projektem, bo tak wygląda
    /// prawdziwa druga kopia repozytorium.
    elsewhere: tempfile::TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let bench = Self {
            home: tempfile::tempdir()?,
            project: tempfile::tempdir()?,
            elsewhere: tempfile::tempdir()?,
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

    fn run_dir(&self, folder: &str) -> PathBuf {
        self.project().join(".loadout").join("runs").join(folder)
    }

    fn put_a_run(&self, folder: &str, id: &str) -> Result<PathBuf, Box<dyn Error>> {
        let dir = self.run_dir(folder);
        fs::create_dir_all(dir.join("handoffs"))?;
        fs::write(dir.join("run.json"), a_run(folder, id))?;
        fs::write(dir.join(KEPT), "what the step handed on")?;
        Ok(dir)
    }

    /// Gałąź, jaką zostawia po sobie krok — bez drzewa, bo drzewo znika zaraz po biegu.
    fn leave_a_branch(&self, branch: &str) -> Result<(), Box<dyn Error>> {
        self.git(&["branch", branch])?;
        Ok(())
    }

    /// Ta sama gałąź, ale WYJĘTA do pracy w drzewie obok.
    fn check_the_branch_out_elsewhere(&self, branch: &str) -> Result<(), Box<dyn Error>> {
        let at = self.elsewhere.path().join("open");
        self.git(&[
            "worktree",
            "add",
            "--quiet",
            "-b",
            branch,
            &at.display().to_string(),
        ])?;
        Ok(())
    }

    /// Zapisuje w bibliotece, ile ostatnich biegów ma zostawać w folderze projektu.
    fn ask_to_keep(&self, how_many: usize) -> Result<(), Box<dyn Error>> {
        fs::write(
            self.home.path().join("settings.json"),
            format!(
                r#"{{ "defaultLead": "", "defaultBudgetUsd": 75.0, "navCollapsed": false, "keepLastRuns": {how_many} }}"#
            ),
        )?;
        Ok(())
    }

    fn branches(&self) -> Result<Vec<String>, Box<dyn Error>> {
        Ok(self
            .git(&["branch", "--format=%(refname:short)"])?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect())
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
