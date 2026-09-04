//! Z-40: „Interrupt" na karcie lidera naprawdę dochodzi do CLI — a kiedy nie ma jak, mówi to
//! wprost, zamiast udawać, że coś pojechało.
//!
//! Droga przerwania istniała w `engine/drivers/claude.rs` od pierwszego dnia i miała dokładnie
//! jednego wołającego: `AgentHandle::cancel`, czyli czasownik KOŃCZĄCY rozmowę. Człowiek
//! patrzący na lidera, który siedzi siódmą minutę w jednym `until grep … sleep 5`, mógł więc
//! tylko czekać albo zamknąć rozmowę razem z całym jej kontekstem.
//!
//! # Pięć dróg, bo tyle ich ma ta prośba
//!
//! Przerwanie w trakcie komendy, w trakcie myślenia, bez żywego wątku, przy CLI które niczego
//! nie ogłosiło, i po zejściu procesu. Droga, której się nie ruszy, jest tą, o którą pyta
//! pierwsze zgłoszenie.
//!
//! # Słaba wersja tego kryterium
//!
//! `assert!(matches!(answer, Interrupted::Sent))`. Przechodzi na implementacji, która oddaje
//! `Sent` i nie wysyła NICZEGO — czyli na przycisku bez skutku (niezmiennik 16), którego z okna
//! nie da się odróżnić od agenta, który przerwania nie usłuchał. Dlatego każda droga liczy, ile
//! linii `ToAgent::Interrupt` naprawdę doszło na wejście dziecka; przy CLI bez ogłoszenia
//! wymagane jest ZERO, bo tam ta linia kosztuje pięć sekund ciszy [T1 §4.1].

use std::error::Error;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::Drivers;
use loadout_lib::commands::chat::{Lead, Terminal, Threads};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, Interrupted, Outcome, Probe, RunSpec,
    SessionRef, ToAgent, TurnBreak, Voice,
};
use loadout_lib::engine::line::{Action, Tool};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::line_channel;
use loadout_lib::library::agents::Agent;
use tokio::sync::mpsc;

/// Zdolność, pod którą i tylko pod którą wolno wysłać `control_request` [T1 §4.6].
const RECEIPT: &str = "interrupt_receipt_v1";

/// Coś, co CLI ogłasza obok — po to, żeby „nie ogłosiło niczego" nie znaczyło „milczało".
const SOMETHING_ELSE: &str = "hooks_v1";

/// Jak nazywa się aplikacja agenta, którą ten dubel udaje.
///
/// „claude", bo o TĘ aplikację chodzi w zdaniu odmowy: dubel stoi za CLI Claude'a, które w jednej
/// wersji ogłasza przerwanie, a w innej nie. Nazwa jedzie razem z odpowiedzią, żeby zdanie na
/// ekranie nie mówiło o Claude nad rozmową z Codeksem.
const AGENT_APP: &str = "claude";

/// Identyfikator wywołania komendy, w której lider stoi.
const CALL: &str = "toolu_wait_01";

/// Komenda, w której lider siedział siedem minut.
const COMMAND: &str = "until grep -q Murmur <(ps aux); do sleep 5; done";

/// Ile czekamy na actora, zanim uznamy, że rozmowa zamilkła.
const WAIT: Duration = Duration::from_secs(1);

/// Czym ten dubel zajmuje turę, zanim człowiek naciśnie „Interrupt".
#[derive(Debug, Clone, Copy)]
enum Doing {
    /// Długa komenda — droga z pierwszego zgłoszenia.
    ACommand,
    /// Samo myślenie: tura idzie, a `Pending` nie ma ani jednego.
    Thinking,
}

/// CLI, które słucha swojego wejścia — i test, który to wejście czyta.
#[derive(Debug)]
struct Listening {
    /// Zdolności ogłoszone w `system/init`. `OnceLock`, dokładnie jak u prawdziwego adaptera.
    announced: Arc<OnceLock<Vec<String>>>,
    /// Głos do dziecka: ten sam kanał, którym jadą tury.
    voice: Voice,
    /// Drugi koniec tego kanału — to, co dziecko naprawdę przeczyta. Test go liczy i potrafi
    /// upuścić, czyli odegrać proces, który zszedł w trakcie rozmowy.
    heard: Mutex<Option<mpsc::Receiver<ToAgent>>>,
    doing: Doing,
}

impl Listening {
    fn new(said_in_init: &str, doing: Doing) -> Arc<Self> {
        let (voice, heard) = mpsc::channel(8);
        let announced: Arc<OnceLock<Vec<String>>> = Arc::new(OnceLock::new());
        // `init` przychodzi zawsze; różni się WYŁĄCZNIE tym, co w nim stoi.
        let _ = announced.set(vec![said_in_init.to_owned()]);
        Arc::new(Self {
            announced,
            voice,
            heard: Mutex::new(Some(heard)),
            doing,
        })
    }

    /// Ile próśb o przerwanie naprawdę doszło na wejście dziecka.
    fn interrupts(&self) -> usize {
        let mut inbox = self.heard.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(inbox) = inbox.as_mut() else {
            return 0;
        };
        let mut asked = 0;
        while let Ok(said) = inbox.try_recv() {
            if matches!(said, ToAgent::Interrupt(_)) {
                asked += 1;
            }
        }
        asked
    }

    /// Dziecko przestaje czytać swoje wejście — tak wygląda proces, który zszedł.
    fn stops_reading(&self) {
        self.heard
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
    }
}

#[async_trait]
impl AgentDriver for Listening {
    fn id(&self) -> &'static str {
        "listening"
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
        match self.doing {
            Doing::ACommand => {
                events
                    .send(DecodedEvent {
                        event: AgentEvent::ToolStart {
                            id: CALL.to_owned(),
                            label: "Wait for Murmur binary to launch".to_owned(),
                        },
                        tool: Some(Tool::Started {
                            action: Action::Ran,
                            target: COMMAND.to_owned(),
                        }),
                    })
                    .await?;
            }
            Doing::Thinking => {
                events.send(AgentEvent::Thinking.into()).await?;
            }
        }
        Ok(Box::new(ListeningHandle {
            announced: Arc::clone(&self.announced),
            voice: Some(self.voice.clone()),
            session: SessionRef {
                vendor: "listening",
                id: spec.run_id.to_string(),
            },
            events: Some(events),
        }))
    }
}

#[derive(Debug)]
struct ListeningHandle {
    announced: Arc<OnceLock<Vec<String>>>,
    voice: Option<Voice>,
    session: SessionRef,
    events: Option<mpsc::Sender<DecodedEvent>>,
}

#[async_trait]
impl AgentHandle for ListeningHandle {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        None
    }

    fn voice(&self) -> Option<Voice> {
        self.voice.clone()
    }

    /// Klamka budowana dokładnie tak, jak buduje ją adapter Claude'a: głos, lista z `init`,
    /// nazwa zdolności i nazwa aplikacji — wszystkie cztery z tej samej strony granicy.
    fn turn_break(&self) -> Option<TurnBreak> {
        Some(TurnBreak::new(
            self.voice.clone()?,
            Arc::clone(&self.announced),
            RECEIPT,
            AGENT_APP,
        ))
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        std::future::pending().await
    }

    async fn cancel(&mut self) -> GroupProof {
        self.voice.take();
        self.events.take();
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

fn drivers_of(state: &Arc<Listening>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::<Listening>::clone(state);
    Arc::new(move |_vendor| Arc::clone(&driver))
}

fn lead() -> Lead {
    Lead {
        agent: Agent::example(),
    }
}

fn terminal_in(project: &tempfile::TempDir, id: &str) -> Terminal {
    Terminal {
        id: id.to_owned(),
        folder: project.path().to_path_buf(),
    }
}

/// Daje actorowi dojść do końca tury pętli, w której zapamiętuje klamkę tej sesji.
async fn settle_actor() {
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
}

/// Rozmowa, w której lider właśnie coś robi. Oddaje rejestr, dubla i terminal.
async fn a_lead_that_is(
    project: &tempfile::TempDir,
    id: &str,
    said_in_init: &str,
    doing: Doing,
) -> Result<(Threads, Arc<Listening>, Terminal), Box<dyn Error>> {
    let state = Listening::new(said_in_init, doing);
    let drivers = drivers_of(&state);
    let threads = Threads::new();
    let terminal = terminal_in(project, id);
    let (sink, _source) = line_channel(64);
    threads.terminal_lines_go_to(&terminal, sink);
    tokio::time::timeout(
        WAIT,
        threads.say_in(&drivers, &lead(), &terminal, "Wait for Murmur to come up"),
    )
    .await
    .map_err(|_| "the fixture never accepted the first message")??;
    settle_actor().await;
    Ok((threads, state, terminal))
}

#[tokio::test]
async fn interrupting_a_long_command_reaches_the_agents_own_input() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let (threads, state, terminal) = a_lead_that_is(
        &project,
        "terminal-stuck-in-a-command",
        RECEIPT,
        Doing::ACommand,
    )
    .await?;

    let answer = threads.interrupt_at(&terminal.id).await;

    assert_eq!(
        answer,
        Interrupted::Sent,
        "the lead had been sitting in one command and the person asked it to stop. Anything but \
         `Sent` here leaves them with a control that answers about a refusal nobody made."
    );
    assert_eq!(
        state.interrupts(),
        1,
        "and the request has to reach the agent's own input, exactly once. A verb that returns \
         `Sent` without writing the line is a button with no effect (invariant 16) — from the \
         screen it is indistinguishable from an agent that ignored the request. Twice is worse: \
         two asks look like two stops in the CLI log."
    );

    let _proofs = threads.close().await;
    Ok(())
}

#[tokio::test]
async fn interrupting_while_the_lead_is_only_thinking_still_reaches_it()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let (threads, state, terminal) = a_lead_that_is(
        &project,
        "terminal-that-is-only-thinking",
        RECEIPT,
        Doing::Thinking,
    )
    .await?;

    let answer = threads.interrupt_at(&terminal.id).await;

    assert_eq!(
        answer,
        Interrupted::Sent,
        "a turn that has not announced a single command is still a turn, and a two-minute think \
         is exactly the wait a person wants out of. A path that only works over a command in \
         flight leaves them with a dead control on the longest silences there are."
    );
    assert_eq!(state.interrupts(), 1);

    let _proofs = threads.close().await;
    Ok(())
}

#[tokio::test]
async fn a_cli_that_never_announced_the_receipt_says_so_and_sends_nothing()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let (threads, state, terminal) = a_lead_that_is(
        &project,
        "terminal-on-a-cli-without-the-receipt",
        SOMETHING_ELSE,
        Doing::ACommand,
    )
    .await?;

    let answer = threads.interrupt_at(&terminal.id).await;

    assert_eq!(
        answer,
        Interrupted::NotAnnounced {
            agent_app: AGENT_APP
        },
        "this CLI never said it understands an in-band stop, so the honest answer names the app \
         and admits it. Answering `Sent` here would put the person in front of a screen where \
         nothing happens and nothing explains why — and the app has to travel with the answer, \
         or the sentence on screen names Claude over a conversation with Codex."
    );
    assert_eq!(
        state.interrupts(),
        0,
        "and NOTHING may go down the pipe. The same line sent where the CLI has never heard of \
         `control_request` buys five seconds of waiting for a reply that never comes — which is \
         the silence this whole task exists to end."
    );

    let _proofs = threads.close().await;
    Ok(())
}

#[tokio::test]
async fn interrupting_a_terminal_with_no_conversation_is_an_answer_not_a_crash()
-> Result<(), Box<dyn Error>> {
    let threads = Threads::new();

    let answer = threads.interrupt_at("terminal-nobody-ever-spoke-in").await;

    assert_eq!(
        answer,
        Interrupted::NoLongerListening,
        "a terminal where no conversation ever started has nothing to interrupt, and saying so \
         is an answer rather than a refusal. The control is not on screen in this state; the \
         path is here because the window can ask a moment after the last row went out."
    );
    Ok(())
}

#[tokio::test]
async fn interrupting_after_the_agent_stopped_reading_says_there_is_nothing_left()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let (threads, state, terminal) = a_lead_that_is(
        &project,
        "terminal-whose-agent-went-away",
        RECEIPT,
        Doing::ACommand,
    )
    .await?;

    /* Proces zszedł: jego koniec potoku zniknął, choć rejestr rozmów jeszcze o tym nie wie. To
     * jest okno, w którym człowiek naciska przycisk narysowany chwilę wcześniej. */
    state.stops_reading();

    let answer = threads.interrupt_at(&terminal.id).await;

    assert_eq!(
        answer,
        Interrupted::NoLongerListening,
        "the agent is gone, so the request has nowhere to land. `Sent` here would be the worst \
         of the three answers: the person waits for a stop that was never delivered, over a \
         conversation that is already over."
    );
    assert_eq!(
        state.interrupts(),
        0,
        "and nothing was counted as delivered on the way out"
    );

    let _proofs = threads.close().await;
    Ok(())
}
