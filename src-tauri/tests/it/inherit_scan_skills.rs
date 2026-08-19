//! AC-1 dla T-54: skan umiejętności gospodarza cytuje pierwszy wiersz **dosłownie**, a katalog
//! bez `SKILL.md` jest normalnym stanem cudzego repozytorium, nie awarią.
//!
//! **Słabą wersją tego kryterium jest `assert!(!entries.is_empty())`** albo
//! `assert!(entries.iter().any(|e| e.name == "log-sweep"))`. Oba przechodzą dla skanu, który
//! wypisuje **wszystkie** katalogi i wkłada pusty napis w miejsce pierwszej linii — czyli dla
//! implementacji, w której „pomiń katalog bez `SKILL.md`" nie istnieje, a człowiek dostaje na
//! ekranie umiejętność, której nie ma.
//!
//! Rozróżnia dopiero porównanie **całego wektora** razem z pierwszymi liniami plus twarde
//! `entries.len() == 2`: długość listy jest jedynym miejscem, w którym „pominięty" różni się od
//! „wypisany z pustą treścią". Drugą stronę domyka wymaganie, żeby obie pierwsze linie były
//! **różne** i żeby jedna z nich brzmiała `---`: wektor, w którym oba wpisy mają to samo,
//! przechodziłby też dla skanu wpisującego w to pole **nazwę katalogu** zamiast czytać plik.
//!
//! JEDEN `#[test]`, i to jest wybór, nie lenistwo. Zaślepka zwracająca pustą listę przechodzi
//! punkty (c), (d) i (f) tego kryterium — rozbite na osobne zestawy dałyby w warstwie `before`
//! obraz „w połowie zielony", czyli dokładnie ten fałszywy sygnał, przed którym stoi ta
//! warstwa. Przypadek pozytywny stoi więc pierwszy i cały zestaw pada na nim.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

use loadout_lib::inherit::HostSkill;
use loadout_lib::inherit::scan;

/// Umiejętność z front-matterem, czyli kształt, który `SKILL.md` ma naprawdę. Pierwszy wiersz
/// to `---` i o to w punkcie (e) chodzi.
const ALPHA_MD: &str = "---\nname: alpha\ndescription: Reads a log and says what broke.\n---\n\nStart from the first stack trace, not from the last line.\n";

/// Druga umiejętność, celowo **bez** front-mattera: jej pierwszy wiersz jest inny niż `---`,
/// więc para wpisów rozróżnia skan, który czyta plik, od skanu, który wpisuje w to pole nazwę
/// katalogu.
const SHIP_MD: &str = "# Ship a task\n\nOne branch, one gate, one round of repair.\n";

/// Zwykły plik obok katalogów. U gospodarza `.claude/skills/` ma takie sąsiedztwo i skan ma je
/// pominąć bez jednego słowa — plik nie jest umiejętnością i nie jest też błędem.
const README_MD: &str = "# Skills\n\nOne folder per skill.\n";

/// Katalog **bez** `SKILL.md`. U gospodarza taki zostaje po ręcznym usunięciu pliku i po
/// nieudanym `git checkout`: jest tam, wygląda jak umiejętność i nie znaczy awarii.
const BARE: &str = "log-sweep";

/// Cudzy projekt i drugi, który po prostu nie ma umiejętności.
struct Host {
    /// Trzyma katalog tymczasowy przy życiu na czas testu; kasuje go `Drop`.
    _tmp: tempfile::TempDir,
    /// Korzeń repozytorium gospodarza — argument skanu.
    project: PathBuf,
    /// `<projekt>/.claude/skills/log-sweep`, czyli katalog bez `SKILL.md`.
    bare: PathBuf,
    /// Repozytorium **bez** katalogu `.claude/skills`. To jest większość repozytoriów.
    without_skills: PathBuf,
}

fn host() -> Host {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("host-repo");
    let skills = project.join(".claude").join("skills");
    let bare = skills.join(BARE);
    let without_skills = tmp.path().join("plain-repo");

    fs::create_dir_all(skills.join("alpha")).unwrap();
    fs::create_dir_all(skills.join("ship-task")).unwrap();
    fs::create_dir_all(&bare).unwrap();
    fs::write(skills.join("alpha").join("SKILL.md"), ALPHA_MD).unwrap();
    fs::write(skills.join("ship-task").join("SKILL.md"), SHIP_MD).unwrap();
    fs::write(skills.join("README.md"), README_MD).unwrap();
    fs::create_dir_all(&without_skills).unwrap();

    Host {
        _tmp: tmp,
        project,
        bare,
        without_skills,
    }
}

/// Cała lista, jakiej oczekujemy — **posortowana po nazwie**, bo kolejność z systemu plików nie
/// jest ustalona, a tę listę czyta człowiek na ekranie wyboru.
fn expected() -> Vec<HostSkill> {
    vec![
        HostSkill {
            name: "alpha".to_owned(),
            first_line: "---".to_owned(),
        },
        HostSkill {
            name: "ship-task".to_owned(),
            first_line: "# Ship a task".to_owned(),
        },
    ]
}

#[test]
fn scan_quotes_the_first_line_and_a_folder_without_a_skill_file_is_not_a_failure() {
    let host = host();

    let entries = scan::skills(&host.project).expect(
        "a host repository with three skill folders, one of them missing SKILL.md, is an \
         ordinary readable shape. Turning it into Err would turn `this host has no skills` into \
         a refusal to start the run (invariant 5)",
    );

    // (a) CAŁA lista, para po parze. `contains` na tej liście nigdy nie odpowiada na pytanie,
    // które ma znaczenie: czy skan wypisał coś, czego u gospodarza nie ma.
    assert_eq!(
        entries,
        expected(),
        "the scan did not return exactly the two skills that have a SKILL.md, in name order, \
         each quoting the first line of its own file"
    );

    // (b) Długość jest jedynym miejscem, w którym „pominięty" różni się od „wypisany z pustą
    // treścią": trzy katalogi na dysku, dwa wpisy na liście.
    assert_eq!(
        entries.len(),
        2,
        "three folders are on disk and only two of them hold a SKILL.md, so a third entry means \
         the scan lists folders instead of skills — and the person is shown a skill that does \
         not exist"
    );

    // (e) Obie pierwsze linie są RÓŻNE i jedna z nich to `---`. Wektor, w którym oba wpisy mają
    // to samo, przechodziłby dla skanu wpisującego w to pole nazwę katalogu zamiast czytać plik.
    assert_ne!(
        entries[0].first_line, entries[1].first_line,
        "both entries quote the same first line, so this field does not come from the files"
    );
    assert!(
        entries.iter().any(|entry| entry.first_line == "---"),
        "no entry quotes `---`, and a SKILL.md with front matter starts with exactly that. The \
         quoted line is what a person reads to recognise whose file this is, so an invented \
         sentence would be shown as if it stood in someone else's file"
    );

    // (c) Katalog bez `SKILL.md` nie ma wpisu.
    assert!(
        !entries.iter().any(|entry| entry.name == BARE),
        "{BARE} has no SKILL.md and still got an entry"
    );

    // (d) …i dalej istnieje. Skan czyta gospodarza i nie sprząta po nim: cudze repozytorium jest
    // dla nas tylko do odczytu.
    assert!(
        fs::symlink_metadata(&host.bare).is_ok(),
        "{} is gone after the scan. Reading someone else's repository must not change it",
        host.bare.display()
    );

    // (f) Repozytorium bez `.claude/skills` — pusta lista i `Ok`, nigdy `Err`. To jest
    // większość repozytoriów, nie awaria.
    let none = scan::skills(&host.without_skills).expect(
        "a repository with no .claude/skills at all is the normal case, not an error: `?` on a \
         missing directory turns `this host has no skills` into a refused run (invariant 5)",
    );
    assert!(
        none.is_empty(),
        "a repository with no .claude/skills returned {} entries",
        none.len()
    );
}
