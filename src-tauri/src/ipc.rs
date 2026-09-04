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
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::{Pin, pin};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll, Waker};

use base64::Engine as _;
use tauri::State;
use tauri::ipc::{Channel, Invoke};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant, MissedTickBehavior, interval_at};

use crate::commands::{self, Drivers, RunControl, RunDeps, RunRequest};
use crate::engine::drivers::{ImageInput, ValidatedImages};
use crate::engine::limits::Limiter;
use crate::engine::line::Line;
use crate::inherit::Lendable;
use crate::library::agents::Agent;
use crate::library::definition::Definition;
use crate::store::Store;
use crate::workflow::WorkflowFile;
use crate::workflow::check::Note;

/// Okno sklejania. 16 ms to jedna klatka przy 60 Hz: dłużej widać jako opóźnienie, krócej
/// nie kupuje już nic, bo koszt siedzi w liczbie wiadomości [T8 §5.3].
pub const FLUSH: Duration = Duration::from_millis(16);

/// Sufit jednej paczki. Liczba z pomiaru, nie z gustu: przy 2000 najgorsza przerwa klatki
/// wynosi 0–1 ms, przy `batch200` i `batch1000` sięga 13–25 ms
/// (pomiar IPC na tej maszynie).
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
                    // i przy 1000 sięga 13-25 ms (pomiar IPC na tej maszynie).
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

/// Co powiedzieć KAŻDEMU drugiemu startowi, kiedy bieg w TYM folderze jeszcze nie zszedł.
///
/// Jedno zdanie na obie drogi (`/ask` i bieg z pliku), bo pytanie jest jedno: „czy coś już
/// idzie tutaj". Osobne brzmienie per komenda znaczyłoby, że człowiek czyta o tej samej odmowie
/// co innego zależnie od tego, którym przyciskiem ją wywołał (niezmiennik 13).
///
/// ZDANIE NAZYWA NASTĘPNY RUCH (DESIGN §8), bo odmowa bez wyjścia zostawia człowieka dokładnie
/// tam, gdzie był. Mówi też DLACZEGO: bez powodu czyta się to jak ograniczenie na złość, a
/// prawdziwy powód jest finansowy — Loadout prowadzi jeden bieg naraz W FOLDERZE, więc drugi
/// uchwyt znaczyłby, że Stop sięga do biegu drugiego, a pierwszy pracuje dalej i dalej płaci
/// (niezmienniki 6 i 11).
///
/// 2026-08-28 — `{name}` I POWÓD, DLA KTÓREGO GO TU DOŁOŻONO. Do tego dnia to zdanie brzmiało
/// bezwarunkowo globalnie („A run is already going"), a zapadka też była globalna: bieg
/// w `~/Projects/ledger` odmawiał startu w `~/Projects/atlas`. Zapadka jest od dziś kluczowana
/// workspace'em ([`AppState::begin_run`]), więc zdanie MUSI nazwać ten jeden folder — inaczej
/// człowiek czyta, że zajęty jest cały Loadout, i szuka Stopu tam, gdzie nic nie idzie.
/// Wypełnia je [`already_going_in`]; szablon zostaje stałą, bo kryterium po stronie okna czyta
/// go z tego pliku, zamiast trzymać drugą kopię zdania (niezmiennik 23).
const ALREADY_GOING: &str = "A run is already going in \"{name}\", and Loadout leads one run at \
                             a time in each folder so that Stop always reaches the one that is \
                             working. Press Stop first, then ask again.";

/// Zdanie chwilowego czekania dla folderu triggera.
///
/// 2026-09: to nie jest stan ledgera — zapis przeżyłby awarię jako kłamstwo. Szablon stoi
/// w Rustowym autorytecie, żeby poll i ekran nie miały dwóch wersji tego samego faktu
/// (niezmienniki 13 i 23).
const WAITING_IN_THE_FOLDER: &str = "Waiting — a run is going in \"{name}\". Loadout will check \
                                     again after that run finishes.";

/// Zdanie odmowy dla konkretnego folderu.
///
/// Wolna funkcja, nie metoda: liczy się z szablonu i jednej nazwy, a nazwę wybiera wołający —
/// który jako jedyny wie, czy folder stoi jeszcze na liście przełącznika.
fn already_going_in(name: &str) -> String {
    ALREADY_GOING.replace("{name}", name)
}

fn waiting_in(name: &str) -> String {
    WAITING_IN_THE_FOLDER.replace("{name}", name)
}

/// Nazwa folderu dokładnie z tej samej biblioteki, którą czyta przełącznik kart.
fn workspace_name(home: &Path, project: &Path) -> String {
    let at = crate::workspace::WorkspaceId::for_folder(project);
    commands::workspaces::list_workspaces_inner(home)
        .unwrap_or_default()
        .into_iter()
        .find(|one| crate::workspace::WorkspaceId::for_folder(Path::new(&one.folder)) == at)
        .map_or_else(
            || {
                at.as_path().file_name().map_or_else(
                    || at.to_string(),
                    |last| last.to_string_lossy().into_owned(),
                )
            },
            |one| one.name,
        )
}

/// Uchwyt do biegu, który idzie **teraz w tym jednym workspace**.
///
/// 2026-08-28 — CAŁA TREŚĆ KLUCZOWANIA ZAPADKI MIEŚCI SIĘ W TYCH DWÓCH POLACH. Do tego dnia
/// [`AppState`] trzymał JEDEN uchwyt na aplikację, więc „ile naraz" na poziomie biegów wynosiło
/// jeden na całego Loadouta — a produkt obiecuje agentów pracujących w TWOICH folderach, w liczbie
/// mnogiej. Tożsamość jest kanoniczna ([`crate::workspace::WorkspaceId`]), a nie surowym napisem
/// ze ścieżką: `~/p/x` i `~/p/./x` to jeden folder i muszą dać jeden wpis, bo dwa uchwyty nad
/// jednym folderem to dwa biegi piszące po tych samych plikach (§6a reguła 1).
struct Live {
    /// Kanoniczna tożsamość folderu — ta sama, którą liczy pasek kart.
    at: crate::workspace::WorkspaceId,
    /// Uchwyt biegu, który ten folder prowadzi teraz.
    control: RunControl,
}

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
    /// Foldery, ktorych biegi ta sesja juz uzgodnila z tym, co naprawde zyje na maszynie.
    ///
    /// 2026-08-23 — RAZ NA FOLDER NA SESJE, i oba slowa sa tu wazne. „Raz", bo uzgodnienie
    /// czyta i przepisuje pliki biegow; wolane przy kazdej komendzie bilo by sie o nie z zywym
    /// biegiem. „Na sesje", bo bieg ubity razem z oknem poznaje sie po tym, ze zginal ZANIM to
    /// okno wstalo — a wszystko, co ta sesja sama uruchomi, jest juz po uzgodnieniu i nigdy
    /// przez nie nie przechodzi.
    reconciled: Mutex<std::collections::BTreeSet<PathBuf>>,
    /// **Jedyna pula miejsc tej aplikacji** — „ile naraz" znaczy naraz (niezmiennik 11).
    ///
    /// Jedna na aplikację, nie jedna na bieg, i to jest różnica między szybszą pracą
    /// a zamrożonym laptopem: trzy karty po trzech agentach to dziewięciu procesów po ~583 MB
    /// szczytowego RSS (`docs/ARCHITECTURE.md` §6a, `[T7 §7.1, V]`). Do 2026-08-24 każdy bieg
    /// zakładał sobie własną pulę (`Limiter::new(request.how_many_at_once)` w
    /// `run_workflow_inner`), więc suwak na trzech znaczył `3 × liczba biegów`.
    ///
    /// Klon dzieli tę samą pulę i to jest cały mechanizm: [`AppState::begin_run`] wkłada klon
    /// do świeżego uchwytu biegu, więc każdy bieg tej aplikacji bierze miejsca stąd,
    /// którymikolwiek drzwiami wszedł.
    slots: Limiter,
    /// Uchwyty biegów, które idą **teraz** — po jednym na workspace, najnowszy na końcu.
    ///
    /// 2026-08-28 — BYŁ TU JEDEN UCHWYT NA CAŁĄ APLIKACJĘ i to jest zdjęta blokada. Zapadka
    /// „jeden bieg naraz" jest od dziś kluczowana kanonicznym folderem ([`Live`]), więc dwa
    /// workspace'y mają swoje biegi w tej samej chwili, a drugi start w TYM SAMYM folderze dalej
    /// jest odmową, nie podmianą. **Zapadka nie jest limiterem i nie wolno jej z nim mieszać**:
    /// sufit sumy równoległych kroków trzyma dalej JEDNA pula ([`AppState::slots`]), globalna
    /// tak samo jak przed tą zmianą (niezmiennik 11).
    ///
    /// `Vec` i skan liniowy, nie mapa: kart jest najwyżej
    /// [`commands::workspaces::MOST_WORKSPACES`], a kolejność jest tu treścią — „najnowszy na
    /// końcu" jest odpowiedzią [`AppState::deps`] na pytanie „który bieg jest ten żywy".
    ///
    /// Wpisy nie są sprzątane po zejściu biegu (jeden na dotknięty folder) i jest to świadomy
    /// dług: uchwyt z dowodem zejścia nikogo nie zatrzymuje, a czyszczenie generacji jest osobną
    /// robotą razem z adresowaniem Stopu po identyfikatorze biegu.
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
    live: Mutex<Vec<Live>>,
    /// Wątki lidera — po jednym na TERMINAL, wszystkie w jednym rejestrze.
    ///
    /// # Dlaczego nie ma tu globalnego zamka asynchronicznego
    ///
    /// Rejestr bierze krótki `std::sync::Mutex` wyłącznie na lookup/insert/clone, a żywą sesję
    /// posiada actor konkretnego terminalu. Dzięki temu wielominutowe `Codex handle.wait()` w
    /// terminalu A nie zatrzymuje wiadomości ani Stopu terminalu B, a żaden zamek synchroniczny
    /// nie przeżywa `await` (niezmiennik 8).
    ///
    /// # 2026-08-20 (T-71) — TU STAŁA JEDNA ROZMOWA NA CAŁĄ APLIKACJĘ, I TO JEST ZDJĘTA BLOKADA
    ///
    /// Do tego dnia stało w tym miejscu `tokio::sync::Mutex<Option<commands::chat::Chat>>`, czyli
    /// JEDNA rozmowa na aplikację, a rejestr wątków — istniejący od T-60, z własnymi kryteriami —
    /// nie był przez żywą aplikację konstruowany ani razu. Pisarz T-60 zapisał powód wprost:
    /// `Threads::say` wymaga WSKAZANEGO lidera, a wskazania nie było czym dowieźć z okna, bo
    /// brakowało klucza obok `folder` — czyli zmiany w `src/sections/run/io.ts`, na którą jego
    /// mandat nie pozwalał. Odmówił podstawienia połowy i miał rację: rozmowa, w której każde
    /// zdanie odbija się o „wskaż lidera", jest odmową, której człowiek nie ma jak spełnić.
    ///
    /// T-71 posiada oba końce tej drogi, więc blokada jest zdjęta w całości, a stare pole
    /// **znika**, nie zostaje obok. Dwa domy dla odpowiedzi „gdzie mieszka ta rozmowa" są pierwszą
    /// rzeczą, która się rozjedzie (niezmiennik 13), i rozjadą się po cichu: jedna droga zakłada
    /// wątki per terminal, druga pisze do jednej rozmowy na całą aplikację, a z ekranu obie
    /// wyglądają tak samo. Pilnuje tego kryterium na źródle tego pliku
    /// (`tests/it/live_chat_goes_through_the_registry.rs`).
    /* Rejestr ma krótki zamek wewnętrzny, a każdy terminal własnego actora. Pole NIE jest
     * opakowane w globalny async Mutex: Codex `handle.wait()` może trwać minuty i nie wolno mu
     * wtedy zatrzymać rozmowy ani Stopu innego terminalu (niezmiennik 8). */
    leads: commands::chat::Threads,
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
    /// Miejsce na jedno porównanie kopii pozycji importu i token tego, które trwa teraz.
    ///
    /// **Osobne pole od [`AppState::drafting`]**, mimo identycznego kształtu w środku: draft
    /// umiejętności i porównanie kopii to dwa różne pytania, zadawane z dwóch różnych ekranów.
    /// Jedno miejsce na oba znaczyłoby, że rozpoczęte porównanie odmawia napisania umiejętności
    /// w sąsiedniej karcie, a Stop w jednej sekcji ubija robotę w drugiej.
    ///
    /// Powód, dla którego nie jest to [`AppState::live`], stoi w całości przy `drafting`:
    /// uchwyt biegu jest PODMIENIANY przy każdym Starcie, więc porównanie trzymające się
    /// tamtego traci swój token w chwili, w której człowiek uruchomi bieg — i Stop przy
    /// wierszu importu przestaje cokolwiek robić, bez ani jednego zdania.
    comparing: commands::import::Comparing,
    /// Miejsce na jedno pisanie kandydatek w Labie i token tej tury.
    ///
    /// **Trzecie osobne pole obok [`AppState::drafting`] i [`AppState::comparing`]**, z tego
    /// samego powodu, dla którego tamte dwa są osobne: trzy różne pytania, zadawane z trzech
    /// różnych ekranów. Jedno miejsce na wszystkie znaczyłoby, że Stop przy kandydatkach
    /// w Labie ubija porównanie kopii otwarte w Imporcie.
    proposing: commands::lab::Proposing,
    /// Rzeczy, które człowiek uruchomił komendą — i których Loadout jest właścicielem.
    ///
    /// **Jedna lista na aplikację, nie jedna na zakres**, i powód stoi w całości przy
    /// [`commands::processes::Processes`]: rzecz uruchomiona w jednym folderze biegnie dalej po
    /// przełączeniu widoku, a lista, która by ją wtedy ukryła, jest listą, po której zostaje
    /// osierocony proces palący maszynę.
    ///
    /// **Osobne pole, nie [`AppState::live`]**, z tego samego powodu, co przy
    /// [`AppState::drafting`]: uchwyt biegu jest PODMIENIANY przy każdym Starcie, a `/start npm
    /// run dev` nie jest biegiem i nie ma prawa zniknąć razem z cudzym. Stop na kafelku przestałby
    /// wtedy cokolwiek robić, bez ani jednego zdania.
    ///
    /// W środku siedzi `std::sync::Mutex` i nigdy nie jest trzymany przez `await`
    /// (niezmiennik 8) — powód i kształt przy [`commands::processes::Processes`].
    /// Rejestr rzeczy, które mają zostać żywe. `Arc`, bo sięga po niego także bieg:
    /// kafelek „uruchom i zostaw" oddaje mu proces, zamiast trzymać go przy kroku.
    started: std::sync::Arc<commands::processes::Processes>,
}

/// Jednorazowe pozwolenie Rusta na odpytanie triggera.
///
/// Typ niesie katalog i zdjętą pod zamkiem listę zajętych workspace'ów; okno nie ma pola,
/// którym mogłoby podrobić tę decyzję. Zajęty tick czyta najwyżej gotowy receipt i nigdy nie
/// fetchuje ani nie zapisuje.
#[derive(Debug)]
pub struct TriggerPollPermit {
    home: PathBuf,
    busy_workspaces: Vec<crate::workspace::WorkspaceId>,
}

impl TriggerPollPermit {
    /// Folder triggera jest zajęty tylko wtedy, gdy TEN kanoniczny klucz był żywy w migawce.
    fn held_folder(&self, slug: &str) -> Result<Option<PathBuf>, String> {
        if self.busy_workspaces.is_empty() {
            return Ok(None);
        }
        let trigger =
            commands::triggers::load(&self.home, slug).map_err(|error| error.to_string())?;
        let Some(folder) = trigger.workspace.map(PathBuf::from) else {
            return Ok(None);
        };
        let at = crate::workspace::WorkspaceId::for_folder(&folder);
        Ok(self.busy_workspaces.contains(&at).then_some(folder))
    }

    /// Wspólna odpowiedź trzech dróg odczytu: trwały receipt ma pierwszeństwo przed czekaniem.
    fn busy_poll(&self, slug: &str) -> Result<Option<commands::triggers::TriggerPoll>, String> {
        let Some(folder) = self.held_folder(slug)? else {
            return Ok(None);
        };
        let sentence = waiting_in(&workspace_name(&self.home, &folder));
        commands::triggers::accepted_while_busy(&self.home, slug)
            .map(|receipt| {
                Some(receipt.unwrap_or(commands::triggers::TriggerPoll::Busy { sentence }))
            })
            .map_err(|error| error.to_string())
    }

    /// Produkcyjny odczyt przez `curl`, wykonywany dopiero po decyzji o zajętości.
    pub fn poll(
        self,
        slug: &str,
        created_at: i64,
    ) -> Result<commands::triggers::TriggerPoll, String> {
        if let Some(busy) = self.busy_poll(slug)? {
            return Ok(busy);
        }
        commands::triggers::poll(&self.home, slug, created_at).map_err(|error| error.to_string())
    }

    /// Ten sam odczyt z wstrzykniętym fetcherem, żeby test mógł policzyć wywołania bez sieci.
    pub fn poll_with<F>(
        self,
        slug: &str,
        created_at: i64,
        fetch: F,
    ) -> Result<commands::triggers::TriggerPoll, String>
    where
        F: FnOnce(
            &commands::triggers::Trigger,
        ) -> Result<Vec<u8>, commands::triggers::TriggerError>,
    {
        if let Some(busy) = self.busy_poll(slug)? {
            return Ok(busy);
        }
        commands::triggers::poll_with(&self.home, slug, created_at, fetch)
            .map_err(|error| error.to_string())
    }

    /// Jedno zdjęcie wstrzymania: trigger znów pyta źródło, ale dokładnie raz.
    ///
    /// Żywy bieg dalej daje `Busy`, tak jak przy [`TriggerPollPermit::poll`] — zdjęcie pauzy
    /// kończy się pytaniem do sieci, więc nie może obejść tej samej decyzji o zajętości.
    pub fn resume(
        self,
        slug: &str,
        created_at: i64,
    ) -> Result<commands::triggers::TriggerPoll, String> {
        if let Some(busy) = self.busy_poll(slug)? {
            return Ok(busy);
        }
        commands::triggers::resume(&self.home, slug, created_at).map_err(|error| error.to_string())
    }

    /// Jawne ponowienie korzysta z tego samego rustowego autorytetu co Start i `/ask`.
    ///
    /// `Accepted` powstaje przed pierwszym wywolaniem sterownika, wiec sam receipt nie dowodzi,
    /// ze poprzedni bieg juz zszedl. Odmowa dzieje sie przed odczytem ledgera: zywy bieg nie
    /// moze dostac drugiej proby tylko dlatego, ze okno pokazalo historyczny receipt.
    pub fn retry(
        self,
        slug: &str,
        created_at: i64,
    ) -> Result<commands::triggers::TriggerDelivery, String> {
        if let Some(folder) = self.held_folder(slug)? {
            return Err(already_going_in(&workspace_name(&self.home, &folder)));
        }
        commands::triggers::retry(&self.home, slug, created_at).map_err(|error| error.to_string())
    }
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

/// Ile najdłużej wolno domykać indeks przy wyjściu z aplikacji.
///
/// 2026-09 (Z-6). Domknięcie dziennika czeka na czytelników, a czytelnikiem jest każde okno tej
/// aplikacji — więc bez sufitu wyjście umiałoby nie nastąpić. Aplikacja, której nie da się
/// zamknąć, jest gorsza niż indeks z dziennikiem na dysku: dziennik następne uruchomienie
/// domknie samo, a z niezamykalnym oknem człowiek zostaje sam.
const CLOSING_THE_INDEX_CEILING: Duration = Duration::from_secs(5);

impl AppState {
    /// Składa stan aplikacji z rzeczy, które umie zbudować wyłącznie powłoka okna.
    ///
    /// Zapadka zaczyna życie **pusta**, a folder bez wpisu dostaje uchwyt **już zeszły**
    /// ([`AppState::nothing_going`]). Bez tego Stop naciśnięty, zanim cokolwiek ruszyło, czekałby
    /// na dowód od biegu, którego nigdy nie było — a `stop_run_inner` mówi to wprost: uchwyt
    /// biegu, którego nikt nie uruchomił, nie ma czego dowieść. Przycisk wieszający okno jest
    /// gorszy od przycisku, który nic nie robi.
    #[must_use]
    pub fn new(home: PathBuf, project: PathBuf, store: Store, drivers: Drivers) -> Self {
        /* JEDYNE MIEJSCE, W KTÓRYM STOI LICZBA KROKÓW CIĘŻKICH, i stoi tu jedynka.
         *
         * Niezmiennik 26: dwa `cargo`/`rustc` naraz na tym Macu przypinają kompresor pamięci
         * i zamrażają maszynę przy zerowym swapie. Krok „sprawdź" bierze więc miejsce z puli
         * **i** to jedno miejsce ciężkie (`engine::limits::Weight::Heavy`), a kroki agenta
         * biegną obok niego normalnie.
         *
         * Szerokość samej puli jest tymczasowa: „ile naraz" jest odpowiedzią człowieka udzieloną
         * przy Starcie, więc ustawia ją pierwszy bieg (`commands::run::run_workflow_inner`).
         * Wpisanie tu ósemki dałoby okno, w którym pula jest szersza niż suwak. */
        let slots = Limiter::with_heavy(1, 1);
        Self {
            home,
            project,
            store,
            drivers,
            reconciled: Mutex::new(std::collections::BTreeSet::new()),
            slots,
            live: Mutex::new(Vec::new()),
            leads: commands::chat::Threads::new(),
            drafting: commands::skills::Drafting::new(),
            comparing: commands::import::Comparing::new(),
            proposing: commands::lab::Proposing::new(),
            started: std::sync::Arc::new(commands::processes::Processes::new()),
        }
    }

    /// Kończy wszystko, co ta aplikacja prowadzi, i domyka indeks. **Cała droga wyjścia.**
    ///
    /// # 2026-09 (Z-6) — dlaczego to jest JEDNO ciało, a nie lista w powłoce okna
    ///
    /// Do tego dnia ta lista stała w `lib.rs`, w obsłudze `CloseRequested`, i była jedyną kopią.
    /// ⌘Q ani Quit z menu nie zamykają okna — kończą aplikację — więc szły zupełnie inną drogą
    /// (`RunEvent::ExitRequested`), której nikt nie obsługiwał: proces znikał, a agenci
    /// przechodzili pod PID 1 i palili limit dalej (niezmiennik 6). Polityka mieszka od dziś
    /// w jednym rdzeniu, a powłoka okna ma po pięć linii na zdarzenie (niezmiennik 23).
    ///
    /// Kolejność jest wiążąca i wynika z tego, co po czym pisze: najpierw schodzą procesy, które
    /// jeszcze coś zapisują, a dopiero na końcu domyka się indeks. Odwrócenie oznaczałoby
    /// domknięty dziennik i pisarza, który po nim jeszcze raz dopisuje.
    ///
    /// **Żadna porażka nie zabiera drogi następnym krokom** i żaden z nich nie kończy tej funkcji
    /// wcześniej. Pierwsze `?` w środku zostawiałoby żywego agenta za każdym razem, gdy
    /// zatrzymanie któregoś biegu się nie udało — czyli dokładnie wtedy, kiedy sprzątanie po nim
    /// jest do czegoś potrzebne.
    pub async fn close_everything_down(&self) {
        /* KAŻDY ŻYWY FOLDER, nie jeden uchwyt: od 2026-08-28 zapadka biegu jest kluczowana
         * workspace'em, więc dwa foldery mogą mieć swoje biegi w tej samej chwili. Wyjście
         * sięgające do jednego z nich zostawiłoby drugi żywy pod PID 1 — dokładnie tę sierotę,
         * przed którą stoi cała ta droga. */
        if let Err(error) = self.stop_every_live_run_before_closing().await {
            tracing::error!("closing anyway: the run could not be stopped: {error}");
        }
        /* Rozmowa z orchestratorem też jest procesem — po zamknięciu okna przeszłaby pod
         * PID 1 i pracowała dalej, a odzyskiwanie po niej nie posprząta, bo rozmowa nie ma
         * wpisu w indeksie biegów. */
        self.close_chat().await;
        /* 2026-08-23 — bez tej linii `npm run dev` podniesiony przez `/start` albo przez
         * kafelek „uruchom i zostaw" przeżywa zamknięcie okna, przechodzi pod PID 1 i trzyma
         * port — a odzyskiwanie po nim nie posprząta, bo nie ma go w indeksie biegów. Rzecz,
         * której Loadout jest właścicielem, ma umrzeć z Loadoutem (niezmiennik 6). */
        for proof in self.close_started().await {
            if matches!(proof, crate::engine::supervisor::GroupProof::Alive { .. }) {
                tracing::error!(
                    "something Loadout started was still alive after the full stop; look for it \
                     in Activity Monitor"
                );
            }
        }
        /* INDEKS NA KOŃCU, pod sufitem czasu. Domknięcie przepisuje dziennik do bazy i przycina
         * plik; bez niego `loadout.db-wal` przeżywa wyjście z dziesiątkami megabajtów
         * (zmierzone 2026-09-02). Sufit stoi przy [`CLOSING_THE_INDEX_CEILING`]. */
        match tokio::time::timeout(CLOSING_THE_INDEX_CEILING, self.store.close()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::error!("the index could not be closed down: {error}"),
            Err(_) => tracing::error!(
                "the index was still busy after {} s, so Loadout is leaving it to the next start",
                CLOSING_THE_INDEX_CEILING.as_secs()
            ),
        }
    }

    /// Kończy **wszystko**, co człowiek uruchomił komendą, i oddaje po jednym dowodzie na rzecz.
    ///
    /// Wołane przy zamykaniu okna, obok zatrzymania biegu i rozmowy z liderem. Powód jest ten
    /// sam, co tam: rzecz, która przeżyje Loadouta, przechodzi pod PID 1 i pracuje dalej
    /// (`recovery.rs`, nagłówek), a odzyskiwanie po niej nie posprząta — nie ma wpisu w indeksie
    /// biegów. `npm run dev` trzymający port 5273 po zamknięciu okna jest tego najtańszym
    /// przykładem; agent palący limit w tle jest najdroższym.
    ///
    /// # Kto ją woła
    ///
    /// [`AppState::close_everything_down`], czyli JEDYNA droga wyjścia z aplikacji — ta sama dla
    /// czerwonego guzika i dla ⌘Q. Do 2026-09 (Z-6) nie wołał jej nikt i było to zgłoszenie
    /// zapisane w tym miejscu (AGENTS.md §7): powłoka okna wołała wtedy dwie linie z trzech,
    /// więc `/start` przeżywał zamknięcie okna.
    ///
    /// Dowód, że ta droga naprawdę kończy każdą rzecz, stoi w
    /// `tests/it/started_processes_die_with_the_window.rs` — o jedną warstwę niżej, na
    /// [`commands::processes::Processes::close`], bo `AppState` wymaga w teście otwartej bazy
    /// i fabryki sterowników.
    pub async fn close_started(&self) -> Vec<crate::engine::supervisor::GroupProof> {
        self.started.close().await
    }

    /// Kończy rozmowy lidera — WSZYSTKIE, po jednej na terminal — i żąda od każdej dowodu.
    ///
    /// Wołane przy zamykaniu okna, obok zatrzymania biegu (`lib.rs`, `CloseRequested`). Rozmowa
    /// jest programem jak każdy inny: po śmierci Loadouta przeszłaby pod PID 1 i pracowała dalej
    /// (`recovery.rs`, nagłówek) — czyli dokładnie ten defekt, który 2026-08-19 naprawiono dla
    /// biegów. Odzyskiwanie po niej nie posprząta, bo rozmowa nie ma wpisu w indeksie biegów,
    /// a osierocony agent pali limit u dostawcy: to jest błąd finansowy, nie higieniczny.
    ///
    /// NAZWA ZOSTAJE, bo woła ją `lib.rs` jedną linią, a tamten plik nie należy do tego zadania
    /// (AGENTS.md §7). Zmieniła się liczba rozmów, które kończy, nie czynność.
    ///
    /// `Threads::close` najpierw wysyła Stop do wszystkich actorów, a dopiero potem czeka na
    /// dowody. Dlatego długa eskalacja terminalu A nie opóźnia nawet rozpoczęcia eskalacji B;
    /// krótki zamek rejestru nie przeżywa żadnego z tych `await`-ów (niezmiennik 8).
    ///
    /// `Alive` po pełnej eskalacji idzie do dziennika, bo jest jedynym trwałym śladem: to jest
    /// stan, o którym nikt się inaczej nie dowie, a `lib.rs` zamyka okno tak czy inaczej —
    /// okno, którego nie da się zamknąć, zamykałoby człowieka wewnątrz aplikacji.
    pub(crate) async fn close_chat(&self) {
        let proofs = self.leads.close().await;
        for proof in proofs {
            if matches!(proof, crate::engine::supervisor::GroupProof::Alive { .. }) {
                tracing::error!(
                    "a lead agent was still answering after Loadout asked it to stop; look for \
                     it in Activity Monitor"
                );
            }
        }
    }

    /// Człowiek zamknął TĘ kartę: rozmowa TEGO terminalu schodzi i oddaje dowód śmierci grupy.
    ///
    /// Kończy JEDEN wątek i milczy o pozostałych — [`AppState::close_chat`] robi to samo dla
    /// wszystkich naraz i należy do zamknięcia OKNA, nie karty.
    pub async fn close_the_lead(&self, terminal: &str) {
        let proof = self.leads.close_at(terminal).await;
        if matches!(
            proof,
            Some(crate::engine::supervisor::GroupProof::Alive { .. })
        ) {
            tracing::error!(
                "a lead agent was still answering after its terminal was closed; look for it in \
                 Activity Monitor"
            );
        }
    }

    /* ── ROZMOWA NALEŻY DO TERMINALU ────────────────────────────────────────────────────────
     *
     * DLACZEGO TE DWIE METODY W OGÓLE ISTNIEJĄ, skoro obok stoją skorupy `#[tauri::command]`.
     * Bo skorupa bierze `State<'_, AppState>`, którego w teście nie da się zbudować — a wtedy
     * jedynym sposobem na osądzenie żywej drogi byłoby zbudowanie [`commands::chat::Threads`]
     * w teście, czyli dowiedzenie mechanizmu, którego produkt nie woła. Dokładnie tę wadę
     * znalazł recenzent T-70 i dokładnie tę wadę opisuje akapit „WĄTEK PER ZAKRES ISTNIEJE
     * I NIE STOI TUTAJ". Skorupa ma więc rozpakować `State` i zawołać to, co niżej, a te dwie
     * metody są tym, co woła okno.
     *
     * `LineSink`, nie `Channel<Vec<Line>>`: kanał do okna umie zbudować wyłącznie okno
     * (`docs/ARCHITECTURE.md` §3), więc zamiana jednego w drugi (`pump_into`) zostaje w skorupie.
     * To jest ten sam szew, którym `tests/it` już dziś otwiera strumień rozmowy.
     */

    /// Okno patrzy na ten terminal — zakłada pompę wierszy i nic więcej.
    ///
    /// Sesja u dostawcy wstaje dopiero przy pierwszym zdaniu ([`AppState::say_to_the_lead`]), bo
    /// tura wystartowana przy montażu ekranu jest turą, za którą ktoś płaci, choć nikt o nic nie
    /// zapytał. Wołane ponownie PRZEKIEROWUJE wiersze na nowy kanał i nie kończy rozmowy: tę drogę
    /// woła każdy montaż ekranu pracy i każde przeładowanie okna.
    ///
    /// BIBLIOTEKA JEDZIE TUTAJ, a nie przy pierwszym zdaniu, i to jest wymóg czasu, nie porządku:
    /// `--add-dir` wchodzi w argv przy STARCIE wątku, więc rozmowa, która zaczęła się przed tym
    /// zdaniem, dostałaby zasięg dopiero przy następnej ([`commands::chat::Threads::library_is`]).
    /// Ta droga stoi przed pierwszym zdaniem z konstrukcji: okno woła ją przy montażu ekranu.
    pub async fn watching_the_lead(
        &self,
        terminal: &str,
        folder: Option<&str>,
        lines: LineSink,
    ) -> Result<(), String> {
        // Ten sam sąd nad folderem, co przy biegu i przy instalacji umiejętności
        // ([`project_folder`]): rozmowa w katalogu, którego nie ma, jest programem, który nie
        // wstaje, a nie ostrzeżeniem. Brak wyboru znaczy „tam, gdzie aplikacja wstała".
        //
        // 2026-09 (Z-10) — `async` wyłącznie dlatego, że [`AppState::project_for`] uzgadnia folder
        // przy pierwszym dotknięciu, a to jest praca gita. Sama ta metoda dalej nie czeka na nic.
        let cwd = self.project_for(folder).await.inspect_err(refused)?;
        self.leads.library_is(self.home.clone());
        self.leads.terminal_lines_go_to(
            &commands::chat::Terminal {
                id: terminal.to_owned(),
                folder: cwd,
            },
            lines,
        );
        Ok(())
    }

    /// Zdanie człowieka do lidera TEGO terminalu.
    ///
    /// `lead` jest identyfikatorem zapisanego agenta i jego brak jest **odmową nazywającą następny
    /// ruch**, nigdy cichym powrotem do zaszytego vendora ([`commands::chat::Lead::pointed_at`]).
    /// Cichy powrót jest tu gorszy niż odmowa: rozmowa idzie, płaci i odpowiada — tylko nie ten
    /// agent, którego człowiek wybrał, a nie ma żadnego sygnału, po którym dałoby się to odróżnić.
    ///
    /// Odmowa wraca NAPISEM, bo dokładnie tym kształtem odrzuca Tauri i dokładnie ten napis
    /// człowiek czyta pod wierszem wejścia (niezmiennik 29).
    /// KIM JEST LIDER, ROZSTRZYGA JEGO ZAPISANA DEFINICJA, i to jest drugie zdjęte zaszycie.
    /// Do 2026-08-20 stała obok tej metody funkcja `chat_driver`, oddająca `Vendor::ClaudeCode`
    /// na sztywno; zniknęła w całości, bo vendora wybiera dziś fabryka po polu `runs_with`
    /// z pliku ([`commands::chat::Threads::say_in`]). Gałąź domyślna jest tym, czego konfiguracją
    /// nie da się wyłączyć — a tutaj nie ma ani jednej gałęzi.
    pub async fn say_to_the_lead(
        &self,
        terminal: &str,
        folder: Option<&str>,
        lead: Option<&str>,
        text: &str,
    ) -> Result<(), String> {
        self.say_to_the_lead_with_images(terminal, folder, lead, text, ValidatedImages::default())
            .await
    }

    pub async fn say_to_the_lead_with_images(
        &self,
        terminal: &str,
        folder: Option<&str>,
        lead: Option<&str>,
        text: &str,
        images: ValidatedImages,
    ) -> Result<(), String> {
        let cwd = self.project_for(folder).await.inspect_err(refused)?;
        /* WSKAZANIE SĄDZIMY PRZED WZIĘCIEM ZAMKA, bo odmowa „nie wskazałeś lidera" nie ma nic
         * wspólnego z rejestrem wątków: czytanie biblioteki pod zamkiem trzymałoby go przez
         * odczyt katalogu, w którym nic się nie zmienia. */
        let who = commands::chat::Lead::pointed_at(self.home.as_path(), lead).map_err(|error| {
            let said = error.to_string();
            refused(&said);
            said
        })?;
        self.leads
            .say_in_with_images(
                &self.drivers,
                &who,
                &commands::chat::Terminal {
                    id: terminal.to_owned(),
                    folder: cwd,
                },
                text,
                images,
            )
            .await
            .map_err(|error| {
                let said = error.to_string();
                refused(&said);
                said
            })
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
    ///
    /// # 2026-08-28 — „NAJNOWSZY ŻYWY UCHWYT", A NIE „UCHWYT FOLDERU STARTOWEGO"
    ///
    /// Znaczenie zostaje CO DO ZNAKU takie, jakie miało: [`AppState::live`] było jednym polem
    /// podmienianym przy każdym starcie, więc stał w nim uchwyt ostatniego startu — niezależnie
    /// od tego, w którym folderze ten start poszedł. Zapadka kluczowana workspace'em ma teraz
    /// tych uchwytów kilka, a `deps_in(self.project)` znaczyłoby „folder, pod którym wstało
    /// okno" — czyli Dalej, Powiedz i zamknięcie karty gubiłyby bieg idący gdziekolwiek indziej.
    ///
    /// # 2026-09 (Z-35) — CO PO TEJ METODZIE ZOSTAŁO: ANI JEDNA DROGA Z OKNA
    ///
    /// Trzy drogi wymienione wyżej — Dalej, Powiedz i Stop — biorą od dziś folder karty
    /// ([`AppState::stop_the_run_in`], [`AppState::continue_the_run_in`],
    /// [`AppState::say_in_the_run_at`]), bo okno ten folder ZNA: samo wysłało go do
    /// `run_workflow` (`src/sections/run/io.ts`, `start`). „Ten, który ruszył ostatni" był
    /// wyborem na czas, w którym adresu nie było, i miał zmierzoną cenę: „dalej" naciśnięte
    /// w folderze A podbijało licznik biegu B.
    ///
    /// Zostają wyłącznie trzy szwy pytające „CZYJ uchwyt jest żywy", wszystkie w `tests/it`
    /// i żaden nie z okna: `no_start_orphans_the_previous.rs`, `ask_respects_the_pool.rs`
    /// i `trigger_run_is_accepted_once.rs`. Te pytają o osierocenie, czyli o rzecz, na którą
    /// odpowiedź MUSI być „najnowszy" — klon, który wołający trzyma w ręku, nie zauważa
    /// podmiany (powód wyżej, akapit z 2026-08-20).
    pub fn deps(&self) -> RunDeps<'_> {
        // Zamek wzięty i oddany w JEDNYM wyrażeniu, przed czymkolwiek, co czeka (niezmiennik 8).
        let newest = self
            .live
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .last()
            .map(|one| one.control.clone());
        self.deps_with(
            self.project.as_path(),
            newest.unwrap_or_else(|| self.nothing_going()),
        )
    }

    /// Współpracownicy biegu, który ma pracować w **tym** katalogu.
    ///
    /// 2026-08-18 — POWSTAŁO, BO FOLDER WYBRANY W OKNIE NIE DOJEŻDŻAŁ NIGDZIE. `AppState.project`
    /// ustala `lib.rs` raz, przy starcie okna, a `＋` na pasku kart zakładał kartę i kończył na
    /// `workspaces.open(...)` — bez ani jednego `invoke`. Człowiek wybierał `~/Projects/moj`,
    /// dostawał kartę z tą nazwą, a agent (gdyby wystartował) pracowałby w katalogu ustalonym
    /// przy starcie. „Agenci pracują w twoim folderze" jest CAŁĄ obietnicą tego produktu, więc
    /// katalog musi przyjechać z żądaniem, a nie ze stałej sprzed wyboru.
    ///
    /// 2026-08-28 — ODDAJE UCHWYT **TEGO** WORKSPACE'U. Folder bez wpisu w zapadce dostaje
    /// uchwyt z dowodem zejścia ([`AppState::nothing_going`]), a nie świeży: Stop nad folderem,
    /// w którym nic nigdy nie ruszyło, czekałby na dowód od biegu, którego nie było, czyli
    /// wieszałby okno w najczęstszym przypadku ze wszystkich.
    fn deps_in<'a>(&'a self, project: &'a Path) -> RunDeps<'a> {
        let at = crate::workspace::WorkspaceId::for_folder(project);
        // Zamek wzięty i oddany w JEDNYM wyrażeniu — między nim a jakimkolwiek `await`
        // wołającego nie ma ani jednej instrukcji (niezmiennik 8).
        let here = self
            .live
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|one| one.at == at)
            .map(|one| one.control.clone());
        self.deps_with(project, here.unwrap_or_else(|| self.nothing_going()))
    }

    /// Wszystko poza uchwytem — jedno miejsce, w którym składa się [`RunDeps`].
    ///
    /// Osobno, bo uchwyt wybiera się dwoma różnymi pytaniami („co idzie w tym folderze" kontra
    /// „co ruszyło ostatnio"), a reszta zależności jest w obu przypadkach ta sama. Dwie kopie
    /// tej listy pól rozjechałyby się przy pierwszym nowym polu (niezmiennik 13).
    fn deps_with<'a>(&'a self, project: &'a Path, control: RunControl) -> RunDeps<'a> {
        RunDeps {
            processes: std::sync::Arc::clone(&self.started),
            home: self.home.as_path(),
            project,
            store: &self.store,
            drivers: Arc::clone(&self.drivers),
            control,
        }
    }

    /// Uchwyt biegu, którego nie ma: niesie pulę aplikacji i **ma już dowód zejścia**.
    ///
    /// Dowód zapala się od razu, bo to jest odpowiedź na „nic tu nie idzie", a nie na „bieg
    /// właśnie ruszył". Uchwyt świeży w tym miejscu zamieniłby Stop nad pustym folderem
    /// w czekanie bez końca, a zapadkę — w zamek, którego nikt nigdy nie otworzy.
    fn nothing_going(&self) -> RunControl {
        let idle = RunControl::sharing(self.slots.clone());
        idle.settle();
        idle
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
    /// Blokady „na zawsze" tu nie ma i nie może być: warunek pyta o DOWÓD ZEJŚCIA
    /// ([`proved_down`]), a ten dowód zapala `settle()` na każdej drodze wyjścia z biegu — więc
    /// bieg, który zszedł, przestaje kogokolwiek zatrzymywać. Zapadka, która nigdy się nie
    /// otwiera, jest gorsza od wady, przed którą stoi.
    ///
    /// # 2026-08-20 (T-69, runda naprawcza) — WARUNEK PYTA O DOWÓD ZEJŚCIA, NIE O „PRACUJE"
    ///
    /// Warunek na [`RunControl::is_working`] zostawiał tę samą wadę otwartą na szczelinę kilku
    /// instrukcji, a wąskie okno nie jest tu żadną obroną — jest wyłącznie powodem, dla którego
    /// trafia się w nie wtedy, kiedy człowiek naciska dwa razy, a nie wtedy, kiedy ktoś tego
    /// szuka. `is_working` znaczy „ruszył i nie zszedł", a „ruszył" zapala PIERWSZA LINIA biegu
    /// (`run_workflow_with_slots`), czyli kod, do którego wchodzi się dopiero po tym, jak ta
    /// metoda oddała zamek i wróciła. Żadnego `await` w środku nie ma, ale `Cargo.toml` włącza
    /// `rt-multi-thread`, a Tauri wysyła każdą komendę jako OSOBNE zadanie tej puli: dwa Starty
    /// stoją na dwóch wątkach naprawdę. Wątek drugi bierze ten sam zamek, zanim pierwszy zawołał
    /// `begin()`, czyta „nikt nie pracuje" i podmienia uchwyt — dokładnie ta cicha podmiana,
    /// przed którą stoi całe to zadanie (niezmienniki 6 i 11).
    ///
    /// Odpowiedzią jest pytanie o jedną chwilę wcześniejsze, i **nie wymaga ono ani jednego
    /// nowego znacznika**: uchwyt trafia do [`AppState::live`] dokładnie w jednym miejscu — niżej,
    /// w tej metodzie — i jest wtedy świeży, więc dowodu jeszcze nie ma i mieć nie może. Folder
    /// bez wpisu odpowiada „nic tu nie idzie" samym brakiem wpisu ([`AppState::nothing_going`]
    /// ma swój własny powód: Stop przed pierwszym biegiem nie ma na co czekać). „We wpisie stoi
    /// uchwyt bez dowodu zejścia" znaczy więc dokładnie „ktoś ten uchwyt już wziął i bieg jeszcze
    /// nie zszedł" — a zapala się to W TEJ SAMEJ instrukcji, w której uchwyt tam wchodzi, pod tym
    /// samym zamkiem. Okno nie jest przez to węższe; nie ma go wcale.
    ///
    /// # 2026-08-28 — PYTANIE BRZMI „CZY COŚ IDZIE W TYM FOLDERZE"
    ///
    /// Zapadka jest kluczowana kanoniczną tożsamością workspace'u ([`Live`]), więc bieg w jednym
    /// folderze nie odmawia startu w drugim: obietnicą produktu są agenci pracujący w TWOICH
    /// folderach, w liczbie mnogiej. Wszystko, co wyżej, zostaje słowo w słowo prawdziwe —
    /// tylko dla jednego workspace'u zamiast dla całej aplikacji, i dokładnie dlatego ciało dalej
    /// jest JEDNO na obie drogi startu.
    ///
    /// **Zapadka nie jest limiterem.** Sufit sumy równoległych kroków trzyma dalej jedna pula
    /// ([`AppState::slots`]) i zostaje globalna; ta metoda odpowiada wyłącznie na pytanie, czyj
    /// uchwyt trzyma Stop (niezmiennik 11).
    ///
    /// Klucz liczy [`crate::workspace::WorkspaceId::for_folder`], a nie porównanie napisów:
    /// `~/p/x`, `~/p/x/` i `~/p/./x` to jeden folder, więc porównanie tekstem wpuściłoby drugi
    /// bieg do folderu, w którym już jeden pisze po plikach (§6a reguła 1).
    ///
    /// Czego świadomie NIE robimy, bo każde z tego przerzedza trafienia i żadne nie zamyka okna:
    /// ponownego sprawdzenia `is_working` po podmianie, zamka `tokio::sync` z `await` w środku,
    /// pętli z ponawianiem. I czego nie robimy, bo psuje Stop: zapalenia `began` przy podmianie —
    /// `stop_run_inner` żądałby wtedy dowodu śmierci od biegu, który jeszcze nie ruszył, czyli
    /// czekałby bez końca. „Czy ktoś wziął uchwyt" i „czy jest co zatrzymywać" to dwa pytania
    /// i mają dwie odpowiedzi.
    ///
    /// Najprostszym zapisem tego warunku byłby drugi znacznik w [`RunControl`] („wzięty", obok
    /// „ruszył"). Ten typ mieszka w `src-tauri/src/commands/mod.rs`, który nie należy do T-69
    /// (`AGENTS.md` §7), a `settled` jest w nim prywatne — stąd sonda w [`proved_down`] zamiast
    /// pola. Zachowanie jest to samo w każdym osiągalnym stanie, bo każdy uchwyt, który tu wchodzi,
    /// jest wzięty przez ten start.
    pub fn begin_run<'a>(&'a self, project: &'a Path) -> Result<RunDeps<'a>, String> {
        let at = crate::workspace::WorkspaceId::for_folder(project);
        let taken = {
            // Zamek na CAŁE pytanie i na wymianę, nie na dwa osobne wyrażenia: „czy coś idzie"
            // sprawdzone przed wzięciem zamka jest odpowiedzią sprzed chwili, a między nią
            // a podmianą mieści się drugi start. Zamek `std::sync` i ani jednego `await`
            // w środku (niezmiennik 8) — powód stoi przy [`AppState::deps_in`].
            let mut live = self.live.lock().unwrap_or_else(PoisonError::into_inner);
            match live.iter().position(|one| one.at == at) {
                // Bieg TEGO folderu jeszcze nie zszedł. Odmawiamy, nie ruszając ani tego wpisu,
                // ani wpisów pozostałych folderów: Stop ma po tej odmowie dosięgnąć dokładnie
                // tego biegu, o którym mówi zdanie.
                Some(going) if !proved_down(&live[going].control) => false,
                // Wpis po biegu, który zszedł, ZNIKA i wraca na koniec listy jako świeży —
                // a nie jest podmieniany w miejscu. Kolejność jest tu treścią: „najnowszy na
                // końcu" jest odpowiedzią [`AppState::deps`] na pytanie, który bieg jest ten
                // żywy, więc wpis zostawiony w środku listy odciąłby od okna bieg, który
                // ruszył jako ostatni.
                settled => {
                    if let Some(going) = settled {
                        live.remove(going);
                    }
                    // Ta jedna instrukcja jest i wpisem, i zamknięciem zapadki dla każdego
                    // następnego startu W TYM FOLDERZE: świeży uchwyt nie ma dowodu zejścia,
                    // a warunek wyżej pyta właśnie o dowód. Powód w całości stoi w nagłówku.
                    // Świeży uchwyt niosący KLON PULI APLIKACJI, nie własną pulę: to jest ta
                    // jedna linia, w której „jeden semafor na całą aplikację" przestaje być
                    // zdaniem w komentarzu (niezmiennik 11). `RunControl::new()` w tym miejscu
                    // zakładałby pulę na bieg, czyli dokładnie wadę, którą naprawia T-94.
                    live.push(Live {
                        at,
                        control: RunControl::sharing(self.slots.clone()),
                    });
                    true
                }
            }
        };
        if !taken {
            // Nazwa folderu liczona POZA zamkiem: wybiera ją lista przełącznika, czyli odczyt
            // pliku z biblioteki. Czytanie dysku pod tym zamkiem trzymałoby go przez czas,
            // w którym inny folder chce wziąć swój uchwyt — a to jest dokładnie ta zapadka,
            // która ma przestać być globalna.
            return Err(self.already_going_where(project));
        }
        Ok(self.deps_in(project))
    }

    /// Zdanie odmowy nazywające folder, w którym coś już idzie.
    ///
    /// NAZWA Z PRZEŁĄCZNIKA, bo to ją człowiek widzi na pasku i to jej będzie szukał, kiedy pójdzie
    /// nacisnąć Stop (`commands::workspaces::list_workspaces_inner`). Ścieżka w tym zdaniu byłaby
    /// prawdziwa i bezużyteczna: „`/Users/x/dev/ledger-ui`" nie jest tym, co stoi w menu.
    ///
    /// Folder spoza listy — bieg z triggera albo katalog, pod którym wstało okno — nazywa się
    /// swoim ostatnim składnikiem. Nieczytelna lista NIE zabiera odmowy (niezmiennik 5): odmowa
    /// bez nazwy jest gorsza niż odmowa z nazwą zgadniętą ze ścieżki, a odmowa, która się nie
    /// odbyła, jest drugim biegiem w tym samym folderze.
    fn already_going_where(&self, project: &Path) -> String {
        already_going_in(&workspace_name(&self.home, project))
    }

    /* ── TRZY DROGI Z OKNA, WSZYSTKIE ADRESOWANE FOLDEREM (2026-09, Z-35) ───────────────────
     *
     * CO BYŁO. Stop wołał `stop_every_live_run`, czyli kończył bieg w KAŻDYM żywym workspace,
     * a Dalej i Powiedz brały [`AppState::deps`], czyli „uchwyt, który ruszył ostatni". Trzy
     * różne drogi, jeden skutek: karta `ledger` zabierała pracę karcie `atlas`. Audyt
     * 2026-09-04 nazywa to A-1 i A-2, a komentarze przy tamtych funkcjach nazywały to wprost
     * jako robotę do zrobienia.
     *
     * DLACZEGO DA SIĘ TO DZIŚ, A NIE DAŁO WCZORAJ. Zapadka [`AppState::live`] jest kluczowana
     * kanoniczną tożsamością workspace'u od 2026-08-28, więc uchwyt tego folderu jest jednym
     * odczytem ([`AppState::deps_in`]). Brakowało wyłącznie ADRESU na drucie — a ten okno zna,
     * bo samo wysłało folder do `run_workflow` (`src/sections/run/io.ts`, `start`).
     *
     * DLACZEGO TO SĄ METODY `AppState`, A NIE CIAŁA SKORUP. Bo `State<'_, AppState>` nie da się
     * zbudować bez żywego Tauri, więc polityka zamknięta w skorupie jest polityką, na którą nie
     * da się napisać kryterium — ta sama cena, którą raz już zapłacił `say_to_agent_inner`
     * (jego nagłówek). Każda z nich ma po dwie linie: uchwyt tego folderu plus wywołanie
     * rdzenia (niezmienniki 1 i 23).
     *
     * CZEGO TU NIE MA: identyfikatora biegu. W jednym folderze idzie najwyżej jeden bieg
     * (zapadka odmawia drugiemu), więc folder JEST adresem — dokładnie ten sam, którym karta
     * biegu nazywa się w oknie (`src/sections/run/tabs/store.ts`, `cardForRun`).
     */

    /// Stop naciśnięty na karcie TEGO folderu. Mówi, czy było co zatrzymywać.
    ///
    /// Wraca **dopiero z dowodem**, że po biegu nic nie żyje (niezmiennik 6), a `false` znaczy
    /// „w tym folderze nic nie idzie" i JEST odpowiedzią, nie błędem: naciśnięcie Stopu nad pustą
    /// kartą nie jest pomyłką człowieka. Zdanie `Nothing is running in <folder>.` składa z tej
    /// odpowiedzi okno (`src/sections/run/entry/entry.tsx`, `whatStopSaid`).
    ///
    /// Folder bez wpisu w zapadce dostaje uchwyt z dowodem zejścia ([`AppState::nothing_going`]),
    /// więc ta droga wraca **bez czekania** — inaczej Stop nad pustą kartą czekałby na dowód od
    /// biegu, którego nigdy nie było, czyli wieszałby okno w najczęstszym przypadku ze wszystkich.
    pub async fn stop_the_run_in(&self, project: &Path) -> Result<bool, commands::RunError> {
        commands::run::stop_if_anything_is_going(&self.deps_in(project)).await
    }

    /// „Dalej" na punkcie kontrolnym karty TEGO folderu.
    ///
    /// Wraca dopiero wtedy, gdy bieg naprawdę ruszył (`wait_until_moving` w rdzeniu), i wraca
    /// od razu, kiedy nie było na co odpowiadać — powód w całości stoi przy
    /// [`commands::run::continue_run_inner`].
    pub async fn continue_the_run_in(
        &self,
        project: &Path,
        answer: Option<String>,
    ) -> Result<(), commands::RunError> {
        commands::run::continue_run_inner(&self.deps_in(project), answer).await
    }

    /// „Powiedz coś agentowi, który pracuje" — agentowi biegu TEGO folderu.
    ///
    /// Brak nazwy znaczy od dziś „ten jeden, który pracuje **w tym folderze**", a nie
    /// „gdziekolwiek": lista pracujących kroków wisi na uchwycie biegu, więc adresowanie uchwytu
    /// adresuje też odmowy (`RunError::SeveralAreWorking` i pozostałe cztery). Nazwa kroku
    /// z sąsiedniego folderu jest przez to ODMÓWIONA, a nie doręczona — i to jest właściwa
    /// odpowiedź: nazwa jest adresem wewnątrz biegu, nigdy drogą do biegu obok.
    pub async fn say_in_the_run_at(
        &self,
        project: &Path,
        agent: Option<&str>,
        text: &str,
    ) -> Result<(), commands::RunError> {
        let deps = self.deps_in(project);
        commands::run::say_to_agent_inner(&deps.control, agent, text).await
    }

    /// Przy zamykaniu okna: każdy żywy folder, ale z sufitem czasu na folder.
    ///
    /// # 2026-09 (Z-35) — JEDYNA DROGA „KAŻDY ŻYWY FOLDER", JAKA ZOSTAŁA
    ///
    /// I ma nią zostać. Stała obok niej `stop_every_live_run`, którą wołał Stop z okna, i to
    /// przez nią jedna karta zabierała pracę drugiej. Zamknięcie okna jest inną czynnością
    /// i dlatego zasięg ma inny: człowiek nie mówi wtedy „skończ ten bieg", tylko „skończ tę
    /// aplikację", a rzecz, która przeżyje Loadouta, przechodzi pod PID 1 i pali limit dostawcy
    /// dalej (niezmiennik 6). Tutaj „wszystkie" jest odpowiedzią na zadane pytanie; tam było
    /// odpowiedzią na inne. Wołający jest jeden: [`AppState::close_everything_down`], czyli
    /// czerwony guzik i ⌘Q.
    ///
    /// DWIE METODY, NIE JEDNA Z FLAGĄ, bo to są dwie polityki i obie mieszkają w rdzeniu
    /// (niezmiennik 23): `stop_if_anything_is_going` czeka na dowód tak długo, jak trzeba, a
    /// `stop_before_closing` odróżnia schodzenie od zacięcia — bo przy zamykaniu podniesione jest
    /// już `prevent_close` i człowiek zostaje z oknem, którego nie da się zamknąć. Tutaj zostaje
    /// wyłącznie „po każdym żywym folderze"; sufit i jego uzasadnienie są tam, gdzie były.
    ///
    /// Foldery zbieramy POD zamkiem, a zatrzymujemy PO jego oddaniu (niezmiennik 8): zatrzymanie
    /// czeka na dowód śmierci grupy, więc zamek trzymany przez ten czas zawieszałby każdy inny
    /// folder dokładnie wtedy, kiedy schodzi ten pierwszy.
    ///
    /// 2026-09 (Z-10) — RAZEM, nie po kolei, i tu boli to najbardziej: sufit jest **na folder**,
    /// więc dwa zacięte biegi trzymały okno przez dwa razy trzydzieści sekund. Zmierzone
    /// w `stop_answers_while_the_trees_close.rs` na dwóch folderach schodzących po sekundzie:
    /// zamknięcie trwało dwie, a nie jedną.
    pub async fn stop_every_live_run_before_closing(&self) -> Result<(), commands::RunError> {
        let folders = self.live_folders();
        let deps: Vec<RunDeps<'_>> = folders.iter().map(|folder| self.deps_in(folder)).collect();
        let outcomes = all_at_once(
            deps.iter()
                .map(commands::run::stop_before_closing)
                .collect(),
        )
        .await;
        /* Pierwsza porażka W KOLEJNOŚCI FOLDERÓW, nie pierwsza w czasie: zdanie o zamknięciu ma
         * być tym samym zdaniem przy tym samym stanie aplikacji, a nie tym, które wróciło
         * szybciej. Porażka jednego folderu NIE zabiera drogi pozostałym — pierwsze `?` w środku
         * zostawiałoby żywego agenta za każdym razem, gdy zatrzymanie któregoś z wcześniejszych
         * folderów się nie udało. */
        outcomes
            .into_iter()
            .find_map(Result::err)
            .map_or(Ok(()), Err)
    }

    /// Foldery, które mają w zapadce swój uchwyt — kopia zdjęta pod zamkiem i oddana bez niego.
    fn live_folders(&self) -> Vec<PathBuf> {
        self.live
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|one| one.at.as_path().to_path_buf())
            .collect()
    }

    /// Rezerwuje żywy uchwyt dla Startu z claimem, nie zmieniając ledgeru przy odmowie zajętości.
    pub fn begin_triggered_run<'a>(
        &'a self,
        project: &'a Path,
        claim: &commands::triggers::TriggerClaim,
    ) -> Result<RunDeps<'a>, String> {
        // Claim i jego workspace są sądzone przed wymianą uchwytu: podrobiona wartość ani
        // folder z aktywnej właśnie karty nie mogą zająć zapadki i odciąć Stopu od prawdziwego
        // biegu. Powtórzenie walidacji po `triggered_project` zamyka też zmianę rejestru między
        // rozstrzygnięciem ścieżki a wzięciem uchwytu (incydent 2026-08-21).
        let frozen = commands::triggers::claimed_workspace(&self.home, claim)
            .map_err(|error| error.to_string())?;
        if frozen != project {
            return Err(commands::triggers::TriggerError::WorkspaceMismatch.to_string());
        }
        self.begin_run(project)
    }

    /// Wybiera projekt triggera z trwałej dostawy, nigdy z aktywnego workspace okna.
    ///
    /// Podany folder jest wyłącznie powtórzeniem do porównania. `None` nie znaczy tutaj
    /// „projekt startowy aplikacji” jak przy ręcznym Run: ledger ma własny, zamrożony autorytet.
    pub fn triggered_project(
        &self,
        folder: Option<&str>,
        claim: &commands::triggers::TriggerClaim,
    ) -> Result<PathBuf, String> {
        let frozen = commands::triggers::claimed_workspace(&self.home, claim)
            .map_err(|error| error.to_string())?;
        if let Some(folder) = folder {
            let repeated = project_folder(Some(folder))?
                .ok_or_else(|| commands::triggers::TriggerError::WorkspaceMismatch.to_string())?;
            if repeated != frozen {
                return Err(commands::triggers::TriggerError::WorkspaceMismatch.to_string());
            }
        }
        Ok(frozen)
    }

    /// Pyta jedyny rustowy autorytet o zajętość przed siecią i przed jakimkolwiek zapisem.
    ///
    /// 2026-09: pytanie jest o workspace zapisany w pliku triggera. Migawka zachowuje wszystkie
    /// kanoniczne klucze bez dowodu zejścia, więc bieg w A nie blokuje źródła triggera w B,
    /// a dwa zajęte foldery nadal odpowiadają niezależnie (niezmienniki 11 i 13).
    #[must_use]
    pub fn trigger_poll_permit(&self) -> TriggerPollPermit {
        let live = self.live.lock().unwrap_or_else(PoisonError::into_inner);
        TriggerPollPermit {
            home: self.home.clone(),
            busy_workspaces: live
                .iter()
                .filter(|one| !proved_down(&one.control))
                .map(|one| one.at.clone())
                .collect(),
        }
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
    /// PUBLICZNE, bo to jest szew, który kryterium ma prawo dotknąć —
    /// `tests/it/runs_left_over_are_reconciled.rs`. Powód stoi przy
    /// [`Self::settle_what_the_last_window_left`]: naprawa raz już wylądowała w kodzie bez
    /// wołających i wyglądała na zrobioną. Kryterium wołające tę metodę pilnuje, że nie
    /// wyląduje tam drugi raz.
    /// 2026-09 (Z-10) — `async`, BO UZGODNIENIE CHODZI PO REPOZYTORIUM. Pierwsze dotknięcie
    /// folderu domyka drzewa, których nikt już nie prowadzi (`isolate::finish`: `git status`,
    /// `commit`, `worktree remove`), i robiło to na wątku wołającego. Zmierzone u właściciela
    /// 2026-09-02: 87 katalogów `work/` do zamknięcia przy jednym otwarciu folderu.
    pub async fn project_for(&self, folder: Option<&str>) -> Result<PathBuf, String> {
        // Brak wyboru jest wartością, nie błędem: dopóki nikt nie otworzył karty, biegniemy
        // tam, gdzie aplikacja wstała. Sam FOLDER sprawdza [`project_folder`] i to jest jedyne
        // miejsce, w którym te trzy zdania odmowy mieszkają.
        let project = project_folder(folder)?.unwrap_or_else(|| self.project.clone());
        self.settle_what_the_last_window_left(&project).await;
        Ok(project)
    }

    /// Sprząta po poprzednim oknie we **wszystkich** znanych folderach, raz, przy starcie.
    ///
    /// Wołane raz, z `lib.rs`, ZANIM okno dostanie ten stan — i oba warunki są tu potrzebne.
    /// „Zanim": w tym momencie ta sesja nie prowadzi jeszcze ani jednego biegu, więc wszystko,
    /// co stoi w `running`, zostawił ktoś inny. „Wszystkich": sierota w folderze, do którego
    /// dziś nie zaglądasz, dalej pali limit dostawcy i dalej trzyma port — a naprawa oparta
    /// wyłącznie na [`Self::project_for`] czeka z nią do dnia, w którym człowiek akurat kliknie
    /// ten workspace. Zmierzone u właściciela 2026-08-23: uzgodnienie ruszyło przy starcie dla
    /// folderu, który był otwarty, a trzy zombie w sąsiednim projekcie stały dalej.
    ///
    /// Skutek uboczny jest tu równie ważny, co sprzątanie: każdy dotknięty folder wchodzi do
    /// zapadki, więc pierwsze późniejsze dotknięcie GO NIE POWTÓRZY. Bez tego bieg uruchomiony
    /// przez tę sesję w folderze, którego okno nie dotknęło od startu, trafiłby na uzgodnienie
    /// w trakcie własnej pracy — i zostałby spisany na straty jako porzucony przez kogoś innego.
    ///
    /// Nieczytelna lista workspace'ów **nie zabiera okna** (niezmiennik 5): zdanie idzie do
    /// dziennika, a folder, pod którym to okno stoi, jest uzgadniany tak czy owak.
    pub async fn settle_everything_left_behind(&self, home: &std::path::Path) {
        let mut folders = vec![self.project.clone()];
        match crate::commands::workspaces::list_workspaces_inner(home) {
            Ok(known) => folders.extend(known.into_iter().map(|one| PathBuf::from(one.folder))),
            Err(error) => tracing::error!(
                "the list of workspaces could not be read, so only the open folder was settled \
                 after the last window: {error}"
            ),
        }
        for folder in folders {
            self.settle_what_the_last_window_left(&folder).await;
        }
    }

    /// Uzgadnia biegi tego folderu z tym, co naprawde zyje — przy PIERWSZYM dotknieciu w tej sesji.
    ///
    /// TUTAJ, A NIE PRZY STARCIE OKNA, i to jest naprawa zmierzona, nie przeczucie. Do 2026-08-23
    /// odzyskiwanie po awarii czytalo baze BIBLIOTEKI (`lib::recover_from_last_time`), a biegi
    /// folderu maja wlasny indeks i wlasne pliki — wiec tamta droga nie widziala ich nigdy.
    /// Zmierzone u wlasciciela: biblioteka miala 19 biegow i ani jednego `running`, a trzy jego
    /// zombie nie byly w niej wcale.
    ///
    /// NIE PRZY OTWARCIU FOLDERU PRZEZ `workspace::Registry`, choc tam pasowaloby najladniej:
    /// ten rejestr NIE MA ANI JEDNEGO WOLAJACEGO w calym drzewie. Naprawa wpieta w niego byla
    /// wpieta w martwy kod — sprawdzone gerpem po `Registry::open`, zero trafien poza definicja.
    /// `project_for` jest droga zywa: przechodzi przez nia kazda komenda dotykajaca projektu.
    ///
    /// Folder, ktorego nie da sie uzgodnic, dalej jest folderem, z ktorym mozna pracowac: wynik
    /// idzie do dziennika, nigdy w odmowe komendy (niezmiennik 5).
    /// 2026-09 (Z-10) — CAŁE UZGODNIENIE JEDZIE NA PULĘ BLOKUJĄCĄ. `reconcile_runs_keeping`
    /// czyta plik wyborów, obchodzi `runs/`, domyka pozostawione drzewa TĄ SAMĄ `isolate::finish`,
    /// co koniec biegu (`commands::reconcile::close_what_the_runs_left`), i kasuje katalogi
    /// zapomnianych biegów. To jest sekundy pracy gita na folderze, którego nikt nie sprzątał —
    /// a stało w linii na wątku wołającego, czyli na tym, na którym okno czeka na odpowiedź.
    ///
    /// Zamek zapadki zostaje PRZED `await` i nie przeżywa go (niezmiennik 8): sprawdzenie „czy
    /// ten folder już był" jest jedną instrukcją nad `BTreeSet`, a trzymanie go przez całe
    /// uzgodnienie zatrzymywałoby każdą inną komendę dotykającą projektu.
    async fn settle_what_the_last_window_left(&self, project: &std::path::Path) {
        {
            let mut seen = self
                .reconciled
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !seen.insert(project.to_path_buf()) {
                return;
            }
        }
        let home = self.home.clone();
        let at = project.to_path_buf();
        let done = tokio::task::spawn_blocking(move || {
            /* ILE BIEGÓW ZOSTAJE, CZYTAMY Z PLIKU, NIE ZE STANU OKNA (2026-09, Z-9).
             *
             * To jest jedyna droga, którą retencja w ogóle dochodzi do dysku, i jedyna chwila,
             * w której wolno kasować katalogi biegów: sprzątanie biegnie, zanim to okno cokolwiek
             * uruchomi, więc każdy bieg zastany tutaj należy do kogoś, kogo już nie ma.
             *
             * Nieczytelny plik wyborów znaczy „nie kasuj nic" (`Keep::everything`), a nie „kasuj
             * domyślnie": pomyłka w tę stronę kosztuje miejsce na dysku, w drugą — całą historię
             * projektu. */
            let keep = match crate::commands::settings::read_settings_inner(&home) {
                Ok(settings) => {
                    crate::commands::reconcile::Keep::last_runs(settings.keep_last_runs)
                }
                Err(error) => {
                    tracing::error!(
                        "what Loadout does by default could not be read, so no run was forgotten \
                         in {}: {error}",
                        at.display()
                    );
                    crate::commands::reconcile::Keep::everything()
                }
            };
            crate::commands::reconcile::reconcile_runs_keeping(&at, &keep)
        })
        .await;
        // Folder, którego nie da się uzgodnić, dalej jest folderem, z którym można pracować
        // (niezmiennik 5) — także wtedy, gdy pracy nie dokończyła pula blokująca.
        let Ok(done) = done else {
            tracing::error!(
                "the runs left in {} could not be settled, so Loadout is leaving them for the \
                 next time this folder is opened",
                project.display()
            );
            return;
        };
        if done.runs > 0 || done.still_alive > 0 || done.closed > 0 || done.forgotten > 0 {
            tracing::info!(
                "{}: {} run(s) and {} step(s) were left over by a closed window, \
                 {} group(s) proven dead, {} still alive, {} folder(s) closed, {} run(s) forgotten",
                project.display(),
                done.runs,
                done.steps,
                done.reaped,
                done.still_alive,
                done.closed,
                done.forgotten,
            );
        }
    }

    /// Nazwa pliku z okna → żądanie biegu w tym jednym projekcie.
    ///
    /// Zapora i jej cena stoją przy [`run_request`]; tutaj zostaje samo podanie biblioteki,
    /// bo katalog domowy jest jedyną rzeczą, którą stan do tej decyzji wnosi.
    ///
    /// 2026-08-29 (T-164) — PROJEKT PRZYJEŻDŻA ARGUMENTEM, nie z pola [`Self::project`].
    /// Bieg ma szukać pliku dokładnie tam, gdzie pokazała go lista, a listę zakresuje folder
    /// wybrany w oknie albo folder zamrożony w triggerze — nie ten, pod którym wstała aplikacja.
    fn request(
        &self,
        project: &Path,
        file_name: &str,
        how_many_at_once: usize,
        task: Option<String>,
    ) -> Result<RunRequest, String> {
        run_request(
            self.home.as_path(),
            project,
            file_name,
            how_many_at_once,
            task,
        )
    }
}

/// Czy ten uchwyt biegu ma już **dowód zejścia** — czyli czy `settle()` na nim zapadło.
///
/// Pytanie startu, nie Stopu, i cały jego powód stoi przy [`AppState::begin_run`]: uchwyt bez
/// dowodu zejścia jest uchwytem wziętym przez start, którego bieg jeszcze nie skończył, i to
/// jest jedyny stan, w którym drugi start musi odmówić.
///
/// # Dlaczego SONDA, a nie `is_working()` ani nowe pole
///
/// [`RunControl::is_working`] odpowiada na inne pytanie („ruszył i nie zszedł"), a różnica
/// między nim a tym jest właśnie tą szczeliną, przez którą bieg dawał się osierocić. Prostszym
/// zapisem byłoby pole w [`RunControl`], ale ten typ mieszka w cudzym pliku (`AGENTS.md` §7,
/// powód w całości przy [`AppState::begin_run`]) i trzyma `settled` prywatnie — jedynym
/// wejściem do tej odpowiedzi jest więc `wait_until_settled()`.
///
/// Sondujemy tę przyszłość DOKŁADNIE RAZ, budzikiem, który nikogo nie budzi
/// ([`Waker::noop`]): `Poll::Ready` znaczy „dowód już jest", `Poll::Pending` znaczy „jeszcze
/// nie". Wewnątrz jest to `CancellationToken::cancelled()`, więc pojedyncze spojrzenie nic nie
/// konsumuje, na nic nie czeka i po porzuceniu przyszłości nie zostawia po sobie ani zapisu, ani
/// czekającego — to jest odczyt jednego znacznika, tylko wyrażony przez jedyne dostępne drzwi.
///
/// SYNCHRONICZNIE, i to jest wymóg, nie wygoda: to zdanie stoi pod zamkiem na
/// [`AppState::live`], a zamek `std::sync` trzymany przez `await` jest niezmiennikiem 8.
fn proved_down(control: &RunControl) -> bool {
    let mut proof = pin!(control.wait_until_settled());
    proof
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
        .is_ready()
}

/// Czeka na WSZYSTKIE te przyszłości naraz i oddaje ich wyniki w kolejności, w jakiej weszły.
///
/// # Dlaczego to jest napisane tutaj, a nie wzięte ze skrzyni [2026-09, Z-10]
///
/// `join_all` mieszka w `futures-util`, a `tokio` nie ma kombinatora nad **dynamiczną** listą:
/// `join!` zna arność w czasie kompilacji, a `JoinSet` żąda `'static`, czego
/// `stop_before_closing(&self.deps_in(&folder))` nie spełnia i spełnić nie może — [`RunDeps`]
/// pożycza [`AppState`]. Dług był tu zapisany od dawna i w dwóch miejscach: identyczny akapit stoi
/// przy `commands::processes::Processes::close` („wersja równoległa wymaga `FuturesUnordered`,
/// czyli skrzyni `futures`"). Nowa skrzynia w drzewie, które ma 527 z nich i mierzy czas
/// kompilacji w minutach, jest droższa niż te dwadzieścia linii.
///
/// Kolejność wyników jest zachowana, bo o niej mówi wołający: pierwsza porażka w kolejności
/// folderów jest tym, co człowiek czyta po nieudanym zamknięciu okna.
///
/// Odpytuje każdą nierozstrzygniętą przyszłość w każdym obiegu i nie zakłada, która się obudziła.
/// Przy dwóch, trzech folderach jest to tańsze niż rejestr budzeń, a przy stu ich nie ma.
async fn all_at_once<F: Future>(work: Vec<F>) -> Vec<F::Output> {
    let mut pending: Vec<(usize, Pin<Box<F>>)> =
        work.into_iter().map(Box::pin).enumerate().collect();
    let mut done: Vec<(usize, F::Output)> = Vec::with_capacity(pending.len());
    std::future::poll_fn(move |cx| {
        // Wynik odbieramy W TEJ SAMEJ chwili, w której przyszłość go oddała. `poll` wywołany po
        // `Ready` drugi raz jest złamaniem kontraktu, więc nie ma sposobu, żeby wrócić po niego
        // później — przyszłość rozstrzygnięta i porzucona bez odebrania wartości jest wynikiem,
        // którego nikt już nie zobaczy.
        pending.retain_mut(|(at, one)| match one.as_mut().poll(cx) {
            Poll::Ready(value) => {
                done.push((*at, value));
                false
            }
            Poll::Pending => true,
        });
        if pending.is_empty() {
            done.sort_by_key(|(at, _)| *at);
            Poll::Ready(
                std::mem::take(&mut done)
                    .into_iter()
                    .map(|(_, value)| value)
                    .collect(),
            )
        } else {
            Poll::Pending
        }
    })
    .await
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

/// Nazwa pliku z okna → żądanie biegu, liczone tą samą regułą, którą liczy lista.
///
/// 2026-08-17 — do 2026-08-29 stała tu **druga kopia** `commands::workflows::in_library`,
/// świadomie i z nazwaną ceną: tamta funkcja była prywatna, a jej plik nie należał do T-30,
/// więc jedno `pub` w cudzym pliku było pytaniem do człowieka, nie cichym dopiskiem
/// (AGENTS.md §7). Komentarz obiecywał wtedy, że kopie znikną w zadaniu, które posiada oba
/// pliki. T-164 jest tym zadaniem: zapora mieszka wyłącznie
/// w [`commands::workflows::where_it_lives`], a tutaj zostaje samo złożenie żądania.
///
/// Powód, dla którego zapora w ogóle istnieje, jest niezmieniony: nazwa przychodzi z webviewa,
/// więc jest wejściem, któremu nie ufamy (T3 §5.2). `Path::join("../../.ssh/config")` wychodzi
/// poza bibliotekę bez jednego ostrzeżenia, a `join("/etc/x")` odrzuca cały prefiks i zwraca
/// `/etc/x` — czyli Start uruchamiałby plik wskazany przez okno.
///
/// 2026-08-17 — wolna funkcja, a nie ciało metody, z jednego powodu: [`AppState`] niesie
/// [`Store`] i [`Drivers`], więc test tej zapory przez metodę musiałby otworzyć bazę
/// i zbudować fabrykę sterowników, żeby sprawdzić `join` na napisie. Zapora, której koszt
/// sprawdzenia jest wyższy niż koszt napisania, jest zaporą niesprawdzoną — a ta jest jedyną
/// rzeczą między webviewem a `Command::new` w cudzym katalogu.
fn run_request(
    home: &Path,
    project: &Path,
    file_name: &str,
    how_many_at_once: usize,
    task: Option<String>,
) -> Result<RunRequest, String> {
    let placed = commands::workflows::where_it_lives(home, Some(project), file_name)
        .map_err(|error| error.to_string())?;
    Ok(RunRequest {
        workflow: placed.path,
        how_many_at_once,
        task,
        // Zwykły bieg to całe workflow i puste wejście: przekazania powstają w nim samym.
        part: None,
        handoffs_from: None,
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
// 2026-08-16 — WSZYSTKIE BYŁY SYNCHRONICZNE i było to wyborem: Tauri wykonuje komendę bez
// `async` na wątku głównym, więc `review_skill` zamrażało okno na czas pobrania (do 20 s,
// `ingest::FETCH_TIMEOUT_SECONDS`), a `suggest_branch_name` na czas `git for-each-ref` przy
// każdym znaku wpisanym w pole Startu. Lekarstwo — `async fn` plus `spawn_blocking` — kosztuje
// cztery wiersze w skorupie, a mandat tego pliku brzmiał „skorupy dwuliniowe". Dług został więc
// zapisany tutaj i zgłoszony człowiekowi (AGENTS.md §7).
//
// 2026-09 (Z-10) — CZŁOWIEK ODPOWIEDZIAŁ: żadna komenda dotykająca dysku, gita albo sieci nie
// jest już synchroniczna. Skorupa ma teraz trzy czynności zamiast dwóch — rozpakuj, oddaj pracę
// puli blokującej, zwiń odpowiedź — i to jest cała zmiana mandatu. Pilnuje jej
// `tests/it/no_command_freezes_the_window.rs`: nowa komenda bez `async` pali ten test domyślnie,
// bez dopisywania jej gdziekolwiek.
//
// Synchroniczna zostaje dokładnie jedna, `new_id`, i wolno jej to, bo nie czeka na nic poza
// procesorem. Ta sama jedna nazwa stoi w stałej `PURE` w tamtym teście, z tym samym powodem.

/// Zdanie dla człowieka, kiedy praca oddana puli blokującej nie wróciła.
///
/// `JoinError` znaczy tu panikę zadania blokującego (`panic` jest w tym drzewie `deny`) albo
/// runtime w rozbiórce — czyli „bardzo nie tak", a nie zwykłą odmowę, która jedzie własną drogą
/// i własnym zdaniem z rdzenia. Jedno zdanie na wszystkie skorupy, bo jest to jeden fakt:
/// „ta praca się nie dokończyła" (niezmiennik 13). Co się nie dokończyło, mówi `what` — i jest to
/// gotowe angielskie dopełnienie, nie nazwa funkcji.
fn did_not_finish(what: &str, error: &tokio::task::JoinError) -> String {
    format!("Loadout could not finish {what}: {error}")
}

/// Wszyscy zapisani agenci.
#[tauri::command]
pub async fn list_agents() -> Result<Vec<Definition<Agent>>, String> {
    tokio::task::spawn_blocking(|| {
        commands::agents::list_agent_definitions_inner(&crate::loadout_dir())
    })
    .await
    .map_err(|error| did_not_finish("reading the agents you have saved", &error))?
    .map_err(|error| error.to_string())
}

/// Świeży uuid v7 — jedna mennica dla wszystkich sekcji.
#[must_use]
#[tauri::command]
pub fn new_id() -> String {
    commands::mint::new_id_inner().to_string()
}

/// Odczytuje konfigurację wskazanego repo bez uruchamiania znalezionych rozszerzeń.
#[tauri::command]
pub async fn scan_setup(
    workspace: std::path::PathBuf,
) -> Result<crate::import::ImportPreview, String> {
    // Ścieżka wchodzi do domknięcia NA WŁASNOŚĆ, więc `drop`, który stał tu wcześniej po to,
    // żeby argument był naprawdę zużyty (`clippy::needless_pass_by_value`), jest już zbędny.
    tokio::task::spawn_blocking(move || {
        commands::import::scan_setup_inner(&crate::your_home(), &workspace)
    })
    .await
    .map_err(|error| did_not_finish("reading that folder", &error))?
    .map_err(|error| error.to_string())
}

/// Zapisuje ponownie zweryfikowaną migawkę do biblioteki Loadouta.
#[tauri::command]
pub async fn apply_setup(
    request: commands::import::ApplySetup,
) -> Result<crate::import::apply::ImportReceipt, String> {
    tokio::task::spawn_blocking(move || {
        commands::import::apply_setup_inner(&crate::loadout_dir(), &crate::your_home(), &request)
    })
    .await
    .map_err(|error| did_not_finish("saving that setup", &error))?
    .map_err(|error| error.to_string())
}

// ── DWIE KOMENDY PORÓWNANIA KOPII ──────────────────────────────────────────────────────────
//
// 2026-08-29 (T-76) — obie łamią regułę sąsiadów wyżej („wszystkie skorupy importu są
// synchroniczne i nie pamiętają niczego"), i obie z tych samych dwóch powodów, dla których
// łamią ją `draft_skill` i `stop_draft` cztery ekrany niżej.
//
// PIERWSZY: `compare_import_copies` czeka na model, dziesiątki sekund, nie 20 ms. Tauri
// wykonuje komendę bez `async` na wątku głównym, więc synchroniczna zamroziłaby okno na cały
// czas czytania — czyli zamieniłaby drugą opinię w zawieszoną aplikację.
//
// DRUGI: obie mają `State`, choć skorupy importu wyżej biorą katalogi z `crate::loadout_dir()`
// i niczego nie pamiętają. Stop musi sięgnąć do środka porównania, które zaczęła INNA komenda,
// więc uchwyt do niego musi gdzieś mieszkać między wywołaniami.
//
// `stop_comparing_copies` jest `async` mimo tego, że nie ma na co czekać — powód stoi przy
// `stop_draft` i jest zmierzony: wersja synchroniczna nie przechodzi bramki
// (`clippy::needless_pass_by_value` na `State` przyjmowanym wartością), a sugestia clippy
// (`&State<'_, AppState>`) przewraca kryterium szwu po stronie okna.
//
// Dowód zejścia grupy (niezmiennik 6) nie wraca tędy i nie ma tędy wracać: niesie go odpowiedź
// `compare_import_copies`, czyli to samo wywołanie, na które okno już czeka.

/// Jedna pozycja planu → zdania agenta o jej kopiach, przy tej pozycji.
///
/// `None` znaczy „człowiek to zatrzymał" i jest **wartością**, nie odmową (niezmiennik 7):
/// okno ma po niej wygasić „porównuje teraz" i nie pokazywać ani odpowiedzi, ani zdania
/// o awarii.
///
/// Zapisu tu nie ma i nie będzie. Agent doradza, a kopię wybiera człowiek tym, co ten ekran
/// już umie — to ta sama granica, co przy weryfikatorze (AGENTS.md §2).
#[tauri::command]
pub async fn compare_import_copies(
    state: State<'_, AppState>,
    workspace: &str,
    item: &str,
    agent: &str,
) -> Result<Option<crate::import::compare::Comparison>, String> {
    commands::import::compare_copies_inner(
        &crate::loadout_dir(),
        // Katalog domowy CZŁOWIEKA, nie biblioteka Loadouta — ten sam argument, którym czyta
        // projekt `scan_setup`: plan musi wyjść dokładnie taki sam, jak ten na ekranie.
        &crate::your_home(),
        &state.drivers,
        &state.comparing,
        std::path::Path::new(workspace),
        item,
        agent,
    )
    .await
    .map(|outcome| match outcome {
        commands::import::CompareOutcome::Compared(comparison) => Some(comparison),
        commands::import::CompareOutcome::Cancelled => None,
    })
    .inspect_err(|said| {
        refused(said);
    })
}

/// „Stop" dla porównania: zatrzymuje agenta, który czyta kopie.
///
/// Osobna komenda od [`stop_draft`] i od [`stop_run`], bo zatrzymuje osobny uchwyt. Jedna
/// komenda na wszystkie znaczyłaby, że Stop przy wierszu importu ubija bieg w sąsiedniej karcie.
#[tauri::command]
pub async fn stop_comparing_copies(state: State<'_, AppState>) -> Result<(), String> {
    state.comparing.stop();
    Ok(())
}

// ── Lab ─────────────────────────────────────────────────────────────────────────────────────
//
// WSZYSTKIE SĄ `async`, także te bez ani jednego `await` w ciele, i to nie jest ozdoba:
// `State<'_, AppState>` wzięte przez wartość w funkcji synchronicznej jest w tym drzewie
// odmową clippy (`needless_pass_by_value`, pedantic pod `-D warnings`), a referencji
// `generate_handler!` w tym miejscu nie przyjmuje. Ten sam kształt i z tego samego powodu
// mają wszystkie pozostałe komendy tej aplikacji biorące stan — powód jest opisany przy
// `list_handoffs`. Async ma zresztą drugi, samodzielny powód: komenda dotykająca zamka nie
// biegnie wtedy na wątku okna.
//
// Jedenaście krawędzi jednej sekcji. Wszystkie biorą `folder`, bo zestaw jest własnością
// PROJEKTU (`lab::EVALS_DIR`): przypadek zbudowany z materiału tego repozytorium nie znaczy nic
// w sąsiednim, a warstwa, która wzięłaby folder sobie sama z katalogu procesu, pokazywałaby
// zestawy z projektu, na który człowiek akurat nie patrzy.

/// Zestawy tego projektu.
#[tauri::command]
pub async fn list_eval_sets(
    state: State<'_, AppState>,
    folder: Option<String>,
) -> Result<Vec<crate::lab::EvalSet>, String> {
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    tokio::task::spawn_blocking(move || commands::lab::list_sets_inner(&project))
        .await
        .map_err(|error| did_not_finish("reading this project's sets", &error))
}

/// Wszystko, co ekran rysuje dla jednego zestawu: on sam, jego przebiegi i różnica.
#[tauri::command]
pub async fn read_eval_board(
    state: State<'_, AppState>,
    folder: Option<String>,
    set: &str,
    how_many: usize,
) -> Result<commands::lab::BoardWire, String> {
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    let set = set.to_owned();
    tokio::task::spawn_blocking(move || commands::lab::read_board_inner(&project, &set, how_many))
        .await
        .map_err(|error| did_not_finish("reading that set", &error))?
        .map_err(|error| {
            let said = error.to_string();
            refused(&said);
            said
        })
}

/// Zakłada zestaw dla agenta albo umiejętności — to jest cały czasownik „Evaluate".
#[tauri::command]
pub async fn create_eval_set(
    state: State<'_, AppState>,
    folder: Option<String>,
    name: &str,
    subject: crate::lab::Subject,
    agent: &str,
) -> Result<commands::lab::OpenSet, String> {
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    let name = name.to_owned();
    let agent = agent.to_owned();
    tokio::task::spawn_blocking(move || {
        commands::lab::create_set_inner(&project, &name, &subject, &agent)
    })
    .await
    .map_err(|error| did_not_finish("making that set", &error))?
    .map_err(|error| {
        let said = error.to_string();
        refused(&said);
        said
    })
}

/// Usuwa zestaw. Przebiegi zostają w historii biegów.
#[tauri::command]
pub async fn delete_eval_set(
    state: State<'_, AppState>,
    folder: Option<String>,
    set: &str,
) -> Result<(), String> {
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    let set = set.to_owned();
    tokio::task::spawn_blocking(move || commands::lab::delete_set_inner(&project, &set))
        .await
        .map_err(|error| did_not_finish("removing that set", &error))?
        .map_err(|error| {
            let said = error.to_string();
            refused(&said);
            said
        })
}

/// Agent czyta ten projekt i pisze kandydatki, które czekają na człowieka.
#[tauri::command]
pub async fn propose_eval_cases(
    state: State<'_, AppState>,
    folder: Option<String>,
    set: &str,
    agent: &str,
) -> Result<commands::lab::ProposedWire, String> {
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    commands::lab::propose_cases_inner(
        &crate::loadout_dir(),
        &state.drivers,
        &state.proposing,
        &project,
        set,
        agent,
    )
    .await
    .inspect_err(|said| {
        refused(said);
    })
}

/// Agent czyta to, co nie przeszło, i proponuje nowy tekst instrukcji. **Nie stosuje go.**
#[tauri::command]
pub async fn propose_eval_fix(
    state: State<'_, AppState>,
    folder: Option<String>,
    set: &str,
    agent: &str,
) -> Result<commands::lab::FixWire, String> {
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    commands::lab::propose_fix_inner(
        &crate::loadout_dir(),
        &state.drivers,
        &state.proposing,
        &project,
        set,
        agent,
    )
    .await
    .inspect_err(|said| {
        refused(said);
    })
}

/// Stosuje poprawkę: zapisuje nowy tekst instrukcji agenta i oddaje jego nową rewizję.
///
/// `expected_revision` jest tym, co okno przeczytało dla TEGO agenta; bez niego Apply skasowałby
/// cudzą, nowszą zmianę tej samej definicji bez jednego zdania.
#[tauri::command]
pub async fn apply_eval_fix(
    agent: &str,
    instructions: String,
    expected_revision: Option<&str>,
) -> Result<String, String> {
    let agent = agent.to_owned();
    let expected_revision = expected_revision.map(str::to_owned);
    tokio::task::spawn_blocking(move || {
        commands::lab::apply_fix_inner(
            &crate::loadout_dir(),
            &agent,
            instructions,
            expected_revision.as_deref(),
        )
    })
    .await
    .map_err(|error| did_not_finish("applying that fix", &error))?
    .map_err(|error| {
        let said = error.to_string();
        refused(&said);
        said
    })
}

/// „Stop" dla pisania kandydatek.
///
/// Osobna komenda od [`stop_comparing_copies`] i od [`stop_draft`], bo zatrzymuje osobny
/// uchwyt — powód w całości stoi przy [`AppState::proposing`].
#[tauri::command]
pub async fn stop_proposing_cases(state: State<'_, AppState>) -> Result<(), String> {
    state.proposing.stop();
    Ok(())
}

/// Przyjmuje albo odrzuca jedną kandydatkę. To jedyna droga z `suggested` do `in-use`.
#[tauri::command]
pub async fn decide_eval_case(
    state: State<'_, AppState>,
    folder: Option<String>,
    set: &str,
    case: &str,
    keep: bool,
    expected_revision: Option<&str>,
) -> Result<commands::lab::OpenSet, String> {
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    let set = set.to_owned();
    let case = case.to_owned();
    let expected_revision = expected_revision.map(str::to_owned);
    tokio::task::spawn_blocking(move || {
        commands::lab::decide_case_inner(&project, &set, &case, keep, expected_revision.as_deref())
    })
    .await
    .map_err(|error| did_not_finish("writing that decision down", &error))?
    .map_err(|error| {
        let said = error.to_string();
        refused(&said);
        said
    })
}

/// Dopisuje albo poprawia jeden przypadek.
#[tauri::command]
pub async fn put_eval_case(
    state: State<'_, AppState>,
    folder: Option<String>,
    set: &str,
    case: crate::lab::Case,
    expected_revision: Option<&str>,
) -> Result<commands::lab::OpenSet, String> {
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    let set = set.to_owned();
    let expected_revision = expected_revision.map(str::to_owned);
    tokio::task::spawn_blocking(move || {
        commands::lab::put_case_inner(&project, &set, case, expected_revision.as_deref())
    })
    .await
    .map_err(|error| did_not_finish("saving that case", &error))?
    .map_err(|error| {
        let said = error.to_string();
        refused(&said);
        said
    })
}

/// Dopisuje albo poprawia jedną kolumnę.
#[tauri::command]
pub async fn put_eval_variant(
    state: State<'_, AppState>,
    folder: Option<String>,
    set: &str,
    variant: crate::lab::Variant,
    expected_revision: Option<&str>,
) -> Result<commands::lab::OpenSet, String> {
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    let set = set.to_owned();
    let expected_revision = expected_revision.map(str::to_owned);
    tokio::task::spawn_blocking(move || {
        commands::lab::put_variant_inner(&project, &set, variant, expected_revision.as_deref())
    })
    .await
    .map_err(|error| did_not_finish("saving that column", &error))?
    .map_err(|error| {
        let said = error.to_string();
        refused(&said);
        said
    })
}

/// Zdejmuje kolumnę.
#[tauri::command]
pub async fn drop_eval_variant(
    state: State<'_, AppState>,
    folder: Option<String>,
    set: &str,
    variant: &str,
    expected_revision: Option<&str>,
) -> Result<commands::lab::OpenSet, String> {
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    let set = set.to_owned();
    let variant = variant.to_owned();
    let expected_revision = expected_revision.map(str::to_owned);
    tokio::task::spawn_blocking(move || {
        commands::lab::drop_variant_inner(&project, &set, &variant, expected_revision.as_deref())
    })
    .await
    .map_err(|error| did_not_finish("removing that column", &error))?
    .map_err(|error| {
        let said = error.to_string();
        refused(&said);
        said
    })
}

/// Puszcza cały zestaw jako **zwykły bieg**.
///
/// Plan schodzi na dysk obok zestawu, a stąd dalej idzie tą samą drogą, co Start z płótna:
/// ta sama pula miejsc, ten sam sufit wydatku, ten sam strumień linii i ten sam wpis
/// w historii. Osobnej pętli po przypadkach tu nie ma i nie będzie — powód w całości stoi
/// w nagłówku `lab::plan`.
#[tauri::command]
pub async fn run_eval_set(
    state: State<'_, AppState>,
    folder: Option<String>,
    set: &str,
    how_many_at_once: usize,
    budget_usd: Option<f64>,
    lines: Channel<Vec<Line>>,
) -> Result<(), String> {
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    let planned = {
        let at = project.clone();
        let set = set.to_owned();
        tokio::task::spawn_blocking(move || {
            commands::lab::plan_a_run_inner(&at, &set, how_many_at_once)
        })
        .await
        .map_err(|error| did_not_finish("planning that set's run", &error))?
        .map_err(|error| {
            let said = error.to_string();
            refused(&said);
            said
        })?
    };
    run_workflow_in_project(
        &state,
        &project,
        &planned.request,
        budget_usd,
        // Prywatna tura refleksji należy do pracy człowieka, nie do pomiaru: notatka wyciągnięta
        // z przebiegu zestawu opisywałaby przypadki testowe jako wiedzę o projekcie.
        false,
        None,
        pump_into(lines),
    )
    .await
}

/// Zapisuje definicję agenta i oddaje rewizję, którą ma teraz jego plik.
///
/// `expected_revision` jest tym, co okno przeczytało; `null` znaczy „tego pliku ma jeszcze nie
/// być". Zapis, który nie niesie rewizji, kasowałby cudzą, nowszą pracę bez jednego zdania.
#[tauri::command]
pub async fn save_agent(agent: Agent, expected_revision: Option<&str>) -> Result<String, String> {
    let expected_revision = expected_revision.map(str::to_owned);
    tokio::task::spawn_blocking(move || {
        commands::agents::save_agent_inner(
            &crate::loadout_dir(),
            agent,
            expected_revision.as_deref(),
        )
    })
    .await
    .map_err(|error| did_not_finish("saving that agent", &error))?
    .map(|written| written.revision)
    .map_err(|error| error.to_string())
}

/// Usuwa agenta po identyfikatorze, razem z jego plikiem.
#[tauri::command]
pub async fn delete_agent(id: &str) -> Result<(), String> {
    let id = id.to_owned();
    tokio::task::spawn_blocking(move || {
        commands::agents::delete_agent_inner(&crate::loadout_dir(), &id)
    })
    .await
    .map_err(|error| did_not_finish("removing that agent", &error))?
    .map_err(|error| error.to_string())
}

// ── CZTERY KOMENDY BIBLIOTEKI WORKFLOW ─────────────────────────────────────────────────────
//
// 2026-08-29 (T-164) — WSZYSTKIE CZTERY BIORĄ `folder` I `State`, i to jest cała treść tego
// zadania po tej stronie granicy. Do tego dnia workflow leżały wyłącznie w `~/.loadout/
// workflows/`, czyli GLOBALNIE: otwierałeś projekt B i czytałeś bibliotekę projektu A. Folder
// przyjeżdża z `activeWorkspace()` w oknie i przechodzi przez [`AppState::project_for`] — tę
// samą, jedyną odpowiedź na pytanie „który to projekt", którą dostaje Start (niezmiennik 13).
//
// `async`, choć żadna z nich na nic nie czeka: skorupa synchroniczna biorąca `State` wartością
// nie przechodzi bramki (`clippy::needless_pass_by_value`), a sugerowana przez clippy pożyczka
// przewraca kryterium szwu po stronie okna. Ten sam powód i ten sam kształt, co przy
// [`stop_comparing_copies`].

/// Wszystko, co leży na obu półkach workflow tego workspace'a, każdy plik ze swoją nazwą.
#[tauri::command]
pub async fn list_workflows(
    state: State<'_, AppState>,
    folder: Option<String>,
) -> Result<Vec<Definition<commands::workflows::WorkflowEntry>>, String> {
    let project = state.project_for(folder.as_deref()).await?;
    tokio::task::spawn_blocking(move || {
        commands::workflows::list_workflow_definitions_inner(&crate::loadout_dir(), Some(&project))
    })
    .await
    .map_err(|error| did_not_finish("reading your workflows", &error))?
    .map_err(|error| error.to_string())
}

/// Wczytuje jeden plik workflow po jego nazwie w katalogu, razem z rewizją tych bajtów.
#[tauri::command]
pub async fn load_workflow(
    state: State<'_, AppState>,
    file_name: &str,
    folder: Option<String>,
) -> Result<commands::workflows::OpenWorkflow, String> {
    let project = state.project_for(folder.as_deref()).await?;
    let file_name = file_name.to_owned();
    tokio::task::spawn_blocking(move || {
        commands::workflows::load_workflow_inner(&crate::loadout_dir(), Some(&project), &file_name)
    })
    .await
    .map_err(|error| did_not_finish("opening that workflow", &error))?
    .map_err(|error| error.to_string())
}

/// Zapisuje plik workflow i oddaje rewizję, którą ma teraz. Odmowa przyjeżdża własnym zdaniem.
///
/// `expected_revision` jest tym, co okno przeczytało; `null` znaczy „tego pliku ma jeszcze nie
/// być". Zapis bez rewizji cofałby cudzą, nowszą pracę i wyglądałby na udany.
#[tauri::command]
pub async fn save_workflow(
    state: State<'_, AppState>,
    file_name: &str,
    workflow: WorkflowFile,
    expected_revision: Option<&str>,
    folder: Option<String>,
) -> Result<String, String> {
    let project = state.project_for(folder.as_deref()).await?;
    let file_name = file_name.to_owned();
    let expected_revision = expected_revision.map(str::to_owned);
    tokio::task::spawn_blocking(move || {
        commands::workflows::save_workflow_inner(
            &crate::loadout_dir(),
            Some(&project),
            &file_name,
            workflow,
            expected_revision.as_deref(),
        )
    })
    .await
    .map_err(|error| did_not_finish("saving that workflow", &error))?
    .map(|saved| saved.revision)
    .map_err(|error| error.to_string())
}

/// Usuwa plik workflow z tej półki, z której go widać.
#[tauri::command]
pub async fn delete_workflow(
    state: State<'_, AppState>,
    file_name: &str,
    folder: Option<String>,
) -> Result<(), String> {
    let project = state.project_for(folder.as_deref()).await?;
    let file_name = file_name.to_owned();
    tokio::task::spawn_blocking(move || {
        commands::workflows::delete_workflow_inner(
            &crate::loadout_dir(),
            Some(&project),
            &file_name,
        )
    })
    .await
    .map_err(|error| did_not_finish("removing that workflow", &error))?
    .map_err(|error| error.to_string())
}

/// Uwagi walidatora o tym workflow — te same, które padają przy zapisie i przed Startem.
#[tauri::command]
pub async fn check_workflow(workflow: WorkflowFile) -> Vec<Note> {
    match tokio::task::spawn_blocking(move || {
        commands::workflows::check_workflow_inner(&crate::loadout_dir(), workflow)
    })
    .await
    {
        Ok(notes) => notes,
        /* PUSTA LISTA ZNACZY „NIC NIE JEST NIE TAK", więc oddanie jej tutaj byłoby zgodą na
         * Start wydaną przez sprawdzenie, które się nie odbyło. Ta komenda nie zwraca `Result`
         * (uwagi są wartością, nie odmową), więc jedyne uczciwe wyjście to uwaga o tym, że
         * uwag nie policzono — o poziomie `Problem`, bo tak samo brzmi każda inna rzecz, po
         * której nie wolno uruchomić workflow (2026-09, Z-10). */
        Err(error) => vec![Note {
            level: crate::workflow::check::Level::Problem,
            step_id: None,
            message: did_not_finish("checking this workflow", &error),
            fix: None,
        }],
    }
}

/// Adres → pobrana i przejrzana umiejętność.
#[tauri::command]
pub async fn review_skill(url: &str) -> Result<commands::skills::ImportWire, String> {
    // Ta jedna komenda czeka na SIEĆ, do dwudziestu sekund (`ingest::FETCH_TIMEOUT_SECONDS`),
    // i jest powodem, dla którego akapit nad `list_agents` w ogóle powstał.
    let url = url.to_owned();
    tokio::task::spawn_blocking(move || {
        commands::skills::review_skill_inner(&crate::loadout_dir(), &url)
    })
    .await
    .map_err(|error| did_not_finish("fetching that skill", &error))?
    .map_err(|error| error.to_string())
}

/// Trzy pytania z formularza → umiejętność przejrzana tym samym rdzeniem, co wklejony link.
#[tauri::command]
pub async fn author_skill(
    authored: commands::skills::Authored,
) -> Result<commands::skills::ImportWire, String> {
    tokio::task::spawn_blocking(move || {
        commands::skills::author_skill_inner(&crate::loadout_dir(), authored)
    })
    .await
    .map_err(|error| did_not_finish("looking over that skill", &error))?
    .map_err(|error| error.to_string())
}

/// Zapisuje przejrzaną umiejętność w katalogach vendorów wybranego zakresu.
#[tauri::command]
pub async fn install_skill(
    item: commands::skills::ImportWire,
    landing: commands::skills::Landing,
    folder: Option<&str>,
) -> Result<(), String> {
    // Rozbiór, a nie `item.name`: z całego przeglądu Rust bierze WYŁĄCZNIE nazwę, bo bajty do
    // zapisania czyta z kopii kanonicznej — z tych samych, które przeskanował i pokazał
    // człowiekowi. Ten jeden wiersz mówi to wprost i nie da się go przeczytać inaczej.
    let commands::skills::ImportWire { name, .. } = item;
    let folder = folder.map(str::to_owned);
    tokio::task::spawn_blocking(move || {
        install_reviewed_skill(&crate::loadout_dir(), &name, landing, folder.as_deref())
    })
    .await
    .map_err(|error| did_not_finish("adding that skill", &error))?
    .map(|_| ())
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
pub async fn list_skills(
    folder: Option<&str>,
) -> Result<Vec<commands::skills::InstalledWire>, String> {
    // Ten sam sąd nad folderem, co przy zapisie i przy Starcie biegu (`project_folder`).
    // Lista czytana z folderu, którego nie ma, jest pustą listą — czyli zdaniem „nic tam nie
    // leży" o katalogu, o który nikt nie umiał zapytać.
    let project = project_folder(folder)?;
    tokio::task::spawn_blocking(move || {
        commands::skills::list_skills_in(&crate::loadout_dir(), project.as_deref())
    })
    .await
    .map_err(|error| did_not_finish("reading the skills you have added", &error))?
    .map_err(|error| error.to_string())
}

/// Co folder, w którym pracuje ten workspace, ma do pożyczenia krokom.
///
/// # Po co ta komenda w ogóle jest
///
/// `AgentStep::borrow` niesie NAZWY, a nazwy trzeba skądś wziąć. Wiersz wyboru, który zna je
/// tylko z pamięci człowieka, każe mu wpisywać je ręcznie i milczy o literówce aż do odmowy
/// przy Starcie. Ta lista jest jedynym miejscem, w którym okno dowiaduje się, co w tym folderze
/// naprawdę leży — a bez niej wiersz „Borrow from this project" byłby kontrolką bez źródła
/// danych (niezmiennik 16).
///
/// Ten sam sąd nad folderem, co przy [`list_skills`] i przy Starcie biegu ([`project_folder`]).
/// Brak wskazanego folderu to pusta odpowiedź, nie odmowa: człowiek, który nie otworzył jeszcze
/// żadnego projektu, ma zobaczyć wiersz, którego nie ma, a nie zdanie o błędzie.
#[tauri::command]
pub async fn list_host_material(folder: Option<&str>) -> Result<Lendable, String> {
    let Some(project) = project_folder(folder)? else {
        return Ok(Lendable::default());
    };
    // Obchód CUDZEGO repozytorium: tyle katalogów, ile ich tam jest, i ani jednego mniej.
    tokio::task::spawn_blocking(move || crate::inherit::scan::what_this_project_can_lend(&project))
        .await
        .map_err(|error| did_not_finish("looking through that folder", &error))?
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
pub async fn delete_skill(
    name: &str,
    landing: commands::skills::Landing,
    folder: Option<&str>,
) -> Result<(), String> {
    // Ten sam sąd nad folderem, co przy zapisie: zdjęcie „z tego projektu" z folderu, którego
    // nie ma, jest odmową o folderze, a nie o umiejętności — i to jest zdanie, po którym
    // człowiek wie, co zrobić.
    let project = project_folder(folder)?;
    let name = name.to_owned();
    tokio::task::spawn_blocking(move || {
        commands::skills::delete_skill_from(
            &crate::loadout_dir(),
            &name,
            landing,
            project.as_deref(),
        )
    })
    .await
    .map_err(|error| did_not_finish("taking that skill away", &error))?
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

/// Co jeden krok oddał następnemu — jedna paczka biegów tego projektu.
///
/// 2026-08-18 — przekazania są JEDYNĄ drogą, którą wynik kroku dochodzi do promptu następnego
/// (`docs/ARCHITECTURE.md` §8), a do tego dnia okno nie miało jak o nie zapytać: pliki
/// powstawały, `memory::handoff` umiało je przeczytać, i człowiek nie widział z tego ani jednej
/// litery.
///
/// Katalog projektu bierzemy ze stanu, nie z `crate::loadout_dir()`: biegi leżą pod
/// `<projekt>/.loadout/runs/`, a nie w bibliotece.
///
/// 2026-08-23 — ZAKRES Z OKNA, i to jest naprawa, nie rozszerzenie. Ta komenda czytała
/// `state.project`, czyli pole ustawiane RAZ przy starcie na `LOADOUT_PROJECT` albo na
/// `~/.loadout/workspace` (`lib.rs`). Pierwsze nie jest w tym repo ustawiane nigdzie, drugie
/// nie istnieje na dysku — a `run_dirs` na nieistniejącym katalogu oddaje pustą listę BEZ
/// błędu. Trzecia strefa sekcji Pamięć pokazywała więc „Nothing yet…" i nawet nie zapalała
/// odmowy, podczas gdy w folderze wybranym w bocznym menu leżało ponad sto prawdziwych plików.
///
/// `folder` przyjeżdża z okna dokładnie tak, jak w [`list_runs`] i [`copy_diagnostics`], i z tego
/// samego powodu: „gdzie pracujemy" ma w całej aplikacji jedną odpowiedź (niezmiennik 13),
/// a jest nią zakres wybrany w bocznym menu. `None` zostaje jawne, żeby to Rust wziął swoją
/// domyślną, zamiast żeby okno podstawiało drugą.
///
/// Sam komentarz w `src/sections/memory/io.ts` zgłaszał to jako lukę czekającą na człowieka:
/// „`list_handoffs` nie przyjmuje w tej fali zakresu… Zgłoszone człowiekowi".
///
/// 2026-09 (Z-49) — zakres liczy KATALOGI BIEGÓW, nie udane odczyty. Nieczytelny katalog
/// (Z-42) zużywa swoje miejsce w paczce, dzięki czemu jedno wejście nigdy nie sięga po
/// jedenasty bieg tylko dlatego, że w jednym z pierwszych dziesięciu nie ma czego pokazać.
#[tauri::command]
pub async fn list_handoffs(
    state: State<'_, AppState>,
    folder: Option<String>,
    after_runs: usize,
    how_many_runs: usize,
) -> Result<commands::handoffs::HandoffPageWire, String> {
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    tokio::task::spawn_blocking(move || {
        commands::handoffs::list_handoffs_inner(&project, after_runs, how_many_runs)
    })
    .await
    .map_err(|error| did_not_finish("reading what the steps handed on", &error))?
    .map_err(|error| error.to_string())
}

/// Co ten projekt do tej pory uruchomił — biegi leżące w JEGO katalogu, od najnowszego.
///
/// 2026-08-23 — zamówienie właściciela: „powinna być opcja zapisu naszych sesji i wyboru
/// z historii, /history komenda np", z warunkiem „pamiętaj że wszystko ma być per workspace ta
/// historia". Ten warunek jest tu jedną linią i jest całym powodem, dla którego ta komenda ma
/// argument: zakres bierzemy z okna, przez [`AppState::project_for`], dokładnie tak jak
/// [`copy_diagnostics`]. Wersja czytająca katalog procesu pokazywałaby biegi sąsiedniego
/// projektu i nie miałaby jak o tym powiedzieć.
///
/// **Nie oddaje odmowy za nieczytelny bieg.** Katalog, którego `run.json` nie da się przeczytać,
/// wraca jako JEDNA POZYCJA z uczciwym zdaniem (`commands::history`, nagłówek modułu) —
/// odmowa całej listy z powodu jednego ręcznie edytowanego pliku jest tą wersją niezmiennika 5,
/// którą najłatwiej napisać przez przypadek.
#[tauri::command]
pub async fn list_runs(
    state: State<'_, AppState>,
    folder: Option<String>,
) -> Result<Vec<commands::history::RunWire>, String> {
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    tokio::task::spawn_blocking(move || commands::history::list_runs_inner(&project))
        .await
        .map_err(|error| did_not_finish("reading this project's runs", &error))
}

/// Jeden bieg z historii, otwarty DO ODCZYTU: jego kroki, ich strumienie i jego przekazania.
///
/// `run` jest nazwą katalogu z `RunWire::folder`, czyli tym samym napisem, który okno dostało
/// z [`list_runs`]. Sprawdza go warstwa niżej, zanim dotknie dysku: nazwa przyjeżdża z okna,
/// a okno rysuje ją z tego, co ktoś wpisał w wiersz wejścia.
///
/// **Wznowienia tędy nie ma i nie ma być.** Ta komenda czyta pliki i nie dotyka ani jednego
/// żywego uchwytu biegu — dlatego jej `State` służy wyłącznie do rozstrzygnięcia zakresu.
#[tauri::command]
pub async fn read_run(
    state: State<'_, AppState>,
    folder: Option<String>,
    run: String,
) -> Result<commands::history::PastRunWire, String> {
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    tokio::task::spawn_blocking(move || commands::history::read_run_inner(&project, &run))
        .await
        .map_err(|error| did_not_finish("opening that run", &error))?
        .map_err(|error| {
            let said = error.to_string();
            refused(&said);
            said
        })
}

/// Zdejmuje gałęzie, które ten bieg zostawił — i **tylko** jego.
///
/// 2026-08-23 (T-95) — POWSTAŁO Z DRUGIEJ POŁOWY SPRZĄTANIA. Katalog roboczy kroku znika po
/// biegu, bo praca jest osiągalna z gałęzi; gałęzie zostawały natomiast na zawsze i nic nie
/// umiało ich zdjąć poza ręcznym `git branch -D` na każdą z osobna. Po tygodniu pracy `git
/// branch` przestaje być do przeczytania.
///
/// Zakres jedzie argumentem, jak w [`list_runs`] i [`read_run`] — „gdzie pracujemy" ma w całej
/// aplikacji jedną odpowiedź (niezmiennik 13).
///
/// Odmowa jest CAŁOŚCIOWA: kiedy którakolwiek z tych gałęzi jest w tej chwili otwarta do pracy
/// w innym folderze, nie znika ani jedna. Połowa zdjęta i połowa nie byłaby stanem, o którym
/// człowiek dowiaduje się dopiero z `git branch`.
#[tauri::command]
pub async fn forget_run_branches(
    state: State<'_, AppState>,
    folder: Option<String>,
    run: String,
) -> Result<Vec<String>, String> {
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    tokio::task::spawn_blocking(move || {
        commands::history::forget_run_branches_inner(&project, &run)
    })
    .await
    .map_err(|error| did_not_finish("taking that run's branches away", &error))?
    .map_err(|error| {
        let said = error.to_string();
        refused(&said);
        said
    })
}

/// Zdejmuje CAŁY bieg: jego gałęzie i jego katalog. Oddaje nazwy zdjętych gałęzi.
///
/// 2026-09 (Z-9) — DRUGA POŁOWA TEGO, CO ZOSTAWAŁO NA ZAWSZE. Gałęzie biegu dało się zdjąć od
/// T-95 ([`forget_run_branches`]); jego katalog — ze strumieniami agentów, przekazaniami i kopiami
/// notatek — nie schodził **niczym**. Zmierzone u właściciela 2026-09-02: 87 katalogów biegów
/// w jednym projekcie, 3,8 GB, jedyną drogą było `rm -rf` z terminala.
///
/// Ta sama ostrożność, co przy gałęziach, i ta sama całościowość: kiedy którakolwiek gałąź tego
/// biegu jest w tej chwili otwarta do pracy w innym folderze, nie znika ANI JEDNA rzecz — ani
/// gałąź, ani katalog. Powód i kolejność w całości stoją przy `history::forget_run_inner`.
#[tauri::command]
pub async fn forget_run(
    state: State<'_, AppState>,
    folder: Option<String>,
    run: String,
) -> Result<Vec<String>, String> {
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    tokio::task::spawn_blocking(move || commands::history::forget_run_inner(&project, &run))
        .await
        .map_err(|error| did_not_finish("taking that run away", &error))?
        .map_err(|error| {
            let said = error.to_string();
            refused(&said);
            said
        })
}

/// Wszystkie notatki leżące na dysku — lista, którą sekcja Pamięć czyta przy wejściu.
///
/// 2026-08-18 — powstało z tego samego powodu, co [`list_skills`]: magazyn notatek startował
/// pusty i nic w produkcji nie umiało go wypełnić, więc `put_note_to_use` przestawiało status
/// notatki, której sekcja nigdy nie pokazała.
#[tauri::command]
pub async fn list_notes(
    state: State<'_, AppState>,
    catalog_folder: Option<String>,
) -> Result<Vec<commands::memory::NoteWire>, String> {
    let project = state
        .project_for(catalog_folder.as_deref())
        .await
        .inspect_err(refused)?;
    let home = state.home.clone();
    tokio::task::spawn_blocking(move || {
        let library_root = commands::memory::notes_root(&home);
        commands::memory::list_notes_for_project_inner(&library_root, &project)
    })
    .await
    .map_err(|error| did_not_finish("reading your notes", &error))?
    .map_err(|error| error.to_string())
}

/// „Use this": od tej chwili notatka wchodzi do promptu.
#[tauri::command]
pub async fn put_note_to_use(
    state: State<'_, AppState>,
    catalog_folder: Option<String>,
    place: commands::memory::NotePlace,
    id: String,
) -> Result<Vec<commands::memory::NoteWire>, commands::memory::NoteRefusal> {
    let project = state
        .project_for(catalog_folder.as_deref())
        .await
        .map_err(commands::memory::NoteRefusal::Said)?;
    let home = state.home.clone();
    let address = commands::memory::NoteAddress { place, id };
    tokio::task::spawn_blocking(move || {
        let library_root = commands::memory::notes_root(&home);
        commands::memory::put_addressed_note_to_use_inner(
            &library_root,
            &project,
            &address,
            &commands::now_utc(),
        )
    })
    .await
    .map_err(|error| {
        commands::memory::NoteRefusal::Said(did_not_finish("putting that note to use", &error))
    })?
}

/// „Stop using": notatka zostaje na liście i przestaje wchodzić do promptu.
#[tauri::command]
pub async fn stop_using_note(
    state: State<'_, AppState>,
    catalog_folder: Option<String>,
    place: commands::memory::NotePlace,
    id: String,
) -> Result<Vec<commands::memory::NoteWire>, commands::memory::NoteRefusal> {
    let project = state
        .project_for(catalog_folder.as_deref())
        .await
        .map_err(commands::memory::NoteRefusal::Said)?;
    let home = state.home.clone();
    let address = commands::memory::NoteAddress { place, id };
    tokio::task::spawn_blocking(move || {
        let library_root = commands::memory::notes_root(&home);
        commands::memory::stop_using_addressed_note_inner(
            &library_root,
            &project,
            &address,
            &commands::now_utc(),
        )
    })
    .await
    .map_err(|error| {
        commands::memory::NoteRefusal::Said(did_not_finish("setting that note aside", &error))
    })?
}

/// „Discard": kandydatka odchodzi do `discarded/` i schodzi z listy.
///
/// 2026-08-23 (T-92) — druga akcja kandydatki, którą makieta rysuje od początku i której do
/// dziś nie było czym obsłużyć. Bez niej lista rośnie monotonicznie i to jest dokładnie ta
/// nieobsługiwana akrecja instrukcji, którą [T6 §5.1] nazywa samą chorobą.
#[tauri::command]
pub async fn discard_note(
    state: State<'_, AppState>,
    catalog_folder: Option<String>,
    place: commands::memory::NotePlace,
    id: String,
) -> Result<Vec<commands::memory::NoteWire>, commands::memory::NoteRefusal> {
    let project = state
        .project_for(catalog_folder.as_deref())
        .await
        .map_err(commands::memory::NoteRefusal::Said)?;
    let home = state.home.clone();
    let address = commands::memory::NoteAddress { place, id };
    tokio::task::spawn_blocking(move || {
        let library_root = commands::memory::notes_root(&home);
        commands::memory::discard_addressed_note_inner(
            &library_root,
            &project,
            &address,
            &commands::now_utc(),
        )
    })
    .await
    .map_err(|error| {
        commands::memory::NoteRefusal::Said(did_not_finish("discarding that note", &error))
    })?
}

/// Przenosi wcześniejszą notatkę projektową z biblioteki do wybranego projektu.
#[tauri::command]
pub async fn move_note_to_project(
    state: State<'_, AppState>,
    catalog_folder: Option<String>,
    place: commands::memory::NotePlace,
    id: String,
) -> Result<Vec<commands::memory::NoteWire>, commands::memory::NoteRefusal> {
    let project = state
        .project_for(catalog_folder.as_deref())
        .await
        .map_err(commands::memory::NoteRefusal::Said)?;
    let home = state.home.clone();
    let address = commands::memory::NoteAddress { place, id };
    tokio::task::spawn_blocking(move || {
        let library_root = commands::memory::notes_root(&home);
        commands::memory::move_note_to_project_inner(&library_root, &project, &address)
    })
    .await
    .map_err(|error| {
        commands::memory::NoteRefusal::Said(did_not_finish("moving that note", &error))
    })?
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
pub async fn list_workspaces() -> Result<Vec<commands::workspaces::WorkspaceWire>, String> {
    tokio::task::spawn_blocking(|| {
        commands::workspaces::list_workspaces_inner(&crate::loadout_dir())
    })
    .await
    .map_err(|error| did_not_finish("reading your list of folders", &error))?
    .map_err(|error| error.to_string())
}

/// Składa pracę biegu w jedną gałąź pod podaną nazwą.
///
/// Wołane z ekranu, kiedy człowiek o to poprosi. Odmowa (nazwa zajęta, zderzenie dwóch kroków
/// na tym samym pliku) wraca zdaniem, bo to są rzeczy, które człowiek ma przeczytać i
/// rozstrzygnąć, a nie awarie aplikacji.
#[tauri::command]
pub async fn fold_run_into_branch(
    folder: &str,
    run: &str,
    name: &str,
) -> Result<commands::finalize::Landing, String> {
    // Pełny checkout i tyle merge'ów, ile bieg zostawił gałęzi — najdłuższa pojedyncza praca,
    // jaką okno umie zlecić poza samym biegiem.
    let folder = folder.to_owned();
    let run = run.to_owned();
    let name = name.to_owned();
    tokio::task::spawn_blocking(move || {
        commands::finalize::fold_run(std::path::Path::new(&folder), &run, &name)
    })
    .await
    .map_err(|error| did_not_finish("putting that run's work together", &error))?
}

/// Jak nazwie się gałąź wyniku dla podanego identyfikatora zadania.
///
/// Liczone PRZY STARCIE, a nie po biegu: propozycja, której człowiek nie widzi przed
/// naciśnięciem, jest zgadywaniem, którego skutek poznaje po godzinie. Odpowiedź niesie też
/// zmierzoną konwencję i to, czy nazwa jest już zajęta.
#[tauri::command]
pub async fn suggest_branch_name(folder: String, id: String) -> commands::branch_name::Proposed {
    // `git for-each-ref` PRZY KAŻDYM ZNAKU wpisanym w pole Startu — na monorepo z tysiącem
    // gałęzi to była pauza na każdą literę (2026-09, Z-10).
    //
    // ARGUMENTY WARTOŚCIĄ, NIE POŻYCZKĄ, i to jest wymóg Tauri, nie wybór: komenda `async`
    // z pożyczonym argumentem MUSI zwracać `Result` (`AsyncCommandMustReturnResult`), a ta nie
    // zwraca i nie ma zwracać — propozycja nazwy nie jest rzeczą, którą wolno odmówić. Nazwy
    // parametrów zostają te same, więc klucze na drucie i `checks/invoke-args.sh` widzą dokładnie
    // to, co widziały (`src/sections/run/branch/io.ts` wysyła `{ folder, id }`).
    tokio::task::spawn_blocking(move || {
        commands::branch_name::proposed(std::path::Path::new(&folder), &id)
    })
    .await
    /* PUSTA NAZWA, NIE PANIKA I NIE ZGADYWANIE. Ta komenda nie zwraca `Result`, bo propozycja
     * nazwy nie jest rzeczą, którą wolno odmówić — a `unwrap` w kodzie produkcyjnym jest
     * zakazany (AGENTS.md §4). Puste pole `name` ma po drugiej stronie granicy jedno,
     * istniejące znaczenie: „człowiek nie wpisał jeszcze ID", czyli ekran nie pokazuje
     * propozycji. Zmyślona nazwa byłaby gałęzią, na której ktoś zostawiłby pracę. */
    .unwrap_or_else(|_| commands::branch_name::Proposed {
        name: String::new(),
        convention: None,
        taken: false,
    })
}

/// Podpowiedzi ścieżek dla `@` — wyłącznie spod wskazanego folderu.
///
/// `folder` przychodzi ARGUMENTEM, nigdy ze stałej: korzeń zna warstwa wyżej, a literał ze
/// ścieżką repo przewróciłby granicę z niezmiennika 1. `typed` to wszystko, co człowiek napisał
/// po małpce; jego ostatni człon jest PRZEDROSTKIEM nazwy, nie katalogiem.
///
/// Odmowa jest zwykłym `Err`, bo `..` w polu jest omyłką człowieka, a nie awarią aplikacji —
/// ekran ma powiedzieć „tędy nie", a nie zniknąć.
#[tauri::command]
pub async fn suggest_paths(
    folder: &str,
    typed: &str,
) -> Result<Vec<commands::paths::Suggestion>, String> {
    let folder = folder.to_owned();
    let typed = typed.to_owned();
    tokio::task::spawn_blocking(move || {
        commands::paths::suggest(std::path::Path::new(&folder), &typed, commands::paths::MOST)
    })
    .await
    .map_err(|error| did_not_finish("looking for files under that folder", &error))?
    .map_err(|error| error.to_string())
}

/// Dokłada workspace albo zmienia nazwę istniejącego. Oddaje CAŁĄ listę po zapisie.
#[tauri::command]
pub async fn save_workspace(
    name: &str,
    folder: &str,
) -> Result<Vec<commands::workspaces::WorkspaceWire>, String> {
    let name = name.to_owned();
    let folder = folder.to_owned();
    tokio::task::spawn_blocking(move || {
        commands::workspaces::save_workspace_inner(&crate::loadout_dir(), &name, &folder)
    })
    .await
    .map_err(|error| did_not_finish("saving that folder", &error))?
    .map_err(|error| error.to_string())
}

/// Zdejmuje workspace z listy. **Folderu nie dotyka** — powód przy `delete_workspace_inner`.
#[tauri::command]
pub async fn delete_workspace(
    id: &str,
) -> Result<Vec<commands::workspaces::WorkspaceWire>, String> {
    let id = id.to_owned();
    tokio::task::spawn_blocking(move || {
        commands::workspaces::delete_workspace_inner(&crate::loadout_dir(), &id)
    })
    .await
    .map_err(|error| did_not_finish("taking that folder off the list", &error))?
    .map_err(|error| error.to_string())
}

/// Co Loadout robi domyślnie: kto prowadzi rozmowę i ile wolno wydać na jeden bieg.
///
/// Katalog bierzemy z `crate::loadout_dir()`, nie ze stanu, i z tego samego powodu, co przy
/// workspace'ach: te wybory są biblioteką użytkownika, a nie stanem żywego biegu. Powód
/// i zakres w całości stoją w `commands::settings`.
#[tauri::command]
pub async fn read_settings() -> Result<commands::settings::SettingsSnapshot, String> {
    tokio::task::spawn_blocking(|| {
        commands::settings::read_settings_snapshot_inner(&crate::loadout_dir())
    })
    .await
    .map_err(|error| did_not_finish("reading your settings", &error))?
    .map_err(|error| error.to_string())
}

/// Zapisuje wszystkie pięć domyślnych wyborów i oddaje to, co ma teraz plik.
///
/// 2026-08-29 — DWA ARGUMENTY, JEDNO WYWOŁANIE, bo plik jest jeden. Zapis niosący samo wskazanie
/// lidera nadpisywałby sufit tym, co akurat miało okno, a zapis niosący samą kwotę robiłby to
/// samo liderowi (`commands::settings::save_settings_inner`).
///
/// 2026-08-31 — TRZECI ARGUMENT, tą samą drogą i z tego samego powodu: tryb bocznego menu jest
/// wyborem człowieka, a nie stanem okna, więc mieszka w tym samym pliku i jedzie tym samym
/// wywołaniem.
///
/// 2026-09 (Z-9) — CZWARTY ARGUMENT: ile ostatnich biegów zostaje w folderze projektu.
/// 2026-09 (Z-18) — PIĄTY ARGUMENT: czy skończony bieg dostaje prywatną turę refleksji.
/// 2026-09 (Z-32) — SZÓSTY ARGUMENT: rewizja, którą to okno naprawdę przeczytało.
#[tauri::command]
pub async fn save_settings(
    default_lead: &str,
    default_budget_usd: f64,
    nav_collapsed: bool,
    keep_last_runs: u32,
    learn_from_runs: bool,
    expected_revision: Option<&str>,
) -> Result<commands::settings::SettingsSnapshot, String> {
    let default_lead = default_lead.to_owned();
    let expected_revision = expected_revision.map(str::to_owned);
    tokio::task::spawn_blocking(move || {
        commands::settings::save_settings_with_revision_inner(
            &crate::loadout_dir(),
            &default_lead,
            default_budget_usd,
            nav_collapsed,
            keep_last_runs,
            learn_from_runs,
            expected_revision.as_deref(),
        )
    })
    .await
    .map_err(|error| did_not_finish("saving your settings", &error))?
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
#[expect(
    clippy::too_many_arguments,
    reason = "kazdy argument jest osobna odpowiedzia czlowieka udzielona przy tym Starcie, \
              a nie polem struktury do zwiniecia: literal RunRequest stoi w 55 plikach i nie \
              ma Default, wiec nowe pole tam przewraca je wszystkie naraz (T-94)"
)]
pub async fn run_workflow(
    state: State<'_, AppState>,
    file_name: &str,
    how_many_at_once: usize,
    folder: Option<String>,
    task: Option<String>,
    budget_usd: Option<f64>,
    reflection_enabled: bool,
    claim: Option<commands::triggers::TriggerClaim>,
    lines: Channel<Vec<Line>>,
) -> Result<(), String> {
    from_the_window(
        &state,
        file_name,
        how_many_at_once,
        folder.as_deref(),
        task,
        budget_usd,
        reflection_enabled,
        claim.as_ref(),
        pump_into(lines),
    )
    .await
}

/// Jedna produkcyjna krawędź przed wyborem projektu: komenda Tauri i sędzia podają tu te same
/// argumenty. Dzięki temu rozstrzygnięcie claim→workspace nie chowa się w nietestowalnej
/// konstrukcji [`State`] ani nie może wrócić do chwilowego aktywnego folderu okna.
pub async fn run_workflow_from_window(
    state: &AppState,
    file_name: &str,
    how_many_at_once: usize,
    folder: Option<&str>,
    task: Option<String>,
    claim: Option<&commands::triggers::TriggerClaim>,
    lines: LineSink,
) -> Result<(), String> {
    // Bez sufitu wydatku, bo ta droga go nie zna. PODPIS ZOSTAJE, i to nie jest wygoda:
    // `tests/it/trigger_workspace_is_authority.rs` — cudze kryterium — woła tę funkcję siedmioma
    // argumentami, a T-94 tamtego pliku nie posiada (AGENTS.md §7). Sufit jedzie więc
    // ARGUMENTEM przez [`from_the_window`], a nie polem `RunRequest`: literał tamtej struktury
    // stoi w tym drzewie w 55 plikach i nie ma `Default`, więc jedno nowe pole przewróciłoby
    // je wszystkie naraz.
    from_the_window(
        state,
        file_name,
        how_many_at_once,
        folder,
        task,
        None,
        true,
        claim,
        lines,
    )
    .await
}

/// Ta sama krawędź, plus sufit wydatku tego biegu.
#[expect(
    clippy::too_many_arguments,
    reason = "kazdy argument jest osobna odpowiedzia czlowieka udzielona przy tym Starcie; \
              zwiniecie ich w strukture znaczy nowe pole w RunRequest, czyli 55 plikow poza \
              tym zadaniem"
)]
async fn from_the_window(
    state: &AppState,
    file_name: &str,
    how_many_at_once: usize,
    folder: Option<&str>,
    task: Option<String>,
    budget_usd: Option<f64>,
    reflection_enabled: bool,
    claim: Option<&commands::triggers::TriggerClaim>,
    lines: LineSink,
) -> Result<(), String> {
    // PROJEKT PRZED ŻĄDANIEM, od 2026-08-29 (T-164). Nazwa pliku nie mówi już, gdzie ten plik
    // leży: workflow tego projektu przesłania biblioteczny o tej samej nazwie, więc bez folderu
    // nie ma jak złożyć ścieżki. Kolejność jest tu więc wymuszona, a nie estetyczna.
    let project = if let Some(claim) = claim {
        state
            .triggered_project(folder, claim)
            .inspect_err(refused)?
    } else {
        state.project_for(folder).await.inspect_err(refused)?
    };
    let request = state
        .request(&project, file_name, how_many_at_once, task)
        .inspect_err(refused)?;
    run_workflow_in_project(
        state,
        &project,
        &request,
        budget_usd,
        reflection_enabled,
        claim,
        lines,
    )
    .await
}

/// Powtarza JEDEN krok skończonego biegu — jako nowy bieg, z wejściem tamtego.
///
/// 2026-08-23, prośba właściciela: „możemy zrobić restart/re-run danego kroku dowolnego agenta,
/// tego teraz nie ma". Powód i trzy rozstrzygnięcia stoją w całości przy [`commands::rerun`];
/// tutaj zostaje sama krawędź: złóż żądanie, powiedz człowiekowi, jeżeli plik zdążył się
/// zmienić, i puść to tą samą drogą, którą idzie zwykły bieg.
#[tauri::command]
pub async fn rerun_step(
    state: State<'_, AppState>,
    file_name: &str,
    step: &str,
    how_many_at_once: usize,
    folder: Option<String>,
    lines: Channel<Vec<Line>>,
) -> Result<Option<String>, String> {
    // Projekt PRZED żądaniem: bieg, którego krok powtarzamy, leży w katalogu tego workspace'a,
    // więc bez niego nie ma gdzie go szukać.
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    // Powtórzenie kroku czyta plik workflow i opis biegu z dysku, więc jedzie na pulę blokującą
    // (2026-09, Z-10). Sam bieg rusza dopiero po tym, zwykłą drogą.
    let again = {
        let at = project.clone();
        let file_name = file_name.to_owned();
        let step = step.to_owned();
        tokio::task::spawn_blocking(move || {
            commands::rerun::again(
                &crate::loadout_dir(),
                &at,
                &file_name,
                &step,
                how_many_at_once,
            )
        })
        .await
        .map_err(|error| did_not_finish("getting that step ready to run again", &error))?
        .map_err(|error| {
            let said = error.to_string();
            refused(&said);
            said
        })?
    };
    run_workflow_in_project(
        &state,
        &project,
        &again.request,
        None,
        true,
        None,
        pump_into(lines),
    )
    .await?;
    // Zdanie o zmienionym pliku wraca WOŁAJĄCEMU, a nie leci w strumień: strumień należy do
    // biegu, a to jest fakt o tym, co ten bieg w ogóle uruchomił.
    Ok(again.said)
}

/// Wznawia wskazany bieg z historii od wskazanego kroku — on i wszystko po nim.
///
/// 2026-08-23, pytanie właściciela nad ekranem historii: „a z history możemy kontynuować?".
/// Powód i różnica wobec [`rerun_step`] stoją przy [`commands::rerun::onward`]; tutaj zostaje
/// sama krawędź.
///
/// NAZWA NIE BRZMI `continue_run`, bo tamta jest zajęta przez odpowiedź na punkt kontrolny —
/// czyli „idź dalej w BIEGU, który stoi". Ta zaczyna nowy bieg z wejściem starego i dwie
/// komendy o jednej nazwie byłyby parą, którą ktoś kiedyś zamieni miejscami.
///
/// `run` jest NAZWĄ KATALOGU, dokładnie tą, którą historia rysuje w wierszu — a nie ścieżką:
/// ścieżka przysłana z okna byłaby drogą do czytania przekazań spoza tego workspace'a.
#[tauri::command]
pub async fn resume_run(
    state: State<'_, AppState>,
    run: &str,
    step: &str,
    how_many_at_once: usize,
    folder: Option<String>,
    lines: Channel<Vec<Line>>,
) -> Result<Option<String>, String> {
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    // Ten sam odczyt z dysku, co przy [`rerun_step`], i z tego samego powodu na puli blokującej.
    let again = {
        let at = project.clone();
        let run = run.to_owned();
        let step = step.to_owned();
        tokio::task::spawn_blocking(move || {
            commands::rerun::onward(&crate::loadout_dir(), &at, &run, &step, how_many_at_once)
        })
        .await
        .map_err(|error| did_not_finish("getting that run ready to carry on", &error))?
        .map_err(|error| {
            let said = error.to_string();
            refused(&said);
            said
        })?
    };
    run_workflow_in_project(
        &state,
        &project,
        &again.request,
        None,
        true,
        None,
        pump_into(lines),
    )
    .await?;
    Ok(again.said)
}

/// Jedna produkcyjna krawędź wykonania: ręczna bez claimu i triggerowa z trwałym claimem.
async fn run_workflow_in_project(
    state: &AppState,
    project: &Path,
    request: &RunRequest,
    budget_usd: Option<f64>,
    reflection_enabled: bool,
    claim: Option<&commands::triggers::TriggerClaim>,
    lines: LineSink,
) -> Result<(), String> {
    let result = if let Some(claim) = claim {
        // 2026-08-29 — SUFIT JEDZIE TAKŻE TĄ GAŁĘZIĄ. Do tego dnia okno wysyłało `budget_usd`
        // przy każdym Starcie, a bieg z dostawy triggera wbijał tu `None` i leciał bez
        // ograniczenia — czyli dokładnie ten cichy bieg bez sufitu, którego nikt nie zamawiał
        // i o którym nic nie mówiło. Bieg z triggera zaczyna się bez człowieka przy klawiaturze,
        // więc jest tym, który najbardziej potrzebuje granicy, a nie tym, który może jej nie mieć.
        commands::run::run_triggered_workflow_with_budget(
            &state
                .begin_triggered_run(project, claim)
                .inspect_err(refused)?,
            request,
            claim,
            lines,
            budget_usd,
        )
        .await
        .map(|_| ())
    } else {
        commands::run::run_workflow_with_reflection(
            &state.begin_run(project).inspect_err(refused)?,
            request,
            lines,
            budget_usd,
            reflection_enabled,
        )
        .await
        .map(|_| ())
    };
    result.map_err(|error| {
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
    budget_usd: Option<f64>,
    lines: Channel<Vec<Line>>,
) -> Result<(), String> {
    let ask = commands::run::AskRequest {
        agent: agent.to_owned(),
        task: task.to_owned(),
        how_many_at_once,
    };
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    // `begin_a_run`, nie `begin_run`: uchwyt żywego biegu nie ma prawa zniknąć pod Stopem
    // (powód w całości stoi przy tamtej metodzie).
    let deps = state.begin_a_run(project.as_path()).inspect_err(refused)?;
    commands::run::run_agent_with_budget(&deps, &ask, pump_into(lines), budget_usd)
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

/// Stop: zatrzymuje bieg TEGO folderu i wraca **dopiero z dowodem**, że nic po nim nie żyje
/// (niezmiennik 6).
///
/// [`crate::commands::Outcome`] przepada tutaj i nic się z nim nie traci: `stop_run_inner` ma
/// jedną odpowiedź — `Cancelled` — bo bieg z anulowanym tokenem melduje anulowanie także wtedy,
/// gdy ostatni krok zdążył się udać.
///
/// # Co znaczy `folder`, a czego `None` znaczyć NIE MOŻE (2026-09, Z-35)
///
/// `folder` jest **kartą, na której naciśnięto Stop**, i przyjeżdża tą samą wartością, którą to
/// okno wysłało do `run_workflow`. ŻADNA CZYNNOŚĆ Z KARTY NIE WYSYŁA `None` — ani `×`
/// (`src/sections/run/tabs/store.ts`, `stopRunOf`), ani przycisk Stop, ani `/stop`: wszystkie trzy
/// biorą adres z `whereTheRunIs`, a ten przy braku pola oddaje identyfikator karty, nie pustkę.
///
/// Zostaje jeden wołający, który `None` podać może, i jest nim okno BEZ ANI JEDNEGO ZAKRESU:
/// bieg, który takie okno ZASTAJE przy montażu, idzie w katalogu, pod którym wstała aplikacja
/// ([`AppState::project_for`]) — czyli w jedynym folderze, którego okno nie umie wypisać, bo
/// `list_runs` oddaje nazwę katalogu biegu, nie ścieżkę projektu. `None` jest tam ADRESEM tego
/// jednego folderu, nie brakiem adresu, a bez niego zdanie „Stop reaches it.", które ten ekran
/// sam wypowiada, byłoby nieprawdą (niezmiennik 29).
///
/// „BEZ FOLDERU" W ZNACZENIU „KAŻDY ŻYWY" nie istnieje w tej komendzie i istnieć nie ma prawa.
/// Ten zasięg należy do dwóch dróg bez okna i tylko do nich: zamknięcia aplikacji
/// ([`AppState::close_everything_down`] → [`AppState::stop_every_live_run_before_closing`], czyli
/// czerwony guzik i ⌘Q) oraz uzgodnienia po poprzednim oknie
/// ([`AppState::settle_everything_left_behind`]).
#[tauri::command]
pub async fn stop_run(state: State<'_, AppState>, folder: Option<String>) -> Result<bool, String> {
    /* CZY JEST CO ZATRZYMYWAĆ — PYTANIE ODPOWIADANE TUTAJ, I TO JEST CAŁA TA ZMIANA.
     *
     * Zgłoszenie właściciela 2026-08-23, cztery wiersze pod rząd: odmowa „A run is already
     * going… Press Stop first", potem `/stop` → **„Nothing is running."**, potem `/run` →
     * ta sama odmowa, potem `/stop` → to samo zdanie. Bieg pracował przez cały ten czas.
     *
     * Zdanie „nic nie biegnie" mówiło do dziś OKNO, z własnej pamięci: `workflow !== ''`
     * w sesji zakresu. Ta pamięć jest ulotna i bywa nieprawdziwa — gubi ją przeładowanie
     * strony, a do dziś kasował ją także każdy odmówiony start. Zapadka biegu jest natomiast
     * JEDNA NA APLIKACJĘ i mieszka tutaj, więc dwie odpowiedzi na jedno pytanie mogły się
     * rozjechać — i rozjechały się dokładnie tam, gdzie boli: człowiek dostawał odmowę, która
     * każe nacisnąć Stop, i Stop, który twierdzi, że nie ma czego zatrzymywać.
     *
     * Okno pyta teraz zamiast zgadywać (niezmiennik 13). `false` znaczy „nie było czego
     * zatrzymać" i JEST odpowiedzią, nie błędem: naciśnięcie Stopu nad pustym ekranem nie jest
     * pomyłką człowieka.
     *
     * Samo pytanie mieszka w rdzeniu ([`commands::run::stop_if_anything_is_going`]), razem
     * z drugim powodem, dla którego jest konieczne: bez niego Stop nad pustym ekranem wieszałby
     * aplikację. Tutaj zostaje wyłącznie transport (niezmienniki 1 i 23).
     *
     * 2026-09 (Z-35) — BIEG TEGO FOLDERU, NIE KAŻDY ŻYWY. Do tego dnia stało tu
     * `stop_every_live_run` i cena była zmierzona: przy dwóch kartach Stop na jednej kończył
     * bieg na obu. Wybór „o jedno za dużo" był świadomy i miał termin ważności — sięgał, bo ta
     * komenda nie brała folderu, a bieg pozostawiony bez ani jednej drogi z okna jest gorszy niż
     * cudzy bieg zatrzymany. Folder zdejmuje ten wybór z obu stron naraz. */
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    state.stop_the_run_in(&project).await.map_err(|error| {
        let said = error.to_string();
        refused(&said);
        said
    })
}

/// Otwiera strumień rozmowy z liderem TEGO terminalu — sam program jeszcze nie wstaje.
///
/// Dwie komendy, nie jedna, i podział ma nazwany powód: kanał do okna umie zbudować **tylko okno**
/// (`docs/ARCHITECTURE.md` §3), więc musi przyjść argumentem — a rozmowa u dostawcy ma wstać
/// dopiero przy pierwszym zdaniu, bo tura wystartowana przy montażu ekranu jest turą, za którą
/// ktoś płaci, choć nikt o nic nie zapytał. Ta komenda zakłada więc pompę i nic więcej.
///
/// Wołana ponownie PRZEKIEROWUJE istniejącą rozmowę na nowy kanał — nie kończy jej.
///
/// 2026-08-19 — WERSJA PIERWSZA ZAMYKAŁA ROZMOWĘ I BYŁO TO WIDAĆ W DZIENNIKU przy pierwszym
/// uruchomieniu: „the pump for this run closed its books delivered=0", trzy razy pod rząd. Tę
/// komendę woła KAŻDY montaż ekranu pracy, każde przeładowanie okna i — od T-71 — każde
/// przełączenie karty, więc wyjście na Agentów i powrót gubiłoby cały wątek, czyli dokładnie to,
/// po co ta rozmowa istnieje („sobie piszemy/zmieniamy coś itp").
///
/// `terminal` jest tożsamością karty, wybitą w oknie (`src/sections/run/tabs/terminal.ts`).
/// Kiedy w zakresie nie stoi ani jedna karta, okno przysyła tu ścieżkę folderu — czyli folder
/// nazywa DOMYŚLNY terminal tego zakresu, tą samą regułą, którą rejestr trzyma u siebie
/// ([`commands::chat::Threads`]).
#[tauri::command]
pub async fn open_chat(
    state: State<'_, AppState>,
    terminal: &str,
    folder: Option<String>,
    lines: Channel<Vec<Line>>,
) -> Result<(), String> {
    state
        .watching_the_lead(terminal, folder.as_deref(), pump_into(lines))
        .await
}

/// Człowiek odpowiedział na pytanie, które lider zadał w tym terminalu.
///
/// # Po co osobna komenda, a nie `continue_run`
///
/// Bo to są dwa różne pytania i dwie różne drogi powrotne. `continue_run` puszcza dalej BIEG
/// stojący na kafelku kontrolnym; tutaj czeka **tura agenta**, zablokowana na wywołaniu
/// narzędzia, i jej odpowiedź wraca do kontekstu tej samej tury.
///
/// # Dlaczego okno woła to przy KAŻDEJ odpowiedzi
///
/// Bo nie ma jak rozstrzygnąć, do kogo należy przypięte pytanie: w jednym strumieniu stoi
/// pytanie lidera i pytanie kafelka. Rozstrzyga strona, która wie — `false` znaczy „w tym
/// terminalu nikt na to nie czekał" i jest **odpowiedzią, nie błędem**. Okno idzie wtedy swoją
/// dotychczasową drogą.
/* `async` i `Result`, jak każda komenda biorąca `State` w tym pliku: Tauri wymaga wtedy pożyczki
 * z czasem życia, a jednolity kształt oszczędza czytelnikowi pytania „czemu ta jedna jest inna".
 * Ta droga nie ma jak zawieść — `Ok` jest tu formą, nie obietnicą. */
#[tauri::command]
pub async fn answer_the_lead(
    state: State<'_, AppState>,
    terminal: &str,
    agent: &str,
    answer: &str,
) -> Result<bool, String> {
    Ok(state.leads.answer_in(terminal, agent, answer.to_owned()))
}

/// Co wskazany lider naprawdę może zrobić w folderze człowieka.
///
/// Okno pyta o to przy montażu ekranu pracy i po każdej zmianie wskazania, bo z tej odpowiedzi
/// składa zdanie stojące POD polem rozmowy — a to zdanie ma być prawdziwe, zanim ktokolwiek
/// naciśnie Enter (2026-09, Z-50). Odmowa wraca napisem, jak każda odmowa tej granicy.
#[tauri::command]
pub async fn what_the_lead_can_do(
    state: State<'_, AppState>,
    lead: Option<String>,
) -> Result<commands::chat::WhatTheLeadCanDo, String> {
    // PULA BLOKUJACA, NIE WATEK OKNA (2026-09-05). `what_the_lead_can_do_inner` czyta agenta
    // z biblioteki i pyta fabryke o sterownik, czyli dotyka dysku — a `no_command_freezes_the_window`
    // sadzi KAZDA skorupe po tym, czy oddaje taka prace `spawn_blocking`. Ten sam watek niesie Stop,
    // linie biegu w drodze na ekran i kazdy zapis do indeksu (Z-10).
    let home = state.home.clone();
    let drivers = Arc::clone(&state.drivers);
    tokio::task::spawn_blocking(move || {
        commands::chat::what_the_lead_can_do_inner(home.as_path(), &drivers, lead.as_deref())
    })
    .await
    .map_err(|error| did_not_finish("asking what this lead can do", &error))?
    .map_err(|error| {
        let said = error.to_string();
        refused(&said);
        said
    })
}

/// Mówi zdanie liderowi tego terminalu. **Nie uruchamia biegu i nie ma jak** — powód przy
/// `commands::chat`.
///
/// `lead` jest identyfikatorem zapisanego agenta i jego brak jest **odmową nazywającą następny
/// ruch**, nigdy cichym powrotem do zaszytego vendora — powód w całości stoi przy
/// [`AppState::say_to_the_lead`].
#[tauri::command]
pub async fn say_to_orchestrator(
    state: State<'_, AppState>,
    terminal: &str,
    folder: Option<String>,
    lead: Option<String>,
    text: &str,
    images: Option<Vec<PastedImage>>,
) -> Result<(), String> {
    say_to_orchestrator_from_window(
        &state,
        terminal,
        folder.as_deref(),
        lead.as_deref(),
        text,
        images.unwrap_or_default(),
    )
    .await
}

/// Testowalna krawedz produkcyjnej komendy: dekoduje drut okna i dopiero potem wpuszcza
/// zatwierdzone obrazy do rejestru rozmow. Skorupa Tauri nie ma drugiej implementacji tej
/// kolejnosci, wiec kryterium moze osadzic pierwsza i kolejna ture bez budowania `State`.
pub async fn say_to_orchestrator_from_window(
    state: &AppState,
    terminal: &str,
    folder: Option<&str>,
    lead: Option<&str>,
    text: &str,
    images: Vec<PastedImage>,
) -> Result<(), String> {
    let images = validate_pasted_images(images).map_err(|error| {
        let said = error.to_string();
        refused(&said);
        said
    })?;
    state
        .say_to_the_lead_with_images(terminal, folder, lead, text, images)
        .await
}

/// Obraz z webviewa. Nazwy pliku nie ma w typie, wiec nie moze opuscic okna przez przypadek.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PastedImage {
    pub mime: String,
    pub base64: String,
}

impl std::fmt::Debug for PastedImage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PastedImage")
            .field("mime", &self.mime)
            .field(
                "base64",
                &format_args!("<private; {} bytes>", self.base64.len()),
            )
            .finish()
    }
}

fn validate_pasted_images(
    images: Vec<PastedImage>,
) -> Result<ValidatedImages, crate::engine::drivers::ImageError> {
    let images = images
        .into_iter()
        .map(|image| {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(image.base64)
                .map_err(|_error| crate::engine::drivers::ImageError::WrongMagic)?;
            ImageInput::from_wire(&image.mime, bytes)
        })
        .collect::<Result<Vec<_>, _>>()?;
    ValidatedImages::validate(images)
}

/// Kopiuje allowlistowany raport aktywnego workspace i zwraca wyłącznie liczniki.
///
/// `async` utrzymuje wstrzykiwane przez Tauri `AppHandle` i `State` w ich rozpoznawalnym
/// kształcie bez blokowania wątku okna. Referencje sugerowane przez `needless_pass_by_value`
/// zostałyby uznane przez generator sygnatur IPC za argumenty, które ma przesłać webview.
#[tauri::command]
pub async fn copy_diagnostics(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    folder: Option<String>,
) -> Result<commands::diagnostics::DiagnosticsReceipt, String> {
    let workspace = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    // Raport składa się z odczytów dysku (`runs/`, `run.json`, dzienniki) — stąd pula blokująca.
    // Schowek dopisuje się w tym samym domknięciu, bo `AppHandle` wolno używać z każdego wątku.
    tokio::task::spawn_blocking(move || {
        commands::diagnostics::copy_diagnostics_with(&workspace, |report| {
            app.clipboard().write_text(report.to_owned())
        })
    })
    .await
    .map_err(|error| did_not_finish("putting that report together", &error))?
    .map_err(|error| {
        let said = error.to_string();
        refused(&said);
        said
    })
}

/// Karta zamknięta: rozmowa TEGO terminalu schodzi, rozmowy pozostałych zostają.
#[tauri::command]
pub async fn close_terminal(state: State<'_, AppState>, terminal: &str) -> Result<(), String> {
    state.close_the_lead(terminal).await;
    Ok(())
}

/// „Dalej": puszcza bieg TEJ karty zza punktu kontrolnego.
///
/// 2026-09 (Z-35) — `folder` DOSZEDŁ I JEST CAŁĄ NAPRAWĄ. Do tego dnia ta skorupa brała
/// [`AppState::deps`], czyli „uchwyt, który ruszył ostatni", więc odpowiedź na pytanie zadane
/// w folderze A podbijała licznik zgód biegowi w folderze B: pytanie z ekranu zostawało bez
/// odpowiedzi, a cudzy bieg ruszał dalej. Nikt tego nie widzi, bo oba skutki wyglądają jak brak
/// skutku.
#[tauri::command]
pub async fn continue_run(
    state: State<'_, AppState>,
    folder: Option<String>,
    answer: Option<String>,
) -> Result<(), String> {
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    state
        .continue_the_run_in(&project, answer)
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
///
/// 2026-09 (Z-35) — `folder` DOSZEDŁ, bo bez niego „ten jeden, który pracuje" znaczyło
/// „gdziekolwiek": [`AppState::deps`] oddawało uchwyt biegu, który ruszył ostatni, więc zdanie
/// wpisane na karcie A szło do agenta z karty B. Tura, za którą ktoś płaci, trafiała do kogoś
/// innego, niż widać na ekranie.
#[tauri::command]
pub async fn say_to_agent(
    state: State<'_, AppState>,
    folder: Option<String>,
    agent: Option<String>,
    text: &str,
) -> Result<(), String> {
    let project = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;
    state
        .say_in_the_run_at(&project, agent.as_deref(), text)
        .await
        .map_err(|error| {
            let said = error.to_string();
            refused(&said);
            said
        })
}

/* ── RZECZY ZAMÓWIONE KOMENDĄ ────────────────────────────────────────────────────────────────
 *
 * Trzy skorupy i ani jednej więcej. Zgłoszenie właściciela 2026-08-20: „jak napiszę aby coś
 * odpalił jakąś apkę to chcę mieć też po prawej gdzie są agenci info o procesach odpalonych itp,
 * i po kliku mogę tam wejść" — czyli uruchom, pokaż, wejdź. Czwartej drogi (pisanie do tej
 * rzeczy) nie ma, bo nie ma kontrolki, która by je wysyłała: pole w schemacie bez kontrolki
 * w UI jest kontrolką bez handlera (niezmiennik 16).
 *
 * WSZYSTKIE TRZY SĄ `async`, i to nie jest styl. `start_process` MUSI być: `Processes::start`
 * zakłada zadanie opróżniające potoki, a `tokio::spawn` poza runtime'em to panika — Tauri
 * wykonuje skorupę bez `async` na wątku puli, nie w pętli zdarzeń. Dwie pozostałe są `async`,
 * bo biorą `State`: skorupa synchroniczna z tym argumentem przewraca bramkę na
 * `clippy::needless_pass_by_value`, a referencja zamieniłaby `state` w klucz, którego okno
 * ma niby wysłać (`src/sections/ipc-signature.ts`). Cały ten rachunek stoi już raz w tym pliku,
 * przy `stop_draft`.
 */

/// Rzecz uruchomiona komendą — kształt, w którym jedzie do okna.
///
/// Osobno od [`commands::processes::StartedProcess`], i to nie jest przepisanie tamtego typu:
/// tamten jest odpowiedzią REJESTRU i ma trzy pola, bo trzy fakty odpowiadają na pytania
/// kafelka. Drut wiezie o jedno więcej — wyjście — a `Serialize` na tamtym typie kazałby
/// rejestrowi wiedzieć o oknie i zlałby dwa różne pytania w jedno.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StartedWire {
    /// Grupa procesów. Jedyna liczba, którą tę rzecz da się zaadresować, i klucz dla Stopu.
    pub pgid: i32,
    /// Wiersz powłoki, co do znaku. To on jest nazwą tej rzeczy na ekranie.
    pub command: String,
    /// Czy to jeszcze biegnie. `false` znaczy „nie ma kafelka", nie „kafelek na szaro"
    /// (`src/sections/run/rail/processes.ts`).
    pub alive: bool,
    /// Co ta rzecz wypisała — **wyłącznie dla tej jednej, w którą człowiek wszedł**.
    ///
    /// `None` dla pozostałych i to jest wybór o zmierzonej cenie: ogon wyjścia ma sufit 64 KB
    /// (`engine::drivers::command::KEEP_LAST`), a okno odświeża listę raz na sekundę — pięć
    /// rzeczy niosących swój ogon w każdej odpowiedzi to 320 KB na sekundę przez granicę, za
    /// cztery panele, których nikt nie ma otwartych. Otwarty jest zawsze najwyżej jeden, więc
    /// pyta o niego okno, podając jego `pgid`.
    pub said: Option<String>,
}

/// `/start <komenda>`: uruchamia rzecz, która ma **zostać**, i oddaje jej grupę.
///
/// Oddaje `pgid`, bo to jedyna liczba, którą okno może potem tę rzecz zaadresować — i oddaje ją
/// **natychmiast**, bo rzecz żyje po powrocie tego wywołania. To jest cała różnica wobec
/// [`run_workflow`], które trwa tyle, co bieg: kafelek ma stać przez cały czas życia tej rzeczy,
/// a nie zgasnąć w chwili, w której wywołanie wróciło.
#[tauri::command]
pub async fn start_process(
    state: State<'_, AppState>,
    command: &str,
    folder: Option<String>,
) -> Result<i32, String> {
    // Folder tą samą drogą, co przy biegu i przy instalacji umiejętności: `project_for` jest
    // jedynym miejscem, w którym mieszkają te trzy zdania odmowy (niezmiennik 13). Brak wyboru
    // znaczy „tam, gdzie aplikacja wstała", a nie „nigdzie".
    let cwd = state
        .project_for(folder.as_deref())
        .await
        .inspect_err(refused)?;

    let line = command.trim();
    if line.is_empty() {
        /* DRUGA ZAPORA, NIE DRUGA POLITYKA. Wiersz wejścia odmawia sam, zanim cokolwiek pojedzie
         * (`src/sections/run/rail/processes.ts`), i to jest UPRZEDZENIE — ta sama para, co
         * `whereItGoes` wobec `say_to_agent_inner`. Tutaj stoi odmowa dla wywołania z każdej
         * innej strony: `/bin/sh -c ""` wstaje, kończy się w tej samej milisekundzie i zostawia
         * kafelek, który mrugnął. */
        let said = "Write the command after /start, like \"/start npm run dev\".".to_owned();
        refused(&said);
        return Err(said);
    }

    let spec = crate::engine::drivers::command::StartSpec {
        command: line.to_owned(),
        cwd,
    };
    // Podniesienie programu to `fork`/`exec` z całym środowiskiem i grupą procesów — praca
    // systemowa, nie oczekiwanie (2026-09, Z-10). Rejestr jest `Arc`, więc klon jest darmowy.
    let started = Arc::clone(&state.started);
    let what = spec.command.clone();
    tokio::task::spawn_blocking(move || {
        started.start(
            &spec,
            // BEZ ZNACZNIKA, i to jest odpowiedź, nie pominięcie (2026-09, Z-01d): `/start`
            // z wiersza wejścia nie należy do żadnego kroku żadnego biegu, więc odzyskiwanie
            // po awarii nie ma prawa uznać go za swoją sierotę. Człowiek ubija go kafelkiem.
            None,
        )
    })
    .await
    .map_err(|error| did_not_finish("starting that command", &error))?
    .map(|one| one.pgid)
    .map_err(|error| {
        // Zdanie mówi, CO nie wstało, bo `os error 2` samo nie mówi nic (DESIGN §8).
        let said = format!("Loadout could not start \"{what}\": {error}");
        refused(&said);
        said
    })
}

/// „Stop" na kafelku: kończy **tę** grupę i wraca dopiero z dowodem.
///
/// `Ok(())` znaczy tu `ESRCH` dla całej grupy, nigdy „sygnał wyszedł" (niezmiennik 6): ekran,
/// który zgasi kafelek po samym sygnale, kłamie o rzeczy, która dalej pracuje i dalej pali
/// maszynę. Dowodem jest [`crate::engine::supervisor::GroupProof`], a nie kod wyjścia lidera —
/// płacimy za wnuki [T7 §3.1].
///
/// Grupa, której rejestr już nie zna, to `Ok(())`, nie odmowa: rzecz, która zeszła sama między
/// odświeżeniem listy a kliknięciem, nie jest awarią (niezmiennik 7).
#[tauri::command]
pub async fn stop_process(state: State<'_, AppState>, pgid: i32) -> Result<(), String> {
    match state.started.stop(pgid).await {
        None | Some(crate::engine::supervisor::GroupProof::Dead { .. }) => Ok(()),
        Some(crate::engine::supervisor::GroupProof::Alive { .. }) => {
            // Odmowa, nie cisza: bez tej gałęzi okno zdjęłoby kafelek nad czymś, co dalej biegnie,
            // a wtedy jedynym miejscem, w którym da się to zobaczyć, jest Monitor aktywności.
            let said = "Loadout asked it to stop and something in it is still running. Look for \
                        it in Activity Monitor before you start another one."
                .to_owned();
            refused(&said);
            Err(said)
        }
    }
}

/// Zredagowana biblioteka triggerów; uszkodzony plik wraca jako nazwany wpis, nie znika.
#[tauri::command]
pub async fn list_triggers(
    state: State<'_, AppState>,
) -> Result<Vec<commands::triggers::TriggerEntry>, String> {
    let home = state.home.clone();
    tokio::task::spawn_blocking(move || commands::triggers::list(&home))
        .await
        .map_err(|error| format!("Loadout could not finish reading the trigger list: {error}"))?
        .map_err(|error| error.to_string())
}

/// Tworzy trigger z formularza; nazwe pliku wybija Rust, a odpowiedz jest zredagowana.
#[tauri::command]
pub async fn create_trigger(
    state: State<'_, AppState>,
    draft: commands::triggers::TriggerDraft,
) -> Result<commands::triggers::TriggerEntry, String> {
    let home = state.home.clone();
    tokio::task::spawn_blocking(move || commands::triggers::create(&home, draft))
        .await
        .map_err(|error| did_not_finish("saving that trigger", &error))?
        .map_err(|error| error.to_string())
}

/// Zapisuje edycje tylko wtedy, gdy zredagowana migawka nadal opisuje ten sam plik.
#[tauri::command]
pub async fn update_trigger(
    state: State<'_, AppState>,
    slug: String,
    expected: commands::triggers::TriggerSnapshot,
    draft: commands::triggers::TriggerDraft,
) -> Result<commands::triggers::TriggerEntry, String> {
    let home = state.home.clone();
    tokio::task::spawn_blocking(move || commands::triggers::update(&home, &slug, &expected, draft))
        .await
        .map_err(|error| did_not_finish("saving those edits", &error))?
        .map_err(|error| error.to_string())
}

/// Potwierdzone Delete najpierw konczy nieprzyjete dostawy, potem chowa konfiguracje.
#[tauri::command]
pub async fn delete_trigger(
    state: State<'_, AppState>,
    slug: String,
    expected: commands::triggers::TriggerSnapshot,
) -> Result<(), String> {
    let home = state.home.clone();
    tokio::task::spawn_blocking(move || commands::triggers::delete(&home, &slug, &expected))
        .await
        .map_err(|error| format!("Loadout could not finish deleting the trigger: {error}"))?
        .map_err(|error| error.to_string())
}

/// Sprawdza nowy, zastepczy albo zapisany klucz bez odpytania issue i bez zapisu stanu.
#[tauri::command]
pub async fn test_linear_connection(
    state: State<'_, AppState>,
    slug: Option<String>,
    api_key: Option<commands::triggers::Secret>,
) -> Result<(), String> {
    let home = state.home.clone();
    tokio::task::spawn_blocking(move || {
        let key = commands::triggers::connection_key(&home, slug.as_deref(), api_key)?;
        commands::triggers::test_connection(&key)
    })
    .await
    .map_err(|error| format!("Loadout could not finish the Linear connection test: {error}"))?
    .map_err(|error| error.to_string())
}

/// Trwały przełącznik jednego triggera. Sekret nigdy nie przekracza tej granicy.
#[tauri::command]
pub async fn set_trigger_enabled(
    state: State<'_, AppState>,
    slug: String,
    enabled: bool,
) -> Result<commands::triggers::TriggerEntry, String> {
    let home = state.home.clone();
    tokio::task::spawn_blocking(move || commands::triggers::set_enabled(&home, &slug, enabled))
        .await
        .map_err(|error| did_not_finish("writing that switch down", &error))?
        .map_err(|error| error.to_string())
}

/// Pyta jedno źródło o następną sprawę. Sekret i adres zostają w konfiguracji `curl` na stdin;
/// okno wysyła tylko nazwę pliku triggera.
#[tauri::command]
pub async fn check_trigger(
    state: State<'_, AppState>,
    slug: String,
) -> Result<commands::triggers::TriggerPoll, String> {
    // Pozwolenie powstaje przed `await`: zamek żywego biegu jest oddany, zanim ruszy proces,
    // a wariant `busy` nie ma w środku katalogu, z którym dałoby się mimo to wykonać fetch.
    let permit = state.trigger_poll_permit();
    tokio::task::spawn_blocking(move || permit.poll(&slug, unix_millis()))
        .await
        .map_err(|error| format!("Loadout could not finish the Linear check: {error}"))?
        .inspect_err(refused)
}

/// Jawne "Retry" wiersza wstrzymanego: Rust zdejmuje pauzę i pyta źródło jeszcze raz.
#[tauri::command]
pub async fn resume_trigger(
    state: State<'_, AppState>,
    slug: String,
) -> Result<commands::triggers::TriggerPoll, String> {
    let permit = state.trigger_poll_permit();
    tokio::task::spawn_blocking(move || permit.resume(&slug, unix_millis()))
        .await
        .map_err(|error| format!("Loadout could not finish the Linear check: {error}"))?
        .inspect_err(refused)
}

/// Jawne "Run again": Rust wybiera trwala dostawe, a okno dostaje tylko nowy uchwyt Startu.
#[tauri::command]
pub async fn retry_trigger(
    state: State<'_, AppState>,
    slug: String,
) -> Result<commands::triggers::TriggerDelivery, String> {
    let permit = state.trigger_poll_permit();
    tokio::task::spawn_blocking(move || permit.retry(&slug, unix_millis()))
        .await
        .map_err(|error| format!("Loadout could not finish retrying this trigger: {error}"))?
        .inspect_err(refused)
}

/// Milisekundy epoki dla trwałego receipt triggera; zegar webviewa nie bierze udziału.
fn unix_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(i64::MAX)
}

/// Wszystko, czego grupa nie ma jeszcze dowodu śmierci — plus wyjście tej jednej rzeczy, w którą
/// człowiek wszedł. Naturalny reaper usuwa wpis dopiero po `ESRCH`, a okno poznaje ten fakt przy
/// następnym odświeżeniu po braku jego `pgid` (2026-08-31).
///
/// `opened` jest `pgid` rzeczy, której panel jest otwarty, albo `None`. Powód, dla którego wyjście
/// jedzie tylko dla niej, stoi przy [`StartedWire::said`].
///
/// `Result` bez ani jednej gałęzi `Err` i nie jest to nasz wybór: Tauri odrzuca na kompilacji
/// skorupę `async`, która bierze `State` i nie zwraca `Result` („async commands that contain
/// references as inputs must return a `Result`"). Odczyt rejestru nie ma jak zawieść — bierze
/// zamek i przepisuje trzy pola — a `async` jest tu wymuszone tym samym `State` (powód
/// w akapicie nad tą trójką).
#[tauri::command]
pub async fn list_processes(
    state: State<'_, AppState>,
    opened: Option<i32>,
) -> Result<Vec<StartedWire>, String> {
    Ok(state
        .started
        .list()
        .into_iter()
        .map(|one| StartedWire {
            said: opened
                .filter(|pgid| *pgid == one.pgid)
                .and_then(|pgid| state.started.said(pgid)),
            pgid: one.pgid,
            command: one.command,
            alive: one.alive,
        })
        .collect())
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
        answer_the_lead,
        apply_eval_fix,
        apply_setup,
        author_skill,
        check_trigger,
        check_workflow,
        close_terminal,
        compare_import_copies,
        continue_run,
        copy_diagnostics,
        create_eval_set,
        create_trigger,
        decide_eval_case,
        delete_agent,
        delete_eval_set,
        delete_skill,
        delete_trigger,
        delete_workflow,
        delete_workspace,
        discard_note,
        draft_skill,
        drop_eval_variant,
        forget_run_branches,
        forget_run,
        install_skill,
        list_agents,
        list_eval_sets,
        list_handoffs,
        list_host_material,
        list_notes,
        list_processes,
        list_runs,
        list_skills,
        list_triggers,
        list_workflows,
        fold_run_into_branch,
        list_workspaces,
        suggest_branch_name,
        suggest_paths,
        load_workflow,
        move_note_to_project,
        new_id,
        open_chat,
        propose_eval_cases,
        propose_eval_fix,
        put_eval_case,
        put_eval_variant,
        put_note_to_use,
        read_eval_board,
        read_run,
        read_settings,
        rerun_step,
        resume_run,
        resume_trigger,
        retry_trigger,
        review_skill,
        run_agent,
        run_eval_set,
        run_workflow,
        save_agent,
        save_settings,
        save_workflow,
        save_workspace,
        say_to_agent,
        say_to_orchestrator,
        scan_setup,
        set_trigger_enabled,
        start_process,
        stop_comparing_copies,
        stop_draft,
        stop_process,
        stop_proposing_cases,
        stop_run,
        stop_using_note,
        test_linear_connection,
        update_trigger,
        what_the_lead_can_do,
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

    use super::run_request;

    /// Katalog biblioteki. Nie istnieje na dysku i istnieć nie musi: `run_request` decyduje
    /// o napisie, nie o pliku — sprawdzanie obecności pliku należy do biegu, który go wczyta.
    const HOME: &str = "/loadout-home";

    /// Folder projektu. Też nie istnieje, i to jest teraz częścią pomiaru: kiedy pliku nie ma
    /// ani w projekcie, ani w bibliotece, `where_it_lives` mówi „nowy plik w projekcie".
    const PROJECT: &str = "/loadout-project";

    /// To, co z żądania da się porównać: gdzie bieg pójdzie i ile ma robić naraz.
    ///
    /// [`super::RunRequest`] nie jest `PartialEq`, a jego plik nie należy do T-30 — więc
    /// zamiast `derive` w cudzym pliku stoi tu rozbiór na dwa pola, które ta zapora ustawia.
    fn requested(file_name: &str, how_many_at_once: usize) -> Result<(PathBuf, usize), String> {
        // Bez zadania: ta zapora sądzi NAZWĘ PLIKU, a zadanie z wiersza nie ma na nią wpływu.
        run_request(
            Path::new(HOME),
            Path::new(PROJECT),
            file_name,
            how_many_at_once,
            None,
        )
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

    /// Kontrola dodatnia: zwykła nazwa przechodzi i ląduje **w katalogu tego projektu**.
    ///
    /// Bez tego przypadku obie odmowy wyżej świecą na zielono także dla zapory, która nie
    /// przepuszcza niczego — czyli dla Startu, który nigdy nic nie uruchamia.
    ///
    /// 2026-08-29 (T-164) — porównanie zmieniło korzeń i to jest właśnie ta zmiana: nazwa,
    /// której nie ma na żadnej półce, jest plikiem TEGO workspace'a.
    #[test]
    fn a_plain_file_name_passes_and_stays_inside_the_open_workspace() {
        assert_eq!(
            requested("nightly-review.yaml", 3),
            Ok((
                crate::commands::workflows::project_workflows(Path::new(PROJECT))
                    .join("nightly-review.yaml"),
                3
            ))
        );
    }
}
