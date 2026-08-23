//! AC-1 dla T-91: szczebel „ile agent ma myśleć" dojeżdża do argv `codex` — i tylko w tej turze,
//! w której Codex ma go czym przyjąć.
//!
//! # Po co to istnieje
//!
//! `Agent.thinking` nie miał do 2026-08-23 ani jednego czytelnika w silniku: `grep` po całym
//! drzewie znajdował `model_reasoning_effort` wyłącznie w `import/adapters.rs`, przy CZYTANIU
//! cudzej konfiguracji. Cztery szczeble z formularza kończyły się na dysku i tam zostawały.
//!
//! # Dwie rzeczy, które ten plik mierzy, i obie są konieczne
//!
//! 1. **Poziom stoi PRZED `exec`.** `-c klucz=wartość` jest u Codeksa opcją GLOBALNĄ, czyli
//!    rodzica, nie podkomendy. Zmierzone na tej maszynie 2026-08-23: podane po `exec` CLI
//!    odrzuca. Ta sama pomyłka wywróciła już raz `exec resume` (`-C` po `resume` kończyło się
//!    `unexpected argument '-C'`), a z okna wyglądało to jak `Didn't work · 0 turns · 0.0s` —
//!    proces wstaje, więc każda asercja pytająca tylko o OBECNOŚĆ flagi zostaje zielona.
//! 2. **Tura wznowienia flagi NIE powtarza.** `codex exec resume` wraca do wątku, który ma już
//!    swój wysiłek; powtórzenie jest w najlepszym razie zdaniem o tym samym, a w najgorszym
//!    przestawia rozmowę w połowie. Powtórzenia nie widać z zewnątrz nigdy — dlatego stoi tu
//!    asercja, a nie komentarz.
//!
//! Kontrola do punktu 2 jest częścią tego samego testu i bez niej punkt 2 jest pułapką:
//! implementacja kasująca z tury wznowienia WSZYSTKIE `-c` przechodzi „bez wysiłku w resume"
//! i po cichu zabiera rozmowie zatwierdzone połączenia MCP, które w każdej świeżej turze
//! `exec resume` muszą wrócić.
//!
//! # Słaba wersja tego kryterium
//!
//! `assert!(argv.iter().any(|a| a.contains("model_reasoning_effort")))`. Przechodzi dla adaptera,
//! który wypisuje jeden poziom wszystkim czterem szczeblom, i dla takiego, który wkłada go po
//! `exec`, czyli tam, gdzie CLI go odrzuca.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w `driver_codex_argv` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]
// Wynik `Result` w teście, który nigdy nie oddaje `Err`: clippy nazywa to zbędnym opakowaniem,
// a `--all-targets` w pełnej bramce podnosi to do błędu (`quick` katalogu tests/ nie widzi).
#![allow(clippy::unnecessary_wraps)]

use std::collections::BTreeSet;
use std::error::Error;
use std::path::{Path, PathBuf};

use loadout_lib::engine::drivers::codex::{CodexDriver, exec_argv, exec_resume_argv};
use loadout_lib::engine::drivers::{AgentDriver, DriverConfiguration, Policy, RunSpec};
use loadout_lib::library::agents::{Agent, Overrides, Thinking, effort_level, resolve};
use uuid::Uuid;

/// Klucz konfiguracji, którym Codex przyjmuje poziom wysiłku. Zmierzone 2026-08-23:
/// `-c model_reasoning_effort=<minimal|low|medium|high|xhigh>` jako opcja GLOBALNA.
///
/// **Wypisany tutaj**, nie zaimportowany ze sterownika: test czytający tę samą stałą, co kod,
/// zawsze się z nim zgadza i nie mierzy niczego (niezmiennik 20).
const KEY: &str = "model_reasoning_effort";

/// Podkomenda, przed którą opcje globalne muszą stać.
const EXEC: &str = "exec";

/// Szczebel, o który pyta wprost proza kryterium, i poziom, którym ma się odezwać u vendora.
const THE_SENTENCE_IN_THE_CONTRACT: (Thinking, &str) = (Thinking::Deep, "high");

/// Wszystkie cztery szczeble — wyczerpująco, żeby piąty nie skompilował tego pliku.
const EVERY: [Thinking; 4] = [
    Thinking::Quick,
    Thinking::Balanced,
    Thinking::Deep,
    Thinking::Deepest,
];

/// Katalog roboczy kroku. Czysta wartość, nie ścieżka na dysku: budowniczy argv jest funkcją
/// czystą i nie ma prawa niczego szukać.
const CWD: &str = "/loadout/step/one";

/// Identyfikator wątku, do którego wraca druga tura.
const THREAD: &str = "th_7c1e";

/// Wpis, który do argv przynoszą zatwierdzone Connections — kontrola do asercji o wznowieniu.
const CONNECTION: &str = "mcp_servers.figma.command=\"npx\"";

/// `RunSpec` pierwszej tury.
fn spec() -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: PathBuf::from(CWD),
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

/// Konfiguracja sterownika niosąca podane argumenty.
fn configured(arguments: Vec<String>) -> DriverConfiguration {
    DriverConfiguration {
        arguments,
        ..DriverConfiguration::default()
    }
}

/// Argumenty, którymi krok biegu oddaje sterownikowi szczebel agenta.
fn what_the_step_hands_over(agent: &Agent) -> Vec<String> {
    CodexDriver::new().effort_argv(effort_level(agent.thinking))
}

/// Definicja agenta z tym szczeblem i niczym więcej różnym.
fn definition(thinking: Thinking) -> Agent {
    Agent {
        thinking,
        ..Agent::example()
    }
}

/// Pozycje, na których w linii stoi `KEY=…`, razem z odczytaną wartością.
fn effort_in(argv: &[String]) -> Vec<(usize, String)> {
    argv.iter()
        .enumerate()
        .filter_map(|(at, arg)| {
            arg.strip_prefix(KEY)
                .and_then(|rest| rest.strip_prefix('='))
                .map(|value| (at, value.to_owned()))
        })
        .collect()
}

/// Gdzie w linii stoi podkomenda.
fn exec_at(argv: &[String]) -> Result<usize, Box<dyn Error>> {
    argv.iter()
        .position(|arg| arg == EXEC)
        .ok_or_else(|| format!("this is not a codex command line at all: {argv:?}").into())
}

#[test]
fn the_deep_rung_reaches_the_first_turn_before_the_subcommand() -> Result<(), Box<dyn Error>> {
    let (rung, level) = THE_SENTENCE_IN_THE_CONTRACT;
    let argv = exec_argv(
        &configured(what_the_step_hands_over(&definition(rung))),
        &spec(),
    );

    let found = effort_in(&argv);
    let (at, value) = match found.as_slice() {
        [one] => one.clone(),
        other => {
            return Err(format!(
                "the line has to carry {KEY} exactly once; it carried it {} time(s). Zero means \
                 the rung never arrived, two means the CLI picks the last one while whoever reads \
                 the line believes the first: {argv:?}",
                other.len()
            )
            .into());
        }
    };

    assert_eq!(
        value, level,
        "a step whose agent is saved on the 'deep' rung has to reach the CLI as \
         `-c {KEY}={level}`. The line came out as {argv:?}"
    );

    assert_eq!(
        argv.get(at.wrapping_sub(1)).map(String::as_str),
        Some("-c"),
        "the override is a KEY=VALUE pair and the pair needs its `-c`; without it the value is \
         a loose word in the line: {argv:?}"
    );

    let exec = exec_at(&argv)?;
    assert!(
        at < exec,
        "`-c` is a global option of `codex`, not of `exec`, so it has to stand BEFORE the \
         subcommand. Measured on this machine: passed after `exec` the CLI refuses, the process \
         still starts, and the window shows a turn that did nothing. The value sat at {at} and \
         `{EXEC}` at {exec}: {argv:?}"
    );
    Ok(())
}

#[test]
fn four_rungs_reach_the_line_as_four_different_levels() -> Result<(), Box<dyn Error>> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for rung in EVERY {
        let argv = exec_argv(
            &configured(what_the_step_hands_over(&definition(rung))),
            &spec(),
        );
        let found = effort_in(&argv);
        let [(at, value)] = found.as_slice() else {
            return Err(format!(
                "the rung {rung:?} put {KEY} in the line {} time(s): {argv:?}",
                found.len()
            )
            .into());
        };
        assert_eq!(
            value,
            effort_level(rung),
            "the line has to carry the level the one table composed for {rung:?}. The table said \
             {:?} and the line said {value:?}",
            effort_level(rung)
        );
        assert!(
            *at < exec_at(&argv)?,
            "every rung stands before the subcommand, not only the one the contract names: {argv:?}"
        );
        seen.insert(value.clone());
    }

    assert_eq!(
        seen.len(),
        EVERY.len(),
        "the four rungs reached the CLI as {} distinct level(s): {seen:?}. An adapter that spells \
         every rung the same way passes every assertion about the key being present, and the \
         person who paid for the deepest rung gets the cheapest one",
        seen.len()
    );
    Ok(())
}

#[test]
fn the_step_overrides_the_agent_it_was_built_from() -> Result<(), Box<dyn Error>> {
    // Agent zapisany najniżej, krok podniesiony najwyżej: dwie wartości, które nie mogą wypaść
    // tak samo, więc pomyłka „czytam definicję zamiast nadpisania" jest widoczna.
    let saved = definition(Thinking::Quick);
    let raised = resolve(
        &saved,
        &Overrides {
            thinking: Some(Thinking::Deepest),
            ..Overrides::default()
        },
    )?
    .agent;

    let argv = exec_argv(&configured(what_the_step_hands_over(&raised)), &spec());
    let found = effort_in(&argv);
    let [(_at, value)] = found.as_slice() else {
        return Err(format!("one override, not {}: {argv:?}", found.len()).into());
    };

    assert_eq!(
        value,
        effort_level(Thinking::Deepest),
        "the panel lets a step raise the rung above its agent's, so the run has to send the \
         step's answer. The agent was saved on the lowest rung and the line came out as {argv:?}"
    );
    assert_ne!(
        value.as_str(),
        effort_level(Thinking::Quick),
        "the line carried the AGENT's rung, so the row in the step panel is a control with no \
         effect: {argv:?}"
    );
    Ok(())
}

#[test]
fn the_resuming_turn_does_not_say_it_again() -> Result<(), Box<dyn Error>> {
    // Konfiguracja niosąca OBIE rzeczy naraz: zatwierdzone połączenie i szczebel. Dokładnie tak
    // wygląda krok, który ma jedno i drugie, i tylko tak da się odróżnić „nie powtarza wysiłku"
    // od „gubi wszystkie opcje globalne".
    let mut arguments = vec!["-c".to_owned(), CONNECTION.to_owned()];
    arguments.extend(what_the_step_hands_over(&definition(Thinking::Deep)));
    let configuration = configured(arguments);

    let first = exec_argv(&configuration, &spec());
    let again = exec_resume_argv(&configuration, THREAD, Path::new(CWD));

    assert_eq!(
        effort_in(&first).len(),
        1,
        "the first turn is the one that opens the thread, so it is the turn that says how hard to \
         think: {first:?}"
    );
    assert!(
        effort_in(&again).is_empty(),
        "`codex exec resume` returns to a thread that already has its effort, so repeating the \
         key is at best a second sentence about the same thing and at worst re-aims a \
         conversation halfway through. The resuming line came out as {again:?}"
    );

    // Kontrola, bez której asercja wyżej jest pułapką: implementacja kasująca z tury wznowienia
    // wszystkie `-c` przechodzi ją i po cichu zabiera rozmowie zatwierdzone połączenia, które
    // w każdej świeżej turze `exec resume` muszą wrócić.
    assert!(
        again.iter().any(|arg| arg == CONNECTION),
        "the approved connection has to survive into the resuming turn - every fresh \
         `exec resume` process needs it again. The line came out as {again:?}"
    );
    assert_eq!(
        again.iter().filter(|arg| *arg == "-c").count(),
        1,
        "one `-c` left, and it is the connection's: a stray flag with nothing after it swallows \
         the next word as its value. The line came out as {again:?}"
    );

    let resumed = exec_at(&again)?;
    let connection_at = again
        .iter()
        .position(|arg| arg == CONNECTION)
        .ok_or("the connection vanished from the resuming line")?;
    assert!(
        connection_at < resumed,
        "and what survived still stands before the subcommand, where the CLI accepts it: {again:?}"
    );
    Ok(())
}
