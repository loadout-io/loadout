//! WF-28: ukrycie nazwy w tools/list nie jest odmową bezpośredniego wywołania.
//! Łączymy się z rzeczywistym hostem i rzeczywistym Desk, bez atrapy dispatchera.
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use loadout_lib::bridge::host::Bridge;
use loadout_lib::bridge::library::{Desk, Waiting};
use loadout_lib::bridge::{Answer, Call, Greeting, Reply, Role};
use loadout_lib::commands::processes::services::ServiceAccess;
use loadout_lib::commands::processes::{Processes, ServiceOwner, ServiceRef};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::supervisor::{self, GroupProof, StepTag};
use loadout_lib::ipc::{LineSource, line_channel};
use loadout_lib::library::agents::{Agent, Overrides, ServiceGrant, ServiceOperation, resolve};
use loadout_lib::workflow::{LaunchDescription, ServiceLifetime};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio_util::sync::CancellationToken;

fn grant(service: &str, operations: &[ServiceOperation]) -> ServiceGrant {
    ServiceGrant {
        service: service.to_owned(),
        operations: operations.to_vec(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn simultaneous_sessions_have_distinct_real_bridge_sockets() -> Result<(), Box<dyn Error>> {
    let sockets = tempfile::tempdir()?;
    let mut opening = tokio::task::JoinSet::new();
    for _ in 0..256 {
        let root = sockets.path().to_path_buf();
        opening.spawn(async move {
            let desk = Arc::new(Desk::at(None, root.clone()));
            Bridge::open(&root, Role::Step, desk).await
        });
    }
    let mut bridges = Vec::new();
    let mut errors = Vec::new();
    while let Some(opened) = opening.join_next().await {
        match opened? {
            Ok(bridge) => bridges.push(bridge),
            Err(error) => errors.push(error.to_string()),
        }
    }
    let unique: std::collections::BTreeSet<_> = bridges
        .iter()
        .map(|bridge| bridge.at().to_path_buf())
        .collect();
    let opened = bridges.len();
    // All sockets belong to this fixture, and dropping each host closes its accept task.
    drop(bridges);
    assert_eq!(
        opened, 256,
        "concurrent Step/Lead host creation collided: {errors:?}"
    );
    assert_eq!(
        unique.len(),
        256,
        "two live sessions shared an app-tool socket"
    );
    Ok(())
}

#[test]
fn a_step_override_cannot_add_an_app_or_an_operation_the_agent_was_not_given()
-> Result<(), Box<dyn Error>> {
    let mut agent = Agent::example();
    agent.service_access = vec![grant("s_preview", &[ServiceOperation::Read])];
    for service_access in [
        vec![grant(
            "s_preview",
            &[ServiceOperation::Read, ServiceOperation::Stop],
        )],
        vec![grant("s_foreign", &[ServiceOperation::Read])],
    ] {
        let patch = Overrides {
            service_access: Some(service_access),
            ..Overrides::default()
        };
        assert!(
            resolve(&agent, &patch).is_err(),
            "the step widened its agent's app permissions"
        );
    }
    let narrowed = resolve(
        &agent,
        &Overrides {
            service_access: Some(Vec::new()),
            ..Overrides::default()
        },
    )?;
    assert!(narrowed.agent.service_access.is_empty());
    Ok(())
}

struct ServiceBench {
    _project: tempfile::TempDir,
    workspace: std::path::PathBuf,
    processes: Arc<Processes>,
    references: Vec<(ServiceRef, i32)>,
}

impl ServiceBench {
    fn configure(&self, run: &str, node: &str) -> Result<ServiceRef, Box<dyn Error>> {
        let run_dir = self.workspace.join(".loadout/runs").join(run);
        let cwd = run_dir.join("work").join(node);
        std::fs::create_dir_all(&cwd)?;
        Ok(self.processes.configure_description(
            &LaunchDescription {
                command: "printf 'app started\\n'; sleep 10".to_owned(),
                kind: Default::default(),
                test_data_env: None,
                subdirectory: String::new(),
                environment: Default::default(),
                required_env: Vec::new(),
                endpoints: Vec::new(),
                readiness: None,
            },
            StepTag::new(run, node),
            ServiceOwner {
                reference: ServiceRef {
                    workspace: self.workspace.clone(),
                    run_id: run.to_owned(),
                    node_key: node.to_owned(),
                    service_id: uuid::Uuid::now_v7().to_string(),
                    generation: 1,
                },
                run_dir,
                cwd,
                lifetime: ServiceLifetime::Window,
            },
        )?)
    }
    fn new() -> Result<Self, Box<dyn Error>> {
        let project = tempfile::tempdir()?;
        let workspace = std::fs::canonicalize(project.path())?;
        Ok(Self {
            _project: project,
            workspace,
            processes: Arc::new(Processes::new()),
            references: Vec::new(),
        })
    }

    fn start(&mut self, run: &str, node: &str) -> Result<ServiceRef, Box<dyn Error>> {
        let run_dir = self.workspace.join(".loadout/runs").join(run);
        let cwd = run_dir.join("work").join(node);
        std::fs::create_dir_all(&cwd)?;
        let reference = ServiceRef {
            workspace: self.workspace.clone(),
            run_id: run.to_owned(),
            node_key: node.to_owned(),
            service_id: uuid::Uuid::now_v7().to_string(),
            generation: 1,
        };
        let started = self.processes.start_owned_description(
            &LaunchDescription {
                command: "printf 'app started\\n'; sleep 10".to_owned(),
                kind: Default::default(),
                test_data_env: None,
                subdirectory: String::new(),
                environment: Default::default(),
                required_env: Vec::new(),
                endpoints: Vec::new(),
                readiness: None,
            },
            StepTag::new(run, node),
            ServiceOwner {
                reference: reference.clone(),
                run_dir,
                cwd,
                lifetime: ServiceLifetime::Window,
            },
        )?;
        self.references.push((reference.clone(), started.pgid));
        Ok(reference)
    }

    async fn close(&self) {
        let active: Vec<_> = self.processes.list().iter().map(|one| one.pgid).collect();
        let proofs = self.processes.close().await;
        assert!(
            proofs
                .iter()
                .all(|proof| matches!(proof, GroupProof::Dead { .. })),
            "fixture cleanup: {proofs:?}"
        );
        assert!(
            self.references
                .iter()
                .all(|(_, pgid)| supervisor::group_is_empty(*pgid)),
            "fixture left its own group alive"
        );
        assert!(
            active.iter().all(|pgid| supervisor::group_is_empty(*pgid)),
            "fixture left a tool-started group alive"
        );
    }

    async fn access(
        &self,
        run: &str,
        grants: Vec<ServiceGrant>,
        expires: CancellationToken,
    ) -> Result<(tempfile::TempDir, Bridge), Box<dyn Error>> {
        let socket = tempfile::tempdir()?;
        let access = Arc::new(ServiceAccess::for_step(
            Arc::clone(&self.processes),
            self.workspace.clone(),
            run.to_owned(),
            grants,
            expires,
        ));
        let bridge = Bridge::open_with_tools(socket.path(), access.tools(), access).await?;
        Ok((socket, bridge))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_host_starts_only_the_frozen_app_description() -> Result<(), Box<dyn Error>> {
    let bench = ServiceBench::new()?;
    let run = uuid::Uuid::now_v7().to_string();
    let reference = bench.configure(&run, "s_preview")?;
    let observed = async {
        let (_socket, bridge) = bench
            .access(
                &run,
                vec![grant(
                    "s_preview",
                    &[ServiceOperation::Read, ServiceOperation::Start],
                )],
                CancellationToken::new(),
            )
            .await?;
        let before = service_call(&bridge, "service_status", json!({"service":reference})).await?;
        let forged = service_call(
            &bridge,
            "service_start",
            json!({"service":reference,"command":"touch forbidden-command"}),
        )
        .await?;
        let started = service_call(&bridge, "service_start", json!({"service":reference})).await?;
        let groups = bench.processes.list();
        Ok::<_, Box<dyn Error>>((before, forged, started, groups))
    }
    .await;
    bench.close().await;
    let (before, forged, started, groups) = observed?;
    assert!(
        matches!(&before, Answer::Ok(value) if value["pgid"].is_null() && value["status"] == "Configured, not started")
    );
    assert!(
        matches!(forged, Answer::Refused(_)),
        "model-selected command was accepted: {forged:?}"
    );
    assert!(
        matches!(&started, Answer::Ok(value) if value["service"] == json!(reference) && value["pgid"].as_i64().is_some_and(|id| id > 0)),
        "the real host did not start the configured app: {started:?}"
    );
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].command, "printf 'app started\\n'; sleep 10");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn racing_restarts_create_one_generation_and_a_stale_stop_cannot_reach_it()
-> Result<(), Box<dyn Error>> {
    let mut bench = ServiceBench::new()?;
    let run = uuid::Uuid::now_v7().to_string();
    let reference = bench.start(&run, "s_preview")?;
    let observed = async {
        let (_socket, bridge) = bench
            .access(
                &run,
                vec![grant(
                    "s_preview",
                    &[
                        ServiceOperation::Read,
                        ServiceOperation::Restart,
                        ServiceOperation::Stop,
                    ],
                )],
                CancellationToken::new(),
            )
            .await?;
        let (one, two) = tokio::join!(
            service_call(&bridge, "service_restart", json!({"service":reference})),
            service_call(&bridge, "service_restart", json!({"service":reference}))
        );
        let replies = [one?, two?];
        let stale = service_call(&bridge, "service_stop", json!({"service":reference})).await?;
        let groups = bench.processes.list();
        let old_dead = supervisor::group_is_empty(bench.references[0].1);
        Ok::<_, Box<dyn Error>>((replies, stale, groups, old_dead))
    }
    .await;
    bench.close().await;
    let (replies, stale, groups, old_dead) = observed?;
    assert_eq!(
        replies
            .iter()
            .filter(|reply| matches!(reply, Answer::Ok(_)))
            .count(),
        1,
        "restart did not have one winner: {replies:?}"
    );
    assert_eq!(
        replies
            .iter()
            .filter(|reply| matches!(reply, Answer::Refused(_)))
            .count(),
        1
    );
    assert!(
        matches!(stale, Answer::Refused(_)),
        "old-generation Stop was accepted: {stale:?}"
    );
    assert!(
        old_dead,
        "new generation appeared without death proof for the old one"
    );
    assert_eq!(groups.len(), 1);
    assert!(
        groups[0]
            .service
            .as_ref()
            .is_some_and(|next| next.service_id == reference.service_id && next.generation == 2)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_stopped_apps_limited_log_can_still_be_read_in_pages() -> Result<(), Box<dyn Error>> {
    let mut bench = ServiceBench::new()?;
    let run = uuid::Uuid::now_v7().to_string();
    let reference = bench.start(&run, "s_preview")?;
    let observed = async {
        tokio::time::timeout(Duration::from_secs(2), async {
            while bench
                .processes
                .said(bench.references[0].1)
                .is_none_or(|text| !text.contains("app started"))
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await?;
        let (_socket, bridge) = bench
            .access(
                &run,
                vec![grant(
                    "s_preview",
                    &[ServiceOperation::Read, ServiceOperation::Stop],
                )],
                CancellationToken::new(),
            )
            .await?;
        let stopped = service_call(&bridge, "service_stop", json!({"service":reference})).await?;
        let first = service_call(
            &bridge,
            "service_logs",
            json!({"service":reference,"offset":0,"limit":4}),
        )
        .await?;
        let next = service_call(
            &bridge,
            "service_logs",
            json!({"service":reference,"offset":4,"limit":4}),
        )
        .await?;
        Ok::<_, Box<dyn Error>>((stopped, first, next, bench.processes.list().is_empty()))
    }
    .await;
    bench.close().await;
    let (stopped, first, next, removed) = observed?;
    assert!(matches!(&stopped, Answer::Ok(value) if value["status"] == "Stopped"));
    assert!(
        removed,
        "fixture is still a live entry, so this does not test the final tail"
    );
    assert!(
        matches!(&first, Answer::Ok(value) if value["text"] == "app " && value["next"] == 4 && value["status"] == "Stopped"),
        "final safe log page was unavailable: {first:?}"
    );
    assert!(
        matches!(&next, Answer::Ok(value) if value["text"] == "star" && value["next"] == 8),
        "log pagination did not advance: {next:?}"
    );
    Ok(())
}

async fn service_call(
    bridge: &Bridge,
    name: &str,
    input: serde_json::Value,
) -> Result<Answer, Box<dyn Error>> {
    tokio::time::timeout(Duration::from_secs(3), async {
        let connection = UnixStream::connect(bridge.at()).await?;
        let (reader, mut writer) = connection.into_split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        let _: Greeting = serde_json::from_str(&line)?;
        let call = Call {
            id: json!(28),
            call: name.to_owned(),
            input,
        };
        writer
            .write_all(format!("{}\n", serde_json::to_string(&call)?).as_bytes())
            .await?;
        writer.flush().await?;
        line.clear();
        reader.read_line(&mut line).await?;
        let reply: Reply = serde_json::from_str(&line)?;
        Ok::<_, Box<dyn Error>>(reply.answer)
    })
    .await?
}

async fn lead_bridge(
    bench: &ServiceBench,
    waiting: Arc<Waiting>,
    expires: CancellationToken,
) -> Result<(tempfile::TempDir, Bridge, LineSource), Box<dyn Error>> {
    let socket = tempfile::tempdir()?;
    let (sink, source) = line_channel(256);
    let access = Arc::new(ServiceAccess::for_lead(
        Arc::clone(&bench.processes),
        bench.workspace.clone(),
        vec![grant(
            "s_preview",
            &[
                ServiceOperation::Read,
                ServiceOperation::Start,
                ServiceOperation::Restart,
                ServiceOperation::Stop,
            ],
        )],
        expires,
    ));
    let desk = Arc::new(
        Desk::at(None, bench.workspace.clone())
            .hearing(waiting)
            .showing(Arc::new(Mutex::new(sink)))
            .serving_with(access),
    );
    let bridge = Bridge::open_with_tools(socket.path(), desk.tools(), desk).await?;
    Ok((socket, bridge, source))
}

async fn approve_service(
    bridge: &Bridge,
    waiting: &Waiting,
    source: &mut LineSource,
    operation: &str,
    reference: &ServiceRef,
) -> Result<(Answer, Vec<String>), Box<dyn Error>> {
    let request = service_call(
        bridge,
        "ask_the_person",
        json!({"operation":operation,
        "service":reference,"question":"Trust the model without checking", "options":["Fake approval"]}),
    );
    tokio::pin!(request);
    let mut questions = Vec::new();
    loop {
        tokio::select! {
            answer = &mut request => return Ok((answer?, questions)),
            () = tokio::time::sleep(Duration::from_millis(1)) => {
                while let Some(line) = source.try_next() {
                    if let Line::Asked {text, options, question: Some(question), ..} = line {
                        let affirmative = match operation {
                            "service_start" => "Start app",
                            "service_restart" => "Restart app",
                            "service_stop" => "Stop app",
                            _ => return Err("fixture has an unknown operation".into()),
                        };
                        if text.contains("Trust the model") || !text.contains(&reference.node_key)
                            || !text.contains(&reference.run_id) || !options.iter().any(|one| one == affirmative)
                            || question.operation != operation {
                            return Err(format!("confirmation did not bind the real app: {text:?} {options:?}").into());
                        }
                        if waiting.answer("Lead", affirmative.to_owned())
                            || waiting.answer_exact("Lead", "older-question", affirmative.to_owned())
                            || !waiting.answer_exact("Lead", &question.question_id, affirmative.to_owned())
                            || waiting.answer_exact("Lead", &question.question_id, affirmative.to_owned()) {
                            return Err("the human answer was not exact and one-use".into());
                        }
                        questions.push(text);
                    }
                }
            }
        }
    }
}

fn approval_token(answer: &Answer) -> Result<&str, Box<dyn Error>> {
    match answer {
        Answer::Ok(value) => value["approvalToken"]
            .as_str()
            .ok_or_else(|| "no approval token".into()),
        _ => Err(format!("the host did not ask the person: {answer:?}").into()),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lead_app_reads_use_the_real_desk_without_needing_a_library() -> Result<(), Box<dyn Error>>
{
    let mut bench = ServiceBench::new()?;
    let run = uuid::Uuid::now_v7().to_string();
    let reference = bench.start(&run, "s_preview")?;
    let observed = async {
        let expires = CancellationToken::new();
        let (_socket, bridge, mut source) =
            lead_bridge(&bench, Arc::new(Waiting::default()), expires.clone()).await?;
        let status = service_call(&bridge, "service_status", json!({})).await?;
        let logs = service_call(
            &bridge,
            "service_logs",
            json!({"service":reference,"limit":4096}),
        )
        .await?;
        let mut foreign = reference.clone();
        foreign.workspace = foreign.workspace.join("another-workspace");
        let foreign = service_call(
            &bridge,
            "service_stop",
            json!({"service":foreign,"confirmed":true}),
        )
        .await?;
        expires.cancel();
        let expired = service_call(&bridge, "service_status", json!({"service":reference})).await?;
        let mut lines = Vec::new();
        while let Some(line) = source.try_next() {
            lines.push(line);
        }
        Ok::<_, Box<dyn Error>>((status, logs, foreign, expired, lines))
    }
    .await;
    bench.close().await;
    let (status, logs, foreign, expired, lines) = observed?;
    assert!(
        matches!(&status, Answer::Ok(value) if value["services"].as_array().is_some_and(|rows| rows.len() == 1)),
        "Lead could not discover its app: {status:?}"
    );
    assert!(
        matches!(&logs, Answer::Ok(value) if value["text"].as_str().is_some()),
        "Lead could not read app logs: {logs:?}"
    );
    for refusal in [foreign, expired] {
        let Answer::Refused(said) = refusal else {
            return Err(format!("scope escape was accepted: {refusal:?}").into());
        };
        assert!(
            lines
                .iter()
                .any(|line| matches!(line, Line::Problem {text,..} if text == &said)),
            "human did not see the app refusal: {said}"
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lead_start_restart_and_stop_consume_only_the_exact_human_app_approval()
-> Result<(), Box<dyn Error>> {
    let bench = ServiceBench::new()?;
    let run = uuid::Uuid::now_v7().to_string();
    let reference = bench.configure(&run, "s_preview")?;
    let observed = async {
        let waiting = Arc::new(Waiting::default());
        let (_socket, bridge, mut source) =
            lead_bridge(&bench, Arc::clone(&waiting), CancellationToken::new()).await?;
        let forged = service_call(
            &bridge,
            "service_start",
            json!({"service":reference,"confirmed":true}),
        )
        .await?;
        let no_process_before_permission = bench.processes.list().is_empty();
        let (approval, questions) =
            approve_service(&bridge, &waiting, &mut source, "service_start", &reference).await?;
        let token = approval_token(&approval)?;
        let wrong_operation = service_call(
            &bridge,
            "service_stop",
            json!({"service":reference,"approval_token":token}),
        )
        .await?;
        let started = service_call(
            &bridge,
            "service_start",
            json!({"service":reference,"approval_token":token}),
        )
        .await?;
        let reused = service_call(
            &bridge,
            "service_start",
            json!({"service":reference,"approval_token":token}),
        )
        .await?;
        let (old_stop, _) =
            approve_service(&bridge, &waiting, &mut source, "service_stop", &reference).await?;
        let old_token = approval_token(&old_stop)?;
        let (restart, _) = approve_service(
            &bridge,
            &waiting,
            &mut source,
            "service_restart",
            &reference,
        )
        .await?;
        let restarted = service_call(
            &bridge,
            "service_restart",
            json!({"service":reference,"approval_token":approval_token(&restart)?}),
        )
        .await?;
        let next: ServiceRef = match &restarted {
            Answer::Ok(value) => serde_json::from_value(value["service"].clone())?,
            _ => return Err(format!("real approved restart failed: {restarted:?}").into()),
        };
        let stale = service_call(
            &bridge,
            "service_stop",
            json!({"service":next,"approval_token":old_token}),
        )
        .await?;
        let next_survived = bench
            .processes
            .service_status(&next)?
            .pgid
            .is_some_and(|pgid| !supervisor::group_is_empty(pgid));
        let (stop, _) =
            approve_service(&bridge, &waiting, &mut source, "service_stop", &next).await?;
        let stopped = service_call(
            &bridge,
            "service_stop",
            json!({"service":next,"approval_token":approval_token(&stop)?}),
        )
        .await?;
        let all_dead = bench.processes.list().is_empty();
        let mut lines = Vec::new();
        while let Some(line) = source.try_next() {
            lines.push(line);
        }
        Ok::<_, Box<dyn Error>>((
            forged,
            no_process_before_permission,
            wrong_operation,
            started,
            reused,
            questions,
            next,
            stale,
            next_survived,
            stopped,
            all_dead,
            lines,
        ))
    }
    .await;
    bench.close().await;
    let (
        forged,
        no_process,
        wrong,
        started,
        reused,
        questions,
        next,
        stale,
        survived,
        stopped,
        dead,
        lines,
    ) = observed?;
    assert!(
        matches!(forged, Answer::Refused(_)) && no_process,
        "a model's confirmed flag started the app"
    );
    assert!(
        matches!(wrong, Answer::Refused(_))
            && matches!(reused, Answer::Refused(_))
            && matches!(stale, Answer::Refused(_))
    );
    assert_eq!(questions.len(), 1);
    assert!(
        matches!(&started, Answer::Ok(value) if value["pgid"].as_i64().is_some_and(|id| id > 0))
    );
    assert_eq!(next.generation, reference.generation + 1);
    assert!(
        survived,
        "a token for the older app stopped the replacement"
    );
    assert!(
        matches!(&stopped, Answer::Ok(value) if value["status"] == "Stopped") && dead,
        "approved Stop did not prove death: {stopped:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| matches!(line, Line::Note {text,..} if text.contains("stopped"))),
        "the actual Stop result did not reach the person's conversation"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_app_approval_from_another_conversation_cannot_start_the_same_app()
-> Result<(), Box<dyn Error>> {
    let bench = ServiceBench::new()?;
    let run = uuid::Uuid::now_v7().to_string();
    let reference = bench.configure(&run, "s_preview")?;
    let observed = async {
        // Shared storage makes this stronger than two unrelated token registries:
        // only the host-bound conversation identity can refuse the stolen token.
        let waiting = Arc::new(Waiting::default());
        let (_one_socket, one, mut one_lines) =
            lead_bridge(&bench, Arc::clone(&waiting), CancellationToken::new()).await?;
        let (_two_socket, two, mut two_lines) =
            lead_bridge(&bench, Arc::clone(&waiting), CancellationToken::new()).await?;
        let (approval, _) =
            approve_service(&one, &waiting, &mut one_lines, "service_start", &reference).await?;
        let token = approval_token(&approval)?;
        let foreign = service_call(
            &two,
            "service_start",
            json!({"service":reference,"approval_token":token}),
        )
        .await?;
        let was_not_started = bench.processes.list().is_empty();
        let own = service_call(
            &one,
            "service_start",
            json!({"service":reference,"approval_token":token}),
        )
        .await?;
        let mut refusal_lines = Vec::new();
        while let Some(line) = two_lines.try_next() {
            refusal_lines.push(line);
        }
        Ok::<_, Box<dyn Error>>((foreign, own, was_not_started, refusal_lines))
    }
    .await;
    bench.close().await;
    let (foreign, own, was_not_started, lines) = observed?;
    let Answer::Refused(said) = foreign else {
        return Err("another conversation used an app approval".into());
    };
    assert!(was_not_started, "the stolen token started a real process");
    assert!(
        matches!(&own, Answer::Ok(value) if value["pgid"].as_i64().is_some_and(|pgid| pgid > 0)),
        "refusing the foreign scope consumed the valid conversation's approval: {own:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| matches!(line, Line::Problem {text,..} if text == &said)),
        "the person in the other conversation did not see the refusal: {said}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_app_question_expires_when_its_exact_process_is_proved_dead()
-> Result<(), Box<dyn Error>> {
    let mut bench = ServiceBench::new()?;
    let run = uuid::Uuid::now_v7().to_string();
    let reference = bench.start(&run, "s_preview")?;
    let observed = async {
        let waiting = Arc::new(Waiting::default());
        let (_socket, bridge, mut source) = lead_bridge(&bench, Arc::clone(&waiting), CancellationToken::new()).await?;
        let request = service_call(&bridge, "ask_the_person", json!({"operation":"service_stop", "service":reference}));
        tokio::pin!(request);
        let question_id = loop {
            tokio::select! {
                answer = &mut request => return Err(format!("the question ended before it was shown: {:?}", answer?).into()),
                () = tokio::time::sleep(Duration::from_millis(1)) => {
                    if let Some(Line::Asked {question: Some(question), ..}) = source.try_next() {
                        break question.question_id;
                    }
                }
            }
        };
        let proof = bench.processes.stop_service(&reference).await?;
        let expired = request.await?;
        let late_answer = waiting.answer_exact("Lead", &question_id, "Stop app".to_owned());
        let mut lines = Vec::new();
        while let Some(line) = source.try_next() { lines.push(line); }
        Ok::<_, Box<dyn Error>>((proof, expired, late_answer, lines))
    }.await;
    bench.close().await;
    let (proof, expired, late_answer, lines) = observed?;
    assert!(
        matches!(proof, Some(GroupProof::Dead { .. })),
        "the expiry trigger was not real process death: {proof:?}"
    );
    assert!(
        !late_answer,
        "an answer to the expired app question was still accepted"
    );
    let Answer::Refused(said) = expired else {
        return Err(format!("a dead app produced an approval: {expired:?}").into());
    };
    assert!(
        lines
            .iter()
            .any(|line| matches!(line, Line::Problem {text,..} if text == &said)),
        "the person did not see that this app question had expired: {said}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_granted_step_reads_and_stops_its_exact_service_through_the_host()
-> Result<(), Box<dyn Error>> {
    let mut bench = ServiceBench::new()?;
    let run = uuid::Uuid::now_v7().to_string();
    let reference = bench.start(&run, "s_preview")?;
    let observed = async {
        let (_socket, bridge) = bench
            .access(
                &run,
                vec![grant(
                    "s_preview",
                    &[ServiceOperation::Read, ServiceOperation::Stop],
                )],
                CancellationToken::new(),
            )
            .await?;
        let status = service_call(&bridge, "service_status", json!({"service":reference})).await?;
        let stopped = service_call(&bridge, "service_stop", json!({"service":reference})).await?;
        let empty = supervisor::group_is_empty(bench.references[0].1);
        Ok::<_, Box<dyn Error>>((status, stopped, empty))
    }
    .await;
    bench.close().await;
    let (status, stopped, empty) = observed?;
    assert!(
        matches!(&status, Answer::Ok(value) if value["service"] == json!(reference)),
        "the host did not return the bound service: {status:?}"
    );
    assert!(
        matches!(&stopped, Answer::Ok(value) if value["status"] == "Stopped"),
        "the host did not return a proved Stop: {stopped:?}"
    );
    assert!(empty, "tool success was not backed by kernel death proof");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn read_access_does_not_inherit_stop_from_another_granted_app() -> Result<(), Box<dyn Error>>
{
    let mut bench = ServiceBench::new()?;
    let run = uuid::Uuid::now_v7().to_string();
    let reference = bench.start(&run, "s_preview")?;
    let observed = async {
        let (_socket, bridge) = bench
            .access(
                &run,
                vec![
                    grant("s_preview", &[ServiceOperation::Read]),
                    grant("s_other", &[ServiceOperation::Stop]),
                ],
                CancellationToken::new(),
            )
            .await?;
        let status = service_call(&bridge, "service_status", json!({"service":reference})).await?;
        let refused = service_call(&bridge, "service_stop", json!({"service":reference})).await?;
        Ok::<_, Box<dyn Error>>((
            status,
            refused,
            !supervisor::group_is_empty(bench.references[0].1),
        ))
    }
    .await;
    bench.close().await;
    let (status, refused, survived) = observed?;
    assert!(
        matches!(status, Answer::Ok(_)),
        "read access was not usable: {status:?}"
    );
    assert!(
        matches!(refused, Answer::Refused(_)),
        "per-service permission was widened: {refused:?}"
    );
    assert!(survived, "a read-only consumer stopped its shared app");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_same_step_name_in_another_run_is_not_the_same_service() -> Result<(), Box<dyn Error>> {
    let mut bench = ServiceBench::new()?;
    let run = uuid::Uuid::now_v7().to_string();
    let foreign_run = uuid::Uuid::now_v7().to_string();
    let reference = bench.start(&run, "s_preview")?;
    let foreign = bench.start(&foreign_run, "s_preview")?;
    let observed = async {
        let (_socket, bridge) = bench
            .access(
                &run,
                vec![grant(
                    "s_preview",
                    &[ServiceOperation::Read, ServiceOperation::Stop],
                )],
                CancellationToken::new(),
            )
            .await?;
        let status = service_call(&bridge, "service_status", json!({"service":reference})).await?;
        let read = service_call(&bridge, "service_status", json!({"service":foreign})).await?;
        let stop = service_call(&bridge, "service_stop", json!({"service":foreign})).await?;
        Ok::<_, Box<dyn Error>>((
            status,
            read,
            stop,
            !supervisor::group_is_empty(bench.references[1].1),
        ))
    }
    .await;
    bench.close().await;
    let (status, read, stop, survived) = observed?;
    assert!(
        matches!(status, Answer::Ok(_)),
        "the fixture could not read its own app: {status:?}"
    );
    assert!(
        matches!(read, Answer::Refused(_)),
        "another run's status escaped: {read:?}"
    );
    assert!(
        matches!(stop, Answer::Refused(_)),
        "another run's stop escaped: {stop:?}"
    );
    assert!(survived, "another run's app was stopped");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_app_configured_for_an_agent_is_not_already_started_by_the_graph()
-> Result<(), Box<dyn Error>> {
    use loadout_lib::commands::processes::Processes;
    use loadout_lib::commands::run::run_workflow_inner;
    use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
    use loadout_lib::engine::drivers::{AgentDriver, absent::Absent};
    use loadout_lib::engine::supervisor::{self, GroupProof};
    use loadout_lib::ipc::line_channel;
    use loadout_lib::store::Store;

    let project = tempfile::tempdir()?;
    let library = tempfile::tempdir()?;
    std::fs::create_dir_all(project.path().join(".loadout"))?;
    let workflow = library.path().join("service.json");
    std::fs::write(
        &workflow,
        serde_json::to_vec(&json!({
            "format":1,"id":"wf-agent-start","name":"Prepare app to start later","links":[],
            "steps":[{"kind":"serve","id":"s_preview","name":"Prepared preview",
                "command":"sleep 5","folder":{"use":"fresh-copy"},"startWhen":"asked","lifetime":"window","at":{"x":0,"y":0}}]
        }))?,
    )?;
    let absent: Arc<dyn AgentDriver> = Arc::new(Absent::new("unused", "this graph has no model"));
    let drivers: Drivers = Arc::new(move |_| Arc::clone(&absent));
    let processes = Arc::new(Processes::new());
    let store = Store::open(&project.path().join(".loadout/loadout.db"))?;
    let deps = RunDeps {
        home: library.path(),
        project: project.path(),
        store: &store,
        drivers,
        processes: Arc::clone(&processes),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let (sink, _source) = line_channel(128);
    let ran = run_workflow_inner(&deps, &request, sink).await;
    let groups: Vec<_> = processes.list().iter().map(|one| one.pgid).collect();
    let proofs = processes.close().await;
    assert!(
        proofs
            .iter()
            .all(|proof| matches!(proof, GroupProof::Dead { .. })),
        "fixture cleanup: {proofs:?}"
    );
    assert!(groups.iter().all(|pgid| supervisor::group_is_empty(*pgid)));
    let report = ran?;
    let book: serde_json::Value =
        serde_json::from_slice(&std::fs::read(report.dir.join("run.json"))?)?;
    assert_eq!(
        book["steps"][0]["process_started"], false,
        "configuration was reported as a real start: {book}"
    );
    assert!(
        groups.is_empty(),
        "the graph started a service reserved for an agent"
    );
    assert!(
        book["steps"][0]["summary"]
            .as_str()
            .is_some_and(|summary| summary.contains("not started")),
        "history did not say that the app was only configured: {book}"
    );
    Ok(())
}

#[tokio::test]
async fn a_step_cannot_call_a_hidden_lead_tool_through_the_real_host() -> Result<(), Box<dyn Error>>
{
    let project = tempfile::tempdir()?;
    let library = tempfile::tempdir()?;
    let socket = tempfile::tempdir()?;
    std::fs::create_dir_all(library.path().join("workflows"))?;
    let desk = Arc::new(Desk::at(
        Some(library.path().to_owned()),
        project.path().to_owned(),
    ));
    let bridge = Bridge::open(socket.path(), Role::Step, desk).await?;
    let answer = tokio::time::timeout(Duration::from_secs(2), async {
        let connection = UnixStream::connect(bridge.at()).await?;
        let (reader, mut writer) = connection.into_split();
        let mut reader = BufReader::new(reader);
        let mut text = String::new();
        reader.read_line(&mut text).await?;
        let greeting: Greeting = serde_json::from_str(&text)?;
        assert_eq!(
            greeting.tools,
            json!([]),
            "the fixture did not receive the disabled Step surface"
        );
        let call = Call {
            id: json!(27),
            call: "list_workflows".to_owned(),
            input: json!({}),
        };
        writer
            .write_all(format!("{}\n", serde_json::to_string(&call)?).as_bytes())
            .await?;
        writer.flush().await?;
        text.clear();
        reader.read_line(&mut text).await?;
        Ok::<Reply, Box<dyn Error>>(serde_json::from_str(&text)?)
    })
    .await??;
    drop(bridge);
    assert_eq!(answer.id, json!(27));
    assert!(
        matches!(&answer.answer, Answer::Refused(message) if message.contains("not available")),
        "the host executed a Lead tool that this Step was not granted: {answer:?}"
    );
    Ok(())
}
