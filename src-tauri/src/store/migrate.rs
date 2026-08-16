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
//! Brak tabeli wersji jest decyzją, nie zaniedbaniem [00-SYNTHESIS §3, `SQLite`]: numer wersji
//! jest drugim źródłem prawdy o schemacie obok samego schematu, a przy dwóch źródłach zawsze
//! czytasz to nieaktualne. Jedyne pytanie, na które ta funkcja odpowiada, brzmi „czy w bazie
//! stoi to, co ma stać", i odpowiada na nie, wykonując zdania, które same sprawdzają, czy mają
//! co robić.
//!
//! # Stan tego pliku: SZKIELET (2026-08-16)
//!
//! Pętla jest już ta docelowa, ale [`schema::STATEMENTS`] jest pusta, więc wykonuje się
//! zero zdań i **żadna tabela nie powstaje**. To jest świadomie zła wartość: AC-1 pada wtedy
//! na kontroli przeciw pustej asercji („zrzut `sqlite_master` po migracji jest pusty"), a nie
//! na braku celu testowego, i tak ma być w tej fazie (`AGENTS.md` §2a p. 5).

use rusqlite::Connection;

use super::Result;
use super::schema;

/// Doprowadza schemat na `conn` do stanu z [`schema::STATEMENTS`].
///
/// Wołane przy **każdym** otwarciu bazy, jako ostatni krok kolejności open → pragmy →
/// `busy_timeout` → `migrate()` [00-SYNTHESIS §3]. Migracja puszczona przed pragmami widzi
/// wyłączone klucze obce, czyli inny świat niż ten, w którym potem biegnie aplikacja.
pub fn migrate(conn: &Connection) -> Result<()> {
    for statement in schema::STATEMENTS {
        // `execute_batch`, nie `execute`: jedno zdanie schematu bywa `CREATE TABLE` razem
        // z indeksami i wyzwalaczami, które do niej należą, a trzymanie ich obok siebie jest
        // jedynym sposobem, żeby nie dało się dodać tabeli i zapomnieć jej wyzwalacza.
        conn.execute_batch(statement)?;
    }
    Ok(())
}
