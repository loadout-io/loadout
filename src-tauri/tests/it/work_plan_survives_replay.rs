//! WP-06: zapisany plan jest prywatnym, związanym wejściem powtórzenia.

#![allow(
    clippy::expect_used,
    clippy::too_many_lines,
    reason = "WP-06 keeps the real-adapter replay story and its filesystem fixture together"
)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::bridge::host::Bridge;
use loadout_lib::bridge::work_plan::PlanDesk;
use loadout_lib::bridge::{Answer, Call, Greeting, Reply};
use loadout_lib::commands::processes::Processes;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, DriverConfiguration, FinishReason, Outcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::{AppState, line_channel};
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::step_receives_selected_context::Bench;

const FROZEN_REQUIREMENT: &str = "WP06-FROZEN-REQUIREMENT remains byte-for-byte stable.";

const CANDIDATE: &str = r#"{
  "goal": "Keep the recorded requirement stable.",
  "inScope": ["Recorded replay"],
  "outOfScope": ["Unrelated workflows"],
  "requirements": [{
    "id": "WP06-R1",
    "text": "WP06-FROZEN-REQUIREMENT remains byte-for-byte stable.",
    "acceptance": [{
      "text": "The recorded consumer receives the same bytes.",
      "verification": "Inspect the controlled adapter input."
    }]
  }],
  "acceptance": [],
  "decisions": ["Recorded replay keeps the published identity."],
  "sections": {
    "Implementation": "Hand the pinned version to the consumer.",
    "Validation": "Compare the adapter input byte for byte."
  },
  "proposals": [],
  "assumptions": [],
  "questions": [],
  "conflicts": [],
  "sources": []
}"#;

#[derive(Default)]
struct Observed {
    prompts: Mutex<Vec<String>>,
    active_consumers: AtomicUsize,
    most_consumers: AtomicUsize,
}

fn drivers(observed: Arc<Observed>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(PlanWriter { observed });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

struct PlanWriter {
    observed: Arc<Observed>,
}

#[async_trait]
impl AgentDriver for PlanWriter {
    fn id(&self) -> &'static str {
        "claude"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: None,
        })
    }

    fn configured(&self, _configuration: &DriverConfiguration) -> Option<Arc<dyn AgentDriver>> {
        // 2026-09-08 (WP-06): prawdziwa droga Use dodaje most jako Connection; dubel, który
        // go odrzuca, zatrzymałby się przed adapterem i nie sprawdził żadnego bajtu promptu.
        Some(Arc::new(Self {
            observed: Arc::clone(&self.observed),
        }))
    }

    fn with_evidence(&self, _target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        // 2026-09-08 (WP-06): nazwa produkcyjnego vendora włącza prywatny cel dowodu;
        // fikstura zachowuje tę drogę, żeby dotrzeć do kontrolowanego adaptera.
        Some(Arc::new(Self {
            observed: Arc::clone(&self.observed),
        }))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        if let Some(path) = spec
            .prompt
            .lines()
            .find_map(|line| line.strip_prefix("LOADOUT_PLAN_CANDIDATE_PATH="))
        {
            let path = PathBuf::from(path);
            let path = if path.is_absolute() {
                path
            } else {
                spec.cwd.join(path)
            };
            fs::create_dir_all(
                path.parent()
                    .ok_or_else(|| anyhow::anyhow!("the candidate path has no parent"))?,
            )?;
            fs::write(path, CANDIDATE)?;
        }
        self.observed
            .prompts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(spec.prompt.clone());
        let is_consumer = spec.prompt.contains("WP06-CONSUMER");
        if is_consumer {
            let active = self
                .observed
                .active_consumers
                .fetch_add(1, Ordering::SeqCst)
                + 1;
            self.observed
                .most_consumers
                .fetch_max(active, Ordering::SeqCst);
        }
        Ok(Box::new(Turn {
            events,
            session: SessionRef {
                vendor: self.id(),
                id: spec.run_id.to_string(),
            },
            active_consumers: is_consumer.then(|| Arc::clone(&self.observed)),
        }))
    }
}

struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    active_consumers: Option<Arc<Observed>>,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        None
    }

    async fn send(&mut self, _: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        if let Some(observed) = self.active_consumers.take() {
            tokio::time::sleep(Duration::from_millis(30)).await;
            observed.active_consumers.fetch_sub(1, Ordering::SeqCst);
        }
        let outcome = Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "Used the plan.".to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session(),
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

fn workflow(bench: &Bench) -> Result<PathBuf, Box<dyn Error>> {
    workflow_with_copies(bench, 1)
}

fn workflow_with_copies(bench: &Bench, use_copies: usize) -> Result<PathBuf, Box<dyn Error>> {
    bench.workflow(
        "work-plan-replay",
        &json!({
            "format": 3,
            "id": "wp-06-recorded-plan",
            "name": "Replay a recorded plan",
            "steps": [
                {
                    "kind": "agent",
                    "id": "create",
                    "name": "Create",
                    "agent": "01990000-0000-7000-8000-000000000606",
                    "instructions": "WP06-AUTHOR create the shared plan.",
                    "plan": {"mode": "create"},
                    "overrides": {},
                    "folder": {"use": "fresh-copy"},
                    "at": {"x": 0, "y": 0}
                },
                {
                    "kind": "agent",
                    "id": "use",
                    "name": "Use",
                    "agent": "01990000-0000-7000-8000-000000000606",
                    "instructions": "WP06-CONSUMER use the complete pinned plan.",
                    "copies": use_copies,
                    "plan": {"mode": "use"},
                    "overrides": {},
                    "folder": {"use": "fresh-copy"},
                    "at": {"x": 240, "y": 0}
                }
            ],
            "links": [{"from": "create", "to": "use"}]
        }),
    )
}

async fn source_run(
    bench: &Bench,
    workflow: PathBuf,
    selected_drivers: Drivers,
) -> Result<loadout_lib::commands::RunReport, Box<dyn Error>> {
    source_run_at_once(bench, workflow, selected_drivers, 1).await
}

async fn source_run_at_once(
    bench: &Bench,
    workflow: PathBuf,
    selected_drivers: Drivers,
    how_many_at_once: usize,
) -> Result<loadout_lib::commands::RunReport, Box<dyn Error>> {
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: selected_drivers,
        processes: Arc::new(Processes::new()),
        control: RunControl::new(),
    };
    let (sink, _events) = line_channel(512);
    Ok(loadout_lib::commands::run::run_workflow_inner(
        &deps,
        &RunRequest {
            workflow,
            how_many_at_once,
            task: None,
            part: None,
            handoffs_from: None,
        },
        sink,
    )
    .await?)
}

fn recorded_version(run_dir: &Path) -> Result<(u64, String), Box<dyn Error>> {
    let bytes = fs::read(run_dir.join("run.json"))?;
    let saved: Value = serde_json::from_slice(&bytes)?;
    let receipt = &saved["steps"][1]["plan_version"];
    Ok((
        receipt["version"].as_u64().ok_or_else(|| {
            format!(
                "the source consumer has no plan version: {}",
                String::from_utf8_lossy(&bytes)
            )
        })?,
        receipt["versionId"]
            .as_str()
            .ok_or("the source consumer has no plan identity")?
            .to_owned(),
    ))
}

fn required_plan_core(prompt: &str) -> Result<&str, Box<dyn Error>> {
    let after = prompt
        .split_once("## Required plan core\n")
        .ok_or("the adapter input has no required plan core")?
        .1;
    Ok(after
        .split_once("\n## End required plan core")
        .ok_or("the adapter input has no end of required plan core")?
        .0)
}

fn run_dir(bench: &Bench, run_id: &str) -> Result<PathBuf, Box<dyn Error>> {
    let source = loadout_lib::commands::lead_history::source_for(bench.project.path(), run_id)?;
    Ok(bench
        .project
        .path()
        .join(".loadout/runs")
        .join(source.run_folder))
}

fn published_version_file(run_dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let root = run_dir.join("plans/workflow-plan");
    let current: Value = serde_json::from_slice(&fs::read(root.join("current.json"))?)?;
    let name = current["file"]
        .as_str()
        .ok_or("the current plan pointer has no version file")?;
    Ok(root.join("versions").join(name))
}

fn replace_one_plan_byte(path: &Path) -> Result<Vec<u8>, Box<dyn Error>> {
    let original = fs::read(path)?;
    let mut changed = original.clone();
    let at = changed
        .windows("FROZEN".len())
        .position(|window| window == b"FROZEN")
        .ok_or("the published plan has no byte selected for the integrity test")?;
    changed[at] = b'B';
    replace_file(path, &changed)?;
    Ok(original)
}

fn replace_file(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
    let permissions = fs::metadata(path)?.permissions();
    // 2026-09-08 (WP-06): kanon jest celowo 0444, lecz proces z szerokim dostępem może usunąć
    // wpis i stworzyć nowy; test odtwarza tę właśnie drogę zamiast osłabiać prawa pliku.
    fs::remove_file(path)?;
    fs::write(path, bytes)?;
    fs::set_permissions(path, permissions)?;
    Ok(())
}

async fn repeat(
    state: &AppState,
    folder: &str,
    source_run_id: &str,
    part: Value,
    mode: &str,
) -> Result<(Value, String), Box<dyn Error>> {
    let preview = state
        .prepare_replay_inner(folder, source_run_id, part, mode)
        .await?;
    let preview_id = preview["previewId"]
        .as_str()
        .ok_or("the replay preview has no ID")?;
    let requested = state
        .authorize_replay_inner(folder, preview_id, "Start replay".to_owned())
        .await?;
    let Line::RunRequested { request_id, .. } = requested else {
        return Err("The repeat bypassed the normal Start request".into());
    };
    let (sink, _events) = line_channel(512);
    let replay = state
        .accept_lead_start_inner(&request_id, 1, Some(8.0), false, sink)
        .await?;
    Ok((preview, replay.run.run_id))
}

async fn read_plan_over_bridge(
    version: &loadout_lib::work_plan::PlanVersion,
) -> Result<String, Box<dyn Error>> {
    let socket = tempfile::tempdir()?;
    let desk = Arc::new(PlanDesk::new(
        "wp-06-bridge-reader",
        version,
        CancellationToken::new(),
    ));
    let bridge = Bridge::open_with_tools(socket.path(), desk.tools(), desk).await?;
    let (reading, mut writing) = UnixStream::connect(bridge.at()).await?.into_split();
    let mut reading = BufReader::new(reading);
    let mut greeting = String::new();
    reading.read_line(&mut greeting).await?;
    let _: Greeting = serde_json::from_str(greeting.trim())?;
    let mut asked = serde_json::to_vec(&Call {
        id: json!("wp-06-read-plan"),
        call: "read_plan".to_owned(),
        input: json!({"versionId": version.version_id}),
    })?;
    asked.push(b'\n');
    writing.write_all(&asked).await?;
    writing.flush().await?;
    let mut answered = String::new();
    reading.read_line(&mut answered).await?;
    let reply: Reply = serde_json::from_str(answered.trim())?;
    let Answer::Ok(value) = reply.answer else {
        return Err("the bridge refused its pinned plan".into());
    };
    Ok(value["text"]
        .as_str()
        .ok_or("the bridge returned no plan text")?
        .to_owned())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_recorded_replay_hands_the_consumer_the_same_plan_text() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let observed = Arc::new(Observed::default());
    let selected_drivers = drivers(Arc::clone(&observed));
    let workflow = workflow(&bench)?;
    let source = source_run(&bench, workflow.clone(), Arc::clone(&selected_drivers)).await?;
    let (version, version_id) = recorded_version(&source.dir)?;
    assert_eq!(version, 1);
    let published =
        loadout_lib::work_plan::read_current_version(&source.dir.join("plans/workflow-plan"))?;
    let version_file = published_version_file(&source.dir)?;
    let original_version_file = replace_one_plan_byte(&version_file)?;
    let bridge_text = read_plan_over_bridge(&published).await?;
    assert!(bridge_text.contains(FROZEN_REQUIREMENT), "{bridge_text}");
    assert!(
        loadout_lib::work_plan::read_current_version(&source.dir.join("plans/workflow-plan"))
            .is_err(),
        "an arbitrary file edit was accepted as the current plan"
    );
    replace_file(&version_file, &original_version_file)?;
    let diagnostics = loadout_lib::commands::diagnostics::support_report(bench.project.path())?;
    assert!(diagnostics.text().contains("\"workPlan\""));
    assert!(!diagnostics.text().contains(FROZEN_REQUIREMENT));
    let context_library = loadout_lib::context::files::library_root(bench.home.path());
    if context_library.exists() {
        fs::remove_dir_all(context_library)?;
    }
    let source_prompt = observed
        .prompts
        .lock()
        .map_err(|_| "prompt observations are poisoned")?
        .iter()
        .find(|prompt| prompt.contains("WP06-CONSUMER"))
        .cloned()
        .ok_or("the source consumer did not receive a prompt")?;
    observed
        .prompts
        .lock()
        .map_err(|_| "prompt observations are poisoned")?
        .clear();

    let mut changed: Value = serde_json::from_slice(&fs::read(&workflow)?)?;
    changed["steps"][0]["instructions"] = json!("WP06-AUTHOR-TODAY must produce a different plan.");
    fs::write(&workflow, serde_json::to_vec_pretty(&changed)?)?;

    let state = AppState::new(
        bench.home.path().to_path_buf(),
        bench.project.path().to_path_buf(),
        Store::open(&bench.db())?,
        selected_drivers,
    );
    let folder = bench
        .project
        .path()
        .to_str()
        .ok_or("the fixture project path is not text")?;
    let (_, replay_id) = repeat(
        &state,
        folder,
        &source.id,
        json!({"kind":"step", "step_id":"use"}),
        "recorded",
    )
    .await?;
    let prompts = observed
        .prompts
        .lock()
        .map_err(|_| "prompt observations are poisoned")?
        .clone();
    let replay_run = fs::read_to_string(run_dir(&bench, &replay_id)?.join("run.json"))?;
    assert_eq!(
        prompts.len(),
        1,
        "the replay did not run only the Use tile: {replay_run}"
    );
    assert!(prompts[0].contains(FROZEN_REQUIREMENT), "{}", prompts[0]);
    assert!(prompts[0].contains(&version_id), "{}", prompts[0]);
    assert_eq!(
        required_plan_core(&prompts[0])?,
        required_plan_core(&source_prompt)?
    );
    state.close_everything_down().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_changed_recorded_snapshot_is_refused_before_start_and_after_restart()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let observed = Arc::new(Observed::default());
    let selected_drivers = drivers(Arc::clone(&observed));
    let workflow = workflow(&bench)?;
    let source = source_run(&bench, workflow, Arc::clone(&selected_drivers)).await?;
    let _ = recorded_version(&source.dir)?;
    let version_file = published_version_file(&source.dir)?;
    let folder = bench
        .project
        .path()
        .to_str()
        .ok_or("the fixture project path is not text")?;
    let part = json!({"kind":"step", "step_id":"use"});
    let started_before_refusals = observed
        .prompts
        .lock()
        .map_err(|_| "prompt observations are poisoned")?
        .len();

    let first_state = AppState::new(
        bench.home.path().to_path_buf(),
        bench.project.path().to_path_buf(),
        Store::open(&bench.db())?,
        Arc::clone(&selected_drivers),
    );
    let original = replace_one_plan_byte(&version_file)?;
    let before_preview = first_state
        .prepare_replay_inner(folder, &source.id, part.clone(), "recorded")
        .await
        .expect_err("a changed package was accepted before its preview");
    assert_recorded_refusal(&before_preview);
    replace_file(&version_file, &original)?;

    let preview = first_state
        .prepare_replay_inner(folder, &source.id, part.clone(), "recorded")
        .await?;
    let preview_id = preview["previewId"]
        .as_str()
        .ok_or("the replay preview has no ID")?;
    let original = replace_one_plan_byte(&version_file)?;
    let after_preview = first_state
        .authorize_replay_inner(folder, preview_id, "Start replay".to_owned())
        .await
        .expect_err("a package changed after preview was accepted");
    assert_recorded_refusal(&after_preview);
    first_state.close_everything_down().await;

    let restarted = AppState::new(
        bench.home.path().to_path_buf(),
        bench.project.path().to_path_buf(),
        Store::open(&bench.db())?,
        selected_drivers,
    );
    let after_restart = restarted
        .prepare_replay_inner(folder, &source.id, part, "recorded")
        .await
        .expect_err("a changed package was accepted after restart");
    assert_recorded_refusal(&after_restart);
    assert_eq!(
        observed
            .prompts
            .lock()
            .map_err(|_| "prompt observations are poisoned")?
            .len(),
        started_before_refusals,
        "a process started despite the visible recorded-input refusal"
    );
    replace_file(&version_file, &original)?;
    restarted.close_everything_down().await;
    Ok(())
}

fn assert_recorded_refusal(error: &impl std::fmt::Display) {
    let said = error.to_string();
    assert!(
        said.starts_with("Recorded inputs are unavailable:"),
        "{said}"
    );
    assert!(
        said.contains("Nothing was replaced with today's files."),
        "{said}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_same_graph_repeated_in_the_other_mode_makes_a_new_version()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let observed = Arc::new(Observed::default());
    let selected_drivers = drivers(observed);
    let workflow = workflow(&bench)?;
    let source = source_run(&bench, workflow, Arc::clone(&selected_drivers)).await?;
    let (_, source_version_id) = recorded_version(&source.dir)?;
    let state = AppState::new(
        bench.home.path().to_path_buf(),
        bench.project.path().to_path_buf(),
        Store::open(&bench.db())?,
        selected_drivers,
    );
    let folder = bench
        .project
        .path()
        .to_str()
        .ok_or("the fixture project path is not text")?;

    let (recorded_preview, recorded_id) = repeat(
        &state,
        folder,
        &source.id,
        json!({"kind":"all"}),
        "recorded",
    )
    .await?;
    let (_, recorded_version_id) = recorded_version(&run_dir(&bench, &recorded_id)?)?;
    let (current_preview, current_id) =
        repeat(&state, folder, &source.id, json!({"kind":"all"}), "current").await?;
    let (_, current_version_id) = recorded_version(&run_dir(&bench, &current_id)?)?;

    let previews = format!("{recorded_preview}\n{current_preview}");
    assert!(
        previews.contains("a new result, not an identical replay"),
        "{previews}"
    );
    assert_ne!(recorded_version_id, source_version_id);
    assert_ne!(current_version_id, source_version_id);
    assert_ne!(current_version_id, recorded_version_id);
    state.close_everything_down().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_plan_consumers_really_overlap_and_keep_one_version() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let observed = Arc::new(Observed::default());
    let run = source_run_at_once(
        &bench,
        workflow_with_copies(&bench, 2)?,
        drivers(Arc::clone(&observed)),
        2,
    )
    .await?;
    assert_eq!(
        observed.most_consumers.load(Ordering::SeqCst),
        2,
        "two Use copies never overlapped in time"
    );
    let saved: Value = serde_json::from_slice(&fs::read(run.dir.join("run.json"))?)?;
    let versions = saved["steps"]
        .as_array()
        .ok_or("the run has no step receipts")?
        .iter()
        .filter(|step| {
            step["node_key"]
                .as_str()
                .is_some_and(|key| key.starts_with("use"))
        })
        .map(|step| step["plan_version"]["versionId"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(versions.len(), 2);
    assert!(versions[0].is_some());
    assert_eq!(versions[0], versions[1]);
    Ok(())
}
