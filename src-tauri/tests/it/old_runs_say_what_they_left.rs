//! Z-46: co zostawiły biegi, których Loadout **nie zamknął** — i jedno wyjście z tego stanu.
//!
//! # Co było zepsute
//!
//! Domykanie drzew przy otwarciu folderu (Z-9, `commands::reconcile::close_what_the_runs_left`)
//! chodzi po `<bieg>/.isolation/<klucz>` — po notatce, którą bieg pisze o każdym katalogu, jaki
//! sobie otworzył. Bieg sprzed tej notatki nie ma jej wcale, więc jego katalog roboczy jest dla
//! tamtej pętli **niewidzialny**: nie zamyka go i — co gorsze — nie mówi o nim ani słowa.
//!
//! Zmierzone u właściciela 2026-09-03 na `urc-monorepo`: dziennik zameldował „75 folder(s)
//! closed", a `git worktree list` dalej wymieniał dwanaście katalogów `work/s_*`, po 264 MB
//! każdy. Gałęzi `loadout/*` stało tam 99 przy czternastu biegach.
//!
//! # CZEGO TU NIE MA, I DLACZEGO (AGENTS.md §2a punkt 4)
//!
//! Zdania w opisie biegu — czyli tego jednego kryterium, które ma paść na drzewie SPRZED tej
//! poprawki — nie ma w tym module, tylko w `a_folder_from_before_the_marker_is_named`. Powód stoi
//! w nagłówku tamtego pliku i jest mechaniczny: `cargo test --test it <moduł>::` buduje CAŁY cel
//! `it`, więc jeden `use loadout_lib::commands::sweep::…` tutaj przewróciłby tamten przypadek na
//! kompilacji, zamiast dać mu paść na asercji. Ten moduł wolno więc oprzeć o `sweep`; tamten nie.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use loadout_lib::commands::history::list_runs_inner;
use loadout_lib::commands::sweep::{
    forget_runs_older_than, forget_what_the_old_runs_left, what_this_folder_could_forget,
};

/// Katalog biegu i identyfikator w jego opisie. Nazwa gałęzi kroku bierze się z drugiego.
const RUN_FOLDER: &str = "20260823-090000__01a03200-0000-7000-8000-0000000000c1";
const RUN_ID: &str = "01a03200-0000-7000-8000-0000000000c1";

/// Klucz pracy kroku: nazwa katalogu w `work/`. Markera w `.isolation/` ten bieg NIE MA.
const KEY: &str = "s_2";

/// Nazwa kafelka, czyli to, po czym człowiek poznaje ten krok na ekranie (niezmiennik 14).
const STEP_NAME: &str = "Writes";

fn a_run(id: &str) -> String {
    format!(
        r#"{{
  "id": "{id}",
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

// ── (2) folder mówi, ILE tego jest ────────────────────────────────────────────────────────

/// Bieg, po którym została sama gałąź: jego katalogu nie ma już na dysku.
const GONE_RUN: &str = "01a03200-0000-7000-8000-0000000000d1";

#[test]
fn the_folder_says_how_many_folders_and_branches_the_old_runs_left() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let head = bench.head()?;
    let work = bench.leave_a_tree_without_a_marker(
        RUN_FOLDER,
        &format!("loadout/{RUN_ID}/{KEY}"),
        &head,
    )?;
    // Dwie gałęzie po biegu, po którym nie ma już żadnego katalogu — dokładnie ten stan, w którym
    // u właściciela stoi 99 gałęzi przy czternastu biegach.
    bench.git(&["branch", &format!("loadout/{GONE_RUN}/s_1")])?;
    bench.git(&["branch", &format!("loadout/{GONE_RUN}/s_2")])?;

    let could = what_this_folder_could_forget(bench.project(), 30);

    assert_eq!(
        (could.work_folders, could.branches),
        (1, 2),
        "the folder has to count what the runs Loadout did not close left behind: one work \
         folder ({}) and two branches of a run whose folder is gone. Until this change nothing \
         counted them at all, and `git worktree list` was the only place they appeared. It said: \
         {could:?}",
        work.display()
    );
    assert!(
        could.said.contains("1 work folder") && could.said.contains("2 branches"),
        "and it has to say it in one sentence with both real numbers in it, because that \
         sentence is the whole of what a person ever learns about this. It said: {}",
        could.said
    );

    Ok(())
}

// ── (3) „Forget them" zdejmuje TYLKO to, co nie niesie własnej pracy ───────────────────────

/// Bieg, którego katalog roboczy człowiek zostawił brudny — jego zmiany nie ma nigdzie indziej.
const DIRTY_FOLDER: &str = "20260823-100000__01a03200-0000-7000-8000-0000000000e1";
const DIRTY_RUN: &str = "01a03200-0000-7000-8000-0000000000e1";

/// Bieg, po którym została gałąź z commitem agenta — i drugi, po którym pusta.
const CARRIES_RUN: &str = "01a03200-0000-7000-8000-0000000000f1";

#[test]
fn forgetting_them_leaves_everything_that_carries_its_own_work() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let head = bench.head()?;

    // Czysty katalog: nie ma w nim nic, czego nie da się odtworzyć, więc schodzi.
    let clean = bench.leave_a_tree_without_a_marker(
        RUN_FOLDER,
        &format!("loadout/{RUN_ID}/{KEY}"),
        &head,
    )?;
    // Brudny: leży w nim zmiana, której nikt nie zapisał. Gałąź jej NIE niesie.
    let dirty = bench.leave_a_tree_without_a_marker(
        DIRTY_FOLDER,
        &format!("loadout/{DIRTY_RUN}/{KEY}"),
        &head,
    )?;
    fs::write(
        dirty.join("half-done.txt"),
        "what the agent had written so far",
    )?;

    // Gałąź z commitem, którego nie ma nigdzie indziej — to jest cała praca tamtego kroku.
    let carries = format!("loadout/{CARRIES_RUN}/s_1");
    bench.commit_on_a_branch(&carries, "the agent saved this itself")?;
    // I gałąź, na której nie ma nic ponad `HEAD` projektu.
    let empty = format!("loadout/{GONE_RUN}/s_1");
    bench.git(&["branch", &empty])?;

    let done = forget_what_the_old_runs_left(bench.project());

    assert!(
        !clean.exists(),
        "the clean work folder is still on disk at {}, so pressing the one control this state has \
         did nothing about it. It said: {done:?}",
        clean.display()
    );
    assert!(
        dirty.exists(),
        "the work folder holding a change nobody saved was taken away, and that change existed \
         nowhere else: not on its branch, not in git, nowhere. This is the one thing this control \
         may never do"
    );
    let branches = bench.branches()?;
    assert!(
        branches.contains(&carries),
        "the branch carrying a commit this project does not have was deleted, so the only copy of \
         what that agent wrote is now reachable by nothing but `git fsck`. Branches left: \
         {branches:?}"
    );
    assert!(
        !branches.contains(&empty),
        "the branch that carries nothing this project does not already have is still here, so the \
         control leaves behind exactly the rows it exists to clear. Branches left: {branches:?}"
    );

    // I ZDANIE NAZYWA TO, CO ZOSTAŁO — po imieniu i ze ścieżką. Bez tego człowiek naciska drugi
    // raz nad tym samym stanem i nie dowiaduje się, dlaczego liczby nie doszły do zera.
    assert!(
        done.said.contains(&dirty.display().to_string()),
        "the answer does not say WHERE the folder it left alone is, so the person is told \
         something stayed and has to find it among every other run's folder. It said: {}",
        done.said
    );
    assert!(
        done.said.contains(&carries),
        "and it does not name the branch it left alone either, so the one branch holding real \
         work reads exactly like the ones that went. It said: {}",
        done.said
    );

    Ok(())
}

// ── (4) „Forget runs older than N days" zdejmuje bieg razem ze wszystkim, co po nim zostało ──

/// Bieg sprzed lat. Data w nazwie jest jedyną, jaką ktokolwiek czyta (`history::when_of`).
const LONG_AGO_FOLDER: &str = "20200101-000000__01a03200-0000-7000-8000-0000000000a1";
const LONG_AGO_RUN: &str = "01a03200-0000-7000-8000-0000000000a1";

/// I bieg z dzisiaj — ten ma zostać, cokolwiek wskazuje zegar w dniu, w którym to biegnie.
const TODAY_RUN: &str = "01a03200-0000-7000-8000-0000000000b1";

/// Ile dni ma mieć bieg, żeby zejść. Jeden, bo bieg z dzisiaj ma zero i zostaje przy każdym
/// uruchomieniu tego kryterium, a nie tylko w dniu, w którym je napisano.
const OLDER_THAN: u32 = 1;

#[test]
fn forgetting_runs_older_than_that_takes_their_branches_and_folders_too()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let head = bench.head()?;
    let long_ago = bench.leave_a_tree_without_a_marker(
        LONG_AGO_FOLDER,
        &format!("loadout/{LONG_AGO_RUN}/{KEY}"),
        &head,
    )?;
    let today = today_folder(TODAY_RUN);
    bench.leave_a_tree_without_a_marker(&today, &format!("loadout/{TODAY_RUN}/{KEY}"), &head)?;

    // NAJPIERW ZDANIE, POTEM SKUTEK — w tej kolejności, bo w tej kolejności czyta to człowiek:
    // nic tu nie schodzi, dopóki nie przeczyta, co zejdzie.
    let could = what_this_folder_could_forget(bench.project(), OLDER_THAN);
    assert_eq!(
        (
            could.older.runs,
            could.older.branches,
            could.older.work_folders
        ),
        (1, 1, 1),
        "the sentence above the control has to count what pressing it would take: one run, its \
         one branch and its one work folder. A control that deletes without saying what goes is \
         the one thing this panel may not have. It said: {:?}",
        could.older
    );
    assert!(
        could.older.said.contains("1 run")
            && could.older.said.contains("1 branch")
            && could.older.said.contains("1 work folder"),
        "and it has to say all three in one sentence, because a person who reads only the number \
         of runs does not know their branches go too. It said: {}",
        could.older.said
    );

    let done = forget_runs_older_than(bench.project(), OLDER_THAN);

    let listed = list_runs_inner(bench.project());
    let folders: Vec<&str> = listed.iter().map(|one| one.folder.as_str()).collect();
    assert_eq!(
        folders,
        vec![today.as_str()],
        "the folder has to keep exactly the runs that are not older than that, and lose the ones \
         that are. Forgetting said: {done:?}"
    );
    assert!(
        !long_ago.exists(),
        "the work folder of the forgotten run is still on disk at {}. Its run is gone from the \
         history, so nothing in the app will ever mention it again",
        long_ago.display()
    );
    let branches = bench.branches()?;
    assert!(
        !branches.contains(&format!("loadout/{LONG_AGO_RUN}/{KEY}")),
        "the branch of the forgotten run is still here, and nothing names it any more: the prefix \
         that identifies a run's branches is read out of the run's own record, and that record is \
         gone. Branches left: {branches:?}"
    );
    assert!(
        branches.contains(&format!("loadout/{TODAY_RUN}/{KEY}")),
        "and the branch of the run that stayed was taken away too, so the control reaches past \
         the date it was given. Branches left: {branches:?}"
    );

    Ok(())
}

// ── (5) zdanie nad kontrolką daty zgadza się z tym, co naprawdę zejdzie ────────────────────

/// Drugi bieg sprzed lat — ten z niezapisaną pracą w katalogu roboczym.
const DIRTY_LONG_AGO_FOLDER: &str = "20200102-000000__01a03200-0000-7000-8000-0000000000a2";
const DIRTY_LONG_AGO_RUN: &str = "01a03200-0000-7000-8000-0000000000a2";

/// Zdanie mówi, co zejdzie — więc ma mówić o tym, co zejdzie, a nie o tym, co się zakwalifikowało
/// po dacie.
///
/// 2026-09 (Z-46) — TO JEST TA SAMA KWALIFIKACJA, DWA RAZY, i dlatego to kryterium istnieje:
/// podgląd liczył wszystkie biegi starsze niż podana liczba dni, a kasowanie pomijało te, w których
/// katalogu leży zmiana, której nikt nie zapisał. Człowiek czytał więc „zejdą dwa", naciskał
/// i zostawał z jednym — bez żadnego powodu, żeby uznać, że tak miało być.
#[test]
fn what_would_go_by_date_counts_only_what_really_goes() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let head = bench.head()?;
    let clean = bench.leave_a_tree_without_a_marker(
        LONG_AGO_FOLDER,
        &format!("loadout/{LONG_AGO_RUN}/{KEY}"),
        &head,
    )?;
    let dirty = bench.leave_a_tree_without_a_marker(
        DIRTY_LONG_AGO_FOLDER,
        &format!("loadout/{DIRTY_LONG_AGO_RUN}/{KEY}"),
        &head,
    )?;
    fs::write(
        dirty.join("half-done.txt"),
        "what the agent had written so far",
    )?;

    let could = what_this_folder_could_forget(bench.project(), OLDER_THAN);

    assert_eq!(
        (
            could.older.runs,
            could.older.branches,
            could.older.work_folders
        ),
        (1, 1, 1),
        "the sentence above the control counts a run that pressing it will not take: its work \
         folder holds a change nobody saved, so forgetting leaves it exactly where it is. A \
         sentence that promises more than the button does is worse than no sentence — the person \
         presses, reads it again, and has no way to tell which of the two numbers is the lie. It \
         said: {:?}",
        could.older
    );

    let done = forget_runs_older_than(bench.project(), OLDER_THAN);

    assert_eq!(
        (done.runs, done.branches, done.work_folders),
        (
            could.older.runs,
            could.older.branches,
            could.older.work_folders
        ),
        "and what really went has to be what the sentence said would go. It said: {done:?}"
    );
    assert!(
        !clean.exists(),
        "the work folder of the run that had nothing unsaved is still on disk at {}",
        clean.display()
    );
    assert!(
        dirty.exists(),
        "the folder holding a change nobody saved was taken away, and that change existed nowhere \
         else. This is the one thing this control may never do"
    );
    let listed: Vec<String> = list_runs_inner(bench.project())
        .into_iter()
        .map(|one| one.folder)
        .collect();
    assert_eq!(
        listed,
        vec![DIRTY_LONG_AGO_FOLDER.to_owned()],
        "the run that stayed has to stay on the list too, or the screen says it is gone while \
         every byte of it is still on disk"
    );
    assert!(
        done.said.contains(DIRTY_LONG_AGO_FOLDER),
        "and the answer has to name the run it left behind, because otherwise the numbers simply \
         come out lower than the sentence promised and nothing says why. It said: {}",
        done.said
    );

    Ok(())
}

// ── (6) bieg, którego gałąź niesie własną, niewlaną pracę, nie schodzi po dacie ────────────

/// Bieg sprzed lat, po którym została gałąź z commitem, którego nie ma nigdzie indziej.
const CARRIES_FOLDER: &str = "20200103-000000__01a03200-0000-7000-8000-0000000000f1";

/// Data nie jest zgodą na skasowanie CZYJEJŚ PRACY.
///
/// 2026-09 (Z-46) — TA DROGA GINĘŁA MIĘDZY DWIEMA KONTROLKAMI. Zamiatacz („Forget them") pyta
/// o gałąź `isolate::carries_its_own_work` i zostawia tę, która niesie commit spoza `HEAD`.
/// Kasowanie po dacie kwalifikowało bieg wyłącznie po brudnym katalogu roboczym, po czym oddawało
/// go `history::forget_run_inner` — a ono zdejmuje KAŻDĄ jego gałąź przez `git branch -D`. Czysta
/// gałąź z niewlanym commitem znikała więc bez słowa, a po `branch -D` nie sięga do niej nic poza
/// `git fsck`. Zmierzone na fiksturze: gałąź szła do kosza razem z jedynym commitem agenta.
#[test]
fn a_run_whose_branch_carries_its_own_work_is_not_forgotten_by_date() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    // Bieg sprzed lat, po którym krok zapisał swoją pracę sam — commit stoi wyłącznie na gałęzi.
    bench.leave_a_run(CARRIES_FOLDER)?;
    let carries = format!("loadout/{CARRIES_RUN}/{KEY}");
    bench.commit_on_a_branch(&carries, "the agent saved this itself")?;
    // I drugi, równie stary, po którym została gałąź bez ani jednego własnego commita.
    bench.leave_a_run(LONG_AGO_FOLDER)?;
    let empty = format!("loadout/{LONG_AGO_RUN}/{KEY}");
    bench.git(&["branch", &empty])?;

    let could = what_this_folder_could_forget(bench.project(), OLDER_THAN);
    assert_eq!(
        (could.older.runs, could.older.branches),
        (1, 1),
        "the sentence above the control counts a run that pressing it must not take: its branch \
         carries a commit this project does not have anywhere else, so forgetting the run would \
         take the only copy of what that agent wrote. It said: {:?}",
        could.older
    );

    let done = forget_runs_older_than(bench.project(), OLDER_THAN);

    let branches = bench.branches()?;
    assert!(
        branches.contains(&carries),
        "the branch carrying a commit this project does not have was deleted by forgetting runs \
         by date. Its run is older than the date, but the date is not permission to throw away \
         somebody's work: after `branch -D` nothing reaches that commit but `git fsck`. Branches \
         left: {branches:?}. Forgetting said: {done:?}"
    );
    assert!(
        !branches.contains(&empty),
        "and the branch that carries nothing this project does not already have is still here, so \
         the control now leaves behind exactly the rows it exists to clear. Branches left: \
         {branches:?}"
    );
    let listed: Vec<String> = list_runs_inner(bench.project())
        .into_iter()
        .map(|one| one.folder)
        .collect();
    assert_eq!(
        listed,
        vec![CARRIES_FOLDER.to_owned()],
        "the run whose branch was kept has to keep its folder too: its record is the only thing \
         that still names that branch as its own"
    );
    assert!(
        done.said.contains(&carries),
        "and the answer has to NAME the branch it left alone, or the person reads a number lower \
         than the one they were promised and nothing tells them which run stayed, or why. It \
         said: {}",
        done.said
    );

    Ok(())
}

/// Nazwa katalogu biegu, który ruszył TERAZ — składana z tego samego zegara, co nazwy prawdziwe.
///
/// Z zegara, a nie z wpisanej daty: kryterium ma mierzyć „nie starszy niż podana liczba dni"
/// w każdym dniu, w którym ktoś je uruchomi, a nie tylko w tym, w którym je napisano.
fn today_folder(id: &str) -> String {
    let now = loadout_lib::commands::now_utc();
    let day: String = now.chars().take(10).filter(char::is_ascii_digit).collect();
    let time: String = now.chars().skip(11).filter(char::is_ascii_digit).collect();
    format!("{day}-{time}__{id}")
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

    fn run_dir(&self, folder: &str) -> PathBuf {
        self.project().join(".loadout").join("runs").join(folder)
    }

    /// Sam opis biegu, bez katalogu roboczego — bieg, po którym została wyłącznie gałąź.
    ///
    /// Identyfikator bierze się z nazwy katalogu za `__`, tak samo jak w prawdziwym biegu
    /// (`commands::run::stamp`), więc gałąź `loadout/<id>/…` należy do tego jednego biegu.
    fn leave_a_run(&self, folder: &str) -> Result<(), Box<dyn Error>> {
        let dir = self.run_dir(folder);
        fs::create_dir_all(&dir)?;
        let id = folder.split("__").nth(1).unwrap_or_default();
        fs::write(dir.join("run.json"), a_run(id))?;
        Ok(())
    }

    /// Bieg skończony, którego katalog roboczy dalej stoi — i **bez** notatki w `.isolation/`.
    ///
    /// To jest cała różnica wobec ławki z `leftovers_are_swept_when_the_folder_opens.rs`: tamta
    /// pisze marker z palca, żeby domykanie miało co przeczytać, a tutaj markera nie ma, bo bieg,
    /// który ten katalog otworzył, jest starszy niż sam pomysł zapisywania takiej notatki.
    fn leave_a_tree_without_a_marker(
        &self,
        folder: &str,
        branch: &str,
        head: &str,
    ) -> Result<PathBuf, Box<dyn Error>> {
        let run_dir = self.run_dir(folder);
        fs::create_dir_all(&run_dir)?;
        // Identyfikator w opisie jest tym, z którego składa się nazwa gałęzi: bez tej zgodności
        // fikstura opisywałaby bieg, do którego jej własna gałąź nie należy.
        let id = branch.split('/').nth(1).unwrap_or_default();
        fs::write(run_dir.join("run.json"), a_run(id))?;

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

    /// Gałąź z commitem, którego nie ma nigdzie indziej — tak wygląda krok, który zapisał się sam.
    ///
    /// Przez drzewo obok i `worktree remove`, bo w drzewie głównym `git checkout` przestawiłby
    /// `HEAD` całego projektu — a wtedy commit byłby w `HEAD` i pytanie „czy ta gałąź niesie coś
    /// swojego" odpowiadałoby „nie" na fiksturze, która miała mówić „tak".
    fn commit_on_a_branch(&self, branch: &str, message: &str) -> Result<(), Box<dyn Error>> {
        let at = self.project().join(".loadout").join("its-own-work");
        self.git(&[
            "worktree",
            "add",
            "--quiet",
            "-b",
            branch,
            &at.display().to_string(),
        ])?;
        fs::write(at.join("what-it-wrote.txt"), message)?;
        let there = |args: &[&str]| -> Result<String, Box<dyn Error>> {
            let out = Command::new("git")
                .arg("-C")
                .arg(&at)
                .args(["-c", "user.name=The Agent"])
                .args(["-c", "user.email=agent@loadout.invalid"])
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
        };
        there(&["add", "-A"])?;
        there(&["commit", "--quiet", "-m", message])?;
        self.git(&["worktree", "remove", "--force", &at.display().to_string()])?;
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
