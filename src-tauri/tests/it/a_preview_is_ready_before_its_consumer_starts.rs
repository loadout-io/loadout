//! WF-26: normalny Start, prawdziwy listener HTTP i agent-konsument.
//! Dubler zastępuje wyłącznie model. Serwer jest osobnym procesem tego samego
//! binarium testowego; nie udajemy gotowości przez pole bool z Processes.

use std::error::Error;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use loadout_lib::commands::processes::Processes;
use loadout_lib::commands::run::{run_workflow_inner, stop_run_inner};
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{self, GroupId, GroupProof};
use loadout_lib::ipc::line_channel;
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::mpsc;

const PATIENCE: Duration = Duration::from_secs(12);
const AGENT: &str = "---\nschema: 1\nid: 01990000-0000-7000-8000-000000002626\nname: Preview consumer\nsummary: Reads its own preview\ncolor: slate\nrunsWith: claude-code\nmodel: opus\nthinking: balanced\nfileAccess: work-freely\ngiveUpAfterMinutes: 2\nwriteResultsTo: \"\"\ntools: everything\nskills: []\nconnections: []\n---\nRead the preview passed by the preceding service.\n";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_delayed_real_server_is_ready_before_qa_and_its_address_reaches_qa()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let port = vacant_port()?;
    let processes = Arc::new(Processes::new());
    let ran = bench.run(&processes, &[("a", port, 600)], false).await;
    let proof = processes.close().await;
    assert_dead(&proof);
    let ran = ran?;
    assert_eq!(
        ran.seen.len(),
        1,
        "the consumer did not execute: {}",
        ran.book
    );
    assert!(
        ran.seen[0].ready,
        "QA ran before its real HTTP endpoint returned 200"
    );
    let url = format!("http://127.0.0.1:{port}");
    assert!(
        ran.seen[0].context.contains(&url),
        "the production RunSpec and addressed handoffs omitted the service URL: {}",
        ran.seen[0].context
    );
    assert!(
        ran.seen[0].context.contains("generation"),
        "QA has a URL but no instance generation"
    );
    assert_eq!(step(&ran.book, "s_preview_a")["status"], "succeeded");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_unrelated_http_200_is_not_our_service_and_is_not_killed() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    // Obcy listener naprawdę należy do procesu testu, NIE do grupy Serve.
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    listener.set_nonblocking(true)?;
    let cancel = tokio_util::sync::CancellationToken::new();
    let done = cancel.clone();
    let foreign = tokio::task::spawn_blocking(move || {
        while !done.is_cancelled() {
            if let Ok((mut connection, _)) = listener.accept() {
                let _ = connection.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nforeign",
                );
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    let processes = Arc::new(Processes::new());
    let ran = bench.run(&processes, &[("foreign", port, 0)], false).await;
    let still_foreign = http_ready(port);
    let proof = processes.close().await;
    cancel.cancel();
    foreign.await?;
    assert_dead(&proof);
    let ran = ran?;
    assert!(
        still_foreign,
        "refusing our service touched the unrelated listener"
    );
    assert!(
        ran.seen.is_empty(),
        "HTTP 200 from a foreign process started QA"
    );
    let preview = step(&ran.book, "s_preview_foreign");
    assert_eq!(preview["status"], "failed");
    assert!(
        preview["error"]
            .as_str()
            .is_some_and(|text| text.contains("another app")),
        "the history does not explain the occupied address: {preview}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_foreign_listener_winning_the_post_spawn_race_is_not_readiness()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let port = vacant_port()?;
    let processes = Arc::new(Processes::new());
    let watched = Arc::clone(&processes);
    let stop_listener = tokio_util::sync::CancellationToken::new();
    let stop = stop_listener.clone();
    let foreign = tokio::spawn(async move {
        // Port jest wolny podczas preflight. Obcy listener pojawia się dopiero,
        // gdy prawdziwa grupa Serve została zarejestrowana po spawn.
        while watched.list().is_empty() {
            tokio::select! {
                () = stop.cancelled() => return Ok::<(), std::io::Error>(()),
                () = tokio::time::sleep(Duration::from_millis(1)) => {}
            }
        }
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
        loop {
            tokio::select! {
                () = stop.cancelled() => return Ok(()),
                accepted = listener.accept() => {
                    let (mut stream, _) = accepted?;
                    let _ = tokio::io::AsyncWriteExt::write_all(&mut stream,
                        b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nforeign").await;
                }
            }
        }
    });
    let ran = bench
        .run(&processes, &[("foreign-race", port, 0)], false)
        .await;
    let foreign_survived = http_ready(port);
    let proofs = processes.close().await;
    stop_listener.cancel();
    let foreign_result = foreign.await;
    assert_dead(&proofs);
    foreign_result??;
    let ran = ran?;
    assert!(
        foreign_survived,
        "refusing the owned app killed the foreign listener"
    );
    let preview = step(&ran.book, "s_preview_foreign-race");
    assert_eq!(
        preview["process_started"], true,
        "the test did not cross the preflight/spawn boundary: {preview}"
    );
    assert!(
        ran.seen.is_empty(),
        "QA used HTTP 200 from the post-spawn foreign listener"
    );
    assert!(
        preview["error"]
            .as_str()
            .is_some_and(|error| error.contains("another app")),
        "the refused ownership was lost from history: {preview}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exit_before_readiness_never_releases_the_consumer() -> Result<(), Box<dyn Error>> {
    assert_not_ready("exit", 0, "exited").await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn readiness_timeout_ends_the_group_before_reporting_failure() -> Result<(), Box<dyn Error>> {
    assert_not_ready("timeout", 60_000, "ready").await
}

async fn assert_not_ready(name: &str, delay: u64, sentence: &str) -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let processes = Arc::new(Processes::new());
    let ran = bench
        .run(&processes, &[(name, vacant_port()?, delay)], false)
        .await;
    let groups = processes.list();
    let alive_after_run = groups
        .iter()
        .any(|entry| !supervisor::group_is_empty(entry.pgid));
    let proof = processes.close().await;
    assert_dead(&proof);
    let ran = ran?;
    assert!(
        ran.seen.is_empty(),
        "QA ran despite missing readiness: {}",
        ran.book
    );
    assert!(
        !alive_after_run,
        "the timeout/early-exit path left an owned group running"
    );
    let preview = step(&ran.book, &format!("s_preview_{name}"));
    assert_eq!(preview["status"], "failed");
    assert!(
        preview["error"]
            .as_str()
            .is_some_and(|text| text.contains(sentence)),
        "history lost the readiness failure: {preview}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stop_while_waiting_does_not_claim_readiness_or_start_qa() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let processes = Arc::new(Processes::new());
    let ran = bench
        .run(&processes, &[("stop", vacant_port()?, 60_000)], true)
        .await;
    let alive_after_run = processes
        .list()
        .iter()
        .any(|entry| !supervisor::group_is_empty(entry.pgid));
    let proof = processes.close().await;
    assert_dead(&proof);
    let ran = ran?;
    assert!(ran.seen.is_empty(), "Stop while waiting released QA");
    assert!(
        !alive_after_run,
        "Stop only cancelled the Rust probe and left the OS group alive"
    );
    assert_ne!(step(&ran.book, "s_preview_stop")["status"], "succeeded");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_copies_receive_different_automatically_allocated_endpoints()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let processes = Arc::new(Processes::new());
    let ran = bench
        .run(&processes, &[("a", 0, 300), ("b", 0, 300)], false)
        .await;
    let ports: Vec<_> = ["a", "b"]
        .into_iter()
        .map(|name| read_port(bench.control.path(), name))
        .collect();
    let proof = processes.close().await;
    assert_dead(&proof);
    let ran = ran?;
    assert_eq!(
        ran.seen.len(),
        2,
        "one of the service consumers never ran: {}",
        ran.book
    );
    assert!(
        ran.seen.iter().all(|seen| seen.ready),
        "a copy did not receive its own ready endpoint: {:?}",
        ran.seen
    );
    let a = ports[0].ok_or("first service got no host-allocated port")?;
    let b = ports[1].ok_or("second service got no host-allocated port")?;
    assert_ne!(a, b, "two live instances claimed the same endpoint");
    for seen in &ran.seen {
        let own = if seen.name == "a" { a } else { b };
        let other = if seen.name == "a" { b } else { a };
        assert!(
            seen.context.contains(&format!("http://127.0.0.1:{own}")),
            "consumer lost its endpoint: {seen:?}"
        );
        assert!(
            !seen.context.contains(&format!("http://127.0.0.1:{other}")),
            "an unrelated branch leaked its endpoint: {seen:?}"
        );
    }
    Ok(())
}

/// Wykonywany wyłącznie jako prawdziwe dziecko Serve. Brak env w pełnej suicie
/// nie tworzy procesu ani listenera. Ignore nie liczy się jako przejście kryterium.
#[test]
#[ignore = "real child-process fixture; invoked by its parent test"]
fn http_service_fixture() -> Result<(), Box<dyn Error>> {
    let control = PathBuf::from(std::env::var("WF26_CONTROL")?);
    let name = std::env::var("WF26_NAME")?;
    if name == "exit" {
        return Err("the requested early exit".into());
    }
    if name.starts_with("foreign") {
        std::thread::sleep(Duration::from_secs(15));
        return Ok(());
    }
    let fixed: u16 = std::env::var("WF26_FIXED_PORT")?.parse()?;
    let port = if fixed == 0 {
        std::env::var("LOADOUT_WEB_PORT")?.parse()?
    } else {
        fixed
    };
    let delay = Duration::from_millis(std::env::var("WF26_DELAY")?.parse()?);
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    listener.set_nonblocking(true)?;
    fs::write(control.join(format!("port-{name}")), port.to_string())?;
    let began = Instant::now();
    println!("the fixture owns its listener");
    while began.elapsed() < Duration::from_secs(15)
        && !control.join(format!("exit-{name}")).exists()
    {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_read_timeout(Some(Duration::from_millis(300)))?;
                let mut request = [0u8; 2048];
                let _ = stream.read(&mut request);
                let code = if began.elapsed() >= delay {
                    "200 OK"
                } else {
                    "503 Not Ready"
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {code}\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error.into()),
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

struct Bench {
    home: TempDir,
    project: TempDir,
    control: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let bench = Self {
            home: TempDir::new()?,
            project: TempDir::new()?,
            control: TempDir::new()?,
        };
        fs::create_dir_all(bench.home.path().join("agents"))?;
        fs::create_dir_all(bench.home.path().join("workflows"))?;
        fs::write(bench.home.path().join("agents/consumer.md"), AGENT)?;
        fs::write(
            bench.project.path().join("input.txt"),
            "a real private input",
        )?;
        Ok(bench)
    }

    /* 2026-09-09 — SUFIT GOTOWOŚCI 1 s NALEŻY DO TESTU, KTÓRY MIERZY PRZEKROCZENIE CZASU,
    a nie do tych, które mierzą sukces. Zmierzone: ten moduł padał **raz na trzy** przebiegi
    w izolacji, na wolnej maszynie (load 3 na 16 rdzeniach), raz jako „the consumer did not
    execute", raz jako `ready: false` mimo przekazania niosącego `"state":"ready"` i właściwy
       port. Przyczyna nie jest losowa: przypadek `a` ma 600 ms opóźnienia serwera, a w tej samej
       sekundzie musi się zmieścić uruchomienie procesu potomnego — którym jest CAŁA binarka
       testowa, ~100 MB w profilu debug, z kontrolą podpisu przy pierwszym starcie.

       Osiem sekund nie osłabia ani jednej asercji: testy sukcesu dalej sądzą, że QA nie rusza,
       zanim prawdziwy serwer odpowie 200. Ścieżkę przekroczenia czasu dowodzi przypadek
       `timeout`, który ma 60 s opóźnienia przy 1 s sufitu — i dla niego sufit zostaje. */
    fn workflow(&self, services: &[(&str, u16, u64)]) -> Result<Value, Box<dyn Error>> {
        let binary = std::env::current_exe()?;
        let mut steps = Vec::new();
        let mut links = Vec::new();
        for (name, port, delay) in services {
            let service = format!("s_preview_{name}");
            let qa = format!("s_qa_{name}");
            let command = format!(
                "WF26_CONTROL={} WF26_NAME={} WF26_FIXED_PORT={port} WF26_DELAY={delay} {} --ignored --exact a_preview_is_ready_before_its_consumer_starts::http_service_fixture --nocapture",
                quoted(&self.control.path().to_string_lossy()),
                quoted(name),
                quoted(&binary.to_string_lossy())
            );
            steps.push(json!({"kind":"serve", "id":service, "name":format!("Preview {name}"), "command":command,
                "folder":{"use":"fresh-copy"}, "lifetime":"window", "at":{"x":0,"y":0},
                "endpoints":[{"name":"web","host":"127.0.0.1","port":port,"portEnv":"LOADOUT_WEB_PORT"}],
                "readiness":{"kind":"http","endpoint":"web","path":"/health","timeoutSeconds":if *name == "timeout" { 1 } else { 8 },"expectedStatus":200}}));
            steps.push(json!({"kind":"agent","id":qa,"name":format!("Consumer {name}"),"agent":"01990000-0000-7000-8000-000000002626", "overrides":{}, "instructions":format!("wf26-consumer:{name}"), "folder":{"use":"fresh-copy"}, "at":{"x":0,"y":200}}));
            links.push(json!({"from":service,"to":qa}));
        }
        Ok(
            json!({"format":1,"id":"wf_ready_preview","name":"Use the ready preview","steps":steps,"links":links}),
        )
    }

    async fn run(
        &self,
        processes: &Arc<Processes>,
        services: &[(&str, u16, u64)],
        stop: bool,
    ) -> Result<Ran, Box<dyn Error>> {
        let workflow = self.home.path().join("workflows/ready.json");
        fs::write(&workflow, serde_json::to_vec(&self.workflow(services)?)?)?;
        fs::create_dir_all(self.project.path().join(".loadout"))?;
        let store = Store::open(&self.project.path().join(".loadout/loadout.db"))?;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let driver: Arc<dyn AgentDriver> = Arc::new(Consumer {
            control: self.control.path().to_owned(),
            seen: Arc::clone(&seen),
        });
        let drivers: Drivers = Arc::new(move |_| Arc::clone(&driver));
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store: &store,
            drivers,
            processes: Arc::clone(processes),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow,
            how_many_at_once: 4,
            task: None,
            part: None,
            handoffs_from: None,
        };
        let (sink, mut source) = line_channel(4096);
        let running = run_workflow_inner(&deps, &request, sink);
        let stopping = async {
            while processes.list().is_empty() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            tokio::time::sleep(Duration::from_millis(80)).await;
            stop_run_inner(&deps).await
        };
        let report = tokio::time::timeout(PATIENCE, async {
            tokio::pin!(running);
            if stop {
                tokio::select! {
                    report = &mut running => report,
                    _ = stopping => running.await,
                }
            } else {
                running.await
            }
        })
        .await;
        while source.try_next().is_some() {}
        let report = match report {
            Ok(report) => report?,
            Err(error) => {
                let _ = stop_run_inner(&deps).await;
                return Err(error.into());
            }
        };
        let book = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
        let seen = seen.lock().unwrap_or_else(PoisonError::into_inner).clone();
        Ok(Ran {
            _report: report,
            book,
            seen,
        })
    }
}

struct Ran {
    _report: RunReport,
    book: Value,
    seen: Vec<Observed>,
}
#[derive(Clone, Debug)]
struct Observed {
    name: String,
    ready: bool,
    context: String,
}
#[derive(Debug)]
struct Consumer {
    control: PathBuf,
    seen: Arc<Mutex<Vec<Observed>>>,
}

#[async_trait]
impl AgentDriver for Consumer {
    fn id(&self) -> &'static str {
        "claude-code"
    }
    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("wf26-test".to_owned()),
        })
    }
    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let name = spec
            .prompt
            .split("wf26-consumer:")
            .nth(1)
            .and_then(|tail| tail.split_whitespace().next())
            .unwrap_or("unknown")
            .to_owned();
        let ready = read_port(&self.control, &name).is_some_and(http_ready);
        let mut context = spec.prompt.clone();
        for line in spec.prompt.lines() {
            if let Some((_, addressed)) = line
                .strip_prefix("- ")
                .and_then(|line| line.split_once(": "))
                && let Some((path, _)) = addressed.rsplit_once(" (")
                && path.contains("/handoffs/")
            {
                context.push_str(&fs::read_to_string(path)?);
            }
        }
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Observed {
                name,
                ready,
                context,
            });
        let session = SessionRef {
            vendor: "claude-code",
            id: spec.run_id.to_string(),
        };
        let _ = events
            .send(
                AgentEvent::Started {
                    session: session.clone(),
                    model: spec.model.unwrap_or_default(),
                    tools: Vec::new(),
                    capabilities: Vec::new(),
                }
                .into(),
            )
            .await;
        Ok(Box::new(Turn { session, events }))
    }
}

#[derive(Debug)]
struct Turn {
    session: SessionRef,
    events: mpsc::Sender<DecodedEvent>,
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
        let outcome = Outcome { ok:true, reason:FinishReason::Completed, text:"## Answer\nRead the supplied preview.\n\n## Evidence\nFixture response.\n\n## Open questions\nNone.\n".to_owned(), cost_usd:None, tokens:Tokens::default(), turns:1, took:Duration::from_millis(1), session:self.session.clone() };
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

fn step<'a>(book: &'a Value, node: &str) -> &'a Value {
    book["steps"]
        .as_array()
        .and_then(|steps| steps.iter().find(|step| step["node_key"] == node))
        .unwrap_or(&Value::Null)
}
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}
fn vacant_port() -> std::io::Result<u16> {
    Ok(TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port())
}
fn read_port(control: &Path, name: &str) -> Option<u16> {
    fs::read_to_string(control.join(format!("port-{name}")))
        .ok()?
        .trim()
        .parse()
        .ok()
}
fn http_ready(port: u16) -> bool {
    let Ok(mut stream) =
        TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(300))
    else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(300)));
    let _ =
        stream.write_all(b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    let mut response = [0u8; 512];
    stream
        .read(&mut response)
        .is_ok_and(|count| response[..count].starts_with(b"HTTP/1.1 200 "))
}
fn assert_dead(proofs: &[GroupProof]) {
    assert!(
        proofs
            .iter()
            .all(|proof| matches!(proof, GroupProof::Dead { .. })),
        "fixture cleanup did not prove all owned groups dead: {proofs:?}"
    );
}
