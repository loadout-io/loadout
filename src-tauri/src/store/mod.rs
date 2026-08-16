//! Magazyn biegu: `SQLite` jako **indeks**, nigdy jako prawda.
//!
//! Cała wartość tego modułu mieści się w jednym zdaniu: `loadout.db` wolno skasować i nic się
//! nie stanie (`docs/ARCHITECTURE.md` §2 pyt. 2, niezmiennik 4). Wszystko tutaj jest temu
//! zdaniu podporządkowane — każda kolumna musi dać się odtworzyć z
//! `<repo>/.loadout/runs/<ts>__<id>/{run.json,logs/,handoffs/}`, a [`Store::rebuild_from`] jest
//! jedynym miejscem, w którym to twierdzenie da się sprawdzić. Pole zapisywane wyłącznie do
//! bazy w trakcie biegu (koszt kroku, podsumowanie dla szyny) łamie ten niezmiennik po cichu:
//! przez trzy tygodnie wszystko działa, bo nikt bazy nie kasuje.
//!
//! Trzy granice, które ten moduł trzyma, i sposób, w jaki każda z nich łamie się cicho:
//!
//! - **Pisze wyłącznie [`writer`]** (niezmiennik 2). Funkcja czytająca, która sięgnie po
//!   `Connection::open(path)` „bo tak prościej", działa miesiąc w testach jednowątkowych
//!   i wykłada się dopiero przy dwóch agentach naraz — czyli w jedynym scenariuszu, dla
//!   którego ten produkt istnieje. Pilnuje tego `checks/quick-boundary.sh` regułą 3: każdy
//!   plik poza `writer.rs` otwiera połączenie **wyłącznie** z `SQLITE_OPEN_READ_ONLY`.
//! - **Pragmy ustawia jedno miejsce** — [`apply_pragmas`] (niezmiennik 23). `busy_timeout`,
//!   `foreign_keys` i `synchronous` są własnością **połączenia** i wracają do wartości
//!   domyślnych przy każdym nowym; `foreign_keys` domyślnie jest **wyłączone**, więc czytelnik,
//!   który ominie ten helper, po cichu przestaje widzieć kaskady. `busy_timeout` zapomniany na
//!   połączeniu czytającym objawia się jako losowe „Save failed" raz na dwa dni i w meetnotes
//!   zajęło to dwóch pisarzy w tle, zanim ktoś zrozumiał, co się dzieje
//!   [00-SYNTHESIS §3, „SQLite"].
//! - **„Append-only" egzekwuje `SQLite`, nie Rust.** Wyzwalacz odmawia także połączeniu, które
//!   nigdy nie widziało tego kodu: migracji, skryptowi naprawczemu, przyszłemu daemonowi,
//!   `sqlite3` z terminala. Odmowa napisana w Ruście przechodzi wszystkie nasze testy i nie
//!   chroni przed niczym, bo testowała nasze API.
//!
//! # Stan tego pliku: SZKIELET (2026-08-16)
//!
//! Ciała funkcji zwracają **świadomie złą wartość** i każde jest tak oznaczone komentarzem
//! `SZKIELET`. To jest wymagany kształt fazy, w której powstają kryteria: test ma się
//! skompilować i paść **w czasie wykonania, na braku ZACHOWANIA** — test, który się nie
//! kompiluje, niczego nie uruchomił (`AGENTS.md` §2a p. 5). `todo!()` tu nie stoi, bo `todo`
//! jest `deny` w `[workspace.lints.clippy]`. Każdy stub jest dobrany tak, żeby **żadnego**
//! kryterium nie dało się na nim przejść, i jest to rozpisane przy każdym ciele z osobna.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

pub mod migrate;
pub mod schema;
pub mod writer;

pub use migrate::migrate;
pub use writer::Writer;

/// Ile milisekund połączenie czeka na zwolnienie bazy, zanim odda `SQLITE_BUSY`.
///
/// Ta sama liczba na **każdym** połączeniu, także czytającym [T7 §5.4]. Połączenie bez tego
/// ustawienia nie czeka ani chwili i oddaje błąd, którego użytkownik czyta jako „nie udało się
/// zapisać" — a naprawdę znaczył on „ktoś inny pisał przez trzy milisekundy".
pub const BUSY_TIMEOUT_MS: i64 = 5_000;

/// Wszystko, czym ten moduł umie odmówić.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// Bazy nie dało się otworzyć. Ścieżka jest w komunikacie, bo bez niej „unable to open
    /// database file" nie mówi, którego pliku dotyczy — a w biegu z kilkoma kartami są cztery.
    // `.path.display()`, nie `{path}`: `PathBuf` nie implementuje `Display` i nigdy nie będzie,
    // bo ścieżka nie musi być poprawnym tekstem. Interpolacja wprost jest błędem kompilacji.
    #[error("the database at {} could not be opened: {source}", .path.display())]
    Open {
        /// Plik, którego dotyczy.
        path: PathBuf,
        /// To, czym odmówiło `SQLite`.
        source: rusqlite::Error,
    },

    /// Wsad odrzucony **w całości**. Liczba wierszy jest w komunikacie, bo wołający ma się
    /// dowiedzieć, ile zdarzeń nie weszło — bez tego „constraint failed" wygląda na jeden zły
    /// wiersz, a naprawdę wróciła cała transakcja (AC-6).
    #[error("a batch of {rows} events was refused whole and rolled back: {source}")]
    Batch {
        /// Ile wierszy niósł wsad, który wrócił.
        rows: usize,
        /// Powód odmowy, prosto z `SQLite`.
        source: rusqlite::Error,
    },

    /// Zadanie pisarza już nie żyje, więc nie ma kto zapisać.
    #[error("this store has no writer task any more, so nothing can be written")]
    WriterGone,

    /// [`Store::open`] wołane spoza środowiska tokio. Zamiast paniki z `Handle::current()`
    /// oddajemy błąd: panika w agentowym runtime zabiera cały bieg (`AGENTS.md`, tabela zakazów).
    #[error("a store can only be opened from inside a tokio runtime")]
    NoRuntime,

    /// Cokolwiek innego z `SQLite`.
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),

    /// Katalog biegu, którego nie dało się przeczytać.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// `run.json`, którego nie dało się zdekodować.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Skrót, którego używa cały moduł.
pub type Result<T> = std::result::Result<T, StoreError>;

/// Cztery pragmy, które muszą czytać się **tak samo na każdym połączeniu**.
///
/// Podział, o który się tu potyka każdy: `journal_mode` jest własnością **bazy** i trwa
/// w pliku, a pozostałe trzy są własnością **połączenia**. Dlatego gołe połączenie do bazy,
/// którą ktoś kiedyś przestawił na WAL, dalej melduje `wal` — i jednocześnie `busy_timeout`
/// równy zeru oraz **wyłączone** klucze obce.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pragmas {
    /// `PRAGMA busy_timeout` — w milisekundach.
    pub busy_timeout: i64,
    /// `PRAGMA foreign_keys` — 0 albo 1.
    pub foreign_keys: i64,
    /// `PRAGMA synchronous` — 0 `OFF`, 1 `NORMAL`, 2 `FULL`, 3 `EXTRA`.
    pub synchronous: i64,
    /// `PRAGMA journal_mode` — na bazie plikowej ma być `wal`.
    pub journal_mode: String,
}

/// Wiersz `runs`, w kształcie, w jakim przychodzi z `run.json`.
///
/// `status` jest `String`em, nie enumem, i to jest decyzja, nie niedbałość: dozwolone wartości
/// pilnuje `CHECK` w schemacie, a nie typ w Ruście (AC-7). Enum tutaj przeniósłby odmowę
/// z `SQLite` do naszego API, czyli dokładnie tam, gdzie nie chroni przed niczym — pierwszy zapis
/// spoza tego API wpuściłby stan, którego UI nie umie narysować.
#[derive(Debug, Clone)]
pub struct NewRun {
    /// uuid v7 — sortuje się po czasie.
    pub id: String,
    /// Który workflow to był.
    pub workflow_id: String,
    /// Kopia grafu **jak biegł**, jako JSON. Bez niej stary bieg po edycji workflow po cichu
    /// zaczyna opowiadać o sobie coś innego [T7 §5.4].
    pub workflow_snapshot: String,
    /// Tytuł widoczny w historii.
    pub title: String,
    /// Jeden z sześciu stanów biegu.
    pub status: String,
    /// Ile kroków naraz miał ten bieg.
    pub concurrency: i64,
    /// Znaczniki czasu, wszystkie w milisekundach epoki.
    pub created_at: i64,
    /// Kiedy ruszył pierwszy krok.
    pub started_at: Option<i64>,
    /// Kiedy skończył się ostatni.
    pub ended_at: Option<i64>,
    /// Powód, jeśli się nie udało.
    pub error: Option<String>,
}

/// Wiersz `steps`. Tabela jest mała (~20 wierszy) i **mutowalna** — to nie jest sprzeczne
/// z append-only `events`, tylko drugą połową tego samego podziału [T7 §5.1].
#[derive(Debug, Clone)]
pub struct NewStep {
    /// uuid v7 kroku.
    pub id: String,
    /// Bieg, do którego należy.
    pub run_id: String,
    /// Stabilny klucz węzła z grafu. Razem z `run_id` jest `UNIQUE`.
    pub node_key: String,
    /// Nazwa po ludzku.
    pub name: String,
    /// `claude` albo `codex`.
    pub agent: String,
    /// Tablica JSON kluczy węzłów, po których ten krok idzie.
    pub depends_on: String,
    /// Jeden z siedmiu stanów kroku (`docs/ARCHITECTURE.md` §5).
    pub status: String,
    /// Które podejście, licząc od zera.
    pub attempt: i64,
    /// Identyfikator sesji przydzielony **z góry**, przed startem procesu [T7 §6.2].
    pub agent_session_id: Option<String>,
    /// Identyfikator procesu potomnego.
    pub pid: Option<i64>,
    /// Grupa procesów — to po niej sprząta odzyskiwanie (T-20).
    pub pgid: Option<i64>,
    /// Kod wyjścia procesu.
    pub exit_code: Option<i64>,
    /// Kiedy ruszył.
    pub started_at: Option<i64>,
    /// Kiedy skończył.
    pub ended_at: Option<i64>,
    /// Ile kosztował. **Musi** stać w `run.json`, inaczej ginie z bazą (niezmiennik 4).
    pub cost_usd: Option<f64>,
    /// Jedna linia dla szyny agentów. Tak samo jak wyżej: plik albo nic.
    pub summary: Option<String>,
    /// Powód, jeśli się nie udało.
    pub error: Option<String>,
}

/// Wiersz `events`. Tabela jest **append-only**, i pilnują tego wyzwalacze w schemacie,
/// nie ten typ.
///
/// `seq` tu nie ma, bo nadaje je `SQLite` (`INTEGER PRIMARY KEY AUTOINCREMENT`) — kolejność jest
/// globalna i monotoniczna, więc nie ma jej kto nadać po stronie ośmiu równoległych producentów.
#[derive(Debug, Clone)]
pub struct NewEvent {
    /// Bieg, do którego zdarzenie należy.
    pub run_id: String,
    /// Krok, jeśli zdarzenie ma krok.
    pub step_id: Option<String>,
    /// Milisekundy epoki.
    pub ts: i64,
    /// Rodzaj zdarzenia po naszej stronie: `assistant`, `tool_use`, `result`, …
    pub kind: String,
    /// `headline`, `detail` albo `raw`. `String`, nie enum, z tego samego powodu, co
    /// [`NewRun::status`]: dozwolone wartości pilnuje `CHECK`, i ma je pilnować także wtedy,
    /// gdy wiersz wchodzi spoza tego API.
    pub level: String,
    /// Treść linii.
    pub body: Option<String>,
}

/// Jedna baza, jedno zadanie piszące, dowolnie wiele czytających.
#[derive(Debug)]
pub struct Store {
    /// Plik bazy. Trzymany, bo [`Store::reader`] otwiera po nim kolejne połączenia.
    path: PathBuf,
    /// Uchwyt do jedynego zadania, które pisze.
    writer: Writer,
    /// Samo zadanie — [`Store::close`] na nie czeka, żeby „zapisane" znaczyło zapisane.
    task: JoinHandle<()>,
}

impl Store {
    /// Otwiera bazę pod `path`, ustawia pragmy, wykonuje `migrate()` i startuje zadanie pisarza.
    ///
    /// Kolejność jest nośna i pochodzi z meetnotes [00-SYNTHESIS §3]: open → pragmy →
    /// `busy_timeout` **na każdym połączeniu** → `migrate()`. Migracja puszczona przed
    /// `foreign_keys` widzi inny świat niż bieg.
    pub fn open(path: &Path) -> Result<Self> {
        // `try_current`, nie `current`: to drugie panikuje poza runtime'em, a panika
        // w agentowym runtime zabiera cały bieg.
        let handle = Handle::try_current().map_err(|_| StoreError::NoRuntime)?;
        let (writer, task) = writer::start(&handle, path)?;
        Ok(Self {
            path: path.to_path_buf(),
            writer,
            task,
        })
    }

    /// Klon uchwytu pisarza. Klonów może być ilu chce — wszystkie prowadzą do **jednego**
    /// zadania i jednego połączenia (niezmiennik 2).
    #[must_use]
    pub fn writer(&self) -> Writer {
        self.writer.clone()
    }

    /// Nowe połączenie **tylko do odczytu**, z tym samym kompletem pragm.
    ///
    /// `SQLITE_OPEN_READ_ONLY` stoi w tej samej linii co `Connection::open_with_flags` nie dla
    /// urody: `checks/quick-boundary.sh` czyta tę linię gerpem i bez flagi uzna ten plik za
    /// drugiego pisarza — i będzie miał rację, bo połączenie bez tej flagi nim jest.
    pub fn reader(&self) -> Result<Connection> {
        let conn = Connection::open_with_flags(&self.path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|source| StoreError::Open {
                path: self.path.clone(),
                source,
            })?;
        apply_pragmas(&conn)?;
        Ok(conn)
    }

    /// Pragmy odczytane z połączenia **pisarza** — jedynego, do którego nikt z zewnątrz nie ma
    /// referencji. Bez tego AC-3 mogłoby sprawdzić trzy połączenia z czterech.
    pub async fn writer_pragmas(&self) -> Result<Pragmas> {
        self.writer.pragmas().await
    }

    /// Odbudowuje **cały** indeks z katalogu biegu. To jest ta ucieczka, o której mówi
    /// `ARCHITECTURE.md` §2 pyt. 2: kasujesz `loadout.db`, wołasz to, dostajesz te same wiersze.
    ///
    /// Czyta `run.json` (bieg i kroki), `logs/agent-<id>.jsonl` (zdarzenia, w kolejności `seq`)
    /// i `handoffs/` (pliki przekazań). Wszystko, czego tu nie ma, **nie istnieje** po skasowaniu
    /// bazy — i to jest jedyny test niezmiennika 4, jaki ten podsystem ma.
    pub async fn rebuild_from(&self, run_dir: &Path) -> Result<()> {
        // SZKIELET (2026-08-16): czytanie katalogu biegu jest całą treścią AC-4, więc tutaj go
        // nie ma. Katalog jest porzucany bez otwarcia ani jednego pliku, do pisarza nie idzie
        // żadne zlecenie, a baza zostaje pusta. Podkreślenie w nazwie znika razem ze szkieletem
        // — sygnatura jest już ta docelowa.
        tracing::debug!(
            run = %run_dir.display(),
            "SZKIELET: the run directory was not read and nothing was indexed"
        );
        // Jedyne `await` w szkielecie. Sygnatura ma być TA, którą wypełni implementacja — test
        // skompilowany dziś przeciwko innej jutro nie skompiluje się wcale — a `async fn` bez
        // `await` przewraca `clippy::unused_async` w pełnej bramce.
        tokio::task::yield_now().await;
        Ok(())
    }

    /// Zamyka kanał i **czeka**, aż pisarz dopisze wszystko, co dostał.
    ///
    /// Bez czekania „zapisane" znaczy tylko „wysłane", a to jest różnica, którą widać dopiero
    /// wtedy, gdy ktoś zamyka aplikację w trakcie biegu.
    pub async fn close(self) -> Result<()> {
        drop(self.writer);
        // Zadanie pisarza nie panikuje ani nie jest anulowane, więc `JoinError` znaczy tu
        // wyłącznie „coś jest bardzo nie tak"; zgłaszamy to jako martwego pisarza, bo dla
        // wołającego skutek jest dokładnie ten.
        self.task.await.map_err(|_| StoreError::WriterGone)?;
        Ok(())
    }
}

/// Ustawia komplet pragm na `conn`. **Jedyne** miejsce, w którym ta polityka mieszka
/// (niezmiennik 23) — konstruktor, który to ominie, jest tym samym błędem, który w meetnotes
/// wracał jako „Save failed" raz na dwa dni.
///
/// `journal_mode` jest własnością bazy i wystarczy raz, ale wołamy go tak samo z każdego
/// połączenia: taniej niż zapamiętać, które połączenie było pierwsze.
pub fn apply_pragmas(_conn: &Connection) -> Result<()> {
    // SZKIELET (2026-08-16): komplet pragm jest całą treścią AC-3, więc tutaj go nie ma.
    // Połączenie wychodzi stąd nietknięte — `busy_timeout` zostaje zerem, klucze obce
    // wyłączone, dziennik w trybie `delete`. Żadnej z tych czterech wartości nie da się na tym
    // szkielecie zobaczyć jako poprawnej. Podkreślenie w nazwie znika razem ze szkieletem —
    // sygnatura jest już ta docelowa.
    tracing::debug!("SZKIELET: no pragma was set on this connection");
    Ok(())
}

/// Odczytuje te same cztery pragmy z **dowolnego** połączenia — także z gołego, otwartego
/// w teście.
///
/// To, że ta funkcja przyjmuje `&Connection`, a nie `&Store`, jest częścią kontraktu AC-3:
/// kontrola przeciw pustej asercji polega na wskazaniu jej na połączenie, o którym wiadomo,
/// że pragm nie ma. Gdyby czytała tylko z naszego typu, nie byłoby czym zmierzyć zera.
pub fn read_pragmas(_conn: &Connection) -> Result<Pragmas> {
    // SZKIELET (2026-08-16): odczyt jest jedną linią na pragmę i powstanie razem z zapisem —
    // rozdzielenie ich dałoby fazę, w której AC-3 pada na czytaniu, a nie na ustawianiu.
    // Zwracamy same zera i pusty `journal_mode`: żadna z czterech asercji AC-3 nie przechodzi,
    // a kontrola na gołym połączeniu (`busy_timeout` = 0, `foreign_keys` = 0) przechodzi
    // przypadkiem i dlatego stoi w teście **po** asercjach pozytywnych.
    tracing::debug!("SZKIELET: no pragma was read back from this connection");
    Ok(Pragmas::default())
}
