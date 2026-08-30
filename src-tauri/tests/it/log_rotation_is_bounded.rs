//! Lokalny dziennik ma sufit, dwie generacje i odmawia plikom, które nie są jego.
//!
//! Sześć faz w JEDNEJ funkcji `#[test]`, bo kolejność jest częścią dowodu, a nie porządkiem
//! czytania. Faza rotacji stoi pierwsza z premedytacją: dopóki writer jest append-only, dalsze
//! fazy nie mają prawa się wykonać. Faza odmowy woła `install_logging`, a to na starym kodzie
//! poszłoby za dowiązaniem i **ustawiło globalny subskrybent** — czyli zmieniłoby stan procesu
//! dla 285 sąsiadów w tym samym binarium. Przerwanie na pierwszej asercji jest tu ochroną.
//!
//! Dlatego też ten plik nigdy nie instaluje globalnego subskrybenta ani globalnego haka paniki:
//! warstwę odpalamy przez `tracing::subscriber::with_default`, które jest lokalne dla wątku.
//! Wyrocznią „hak paniki nadal żyje" pozostaje `tests/shell_logging.rs` — osobny cel, właśnie
//! dlatego, że mierzy stan CAŁEGO procesu.

use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _, symlink};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use loadout_lib::engine::supervisor::{
    PrivateFileDisposition, PrivateFileFacts, PrivateFileKind, PrivateFileModePolicy,
    PrivateFileProblem, validate_private_file_facts,
};
use loadout_lib::install_logging;
use loadout_lib::logging::{
    BoundedLogLayer, BoundedLogWriter, LOG_BACKUP_FILE, LOG_FILE, LOG_FILE_LIMIT,
    LOG_TRUNCATION_MARKER, write_log_open_error,
};
use tracing_subscriber::layer::SubscriberExt as _;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

/// Sufit na potrzeby kryterium. Produkcyjne 8 MiB opisuje tę samą politykę; mały sufit sprawia,
/// że rotacja jest faktem, a nie obietnicą na plik, którego test nigdy nie zapełni.
const LIMIT: usize = 192;

/// Znak z tekstem, który człowiek rozpozna w pliku, i cyfra w środku, żeby nie dało się go
/// pomylić ze słowem, które `tracing` sam wstawia w linię.
const RETRY_MARK: &str = "r3try-after-repair";

/// `Display`, który oddaje treść po dwa bajty i liczy, ile razy go o nie poproszono.
///
/// Pełna reprezentacja NIGDY nie istnieje po stronie fikstury. Bez tego mały plik na końcu
/// nie odróżniłby ograniczonego formattera od takiego, który najpierw zbudował cały `String`
/// w pamięci, a dopiero potem go przyciął.
struct IncrementalUtf8 {
    repeats: usize,
    formatted_chunks: Arc<AtomicUsize>,
}

impl fmt::Display for IncrementalUtf8 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for _ in 0..self.repeats {
            self.formatted_chunks.fetch_add(1, Ordering::SeqCst);
            formatter.write_str("ż")?;
        }
        Ok(())
    }
}

#[test]
fn the_local_log_keeps_two_bounded_generations_and_refuses_unsafe_files() -> TestResult {
    assert_eq!(
        LOG_FILE_LIMIT,
        8 * 1024 * 1024,
        "the production log has to keep its exact 8 MiB per-generation ceiling"
    );
    rotation_keeps_whole_entries()?;
    concurrent_clones_share_one_bounded_pair()?;
    an_oversized_event_lands_as_one_bounded_utf8_entry()?;
    a_wide_open_legacy_log_is_tightened_and_bounded()?;
    unsafe_log_files_are_refusals_that_name_the_file()?;
    a_refused_rotation_changes_nothing_and_the_next_event_lands_once()?;
    Ok(())
}

/// Rotacja zachodzi na granicy KOMPLETNEGO wpisu: poprzednia generacja idzie do backupu
/// w całości, a nowy plik zaczyna się wpisem, który tę rotację wywołał.
fn rotation_keeps_whole_entries() -> TestResult {
    let home = tempfile::tempdir()?;
    let writer = BoundedLogWriter::open(home.path(), LIMIT)?;
    let trigger = "rotation-trigger-stays-whole\n";
    let older = "a".repeat(LIMIT - trigger.len() + 1);

    writer.write_display(&older)?;
    writer.write_display(trigger)?;

    assert_eq!(
        fs::read(home.path().join(LOG_BACKUP_FILE)).unwrap_or_default(),
        older.as_bytes(),
        "the older generation has to move to {LOG_BACKUP_FILE} whole; an append-only writer \
         leaves no backup at all and glues both entries into one file"
    );
    assert_eq!(
        fs::read(home.path().join(LOG_FILE))?,
        trigger.as_bytes(),
        "the entry that triggered rotation has to start the new {LOG_FILE}, whole and once"
    );
    assert_pair_bounded(home.path(), LIMIT)?;
    Ok(())
}

/// Cztery klony i więcej, wszystkie piszące naraz: jeden właściciel zapisów znaczy, że żaden
/// wpis nie wjeżdża w cudzy i że para plików nadal ma sufit.
fn concurrent_clones_share_one_bounded_pair() -> TestResult {
    let home = tempfile::tempdir()?;
    let writer = BoundedLogWriter::open(home.path(), LIMIT)?;
    let refusals = Arc::new(Mutex::new(Vec::<String>::new()));

    std::thread::scope(|scope| {
        for worker in 0..6 {
            let writer = writer.clone();
            let refusals = Arc::clone(&refusals);
            scope.spawn(move || {
                for event in 0..80 {
                    let line = format!("worker-{worker}-{event}-payload\n");
                    if let Err(error) = writer.write_display(&line) {
                        lock(&refusals).push(error.to_string());
                    }
                }
            });
        }
    });

    let seen = lock(&refusals).clone();
    assert!(
        seen.is_empty(),
        "six clones writing at once must not lose an entry to an error: {seen:?}"
    );
    assert_pair_bounded(home.path(), LIMIT)?;

    let retained = String::from_utf8(retained_pair(home.path())?)?;
    let mut whole = 0usize;
    for line in retained.lines().filter(|line| !line.is_empty()) {
        whole += 1;
        assert_eq!(
            line.matches("worker-").count(),
            1,
            "two entries formatted at once ran into one another: {line}"
        );
        assert!(
            line.ends_with("-payload"),
            "an entry came out cut short or joined to its neighbour: {line}"
        );
    }
    assert!(whole > 1, "the bounded pair kept no complete entry at all");
    Ok(())
}

/// Zdarzenie większe od sufitu jest JEDNYM wpisem: plik czyta się jako UTF-8, a ostatnie bajty,
/// które widzi człowiek, to marker. Idzie prawdziwym `tracing`, nie skrótem przez writer.
fn an_oversized_event_lands_as_one_bounded_utf8_entry() -> TestResult {
    let home = tempfile::tempdir()?;
    let writer = BoundedLogWriter::open(home.path(), LIMIT)?;
    let formatted_chunks = Arc::new(AtomicUsize::new(0));
    let huge = IncrementalUtf8 {
        repeats: LIMIT * 40,
        formatted_chunks: Arc::clone(&formatted_chunks),
    };
    let subscriber = tracing_subscriber::registry().with(BoundedLogLayer::new(writer));

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(payload = %huge, "oversized-event");
    });

    let asked = formatted_chunks.load(Ordering::SeqCst);
    assert!(
        asked <= LIMIT / "ż".len() + 1,
        "the layer asked for {asked} chunks before a {LIMIT}-byte ceiling; a writer fed by an \
         unbounded String would have asked for all {} of them",
        huge.repeats
    );

    let body = String::from_utf8(fs::read(home.path().join(LOG_FILE))?)?;
    assert!(
        body.ends_with(LOG_TRUNCATION_MARKER),
        "the last bytes a human reads have to say why the entry stops: {body}"
    );
    assert_eq!(
        body.matches(LOG_TRUNCATION_MARKER).count(),
        1,
        "one event has to land as one entry, with one marker"
    );
    assert_pair_bounded(home.path(), LIMIT)?;
    Ok(())
}

/// Zastany `loadout.log` z prawami `0644` i treścią ponad sufit: prawa zacieśnione przez
/// deskryptor, rozmiar sprowadzony pod sufit, a po pierwszej rotacji stoją dokładnie dwa pliki.
fn a_wide_open_legacy_log_is_tightened_and_bounded() -> TestResult {
    let home = tempfile::tempdir()?;
    let legacy = u64::try_from(LIMIT * 64)?;
    seed_legacy_file(&home.path().join(LOG_FILE), legacy)?;
    seed_legacy_file(&home.path().join(LOG_BACKUP_FILE), legacy)?;

    let writer = BoundedLogWriter::open(home.path(), LIMIT)?;
    assert_eq!(
        mode_of(&home.path().join(LOG_FILE))?,
        0o600,
        "a log left behind by an older release has to be tightened before the first write"
    );
    assert_pair_bounded(home.path(), LIMIT)?;

    writer.write_display("legacy-generation-still-rotates\n")?;
    assert_pair_bounded(home.path(), LIMIT)?;
    Ok(())
}

/// Dowiązanie, katalog i plik obcego właściciela są odmowami, a zdanie, które człowiek czyta,
/// nazywa plik i powód. Bajty i prawa celu dowiązania zostają nietknięte.
fn unsafe_log_files_are_refusals_that_name_the_file() -> TestResult {
    let linked = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    let victim = outside.path().join("victim.log");
    fs::write(&victim, b"outside-bytes")?;
    let victim_mode = mode_of(&victim)?;
    symlink(&victim, linked.path().join(LOG_FILE))?;
    assert_refusal_names(linked.path(), PrivateFileKind::Symlink)?;
    assert_eq!(
        fs::read(&victim)?,
        b"outside-bytes",
        "refusing a symlink must not write through it"
    );
    assert_eq!(
        mode_of(&victim)?,
        victim_mode,
        "refusing a symlink must not chmod what it points at"
    );

    let taken = tempfile::tempdir()?;
    fs::create_dir(taken.path().join(LOG_FILE))?;
    assert_refusal_names(taken.path(), PrivateFileKind::Directory)?;

    // Inode należący do obcego UID-a wymaga roota, więc gałąź „obcy właściciel" wykonujemy na
    // tym samym rdzeniu polityki, którego opener używa na deskryptorze (niezmiennik 23).
    let mine = fs::metadata(taken.path())?.uid();
    let foreign = mine.wrapping_add(1);
    assert_eq!(
        validate_private_file_facts(
            PrivateFileFacts {
                kind: PrivateFileKind::Regular,
                owner: foreign,
                mode: 0o600,
            },
            mine,
            PrivateFileModePolicy::TightenOwnedLegacy,
        ),
        Err(PrivateFileProblem::ForeignOwner {
            actual: foreign,
            expected: mine,
        }),
        "a file belonging to somebody else is a refusal, never a file to tighten"
    );
    assert_eq!(
        validate_private_file_facts(
            PrivateFileFacts {
                kind: PrivateFileKind::Regular,
                owner: mine,
                mode: 0o644,
            },
            mine,
            PrivateFileModePolicy::TightenOwnedLegacy,
        ),
        Ok(PrivateFileDisposition::TightenPermissions),
        "our own wide-open log is tightened, not refused"
    );
    Ok(())
}

/// Rotacja, która się nie udała, nie zmienia ani jednego bajtu, nie opisuje siebie w dzienniku
/// i nie zamyka drogi następnemu zdarzeniu.
fn a_refused_rotation_changes_nothing_and_the_next_event_lands_once() -> TestResult {
    let home = tempfile::tempdir()?;
    let writer = BoundedLogWriter::open(home.path(), LIMIT)?;
    let current_path = home.path().join(LOG_FILE);
    let backup_path = home.path().join(LOG_BACKUP_FILE);

    writer.write_display(&"g1-".repeat(60))?;
    writer.write_display("g2-second-generation\n")?;
    writer.write_display(&"f-".repeat(80))?;
    let current_before = fs::read(&current_path)?;
    let backup_before = fs::read(&backup_path)?;
    fs::set_permissions(&backup_path, fs::Permissions::from_mode(0o644))?;

    emit_through_the_layer(&writer);
    assert_eq!(
        fs::read(&current_path)?,
        current_before,
        "a refused rotation must not append the triggering entry to a full current log"
    );
    assert_eq!(
        fs::read(&backup_path)?,
        backup_before,
        "a refused rotation must leave the previous backup byte for byte"
    );
    assert_eq!(
        occurrences(&retained_pair(home.path())?, RETRY_MARK.as_bytes()),
        0,
        "the failed rotation must not describe itself in the very log that failed"
    );

    fs::set_permissions(&backup_path, fs::Permissions::from_mode(0o600))?;
    emit_through_the_layer(&writer);
    assert_eq!(
        fs::read(&backup_path)?,
        current_before,
        "the repaired rotation has to move the whole previous generation to the backup"
    );
    assert_eq!(
        occurrences(&retained_pair(home.path())?, RETRY_MARK.as_bytes()),
        1,
        "the first event after the repair has to land in {LOG_FILE} exactly once"
    );
    assert_pair_bounded(home.path(), LIMIT)?;
    Ok(())
}

/// Jedno zdarzenie przez produkcyjną warstwę, lokalnie dla tego wątku. Warstwa nie ma kanału
/// błędu, więc to ona — a nie test — decyduje, co się dzieje z odmową.
fn emit_through_the_layer(writer: &BoundedLogWriter) {
    let subscriber = tracing_subscriber::registry().with(BoundedLogLayer::new(writer.clone()));
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("{RETRY_MARK}");
    });
}

/// Zdanie, które człowiek dostaje na wyjściu diagnostycznym, sądzone przez tę samą produkcyjną
/// drogę co stderr w `run()`, tylko z buforem zamiast terminala (niezmiennik 29).
fn assert_refusal_names(dir: &Path, expected: PrivateFileKind) -> TestResult {
    let error = match install_logging(dir) {
        Err(error) => error,
        Ok(path) => {
            return Err(format!(
                "logging started on an unsafe {expected:?} at {}",
                path.display()
            )
            .into());
        }
    };
    let mut visible = Vec::new();
    write_log_open_error(&mut visible, &error)?;
    assert_eq!(
        visible,
        format!(
            "Loadout could not open its log file: refusing unsafe local log file {LOG_FILE}: \
             the private path is not a regular file ({expected:?})\n"
        )
        .as_bytes(),
        "the sentence a human reads has to name the file and the reason"
    );
    Ok(())
}

/// Plik zostawiony przez starsze wydanie: szerokie prawa i rozmiar ponad sufit.
fn seed_legacy_file(path: &Path, length: u64) -> io::Result<()> {
    let file = File::create(path)?;
    file.set_len(length)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o644))
}

fn mode_of(path: &Path) -> io::Result<u32> {
    Ok(fs::metadata(path)?.permissions().mode() & 0o777)
}

/// Dwa pliki, oba pod sufitem, oba tylko dla właściciela — i ani jednego trzeciego.
fn assert_pair_bounded(dir: &Path, limit: usize) -> TestResult {
    let ceiling = u64::try_from(limit)?;
    let mut seen = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        assert!(
            name == LOG_FILE || name == LOG_BACKUP_FILE,
            "the log directory holds a third generation or a leftover temp: {name}"
        );
        let metadata = fs::symlink_metadata(entry.path())?;
        assert!(
            metadata.file_type().is_file(),
            "{name} has to be a regular file, never a symlink or another filesystem object"
        );
        assert!(
            metadata.len() <= ceiling,
            "{name} grew past the {limit}-byte ceiling"
        );
        assert_eq!(
            metadata.permissions().mode() & 0o777,
            0o600,
            "{name} is readable by somebody other than its owner"
        );
        seen.push(name);
    }
    assert!(
        seen.iter().any(|name| name == LOG_FILE),
        "the current log has to exist after logging started"
    );
    Ok(())
}

/// Obie generacje w kolejności, w jakiej czyta je człowiek: najpierw starsza.
fn retained_pair(dir: &Path) -> io::Result<Vec<u8>> {
    let mut retained = Vec::new();
    for name in [LOG_BACKUP_FILE, LOG_FILE] {
        match fs::read(dir.join(name)) {
            Ok(mut bytes) => retained.append(&mut bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(retained)
}

fn occurrences(haystack: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() || needle.len() > haystack.len() {
        return 0;
    }
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

/// Mutex fikstury nie przechodzi przez `await`; odzyskanie zatrucia zachowuje wcześniejsze błędy.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
