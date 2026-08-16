//! AC-3 dla T-06: każde połączenie dostaje ten sam komplet pragm.
//!
//! Pułapka, o którą się tu potyka każdy, i jedyny powód, dla którego to kryterium istnieje:
//! **`journal_mode` jest własnością BAZY i trwa w pliku, a `busy_timeout`, `foreign_keys`
//! i `synchronous` są własnością POŁĄCZENIA i wracają do wartości domyślnych przy każdym
//! nowym.** `foreign_keys` domyślnie jest **wyłączone**, więc czytelnik, który go nie ustawi,
//! po cichu przestaje widzieć kaskady — i nie dowiaduje się o tym niczym poza brakującym
//! wierszem. `busy_timeout` zapomniany na połączeniu czytającym objawia się jako losowe
//! „Save failed" raz na dwa dni; w meetnotes zajęło to dwóch pisarzy w tle, zanim ktoś
//! zrozumiał, co się dzieje [00-SYNTHESIS §3].
//!
//! **Słaba wersja tego kryterium to sprawdzenie pragm wyłącznie na połączeniu zwróconym przez
//! `open()`.** Przechodzi, gdy `reader()` omija helper i idzie prosto do
//! `Connection::open_with_flags` — czyli w dokładnie tym przypadku, który zabolał w produkcji.
//! Rozróżnia je pętla po **wszystkich** publicznych konstruktorach: `Store::open` razem z jego
//! połączeniem pisarza i `Store::reader`.
//!
//! Czego to kryterium **nie** widzi i trzeba to wiedzieć: konstruktora dodanego jutro, który
//! znowu ominie helper. To zostaje ludzkim osądem i recenzją, i tak ma zostać.
//!
//! Kontrola przeciw pustej asercji stoi na końcu i jest wyjątkowo dosłowna: gołe
//! `Connection::open(path)` do **tej samej** bazy melduje `journal_mode` = `wal`, bo to
//! własność pliku, a jednocześnie `busy_timeout` = 0 i `foreign_keys` = 0, bo to własność
//! połączenia. Jeśli ta kontrola przestanie być czerwona w drugą stronę — to znaczy jeśli gołe
//! połączenie zacznie meldować nasze wartości — asercje wyżej nie mierzą niczego.

use rusqlite::Connection;

use loadout_lib::store::{Pragmas, Store, read_pragmas};

/// Ile milisekund połączenie ma czekać, zanim odda `SQLITE_BUSY` [T7 §5.4].
const BUSY_TIMEOUT: i64 = 5_000;

/// `PRAGMA synchronous` = 1, czyli `NORMAL`. Razem z WAL to jest zmierzona konfiguracja
/// 662 238 wierszy na sekundę; bez WAL 2 698, czyli **25× wolniej** [T7 §5.3].
const SYNCHRONOUS_NORMAL: i64 = 1;

/// Sprawdza komplet na jednym połączeniu i mówi, o które połączenie chodziło.
fn assert_complete(got: &Pragmas, whose: &str) {
    assert_eq!(
        got.busy_timeout, BUSY_TIMEOUT,
        "{whose} carries busy_timeout = {} instead of {BUSY_TIMEOUT}. A connection without it \
         does not wait a single millisecond for another writer and hands back an error the user \
         reads as 'could not save' — while the truth was 'somebody else wrote for three \
         milliseconds'",
        got.busy_timeout
    );
    assert_eq!(
        got.foreign_keys, 1,
        "{whose} carries foreign_keys = {}. SQLite defaults this to OFF on EVERY new connection, \
         so this is not something that stays set: a connection that skips the helper stops \
         seeing cascades and nothing anywhere says so",
        got.foreign_keys
    );
    assert_eq!(
        got.synchronous, SYNCHRONOUS_NORMAL,
        "{whose} carries synchronous = {} instead of {SYNCHRONOUS_NORMAL} (NORMAL)",
        got.synchronous
    );
    assert_eq!(
        got.journal_mode, "wal",
        "{whose} reports journal_mode = {:?}. Forgetting WAL is the one way to get persistence \
         wrong here: measured, it costs 25x — 2 698 rows/s against 662 238 [T7 §5.3]",
        got.journal_mode
    );
}

#[tokio::test]
async fn every_public_constructor_hands_out_the_same_four_pragmas() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("loadout.db");

    let store = Store::open(&db)?;

    // ── Połączenie pisarza ─────────────────────────────────────────────────────────────────
    // Jedyne, do którego nikt z zewnątrz nie ma referencji — i dlatego jedyne, które łatwo
    // pominąć w takim sprawdzeniu.
    assert_complete(&store.writer_pragmas().await?, "the writer connection");

    // ── Store::reader ──────────────────────────────────────────────────────────────────────
    let reader = store.reader()?;
    assert_complete(
        &read_pragmas(&reader)?,
        "the connection from Store::reader()",
    );

    // ── Kontrola przeciw pustej asercji ────────────────────────────────────────────────────
    // Gołe połączenie do TEJ SAMEJ bazy. Jeśli ono raportuje to samo, co dwa wyżej, to znaczy,
    // że nic z tego nie jest naszą zasługą i asercje wyżej nie mierzą niczego.
    let bare = Connection::open(&db)?;
    let untouched = read_pragmas(&bare)?;
    assert_eq!(
        untouched.busy_timeout, 0,
        "a bare Connection::open() reports busy_timeout = {}, not 0. Either SQLite changed its \
         default or read_pragmas() is not reading the connection it was handed — and in both \
         cases the assertions above stopped measuring anything",
        untouched.busy_timeout
    );
    assert_eq!(
        untouched.foreign_keys, 0,
        "a bare Connection::open() reports foreign_keys = {}, not 0. This control is the whole \
         reason the checks above mean something: foreign keys are OFF by default and have to be \
         switched on per connection, every time",
        untouched.foreign_keys
    );
    assert_eq!(
        untouched.journal_mode, "wal",
        "a bare connection to the same file reports journal_mode = {:?}. This one SHOULD match \
         ours: the journal mode belongs to the DATABASE and survives in the file, which is \
         exactly why it is the one pragma nobody forgets and the other three are the ones \
         everybody does",
        untouched.journal_mode
    );

    store.close().await?;
    Ok(())
}
