//! Warstwa komend: co się dzieje, kiedy człowiek naciśnie Start, Stop albo Continue.
//!
//! **W tym katalogu nie ma ani jednego `#[tauri::command]` i ani jednego `use tauri::`.**
//! `docs/ARCHITECTURE.md` §3 daje słowo „Tauri" wyłącznie plikowi `ipc.rs`, a mapa własności daje
//! ten katalog zadaniu T-15. Godzimy to tak: tutaj mieszkają **wyłącznie** funkcje `*_inner`
//! biorące [`RunDeps`], a dwuliniowe skorupy `#[tauri::command]` i jedna lista
//! `generate_handler!` należą do T-07. Powód jest testowy, nie estetyczny: `State<'_, AppState>`
//! nie da się zbudować w teście jednostkowym, a `&RunDeps` da się [04 §2.1].
//!
//! 2026-08-16 — zdanie wyżej mówiło „należą do T-07". T-07 wylądował z ośmioma zielonymi
//! kryteriami o pompie i **bez ani jednej skorupy**, bo żadne kryterium nie sięgało szwu:
//! `Failed to launch` jest na liście `NOT_A_REAL_RED`, więc nic, co wymaga żywego Tauri, nie
//! może być kryterium. Adresatem jest T-27 i tam ten dług jest spłacany razem z dowodem, który
//! nie potrzebuje okna: `src-tauri/commands.golden.txt` czytany z obu stron granicy.
//!
//! # Co gdzie mieszka
//!
//! Ten plik to **typy i uchwyty**: [`RunDeps`], [`RunRequest`], [`RunReport`], [`RunError`]
//! i [`RunControl`] — czyli wszystko, czym woła się bieg i czym sięga się do niego w trakcie.
//! Same trzy funkcje biegu (`run_workflow_inner`, `stop_run_inner`, `continue_run_inner`)
//! siedzą w [`run`], razem z całym zapisem `run.json`.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::engine::drivers::AgentDriver;
use crate::engine::step::StepState;
use crate::library::agents::{AgentError, Vendor};
use crate::store::{Store, StoreError};
use crate::workflow::check::Note;
use crate::workflow::file::LoadError;

/// Biblioteka agentów: wypisz, zapisz, usuń. Wypełnia T-27.
pub mod agents;
/// Pamięć: weź notatkę do użytku i przestań jej używać. Wypełnia T-27.
pub mod memory;
/// Mennica identyfikatorów uuid v7 — jedna dla wszystkich sekcji. Wypełnia T-27.
pub mod mint;
pub mod run;
/// Umiejętności: przeczytaj link, zainstaluj przejrzane. Wypełnia T-27.
pub mod skills;
/// Pliki workflow: wczytaj, zapisz, sprawdź. Wypełnia T-27.
pub mod workflows;

/// Chwila **teraz** w ISO 8601 UTC — to, co `memory::notes::Actor::You` nazywa `at`.
///
/// Zegar stoi tutaj, w warstwie komend, bo `memory::notes` go świadomie nie ma: `at` opisuje
/// chwilę, w której **człowiek** kliknął, a moduł, który sam czyta zegar, nie da się przetestować
/// bez czekania. Okno też go nie podaje — front, który stempluje czas zapisu, stempluje czas
/// SWOJEGO zegara, a plik ma nieść jeden.
///
/// 2026-08-16 — algorytm dni→data (proleptyczny kalendarz gregoriański, era 400-letnia) stoi
/// w tym drzewie trzeci raz, obok `memory::handoff::now_utc` i `commands::run::stamp`. To nie
/// jest przeoczenie: tamta pierwsza jest **prywatna** w pliku, który nie należy do tego zadania,
/// a `chrono`/`time` odpadają, bo `src-tauri/Cargo.toml` też nie jest nasz (AGENTS.md §7). Trzy
/// kopie jednego rachunku to jest rzecz do zgłoszenia człowiekowi, nie do rozstrzygnięcia po
/// cichu w cudzym pliku.
#[must_use]
pub fn now_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());

    let (days, rest) = (secs / 86_400, secs % 86_400);
    let (hour, minute, second) = (rest / 3600, (rest % 3600) / 60, rest % 60);

    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Skąd bieg bierze sterownik dla vendora, którym biegnie agent kroku.
///
/// Funkcja, nie mapa: vendorów jest dwóch od pierwszego dnia (decyzja D3), a trzeci ma wejść bez
/// wydania Loadouta. Uchwyt jest `Arc`iem, bo zadanie każdego kroku dostaje własny klon — planista
/// wymaga od domknięcia `'static` (`engine::scheduler::execute`), więc pożyczka tu nie przejdzie.
pub type Drivers = Arc<dyn Fn(Vendor) -> Arc<dyn AgentDriver> + Send + Sync>;

/// Współpracownicy **jednego** biegu.
///
/// `RunDeps` zamiast globalnego `AppState` i to jest cała różnica między kryterium, które da się
/// napisać, a kryterium, które potrzebuje okna: `State<'_, AppState>` nie da się zbudować
/// w teście, a tę strukturę da się w sześciu wierszach. `AppState` po stronie Tauri (T-01/T-07)
/// tylko ją składa.
pub struct RunDeps<'a> {
    /// `~/.loadout` — biblioteka użytkownika: `agents/`, `workflows/`, `skills/`
    /// (`docs/ARCHITECTURE.md` §8). Przychodzi **argumentem**, nigdy z `HOME` czytanego w środku:
    /// katalog domowy odczytany tutaj znaczyłby, że każdy test pisze do prawdziwej biblioteki.
    pub home: &'a Path,
    /// Katalog projektu, w którym biegnie workflow. To pod nim ląduje
    /// `.loadout/runs/<ts>__<id>/`.
    pub project: &'a Path,
    /// Indeks biegu. **Nie jest prawdą** (niezmiennik 4): wszystko, co tu wchodzi, musi dać się
    /// odtworzyć z `run.json` i `logs/`, bo `loadout.db` wolno skasować.
    pub store: &'a Store,
    /// Fabryka sterowników. Uchwyt, nie pożyczka — patrz [`Drivers`].
    pub drivers: Drivers,
    /// Uchwyt do tego biegu: Stop i Continue sięgają nim do środka.
    ///
    /// 2026-08-16 — `TASK.md` wymienia w tym miejscu `CancellationToken` i on tu jest, wewnątrz
    /// [`RunControl`] (`RunControl::cancel_token`). Osobny typ, bo token umie powiedzieć
    /// dokładnie jedno słowo — „stop" — a punkt kontrolny potrzebuje drugiego: „dalej"
    /// (T3 §6.1 reguła 5). Dwa tokeny obok siebie w tej strukturze byłyby tym samym typem
    /// z dwoma znaczeniami, czyli parą, którą prędzej czy później ktoś zamieni miejscami.
    pub control: RunControl,
}

impl fmt::Debug for RunDeps<'_> {
    /// Ręcznie, bo [`Drivers`] jest domknięciem i `Debug` nie ma dla niego nic do powiedzenia.
    /// `missing_debug_implementations` jest w `Cargo.toml` ostrzeżeniem, a bramka woła clippy
    /// z `-D warnings`, więc „ta struktura po prostu nie ma `Debug`" nie jest tu wyjściem.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RunDeps")
            .field("home", &self.home)
            .field("project", &self.project)
            .field("drivers", &"<factory>")
            .field("control", &self.control)
            .finish_non_exhaustive()
    }
}

/// Uchwyt do żywego biegu — to, czym Stop i Continue sięgają do środka.
///
/// Klon dzieli te same sygnały; jeden bieg, jeden uchwyt, dowolnie wiele klonów.
#[derive(Clone, Debug)]
pub struct RunControl {
    inner: Arc<Signals>,
}

/// Trzy sygnały jednego biegu.
#[derive(Debug)]
struct Signals {
    /// Token **tego** biegu, nigdy globalny `AtomicBool` (niezmiennik 7): bool przecieka między
    /// biegami, więc drugi bieg po anulowanym startuje jako już anulowany i kończy się
    /// w milisekundach z samymi `Cancelled` — co wygląda jak szybki bieg, a nie jak awaria.
    cancel: CancellationToken,
    /// Ile razy człowiek powiedział „dalej". **Licznik, nie flaga**: bieg z dwoma punktami
    /// kontrolnymi przeszedłby przez drugi bez pytania, gdyby zgoda była flagą, która raz
    /// zapalona zostaje zapalona. Pytanie, które nie pyta, jest gorsze od jego braku.
    go_on: watch::Sender<u64>,
    /// Czy bieg **stoi** na punkcie kontrolnym.
    ///
    /// Tu jest właściciel tego faktu; `"status": "paused"` w `run.json` jest jego trwałym
    /// lustrem, bo stan, który nie dociera na dysk, nie przeżywa awarii aplikacji
    /// (niezmiennik 4). `paused` jest stanem **biegu** i nigdy stanem kroku
    /// (`docs/ARCHITECTURE.md` §5).
    paused: watch::Sender<bool>,
    /// Zapalane, kiedy bieg naprawdę zszedł — po ostatnim kroku, nie po wysłaniu Stopu.
    /// Bez tego `stop_run_inner` mówiłby „zatrzymane" w chwili, w której wysłał sygnał
    /// (niezmiennik 6: dopóki nie ma dowodu, traktujemy jako żywe).
    settled: CancellationToken,
}

impl RunControl {
    /// Świeży uchwyt: bieg jeszcze nie ruszył, nikt go nie zatrzymał i nikt nie powiedział
    /// „dalej".
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Signals {
                cancel: CancellationToken::new(),
                go_on: watch::Sender::new(0),
                paused: watch::Sender::new(false),
                settled: CancellationToken::new(),
            }),
        }
    }

    /// Token anulowania **tego** biegu. Klon dostaje planista i klon dostaje każdy krok —
    /// do środka, nie obok: zdjęcie zadania Rusta z zewnątrz zostawia żywy proces palący limit
    /// u dostawcy [T7 §3.1].
    #[must_use]
    pub fn cancel_token(&self) -> CancellationToken {
        self.inner.cancel.clone()
    }

    /// Człowiek nacisnął Stop.
    pub fn stop(&self) {
        self.inner.cancel.cancel();
    }

    /// Człowiek nacisnął Continue przy punkcie kontrolnym.
    pub fn go_on(&self) {
        self.inner.go_on.send_modify(|times| *times += 1);
    }

    /// Zakłada nasłuch na „dalej" i oddaje go wołającemu, **zanim** bieg ogłosi, że stoi.
    ///
    /// 2026-08-16 — to nie jest wariant [`RunControl::wait_for_go_on`] dla wygody, tylko jedyny
    /// kształt, w którym punkt kontrolny nie ma wyścigu. Pauza staje się widoczna przez
    /// `run.json` na dysku, a Continue przychodzi z zewnątrz w reakcji na to, co widać —
    /// więc kolejność „zapisz pauzę, potem zacznij słuchać" ma okno, w którym odpowiedź
    /// człowieka trafia do nikogo. Licznik podbity w tym oknie nie budzi nikogo, bo świeża
    /// subskrypcja `watch` liczy dopiero **następną** zmianę, i bieg stoi już do końca świata.
    ///
    /// Kolejność, która działa, jest odwrotna i egzekwuje ją typ: `GoOn` istnieje **przed**
    /// zapisem pauzy, bo bez tej wartości nie ma na czym czekać.
    #[must_use]
    pub fn listen_for_go_on(&self) -> GoOn {
        let mut told = self.inner.go_on.subscribe();
        // Liczba zapamiętana TERAZ jest tym, co odróżnia zgodę na **ten** punkt kontrolny
        // od zgody sprzed dziesięciu minut.
        let before = *told.borrow_and_update();
        GoOn {
            told,
            before,
            cancel: self.inner.cancel.clone(),
        }
    }

    /// Czeka, aż ktoś powie „dalej" **albo** zatrzyma bieg. Wraca `true`, kiedy padło „dalej".
    ///
    /// Nasłuch zaczyna się dopiero tutaj, więc ta droga jest dobra tam, gdzie nikt nie zdąży
    /// odpowiedzieć wcześniej, niż zaczniemy słuchać. Punkt kontrolny bierze
    /// [`RunControl::listen_for_go_on`] i powód stoi przy nim.
    pub async fn wait_for_go_on(&self) -> bool {
        self.listen_for_go_on().wait().await
    }

    /// Bieg stanął na punkcie kontrolnym i czeka na człowieka.
    pub fn pause(&self) {
        self.inner.paused.send_replace(true);
    }

    /// Bieg rusza dalej: pytanie ma odpowiedź albo przestało mieć znaczenie.
    pub fn resume(&self) {
        self.inner.paused.send_replace(false);
    }

    /// Czeka, aż bieg przestanie stać na punkcie kontrolnym.
    ///
    /// Wraca **od razu**, kiedy bieg nie stoi, i wraca też wtedy, gdy bieg zszedł — bo inaczej
    /// Continue naciśnięte w biegu bez pytania wisiałoby do końca świata, a przycisk, który
    /// zawiesza okno, jest gorszy od przycisku, który nic nie robi.
    pub async fn wait_until_moving(&self) {
        let mut paused = self.inner.paused.subscribe();
        loop {
            if !*paused.borrow_and_update() || self.inner.settled.is_cancelled() {
                return;
            }
            tokio::select! {
                biased;
                () = self.inner.settled.cancelled() => return,
                changed = paused.changed() => {
                    if changed.is_err() {
                        // Nadawca zginął razem z biegiem; nie ma na co czekać.
                        return;
                    }
                }
            }
        }
    }

    /// Bieg zszedł: wszystkie kroki są rozstrzygnięte i nic po nim nie żyje.
    pub fn settle(&self) {
        self.inner.settled.cancel();
    }

    /// Czeka na dowód z [`RunControl::settle`].
    pub async fn wait_until_settled(&self) {
        self.inner.settled.cancelled().await;
    }
}

impl Default for RunControl {
    fn default() -> Self {
        Self::new()
    }
}

/// Założony nasłuch na „dalej" — wartość, którą punkt kontrolny trzyma w ręku, zanim ogłosi
/// pauzę. Powód, dla którego to jest osobny typ, stoi przy [`RunControl::listen_for_go_on`].
#[derive(Debug)]
pub struct GoOn {
    /// Licznik zgód, obserwowany od chwili założenia nasłuchu.
    told: watch::Receiver<u64>,
    /// Ile razy padło „dalej", zanim ten punkt kontrolny zaczął słuchać.
    before: u64,
    /// Ten sam token, którym Stop kończy bieg: pytanie bez odpowiedzi musi dać się zamknąć.
    cancel: CancellationToken,
}

impl GoOn {
    /// Czeka, aż ktoś powie „dalej" **albo** zatrzyma bieg. Wraca `true`, kiedy padło „dalej".
    ///
    /// Bierze `self` przez wartość, bo nasłuch odpowiada na **jedno** pytanie: nasłuch użyty
    /// drugi raz odpowiadałby na drugie pytanie zgodą wydaną na pierwsze.
    pub async fn wait(mut self) -> bool {
        loop {
            if self.cancel.is_cancelled() {
                return false;
            }
            if *self.told.borrow_and_update() > self.before {
                return true;
            }
            tokio::select! {
                biased;
                () = self.cancel.cancelled() => return false,
                changed = self.told.changed() => {
                    if changed.is_err() {
                        // Nadawca zginął razem z biegiem; nie ma na co czekać.
                        return false;
                    }
                }
            }
        }
    }
}

/// Żądanie z interfejsu: co uruchomić i ile naraz.
#[derive(Debug, Clone)]
pub struct RunRequest {
    /// Plik workflow — **pełna ścieżka**, nie slug. Bieg nie ufa UI (T3 §5.2): ten plik mógł
    /// zostać zmergowany gitem albo poprawiony ręcznie między zapisem a naciśnięciem Start,
    /// więc jedyne, co o nim wiadomo na pewno, to gdzie leży.
    pub workflow: PathBuf,
    /// Ile kroków ma **naprawdę** działać naraz.
    ///
    /// Liczba przychodzi w żądaniu, nigdy ze stałej w kodzie (niezmiennik 11). Cicha wersja
    /// złamania nie wygląda jak zły algorytm — wygląda jak pole, które jest wczytywane,
    /// logowane i nigdzie nie podawane, a semafor dostaje `1`. Tak przegrał poprzedni prototyp.
    pub how_many_at_once: usize,
}

/// Czym skończył się bieg.
///
/// **Wartość, nie `Err`** (niezmiennik 7): anulowanie jest jednym z normalnych zakończeń,
/// a `Err(Cancelled)` zmusza każdego wołającego do rozróżniania „to się nie udało" od „to
/// zatrzymał człowiek" — rozróżnienie zgubione raz jest zgubione wszędzie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Bieg doszedł do końca sam.
    Done,
    /// Bieg zatrzymał człowiek.
    Cancelled,
}

/// Co bieg zostawił po sobie wołającemu.
///
/// Wszystko, co tu stoi, stoi też w `run.json` — to nie jest duplikat, tylko dwa czasy: ta
/// struktura odpowiada wołającemu **teraz**, a plik odpowiada za tydzień, po skasowaniu bazy
/// (niezmiennik 4).
#[derive(Debug, Clone)]
pub struct RunReport {
    /// uuid v7 biegu — sortuje się po czasie.
    pub id: String,
    /// `<projekt>/.loadout/runs/<ts>__<id>/`.
    pub dir: PathBuf,
    /// Czym się skończył.
    pub outcome: Outcome,
    /// Stan końcowy każdego kroku, **w kolejności z pliku workflow**. Po powrocie nie ma tu
    /// prawa zostać `Pending`, `Ready` ani `Running`.
    pub steps: Vec<StepState>,
}

/// Czym bieg umie odmówić.
///
/// Każdy wariant jest osobnym zdaniem dla użytkownika, bo każdy naprawia się inaczej.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    /// [`crate::workflow::check`] znalazło problem. **Nic nie ruszyło** — ani jeden proces,
    /// ani jeden katalog.
    ///
    /// Zdanie jest **tym samym zdaniem**, które zwrócił walidator, słowo w słowo. Własne
    /// tłumaczenie byłoby drugim miejscem, w którym mieszka ten sam komunikat, i jedno z nich
    /// zawsze jest nieaktualne (tak samo czyta to `workflow::file::SaveError::Refused`).
    #[error("{}", .0.message)]
    Refused(Note),
    /// Pliku workflow nie dało się wczytać.
    #[error(transparent)]
    Unreadable(#[from] LoadError),
    /// Krok nazywa agenta, którego nie da się przeczytać albo którego nie ma w bibliotece.
    #[error(transparent)]
    Agent(#[from] AgentError),
    /// Grafu nie dało się zbudować.
    #[error(transparent)]
    Graph(#[from] crate::engine::dag::DagError),
    /// Indeks odmówił.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// Katalog biegu albo `run.json` nie dały się zapisać.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Czegoś nie dało się zamienić w JSON albo z niego wyjąć.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
