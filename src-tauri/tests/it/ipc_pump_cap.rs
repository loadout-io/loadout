//! AC-2 dla T-07: szybki producent dostaje paczkę **z licznika**, nie czekając na zegar.
//!
//! Zegar w tym pliku jest zatrzymany i do ostatniej asercji **ani razu nie przesuwany**.
//! To jest cały pomiar: paczka, która wychodzi przy zatrzymanym zegarze, wyszła z licznika
//! i tylko z niego.
//!
//! **Słaba wersja tego kryterium: sprawdzić, że po 5000 linii i jednym tyknięciu wszystko
//! dotarło.** Przechodzi na pompie wyłącznie zegarowej, która wysyła jedną paczkę 5000 linii —
//! czyli na tej, która przy `find /usr/share` (121 000 linii/s, [T2 §6.1]) wyśle 121 000 linii
//! jednym `evaluate_script`. Rozróżniają je dwie rzeczy: że paczki powstały **przy
//! zatrzymanym zegarze**, i że mają długość **równo 2000**, a nie „nie więcej niż".
//!
//! Limit 2000 jest liczbą z pomiaru, nie z gustu: przy 2000 najgorsza przerwa klatki wynosi
//! 0–1 ms, przy `batch200` i `batch1000` sięga 13–25 ms (`T8-ipcbench-results.txt`).
//!
//! Kolejka jest tu celowo pojemna. Przedmiotem tego kryterium jest sufit **paczki**, a nie
//! sufit **kolejki** — linia odrzucona z braku miejsca w kolejce zaciemniłaby pomiar
//! zdarzeniem, o którym to kryterium nic nie mówi. Przepełnienie kolejki mierzy AC-3.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Result, anyhow};
use loadout_lib::engine::line::Line;
use loadout_lib::ipc::{Sent, line_channel, spawn_pump};
use serde_json::Value;
use tauri::ipc::{Channel, InvokeResponseBody};
use tokio::time::{Instant, advance};

/// Kto produkuje linie.
const AGENT: &str = "builder";

/// Kolejka z zapasem — patrz akapit w nagłówku.
const ROOMY: usize = 8_192;

/// Ile linii wpychamy: dwa pełne sufity i ogon.
const LINES: u64 = 5_000;

/// Ponumerowana linia. Numer jedzie w tekście, bo `Line` nie ma pola sekwencji.
fn line(n: u64) -> Line {
    Line::Note {
        agent: AGENT.to_owned(),
        text: n.to_string(),

        body: Vec::new(),
    }
}

/// Paczki, które **naprawdę wyszły kanałem**, w kolejności wyjścia i w postaci, w jakiej
/// zobaczyłby je webview.
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

    /// Długości paczek, w kolejności wyjścia.
    fn lengths(&self) -> Result<Vec<usize>> {
        let seen = self
            .0
            .lock()
            .map_err(|error| anyhow!("the recorder was poisoned: {error}"))?;
        seen.iter()
            .cloned()
            .map(|body| {
                body.deserialize::<Vec<Value>>()
                    .map(|batch| batch.len())
                    .map_err(Into::into)
            })
            .collect()
    }
}

/// Oddaje sterowanie pompie, **nie ruszając zegara**. `yield_now` zawsze zostawia zadanie
/// gotowe do biegu, więc wirtualny czas nie przeskakuje sam do najbliższego tyknięcia.
///
/// Powtórzeń jest dużo, bo tokio przerywa zadanie po ~128 operacjach na jedno odpytanie
/// (budżet kooperacyjny), a tutaj pompa ma do odebrania pięć tysięcy linii.
async fn settle() {
    for _ in 0..512 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn a_fast_producer_gets_full_batches_from_the_cap_with_the_clock_standing_still() -> Result<()>
{
    let started = Instant::now();
    let delivered = Delivered::default();
    let (sink, source) = line_channel(ROOMY);
    let pump = spawn_pump(source, delivered.channel());

    let queued = (1..=LINES)
        .filter(|n| sink.send(line(*n)) == Sent::Queued)
        .count();
    assert_eq!(
        queued,
        usize::try_from(LINES)?,
        "the queue is deliberately roomy here, so every line is taken; a refusal would mean \
         this test measures the queue instead of the batch cap"
    );

    settle().await;
    assert_eq!(
        started.elapsed(),
        Duration::ZERO,
        "not one virtual millisecond has passed — whatever left the channel below left it \
         because of the COUNT, and the timer had nothing to do with it"
    );
    assert_eq!(
        delivered.lengths()?,
        vec![2_000, 2_000],
        "5000 lines through a 2000 line cap are two full batches, sent the moment the cap is \
         reached, and 1000 lines still waiting. One batch of 5000 here is a timer-only pump: \
         at 121 000 lines/s it hands the webview 121 000 lines in a single `evaluate_script` \
         [T2 §6.1]. Batches of 200 or 1000 would be inside the cap, and both cost 13-25 ms of \
         worst-case frame gap against 0-1 ms at 2000 (T8-ipcbench-results.txt)"
    );

    advance(Duration::from_millis(16)).await;
    settle().await;
    assert_eq!(
        delivered.lengths()?,
        vec![2_000, 2_000, 1_000],
        "and the 1000 that were left over close on the TIMER, one window later — the tail of \
         a burst is not a special case, it is what the end of every burst looks like"
    );

    drop(sink);
    let stats = pump.await?;
    assert_eq!(
        stats.delivered, LINES,
        "the balance the pump reports matches what the channel actually saw, line for line"
    );
    Ok(())
}
