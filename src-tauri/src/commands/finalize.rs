//! Składanie pracy biegu w JEDNĄ gałąź, którą człowiek może obejrzeć.
//!
//! # Po co to istnieje
//!
//! Bieg kończy się sukcesem, praca istnieje — i leży na kilku gałęziach `loadout/<bieg>/<krok>`,
//! o których nikt nie mówi. Żeby ją zobaczyć, trzeba wiedzieć, że w ogóle są, i znać schemat
//! nazwy. To jest ostatni krok ścieżki, którego produkt nie miał: „bieg skończony, oto co
//! powstało".
//!
//! # Dlaczego do WŁASNEJ gałęzi, a nie do gałęzi człowieka
//!
//! `FOUNDATIONS §2.1` rozdziela trzy rzeczy, których nie wolno mylić: co agent powiedział, co
//! znalazły sprawdzenia i **co człowiek zatwierdził**. Scalenie prosto na gałąź, na której ktoś
//! siedzi, zjada tę trzecią — maszyna robi ruch należący do człowieka, w jedynym miejscu, gdzie
//! pomyłka dotyka jego własnego drzewa roboczego. Tu powstaje jedna gałąź wyniku; wzięcie jej to
//! jeden `git merge`, i to jest decyzja człowieka.
//!
//! # Dlaczego wszystko albo nic
//!
//! Scalanie jest sekwencyjne, więc konfliktu nie da się poznać inaczej niż próbą. Ale gałąź
//! złożona w POŁOWIE opisuje stan, którego nie opisuje żaden krok — a to jest dokładnie ta wada,
//! przed którą stoi [`super::fan_in::fold_the_copies`]. Kiedy któreś scalenie nie wychodzi, nie
//! zostaje po nas nic poza zdaniem o tym, co się zderzyło.

use std::path::Path;
use std::process::Command;

use serde::Serialize;

/// Czym skończyło się składanie.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Landing {
    /// Gałąź istnieje i niesie pracę tylu kroków.
    Landed {
        /// Nazwa, pod którą praca stoi.
        branch: String,
        /// Ile gałęzi kroków się na nią złożyło.
        steps: usize,
    },
    /// Żaden krok niczego nie zmienił, więc nie ma czego składać. To jest **uczciwy wynik**,
    /// a nie awaria: bieg, w którym nikt nic nie napisał, nie ma prawa udawać, że coś zostawił.
    Nothing,
    /// Dwie gałęzie napisały w tym samym miejscu i człowiek musi rozstrzygnąć, która ma rację.
    Clash {
        /// Gałąź, która się nie złożyła.
        with: String,
        /// Pliki, na których stanęło. To jest jedyna rzecz, którą da się tu powiedzieć konkretnie.
        files: Vec<String>,
    },
}

/// Wołanie gita z tożsamością podaną na miejscu — ten sam kształt, co w [`super::isolate`].
///
/// Tożsamość jest tu, bo commit scalenia nie ma prawa zależeć od tego, czy ktoś ustawił
/// `user.email`, a `commit.gpgsign` gasimy, bo podpisywanie czeka na hasło i zawiesiłoby bieg
/// bez jednego słowa na ekranie.
fn git(at: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(at)
        .args(["-c", "user.name=Loadout"])
        .args(["-c", "user.email=loadout@localhost"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_owned());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Czy taka gałąź w ogóle jest.
fn there(project: &Path, branch: &str) -> bool {
    git(project, &["rev-parse", "--verify", "--quiet", branch]).is_ok()
}

/// Pliki, na których stanęło scalanie.
fn stuck_on(at: &Path) -> Vec<String> {
    git(at, &["diff", "--name-only", "--diff-filter=U"])
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|one| !one.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Składa gałęzie kroków w jedną gałąź wyniku.
///
/// `base` to commit, od którego bieg wystartował — gałąź wyniku zaczyna się tam, żeby niosła
/// wyłącznie to, co zrobił bieg, a nie wszystko, co człowiek ma na swojej gałęzi.
///
/// Kroki, które nic nie zmieniły, nie mają gałęzi i są po cichu pomijane: brak gałęzi znaczy
/// „ten krok nie napisał ani bajtu", a nie „coś się zepsuło".
pub fn fold_into_one(
    project: &Path,
    result: &str,
    base: &str,
    steps: &[String],
) -> Result<Landing, String> {
    if there(project, result) {
        return Err(format!(
            "there is already a branch called {result}, so the work of this run has nowhere to land"
        ));
    }
    let real: Vec<&String> = steps.iter().filter(|one| there(project, one)).collect();
    if real.is_empty() {
        return Ok(Landing::Nothing);
    }

    // Drzewo tymczasowe, nigdy drzewo człowieka: scalanie pisze po plikach, a te pliki mają być
    // nasze. `--detach`, bo gałąź powstaje DOPIERO wtedy, gdy wszystko się złożyło.
    let work = tempfile::tempdir().map_err(|error| error.to_string())?;
    let at = work.path();
    let at_text = at.to_string_lossy().into_owned();
    git(
        project,
        &["worktree", "add", "--detach", "--quiet", &at_text, base],
    )?;

    let mut landed = Ok(Landing::Landed {
        branch: result.to_owned(),
        steps: real.len(),
    });
    for one in &real {
        if git(at, &["merge", "--no-ff", "--no-edit", one]).is_err() {
            let files = stuck_on(at);
            let _ = git(at, &["merge", "--abort"]);
            landed = Ok(Landing::Clash {
                with: (*one).clone(),
                files,
            });
            break;
        }
    }

    if matches!(landed, Ok(Landing::Landed { .. })) {
        // Gałąź nazwana dopiero teraz, kiedy wiadomo, że niesie komplet.
        if let Err(said) = git(at, &["branch", result, "HEAD"]) {
            landed = Err(said);
        }
    }

    // Drzewo znika ZAWSZE, także po konflikcie: katalog, którego nikt nie szuka, jest tą samą
    // wadą co katalogi biegów zostawiane na dysku przed T-95.
    let _ = git(project, &["worktree", "remove", "--force", &at_text]);
    landed
}

/// Gałęzie, które ten bieg po sobie zostawił.
///
/// Przedrostek składa [`super::isolate::branch_for`] — TA SAMA funkcja, która te gałęzie nazywa.
/// Napis sklejony tutaj z palca byłby drugą regułą na pytanie „które gałęzie są tego biegu"
/// i rozjechałby się w dniu, w którym zmieni się nazywanie (niezmiennik 13). Ta sama droga stoi
/// już w `history::forget_run_branches_inner`, która na tej odpowiedzi KASUJE.
#[must_use]
pub fn branches_of_run(project: &Path, run: &str) -> Vec<String> {
    if run.trim().is_empty() {
        // Pusty człon środkowy dałby `loadout//`, czyli wzorzec pasujący do gałęzi KAŻDEGO biegu.
        return Vec::new();
    }
    super::isolate::branches_under(project, &super::isolate::branch_for(run, ""))
}

/// Commit, od którego ten bieg wystartował.
///
/// Liczony, a nie pamiętany: wspólny przodek wszystkich gałęzi kroków JEST tym commitem, bo
/// każda z nich powstała z niego. Trzymanie go w pliku biegu byłoby drugą kopią odpowiedzi,
/// która potrafi się rozjechać z gałęziami (niezmiennik 13).
fn where_it_started(project: &Path, steps: &[String]) -> Option<String> {
    let mut args = vec!["merge-base", "--octopus"];
    args.extend(steps.iter().map(String::as_str));
    git(project, &args).ok().map(|said| said.trim().to_owned())
}

/// Składa pracę całego biegu pod podaną nazwą.
///
/// Wołane, kiedy CZŁOWIEK o to poprosi — nie na końcu biegu. Gałąź wyniku jest propozycją,
/// a nie ruchem, który maszyna robi za niego (`FOUNDATIONS §2.1`).
pub fn fold_run(project: &Path, run: &str, name: &str) -> Result<Landing, String> {
    let steps = branches_of_run(project, run);
    if steps.is_empty() {
        return Ok(Landing::Nothing);
    }
    let base = where_it_started(project, &steps)
        .ok_or_else(|| "the branches of this run have no commit in common".to_owned())?;
    fold_into_one(project, name, &base, &steps)
}
