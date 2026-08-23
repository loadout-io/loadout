//! AC-3 dla T-63: polityka zostaje **sufitem**, a odmowa nazywa narzędzie i szczebel.
//!
//! # Po co to istnieje
//!
//! Lista narzędzi agenta wybiera **z** tego, co daje jego dial — nigdy ponad. Bez tego zdania pole
//! `tools` byłoby drugą drogą do uprawnień, obok trzypozycyjnego diala bezpieczeństwa, czyli
//! dokładnie tym, czego zakazuje `DECISIONS-LOCKED.md` §D6 o przelotce („przelotka nie omija diala
//! bezpieczeństwa") i czym w repo źródłowym po cichu umarło skanowanie sekretów [raport 05 §4].
//!
//! Druga połowa jest droższa i mniej oczywista: **odmowa musi być słyszalna**. Agent, któremu po
//! cichu zabrano narzędzie, wygląda z zewnątrz dokładnie jak agent, który „nie umiał" — pisze, że
//! zrobi, nie robi, a diagnoza zaczyna się od czytania promptu. Godzina za każdy taki przypadek.
//! Dlatego `tools` ponad sufitem jest odmową nazywającą **narzędzie** (żeby wiedzieć, który wiersz
//! skreślić) i **politykę** (żeby wiedzieć, że alternatywą jest poszerzenie dostępu).
//!
//! **Ten plik nie czyta `claude.rs` z dysku** (niezmiennik 20).
//!
//! # Słaba wersja tego kryterium
//!
//! Test na samym (a), czyli „`Write` nie dojechało". Przechodzi dla implementacji, która odcina po
//! cichu — czyli dla **najdroższej** wersji tej wady, tej, o której nikt się nie dowie. Rozróżnia
//! to (b). A samo (b) bez (c) przechodzi dla implementacji, która nie daje **nigdy nic**: dlatego
//! ten sam agent na `work freely` musi `Write` DOSTAĆ.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w `driver_claude_tool_surface` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]
// Wynik `Result` w teście, który nigdy nie oddaje `Err`: clippy nazywa to zbędnym opakowaniem,
// a `--all-targets` w pełnej bramce podnosi to do błędu (`quick` katalogu tests/ nie widzi).
// ADDYTYWNIE, bo asercji tego pliku zdejmować nie wolno — a jednolity kształt podpisu jest tym,
// dzięki któremu `?` da się dopisać w kolejnej asercji bez ruszania nagłówka funkcji.
#![allow(clippy::unnecessary_wraps)]
// Dwa zagnieżdżone `if let` w `every_tool_in`: clippy chce z nich jednego, a `--all-targets`
// w pełnej bramce podnosi to do błędu (`quick` tests/ nie widzi). ADDYTYWNIE, bo asercji tego
// pliku zdejmować nie wolno — a warunek „flaga jest" i warunek „coś za nią stoi" to dwa różne
// zdania o argv i sklejone czytają się jak jedno.
#![allow(clippy::collapsible_if)]

use std::collections::BTreeSet;
use std::error::Error;
use std::path::PathBuf;

use loadout_lib::commands::run::policy_of;
use loadout_lib::engine::drivers::claude::{
    ClaudeDriver, ToolSurface, ToolsRefused, tool_surface, tools_for,
};
use loadout_lib::engine::drivers::{Policy, RunSpec};
use loadout_lib::library::agents::{Agent, FileAccess, Tools};
use uuid::Uuid;

/// Trzy pozycje dialu razem z brzmieniem, jakie mają na ekranie — komunikat porażki ma nazywać tę,
/// która się przedostała, a `FileAccess::LookOnly` nie jest zdaniem, które ktoś przeczyta w oknie.
const DIAL: [(FileAccess, &str); 3] = [
    (FileAccess::LookOnly, "look only"),
    (FileAccess::AskFirst, "ask first"),
    (FileAccess::WorkFreely, "work freely"),
];

/// Dwa czasowniki sięgające POZA repo, na które sufit się **nie** rozciąga.
const WEB: [&str; 2] = ["WebSearch", "WebFetch"];

/// Definicja agenta, jaką człowiek zapisał w bibliotece.
fn definition(access: FileAccess, tools: &[&str]) -> Agent {
    Agent {
        file_access: access,
        tools: Tools::Only(tools.iter().copied().map(str::to_owned).collect()),
        ..Agent::example()
    }
}

/// Powierzchnia narzędzi TEGO agenta: jego lista przepuszczona przez sufit jego dialu.
///
/// Dial przechodzi przez [`policy_of`], czyli przez tę samą tabelę, którą czyta bieg
/// (niezmiennik 23) — asercja o wpisanej z palca [`Policy`] przechodziłaby także dla drugiej kopii
/// tego dopasowania.
fn surface_of(agent: &Agent) -> ToolSurface {
    let wanted = match &agent.tools {
        Tools::Everything => None,
        Tools::Only(names) => Some(names.clone()),
    };
    tool_surface(policy_of(agent.file_access), wanted.as_deref())
}

/// Argumenty gotowej komendy jednej tury, złożone z tego, co polityka rzeczywiście dała.
fn argv_for(agent: &Agent, available: Vec<String>) -> Vec<String> {
    let spec = RunSpec {
        run_id: Uuid::now_v7(),
        cwd: PathBuf::from("."),
        prompt: "rename the widget".to_owned(),
        model: None,
        system_append: None,
        reaches_the_web: false,
        policy: policy_of(agent.file_access),
        tools: Some(available),
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

/// Pozycja listy sprowadzona do samej nazwy narzędzia — `Bash(git *)` znaczy tu `Bash`.
fn normalise(entry: &str) -> String {
    entry
        .split_once('(')
        .map_or(entry, |(name, _)| name)
        .trim()
        .to_owned()
}

/// Wszystkie nazwy narzędzi, jakie widać w argv — z **obu** flag naraz.
///
/// Obie, bo „nie dostał `Write`" jest prawdą tylko wtedy, kiedy nie ma go ani w zestawie, ani na
/// liście auto-zatwierdzania: jedna flaga bez drugiej to połowa odpowiedzi.
fn every_tool_in(args: &[String]) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    for flag in ["--tools", "--allowedTools"] {
        if let Some(at) = args.iter().position(|arg| arg == flag) {
            if let Some(value) = args.get(at + 1) {
                seen.extend(value.split(',').map(normalise));
            }
        }
    }
    seen
}

#[test]
fn writing_is_never_handed_to_a_look_only_agent() -> Result<(), Box<dyn Error>> {
    // ── (a) LISTA WYBIERA Z DOSTĘPNYCH, NIGDY PONAD ─────────────────────────────────────────
    for asked in [["Read", "Write"], ["Read", "Bash"]] {
        let agent = definition(FileAccess::LookOnly, &asked);
        let over = asked[1];
        let surface = surface_of(&agent);

        assert!(
            !surface.available.iter().any(|tool| tool == over),
            "an agent set to 'look only' asked for {over} and got it. The tool list picks FROM what \
             the dial gives, never above it: a second road to permissions standing next to the \
             three-position safety dial is how secret scanning quietly died in the source repo. It \
             came out as {:?}",
            surface.available
        );

        let seen = every_tool_in(&argv_for(&agent, surface.available.clone()));
        assert!(
            !seen.contains(over),
            "{over} reached argv for an agent set to 'look only'. Neither flag may name it - \
             --tools is availability and --allowedTools is auto-approval, and the promise on screen \
             covers both. argv carried {seen:?}"
        );
    }
    Ok(())
}

#[test]
fn the_refusal_names_the_tool_and_the_policy() -> Result<(), Box<dyn Error>> {
    // ── (b) ODMOWA, NIE CICHE POMINIĘCIE ────────────────────────────────────────────────────
    let agent = definition(FileAccess::LookOnly, &["Read", "Write"]);
    let policy = policy_of(agent.file_access);
    let refused = surface_of(&agent).refused.expect(
        "asking a 'look only' agent for Write has to be REFUSED. Dropping it silently is the \
         expensive version of this defect: the agent looks like one that did not know how, and the \
         diagnosis starts by re-reading the prompt",
    );

    match refused {
        ToolsRefused::AbovePolicy {
            policy: named,
            tools,
        } => {
            assert_eq!(
                tools,
                vec!["Write".to_owned()],
                "the refusal has to name the tool it cut, by name and only the ones it cut: that \
                 name IS the row the person deletes in the form. It named {tools:?}"
            );
            assert_eq!(
                named, policy,
                "the refusal has to name the policy that cut it, because widening the dial is the \
                 other repair and a sentence without it leaves the person guessing which one they \
                 have. It named {named:?}, the agent runs under {policy:?}"
            );
        }
        ToolsRefused::NothingChosen => {
            return Err(
                "a list of two names is not an empty list: this request has tools in it, \
                        one of which is above the ceiling, so the refusal has to be the one that \
                        names it"
                    .into(),
            );
        }
    }
    Ok(())
}

#[test]
fn work_freely_does_hand_over_writing() -> Result<(), Box<dyn Error>> {
    // ── (c) KONTROLA DODATNIA: SUFIT NIE JEST ZAPORĄ NA WSZYSTKO ────────────────────────────
    //
    // Bez tego przypadku wszystko wyżej przechodzi dla implementacji, która nie daje NIGDY nic —
    // czyli dla sterownika, który odmawia każdej liście i tym samym kasuje całe to pole.
    let agent = definition(FileAccess::WorkFreely, &["Read", "Write"]);
    let surface = surface_of(&agent);

    assert_eq!(
        surface.refused, None,
        "an agent set to 'work freely' asking for Write has to get it: that dial position exists \
         precisely to allow writing. It refused {:?}",
        surface.refused
    );
    assert!(
        surface.available.iter().any(|tool| tool == "Write"),
        "Write is within reach of 'work freely', so it has to come through the tool list. It came \
         out as {:?}",
        surface.available
    );

    let seen = every_tool_in(&argv_for(&agent, surface.available));
    assert!(
        seen.contains("Write"),
        "Write never reached argv for an agent set to 'work freely'. argv carried {seen:?}"
    );
    Ok(())
}

#[test]
fn the_ceiling_covers_files_and_commands_but_not_the_web() -> Result<(), Box<dyn Error>> {
    // ── (d) SUFIT JEST O PLIKACH I O KOMENDACH ──────────────────────────────────────────────
    //
    // To jest cała różnica, dzięki której „look only" znaczy „nie zmienia plików", a nie „nie widzi
    // świata". Sieć wolno wymienić na każdym szczeblu; `Write` i `Bash` dopiero tam, gdzie dial je
    // daje.
    let watching = definition(FileAccess::LookOnly, &["Read", "WebSearch", "WebFetch"]);
    let surface = surface_of(&watching);
    assert_eq!(
        surface.refused, None,
        "the ceiling must not reach the web: 'look only' is a promise about FILES, and an agent \
         that cannot look anything up is a different product than the one the dial describes. It \
         refused {:?}",
        surface.refused
    );
    for tool in WEB {
        assert!(
            surface.available.iter().any(|name| name == tool),
            "{tool} was named by a 'look only' agent and has to come through. It came out as {:?}",
            surface.available
        );
    }

    // A czego ta furtka NIE otwiera: nazwy, której nie ma na ŻADNYM suficie. `Task` startuje proces
    // poza naszą grupą, czyli poza dowodem śmierci z niezmiennika 6 — jedno takie wywołanie spaliło
    // 38-41 tys. tokenów poza rozliczeniem Loadouta [2026-08-19]. Sufit dotyczy go na każdym
    // szczeblu, także na najwyższym.
    for (access, label) in DIAL {
        let starting = definition(access, &["Read", "Task"]);
        assert!(
            surface_of(&starting).refused.is_some(),
            "an agent set to '{label}' asked for Task and was not refused. No policy has it on its \
             ceiling: it starts a process outside our process group, so the death proof stays true \
             and stops meaning anything. The web exemption is two names, not a door for everything \
             the vendor ships"
        );
    }
    Ok(())
}

#[test]
fn the_three_policies_are_three_different_ceilings() -> Result<(), Box<dyn Error>> {
    // ── (e) KONTROLA: TRZY SUFITY, NIE JEDEN MIERZONY TRZY RAZY ─────────────────────────────
    //
    // Bez tej linii wszystko wyżej przechodzi dla `tools_for`, które oddaje jedną i tę samą listę
    // trzem politykom: (a) i (c) mierzyłyby wtedy ten sam sufit i nie mogłyby się różnić.
    let ceilings: Vec<(Policy, Vec<&str>)> = DIAL
        .iter()
        .map(|(access, _)| {
            let policy = policy_of(*access);
            (policy, tools_for(policy).to_vec())
        })
        .collect();

    for (index, (policy, ceiling)) in ceilings.iter().enumerate() {
        assert!(
            ceiling.len() >= 2,
            "{policy:?} has a ceiling of {} tool(s): {ceiling:?}. Anything this thin is not a \
             policy, it is a table nobody filled in",
            ceiling.len()
        );
        for (other, other_ceiling) in ceilings.iter().skip(index + 1) {
            assert_ne!(
                ceiling, other_ceiling,
                "{policy:?} and {other:?} have the same ceiling, so this whole file measures one \
                 ceiling three times: 'the list picks from what the dial gives' cannot be told \
                 apart from 'the list picks from one fixed set'"
            );
        }
    }
    Ok(())
}
