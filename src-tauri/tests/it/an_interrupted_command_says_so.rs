//! Z-40: tura przerwana przez człowieka zostawia w strumieniu zdanie o TEJ komendzie, którą
//! zatrzymała — nie ciszę i nie wiersz mówiący, że coś „nie zadziałało".
//!
//! Zmierzone 2026-09-04 na liderze: siedem minut w jednym wywołaniu Basha
//! (`until grep … ; do sleep 5; done`). Po Z-36 człowiek WIDZI ten wiersz, tykający co trzydzieści
//! sekund, i to jest cała naprawa tamtego zadania. Czego nadal nie było: drogi, żeby to
//! zatrzymać — a kiedy tura kończy się `Cancelled`, `Curator` wołał wyłącznie `close_then(done)`,
//! więc `pending` zostawało otwarte i wiersz komendy mówił `Running: … · 7m` do końca rozmowy.
//! Człowiek naciskał przycisk, tura stawała, a ekran wyglądał identycznie jak przed naciśnięciem.
//!
//! # Dlaczego ten plik kompiluje się na starym drzewie
//!
//! Bo nie dotyka ANI JEDNEJ nowej sygnatury (AGENTS.md §2a p. 4): dubel wpuszcza `Finished`
//! z `FinishReason::Cancelled`, a wiersze czytamy z `line_channel()`, czyli tą samą drogą, którą
//! karmi się okno. Test, który się nie skompilował, niczego nie uruchomił i niczego nie dowiódł.
//!
//! # Słaba wersja tego kryterium
//!
//! `assert!(said.iter().any(|text| text.contains("Interrupted")))`. Przechodzi na implementacji,
//! która dokłada zdanie o przerwaniu OBOK wiersza `Running: …` — czyli zostawia na ekranie dwa
//! wiersze o jednej komendzie, z których jeden dalej mówi, że ona idzie. Dlatego kryterium pyta
//! o OSTATNIE słowo o tej komendzie i porównuje je co do znaku: jeden nośnik, jeden wiersz
//! (2026-09, Z-36).

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::Drivers;
use loadout_lib::commands::chat::{Lead, Terminal, Threads};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::line::{Action, Line, LineKind, Tool};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{LineSource, line_channel};
use loadout_lib::library::agents::Agent;
use tokio::sync::mpsc;

/// Ile czekamy na wiersze, zanim uznamy, że rozmowa zamilkła.
const WAIT: Duration = Duration::from_secs(1);

/// Identyfikator wywołania — jeden na całą komendę, po nim wynik trafia do swojego wiersza.
const CALL: &str = "toolu_wait_01";

/// Komenda, w której lider siedział siedem minut. To ona jest podmiotem wiersza.
const COMMAND: &str = "until grep -q Murmur <(ps aux); do sleep 5; done";

/// Opis, który model napisał sobie sam. Jedzie w fiksturze, bo jedzie na prawdziwym drucie.
const DESCRIPTION: &str = "Wait for Murmur binary to launch";

/// Ile ta komenda trwała, wedle vendora, w chwili przerwania.
const SECONDS: u64 = 420;

/// Dubel, który wpuszcza komendę w toku i kończy turę tak, jak kończy ją przerwanie.
#[derive(Debug)]
struct StoppedMidCommand;

#[async_trait]
impl AgentDriver for StoppedMidCommand {
    fn id(&self) -> &'static str {
        "stopped-mid-command"
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
        let session = SessionRef {
            vendor: "stopped-mid-command",
            id: spec.run_id.to_string(),
        };
        /* Zapowiedź, bicie serca i koniec tury — dokładnie ta trójka, którą widać na żywym
         * drucie po naciśnięciu przerwania. Kolejka zdarzeń ma 128 miejsc, więc wszystkie trzy
         * czekają w niej do chwili, w której actor uruchomi czytnik. */
        events
            .send(DecodedEvent {
                event: AgentEvent::ToolStart {
                    id: CALL.to_owned(),
                    label: DESCRIPTION.to_owned(),
                },
                tool: Some(Tool::Started {
                    action: Action::Ran,
                    target: COMMAND.to_owned(),
                }),
            })
            .await?;
        events
            .send(
                AgentEvent::ToolProgress {
                    id: Some(CALL.to_owned()),
                    elapsed_seconds: Some(SECONDS),
                }
                .into(),
            )
            .await?;
        events
            .send(
                AgentEvent::Finished(Outcome {
                    ok: false,
                    reason: FinishReason::Cancelled,
                    text: String::new(),
                    cost_usd: None,
                    tokens: Tokens::default(),
                    turns: 1,
                    took: Duration::from_secs(430),
                    session: session.clone(),
                })
                .into(),
            )
            .await?;
        Ok(Box::new(StillOpenHandle {
            session,
            events: Some(events),
        }))
    }
}

/// Uchwyt, który TRZYMA nadajnik zdarzeń po zakończonej turze.
///
/// Trzyma go z rozmysłem: porzucony dałby czytnikowi EOF, a wtedy komendę domknąłby
/// `Curator::flush` zdaniem `Ran … — didn't work`. Kryterium ma sądzić drogę PRZERWANIA, a nie
/// drogę urwanego strumienia — te dwie mówią o czym innym i mają różne zdania.
#[derive(Debug)]
struct StillOpenHandle {
    session: SessionRef,
    events: Option<mpsc::Sender<DecodedEvent>>,
}

#[async_trait]
impl AgentHandle for StillOpenHandle {
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
        self.events.take();
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

fn one_driver() -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(StoppedMidCommand);
    Arc::new(move |_vendor| Arc::clone(&driver))
}

fn lead() -> Lead {
    Lead {
        agent: Agent::example(),
    }
}

/// Wszystko, co rozmowa powiedziała, aż do wiersza kończącego turę.
async fn said_until_the_turn_ends(source: &mut LineSource) -> Result<Vec<Line>, Box<dyn Error>> {
    let mut said = Vec::new();
    tokio::time::timeout(WAIT, async {
        loop {
            while let Some(line) = source.try_next() {
                let ends = line.kind() == LineKind::Done;
                said.push(line);
                if ends {
                    return;
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "the interrupted turn never reached a closing row on screen")?;
    Ok(said)
}

/// Zdania wierszy o komendach, w kolejności, w jakiej doszły do okna.
fn about_commands(said: &[Line]) -> Vec<&str> {
    said.iter()
        .filter(|line| line.kind() == LineKind::Ran)
        .map(Line::text)
        .collect()
}

#[tokio::test]
async fn an_interrupted_turn_says_which_command_it_stopped() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let terminal = Terminal {
        id: "terminal-with-an-interrupted-command".to_owned(),
        folder: project.path().to_path_buf(),
    };
    let drivers = one_driver();
    let threads = Threads::new();
    let (sink, mut source) = line_channel(64);
    threads.terminal_lines_go_to(&terminal, sink);

    threads
        .say_in(&drivers, &lead(), &terminal, "Wait for Murmur to come up")
        .await?;

    let said = said_until_the_turn_ends(&mut source).await?;
    let stopped = format!("Interrupted — {COMMAND} stopped after 7m");
    let rows = about_commands(&said);

    assert_eq!(
        rows.last().copied(),
        Some(stopped.as_str()),
        "the turn was stopped in the middle of a command that had been running for seven \
         minutes, and the last word about that command still has to name it and say how long it \
         got. Without this row the screen after the stop reads exactly like the screen before \
         it — `Running: …` ticking over a command nobody is running — which is the whole reason \
         the control exists. All of the rows were {:?}",
        said.iter().map(Line::text).collect::<Vec<_>>(),
    );
    assert!(
        !rows
            .iter()
            .rev()
            .skip(1)
            .any(|text| text.starts_with(&format!("Interrupted — {COMMAND}"))),
        "ONE ROW, NOT TWO: the row that opened when the command started and the row that stops \
         it are the same carrier, so the stopping sentence REPLACES the one about work in \
         flight. The rows were {rows:?}"
    );

    let _proofs = threads.close().await;
    Ok(())
}
