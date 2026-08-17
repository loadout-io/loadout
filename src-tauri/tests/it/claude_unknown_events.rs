//! AC-3 dla T-04: nieznany typ zdarzenia i linia, która nie jest JSON-em, nie kończą biegu.
//!
//! **Słaba wersja tego kryterium to
//! `assert!(serde_json::from_str::<ClaudeLine>(unknown).is_ok())`.** Przechodzi ją sam
//! `#[serde(other)]` i nie mówi nic o **biegu**: prawdziwą regresją jest `?` w pętli
//! czytającej, który kończy krok na pierwszej linii spoza schematu. Enum ma wtedy wariant
//! `Unknown`, test na deserializacji jest zielony, a krok i tak pada — bo błąd wraca
//! z `push`, nie z serde. Vendorzy dokładają typy zdarzeń co tydzień, po cichu
//! [niezmiennik 5, T7 ryzyko 4].
//!
//! Dlatego mierzymy **całą sekwencję**: szesnaście prawdziwych linii z tej maszyny, trzy
//! linie śmiecia wstrzyknięte między `assistant` a `result`, i pytanie, czy `result`
//! z linii **po** śmieciu wciąż przyszedł. To jest ta jedna asercja, której nie przechodzi
//! implementacja z `?` w pętli.
//!
//! Fikstura jest złotym plikiem tego zadania: 16 prawdziwych linii, nie JSON pisany ręcznie —
//! ręczny zawsze dryfuje w stronę optymizmu [T7 §8.1].

use std::error::Error;

use loadout_lib::engine::drivers::AgentEvent;
use loadout_lib::engine::drivers::claude::ClaudeDecoder;

/// Szesnaście prawdziwych linii `stream-json` z tej maszyny, w tym `rate_limit_event`,
/// `result/success` i `system/init` z `capabilities`.
const FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../docs/research/fixtures/claude-stream.jsonl"
));

/// Ile linii ma mieć fikstura. Asercja, nie komentarz: gdyby ktoś ją przyciął, ten test
/// przechodziłby na krótszej sekwencji i nie zauważylibyśmy tego.
const FIXTURE_LINES: usize = 16;

/// Po czym poznajemy linię wyniku w fiksturze. Klucz `type` stoi w niej na **końcu** obiektu,
/// więc szukamy pary, a nie prefiksu.
const RESULT_TAG: &str = r#""type":"result""#;

/// Typ zdarzenia, którego nikt nigdy nie wysłał. Poprawny JSON, nieznany `type`.
const UNKNOWN_TYPE: &str = r#"{"type":"quantum_flux","payload":{"a":1}}"#;

/// Znany typ z kluczem, którego nasza struktura nie zna — czyli dokładnie to, co vendor
/// dokłada co tydzień.
const INIT_NEW_KEY: &str = r#"{"type":"system","subtype":"init","brand_new_key":42}"#;

/// W ogóle nie JSON. Zdarza się przy ucięciu potoku i przy ostrzeżeniu wypisanym w środek
/// strumienia.
const NOT_JSON: &str = "not json at all";

/// Zdolność ogłaszana przez CLI 2.1.233 w `system/init` — na niej, a nie na numerze wersji,
/// feature-detektuje się przerwanie w paśmie.
const CAPABILITY: &str = "interrupt_receipt_v1";

/// Linie fikstury i indeks linii `result`.
fn fixture() -> Result<(Vec<&'static str>, usize), Box<dyn Error>> {
    let lines: Vec<&'static str> = FIXTURE.lines().collect();
    if lines.len() != FIXTURE_LINES {
        return Err(format!(
            "the fixture holds {} lines, not {FIXTURE_LINES}",
            lines.len()
        )
        .into());
    }
    let cut = lines
        .iter()
        .position(|line| line.contains(RESULT_TAG))
        .ok_or("the fixture holds no result line, so this test would prove nothing")?;
    Ok((lines, cut))
}

/// Ile razy dekoder ogłosił koniec tury.
fn finished(events: &[AgentEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, AgentEvent::Finished(_)))
        .count()
}

/// Czy któreś `Started` niesie tę zdolność.
fn announces(events: &[AgentEvent], capability: &str) -> bool {
    events.iter().any(|event| match event {
        AgentEvent::Started { capabilities, .. } => {
            capabilities.iter().any(|name| name.as_str() == capability)
        }
        _ => false,
    })
}

#[test]
fn neither_an_unknown_type_nor_a_line_of_junk_is_reported_as_a_failure()
-> Result<(), Box<dyn Error>> {
    let (lines, cut) = fixture()?;
    let mut decoder = ClaudeDecoder::new();
    for line in &lines[..cut] {
        decoder.push(line);
    }

    let clean = decoder.unparsed();
    assert_eq!(
        clean, 0,
        "every line of the fixture is real JSON, so nothing should have been dropped yet; \
         starting from a non-zero count would make 'grew by exactly one' meaningless"
    );

    // ── Poprawny JSON, nieznany typ ───────────────────────────────────────────────────────
    let from_unknown = decoder.push(UNKNOWN_TYPE);
    assert!(
        from_unknown.is_empty(),
        "an event type nobody has seen before has nothing to show and nothing to say; it \
         produced {from_unknown:?}"
    );
    assert_eq!(
        decoder.unparsed(),
        clean,
        "an unknown type is recognised, not unparsable: the line was read, it simply means \
         nothing to us. Counting it as junk hides the lines that really were junk"
    );

    // ── Znany typ, nieznany klucz ─────────────────────────────────────────────────────────
    let from_new_key = decoder.push(INIT_NEW_KEY);
    assert!(
        from_new_key
            .iter()
            .any(|event| matches!(event, AgentEvent::Started { .. })),
        "a key the struct has never heard of must not cost us the event that carries it; \
         the line produced {from_new_key:?}"
    );
    assert_eq!(
        decoder.unparsed(),
        clean,
        "a new key on a known event is the normal weekly change at both vendors, not junk"
    );

    // ── W ogóle nie JSON ──────────────────────────────────────────────────────────────────
    let from_junk = decoder.push(NOT_JSON);
    assert!(
        from_junk.is_empty(),
        "a line that is not JSON cannot mean anything; it produced {from_junk:?}"
    );
    assert_eq!(
        decoder.unparsed(),
        clean + 1,
        "junk is counted once, so the debug file says how much of the stream was lost - and \
         counted only once, so the number stays worth reading"
    );

    Ok(())
}

#[test]
fn the_result_that_arrives_after_the_junk_still_ends_the_turn() -> Result<(), Box<dyn Error>> {
    let (lines, cut) = fixture()?;
    let mut decoder = ClaudeDecoder::new();
    let mut events = Vec::new();

    for line in &lines[..cut] {
        events.extend(decoder.push(line));
    }
    assert_eq!(
        finished(&events),
        0,
        "nothing before the result line may claim the turn is over"
    );

    for junk in [UNKNOWN_TYPE, INIT_NEW_KEY, NOT_JSON] {
        events.extend(decoder.push(junk));
    }

    // Wszystko, co zostało — w fiksturze jest to linia `result`. Gdyby pętla czytająca
    // kończyła krok na pierwszej linii spoza schematu, ten wynik nigdy by nie przyszedł
    // i krok zostałby w `running` do końca biegu.
    for line in &lines[cut..] {
        events.extend(decoder.push(line));
    }

    assert_eq!(
        finished(&events),
        1,
        "exactly one end of turn has to survive three junk lines standing in front of it. \
         Zero means a `?` in the reading loop ate the step's result; more than one means two \
         different lines both claimed to end it. The whole sequence produced {events:?}"
    );
    assert!(
        announces(&events, CAPABILITY),
        "the init line has to hand over its capabilities even though the struct does not know \
         most of its keys - that list is what decides whether cancelling can be graceful or \
         has to be a signal. The sequence produced {events:?}"
    );

    Ok(())
}
