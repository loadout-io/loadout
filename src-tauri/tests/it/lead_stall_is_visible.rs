//! Cisza Leada nie może wyglądać jak zdrowa rozmowa.
//!
//! # Trzy osiągalne drogi do ciszy
//!
//! Pierwsza stoi w moście pytań. `Desk::ask` odkładało kanał odpowiedzi, próbowało pokazać
//! `Line::Asked`, ignorowało `Sent::Dropped`, a potem czekało bez końca. Pełna kolejka ekranu
//! zmieniała więc widoczne pytanie w niewidzialną blokadę całej tury.
//!
//! Druga stoi przy czytniku rozmowy. EOF kanału zdarzeń kończył `read_along`, lecz actor będący
//! jedynym właścicielem `Session` nic o tym nie wiedział. Wątek pozostawał `active`, choć nie było
//! już drogi, którą mogłaby przyjść odpowiedź.
//!
//! Trzecia zostawia proces i kanał zdarzeń otwarte, lecz nigdy nie wysyła `Finished`. Sam EOF tej
//! wersji nie wykryje. Limit zapisany przy agencie musi więc należeć do każdej przyjętej odpowiedzi,
//! a jego przekroczenie ma przejść przez pełne zatrzymanie procesu i zdanie widoczne na ekranie.
//!
//! # Słabe wersje kryterium
//!
//! Samo `say_in(...).is_ok()` przechodzi w obu wadliwych stanach: znaczy tylko, że vendor przyjął
//! wejście. Sam wynik `Answer::Refused` także nie wystarcza — test musi dowieść, że przy pełnej
//! kolejce wraca bez odpowiedzi człowieka, zamiast wisieć.

#![allow(clippy::expect_used)]

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::bridge::host::{Answers, Bridge};
use loadout_lib::bridge::library::{Desk, Waiting};
use loadout_lib::bridge::{Answer, Call, Role};
use loadout_lib::commands::Drivers;
use loadout_lib::commands::chat::{LEAD, Lead, Terminal, Threads};
use loadout_lib::durable_file::{
    FaultAction, FaultInjector, FaultPoint, PublicationEvent, scoped_faults,
};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome,
    Outcome as TurnOutcome, Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::line::{Line, LineKind};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::{
    ConversationMetadata, ConversationVendor, EvidenceTarget, SafeInputManifest, TurnCounters,
};
use loadout_lib::ipc::{Sent, line_channel};
use loadout_lib::library::agents::Agent;
use serde_json::{Value as Json, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{Notify, mpsc};
use uuid::Uuid;

const WAIT: Duration = Duration::from_secs(1);
const DEADLINE_SETTLE: Duration = Duration::from_millis(100);

#[derive(Debug)]
struct EndsWithoutAnswer {
    starts: Arc<AtomicUsize>,
}

#[async_trait]
impl AgentDriver for EndsWithoutAnswer {
    fn id(&self) -> &'static str {
        "ends-without-answer"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("fixture".to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        _events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        /* `_events` ginie przy wyjściu: dokładnie tak wygląda proces, którego stdout się zamknął
         * bez umówionego `Finished`. Uchwyt zostaje, żeby actor nadal musiał go rozliczyć. */
        Ok(Box::new(EndedHandle {
            session: SessionRef {
                vendor: "ends-without-answer",
                id: spec.run_id.to_string(),
            },
        }))
    }
}

#[derive(Debug)]
struct EndedHandle {
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for EndedHandle {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        anyhow::bail!("the fixture has already stopped listening")
    }

    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        anyhow::bail!("the fixture ended without a turn result")
    }

    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(1))
    }
}

fn one_driver() -> (Drivers, Arc<AtomicUsize>) {
    let starts = Arc::new(AtomicUsize::new(0));
    let driver: Arc<dyn AgentDriver> = Arc::new(EndsWithoutAnswer {
        starts: Arc::clone(&starts),
    });
    (Arc::new(move |_vendor| Arc::clone(&driver)), starts)
}

#[derive(Debug)]
struct EndsButStaysAlive {
    starts: Arc<AtomicUsize>,
    cancels: Arc<AtomicUsize>,
    finishes_before_eof: bool,
}

#[async_trait]
impl AgentDriver for EndsButStaysAlive {
    fn id(&self) -> &'static str {
        "ends-but-stays-alive"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("fixture".to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        let session = SessionRef {
            vendor: "ends-but-stays-alive",
            id: spec.run_id.to_string(),
        };
        if self.finishes_before_eof {
            events.send(completed(session.clone())).await?;
        }
        Ok(Box::new(AliveThenDeadHandle {
            cancels: Arc::clone(&self.cancels),
            session,
        }))
    }
}

#[derive(Debug)]
struct AliveThenDeadHandle {
    cancels: Arc<AtomicUsize>,
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for AliveThenDeadHandle {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        anyhow::bail!("the fixture stopped accepting input")
    }

    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        anyhow::bail!("the fixture ended without a turn result")
    }

    async fn cancel(&mut self) -> GroupProof {
        if self.cancels.fetch_add(1, Ordering::SeqCst) == 0 {
            GroupProof::Alive { group: None }
        } else {
            GroupProof::Dead { status: None }
        }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(1))
    }
}

fn alive_eof_driver() -> (Drivers, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let starts = Arc::new(AtomicUsize::new(0));
    let cancels = Arc::new(AtomicUsize::new(0));
    let driver: Arc<dyn AgentDriver> = Arc::new(EndsButStaysAlive {
        starts: Arc::clone(&starts),
        cancels: Arc::clone(&cancels),
        finishes_before_eof: false,
    });
    (
        Arc::new(move |_vendor| Arc::clone(&driver)),
        starts,
        cancels,
    )
}

fn finished_alive_eof_driver() -> (Drivers, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let starts = Arc::new(AtomicUsize::new(0));
    let cancels = Arc::new(AtomicUsize::new(0));
    let driver: Arc<dyn AgentDriver> = Arc::new(EndsButStaysAlive {
        starts: Arc::clone(&starts),
        cancels: Arc::clone(&cancels),
        finishes_before_eof: true,
    });
    (
        Arc::new(move |_vendor| Arc::clone(&driver)),
        starts,
        cancels,
    )
}

#[derive(Debug)]
struct DelayedStartState {
    starts: AtomicUsize,
    cancels: AtomicUsize,
    release: Notify,
}

#[derive(Debug)]
struct DelayedStart {
    state: Arc<DelayedStartState>,
}

#[async_trait]
impl AgentDriver for DelayedStart {
    fn id(&self) -> &'static str {
        "delayed-start"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("fixture".to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.state.starts.fetch_add(1, Ordering::SeqCst);
        self.state.release.notified().await;
        Ok(Box::new(DelayedStartHandle {
            state: Arc::clone(&self.state),
            session: SessionRef {
                vendor: "delayed-start",
                id: spec.run_id.to_string(),
            },
            events: Some(events),
        }))
    }
}

#[derive(Debug)]
struct DelayedStartHandle {
    state: Arc<DelayedStartState>,
    session: SessionRef,
    events: Option<mpsc::Sender<DecodedEvent>>,
}

#[async_trait]
impl AgentHandle for DelayedStartHandle {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        anyhow::bail!("the fixture does not accept a follow-up")
    }

    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        std::future::pending().await
    }

    async fn cancel(&mut self) -> GroupProof {
        self.state.cancels.fetch_add(1, Ordering::SeqCst);
        self.events.take();
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

fn delayed_start_driver() -> (Drivers, Arc<DelayedStartState>) {
    let state = Arc::new(DelayedStartState {
        starts: AtomicUsize::new(0),
        cancels: AtomicUsize::new(0),
        release: Notify::new(),
    });
    let driver: Arc<dyn AgentDriver> = Arc::new(DelayedStart {
        state: Arc::clone(&state),
    });
    (Arc::new(move |_vendor| Arc::clone(&driver)), state)
}

#[derive(Debug)]
struct HoldFirstTurnCommit {
    entered: Mutex<Option<std_mpsc::Sender<()>>>,
    release: Mutex<std_mpsc::Receiver<()>>,
    held: AtomicBool,
}

impl FaultInjector for HoldFirstTurnCommit {
    fn action(&self, event: &PublicationEvent) -> FaultAction {
        let is_first_turn = event.target.ends_with(Path::new("turns").join("0001.json"));
        if event.point == FaultPoint::BeforeCommit
            && is_first_turn
            && !self.held.swap(true, Ordering::SeqCst)
        {
            if let Some(entered) = self
                .entered
                .lock()
                .expect("the publication entry signal was poisoned")
                .take()
            {
                let _ = entered.send(());
            }
            let _ = self
                .release
                .lock()
                .expect("the publication release signal was poisoned")
                .recv();
        }
        FaultAction::Continue
    }
}

fn lead() -> Lead {
    Lead {
        agent: Agent::example(),
    }
}

fn lead_with_limit(minutes: u32) -> Lead {
    let mut agent = Agent::example();
    agent.give_up_after_minutes = minutes;
    Lead { agent }
}

#[derive(Debug)]
struct OpenReaderState {
    starts: AtomicUsize,
    cancels: AtomicUsize,
    waits: AtomicUsize,
    wait_drops: AtomicUsize,
    events: Mutex<Option<mpsc::Sender<DecodedEvent>>>,
    closes_events_on_cancel: bool,
    finishes_when_wait_is_dropped: bool,
}

impl OpenReaderState {
    fn new(closes_events_on_cancel: bool, finishes_when_wait_is_dropped: bool) -> Self {
        Self {
            starts: AtomicUsize::new(0),
            cancels: AtomicUsize::new(0),
            waits: AtomicUsize::new(0),
            wait_drops: AtomicUsize::new(0),
            events: Mutex::new(None),
            closes_events_on_cancel,
            finishes_when_wait_is_dropped,
        }
    }

    fn events(&self) -> mpsc::Sender<DecodedEvent> {
        self.events
            .lock()
            .expect("the fixture event slot was poisoned")
            .as_ref()
            .expect("the Lead process has not started yet")
            .clone()
    }
}

#[derive(Debug)]
struct NeverFinishes {
    state: Arc<OpenReaderState>,
}

#[async_trait]
impl AgentDriver for NeverFinishes {
    fn id(&self) -> &'static str {
        "never-finishes"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("fixture".to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.state.starts.fetch_add(1, Ordering::SeqCst);
        *self
            .state
            .events
            .lock()
            .expect("the fixture event slot was poisoned") = Some(events);
        Ok(Box::new(NeverFinishesHandle {
            state: Arc::clone(&self.state),
            session: SessionRef {
                vendor: "never-finishes",
                id: spec.run_id.to_string(),
            },
        }))
    }
}

#[derive(Debug)]
struct NeverFinishesHandle {
    state: Arc<OpenReaderState>,
    session: SessionRef,
}

/// Wysyła wynik starej tury dokładnie wtedy, gdy actor porzuca oczekiwanie na nią.
///
/// To jest deterministyczny szew wyścigu: pierwszy check deadline'u widzi ciszę, porzucenie
/// `handle.wait()` budzi prawdziwy `Finished`, a drugi check może go już zobaczyć. Bez sygnału
/// z `Drop` test zależałby od liczby tur schedulera i czasem sądziłby inne okno niż produkcja.
struct FinishWhenWaitDrops {
    state: Option<Arc<OpenReaderState>>,
    session: SessionRef,
}

impl Drop for FinishWhenWaitDrops {
    fn drop(&mut self) {
        let Some(state) = self.state.take() else {
            return;
        };
        state.wait_drops.fetch_add(1, Ordering::SeqCst);
        let events = state
            .events
            .lock()
            .expect("the fixture event slot was poisoned")
            .as_ref()
            .cloned();
        if let Some(events) = events {
            let _ = events.try_send(completed(self.session.clone()));
        }
    }
}

#[async_trait]
impl AgentHandle for NeverFinishesHandle {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        self.state.waits.fetch_add(1, Ordering::SeqCst);
        let _finish = FinishWhenWaitDrops {
            state: self
                .state
                .finishes_when_wait_is_dropped
                .then(|| Arc::clone(&self.state)),
            session: self.session.clone(),
        };
        std::future::pending().await
    }

    async fn cancel(&mut self) -> GroupProof {
        self.state.cancels.fetch_add(1, Ordering::SeqCst);
        /* To zamknięcie jest skutkiem dowiedzionego zatrzymania procesu, nie symulowanym EOF.
         * Przed `cancel` nadajnik pozostaje żywy bez `Finished`, czyli kryterium mierzy deadline. */
        if self.state.closes_events_on_cancel {
            self.state
                .events
                .lock()
                .expect("the fixture event slot was poisoned")
                .take();
        }
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

fn open_reader_driver() -> (Drivers, Arc<OpenReaderState>) {
    open_reader_driver_that(true)
}

fn open_reader_driver_that(closes_events_on_cancel: bool) -> (Drivers, Arc<OpenReaderState>) {
    open_reader_driver_with(closes_events_on_cancel, false)
}

fn open_reader_driver_with(
    closes_events_on_cancel: bool,
    finishes_when_wait_is_dropped: bool,
) -> (Drivers, Arc<OpenReaderState>) {
    let state = Arc::new(OpenReaderState::new(
        closes_events_on_cancel,
        finishes_when_wait_is_dropped,
    ));
    let driver: Arc<dyn AgentDriver> = Arc::new(NeverFinishes {
        state: Arc::clone(&state),
    });
    (Arc::new(move |_vendor| Arc::clone(&driver)), state)
}

#[derive(Debug, Default)]
struct QueuedQuestionState {
    starts: AtomicUsize,
    cancels: AtomicUsize,
    calls_written: AtomicUsize,
    bridge_failures: AtomicUsize,
}

#[derive(Clone, Debug)]
struct QueuedQuestionDriver {
    state: Arc<QueuedQuestionState>,
    socket: PathBuf,
}

#[async_trait]
impl AgentDriver for QueuedQuestionDriver {
    fn id(&self) -> &'static str {
        "queued-question"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("fixture".to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.state.starts.fetch_add(1, Ordering::SeqCst);
        let socket = self.socket.clone();
        let state = Arc::clone(&self.state);
        let call = tokio::spawn(async move {
            if queued_question(socket, Arc::clone(&state)).await.is_err() {
                state.bridge_failures.fetch_add(1, Ordering::SeqCst);
            }
        });
        Ok(Box::new(QueuedQuestionHandle {
            state: Arc::clone(&self.state),
            call,
            events: Some(events),
            session: SessionRef {
                vendor: "queued-question",
                id: spec.run_id.to_string(),
            },
        }))
    }
}

async fn queued_question(socket: PathBuf, state: Arc<QueuedQuestionState>) -> anyhow::Result<()> {
    let stream = UnixStream::connect(socket).await?;
    let (reading, mut writing) = stream.into_split();
    let mut reading = BufReader::new(reading);
    let mut greeting = String::new();
    reading.read_line(&mut greeting).await?;

    let mut bytes = serde_json::to_vec(&Call {
        id: json!("queued-question"),
        call: "ask_the_person".to_owned(),
        input: json!({ "question": "Which path should I inspect?" }),
    })?;
    bytes.push(b'\n');
    writing.write_all(&bytes).await?;
    writing.flush().await?;
    state.calls_written.fetch_add(1, Ordering::SeqCst);

    /* Brak odczytu `LineSource` imituje pompę, która przyjęła wiersz, ale nie dostarczyła go do
     * okna. Most czeka na odpowiedź aż actor zastosuje skonfigurowany deadline całej tury. */
    let mut reply = String::new();
    reading.read_line(&mut reply).await?;
    Ok(())
}

#[derive(Debug)]
struct QueuedQuestionHandle {
    state: Arc<QueuedQuestionState>,
    call: tokio::task::JoinHandle<()>,
    events: Option<mpsc::Sender<DecodedEvent>>,
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for QueuedQuestionHandle {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        std::future::pending().await
    }

    async fn cancel(&mut self) -> GroupProof {
        self.state.cancels.fetch_add(1, Ordering::SeqCst);
        self.call.abort();
        self.events.take();
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

fn queued_question_driver(socket: PathBuf) -> (Drivers, Arc<QueuedQuestionState>) {
    let state = Arc::new(QueuedQuestionState::default());
    let driver: Arc<dyn AgentDriver> = Arc::new(QueuedQuestionDriver {
        state: Arc::clone(&state),
        socket,
    });
    (Arc::new(move |_vendor| Arc::clone(&driver)), state)
}

fn completed(session: SessionRef) -> DecodedEvent {
    AgentEvent::Finished(TurnOutcome {
        ok: true,
        reason: FinishReason::Completed,
        text: "Finished at the deadline".to_owned(),
        cost_usd: None,
        tokens: Tokens::default(),
        turns: 1,
        took: Duration::from_mins(1),
        session,
    })
    .into()
}

async fn settle_actor() {
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
}

async fn visible_problem_after_scheduling(
    source: &mut loadout_lib::ipc::LineSource,
) -> Result<String, Box<dyn Error>> {
    for _ in 0..64 {
        while let Some(line) = source.try_next() {
            if line.kind() == LineKind::Problem {
                return Ok(line.text().to_owned());
            }
        }
        tokio::task::yield_now().await;
    }
    Err("the reply deadline passed, but no problem reached the screen".into())
}

async fn visible_through_problem(
    source: &mut loadout_lib::ipc::LineSource,
) -> Result<Vec<(LineKind, String)>, Box<dyn Error>> {
    let mut seen = Vec::new();
    for _ in 0..128 {
        while let Some(line) = source.try_next() {
            let is_problem = line.kind() == LineKind::Problem;
            seen.push((line.kind(), line.text().to_owned()));
            if is_problem {
                return Ok(seen);
            }
        }
        tokio::task::yield_now().await;
    }
    Err("the queued question never reached a visible reply-deadline problem".into())
}

fn assert_no_problem(source: &mut loadout_lib::ipc::LineSource) {
    while let Some(line) = source.try_next() {
        assert_ne!(
            line.kind(),
            LineKind::Problem,
            "a healthy or unlimited reply was shown as a problem: {}",
            line.text()
        );
    }
}

fn only_conversation_receipts(workspace: &Path) -> Result<(Json, Json), Box<dyn Error>> {
    let root = only_conversation_root(workspace)?;
    let conversation = serde_json::from_slice(&std::fs::read(root.join("conversation.json"))?)?;
    let turn = serde_json::from_slice(&std::fs::read(root.join("turns").join("0001.json"))?)?;
    Ok((conversation, turn))
}

fn only_conversation_root(workspace: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let conversations = workspace.join(".loadout").join("conversations");
    let roots: Vec<PathBuf> = std::fs::read_dir(&conversations)?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_type().ok()?.is_dir().then_some(entry.path()))
        .collect();
    let [root] = roots.as_slice() else {
        return Err(format!(
            "expected one private conversation receipt under {}, found {}",
            conversations.display(),
            roots.len()
        )
        .into());
    };
    Ok(root.clone())
}

fn conversation_turn(workspace: &Path, number: usize) -> Result<Json, Box<dyn Error>> {
    let root = only_conversation_root(workspace)?;
    Ok(serde_json::from_slice(&std::fs::read(
        root.join("turns").join(format!("{number:04}.json")),
    )?)?)
}

fn take_visible_problem(source: &mut loadout_lib::ipc::LineSource) -> Option<String> {
    let mut problem = None;
    while let Some(line) = source.try_next() {
        if line.kind() == LineKind::Problem {
            problem = Some(line.text().to_owned());
        }
    }
    problem
}

async fn terminal_turn_state_after_scheduling(
    workspace: &Path,
    number: usize,
) -> Result<Option<String>, Box<dyn Error>> {
    let until = std::time::Instant::now() + WAIT;
    loop {
        let turn = conversation_turn(workspace, number)?;
        let state = turn.get("state").and_then(Json::as_str).map(str::to_owned);
        if state.as_deref() != Some("sending") || std::time::Instant::now() >= until {
            return Ok(state);
        }
        /* Receipty publikują się w puli blokującej. Bariera czeka za pracą już zleconą przez
         * actora, lecz używa czasu ściennego: zegar Tokio jest celowo zamrożony w kryterium
         * granicy deadline'u i nie może sam zakończyć tego oczekiwania. */
        tokio::task::spawn_blocking(|| std::thread::sleep(Duration::from_millis(1))).await?;
        tokio::task::yield_now().await;
    }
}

async fn visible_problem(
    source: &mut loadout_lib::ipc::LineSource,
) -> Result<String, Box<dyn Error>> {
    tokio::time::timeout(WAIT, async {
        loop {
            if let Some(line) = source.try_next()
                && line.kind() == LineKind::Problem
            {
                return line.text().to_owned();
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "the Lead stopped producing events, but no problem reached the screen".into())
}

async fn wait_for_visible_done(
    source: &mut loadout_lib::ipc::LineSource,
) -> Result<(), Box<dyn Error>> {
    tokio::time::timeout(WAIT, async {
        loop {
            while let Some(line) = source.try_next() {
                if line.kind() == LineKind::Done {
                    return;
                }
            }
            /* `finish_turn` publikuje receipt w `spawn_blocking`. Samo `yield_now` na runtime
             * current-thread nie gwarantuje, że pula blokująca zdąży oddać ten etap czytnikowi. */
            let _ = tokio::task::spawn_blocking(|| {}).await;
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "the first Finished never crossed the real Lead reader".into())
}

async fn wait_for_blocked_handle(state: &OpenReaderState) -> Result<(), Box<dyn Error>> {
    tokio::time::timeout(WAIT, async {
        while state.waits.load(Ordering::SeqCst) != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "the follow-up did not reach the blocked handle.wait path in time")?;
    assert_eq!(
        state.waits.load(Ordering::SeqCst),
        1,
        "the follow-up never reached the blocked production handle.wait path"
    );
    Ok(())
}

#[tokio::test]
async fn a_reader_that_ends_without_finished_is_visible_and_the_next_message_starts_fresh()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let terminal = Terminal {
        id: "terminal-with-ended-reader".to_owned(),
        folder: project.path().to_path_buf(),
    };
    let (drivers, starts) = one_driver();
    let threads = Threads::new();
    let (sink, mut source) = line_channel(16);
    threads.terminal_lines_go_to(&terminal, sink);

    threads
        .say_in(&drivers, &lead(), &terminal, "Can you check this?")
        .await?;

    let problem = visible_problem(&mut source).await?;
    assert!(
        problem.contains("stopped") && problem.contains("reply"),
        "the screen has to say what stopped and what is missing, not expose a wire error: {problem}"
    );
    assert!(
        !threads.is_live_at(&terminal.id),
        "an event reader that ended without Finished stayed described as a live conversation"
    );
    let (conversation, turn) = only_conversation_receipts(project.path())?;
    assert_eq!(
        (
            conversation.get("state").and_then(Json::as_str),
            conversation.get("failureKind").and_then(Json::as_str),
            conversation.get("complete").and_then(Json::as_bool),
            conversation.get("deathProof").and_then(Json::as_bool),
        ),
        (Some("failed"), Some("agentFailed"), Some(true), Some(true)),
        "unexpected EOF is an agent failure with proved death, never an explicit cancellation: \
         {conversation}"
    );
    assert_eq!(
        (
            turn.get("state").and_then(Json::as_str),
            turn.get("failureKind").and_then(Json::as_str),
        ),
        (Some("failed"), Some("agentFailed")),
        "the unfinished delivered turn must carry the same terminal cause as its conversation: \
         {turn}"
    );

    threads
        .say_in(&drivers, &lead(), &terminal, "Try once more")
        .await
        .expect("the dead proof must let the same actor open a fresh conversation");
    assert_eq!(
        starts.load(Ordering::SeqCst),
        2,
        "the next message was handed to the ended session instead of opening a fresh one"
    );
    Ok(())
}

#[tokio::test]
async fn eof_with_alive_proof_stays_closing_and_a_later_stop_reuses_the_handle()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let terminal = Terminal {
        id: "terminal-with-alive-eof".to_owned(),
        folder: project.path().to_path_buf(),
    };
    let (drivers, starts, cancels) = alive_eof_driver();
    let threads = Threads::new();
    let (sink, mut source) = line_channel(16);
    threads.terminal_lines_go_to(&terminal, sink);

    threads
        .say_in(
            &drivers,
            &lead(),
            &terminal,
            "Do not lose the process handle",
        )
        .await?;
    let problem = visible_problem(&mut source).await?;
    assert!(
        problem.contains("still running") && problem.contains("still tracking"),
        "Alive must not promise a fresh conversation or hide the retained process: {problem}"
    );
    assert!(
        threads.is_live_at(&terminal.id),
        "GroupProof::Alive removed the Closing conversation and its only process handle"
    );
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert_eq!(cancels.load(Ordering::SeqCst), 1);

    let proof = threads
        .close_at(&terminal.id)
        .await
        .expect("Closing must retain the same handle for another Stop attempt");
    assert!(matches!(proof, GroupProof::Dead { .. }));
    assert_eq!(
        cancels.load(Ordering::SeqCst),
        2,
        "the retry did not reach the handle that previously returned Alive"
    );
    assert!(
        !threads.is_live_at(&terminal.id),
        "the second proof was Dead, but the terminal stayed Closing"
    );
    Ok(())
}

#[tokio::test]
async fn completed_reply_eof_with_alive_proof_stays_an_agent_failure_after_dead()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let terminal = Terminal {
        id: "terminal-with-finished-but-alive-eof".to_owned(),
        folder: project.path().to_path_buf(),
    };
    let (drivers, starts, cancels) = finished_alive_eof_driver();
    let threads = Threads::new();
    let (sink, mut source) = line_channel(16);
    threads.terminal_lines_go_to(&terminal, sink);

    threads
        .say_in(
            &drivers,
            &lead(),
            &terminal,
            "Finish this reply, then lose the event stream",
        )
        .await?;
    let problem = visible_problem(&mut source).await?;
    assert!(
        problem.contains("still running") && problem.contains("still tracking"),
        "the screen did not disclose the live process after its finished stream ended: {problem}"
    );
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert_eq!(cancels.load(Ordering::SeqCst), 1);

    let proof = threads
        .close_at(&terminal.id)
        .await
        .expect("the retained process needs a second proof");
    assert!(matches!(proof, GroupProof::Dead { .. }));
    let (conversation, turn) = only_conversation_receipts(project.path())?;
    assert_eq!(
        (
            conversation.get("state").and_then(Json::as_str),
            conversation.get("failureKind").and_then(Json::as_str),
            conversation.get("deathProof").and_then(Json::as_bool),
        ),
        (Some("failed"), Some("agentFailed"), Some(true)),
        "the visible reader failure was later rewritten as a healthy close: {conversation}"
    );
    assert_eq!(
        turn.get("state").and_then(Json::as_str),
        Some("succeeded"),
        "the completed reply itself should stay successful: {turn}"
    );
    Ok(())
}

#[tokio::test]
async fn losing_the_terminal_after_failed_eof_keeps_the_agent_failure_cause()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let terminal = Terminal {
        id: "terminal-dropped-after-alive-eof".to_owned(),
        folder: project.path().to_path_buf(),
    };
    let (drivers, _starts, cancels) = alive_eof_driver();
    let threads = Threads::new();
    let (sink, mut source) = line_channel(16);
    threads.terminal_lines_go_to(&terminal, sink);

    threads
        .say_in(&drivers, &lead(), &terminal, "Lose the terminal after EOF")
        .await?;
    let _problem = visible_problem(&mut source).await?;
    assert_eq!(cancels.load(Ordering::SeqCst), 1);

    /* Zniknięcie ostatniego właściciela zamyka oba kanały actora. Druga eskalacja musi zachować
     * przyczynę ustaloną przy EOF, a nie zamienić ją w późny, domyślny Stop. */
    drop(threads);
    tokio::time::timeout(WAIT, async {
        while cancels.load(Ordering::SeqCst) != 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "the orphaned closing actor never retried its retained process")?;
    tokio::time::timeout(WAIT, async {
        loop {
            if let Ok((conversation, _turn)) = only_conversation_receipts(project.path())
                && conversation.get("deathProof").and_then(Json::as_bool) == Some(true)
            {
                break;
            }
            let _barrier = tokio::task::spawn_blocking(|| {}).await;
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "the orphaned actor proved Dead but did not publish its terminal receipt")?;
    let (conversation, _turn) = only_conversation_receipts(project.path())?;
    assert_eq!(
        (
            conversation.get("state").and_then(Json::as_str),
            conversation.get("failureKind").and_then(Json::as_str),
        ),
        (Some("failed"), Some("agentFailed")),
        "channel close replaced a known reader failure with cancellation: {conversation}"
    );
    Ok(())
}

#[tokio::test]
async fn stop_during_the_first_start_is_answered_without_abandoning_start_ownership()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let terminal = Terminal {
        id: "terminal-stopped-during-start".to_owned(),
        folder: project.path().to_path_buf(),
    };
    let (drivers, state) = delayed_start_driver();
    let threads = Arc::new(Threads::new());
    let (sink, _source) = line_channel(16);
    threads.terminal_lines_go_to(&terminal, sink);

    let speaking = {
        let threads = Arc::clone(&threads);
        let drivers = Arc::clone(&drivers);
        let terminal = terminal.clone();
        tokio::spawn(async move {
            threads
                .say_in(&drivers, &lead_with_limit(1), &terminal, "Stop this start")
                .await
        })
    };
    tokio::time::timeout(WAIT, async {
        while state.starts.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "the fixture never entered AgentDriver::start")?;

    let stopped =
        tokio::time::timeout(Duration::from_millis(100), threads.close_at(&terminal.id)).await;
    state.release.notify_one();
    let stopped = stopped.map_err(|_| "Stop stayed queued behind the first driver start")?;
    assert!(
        matches!(stopped, Some(GroupProof::Alive { group: None })),
        "before start returns there is no honest Dead proof: {stopped:?}"
    );
    assert!(
        speaking.await?.is_err(),
        "the message whose start was stopped was reported as delivered"
    );
    tokio::time::timeout(WAIT, async {
        while state.cancels.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "the retained start result was never stopped after it became controllable")?;
    assert_eq!(state.cancels.load(Ordering::SeqCst), 1);
    tokio::time::timeout(WAIT, async {
        while threads.is_live_at(&terminal.id) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "the actor never published that the retained process was dead")?;

    /* Stop odpowiedział `Alive`, bo podczas handshake nie było jeszcze uchwytu. Kiedy actor
     * później uzyskał uchwyt i dowiódł `Dead`, ten sam terminal musi dostać świeżą rozmowę bez
     * drugiego kliknięcia Stop. Martwy wpis w rejestrze zamieniałby tę wiadomość w
     * `StoppedListening`, mimo że płatny proces już nie żyje. */
    let speaking_again = {
        let threads = Arc::clone(&threads);
        let drivers = Arc::clone(&drivers);
        let terminal = terminal.clone();
        tokio::spawn(async move {
            threads
                .say_in(
                    &drivers,
                    &lead_with_limit(1),
                    &terminal,
                    "Start a fresh conversation",
                )
                .await
        })
    };
    tokio::time::timeout(WAIT, async {
        while state.starts.load(Ordering::SeqCst) < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "the dead first actor remained registered for the terminal")?;
    state.release.notify_one();
    assert!(
        speaking_again.await?.is_ok(),
        "the terminal did not start a fresh conversation after the retained process died"
    );
    assert!(
        matches!(
            threads.close_at(&terminal.id).await,
            Some(GroupProof::Dead { .. })
        ),
        "the replacement conversation was not owned by the registry"
    );
    Ok(())
}

#[tokio::test]
async fn window_close_forgets_a_first_start_that_died_after_its_earlier_alive_proof()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let terminal = Terminal {
        id: "terminal-closed-after-delayed-start".to_owned(),
        folder: project.path().to_path_buf(),
    };
    let (drivers, state) = delayed_start_driver();
    let threads = Arc::new(Threads::new());
    let (sink, _source) = line_channel(16);
    threads.terminal_lines_go_to(&terminal, sink);

    let speaking = {
        let threads = Arc::clone(&threads);
        let drivers = Arc::clone(&drivers);
        let terminal = terminal.clone();
        tokio::spawn(async move {
            threads
                .say_in(
                    &drivers,
                    &lead_with_limit(1),
                    &terminal,
                    "Close this window",
                )
                .await
        })
    };
    tokio::time::timeout(WAIT, async {
        while state.starts.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "the fixture never entered AgentDriver::start")?;

    let first_proof =
        tokio::time::timeout(Duration::from_millis(100), threads.close_at(&terminal.id)).await;
    state.release.notify_one();
    assert!(
        matches!(first_proof?, Some(GroupProof::Alive { group: None })),
        "the first Stop invented a Dead proof before a process handle existed"
    );
    assert!(speaking.await?.is_err());
    tokio::time::timeout(WAIT, async {
        while threads.is_live_at(&terminal.id) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "the retained process never reached the actor's proven-closed state")?;

    let window_proofs = threads.close().await;
    assert!(
        window_proofs.is_empty(),
        "window close reported a proven-dead retained start as still alive: {window_proofs:?}"
    );
    assert!(
        threads.close_at(&terminal.id).await.is_none(),
        "window close left the stale generation in the terminal registry"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn the_first_reply_deadline_also_covers_a_driver_stuck_while_starting()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let terminal = Terminal {
        id: "terminal-with-stalled-start".to_owned(),
        folder: project.path().to_path_buf(),
    };
    let (drivers, state) = delayed_start_driver();
    let threads = Arc::new(Threads::new());
    let (sink, mut source) = line_channel(16);
    threads.terminal_lines_go_to(&terminal, sink);

    let speaking = {
        let threads = Arc::clone(&threads);
        let drivers = Arc::clone(&drivers);
        let terminal = terminal.clone();
        tokio::spawn(async move {
            threads
                .say_in(
                    &drivers,
                    &lead_with_limit(1),
                    &terminal,
                    "Do not go silent while starting",
                )
                .await
        })
    };
    tokio::time::timeout(WAIT, async {
        while state.starts.load(Ordering::SeqCst) == 0 {
            let _barrier = tokio::task::spawn_blocking(|| {}).await;
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "the fixture never entered AgentDriver::start")?;

    tokio::time::pause();
    tokio::time::advance(Duration::from_mins(1)).await;
    let problem = visible_problem_after_scheduling(&mut source).await;
    state.release.notify_one();
    tokio::time::resume();
    let problem = problem?;
    assert!(
        problem.contains("within 1 minute") && problem.contains("still tracking"),
        "a stalled start needs the same visible reply deadline and honest live status: {problem}"
    );
    assert!(speaking.await?.is_err());
    tokio::time::timeout(WAIT, async {
        while state.cancels.load(Ordering::SeqCst) == 0 {
            let _barrier = tokio::task::spawn_blocking(|| {}).await;
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "the timed-out start returned a handle but the actor did not stop it")?;
    assert_eq!(state.cancels.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn an_open_reader_without_finished_reaches_the_leads_deadline_and_starts_fresh()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let terminal = Terminal {
        id: "terminal-with-stalled-reply".to_owned(),
        folder: project.path().to_path_buf(),
    };
    let (drivers, state) = open_reader_driver();
    let threads = Threads::new();
    let (sink, mut source) = line_channel(16);
    threads.terminal_lines_go_to(&terminal, sink);

    threads
        .say_in(
            &drivers,
            &lead_with_limit(1),
            &terminal,
            "Please finish this reply",
        )
        .await?;
    assert_eq!(state.cancels.load(Ordering::SeqCst), 0);

    tokio::time::advance(Duration::from_mins(1)).await;
    settle_actor().await;
    tokio::time::advance(DEADLINE_SETTLE).await;
    let problem = visible_problem_after_scheduling(&mut source).await?;
    assert!(
        problem.contains("within 1 minute")
            && problem.contains("stopped")
            && problem.contains("fresh conversation"),
        "the screen must name the expired reply, proved stop, and next action: {problem}"
    );
    assert_eq!(
        state.cancels.load(Ordering::SeqCst),
        1,
        "the deadline changed UI state without stopping the owned process"
    );
    assert!(
        !threads.is_live_at(&terminal.id),
        "a process proved dead at its reply deadline stayed described as live"
    );

    threads
        .say_in(
            &drivers,
            &lead_with_limit(1),
            &terminal,
            "Start a fresh conversation",
        )
        .await?;
    assert_eq!(
        state.starts.load(Ordering::SeqCst),
        2,
        "the next message was sent to the timed-out session instead of starting fresh"
    );
    let _proofs = threads.close().await;
    Ok(())
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_reader_aborted_after_dead_proof_still_leaves_a_terminal_failed_receipt()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let terminal = Terminal {
        id: "terminal-with-reader-that-will-not-drain".to_owned(),
        folder: project.path().to_path_buf(),
    };
    /* Ten dubler zachowuje dodatkowy nadajnik nawet po `Dead`. Wymusza bezpiecznik readera,
     * zamiast pozwolić, by zwykły EOF przypadkiem zazielenił kryterium. */
    let (drivers, state) = open_reader_driver_that(false);
    let threads = Threads::new();
    let (sink, mut source) = line_channel(16);
    threads.terminal_lines_go_to(&terminal, sink);

    threads
        .say_in(
            &drivers,
            &lead_with_limit(1),
            &terminal,
            "The adapter will keep its reader open",
        )
        .await?;
    tokio::time::advance(Duration::from_mins(1)).await;
    settle_actor().await;
    tokio::time::advance(DEADLINE_SETTLE).await;
    settle_actor().await;
    tokio::time::advance(Duration::from_secs(1)).await;
    let _problem = visible_problem_after_scheduling(&mut source).await?;

    assert_eq!(state.cancels.load(Ordering::SeqCst), 1);
    assert!(
        !threads.is_live_at(&terminal.id),
        "reader cleanup failure resurrected a process already covered by GroupProof::Dead"
    );
    /* Problem jest widoczny natychmiast po dowodzie procesu. Finalizacja receiptu ma jeszcze
     * własny sekundowy bezpiecznik readera; przesuwamy go po publikacji problemu, a `close`
     * stanowi deterministyczną barierę actora zamiast zgadywania liczby tur schedulera. */
    tokio::time::advance(Duration::from_secs(1)).await;
    let _proofs = threads.close().await;
    let (conversation, turn) = only_conversation_receipts(project.path())?;
    assert_eq!(
        conversation.get("state").and_then(Json::as_str),
        Some("failed"),
        "the aborted reader left the conversation lifecycle active: {conversation}"
    );
    assert_eq!(
        conversation.get("failureKind").and_then(Json::as_str),
        Some("agentFailed"),
        "a reply deadline is an agent failure even when evidence drain is unhealthy: {conversation}"
    );
    assert_eq!(
        conversation.get("deathProof").and_then(Json::as_bool),
        Some(true),
        "the receipt discarded the Dead proof because its reader needed aborting: {conversation}"
    );
    assert_ne!(
        turn.get("state").and_then(Json::as_str),
        Some("delivered"),
        "the reader abort left its accepted turn nonterminal: {turn}"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn finished_at_the_reply_deadline_wins_the_race_and_is_not_cancelled()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let terminal = Terminal {
        id: "terminal-finishing-at-deadline".to_owned(),
        folder: project.path().to_path_buf(),
    };
    let (drivers, state) = open_reader_driver();
    let threads = Threads::new();
    let (sink, mut source) = line_channel(16);
    threads.terminal_lines_go_to(&terminal, sink);

    threads
        .say_in(
            &drivers,
            &lead_with_limit(1),
            &terminal,
            "Finish exactly at the boundary",
        )
        .await?;
    let events = state.events();
    let boundary = tokio::time::Instant::now() + Duration::from_mins(1);
    let finished = tokio::spawn(async move {
        tokio::time::sleep_until(boundary).await;
        /* Zdarzenie jest gotowe na granicy, lecz zajęty runtime może zaplanować pętlę czytającą
         * później niż sam timer. Stała liczba `yield_now` po stronie actora nie jest protokołem:
         * dokładamy więcej gotowych tur niż stary arbitraż, żeby test odtwarzał obciążenie. */
        for _ in 0..32 {
            tokio::task::yield_now().await;
        }
        events
            .send(completed(SessionRef {
                vendor: "never-finishes",
                id: "boundary".to_owned(),
            }))
            .await
    });
    /* Nadajnik musi najpierw zarejestrować swój zegar. Inaczej `advance` mierzyłoby wyłącznie
     * deadline actora, a nie wyścig dwóch zdarzeń przypadających na tę samą chwilę. */
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_mins(1)).await;
    finished
        .await
        .expect("the boundary event task panicked")
        .expect("the actor cancelled before accepting Finished at the boundary");
    settle_actor().await;

    assert_eq!(
        state.cancels.load(Ordering::SeqCst),
        0,
        "Finished ready at the deadline lost to the timer and killed a healthy conversation"
    );
    assert!(
        threads.is_live_at(&terminal.id),
        "a normally finished reply closed the reusable conversation"
    );
    assert_no_problem(&mut source);
    let _proofs = threads.close().await;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn a_late_finished_cannot_leave_the_follow_up_future_abandoned_and_sending()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let terminal = Terminal {
        id: "terminal-with-finished-between-deadline-checks".to_owned(),
        folder: project.path().to_path_buf(),
    };
    /* `wait()` jest wysyłką kolejnej tury w sesji bez stałego głosu. Jego Drop wpuszcza
     * `Finished` starej tury dokładnie po pierwszej decyzji deadline'u, więc kryterium nie
     * zgaduje wyścigu liczbą `yield_now`. */
    let (drivers, state) = open_reader_driver_with(true, true);
    let threads = Arc::new(Threads::new());
    let (sink, mut source) = line_channel(16);
    threads.terminal_lines_go_to(&terminal, sink);

    threads
        .say_in(
            &drivers,
            &lead_with_limit(1),
            &terminal,
            "The first reply will finish at the second deadline check",
        )
        .await?;

    let follow_threads = Arc::clone(&threads);
    let follow_drivers = Arc::clone(&drivers);
    let follow_terminal = terminal.clone();
    let follow = tokio::spawn(async move {
        follow_threads
            .say_in(
                &follow_drivers,
                &lead_with_limit(1),
                &follow_terminal,
                "This follow-up must not be left half-delivered",
            )
            .await
    });
    wait_for_blocked_handle(&state).await?;

    /* Zegar zamrażamy dopiero po wejściu follow-upu w `handle.wait()`. `start_paused` mogłoby
     * auto-przesunąć pierwszą minutę podczas asynchronicznego zapisu receiptu i ominąć szew,
     * który ten test ma sądzić. */
    tokio::time::pause();
    tokio::time::advance(Duration::from_mins(1)).await;
    settle_actor().await;
    tokio::time::advance(DEADLINE_SETTLE).await;
    tokio::time::resume();
    let finished_before_cleanup = tokio::time::timeout(WAIT, async {
        while !follow.is_finished() {
            let _barrier = tokio::task::spawn_blocking(|| {}).await;
            tokio::task::yield_now().await;
        }
    })
    .await
    .is_ok();
    let cancelled_before_cleanup = state.cancels.load(Ordering::SeqCst);
    let live_before_cleanup = threads.is_live_at(&terminal.id);
    let second_state = terminal_turn_state_after_scheduling(project.path(), 2).await?;
    let problem = take_visible_problem(&mut source);

    /* Sprzątanie jest po snapshotach. Na wadliwej wersji dopiero ten Stop rozliczyłby `sending`
     * i ukrył dokładnie stan, o który pyta kryterium. */
    let _proofs = threads.close().await;
    let follow_result = follow.await?;

    assert_eq!(
        state.wait_drops.load(Ordering::SeqCst),
        1,
        "the fixture did not put Finished between the two deadline decisions"
    );
    assert!(
        finished_before_cleanup,
        "the deadline path neither completed the follow-up command nor made it retryable"
    );
    assert_eq!(
        cancelled_before_cleanup, 1,
        "Finished from the old reply revoked a deadline after its follow-up send future was already \
         abandoned; the owned process was left running"
    );
    assert!(
        !live_before_cleanup,
        "the actor kept a live session after abandoning the follow-up delivery future"
    );
    assert_ne!(
        second_state.as_deref(),
        Some("sending"),
        "the abandoned follow-up stayed as a nonterminal sending receipt"
    );
    assert!(
        problem
            .as_deref()
            .is_some_and(|text| text.contains("within 1 minute") && text.contains("stopped")),
        "the screen did not explain why the half-delivered follow-up was stopped: {problem:?}"
    );
    assert!(
        follow_result.is_err(),
        "a follow-up whose delivery future was abandoned was reported as delivered"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn eof_during_a_blocked_follow_up_wait_is_visible_and_stops_the_process()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let terminal = Terminal {
        id: "terminal-with-eof-inside-follow-up-wait".to_owned(),
        folder: project.path().to_path_buf(),
    };
    let (drivers, state) = open_reader_driver();
    let threads = Arc::new(Threads::new());
    let (sink, mut source) = line_channel(16);
    threads.terminal_lines_go_to(&terminal, sink);

    threads
        .say_in(
            &drivers,
            &lead_with_limit(0),
            &terminal,
            "Finish the first reply normally",
        )
        .await?;
    let events = state.events();
    events
        .send(completed(SessionRef {
            vendor: "never-finishes",
            id: "first-reply-before-eof".to_owned(),
        }))
        .await?;
    drop(events);

    /* `Done` dowodzi, że stary `Finished` przeszedł przez prawdziwy reader. Następny `wait()` nie
     * ma więc starego deadline'u, który przypadkiem uratowałby obsługę EOF. */
    wait_for_visible_done(&mut source).await?;

    let follow_threads = Arc::clone(&threads);
    let follow_drivers = Arc::clone(&drivers);
    let follow_terminal = terminal.clone();
    let follow = tokio::spawn(async move {
        follow_threads
            .say_in(
                &follow_drivers,
                &lead_with_limit(0),
                &follow_terminal,
                "This send is waiting when the event stream closes",
            )
            .await
    });
    wait_for_blocked_handle(&state).await?;

    let sender = state
        .events
        .lock()
        .expect("the fixture event slot was poisoned")
        .take();
    assert!(
        sender.is_some(),
        "the fixture had no live event stream to close"
    );
    drop(sender);
    for _ in 0..128 {
        if follow.is_finished() {
            break;
        }
        tokio::task::yield_now().await;
    }

    let finished_before_cleanup = follow.is_finished();
    let cancelled_before_cleanup = state.cancels.load(Ordering::SeqCst);
    let live_before_cleanup = threads.is_live_at(&terminal.id);
    let second_state = terminal_turn_state_after_scheduling(project.path(), 2).await?;
    let problem = take_visible_problem(&mut source);

    /* Jawny Stop budzi również wadliwy actor, dlatego obserwacje muszą powstać przed nim. */
    let _proofs = threads.close().await;
    let follow_result = follow.await?;

    assert!(
        finished_before_cleanup,
        "reader EOF was invisible while the actor awaited delivery of a follow-up"
    );
    assert_eq!(
        cancelled_before_cleanup, 1,
        "reader EOF did not pass through GroupProof while follow-up delivery was blocked"
    );
    assert!(
        !live_before_cleanup,
        "the event reader ended, but the conversation stayed described as live"
    );
    assert_ne!(
        second_state.as_deref(),
        Some("sending"),
        "the follow-up stayed nonterminal after its only response stream ended"
    );
    assert!(
        problem
            .as_deref()
            .is_some_and(|text| text.contains("stopped") && text.contains("reply")),
        "the same stream in which the person waited did not name the reader failure: {problem:?}"
    );
    assert!(
        follow_result.is_err(),
        "a follow-up whose event stream ended was reported as delivered"
    );
    Ok(())
}

#[tokio::test]
async fn cancelled_pending_turn_stays_cancelled_when_its_receipt_is_unreadable()
-> Result<(), Box<dyn Error>> {
    let workspace = tempfile::tempdir()?;
    let evidence = EvidenceTarget::lead(
        workspace.path(),
        Uuid::now_v7(),
        SafeInputManifest {
            prompt_bytes: 1,
            context: Vec::new(),
            images: Vec::new(),
        },
    );
    evidence
        .begin_conversation(ConversationMetadata {
            vendor: ConversationVendor::Codex,
            model_configured: true,
        })
        .await?;
    evidence
        .begin_turn(
            1,
            &SafeInputManifest {
                prompt_bytes: 1,
                context: Vec::new(),
                images: Vec::new(),
            },
        )
        .await?;

    /* Aggregate już dowodzi `attempts > turns`; uszkodzenie szczegółu nie może zmienić jawnego
     * Stopu człowieka w bezprzyczynowe `closed` w raporcie wsparcia. */
    let turn_path = evidence.root().join("turns/0001.json");
    let pending: Json = serde_json::from_slice(&std::fs::read(&turn_path)?)?;
    assert_eq!(pending.get("state").and_then(Json::as_str), Some("sending"));
    std::fs::write(&turn_path, b"{")?;
    assert!(
        evidence.finish_conversation(Some(0), true).await.is_err(),
        "an unreadable turn was incorrectly advertised as complete evidence"
    );

    let conversation: Json =
        serde_json::from_slice(&std::fs::read(evidence.root().join("conversation.json"))?)?;
    assert_eq!(
        conversation.get("state").and_then(Json::as_str),
        Some("cancelled")
    );
    assert_eq!(
        conversation.get("failureKind").and_then(Json::as_str),
        Some("cancelled"),
        "the known Stop reason disappeared only because its pending turn was unreadable"
    );
    assert_eq!(
        conversation.get("complete").and_then(Json::as_bool),
        Some(false)
    );
    assert_eq!(
        conversation.get("deathProof").and_then(Json::as_bool),
        Some(true)
    );
    assert!(
        conversation.get("endedAt").and_then(Json::as_i64).is_some(),
        "a proven-dead conversation needs a terminal timestamp"
    );
    Ok(())
}

#[tokio::test]
async fn cancelled_delivered_turn_stays_cancelled_when_its_receipt_is_unreadable()
-> Result<(), Box<dyn Error>> {
    let workspace = tempfile::tempdir()?;
    let evidence = EvidenceTarget::lead(
        workspace.path(),
        Uuid::now_v7(),
        SafeInputManifest {
            prompt_bytes: 1,
            context: Vec::new(),
            images: Vec::new(),
        },
    );
    evidence
        .begin_conversation(ConversationMetadata {
            vendor: ConversationVendor::Codex,
            model_configured: true,
        })
        .await?;
    evidence
        .begin_turn(
            1,
            &SafeInputManifest {
                prompt_bytes: 1,
                context: Vec::new(),
                images: Vec::new(),
            },
        )
        .await?;
    evidence.accept_turn(1).await?;

    let turn_path = evidence.root().join("turns/0001.json");
    let delivered: Json = serde_json::from_slice(&std::fs::read(&turn_path)?)?;
    assert_eq!(
        delivered.get("state").and_then(Json::as_str),
        Some("delivered")
    );
    std::fs::write(&turn_path, b"{")?;
    assert!(
        evidence.finish_conversation(Some(0), true).await.is_err(),
        "an unreadable accepted turn was incorrectly advertised as complete evidence"
    );

    let conversation: Json =
        serde_json::from_slice(&std::fs::read(evidence.root().join("conversation.json"))?)?;
    assert_eq!(
        (
            conversation.get("state").and_then(Json::as_str),
            conversation.get("failureKind").and_then(Json::as_str),
            conversation.get("complete").and_then(Json::as_bool),
            conversation.get("deathProof").and_then(Json::as_bool),
        ),
        (
            Some("cancelled"),
            Some("cancelled"),
            Some(false),
            Some(true),
        ),
        "a known explicit Stop was lost because the accepted turn detail was unreadable: \
         {conversation}"
    );
    Ok(())
}

#[tokio::test]
async fn terminal_finalization_waits_for_an_aborted_reader_publication()
-> Result<(), Box<dyn Error>> {
    let workspace = tempfile::tempdir()?;
    let evidence = EvidenceTarget::lead(
        workspace.path(),
        Uuid::now_v7(),
        SafeInputManifest {
            prompt_bytes: 1,
            context: Vec::new(),
            images: Vec::new(),
        },
    );
    evidence
        .begin_conversation(ConversationMetadata {
            vendor: ConversationVendor::Codex,
            model_configured: true,
        })
        .await?;
    evidence
        .begin_turn(
            1,
            &SafeInputManifest {
                prompt_bytes: 1,
                context: Vec::new(),
                images: Vec::new(),
            },
        )
        .await?;
    evidence.accept_turn(1).await?;

    let (entered_tx, entered_rx) = std_mpsc::channel();
    let (release_tx, release_rx) = std_mpsc::channel();
    let faults = Arc::new(HoldFirstTurnCommit {
        entered: Mutex::new(Some(entered_tx)),
        release: Mutex::new(release_rx),
        held: AtomicBool::new(false),
    });
    let _scope = scoped_faults(evidence.root(), faults)?;
    let finishing_turn = {
        let evidence = evidence.clone();
        tokio::spawn(async move {
            evidence
                .finish_turn(1, TurnCounters::default(), true, false)
                .await
        })
    };
    tokio::task::spawn_blocking(move || entered_rx.recv_timeout(WAIT)).await??;
    finishing_turn.abort();
    let _aborted = finishing_turn.await;

    /* Czytelnik został przerwany dokładnie podczas publikacji. Uszkodzony stary cel sprawia,
     * że finalizer nie ma własnego zapisu tury, za którym przypadkiem zaczekałby na ten sam lock. */
    std::fs::write(evidence.root().join("turns/0001.json"), b"{")?;
    let mut finalizer = {
        let evidence = evidence.clone();
        tokio::spawn(async move { evidence.finish_conversation(Some(0), true).await })
    };
    let finished_early = tokio::time::timeout(Duration::from_millis(100), &mut finalizer).await;
    let overtook_publication = finished_early.is_ok();
    release_tx
        .send(())
        .map_err(|_| "the blocked evidence publisher disappeared before release")?;
    let final_result = match finished_early {
        Ok(joined) => joined?,
        Err(_) => finalizer.await?,
    };
    assert!(
        !overtook_publication,
        "terminal finalization overtook a publication whose reader future had been aborted"
    );
    assert!(
        final_result.is_err(),
        "the deliberately corrupt turn receipt unexpectedly finalized cleanly"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_zero_minute_lead_has_no_reply_deadline() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let terminal = Terminal {
        id: "terminal-with-unlimited-reply".to_owned(),
        folder: project.path().to_path_buf(),
    };
    let (drivers, state) = open_reader_driver();
    let threads = Threads::new();
    let (sink, mut source) = line_channel(16);
    threads.terminal_lines_go_to(&terminal, sink);

    threads
        .say_in(
            &drivers,
            &lead_with_limit(0),
            &terminal,
            "This reply has no time limit",
        )
        .await?;
    tokio::time::advance(Duration::from_hours(24)).await;
    settle_actor().await;

    assert_eq!(
        state.cancels.load(Ordering::SeqCst),
        0,
        "zero minutes means no limit, but the actor treated it as an immediate deadline"
    );
    assert!(
        threads.is_live_at(&terminal.id),
        "an unlimited open reply was silently removed from the live registry"
    );
    assert_no_problem(&mut source);
    let _proofs = threads.close().await;
    Ok(())
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_queued_question_the_pump_never_delivers_is_bounded_by_the_leads_deadline()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let terminal = Terminal {
        id: "terminal-with-undelivered-queued-question".to_owned(),
        folder: project.path().to_path_buf(),
    };
    let bridge_home = tempfile::tempdir()?;
    let threads = Threads::new();
    let (sink, mut source) = line_channel(16);
    let waiting = Arc::new(Waiting::default());
    let desk = Arc::new(
        /* `Desk::answer` odmawia całej powierzchni narzędzi bez znanej biblioteki. Kryterium
         * podaje prawdziwy, izolowany katalog, żeby dojść do produkcyjnej gałęzi pytania. */
        Desk::at(
            Some(bridge_home.path().to_path_buf()),
            project.path().to_path_buf(),
        )
        .showing(Arc::new(Mutex::new(sink.clone())))
        .hearing(Arc::clone(&waiting)),
    );
    let bridge = Bridge::open(bridge_home.path(), Role::Lead, desk).await?;
    let (drivers, state) = queued_question_driver(bridge.at().to_path_buf());
    threads.terminal_lines_go_to(&terminal, sink);

    threads
        .say_in(
            &drivers,
            &lead_with_limit(1),
            &terminal,
            "Ask me before choosing a path",
        )
        .await?;
    for _ in 0..128 {
        if state.calls_written.load(Ordering::SeqCst) == 1 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(
        state.calls_written.load(Ordering::SeqCst),
        1,
        "the configured Lead never reached its real Loadout bridge"
    );
    assert_eq!(state.bridge_failures.load(Ordering::SeqCst), 0);

    for _ in 0..128 {
        if waiting.is_waiting_for_test(LEAD) {
            break;
        }
        /* Gniazdo i host żyją w osobnych zadaniach. Bariera puli daje im uczciwą turę także
         * przy zatrzymanym zegarze, bez konsumowania `LineSource`, który jest treścią testu. */
        tokio::task::spawn_blocking(|| {})
            .await
            .map_err(|error| format!("the bridge scheduling barrier failed: {error}"))?;
        tokio::task::yield_now().await;
    }
    assert!(
        waiting.is_waiting_for_test(LEAD),
        "the bridge wrote the call but Desk never parked its visible question"
    );

    /* `source` celowo nie było dotąd czytane. `Asked` może więc dostać `Sent::Queued`, chociaż
     * odpowiedzialna za ekran pompa nigdy go nie odebrała i człowiek nie ma na co odpowiedzieć. */
    tokio::time::advance(Duration::from_mins(1)).await;
    settle_actor().await;
    tokio::time::advance(DEADLINE_SETTLE).await;
    let seen = visible_through_problem(&mut source).await?;
    assert!(
        seen.iter().any(|(kind, text)| {
            *kind == LineKind::Asked && text == "Which path should I inspect?"
        }),
        "the fixture did not prove that the blocking question was accepted into the UI queue: \
         {seen:?}"
    );
    assert!(
        seen.iter().any(|(kind, text)| {
            *kind == LineKind::Problem
                && text.contains("within 1 minute")
                && text.contains("stopped")
        }),
        "a queued-but-undelivered question escaped the configured reply deadline: {seen:?}"
    );
    assert_eq!(
        state.cancels.load(Ordering::SeqCst),
        1,
        "the timeout sentence appeared without cancelling the process parked on the question"
    );
    assert!(
        !threads.is_live_at(&terminal.id),
        "the question stayed an active conversation after its process was proved dead"
    );
    Ok(())
}

#[tokio::test]
async fn a_question_that_cannot_reach_the_screen_is_refused_instead_of_parking_the_turn()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let waiting = Arc::new(Waiting::default());
    let (sink, _source) = line_channel(1);
    assert_eq!(
        sink.send(Line::Told {
            agent: "Person".to_owned(),
            text: "this fills the screen queue".to_owned(),
        }),
        Sent::Queued,
        "the fixture did not fill its one-line queue"
    );
    let desk = Desk::at(
        Some(home.path().to_path_buf()),
        PathBuf::from(project.path()),
    )
    .showing(Arc::new(Mutex::new(sink)))
    .hearing(Arc::clone(&waiting));

    let answer = tokio::time::timeout(
        WAIT,
        desk.answer(Call {
            id: json!(1),
            call: "ask_the_person".to_owned(),
            input: json!({ "question": "Which file should I change?" }),
        }),
    )
    .await
    .expect("a question dropped by the screen parked the whole Lead turn");

    let Answer::Refused(sentence) = answer else {
        return Err("an invisible question was reported to the agent as answered".into());
    };
    assert!(
        sentence.contains("could not show") && sentence.contains("without waiting"),
        "the refusal has to tell the agent why it must continue without an answer: {sentence}"
    );
    assert!(
        !waiting.answer("Lead", "too late".to_owned()),
        "the invisible question left a live waiting slot after it was refused"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_dropped_older_connection_cannot_withdraw_a_newer_visible_question()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let waiting = Arc::new(Waiting::default());

    let (older_sink, _older_source) = line_channel(1);
    assert_eq!(
        older_sink.send(Line::Told {
            agent: "Person".to_owned(),
            text: "this fills only the older screen queue".to_owned(),
        }),
        Sent::Queued
    );
    let older_lines = Arc::new(Mutex::new(older_sink));
    let (locked, locked_at) = std::sync::mpsc::channel();
    let (release, released_at) = std::sync::mpsc::channel::<()>();
    let held_lines = Arc::clone(&older_lines);
    let holder = std::thread::spawn(move || {
        let _held = held_lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = locked.send(());
        let _ = released_at.recv();
    });
    locked_at.recv_timeout(WAIT)?;

    let (newer_sink, mut newer_source) = line_channel(1);
    let older = Arc::new(
        Desk::at(
            Some(home.path().to_path_buf()),
            project.path().to_path_buf(),
        )
        .showing(older_lines)
        .hearing(Arc::clone(&waiting)),
    );
    let newer = Arc::new(
        Desk::at(
            Some(home.path().to_path_buf()),
            project.path().to_path_buf(),
        )
        .showing(Arc::new(Mutex::new(newer_sink)))
        .hearing(Arc::clone(&waiting)),
    );
    let older_call = tokio::spawn(async move {
        older
            .answer(Call {
                id: json!("older"),
                call: "ask_the_person".to_owned(),
                input: json!({ "question": "The older invisible question" }),
            })
            .await
    });
    tokio::time::timeout(WAIT, async {
        while !waiting.is_waiting_for_test(LEAD) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "the older connection never parked its question")?;

    let newer_call = tokio::spawn(async move {
        newer
            .answer(Call {
                id: json!("newer"),
                call: "ask_the_person".to_owned(),
                input: json!({ "question": "The newer visible question" }),
            })
            .await
    });
    tokio::time::timeout(WAIT, async {
        loop {
            while let Some(line) = newer_source.try_next() {
                if line.kind() == LineKind::Asked && line.text() == "The newer visible question" {
                    return;
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "the newer connection never showed its question")?;

    /* A dopiero teraz kończy `send -> Dropped`. Jego wycofanie nie może dotknąć pytania B,
     * które zostało zaparkowane później i naprawdę dotarło na ekran. */
    drop(release);
    tokio::task::spawn_blocking(move || holder.join())
        .await?
        .map_err(|_| "the fixture line-lock holder panicked")?;
    let older_reply = tokio::time::timeout(WAIT, older_call).await??;
    assert!(
        matches!(older_reply, Answer::Refused(ref text) if text.contains("could not show")),
        "the older full queue did not exercise the production Dropped path: {older_reply:?}"
    );

    assert!(
        waiting.answer(LEAD, "Answer for the visible question".to_owned()),
        "the older Dropped call withdrew a newer visible question from another connection"
    );
    let newer_reply = tokio::time::timeout(WAIT, newer_call).await??;
    assert_eq!(
        newer_reply,
        Answer::Ok(json!("Answer for the visible question"))
    );
    Ok(())
}
