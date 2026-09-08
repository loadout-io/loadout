//! Z-41: opublikowane przekazanie nie jest edytowalnym katalogiem współdzielonym między krokami.
//!
//! Dubler poznaje krok po katalogu roboczym, nigdy po prompcie, który jest częścią sądzonej
//! drogi. `Editor` najpierw próbuje zwykłego zapisu, a potem odtwarza realny atak: kasuje plik
//! przez zapisywalny katalog i kładzie pod tą samą nazwą inne bajty.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, Part, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::line::LineKind;
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{LineSource, QUEUE_CAP, line_channel};
use loadout_lib::memory::handoff::{self, BODY_CAP, MetaDraft};
use loadout_lib::store::Store;
use tempfile::TempDir;
use tokio::sync::mpsc;

const VENDOR: &str = "fake";
const SCOUT: &str = "s_scout";
const EDITOR: &str = "s_editor";
const READER_NAME: &str = "Reader";
const PATIENCE: Duration = Duration::from_secs(20);
const TURN: Duration = Duration::from_millis(20);

const AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-000000000411
name: Worker
summary: Exercises the handoff boundary
color: slate
runsWith: claude-code
model: sonnet
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 20
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Do the named step.
";

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_read_only_handoff",
  "name": "Read-only handoff",
  "steps": [
    {
      "kind": "agent",
      "id": "s_scout",
      "name": "Scout",
      "agent": "01990000-0000-7000-8000-000000000411",
      "overrides": {},
      "instructions": "Publish findings.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_editor",
      "name": "Editor",
      "agent": "01990000-0000-7000-8000-000000000411",
      "overrides": {},
      "instructions": "Work after Scout.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_reader",
      "name": "Reader",
      "agent": "01990000-0000-7000-8000-000000000411",
      "overrides": {},
      "instructions": "Read both results.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 480, "y": 0 }
    }
  ],
  "links": [
    { "from": "s_scout", "to": "s_editor" },
    { "from": "s_scout", "to": "s_reader" },
    { "from": "s_editor", "to": "s_reader" }
  ]
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_changed_handoff_stops_the_step_that_reads_it() -> Result<(), Box<dyn Error>> {
    assert_changed_run(Tamper::ChangeBody).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_truncated_handoff_stops_the_step_that_reads_it() -> Result<(), Box<dyn Error>> {
    assert_changed_run(Tamper::TruncateBody).await
}

/// ZNIKNIETY PLIK TO NIE ZMIENIONY PLIK (2026-09-05, przy landowaniu Z-41).
///
/// Ten test zadal pierwotnie zdania „Handoff … was changed after Scout published it." takze dla
/// USUNIECIA — a to nieprawda o tym, co sie stalo, i klocilo sie z T-101
/// (`context_failures_take_the_chosen_path`), gdzie sabotazysta usuwa wynik poprzednika i karta
/// mowi zdanie ogolne. Sila asercji zostaje ta sama: krok MA padac przed startem sterownika,
/// zdanie MA stac na ekranie jako `Problem` pod Readerem i MA wrocic z `read_run_inner`.
/// Zmienia sie wylacznie to, ktore zdanie jest prawdziwe.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_missing_handoff_stops_the_step_that_reads_it() -> Result<(), Box<dyn Error>> {
    assert_refused_run(Tamper::RemoveHandoff, CONTEXT_NOT_PROVEN).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_changed_full_attachment_stops_the_step_that_reads_it() -> Result<(), Box<dyn Error>> {
    assert_changed_run(Tamper::ChangeAttachment).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn superseding_a_handoff_is_not_reported_as_a_change() -> Result<(), Box<dyn Error>> {
    let (report, mut lines, _seen, _bench) = run_with(Tamper::Supersede).await?;
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; 3],
        "a legal correction stopped the Reader: {:?}",
        report.steps
    );
    while let Some(line) = lines.try_next() {
        assert!(
            !line.text().contains("was changed after"),
            "supersede produced a change refusal on screen: {:?}",
            line.text()
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_untouched_handoff_from_an_earlier_run_starts_normally() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let (first, _lines, _seen) = run_on(&bench, Tamper::None, None, None).await?;
    assert_eq!(
        first.steps,
        vec![StepState::Succeeded; 3],
        "the source run did not finish: {:?}",
        first.steps
    );

    let (resumed, mut lines, _seen) = run_on(
        &bench,
        Tamper::None,
        Some(Part::Just(vec!["s_reader".to_owned()])),
        Some(first.dir),
    )
    .await?;
    assert_eq!(
        resumed.steps,
        vec![StepState::Succeeded],
        "an untouched carried handoff stopped the resumed Reader: {:?}",
        resumed.steps
    );
    while let Some(line) = lines.try_next() {
        assert!(
            !line.text().contains("was changed after"),
            "the resumed run produced a false change refusal: {:?}",
            line.text()
        );
    }
    Ok(())
}

const CONTEXT_NOT_PROVEN: &str =
    "Loadout could not prove the context files for this agent, so it did not start the step.";

async fn assert_changed_run(tamper: Tamper) -> Result<(), Box<dyn Error>> {
    let (report, mut lines, seen, bench) = run_with(tamper).await?;
    let changed = seen.changed_sentence()?;
    judge_refusal(&report, &mut lines, &bench, &changed)
}

/// Ten sam sedzia, ale zdanie podaje wolajacy — bo nie kazda awaria kontekstu jest zmiana.
async fn assert_refused_run(tamper: Tamper, sentence: &str) -> Result<(), Box<dyn Error>> {
    let (report, mut lines, _seen, bench) = run_with(tamper).await?;
    judge_refusal(&report, &mut lines, &bench, sentence)
}

fn judge_refusal(
    report: &RunReport,
    lines: &mut LineSource,
    bench: &Bench,
    sentence: &str,
) -> Result<(), Box<dyn Error>> {
    assert_eq!(
        report.steps,
        vec![
            StepState::Succeeded,
            StepState::Succeeded,
            StepState::Failed
        ],
        "Reader has to fail before its driver starts; the run ended as {:?}",
        report.steps
    );

    let mut visible = false;
    // 2026-09-08 — komunikat POKAZUJE, co naprawde stanelo pod Readerem. Gole „nie bylo takiej
    // linii" kazalo zgadywac, czy zdanie sie zmienilo, czy linia w ogole nie powstala.
    let mut problems = Vec::new();
    while let Some(line) = lines.try_next() {
        if line.kind() == LineKind::Problem && line.agent() == READER_NAME {
            problems.push(line.text().to_owned());
            if line.text() == sentence {
                visible = true;
            }
        }
    }
    assert!(
        visible,
        "the tampered handoff was not a Problem line under Reader with the sentence {sentence:?}; \
         the Problem lines under Reader were: {problems:?}"
    );

    let folder = report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the run directory has no name")?;
    let history = read_run_inner(bench.project.path(), folder)?;
    let reader = history
        .steps
        .iter()
        .find(|step| step.name == READER_NAME)
        .ok_or("Reader is absent from the run history")?;
    assert!(
        reader.error.contains(sentence),
        "read_run_inner did not carry the visible refusal into Reader.error: {:?}",
        reader.error
    );
    Ok(())
}

async fn run_with(
    tamper: Tamper,
) -> Result<(RunReport, LineSource, Arc<Seen>, Bench), Box<dyn Error>> {
    let bench = Bench::new()?;
    let (report, lines, seen) = run_on(&bench, tamper, None, None).await?;
    Ok((report, lines, seen, bench))
}

async fn run_on(
    bench: &Bench,
    tamper: Tamper,
    part: Option<Part>,
    handoffs_from: Option<PathBuf>,
) -> Result<(RunReport, LineSource, Arc<Seen>), Box<dyn Error>> {
    let workflow = bench.workflow()?;
    let store = Store::open(&bench.db())?;
    let seen = Arc::new(Seen::default());
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(tamper, Arc::clone(&seen)),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
        task: None,
        part,
        handoffs_from,
    };
    let (sink, source) = line_channel(QUEUE_CAP);
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| format!("the run did not finish within {PATIENCE:?}"))??;
    Ok((report, source, seen))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tamper {
    ChangeBody,
    TruncateBody,
    RemoveHandoff,
    ChangeAttachment,
    Supersede,
    None,
}

#[derive(Debug, Default)]
struct Seen {
    changed_name: Mutex<Option<String>>,
}

impl Seen {
    fn mark_changed(&self, path: &Path) {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        *self
            .changed_name
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = name;
    }

    fn changed_sentence(&self) -> Result<String, Box<dyn Error>> {
        let name = self
            .changed_name
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or("the tampering driver did not record the handoff name")?;
        Ok(format!(
            "Handoff {name} was changed after Scout published it."
        ))
    }
}

fn fake_drivers(tamper: Tamper, seen: Arc<Seen>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { tamper, seen });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

fn step_of(cwd: &Path) -> &str {
    cwd.file_name().and_then(|name| name.to_str()).unwrap_or("")
}

fn only_handoff(run_dir: &Path) -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
    let mut paths: Vec<PathBuf> = fs::read_dir(run_dir.join("handoffs"))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    paths.sort();
    match paths.as_slice() {
        [only] => Ok(only.clone()),
        _ => Err(format!("Editor expected one handoff and found {paths:?}").into()),
    }
}

fn only_attachment(run_dir: &Path) -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
    let mut paths: Vec<PathBuf> = fs::read_dir(run_dir.join("attachments"))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    paths.sort();
    match paths.as_slice() {
        [only] => Ok(only.clone()),
        _ => Err(format!("Editor expected one full attachment and found {paths:?}").into()),
    }
}

fn assert_read_only(path: &Path) -> anyhow::Result<()> {
    let mode = fs::metadata(path)?.permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o444,
        "a published handoff output must be 0444 before a later step receives its directory"
    );
    let denied = OpenOptions::new()
        .append(true)
        .open(path)
        .expect_err("a later step could append to a published handoff output");
    assert_eq!(
        denied.kind(),
        ErrorKind::PermissionDenied,
        "the write refusal was not EACCES: {denied}"
    );
    Ok(())
}

fn replace_file(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    // 2026-09 — `0444` chroni deskryptor, ale prawo do katalogu nadal pozwala podmienić nazwę;
    // właśnie ten przypadek ma zatrzymać kontrola przed uruchomieniem czytelnika.
    fs::remove_file(path)?;
    fs::write(path, bytes)?;
    Ok(())
}

fn reply_of(step: &str, tamper: Tamper) -> String {
    if step == SCOUT && tamper == Tamper::ChangeAttachment {
        return format!(
            "## Answer\n{}\n\n## Evidence\nObserved.\n\n## Open questions\nNone.\n",
            "full finding ".repeat(BODY_CAP / 8)
        );
    }
    if step == SCOUT {
        "## Answer\nScout findings.\n\n## Evidence\nObserved.\n\n## Open questions\nNone.\n"
            .to_owned()
    } else {
        "## Answer\nWork complete.\n\n## Evidence\nObserved.\n\n## Open questions\nNone.\n"
            .to_owned()
    }
}

#[derive(Debug)]
struct Fake {
    tamper: Tamper,
    seen: Arc<Seen>,
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        VENDOR
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some(VENDOR.to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let step = step_of(&spec.cwd);
        if step == EDITOR {
            let run_dir = spec
                .cwd
                .parent()
                .and_then(Path::parent)
                .ok_or_else(|| anyhow::anyhow!("the step cwd is not inside a run"))?;
            let handoff = only_handoff(run_dir).map_err(|error| anyhow::anyhow!(error))?;
            assert_read_only(&handoff)?;

            match self.tamper {
                Tamper::ChangeBody => {
                    self.seen.mark_changed(&handoff);
                    let mut changed = fs::read(&handoff)?;
                    changed.extend_from_slice(b"changed after publication\n");
                    replace_file(&handoff, &changed)?;
                }
                Tamper::TruncateBody => {
                    self.seen.mark_changed(&handoff);
                    let mut changed = fs::read(&handoff)?;
                    let _ = changed.pop();
                    replace_file(&handoff, &changed)?;
                }
                Tamper::RemoveHandoff => {
                    self.seen.mark_changed(&handoff);
                    fs::remove_file(&handoff)?;
                }
                Tamper::ChangeAttachment => {
                    self.seen.mark_changed(&handoff);
                    let attachment =
                        only_attachment(run_dir).map_err(|error| anyhow::anyhow!(error))?;
                    assert_read_only(&attachment)?;
                    let mut changed = fs::read(&attachment)?;
                    changed.extend_from_slice(b"changed after publication\n");
                    replace_file(&attachment, &changed)?;
                }
                Tamper::Supersede => {
                    let old = handoff::read_handoff(&handoff)?;
                    let draft = MetaDraft {
                        run: old.meta.run.clone(),
                        step: old.meta.step,
                        from: old.meta.from.clone(),
                        to: old.meta.to.clone(),
                        kind: old.meta.kind.clone(),
                        title: old.meta.title.clone(),
                        reads: old.meta.reads.clone(),
                    };
                    let _ = handoff::supersede(
                        run_dir,
                        &old.meta.id,
                        draft,
                        "## Answer\nCorrected.\n\n## Evidence\nObserved.\n\n## Open questions\nNone.\n",
                    )?;
                }
                Tamper::None => {}
            }
        }

        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };
        let reply = reply_of(step, self.tamper);
        let _ = events
            .send(
                AgentEvent::Started {
                    session: session.clone(),
                    model: spec.model.clone().unwrap_or_default(),
                    tools: Vec::new(),
                    capabilities: Vec::new(),
                }
                .into(),
            )
            .await;
        let _ = events
            .send(
                AgentEvent::Said {
                    text: reply.clone(),
                }
                .into(),
            )
            .await;
        Ok(Box::new(Turn {
            events,
            session,
            reply,
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    reply: String,
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
        tokio::time::sleep(TURN).await;
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: self.reply.clone(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: TURN,
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

#[derive(Debug)]
struct Bench {
    home: TempDir,
    project: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(home.path().join("agents/worker.md"), AGENT)?;
        Ok(Self { home, project })
    }

    fn workflow(&self) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.home.path().join("workflows/read-only-handoff.json");
        fs::write(&path, WORKFLOW)?;
        Ok(path)
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout/loadout.db")
    }
}
