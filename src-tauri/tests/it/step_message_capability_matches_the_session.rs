//! WF-08 RED: public `AppState` → prawdziwe planowanie → utworzony `AgentHandle`.
//! Dubler zastępuje wyłącznie vendora, nigdy rejestr sesji ani wynik wysyłki.

#![allow(clippy::expect_used, clippy::too_many_lines, clippy::similar_names)]

use std::error::Error;
use std::fs;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::step_message::StepMessageResult;
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
use serde_json::json;
use tokio::sync::{Notify, mpsc};

#[tokio::test]
async fn a_running_handle_without_voice_is_unsupported_not_finished() -> Result<(), Box<dyn Error>>
{
    scenario(false, true).await
}

#[tokio::test]
async fn a_handle_with_voice_accepts_exact_text_at_its_exact_address() -> Result<(), Box<dyn Error>>
{
    scenario(true, true).await
}

#[tokio::test]
async fn fixed_inputs_refuse_messages_even_when_the_real_session_has_voice()
-> Result<(), Box<dyn Error>> {
    scenario(true, false).await
}

async fn scenario(with_voice: bool, external_messages: bool) -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let project = root.path().canonicalize()?.join("project");
    let home = root.path().canonicalize()?.join("home");
    fs::create_dir_all(project.join(".loadout"))?;
    fs::create_dir_all(project.join(".loadout/workflows"))?;
    let agent = Agent::example();
    write_agent_file(&project.join(".loadout/agents"), &agent, None)?;
    let workflow = project.join(".loadout/workflows/message.json");
    fs::write(
        &workflow,
        json!({"format":1,"id":"wf-message","name":"Message",
        "executionInputs":{"schema":1,"effects":{"externalMessages":external_messages}},
        "steps":[{"kind":"agent","id":"builder","name":"Builder","agent":agent.id,
        "instructions":"Return a brief result.","overrides":{},"folder":{"use":"project"},
        "at":{"x":0,"y":0}}],"links":[]})
        .to_string(),
    )?;
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let (voice, mut inbox) = mpsc::channel(4);
    let driver: Arc<dyn AgentDriver> = Arc::new(Driver {
        entered: Arc::clone(&entered),
        release: Arc::clone(&release),
        voice: with_voice.then_some(voice),
    });
    let drivers: Drivers = Arc::new(move |_| Arc::clone(&driver));
    let state = AppState::new(
        home,
        project.clone(),
        Store::open(&project.join(".loadout/loadout.db"))?,
        drivers,
    );
    let deps = state.begin_run(&project)?;
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (sink, mut output) = line_channel(256);
    let (ran, checked) = tokio::time::timeout(Duration::from_secs(10), async {
        let running = run_workflow_inner(&deps, &request, sink);
        tokio::pin!(running);
        tokio::select! {
            result = &mut running => {
                let why = format!("the real workflow ended before creating the message session: {result:?}");
                return (result, Err::<(), Box<dyn Error>>(why.into()));
            }
            () = entered.notified() => {},
        }
        tokio::join!(running, async {
            let address = deps.control.run_address().ok_or("runtime has no durable address")?;
            let reply = state.send_to_step_in(&project, &address.id, "builder", "  exact message  ");
            if !external_messages {
                let refused = "This comparison uses fixed inputs. Start a new run to change them.";
                assert_eq!(serde_json::to_value(reply.result)?, json!("fixedInputs"),
                    "a fixed-input comparison accepted an external instruction");
                assert_eq!(reply.said, refused);
                assert!(state.step_message_recipients_in(&project).iter().all(|one| !one.can_receive));
                let legacy = state.say_in_the_run_at(&project, Some("Builder"), "legacy bypass").await;
                assert!(legacy.is_err_and(|error| error.to_string() == refused));
                let fixed = deps.control.clone();
                let (lead_sink, mut lead_lines) = line_channel(32);
                let desk = loadout_lib::bridge::library::Desk::at(None, project.clone())
                    .showing(Arc::new(std::sync::Mutex::new(lead_sink)))
                    .reading_runs_with(loadout_lib::commands::lead_history::LeadRunLookup::new(move |_| Some(fixed.clone())));
                let lead = loadout_lib::bridge::host::Answers::answer(&desk, loadout_lib::bridge::Call {
                    id: json!(1), call: "send_to_step".to_owned(),
                    input: json!({"run_id":address.id,"node_key":"builder","text":"model bypass"}),
                }).await;
                assert!(matches!(lead, loadout_lib::bridge::Answer::Ok(value) if value["result"] == "fixedInputs"));
                assert!(matches!(lead_lines.try_next(), Some(Line::Note { text, .. }) if text == refused),
                    "the Lead conversation did not show the actual fixed-input refusal");
                assert!(inbox.try_recv().is_err(), "a blocked instruction still reached the vendor");
                release.notify_one();
                return Ok::<(), Box<dyn Error>>(());
            }
            assert_eq!(reply.result, if with_voice { StepMessageResult::AcceptedBySession }
                else { StepMessageResult::UnsupportedDuringRun });
            let recipients = state.step_message_recipients_in(&project);
            let actual = recipients.iter().find(|one| one.node_key == "builder").ok_or("created handle absent from capabilities")?;
            assert_eq!(actual.run_id, address.id);
            assert_eq!(actual.can_receive, with_voice);
            assert!(!actual.finished);
            let stale = state.send_to_step_in(&project, "older-run", "builder", "do not deliver");
            assert_eq!(stale.result, StepMessageResult::StaleRun);
            let wrong = state.send_to_step_in(&project, &address.id, "Builder", "do not deliver");
            assert_eq!(wrong.result, StepMessageResult::NoSuchStep, "a display name is not an exact session address");
            if with_voice {
                /* L-01 (2026-09-06): PRZYJĘTE PRZEZ LOADOUT TO NIE TO SAMO, CO PODANE
                 * TRANSPORTOWI. Do tego dnia wiadomość szła prosto w kanał vendora w środku
                 * trwającej tury — a Loadout, nic o niej nie wiedząc, zamykał wejście zaraz
                 * po pierwszym `result` i zabijał turę, którą vendor dla niej zaczął (I-01).
                 * Teraz czeka w kolejce biegu i dostaje własną turę. */
                assert!(inbox.try_recv().is_err(),
                    "an accepted message was pushed into the running turn instead of waiting for its own");
            }
            let mut lines = Vec::new();
            while let Some(line) = output.try_next() { lines.push(line); }
            if with_voice {
                assert!(lines.iter().any(|line| matches!(line, Line::Told { agent, text }
                    if agent == "Builder" && text == "  exact message  ")));
            } else {
                assert!(reply.said.contains("does not accept messages while it is running"));
                assert!(!lines.iter().any(|line| matches!(line, Line::Told { .. })),
                    "a refused message is not a delivered instruction");
            }
            // Entry shows the returned refusal in the sending conversation (browser criterion
            // step-message-refusal-appears-once). A second Problem here duplicates that fact,
            // or worse: sends an old run's refusal into a replacement run's unrelated stream.
            assert!(!lines.iter().any(|line| matches!(line, Line::Problem { text, .. }
                if text == &reply.said || text == &stale.said || text == &wrong.said)),
                "the sender owns a refusal; the runtime must not echo it into another stream");
            release.notify_one();
            if with_voice {
                // Dopiero po wyniku bieżącej tury Loadout podaje przyjętą wiadomość sesji.
                let handed = tokio::time::timeout(Duration::from_secs(5), inbox.recv())
                    .await
                    .map_err(|_| "the accepted message never reached the session as its own turn")?;
                assert!(matches!(handed, Some(ToAgent::Turn(text)) if text == "  exact message  "));
                assert!(inbox.try_recv().is_err(), "stale or misaddressed message reached the channel");
            }
            Ok::<(), Box<dyn Error>>(())
        })
    }).await?;
    checked?;
    let _ = ran?;
    assert!(
        state
            .step_message_recipients_in(&project)
            .iter()
            .all(|one| !one.can_receive),
        "completion left a live capability behind"
    );
    Ok(())
}

struct Driver {
    entered: Arc<Notify>,
    release: Arc<Notify>,
    voice: Option<Voice>,
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
        Ok(Box::new(Handle {
            entered: Arc::clone(&self.entered),
            release: Arc::clone(&self.release),
            voice: self.voice.clone(),
            events,
            session: SessionRef {
                vendor: "claude-code",
                id: spec.run_id.to_string(),
            },
            turns: 0,
        }))
    }
}

struct Handle {
    entered: Arc<Notify>,
    release: Arc<Notify>,
    voice: Option<Voice>,
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    /// L-01: kolejna tura kończy się od razu — treść czyta sam test, prosto z kanału.
    turns: u32,
}

#[async_trait]
impl AgentHandle for Handle {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }
    fn group(&self) -> Option<GroupId> {
        None
    }
    fn voice(&self) -> Option<Voice> {
        self.voice.clone()
    }
    async fn send(&mut self, _: String) -> anyhow::Result<()> {
        anyhow::bail!("the runtime must use the exposed channel")
    }
    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        self.turns += 1;
        if self.turns == 1 {
            self.entered.notify_one();
            self.release.notified().await;
        }
        let result = Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "A saved result".to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send(AgentEvent::Finished(result.clone()).into())
            .await;
        Ok(result)
    }
    async fn cancel(&mut self) -> GroupProof {
        self.release.notify_one();
        GroupProof::Dead { status: None }
    }
    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}
