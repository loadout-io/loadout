//! AC-1 dla T-07: wolny producent dostaje swoją paczkę **z zegara**, nie z licznika.
//!
//! To jest ścieżka, której benchmark nie zmierzył, i jedyna, która działa w produkcji.
//! 50 000 linii przy limicie 2000 to dokładnie 25 wysyłek
//! (`coalesce_16ms_cap2000(rep1)/sends=25` w `T8-ipcbench-results.txt`), czyli zmierzono
//! wyłącznie ścieżkę licznika, a zegar **nigdy nie wystrzelił** [T8 ryzyko 3]. Prawdziwy agent
//! produkuje ~7 zdarzeń na sekundę [T2 §6.1], więc pompa, która czeka na 2000 linii, milczy
//! w aplikacji przez minuty — i wygląda to jak zawieszony agent, nie jak zepsuty bufor.
//!
//! **Słaba wersja tego kryterium: wysłać 3 linie, poczekać 100 ms i sprawdzić, że przyszły.**
//! Przechodzi zarówno na pompie z zegarem, jak i na pompie wysyłającej każdą linię z osobna —
//! czyli na tej, która daje 1,5 klatki na sekundę [T8 §5.3]. Rozróżniają je trzy pomiary:
//!
//! 1. **przed progiem**: 0 paczek w 15 ms wyklucza wysyłkę per linia,
//! 2. **dokładnie jedna** paczka po progu wyklucza trzy paczki po jednej linii,
//! 3. **brak pustych paczek w ciszy** wyklucza tykanie na sucho, czyli darmowy
//!    `evaluate_script` bez treści.
//!
//! Zegar jest wirtualny (`start_paused`), więc test mierzy okno sklejania, a nie planistę
//! systemu operacyjnego. `advance` przesuwa go o dokładnie tyle, ile prosimy;
//! [`settle`] oddaje sterowanie pompie **nie ruszając zegara**, więc każda asercja stoi
//! w chwili, którą test nazwał.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Result, anyhow};
use loadout_lib::engine::line::Line;
use loadout_lib::ipc::{Sent, line_channel, spawn_pump};
use serde_json::Value;
use tauri::ipc::{Channel, InvokeResponseBody};
use tokio::time::{Instant, advance};

/// Kto produkuje linie. Jeden agent wystarcza: przedmiotem pomiaru jest chwila wysyłki.
const AGENT: &str = "builder";

/// Kolejka z zapasem. Ten test nie mierzy przepełnienia kolejki (to robi AC-3), więc żadna
/// linia nie ma prawa odpaść z powodu, o którym nie mówi kryterium.
const ROOMY: usize = 64;

/// Ponumerowana linia. Numer jedzie w tekście, bo `Line` nie ma pola sekwencji — a to, czego
/// szukamy, to długość paczki i chwila jej wyjścia.
fn line(n: u64) -> Line {
    Line::Note {
        agent: AGENT.to_owned(),
        text: n.to_string(),
    }
}

/// Paczki, które **naprawdę wyszły kanałem**, w kolejności wyjścia i w postaci, w jakiej
/// zobaczyłby je webview.
///
/// Zapisujemy surowe `InvokeResponseBody`, czyli to, co `Channel::send` oddaje domknięciu —
/// atrapa własnego sinka mierzyłaby drogę, której w produkcji nie ma.
#[derive(Debug, Clone, Default)]
struct Delivered(Arc<Mutex<Vec<InvokeResponseBody>>>);

impl Delivered {
    /// Kanał, który pompa dostanie zamiast okna.
    fn channel(&self) -> Channel<Vec<Line>> {
        let seen = Arc::clone(&self.0);
        Channel::new(move |body| {
            // `std::sync::Mutex` w domknięciu SYNCHRONICZNYM: nie ma tu `await`, więc
            // niezmiennik 8 stoi. Zamek wzięty przed wysyłką zawiesza aplikację raz na
            // tysiąc biegów i jest niewidoczny dla `clippy::await_holding_lock`.
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

/// Oddaje sterowanie planiście tyle razy, żeby pompa zdążyła przerobić wszystko, co ma —
/// **nie ruszając zegara**. `yield_now` zawsze zostawia zadanie gotowe do biegu, więc
/// wirtualny czas nie przeskakuje sam do najbliższego tyknięcia.
async fn settle() {
    for _ in 0..64 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn a_slow_producer_gets_its_batch_from_the_timer_and_not_before_the_window() -> Result<()> {
    let started = Instant::now();
    let delivered = Delivered::default();
    let (sink, source) = line_channel(ROOMY);
    let pump = spawn_pump(source, delivered.channel());

    let taken: Vec<Sent> = (1..=3).map(|n| sink.send(line(n))).collect();
    assert_eq!(
        taken,
        vec![Sent::Queued; 3],
        "three lines from a slow agent fit in the queue with room to spare; a refusal here \
         would mean this test measures the queue instead of the window"
    );

    settle().await;
    assert_eq!(
        started.elapsed(),
        Duration::ZERO,
        "nothing in this test may move the clock on its own — every assertion below stands \
         at the moment the test names, not at the moment the runtime happened to reach"
    );
    assert_eq!(
        delivered.batches()?.len(),
        0,
        "at t=0 the pump has the three lines and has sent NOTHING. A batch here is a pump \
         that ships one message per line: 13.8 µs per line and 1.5 frames per second, which \
         is a frozen window, not a slow one [T8 §5.3]"
    );

    advance(Duration::from_millis(15)).await;
    settle().await;
    assert_eq!(
        delivered.batches()?.len(),
        0,
        "15 ms is still inside the 16 ms window, so still nothing has left. This is the \
         assertion that a 'send every line' pump cannot pass"
    );

    advance(Duration::from_millis(2)).await;
    settle().await;
    let batches = delivered.batches()?;
    assert_eq!(
        batches.len(),
        1,
        "past the window the timer fires ONCE and ships ONE batch. Three batches here means \
         one message per line survived the window; zero means the pump waits for the 2000 \
         line cap, which a real agent producing ~7 events per second never reaches — it \
         looks like a hung agent, not a broken buffer [T8 risk 3, T2 §6.1]"
    );
    assert_eq!(
        batches[0].len(),
        3,
        "and it carries all three lines, because the whole point is that the number of \
         MESSAGES is small, not that the bytes are cheap [T8 §5.2]"
    );

    advance(Duration::from_millis(500)).await;
    settle().await;
    assert_eq!(
        delivered.batches()?.len(),
        1,
        "half a second of silence produces no further batch. A tick that ships an empty Vec \
         is a free `evaluate_script` — cost with no content, thirty times a second, forever"
    );

    drop(sink);
    let stats = pump.await?;
    assert_eq!(
        stats.delivered, 3,
        "and the balance the pump reports matches what the channel actually saw"
    );
    Ok(())
}
