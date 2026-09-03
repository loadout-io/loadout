//! Z-6: droga, którą naprawdę idzie ⌘Q, dochodzi do magazynu.
//!
//! Osobny moduł od `quitting_leaves_the_index_small`, i podział przebiega dokładnie po tym, czego
//! na starym drzewie jeszcze nie ma. Tam nie wolno wymienić ani jednego nowego symbolu, bo tamten
//! plik ma się URUCHOMIĆ przed poprawką i paść na asercji (AGENTS.md §2a punkt 4); tutaj wolno
//! wszystko, bo ten plik dowodzi rzeczy, której przed poprawką nie ma jak nazwać.
//!
//! Pierwszy test jest drugą połową kryterium (niezmiennik 29): tamten dowodzi, że mechanizm
//! istnieje, ten — że droga wyjścia z aplikacji do niego dochodzi. Do 2026-09 nie dochodziła:
//! powłoka okna kończyła biegi i rozmowy, a o indeksie nie mówiła ani słowa.

use std::sync::Arc;

use loadout_lib::commands::Drivers;
use loadout_lib::engine::drivers::AgentDriver;
use loadout_lib::engine::drivers::absent::Absent;
use loadout_lib::ipc::AppState;
use loadout_lib::store::{JOURNAL_SIZE_LIMIT_BYTES, Store, read_pragmas};

use super::quitting_leaves_the_index_small::{
    WAL_CEILING_BYTES, a_long_run_worth_of_lines, journal_beside,
};

/// Fabryka sterowników dla drogi, która żadnego agenta nie startuje.
fn no_agents_needed() -> Drivers {
    let absent: Arc<dyn AgentDriver> = Arc::new(Absent::new("nobody", "Z-6"));
    Arc::new(move |_vendor| Arc::clone(&absent))
}

#[tokio::test]
async fn the_way_out_of_the_app_closes_the_index_too() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let db = home.path().join("loadout.db");

    let store = Store::open(&db)?;
    a_long_run_worth_of_lines(&store).await?;
    // Czytelnik żyje przez całe wyjście — okno trzyma swoje przez całe swoje życie.
    let reader = store.reader()?;

    // PRODUKCYJNE DRZWI, nie samo `Store::close`: to jest jedyna droga, którą woła zarówno
    // `CloseRequested`, jak i `ExitRequested` (`lib.rs`), więc tylko tędy da się zapytać, czy
    // wyjście z aplikacji naprawdę domyka indeks.
    let state = AppState::new(
        home.path().to_path_buf(),
        project.path().to_path_buf(),
        store,
        no_agents_needed(),
    );
    state.close_everything_down().await;

    let wal = journal_beside(&db)?;
    assert!(
        wal <= WAL_CEILING_BYTES,
        "the way out of the app left {wal} bytes of journal behind, over the \
         {WAL_CEILING_BYTES} it is allowed to. Stopping the runs and the lead agents is only \
         part of leaving: the index is written to the very last moment, so whoever closes the \
         one has to close the other"
    );

    // Drugie wyjście tą samą drogą jest ciszą, nie zawisem: czerwony guzik i ⌘Q potrafią przyjść
    // jedno po drugim, a okno, które nie umie się zamknąć, jest gorsze niż dziennik na dysku.
    state.close_everything_down().await;

    drop(reader);
    Ok(())
}

#[tokio::test]
async fn every_connection_carries_the_ceiling_the_journal_grows_to() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let store = Store::open(&dir.path().join("loadout.db"))?;

    let writing = store.writer_pragmas().await?;
    assert_eq!(
        writing.journal_size_limit, JOURNAL_SIZE_LIMIT_BYTES,
        "the writing connection carries a journal limit of {} instead of \
         {JOURNAL_SIZE_LIMIT_BYTES}. Without it the journal never shrinks: closing it down copies \
         its pages into the database and rewinds it for reuse, but the file itself stays at its \
         high-water mark for as long as the library lives",
        writing.journal_size_limit
    );

    // I na czytelniku, bo to jest własność POŁĄCZENIA, nie bazy — dokładnie tak, jak busy_timeout
    // i klucze obce. Połączenie, które ją pominie, przycina dziennik po sobie do niczego.
    let reading = read_pragmas(&store.reader()?)?;
    assert_eq!(
        reading.journal_size_limit, JOURNAL_SIZE_LIMIT_BYTES,
        "the read-only connection carries a journal limit of {} instead of \
         {JOURNAL_SIZE_LIMIT_BYTES}",
        reading.journal_size_limit
    );

    /* SPINKA MIĘDZY DWOMA MODUŁAMI. `the_journal_stops_growing_while_the_app_is_open` mierzy
     * wzrost dziennika wobec liczby wpisanej u siebie, bo tamten plik nie ma prawa znać tej
     * stałej. Bez tej asercji obie liczby mogłyby się rozjechać po cichu i tamto kryterium
     * mierzyłoby sufit, którego w produkcie nie ma (2026-09). */
    let ceiling = u64::try_from(JOURNAL_SIZE_LIMIT_BYTES)?;
    assert!(
        ceiling < super::quitting_leaves_the_index_small::JOURNAL_CEILING_WHILE_OPEN_BYTES,
        "the journal limit this app sets ({ceiling}) is not below the ceiling the growth check \
         measures against. Those two numbers have to move together: one transaction can always \
         sit above the limit before the next close-down takes it away, so the check leaves room \
         for exactly that and no more"
    );

    store.close().await?;
    Ok(())
}
