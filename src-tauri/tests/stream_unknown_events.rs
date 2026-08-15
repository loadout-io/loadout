//! AC-6 dla T-05: nieznane i uszkodzone linie nie przerywają biegu, są w tee i są policzone
//! (niezmiennik 5).
//!
//! **Słaba wersja tego kryterium to `assert!(result.is_ok())`.** Przechodzi ją implementacja,
//! która przy pierwszym nieznanym typie wychodzi z pętli i zwraca `Ok` z tym, co zdążyła —
//! czyli gubi resztę biegu i nikomu nic nie mówi. Krok zostaje wtedy w `running` do końca biegu,
//! bo zdarzenie końca przyszło linię za późno.
//!
//! Rozróżniają je dwie rzeczy naraz: obecność wiersza `done`, który pochodzi z **ostatniego**
//! zdarzenia, oraz licznik `unrecognised == 3`. Licznik zerowy zdradza dekoder, który uszkodzone
//! linie po prostu połknął jako „nic" — a wtedy nikt się nigdy nie dowie, ile strumienia
//! przepadło.

use std::path::Path;

use loadout_lib::engine::line::{Line, LineKind};
use loadout_lib::engine::stream::{self, Stats};
use tokio::io::BufReader;
use tokio::sync::mpsc;

/// Agent, którego strumień to jest.
const AGENT: &str = "builder";

/// Proza, która ma zostać wierszem `note`.
const PROSE: &str = "Greeting message stored in file.";

/// Pięć linii: dobra, nieznany typ, śmieć, znany typ bez wymaganej treści, wynik.
///
/// Kolejność jest całym kryterium: trzy linie nie do przeczytania stoją **przed** wynikiem,
/// więc wynik dochodzi tylko wtedy, gdy pętla po nich nie stanęła.
const LINES: [&str; 5] = [
    r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Greeting message stored in file."}]}}"#,
    r#"{"type":"quantum_flux","x":1}"#,
    "not json at all",
    r#"{"type":"assistant"}"#,
    r#"{"type":"result","subtype":"success","is_error":false,"num_turns":2,"duration_ms":6220,"total_cost_usd":0.14836290000000002,"session_id":"d24ee572-640c-4442-9c15-587dff952b98"}"#,
];

/// Ile z tych pięciu linii nie da się przeczytać jako znane zdarzenie: nieznany typ, śmieć
/// i znany typ bez wymaganej treści.
const UNREADABLE: usize = 3;

/// Wejście jako bajty, każda linia zakończona `\n`.
fn input() -> Vec<u8> {
    let mut bytes = Vec::new();
    for line in LINES {
        bytes.extend_from_slice(line.as_bytes());
        bytes.extend_from_slice(b"\n");
    }
    bytes
}

/// Puszcza wejście przez pompę i oddaje wszystko, co z niej wyszło.
async fn run(dir: &Path) -> anyhow::Result<(Stats, Vec<Line>, Vec<u8>)> {
    let bytes = input();
    let source = dir.join("stdout.jsonl");
    tokio::fs::write(&source, &bytes).await?;
    let tee = dir.join("agent-1.jsonl");
    let reader = BufReader::new(tokio::fs::File::open(&source).await?);

    let (tx, mut rx) = mpsc::channel(256);
    // `?` tutaj jest asercją samą w sobie: pompa, która zwraca błąd na linii spoza schematu,
    // kończy krok w połowie i wygląda to jak awaria agenta, nie jak nasz parser.
    let stats = stream::pump(reader, &tee, AGENT, tx).await?;

    let mut history = Vec::new();
    while let Some(line) = rx.recv().await {
        history.push(line);
    }
    Ok((stats, history, std::fs::read(&tee).unwrap_or_default()))
}

#[tokio::test]
async fn the_event_after_the_broken_line_still_becomes_a_row() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (_, history, _) = run(dir.path()).await?;

    let kinds: Vec<LineKind> = history.iter().map(Line::kind).collect();
    assert_eq!(
        kinds,
        [LineKind::Note, LineKind::Done],
        "the prose came before the three unreadable lines and the result came after them, so \
         both have to be here. A history without the closing row is a loop that stopped at the \
         first line it did not understand — the step then sits in 'running' until the run ends, \
         and it reads like the agent hung. The history was {history:?}"
    );
    // Sam rodzaj nie wystarcza: pusty wiersz `note` też jest rodzaju `Note`, a wtedy proza
    // sprzed uszkodzonych linii przepadła, choć kształt historii wygląda poprawnie.
    assert_eq!(
        history[0].text(),
        PROSE,
        "the row before the three unreadable lines carries the prose the agent wrote, not just \
         the shape of a note. The history was {history:?}"
    );

    Ok(())
}

#[tokio::test]
async fn the_lines_nobody_could_read_are_counted_rather_than_swallowed() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (stats, _, _) = run(dir.path()).await?;

    assert_eq!(
        stats.unrecognised, UNREADABLE,
        "three of the five lines say nothing we can use: an event type nobody has sent before, \
         a line that is not JSON at all, and an assistant line with no message on it. Zero here \
         is a decoder that swallows damage silently, and then nobody ever learns how much of \
         the stream was lost. The stats were {stats:?}"
    );

    Ok(())
}

#[tokio::test]
async fn every_line_is_in_the_tee_including_the_ones_that_meant_nothing() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (_, _, teed) = run(dir.path()).await?;

    let expected = input();
    assert_eq!(
        teed.len(),
        expected.len(),
        "the tee holds {} bytes and the stream carried {}",
        teed.len(),
        expected.len()
    );
    // Porównanie bajtów, wiadomość tekstem: `assert_eq!` na dwóch wektorach wypisuje setki
    // liczb, a to jest ta chwila, w której ktoś ma tę wiadomość przeczytać.
    assert!(
        teed == expected,
        "the tee happens before decoding, so all five lines are in it byte for byte — including \
         the three the decoder could do nothing with, which are exactly the ones a bug report \
         needs. The tee reads {:?} and the stream read {:?}",
        String::from_utf8_lossy(&teed),
        String::from_utf8_lossy(&expected)
    );

    Ok(())
}
