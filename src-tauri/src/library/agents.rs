//! Definicja agenta: kształt na drucie, dwuwarstwowe dziedziczenie, plik na dysku
//! i przelotka na opcje vendora.
//!
//! **To jest szkielet.** Ciała funkcji są `todo!()`, żeby kryteria akceptacji dało się
//! uruchomić i żeby padły na BRAKU ZACHOWANIA, a nie na braku modułu (`AGENTS.md` §2a p. 5).
//! `clippy::todo = "deny"` w `Cargo.toml` pilnuje, żeby ani jeden z nich nie dożył pełnej
//! bramki.
//!
//! Parametry mają na razie podkreślenie, bo puste ciało ich nie używa, a `-D warnings`
//! w bramce podnosi `unused_variables` do błędu. Implementacja zdejmuje podkreślenie tym
//! samym ruchem, którym kasuje `todo!()` — jedno i drugie znika razem.
//!
//! Trzy reguły trzymają ten plik w kupie i żadna nie jest kosmetyczna:
//!
//! 1. **Każde pole [`Agent`] jest wymagane** — ani jednego `Option<T>`. Szablon jest zawsze
//!    kompletny, więc wynik złożenia zawsze się deserializuje [T4 §4.3, reguła 2].
//! 2. **[`Overrides`] jest w całości `Option`.** „Czy to nadpisane?" ma być pytaniem
//!    o typ, nie o wartość.
//! 3. **Nigdzie nie ma `null`.** 2026-08-15, sprawdzone lokalnie na `json-patch` 4.2.0:
//!    w RFC 7396 `null` w patchu **kasuje klucz**, a skasowany klucz to plik ustawień,
//!    który się nie wczyta [T4 §4.3, reguła 1]. Dlatego „brak limitu" to
//!    `giveUpAfterMinutes: 0`, „brak umiejętności" to `[]`, a „wszystkie narzędzia" to
//!    wariant [`Tools::Everything`]. Kodujemy brak wartością, nigdy pustką.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Program, który uruchamia agenta.
///
/// **Nie da się tego nadpisać na kroku** [T4 §6.4]. Przełączenie vendora unieważniłoby
/// połowę pozostałych pól (lista `tools`, której Codex nie umie uszanować), a odmowa
/// na poziomie typu kasuje całą klasę walidacji. Ta sama rola u drugiego vendora to
/// duplikat agenta — jedno kliknięcie i plik, który da się przeczytać.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Vendor {
    ClaudeCode,
    Codex,
}

/// Ile agent ma myśleć. Cztery szczeble, tłumaczone niżej na `--effort`
/// i `model_reasoning_effort` — nazwy vendorów nigdy nie docierają na ekran (niezmiennik 14).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Thinking {
    Quick,
    #[default]
    Balanced,
    Deep,
    Deepest,
}

/// Co agent może zrobić z plikami. Trzypozycyjny dial bezpieczeństwa, jedyny, jaki widzi
/// użytkownik; siedem trybów Claude'a i trzy piaskownice Codeksa są tłumaczone pod spodem
/// [T4 §3.3, §6.3].
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FileAccess {
    #[default]
    LookOnly,
    AskFirst,
    WorkFreely,
}

/// Narzędzia: wszystkie albo wymieniona lista.
///
/// `rename_all` **i** `rename_all_fields` stoją tu razem świadomie (04 §2.5): pierwsze
/// nazywa warianty, drugie pola wariantu strukturalnego. Dzisiaj `Only` jest wariantem
/// krotkowym, więc drugi atrybut nic nie robi — i po to tu jest. Dzień, w którym ktoś
/// zamieni go na `Only { names: Vec<String> }`, jest dniem, w którym bez niego
/// `started_at` poleciałoby do frontendu, który czyta wyłącznie `camelCase`, i położyło
/// ekran.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum Tools {
    #[default]
    Everything,
    Only(Vec<String>),
}

/// Kolor tożsamości agenta. Pięć wartości, bo tyle jest tokenów `--id-1`…`--id-5`
/// (`docs/design/DESIGN.md` §3).
///
/// Enum, nie `String` — i to jest odpowiedź na otwarte pytanie O6 z T4, które brzmiało
/// „ośmiu Claude'owych czy własnych". DESIGN §3 rozstrzygnął je na pięć przygaszonych
/// tokenów, bo kolor tożsamości **nigdy** nie może być pomylony z kolorem stanu; dla
/// ośmiu Claude'owych nie mamy tokenów, a kolor bez tokenu to hex w komponencie.
/// Przy enumie „`color: neon` jest odmową" wynika z typu, a nie z walidatora, który
/// ktoś kiedyś zapomni zawołać.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Color {
    Slate,
    Plum,
    Clay,
    Moss,
    Rose,
}

/// Surowe opcje vendora: `{ "claude": { "--jakas-nowa-flaga": "wartosc" } }`.
///
/// `BTreeMap`, nie `HashMap`: zapis ma być deterministyczny, żeby dwa zapisy tej samej
/// definicji dały bajt w bajt ten sam plik (`DECISIONS-LOCKED.md` §D6). `HashMap` daje
/// przy każdym uruchomieniu inną kolejność kluczy, czyli plik, który „zmienia się sam"
/// w każdym `git diff`.
pub type VendorOptions = BTreeMap<String, BTreeMap<String, String>>;

/// Zapisany agent. Piętnaście kluczy na drucie i ani jednego z podkreśleniem.
///
/// `deny_unknown_fields` jest tu jedyną obroną przed defektem zmierzonym w T4 §9:
/// `claude --agents '{"broken":{"model":"sonnet"}}' -p "hi"` kończy się **kodem 0, bez
/// słowa na stderr**. Źle zbudowana definicja wygląda dokładnie tak samo jak zła instrukcja
/// w promptcie i kosztuje godziny diagnozy — więc walidacja jest nasza i dzieje się, zanim
/// cokolwiek odpalimy.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Agent {
    /// Wersja schematu. Jedna liczba, wprowadzona teraz, bo dopisanie jej później znaczy
    /// zgadywanie, co znaczą pliki bez niej [T4 §5.2].
    pub schema: u8,
    /// Stabilny przez zmianę nazwy. Ukryty przed użytkownikiem.
    pub id: Uuid,
    pub name: String,
    /// Jedno zdanie „co to robi" — etykieta `What it does`.
    pub summary: String,
    pub color: Color,
    /// Prompt systemowy. Na dysku jest treścią pliku, nie kluczem front-mattera.
    pub instructions: String,
    pub runs_with: Vendor,
    pub model: String,
    pub thinking: Thinking,
    pub file_access: FileAccess,
    /// `0` znaczy „bez limitu". Nigdy `None` — patrz reguła 3 w nagłówku modułu.
    pub give_up_after_minutes: u32,
    pub tools: Tools,
    pub skills: Vec<String>,
    /// Nazwy serwerów narzędziowych. W interfejsie: `Connections`.
    pub connections: Vec<String>,
    /// Ścieżka pliku pamięci; `""` znaczy „nigdzie". Ustawiane **na kroku**, nie w tym
    /// formularzu — ścieżka wyniku należy do kroku, nie do roli (`docs/mockup/index.html`,
    /// panel kroku). W typie zostaje, bo krok nadpisuje pole, którego szablon musi mieć.
    pub write_results_to: String,
    /// Przelotka `DECISIONS-LOCKED.md` §D6: Loadout tego **nie interpretuje**. Bez niej
    /// każda nowa flaga vendora wymaga wydania Loadouta.
    ///
    /// `skip_serializing_if` nie jest tu wygodą. Pusta przelotka nie ma prawa dołożyć
    /// szesnastego klucza do zapisanego agenta — to jest dokładnie ten „jeden klucz
    /// więcej", przed którym broni kryterium 1.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vendor_options: VendorOptions,
}

impl Agent {
    /// Agent, na którym opisujemy kształt na drucie. Jeden, żeby kryterium miało co
    /// serializować, i żeby „jak wygląda zapisany agent" miało jedną odpowiedź w repo.
    #[must_use]
    pub fn example() -> Self {
        todo!("Agent::example: kryterium 1 zamraża piętnaście kluczy tego obiektu")
    }
}

/// Co pamięta jeden krok workflow: **wyłącznie różnicę** wobec agenta.
///
/// Serializuje się do patcha RFC 7396 — brak klucza znaczy „idź za agentem". Nigdy nie
/// emituje `null`, bo `skip_serializing_if` nie ma jak go wyprodukować.
///
/// Czego tu nie ma i mieć nie będzie: `id`, `name`, `runsWith`. Krok, który przestawia
/// vendora, unieważnia połowę reszty [T4 §6.4].
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Overrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<Thinking>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_access: Option<FileAccess>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub give_up_after_minutes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Tools>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connections: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub write_results_to: Option<String>,
}

/// Wynik złożenia agenta z nadpisaniem: co naprawdę pobiegnie plus lista nazw do znacznika
/// „N changed".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub agent: Agent,
    /// Nazwy nadpisanych pól, posortowane. Puste, kiedy krok niczego nie zmienił.
    pub changed: Vec<String>,
}

/// Co poszło nie tak z definicją agenta.
///
/// Komunikat zawsze nazywa **plik**, a przy pliku także klucz, przez który się wywrócił.
/// T4 §10: „pokaż nazwę pliku i «Open in editor», nie połykaj" — połknięty błąd wygląda
/// jak zła instrukcja w promptcie i kosztuje godziny.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// Pliku nie da się przeczytać albo mówi coś, czego definicja agenta nie zna.
    #[error("{file} — {detail}")]
    Unreadable { file: String, detail: String },
    /// Na drucie stanęła pustka tam, gdzie ma stać wartość. W RFC 7396 `null` kasuje
    /// klucz, więc przepuszczony `null` produkuje plik ustawień, który się nie wczyta.
    #[error("{field} has no value. Remove the line to go back to the agent's setting")]
    EmptySetting { field: String },
}

/// Agent + nadpisania -> co naprawdę pobiegnie, plus lista nazw dla znacznika „N changed".
///
/// Cała algebra dziedziczenia to te kilkanaście linii: złożenie RFC 7396 i policzenie
/// kluczy patcha. Wariant „pełna kopia agenta na kroku" (T4 §4.1 A) byłby prostszy
/// i **fałszywy**: edycja szablonu nigdy nie dotarłaby do workflow.
pub fn resolve(_base: &Agent, _overrides: &Overrides) -> Result<Resolved, serde_json::Error> {
    todo!("resolve: złożenie RFC 7396 plus posortowane nazwy nadpisanych pól")
}

/// Formularz pokazuje wartości efektywne; przy zapisie zostaje z nich **sama różnica**.
///
/// Pola, których krok nie może ruszyć (`id`, `name`, `runsWith`), nie mają prawa wypłynąć
/// do patcha, choćby się różniły.
pub fn capture(_base: &Agent, _edited: &Agent) -> Result<Overrides, serde_json::Error> {
    todo!("capture: różnica ograniczona do pól, które krok może zmienić")
}

/// Odmawia `null`-a na surowym JSON-ie, zanim stanie się on [`Overrides`] albo [`Agent`].
///
/// Woła się to na tym, co przyszło z zewnątrz — z formularza, z pliku workflow, z importu.
/// Po tym sprawdzeniu złożenie merge patchem jest funkcją totalną, a jedyna słynna
/// pułapka RFC 7396 znika [T4 §4.3].
pub fn validate_no_nulls(_raw: &Value) -> Result<(), AgentError> {
    todo!("validate_no_nulls: komunikat ma nazwać pole, na którym stanęła pustka")
}

/// Czyta `agents/<slug>.md`: front-matter YAML + treść jako instrukcje.
///
/// Treść jest instrukcjami i **tylko** treść nimi jest. Klucz `instructions` we
/// front-matterze dawałby dwa źródła prawdy dla najdłuższego pola definicji [T4 §5.1].
pub fn read_agent_file(_path: &Path) -> Result<Agent, AgentError> {
    todo!("read_agent_file: front-matter -> JSON -> Agent, treść -> instructions")
}

/// Zapisuje agenta do `dir/<slug>.md` i zwraca ścieżkę, pod którą wylądował.
///
/// Slug wyprowadzamy z nazwy tutaj, w jednym miejscu, żeby wołający nie musiał znać
/// reguły — a przy okazji żeby była JEDNA reguła.
pub fn write_agent_file(_dir: &Path, _agent: &Agent) -> Result<PathBuf, AgentError> {
    todo!("write_agent_file: deterministyczny front-matter, treść pod nim")
}

/// Tłumaczy przelotkę [`VendorOptions`] na dodatkowe argumenty **jednego** vendora.
///
/// Czysta funkcja i nic więcej. Komendę buduje sterownik — `claude.rs` (T-04) i `codex.rs`
/// (T-10) — bo polityka mieszka w jednym rdzeniu, a adaptery mają po pięć linii
/// (niezmiennik 23). Nazwy vendora, której nie znamy, nie tykamy: przelotka ma przetrwać
/// vendora, którego jeszcze nie wspieramy (`DECISIONS-LOCKED.md` §D6).
#[must_use]
pub fn vendor_args(_agent: &Agent, _vendor: &str) -> Vec<String> {
    todo!("vendor_args: para klucz-wartość obok siebie, tylko dla tego vendora")
}
