//! AC-4 dla T-115: Codeks dostaje otwieralne pełne ścieżki, a prompt Claude'a się nie zmienia.
//!
//! Specyfikacja nie pyta prywatnego `prompt_for()`. Uruchamia dwa prawdziwe grafy `Source →
//! Reader`, zdejmuje z `RunSpec` prompt zmontowany przez produkcyjny bieg i otwiera wymieniony
//! plik tak, jak zrobiłby to krok — z jego `cwd`, po podanej ścieżce bezwzględnej.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::Vendor;
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

const PATIENCE: Duration = Duration::from_secs(30);
const CODEX_READER: &str = "read codex: use what Source found.";
const CLAUDE_READER: &str = "read claude: use what Source found.";
const FILES_ARE_OUTSIDE: &str =
    "These files are outside your working directory, so read them at the full paths shown.";

const INDEX_OPENS: &str = "Steps before this one left what they found in these files:";
const INDEX_CLOSES: &str =
    "Read the ones you need; their contents were not copied into this prompt.";
const NO_TIME_LIMIT: &str =
    "There is no time limit on this step, so take the time the work really needs.";
const HOW_TO_ANSWER: &str = "Your last message is what this step passes on. What comes next reads it and nothing else, so leave nothing worth keeping outside it.

Write it under these three headings, each one alone on its line and in this order:

## Answer
what comes next needs to know.

## Evidence
`file:line`, or a link, for every claim above.

## Open
what you could not settle.

Do not write your results to a file. Loadout files your last message for you, and a file you write yourself is read by nobody.";

const SOURCE_AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-000000001154
name: Source
summary: Leaves one result
color: moss
runsWith: claude-code
model: opus
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 0
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Find the result.
";

const CODEX_AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-000000001155
name: Codex Reader
summary: Reads a file outside its copy
color: plum
runsWith: codex
model: gpt-5.6-sol
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 0
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Read the result.
";

const CLAUDE_AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-000000001156
name: Claude Reader
summary: Carries an extra directory
color: clay
runsWith: claude-code
model: opus
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 0
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Read the result.
";

const CODEX_WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_t115_codex_paths",
  "name": "Codex reads the result",
  "steps": [
    {
      "kind": "agent",
      "id": "s_source",
      "name": "Source",
      "agent": "01990000-0000-7000-8000-000000001154",
      "overrides": {},
      "instructions": "source: leave the useful result.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_reader",
      "name": "Codex Reader",
      "agent": "01990000-0000-7000-8000-000000001155",
      "overrides": {},
      "instructions": "read codex: use what Source found.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 160 }
    }
  ],
  "links": [{ "from": "s_source", "to": "s_reader" }]
}"#;

const CLAUDE_WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_t115_claude_paths",
  "name": "Claude reads the result",
  "steps": [
    {
      "kind": "agent",
      "id": "s_source",
      "name": "Source",
      "agent": "01990000-0000-7000-8000-000000001154",
      "overrides": {},
      "instructions": "source: leave the useful result.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_reader",
      "name": "Claude Reader",
      "agent": "01990000-0000-7000-8000-000000001156",
      "overrides": {},
      "instructions": "read claude: use what Source found.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 160 }
    }
  ],
  "links": [{ "from": "s_source", "to": "s_reader" }]
}"#;

#[test]
fn carrying_extra_directories_is_the_default_and_codex_explicitly_says_no() {
    assert!(
        ClaudeDriver::new().carries_extra_dirs(),
        "the conservative default keeps today's extra-directory transport for Claude and for \
         every driver that has not opted out"
    );
    assert!(
        !CodexDriver::new().carries_extra_dirs(),
        "Codex has no equivalent of Claude's --add-dir and must explicitly opt out so the \
         prompt can make its absolute paths actionable"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_real_codex_prompt_explains_and_opens_its_outside_path() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let watch = Arc::new(Watch::default());
    let report = bench.run(&bench.codex_workflow, Arc::clone(&watch)).await?;
    let asked = watch.reader(CODEX_READER)?;
    let handoff = source_handoff(&report.dir)?;

    assert!(
        handoff.is_absolute(),
        "the prompt promises a full path, so the value itself must be absolute: {handoff:?}"
    );
    assert!(
        !handoff.starts_with(&asked.cwd),
        "this oracle is about a file outside the step copy, but {:?} sits under {:?}",
        handoff,
        asked.cwd
    );
    assert!(
        asked.prompt.contains(&handoff.display().to_string()),
        "the real prompt does not carry the full path it expects the Codex step to open: {:?}",
        asked.prompt
    );
    assert_eq!(
        asked.prompt.matches(FILES_ARE_OUTSIDE).count(),
        1,
        "the Codex prompt needs one plain-English sentence explaining why these addresses do \
         not live under cwd. It was {:?}",
        asked.prompt
    );

    // `Path::join` with an absolute right-hand side resolves to that absolute path. This is
    // exactly the operation a process standing in `cwd` performs when it opens the address.
    let opened_from_the_step = asked.cwd.join(&handoff);
    assert_eq!(opened_from_the_step, handoff);
    let contents = fs::read_to_string(opened_from_the_step)?;
    assert!(
        contents.contains("Source found the answer"),
        "the address opens, but not to the result Source handed over: {contents:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_claude_prompt_is_byte_for_byte_the_pre_t115_prompt() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let watch = Arc::new(Watch::default());
    let report = bench
        .run(&bench.claude_workflow, Arc::clone(&watch))
        .await?;
    let asked = watch.reader(CLAUDE_READER)?;
    let handoff = source_handoff(&report.dir)?;
    let expected = format!(
        "{CLAUDE_READER}\n\n{INDEX_OPENS}\n- Source: {} (what the step before left)\n\n\
         {INDEX_CLOSES}\n\n{HOW_TO_ANSWER}\n\n{NO_TIME_LIMIT}",
        handoff.display()
    );
    assert_eq!(
        asked.prompt, expected,
        "Claude carries the directory already, so T-115 must not add, remove or reorder one \
         byte of its prompt"
    );
    assert!(
        !asked.prompt.contains(FILES_ARE_OUTSIDE),
        "the Codex-only explanation leaked into Claude's unchanged prompt"
    );
    Ok(())
}

fn source_handoff(run_dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(run_dir.join("handoffs"))? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry.file_name().to_string_lossy().contains("__source__")
        {
            files.push(entry.path());
        }
    }
    assert_eq!(
        files.len(),
        1,
        "the two-step fixture must leave exactly one Source file, not {files:?}"
    );
    files
        .pop()
        .ok_or_else(|| "the handoff disappeared after it was counted".into())
}

#[derive(Clone, Debug)]
struct Asked {
    prompt: String,
    cwd: PathBuf,
}

#[derive(Debug, Default)]
struct Watch(Mutex<Vec<Asked>>);

impl Watch {
    fn record(&self, spec: &RunSpec) {
        self.lock().push(Asked {
            prompt: spec.prompt.clone(),
            cwd: spec.cwd.clone(),
        });
    }

    fn reader(&self, opening: &str) -> Result<Asked, Box<dyn Error>> {
        self.lock()
            .iter()
            .find(|asked| asked.prompt.starts_with(opening))
            .cloned()
            .ok_or_else(|| format!("no driver received a prompt starting with {opening:?}").into())
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Asked>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Debug)]
struct RecordingDriver {
    vendor: &'static str,
    carries: bool,
    watch: Arc<Watch>,
}

#[async_trait]
impl AgentDriver for RecordingDriver {
    fn id(&self) -> &'static str {
        self.vendor
    }

    fn carries_extra_dirs(&self) -> bool {
        self.carries
    }

    fn with_evidence(&self, _target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            vendor: self.vendor,
            carries: self.carries,
            watch: Arc::clone(&self.watch),
        }))
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("recording fixture".to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.watch.record(&spec);
        let session = SessionRef {
            vendor: self.vendor,
            id: spec.run_id.to_string(),
        };
        let _ = events
            .send(
                (AgentEvent::Started {
                    session: session.clone(),
                    model: spec.model.unwrap_or_default(),
                    tools: Vec::new(),
                    capabilities: Vec::new(),
                })
                .into(),
            )
            .await;
        Ok(Box::new(RecordingTurn { events, session }))
    }
}

#[derive(Debug)]
struct RecordingTurn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for RecordingTurn {
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
        let outcome = Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "## Answer\nSource found the answer.\n\n## Evidence\nnone.\n\n## Open\nnothing."
                .to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_secs(1),
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send((AgentEvent::Finished(outcome.clone())).into())
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

struct Bench {
    home: TempDir,
    project: TempDir,
    codex_workflow: PathBuf,
    claude_workflow: PathBuf,
    store: Store,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(home.path().join("agents/source.md"), SOURCE_AGENT)?;
        fs::write(home.path().join("agents/codex-reader.md"), CODEX_AGENT)?;
        fs::write(home.path().join("agents/claude-reader.md"), CLAUDE_AGENT)?;
        let codex_workflow = home.path().join("workflows/codex-paths.json");
        fs::write(&codex_workflow, CODEX_WORKFLOW)?;
        let claude_workflow = home.path().join("workflows/claude-paths.json");
        fs::write(&claude_workflow, CLAUDE_WORKFLOW)?;
        let store = Store::open(&project.path().join(".loadout/loadout.db"))?;
        Ok(Self {
            home,
            project,
            codex_workflow,
            claude_workflow,
            store,
        })
    }

    async fn run(
        &self,
        workflow: &Path,
        watch: Arc<Watch>,
    ) -> Result<loadout_lib::commands::RunReport, Box<dyn Error>> {
        let claude: Arc<dyn AgentDriver> = Arc::new(RecordingDriver {
            vendor: "claude",
            carries: true,
            watch: Arc::clone(&watch),
        });
        let codex: Arc<dyn AgentDriver> = Arc::new(RecordingDriver {
            vendor: "codex",
            carries: false,
            watch,
        });
        let drivers: Drivers = Arc::new(move |vendor| match vendor {
            Vendor::ClaudeCode => Arc::clone(&claude),
            Vendor::Codex => Arc::clone(&codex),
        });
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store: &self.store,
            drivers,
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow: workflow.to_path_buf(),
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        };
        let (sink, source) = line_channel(QUEUE_CAP);
        let pump = spawn_pump(source, Channel::new(|_| Ok(())));
        let (report, _) = tokio::time::timeout(PATIENCE, async {
            tokio::join!(run_workflow_inner(&deps, &request, sink), pump)
        })
        .await
        .map_err(|_| format!("the handoff-path run did not finish within {PATIENCE:?}"))?;
        Ok(report?)
    }
}
