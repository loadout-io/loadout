//! Z-6: wyjście z aplikacji zostawia indeks domknięty, a nie dziennik wielkości biegu.
//!
//! Zmierzone 2026-09-02 na żywej bibliotece: `loadout.db-wal` przeżywał zamknięcie okna
//! z dziesiątkami megabajtów. To nie jest kwestia miejsca na dysku — dziennik, który nie został
//! domknięty, jest tą częścią indeksu, której następne uruchomienie musi się dopiero domyślić.
//!
//! **Słaba wersja tego kryterium zamyka magazyn bez ani jednego żywego czytelnika.** Przechodzi
//! zawsze i na każdym kodzie: `SQLite` sam domyka i kasuje dziennik, kiedy ginie OSTATNIE
//! połączenie do bazy. Aplikacja tak nie wygląda ani przez chwilę — okno trzyma czytelników przez
//! całe swoje życie ([`loadout_lib::store::Store::reader`]) — więc test bez czytelnika mierzy
//! układ, którego u człowieka nie ma. Dlatego czytelnik jest tu otwarty PRZED zamknięciem
//! i trzymany aż po pomiar.
//!
//! # Ten moduł nie zna ANI JEDNEGO symbolu, który powstał razem z poprawką
//!
//! I to jest jego druga własność, równie wiążąca co pierwsza (AGENTS.md §2a punkt 4). Cel `it`
//! kompiluje się w całości albo wcale, więc jeden `use` czegoś nowego w którymkolwiek module
//! zabiera możliwość URUCHOMIENIA tego pliku na starym drzewie — a test, który się nie
//! skompilował, niczego nie uruchomił i niczego nie dowiódł. Wszystko, co wymaga nowego API,
//! mieszka obok, w `the_way_out_of_the_app_closes_the_index`.

use std::fs;
use std::path::Path;

use loadout_lib::store::{NewEvent, NewRun, Store};

/// Bieg, do którego należą wszystkie zdarzenia.
const RUN_ID: &str = "01996500-0000-7000-8000-00000000000a";

/// Ile wierszy ma jeden wsad [T7 §5.3].
const BATCH: usize = 100;

/// Ile wsadów ma „długi bieg". Razem z [`LINE_BYTES`] daje ~10 MB zapisu, czyli tyle, ile
/// w bibliotece uzbiera jeden długi bieg — i ponad dwukrotność progu, po którym `SQLite` sam
/// sięga po dziennik.
const BATCHES: usize = 20;

/// Ile bajtów ma treść jednej linii transkryptu.
const LINE_BYTES: usize = 5 * 1024;

/// Ile wolno zostać w dzienniku po zamknięciu. Liczba jest wprost z kryterium audytu z 2026-09-02
/// („`loadout.db-wal` po zamknięciu poniżej 1 MB"), nie z pomiaru maszyny.
pub(crate) const WAL_CEILING_BYTES: u64 = 1024 * 1024;

/// Ile wolno mieć dziennikowi przy OTWARTEJ aplikacji, kiedy nikt już nie blokuje domknięcia.
///
/// Dwa mebibajty ponad `store::JOURNAL_SIZE_LIMIT_BYTES`, i ta nadwyżka nie jest zapasem na
/// wszelki wypadek: sufit z pragmy przycina plik PO domknięciu, więc jedna transakcja zawsze może
/// usiąść ponad nim, zanim następne domknięcie ją zabierze. Nasza ma ~500 KB
/// ([`BATCH`] × [`LINE_BYTES`]).
///
/// Liczba stoi tutaj, a nie jako `use` stałej produkcyjnej, z powodu opisanego w nagłówku modułu.
/// Że obie się zgadzają, pilnuje `the_way_out_of_the_app_closes_the_index` — tam wolno znać nowe
/// symbole, więc tam mieszka asercja, która nie pozwoli im się rozjechać.
pub(crate) const JOURNAL_CEILING_WHILE_OPEN_BYTES: u64 = 6 * 1024 * 1024;

/// Ile wsadów idzie do bazy przy czytelniku, który trzyma swoją migawkę.
///
/// Dwadzieścia megabajtów, czyli grubo ponad każdy sufit w tym pliku: dopóki czytelnik trzyma
/// migawkę, `SQLite` nie ma prawa przewinąć dziennika na początek i plik rośnie liniowo.
/// Dokładnie tak biblioteka doszła do 42 MB.
const BATCHES_UNDER_A_HELD_SNAPSHOT: usize = 40;

/// Bieg w kształcie, w jakim wchodzi do bazy.
fn a_run() -> NewRun {
    NewRun {
        id: RUN_ID.to_owned(),
        workflow_id: "ship-a-feature".to_owned(),
        workflow_snapshot: r#"{"nodes":[],"edges":[]}"#.to_owned(),
        title: "Fix the CSV parser".to_owned(),
        status: "running".to_owned(),
        concurrency: 3,
        created_at: 1_755_300_000_000,
        started_at: Some(1_755_300_001_000),
        ended_at: None,
        boot_id: None,
        error: None,
    }
}

/// Sto linii transkryptu, każda tej samej, realistycznej długości.
fn a_batch(round: usize) -> Vec<NewEvent> {
    (0..BATCH)
        .map(|line| NewEvent {
            run_id: RUN_ID.to_owned(),
            step_id: None,
            ts: 1_755_300_002_000,
            kind: "assistant".to_owned(),
            level: "detail".to_owned(),
            body: Some(format!("{round}:{line} {}", "x".repeat(LINE_BYTES))),
        })
        .collect()
}

/// Zapisuje ~10 MB transkryptu tą samą drogą, którą pisze bieg.
///
/// `pub(crate)`, bo sądzi tę samą bazę także moduł obok — a dwie kopie tej fikstury znaczyłyby
/// dwa różne „10 MB" i dwa różne wyniki przy tej samej implementacji.
pub(crate) async fn a_long_run_worth_of_lines(store: &Store) -> anyhow::Result<()> {
    let writer = store.writer();
    writer.insert_run(a_run()).await?;
    for round in 0..BATCHES {
        writer.append_events(a_batch(round)).await?;
    }
    Ok(())
}

/// Ile bajtów ma dziennik obok tej bazy.
pub(crate) fn journal_beside(db: &Path) -> anyhow::Result<u64> {
    Ok(fs::metadata(db.with_extension("db-wal"))?.len())
}

#[tokio::test]
async fn closing_the_store_truncates_the_wal_it_leaves_behind() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("loadout.db");
    let store = Store::open(&db)?;

    a_long_run_worth_of_lines(&store).await?;

    // TU PADA STARA IMPLEMENTACJA. Czytelnik żyje przez całe zamknięcie — dokładnie tak, jak
    // żyje okno — więc `SQLite` nie ma prawa skasować dziennika przy zamknięciu pisarza, a bez
    // jawnego domknięcia plik zostaje przy swoim znaku wysokiej wody.
    let reader = store.reader()?;
    store.close().await?;

    let wal = journal_beside(&db)?;
    assert!(
        wal <= WAL_CEILING_BYTES,
        "after the store was closed its journal still holds {wal} bytes, over the {WAL_CEILING_BYTES} \
         the library is allowed to leave behind. Closing the last writer is not enough while \
         anything else still has the file open — and the window holds readers for its whole life, \
         so this is the shape every real quit has"
    );

    drop(reader);
    Ok(())
}

#[tokio::test]
async fn the_journal_stops_growing_while_the_app_is_open() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("loadout.db");
    let store = Store::open(&db)?;
    let writer = store.writer();
    writer.insert_run(a_run()).await?;

    /* CZYTELNIK Z ZATRZYMANĄ MIGAWKĄ, i to jest cały układ tego kryterium. Domknięcie dziennika
     * potrafi przepisać strony do bazy, ale nie ma prawa przewinąć pliku na początek, dopóki
     * ktokolwiek czyta z wcześniejszej migawki — więc dziennik rośnie liniowo z zapisem. Okno
     * Loadouta czyta bazę bez przerwy, więc to nie jest układ sztuczny: dokładnie tędy biblioteka
     * doszła do 42 MB przy bazie, w której nic tyle nie ważyło (2026-09-02).
     *
     * `BEGIN` samo w sobie niczego nie blokuje — jest odroczone. Migawkę bierze dopiero pierwsze
     * zapytanie w tej transakcji, i dlatego stoi tu zaraz po nim. */
    let reader = store.reader()?;
    reader.execute_batch("BEGIN")?;
    let _snapshot: i64 = reader.query_row("SELECT count(*) FROM events", [], |row| row.get(0))?;

    for round in 0..BATCHES_UNDER_A_HELD_SNAPSHOT {
        writer.append_events(a_batch(round)).await?;
    }
    let while_held = journal_beside(&db)?;
    assert!(
        while_held > JOURNAL_CEILING_WHILE_OPEN_BYTES,
        "the journal only reached {while_held} bytes while a reader held its snapshot, so this \
         test never built the situation it is about and the assertion below would pass on any \
         code at all"
    );

    // Czytelnik oddaje migawkę. Od tej chwili domknięcia znowu przechodzą do końca — i to jest
    // ten moment, w którym sufit z pragmy ma zabrać plik z powrotem w dół.
    reader.execute_batch("COMMIT")?;

    // TU PADA STARA IMPLEMENTACJA. Bez `journal_size_limit` domknięcie przewija dziennik do
    // ponownego użycia i zostawia plik przy znaku wysokiej wody — na zawsze. Aplikacja stoi
    // otwarta przez cały ten czas i nikt jej nie zamyka, więc nie ratuje tu nic z drogi wyjścia.
    for round in 0..BATCHES {
        writer.append_events(a_batch(round)).await?;
    }
    let after = journal_beside(&db)?;
    assert!(
        after <= JOURNAL_CEILING_WHILE_OPEN_BYTES,
        "with the app still open and nothing holding the index back any more, the journal stands \
         at {after} bytes — it grew to {while_held} and never came down. It is capped at \
         {JOURNAL_CEILING_WHILE_OPEN_BYTES} here. A journal that only ever grows is how a library \
         whose whole database is 72 MB ends up carrying a 42 MB journal beside it"
    );

    drop(reader);
    store.close().await?;
    Ok(())
}
