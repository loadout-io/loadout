//! AC-2 dla T-44: co zapisane w projekcie, to widoczne i zabieralne — a globalne zostaje.
//!
//! # Dlaczego zakres wchodzi w obie strony naraz
//!
//! Bo droga zapisu bez drogi odczytu jest gorsza niż brak funkcji. `list_skills_inner` czytał do
//! 2026-08-19 wyłącznie katalogi globalne, więc umiejętność zapisana „w tym projekcie" nie
//! pojawiłaby się na liście — a wtedy człowiek jej nie widzi i **nie ma jak jej zabrać**, choć
//! leży w żywej konfiguracji jego narzędzi agentowych. To jest dokładnie ten kształt defektu,
//! który to repo naprawiało trzy razy w tym tygodniu (T-26, T-27, T-38): mechanizm wylądował,
//! nikt go nie zawołał.
//!
//! # Słaba asercja i to, co ją odróżnia
//!
//! **Słabą asercją jest sprawdzenie, że po usunięciu katalog projektowy nie istnieje.** Przechodzi
//! implementacja, która skasowała oba zakresy naraz, i taka, która skasowała cudzy katalog o tej
//! samej nazwie. Rozróżniają dwie rzeczy w fikstrze: kopia globalna o TEJ SAMEJ nazwie, sądzona
//! bajtami i czasem modyfikacji, oraz katalog obok niej, którego Loadout nigdy nie napisał.
//!
//! Czas modyfikacji, nie tylko bajty: `emit` jest deterministyczny, więc kopia przepisana jeszcze
//! raz ma dokładnie tę samą treść. „Nie dotknięto" i „napisano to samo" różni tylko znacznik.
//!
//! # Czyj jest katalog
//!
//! Odpowiedź mieszka w sidecarze (`skills/installed.json`) i w `place::remove` — tutaj nie
//! powstaje drugi raz (niezmiennik 23). Kolizja nazw jest normalna, nie wyjątkowa: `pdf` to
//! oczywista nazwa i ktoś mógł napisać swoją ręcznie, wprost w repo zespołu.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use loadout_lib::commands::skills::{
    Landing, delete_skill_from, install_skill_into, list_skills_in,
};

const SKILLS_DIR: &str = "skills";
const SKILL_FILE: &str = "SKILL.md";

/// Umiejętność zainstalowana DWA razy: raz globalnie, raz w projekcie. Jedna nazwa, dwie rzeczy.
const NAME: &str = "pdf";

/// Katalog w korzeniu projektu, którego Loadout nigdy nie napisał — bez wpisu w sidecarze.
///
/// Nazwa jest po `pdf` alfabetycznie, bo lista wraca posortowana i kolejność jest częścią
/// odpowiedzi: lista, która przetasowuje się między dwoma wejściami w sekcję, każe człowiekowi
/// szukać wiersza tam, gdzie go ostatnio widział.
const SOMEBODY_ELSES: &str = "team-notes";

struct World {
    _tmp: tempfile::TempDir,
    home: PathBuf,
    library: PathBuf,
    project: PathBuf,
}

/// Fikstura z całego zdania AC-2: ta sama nazwa w dwóch zakresach plus cudzy katalog obok.
fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let library = home.join(".loadout");
    let project = tmp.path().join("project");
    fs::create_dir_all(library.join(SKILLS_DIR)).unwrap();
    fs::create_dir_all(&project).unwrap();

    plant_canonical(&library, NAME);
    install_skill_into(&library, NAME, Landing::Everywhere, None)
        .expect("a reviewed skill installs in the global scope");
    install_skill_into(&library, NAME, Landing::ThisProject, Some(&project)).expect(
        "the project scope with a real root has to install — Scope::Project and place::plan have \
         taken it since T-18, and this is the layer that was never allowed to ask",
    );

    // Cudzy katalog: napisany wprost w repo, bez kopii kanonicznej i bez wpisu w sidecarze.
    let theirs = project.join(".claude").join("skills").join(SOMEBODY_ELSES);
    fs::create_dir_all(&theirs).unwrap();
    fs::write(theirs.join(SKILL_FILE), skill_md(SOMEBODY_ELSES)).unwrap();

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

fn plant_canonical(library: &Path, name: &str) {
    let dir = library.join(SKILLS_DIR).join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(SKILL_FILE), skill_md(name)).unwrap();
}

/// Dwa katalogi umiejętności pod danym korzeniem, **wypisane literalnie** — nie z
/// `DESTINATION_DIRS`. Kryterium sprawdzające implementację jej własną tablicą przechodzi po
/// każdej zmianie tej tablicy, łącznie z literówką (`skills_ingest_no_exec.rs` ma ten sam powód).
fn skill_dirs(root: &Path, name: &str) -> [PathBuf; 2] {
    [
        root.join(".claude").join("skills").join(name),
        root.join(".agents").join("skills").join(name),
    ]
}

/// Nazwy z listy, w kolejności, w jakiej ją oddano — z powtórzeniami, jeżeli są.
fn listed(library: &Path, project: Option<&Path>) -> Vec<String> {
    list_skills_in(library, project)
        .expect("reading the agent directories is a state, not a failure")
        .into_iter()
        .map(|one| one.name)
        .collect()
}

/// Bajty i czas modyfikacji obu globalnych kopii tej umiejętności.
fn global_state(home: &Path, name: &str) -> Vec<(Vec<u8>, SystemTime)> {
    skill_dirs(home, name)
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

// ── (a) i (b) lista odpowiada na „co widzi agent pracujący tutaj" ────────────────────────────

#[test]
fn the_list_carries_both_roots_when_a_project_is_open_and_each_skill_exactly_once() {
    let world = world();

    assert_eq!(
        listed(&world.library, Some(&world.project)),
        vec![NAME.to_owned(), SOMEBODY_ELSES.to_owned()],
        "with a project open the list has to carry what lies in BOTH roots, each skill once. \
         Once, not twice, because an install writes into two vendor folders and the set of names \
         is one — a repeated row shows a person two lines about one file and counts it twice in \
         the number above the section. A skill saved 'in this project' and missing from this list \
         is a skill nobody can take back out, and it sits in the live folders their agent apps read"
    );
}

#[test]
fn the_same_list_without_an_open_project_carries_only_what_is_global() {
    let world = world();

    assert_eq!(
        listed(&world.library, None),
        vec![NAME.to_owned()],
        "with no project open the list still shows something that lies only under the project \
         root. This list answers 'what does the agent working HERE see', not 'what did we ever \
         write' — and '{SOMEBODY_ELSES}' is reachable only from inside that repository"
    );
}

// ── (c) zdjęcie z projektu zostawia globalne ─────────────────────────────────────────────────

#[test]
fn removing_from_the_project_takes_both_project_copies_and_leaves_the_global_one_alone() {
    let world = world();
    let before = global_state(&world.home, NAME);
    assert!(
        before.iter().all(|(bytes, _)| !bytes.is_empty()),
        "control against comparing two nothings: the fixture left no readable global copy of \
         '{NAME}', so 'the global copy is untouched' below would hold for an implementation that \
         deleted it"
    );

    delete_skill_from(
        &world.library,
        NAME,
        Landing::ThisProject,
        Some(&world.project),
    )
    .expect(
        "removing a skill from the project scope has to succeed — Loadout wrote those two folders \
         and the sidecar says so",
    );

    for dir in skill_dirs(&world.project, NAME) {
        assert!(
            fs::symlink_metadata(&dir).is_err(),
            "the project copy at {} is still there. An install writes into TWO vendor folders, so \
             a remove that cleaned one of them looks exactly like success: the row leaves the \
             screen and the file stays where the agent reaches for it",
            dir.display()
        );
    }

    assert_eq!(
        global_state(&world.home, NAME),
        before,
        "removing '{NAME}' from the project also changed the global copy: bytes or modification \
         time. The same name in two scopes is TWO things — the person asked for the one that \
         travels with this repository, and the one on their machine is used by every other project"
    );

    assert_eq!(
        listed(&world.library, Some(&world.project)),
        vec![NAME.to_owned(), SOMEBODY_ELSES.to_owned()],
        "after the project copy is gone the list read from disk no longer carries '{NAME}' at \
         all, and the global copy is still on disk. The list is read back from the folders after \
         every remove precisely so that a row disappears only when the file really did"
    );
}

// ── (d) cudzy katalog zostaje, a zdanie odmowy nazywa ścieżkę ────────────────────────────────

#[test]
fn a_folder_loadout_never_wrote_is_left_where_it_is_and_the_refusal_names_it() {
    let world = world();
    let theirs = world
        .project
        .join(".claude")
        .join("skills")
        .join(SOMEBODY_ELSES);
    let bytes = fs::read(theirs.join(SKILL_FILE)).unwrap();

    let said = delete_skill_from(
        &world.library,
        SOMEBODY_ELSES,
        Landing::ThisProject,
        Some(&world.project),
    )
    .expect_err(
        "a folder Loadout never wrote was removed. The sidecar is the only thing that says which \
         of two folders with one name is ours, and when it says 'not ours' nothing is deleted — \
         not even the second copy",
    )
    .to_string();

    assert!(
        said.contains(&theirs.display().to_string()),
        "the refusal does not name the folder it is about: {said:?}. A person reading it has to \
         go and look at that folder to decide what to do, and 'pdf' is an obvious name somebody \
         may well have written by hand in their own repository"
    );
    assert_eq!(
        fs::read(theirs.join(SKILL_FILE)).unwrap_or_default(),
        bytes,
        "the folder at {} was written by somebody else and Loadout changed it anyway. This is \
         work that cannot be recovered — it was never in the canonical copies",
        theirs.display()
    );
}
