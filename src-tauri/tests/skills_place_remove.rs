//! AC-6 dla T-18: usunięcie zabiera obie kopie i nic poza nimi.
//!
//! **Słabą wersją tego kryterium jest `assert!(!dir.exists())`.** Przechodzi ją implementacja,
//! która skasowała cały `.claude/skills/`, i taka, która skasowała cudzą umiejętność o tej
//! samej nazwie. Obie „działają" dokładnie do pierwszego użytkownika, który miał tam coś
//! swojego, i obie są nie do odzyskania.
//!
//! Rozróżniają: obecność sąsiada `other-skill/` z **tymi samymi bajtami** po usunięciu oraz
//! drugi scenariusz, w którym katalog o naszej nazwie napisał ktoś inny.
//!
//! Kanoniczna kopia w danych aplikacji zostaje w obu scenariuszach: katalogi vendorów są
//! wyjściem builda, a źródło jest jedno (niezmiennik 4). Usunięcie, które kasuje źródło,
//! zamienia „odinstaluj z Codeksa" w „skasuj umiejętność".

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use loadout_lib::skills::place::{self, Removed};
use loadout_lib::skills::{BundledFile, Roots, Scope, Skill};

const NAME: &str = "pdf";
const DESCRIPTION: &str = "Extracts text and tables from PDF files.";
const RUN_SH: &str = "#!/bin/sh\nexit 0\n";
const CANONICAL_MD: &str = "---\nname: pdf\nargument-hint: <file.pdf>\n---\n\nAuthored here.\n";

/// Sąsiad: cudza umiejętność o innej nazwie, w obu katalogach, po obu stronach testu.
const OTHER_MD: &str = "---\nname: other-skill\ndescription: Not ours.\n---\n\nSomebody's.\n";
const OTHER_NOTE: &str = "a file only the other skill has\n";

/// Cudzy katalog o **naszej** nazwie: Loadout go nie pisał, więc nie ma go w sidecarze.
const FOREIGN_MD: &str = "---\nname: pdf\ndescription: Someone else's pdf skill.\n---\n";

struct World {
    _tmp: tempfile::TempDir,
    roots: Roots,
    skill: Skill,
    canonical: PathBuf,
}

fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let data = tmp.path().join("data");
    let canonical = data.join("skills").join(NAME);

    fs::create_dir_all(canonical.join("scripts")).unwrap();
    fs::write(canonical.join("SKILL.md"), CANONICAL_MD).unwrap();
    fs::write(canonical.join("scripts/run.sh"), RUN_SH).unwrap();
    fs::create_dir_all(&home).unwrap();

    let skill = Skill {
        name: NAME.to_owned(),
        description: DESCRIPTION.to_owned(),
        body: "Read the file first.\n".to_owned(),
        files: vec![BundledFile {
            relative: PathBuf::from("scripts/run.sh"),
            source: canonical.join("scripts/run.sh"),
        }],
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
        canonical,
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

fn seed_neighbour(home: &Path) {
    for root in vendor_roots(home) {
        let other = root.join("other-skill");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("SKILL.md"), OTHER_MD).unwrap();
        fs::write(other.join("note.md"), OTHER_NOTE).unwrap();
    }
}

/// (ścieżka względna, bajty) dla każdego pliku w drzewie, posortowane. Katalogi pomijamy:
/// pytanie brzmi „czy bajty są nietknięte", a katalog bajtów nie ma.
fn files(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                stack.push(path);
            } else if let Ok(bytes) = fs::read(&path) {
                let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
                out.push((relative, bytes));
            }
        }
    }
    out.sort();
    out
}

fn install(world: &World) {
    let plan = place::plan(&world.skill, Scope::Global, &world.roots)
        .expect("plan refused a skill with a valid name and description");
    place::apply(&plan, &world.skill).expect("apply could not carry out its own plan");
}

#[test]
fn removing_our_skill_takes_both_copies() {
    let world = world();
    seed_neighbour(&world.roots.home);
    install(&world);

    let outcome = place::remove(NAME, Scope::Global, &world.roots)
        .expect("remove failed on a skill Loadout installed itself");

    let mut want = skill_dirs(&world.roots.home).to_vec();
    want.sort();
    let paths = match outcome {
        Removed::Done { mut paths } => {
            paths.sort();
            paths
        }
        Removed::Skipped { path, why } => {
            // Nasza własna instalacja została uznana za cudzą — czyli sidecar nie zapisał,
            // że to my. Wtedy „Remove" nigdy niczego nie usunie.
            Vec::from([PathBuf::from(format!(
                "<skipped {}: {why}>",
                path.display()
            ))])
        }
    };
    assert_eq!(
        paths, want,
        "remove() did not report taking both copies. One vendor keeps the skill and the person \
         who pressed Remove has no way to find out which"
    );

    for dir in skill_dirs(&world.roots.home) {
        assert!(
            fs::symlink_metadata(&dir).is_err(),
            "{} is still there after Remove",
            dir.display()
        );
    }
}

#[test]
fn the_neighbour_and_the_canonical_copy_survive_the_removal() {
    let world = world();
    seed_neighbour(&world.roots.home);
    let neighbours: Vec<_> = vendor_roots(&world.roots.home)
        .map(|root| files(&root.join("other-skill")))
        .to_vec();
    let source = files(&world.canonical);
    install(&world);

    place::remove(NAME, Scope::Global, &world.roots)
        .expect("remove failed on a skill Loadout installed itself");

    for (root, before) in vendor_roots(&world.roots.home).iter().zip(&neighbours) {
        let other = root.join("other-skill");
        assert_eq!(
            &files(&other),
            before,
            "{} changed while a different skill was being removed. Deleting the whole \
             `skills/` folder is the cheapest way to make `!dir.exists()` true, and it takes \
             everything the user ever put there",
            other.display()
        );
    }

    assert_eq!(
        files(&world.canonical),
        source,
        "the canonical copy in app data is gone or changed. The two vendor directories are \
         build output; the canonical skill is the source (invariant 4). Removing the source \
         turns `uninstall from Codex` into `delete the skill`"
    );
}

#[test]
fn a_directory_loadout_never_wrote_is_left_where_it_is() {
    let world = world();
    seed_neighbour(&world.roots.home);
    // Nic nie instalujemy: te katalogi są cudze i sidecar o nich nie wie.
    for dir in skill_dirs(&world.roots.home) {
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), FOREIGN_MD).unwrap();
    }
    let before: Vec<_> = skill_dirs(&world.roots.home)
        .map(|dir| files(&dir))
        .to_vec();

    let outcome = place::remove(NAME, Scope::Global, &world.roots)
        .expect("remove reported an error where it should have reported a refusal");

    assert!(
        matches!(outcome, Removed::Skipped { .. }),
        "a `{NAME}` directory Loadout never wrote was treated as ours; remove() said \
         {outcome:?}. Name collision is normal — the sidecar is the only thing that says which \
         of the two is ours"
    );
    if let Removed::Skipped { path, why } = &outcome {
        assert!(
            !why.trim().is_empty(),
            "{} was left alone with no sentence saying why, so the person sees a Remove that \
             did nothing",
            path.display()
        );
    }

    for (dir, bytes) in skill_dirs(&world.roots.home).iter().zip(&before) {
        assert_eq!(
            &files(dir),
            bytes,
            "{} belongs to somebody else and its bytes changed",
            dir.display()
        );
    }
}
