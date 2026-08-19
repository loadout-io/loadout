//! `CodexDriver` — nowy proces na turę, `thread_id` jako uchwyt wznowienia.
//!
//! Codex łamie dokładnie tę część kontraktu, którą `claude` spełnia za darmo: nie ma trybu
//! dwukierunkowego, więc każda tura to **nowy proces** z `codex exec resume` [T1 §6.4]. Cała ta
//! różnica ma zostać po tej stronie traitu — jeżeli wyjdzie na wierzch, to znaczy, że
//! `AgentDriver` jest fikcją, a nie abstrakcją, i to jest **wynik badania, nie porażka do
//! ukrycia** [PLAN §8, założenie 5].
//!
//! # Stan tego pliku: KOMPLETNY wobec sześciu kryteriów (2026-08-19)
//!
//! Odpowiedź na założenie 5 z PLAN §8 brzmi **tak**: `AgentDriver` wytrzymał drugiego vendora
//! bez jednej zmiany w `drivers/mod.rs` i bez jednej w `stream.rs`. Cała różnica — proces na
//! turę, brak dwukierunkowego stdinu, tożsamość zbierana z drutu zamiast nadawana przed startem
//! — zmieściła się po tej stronie traitu. Dwie rzeczy, które trait wchłonął, warto nazwać, bo to
//! one były ryzykiem: [`AgentHandle::send`] startuje **nowy proces** zamiast pisać do żywego,
//! a [`AgentHandle::voice`] zostaje przy domyślnym `None`, bo tej sesji naprawdę nie da się
//! zagadać w trakcie tury — i to jest dokładnie ten wariant, który trait przewidział.
//!
//! # Czego ten plik świadomie NIE robi
//!
//! **Nie wypełnia [`super::DecodedEvent::tool`]** — jedzie tam `None`. Fakty o czynności buduje
//! `stream::decode` z tej samej linii drutu, a `stream.rs` należy do T-05 i leży poza blokiem
//! OWNS tego zadania. Skutek jest wąski i zgłoszony: transkrypt kroku Codeksa pokaże prozę
//! agenta, ale nie wiersze `read`, `edit` ani `ran`. To jest ta sama awaria, którą u Claude'a
//! zmierzono 2026-08-18, i domyka ją `decode_codex` w tamtym pliku, nie tutaj — druga tabela
//! nazw z drutu po tej stronie byłaby drugą implementacją kuracji (niezmienniki 15 i 23).
//!
//! **Nie zapisuje surowego strumienia na dysk.** `logs/agent-<krok>.jsonl` czyta `store::rebuild`
//! (T-06), więc bez tego zapisu skasowanie `loadout.db` zabiera zdarzenia kroków Codeksa
//! (niezmiennik 4). Mechanizm istnieje — `claude::Transcript` plus `stream::Recorder` — ale
//! wołającego nie ma i **nie miałby go także po dopisaniu go tutaj**: `commands::run` nie woła
//! `ClaudeDriver::with_transcript` po dziś dzień, a jedyne miejsce, w którym ta wartość powinna
//! stać dla OBU sterowników, to `RunSpec` w `drivers/mod.rs`. To jest jeden wiersz poza tym
//! zadaniem, czyli pytanie do człowieka, nie cichy dopisek (`AGENTS.md` §7).
//!
//! # Czego ten plik nie ma prawa zawierać
//!
//! Zero `#[cfg(unix)]`, zero `libc`, zero stałych sygnałów: zabijanie grupy i dowód jej śmierci
//! należą do `engine/supervisor.rs` (niezmiennik 3, egzekwuje `checks/quick-boundary.sh`).
//! `cancel()` ma z tamtej eskalacji **korzystać**, nie powtarzać jej trzema linijkami obok —
//! bo wtedy port na Windows przestaje być gałęzią `cfg`, a staje się przepisaniem.
//!
//! Nie ma tu też ani jednego `tauri::*` (niezmiennik 1): sterownik nie wie, że istnieje okno.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use anyhow::anyhow;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{ChildStderr, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot};

use super::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Policy, Probe,
    RunSpec, SessionRef, Tokens,
};
use crate::engine::supervisor::{self, DEFAULT_GRACE, GroupId, GroupProof, StdinPlan, Supervised};

/// Etykieta tego vendora — ta sama w [`SessionRef::vendor`] i w [`AgentDriver::id`].
///
/// To ona ląduje w bazie przy kroku (T-06) i po niej wznowienie wie, do którego CLI wrócić.
pub const VENDOR: &str = "codex";

/// Czym woła się CLI, kiedy nikt nie podał własnej ścieżki. Gołe „codex", nie ścieżka
/// bezwzględna: znajduje się przez `PATH`, a `PATH` jest jedną ze zmiennych, które supervisor
/// przepuszcza przez `env_clear()`.
const DEFAULT_BINARY: &str = "codex";

/// Numer pierwszej tury sesji. Numeracja zaczyna się od jedynki, żeby zero mogło znaczyć
/// „nikt niczego nie anulował" — powód w całości przy [`CodexHandle::cancelled`].
const FIRST_TURN: u64 = 1;

/// Generacja, która nie jest numerem żadnej tury.
const NOT_CANCELLED: u64 = 0;

/// Ile bajtów skargi trzymamy. **Pierwsze, nie ostatnie**: pierwsza linia mówi, co się stało
/// („command not found", „not logged in"), ostatnia jest zwykle ogonem śladu stosu. Bufor bez
/// limitu byłby za to miejscem, w którym gadatliwy agent zjada pamięć okna.
const COMPLAINT_KEPT: usize = 4 * 1024;

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
    /// Prompt jedzie **stdinem i tylko stdinem** (niezmiennik 9), a deskryptor zostaje
    /// **zamknięty**: bez EOF `codex exec` wypisuje `Reading additional input from stdin...`
    /// i czeka [T1, „Worth adding"]. To jest cała różnica wobec `claude.rs`, gdzie ten sam
    /// deskryptor zostaje otwarty na kolejne tury — Codex kolejnych tur tym kanałem nie
    /// przyjmuje [T1 §6.4].
    pub async fn start_session(
        &self,
        spec: RunSpec,
        tx: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<CodexHandle> {
        let argv = build_exec_argv(&spec);

        // Wznowienie zna swoją tożsamość, ZANIM padnie pierwsza linia: dostało ją od tego, kto
        // je zamówił. Pierwsza tura nie zna jej wcale i to jest uczciwe — sesja Codeksa
        // przychodzi z drutu, w `thread.started`, więc dopóki nikt nie przeczytał ani jednej
        // linii, nie ma czym się podpisać.
        let threads: Vec<String> = spec
            .resume
            .as_ref()
            .map(|session| session.id.clone())
            .into_iter()
            .collect();
        let threads = Arc::new(Mutex::new(threads));
        let cancelled = Arc::new(AtomicU64::new(NOT_CANCELLED));

        let turn = Turn {
            binary: self.binary.clone(),
            cwd: spec.cwd.clone(),
            argv,
            prompt: spec.prompt,
            events: tx.clone(),
            threads: Arc::clone(&threads),
            number: FIRST_TURN,
            cancelled: Arc::clone(&cancelled),
        };
        let (process, outcome) = turn.start()?;

        // `tokio::spawn` tylko PLANUJE zadanie — nie odpytuje go ani razu. To ustąpienie daje
        // świeżo uruchomionej pętli czytającej jej pierwsze odpytanie, więc wołający dostaje
        // uchwyt do sesji, która już czyta, a nie do takiej, która dopiero stoi w kolejce.
        //
        // Stoi tu także dlatego, że ta funkcja MUSI być asynchroniczna: jest ciałem
        // `AgentDriver::start`, a kryteria wołają ją przez `timeout(...)`, czyli po Future.
        // Wyciszenie lintu `clippy::unused_async` nie jest tu wyjściem — jedyna droga przez
        // `quick-suppressions` prowadzi przez `checks/`, czyli przez to, co nas sądzi
        // (`AGENTS.md` §7).
        //
        // Nazwa tego atrybutu jest wyżej wypisana bez nawiasu kwadratowego celowo, tak samo jak
        // w `supervisor.rs`: `quick-suppressions` gerpuje SUROWY tekst pliku, więc wypisana
        // w pełni wywraca to sprawdzenie także z komentarza, w którym jest tylko wzmianką.
        // Zmierzone na tym pliku 2026-08-19, jedno trafienie.
        tokio::task::yield_now().await;

        Ok(CodexHandle {
            binary: self.binary.clone(),
            cwd: spec.cwd,
            events: tx,
            threads,
            cancelled,
            number: FIRST_TURN,
            process: Some(process),
            outcome: Some(outcome),
        })
    }
}

/// Wszystko, czego potrzeba, żeby ruszyć **jedną** turę Codeksa.
///
/// Istnieje jako typ, a nie jako osiem argumentów funkcji, bo tur jest wiele i każda startuje
/// dokładnie tak samo: [`CodexDriver::start_session`] robi pierwszą, [`AgentHandle::send`] każdą
/// następną. Dwa miejsca składające ten sam start osobno rozjeżdżają się przy pierwszej zmianie
/// — a rozjazd byłby cichy, bo obie drogi dalej uruchamiałyby proces.
#[derive(Debug)]
struct Turn {
    /// Co uruchamiamy.
    binary: PathBuf,
    /// Katalog roboczy kroku.
    cwd: PathBuf,
    /// Linia poleceń bez nazwy binarki i bez promptu.
    argv: Vec<String>,
    /// Treść tury. Jedzie stdinem (niezmiennik 9).
    prompt: String,
    /// Dokąd sypać zdarzeniami.
    events: mpsc::Sender<DecodedEvent>,
    /// Wspólna pamięć identyfikatorów wątku — jedna na sesję, nie na turę.
    threads: Arc<Mutex<Vec<String>>>,
    /// Która to tura tej sesji. Pierwsza ma numer [`FIRST_TURN`].
    number: u64,
    /// Generacja anulowania, wspólna dla sesji (powód przy [`CodexHandle::cancelled`]).
    cancelled: Arc<AtomicU64>,
}

impl Turn {
    /// Startuje proces tury i oddaje uchwyt do niego oraz obietnicę jej wyniku.
    ///
    /// Proces startuje przez `engine::supervisor::spawn` i **tylko** przez nie: własna grupa
    /// procesów, `env_clear()` i cała eskalacja zabijania mieszkają tam (niezmienniki 3 i 23).
    /// Ten plik nie zna ani jednej stałej sygnału.
    fn start(self) -> anyhow::Result<(Supervised, oneshot::Receiver<Outcome>)> {
        let mut command = Command::new(&self.binary);
        // Katalog roboczy przychodzi ARGUMENTEM, nigdy stałą: literał ze ścieżką repo w pliku
        // pod `engine/` przewraca granicę z niezmiennika 1.
        command.current_dir(&self.cwd);
        command.args(&self.argv);

        // `Write`, nie `Keep`: po prompcie deskryptor się ZAMYKA, bo to zamknięcie jest tym
        // EOF-em, na który `codex exec` czeka. `Keep` zostawiłby proces wiszący na wejściu,
        // które nigdy się nie skończy — i wyglądałoby to jak agent, który myśli.
        let mut process = supervisor::spawn(command, StdinPlan::Write(self.prompt))?;

        let stdout = process
            .stdout()
            .ok_or_else(|| anyhow!("the agent started without an output stream to read"))?;

        // SKARGI ODBIERAMY I OPRÓŻNIAMY. Potok o pojemności ~64 KB, którego nikt nie odbiera,
        // zatrzymuje dziecko na `write` — czyli agent gadatliwy poza strumieniem zdarzeń wisi,
        // a z okna wygląda to jak agent, który myśli. Drugi powód jest w [`CodexDecoder::
        // end_of_stream`]: pierwsza linia skargi odpowiada na „dlaczego" w praktycznie każdym
        // realnym przypadku, a bez niej krok pada zdaniem bez przyczyny.
        let complaint = Arc::new(Mutex::new(String::new()));
        if let Some(stderr) = process.stderr() {
            let into = Arc::clone(&complaint);
            let _drain = tokio::spawn(drain_complaints(stderr, into));
        }

        let (tell, told) = oneshot::channel();
        // Pętla czytająca żyje własnym zadaniem: uchwyt ma zostać responsywny na `cancel()`
        // także wtedy, gdy nikt nie woła `wait()`.
        let _reader = tokio::spawn(pump(
            stdout,
            self.events,
            tell,
            self.threads,
            self.number,
            self.cancelled,
            complaint,
        ));

        Ok((process, told))
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
/// `spec.resume` przełącza tę funkcję na linię wznowienia [`resume_argv`], bo to jest ta sama
/// decyzja co u Claude'a (`--session-id` albo `--resume`, nigdy oba) — tylko u Codeksa
/// wznowienie jest osobnym **podpoleceniem**, a nie flagą.
#[must_use]
pub fn build_exec_argv(spec: &RunSpec) -> Vec<String> {
    let Some(session) = &spec.resume else {
        return first_turn_argv(spec);
    };
    resume_argv(&session.id, &spec.cwd)
}

/// Linia pierwszej tury, w kolejności z T1 §8.4.
fn first_turn_argv(spec: &RunSpec) -> Vec<String> {
    let mut argv = vec![
        "exec".to_owned(),
        "--json".to_owned(),
        "--ignore-user-config".to_owned(),
        "--skip-git-repo-check".to_owned(),
        "-C".to_owned(),
        spec.cwd.display().to_string(),
    ];

    // `None` znaczy „to, co vendor ma domyślnie", więc flagi nie ma wcale. Pusty `-m` byłby
    // modelem o nazwie zerowej długości, a to jest co innego niż brak wyboru.
    if let Some(model) = &spec.model {
        argv.push("-m".to_owned());
        argv.push(model.clone());
    }

    // DOKŁADNIE JEDNO `-s`, zawsze. Zero znaczy, że dial nie decyduje o niczym i Codex spada
    // na własną domyślną; dwa znaczą, że wygrywa ostatnie, a kto czyta linię poleceń, ten
    // wierzy pierwszemu.
    argv.push("-s".to_owned());
    argv.push(sandbox_mode(spec.policy).to_owned());

    // Myślnik na końcu jest tym, co każe czytać prompt ze stdinu [T1 §6.1]. Bez niego trzeba by
    // go podać argumentem — czyli złamać niezmiennik 9 dokładnie tak, jak podpowiada T1 §8.4.
    argv.push("-".to_owned());
    argv
}

/// Linia tury wznawiającej [T1 §8.4].
///
/// Czego tu **nie ma i nie ma prawa być**: `-m` i `-s` należą do pierwszej tury (rozmowa ma już
/// swój model i swoją piaskownicę), a `--skip-git-repo-check` razem z nimi — wznawiana rozmowa
/// przeszła tę bramkę raz.
fn resume_argv(thread: &str, cwd: &Path) -> Vec<String> {
    vec![
        "exec".to_owned(),
        "resume".to_owned(),
        thread.to_owned(),
        "--json".to_owned(),
        "--ignore-user-config".to_owned(),
        "-C".to_owned(),
        cwd.display().to_string(),
        "-".to_owned(),
    ]
}

/// Cała tabela tłumaczenia polityki na piaskownicę — **jedna, w adapterze** (niezmiennik 23).
///
/// Trzy warianty po ludzku muszą dojechać do CLI jako trzy **różne** tryby: adapter wypisujący
/// jeden tryb dla wszystkich trzech przechodzi każde sprawdzenie, które pyta tylko, czy flaga
/// jest. Agent, któremu obiecano „No limits", a dano `read-only`, nie zapisze ani linii.
///
/// Czego ta tabela nie ma i nigdy nie będzie miała: `--dangerously-bypass-approvals-and-sandbox`.
/// To nie jest czwarty stopień diala, tylko drzwi obok niego — wyłącza zatwierdzenia **i**
/// piaskownicę naraz. Cicha wersja złamania niezmiennika 23 wygląda inaczej: adapter dokłada
/// sobie własną listę dozwolonych narzędzi „bo Codex ma inne nazwy" i tak właśnie po cichu
/// umarło skanowanie sekretów w repo źródłowym [raport 05 §4].
const fn sandbox_mode(policy: Policy) -> &'static str {
    match policy {
        Policy::ReadOnly => "read-only",
        Policy::EditInFolder => "workspace-write",
        Policy::Unrestricted => "danger-full-access",
    }
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

// ── Pętla czytająca ───────────────────────────────────────────────────────────────────────

/// Opróżnia strumień skarg do EOF i zapamiętuje początek tego, co powiedział.
///
/// **Opróżnia**, a nie „czyta, jeśli ktoś zapyta", i to jest cały powód, dla którego to zadanie
/// istnieje osobno: potok o pojemności ~64 KB, którego nikt nie odbiera, zatrzymuje dziecko na
/// `write`. Bliźniak z `claude.rs` — wspólne miejsce dla obu jest poza blokiem OWNS tego zadania.
///
/// Bez `?` i bez `unwrap` (niezmiennik 5): błąd odczytu skargi nie ma prawa zabrać tury.
/// Zamek brany i oddany w jednym wyrażeniu, nigdy przez `await` (niezmiennik 8).
async fn drain_complaints(stderr: ChildStderr, into: Arc<Mutex<String>>) {
    let mut reader = BufReader::new(stderr);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let mut held = into.lock().unwrap_or_else(PoisonError::into_inner);
        if held.len() < COMPLAINT_KEPT {
            held.push_str(&line);
        }
        // Bez `break` po przekroczeniu limitu: pętla musi dalej OPRÓŻNIAĆ potok, nawet gdy nic
        // już nie zapamiętuje. Wyjście tutaj przywróciłoby dokładnie tę blokadę, przed którą
        // to zadanie stoi.
    }
}

/// Czyta strumień zdarzeń jednej tury linia po linii i sypie zdarzeniami aż do jego końca.
///
/// **Nie ma tu `?` i to nie jest przeoczenie** (niezmiennik 5): jedyny sposób, żeby nieznana
/// linia zabiła turę, to zwrócić z tej pętli błąd. Dekoder oddaje pusty wektor, a pętla leci
/// dalej — a prawdziwy bieg Codeksa przeplótł ten strumień liniami `ERROR rmcp::transport::
/// worker: …` [T2 §9.3, zweryfikowane zagrożenie].
///
/// Zdarzenie końca pada **zawsze**, także wtedy, gdy tura nie powiedziała ani słowa: krok bez
/// niego wisiałby w `running` do końca biegu.
async fn pump(
    stdout: ChildStdout,
    events: mpsc::Sender<DecodedEvent>,
    outcome: oneshot::Sender<Outcome>,
    threads: Arc<Mutex<Vec<String>>>,
    number: u64,
    cancelled: Arc<AtomicU64>,
    complaint: Arc<Mutex<String>>,
) {
    // Zegar startuje TU, a nie w dekoderze: Codex nie mówi, ile trwała tura, więc jedyna
    // uczciwa liczba jest tą, którą zmierzyliśmy sami (2026-08-19). Zero w tym polu wypisałoby
    // na ekranie „0s" przy każdym kroku — to ta sama klasa kłamstwa co `$0.00` przy koszcie.
    let began = Instant::now();
    let mut reader = BufReader::new(stdout);
    let mut decoder = CodexDecoder::new();
    let mut buffer: Vec<u8> = Vec::with_capacity(8 * 1024);
    let mut told = Some(outcome);
    let mut seen: Option<String> = None;

    loop {
        buffer.clear();
        // `read_until`, nie `lines()`: `lines()` przewraca się na bajtach nie-UTF-8, a linia,
        // której nie da się przeczytać, ma zostać POLICZONA, a nie urwać czytanie.
        match reader.read_until(b'\n', &mut buffer).await {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) => {
                tracing::debug!(%error, "the agent output stream broke off");
                break;
            }
        }

        // `from_utf8_lossy`, żeby KAŻDA linia doszła do dekodera: uszkodzona nie sparsuje się
        // jako JSON i wpadnie do licznika porzuconych, zamiast zniknąć przed policzeniem.
        // Bajtowa identyczność nie jest tu wymaganiem, bo tee na dysk należy do T-05 i ten
        // sterownik go nie ma (patrz nagłówek pliku).
        let line = String::from_utf8_lossy(&buffer);
        let produced = decoder.push(&line);
        remember_thread(&decoder, &mut seen, &threads);

        for event in produced {
            emit(event, began, &events, &mut told).await;
        }
    }

    // Skargę czytamy DOPIERO TERAZ, po EOF na wyjściu: proces, który się przewrócił, pisze ją,
    // zanim zamknie strumień zdarzeń, więc w tej chwili buforek ma już to, co miał do
    // powiedzenia. Zamek brany i oddany w JEDNYM wyrażeniu (niezmiennik 8).
    let said = complaint
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let stopped_by_a_person = cancelled.load(Ordering::SeqCst) == number;
    if let Some(event) = decoder.end_of_stream(stopped_by_a_person, &said) {
        emit(event, began, &events, &mut told).await;
    }

    if decoder.dropped() > 0 {
        tracing::debug!(
            dropped = decoder.dropped(),
            turn = number,
            "lines of the agent stream produced nothing and were let go"
        );
    }

    // Nadajniki giną RAZEM Z TĄ PĘTLĄ i to jest ich druga robota: zamknięty kanał jest jedynym
    // sygnałem, po którym odbiorca wie, że nic już nie przyjdzie.
    drop(events);
    drop(told);
}

/// Dopisuje identyfikator wątku do wspólnej pamięci sesji, jeśli jest nowy.
///
/// Powtórzenie tego samego identyfikatora **nie** dokłada wiersza: lista odpowiada na pytanie
/// „czy vendor przestawił uchwyt", a ten sam numer powtórzony trzy razy nie jest przestawieniem.
///
/// 2026-08-19 — ROZBIEŻNOŚĆ ZAPISUJEMY RAZ, przy turze, w której powstała. T1 §11 pytanie 5 nie
/// rozstrzyga, czy `codex exec resume` mintuje nowy identyfikator, więc kiedy vendor odda inny
/// niż tożsamość sesji, to jest fakt wart jednego wiersza w dzienniku — i dokładnie jednego,
/// bo wiersz na każdą linię strumienia zamieniłby go w szum.
fn remember_thread(
    decoder: &CodexDecoder,
    seen: &mut Option<String>,
    threads: &Mutex<Vec<String>>,
) {
    let Some(id) = decoder.thread() else {
        return;
    };
    if seen.as_deref() == Some(id) {
        return;
    }
    seen.replace(id.to_owned());

    let mut held = threads.lock().unwrap_or_else(PoisonError::into_inner);
    if held.last().map(String::as_str) == Some(id) {
        return;
    }
    let identity = held.first().cloned();
    held.push(id.to_owned());
    drop(held);

    if let Some(identity) = identity
        && identity != id
    {
        tracing::info!(
            session = %identity,
            handed_back = %id,
            "the agent answered with a different thread id than the one this session is known by; \
             the session keeps its first id and the next turn resumes the newest"
        );
    }
}

/// Wypuszcza jedno zdarzenie — **najpierw** do [`AgentHandle::wait`], potem na ekran.
///
/// Ta kolejność jest jedyną obroną przed wolnym konsumentem: kanał zdarzeń z pełnym buforem
/// zatrzymuje wysyłkę, a wynik tury, który utknął za nim, wygląda jak zawieszony agent.
async fn emit(
    event: AgentEvent,
    began: Instant,
    events: &mpsc::Sender<DecodedEvent>,
    told: &mut Option<oneshot::Sender<Outcome>>,
) {
    let mut event = event;
    if let AgentEvent::Finished(outcome) = &mut event {
        // Czas mierzony przez nas, bo vendor go nie podaje (powód przy starcie zegara w [`pump`]).
        outcome.took = began.elapsed();
        if let Some(tell) = told.take() {
            let _ = tell.send(outcome.clone());
        }
    }
    // Fakt o narzędziu jedzie tu jako `None` i to jest ZGŁOSZONA dziura, nie przeoczenie:
    // buduje go `stream::decode` z tej samej linii drutu, a `stream.rs` należy do T-05 i leży
    // poza blokiem OWNS tego zadania. Skutek jest wąski i nazwany: transkrypt kroku Codeksa
    // pokaże prozę agenta, ale nie wiersze `read`, `edit` ani `ran` — dokładnie ta sama awaria,
    // którą u Claude'a zmierzono 2026-08-18 i naprawiono przez [`DecodedEvent`].
    let _ = events.send(event.into()).await;
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

/// Żywa sesja `codex` — **wiele procesów**, jedna tożsamość.
///
/// To jest cała różnica wobec `ClaudeHandle`, w którym proces jest jeden na całą sesję. Tura
/// druga i każda następna to `codex exec resume <thread_id>`, czyli świeży proces, zimny start
/// i odbudowa cache'u [T1 §8.1] — świadomy koszt, nie brak.
#[derive(Debug)]
pub struct CodexHandle {
    /// Co uruchamiamy w kolejnych turach. Kopia z [`CodexDriver`], bo uchwyt przeżywa sterownik.
    binary: PathBuf,
    /// Katalog roboczy tej rozmowy. Kolejne tury dostają go z powrotem w `-C`.
    cwd: PathBuf,
    /// Kanał zdarzeń tej sesji. **Wszystkie** tury sypią w ten sam, bo z zewnątrz to jedna
    /// rozmowa — proces na turę jest szczegółem, który trait ma wchłonąć.
    events: mpsc::Sender<DecodedEvent>,
    /// Każdy `thread_id`, jaki ta sesja dostała, w kolejności przybycia. Pierwszy jest
    /// tożsamością, ostatni jest celem wznowienia.
    ///
    /// 2026-08-19 — TO POLE ISTNIEJE, BO T1 §11 PYTANIE 5 JEST OTWARTE: nie wiadomo, czy
    /// `codex exec resume` oddaje ten sam identyfikator, czy mintuje nowy. Dopóki nie wiadomo,
    /// sterownik nie ma prawa **zakładać** żadnej z dwóch odpowiedzi: trzyma obie liczby
    /// i zachowuje się poprawnie w obu przypadkach.
    ///
    /// Dzielone, bo pisze to pętla czytająca, a czyta uchwyt — i czyta **w trakcie** tury, nie
    /// po niej: [`AgentHandle::session`] ma odpowiadać prawdę od chwili, w której vendor ogłosił
    /// identyfikator, bo to ją T-06 zapisuje przy kroku. Zamek brany i oddawany w jednym
    /// wyrażeniu, nigdy przez `await` (niezmiennik 8).
    threads: Arc<Mutex<Vec<String>>>,
    /// Numer tury, którą anulowano — **generacja**, nie znacznik logiczny.
    ///
    /// Niezmiennik 7 czyta się tu dosłownie: `AtomicBool` przeciekłby między turami, bo sesja
    /// Codeksa ma ich wiele, a znacznik podniesiony przy turze pierwszej kazałby turze drugiej
    /// zameldować „człowiek nacisnął Stop", choć nikt niczego nie nacisnął. Liczba nie przecieka:
    /// pętla czytająca tury N pyta, czy anulowano dokładnie N. [`NOT_CANCELLED`] nie jest
    /// numerem żadnej tury, bo numeracja zaczyna się od [`FIRST_TURN`].
    cancelled: Arc<AtomicU64>,
    /// Która tura trwa albo skończyła się ostatnio.
    number: u64,
    /// Proces **bieżącej** tury. `None` dopiero po [`AgentHandle::close`] — między turami
    /// zostaje tu proces poprzedniej, zebrany, żeby nie został po nim zombie.
    process: Option<Supervised>,
    /// Obietnica wyniku bieżącej tury. `None` znaczy „ta tura została już odebrana", i to jest
    /// jedyny stan, w którym wolno zacząć następną.
    ///
    /// `oneshot`, a nie kanał: tura ma dokładnie jeden wynik, a nadajnik ginący razem z pętlą
    /// czytającą zamienia „pętla padła" w `Err` zamiast w czekanie bez końca.
    outcome: Option<oneshot::Receiver<Outcome>>,
}

impl CodexHandle {
    /// Identyfikatory wątku, które ta sesja zobaczyła — pierwszy z przodu.
    ///
    /// Czyta to kryterium o wznowieniu i **nikt poza nim** nie musi (niezmiennik 21): sama
    /// tożsamość jedzie przez [`AgentHandle::session`], a cel wznowienia sterownik zna sam.
    /// Tu chodzi o różnicę między „widzieliśmy dwa identyfikatory i pamiętamy oba" a „drugi
    /// nadpisał pierwszy", której z zewnątrz nie da się inaczej odróżnić.
    ///
    /// **Migawka, nie pożyczka** (2026-08-19). Szkielet oddawał `&[String]`, bo miał jednego
    /// pisarza i żadnego czytelnika. Odkąd pisze to pętla czytająca, lista siedzi za zamkiem,
    /// a pożyczki zza zamka nie da się oddać na zewnątrz — kopia trzech napisów raz na turę jest
    /// tańsza niż jakikolwiek sposób, żeby tego uniknąć.
    #[must_use]
    pub fn threads_seen(&self) -> Vec<String> {
        self.threads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Identyfikator, który wznowi kolejna tura: **najnowszy**, bo to jego vendor potwierdził
    /// ostatnio.
    fn newest_thread(&self) -> Option<String> {
        self.threads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .last()
            .cloned()
    }
}

#[async_trait]
impl AgentHandle for CodexHandle {
    /// Tożsamość tej rozmowy, czyli identyfikator z **pierwszego** `thread.started`.
    ///
    /// Nigdy nie przestawiany w trakcie sesji, choć vendor bywa innego zdania w każdej turze.
    /// Cicha porażka numer jeden tego zadania wygląda dokładnie odwrotnie: sterownik mintuje
    /// nowy `SessionRef` przy każdej turze, bo przecież `thread.started` przyszło znowu — szyna
    /// pokazuje wtedy trzech agentów zamiast jednego, trzy podsumowania „Done", trzy koszty,
    /// i **wszystko wygląda na skończone**, więc nikt tego nie zgłosi.
    ///
    /// Pusty identyfikator znaczy „pierwsza linia jeszcze nie przyszła", a nie „nie ma sesji".
    fn session(&self) -> SessionRef {
        SessionRef {
            vendor: VENDOR,
            id: self
                .threads
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .first()
                .cloned()
                .unwrap_or_default(),
        }
    }

    /// Grupa procesów **bieżącej** tury.
    ///
    /// `None` dopiero po zamknięciu sesji i to nie jest brak: przy sterowniku z procesem na turę
    /// naprawdę bywa chwila, w której nie ma czego zabić. `ClaudeHandle` oddaje tu zawsze `Some`,
    /// bo tam proces żyje przez całą sesję — i to jest ta różnica, którą trait ma wchłonąć.
    fn group(&self) -> Option<GroupId> {
        self.process.as_ref().map(Supervised::group)
    }

    /// Kolejna tura: **nowy proces** z `codex exec resume <thread_id>` i promptem na stdin.
    ///
    /// Wznawiamy po **najnowszym** identyfikatorze, nie po tożsamości sesji: T1 §11 pytanie 5 nie
    /// rozstrzyga, czy `resume` mintuje nowy, więc sterownik ma być poprawny w obu przypadkach —
    /// a najnowszy jest tym, który vendor potwierdził ostatnio. Wznawianie po pierwszym byłoby
    /// sterownikiem, który założył jedną z dwóch odpowiedzi.
    async fn send(&mut self, text: String) -> anyhow::Result<()> {
        if self.outcome.is_some() {
            anyhow::bail!(
                "a follow-up turn of {} bytes has nowhere to go yet: the previous turn has not \
                 been collected, and codex exec has no way to take two at once - it reads one \
                 prompt, answers it and exits",
                text.len()
            );
        }

        let Some(thread) = self.newest_thread() else {
            anyhow::bail!(
                "a follow-up turn of {} bytes has nothing to resume: this session never heard a \
                 thread id, and that id is the only handle codex exec resume takes",
                text.len()
            );
        };

        // Zebranie poprzedniego procesu jest częścią tury, nie sprzątaniem po niej: zombie NADAL
        // odpowiada na sygnał zerowy, więc grupa z zombie w środku nigdy nie da `ESRCH`
        // (niezmiennik 6).
        if let Some(previous) = self.process.as_mut() {
            let _reaped = previous.wait().await;
        }

        self.number += 1;
        let turn = Turn {
            binary: self.binary.clone(),
            cwd: self.cwd.clone(),
            argv: resume_argv(&thread, &self.cwd),
            prompt: text,
            events: self.events.clone(),
            threads: Arc::clone(&self.threads),
            number: self.number,
            cancelled: Arc::clone(&self.cancelled),
        };
        let (process, outcome) = turn.start()?;

        // Podmiana, nie dopisanie: stary uchwyt ginie tutaj, a jego `Drop` jest ostatnią linią
        // obrony przed wyciekiem grupy.
        self.process = Some(process);
        self.outcome = Some(outcome);
        Ok(())
    }

    /// Czeka na koniec bieżącej tury.
    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        let told = self.outcome.take().ok_or_else(|| {
            anyhow!("this session has no turn in flight, so there is no outcome to wait for")
        })?;
        let outcome = told
            .await
            .map_err(|_| anyhow!("the turn ended without ever saying how it went"))?;

        // Zebranie procesu MUSI paść na każdej ścieżce terminalnej — powód przy `send`.
        if let Some(process) = self.process.as_mut() {
            let _reaped = process.wait().await;
        }
        Ok(outcome)
    }

    /// Anuluje turę i **dowodzi**, że po grupie nic nie zostało.
    ///
    /// Eskalacja jest w całości z `engine/supervisor.rs` (niezmiennik 3): SIGTERM na grupę,
    /// łaska, SIGKILL, a potem pętla dowodowa aż do `ESRCH`. Stopnia „przerwanie w paśmie" tu
    /// nie ma i nie będzie — `codex exec` nie czyta stdinu po pierwszym prompcie [T1 §6.4].
    ///
    /// Generacja idzie w górę **przed** sygnałem i to nie jest kwestia porządku: pętla czytająca
    /// pyta o nią dopiero na EOF, a EOF przychodzi zaraz po zabiciu — znacznik postawiony po
    /// sygnale bywa spóźniony, a wtedy „człowiek nacisnął Stop" melduje się jako „agent się
    /// przewrócił" (niezmiennik 7 złamany o jedną instrukcję).
    async fn cancel(&mut self) -> GroupProof {
        self.cancelled.store(self.number, Ordering::SeqCst);

        let Some(process) = self.process.as_mut() else {
            // Sesja bez procesu nie ma czego zabić i nie ma czego palić w tle. `Alive` posłałoby
            // wołającego po grupę, której nie ma; `Dead` mówi to, co jest prawdą — nie zostało
            // nic. Statusu nie ma, bo nie było czyjego odebrać.
            return GroupProof::Dead { status: None };
        };
        process.stop(DEFAULT_GRACE).await
    }

    /// Koniec sesji: czeka, aż bieżąca tura wyjdzie **sama**.
    ///
    /// Wejścia nie ma tu czego zamykać — `codex exec` dostał EOF razem z promptem, bo bez niego
    /// w ogóle by nie ruszył. To jest ta połowa kontraktu, którą Codex spełnia za darmo, i ta
    /// sama, przez którą traci wielotury w jednym procesie.
    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        let Some(process) = self.process.as_mut() else {
            return Ok(None);
        };
        let status = process.wait().await?;
        // `None` znaczy „proces zginął od sygnału i kodu po prostu nie ma" — to jest ta sama
        // różnica, którą mierzy dowód z `cancel()`.
        Ok(status.code())
    }
}

#[async_trait]
impl AgentDriver for CodexDriver {
    fn id(&self) -> &'static str {
        VENDOR
    }

    /// Pyta binarkę o wersję. **Brak pliku to `Ok(Probe { found: false, .. })`, nigdy `Err`**:
    /// nieobecne CLI jest ekranem ustawień, a nie awarią startu aplikacji.
    ///
    /// Najprościej, jak się da, i to jest świadome — ekranu ustawień na tym nie budujemy
    /// („Świadomie poza zakresem"). Nieudany start jest odpowiedzią w **każdej** postaci, nie
    /// tylko przy braku pliku: binarka bez prawa wykonania i binarka, której nie ma, znaczą dla
    /// użytkownika dokładnie to samo zdanie.
    async fn probe(&self) -> anyhow::Result<Probe> {
        let mut command = Command::new(&self.binary);
        command.arg("--version");

        // Przez ten sam start co bieg, a nie własną komendą obok: `env_clear()` plus jawna lista
        // przepuszczanych zmiennych mieszka w jednym rdzeniu (niezmiennik 23), a `/dev/null` na
        // wejściu oszczędza czekanie na EOF, którego nikt by nie wysłał.
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
        let _reaped = process.wait().await;

        Ok(Probe {
            found: true,
            version,
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
