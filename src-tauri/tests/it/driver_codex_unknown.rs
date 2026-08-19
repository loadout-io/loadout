//! AC-3 dla T-10: nieznane, niepełne i niebędące JSON-em linie nie przerywają tury.
//!
//! **Słaba wersja tego kryterium podaje wyłącznie nieznane typy NAJWYŻSZEGO poziomu.**
//! Przechodzi ją także parser, w którym nieznany `item.type` panikuje na brakującym polu albo
//! oddaje `Err`, który kończy turę: `{"type":"turn.hiccup"}` łapie `#[serde(other)]` na
//! zewnętrznym enumie i nic więcej się nie dzieje.
//!
//! Rozróżnia to **kolejność**: po wszystkich sześciu śmieciach musi jeszcze przejść prawdziwe
//! zdarzenie. To jedyna asercja, która dowodzi, że strumień **przeżył**, a nie tylko że nic nie
//! wyprodukował — bo „nic nie wyprodukował" jest prawdą również o parserze, który padł na
//! pierwszej z tych linii.
//!
//! Prawdziwa regresja nie siedzi w typie, tylko w pętli: `let event = serde_json::from_str(&l)?;`
//! kończy krok na pierwszej linii spoza schematu. Enum ma wtedy wariant `Unknown`, test na
//! deserializacji jest zielony, a krok i tak pada.
//!
//! **Szósta linia jest prawdziwa.** `ERROR rmcp::transport::worker: …` przeplotło się z JSON-em
//! na stdout w rzeczywistym biegu [T2 §9.3, „Verified hazard"] — to nie jest wymyślony śmieć,
//! tylko zmierzone zagrożenie, i dlatego stdout i stderr nigdy nie idą przez `2>&1`.
//!
//! **„Zero błędów" jest tu wymuszone SYGNATURĄ, nie asercją:** `CodexDecoder::push` nie zwraca
//! `Result`, więc nie ma czego przepuścić przez `?` w pętli czytającej (niezmiennik 5).
//! Zmierzalną częścią jest licznik: każda z sześciu linii ma zostawić po sobie **dokładnie
//! jeden** wpis, bo liczba porzuconych linii jest tym, co idzie do pliku debug i do zgłoszenia
//! błędu.

use loadout_lib::engine::drivers::AgentEvent;
use loadout_lib::engine::drivers::codex::CodexDecoder;

/// Prawdziwe zdarzenie **przed** śmieciem. Bez niego nie wiadomo, czy dekoder w ogóle działał,
/// zanim zaczęliśmy go psuć.
const GOOD_BEFORE: &str =
    r#"{"type":"item.completed","item":{"type":"agent_message","id":"item_0","text":"before"}}"#;

/// Prawdziwe zdarzenie **po** śmieciu. To ono jest całym kryterium.
const GOOD_AFTER: &str =
    r#"{"type":"item.completed","item":{"type":"agent_message","id":"item_1","text":"after"}}"#;

/// Nieznany typ najwyższego poziomu. Poprawny JSON, typ, którego nikt nigdy nie wysłał.
const UNKNOWN_TOP: &str = r#"{"type":"turn.hiccup"}"#;

/// Nieznany typ **elementu** — ta linia jest tu naprawdę po coś. Vendorzy dokładają typy
/// `item.*` co tydzień, po cichu, i to jest miejsce, w którym parser zwykle pęka: koperta jest
/// znana, wnętrze nie.
const UNKNOWN_ITEM: &str = r#"{"type":"item.completed","item":{"type":"quantum_flux"}}"#;

/// `command_execution` bez `exit_code` i bez `command`.
///
/// Niezmiennik 5 łamie się tu cicho przez `item.exit_code` zadeklarowane jako `i32` zamiast
/// `Option<i32>`: pierwszy `command_execution` w stanie `in_progress` przewraca wtedy całą turę.
/// Bez nazwy komendy i bez kodu wyjścia nie ma z czego zbudować ani etykiety, ani `ok`, więc
/// poprawną odpowiedzią jest cisza — nie zmyślone zdarzenie.
const HALF_COMMAND: &str = r#"{"type":"item.completed","item":{"type":"command_execution"}}"#;

/// Ucięty JSON. Tak wygląda linia, na której proces zginął w połowie zapisu.
const TRUNCATED: &str = r#"{"type":"item.completed","item":{"type":"agent_m"#;

/// Pusta linia. NDJSON kończy się nią przy każdym normalnym wyjściu.
const EMPTY: &str = "";

/// Prawdziwa linia hałasu z prawdziwego biegu [T2 §9.3, „Verified hazard"].
const TRACING_NOISE: &str =
    "ERROR rmcp::transport::worker: transport closed unexpectedly, dropping session";

/// Sześć linii, w tej kolejności, wstrzykniętych między dwa prawdziwe zdarzenia.
const JUNK: [(&str, &str); 6] = [
    ("an unknown top-level type", UNKNOWN_TOP),
    ("an unknown item type", UNKNOWN_ITEM),
    (
        "a command_execution with neither command nor exit_code",
        HALF_COMMAND,
    ),
    ("a truncated JSON line", TRUNCATED),
    ("an empty line", EMPTY),
    ("a real tracing line interleaved on stdout", TRACING_NOISE),
];

/// Proza ze zdarzenia, jeśli to zdarzenie w ogóle jest prozą.
fn said(event: &AgentEvent) -> Option<&str> {
    match event {
        AgentEvent::Said { text } => Some(text.as_str()),
        _ => None,
    }
}

#[test]
fn six_junk_lines_produce_nothing_and_are_each_counted_once() {
    let mut decoder = CodexDecoder::new();

    let opening = decoder.push(GOOD_BEFORE);
    assert_eq!(
        opening.len(),
        1,
        "the decoder has to work BEFORE we start feeding it junk; otherwise 'it produced \
         nothing' below would be true for the boring reason. It produced {opening:?}"
    );

    for (what, line) in JUNK {
        let before = decoder.dropped();
        let events = decoder.push(line);

        assert!(
            events.is_empty(),
            "{what} has nothing to show and nothing to say, so it must produce no event at all. \
             It produced {events:?}"
        );
        assert_eq!(
            decoder.dropped(),
            before + 1,
            "{what} has to land in the dropped-line counter exactly once. That number is what \
             the debug file and the bug report are read from - a line the parser silently let \
             go, and did not count, is a hole nobody can measure"
        );
    }
}

#[test]
fn the_real_event_after_the_junk_still_arrives() {
    let mut decoder = CodexDecoder::new();
    let mut events = decoder.push(GOOD_BEFORE);

    for (_, line) in JUNK {
        events.extend(decoder.push(line));
    }

    // TO jest kryterium. Wszystko powyżej mierzy, że nic nie wyszło — co jest prawdą także
    // o parserze, który padł na pierwszej z tych linii i już nigdy niczego nie przeczytał.
    let after = decoder.push(GOOD_AFTER);
    assert_eq!(
        after.len(),
        1,
        "exactly one Said has to survive six junk lines standing in front of it. Zero means a \
         `?` in the reading loop ate the rest of the turn - and vendors add event types every \
         week, silently, so this is the regression that arrives on its own. It produced {after:?}"
    );
    assert_eq!(
        after.first().and_then(said),
        Some("after"),
        "and it has to be the prose that really came off the wire, not an empty shell. \
         It produced {after:?}"
    );

    events.extend(after);
    let prose: Vec<&str> = events.iter().filter_map(said).collect();
    assert_eq!(
        prose,
        vec!["before", "after"],
        "both real events, in order, with six unreadable lines between them: that is the whole \
         claim. It produced {events:?}"
    );
}
