//! CT-03a: most niesie obraz jako obraz, lista narzędzi mówi prawdę o czasowniku, a krok
//! Codeksa dostaje swój serwer.
//!
//! # Trzy rzeczy, każda zmierzona jako brakująca (IMPLEMENTATION.md §0d)
//!
//! 1. **Obraz.** Do dziś jedyną drogą był `Answer::Ok(json!({"data": …}))`, a `serve::text_of`
//!    robi z tego ładnie sformatowany JSON z base64 w środku. Model dostaje wtedy TEKST — nigdy
//!    obraz. Kontrola negatywna w tym pliku pokazuje dokładnie tę starą drogę, żeby zieleń
//!    kryterium nie mogła pochodzić z niej.
//! 2. **Adnotacja.** Zmierzone 2026-09-07 na `codex-cli 0.153.4`: `codex exec` odmawia KAŻDEGO
//!    wywołania narzędzia zdaniem `MCP tool call requires approval, but approval policy is
//!    never`, dopóki narzędzie nie odda `annotations: {"readOnlyHint": true}`. W tej samej
//!    rozmowie i na tym samym serwerze narzędzie z adnotacją przeszło, a mutujące odbiło się.
//!    Adnotacja jest więc drogą, nie ozdobą — i musi być PRAWDĄ o czasowniku, bo inaczej
//!    kupujemy zieleń kłamstwem.
//! 3. **Argv kroku.** Serwer Loadouta jedzie do `codex exec` tą samą drogą, co połączenia
//!    człowieka (`connections::runtime`), więc opcje `-c` muszą stać PRZED podkomendą — po niej
//!    nie są opcjami globalnymi i CLI je odrzuca.
//!
//! # Dlaczego kryterium obrazu jest OBROTEM, a nie porównaniem dwóch funkcji
//!
//! Bo dokładnie tak zginęła poprzednia wada z 2026-08-30: aplikacja pisała linię, której most
//! nie umiał przeczytać, a wszystkie ówczesne kryteria czytały surowy JSON zamiast tego, co
//! czyta MOST. Serwer był `connected`, model wywołał czasownik, a wróciło „Loadout answered in
//! a way this version could not read". `Answer` jest enumem z zewnętrznym tagiem pod
//! `#[serde(flatten)]`, więc nowy wariant jest zmianą tej linii — i sądzi go tu prawdziwe
//! gniazdo, prawdziwe powitanie i to samo dekodowanie, którego używa `serve::serve`.
//!
//! # Słaba wersja tych kryteriów
//!
//! `assert!(tool_result(&id, &Answer::Image{…})["result"]["content"][0]["type"] == "image")` —
//! dwie funkcje zgadzające się ze sobą, przechodzące także wtedy, gdy linia między nimi nie
//! przechodzi przez gniazdo. I `assert!(listed.iter().any(|t| t["annotations"].is_object()))` —
//! zielone dla implementacji, która nakleja adnotację na WSZYSTKO, czyli dla kłamstwa, przed
//! którym broni ten etap.

use std::collections::BTreeSet;
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use loadout_lib::bridge::host::{Answers, Bridge};
use loadout_lib::bridge::{Answer, Call, Greeting, Reply, Role, serve, verbs};
use loadout_lib::commands::processes::Processes;
use loadout_lib::commands::processes::services::ServiceAccess;
use loadout_lib::connections::runtime;
use loadout_lib::engine::drivers::codex::{build_exec_argv, exec_argv};
use loadout_lib::engine::drivers::{Policy, RunSpec};
use loadout_lib::library::agents::{ServiceGrant, ServiceOperation};

/// Najmniejszy prawdziwy PNG, 1×1. Te bajty mają wrócić z mostu ZNAK W ZNAK: base64 przepisany
/// przez inny koder byłby innym napisem, a model dostałby wtedy obraz, którego aplikacja nie
/// podała.
const PIXEL: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

/// Rodzaj obrazu, jak nazywa go protokół w kluczu `mimeType`.
const PIXEL_KIND: &str = "image/png";

/// Czasownik, którym w tym pliku pytamy o cokolwiek. Musi stać na liście lidera, bo host odmawia
/// nazw spoza powitania, zanim w ogóle zapyta aplikację.
const ANY_VERB: &str = "list_workflows";

/// Identyfikator wywołania — wraca w odpowiedzi i po nim poznajemy, że to nasza odpowiedź.
const ASKED: &str = "call-ct03a";

/// Podkomenda Codeksa, przed którą opcje globalne muszą stać.
const EXEC: &str = "exec";

/// Prefiks nadpisań, którymi serwer Loadouta wchodzi do konfiguracji `codex exec`.
const OURS: &str = "mcp_servers.loadout.";

/// Katalog roboczy kroku. Czysta wartość, nie ścieżka na dysku: budowniczy argv jest funkcją
/// czystą i nie ma prawa niczego szukać.
const CWD: &str = "/loadout/step/ct03a";

/// Czasowniki, które NICZEGO nie zmieniają — i wyłącznie one mają prawo do adnotacji.
///
/// Lista jest wypisana tutaj, a nie policzona z tabeli czasowników, bo policzona zgadzałaby się
/// z kodem zawsze (niezmiennik 20). To jest ludzki osąd o KAŻDYM czasowniku, spisany raz:
/// `ask_the_person` zapisuje potwierdzenie i bije jednorazowy `approvalToken`, a każdy
/// `prepare_*` rejestruje podgląd, którym potem autoryzuje się start — więc żaden z nich tu nie
/// stoi, choć nazwa każdego brzmi niewinnie.
/// 2026-09-08 (CT-07) — CZTERY CZYTELNIKI KONTEKSTU DOSZŁY DO KONTRAKTU, i to jest zmiana
/// zamierzona, nie poluzowanie tej wyroczni. Lead dostał przy polu rozmowy ten sam picker
/// zestawów, co panel kroku, więc musi umieć je odczytać. Każdy z tych czterech naprawdę
/// niczego nie zmienia (`bridge::context::context_tools` deklaruje `read_only: true` na
/// wszystkich), a porównanie dalej idzie na CAŁYM zbiorze: nazwa dopisana tu bez adnotacji
/// w kodzie, albo adnotacja bez nazwy tutaj, nadal przewraca ten test.
const ONLY_READ: [&str; 15] = [
    "list_workflows",
    "list_agents",
    "get_run_status",
    "list_runs",
    "read_run_summary",
    "list_handoffs",
    "read_handoff",
    "list_peers",
    "read_messages",
    "service_status",
    "service_logs",
    "list_context",
    "search_context",
    "read_context",
    "view_context_image",
];

/// Aplikacja oddająca obraz TYPOWANYM wariantem — droga, którą ten etap buduje.
struct Painter;

#[async_trait]
impl Answers for Painter {
    async fn answer(&self, _call: Call) -> Answer {
        Answer::Image {
            data: PIXEL.to_owned(),
            mime: PIXEL_KIND.to_owned(),
        }
    }
}

/// Ta sama aplikacja starą, tekstową drogą: base64 zapakowany w zwykłą odpowiedź.
struct PainterInWords;

#[async_trait]
impl Answers for PainterInWords {
    async fn answer(&self, _call: Call) -> Answer {
        Answer::Ok(json!({ "data": PIXEL, "mimeType": PIXEL_KIND }))
    }
}

/// Pełny obrót: aplikacja odpowiada przez prawdziwe gniazdo, most czyta linię TYM SAMYM typem,
/// którym czyta ją `serve::serve`, i składa z niej wynik narzędzia — czyli to, co naprawdę
/// dociera do modelu.
async fn what_reaches_the_model(answers: Arc<dyn Answers>) -> Result<Value, Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let bridge = Bridge::open(home.path(), Role::Lead, answers).await?;

    let (reading, mut writing) = UnixStream::connect(bridge.at()).await?.into_split();
    let mut reading = BufReader::new(reading);

    let mut hello = String::new();
    reading.read_line(&mut hello).await?;
    // Powitanie czytamy typem mostu, nie surowym JSON-em: gdyby ta linia była dla niego
    // nieczytelna, agent nie miałby ani jednego narzędzia i nic by tego nie powiedziało.
    let _: Greeting = serde_json::from_str(hello.trim())?;

    let mut line = serde_json::to_vec(&Call {
        id: json!(ASKED),
        call: ANY_VERB.to_owned(),
        input: json!({}),
    })?;
    line.push(b'\n');
    writing.write_all(&line).await?;
    writing.flush().await?;

    let mut back = String::new();
    reading.read_line(&mut back).await?;
    /* DOKŁADNIE TE DWA KROKI ROBI `serve::serve`: dekodowanie jako `Reply`, potem `tool_result`.
     * Odczyt surowym `Value` przechodziłby nad linią, której most nie umie przeczytać — czyli
     * byłby kryterium zgodnym z samym sobą, a nie z vendorem (2026-08-30). */
    let reply: Reply = serde_json::from_str(back.trim())?;
    Ok(serve::tool_result(&reply.id, &reply.answer))
}

#[tokio::test]
async fn an_image_answer_reaches_the_model_as_an_image_block() -> Result<(), Box<dyn Error>> {
    let said = what_reaches_the_model(Arc::new(Painter)).await?;

    assert_eq!(
        said.pointer("/result/content/0/type")
            .and_then(Value::as_str),
        Some("image"),
        "the model has to receive an image block. Anything else means the picture arrived as \
         prose about a picture, and no amount of base64 inside text is an image: {said}"
    );
    assert_eq!(
        said.pointer("/result/content/0/mimeType")
            .and_then(Value::as_str),
        Some(PIXEL_KIND),
        "without the kind of picture the vendor cannot decode it, and the block is dropped in \
         silence — which from the outside looks exactly like a model that ignored it: {said}"
    );
    assert_eq!(
        said.pointer("/result/content/0/data")
            .and_then(Value::as_str),
        Some(PIXEL),
        "the bytes have to survive the trip unchanged: {said}"
    );
    assert!(
        said.pointer("/result/content/0/text").is_none(),
        "an image block carrying text as well is two answers to one question: {said}"
    );
    assert!(
        said.pointer("/result/isError").is_none(),
        "a picture that arrived is not a failure: {said}"
    );
    Ok(())
}

/// KONTROLA NEGATYWNA. Bez niej zieleń wyżej przechodziłaby także dla mostu, który po prostu
/// przepisuje każdą odpowiedź do bloku obrazu.
#[tokio::test]
async fn the_same_image_sent_as_text_is_not_an_image_block() -> Result<(), Box<dyn Error>> {
    let said = what_reaches_the_model(Arc::new(PainterInWords)).await?;

    assert_eq!(
        said.pointer("/result/content/0/type")
            .and_then(Value::as_str),
        Some("text"),
        "the old road has no way to say `image`, and pretending otherwise here would hide the \
         whole reason this stage exists: {said}"
    );
    let text = said
        .pointer("/result/content/0/text")
        .and_then(Value::as_str)
        .ok_or("the old road puts the whole answer into one text block")?;
    assert!(
        text.contains(PIXEL),
        "the same bytes travel, but as characters a model reads rather than a picture it sees: \
         {text}"
    );
    assert!(
        said.pointer("/result/content/0/data").is_none(),
        "a text block with a data key would be a shape neither vendor reads: {said}"
    );
    Ok(())
}

/// Wszystkie tabele narzędzi, które ten produkt naprawdę wystawia.
///
/// Krok NIE bierze swojej listy z `verbs::tool_list` — bierze ją z tabeli wiadomości i z tabeli
/// aplikacji. Kryterium pytające wyłącznie `tool_list` byłoby zielone nad ścieżką, po której
/// krok nie chodzi, a to właśnie KROK odbija się od zatwierdzania Codeksa (§0d).
fn every_listed_tool() -> Result<Vec<Value>, Box<dyn Error>> {
    let every_operation = vec![
        ServiceOperation::Read,
        ServiceOperation::Start,
        ServiceOperation::Restart,
        ServiceOperation::Stop,
    ];
    let apps = ServiceAccess::for_lead(
        Arc::new(Processes::new()),
        PathBuf::from(CWD),
        vec![ServiceGrant {
            service: "s_ct03a".to_owned(),
            operations: every_operation,
        }],
        CancellationToken::new(),
    );

    let mut listed = verbs::tool_list(Role::Lead)
        .as_array()
        .ok_or("the lead's tool table is an array of tool definitions")?
        .clone();
    listed.extend(verbs::message_tools().iter().map(verbs::Verb::listed));
    listed.extend(
        apps.tools()
            .as_array()
            .ok_or("the app table is an array of tool definitions")?
            .iter()
            .cloned(),
    );
    Ok(listed)
}

/// Nazwa narzędzia z jego definicji.
fn named(tool: &Value) -> Option<&str> {
    tool.get("name").and_then(Value::as_str)
}

#[test]
fn only_reading_verbs_carry_the_read_only_hint() -> Result<(), Box<dyn Error>> {
    let listed = every_listed_tool()?;

    let hinted: BTreeSet<&str> = listed
        .iter()
        .filter(|tool| {
            tool.pointer("/annotations/readOnlyHint")
                .and_then(Value::as_bool)
                == Some(true)
        })
        .filter_map(named)
        .collect();

    assert_eq!(
        hinted,
        ONLY_READ.into_iter().collect::<BTreeSet<_>>(),
        "the hint has to stand on exactly the verbs that change nothing. Missing means \
         `codex exec` refuses that call outright — `MCP tool call requires approval, but \
         approval policy is never`, measured 2026-09-07 — and the bridge looks broken. Extra \
         means we bought that green with a lie about what the verb does"
    );
    Ok(())
}

/// OSOBNO, i to nie jest powtórzenie asercji wyżej: ta pyta o KAŻDE narzędzie po nazwie, więc
/// mówi, KTÓRY czasownik kłamie, zamiast pokazać dwa różniące się zbiory.
#[test]
fn no_changing_verb_carries_the_read_only_hint() -> Result<(), Box<dyn Error>> {
    let listed = every_listed_tool()?;
    let reading: BTreeSet<&str> = ONLY_READ.into_iter().collect();

    let changing: Vec<&str> = listed
        .iter()
        .filter_map(named)
        .filter(|name| !reading.contains(name))
        .collect();
    for expected in [
        "ask_the_person",
        "start_workflow",
        "send_message",
        "service_start",
    ] {
        assert!(
            changing.contains(&expected),
            "the tables under test have to carry the verbs that change things, or this criterion \
             passes over an empty set: {changing:?}"
        );
    }

    for tool in &listed {
        let name = named(tool).ok_or("every listed tool carries its name")?;
        if reading.contains(name) {
            continue;
        }
        assert_ne!(
            tool.pointer("/annotations/readOnlyHint")
                .and_then(Value::as_bool),
            Some(true),
            "{name} changes state, so this hint would tell the vendor it may run without ever \
             asking the person. The hint is a description of the verb, not a key to the door"
        );
    }
    Ok(())
}

/// A TERAZ TAM, GDZIE CZYTA JĄ MODEL. Dwie asercje wyżej pytają tabele, czyli miejsce, w którym
/// adnotacja powstaje; ta pyta odpowiedź `tools/list`, czyli jedyne miejsce, z którego vendor
/// kiedykolwiek się o niej dowie. Między jednym a drugim mieszka klasa wady, dla której to repo
/// powstało (niezmiennik 29): tabela policzona i lista wysłana to dwie różne rzeczy.
#[tokio::test]
async fn the_tool_list_a_step_receives_carries_the_hint() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let tools = Value::Array(
        verbs::message_tools()
            .iter()
            .map(verbs::Verb::listed)
            .collect(),
    );
    let bridge = Bridge::open_with_tools(home.path(), tools, Arc::new(Painter)).await?;

    let (reading, _writing) = UnixStream::connect(bridge.at()).await?.into_split();
    let mut reading = BufReader::new(reading);
    let mut hello = String::new();
    reading.read_line(&mut hello).await?;
    let greeting: Greeting = serde_json::from_str(hello.trim())?;

    // Most odpowiada na `tools/list` z powitania i tylko z niego — sam listy nie liczy.
    let said = serve::local_answer(
        &json!({ "jsonrpc": "2.0", "id": ASKED, "method": "tools/list" }),
        &greeting.tools,
    )
    .ok_or("tools/list has to be answered")?;
    let sent = said
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .ok_or("the vendor reads `result.tools`")?;

    let hint = |name: &str| -> Option<bool> {
        sent.iter()
            .find(|tool| named(tool) == Some(name))?
            .pointer("/annotations/readOnlyHint")
            .and_then(Value::as_bool)
    };
    assert_eq!(
        hint("read_messages"),
        Some(true),
        "without this, `codex exec` refuses the call and the step looks like a broken bridge: \
         {sent:?}"
    );
    assert_ne!(
        hint("send_message"),
        Some(true),
        "and storing a message is not a read, whatever it costs us in refusals: {sent:?}"
    );
    Ok(())
}

/// `RunSpec` kroku biegu — pierwsza tura, bez wznowienia.
fn step() -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: PathBuf::from(CWD),
        prompt: "look at the picture".to_owned(),
        model: None,
        system_append: None,
        policy: Policy::EditInFolder,
        reaches_the_web: false,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

#[tokio::test]
async fn a_step_with_a_bridge_carries_its_server_and_one_without_stays_byte_exact()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let run_dir = tempfile::tempdir()?;
    let bridge = Bridge::open(home.path(), Role::Step, Arc::new(Painter)).await?;

    // Ta sama droga, którą jadą połączenia człowieka: most jako zwykłe Połączenie, a nadpisania
    // `-c` liczy `connections::runtime`. Druga droga byłaby drugą polityką (niezmiennik 23).
    let configured =
        runtime::for_driver(run_dir.path(), "codex", &[bridge.as_connection()?], |_| {
            None
        })?;
    let argv = exec_argv(&configured, &step());

    let subcommand = argv
        .iter()
        .position(|argument| argument == EXEC)
        .ok_or_else(|| format!("this is not a codex command line at all: {argv:?}"))?;
    let ours: Vec<usize> = argv
        .iter()
        .enumerate()
        .filter(|(_, argument)| argument.starts_with(OURS))
        .map(|(at, _)| at)
        .collect();
    assert!(
        !ours.is_empty(),
        "a step that may use Loadout's own verbs has to carry its server, or `codex exec` runs \
         with no bridge at all and the agent silently has nothing: {argv:?}"
    );
    assert!(
        ours.iter().all(|at| *at < subcommand),
        "`-c` is a global option of `codex`, not of `exec`. Standing after the subcommand it is \
         read as its argument — refused at best, swallowed in silence at worst: {argv:?}"
    );
    let listed = argv
        .iter()
        .find_map(|argument| argument.strip_prefix("mcp_servers.loadout.args="))
        .ok_or_else(|| format!("the server override carries no arguments: {argv:?}"))?;
    assert!(
        listed.contains(&bridge.at().display().to_string()),
        "the server has to be pointed at THIS bridge's socket. Holding the socket is the whole \
         capability here, so the wrong path hands over somebody else's verbs: {listed}"
    );

    /* KONTROLA CO DO BAJTU. Bez niej zieleń przechodzi dla implementacji, która dokłada pustą
     * flagę każdemu krokowi — a pusta flaga połyka następny argument jako swoją wartość. */
    let untouched = runtime::for_driver(run_dir.path(), "codex", &[], |_| None)?;
    assert_eq!(
        exec_argv(&untouched, &step()),
        build_exec_argv(&step()),
        "a step without a bridge must not change by one byte"
    );
    Ok(())
}

#[test]
fn an_answer_shape_the_bridge_cannot_read_becomes_a_sentence() -> Result<(), Box<dyn Error>> {
    for known in [
        json!({ "id": ASKED, "ok": { "workflows": [] } }),
        json!({ "id": ASKED, "error": "Nothing to run yet." }),
        json!({ "id": ASKED, "image": { "data": PIXEL, "mime": PIXEL_KIND } }),
    ] {
        assert!(
            serde_json::from_str::<Reply>(&known.to_string()).is_ok(),
            "the bridge has to read every line the app writes today; one it cannot read reaches \
             the model as an apology instead of an answer: {known}"
        );
    }

    // Kształt z przyszłej wersji aplikacji, o którym ten most nie wie.
    let stranger = json!({ "id": ASKED, "sketch": { "data": PIXEL } });
    assert!(
        serde_json::from_str::<Reply>(&stranger.to_string()).is_err(),
        "a shape this version does not know must not decode into something it is not: {stranger}"
    );

    /* I TO JEST DRUGA POŁOWA KRYTERIUM. Most zamienia nieczytelną linię w odmowę, a odmowa musi
     * dojechać do modelu jako WYNIK NARZĘDZIA: błąd protokołu bywa u vendorów ucinany do „tool
     * failed", a cisza wygląda dokładnie jak zawieszony agent. */
    let said = serve::tool_result(
        &json!(ASKED),
        &Answer::Refused("Loadout answered in a way this version could not read.".to_owned()),
    );
    assert_eq!(
        said.pointer("/result/isError").and_then(Value::as_bool),
        Some(true),
        "the model has to know the call did not succeed: {said}"
    );
    let sentence = said
        .pointer("/result/content/0/text")
        .and_then(Value::as_str)
        .ok_or("an unreadable answer still reaches the model as a text block")?;
    assert!(
        sentence.contains(' ') && sentence.ends_with('.'),
        "what arrives is a finished sentence a person could read, never a code and never \
         silence: {sentence}"
    );
    Ok(())
}
