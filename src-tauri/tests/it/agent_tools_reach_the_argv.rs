//! AC-1 dla T-63: lista z definicji agenta dojeżdża do `--tools`, a agent domyślny nie zmienia
//! w argv ani bajtu.
//!
//! # Po co to istnieje
//!
//! `Agent.tools` jest polem formularza agenta od T-11: widać je w panelu kroku jako
//! „Agent uses: …", zapisuje się na dysk — i nie ma ani jednego konsumenta w silniku.
//! `RunSpec` nie miał pola na narzędzia, `commands/run.rs` nigdy `agent.tools` nie czytał,
//! a jedynym źródłem `--tools` był `tools_for(policy)`. Człowiek, który zawęża narzędzia agentowi,
//! bo nie chce, żeby sięgał do sieci albo odpalał komendy, dostaje ekran, który to przyjmuje,
//! zapisuje i potwierdza. Agent i tak dostaje wszystko, co daje jego polityka, i nikt się o tym
//! nie dowie: „agent nie użył narzędzia" jest z zewnątrz nieodróżnialne od „agent uznał, że nie
//! warto". To jest martwa kontrolka (niezmiennik 16) schowana o warstwę głębiej — takiej nie da
//! się zobaczyć, klikając.
//!
//! **Ten plik nie czyta `claude.rs` z dysku** (niezmiennik 20). Wyrocznią jest **zbudowana
//! komenda**, z tego samego powodu, który stoi w nagłówku `driver_claude_tool_surface.rs`:
//! selftest w repo źródłowym asertował obecność flagi w skrypcie, przechodził **na komentarzu**,
//! a żywa flaga brzmiała inaczej [raport 06 §2].
//!
//! # Słaba wersja tego kryterium
//!
//! Sam punkt (a), czyli „`Tools::Only` daje swoją listę". Przechodzi dla implementacji, która
//! `Tools::Everything` ignoruje i **zawsze** składa listę po swojemu — a to przewraca
//! `claude_argv_policy.rs` i `driver_claude_policy_surface.rs` dokładnie tak, jak zrobiło to
//! wycofane T-59. Rozróżnia to punkt (b): dla agenta domyślnego obie flagi muszą wyjść znak
//! w znak takie, jakie wychodziły przed tym zadaniem. Dlatego napisy `--allowedTools` stoją niżej
//! **wypisane dosłownie**, a nie zaimportowane z `permission_flags`: test czytający tę samą stałą
//! co kod zawsze się z nim zgadza i nie mierzy niczego.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w `driver_claude_tool_surface` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]
// Wynik `Result` w teście, który nigdy nie oddaje `Err`: clippy nazywa to zbędnym opakowaniem,
// a `--all-targets` w pełnej bramce podnosi to do błędu (`quick` katalogu tests/ nie widzi).
// ADDYTYWNIE, bo asercji tego pliku zdejmować nie wolno — a jednolity kształt podpisu jest tym,
// dzięki któremu `?` da się dopisać w kolejnej asercji bez ruszania nagłówka funkcji.
#![allow(clippy::unnecessary_wraps)]

use std::error::Error;
use std::path::PathBuf;

use loadout_lib::commands::run::policy_of;
use loadout_lib::engine::drivers::claude::{ClaudeDriver, ToolsRefused, tool_surface, tools_for};
use loadout_lib::engine::drivers::{Policy, RunSpec};
use loadout_lib::library::agents::{Agent, FileAccess, Tools};
use uuid::Uuid;

/// Trzy pozycje dialu razem z polityką, na którą je tłumaczy bieg, i brzmieniem z ekranu.
///
/// Dial, nie sama [`Policy`], bo to dial widzi człowiek w formularzu agenta — a tłumaczenie idzie
/// przez [`policy_of`], czyli przez TĘ SAMĄ tabelę, którą czyta bieg (niezmiennik 23).
const DIAL: [(FileAccess, Policy, &str); 3] = [
    (FileAccess::LookOnly, Policy::ReadOnly, "Read only"),
    (
        FileAccess::AskFirst,
        Policy::EditInFolder,
        "Can edit this folder",
    ),
    (FileAccess::WorkFreely, Policy::Unrestricted, "No limits"),
];

/// `--allowedTools`, znak w znak tak, jak wychodziło przed T-63 — **wypisane tutaj**, nie
/// zaimportowane.
///
/// To jest cała treść punktu (b) i jedyna rzecz, która chroni trzech wyładowanych strażników:
/// `claude_argv_policy.rs` asertuje dokładnie te napisy, a `driver_claude_policy_surface.rs`
/// asertuje **ostre** zawierania trzech list. Implementacja, która składa listę po swojemu także
/// dla agenta domyślnego, przewraca oba te pliki naraz — i to jest dokładnie ten sposób, w jaki
/// wywróciło się wycofane T-59.
const AUTO_APPROVED_BEFORE_T63: [(Policy, Option<&str>); 3] = [
    (Policy::ReadOnly, Some("Read,Grep,Glob")),
    (
        Policy::EditInFolder,
        Some("Read,Grep,Glob,Edit,Write,Bash(git *)"),
    ),
    (Policy::Unrestricted, None),
];

/// Lista, którą człowiek wpisał agentowi w formularzu.
///
/// Świadomie **różna** od sufitu każdej z trzech polityk (`Glob` skreślony, `WebSearch` dopisany):
/// fikstura równa `tools_for(policy)` porównywałaby dwie identyczne rzeczy i przechodziłaby dla
/// sterownika, który pola `tools` nie czyta wcale. Pilnuje tego kontrola w punkcie (e).
const NARROWED: [&str; 3] = ["Read", "Grep", "WebSearch"];

/// Definicja agenta, jaką człowiek zapisał w bibliotece.
///
/// `Agent::example()` jako baza, bo „jak wygląda zapisany agent" ma w tym repo jedną odpowiedź
/// (`library::agents`); ręcznie wypisane piętnaście pól byłoby drugą.
fn definition(access: FileAccess, tools: Tools) -> Agent {
    Agent {
        file_access: access,
        tools,
        ..Agent::example()
    }
}

/// Lista narzędzi z definicji agenta.
///
/// `Tools::Everything` nie wymienia niczego i to jest jego treść: „tyle, ile daje polityka".
fn asked_for(agent: &Agent) -> Option<Vec<String>> {
    match &agent.tools {
        Tools::Everything => None,
        Tools::Only(names) => Some(names.clone()),
    }
}

/// Argumenty gotowej komendy jednej tury — jako właścicielskie napisy, bo komenda ginie razem
/// z tą funkcją.
fn argv(policy: Policy, tools: Option<Vec<String>>) -> Vec<String> {
    let spec = RunSpec {
        run_id: Uuid::now_v7(),
        cwd: PathBuf::from("."),
        prompt: "rename the widget".to_owned(),
        model: None,
        system_append: None,
        policy,
        tools,
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

/// Wartość stojąca **zaraz za** flagą. `None`, kiedy flagi nie ma albo kiedy nikt jej nic nie podał.
fn value_after(args: &[String], flag: &str) -> Option<String> {
    let at = args.iter().position(|arg| arg == flag)?;
    args.get(at + 1).cloned()
}

/// Ile razy flaga stoi w argv. Liczba, nie obecność: druga `--tools` znaczy, że wygrywa ostatnia,
/// a każde sprawdzenie pytające o obecność zostaje wtedy zielone.
fn count_of(args: &[String], flag: &str) -> usize {
    args.iter().filter(|arg| *arg == flag).count()
}

/// Napisy z tablicy fikstur jako lista właścicielska.
fn names(list: &[&str]) -> Vec<String> {
    list.iter().copied().map(str::to_owned).collect()
}

#[test]
fn the_agents_own_list_is_the_whole_tools_flag() -> Result<(), Box<dyn Error>> {
    // ── (a) LISTA Z DEFINICJI, CAŁA I TYLKO ONA ─────────────────────────────────────────────
    let agent = definition(FileAccess::LookOnly, Tools::Only(names(&NARROWED)));
    let policy = policy_of(agent.file_access);
    let wanted = asked_for(&agent).expect("an agent defined as Tools::Only names its tools");

    // ── (e) KONTROLA: FIKSTURA MUSI SIĘ RÓŻNIĆ OD SUFITU ────────────────────────────────────
    //
    // Bez tej linii wszystko niżej przechodzi dla sterownika, który pola `tools` nie czyta wcale:
    // porównywalibyśmy `tools_for(policy)` z `tools_for(policy)`.
    assert_ne!(
        wanted,
        names(tools_for(policy)),
        "this fixture has to ask for something OTHER than the policy's own ceiling, or the \
         assertions below compare two identical things and pass for a driver that never reads the \
         agent's list at all"
    );

    let surface = tool_surface(policy, Some(&wanted));
    assert_eq!(
        surface.refused, None,
        "every name on this list is either within {policy:?} or a web tool, so nothing may be \
         refused. It refused {:?}",
        surface.refused
    );
    assert_eq!(
        surface.available, wanted,
        "the list the person typed has to come back whole and in their order: the tool surface is \
         what they chose, not a set the driver rebuilt. It came out as {:?}",
        surface.available
    );

    let args = argv(policy, Some(surface.available));
    assert_eq!(
        count_of(&args, "--tools"),
        1,
        "--tools has to reach the CLI exactly once, with one comma-separated argument - the same \
         shape claude --help gives it. Twice means the last one wins and the first is a line in \
         `ps` that nobody obeys. argv was {args:?}"
    );
    assert_eq!(
        value_after(&args, "--tools").as_deref(),
        Some(NARROWED.join(",").as_str()),
        "an agent whose definition narrows its tools has to reach the CLI with exactly that list. \
         Anything wider is a setting the person made, the app confirmed, and the run ignored. \
         argv was {args:?}"
    );
    Ok(())
}

#[test]
fn the_default_agent_changes_not_one_byte() -> Result<(), Box<dyn Error>> {
    // ── (b) `Tools::Everything` ZOSTAWIA ARGV TAKIE, JAKIE BYŁO ─────────────────────────────
    //
    // To jest cała różnica między tym zadaniem a wycofanym T-59: dopóki agent domyślny składa te
    // same dwie flagi, trzej strażnicy (`claude_argv_policy`, `driver_claude_policy_surface`,
    // `driver_claude_tool_surface`) zostają prawdziwi BEZ tknięcia.
    for (access, policy, label) in DIAL {
        let agent = definition(access, Tools::Everything);
        assert_eq!(
            policy_of(agent.file_access),
            policy,
            "the dial has to reach the driver through the run's own table, so '{label}' means \
             {policy:?}"
        );
        assert_eq!(
            asked_for(&agent),
            None,
            "`Tools::Everything` asks for nothing of its own - it means 'as much as the policy \
             gives'. Anything else here would be the default agent carrying a list"
        );

        let args = argv(policy, None);

        assert_eq!(
            count_of(&args, "--tools"),
            1,
            "'{label}' ({policy:?}) reached the CLI with --tools {} time(s). argv was {args:?}",
            count_of(&args, "--tools")
        );
        assert_eq!(
            value_after(&args, "--tools").as_deref(),
            Some(tools_for(policy).join(",").as_str()),
            "'{label}' ({policy:?}) has to hand the default agent the policy's own ceiling, byte \
             for byte. A driver that rebuilds this list for everybody breaks the three landed \
             guards that pin these exact strings. argv was {args:?}"
        );

        let expected = AUTO_APPROVED_BEFORE_T63
            .iter()
            .find(|(row, _)| *row == policy)
            .map(|(_, value)| *value)
            .expect("every policy has a row in the pre-T63 auto-approval table");
        assert_eq!(
            value_after(&args, "--allowedTools").as_deref(),
            expected,
            "'{label}' ({policy:?}) has to auto-approve exactly what it auto-approved before this \
             task. These strings are pinned by claude_argv_policy.rs, which this task must leave \
             green without touching it. argv was {args:?}"
        );
        assert_eq!(
            count_of(&args, "--allowedTools"),
            usize::from(expected.is_some()),
            "'{label}' ({policy:?}) carries --allowedTools the wrong number of times. argv was \
             {args:?}"
        );
    }
    Ok(())
}

#[test]
fn an_empty_list_is_a_refusal_never_an_empty_flag() -> Result<(), Box<dyn Error>> {
    // ── (c) `Only([])` ODMAWIA, NIE WYSYŁA PUSTKI ───────────────────────────────────────────
    //
    // `--tools ""` znaczy u vendora „żadnych narzędzi", czyli agent, który nie przeczyta ani
    // jednego pliku i z zewnątrz wygląda dokładnie jak agent zawieszony. Człowiek, który
    // wyczyścił listę, ma dostać zdanie, a nie taki bieg.
    let agent = definition(FileAccess::LookOnly, Tools::Only(Vec::new()));
    let wanted = asked_for(&agent).expect("Tools::Only carries its list even when it is empty");
    assert!(
        wanted.is_empty(),
        "this fixture is the cleared list; anything else measures a different state"
    );

    for (_, policy, label) in DIAL {
        let surface = tool_surface(policy, Some(&wanted));
        assert_eq!(
            surface.refused,
            Some(ToolsRefused::NothingChosen),
            "'{label}' ({policy:?}) took a cleared tool list as an instruction. An empty list is a \
             refusal at build time: --tools \"\" is the vendor's own word for 'disable all tools', \
             so the step would start an agent that cannot read a single file. It answered {:?}",
            surface.refused
        );
        assert!(
            !surface.available.is_empty(),
            "'{label}' ({policy:?}) came back with an empty availability list, which is the one \
             value no policy may ever send. A refused step never starts, so this list stays the \
             policy's ceiling - it must never become the zero-length argument itself"
        );
    }
    Ok(())
}

#[test]
fn no_limits_still_sends_no_auto_approval_list() -> Result<(), Box<dyn Error>> {
    // ── (d) `Unrestricted` Z ZAWĘŻONĄ LISTĄ DALEJ NIE WYSYŁA `--allowedTools` ────────────────
    //
    // Lista dozwolonych nie wiąże `bypassPermissions` — wszystko jest zatwierdzone niezależnie od
    // niej [T1 §5.2] — więc wysłanie jej byłoby kłamstwem o tym, co ogranicza: w argv widać listę,
    // w rzeczywistości nie obowiązuje nic, a kto czyta `ps` albo dziennik, ten uwierzy liście.
    // Zawężenie ZESTAWU jest tu prawdą (`--tools` jest twarda), zawężenie ZATWIERDZANIA nie jest.
    let agent = definition(FileAccess::WorkFreely, Tools::Only(names(&NARROWED)));
    let policy = policy_of(agent.file_access);
    assert_eq!(
        policy,
        Policy::Unrestricted,
        "this case is about the top of the dial; anything else measures a different policy"
    );

    let wanted = asked_for(&agent).expect("an agent defined as Tools::Only names its tools");
    let surface = tool_surface(policy, Some(&wanted));
    let args = argv(policy, Some(surface.available.clone()));

    assert_eq!(
        value_after(&args, "--tools").as_deref(),
        Some(NARROWED.join(",").as_str()),
        "'No limits' with a narrowed list has to reach the CLI with that list: --tools is an \
         availability filter and it binds whatever the permission mode says. argv was {args:?}"
    );
    assert_eq!(
        count_of(&args, "--allowedTools"),
        0,
        "'No limits' with a narrowed list still must not send --allowedTools at all. The list does \
         not constrain bypassPermissions, so sending one says something is restricted when nothing \
         is - and this is the exact string claude_argv_policy.rs pins as absent. argv was {args:?}"
    );
    Ok(())
}
