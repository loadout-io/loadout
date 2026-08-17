//! AC-3 dla T-19: dołączonego skryptu nikt nie uruchamia — ani przy imporcie, ani przy
//! instalacji — a mimo to skrypt dojeżdża na miejsce w całości.
//!
//! **Słabą wersją tego kryterium jest samo `assert!(!sentinel.exists())`.** Przechodzi ją
//! implementacja, która skryptu w ogóle nie skopiowała, czyli import gubiący po cichu połowę
//! umiejętności: `SKILL.md` mówi „uruchom `scripts/run.sh`", katalog takiego pliku nie ma,
//! a użytkownik dowiaduje się o tym przy pierwszym użyciu, długo po zielonym „Installed".
//! Przechodzi ją też skrypt, który jest zepsuty i nie stworzyłby pliku-sentinela, nawet gdyby
//! ktoś go odpalił — czyli sonda, która nic nie mierzy.
//!
//! Rozróżniają trzy asercje naraz: sentinel NIE istnieje, skrypt JEST w obu katalogach
//! docelowych z tymi samymi bajtami, i — na końcu — sonda jest żywa: ten sam plik, uruchomiony
//! wprost, sentinela tworzy. Bez tej trzeciej dwie pierwsze przechodzą na `touch`, który nigdy
//! nie działał (niezmiennik 20: zasadź prawdziwe naruszenie i wymagaj, żeby było widać).

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
// `PermissionsExt` to jedyny sposób nadać bit wykonywalności. Wolno go tu użyć: niezmiennik 3
// dotyczy kodu wysyłanego, a `checks/quick-boundary.sh` wyłącza pliki testowe po ścieżce.
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use loadout_lib::skills::ingest;
use loadout_lib::skills::place;
use loadout_lib::skills::{Roots, Scope};

const NAME: &str = "pdf";

/// Pobrany `SKILL.md`. Treść jest nudna naumyślnie — to kryterium jest o skrypcie obok niej,
/// nie o skanowaniu.
const SKILL_MD: &str = concat!(
    "---\n",
    "name: pdf\n",
    "description: Extracts text and tables from PDF files.\n",
    "---\n",
    "\n",
    "# PDF\n",
    "\n",
    "Read the file first, then answer from what it says.\n",
);

/// Uprawnienia dołączonego skryptu w źródle: wykonywalny, jak każdy skrypt, na który wskazuje
/// ciało umiejętności.
const SOURCE_MODE: u32 = 0o755;

/// Katalog pobrania, dwa katalogi docelowe i miejsce, w którym leżałby dowód uruchomienia.
struct World {
    /// Trzyma katalog tymczasowy przy życiu na czas testu; kasuje go `Drop`.
    _tmp: tempfile::TempDir,
    /// To, co „przyszło z sieci" — katalog, który dostaje [`ingest::from_folder`].
    source: PathBuf,
    /// Plik, którego istnienie znaczy „ktoś uruchomił dołączony skrypt".
    sentinel: PathBuf,
    home: PathBuf,
    roots: Roots,
}

fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("downloaded");
    let home = tmp.path().join("home");
    // Sentinel leży w INNYM katalogu niż źródło i niż cele: gdyby leżał w którymkolwiek
    // z nich, „nie ma go" dałoby się osiągnąć sprzątaniem, a nie nieuruchamianiem.
    let sentinel = tmp.path().join("watch").join("pwned");
    let data = tmp.path().join("data");

    fs::create_dir_all(source.join("scripts")).unwrap();
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(sentinel.parent().unwrap()).unwrap();

    fs::write(source.join("SKILL.md"), SKILL_MD).unwrap();
    fs::write(
        source.join("scripts").join("pwn.sh"),
        format!("#!/bin/sh\ntouch \"{}\"\n", sentinel.display()),
    )
    .unwrap();
    fs::set_permissions(
        source.join("scripts").join("pwn.sh"),
        fs::Permissions::from_mode(SOURCE_MODE),
    )
    .unwrap();

    World {
        _tmp: tmp,
        source,
        sentinel,
        home: home.clone(),
        roots: Roots {
            home,
            project: None,
            data,
        },
    }
}

/// Dwa katalogi umiejętności pod domem. **Wypisane literalnie**, nie wzięte z
/// `DESTINATION_DIRS`: kryterium sprawdzające implementację jej własną tablicą przechodzi
/// po każdej zmianie tej tablicy, łącznie z literówką.
fn installed_scripts(home: &Path) -> [PathBuf; 2] {
    [
        home.join(".claude/skills")
            .join(NAME)
            .join("scripts/pwn.sh"),
        home.join(".agents/skills")
            .join(NAME)
            .join("scripts/pwn.sh"),
    ]
}

#[test]
fn the_whole_pass_ends_with_the_script_in_place_and_never_run() {
    let world = world();
    let source_script = world.source.join("scripts").join("pwn.sh");
    let source_bytes = fs::read(&source_script).unwrap();

    // Cały przebieg, krok po kroku, tak jak zrobi go aplikacja: rozpoznanie i pobranie
    // (katalog, do którego trafiły bajty) → normalizacja i skan → walidacja → instalacja.
    let import = ingest::from_folder(&world.source)
        .expect("reading a downloaded skill folder from disk should not fail");
    let plan = place::plan(&import.skill, Scope::Global, &world.roots)
        .expect("a skill with a valid name and a description should be plannable");
    place::apply(&plan, &import.skill).expect("apply could not carry out its own plan");

    assert!(
        fs::symlink_metadata(&world.sentinel).is_err(),
        "{} exists, so something in the pass ran the bundled script. A skill from the internet \
         is a set of instructions, and `scripts/` is the part of it that does not need an agent \
         to decide anything — it runs the moment anybody calls it [T5 §5.4]",
        world.sentinel.display()
    );

    for installed in installed_scripts(&world.home) {
        assert_eq!(
            fs::read(&installed).unwrap_or_default(),
            source_bytes,
            "{} does not carry the bytes that came in. `nobody ran it` is trivially true of a \
             script that was never copied, and a skill whose body says `scripts/pwn.sh` and \
             whose folder has no such file fails on first use, long after `Installed`",
            installed.display()
        );
    }

    assert_eq!(
        import.scripts, 1,
        "the review card is handed the number of bundled scripts, and it is one here. The card \
         says `Includes N scripts — these will not run unless an agent chooses to run them.`, \
         and a number that is not counted from what arrived says the same thing about a skill \
         that bundles none"
    );

    // SONDA MUSI BYĆ ŻYWA. Bez tego kroku obie asercje wyżej przechodzą także wtedy, gdy
    // `pwn.sh` jest zepsuty i nie stworzyłby sentinela nawet uruchomiony wprost — czyli
    // kryterium mierzyłoby własną fikstę, nie zachowanie kodu (niezmiennik 20).
    //
    // Uruchamiamy KOPIĘ ZAINSTALOWANĄ, nie źródło: to samo jedno wywołanie dowodzi przy okazji,
    // że bit wykonywalności przeżył instalację, więc „skrypt jest na miejscu" znaczy „da się go
    // użyć", a nie „bajty leżą".
    let [claude_copy, _] = installed_scripts(&world.home);
    let ran = Command::new(&claude_copy)
        .status()
        .expect("the installed script should be runnable when somebody deliberately runs it");
    assert!(ran.success(), "the planted script did not run cleanly");
    assert!(
        fs::symlink_metadata(&world.sentinel).is_ok(),
        "run on purpose, {} still did not create {}. The script is inert, so `the sentinel is \
         absent` was never a statement about the code under test",
        claude_copy.display(),
        world.sentinel.display()
    );
}
