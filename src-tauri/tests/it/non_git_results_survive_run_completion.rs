//! WF-06: jedyna kopia wyników zwykłego folderu przeżywa koniec biegu i retencję.
//! Test nie imituje finalizera: fake agent pisze po rzeczywistym RunSpec.cwd runnera.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::history::{
    forget_run_inner, forget_run_with_results_inner, read_run_inner, result_folder_inner,
};
use loadout_lib::commands::reconcile::{Keep, reconcile_runs, reconcile_runs_keeping};
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::mpsc;

const KEY: &str = "writer";
const PATIENCE: Duration = Duration::from_secs(30);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_complete_non_git_result_restores_exactly_and_refuses_occupied_or_changed_sources()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let report = bench.go(Edit::Changed, Finish::Done).await?;
    let original = fs::read(report.dir.join("run.json"))?;
    let description: Value = serde_json::from_slice(&original)?;
    let input = json!({"source_run_id":description["id"],"result_id":KEY});
    let material =
        loadout_lib::commands::result_restore::prepare(bench.project.path(), &input).await?;
    let restored = material.restore().await?;
    let folder = Path::new(
        restored["folder"]
            .as_str()
            .ok_or("restored folder missing")?,
    );
    assert_changed(folder)?;
    assert_eq!(
        loadout_lib::commands::result_restore::restored_folder(bench.project.path(), folder)?,
        folder
    );
    let moved = folder.with_file_name("the-original-restored-folder");
    fs::rename(folder, &moved)?;
    fs::create_dir(folder)?;
    let receipt = folder.join(".loadout-restored.json");
    fs::copy(moved.join(".loadout-restored.json"), &receipt)?;
    assert!(
        loadout_lib::commands::result_restore::restored_folder(bench.project.path(), folder)
            .is_err(),
        "a different directory with a copied receipt was treated as Loadout's owned result"
    );
    bench.assert_project_untouched()?;
    assert_eq!(fs::read(report.dir.join("run.json"))?, original);
    for symlink in [false, true] {
        let material =
            loadout_lib::commands::result_restore::prepare(bench.project.path(), &input).await?;
        let other = tempfile::tempdir()?;
        if symlink {
            fs::write(other.path().join("mine"), "do not overwrite")?;
            loadout_lib::engine::supervisor::link(other.path(), &material.destination)?;
        } else {
            fs::create_dir(&material.destination)?;
            fs::write(material.destination.join("mine"), "do not overwrite")?;
        }
        let destination = material.destination.clone();
        assert!(
            material.restore().await.is_err(),
            "an occupied destination was reused"
        );
        assert_eq!(
            fs::read_to_string(destination.join("mine"))?,
            "do not overwrite"
        );
    }
    let material =
        loadout_lib::commands::result_restore::prepare(bench.project.path(), &input).await?;
    fs::write(
        bench.last_cwd()?.join("modified.txt"),
        "different source bytes",
    )?;
    assert!(
        material.restore().await.is_err(),
        "changed source files were called the saved result"
    );
    assert!(
        loadout_lib::commands::result_restore::prepare(bench.project.path(), &input)
            .await
            .is_err()
    );
    bench.assert_project_untouched()?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn completed_and_failed_steps_keep_all_file_operations_in_their_result_folder()
-> Result<(), Box<dyn Error>> {
    for finish in [Finish::Done, Finish::Failed, Finish::Stopped] {
        let bench = Bench::new()?;
        let report = bench.go(Edit::Changed, finish).await?;
        let result = bench.last_cwd()?;
        assert_changed(&result)?;
        bench.assert_project_untouched()?;
        let history =
            serde_json::to_value(read_run_inner(bench.project.path(), &run_name(&report)?)?)?;
        assert!(
            history
                .get("resultFolders")
                .and_then(Value::as_array)
                .is_some_and(|folders| folders
                    .iter()
                    .any(|one| one.get("path").and_then(Value::as_str) == result.to_str())),
            "the production history did not expose the retained result folder"
        );
        let described: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
        assert_eq!(described["copy_results"][KEY]["kind"], "folder");
        assert!(described["copy_results"][KEY]["digest"].as_str().is_some());
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_unreadable_origin_preserves_even_an_apparently_unchanged_copy()
-> Result<(), Box<dyn Error>> {
    for edit in [Edit::MissingOrigin, Edit::CorruptOrigin] {
        let bench = Bench::new()?;
        let report = bench.go(edit, Finish::Done).await?;
        let result = bench.last_cwd()?;
        assert!(
            result.is_dir(),
            "uncertainty deleted the only copy instead of retaining it"
        );
        assert_eq!(fs::read_to_string(result.join("modified.txt"))?, "original");
        let history = read_run_inner(bench.project.path(), &run_name(&report)?)?;
        assert!(history.steps[0].error.contains("kept"));
        let described: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
        assert_eq!(described["copy_results"][KEY]["kind"], "folder");
        assert!(
            described["copy_results"][KEY]["digest"].is_null(),
            "an uncertain copy must not claim a verified reusable digest"
        );
        bench.assert_project_untouched()?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ordinary_retention_keeps_the_only_result_while_removing_an_old_unchanged_run()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let changed = bench.go(Edit::Changed, Finish::Done).await?;
    let result = bench.last_cwd()?;
    let unchanged = bench.go(Edit::Nothing, Finish::Done).await?;
    let unchanged_copy = bench.last_cwd()?;
    let newest = bench.go(Edit::Nothing, Finish::Done).await?;
    let _recovered = reconcile_runs(bench.project.path());
    let _trimmed = reconcile_runs_keeping(bench.project.path(), &Keep::last_runs(1));
    assert_changed(&result)?;
    assert!(
        changed.dir.join("run.json").is_file(),
        "retention removed the record protecting the only saved result"
    );
    assert!(
        !unchanged_copy.exists(),
        "a proven unchanged private copy can be removed"
    );
    assert!(
        !unchanged.dir.exists(),
        "retention should still remove ordinary older runs"
    );
    assert!(newest.dir.exists());
    bench.assert_project_untouched()?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn forgetting_results_requires_the_exact_current_folder_list() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let report = bench.go(Edit::Changed, Finish::Done).await?;
    let result = bench.last_cwd()?;
    let name = run_name(&report)?;
    assert_eq!(
        result_folder_inner(bench.project.path(), &name, KEY)?,
        result
    );
    assert!(result_folder_inner(bench.project.path(), &name, "../writer").is_err());
    assert!(result_folder_inner(bench.project.path(), "../outside", KEY).is_err());
    let error = forget_run_inner(bench.project.path(), &name)
        .err()
        .ok_or("ordinary Forget deleted the only result")?;
    assert!(
        error
            .to_string()
            .contains(result.to_str().ok_or("result path is not text")?)
    );
    for wrong in [
        Vec::new(),
        vec![bench.project.path().to_owned()],
        vec![result.clone(), result.clone()],
    ] {
        assert!(forget_run_with_results_inner(bench.project.path(), &name, Some(&wrong)).is_err());
        assert_changed(&result)?;
    }
    // A newly retained copy invalidates the paths shown before the confirmation.
    let another = report.dir.join("work/another");
    fs::create_dir(&another)?;
    fs::write(another.join("only-result"), "keep me")?;
    assert!(
        forget_run_with_results_inner(
            bench.project.path(),
            &name,
            Some(std::slice::from_ref(&result))
        )
        .is_err()
    );
    assert_changed(&result)?;
    let confirmed = [another, result];
    forget_run_with_results_inner(bench.project.path(), &name, Some(&confirmed))?;
    assert!(!report.dir.exists());
    bench.assert_project_untouched()?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_folder_confirmation_cannot_bypass_a_live_run_or_pending_finalization()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let report = bench.go(Edit::Changed, Finish::Done).await?;
    let result = bench.last_cwd()?;
    let name = run_name(&report)?;
    let record = report.dir.join("run.json");
    let original: Value = serde_json::from_slice(&fs::read(&record)?)?;
    for status in ["running", "paused"] {
        let mut running = original.clone();
        running["status"] = json!(status);
        fs::write(&record, serde_json::to_vec(&running)?)?;
        assert!(
            forget_run_with_results_inner(
                bench.project.path(),
                &name,
                Some(std::slice::from_ref(&result))
            )
            .is_err()
        );
        assert_changed(&result)?;
    }
    for pending in [json!([KEY]), json!("unreadable")] {
        let mut waiting = original.clone();
        waiting["pending_finalization"] = pending;
        fs::write(&record, serde_json::to_vec(&waiting)?)?;
        assert!(
            forget_run_with_results_inner(
                bench.project.path(),
                &name,
                Some(std::slice::from_ref(&result))
            )
            .is_err()
        );
        assert_changed(&result)?;
    }
    fs::write(&record, serde_json::to_vec(&original)?)?;
    fs::create_dir_all(report.dir.join("services"))?;
    fs::write(
        report.dir.join("services/unknown.json"),
        "unreadable service state",
    )?;
    assert!(
        forget_run_with_results_inner(
            bench.project.path(),
            &name,
            Some(std::slice::from_ref(&result))
        )
        .is_err()
    );
    assert_changed(&result)?;
    bench.assert_project_untouched()?;
    Ok(())
}

fn assert_changed(result: &Path) -> Result<(), Box<dyn Error>> {
    assert!(
        result.is_dir(),
        "run completion deleted the only non-git result: {}",
        result.display()
    );
    assert_eq!(fs::read_to_string(result.join("modified.txt"))?, "changed");
    assert!(!result.join("deleted.txt").exists());
    assert_eq!(fs::read(result.join("added.bin"))?, vec![0, 255, 1, 128]);
    assert_eq!(
        fs::read_link(result.join("link"))?,
        PathBuf::from("modified.txt")
    );
    assert!(loadout_lib::engine::supervisor::executable_bits(
        &fs::metadata(result.join("tool.sh"))?
    ));
    Ok(())
}

fn run_name(report: &RunReport) -> Result<String, Box<dyn Error>> {
    report
        .dir
        .file_name()
        .and_then(|one| one.to_str())
        .map(str::to_owned)
        .ok_or_else(|| "run name missing".into())
}

#[derive(Clone, Copy)]
enum Finish {
    Done,
    Failed,
    Stopped,
}

#[derive(Clone, Copy)]
enum Edit {
    Changed,
    Nothing,
    MissingOrigin,
    CorruptOrigin,
}

struct Bench {
    home: TempDir,
    project: TempDir,
    workflow: PathBuf,
    cwd: Arc<Mutex<Option<PathBuf>>>,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(project.path().join("modified.txt"), "original")?;
        fs::write(project.path().join("deleted.txt"), "keep in original")?;
        fs::write(project.path().join("tool.sh"), "#!/bin/sh\nexit 0\n")?;
        let agent = "01990000-0000-7000-8000-0000000000a6";
        fs::write(
            home.path().join("agents/scribe.md"),
            format!(
                "---\nschema: 1\nid: {agent}\nname: Scribe\nsummary: Writes files\ncolor: slate\nrunsWith: claude-code\nmodel: opus\nthinking: balanced\nfileAccess: work-freely\ngiveUpAfterMinutes: 20\nwriteResultsTo: \"\"\ntools: everything\nskills: []\nconnections: []\n---\nDo the work.\n"
            ),
        )?;
        let workflow = home.path().join("workflows/non-git.json");
        fs::write(
            &workflow,
            serde_json::to_vec(&json!({
                "format":1, "id":"wf06_non_git", "name":"Keep the only result", "steps":[{
                    "kind":"agent", "id":KEY, "name":"Write the result", "agent":agent,
                    "instructions":"WF06 write the requested files", "folder":{"use":"fresh-copy"}, "at":{"x":0,"y":0}
                }], "links":[]
            }))?,
        )?;
        Ok(Self {
            home,
            project,
            workflow,
            cwd: Arc::new(Mutex::new(None)),
        })
    }

    async fn go(&self, edit: Edit, finish: Finish) -> Result<RunReport, Box<dyn Error>> {
        let control = RunControl::new();
        let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
            edit,
            finish,
            control: control.clone(),
            cwd: Arc::clone(&self.cwd),
        });
        let drivers: Drivers = Arc::new(move |_| Arc::clone(&driver));
        let store = Store::open(&self.project.path().join(".loadout/loadout.db"))?;
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store: &store,
            drivers,
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control,
        };
        let request = RunRequest {
            workflow: self.workflow.clone(),
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        };
        let (sink, source) = line_channel(QUEUE_CAP);
        let pump = spawn_pump(source, tauri::ipc::Channel::new(|_| Ok(())));
        let report =
            tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink)).await??;
        tokio::time::timeout(PATIENCE, pump).await??;
        Ok(report)
    }

    fn last_cwd(&self) -> Result<PathBuf, Box<dyn Error>> {
        self.cwd
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(|| "the actual driver never started".into())
    }

    fn assert_project_untouched(&self) -> Result<(), Box<dyn Error>> {
        assert_eq!(
            fs::read_to_string(self.project.path().join("modified.txt"))?,
            "original"
        );
        assert_eq!(
            fs::read_to_string(self.project.path().join("deleted.txt"))?,
            "keep in original"
        );
        assert!(!self.project.path().join("added.bin").exists());
        assert!(!self.project.path().join("link").exists());
        assert!(!loadout_lib::engine::supervisor::executable_bits(
            &fs::metadata(self.project.path().join("tool.sh"))?
        ));
        Ok(())
    }
}

struct Fake {
    edit: Edit,
    finish: Finish,
    control: RunControl,
    cwd: Arc<Mutex<Option<PathBuf>>>,
}

#[async_trait]
impl AgentDriver for Fake {
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
        *self.cwd.lock().unwrap_or_else(PoisonError::into_inner) = Some(spec.cwd.clone());
        match self.edit {
            Edit::Changed => {
                fs::write(spec.cwd.join("modified.txt"), "changed")?;
                fs::remove_file(spec.cwd.join("deleted.txt"))?;
                fs::write(spec.cwd.join("added.bin"), [0, 255, 1, 128])?;
                loadout_lib::engine::supervisor::link(
                    Path::new("modified.txt"),
                    &spec.cwd.join("link"),
                )?;
                loadout_lib::engine::supervisor::set_executable_file(
                    &std::fs::File::open(spec.cwd.join("tool.sh"))?,
                    true,
                )?;
            }
            Edit::MissingOrigin | Edit::CorruptOrigin => {
                let run = spec
                    .cwd
                    .parent()
                    .and_then(Path::parent)
                    .ok_or_else(|| anyhow::anyhow!("run root missing"))?;
                let manifest = run.join("input/manifest.json");
                match self.edit {
                    Edit::MissingOrigin => fs::remove_file(manifest)?,
                    _ => fs::write(manifest, b"not a valid description")?,
                }
            }
            Edit::Nothing => {}
        }
        Ok(Box::new(Turn {
            events,
            finish: self.finish,
            control: self.control.clone(),
            session: SessionRef {
                vendor: self.id(),
                id: spec.run_id.to_string(),
            },
        }))
    }
}

struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    finish: Finish,
    control: RunControl,
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
    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        if matches!(self.finish, Finish::Stopped) {
            self.control.stop();
            std::future::pending::<()>().await;
        }
        let outcome = Outcome {
            ok: !matches!(self.finish, Finish::Failed),
            reason: FinishReason::Completed,
            text: "The file operation finished.".to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session.clone(),
        };
        self.events
            .send(AgentEvent::Finished(outcome.clone()).into())
            .await?;
        Ok(outcome)
    }
    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }
    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}
