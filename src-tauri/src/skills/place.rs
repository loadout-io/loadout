//! Rozmieszczanie: walidacja, emiter `SpecStrict`, plan, kopiowanie, usuwanie i werdykt
//! „czy vendor to widzi".
//!
//! Cicha porażka, przed którą stoi cały ten plik, to zielony ptaszek „Installed for 6 tools"
//! postawiony dlatego, że `fs::write` zwróciło `Ok`. Plik leży, ścieżka jest o poziom obok tej,
//! w którą vendor zagląda, a użytkownik dowiaduje się o tym nigdy — bo „agent nie wie
//! o umiejętności" nie da się odróżnić od „model nie uznał, że warto jej użyć". Dlatego
//! [`discovery_from_init`] czyta zdarzenie od vendora zamiast wnioskować z kodu powrotu.
//!
//! **Kopiujemy, nie symlinkujemy** [T5 §4.5]. Dowiązanie działa u Claude Code i jest tam nawet
//! udokumentowane, ale: u pozostałych pięciu vendorów jest niezweryfikowane, rozpada się
//! u każdego kolegi z zespołu, który sklonuje repo z umiejętnością w zakresie projektu, i na
//! Windowsie wymaga trybu dewelopera albo uprawnień administratora — czyli jest największym
//! zagrożeniem dla przenośności w całym tym projekcie. `fs::copy` zachowuje przy tym
//! uprawnienia, więc `scripts/run.sh` zostaje wykonywalny.
//!
//! Kodu platformowego tu nie ma (niezmiennik 3): dowiązanie wykrywamy `fs::symlink_metadata`,
//! nie `#[cfg(unix)]`.

use std::path::{Path, PathBuf};

use super::{Result, Roots, Scope, Skill, SkillDoc};

/// Co instalacja zapisze — pokazywane człowiekowi, zanim cokolwiek się wydarzy.
#[derive(Debug, Clone)]
pub struct InstallPlan {
    /// Dwa katalogi umiejętności: `<korzeń>/.claude/skills/<name>` i `…/.agents/skills/<name>`.
    pub writes: Vec<PathBuf>,
    /// Katalogi o tej nazwie, które już tam są — nasze do nadpisania albo cudze.
    pub conflicts: Vec<Conflict>,
    /// Sidecar, w którym [`apply`] zapisze, że te katalogi napisał Loadout.
    ///
    /// Plan niesie tę ścieżkę, żeby [`apply`] nie potrzebowało [`Roots`]: plan jest pełnym
    /// opisem tego, co się stanie, a zapis „to jest nasze" jest częścią tego, co się stanie.
    /// Sidecar leży poza oboma drzewami docelowymi — jest zapisem Loadouta o wyjściu builda,
    /// nie kolejnym plikiem obok `SKILL.md` (niezmiennik 21: nikt nie czyta `.loadout-marker`,
    /// a sidecar czytają [`plan`] i [`remove`]).
    pub sidecar: PathBuf,
}

/// Katalog o tej samej nazwie już istnieje w miejscu docelowym.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conflict {
    /// Loadout pisał ten katalog wcześniej — jest w sidecarze. Instalacja go nadpisze,
    /// i tak ma być: katalogi vendorów są wyjściem builda (niezmiennik 4).
    Update { path: PathBuf },
    /// Katalogu nie ma w sidecarze, więc nie jest nasz. Nie nadpisujemy go bez pytania.
    Foreign {
        path: PathBuf,
        /// Pierwszy wiersz cudzego `SKILL.md`, zacytowany dosłownie — żeby człowiek
        /// zobaczył, czyj to plik, zanim zdecyduje.
        first_line: String,
    },
}

/// Wynik usunięcia.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Removed {
    /// Obie kopie zdjęte. Kanoniczna umiejętność w danych aplikacji zostaje — usuwamy
    /// wyjście builda, nie źródło (niezmiennik 4).
    Done { paths: Vec<PathBuf> },
    /// Co najmniej jeden katalog o tej nazwie nie jest nasz. Nie kasujemy **niczego**:
    /// pół usunięcia zostawia stan, którego nikt nie umie opisać, a cudza umiejętność
    /// skasowana „przy okazji" jest nie do odzyskania.
    Skipped {
        path: PathBuf,
        /// Zdanie dla człowieka: dlaczego to zostało.
        why: String,
    },
}

/// Czy vendor naprawdę widzi umiejętność [T5 §6.3, poziom 3].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Discovery {
    /// Vendor wymienił ją na swojej liście.
    Seen,
    /// Vendor wymienił swoje umiejętności i tej wśród nich nie ma.
    NotSeen {
        /// Ścieżki, w które pisaliśmy. To jest cała treść zgłoszenia dla człowieka:
        /// „napisaliśmy tu i tu, a vendor tego nie widzi".
        looked_in: Vec<PathBuf>,
    },
    /// Nie wiadomo — i to nie jest błąd (niezmiennik 5). Brak CLI, zdarzenie o nieznanym
    /// kształcie, nowa wersja vendora: żadne z tego nie może zaświecić się na czerwono.
    Unknown(&'static str),
}

/// Dwa katalogi docelowe dla danego zakresu — same korzenie, bez `<name>`.
///
/// `project = None` przy [`Scope::Project`] daje ścieżki względne, czyli „tutaj": tak samo
/// rozwiązuje je Codex (`$CWD/.agents/skills`). Do dysku to nigdy nie dochodzi — [`plan`]
/// odmawia zakresu projektowego bez korzenia zwrotem [`super::Error::NoProjectRoot`].
#[must_use]
pub fn destinations(scope: Scope, home: &Path, project: Option<&Path>) -> [PathBuf; 2] {
    todo!("T-18 AC-1: dwa korzenie z DESTINATION_DIRS, {scope:?} {home:?} {project:?}")
}

/// Reguły walidatora referencyjnego, przepisane w Ruście [T5 §6.2].
///
/// Przepisane, nie wywołane: `agentskills` jest w Pythonie, a `uv` jako zależność środowiska
/// uruchomieniowego aplikacji desktopowej w Ruście to zły interes za ~40 linii reguł. CLI
/// zostaje wyrocznią różnicową w naszym własnym CI, nie w produkcie.
///
/// Komunikaty są **dosłownie** te z wyroczni, bo użytkownik zobaczy je też wtedy, gdy vendor
/// odmówi po swojemu — a dwa różne zdania o tej samej przyczynie to dwa różne zgłoszenia.
/// Jedna przyczyna, jeden komunikat: wspólne „invalid skill" na osiem przyczyn nie mówi
/// nikomu, co poprawić.
///
/// Nazwa katalogu jest osobnym argumentem, bo dwie reguły dotyczą **jej**, nie pliku:
/// zgodność z `name` i zarezerwowane [`super::RESERVED_DIR_NAME`].
pub fn validate_strict(dir_name: &str, doc: &SkillDoc) -> Result<(), Vec<String>> {
    todo!("T-18 AC-3: reguły z T5 §6.2 dla {dir_name} i {doc:?}")
}

/// Jeden `SKILL.md` w trybie `SpecStrict` i lista pól, które zostały zdjęte.
///
/// Jeden plik do obu katalogów, nie dwa warianty: jeden plik to jeden diff i jedna rzecz do
/// zdebugowania. `ClaudeExtended` jest świadomie poza zakresem [T5 §11].
///
/// Zdjęte pola **wracają**, nie giną: `argument-hint` jako wiersz `Arguments: …`, a
/// `context: fork` jako `Run this as an isolated task.` przed pierwszym akapitem [T5 §4.2].
/// Reszta z czternastki nie ma przenośnego odpowiednika i zostaje tylko na zwróconej liście —
/// po to, żeby UI mogło powiedzieć, co dokładnie zniknęło.
#[must_use]
pub fn emit(skill: &Skill) -> (String, Vec<&'static str>) {
    todo!("T-18 AC-2: front-matter w kolejności SPEC_FIELDS dla {skill:?}")
}

/// Co się wydarzy, jeszcze zanim cokolwiek się wydarzy.
///
/// Waliduje **przed** pierwszym zapisem i nie tworzy ani jednego katalogu — żadnego
/// „utwórzmy, żeby sprawdzić uprawnienia". Dwa powody: użytkownik ma zobaczyć listę zmian,
/// zanim je zatwierdzi, a odmowa w połowie zostawia katalog, którego nikt nie posprząta.
pub fn plan(skill: &Skill, scope: Scope, roots: &Roots) -> Result<InstallPlan> {
    todo!("T-18 AC-4: plan bez zapisu dla {skill:?} {scope:?} {roots:?}")
}

/// Wykonuje plan: `SKILL.md` z [`emit`] do obu katalogów, pliki dołączone przez `fs::copy`,
/// wpis do sidecara.
///
/// Tworzy dokładnie te ścieżki, które plan wymienił, i ani jednej więcej.
pub fn apply(plan: &InstallPlan, skill: &Skill) -> Result<()> {
    todo!("T-18 AC-4: zapis dokładnie tego, co w {plan:?}, dla {skill:?}")
}

/// Zdejmuje obie kopie umiejętności — i nic poza nimi.
///
/// Kanoniczna kopia w danych aplikacji zostaje: katalogi vendorów są wyjściem builda,
/// a źródło jest jedno (niezmiennik 4). Katalog, którego nie ma w sidecarze, jest cudzy
/// i nie jest kasowany.
pub fn remove(name: &str, scope: Scope, roots: &Roots) -> Result<Removed> {
    todo!("T-18 AC-6: zdjęcie obu kopii {name} dla {scope:?} {roots:?}")
}

/// Werdykt „czy Claude to widzi", odczytany ze zdarzenia `system`/`init`.
///
/// Reguła ma **kolejność** i to jest jej cała treść: jeśli zdarzenie niesie tablicę `skills`,
/// liczy się wyłącznie ona; jeśli nie — liczy się `slash_commands`, bo tak umiejętność
/// z `~/.claude/skills` objawia się w CLI v2.1.233; jeśli nie ma żadnej z nich, odpowiedź
/// brzmi [`Discovery::Unknown`], nigdy „nie widzi".
///
/// DLACZEGO nie `init_line.contains(name)`: nazwa umiejętności potrafi wystąpić w `cwd`
/// (`/home/u/review-pull-requests/x`) i w `mcp_servers[].name`, nie występując w żadnej
/// z dwóch tablic. Wyszukiwanie po całej linii mówi wtedy „widzi" i to jest dokładnie ten
/// fałszywy zielony ptaszek, o który chodzi w tym zadaniu.
///
/// Pusty `init_line` znaczy „CLI nigdy nie wystartowało", czyli `Unknown("not installed")` —
/// brak vendora nie może być czerwony [T5 §6.3].
#[must_use]
pub fn discovery_from_init(name: &str, init_line: &str, wrote: &[PathBuf]) -> Discovery {
    todo!("T-18 AC-5: skills, potem slash_commands, dla {name} w {init_line} ({wrote:?})")
}
