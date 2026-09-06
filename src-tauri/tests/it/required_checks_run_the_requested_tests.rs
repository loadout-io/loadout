//! V-01: licznik przejść nie mówi, KTÓRE testy przeszły.
//!
//! Incydenty I-03/I-04 (bieg z 2026-09-06): Backend miał dopisać 13 testów, `cd src-tauri`
//! nie powiodło się, a potem raportował testy jako istniejące. Jego własne filtry uruchamiały
//! zero albo dwa niezwiązane testy — i wszystko to przechodziło, bo jedynym pytaniem było
//! „czy licznik jest dodatni".
//!
//! Prawdziwy `CommandDriver`, prawdziwy proces, prawdziwe potoki. Dubler zastępuje wyłącznie
//! suitę testową — tekstem, który drukuje ten sam kształt, co libtest i TAP.

use std::error::Error;
use std::time::Duration;

use loadout_lib::engine::drivers::command::{CheckHow, CheckSpec, CommandDriver};
use tokio_util::sync::CancellationToken;

/// Trzynaście wymaganych testów, dwa niezwiązane uruchomione, exit 0 i dodatni licznik.
/// Dokładnie I-04.
#[tokio::test]
async fn two_unrelated_passes_do_not_confirm_thirteen_required_tests() -> Result<(), Box<dyn Error>>
{
    let required: Vec<String> = (1..=13).map(|n| format!("wanted::case_{n}")).collect();
    let report = check(
        "printf 'test other::old_one ... ok\\ntest other::old_two ... ok\\n\
         test result: ok. 2 passed; 0 failed\\n'",
        r"(\d+) passed",
        &required,
    )
    .await?;
    assert!(
        !report.confirmed,
        "a check confirmed thirteen required tests after running two unrelated ones"
    );
    let said = report.said;
    assert!(
        said.contains("13 tests") && said.contains("did not run"),
        "the check did not say how many of its required tests never ran: {said:?}"
    );
    assert!(
        said.contains("wanted::case_1"),
        "the check did not name a single missing test: {said:?}"
    );
    Ok(())
}

/// Testy dodatkowe nie zastępują wymaganych. To jest ta sama granica, tylko od drugiej strony:
/// suita może urosnąć o pięć nowych przejść i nadal nie potwierdzać tego, co miała potwierdzić.
#[tokio::test]
async fn extra_passing_tests_never_stand_in_for_a_required_one() -> Result<(), Box<dyn Error>> {
    let report = check(
        "printf 'test wanted::first ... ok\\ntest spare::a ... ok\\ntest spare::b ... ok\\n\
         test spare::c ... ok\\ntest result: ok. 4 passed; 0 failed\\n'",
        r"(\d+) passed",
        &["wanted::first".to_owned(), "wanted::second".to_owned()],
    )
    .await?;
    assert!(
        !report.confirmed,
        "four passing tests stood in for the one required test that never ran"
    );
    assert!(
        report.said.contains("wanted::second") && !report.said.contains("wanted::first"),
        "the check blamed the wrong test: {:?}",
        report.said
    );
    Ok(())
}

/// Wymagany test, który się wykonał i PADŁ, to inny fakt niż wymagany test, którego nie było.
/// Człowiek naprawia je inaczej.
#[tokio::test]
async fn a_required_test_that_ran_and_failed_is_not_a_missing_one() -> Result<(), Box<dyn Error>> {
    let report = check(
        "printf 'test wanted::first ... ok\\ntest wanted::second ... FAILED\\n\
         test result: FAILED. 1 passed; 1 failed\\n'; true",
        r"(\d+) passed",
        &["wanted::first".to_owned(), "wanted::second".to_owned()],
    )
    .await?;
    assert!(!report.confirmed, "a failing required test was confirmed");
    assert!(
        report.said.contains("did not pass") && report.said.contains("wanted::second"),
        "a required test that ran and failed was reported as never run: {:?}",
        report.said
    );
    Ok(())
}

/// Komplet wymaganych testów, wykonanych i zdanych, potwierdza sprawdzenie.
#[tokio::test]
async fn the_requested_suite_with_its_real_ids_confirms_the_check() -> Result<(), Box<dyn Error>> {
    let report = check(
        "printf 'test wanted::first ... ok\\ntest wanted::second ... ok\\n\
         test result: ok. 2 passed; 0 failed\\n'",
        r"(\d+) passed",
        &["wanted::first".to_owned(), "wanted::second".to_owned()],
    )
    .await?;
    assert!(
        report.confirmed,
        "a complete, passing required suite was not confirmed: {:?}",
        report.said
    );
    Ok(())
}

/// Ten sam kontrakt dla runnera frontendowego. TAP jest tu jedynym kształtem, w którym
/// tożsamość testu jest jednoznaczna, a nie zależna od reportera.
#[tokio::test]
async fn a_tap_reporter_maps_onto_the_same_contract() -> Result<(), Box<dyn Error>> {
    let report = check(
        "printf 'TAP version 13\\n1..2\\nok 1 - src/a.test.ts > keeps the draft\\n\
         not ok 2 - src/a.test.ts > sends once\\n# pass 1\\n'",
        r"pass (\d+)",
        &[
            "src/a.test.ts > keeps the draft".to_owned(),
            "src/a.test.ts > sends once".to_owned(),
        ],
    )
    .await?;
    assert!(!report.confirmed, "a failing TAP case was confirmed");
    assert!(
        report.said.contains("sends once") && report.said.contains("did not pass"),
        "the frontend runner's failing case did not reach the same contract: {:?}",
        report.said
    );
    Ok(())
}

/// Sprawdzenie bez listy wymaganych testów zachowuje dotychczasowy kontrakt.
#[tokio::test]
async fn a_check_without_required_tests_keeps_its_old_contract() -> Result<(), Box<dyn Error>> {
    let report = check(
        "printf 'test result: ok. 2 passed; 0 failed\\n'",
        r"(\d+) passed",
        &[],
    )
    .await?;
    assert!(
        report.confirmed,
        "an existing check without a required list stopped passing: {:?}",
        report.said
    );
    Ok(())
}

/// I-03: krok, którego katalog roboczy nie istnieje, nie ma prawa wyglądać na wykonany.
/// Nie ma też prawa niczego uruchomić — z niewłaściwego katalogu jedna komenda robi
/// zupełnie inną robotę.
#[tokio::test]
async fn a_missing_working_directory_stops_before_anything_runs() -> Result<(), Box<dyn Error>> {
    let workspace = tempfile::tempdir()?;
    let sentinel = workspace.path().join("it-ran");
    let missing = workspace.path().join("src-tauri");
    let spec = CheckSpec {
        command: format!("touch {}", sentinel.display()),
        proof: r"(\d+) passed".to_owned(),
        cwd: missing.clone(),
        required_tests: Vec::new(),
    };
    let driver = CommandDriver::new();
    let started = driver.start(&spec);
    let said = match started {
        Ok(_) => return Err("the driver spawned a check into a directory that does not exist".into()),
        Err(error) => error.to_string(),
    };
    // `No such file or directory (os error 2)` jest prawdą i nie mówi CZEGO nie ma. Człowiek
    // czyta ją przy kroku, którego komenda zaczyna się od `cd src-tauri`, i nie wie, czy nie ma
    // katalogu, komendy, czy pliku, którego ta komenda szukała.
    assert!(
        said.contains(&missing.display().to_string()) && said.contains("does not exist"),
        "the refusal does not name the folder that is missing: {said:?}"
    );
    assert!(
        !sentinel.exists(),
        "the check ran even though its working directory was gone"
    );
    Ok(())
}

/// Prawdziwa suita wypisuje więcej niż zachowywany ogon. Wymagany test wypisany na POCZĄTKU
/// nie może po tym wyglądać na niewykonany.
///
/// Krok „sprawdź" trzyma ostatnie 64 KiB wyjścia. Sądzenie po tym ogonie mówiłoby „nie wykonał
/// się" o każdym teście, po którym runner wypisał dość tekstu — czyli o każdym teście z 1600.
#[tokio::test]
async fn a_pass_printed_before_the_kept_tail_still_counts() -> Result<(), Box<dyn Error>> {
    let report = check(
        "printf 'test wanted::first ... ok\\n'; \
         for i in $(seq 1 3000); do printf 'noise line %s aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\\n' \"$i\"; done; \
         printf 'test result: ok. 1 passed; 0 failed\\n'",
        r"(\d+) passed",
        &["wanted::first".to_owned()],
    )
    .await?;
    // Bez tej asercji kryterium nie sądzi niczego: gdyby wyjście zmieściło się w ogonie,
    // przechodziłoby także po zebranym tekście, czyli po kodzie sprzed tej poprawki.
    assert!(
        report.kept.starts_with("[Loadout omitted earlier output from this check.]"),
        "the fixture did not print past the kept tail, so this criterion proves nothing"
    );
    assert!(
        !report.kept.contains("test wanted::first"),
        "the required test is still inside the kept output, so this criterion proves nothing"
    );
    assert!(
        report.confirmed,
        "a required test that really ran was lost because the kept output is only a tail: {:?}",
        report.said
    );
    Ok(())
}

/// Cudze zdanie na `stderr` nie jest wynikiem testu. Runnery wypisują werdykty na `stdout`;
/// wszystko, co w tej komendzie napisał ktokolwiek inny, jedzie drugim potokiem.
#[tokio::test]
async fn a_line_on_stderr_cannot_pass_for_a_test_result() -> Result<(), Box<dyn Error>> {
    let report = check(
        "printf 'test wanted::first ... ok\\n' >&2; printf 'test result: ok. 1 passed\\n'",
        r"(\d+) passed",
        &["wanted::first".to_owned()],
    )
    .await?;
    assert!(
        !report.confirmed,
        "a sentence written to the complaint stream was accepted as a passing test"
    );
    assert!(
        report.said.contains("did not run") && report.said.contains("wanted::first"),
        "the check did not report the required test as never run: {:?}",
        report.said
    );
    Ok(())
}

/// Zdanie musi dojść tam, gdzie czyta je CZŁOWIEK (niezmiennik 29). Prawdziwy bieg,
/// prawdziwy krok „sprawdź", zapisany `run.json`.
///
/// „Sprawdzenie nie przeszło" nad suitą, która wykonała dwa niezwiązane testy zamiast dwóch
/// wymaganych, wysyła człowieka szukać wady w produkcie — a wada jest w komendzie.
#[tokio::test]
async fn the_run_says_which_required_tests_never_ran() -> Result<(), Box<dyn Error>> {
    use loadout_lib::commands::run::run_workflow_inner;
    use loadout_lib::commands::{Drivers, RunRequest};
    use loadout_lib::ipc::{AppState, line_channel};
    use loadout_lib::store::Store;
    use serde_json::{Value, json};

    let root = tempfile::tempdir()?;
    let project = root.path().canonicalize()?.join("project");
    let home = root.path().canonicalize()?.join("home");
    std::fs::create_dir_all(project.join(".loadout"))?;
    std::fs::create_dir_all(home.join("workflows"))?;
    let workflow = home.join("workflows/verify.json");
    std::fs::write(
        &workflow,
        json!({"format":1,"id":"wf-verify","name":"Verify","steps":[{
            "kind":"check","id":"verify","name":"Verify",
            "command":"printf 'test other::old_one ... ok\\ntest result: ok. 1 passed\\n'",
            "proof":"(\\d+) passed",
            "requiredTests":["wanted::first","wanted::second"],
            "folder":{"use":"project"},"at":{"x":0,"y":0}}],"links":[]})
        .to_string(),
    )?;
    let drivers: Drivers = std::sync::Arc::new(|_| {
        unreachable_driver()
    });
    let state = AppState::new(
        home,
        project.clone(),
        Store::open(&project.join(".loadout/loadout.db"))?,
        drivers,
    );
    let deps = state.begin_run(&project)?;
    let (sink, _lines) = line_channel(256);
    let report = tokio::time::timeout(
        Duration::from_secs(30),
        run_workflow_inner(
            &deps,
            &RunRequest {
                workflow,
                how_many_at_once: 1,
                task: None,
                part: None,
                handoffs_from: None,
            },
            sink,
        ),
    )
    .await??;
    let run: Value = serde_json::from_slice(&std::fs::read(report.dir.join("run.json"))?)?;
    let said = run["steps"][0]["error"].as_str().unwrap_or_default();
    assert!(
        said.contains("did not run 2 of the 2 tests it must confirm")
            && said.contains("wanted::first"),
        "the saved run does not say which required tests never ran: {said:?}"
    );
    assert_eq!(
        run["steps"][0]["status"].as_str(),
        Some("failed"),
        "a check that confirmed none of its required tests was recorded as anything but failed"
    );
    Ok(())
}

/// Ten workflow nie ma kroku agenta, więc fabryka sterowników nie ma prawa zostać zawołana.
fn unreachable_driver() -> std::sync::Arc<dyn loadout_lib::engine::drivers::AgentDriver> {
    panic!("a check-only workflow asked for an agent driver")
}

// ---------------------------------------------------------------------------------------------

struct Judged {
    confirmed: bool,
    said: String,
    /// To, co z wyjścia zostało zachowane — czyli ogon, nie cały strumień.
    kept: String,
}

/// Prawdziwy sterownik, prawdziwy proces, prawdziwa grupa. Oddaje werdykt i zdanie.
async fn check(command: &str, proof: &str, required: &[String]) -> Result<Judged, Box<dyn Error>> {
    let workspace = tempfile::tempdir()?;
    let spec = CheckSpec {
        command: command.to_owned(),
        proof: proof.to_owned(),
        cwd: workspace.path().to_path_buf(),
        required_tests: required.to_vec(),
    };
    let driver = CommandDriver::new();
    let cancel = CancellationToken::new();
    let running = driver.run(&spec, &cancel);
    tokio::pin!(running);
    let result = tokio::select! {
        result = &mut running => result?,
        () = tokio::time::sleep(Duration::from_secs(10)) => {
            cancel.cancel();
            running.await?
        }
    };
    let CheckHow::Ran(report) = result.how else {
        return Err("the command fixture never completed".into());
    };
    Ok(Judged {
        kept: report.output.clone(),
        confirmed: report.passed,
        said: report
            .required
            .as_ref()
            .map(loadout_lib::engine::drivers::command::RequiredTests::said)
            .unwrap_or_default(),
    })
}
