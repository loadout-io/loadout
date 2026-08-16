//! Schemat: pięć tabel, `STRICT`, i odmowy, które `SQLite` wypowiada **sam**.
//!
//! Ten plik jest jednym z trzech, którym `checks/quick-boundary.sh` pozwala nieść DDL —
//! obok `store::migrate` i `store::writer`. Nazwa nie jest dowolna: sprawdzenie zna ją
//! z nazwy, a `CREATE TABLE` w czwartym pliku przewraca bramkę i ma rację.
//!
//! Co tu **musi** stanąć i dlaczego każdy z tych punktów jest osobnym kryterium:
//!
//! - **Pięć tabel z [T7 §5.4]** — `runs`, `steps`, `events`, `artifacts`, `memory` — wszystkie
//!   `STRICT`. Bez `STRICT` `SQLite` przyjmie `attempt = 'dwa'` i policzy to jako tekst, a wtedy
//!   pierwszy odczyt dostaje wartość, której nie umie dodać (AC-7 b).
//! - **`CHECK` na `steps.status`** z siedmioma stanami z `docs/ARCHITECTURE.md` §5 i **`CHECK`
//!   na `events.level`** z trzema poziomami. Odmowa napisana w naszej funkcji `insert_step`
//!   przechodzi na schemacie bez `CHECK` — a wtedy pierwszy zapis spoza naszego API (migracja,
//!   odbudowa z AC-4, skrypt) wpuszcza stan, którego UI nie umie narysować (AC-7 a).
//! - **`REFERENCES … ON DELETE CASCADE`** na `steps.run_id`, `events.run_id`, `events.step_id`
//!   i `artifacts.*` — kasowanie biegu ma zabrać jego kroki i zdarzenia (AC-7 c). Działa
//!   wyłącznie przy `foreign_keys` ON, czyli razem z [`super::apply_pragmas`].
//! - **`UNIQUE (run_id, node_key)`** na `steps` (AC-7 d).
//! - **Wyzwalacze reject-update i reject-delete na `events`**, każdy z `RAISE(ABORT, …)`
//!   niosącym [`APPEND_ONLY_REFUSAL`]. To one, a nie Rust, czynią „append-only" prawdą dla
//!   połączenia, które omija nasze API: migracji, skryptu naprawczego, przyszłego daemona,
//!   `sqlite3` z terminala (AC-2). Wzorzec przychodzi z poprzedniego prototypu,
//!   `the earlier prototype's store/src/schema.rs:163-190` — trzy linie na wyzwalacz.
//! - **Indeksy z [T7 §5.4]**, w tym **częściowy** `WHERE level = 'headline'`: szyna czyta
//!   wyłącznie nagłówki, więc indeks ma zawierać wyłącznie nagłówki.
//!
//! Czego tu **nie** ma i nie będzie: tabeli wersji schematu i drugiej wersji schematu. Jedna
//! wersja, aż zajdzie potrzeba drugiej (`AGENTS.md`, tabela zakazów). Migracja jest addytywna
//! i idempotentna (niezmiennik 25), więc wersji nie ma po czym numerować.
//!
//! # Stan tego pliku: SZKIELET (2026-08-16)
//!
//! [`STATEMENTS`] jest **pusta**. To jest świadomie zła wartość, nie niedopatrzenie: cała
//! treść AC-1, AC-2 i AC-7 mieszka w tych zdaniach SQL, więc wpisanie ich tutaj byłoby
//! implementacją, a nie szkieletem. Pusta lista sprawia, że `store::migrate` wykonuje zero
//! zdań i **żadna** tabela nie powstaje — a wtedy każde z tych trzech kryteriów pada na braku
//! zachowania, nie na braku celu testowego.

/// Tekst, którym wyzwalacze `events` odmawiają.
///
/// Stoi w stałej, a nie wklejony dwa razy w SQL, bo pojawia się w komunikacie błędu, który
/// czyta człowiek — i dlatego, że test AC-2 sprawdza, czy odmowa przyszła **z wyzwalacza**,
/// a nie z tabeli otwartej tylko do odczytu. Te dwa błędy wyglądają identycznie, jeśli patrzeć
/// wyłącznie na to, że `UPDATE` się nie udał.
pub const APPEND_ONLY_REFUSAL: &str = "events is append-only";

/// Zdania SQL schematu, w kolejności wykonania.
///
/// Każde musi być **idempotentne** (`IF NOT EXISTS`), bo `store::migrate` biegnie przy każdym
/// otwarciu bazy i wolno mu wykonać się dwa razy pod rząd bez żadnej różnicy (niezmiennik 25,
/// AC-1). Kolumnę dokłada się przez `add_column_if_missing`, nigdy gołym `ALTER TABLE … ADD
/// COLUMN`: drugi start aplikacji rzuca wtedy `duplicate column name`, a użytkownik widzi
/// „nie udało się otworzyć projektu".
pub const STATEMENTS: &[&str] = &[
    // SZKIELET (2026-08-16): pięć tabel, cztery indeksy i dwa wyzwalacze są całą treścią AC-1,
    // AC-2 i AC-7. Lista zostaje pusta do fazy implementacji.
];
