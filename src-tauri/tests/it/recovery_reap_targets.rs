//! AC-2 dla T-20: do sprzątania trafiają wyłącznie `pgid`, których zabicie jest bezpieczne.
//!
//! Sześć wierszy, wszystkie w stanie, w którym sprzątanie w ogóle wchodzi w grę, i wszystkie
//! z tym samym czasem startu systemu — strażnik z AC-1 przepuszcza tu wszystko, żeby to
//! kryterium mierzyło wyłącznie filtr po samej wartości `pgid`:
//!
//! | `pgid`           | dlaczego nie wolno |
//! |------------------|--------------------|
//! | `Some(0)`        | `0` w `killpg` znaczy **własna grupa wołającego** — Loadout zabija sam siebie |
//! | `None`           | spawn nie doszedł do zapisu; nie ma czego zabić |
//! | `Some(-9)`       | wartość ujemna nie jest grupą (znak jest selektorem w `kill`, nie częścią numeru) |
//! | `Some(own_pgid)` | to samo co `0`, tylko napisane wprost — Loadout w pętli startowej |
//!
//! Zostają dwa wiersze z `pgid = 4321`, drugi z nich po ponowieniu kroku. Do `reap` wchodzi
//! **jedna** liczba: dwa `SIGTERM` do tej samej grupy to drugi sygnał wysłany do grupy, która
//! już nie istnieje.
//!
//! **Słaba wersja tego kryterium to `assert_eq!(plan.reap.len(), 1)`.** Przechodzi ją
//! implementacja, która bierze **ostatni** wiersz zamiast filtrować — przy tym zestawie długość
//! też wynosi 1, bo ostatni wiersz akurat niesie 4321. Dlatego porównujemy cały wektor z
//! `vec![4321]` i osobno wymagamy, żeby cztery odrzucone wiersze **były wypisane po nazwie**:
//! filtr, który je odrzuca po cichu, i filtr, który ich w ogóle nie widzi, dają identyczne
//! `reap` i różnią się dopiero tutaj.

use loadout_lib::recovery::{self, Machine, RecoveryPlan, RecoveryRow};

/// Czas startu systemu — jeden i ten sam po obu stronach, więc strażnik z AC-1 nie ma tu nic
/// do roboty i nie zasłania mierzonego filtru.
const BOOT: &str = "1786900000";

/// Własna grupa procesów Loadouta. To jest treść jednego z sześciu wierszy, nie tło.
const OWN_PGID: i32 = 501;

/// Jedyny `pgid`, który wolno zabić.
const SAFE_PGID: i32 = 4321;

/// Bieg, do którego należy całe sześć wierszy.
const RUN: &str = "0199ab00-0000-7000-8000-000000000002";

/// Krok z `pgid = 0`.
const STEP_ZERO: &str = "step-pgid-zero";
/// Krok bez `pgid`.
const STEP_MISSING: &str = "step-pgid-missing";
/// Krok z ujemnym `pgid`.
const STEP_NEGATIVE: &str = "step-pgid-negative";
/// Krok, którego `pgid` jest naszą własną grupą.
const STEP_OURS: &str = "step-pgid-ours";
/// Pierwsza próba kroku, którego grupę wolno zabić.
const STEP_SAFE: &str = "step-safe-first-try";
/// Druga próba tego samego kroku — ten sam `pgid`.
const STEP_SAFE_AGAIN: &str = "step-safe-second-try";

fn row(step_id: &str, step_status: &str, pgid: Option<i32>) -> RecoveryRow {
    RecoveryRow {
        step_id: step_id.to_owned(),
        run_id: RUN.to_owned(),
        run_status: "running".to_owned(),
        step_status: step_status.to_owned(),
        run_boot_id: Some(BOOT.to_owned()),
        pid: pgid,
        pgid,
    }
}

/// Sześć wierszy w kolejności z kryterium.
fn rows() -> Vec<RecoveryRow> {
    vec![
        row(STEP_ZERO, "running", Some(0)),
        row(STEP_MISSING, "running", None),
        row(STEP_NEGATIVE, "running", Some(-9)),
        row(STEP_OURS, "running", Some(OWN_PGID)),
        row(STEP_SAFE, "running", Some(SAFE_PGID)),
        row(STEP_SAFE_AGAIN, "ready", Some(SAFE_PGID)),
    ]
}

/// Kroki wypisane jako nieczytelne, w kolejności planu.
fn unreadable_ids(plan: &RecoveryPlan) -> Vec<String> {
    plan.unreadable
        .iter()
        .map(|entry| entry.step_id.clone())
        .collect()
}

#[test]
fn only_a_pgid_that_is_safe_to_kill_reaches_the_reap_list() {
    let machine = Machine {
        boot_id: BOOT.to_owned(),
        own_pgid: OWN_PGID,
    };

    let plan = recovery::decide(&rows(), &machine);

    // ── Cały wektor, nie jego długość ──────────────────────────────────────────────────────
    assert_eq!(
        plan.reap,
        vec![SAFE_PGID],
        "of these six rows exactly one pgid is safe to kill. 0 means 'the caller's own group' \
         in killpg, so a row carrying it makes Loadout kill itself during startup and the crash \
         looks like a crash of the recovery loop; {OWN_PGID} is the same thing spelled out; -9 \
         is not a group at all, because the sign is the group selector in kill and not part of \
         the number; and a missing pgid means the spawn never got as far as writing one down. \
         The duplicate is gone because a second SIGTERM goes to a group that no longer exists. \
         Plan wants to reap {:?}",
        plan.reap
    );

    // ── Cztery odrzucone wiersze, każdy po nazwie ──────────────────────────────────────────
    // Bez tej asercji filtr, który odrzuca wiersz po cichu, jest nieodróżnialny od filtru,
    // który go w ogóle nie zobaczył: oba dają `reap == [4321]`.
    assert_eq!(
        unreadable_ids(&plan),
        vec![
            STEP_ZERO.to_owned(),
            STEP_MISSING.to_owned(),
            STEP_NEGATIVE.to_owned(),
            STEP_OURS.to_owned(),
        ],
        "the four rejected rows have to be named, each carrying its own step_id, and the two \
         rows holding {SAFE_PGID} must not be among them — the second of those is a retry of the \
         same step, and dropping a duplicate is a decision, not a defect. A row that disappears \
         without a word is the failure this list exists to prevent"
    );

    for entry in &plan.unreadable {
        assert!(
            !entry.reason.trim().is_empty(),
            "the unreadable entry for {} carries no reason. This list is read by a human after \
             a crash; an entry without a sentence tells them a row was dropped and nothing else",
            entry.step_id
        );
    }
}
