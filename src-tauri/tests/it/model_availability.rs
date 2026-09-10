use anyhow::Context;
use async_trait::async_trait;
use loadout_lib::{
    commands::{Drivers, RunControl, RunDeps, RunRequest},
    engine::drivers::{
        AgentDriver, AgentHandle, DecodedEvent, DriverConfiguration, Probe, RunSpec,
        models::{self, ModelCatalog},
    },
    library::agents::Agent,
};
use serde_json::json;
use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::sync::mpsc;

#[test]
fn catalog_rejects_the_incident_and_unsupported_effort() -> anyhow::Result<()> {
    let catalog = models::parse_models(
        &[
            json!({"model":"gpt-6-astra","displayName":"Astra","isDefault":true,"supportedReasoningEfforts":[{"reasoningEffort":"high"}]}),
        ],
        true,
    )?;
    assert!(catalog.check(Some("gpt-6"), "high").is_err());
    assert!(catalog.check(Some("gpt-6-astra"), "high").is_ok());
    assert!(catalog.check(Some("gpt-6-astra"), "ultra").is_err());
    assert!(models::parse_models(&[], true).is_err());
    Ok(())
}
#[test]
fn claude_catalog_preserves_resolved_names_and_published_aliases() -> anyhow::Result<()> {
    let catalog = models::parse_models(
        &[json!({"value":"opus[1m]","resolvedModel":"claude-opus-5[1m]","displayName":"Opus"})],
        false,
    )?;
    for id in ["opus", "opus[1m]", "claude-opus-5", "claude-opus-5[1m]"] {
        catalog.check(Some(id), "high")?;
    }
    assert!(catalog.check(Some("opus4"), "high").is_err());
    models::parse_models(&[json!({"value":"default"})], false)?.check(Some("inherit"), "medium")?;
    Ok(())
}
struct Checked {
    starts: Arc<AtomicUsize>,
}
#[async_trait]
impl AgentDriver for Checked {
    fn id(&self) -> &'static str {
        "codex"
    }
    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("test".into()),
        })
    }
    async fn model_catalog(&self) -> anyhow::Result<Option<ModelCatalog>> {
        Ok(Some(models::parse_models(
            &[json!({"model":"good","isDefault":true})],
            true,
        )?))
    }
    async fn start(
        &self,
        _: RunSpec,
        _: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        anyhow::bail!("A step started before all models were checked")
    }
}
#[tokio::test]
async fn last_step_model_refuses_the_whole_workflow_before_the_first_start() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let project = temp.path().join("project");
    let home = temp.path().join("library");
    fs::create_dir_all(project.join(".loadout"))?;
    fs::create_dir_all(home.join("agents"))?;
    let mut agent = Agent::example();
    agent.model = "good".into();
    agent.skills.clear();
    agent.connections.clear();
    agent.runs_with = loadout_lib::library::agents::Vendor::Codex;
    loadout_lib::commands::agents::save_agent_inner(&home, &agent, None)?;
    let path = temp.path().join("workflow.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({"format":1,"id":"models","name":"Models","steps":[
 {"kind":"agent","id":"first","name":"First","agent":agent.id.to_string(),"instructions":"Work","folder":{"use":"project"}},
 {"kind":"agent","id":"last","name":"Final plan","agent":agent.id.to_string(),"instructions":"Plan","folder":{"use":"project"},"overrides":{"model":"gpt-6"}}
 ],"links":[{"from":"first","to":"last"}]}))?,
    )?;
    let starts = Arc::new(AtomicUsize::new(0));
    let d = Arc::new(Checked {
        starts: starts.clone(),
    });
    let drivers: Drivers = Arc::new(move |_| d.clone());
    let store = loadout_lib::store::Store::open(&project.join(".loadout/loadout.db"))?;
    let deps = RunDeps {
        home: &home,
        library: home.clone(),
        project: &project,
        store: &store,
        drivers,
        control: RunControl::new(),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
    };
    let request = RunRequest {
        workflow: path,
        how_many_at_once: 1,
        task: Some("work".into()),
        part: None,
        handoffs_from: None,
    };
    let (sink, _source) = loadout_lib::ipc::line_channel(4096);
    let result = loadout_lib::commands::run::run_workflow_inner(&deps, &request, sink).await;
    let error = result
        .err()
        .context("invalid last model must refuse")?
        .to_string();
    assert!(
        error.contains("Final plan") && error.contains("gpt-6"),
        "{error}"
    );
    assert_eq!(starts.load(Ordering::SeqCst), 0);
    Ok(())
}
#[tokio::test]
async fn real_catalog_process_follows_pagination_without_starting_a_turn() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let binary = temp.path().join("fake-codex");
    fs::write(
        &binary,
        r"#!/usr/bin/env python3
import sys,json
for line in sys.stdin:
 r=json.loads(line);m=r.get('method')
 if m=='initialize': print(json.dumps({'id':r['id'],'result':{}}),flush=True)
 elif m=='config/read': print(json.dumps({'id':r['id'],'result':{'config':{'model':'first'}}}),flush=True)
 elif m=='model/list':
  page=r['params'].get('cursor')
  print(json.dumps({'id':r['id'],'result':{'data':[{'model':'last' if page else 'first','isDefault':not page}],'nextCursor':None if page else 'next'}}),flush=True)
 elif m!='initialized': raise SystemExit(9)
",
    )?;
    loadout_lib::engine::supervisor::set_executable_file(&fs::File::open(&binary)?, true)?;
    let result = models::discover(&binary, &DriverConfiguration::default(), true).await?;
    assert_eq!(
        result
            .models
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "last"]
    );
    Ok(())
}

#[tokio::test]
#[ignore = "Requires installed signed-in vendor CLIs; lists metadata without a model turn"]
async fn live_catalogs_from_installed_apps() -> anyhow::Result<()> {
    for driver in [
        Arc::new(loadout_lib::engine::drivers::codex::CodexDriver::new()) as Arc<dyn AgentDriver>,
        Arc::new(loadout_lib::engine::drivers::claude::ClaudeDriver::new()) as Arc<dyn AgentDriver>,
    ] {
        let catalog = driver
            .model_catalog()
            .await?
            .ok_or_else(|| anyhow::anyhow!("no catalog"))?;
        println!(
            "{}: {:?}",
            driver.id(),
            catalog
                .models
                .iter()
                .filter(|m| !m.hidden)
                .map(|m| &m.id)
                .collect::<Vec<_>>()
        );
        assert!(!catalog.models.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn unresponsive_catalog_times_out_and_proves_its_group_dead() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let binary = temp.path().join("silent-cli");
    let address_file = temp.path().join("pid");
    fs::write(
        &binary,
        r"#!/usr/bin/env python3
import os,time,signal
with open(os.environ['MODEL_TEST_PID'],'w') as f: f.write(str(os.getpgrp()))
signal.signal(signal.SIGTERM, signal.SIG_IGN)
while True: time.sleep(1)
",
    )?;
    loadout_lib::engine::supervisor::set_executable_file(&fs::File::open(&binary)?, true)?;
    let config = DriverConfiguration {
        environment: vec![("MODEL_TEST_PID".into(), address_file.as_os_str().to_owned())],
        ..Default::default()
    };
    let result = models::discover(&binary, &config, true).await;
    assert!(
        result
            .err()
            .context("unresponsive CLI cannot pass")?
            .to_string()
            .contains("timed out")
    );
    let pgid = fs::read_to_string(address_file)?.parse::<i32>()?;
    assert!(
        loadout_lib::engine::supervisor::group_is_empty(pgid),
        "probe survived its timeout"
    );
    Ok(())
}
