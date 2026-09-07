//! 2026-09-05 (WF-01): zmiana projektu pomiędzy kopiami nie zmienia wejścia biegu.
//! Szew tylko edytuje źródło po pierwszej izolacji; cały layout i odczyt cwd są produkcyjne.

#![allow(clippy::too_many_lines)]

use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::{
    PrestartFaultInjector, PrestartFaultPoint, RolledBackResource,
    run_workflow_with_prestart_faults,
};
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::line_channel;
use loadout_lib::store::Store;
use serde_json::json;
use tokio::sync::mpsc;

const INITIAL: &str = "the captured input\n";
const LATER: &str = "the human edited the project later\n";

#[test]
fn capture_retries_a_change_but_refuses_a_source_that_never_settles() -> Result<(), Box<dyn Error>>
{
    use loadout_lib::commands::input_snapshot;
    let project = tempfile::tempdir()?;
    let run = tempfile::tempdir()?;
    fs::write(project.path().join("a"), "before")?;
    let captured = input_snapshot::capture_with_observer(project.path(), run.path(), |attempt| {
        if attempt == 0 {
            fs::write(project.path().join("a"), "after")?;
        }
        Ok(())
    })?;
    let output = tempfile::tempdir()?;
    captured.materialize(output.path())?;
    assert_eq!(fs::read_to_string(output.path().join("a"))?, "after");
    let unstable = tempfile::tempdir()?;
    let attempts = std::cell::Cell::new(0);
    let result =
        input_snapshot::capture_with_observer(project.path(), unstable.path(), |attempt| {
            attempts.set(attempts.get() + 1);
            fs::write(project.path().join("a"), format!("change {attempt}"))
        });
    assert!(result.is_err());
    assert_eq!(
        attempts.get(),
        3,
        "initial capture plus at most two retries"
    );
    assert!(!unstable.path().join("input").exists());
    Ok(())
}

#[test]
fn saved_input_is_read_without_sqlite_and_rejects_changed_bytes_and_paths()
-> Result<(), Box<dyn Error>> {
    use loadout_lib::commands::input_snapshot;
    let project = tempfile::tempdir()?;
    let run = tempfile::tempdir()?;
    fs::write(project.path().join("a"), "before")?;
    let captured = input_snapshot::capture(project.path(), run.path())?;
    let reloaded = input_snapshot::read(run.path())?;
    assert_eq!(reloaded.id(), captured.id());
    fs::write(run.path().join("input/files/a"), "forged")?;
    assert!(
        input_snapshot::read(run.path()).is_err(),
        "same-length replacement is not authentic input"
    );
    fs::write(run.path().join("input/files/a"), "before")?;
    let manifest_path = run.path().join("input/manifest.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    let entry = manifest["entries"]["a"].clone();
    manifest["entries"]
        .as_object_mut()
        .ok_or("no entries")?
        .insert("../outside".into(), entry);
    fs::write(manifest_path, serde_json::to_vec(&manifest)?)?;
    assert!(
        input_snapshot::read(run.path()).is_err(),
        "a manifest cannot name a path outside its input"
    );
    Ok(())
}

struct ChangeSource(PathBuf);

impl PrestartFaultInjector for ChangeSource {
    fn check(&self, point: PrestartFaultPoint) -> io::Result<()> {
        if point == PrestartFaultPoint::AfterFirstIsolation {
            fs::write(self.0.join("input.txt"), LATER)?;
        }
        Ok(())
    }
    fn rolled_back(&self, _: &RolledBackResource) {}
}

#[tokio::test]
async fn git_wip_is_captured_once_for_all_copies() -> Result<(), Box<dyn Error>> {
    copies_keep_one_input(true, 0).await
}

#[tokio::test]
async fn a_plain_folder_is_captured_once_for_all_copies() -> Result<(), Box<dyn Error>> {
    copies_keep_one_input(false, 0).await
}

#[tokio::test]
async fn an_unchanged_single_result_can_resume_after_the_host_changes() -> Result<(), Box<dyn Error>>
{
    copies_keep_one_input(true, 1).await
}

#[tokio::test]
async fn resuming_two_copies_keeps_their_different_saved_results() -> Result<(), Box<dyn Error>> {
    copies_keep_one_input(true, 2).await
}

#[tokio::test]
async fn a_plain_unchanged_result_resumes_without_reading_the_new_host()
-> Result<(), Box<dyn Error>> {
    copies_keep_one_input(false, 1).await
}

#[tokio::test]
async fn two_plain_results_resume_with_their_own_files() -> Result<(), Box<dyn Error>> {
    copies_keep_one_input(false, 2).await
}

async fn copies_keep_one_input(use_git: bool, resume_copies: usize) -> Result<(), Box<dyn Error>> {
    let resume = resume_copies > 0;
    let copies = if resume { resume_copies } else { 2 };
    let root = tempfile::tempdir()?;
    let home = root.path().join("home");
    let project = root.path().join("project");
    fs::create_dir_all(home.join("agents"))?;
    fs::create_dir_all(home.join("workflows"))?;
    fs::create_dir_all(project.join(".loadout"))?;
    fs::write(
        project.join("input.txt"),
        if resume { INITIAL } else { "committed\n" },
    )?;
    if use_git {
        git(&project, &["init", "--quiet"])?;
        git(&project, &["add", "input.txt"])?;
        git(&project, &["commit", "--quiet", "-m", "baseline"])?;
    }
    fs::write(project.join("input.txt"), INITIAL)?;
    fs::write(
        home.join("agents/reader.md"),
        "---\nschema: 1\nid: 01990000-0000-7000-8000-0000000000a1\nname: Reader\nsummary: Reads its actual input\ncolor: slate\nrunsWith: claude-code\nmodel: opus\nthinking: balanced\nfileAccess: work-freely\ngiveUpAfterMinutes: 20\nwriteResultsTo: \"\"\ntools: everything\nskills: []\nconnections: []\n---\nRead the input.\n",
    )?;
    let workflow = home.join("workflows/input.json");
    fs::write(
        &workflow,
        serde_json::to_vec(&json!({
            "format": 1, "id": "wf_input", "name": "Shared input", "links": [],
            "steps": [{"kind": "agent", "id": "reader", "name": "Read input",
                "agent": "01990000-0000-7000-8000-0000000000a1", "overrides": {},
                "copies": copies, "instructions": "read input", "folder": {"use": "fresh-copy"},
                "at": {"x": 0, "y": 0}}]
        }))?,
    )?;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let driver: Arc<dyn AgentDriver> = Arc::new(Reader {
        project: project.clone(),
        seen: Arc::clone(&seen),
        writes_for_each_copy: resume_copies > 1,
    });
    let drivers: Drivers = Arc::new(move |_| Arc::clone(&driver));
    let store = Store::open(&project.join(".loadout/loadout.db"))?;
    let deps = RunDeps {
        home: &home,
        project: &project,
        store: &store,
        drivers,
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let mut request = RunRequest {
        workflow,
        how_many_at_once: 2,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (sink, _source) = line_channel(4096);
    let report = tokio::time::timeout(
        Duration::from_secs(30),
        run_workflow_with_prestart_faults(
            &deps,
            &request,
            sink,
            Arc::new(ChangeSource(project.clone())),
        ),
    )
    .await??;
    assert_eq!(fs::read_to_string(project.join("input.txt"))?, LATER);
    if resume {
        if resume_copies > 1 {
            fs::write(project.join("input.txt"), INITIAL)?;
        }
        request.handoffs_from = Some(report.dir.clone());
        let (sink, _source) = line_channel(4096);
        let resumed = loadout_lib::commands::run::run_workflow_inner(&deps, &request, sink).await;
        assert!(
            resumed.is_ok(),
            "a recorded unchanged result is not a missing result: {resumed:?}"
        );
    }
    let actual = seen
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(
        actual.len(),
        copies * if resume { 2 } else { 1 },
        "both real copy paths must reach the driver"
    );
    assert!(
        actual.iter().take(copies).all(|text| text == INITIAL),
        "copies saw different inputs: {actual:?}"
    );
    if resume_copies > 1 {
        let results: std::collections::BTreeSet<_> = actual.iter().skip(copies).collect();
        assert_eq!(
            results.len(),
            copies,
            "each resumed copy must keep its own earlier changes"
        );
        assert!(
            results.iter().all(|text| text.contains("saved for")),
            "resuming replaced earlier results with input: {actual:?}"
        );
    } else if resume {
        assert!(actual.iter().all(|text| text == INITIAL));
    }
    Ok(())
}

struct Reader {
    project: PathBuf,
    seen: Arc<Mutex<Vec<String>>>,
    writes_for_each_copy: bool,
}

#[async_trait]
impl AgentDriver for Reader {
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
        if spec.cwd != self.project {
            self.seen
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(fs::read_to_string(spec.cwd.join("input.txt"))?);
            if self.writes_for_each_copy
                && fs::read_to_string(spec.cwd.join("input.txt"))? == INITIAL
            {
                let key = spec
                    .cwd
                    .file_name()
                    .ok_or_else(|| anyhow::anyhow!("missing copy key"))?;
                fs::write(
                    spec.cwd.join("input.txt"),
                    format!("{INITIAL}saved for {}\n", key.to_string_lossy()),
                )?;
            }
        }
        Ok(Box::new(Turn {
            events,
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
        let outcome = Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "Read the input.".into(),
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

fn git(project: &Path, args: &[&str]) -> Result<(), Box<dyn Error>> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(project)
        .args([
            "-c",
            "user.name=Loadout test",
            "-c",
            "user.email=test@loadout.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()?;
    assert!(
        output.status.success(),
        "git fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

/// Plik, który człowiek sam ZACOMMITOWAŁ, nie jest jego sekretem — a filtr prywatności
/// kasował go z każdej świeżej kopii.
///
/// # Co się działo
///
/// `.env.example` jest jednym z najpospolitszych plików w repozytoriach: szablon konfiguracji,
/// celowo opublikowany przez autora. Filtr, który ma nie wynosić `.env` z sekretami, wycinał go
/// razem z resztą. Kopia kroku jest drzewem roboczym odbitym od `HEAD`, więc brak śledzonego
/// pliku czytał się w niej jako USUNIĘCIE — i bieg commitował to usunięcie na swoją gałąź.
/// Filtr prywatności kasował więc człowiekowi śledzony plik, po cichu i w każdym biegu.
///
/// # Czego to kryterium NIE osłabia
///
/// Prawdziwy `.env` z sekretami nie jest w gicie śledzony — po to stoi w `.gitignore` — więc
/// dalej nie wchodzi do kopii. Sądzimy tu obie strony naraz: śledzony szablon WCHODZI,
/// nieśledzony sekret NIE WCHODZI.
#[test]
fn a_committed_env_template_survives_the_copy_and_a_real_secret_does_not()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = home.path().join("project");
    let run_dir = home.path().join("run");
    fs::create_dir_all(&project)?;
    fs::create_dir_all(&run_dir)?;

    let git = |args: &[&str]| -> Result<(), Box<dyn Error>> {
        let done = std::process::Command::new("git")
            .args(args)
            .current_dir(&project)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()?;
        assert!(done.status.success(), "git {args:?}: {done:?}");
        Ok(())
    };
    git(&["init", "--quiet"])?;
    fs::write(project.join(".env.example"), "API_KEY=put-yours-here\n")?;
    fs::write(project.join("main.rs"), "fn main() {}\n")?;
    fs::write(project.join(".gitignore"), ".env\n")?;
    git(&["add", "."])?;
    git(&["commit", "--quiet", "-m", "first"])?;
    // Prawdziwy sekret: NIEŚLEDZONY, bo tak trzyma go każdy.
    fs::write(project.join(".env"), "API_KEY=this-is-real\n")?;

    let captured =
        loadout_lib::commands::input_snapshot::capture_selected(&project, &run_dir, &[])?;
    let inside: Vec<&Path> = captured.entries().keys().map(PathBuf::as_path).collect();

    assert!(
        inside.contains(&Path::new(".env.example")),
        "a committed .env.example was cut out of the run's input. The copy is a work tree taken \
         from HEAD, so a tracked file missing from it reads as a DELETION — and the run commits \
         that deletion to its own branch. The privacy filter would be deleting the person's own \
         file. Files in the snapshot: {inside:?}"
    );
    assert!(
        !inside.contains(&Path::new(".env")),
        "an untracked .env reached the run's input. That is the file this filter exists for: it \
         holds the person's real secrets and must never travel into a step's copy. \
         Files in the snapshot: {inside:?}"
    );
    Ok(())
}
