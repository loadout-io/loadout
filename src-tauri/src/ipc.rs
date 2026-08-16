//! Granica z oknem: pompa sklejająca linie w paczki i kanał, którym te paczki jadą do
//! webviewa.
//!
//! **To jest jedyny plik poza `main.rs`/`lib.rs`, w którym wolno napisać słowo `tauri`**
//! (`docs/ARCHITECTURE.md` §3, niezmiennik 1). `Line` przekracza granicę wyłącznie przez
//! `serde`, a stąd nie wychodzi nic, co `engine/` musiałoby zaimportować z powrotem — inaczej
//! silnik przestaje się dać przetestować bez okna i osobny daemon nigdy nie powstanie.
//!
//! # Dlaczego pompa w ogóle istnieje
//!
//! Jedna wiadomość na linię to zmierzone **13,8 µs/linię i 1,5 klatki na sekundę**: WKWebView
//! jest wtedy zapchany kolejką `eval` i okno jest zawieszone, nie „wolne". Sklejanie 16 ms /
//! 2000 linii daje **0,18 µs/linię i 100–111 fps** [T8 §5.3]. Optymalizuje się **liczbę
//! wiadomości**, nie bajty: i `emit`, i `Channel` to jeden `evaluate_script` na wiadomość,
//! a `serde_json` to ~1% kosztu [T8 §5.2].
//!
//! Dwie ścieżki wyjścia z bufora są równie ważne i tylko jedna była mierzona. Benchmark
//! wysłał 50 000 linii przy limicie 2000 i zrobił **dokładnie 25 wysyłek**
//! (`coalesce_16ms_cap2000(rep1)/sends=25`), czyli **zegar nigdy nie wystrzelił** [T8 ryzyko
//! 3]. Prawdziwy agent produkuje ~7 zdarzeń na sekundę [T2 §6.1], więc w produkcji działa
//! **wyłącznie** ścieżka zegara — ta, której nikt nie zmierzył. Pompa czekająca na 2000 linii
//! jest w benchmarku doskonała i w aplikacji milczy przez minuty, a wygląda to jak zawieszony
//! agent, nie jak zepsuty bufor.
//!
//! # Ścieżka dysku nigdy nie gubi, ścieżka UI zawsze może
//!
//! Agent robiący `find /usr/share` sypie 121 000 linii/s [T2 §6.1]. Kolejka do pompy jest
//! **ograniczona** ([`QUEUE_CAP`]) i pisze się do niej przez `try_send`: kiedy jest pełna,
//! linia jest porzucana i policzona, nigdy kolejkowana bez końca [T7 §4.1]. Zgubiona klatka
//! jest niewidoczna, zgubione zdarzenie to błąd — dlatego gubi **tylko** ta droga, a nie
//! zapis na dysk.
//!
//! Porzuconych linii jest **jedna** liczba i wraca **jedną** drogą: licznik żyje w parze
//! [`LineSink`]/[`LineSource`], a oddaje go [`PumpStats`] razem z `delivered` (niezmiennik
//! 13). Pompa licząca porzucone, kanał liczący niedostarczone i front liczący brakujące
//! numery to trzy liczby, które prawie się zgadzają i żadna nie jest prawdą.
//!
//! # Stan tego pliku: SZKIELET (2026-08-16)
//!
//! Ciała funkcji zwracają **świadomie złą wartość** i są tak oznaczone. To jest wymagany
//! kształt fazy, w której powstają kryteria: test ma się skompilować i paść **w czasie
//! wykonania, na braku ZACHOWANIA** (`AGENTS.md` §2a p. 5). `todo!()` tu nie stoi, bo
//! `clippy::todo` jest w tym drzewie `deny`, a `checks/full-clippy.sh` woła clippy
//! z `-D warnings` — czerwień bramki zamiast czerwieni kryterium niczego by nie poświadczyła.
//! Przy każdym ciele stoi osobno, dlaczego na tym stubie nie da się przejść żadnego
//! kryterium.

use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use tauri::ipc::Channel;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Duration;

use crate::engine::line::Line;

/// Okno sklejania. 16 ms to jedna klatka przy 60 Hz: dłużej widać jako opóźnienie, krócej
/// nie kupuje już nic, bo koszt siedzi w liczbie wiadomości [T8 §5.3].
pub const FLUSH: Duration = Duration::from_millis(16);

/// Sufit jednej paczki. Liczba z pomiaru, nie z gustu: przy 2000 najgorsza przerwa klatki
/// wynosi 0–1 ms, przy `batch200` i `batch1000` sięga 13–25 ms
/// (`T8-ipcbench-results.txt`).
pub const BATCH_CAP: usize = 2_000;

/// Pojemność kolejki producent → pompa [T7 §4.1]. Ograniczona, żeby `Vec` nie rósł w tle:
/// pamięci ma starczyć na trzech agentów po 583 MB [T7 §7.1].
pub const QUEUE_CAP: usize = 256;

/// Co się stało z linią oddaną pompie.
///
/// Wartość, nie błąd: przepełniona kolejka do okna jest **normalnym** stanem szybkiego agenta,
/// a nie awarią biegu (niezmiennik 7 w duchu).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sent {
    /// Linia stoi w kolejce do pompy i zostanie dostarczona.
    Queued,
    /// Kolejka była pełna albo pompa już nie żyje. Linia przepadła i **jest policzona**.
    Dropped,
}

/// Bilans jednego biegu pompy, oddawany przez [`JoinHandle`], kiedy pompa się kończy.
///
/// `delivered + dropped` musi domykać się co do sztuki wobec liczby linii, które producent
/// oddał — implementacja gubiąca linie bez liczenia nie ma jak tego spełnić.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PumpStats {
    /// Ile linii naprawdę wyszło kanałem, zsumowane po paczkach.
    pub delivered: u64,
    /// Ile linii przepadło na pełnej kolejce. **Jedna** liczba, **jedna** droga
    /// (niezmiennik 13).
    pub dropped: u64,
    /// Najwyższy stan bufora pompy w całym biegu. Twardy sufit mierzony w środku, nie
    /// zgadywany z zewnątrz: to jest jedyna asercja, która odróżnia bufor ograniczony od
    /// takiego, który po prostu zdążył się opróżnić.
    pub max_buffered: usize,
}

/// Nadajnik linii do pompy. **Nigdy nie blokuje producenta**: na pełnej kolejce linia jest
/// porzucana i policzona (`try_send`, [T7 §4.1]).
///
/// Klonowalny, bo linie sypie kilku agentów naraz do jednej pompy — jedno okno sklejania na
/// bieg, nie jedno na agenta.
#[derive(Debug, Clone)]
pub struct LineSink {
    /// Ograniczona kolejka do pompy.
    tx: mpsc::Sender<Line>,
    /// Wspólny licznik porzuconych. Trzymany tutaj **i** w [`LineSource`], żeby bilans
    /// wracał jedną drogą, choć liczy go druga strona (niezmiennik 13).
    dropped: Arc<AtomicU64>,
}

/// Odbiornik linii dla pompy — kolejka razem z licznikiem porzuconych.
///
/// Licznik jedzie **z** kolejką, a nie osobnym argumentem, bo tylko tak nie da się zawołać
/// [`spawn_pump`] z cudzym licznikiem i dostać bilansu, który się nie domyka.
#[derive(Debug)]
pub struct LineSource {
    /// Ograniczona kolejka od producentów.
    rx: mpsc::Receiver<Line>,
    /// Wspólny licznik porzuconych; pompa przepisuje go do [`PumpStats`].
    dropped: Arc<AtomicU64>,
}

/// Buduje parę nadajnik/odbiornik o zadanej pojemności.
///
/// Pojemność jest argumentem, a nie stałą wpisaną w środku, żeby test mógł oddzielić
/// przepełnienie kolejki od sufitu paczki. Produkcja podaje [`QUEUE_CAP`].
#[must_use]
pub fn line_channel(capacity: usize) -> (LineSink, LineSource) {
    let (tx, rx) = mpsc::channel(capacity);
    let dropped = Arc::new(AtomicU64::new(0));
    (
        LineSink {
            tx,
            dropped: Arc::clone(&dropped),
        },
        LineSource { rx, dropped },
    )
}

impl LineSink {
    /// Oddaje linię pompie i mówi, czy została przyjęta.
    ///
    /// Synchroniczna i bez `await` z rozmysłem: `try_send` albo ma miejsce, albo nie ma —
    /// czekanie na miejsce jest dokładnie tym blokowaniem producenta, którego ta granica
    /// zabrania (nadawca to pętla czytająca stdout agenta, a agent ma nie zwalniać przez to,
    /// że okno nie nadąża).
    #[must_use]
    pub fn send(&self, line: Line) -> Sent {
        // SZKIELET (2026-08-16). Świadomie zła wartość: linia jest porzucana, ale meldowana
        // jako przyjęta i NIE jest liczona. Żadnego kryterium nie da się na tym przejść —
        // AC-1..AC-2 i AC-4..AC-6 nie zobaczą ani jednej paczki, a AC-3 dostanie
        // `dropped == 0` i bilans `0 != 200_000`. Stub, który by liczył porzucone, zamknąłby
        // bilans AC-3 zerem po stronie `delivered` i to byłby stub PRZECHODZĄCY kryterium.
        let _ = (&self.tx, &self.dropped, line);
        Sent::Queued
    }
}

/// Startuje pompę: linie z `source` wychodzą `channel`em jako paczki, najwyżej raz na
/// [`FLUSH`] albo po [`BATCH_CAP`] linii — co przyjdzie pierwsze.
///
/// Bufor pompy to zwykły `Vec` **w zadaniu**, nigdy współdzielony `Mutex<Vec<Line>>`
/// (niezmiennik 8): zamek wzięty przed wysyłką zawiesza aplikację raz na tysiąc biegów,
/// a `clippy::await_holding_lock` widzi tylko oczywisty przypadek.
///
/// `JoinHandle` oddaje [`PumpStats`], bo koniec biegu jest jedynym momentem, w którym bilans
/// jest kompletny — i dlatego pompa musi się kończyć sama, a nie być zabijana z zewnątrz.
#[must_use]
pub fn spawn_pump(source: LineSource, channel: Channel<Vec<Line>>) -> JoinHandle<PumpStats> {
    tokio::spawn(async move {
        // SZKIELET (2026-08-16). Świadomie zła wartość: zadanie kończy się natychmiast,
        // nie wysyłając ani jednej paczki, i oddaje pusty bilans. Kryterium przechodzące
        // na tym stubie nie istnieje — każde z sześciu rustowych mierzy albo moment
        // wysyłki, albo bilans, a tutaj nie ma ani jednej wysyłki i bilans jest zerem.
        //
        // Rozbiór na pola, a nie `let _ = source`: pole, którego nikt nie czyta, jest
        // `dead_code`, a to jest `-D warnings` w bramce — czerwień lintu zamiast czerwieni
        // kryterium.
        let LineSource { rx, dropped } = source;
        let _ = (rx, dropped, channel);
        PumpStats::default()
    })
}
