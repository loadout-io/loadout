//! Umiejętności: jeden kanoniczny folder, dwa katalogi, sześciu vendorów.
//!
//! Kompilatora tu nie ma i nie będzie. Format Agent Skills skonwergował — wszystkie sześć
//! narzędzi z MVP czyta ten sam `SKILL.md` z tym samym front-matterem, a różni je **wyłącznie
//! katalog, do którego zaglądają** [T5 §0]. Cały „backend przenośności" to więc dwie nazwy
//! katalogów, i dlatego stoją tu raz, jako [`DESTINATION_DIRS`], zamiast być wpisane w trzy
//! funkcje `place.rs` (niezmiennik 23). §10 T5 nazywa „vendor po cichu przenosi ścieżkę"
//! ryzykiem numer jeden tego projektu: przy jednej tablicy to jest zmiana jednej linii,
//! przy trzech kopiach to jest zmiana dwóch linii i trzecia, która zostaje stara.
//!
//! Ten plik trzyma **dane**: typy i tablice. Zachowanie — walidacja, emiter, plan, kopiowanie,
//! usuwanie i werdykt „czy vendor to widzi" — mieszka w [`place`].

use std::collections::BTreeMap;
use std::path::PathBuf;

pub mod place;

/// Dwie nazwy katalogów, które pokrywają wszystkich sześciu vendorów z MVP [T5 §3.1].
///
/// Pięciu z sześciu czyta `.agents/skills/`; szósty (Claude Code) czyta `.claude/skills/`
/// i **nie** czyta `.agents/` — to jest zweryfikowane kontrolą negatywną, nie domysłem
/// [T5 §9 fakt 4]. Dlatego dwa wpisy, nie jeden i nie sześć.
///
/// Kolejność jest kolejnością zapisu i kolejnością w [`place::destinations`]. Nie ma znaczenia
/// dla vendorów; ma znaczenie dla `git diff` planu instalacji, który człowiek czyta przed
/// naciśnięciem przycisku.
pub const DESTINATION_DIRS: [&str; 2] = [".claude/skills", ".agents/skills"];

/// Vendorzy, których te dwa katalogi obsługują. Lista jest tu po to, żeby UI („Installed for
/// 6 tools") liczyło z tego samego miejsca, z którego bierze się zapis — a nie z osobnej stałej,
/// która przeżyje usunięcie vendora z [`DESTINATION_DIRS`].
///
/// Świadomie **nie ma** tu Windsurfa, Cline'a, Factory ani Copilota: ich ścieżki mają wyłącznie
/// źródła zewnętrzne, nie oficjalną dokumentację [T5 §3.2]. Ścieżka niezweryfikowana daje
/// dokładnie tę awarię, przed którą stoi to zadanie — plik leży, vendor go nie widzi.
pub const VENDORS: [&str; 6] = [
    "Claude Code",
    "Codex",
    "Cursor",
    "Gemini CLI",
    "opencode",
    "Amp",
];

/// Sześć pól specyfikacji, w kolejności emisji [T5 §2.3].
///
/// Kolejność jest stabilna z jednego powodu: `SKILL.md` w zakresie projektu ląduje w repo
/// zespołu, a nagłówek przestawiający pola przy każdym „Update" zamienia `git diff` w szum,
/// w którym nikt nie zauważy zmiany `description`.
pub const SPEC_FIELDS: [&str; 6] = [
    "name",
    "description",
    "license",
    "compatibility",
    "metadata",
    "allowed-tools",
];

/// Czternaście pól, które Claude Code przyjmuje, a których w specyfikacji nie ma
/// [T5 §4.2 + fact-check §3].
///
/// DLACZEGO wszystkie czternaście, a nie „te, o które ktoś poprosi": każde z nich wywraca
/// **dowolną** ścieżkę spec-strict komunikatem `Unexpected fields in frontmatter: …`, więc
/// jedno przeoczone pole daje „działa u mnie" i nie działa u pięciu pozostałych vendorów.
/// `hooks` jest tu dodatkowo polem wykonującym kod — umiejętność ściągnięta z sieci może je
/// nieść, a emiter jest ostatnim miejscem, w którym da się je zdjąć.
///
/// Zdjęcie nie znaczy „skasowanie bez śladu": [`place::emit`] zwraca listę zdjętych pól,
/// a treść `argument-hint` i `context: fork` wraca do ciała jako zdania [T5 §4.2].
pub const NON_SPEC_FIELDS: [&str; 14] = [
    "when_to_use",
    "argument-hint",
    "arguments",
    "disable-model-invocation",
    "user-invocable",
    "disallowed-tools",
    "model",
    "effort",
    "context",
    "agent",
    "background",
    "hooks",
    "paths",
    "shell",
];

/// Nazwa folderu, którą Claude Code pomija — w dowolnej wielkości liter
/// [T5 fact-check, „Worth adding"].
///
/// Umiejętność napisana w folderze o tej nazwie jest poprawna, zainstalowana i niewidoczna.
/// To jest ta sama klasa cichej porażki co zła ścieżka, więc walidacja ją blokuje, zamiast
/// ostrzegać.
pub const RESERVED_DIR_NAME: &str = "synced";

/// Zakres instalacji. Dwie wartości, bo vendorzy znają dwa: „u mnie" i „w tym repo".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Katalog domowy użytkownika — umiejętność widoczna w każdym projekcie.
    Global,
    /// Korzeń repozytorium — umiejętność jedzie z projektem do zespołu.
    Project,
}

/// Korzenie, których dotyka rozmieszczanie. Jeden argument zamiast trzech luźnych ścieżek,
/// żeby nie dało się podać `data` w miejsce `home` i dowiedzieć się o tym z testu.
#[derive(Debug, Clone)]
pub struct Roots {
    /// Katalog domowy — korzeń zakresu [`Scope::Global`].
    pub home: PathBuf,
    /// Korzeń repozytorium — korzeń zakresu [`Scope::Project`]. `None` znaczy „nie ma
    /// otwartego projektu" i [`place::plan`] odmawia wtedy zakresu projektowego, zamiast
    /// zgadywać katalog roboczy.
    pub project: Option<PathBuf>,
    /// Dane aplikacji. Leży tu kanoniczna kopia (`skills/<name>/`) i sidecar
    /// (`skills/installed.json`), czyli jedyny zapis o tym, który katalog vendora napisał
    /// Loadout, a który jest cudzy.
    pub data: PathBuf,
}

/// Plik dołączony do umiejętności: `scripts/`, `references/`, `assets/` [T5 §2.2].
///
/// Dwie ścieżki, bo instalacja to `fs::copy` z kanonicznej kopii do katalogu vendora —
/// a `fs::copy` zachowuje uprawnienia, czyli bit wykonywalności `scripts/run.sh`. Zapis
/// bajtów przez `fs::write` gubi go po cichu i skrypt przestaje się dać uruchomić dopiero
/// u użytkownika.
#[derive(Debug, Clone)]
pub struct BundledFile {
    /// Ścieżka **względna** wewnątrz katalogu umiejętności: `scripts/run.sh`.
    pub relative: PathBuf,
    /// Plik w kanonicznej kopii, z którego kopiujemy.
    pub source: PathBuf,
}

/// Kanoniczna umiejętność: sześć pól specyfikacji, ciało i pliki [T5 §4.1].
///
/// Kanoniczną postacią jest sama umiejętność spec-strict, nie osobna reprezentacja pośrednia:
/// warstwa tłumaczeń bez drugiego konsumenta to dokładnie ta złożoność, na którą umarł
/// poprzedni prototyp.
///
/// Pochodzenie i stan przeglądu (`Origin`, `TrustState`) **nie są** polami tej struktury i nie
/// trafiają do `metadata` — mieszkają w sidecarze aplikacji [T5 §4.1]. Dzięki temu plik, który
/// piszemy, jest bajt w bajt tym, co napisałby człowiek.
#[derive(Debug, Clone, Default)]
pub struct Skill {
    /// `^[a-z0-9]+(-[a-z0-9]+)*$`, ≤64 znaki. Jest też nazwą katalogu.
    pub name: String,
    /// Co robi **i kiedy jej użyć**, 1..=1024 znaki. Jedyne pole, po którym model decyduje,
    /// czy w ogóle sięgnąć po umiejętność.
    pub description: String,
    pub license: Option<String>,
    /// ≤500 znaków.
    pub compatibility: Option<String>,
    /// Mapa tekst→tekst. `BTreeMap`, bo kolejność kluczy w pliku ma być powtarzalna.
    pub metadata: BTreeMap<String, String>,
    /// Lista narzędzi rozdzielona spacjami. Pole **eksperymentalne**, wsparcie nierówne —
    /// emitujemy je, kiedy przyszło z importu, i nigdy się na nim nie opieramy [T5 ryzyka].
    pub allowed_tools: Option<String>,

    /// Markdown za front-matterem.
    pub body: String,
    pub files: Vec<BundledFile>,

    /// Pola front-mattera spoza specyfikacji, przyniesione przez import — surowy tekst
    /// wartości, bo emiterowi wystarczy wiedzieć, że pole **było**. To z tej mapy
    /// [`place::emit`] wylicza listę zdjętych pól i to stąd bierze treść, którą chowa
    /// w ciele zamiast wyrzucić.
    pub extras: BTreeMap<String, String>,
}

/// `SKILL.md` tak, jak leży: front-matter w kolejności z pliku i ciało.
///
/// Osobny typ od [`Skill`], bo walidacja odpowiada na pytanie o **plik**, nie o nasz model:
/// „czy są pola spoza szóstki", „czy nazwa katalogu zgadza się z `name`". Po zbudowaniu
/// [`Skill`] te pytania są już bez sensu — pola spoza szóstki wylądowały w `extras`, a nazwa
/// katalogu przestała istnieć.
#[derive(Debug, Clone, Default)]
pub struct SkillDoc {
    /// Pary `klucz → surowa wartość` w kolejności z pliku. Kolejność jest częścią wejścia,
    /// bo komunikat o nieoczekiwanych polach je wylicza.
    pub fields: Vec<(String, String)>,
    /// Markdown za front-matterem.
    pub body: String,
}

/// Błędy rozmieszczania.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),

    /// Umiejętność nie przeszła walidacji. Komunikaty są dosłownie te z walidatora
    /// referencyjnego [T5 §6.2] — jeden komunikat na przyczynę, nigdy jeden wspólny na osiem.
    #[error("{}", messages.join("; "))]
    Invalid { messages: Vec<String> },

    /// [`Scope::Project`] bez znanego korzenia repozytorium. Odmowa, nie katalog roboczy:
    /// zgadnięty korzeń zapisuje umiejętność w losowym miejscu i nikt się o tym nie dowie.
    #[error("there is no open project, so a project skill has no place to go")]
    NoProjectRoot,
}

/// Skrót modułu. Drugi parametr z domyślną wartością, bo `validate_strict` zwraca listę
/// komunikatów, a nie [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;
