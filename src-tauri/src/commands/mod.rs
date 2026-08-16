//! Warstwa komend: co się dzieje, kiedy człowiek naciśnie Start, Stop albo Continue.
//!
//! **W tym katalogu nie ma ani jednego `#[tauri::command]` i ani jednego `use tauri::`.**
//! `docs/ARCHITECTURE.md` §3 daje słowo „Tauri" wyłącznie plikowi `ipc.rs`, a mapa własności daje
//! ten katalog zadaniu T-15. Godzimy to tak: tutaj mieszkają **wyłącznie** funkcje `*_inner`
//! biorące [`RunDeps`], a dwuliniowe skorupy `#[tauri::command]` i jedna lista
//! `generate_handler!` należą do T-07. Powód jest testowy, nie estetyczny: `State<'_, AppState>`
//! nie da się zbudować w teście jednostkowym, a `&RunDeps` da się [04 §2.1].
//!
//! # Stan tego pliku: SZKIELET (2026-08-16)
//!
//! Typy są tu w całości, bo to one są kontraktem, o który opierają się kryteria. Ciała funkcji
//! biegu siedzą w [`run`] i są `todo!()`: test ma się **skompilować** i paść w czasie wykonania,
//! na braku zachowania, a nie na braku modułu (`AGENTS.md` §2a p. 5). `clippy::todo = deny`
//! w `Cargo.toml` pilnuje, żeby żaden z nich nie dożył pełnej bramki.

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

pub mod run;

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

    /// Czeka, aż ktoś powie „dalej" **albo** zatrzyma bieg. Wraca `true`, kiedy padło „dalej".
    ///
    /// Liczba zapamiętana przed czekaniem jest tym, co odróżnia zgodę na **ten** punkt kontrolny
    /// od zgody sprzed dziesięciu minut.
    pub async fn wait_for_go_on(&self) -> bool {
        let mut told = self.inner.go_on.subscribe();
        let before = *told.borrow_and_update();
        loop {
            if self.inner.cancel.is_cancelled() {
                return false;
            }
            if *told.borrow_and_update() > before {
                return true;
            }
            tokio::select! {
                biased;
                () = self.inner.cancel.cancelled() => return false,
                changed = told.changed() => {
                    if changed.is_err() {
                        // Nadawca zginął razem z biegiem; nie ma na co czekać.
                        return false;
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
