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
//! # Dwie drogi wyjścia z bufora, obie obowiązkowe
//!
//! Paczka wychodzi **z zegara** ([`FLUSH`]) albo **z licznika** ([`BATCH_CAP`]) — co przyjdzie
//! pierwsze. Pompa wyłącznie zegarowa oddaje przy `find /usr/share` 121 000 linii jednym
//! `evaluate_script`; pompa wyłącznie licznikowa milczy przez minuty u agenta produkującego
//! ~7 zdarzeń na sekundę i wygląda to jak agent, który się zawiesił.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tauri::ipc::Channel;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant, MissedTickBehavior, interval_at};

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
        if self.tx.try_send(line).is_err() {
            // Kolejka pełna i pompa martwa to z punktu widzenia producenta ta sama
            // odpowiedź: linia nie pojedzie. Rozróżnianie ich tutaj dałoby drugą liczbę
            // opisującą to samo zdarzenie (niezmiennik 13), a agent i tak nie ma z tą
            // różnicą co zrobić — biegnie dalej i płaci dalej.
            //
            // `Release` po tej stronie, `Acquire` po stronie pompy: bilans oddawany przez
            // `JoinHandle` ma widzieć KAŻDY inkrement, także ten zrobiony chwilę przed
            // zamknięciem kolejki.
            self.dropped.fetch_add(1, Ordering::Release);
            return Sent::Dropped;
        }
        Sent::Queued
    }
}

/// Wypycha bufor **jedną** wiadomością i mówi, czy odbiorca jeszcze tam jest.
///
/// Pusty bufor nie wysyła nic. Pusta paczka to `evaluate_script` bez treści — koszt
/// sześćdziesiąt razy na sekundę przez cały bieg, a na ekranie nic. Dlatego cisza jest tu
/// mierzona osobnym kryterium: pół sekundy bez linii ma nie wyprodukować ani jednej
/// wiadomości.
///
/// Synchroniczne, bo `Channel::send` jest synchroniczne. Między wzięciem bufora a wysyłką nie
/// ma więc ani jednego `await` — niezmiennik 8 stoi tu z konstrukcji funkcji, a nie z uwagi
/// w komentarzu.
fn flush(channel: &Channel<Vec<Line>>, buffer: &mut Vec<Line>, stats: &mut PumpStats) -> bool {
    if buffer.is_empty() {
        return true;
    }

    // `mem::take` zostawia w miejscu pusty `Vec`, więc następna paczka zbiera się od zera,
    // a ta, która odjechała, nie jest kopiowana [T8 §5.4].
    let batch = std::mem::take(buffer);
    // Nigdy więcej niż `BATCH_CAP`, bo bufor jest opróżniany dokładnie na tej granicy —
    // konwersja nie ma jak stracić bitu.
    let carried = batch.len() as u64;

    if channel.send(batch).is_err() {
        // Okno zniknęło. Wysyłka zakończona błędem nie jest dostawą, więc `delivered` tej
        // paczki nie liczy — a pompa kończy się w tym samym tyknięciu, w którym się o tym
        // dowiedziała. Pompa, która przełknie odmowę i tyka dalej, jest zadaniem w tle,
        // którego nikt nie widzi i nikt nie zatrzyma; wraca do kanału raz na okno, do końca
        // biegu, i za każdym razem dostaje ten sam błąd.
        return false;
    }

    stats.delivered += carried;
    true
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
    // Zegar rusza TUTAJ, przy wywołaniu — nie przy pierwszym odpytaniu zadania. Różnica jest
    // widoczna dokładnie tam, gdzie boli: zadanie zostaje odpytane dopiero wtedy, kiedy
    // planista odda mu sterowanie, a to bywa całe okno później niż chwila, w której producent
    // oddał pierwszą linię. Okno sklejania liczone od pierwszego odpytania jest więc oknem
    // o nieznanej długości, a `interval` postawiony wewnątrz zadania to milczący sposób,
    // żeby je takim uczynić.
    //
    // `interval_at(now + FLUSH)`, a nie `interval(FLUSH)`: `interval` tyka po raz pierwszy
    // NATYCHMIAST. Ten pierwszy tyk przypada na moment, w którym bufor jest pusty albo dopiero
    // się zapełnia — czyli albo idzie w próżnię, albo rozcina pierwszą paczkę na kawałki
    // wysyłane po jednej linii. Pierwsze okno ma być pełne, jak każde następne.
    let mut ticks = interval_at(Instant::now() + FLUSH, FLUSH);
    // `Delay`, nie `Burst`: po dłuższej ciszy `Burst` nadrabia zaległe tyknięcia jedno po
    // drugim, żeby wrócić na siatkę. Każde z nich zastaje pusty bufor, więc kosztuje tylko
    // przebiegi pętli — ale jest ich tyle, ile trwała cisza, a cisza u prawdziwego agenta
    // trwa minutami [T8 §5.4].
    ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);

    // Kolejka i licznik rozdzielają się dopiero tutaj: do tej pory jechały razem, żeby nie dało
    // się zawołać pompy z cudzym licznikiem (niezmiennik 13).
    let LineSource { mut rx, dropped } = source;

    tokio::spawn(async move {
        // Zwykły `Vec` w zadaniu, nigdy `Mutex<Vec<Line>>` (niezmiennik 8). Pojemność od razu
        // na całą paczkę: bufor rośnie do sufitu w każdym bursta, a realokacja w środku okna
        // sklejania jest kopiowaniem tego, co i tak zaraz odjedzie.
        let mut buffer: Vec<Line> = Vec::with_capacity(BATCH_CAP);
        let mut stats = PumpStats::default();

        loop {
            tokio::select! {
                // `biased`, i to nie jest kosmetyka. Losowa kolejność gałęzi znaczy, że przy
                // gotowym tyknięciu i pełnej kolejce pompa raz zabiera linie, a raz wysyła to,
                // co zdążyła zebrać — czyli ta sama paczka wychodzi w kawałkach albo nie
                // wychodzi wcale, zależnie od losowania. Kolejność jest więc ustalona:
                // NAJPIERW zabierz wszystko, co już stoi w kolejce, POTEM patrz na zegar.
                biased;
                line = rx.recv() => match line {
                    Some(line) => {
                        buffer.push(line);
                        // Mierzone w środku pompy, bo z zewnątrz bufor, który zdążył się
                        // opróżnić, wygląda dokładnie tak samo jak ograniczony.
                        stats.max_buffered = stats.max_buffered.max(buffer.len());
                    }
                    None => break,
                },
                _ = ticks.tick() => {
                    if !flush(&channel, &mut buffer, &mut stats) {
                        break;
                    }
                }
            }
        }

        // Dopiero teraz, bo dopiero teraz jest kompletny: kolejka oddała `None`, czyli ostatni
        // producent zniknął i żaden inkrement już nie dojdzie.
        stats.dropped = dropped.load(Ordering::Acquire);
        stats
    })
}
