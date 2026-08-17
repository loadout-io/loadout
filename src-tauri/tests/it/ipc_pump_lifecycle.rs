//! AC-5 dla T-07: ostatnie linie biegu nie giną, a martwy kanał kończy pompę.
//!
//! Dwa końce życia pompy i oba są cichymi trybami porażki.
//!
//! **(a) Producent zamyka nadajnik z niepełnym buforem.** Słaba wersja tego kryterium
//! sprawdza, że `JoinHandle` się kończy. Przechodzi na pompie, która na `None` z `recv()` po
//! prostu wychodzi z pętli — i gubi ostatnią, niepełną paczkę, czyli **końcówkę każdego
//! biegu**, w tym wiersz `done` z kosztem. Rozróżnia je policzenie **siedmiu** linii
//! w ostatniej paczce i to, że przyszły **bez** przesuwania zegara do następnego tyknięcia.
//!
//! **(b) Odbiorca zniknął (okno zamknięte).** `Channel::send` zaczyna zwracać `Err`. Pompa ma
//! się skończyć w obrębie jednego tyknięcia, a producent, który wysyła dalej, ma dostać
//! **odmowę zamiast blokady** — inaczej zamknięcie okna zatrzymuje agenta, który biegnie
//! dalej i płaci dalej.

use std::sync::atomic::{AtomicU64, Ordering};
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

/// Kolejka z zapasem: te dwa przypadki nie mierzą przepełnienia (to robi AC-3).
const ROOMY: usize = 64;

/// Okno sklejania.
const FLUSH: Duration = Duration::from_millis(16);

/// Ile linii zostaje w buforze, kiedy producent się kończy.
const TAIL: u64 = 7;

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

/// Kanał, którego odbiorcy już nie ma: liczy próby i każdą odrzuca.
#[derive(Debug, Clone, Default)]
struct Broken(Arc<AtomicU64>);

impl Broken {
    /// Kanał, który pompa dostanie zamiast zamkniętego okna.
    fn channel(&self) -> Channel<Vec<Line>> {
        let tries = Arc::clone(&self.0);
        Channel::new(move |_body| {
            tries.fetch_add(1, Ordering::Relaxed);
            // Dokładnie to, co robi `Channel` po zamknięciu okna: wysyłka nie ma dokąd pójść.
            Err(tauri::Error::WebviewNotFound)
        })
    }

    /// Ile razy pompa próbowała wysłać.
    fn tries(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Oddaje sterowanie pompie, nie ruszając zegara.
async fn settle() {
    for _ in 0..64 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn the_last_lines_of_a_run_leave_before_the_pump_does() -> Result<()> {
    let started = Instant::now();
    let delivered = Delivered::default();
    let (sink, source) = line_channel(ROOMY);
    let pump = spawn_pump(source, delivered.channel());

    let queued = (1..=TAIL)
        .filter(|n| sink.send(line(*n)) == Sent::Queued)
        .count();
    assert_eq!(
        queued,
        usize::try_from(TAIL)?,
        "seven lines fit in a roomy queue"
    );

    // Sześć milisekund w oknie: do tyknięcia zostaje jeszcze dziesięć.
    advance(Duration::from_millis(6)).await;
    settle().await;
    assert_eq!(
        delivered.batches()?.len(),
        0,
        "nothing has left yet — the window has not closed, and this is the state every run \
         ends in: a partial buffer and a producer that has just stopped"
    );

    drop(sink);
    let stats = pump.await?;

    assert_eq!(
        started.elapsed(),
        Duration::from_millis(6),
        "the tail left WITHOUT waiting for the next tick. A pump that flushes only on the \
         timer makes the end of every run arrive up to a window late, and the last thing a \
         user waits for is the `done` line with the cost on it"
    );
    let batches = delivered.batches()?;
    assert_eq!(
        batches.len(),
        1,
        "and it left as one last batch. Zero batches here is the loop that simply exits on \
         `None` from `recv()` — it loses the end of every run, silently, which is the worst \
         way to lose anything"
    );
    assert_eq!(
        batches[0].len(),
        usize::try_from(TAIL)?,
        "carrying all seven lines, not the six that happened to be there a moment earlier"
    );
    assert_eq!(
        stats.delivered, TAIL,
        "and the pump only finishes AFTER that send, so its balance includes the whole tail"
    );
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_channel_that_refuses_ends_the_pump_and_the_producer_gets_a_refusal() -> Result<()> {
    let started = Instant::now();
    let broken = Broken::default();
    let (sink, source) = line_channel(ROOMY);
    let pump = spawn_pump(source, broken.channel());

    let queued = (1..=5)
        .filter(|n| sink.send(line(*n)) == Sent::Queued)
        .count();
    assert_eq!(queued, 5, "five lines fit in a roomy queue");

    advance(FLUSH).await;
    settle().await;
    let stats = pump.await?;

    assert_eq!(
        broken.tries(),
        1,
        "the pump tried to send exactly once and took the refusal for an answer. Zero means \
         it never reached the channel at all; more than one means it keeps shouting into a \
         window that is gone, once per tick, for the rest of the run"
    );
    assert!(
        started.elapsed() < FLUSH * 2,
        "and it ended within one tick of that refusal, at {:?}. A pump that outlives its \
         window is a task nobody can see and nobody can stop",
        started.elapsed()
    );
    assert_eq!(
        stats.delivered, 0,
        "nothing was delivered, because nothing arrived: a send that returned an error is \
         not a delivery"
    );
    assert_eq!(
        stats.delivered + stats.dropped,
        5,
        "and the balance still closes over all five lines the producer handed the pump \
         (invariant 13). `delivered == 0` alone says nothing about WHERE they went: a pump \
         that drops the refused batch out of both numbers looks identical here, and lines \
         that vanish from the balance are the ones nobody notices"
    );

    let before = Instant::now();
    let after_death = sink.send(line(6));
    assert_eq!(
        after_death,
        Sent::Dropped,
        "a producer that keeps going gets a REFUSAL, not a wait. The agent behind this sink \
         is still running and still costing money; the closed window is not its problem"
    );
    assert_eq!(
        before.elapsed(),
        Duration::ZERO,
        "and the refusal cost it no time at all"
    );
    Ok(())
}
