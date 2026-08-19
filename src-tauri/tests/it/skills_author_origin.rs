//! AC-2 dla T-42: pochodzenie jest prawdą dla wszystkich czterech źródeł i przeżywa restart.
//!
//! # Dlaczego cztery źródła w jednym teście
//!
//! Słabą wersją tego kryterium jest asercja, że umiejętność napisana tutaj ma
//! `from_the_internet == false`. Przechodzi ją implementacja, która odwróciła stałą na nowej
//! drodze i zostawiła listę taką, jaka jest — a lista wyprowadza znacznik z **istnienia kopii
//! kanonicznej** (`list_skills_in`, dziś jedna linia: `library/skills/<name>/SKILL.md`
//! `.is_file()`). Wtedy karta mówi prawdę, lista dalej kłamie, i kłamie dokładnie o tej
//! umiejętności, którą człowiek właśnie napisał sam.
//!
//! Przesłanka tamtej linii była prawdziwa **do tego zadania**: kopie kanoniczne powstawały
//! wyłącznie w `review_skill_inner`, czyli na jedynej drodze, którą coś tu wchodziło z sieci.
//! Nowa droga wejścia też odkłada kopię kanoniczną, więc przesłanka przestaje obowiązywać
//! i znacznik musi mieć **własny zapis** (niezmiennik 4: pliki są prawdą, a pole, którego nie da
//! się już wywnioskować, trzeba zapisać jawnie).
//!
//! Rozstrzyga: cztery źródła naraz, wszystkie czytane przez `list_skills_in` **z dysku**.
//!
//! # Dlaczego nieobecność zapisu znaczy „z internetu"
//!
//! Bo dowód nieobecności to nie to samo, co nieobecność dowodu — ta sama reguła, którą trzymają
//! `DeepScan::Unavailable` i `Discovery::Unknown`. Kopia kanoniczna bez zapisu pochodzenia to
//! umiejętność z czasów **przed** tym zadaniem, a wtedy kopie kanoniczne powstawały tylko na
//! drodze linku. Ostrożny kierunek jest więc jedyny uczciwy: znacznik zastępuje podpisy
//! i weryfikację pochodzenia, których v1 nie ma, więc ma świecić tam, gdzie treść MOŻE być od
//! obcego. Odwrotny domyślny („nie wiem, więc pewnie własna") gasi go dokładnie tam, gdzie jest
//! potrzebny.
//!
//! # Dwa miejsca, w których ten zapis stać nie może
//!
//! Oba są zmierzone, nie teoretyczne, i oba mają tu własny test:
//!
//! * **`skills/installed.json`.** `place::write_sidecar` odtwarza CAŁY plik z samego zbioru
//!   ścieżek (`place.rs:673-689`), więc cokolwiek dopisanego obok przepada przy następnej
//!   instalacji albo usunięciu — po cichu. Dlatego nie wystarcza asercja „wpisy są te same":
//!   test wywołuje jeszcze jedną instalację i pyta, czy pochodzenie to przeżyło.
//! * **wnętrze katalogu umiejętności.** `ingest::bundled_files` zabiera KAŻDEGO sąsiada
//!   `SKILL.md`, więc znacznik położony obok niego pojechałby do katalogów vendorów jako plik
//!   dołączony umiejętności — do żywej konfiguracji narzędzi człowieka.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use loadout_lib::commands::skills::{
    Authored, Landing, author_skill_inner, install_skill_into, list_skills_in, remember_origin,
};

const SKILLS_DIR: &str = "skills";
const SKILL_FILE: &str = "SKILL.md";
const SIDECAR_FILE: &str = "installed.json";

/// Umiejętność wciągnięta z linku: kopia kanoniczna plus zapisane pochodzenie.
const FROM_A_LINK: &str = "pdf";

/// Umiejętność napisana tutaj — nazwa jest slugiem tego, co człowiek wpisał w pierwsze pole.
const WRITTEN_HERE_TYPED: &str = "Review pull requests";
const WRITTEN_HERE: &str = "review-pull-requests";

/// Katalog, który ktoś inny włożył wprost do `~/.claude/skills/`. Bez kopii kanonicznej,
/// bez zapisu — i widoczna na liście, bo agent ją widzi.
const BY_HAND: &str = "notatki";

/// Kopia kanoniczna bez zapisu pochodzenia: umiejętność z czasów przed tym zadaniem.
const FROM_BEFORE: &str = "release-notes";

/// Dwa katalogi docelowe. **Wypisane literalnie**, nie wzięte z `DESTINATION_DIRS`: kryterium ma
/// sądzić ścieżki, a nie zgadzać się samo ze sobą.
fn vendor_roots(home: &Path) -> [PathBuf; 2] {
    [
        home.join(".claude").join("skills"),
        home.join(".agents").join("skills"),
    ]
}

struct World {
    _tmp: tempfile::TempDir,
    home: PathBuf,
    /// `~/.loadout`. Katalog domowy jest jego RODZICEM (`commands::skills::global_roots`).
    library: PathBuf,
}

fn empty_world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().to_path_buf();
    let library = home.join(".loadout");
    fs::create_dir_all(library.join(SKILLS_DIR)).unwrap();
    World {
        _tmp: tmp,
        home,
        library,
    }
}

fn skill_md(name: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: Pulls the tables out of files nobody wants to read.\n\
         ---\n\nRead the file first.\n"
    )
}

/// Kopia kanoniczna z jednym plikiem dołączonym — tak, jak ją odkłada `review_skill_inner`.
///
/// Plik dołączony jest tu po to, żeby (d) miało co porównać: katalog z samym `SKILL.md` nie
/// odróżnia „skopiowano dokładnie to, co w kopii" od „skopiowano jeden plik".
fn plant_canonical(library: &Path, name: &str) -> PathBuf {
    let dir = library.join(SKILLS_DIR).join(name);
    fs::create_dir_all(dir.join("scripts")).unwrap();
    fs::write(dir.join(SKILL_FILE), skill_md(name)).unwrap();
    fs::write(dir.join("scripts").join("run.sh"), "#!/bin/sh\nexit 0\n").unwrap();
    dir
}

/// Umiejętność, którą ktoś inny napisał wprost w katalogu vendora.
fn plant_by_hand(home: &Path, name: &str) {
    let dir = vendor_roots(home)[0].join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(SKILL_FILE), skill_md(name)).unwrap();
}

/// Trzy odpowiedzi z formularza.
fn answers(name: &str) -> Authored {
    Authored {
        name: name.to_owned(),
        when_to_use: "Use this when somebody asks for a second look at a pull request.".to_owned(),
        what_to_do: "Read the change first, then say in one paragraph what to fix.\n".to_owned(),
    }
}

/// `from_the_internet` dla tej nazwy, przeczytane przez `list_skills_in` **z dysku**.
///
/// `None` znaczy „nie ma jej na liście wcale", i to jest inna odpowiedź niż `Some(false)`:
/// pierwsza mówi, że agent jej nie widzi, druga — że widzi i że nikt jej nie pobierał.
fn marked(library: &Path, name: &str) -> Option<bool> {
    list_skills_in(library, None)
        .expect("reading the agent directories is a state, not a failure")
        .into_iter()
        .find(|one| one.name == name)
        .map(|one| one.from_the_internet)
}

/// Ścieżki, o których sidecar instalacji mówi „to napisał Loadout".
fn recorded_paths(sidecar: &Path) -> BTreeSet<String> {
    let text = fs::read_to_string(sidecar).unwrap_or_default();
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|value| value.get("installed").cloned())
        .and_then(|listed| serde_json::from_value::<Vec<String>>(listed).ok())
        .unwrap_or_default()
        .into_iter()
        .collect()
}

/// Pliki w drzewie, ścieżkami WZGLĘDNYMI wobec jego korzenia.
fn relative_files(root: &Path) -> BTreeSet<PathBuf> {
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
                stack.push(path);
            } else if let Ok(relative) = path.strip_prefix(root) {
                out.insert(relative.to_path_buf());
            }
        }
    }
    out
}

// ── (a) cztery źródła, jedna lista ─────────────────────────────────────────────────────────

#[test]
fn the_list_tells_the_truth_about_every_way_a_skill_gets_onto_this_machine() {
    let world = empty_world();

    // 1. Z linku: kopia kanoniczna plus zapisane pochodzenie. Prawdziwego pobrania tu nie ma
    //    i być nie może — bramka wymagająca internetu czerwieni się od cudzych awarii.
    plant_canonical(&world.library, FROM_A_LINK);
    remember_origin(&world.library, FROM_A_LINK, true)
        .expect("the link path has to write down that these bytes came from a stranger");
    install_skill_into(&world.library, FROM_A_LINK, Landing::Everywhere, None)
        .expect("a reviewed skill installs");

    // 2. Napisana tutaj: prawdziwą drogą wejścia, bo pytanie brzmi, czy TA droga to zapisuje.
    let written = author_skill_inner(&world.library, answers(WRITTEN_HERE_TYPED))
        .expect("three fields a person typed have to reach the same pipeline a link reaches");
    install_skill_into(&world.library, &written.name, Landing::Everywhere, None)
        .expect("a skill written here installs");

    // 3. Cudzy katalog: bez kopii kanonicznej i bez zapisu, ale agent go widzi.
    plant_by_hand(&world.home, BY_HAND);

    // 4. Kopia kanoniczna bez zapisu pochodzenia: umiejętność z czasów przed tym zadaniem.
    plant_canonical(&world.library, FROM_BEFORE);
    install_skill_into(&world.library, FROM_BEFORE, Landing::Everywhere, None)
        .expect("an older skill installs the same way");

    let listed: Vec<(String, bool)> = list_skills_in(&world.library, None)
        .expect("reading the agent directories is a state, not a failure")
        .into_iter()
        .map(|one| (one.name, one.from_the_internet))
        .collect();

    assert_eq!(
        listed,
        // Alfabetycznie, bo `list_skills_in` zwija oba katalogi docelowe do `BTreeSet`:
        // `notatki`, `pdf`, `release-notes`, `review-pull-requests`.
        vec![
            (BY_HAND.to_owned(), false),
            (FROM_A_LINK.to_owned(), true),
            (FROM_BEFORE.to_owned(), true),
            (WRITTEN_HERE.to_owned(), false),
        ],
        "the list read off disk does not tell the truth about where these four skills came from. \
         The marker stands in for the signing and provenance v1 does not have, so it has to be \
         lit exactly where the content MAY be a stranger's: a pasted link yes, a skill this \
         person typed here no, somebody else's folder no, and a canonical copy nobody recorded \
         yes — because before this task those existed only on the link path. Order is \
         alphabetical and part of the answer: a list that reshuffles between two visits to the \
         section makes a person look for a row where they last saw it"
    );
}

// ── (b) brak zapisu jest ostrożnym „tak", nie „napisana tutaj" ──────────────────────────────

#[test]
fn a_canonical_copy_nobody_recorded_is_from_a_link_and_not_written_here() {
    let world = empty_world();
    plant_canonical(&world.library, FROM_BEFORE);
    install_skill_into(&world.library, FROM_BEFORE, Landing::Everywhere, None)
        .expect("an older skill installs");

    assert_eq!(
        marked(&world.library, FROM_BEFORE),
        Some(true),
        "'{FROM_BEFORE}' has a canonical copy and no record of where it came from. Until this \
         task canonical copies were made in `review_skill_inner` and nowhere else, so the only \
         honest answer is the cautious one — the same rule `DeepScan::Unavailable` and \
         `Discovery::Unknown` already hold: absence of proof is not proof of absence. Answering \
         'written here' would put out the marker on exactly the skills whose text came from a \
         stranger"
    );

    // Kontrola przeciw stałej: ta sama umiejętność Z zapisem musi odpowiedzieć inaczej. Bez tej
    // połowy asercja wyżej przechodzi też na liście, która na wszystko mówi „z internetu".
    remember_origin(&world.library, FROM_BEFORE, false)
        .expect("recording where a skill came from is the whole point of this criterion");
    assert_eq!(
        marked(&world.library, FROM_BEFORE),
        Some(false),
        "the record says this skill was written here and the list still says it came from a \
         link, so the list is not reading the record at all — it is answering from the existence \
         of the canonical copy, which is the premise this task retires"
    );
}

// ── (c) zapis nie mieszka w sidecarze instalacji ────────────────────────────────────────────

#[test]
fn where_a_skill_came_from_does_not_live_in_the_install_sidecar() {
    let world = empty_world();
    plant_canonical(&world.library, FROM_A_LINK);
    plant_canonical(&world.library, FROM_BEFORE);
    install_skill_into(&world.library, FROM_A_LINK, Landing::Everywhere, None)
        .expect("a reviewed skill installs");

    let sidecar = world.library.join(SKILLS_DIR).join(SIDECAR_FILE);
    let before = recorded_paths(&sidecar);
    assert_eq!(
        before.len(),
        2,
        "the install sidecar should hold the two vendor directories `install_skill_into` just \
         wrote, and it holds {before:?}. Everything below compares against this set, and an \
         empty one agrees with anything"
    );

    remember_origin(&world.library, FROM_A_LINK, true)
        .expect("recording where a skill came from is the whole point of this criterion");
    assert_eq!(
        recorded_paths(&sidecar),
        before,
        "recording where '{FROM_A_LINK}' came from changed the entries in {SIDECAR_FILE}. That \
         file answers one question — which vendor directory did Loadout write — and `remove` \
         refuses to delete anything that is not in it"
    );

    // I ta połowa jest tą, na której to naprawdę stoi: `place::write_sidecar` odtwarza CAŁY plik
    // ze zbioru ścieżek, więc pochodzenie dopisane do niego przepada przy następnej instalacji.
    install_skill_into(&world.library, FROM_BEFORE, Landing::Everywhere, None)
        .expect("a second skill installs");
    assert_eq!(
        marked(&world.library, FROM_A_LINK),
        Some(true),
        "installing a second skill lost the record of where '{FROM_A_LINK}' came from. \
         `place::write_sidecar` rebuilds {SIDECAR_FILE} from the set of paths alone, so anything \
         living in that file is erased by the next install or remove — silently, and only for \
         skills imported before it"
    );
}

// ── (d) zapis nie jedzie do katalogów vendorów ──────────────────────────────────────────────

#[test]
fn the_record_does_not_ride_along_into_the_agent_directories() {
    let world = empty_world();
    let canonical = plant_canonical(&world.library, FROM_A_LINK);
    remember_origin(&world.library, FROM_A_LINK, true)
        .expect("recording where a skill came from is the whole point of this criterion");
    install_skill_into(&world.library, FROM_A_LINK, Landing::Everywhere, None)
        .expect("a reviewed skill installs");

    let wanted = relative_files(&canonical);
    assert!(
        wanted.contains(&PathBuf::from("scripts").join("run.sh")),
        "the fixture planted no bundled file, so comparing the two trees below would not tell \
         'copied exactly what the canonical copy holds' from 'copied one file'. Canonical copy \
         holds: {wanted:?}"
    );

    for root in vendor_roots(&world.home) {
        let installed = root.join(FROM_A_LINK);
        assert_eq!(
            relative_files(&installed),
            wanted,
            "{} holds something the canonical copy does not. `ingest::bundled_files` takes EVERY \
             neighbour of {SKILL_FILE}, so a record of where the skill came from written next to \
             it becomes a bundled file of the skill and rides into the live folders this person's \
             agent apps read on every run",
            installed.display()
        );
    }

    // Kontrola: zapis pochodzenia naprawdę istnieje, więc asercja wyżej mówi o świecie, w którym
    // jest co przenieść. Bez niej przechodzi ona na implementacji, która nie zapisuje nic.
    assert_eq!(
        marked(&world.library, FROM_A_LINK),
        Some(true),
        "nothing records where '{FROM_A_LINK}' came from, so the comparison above is about two \
         trees that were never at risk of differing"
    );
}
