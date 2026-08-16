//! Rejestr workspace'ów: **jeden folder = jedna karta**, i wszystko, co karty dzielą.
//!
//! Karta to folder, w którym pracuje AI (`docs/ARCHITECTURE.md` §6a). To jest jedyne miejsce,
//! które wie, ile kart jest otwartych i która jest na wierzchu. Silnik zna wyłącznie biegi
//! i nie ma prawa usłyszeć słowa „karta" (§6a reguła 2): karta jest zapytaniem „pokaż mi bieg
//! dla tego folderu", a nie stanem, który bieg musi obserwować.
//!
//! Trzy własności mieszkają tutaj i każda z nich łamie się po cichu:
//!
//! - **Pompa linii należy do KARTY, nie do widoku.** Przełączenie karty jest wyłącznie zmianą
//!   widoku, więc odbiornik strumienia nie ma prawa wisieć na tym, co akurat widać. Wersja,
//!   w której wisi, przechodzi każdy test pisany na karcie aktywnej i gubi linie dokładnie
//!   wtedy, kiedy człowiek zajrzy do innego folderu — a wraca do niej z pustą historią albo
//!   z „Thinking…" sprzed dwóch minut.
//! - **Pula miejsc jest JEDNA na całą aplikację** (niezmiennik 11, §6a). Trzy karty po trzech
//!   agentach to dziewięciu agentów po ~583 MB [T7 ryzyko 3] — na 16 GB to zamrożony laptop,
//!   a nie szybsza praca. Limit liczony per bieg wygląda identycznie do chwili, w której ktoś
//!   otworzy trzecią kartę.
//! - **Drugie otwarcie tego samego folderu oddaje TEN SAM magazyn** (niezmiennik 2).
//!   `Store::open` nie ma żadnej obrony przed drugim otwarciem tej samej bazy i świadomie jej
//!   nie dostaje — decyzja człowieka z 2026-08-16 brzmi, że gwarancja mieszka w rejestrze
//!   workspace'ów, czyli tutaj. Rejestr, który przy drugim `open()` woła `Store::open` jeszcze
//!   raz, uruchamia drugie **zapisujące** połączenie do tego samego pliku, a to jest
//!   zakleszczenie, nie „czasem wolniej" [T7 ryzyko 7]. Po `WorkspaceId` tego nie widać:
//!   identyfikator jest wyliczany ze ścieżki, więc zgadza się także wtedy.
//!
//! # Stan tego pliku: SZKIELET (2026-08-16)
//!
//! Rejestr **niczego nie zapamiętuje**. [`Registry::open`] kanonikalizuje ścieżkę i sprawdza,
//! czy to w ogóle folder — to jest czysta połowa, którą widać w sygnaturze — po czym oddaje
//! identyfikator i **nie zakłada karty**. Wszystko, co pyta o stan rejestru, mówi więc prawdę:
//! [`Registry::tabs`] jest pusty, [`Registry::store`] nie zna żadnego folderu, a
//! [`Registry::set_active`], [`Registry::slots`] i [`Registry::attach_run`] odmawiają przez
//! [`WorkspaceError::NoSuchWorkspace`] — bo takiej karty naprawdę nie ma.
//!
//! To jest wymagany kształt fazy, w której powstają kryteria: test ma się skompilować i paść
//! **w czasie wykonania, na braku ZACHOWANIA** (`AGENTS.md` §2a p. 5). Żadnego kryterium nie
//! da się na tym przejść — każde z trzech pyta rejestr o kartę, którą przed chwilą otworzyło.
//!
//! Ciała nie są `todo!()` i nie mogą nim być: `clippy::todo` jest w `Cargo.toml` na `deny`,
//! a `checks/quick-clippy.sh` biegnie z `-D warnings` w KAŻDEJ warstwie bramki, także w tej.
//! Ten sam wybór — świadomie niekompletne ciało zamiast makra paniki — zapisał T-02
//! w nagłówku `engine/mod.rs`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::engine::limits::Limiter;
use crate::store::{Store, StoreError};

/// Ile folderów pamięta menu wyboru. Lista ostatnio używanych jest **krótka z rozmysłem**:
/// menu, które trzeba przewijać, przestaje być skrótem (`docs/ARCHITECTURE.md` §6a).
pub const RECENT_CAP: usize = 10;

/// Karta w rejestrze: identyfikator i magazyn tego folderu.
///
/// Krotka, a nie struktura, i to jest wybór na czas szkieletu: struktura, której nikt jeszcze
/// nie konstruuje, jest dla `rustc` martwym typem, a jedyne wyjście z tego — atrybut wyciszający
/// ten lint nad definicją — przewraca `checks/quick-suppressions.sh`, i słusznie: to jest ta
/// jedna linia, która wyłącza bramkę od środka i w diffie wygląda jak zwykły kod. Struktura
/// wraca razem z pierwszą kartą, którą rejestr naprawdę założy.
type OpenTab = (WorkspaceId, Arc<Store>);

/// Identyfikator workspace'a: **kanoniczna ścieżka folderu**, i nic więcej.
///
/// Wyliczany, nie nadawany, i to jest cała jego treść: `~/Projects/meetnotes`,
/// `~/Projects/meetnotes/`, `~/Projects/./meetnotes` i dowiązanie symboliczne wskazujące na ten
/// sam katalog dają jedną wartość, więc żadne z nich nie założy drugiej karty.
///
/// **Czego ten typ NIE dowodzi.** Skoro jest wyliczany ze ścieżki, to zgadza się **zawsze** —
/// także w rejestrze, który pod spodem otworzył drugi magazyn do tego samego pliku. Tożsamości
/// magazynu trzeba dowieść osobno i po `Arc::ptr_eq`, nie po tej wartości.
///
/// `PathBuf`, nie `String`: ścieżka nie musi być poprawnym tekstem, a `to_string_lossy` skleiłby
/// dwa różne foldery w jeden identyfikator dokładnie wtedy, gdy nie są tekstem.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WorkspaceId(PathBuf);

impl WorkspaceId {
    /// Kanoniczny folder tej karty.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl std::fmt::Display for WorkspaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `.display()`, nie `{:?}`: ta wartość jedzie do webviewa jako identyfikator karty,
        // a cudzysłowy i escape'y z `Debug` byłyby wtedy częścią klucza Reacta.
        write!(f, "{}", self.0.display())
    }
}

/// Wszystko, czym ten moduł odmawia.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    /// Wskazano coś, co nie jest folderem. Plik i katalog, którego nie ma, wyglądają dla
    /// użytkownika tak samo — więc obie drogi kończą tutaj, a ścieżka jest w komunikacie.
    // `.path.display()`, nie `{path}`: `PathBuf` nie implementuje `Display` i nigdy nie będzie.
    #[error("{} is not a folder, so nothing can work there", .path.display())]
    NotAFolder {
        /// Ścieżka, o którą prosił wołający — **jak ją podał**, nie po kanonikalizacji.
        path: PathBuf,
    },

    /// Folder istnieje, ale nie dało się go przeczytać: brak uprawnień, zerwane dowiązanie,
    /// odmontowany dysk sieciowy.
    #[error("{} could not be read: {source}", .path.display())]
    Unreadable {
        /// Ścieżka, o którą prosił wołający.
        path: PathBuf,
        /// To, czym odmówił system plików.
        source: std::io::Error,
    },

    /// Pytanie o kartę, której nie ma. **Nie jest to awaria** — karta mogła zostać zamknięta
    /// między odczytem UI a wywołaniem, i wtedy jedyną poprawną odpowiedzią jest „już jej nie
    /// ma", a nie panika w agentowym runtime.
    #[error("no tab is open for {0}")]
    NoSuchWorkspace(WorkspaceId),

    /// Pompa tej karty już nie żyje, więc linie biegu nie mają gdzie wylądować.
    #[error("this tab has no line pump any more, so its run has nowhere to write")]
    PumpGone,

    /// Cokolwiek, czym odmówił magazyn tego folderu.
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Skrót, którego używa cały moduł.
pub type Result<T> = std::result::Result<T, WorkspaceError>;

/// Czym skończył się bieg karty.
///
/// **Wartość, nie `Result`** (niezmiennik 7): anulowanie po zamknięciu karty jest normalnym
/// zakończeniem biegu, a nie usterką. `Err(Cancelled)` zmuszałoby wołającego do rozpakowywania
/// błędu, który błędem nie jest — a stamtąd jest już tylko krok do pokazania świadomego Stopu
/// jako awarii.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    /// Bieg doszedł do końca, a karta zapisała **wszystko**, co jej podał.
    Succeeded,
    /// Człowiek zamknął kartę i potwierdził. Nigdy po cichu i nigdy jako błąd.
    Cancelled,
    /// Pompa karty skończyła przed biegiem: co najmniej jedna linia, którą karta przyjęła,
    /// nie doszła do magazynu.
    ///
    /// To jest cicha porażka tego zadania nazwana wartością. Bez tego wariantu bieg, któremu
    /// przełączenie karty odpięło odbiornik, melduje `Succeeded` i wygląda dokładnie jak zdrowy
    /// — z historią krótszą o te sto linii, których nikt nie liczył.
    Interrupted,
}

/// Jedna linia biegu w drodze do magazynu karty.
///
/// Kształt jest cieniem [`crate::store::NewEvent`] bez `run_id` i `step_id`: te dwa dokłada
/// karta, bo tylko ona wie, którego biegu słucha. Wołający ma nie mieć jak wysłać linii
/// do cudzego biegu.
#[derive(Debug, Clone)]
pub struct RunLine {
    /// Milisekundy epoki.
    pub ts: i64,
    /// Rodzaj zdarzenia po naszej stronie: `assistant`, `tool_use`, `result`, …
    pub kind: String,
    /// `headline`, `detail` albo `raw`.
    pub level: String,
    /// Treść linii.
    pub body: String,
}

/// Wejście do pompy jednej karty: tędy bieg oddaje swoje linie.
///
/// **Odbiornik został w karcie.** Ten uchwyt jest tylko nadajnikiem, więc nie ma czego odpiąć
/// przy przełączeniu widoku — a to jest dokładnie ta jedna decyzja, o którą chodzi w AC-1.
#[derive(Debug)]
pub struct RunSink {
    /// Kanał do pompy karty.
    lines: mpsc::Sender<RunLine>,
    /// Zadanie pompy. Oddaje, **ile linii naprawdę dopisało do magazynu**.
    pump: JoinHandle<u64>,
    /// Ile linii ten uchwyt przyjął. Porównanie tych dwóch liczb jest jedynym miejscem,
    /// w którym „nic nie zginęło" jest zdaniem sprawdzalnym, a nie założeniem.
    accepted: u64,
}

impl RunSink {
    /// Oddaje linię karcie. Czeka, kiedy pompa jest w tyle — nigdy jej nie wyprzedza.
    ///
    /// Kanał jest **ograniczony**, a nie `unbounded_channel`: nieograniczony zamienia wolniejszą
    /// pompę w rosnącą stertę i awaria przychodzi jako pamięć zamiast jako czekanie, czyli
    /// najpóźniej jak się da i bez śladu, kto ją spowodował.
    pub async fn send(&mut self, line: RunLine) -> Result<()> {
        self.lines
            .send(line)
            .await
            .map_err(|_| WorkspaceError::PumpGone)?;
        self.accepted += 1;
        Ok(())
    }

    /// Domyka bieg: zamyka kanał, **czeka** na pompę i mówi, czym ten bieg się skończył.
    ///
    /// Czekanie nie jest uprzejmością. Bez niego „zapisane" znaczy tylko „wysłane", a różnicę
    /// między tymi dwoma widać wyłącznie wtedy, gdy ktoś zamknie kartę w trakcie biegu.
    ///
    /// `outcome` mówi, czym skończył **bieg**; wynik mówi, czym skończyła się **karta**. Te dwie
    /// rzeczy rozjeżdżają się dokładnie wtedy, gdy pompa czegoś nie doniosła — i wtedy wygrywa
    /// [`RunOutcome::Interrupted`], bo historia z dziurą nie jest historią udanego biegu.
    pub async fn finish(self, outcome: RunOutcome) -> Result<RunOutcome> {
        // Nadajnik ginie PRZED czekaniem na pompę. Zostawiony przy życiu trzyma kanał otwarty,
        // więc pętla pompy nie ma prawa się skończyć i `await` niżej wisiałby bez końca.
        drop(self.lines);
        let written = self.pump.await.map_err(|_| WorkspaceError::PumpGone)?;
        Ok(if written == self.accepted {
            outcome
        } else {
            RunOutcome::Interrupted
        })
    }
}

/// Rejestr otwartych kart i jedyne miejsce, w którym mieszka wspólna pula miejsc.
#[derive(Debug)]
pub struct Registry {
    /// **Jedna pula na całą aplikację**, nie jedna na bieg (niezmiennik 11).
    ///
    /// Powód jest arytmetyczny, nie estetyczny: trzy karty po trzech agentach to dziewięciu
    /// agentów po ~583 MB szczytowego RSS [T7 ryzyko 3, V], czyli ~5,2 GB na maszynie, na
    /// której ta aplikacja jest tylko jednym z programów. Limit liczony per bieg zachowuje się
    /// identycznie jak globalny do chwili, w której ktoś otworzy drugą kartę — i dlatego
    /// nie ma go jak zauważyć bez testu na dwa biegi naraz.
    ///
    /// [`Limiter`] jest klonowalnym **uchwytem** do puli, więc dzielenie jej sprowadza się
    /// do klonowania tego pola. Nowa pula „na razie per karta, potem się scali" jest tym samym
    /// błędem, tylko odroczonym: po scaleniu nikt nie sprawdzi, czy stara zniknęła.
    slots: Limiter,

    /// Karty w kolejności otwarcia — ta sama, w której stoją na pasku.
    open: Mutex<Vec<OpenTab>>,

    /// Karta na wierzchu. `None`, dopóki żadna nie jest otwarta.
    ///
    /// **To jest cały stan przełączania.** Zmiana tego pola nie ma prawa dotknąć niczego poza
    /// nim (§6a reguła 2); w chwili, w której zacznie odpinać odbiorniki, biegi w tle zaczną
    /// pisać do kanału, którego nikt nie czyta.
    active: Mutex<Option<WorkspaceId>>,

    /// Ostatnio używane foldery, **najnowszy pierwszy**, przycięte do [`RECENT_CAP`].
    ///
    /// Osobno od `open`, bo odpowiada na inne pytanie: `open` mówi, co stoi na pasku, a to
    /// mówi, co zaproponować w menu wyboru folderu — także folder, którego karta jest już
    /// zamknięta.
    recent: Mutex<Vec<WorkspaceId>>,
}

impl Registry {
    /// Pusty rejestr nad **podaną** pulą miejsc.
    ///
    /// Pula wchodzi argumentem, a nie powstaje tutaj, i to jest różnica nośna: rejestr, który
    /// robi sobie własną pulę, jest nie do odróżnienia od rejestru, który robi po jednej na
    /// kartę. Skoro pula przychodzi z zewnątrz, „jedna na aplikację" jest zdaniem o tym, kto
    /// ją tworzy, i da się je sprawdzić.
    #[must_use]
    pub fn new(slots: Limiter) -> Self {
        Self {
            slots,
            open: Mutex::new(Vec::new()),
            active: Mutex::new(None),
            recent: Mutex::new(Vec::new()),
        }
    }

    /// Otwiera folder jako kartę i oddaje jej identyfikator.
    ///
    /// Ten sam folder **nigdy** nie zakłada drugiej karty (§6a reguła 1): drugie wywołanie
    /// oddaje ten sam identyfikator, ten sam magazyn i przełącza na kartę, która już stoi.
    /// Dwa biegi w jednym katalogu kolidowałyby na plikach, a kopia per krok chroni tylko kroki
    /// między sobą, nie biegi między sobą.
    ///
    /// Porównanie idzie po **kanonikalizacji**, nie po tekście: `~/Projects/meetnotes/`
    /// i `~/Projects/./meetnotes` to ten sam folder, a dowiązanie symboliczne to ten sam folder
    /// pod inną nazwą. Porównanie surowych stringów przechodzi dla dwóch identycznych wywołań
    /// i pęka na każdym z tych trzech.
    pub fn open(&self, folder: &Path) -> Result<WorkspaceId> {
        // Kanonikalizacja załatwia `.`, `..`, końcowy ukośnik i dowiązania jednym wywołaniem
        // jądra — i jest jedynym sposobem, żeby zrobić to poprawnie: ręczne sklejanie
        // komponentów nie widzi dowiązania, a właśnie ono jest najczęstszym wejściem
        // (`~/work` wskazujące na `~/Projects`).
        let canonical = folder
            .canonicalize()
            .map_err(|source| WorkspaceError::Unreadable {
                path: folder.to_path_buf(),
                source,
            })?;
        if !canonical.is_dir() {
            return Err(WorkspaceError::NotAFolder {
                path: folder.to_path_buf(),
            });
        }

        // SZKIELET (2026-08-16). Tu kończy się czysta połowa i zaczyna brakujące zachowanie:
        // karta NIE trafia do `open`, magazyn folderu NIE powstaje, a lista ostatnich zostaje
        // pusta. Identyfikator jest prawdziwy — jest wyliczany ze ścieżki i niczego o rejestrze
        // nie twierdzi — więc żadne kryterium nie da się na tym przejść: wszystkie trzy pytają
        // rejestr o kartę, którą przed chwilą otworzyły.
        Ok(WorkspaceId(canonical))
    }

    /// Karty na pasku, w kolejności otwarcia.
    #[must_use]
    pub fn tabs(&self) -> Vec<WorkspaceId> {
        lock(&self.open).iter().map(|(id, _)| id.clone()).collect()
    }

    /// Ostatnio używane foldery, najnowszy pierwszy, najwyżej [`RECENT_CAP`].
    #[must_use]
    pub fn recent(&self) -> Vec<WorkspaceId> {
        lock(&self.recent).clone()
    }

    /// Magazyn tej karty — **ten sam obiekt** przy każdym pytaniu.
    ///
    /// Tożsamość, nie równoważność, i to jest cała treść niezmiennika 2 w tym module:
    /// dwa `Arc` wskazujące na dwa różne [`Store`] to dwa zapisujące połączenia do jednego
    /// pliku, czyli zakleszczenie. Sprawdza się to `Arc::ptr_eq`, bo po `WorkspaceId` tego
    /// nie widać.
    #[must_use]
    pub fn store(&self, id: &WorkspaceId) -> Option<Arc<Store>> {
        lock(&self.open)
            .iter()
            .find(|(open, _)| open == id)
            .map(|(_, store)| Arc::clone(store))
    }

    /// Karta na wierzchu.
    #[must_use]
    pub fn active(&self) -> Option<WorkspaceId> {
        lock(&self.active).clone()
    }

    /// Przełącza kartę. **Wyłącznie zmiana widoku** (§6a reguła 2).
    ///
    /// Nic się tu nie pauzuje, nie odłącza i nie ginie: biegi w kartach w tle działają dalej,
    /// a ich linie idą tą samą pompą co przedtem. Implementacja, która przy okazji odpina
    /// odbiornik strumienia, przechodzi każdy test pisany na karcie aktywnej.
    pub fn set_active(&self, id: &WorkspaceId) -> Result<()> {
        // SZKIELET (2026-08-16): rejestr nie zna żadnej karty, więc odmowa jest tu zdaniem
        // prawdziwym. Brakuje wpisania `id` do `self.active` — i niczego więcej.
        self.known(id)?;
        *lock(&self.active) = Some(id.clone());
        Ok(())
    }

    /// Uchwyt do wspólnej puli miejsc dla biegu w tej karcie.
    ///
    /// **Klon, nigdy nowa pula.** Trzy karty proszące o miejsca dostają trzy uchwyty do jednej
    /// puli; wersja z `Limiter::new` per karta daje przy limicie 2 sześciu agentów naraz
    /// i wygląda z zewnątrz identycznie — wszystkie biegi się kończą, tylko maszyna staje.
    pub fn slots(&self, id: &WorkspaceId) -> Result<Limiter> {
        self.known(id)?;
        Ok(self.slots.clone())
    }

    /// Podpina bieg do karty i oddaje wejście do jej pompy.
    ///
    /// Wiersz `runs` zakłada ten, kto bieg **zaczyna** (T-07); karta dostaje już tylko jego
    /// identyfikator i obowiązek doniesienia każdej linii do magazynu tego folderu.
    ///
    /// Pompa jest zadaniem karty i żyje tak długo, jak długo żyje [`RunSink`] — nie tak długo,
    /// jak długo karta jest widoczna. Ta jedna różnica jest całym AC-1.
    pub fn attach_run(&self, id: &WorkspaceId, run_id: &str) -> Result<RunSink> {
        self.known(id)?;
        // SZKIELET (2026-08-16): karty nie ma, więc ta linia jest nieosiągalna — a kiedy karta
        // będzie, brakuje tu zadania pompy: pętli czytającej kanał DO KOŃCA i wsadowego
        // `append_events` przez `Store::writer` tej karty (niezmiennik 2 — nigdy własne
        // połączenie zapisujące).
        tracing::debug!(tab = %id, run = run_id, "this tab has no line pump yet");
        Err(WorkspaceError::PumpGone)
    }

    /// Odmawia, kiedy takiej karty nie ma. Jedno miejsce, żeby komunikat brzmiał tak samo
    /// niezależnie od tego, kto pytał.
    fn known(&self, id: &WorkspaceId) -> Result<()> {
        if lock(&self.open).iter().any(|(open, _)| open == id) {
            Ok(())
        } else {
            Err(WorkspaceError::NoSuchWorkspace(id.clone()))
        }
    }
}

/// Zamek na stanie rejestru.
///
/// Zatrute zamki odplatamy, zamiast panikować: panika w agentowym runtime zabiera cały bieg
/// (`AGENTS.md` §4), a lista kart po panice jednego kroku jest dalej poprawna.
///
/// **Ten guard nigdy nie przechodzi przez `await`** (niezmiennik 8). Każde jego wzięcie mieści
/// się w jednym wyrażeniu bez punktu zawieszenia; `clippy::await_holding_lock` (deny
/// w `Cargo.toml`) pilnuje reszty, ale sam w sobie jest siatką, nie projektem.
fn lock<T>(cell: &Mutex<T>) -> MutexGuard<'_, T> {
    cell.lock().unwrap_or_else(PoisonError::into_inner)
}
