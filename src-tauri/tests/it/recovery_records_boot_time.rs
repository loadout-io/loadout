//! AC-3 dla T-35: czas startu maszyny JEST zapisany, bo bez niego odzyskiwanie nie ma czym sądzić.
//!
//! `recovery::decide` ma strażnika: strzela do grupy procesów dopiero wtedy, gdy zapisany przy
//! biegu czas startu systemu zgadza się z tym, co maszyna mówi teraz. Powód nie jest teoretyczny
//! — `kern.maxproc` na macOS wynosi 16 000, więc PID-y przewijają się w godzinach. Po restarcie
//! zapisany `pgid` z dużym prawdopodobieństwem należy do czegoś zupełnie niewinnego, a `killpg`
//! po nim jest błędem poprawności [T7 ryzyko 2].
//!
//! Do 2026-08-17 tego znacznika **nikt nigdzie nie zapisywał**: kolumny nie było w schemacie,
//! `sysctl kern.boottime` nie miał czytelnika, a `add_column_if_missing` — jedyna dozwolona
//! droga dokładania kolumn — istniało wyłącznie w komentarzach. Gdyby wpiąć odzyskiwanie w tym
//! stanie, każdy wiersz padłby na `NO_BOOT_TIME` i nic by nie posprzątało. Strażnik był w kodzie,
//! był zielony w testach `decide()` i nie strzeliłby nigdy.
//!
//! **Słaba wersja tego kryterium:** sprawdzenie, że pole istnieje. Przechodzi ją zero wpisane raz
//! przy migracji i przechodzi `NULL` w każdym wierszu. Rozróżnia dopiero KONTROLA NEGATYWNA
//! niżej: te same dane bez znacznika **muszą** dać `NO_BOOT_TIME`, inaczej test nie mierzy niczego.

use std::error::Error;

use loadout_lib::engine::supervisor::machine_booted_at;
use loadout_lib::recovery::{Machine, RecoveryRow, decide, reason};

/// Wiersz biegu zastanego w `running` po awarii — czyli dokładnie ten, który odzyskiwanie sądzi.
fn interrupted(boot: Option<&str>) -> RecoveryRow {
    RecoveryRow {
        step_id: "s1".to_owned(),
        run_id: "r1".to_owned(),
        run_status: "running".to_owned(),
        step_status: "running".to_owned(),
        run_boot_id: boot.map(str::to_owned),
        pid: Some(4242),
        pgid: Some(4242),
    }
}

#[test]
fn the_machine_says_when_it_booted_and_the_answer_is_usable() -> Result<(), Box<dyn Error>> {
    let booted = machine_booted_at()
        .ok_or("the machine did not say when it booted, so nothing below has a guard to use")?;

    // Sekundy uniksowe, nie zdanie po ludzku: ta wartość ląduje w bazie i jest PORÓWNYWANA
    // z odpowiedzią tej samej funkcji po restarcie. Napis zależny od lokalizacji systemu
    // przestałby się zgadzać sam ze sobą po zmianie języka.
    assert!(
        booted.chars().all(|c| c.is_ascii_digit()) && !booted.is_empty(),
        "the boot marker has to be plain unix seconds -- it gets compared as a string after a \
         restart, so anything locale-shaped stops matching itself. Got: {booted:?}"
    );

    // Ta sama maszyna, pytana dwa razy, ma odpowiedzieć tak samo. Bez tego strażnik porównywałby
    // dwie różne wartości przy każdym uruchomieniu i blokowałby sprzątanie na zawsze.
    assert_eq!(
        machine_booted_at().as_deref(),
        Some(booted.as_str()),
        "asking twice gave two answers, so the guard would never match and recovery would never \
         clean anything up"
    );
    Ok(())
}

#[test]
fn a_run_with_the_marker_is_judged_and_one_without_it_is_not() {
    let booted = machine_booted_at().unwrap_or_else(|| "1785488462".to_owned());
    let machine = Machine {
        boot_id: booted.clone(),
        own_pgid: 1,
    };

    // ── Z ZNACZNIKIEM: `decide` ma czym sądzić i nie odmawia z powodu jego braku ────────────
    let plan = decide(&[interrupted(Some(&booted))], &machine);
    let refusals: Vec<&str> = plan
        .unreadable
        .iter()
        .map(|row| row.reason.as_str())
        .collect();
    assert!(
        !refusals.contains(&reason::NO_BOOT_TIME),
        "the row carries the boot marker, so recovery must not refuse for lack of one. It said: \
         {refusals:?}"
    );
    assert!(
        plan.reap.contains(&4242),
        "a run left running on THIS boot has a process group worth proving dead; recovery named \
         none. Plan: {plan:?}"
    );

    // ── KONTROLA NEGATYWNA: bez znacznika strażnik MUSI wstrzymać strzał ────────────────────
    // Bez tej połowy test przechodziłby także na implementacji, która ignoruje znacznik i strzela
    // zawsze — czyli na tej jednej, przed którą całe to pole istnieje.
    let blind = decide(&[interrupted(None)], &machine);
    assert!(
        blind.reap.is_empty(),
        "a row with no boot marker must not be reaped: after a restart its pgid probably belongs \
         to something innocent (kern.maxproc = 16 000, so pids wrap in hours). Plan: {blind:?}"
    );
    let why: Vec<&str> = blind
        .unreadable
        .iter()
        .map(|row| row.reason.as_str())
        .collect();
    assert!(
        why.contains(&reason::NO_BOOT_TIME),
        "the refusal has to say WHICH guard stopped it, so the human knows this is a missing \
         marker and not a healthy run. It said: {why:?}"
    );
}
