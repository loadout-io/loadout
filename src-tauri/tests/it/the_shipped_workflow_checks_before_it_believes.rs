//! V-03: kolejność „implementacja → sprawdzenia → QA na działającej aplikacji" jest DANYMI
//! grafu, nie gałęzią w schedulerze (niezmiennik 27, decyzja D7).
//!
//! Kryterium czyta plik, który naprawdę leży w repo i który człowiek zaimportuje, i sądzi go
//! produkcyjnym wczytywaniem oraz produkcyjnym rozwinięciem pętli. Asercja na obecność napisu
//! przechodziłaby na komentarzu (niezmiennik 20).

#![allow(clippy::panic)]
#![allow(clippy::expect_used, clippy::too_many_lines, clippy::similar_names)]

use std::error::Error;
use std::path::{Path, PathBuf};

use loadout_lib::library::agents::Agent;
use loadout_lib::workflow::criteria::Method;
use loadout_lib::workflow::{Step, WorkflowFile};

/// Gdzie leży to, co człowiek importuje.
fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

fn shipped() -> Result<WorkflowFile, Box<dyn Error>> {
    let at = repo().join(".loadout/workflows/verified-change.json");
    Ok(serde_json::from_slice(&std::fs::read(at)?)?)
}

/// Dwie poprawki po pierwszym podejściu — czyli TRZY rundy.
///
/// `max_turns` liczy rundy, a nie poprawki: ostatnia runda to `turns - 1`
/// (`workflow::unroll::Loop`). Wpisana tam dwójka dałaby JEDNĄ poprawkę, więc ta liczba jest
/// dokładnie tą pułapką, przed którą ostrzega plan — i dlatego sądzi ją rozwinięcie grafu,
/// a nie odczyt pola.
#[test]
fn the_repair_loop_gives_exactly_two_tries_after_the_first() -> Result<(), Box<dyn Error>> {
    let file = shipped()?;
    let unrolled = loadout_lib::workflow::unroll::unroll(&file);
    let the_loop = unrolled
        .loops
        .first()
        .ok_or("the shipped workflow has no repair loop at all")?;
    assert_eq!(
        the_loop.turns,
        3,
        "the repair loop gives {} tries in total, so the work gets {} repairs, not two",
        the_loop.turns,
        the_loop.turns.saturating_sub(1)
    );
    Ok(())
}

/// Sprawdzenie wymienia testy PO NAZWIE i zatrzymuje bieg, kiedy ich nie ma.
#[test]
fn the_checks_name_the_tests_they_must_confirm() -> Result<(), Box<dyn Error>> {
    let file = shipped()?;
    let check = file
        .steps
        .iter()
        .find_map(|step| match step {
            Step::Check(check) => Some(check),
            _ => None,
        })
        .ok_or("the shipped workflow has no step that runs the tests")?;
    assert!(
        !check.required_tests.is_empty(),
        "the check counts tests without naming any, so two unrelated passes read as a full suite"
    );
    assert_eq!(
        check.when_it_fails,
        loadout_lib::workflow::WhenItFails::Stop,
        "a check that carries on anyway lets unverified work reach the step that judges it"
    );
    Ok(())
}

/// Wymagania QA są zachowaniami widocznymi dla człowieka i potwierdza się je DZIAŁAJĄCĄ
/// aplikacją. Mock nie zaspokaja tych kryteriów — pilnuje tego `criteria::Method`.
#[test]
fn the_running_app_is_the_only_way_to_confirm_the_qa_requirements() -> Result<(), Box<dyn Error>> {
    let file = shipped()?;
    let qa = agent_named(&file, "Check the running app")?;
    assert!(
        !qa.criteria.is_empty(),
        "the step that judges the work has no approved requirements, so one word at the end \
         decides again"
    );
    for one in &qa.criteria {
        assert!(
            one.required,
            "requirement {} is optional, and an optional requirement is a suggestion",
            one.id
        );
        assert_eq!(
            one.method,
            Method::FullRuntime,
            "requirement {} can be answered without ever starting the application",
            one.id
        );
        assert!(
            one.behaviour.split_whitespace().count() >= 4,
            "requirement {} does not describe anything a person could watch happen: {:?}",
            one.id,
            one.behaviour
        );
    }
    Ok(())
}

/// QA ma ODRĘBNĄ rolę. Krok, który sądzi swoją własną pracę, nie ma już niezależnej odpowiedzi.
#[test]
fn the_step_that_judges_is_not_one_of_the_steps_that_built() -> Result<(), Box<dyn Error>> {
    let file = shipped()?;
    let qa = agent_named(&file, "Check the running app")?;
    for name in ["Backend", "Frontend", "Put the work together"] {
        let builder = agent_named(&file, name)?;
        assert_ne!(
            builder.agent, qa.agent,
            "\"{name}\" and the step that judges the work are the same role"
        );
    }
    Ok(())
}

/// Rola QA istnieje w repo i mówi o SPRAWDZANIU, nie o pisaniu.
#[test]
fn the_shipped_qa_role_is_not_told_to_implement() -> Result<(), Box<dyn Error>> {
    let at = repo().join(".loadout/agents/verifies-the-running-app.md");
    let agent: Agent = loadout_lib::library::agents::read_agent_file(&at)
        .map_err(|why| format!("the shipped QA role does not load: {why}"))?;
    assert_eq!(
        agent.file_access,
        loadout_lib::library::agents::FileAccess::LookOnly,
        "the role that judges the work may change it, so its verdict is about its own repair"
    );
    let said = agent.instructions.to_lowercase();
    assert!(
        said.contains("not tested"),
        "the role was never told that something it could not measure has its own answer"
    );
    assert!(
        !said.contains("implement"),
        "the role that checks the work inherited an instruction to build it"
    );
    /* P-03b: KAŻDA AKCJA ADRESOWANA TOŻSAMOŚCIĄ INSTANCJI TESTOWEJ.
     *
     * Tego jednego Loadout nie umie wymusić kodem i to jest udokumentowany wybór, nie
     * przeoczenie (niezmiennik 28): agent steruje oknem przez `Bash` i `osascript`, więc
     * przechwycenie każdej akcji znaczyłoby napisanie własnego systemu Computer Use — czego
     * plan zabrania wprost. Egzekwowalne jest to, co obok: metoda `full-runtime`, potwierdzone
     * okno przed startem QA i „nie zmierzono" bez drogi do okna.
     *
     * Zostaje więc zdanie w roli — i ono ma tam BYĆ, bo kliknięcie wysłane po NAZWIE aplikacji
     * trafia w tę, na którą akurat wskazuje menedżer okien, czyli potencjalnie w prawdziwą
     * aplikację człowieka z jego prawdziwymi nagraniami. */
    assert!(
        said.contains("unix id") && said.contains("never the"),
        "the role was never told to address the exact process it was given"
    );
    assert!(
        said.contains("do not record audio") || said.contains("not record"),
        "the role was never told that a real conversation is not test material"
    );
    Ok(())
}

fn agent_named<'a>(
    file: &'a WorkflowFile,
    name: &str,
) -> Result<&'a loadout_lib::workflow::AgentStep, Box<dyn Error>> {
    file.steps
        .iter()
        .find_map(|step| match step {
            Step::Agent(agent) if agent.name == name => Some(agent),
            _ => None,
        })
        .ok_or_else(|| format!("the shipped workflow has no step named {name}").into())
}
