//! AC-1 dla T-20: zmiana czasu startu systemu wyłącza sprzątanie po `pgid` całkowicie.
//!
//! `kern.maxproc` na macOS wynosi 16 000 [T7 §6.3, V]. PID-y przewijają się w godzinach, więc po
//! restarcie maszyny zapisany `pgid` z dużym prawdopodobieństwem należy do czegoś zupełnie
//! niewinnego, a `killpg` po nim jest błędem poprawności, nie ryzykiem teoretycznym
//! [T7 ryzyko 2]. Strażnikiem jest czas startu systemu — nie „sprawdzimy, czy proces wygląda
//! znajomo".
//!
//! **Słaba wersja tego kryterium to `assert!(plan.reap.is_empty())` po zmianie czasu startu.**
//! Przechodzi ją implementacja, która nie sprząta **nigdy** — czyli dokładnie ten wariant, który
//! w produkcji zostawia agenta na całą noc i pali limit do rana. Dlatego trzy przypadki stoją
//! w jednej funkcji testowej, obok siebie:
//!
//! 1. zgodny czas startu → `reap` jest dokładnie `[4321, 4322, 4323]`, a nie „coś niepustego",
//! 2. inny czas startu → `reap` puste, ale te same trzy kroki nadal dostają status,
//! 3. brak czasu startu w wierszu → `reap` puste, a wiersze idą do `plan.unreadable`.
//!
//! Wiersz trzeci jest osobnym przypadkiem, bo brak strażnika **nie jest** zgodą na strzał, i nie
//! jest też tym samym co strażnik, który powiedział „nie": drugi przypadek to decyzja („restart
//! już zabił sieroty"), trzeci to niewiedza. Dlatego w drugim `plan.unreadable` jest puste,
//! a w trzecim nie.
//!
//! **Dwa kroki `succeeded` w fikstrze niosą `pgid`**, choć są skończone. To jest celowe:
//! kolumna `steps.pgid` zostaje wypełniona po zakończeniu kroku, więc implementacja, która
//! zbiera po prostu „wszystkie wiersze z `pgid`", zwróciłaby pięć liczb zamiast trzech. Bez tych
//! dwóch wierszy kryterium przepuszczałoby filtr, który nie patrzy na status.

use loadout_lib::recovery::{self, Machine, RecoveryPlan, RecoveryRow};

/// Czas startu maszyny, na której Loadout właśnie wstał.
const BOOT_NOW: &str = "1786900000";
/// Czas startu zapisany przy biegu, sprzed restartu maszyny.
const BOOT_BEFORE: &str = "1786800000";
/// Własna grupa Loadouta. Trzymana z dala od `pgid`-ów z fikstury, żeby nie mieszać się
/// z kryterium AC-2 — tam ta liczba jest treścią, tutaj tylko tłem.
const OWN_PGID: i32 = 501;

/// Bieg, do którego należy całe pięć wierszy.
const RUN: &str = "0199ab00-0000-7000-8000-000000000001";

/// Pierwszy przerwany krok.
const STEP_A: &str = "step-a";
/// Drugi przerwany krok.
const STEP_B: &str = "step-b";
/// Krok, który dostał permit, ale nie zdążył wystartować.
const STEP_C: &str = "step-c";
/// Krok skończony przed awarią — z `pgid`, który po nim został.
const STEP_D: &str = "step-d";
/// Drugi krok skończony przed awarią.
const STEP_E: &str = "step-e";

/// Jeden wiersz. `pid` równy `pgid`, bo na uniksie lider grupy to proces, który uruchomiliśmy.
fn row(step_id: &str, step_status: &str, pgid: i32, boot: Option<&str>) -> RecoveryRow {
    RecoveryRow {
        step_id: step_id.to_owned(),
        run_id: RUN.to_owned(),
        run_status: "running".to_owned(),
        step_status: step_status.to_owned(),
        run_boot_id: boot.map(str::to_owned),
        pid: Some(pgid),
        pgid: Some(pgid),
    }
}

/// Ten sam zestaw pięciu wierszy, przepuszczany trzy razy z różnym czasem startu.
fn rows(boot: Option<&str>) -> Vec<RecoveryRow> {
    vec![
        row(STEP_A, "running", 4321, boot),
        row(STEP_B, "running", 4322, boot),
        row(STEP_C, "ready", 4323, boot),
        row(STEP_D, "succeeded", 4319, boot),
        row(STEP_E, "succeeded", 4320, boot),
    ]
}

fn machine() -> Machine {
    Machine {
        boot_id: BOOT_NOW.to_owned(),
        own_pgid: OWN_PGID,
    }
}

/// `plan.step_status` jako czytelne wiersze. Formatuje **status i powód razem**, bo cała
/// pomyłka z `docs/ARCHITECTURE.md` §5 polega na wpisaniu `interrupted` w kolumnę statusu kroku
/// zamiast w kolumnę powodu — a wtedy tylko jedna z tych dwóch wartości jest zła.
fn status_lines(plan: &RecoveryPlan) -> Vec<String> {
    plan.step_status
        .iter()
        .map(|change| {
            format!(
                "{} -> {} / {}",
                change.step_id, change.status, change.reason
            )
        })
        .collect()
}

/// Kroki wypisane jako nieczytelne, w kolejności planu.
fn unreadable_ids(plan: &RecoveryPlan) -> Vec<String> {
    plan.unreadable
        .iter()
        .map(|entry| entry.step_id.clone())
        .collect()
}

/// Trzy zmiany statusu, których wymagają wszystkie trzy przypadki.
fn wanted_status_lines() -> Vec<String> {
    vec![
        format!("{STEP_A} -> failed / interrupted"),
        format!("{STEP_B} -> failed / interrupted"),
        format!("{STEP_C} -> failed / interrupted"),
    ]
}

fn run_lines(plan: &RecoveryPlan) -> Vec<String> {
    plan.run_status
        .iter()
        .map(|change| format!("{} -> {}", change.run_id, change.status))
        .collect()
}

fn changed_step_ids(plan: &RecoveryPlan) -> Vec<String> {
    plan.step_status
        .iter()
        .map(|change| change.step_id.clone())
        .collect()
}

#[test]
fn a_changed_boot_time_turns_reaping_off_and_nothing_else_off() {
    let machine = machine();

    // ── 1. Ten sam czas startu: sprzątamy, i to dokładnie te trzy grupy ────────────────────
    // To jest asercja rozstrzygająca całego kryterium. Bez niej implementacja, która nie
    // sprząta nigdy, przechodzi oba pozostałe przypadki i zostawia agenta na całą noc.
    let same = recovery::decide(&rows(Some(BOOT_NOW)), &machine);
    assert_eq!(
        same.reap,
        vec![4321, 4322, 4323],
        "with the recorded boot time equal to this machine's, the two running steps and the \
         ready one have to be reaped, in row order. The two finished steps carry a pgid as well \
         (4319, 4320) and must NOT be here — a filter that collects every row with a pgid \
         instead of every unfinished row lands exactly there. Plan said {:?}",
        same.reap
    );
    assert!(
        same.unreadable.is_empty(),
        "every one of these five rows is readable, so nothing belongs in unreadable: {:?}",
        same.unreadable
    );
    assert_eq!(
        status_lines(&same),
        wanted_status_lines(),
        "the two running steps and the ready one go to failed with reason interrupted. \
         ARCHITECTURE §5: interrupted is a status of the RUN; the step goes to failed and the \
         reason is a separate field, and store::schema's CHECK will not accept a step whose \
         status column says interrupted"
    );
    assert_eq!(
        run_lines(&same),
        vec![format!("{RUN} -> interrupted")],
        "one proven cut-off row is enough to mark this run, but five rows must not duplicate it"
    );
    assert_eq!(
        changed_step_ids(&same),
        vec![STEP_A.to_owned(), STEP_B.to_owned(), STEP_C.to_owned()],
        "the finished rows keep their status even though they still carry process groups"
    );

    // ── 2. Inny czas startu: nie sprzątamy, ale nadal zapisujemy przerwanie ────────────────
    // Restart maszyny już zabił sieroty, więc nie ma czego sprzątać. Krok nadal został
    // przerwany w połowie, więc recovery zapisuje ten fakt i nie konstruuje dalszego działania.
    let rebooted = recovery::decide(&rows(Some(BOOT_BEFORE)), &machine);
    assert!(
        rebooted.reap.is_empty(),
        "the run recorded boot time {BOOT_BEFORE} and this machine booted at {BOOT_NOW}, so \
         every recorded pgid may now belong to a stranger: kern.maxproc is 16000 on macOS and \
         PIDs recycle [T7 §6.3, V]. Nothing may be reaped, and this plan wants to reap {:?}",
        rebooted.reap
    );
    assert!(
        rebooted.unreadable.is_empty(),
        "a boot time that says 'the machine restarted' is an ANSWER, not a gap: we know the \
         orphans are gone. Only a MISSING boot time is unreadable. Plan said {:?}",
        rebooted.unreadable
    );
    assert_eq!(
        status_lines(&rebooted),
        wanted_status_lines(),
        "the reboot killed the orphans, it did not finish the steps. The same three steps still \
         go to failed with reason interrupted"
    );
    assert_eq!(
        run_lines(&rebooted),
        run_lines(&same),
        "the run-status fact cannot depend on whether the machine rebooted"
    );
    assert_eq!(
        changed_step_ids(&rebooted),
        changed_step_ids(&same),
        "the same three rows were cut off on either boot; only the signal decision changes"
    );

    // ── 3. Brak czasu startu: nie sprzątamy i mówimy to głośno ─────────────────────────────
    let ancient = recovery::decide(&rows(None), &machine);
    assert!(
        ancient.reap.is_empty(),
        "the row predates the boot-time column, so there is no guard at all. No guard is not \
         permission to shoot: a recorded pgid with nothing to date it against is exactly the \
         stranger's process this criterion exists to protect. Plan wants to reap {:?}",
        ancient.reap
    );
    assert_eq!(
        unreadable_ids(&ancient),
        vec![STEP_A.to_owned(), STEP_B.to_owned(), STEP_C.to_owned()],
        "the three unfinished rows could not be decided about, so each has to be named in \
         unreadable — a row that vanishes silently is the failure mode this list exists for. \
         The two finished rows decide nothing and belong nowhere"
    );
    assert!(
        ancient.run_status.is_empty(),
        "without a boot marker no live row is proven cut off, so the run cannot be marked: {:?}",
        ancient.run_status
    );
    assert!(
        ancient.step_status.is_empty(),
        "without a boot marker no live step is proven cut off, so no step status can change: \
         {:?}",
        ancient.step_status
    );
    for entry in &ancient.unreadable {
        assert!(
            !entry.reason.trim().is_empty(),
            "the unreadable entry for {} carries no reason, so nobody reading the log learns \
             why the pgid was left alone",
            entry.step_id
        );
    }
}
