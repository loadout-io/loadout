//! T-82 AC-3: odtworzony graf przechodzi przez produkcyjne `preview` → `apply` → Run.
//!
//! Sama równość pięciu kroków nie dowodzi grafu: fikstura mogłaby zostać zapisana i nigdy nie
//! uruchomiona albo uruchomiona jeden-po-drugim. Dubler mierzy więc okna obu recenzentów, prompt
//! wykonawcy z indeksem przekazania planisty, realny zasięg przypisanego skilla oraz start
//! składacza dopiero po zamknięciu obu gałęzi. Wszystkie kroki dostają własne kopie, żeby
//! walidator nie odmówił prawidłowej równoległości z powodu kolizji ścieżek (niezmiennik 12).

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check_to_run};
use loadout_lib::workflow::{Skills, Step, file};
use tauri::ipc::Channel;
use tokio::sync::mpsc;

const VENDOR: &str = "fake";
const WORKFLOW_NAME: &str = "Delivery Circuit";
const SOURCE_PATH: &str = ".claude/workflows/delivery-circuit.js";
const SKILL: &str = "assembly-guide";
const PLAN: &str = "Pathfinder";
const BUILD: &str = "Maker";
const VISUAL: &str = "Prism";
const CODE: &str = "Sentinel";
const COMBINE: &str = "Binder";
const PLAN_TASK: &str = "map the delivery";
const BUILD_TASK: &str = "build the delivery";
const VISUAL_TASK: &str = "inspect the visible result";
const CODE_TASK: &str = "inspect the implementation";
const COMBINE_TASK: &str = "combine both reviews";
const REVIEW_TURN: Duration = Duration::from_millis(250);
const OTHER_TURN: Duration = Duration::from_millis(30);
const MIN_REVIEW_OVERLAP: Duration = Duration::from_millis(80);
const PATIENCE: Duration = Duration::from_secs(30);

const SOURCE: &str = r#"workflow("Delivery Circuit", () => {
  const plan = agent("pathfinder", {
    name: "Pathfinder",
    task: "map the delivery",
    folder: "fresh-copy"
  });
  const build = agent("maker", {
    name: "Maker",
    task: "build the delivery",
    after: [plan],
    skills: ["assembly-guide"],
    folder: "fresh-copy"
  });
  const visual = agent("prism", {
    name: "Prism",
    task: "inspect the visible result",
    after: [build],
    folder: "fresh-copy"
  });
  const code = agent("sentinel", {
    name: "Sentinel",
    task: "inspect the implementation",
    after: [build],
    folder: "fresh-copy"
  });
  agent("binder", {
    name: "Binder",
    task: "combine both reviews",
    after: [visual, code],
    folder: "fresh-copy"
  });
});
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(clippy::too_many_lines)]
async fn imported_fan_out_and_join_reach_the_real_run_path() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    write_repository(project.path())?;
    let preview = loadout_lib::import::translate::preview(project.path())?;
    assert!(
        preview.draft.runnable(),
        "the complete five-agent source and its skill must be ready before apply; unresolved fixtures do not exercise Run"
    );

    let imported = preview
        .draft
        .workflows
        .iter()
        .find(|workflow| workflow.name == WORKFLOW_NAME)
        .ok_or_else(|| {
            format!(
                "{WORKFLOW_NAME} was not reconstructed from {SOURCE_PATH}; there is no graph to run"
            )
        })?;
    let maker = imported
        .steps
        .iter()
        .find_map(|step| match step {
            Step::Agent(step) if step.name == BUILD => Some(step),
            Step::Agent(_) | Step::Check(_) | Step::Checkpoint(_) | Step::Serve(_) => None,
        })
        .ok_or("the imported graph has no Maker agent step")?;
    assert_eq!(
        maker.skills,
        Skills::Only(vec![SKILL.to_owned()]),
        "the runtime can only prove a selected skill if the imported step preserved that selection"
    );

    let home = tempfile::tempdir()?;
    loadout_lib::import::apply::apply(home.path(), &preview.draft)?;
    let item = preview
        .draft
        .items
        .iter()
        .find(|item| {
            item.sources
                .iter()
                .any(|source| source.path == Path::new(SOURCE_PATH))
        })
        .ok_or("the workflow source has no typed import item")?;
    let target = item
        .target
        .as_ref()
        .ok_or("the ready workflow item has no saved target")?;
    let workflow_path = home.path().join(target);
    let saved = file::load(&workflow_path)?;
    let problems: Vec<String> = check_to_run(&saved)
        .into_iter()
        .filter(|note| note.level == Level::Problem)
        .map(|note| note.message)
        .collect();
    assert!(
        problems.is_empty(),
        "the reconstructed workflow would be refused before the driver ran: {problems:?}"
    );

    fs::create_dir_all(project.path().join(".loadout"))?;
    let store = Store::open(&project.path().join(".loadout/loadout.db"))?;
    let watch = Arc::new(Watch::default());
    let deps = RunDeps {
        home: home.path(),
        project: project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch)),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: workflow_path,
        how_many_at_once: 3,
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
    .map_err(|_| format!("the imported workflow did not finish within {PATIENCE:?}"))?;
    let report = report?;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; 5],
        "all five imported agent steps must actually finish; they ended as {:?}",
        report.steps
    );
    let runs = watch.snapshot();
    assert_eq!(
        runs.len(),
        5,
        "the driver saw {} turns instead of all five imported steps: {:?}",
        runs.len(),
        runs.iter()
            .map(|run| run.label.as_str())
            .collect::<Vec<_>>()
    );

    let plan = one(&runs, PLAN)?;
    let build = one(&runs, BUILD)?;
    let visual = one(&runs, VISUAL)?;
    let code = one(&runs, CODE)?;
    let combine = one(&runs, COMBINE)?;
    assert!(
        plan.end()? <= build.from,
        "Maker started before Pathfinder finished, so the imported arrow is decorative"
    );
    assert!(
        build.prompt.contains("handoffs/") && build.prompt.contains(PLAN),
        "Maker did not receive Pathfinder's handoff index. Its prompt was: {}",
        build.prompt
    );
    assert_eq!(
        build.skills,
        BTreeSet::from([SKILL.to_owned()]),
        "Maker reached the driver without exactly the skill selected by the imported source"
    );
    assert!(
        build.end()? <= visual.from && build.end()? <= code.from,
        "a reviewer started before Maker handed over the implementation"
    );

    let overlap = visual
        .end()?
        .min(code.end()?)
        .saturating_duration_since(visual.from.max(code.from));
    assert!(
        overlap >= MIN_REVIEW_OVERLAP,
        "the two source siblings shared only {overlap:?}; they were dispatched wide but did not run in parallel"
    );
    assert!(
        visual.end()? <= combine.from && code.end()? <= combine.from,
        "Binder started before both review branches finished"
    );
    assert!(
        combine.prompt.contains(VISUAL) && combine.prompt.contains(CODE),
        "Binder must receive both review handoffs after the join. Its prompt was: {}",
        combine.prompt
    );
    Ok(())
}

fn write_repository(root: &Path) -> Result<(), Box<dyn Error>> {
    let agents = root.join(".claude/agents");
    fs::create_dir_all(&agents)?;
    for (role, name, skills) in [
        ("pathfinder", PLAN, ""),
        ("maker", BUILD, "skills: [assembly-guide]\n"),
        ("prism", VISUAL, ""),
        ("sentinel", CODE, ""),
        ("binder", COMBINE, ""),
    ] {
        fs::write(
            agents.join(format!("{role}.md")),
            format!(
                "---\nname: {role}\ndescription: {name} role\nmodel: sonnet\ntools: [Read, Write]\n{skills}---\nDo the {name} work.\n"
            ),
        )?;
    }

    let skill = root.join(".agents/skills").join(SKILL);
    fs::create_dir_all(&skill)?;
    fs::write(
        skill.join("SKILL.md"),
        format!(
            "---\nname: {SKILL}\ndescription: Builds the delivery consistently\n---\nFollow the delivery conventions.\n"
        ),
    )?;
    let source = root.join(SOURCE_PATH);
    fs::create_dir_all(source.parent().ok_or("workflow source has no parent")?)?;
    fs::write(source, SOURCE)?;
    Ok(())
}

#[derive(Debug, Clone)]
struct Ran {
    label: String,
    from: Instant,
    to: Option<Instant>,
    prompt: String,
    skills: BTreeSet<String>,
}

impl Ran {
    fn end(&self) -> Result<Instant, Box<dyn Error>> {
        self.to
            .ok_or_else(|| format!("{} entered the driver but never left", self.label).into())
    }
}

#[derive(Debug, Default)]
struct Watch(Mutex<Vec<Ran>>);

impl Watch {
    fn entered(&self, label: &str, prompt: String, skills: BTreeSet<String>) -> usize {
        let mut runs = self.lock();
        runs.push(Ran {
            label: label.to_owned(),
            from: Instant::now(),
            to: None,
            prompt,
            skills,
        });
        runs.len() - 1
    }

    fn left(&self, index: usize) {
        if let Some(run) = self.lock().get_mut(index) {
            run.to.get_or_insert_with(Instant::now);
        }
    }

    fn snapshot(&self) -> Vec<Ran> {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Ran>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn one<'a>(runs: &'a [Ran], label: &str) -> Result<&'a Ran, Box<dyn Error>> {
    let found: Vec<_> = runs.iter().filter(|run| run.label == label).collect();
    match found.as_slice() {
        [only] => Ok(*only),
        other => Err(format!("expected one {label} run, found {}", other.len()).into()),
    }
}

fn fake_drivers(watch: Arc<Watch>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        watch,
        flags: Vec::new(),
    });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct Fake {
    watch: Arc<Watch>,
    flags: Vec<String>,
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

    fn inheriting(&self, flags: &[String]) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            watch: Arc::clone(&self.watch),
            flags: flags.to_vec(),
        }))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let label = label_of(&spec.prompt).to_owned();
        let index = self.watch.entered(
            &label,
            spec.prompt.clone(),
            within_reach(&spec.cwd, &self.flags),
        );
        let session = SessionRef {
            vendor: VENDOR,
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
        let hold = if label == VISUAL || label == CODE {
            REVIEW_TURN
        } else {
            OTHER_TURN
        };
        Ok(Box::new(Turn {
            watch: Arc::clone(&self.watch),
            events,
            session,
            index,
            label,
            hold,
        }))
    }
}

#[derive(Debug)]
struct Turn {
    watch: Arc<Watch>,
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    index: usize,
    label: String,
    hold: Duration,
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
        tokio::time::sleep(self.hold).await;
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: format!("handoff from {}", self.label),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: self.hold,
            session: self.session.clone(),
        };
        self.watch.left(self.index);
        let _ = self
            .events
            .send((AgentEvent::Finished(outcome.clone())).into())
            .await;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> GroupProof {
        self.watch.left(self.index);
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        self.watch.left(self.index);
        Ok(Some(0))
    }
}

fn label_of(prompt: &str) -> &str {
    for (task, label) in [
        (PLAN_TASK, PLAN),
        (BUILD_TASK, BUILD),
        (VISUAL_TASK, VISUAL),
        (CODE_TASK, CODE),
        (COMBINE_TASK, COMBINE),
    ] {
        if prompt.starts_with(task) {
            return label;
        }
    }
    "unrecognized imported step"
}

fn skills_under(root: &Path) -> BTreeSet<String> {
    let Ok(entries) = fs::read_dir(root) else {
        return BTreeSet::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("SKILL.md").is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect()
}

fn within_reach(cwd: &Path, flags: &[String]) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for (index, flag) in flags.iter().enumerate() {
        if flag == "--plugin-dir"
            && let Some(directory) = flags.get(index + 1)
        {
            found.extend(skills_under(&PathBuf::from(directory).join("skills")));
        }
    }
    found.extend(skills_under(&cwd.join(".agents/skills")));
    found.extend(skills_under(&cwd.join(".claude/skills")));
    found
}
