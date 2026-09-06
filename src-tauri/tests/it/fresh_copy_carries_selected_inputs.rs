//! WF-14: wybór wejścia jest konsumowany przez prawdziwy snapshot/layout przed startem.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{line_channel, spawn_pump};
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::mpsc;

const INPUT: &str = "WF14-EXPLICIT-LOCAL-INPUT";
const AGENT: &str = "01990000-0000-7000-8000-000000000014";

#[tokio::test]
async fn both_git_copies_receive_selected_untracked_input_and_tracked_wip_not_unselected_env()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new(true)?;
    bench.run(json!(["fixtures/local*.json"])).await?;
    let seen = bench.seen.lock().unwrap_or_else(PoisonError::into_inner);
    assert_eq!(seen.len(), 2);
    for one in seen.iter() {
        assert_eq!(one.tracked.as_deref(), Some("tracked WIP"));
        assert_eq!(
            one.selected.as_deref(),
            Some(INPUT),
            "the workflow ignored the explicit additional input"
        );
        assert!(
            !one.secret,
            "unselected private input was copied automatically"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_plain_folder_does_not_automatically_copy_an_unselected_env_file()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new(false)?;
    bench.run(json!([])).await?;
    let seen = bench.seen.lock().unwrap_or_else(PoisonError::into_inner);
    assert_eq!(seen.len(), 2);
    for one in seen.iter() {
        assert_eq!(one.tracked.as_deref(), Some("tracked WIP"));
        assert!(
            !one.secret,
            "a plain folder silently included .env without an explicit choice"
        );
    }
    Ok(())
}

#[tokio::test]
async fn invalid_selection_refuses_before_any_agent_and_names_the_input()
-> Result<(), Box<dyn Error>> {
    for pattern in [
        "missing/*.json",
        "../outside.json",
        "/etc/passwd",
        "fixtures/outside.json",
    ] {
        let bench = Bench::new(true)?;
        fs::write(bench.root.path().join("outside.json"), "outside")?;
        loadout_lib::engine::supervisor::link(
            Path::new("../../outside.json"),
            &bench.project().join("fixtures/outside.json"),
        )?;
        let answer = bench.run(json!([pattern])).await;
        assert!(
            answer.is_err(),
            "invalid selected input did not refuse Start: {pattern}"
        );
        assert!(
            bench
                .seen
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_empty(),
            "a process started despite invalid selected input"
        );
        assert!(
            answer
                .err()
                .ok_or("checked above")?
                .to_string()
                .contains(pattern),
            "the refusal does not identify the selected input"
        );
    }
    Ok(())
}

#[tokio::test]
async fn preparation_precedes_consumers_in_the_same_copy_and_is_not_a_hidden_stage()
-> Result<(), Box<dyn Error>> {
    for (with_preparation, fail) in [(true, false), (false, false), (true, true)] {
        let bench = Bench::new(false)?;
        // Node's own test process executes the assertion and reports the counted passes.
        // There is no printed pretend pass and no package-manager/network dependency.
        fs::write(
            bench.project().join("prepare.cjs"),
            format!(
                r"
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
test('the explicit preparation made this working copy ready', () => {{
  fs.mkdirSync('node_modules', {{ recursive: true }});
  fs.writeFileSync('node_modules/.wf14-ready', fs.realpathSync('.'));
  assert.equal(fs.readFileSync('node_modules/.wf14-ready', 'utf8'), {});
}});
",
                if fail {
                    "'wrong folder'"
                } else {
                    "fs.realpathSync('.')"
                }
            ),
        )?;
        let mut steps = Vec::new();
        let mut links = Vec::new();
        for suffix in ["a", "b"] {
            if with_preparation {
                steps.push(json!({"kind":"check","id":format!("prepare-{suffix}"),"name":format!("Prepare {suffix}"),
                    "command":"node --test --test-reporter=tap prepare.cjs","proof":"# pass (\\d+)","whenItFails":"stop",
                    "folder":{"use":"fresh-copy"},"at":{"x":0,"y":0}}));
                links.push(
                    json!({"from":format!("prepare-{suffix}"),"to":format!("consume-{suffix}")}),
                );
            }
            steps.push(json!({"kind":"agent","id":format!("consume-{suffix}"),"name":format!("Consume {suffix}"),"agent":AGENT,
                "copies":1,"instructions":"Read prepared files","folder":{"use":if with_preparation {"same-copy"} else {"fresh-copy"}},"at":{"x":0,"y":0}}));
        }
        let report = bench.run_document(json!({"format":1,"id":"wf14-prepare","name":"Explicit preparation","steps":steps,"links":links})).await?;
        let seen = bench.seen.lock().unwrap_or_else(PoisonError::into_inner);
        if fail {
            assert!(
                seen.is_empty(),
                "a consumer started after the real preparation assertion failed"
            );
        } else {
            assert_eq!(
                seen.len(),
                2,
                "preparation run: {}",
                fs::read_to_string(report.dir.join("run.json"))?
            );
            assert_ne!(
                seen[0].cwd, seen[1].cwd,
                "independent preparation branches shared one tree"
            );
            for one in seen.iter() {
                if with_preparation {
                    assert_eq!(
                        one.prepared.as_deref(),
                        one.cwd.to_str(),
                        "the consumer did not inherit the prepared copy"
                    );
                } else {
                    assert!(
                        one.prepared.is_none(),
                        "removing the graph node did not remove preparation"
                    );
                }
            }
        }
        assert!(
            !bench.project().join("node_modules").exists(),
            "preparation changed the host project"
        );
    }
    Ok(())
}

struct Bench {
    root: TempDir,
    seen: Arc<Mutex<Vec<ReadBack>>>,
}

impl Bench {
    fn new(git_repo: bool) -> Result<Self, Box<dyn Error>> {
        let bench = Self {
            root: tempfile::tempdir()?,
            seen: Arc::new(Mutex::new(Vec::new())),
        };
        fs::create_dir_all(bench.home().join("agents"))?;
        fs::create_dir_all(bench.home().join("workflows"))?;
        fs::create_dir_all(bench.project().join(".loadout"))?;
        fs::create_dir_all(bench.project().join("fixtures"))?;
        fs::write(bench.project().join("tracked.txt"), "committed")?;
        fs::write(bench.project().join(".gitignore"), ".env\n.loadout/\n")?;
        if git_repo {
            git(&bench.project(), &["init", "--quiet"])?;
            git(&bench.project(), &["add", "tracked.txt", ".gitignore"])?;
            git(
                &bench.project(),
                &[
                    "-c",
                    "user.name=WF14 Fixture",
                    "-c",
                    "user.email=wf14@example.invalid",
                    "commit",
                    "--quiet",
                    "-m",
                    "input",
                ],
            )?;
        }
        fs::write(bench.project().join("tracked.txt"), "tracked WIP")?;
        fs::write(bench.project().join("fixtures/local-input.json"), INPUT)?;
        fs::write(bench.project().join(".env"), "PRIVATE_VALUE=must-not-copy")?;
        fs::write(
            bench.home().join("agents/reader.md"),
            format!(
                "---\nschema: 1\nid: {AGENT}\nname: Reader\nsummary: Reads inputs\ncolor: slate\nrunsWith: claude-code\nmodel: opus\nthinking: balanced\nfileAccess: look-only\ngiveUpAfterMinutes: 20\nwriteResultsTo: \"\"\ntools: everything\nskills: []\nconnections: []\n---\nRead the selected files.\n"
            ),
        )?;
        Ok(bench)
    }
    fn home(&self) -> PathBuf {
        self.root.path().join("home")
    }
    fn project(&self) -> PathBuf {
        self.root.path().join("project")
    }
    async fn run(&self, additional: Value) -> Result<(), Box<dyn Error>> {
        self.run_document(json!({"format":1,"id":"wf14","name":"Selected input","additionalInputs":additional,"links":[],"steps":[{
            "kind":"agent","id":"reader","name":"Read selected files","agent":AGENT,"copies":2,
            "instructions":"Read the requested input","folder":{"use":"fresh-copy"},"at":{"x":0,"y":0}
        }]})).await.map(|_| ())
    }
    async fn run_document(
        &self,
        document: Value,
    ) -> Result<loadout_lib::commands::RunReport, Box<dyn Error>> {
        let workflow = self.home().join("workflows/input.json");
        fs::write(&workflow, document.to_string())?;
        let driver: Arc<dyn AgentDriver> = Arc::new(Reader {
            seen: Arc::clone(&self.seen),
            project: self.project(),
        });
        let drivers: Drivers = Arc::new(move |_| Arc::clone(&driver));
        let store = Store::open(&self.project().join(".loadout/loadout.db"))?;
        let deps = RunDeps {
            home: &self.home(),
            project: &self.project(),
            store: &store,
            drivers,
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow,
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        };
        let (sink, source) = line_channel(256);
        let pump = spawn_pump(source, tauri::ipc::Channel::new(|_| Ok(())));
        let answer = tokio::time::timeout(
            Duration::from_secs(30),
            run_workflow_inner(&deps, &request, sink),
        )
        .await?;
        tokio::time::timeout(Duration::from_secs(30), pump).await??;
        Ok(answer?)
    }
}

#[derive(Debug)]
struct ReadBack {
    tracked: Option<String>,
    selected: Option<String>,
    secret: bool,
    prepared: Option<String>,
    cwd: PathBuf,
}
struct Reader {
    seen: Arc<Mutex<Vec<ReadBack>>>,
    project: PathBuf,
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
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(ReadBack {
                tracked: fs::read_to_string(spec.cwd.join("tracked.txt")).ok(),
                selected: fs::read_to_string(spec.cwd.join("fixtures/local-input.json")).ok(),
                secret: spec.cwd.join(".env").exists(),
                prepared: fs::read_to_string(spec.cwd.join("node_modules/.wf14-ready")).ok(),
                cwd: fs::canonicalize(&spec.cwd)?,
            });
        // Edycja źródła po pierwszym starcie nie może zmienić wejścia drugiej kopii.
        fs::write(
            self.project.join("fixtures/local-input.json"),
            "edited after the first process",
        )?;
        Ok(Box::new(Turn {
            events,
            session: SessionRef {
                vendor: self.id(),
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
    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }
    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        let outcome = Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "Read files".to_owned(),
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
fn git(project: &Path, args: &[&str]) -> Result<(), Box<dyn Error>> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(project)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(())
}
