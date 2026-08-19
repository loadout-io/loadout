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

use std::path::Path;

use super::{Result, Rewritten};

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
    let _ = (project, selected);
    Ok(Rewritten {
        dir: into.to_path_buf(),
        names: Vec::new(),
    })
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
    let _ = rewritten;
    Vec::new()
}
