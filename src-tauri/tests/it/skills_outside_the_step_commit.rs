//! Z-8: półka umiejętności i duże pliki robocze nie należą do commita kroku.
//!
//! Oba przypadki idą przez prawdziwy bieg `fresh-copy`, bo sama funkcja wybierająca pathspeki
//! nie dowodzi ani tego, że używa jej zamknięcie drzewa, ani że zdanie dociera do historii
//! otwieranej przez okno (niezmiennik 29).

use std::error::Error;
use std::fs::{self, OpenOptions};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::finalize::{Landing, fold_into_one};
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::isolate;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

const VENDOR: &str = "fake";
const PATIENCE: Duration = Duration::from_secs(30);
const SKILL: &str = "alpha";
const STEP: &str = "s_1";
const STEP_NAME: &str = "Build";
const MADE: &str = "answer.txt";
const SKILL_TASK: &str = "write one source file with the selected skill";
const HEAVY_TASK: &str = "write one source file and a large build folder";
const HEAVY_DIR: &str = "dist";
const HEAVY_FILE: &str = "dist/cache.bin";
const HEAVY_BYTES: u64 = 52 * 1024 * 1024;

// Skopiowane zdanie produktu: test sądzi to, co czyta człowiek, a nie prywatną stałą produkcji.
const LEFT_BEHIND: &str = "Loadout left 52 MB from dist/ out of this step's commit because a step branch does not carry more than 50 MB of new files.";

const SKILL_FILE: &str = "---
name: alpha
description: Writes the source file requested by this step.
---

Write the requested source file.
";

fn agent_file(with_skill: bool) -> String {
    let skills = if with_skill { "[alpha]" } else { "[]" };
    format!(
        "---
schema: 1
id: 01990000-0000-7000-8000-0000000000e8
name: Builder
summary: Writes the requested files
color: slate
runsWith: codex
model: gpt-5
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 20
writeResultsTo: \"\"
tools: everything
skills: {skills}
connections: []
---
Do the work.
"
    )
}

fn workflow(task: &str) -> String {
    format!(
        r#"{{
  "format": 1,
  "id": "wf_skills_outside_the_step_commit",
  "name": "Keep generated inputs out of the result",
  "steps": [
    {{
      "kind": "agent",
      "id": "{STEP}",
      "name": "{STEP_NAME}",
      "agent": "01990000-0000-7000-8000-0000000000e8",
      "overrides": {{}},
      "instructions": "{task}",
      "folder": {{ "use": "fresh-copy" }},
      "at": {{ "x": 0, "y": 0 }}
    }}
  ],
  "links": []
}}
"#
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_step_with_a_skill_does_not_commit_the_shelf() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new(true)?;
    let base = bench.head()?;
    let report = bench.run(SKILL_TASK).await?;
    let branch = isolate::branch_for(&report.id, STEP);

    let names = bench.changed_names(&base, &branch)?;
    assert!(
        names.iter().any(|name| name == MADE),
        "the ordinary file the agent wrote is not in the step commit, so an implementation that \
         commits nothing passes the shelf assertion. The commit contains: {names:?}"
    );
    assert!(
        names.iter().all(|name| !name.starts_with(".agents/")),
        "the shelf Loadout put in the step's fresh copy became part of the person's work. `git \
         show --stat --name-only {base}..{branch}` lists: {names:?}"
    );

    // 2026-09 (Z-8): ten sam pathspek musi przeżyć składanie, bo gałąź wynikowa jest tym, co
    // człowiek naprawdę bierze; sam czysty commit kroku nie dowodzi końca tej drogi.
    let result = format!("result/{}", report.id);
    let landed = fold_into_one(
        bench.project.path(),
        &result,
        &base,
        std::slice::from_ref(&branch),
    )?;
    assert_eq!(
        landed,
        Landing::Landed {
            branch: result.clone(),
            steps: 1,
        },
        "the step branch did not fold into the result branch"
    );
    let result_names = bench.changed_names(&base, &result)?;
    assert!(
        result_names.iter().any(|name| name == MADE)
            && result_names
                .iter()
                .all(|name| !name.starts_with(".agents/")),
        "the result branch either lost the agent's work or gained Loadout's shelf: {result_names:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn heavy_untracked_files_stay_out_of_the_commit_and_the_run_says_so()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new(false)?;
    let base = bench.head()?;
    let report = bench.run(HEAVY_TASK).await?;
    let branch = isolate::branch_for(&report.id, STEP);
    let names = bench.changed_names(&base, &branch)?;

    assert!(
        names.iter().any(|name| name == MADE),
        "the ordinary file was left out together with the large folder: {names:?}"
    );
    assert!(
        names.iter().all(|name| !name.starts_with("dist/")),
        "the large untracked folder is still in the step commit: {names:?}"
    );

    let folder = report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the run directory has no UTF-8 folder name")?;
    let past = read_run_inner(bench.project.path(), folder)?;
    let said = past
        .steps
        .iter()
        .find(|step| step.tile == STEP)
        .map(|step| step.error.as_str())
        .ok_or("the open run does not contain the step")?;
    assert_eq!(
        said, LEFT_BEHIND,
        "the same command the run window uses does not show the sentence naming dist/ and its \
         size (invariant 29)"
    );
    Ok(())
}

#[derive(Debug)]
struct Fake;

fn fake_drivers() -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake);
    Arc::new(move |_vendor| Arc::clone(&driver))
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
        if spec.prompt.contains(SKILL_TASK) {
            let skill = spec.cwd.join(".agents/skills").join(SKILL).join("SKILL.md");
            if !skill.is_file() {
                anyhow::bail!(
                    "the driver cannot take skills in argv and the selected skill is not on its shelf: {}",
                    skill.display()
                );
            }
        } else if spec.prompt.contains(HEAVY_TASK) {
            fs::create_dir_all(spec.cwd.join(HEAVY_DIR))?;
            // 2026-09 (Z-8): rozmiar logiczny rzadkiego pliku przekracza prawdziwy próg 50 MiB,
            // ale test nie zapisuje 52 MiB zer na dysk tylko po to, żeby policzyć `metadata.len()`.
            OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(spec.cwd.join(HEAVY_FILE))?
                .set_len(HEAVY_BYTES)?;
        } else {
            anyhow::bail!("the driver received an unknown task: {}", spec.prompt);
        }
        fs::write(spec.cwd.join(MADE), "work the person asked for\n")?;

        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };
        let _ = events
            .send(
                (AgentEvent::Started {
                    session: session.clone(),
                    model: spec.model.clone().unwrap_or_default(),
                    tools: Vec::new(),
                    capabilities: Vec::new(),
                })
                .into(),
            )
            .await;
        Ok(Box::new(Turn { events, session }))
    }
}

#[derive(Debug)]
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
            text: String::new(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
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
}

impl Bench {
    fn new(with_skill: bool) -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(home.path().join("skills"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(
            home.path().join("agents/builder.md"),
            agent_file(with_skill),
        )?;
        if with_skill {
            fs::create_dir_all(home.path().join("skills").join(SKILL))?;
            fs::write(
                home.path().join("skills").join(SKILL).join("SKILL.md"),
                SKILL_FILE,
            )?;
        }
        fs::write(project.path().join("notes.txt"), "the human's file\n")?;
        let bench = Self { home, project };
        bench.git(&["init", "--quiet", "--initial-branch=main"])?;
        fs::write(bench.project.path().join(".gitignore"), ".loadout/\n")?;
        bench.git(&["add", "-A"])?;
        bench.git(&["commit", "--quiet", "-m", "the human's first commit"])?;
        Ok(bench)
    }

    async fn run(&self, task: &str) -> Result<RunReport, Box<dyn Error>> {
        let workflow_path = self.home.path().join("workflows/z8.json");
        fs::write(&workflow_path, workflow(task))?;
        let store = Store::open(&self.project.path().join(".loadout/loadout.db"))?;
        let deps = RunDeps {
            home: self.home.path(),
            library: self.home.path().to_path_buf(),
            project: self.project.path(),
            store: &store,
            drivers: fake_drivers(),
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow: workflow_path,
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        };
        let (sink, source) = line_channel(QUEUE_CAP);
        let pump = spawn_pump(source, Channel::new(|_| Ok(())));
        let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
            .await
            .map_err(|_| "the run did not come back")??;
        let _ = tokio::time::timeout(PATIENCE, pump).await;
        assert_eq!(
            report.steps,
            vec![StepState::Succeeded],
            "the step did not finish, so the commit assertions do not describe the requested run"
        );
        Ok(report)
    }

    fn head(&self) -> Result<String, Box<dyn Error>> {
        Ok(self.git(&["rev-parse", "HEAD"])?.trim().to_owned())
    }

    fn changed_names(&self, base: &str, branch: &str) -> Result<Vec<String>, Box<dyn Error>> {
        let range = format!("{base}..{branch}");
        Ok(self
            .git(&["show", "--stat", "--name-only", "--format=", &range])?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect())
    }

    fn git(&self, args: &[&str]) -> Result<String, Box<dyn Error>> {
        git(self.project.path(), args).map_err(Into::into)
    }
}

fn git(at: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(at)
        .args(["-c", "user.name=The Agent"])
        .args(["-c", "user.email=agent@loadout.invalid"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "git {args:?} refused: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
