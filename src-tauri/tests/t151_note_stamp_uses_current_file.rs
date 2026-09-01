//! T-151 AC-1: the run freezes context, while the usage stamp edits only the current file.

use std::error::Error as StdError;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::memory::{
    NoteAddress, NotePlace, discard_addressed_note_inner, stop_using_addressed_note_inner,
};
use loadout_lib::commands::run::{FrozenPromptHook, run_workflow_with_snapshot_hook};
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, StepSettings, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::{EvidenceStreams, EvidenceTarget};
use loadout_lib::ipc::{QUEUE_CAP, line_channel};
use loadout_lib::library::agents::Vendor;
use loadout_lib::memory::notes::{RealMoveIo, move_note_file_with_io};
use loadout_lib::store::Store;
use serde_json::Value;
use tempfile::TempDir;
use tokio::sync::mpsc;

const PATIENCE: Duration = Duration::from_secs(20);
const ORIGINAL_RULE: &str = "T151 original rule frozen for this run";
const CURRENT_RULE: &str = "T151 current rule edited after prompt planning";
const ANSWER: &str = "## Answer\nDone.\n\n## Evidence\nfixture.rs:1\n\n## Open\nNone.\n";

const AGENT: &str = r"---
schema: 1
id: 01990000-0000-7000-8000-000000000151
name: T151 Builder
summary: Exercises the current-file stamp boundary
color: slate
runsWith: claude-code
model: haiku
thinking: balanced
fileAccess: look-only
giveUpAfterMinutes: 20
writeResultsTo: handoffs/result.md
tools: everything
skills: []
connections: []
---
Return the fixture result.
";

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_t151_current_note",
  "name": "T151 current note",
  "steps": [{
    "kind": "agent", "id": "build", "name": "Build",
    "agent": "01990000-0000-7000-8000-000000000151", "overrides": {},
    "copies": 1, "instructions": "Return the fixture result.", "skills": "all",
    "folder": { "use": "project" }, "handover": "notes",
    "at": { "x": 0, "y": 0 }
  }],
  "links": []
}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stamp_changes_only_last_used_at_in_the_current_file() -> Result<(), Box<dyn StdError>> {
    let scene = Scene::new(NotePlace::Project)?;
    let current = current_note("this-project");
    let hook = scene.edit_hook(current.clone());
    let (report, prompts) = scene.run(hook).await?;

    let stamped = fs::read_to_string(&scene.source)?;
    assert_eq!(restore_null_stamp(&stamped)?, current);
    assert!(stamped.contains("modified: 2026-08-28T12:00:00Z"));
    assert!(stamped.contains("unknown_key: current-value"));
    assert!(stamped.contains("T151 current body"));
    assert_frozen_run(&report, &prompts)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn move_after_prompt_freeze_is_not_undone_by_the_stamp() -> Result<(), Box<dyn StdError>> {
    let scene = Scene::new(NotePlace::Library)?;
    let moved_bytes = current_note("everywhere");
    let target = scene.project_memory().join("notes/frozen.md");
    let hook = scene.move_hook(moved_bytes.clone());
    let (report, prompts) = scene.run(hook).await?;

    assert!(
        !scene.source.exists(),
        "the stamp recreated the moved source"
    );
    assert_eq!(fs::read_to_string(target)?, moved_bytes);
    assert_frozen_run(&report, &prompts)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn discard_after_prompt_freeze_is_not_resurrected_by_the_stamp()
-> Result<(), Box<dyn StdError>> {
    let scene = Scene::new(NotePlace::Project)?;
    let discarded_bytes = current_note("this-project");
    let stopped_bytes = stopped_note("this-project");
    let hook = scene.discard_hook(discarded_bytes);
    let (report, prompts) = scene.run(hook).await?;

    assert!(
        !scene.source.exists(),
        "the stamp resurrected the discarded note"
    );
    let discarded = only_markdown(&scene.project_memory().join("discarded"))?;
    assert_eq!(fs::read_to_string(discarded)?, stopped_bytes);
    assert_frozen_run(&report, &prompts)?;
    Ok(())
}

struct Scene {
    _root: TempDir,
    home: PathBuf,
    project: TempDir,
    source: PathBuf,
}

impl Scene {
    fn new(place: NotePlace) -> Result<Self, Box<dyn StdError>> {
        let root = tempfile::tempdir()?;
        let home = root.path().join("home");
        let project = tempfile::tempdir()?;
        fs::create_dir_all(home.join("agents"))?;
        fs::create_dir_all(home.join("workflows"))?;
        fs::create_dir_all(home.join("memory/notes"))?;
        fs::create_dir_all(project.path().join(".loadout/memory/notes"))?;
        fs::write(home.join("agents/t151-builder.md"), AGENT)?;
        fs::write(home.join("workflows/t151.json"), WORKFLOW)?;
        let source = match &place {
            NotePlace::Library => home.join("memory/notes/frozen.md"),
            NotePlace::Project => project.path().join(".loadout/memory/notes/frozen.md"),
        };
        fs::write(&source, original_note(&place))?;
        Ok(Self {
            _root: root,
            home,
            project,
            source,
        })
    }

    fn project_memory(&self) -> PathBuf {
        self.project.path().join(".loadout/memory")
    }

    fn edit_hook(&self, current: String) -> FrozenPromptHook {
        let source = self.source.clone();
        Arc::new(move |prompt| {
            assert!(prompt.contains(ORIGINAL_RULE));
            assert!(!prompt.contains(CURRENT_RULE));
            fs::write(&source, &current).expect("the T151 hook must edit the live note");
        })
    }

    fn move_hook(&self, current: String) -> FrozenPromptHook {
        let source = self.source.clone();
        let target = self.project_memory().join("notes/frozen.md");
        Arc::new(move |prompt| {
            assert!(prompt.contains(ORIGINAL_RULE));
            fs::write(&source, &current).expect("the T151 hook must edit before Move");
            let mut io = RealMoveIo;
            let moved = move_note_file_with_io(&mut io, &source, &target);
            assert!(
                moved.is_ok(),
                "the production Move must finish inside the hook"
            );
        })
    }

    fn discard_hook(&self, current: String) -> FrozenPromptHook {
        let source = self.source.clone();
        let library = self.home.join("memory");
        let project = self.project.path().to_owned();
        Arc::new(move |prompt| {
            assert!(prompt.contains(ORIGINAL_RULE));
            fs::write(&source, &current).expect("the T151 hook must edit before Discard");
            let address = NoteAddress {
                place: NotePlace::Project,
                id: "frozen".to_owned(),
            };
            let stopped = stop_using_addressed_note_inner(
                &library,
                &project,
                &address,
                "2026-08-28T12:00:30Z",
            );
            assert!(
                stopped.is_ok(),
                "the production Stop using must make the note eligible for Discard"
            );
            let discarded = discard_addressed_note_inner(
                &library,
                &project,
                &address,
                "2026-08-28T12:01:00Z",
            );
            assert!(
                discarded.is_ok(),
                "the production Discard must finish inside the hook"
            );
        })
    }

    async fn run(
        &self,
        hook: FrozenPromptHook,
    ) -> Result<(RunReport, Vec<String>), Box<dyn StdError>> {
        let store = Store::open(&self.project.path().join(".loadout/loadout.db"))?;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let deps = RunDeps {
            home: &self.home,
            project: self.project.path(),
            store: &store,
            drivers: fake_drivers(Arc::clone(&seen)),
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow: self.home.join("workflows/t151.json"),
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        };
        let (lines, _source) = line_channel(QUEUE_CAP);
        let report = tokio::time::timeout(
            PATIENCE,
            run_workflow_with_snapshot_hook(&deps, &request, lines, None, false, Some(hook)),
        )
        .await
        .map_err(|_| "the T151 fixture timed out")??;
        let prompts = seen.lock().map(|one| one.clone()).unwrap_or_default();
        Ok((report, prompts))
    }
}

fn original_note(place: &NotePlace) -> String {
    let scope = match place {
        NotePlace::Library => "everywhere",
        NotePlace::Project => "this-project",
    };
    format!(
        "---\nscope: {scope}\nkind: rule\ntitle: Frozen note\nrule: {ORIGINAL_RULE}\nbecause: T151 freezes this before the hook\nstatus: in-use\noccurrences: 1\nmodified: 2026-08-28T10:00:00Z\nunknown_key: original-value\nlast_used_at: null\n---\n\nT151 original body\n"
    )
}

fn current_note(scope: &str) -> String {
    format!(
        "---\nscope: {scope}\nkind: rule\ntitle: Frozen note\nrule: {CURRENT_RULE}\nbecause: T151 keeps current bytes\nstatus: in-use\noccurrences: 1\nmodified: 2026-08-28T12:00:00Z\nunknown_key: current-value\nlast_used_at: null\n---\n\nT151 current body\n"
    )
}

fn stopped_note(scope: &str) -> String {
    current_note(scope)
        .replacen("status: in-use", "status: suggested", 1)
        .replacen(
            "modified: 2026-08-28T12:00:00Z",
            "modified: 2026-08-28T12:00:30Z",
            1,
        )
}

fn restore_null_stamp(stamped: &str) -> Result<String, Box<dyn StdError>> {
    let line = stamped
        .lines()
        .find(|line| line.starts_with("last_used_at:"))
        .ok_or("the current file has no last_used_at line")?;
    assert_ne!(
        line, "last_used_at: null",
        "the current file was not stamped"
    );
    Ok(stamped.replacen(line, "last_used_at: null", 1))
}

fn only_markdown(directory: &Path) -> Result<PathBuf, Box<dyn StdError>> {
    let files = fs::read_dir(directory)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect::<Vec<_>>();
    assert_eq!(
        files.len(),
        1,
        "Discard must leave exactly one archived note"
    );
    files
        .into_iter()
        .next()
        .ok_or_else(|| "the archive is empty".into())
}

fn assert_frozen_run(report: &RunReport, prompts: &[String]) -> Result<(), Box<dyn StdError>> {
    assert_eq!(prompts.len(), 1);
    assert!(prompts[0].contains(ORIGINAL_RULE));
    assert!(!prompts[0].contains(CURRENT_RULE));
    let receipt: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    let memory = receipt["memory"]
        .as_array()
        .ok_or("receipt has no memory list")?;
    assert_eq!(memory.len(), 1);
    assert_eq!(
        memory[0]["bytes"]
            .as_u64()
            .and_then(|bytes| usize::try_from(bytes).ok()),
        Some(ORIGINAL_RULE.len())
    );
    Ok(())
}

fn fake_drivers(seen: Arc<Mutex<Vec<String>>>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        seen,
        evidence: None,
    });
    Arc::new(move |_vendor: Vendor| Arc::clone(&driver))
}

#[derive(Clone)]
struct Fake {
    seen: Arc<Mutex<Vec<String>>>,
    evidence: Option<EvidenceTarget>,
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        "t151-fake"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("t151".to_owned()),
        })
    }

    fn with_settings(
        &self,
        _settings: &StepSettings,
    ) -> Option<anyhow::Result<Arc<dyn AgentDriver>>> {
        Some(Ok(Arc::new(self.clone())))
    }

    fn with_evidence(&self, target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            seen: Arc::clone(&self.seen),
            evidence: Some(target),
        }))
    }

    fn with_budget(&self, _dollars: f64) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        if let Ok(mut prompts) = self.seen.lock() {
            prompts.push(spec.prompt.clone());
        }
        self.write_evidence().await?;
        let session = SessionRef {
            vendor: "t151-fake",
            id: spec.run_id.to_string(),
        };
        let _ = events
            .send(
                AgentEvent::Started {
                    session: session.clone(),
                    model: spec.model.unwrap_or_default(),
                    tools: Vec::new(),
                    capabilities: Vec::new(),
                }
                .into(),
            )
            .await;
        Ok(Box::new(Turn { events, session }))
    }
}

impl Fake {
    async fn write_evidence(&self) -> anyhow::Result<()> {
        let target = self
            .evidence
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("the T151 turn has no evidence target"))?;
        let EvidenceStreams {
            mut stdout,
            mut stderr,
        } = target.open().await?;
        stdout.write(b"{\"type\":\"t151-fixture\"}\n").await?;
        stderr.write(b"t151 fixture stderr\n").await?;
        stdout.close().await?;
        stderr.close().await?;
        Ok(())
    }
}

struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: ANSWER.to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(2),
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
