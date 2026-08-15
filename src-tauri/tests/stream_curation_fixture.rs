//! AC-1 dla T-05: szesnaście prawdziwych zdarzeń zostawia **dokładnie trzy** wiersze historii.
//!
//! To jest jedyne miejsce, w którym powstaje wartość produktu, i jedyne kryterium, które mierzy
//! ją w całości: wejściem jest cały złoty plik z prawdziwego biegu (16 zdarzeń, 25 584 bajty),
//! a wyjściem historia, którą zobaczy człowiek. Cichy tryb porażki nie wygląda jak awaria —
//! wygląda jak widok, który „działa" i znowu jest ścianą tekstu, bo mapowanie przepuściło
//! `thinking` albo `system/init` (9 929 bajtów, 42% strumienia [T7 §4.3]).
//!
//! **Słaba wersja tego kryterium to
//! `assert!(lines.iter().any(|l| l.kind() == LineKind::Note))`.** Przechodzi ją implementacja,
//! która przepuszcza wszystkie szesnaście zdarzeń, byle jedno z nich było notatką — czyli
//! dokładnie ta, przed którą to zadanie istnieje. Rozróżnia je **długość** (3) **plus cała
//! sekwencja rodzajów** `[Read, Note, Done]`: sama długość przechodzi na mapowaniu, które gubi
//! `read` i dokłada wiersz dla `init`.

use std::path::Path;

use loadout_lib::engine::line::{Line, LineKind};
use loadout_lib::engine::stream;
use tokio::io::BufReader;
use tokio::sync::mpsc;

/// Złoty plik tego zadania: 16 zdarzeń `stream-json` z prawdziwego biegu na tej maszynie.
/// Bajty, nie tekst — ten sam plik mierzy w AC-5 bajtową identyczność tee.
const FIXTURE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../docs/research/fixtures/claude-stream.jsonl"
));

/// Ile bajtów ma mieć fikstura. Asercja, nie komentarz: gdyby ktoś ją przyciął, to kryterium
/// mierzyłoby krótszy strumień i nikt by tego nie zauważył.
const FIXTURE_BYTES: usize = 25_584;

/// Ile zdarzeń niesie fikstura.
const FIXTURE_EVENTS: usize = 16;

/// Ile wierszy ma z nich zostać. Trzynaście zdarzeń nie ma prawa dołożyć ani jednego.
const HISTORY_ROWS: usize = 3;

/// Agent, który to wszystko zrobił. Wchodzi w klucz grupy sklejania, więc musi gdzieś być.
const AGENT: &str = "builder";

/// Proza z fikstury, dosłownie. Jedyny tekst, który w tym strumieniu wolno pokazać.
const PROSE: &str = "Greeting message stored in file.";

/// Plik, który agent przeczytał. Pełna ścieżka, bo rozwinięcie wiersza `read` pokazuje pliki,
/// a nie same nazwy.
const READ_SUFFIX: &str = "/sample.txt";

/// Ile tur, ile milisekund i ile pieniędzy ogłosiła linia `result`.
const TURNS: u32 = 2;
/// Czas trwania z drutu, w milisekundach.
const DURATION_MS: u64 = 6_220;
/// Koszt z drutu, co do bitu. Zaokrąglenie tej liczby gubi ją bezpowrotnie: `Line` jest
/// jedyną rzeczą, którą dostaje widok.
const COST_USD: f64 = 0.148_362_900_000_000_02;

/// Puszcza bajty przez pompę i oddaje historię, którą zobaczyłby człowiek.
///
/// Wejście jedzie przez prawdziwy plik i `BufReader`, a nie przez bufor w pamięci: pompa czyta
/// `impl AsyncBufRead`, więc test ma ją karmić tym samym kształtem, którym karmi ją potok
/// dziecka.
async fn history(bytes: &[u8], dir: &Path) -> anyhow::Result<Vec<Line>> {
    let source = dir.join("stdout.jsonl");
    tokio::fs::write(&source, bytes).await?;
    let reader = BufReader::new(tokio::fs::File::open(&source).await?);

    // Pojemność z ogromnym zapasem: gdyby kanał się zapchał, pompa czekałaby na odbiorcę,
    // który jeszcze nie biegnie, i kryterium skończyłoby się limitem czasu — czyli fałszywą
    // czerwienią, która niczego nie dowodzi.
    let (tx, mut rx) = mpsc::channel(256);
    stream::pump(reader, &dir.join("agent-1.jsonl"), AGENT, tx).await?;

    let mut lines = Vec::new();
    while let Some(line) = rx.recv().await {
        lines.push(line);
    }
    Ok(lines)
}

#[tokio::test]
async fn sixteen_real_events_leave_exactly_three_rows_in_this_order() -> anyhow::Result<()> {
    assert_eq!(
        FIXTURE.len(),
        FIXTURE_BYTES,
        "the golden file is not the one this criterion was written against, so nothing below \
         means what it says"
    );
    assert_eq!(
        FIXTURE
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .count(),
        FIXTURE_EVENTS,
        "the golden file has to hold all sixteen events, including the three hooks and the init \
         line this criterion exists to drop"
    );

    let dir = tempfile::tempdir()?;
    let lines = history(FIXTURE, dir.path()).await?;

    assert_eq!(
        lines.len(),
        HISTORY_ROWS,
        "sixteen events have to leave three rows. More means the mapping let through some of \
         the thirteen that must never be seen — three hook_started, three hook_response, init \
         (42% of the stream), three thinking_tokens, the thinking block, the allowed \
         rate_limit_event and the tool_result that only closes the read row. Fewer means it \
         also dropped something a person needs. The history was {lines:?}"
    );

    let kinds: Vec<LineKind> = lines.iter().map(Line::kind).collect();
    assert_eq!(
        kinds,
        [LineKind::Read, LineKind::Note, LineKind::Done],
        "the three rows are the read, the prose and the closing line, in the order they \
         happened. A length of three with a different sequence is a mapping that lost the read \
         and invented a row for init — the same wall of text, only shorter"
    );

    Ok(())
}

#[tokio::test]
async fn the_three_rows_say_what_the_stream_said() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let lines = history(FIXTURE, dir.path()).await?;
    assert_eq!(
        lines.len(),
        HISTORY_ROWS,
        "without three rows there is nothing to read values out of; the history was {lines:?}"
    );

    // ── Wiersz 1: przeczytany plik ────────────────────────────────────────────────────────
    assert_eq!(
        lines[0].count(),
        1,
        "one Read tool call is one file, so the counter says 1. A row that says anything else \
         is counting events, not files"
    );
    let paths: Vec<&str> = lines[0].paths().iter().map(String::as_str).collect();
    assert_eq!(
        paths.len(),
        1,
        "the read row carries the paths it coalesced, and there was exactly one; it carried \
         {paths:?}"
    );
    assert!(
        paths[0].ends_with(READ_SUFFIX),
        "the row has to carry the FULL path from the stream, because expanding it shows the \
         files. The label the vendor writes for itself keeps only the file name, and a row \
         built from that can never show where the file was. It carried {:?}",
        paths[0]
    );

    // ── Wiersz 2: proza, dosłownie ────────────────────────────────────────────────────────
    assert_eq!(
        lines[1].text(),
        PROSE,
        "the prose is the one thing in this stream a person wants, and it is copied, not \
         summarised"
    );

    // ── Wiersz 3: linia zamykająca ────────────────────────────────────────────────────────
    match &lines[2] {
        Line::Done {
            turns,
            duration_ms,
            cost_usd,
            ..
        } => {
            assert_eq!(
                *turns, TURNS,
                "the turn count is copied from the result line, not counted by us"
            );
            assert_eq!(
                *duration_ms, DURATION_MS,
                "the duration is the vendor's duration_ms, copied. Ours would measure the test"
            );
            assert_eq!(
                cost_usd.map(f64::to_bits),
                Some(COST_USD.to_bits()),
                "the cost is copied to the bit. Rounding it here loses it for good: this row is \
                 the only thing the view ever receives, and $0.15 cannot be turned back into \
                 {COST_USD}"
            );
        }
        other => {
            return Err(anyhow::anyhow!(
                "the last row of the run is the closing line; this one was {other:?}"
            ));
        }
    }

    Ok(())
}
