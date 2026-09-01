//! AC-2 dla T-06: wyzwalacze append-only łapią połączenie, które omija nasze API.
//!
//! „Append-only" egzekwowane w Ruście chroni przed niczym. Wystarczy jedno połączenie, które
//! nie przechodzi przez nasze funkcje — migracja, skrypt naprawczy, przyszły daemon, `sqlite3`
//! z terminala — i historia daje się przepisać, a **wszystkie nasze testy dalej są zielone**,
//! bo testowały nasze API. Wzorzec, który to naprawia, ma trzy linie na wyzwalacz i przychodzi
//! z poprzedniego prototypu (`the earlier prototype's store/src/schema.rs:163-190`) [FOUNDATIONS §3].
//!
//! **Słaba wersja tego kryterium to próba `UPDATE` przez nasze API, która dostaje odmowę
//! z Rusta.** Przechodzi na bazie bez ani jednego wyzwalacza. Rozróżnia je połączenie otwarte
//! **bezpośrednio tutaj**, z prawem zapisu, z pominięciem całego naszego Rusta.
//!
//! Drugi sposób, w jaki to kryterium mogłoby przejść, nic nie mierząc: baza otwarta **w całości**
//! tylko do odczytu przechodzi (a) i (b) nie mając ani jednego wyzwalacza. Dlatego jest punkt
//! (c) — kontrola dodatnia na **tym samym** połączeniu. `runs` i `steps` są z założenia
//! mutowalne, a `events` z założenia rośnie [T7 §5.1]; jeśli te trzy operacje też odmawiają,
//! to nie jest wyzwalacz, tylko brak prawa zapisu, i (a) z (b) nie znaczą nic.
//!
//! Punkt (e) domyka drogę, której (a) i (b) nie dotykają, a która kasuje transkrypt równie
//! skutecznie: **`DELETE FROM steps` przy żywym biegu**. `events.step_id` niesie
//! `ON DELETE CASCADE`, więc skasowanie kroku zabiera jego linie transkryptu, nie wymieniając
//! `events` ani razu — a to jest dokładnie ten ruch, który zrobi skrypt naprawczy („usuń
//! nieudany krok i ponów"). `events_reject_delete` ma to złapać, bo kaskada `ON DELETE` odpala
//! wyzwalacze `BEFORE DELETE` tabeli potomnej, a jego `WHEN` pyta wyłącznie o to, czy bieg
//! rodzic dalej stoi. Bez tej próby ta ścieżka jest przeczytana, nie sprawdzona (dopisane
//! 2026-08-16, druga opinia: luka w pokryciu, nie wada kodu).

use rusqlite::{Connection, params};

use loadout_lib::store::{NewEvent, NewRun, Store};

/// Bieg, do którego należą wszystkie zdarzenia w tym pliku.
const RUN_ID: &str = "01996500-0000-7000-8000-000000000001";

/// Krok, którym punkt (e) próbuje skasować transkrypt kaskadą.
const STEP_ID: &str = "01996500-0000-7000-8000-0000000000a1";

/// Tekst, którym `RAISE(ABORT, …)` w wyzwalaczu ma odmówić.
///
/// Wpisany tutaj ręcznie, a nie zaimportowany ze schematu, i to jest cała jego wartość:
/// zaimportowana stała zgadzałaby się sama ze sobą, także wtedy, gdyby ktoś ustawił ją na pusty
/// napis. To jest jedyne miejsce, w którym ten kontrakt jest **napisany**, a nie odczytany.
const REFUSAL: &str = "events is append-only";

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

/// Zdarzenie transkryptu.
fn an_event(ts: i64, body: &str) -> NewEvent {
    NewEvent {
        run_id: RUN_ID.to_owned(),
        step_id: None,
        ts,
        kind: "assistant".to_owned(),
        level: "headline".to_owned(),
        body: Some(body.to_owned()),
    }
}

/// Treść odmowy, albo pusty napis, jeśli operacja się **udała**.
///
/// Osobna funkcja, bo ten wzorzec powtarza się cztery razy, a `unwrap()` na `Result` jest
/// w tym drzewie `deny` także w plikach testowych — `[workspace.lints]` obowiązuje całe drzewo,
/// a `checks/full-clippy.sh` woła `cargo clippy --all-targets`.
fn refusal_text(outcome: &rusqlite::Result<usize>) -> String {
    outcome
        .as_ref()
        .err()
        .map(ToString::to_string)
        .unwrap_or_default()
}

#[tokio::test]
async fn a_connection_that_never_saw_our_rust_still_cannot_rewrite_history() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("loadout.db");

    // ── Trzy zdarzenia, przez store::writer ────────────────────────────────────────────────
    let store = Store::open(&db)?;
    let writer = store.writer();
    writer.insert_run(a_run()).await?;
    for line in 1..=3_i64 {
        writer
            .append_events(vec![an_event(
                1_755_300_001_000 + line,
                &format!("line {line}"),
            )])
            .await?;
    }
    store.close().await?;

    // ── Połączenie otwarte BEZPOŚREDNIO tutaj, z prawem zapisu ─────────────────────────────
    // To jest cała treść tego kryterium: ten obiekt nigdy nie widział naszego Rusta i nie ma
    // jak zapytać go o pozwolenie.
    let bare = Connection::open(&db)?;

    // Kontrola przeciw pustej asercji: bez tych trzech wierszy `UPDATE` odmówiłby dlatego,
    // że nie ma czego aktualizować, a `count(*) = 4` niżej wyszłoby z zupełnie innego biegu.
    let planted: i64 = bare.query_row("SELECT count(*) FROM events", [], |row| row.get(0))?;
    assert_eq!(
        planted, 3,
        "the three events written through store::writer are not in the file, so nothing below \
         is about a trigger — an UPDATE that matches no row succeeds quietly"
    );

    // ── (a) UPDATE ─────────────────────────────────────────────────────────────────────────
    let update = bare.execute("UPDATE events SET body = 'x' WHERE seq = 1", []);
    let update_said = refusal_text(&update);
    assert!(
        update.is_err(),
        "a plain writable connection rewrote a row of events. Everything about this subsystem \
         that is worth anything rests on the transcript being what happened, and Rust-side \
         checks do not reach a connection that never called them"
    );
    assert!(
        update_said.contains(REFUSAL),
        "events refused the UPDATE, but not with the words its trigger raises. That difference \
         matters: a whole-database refusal (no write permission, a read-only file) looks exactly \
         like a trigger from here, and it would also refuse the writes in (c) that MUST work. \
         SQLite said: {update_said:?}"
    );

    // ── (b) DELETE ─────────────────────────────────────────────────────────────────────────
    let delete = bare.execute("DELETE FROM events WHERE seq = 1", []);
    let delete_said = refusal_text(&delete);
    assert!(
        delete.is_err(),
        "a plain writable connection deleted a row of events. A transcript with a hole in it is \
         worse than no transcript, because nobody can tell the hole is there"
    );
    assert!(
        delete_said.contains(REFUSAL),
        "events refused the DELETE, but not with the words its trigger raises. SQLite said: \
         {delete_said:?}"
    );

    // ── (c) Kontrola dodatnia, na TYM SAMYM połączeniu ─────────────────────────────────────
    // Bez tego bloku baza otwarta w całości tylko do odczytu przeszłaby (a) i (b) nie mając
    // ani jednego wyzwalacza.
    let mutate_run = bare.execute("UPDATE runs SET status = 'succeeded'", []);
    assert!(
        mutate_run.is_ok(),
        "runs is a mutable table by design [T7 §5.1] and this connection could not write to it, \
         so (a) and (b) above measured a read-only database, not a trigger: {:?}",
        refusal_text(&mutate_run)
    );

    let append = bare.execute(
        "INSERT INTO events (run_id, ts, kind, level, body) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            RUN_ID,
            1_755_300_005_000_i64,
            "assistant",
            "headline",
            "line 4"
        ],
    );
    assert!(
        append.is_ok(),
        "events is append-ONLY, not read-only: it has to keep growing from any connection. \
         A trigger that refuses INSERT stops the run itself: {:?}",
        refusal_text(&append)
    );

    // ── (d) Stan, nie kod powrotu (niezmiennik 19) ─────────────────────────────────────────
    let total: i64 = bare.query_row("SELECT count(*) FROM events", [], |row| row.get(0))?;
    assert_eq!(
        total, 4,
        "events should hold the three rows written through our API plus the one appended \
         directly. Any other number means one of the four operations above did something other \
         than what its return value claimed"
    );

    let first: String = bare.query_row("SELECT body FROM events WHERE seq = 1", [], |row| {
        row.get(0)
    })?;
    assert_eq!(
        first, "line 1",
        "the first event's body changed. The UPDATE reported an error and went through anyway, \
         which is the one outcome nobody checks for"
    );

    Ok(())
}

/// (e) `DELETE FROM steps` przy żywym biegu, czyli kasowanie transkryptu bez słowa `events`.
///
/// Osobna funkcja i osobna baza, bo `clippy::too_many_lines` obowiązuje także pliki testowe
/// (`checks/full-clippy.sh` woła `--all-targets`), a żadnej asercji wyżej nie wolno zdjąć,
/// żeby zrobić tu miejsce.
///
/// Ta droga omija (a) i (b) w całości: `events.step_id` niesie `ON DELETE CASCADE`, więc krok
/// zabiera ze sobą swoje linie transkryptu, a zdanie SQL nie wspomina o `events` ani razu.
/// Skrypt naprawczy („usuń nieudany krok i ponów") napisze dokładnie to.
#[tokio::test]
async fn deleting_a_step_of_a_live_run_cannot_take_its_transcript_with_it() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("loadout.db");

    // Sam bieg wchodzi przez nasze API, bo `runs` musi istnieć, zanim cokolwiek się o niego
    // oprze; wszystko, co to kryterium mierzy, dzieje się niżej, na gołym połączeniu.
    let store = Store::open(&db)?;
    let writer = store.writer();
    writer.insert_run(a_run()).await?;
    store.close().await?;

    let bare = Connection::open(&db)?;

    let step = bare.execute(
        "INSERT INTO steps (id, run_id, node_key, name, agent, status) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            STEP_ID,
            RUN_ID,
            "implement",
            "Fix the CSV parser",
            "claude",
            "succeeded"
        ],
    );
    assert!(
        step.is_ok(),
        "steps is a mutable table by design [T7 §5.1] and this connection could not write to it, \
         so there is no step to cascade from and the DELETE below would measure nothing: {:?}",
        refusal_text(&step)
    );

    let owned = bare.execute(
        "INSERT INTO events (run_id, step_id, ts, kind, level, body) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            RUN_ID,
            STEP_ID,
            1_755_300_006_000_i64,
            "assistant",
            "headline",
            "line 5"
        ],
    );
    assert!(
        owned.is_ok(),
        "the event that hangs off the step did not go in, so the DELETE below would cascade into \
         an empty set and succeed for a reason that has nothing to do with the trigger: {:?}",
        refusal_text(&owned)
    );

    // Kaskada istnieje wyłącznie przy `foreign_keys` ON, a domyślnej wartości nie wolno tu
    // zakładać w ŻADNĄ stronę: ten bundled SQLite kompiluje się z -DSQLITE_DEFAULT_FOREIGN_KEYS=1
    // (libsqlite3-sys 0.38.2, build.rs:153), podręcznikowy SQLite ma 0. Ustawiamy jawnie
    // i sprawdzamy odczytem, żeby czerwień z (e) mogła pochodzić wyłącznie od wyzwalacza,
    // a nie od kaskady, której nie było (zmierzone 2026-08-16).
    bare.pragma_update(None, "foreign_keys", 1_i64)?;
    let cascades_on: i64 = bare.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
    assert_eq!(
        cascades_on, 1,
        "foreign_keys is {cascades_on} on this connection, so DELETE FROM steps would not reach \
         events at all and the assertions below would pass without a single trigger in the file"
    );

    let cascade = bare.execute("DELETE FROM steps WHERE id = ?1", params![STEP_ID]);
    let cascade_said = refusal_text(&cascade);
    assert!(
        cascade.is_err(),
        "deleting a step of a run that is still there took its transcript lines with it. The \
         statement never names `events`, which is exactly why this path is the one that gets \
         written by accident — and the hole it leaves is invisible from the outside"
    );
    assert!(
        cascade_said.contains(REFUSAL),
        "the cascade was refused, but not with the words events_reject_delete raises. A refusal \
         from anywhere else (a foreign key without the cascade, a read-only file) looks identical \
         from here and would say nothing about append-only. SQLite said: {cascade_said:?}"
    );

    // Stan, nie kod powrotu (niezmiennik 19): RAISE(ABORT) ma wycofać całe zdanie, więc krok
    // stoi na miejscu razem ze swoją linią. Bez tego przechodzi też implementacja, która oddaje
    // błąd po skasowaniu wiersza.
    let step_left: i64 = bare.query_row(
        "SELECT count(*) FROM steps WHERE id = ?1",
        params![STEP_ID],
        |row| row.get(0),
    )?;
    assert_eq!(
        step_left, 1,
        "the step is gone even though the DELETE reported an error, so the abort did not roll the \
         statement back"
    );

    let owned_left: i64 = bare.query_row(
        "SELECT count(*) FROM events WHERE step_id = ?1",
        params![STEP_ID],
        |row| row.get(0),
    )?;
    assert_eq!(
        owned_left, 1,
        "the step's transcript line is gone even though the DELETE reported an error. That is the \
         worst of the outcomes here: the caller believes nothing happened and the transcript has \
         a hole"
    );

    Ok(())
}
