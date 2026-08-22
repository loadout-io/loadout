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
//! Bitu wykonywalności nie wykrywamy — wykrywamy go **nie wykrywając**: ten plik zapisuje
//! wyłącznie to, co sam postanowił zapisać, więc żaden `PermissionsExt` ani `#[cfg(unix)]` nie
//! jest tu potrzebny (niezmienniki 3 i 4).
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

use super::scan;
use super::{Result, Rewritten};
use crate::skills::StepSkills;
use crate::skills::place::copy_the_skill;

/// Poziom, bez którego plugin ładuje się i rejestruje ZERO umiejętności.
///
/// Zmierzone [S1 §2]: `<katalog>/alpha/SKILL.md` → przebieg M3, 54 → 54, plugin widoczny
/// w `init.plugins` jako pełnoprawny wpis; `<katalog>/skills/alpha/SKILL.md` → M3a, 54 → 56.
/// Nie ma błędu, nie ma ostrzeżenia, jest zielony wpis w zdarzeniu startowym.
const SKILLS_LEVEL: &str = "skills";

/// Nazwa pliku umiejętności po NASZEJ stronie granicy.
///
/// Ta sama, co u gospodarza, ale nie z tego powodu: to konwencja vendora, którego katalog
/// budujemy. Gdyby gospodarz nazywał swój plik inaczej, ten tutaj nadal musiałby nazywać się
/// tak, bo to `claude --plugin-dir` go szuka.
const SKILL_FILE: &str = "SKILL.md";

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
const INHERITED_PLUGIN: &str = "loadout-inherited";

/// Nazwa pluginu z materiałem BIBLIOTEKI Loadouta.
///
/// Inna niż [`INHERITED_PLUGIN`], bo przedrostek w `system/init` (`<plugin>:<nazwa>` [S1 §2])
/// jest jedyną rzeczą, po której człowiek pozna, skąd wzięła się umiejętność, którą sesja
/// właśnie ogłosiła. Jedna nazwa na oba źródła zlepiłaby „to twoje" i „to z tego repozytorium"
/// w jeden napis — a to są dwa różne pytania o zaufanie.
const LIBRARY_PLUGIN: &str = "loadout-skills";

/// Przepisuje wybrane umiejętności gospodarza do katalogu pluginu biegu.
///
/// `project` to korzeń **cudzego** repozytorium (czytamy `<projekt>/.claude/skills/<nazwa>/`),
/// `selected` to nazwy katalogów z [`super::scan::skills`], a `into` to katalog pluginu biegu
/// (`<projekt>/.loadout/runs/<ts>__<id>/plugin/`) — **podany argumentem**, bo znaczek czasu
/// i identyfikator biegu należą do biegu, a nie do dziedziczenia.
///
/// Powstaje `.claude-plugin/plugin.json` oraz `skills/<nazwa>/SKILL.md` na każdą wybraną
/// umiejętność, bajt w bajt taki, jaki leży u gospodarza — i **ani jednej ścieżki więcej**.
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
    // CZYTAMY WSZYSTKO, ZANIM ZAPISZEMY COKOLWIEK. Obietnica „nie odziedziczono niczego →
    // katalog nie powstał" jest sprawdzalna tylko wtedy, gdy wiemy, że nie ma czego włożyć,
    // PRZED pierwszym `create_dir_all`. Pusty katalog przekazany vendorowi to plugin, który
    // ładuje się i rejestruje zero umiejętności — ta sama cicha zieleń co poziom bez `skills/`.
    let mut carried: Vec<(&String, Vec<u8>)> = Vec::new();
    for name in selected {
        // `None` znaczy „to nie jest nazwa jednego katalogu" i wyklucza wpis z obu ścieżek
        // naraz: czytanej u gospodarza i pisanej u nas. Niżej `join(name)` opiera się na tym,
        // że do `carried` wchodzą wyłącznie nazwy, które przez ten warunek przeszły.
        let Some(source) = scan::skill_file(project, name) else {
            continue;
        };
        match fs::read(&source) {
            Ok(bytes) => carried.push((name, bytes)),
            // Wybrana umiejętność, której u gospodarza nie ma, jest normalnym stanem cudzego
            // repozytorium (niezmiennik 5) — człowiek mógł ją odznaczyć przed chwilą w innym
            // narzędziu. Widać to w `names`: to lista tego, co NAPRAWDĘ pojechało, a nie tego,
            // o co poproszono, i różnica między nią a `selected` jest jedynym miejscem, w którym
            // ta strata jest widoczna.
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            // Awaria dysku to co innego: o niej człowiek ma się dowiedzieć.
            Err(error) => return Err(error.into()),
        }
    }

    let rewritten = Rewritten {
        dir: into.to_path_buf(),
        names: carried.iter().map(|(name, _)| (*name).clone()).collect(),
    };
    if carried.is_empty() {
        return Ok(rewritten);
    }

    // BAJT W BAJT. Nie przez `place::emit`, bo emiter normalizuje — zdejmuje pola spoza
    // specyfikacji, przestawia kolejność, przecytowuje skalary — i zwraca poprawny `SKILL.md`,
    // tylko INNY. Człowiek ma móc porównać `diff` z cudzym plikiem i zobaczyć zero różnic.
    //
    // I nie przez `fs::copy`, mimo że to jedna linia mniej: `fs::copy` przenosi uprawnienia
    // razem z treścią, więc cudzy `SKILL.md` z bitem wykonywalności wylądowałby u nas
    // wykonywalny. `read` + `write` zapisuje wyłącznie to, co sam postanowił zapisać, i dlatego
    // ten plik nie potrzebuje ani `PermissionsExt`, ani gałęzi platformowej (niezmienniki 3 i 4).
    for (name, bytes) in &carried {
        let dir = into.join(SKILLS_LEVEL).join(name.as_str());
        fs::create_dir_all(&dir)?;
        fs::write(dir.join(SKILL_FILE), bytes)?;
    }

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
/// **CAŁY KATALOG UMIEJĘTNOŚCI, nie sam `SKILL.md`**, i ta jedna różnica jest rozstrzygnięciem,
/// nie niekonsekwencją. Umiejętność gospodarza Loadout **cytuje**: jego `scripts/` to cudza
/// maszyneria, której nie przenosimy (nagłówek tego pliku). Umiejętność z biblioteki Loadout
/// **posiada** — jej pliki dołączone zapisał `place::apply` i są jej częścią, a `SKILL.md`
/// odsyłający do skryptu, którego przy nim nie ma, jest umiejętnością zepsutą po cichu.
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

    for (name, source) in skills.names.iter().zip(&skills.dirs) {
        copy_the_skill(source, &into.join(SKILLS_LEVEL).join(name))?;
    }
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
    let manifest = into.join(MANIFEST_DIR);
    fs::create_dir_all(&manifest)?;
    fs::write(
        manifest.join(MANIFEST_FILE),
        format!("{{\n  \"name\": \"{plugin}\"\n}}\n"),
    )?;
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
