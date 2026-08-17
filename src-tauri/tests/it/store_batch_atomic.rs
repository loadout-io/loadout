//! AC-6 dla T-06: wsad 100 wierszy to jedna transakcja, a zły wiersz nie zabija pisarza.
//!
//! Sto wierszy na transakcję to zmierzona konfiguracja z [T7 §5.3] — 662 238 wierszy na sekundę
//! przy WAL i `synchronous=NORMAL`. Ale to kryterium nie jest o przepustowości i celowo nie ma
//! tu nic o czasie: liczby z T7 są kilka rzędów wielkości ponad potrzebą, a kryterium mierzące
//! czas mierzyłoby maszynę. To kryterium jest o dwóch rzeczach, których szybkość nie kupuje.
//!
//! **Pierwsza: wsad wraca w całości.** Słaba wersja to `assert!(result.is_err())`. Przechodzi na
//! implementacji, która wstawia wiersze 1–56, przewraca się na 57. i zostawia bazę w połowie —
//! a to jest **gorsze** niż brak wsadu, bo transkrypt ma wtedy dziurę, o której nikt nie wie.
//! Rozróżnia je równość licznika **sprzed** i **po** feralnym wsadzie.
//!
//! **Druga: pisarz to przeżywa.** Bez tego przechodzi implementacja, która ratuje atomowość,
//! kończąc zadanie pisarza — użytkownik dostaje wtedy bieg, w którym nic więcej się nie zapisze,
//! i aplikację, którą trzeba zrestartować po jednym złym zdarzeniu. Rozróżnia je **następny**
//! wsad, który ma wejść w całości.
//!
//! Zły wiersz jest zły dlatego, że łamie `CHECK` na `events.level` — nie dlatego, że nasz Rust
//! go odrzucił. To jest ta sama teza, co w AC-7: odmawia schemat, a my tylko przekazujemy jego
//! odmowę wołającemu.

use rusqlite::Connection;

use loadout_lib::store::{NewEvent, NewRun, Store};

/// Bieg, do którego należą wszystkie zdarzenia.
const RUN_ID: &str = "01996500-0000-7000-8000-000000000001";

/// Ile wierszy ma wsad [T7 §5.3].
///
/// `i64`, nie `usize`: ta liczba jest porównywana z `count(*)` z `SQLite`, a `usize as i64`
/// przewraca `clippy::cast_possible_wrap` w pełnej bramce. Rzutowanie w teście jest darmowe
/// do napisania i za każdym razem kosztuje rundę.
const BATCH: i64 = 100;

/// Który wiersz w nim jest zły. Liczony od jedynki, jak w treści kryterium.
const BAD_ROW: i64 = 57;

/// Poziom spoza `headline|detail|raw`. `String`, nie enum — bo o odmowie ma decydować `CHECK`
/// w schemacie, a nie typ w Ruście. Enum uczyniłby ten test niemożliwym do napisania, co samo
/// w sobie byłoby odpowiedzią na pytanie, gdzie mieszka odmowa.
const BAD_LEVEL: &str = "loud";

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

/// Sto zdarzeń. Kiedy `spoiled`, 57. z nich niesie poziom, którego `CHECK` nie przyjmie.
fn batch_of(prefix: &str, spoiled: bool) -> Vec<NewEvent> {
    (1..=BATCH)
        .map(|line| NewEvent {
            run_id: RUN_ID.to_owned(),
            step_id: None,
            ts: 1_755_300_002_000,
            kind: "assistant".to_owned(),
            level: if spoiled && line == BAD_ROW {
                BAD_LEVEL.to_owned()
            } else {
                "detail".to_owned()
            },
            body: Some(format!("{prefix} {line}")),
        })
        .collect()
}

/// Ile wierszy stoi w `events`.
fn events_in(conn: &Connection) -> anyhow::Result<i64> {
    Ok(conn.query_row("SELECT count(*) FROM events", [], |row| row.get(0))?)
}

/// Ile wierszy w `events` ma treść zaczynającą się od `prefix`.
fn events_from(conn: &Connection, prefix: &str) -> anyhow::Result<i64> {
    Ok(conn.query_row(
        "SELECT count(*) FROM events WHERE body LIKE ?1",
        [format!("{prefix}%")],
        |row| row.get(0),
    )?)
}

#[tokio::test]
async fn a_batch_with_one_bad_row_leaves_nothing_behind_and_the_writer_lives() -> anyhow::Result<()>
{
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("loadout.db");

    let store = Store::open(&db)?;
    let writer = store.writer();
    writer.insert_run(a_run()).await?;

    let reader = store.reader()?;

    // Kontrola przeciw pustej asercji: zanim cokolwiek porównamy, dobry wsad musi wchodzić.
    // Bez tego „liczba wierszy się nie zmieniła" jest prawdą także wtedy, gdy nie zapisuje się
    // nic, nigdy, a wsad z 57. wierszem odmawia z zupełnie innego powodu.
    writer
        .append_events(batch_of("warm up line", false))
        .await?;
    let before = events_in(&reader)?;
    assert_eq!(
        before, BATCH,
        "a clean batch of {BATCH} events did not land, so nothing below is about atomicity"
    );

    // ── Feralny wsad ───────────────────────────────────────────────────────────────────────
    let refused = writer.append_events(batch_of("bad batch line", true)).await;
    let said = refused
        .as_ref()
        .err()
        .map(ToString::to_string)
        .unwrap_or_default();
    assert!(
        refused.is_err(),
        "a batch whose {BAD_ROW}th row carries level {BAD_LEVEL:?} came back Ok. Either the CHECK \
         on events.level is not in the schema, or the writer swallowed the refusal — and a \
         swallowed refusal is a transcript that quietly disagrees with the log file it came from"
    );
    assert!(
        said.contains(&BATCH.to_string()),
        "the error came back without naming the batch. The caller has to learn that {BATCH} \
         events did not land, not that one row was refused: those two sentences ask for \
         different repairs. The error said: {said:?}"
    );

    let after = events_in(&reader)?;
    assert_eq!(
        after,
        before,
        "the bad batch left {} rows behind. One batch is one transaction — all {BATCH} rows or \
         none of them. Fifty-six orphans are worse than no batch at all, because the transcript \
         then has a hole nobody can see",
        after - before
    );
    assert_eq!(
        events_from(&reader, "bad batch line")?,
        0,
        "rows from the refused batch are in the transcript"
    );

    // ── Pisarz przeżył ─────────────────────────────────────────────────────────────────────
    let next = writer
        .append_events(batch_of("good batch line", false))
        .await;
    assert!(
        next.is_ok(),
        "the next, perfectly good batch did not go through, so the writer task died with the bad \
         row. An implementation that buys atomicity by ending the writer leaves the user with a \
         run that records nothing more and an app that needs restarting: {next:?}"
    );

    let settled = events_in(&reader)?;
    assert_eq!(
        settled,
        before + BATCH,
        "the good batch landed {} of its {BATCH} rows",
        settled - before
    );
    assert_eq!(
        events_from(&reader, "good batch line")?,
        BATCH,
        "the good batch is not in the transcript in full"
    );

    drop(reader);
    store.close().await?;
    Ok(())
}
