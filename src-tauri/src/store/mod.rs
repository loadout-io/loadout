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
//!   domyślnych przy każdym nowym, więc czytelnik, który ominie ten helper, po cichu przestaje
//!   widzieć kaskady. `busy_timeout` zapomniany na połączeniu czytającym objawia się jako losowe
//!   „Save failed" raz na dwa dni i w meetnotes zajęło to dwóch pisarzy w tle, zanim ktoś
//!   zrozumiał, co się dzieje [00-SYNTHESIS §3, „SQLite"].
//!
//!   Czym te wartości domyślne **są**, nie wolno się tu podpierać w żadną stronę, i to jest
//!   blizna, nie ostrożność. Zmierzone 2026-08-16 na rusqlite 0.40.2 z `features = ["bundled"]`
//!   (SQLite 3.53.2): gołe `Connection::open` melduje `foreign_keys` = **1** (ten build idzie
//!   z `-DSQLITE_DEFAULT_FOREIGN_KEYS=1`, podręcznikowy SQLite ma 0) i `busy_timeout` = **5000**
//!   (ustawia je samo rusqlite). Obie są dokładnie tymi, których wymagamy — więc asercja na
//!   naszym konstruktorze przechodzi także wtedy, gdy ten helper nigdy ich nie tknął, a asercja
//!   na gołym połączeniu (`= 0`) pada, choć kod jest dobry. Oba te błędy ta gałąź już popełniła.
//!   Dlatego helper ustawia całą trójkę **jawnie**, a AC-3 dowodzi jej dwustopniowo: najpierw
//!   przestaw na wartość, której nie akceptujemy, potem zawołaj helper.
//! - **„Append-only" egzekwuje `SQLite`, nie Rust.** Wyzwalacz odmawia także połączeniu, które
//!   nigdy nie widziało tego kodu: migracji, skryptowi naprawczemu, przyszłemu daemonowi,
//!   `sqlite3` z terminala. Odmowa napisana w Ruście przechodzi wszystkie nasze testy i nie
//!   chroni przed niczym, bo testowała nasze API.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

pub mod migrate;
/// Katalog biegu → wiersze indeksu. Prywatny, bo jest drogą wejścia do [`Store::rebuild_from`],
/// a nie osobnym API: gdyby ktoś mógł go wywołać z pominięciem [`Store`], mógłby też ominąć
/// jedyne miejsce, które te wiersze niesie do pisarza.
pub(crate) mod rebuild;
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

/// `PRAGMA journal_mode` — jedyna z czwórki, która jest własnością **bazy**, a nie połączenia.
///
/// Zmierzone [T7 §5.3]: zapomniany WAL kosztuje **25×** (2 698 wierszy/s wobec 662 238).
const JOURNAL_MODE_WAL: &str = "wal";

/// `PRAGMA synchronous` = 1. Nazwana stała, bo `1` w wywołaniu nie mówi, że chodzi o `NORMAL`,
/// a sąsiednie `0` i `2` znaczą `OFF` i `FULL`.
const SYNCHRONOUS_NORMAL: i64 = 1;

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
    /// Kiedy wstała maszyna, na której ten bieg ruszył (`sysctl kern.boottime`, sekundy).
    ///
    /// STRAŻNIK odzyskiwania po awarii, nie diagnostyka. `None` znaczy „nie wiadomo" i jest
    /// brakiem strażnika, a nie zgodą na strzał: `recovery::decide` odpowiada wtedy
    /// `NO_BOOT_TIME` i NIC nie zabija. Wypełnia to `commands::run`, bo tylko tam wiadomo,
    /// kiedy bieg naprawdę ruszył.
    pub boot_id: Option<String>,
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

/// Wiersz `artifacts`: **ścieżka** do pliku, który leży na dysku, nigdy jego treść [T7 §5.4].
///
/// Odwrócenie tego — trzymanie bajtów w bazie — czyni bazę drugim źródłem prawdy o treści,
/// która i tak jest w pliku, i łamie niezmiennik 4 tym samym ruchem: skasowanie `loadout.db`
/// zabierałoby wtedy coś, czego nie ma gdzie indziej.
#[derive(Debug, Clone)]
pub struct NewArtifact {
    /// Klucz wyliczony z biegu i ścieżki względnej, nigdy świeży uuid: odbudowa ma dać ten sam
    /// wiersz co pierwsze indeksowanie, a `id` też jest kolumną w porównaniu (AC-4).
    pub id: String,
    /// Bieg, do którego artefakt należy.
    pub run_id: String,
    /// Krok, jeśli artefakt ma krok.
    pub step_id: Option<String>,
    /// Co to jest: `raw_log` dla surowego strumienia agenta, `handoff` dla pliku przekazania.
    pub kind: String,
    /// Nazwa pliku, tak jak stoi na dysku.
    pub name: String,
    /// Ścieżka do pliku.
    pub path: String,
    /// Rozmiar pliku w bajtach.
    pub bytes: Option<i64>,
    /// Kiedy powstał — wyliczone z `run.json`, nigdy z czasu modyfikacji pliku: `mtime` zmienia
    /// się przy kopiowaniu katalogu i wtedy odbudowa daje inny wiersz niż indeksowanie.
    pub created_at: i64,
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
        // Czytanie idzie na pulę blokującą, bo `logs/agent-<id>.jsonl` długiego biegu bywa duży
        // (200 000 zdarzeń to normalna wielkość [T7 §5.3]), a jedyny objaw czytania go wprost na
        // pętli byłby taki, że okno przestaje odpowiadać w chwili otwierania projektu.
        let directory = run_dir.to_path_buf();
        let indexed = tokio::task::spawn_blocking(move || rebuild::read(&directory))
            .await
            // Zadanie blokujące nie panikuje — `panic` jest `deny` w tym drzewie — więc `Err` tu
            // znaczy „bardzo nie tak" i dla wołającego jest tym, czym każdy inny błąd odczytu.
            .map_err(|joined| StoreError::Io(std::io::Error::other(joined)))??;

        // 2026-08-25: odbudowa jest wymianą jednego materializowanego widoku, więc wszystkie
        // cztery kolekcje jadą jednym zleceniem. Kilka zleceń pozwalało czytelnikowi zobaczyć
        // pół starego i pół nowego biegu, a ponowienie zatrzymywało się już na kluczu `runs`.
        self.writer
            .replace_snapshot(
                indexed.run,
                indexed.steps,
                indexed.steps_events,
                indexed.artifacts,
            )
            .await
    }

    /// Zamyka kanał i **czeka**, aż pisarz dopisze wszystko, co dostał.
    ///
    /// Bez czekania „zapisane" znaczy tylko „wysłane", a to jest różnica, którą widać dopiero
    /// wtedy, gdy ktoś zamyka aplikację w trakcie biegu.
    pub async fn close(self) -> Result<()> {
        // Rozbiór na części, bo `writer` musi wysłać zlecenie zamknięcia ZANIM oddamy sterowanie
        // na `task.await`. Samo `drop(writer)` nie wystarcza i nie jest to detal: wołający, który
        // trzyma własny klon [`Writer`] — a trzyma go każdy, kto cokolwiek zapisał — zostawia
        // w kanale żywego nadawcę, więc pętla pisarza nie ma prawa się skończyć i `await` niżej
        // wisi bez końca.
        let Self { writer, task, .. } = self;
        writer.shutdown().await?;
        drop(writer);
        // Zadanie pisarza nie panikuje ani nie jest anulowane, więc `JoinError` znaczy tu
        // wyłącznie „coś jest bardzo nie tak"; zgłaszamy to jako martwego pisarza, bo dla
        // wołającego skutek jest dokładnie ten.
        task.await.map_err(|_| StoreError::WriterGone)?;
        Ok(())
    }
}

/// Ustawia komplet pragm na `conn`. **Jedyne** miejsce, w którym ta polityka mieszka
/// (niezmiennik 23) — konstruktor, który to ominie, jest tym samym błędem, który w meetnotes
/// wracał jako „Save failed" raz na dwa dni.
///
/// `journal_mode` jest własnością bazy i wystarczy raz, ale wołamy go tak samo z każdego
/// połączenia: taniej niż zapamiętać, które połączenie było pierwsze.
pub fn apply_pragmas(conn: &Connection) -> Result<()> {
    // `journal_mode` jako jedyny z czwórki jest własnością BAZY i zapisuje się do pliku, więc
    // jako jedyny wymaga prawa zapisu. Pytamy najpierw, zamiast ustawiać na ślepo, i to nie jest
    // ostrożność na wyrost: połączenie z `Store::reader` jest otwarte tylko do odczytu, a
    // `PRAGMA journal_mode = WAL` na bazie, która NIE jest jeszcze w WAL, odmawia mu słowami
    // „attempt to write a readonly database". Baza już przestawiona przez pisarza odpowiada
    // „wal" bez zapisu, więc pominięcie zdania nie zmienia niczego poza tym, że czytelnik
    // pierwszej w życiu bazy dostaje pragmy zamiast błędu otwarcia projektu.
    let journal: String = conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
    if !journal.eq_ignore_ascii_case(JOURNAL_MODE_WAL) {
        conn.pragma_update(None, "journal_mode", JOURNAL_MODE_WAL)?;
    }

    // Zmierzone [T7 §5.3]: WAL bez `synchronous = NORMAL` to dalej tylko połowa rzeczy.
    // Razem dają 662 238 wierszy/s przy wsadzie 100; sam dziennik rollback daje 2 698.
    conn.pragma_update(None, "synchronous", SYNCHRONOUS_NORMAL)?;

    // Klucze obce są własnością POŁĄCZENIA i wracają do domyślnej wartości przy każdym nowym.
    // Połączenie, które to pominie, przestaje widzieć kaskady — po cichu i bez śladu.
    conn.pragma_update(None, "foreign_keys", 1_i64)?;

    // Na końcu i na KAŻDYM połączeniu, także czytającym [00-SYNTHESIS §3]. Zapomniany na
    // czytelniku objawia się jako losowe „Save failed" raz na dwa dni, a w meetnotes zajęło to
    // dwóch pisarzy w tle, zanim ktokolwiek zrozumiał, co widzi.
    conn.pragma_update(None, "busy_timeout", BUSY_TIMEOUT_MS)?;
    Ok(())
}

/// Odczytuje te same cztery pragmy z **dowolnego** połączenia — także z gołego, otwartego
/// w teście.
///
/// To, że ta funkcja przyjmuje `&Connection`, a nie `&Store`, jest częścią kontraktu AC-3:
/// kontrola przeciw pustej asercji polega na wskazaniu jej na połączenie, o którym wiadomo,
/// że pragm nie ma. Gdyby czytała tylko z naszego typu, nie byłoby czym zmierzyć zera.
pub fn read_pragmas(conn: &Connection) -> Result<Pragmas> {
    // Odczyt idzie prosto do `PRAGMA`, bez ani jednej wartości zapamiętanej po drodze. To jest
    // cała jego wartość: funkcja, która oddawałaby to, co [`apply_pragmas`] miało ustawić,
    // zgadzałaby się sama ze sobą także na połączeniu, którego nikt nigdy nie tknął.
    Ok(Pragmas {
        busy_timeout: conn.query_row("PRAGMA busy_timeout", [], |row| row.get(0))?,
        foreign_keys: conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?,
        synchronous: conn.query_row("PRAGMA synchronous", [], |row| row.get(0))?,
        journal_mode: conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?,
    })
}
