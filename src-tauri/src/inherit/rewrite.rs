//! Pisanie do siebie. Jedyne miejsce w tym zadaniu, które dotyka dysku zapisem.
//!
//! „Przepisanie" jest tu czasownikiem dosłownym: czytamy cudze pliki i przenosimy ich bajty do
//! katalogu, który sami stworzyliśmy. **Nie** idziemy przez `skills::place` i to jest
//! rozstrzygnięcie, nie przeoczenie: `place::emit` normalizuje (zdejmuje czternaście pól spoza
//! specyfikacji, przepisuje cytowanie skalarów YAML-a, ustawia kolejność pól), a `place::apply`
//! pisze do dwóch katalogów vendorów użytkownika i do sidecara. Obie te rzeczy są poprawne dla
//! umiejętności, którą Loadout **posiada**, i obie są złe dla umiejętności, którą Loadout
//! **cytuje**: człowiek ma móc porównać `diff` i zobaczyć zero różnic, a każda nasza „poprawka"
//! w cudzym pliku jest zmianą treści promptu, o której autor umiejętności się nie dowie.
//!
//! Katalog pluginu jest **wyjściem builda** i musi dać się skasować bez straty (niezmiennik 4)
//! — źródłem jest repo gospodarza, do którego ten plik nigdy nie pisze. Ma też dokładnie
//! jednego czytelnika, `claude --plugin-dir`, i dokładnie dwie powierzchnie z AC-2
//! (niezmiennik 21): `commands/`, `hooks/`, `agents/` ani `mcp.json` tu nie powstają, bo S-1
//! nie zmierzył żadnej z nich [S1 §3].
//!
//! WF-13 (2026-09-05): kompletność katalogu i prawa helperów rozstrzyga wspólny bundle,
//! a operacje zależne od platformy pozostają w supervisorze. Żaden helper nie jest odpalany.
//!
//! 2026-08-22 (T-79) — DRUGI KORZEŃ ŹRÓDŁOWY, TA SAMA DROGA. Katalog pluginu jest jedynym
//! kanałem, którym Claude Code przyjmuje umiejętność podaną z zewnątrz [S1 §3], a Loadout ma
//! dwa źródła takich umiejętności: cudze repozytorium ([`plugin_dir`]) i własną bibliotekę
//! ([`plugin_dir_from_the_library`]). Druga funkcja mieszka tutaj, a nie w `skills/place.rs`,
//! bo obowiązkowy poziom `skills/`, manifest przypinający przedrostek i reguła „pusty wybór nie
//! tworzy katalogu" są **jedną** wiedzą o tym vendorze — a druga jej kopia byłaby pierwszą
//! rzeczą, która zostanie stara (niezmiennik 23).

use std::fs;
use std::io;
use std::path::Path;

use super::{Result, Rewritten};
use crate::skills::StepSkills;

/// Poziom, bez którego plugin ładuje się i rejestruje ZERO umiejętności.
///
/// Zmierzone [S1 §2]: `<katalog>/alpha/SKILL.md` → przebieg M3, 54 → 54, plugin widoczny
/// w `init.plugins` jako pełnoprawny wpis; `<katalog>/skills/alpha/SKILL.md` → M3a, 54 → 56.
/// Nie ma błędu, nie ma ostrzeżenia, jest zielony wpis w zdarzeniu startowym.
const SKILLS_LEVEL: &str = "skills";

/// Manifest pluginu: katalog, plik i cała jego treść.
///
/// `plugin.json` **nie jest** warunkiem działania na CLI 2.1.233 — `/tmp/s1-plugin-a` nie miał
/// żadnego manifestu i obie umiejętności się zarejestrowały [S1 §3]. Piszemy go z jednego,
/// konkretnego powodu: umiejętności wracają w `system/init` z przedrostkiem od nazwy katalogu
/// (`s1-plugin-a:alpha`), a nasz katalog nazywa się od biegu — bez przypiętej nazwy przedrostek
/// zmieniałby się co bieg i żaden ekran nie mógłby go pokazać dwa razy tak samo.
///
/// Jedno pole, bo dokładnie jedno ma czytelnika (niezmiennik 21). Treść składamy `format!`, a nie
/// `serde_json`: nazwa jest tu jedyną wartością, jest stałą tego pliku i nie ma w niej znaku,
/// który trzeba by cytować — serializator dołożyłby wyłącznie ścieżkę błędu, której nie da się
/// wywołać.
const MANIFEST_DIR: &str = ".claude-plugin";
const MANIFEST_FILE: &str = "plugin.json";

/// Nazwa pluginu z materiałem CUDZEGO repozytorium.
///
/// `pub(crate)` od 2026-09 (Z-47): historia biegu poznaje po tej nazwie, co krokowi dał sam bieg,
/// a co dobrał sobie z folderu. Kopia tego napisu po tamtej stronie rozjechałaby się przy
/// pierwszej zmianie nazwy — i wtedy krok zacząłby liczyć własność biegu jako cudzą, milcząc
/// dokładnie o tym, o czym ma mówić (niezmiennik 13).
pub(crate) const INHERITED_PLUGIN: &str = "loadout-inherited";

/// Nazwa pluginu z materiałem BIBLIOTEKI Loadouta.
///
/// Inna niż [`INHERITED_PLUGIN`], bo przedrostek w `system/init` (`<plugin>:<nazwa>` [S1 §2])
/// jest jedyną rzeczą, po której człowiek pozna, skąd wzięła się umiejętność, którą sesja
/// właśnie ogłosiła. Jedna nazwa na oba źródła zlepiłaby „to twoje" i „to z tego repozytorium"
/// w jeden napis — a to są dwa różne pytania o zaufanie.
///
/// `pub(crate)` z tego samego powodu, co przy [`INHERITED_PLUGIN`].
pub(crate) const LIBRARY_PLUGIN: &str = "loadout-skills";

/// Przepisuje wybrane umiejętności gospodarza do katalogu pluginu biegu.
///
/// `project` to korzeń **cudzego** repozytorium (czytamy `<projekt>/.claude/skills/<nazwa>/`),
/// `selected` to nazwy katalogów z [`super::scan::skills`], a `into` to katalog pluginu biegu
/// (`<projekt>/.loadout/runs/<ts>__<id>/plugin/`) — **podany argumentem**, bo znaczek czasu
/// i identyfikator biegu należą do biegu, a nie do dziedziczenia.
///
/// Powstaje `.claude-plugin/plugin.json` oraz `skills/<nazwa>/SKILL.md` na każdą wybraną
/// umiejętność wraz ze wszystkimi zasobami względnymi, bajt w bajt jak u gospodarza.
///
/// POZIOM `skills/` JEST OBOWIĄZKOWY i to jest zmierzone: `<katalog>/alpha/SKILL.md` daje
/// plugin, który się ładuje, pojawia się w `init.plugins` jako pełnoprawny wpis i rejestruje
/// **zero** umiejętności [S1 §2, przebieg M3: 54 → 54]; `skills/alpha/SKILL.md` rejestruje obie
/// [M3a: 54 → 56]. Nie ma błędu, nie ma ostrzeżenia, jest zielony wpis w zdarzeniu startowym.
///
/// `plugin.json` **nie jest** warunkiem działania na CLI 2.1.233 [S1 §3] i piszemy go mimo to,
/// z konkretnego powodu: umiejętności wracają w `system/init` z przedrostkiem od nazwy katalogu
/// (`s1-plugin-a:alpha`), a nasz katalog nazywa się od biegu — bez przypiętej nazwy przedrostek
/// zmieniałby się co bieg i żaden ekran nie mógłby go pokazać stabilnie.
///
/// Pusta lista wybranych albo host bez `.claude/skills` **nie tworzy katalogu**: pusty katalog
/// przekazany vendorowi to plugin ładujący się z zerem umiejętności, czyli ta sama cicha
/// zieleń, o którą chodzi wyżej.
pub fn plugin_dir(project: &Path, selected: &[String], into: &Path) -> Result<Rewritten> {
    // WF-13: Borrow przenosi ten sam pełny katalog co biblioteka. Kopiowanie helpera
    // nie jest jego wykonaniem. Wszystkie źródła walidujemy przed pierwszym zapisem.
    /// Nazwa spoza JEDNEGO katalogu nie wyznacza ani ścieżki czytanej u gospodarza, ani
    /// pisanej u nas — i tego WF-13 słusznie odmawia.
    const NOT_A_SINGLE_FOLDER: &str = "The selected skill name is not valid.";

    let mut carried = Vec::new();
    for name in selected {
        let file = super::scan::skill_file(project, name)
            .ok_or_else(|| io::Error::other(NOT_A_SINGLE_FOLDER))?;
        /* WYBRANA UMIEJĘTNOŚĆ, KTÓREJ U GOSPODARZA NIE MA, TO NORMALNY STAN CUDZEGO
         * REPOZYTORIUM (niezmiennik 5), a nie awaria: człowiek mógł ją przed chwilą odznaczyć
         * w innym narzędziu. Wypada z `carried`, więc i z `Rewritten::names` — a ODMOWĘ
         * wystawia `wire::every_name_is_really_there`, bo tylko ona wie, co człowiek widział
         * na ekranie wyboru.
         *
         * `scan::skill_file` SKŁADA ścieżkę i nie pyta o istnienie, więc bez tego pytania
         * gospodarz bez `.claude/skills` wywracał cały zapis. Awaria dysku to co innego —
         * o niej człowiek ma się dowiedzieć. */
        match fs::symlink_metadata(&file) {
            Ok(_) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        }
        let source = file
            .parent()
            .ok_or_else(|| io::Error::other(NOT_A_SINGLE_FOLDER))?;
        carried.push(crate::skills::bundle::borrowed_from_source(name, source)?);
    }

    let rewritten = Rewritten {
        dir: into.to_path_buf(),
        names: carried.iter().map(|skill| skill.name.clone()).collect(),
    };
    if carried.is_empty() {
        return Ok(rewritten);
    }

    for skill in &carried {
        skill
            .bundle
            .materialize(&into.join(SKILLS_LEVEL).join(&skill.name))?;
    }

    crate::skills::bundle::write_delivered(into, &carried)?;
    pin_the_name(into, INHERITED_PLUGIN)?;

    Ok(rewritten)
}

/// Przepisuje umiejętności **naszej biblioteki** do katalogu pluginu jednego kroku.
///
/// TA SAMA DROGA, INNY KORZEŃ ŹRÓDŁOWY, i to jest cała różnica wobec [`plugin_dir`]: ten sam
/// obowiązkowy poziom `skills/`, ten sam manifest przypinający przedrostek, ta sama obietnica
/// „pusty wybór nie tworzy katalogu". Zmienia się to, skąd bierzemy bajty — z `<dane>/skills/`
/// zamiast z `.claude/skills/` cudzego repozytorium — i to, ile ich bierzemy.
///
/// WF-13: cały katalog, tą samą drogą co Borrow. Wcześniejszy podział cytowanie/własność
/// gubił helpery wyłącznie na jednej ścieżce, choć oba ekrany obiecywały ten sam skill.
///
/// `into` przychodzi argumentem, tak samo jak w [`plugin_dir`], bo katalog kroku należy do biegu,
/// a nie do rozmieszczania.
pub fn plugin_dir_from_the_library(skills: &StepSkills, into: &Path) -> Result<Rewritten> {
    let rewritten = Rewritten {
        dir: into.to_path_buf(),
        names: skills.names.clone(),
    };
    // Pusty wybór NIE TWORZY KATALOGU — ten sam powód, co w [`plugin_dir`]: pusty katalog podany
    // vendorowi to plugin, który ładuje się i rejestruje zero umiejętności.
    if skills.names.is_empty() {
        return Ok(rewritten);
    }

    let carried = skills
        .names
        .iter()
        .zip(&skills.dirs)
        .map(|(name, source)| crate::skills::bundle::from_source(name, source))
        .collect::<io::Result<Vec<_>>>()?;
    for skill in &carried {
        skill
            .bundle
            .materialize(&into.join(SKILLS_LEVEL).join(&skill.name))?;
    }
    crate::skills::bundle::write_delivered(into, &carried)?;
    pin_the_name(into, LIBRARY_PLUGIN)?;

    Ok(rewritten)
}

/// Przypina nazwę pluginu manifestem — **na końcu**, i to jest wybór kierunku porażki.
///
/// Przerwany zapis zostawia wtedy katalog z umiejętnościami i bez przypiętej nazwy — przedrostek
/// spada do nazwy katalogu biegu, czyli degraduje się do niestabilnego. Manifest zapisany
/// pierwszy zostawiłby przy tej samej porażce katalog z nazwą i z zerem umiejętności, czyli
/// dokładnie ten kształt, który ładuje się na zielono i nic nie wnosi.
fn pin_the_name(into: &Path, plugin: &str) -> Result<()> {
    // WF-13: create_dir_all + write szły przez podmienione .claude-plugin do cudzych plików.
    let root = crate::engine::supervisor::PublicationRoot::open(into)?;
    root.ensure_directory(Path::new(MANIFEST_DIR), 0o700)?;
    crate::durable_file::DurableFilePublisher::new(into)
        .atomic_create_if_absent(
            &into.join(MANIFEST_DIR).join(MANIFEST_FILE),
            format!("{{\n  \"name\": \"{plugin}\"\n}}\n").as_bytes(),
            crate::durable_file::ModePolicy::Exact(crate::durable_file::PRIVATE_FILE_MODE),
        )
        .map_err(super::super::durable_file::PublishError::into_io)?;
    root.validate_path_identity(into)?;
    Ok(())
}

/// Fragment argv, który sterownik dopnie do swojego: `["--plugin-dir", <katalog>]` albo nic.
///
/// KOMPOZYTOR, NIE WIRING. `ClaudeDriver::command` należy do sąsiedniego zadania tej fali
/// (odcięcie ustawień, `--setting-sources ""`, przepisany `permissions.deny`) — dwa zadania
/// piszące do jednego pliku to kolizja, której ta fala unika z premedytacją. Ta funkcja nie zna
/// słowa `ClaudeDriver`.
///
/// Fragment jest **dwuelementowy albo pusty, nigdy jednoelementowy**. `--plugin-dir` bez
/// wartości połknęłoby następną flagę sterownika jako swój argument — i to jest kształt łatwy
/// do pomylenia z `--setting-sources ""` z sąsiedniego zadania, gdzie pusty argument jest
/// poprawny (niezmiennik 20).
#[must_use]
pub fn plugin_argv(rewritten: &Rewritten) -> Vec<String> {
    // `names`, nie `dir`: ścieżka jest znana zawsze, także wtedy, gdy nic po niej nie leży.
    // Pytanie „czy jest co odziedziczyć" ma dokładnie jedną odpowiedź w tym typie i to jest ta.
    if rewritten.names.is_empty() {
        return Vec::new();
    }

    let dir = rewritten.dir.to_string_lossy();
    if dir.is_empty() {
        // Flaga bez wartości połknęłaby następną flagę sterownika jako swój argument. Kształt
        // „pusty argument jest poprawny" istnieje w tym samym argv — `--setting-sources ""`
        // z sąsiedniego zadania — i pomylenie tych dwóch jest realne, więc pusta wartość nie
        // wychodzi stąd nigdy: bez ścieżki nie ma flagi.
        return Vec::new();
    }

    vec!["--plugin-dir".to_owned(), dir.into_owned()]
}
