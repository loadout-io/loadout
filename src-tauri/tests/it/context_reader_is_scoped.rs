//! CT-03b: kursor, zakres i odbiorca są jednym kontraktem ograniczonego czytelnika.
//!
//! Odmowy są czytane na KOŃCU prawdziwego gniazda, nie z wartości rdzenia. To rozróżnia
//! funkcję, która umie odmówić, od produktu, który rzeczywiście oddaje to zdanie vendorowi
//! (niezmiennik 29).

use std::error::Error;
use std::sync::Arc;

use loadout_lib::bridge::context::ContextDesk;
use loadout_lib::bridge::host::Bridge;
use loadout_lib::bridge::{Call, Greeting, Reply, serve};
use loadout_lib::context::access::{Allotment, ContextAccess, ContextShelf, Denied};
use loadout_lib::context::files::{DraftEdit, create_set, folder_of, library_root, save_draft};
use loadout_lib::context::limits::{READ_TEXT_BYTES, SEARCH_ANSWER_BYTES, SEARCH_RESULTS};
use loadout_lib::context::sources::{self, SourcePart};
use loadout_lib::context::{
    ContextDraft, ContextSource, Preparation, SCHEMA, SourceKind, StoredFile,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio_util::sync::CancellationToken;

const NOW: &str = "2026-09-08T10:00:00Z";

#[derive(Debug)]
struct Bench {
    _home: tempfile::TempDir,
    library: std::path::PathBuf,
    set: String,
    shelf: ContextShelf,
}

impl Bench {
    fn with_sources(sources: Vec<ContextSource>) -> Result<Self, Box<dyn Error>> {
        let home = tempfile::tempdir()?;
        let library = library_root(home.path());
        let made = create_set(&library, "Private briefs", NOW)?;
        let saved = save_draft(
            &library,
            &DraftEdit {
                id: made.set.id.clone(),
                title: made.set.title,
                description: made.set.description,
                draft: ContextDraft {
                    schema: SCHEMA,
                    sources,
                    ..ContextDraft::default()
                },
                expected_revision: Some(made.revision),
                at: NOW.to_owned(),
            },
        )?;
        let shelf = ContextShelf::open(&library, &saved.set.id)?;
        Ok(Self {
            _home: home,
            library,
            set: saved.set.id,
            shelf,
        })
    }

    fn access(
        &self,
        holder: &str,
        allotments: Vec<Allotment>,
        expires: CancellationToken,
    ) -> Result<ContextAccess, Box<dyn Error>> {
        Ok(self.shelf.grant(holder, allotments, expires)?)
    }
}

fn text_source(id: &str, text: impl Into<String>) -> ContextSource {
    ContextSource {
        id: id.to_owned(),
        kind: SourceKind::Text,
        name: "Brief".to_owned(),
        text: text.into(),
        ..ContextSource::default()
    }
}

async fn bridge_for(access: ContextAccess) -> Result<(tempfile::TempDir, Bridge), Box<dyn Error>> {
    let socket = tempfile::tempdir()?;
    let desk = Arc::new(ContextDesk::new(access));
    let bridge = Bridge::open_with_tools(socket.path(), desk.tools(), desk).await?;
    Ok((socket, bridge))
}

/// Jeden pełny obrót host → linia aplikacji → typ mostu → wynik narzędzia.
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
        id: json!(format!("ct-03b-{call}")),
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

fn value_from(result: &Value) -> Result<Value, Box<dyn Error>> {
    let text = result
        .pointer("/result/content/0/text")
        .and_then(Value::as_str)
        .ok_or("the bridge returned no text block")?;
    Ok(serde_json::from_str(text)?)
}

fn refusal_from(result: &Value) -> Result<&str, Box<dyn Error>> {
    assert_eq!(
        result.pointer("/result/isError").and_then(Value::as_bool),
        Some(true),
        "a refusal has to remain a failed tool result at the far end of the bridge: {result}"
    );
    result
        .pointer("/result/content/0/text")
        .and_then(Value::as_str)
        .ok_or_else(|| "a named refusal cannot be empty".into())
}

async fn refused_as(
    access: ContextAccess,
    call: &str,
    input: Value,
    expected: Denied,
) -> Result<(), Box<dyn Error>> {
    let (_socket, bridge) = bridge_for(access).await?;
    let result = call_through_bridge(&bridge, call, input).await?;
    assert_eq!(
        refusal_from(&result)?,
        expected.to_string(),
        "each repair is different, so the refusal has to say which boundary was crossed"
    );
    Ok(())
}

#[test]
fn the_core_enforces_the_same_boundaries_without_a_socket() -> Result<(), Box<dyn Error>> {
    let id = "core-long";
    let foreign = "core-foreign";
    let original = format!(
        "{}ę{}",
        "a".repeat(READ_TEXT_BYTES - 1),
        "needle ".repeat(READ_TEXT_BYTES / 3)
    );
    let bench = Bench::with_sources(vec![
        text_source(id, original.clone()),
        text_source(foreign, "needle outside the grant"),
    ])?;
    let lifetime = CancellationToken::new();
    let access = bench.access("core-reader", vec![Allotment::source(id)], lifetime.clone())?;

    let mut joined = String::new();
    let mut cursor = None;
    let mut first_cursor = None;
    loop {
        let read = access.read(id, cursor.as_deref())?;
        assert!(read.text.len() <= READ_TEXT_BYTES);
        joined.push_str(&read.text);
        cursor = read.cursor;
        if first_cursor.is_none() {
            first_cursor.clone_from(&cursor);
        }
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(joined, original);
    assert_eq!(access.read(foreign, None), Err(Denied::NotGranted));
    assert_eq!(
        access.read("invented", None),
        Err(Denied::UnknownIdentifier)
    );
    assert_eq!(
        access.read(id, Some("invented-cursor")),
        Err(Denied::InvalidCursor)
    );
    let found = access.search("needle", None)?;
    assert_eq!(found.results.len(), 1);
    assert_eq!(found.results[0].id, id);
    assert!(serde_json::to_string_pretty(&found)?.len() <= SEARCH_ANSWER_BYTES);
    let tools = ContextDesk::new(access.clone()).tools();
    assert_eq!(tools.as_array().map(Vec::len), Some(4));
    assert!(tools.as_array().into_iter().flatten().all(|tool| {
        tool.pointer("/annotations/readOnlyHint")
            .and_then(Value::as_bool)
            == Some(true)
    }));

    let issued = first_cursor.ok_or("the long source issued no cursor")?;
    let narrow = bench.access(
        "core-reader",
        vec![Allotment::bytes(id, 0, 1024)],
        lifetime.clone(),
    )?;
    assert_eq!(narrow.read(id, Some(&issued)), Err(Denied::OutsideRange));
    let other = bench.access(
        "other-reader",
        vec![Allotment::source(id)],
        lifetime.clone(),
    )?;
    assert_eq!(
        other.read(id, Some(&issued)),
        Err(Denied::DifferentRecipient)
    );
    lifetime.cancel();
    assert_eq!(access.read(id, None), Err(Denied::Expired));
    Ok(())
}

#[tokio::test]
async fn two_recipients_that_differ_only_in_what_they_were_given() -> Result<(), Box<dyn Error>> {
    let left = "source-left";
    let right = "source-right";
    let bench = Bench::with_sources(vec![
        text_source(left, "left recipient only"),
        text_source(right, "right recipient only"),
    ])?;
    let lifetime = CancellationToken::new();
    let left_access = bench.access(
        "left-recipient",
        vec![Allotment::source(left)],
        lifetime.clone(),
    )?;
    let right_access = bench.access("right-recipient", vec![Allotment::source(right)], lifetime)?;
    let (_left_socket, left_bridge) = bridge_for(left_access).await?;
    let (_right_socket, right_bridge) = bridge_for(right_access).await?;

    let left_own =
        value_from(&call_through_bridge(&left_bridge, "read_context", json!({"id":left})).await?)?;
    let right_own = value_from(
        &call_through_bridge(&right_bridge, "read_context", json!({"id":right})).await?,
    )?;
    assert_eq!(left_own["text"], "left recipient only");
    assert_eq!(right_own["text"], "right recipient only");

    for (bridge, foreign) in [(&left_bridge, right), (&right_bridge, left)] {
        let refused = call_through_bridge(bridge, "read_context", json!({"id":foreign})).await?;
        assert_eq!(refusal_from(&refused)?, Denied::NotGranted.to_string());
    }
    Ok(())
}

#[tokio::test]
async fn every_cursor_chunk_reassembles_the_text_and_the_window_reads_the_same_first_part()
-> Result<(), Box<dyn Error>> {
    // Wielobajtowy znak zaczyna się bajt przed granicą. Cięcie po stałej 16 KiB weszłoby
    // w jego środek; cofnięcie musi zostawić go W CAŁOŚCI do następnego fragmentu.
    let mut original = "a".repeat(READ_TEXT_BYTES - 1);
    original.push('ę');
    original.push_str(&"b".repeat(READ_TEXT_BYTES + 91));
    original.push_str("THE-END");
    let id = "long-source";
    let bench = Bench::with_sources(vec![text_source(id, original.clone())])?;
    let access = bench.access(
        "reader",
        vec![Allotment::source(id)],
        CancellationToken::new(),
    )?;
    let preview = sources::read_source(&bench.library, &bench.set, id, None)?;
    let first = access.read(id, None)?;
    match preview {
        SourcePart::Text { text, more } => {
            assert_eq!(
                text, first.text,
                "the window and agent diverged before the first cursor"
            );
            assert_eq!(more, first.cursor.is_some());
        }
        other => return Err(format!("text preview answered with {other:?}").into()),
    }

    let (_socket, bridge) = bridge_for(access).await?;
    let mut joined = String::new();
    let mut cursor: Option<String> = None;
    loop {
        let answer = value_from(
            &call_through_bridge(&bridge, "read_context", json!({"id":id,"cursor":cursor})).await?,
        )?;
        let part = answer["text"].as_str().unwrap_or_default();
        assert!(
            part.len() <= READ_TEXT_BYTES,
            "one frame carried {} bytes instead of at most {READ_TEXT_BYTES}",
            part.len()
        );
        joined.push_str(part);
        cursor = answer["cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(
        joined, original,
        "a byte missing or repeated at either cursor boundary changes the reconstructed source"
    );
    Ok(())
}

#[tokio::test]
async fn forged_foreign_expired_and_out_of_range_reads_are_distinct_sentences()
-> Result<(), Box<dyn Error>> {
    let id = "long-source";
    let foreign = "other-source";
    let text = "x".repeat(READ_TEXT_BYTES * 2 + 7);
    let bench = Bench::with_sources(vec![
        text_source(id, text),
        text_source(foreign, "foreign words"),
    ])?;
    let lifetime = CancellationToken::new();
    let broad = bench.access(
        "same-recipient",
        vec![Allotment::source(id)],
        lifetime.clone(),
    )?;
    let issued = broad
        .read(id, None)?
        .cursor
        .ok_or("the long source issued no cursor")?;

    refused_as(
        broad.clone(),
        "read_context",
        json!({"id":"invented-source"}),
        Denied::UnknownIdentifier,
    )
    .await?;
    refused_as(
        broad.clone(),
        "read_context",
        json!({"id":id,"cursor":"invented-cursor"}),
        Denied::InvalidCursor,
    )
    .await?;
    refused_as(
        bench.access(
            "same-recipient",
            vec![Allotment::bytes(id, 0, 1024)],
            lifetime.clone(),
        )?,
        "read_context",
        json!({"id":id,"cursor":issued}),
        Denied::OutsideRange,
    )
    .await?;
    refused_as(
        broad.clone(),
        "read_context",
        json!({"id":foreign}),
        Denied::NotGranted,
    )
    .await?;
    refused_as(
        bench.access(
            "other-recipient",
            vec![Allotment::source(id)],
            lifetime.clone(),
        )?,
        "read_context",
        json!({"id":id,"cursor":issued}),
        Denied::DifferentRecipient,
    )
    .await?;

    lifetime.cancel();
    refused_as(broad, "read_context", json!({"id":id}), Denied::Expired).await?;
    Ok(())
}

#[tokio::test]
async fn search_is_scoped_ordered_and_bounded_in_count_and_bytes() -> Result<(), Box<dyn Error>> {
    let mut sources = Vec::new();
    let mut allotted = Vec::new();
    let mut expected = Vec::new();
    for number in 0..15 {
        let id = format!("given-{number:02}");
        sources.push(text_source(
            &id,
            format!("needle {number:02} {}", "detail ".repeat(90)),
        ));
        expected.push(id.clone());
        allotted.push(Allotment::source(id));
    }
    sources.push(text_source(
        "not-given",
        "needle must never appear in search results",
    ));
    let bench = Bench::with_sources(sources)?;
    let access = bench.access("searcher", allotted, CancellationToken::new())?;
    let desk = ContextDesk::new(access.clone());
    let tools = desk.tools();
    assert_eq!(tools.as_array().map(Vec::len), Some(4));
    assert!(tools.as_array().into_iter().flatten().all(|tool| {
        tool.pointer("/annotations/readOnlyHint")
            .and_then(Value::as_bool)
            == Some(true)
    }));
    let (_socket, bridge) = bridge_for(access).await?;

    let listed = value_from(&call_through_bridge(&bridge, "list_context", json!({})).await?)?;
    assert_eq!(listed["items"].as_array().map(Vec::len), Some(15));

    let mut found = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let answered = call_through_bridge(
            &bridge,
            "search_context",
            json!({"query":"needle","cursor":cursor}),
        )
        .await?;
        let wire_text = answered
            .pointer("/result/content/0/text")
            .and_then(Value::as_str)
            .ok_or("search returned no text block")?;
        assert!(
            wire_text.len() <= SEARCH_ANSWER_BYTES,
            "the serialized answer has {} bytes instead of at most {SEARCH_ANSWER_BYTES}",
            wire_text.len()
        );
        let page: Value = serde_json::from_str(wire_text)?;
        let results = page["results"]
            .as_array()
            .ok_or("search results are not an array")?;
        assert!(results.len() <= SEARCH_RESULTS);
        found.extend(
            results
                .iter()
                .filter_map(|one| one["id"].as_str().map(str::to_owned)),
        );
        cursor = page["cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(
        found, expected,
        "filesystem order must not reorder search results"
    );
    assert!(!found.iter().any(|id| id == "not-given"));
    Ok(())
}

#[tokio::test]
async fn one_selected_pdf_page_has_no_road_to_the_whole_original() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let library = library_root(home.path());
    let made = create_set(&library, "Selected page", NOW)?;
    let source = "paper-source";
    let relative = format!("sources/{source}/r1/original.pdf");
    let folder = folder_of(&library, &made.set.id)?;
    loadout_lib::context::files::publish_source_file(
        &folder,
        &relative,
        b"SECRET_ONLY_IN_THE_ORIGINAL",
    )?;
    loadout_lib::context::files::publish_source_file(
        &folder,
        &format!("sources/{source}/r1/pages/page-0001.txt"),
        b"Visible first page.",
    )?;
    loadout_lib::context::files::publish_source_file(
        &folder,
        &format!("sources/{source}/r1/pages/page-0002.txt"),
        b"A second page that was not selected.",
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
                    kind: SourceKind::Pdf,
                    name: "Research paper".to_owned(),
                    preparation: Preparation::Ready,
                    file: Some(StoredFile {
                        path: relative,
                        revision: "r1".to_owned(),
                        mime: "application/pdf".to_owned(),
                        pages: Some(2),
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
        "page-reader",
        vec![Allotment::page(source, 1)],
        CancellationToken::new(),
    )?;
    let (_socket, bridge) = bridge_for(access).await?;
    let list = value_from(&call_through_bridge(&bridge, "list_context", json!({})).await?)?;
    let items = list["items"].as_array().ok_or("the list has no items")?;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["page"], 1);

    let search = value_from(
        &call_through_bridge(
            &bridge,
            "search_context",
            json!({"query":"SECRET_ONLY_IN_THE_ORIGINAL"}),
        )
        .await?,
    )?;
    assert_eq!(search["results"].as_array().map(Vec::len), Some(0));
    let page_two = format!("{source}/page/0002");
    let refused = call_through_bridge(&bridge, "read_context", json!({"id":page_two})).await?;
    assert_eq!(refusal_from(&refused)?, Denied::NotGranted.to_string());
    Ok(())
}
