//! WF-28: usunięcie żywego procesu nie odbiera przyczyny i ograniczonego ogona.

#![allow(clippy::too_many_lines)]
#![allow(clippy::default_trait_access, clippy::unreadable_literal)]
use std::error::Error;
use std::fs;
use std::sync::Arc;
use std::time::Duration;

use loadout_lib::bridge::host::Bridge;
use loadout_lib::bridge::{Answer, Call, Greeting, Reply};
use loadout_lib::commands::processes::services::ServiceAccess;
use loadout_lib::commands::processes::{Processes, ServiceOwner, ServiceRef};
use loadout_lib::engine::supervisor::{self, GroupProof, StepTag};
use loadout_lib::library::agents::{ServiceGrant, ServiceOperation};
use loadout_lib::workflow::TargetKind;
use loadout_lib::workflow::{LaunchDescription, ServiceLifetime};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_read_only_consumer_can_read_the_last_error_and_exit_reason_after_natural_death()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let socket = tempfile::tempdir()?;
    let workspace = fs::canonicalize(project.path())?;
    let run = uuid::Uuid::now_v7().to_string();
    let run_dir = workspace.join(".loadout/runs").join(&run);
    let cwd = run_dir.join("work/s_preview");
    fs::create_dir_all(&cwd)?;
    let processes = Arc::new(Processes::new());
    let reference = ServiceRef {
        workspace: workspace.clone(),
        run_id: run.clone(),
        node_key: "s_preview".to_owned(),
        service_id: uuid::Uuid::now_v7().to_string(),
        generation: 1,
    };
    let started = processes.start_owned_description(
        &LaunchDescription {
            command: "printf 'The app could not load its entry point.\\n' >&2; exit 7".to_owned(),
            kind: TargetKind::default(),
            test_data_env: None,
            subdirectory: String::new(),
            environment: Default::default(),
            required_env: Vec::new(),
            endpoints: Vec::new(),
            readiness: None,
        },
        StepTag::new(&run, "s_preview"),
        ServiceOwner {
            reference: reference.clone(),
            run_dir,
            cwd,
            lifetime: ServiceLifetime::Window,
        },
    )?;
    let observed = async {
        tokio::time::timeout(Duration::from_secs(3), async {
            while !processes.list().is_empty() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await?;
        let access = Arc::new(ServiceAccess::for_step(
            Arc::clone(&processes),
            workspace,
            run,
            vec![ServiceGrant {
                service: "s_preview".to_owned(),
                operations: vec![ServiceOperation::Read],
            }],
            CancellationToken::new(),
        ));
        let bridge = Bridge::open_with_tools(socket.path(), access.tools(), access).await?;
        let logs = call(
            &bridge,
            "service_logs",
            json!({"service":reference,"limit":128}),
        )
        .await?;
        let status = call(&bridge, "service_status", json!({"service":reference})).await?;
        let refused = call(
            &bridge,
            "service_logs",
            json!({"service":reference,"limit":1000000}),
        )
        .await?;
        Ok::<_, Box<dyn Error>>((logs, status, refused))
    }
    .await;
    let cleanup = processes.close().await;
    assert!(
        cleanup
            .iter()
            .all(|one| matches!(one, GroupProof::Dead { .. })),
        "cleanup: {cleanup:?}"
    );
    assert!(supervisor::group_is_empty(started.pgid));
    let (logs, status, refused) = observed?;
    let Answer::Ok(logs) = logs else {
        return Err(format!("missing final log: {logs:?}").into());
    };
    assert!(
        logs["text"]
            .as_str()
            .is_some_and(|text| text.contains("could not load its entry point")),
        "{logs}"
    );
    assert!(
        matches!(refused, Answer::Refused(_)),
        "unbounded output was accepted: {refused:?}"
    );
    let Answer::Ok(status) = status else {
        return Err(format!("missing final status: {status:?}").into());
    };
    assert_eq!(
        status["exitCode"], 7,
        "the host lost the real exit code after removing the live process: {status}"
    );
    assert!(
        status["exitReason"]
            .as_str()
            .is_some_and(|text| text.contains("code 7")),
        "no human-readable reason: {status}"
    );
    Ok(())
}

async fn call(bridge: &Bridge, name: &str, input: Value) -> Result<Answer, Box<dyn Error>> {
    tokio::time::timeout(Duration::from_secs(3), async {
        let (reader, mut writer) = UnixStream::connect(bridge.at()).await?.into_split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        let _: Greeting = serde_json::from_str(&line)?;
        writer
            .write_all(
                format!(
                    "{}\n",
                    serde_json::to_string(&Call {
                        id: json!(1),
                        call: name.to_owned(),
                        input
                    })?
                )
                .as_bytes(),
            )
            .await?;
        writer.flush().await?;
        line.clear();
        reader.read_line(&mut line).await?;
        let reply: Reply = serde_json::from_str(&line)?;
        Ok::<_, Box<dyn Error>>(reply.answer)
    })
    .await?
}
