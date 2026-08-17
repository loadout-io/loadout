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
//! Kontrola przeciw pustej asercji stoi na końcu i **nie jest symetryczna**, bo świat taki nie
//! jest. Gołe `Connection::open(path)` do tej samej bazy melduje `journal_mode` = `wal` (własność
//! pliku) i `foreign_keys` = 0 (własność połączenia, domyślnie wyłączone) — te dwie kontrole
//! działają wprost. Ale `busy_timeout` **nie** wraca do zera: rusqlite 0.40.2 ustawia
//! pięciosekundowy timeout sam, przy otwarciu, czyli dokładnie tyle, ile wymagamy. Zmierzone
//! 2026-08-16, po tym, jak `assert_eq!(untouched.busy_timeout, 0)` padło na bramce. Ta pragma
//! dostaje więc kontrolę DWUSTOPNIOWĄ, opisaną przy niej samej.

use rusqlite::Connection;

use loadout_lib::store::{Pragmas, Store, apply_pragmas, read_pragmas};

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
    // Zamysl oryginalny: gole `Connection::open` pokazuje DOMYSLNE wartosci, wiec jesli melduje
    // to samo, co nasze konstruktory, to nic z tego nie jest nasza zasluga.
    //
    // Na TYM stosie ten zamysl nie dziala, i to jest blizna. Zmierzone 2026-08-16 na rusqlite
    // 0.40.2 z `features = ["bundled"]` (SQLite 3.53.2): gole polaczenie melduje
    // `busy_timeout` = 5000, bo rusqlite ustawia je samo przy otwarciu, oraz `foreign_keys` = 1,
    // bo bundlowany SQLite jest kompilowany z wlaczonymi kluczami obcymi. Obie te wartosci sa
    // DOKLADNIE tymi, ktorych wymagamy wyzej -- wiec `assert_complete` przechodzilby dla nich
    // takze wtedy, gdyby `apply_pragmas` nigdy ich nie tknelo. Pusta asercja schowana w kryterium
    // napisanym po to, zeby puste asercje lapac.
    //
    // Kontrola nie porownuje sie wiec z domyslnymi wartosciami stosu -- one moga byc jakiekolwiek
    // i moga sie zmienic przy nastepnym `cargo update`. Dowodzi DWOCH rzeczy o kazdej pragmie
    // POLACZENIA z osobna:
    //   (a) czytnik czyta POLACZENIE, a nie pamieta wartosci: ustaw jawnie wartosc, ktorej NIE
    //       akceptujemy, i wymagaj, zeby odczyt ja zobaczyl;
    //   (b) to NASZ kod ustawia wlasciwa: zawolaj `apply_pragmas` na TYM SAMYM polaczeniu.
    // Bez (a) krok (b) nie dowodzi niczego, bo dobra wartosc mogla tam byc od poczatku.
    let bare = Connection::open(&db)?;

    // `journal_mode` zostaje przy starej kontroli i to jest poprawne: nalezy do BAZY, trwa
    // w pliku, i dlatego jako jedyna z czterech MA sie tu zgadzac z nasza.
    assert_eq!(
        read_pragmas(&bare)?.journal_mode,
        "wal",
        "a bare connection to the same file reports journal_mode = {:?}. This one SHOULD match \
         ours: the journal mode belongs to the DATABASE and survives in the file, which is \
         exactly why it is the one pragma nobody forgets and the other three are the ones \
         everybody does",
        read_pragmas(&bare)?.journal_mode
    );

    // (a) trzy pragmy polaczenia, kazda przestawiona na wartosc, ktorej nie akceptujemy.
    bare.pragma_update(None, "busy_timeout", 0)?;
    bare.pragma_update(None, "foreign_keys", 0)?;
    bare.pragma_update(None, "synchronous", 2)?;
    let wrong = read_pragmas(&bare)?;
    assert_eq!(
        wrong.busy_timeout, 0,
        "read_pragmas() reports busy_timeout = {} on a connection where it was just set to 0. \
         It is not reading the connection it was handed, so every busy_timeout assertion in \
         this file is a statement about a remembered value, not about a connection",
        wrong.busy_timeout
    );
    assert_eq!(
        wrong.foreign_keys, 0,
        "read_pragmas() reports foreign_keys = {} on a connection where it was just set to 0 \
         -- same problem, same consequence",
        wrong.foreign_keys
    );
    assert_eq!(
        wrong.synchronous, 2,
        "read_pragmas() reports synchronous = {} on a connection where it was just set to 2",
        wrong.synchronous
    );

    // (b) i dopiero TERAZ nasz helper, na tym samym polaczeniu, startujac od zlych wartosci.
    apply_pragmas(&bare)?;
    let helped = read_pragmas(&bare)?;
    assert_eq!(
        helped.busy_timeout, BUSY_TIMEOUT,
        "apply_pragmas() left busy_timeout = {} on a connection where it had been zeroed. This \
         is the only assertion in this file that tells 'we set it' apart from 'the stack set it \
         for us' -- rusqlite's own default is exactly {BUSY_TIMEOUT}",
        helped.busy_timeout
    );
    assert_eq!(
        helped.foreign_keys, 1,
        "apply_pragmas() left foreign_keys = {} on a connection where it had been switched off. \
         The bundled SQLite defaults this ON, so this step is the only thing standing between \
         a helper that sets it and a helper that does nothing at all",
        helped.foreign_keys
    );
    assert_eq!(
        helped.synchronous, SYNCHRONOUS_NORMAL,
        "apply_pragmas() left synchronous = {} instead of {SYNCHRONOUS_NORMAL} (NORMAL) on a \
         connection where it had been set to FULL",
        helped.synchronous
    );

    store.close().await?;
    Ok(())
}
