//! AC-3 dla T-07: producent szybszy od UI nie zapycha pamięci, a bilans się zgadza.
//!
//! Agent robiący `find /usr/share` sypie 121 000 linii/s [T2 §6.1]. Kolejka do pompy jest
//! ograniczona (~256, `try_send`) i na `Full` linia jest porzucana i policzona, nigdy
//! kolejkowana bez końca [T7 §4.1]. Zasada jest twarda: **ścieżka dysku nigdy nie gubi,
//! ścieżka UI zawsze może** — zgubiona klatka jest niewidoczna, zgubione zdarzenie to błąd.
//!
//! **Słaba wersja tego kryterium: `assert!(stats.dropped >= 0)` albo sam pomiar, że test się
//! kończy.** Przechodzi na pompie z nieograniczonym `Vec`, bo ona też się kończy — tylko po
//! drodze zjada gigabajt pamięci, której ma starczyć na trzech agentów po 583 MB [T7 §7.1].
//! Rozróżniają je: **twardy sufit** `max_buffered <= 2000` mierzony w środku pompy (bufor,
//! który przypadkiem zdążył się opróżnić, jest nie do odróżnienia od ograniczonego, jeśli
//! patrzeć tylko z zewnątrz) oraz **równość bilansu** — implementacja gubiąca linie bez
//! liczenia nie domknie `delivered + dropped`.
//!
//! # Jak jest tu zrobiony „celowo wolny odbiorca"
//!
//! `Channel::send` jest **synchroniczne** (`fn send(&self, data: TSend) -> Result<()>`,
//! [T2 §6.3]), więc domknięcie kanału nie ma jak zaczekać w wirtualnym czasie — nie da się
//! w nim wykonać `await`. Wolność odbiorcy jest więc zrobiona jedyną drogą, która mierzy to
//! samo: pompa dostaje **dokładnie jedno tyknięcie na każde 500 wyprodukowanych linii**.
//! Producent wypycha swoją porcję nie oddając sterowania, więc kolejka przepełnia się przy
//! każdej porcji, a okno nadąża odebrać tyle, ile zdąży w jednym oknie sklejania. To jest ten
//! sam stan przeciążenia, co w produkcji, i jest powiedziany wprost zamiast udawany.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Result, anyhow};
use loadout_lib::engine::line::Line;
use loadout_lib::ipc::{QUEUE_CAP, Sent, line_channel, spawn_pump};
use serde_json::Value;
use tauri::ipc::{Channel, InvokeResponseBody};
use tokio::time::{Instant, advance};

/// Kto produkuje linie.
const AGENT: &str = "builder";

/// Okno sklejania — tyle wirtualnego czasu test daruje odbiorcy na każdą porcję.
const FLUSH: Duration = Duration::from_millis(16);

/// Ile linii w jednej porcji. Więcej niż pojemność kolejki, więc każda porcja **musi**
/// przepełnić kolejkę: bez tego test zmierzyłby łatwy przypadek.
const CHUNK: u64 = 500;

/// Ile porcji. `CHUNK * CHUNKS == 200_000`.
const CHUNKS: u64 = 400;

/// Ile linii razem.
const LINES: u64 = CHUNK * CHUNKS;

/// Twardy sufit bufora pompy.
const BUFFER_CEILING: usize = 2_000;

/// Ponumerowana linia.
fn line(n: u64) -> Line {
    Line::Note {
        agent: AGENT.to_owned(),
        text: n.to_string(),
    }
}

/// Paczki, które **naprawdę wyszły kanałem**.
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

    /// Ile linii przeszło kanałem naprawdę — policzone z drutu, nie z licznika pompy.
    ///
    /// Niezmiennik 19: liczba, którą pompa sama o sobie mówi, nie jest dowodem. Ta jest
    /// liczona po stronie odbiorcy i musi się zgodzić z tamtą.
    fn lines(&self) -> Result<u64> {
        let seen = self
            .0
            .lock()
            .map_err(|error| anyhow!("the recorder was poisoned: {error}"))?;
        let mut total = 0_u64;
        for body in seen.iter().cloned() {
            let batch = body.deserialize::<Vec<Value>>()?;
            total += u64::try_from(batch.len())?;
        }
        Ok(total)
    }
}

/// Oddaje sterowanie pompie, nie ruszając zegara.
async fn settle(times: usize) {
    for _ in 0..times {
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn an_overloaded_pump_bounds_its_buffer_and_closes_its_balance_to_the_line() -> Result<()> {
    let started = Instant::now();
    let delivered = Delivered::default();
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, delivered.channel());

    let mut queued = 0_u64;
    let mut refused = 0_u64;
    let mut worst_stall = Duration::ZERO;

    for chunk in 0..CHUNKS {
        // Porcja leci bez oddania sterowania: tak wygląda agent, który sypie szybciej, niż
        // okno odbiera. Zegar mierzymy WOKÓŁ porcji, bo to jest chwila, w której zła
        // implementacja czekałaby na miejsce w kolejce zamiast odmówić.
        let before = Instant::now();
        for step in 0..CHUNK {
            match sink.send(line(chunk * CHUNK + step + 1)) {
                Sent::Queued => queued += 1,
                Sent::Dropped => refused += 1,
            }
        }
        worst_stall = worst_stall.max(before.elapsed());

        // Jedno tyknięcie na porcję — cały budżet, jaki dostaje celowo wolny odbiorca.
        advance(FLUSH).await;
        settle(16).await;
    }

    assert_eq!(
        worst_stall,
        Duration::ZERO,
        "no single line ever cost the producer a moment of waiting: a full queue is answered \
         with a refusal, not with a wait. A producer that blocks for room is a producer whose \
         agent slows down because the WINDOW is behind, and the agent is the thing we are \
         paying for"
    );
    assert_eq!(
        started.elapsed(),
        FLUSH * u32::try_from(CHUNKS)?,
        "and the whole run took exactly the ticks this test handed out — not one more. Any \
         extra means something on the producer's path waited for the pump"
    );

    drop(sink);
    let stats = pump.await?;

    assert!(
        stats.max_buffered <= BUFFER_CEILING,
        "the pump's own buffer never went past {BUFFER_CEILING} lines; it stood at {}. This is \
         the assertion an unbounded `Vec` cannot pass, and it has to be measured INSIDE the \
         pump: from outside, a buffer that merely happened to drain looks exactly like a \
         bounded one",
        stats.max_buffered
    );
    assert_eq!(
        stats.delivered + stats.dropped,
        LINES,
        "the balance closes to the line: every one of the {LINES} lines was either delivered \
         or counted as lost. An implementation that drops lines without counting them lands \
         short here, and short is exactly what nobody notices — a missing line looks like an \
         agent that said nothing"
    );
    assert!(
        stats.dropped > 0,
        "and the run really did go into overload, instead of measuring the easy case: \
         {CHUNK} lines per chunk against a queue of {QUEUE_CAP} cannot all fit"
    );
    assert_eq!(
        (stats.delivered, stats.dropped),
        (queued, refused),
        "the two ends agree on both numbers. The count of lost lines is ONE number reported \
         ONE way (invariant 13); a pump counting drops, a channel counting undelivered and a \
         front counting missing numbers are three numbers that nearly agree and none of them \
         is true"
    );
    assert_eq!(
        delivered.lines()?,
        stats.delivered,
        "and what the channel actually carried matches what the pump says it carried — the \
         pump's own count is not evidence about the pump (invariant 19)"
    );
    Ok(())
}
