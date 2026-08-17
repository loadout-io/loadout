//! AC-4 dla T-18: plan pokazuje wszystko, co zostanie zapisane, i sam nie zapisuje nic.
//!
//! **Słabą wersją tego kryterium jest `assert_eq!(plan.writes.len(), 2)`.** Przechodzi ją
//! implementacja, która przy okazji tworzy katalogi „żeby sprawdzić uprawnienia" — po czym
//! odmowa w kroku walidacji zostawia śmieci, których nikt nie posprząta — i taka, która przy
//! `apply` pisze trzeci plik, o którym plan nic nie mówił.
//!
//! Rozróżniają: **rekurencyjny listing obu drzew docelowych ze ścieżką, rozmiarem i `mtime`,
//! porównany przed i po `plan()`**, oraz różnica zbiorów po `apply()`.
//!
//! `mtime` jest w listingu nieprzypadkowo: zapis tej samej długości pod tą samą ścieżką nie
//! zmienia ani jednego z pozostałych pól, a katalog, do którego coś dołożono, zmienia swój.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use loadout_lib::skills::place::{self, Conflict, InstallPlan};
use loadout_lib::skills::{BundledFile, Roots, Scope, Skill};

const NAME: &str = "pdf";
const DESCRIPTION: &str = "Extracts text and tables from PDF files.";
const RUN_SH: &str = "#!/bin/sh\nexit 0\n";
const API_MD: &str = "# The API\n";

/// Cudza umiejętność o innej nazwie — stoi w obu katalogach przez cały test i ma z nich
/// wyjść nietknięta.
const OTHER_MD: &str = "---\nname: other-skill\ndescription: Not ours.\n---\n";

/// Cudzy katalog o **tej samej** nazwie. Pierwszy wiersz jest rozpoznawalny, bo to jego
/// cytuje `Conflict::Foreign`.
const FOREIGN_FIRST_LINE: &str = "# Not ours";
const FOREIGN_MD: &str = "# Not ours\n\nSomeone else put this folder here by hand.\n";

struct World {
    _tmp: tempfile::TempDir,
    roots: Roots,
    skill: Skill,
}

fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let data = tmp.path().join("data");
    let canonical = data.join("skills").join(NAME);

    fs::create_dir_all(canonical.join("scripts")).unwrap();
    fs::create_dir_all(canonical.join("references")).unwrap();
    fs::write(canonical.join("SKILL.md"), "---\nname: pdf\n---\n").unwrap();
    fs::write(canonical.join("scripts/run.sh"), RUN_SH).unwrap();
    fs::write(canonical.join("references/api.md"), API_MD).unwrap();
    fs::create_dir_all(&home).unwrap();

    let skill = Skill {
        name: NAME.to_owned(),
        description: DESCRIPTION.to_owned(),
        body: "Read the file first.\n".to_owned(),
        files: vec![
            BundledFile {
                relative: PathBuf::from("scripts/run.sh"),
                source: canonical.join("scripts/run.sh"),
            },
            BundledFile {
                relative: PathBuf::from("references/api.md"),
                source: canonical.join("references/api.md"),
            },
        ],
        ..Skill::default()
    };

    World {
        _tmp: tmp,
        roots: Roots {
            home,
            project: None,
            data,
        },
        skill,
    }
}

/// Dwa korzenie vendorów. **Wypisane literalnie**, nie wzięte z `DESTINATION_DIRS`.
fn vendor_roots(home: &Path) -> [PathBuf; 2] {
    [
        home.join(".claude").join("skills"),
        home.join(".agents").join("skills"),
    ]
}

fn skill_dirs(home: &Path) -> [PathBuf; 2] {
    vendor_roots(home).map(|root| root.join(NAME))
}

/// Zakłada oba korzenie vendorów i wkłada do każdego cudzą umiejętność o innej nazwie.
fn seed_vendor_roots(home: &Path) {
    for root in vendor_roots(home) {
        let other = root.join("other-skill");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("SKILL.md"), OTHER_MD).unwrap();
    }
}

/// (ścieżka, rozmiar, `mtime`) dla całego drzewa, posortowane.
///
/// `DirEntry::metadata` nie podąża za dowiązaniem, więc link zobaczymy jako link, a nie jako
/// to, na co wskazuje.
fn listing(root: &Path) -> Vec<(PathBuf, u64, SystemTime)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            let path = entry.path();
            if meta.is_dir() {
                stack.push(path.clone());
            }
            out.push((path, meta.len(), meta.modified().unwrap_or(UNIX_EPOCH)));
        }
    }
    out.sort();
    out
}

/// Oba drzewa docelowe naraz, w jednej posortowanej liście.
fn destinations_listing(home: &Path) -> Vec<(PathBuf, u64, SystemTime)> {
    let mut out: Vec<_> = vendor_roots(home).iter().flat_map(|r| listing(r)).collect();
    out.sort();
    out
}

fn paths(listing: &[(PathBuf, u64, SystemTime)]) -> HashSet<PathBuf> {
    listing.iter().map(|(path, _, _)| path.clone()).collect()
}

fn make_plan(world: &World) -> InstallPlan {
    place::plan(&world.skill, Scope::Global, &world.roots)
        .expect("plan refused a skill with a valid name and description")
}

#[test]
fn plan_does_not_touch_a_single_byte() {
    let world = world();
    seed_vendor_roots(&world.roots.home);

    let before = destinations_listing(&world.roots.home);
    let data_before = listing(&world.roots.data);
    let plan = make_plan(&world);
    let after = destinations_listing(&world.roots.home);

    assert_eq!(
        before, after,
        "plan() changed the destination trees. It exists to be read by a person before they \
         agree to anything; a plan that installs while it reports is not a plan.\n  before: \
         {before:?}\n  after:  {after:?}"
    );
    assert_eq!(
        data_before,
        listing(&world.roots.data),
        "plan() wrote into app data — the canonical copy and the sidecar are `apply`'s business"
    );
    assert!(
        fs::symlink_metadata(&plan.sidecar).is_err(),
        "plan() created its own sidecar at {}. Nothing is installed yet, so the record saying \
         `Loadout wrote this` is a lie that outlives the user pressing Cancel",
        plan.sidecar.display()
    );
}

#[test]
fn plan_names_the_two_directories_and_no_conflicts_on_open_ground() {
    let world = world();
    seed_vendor_roots(&world.roots.home);
    let plan = make_plan(&world);

    let mut got = plan.writes.clone();
    got.sort();
    let mut want = skill_dirs(&world.roots.home).to_vec();
    want.sort();
    assert_eq!(
        got, want,
        "the plan does not name the two directories that make all six tools see the skill"
    );
    assert!(
        plan.conflicts.is_empty(),
        "nothing called `{NAME}` was there, yet the plan reports {:?}",
        plan.conflicts
    );
}

#[test]
fn apply_creates_exactly_the_paths_the_plan_named_and_nothing_else() {
    let world = world();
    seed_vendor_roots(&world.roots.home);
    let before = destinations_listing(&world.roots.home);
    let plan = make_plan(&world);
    place::apply(&plan, &world.skill).expect("apply could not carry out its own plan");
    let after = destinations_listing(&world.roots.home);

    let new: HashSet<PathBuf> = paths(&after).difference(&paths(&before)).cloned().collect();
    let mut want = HashSet::new();
    for dir in skill_dirs(&world.roots.home) {
        want.insert(dir.join("SKILL.md"));
        want.insert(dir.join("scripts"));
        want.insert(dir.join("scripts").join("run.sh"));
        want.insert(dir.join("references"));
        want.insert(dir.join("references").join("api.md"));
        want.insert(dir);
    }

    assert_eq!(
        new,
        want,
        "apply() did not create exactly what the plan named.\n  it made but did not promise: \
         {:?}\n  it promised but did not make: {:?}",
        new.difference(&want).collect::<Vec<_>>(),
        want.difference(&new).collect::<Vec<_>>()
    );

    // Cudza umiejętność stała obok przez cały zapis. Rozmiar i `mtime` bez zmian.
    let untouched: Vec<_> = before
        .iter()
        .filter(|(path, _, _)| path.components().any(|c| c.as_os_str() == "other-skill"))
        .collect();
    assert!(
        !untouched.is_empty(),
        "the test seeded no neighbour to guard"
    );
    for entry in untouched {
        assert!(
            after.contains(entry),
            "{} changed while a different skill was being installed",
            entry.0.display()
        );
    }
}

#[test]
fn a_directory_we_wrote_is_an_update_and_a_stranger_is_foreign() {
    let world = world();
    seed_vendor_roots(&world.roots.home);
    let first = make_plan(&world);
    place::apply(&first, &world.skill).expect("apply could not carry out its own plan");

    let again = make_plan(&world);
    for dir in skill_dirs(&world.roots.home) {
        assert!(
            again
                .conflicts
                .contains(&Conflict::Update { path: dir.clone() }),
            "{} was written by Loadout and is in the sidecar, so re-installing over it is an \
             update, not a stranger. The plan says: {:?}",
            dir.display(),
            again.conflicts
        );
    }
    assert_eq!(again.conflicts.len(), 2, "{:?}", again.conflicts);
}

#[test]
fn a_directory_nobody_recorded_is_quoted_back_to_the_person() {
    let world = world();
    seed_vendor_roots(&world.roots.home);
    for dir in skill_dirs(&world.roots.home) {
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), FOREIGN_MD).unwrap();
    }

    let plan = make_plan(&world);
    for dir in skill_dirs(&world.roots.home) {
        assert!(
            plan.conflicts.contains(&Conflict::Foreign {
                path: dir.clone(),
                first_line: FOREIGN_FIRST_LINE.to_owned(),
            }),
            "{} is not in the sidecar, so Loadout did not write it — overwriting it silently \
             destroys someone else's work, and the first line is what lets a person recognise \
             whose. The plan says: {:?}",
            dir.display(),
            plan.conflicts
        );
    }
    assert_eq!(plan.conflicts.len(), 2, "{:?}", plan.conflicts);
}
