//! AC-1 dla T-44: dwa zakresy piszą w dwa korzenie, a bez projektu nie powstaje NIC — nigdzie.
//!
//! # Co tu jest naprawdę sprawdzane
//!
//! Nie `place::plan`. Cały mechanizm zakresu jest napisany i przetestowany od T-18 —
//! `skills_place_destinations.rs` dowodzi, że pod korzeniem repo powstają te same dwie nazwy
//! katalogów. Sprawdzana jest **warstwa, która ma go zawołać**: do 2026-08-19 jedyny konstruktor
//! [`loadout_lib::skills::Roots`] w produkcji miał wpisane `project: None`, więc zakres projektowy
//! był osiągalny wyłącznie z testu. Dlatego droga wejścia jest tu ta sama, którą idzie okno.
//!
//! **Słabą wersją tego kryterium jest asercja, że `plan` zwrócił `Err` przy braku korzenia.**
//! Przechodzi na dzisiejszym kodzie, bez ani jednej linii zmiany, bo `plan` już tak robi — i nie
//! mówi nic o warstwie, która ma `plan` zawołać. Rozstrzyga liczenie wpisów w trzech katalogach
//! po próbie, łącznie z katalogiem roboczym procesu.
//!
//! # Dlaczego katalog roboczy procesu jest tu trzecim korzeniem
//!
//! Bo `place::destinations(Scope::Project, home, None)` nie zawiera warunku i oddaje ścieżki
//! **względne**: `.claude/skills` i `.agents/skills`. Do dysku nie dochodzą dziś tylko dlatego,
//! że `plan` odmawia wcześniej. Każda implementacja, która zawoła `destinations` albo `apply`
//! z pominięciem `plan`, zapisze umiejętność pod katalogiem roboczym procesu — czyli
//! w `npm run tauri dev` pod `src-tauri/.claude/skills`. To jest to samo „zgadywanie cwd", które
//! doc `Roots.project` nazywa wprost, tylko zrobione przez `Path::join` na pustce.
//!
//! Katalog roboczy jest stanem CAŁEGO PROCESU, a `tests/it/` jest jednym binarium — nagłówek
//! `main.rs` mówi wprost, że test mierzący stan procesu zwykle dostaje własny cel. Ten zostaje
//! tutaj, bo `check:` tego kryterium wskazuje cel `it`, a okno podmiany jest zawężone do
//! jednego wywołania i przywracane przez [`WorkingDir`] także przy panice asercji. Ani jeden
//! moduł tego celu nie rozwiązuje ścieżek względnych w czasie wykonania — wszystkie liczą je
//! z `CARGO_MANIFEST_DIR` albo przez `include_str!`.
//!
//! Test nie dotyka prawdziwego `~/.claude/skills`: „dom", „projekt" i „gdzie indziej" to trzy
//! katalogi w katalogach tymczasowych, a biblioteka leży pod domem, dokładnie jak `~/.loadout`.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use loadout_lib::commands::skills::Landing;
use loadout_lib::ipc::{install_reviewed_skill, project_folder};
use loadout_lib::skills::Error as CoreError;

/// Katalog kopii kanonicznych wewnątrz biblioteki i nazwa pliku umiejętności.
const SKILLS_DIR: &str = "skills";
const SKILL_FILE: &str = "SKILL.md";

/// Umiejętność, którą to kryterium rozmieszcza.
const NAME: &str = "pdf";

/// Druga umiejętność, zainstalowana globalnie PRZED próbą projektową.
///
/// Ona jest tu całą różnicą między „zakres projektowy zapisał w projekcie" i „zakres projektowy
/// zapisał w projekcie, a przy okazji przepisał to, co leżało w domu". Katalog globalny, który
/// po prostu NIE POWSTAŁ dla `pdf`, tego nie odróżnia.
const SENTINEL: &str = "release-notes";

/// Dom, biblioteka i korzeń projektu — w jednym katalogu tymczasowym.
struct World {
    /// Trzyma katalog tymczasowy przy życiu na czas testu; kasuje go `Drop`.
    _tmp: tempfile::TempDir,
    home: PathBuf,
    /// `~/.loadout`. Katalog domowy jest jego RODZICEM, i to jest jedyne miejsce, w którym
    /// instalacja pyta o dom (`commands::skills::roots_for`).
    library: PathBuf,
    /// Korzeń „projektu", czyli to, co okno bierze z `activeWorkspace()?.folder`.
    project: PathBuf,
}

fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let library = home.join(".loadout");
    let project = tmp.path().join("project");
    fs::create_dir_all(library.join(SKILLS_DIR)).unwrap();
    fs::create_dir_all(&project).unwrap();
    plant_canonical(&library, NAME);
    World {
        _tmp: tmp,
        home,
        library,
        project,
    }
}

fn skill_md(name: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: Pulls the tables out of files nobody wants to read.\n\
         ---\n\nRead the file first.\n"
    )
}

/// Kopia kanoniczna w danych aplikacji — tak, jak ją odkłada `review_skill_inner`.
fn plant_canonical(library: &Path, name: &str) {
    let dir = library.join(SKILLS_DIR).join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(SKILL_FILE), skill_md(name)).unwrap();
}

/// Dwa katalogi umiejętności pod danym korzeniem.
///
/// **Wypisane literalnie**, nie wzięte z `DESTINATION_DIRS`: kryterium sprawdzające implementację
/// jej własną tablicą przechodzi po każdej zmianie tej tablicy, łącznie z literówką. Ten sam powód
/// stoi w `skills_ingest_no_exec.rs`.
fn skill_dirs(root: &Path, name: &str) -> [PathBuf; 2] {
    [
        root.join(".claude").join("skills").join(name),
        root.join(".agents").join("skills").join(name),
    ]
}

/// Czy pod tym katalogiem leży plik umiejętności.
fn wrote_skill(dir: &Path) -> bool {
    fs::symlink_metadata(dir.join(SKILL_FILE)).is_ok_and(|meta| meta.is_file())
}

/// Każdy wpis w drzewie — plik I katalog — ścieżką WZGLĘDNĄ wobec korzenia.
///
/// Także katalogi, nie tylko pliki: `create_dir_all` bez zapisu jest zapisem w cudze drzewo
/// i tak samo zostawia po sobie ślad w repo zespołu.
fn entries_under(root: &Path) -> BTreeSet<PathBuf> {
    let mut out = BTreeSet::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.is_dir() {
                stack.push(path.clone());
            }
            if let Ok(relative) = path.strip_prefix(root) {
                out.insert(relative.to_path_buf());
            }
        }
    }
    out
}

/// Bajty i czas modyfikacji obu globalnych kopii [`SENTINEL`].
///
/// Bajty NIE WYSTARCZAJĄ: `emit` jest deterministyczny, więc instalacja, która przepisała ten
/// plik jeszcze raz, zostawia dokładnie tę samą treść. Czas modyfikacji jest jedyną rzeczą, która
/// odróżnia „nie dotknięto" od „napisano to samo".
fn sentinel_state(home: &Path) -> Vec<(Vec<u8>, SystemTime)> {
    skill_dirs(home, SENTINEL)
        .iter()
        .map(|dir| {
            let file = dir.join(SKILL_FILE);
            let bytes = fs::read(&file).unwrap_or_default();
            let when = fs::metadata(&file)
                .and_then(|meta| meta.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            (bytes, when)
        })
        .collect()
}

/// Ścieżka tak, jak przysyła ją okno: napisem.
fn as_the_window_sends_it(path: &Path) -> &str {
    path.to_str()
        .expect("a temporary directory path is valid UTF-8 on this platform")
}

/// Katalog roboczy procesu, przywracany na KAŻDEJ drodze wyjścia — także przez panikę asercji.
///
/// Struktura z `Drop`, a nie para wywołań: test, który zostawiłby katalog roboczy w katalogu
/// tymczasowym, zostawia go na ścieżce, którą `Drop` `TempDir`-a zaraz skasuje — a wtedy każdy
/// następny test w tym samym procesie pracuje w katalogu, którego nie ma.
struct WorkingDir(PathBuf);

impl WorkingDir {
    fn moved_to(dir: &Path) -> Self {
        let was = std::env::current_dir().expect("the test process has a working directory");
        std::env::set_current_dir(dir).expect("a fresh temporary directory can be entered");
        Self(was)
    }
}

impl Drop for WorkingDir {
    fn drop(&mut self) {
        // Bez `expect`: przywrócenie katalogu roboczego nie jest twierdzeniem tego kryterium,
        // a panika w `Drop` w trakcie panikującej asercji przerywa proces bez jej komunikatu.
        let _ = std::env::set_current_dir(&self.0);
    }
}

// ── (a) „wszędzie" pisze pod domem i nigdzie więcej ─────────────────────────────────────────

#[test]
fn everywhere_writes_both_vendor_names_under_home_and_not_one_entry_under_the_project() {
    let world = world();

    let wrote = install_reviewed_skill(&world.library, NAME, Landing::Everywhere, None)
        .expect("a reviewed skill with a valid name installs in the scope that needs no project");

    for dir in skill_dirs(&world.home, NAME) {
        assert!(
            wrote_skill(&dir),
            "nothing was written at {}. The global scope is the one this window could already \
             reach, so losing it while adding the other one trades one broken half for another",
            dir.display()
        );
    }
    assert_eq!(
        wrote.len(),
        2,
        "the install reported {:?} as the folders it wrote. Two names cover all six agent apps \
         [T5 §3.1], and a person reads that list before pressing the button",
        wrote
    );
    assert_eq!(
        entries_under(&world.project),
        BTreeSet::new(),
        "the global scope also left something under the project root at {}. A skill a person \
         asked for on this machine has no business landing in the repository their team clones",
        world.project.display()
    );
}

// ── (b) „ten projekt" pisze pod projektem i nie dotyka domu ──────────────────────────────────

#[test]
fn this_project_writes_the_same_two_names_under_the_project_and_leaves_the_global_copy_alone() {
    let world = world();
    plant_canonical(&world.library, SENTINEL);
    install_reviewed_skill(&world.library, SENTINEL, Landing::Everywhere, None)
        .expect("the sentinel is installed globally before the project scope is asked for");
    let before = sentinel_state(&world.home);

    install_reviewed_skill(
        &world.library,
        NAME,
        Landing::ThisProject,
        Some(as_the_window_sends_it(&world.project)),
    )
    .expect(
        "the project scope with a real root has to install. Scope::Project, place::plan and \
         place::remove have taken it since T-18; this is the layer that was never allowed to ask",
    );

    for dir in skill_dirs(&world.project, NAME) {
        assert!(
            wrote_skill(&dir),
            "the choice was 'this project' and nothing was written at {}. A choice that does not \
             change where the file lands is worse than no choice at all (invariant 16), and this \
             one decides whether the skill travels with the repository or stays on one machine",
            dir.display()
        );
    }

    for dir in skill_dirs(&world.home, NAME) {
        assert!(
            fs::symlink_metadata(&dir).is_err(),
            "the project scope also wrote {}. A skill written into the home folder is in every \
             later run of every project on this machine, and nobody asked for that",
            dir.display()
        );
    }

    assert_eq!(
        sentinel_state(&world.home),
        before,
        "installing '{NAME}' into the project changed the global copy of '{SENTINEL}': bytes or \
         modification time. Bytes alone would not tell 'untouched' from 'written again with the \
         same content', because emit() is deterministic — and rewriting a file the person did not \
         mention is how a project-scoped install quietly becomes a global one"
    );
}

// ── (c) bez korzenia projektu nie powstaje ani jeden wpis ────────────────────────────────────

#[test]
fn without_a_project_root_nothing_is_created_anywhere_not_even_in_the_working_directory() {
    let world = world();
    plant_canonical(&world.library, SENTINEL);
    install_reviewed_skill(&world.library, SENTINEL, Landing::Everywhere, None)
        .expect("the sentinel is installed globally, so the tree below is not empty");

    let elsewhere = tempfile::tempdir().unwrap();
    let home_before = entries_under(&world.home);
    let project_before = entries_under(&world.project);
    assert!(
        !home_before.is_empty(),
        "control against comparing two empty sets: the fixture planted nothing under home, so \
         'nothing new appeared' below would hold for an install that never runs at all"
    );
    assert_eq!(
        entries_under(elsewhere.path()),
        BTreeSet::new(),
        "control: the third temporary directory has to start empty, or the count after the \
         attempt says nothing about what the attempt wrote"
    );

    // Katalog roboczy podmieniony na czas JEDNEGO wywołania i przywrócony przez `Drop`, zanim
    // padnie którakolwiek asercja niżej.
    let said = {
        let _cwd = WorkingDir::moved_to(elsewhere.path());
        install_reviewed_skill(&world.library, NAME, Landing::ThisProject, None).expect_err(
            "asking for the project scope with no open project has to be a refusal. The one \
             thing it must never be is a guess: destinations(Scope::Project, home, None) returns \
             RELATIVE paths, so a guess writes the skill under whatever folder the process \
             happens to be in",
        )
    };

    assert_eq!(
        said,
        CoreError::NoProjectRoot.to_string(),
        "the refusal is not the sentence the core already says for this. A second wording of one \
         cause is a second bug report from the same person, and this one is read by somebody who \
         has to work out that they need to open a workspace first"
    );
    assert_eq!(
        entries_under(&world.home),
        home_before,
        "the refused install still created something under home at {}",
        world.home.display()
    );
    assert_eq!(
        entries_under(&world.project),
        project_before,
        "the refused install still created something under the project root at {}",
        world.project.display()
    );
    assert_eq!(
        entries_under(elsewhere.path()),
        BTreeSet::new(),
        "the refused install wrote into the working directory of the process ({}). This is the \
         assertion that matters here: without a project root the two destination paths are \
         RELATIVE, so an implementation that reaches destinations() or apply() without going \
         through plan() lands the skill beside whatever the process was started in — in \
         `npm run tauri dev` that is src-tauri/",
        elsewhere.path().display()
    );
}

// ── (d) zły folder z okna odmawia tym samym zdaniem, co bieg ─────────────────────────────────

#[test]
fn a_folder_that_is_not_an_existing_absolute_directory_is_refused_the_way_a_run_refuses_it() {
    let world = world();
    let a_file = world.home.join("not-a-folder.txt");
    fs::write(&a_file, "a file, not a folder\n").unwrap();
    let gone = world.project.join("moved-away");

    // Trzy kształty, jedna wyrocznia. `project_folder` jest jedynym miejscem, w którym te zdania
    // mieszkają, i `AppState::project_for` — droga Startu — jest nad nim jedną linią. Zdanie
    // przepisane w tym pliku z palca przechodziłoby także wtedy, gdyby bieg mówił co innego.
    for bad in [
        "projects/loadout".to_owned(),
        as_the_window_sends_it(&a_file).to_owned(),
        as_the_window_sends_it(&gone).to_owned(),
    ] {
        let run_says = project_folder(Some(&bad)).expect_err(
            "a run refuses this folder, so there is a sentence to compare against. If this is \
             Ok, the oracle is gone and the comparison below is two identical nothings",
        );
        let install_says = install_reviewed_skill(
            &world.library,
            NAME,
            Landing::ThisProject,
            Some(&bad),
        )
        .expect_err(
            "the window sent a folder that is not an existing absolute directory and the install \
             took it. Nothing below this layer asks that question — place::plan believes the \
             root it is handed — so the skill lands wherever Path::join puts it",
        );

        assert_eq!(
            install_says, run_says,
            "installing a skill and starting a run answer 'which project is this' with two \
             different sentences for the folder {bad:?}. There is one workspace, one folder and \
             one function that judges it; two answers drift apart on the first day somebody \
             switches tabs, and the person reads whichever of the two happens to reach them"
        );
    }
}
