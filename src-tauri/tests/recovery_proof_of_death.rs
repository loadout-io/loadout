//! AC-5 dla T-20: bez dowodu śmierci grupa jest żywa, a `EPERM` jest sygnałem przewinięcia PID-a.
//!
//! Niezmiennik 6 czyta się dosłownie: dopóki `kill(-pgid, 0)` nie dał `ESRCH`, grupa jest żywa.
//! `kill` odpowiada jednak na trzy sposoby i **dwa z nich nie są śmiercią**:
//!
//! * `ESRCH` → [`ReapOutcome::ProvenDead`]. W grupie nie ma nikogo. Jedyny dowód, jaki istnieje.
//! * grupa odpowiada → [`ReapOutcome::StillAlive`]. Sierota żyje i pali limit dalej.
//! * `EPERM` → [`ReapOutcome::Foreign`]. Grupa **istnieje i należy do kogoś innego**, czyli
//!   `pgid` został przewinięty. To jest najgorszy z trzech, bo jest jedynym, który wygląda jak
//!   sukces: cichy błąd polega na potraktowaniu każdego niezerowego wyniku `kill` jako „już nie
//!   żyje" i zameldowaniu posprzątanego biegu.
//!
//! **Słaba wersja tego kryterium to `assert!(!report.unproven.is_empty())`.** Przechodzi ją
//! implementacja, która nigdy nie uznaje niczego za sprzątnięte i wszystko wrzuca do `unproven`
//! — raport, który zawsze mówi „nie wiem" i przez to nigdy się nie myli. Dlatego trzy wektory
//! są porównywane wprost, w jednym teście, a do tego liczy się wywołania domykacza: cudza grupa
//! nie dostaje `SIGKILL`, bo po tamtej stronie stoi dokładnie ten niewinny proces, przed którym
//! broni strażnik z AC-1.
//!
//! Trzy przebiegi tego samego planu z trzema różnymi domykaczami stoją tu obok siebie, bo
//! `is_clean()` jest zdaniem o **dwóch** listach naraz. Stała `false` przechodzi przebieg
//! pierwszy i trzeci, stała `true` przechodzi drugi; żadna nie przechodzi wszystkich trzech.

use loadout_lib::recovery::{self, ReapOutcome, RecoveryPlan};

/// Grupa, która naprawdę już nie żyje.
const DEAD: i32 = 4321;
/// Grupa, która nadal odpowiada na sygnał.
const ALIVE: i32 = 4322;
/// Grupa, która należy do kogoś innego.
const STRANGER: i32 = 4323;

/// Plan z trzema grupami do sprzątnięcia i niczym więcej.
///
/// Budowany wprost, a nie przez `decide`: to kryterium jest o [`recovery::apply`] i o niczym
/// innym. Wpuszczenie tu `decide` znaczyłoby, że błąd w wyborze celów przewraca także ten test
/// i trzeba czytać dwa kryteria, żeby wiedzieć, które zachowanie zniknęło.
fn plan() -> RecoveryPlan {
    RecoveryPlan {
        reap: vec![DEAD, ALIVE, STRANGER],
        ..RecoveryPlan::default()
    }
}

#[test]
fn only_esrch_counts_as_death_and_eperm_is_somebody_else() {
    let plan = plan();

    // ── Przebieg 1: po jednym z każdego ────────────────────────────────────────────────────
    let mut calls: Vec<i32> = Vec::new();
    let report = {
        let mut closer = |pgid: i32| {
            calls.push(pgid);
            match pgid {
                DEAD => ReapOutcome::ProvenDead,
                ALIVE => ReapOutcome::StillAlive,
                _ => ReapOutcome::Foreign,
            }
        };
        recovery::apply(&plan, &mut closer)
    };

    assert_eq!(
        report.reaped,
        vec![DEAD],
        "only the group that answered ESRCH may be reported as reaped. Everything else is a \
         group we sent a signal to and learned nothing about — and 'we sent a signal' is the \
         sentence invariant 6 exists to keep out of a report"
    );
    assert_eq!(
        report.unproven,
        vec![ALIVE],
        "the group that still answers is alive, and an orphaned claude burns quota in the \
         background: that is a money error, not a hygiene one"
    );
    assert_eq!(
        report.foreign,
        vec![STRANGER],
        "EPERM is NOT proof of death. It means the group exists and belongs to somebody else, \
         which means the pgid was recycled — kern.maxproc is 16000 on macOS [T7 §6.3, V]. A \
         report that files this one under reaped claims to have cleaned up a run it never touched"
    );
    assert_eq!(
        calls,
        vec![DEAD, ALIVE, STRANGER],
        "each group gets exactly one call, in plan order. A second call on {STRANGER} would be \
         the escalation to SIGKILL, and the thing on the other end is the innocent process the \
         boot-time guard from AC-1 exists to protect"
    );
    assert!(
        !report.is_clean(),
        "one group is still alive and one belongs to a stranger, so this recovery is not clean. \
         A report that calls itself clean here is the one a human reads before closing the lid \
         on a laptop that keeps spending money"
    );

    // ── Przebieg 2: wszystko z dowodem ─────────────────────────────────────────────────────
    let mut every_call: Vec<i32> = Vec::new();
    let clean = {
        let mut closer = |pgid: i32| {
            every_call.push(pgid);
            ReapOutcome::ProvenDead
        };
        recovery::apply(&plan, &mut closer)
    };
    assert_eq!(
        clean.reaped,
        vec![DEAD, ALIVE, STRANGER],
        "every group answered ESRCH, so every group is reaped"
    );
    assert!(
        clean.unproven.is_empty() && clean.foreign.is_empty(),
        "nothing was left in doubt: unproven {:?}, foreign {:?}",
        clean.unproven,
        clean.foreign
    );
    assert!(
        clean.is_clean(),
        "with proof for all three groups this report IS clean. Without this line an \
         implementation that answers 'not clean' to everything passes the whole criterion by \
         never being sure of anything"
    );
    assert_eq!(
        every_call,
        vec![DEAD, ALIVE, STRANGER],
        "still one call per group"
    );

    // ── Przebieg 3: sama cudza grupa też nie jest czystym raportem ─────────────────────────
    let strangers = {
        let mut closer = |_pgid: i32| ReapOutcome::Foreign;
        recovery::apply(&plan, &mut closer)
    };
    assert!(
        strangers.reaped.is_empty(),
        "not one of these groups was proven dead, so not one may be reported as reaped: {:?}",
        strangers.reaped
    );
    assert_eq!(
        strangers.foreign,
        vec![DEAD, ALIVE, STRANGER],
        "all three pgids now belong to strangers, which is exactly what a reboot between the \
         crash and this start looks like from inside kill()"
    );
    assert!(
        !strangers.is_clean(),
        "nobody was killed and nothing went wrong, and the report is still not clean: our \
         orphans are somewhere we can no longer name. is_clean() is true only when unproven AND \
         foreign are both empty"
    );
}
