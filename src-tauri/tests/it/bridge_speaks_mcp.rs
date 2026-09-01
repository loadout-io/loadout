//! Most mówi protokołem, którym mówi vendor — i oddaje odpowiedź aplikacji, nie swoją.
//!
//! # Co tu jest sądzone
//!
//! Dwie połowy jednej drogi. Górna: [`serve::local_answer`] odpowiada na to, co da się
//! rozstrzygnąć bez pytania aplikacji, i **nie milczy** na nic. Dolna: gniazdo wita mostu listą
//! narzędzi, przyjmuje wywołanie i oddaje odpowiedź z tym samym identyfikatorem.
//!
//! # Dlaczego identyfikator jest tu kryterium, a nie szczegółem
//!
//! Bo czasownik, który blokuje turę, zamienia pomyłkę w identyfikatorze w **odpowiedź na cudze
//! pytanie**. Przy dwóch sesjach w jednym oknie to jest wada, której nie widać w żadnym logu:
//! obie tury dostają wynik, tylko nie swój.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w `chat_never_starts_a_run` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use loadout_lib::bridge::host::{Answers, Bridge};
use loadout_lib::bridge::{Answer, Call, Greeting, Reply, Role, serve, verbs};

/// Lista narzędzi lidera — ta sama, którą dostaje most w powitaniu.
fn tools() -> Value {
    verbs::tool_list(Role::Lead)
}

#[test]
fn initialize_answers_with_a_version_and_a_name() {
    let asked = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" });
    let said = serve::local_answer(&asked, &tools()).expect("initialize has to be answered");

    assert_eq!(said.pointer("/id").and_then(Value::as_i64), Some(1));
    assert!(
        said.pointer("/result/protocolVersion")
            .and_then(Value::as_str)
            .is_some_and(|version| !version.is_empty()),
        "a server that does not name a protocol version never reaches `connected`, and the lead \
         then has no tools without one word of explanation anywhere"
    );
    assert_eq!(
        said.pointer("/result/serverInfo/name")
            .and_then(Value::as_str),
        Some("loadout"),
        "this name is the one that becomes `mcp__loadout` in --allowedTools, so changing it here \
         is changing argv"
    );
}

/// 2026-08-30 — TO KRYTERIUM POWSTAŁO Z WADY, KTÓREJ POPRZEDNIA WERSJA NIE MOGŁA ZOBACZYĆ.
///
/// Stało tu `assert_eq!(said.pointer("/result"), Some(&tools()))` — czyli porównanie odpowiedzi
/// z tą samą wartością, którą kryterium samo podało na wejściu. Przechodziło przy KAŻDYM
/// opakowaniu, byle po obu stronach było takie samo. Żywy `claude 2.1.251` odrzucił to natychmiast:
/// `result` był gołą tablicą, serwer został na `pending`, a lider napisał człowiekowi
/// „I don't have a loadout tool available in my current toolset".
///
/// Teraz sądzony jest KSZTAŁT, którego chce vendor, wprost — i to jest różnica między kryterium
/// zgodnym z sobą a kryterium zgodnym z rzeczywistością (niezmiennik 20).
#[test]
fn the_tool_list_reply_has_the_shape_the_vendor_reads() {
    let asked = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" });
    let said = serve::local_answer(&asked, &tools()).expect("tools/list has to be answered");

    let listed = said
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .expect(
            "the vendor reads `result.tools`. A bare array there leaves the server on `pending` \
             and the lead tells the person it has no such tool",
        );

    assert_eq!(
        listed.len(),
        4,
        "the four verbs the app greeted with, and nothing invented on the way"
    );
    assert_eq!(
        listed
            .first()
            .and_then(|first| first.get("name"))
            .and_then(Value::as_str),
        Some("ask_the_person"),
        "the bridge repeats the list the app handed it. A list it computed itself would be a \
         process handing itself permissions, and greeting first exists so that it cannot"
    );
}

#[test]
fn a_tool_call_is_not_answered_locally() {
    let asked = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "list_workflows", "arguments": {} },
    });

    assert!(
        serve::local_answer(&asked, &tools()).is_none(),
        "only the app knows what this person has. A bridge answering here would be answering \
         about a library it never read"
    );
}

#[test]
fn an_unknown_method_gets_a_sentence_and_never_silence() {
    let asked = json!({ "jsonrpc": "2.0", "id": 4, "method": "resources/list" });
    let said = serve::local_answer(&asked, &tools()).expect("an unknown method still gets a reply");

    assert!(
        said.pointer("/error/message")
            .and_then(Value::as_str)
            .is_some_and(|said| said.contains("resources/list")),
        "the reply has to name the method. A vendor waiting for an answer that never comes looks \
         exactly like a hung agent, and nothing on the screen says which side stopped"
    );
}

#[test]
fn a_notification_gets_no_reply_at_all() {
    let asked = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });

    assert!(
        serve::local_answer(&asked, &tools()).is_none(),
        "a notification carries no id, so a reply to it is a protocol error on our side, not \
         politeness"
    );
}

/// Dubler aplikacji: oddaje nazwę czasownika, o który poproszono.
struct Echo;

#[async_trait]
impl Answers for Echo {
    async fn answer(&self, call: Call) -> Answer {
        Answer::Ok(json!({ "answered": call.call }))
    }
}

/// Dubler, który zawsze odmawia — gotowym zdaniem dla człowieka.
struct Busy;

#[async_trait]
impl Answers for Busy {
    async fn answer(&self, _call: Call) -> Answer {
        Answer::Refused("Something is already running in this workspace.".to_owned())
    }
}

/// Łączy się z gniazdem, odbiera powitanie i oddaje kanał gotowy do wywołań.
async fn dial(
    bridge: &Bridge,
) -> (
    BufReader<tokio::net::unix::OwnedReadHalf>,
    tokio::net::unix::OwnedWriteHalf,
    Greeting,
) {
    let stream = UnixStream::connect(bridge.at())
        .await
        .expect("the socket the bridge just opened has to accept a connection");
    let (reading, writing) = stream.into_split();
    let mut reading = BufReader::new(reading);
    let mut hello = String::new();
    reading
        .read_line(&mut hello)
        .await
        .expect("the app greets first");
    let greeting: Greeting =
        serde_json::from_str(hello.trim()).expect("the greeting has to be readable");
    (reading, writing, greeting)
}

/// Wysyła wywołanie i oddaje odpowiedź **przeczytaną tak, jak czyta ją most**.
///
/// 2026-08-30 — TO JEST CAŁA POPRAWKA TEGO PLIKU. Wcześniej ta funkcja oddawała surowy
/// `serde_json::Value`, więc żadne kryterium nie pytało, czy most UMIE PRZECZYTAĆ to, co pisze
/// aplikacja. Na żywym `claude 2.1.251` wyszło, że nie umie: `Answer` jest enumem z zewnętrznym
/// tagiem, czyli obiektem o dokładnie jednym kluczu, a aplikacja dokleiła obok `id`. Serwer był
/// `connected`, model wywołał czasownik, aplikacja go odebrała — a do modelu wróciło „Loadout
/// answered in a way this version could not read".
async fn call_it(
    reading: &mut BufReader<tokio::net::unix::OwnedReadHalf>,
    writing: &mut tokio::net::unix::OwnedWriteHalf,
    call: &Call,
) -> Reply {
    let mut line = serde_json::to_vec(call).expect("a call has to encode");
    line.push(b'\n');
    writing.write_all(&line).await.expect("the socket takes it");
    writing.flush().await.expect("and flushes");

    let mut back = String::new();
    reading
        .read_line(&mut back)
        .await
        .expect("every call gets an answer");
    /* TYM SAMYM TYPEM, KTÓRYM CZYTA MOST. Odczyt jako surowy `Value` przechodziłby nad linią,
     * której most nie umie przeczytać — czyli byłby kryterium zgodnym z samym sobą. */
    serde_json::from_str::<Reply>(back.trim())
        .expect("the bridge reads this line as a Reply, so the criterion has to read it the same")
}

#[tokio::test]
async fn the_app_greets_with_exactly_what_this_role_may_use() {
    let home = tempfile::tempdir().expect("a temporary directory");
    let bridge = Bridge::open(home.path(), Role::Lead, Arc::new(Echo))
        .await
        .expect("the bridge opens");

    let (_reading, _writing, greeting) = dial(&bridge).await;

    assert_eq!(
        greeting.tools,
        verbs::tool_list(Role::Lead),
        "the greeting is the whole surface of this session, and the app computes it. A bridge \
         that worked it out itself could widen it"
    );
}

#[tokio::test]
async fn a_step_is_greeted_with_nothing() {
    let home = tempfile::tempdir().expect("a temporary directory");
    let bridge = Bridge::open(home.path(), Role::Step, Arc::new(Echo))
        .await
        .expect("the bridge opens");

    let (_reading, _writing, greeting) = dial(&bridge).await;

    let listed = greeting
        .tools
        .as_array()
        .expect("a greeting always carries the array, even when it is empty");
    assert!(
        listed.is_empty(),
        "a step inside a run is greeted with an empty list, so the model never learns any of \
         these verbs exist. Absent, not refused"
    );
}

#[tokio::test]
async fn the_answer_comes_back_under_the_id_that_asked() {
    let home = tempfile::tempdir().expect("a temporary directory");
    let bridge = Bridge::open(home.path(), Role::Lead, Arc::new(Echo))
        .await
        .expect("the bridge opens");
    let (mut reading, mut writing, _) = dial(&bridge).await;

    let said = call_it(
        &mut reading,
        &mut writing,
        &Call {
            id: json!("call-77"),
            call: "list_workflows".to_owned(),
            input: json!({}),
        },
    )
    .await;

    assert_eq!(
        said.id,
        json!("call-77"),
        "the id travels back with the answer. Without it two turns in one window can be handed \
         each other's results, and a verb that blocks a turn turns that into an answer to \
         somebody else's question"
    );
    assert_eq!(
        said.answer,
        Answer::Ok(json!({ "answered": "list_workflows" })),
        "the answer is the app's, not the bridge's — and it survives the trip in one piece"
    );
}

#[tokio::test]
async fn a_refusal_travels_as_the_sentence_a_person_would_read() {
    let home = tempfile::tempdir().expect("a temporary directory");
    let bridge = Bridge::open(home.path(), Role::Lead, Arc::new(Busy))
        .await
        .expect("the bridge opens");
    let (mut reading, mut writing, _) = dial(&bridge).await;

    let said = call_it(
        &mut reading,
        &mut writing,
        &Call {
            id: json!(9),
            call: "list_workflows".to_owned(),
            input: json!({}),
        },
    )
    .await;

    assert_eq!(
        said.answer,
        Answer::Refused("Something is already running in this workspace.".to_owned()),
        "a refusal is a whole sentence, the same one the person would read on screen. An error \
         code here would reach the model as 'it failed', and the lead would tell the person \
         nothing useful"
    );
}

#[tokio::test]
async fn a_refusal_reaches_the_model_as_a_tool_result_not_a_protocol_error() {
    let refused = Answer::Refused("Nothing to run yet.".to_owned());
    let said = serve::tool_result(&json!(5), &refused);

    assert_eq!(
        said.pointer("/result/isError").and_then(Value::as_bool),
        Some(true),
        "the model has to know the call did not succeed"
    );
    assert_eq!(
        said.pointer("/result/content/0/text")
            .and_then(Value::as_str),
        Some("Nothing to run yet."),
        "and it has to get the sentence itself. Vendors trim a JSON-RPC error down to 'tool \
         failed', which is how a lead ends up repeating a call that can never work"
    );
}

#[tokio::test]
async fn the_socket_is_readable_by_nobody_else() {
    use std::os::unix::fs::PermissionsExt;

    let home = tempfile::tempdir().expect("a temporary directory");
    let bridge = Bridge::open(home.path(), Role::Lead, Arc::new(Echo))
        .await
        .expect("the bridge opens");

    let mode = std::fs::metadata(bridge.at())
        .expect("the socket file is there")
        .permissions()
        .mode()
        & 0o777;

    assert_eq!(
        mode, 0o600,
        "holding the socket IS the capability here, so its permissions are the whole fence. \
         Group or world access would hand every process on this machine the lead's verbs"
    );
}
