//! P-03a: brak zgody na sterowanie oknem, awaria automatyzacji i defekt aplikacji to trzy
//! różne wyniki.
//!
//! Incydenty I-06/I-07 (bieg z 2026-09-06): wymagania dopuszczały mocked-IPC **albo** samo
//! uruchomienie aplikacji, a krok QA nie miał ani jednego narzędzia do okna — jego sesja
//! dostała `Bash/Edit/Glob/Grep/Read/WebFetch/WebSearch/Write` i zero serwerów MCP.
//! Mimo to wydał werdykt o produkcie.
//!
//! Dubler zastępuje wyłącznie odpowiedź systemu: podstawiamy własny „interpreter", który
//! kończy się dokładnie tak, jak `osascript` bez zgody. Uprawnień maszyny, na której biegnie
//! ten test, nie zmieniamy i nie potrzebujemy.

#![allow(clippy::doc_markdown)]
#![allow(clippy::expect_used, clippy::too_many_lines, clippy::similar_names)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;
use std::time::Duration;

use loadout_lib::engine::native_ui::{NativeUiAccess, probe_with};
use loadout_lib::workflow::criteria::{
    Criterion, Judgement, Method, Outcome, judge, without_a_native_route,
};

/// Odmowa systemu rozpoznana po NUMERZE, nie po angielskim zdaniu.
#[tokio::test]
async fn a_refused_permission_is_recognised_in_any_language() -> Result<(), Box<dyn Error>> {
    // Ta sama skarga, którą `osascript` wypisuje na tej maszynie — po polsku, z numerem.
    let access = probe_with(pretending(
        "echo 'execution error: System Events — błąd: Nie masz zgody. (-1743)' >&2; exit 1",
    )?)
    .await;
    let NativeUiAccess::NotPermitted { said } = access else {
        return Err(format!("a refused permission was read as something else: {access:?}").into());
    };
    assert!(
        said.contains("permission on this machine, not a result about the application"),
        "the sentence lets a missing permission read as a verdict about the product: {said:?}"
    );
    assert!(
        said.contains("Privacy & Security"),
        "the sentence does not say where a person grants it: {said:?}"
    );
    Ok(())
}

/// Awaria automatyzacji to nie brak zgody. Człowiek robi z nimi dwie różne rzeczy.
#[tokio::test]
async fn a_broken_automation_is_not_a_missing_permission() -> Result<(), Box<dyn Error>> {
    let access = probe_with(pretending("echo 'something else went wrong' >&2; exit 2")?).await;
    assert!(
        matches!(access, NativeUiAccess::Unknown { .. }),
        "an unexplained failure was reported as a refused permission: {access:?}"
    );
    assert!(!access.can_judge_the_product());
    Ok(())
}

/// Brak interpretera to trzeci, jeszcze inny wynik.
#[tokio::test]
async fn a_computer_without_the_interpreter_says_so() -> Result<(), Box<dyn Error>> {
    let missing = tempfile::tempdir()?.path().join("not-here");
    let access = probe_with(missing).await;
    assert!(
        matches!(access, NativeUiAccess::Missing { .. }),
        "a computer that cannot drive windows at all was reported as something else: {access:?}"
    );
    Ok(())
}

/// Sonda, która odpowiedziała, nie obiecuje niczego o produkcie — mówi tylko, że jest czym
/// wykonać scenariusz.
#[tokio::test]
async fn a_working_route_only_says_there_is_something_to_drive() -> Result<(), Box<dyn Error>> {
    let access = probe_with(pretending("echo 42")?).await;
    assert_eq!(access, NativeUiAccess::Ready);
    assert!(access.can_judge_the_product());
    assert!(access.said().is_empty());
    Ok(())
}

/// Sedno P-03a: bez drogi do okna kryterium wymagające pełnej aplikacji nie może być
/// potwierdzone, cokolwiek weryfikator o nim napisał.
#[test]
fn a_native_requirement_cannot_be_confirmed_without_a_route_to_the_window() {
    let approved = vec![
        Criterion {
            id: "c1".to_owned(),
            behaviour: "The audio is kept".to_owned(),
            required: true,
            method: Method::AutomatedTest,
        },
        Criterion {
            id: "c2".to_owned(),
            behaviour: "The next recording starts after Later".to_owned(),
            required: true,
            method: Method::FullRuntime,
        },
    ];
    let complete = "criterion c1: passed\ncriterion c2: passed via full runtime — I clicked it\n";
    let mut judged: Judgement = judge(&approved, complete);
    assert_eq!(
        judged.outcome,
        Outcome::Passed,
        "the fixture does not start from a report that would otherwise pass"
    );

    without_a_native_route(&approved, &mut judged);
    assert_eq!(
        judged.outcome,
        Outcome::NotJudged,
        "a scenario nobody could run was accepted because the verifier said it ran"
    );
    assert_eq!(judged.not_tested, vec!["c2".to_owned()]);
    assert!(
        judged.failed.is_empty(),
        "a missing permission on this computer was recorded as a defect in the application"
    );
    assert!(
        judged.said().contains("could not measure 1"),
        "the sentence does not separate what was not measured: {:?}",
        judged.said()
    );
}

/// Kryterium, które padło naprawdę, zostaje porażką także wtedy, gdy drogi do okna nie było:
/// zaobserwowana wada nie znika, bo czegoś innego nie dało się zmierzyć.
#[test]
fn an_observed_failure_survives_the_missing_route() {
    let approved = vec![Criterion {
        id: "c2".to_owned(),
        behaviour: "The next recording starts after Later".to_owned(),
        required: true,
        method: Method::FullRuntime,
    }];
    let mut judged = judge(&approved, "criterion c2: failed — nothing started\n");
    without_a_native_route(&approved, &mut judged);
    assert_eq!(judged.outcome, Outcome::DidNotPass);
    assert_eq!(judged.failed, vec!["c2".to_owned()]);
    assert!(judged.not_tested.is_empty());
}

/// Żywa sonda TEJ maszyny. `--ignored`, bo odpowiedź zależy od uprawnień komputera,
/// a nie od kodu: na maszynie bez zgody musiałaby być czerwona, choć nic nie jest zepsute.
///
/// Uruchomienie: `cargo test --test it native_scenarios -- --ignored --nocapture`
#[tokio::test]
#[ignore = "answers about this computer's permissions, not about this code"]
async fn what_this_computer_can_actually_do() {
    let access = loadout_lib::engine::native_ui::can_drive_native_ui().await;
    println!("native UI route on this computer: {access:?}");
    assert!(
        !matches!(access, NativeUiAccess::Missing { .. }),
        "this computer has no way to drive a native window at all"
    );
}

/// Zapisuje wykonywalny plik, który udaje odpowiedź systemu.
fn pretending(body: &str) -> Result<PathBuf, Box<dyn Error>> {
    // Katalog przeżywa test, bo `PathBuf` musi wskazywać istniejący plik w chwili sondy.
    let home = tempfile::Builder::new().prefix("native-ui").tempdir()?;
    let at = home.path().join("osascript");
    fs::write(&at, format!("#!/bin/sh\n{body}\n"))?;
    fs::set_permissions(&at, fs::Permissions::from_mode(0o755))?;
    // Katalog zostaje na dysku do końca procesu testowego; sonda odpala go raz.
    std::mem::forget(home);
    Ok(at)
}

/// ŻYWY DOWÓD DROGI DO OKNA: prawdziwe okno, policzone i sterowane po `unix id`.
///
/// `what_this_computer_can_actually_do` wyżej mówi tylko, że `osascript` odpowiada. To jest
/// o jeden krok dalej i o to, co P-03 naprawdę obiecuje: że Loadout **policzy okna procesu,
/// który sam uruchomił**, i że działanie zaadresowane jego `unix id` naprawdę w nie trafia.
/// Dublerem tego się nie dowiedzie — dubler oddaje liczbę, którą mu wpiszemy.
///
/// Instancja jest WŁASNA i sprzątana: `open -na` startuje NOWY egzemplarz, scenariusz dotyka
/// wyłącznie jego pustego okna, a `quit` na tym samym `unix id` go zamyka. Okno człowieka nie
/// jest ani czytane, ani zamykane — o to właśnie chodzi w adresowaniu po `unix id` zamiast po
/// nazwie aplikacji.
///
/// `--ignored`, bo odpowiada o uprawnieniach TEJ maszyny i otwiera na niej okno.
///
/// ```text
/// cargo test --manifest-path src-tauri/Cargo.toml --test it \
///   native_scenarios_need_a_real_route_to_the_window::a_real_window -- --ignored --nocapture
/// ```
#[tokio::test]
#[ignore = "opens a real window on this computer and drives it"]
async fn a_real_window_is_counted_and_driven_by_the_identity_it_was_given() {
    let interpreter = loadout_lib::engine::native_ui::interpreter();
    let before = own_instances();

    std::process::Command::new("/usr/bin/open")
        .args(["-na", "TextEdit"])
        .status()
        .expect("a fresh instance could not be started");

    let mut mine = None;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        if let Some(pid) = own_instances().into_iter().find(|pid| !before.contains(pid)) {
            mine = Some(pid);
            break;
        }
    }
    let pid = mine.expect("the instance this test started never appeared");

    // 1. OKNO JEST POLICZONE — i to jest ta połowa prawdy, której otwarty port nie daje.
    let mut windows = 0;
    for _ in 0..40 {
        windows = loadout_lib::engine::native_ui::windows_of(&interpreter, pid)
            .await
            .unwrap_or(0);
        if windows > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    println!("windows of the instance this test started (pid {pid}): {windows}");

    // 2. DZIAŁANIE TRAFIA W TĘ INSTANCJĘ — nazwa okna wraca z procesu wskazanego `unix id`.
    let named = std::process::Command::new(&interpreter)
        .arg("-e")
        .arg(format!(
            "tell application \"System Events\" to tell (first process whose unix id is {pid}) \
             to get name of window 1"
        ))
        .output()
        .expect("the window could not be asked for its name");
    let title = String::from_utf8_lossy(&named.stdout).trim().to_owned();
    println!("window 1 of pid {pid} is named {title:?}");

    /* 3. SPRZĄTANIE PO SOBIE, tym samym adresem, i robione PRZED asercjami, żeby czerwień nie
     *    zostawiała okna na ekranie człowieka.
     *
     * Samo `quit` nie wystarcza i to jest zmierzony fakt, nie ostrożność: świeży egzemplarz
     * TextEdit staje z otwartym oknem dialogowym („Otwórz"), a aplikacja z modalnym oknem
     * odkłada zamknięcie. Czekamy więc na SKUTEK, a nie na wysłanie polecenia — i dopiero
     * gdy skutku nie ma, sięgamy po sygnał do TEGO pid. */
    let _ = std::process::Command::new(&interpreter)
        .arg("-e")
        .arg(format!(
            "tell application \"System Events\" to tell (first process whose unix id is {pid}) \
             to quit"
        ))
        .output();
    for _ in 0..20 {
        if !own_instances().contains(&pid) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    if own_instances().contains(&pid) {
        let _ = std::process::Command::new("/bin/kill")
            .arg(pid.to_string())
            .status();
        for _ in 0..20 {
            if !own_instances().contains(&pid) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    assert!(
        windows > 0,
        "Loadout started an application, saw it alive, and could not confirm a single window — \
         which is exactly the state in which a scenario meant to happen in a window gets a \
         verdict anyway"
    );
    assert!(
        !title.is_empty(),
        "an action addressed by unix id reached no window, so the identity Loadout hands the \
         QA step is not usable for anything"
    );
    assert!(
        !own_instances().contains(&pid),
        "the instance this test started is still running: {pid}"
    );
}

/// PID-y egzemplarzy TextEdit widziane teraz.
fn own_instances() -> Vec<u32> {
    let listed = std::process::Command::new("/bin/ps")
        .args(["-axo", "pid=,command="])
        .output()
        .expect("this computer cannot list its own processes");
    String::from_utf8_lossy(&listed.stdout)
        .lines()
        .filter(|line| line.contains("TextEdit.app/Contents/MacOS/TextEdit"))
        .filter_map(|line| line.split_whitespace().next()?.parse().ok())
        .collect()
}
