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
//!
//!   2026-08-24 (T-94) — **ŻYWA PULA PRZENIOSŁA SIĘ DO `AppState`**, a [`Registry::slots`]
//!   dalej nie ma produkcyjnego wołającego. Aplikacja trzyma jeden [`crate::engine::limits::
//!   Limiter`] i wkłada jego klon do uchwytu każdego biegu (`ipc::AppState::begin_run`), więc
//!   własność opisana wyżej jest od tego dnia egzekwowana — tylko nie tędy. Co zrobić z tym
//!   rejestrem, jest decyzją człowieka, nie tego zadania: jeden fakt ma mieć jeden dom
//!   (niezmiennik 13), a dziś ma dwa domy, z których zamieszkany jest jeden.
//! - **Drugie otwarcie tego samego folderu oddaje TEN SAM magazyn** (niezmiennik 2).
//!   `Store::open` nie ma żadnej obrony przed drugim otwarciem tej samej bazy i świadomie jej
//!   nie dostaje — decyzja człowieka z 2026-08-16 brzmi, że gwarancja mieszka w rejestrze
//!   workspace'ów, czyli tutaj. Rejestr, który przy drugim `open()` woła `Store::open` jeszcze
//!   raz, uruchamia drugie **zapisujące** połączenie do tego samego pliku, a to jest
//!   zakleszczenie, nie „czasem wolniej" [T7 ryzyko 7]. Po `WorkspaceId` tego nie widać:
//!   identyfikator jest wyliczany ze ścieżki, więc zgadza się także wtedy.
//!
//! # Gdzie leży magazyn karty
//!
//! W folderze workspace'a, pod `<folder>/.loadout/loadout.db` (`docs/ARCHITECTURE.md` §8).
//! Jeden folder ma jeden indeks i jednego pisarza; kasowanie tego pliku kosztuje indeks
//! i nic poza nim (niezmiennik 4). Rejestr zakłada `.loadout/` przy pierwszym otwarciu,
//! bo bez katalogu magazyn nie ma gdzie stanąć.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::engine::limits::Limiter;
use crate::store::{NewEvent, Store, StoreError};

/// Ile folderów pamięta menu wyboru. Lista ostatnio używanych jest **krótka z rozmysłem**:
/// menu, które trzeba przewijać, przestaje być skrótem (`docs/ARCHITECTURE.md` §6a).
pub const RECENT_CAP: usize = 10;

/// Katalog projektowy Loadouta wewnątrz folderu workspace'a (`docs/ARCHITECTURE.md` §8).
const PROJECT_DIR: &str = ".loadout";

/// Indeks tego folderu. **Wolno go skasować** — jest indeksem, nie prawdą (niezmiennik 4).
const INDEX_FILE: &str = "loadout.db";

/// Ile linii mieści się w kanale karty, zanim bieg zaczeka na pompę.
///
/// Kanał **ograniczony**, nigdy `unbounded_channel`: nieograniczony zamienia wolniejszą pompę
/// w rosnącą stertę i awaria przychodzi jako pamięć zamiast jako czekanie — czyli najpóźniej
/// jak się da i bez śladu, kto ją spowodował.
const LINES_IN_FLIGHT: usize = 256;

/// Ile linii idzie do magazynu w jednej transakcji.
///
/// Zmierzone [T7 §5.3]: wsad stu wierszy to 662 238 wierszy/s wobec 67 144 przy jednym wierszu
/// na transakcję. Pompa bierze tyle, ile akurat stoi w kanale, więc przy wolnym strumieniu
/// wsad ma jeden wiersz i nic na to nie czeka — ta stała jest sufitem, nie progiem.
const LINES_PER_BATCH: usize = 100;

/// Karta w rejestrze: identyfikator folderu i magazyn tego folderu.
#[derive(Debug)]
struct OpenTab {
    /// Kanoniczna ścieżka folderu — ta sama wartość, którą dostał wołający [`Registry::open`].
    id: WorkspaceId,

    /// Magazyn **tego** folderu.
    ///
    /// `Arc`, bo tożsamość tego obiektu jest całą treścią niezmiennika 2 w tym module: drugie
    /// otwarcie folderu oddaje ten sam `Arc`, a nie równoważny magazyn — dwa `Store` nad jednym
    /// plikiem to dwa zapisujące połączenia, czyli zakleszczenie [T7 ryzyko 7].
    store: Arc<Store>,
}

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
    /// Tożsamość TEGO folderu — jedna na całe repo, także dla folderu, którego już nie ma.
    ///
    /// 2026-08-28 — POWSTAŁA, ŻEBY ZAPADKA BIEGU MIAŁA TEN SAM KLUCZ, CO PASEK KART. Zapadka
    /// „jeden bieg naraz" w [`crate::ipc::AppState`] jest kluczowana workspace'em, a klucz
    /// liczony z surowego napisu ścieżki dawałby dwa uchwyty dla `~/p/x` i `~/p/./x` — czyli
    /// dwa biegi w jednym folderze, po plikach których piszą obaj (§6a reguła 1).
    ///
    /// **TOTALNA Z ROZMYSŁU**, i to jest cała różnica wobec [`Registry::open`]: folder skasowany
    /// albo odmontowany W TRAKCIE biegu przestaje się kanonikalizować, a wtedy zapadka nie
    /// znalazłaby uchwytu i Stop nie miałby jak dosięgnąć agenta, który dalej pracuje i dalej
    /// płaci (niezmienniki 6 i 11). Nieczytelny folder oddaje więc ścieżkę PODANĄ: jest to
    /// klucz gorszy (dwa zapisy tej samej ścieżki mogą się rozjechać), ale jest to klucz.
    /// Odmowa nad nieczytelnym folderem należy do otwierania karty, nie do tożsamości.
    #[must_use]
    pub fn for_folder(folder: &Path) -> Self {
        Self(
            folder
                .canonicalize()
                .unwrap_or_else(|_error| folder.to_path_buf()),
        )
    }

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

    /// Folder istnieje i da się go przeczytać, ale nie da się w nim założyć `.loadout/`, czyli
    /// katalogu, w którym mieszka indeks tego workspace'a (`docs/ARCHITECTURE.md` §8). Dysk
    /// zamontowany tylko do odczytu, cudzy katalog, pełny wolumen — wszystkie kończą tutaj.
    // `.path.display()`, nie `{path}`: `PathBuf` nie implementuje `Display` i nigdy nie będzie.
    #[error("{} could not be set up for work: {source}", .path.display())]
    NotWritable {
        /// Katalog, którego nie dało się założyć.
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
        // Nieczytelny folder odmawia TUTAJ, a nie w tożsamości: [`WorkspaceId::for_folder`] jest
        // funkcją totalną (powód w całości stoi przy niej), więc sama nie ma czym odmówić.
        // To jedno wywołanie zostaje wyłącznie po `io::Error`, którym odmawia system plików —
        // kanoniczną ścieżkę wybija niżej ta jedna funkcja, żeby zapadka biegu i pasek kart
        // liczyły tożsamość TYM SAMYM rachunkiem (niezmiennik 13).
        folder
            .canonicalize()
            .map_err(|source| WorkspaceError::Unreadable {
                path: folder.to_path_buf(),
                source,
            })?;
        let id = WorkspaceId::for_folder(folder);
        if !id.as_path().is_dir() {
            return Err(WorkspaceError::NotAFolder {
                path: folder.to_path_buf(),
            });
        }

        {
            // Sprawdzenie i założenie karty pod JEDNYM zamkiem, w jednym bloku. Rozdzielone
            // na „zajrzyj, zwolnij, otwórz magazyn, wstaw" przepuszczają dwa wątki otwierające
            // ten sam folder w tej samej chwili — obydwa nie znajdują karty i obydwa wołają
            // `Store::open`. Skutkiem są dwa ZAPISUJĄCE połączenia do jednego pliku, czyli
            // dokładnie to, przed czym stoi niezmiennik 2, tylko trudniejsze do zobaczenia,
            // bo zdarza się raz na sto uruchomień.
            //
            // Ten zamek nie przechodzi przez `await` (niezmiennik 8): `Store::open` jest
            // funkcją synchroniczną — startuje zadanie pisarza, ale na nic nie czeka.
            let mut open = lock(&self.open);
            if !open.iter().any(|tab| tab.id == id) {
                let store = open_store(id.as_path())?;
                open.push(OpenTab {
                    id: id.clone(),
                    store,
                });
            }
        }

        // Otwarcie folderu, który już ma kartę, PRZEŁĄCZA na nią (§6a reguła 1) — a otwarcie
        // jest użyciem, więc folder idzie na górę listy ostatnich także wtedy, gdy jego karta
        // stała na pasku od rana.
        self.remember(&id);
        self.set_active(&id)?;
        Ok(id)
    }

    /// Karty na pasku, w kolejności otwarcia.
    #[must_use]
    pub fn tabs(&self) -> Vec<WorkspaceId> {
        lock(&self.open).iter().map(|tab| tab.id.clone()).collect()
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
            .find(|tab| tab.id == *id)
            .map(|tab| Arc::clone(&tab.store))
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
        // Jedno pole i ani jednej linii więcej. To nie jest oszczędność, tylko cała treść
        // §6a reguły 2: przełączenie, które przy okazji czegokolwiek dotyka — odbiornika,
        // pauzy, pompy — jest przełączeniem, które gubi linie w karcie w tle.
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
        // Magazyn TEJ karty, wzięty raz i oddany pompie na własność. Pompa, która pytałaby
        // rejestr o magazyn przy każdym wsadzie, pisałaby do magazynu karty AKTYWNEJ w chwili
        // zapisu — a to jest ta sama awaria co odpięty odbiornik, tylko widać ją jako dwa
        // transkrypty sklejone w jednym pliku.
        let store = self
            .store(id)
            .ok_or_else(|| WorkspaceError::NoSuchWorkspace(id.clone()))?;

        // `try_current`, nie `tokio::spawn`: to drugie panikuje poza runtime'em, a panika
        // w agentowym runtime zabiera cały bieg (`AGENTS.md` §4). Wołający dostaje ten sam
        // błąd, który dostałby od magazynu.
        let handle = Handle::try_current().map_err(|_| StoreError::NoRuntime)?;
        let (lines, inbox) = mpsc::channel(LINES_IN_FLIGHT);

        // Pompa wisi na KARCIE, nie na widoku, i to jest całe AC-1. Zadanie żyje tak długo,
        // jak długo żyje [`RunSink`] biegu — przełączenie karty nie ma do niego dostępu,
        // więc nie ma czego odpiąć i nie ma gdzie zgubić stu linii.
        let pump = handle.spawn(pump(store, run_id.to_owned(), inbox));
        Ok(RunSink {
            lines,
            pump,
            accepted: 0,
        })
    }

    /// Odmawia, kiedy takiej karty nie ma. Jedno miejsce, żeby komunikat brzmiał tak samo
    /// niezależnie od tego, kto pytał.
    fn known(&self, id: &WorkspaceId) -> Result<()> {
        if lock(&self.open).iter().any(|tab| tab.id == *id) {
            Ok(())
        } else {
            Err(WorkspaceError::NoSuchWorkspace(id.clone()))
        }
    }

    /// Kładzie folder na górze listy ostatnio używanych i przycina ją do [`RECENT_CAP`].
    ///
    /// Kolejność jest po **użyciu**, nie po pierwszym otwarciu: lista układana wstawianiem
    /// czyta się tak samo przez pierwszych dziesięć otwarć i potem nigdy się nie zmienia,
    /// czyli jest menu, którego górna pozycja jest folderem sprzed tygodnia.
    fn remember(&self, id: &WorkspaceId) {
        let mut recent = lock(&self.recent);
        recent.retain(|seen| seen != id);
        recent.insert(0, id.clone());
        recent.truncate(RECENT_CAP);
    }
}

/// Otwiera magazyn folderu — `<folder>/.loadout/loadout.db` (`docs/ARCHITECTURE.md` §8).
///
/// Woła się to **wyłącznie** z [`Registry::open`], spod zamka na liście kart, i tylko dla
/// folderu, który karty jeszcze nie ma. Drugie wywołanie dla tego samego folderu byłoby drugim
/// zapisującym połączeniem do tego samego pliku (niezmiennik 2).
fn open_store(folder: &Path) -> Result<Arc<Store>> {
    let project = folder.join(PROJECT_DIR);
    std::fs::create_dir_all(&project).map_err(|source| WorkspaceError::NotWritable {
        path: project.clone(),
        source,
    })?;
    /* UZGODNIENIE Z PLIKAMI, ZANIM KTOKOLWIEK ZAJRZY DO TEGO FOLDERU.
     *
     * 2026-08-23 — zamowienie wlasciciela: „zrob cos aby nie bylo takich sytuacji ze jakis proces
     * wisi". Bieg, ktory zginal razem z aplikacja, zostawal w swoim `run.json` na zawsze jako
     * `running`; zmierzone u niego trzy takie naraz, siedem grup procesow dawno martwych.
     *
     * TUTAJ, a nie przy starcie okna, i to jest cala tresc tego miejsca: `lib::recover_from_last_time`
     * czyta baze biblioteki, a biegi folderu maja WLASNY indeks i wlasne pliki — wiec tamta droga
     * nie widziala ich nigdy. Ta widzi, bo folder wlasnie sie otwiera i wiemy, o ktory chodzi.
     *
     * RAZ NA FOLDER I SPOD ZAMKA: `open_store` wola sie wylacznie z `Registry::open` dla folderu,
     * ktory karty jeszcze nie ma (patrz doc wyzej). To jedyna chwila, w ktorej nikt inny tych
     * plikow nie trzyma — uzgodnienie w tle bilo by sie o nie z zywym biegiem.
     *
     * PRZED `Store::open`, zeby indeks otwieral sie nad plikami juz uczciwymi.
     *
     * NIE ODDAJE ODMOWY. Folder, ktorego biegow nie da sie uzgodnic, ma sie OTWORZYC — czlowiek,
     * ktoremu nie wstaje projekt przez jeden uszkodzony plik starego biegu, traci znacznie
     * wiecej niz jeden wiersz historii (niezmiennik 5). */
    let done = crate::commands::reconcile::reconcile_runs(folder);
    if done.runs > 0 || done.still_alive > 0 {
        tracing::info!(
            "opening {}: {} run(s) and {} step(s) left over from a closed window, \
             {} group(s) proven dead, {} still alive",
            folder.display(),
            done.runs,
            done.steps,
            done.reaped,
            done.still_alive,
        );
    }

    Ok(Arc::new(Store::open(&project.join(INDEX_FILE))?))
}

/// Pompa jednej karty: czyta kanał **do końca** i dopisuje linie do magazynu tego folderu.
///
/// Oddaje, ile linii naprawdę weszło do magazynu. Ta liczba, porównana z tym, ile linii karta
/// przyjęła, jest jedynym miejscem, w którym „nic nie zginęło" jest zdaniem sprawdzalnym —
/// bez niej bieg z dziurą w transkrypcie melduje `Succeeded` i wygląda dokładnie jak zdrowy.
///
/// Zapis idzie przez [`Store::writer`] tej karty, nigdy przez własne połączenie (niezmiennik 2).
async fn pump(store: Arc<Store>, run_id: String, mut lines: mpsc::Receiver<RunLine>) -> u64 {
    let writer = store.writer();
    let mut taken: Vec<RunLine> = Vec::with_capacity(LINES_PER_BATCH);
    let mut written: u64 = 0;

    // `recv_many`, nie `recv` w pętli: wsad wielkości tego, co akurat stoi w kanale, jest
    // o rząd wielkości tańszy od wiersza na transakcję [T7 §5.3] i nie kosztuje ani milisekundy
    // czekania — przy jednej linii w kanale wsad ma jedną linię. Zero znaczy kanał zamknięty
    // i opróżniony, czyli koniec biegu; dopiero wtedy ta pętla ma prawo się skończyć.
    while lines.recv_many(&mut taken, LINES_PER_BATCH).await > 0 {
        let batch: Vec<NewEvent> = taken
            .drain(..)
            .map(|line| NewEvent {
                run_id: run_id.clone(),
                // Karta nie zna kroków — zna bieg. Krok dokłada ten, kto go uruchomił (T-07).
                step_id: None,
                ts: line.ts,
                kind: line.kind,
                level: line.level,
                body: Some(line.body),
            })
            .collect();
        let rows = batch.len();

        match writer.append_events(batch).await {
            // `try_from`, nie `as`: obcięte bity policzyłyby zapis, którego nie było, i wtedy
            // liczba wpisanych linii przestaje być dowodem czegokolwiek.
            Ok(()) => written += u64::try_from(rows).unwrap_or(0),
            // Wsad wraca w CAŁOŚCI (transakcja), więc `written` po prostu nie rośnie i bieg
            // kończy się jako [`RunOutcome::Interrupted`]. Pętla leci dalej: jeden odrzucony
            // wsad nie ma prawa zabrać reszty transkryptu, a wołający i tak się dowie.
            Err(error) => tracing::error!(
                run = run_id,
                rows,
                "a batch of this run's lines did not reach the store: {error}"
            ),
        }
    }
    written
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
