//! Migracja: addytywna, idempotentna, bez frameworka i bez tabeli wersji (niezmiennik 25).
//!
//! Wolno ją wywołać dowolną liczbę razy na tym samym połączeniu i **nic** się po drugim razie
//! nie zmienia — ani schemat, ani wiersze, które ktoś w międzyczasie wstawił (AC-1). To nie
//! jest właściwość, która wychodzi sama: `assert!(migrate(&c).is_ok())` dwa razy pod rząd
//! przechodzi, dopóki migracja składa się z samych `CREATE TABLE IF NOT EXISTS`, i przestaje
//! w dniu, w którym ktoś dopisze gołe `ALTER TABLE steps ADD COLUMN` — bo `Ok` z pierwszego
//! przebiegu nie mówi nic o drugim. Dlatego kolumnę dokłada się przez `add_column_if_missing`
//! (sprawdź `PRAGMA table_info`, potem dodaj), a `DROP`, `ALTER … DROP COLUMN` i przepisywanie
//! wierszy są zakazane.
//!
//! Brak tabeli wersji jest decyzją, nie zaniedbaniem [FOUNDATIONS §3, `SQLite`]: numer wersji
//! jest drugim źródłem prawdy o schemacie obok samego schematu, a przy dwóch źródłach zawsze
//! czytasz to nieaktualne. Jedyne pytanie, na które ta funkcja odpowiada, brzmi „czy w bazie
//! stoi to, co ma stać", i odpowiada na nie, wykonując zdania, które same sprawdzają, czy mają
//! co robić.

use rusqlite::Connection;

use super::Result;
use super::schema;

/// Doprowadza schemat na `conn` do stanu z [`schema::STATEMENTS`].
///
/// Wołane przy **każdym** otwarciu bazy, jako ostatni krok kolejności open → pragmy →
/// `busy_timeout` → `migrate()` [FOUNDATIONS §3]. Migracja puszczona przed pragmami widzi
/// wyłączone klucze obce, czyli inny świat niż ten, w którym potem biegnie aplikacja.
/// Dokłada kolumnę, jeśli tabela jej jeszcze nie ma.
///
/// 2026-08-17 — NAPISANE, BO BYŁO OBIECANE I NIE ISTNIAŁO. Nagłówek tego pliku i `schema.rs`
/// odsyłały do `add_column_if_missing` jako do jedynej dozwolonej drogi dokładania kolumn,
/// a funkcji o tej nazwie nie było w repo. Pierwszy, kto potrzebowałby kolumny, napisałby
/// gołe `ALTER TABLE … ADD COLUMN` — czyli dokładnie to, przed czym te dwa komentarze
/// ostrzegają, bo drugi start aplikacji rzuca wtedy `duplicate column name`, a użytkownik
/// czyta „nie udało się otworzyć projektu".
///
/// `CREATE TABLE IF NOT EXISTS` w `schema.rs` załatwia wyłącznie bazy NOWE: tabela, która już
/// stoi, nie dostanie nowego pola stamtąd nigdy. Dlatego oba miejsca są potrzebne i oba
/// opisują tę samą kolumnę — to jedyny w tym pliku wyjątek od „jeden fakt, jedno miejsce",
/// i jest wymuszony przez `SQLite`, a nie wybrany.
fn add_column_if_missing(conn: &Connection, table: &str, column: &str, decl: &str) -> Result<()> {
    // `PRAGMA table_info` zamiast łapania błędu z `ALTER TABLE`: odpowiedź „ta kolumna już
    // jest" i odpowiedź „ta tabela jest zepsuta" wyglądają w błędzie tak samo, a różnią się
    // wszystkim. Pytamy więc wprost.
    let mut q = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut has = false;
    let mut rows = q.query([])?;
    while let Some(row) = rows.next()? {
        if row.get::<_, String>(1)? == column {
            has = true;
            break;
        }
    }
    drop(rows);
    drop(q);
    if !has {
        conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"))?;
    }
    Ok(())
}

pub fn migrate(conn: &Connection) -> Result<()> {
    for statement in schema::STATEMENTS {
        // `execute_batch`, nie `execute`: jedno zdanie schematu bywa `CREATE TABLE` razem
        // z indeksami i wyzwalaczami, które do niej należą, a trzymanie ich obok siebie jest
        // jedynym sposobem, żeby nie dało się dodać tabeli i zapomnieć jej wyzwalacza.
        conn.execute_batch(statement)?;
    }

    // Kolumny dokładane do tabel, które mogą już istnieć u kogoś na dysku. Lista rośnie
    // w dół i nigdy nie kurczy się w miejscu: usunięta pozycja to kolumna, której baza
    // sprzed tej wersji nigdy nie dostanie.
    add_column_if_missing(conn, "runs", "boot_id", "TEXT")?;

    Ok(())
}
