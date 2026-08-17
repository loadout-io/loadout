//! Kryterium 5 dla T-01: dziennik przeżywa panikę i nie zjada deskryptorów.
//!
//! JEDNA funkcja `#[test]` i trzy fazy, nie trzy funkcje. Globalny subskrybent `tracing` ustawia
//! się RAZ na uruchomienie binarki testowej, więc druga instalacja zwróciłaby błąd i test
//! przewróciłby się z powodu, który nie ma nic wspólnego z badanym kodem.
//!
//! Czego ten plik świadomie NIE robi: nie asertuje „plik istnieje i jest niepusty". Taka asercja
//! przechodzi na haku, który ZASTĘPUJE poprzedni (pierwsza panika w release i tak nie zostawi
//! śladu), i przechodzi na pisarzu klonującym uchwyt na każdą linijkę. Rozróżniają je dwie rzeczy
//! niżej: flaga wartownika po panice i stała liczba otwartych plików przy 1600 liniach z ośmiu
//! wątków [T8 §9, incydent Murmura z `dup(2)` na linijkę].
//!
//! Plik dziennika czytamy przez `unwrap_or_default()` — test ma paść na asercji o TREŚCI,
//! nigdy na otwarciu pliku (AGENTS.md §2a p. 5).

use std::collections::HashSet;
use std::fs;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

/// Znaczniki mają w środku cyfrę, żeby nie dało się ich pomylić ze zwykłym słowem, które
/// `tracing` sam wstawia w linię (czas, poziom, cel).
const WARMUP: &str = "m4rk-warmup";
const STORM: &str = "m4rk-storm";
const BOOM: &str = "s3ntinel-boom";

const WORKERS: usize = 8;
const PER_WORKER: usize = 200;

/// Wartownik fazy 3. Hak, który T-01 instaluje, ma ten hak ZAWOŁAĆ, a nie zastąpić.
static SENTINEL: AtomicBool = AtomicBool::new(false);

/// Liczba otwartych plików, mierzona zawsze tą samą metodą. Sam odczyt katalogu też otwiera
/// wpis — offset jest wtedy identyczny przed i po, więc skraca się w porównaniu.
fn open_files() -> io::Result<usize> {
    Ok(fs::read_dir("/dev/fd")?.count())
}

#[test]
fn the_log_survives_a_panic_and_keeps_one_file_handle() -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let log = loadout_lib::install_logging(home.path())?;

    // ── Faza 1: trzy zdarzenia to trzy linie w pliku, którego ścieżkę zwrócono ──────────────
    for i in 0..3 {
        tracing::info!("{} {}", WARMUP, i);
    }
    let body = fs::read_to_string(&log).unwrap_or_default();
    let warmup = body.lines().filter(|line| line.contains(WARMUP)).count();
    assert_eq!(
        warmup, 3,
        "three events have to land as three lines in the file install_logging returned; \
         that file holds {warmup} of them"
    );

    // ── Faza 2: 1600 kompletnych linii z ośmiu wątków, przy stałej liczbie otwartych plików ─
    let before = open_files()?;
    std::thread::scope(|scope| {
        for worker in 0..WORKERS {
            scope.spawn(move || {
                for index in 0..PER_WORKER {
                    tracing::info!("{} {}-{}", STORM, worker, index);
                }
            });
        }
    });
    let after = open_files()?;
    assert_eq!(
        before, after,
        "writing 1600 lines from eight threads has to leave the same number of open files as \
         it started with ({before} before, {after} after); a writer that clones the handle per \
         line is how Murmur ran out of them inside the logging code itself"
    );

    let body = fs::read_to_string(&log).unwrap_or_default();
    let mut marks: HashSet<(usize, usize)> = HashSet::new();
    let mut whole = 0usize;
    for line in body.lines().filter(|line| line.contains(STORM)) {
        whole += 1;
        assert_eq!(
            line.matches(STORM).count(),
            1,
            "two writes ran into one another and produced a line carrying the mark twice: {line}"
        );
        let tail = line.rsplit(STORM).next().unwrap_or_default().trim();
        let mark = tail.split_once('-').and_then(|(worker, index)| {
            Some((worker.parse::<usize>().ok()?, index.parse::<usize>().ok()?))
        });
        assert!(
            mark.is_some(),
            "a line came out cut short or run together with another one: {line}"
        );
        if let Some(pair) = mark {
            marks.insert(pair);
        }
    }
    let expected = WORKERS * PER_WORKER;
    assert_eq!(
        whole, expected,
        "eight threads writing 200 events each have to leave {expected} lines, not {whole}"
    );
    assert_eq!(
        marks.len(),
        expected,
        "every one of the {expected} lines has to read on its own; {} of them did",
        marks.len()
    );

    // ── Faza 3: hak paniki woła poprzedni hak i zostawia ślad w pliku ───────────────────────
    std::panic::set_hook(Box::new(|_| {
        SENTINEL.store(true, Ordering::SeqCst);
    }));
    loadout_lib::install_panic_hook();
    let boom = std::panic::catch_unwind(|| {
        unreachable!("{}", BOOM);
    });
    assert!(
        boom.is_err(),
        "the deliberate panic has to unwind, otherwise the rest of this phase measures nothing"
    );
    assert!(
        SENTINEL.load(Ordering::SeqCst),
        "install_panic_hook replaced the hook that was already in place instead of calling it; \
         the first panic in a release build then leaves no trace at all"
    );
    let body = fs::read_to_string(&log).unwrap_or_default();
    assert!(
        body.contains(BOOM),
        "the panic has to reach the file, because the default hook writes only to the output \
         LaunchServices throws away"
    );

    Ok(())
}
