//! AC-4 dla T-07: kolejność jest zachowana, paczek jest mało i żadna nie jest pusta.
//!
//! `Channel` jest wybrany między innymi za to, że **gwarantuje kolejność**, w odróżnieniu od
//! `emit` [T8 §5.2]. Tu się to sprawdza — przy mieszance obu dróg wyjścia z bufora: kilka
//! pełnych paczek z licznika plus ogony z zegara.
//!
//! **Słaba wersja tego kryterium: posortować zebrane numery i sprawdzić, że są kompletne.**
//! Sortowanie kasuje dokładnie tę własność, którą mierzymy. Dlatego porównanie jest z
//! `(1..=n)` **bez sortowania**, a drugą asercją jest `batch.len() > 0` na każdej paczce:
//! pusta paczka to darmowy `evaluate_script`, czyli koszt bez treści.
//!
//! Czwarta asercja mówi, po co to wszystko: paczek ma być **mało**. Cała decyzja polega na
//! tym, że wiadomości jest niewiele, a nie na tym, że bajty są tanie — `serde_json` to ~1%
//! kosztu, reszta to skok na główny wątek webviewa [T8 §5.2].

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Result, anyhow};
use loadout_lib::engine::line::Line;
use loadout_lib::ipc::{Sent, line_channel, spawn_pump};
use serde_json::Value;
use tauri::ipc::{Channel, InvokeResponseBody};
use tokio::time::advance;

/// Kto produkuje linie.
const AGENT: &str = "builder";

/// Kolejka z zapasem: przedmiotem pomiaru jest kolejność, nie przepełnienie (AC-3).
const ROOMY: usize = 16_384;

/// Ile linii w jednej porcji: jedna pełna paczka z licznika (2000) plus ogon (500).
const CHUNK: u64 = 2_500;

/// Ile porcji.
const CHUNKS: u64 = 3;

/// Ile linii razem.
const LINES: u64 = CHUNK * CHUNKS;

/// Sufit liczby paczek. 7500 linii ma zmieścić się w garści wiadomości, nie w setkach.
const BATCH_CEILING: usize = 100;

/// Ponumerowana linia. Numer jedzie w tekście, bo `Line` nie ma pola sekwencji — a numer jest
/// tu jedyną treścią, która ma znaczenie.
fn line(n: u64) -> Line {
    Line::Note {
        agent: AGENT.to_owned(),
        text: n.to_string(),

        body: Vec::new(),
    }
}

/// Paczki, które **naprawdę wyszły kanałem**, w kolejności wyjścia.
#[derive(Debug, Clone, Default)]
struct Delivered(Arc<Mutex<Vec<InvokeResponseBody>>>);

impl Delivered {
    /// Kanał, który pompa dostanie zamiast okna.
    fn channel(&self) -> Channel<Vec<Line>> {
        let seen = Arc::clone(&self.0);
        Channel::new(move |body| {
            // `std::sync::Mutex` w domknięciu SYNCHRONICZNYM: nie ma tu `await`, więc
            // niezmiennik 8 stoi.
            if let Ok(mut seen) = seen.lock() {
                seen.push(body);
            }
            Ok(())
        })
    }

    /// Paczki rozpakowane z drutu.
    fn batches(&self) -> Result<Vec<Vec<Value>>> {
        let seen = self
            .0
            .lock()
            .map_err(|error| anyhow!("the recorder was poisoned: {error}"))?;
        seen.iter()
            .cloned()
            .map(|body| body.deserialize::<Vec<Value>>().map_err(Into::into))
            .collect()
    }
}

/// Numer linii, odczytany z drutu.
fn number(value: &Value) -> Result<u64> {
    let text = value
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("a delivered line carries no `text` on the wire: {value}"))?;
    Ok(text.parse::<u64>()?)
}

/// Oddaje sterowanie pompie, nie ruszając zegara.
async fn settle() {
    for _ in 0..512 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn every_line_arrives_once_in_order_and_no_batch_is_empty() -> Result<()> {
    let delivered = Delivered::default();
    let (sink, source) = line_channel(ROOMY);
    let pump = spawn_pump(source, delivered.channel());

    for chunk in 0..CHUNKS {
        // Porcja większa niż sufit paczki: pierwsza paczka wychodzi z licznika…
        let queued = (0..CHUNK)
            .filter(|step| sink.send(line(chunk * CHUNK + step + 1)) == Sent::Queued)
            .count();
        assert_eq!(
            queued,
            usize::try_from(CHUNK)?,
            "the queue is roomy here, so nothing is refused; this test is about order"
        );
        settle().await;
        // …a ogon dopiero z zegara. Obie drogi w jednym biegu, bo w produkcji przeplatają się
        // dokładnie tak: burst, cisza, burst.
        advance(Duration::from_millis(16)).await;
        settle().await;
    }

    drop(sink);
    let stats = pump.await?;
    let batches = delivered.batches()?;

    let empty = batches.iter().filter(|batch| batch.is_empty()).count();
    assert_eq!(
        empty,
        0,
        "not one of the {} batches was empty. An empty batch is a free `evaluate_script`: \
         cost with no content, up to sixty times a second, for as long as the run lasts",
        batches.len()
    );

    let numbers: Vec<u64> = batches
        .iter()
        .flatten()
        .map(number)
        .collect::<Result<Vec<u64>>>()?;
    let expected: Vec<u64> = (1..=LINES).collect();
    assert_eq!(
        numbers, expected,
        "gluing the batches back together gives the numbers rising, without repeats and \
         WITHOUT SORTING. Sorting first would erase the one property being measured — order \
         is half of why this boundary uses a channel instead of the event system [T8 §5.2]"
    );

    let carried: usize = batches.iter().map(Vec::len).sum();
    assert_eq!(
        u64::try_from(carried)?,
        stats.delivered,
        "and the lengths of the batches add up to what the pump says it delivered"
    );

    assert!(
        batches.len() <= BATCH_CEILING,
        "{LINES} lines left as {} messages, and the ceiling is {BATCH_CEILING}. This is the \
         whole decision: one message per line is 13.8 µs and 1.5 frames per second, batching \
         is 0.18 µs and 100+ fps, and the difference is the NUMBER OF MESSAGES, not the bytes \
         [T8 §5.2, §5.3]",
        batches.len()
    );
    Ok(())
}
