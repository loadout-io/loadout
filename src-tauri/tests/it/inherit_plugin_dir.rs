//! AC-2 dla T-54: katalog pluginu ma dokładnie kształt, który vendor rozumie, i dokładnie tę
//! treść, która przyszła — ani jednej ścieżki więcej.
//!
//! **Słabą wersją tego kryterium jest
//! `assert!(dir.join("skills").join(name).join("SKILL.md").exists())`.** Przechodzi dla
//! implementacji, która obok wybranych plików wsypała do katalogu **cały** `.claude/`
//! gospodarza — razem z `format.sh`, `settings.json` i trzecią, niewybraną umiejętnością — bo
//! pytanie „czy jest" nigdy nie pyta „czy tylko". To jest dokładnie ta droga, którą maszyneria
//! gospodarza wchodzi do naszego biegu.
//!
//! Rozróżnia to **równość zbioru** wszystkich ścieżek pod katalogiem pluginu, nie zawieranie.
//!
//! Drugi wariant słabości: `assert_eq!(fs::read_to_string(..).trim(), oczekiwane.trim())`.
//! Przechodzi dla implementacji, która przepuściła plik przez `skills::place::emit`, bo `emit`
//! zwraca poprawny `SKILL.md`, tylko **inny** — przestawione pola, zdjęte `argument-hint`,
//! przecytowane skalary. Rozróżnia to porównanie surowych bajtów: `Vec<u8>`, bez `String`, bez
//! `trim`. Człowiek ma móc porównać `diff` i zobaczyć zero różnic, bo każda nasza „poprawka"
//! w cudzym pliku jest zmianą treści promptu, o której autor umiejętności się nie dowie.
//!
//! JEDEN `#[test]`: zaślepka, która nic nie zapisuje, przechodzi punkt (f) — „katalog
//! gospodarza jest po operacji taki jak przed". Rozbity na osobne zestawy dałby w warstwie
//! `before` obraz „w połowie zielony". Przypadek pozytywny stoi więc pierwszy.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
// `PermissionsExt` to jedyny sposób zapytać o bit wykonywalności. Wolno go tu użyć:
// niezmiennik 3 dotyczy kodu wysyłanego, a `checks/quick-boundary.sh` wyłącza pliki testowe
// po ścieżce.
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use loadout_lib::inherit::rewrite;

const ALPHA_MD: &str = "---\nname: alpha\ndescription: Reads a log and says what broke.\nargument-hint: <path>\n---\n\nStart from the first stack trace.\n";
const BETA_MD: &str = "---\nname: beta\ndescription: Turns a failing gate into one sentence.\n---\n\nQuote the first failing assertion, not the summary line.\n";
const GAMMA_MD: &str = "---\nname: gamma\ndescription: The one nobody picked.\n---\n\nThis file must not leave the host repository.\n";

/// Plik dołączony **wewnątrz** wybranej umiejętności. Do katalogu pluginu jedzie sam
/// `SKILL.md`, więc ten plik ma zostać u gospodarza — to jest nazwany koszt, nie przeoczenie.
const ANTI_PATTERNS_MD: &str = "# Anti-patterns\n\nOne per line.\n";

/// Hak gospodarza, w kształcie, który tam naprawdę jest: `0755` i dziesięciu takich w katalogu.
/// Skrypt jest maszynerią z definicji i to jest cała treść tego zadania.
const FORMAT_SH: &str = "#!/bin/sh\nexec cargo fmt\n";
const HOOK_MODE: u32 = 0o755;

/// Cudze repozytorium i nasz katalog biegu — dwa rozłączne drzewa w jednym katalogu tymczasowym.
struct World {
    /// Trzyma katalog tymczasowy przy życiu na czas testu; kasuje go `Drop`.
    _tmp: tempfile::TempDir,
    /// Korzeń repozytorium gospodarza.
    host: PathBuf,
    /// `<host>/.claude` — całe cudze drzewo, którego po operacji ma nie ubyć ani nie przybyć.
    host_claude: PathBuf,
    /// Katalog pluginu biegu, w kształcie, który zbuduje ścieżka biegu. **Nie istnieje** przed
    /// wywołaniem: jego rodzice też mają powstać.
    plugin: PathBuf,
}

fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let host = tmp.path().join("host-repo");
    let host_claude = host.join(".claude");
    let skills = host_claude.join("skills");

    fs::create_dir_all(skills.join("alpha").join("references")).unwrap();
    fs::create_dir_all(skills.join("beta")).unwrap();
    fs::create_dir_all(skills.join("gamma")).unwrap();
    fs::create_dir_all(host_claude.join("hooks")).unwrap();

    fs::write(skills.join("alpha").join("SKILL.md"), ALPHA_MD).unwrap();
    fs::write(
        skills
            .join("alpha")
            .join("references")
            .join("anti-patterns.md"),
        ANTI_PATTERNS_MD,
    )
    .unwrap();
    fs::write(skills.join("beta").join("SKILL.md"), BETA_MD).unwrap();
    fs::write(skills.join("gamma").join("SKILL.md"), GAMMA_MD).unwrap();

    let hook = host_claude.join("hooks").join("format.sh");
    fs::write(&hook, FORMAT_SH).unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(HOOK_MODE)).unwrap();

    // Katalog biegu Loadouta, w kształcie, który zbuduje ścieżka biegu — i rozłączny z drzewem
    // gospodarza, żeby punkt (f) miał co porównywać.
    let plugin = tmp
        .path()
        .join("loadout-project")
        .join(".loadout")
        .join("runs")
        .join("20260819T101500__r7")
        .join("plugin");

    World {
        _tmp: tmp,
        host,
        host_claude,
        plugin,
    }
}

/// Całe drzewo pod `root`, ścieżkami względnymi: `None` dla katalogu, `Some(bajty)` dla pliku.
///
/// `symlink_metadata`, nie `metadata`: dowiązanie ma zostać dowiązaniem, a nie zniknąć w treści
/// pliku, na który wskazuje.
fn tree(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut found = BTreeMap::new();
    walk(root, Path::new(""), &mut found);
    found
}

fn walk(dir: &Path, prefix: &Path, found: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
    let Ok(listing) = fs::read_dir(dir) else {
        return;
    };
    for entry in listing {
        let entry = entry.unwrap();
        let path = entry.path();
        let relative = prefix.join(entry.file_name());
        if fs::symlink_metadata(&path).unwrap().is_dir() {
            found.insert(relative.clone(), None);
            walk(&path, &relative, found);
        } else {
            found.insert(relative, Some(fs::read(&path).unwrap()));
        }
    }
}

/// Same pliki spod `root`, ścieżkami względnymi.
fn files(root: &Path) -> BTreeSet<PathBuf> {
    tree(root)
        .into_iter()
        .filter_map(|(path, bytes)| bytes.map(|_| path))
        .collect()
}

#[test]
fn the_plugin_directory_holds_the_two_chosen_skills_byte_for_byte_and_nothing_else() {
    let world = world();
    let before = tree(&world.host_claude);
    let selected = vec!["alpha".to_owned(), "beta".to_owned()];

    let rewritten = rewrite::plugin_dir(&world.host, &selected, &world.plugin)
        .expect("rewriting two skills of a three-skill host into a fresh run directory");

    // Wynik przepisania opisuje to, co naprawdę powstało: katalog jest tym, który podano
    // argumentem (a nie podkatalogiem, o którym wiedziałby tylko `rewrite.rs`), a lista nazw
    // jest listą wybranych. Na tej ścieżce stanie za chwilę flaga `--plugin-dir`.
    assert_eq!(
        rewritten.dir, world.plugin,
        "the rewrite reports a different plugin directory than the one it was given"
    );
    assert_eq!(
        rewritten.names, selected,
        "the rewrite does not report the two skills it was asked to carry over"
    );

    // (a) `skills/<nazwa>/SKILL.md` dla OBU wybranych. `symlink_metadata`, nie `exists()`:
    // `exists()` podąża za dowiązaniem, a dowiązanie do cudzego repozytorium znaczy, że treść
    // promptu zmienia się, kiedy gospodarz zmieni plik w trakcie biegu.
    //
    // Poziom `skills/` jest OBOWIĄZKOWY i to jest zmierzone: `<katalog>/alpha/SKILL.md` daje
    // plugin, który się ładuje, pojawia się w `init.plugins` jako pełnoprawny wpis i rejestruje
    // ZERO umiejętności [S1 §2, przebieg M3: 54 → 54]; `skills/alpha/SKILL.md` rejestruje obie
    // [M3a: 54 → 56]. Nie ma błędu, nie ma ostrzeżenia, jest zielony wpis w zdarzeniu startowym.
    for (name, source) in [("alpha", ALPHA_MD), ("beta", BETA_MD)] {
        let written = world.plugin.join("skills").join(name).join("SKILL.md");
        assert!(
            fs::symlink_metadata(&written).is_ok(),
            "nothing at {} — a plugin directory that omits the skills/ level loads and \
             registers no skills at all, with a green-looking entry in the startup event",
            written.display()
        );

        // (b) BAJTY, nie `String` po `trim`. Emiter umiejętności, którą Loadout posiada,
        // przestawia pola, zdejmuje `argument-hint` i przecytowuje skalary — zwraca poprawny
        // SKILL.md, tylko inny. Umiejętność, którą Loadout CYTUJE, ma przyjechać bajt w bajt,
        // żeby człowiek mógł porównać `diff` i zobaczyć zero różnic.
        assert_eq!(
            fs::read(&written).unwrap(),
            source.as_bytes().to_vec(),
            "{} is not the bytes that lie in the host repository",
            written.display()
        );
    }

    // (d) RÓWNOŚĆ zbioru wszystkich ścieżek, nie zawieranie. „Czy jest" nigdy nie pyta „czy
    // tylko", a katalog pluginu ma dokładnie jednego czytelnika i dokładnie dwie powierzchnie
    // (niezmiennik 21).
    let expected: BTreeSet<PathBuf> = [
        PathBuf::from(".claude-plugin").join("plugin.json"),
        PathBuf::from("skills").join("alpha").join("SKILL.md"),
        PathBuf::from("skills").join("beta").join("SKILL.md"),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        files(&world.plugin),
        expected,
        "the plugin directory does not hold exactly the manifest and the two chosen SKILL.md \
         files. Every extra path here is a piece of someone else's harness inside our run"
    );

    // (e) Trzy rzeczy wymienione z nazwy, bo komunikat porażki ma powiedzieć, KTÓRA weszła.
    // Skrypt jest maszynerią z definicji; trzecia umiejętność nie została wybrana; dołączone
    // pliki są nazwanym kosztem („Świadomie poza zakresem"), nie przeoczeniem.
    let everything = tree(&world.plugin);
    for forbidden in ["format.sh", "gamma", "references", "anti-patterns.md"] {
        assert!(
            everything
                .keys()
                .all(|path| path.iter().all(|part| part != forbidden)),
            "`{forbidden}` reached the plugin directory"
        );
    }

    // (c) `plugin.json` NIE jest warunkiem działania na CLI 2.1.233 [S1 §3] i piszemy go mimo
    // to, z konkretnego powodu: umiejętności wracają w `system/init` z przedrostkiem od nazwy
    // katalogu (`s1-plugin-a:alpha`), a nasz katalog nazywa się od biegu — bez przypiętej nazwy
    // przedrostek zmieniałby się co bieg i żaden ekran nie mógłby go pokazać stabilnie.
    let manifest = world.plugin.join(".claude-plugin").join("plugin.json");
    let parsed: serde_json::Value = serde_json::from_slice(&fs::read(&manifest).unwrap())
        .expect("the manifest is not readable JSON");
    assert!(
        parsed
            .get("name")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|name| !name.is_empty()),
        "the manifest carries no name, so the prefix every skill comes back under is the run \
         directory's basename and no screen can show it twice the same way"
    );

    // (f) Cudze repozytorium jest dla nas TYLKO DO ODCZYTU. Przepisanie czyta źródło i niczego
    // w nim nie dotyka — łącznie z bitem wykonywalności haka, którego nie kopiujemy i nie
    // zdejmujemy.
    assert_eq!(
        tree(&world.host_claude),
        before,
        "the host repository changed while we were reading it"
    );
    assert_eq!(
        fs::metadata(world.host_claude.join("hooks").join("format.sh"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        HOOK_MODE,
        "the host's hook lost its permissions"
    );
}
