//! AC-1 dla T-18: umiejętność ląduje jako dwie **niezależne kopie**, nie jako dowiązanie.
//!
//! **Słabą wersją tego kryterium jest `assert!(p.exists())`.** `exists()` podąża za
//! dowiązaniem, więc przechodzi na implementacji, która zrobiła symlink — a symlink w repo
//! scommitowanym przez zespół rozpada się u każdego, kto je sklonuje, i jest największym
//! zagrożeniem dla portu na Windows w całym tym projekcie [T5 §4.5]. Przechodzi też na
//! twardym dowiązaniu, po którym „dwie kopie" to jeden plik i edycja jednej zmienia drugą.
//!
//! Rozróżniają dopiero dwie rzeczy naraz: `fs::symlink_metadata`, które **nie** podąża za
//! linkiem, i porównanie pary `(dev, ino)` obu kopii `SKILL.md`. Sam numer i-węzła nie
//! wystarczy — dwa pliki na różnych urządzeniach mogą go dzielić.
//!
//! Test nie dotyka prawdziwego `~/.claude/skills`: „dom", „repo" i „dane aplikacji" to trzy
//! katalogi w jednym `TempDir`.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
// `MetadataExt` to jedyny sposób zapytać o i-węzeł, a `PermissionsExt` — o bit wykonywalności.
// Wolno ich tu użyć: niezmiennik 3 dotyczy kodu wysyłanego, a `checks/quick-boundary.sh`
// wyłącza pliki testowe po ścieżce.
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use loadout_lib::skills::place;
use loadout_lib::skills::{BundledFile, Roots, Scope, Skill};

const NAME: &str = "pdf";
const DESCRIPTION: &str = "Extracts text and tables from PDF files. Use it when the user \
                           points at a .pdf and asks what is inside.";
const BODY: &str = "Read the file first, then answer from what it says.\n";

const RUN_SH: &str = "#!/bin/sh\nexit 0\n";
const API_MD: &str = "# The API\n\nOne level deep, as the spec asks.\n";

/// Uprawnienia kanonicznego `scripts/run.sh`: **wykonywalny**, jak każdy skrypt, na który
/// wskazuje ciało umiejętności. Bez tego porównanie uprawnień w [`assert_installed`] nic nie
/// znaczy — źródło z domyślnym `0644` daje po drugiej stronie tę samą wartość także wtedy,
/// gdy instalacja zapisała bajty przez `fs::write` i bit wykonywalności zgubiła.
const SOURCE_MODE: u32 = 0o755;

/// Kanoniczny `SKILL.md` w danych aplikacji. Celowo **inny** niż to, co wypluje `emit`:
/// niesie pole spoza specyfikacji, którego wersja instalowana mieć nie może. Gdyby
/// instalacja pisała po źródle, ta różnica jest tym, co to widzi.
const CANONICAL_MD: &str = "---\nname: pdf\nargument-hint: <file.pdf>\n---\n\nAuthored here.\n";

/// Dom, repo i dane aplikacji — trzy korzenie w jednym katalogu tymczasowym.
struct World {
    /// Trzyma katalog tymczasowy przy życiu na czas testu; kasuje go `Drop`.
    _tmp: tempfile::TempDir,
    roots: Roots,
    skill: Skill,
    /// Kanoniczna kopia w danych aplikacji, czyli źródło (niezmiennik 4).
    canonical: PathBuf,
}

fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let project = tmp.path().join("repo");
    let data = tmp.path().join("data");
    let canonical = data.join("skills").join(NAME);

    fs::create_dir_all(canonical.join("scripts")).unwrap();
    fs::create_dir_all(canonical.join("references")).unwrap();
    fs::write(canonical.join("SKILL.md"), CANONICAL_MD).unwrap();
    fs::write(canonical.join("scripts/run.sh"), RUN_SH).unwrap();
    fs::set_permissions(
        canonical.join("scripts/run.sh"),
        fs::Permissions::from_mode(SOURCE_MODE),
    )
    .unwrap();
    fs::write(canonical.join("references/api.md"), API_MD).unwrap();
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&project).unwrap();

    let skill = Skill {
        name: NAME.to_owned(),
        description: DESCRIPTION.to_owned(),
        body: BODY.to_owned(),
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
            project: Some(project),
            data,
        },
        skill,
        canonical,
    }
}

/// Dwa katalogi umiejętności pod danym korzeniem. **Wypisane literalnie**, nie wzięte
/// z `DESTINATION_DIRS`: kryterium sprawdzające implementację jej własną tablicą przechodzi
/// po każdej zmianie tej tablicy, łącznie z literówką.
fn skill_dirs(root: &Path) -> [PathBuf; 2] {
    [
        root.join(".claude").join("skills").join(NAME),
        root.join(".agents").join("skills").join(NAME),
    ]
}

fn install(world: &World, scope: Scope) {
    let plan = place::plan(&world.skill, scope, &world.roots)
        .expect("plan refused a skill that carries a valid name, a description and nothing else");
    place::apply(&plan, &world.skill).expect("apply could not carry out its own plan");
}

/// Zawartość pliku albo zdanie mówiące, czego zabrakło — żeby asercja równości pokazała
/// ścieżkę zamiast bezimiennego „No such file".
fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| format!("<nothing readable at {}: {error}>", path.display()))
}

/// Bity uprawnień pliku, albo `0`, kiedy pliku nie ma — asercja równości pokazuje wtedy
/// różnicę zamiast panikować bez nazwy ścieżki.
///
/// `metadata`, nie `symlink_metadata`: pytanie brzmi „co dostanie ten, kto uruchomi ten plik",
/// a dowiązanie w miejscu kopii wyklucza osobno [`neither_copy_is_a_link_and_the_two_are_different_files`].
fn mode(path: &Path) -> u32 {
    fs::metadata(path).map_or(0, |meta| meta.permissions().mode() & 0o777)
}

fn dev_ino(path: &Path) -> (u64, u64) {
    let meta = fs::symlink_metadata(path).expect("symlink_metadata on a path that should exist");
    (meta.dev(), meta.ino())
}

/// `SKILL.md` bajt w bajt z tym, co zwrócił `emit`, plus oba dołączone pliki w tym samym
/// układzie względnym i z tymi samymi uprawnieniami, co w kopii kanonicznej.
fn assert_installed(dir: &Path, doc: &str, canonical: &Path) {
    assert_eq!(
        read(&dir.join("SKILL.md")),
        doc,
        "the file at {} is not the bytes emit() returned. Two destinations, one artifact: \
         a second wording of the same skill is a second thing to debug",
        dir.display()
    );
    assert_eq!(
        read(&dir.join("scripts").join("run.sh")),
        RUN_SH,
        "scripts/run.sh did not arrive under {} in the same relative place it holds in the \
         canonical copy; a skill whose body says `scripts/run.sh` and whose folder does not \
         have one is a skill that fails on first use",
        dir.display()
    );
    // Bajty to nie wszystko, co ten plik niesie. `fs::copy` zachowuje uprawnienia,
    // `fs::write(fs::read(..))` je gubi — i gubi je po cichu: `run.sh` bez bitu
    // wykonywalności ma tę samą treść, wygląda na zainstalowany i przewraca się dopiero
    // u użytkownika, przy pierwszym uruchomieniu, długo po komunikacie „Installed".
    //
    // Porównanie jest ze ŹRÓDŁEM, nie ze stałą i nie z `mode & 0o111 != 0`: to pierwsze
    // przestałoby mieć związek z tym, co `fs::copy` naprawdę przenosi, a to drugie przechodzi
    // na dowolnym trybie, w którym cokolwiek jest wykonywalne.
    assert_eq!(
        mode(&dir.join("scripts").join("run.sh")),
        mode(&canonical.join("scripts").join("run.sh")),
        "scripts/run.sh under {} does not carry the permissions it has in the canonical copy. \
         The bytes arrived and the executable bit did not, so the skill installs cleanly and \
         its script cannot be run",
        dir.display()
    );
    assert_eq!(
        read(&dir.join("references").join("api.md")),
        API_MD,
        "references/api.md did not arrive under {}",
        dir.display()
    );
}

#[test]
fn global_scope_writes_both_vendor_names_under_home() {
    let world = world();
    let (doc, _) = place::emit(&world.skill);
    install(&world, Scope::Global);

    for dir in skill_dirs(&world.roots.home) {
        assert_installed(&dir, &doc, &world.canonical);
    }
}

#[test]
fn project_scope_writes_the_same_two_names_under_the_repo_root() {
    let world = world();
    let (doc, _) = place::emit(&world.skill);
    let project = world
        .roots
        .project
        .clone()
        .expect("this world was built with a project root");
    install(&world, Scope::Project);

    for dir in skill_dirs(&project) {
        assert_installed(&dir, &doc, &world.canonical);
    }
    // Zakres projektu ma zostać w projekcie. Zapis „przy okazji" do domu robi z umiejętności
    // projektowej globalną i jest niewidoczny do dnia, w którym zaczyna wchodzić w drogę.
    for dir in skill_dirs(&world.roots.home) {
        assert!(
            fs::symlink_metadata(&dir).is_err(),
            "a project-scoped install also wrote {}",
            dir.display()
        );
    }
}

#[test]
fn neither_copy_is_a_link_and_the_two_are_different_files() {
    let world = world();
    install(&world, Scope::Global);
    let [claude, agents] = skill_dirs(&world.roots.home);

    for path in [
        claude.clone(),
        agents.clone(),
        claude.join("SKILL.md"),
        agents.join("SKILL.md"),
    ] {
        let meta = fs::symlink_metadata(&path);
        assert!(
            meta.is_ok(),
            "nothing at all exists at {} — symlink_metadata, unlike exists(), does not \
             quietly follow a link to somewhere else",
            path.display()
        );
        assert!(
            !meta.unwrap().file_type().is_symlink(),
            "{} is a symbolic link. It works in Claude Code and nowhere we have verified; \
             committed with a project, it breaks for every teammate who clones the repo, and \
             on Windows it needs Developer Mode [T5 §4.5]",
            path.display()
        );
    }

    assert_ne!(
        dev_ino(&claude.join("SKILL.md")),
        dev_ino(&agents.join("SKILL.md")),
        "both copies of SKILL.md are the same inode, so this is one file with two names. \
         Editing one edits the other, and deleting the skill from one vendor deletes it from \
         the other"
    );
}

#[test]
fn the_canonical_copy_in_app_data_is_left_alone() {
    let world = world();
    install(&world, Scope::Global);

    // Kanoniczna umiejętność jest ŹRÓDŁEM; oba katalogi vendorów są wyjściem builda
    // (niezmiennik 4). Instalacja czyta stąd i nie pisze tutaj — inaczej po pierwszym
    // „Update" kanoniczna kopia przestaje być kanoniczna.
    assert_eq!(
        read(&world.canonical.join("SKILL.md")),
        CANONICAL_MD,
        "the install rewrote the canonical SKILL.md in app data. The vendor directories are \
         build output and the canonical copy is the source; once the install writes back into \
         it, the source is whatever the last emit happened to produce"
    );
    assert_eq!(
        read(&world.canonical.join("scripts").join("run.sh")),
        RUN_SH
    );
    assert_eq!(
        read(&world.canonical.join("references").join("api.md")),
        API_MD
    );

    for dir in skill_dirs(&world.roots.home) {
        assert_ne!(
            dev_ino(&dir.join("SKILL.md")),
            dev_ino(&world.canonical.join("SKILL.md")),
            "{} and the canonical copy are the same file",
            dir.display()
        );
    }
}
