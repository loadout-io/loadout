//! AC-1 for T-114: copy work keys and Git refs encode the same identity without sharing syntax.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::sync::Arc;
use std::time::Duration;

use loadout_lib::commands::RunRequest;
use loadout_lib::engine::step::StepState;
use loadout_lib::store::Store;

use self::support::{Rig, Spy, git, run};

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_t114_two_copies",
  "name": "Two copies",
  "steps": [{
    "kind": "agent", "id": "s_2", "name": "Build",
    "agent": "01990000-0000-7000-8000-000000001114", "overrides": {},
    "copies": 2, "instructions": "build copy {{copy}} of {{copies}}",
    "folder": { "use": "fresh-copy" }
  }],
  "links": []
}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_copies_keep_exact_work_keys_refs_and_recovery_sources() -> Result<(), Box<dyn Error>> {
    let rig = Rig::git()?;
    let workflow = rig.workflow("two-copies", WORKFLOW)?;
    let store = Store::open(&rig.db())?;
    let first_spy = Arc::new(Spy::new(write_copy_marker, |_| String::new()));

    let first = run(
        &rig,
        &store,
        Arc::clone(&first_spy),
        request(workflow.clone(), None),
    )
    .await??;
    assert_eq!(first.steps, vec![StepState::Succeeded; 2]);

    let seen = first_spy.started();
    let mut folders: Vec<&str> = seen
        .iter()
        .filter_map(|one| one.cwd.file_name()?.to_str())
        .collect();
    folders.sort_unstable();
    assert_eq!(folders, ["s_2", "s_2~2"]);
    assert!(support::shared_for(&seen) >= Duration::from_millis(60));

    let prefix = format!("loadout/{}/", first.id);
    let branches = loadout_lib::commands::isolate::branches_under(rig.project(), &prefix);
    assert_eq!(branches, [format!("{prefix}s_2"), format!("{prefix}s_2-2")]);
    assert_eq!(
        git(rig.project(), &["show", &format!("{prefix}s_2:copy-1.txt")])?,
        "one\n"
    );
    assert_eq!(
        git(
            rig.project(),
            &["show", &format!("{prefix}s_2-2:copy-2.txt")]
        )?,
        "two\n"
    );

    let resumed_spy = Arc::new(Spy::new(|_| Ok(()), |_| String::new()));
    run(
        &rig,
        &store,
        Arc::clone(&resumed_spy),
        request(workflow, Some(first.dir)),
    )
    .await??;
    let inherited: BTreeMap<u8, Vec<String>> = resumed_spy
        .started()
        .into_iter()
        .map(|one| (support::copy_number(&one.prompt), one.files_before_start))
        .collect();
    assert!(inherited[&1].iter().any(|name| name == "copy-1.txt"));
    assert!(inherited[&2].iter().any(|name| name == "copy-2.txt"));
    assert!(!inherited[&2].iter().any(|name| name == "copy-1.txt"));
    Ok(())
}

fn request(workflow: std::path::PathBuf, from: Option<std::path::PathBuf>) -> RunRequest {
    RunRequest {
        workflow,
        how_many_at_once: 2,
        task: None,
        part: None,
        handoffs_from: from,
    }
}

fn write_copy_marker(spec: &loadout_lib::engine::drivers::RunSpec) -> anyhow::Result<()> {
    let copy = support::copy_number(&spec.prompt);
    let word = if copy == 1 { "one\n" } else { "two\n" };
    fs::write(spec.cwd.join(format!("copy-{copy}.txt")), word)?;
    Ok(())
}

pub(crate) mod support {
    use std::error::Error;
    use std::fmt;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
    use std::time::{Duration, Instant};

    use async_trait::async_trait;
    use loadout_lib::commands::run::run_workflow_inner;
    use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunError, RunReport, RunRequest};
    use loadout_lib::engine::drivers::{
        AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
        Probe, RunSpec, SessionRef, Tokens,
    };
    use loadout_lib::engine::supervisor::{GroupId, GroupProof};
    use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
    use loadout_lib::store::Store;
    use tauri::ipc::Channel;
    use tempfile::TempDir;
    use tokio::sync::mpsc;

    const VENDOR: &str = "claude-code";
    const TURN: Duration = Duration::from_millis(120);
    const PATIENCE: Duration = Duration::from_secs(20);
    const AGENT: &str = "---\nschema: 1\nid: 01990000-0000-7000-8000-000000001114\nname: T114 Hand\nsummary: Runs the T-114 acceptance fixtures\ncolor: moss\nrunsWith: claude-code\nmodel: opus\nthinking: balanced\nfileAccess: work-freely\ngiveUpAfterMinutes: 20\nwriteResultsTo: \"\"\ntools: everything\nskills: []\nconnections: []\n---\nDo the work.\n";

    pub(crate) struct Rig {
        pub home: TempDir,
        pub project: TempDir,
    }

    impl Rig {
        pub fn plain() -> Result<Self, Box<dyn Error>> {
            let home = TempDir::new()?;
            let project = TempDir::new()?;
            fs::create_dir_all(home.path().join("agents"))?;
            fs::create_dir_all(home.path().join("workflows"))?;
            fs::create_dir_all(project.path().join(".loadout"))?;
            fs::write(home.path().join("agents/t114.md"), AGENT)?;
            Ok(Self { home, project })
        }

        pub fn git() -> Result<Self, Box<dyn Error>> {
            let rig = Self::plain()?;
            git(rig.project(), &["init", "--quiet"])?;
            fs::write(rig.project().join("README.md"), "the project\n")?;
            fs::write(rig.project().join(".gitignore"), ".loadout/\n")?;
            git(rig.project(), &["add", "-A"])?;
            git(rig.project(), &["commit", "--quiet", "-m", "fixture"])?;
            Ok(rig)
        }

        pub fn project(&self) -> &Path {
            self.project.path()
        }

        pub fn workflow(&self, slug: &str, text: &str) -> Result<PathBuf, Box<dyn Error>> {
            let path = self
                .home
                .path()
                .join("workflows")
                .join(format!("{slug}.json"));
            fs::write(&path, text)?;
            Ok(path)
        }

        pub fn db(&self) -> PathBuf {
            self.project().join(".loadout/loadout.db")
        }

        pub fn run_dirs(&self) -> Vec<PathBuf> {
            fs::read_dir(self.project().join(".loadout/runs"))
                .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
                .unwrap_or_default()
        }
    }

    type Effect = dyn Fn(&RunSpec) -> anyhow::Result<()> + Send + Sync;
    type Reply = dyn Fn(&RunSpec) -> String + Send + Sync;

    pub(crate) struct Spy {
        seen: Arc<Mutex<Vec<Started>>>,
        effect: Arc<Effect>,
        reply: Arc<Reply>,
    }

    impl fmt::Debug for Spy {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter
                .debug_struct("Spy")
                .field("started", &self.started().len())
                .finish_non_exhaustive()
        }
    }

    impl Spy {
        pub fn new<E, R>(effect: E, reply: R) -> Self
        where
            E: Fn(&RunSpec) -> anyhow::Result<()> + Send + Sync + 'static,
            R: Fn(&RunSpec) -> String + Send + Sync + 'static,
        {
            Self {
                seen: Arc::new(Mutex::new(Vec::new())),
                effect: Arc::new(effect),
                reply: Arc::new(reply),
            }
        }

        pub fn answering<R>(reply: R) -> Self
        where
            R: Fn(&RunSpec) -> String + Send + Sync + 'static,
        {
            Self::new(|_| Ok(()), reply)
        }

        pub fn started(&self) -> Vec<Started> {
            self.lock().clone()
        }

        pub fn count(&self) -> usize {
            self.lock().len()
        }

        fn enter(&self, spec: &RunSpec) -> usize {
            let mut files: Vec<String> = fs::read_dir(&spec.cwd)
                .map(|entries| {
                    entries
                        .flatten()
                        .filter_map(|entry| entry.file_name().into_string().ok())
                        .collect()
                })
                .unwrap_or_default();
            files.sort();
            let mut seen = self.lock();
            seen.push(Started {
                prompt: spec.prompt.clone(),
                cwd: spec.cwd.clone(),
                extra_dirs: spec.extra_dirs.clone(),
                files_before_start: files,
                entered: Instant::now(),
                left: None,
            });
            seen.len() - 1
        }

        fn leave(&self, at: usize) {
            if let Some(one) = self.lock().get_mut(at) {
                one.left = Some(Instant::now());
            }
        }

        fn lock(&self) -> MutexGuard<'_, Vec<Started>> {
            self.seen.lock().unwrap_or_else(PoisonError::into_inner)
        }
    }

    #[derive(Clone, Debug)]
    pub(crate) struct Started {
        pub prompt: String,
        pub cwd: PathBuf,
        pub extra_dirs: Vec<PathBuf>,
        pub files_before_start: Vec<String>,
        entered: Instant,
        left: Option<Instant>,
    }

    pub fn copy_number(prompt: &str) -> u8 {
        if prompt
            .lines()
            .next()
            .is_some_and(|line| line.contains("copy 2 of 2"))
        {
            2
        } else {
            1
        }
    }

    pub fn shared_for(seen: &[Started]) -> Duration {
        let latest = seen.iter().map(|one| one.entered).max();
        let earliest = seen.iter().filter_map(|one| one.left).min();
        match (latest, earliest) {
            (Some(from), Some(to)) => to.saturating_duration_since(from),
            _ => Duration::ZERO,
        }
    }

    pub async fn run(
        rig: &Rig,
        store: &Store,
        spy: Arc<Spy>,
        request: RunRequest,
    ) -> Result<Result<RunReport, RunError>, Box<dyn Error>> {
        let driver: Arc<dyn AgentDriver> = spy;
        let drivers: Drivers = Arc::new(move |_| Arc::clone(&driver));
        let deps = RunDeps {
            home: rig.home.path(),
            project: rig.project(),
            store,
            drivers,
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let (sink, source) = line_channel(QUEUE_CAP);
        let pump = spawn_pump(source, Channel::new(|_| Ok(())));
        let both = tokio::time::timeout(PATIENCE, async {
            tokio::join!(run_workflow_inner(&deps, &request, sink), pump)
        })
        .await
        .map_err(|_| "the T-114 fixture did not return")?;
        Ok(both.0)
    }

    #[async_trait]
    impl AgentDriver for Spy {
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
            let at = self.enter(&spec);
            (self.effect)(&spec)?;
            let answer = (self.reply)(&spec);
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
            Ok(Box::new(Turn {
                spy: Arc::new(self.clone_for_turn()),
                at,
                answer,
                events,
                session,
            }))
        }
    }

    impl Spy {
        fn clone_for_turn(&self) -> Self {
            Self {
                seen: Arc::clone(&self.seen),
                effect: Arc::clone(&self.effect),
                reply: Arc::clone(&self.reply),
            }
        }
    }

    #[derive(Debug)]
    struct Turn {
        spy: Arc<Spy>,
        at: usize,
        answer: String,
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
            tokio::time::sleep(TURN).await;
            self.spy.leave(self.at);
            let outcome = TurnOutcome {
                ok: true,
                reason: FinishReason::Completed,
                text: self.answer.clone(),
                cost_usd: None,
                tokens: Tokens::default(),
                turns: 1,
                took: TURN,
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

    pub fn git(at: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
        let output = Command::new("git")
            .arg("-C")
            .arg(at)
            .args(["-c", "user.name=Loadout Test"])
            .args(["-c", "user.email=test@loadout.invalid"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr)
                .trim()
                .to_owned()
                .into());
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}
