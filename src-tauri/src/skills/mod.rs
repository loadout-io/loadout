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

pub mod ingest;
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
pub const DESTINATION_DIRS: [&str; 2] = [SHELF_CLAUDE_READS, SHELF_THE_OTHER_FIVE_READ];

/// Półka Claude Code. Osobna nazwa, bo osobno pyta o nią bieg.
///
/// 2026-08-22 (T-79) — rozbicie tablicy na dwie nazwane stałe nie dokłada ani jednego napisu:
/// tablica składa się z nich, więc `.agents/skills` dalej stoi w repo **raz**. Powodem jest
/// [`StepSkills::into_the_step_folder`], które pyta o JEDNĄ z tych dwóch półek — a indeks
/// (`DESTINATION_DIRS[1]`) byłby odwołaniem, które po przestawieniu tablicy dalej się kompiluje
/// i wskazuje na drugiego vendora.
pub const SHELF_CLAUDE_READS: &str = ".claude/skills";

/// Półka, do której zaglądają Codex, Cursor, Gemini CLI, opencode i Amp [T5 §3.1].
///
/// Dla tych pięciu **nie ma drugiego kanału**: żaden z nich nie umie przyjąć ścieżki katalogu
/// umiejętności argumentem, więc „agent ma umiejętność" znaczy dla nich dosłownie „plik leży
/// w jego katalogu roboczym".
pub const SHELF_THE_OTHER_FIVE_READ: &str = ".agents/skills";

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
/// Poprzedni prototyp.
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

/// Umiejętności, które **jeden krok** naprawdę dostaje — policzone z efektywnego agenta.
///
/// Zbiór liczy się tak, jak liczy się cała reszta definicji agenta: `library::agents::resolve`
/// scala krok z agentem patchem RFC 7396, więc brak klucza na kroku znaczy „weź to, co ma
/// agent", `[]` znaczy „żadnych", a lista znaczy **podzbiór** tego, co agent ma. Nazwa spoza
/// zbioru agenta jest odmową ([`Missing`]), nie cichym dołożeniem: krok, który dostał więcej,
/// niż dał mu jego agent, jest krokiem, o którego uprawnieniach nie mówi żaden ekran.
///
/// DWA POLA, NIE JEDNO, i to jest ta sama różnica, co w [`crate::inherit::Rewritten`]: `names`
/// odpowiada na pytanie „co człowiek wybrał", a `dirs` na pytanie „skąd to wzięliśmy". Sama
/// lista nazw kazałaby każdemu wołającemu drugi raz składać ścieżkę kanonicznej kopii, a druga
/// kopia tej reguły jest tą, która przy pierwszej zmianie układu katalogów zostaje stara
/// (niezmiennik 23).
///
/// Pusty zbiór jest normalnym wynikiem, nie awarią: tak wygląda krok agenta, któremu nikt
/// umiejętności nie przypisał, i krok, który wyłączył je wszystkie zapisem `[]`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StepSkills {
    /// Nazwy w kolejności z definicji agenta — tej samej, w której człowiek je widzi.
    pub names: Vec<String>,
    /// Katalog, z którego pojadą bajty każdej nazwy, w tej samej kolejności.
    ///
    /// 2026-09-02 — NIE ZAWSZE JEST TO KANONICZNA KOPIA `<dane>/skills/<nazwa>/`. Rozwiązywanie
    /// pyta też półek vendorów (`place::shelves_of`), więc pod tą nazwą może stać umiejętność
    /// napisana przez człowieka wprost w jego katalogu domowym albo w repozytorium, w którym
    /// pracuje. Która to była, mówi [`place::Whence`] — a `place::Found` niesie jedno i drugie
    /// obok siebie, żeby nikt nie musiał liczyć tej ścieżki drugi raz (niezmiennik 23).
    pub dirs: Vec<PathBuf>,
}

/// Czym umiejętność BYŁA, kiedy bieg po nią sięgnął: odcisk jej plików i ich łączna długość.
///
/// 2026-08-28 (T-154) — DWA POLA, NIE JEDNO, i to jest ten sam wybór, co przy
/// [`crate::commands::run`]`::MemoryRecord`: odcisk i liczba bajtów odpowiadają na to samo pytanie
/// dwiema drogami, więc rachunek przepisany po fakcie rozjeżdża się sam ze sobą. Liczone
/// z CAŁEGO katalogu, nie z samego `SKILL.md` — to katalog jedzie do kroku
/// ([`place::copy_the_skill`]), a umiejętność, której `scripts/check.sh` ktoś podmienił,
/// jest umiejętnością zmienioną.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Material {
    /// Odcisk ścieżek i bajtów, tą samą drogą, którą liczy się odcisk pliku workflow.
    pub hash: String,
    /// Ile bajtów miały te pliki razem.
    pub bytes: u64,
}

/// Dlaczego wybrana umiejętność nie dojedzie do kroku — po ludzku, bo to zdanie czyta człowiek.
///
/// Cztery powody, bo naprawia się je czterema różnymi ruchami: dopisz umiejętność do biblioteki,
/// dopisz ją agentowi, popraw jej `SKILL.md`, daj krokowi własną kopię folderu. Jedno wspólne
/// zdanie na cztery stany zostawia trzy czwarte ludzi przy instrukcji, która w ich przypadku nie
/// może zadziałać — ten sam wybór i ten sam powód stoi przy
/// [`crate::engine::drivers::claude::ToolsRefused`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Why {
    /// Nazwy nie ma w bibliotece Loadouta.
    #[error("your library has nothing saved under that name")]
    NotInTheLibrary,
    /// Nazwa jest w bibliotece, ale nie na agencie tego kroku.
    #[error(
        "the agent on this step was never given it, and a step may only narrow what its agent \
         already has"
    )]
    NotOnTheAgent,
    /// `SKILL.md` nie da się przeczytać albo nie przechodzi walidatora.
    #[error("its SKILL.md could not be read as a skill")]
    Unusable,
    /// Krok pracuje wprost w folderze człowieka, więc kopia nie ma gdzie stanąć.
    ///
    /// Odmowa, nie cichy zapis: katalog dopisany do cudzego repozytorium jest zmianą, o której
    /// jego właściciel dowiaduje się z `git status`, a Loadout obiecuje pisać wyłącznie do
    /// własnego katalogu biegu (`docs/ARCHITECTURE.md` §8).
    #[error(
        "this step works straight inside {}, and Loadout writes nothing into a folder of yours. \
         Give the step its own copy of your files, or take the skill off it",
        .folder.display()
    )]
    WouldWriteIntoYourFolder {
        /// Folder, do którego Loadout odmówił pisać — człowiek szuka ścieżki, nie identyfikatora.
        folder: PathBuf,
    },
}

/// Umiejętność, której ten krok nie dostanie — **odmowa nazywająca pozycję i krok**.
///
/// TO JEST ODMOWA, A NIE POMINIĘCIE, i to jest jedyny powód, dla którego ten typ istnieje obok
/// [`Error`]. Ciche pominięcie daje bieg, w którym człowiek zaznaczył pięć umiejętności, agent
/// dostał trzy, nic nie padło i nikt się o tym nie dowiedział — bo „agent nie zna tej
/// umiejętności" jest z zewnątrz nieodróżnialne od „model nie uznał, że warto po nią sięgnąć".
/// Ta sama cicha porażka, przed którą stoi [`crate::inherit::Error::NotInTheHost`].
///
/// Zdanie wymienia OBIE nazwy: bez nazwy umiejętności odmowa zamienia jedno odznaczenie
/// w przeszukiwanie listy, a bez nazwy kroku człowiek nie wie, który kafelek otworzyć.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "\"{step}\" was set to use the skill \"{skill}\", and {why}. Nothing started: a step that \
     quietly knows less than you picked answers as though there was nothing to know."
)]
pub struct Missing {
    /// Nazwa kroku — ta z kafelka, bo to jej szuka człowiek.
    pub step: String,
    /// Nazwa umiejętności, dosłownie tak, jak stoi w definicji.
    pub skill: String,
    /// Co dokładnie odmówiło.
    pub why: Why,
}

/// Co się stało z materiałem, który bieg zamroził — po ludzku, bo to zdanie czyta człowiek.
///
/// 2026-08-28 (T-154) — DWA ZDANIA, BO DWIE RÓŻNE NAPRAWY, i to jest ten sam powód, dla którego
/// [`Why`] ma cztery warianty zamiast jednego wspólnego. Materiał, który się zmienił, przywraca
/// się do postaci z tamtego dnia albo puszcza od nowa jako nowy bieg; umiejętność, po którą ten
/// bieg już nie sięga, wraca dopisaniem jej agentowi. Jedno wspólne zdanie na oba stany zostawia
/// połowę ludzi przy instrukcji, która w ich przypadku nie może zadziałać.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Moved {
    /// Bieg deklarował ją na starcie, a dziś jej w tym kroku nie ma.
    #[error("this run does not reach it any more, so repeating the step would work with less")]
    Gone,
    /// Jest, tylko pod tą nazwą leży dziś co innego.
    #[error("what is saved under that name is not the material the first run was given")]
    Changed,
}

/// Umiejętność, którą bieg zamroził i która nie jest już tym, czym była — **odmowa nazywająca
/// pozycję i krok**.
///
/// 2026-08-28 (T-154). TO JEST ODMOWA, A NIE POWTÓRZENIE Z INNYM MATERIAŁEM, i to jest jedyny
/// powód, dla którego ten typ istnieje obok [`Missing`]. Człowiek powtarza krok po to, żeby
/// zobaczyć, czy JEGO poprawka zmieniła wynik — powtórzenie, w którym po cichu przesunęło się
/// też wejście, odpowiada na inne pytanie i nie mówi o tym ani słowem. Ta sama cicha porażka,
/// przed którą stoi `commands::run::what_the_run_before_left`.
///
/// Zdanie wymienia OBIE nazwy, dokładnie jak [`Missing`]: bez nazwy umiejętności odmowa zamienia
/// jedno przywrócenie pliku w przeszukiwanie biblioteki, a bez nazwy kroku człowiek nie wie,
/// który kafelek otworzyć (niezmiennik 29).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "\"{step}\" ran with the skill \"{skill}\", and {why}. Nothing started: running it again with \
     other material answers a different question than the one you are comparing it against."
)]
pub struct NotAsItWas {
    /// Nazwa kroku — ta z kafelka, bo to jej szuka człowiek.
    pub step: String,
    /// Nazwa umiejętności, dosłownie tak, jak stała w tamtym biegu.
    pub skill: String,
    /// Co dokładnie się z nią stało.
    pub why: Moved,
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

    /// Umiejętność, której ten krok nie dostanie ([`Missing`]).
    ///
    /// PRZEZROCZYSTY, bo [`Missing`] jest już zdaniem napisanym dla człowieka — a zdanie
    /// nadpisane drugim zdaniem o tej samej odmowie to dwa miejsca, w których mieszka jedna
    /// odpowiedź (niezmiennik 13). Wariant istnieje po to, żeby rozmieszczanie umiejętności
    /// kroku miało JEDEN typ błędu na dwa różne stany: odmowę (ta pozycja nie dojedzie) i awarię
    /// dysku ([`Error::Io`]). Bez niego awaria dysku musiałaby udawać jedną z czterech przyczyn
    /// z [`Why`], czyli kłamać o tym, co się stało.
    #[error(transparent)]
    Refused(#[from] Missing),
}

/// Skrót modułu. Drugi parametr z domyślną wartością, bo `validate_strict` zwraca listę
/// komunikatów, a nie [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;
