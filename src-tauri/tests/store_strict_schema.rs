//! AC-7 dla T-06: schemat odmawia sam, bez pomocy Rusta.
//!
//! **Słaba wersja tego kryterium to sprawdzenie, że nasza funkcja `insert_step` odrzuca zły
//! status.** Przechodzi na schemacie bez ani jednego `CHECK` i bez `STRICT` — a wtedy pierwszy
//! zapis spoza naszego API (migracja, odbudowa z AC-4, skrypt naprawczy) wpuszcza stan, którego
//! UI nie umie narysować, i widok pokazuje pustkę. Rozróżnia je to, że **wszystkie cztery próby
//! idą surowym SQL-em na surowym połączeniu**, więc czerwień może pochodzić wyłącznie od
//! schematu.
//!
//! Druga rzecz, bez której to kryterium nic nie mierzy: `Err` samo w sobie nie znaczy „schemat
//! odmówił". Na bazie **bez tabeli `steps`** wszystkie cztery próby też są `Err`. Dlatego każda
//! odmowa ma tu obok siebie **przyjęcie**: siedem legalnych stanów wchodzi, ósmy nie; ten sam
//! `node_key` pod innym biegiem wchodzi, pod tym samym nie. Kryterium mówi więc, co schemat
//! **przyjmuje**, a nie tylko, że coś odrzuca.
//!
//! Siedem nazw stanu jest tu wpisanych ręcznie **z rozmysłem**: to jest model referencyjny
//! z `docs/ARCHITECTURE.md` §5, a nie odczyt z implementacji. Ta sama siódemka stoi w enumie
//! `engine::step::StepState` (T-02 AC-7) — rozjazd między nim a `CHECK`iem w tej kolumnie
//! skończyłby się wierszem, którego `SQLite` nie przyjmie, w trakcie biegu.

use rusqlite::{Connection, params};

use loadout_lib::store::{apply_pragmas, migrate};

/// Pięć tabel z [T7 §5.4].
const TABLES: [&str; 5] = ["runs", "steps", "events", "artifacts", "memory"];

/// Siedem stanów kroku z `docs/ARCHITECTURE.md` §5. `paused` jest stanem **biegu**, nigdy kroku.
const STATUSES: [&str; 7] = [
    "pending",
    "ready",
    "running",
    "succeeded",
    "failed",
    "cancelled",
    "skipped",
];

/// Ósmy stan, którego nie ma. Brzmi rozsądnie i dlatego jest dobrym testem.
const NOT_A_STATUS: &str = "finished";

/// Bieg, który skasujemy.
const RUN_ID: &str = "01996500-0000-7000-8000-000000000001";

/// Bieg, który ma to przeżyć — bez niego kaskada jest nieodróżnialna od wyczyszczenia tabeli.
const OTHER_RUN_ID: &str = "01996500-0000-7000-8000-000000000002";

/// Treść odmowy, albo pusty napis, kiedy operacja się **udała**.
fn refusal_text(outcome: &rusqlite::Result<usize>) -> String {
    outcome
        .as_ref()
        .err()
        .map(ToString::to_string)
        .unwrap_or_default()
}

/// Wstawia bieg surowym SQL-em.
fn insert_run(conn: &Connection, id: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO runs (id, workflow_id, workflow_snapshot, title, status, concurrency, \
         created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            "ship-a-feature",
            r#"{"nodes":[],"edges":[]}"#,
            "Fix the CSV parser",
            "running",
            3_i64,
            1_755_300_000_000_i64
        ],
    )
}

/// Wstawia krok surowym SQL-em. Wszystko oprócz `status` i `node_key` jest tu stałe, żeby
/// jedyną zmienną w każdej próbie było to, o co ta próba pyta.
fn insert_step(
    conn: &Connection,
    id: &str,
    run_id: &str,
    node_key: &str,
    status: &str,
) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO steps (id, run_id, node_key, name, agent, depends_on, status, attempt) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            run_id,
            node_key,
            "Read the parser",
            "claude",
            "[]",
            status,
            0_i64
        ],
    )
}

/// Wstawia zdarzenie surowym SQL-em.
fn insert_event(conn: &Connection, run_id: &str, step_id: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO events (run_id, step_id, ts, kind, level, body) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            run_id,
            step_id,
            1_755_300_002_000_i64,
            "assistant",
            "headline",
            "Read six files"
        ],
    )
}

/// Ile wierszy tabeli należy do tego biegu.
fn rows_for(conn: &Connection, table: &str, run_id: &str) -> anyhow::Result<i64> {
    Ok(conn.query_row(
        &format!("SELECT count(*) FROM {table} WHERE run_id = ?1"),
        [run_id],
        |row| row.get(0),
    )?)
}

#[test]
fn the_schema_refuses_on_its_own() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("loadout.db");

    // Połączenie otwarte BEZPOŚREDNIO tutaj. Wszystko niżej idzie po nim, surowym SQL-em, bez
    // ani jednej naszej funkcji zapisującej — więc każda odmowa pochodzi od schematu.
    let conn = Connection::open(&db)?;
    apply_pragmas(&conn)?;
    migrate(&conn)?;

    // ── Kontrole przeciw pustej asercji ────────────────────────────────────────────────────
    for table in TABLES {
        let present: i64 = conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get(0),
        )?;
        assert_eq!(
            present, 1,
            "there is no {table} table, and on a database without it EVERY insert below is an \
             error too. Then this criterion reads as green-adjacent while measuring nothing"
        );
    }

    // Warunek wstępny dla (c), nie teza tego pliku: kaskady działają wyłącznie przy
    // `foreign_keys` ON, i to AC-3 jest kryterium od tego, czy nasze połączenia je mają.
    // Stoi tutaj, żeby czerwień w (c) dało się odróżnić od czerwieni w AC-3.
    let foreign_keys: i64 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
    assert_eq!(
        foreign_keys, 1,
        "store::apply_pragmas left foreign keys off on this connection, so the cascade in (c) \
         below could not fire whatever the schema says. That is AC-3's criterion, not this one"
    );

    insert_run(&conn, RUN_ID)?;
    insert_run(&conn, OTHER_RUN_ID)?;

    // ── (a) CHECK na steps.status ──────────────────────────────────────────────────────────
    for (index, status) in STATUSES.into_iter().enumerate() {
        let accepted = insert_step(
            &conn,
            &format!("step-{index}"),
            RUN_ID,
            &format!("node-{index}"),
            status,
        );
        assert!(
            accepted.is_ok(),
            "the schema refused {status:?}, which is one of the seven states of \
             docs/ARCHITECTURE.md §5. A CHECK that is too narrow stops the run itself, and it \
             stops it in the middle: {:?}",
            refusal_text(&accepted)
        );
    }

    let bad_status = insert_step(&conn, "step-x", RUN_ID, "node-x", NOT_A_STATUS);
    let bad_status_said = refusal_text(&bad_status);
    assert!(
        bad_status.is_err(),
        "the schema accepted status {NOT_A_STATUS:?}. Seven states are drawable; anything else \
         reaches the rail as a step the UI has no cell for, and the view shows a blank"
    );
    assert!(
        bad_status_said.contains("CHECK"),
        "steps refused {NOT_A_STATUS:?}, but not with a CHECK constraint. Any other reason means \
         the allowed set is not written into the schema, and then the first write from outside \
         our API puts it there: {bad_status_said:?}"
    );

    // ── (b) STRICT ─────────────────────────────────────────────────────────────────────────
    // Bez STRICT SQLite przyjmie to i policzy jako tekst — a wtedy pierwszy odczyt dostaje
    // „try 'dwa' of 3" albo panikę przy dodawaniu.
    let wrong_type = conn.execute(
        "INSERT INTO steps (id, run_id, node_key, name, agent, depends_on, status, attempt) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            "step-y",
            RUN_ID,
            "node-y",
            "Read the parser",
            "claude",
            "[]",
            "running",
            "dwa"
        ],
    );
    let wrong_type_said = refusal_text(&wrong_type);
    assert!(
        wrong_type.is_err(),
        "steps.attempt took the text 'dwa'. That is what a table without STRICT does: it stores \
         it and calls it text, and the column stops being a number some time later, in code that \
         did nothing wrong"
    );
    assert!(
        wrong_type_said.contains("attempt"),
        "the refusal does not name steps.attempt, so it may not be about the column's type at \
         all: {wrong_type_said:?}"
    );

    // ── (d) UNIQUE (run_id, node_key) ──────────────────────────────────────────────────────
    let duplicate = insert_step(&conn, "step-z", RUN_ID, "node-0", "running");
    let duplicate_said = refusal_text(&duplicate);
    assert!(
        duplicate.is_err(),
        "the same node ran twice in one run and the schema took both. The stable node key is how \
         a rebuild from files finds the row it is supposed to update (AC-4); two rows and it \
         updates one of them"
    );
    assert!(
        duplicate_said.contains("UNIQUE"),
        "steps refused the second row, but not with a UNIQUE constraint: {duplicate_said:?}"
    );

    let same_key_other_run = insert_step(&conn, "step-w", OTHER_RUN_ID, "node-0", "running");
    assert!(
        same_key_other_run.is_ok(),
        "the schema refused node key 'node-0' under a DIFFERENT run. The constraint is on the \
         pair (run_id, node_key): on node_key alone, the second run of any workflow is \
         impossible: {:?}",
        refusal_text(&same_key_other_run)
    );

    // ── (c) ON DELETE CASCADE ──────────────────────────────────────────────────────────────
    insert_event(&conn, RUN_ID, "step-0")?;
    insert_event(&conn, OTHER_RUN_ID, "step-w")?;

    let planted_steps = rows_for(&conn, "steps", RUN_ID)?;
    let planted_events = rows_for(&conn, "events", RUN_ID)?;
    assert!(
        planted_steps > 0 && planted_events > 0,
        "the rows the cascade is supposed to take are not there ({planted_steps} steps, \
         {planted_events} events), so its firing proves nothing"
    );

    conn.execute("DELETE FROM runs WHERE id = ?1", params![RUN_ID])?;

    assert_eq!(
        (
            rows_for(&conn, "steps", RUN_ID)?,
            rows_for(&conn, "events", RUN_ID)?
        ),
        (0, 0),
        "deleting the run left its steps or its events behind. Rows pointing at a run that no \
         longer exists are the shape history takes when a user clears one item and the list \
         keeps showing pieces of it"
    );
    assert_eq!(
        (
            rows_for(&conn, "steps", OTHER_RUN_ID)?,
            rows_for(&conn, "events", OTHER_RUN_ID)?
        ),
        (1, 1),
        "the OTHER run lost rows too, so what happened was not a cascade but a sweep"
    );

    Ok(())
}
