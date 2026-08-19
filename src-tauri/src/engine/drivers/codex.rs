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

use async_trait::async_trait;
use tokio::sync::mpsc;

use super::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, Outcome, Probe, RunSpec, SessionRef,
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
    /// # SZKIELET (2026-08-19)
    ///
    /// Zawsze pusto. Kryterium o złotym pliku pada na `assert!(!events.is_empty())`, a kryterium
    /// o śmieciach — na ostatniej linii, tej, która dowodzi, że strumień **przeżył**: prawdziwe
    /// `agent_message` po sześciu śmieciach ma dać dokładnie jeden `Said`, a tu nie daje żadnego.
    pub fn push(&mut self, _line: &str) -> Vec<AgentEvent> {
        Vec::new()
    }

    /// Ile linii dekoder porzucił.
    ///
    /// # SZKIELET (2026-08-19)
    ///
    /// Zawsze zero, więc kryterium o śmieciach pada także na przyroście licznika: sześć linii,
    /// których nie da się przeczytać, ma zostawić po sobie sześć wpisów, a nie ciszę.
    #[must_use]
    pub fn dropped(&self) -> usize {
        self.dropped
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
