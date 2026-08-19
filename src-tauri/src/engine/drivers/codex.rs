//! `CodexDriver` — nowy proces na turę, `thread_id` jako uchwyt wznowienia.
//!
//! Codex łamie dokładnie tę część kontraktu, którą `claude` spełnia za darmo: nie ma trybu
//! dwukierunkowego, więc każda tura to **nowy proces** z `codex exec resume` [T1 §6.4]. Cała ta
//! różnica ma zostać po tej stronie traitu — jeżeli wyjdzie na wierzch, to znaczy, że
//! `AgentDriver` jest fikcją, a nie abstrakcją, i to jest **wynik badania, nie porażka do
//! ukrycia** [PLAN §8, założenie 5].
//!
//! # Stan tego pliku: SZKIELET (2026-08-19)
//!
//! Ciała zwracają **świadomie złą wartość**: pustą listę argumentów, pustą sesję, zero
//! zdarzeń, `GroupProof::Alive` („nie mam dowodu śmierci", niezmiennik 6) i `Err` tam, gdzie
//! tura miałaby powiedzieć, jak poszła. To jest wymagany kształt fazy kontraktu: kryterium ma
//! się **skompilować** i paść w czasie wykonania, na braku ZACHOWANIA — test, który się nie
//! kompiluje, niczego nie uruchomił (`AGENTS.md` §2a p. 5). Żadna z tych wartości nie
//! przechodzi żadnego z sześciu kryteriów; przy każdym ciele stoi osobno, dlaczego.
//!
//! `todo!()` tu nie ma i nie będzie: `clippy::todo` jest `deny` w `[workspace.lints]`, więc
//! szkielet z nim nie przeszedłby nawet `./verify.sh quick` — a wtedy faza kontraktu kończy się
//! na czerwieni, która nie mówi nic o kryteriach.
//!
//! # Czego ten plik nie ma prawa zawierać
//!
//! Zero `#[cfg(unix)]`, zero `libc`, zero stałych sygnałów: zabijanie grupy i dowód jej śmierci
//! należą do `engine/supervisor.rs` (niezmiennik 3, egzekwuje `checks/quick-boundary.sh`).
//! `cancel()` ma z tamtej eskalacji **korzystać**, nie powtarzać jej trzema linijkami obok —
//! bo wtedy port na Windows przestaje być gałęzią `cfg`, a staje się przepisaniem.
//!
//! Nie ma tu też ani jednego `tauri::*` (niezmiennik 1): sterownik nie wie, że istnieje okno.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;

use super::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use crate::engine::supervisor::{GroupId, GroupProof};

/// Etykieta tego vendora — ta sama w [`SessionRef::vendor`] i w [`AgentDriver::id`].
///
/// To ona ląduje w bazie przy kroku (T-06) i po niej wznowienie wie, do którego CLI wrócić.
pub const VENDOR: &str = "codex";

/// Czym woła się CLI, kiedy nikt nie podał własnej ścieżki. Gołe „codex", nie ścieżka
/// bezwzględna: znajduje się przez `PATH`, a `PATH` jest jedną ze zmiennych, które supervisor
/// przepuszcza przez `env_clear()`.
const DEFAULT_BINARY: &str = "codex";

/// Sterownik `codex`.
///
/// Ścieżka do binarki jest **polem**, nie stałą, i to jest jedyny szew, przez który kryteria
/// wpuszczają skrypt-atrapę zamiast prawdziwego CLI — inaczej żadnego z nich nie dałoby się
/// uruchomić bez konta i bez sieci.
#[derive(Debug, Clone)]
pub struct CodexDriver {
    /// Co uruchamiamy.
    binary: PathBuf,
}

impl Default for CodexDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl CodexDriver {
    /// Sterownik wołający `codex` z `PATH`.
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

    /// Startuje sesję i oddaje **konkretny** uchwyt.
    ///
    /// Istnieje obok [`AgentDriver::start`], a nie zamiast niego: trait oddaje
    /// `Box<dyn AgentHandle>`, więc przez niego nie da się zapytać o fakt, którego trait nie
    /// zna — a [`CodexHandle::threads_seen`] jest dokładnie takim faktem i to on rozstrzyga
    /// kryterium o jednej tożsamości przez wiele tur. Implementacja traitu woła tę metodę
    /// i pakuje jej wynik w pudełko, więc ciało jest jedno.
    ///
    /// # SZKIELET (2026-08-19)
    ///
    /// Nie startuje procesu i **od razu porzuca kanał zdarzeń**: sesja, której nikt nie
    /// uruchomił, nie ma czego nim przysłać, a odbiornik ma dostać `None` zamiast ciszy —
    /// kryterium ma paść na asercji w ułamku sekundy, a nie na limicie czasu, bo limit czasu
    /// w bramce jest fałszywą czerwienią (rc 124), nie dowodem.
    pub async fn start_session(
        &self,
        spec: RunSpec,
        tx: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<CodexHandle> {
        tracing::debug!(
            binary = %self.binary.display(),
            "the codex driver is still a skeleton, so this step starts no process"
        );
        drop(tx);

        // `yield_now`, żeby ta sygnatura była asynchroniczna JUŻ TERAZ. Implementacja czeka
        // tutaj na potok wejściowy procesu, a dołożenie `async` po napisaniu kryteriów
        // kosztowałoby przepisanie wszystkich sześciu — czyli zmianę specyfikacji w fazie,
        // w której specyfikacja jest już kontraktem.
        tokio::task::yield_now().await;

        Ok(CodexHandle {
            // Pusty identyfikator, a nie wymyślony: sesja Codeksa przychodzi z drutu, w linii
            // `thread.started`, więc dopóki nikt nie przeczytał ani jednej linii, nie ma czym
            // się podpisać. Kryterium o wznowieniu porównuje to z identyfikatorem z atrapy
            // i pada na pierwszej asercji.
            session: SessionRef {
                vendor: VENDOR,
                id: spec
                    .resume
                    .map_or_else(String::new, |session| session.id.clone()),
            },
            threads: Vec::new(),
        })
    }
}

/// Argumenty **pierwszej** tury — bez nazwy binarki, bez promptu.
///
/// Linia w wersji wiążącej [T1 §6.1, §8.4]:
///
/// | Fragment | Dlaczego dokładnie tak |
/// |---|---|
/// | `exec` | tryb nieinteraktywny; `resume` jest osobnym podpoleceniem |
/// | `--json` | zdarzenia jako JSONL na stdout, a nie bajty terminala |
/// | `--ignore-user-config` | globalny `config.toml` użytkownika wywalił prawdziwy bieg czterema liniami `ERROR` z wygasłego OAuth [T1 §6.3] |
/// | `--skip-git-repo-check` | katalog kroku bywa świeżą kopią bez gita |
/// | `-C <cwd>` | katalog roboczy przychodzi **argumentem**, nigdy stałą (niezmiennik 1) |
/// | `-m <model>` | alias albo pełny identyfikator modelu |
/// | `-s <tryb>` | jedyne tłumaczenie [`super::Policy`] na piaskownicę (niezmiennik 23) |
/// | `-` | prompt jedzie **stdinem**; `codex exec` czyta go stąd, gdy podasz myślnik [T1 §6.1] |
///
/// Czego tu **nigdy** nie ma: promptu (niezmiennik 9 — argumenty widzi `ps` każdego
/// użytkownika maszyny) i `--dangerously-bypass-approvals-and-sandbox` (to jest obejście
/// całego diala, a nie jeden z jego trzech stopni).
///
/// # SZKIELET (2026-08-19)
///
/// Pusta lista. Kryterium porównuje **całą** sekwencję argumentów z linią z T1 §8.4, więc pada
/// na pierwszej asercji. Pusta lista przechodzi za to obie asercje o NIEOBECNOŚCI (promptu
/// i flagi obejścia) — i to jest właśnie powód, dla którego samo „nie ma w argv" nie może być
/// jedynym pomiarem tego kryterium.
#[must_use]
pub fn build_exec_argv(_spec: &RunSpec) -> Vec<String> {
    Vec::new()
}

// ── Wire enum Codeksa ─────────────────────────────────────────────────────────────────────
//
// Kształt z drutu mieszka WYŁĄCZNIE tutaj. Powyżej tej linii nie ma ani jednego `serde`, poniżej
// nie ma ani jednego [`AgentEvent`] — to jest ten sam podział, dzięki któremu ten plik powstał
// bez dotykania `stream.rs` i bez zmiany traitu [PLAN §8, założenie 5].

/// Pole, którego kształt vendor może zmienić bez uprzedzenia.
///
/// Cokolwiek nie pasuje, znika jako `None` — zamiast wywalić **całą linię** do licznika
/// porzuconych. To jest niezmiennik 5 w miejscu, w którym naprawdę się łamie: `#[serde(other)]`
/// ratuje nieznany `type`, ale nie ratuje znanego typu, któremu vendor zmienił kształt pola
/// zagnieżdżonego — a wtedy tracimy linię, która w 95% była dla nas czytelna.
///
/// Bliźniak tej funkcji stoi w `claude.rs` i to jest świadome powtórzenie, nie przeoczenie:
/// wspólne miejsce dla obu jest w `drivers/mod.rs`, a ten task ma tam prawo dopisać **jeden**
/// wiersz `pub mod codex;` i nic więcej.
fn lenient<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let value = Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).ok())
}

/// Jedna linia strumienia `codex exec --json` [T1 §6.2].
///
/// `#[serde(other)] Unknown` jest nienegocjowalny: vendorzy dokładają typy zdarzeń co tydzień,
/// po cichu, i bieg nie ma prawa na tym paść (niezmiennik 5). Sam ten atrybut jednak **nie
/// wystarcza** — decyduje to, że [`CodexDecoder::push`] nie zwraca `Result`, więc nie ma czego
/// przepuścić przez `?` w pętli czytającej.
///
/// Nazwy są kropkowane (`thread.started`, a nie `thread_started`), więc każdy wariant ma własne
/// `rename`: `rename_all = "snake_case"` zamieniłoby je na nazwy, których Codex nigdy nie
/// wypisał, a linia z drutu wpadłaby cicho do `Unknown`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum CodexLine {
    /// Otwarcie rozmowy. `thread_id` jest uchwytem wznowienia [T1 §6.2].
    #[serde(rename = "thread.started")]
    ThreadStarted { thread_id: Option<String> },
    /// Tura ruszyła. T2 §9.3 stawia przy tej linii myślnik — nic z niej nie wynika.
    #[serde(rename = "turn.started")]
    TurnStarted {},
    /// Tura skończyła się sama. Jedyna linia, która niesie zużycie kontekstu.
    #[serde(rename = "turn.completed")]
    TurnCompleted {
        #[serde(default, deserialize_with = "lenient")]
        usage: Option<Usage>,
    },
    /// Turę zamknął błąd — kształt z prawdziwego biegu [T1 §6.2].
    #[serde(rename = "turn.failed")]
    TurnFailed {
        #[serde(default, deserialize_with = "lenient")]
        error: Option<WireError>,
    },
    /// Czynność się zaczęła.
    #[serde(rename = "item.started")]
    ItemStarted {
        #[serde(default, deserialize_with = "lenient")]
        item: Option<Item>,
    },
    /// Czynność trwa.
    ///
    /// Świadomie **bez treści**: żywy licznik czasu dla `command_execution` jest poza zakresem
    /// T-10 [T2 §12 pytanie 3], więc poprawnym mapowaniem jest zero zdarzeń, a nie drugi
    /// `ToolStart`. Wariant istnieje mimo to, bo bez niego ta linia byłaby **nieznanym typem**
    /// i wpadłaby do licznika porzuconych — a korekta 9 w T1 potwierdza, że ten typ istnieje.
    #[serde(rename = "item.updated")]
    ItemUpdated {},
    /// Czynność się skończyła.
    #[serde(rename = "item.completed")]
    ItemCompleted {
        #[serde(default, deserialize_with = "lenient")]
        item: Option<Item>,
    },
    /// Skarga vendora w środku tury. Nie kończy jej — turę zamyka `turn.completed` albo
    /// `turn.failed` [T1 §8.5].
    ///
    /// `rename` stoi tu, choć nazwa z drutu jest jednym słowem, i **nie jest ozdobą**: bez niego
    /// serde szuka wariantu `"Error"`, linia `{"type":"error",…}` wpada w `Unknown`, a jedyne
    /// zdanie mówiące, co się stało, znika po cichu. Zmierzone na złotym pliku 2026-08-19 — dwie
    /// uwagi zamieniły się w jedną, a bieg wyglądał normalnie.
    #[serde(rename = "error")]
    Error { message: Option<String> },
    /// Wszystko, czego jeszcze nie znamy.
    #[serde(other)]
    Unknown,
}

/// Zużycie kontekstu z `turn.completed` [T1 §6.2].
///
/// Czego tu **nie ma**: `cost_usd`. Codex go nie podaje, a szacowanie z tokenów jest świadomie
/// poza zakresem — cennik w kodzie byłby trzecim miejscem, w którym trzeba go aktualizować.
#[derive(Debug, Deserialize)]
struct Usage {
    #[serde(rename = "input_tokens")]
    input: Option<u64>,
    /// Ta liczba, i tylko ta, mówi, czy izolacja kontekstu w ogóle działa [T1 §3.3].
    #[serde(rename = "cached_input_tokens")]
    cached: Option<u64>,
    #[serde(rename = "output_tokens")]
    output: Option<u64>,
}

/// Koperta błędu z `turn.failed`. Zdanie w środku jest już napisane po angielsku i to ono
/// odpowiada na pytanie „dlaczego", które ktoś zaraz zada.
#[derive(Debug, Deserialize)]
struct WireError {
    message: Option<String>,
}

/// Czynność wewnątrz tury **[3p] 2026-08-19**.
///
/// Nazwy typów i pól pochodzą z T1 §6.2 (lista wydobyta z binarki 0.147.0) i z tabeli T2 §9.3,
/// czyli ze źródła trzeciej strony potwierdzonego dokumentacją — **nie z prawdziwego biegu**.
/// Złoty plik ze spike'u S-3 nie dotyka ani jednego z tych typów, bo tamten bieg wpadł w limit
/// konta, zanim agent cokolwiek zrobił. Kiedy S-3 nagra prawdziwą turę, ten komentarz znika
/// razem z niepewnością, a nie sam.
///
/// `Option<T>` na **każdym** polu, łącznie z `exit_code`: pierwszy `command_execution` w stanie
/// `in_progress` nie ma go jeszcze, a `i32` w tym miejscu przewraca całą turę (niezmiennik 5).
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Item {
    /// Komenda w powłoce: `command`, `aggregated_output`, `exit_code`.
    CommandExecution {
        id: Option<String>,
        command: Option<String>,
        exit_code: Option<i32>,
        aggregated_output: Option<String>,
    },
    /// Zmiana plików — **lista**, nie jeden plik.
    FileChange {
        #[serde(default, deserialize_with = "lenient")]
        changes: Option<Vec<Change>>,
    },
    /// Proza agenta, dosłownie.
    AgentMessage { text: Option<String> },
    /// Agent myśli. Treści **nie czytamy**: myślenie nie wchodzi do historii
    /// [`docs/ARCHITECTURE.md` §6, reguła 5].
    Reasoning {},
    /// Szukanie w sieci.
    WebSearch {
        id: Option<String>,
        query: Option<String>,
    },
    /// Czynność w podłączonej aplikacji.
    McpToolCall {
        id: Option<String>,
        server: Option<String>,
        tool: Option<String>,
    },
    /// Typ, którego nie znamy — a przybywa ich co tydzień, po cichu.
    #[serde(other)]
    Unknown,
}

/// Jedna pozycja z `file_change.changes[]`.
///
/// `kind` (`add` / `modify` / `delete`) tu **nie wchodzi**, bo nikt go nie czyta: rodzaj zmiany
/// jest faktem dla kuracji, a ta należy do T-05 i dostaje go z tej samej linii drutu. Pole bez
/// czytelnika jest zakazane (niezmiennik 21).
#[derive(Debug, Deserialize)]
struct Change {
    path: Option<String>,
}

/// Dekoder jednego strumienia Codeksa: linia tekstu → zero lub więcej [`AgentEvent`].
///
/// **`push` nie zwraca `Result` i to jest cały niezmiennik 5 w jednej sygnaturze.** Cicha wersja
/// złamania nie siedzi w typie — siedzi w pętli: `let event = serde_json::from_str(&line)?;`
/// kończy turę na pierwszej linii, która nie jest JSON-em, a prawdziwy bieg Codeksa przeplótł
/// stdout liniami `ERROR rmcp::transport::worker: …` [T2 §9.3, zweryfikowane zagrożenie].
/// Skoro nieznanej linii nie da się zwrócić jako błąd, nie da się na niej wywalić biegu.
#[derive(Debug, Default)]
pub struct CodexDecoder {
    /// Ile linii dekoder porzucił: nie zrozumiał ich albo nic z nich nie wynikało. Liczba idzie
    /// do pliku debug i do zgłoszenia błędu, a nie do przerwania tury (niezmiennik 5).
    dropped: usize,
    /// Ostatni `thread_id`, jaki ogłosił ten strumień. Uchwyt wznowienia i podpis pod wynikiem
    /// tury [T1 §6.2].
    thread: Option<String>,
    /// Czy któraś linia zamknęła już turę. Po tym poznaje [`Self::end_of_stream`], że nie ma
    /// czego domykać — i to jest cała obrona przed drugim `Finished`.
    ended: bool,
    /// Ostatnia proza agenta, czyli to, co krok przekazuje dalej. Zbierana po drodze, bo
    /// `turn.completed` jej **nie powtarza** — inaczej niż linia `result` u Claude'a.
    said: String,
}

impl CodexDecoder {
    /// Świeży dekoder, przed pierwszą linią.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Wpuszcza jedną linię strumienia i oddaje zdarzenia, które z niej wynikają.
    ///
    /// Pusty wektor jest **normalną odpowiedzią**, nie sygnałem błędu: tak wygląda
    /// `thread.started` (zapamiętanie identyfikatora, bez zdarzenia), `turn.started` i każdy typ,
    /// którego jeszcze nie znamy.
    ///
    /// # Co wpada do licznika porzuconych, a co nie
    ///
    /// Licznik odpowiada na jedno pytanie: **ile razy strumień powiedział coś, z czego nic nie
    /// wynikło**. Wpadają więc: nie-JSON, ucięta linia, pusta linia, nieznany typ najwyższego
    /// poziomu, nieznany typ czynności i znana czynność bez pól, z których dałoby się cokolwiek
    /// zbudować. Nie wpadają trzy linie, które są **rozpoznane i celowo nieme**:
    /// `thread.started` (uczy nas identyfikatora), `turn.started` i `item.updated`. Liczenie ich
    /// zrobiłoby z tej liczby stałą — każdy zdrowy bieg miałby ją niezerową, a wtedy przestaje
    /// odróżniać zdrowy bieg od dziury.
    ///
    /// To jest inna umowa niż `ClaudeDecoder::unparsed`, gdzie nieznany `type` jest ROZPOZNANY
    /// i nieliczony. Różnica jest świadoma i wynika z różnicy strumieni: Claude wysyła kilka
    /// typów, których i tak nigdy nie pokazujemy, a Codex wysyła prawie wyłącznie rzeczy, które
    /// mają trafić na ekran — więc u niego nieznany typ to naprawdę zgubiona treść.
    pub fn push(&mut self, line: &str) -> Vec<AgentEvent> {
        let line = line.trim();
        if line.is_empty() {
            self.dropped += 1;
            return Vec::new();
        }

        let parsed = match serde_json::from_str::<CodexLine>(line) {
            Ok(parsed) => parsed,
            Err(error) => {
                self.dropped += 1;
                // Treści linii tu nie ma, i to jest świadome: surowy strumień leży już na dysku
                // (tee z T-05), a dziennik aplikacji czyta się w zgłoszeniu błędu — nie ma
                // powodu, żeby druga kopia cudzego tekstu jechała jeszcze tędy.
                tracing::debug!(
                    bytes = line.len(),
                    %error,
                    "a line of the agent stream could not be read; dropping it"
                );
                return Vec::new();
            }
        };

        // Całe mapowanie linia → zdarzenia stoi w JEDNYM match: to jest ta lista, którą czyta
        // się, pytając „co ten sterownik w ogóle rozumie".
        let events = match parsed {
            CodexLine::ThreadStarted { thread_id } => {
                let id = thread_id.filter(|id| !id.trim().is_empty());
                let Some(id) = id else {
                    // Otwarcie rozmowy bez uchwytu wznowienia jest linią, z której naprawdę nic
                    // nie wynika — i to jest dokładnie ten przypadek, dla którego licznik istnieje.
                    self.dropped += 1;
                    return Vec::new();
                };
                self.thread = Some(id);
                return Vec::new();
            }
            // Rozpoznane i celowo nieme (powód w całości wyżej).
            CodexLine::TurnStarted {} | CodexLine::ItemUpdated {} => return Vec::new(),
            CodexLine::ItemStarted { item } => item.map(Self::begun).unwrap_or_default(),
            CodexLine::ItemCompleted { item } => {
                item.map(|item| self.completed(item)).unwrap_or_default()
            }
            CodexLine::TurnCompleted { usage } => vec![self.finish(usage.as_ref())],
            CodexLine::TurnFailed { error } => self.failed(error.and_then(|error| error.message)),
            // Skarga nie kończy tury: obie linie niosą problem na ekran (T2 §9.3 mapuje obie na
            // `problem`), ale turę zamyka ta, która ją zamyka.
            CodexLine::Error { message } => Self::notice(message),
            CodexLine::Unknown => Vec::new(),
        };

        if events.is_empty() {
            self.dropped += 1;
        }
        events
    }

    /// `item.started` → zapowiedź czynności, albo cisza.
    ///
    /// Cisza dla prozy i myślenia: one **są** dopiero wtedy, gdy się skończą, a wiersz otwarty na
    /// zapowiedź zdania zostałby otwarty na zawsze.
    fn begun(item: Item) -> Vec<AgentEvent> {
        match item {
            Item::CommandExecution { id, command, .. } => {
                Self::tool_start(id, command_label(command.as_deref()))
            }
            Item::WebSearch { id, query } => Self::tool_start(id, search_label(query.as_deref())),
            Item::McpToolCall {
                id, server, tool, ..
            } => Self::tool_start(id, app_label(server.as_deref(), tool.as_deref())),
            _ => Vec::new(),
        }
    }

    /// `item.completed` → to, co z tej czynności zostało.
    fn completed(&mut self, item: Item) -> Vec<AgentEvent> {
        match item {
            // `ok` bierze się z `exit_code` i **znikąd indziej**: komenda, która wyszła jedynką,
            // ma się czytać jako nieudana, inaczej transkrypt mówi, że krok przebiegł czysto,
            // podczas gdy budowanie było zepsute. Bez kodu wyjścia nie ma z czego zbudować `ok`,
            // więc poprawną odpowiedzią jest cisza, a nie zmyślony sukces.
            Item::CommandExecution {
                id,
                exit_code,
                aggregated_output,
                ..
            } => match (id.filter(|id| !id.is_empty()), exit_code) {
                (Some(id), Some(code)) => vec![AgentEvent::ToolEnd {
                    id,
                    ok: code == 0,
                    summary: first_line(aggregated_output.as_deref().unwrap_or_default()),
                }],
                _ => Vec::new(),
            },
            // Po jednym zdarzeniu na pozycję listy: jedno na całą czynność powiedziałoby
            // człowiekowi, że zmienił się jeden plik, podczas gdy zmieniły się dwa.
            Item::FileChange { changes } => changes
                .unwrap_or_default()
                .into_iter()
                .filter_map(|change| change.path)
                .filter(|path| !path.trim().is_empty())
                .map(|path| AgentEvent::FileEdit { path: path.into() })
                .collect(),
            Item::AgentMessage { text } => {
                let text = text.unwrap_or_default();
                if text.trim().is_empty() {
                    return Vec::new();
                }
                // Ostatnia wypowiedź jest tym, co krok przekazuje dalej — a `turn.completed`
                // jej nie powtarza, więc jedyne miejsce, w którym da się ją złapać, jest tutaj.
                self.said.clone_from(&text);
                vec![AgentEvent::Said { text }]
            }
            Item::Reasoning {} => vec![AgentEvent::Thinking],
            // Ani szukanie, ani podłączona aplikacja nie mają kodu wyjścia: zakończyły się, więc
            // się udały. Wymaganie tu `exit_code` skasowałoby oba wiersze z transkryptu.
            Item::WebSearch { id, query } => {
                Self::tool_end(id, first_line(query.as_deref().unwrap_or_default()))
            }
            Item::McpToolCall { id, server, tool } => {
                Self::tool_end(id, app_label(server.as_deref(), tool.as_deref()))
            }
            Item::Unknown => Vec::new(),
        }
    }

    /// Zapowiedź czynności — bez identyfikatora nie ma czego zapowiedzieć, bo to po nim
    /// [`AgentEvent::ToolEnd`] trafia do swojej linii.
    fn tool_start(id: Option<String>, label: String) -> Vec<AgentEvent> {
        match id.filter(|id| !id.is_empty()) {
            Some(id) => vec![AgentEvent::ToolStart { id, label }],
            None => Vec::new(),
        }
    }

    /// Koniec czynności, która nie ma kodu wyjścia.
    fn tool_end(id: Option<String>, summary: String) -> Vec<AgentEvent> {
        match id.filter(|id| !id.is_empty()) {
            Some(id) => vec![AgentEvent::ToolEnd {
                id,
                ok: true,
                summary,
            }],
            None => Vec::new(),
        }
    }

    /// Skarga vendora → uwaga na ekran, dosłownie tym zdaniem, które napisał.
    ///
    /// To jedyna rzecz, która mówi czytającemu, że chodziło o limit kredytów i kiedy wraca —
    /// przepisanie tego własnymi słowami skasowałoby datę i adres.
    fn notice(message: Option<String>) -> Vec<AgentEvent> {
        match message.filter(|text| !text.trim().is_empty()) {
            Some(text) => vec![AgentEvent::Notice { text }],
            None => Vec::new(),
        }
    }

    /// `turn.completed` → koniec tury, która się udała.
    fn finish(&mut self, usage: Option<&Usage>) -> AgentEvent {
        self.ended = true;
        AgentEvent::Finished(Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text: self.said.clone(),
            // `None`, nie zero, i to jest cała różnica: Codex kosztu nie podaje, a `Some(0.0)`
            // wypisze na ekranie `$0.00` i nauczy człowieka, że Codex jest darmowy — po czym ta
            // liczba zsumuje się w rachunek, którego nikt nie zamawiał.
            cost_usd: None,
            tokens: Tokens {
                input: usage.and_then(|usage| usage.input).unwrap_or_default(),
                output: usage.and_then(|usage| usage.output).unwrap_or_default(),
                cached: usage.and_then(|usage| usage.cached).unwrap_or_default(),
            },
            // Jeden proces to jedna tura — to jest fakt o NASZYM wywołaniu, nie liczba z drutu.
            // Codex nie ma odpowiednika `num_turns` i nie ma czego tu zgadywać.
            turns: 1,
            // Vendor nie mówi, ile to trwało. Zero jest tu uczciwe tylko dlatego, że wypełnia to
            // pole zmierzonym czasem sterownik, w [`pump`] — dekoder zegara nie ma i mieć nie ma
            // po co (2026-08-19).
            took: Duration::ZERO,
            session: self.session_ref(),
        })
    }

    /// `turn.failed` → uwaga **i** koniec tury.
    ///
    /// Dwa zdarzenia z jednej linii, nie dwa `Finished`: problem ma dojść na ekran, a turę zamyka
    /// się raz (AC-5, niezmiennik 13 czytany od strony szyny).
    fn failed(&mut self, message: Option<String>) -> Vec<AgentEvent> {
        self.ended = true;
        let said = message.filter(|text| !text.trim().is_empty());
        let why = said.clone().unwrap_or_else(|| {
            "The agent stopped before it finished its turn, and said nothing about why.".to_owned()
        });

        let mut events = Self::notice(said);
        events.push(AgentEvent::Finished(Outcome {
            ok: false,
            // Zdanie vendora jedzie CAŁE, nieprzycięte: to ono niesie datę i adres, pod którym
            // limit wraca, a przycięte do jednej linijki traci dokładnie tę połowę.
            reason: FinishReason::Failed(why),
            text: self.said.clone(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::ZERO,
            session: self.session_ref(),
        }));
        events
    }

    /// Sesja tej rozmowy. Pusty identyfikator znaczy „`thread.started` jeszcze nie przyszło",
    /// a nie „nie ma sesji".
    fn session_ref(&self) -> SessionRef {
        SessionRef {
            vendor: VENDOR,
            id: self.thread.clone().unwrap_or_default(),
        }
    }

    /// Identyfikator wątku, który ten strumień ogłosił jako ostatni.
    ///
    /// Czyta to [`pump`] i **nikt poza nim** (niezmiennik 21): to stąd bierze się jeden wpis na
    /// turę w [`CodexHandle::threads_seen`], czyli różnica między „widzieliśmy dwa identyfikatory
    /// i pamiętamy oba" a „drugi nadpisał pierwszy".
    #[must_use]
    pub fn thread(&self) -> Option<&str> {
        self.thread.as_deref()
    }

    /// Ile linii dekoder porzucił.
    #[must_use]
    pub fn dropped(&self) -> usize {
        self.dropped
    }

    /// Domyka turę, kiedy strumień się skończył.
    ///
    /// Zwraca [`AgentEvent::Finished`] **tylko** wtedy, gdy linia zamykająca nie przyszła — bo
    /// wtedy nikt inny go nie wypuści, a krok bez zdarzenia końca wisiałby w `running` do końca
    /// biegu. Strumień zakończony kodem 0 bez `turn.completed` jest **niepowodzeniem**, nie
    /// sukcesem: wyjście procesu jest sygnałem wtórnym [T1 §8.5], a agent, który wyszedł czysto
    /// i nie powiedział, co zrobił, nie ma czego przekazać dalej.
    ///
    /// `cancelled` przychodzi **argumentem**, z generacji trzymanej przez uchwyt, a nie z
    /// globalnego znacznika: to jest ta sama różnica, o której mówi niezmiennik 7, tylko widziana
    /// od strony dekodera. Anulowanie jest wtedy WARTOŚCIĄ ([`FinishReason::Cancelled`]),
    /// a nie błędem, więc „człowiek nacisnął Stop" nie ląduje w tej samej gałęzi co „padło
    /// połączenie".
    ///
    /// Kodu wyjścia tu nie ma i nie da się go tu mieć: uchwyt procesu został przy sterowniku,
    /// a ta ścieżka biegnie na EOF wyjścia, czyli ZANIM proces zdąży zostać zebrany. Zdanie niesie
    /// więc pierwszą linię skargi — i to ona odpowiada na „dlaczego" w praktycznie każdym realnym
    /// przypadku.
    pub fn end_of_stream(&mut self, cancelled: bool, complaint: &str) -> Option<AgentEvent> {
        if self.ended {
            return None;
        }
        self.ended = true;

        let reason = if cancelled {
            FinishReason::Cancelled
        } else {
            let mut why = "The agent stopped without ever finishing its turn.".to_owned();
            if let Some(first) = complaint
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
            {
                why.push(' ');
                why.push_str(&first_line(first));
            }
            FinishReason::Failed(why)
        };

        Some(AgentEvent::Finished(Outcome {
            ok: false,
            reason,
            text: self.said.clone(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 0,
            took: Duration::ZERO,
            session: self.session_ref(),
        }))
    }
}

/// Ile znaków wolno mieć jednolinijkowemu podsumowaniu, zanim zostanie przycięte. Pełne wyjście
/// i tak zostaje za kliknięciem — to jest linia w wierszu, nie dokument.
const SUMMARY_LIMIT: usize = 120;

/// Pierwsza niepusta linia, przycięta do długości, która mieści się w wierszu.
///
/// Bliźniak z `claude.rs`, z tego samego powodu co [`lenient`]: wspólne miejsce dla obu jest
/// w `drivers/mod.rs`, którego ten task nie posiada.
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

/// Etykieta komendy: sama komenda, bo to ona jest tym, co człowiek chce zobaczyć.
fn command_label(command: Option<&str>) -> String {
    match command.map(str::trim).filter(|command| !command.is_empty()) {
        Some(command) => first_line(command),
        None => "Running a command".to_owned(),
    }
}

/// Etykieta szukania w sieci.
fn search_label(query: Option<&str>) -> String {
    match query.map(str::trim).filter(|query| !query.is_empty()) {
        Some(query) => format!("Searching for {}", first_line(query)),
        None => "Searching the web".to_owned(),
    }
}

/// Etykieta czynności w podłączonej aplikacji.
///
/// Zdanie po ludzku, nigdy nazwa z drutu (niezmiennik 14): „Asking notion to search" mówi
/// czytającemu, co się dzieje, a `mcp_tool_call` nie mówi nic nikomu poza nami.
fn app_label(server: Option<&str>, tool: Option<&str>) -> String {
    let server = server.map(str::trim).filter(|name| !name.is_empty());
    let tool = tool.map(str::trim).filter(|name| !name.is_empty());
    match (server, tool) {
        (Some(server), Some(tool)) => format!("Asking {server} to {tool}"),
        (Some(server), None) => format!("Asking {server}"),
        _ => "Working".to_owned(),
    }
}

/// Żywa sesja `codex` — **wiele procesów**, jedna tożsamość.
///
/// To jest cała różnica wobec `ClaudeHandle`, w którym proces jest jeden na całą sesję. Tura
/// druga i każda następna to `codex exec resume <thread_id>`, czyli świeży proces, zimny start
/// i odbudowa cache'u [T1 §8.1] — świadomy koszt, nie brak.
#[derive(Debug)]
pub struct CodexHandle {
    /// Tożsamość tej rozmowy, czyli identyfikator z **pierwszego** `thread.started`.
    ///
    /// Nigdy nie przestawiany w trakcie sesji. Cicha porażka numer jeden tego zadania wygląda
    /// dokładnie odwrotnie: sterownik mintuje nowy `SessionRef` przy każdej turze, bo przecież
    /// `thread.started` przyszło znowu — szyna pokazuje wtedy trzech agentów zamiast jednego,
    /// trzy podsumowania „Done", trzy koszty, i **wszystko wygląda na skończone**.
    session: SessionRef,
    /// Każdy `thread_id`, jaki ta sesja dostała, w kolejności przybycia. Pierwszy jest
    /// tożsamością, ostatni jest celem wznowienia — a jeden wpis na turę znaczy, że rozbieżność
    /// między nimi została zapisana raz, a nie przy każdej linii.
    ///
    /// 2026-08-19 — TO POLE ISTNIEJE, BO T1 §11 PYTANIE 5 JEST OTWARTE: nie wiadomo, czy
    /// `codex exec resume` oddaje ten sam identyfikator, czy mintuje nowy. Dopóki nie wiadomo,
    /// sterownik nie ma prawa **zakładać** żadnej z dwóch odpowiedzi: trzyma obie liczby
    /// i zachowuje się poprawnie w obu przypadkach.
    threads: Vec<String>,
}

impl CodexHandle {
    /// Identyfikatory wątku, które ta sesja zobaczyła — pierwszy z przodu.
    ///
    /// Czyta to kryterium o wznowieniu i **nikt poza nim** nie musi (niezmiennik 21): sama
    /// tożsamość jedzie przez [`AgentHandle::session`], a cel wznowienia sterownik zna sam.
    /// Tu chodzi o różnicę między „widzieliśmy dwa identyfikatory i pamiętamy oba" a „drugi
    /// nadpisał pierwszy", której z zewnątrz nie da się inaczej odróżnić.
    ///
    /// # SZKIELET (2026-08-19)
    ///
    /// Pusto, bo nikt nie przeczytał ani jednej linii.
    #[must_use]
    pub fn threads_seen(&self) -> &[String] {
        &self.threads
    }
}

#[async_trait]
impl AgentHandle for CodexHandle {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    /// Grupa procesów **bieżącej** tury.
    ///
    /// `None` między turami i to nie jest brak: przy sterowniku z procesem na turę naprawdę
    /// bywa chwila, w której nie ma czego zabić. `ClaudeHandle` oddaje tu zawsze `Some`, bo
    /// tam proces żyje przez całą sesję — i to jest ta różnica, którą trait ma wchłonąć.
    ///
    /// # SZKIELET (2026-08-19)
    ///
    /// Zawsze `None`, więc kryterium o anulowaniu nie ma czego zapytać jądra o dowód śmierci.
    fn group(&self) -> Option<GroupId> {
        None
    }

    /// Kolejna tura: **nowy proces** z `codex exec resume <thread_id>` i promptem na stdin.
    ///
    /// # SZKIELET (2026-08-19)
    ///
    /// Odmawia zdaniem, które nazywa sesję i rozmiar tury. Kryterium o wznowieniu pada
    /// wcześniej — na tożsamości, której ten szkielet nie ma skąd wziąć.
    async fn send(&mut self, text: String) -> anyhow::Result<()> {
        anyhow::bail!(
            "a follow-up turn of {} bytes has nowhere to go: session {:?} was never started, \
             because this driver is still a skeleton",
            text.len(),
            self.session.id
        )
    }

    /// Czeka na koniec bieżącej tury.
    ///
    /// # SZKIELET (2026-08-19)
    ///
    /// Odmawia. **`Err`, a nie wymyślony `Outcome`**, i to jest wybór, nie wygoda: każdy
    /// zmyślony wynik przechodziłby część kryterium o zakończeniu (`ok == false` z powodem
    /// `Failed` jest dosłownie przypadkiem (b), a `FinishReason::Cancelled` — całym kryterium
    /// o anulowaniu). Odmowa nie przechodzi żadnego.
    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        anyhow::bail!(
            "session {:?} has no outcome to give: this driver is still a skeleton, so no turn \
             was ever started and nothing read the stream",
            self.session.id
        )
    }

    /// Anuluje turę i **dowodzi**, że po grupie nic nie zostało.
    ///
    /// Eskalacja jest w całości z `engine/supervisor.rs` (niezmiennik 3): SIGTERM na grupę,
    /// łaska, SIGKILL, a potem pętla dowodowa aż do `ESRCH`. Stopnia „przerwanie w paśmie" tu
    /// nie ma i nie będzie — `codex exec` nie czyta stdinu po pierwszym prompcie [T1 §6.4].
    ///
    /// # SZKIELET (2026-08-19)
    ///
    /// `Alive`, bo to jest jedyna uczciwa odpowiedź szkieletu: niezmiennik 6 mówi, że dopóki
    /// `kill(-pgid, 0)` nie dał `ESRCH`, grupa jest żywa. `Dead` byłoby kłamstwem, które
    /// przechodzi połowę kryterium o anulowaniu.
    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Alive
    }

    /// Zamyka sesję.
    ///
    /// # SZKIELET (2026-08-19)
    ///
    /// Nie ma czego zamykać, więc nie ma też kodu wyjścia. Żadne kryterium tego nie pyta.
    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(None)
    }
}

#[async_trait]
impl AgentDriver for CodexDriver {
    fn id(&self) -> &'static str {
        VENDOR
    }

    /// Czy CLI jest i w jakiej wersji.
    ///
    /// # SZKIELET (2026-08-19)
    ///
    /// „Nie ma" — zgodnie z kontraktem traitu brak binarki jest ekranem ustawień, a nie awarią
    /// startu, więc ta odpowiedź nikogo nie wywraca. Żadne kryterium tego zadania jej nie sądzi
    /// (`probe` jest świadomie poza zakresem), więc szkielet nie ma tu czego przejść ani oblać.
    async fn probe(&self) -> anyhow::Result<Probe> {
        tracing::debug!(
            binary = %self.binary.display(),
            "the codex probe is still a skeleton and asks the binary nothing"
        );
        Ok(Probe {
            found: false,
            version: None,
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        tx: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        Ok(Box::new(self.start_session(spec, tx).await?))
    }
}
