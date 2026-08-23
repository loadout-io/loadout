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
//! **Nie buduje faktów o czynności** — robi to `stream::decode_codex` z tej samej linii drutu
//! i stamtąd jadą one do kuratora (T-97, 2026-08-24). Druga tabela nazw z drutu po tej stronie
//! byłaby drugą implementacją kuracji (niezmienniki 15 i 23), więc ten plik zostaje tabelą
//! „co znaczy które zdarzenie" i niczym więcej. Do 2026-08-24 nie robił tego **nikt** i skutek
//! był dokładnie taki, jak zapowiadał ten akapit: transkrypt kroku Codeksa pokazywał prozę
//! agenta i ani jednego wiersza `read`, `edit` czy `ran`.
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

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use anyhow::anyhow;
use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::timeout;

use super::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, DriverConfiguration, FinishReason, Outcome,
    Policy, Probe, RunSpec, SessionRef, Tokens, ValidatedImages,
};
use crate::engine::stream;
use crate::engine::supervisor::{self, DEFAULT_GRACE, GroupId, GroupProof, StdinPlan, Supervised};
use crate::evidence::{EvidenceStreams, EvidenceTarget, EvidenceWriter};

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

/// App Server gets one chance to acknowledge an in-band interrupt before the supervisor takes
/// over. The supervisor still runs afterwards: a JSON-RPC response proves only that the request
/// was read, never that the process group is dead (invariants 6 and 10).
const APP_INTERRUPT_WINDOW: Duration = Duration::from_secs(2);

/// Commands waiting between a Lead handle and its one stdin owner. The protocol is sequential at
/// the product boundary, but a small reserve prevents stderr/stdout draining from depending on a
/// consumer polling at precisely the right moment.
const APP_COMMAND_CAPACITY: usize = 16;

/// Outcomes are one-at-a-time by contract. A reserve lets the reader finish emitting before the
/// UI begins awaiting the result.
const APP_OUTCOME_CAPACITY: usize = 4;

/// Odstęp między ponowieniami eskalacji na ścieżce startu, która nie ma już komu oddać
/// uchwytu. `Alive` nie może wyjść z tej funkcji razem z jedynym właścicielem procesu.
const START_CLEANUP_RETRY: Duration = Duration::from_secs(1);

/// Tylko ten wariant pozwala porzucic uchwyt procesu i joiny czytnikow. `Alive` nie znaczy
/// „stop sie nie udal, posprzataj mimo to", tylko „jadro nadal widzi grupe" — wiec caly stan
/// musi zostac w handle, aby kolejne Stop moglo ponowic eskalacje (niezmiennik 6).
fn proof_allows_cleanup(proof: &GroupProof) -> bool {
    matches!(proof, GroupProof::Dead { .. })
}

fn mark_evidence_incomplete(target: Option<&EvidenceTarget>) {
    if let Some(target) = target {
        target.mark_incomplete();
    }
}

/// Fatalna porażka startu nie ma zewnętrznego uchwytu, który mógłby ponowić Stop. Dlatego ten
/// właściciel zostaje w funkcji tak długo, aż supervisor przyniesie rzeczywisty dowód `Dead`.
async fn stop_startup_process(process: &mut Supervised) -> GroupProof {
    loop {
        let proof = process.stop(DEFAULT_GRACE).await;
        if proof_allows_cleanup(&proof) {
            return proof;
        }
        tracing::error!(
            "the Codex App Server is still alive after a failed start; Loadout retains its handle \
             and will retry"
        );
        tokio::time::sleep(START_CLEANUP_RETRY).await;
    }
}

/// Sterownik `codex`.
///
/// Ścieżka do binarki jest **polem**, nie stałą, i to jest jedyny szew, przez który kryteria
/// wpuszczają skrypt-atrapę zamiast prawdziwego CLI — inaczej żadnego z nich nie dałoby się
/// uruchomić bez konta i bez sieci.
#[derive(Clone)]
pub struct CodexDriver {
    /// Co uruchamiamy.
    binary: PathBuf,
    /// Prywatny target dowodow gotowy dla jednej logicznej sesji.
    evidence: Option<EvidenceTarget>,
    /// Konfiguracja Connections jednego kroku; Debug pokazuje tylko nazwy środowiska.
    configuration: DriverConfiguration,
}

impl fmt::Debug for CodexDriver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexDriver")
            .field(
                "uses_custom_binary",
                &(self.binary != Path::new(DEFAULT_BINARY)),
            )
            .field("has_evidence", &self.evidence.is_some())
            .finish_non_exhaustive()
    }
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
            evidence: None,
            configuration: DriverConfiguration::default(),
        }
    }

    /// Sterownik wołający konkretny plik. Szew dla kryteriów, które uruchamiają prawdziwy
    /// proces — i dla użytkownika, który trzyma CLI poza `PATH`.
    #[must_use]
    pub fn with_binary(binary: PathBuf) -> Self {
        Self {
            binary,
            evidence: None,
            configuration: DriverConfiguration::default(),
        }
    }

    #[must_use]
    pub fn with_configuration(mut self, configuration: DriverConfiguration) -> Self {
        self.configuration = configuration;
        self
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
        mut spec: RunSpec,
        tx: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<CodexHandle> {
        // Dowody otwieramy PRZED spawnem. Nieudany zapis manifestu nie moze dac procesu, ktory
        // wykonal prace bez odbudowywalnego sladu, a wczesne `?` nie zostawia wtedy grupy.
        let evidence = match &self.evidence {
            Some(target) => Some(target.open().await?),
            None => None,
        };
        let (stdout_evidence, stderr_evidence) = split_evidence(evidence);
        let argv = exec_argv(&self.configuration, &spec);
        // Wyjete TU, a nie przy `Turn`: `spec.prompt` idzie tam przeniesieniem, wiec
        // pozyczka `spec.system_append` w tym samym wyrazeniu nie ma prawa istniec.
        // `take()` zamiast `clone()` — instrukcje agenta bywaja akapitami.
        let instructions = spec.system_append.take();

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
            prompt: after_the_standing_orders(instructions.as_deref(), spec.prompt),
            events: tx.clone(),
            threads: Arc::clone(&threads),
            number: FIRST_TURN,
            cancelled: Arc::clone(&cancelled),
            stdout_evidence,
            stderr_evidence,
            evidence_target: self.evidence.clone(),
            configuration: self.configuration.clone(),
        };
        let started = turn.start();
        if started.is_err() {
            mark_evidence_incomplete(self.evidence.as_ref());
        }
        let (process, outcome, drained, stderr_task) = started?;

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
            evidence: self.evidence.clone(),
            threads,
            cancelled,
            number: FIRST_TURN,
            process: Some(process),
            outcome: Some(outcome),
            drained: Some(drained),
            stderr_task: Some(stderr_task),
            configuration: self.configuration.clone(),
        })
    }
}

/// Wszystko, czego potrzeba, żeby ruszyć **jedną** turę Codeksa.
///
/// Istnieje jako typ, a nie jako osiem argumentów funkcji, bo tur jest wiele i każda startuje
/// dokładnie tak samo: [`CodexDriver::start_session`] robi pierwszą, [`AgentHandle::send`] każdą
/// następną. Dwa miejsca składające ten sam start osobno rozjeżdżają się przy pierwszej zmianie
/// — a rozjazd byłby cichy, bo obie drogi dalej uruchamiałyby proces.
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
    /// Surowy stdout workflow; Lead ma osobna, filtrowana petle App Servera.
    stdout_evidence: Option<EvidenceWriter>,
    /// Surowy stderr workflow, zawsze bajt w bajt.
    stderr_evidence: Option<EvidenceWriter>,
    /// Wspólny bezpiecznik kompletu; błędy odczytu i kanałów są utratą dowodu nawet wtedy,
    /// gdy sam deskryptor pliku nadal przyjmuje bajty.
    evidence_target: Option<EvidenceTarget>,
    configuration: DriverConfiguration,
}

type StartedTurn = (
    Supervised,
    oneshot::Receiver<Outcome>,
    oneshot::Receiver<()>,
    JoinHandle<()>,
);

struct PumpInput {
    stdout: ChildStdout,
    events: mpsc::Sender<DecodedEvent>,
    outcome: oneshot::Sender<Outcome>,
    threads: Arc<Mutex<Vec<String>>>,
    number: u64,
    cancelled: Arc<AtomicU64>,
    complaint: Arc<Mutex<String>>,
    evidence: Option<EvidenceWriter>,
    evidence_target: Option<EvidenceTarget>,
    drained: oneshot::Sender<()>,
}

impl fmt::Debug for Turn {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Turn")
            .field(
                "uses_custom_binary",
                &(self.binary != Path::new(DEFAULT_BINARY)),
            )
            .field("workspace", &"<private>")
            .field("argument_count", &self.argv.len())
            .field("prompt_bytes", &self.prompt.len())
            .field("number", &self.number)
            .field("has_evidence", &self.stdout_evidence.is_some())
            .finish_non_exhaustive()
    }
}

impl Turn {
    /// Startuje proces tury i oddaje uchwyt do niego oraz obietnicę jej wyniku.
    ///
    /// Proces startuje przez `engine::supervisor::spawn` i **tylko** przez nie: własna grupa
    /// procesów, `env_clear()` i cała eskalacja zabijania mieszkają tam (niezmienniki 3 i 23).
    /// Ten plik nie zna ani jednej stałej sygnału.
    fn start(self) -> anyhow::Result<StartedTurn> {
        let mut command = Command::new(&self.binary);
        // Katalog roboczy przychodzi ARGUMENTEM, nigdy stałą: literał ze ścieżką repo w pliku
        // pod `engine/` przewraca granicę z niezmiennika 1.
        command.current_dir(&self.cwd);
        command.args(&self.argv);

        // `Write`, nie `Keep`: po prompcie deskryptor się ZAMYKA, bo to zamknięcie jest tym
        // EOF-em, na który `codex exec` czeka. `Keep` zostawiłby proces wiszący na wejściu,
        // które nigdy się nie skończy — i wyglądałoby to jak agent, który myśli.
        let mut process = supervisor::spawn_with_environment(
            command,
            StdinPlan::Write(self.prompt),
            &self.configuration.environment,
        )?;

        let Some(stdout) = process.stdout() else {
            mark_evidence_incomplete(self.evidence_target.as_ref());
            return Err(anyhow!(
                "the agent started without an output stream to read"
            ));
        };

        // SKARGI ODBIERAMY I OPRÓŻNIAMY. Potok o pojemności ~64 KB, którego nikt nie odbiera,
        // zatrzymuje dziecko na `write` — czyli agent gadatliwy poza strumieniem zdarzeń wisi,
        // a z okna wygląda to jak agent, który myśli. Drugi powód jest w [`CodexDecoder::
        // end_of_stream`]: pierwsza linia skargi odpowiada na „dlaczego" w praktycznie każdym
        // realnym przypadku, a bez niej krok pada zdaniem bez przyczyny.
        let complaint = Arc::new(Mutex::new(String::new()));
        let stderr_task = if let Some(stderr) = process.stderr() {
            let into = Arc::clone(&complaint);
            tokio::spawn(drain_complaints(
                stderr,
                into,
                self.stderr_evidence,
                self.evidence_target.clone(),
            ))
        } else {
            mark_evidence_incomplete(self.evidence_target.as_ref());
            tokio::spawn(close_evidence(self.stderr_evidence))
        };

        let (tell, told) = oneshot::channel();
        let (drained_tx, drained_rx) = oneshot::channel();
        // Pętla czytająca żyje własnym zadaniem: uchwyt ma zostać responsywny na `cancel()`
        // także wtedy, gdy nikt nie woła `wait()`.
        let _reader = tokio::spawn(pump(PumpInput {
            stdout,
            events: self.events,
            outcome: tell,
            threads: self.threads,
            number: self.number,
            cancelled: self.cancelled,
            complaint,
            evidence: self.stdout_evidence,
            evidence_target: self.evidence_target,
            drained: drained_tx,
        }));

        Ok((process, told, drained_rx, stderr_task))
    }
}

fn split_evidence(
    evidence: Option<EvidenceStreams>,
) -> (Option<EvidenceWriter>, Option<EvidenceWriter>) {
    match evidence {
        Some(EvidenceStreams { stdout, stderr }) => (Some(stdout), Some(stderr)),
        None => (None, None),
    }
}

// ── Lead przez Codex App Server ───────────────────────────────────────────────────────────

/// Jedyny wlasciciel JSON-RPC stdinu. Zaden wariant nie implementuje `Debug`: request tury
/// zawiera prompt oraz data URL obrazu i nie ma bezpiecznej reprezentacji tekstowej.
enum AppCommand {
    Request {
        id: u64,
        method: &'static str,
        body: Vec<u8>,
        begins_turn: bool,
        reply: oneshot::Sender<anyhow::Result<Value>>,
    },
    Notify {
        body: Vec<u8>,
        reply: oneshot::Sender<anyhow::Result<()>>,
    },
    SetThread {
        id: String,
        reply: oneshot::Sender<()>,
    },
    MarkCancelled,
    Close {
        reply: oneshot::Sender<anyhow::Result<()>>,
    },
}

struct PendingAppRequest {
    method: &'static str,
    reply: oneshot::Sender<anyhow::Result<Value>>,
}

/// Klonowalny glos do jednego aktora App Servera. Licznik jest atomowy tylko dlatego, ze
/// przerwanie i zwykla tura moga przyjsc z roznych taskow; nie przechowuje stanu anulowania.
#[derive(Clone)]
struct AppClient {
    commands: mpsc::Sender<AppCommand>,
    next_id: Arc<AtomicU64>,
    evidence: Option<EvidenceTarget>,
}

impl fmt::Debug for AppClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AppClient")
            .field("next_id", &self.next_id.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl AppClient {
    fn new(commands: mpsc::Sender<AppCommand>, evidence: Option<EvidenceTarget>) -> Self {
        Self {
            commands,
            next_id: Arc::new(AtomicU64::new(1)),
            evidence,
        }
    }

    fn mark_incomplete(&self) {
        mark_evidence_incomplete(self.evidence.as_ref());
    }

    async fn request(
        &self,
        method: &'static str,
        params: Value,
        begins_turn: bool,
    ) -> anyhow::Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let body = serde_json::to_vec(&json!({
            "id": id,
            "method": method,
            "params": params,
        }))
        .map_err(|_| {
            self.mark_incomplete();
            anyhow!("Loadout could not encode a Codex App Server request.")
        })?;
        let (reply, response) = oneshot::channel();
        self.commands
            .send(AppCommand::Request {
                id,
                method,
                body,
                begins_turn,
                reply,
            })
            .await
            .map_err(|_| {
                self.mark_incomplete();
                anyhow!("The Codex App Server input channel closed.")
            })?;
        let answer = response.await.map_err(|_| {
            self.mark_incomplete();
            anyhow!("The Codex App Server stopped before it answered.")
        })?;
        if answer.is_err() {
            self.mark_incomplete();
        }
        answer
    }

    async fn notify(&self, method: &'static str, params: Value) -> anyhow::Result<()> {
        let body = serde_json::to_vec(&json!({
            "method": method,
            "params": params,
        }))
        .map_err(|_| {
            self.mark_incomplete();
            anyhow!("Loadout could not encode a Codex App Server notification.")
        })?;
        let (reply, response) = oneshot::channel();
        self.commands
            .send(AppCommand::Notify { body, reply })
            .await
            .map_err(|_| {
                self.mark_incomplete();
                anyhow!("The Codex App Server input channel closed.")
            })?;
        let answer = response.await.map_err(|_| {
            self.mark_incomplete();
            anyhow!("The Codex App Server stopped before reading its input.")
        })?;
        if answer.is_err() {
            self.mark_incomplete();
        }
        answer
    }

    async fn set_thread(&self, id: String) -> anyhow::Result<()> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(AppCommand::SetThread { id, reply })
            .await
            .map_err(|_| {
                self.mark_incomplete();
                anyhow!("The Codex App Server stopped before opening its thread.")
            })?;
        response.await.map_err(|_| {
            self.mark_incomplete();
            anyhow!("The Codex App Server stopped before opening its thread.")
        })
    }

    async fn mark_cancelled(&self) {
        if self.commands.send(AppCommand::MarkCancelled).await.is_err() {
            self.mark_incomplete();
        }
    }

    async fn close(&self) -> anyhow::Result<()> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(AppCommand::Close { reply })
            .await
            .map_err(|_| {
                self.mark_incomplete();
                anyhow!("The Codex App Server input channel already closed.")
            })?;
        let answer = response.await.map_err(|_| {
            self.mark_incomplete();
            anyhow!("The Codex App Server stopped while closing its input.")
        })?;
        if answer.is_err() {
            self.mark_incomplete();
        }
        answer
    }
}

/// Stan kuracji App Servera. Uzywa tych samych metod `CodexDecoder`, ktore obsluguja `exec`;
/// koperta JSON-RPC jest jedyna roznica transportowa.
struct AppServerState {
    decoder: CodexDecoder,
    active: bool,
    cancelled: bool,
    began: Instant,
    cumulative: Tokens,
    baseline: Tokens,
}

impl fmt::Debug for AppServerState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AppServerState")
            .field("active", &self.active)
            .field("cancelled", &self.cancelled)
            .field("dropped", &self.decoder.dropped())
            .finish_non_exhaustive()
    }
}

impl AppServerState {
    fn new() -> Self {
        Self {
            decoder: CodexDecoder::new(),
            active: false,
            cancelled: false,
            began: Instant::now(),
            cumulative: Tokens::default(),
            baseline: Tokens::default(),
        }
    }

    fn begin_turn(&mut self) {
        self.decoder.ended = false;
        self.decoder.said.clear();
        self.active = true;
        self.cancelled = false;
        self.began = Instant::now();
        self.baseline = self.cumulative;
    }

    fn set_thread(&mut self, id: String) {
        self.decoder.thread = Some(id);
    }

    fn mark_cancelled(&mut self) {
        self.cancelled = true;
    }

    fn notification(&mut self, value: &Value) -> Vec<AgentEvent> {
        let Some(method) = value.get("method").and_then(Value::as_str) else {
            self.decoder.dropped += 1;
            return Vec::new();
        };

        match method {
            "turn/started" | "thread/started" => Vec::new(),
            "thread/tokenUsage/updated" => {
                if let Some(total) = app_usage(value) {
                    self.cumulative = total;
                }
                Vec::new()
            }
            "item/started" if self.active => {
                app_item(value).map(CodexDecoder::begun).unwrap_or_default()
            }
            "item/completed" if self.active => app_item(value)
                .map(|item| self.decoder.completed(item))
                .unwrap_or_default(),
            "turn/completed" if self.active => self.complete_turn(value),
            "error" => CodexDecoder::notice(
                value
                    .pointer("/params/error/message")
                    .or_else(|| value.pointer("/params/message"))
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            ),
            // Nieznany payload jest liczony i porzucany. Nigdy nie trafia do dowodow, bo moze
            // byc nowym wariantem odbicia userMessage z promptem albo obrazem.
            _ => {
                self.decoder.dropped += 1;
                Vec::new()
            }
        }
    }

    fn complete_turn(&mut self, value: &Value) -> Vec<AgentEvent> {
        self.active = false;
        let status = value
            .pointer("/params/turn/status")
            .and_then(Value::as_str)
            .unwrap_or("completed");

        if self.cancelled || status.eq_ignore_ascii_case("interrupted") {
            return self.decoder.end_of_stream(true, "");
        }
        if status.eq_ignore_ascii_case("failed") {
            let message = value
                .pointer("/params/turn/error/message")
                .or_else(|| value.pointer("/params/error/message"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            return self.decoder.failed(message);
        }

        if let Some(total) = app_usage(value) {
            self.cumulative = total;
        }
        let used = token_delta(self.cumulative, self.baseline);
        vec![self.decoder.finish_tokens(used)]
    }

    fn end_of_stream(&mut self, complaint: &str) -> Vec<AgentEvent> {
        if !self.active {
            return Vec::new();
        }
        self.active = false;
        self.decoder.end_of_stream(self.cancelled, complaint)
    }
}

fn app_item(value: &Value) -> Option<Item> {
    let item = value.pointer("/params/item")?.clone();
    serde_json::from_value(item).ok()
}

fn app_usage(value: &Value) -> Option<Tokens> {
    ["/params/usage", "/params/tokenUsage/total", "/params/total"]
        .into_iter()
        .filter_map(|path| value.pointer(path))
        .find_map(|usage| serde_json::from_value::<Usage>(usage.clone()).ok())
        .map(|usage| Tokens {
            input: usage.input.unwrap_or_default(),
            output: usage.output.unwrap_or_default(),
            cached: usage.cached.unwrap_or_default(),
        })
}

fn token_delta(total: Tokens, baseline: Tokens) -> Tokens {
    fn component(total: u64, baseline: u64) -> u64 {
        if total >= baseline {
            total - baseline
        } else {
            // Vendor zrestartowal licznik; zero zgubiloby cala ture, wiec nowa wartosc jest
            // jedynym uczciwym zuzyciem od restartu.
            total
        }
    }

    Tokens {
        input: component(total.input, baseline.input),
        output: component(total.output, baseline.output),
        cached: component(total.cached, baseline.cached),
    }
}

/// Buduje NOWY rekord dowodu z zamknietej listy pol. Nigdy nie zapisuje calej koperty App
/// Servera: `turn/completed` niesie pelna liste `Turn.items`, a w niej moze zyc poprzedni
/// `userMessage` razem z data URL obrazu, mimo ze osobne item-notification bylo odfiltrowane.
fn curated_app_evidence(value: &Value) -> Option<Vec<u8>> {
    let method = value.get("method").and_then(Value::as_str)?;
    let record = match method {
        "turn/started" | "turn/completed" => {
            let status = value
                .pointer("/params/turn/status")
                .and_then(Value::as_str)
                .filter(|status| {
                    matches!(
                        *status,
                        "completed" | "failed" | "interrupted" | "inProgress"
                    )
                })
                .unwrap_or("unknown");
            let usage = app_usage(value).unwrap_or_default();
            json!({
                "method": method,
                "status": status,
                "usage": {
                    "inputTokens": usage.input,
                    "outputTokens": usage.output,
                    "cachedInputTokens": usage.cached,
                }
            })
        }
        "thread/tokenUsage/updated" => {
            let usage = app_usage(value)?;
            json!({
                "method": method,
                "usage": {
                    "inputTokens": usage.input,
                    "outputTokens": usage.output,
                    "cachedInputTokens": usage.cached,
                }
            })
        }
        "item/started" | "item/completed" => {
            let item = curated_app_item(app_item(value)?)?;
            json!({ "method": method, "item": item })
        }
        // `thread/started` potrafi niesc path i cala metadane watku; pozostale metody sa
        // nieznane albo jawnie prywatne (`codex/event/user_message`).
        _ => return None,
    };

    let mut bytes = serde_json::to_vec(&record).ok()?;
    if bytes
        .windows(b"data:image/".len())
        .any(|window| window == b"data:image/")
    {
        return None;
    }
    bytes.push(b'\n');
    Some(bytes)
}

fn curated_app_item(item: Item) -> Option<Value> {
    match item {
        Item::CommandExecution {
            command,
            exit_code,
            aggregated_output,
            ..
        } => Some(json!({
            "type": "commandExecution",
            "command": command,
            "exitCode": exit_code,
            "aggregatedOutput": aggregated_output,
        })),
        Item::FileChange { changes } => Some(json!({
            "type": "fileChange",
            "changes": changes
                .unwrap_or_default()
                .into_iter()
                .filter_map(|change| change.path)
                .collect::<Vec<_>>(),
        })),
        Item::AgentMessage { text } => Some(json!({
            "type": "agentMessage",
            "text": text,
        })),
        Item::Reasoning {} => Some(json!({ "type": "reasoning" })),
        Item::WebSearch { query, .. } => Some(json!({
            "type": "webSearch",
            "query": query,
        })),
        Item::McpToolCall { server, tool, .. } => Some(json!({
            "type": "mcpToolCall",
            "server": server,
            "tool": tool,
        })),
        Item::Unknown => None,
    }
}

async fn write_app_line(stdin: &mut ChildStdin, body: &[u8]) -> anyhow::Result<()> {
    stdin
        .write_all(body)
        .await
        .map_err(|_| anyhow!("The Codex App Server stopped reading requests."))?;
    stdin
        .write_all(b"\n")
        .await
        .map_err(|_| anyhow!("The Codex App Server stopped reading requests."))?;
    stdin
        .flush()
        .await
        .map_err(|_| anyhow!("The Codex App Server stopped reading requests."))
}

async fn handle_app_command(
    command: Option<AppCommand>,
    stdin: &mut ChildStdin,
    pending: &mut HashMap<u64, PendingAppRequest>,
    state: &mut AppServerState,
    evidence_target: Option<&EvidenceTarget>,
) -> bool {
    match command {
        Some(AppCommand::Request {
            id,
            method,
            body,
            begins_turn,
            reply,
        }) => {
            if begins_turn {
                state.begin_turn();
            }
            match write_app_line(stdin, &body).await {
                Ok(()) => {
                    pending.insert(id, PendingAppRequest { method, reply });
                }
                Err(error) => {
                    mark_evidence_incomplete(evidence_target);
                    if reply.send(Err(error)).is_err() {
                        mark_evidence_incomplete(evidence_target);
                    }
                }
            }
            true
        }
        Some(AppCommand::Notify { body, reply }) => {
            let result = write_app_line(stdin, &body).await;
            if result.is_err() {
                mark_evidence_incomplete(evidence_target);
            }
            if reply.send(result).is_err() {
                mark_evidence_incomplete(evidence_target);
            }
            true
        }
        Some(AppCommand::SetThread { id, reply }) => {
            state.set_thread(id);
            if reply.send(()).is_err() {
                mark_evidence_incomplete(evidence_target);
            }
            true
        }
        Some(AppCommand::MarkCancelled) => {
            state.mark_cancelled();
            true
        }
        Some(AppCommand::Close { reply }) => {
            let result = stdin
                .shutdown()
                .await
                .map_err(|_| anyhow!("The Codex App Server input could not be closed."));
            if result.is_err() {
                mark_evidence_incomplete(evidence_target);
            }
            if reply.send(result).is_err() {
                mark_evidence_incomplete(evidence_target);
            }
            false
        }
        None => {
            mark_evidence_incomplete(evidence_target);
            let _ = stdin.shutdown().await;
            false
        }
    }
}

struct AppServerInput<Output> {
    stdin: ChildStdin,
    stdout: Output,
    commands: mpsc::Receiver<AppCommand>,
    events: mpsc::Sender<DecodedEvent>,
    outcomes: mpsc::Sender<Outcome>,
    complaint: Arc<Mutex<String>>,
    evidence: Option<EvidenceWriter>,
    evidence_target: Option<EvidenceTarget>,
}

fn fail_pending_app_requests(
    pending: HashMap<u64, PendingAppRequest>,
    evidence_target: Option<&EvidenceTarget>,
) {
    for request in pending.into_values() {
        if request
            .reply
            .send(Err(anyhow!(
                "The Codex App Server stopped before its {} request completed.",
                request.method
            )))
            .is_err()
        {
            mark_evidence_incomplete(evidence_target);
        }
    }
}

async fn app_server_actor<Output>(input: AppServerInput<Output>)
where
    Output: AsyncRead + Unpin,
{
    let AppServerInput {
        mut stdin,
        stdout,
        mut commands,
        events,
        outcomes,
        complaint,
        mut evidence,
        evidence_target,
    } = input;
    let mut reader = BufReader::new(stdout);
    let mut buffer = Vec::with_capacity(8 * 1024);
    let mut pending: HashMap<u64, PendingAppRequest> = HashMap::new();
    let mut state = AppServerState::new();
    let mut commands_open = true;

    loop {
        buffer.clear();
        tokio::select! {
            command = commands.recv(), if commands_open => {
                commands_open =
                    handle_app_command(
                        command,
                        &mut stdin,
                        &mut pending,
                        &mut state,
                        evidence_target.as_ref(),
                    ).await;
            }
            read = reader.read_until(b'\n', &mut buffer) => {
                match read {
                    Ok(0) => break,
                    Err(_) => {
                        mark_evidence_incomplete(evidence_target.as_ref());
                        tracing::debug!("the Codex App Server output stream broke off");
                        break;
                    }
                    Ok(_) => {}
                }

                let parsed = match serde_json::from_slice::<Value>(&buffer) {
                    Ok(parsed) => parsed,
                    Err(_error) => {
                        state.decoder.dropped += 1;
                        tracing::debug!(bytes = buffer.len(), "an App Server line could not be read; dropping it");
                        continue;
                    }
                };

                if let Some(id) = parsed.get("id").and_then(Value::as_u64) {
                    if let Some(request) = pending.remove(&id) {
                        let answer = if parsed.get("error").is_some_and(|error| !error.is_null()) {
                            Err(anyhow!("The Codex App Server rejected its {} request.", request.method))
                        } else {
                            Ok(parsed.get("result").cloned().unwrap_or(Value::Null))
                        };
                        if request.reply.send(answer).is_err() {
                            mark_evidence_incomplete(evidence_target.as_ref());
                        }
                    } else {
                        state.decoder.dropped += 1;
                    }
                    continue;
                }

                if let Some(record) = curated_app_evidence(&parsed)
                    && let Some(writer) = evidence.as_mut()
                    && let Err(_error) = writer.write(&record).await
                {
                    tracing::debug!("the curated App Server evidence could not be appended");
                }

                let began = state.began;
                for event in state.notification(&parsed) {
                    emit_app(
                        event,
                        began,
                        &events,
                        &outcomes,
                        evidence_target.as_ref(),
                    )
                    .await;
                }
            }
        }
    }

    fail_pending_app_requests(pending, evidence_target.as_ref());
    close_evidence(evidence).await;
    let said = complaint
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let began = state.began;
    for event in state.end_of_stream(&said) {
        emit_app(event, began, &events, &outcomes, evidence_target.as_ref()).await;
    }
}

async fn emit_app(
    mut event: AgentEvent,
    began: Instant,
    events: &mpsc::Sender<DecodedEvent>,
    outcomes: &mpsc::Sender<Outcome>,
    evidence_target: Option<&EvidenceTarget>,
) {
    if let AgentEvent::Finished(outcome) = &mut event {
        if outcome.took.is_zero() {
            outcome.took = began.elapsed();
        }
        if outcomes.send(outcome.clone()).await.is_err() {
            mark_evidence_incomplete(evidence_target);
        }
    }
    if events.send(event.into()).await.is_err() {
        mark_evidence_incomplete(evidence_target);
    }
}

fn app_server_sandbox(policy: Policy) -> &'static str {
    match policy {
        Policy::ReadOnly => "readOnly",
        Policy::EditInFolder => "workspaceWrite",
        Policy::Unrestricted => "dangerFullAccess",
    }
}

fn app_turn_input(text: &str, images: &ValidatedImages) -> anyhow::Result<Vec<Value>> {
    let mut input = Vec::with_capacity(images.as_slice().len() + usize::from(!text.is_empty()));
    if !text.is_empty() {
        input.push(json!({ "type": "text", "text": text }));
    }
    for image in images.as_slice() {
        // Data URL zyje tylko w anonimowej pamieci tego requestu i leci jednym stdinem. Nigdy
        // nie ma sciezki pliku, argv ani surowego tee App Servera (niezmiennik 9).
        let url = format!(
            "data:{};base64,{}",
            image.mime().as_str(),
            BASE64_STANDARD.encode(image.bytes())
        );
        input.push(json!({ "type": "image", "url": url }));
    }
    if input.is_empty() {
        anyhow::bail!("Write a message or attach an image before starting the agent.");
    }
    Ok(input)
}

fn ephemeral_thread(result: &Value) -> anyhow::Result<String> {
    let id = result
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("The Codex App Server opened a thread without an id."))?;
    if result.pointer("/thread/ephemeral").and_then(Value::as_bool) != Some(true)
        || result.pointer("/thread/path") != Some(&Value::Null)
    {
        anyhow::bail!("The Codex App Server did not confirm an ephemeral thread.");
    }
    Ok(id)
}

fn started_turn(result: &Value) -> anyhow::Result<String> {
    result
        .pointer("/turn/id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("The Codex App Server started a turn without an id."))
}

/// Jeden proces App Servera na cala rozmowe Lead. Pola tekstowe sa identyfikatorami vendora,
/// wiec reczny Debug pokazuje wylacznie stan transportu.
struct CodexConversationHandle {
    process: Option<Supervised>,
    client: AppClient,
    evidence: Option<EvidenceTarget>,
    session_id: String,
    active_turn: Option<String>,
    in_flight: bool,
    outcomes: mpsc::Receiver<Outcome>,
    reader_task: Option<JoinHandle<()>>,
    stderr_task: Option<JoinHandle<()>>,
}

impl fmt::Debug for CodexConversationHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexConversationHandle")
            .field("has_process", &self.process.is_some())
            .field("has_session", &!self.session_id.is_empty())
            .field("has_active_turn", &self.active_turn.is_some())
            .field("in_flight", &self.in_flight)
            .finish_non_exhaustive()
    }
}

impl CodexConversationHandle {
    async fn handshake(&mut self, spec: RunSpec, images: ValidatedImages) -> anyhow::Result<()> {
        if spec.resume.is_some() {
            anyhow::bail!("An ephemeral Codex Lead conversation cannot resume a persisted thread.");
        }

        let _initialized = self
            .client
            .request(
                "initialize",
                json!({
                    "clientInfo": {
                        "name": "loadout",
                        "title": "Loadout",
                        "version": env!("CARGO_PKG_VERSION"),
                    }
                }),
                false,
            )
            .await?;
        self.client.notify("initialized", json!({})).await?;

        let mut params = serde_json::Map::new();
        params.insert("ephemeral".to_owned(), Value::Bool(true));
        params.insert(
            "approvalPolicy".to_owned(),
            Value::String("never".to_owned()),
        );
        params.insert(
            "sandbox".to_owned(),
            Value::String(app_server_sandbox(spec.policy).to_owned()),
        );
        if let Some(model) = spec.model {
            params.insert("model".to_owned(), Value::String(model));
        }
        if let Some(instructions) = spec.system_append {
            params.insert(
                "developerInstructions".to_owned(),
                Value::String(instructions),
            );
        }
        /* SZCZEBLA „ILE MYSLEC" TU NIE MA I JEST TO ZGLOSZENIE, NIE PRZEOCZENIE (2026-08-23,
         * T-91). Krok biegu dostaje go jako `-c model_reasoning_effort=<poziom>` przed `exec`;
         * ta droga to App Server, a jego protokol takiego pola NIE MA. Zmierzone na codex-cli
         * 0.148.0, `codex app-server generate-json-schema --experimental`: `ThreadStartParams`
         * ma 25 pol (`model`, `sandbox`, `approvalPolicy`, `developerInstructions`, …) i ani
         * jednego o wysilku — jedyne wystapienie slowa „effort" w calym pliku stoi w opisie
         * DEPRECATED-owanego `multiAgentMode`.
         *
         * Zostaja dwie drogi i obie sa zgadywaniem, ktorego to zadanie nie kupuje: nietypowana
         * mapa `config` (`additionalProperties: true`, wiec schemat nie powie, czy klucz jest
         * plaski) albo `-c` w argv samego `app-server`, gdzie `--help` przyjmuje go w INNYM
         * miejscu linii niz przed `exec`. Niepoznany klucz w `thread/start` jest odmowa startu
         * watku, czyli liderem, ktory przestaje rozmawiac — a zadne kryterium tego nie sadzi.
         * Do rozstrzygniecia przez czlowieka, na zywym watku.
         *
         * `cwd` celowo nie ma w JSON-ie. `command.current_dir` daje agentowi folder, a jawne
         * pole cwd w App Serverze 0.148 potrafi zapisac zaufanie projektu pod `.codex`. */
        let result = self
            .client
            .request("thread/start", Value::Object(params), false)
            .await?;
        self.session_id = ephemeral_thread(&result)?;
        self.client.set_thread(self.session_id.clone()).await?;
        self.start_turn(spec.prompt, images).await
    }

    async fn start_turn(&mut self, text: String, images: ValidatedImages) -> anyhow::Result<()> {
        if self.in_flight {
            anyhow::bail!("Wait for the current Codex turn before sending another message.");
        }
        let input = app_turn_input(&text, &images)?;
        let result = match self
            .client
            .request(
                "turn/start",
                json!({
                    "threadId": self.session_id,
                    "input": input,
                }),
                true,
            )
            .await
        {
            Ok(result) => result,
            Err(error) => {
                let _ = self.force_stop().await;
                return Err(error);
            }
        };
        let turn = match started_turn(&result) {
            Ok(turn) => turn,
            Err(error) => {
                // Bez id nie da sie ani przerwac, ani odroznic wyniku tej tury od kolejnej.
                // To jest fatalny blad transportu, nie zaproszenie do ponownego send na tym
                // samym kanale z potencjalnie starym outcome.
                let _ = self.force_stop().await;
                return Err(error);
            }
        };
        self.active_turn = Some(turn);
        self.in_flight = true;
        Ok(())
    }

    async fn finish_tasks(&mut self) {
        if let Some(reader) = self.reader_task.take()
            && reader.await.is_err()
        {
            mark_evidence_incomplete(self.evidence.as_ref());
            tracing::warn!("the Codex App Server evidence reader did not join cleanly");
        }
        if let Some(stderr) = self.stderr_task.take()
            && stderr.await.is_err()
        {
            mark_evidence_incomplete(self.evidence.as_ref());
            tracing::warn!("the Codex App Server complaint reader did not join cleanly");
        }
    }

    async fn cleanup_after_proof(&mut self, proof: &GroupProof) {
        if proof_allows_cleanup(proof) {
            self.finish_tasks().await;
            self.process = None;
        }
    }

    async fn force_stop(&mut self) -> GroupProof {
        let proof = match self.process.as_mut() {
            Some(process) => process.stop(DEFAULT_GRACE).await,
            None => GroupProof::Dead { status: None },
        };
        self.cleanup_after_proof(&proof).await;
        proof
    }

    async fn force_stop_after_failed_start(&mut self) {
        loop {
            let proof = self.force_stop().await;
            if proof_allows_cleanup(&proof) {
                return;
            }
            tracing::error!(
                "the Codex App Server is still alive after a failed handshake; Loadout retains \
                 its handle and will retry"
            );
            tokio::time::sleep(START_CLEANUP_RETRY).await;
        }
    }
}

impl CodexDriver {
    async fn start_app_conversation(
        &self,
        spec: RunSpec,
        images: ValidatedImages,
        tx: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<CodexConversationHandle> {
        let evidence = match &self.evidence {
            Some(target) => Some(target.open().await?),
            None => None,
        };
        let (stdout_evidence, stderr_evidence) = split_evidence(evidence);

        let mut command = Command::new(&self.binary);
        command.current_dir(&spec.cwd);
        // ZATWIERDZONE CONNECTIONS JADĄ TAKŻE TĘDY (2026-08-24, T-97). Do tego dnia ta droga nie
        // doklejała `configuration.arguments` **wcale**, więc lider rozmawiający przez App Server
        // nie dostawał ani jednego serwera — a przez `exec` dostawał. Ten sam agent odpowiadał
        // inaczej zależnie od tego, którą drogą go zawołano, i nic tego nie mówiło.
        command.args(app_server_argv(&self.configuration));
        let mut process = match supervisor::spawn(command, StdinPlan::Keep(String::new())) {
            Ok(process) => process,
            Err(error) => {
                mark_evidence_incomplete(self.evidence.as_ref());
                return Err(error.into());
            }
        };

        let Some(stdout) = process.stdout() else {
            mark_evidence_incomplete(self.evidence.as_ref());
            close_evidence(stdout_evidence).await;
            close_evidence(stderr_evidence).await;
            let _proof = stop_startup_process(&mut process).await;
            anyhow::bail!("The Codex App Server started without an output stream.");
        };
        let Some(stdin) = process.stdin().await else {
            mark_evidence_incomplete(self.evidence.as_ref());
            close_evidence(stdout_evidence).await;
            close_evidence(stderr_evidence).await;
            let _proof = stop_startup_process(&mut process).await;
            anyhow::bail!("The Codex App Server started without an input stream.");
        };

        let complaint = Arc::new(Mutex::new(String::new()));
        let stderr_task = if let Some(stderr) = process.stderr() {
            tokio::spawn(drain_complaints(
                stderr,
                Arc::clone(&complaint),
                stderr_evidence,
                self.evidence.clone(),
            ))
        } else {
            mark_evidence_incomplete(self.evidence.as_ref());
            tokio::spawn(close_evidence(stderr_evidence))
        };
        let (commands_tx, commands_rx) = mpsc::channel(APP_COMMAND_CAPACITY);
        let (outcomes_tx, outcomes_rx) = mpsc::channel(APP_OUTCOME_CAPACITY);
        let reader_task = tokio::spawn(app_server_actor(AppServerInput {
            stdin,
            stdout,
            commands: commands_rx,
            events: tx,
            outcomes: outcomes_tx,
            complaint,
            evidence: stdout_evidence,
            evidence_target: self.evidence.clone(),
        }));
        let mut handle = CodexConversationHandle {
            process: Some(process),
            client: AppClient::new(commands_tx, self.evidence.clone()),
            evidence: self.evidence.clone(),
            session_id: String::new(),
            active_turn: None,
            in_flight: false,
            outcomes: outcomes_rx,
            reader_task: Some(reader_task),
            stderr_task: Some(stderr_task),
        };

        if let Err(error) = handle.handshake(spec, images).await {
            mark_evidence_incomplete(handle.evidence.as_ref());
            handle.force_stop_after_failed_start().await;
            return Err(error);
        }
        Ok(handle)
    }
}

#[async_trait]
impl AgentHandle for CodexConversationHandle {
    fn session(&self) -> SessionRef {
        SessionRef {
            vendor: VENDOR,
            id: self.session_id.clone(),
        }
    }

    fn group(&self) -> Option<GroupId> {
        self.process.as_ref().map(Supervised::group)
    }

    async fn send(&mut self, text: String) -> anyhow::Result<()> {
        self.start_turn(text, ValidatedImages::default()).await
    }

    async fn send_with_images(
        &mut self,
        text: String,
        images: ValidatedImages,
    ) -> anyhow::Result<()> {
        self.start_turn(text, images).await
    }

    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        if !self.in_flight {
            anyhow::bail!("This Codex conversation has no turn to wait for.");
        }
        let Some(outcome) = self.outcomes.recv().await else {
            mark_evidence_incomplete(self.evidence.as_ref());
            return Err(anyhow!("The Codex App Server ended without a turn result."));
        };
        self.in_flight = false;
        self.active_turn = None;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> GroupProof {
        self.client.mark_cancelled().await;
        if let Some(turn) = self.active_turn.clone() {
            let interrupt = self.client.request(
                "turn/interrupt",
                json!({
                    "threadId": self.session_id,
                    "turnId": turn,
                }),
                false,
            );
            // Odpowiedz JSON-RPC nie jest dowodem smierci. Okno daje vendorowi szanse domknac
            // ture, po czym zawsze przechodzimy przez supervisor i jego GroupProof.
            let _ = timeout(APP_INTERRUPT_WINDOW, interrupt).await;
        }
        let proof = self.force_stop().await;
        if proof_allows_cleanup(&proof) {
            self.in_flight = false;
            self.active_turn = None;
        }
        proof
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        if self.process.is_none() {
            return Ok(None);
        }
        if let Err(error) = self.client.close().await {
            let _ = self.force_stop().await;
            return Err(error);
        }
        let waited = match self.process.as_mut() {
            Some(process) => process.wait().await,
            None => return Ok(None),
        };
        let status = match waited {
            Ok(status) => status,
            Err(error) => {
                let _ = self.force_stop().await;
                return Err(error.into());
            }
        };
        self.finish_tasks().await;
        self.process = None;
        Ok(status.code())
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
/// Nagłówek, pod którym instrukcje agenta wchodzą do promptu Codeksa.
///
/// Po angielsku i bez naszych słów z drutu, jak wszystko, co czyta agent (decyzja D5,
/// niezmiennik 14).
const STANDING_ORDERS_OPEN: &str =
    "Who you are and how you work. This holds for everything below and does not change:";

/// Instrukcje agenta, potem robota — czyli prompt pierwszej tury `codex exec`.
///
/// DLACZEGO TO MUSI TU BYĆ. `RunSpec::system_append` niesie instrukcje agenta, czyli to, co
/// odróżnia researchera od planisty. `claude.rs` oddaje je flagą `--append-system-prompt`.
/// `codex exec` TAKIEJ FLAGI NIE MA, a jedyne miejsce w tym pliku, które kiedykolwiek czytało
/// to pole, to `handshake()` — App Server, czyli czat z liderem. Ścieżka biegu nie czytała ich
/// nigdy, więc każdy agent codexowy w każdym workflow biegł bez swojej roli.
///
/// ZMIERZONE 2026-08-23, trzema odczytami: `first_turn_argv` nie dotyka `system_append` ani
/// razu; `grep system_append` po całym drzewie daje w tym pliku jedno trafienie i jest nim
/// `handshake`; nic w drzewie Rusta nie zapisuje instrukcji na dysk dla Codeksa (żadnego
/// `AGENTS.md` w drzewie roboczym kroku).
///
/// STDIN, NIE ARGV — i to nie jest ustępstwo. Niezmiennik 9 zabrania treści w argumentach, bo
/// `ps` pokazuje je każdemu użytkownikowi maszyny. Instrukcje podane tą drogą są więc BARDZIEJ
/// prywatne niż flaga, którą dostaje Claude, a nie mniej.
///
/// NA GÓRZE, bo w tym porządku czyta model i w tym porządku składa prompt
/// `commands::run::plan_step`: notatki, zadanie biegu, robota kroku — od najogólniejszego do
/// najkonkretniejszego. Rola agenta stoi nad tym wszystkim.
///
/// TYLKO PIERWSZA TURA. `codex exec resume` wraca do tego samego wątku, więc instrukcje wysłane
/// raz zostają w rozmowie; powtórzenie ich przy każdej turze byłoby drugim zdaniem o tym samym.
///
/// BEZ INSTRUKCJI ODDAJE PROMPT CO DO BAJTU: agent bez własnych instrukcji ma dostać dokładnie
/// to, co dostawał, więc `None` nie dokłada ani jednego znaku.
fn after_the_standing_orders(instructions: Option<&str>, prompt: String) -> String {
    let Some(orders) = instructions.map(str::trim).filter(|one| !one.is_empty()) else {
        return prompt;
    };
    format!("{STANDING_ORDERS_OPEN}\n\n{orders}\n\n---\n\n{prompt}")
}

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

    /* SIEĆ JEST TU USTAWIENIEM PIASKOWNICY, NIE NAZWĄ NARZĘDZIA, i to jest cała różnica między
     * tym adapterem a claude'owym. Codex nie ma listy narzędzi — u niego dostęp do internetu
     * wisi przy `workspace-write` jako `network_access`, domyślnie WYŁĄCZONY.
     *
     * 2026-08-23 — z pytania właściciela „czemu dostępu do neta nie mają?". Do tego dnia ta
     * skrzynia nie wysyłała `network_access` ANI RAZU (sprawdzone gerpem po całym drzewie Rusta),
     * więc agent codexowy do researchu — `codex-reaserch`, `planner`, `riczi` — nie miał jak
     * dostać sieci inaczej niż przez `danger-full-access`, czyli zdejmując całą piaskownicę.
     * Wybór między „widzi świat i może zepsuć wszystko" a „nie zepsuje niczego i nie widzi nic"
     * jest dokładnie tym, co T-63 usunęło po stronie Claude'a.
     *
     * TYLKO PRZY `workspace-write`. Przy `read-only` ten klucz nie ma zastosowania (Codex go
     * tam nie czyta), a przy `danger-full-access` sieć jest już otwarta i dopisanie go byłoby
     * drugim zdaniem o tym samym. */
    if spec.reaches_the_web && matches!(spec.policy, Policy::EditInFolder) {
        argv.push("-c".to_owned());
        argv.push("sandbox_workspace_write.network_access=true".to_owned());
    }

    // Myślnik na końcu jest tym, co każe czytać prompt ze stdinu [T1 §6.1]. Bez niego trzeba by
    // go podać argumentem — czyli złamać niezmiennik 9 dokładnie tak, jak podpowiada T1 §8.4.
    argv.push("-".to_owned());
    argv
}

/// Linia tury wznawiającej [T1 §8.4].
///
/// `-C` stoi PRZED `resume`, bo jest opcją rodzica `codex exec`, nie podkomendy `resume`.
/// Zmierzone 2026-08-21 na codex-cli 0.148.0: `exec resume <id> ... -C <cwd> -` kończy się
/// natychmiast `unexpected argument '-C'`, zanim prompt dotrze do rozmowy. Proces mimo to daje
/// się uruchomić, więc z okna wyglądało to jak `Didn't work · 0 turns · 0.0s`.
///
/// Czego tu **nie ma i nie ma prawa być**: `-m` i `-s` należą do pierwszej tury (rozmowa ma już
/// swój model i swoją piaskownicę), a `--skip-git-repo-check` razem z nimi — wznawiana rozmowa
/// przeszła tę bramkę raz.
fn resume_argv(thread: &str, cwd: &Path) -> Vec<String> {
    vec![
        "exec".to_owned(),
        "-C".to_owned(),
        cwd.display().to_string(),
        "resume".to_owned(),
        thread.to_owned(),
        "--json".to_owned(),
        "--ignore-user-config".to_owned(),
        "-".to_owned(),
    ]
}

/// Klucz konfiguracji, którym Codex przyjmuje poziom wysiłku.
///
/// Zmierzone 2026-08-23 na tej maszynie: `-c model_reasoning_effort=<minimal|low|medium|high|
/// xhigh>` jest opcją GLOBALNĄ `codex`, nie podkomendy `exec`. Podany po `exec` CLI odrzuca —
/// tak samo, jak odrzucało `-C` postawione po `resume` (patrz [`resume_argv`]).
///
/// Ten sam klucz czyta importer, w drugą stronę (`import::adapters`).
const EFFORT_KEY: &str = "model_reasoning_effort";

/// Pełne argv jednej tury `exec`: opcje globalne z konfiguracji, potem linia podkomendy.
///
/// Istnieje jako funkcja, a nie dwie linie w [`CodexDriver::start_session`], bo dokładnie ten
/// skład — „konfiguracja, potem `exec`" — jest tym, czego dotyczy kryterium o kolejności.
/// Sklejenie liczone w miejscu startu procesu dałoby się sprawdzić wyłącznie przez uruchomienie
/// prawdziwej binarki.
#[must_use]
pub fn exec_argv(configuration: &DriverConfiguration, spec: &RunSpec) -> Vec<String> {
    // Wznowienie przychodzi tędy także wtedy, gdy startuje je świeży sterownik (`spec.resume`),
    // więc wysiłek odpada w OBU drogach do `exec resume`, a nie tylko w [`CodexHandle::send`].
    let mut argv = if spec.resume.is_some() {
        without_the_effort(&configuration.arguments)
    } else {
        configuration.arguments.clone()
    };
    argv.extend(build_exec_argv(spec));
    argv
}

/// Pełne argv App Servera: opcje globalne z konfiguracji, potem linia podkomendy.
///
/// # Dlaczego opcje stoją PRZED podkomendą (2026-08-24, T-97)
///
/// Z tego samego powodu, z którego stoją przed `exec` (patrz [`EFFORT_KEY`]): `-c` jest opcją
/// **globalną** `codex`, a nie opcją podkomendy. Postawiona za `app-server` jest czytana jako
/// jego argument — czyli w najlepszym razie odrzucana, a w gorszym połykana w ciszy, i wtedy
/// lider rozmawia bez ani jednego zatwierdzonego serwera, a nic tego nie mówi.
///
/// Wysiłek tędy nie jedzie i to nie jest przeoczenie: `ThreadStartParams` App Servera nie ma
/// pola o wysiłku (zmierzone na codex-cli 0.148.0, powód w całości przy `thread/start`), a `-c`
/// dla wysiłku na tej drodze byłoby zgadywaniem, którego to zadanie nie kupuje. Connections mają
/// tu inną pozycję: bez nich zatwierdzone przez człowieka połączenie nie dojeżdża **wcale**.
///
/// Istnieje jako funkcja, a nie trzy linie w [`CodexDriver::start_app_conversation`], z tego
/// samego powodu co [`exec_argv`]: kolejność jest tym, czego dotyczy kryterium, a policzona
/// w miejscu startu procesu dałaby się sprawdzić wyłącznie przez uruchomienie prawdziwej binarki.
#[must_use]
pub fn app_server_argv(configuration: &DriverConfiguration) -> Vec<String> {
    let mut argv = without_the_effort(&configuration.arguments);
    argv.extend(APP_SERVER.iter().copied().map(str::to_owned));
    argv
}

/// Linia podkomendy App Servera — dokładnie ta, którą ta droga składała przed T-97.
///
/// Stała, a nie trzy literały w miejscu użycia: to jest argv, którego „co do bajtu jak dziś"
/// wymaga kryterium, więc ma jedno miejsce, w którym da się je przeczytać.
const APP_SERVER: [&str; 3] = ["app-server", "--listen", "stdio://"];

/// Pełne argv tury wznowienia — bez powtórzonego wysiłku, z zachowanymi Connections.
#[must_use]
pub fn exec_resume_argv(
    configuration: &DriverConfiguration,
    thread: &str,
    cwd: &Path,
) -> Vec<String> {
    let mut argv = without_the_effort(&configuration.arguments);
    argv.extend(resume_argv(thread, cwd));
    argv
}

/// Opcje globalne bez ustawienia wysiłku — czyli to, co jedzie w turze wznawiającej.
///
/// # Dlaczego wysiłek odpada, a Connections zostają
///
/// `codex exec resume` wraca do wątku, który wysiłek już MA: dostał go przy pierwszej turze
/// i Codex trzyma go po swojej stronie. Powtórzenie jest w najlepszym razie drugim zdaniem
/// o tym samym, a w najgorszym przestawia rozmowę w połowie. Connections są odwrotnie: każda
/// tura to ŚWIEŻY PROCES, więc serwer niepodany drugi raz po prostu nie istnieje dla drugiej
/// tury — i to jest powód, dla którego ta funkcja odejmuje jeden konkretny klucz, a nie
/// wszystkie `-c`. Implementacja kasująca całą rodzinę przechodzi każde sprawdzenie pytające
/// o brak wysiłku i po cichu zabiera rozmowie zatwierdzone połączenia.
fn without_the_effort(arguments: &[String]) -> Vec<String> {
    let prefix = format!("{EFFORT_KEY}=");
    let mut out = Vec::with_capacity(arguments.len());
    let mut skip_value = false;
    for (at, argument) in arguments.iter().enumerate() {
        if skip_value {
            skip_value = false;
            continue;
        }
        // Para, nie sam klucz: `-c` bez wartości połknęłoby następny argument jako swój, więc
        // odjęcie połowy pary jest gorsze niż jej zostawienie.
        if argument == "-c"
            && arguments
                .get(at + 1)
                .is_some_and(|value| value.starts_with(&prefix))
        {
            skip_value = true;
            continue;
        }
        out.push(argument.clone());
    }
    out
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
    #[serde(rename = "input_tokens", alias = "inputTokens")]
    input: Option<u64>,
    /// Ta liczba, i tylko ta, mówi, czy izolacja kontekstu w ogóle działa [T1 §3.3].
    #[serde(rename = "cached_input_tokens", alias = "cachedInputTokens")]
    cached: Option<u64>,
    #[serde(rename = "output_tokens", alias = "outputTokens")]
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
    #[serde(alias = "commandExecution")]
    CommandExecution {
        id: Option<String>,
        command: Option<String>,
        #[serde(alias = "exitCode")]
        exit_code: Option<i32>,
        #[serde(alias = "aggregatedOutput")]
        aggregated_output: Option<String>,
    },
    /// Zmiana plików — **lista**, nie jeden plik.
    #[serde(alias = "fileChange")]
    FileChange {
        #[serde(default, deserialize_with = "lenient")]
        changes: Option<Vec<Change>>,
    },
    /// Proza agenta, dosłownie.
    #[serde(alias = "agentMessage")]
    AgentMessage { text: Option<String> },
    /// Agent myśli. Treści **nie czytamy**: myślenie nie wchodzi do historii
    /// [`docs/ARCHITECTURE.md` §6, reguła 5].
    Reasoning {},
    /// Szukanie w sieci.
    #[serde(alias = "webSearch")]
    WebSearch {
        id: Option<String>,
        query: Option<String>,
    },
    /// Czynność w podłączonej aplikacji.
    #[serde(alias = "mcpToolCall")]
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
            Err(_error) => {
                self.dropped += 1;
                // Treści linii tu nie ma, i to jest świadome: surowy strumień leży już na dysku
                // (tee z T-05), a dziennik aplikacji czyta się w zgłoszeniu błędu — nie ma
                // powodu, żeby druga kopia cudzego tekstu jechała jeszcze tędy.
                tracing::debug!(
                    bytes = line.len(),
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
        self.finish_tokens(Tokens {
            input: usage.and_then(|usage| usage.input).unwrap_or_default(),
            output: usage.and_then(|usage| usage.output).unwrap_or_default(),
            cached: usage.and_then(|usage| usage.cached).unwrap_or_default(),
        })
    }

    /// Wspolny koniec tury dla `exec` i App Servera. Ten drugi odejmuje kumulatywny licznik
    /// przed wywolaniem, dzieki czemu ekran nie dolicza poprzednich tur po raz drugi.
    fn finish_tokens(&mut self, tokens: Tokens) -> AgentEvent {
        self.ended = true;
        AgentEvent::Finished(Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text: self.said.clone(),
            // `None`, nie zero, i to jest cała różnica: Codex kosztu nie podaje, a `Some(0.0)`
            // wypisze na ekranie `$0.00` i nauczy człowieka, że Codex jest darmowy — po czym ta
            // liczba zsumuje się w rachunek, którego nikt nie zamawiał.
            cost_usd: None,
            tokens,
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
    pub fn end_of_stream(&mut self, cancelled: bool, complaint: &str) -> Vec<AgentEvent> {
        if self.ended {
            return Vec::new();
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

        /* 2026-08-21 — `FinishReason` NIE JEDZIE DO WIERSZA `Done`. Zmierzone na żywym
         * `codex exec resume`: parser odrzucił źle położone `-C`, a człowiek zobaczył wyłącznie
         * `Didn't work · 0 turns · 0.0s`, choć pełna diagnoza była już tutaj. `Notice` przechodzi
         * przez jedyną kurację do widocznego `Line::Problem`; anulowanie pozostaje wartością i
         * nie udaje awarii (niezmienniki 7, 15 i 29). */
        let mut events = Vec::with_capacity(2);
        if let FinishReason::Failed(why) = &reason {
            events.push(AgentEvent::Notice { text: why.clone() });
        }
        events.push(AgentEvent::Finished(Outcome {
            ok: false,
            reason,
            text: self.said.clone(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 0,
            took: Duration::ZERO,
            session: self.session_ref(),
        }));
        events
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
async fn drain_complaints(
    mut stderr: ChildStderr,
    into: Arc<Mutex<String>>,
    mut evidence: Option<EvidenceWriter>,
    evidence_target: Option<EvidenceTarget>,
) {
    let mut buffer = vec![0_u8; 8 * 1024];
    loop {
        let read = match stderr.read(&mut buffer).await {
            Ok(0) => break,
            Err(_) => {
                mark_evidence_incomplete(evidence_target.as_ref());
                tracing::debug!("the agent complaint stream broke off");
                break;
            }
            Ok(read) => read,
        };
        let chunk = &buffer[..read];
        if let Some(writer) = evidence.as_mut()
            && let Err(_error) = writer.write(chunk).await
        {
            tracing::debug!("the private stderr evidence could not be appended");
        }
        let mut held = into.lock().unwrap_or_else(PoisonError::into_inner);
        if held.len() < COMPLAINT_KEPT {
            let left = COMPLAINT_KEPT - held.len();
            let lossy = String::from_utf8_lossy(chunk);
            held.extend(lossy.chars().take(left));
        }
        // Bez `break` po przekroczeniu limitu: pętla musi dalej OPRÓŻNIAĆ potok, nawet gdy nic
        // już nie zapamiętuje. Wyjście tutaj przywróciłoby dokładnie tę blokadę, przed którą
        // to zadanie stoi.
    }
    close_evidence(evidence).await;
}

async fn close_evidence(evidence: Option<EvidenceWriter>) {
    if let Some(writer) = evidence
        && let Err(_error) = writer.close().await
    {
        tracing::debug!("the private evidence stream could not be flushed");
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
async fn pump(input: PumpInput) {
    let PumpInput {
        stdout,
        events,
        outcome,
        threads,
        number,
        cancelled,
        complaint,
        mut evidence,
        evidence_target,
        drained,
    } = input;
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
            Err(_) => {
                mark_evidence_incomplete(evidence_target.as_ref());
                tracing::debug!("the agent output stream broke off");
                break;
            }
        }

        // Workflow zachowuje surowy strumien bajt w bajt, przed dekodowaniem. Nieznany event
        // nadal jest dowodem, mimo ze czysty widok zgodnie z niezmiennikiem 5 go porzuca.
        if let Some(writer) = evidence.as_mut()
            && let Err(_error) = writer.write(&buffer).await
        {
            tracing::debug!("the private stdout evidence could not be appended");
        }

        // `from_utf8_lossy`, żeby KAŻDA linia doszła do dekodera: uszkodzona nie sparsuje się
        // jako JSON i wpadnie do licznika porzuconych, zamiast zniknąć przed policzeniem.
        // Bajtowa identyczność nie jest tu wymaganiem, bo tee na dysk należy do T-05 i ten
        // sterownik go nie ma (patrz nagłówek pliku).
        let line = String::from_utf8_lossy(&buffer);
        // ZDARZENIA I FAKTY O CZYNNOŚCI Z JEDNEJ LINII I JEDNYM WYWOŁANIEM (2026-08-24, T-97).
        // Do tego dnia stało tu `decoder.push(&line)`, a `tool` jechało dalej jako `None`:
        // kurator nie miał z czego wybrać wariantu wiersza, więc transkrypt kroku Codeksa był
        // samą prozą — ani jednego `Ran`, `Edited` czy `Searched`. Tabela nazw z drutu została
        // po tamtej stronie (`stream::decode_codex`), bo druga jej kopia tutaj byłaby drugą
        // implementacją kuracji (niezmienniki 15 i 23).
        let produced = match stream::decode_codex(&mut decoder, &line) {
            stream::Decoded::Events(events) => events,
            stream::Decoded::Unrecognised => Vec::new(),
        };
        remember_thread(&decoder, &mut seen, &threads);

        for decoded in produced {
            emit(decoded, began, &events, &mut told, evidence_target.as_ref()).await;
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
    for event in decoder.end_of_stream(stopped_by_a_person, &said) {
        // Koniec strumienia nie opisuje żadnej czynności, więc nie ma tu czego przypinać.
        emit(
            event.into(),
            began,
            &events,
            &mut told,
            evidence_target.as_ref(),
        )
        .await;
    }

    if decoder.dropped() > 0 {
        tracing::debug!(
            dropped = decoder.dropped(),
            turn = number,
            "lines of the agent stream produced nothing and were let go"
        );
    }

    close_evidence(evidence).await;
    if drained.send(()).is_err() {
        mark_evidence_incomplete(evidence_target.as_ref());
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

    if identity.is_some_and(|identity| identity != id) {
        tracing::info!(
            "the agent answered with a different thread id than the one this session is known \
             by; the session keeps its first id and the next turn resumes the newest"
        );
    }
}

/// Wypuszcza jedno zdarzenie — **najpierw** do [`AgentHandle::wait`], potem na ekran.
///
/// Ta kolejność jest jedyną obroną przed wolnym konsumentem: kanał zdarzeń z pełnym buforem
/// zatrzymuje wysyłkę, a wynik tury, który utknął za nim, wygląda jak zawieszony agent.
async fn emit(
    decoded: DecodedEvent,
    began: Instant,
    events: &mpsc::Sender<DecodedEvent>,
    told: &mut Option<oneshot::Sender<Outcome>>,
    evidence_target: Option<&EvidenceTarget>,
) {
    let mut decoded = decoded;
    if let AgentEvent::Finished(outcome) = &mut decoded.event {
        // Czas mierzony przez nas, bo vendor go nie podaje (powód przy starcie zegara w [`pump`]).
        // Warunek, a nie przypisanie wprost: gdyby `turn.completed` kiedyś zaczęło nieść własną
        // liczbę, dekoder ją tu położy, a to jest liczba VENDORA — nadpisanie jej naszą byłoby
        // cichym skasowaniem jedynego pomiaru, którego sami nie umiemy zrobić lepiej.
        if outcome.took.is_zero() {
            outcome.took = began.elapsed();
        }
        if let Some(tell) = told.take()
            && tell.send(outcome.clone()).is_err()
        {
            mark_evidence_incomplete(evidence_target);
        }
    }
    // FAKT O CZYNNOŚCI JEDZIE DALEJ, nie ginie tutaj. Bez niego `Curator::tool_start` oddaje
    // pustkę i wiersze `read`, `search`, `edit`, `ran` nie powstają nigdy — ta sama awaria,
    // którą u Claude'a zmierzono 2026-08-18 i naprawiono przez [`DecodedEvent`].
    if events.send(decoded).await.is_err() {
        mark_evidence_incomplete(evidence_target);
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

/// Żywa sesja `codex` — **wiele procesów**, jedna tożsamość.
///
/// To jest cała różnica wobec `ClaudeHandle`, w którym proces jest jeden na całą sesję. Tura
/// druga i każda następna to `codex exec resume <thread_id>`, czyli świeży proces, zimny start
/// i odbudowa cache'u [T1 §8.1] — świadomy koszt, nie brak.
pub struct CodexHandle {
    /// Co uruchamiamy w kolejnych turach. Kopia z [`CodexDriver`], bo uchwyt przeżywa sterownik.
    binary: PathBuf,
    /// Katalog roboczy tej rozmowy. Kolejne tury dostają go z powrotem w `-C`.
    cwd: PathBuf,
    /// Kanał zdarzeń tej sesji. **Wszystkie** tury sypią w ten sam, bo z zewnątrz to jedna
    /// rozmowa — proces na turę jest szczegółem, który trait ma wchłonąć.
    events: mpsc::Sender<DecodedEvent>,
    /// Ten sam append-only target otwierany osobno dla kazdego procesu `exec resume`.
    evidence: Option<EvidenceTarget>,
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
    /// EOF stdoutu i zamkniecie jego writer-a. Nigdy nie czekamy na to przed zakonczeniem
    /// procesu, bo potok moze miec jeszcze dane do oproznienia.
    drained: Option<oneshot::Receiver<()>>,
    /// Osobny czytelnik stderr; `JoinHandle` jest jedynym dowodem, ze bajty zostaly doslane i
    /// zsynchronizowane przed oddaniem wyniku wyzej.
    stderr_task: Option<JoinHandle<()>>,
    /// Te same Connections muszą wrócić w każdej świeżej turze `codex exec resume`.
    configuration: DriverConfiguration,
}

impl fmt::Debug for CodexHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let thread_count = self
            .threads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len();
        formatter
            .debug_struct("CodexHandle")
            .field(
                "uses_custom_binary",
                &(self.binary != Path::new(DEFAULT_BINARY)),
            )
            .field("workspace", &"<private>")
            .field("thread_count", &thread_count)
            .field("number", &self.number)
            .field("has_process", &self.process.is_some())
            .field("has_outcome", &self.outcome.is_some())
            .field("has_evidence", &self.evidence.is_some())
            .finish_non_exhaustive()
    }
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

    /// Czeka, az oba czytniki oproznia potoki i zamkna append-only dowody.
    async fn drain_current(&mut self) {
        if let Some(drained) = self.drained.take()
            && drained.await.is_err()
        {
            mark_evidence_incomplete(self.evidence.as_ref());
            tracing::warn!("the Codex stdout evidence reader did not finish cleanly");
        }
        if let Some(stderr) = self.stderr_task.take()
            && stderr.await.is_err()
        {
            mark_evidence_incomplete(self.evidence.as_ref());
            tracing::warn!("the Codex stderr evidence reader did not join cleanly");
        }
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
        self.drain_current().await;

        let evidence = match &self.evidence {
            Some(target) => Some(target.open().await?),
            None => None,
        };
        let (stdout_evidence, stderr_evidence) = split_evidence(evidence);

        self.number += 1;
        let argv = exec_resume_argv(&self.configuration, &thread, &self.cwd);
        let turn = Turn {
            binary: self.binary.clone(),
            cwd: self.cwd.clone(),
            argv,
            prompt: text,
            events: self.events.clone(),
            threads: Arc::clone(&self.threads),
            number: self.number,
            cancelled: Arc::clone(&self.cancelled),
            stdout_evidence,
            stderr_evidence,
            evidence_target: self.evidence.clone(),
            configuration: self.configuration.clone(),
        };
        let started = turn.start();
        if started.is_err() {
            mark_evidence_incomplete(self.evidence.as_ref());
        }
        let (process, outcome, drained, stderr_task) = started?;

        // Podmiana, nie dopisanie: stary uchwyt ginie tutaj, a jego `Drop` jest ostatnią linią
        // obrony przed wyciekiem grupy.
        self.process = Some(process);
        self.outcome = Some(outcome);
        self.drained = Some(drained);
        self.stderr_task = Some(stderr_task);
        Ok(())
    }

    /// Czeka na koniec bieżącej tury.
    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        let told = self.outcome.take().ok_or_else(|| {
            anyhow!("this session has no turn in flight, so there is no outcome to wait for")
        })?;
        let Ok(outcome) = told.await else {
            mark_evidence_incomplete(self.evidence.as_ref());
            return Err(anyhow!("the turn ended without ever saying how it went"));
        };

        // Zebranie procesu MUSI paść na każdej ścieżce terminalnej — powód przy `send`.
        if let Some(process) = self.process.as_mut() {
            let _reaped = process.wait().await;
        }
        self.drain_current().await;
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
        let proof = process.stop(DEFAULT_GRACE).await;
        if proof_allows_cleanup(&proof) {
            self.drain_current().await;
        }
        proof
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
        self.drain_current().await;
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

    fn configured(&self, configuration: &DriverConfiguration) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(
            self.clone().with_configuration(configuration.clone()),
        ))
    }

    /// `-c model_reasoning_effort=<poziom>` — cała wiedza tego adaptera o szczeblu „ile myśleć".
    ///
    /// Poziom przychodzi gotowy z jedynej tabeli (`library::agents::effort_level`), więc tu nie
    /// ma ani jednego `match`: dopisanie go byłoby drugą kopią tamtej tabeli (niezmiennik 23).
    ///
    /// Para `-c KLUCZ=WARTOŚĆ` jedzie do argv PRZED `exec` — o to dba [`exec_argv`], bo tam
    /// mieszka kolejność. Tutaj powstaje sama para i nic poza nią.
    fn effort_argv(&self, level: &str) -> Vec<String> {
        vec!["-c".to_owned(), format!("{EFFORT_KEY}={level}")]
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
            Err(_error) => {
                tracing::debug!(
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

    async fn start_conversation(
        &self,
        spec: RunSpec,
        images: ValidatedImages,
        tx: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        Ok(Box::new(
            self.start_app_conversation(spec, images, tx).await?,
        ))
    }

    fn with_evidence(&self, target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        let mut configured = self.clone();
        configured.evidence = Some(target);
        Some(Arc::new(configured))
    }

    /* NIE ZAWĘŻA, I TO JEST FAKT O TYM CLI, NIE NASZE USTĘPSTWO (2026-08-24, T-97).
     *
     * Codex nie ma odpowiednika `--tools`: to, po co agent może sięgnąć, wynika u niego wyłącznie
     * z trybu piaskownicy — dokładnie tego, co składa [`build_exec_argv`] z `spec.policy`.
     * `library::agents::CAPABILITIES` mówi o tym polu `Unavailable` od T-11, formularz agenta
     * wygasza je przy tym vendorze, a `spec.tools` nie czyta w tym pliku ani jedna linia.
     *
     * Do tego dnia `commands::run` sądziło mimo to listę takiego agenta przeciw suficie Claude'a
     * i potrafiło odmówić CAŁEGO biegu o wpis, którego ten adapter i tak nigdy nie zobaczy. */
    fn narrows_its_tools(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod stop_proof_tests {
    use std::io::{self, Write};
    use std::path::PathBuf;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex, PoisonError};
    use std::task::{Context, Poll};
    use std::time::Duration;

    use tempfile::TempDir;
    use tokio::io::{AsyncRead, ReadBuf};
    use tokio::process::Command;
    use tokio::sync::{mpsc, oneshot};

    use super::{
        AgentHandle, AppClient, AppServerInput, AppServerState, CodexConversationHandle,
        CodexDriver, CodexHandle, DriverConfiguration, GroupProof, Turn, app_server_actor,
        proof_allows_cleanup, remember_thread, stop_startup_process,
    };
    use crate::engine::supervisor::{self, StdinPlan, Supervised};
    use crate::evidence::{EvidenceTarget, SafeInputManifest};

    #[derive(Debug)]
    struct Scribe(Arc<Mutex<Vec<u8>>>);

    struct FailingOutput;

    impl AsyncRead for FailingOutput {
        fn poll_read(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
            _buffer: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Ready(Err(io::Error::other(
                "deterministic App Server reader failure",
            )))
        }
    }

    impl Write for Scribe {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn target(directory: &TempDir, name: &str) -> EvidenceTarget {
        EvidenceTarget::workflow_step(
            directory.path().to_path_buf(),
            name.to_owned(),
            SafeInputManifest::default(),
        )
    }

    fn empty_handle(evidence: EvidenceTarget) -> CodexHandle {
        let (events, _inbox) = mpsc::channel(1);
        CodexHandle {
            binary: PathBuf::from("codex"),
            cwd: PathBuf::from("/private/workspace"),
            events,
            evidence: Some(evidence),
            configuration: DriverConfiguration::default(),
            threads: Arc::new(Mutex::new(vec!["vendor-thread".to_owned()])),
            cancelled: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            number: 1,
            process: None,
            outcome: None,
            drained: None,
            stderr_task: None,
        }
    }

    fn empty_app_handle(evidence: EvidenceTarget) -> CodexConversationHandle {
        let (commands, command_inbox) = mpsc::channel(1);
        drop(command_inbox);
        let (outcome_sender, outcomes) = mpsc::channel(1);
        drop(outcome_sender);
        CodexConversationHandle {
            process: None,
            client: AppClient::new(commands, Some(evidence.clone())),
            evidence: Some(evidence),
            session_id: String::new(),
            active_turn: None,
            in_flight: false,
            outcomes,
            reader_task: None,
            stderr_task: None,
        }
    }

    fn sleeping_process() -> anyhow::Result<Supervised> {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 60"]);
        Ok(supervisor::spawn(command, StdinPlan::Null)?)
    }

    #[test]
    fn alive_keeps_process_readers_and_evidence_owned_for_retry() {
        assert!(!proof_allows_cleanup(&GroupProof::Alive));
        assert!(proof_allows_cleanup(&GroupProof::Dead { status: None }));
    }

    #[tokio::test]
    async fn actual_app_handle_keeps_process_tasks_state_and_evidence_until_dead()
    -> anyhow::Result<()> {
        let directory = TempDir::new()?;
        let evidence = target(&directory, "app-alive-then-dead");
        let state = Arc::new(Mutex::new(AppServerState::new()));
        state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .begin_turn();

        let (release_reader, reader_release) = oneshot::channel();
        let reader_state = Arc::clone(&state);
        let reader_task = tokio::spawn(async move {
            let _released = reader_release.await;
            reader_state
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .active = false;
        });
        let (release_stderr, stderr_release) = oneshot::channel();
        let stderr_task = tokio::spawn(async move {
            let _released = stderr_release.await;
        });

        let mut handle = empty_app_handle(evidence.clone());
        handle.process = Some(sleeping_process()?);
        handle.reader_task = Some(reader_task);
        handle.stderr_task = Some(stderr_task);
        let group = handle.group();

        tokio::time::timeout(
            Duration::from_millis(200),
            handle.cleanup_after_proof(&GroupProof::Alive),
        )
        .await?;

        assert_eq!(
            handle.group(),
            group,
            "Alive dropped the only process owner"
        );
        assert!(
            handle.reader_task.is_some(),
            "Alive dropped the App Server state owner"
        );
        assert!(
            handle.stderr_task.is_some(),
            "Alive dropped the complaint reader owner"
        );
        assert!(
            handle.evidence.is_some(),
            "Alive dropped the evidence target"
        );
        assert!(
            evidence.is_healthy(),
            "retaining an Alive group is not an evidence failure"
        );
        assert!(
            state.lock().unwrap_or_else(PoisonError::into_inner).active,
            "the reader-owned AppServerState was cleared before a Dead proof"
        );

        release_reader
            .send(())
            .map_err(|()| anyhow::anyhow!("the App Server reader was not retained after Alive"))?;
        release_stderr
            .send(())
            .map_err(|()| anyhow::anyhow!("the complaint reader was not retained after Alive"))?;
        let process = handle
            .process
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("the second Stop lost its process owner"))?;
        let proof = stop_startup_process(process).await;
        assert!(matches!(&proof, GroupProof::Dead { .. }));
        handle.cleanup_after_proof(&proof).await;

        assert!(
            handle.process.is_none(),
            "Dead did not release the process owner"
        );
        assert!(
            handle.reader_task.is_none(),
            "Dead did not join the App Server reader"
        );
        assert!(
            handle.stderr_task.is_none(),
            "Dead did not join the complaint reader"
        );
        assert!(
            !state.lock().unwrap_or_else(PoisonError::into_inner).active,
            "the joined reader left AppServerState active"
        );
        assert!(
            evidence.is_healthy(),
            "clean joins after Dead must keep evidence complete"
        );
        Ok(())
    }

    #[tokio::test]
    async fn actual_app_reader_error_poisons_its_evidence_target() -> anyhow::Result<()> {
        let directory = TempDir::new()?;
        let evidence = target(&directory, "app-read-error");
        let mut command = Command::new("sh");
        command.args(["-c", "read _line"]);
        let mut process = supervisor::spawn(command, StdinPlan::Keep(String::new()))?;
        let stdin = process
            .stdin()
            .await
            .ok_or_else(|| anyhow::anyhow!("the App Server process has no input pipe"))?;
        let (commands, command_inbox) = mpsc::channel(1);
        let (events, _event_inbox) = mpsc::channel(1);
        let (outcomes, _outcome_inbox) = mpsc::channel(1);

        app_server_actor(AppServerInput {
            stdin,
            stdout: FailingOutput,
            commands: command_inbox,
            events,
            outcomes,
            complaint: Arc::new(Mutex::new(String::new())),
            evidence: None,
            evidence_target: Some(evidence.clone()),
        })
        .await;
        drop(commands);

        assert!(
            !evidence.is_healthy(),
            "an App Server stdout read error must make a later Finished receipt ineligible"
        );
        let proof = stop_startup_process(&mut process).await;
        assert!(matches!(proof, GroupProof::Dead { .. }));
        Ok(())
    }

    #[tokio::test]
    async fn each_actual_app_reader_join_error_poisons_its_own_target() -> anyhow::Result<()> {
        let directory = TempDir::new()?;

        let stdout_evidence = target(&directory, "app-stdout-join");
        let mut stdout_handle = empty_app_handle(stdout_evidence.clone());
        let stdout_reader = tokio::spawn(async { std::future::pending::<()>().await });
        stdout_reader.abort();
        stdout_handle.reader_task = Some(stdout_reader);
        stdout_handle.finish_tasks().await;
        assert!(
            !stdout_evidence.is_healthy(),
            "an App Server stdout JoinError must poison its receipt"
        );

        let stderr_evidence = target(&directory, "app-stderr-join");
        let mut stderr_handle = empty_app_handle(stderr_evidence.clone());
        let stderr_reader = tokio::spawn(async { std::future::pending::<()>().await });
        stderr_reader.abort();
        stderr_handle.stderr_task = Some(stderr_reader);
        stderr_handle.finish_tasks().await;
        assert!(
            !stderr_evidence.is_healthy(),
            "an App Server stderr JoinError must poison its receipt"
        );
        Ok(())
    }

    #[tokio::test]
    async fn closed_app_and_exec_channels_poison_the_actual_evidence_targets() -> anyhow::Result<()>
    {
        let directory = TempDir::new()?;
        let app_target = target(&directory, "app-channel");
        let (commands, inbox) = mpsc::channel(1);
        drop(inbox);
        let client = AppClient::new(commands, Some(app_target.clone()));
        assert!(
            client
                .request("initialize", serde_json::json!({}), false)
                .await
                .is_err(),
            "a closed App Server command channel must be a transport failure"
        );
        assert!(
            !app_target.is_healthy(),
            "the receipt must stay incomplete after the App Server channel disappears"
        );

        let exec_target = target(&directory, "exec-channel");
        let mut handle = empty_handle(exec_target.clone());
        let (finished, outcome) = oneshot::channel();
        drop(finished);
        handle.outcome = Some(outcome);
        assert!(
            handle.wait().await.is_err(),
            "a vanished exec outcome sender must not manufacture a finished turn"
        );
        assert!(
            !exec_target.is_healthy(),
            "a post-channel Finished result cannot be accepted as complete evidence"
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_reader_join_failure_poisons_the_actual_exec_target() -> anyhow::Result<()> {
        let directory = TempDir::new()?;
        let evidence = target(&directory, "join-failure");
        let mut handle = empty_handle(evidence.clone());
        let reader = tokio::spawn(async { std::future::pending::<()>().await });
        reader.abort();
        handle.stderr_task = Some(reader);

        handle.drain_current().await;

        assert!(
            !evidence.is_healthy(),
            "a reader that did not join cannot leave the receipt eligible for completion"
        );
        Ok(())
    }

    #[test]
    fn actual_debug_objects_and_thread_change_trace_redact_control_data() {
        const BINARY: &str = "/PRIVATE_BINARY_SENTINEL/codex";
        const WORKSPACE: &str = "/PRIVATE_WORKSPACE_SENTINEL/project";
        const MODEL: &str = "PRIVATE_MODEL_SENTINEL";
        const FIRST_THREAD: &str = "PRIVATE_VENDOR_THREAD_SENTINEL_A";
        const NEXT_THREAD: &str = "PRIVATE_VENDOR_THREAD_SENTINEL_B";

        let (events, _inbox) = mpsc::channel(1);
        let turn = Turn {
            binary: PathBuf::from(BINARY),
            cwd: PathBuf::from(WORKSPACE),
            argv: vec![
                "exec".to_owned(),
                "-C".to_owned(),
                WORKSPACE.to_owned(),
                "-m".to_owned(),
                MODEL.to_owned(),
                "resume".to_owned(),
                FIRST_THREAD.to_owned(),
            ],
            prompt: "PRIVATE_PROMPT_SENTINEL".to_owned(),
            events: events.clone(),
            threads: Arc::new(Mutex::new(vec![FIRST_THREAD.to_owned()])),
            number: 2,
            cancelled: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            stdout_evidence: None,
            stderr_evidence: None,
            evidence_target: None,
            configuration: DriverConfiguration::default(),
        };
        let turn_debug = format!("{turn:?}");

        let handle = CodexHandle {
            binary: PathBuf::from(BINARY),
            cwd: PathBuf::from(WORKSPACE),
            events,
            evidence: None,
            threads: Arc::new(Mutex::new(vec![FIRST_THREAD.to_owned()])),
            cancelled: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            number: 2,
            process: None,
            outcome: None,
            drained: None,
            stderr_task: None,
            configuration: DriverConfiguration::default(),
        };
        let handle_debug = format!("{handle:?}");
        let driver_debug = format!("{:?}", CodexDriver::with_binary(PathBuf::from(BINARY)));

        let notes = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&notes);
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_ansi(false)
            .with_writer(move || Scribe(Arc::clone(&sink)))
            .finish();
        let threads = Mutex::new(vec![FIRST_THREAD.to_owned()]);
        let mut seen = None;
        tracing::subscriber::with_default(subscriber, || {
            let mut decoder = super::CodexDecoder::new();
            decoder.thread = Some(NEXT_THREAD.to_owned());
            remember_thread(&decoder, &mut seen, &threads);
        });
        let trace = {
            let held = notes.lock().unwrap_or_else(PoisonError::into_inner);
            String::from_utf8_lossy(&held).into_owned()
        };
        let exposed = format!("{turn_debug}\n{handle_debug}\n{driver_debug}\n{trace}");

        assert!(
            trace.contains("different thread id"),
            "the oracle must exercise the real trace"
        );
        for private in [BINARY, WORKSPACE, MODEL, FIRST_THREAD, NEXT_THREAD] {
            assert!(
                !exposed.contains(private),
                "private Codex control data escaped through Debug or tracing: {exposed}"
            );
        }
        assert!(turn_debug.contains("argument_count"));
        assert!(handle_debug.contains("thread_count"));
        assert!(driver_debug.contains("uses_custom_binary"));
    }
}
