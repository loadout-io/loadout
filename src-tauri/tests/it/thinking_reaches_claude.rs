//! AC-2 dla T-91: szczebel „ile agent ma myśleć" dojeżdża do argv `claude` jako `--effort`.
//!
//! # Po co to istnieje
//!
//! `Agent.thinking` jest polem formularza od T-11: człowiek je ustawia, panel kroku pozwala je
//! nadpisać, plik na dysku je zapisuje — a doc przy tym polu do 2026-08-23 twierdził, że jest
//! ono „tłumaczone niżej na `--effort` i `model_reasoning_effort`". Gerp po całym drzewie mówił
//! co innego: te dwa napisy stały wyłącznie w `import/adapters.rs`, przy **czytaniu** cudzej
//! konfiguracji. Żaden sterownik ani budowniczy argv nie czytał tego pola. Planer właściciela,
//! zapisany na szczeblu najwyższym, biegał na domyślnym wysiłku od pierwszego dnia — i nie da
//! się tego zobaczyć, klikając, bo „model myślał krócej" jest z zewnątrz nieodróżnialne od
//! „model uznał, że nie warto" (niezmiennik 16 schowany o warstwę głębiej).
//!
//! **Ten plik nie czyta `claude.rs` z dysku** (niezmiennik 20). Wyrocznią jest **zbudowana
//! komenda**, z tego samego powodu, co w `claude_argv_policy.rs`: selftest w repo źródłowym
//! asertował obecność flagi w skrypcie, przechodził na komentarzu, a żywa flaga brzmiała
//! inaczej [raport 06 §2].
//!
//! # Słaba wersja tego kryterium
//!
//! `assert!(argv.contains(&"--effort".into()))`. Przechodzi dla adaptera, który wypisuje jeden
//! poziom wszystkim czterem szczeblom — czyli dla kontrolki, która nie kontroluje niczego, i to
//! w wersji najdroższej: agent zamówiony na szczeblu najwyższym płaci za najniższy. Dlatego
//! każda asercja niżej czyta **wartość stojącą zaraz za flagą**, a kontrola liczy, ile RÓŻNYCH
//! wartości dały cztery szczeble.
//!
//! # Czego ten plik NIE sądzi, i to jest zgłoszenie, nie przeoczenie
//!
//! Proza AC-2 mówi też, że przelotka z T-90 podająca `--effort` ma być odmową nazywającą flagę,
//! „tak jak reszta listy zarezerwowanych". Lista zarezerwowanych jest jedna i mieszka
//! w `src-tauri/src/workflow/check.rs` (`RESERVED_CLAUDE`) — pliku, którego blok OWNS tego
//! zadania **nie zawiera**. Dopisanie tam ósmej pozycji jest całą robotą i jest robotą na jedną
//! linię, ale w cudzym pliku, a druga kopia listy po tej stronie byłaby dokładnie tym rozjazdem,
//! przed którym stoi komentarz przy tamtej stałej (niezmiennik 23). Zgłoszone człowiekowi.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w `claude_argv_policy` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]
// Wynik `Result` w teście, który nigdy nie oddaje `Err`: clippy nazywa to zbędnym opakowaniem,
// a `--all-targets` w pełnej bramce podnosi to do błędu (`quick` katalogu tests/ nie widzi).
#![allow(clippy::unnecessary_wraps)]

use std::collections::BTreeSet;
use std::error::Error;
use std::path::PathBuf;

use loadout_lib::commands::chat::Lead;
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::{AgentDriver, DriverConfiguration, Policy, RunSpec};
use loadout_lib::library::agents::{Agent, Overrides, Thinking, effort_level, resolve};
use uuid::Uuid;

/// Flaga, którą Claude Code przyjmuje poziom wysiłku. Zmierzone 2026-08-23 na 2.1.241:
/// `--effort <level>` z wartościami `low, medium, high, xhigh, max`.
///
/// **Wypisana tutaj**, nie zaimportowana ze sterownika: test czytający tę samą stałą, co kod,
/// zawsze się z nim zgadza i nie mierzy niczego (niezmiennik 20).
const EFFORT: &str = "--effort";

/// Szczebel, o który pyta wprost proza kryterium, i poziom, którym ma się odezwać u vendora.
const THE_SENTENCE_IN_THE_CONTRACT: (Thinking, &str) = (Thinking::Deepest, "xhigh");

/// Wszystkie cztery szczeble — wyczerpująco, żeby piąty nie skompilował tego pliku.
const EVERY: [Thinking; 4] = [
    Thinking::Quick,
    Thinking::Balanced,
    Thinking::Deep,
    Thinking::Deepest,
];

/// `RunSpec` kroku. Różni się od pozostałych wyłącznie tym, czego dotyczy dana asercja —
/// szczebel NIE jedzie polem tej struktury i to jest wybór projektowy: `RunSpec` nie ma
/// `Default` i konstruuje go w tym drzewie ponad trzydzieści miejsc.
fn spec() -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: PathBuf::from("."),
        prompt: "rename the widget".to_owned(),
        model: None,
        system_append: None,
        policy: Policy::EditInFolder,
        reaches_the_web: false,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Argumenty gotowej komendy jednej tury, jako właścicielskie napisy: komenda ginie razem z tą
/// funkcją.
fn argv(arguments: Vec<String>) -> Vec<String> {
    let driver = ClaudeDriver::new().with_configuration(DriverConfiguration {
        arguments,
        ..DriverConfiguration::default()
    });
    driver
        .command(&spec())
        .as_std()
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

/// Argumenty, którymi krok biegu oddaje sterownikowi szczebel agenta.
fn what_the_step_hands_over(agent: &Agent) -> Vec<String> {
    ClaudeDriver::new().effort_argv(effort_level(agent.thinking))
}

/// Wartość stojąca **zaraz za** flagą, plus ile razy ta flaga w ogóle padła.
fn effort_in(argv: &[String]) -> (Option<String>, usize) {
    let times = argv.iter().filter(|arg| *arg == EFFORT).count();
    let value = argv
        .iter()
        .position(|arg| arg == EFFORT)
        .and_then(|at| argv.get(at + 1))
        .cloned();
    (value, times)
}

/// Definicja agenta z tym szczeblem i niczym więcej różnym.
fn definition(thinking: Thinking) -> Agent {
    Agent {
        thinking,
        ..Agent::example()
    }
}

#[test]
fn the_deepest_rung_asks_the_vendor_for_its_deepest_level() -> Result<(), Box<dyn Error>> {
    let (rung, level) = THE_SENTENCE_IN_THE_CONTRACT;
    let handed = what_the_step_hands_over(&definition(rung));
    let line = argv(handed.clone());
    let (value, times) = effort_in(&line);

    assert_eq!(
        value.as_deref(),
        Some(level),
        "a step whose agent is saved on the deepest rung has to reach the CLI as `{EFFORT} \
         {level}`. The command came out as {line:?}"
    );
    assert_eq!(
        times, 1,
        "exactly one {EFFORT} in the line: zero means the rung never arrived, and two means the \
         CLI picks the last one while whoever reads the command believes the first. It came out \
         {times} time(s) in {line:?}"
    );
    Ok(())
}

#[test]
fn four_rungs_reach_the_command_as_four_different_levels() -> Result<(), Box<dyn Error>> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for rung in EVERY {
        let line = argv(what_the_step_hands_over(&definition(rung)));
        let (value, times) = effort_in(&line);
        let value = value.ok_or_else(|| {
            format!("the rung {rung:?} reached the command without {EFFORT} at all: {line:?}")
        })?;
        assert_eq!(
            times, 1,
            "the rung {rung:?} put {EFFORT} in the line {times} time(s): {line:?}"
        );
        assert_eq!(
            value,
            effort_level(rung),
            "the command has to carry the level the one table composed for {rung:?}. The table \
             said {:?} and the command said {value:?}",
            effort_level(rung)
        );
        seen.insert(value);
    }

    assert_eq!(
        seen.len(),
        EVERY.len(),
        "the four rungs reached the CLI as {} distinct level(s): {seen:?}. An adapter that spells \
         every rung the same way passes every assertion about the flag being present, and the \
         person who paid for the deepest rung gets the cheapest one",
        seen.len()
    );
    Ok(())
}

#[test]
fn the_step_overrides_the_agent_it_was_built_from() -> Result<(), Box<dyn Error>> {
    // Agent zapisany najniżej, krok podniesiony najwyżej: dwie wartości, które nie mogą
    // wypaść tak samo, więc pomyłka „czytam definicję zamiast nadpisania" jest widoczna.
    let saved = definition(Thinking::Quick);
    let raised = resolve(
        &saved,
        &Overrides {
            thinking: Some(Thinking::Deepest),
            ..Overrides::default()
        },
    )?
    .agent;

    let line = argv(what_the_step_hands_over(&raised));
    let (value, _times) = effort_in(&line);

    assert_eq!(
        value.as_deref(),
        Some(effort_level(Thinking::Deepest)),
        "the panel lets a step raise the rung above its agent's, so the run has to send the \
         step's answer. The agent was saved on the lowest rung and the command came out as \
         {line:?}"
    );
    assert_ne!(
        value.as_deref(),
        Some(effort_level(Thinking::Quick)),
        "the command carried the AGENT's rung, so the row in the step panel is a control with no \
         effect: {line:?}"
    );
    Ok(())
}

#[test]
fn the_lead_is_handed_what_the_step_is_handed() -> Result<(), Box<dyn Error>> {
    let mut walked = 0_usize;
    for rung in EVERY {
        let agent = definition(rung);
        let lead = Lead {
            agent: agent.clone(),
        };

        let by_the_conversation = ClaudeDriver::new().effort_argv(lead.effort());
        let by_the_run = what_the_step_hands_over(&agent);

        assert_eq!(
            by_the_conversation, by_the_run,
            "a conversation with a lead saved on rung {rung:?} has to be handed the same argv as \
             a run step with the same agent. The conversation got {by_the_conversation:?} and the \
             run got {by_the_run:?}"
        );

        let line = argv(by_the_conversation);
        let (value, times) = effort_in(&line);
        assert_eq!(
            value.as_deref(),
            Some(effort_level(rung)),
            "and it has to reach the conversation's command line too, not only the vector: {line:?}"
        );
        assert_eq!(times, 1, "one {EFFORT}, as everywhere else: {line:?}");
        walked += 1;
    }
    assert_eq!(walked, EVERY.len(), "every rung has to be walked here too");
    Ok(())
}

#[test]
fn a_step_that_says_nothing_about_thinking_still_asks_for_a_level() -> Result<(), Box<dyn Error>> {
    // Kontrola przeciw implementacji, która wysyła flagę wyłącznie „kiedy człowiek coś wybrał":
    // `Thinking` ma wartość domyślną (`balanced`), więc agent, którego nikt nie tykał, ma swój
    // szczebel tak samo jak każdy inny. Cisza w tym miejscu byłaby domyślną vendora, czyli
    // czwartą, nienazwaną pozycją dialu.
    let untouched = Agent::example();
    let line = argv(what_the_step_hands_over(&untouched));
    let (value, _times) = effort_in(&line);

    assert_eq!(
        value.as_deref(),
        Some(effort_level(untouched.thinking)),
        "an agent nobody edited still sits on a rung, and that rung has to reach the CLI. The \
         command came out as {line:?}"
    );
    Ok(())
}
