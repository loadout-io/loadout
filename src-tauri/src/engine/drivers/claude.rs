//! `ClaudeDriver` — jeden długo żyjący proces, dwukierunkowy stdin, wiele tur w jednej sesji.
//!
//! Zweryfikowane end-to-end na tej maszynie: proces zostaje przy życiu między turami, oddaje
//! ten sam `session_id`, przyjmuje przerwanie w paśmie i wychodzi 0, kiedy zamkniemy mu stdin
//! [T1 §2, §4.6, 2026-08-15]. Wariant awaryjny — nowy proces na turę z `--resume` — jest
//! legalny i za tym samym traitem, ale płaci zimny start i odbudowę cache'u przy **każdej**
//! turze [T1 §8.1]. To jest ten koszt, którego to zadanie ma uniknąć.
//!
//! # Trzy rzeczy, które w tym pliku wychodzą cicho źle. Wszystkie zmierzone.
//!
//! **1. Brak izolacji kontekstu.** Bez `--strict-mcp-config --setting-sources ""` jeden bieg
//! ładuje 73 narzędzia z 9 serwerów i pali **36 870** tokenów tworzenia cache'u zamiast
//! **4 725** [T1 §3.3, korekta 4, 2026-08-15]. Nic nie pęka — jest tylko drożej i wolniej, na
//! każdym kroku, na zawsze. `--tools ""` **nie wystarcza**: pierwszy bieg podał ją i `init`
//! dalej wymieniał wszystkie narzędzia `mcp__`.
//!
//! **2. `--bare`.** Vendor sam ją poleca i zapowiada jako przyszłą domyślną dla `-p`
//! [T1 §3.3, docs] — a ona **nigdy nie czyta OAuth ani keychaina** i tutaj wywaliła bieg na
//! `Not logged in · Please run /login`, `terminal_reason:"api_error"` [T1 §3.3, ran].
//! Użytkownik subskrypcji nie może jej użyć. Dlatego izolacja idzie dwiema flagami wyżej,
//! a nie tą jedną.
//!
//! **3. `subtype`.** Ten sam nieudany bieg przyszedł z `"subtype":"success"` przy
//! `"is_error":true` [T1 §4.4, potwierdzone ponownie]. Sterownik czytający `subtype` melduje
//! sukces kroku, który nie zrobił nic, a stożek poniżej rusza na pustym przekazaniu. Czytamy
//! `is_error` i `terminal_reason`; wyjście procesu jest sygnałem **drugorzędnym** [T1 §8.5].
//!
//! # Co ten plik posiada, a czego nie
//!
//! Tu mieszka wire enum Claude i mapowanie **linia → [`AgentEvent`]**. Pętla czytająca, tee
//! surowego `agent-<id>.jsonl` na dysk i kuracja `AgentEvent` → `Line` należą do T-05. Ten
//! podział jest jedynym, przy którym `CodexDriver` (T-10) powstaje bez dotykania `stream.rs`.
//!
//! # Stan tego pliku: JEDNA TURA NA SESJĘ (2026-08-15)
//!
//! Argv, dekoder i anulowanie są kompletne. **Druga tura tym samym procesem nie działa**, i to
//! nie jest niedokończona robota, tylko zgłoszenie: koperta kolejnej tury potrzebuje stdinu,
//! który przeżyje pierwszy zapis, a jedyny start procesu w tym repo
//! (`engine::supervisor::spawn`) zamyka go po jednym `StdinPlan::Write`. Brakujący wariant
//! i akcesor leżą w `supervisor.rs`, czyli poza blokiem OWNS tego zadania — `AGENTS.md` §7.
//! Powód rozpisany jest przy [`AgentDriver::start`] i [`AgentHandle::send`].

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::anyhow;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{ChildStdout, Command};
use tokio::sync::mpsc;

use super::{
    AgentDriver, AgentEvent, AgentHandle, FinishReason, Outcome, Policy, Probe, RunSpec,
    SessionRef, Tokens,
};
use crate::engine::supervisor::{self, DEFAULT_GRACE, GroupId, GroupProof, StdinPlan, Supervised};

/// Etykieta tego vendora — ta sama w [`SessionRef::vendor`] i w [`AgentDriver::id`].
pub const VENDOR: &str = "claude";

/// Czym woła się CLI, kiedy nikt nie podał własnej ścieżki.
///
/// Gołe „claude", nie ścieżka bezwzględna: na tej maszynie to skrypt powłoki, który znajduje
/// się przez `PATH` — a `PATH` jest jedną z sześciu zmiennych, które supervisor przepuszcza
/// przez `env_clear()` [T-03, `PASSTHROUGH`].
const DEFAULT_BINARY: &str = "claude";

/// Wiersz transportu: cztery flagi, które decydują, **czym** jest to wywołanie.
///
/// `--verbose` nie jest ozdobą — bez niej CLI odmawia startu, dosłownie:
/// `Error: When using --print, --output-format=stream-json requires --verbose` [T1 §3.1, ran].
/// `--input-format stream-json` jest tą jedną flagą, dzięki której proces zostaje żywy między
/// turami; bez niej każda tura płaci zimny start i odbudowę cache'u [T1 §4.6, ran].
const TRANSPORT: [&str; 6] = [
    "-p",
    "--output-format",
    "stream-json",
    "--input-format",
    "stream-json",
    "--verbose",
];

/// Izolacja kontekstu, dwie flagi i **argument o zerowej długości**.
///
/// 2026-08-15 — bieg bez nich załadował 73 narzędzia MCP z 9 serwerów i spalił **36 870**
/// tokenów tworzenia cache'u zamiast **4 725** [T1 §3.3, korekta 4, ran]. Nic nie pęka; jest
/// tylko drożej i wolniej, na każdym kroku, na zawsze. `--tools ""` **nie wystarcza**: bieg,
/// który ją podał, dalej wymieniał w `init` wszystkie narzędzia `mcp__` [T1 §3.3, ran].
///
/// Wartość `--setting-sources` ma **zero znaków** i to jest cała różnica: `"user,project"`
/// w tym miejscu przechodzi każde sprawdzenie pytające o obecność flagi i nie izoluje niczego.
const LEAN_CONTEXT: [&str; 3] = ["--strict-mcp-config", "--setting-sources", ""];

/// `subtype` linii `system`, która ogłasza sesję, model, narzędzia i zdolności [T1 §4.1].
const INIT: &str = "init";

/// `subtype` linii, która znaczy „model myśli" — i nic poza tym.
const THINKING_TOKENS: &str = "thinking_tokens";

/// `subtype` linii o ponowieniu zapytania do dostawcy [T1 §4.5, docs].
const API_RETRY: &str = "api_retry";

/// Jedyny stan limitu, przy którym jest jeszcze co wysyłać.
const ALLOWED: &str = "allowed";

/// Po tym prefiksie `subtype` poznajemy, że linia `result` opisuje błąd — używane **wyłącznie**
/// wtedy, gdy vendor nie dosłał `is_error`.
const ERROR_PREFIX: &str = "error";

/// Po tym prefiksie poznajemy sufit: `error_max_turns` i cokolwiek, co vendor dołoży obok.
const CEILING_PREFIX: &str = "error_max";

/// `terminal_reason` tury zdjętej przerwaniem.
const CANCELLED: &str = "cancelled";

/// Ile znaków wolno mieć jednolinijkowemu podsumowaniu, zanim zostanie przycięte. Pełne wyjście
/// i tak zostaje za kliknięciem — to jest linia w wierszu, nie dokument.
const SUMMARY_LIMIT: usize = 120;

/// Ile wyników tury mieści się w kanale między pętlą czytającą a [`AgentHandle::wait`].
///
/// Tura jest jedna naraz, więc jeden slot wystarczyłby — ale wynik, który nie ma gdzie wejść,
/// zatrzymuje pętlę czytającą, a zatrzymana pętla wygląda dokładnie jak zawieszony agent.
/// Zapas jest tańszy niż to rozróżnienie w zgłoszeniu błędu.
const TURNS_IN_FLIGHT: usize = 8;

/// Cała tabela tłumaczenia polityki na flagi vendora — **jedna, w adapterze** (niezmiennik 23).
///
/// Zwraca tryb uprawnień i listę dozwolonych narzędzi; `None` w drugim polu znaczy „nie wysyłaj
/// `--allowedTools` w ogóle".
///
/// **`Unrestricted` nie dostaje listy i to nie jest przeoczenie.** Lista dozwolonych narzędzi
/// nie ogranicza `bypassPermissions` — wszystko jest zatwierdzone niezależnie od niej
/// [T1 §5.2]. Wysłanie obu naraz to kłamstwo o tym, co jest ograniczone: w argv widać listę,
/// w rzeczywistości nie obowiązuje nic, a kto czyta `ps` albo dziennik, ten uwierzy liście.
///
/// Żaden wariant nie brzmi `default`: CLI 2.1.233 przyjmuje tę nazwę w czasie wykonania, ale
/// **nie wymienia jej** we własnym komunikacie odrzucenia (`acceptEdits, auto,
/// bypassPermissions, manual, dontAsk, plan`), a dokumentacja nazywa `manual` jej aliasem
/// [T1 korekta 10]. Opieranie się na nazwie, której własne CLI nie przyznaje, to jedna wersja
/// od cichego „unknown option".
///
/// Cicha wersja złamania niezmiennika 23 nie wygląda jak drugi adapter — wygląda jak
/// `if agent == "claude" { … }` w miejscu wywołania, i tak właśnie po cichu umarło skanowanie
/// sekretów w repo źródłowym [raport 05 §4].
const fn permission_flags(policy: Policy) -> (&'static str, Option<&'static str>) {
    match policy {
        Policy::ReadOnly => ("dontAsk", Some("Read,Grep,Glob")),
        // `Bash(git *)` to git i **tylko** git; gołe `Bash` byłoby każdą komendą na maszynie.
        Policy::EditInFolder => ("acceptEdits", Some("Read,Grep,Glob,Edit,Write,Bash(git *)")),
        Policy::Unrestricted => ("bypassPermissions", None),
    }
}

/// Sterownik `claude`.
///
/// Ścieżka do binarki jest **polem**, nie stałą, i to jest jedyny szew, przez który kryteria
/// AC-6 i AC-7 wpuszczają skrypt-atrapę zamiast prawdziwego CLI. Atrapa loguje **obok
/// siebie**, nigdy przez zmienną środowiskową: supervisor robi `env_clear()`, więc fikstura
/// sterowana envem po cichu przestałaby działać.
#[derive(Debug, Clone)]
pub struct ClaudeDriver {
    /// Co uruchamiamy.
    binary: PathBuf,
}

impl Default for ClaudeDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeDriver {
    /// Sterownik wołający `claude` z `PATH`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            binary: PathBuf::from(DEFAULT_BINARY),
        }
    }

    /// Sterownik wołający konkretny plik. Szew dla kryteriów, które uruchamiają prawdziwy
    /// proces — i dla użytkownika, który trzyma CLI poza `PATH`.
    #[must_use]
    pub fn with_binary(binary: PathBuf) -> Self {
        Self { binary }
    }

    /// Buduje komendę jednej tury. **Promptu w niej nie ma i nigdy nie będzie**
    /// (niezmiennik 9): treść zadania jedzie kopertą na stdin, bo argumenty widzi `ps`
    /// każdego użytkownika maszyny.
    ///
    /// Linia argv w wersji wiążącej [T1 §8.3, `docs/ARCHITECTURE.md` §4]:
    ///
    /// | Fragment | Dlaczego dokładnie tak |
    /// |---|---|
    /// | `-p` | brama do wszystkiego poniżej |
    /// | `--output-format stream-json` | zdarzenia, nie bajty terminala |
    /// | `--input-format stream-json` | dwukierunkowy stdin: proces zostaje na wiele tur |
    /// | `--verbose` | bez niej CLI **odmawia**: `Error: When using --print, --output-format=stream-json requires --verbose` [T1 §3.1] |
    /// | `--session-id <run_id>` \| `--resume <id>` | dokładnie jedno z dwóch, nigdy oba |
    /// | `--strict-mcp-config` | 73 narzędzia z 9 serwerów zostają za drzwiami [T1 korekta 4] |
    /// | `--setting-sources ""` | argument o **zerowej długości**; `"user,project"` w tym miejscu to izolacja, która nie działa |
    /// | `--permission-mode` + `--allowedTools` | z [`super::Policy`], jedną tabelą (niezmiennik 23) |
    ///
    /// Czego tu **nie ma**: `--bare` (wywala subskrypcję [T1 §3.3]), `--max-turns`
    /// i `--max-budget-usd` (spike S-2 nierozstrzygnięty [`docs/ARCHITECTURE.md` §11]).
    #[must_use]
    pub fn command(&self, spec: &RunSpec) -> Command {
        let mut command = Command::new(&self.binary);

        // Katalog roboczy przychodzi ARGUMENTEM, nigdy stałą: literał ze ścieżką repo w pliku
        // pod `engine/` przewraca granicę z niezmiennika 1, bo `checks/quick-boundary.sh`
        // gerpuje `-i tauri` po niekomentowanych liniach, a każda nasza ścieżka zaczyna się
        // od `src-tauri/`.
        command.current_dir(&spec.cwd);

        command.args(TRANSPORT);

        // Dokładnie jedno z dwóch, nigdy oba: to są dwie różne sesje, a CLI musiałoby zgadnąć,
        // która wygrywa. Sesję świeżego biegu nadajemy MY, zanim proces wystartuje — dopiero
        // to znosi wyścig o to, pod jakim numerem zapisać krok [T1 §4.6, T7 §6.2].
        match &spec.resume {
            None => {
                command.arg("--session-id").arg(spec.run_id.to_string());
            }
            Some(session) => {
                command.arg("--resume").arg(&session.id);
            }
        }

        command.args(LEAN_CONTEXT);

        // Jedna tabela, jedno miejsce (niezmiennik 23). `None` znaczy „nie wysyłaj listy",
        // a nie „wyślij pustą": pusta lista i brak listy to dla CLI dwie różne rzeczy.
        let (mode, tools) = permission_flags(spec.policy);
        command.arg("--permission-mode").arg(mode);
        if let Some(tools) = tools {
            command.arg("--allowedTools").arg(tools);
        }

        if let Some(model) = &spec.model {
            command.arg("--model").arg(model);
        }

        // KONFIGURACJA agenta, nie treść zadania. Treść zadania w tym polu byłaby
        // niezmiennikiem 9 złamanym po cichu: stąd wchodzi do argv, a argv widzi `ps` każdego
        // użytkownika maszyny.
        if let Some(append) = &spec.system_append {
            command.arg("--append-system-prompt").arg(append);
        }

        for dir in &spec.extra_dirs {
            command.arg("--add-dir").arg(dir);
        }

        // Promptu tu nie ma i nigdy nie będzie (niezmiennik 9). Jedzie kopertą na stdin.
        //
        // Nie ma tu też `--bare` (nigdy nie czyta OAuth ani keychaina; na tej maszynie wywaliła
        // bieg na `Not logged in · Please run /login` z `terminal_reason:"api_error"`
        // [T1 §3.3, ran]), ani `--max-turns` / `--max-budget-usd` — spike S-2 nie rozstrzygnął
        // sprzeczności T1 vs T4, a sufit i tak egzekwuje limit czasu ściennego z T-03
        // [`docs/ARCHITECTURE.md` §11].
        command
    }
}

// ── Wire enum Claude ──────────────────────────────────────────────────────────────────────
//
// Kształt z drutu mieszka WYŁĄCZNIE tutaj. Powyżej tej linii nie ma ani jednego `serde`, poniżej
// nie ma ani jednego [`AgentEvent`] — to jest ten sam podział, dzięki któremu `CodexDriver`
// (T-10) powstaje bez dotykania `stream.rs` [PLAN §8, ryzyko 5].

/// Pole, którego kształt vendor może zmienić bez uprzedzenia.
///
/// Cokolwiek nie pasuje, znika jako `None` — zamiast wywalić **całą linię** do licznika śmieci.
/// To jest niezmiennik 5 w miejscu, w którym naprawdę się łamie: `#[serde(other)]` ratuje
/// nieznany `type`, ale nie ratuje znanego typu, któremu vendor zmienił kształt pola
/// zagnieżdżonego — a wtedy tracimy linię, która w 95% była dla nas czytelna.
fn lenient<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let value = Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).ok())
}

/// Jedna linia strumienia `stream-json` [T1 §8.5].
///
/// `#[serde(other)] Unknown` jest nienegocjowalny: vendorzy dokładają typy zdarzeń co tydzień,
/// po cichu, i bieg nie ma prawa na tym paść (niezmiennik 5). Sam ten atrybut jednak **nie
/// wystarcza** — decyduje to, że [`ClaudeDecoder::push`] nie zwraca `Result`, więc nie ma czego
/// przepuścić przez `?` w pętli czytającej.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClaudeLine {
    /// `init`, `thinking_tokens`, `api_retry`, haki — rozróżniane po `subtype`.
    System(SystemLine),
    /// Proza, myślenie i wywołania narzędzi.
    Assistant(TurnLine),
    /// Wyniki narzędzi wracające do modelu (i nasze koperty, gdyby ktoś włączył ich echo).
    User(TurnLine),
    /// Limit u dostawcy. Pola siedzą **zagnieżdżone** [T1 korekta 3].
    RateLimitEvent(RateLimitLine),
    /// Koniec tury. Dokładnie jedna na turę [T1 §4.4].
    Result(Box<ResultLine>),
    /// Wszystko, czego jeszcze nie znamy. Linia jest **rozpoznana**, tylko nic nie znaczy.
    #[serde(other)]
    Unknown,
}

/// Linia `system/*`. Każde pole opcjonalne, bo `init` z 2.1.233 ma ich dwadzieścia kilka,
/// a `hook_response` — pięć zupełnie innych [T1 §4.1, korekta 5].
#[derive(Debug, Deserialize)]
struct SystemLine {
    subtype: Option<String>,
    session_id: Option<String>,
    model: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    tools: Option<Vec<String>>,
    #[serde(default, deserialize_with = "lenient")]
    capabilities: Option<Vec<String>>,
    attempt: Option<u32>,
    max_retries: Option<u32>,
}

/// Linia `assistant` albo `user`: obie niosą wiadomość z blokami treści [T1 §4.2, §4.3].
#[derive(Debug, Deserialize)]
struct TurnLine {
    message: Option<TurnMessage>,
}

/// Wiadomość jednej strony rozmowy.
#[derive(Debug, Deserialize)]
struct TurnMessage {
    /// **Surowe** wartości, nie od razu `Vec<Block>`, i to jest różnica z pomiarem za sobą:
    /// jeden blok o nieoczekiwanym kształcie kosztowałby nas **wszystkie** bloki tej
    /// wiadomości, bo `Vec<T>` jest albo cały, albo wcale. Każdy blok czytamy z osobna
    /// w [`ClaudeDecoder::blocks`] (niezmiennik 5).
    #[serde(default, deserialize_with = "lenient")]
    content: Option<Vec<Value>>,
}

/// Blok treści wewnątrz wiadomości.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Block {
    /// Model myśli. Treści myślenia **nie czytamy** — nie wchodzi na transkrypt
    /// [`docs/ARCHITECTURE.md` §6, reguła 5].
    Thinking {},
    /// Proza, dosłownie.
    Text { text: Option<String> },
    /// Czynność narzędziem.
    ToolUse {
        id: Option<String>,
        name: Option<String>,
        #[serde(default, deserialize_with = "lenient")]
        input: Option<ToolInput>,
    },
    /// Wynik czynności.
    ToolResult {
        tool_use_id: Option<String>,
        content: Option<Value>,
        is_error: Option<bool>,
    },
    /// Blok, którego nie znamy.
    #[serde(other)]
    Unknown,
}

/// To, co nas interesuje w argumentach narzędzia.
#[derive(Debug, Deserialize)]
struct ToolInput {
    /// Etykieta po ludzku, **napisana przez sam model**. To jest prezent: dostajemy zdanie
    /// gotowe na ekran, za darmo i bez zgadywania [T1 §8.6, ran].
    description: Option<String>,
    file_path: Option<String>,
}

/// Linia `rate_limit_event`.
#[derive(Debug, Deserialize)]
struct RateLimitLine {
    /// Koperta, której raport T1 §4.5 **nie miał** — i to jest cała pułapka tego zdarzenia
    /// [T1 korekta 3]. Parser napisany pod kształt płaski deserializuje się bez błędu, nie
    /// widzi nic, banner się nie pokazuje i dowiadujesz się o tym z rachunku.
    #[serde(default, deserialize_with = "lenient")]
    rate_limit_info: Option<RateLimitInfo>,
}

/// Wnętrze koperty limitu. Klucze są tu `camelCase`, w odróżnieniu od reszty strumienia —
/// tak je wypisało CLI 2.1.233 i tak zostaje.
#[derive(Debug, Deserialize)]
struct RateLimitInfo {
    status: Option<String>,
    #[serde(rename = "resetsAt")]
    resets_at: Option<i64>,
    #[serde(rename = "rateLimitType")]
    rate_limit_type: Option<String>,
}

/// Linia `result` — jedyna, która kończy turę [T1 §4.4].
#[derive(Debug, Deserialize)]
struct ResultLine {
    /// **Nigdy nie rozstrzyga o powodzeniu.** Nieudany bieg przyszedł z `"subtype":"success"`
    /// przy `"is_error":true` [T1 §4.4, ran, potwierdzone ponownie]. Czytamy go wyłącznie po to,
    /// żeby odróżnić sufit tur (`error_max_*`) od reszty.
    subtype: Option<String>,
    /// To pole, a nie `subtype`, mówi, czy krok się udał.
    is_error: Option<bool>,
    terminal_reason: Option<String>,
    session_id: Option<String>,
    num_turns: Option<u32>,
    total_cost_usd: Option<f64>,
    duration_ms: Option<u64>,
    /// Ostatnia wypowiedź agenta — to, co krok przekazuje dalej.
    result: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    usage: Option<Usage>,
}

/// Zużycie kontekstu z drutu. Trzy pola z kilkunastu: reszta to statystyki, których nikt nie
/// czyta, a pole bez czytelnika jest zakazane (niezmiennik 21).
#[derive(Debug, Deserialize)]
struct Usage {
    #[serde(rename = "input_tokens")]
    input: Option<u64>,
    #[serde(rename = "output_tokens")]
    output: Option<u64>,
    /// Ta liczba, i tylko ta, mówi, czy izolacja kontekstu w ogóle działa [T1 §3.3].
    #[serde(rename = "cache_read_input_tokens")]
    cached: Option<u64>,
}

/// Dekoder jednego strumienia: linia tekstu → zero lub więcej [`AgentEvent`].
///
/// **`push` nie zwraca `Result` i to jest cały niezmiennik 5 w jednej sygnaturze.** Cicha
/// wersja złamania nie siedzi w typie — enum z `#[serde(other)]` ma wariant `Unknown` i to
/// nie pomaga — tylko w **pętli**: `let ev = serde_json::from_str(&line)?;` kończy krok na
/// pierwszej linii, która nie jest JSON-em, a vendorzy dokładają typy zdarzeń co tydzień, po
/// cichu [niezmiennik 5, T7 ryzyko 4]. Nieznaną linię logujemy i porzucamy; skoro nie da się
/// jej zwrócić jako błąd, nie da się na niej wywalić biegu.
///
/// Kształt wire enuma, który tu wejdzie [T1 §8.5]: `#[serde(tag = "type")]` z wariantem
/// `#[serde(other)] Unknown` i `Option<T>` na **każdym** polu, które nie jest niezbędne.
#[derive(Debug, Default)]
pub struct ClaudeDecoder {
    /// Ile linii nie dało się w ogóle sparsować. Rośnie tylko dla śmieci — linia z poprawnym
    /// JSON-em i nieznanym `type` jest **rozpoznana**, tylko nic nie znaczy.
    unparsed: usize,
    /// Sesja, którą CLI ogłosiło w `init` albo powtórzyło w `result`. Trzymamy ją, żeby
    /// zdarzenie końca miało czym się podpisać także wtedy, gdy strumień urwał się bez `result`.
    session: Option<String>,
    /// Czy któraś linia `result` już zamknęła turę. Po tym poznaje [`Self::end_of_stream`],
    /// że nie ma czego domykać.
    ended: bool,
    /// Wywołania narzędzi, które zapowiedziały zmianę pliku, czekające na swój wynik.
    ///
    /// [`AgentEvent::FileEdit`] mówi „agent **zmienił** plik" w czasie przeszłym, więc wolno go
    /// wypuścić dopiero, kiedy narzędzie się udało. Wpis znika przy wyniku niezależnie od tego,
    /// czy zmiana doszła do skutku — inaczej mapa rosłaby przez cały bieg.
    edits: HashMap<String, PathBuf>,
}

impl ClaudeDecoder {
    /// Świeży dekoder, przed pierwszą linią.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Wpuszcza jedną linię strumienia i oddaje zdarzenia, które z niej wynikają.
    ///
    /// Pusty wektor jest **normalną odpowiedzią**, nie sygnałem błędu: tak wyglądają
    /// `thinking_tokens`, hooki `SessionStart` i każdy typ zdarzenia, którego jeszcze nie
    /// znamy.
    pub fn push(&mut self, line: &str) -> Vec<AgentEvent> {
        let line = line.trim();
        if line.is_empty() {
            // Pusta linia nie jest śmieciem: NDJSON kończy się nią przy każdym normalnym
            // wyjściu, a licznik śmieci ma zostać liczbą, którą warto czytać.
            return Vec::new();
        }

        // Całe mapowanie linia → zdarzenia stoi w JEDNYM match, razem z gałęzią śmiecia: to jest
        // ta lista, którą czyta się, pytając „co ten sterownik w ogóle rozumie".
        match serde_json::from_str::<ClaudeLine>(line) {
            Err(error) => {
                self.unparsed += 1;
                // Treści linii tu nie ma, i to jest świadome: surowy strumień leży już na dysku
                // (tee z T-05), a dziennik aplikacji czyta się w zgłoszeniu błędu — nie ma
                // powodu, żeby druga kopia cudzego tekstu jechała jeszcze tędy.
                tracing::debug!(
                    bytes = line.len(),
                    %error,
                    "a line of the agent stream could not be read; dropping it"
                );
                Vec::new()
            }
            Ok(ClaudeLine::System(line)) => self.system(&line),
            Ok(ClaudeLine::Assistant(line) | ClaudeLine::User(line)) => self.blocks(line.message),
            Ok(ClaudeLine::RateLimitEvent(line)) => Self::rate_limit(&line),
            Ok(ClaudeLine::Result(line)) => vec![self.finish(&line)],
            // Nieznany typ jest ROZPOZNANY — linia się wczytała, tylko nic dla nas nie znaczy.
            // Liczenie jej jako śmiecia zasłoniłoby linie, które naprawdę były śmieciem.
            Ok(ClaudeLine::Unknown) => Vec::new(),
        }
    }

    /// `system/*` → zdarzenie albo cisza.
    ///
    /// Haki `SessionStart` są celowo niczym: pojawiają się nawet bez `--include-hook-events`
    /// i znikają pod `--setting-sources ""`, a użytkownikowi nie mówią nic [T1 §4.5, ran].
    fn system(&mut self, line: &SystemLine) -> Vec<AgentEvent> {
        match line.subtype.as_deref() {
            Some(INIT) => {
                if let Some(id) = &line.session_id {
                    self.session = Some(id.clone());
                }
                vec![AgentEvent::Started {
                    session: self.session_ref(line.session_id.as_deref()),
                    model: line.model.clone().unwrap_or_default(),
                    tools: line.tools.clone().unwrap_or_default(),
                    // Na TEJ liście, a nie na numerze wersji, feature-detektuje się przerwanie
                    // w paśmie [T1 §4.1, §4.6].
                    capabilities: line.capabilities.clone().unwrap_or_default(),
                }]
            }
            // Nigdy nie niesie tekstu: to jest stały slot na dole ekranu, nie wpis w historii
            // [`docs/ARCHITECTURE.md` §6, reguła 5].
            Some(THINKING_TOKENS) => vec![AgentEvent::Thinking],
            // 2026-08-15 — kształt tej linii jest [docs], nie [ran]: nie ma jej w fiksturze,
            // więc mapowanie zostaje możliwie głupie. Zdanie po angielsku, nigdy `api_retry`
            // na ekranie (niezmiennik 14).
            Some(API_RETRY) => vec![AgentEvent::Notice {
                text: retry_sentence(line.attempt, line.max_retries),
            }],
            _ => Vec::new(),
        }
    }

    /// Bloki treści jednej wiadomości → zdarzenia.
    ///
    /// Każdy blok czytany **osobno**: jeden blok o nieznanym kształcie nie ma prawa kosztować
    /// nas pozostałych (niezmiennik 5).
    fn blocks(&mut self, message: Option<TurnMessage>) -> Vec<AgentEvent> {
        let Some(blocks) = message.and_then(|message| message.content) else {
            return Vec::new();
        };

        let mut events = Vec::new();
        for raw in blocks {
            match serde_json::from_value::<Block>(raw).unwrap_or(Block::Unknown) {
                Block::Thinking {} => events.push(AgentEvent::Thinking),
                Block::Text { text } => {
                    let text = text.unwrap_or_default();
                    if !text.trim().is_empty() {
                        events.push(AgentEvent::Said { text });
                    }
                }
                Block::ToolUse { id, name, input } => {
                    let id = id.unwrap_or_default();
                    let name = name.unwrap_or_default();
                    events.push(AgentEvent::ToolStart {
                        id: id.clone(),
                        label: tool_label(&name, input.as_ref()),
                    });
                    if let Some(path) = editing_path(&name, input) {
                        self.edits.insert(id, path);
                    }
                }
                Block::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => {
                    let id = tool_use_id.unwrap_or_default();
                    let ok = !is_error.unwrap_or(false);
                    let edited = self.edits.remove(&id);
                    events.push(AgentEvent::ToolEnd {
                        id,
                        ok,
                        summary: summarise(content.as_ref()),
                    });
                    if ok && let Some(path) = edited {
                        events.push(AgentEvent::FileEdit { path });
                    }
                }
                Block::Unknown => {}
            }
        }
        events
    }

    /// `rate_limit_event` → zdarzenie limitu, albo nic.
    ///
    /// **Nic, kiedy brakuje którejkolwiek z trzech wartości** — i to jest cały sens tego
    /// kryterium. Zdarzenie z `resets_at == 0` mówi „limit wraca o 01:00 czasu uniksowego 1970",
    /// czyli wygląda jak odpowiedź; brak bannera przynajmniej nie kłamie [T1 korekta 3].
    fn rate_limit(line: &RateLimitLine) -> Vec<AgentEvent> {
        let Some(info) = &line.rate_limit_info else {
            tracing::debug!("a rate limit line arrived without its envelope; dropping it");
            return Vec::new();
        };
        let (Some(status), Some(resets_at), Some(window)) = (
            info.status.as_deref(),
            info.resets_at,
            info.rate_limit_type.as_deref(),
        ) else {
            tracing::debug!("a rate limit line arrived half-filled; dropping it");
            return Vec::new();
        };

        vec![AgentEvent::RateLimit {
            status: status.to_owned(),
            resets_at,
            rate_limit_type: window.to_owned(),
            // Cokolwiek innego niż „allowed" znaczy, że nie ma już czego wysyłać, więc bieg ma
            // stanąć zamiast palić tury na odmowach. Samą pauzę robi T-21.
            pause_run: status != ALLOWED,
        }]
    }

    /// Linia `result` → koniec tury.
    ///
    /// **`subtype` nie rozstrzyga o niczym poza sufitem tur.** Nieudany bieg przyszedł
    /// z `"subtype":"success"` przy `"is_error":true` i `"terminal_reason":"api_error"`
    /// [T1 §4.4, ran]. Sterownik czytający `subtype` melduje sukces kroku, który nie zrobił nic,
    /// a stożek poniżej rusza na pustym przekazaniu.
    fn finish(&mut self, line: &ResultLine) -> AgentEvent {
        self.ended = true;
        if let Some(id) = &line.session_id {
            self.session = Some(id.clone());
        }

        // Brak `is_error` nie jest obietnicą sukcesu: kiedy vendor go nie dosłał, pytamy
        // `subtype`, bo to jedyne, co zostało. Kiedy dosłał — `subtype` nie ma tu głosu.
        let failed = line.is_error.unwrap_or_else(|| {
            line.subtype
                .as_deref()
                .is_some_and(|subtype| subtype.starts_with(ERROR_PREFIX))
        });

        let reason = if !failed {
            FinishReason::Completed
        } else if line.terminal_reason.as_deref() == Some(CANCELLED) {
            // Anulowanie jest wartością, nie błędem (niezmiennik 7): krok, który ktoś zatrzymał
            // celowo, nie ma prawa czytać się tak samo jak krok, który się zepsuł.
            FinishReason::Cancelled
        } else if line
            .subtype
            .as_deref()
            .is_some_and(|subtype| subtype.starts_with(CEILING_PREFIX))
        {
            FinishReason::LimitReached
        } else {
            FinishReason::Failed(failure_sentence(line))
        };

        let usage = line.usage.as_ref();
        AgentEvent::Finished(Outcome {
            ok: !failed,
            reason,
            text: line.result.clone().unwrap_or_default(),
            // `None`, nie zero: zero jest liczbą i sumuje się w rachunek, którego nikt nie
            // zamawiał.
            cost_usd: line.total_cost_usd,
            tokens: Tokens {
                input: usage.and_then(|usage| usage.input).unwrap_or_default(),
                output: usage.and_then(|usage| usage.output).unwrap_or_default(),
                cached: usage.and_then(|usage| usage.cached).unwrap_or_default(),
            },
            turns: line.num_turns.unwrap_or_default(),
            took: Duration::from_millis(line.duration_ms.unwrap_or_default()),
            session: self.session_ref(line.session_id.as_deref()),
        })
    }

    /// Sesja tej rozmowy: to, co powiedziała linia, a w drugiej kolejności to, co pamiętamy.
    fn session_ref(&self, from_line: Option<&str>) -> SessionRef {
        SessionRef {
            vendor: VENDOR,
            id: from_line
                .map(str::to_owned)
                .or_else(|| self.session.clone())
                .unwrap_or_default(),
        }
    }

    /// Ile linii dekoder porzucił jako niesparsowalne. To jest licznik do pliku debug
    /// i do zgłoszenia błędu, a nie powód, żeby zatrzymać bieg.
    #[must_use]
    pub fn unparsed(&self) -> usize {
        self.unparsed
    }

    /// Domyka turę, kiedy strumień się skończył. `exit_code` jest sygnałem **drugorzędnym**
    /// [T1 §8.5].
    ///
    /// Zwraca [`AgentEvent::Finished`] tylko wtedy, gdy linia `result` **nie przyszła** —
    /// bo wtedy nikt inny go nie wypuści, a krok bez zdarzenia końca wisiałby w `running` do
    /// końca biegu. Strumień zakończony kodem 0 bez `result` jest **niepowodzeniem**, nie
    /// sukcesem: proces, który wyszedł czysto i nie powiedział, co zrobił, nie ma czego
    /// przekazać dalej.
    pub fn end_of_stream(&mut self, exit_code: Option<i32>) -> Option<AgentEvent> {
        if self.ended {
            return None;
        }
        self.ended = true;

        // Kod wyjścia jest w tym zdaniu opisem, nie dowodem: proces, który wyszedł czysto i nie
        // powiedział, co zrobił, nie ma czego przekazać dalej [T1 §8.5].
        let why = match exit_code {
            Some(code) => format!("The agent exited with code {code} and never sent its result."),
            None => "The agent stopped without ever sending its result.".to_owned(),
        };

        Some(AgentEvent::Finished(Outcome {
            ok: false,
            reason: FinishReason::Failed(why),
            text: String::new(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 0,
            took: Duration::ZERO,
            session: self.session_ref(None),
        }))
    }
}

/// Zdanie, które ląduje w [`FinishReason::Failed`], czyli **na ekranie**.
///
/// Najpierw własna wypowiedź agenta: to ona odpowiada na pytanie „dlaczego", które ktoś zaraz
/// zada. Dopiero kiedy jej nie ma, tłumaczymy enum z drutu na angielskie zdanie — `api_error`
/// samo w sobie nie ma prawa dojechać na ekran (niezmiennik 14).
fn failure_sentence(line: &ResultLine) -> String {
    if let Some(text) = line.result.as_deref()
        && !text.trim().is_empty()
    {
        return first_line(text);
    }
    match line.terminal_reason.as_deref() {
        Some("api_error") => "The model provider returned an error.".to_owned(),
        Some("timeout") => "The agent ran out of time.".to_owned(),
        _ => "The agent stopped before it finished.".to_owned(),
    }
}

/// Zdanie o ponowieniu zapytania. Liczby wchodzą tylko wtedy, gdy vendor je podał.
fn retry_sentence(attempt: Option<u32>, max_retries: Option<u32>) -> String {
    match (attempt, max_retries) {
        (Some(attempt), Some(max)) => format!("Retrying — try {attempt} of {max}."),
        (Some(attempt), None) => format!("Retrying — try {attempt}."),
        _ => "Retrying.".to_owned(),
    }
}

/// Etykieta czynności, gotowa na ekran.
///
/// Pierwszy wybór to zawsze `description`: model pisze ją sam, po ludzku, i to jest najlepszy
/// tekst, jaki tu w ogóle może być [T1 §8.6, ran]. Reszta to zapasowe trzy czasowniki — a że
/// **kuracja należy do T-05**, nie zgadujemy tu niczego więcej.
fn tool_label(name: &str, input: Option<&ToolInput>) -> String {
    if let Some(description) = input.and_then(|input| input.description.as_deref())
        && !description.trim().is_empty()
    {
        return first_line(description);
    }

    let target = input
        .and_then(|input| input.file_path.as_deref())
        .map(file_name);
    match (verb_for(name), target) {
        (verb, Some(target)) => format!("{verb} {target}"),
        (verb, None) => verb.to_owned(),
    }
}

/// Czasownik dla rodziny narzędzi [T1 §8.6].
fn verb_for(name: &str) -> &'static str {
    match name {
        "Read" | "Grep" | "Glob" | "NotebookRead" => "Reading",
        "Edit" | "Write" | "NotebookEdit" => "Editing",
        "Bash" | "BashOutput" => "Running a command",
        // Narzędzia, których nie znamy — a jest ich siedemdziesiąt kilka i przybywa co tydzień.
        // Nazwa własna narzędzia jest tu jedyną prawdą, jaką mamy.
        _ => "Working",
    }
}

/// Ścieżka, którą to wywołanie zmieni — o ile w ogóle coś zmienia.
fn editing_path(name: &str, input: Option<ToolInput>) -> Option<PathBuf> {
    if verb_for(name) != "Editing" {
        return None;
    }
    input
        .and_then(|input| input.file_path)
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
}

/// Sama nazwa pliku: pełna ścieżka w etykiecie to trzy czwarte linii zjedzone przez katalogi,
/// których użytkownik nie wybierał.
fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Jednolinijkowe podsumowanie wyniku narzędzia. Pełne wyjście zostaje za kliknięciem (T-05).
fn summarise(content: Option<&Value>) -> String {
    let text = match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    };
    first_line(&text)
}

/// Pierwsza niepusta linia, przycięta do długości, która mieści się w wierszu.
fn first_line(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    if line.chars().count() > SUMMARY_LIMIT {
        line.chars().take(SUMMARY_LIMIT).collect::<String>() + "…"
    } else {
        line.to_owned()
    }
}

// ── Koperta wiadomości ────────────────────────────────────────────────────────────────────

/// Jedna linia stdinu: `{"type":"user","message":{"role":"user","content":[{"type":"text",…}]}}`
/// [T1 §4.6, ran].
///
/// Tędy — i **wyłącznie tędy** — jedzie treść zadania (niezmiennik 9). Cicha wersja złamania nie
/// wygląda jak prompt w argv: wygląda jak `--append-system-prompt` z wklejoną treścią zadania,
/// a argumenty widzi `ps` każdego użytkownika maszyny.
#[derive(Debug, Serialize)]
struct Envelope<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    message: EnvelopeMessage<'a>,
}

/// Wiadomość w kopercie.
#[derive(Debug, Serialize)]
struct EnvelopeMessage<'a> {
    role: &'static str,
    content: [EnvelopeBlock<'a>; 1],
}

/// Jedyny blok treści koperty.
#[derive(Debug, Serialize)]
struct EnvelopeBlock<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    text: &'a str,
}

/// Buduje kopertę jednej tury — **jedna linia**, bo CLI czyta stdin linia po linii.
///
/// Serializujemy zamiast sklejać stringi: prompt z cudzysłowem albo znakiem nowej linii,
/// wklejony ręcznie, rozjeżdża linię JSON i cała tura ginie na parsowaniu po drugiej stronie.
fn user_envelope(text: &str) -> serde_json::Result<String> {
    serde_json::to_string(&Envelope {
        kind: "user",
        message: EnvelopeMessage {
            role: "user",
            content: [EnvelopeBlock { kind: "text", text }],
        },
    })
}

// ── Pętla czytająca ───────────────────────────────────────────────────────────────────────

/// Czyta stdout linia po linii i sypie zdarzeniami, aż do końca strumienia.
///
/// **Nie ma tu `?` i to nie jest przeoczenie** (niezmiennik 5): jedyny sposób, żeby nieznana
/// linia zabiła bieg, to zwrócić z tej pętli błąd. Dekoder oddaje pusty wektor, a pętla leci
/// dalej.
///
/// Tee surowego `agent-<id>.jsonl` na dysk i kuracja zdarzenie → linia należą do T-05; tutaj
/// jest tylko to, co bez procesu nie ma sensu.
async fn pump(
    stdout: ChildStdout,
    events: mpsc::Sender<AgentEvent>,
    outcomes: mpsc::Sender<Outcome>,
) {
    let mut lines = BufReader::new(stdout).lines();
    let mut decoder = ClaudeDecoder::new();

    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                for event in decoder.push(&line) {
                    emit(event, &events, &outcomes).await;
                }
            }
            Ok(None) => break,
            Err(error) => {
                tracing::debug!(%error, "the agent output stream broke off");
                break;
            }
        }
    }

    // Kod wyjścia jest sygnałem drugorzędnym i tu go nie mamy: uchwyt procesu został przy
    // wołającym, a strumień skończył się przed nim. Zdarzenie końca musi paść mimo to, inaczej
    // krok wisi w `running` do końca biegu [T1 §8.5].
    if let Some(event) = decoder.end_of_stream(None) {
        emit(event, &events, &outcomes).await;
    }

    // Oba nadajniki giną RAZEM Z TĄ PĘTLĄ i to jest ich druga robota: zamknięty kanał wyników
    // jest jedynym sygnałem, po którym `wait()` wie, że nic już nie przyjdzie. Bez tego czekanie
    // na turę, która nigdy się nie skończy, jest nieodróżnialne od czekania na turę, która trwa.
    drop(events);
    drop(outcomes);
}

/// Wypuszcza jedno zdarzenie — **najpierw** do [`AgentHandle::wait`], potem na ekran.
///
/// Ta kolejność jest jedyną obroną przed wolnym konsumentem: kanał zdarzeń z pełnym buforem
/// zatrzymuje wysyłkę, a wynik tury, który utknął za nim, wygląda jak zawieszony agent.
/// Odwrotna kolejność kosztowałaby dokładnie to [T1 „Worth adding": wolny konsument opóźnia
/// wyjście do 30 s].
async fn emit(
    event: AgentEvent,
    events: &mpsc::Sender<AgentEvent>,
    outcomes: &mpsc::Sender<Outcome>,
) {
    if let AgentEvent::Finished(outcome) = &event {
        let _ = outcomes.send(outcome.clone()).await;
    }
    // Zamknięty kanał zdarzeń nie kończy pętli: nikt już nie patrzy na ekran, ale wynik tury
    // nadal ma dojść tam, gdzie ktoś na niego czeka.
    let _ = events.send(event).await;
}

/// Żywa sesja `claude` — jeden proces, wiele tur.
#[derive(Debug)]
pub struct ClaudeHandle {
    /// Sesja, którą sami nadaliśmy przed startem [T7 §6.2].
    session: SessionRef,
    /// Proces sesji, razem z całą eskalacją zabijania i dowodem z T-03. Grupa procesów jest
    /// jego polem, a nie kopią tutaj: dwie kopie tego samego faktu rozjeżdżają się dokładnie
    /// w chwili, w której zaczyna on być ciekawy.
    process: Supervised,
    /// Wyniki tur, w kolejności, w jakiej padły. Osobno od kanału zdarzeń, bo `wait()` musi je
    /// dostać także wtedy, gdy nikt nie czyta ekranu.
    outcomes: mpsc::Receiver<Outcome>,
}

#[async_trait]
impl AgentHandle for ClaudeHandle {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        // Zawsze `Some`: ten sterownik trzyma proces przez całą sesję, więc nie ma chwili
        // „między turami", w której nie byłoby czego zabić. Czyta to T-06 (zapis `pid`/`pgid`
        // przy kroku) i T-20 (sprzątanie po awarii aplikacji).
        Some(self.process.group())
    }

    /// Kolejna tura **tym samym procesem**: koperta na stdin, stdin zostaje otwarty.
    ///
    /// Koperta, jedna linia JSON [T1 §4.6]:
    /// `{"type":"user","message":{"role":"user","content":[{"type":"text","text":"…"}]}}`
    ///
    /// # Ta metoda nie ma ciała i to jest ZGŁOSZENIE, nie niedopatrzenie (2026-08-15)
    ///
    /// Druga tura wymaga uchwytu do stdinu procesu, który żyje dłużej niż jeden zapis. Cały
    /// start procesu w tym repo idzie przez `engine::supervisor::spawn`, a ono zna dwa plany
    /// stdinu: `Null` (`/dev/null`) i `Write(String)` — jeden zapis, po którym zadanie
    /// piszące **porzuca potok**, czyli zamyka deskryptor i daje dziecku EOF. Trzeciego planu
    /// („pisz i zostaw otwarte") nie ma, a `Supervised` nie wystawia potoku wejściowego żadną
    /// metodą; pole `child` jest prywatne dla modułu `supervisor`, więc `impl` w tym pliku go
    /// nie widzi.
    ///
    /// Ominięcie tego bez dotknięcia `supervisor.rs` znaczyłoby wystartować proces samemu —
    /// czyli własną grupę procesów i własną eskalację sygnałów w tym pliku, co łamie
    /// niezmiennik 3 dokładnie tak, jak opisuje go `supervisor.rs`: „`libc::SIGTERM`
    /// zaimportowany »na chwilę« w pliku wywołującym łamie niezmiennik 3 po cichu, bo w diffie
    /// wygląda jak zwykły `use`".
    ///
    /// `src-tauri/src/engine/supervisor.rs` **nie leży w bloku OWNS tego zadania**, więc to
    /// jest pytanie do człowieka (`AGENTS.md` §7), nie cichy dopisek. Brakuje jednego wariantu
    /// i jednego akcesora; szczegóły w komentarzu przy [`AgentDriver::start`].
    async fn send(&mut self, text: String) -> anyhow::Result<()> {
        Err(anyhow!(
            "a follow-up turn of {} bytes has nowhere to go: session {} was started with a stdin \
             that closes after the first envelope, and keeping it open needs a plan this file is \
             not allowed to add to the supervisor",
            text.len(),
            self.session.id
        ))
    }

    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        self.outcomes.recv().await.ok_or_else(|| {
            anyhow!(
                "session {} ended without ever saying how the turn went",
                self.session.id
            )
        })
    }

    /// Trzy stopnie, w tej kolejności i nigdy krócej [T1 §8.5].
    ///
    /// 1. **Tylko** jeśli `init` ogłosił `interrupt_receipt_v1`: `control_request` z podtypem
    ///    `interrupt` na stdin i czekanie ≤5 s. Sesja zostaje wznawialna [T1 §4.6]. Wysłanie
    ///    go tam, gdzie CLI go nie obsługuje, kończy się pięcioma sekundami czekania na
    ///    odpowiedź, która nie przyjdzie — dlatego zdolność, a nie numer wersji [T1 §4.1].
    /// 2. Inaczej, albo po upływie tego okna: SIGTERM na **grupę**. `claude` dosypuje wtedy
    ///    transkrypt, zwalnia zamek sesji i odpala hooki `SessionEnd`, wychodząc 143.
    /// 3. Po oknie łaski: SIGKILL na grupę i **pętla dowodowa**, aż `kill(-pgid, 0)` odpowie
    ///    `ESRCH`. Oba ostatnie kroki to gotowa ścieżka z T-03 — ten plik nie ma prawa znać
    ///    ani jednej stałej sygnału (niezmiennik 3).
    ///
    /// Kiedy proces wyszedł **sam** po przerwaniu, status w dowodzie jest jego własnym kodem
    /// wyjścia, nie sygnałem. To jest jedyny obserwowalny ślad różnicy między wznawialną
    /// sesją a zabitą.
    ///
    /// # Stopnia pierwszego tu nie ma i to jest to samo zgłoszenie (2026-08-15)
    ///
    /// `control_request` jedzie **na stdin**, a stdin tej sesji jest zamknięty od pierwszej
    /// koperty — powód stoi przy [`AgentHandle::send`]. Zdolność `interrupt_receipt_v1`
    /// przychodzi z `init` i jest już dekodowana ([`AgentEvent::Started`]), więc feature-detekcja
    /// ma z czego działać; brakuje wyłącznie kanału, którym pytanie miałoby wyjść.
    ///
    /// Czego tu **nie zrobiono zamiast tego**: nie wysyłamy przerwania „na ślepo" i nie
    /// prowadzimy dziewiątką. Pierwsze byłoby pięcioma sekundami czekania na odpowiedź, która
    /// nie przyjdzie, tam gdzie CLI tego nie obsługuje; drugie kosztowałoby wznawialność sesji,
    /// dosypanie transkryptu i hooki `SessionEnd` [T1 §4.6]. Zostają stopnie dwa i trzy, w całości
    /// z T-03: SIGTERM na grupę, okno łaski, SIGKILL i pętla dowodowa aż do `ESRCH`.
    async fn cancel(&mut self) -> GroupProof {
        self.process.stop(DEFAULT_GRACE).await
    }

    /// Koniec **sesji**, nie tury: dziecko dostaje EOF i wychodzi samo.
    ///
    /// Bez tego czasownika każdy skończony krok zostawiałby żywy proces — `claude` z otwartym
    /// stdinem czeka w nieskończoność [T1 §2]. Tu zostało z tego samo czekanie: stdin zamknął
    /// się już przy starcie (powód przy [`AgentHandle::send`]), więc nie ma czego domykać,
    /// a proces i tak wychodzi sam.
    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        // `None` znaczy „proces zginął od sygnału i kodu po prostu nie ma" — to jest ta sama
        // różnica, którą mierzy dowód z `cancel()`.
        Ok(self.process.wait().await?.code())
    }
}

#[async_trait]
impl AgentDriver for ClaudeDriver {
    fn id(&self) -> &'static str {
        VENDOR
    }

    /// Pyta binarkę o wersję. **Brak pliku to `Ok(Probe { found: false, .. })`, nigdy `Err`**:
    /// nieobecne CLI jest ekranem ustawień, a nie awarią startu aplikacji.
    ///
    /// Nieudany start jest tu odpowiedzią w **każdej** postaci, nie tylko przy braku pliku:
    /// binarka bez prawa wykonania i binarka, której nie ma, znaczą dla użytkownika dokładnie
    /// to samo zdanie („zainstaluj to"), a `Err` z tego miejsca wywala Loadouta, zanim
    /// ktokolwiek zobaczy, co jest do naprawienia.
    async fn probe(&self) -> anyhow::Result<Probe> {
        let mut command = Command::new(&self.binary);
        command.arg("--version");

        // Przez ten sam spawn co bieg, a nie własną komendą obok: `env_clear()` plus jawna lista
        // przepuszczanych zmiennych mieszka w jednym rdzeniu (niezmiennik 23), a `/dev/null` na
        // stdinie oszczędza tu 3 s ostrzeżenia `no stdin data received` [T1 §4.6].
        let mut process = match supervisor::spawn(command, StdinPlan::Null) {
            Ok(process) => process,
            Err(error) => {
                tracing::debug!(
                    binary = %self.binary.display(),
                    %error,
                    "the agent CLI could not be started, so the setup screen has its answer"
                );
                return Ok(Probe {
                    found: false,
                    version: None,
                });
            }
        };

        let mut version = None;
        if let Some(stdout) = process.stdout() {
            version = first_answer(stdout).await;
        }

        // Zebranie procesu jest częścią jego uruchomienia, nie sprzątaniem po nim: zombie nadal
        // odpowiada na sygnał zerowy, więc niezebrany `--version` zostawiłby grupę, której nikt
        // nigdy nie udowodni martwej (niezmiennik 6).
        let _ = process.wait().await;

        Ok(Probe {
            found: true,
            version,
        })
    }

    /// Startuje sesję i zaczyna sypać zdarzeniami na `tx`.
    ///
    /// Kolejność jest wymuszona przez odzyskiwanie po awarii: sesję nadajemy **przed**
    /// startem, `pid` i `pgid` są znane **zanim** cokolwiek zostanie przeczytane ze stdout
    /// [T7 §6.2]. Prompt wchodzi pierwszą kopertą na stdin — nigdy w argv (niezmiennik 9).
    ///
    /// # Stdin zamyka się po tej kopercie — to jest ZGŁOSZENIE (2026-08-15)
    ///
    /// Docelowo stdin **zostaje otwarty**, a zamknięcie go jest osobnym czasownikiem
    /// ([`AgentHandle::close`]), bo znaczy „koniec sesji", a nie „koniec tury". To jest cała
    /// różnica między jednym procesem na sesję a wariantem awaryjnym B z T1 §8.1 (nowy proces
    /// na turę z `--resume`), który płaci zimny start i odbudowę cache'u przy **każdej** turze.
    ///
    /// `engine::supervisor::spawn` — jedyna droga do procesu we własnej grupie w tym repo —
    /// zna dwa plany stdinu: `Null` i `Write(String)`. Ten drugi pisze raz i **porzuca potok**,
    /// czyli zamyka deskryptor. Trzeciego planu nie ma, a `Supervised` nie oddaje potoku
    /// wejściowego żadną metodą. Do domknięcia AC-6 i AC-7 brakuje w `supervisor.rs` jednego
    /// wariantu (`StdinPlan::Keep`, albo `Write` bez zamknięcia) i jednego akcesora
    /// (`Supervised::stdin() -> Option<ChildStdin>`) — razem kilkanaście wierszy w pliku, który
    /// **nie leży w bloku OWNS tego zadania**. `AGENTS.md` §7: to jest pytanie do człowieka,
    /// a nie cichy dopisek do cudzego pliku, i nie jest to też powód, żeby wystartować proces
    /// obok supervisora i wnieść stałe sygnałów do tego pliku (niezmiennik 3).
    ///
    /// Do tego czasu sesja obsługuje **jedną** turę: prompt wchodzi kopertą, zdarzenia płyną,
    /// wynik pada, anulowanie przechodzi pełną eskalacją z T-03. Druga tura zwraca błąd, który
    /// mówi dokładnie to samo co ten akapit ([`AgentHandle::send`]).
    async fn start(
        &self,
        spec: RunSpec,
        tx: mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        // Sesję nadajemy PRZED startem procesu: dopiero to znosi wyścig o to, pod jakim numerem
        // zapisać krok, i dopiero to czyni odzyskiwanie po awarii możliwym [T7 §6.2].
        let session = SessionRef {
            vendor: VENDOR,
            id: spec
                .resume
                .as_ref()
                .map_or_else(|| spec.run_id.to_string(), |session| session.id.clone()),
        };

        let envelope = user_envelope(&spec.prompt)?;
        let mut process = supervisor::spawn(
            self.command(&spec),
            // Prompt wyłącznie tędy (niezmiennik 9). Znak nowej linii jest częścią protokołu:
            // CLI czyta stdin linia po linii i bez niego czekałoby na resztę koperty.
            StdinPlan::Write(format!("{envelope}\n")),
        )?;

        let stdout = process
            .stdout()
            .ok_or_else(|| anyhow!("the agent started without an output stream to read"))?;

        let (finished, outcomes) = mpsc::channel(TURNS_IN_FLIGHT);
        // Pętla czytająca żyje własnym zadaniem: uchwyt ma zostać responsywny na `cancel()`
        // także wtedy, gdy nikt nie woła `wait()`.
        let _reader = tokio::spawn(pump(stdout, tx, finished));

        Ok(Box::new(ClaudeHandle {
            session,
            process,
            outcomes,
        }))
    }
}

/// Pierwsza niepusta linia, jaką powiedziała binarka. Tyle wystarczy na pytanie o wersję.
async fn first_answer(stdout: ChildStdout) -> Option<String> {
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim();
        if !line.is_empty() {
            return Some(line.to_owned());
        }
    }
    None
}
