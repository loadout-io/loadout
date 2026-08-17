//! AC-6 dla T-20: nieczytelny wiersz jest wypisany, nie pominięty i nie fatalny.
//!
//! Te wiersze zapisała **starsza** wersja Loadouta (niezmiennik 5). Nieznany status kroku,
//! nieznany status biegu, brak `session_id` przy kroku w `running`, próba, której nie da się
//! powiększyć, i próba poniżej zera — żadne z nich nie ma prawa wywołać paniki. Panika
//! w `recovery.rs` to aplikacja, która nie startuje **dokładnie po tym, jak się wywaliła**,
//! czyli w jedynym momencie, kiedy użytkownik jej potrzebuje.
//!
//! Licznik prób psuje się na **dwa** sposoby i każdy ma tu swój wiersz. Kryterium w `TASK.md`
//! nazywa po imieniu tylko próbę „nienaturalnie dużą", więc jednym wierszem z `i64::MAX` da się
//! je spełnić w całości — a gałąź ujemnej próby nie wykonuje się wtedy ani razu i nikt się o tym
//! nie dowie (2026-08-16, druga opinia do T-20). Wiersz z `attempt = -1` stoi tu po to, żeby obie
//! gałęzie zostały **wywołane**, i każdy z tych dwóch wierszy jest sprawdzany po swoim własnym
//! zdaniu: gdyby test dopuszczał którekolwiek z dwóch, jeden wiersz „pokryłby" obie gałęzie.
//!
//! **Słaba wersja tego kryterium to
//! `assert!(std::panic::catch_unwind(|| decide(&rows, &m)).is_ok())`.** Spełnia ją funkcja,
//! która przy pierwszym nieznanym stringu zwraca pusty plan i porzuca **także trzy dobre
//! wiersze** — awaria cicha i uprzejma, czyli najgorszy z możliwych wariantów: trzej agenci
//! zostają przy życiu, a aplikacja melduje spokojny start. Dlatego to kryterium nie sprawdza
//! braku paniki wprost. Sprawdza, że po przejściu przez pięć nieczytelnych wierszy **trzy dobre
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

/// Bieg w znanym stanie. Leży w nim siedem z ośmiu wierszy.
const RUN_MAIN: &str = "0199ab00-0000-7000-8000-000000000601";
/// Bieg w stanie, którego ta wersja Loadouta nie zna.
const RUN_DRAINING: &str = "0199ab00-0000-7000-8000-000000000602";

/// Grupy trzech poprawnych wierszy — jedyne, które wolno tknąć.
const GOOD_PGIDS: [i32; 3] = [6011, 6012, 6013];

/// Zdanie, które ma dostać wiersz z próbą, której nie da się powiększyć.
///
/// Przepisane, a nie zaimportowane, i to jest celowe: `recovery::reason` jest prywatny, a nawet
/// gdyby nie był, porównanie stałej z samą sobą przechodzi po każdej zmianie tekstu. Tu stoi
/// zdanie, które zobaczy człowiek po awarii — jeśli się zmieni, ten test ma o tym powiedzieć.
const TRY_COUNT_MAXED: &str =
    "The number of tries written down for this step cannot be counted any higher.";
/// Zdanie dla wiersza z próbą poniżej zera. Inne niż powyższe, bo to inna gałąź.
const TRY_COUNT_BELOW_ZERO: &str = "The number of tries written down for this step is below zero, so the next try could not be \
     numbered.";

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

/// Osiem wierszy: pięć nieczytelnych, przemieszanych z trzema poprawnymi.
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
        // Próba poniżej zera. Tego licznika nie da się kontynuować w drugą stronę: numer
        // następnej próby wyszedłby mniejszy albo równy numerowi, który już gdzieś stoi,
        // więc ponowienie wylądowałoby na wierszu cudzej próby.
        row(
            "row-negative-attempt",
            RUN_MAIN,
            "running",
            "running",
            6005,
            Some("5f6d1c22-0000-4000-8000-000000000005"),
            -1,
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

/// Powód wypisany przy danym kroku, albo `None`, jeśli plan o nim nie mówi.
fn reason_for(plan: &RecoveryPlan, step_id: &str) -> Option<String> {
    plan.unreadable
        .iter()
        .find(|entry| entry.step_id == step_id)
        .map(|entry| entry.reason.clone())
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
fn five_unreadable_rows_are_named_and_the_three_good_ones_are_handled_in_full() {
    let machine = Machine {
        boot_id: BOOT.to_owned(),
        own_pgid: OWN_PGID,
    };

    // Samo to wywołanie przechodzi przez każdą ze ścieżek, na których kusi `unwrap()`:
    // `session_id` bez wartości, status spoza enuma, `attempt`, którego nie da się powiększyć,
    // i `attempt` poniżej zera.
    let plan = recovery::decide(&rows(), &machine);

    // ── Trzy poprawne wiersze, obsłużone w całości ─────────────────────────────────────────
    assert_eq!(
        plan.reap,
        GOOD_PGIDS.to_vec(),
        "the three readable rows have orphans to clean up and none of the five unreadable ones \
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

    // ── Pięć nieczytelnych wierszy, każdy po nazwie ────────────────────────────────────────
    assert_eq!(
        unreadable_ids(&plan),
        vec![
            "row-huge-attempt".to_owned(),
            "row-negative-attempt".to_owned(),
            "row-no-session".to_owned(),
            "row-unknown-run-status".to_owned(),
            "row-unknown-step-status".to_owned(),
        ],
        "each of the five rows an older Loadout wrote has to come back named. Dropping one is \
         how a step disappears from a run that a human is still looking at, and panicking on one \
         is an app that will not start immediately after the crash that made it need to"
    );

    // Dwa sposoby, na jakie psuje się licznik prób, dostają dwa różne zdania. Sprawdzane po
    // pełnym tekście, każdy osobno: gdyby wystarczyło „którekolwiek z dwóch", jeden wiersz
    // liczyłby się za obie gałęzie, a druga nie wykonałaby się ani razu.
    assert_eq!(
        reason_for(&plan, "row-huge-attempt").as_deref(),
        Some(TRY_COUNT_MAXED),
        "the row whose try counter cannot go any higher has to say so in that many words"
    );
    assert_eq!(
        reason_for(&plan, "row-negative-attempt").as_deref(),
        Some(TRY_COUNT_BELOW_ZERO),
        "a try counter below zero is a different thing from one that has run out of room, and \
         the human reading this list after a crash gets told which one happened"
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
