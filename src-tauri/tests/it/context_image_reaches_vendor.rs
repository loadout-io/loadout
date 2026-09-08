//! CT-03b: obraz z przydzielonego źródła kończy obrót jako blok obrazu, nie tekst z base64.
//!
//! Fikstura ma fakt dostępny wyłącznie w pikselach. Jej nazwa, nazwa źródła, opis i polecenie
//! żywej próby nie podają ani kodu, ani koloru, ani kształtu.

use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine as _;
use loadout_lib::bridge::context::{ContextDesk, context_tools};
use loadout_lib::bridge::host::{Answers, Bridge};
use loadout_lib::bridge::{Answer, Call, Greeting, Reply, serve};
use loadout_lib::connections::{Connection, Transport, runtime};
use loadout_lib::context::access::{Allotment, ContextAccess, ContextShelf, Denied, ImageVariant};
use loadout_lib::context::files::{DraftEdit, create_set, folder_of, library_root, save_draft};
use loadout_lib::context::limits::IMAGE_ANSWER_BYTES;
use loadout_lib::context::sources::{self, ImportItem, ImportRequest};
use loadout_lib::context::{ContextDraft, ContextSource, SCHEMA, SourceKind, StoredFile};
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::{AgentDriver, Policy, RunSpec};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const NOW: &str = "2026-09-08T11:00:00Z";
const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../e2e/fixtures/context/visual-reference.png"
);

#[derive(Debug)]
struct Picture {
    _home: tempfile::TempDir,
    access: ContextAccess,
    id: String,
}

impl Picture {
    fn imported() -> Result<Self, Box<dyn Error>> {
        let home = tempfile::tempdir()?;
        let library = library_root(home.path());
        let made = create_set(&library, "Visual material", NOW)?;
        let report = sources::import(
            &library,
            &ImportRequest {
                set_id: made.set.id.clone(),
                operation_id: "ct-03b-image".to_owned(),
                expected_revision: Some(made.revision),
                items: vec![ImportItem {
                    // Nazwa celowo nic nie mówi o tym, co jest w pikselach.
                    name: "reference.png".to_owned(),
                    path: Some(FIXTURE.to_owned()),
                    text: None,
                    image: None,
                }],
                at: NOW.to_owned(),
            },
        )?;
        let id = report
            .results
            .first()
            .and_then(|result| result.added.first())
            .cloned()
            .ok_or("the real PNG was not imported")?;
        let shelf = ContextShelf::open(&library, &report.read.set.id)?;
        let access = shelf.grant(
            "picture-reader",
            vec![Allotment::source(&id)],
            CancellationToken::new(),
        )?;
        Ok(Self {
            _home: home,
            access,
            id,
        })
    }
}

async fn call_through_bridge(
    bridge: &Bridge,
    call: &str,
    input: Value,
) -> Result<Value, Box<dyn Error>> {
    let (reading, mut writing) = UnixStream::connect(bridge.at()).await?.into_split();
    let mut reading = BufReader::new(reading);
    let mut hello = String::new();
    reading.read_line(&mut hello).await?;
    let _: Greeting = serde_json::from_str(hello.trim())?;

    let mut asked = serde_json::to_vec(&Call {
        id: json!("ct-03b-image"),
        call: call.to_owned(),
        input,
    })?;
    asked.push(b'\n');
    writing.write_all(&asked).await?;
    writing.flush().await?;

    let mut answered = String::new();
    reading.read_line(&mut answered).await?;
    let reply: Reply = serde_json::from_str(answered.trim())?;
    Ok(serve::tool_result(&reply.id, &reply.answer))
}

fn assert_one_image_block(answered: &Value, expected: &str) -> Result<(), Box<dyn Error>> {
    let content = answered
        .pointer("/result/content")
        .and_then(Value::as_array)
        .ok_or("the tool result has no content blocks")?;
    assert_eq!(
        content.len(),
        1,
        "one read returns one image; a caption beside it would be a second answer"
    );
    assert_eq!(content[0]["type"], "image");
    assert_eq!(content[0]["mimeType"], "image/png");
    assert_eq!(content[0]["data"], expected);
    assert!(content[0].get("text").is_none());
    assert!(answered.pointer("/result/isError").is_none());
    Ok(())
}

#[tokio::test]
async fn the_context_desk_and_serializer_keep_the_typed_image() -> Result<(), Box<dyn Error>> {
    let picture = Picture::imported()?;
    let expected = picture.access.image(&picture.id, ImageVariant::Agent)?;
    let desk = ContextDesk::new(picture.access);
    let answer = desk
        .answer(Call {
            id: json!("ct-03b-direct-image"),
            call: "view_context_image".to_owned(),
            input: json!({"id":picture.id}),
        })
        .await;
    let answered = serve::tool_result(&json!("ct-03b-direct-image"), &answer);
    assert_one_image_block(&answered, &expected.data)
}

#[tokio::test]
async fn the_context_picture_finishes_the_whole_turn_as_one_image_block()
-> Result<(), Box<dyn Error>> {
    let picture = Picture::imported()?;
    let expected = picture.access.image(&picture.id, ImageVariant::Agent)?;
    let decoded = base64::engine::general_purpose::STANDARD.decode(&expected.data)?;
    assert!(decoded.len() <= IMAGE_ANSWER_BYTES);

    let socket = tempfile::tempdir()?;
    let desk = Arc::new(ContextDesk::new(picture.access));
    let bridge = Bridge::open_with_tools(socket.path(), desk.tools(), desk).await?;
    let answered =
        call_through_bridge(&bridge, "view_context_image", json!({"id":picture.id})).await?;
    assert_one_image_block(&answered, &expected.data)
}

/// Stara droga bierze TE SAME bajty i pakuje je do zwykłej odpowiedzi tekstowej.
struct PictureInWords {
    data: String,
    mime: String,
}

#[async_trait]
impl Answers for PictureInWords {
    async fn answer(&self, _call: Call) -> Answer {
        Answer::Ok(json!({"data":self.data,"mimeType":self.mime}))
    }
}

#[tokio::test]
async fn the_same_context_picture_on_the_old_road_is_not_an_image_block()
-> Result<(), Box<dyn Error>> {
    let picture = Picture::imported()?;
    let expected = picture.access.image(&picture.id, ImageVariant::Agent)?;
    let socket = tempfile::tempdir()?;
    let tools = Value::Array(
        context_tools()
            .iter()
            .map(loadout_lib::bridge::verbs::Verb::listed)
            .collect(),
    );
    let bridge = Bridge::open_with_tools(
        socket.path(),
        tools,
        Arc::new(PictureInWords {
            data: expected.data.clone(),
            mime: expected.mime,
        }),
    )
    .await?;
    let answered =
        call_through_bridge(&bridge, "view_context_image", json!({"id":picture.id})).await?;

    assert_eq!(
        answered.pointer("/result/content/0/type"),
        Some(&json!("text"))
    );
    let text = answered
        .pointer("/result/content/0/text")
        .and_then(Value::as_str)
        .ok_or("the old road returned no text")?;
    assert!(text.contains(&expected.data));
    assert!(answered.pointer("/result/content/0/data").is_none());
    Ok(())
}

#[tokio::test]
async fn an_image_over_five_mib_is_refused_before_base64_expands_it() -> Result<(), Box<dyn Error>>
{
    let home = tempfile::tempdir()?;
    let library = library_root(home.path());
    let made = create_set(&library, "Large picture", NOW)?;
    let source = "large-picture";
    let folder = folder_of(&library, &made.set.id)?;
    let original = format!("sources/{source}/r1/original.png");
    let derived = format!("sources/{source}/r1/for-the-agent.png");
    loadout_lib::context::files::publish_source_file(&folder, &original, b"unused")?;
    loadout_lib::context::files::publish_source_file(
        &folder,
        &derived,
        &vec![b'x'; IMAGE_ANSWER_BYTES + 1],
    )?;
    loadout_lib::context::files::publish_source_file(
        &folder,
        &format!("sources/{source}/r1/thumbnail.png"),
        b"preview",
    )?;
    let saved = save_draft(
        &library,
        &DraftEdit {
            id: made.set.id.clone(),
            title: made.set.title,
            description: made.set.description,
            draft: ContextDraft {
                schema: SCHEMA,
                sources: vec![ContextSource {
                    id: source.to_owned(),
                    kind: SourceKind::Image,
                    name: "Large picture".to_owned(),
                    file: Some(StoredFile {
                        path: original,
                        revision: "r1".to_owned(),
                        mime: "image/png".to_owned(),
                        ..StoredFile::default()
                    }),
                    ..ContextSource::default()
                }],
                ..ContextDraft::default()
            },
            expected_revision: Some(made.revision),
            at: NOW.to_owned(),
        },
    )?;
    let shelf = ContextShelf::open(&library, &saved.set.id)?;
    let access = shelf.grant(
        "large-reader",
        vec![Allotment::source(source)],
        CancellationToken::new(),
    )?;
    assert_eq!(
        access.image(source, ImageVariant::Agent),
        Err(Denied::ImageTooLarge)
    );
    Ok(())
}

fn executable_connection(bridge: &Bridge) -> Result<Connection, Box<dyn Error>> {
    let mut connection = bridge.as_connection()?;
    match &mut connection.transport {
        Transport::Stdio { command, .. } => {
            // W teście `current_exe()` jest binarką celu `it`, która nie obsługuje `--bridge`.
            // Produkcyjna binarka zbudowana obok jest właściwym odpowiednikiem procesu aplikacji.
            env!("CARGO_BIN_EXE_loadout").clone_into(command);
        }
        Transport::Http { .. } => {
            return Err("Loadout's own bridge is not a stdio connection".into());
        }
    }
    Ok(connection)
}

fn live_spec(cwd: &Path) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: "Use the context tools. List the available material, open its picture, and answer with only the exact short code, the color, and the geometric shape visible in the picture. Do not infer them from the item name.".to_owned(),
        model: None,
        system_append: None,
        policy: Policy::ReadOnly,
        reaches_the_web: false,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

async fn ask_live_vendor(
    vendor: &str,
    driver: Box<dyn AgentDriver>,
    picture: Picture,
) -> Result<String, Box<dyn Error>> {
    let socket = tempfile::tempdir()?;
    let desk = Arc::new(ContextDesk::new(picture.access));
    let bridge = Bridge::open_with_tools(socket.path(), desk.tools(), desk).await?;
    let runtime_dir = tempfile::tempdir()?;
    let configuration = runtime::for_driver(
        runtime_dir.path(),
        vendor,
        &[executable_connection(&bridge)?],
        |_name| None,
    )?;
    let driver = driver
        .configured(&configuration)
        .ok_or("the vendor did not accept its tool-server configuration")?;
    let project = tempfile::tempdir()?;
    let (events_tx, mut events) = mpsc::channel(4096);
    let draining = tokio::spawn(async move { while events.recv().await.is_some() {} });
    let mut handle = timeout(
        Duration::from_mins(4),
        driver.start(live_spec(project.path()), events_tx),
    )
    .await??;
    let outcome = timeout(Duration::from_mins(4), handle.wait()).await??;
    let _ = timeout(Duration::from_secs(30), handle.close()).await??;
    drop(handle);
    draining.await?;
    assert!(
        outcome.ok,
        "{vendor} ended without a successful answer: {outcome:?}"
    );
    Ok(outcome.text)
}

/// Oba prawdziwe CLI muszą same wywołać `view_context_image`; sama obecność w `tools/list`
/// nie może udowodnić, co było w pikselach.
#[tokio::test]
#[ignore = "asks the real claude and codex CLIs to inspect a private picture"]
async fn both_clis_name_the_detail_that_exists_only_in_pixels() -> Result<(), Box<dyn Error>> {
    for vendor in ["claude", "codex"] {
        let probe_driver: Box<dyn AgentDriver> = match vendor {
            "claude" => Box::new(ClaudeDriver::new()),
            "codex" => Box::new(CodexDriver::new()),
            _ => return Err("the live fixture names an unsupported vendor".into()),
        };
        let probe = timeout(Duration::from_secs(30), probe_driver.probe()).await??;
        assert!(probe.found, "{vendor} is not installed: {probe:?}");
        let said = ask_live_vendor(vendor, probe_driver, Picture::imported()?).await?;
        let lowered = said.to_ascii_lowercase();
        println!(
            "{vendor} {:?} answered with default model: {said:?}",
            probe.version
        );
        assert!(
            lowered.contains("q-6284") && lowered.contains("teal") && lowered.contains("hexagon"),
            "{vendor} did not report all three facts available only in the pixels: {said:?}"
        );
    }
    Ok(())
}
