//! AC-5 dla T-05: tee jest **bajtowo** tym, co wypluło dziecko.
//!
//! To jest plik, który użytkownik wysyła jako dowód, i to on pozwala skasować `loadout.db`
//! (`ARCHITECTURE.md` §2 pyt. 2). W chwili, w której przestaje być bajtowo tym, co przyszło
//! z potoku, kasowanie indeksu przestaje być bezpieczne, a `store/` po cichu staje się
//! poprzednim prototypem.
//!
//! **Słaba wersja tego kryterium to porównanie liczby linii albo
//! `read_to_string(tee) == read_to_string(źródło)` po `trim()`.** Przechodzi ją implementacja,
//! która przepuszcza linię przez `serde_json` w obie strony — a taka zmienia kolejność kluczy,
//! rozwija `<` do `<` i skraca `0.14836290000000002`. Rozróżnia je porównanie **wektorów
//! bajtów** plus linia kończąca się `\r\n` w wejściu: normalizacja końca linii jest jedynym
//! z tych czterech błędów, którego porównanie stringów po `trim()` nie widzi w ogóle.

use std::path::Path;

use loadout_lib::engine::stream;
use tokio::io::BufReader;
use tokio::sync::mpsc;

/// Złoty plik: 16 zdarzeń, 25 584 bajty, kończy się `\n`.
const FIXTURE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../docs/research/fixtures/claude-stream.jsonl"
));

/// Agent, którego strumień to jest.
const AGENT: &str = "builder";

/// Linia kończąca się `\r\n`. `BufReader::lines()` zjada z niej `\r` i wtedy bajtowej
/// identyczności nie da się już osiągnąć — dlatego pętla ma czytać `read_until(b'\n')`.
const CRLF_LINE: &str =
    r#"{"type":"system","subtype":"hook_started","hook_name":"SessionStart:startup"}"#;

/// Escape JSON-owy znaku mniejszości: sześć znaków, ukośnik i `u003c`. W Ruście zapisany
/// z podwójnym ukośnikiem, bo pojedynczy byłby escape'em **Rusta**, a na drucie ma stać ten
/// z JSON-a.
const ESCAPED: &str = "\\u003c";

/// Linia z tym escape'em w środku. Runda przez `serde_json` rozwija go do gołego znaku i plik
/// przestaje być tym, co przyszło z potoku.
///
/// Sklejona z `concat!`, bo w surowym literale ta sekwencja musiałaby stać dosłownie, a wtedy
/// jedno „posprzątanie" pliku przez edytor kasuje całą pułapkę bez śladu w diffie.
const ESCAPED_LINE: &str = concat!(
    r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"a "#,
    "\\u003c",
    r#" b"}]}}"#
);

/// Linia z liczbą, która nie przeżywa rundy przez `f64` w druga stronę.
const NUMBER_LINE: &str = r#"{"type":"result","subtype":"success","is_error":false,"num_turns":2,"duration_ms":6220,"total_cost_usd":0.14836290000000002}"#;

/// Linia, której nikt nie sparsuje. Tee dzieje się PRZED dekodowaniem, więc ta też w nim jest.
const JUNK_LINE: &str = "not json at all";

/// Wejście: cała fikstura plus trzy doklejone linie, z których jedna kończy się `\r\n`.
fn input() -> Vec<u8> {
    let mut bytes = FIXTURE.to_vec();
    bytes.extend_from_slice(CRLF_LINE.as_bytes());
    bytes.extend_from_slice(b"\r\n");
    bytes.extend_from_slice(ESCAPED_LINE.as_bytes());
    bytes.extend_from_slice(b"\n");
    bytes.extend_from_slice(NUMBER_LINE.as_bytes());
    bytes.extend_from_slice(b"\n");
    bytes
}

/// Czy `haystack` zawiera `needle` jako ciąg bajtów.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Kawałek bajtów wokół podanego przesunięcia, czytelnie i krótko.
fn around(bytes: &[u8], at: Option<usize>) -> String {
    let at = at.unwrap_or_default();
    let from = at.saturating_sub(20);
    let to = at.saturating_add(20).min(bytes.len());
    String::from_utf8_lossy(&bytes[from..to]).into_owned()
}

/// Porównuje dwa ciągi bajtów i pada z **krótką** wiadomością.
///
/// `assert_eq!` na dwóch wektorach po 25 KB wypisuje dwadzieścia pięć tysięcy liczb i raport
/// bramki przestaje dać się przeczytać — a to jest dokładnie ta chwila, w której ktoś ma go
/// przeczytać.
fn assert_same_bytes(written: &[u8], expected: &[u8], why: &str) {
    assert_eq!(
        written.len(),
        expected.len(),
        "{why} The tee holds {} bytes and the stream carried {}",
        written.len(),
        expected.len()
    );
    let divergence = written
        .iter()
        .zip(expected)
        .position(|(left, right)| left != right);
    assert!(
        divergence.is_none(),
        "{why} They part company at byte {divergence:?}. There the tee reads {:?} and the \
         stream read {:?}",
        around(written, divergence),
        around(expected, divergence)
    );
}

/// Puszcza bajty przez pompę i oddaje to, co wylądowało w pliku tee.
///
/// Brak pliku czytamy jako pustkę **celowo**: „tee nie powstało" ma paść na porównaniu bajtów,
/// a nie na błędzie wejścia-wyjścia, który bramka słusznie czyta jako fałszywą czerwień.
async fn teed(bytes: &[u8], dir: &Path) -> anyhow::Result<Vec<u8>> {
    let source = dir.join("stdout.jsonl");
    tokio::fs::write(&source, bytes).await?;
    let tee = dir.join("agent-1.jsonl");
    let reader = BufReader::new(tokio::fs::File::open(&source).await?);

    // Odbiornik żyje do końca funkcji: to kryterium jest o ścieżce DYSKU, a ścieżka dysku nie
    // ma prawa zależeć od tego, czy widok nadąża [T7 §4.1].
    let (tx, _rx) = mpsc::channel(256);
    stream::pump(reader, &tee, AGENT, tx).await?;

    Ok(std::fs::read(&tee).unwrap_or_default())
}

#[tokio::test]
async fn the_tee_is_byte_for_byte_what_the_child_wrote() -> anyhow::Result<()> {
    let bytes = input();

    // ── Wejście naprawdę niesie trzy pułapki, o których jest to kryterium ──────────────────
    assert!(
        contains(&bytes, b"\r\n"),
        "without a line ending in CRLF this test cannot see line-ending normalisation at all, \
         and it is the one failure a trimmed string comparison also misses"
    );
    assert!(
        contains(&bytes, ESCAPED.as_bytes()),
        "without the JSON escape in the input this test cannot see a round trip through \
         serde_json, which expands it to the bare character"
    );
    assert!(
        contains(&bytes, b"0.14836290000000002"),
        "without the long number this test cannot see a round trip through f64 formatting"
    );

    let dir = tempfile::tempdir()?;
    let written = teed(&bytes, dir.path()).await?;

    assert_same_bytes(
        &written,
        &bytes,
        "the tee is not what the child wrote. This file is the one a user attaches to a bug \
         report and the one that makes deleting loadout.db safe, so the moment it stops being \
         byte for byte the stream, the index stops being a rebuildable cache. A difference in \
         the middle is a round trip through serde_json (key order, the escape, the long \
         number); a difference in length is a reader that ate the CR.",
    );

    Ok(())
}

#[tokio::test]
async fn a_line_nobody_could_read_is_in_the_tee_all_the_same() -> anyhow::Result<()> {
    let mut bytes = FIXTURE.to_vec();
    bytes.extend_from_slice(JUNK_LINE.as_bytes());
    bytes.extend_from_slice(b"\n");

    let dir = tempfile::tempdir()?;
    let written = teed(&bytes, dir.path()).await?;

    assert!(
        contains(&written, JUNK_LINE.as_bytes()),
        "the tee happens BEFORE decoding, so a line no parser understands is still in the file. \
         A tee written after a successful parse loses exactly the lines a bug report needs. The \
         tee held {} bytes",
        written.len()
    );
    assert_same_bytes(
        &written,
        &bytes,
        "and the rest of the stream is still byte for byte what came in.",
    );

    Ok(())
}
