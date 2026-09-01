//! Most i kanał odpowiedzi Leada są jednym zasobem terminalu.
//!
//! Dwa pierwsze zdania mogą równolegle dojść do asynchronicznego otwarcia gniazda. Jeżeli mapa
//! mostów rozstrzygnie ten wyścig osobno od mapy oczekujących odpowiedzi, zwycięski `Desk` parkuje
//! pytanie w jednym `Waiting`, a `Threads::answer_in` szuka go w drugim. Pytanie jest wtedy
//! widoczne, klik działa w UI, lecz odpowiedź nie dociera do agenta — dokładnie jak Lead, który
//! przestał odpisywać.

#![allow(clippy::expect_used)]

use std::error::Error;
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc as std_mpsc};
use std::task::{Context, Poll, Waker};

use async_trait::async_trait;
use loadout_lib::bridge::Call;
use loadout_lib::commands::Drivers;
use loadout_lib::commands::chat::{LEAD, Lead, Terminal, Threads};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentHandle, DecodedEvent, DriverConfiguration, Outcome, Probe, RunSpec,
    SessionRef, ToAgent, Voice,
};
use loadout_lib::engine::line::{Line, LineKind};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::line_channel;
use loadout_lib::library::agents::Agent;
use serde_json::Value as Json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

#[derive(Debug, Default)]
struct QuestionState {
    calls: AtomicUsize,
    replies: AtomicUsize,
    failures: AtomicUsize,
}

#[derive(Clone, Debug)]
struct QuestionDriver {
    state: Arc<QuestionState>,
    socket: Option<PathBuf>,
}

impl QuestionDriver {
    fn new(state: Arc<QuestionState>) -> Self {
        Self {
            state,
            socket: None,
        }
    }

    /// Ścieżka gniazda jedzie prawdziwą konfiguracją Claude'a, nie testowym skrótem obok niej.
    fn socket_from(configuration: &DriverConfiguration) -> Option<PathBuf> {
        let config = configuration
            .arguments
            .windows(2)
            .find(|pair| pair.first().is_some_and(|arg| arg == "--mcp-config"))?
            .get(1)?;
        let document: Json = serde_json::from_slice(&std::fs::read(config).ok()?).ok()?;
        document
            .pointer("/mcpServers/loadout/args")?
            .as_array()?
            .last()?
            .as_str()
            .map(PathBuf::from)
    }
}

#[async_trait]
impl AgentDriver for QuestionDriver {
    fn id(&self) -> &'static str {
        "claude"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("fixture".to_owned()),
        })
    }

    fn configured(&self, configuration: &DriverConfiguration) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            state: Arc::clone(&self.state),
            socket: Self::socket_from(configuration),
        }))
    }

    fn with_evidence(&self, _target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let socket = self.socket.clone().ok_or_else(|| {
            anyhow::anyhow!("the real Loadout bridge socket did not reach the driver")
        })?;
        let state = Arc::clone(&self.state);
        let call = tokio::spawn(async move {
            if ask_through(socket, Arc::clone(&state)).await.is_err() {
                state.failures.fetch_add(1, Ordering::SeqCst);
            }
        });
        let (voice, heard) = mpsc::channel(4);
        Ok(Box::new(QuestionHandle {
            call,
            events: Some(events),
            heard,
            session: SessionRef {
                vendor: "claude",
                id: spec.run_id.to_string(),
            },
            voice,
        }))
    }
}

async fn ask_through(socket: PathBuf, state: Arc<QuestionState>) -> anyhow::Result<()> {
    let stream = UnixStream::connect(socket).await?;
    let (reading, mut writing) = stream.into_split();
    let mut reading = BufReader::new(reading);
    let mut greeting = String::new();
    reading.read_line(&mut greeting).await?;

    let mut bytes = serde_json::to_vec(&Call {
        id: serde_json::json!("paired-question"),
        call: "ask_the_person".to_owned(),
        input: serde_json::json!({ "question": "Which path should I inspect?" }),
    })?;
    bytes.push(b'\n');
    writing.write_all(&bytes).await?;
    writing.flush().await?;
    state.calls.fetch_add(1, Ordering::SeqCst);

    let mut reply = String::new();
    reading.read_line(&mut reply).await?;
    if reply.is_empty() {
        anyhow::bail!("the visible question never received its bridge reply");
    }
    state.replies.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

#[derive(Debug)]
struct QuestionHandle {
    call: tokio::task::JoinHandle<()>,
    events: Option<mpsc::Sender<DecodedEvent>>,
    /// Odbiornik musi żyć: drugie równoległe zdanie jest prawdziwym follow-upem przez `Voice`.
    heard: mpsc::Receiver<ToAgent>,
    session: SessionRef,
    voice: Voice,
}

#[async_trait]
impl AgentHandle for QuestionHandle {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn voice(&self) -> Option<Voice> {
        Some(self.voice.clone())
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
        self.call.abort();
        self.events.take();
        self.heard.close();
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

fn visible_question(source: &mut loadout_lib::ipc::LineSource) -> Option<Line> {
    for _ in 0..256 {
        if let Some(line) = source.try_next()
            && line.kind() == LineKind::Asked
        {
            return Some(line);
        }
        std::thread::yield_now();
    }
    None
}

#[test]
fn simultaneous_first_messages_keep_the_bridge_and_its_answers_paired() -> Result<(), Box<dyn Error>>
{
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()?;
    runtime.block_on(async {
        let home = tempfile::tempdir()?;
        let project = tempfile::tempdir()?;
        let terminal = Terminal {
            id: "two-first-messages".to_owned(),
            folder: project.path().to_path_buf(),
        };
        let threads = Threads::new();
        threads.library_is(home.path().to_path_buf());
        let (sink, mut source) = line_channel(32);
        threads.terminal_lines_go_to(&terminal, sink);

        let state = Arc::new(QuestionState::default());
        let driver: Arc<dyn AgentDriver> = Arc::new(QuestionDriver::new(Arc::clone(&state)));
        let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));
        let lead = Lead {
            agent: Agent::example(),
        };

        /* Jedyny blocking worker stoi, więc każde `Bridge::open/create_dir_all` zatrzymuje się
         * po własnym `waiting.insert`, zanim którykolwiek most może wygrać mapę. Ręczny pierwszy
         * poll ustala kolejność bez sleepów i bez probabilistycznego wyścigu. */
        let (worker_started, occupied) = std_mpsc::sync_channel(0);
        let (release_worker, released) = std_mpsc::sync_channel(0);
        let blocker = tokio::task::spawn_blocking(move || {
            let _ = worker_started.send(());
            let _ = released.recv();
        });
        occupied.recv()?;

        let mut first = Box::pin(threads.say_in(&drivers, &lead, &terminal, "First"));
        let mut second = Box::pin(threads.say_in(&drivers, &lead, &terminal, "Second"));
        {
            let waker = Waker::noop();
            let mut context = Context::from_waker(waker);
            assert!(
                matches!(first.as_mut().poll(&mut context), Poll::Pending),
                "the first message did not stop at the deliberately occupied bridge open"
            );
            assert!(
                matches!(second.as_mut().poll(&mut context), Poll::Pending),
                "the second message did not enter the same bridge race"
            );
        }

        let _ = release_worker.send(());
        blocker.await?;
        first.await?;
        second.await?;

        let mut asked = None;
        for _ in 0..256 {
            if let Some(line) = visible_question(&mut source) {
                asked = Some(line);
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(
            matches!(asked, Some(Line::Asked { .. })),
            "the configured driver never asked through the winning production bridge"
        );
        assert_eq!(state.calls.load(Ordering::SeqCst), 1);
        assert_eq!(state.failures.load(Ordering::SeqCst), 0);
        assert!(
            threads.answer_in(&terminal.id, LEAD, "Inspect src".to_owned()),
            "the question is visible through the winning bridge, but answer_in points at the \
             losing Waiting; the Lead would stay blocked until its deadline"
        );

        for _ in 0..128 {
            if state.replies.load(Ordering::SeqCst) == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(
            state.replies.load(Ordering::SeqCst),
            1,
            "answer_in accepted the click but the exact bridge call did not receive it"
        );
        let _proofs = threads.close().await;
        Ok::<(), Box<dyn Error>>(())
    })
}
