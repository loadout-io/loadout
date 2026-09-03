//! Z-9: drzewo, które zostało po biegu, domyka się przy pierwszym dotknięciu folderu.
//!
//! # Co było zepsute
//!
//! Sprzątanie po biegu istniało wyłącznie dla biegu, który właśnie się kończy
//! (`commands::run::close_the_trees`). Bieg, który zginął razem z aplikacją, zostawiał swój
//! katalog roboczy na dysku razem z pełnym checkoutem repozytorium, wpisem w rejestrze gita
//! i pracą agenta, która nigdy nie doszła na gałąź. Uzgodnienie przy otwarciu folderu
//! (`commands::reconcile`) przepisywało `run.json` i dobijało grupy procesów — a drzew nie
//! tykało ani razu.
//!
//! Zmierzone u właściciela 2026-09-02 na `urc-monorepo`: 87 katalogów `work/`, 89 wpisów
//! w rejestrze gita, 99 gałęzi `loadout/*`, 3,8 GB.
//!
//! # SŁABĄ WERSJĄ jest samo `assert!(!work.exists())`
//!
//! Przechodzi ją `remove_dir_all`, czyli skasowanie jedynej kopii pracy agenta razem z wpisem
//! zostawionym w rejestrze gita — a taki wpis odmawia potem założenia drzewa pod tą samą
//! ścieżką. Dlatego w tym samym teście stoi commit na gałęzi kroku (praca jest osiągalna
//! z gita) i czysty rejestr (następny bieg pod tą ścieżką ruszy).

use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command;

use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::reconcile::reconcile_runs;

/// Katalog biegu i identyfikator w jego opisie. Nazwa gałęzi kroku bierze się z drugiego.
const RUN_FOLDER: &str = "20260901-120000__01a03000-0000-7000-8000-0000000000f1";
const RUN_ID: &str = "01a03000-0000-7000-8000-0000000000f1";

/// Klucz pracy kroku: nazwa katalogu w `work/` i nazwa pliku w `.isolation/`.
const KEY: &str = "s_1";

/// Nazwa kafelka, czyli to, po czym człowiek poznaje ten krok na ekranie (niezmiennik 14).
const STEP_NAME: &str = "Writes";

/// Plik, który krok zdążył napisać, i którego nikt nie zacommitował.
const MADE: &str = "the-work.txt";
const MADE_TEXT: &str = "this is what the step wrote before the app went away";

/// Nasz własny katalog w projekcie. Człowiek nie ma go widzieć w swoim `git status`.
const OURS: &str = ".loadout/";

fn a_run() -> String {
    format!(
        r#"{{
  "id": "{RUN_ID}",
  "workflow_id": "sweep.json",
  "workflow_hash": "abc",
  "workflow_snapshot": {{ "format": 1 }},
  "title": "Sweep",
  "status": "interrupted",
  "concurrency": 1,
  "created_at": 1788000000000,
  "started_at": 1788000000000,
  "ended_at": 1788000060000,
  "error": null,
  "steps": [
    {{
      "id": "01a03000-0000-7000-8000-0000000000f2",
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

// ── (1) drzewo po przerwanym biegu ────────────────────────────────────────────────────────

#[test]
fn a_tree_left_by_an_interrupted_run_is_closed_when_the_folder_opens() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let head = bench.head()?;
    let branch = format!("loadout/{RUN_ID}/{KEY}");
    let work = bench.leave_a_tree(&branch, &head)?;

    let done = reconcile_runs(bench.project());

    assert!(
        !work.exists(),
        "the folder the step worked in is still on disk at {}. It holds a whole checkout of the \
         repository and it is not going anywhere by itself: nothing but the run that made it ever \
         looked at it, and that run is over. Settling the folder said: {done:?}",
        work.display()
    );

    // Praca jest OSIĄGALNA Z GITA, nie skasowana. Bez tej asercji całe kryterium spełnia
    // `remove_dir_all`, czyli zdjęcie jedynej kopii tego, co agent zdążył napisać.
    let subjects = bench.git(&["log", "--format=%s", &format!("{head}..{branch}")])?;
    let subjects: Vec<&str> = subjects
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(
        subjects.len(),
        1,
        "`git log {head}..{branch}` has to name exactly one commit carrying the work that was \
         left in that folder. It said: {subjects:?}"
    );
    let saved = bench.git(&["show", &format!("{branch}:{MADE}")])?;
    assert_eq!(
        saved.trim(),
        MADE_TEXT,
        "the branch is there and the work that was in the folder is not in it"
    );

    // I rejestr jest czysty. Skasowany katalog ze wpisem, który po nim zostaje, odmawia
    // założenia drzewa pod tą samą ścieżką — czyli następnego biegu tego kroku.
    let listed = bench.git(&["worktree", "list", "--porcelain"])?;
    assert!(
        !listed.contains(&work.display().to_string()),
        "git still lists {} as a place to work, so deleting the folder was not enough: the next \
         run under that path is refused. The list says: {listed}",
        work.display()
    );

    Ok(())
}

// ── (1b) katalog, którego nie dało się zdjąć, jest NAZWANY w opisie biegu ─────────────────

/// Niezmiennik 29: zdanie o nieudanym sprzątaniu czyta się tam, gdzie człowiek o biegu czyta.
///
/// Wartość zwrócona przez `isolate::finish` dowodzi, że mechanizm istnieje. Zdanie w opisie
/// otwartego biegu — czytane TĄ SAMĄ komendą, którą woła okno — dowodzi, że produkt działa.
/// Między jednym a drugim mieszka klasa wady, dla której to repo powstało: katalog z pełnym
/// odpisem repozytorium stoi w projekcie, następny bieg tego kroku jest przez niego odmawiany,
/// a bieg czyta się zielono.
#[test]
fn a_folder_that_could_not_be_cleared_away_is_named_in_the_run_record() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let head = bench.head()?;
    let branch = format!("loadout/{RUN_ID}/{KEY}");
    let work = bench.leave_a_tree(&branch, &head)?;
    // ZAMEK GITA to jedyny znany sposób, żeby `worktree remove --force` odmówił
    // DETERMINISTYCZNIE: zamek zdejmuje dopiero drugie `--force`, a `isolate::remove_tree` podaje
    // jedno. Bez niego to kryterium mierzyłoby ścieżkę, której nie da się wywołać.
    bench.git(&["worktree", "lock", &work.display().to_string()])?;

    let done = reconcile_runs(bench.project());

    let past = read_run_inner(bench.project(), RUN_FOLDER)?;
    let step = past
        .steps
        .iter()
        .find(|one| one.name == STEP_NAME)
        .ok_or("the open run does not name that step at all")?;
    assert!(
        !step.error.is_empty(),
        "the run's record says nothing about the folder that could not be cleared away. A green \
         run over a folder holding a whole checkout of the repository is a leftover nobody looks \
         for: nothing mentioned it, and the next run under that path is refused. Settling the \
         folder said: {done:?}"
    );
    assert!(
        step.error.contains(&work.display().to_string()),
        "the sentence does not say WHERE the leftover is, so the person is told something went \
         wrong and then has to find one folder among the ones every other run left. It said: {}",
        step.error
    );

    Ok(())
}

// ── (5) nasz katalog nie zaśmieca `git status` człowieka ──────────────────────────────────

#[test]
fn our_own_folder_stays_out_of_the_persons_git_status() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let head = bench.head()?;
    let branch = format!("loadout/{RUN_ID}/{KEY}");
    bench.leave_a_tree(&branch, &head)?;

    let done = reconcile_runs(bench.project());

    let excluded = fs::read_to_string(bench.project().join(".git").join("info").join("exclude"))?;
    assert!(
        excluded.lines().any(|line| line.trim() == OURS),
        "the project's own list of things git ignores does not mention {OURS}, so every run \
         Loadout leaves in that folder shows up as the person's own untracked work. Settling the \
         folder said {done:?} and the list said: {excluded}"
    );

    // ZDANIE JEST W `git status`, NIE W PLIKU: plik dowodzi mechanizmu, a to jest ta jedna
    // odpowiedź, którą człowiek naprawdę czyta.
    let untracked = bench.git(&["status", "--porcelain"])?;
    assert!(
        !untracked.contains(".loadout"),
        "`git status` still shows the person our own folder, so the first thing they read after a \
         run is a list of files they did not write. It said: {untracked}"
    );

    // A `.gitignore` człowieka zostaje NIETKNIĘTY. To jest jego plik, w jego commicie —
    // dopisanie tam czegokolwiek jest zmianą, której nie zamawiał.
    assert!(
        !bench.project().join(".gitignore").exists(),
        "Loadout wrote into the person's own .gitignore. That file is theirs and it is committed; \
         our folder belongs in the list only git itself reads"
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

    /// Bieg w stanie terminalnym, którego drzewo dalej stoi na dysku — dokładnie to, co zostaje
    /// po aplikacji ubitej w trakcie pracy.
    ///
    /// Marker izolacji pisze bieg (`commands::run::write_isolation_marker`) i to z niego bierze
    /// się nazwa gałęzi oraz punkt startu. Fikstura składa go z palca, bo bieg, który go napisał,
    /// już nie żyje.
    fn leave_a_tree(&self, branch: &str, head: &str) -> Result<std::path::PathBuf, Box<dyn Error>> {
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
        fs::write(work.join(MADE), MADE_TEXT)?;

        let markers = run_dir.join(".isolation");
        fs::create_dir_all(&markers)?;
        fs::write(
            markers.join(KEY),
            format!(r#"{{"state":"complete","branch":"{branch}","head":"{head}"}}"#),
        )?;
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
