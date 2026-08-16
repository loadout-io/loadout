//! AC-6 dla T-20: nieczytelny wiersz jest wypisany, nie pominięty i nie fatalny.
//!
//! Te wiersze zapisała **starsza** wersja Loadouta (niezmiennik 5). Nieznany status kroku,
//! nieznany status biegu, brak `session_id` przy kroku w `running`, próba, której nie da się
//! powiększyć — żadne z nich nie ma prawa wywołać paniki. Panika w `recovery.rs` to aplikacja,
//! która nie startuje **dokładnie po tym, jak się wywaliła**, czyli w jedynym momencie, kiedy
//! użytkownik jej potrzebuje.
//!
//! **Słaba wersja tego kryterium to
//! `assert!(std::panic::catch_unwind(|| decide(&rows, &m)).is_ok())`.** Spełnia ją funkcja,
//! która przy pierwszym nieznanym stringu zwraca pusty plan i porzuca **także trzy dobre
//! wiersze** — awaria cicha i uprzejma, czyli najgorszy z możliwych wariantów: trzej agenci
//! zostają przy życiu, a aplikacja melduje spokojny start. Dlatego to kryterium nie sprawdza
//! braku paniki wprost. Sprawdza, że po przejściu przez cztery nieczytelne wiersze **trzy dobre
//! są obsłużone w całości** — a że `decide` w ogóle wróciło, wynika z tego samo.
//!
//! Kryterium obejmuje też brak `unwrap()` na ścieżce wykonania. `cargo clippy` z polityką repo
//! (`unwrap_used = "deny"`) złapałby go i bez tego, ale test ma te ścieżki **wywołać** — zieleń
//! lintu jest twierdzeniem o kształcie kodu, nie o tym, że kod tamtędy przeszedł.

use loadout_lib::recovery::{self, Machine, RecoveryPlan, RecoveryRow};

/// Czas startu systemu — zgodny, żeby to kryterium nie mierzyło strażnika z AC-1.
const BOOT: &str = "1786900000";
/// Własna grupa Loadouta.
const OWN_PGID: i32 = 501;

/// Bieg w znanym stanie. Leży w nim sześć z siedmiu wierszy.
const RUN_MAIN: &str = "0199ab00-0000-7000-8000-000000000601";
/// Bieg w stanie, którego ta wersja Loadouta nie zna.
const RUN_DRAINING: &str = "0199ab00-0000-7000-8000-000000000602";

/// Grupy trzech poprawnych wierszy — jedyne, które wolno tknąć.
const GOOD_PGIDS: [i32; 3] = [6011, 6012, 6013];

fn row(
    step_id: &str,
    run_id: &str,
    run_status: &str,
    step_status: &str,
    pgid: i32,
    session_id: Option<&str>,
    attempt: i64,
) -> RecoveryRow {
    RecoveryRow {
        step_id: step_id.to_owned(),
        run_id: run_id.to_owned(),
        run_status: run_status.to_owned(),
        step_status: step_status.to_owned(),
        run_boot_id: Some(BOOT.to_owned()),
        pid: Some(pgid),
        pgid: Some(pgid),
        session_id: session_id.map(str::to_owned),
        attempt,
    }
}

/// Poprawny wiersz: bieg `running`, krok nieskończony, sesja na miejscu.
fn good(step_id: &str, step_status: &str, pgid: i32) -> RecoveryRow {
    row(
        step_id,
        RUN_MAIN,
        "running",
        step_status,
        pgid,
        Some("5f6d1c22-0000-4000-8000-000000000000"),
        0,
    )
}

/// Siedem wierszy: cztery nieczytelne, przemieszane z trzema poprawnymi.
///
/// Przemieszane celowo. Gdyby cztery złe stały na końcu, implementacja przerywająca pętlę na
/// pierwszym nieznanym stringu i tak zdążyłaby obsłużyć trzy dobre — czyli przeszłaby test,
/// nie mając tej własności, której test szuka.
fn rows() -> Vec<RecoveryRow> {
    vec![
        // Status kroku, którego nasz enum nie zna. Wartość z drutu dołożona przez przyszłą
        // wersję: nie wiemy, czy krok się skończył, więc nie wiemy, czy wolno strzelać.
        row(
            "row-unknown-step-status",
            RUN_MAIN,
            "running",
            "zombie",
            6001,
            Some("5f6d1c22-0000-4000-8000-000000000001"),
            0,
        ),
        good("good-1", "running", GOOD_PGIDS[0]),
        // Status biegu, którego nasz enum nie zna.
        row(
            "row-unknown-run-status",
            RUN_DRAINING,
            "draining",
            "running",
            6002,
            Some("5f6d1c22-0000-4000-8000-000000000002"),
            0,
        ),
        good("good-2", "running", GOOD_PGIDS[1]),
        // Krok w `running` bez sesji: istnieje proces, którego sesji nie umiemy nazwać, więc
        // nie ma czego zaproponować w pytaniu [T7 §6.2, V].
        row(
            "row-no-session",
            RUN_MAIN,
            "running",
            "running",
            6003,
            None,
            0,
        ),
        good("good-3", "ready", GOOD_PGIDS[2]),
        // Próba, której nie da się powiększyć. `attempt + 1` na tej wartości się przekręca,
        // a próba mniejsza od poprzedniej to ponowienie, które ląduje na wierszu innej próby.
        row(
            "row-huge-attempt",
            RUN_MAIN,
            "running",
            "running",
            6004,
            Some("5f6d1c22-0000-4000-8000-000000000004"),
            i64::MAX,
        ),
    ]
}

/// Kroki, o które plan pyta, posortowane.
fn asked_steps(plan: &RecoveryPlan) -> Vec<String> {
    let mut ids: Vec<String> = plan
        .ask
        .iter()
        .map(|question| question.step_id.clone())
        .collect();
    ids.sort();
    ids
}

/// Kroki wypisane jako nieczytelne, posortowane.
fn unreadable_ids(plan: &RecoveryPlan) -> Vec<String> {
    let mut ids: Vec<String> = plan
        .unreadable
        .iter()
        .map(|entry| entry.step_id.clone())
        .collect();
    ids.sort();
    ids
}

/// Zmiany statusu kroków jako posortowane wiersze.
fn step_lines(plan: &RecoveryPlan) -> Vec<String> {
    let mut lines: Vec<String> = plan
        .step_status
        .iter()
        .map(|change| {
            format!(
                "{} -> {} / {}",
                change.step_id, change.status, change.reason
            )
        })
        .collect();
    lines.sort();
    lines
}

#[test]
fn four_unreadable_rows_are_named_and_the_three_good_ones_are_handled_in_full() {
    let machine = Machine {
        boot_id: BOOT.to_owned(),
        own_pgid: OWN_PGID,
    };

    // Samo to wywołanie przechodzi przez każdą ze ścieżek, na których kusi `unwrap()`:
    // `session_id` bez wartości, status spoza enuma, `attempt`, którego nie da się powiększyć.
    let plan = recovery::decide(&rows(), &machine);

    // ── Trzy poprawne wiersze, obsłużone w całości ─────────────────────────────────────────
    assert_eq!(
        plan.reap,
        GOOD_PGIDS.to_vec(),
        "the three readable rows have orphans to clean up and none of the four unreadable ones \
         does — we do not know enough about them to send a signal. An implementation that gives \
         up on the first unknown string returns an empty list here, and that is the polite, \
         silent failure: three agents survive and the app reports a quiet start"
    );
    assert_eq!(
        asked_steps(&plan),
        vec![
            "good-1".to_owned(),
            "good-2".to_owned(),
            "good-3".to_owned()
        ],
        "one question per readable interrupted step"
    );
    for wanted in [
        "good-1 -> failed / interrupted",
        "good-2 -> failed / interrupted",
        "good-3 -> failed / interrupted",
    ] {
        assert!(
            step_lines(&plan).iter().any(|line| line == wanted),
            "the plan does not write {wanted:?}. The three readable rows have to be handled in \
             full — reap, status and question — not just counted. Plan wrote {:?}",
            step_lines(&plan)
        );
    }

    // ── Cztery nieczytelne wiersze, każdy po nazwie ────────────────────────────────────────
    assert_eq!(
        unreadable_ids(&plan),
        vec![
            "row-huge-attempt".to_owned(),
            "row-no-session".to_owned(),
            "row-unknown-run-status".to_owned(),
            "row-unknown-step-status".to_owned(),
        ],
        "each of the four rows an older Loadout wrote has to come back named. Dropping one is \
         how a step disappears from a run that a human is still looking at, and panicking on one \
         is an app that will not start immediately after the crash that made it need to"
    );

    for entry in &plan.unreadable {
        assert!(
            !entry.reason.trim().is_empty(),
            "the entry for {} carries no reason at all",
            entry.step_id
        );
        assert!(
            !entry.reason.contains('\n'),
            "the reason for {} spans more than one line; this is one sentence in a list a human \
             reads after a crash, not a report: {:?}",
            entry.step_id,
            entry.reason
        );
        // „Po angielsku" jest osądem człowieka i tak zostaje. Mechanicznie da się sprawdzić
        // tylko jedną połowę: zdanie po angielsku nie ma znaków diakrytycznych, a dokumentacja
        // tego repo jest pełna ą, ę i ł. To jest proxy, nie dowód, i mówimy to wprost (D5).
        assert!(
            entry.reason.is_ascii(),
            "the reason for {} is not ASCII, so it is very likely not the English sentence D5 \
             asks for: {:?}",
            entry.step_id,
            entry.reason
        );
    }
}
