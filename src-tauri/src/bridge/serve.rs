//! Pętla MCP po stdio — **to biegnie w procesie mostu**, nie w aplikacji.
//!
//! Most startuje `claude` (albo `codex`), czytając wiersz z konfiguracji, którą napisał Loadout.
//! Dzięki temu most stoi w NASZEJ grupie procesów: ginie razem z nią i wchodzi do dowodu śmierci
//! (niezmiennik 6) bez ani jednej linii kodu. Serwer nasłuchujący po stronie aplikacji stałby
//! poza tym dowodem.
//!
//! # Most nie wie, co wolno oferować — dowiaduje się
//!
//! Po połączeniu z gniazdem aplikacja **odzywa się pierwsza** i podaje listę narzędzi tej sesji
//! ([`Greeting`]). Most nie liczy jej sam i nie zna pojęcia roli, więc **nie ma jak poszerzyć
//! własnej powierzchni** — nawet gdyby jego argv ktoś podmienił. Tabela ról zostaje po stronie,
//! która zna człowieka.
//!
//! # Dlaczego rozbiór jest osobną, czystą funkcją
//!
//! [`local_answer`] odpowiada na wszystko, co da się rozstrzygnąć bez pytania aplikacji, i jest
//! funkcją od wiadomości do wiadomości — więc sądzi się ją bez gniazda, bez procesu i bez
//! vendora. Polityka zamknięta w pętli byłaby kodem, którego żadne kryterium nie dotknie.

use std::path::Path;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use super::{Answer, Call, Greeting, Reply};

/// Wersja protokołu MCP, którą ten most ogłasza.
///
/// Zmierzone 2026-08-29 sondą na `claude 2.1.251`: serwer podający tę wartość zgłosił się jako
/// `connected`, a jego narzędzie weszło do `system/init`. Data jest tu treścią — vendor zmienia
/// obsługiwane wersje bez zapowiedzi, a most, który zgłosi nowszą niż CLI, nie połączy się wcale.
const PROTOCOL: &str = "2024-11-05";

/// Jak most nazywa się w `system/init` vendora. To samo słowo, które stoi w `--allowedTools`
/// jako `mcp__loadout`, więc **zmiana tej nazwy jest zmianą argv** i musi iść razem z tamtą.
pub const SERVER: &str = "loadout";

/// Odpowiedź na wiadomość, którą da się rozstrzygnąć **bez pytania aplikacji**.
///
/// `None` znaczy dwie różne rzeczy i to jest świadome: albo to zawiadomienie, na które nie
/// odpowiada się w ogóle (`notifications/*`), albo wywołanie narzędzia, które musi pojechać
/// gniazdem. Rozróżnia je wołający, patrząc na `method` — a nie ta funkcja, bo wtedy
/// musiałaby znać gniazdo.
///
/// Nieznana metoda dostaje **błąd JSON-RPC, nie ciszę**: vendor czekający na odpowiedź, która nie
/// przyjdzie, wygląda dokładnie jak zawieszony agent (niezmiennik 5 w duchu — nie wywalamy się na
/// nieznanym, ale też nie milczymy).
#[must_use]
pub fn local_answer(message: &Value, tools: &Value) -> Option<Value> {
    let id = message.get("id").cloned();
    let method = message.get("method").and_then(Value::as_str)?;

    // Zawiadomienie nie ma `id` i nie ma na nie odpowiedzi. Odpowiedź z `id: null` jest dla
    // drugiej strony błędem protokołu, nie uprzejmością.
    let id = id?;

    match method {
        "initialize" => Some(reply(
            &id,
            &json!({
                "protocolVersion": PROTOCOL,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": SERVER, "version": env!("CARGO_PKG_VERSION") },
            }),
        )),
        "ping" => Some(reply(&id, &json!({}))),
        /* TU I TYLKO TU powstaje kształt odpowiedzi protokołu. Zmierzone 2026-08-30: bez tego
         * opakowania `result` jest gołą tablicą, vendor zostaje na `pending`, a lider mówi
         * człowiekowi, że nie ma takiego narzędzia. */
        "tools/list" => Some(reply(&id, &json!({ "tools": tools.clone() }))),
        // `tools/call` jedzie gniazdem — jego odpowiedź zna wyłącznie aplikacja.
        "tools/call" => None,
        _ => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": format!("Loadout does not answer {method}.") },
        })),
    }
}

/// Wynik narzędzia w kształcie, którego chce MCP.
///
/// Treść jedzie jako tekst, nie jako obiekt: `content` z jednym blokiem `text` jest kształtem,
/// który zmierzyłem jako działający u obu vendorów, a strukturę i tak czyta model, nie kod.
#[must_use]
pub fn tool_result(id: &Value, answer: &Answer) -> Value {
    match answer {
        Answer::Ok(said) => reply(
            id,
            &json!({ "content": [{ "type": "text", "text": text_of(said) }] }),
        ),
        /* ODMOWA JEST WYNIKIEM NARZĘDZIA, NIE BŁĘDEM PROTOKOŁU. Błąd JSON-RPC bywa u vendorów
         * ucinany do „tool failed", a wtedy zdanie mówiące, DLACZEGO Loadout odmówił, nie
         * dociera do modelu — więc lider powtarza to samo wywołanie albo mówi człowiekowi, że
         * „coś nie zadziałało". Zdanie ma dojechać w całości. */
        Answer::Refused(said) => reply(
            id,
            &json!({
                "content": [{ "type": "text", "text": said }],
                "isError": true,
            }),
        ),
    }
}

/// Treść odpowiedzi jako tekst — obiekt jedzie jako JSON, napis jako on sam.
fn text_of(said: &Value) -> String {
    said.as_str().map_or_else(
        || serde_json::to_string_pretty(said).unwrap_or_else(|_| said.to_string()),
        std::borrow::ToOwned::to_owned,
    )
}

/// Poprawna odpowiedź JSON-RPC.
fn reply(id: &Value, result: &Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// Cała praca procesu mostu: stdio po stronie vendora, gniazdo po stronie Loadouta.
///
/// # Kolejność jest treścią
///
/// Najpierw łączymy się z gniazdem i **czekamy na powitanie**, dopiero potem czytamy stdin.
/// Odwrotna kolejność ma okno, w którym vendor pyta o `tools/list`, a most nie wie jeszcze, co
/// wolno mu wymienić — i albo odpowiada pustką (agent bez narzędzi, wyglądający jak zepsuty),
/// albo musi je zgadnąć, czyli sam sobie nadać uprawnienia.
///
/// # Czego tu nie ma
///
/// Ani jednej decyzji o tym, co wolno. Most jest rurą, która umie ramkować MCP.
pub async fn serve(socket: &Path) -> anyhow::Result<()> {
    /* NAZWY ROZŁĄCZNE, nie `link_in`/`link_out`: `clippy::similar_names` czyta taką parę jako
     * pomyłkę czekającą na okazję — a tu pomyłka znaczyłaby odpowiedź wysłaną w stronę, z której
     * przyszło pytanie. */
    let (from_app, mut to_app) = UnixStream::connect(socket).await?.into_split();
    let mut from_app = BufReader::new(from_app);

    let mut hello = String::new();
    from_app.read_line(&mut hello).await?;
    let greeting: Greeting = serde_json::from_str(&hello)?;
    let tools = greeting.tools;

    let mut vendor_in = BufReader::new(tokio::io::stdin());
    let mut vendor_out = tokio::io::stdout();

    let mut said = String::new();
    loop {
        said.clear();
        if vendor_in.read_line(&mut said).await? == 0 {
            // Vendor zamknął stdin: tura się skończyła i most nie ma już komu odpowiadać.
            return Ok(());
        }
        let Ok(message) = serde_json::from_str::<Value>(said.trim()) else {
            // Linia, której nie da się przeczytać, jest porzucana — nigdy nie wywala mostu
            // (niezmiennik 5). Vendor dokłada kształty co tydzień.
            continue;
        };

        if let Some(answer) = local_answer(&message, &tools) {
            write_line(&mut vendor_out, &answer).await?;
            continue;
        }

        let Some(id) = message.get("id").cloned() else {
            continue;
        };
        let Some(call) = Call::from_tool_call(&message) else {
            continue;
        };

        write_line(&mut to_app, &serde_json::to_value(&call)?).await?;

        let mut back = String::new();
        if from_app.read_line(&mut back).await? == 0 {
            // Aplikacja zeszła. Zdanie jedzie do modelu jako wynik narzędzia, żeby lider
            // powiedział człowiekowi, co się stało, zamiast milczeć.
            let gone = Answer::Refused(
                "Loadout is no longer listening, so this could not be \
                                        done. Say it again once the app is back."
                    .to_owned(),
            );
            write_line(&mut vendor_out, &tool_result(&id, &gone)).await?;
            return Ok(());
        }
        let answer = serde_json::from_str::<Reply>(back.trim()).map_or_else(
            |_| {
                Answer::Refused("Loadout answered in a way this version could not read.".to_owned())
            },
            |reply| reply.answer,
        );
        write_line(&mut vendor_out, &tool_result(&id, &answer)).await?;
    }
}

/// Jedna wiadomość, jedna linia. Ten sam kształt po obu stronach mostu.
async fn write_line<W>(sink: &mut W, value: &Value) -> anyhow::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    sink.write_all(&bytes).await?;
    sink.flush().await?;
    Ok(())
}
