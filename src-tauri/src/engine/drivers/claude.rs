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
//! # Stan tego pliku: SZKIELET (2026-08-15)
//!
//! Sygnatury są prawdziwe, ciała są **jawnie niezaimplementowane** i tak oznaczone. To jest
//! wymagany kształt fazy kontraktu: kryterium ma paść na braku ZACHOWANIA, a nie na tym, że
//! plik się nie wczytał (`AGENTS.md` §2a p. 5). `unimplemented!`, nie `todo!` — `clippy::todo`
//! jest w `Cargo.toml` na `deny`, a `checks/quick-clippy.sh` biegnie w każdej turze.

use std::path::PathBuf;

use async_trait::async_trait;
use tokio::process::Command;
use tokio::sync::mpsc;

use super::{AgentDriver, AgentEvent, AgentHandle, Outcome, Probe, RunSpec, SessionRef};
use crate::engine::supervisor::{GroupId, GroupProof};

/// Etykieta tego vendora — ta sama w [`SessionRef::vendor`] i w [`AgentDriver::id`].
pub const VENDOR: &str = "claude";

/// Czym woła się CLI, kiedy nikt nie podał własnej ścieżki.
///
/// Gołe „claude", nie ścieżka bezwzględna: na tej maszynie to skrypt powłoki, który znajduje
/// się przez `PATH` — a `PATH` jest jedną z sześciu zmiennych, które supervisor przepuszcza
/// przez `env_clear()` [T-03, `PASSTHROUGH`].
const DEFAULT_BINARY: &str = "claude";

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
        unimplemented!(
            "{} would be started for run {} with an empty argv: no transport flags, no context \
             isolation, no policy",
            self.binary.display(),
            spec.run_id
        )
    }
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
        unimplemented!(
            "no line decodes to an event yet; {} bytes dropped, {} unparsed so far",
            line.len(),
            self.unparsed
        )
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
        unimplemented!(
            "a stream that ended (exit {exit_code:?}) with {} unparsed lines still reports \
             nothing at all",
            self.unparsed
        )
    }
}

/// Żywa sesja `claude` — jeden proces, wiele tur.
#[derive(Debug)]
pub struct ClaudeHandle {
    /// Sesja, którą sami nadaliśmy przed startem [T7 §6.2].
    session: SessionRef,
    /// Grupa procesów tej sesji, dopóki żyje.
    group: Option<GroupId>,
}

#[async_trait]
impl AgentHandle for ClaudeHandle {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        self.group
    }

    /// Kolejna tura **tym samym procesem**: koperta na stdin, stdin zostaje otwarty.
    ///
    /// Koperta, jedna linia JSON [T1 §4.6]:
    /// `{"type":"user","message":{"role":"user","content":[{"type":"text","text":"…"}]}}`
    async fn send(&mut self, text: String) -> anyhow::Result<()> {
        unimplemented!(
            "nothing carries {} bytes of a follow-up turn into session {}",
            text.len(),
            self.session.id
        )
    }

    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        unimplemented!("session {} never reports an outcome", self.session.id)
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
    async fn cancel(&mut self) -> GroupProof {
        unimplemented!(
            "session {} is never interrupted, its group is never signalled and nothing is ever \
             proved dead",
            self.session.id
        )
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        unimplemented!(
            "session {} keeps its stdin open, so the process never gets its EOF",
            self.session.id
        )
    }
}

#[async_trait]
impl AgentDriver for ClaudeDriver {
    fn id(&self) -> &'static str {
        VENDOR
    }

    /// Pyta binarkę o wersję. **Brak pliku to `Ok(Probe { found: false, .. })`, nigdy `Err`**:
    /// nieobecne CLI jest ekranem ustawień, a nie awarią startu aplikacji.
    async fn probe(&self) -> anyhow::Result<Probe> {
        unimplemented!("nobody ever asks {} for its version", self.binary.display())
    }

    /// Startuje sesję i zaczyna sypać zdarzeniami na `tx`.
    ///
    /// Kolejność jest wymuszona przez odzyskiwanie po awarii: sesję nadajemy **przed**
    /// startem, `pid` i `pgid` są znane **zanim** cokolwiek zostanie przeczytane ze stdout
    /// [T7 §6.2]. Prompt wchodzi pierwszą kopertą na stdin i stdin **zostaje otwarty** —
    /// zamknięcie go jest osobnym czasownikiem ([`AgentHandle::close`]), bo znaczy „koniec
    /// sesji", a nie „koniec tury".
    async fn start(
        &self,
        spec: RunSpec,
        tx: mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        unimplemented!(
            "{} is never started for run {}, so no event ever reaches the channel (closed: {})",
            self.binary.display(),
            spec.run_id,
            tx.is_closed()
        )
    }
}
