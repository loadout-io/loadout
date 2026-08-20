//! AC-2 dla T-63: sieć jest wyborem agenta na **każdym** szczeblu polityki.
//!
//! # Po co to istnieje
//!
//! To jest zamówienie, dla którego całe to zadanie powstało: lider do researchu, który **nie może
//! zepsuć repo**. Dziś takiego agenta nie da się skonfigurować — `WebFetch` i `WebSearch` daje
//! wyłącznie `Policy::Unrestricted`, czyli ta sama pozycja dialu, która daje `Write` i `Bash`.
//! Człowiek ma więc wybór między „widzi świat i może zepsuć pliki" a „nie zepsuje niczego i nie
//! widzi nic".
//!
//! Wycofane T-59 chciało to naprawić, wpuszczając sieć na każdy szczebel `Policy` — i to była
//! konstrukcja tania i zła: `driver_claude_policy_surface.rs` asertuje **ostre** zawieranie
//! `EditInFolder ⊊ Unrestricted`, a po przeniesieniu sieci w dół `Unrestricted` nie dokładałby do
//! `--tools` niczego własnego. Tamto kryterium jest DOBRE (ostre zawieranie łapie adapter
//! drukujący jedną listę dla trzech polityk), więc droga prowadzi tam, gdzie D6 wskazuje od
//! początku: **wszystko, co vendor wprowadzi, konfigurujemy per agent**. Sufit polityki zostaje
//! nietknięty; sieć wchodzi tylko wtedy, kiedy agent sam ją wymieni.
//!
//! **Ten plik nie czyta `claude.rs` z dysku** (niezmiennik 20). Wyrocznią jest zbudowana komenda.
//!
//! # Słaba wersja tego kryterium
//!
//! `assert!(tools.contains("WebSearch"))` na samym `--tools`. Przechodzi dla narzędzia, które jest
//! pod ręką i **zawsze odmawia**: `--tools` to lista dostępności, a przy `--permission-mode
//! dontAsk` nie ma kto zatwierdzić czynności spoza `--allowedTools`. Dostępność bez zatwierdzenia
//! jest w biegu bez człowieka bezużyteczna, i to jest cała treść punktu (a) — obie flagi, nie
//! jedna. Rozróżnia to (a) razem z (c): tryb uprawnień ma dalej wynikać WYŁĄCZNIE z polityki, więc
//! nie wolno kupić sobie tej sieci, przestawiając agenta na `bypassPermissions`.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w `driver_claude_tool_surface` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]

use std::error::Error;
use std::path::PathBuf;

use loadout_lib::commands::run::policy_of;
use loadout_lib::engine::drivers::RunSpec;
use loadout_lib::engine::drivers::claude::{ClaudeDriver, tool_surface};
use loadout_lib::library::agents::{Agent, FileAccess, Tools};
use uuid::Uuid;

/// Dwa czasowniki sięgające POZA repo. Wypisane tutaj, nie zaimportowane z `claude.rs`: test
/// czytający tę samą stałą co kod zawsze się z nim zgadza i nie mierzy niczego.
const WEB: [&str; 2] = ["WebSearch", "WebFetch"];

/// Trzy czasowniki, których agent „look only" nie ma prawa mieć w żadnej postaci.
///
/// `Bash` stoi tu gołe, a porównanie niżej idzie po **znormalizowanej** nazwie, bo `Bash(git *)`
/// z `--allowedTools` jest tym samym narzędziem w składni zakresowej — i przepuszczone przez
/// porównanie surowych napisów byłoby tą samą komendą w przebraniu.
const OFF_LIMITS: [&str; 3] = ["Edit", "Write", "Bash"];

/// Lista agenta do researchu: czytanie repo plus sieć, bez ani jednego czasownika zmieniającego
/// pliki. To jest dokładnie ten agent, którego dziś nie da się zapisać.
const LOOK_AND_LEARN: [&str; 5] = ["Read", "Grep", "Glob", "WebSearch", "WebFetch"];

/// Ten sam agent bez sieci — kontrola do punktu (d).
const LOOK_ONLY: [&str; 2] = ["Read", "Grep"];

/// Definicja agenta, jaką człowiek zapisał w bibliotece.
fn definition(access: FileAccess, tools: &[&str]) -> Agent {
    Agent {
        file_access: access,
        tools: Tools::Only(tools.iter().copied().map(str::to_owned).collect()),
        ..Agent::example()
    }
}

/// Argumenty gotowej komendy jednej tury dla TEGO agenta.
///
/// Droga jest cała: dial z definicji przechodzi przez [`policy_of`] (ta sama tabela, którą czyta
/// bieg), lista z definicji przez [`tool_surface`], i dopiero to wchodzi do [`RunSpec`].
fn argv_for(agent: &Agent) -> Vec<String> {
    let policy = policy_of(agent.file_access);
    let wanted = match &agent.tools {
        Tools::Everything => None,
        Tools::Only(names) => Some(names.clone()),
    };
    let surface = tool_surface(policy, wanted.as_deref());
    assert_eq!(
        surface.refused, None,
        "this fixture asks only for reading and for the web, and neither is above {policy:?}. It \
         refused {:?}",
        surface.refused
    );

    let spec = RunSpec {
        run_id: Uuid::now_v7(),
        cwd: PathBuf::from("."),
        prompt: "find out how the vendor names its rate limit fields".to_owned(),
        model: None,
        system_append: None,
        policy,
        tools: Some(surface.available),
        extra_dirs: Vec::new(),
        resume: None,
    };
    ClaudeDriver::new()
        .command(&spec)
        .as_std()
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

/// Wartość stojąca **zaraz za** flagą.
fn value_after(args: &[String], flag: &str) -> Option<String> {
    let at = args.iter().position(|arg| arg == flag)?;
    args.get(at + 1).cloned()
}

/// Pozycja listy sprowadzona do samej nazwy narzędzia — `Bash(git *)` znaczy tu `Bash`.
///
/// Obie flagi normalizujemy **tak samo**, bo inaczej porównywalibyśmy dwa różne alfabety.
fn normalise(entry: &str) -> String {
    entry
        .split_once('(')
        .map_or(entry, |(name, _)| name)
        .trim()
        .to_owned()
}

/// Nazwy narzędzi stojące za flagą, znormalizowane. `None`, kiedy flagi w argv nie ma.
fn entries(args: &[String], flag: &str) -> Option<Vec<String>> {
    value_after(args, flag).map(|value| value.split(',').map(normalise).collect())
}

/// Nazwy z `--tools` — brak tej flagi jest **porażką**, nie ciszą: bez niej agent zachowuje
/// cokolwiek CLI ma domyślnie, a `--allowedTools` tego nie zawęża.
fn availability(args: &[String]) -> Vec<String> {
    entries(args, "--tools").expect(
        "--tools has to be in argv: without it the agent keeps whatever the CLI ships with, and \
         --allowedTools only decides what goes without asking",
    )
}

#[test]
fn the_web_reaches_both_flags_for_a_look_only_agent() -> Result<(), Box<dyn Error>> {
    // ── (a) DOSTĘPNOŚĆ **I** ZATWIERDZENIE ──────────────────────────────────────────────────
    let agent = definition(FileAccess::LookOnly, &LOOK_AND_LEARN);
    let args = argv_for(&agent);

    let available = availability(&args);
    let approved = entries(&args, "--allowedTools").expect(
        "a 'look only' agent runs with --permission-mode dontAsk, so an available tool that is \
         not auto-approved can never be used: there is nobody at the keyboard to approve it. \
         The flag has to be there",
    );

    for tool in WEB {
        assert!(
            available.contains(&tool.to_owned()),
            "the agent's definition names {tool}, so it has to be in the set. Its list is what the \
             person chose; a run that drops it is a setting the app confirmed and ignored. \
             --tools came out as {available:?}"
        );
        assert!(
            approved.contains(&tool.to_owned()),
            "{tool} is available and NOT auto-approved, which in a run with nobody at the keyboard \
             is the same as absent - the agent asks and no answer ever comes. Availability without \
             approval is the version of this feature that looks like a tool which always refuses. \
             --allowedTools came out as {approved:?}"
        );
    }
    Ok(())
}

#[test]
fn the_same_agent_gets_no_writing_and_no_commands() -> Result<(), Box<dyn Error>> {
    // ── (b) SIEĆ NIE OTWIERA NICZEGO PRZY OKAZJI ────────────────────────────────────────────
    //
    // Bez tego punktu (a) przechodzi dla implementacji, która przy pierwszym narzędziu spoza
    // sufitu poddaje się i wysyła `bypassPermissions` z całą listą. Wtedy „lider do researchu"
    // dostaje `Write` i `Bash`, czyli dokładnie to, przed czym człowiek go zawężał.
    let agent = definition(FileAccess::LookOnly, &LOOK_AND_LEARN);
    let args = argv_for(&agent);

    let available = availability(&args);
    let approved = entries(&args, "--allowedTools").unwrap_or_default();

    for tool in OFF_LIMITS {
        assert!(
            !available.contains(&tool.to_owned()),
            "a 'look only' agent has {tool} within reach. This is an assertion about behaviour, \
             not about two strings differing: the person set this agent to look only, and reading \
             the web is not writing files. --tools came out as {available:?}"
        );
        assert!(
            !approved.contains(&tool.to_owned()),
            "a 'look only' agent auto-approves {tool}. The comparison runs on the normalised name, \
             so Bash(git *) counts as Bash - the scoped syntax is the same command in disguise. \
             --allowedTools came out as {approved:?}"
        );
    }
    Ok(())
}

#[test]
fn the_permission_mode_still_comes_from_the_policy_alone() -> Result<(), Box<dyn Error>> {
    // ── (c) LISTA NARZĘDZI NIE RUSZA TRYBU UPRAWNIEŃ ────────────────────────────────────────
    //
    // To jest druga połowa punktu (a) i bez niej sieć da się „kupić", przestawiając agenta na
    // tryb, który zatwierdza wszystko. Tryb wynika z dialu i z niczego innego.
    let agent = definition(FileAccess::LookOnly, &LOOK_AND_LEARN);
    let args = argv_for(&agent);

    assert_eq!(
        value_after(&args, "--permission-mode").as_deref(),
        Some("dontAsk"),
        "the tool list must not move the permission mode. 'Read only' means dontAsk whatever the \
         agent asked for; anything wider buys the web by handing over everything else with it. \
         argv was {args:?}"
    );

    // Ta sama polityka bez zawężenia musi dać ten sam tryb — inaczej asercja wyżej pilnowałaby
    // stałej, a nie tego, że tryb pochodzi z dialu.
    let plain = definition(FileAccess::LookOnly, &LOOK_ONLY);
    assert_eq!(
        value_after(&args, "--permission-mode"),
        value_after(&argv_for(&plain), "--permission-mode"),
        "two agents on the same dial position reached the CLI with two different permission modes, \
         so the mode is being read off the tool list. The dial is the only thing that decides it"
    );
    Ok(())
}

#[test]
fn a_look_only_agent_without_the_web_does_not_get_it() -> Result<(), Box<dyn Error>> {
    // ── (d) KONTROLA: NIKT NIE DOSYPUJE SIECI ───────────────────────────────────────────────
    //
    // Bez tej kontroli punkt (a) przechodzi dla implementacji, która dokłada `WebSearch`
    // i `WebFetch` KAŻDEMU — czyli dla wpuszczenia sieci na wszystkie szczeble polityki, przez
    // które wywróciło się wycofane T-59. Sieć jest wyborem agenta, nie prezentem.
    let agent = definition(FileAccess::LookOnly, &LOOK_ONLY);
    let args = argv_for(&agent);

    let available = availability(&args);
    let approved = entries(&args, "--allowedTools").unwrap_or_default();

    for tool in WEB {
        assert!(
            !available.contains(&tool.to_owned()),
            "this agent's definition does not name {tool}, so nothing may hand it over. A run that \
             tops the list up is the same defect from the other side: the person's choice is not \
             what runs. --tools came out as {available:?}"
        );
        assert!(
            !approved.contains(&tool.to_owned()),
            "this agent does not ask for {tool} and yet it is auto-approved. \
             --allowedTools came out as {approved:?}"
        );
    }
    Ok(())
}
