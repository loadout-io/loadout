//! AC-3 dla T-54: flaga `--plugin-dir` powstaje tylko wtedy, gdy jest co odziedziczyć — i nigdy
//! bez wartości.
//!
//! **Słabą wersją tego kryterium jest `assert!(argv.contains(&"--plugin-dir".to_string()))`.**
//! Przechodzi dla kompozytora, który wypisuje flagę **zawsze**, także przy pustym
//! dziedziczeniu, i przechodzi dla flagi **bez wartości** — bo `contains` pyta o jeden element,
//! a `--plugin-dir` bez argumentu połknie następną flagę sterownika jako swój.
//!
//! Rozróżniają to trzy rzeczy naraz: dokładna **długość** fragmentu (2 albo 0, nigdy 1),
//! porównanie drugiego elementu z realną ścieżką jako `Path` — pod którą w tym samym teście
//! leży `skills/<nazwa>/SKILL.md` — oraz **nieistnienie katalogu** w przypadku pustym. Bez tego
//! ostatniego „pusty katalog nie trafia do argv" jest spełnialne przez kompozytor, który
//! katalog i tak stworzył, tylko go nie wymienił.
//!
//! To kryterium sądzi funkcję w module `inherit`, **nie** `engine/drivers/claude.rs`. Sterownik
//! składa argv u siebie i ten plik należy do sąsiedniego zadania tej fali; dwa zadania piszące
//! do jednego pliku to kolizja, której ta fala unika z premedytacją. Ten test nie zna słowa
//! `ClaudeDriver`.
//!
//! JEDEN `#[test]`: zaślepka zwracająca pusty fragment i nietworząca katalogu przechodzi punkty
//! (b) i (c) — rozbite na osobne zestawy dałyby w warstwie `before` obraz „w połowie zielony".
//! Przypadek pozytywny stoi więc pierwszy.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

use loadout_lib::inherit::rewrite;

const ALPHA_MD: &str = "---\nname: alpha\ndescription: Reads a log and says what broke.\n---\n\nStart from the first stack trace.\n";
const BETA_MD: &str = "---\nname: beta\ndescription: Turns a failing gate into one sentence.\n---\n\nQuote the first failing assertion.\n";

/// Gospodarz z dwiema umiejętnościami, gospodarz bez `.claude/skills`, i trzy różne katalogi
/// biegu — po jednym na każdy przypadek, żeby żaden nie oglądał śladów po poprzednim.
struct World {
    /// Trzyma katalog tymczasowy przy życiu na czas testu; kasuje go `Drop`.
    _tmp: tempfile::TempDir,
    /// Repozytorium gospodarza z dwiema umiejętnościami.
    host: PathBuf,
    /// Repozytorium **bez** katalogu `.claude/skills`.
    without_skills: PathBuf,
    /// Katalog biegu dla przypadku (a).
    run_full: PathBuf,
    /// Katalog biegu dla przypadku „pusta lista wybranych".
    run_none_selected: PathBuf,
    /// Katalog biegu dla przypadku „host bez umiejętności".
    run_bare_host: PathBuf,
}

fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let host = tmp.path().join("host-repo");
    let skills = host.join(".claude").join("skills");
    let without_skills = tmp.path().join("plain-repo");

    fs::create_dir_all(skills.join("alpha")).unwrap();
    fs::create_dir_all(skills.join("beta")).unwrap();
    fs::write(skills.join("alpha").join("SKILL.md"), ALPHA_MD).unwrap();
    fs::write(skills.join("beta").join("SKILL.md"), BETA_MD).unwrap();
    fs::create_dir_all(&without_skills).unwrap();

    let runs = tmp
        .path()
        .join("loadout-project")
        .join(".loadout")
        .join("runs");

    World {
        _tmp: tmp,
        host,
        without_skills,
        run_full: runs.join("20260819T101500__r7").join("plugin"),
        run_none_selected: runs.join("20260819T101501__r8").join("plugin"),
        run_bare_host: runs.join("20260819T101502__r9").join("plugin"),
    }
}

/// Fragment nigdy nie niesie `--plugin-dir` z wartością o zerowej długości.
///
/// Osobna funkcja, bo to pytanie zadajemy w KAŻDYM z trzech przypadków, a nie tylko w tym
/// pozytywnym. `--setting-sources ""` z sąsiedniego zadania jest flagą, której pusty argument
/// jest **poprawny**, i pomylenie tych dwóch kształtów jest realne.
fn no_flag_without_a_value(argv: &[String]) {
    for (index, argument) in argv.iter().enumerate() {
        assert!(
            argument != "--plugin-dir"
                || argv.get(index + 1).is_some_and(|value| !value.is_empty()),
            "the fragment carries --plugin-dir with nothing after it, so the driver's next flag \
             becomes its argument: {argv:?}"
        );
    }
}

#[test]
fn the_flag_names_the_directory_that_exists_and_is_absent_when_nothing_was_inherited() {
    let world = world();

    // (a) Dwie odziedziczone umiejętności → fragment dokładnie dwuelementowy.
    let selected = vec!["alpha".to_owned(), "beta".to_owned()];
    let rewritten = rewrite::plugin_dir(&world.host, &selected, &world.run_full)
        .expect("rewriting two skills of a two-skill host into a fresh run directory");
    let argv = rewrite::plugin_argv(&rewritten);

    assert_eq!(
        argv.len(),
        2,
        "the fragment for two inherited skills is not exactly [--plugin-dir, <dir>]: {argv:?}"
    );
    assert_eq!(
        argv.first().map(String::as_str),
        Some("--plugin-dir"),
        "the fragment does not start with the flag: {argv:?}"
    );
    no_flag_without_a_value(&argv);

    // Drugi element porównany jako `Path`, nie jako fragment napisu: `contains` na napisie
    // przechodzi dla ścieżki o jeden poziom obok, a o jeden poziom obok jest cała ta klasa
    // cichych porażek.
    let named = PathBuf::from(argv.get(1).expect("the fragment has a second element"));
    assert_eq!(
        named, rewritten.dir,
        "the flag names a different directory than the one the rewrite reported"
    );

    // …i pod tą ścieżką naprawdę leży to, po co vendor tam zajrzy. Flaga wskazująca katalog,
    // którego nie ma, jest tą samą zielenią co plugin rejestrujący zero umiejętności.
    for name in ["alpha", "beta"] {
        let written = named.join("skills").join(name).join("SKILL.md");
        assert!(
            fs::symlink_metadata(&written).is_ok(),
            "the flag points at {}, and {} is not there",
            named.display(),
            written.display()
        );
    }

    // (b) i (c) — nic nie odziedziczono, w dwóch kształtach, w jakich to naprawdę wychodzi.
    let empty_selection = rewrite::plugin_dir(&world.host, &[], &world.run_none_selected)
        .expect("choosing no skills is a normal answer, not a failure");
    let bare_host = rewrite::plugin_dir(
        &world.without_skills,
        &["alpha".to_owned()],
        &world.run_bare_host,
    )
    .expect("a host repository with no .claude/skills is the normal case (invariant 5)");

    for (rewritten, dir, what) in [
        (
            &empty_selection,
            &world.run_none_selected,
            "no skill was chosen",
        ),
        (&bare_host, &world.run_bare_host, "the host has no skills"),
    ] {
        let argv = rewrite::plugin_argv(rewritten);
        no_flag_without_a_value(&argv);
        assert!(
            argv.is_empty(),
            "{what}, and the fragment is still {argv:?}. A plugin directory with no skills in it \
             loads and registers nothing, with a green-looking entry in the startup event"
        );
        // (c) Bez tego „pusty katalog nie trafia do argv" jest spełnialne przez kompozytor,
        // który katalog i tak stworzył, tylko go nie wymienił — a katalog, który powstał, prędzej
        // czy później zostanie komuś podany.
        assert!(
            fs::symlink_metadata(dir).is_err(),
            "{what}, and {} was created anyway",
            dir.display()
        );
    }
}
