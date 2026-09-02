//! Ograniczony lokalny dziennik: jeden właściciel zapisów i najwyżej dwie generacje.
//!
//! Osobno od `lib.rs`, bo writer jest czystym, synchronicznym właścicielem plików i daje się
//! wykonać bez uruchamiania okna. Operacje platformowe mieszkają wyłącznie
//! w `engine::supervisor` (niezmiennik 3) — tutaj stoi sama polityka.

use std::fmt;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, Write as _};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use tracing::{Event, Subscriber};
use tracing_subscriber::fmt::format::{DefaultFields, FormatFields as _, Writer};
use tracing_subscriber::fmt::time::{FormatTime as _, SystemTime};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

use crate::engine::supervisor::{
    PrivateFileHandle, PrivateFileModePolicy, PrivateFileProblem, PrivateLeafError, PublicationRoot,
};

/// Nazwa bieżącego dziennika wewnątrz katalogu podanego do `install_logging`.
pub const LOG_FILE: &str = "loadout.log";

/// Nazwa poprzedniej generacji. Trzeciej nie ma i nie będzie: dwie generacje to cały budżet.
pub const LOG_BACKUP_FILE: &str = "loadout.log.1";

/// Sufit każdej z generacji. 8 MiB na plik, więc dziennik zajmuje najwyżej 16 MiB na dysku.
pub const LOG_FILE_LIMIT: usize = 8 * 1024 * 1024;

/// Co człowiek czyta na końcu wpisu, który się nie zmieścił.
pub const LOG_TRUNCATION_MARKER: &str = "[log entry truncated]\n";

/// Dlaczego dziennik nie ruszył. Odmowa jest osobnym wariantem, bo `run()` wypisuje ją
/// człowiekowi i ma powiedzieć, KTÓRY plik i DLACZEGO (niezmiennik 29).
#[derive(Debug, thiserror::Error)]
pub enum LogOpenError {
    #[error("could not open the local log: {0}")]
    Io(#[from] io::Error),
    #[error("refusing unsafe local log file {name}: {problem}")]
    UnsafeFile {
        name: &'static str,
        problem: PrivateFileProblem,
    },
}

/// Jedyna produkcyjna droga zdania o odmowie otwarcia dziennika do sinka diagnostycznego.
/// Test podaje tu bufor, a `run()` prawdziwe stderr, więc oba sądzą dokładnie ten sam zapis.
pub fn write_log_open_error(sink: &mut impl io::Write, error: &io::Error) -> io::Result<()> {
    writeln!(sink, "Loadout could not open its log file: {error}")
}

/// Bieżąca generacja: uchwyt z dowiedzioną tożsamością i znana długość pliku.
#[derive(Debug)]
struct ActiveLog {
    handle: PrivateFileHandle,
    length: u64,
}

/// Stan, którego pilnuje jedyny właściciel zapisów. `root` jest utrzymanym deskryptorem
/// katalogu, więc rotacja nie spaceruje po nazwach, których ktoś może podmienić w trakcie.
#[derive(Debug)]
struct WriterState {
    root: PublicationRoot,
    current: Option<ActiveLog>,
    limit: u64,
}

/// Jeden synchroniczny właściciel obu generacji. Klony dzielą dokładnie ten sam stan, więc
/// „kto teraz rotuje" nie jest pytaniem: mutex jest synchroniczny i nigdy nie przechodzi przez
/// `await` (niezmiennik 8).
#[derive(Clone, Debug)]
pub struct BoundedLogWriter {
    state: Arc<Mutex<WriterState>>,
    limit: usize,
}

/// Produkcyjna warstwa `tracing`, która formatuje zdarzenie WPROST do ograniczonego wpisu.
///
/// Nie używamy `tracing_subscriber::fmt::Layer`: tamta buduje całe zdarzenie w `String`, zanim
/// dojdzie do writera, więc jeden ogromny `Display` alokuje bez ograniczenia jeszcze przed
/// naszym sufitem — czyli dokładnie tam, gdzie nie ma już kto o tym opowiedzieć.
#[derive(Clone, Debug)]
pub struct BoundedLogLayer {
    owner: BoundedLogWriter,
    mirror_to_stderr: bool,
}

impl BoundedLogWriter {
    /// Otwiera dziennik w `dir` i przejmuje wyłączność na zapisy do obu generacji.
    ///
    /// Plik zastany po starszym wydaniu jest zacieśniany przez deskryptor i sprowadzany pod
    /// sufit — nie kasowany. Skasowanie zabrałoby jedyny ślad po ostatniej awarii, a to jest
    /// dokładnie ten plik, po który sięga się po niej jako po pierwszym.
    pub fn open(dir: &Path, limit: usize) -> Result<Self, LogOpenError> {
        if limit < LOG_TRUNCATION_MARKER.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the local log limit is smaller than its truncation marker",
            )
            .into());
        }
        let ceiling = u64::try_from(limit).map_err(io::Error::other)?;
        fs::create_dir_all(dir)?;
        let root = PublicationRoot::open(dir)?;

        let current = match open_named(&root, LOG_FILE, PrivateFileModePolicy::TightenOwnedLegacy)?
        {
            Some(opened) => opened,
            None => root
                .create_private(Path::new(LOG_FILE))
                .map_err(|error| map_open_error(LOG_FILE, error))?,
        };
        repair_legacy_size(&current, ceiling)?;

        if let Some(backup) = open_named(
            &root,
            LOG_BACKUP_FILE,
            PrivateFileModePolicy::TightenOwnedLegacy,
        )? {
            repair_legacy_size(&backup, ceiling)?;
        }

        let length = current.file().metadata()?.len();
        Ok(Self {
            state: Arc::new(Mutex::new(WriterState {
                root,
                current: Some(ActiveLog {
                    handle: current,
                    length,
                }),
                limit: ceiling,
            })),
            limit,
        })
    }

    /// Formatuje wartość bez pośredniego, nieograniczonego `String` i oddaje kompletny wpis
    /// jedynemu właścicielowi zapisów.
    ///
    /// Po osiągnięciu sufitu formatter jest przerywany przez `fmt::Error`, ale taki wewnętrzny
    /// sygnał nasycenia nadal publikuje wpis — z markerem na końcu.
    pub fn write_display(&self, value: &(impl fmt::Display + ?Sized)) -> io::Result<()> {
        let mut entry = BoundedEntry::new(self.limit);
        if let Err(error) = fmt::write(&mut entry, format_args!("{value}"))
            && !entry.is_truncated()
        {
            return Err(io::Error::other(error));
        }
        entry.finish();
        self.write_entry(entry.as_bytes())
    }

    fn write_entry(&self, bytes: &[u8]) -> io::Result<()> {
        lock(&self.state).write_entry(bytes)
    }
}

impl BoundedLogLayer {
    #[must_use]
    pub const fn new(owner: BoundedLogWriter) -> Self {
        Self {
            owner,
            mirror_to_stderr: false,
        }
    }

    /// Aplikacja zachowuje drugi sink diagnostyczny, ale dostaje już ograniczony wpis: stderr
    /// nie uruchamia drugiego formattera i nie materializuje zdarzenia jeszcze raz.
    #[must_use]
    pub const fn with_stderr(mut self) -> Self {
        self.mirror_to_stderr = true;
        self
    }

    fn format_event<S>(&self, event: &Event<'_>, context: &Context<'_, S>) -> BoundedEntry
    where
        S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    {
        let mut entry = BoundedEntry::new(self.owner.limit);
        let mut timestamp = Writer::new(&mut entry);
        let _bounded_or_full = SystemTime.format_time(&mut timestamp);

        if !entry.is_truncated() {
            let metadata = event.metadata();
            let _bounded_or_full =
                write!(entry, " {:>5} {}: ", metadata.level(), metadata.target());
        }

        // Nazwy aktywnych spanów bez magazynowania ich pól w osobnych `String`ach. Same pola
        // zdarzenia niżej nadal idą domyślną, czytelną składnią `tracing`.
        if !entry.is_truncated()
            && let Some(scope) = context.event_scope(event)
        {
            for span in scope.from_root() {
                if write!(entry, "{}:", span.metadata().name()).is_err() {
                    break;
                }
            }
        }

        if !entry.is_truncated() {
            let fields = DefaultFields::new();
            let _bounded_or_full = fields.format_fields(Writer::new(&mut entry), event);
        }
        if !entry.is_truncated() {
            let _bounded_or_full = entry.write_char('\n');
        }
        entry.finish();
        entry
    }
}

impl<S> Layer<S> for BoundedLogLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_event(&self, event: &Event<'_>, context: Context<'_, S>) {
        let entry = self.format_event(event, &context);

        // Warstwa `tracing` nie ma kanału błędu, a opisanie awarii dziennika TYM SAMYM
        // dziennikiem to rekurencja, która kończy się stosem. Odrzucamy dokładnie to jedno
        // zdarzenie; następne spróbuje ponownie.
        let _dropped_error = self.owner.write_entry(entry.as_bytes());

        if self.mirror_to_stderr {
            let mut stderr = io::stderr().lock();
            let _diagnostic_error = stderr.write_all(entry.as_bytes());
            let _diagnostic_error = stderr.flush();
        }
    }
}

impl WriterState {
    fn write_entry(&mut self, bytes: &[u8]) -> io::Result<()> {
        let entry_length = u64::try_from(bytes.len()).map_err(io::Error::other)?;
        if entry_length > self.limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the bounded log formatter exceeded its configured limit",
            ));
        }
        self.adopt_current()?;

        // 2026-08-31: `write_all` potrafi zapisać prefiks i DOPIERO POTEM zwrócić błąd, więc
        // długość z pamięci nie jest autorytetem. Przed każdą decyzją o rotacji odtwarzamy ją
        // z utrzymanego deskryptora i domykamy sufit bez ponownego otwierania nazwy.
        let length = {
            let current = self.current.as_mut().ok_or_else(missing_current)?;
            let actual = current.handle.file().metadata()?.len();
            if actual > self.limit {
                current.handle.file().set_len(self.limit)?;
                current.handle.file().sync_all()?;
                current.length = self.limit;
            } else {
                current.length = actual;
            }
            current.length
        };

        // Równość też obraca niepusty plik. Dzięki temu żaden wpis nie zaczyna się na pliku,
        // który jest już pełny — a kontrolowana odmowa zostawia go nietkniętym.
        if length > 0 && length.saturating_add(entry_length) >= self.limit {
            self.rotate()?;
        }

        let current = self.current.as_mut().ok_or_else(missing_current)?;
        current.handle.file_mut().write_all(bytes)?;
        current.handle.file_mut().flush()?;
        current.length = current.length.saturating_add(entry_length);
        Ok(())
    }

    /// Dowodzi, że nazwa nadal wskazuje NASZ inode, a jeśli nie — zakłada uchwyt od nowa.
    ///
    /// Skasowanie `loadout.log` w trakcie biegu nie ma prawa uciszyć dziennika do końca biegu:
    /// luka jest wadą, cisza jest awarią. Podmiana na coś, co nie jest prywatnym plikiem tego
    /// użytkownika, zostaje odmową — uchwyt zostaje, a zdarzenie przepada (2026-08-31).
    fn adopt_current(&mut self) -> io::Result<()> {
        let held = self
            .current
            .as_ref()
            .map(|current| current.handle.identity());
        if let Some(identity) = held {
            if self
                .root
                .validate_private_identity(Path::new(LOG_FILE), identity)
                .map_err(private_leaf_io)?
            {
                return Ok(());
            }
            self.current = None;
        }

        // `ExactOwnerOnly`, nie `TightenOwnedLegacy`: zacieśnianie zastanego pliku jest
        // uprzejmością STARTU, a nie zachowaniem biegu. Plik, który w trakcie biegu nagle ma
        // szersze prawa, jest podmianą, nie spadkiem.
        let handle = match self
            .root
            .open_private_existing(Path::new(LOG_FILE), PrivateFileModePolicy::ExactOwnerOnly)
            .map_err(private_leaf_io)?
        {
            Some(existing) => existing,
            None => self
                .root
                .create_private(Path::new(LOG_FILE))
                .map_err(private_leaf_io)?,
        };
        let length = handle.file().metadata()?.len();
        self.current = Some(ActiveLog { handle, length });
        Ok(())
    }

    /// Przenosi całą bieżącą generację pod nazwę backupu i zakłada nową.
    ///
    /// Nic tu nie sprząta po nieudanym `renameat`: kiedy sama nazwa zmieni właściciela, kolejny
    /// wpis zauważy to w [`WriterState::adopt_current`] i założy plik od nowa. Ratowanie stanu
    /// w ciemno w TYM miejscu wymagałoby zgadywania, po której stronie rename się zatrzymał.
    fn rotate(&mut self) -> io::Result<()> {
        let current = self.current.as_ref().ok_or_else(missing_current)?;
        current.handle.file().sync_all()?;
        let identity = current.handle.identity();

        self.root
            .replace_private_name(Path::new(LOG_FILE), Path::new(LOG_BACKUP_FILE), identity)
            .map_err(private_leaf_io)?;

        self.current = None;
        self.adopt_current()
    }
}

fn open_named(
    root: &PublicationRoot,
    name: &'static str,
    mode_policy: PrivateFileModePolicy,
) -> Result<Option<PrivateFileHandle>, LogOpenError> {
    root.open_private_existing(Path::new(name), mode_policy)
        .map_err(|error| map_open_error(name, error))
}

/// Sprowadza zastany plik pod sufit. Zostaje POCZĄTEK, bo pierwszy wpis po starcie i tak obróci
/// ten plik do backupu — czyli cała zastana treść zostaje, a nowe logowanie zaczyna od pustego.
fn repair_legacy_size(handle: &PrivateFileHandle, limit: u64) -> io::Result<()> {
    if handle.file().metadata()?.len() > limit {
        handle.file().set_len(limit)?;
        handle.file().sync_all()?;
    }
    Ok(())
}

fn map_open_error(name: &'static str, error: PrivateLeafError) -> LogOpenError {
    match error {
        PrivateLeafError::Unsafe(problem) => LogOpenError::UnsafeFile { name, problem },
        PrivateLeafError::Io(error) => LogOpenError::Io(error),
    }
}

fn private_leaf_io(error: PrivateLeafError) -> io::Error {
    match error {
        PrivateLeafError::Unsafe(problem) => {
            io::Error::new(io::ErrorKind::PermissionDenied, problem)
        }
        PrivateLeafError::Io(error) => error,
    }
}

fn missing_current() -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        "the current local log is unavailable",
    )
}

/// Bufor rośnie wyłącznie do sufitu. Pierwsza próba przekroczenia zwraca `fmt::Error`, więc
/// upstream przerywa ogromny `Display`; marker ląduje na poprawnej granicy UTF-8.
#[derive(Debug)]
struct BoundedEntry {
    bytes: Vec<u8>,
    limit: usize,
    truncated: bool,
    finished: bool,
}

impl BoundedEntry {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(limit.min(4 * 1024)),
            limit,
            truncated: false,
            finished: false,
        }
    }

    fn push_bytes(&mut self, incoming: &[u8]) -> fmt::Result {
        if self.truncated {
            return Err(fmt::Error);
        }
        let retained = incoming
            .len()
            .min(self.limit.saturating_sub(self.bytes.len()));
        self.reserve_for(retained);
        self.bytes.extend_from_slice(&incoming[..retained]);
        if retained != incoming.len() {
            self.truncated = true;
            return Err(fmt::Error);
        }
        Ok(())
    }

    fn reserve_for(&mut self, additional: usize) {
        let required = self.bytes.len().saturating_add(additional);
        if required > self.bytes.capacity() {
            self.bytes.reserve_exact(required - self.bytes.len());
        }
    }

    /// Domyka wpis: ucięty dostaje marker, a granica cięcia jest granicą ZNAKU, nie bajtu —
    /// inaczej plik przestałby być czytelny jako UTF-8 na jednym oberwanym „ż".
    fn finish(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        if !self.truncated && std::str::from_utf8(&self.bytes).is_ok() {
            return;
        }
        self.truncated = true;
        let payload_limit = self.limit.saturating_sub(LOG_TRUNCATION_MARKER.len());
        let candidate = self.bytes.len().min(payload_limit);
        let valid = match std::str::from_utf8(&self.bytes[..candidate]) {
            Ok(_whole) => candidate,
            Err(error) => error.valid_up_to(),
        };
        self.bytes.truncate(valid);
        self.reserve_for(LOG_TRUNCATION_MARKER.len());
        self.bytes
            .extend_from_slice(LOG_TRUNCATION_MARKER.as_bytes());
    }

    fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    const fn is_truncated(&self) -> bool {
        self.truncated
    }
}

impl fmt::Write for BoundedEntry {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.push_bytes(value.as_bytes())
    }
}

/// Mutex jest synchroniczny i nigdy nie przechodzi przez `await` (niezmiennik 8). Zatrucie
/// odzyskujemy: dziennik, który zamilkł po cudzej panice, jest gorszy niż dziennik z luką.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
