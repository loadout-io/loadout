//! AC-2 dla T-35: bieg zabity razem z aplikacją jest po restarcie OZNACZONY, a nie „biegnie".
//!
//! `recovery::decide()` i `apply()` istniały od T-20, miały własne kryteria i **nikt ich nie
//! wołał**. Ekran pokazywał `running` dla biegu, którego nikt już nie prowadzi — czyli coś
//! gorszego niż pusta lista, bo wygląda na pracę w toku.
//!
//! **Słaba wersja tego kryterium:** sprawdzenie, że `decide()` zwróciło decyzję. Przechodziła
//! przed poprawką — funkcja działała i nikt jej nie wołał. Rozróżnia dopiero wywołanie przez
//! ŚCIEŻKĘ STARTOWĄ (`loadout_lib::recover_from_last_time`), czyli tę samą, którą wykonuje
//! `setup` okna, i sprawdzenie stanu W BAZIE — a nie w zwróconej strukturze.

use std::error::Error;

use loadout_lib::recover_from_last_time;
use loadout_lib::store::{NewRun, NewStep, Store};
use tempfile::TempDir;

/// Bieg zastany w `running`: aplikacja zginęła, zanim zdążyła cokolwiek domknąć.
fn interrupted_run(boot: Option<&str>) -> NewRun {
    NewRun {
        id: "01990000-0000-7000-8000-00000000r001".to_owned(),
        workflow_id: "wf".to_owned(),
        workflow_snapshot: r#"{"steps":[],"links":[]}"#.to_owned(),
        title: "Fix the CSV parser".to_owned(),
        status: "running".to_owned(),
        concurrency: 3,
        created_at: 1_755_300_000_000,
        started_at: Some(1_755_300_001_000),
        ended_at: None,
        error: None,
        boot_id: boot.map(str::to_owned),
    }
}

/// Krok bez `ended_at` — czyli taki, o którym baza wciąż myśli, że pracuje.
fn running_step() -> NewStep {
    NewStep {
        id: "01990000-0000-7000-8000-00000000s001".to_owned(),
        run_id: "01990000-0000-7000-8000-00000000r001".to_owned(),
        node_key: "s_one".to_owned(),
        name: "Wedged".to_owned(),
        agent: "01990000-0000-7000-8000-0000000000a1".to_owned(),
        depends_on: "[]".to_owned(),
        status: "running".to_owned(),
        attempt: 0,
        agent_session_id: Some("sess".to_owned()),
        // `pgid` sprzed restartu maszyny. To jest liczba, której NIE WOLNO użyć na oślep.
        pid: Some(4242),
        pgid: Some(4242),
        exit_code: None,
        started_at: Some(1_755_300_002_000),
        ended_at: None,
        cost_usd: None,
        summary: None,
        error: None,
    }
}

/// Co baza mówi o tym biegu i jego kroku po odzyskiwaniu.
fn state(store: &Store) -> Result<(String, String, Option<String>), Box<dyn Error>> {
    let conn = store.reader()?;
    let run: String = conn.query_row(
        "SELECT status FROM runs WHERE id = ?1",
        ["01990000-0000-7000-8000-00000000r001"],
        |row| row.get(0),
    )?;
    let (step, why): (String, Option<String>) = conn.query_row(
        "SELECT status, error FROM steps WHERE id = ?1",
        ["01990000-0000-7000-8000-00000000s001"],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok((run, step, why))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_run_killed_with_the_app_comes_back_marked_not_running() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let store = Store::open(&home.path().join("loadout.db"))?;

    // Znacznik SPRZED restartu maszyny: napis, który na pewno nie zrówna się z tym, co ta
    // maszyna mówi teraz. To jest ten wiersz, którego pgid wolno tknąć dopiero po dowodzie,
    // że należy jeszcze do nas.
    store
        .writer()
        .insert_run(interrupted_run(Some("1")))
        .await?;
    store.writer().insert_step(running_step()).await?;

    let (before_run, before_step, _) = state(&store)?;
    assert_eq!(
        (before_run.as_str(), before_step.as_str()),
        ("running", "running"),
        "the fixture has to start from the state recovery exists for; it started from something \
         else and nothing below would mean anything"
    );

    // ── ŚCIEŻKA STARTOWA, nie `decide()` wprost ────────────────────────────────────────────
    // To jest cała treść tego kryterium: `decide()` dawało się wołać od T-20 i przez cały ten
    // czas nie wołał go nikt.
    let (runs, steps, report) = recover_from_last_time(&store).await?;

    assert_eq!(
        (runs, steps),
        (1, 1),
        "the startup path has to mark exactly the one run and the one step that were left \
         running. It marked {runs} run(s) and {steps} step(s)"
    );

    let (after_run, after_step, why) = state(&store)?;
    assert_eq!(
        after_run, "interrupted",
        "a run nobody is driving any more must not stay 'running': on screen that reads as work \
         in progress, which is worse than an empty list. It is: {after_run}"
    );
    assert_eq!(
        after_step, "failed",
        "the step ended when the app died, so it did not succeed and it was not cancelled by \
         anyone. It is: {after_step}"
    );
    let why = why.ok_or("the step carries no reason, so the human cannot tell why it stopped")?;
    assert!(
        !why.trim().is_empty(),
        "an empty reason reads exactly like no reason at all"
    );

    // ── NIC NIE ZOSTAŁO ZABITE NA OŚLEP ────────────────────────────────────────────────────
    // Ta połowa jest ważniejsza od poprzedniej. `kern.maxproc` na macOS wynosi 16 000, więc
    // PID-y przewijają się w godzinach: zapisany pgid 4242 po restarcie maszyny z dużym
    // prawdopodobieństwem należy do czegoś zupełnie niewinnego.
    assert!(
        report.reaped.is_empty() && report.unproven.is_empty() && report.foreign.is_empty(),
        "recovery touched a process group belonging to a run that started on ANOTHER boot. \
         Nothing may be signalled without the boot marker matching. Report: {report:?}"
    );

    Ok(())
}
