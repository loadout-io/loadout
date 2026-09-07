//! L-01: kolejka tur należy do Loadouta, a zamknięcie przyjmowania jest atomowe.
//!
//! Incydent I-01 (bieg z 2026-09-06): Architect przyjął wiadomość Leada, po pierwszym `result`
//! zaczął kolejną turę — a Loadout zamknął wejście i zatrzymał proces. Wiadomość człowieka
//! zginęła razem z turą, która miała ją obsłużyć.
//!
//! Dubler zastępuje wyłącznie vendora. Rejestr sesji, kolejka, sumowanie zużycia i domknięcie
//! sesji pochodzą z produkcyjnego `run_workflow_inner`.

#![allow(clippy::expect_used, clippy::too_many_lines, clippy::similar_names)]
#![allow(clippy::assigning_clones, clippy::duration_suboptimal_units)]
#![allow(clippy::struct_field_names, clippy::implicit_clone)]

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::step_message::{MESSAGE_LIMIT_BYTES, QUEUE_LIMIT, StepMessageResult};
use loadout_lib::commands::{Drivers, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, ToAgent, Tokens, Voice,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{AppState, line_channel};
use loadout_lib::library::agents::{Agent, write_agent_file};
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tokio::sync::{Notify, mpsc};

/// I-01 wprost: dwie wiadomości przyjęte w trakcie pierwszej tury dostają własne tury,
/// w kolejności przyjęcia, a wynik kroku pochodzi z ostatniej obsłużonej tury.
#[tokio::test]
async fn queued_messages_get_their_own_turns_before_the_session_closes()
-> Result<(), Box<dyn Error>> {
    let world = World::new()?;
    let (run, _) = world
        .drive(async |address: &str| {
            let first = world.say(address, "also update the README");
            assert_eq!(
                first.result,
                StepMessageResult::AcceptedBySession,
                "Loadout refused a message to a step whose session is still working"
            );
            let second = world.say(address, "and mention the limit");
            assert_eq!(
                second.result,
                StepMessageResult::AcceptedBySession,
                "the second clarification was not accepted into the queue"
            );
            world.session.release.notify_one();
            Ok(())
        })
        .await?;

    assert_eq!(
        world.delivered(),
        vec![
            "also update the README".to_owned(),
            "and mention the limit".to_owned()
        ],
        "the accepted messages never reached the agent's session as their own turns"
    );
    let step = row(&run, "Builder")?;
    assert_eq!(
        step.get("summary").and_then(Value::as_str),
        Some("turn 3 handled: and mention the limit"),
        "the step's result came from an earlier turn than the last one Loadout handled"
    );
    // Zużycie trzech tur zsumowane dokładnie raz. Wartości dobrane tak, żeby suma była dokładna
    // w dwójkowym zapisie — 0.25 + 0.5 + 0.125 nie ma reszty.
    assert_eq!(
        step.get("cost_usd").and_then(Value::as_f64),
        Some(0.875),
        "the cost of the extra turns was dropped or counted twice"
    );
    assert_eq!(
        step.get("uncached_input").and_then(Value::as_u64),
        Some(60),
        "input tokens were not summed across the handled turns"
    );
    assert_eq!(
        step.get("output").and_then(Value::as_u64),
        Some(6),
        "output tokens were not summed across the handled turns"
    );
    assert_eq!(
        step.get("vendor_turns").and_then(Value::as_u64),
        Some(3),
        "the vendor's own turn count was not summed across the handled turns"
    );
    Ok(())
}

/// Sprawdzenie pustej kolejki i zamknięcie przyjmowania są jedną operacją: po zakończeniu
/// kroku żadna wiadomość nie może już wejść, a tym bardziej dojechać do vendora.
#[tokio::test]
async fn a_finished_step_stops_accepting_and_delivers_nothing() -> Result<(), Box<dyn Error>> {
    let world = World::with_a_step_after(true)?;
    world
        .drive(async |address: &str| {
            world.session.release.notify_one();
            // Bieg wciąż idzie: pracuje krok po nim, a sesja Buildera właśnie się zamknęła.
            world.session.next_entered.notified().await;
            let late = world.say(address, "too late");
            assert_eq!(
                late.result,
                StepMessageResult::RecipientFinished,
                "a step whose session closed still accepted work"
            );
            assert!(
                late.said.contains("finished"),
                "the refusal does not say the addressed session has finished: {}",
                late.said
            );
            world.session.next_release.notify_one();
            Ok(())
        })
        .await?;
    assert!(
        !world.delivered().iter().any(|one| one == "too late"),
        "a message accepted after the close reached the agent anyway"
    );
    Ok(())
}

/// Wiadomość pod cudzym adresem nie ma prawa zostać obsłużona przez żywą sesję tego biegu.
#[tokio::test]
async fn a_misaddressed_message_never_reaches_a_live_session() -> Result<(), Box<dyn Error>> {
    let world = World::new()?;
    world
        .drive(async |address: &str| {
            let wrong = world.say_to(address, "builder-2", "wrong address");
            assert_eq!(wrong.result, StepMessageResult::NoSuchStep);
            let stale = world.say_to("an-older-run", "builder", "older run");
            assert_eq!(stale.result, StepMessageResult::StaleRun);
            world.session.release.notify_one();
            Ok(())
        })
        .await?;
    assert!(
        world.delivered().is_empty(),
        "a misaddressed message was delivered to a live session"
    );
    Ok(())
}

/// Pełna kolejka i zbyt długa wiadomość są jawnymi odmowami, nie nieograniczonym czekaniem.
#[tokio::test]
async fn a_full_queue_and_an_oversized_message_are_refused() -> Result<(), Box<dyn Error>> {
    let world = World::new()?;
    let replies = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&replies);
    world
        .drive(async |address: &str| {
            let huge = "x".repeat(MESSAGE_LIMIT_BYTES + 1);
            let refused = world.say(address, &huge);
            assert_eq!(
                refused.result,
                StepMessageResult::TooLong,
                "an oversized message was taken into the queue"
            );
            for index in 0..(QUEUE_LIMIT + 2) {
                let reply = world.say(address, &format!("note {index}"));
                seen.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(reply.result);
            }
            world.session.release.notify_one();
            Ok(())
        })
        .await?;
    let results = replies
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    assert_eq!(
        results
            .iter()
            .filter(|one| **one == StepMessageResult::AcceptedBySession)
            .count(),
        QUEUE_LIMIT,
        "the queue accepted a different number of messages than its stated limit"
    );
    assert!(
        results
            .iter()
            .skip(QUEUE_LIMIT)
            .all(|one| *one == StepMessageResult::QueueFull),
        "a full queue answered with something other than an explicit refusal"
    );
    let delivered = world.delivered();
    assert_eq!(
        delivered.len(),
        QUEUE_LIMIT,
        "the number of delivered turns does not match what Loadout said it accepted"
    );
    assert!(
        !delivered.iter().any(|one| one.len() > MESSAGE_LIMIT_BYTES),
        "a refused oversized message reached the agent anyway"
    );
    Ok(())
}

/// L-01: sufit czasu należy do KROKU, nie do tury. Wiadomość nie kupuje agentowi nowego zegara,
/// a praca zostawiona w kolejce po limicie jest widoczna, nie milcząca.
///
/// Zatrzymany zegar: pierwsza tura pracuje 40 s, druga chce 40 s, a krok ma minutę. Kiedy limit
/// jest liczony od początku kroku, druga tura pada na 60. sekundzie. Gdyby zegar startował od
/// nowa przy każdej wiadomości, obie tury zmieściłyby się w limicie i krok skończyłby się dobrze.
#[tokio::test(start_paused = true)]
async fn a_message_does_not_buy_the_step_a_fresh_clock() -> Result<(), Box<dyn Error>> {
    let world = World::working(false, 1, Duration::from_secs(600), Duration::from_secs(40))?;
    let (run, _) = world
        .drive(async |address: &str| {
            assert_eq!(
                world.say(address, "first note").result,
                StepMessageResult::AcceptedBySession
            );
            assert_eq!(
                world.say(address, "second note").result,
                StepMessageResult::AcceptedBySession
            );
            world.session.release.notify_one();
            Ok(())
        })
        .await?;
    let step = row(&run, "Builder")?;
    let said = step
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        said.contains("ran longer than its 1 minute limit"),
        "the step outlived its own limit because a message restarted the clock: {said:?}"
    );
    assert_eq!(
        world.delivered(),
        vec!["first note".to_owned()],
        "the limit stopped the step, so only the message it actually got may be delivered"
    );
    // Praca przyjęta i nieoddana jest faktem na ekranie, nie ciszą.
    let left = world
        .said
        .borrow()
        .iter()
        .filter_map(|line| match line {
            Line::Problem { text, .. } => Some(text.clone()),
            _ => None,
        })
        .find(|text| text.contains("could read"));
    assert_eq!(
        left.as_deref(),
        Some("Builder stopped before it could read 1 message you sent to it."),
        "the run never said that an accepted message was never read"
    );
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Świat testu: prawdziwy `AppState`, prawdziwy bieg, dubler wyłącznie w miejscu vendora.
// ---------------------------------------------------------------------------------------------

struct World {
    _root: tempfile::TempDir,
    project: PathBuf,
    workflow: PathBuf,
    state: AppState,
    session: Arc<Session>,
    /// Sufit cierpliwości samego testu. Pod zatrzymanym zegarem musi być dalej niż limit kroku,
    /// bo tokio przewija czas do NAJBLIŻSZEGO terminu — a tym ma być limit, nie ten sufit.
    patience: Duration,
    /// Wiersze, które bieg pokazał człowiekowi.
    said: std::cell::RefCell<Vec<Line>>,
}

struct Session {
    entered: Notify,
    release: Notify,
    /// Drugi krok grafu — istnieje po to, żeby bieg jeszcze żył, kiedy sesja pierwszego
    /// kroku jest już zamknięta. Bez tego jedyną odpowiedzią jest „ten bieg już nie idzie",
    /// czyli inny fakt niż „ten krok już nie przyjmuje pracy".
    next_entered: Notify,
    next_release: Notify,
    started: std::sync::atomic::AtomicUsize,
    delivered: Mutex<Vec<String>>,
}

impl World {
    fn new() -> Result<Self, Box<dyn Error>> {
        Self::of(false, 0, Duration::from_secs(30))
    }

    fn with_a_step_after(second: bool) -> Result<Self, Box<dyn Error>> {
        Self::of(second, 0, Duration::from_secs(30))
    }

    fn of(second: bool, minutes: u32, patience: Duration) -> Result<Self, Box<dyn Error>> {
        Self::working(second, minutes, patience, Duration::ZERO)
    }

    fn working(
        second: bool,
        minutes: u32,
        patience: Duration,
        works: Duration,
    ) -> Result<Self, Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let project = root.path().canonicalize()?.join("project");
        let home = root.path().canonicalize()?.join("home");
        fs::create_dir_all(project.join(".loadout"))?;
        fs::create_dir_all(home.join("workflows"))?;
        let mut agent = Agent::example();
        if minutes > 0 {
            agent.give_up_after_minutes = minutes;
        }
        write_agent_file(&home.join("agents"), &agent, None)?;
        let workflow = home.join("workflows/message.json");
        let mut steps = vec![json!({"kind":"agent","id":"builder","name":"Builder",
            "agent":agent.id,"instructions":"Return a brief result.","overrides":{},
            "folder":{"use":"project"},"at":{"x":0,"y":0}})];
        let mut links = Vec::new();
        if second {
            steps.push(
                json!({"kind":"agent","id":"packer","name":"Packer","agent":agent.id,
                "instructions":"Return a brief result.","overrides":{},
                "folder":{"use":"project"},"at":{"x":200,"y":0}}),
            );
            links.push(json!({"from":"builder","to":"packer"}));
        }
        fs::write(
            &workflow,
            json!({"format":1,"id":"wf-turns","name":"Turns",
            "executionInputs":{"schema":1,"effects":{"externalMessages":true}},
            "steps":steps,"links":links})
            .to_string(),
        )?;
        let session = Arc::new(Session {
            entered: Notify::new(),
            release: Notify::new(),
            next_entered: Notify::new(),
            next_release: Notify::new(),
            started: std::sync::atomic::AtomicUsize::new(0),
            delivered: Mutex::new(Vec::new()),
        });
        let driver: Arc<dyn AgentDriver> = Arc::new(Driver {
            session: Arc::clone(&session),
            works_for: works,
        });
        let drivers: Drivers = Arc::new(move |_| Arc::clone(&driver));
        let state = AppState::new(
            home,
            project.clone(),
            Store::open(&project.join(".loadout/loadout.db"))?,
            drivers,
        );
        Ok(Self {
            _root: root,
            project,
            workflow,
            state,
            session,
            patience,
            said: std::cell::RefCell::new(Vec::new()),
        })
    }

    fn say(
        &self,
        run_id: &str,
        text: &str,
    ) -> loadout_lib::commands::step_message::StepMessageReply {
        self.say_to(run_id, "builder", text)
    }

    fn say_to(
        &self,
        run_id: &str,
        node_key: &str,
        text: &str,
    ) -> loadout_lib::commands::step_message::StepMessageReply {
        self.state
            .send_to_step_in(&self.project, run_id, node_key, text)
    }

    fn delivered(&self) -> Vec<String> {
        self.session
            .delivered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Odpala prawdziwy bieg, wpuszcza `interact` dopiero w środku pierwszej tury i oddaje
    /// zapisany `run.json` razem z adresem biegu.
    async fn drive<F>(&self, interact: F) -> Result<(Value, String), Box<dyn Error>>
    where
        F: AsyncFnOnce(&str) -> Result<(), Box<dyn Error>>,
    {
        let deps = self.state.begin_run(&self.project)?;
        let request = RunRequest {
            workflow: self.workflow.clone(),
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        };
        let (sink, mut output) = line_channel(1024);
        let address = std::cell::RefCell::new(String::new());
        let (ran, checked) = tokio::time::timeout(self.patience, async {
            let running = run_workflow_inner(&deps, &request, sink);
            tokio::pin!(running);
            tokio::select! {
                result = &mut running => {
                    let why = format!("the workflow ended before its first turn began: {result:?}");
                    return (result, Err::<(), Box<dyn Error>>(why.into()));
                }
                () = self.session.entered.notified() => {}
            }
            tokio::join!(running, async {
                let here = deps
                    .control
                    .run_address()
                    .ok_or("the runtime has no durable address")?
                    .id;
                *address.borrow_mut() = here.clone();
                interact(&here).await
            })
        })
        .await?;
        checked?;
        while let Some(line) = output.try_next() {
            self.said.borrow_mut().push(line);
        }
        let report = ran?;
        let run = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
        let here = address.borrow().clone();
        Ok((run, here))
    }
}

fn row<'a>(run: &'a Value, name: &str) -> Result<&'a Value, Box<dyn Error>> {
    run.get("steps")
        .and_then(Value::as_array)
        .and_then(|steps| {
            steps
                .iter()
                .find(|step| step.get("name").and_then(Value::as_str) == Some(name))
        })
        .ok_or_else(|| format!("run.json has no step named {name}").into())
}

struct Driver {
    session: Arc<Session>,
    works_for: Duration,
}

#[async_trait]
impl AgentDriver for Driver {
    fn id(&self) -> &'static str {
        "claude-code"
    }
    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: None,
        })
    }
    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let (voice, inbox) = mpsc::channel(QUEUE_LIMIT + 4);
        let index = self
            .session
            .started
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(Box::new(Handle {
            session: Arc::clone(&self.session),
            index,
            works_for: self.works_for,
            voice,
            inbox,
            turns: 0,
            events,
            session_ref: SessionRef {
                vendor: "claude-code",
                id: spec.run_id.to_string(),
            },
        }))
    }
}

struct Handle {
    session: Arc<Session>,
    /// Który to krok grafu, w kolejności startu.
    index: usize,
    /// Ile każda tura „pracuje" na zegarze. Zero znaczy: kończy się natychmiast.
    works_for: Duration,
    voice: Voice,
    inbox: mpsc::Receiver<ToAgent>,
    turns: u32,
    events: mpsc::Sender<DecodedEvent>,
    session_ref: SessionRef,
}

/// Zużycie kolejnych tur. Suma trzech pierwszych jest dokładna w dwójkowym zapisie.
const TURN_COSTS: [f64; 3] = [0.25, 0.5, 0.125];

#[async_trait]
impl AgentHandle for Handle {
    fn session(&self) -> SessionRef {
        self.session_ref.clone()
    }
    fn group(&self) -> Option<GroupId> {
        None
    }
    fn voice(&self) -> Option<Voice> {
        Some(self.voice.clone())
    }
    async fn send(&mut self, _: String) -> anyhow::Result<()> {
        anyhow::bail!("the runtime must use the exposed channel")
    }
    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        self.turns += 1;
        let text = if self.turns == 1 && self.index > 0 {
            self.session.next_entered.notify_one();
            self.session.next_release.notified().await;
            "the step after it".to_owned()
        } else if self.turns == 1 {
            self.session.entered.notify_one();
            self.session.release.notified().await;
            "turn 1 handled: the original instruction".to_owned()
        } else {
            // Kolejna tura istnieje wyłącznie dlatego, że Loadout naprawdę podał jej treść.
            let Some(ToAgent::Turn(said)) = self.inbox.recv().await else {
                anyhow::bail!("the session was asked for another turn without being given one");
            };
            self.session
                .delivered
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(said.clone());
            format!("turn {} handled: {said}", self.turns)
        };
        if self.works_for > Duration::ZERO {
            tokio::time::sleep(self.works_for).await;
        }
        let index = (self.turns as usize).min(TURN_COSTS.len()) - 1;
        let result = Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text,
            cost_usd: Some(TURN_COSTS[index]),
            tokens: Tokens {
                uncached_input: u64::from(self.turns) * 10,
                cache_read: 0,
                cache_write: 0,
                output: u64::from(self.turns),
            },
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session_ref.clone(),
        };
        let _ = self
            .events
            .send(AgentEvent::Finished(result.clone()).into())
            .await;
        Ok(result)
    }
    async fn cancel(&mut self) -> GroupProof {
        self.session.release.notify_one();
        self.session.next_release.notify_one();
        GroupProof::Dead { status: None }
    }
    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}
