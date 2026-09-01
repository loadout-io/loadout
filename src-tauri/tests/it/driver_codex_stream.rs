//! AC-2 dla T-10: złoty plik ze spike'u S-3 jest wejściem parsera.
//!
//! **Słaba wersja tego kryterium to `for line in fixture.lines() { … }` z asercjami w środku
//! pętli.** Kiedy plik jest pusty albo krótszy, niż ktokolwiek zakładał, pętla wykonuje się
//! zero razy, test jest zielony i **nie poświadcza niczego** (niezmiennik 19). To jest ta sama
//! rodzina co „czysty przebieg, który nic nie zmierzył".
//!
//! Rozróżniają to trzy strażniki wpisane wprost: `lines.len() >= 4`, `!events.is_empty()`
//! i porównanie **pełnej sekwencji rodzajów** zdarzeń — nie „czy wśród nich jest jakieś `Said`".
//!
//! # Czym jest ten złoty plik i czego w nim nie ma
//!
//! `docs/research/fixtures/codex-stream.jsonl` pochodzi z **prawdziwego** biegu (S-3), ale bieg
//! ten wpadł w limit konta: plik zawiera kopertę awaryjną — `thread.started`, `turn.started`,
//! `error`, `turn.failed` — dokładnie tak, jak opisuje T1 §6.2. To jest pełnoprawne wejście
//! i asertujemy **dokładnie to, co w nim jest**. Złoty plik dopisany z dokumentacji byłby
//! parserem przetestowanym wobec naszych przekonań zamiast wobec tego, co Codex naprawdę
//! wypisuje — i pierwszy prawdziwy strumień rozsypałby się w produkcji.
//!
//! Skutek jest taki, że sam złoty plik nie dotyka ani jednego typu `item.*`. Drugi test w tym
//! pliku sądzi więc mapowanie `item.*` na liniach pisanych ręcznie i jest **oznaczony `[3p]`**
//! (2026-08-19, niezmiennik 24): nazwy typów pochodzą z T1 §6.2 i T2 §9.3, czyli ze źródła
//! trzeciej strony potwierdzonego dokumentacją, a nie z biegu. Kiedy S-3 nagra prawdziwą turę,
//! ten drugi test ma zostać zastąpiony fiksturą, a nie rozbudowany.
//!
//! # Dwie decyzje mapowania, które kryterium zostawia otwarte
//!
//! `turn.started` nie daje zdarzenia — T2 §9.3 stawia przy nim myślnik.
//!
//! `error` i `turn.failed` dają razem `Notice` + `Notice` + `Finished`, a nie dwa `Finished`.
//! Powód jest w AC-5: **dokładnie jedno** `Finished` na turę. Obie linie niosą problem na
//! ekran (T2 §9.3 mapuje obie na `problem`), ale turę zamyka ta, która ją zamyka —
//! `turn.completed` albo `turn.failed` [T1 §8.5].

use std::error::Error;
use std::path::Path;

use loadout_lib::engine::drivers::codex::CodexDecoder;
use loadout_lib::engine::drivers::{AgentEvent, DecodedEvent, FinishReason};
use loadout_lib::engine::line::{Action, Tool};
use loadout_lib::engine::stream::{Decoded, decode_codex};

/// Złoty plik ze spike'u S-3. Ścieżka złożona z `CARGO_MANIFEST_DIR`, więc test nie zależy od
/// tego, skąd go uruchomiono.
const FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../docs/research/fixtures/codex-stream.jsonl"
));

/// Ile linii ma mieć złoty plik **co najmniej**. Strażnik, nie komentarz: przycięty plik
/// przepuściłby test na krótszej sekwencji i nikt by tego nie zauważył.
const FIXTURE_LINES: usize = 4;

/// Identyfikator wątku z pierwszej linii złotego pliku. To on jest uchwytem wznowienia:
/// nieudany bieg z T1 §6.2 miał pod tym numerem plik rollout w `~/.codex/sessions/`.
const THREAD: &str = "01a01b33-ee8d-74e2-b621-6a3159c7683f";

/// Komunikat, który w złotym pliku stoi i w `error`, i w `turn.failed`.
const MESSAGE: &str = "You've hit your usage limit. Visit \
                       https://chatgpt.com/codex/settings/usage to purchase more credits or \
                       try again at Aug 20th, 2026 5:30 AM.";

/// Strumień typów `item.*`, których złoty plik nie zawiera **[3p] 2026-08-19**.
///
/// Nazwy typów i ich pola pochodzą z T1 §6.2 (lista wydobyta z binarki 0.147.0) oraz z tabeli
/// T2 §9.3; kształt pól pozostaje niezweryfikowany prawdziwym biegiem. `item.updated` jest tu
/// celowo: korekta 9 w T1 potwierdza, że ten typ **istnieje**, a żywy licznik czasu jest
/// świadomie poza zakresem T-10 — więc poprawnym mapowaniem jest **zero zdarzeń**, a nie drugi
/// `ToolStart`.
const ITEMS: &str = r#"{"type":"thread.started","thread_id":"01a01b33-3p"}
{"type":"item.started","item":{"type":"command_execution","id":"item_0","command":"cargo test"}}
{"type":"item.updated","item":{"type":"command_execution","id":"item_0","command":"cargo test","status":"in_progress"}}
{"type":"item.completed","item":{"type":"command_execution","id":"item_0","command":"cargo test","exit_code":0,"aggregated_output":"ok"}}
{"type":"item.completed","item":{"type":"command_execution","id":"item_1","command":"false","exit_code":1,"aggregated_output":""}}
{"type":"item.completed","item":{"type":"file_change","id":"item_2","changes":[{"path":"src/a.rs","kind":"modify"},{"path":"src/b.rs","kind":"add"}]}}
{"type":"item.completed","item":{"type":"agent_message","id":"item_3","text":"done"}}
{"type":"item.completed","item":{"type":"reasoning","id":"item_4","text":"weighing two shapes"}}
{"type":"item.started","item":{"type":"web_search","id":"item_5","query":"rust mpsc"}}
{"type":"item.completed","item":{"type":"web_search","id":"item_5","query":"rust mpsc"}}
{"type":"item.started","item":{"type":"mcp_tool_call","id":"item_6","server":"notion","tool":"search"}}
{"type":"item.completed","item":{"type":"mcp_tool_call","id":"item_6","server":"notion","tool":"search"}}
{"type":"turn.completed","usage":{"input_tokens":24763,"cached_input_tokens":24448,"output_tokens":122}}"#;

/// Prawdziwy kształt `McpToolCallThreadItem` z protokołu App Servera Codeksa 0.152.0.
///
/// `status`, `error.message` i `result` są polami typowanymi przez schemat vendora. Nie wolno
/// zastępować ich zgadywaniem po tekście: `failed` jest faktem o wyniku, a wiadomość jest pełnym
/// wyjściem tej porażki, które musi dojechać do ogólnej ochrony przed powtarzaniem narzędzia.
const FAILED_MCP: &str = r#"{"type":"item.started","item":{"type":"mcp_tool_call","id":"browser_1","server":"playwright","tool":"browser_navigate","status":"inProgress"}}
{"type":"item.completed","item":{"type":"mcp_tool_call","id":"browser_1","server":"playwright","tool":"browser_navigate","status":"failed","error":{"message":"browserType.launch: Executable doesn't exist"},"result":null}}"#;

/// Przyszły, niezgodny kształt jednego pola nie może skasować całej znanej pozycji (niezmiennik 5).
const MALFORMED_MCP_STATUS: &str = r#"{"type":"item.completed","item":{"type":"mcp_tool_call","id":"future_1","server":"future","tool":"read","status":{"name":"completed"},"result":"still readable"}}"#;

const MISSING_BROWSER: &str = "browserType.launch: Executable doesn't exist";

/// Rodzaj zdarzenia jako słowo, żeby dało się porównać **całą sekwencję** jednym
/// `assert_eq!`. `AgentEvent` nie implementuje `PartialEq` — i słusznie, bo niesie `Outcome`
/// z liczbami zmiennoprzecinkowymi.
fn kind(event: &AgentEvent) -> &'static str {
    match event {
        AgentEvent::Started { .. } => "Started",
        AgentEvent::Thinking => "Thinking",
        AgentEvent::Said { .. } => "Said",
        AgentEvent::ToolStart { .. } => "ToolStart",
        AgentEvent::ToolEnd { .. } => "ToolEnd",
        AgentEvent::FileEdit { .. } => "FileEdit",
        AgentEvent::RateLimit { .. } => "RateLimit",
        AgentEvent::Notice { .. } => "Notice",
        AgentEvent::Finished(_) => "Finished",
    }
}

/// Przepuszcza cały tekst przez świeży dekoder, linia po linii, i zbiera wszystko, co wyszło.
fn decode_all(text: &str) -> Vec<AgentEvent> {
    let mut decoder = CodexDecoder::new();
    let mut events = Vec::new();
    for line in text.lines() {
        events.extend(decoder.push(line));
    }
    events
}

/// Jak [`decode_all`], ale zachowuje fakty `Tool`, które naprawdę płyną do `forward`.
fn decode_all_with_tool(text: &str) -> Vec<DecodedEvent> {
    let mut decoder = CodexDecoder::new();
    let mut events = Vec::new();
    for line in text.lines() {
        let Decoded::Events(parsed_events) = decode_codex(&mut decoder, line) else {
            continue;
        };
        events.extend(parsed_events);
    }
    events
}

#[test]
fn the_golden_file_produces_exactly_the_sequence_it_carries() -> Result<(), Box<dyn Error>> {
    let lines: Vec<&str> = FIXTURE.lines().collect();
    assert!(
        lines.len() >= FIXTURE_LINES,
        "the golden file has to hold at least {FIXTURE_LINES} lines - the envelope S-3 really \
         recorded. It holds {}, and a loop over a truncated file makes every assertion inside \
         it true about nothing",
        lines.len()
    );

    let events = decode_all(FIXTURE);
    assert!(
        !events.is_empty(),
        "the parser produced nothing at all from a file with {} real lines in it",
        lines.len()
    );

    let kinds: Vec<&str> = events.iter().map(kind).collect();
    assert_eq!(
        kinds,
        vec!["Notice", "Notice", "Finished"],
        "the whole sequence is the assertion, not 'is there a Notice somewhere'. \
         thread.started stores the id and shows nothing, turn.started shows nothing, and the \
         two lines that carry the failure become two notices and exactly one end of turn. \
         It produced {events:?}"
    );

    let AgentEvent::Notice { text } = &events[0] else {
        return Err(format!("the first event should be a notice: {:?}", events[0]).into());
    };
    assert_eq!(
        text, MESSAGE,
        "the notice has to carry what the vendor actually said, verbatim - that sentence is the \
         only thing telling whoever reads it that this was a credit limit and when it lifts"
    );

    let AgentEvent::Finished(outcome) = &events[2] else {
        return Err(format!("the third event should end the turn: {:?}", events[2]).into());
    };
    assert!(
        !outcome.ok,
        "a turn that failed did not succeed. It came out as {outcome:?}"
    );
    assert!(
        matches!(outcome.reason, FinishReason::Failed(_)),
        "turn.failed has to end with a readable reason, because this is the case where somebody \
         asks why. It came out as {:?}",
        outcome.reason
    );
    assert_eq!(
        outcome.session.id, THREAD,
        "thread.started shows nothing, but it is REMEMBERED: this id is the resume handle, the \
         thing T-06 stores next to the step, and the only proof that the first line was read at \
         all. It came out as {:?}",
        outcome.session
    );
    assert_eq!(
        outcome.session.vendor, "codex",
        "the session has to say which adapter minted it, or resuming comes back to the wrong CLI"
    );

    Ok(())
}

#[test]
fn every_item_type_maps_to_its_own_event() {
    let events = decode_all(ITEMS);
    let kinds: Vec<&str> = events.iter().map(kind).collect();

    assert_eq!(
        kinds,
        vec![
            "ToolStart", // command_execution started
            "ToolEnd",   // command_execution completed, exit 0
            "ToolEnd",   // command_execution completed, exit 1
            "FileEdit",  // file_change, changes[0]
            "FileEdit",  // file_change, changes[1]
            "Said",      // agent_message
            "Thinking",  // reasoning
            "ToolStart", // web_search started
            "ToolEnd",   // web_search completed
            "ToolStart", // mcp_tool_call started
            "ToolEnd",   // mcp_tool_call completed
            "Finished",  // turn.completed
        ],
        "the mapping from T2 section 9.3, whole. item.updated is deliberately absent from this \
         list: the type exists (T1 correction 9), but a live timer for command_execution is out \
         of scope for T-10, so the honest mapping is no event rather than a second ToolStart. \
         It produced {events:?}"
    );

    let edits: Vec<&Path> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::FileEdit { path } => Some(path.as_path()),
            _ => None,
        })
        .collect();
    assert_eq!(
        edits,
        vec![Path::new("src/a.rs"), Path::new("src/b.rs")],
        "one FileEdit per entry in changes[], in order. A single event for the whole item would \
         tell the person that one file changed when two did"
    );

    let ok_by_id: Vec<(&str, bool)> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::ToolEnd { id, ok, .. } => Some((id.as_str(), *ok)),
            _ => None,
        })
        .collect();
    assert_eq!(
        ok_by_id,
        vec![
            ("item_0", true),
            ("item_1", false),
            ("item_5", true),
            ("item_6", true),
        ],
        "`ok` comes from exit_code and from nowhere else: a command that exited 1 has to read as \
         failed, or the transcript says the step ran cleanly while the build was broken. The ids \
         have to survive too - that is how ToolEnd finds the line ToolStart opened"
    );

    let said: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::Said { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        said,
        vec!["done"],
        "agent_message is the only prose Codex writes, and item.text is where it lives"
    );
}

#[test]
fn a_typed_mcp_failure_keeps_its_status_target_and_full_error() -> Result<(), Box<dyn Error>> {
    let decoded = decode_all_with_tool(FAILED_MCP);
    assert_eq!(
        decoded.len(),
        2,
        "one typed MCP call has exactly one start and one end; it produced {decoded:?}"
    );

    let DecodedEvent {
        event: AgentEvent::ToolStart { id, .. },
        tool: Some(Tool::Started { action, target }),
    } = &decoded[0]
    else {
        return Err(format!("the MCP start lost its structured target: {:?}", decoded[0]).into());
    };
    assert_eq!(id, "browser_1");
    assert_eq!(*action, Action::Ran);
    assert_eq!(target, "playwright browser_navigate");

    let DecodedEvent {
        event: AgentEvent::ToolEnd { id, ok, summary },
        tool: Some(Tool::Ended { output }),
    } = &decoded[1]
    else {
        return Err(format!("the MCP end lost its structured result: {:?}", decoded[1]).into());
    };
    assert_eq!(id, "browser_1");
    assert!(
        !ok,
        "status=failed must never be rewritten to a successful tool call"
    );
    assert_eq!(summary, MISSING_BROWSER);
    assert_eq!(
        output, MISSING_BROWSER,
        "forward needs the complete typed error, not the server/tool label or a UI summary"
    );
    Ok(())
}

#[test]
fn a_changed_status_shape_does_not_drop_the_rest_of_the_known_item() -> Result<(), Box<dyn Error>> {
    let decoded = decode_all_with_tool(MALFORMED_MCP_STATUS);
    let [
        DecodedEvent {
            event: AgentEvent::ToolEnd { id, ok, .. },
            tool: Some(Tool::Ended { output }),
        },
    ] = decoded.as_slice()
    else {
        return Err(format!(
            "a malformed optional status dropped the otherwise known MCP item: {decoded:?}"
        )
        .into());
    };
    assert_eq!(id, "future_1");
    assert!(
        *ok,
        "an unreadable status is unknown, not evidence of failure"
    );
    assert_eq!(output, "still readable");
    Ok(())
}
