//! WF-22 RED: nowy Threads dostaje ograniczone fakty w rzeczywistym `RunSpec`, bez prompt archive.

use std::error::Error;
use std::fs;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::Drivers;
use loadout_lib::commands::chat::{Lead, Terminal, Threads};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::line_channel;
use loadout_lib::library::agents::Agent;
use serde_json::json;
use tokio::sync::mpsc;

const RUN: &str = "01980000-0000-7000-8000-000000000001";
const ACCEPTED: &str = "Only approved endpoint conventions belong in this project.";
const REJECTED: &str = "REJECTED-NOTE-DO-NOT-INHERIT";
const PRIVATE: &str = "PRIVATE-OTHER-WORKSPACE";

#[tokio::test]
async fn editing_then_withdrawing_the_last_note_refreshes_the_same_conversation()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let home = root.path().join("home");
    let project = root.path().join("project");
    fs::create_dir_all(&home)?;
    fs::create_dir_all(project.join(".loadout/memory/notes"))?;
    let note = project.join(".loadout/memory/notes/accepted.md");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let driver: Arc<dyn AgentDriver> = Arc::new(Recorder {
        seen: Arc::clone(&seen),
    });
    let drivers: Drivers = Arc::new(move |_| Arc::clone(&driver));
    let threads = Threads::new();
    threads.library_is(home);
    let terminal = Terminal {
        id: "same-conversation".to_owned(),
        folder: project,
    };
    let (sink, mut lines) = line_channel(256);
    threads.terminal_lines_go_to(&terminal, sink);
    let lead = Lead {
        agent: Agent::example(),
    };
    for (rule, status) in [
        ("FIRST-APPROVED-RULE", "in-use"),
        ("UPDATED-APPROVED-RULE", "in-use"),
        ("UPDATED-APPROVED-RULE", "suggested"),
    ] {
        fs::write(
            &note,
            format!(
                "---\nscope: this-project\nkind: fact\ntitle: Accepted rule\nrule: {rule}\nbecause: A person chose this\nstatus: {status}\noccurrences: 1\nmodified: 2026-09-06T03:00:00Z\n---\n"
            ),
        )?;
        threads
            .say_in(
                &drivers,
                &lead,
                &terminal,
                "Which project facts are current now?",
            )
            .await?;
        while lines.try_next().is_some() {}
    }
    let _ = threads.close().await;
    let seen = seen.lock().unwrap_or_else(PoisonError::into_inner);
    assert_eq!(
        seen.len(),
        3,
        "all three real turns must reach the existing agent"
    );
    assert!(seen[0].0.contains("FIRST-APPROVED-RULE"));
    assert!(seen[1].0.contains("UPDATED-APPROVED-RULE"));
    assert!(!seen[1].0.contains("FIRST-APPROVED-RULE"));
    assert!(!seen[2].0.contains("UPDATED-APPROVED-RULE"));
    assert!(
        seen[2].0.contains("\"acceptedProjectNotes\":[]"),
        "withdrawing the last note silently left the old approved context in force: {}",
        seen[2].0
    );
    assert!(
        seen[2]
            .0
            .contains("saved project context refreshed between turns")
    );
    assert!(
        seen[1].1 && seen[2].1,
        "refresh must use the same conversation, not reset the agent"
    );
    Ok(())
}

#[tokio::test]
async fn a_fresh_conversation_gets_saved_facts_without_resuming_old_messages()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let home = root.path().join("home");
    let project = root.path().join("project");
    let other = root.path().join("other");
    fs::create_dir_all(&home)?;
    fs::create_dir_all(project.join(".loadout/memory/notes"))?;
    fs::create_dir_all(other.join(".loadout/memory/notes"))?;
    let directory = project
        .join(".loadout/runs")
        .join(format!("20260904-100000__{RUN}"));
    fs::create_dir_all(&directory)?;
    fs::write(
        directory.join("run.json"),
        json!({"id": RUN, "title": "Earlier parser fix",
        "status": "succeeded", "workflow_id": "wf-parser", "steps": [],
        "old_consent": "OLD-CONSENT-DO-NOT-RESTORE", "unverified_model_claim": "Never tested"})
        .to_string(),
    )?;
    for (at, name, rule, status) in [
        (&project, "accepted", ACCEPTED, "in-use"),
        (&project, "suggested", REJECTED, "suggested"),
        (&other, "private", PRIVATE, "in-use"),
    ] {
        fs::write(
            at.join(".loadout/memory/notes").join(format!("{name}.md")),
            format!(
                "---\nscope: this-project\nkind: fact\ntitle: {name}\nrule: {rule}\nbecause: A person accepted this\nstatus: {status}\noccurrences: 1\nmodified: 2026-09-04T10:00:00Z\n---\n"
            ),
        )?;
    }
    let seen = Arc::new(Mutex::new(Vec::new()));
    let driver: Arc<dyn AgentDriver> = Arc::new(Recorder {
        seen: Arc::clone(&seen),
    });
    let drivers: Drivers = Arc::new(move |_| Arc::clone(&driver));
    let lead = Lead {
        agent: Agent::example(),
    };
    for index in 0..2 {
        let threads = Threads::new();
        threads.library_is(home.clone());
        let terminal = Terminal {
            id: format!("fresh-{index}"),
            folder: project.clone(),
        };
        let (sink, mut source) = line_channel(128);
        threads.terminal_lines_go_to(&terminal, sink);
        threads
            .say_in(&drivers, &lead, &terminal, "What did we finish here?")
            .await?;
        let mut received = Vec::new();
        while let Some(line) = source.try_next() {
            received.push(line);
        }
        assert!(
            received
                .iter()
                .any(|line| matches!(line, Line::Note { text, .. }
            if text == "New conversation with saved project context.")),
            "the person must see the same honest scope of restored context as the Lead"
        );
        assert!(
            received
                .iter()
                .all(|line| !matches!(line, Line::Told { text, .. }
            if text.contains(ACCEPTED) || text.contains(RUN))),
            "internal factual context must not masquerade as a human message"
        );
        let _ = threads.close().await;
    }
    let seen = seen.lock().unwrap_or_else(PoisonError::into_inner);
    assert_eq!(
        seen.len(),
        2,
        "both new registries must reach a real driver"
    );
    for (prompt, resumed) in seen.iter() {
        assert!(
            !resumed,
            "saved facts must not impersonate a restored vendor session"
        );
        assert!(
            prompt.contains(RUN),
            "the new conversation has no old run identity"
        );
        assert!(
            prompt.contains(ACCEPTED),
            "the approved project rule did not reach the Lead"
        );
        assert!(!prompt.contains(REJECTED));
        assert!(!prompt.contains(PRIVATE));
        assert!(!prompt.contains("OLD-CONSENT-DO-NOT-RESTORE"));
        assert!(!prompt.contains("Never tested"));
        assert!(prompt.contains("new conversation with saved project context"));
    }
    Ok(())
}

struct Recorder {
    seen: Arc<Mutex<Vec<(String, bool)>>>,
}

#[async_trait]
impl AgentDriver for Recorder {
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
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((
                format!(
                    "{}\n{}",
                    spec.system_append.unwrap_or_default(),
                    spec.prompt
                ),
                spec.resume.is_some(),
            ));
        Ok(Box::new(Turn {
            events,
            seen: Arc::clone(&self.seen),
            session: SessionRef {
                vendor: "claude-code",
                id: spec.run_id.to_string(),
            },
        }))
    }
}

struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    seen: Arc<Mutex<Vec<(String, bool)>>>,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }
    fn group(&self) -> Option<GroupId> {
        None
    }
    async fn send(&mut self, text: String) -> anyhow::Result<()> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((text, true));
        Ok(())
    }
    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        let outcome = Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "Ready".to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send(AgentEvent::Finished(outcome.clone()).into())
            .await;
        Ok(outcome)
    }
    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }
    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}
