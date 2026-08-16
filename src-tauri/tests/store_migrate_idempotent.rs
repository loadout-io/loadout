//! AC-1 dla T-06: `migrate()` wywołane dwa razy na tym samym połączeniu nie zmienia niczego.
//!
//! Migracje są addytywne i idempotentne (niezmiennik 25). Cicho łamie się to tak: ktoś dopisuje
//! gołe `ALTER TABLE steps ADD COLUMN`, bo „przecież kolumny jeszcze nie ma". Drugi start
//! aplikacji rzuca `duplicate column name`, a użytkownik widzi „nie udało się otworzyć
//! projektu" — czyli awaria pokazuje się u kogoś, kto otworzył ten sam projekt drugi raz,
//! i nigdy u autora zmiany.
//!
//! **Słaba wersja tego kryterium to `assert!(migrate(&c).is_ok()); assert!(migrate(&c).is_ok())`.**
//! Przechodzi, dopóki migracja składa się z samych `CREATE TABLE IF NOT EXISTS` — i przestaje
//! dokładnie w dniu, w którym przestaje, bo `Ok` z pierwszego przebiegu nie mówi **nic**
//! o drugim. Rozróżniają je dwie rzeczy: porównanie **pełnego zrzutu `sqlite_master`** (łapie
//! zmieniony wyzwalacz, zgubiony indeks i przepisaną tabelę — bo porównujemy także treść `sql`
//! każdego obiektu) oraz przeżycie wierszy wstawionych **pomiędzy** wywołaniami (łapie migrację,
//! która przy okazji „naprawia" dane).
//!
//! Kontrola przeciw pustej asercji jest tu nośna i stoi na początku: dwa **puste** zrzuty też
//! są równe. Bez sprawdzenia, że po pierwszej migracji w bazie cokolwiek stoi, to kryterium
//! przechodziłoby na migracji, która nie robi nic.

use rusqlite::types::Value;
use rusqlite::{Connection, params};

use loadout_lib::store::migrate;

/// Pięć tabel z [T7 §5.4]. Lista jest tu wpisana ręcznie **z rozmysłem**: jest modelem
/// referencyjnym schematu, a nie jego odczytem. Odczyt z implementacji zgodziłby się sam ze sobą.
const TABLES: [&str; 5] = ["runs", "steps", "events", "artifacts", "memory"];

/// Bieg wstawiony **pomiędzy** dwoma wywołaniami migracji.
const RUN_ID: &str = "01996500-0000-7000-8000-000000000001";

/// Zrzut schematu: rodzaj obiektu, nazwa, tabela i **pełna** treść `sql`.
///
/// `sql` jest w porównaniu, a nie pominięte, bo bez niego wyzwalacz podmieniony na inny
/// o tej samej nazwie przechodzi bez śladu — a to jest dokładnie ta zmiana, po której
/// „append-only" przestaje być prawdą.
fn schema_dump(conn: &Connection) -> anyhow::Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY type, name")?;
    let rows = stmt.query_map([], |row| {
        Ok(format!(
            "{}|{}|{}|{}",
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?.unwrap_or_default(),
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Wszystkie wiersze tabeli, wszystkie kolumny, w postaci porównywalnej tekstowo.
///
/// `SELECT *`, nie lista kolumn wpisana w test: kolumna dodana jutro wchodzi do porównania
/// sama i nie da się jej po cichu wyjąć spod tego kryterium.
fn rows_of(conn: &Connection, table: &str) -> anyhow::Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("SELECT * FROM {table} ORDER BY 1"))?;
    let width = stmt.column_count();
    let rows = stmt.query_map([], move |row| {
        let mut cells = Vec::with_capacity(width);
        for index in 0..width {
            cells.push(format!("{:?}", row.get::<_, Value>(index)?));
        }
        Ok(cells.join(" | "))
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[test]
fn migrating_twice_leaves_the_schema_and_the_rows_exactly_as_they_were() -> anyhow::Result<()> {
    let conn = Connection::open_in_memory()?;

    migrate(&conn)?;
    let before = schema_dump(&conn)?;

    // ── Kontrola przeciw pustej asercji ────────────────────────────────────────────────────
    // Dwa puste zrzuty są równe, więc bez tego bloku całe kryterium przechodzi na migracji,
    // która nie tworzy niczego.
    assert!(
        !before.is_empty(),
        "migrate() left sqlite_master empty, so the comparison below would be two empty dumps \
         agreeing with each other. Everything this criterion is about — tables, indexes, \
         triggers — has to exist first"
    );
    for table in TABLES {
        let wanted = format!("table|{table}|");
        assert!(
            before.iter().any(|object| object.starts_with(&wanted)),
            "the schema has no {table} table. The five tables of T7 §5.4 are the whole index; \
             a missing one is a column of the run that cannot be rebuilt after loadout.db is \
             deleted. sqlite_master holds: {before:?}"
        );
    }
    assert!(
        before.iter().any(|object| object.starts_with("trigger|")),
        "the schema carries no trigger at all, so append-only is enforced nowhere and this \
         comparison has no trigger text to notice a change in. sqlite_master holds: {before:?}"
    );
    assert!(
        before.iter().any(|object| object.starts_with("index|")),
        "the schema carries no index at all, so a lost index cannot show up in this comparison. \
         sqlite_master holds: {before:?}"
    );

    // ── Wiersze wstawione POMIĘDZY wywołaniami ─────────────────────────────────────────────
    // To one łapią migrację, która przy okazji „naprawia" dane: schemat może być identyczny,
    // a wiersz przepisany.
    conn.execute(
        "INSERT INTO runs (id, workflow_id, workflow_snapshot, title, status, concurrency, \
         created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            RUN_ID,
            "ship-a-feature",
            r#"{"nodes":[],"edges":[]}"#,
            "Fix the CSV parser",
            "succeeded",
            3_i64,
            1_755_300_000_000_i64
        ],
    )?;
    conn.execute(
        "INSERT INTO events (run_id, ts, kind, level, body) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            RUN_ID,
            1_755_300_001_000_i64,
            "assistant",
            "headline",
            "Read six files"
        ],
    )?;

    let runs_before = rows_of(&conn, "runs")?;
    let events_before = rows_of(&conn, "events")?;
    assert_eq!(
        (runs_before.len(), events_before.len()),
        (1, 1),
        "the two rows this criterion is about did not land, so their survival proves nothing"
    );

    // ── Drugi przebieg ─────────────────────────────────────────────────────────────────────
    let second = migrate(&conn);
    assert!(
        second.is_ok(),
        "the second call to migrate() came back with an error. This is the shape of the failure \
         a user meets on the SECOND launch of the app and never on the first: {second:?}"
    );

    assert_eq!(
        schema_dump(&conn)?,
        before,
        "migrate() changed the schema on its second run. Compared here is the full text of every \
         object in sqlite_master, so this catches a rewritten trigger and a dropped index just as \
         well as a rewritten table — and each of those is a silent change of what append-only and \
         'delete loadout.db safely' mean"
    );
    assert_eq!(
        rows_of(&conn, "runs")?,
        runs_before,
        "the run inserted between the two calls did not come out the other side unchanged. \
         A migration that touches rows is not additive, and the row it touches is a row nobody \
         is watching"
    );
    assert_eq!(
        rows_of(&conn, "events")?,
        events_before,
        "the event inserted between the two calls did not come out the other side unchanged. \
         events is the transcript: a migration allowed to rewrite it can rewrite history, which \
         is the one thing the triggers on this table exist to prevent"
    );

    Ok(())
}
