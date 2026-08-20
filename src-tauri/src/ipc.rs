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
//! Paczka wychodzi więc **z zegara** ([`FLUSH`]) albo **z licznika** ([`BATCH_CAP`]) — co
//! przyjdzie pierwsze, i obie drogi są obowiązkowe.
//!
//! # Pompa kończy się sama, na dwa sposoby i tylko na te dwa
//!
//! Kiedy producent zniknął, ostatnia — niepełna — paczka wychodzi **przed** końcem zadania;
//! pętla, która na tym wyjściu po prostu wychodzi, gubi końcówkę każdego biegu, w tym wiersz
//! `done` z kosztem. Kiedy kanał odmówił, pompa kończy się w tym samym tyknięciu: pompa, która
//! przełknie odmowę i tyka dalej, jest zadaniem w tle, którego nikt nie widzi i nikt nie
//! zatrzyma. Trzeciego wyjścia nie ma i nie ma go z powodu, który widać w [`PumpStats`] —
//! bilans jest kompletny dopiero w chwili końca, więc pompy nie wolno zabijać z zewnątrz.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use tauri::State;
use tauri::ipc::{Channel, Invoke};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant, MissedTickBehavior, interval_at};

use crate::commands::{self, Drivers, RunControl, RunDeps, RunRequest};
use crate::engine::line::Line;
use crate::library::agents::Agent;
use crate::store::Store;
use crate::workflow::WorkflowFile;
use crate::workflow::check::Note;

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

impl LineSource {
    /// Linia, która już czeka w kolejce — albo `None`, kiedy nic nie czeka.
    ///
    /// # Po co to jest, skoro jedynym konsumentem jest pompa
    ///
    /// Bo bez tego **nie da się osądzić, co bieg wypuścił**, inaczej niż budując
    /// `tauri::ipc::Channel` i deserializując `InvokeResponseBody` — czyli mierząc pompę tam, gdzie
    /// pytanie dotyczy zawartości. Kryterium „tura człowieka wchodzi do historii"
    /// (`tests/it/person_turn_is_visible.rs`) pyta o JEDEN wiersz w strumieniu i nie ma powodu
    /// przechodzić przez serializację, żeby go zobaczyć.
    ///
    /// `try_recv`, nie `recv`: „nic nie przyszło" jest odpowiedzią, o którą te kryteria pytają
    /// wprost (zdanie odrzucone nie ma prawa zostawić wiersza), a `recv().await` zawieszałby test
    /// dokładnie w przypadku, w którym poprawnym wynikiem jest pustka.
    pub fn try_next(&mut self) -> Option<Line> {
        self.rx.try_recv().ok()
    }
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
        //
        // Ale te linie producent oddał pompie, a kanał ich nie przyjął: skoro nie są dostawą,
        // są stratą i muszą być POLICZONE. Bez tego wiersza `delivered + dropped` nie domyka
        // się wobec tego, co producent oddał (niezmiennik 13, doc `PumpStats`) — paczka
        // z ostatniego tyknięcia znika z obu liczb naraz, czyli dokładnie tak, jak znika
        // linia, której nikt nie zauważy.
        stats.dropped += carried;
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
    // Zegar rusza TUTAJ, przy wywołaniu — nie przy pierwszym odpytaniu zadania. Zadanie zostaje
    // odpytane dopiero wtedy, kiedy planista odda mu sterowanie, a to bywa całe okno później
    // niż chwila, w której producent oddał pierwszą linię: okno sklejania liczone od pierwszego
    // odpytania ma nieznaną długość, a `interval` postawiony wewnątrz zadania to milczący
    // sposób, żeby je takim uczynić.
    // ZMIERZONE 2026-08-16 przez przeniesienie tych dwóch wierszy do środka `spawn`: pompa
    // dowiaduje się o martwym kanale okno za późno i kończy się dopiero na tyknięciu, którego
    // nikt nie zamówił (`ipc_pump_lifecycle.rs:185`).
    //
    // `interval_at(now + FLUSH)`, a nie `interval(FLUSH)`: `interval` tyka po raz pierwszy
    // NATYCHMIAST. Ten pierwszy tyk przypada na moment, w którym bufor jest pusty albo dopiero
    // się zapełnia — czyli albo idzie w próżnię, albo rozcina pierwszą paczkę na kawałki.
    // ZMIERZONE 2026-08-16 przez podmianę na `interval(FLUSH)`: trzy linie wolnego producenta
    // wychodzą w chwili t=0, zamiast poczekać na okno (`ipc_pump_timer.rs:117`).
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
                // Kolejność gałęzi ustalona, nie losowana: NAJPIERW zabierz wszystko, co już
                // stoi w kolejce, POTEM patrz na zegar. Bez tego tyknięcie, które wygra
                // losowanie przy niepustej kolejce, wydaje wiadomość na to, co pompa zdążyła
                // zebrać, a reszta czeka całe następne okno — ta sama sekwencja linii daje
                // wtedy różny podział na paczki w kolejnych biegach.
                //
                // ZMIERZONE 2026-08-16: tego NIE łapie żadne z ośmiu kryteriów. Pompa bez
                // `biased` przechodzi wszystkie osiem, także w powtórzeniach. Zostaje tu za
                // determinizm podziału na paczki — to on czyni pomiar „25 wysyłek" [T8 ryzyko
                // 3] powtarzalnym — ale niczym nie jest poświadczony i przy zmianie tej linii
                // trzeba sprawdzić ręcznie, co się dzieje z długościami paczek.
                biased;
                line = rx.recv() => {
                    let Some(line) = line else {
                        // Kolejka oddała `None`, czyli ostatni producent zniknął — a w buforze
                        // została niepełna paczka. Wychodzi TERAZ, bez czekania na tyknięcie,
                        // i dopiero potem kończy się zadanie. Pętla, która na `None` po prostu
                        // wychodzi, gubi końcówkę KAŻDEGO biegu — w tym wiersz `done`
                        // z kosztem, czyli dokładnie tę jedną linię, na którą użytkownik
                        // czeka. Gubi ją po cichu, bo bieg wygląda na skończony.
                        //
                        // Odpowiedź kanału jest tu bez znaczenia: to i tak ostatnia paczka,
                        // a zaraz za nią stoi wyjście z pętli.
                        flush(&channel, &mut buffer, &mut stats);
                        break;
                    };

                    buffer.push(line);
                    // Mierzone w środku pompy, bo z zewnątrz bufor, który zdążył się
                    // opróżnić, wygląda dokładnie tak samo jak ograniczony.
                    stats.max_buffered = stats.max_buffered.max(buffer.len());

                    // Druga droga wyjścia, i to ona jako jedyna trzyma sufit pamięci: przy
                    // 121 000 linii na sekundę [T2 §6.1] czekanie na tyknięcie znaczy 1 900
                    // linii w buforze na każdą milisekundę zwłoki okna. Sufit jest liczbą
                    // z pomiaru: przy 2000 najgorsza przerwa klatki wynosi 0-1 ms, przy 200
                    // i przy 1000 sięga 13-25 ms (`T8-ipcbench-results.txt`).
                    if buffer.len() >= BATCH_CAP && !flush(&channel, &mut buffer, &mut stats) {
                        break;
                    }
                },
                _ = ticks.tick() => {
                    if !flush(&channel, &mut buffer, &mut stats) {
                        break;
                    }
                }
            }
        }

        // Pętla ma dwa wyjścia i tylko w jednym ta liczba jest ostateczna. Kiedy kolejka oddała
        // `None`, ostatni producent zniknął i żaden inkrement już nie dojdzie. Kiedy wyszliśmy
        // na odmowie kanału, producent biegnie dalej — i to jest w porządku: od tej chwili
        // każda jego linia przepada z definicji, bo okna nie ma, a nie z braku miejsca
        // w kolejce. Bilans opisuje więc bieg POMPY, i tak jest czytany po drugiej stronie
        // `JoinHandle`.
        // `+=`, nie `=`: na wyjściu przez odmowę kanału `flush` zdążył już dopisać do tej
        // liczby paczkę, której okno nie przyjęło. Przypisanie kasowałoby ją po cichu, a bilans
        // wyglądałby na policzony — obie drogi straty schodzą się w JEDNEJ liczbie, oddawanej
        // JEDNĄ drogą (niezmiennik 13), więc muszą się sumować, nie nadpisywać.
        stats.dropped += dropped.load(Ordering::Acquire);
        stats
    })
}

// ── STAN, KTÓRY SKORUPY ROZPAKOWUJĄ ────────────────────────────────────────────────────────

/// Katalog biblioteki, w którym leżą pliki workflow. Ta sama nazwa, co
/// w `commands::workflows` — i to jest jedyna rzecz, którą ten plik o tamtym katalogu wie.
const WORKFLOWS_DIR: &str = "workflows";

/// Co powiedzieć KAŻDEMU drugiemu startowi, kiedy pierwszy bieg jeszcze nie zszedł.
///
/// Jedno zdanie na obie drogi (`/ask` i bieg z pliku), bo pytanie jest jedno: „czy coś już
/// idzie". Osobne brzmienie per komenda znaczyłoby, że człowiek czyta o tej samej odmowie co
/// innego zależnie od tego, którym przyciskiem ją wywołał (niezmiennik 13).
///
/// ZDANIE NAZYWA NASTĘPNY RUCH (DESIGN §8), bo odmowa bez wyjścia zostawia człowieka dokładnie
/// tam, gdzie był. Mówi też DLACZEGO: bez powodu czyta się to jak ograniczenie na złość, a
/// prawdziwy powód jest finansowy — Loadout prowadzi jeden bieg naraz, więc drugi uchwyt
/// znaczyłby, że Stop sięga do biegu drugiego, a pierwszy pracuje dalej i dalej płaci
/// (niezmienniki 6 i 11).
const ALREADY_GOING: &str = "A run is already going, and Loadout leads one at a time so that \
                             Stop always reaches the one that is working. Press Stop first, \
                             then ask again.";

/// Ci współpracownicy biegu, którzy **przeżywają jedno wywołanie komendy**.
///
/// To jest dokładnie ta połowa [`RunDeps`], której nie da się przysłać z okna: baza otwarta raz
/// na aplikację, katalogi rozstrzygnięte przy starcie i uchwyt do żywego biegu — po nim Stop
/// sięga do środka czegoś, co uruchomiła **inna** komenda. Reszta (który plik, ile naraz)
/// przyjeżdża argumentem, bo to są odpowiedzi człowieka udzielone w tej chwili, a nie stan
/// aplikacji.
///
/// Struktura stoi w tym pliku, bo `State<'_, _>` jest pojęciem Tauri, a `ipc.rs` jest jedynym
/// miejscem, które ma prawo je znać (`docs/ARCHITECTURE.md` §3). Warstwa `commands/` jej nie
/// widzi i widzieć nie ma: `*_inner` biorą `&RunDeps`, który da się zbudować w teście
/// jednostkowym w sześciu wierszach (niezmiennik 23).
///
/// 2026-08-17 — **nikt jej jeszcze nie oddaje builderowi.** `.manage(…)` mieszka
/// w `src-tauri/src/lib.rs`, a tego pliku T-30 nie posiada; tam też mieszka fabryka
/// [`Drivers`], której to drzewo dziś nie ma nigdzie poza testami. Dopóki człowiek nie dopisze
/// tych dwóch rzeczy, trzy komendy biegu są zarejestrowane i odmawiają wywołania zdaniem
/// „state not managed" — głośno, w chwili pierwszego kliknięcia. Zgłoszone zamiast
/// rozstrzygnięte tutaj (AGENTS.md §7).
pub struct AppState {
    /// `~/.loadout` — biblioteka użytkownika.
    home: PathBuf,
    /// Katalog projektu, pod którym powstaje `.loadout/runs/<ts>__<id>/`.
    project: PathBuf,
    /// Indeks biegów. Otwarty raz: drugie połączenie **zapisujące** to zakleszczenie, nie
    /// „czasem wolniej" (niezmiennik 2).
    store: Store,
    /// Fabryka sterowników vendorów.
    drivers: Drivers,
    /// Uchwyt do biegu, który idzie **teraz**.
    ///
    /// `std::sync::Mutex` i nigdy trzymany przez `await` (niezmiennik 8): każde wzięcie tego
    /// zamka mieści się w jednym wyrażeniu, które kopiuje uchwyt i oddaje zamek, a dopiero
    /// kopia jedzie w bieg. Zamek trzymany przez bieg zawiesiłby Stop na czas biegu — czyli
    /// dokładnie wtedy, kiedy Stop jest do czegokolwiek potrzebny.
    ///
    /// Wymieniany na świeży przy każdym starcie, KTÓRY DOSZEDŁ DO SKUTKU
    /// ([`AppState::begin_run`]), bo anulowanie jest monotoniczne: uchwyt raz zatrzymany zostaje
    /// zatrzymany, więc drugi bieg na tym samym uchwycie startuje jako już anulowany i kończy się
    /// w milisekundach z samymi `cancelled`. To wygląda jak szybki bieg, nie jak awaria
    /// (niezmiennik 7).
    ///
    /// 2026-08-20 (T-69) — „KTÓRY DOSZEDŁ DO SKUTKU" jest tu całą treścią poprawki. Do tego dnia
    /// start z płótna wymieniał to pole BEZWARUNKOWO, także pod żywym biegiem — a wtedy Stop
    /// czytał uchwyt DRUGIEGO biegu, pierwszy pracował dalej i dalej płacił, i nie było już
    /// nikogo, kto mógłby zażądać od niego dowodu śmierci grupy (niezmienniki 6 i 11). Kto teraz
    /// odmawia i dlaczego jednym ciałem, stoi przy [`AppState::begin_run`].
    live: Mutex<RunControl>,
    /// Rozmowa z orchestratorem. `None`, dopóki okno jej nie otworzyło.
    ///
    /// # Dlaczego `tokio::sync::Mutex`, a nie ten sam rodzaj co przy [`AppState::live`]
    ///
    /// Bo wysłanie tury do orchestratora JEST `await`-em: głos to kanał, a start sesji odpala
    /// proces. Zamek `std::sync` trzymany przez `await` jest tym, czego zabrania niezmiennik 8
    /// i co `Cargo.toml` odrzuca lintem `await_holding_lock`. Tamten zamek trzyma się przez jedno
    /// wyrażenie kopiujące uchwyt; ten trzyma się przez rozmowę z procesem.
    ///
    /// Jedna na aplikację, nie jedna na zakres — i to jest do przemyślenia, kiedy zakresy dostaną
    /// własne sesje (`workspace::Registry`). Dziś przełączenie zakresu zostawia rozmowę tam, gdzie
    /// była, bo folder jedzie argumentem przy każdym zdaniu.
    ///
    /// # 2026-08-20 — WĄTEK PER ZAKRES ISTNIEJE I NIE STOI TUTAJ, I JEST TO ZGŁOSZENIE
    ///
    /// [`commands::chat::Threads`] robi dokładnie to, co obiecuje akapit wyżej: trzyma wątek na
    /// zakres, kieruje wiersze do strumienia tego zakresu i przy zamknięciu okna oddaje po jednym
    /// dowodzie śmierci grupy na wątek. Nie da się go tu jednak podstawić w połowie: `Threads::say`
    /// wymaga [`commands::chat::Lead`], czyli WSKAZANEGO agenta, a wskazania nie ma czym dowieźć
    /// z okna. [`say_to_orchestrator`] musiałaby dostać klucz `lead` obok `folder`, co znaczy zmianę
    /// w `src/sections/run/io.ts` — a mandat T-60 na tamten plik (należący do niewyładowanego T-41)
    /// pozwala dopisać WYŁĄCZNIE klucz `folder` przy [`open_chat`]. Nowej komendy nie da się dodać
    /// obok, bo `tests/it/ipc_commands_registered.rs` porównuje listę handlera
    /// z `src-tauri/commands.golden.txt` co do sztuki.
    ///
    /// Podstawienie samej połowy byłoby gorsze niż zostawienie tego stanu: rozmowa, w której każde
    /// zdanie odbija się o „wskaż lidera", jest odmową, której człowiek nie ma jak spełnić. Dopóki
    /// człowiek nie rozstrzygnie tego jednego pytania, żywa rozmowa idzie tą drogą, ze zaszytym
    /// vendorem z [`AppState::chat_driver`].
    chat: tokio::sync::Mutex<Option<commands::chat::Chat>>,
    /// Miejsce na jeden draft umiejętności i token tego, który pisze teraz.
    ///
    /// **Osobne pole, nie [`AppState::live`]**, i to jest cała treść tego wiersza: `live` jest
    /// PODMIENIANY przy każdym Starcie ([`AppState::begin_run`]), więc draft trzymający się
    /// tamtego uchwytu traci swój token w chwili, w której człowiek uruchomi bieg w innej
    /// karcie — a wtedy Stop na drafcie przestaje cokolwiek robić, bez ani jednego zdania.
    ///
    /// W środku siedzi `std::sync::Mutex` i **nigdy nie jest trzymany przez `await`**
    /// (niezmiennik 8): klon tokena bierze i oddaje jedno wyrażenie w
    /// [`commands::skills::Drafting`], przed czymkolwiek, co czeka. Zamek trzymany przez turę
    /// zawiesiłby Stop na czas pisania przez model — czyli dokładnie wtedy, kiedy Stop jest
    /// do czegokolwiek potrzebny.
    drafting: commands::skills::Drafting,
}

impl fmt::Debug for AppState {
    /// Ręcznie, bo [`Drivers`] jest domknięciem i `Debug` nie ma dla niego nic do powiedzenia —
    /// ten sam powód, co przy `RunDeps`.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AppState")
            .field("home", &self.home)
            .field("project", &self.project)
            .field("drivers", &"<factory>")
            .finish_non_exhaustive()
    }
}

impl AppState {
    /// Składa stan aplikacji z rzeczy, które umie zbudować wyłącznie powłoka okna.
    ///
    /// Uchwyt biegu zaczyna życie **już zeszły**. Bez tego Stop naciśnięty, zanim cokolwiek
    /// ruszyło, czekałby na dowód od biegu, którego nigdy nie było — a `stop_run_inner` mówi to
    /// wprost: uchwyt biegu, którego nikt nie uruchomił, nie ma czego dowieść. Przycisk
    /// wieszający okno jest gorszy od przycisku, który nic nie robi.
    #[must_use]
    pub fn new(home: PathBuf, project: PathBuf, store: Store, drivers: Drivers) -> Self {
        let idle = RunControl::new();
        idle.settle();
        Self {
            home,
            project,
            store,
            drivers,
            live: Mutex::new(idle),
            chat: tokio::sync::Mutex::new(None),
            drafting: commands::skills::Drafting::new(),
        }
    }

    /// Kończy rozmowę z orchestratorem, jeśli jakaś stoi.
    ///
    /// Wołane przy zamykaniu okna, obok zatrzymania biegu. Proces rozmowy jest procesem jak każdy
    /// inny: po śmierci Loadouta przeszedłby pod PID 1 i pracował dalej (`recovery.rs`, nagłówek) —
    /// czyli dokładnie ten defekt, który 2026-08-19 naprawiono dla biegów. Odzyskiwanie po nim nie
    /// posprząta, bo rozmowa nie ma wpisu w indeksie biegów.
    pub(crate) async fn close_chat(&self) {
        if let Some(chat) = self.chat.lock().await.as_mut() {
            chat.close().await;
        }
    }

    /// Sterownik, którym rozmawia orchestrator.
    ///
    /// `Vendor::ClaudeCode` na sztywno i to jest świadome: rozmowa nie jest krokiem workflow, więc
    /// nie ma definicji agenta, z której można by wziąć vendora. W dniu, w którym orchestrator
    /// stanie się konfigurowalny, ta funkcja zniknie na rzecz jego zapisanej definicji.
    ///
    /// 2026-08-20 — DEFINICJA JUŻ JEST, DRUTU DO NIEJ NIE MA. Odczyt zapisanej definicji stoi
    /// w [`commands::chat::Lead::pointed_at`] i to on ma tę funkcję skasować; brakuje jednej
    /// rzeczy, i jest nią wskazanie z okna. Powód, dlaczego nie da się go tu dowieźć, i jedyne
    /// pytanie do człowieka stoją przy [`AppState::chat`].
    fn chat_driver(&self) -> std::sync::Arc<dyn crate::engine::drivers::AgentDriver> {
        (self.drivers)(crate::library::agents::Vendor::ClaudeCode)
    }

    /// Współpracownicy biegu, który idzie teraz.
    ///
    /// Zamek zatruty odplatamy zamiast panikować: `panic!` w agentowym runtime zabiera cały
    /// bieg (AGENTS.md §4), a uchwyt po panice jednego kroku jest dalej poprawnym uchwytem.
    ///
    /// `pub(crate)` od 2026-08-19: obsługa zamknięcia okna stoi w `lib.rs`, poza tym modułem,
    /// a musi dosięgnąć tego samego żywego biegu, co komenda Stop — inaczej zatrzymywałaby
    /// **inny** uchwyt niż ten, który naprawdę prowadzi agentów.
    ///
    /// 2026-08-20 — `pub`, bo T-62 AC-2 pyta o UCHWYT, KTÓRY JEST ŻYWY, a nie o klon, który
    /// wołający sam trzyma w ręku. Drugie `/ask`, które po cichu podmieni [`AppState::live`],
    /// nie zmienia niczego w klonie, jaki test dostał przy pierwszym — więc jedynym miejscem,
    /// z którego widać osierocenie, jest ta metoda. Nie jest to druga odpowiedź na „który bieg
    /// jest żywy" (niezmiennik 13): to jest TA odpowiedź, tylko odczytana z zewnątrz.
    pub fn deps(&self) -> RunDeps<'_> {
        self.deps_in(self.project.as_path())
    }

    /// Współpracownicy biegu, który ma pracować w **tym** katalogu.
    ///
    /// 2026-08-18 — POWSTAŁO, BO FOLDER WYBRANY W OKNIE NIE DOJEŻDŻAŁ NIGDZIE. `AppState.project`
    /// ustala `lib.rs` raz, przy starcie okna, a `＋` na pasku kart zakładał kartę i kończył na
    /// `workspaces.open(...)` — bez ani jednego `invoke`. Człowiek wybierał `~/Projects/moj`,
    /// dostawał kartę z tą nazwą, a agent (gdyby wystartował) pracowałby w katalogu ustalonym
    /// przy starcie. „Agenci pracują w twoim folderze" jest CAŁĄ obietnicą tego produktu, więc
    /// katalog musi przyjechać z żądaniem, a nie ze stałej sprzed wyboru.
    fn deps_in<'a>(&'a self, project: &'a Path) -> RunDeps<'a> {
        RunDeps {
            home: self.home.as_path(),
            project,
            store: &self.store,
            drivers: Arc::clone(&self.drivers),
            // Zamek wzięty i oddany w JEDNYM wyrażeniu — między nim a jakimkolwiek `await`
            // wołającego nie ma ani jednej instrukcji (niezmiennik 8).
            control: self
                .live
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
        }
    }

    /// Świeży uchwyt biegu z pliku i współpracownicy, którzy go używają — albo zdanie o tym,
    /// dlaczego jeszcze go nie ma.
    ///
    /// Powód wymiany stoi przy [`AppState::live`]. Wymiana jest tutaj, a nie w skorupie, bo
    /// skorupa z tym `let` w środku byłaby o jedną decyzję dalej od „rozpakuj i zawołaj".
    ///
    /// # 2026-08-20 — `pub` I `Result`, BO TA DROGA JEST SĄDZONA OBOK DRUGIEJ
    ///
    /// `pub`, bo `tests/it/no_start_orphans_the_previous.rs` pyta o WSZYSTKIE pary dróg startu
    /// i nie ma jak zapytać o tę, jeśli jedyne wejście do niej stoi za `#[tauri::command]`.
    /// `Result`, bo obie drogi mają od tego kryterium jedną odpowiedź na jedno pytanie („czy
    /// coś już idzie"), a droga, która nie umie odmówić, nie ma czym tej odpowiedzi oddać.
    ///
    /// # 2026-08-20 (T-69) — TU MIESZKA CAŁA ODPOWIEDŹ NA „CZY COŚ JUŻ IDZIE"
    ///
    /// Do tego dnia ta metoda podmieniała uchwyt **bezwarunkowo**, a warunek stał tylko w drodze
    /// `/ask` ([`AppState::begin_a_run`]) — i była to naprawa jednej strony. Ścieżka awarii szła
    /// przez trzy warstwy okna do jednej linii tutaj: `/ask` startował agenta, człowiek naciskał
    /// Start, [`AppState::live`] zostawał nadpisany, Stop sięgał do biegu DRUGIEGO, a agent
    /// z `/ask` pracował dalej i dalej płacił. Dowodu śmierci grupy nie było komu zażądać, bo
    /// uchwyt, który jako jedyny o tamtym biegu wiedział, właśnie przestał istnieć (niezmienniki
    /// 6 i 11). Z okna nie było po tym ŻADNEJ drogi do tamtego biegu.
    ///
    /// Warunek stoi więc w JEDNYM ciele, a tamta metoda woła to ciało: dwie kopie tego pytania
    /// to dwie odpowiedzi na „czy coś już idzie" (niezmiennik 13), a przy dwóch kopiach zawsze
    /// poprawia się tę, której nikt nie woła. Zamknięte są przez to WSZYSTKIE pary dróg startu,
    /// nie jedna — Start → Start i Start → `/ask` dokładnie tak samo jak `/ask` → Start.
    ///
    /// Odmowa, nie kolejka, i to jest wybór z nazwaną ceną: czekanie w tej metodzie trzymałoby
    /// zamek na `live` przez cały poprzedni bieg, czyli zawieszałoby Stop dokładnie wtedy, kiedy
    /// Stop jest do czegokolwiek potrzebny. Zdanie mówi, co zrobić (DESIGN §8), więc człowiek
    /// ma następny ruch, a nie ciszę.
    ///
    /// Blokady „na zawsze" tu nie ma i nie może być: warunek pyta [`RunControl::is_working`],
    /// czyli „ruszył i jeszcze nie zszedł", więc bieg, który zszedł (`settle`), przestaje
    /// kogokolwiek zatrzymywać. Zapadka, która nigdy się nie otwiera, jest gorsza od wady, przed
    /// którą stoi.
    pub fn begin_run<'a>(&'a self, project: &'a Path) -> Result<RunDeps<'a>, String> {
        {
            // Zamek na CAŁE pytanie i na wymianę, nie na dwa osobne wyrażenia: „czy coś idzie"
            // sprawdzone przed wzięciem zamka jest odpowiedzią sprzed chwili, a między nią
            // a podmianą mieści się drugi start. Zamek `std::sync` i ani jednego `await`
            // w środku (niezmiennik 8) — powód stoi przy [`AppState::deps_in`].
            let mut live = self.live.lock().unwrap_or_else(PoisonError::into_inner);
            if live.is_working() {
                return Err(ALREADY_GOING.to_owned());
            }
            *live = RunControl::new();
        }
        Ok(self.deps_in(project))
    }

    /// Świeży uchwyt dla biegu z `/ask` — ta sama polityka, co przy starcie z płótna.
    ///
    /// # Dlaczego ta nazwa zostaje, choć ciało jest jedno
    ///
    /// Bo woła ją skorupa [`run_agent`], a `tests/it/no_start_orphans_the_previous.rs` pyta obie
    /// drogi Z NAZWY: macierz par dróg startu nie ma jak istnieć, jeśli obie drogi wchodzą jednym
    /// wejściem. Ciało jest jednak wspólne od 2026-08-20 (T-69) i mieszka w
    /// [`AppState::begin_run`] razem z całym powodem — do tego dnia warunek stał TYLKO tutaj,
    /// czyli `/ask` nie potrafił osierocić biegu, a Start potrafił.
    ///
    /// Dlaczego `/ask` nie ma i nie może mieć zapadki w oknie — `src/sections/run/io.ts`, akapit
    /// przy `ask`: jest to jedna linia w wierszu wejścia, najczęstsza czynność dnia, a nie drugie
    /// naciśnięcie tego samego przycisku.
    pub fn begin_a_run<'a>(&'a self, project: &'a Path) -> Result<RunDeps<'a>, String> {
        self.begin_run(project)
    }

    /// Katalog, w którym ma biec workflow: ten wybrany w oknie albo ten ze startu aplikacji.
    ///
    /// Ścieżka przychodzi z webviewa, więc jest wejściem, któremu nie ufamy (T3 §5.2) — ale
    /// inaczej niż nazwa pliku workflow, NIE jest sklejana z niczym po naszej stronie: to jest
    /// katalog, który człowiek wskazał systemowym oknem wyboru, i on ma prawo leżeć gdziekolwiek.
    /// Sprawdzamy więc nie „czy jest w bibliotece", a „czy to w ogóle jest folder" — bo pomyłka
    /// tutaj kończy się procesem agenta uruchomionym w katalogu, którego nie ma, i zdaniem
    /// o systemie plików zamiast o folderze.
    ///
    /// Każda odmowa mówi, co zrobić (DESIGN §8). Zdanie „os error 2" nie mówi nic.
    fn project_for(&self, folder: Option<&str>) -> Result<PathBuf, String> {
        // Brak wyboru jest wartością, nie błędem: dopóki nikt nie otworzył karty, biegniemy
        // tam, gdzie aplikacja wstała. Sam FOLDER sprawdza [`project_folder`] i to jest jedyne
        // miejsce, w którym te trzy zdania odmowy mieszkają.
        Ok(project_folder(folder)?.unwrap_or_else(|| self.project.clone()))
    }

    /// Nazwa pliku z okna → żądanie biegu.
    ///
    /// Zapora i jej cena stoją przy [`run_request`]; tutaj zostaje samo podanie biblioteki,
    /// bo katalog domowy jest jedyną rzeczą, którą stan do tej decyzji wnosi.
    fn request(
        &self,
        file_name: &str,
        how_many_at_once: usize,
        task: Option<String>,
    ) -> Result<RunRequest, String> {
        run_request(self.home.as_path(), file_name, how_many_at_once, task)
    }
}

/// Folder przysłany z okna → korzeń projektu, albo zdanie o tym, czego z nim nie da się zrobić.
///
/// JEDNA ODPOWIEDŹ NA PYTANIE „KTÓRY TO PROJEKT" (niezmiennik 13) — i to jest cały powód, dla
/// którego ta funkcja istnieje osobno. Bieg pyta o to przez [`AppState::project_for`], a od
/// 2026-08-19 pyta też instalacja umiejętności w zakresie „ten projekt": obie drogi dostają
/// ścieżkę z `activeWorkspace()` w oknie i obie muszą odmówić tym samym zdaniem. Druga kopia
/// tych trzech warunków znaczyłaby, że człowiek czyta o folderze co innego zależnie od tego,
/// czy nacisnął Run, czy „Add this skill".
///
/// `Ok(None)` znaczy „okno nic nie przysłało", czyli **nie ma otwartego zakresu** — i to jest
/// wartość, nie błąd. Co z nią zrobić, decyduje wołający: bieg bierze wtedy katalog, pod którym
/// wstała aplikacja, a instalacja projektowa odmawia zdaniem z rdzenia
/// (`skills::Error::NoProjectRoot`), bo zgadnięty korzeń zapisuje umiejętność w losowym miejscu.
///
/// WOLNA FUNKCJA, A NIE METODA [`AppState`], z tego samego powodu, co [`run_request`]: tamten
/// typ niesie [`Store`] i [`Drivers`], więc test tej zapory musiałby otworzyć bazę i zbudować
/// fabrykę sterowników, żeby sprawdzić trzy warunki na napisie. Zapora, której koszt sprawdzenia
/// jest wyższy niż koszt napisania, jest zaporą niesprawdzoną.
///
/// Każda odmowa mówi, co zrobić (DESIGN §8). Zdanie „os error 2" nie mówi nic.
pub fn project_folder(folder: Option<&str>) -> Result<Option<PathBuf>, String> {
    let Some(folder) = folder.map(str::trim).filter(|folder| !folder.is_empty()) else {
        return Ok(None);
    };
    let path = PathBuf::from(folder);
    if !path.is_absolute() {
        return Err(format!(
            "Loadout needs the whole path to the folder you want to work in, and \"{folder}\" \
             is only part of one. Open the folder again from the tab bar."
        ));
    }
    match fs::metadata(&path) {
        Ok(what) if what.is_dir() => Ok(Some(path)),
        Ok(_) => Err(format!(
            "\"{folder}\" is a file, not a folder. Pick the folder your project lives in."
        )),
        Err(_) => Err(format!(
            "The folder \"{folder}\" is not there any more, so nothing was started. Open it \
             again from the tab bar."
        )),
    }
}

/// Nazwa pliku z okna → żądanie biegu, liczone wyłącznie z katalogu biblioteki.
///
/// 2026-08-17 — ta zapora jest **drugą kopią** `commands::workflows::in_library`, i jest
/// tu świadomie, z nazwaną ceną. Nazwa przychodzi z webviewa, więc jest wejściem, któremu
/// nie ufamy (T3 §5.2): `Path::join("../../.ssh/config")` wychodzi poza bibliotekę bez
/// jednego ostrzeżenia, a `join("/etc/x")` odrzuca cały prefiks i zwraca `/etc/x` — czyli
/// Start uruchamiałby plik wskazany przez okno. Tamta funkcja jest prywatna,
/// a `src-tauri/src/commands/workflows.rs` nie należy do T-30, więc jedno `pub` w cudzym
/// pliku jest pytaniem do człowieka, nie cichym dopiskiem (AGENTS.md §7). Reguła
/// przepisana w adapterze jest tym, przed czym stoi niezmiennik 23 — dlatego stoi tu ten
/// akapit, a nie sama zapora: dopóki obie kopie żyją, zmiana jednej ma być czerwona
/// u człowieka, który to czyta.
///
/// 2026-08-17 — wolna funkcja, a nie ciało metody, z jednego powodu: [`AppState`] niesie
/// [`Store`] i [`Drivers`], więc test tej zapory przez metodę musiałby otworzyć bazę
/// i zbudować fabrykę sterowników, żeby sprawdzić `join` na napisie. Zapora, której koszt
/// sprawdzenia jest wyższy niż koszt napisania, jest zaporą niesprawdzoną — a ta jest jedyną
/// rzeczą między webviewem a `Command::new` w cudzym katalogu.
fn run_request(
    home: &Path,
    file_name: &str,
    how_many_at_once: usize,
    task: Option<String>,
) -> Result<RunRequest, String> {
    if Path::new(file_name)
        .file_name()
        .is_none_or(|name| name != file_name)
    {
        return Err(format!(
            "{file_name} is not the name of a file in the workflow folder"
        ));
    }
    Ok(RunRequest {
        workflow: home.join(WORKFLOWS_DIR).join(file_name),
        how_many_at_once,
        task,
    })
}

/// Startuje pompę tego biegu i oddaje nadajnik, którym bieg pisze linie.
///
/// Kanał przychodzi **argumentem komendy** i nie ma jak przyjść inaczej: `Channel<Vec<Line>>`
/// jest uchwytem do konkretnego webviewa, więc zakłada go okno i podaje przy `invoke`
/// (`docs/ARCHITECTURE.md` §3, §4). Rust nie ma z czego go zbudować sam.
///
/// `JoinHandle` pompy nie ginie po cichu. Bilans przyjętych i porzuconych jest kompletny
/// dopiero w chwili, w której pompa kończy się sama ([`PumpStats`]), a jedynym czytelnikiem,
/// jakiego dziś ma, jest dziennik — porzucona linia, o której nikt nigdy nie napisał ani słowa,
/// jest nie do odróżnienia od linii, której agent nie powiedział (niezmiennik 13).
fn pump_into(channel: Channel<Vec<Line>>) -> LineSink {
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, channel);
    // `drop` na `JoinHandle` **nie przerywa** zadania — odbiera tylko prawo czekania na nie.
    // Odczepiamy z rozmysłem: komenda biegu wraca, kiedy wróci bieg, a bilans przychodzi chwilę
    // później, bo pompa dowiaduje się o końcu producenta dopiero po jego zniknięciu.
    drop(tokio::spawn(async move {
        if let Ok(stats) = pump.await {
            tracing::info!(
                delivered = stats.delivered,
                dropped = stats.dropped,
                max_buffered = stats.max_buffered,
                "the pump for this run closed its books"
            );
        }
    }));
    sink
}

// ── SKORUPY KOMEND ─────────────────────────────────────────────────────────────────────────
//
// Każda z nich robi dwie rzeczy i ani jednej więcej: rozpakowuje to, co komenda ma dostać
// (katalog biblioteki, chwilę zegara), i woła funkcję `*_inner` z `commands/`. Logika napisana
// TUTAJ jest logiką, której nie da się przetestować bez Tauri — czyli dokładnie tym długiem,
// który to zadanie spłaca (niezmiennik 23).
//
// Nazwa komendy to nazwa funkcji, znak w znak z `src-tauri/commands.golden.txt`. Ten sam plik
// czytają OBA testy rejestracji: `ipc_commands_registered.rs` po tej stronie granicy
// i `src/sections/commands-wired.test.ts` po stronie okna.
//
// 2026-08-16 — WSZYSTKIE SĄ SYNCHRONICZNE i to jest wybór, nie przeoczenie. Tauri wykonuje
// komendę bez `async` na wątku głównym, więc `review_skill` zamraża okno na czas pobrania
// (do 20 s, `ingest::FETCH_TIMEOUT_SECONDS`). Lekarstwem jest `async fn` + `spawn_blocking`,
// czyli cztery wiersze logiki w skorupie — a mandat tego pliku brzmi „skorupy dwuliniowe",
// i jest to mandat, który broni jedynej rzeczy, jakiej to zadanie dowodzi. Zgłoszone
// człowiekowi zamiast rozstrzygnięte tutaj (AGENTS.md §7).

/// Wszyscy zapisani agenci.
#[tauri::command]
pub fn list_agents() -> Result<Vec<Agent>, String> {
    commands::agents::list_agents_inner(&crate::loadout_dir()).map_err(|error| error.to_string())
}

/// Świeży uuid v7 — jedna mennica dla wszystkich sekcji.
#[must_use]
#[tauri::command]
pub fn new_id() -> String {
    commands::mint::new_id_inner().to_string()
}

/// Zapisuje definicję agenta.
#[tauri::command]
pub fn save_agent(agent: Agent) -> Result<(), String> {
    commands::agents::save_agent_inner(&crate::loadout_dir(), agent)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Usuwa agenta po identyfikatorze, razem z jego plikiem.
#[tauri::command]
pub fn delete_agent(id: &str) -> Result<(), String> {
    commands::agents::delete_agent_inner(&crate::loadout_dir(), id)
        .map_err(|error| error.to_string())
}

/// Wszystko, co leży w katalogu workflow, każdy plik ze swoją nazwą.
#[tauri::command]
pub fn list_workflows() -> Result<Vec<commands::workflows::WorkflowEntry>, String> {
    commands::workflows::list_workflows_inner(&crate::loadout_dir())
        .map_err(|error| error.to_string())
}

/// Wczytuje jeden plik workflow po jego nazwie w katalogu.
#[tauri::command]
pub fn load_workflow(file_name: &str) -> Result<WorkflowFile, String> {
    commands::workflows::load_workflow_inner(&crate::loadout_dir(), file_name)
        .map_err(|error| error.to_string())
}

/// Zapisuje plik workflow. Odmowa walidatora przyjeżdża jego własnym zdaniem.
#[tauri::command]
pub fn save_workflow(file_name: &str, workflow: WorkflowFile) -> Result<(), String> {
    commands::workflows::save_workflow_inner(&crate::loadout_dir(), file_name, workflow)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Usuwa plik workflow z katalogu.
#[tauri::command]
pub fn delete_workflow(file_name: &str) -> Result<(), String> {
    commands::workflows::delete_workflow_inner(&crate::loadout_dir(), file_name)
        .map_err(|error| error.to_string())
}

/// Uwagi walidatora o tym workflow — te same, które padają przy zapisie i przed Startem.
#[must_use]
#[tauri::command]
pub fn check_workflow(workflow: WorkflowFile) -> Vec<Note> {
    commands::workflows::check_workflow_inner(workflow)
}

/// Adres → pobrana i przejrzana umiejętność.
#[tauri::command]
pub fn review_skill(url: &str) -> Result<commands::skills::ImportWire, String> {
    commands::skills::review_skill_inner(&crate::loadout_dir(), url)
        .map_err(|error| error.to_string())
}

/// Trzy pytania z formularza → umiejętność przejrzana tym samym rdzeniem, co wklejony link.
#[tauri::command]
pub fn author_skill(
    authored: commands::skills::Authored,
) -> Result<commands::skills::ImportWire, String> {
    commands::skills::author_skill_inner(&crate::loadout_dir(), authored)
        .map_err(|error| error.to_string())
}

/// Zapisuje przejrzaną umiejętność w katalogach vendorów wybranego zakresu.
#[tauri::command]
pub fn install_skill(
    item: commands::skills::ImportWire,
    landing: commands::skills::Landing,
    folder: Option<&str>,
) -> Result<(), String> {
    // Rozbiór, a nie `item.name`: z całego przeglądu Rust bierze WYŁĄCZNIE nazwę, bo bajty do
    // zapisania czyta z kopii kanonicznej — z tych samych, które przeskanował i pokazał
    // człowiekowi. Ten jeden wiersz mówi to wprost i nie da się go przeczytać inaczej.
    let commands::skills::ImportWire { name, .. } = item;
    install_reviewed_skill(&crate::loadout_dir(), &name, landing, folder).map(|_| ())
}

/// Ciało [`install_skill`] z biblioteką podaną **argumentem**.
///
/// WOLNA FUNKCJA Z TEGO SAMEGO POWODU, CO [`run_request`] i [`project_folder`]: skorupa liczy
/// bibliotekę przez `crate::loadout_dir()`, czyli z prawdziwego `HOME`, więc test tej drogi
/// pisałby do katalogów vendorów człowieka, który go uruchomił. A sprawdzić trzeba właśnie ją:
/// to jest jedyne miejsce, w którym zakres z okna spotyka się z korzeniem projektu, i jedyne,
/// w którym da się pomylić „nie ma otwartego projektu" z „zapisz gdziekolwiek".
///
/// Oddaje ścieżki z planu, bo to jest jedyna odpowiedź na pytanie „co się właśnie stało";
/// skorupa wyżej zwija je do `()`, bo okno pyta tylko o to, czy się udało.
pub fn install_reviewed_skill(
    library: &Path,
    name: &str,
    landing: commands::skills::Landing,
    folder: Option<&str>,
) -> Result<Vec<PathBuf>, String> {
    // FOLDER SĄDZI [`project_folder`], I TO JEST CAŁA TREŚĆ TEJ LINII. Nic poniżej tej warstwy
    // nie pyta, czy ścieżka jest bezwzględna i czy to w ogóle katalog — `place::plan` wierzy
    // korzeniowi, który dostał. Bez tego wywołania ścieżka względna z okna dojechałaby do
    // rozmieszczania i umiejętność wylądowałaby tam, gdzie ją postawi `Path::join`.
    //
    // `?` oddaje zdanie tej funkcji SŁOWO W SŁOWO, bo to samo zdanie czyta człowiek, który
    // nacisnął Run (`AppState::project_for` jest nad nią jedną linią). Własne brzmienie tutaj
    // byłoby drugą odpowiedzią na „który to projekt" (niezmiennik 13).
    //
    // Sprawdzamy przy KAŻDYM zakresie, nie tylko przy „ten projekt": folder, którego nie ma,
    // jest tą samą pomyłką niezależnie od tego, gdzie ma wylądować plik, a odmowa zależna od
    // pozycji wyboru każe człowiekowi zgadywać, czy jego zakres jest w porządku.
    let project = project_folder(folder)?;
    commands::skills::install_skill_into(library, name, landing, project.as_deref())
        .map_err(|error| error.to_string())
}

/// Co naprawdę leży w katalogach agentów — lista, którą sekcja Umiejętności czyta przy wejściu.
///
/// 2026-08-18 — bez tej komendy licznik „N saved" pokazywał wyłącznie to, co dodano w TEJ
/// sesji: `install_skill` pisało na dysk, a okno nie miało jak tego odczytać z powrotem, więc
/// zainstalowana umiejętność znikała po restarcie. To był niezmiennik 4 złamany wprost —
/// pliki są prawdą, a ekran mówił co innego.
///
/// 2026-08-19 — FOLDER, BO LISTA ODPOWIADA NA PYTANIE „CO WIDZI AGENT PRACUJĄCY TUTAJ". Bez niego
/// umiejętność zapisana „w tym projekcie" nie pojawiłaby się na ekranie, więc człowiek nie miałby
/// jak jej zabrać — droga zapisu bez drogi odczytu jest gorsza niż brak funkcji.
#[tauri::command]
pub fn list_skills(folder: Option<&str>) -> Result<Vec<commands::skills::InstalledWire>, String> {
    // Ten sam sąd nad folderem, co przy zapisie i przy Starcie biegu (`project_folder`).
    // Lista czytana z folderu, którego nie ma, jest pustą listą — czyli zdaniem „nic tam nie
    // leży" o katalogu, o który nikt nie umiał zapytać.
    let project = project_folder(folder)?;
    commands::skills::list_skills_in(&crate::loadout_dir(), project.as_deref())
        .map_err(|error| error.to_string())
}

/// Zdejmuje umiejętność z katalogów agentów.
///
/// 2026-08-18 — bez tej komendy sekcja Umiejętności umiała tylko dokładać. Lista z
/// [`list_skills`] czyta pliki, więc wiersz na ekranie odpowiada dyskowi — a jedyne, co
/// człowiek mógł z nim zrobić, to zainstalować go jeszcze raz (niezmiennik 16).
///
/// 2026-08-19 — ZAKRES I FOLDER, bo ta sama nazwa w dwóch zakresach to dwie rzeczy: zdjęcie
/// „z tego projektu" ma zostawić kopię globalną tam, gdzie jest.
#[tauri::command]
pub fn delete_skill(
    name: &str,
    landing: commands::skills::Landing,
    folder: Option<&str>,
) -> Result<(), String> {
    // Ten sam sąd nad folderem, co przy zapisie: zdjęcie „z tego projektu" z folderu, którego
    // nie ma, jest odmową o folderze, a nie o umiejętności — i to jest zdanie, po którym
    // człowiek wie, co zrobić.
    let project = project_folder(folder)?;
    commands::skills::delete_skill_from(&crate::loadout_dir(), name, landing, project.as_deref())
        .map_err(|error| error.to_string())
}

// ── DWIE KOMENDY DRAFTU ────────────────────────────────────────────────────────────────────
//
// 2026-08-19 — te dwie łamią OBIE reguły sąsiadów wyżej i każda z nich ma na to własny powód.
//
// PIERWSZA: `draft_skill` jest `async`, choć akapit nad `list_agents` mówi, że wszystkie
// skorupy umiejętności są synchroniczne. Tam ten dług jest zapisany jawnie i tutaj się nie
// domyka: Tauri wykonuje komendę bez `async` na wątku głównym, a ta czeka na model —
// dziesiątki sekund, nie 20 ms. Synchroniczna zamroziłaby okno na cały czas pisania, czyli
// zamieniłaby jedyną nową drogę tej sekcji w zawieszoną aplikację.
//
// DRUGA: obie mają `State`, choć czternaście skorup wyżej bierze katalog z
// `crate::loadout_dir()` i niczego nie pamięta. Powód jest ten sam, co przy trzech komendach
// biegu: Stop musi sięgnąć do środka draftu, który zaczęła INNA komenda, więc uchwyt do niego
// musi gdzieś mieszkać między wywołaniami.
//
// `stop_draft` jest `async` mimo tego, że nie ma na co czekać — cofa jedno wyrażenie na tokenie
// i wraca. Zmierzone 2026-08-19, i to jest poprawka do zdania, które stało tu wcześniej:
// wersja synchroniczna NIE PRZECHODZI bramki. `clippy::needless_pass_by_value` (pedantic, a bramka
// woła clippy z `-D warnings`) melduje „this argument is passed by value, but not consumed":
// `State` przyjeżdża wartością, bo taka jest konwencja wywołania Tauri, a ciało go tylko pożycza.
// Sugestia clippy — `&State<'_, AppState>` — jest tu gorsza niż lint: `windowSideArguments`
// w `src/sections/ipc-signature.ts` rozpoznaje wstrzykiwany argument po wzorcu `: State<`, więc
// referencja zamieniłaby `state` w klucz, którego okno ma niby wysłać, i przewróciła kryterium
// szwu po tamtej stronie granicy.
//
// Odwrotnego lintu, którego obawiał się poprzedni akapit, nie ma: `clippy::unused_async` nie
// świeci na skorupie komendy — precedens stoi dwa akapity niżej w tym samym pliku
// (`list_handoffs` jest `async` i nie ma w ciele ani jednego `await`), a `generate_handler!`
// bierze te funkcje jako wartości. Async ma zresztą własny, samodzielny powód: żadna komenda
// dotykająca zamka draftu nie biegnie wtedy na wątku okna.
//
// Dowód zejścia grupy (niezmiennik 6) nie wraca tędy i nie ma tędy wracać: niesie go odpowiedź
// `draft_skill`, czyli to samo wywołanie, na które okno już czeka.

/// Jedno zdanie człowieka → trzy pola napisane przez agenta, którego wybrał.
///
/// `None` znaczy „człowiek to zatrzymał" i jest **wartością**, nie odmową (niezmiennik 7):
/// okno ma po niej wygasić stan „pisze" i nie pokazywać ani draftu, ani zdania o awarii.
/// Odmowa jedzie zwykłą drogą odmowy i niesie zdanie rdzenia.
///
/// Zapisu tu nie ma. Trzy pola lądują w formularzu z T-42 i dopiero `author_skill` składa
/// z nich plik, skanuje go i odkłada kopię kanoniczną — więc tekst poprawiony po drafcie
/// przechodzi przez skan tak samo jak wpisany ręką (niezmiennik 23).
#[tauri::command]
pub async fn draft_skill(
    state: State<'_, AppState>,
    want: &str,
    agent: &str,
) -> Result<Option<commands::skills::Authored>, String> {
    commands::skills::draft_skill_inner(
        &crate::loadout_dir(),
        &state.drivers,
        &state.drafting,
        want,
        agent,
    )
    .await
    .map(|outcome| match outcome {
        commands::skills::DraftOutcome::Wrote(authored) => Some(authored),
        commands::skills::DraftOutcome::Cancelled => None,
    })
    .map_err(|error| {
        let said = error.to_string();
        refused(&said);
        said
    })
}

/// „Stop" dla draftu: zatrzymuje agenta, który pisze umiejętność.
///
/// Osobna komenda od [`stop_run`], bo zatrzymuje osobny uchwyt. Jedna komenda na oba
/// znaczyłaby, że Stop w sekcji Umiejętności ubija bieg w sąsiedniej karcie.
#[tauri::command]
pub async fn stop_draft(state: State<'_, AppState>) -> Result<(), String> {
    state.drafting.stop();
    Ok(())
}

/// Co jeden krok oddał następnemu — wszystkie przekazania biegów tego projektu.
///
/// 2026-08-18 — przekazania są JEDYNĄ drogą, którą wynik kroku dochodzi do promptu następnego
/// (`docs/ARCHITECTURE.md` §8), a do tego dnia okno nie miało jak o nie zapytać: pliki
/// powstawały, `memory::handoff` umiało je przeczytać, i człowiek nie widział z tego ani jednej
/// litery.
///
/// Katalog projektu bierzemy ze stanu, nie z `crate::loadout_dir()`: biegi leżą pod
/// `<projekt>/.loadout/runs/`, a nie w bibliotece. Dlatego ta jedna komenda odczytu ma `State`,
/// choć nie dotyka żywego biegu.
#[tauri::command]
pub async fn list_handoffs(
    state: State<'_, AppState>,
) -> Result<Vec<commands::handoffs::HandoffWire>, String> {
    commands::handoffs::list_handoffs_inner(state.project.as_path())
        .map_err(|error| error.to_string())
}

/// Wszystkie notatki leżące na dysku — lista, którą sekcja Pamięć czyta przy wejściu.
///
/// 2026-08-18 — powstało z tego samego powodu, co [`list_skills`]: magazyn notatek startował
/// pusty i nic w produkcji nie umiało go wypełnić, więc `put_note_to_use` przestawiało status
/// notatki, której sekcja nigdy nie pokazała.
#[tauri::command]
pub fn list_notes() -> Result<Vec<commands::memory::NoteWire>, String> {
    let root = commands::memory::notes_root(&crate::loadout_dir());
    commands::memory::list_notes_inner(&root).map_err(|error| error.to_string())
}

/// „Use this": od tej chwili notatka wchodzi do promptu.
#[tauri::command]
pub fn put_note_to_use(
    id: &str,
) -> Result<commands::memory::NoteWire, commands::memory::NoteRefusal> {
    let root = commands::memory::notes_root(&crate::loadout_dir());
    commands::memory::put_note_to_use_inner(&root, id, &commands::now_utc())
}

/// „Stop using": notatka zostaje na liście i przestaje wchodzić do promptu.
#[tauri::command]
pub fn stop_using_note(
    id: &str,
) -> Result<commands::memory::NoteWire, commands::memory::NoteRefusal> {
    let root = commands::memory::notes_root(&crate::loadout_dir());
    commands::memory::stop_using_note_inner(&root, id, &commands::now_utc())
}

/// Workspace'y: nazwane zakresy pracy. Lista, dokładanie, zdejmowanie.
///
/// 2026-08-18 — TRZY KOMENDY, KTÓRE ZASTĄPIŁY SYSTEMOWE OKNO PRZY KAŻDYM BIEGU. Folder pracy
/// wybierało się do tego dnia okienkiem wyboru katalogu, otwieranym przed uruchomieniem
/// workflow, jeśli żadna karta nie była otwarta. To jest decyzja o PROJEKCIE, podejmowana raz —
/// nie czynność powtarzana przed każdą pracą. Powód i zakres w całości stoją
/// w `commands::workspaces`.
///
/// Katalog bierzemy z `crate::loadout_dir()`, nie ze stanu: lista workspace'ów jest biblioteką
/// użytkownika, a nie stanem żywego biegu — czyli dokładnie tak samo jak agenci, workflow
/// i notatki, i z tego samego powodu ta komenda nie ma `State`.
#[tauri::command]
pub fn list_workspaces() -> Result<Vec<commands::workspaces::WorkspaceWire>, String> {
    commands::workspaces::list_workspaces_inner(&crate::loadout_dir())
        .map_err(|error| error.to_string())
}

/// Dokłada workspace albo zmienia nazwę istniejącego. Oddaje CAŁĄ listę po zapisie.
#[tauri::command]
pub fn save_workspace(
    name: &str,
    folder: &str,
) -> Result<Vec<commands::workspaces::WorkspaceWire>, String> {
    commands::workspaces::save_workspace_inner(&crate::loadout_dir(), name, folder)
        .map_err(|error| error.to_string())
}

/// Zdejmuje workspace z listy. **Folderu nie dotyka** — powód przy `delete_workspace_inner`.
#[tauri::command]
pub fn delete_workspace(id: &str) -> Result<Vec<commands::workspaces::WorkspaceWire>, String> {
    commands::workspaces::delete_workspace_inner(&crate::loadout_dir(), id)
        .map_err(|error| error.to_string())
}

// ── TRZY KOMENDY BIEGU ─────────────────────────────────────────────────────────────────────
//
// Te trzy wyglądają inaczej niż czternaście wyżej i różnica jest jedna: biegu nie da się
// obsłużyć bez stanu. Stop musi sięgnąć do środka biegu, który zaczęła INNA komenda, więc
// uchwyt do niego musi gdzieś mieszkać między wywołaniami — i to jest cały powód, dla którego
// [`AppState`] w ogóle istnieje. Reszta pliku bierze katalog z `crate::loadout_dir()` i niczego
// nie pamięta.
//
// 2026-08-17 — WSZYSTKIE TRZY ODDAJĄ `()` I TO JEST DŁUG, NIE WYBÓR. `RunReport` niesie
// identyfikator biegu i jego katalog, ale nie jest `Serialize`, a `src-tauri/src/commands/mod.rs`
// nie należy do T-30 — jedno `#[derive(Serialize)]` w cudzym pliku jest pytaniem do człowieka
// (AGENTS.md §7). Do tego czasu okno dowiaduje się o wyniku biegu z indeksu, który powstaje
// z katalogu biegu (niezmiennik 4), a nie z odpowiedzi tej komendy.

/// Start: uruchamia workflow z biblioteki i oddaje jego linie oknu.
///
/// `task` to zdanie z wiersza wejścia — co ten bieg ma zbudować. `None` znaczy „tylko to, co
/// stoi w pliku"; powód i sposób wpisania go w prompt kroku stoją przy [`RunRequest::task`]
/// i [`commands::run`].
#[tauri::command]
pub async fn run_workflow(
    state: State<'_, AppState>,
    file_name: &str,
    how_many_at_once: usize,
    folder: Option<String>,
    task: Option<String>,
    lines: Channel<Vec<Line>>,
) -> Result<(), String> {
    let request = state
        .request(file_name, how_many_at_once, task)
        .inspect_err(refused)?;
    let project = state.project_for(folder.as_deref()).inspect_err(refused)?;
    commands::run::run_workflow_inner(
        // `?` NA MIEJSCU, A NIE W OSOBNYM `let`, i to nie jest gust: `run_commands_registered`
        // liczy instrukcje tej skorupy z sufitem 3 („rozpakuj, zawołaj, wróć"), a czwarta
        // instrukcja jest logiką napisaną tam, gdzie żaden test jednostkowy jej nie dosięgnie
        // (niezmiennik 23). Od T-69 ta droga umie odmówić tak samo jak `/ask`: uchwyt żywego
        // biegu nie ma prawa zniknąć pod Stopem (powód w całości przy [`AppState::begin_run`]).
        &state.begin_run(project.as_path()).inspect_err(refused)?,
        &request,
        pump_into(lines),
    )
    .await
    .map(|_| ())
    .map_err(|error| {
        let said = error.to_string();
        refused(&said);
        said
    })
}

/// `/ask`: uruchamia JEDNEGO agenta z jednym zdaniem — i jest to zwykły bieg.
///
/// Jednostką pracy było do dziś PLIK: żeby puścić jednego agenta z jednym zdaniem, człowiek
/// musiał wejść do edytora, założyć workflow, postawić jeden kafelek, zapisać go i wrócić.
/// Ta komenda jest skrótem do TEJ SAMEJ maszynerii, nie drugą maszynerią obok: katalog biegu,
/// `run.json`, miejsce we wspólnej puli i dowód śmierci grupy przychodzą z
/// [`commands::run::run_agent_inner`], dokładnie jak przy [`run_workflow`].
///
/// `how_many_at_once` jedzie argumentem, nie stałą `1`, i to jest cała obrona niezmiennika 11:
/// bieg jednokrokowy bierze miejsce z puli całej aplikacji, więc dwa `/ask` przy suwaku na
/// trzech to najwyżej trzech pracujących agentów, a nie piątka.
#[tauri::command]
pub async fn run_agent(
    state: State<'_, AppState>,
    agent: &str,
    task: &str,
    how_many_at_once: usize,
    folder: Option<String>,
    lines: Channel<Vec<Line>>,
) -> Result<(), String> {
    let ask = commands::run::AskRequest {
        agent: agent.to_owned(),
        task: task.to_owned(),
        how_many_at_once,
    };
    let project = state.project_for(folder.as_deref()).inspect_err(refused)?;
    // `begin_a_run`, nie `begin_run`: uchwyt żywego biegu nie ma prawa zniknąć pod Stopem
    // (powód w całości stoi przy tamtej metodzie).
    let deps = state.begin_a_run(project.as_path()).inspect_err(refused)?;
    commands::run::run_agent_inner(&deps, &ask, pump_into(lines))
        .await
        .map(|_| ())
        .map_err(|error| {
            let said = error.to_string();
            refused(&said);
            said
        })
}

/// Odmowa **do dziennika**, zanim pojedzie przez granicę.
///
/// 2026-08-18 — PO CO TO ISTNIEJE. Skorupy robiły `.map_err(|e| e.to_string())` i nie logowały
/// niczego, a po drugiej stronie granicy Tauri odrzuca surowym napisem. Dziennik właściciela
/// miał siedemnaście nieudanych startów i **ani jednej linii o powodzie** — czyli jedyne
/// trwałe miejsce, w którym dałoby się dowiedzieć, dlaczego bieg nie ruszył, milczało.
/// Zdanie idzie w `warn`, nie `error`: odmowa jest normalnym zakończeniem żądania, które
/// człowiek zaraz naprawi, a nie awarią aplikacji.
fn refused(said: &String) {
    tracing::warn!(%said, "Loadout turned down a run");
}

/// Stop: zatrzymuje bieg i wraca **dopiero z dowodem**, że nic po nim nie żyje (niezmiennik 6).
///
/// [`crate::commands::Outcome`] przepada tutaj i nic się z nim nie traci: `stop_run_inner` ma
/// jedną odpowiedź — `Cancelled` — bo bieg z anulowanym tokenem melduje anulowanie także wtedy,
/// gdy ostatni krok zdążył się udać.
#[tauri::command]
pub async fn stop_run(state: State<'_, AppState>) -> Result<(), String> {
    commands::run::stop_run_inner(&state.deps())
        .await
        .map(|_| ())
        .map_err(|error| {
            let said = error.to_string();
            refused(&said);
            said
        })
}

/// Otwiera strumień rozmowy z orchestratorem — sam proces jeszcze nie wstaje.
///
/// Dwie komendy, nie jedna, i podział ma nazwany powód: kanał do okna umie zbudować **tylko okno**
/// (`docs/ARCHITECTURE.md` §3), więc musi przyjść argumentem — a sesja u dostawcy ma wstać dopiero
/// przy pierwszym zdaniu, bo tura wystartowana przy montażu ekranu jest turą, za którą ktoś płaci,
/// choć nikt o nic nie zapytał. Ta komenda zakłada więc pompę i nic więcej.
///
/// Wołana ponownie PRZEKIEROWUJE istniejącą rozmowę na nowy kanał — nie kończy jej.
///
/// 2026-08-19 — WERSJA PIERWSZA ZAMYKAŁA ROZMOWĘ I BYŁO TO WIDAĆ W DZIENNIKU przy pierwszym
/// uruchomieniu: „the pump for this run closed its books delivered=0", trzy razy pod rząd. Tę
/// komendę woła KAŻDY montaż ekranu pracy i każde przeładowanie okna, więc wyjście na Agentów
/// i powrót gubiłoby cały wątek — czyli dokładnie to, po co ta rozmowa istnieje („sobie
/// piszemy/zmieniamy coś itp"). Sesja u dostawcy nie ma powodu o tym wiedzieć: zmienia się tylko
/// to, komu jej wiersze są pokazywane ([`commands::chat::Chat::lines_go_to`]).
#[tauri::command]
pub async fn open_chat(
    state: State<'_, AppState>,
    lines: Channel<Vec<Line>>,
) -> Result<(), String> {
    let mut chat = state.chat.lock().await;
    match chat.as_ref() {
        Some(open) => open.lines_go_to(pump_into(lines)),
        None => *chat = Some(commands::chat::Chat::new(pump_into(lines))),
    }
    Ok(())
}

/// Mówi zdanie orchestratorowi. **Nie uruchamia biegu i nie ma jak** — powód przy `commands::chat`.
#[tauri::command]
pub async fn say_to_orchestrator(
    state: State<'_, AppState>,
    folder: Option<String>,
    text: &str,
) -> Result<(), String> {
    let cwd = state.project_for(folder.as_deref()).inspect_err(refused)?;
    let driver = state.chat_driver();
    let mut chat = state.chat.lock().await;
    let open = chat.as_mut().ok_or_else(|| {
        let said =
            "The lead agent is not ready yet. Reopen the work screen and try again.".to_owned();
        refused(&said);
        said
    })?;
    open.say(driver.as_ref(), cwd, text).await.map_err(|error| {
        let said = error.to_string();
        refused(&said);
        said
    })
}

/// „Dalej": puszcza bieg zza punktu kontrolnego.
#[tauri::command]
pub async fn continue_run(
    state: State<'_, AppState>,
    answer: Option<String>,
) -> Result<(), String> {
    commands::run::continue_run_inner(&state.deps(), answer)
        .await
        .map_err(|error| {
            let said = error.to_string();
            refused(&said);
            said
        })
}

/// „Powiedz coś agentowi, który pracuje" — kolejna tura w jego żywej sesji.
///
/// Cała polityka — wybór adresata i pięć różnych odmów — stoi w
/// [`commands::run::say_to_agent_inner`], razem z powodem, dla którego stoi tam, a nie tutaj.
/// Ta skorupa robi dwie rzeczy, które umie zrobić tylko ona: sięga po zależności biegu
/// z `State` i zamienia odmowę w napis dla okna.
#[tauri::command]
pub async fn say_to_agent(
    state: State<'_, AppState>,
    agent: Option<String>,
    text: &str,
) -> Result<(), String> {
    commands::run::say_to_agent_inner(&state.deps().control, agent.as_deref(), text)
        .await
        .map_err(|error| {
            let said = error.to_string();
            refused(&said);
            said
        })
}

/// Jedyna lista komend, którą dostaje okno.
///
/// Jedna, bo builder pamięta **ostatnią**, którą mu podano: druga podmieniłaby pierwszą po
/// cichu i połowa komend zniknęłaby zza zielonego kryterium. Zbiór nazw tutaj równa się co do
/// sztuki zbiorowi z `src-tauri/commands.golden.txt` i pilnuje tego
/// `src-tauri/tests/ipc_commands_registered.rs` — zawieranie w jedną stronę nie odróżniłoby
/// komendy, o której front nie wie, od komendy, na którą `invoke` nigdy nie trafia.
///
/// Funkcja, a nie makro wołane w `lib.rs`: słowo „tauri" ma prawo paść wyłącznie w tym pliku
/// (`docs/ARCHITECTURE.md` §3, niezmiennik 1).
pub fn command_handler() -> impl Fn(Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        author_skill,
        check_workflow,
        continue_run,
        delete_agent,
        delete_skill,
        delete_workflow,
        delete_workspace,
        draft_skill,
        install_skill,
        list_agents,
        list_handoffs,
        list_notes,
        list_skills,
        list_workflows,
        list_workspaces,
        load_workflow,
        new_id,
        open_chat,
        put_note_to_use,
        review_skill,
        run_agent,
        run_workflow,
        save_agent,
        save_workflow,
        save_workspace,
        say_to_agent,
        say_to_orchestrator,
        stop_draft,
        stop_run,
        stop_using_note
    ]
}

/// Zapora [`run_request`] pod obstrzałem — jedyna rzecz w tym pliku, którą da się sprawdzić
/// bez okna, i jedyna, której żadne kryterium T-30 nie dotyka.
///
/// 2026-08-17 — powstało dlatego, że druga opinia zmierzyła to wprost: AC-1 chodzi po pompie,
/// AC-2 po liście komend, AC-4 po stronie okna, a `run_request` nie jest wołane w żadnym
/// z nich. Zapora bez testu jest zaporą do chwili pierwszego refaktoru — a ta stoi między
/// napisem z webviewa a plikiem, który bieg naprawdę odpali.
///
/// Trzy przypadki, bo dwa nie wystarczą. Sama odmowa przechodzi na zaporze, która odrzuca
/// **wszystko** — a taka jest nie do odróżnienia od zepsutego Startu dopóki ktoś nie kliknie.
/// Dlatego trzeci przypadek jest dodatni i porównuje **całą** ścieżkę, nie sam fakt `Ok`:
/// zapora przepuszczająca nazwę i gubiąca katalog biblioteki jest tym samym błędem, tylko
/// w drugą stronę.
#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{WORKFLOWS_DIR, run_request};

    /// Katalog biblioteki. Nie istnieje na dysku i istnieć nie musi: `run_request` decyduje
    /// o napisie, nie o pliku — sprawdzanie obecności pliku należy do biegu, który go wczyta.
    const HOME: &str = "/loadout-home";

    /// To, co z żądania da się porównać: gdzie bieg pójdzie i ile ma robić naraz.
    ///
    /// [`super::RunRequest`] nie jest `PartialEq`, a jego plik nie należy do T-30 — więc
    /// zamiast `derive` w cudzym pliku stoi tu rozbiór na dwa pola, które ta zapora ustawia.
    fn requested(file_name: &str, how_many_at_once: usize) -> Result<(PathBuf, usize), String> {
        // Bez zadania: ta zapora sądzi NAZWĘ PLIKU, a zadanie z wiersza nie ma na nią wpływu.
        run_request(Path::new(HOME), file_name, how_many_at_once, None)
            .map(|request| (request.workflow, request.how_many_at_once))
    }

    /// `..` w nazwie wychodzi poza bibliotekę i `Path::join` nie powie o tym ani słowa.
    #[test]
    fn a_name_that_climbs_out_of_the_library_is_refused() {
        assert!(
            requested("../../.ssh/config", 1).is_err(),
            "a name carrying `..` reaches outside the workflow folder and must be refused"
        );
    }

    /// Ścieżka bezwzględna jest gorsza od `..`: `join` **odrzuca cały prefiks**, więc bez
    /// zapory bieg odpaliłby dokładnie ten plik, który wskazało okno.
    #[test]
    fn an_absolute_path_is_refused() {
        assert!(
            requested("/etc/x", 1).is_err(),
            "an absolute name replaces the library prefix entirely and must be refused"
        );
    }

    /// Kontrola dodatnia: zwykła nazwa przechodzi i ląduje **w** bibliotece.
    ///
    /// Bez tego przypadku obie odmowy wyżej świecą na zielono także dla zapory, która nie
    /// przepuszcza niczego — czyli dla Startu, który nigdy nic nie uruchamia.
    #[test]
    fn a_plain_file_name_passes_and_stays_inside_the_library() {
        assert_eq!(
            requested("nightly-review.yaml", 3),
            Ok((
                Path::new(HOME)
                    .join(WORKFLOWS_DIR)
                    .join("nightly-review.yaml"),
                3
            ))
        );
    }
}
