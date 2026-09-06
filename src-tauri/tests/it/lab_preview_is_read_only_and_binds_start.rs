//! WF-19: preview nie jest niewidocznym Startem, a Start nie podmienia zatwierdzonych źródeł.
use async_trait::async_trait;
use loadout_lib::{
    commands::{self, Drivers},
    engine::drivers::{AgentDriver, AgentHandle, DecodedEvent, Probe, RunSpec},
    library::agents::Agent,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::sync::mpsc;

#[tokio::test]
async fn preview_never_recovers_or_removes_interrupted_definition_publications()
-> Result<(), Box<dyn Error>> {
    use loadout_lib::durable_file::{
        DurableFilePublisher, FaultAction, FaultInjector, FaultPoint, ModePolicy, PublicationEvent,
        scoped_faults,
    };
    struct CrashBeforeCommit;
    impl FaultInjector for CrashBeforeCommit {
        fn action(&self, event: &PublicationEvent) -> FaultAction {
            if event.point == FaultPoint::BeforeCommit {
                FaultAction::Crash
            } else {
                FaultAction::Continue
            }
        }
    }
    for shelf in ["workflows", "agents"] {
        let bench = Bench::new(false, true)?;
        let dir = if shelf == "agents" {
            bench.home.join("agents")
        } else {
            bench.project.join(".loadout/workflows")
        };
        let target = fs::read_dir(&dir)?
            .next()
            .ok_or("fixture shelf is empty")??
            .path();
        let before_crash = files(&dir)?;
        {
            let _scope = scoped_faults(&dir, Arc::new(CrashBeforeCommit))?;
            let crashed = DurableFilePublisher::new(&dir).atomic_replace(
                &target,
                &fs::read(&target)?,
                ModePolicy::PreserveExistingOr(0o600),
            );
            assert!(
                crashed.is_err(),
                "fixture did not interrupt the actual publisher"
            );
        }
        assert!(
            files(&dir)?.len() > before_crash.len(),
            "actual publication did not retain its temporary file"
        );
        let before_preview = files(bench.root.path())?;
        let _shown = commands::lab::preview_run_inner(
            &bench.home,
            &bench.project,
            "preview",
            Some(&bench.revision),
            &bench.drivers,
        )
        .await?;
        assert_eq!(
            files(bench.root.path())?,
            before_preview,
            "preview performed recovery or cleanup in {shelf}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn preview_reports_the_shared_size_without_creating_plans_runs_or_private_runtime_files()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new(false, true)?;
    let before = files(bench.root.path())?;
    let shown = commands::lab::preview_run_inner(
        &bench.home,
        &bench.project,
        "preview",
        Some(&bench.revision),
        &bench.drivers,
    )
    .await?;
    assert!(
        shown.cannot_run.is_none(),
        "an available diagnostic comparison was refused: {shown:?}"
    );
    let size = shown.size.ok_or("the person cannot see the planned size")?;
    assert_eq!(
        (size.cells, size.nodes, size.edges, size.trees),
        (2, 4, 2, 4)
    );
    assert_eq!(shown.revision, bench.revision);
    assert!(shown.source_revision.is_some());
    assert!(!shown.protected);
    assert!(
        bench.probed.load(Ordering::SeqCst) > 0,
        "executor availability was not checked"
    );
    assert_eq!(
        files(bench.root.path())?,
        before,
        "preview wrote run/plan/runtime artifacts"
    );
    Ok(())
}

#[tokio::test]
async fn a_missing_examiner_interpreter_refuses_preview_and_start_before_paid_work()
-> Result<(), Box<dyn Error>> {
    examiner_is_unavailable(false).await
}

#[tokio::test]
async fn a_non_executable_examiner_interpreter_refuses_preview_and_start_before_paid_work()
-> Result<(), Box<dyn Error>> {
    examiner_is_unavailable(true).await
}

async fn examiner_is_unavailable(exists: bool) -> Result<(), Box<dyn Error>> {
    let mut bench = Bench::new(true, true)?;
    // Ten przypadek dotyczy interpretera, nie osobno sprawdzonej odmowy ochrony drivera.
    // Realny dowód supervisora nadal biegnie; atrapa deklaruje wyłącznie swój kontrakt.
    let probed = bench.probed.clone();
    let started = bench.started.clone();
    bench.drivers = Arc::new(move |_| {
        Arc::new(ReadOnly {
            found: true,
            probed: probed.clone(),
            started: started.clone(),
            supports_protection: true,
        })
    });
    let program = bench.root.path().join("unavailable-interpreter");
    if exists {
        fs::write(&program, b"this must never be executed")?;
        loadout_lib::engine::supervisor::set_executable_file(
            &std::fs::File::open(&program)?,
            false,
        )?;
    }
    let mut open = commands::lab::read_set_inner(&bench.project, "preview")?;
    open.set.cases[0]
        .extra
        .get_mut("examiner")
        .ok_or("examiner missing in fixture")?["program"] = json!(program);
    let revision = commands::lab::save_set_inner(&bench.project, &open.set, Some(&open.revision))?;
    let before = files(bench.root.path())?;
    let shown = commands::lab::preview_run_inner(
        &bench.home,
        &bench.project,
        "preview",
        Some(&revision),
        &bench.drivers,
    )
    .await?;
    let reason = shown
        .cannot_run
        .ok_or("preview accepted an unavailable examiner interpreter")?;
    assert!(
        reason.to_lowercase().contains("interpreter"),
        "preview did not identify the unavailable interpreter: {reason}"
    );
    let refusal = commands::lab::plan_a_run_with_expected(
        &bench.home,
        &bench.project,
        "preview",
        2,
        Some(&revision),
        None,
    );
    let reason = match refusal {
        Ok(_) => return Err(
            "the actual planner accepted an unavailable examiner interpreter before paid workers"
                .into(),
        ),
        Err(error) => error.to_string(),
    };
    assert!(
        reason.to_lowercase().contains("interpreter"),
        "Start did not identify the unavailable interpreter: {reason}"
    );
    assert_eq!(
        files(bench.root.path())?,
        before,
        "refused preview/Start created files"
    );
    Ok(())
}

#[tokio::test]
async fn a_changed_set_refuses_preview_and_the_actual_start_against_the_reviewed_revision()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new(false, true)?;
    let mut open = commands::lab::read_set_inner(&bench.project, "preview")?;
    open.set.name = "Changed after the view opened".to_owned();
    commands::lab::save_set_inner(&bench.project, &open.set, Some(&open.revision))?;
    assert!(
        commands::lab::preview_run_inner(
            &bench.home,
            &bench.project,
            "preview",
            Some(&bench.revision),
            &bench.drivers
        )
        .await
        .is_err()
    );
    let before = files(bench.root.path())?;
    assert!(
        commands::lab::plan_a_run_with_expected(
            &bench.home,
            &bench.project,
            "preview",
            2,
            Some(&bench.revision),
            None
        )
        .is_err()
    );
    assert_eq!(files(bench.root.path())?, before);
    Ok(())
}

#[tokio::test]
async fn even_an_unpinned_legacy_source_cannot_change_between_preview_and_the_actual_start()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new(false, true)?;
    let shown = commands::lab::preview_run_inner(
        &bench.home,
        &bench.project,
        "preview",
        Some(&bench.revision),
        &bench.drivers,
    )
    .await?;
    let source_revision = shown.source_revision.ok_or("missing source binding")?;
    let source = bench.project.join(".loadout/workflows/a.json");
    let mut graph: Value = serde_json::from_slice(&fs::read(&source)?)?;
    graph["steps"][0]["instructions"] = json!("Changed after preview");
    fs::write(&source, serde_json::to_vec(&graph)?)?;
    let before = files(bench.root.path())?;
    let result = commands::lab::plan_a_run_with_expected(
        &bench.home,
        &bench.project,
        "preview",
        2,
        Some(&bench.revision),
        Some(&source_revision),
    );
    assert!(
        result.is_err(),
        "Start adopted a different source workflow from the one whose size was reviewed"
    );
    assert_eq!(files(bench.root.path())?, before);
    Ok(())
}

#[tokio::test]
async fn missing_agent_apps_are_visible_before_any_paid_start() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new(false, false)?;
    let before = files(bench.root.path())?;
    let shown = commands::lab::preview_run_inner(
        &bench.home,
        &bench.project,
        "preview",
        Some(&bench.revision),
        &bench.drivers,
    )
    .await?;
    assert!(
        shown
            .cannot_run
            .as_ref()
            .is_some_and(|said| !said.is_empty()),
        "an unavailable executor appeared ready: {shown:?}"
    );
    assert!(bench.probed.load(Ordering::SeqCst) > 0);
    assert_eq!(files(bench.root.path())?, before);
    Ok(())
}

#[tokio::test]
async fn a_driver_without_read_only_protection_support_does_not_appear_ready()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new(true, true)?;
    let before = files(bench.root.path())?;
    let shown = commands::lab::preview_run_inner(
        &bench.home,
        &bench.project,
        "preview",
        Some(&bench.revision),
        &bench.drivers,
    )
    .await?;
    assert!(shown.protected);
    assert!(
        shown
            .cannot_run
            .as_ref()
            .is_some_and(|said| !said.is_empty()),
        "unknown protection support appeared ready: {shown:?}"
    );
    assert_eq!(files(bench.root.path())?, before);
    Ok(())
}

struct Bench {
    root: tempfile::TempDir,
    home: PathBuf,
    project: PathBuf,
    revision: String,
    drivers: Drivers,
    probed: Arc<AtomicUsize>,
    started: Arc<AtomicUsize>,
}
impl Bench {
    fn new(protected: bool, found: bool) -> Result<Self, Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let home = root.path().join("home");
        let project = root.path().join("project");
        fs::create_dir_all(home.join("agents"))?;
        fs::create_dir_all(project.join(".loadout/workflows"))?;
        let mut agent = Agent::example();
        agent.skills.clear();
        agent.connections.clear();
        commands::agents::save_agent_inner(&home, &agent, None)?;
        for id in ["a", "b"] {
            fs::write(
                project.join(format!(".loadout/workflows/{id}.json")),
                serde_json::to_vec(&json!({
                "format":1,"id":id,"name":id,"steps":[{"kind":"agent","id":"out","name":"Work","agent":agent.id.to_string(),
                    "instructions":"Do the task","folder":{"use":"fresh-copy"},"skills":[],"at":{"x":0,"y":0}}],"links":[]}))?,
            )?;
        }
        let mut case = json!({"id":"case","name":"Case","task":"Task","status":"in-use","command":"node test.cjs","proof":"passed: (\\d+)"});
        if protected {
            case["command"] = json!("");
            case["proof"] = json!("");
            case["proofMode"] = json!("external-assessment-v1");
            case["examiner"] = json!({"kind":"python","program":std::env::current_exe()?,"source":"raise RuntimeError('not executed during preview')"});
        }
        let set = serde_json::from_value(
            json!({"format":2,"id":"preview","name":"Preview","subject":{"kind":"workflow","id":"a"},"protected":protected,
            "cases":[case],"variants":[{"id":"a","name":"A","workflow":{"id":"a","outputStep":"out"}},
                {"id":"b","name":"B","workflow":{"id":"b","outputStep":"out"}}]}),
        )?;
        let revision = commands::lab::save_set_inner(&project, &set, None)?;
        let probed = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(AtomicUsize::new(0));
        let start_calls = started.clone();
        let calls = probed.clone();
        let drivers: Drivers = Arc::new(move |_| {
            Arc::new(ReadOnly {
                found,
                probed: calls.clone(),
                started: start_calls.clone(),
                supports_protection: false,
            })
        });
        Ok(Self {
            root,
            home,
            project,
            revision,
            drivers,
            probed,
            started,
        })
    }
}
impl Drop for Bench {
    fn drop(&mut self) {
        // 2026-09-06: błąd startu może zostać obsłużony przez runtime. Licznik dowodzi
        // braku próby również wtedy; panika wyłącznie w zadaniu potomnym tego nie dowodziła.
        if !std::thread::panicking() {
            assert_eq!(
                self.started.load(Ordering::SeqCst),
                0,
                "a read-only preview started paid work"
            );
        }
    }
}

struct ReadOnly {
    found: bool,
    probed: Arc<AtomicUsize>,
    started: Arc<AtomicUsize>,
    supports_protection: bool,
}
#[async_trait]
impl AgentDriver for ReadOnly {
    fn id(&self) -> &'static str {
        "claude-code"
    }
    fn protected_readiness(&self) -> Option<anyhow::Result<()>> {
        self.supports_protection.then_some(Ok(()))
    }
    async fn probe(&self) -> anyhow::Result<Probe> {
        self.probed.fetch_add(1, Ordering::SeqCst);
        Ok(Probe {
            found: self.found,
            version: None,
        })
    }
    async fn start(
        &self,
        _: RunSpec,
        _: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.started.fetch_add(1, Ordering::SeqCst);
        anyhow::bail!("a read-only preview started paid work")
    }
}
type FileTree = BTreeMap<PathBuf, Option<Vec<u8>>>;

fn files(root: &Path) -> Result<FileTree, Box<dyn Error>> {
    fn visit(
        root: &Path,
        at: &Path,
        found: &mut BTreeMap<PathBuf, Option<Vec<u8>>>,
    ) -> Result<(), Box<dyn Error>> {
        for entry in fs::read_dir(at)? {
            let entry = entry?;
            let path = entry.path();
            let key = path.strip_prefix(root)?.to_owned();
            if entry.file_type()?.is_dir() {
                found.insert(key, None);
                visit(root, &path, found)?;
            } else {
                found.insert(key, Some(fs::read(path)?));
            }
        }
        Ok(())
    }
    let mut found = BTreeMap::new();
    visit(root, root, &mut found)?;
    Ok(found)
}
