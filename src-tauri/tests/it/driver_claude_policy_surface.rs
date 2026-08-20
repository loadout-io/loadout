//! AC-2 dla T-53: trzy polityki, trzy **niepuste i coraz szersze** powierzchnie narzędzi,
//! a `--allowedTools` przestaje być jedynym ogranicznikiem.
//!
//! To są **dwie różne flagi o dwóch różnych znaczeniach** i cała ta fala jest o tym
//! rozróżnieniu. `--allowedTools` to lista **auto-zatwierdzania**: narzędzie spoza niej dalej
//! jest w zestawie, tylko zapyta. `--tools` to twarda lista **dostępności**: czego na niej nie
//! ma, tego proces nie ma pod ręką [zmierzone 2026-08-19]. Kryterium T-04
//! (`claude_argv_policy.rs`) dalej mówi, że `Unrestricted` **nie wysyła** `--allowedTools`,
//! i to zadanie tego nie rusza.
//!
//! **Ten test nie czyta `claude.rs` z dysku** (niezmiennik 20). Pyta zbudowaną komendę.
//!
//! # Dwie słabe wersje tego kryterium
//!
//! **Pierwsza.** `assert!(!tools.is_empty())` powtórzone dla trzech polityk przechodzi dla
//! adaptera, który wypisuje **jedną i tę samą** listę wszystkim trzem — czyli dla dokładnie tej
//! pomyłki, którą T-04 nazwało już raz przy `--permission-mode` („trzy polityki po ludzku muszą
//! dojść do CLI jako trzy różne tryby"). Rozróżnia to łańcuch **ostrych** zawierań
//! `ReadOnly ⊊ EditInFolder ⊊ Unrestricted` plus jawna nieobecność `Write` i `Edit`
//! w `ReadOnly`: agent obiecany jako czytający nie ma prawa mieć pod ręką pisania, a agent bez
//! ograniczeń nie ma prawa mieć **mniej** niż ten, który edytuje folder.
//!
//! **Druga** siedzi przy izolacji kontekstu. `has_flag(&args, "--setting-sources")` przechodzi
//! dla argv, które niesie tę flagę **dwa razy**, drugi raz z `project` — a to jest dokładnie
//! ten kształt, w którym haki gospodarza wracają tego dnia, w którym ktoś doda `--settings`
//! i „zrobi, żeby się wczytywało". Wtedy jego `PreToolUse` znów startuje proces w swojej
//! grupie, dziecko dostaje `ppid=1` i przeżywa wyjście `claude` [30 sierot, 2026-08-19].
//! Widzi to wyłącznie **liczba wystąpień** postawiona razem z asercją o pustym argumencie.

use std::collections::BTreeSet;
use std::error::Error;
use std::ffi::OsStr;
use std::path::PathBuf;

use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::{Policy, RunSpec};
use uuid::Uuid;

/// Trzy polityki po ludzku, razem z brzmieniem, jakie mają na ekranie.
const POLICIES: [(Policy, &str); 3] = [
    (Policy::ReadOnly, "Read only"),
    (Policy::EditInFolder, "Can edit this folder"),
    (Policy::Unrestricted, "No limits"),
];

/// Słowo vendora znaczące „wszystkie narzędzia".
const EVERYTHING: &str = "default";

/// Dwa czasowniki, których agent obiecany jako czytający nie ma prawa mieć w zestawie.
const WRITING: [&str; 2] = ["Write", "Edit"];

/// `RunSpec` różniący się od pozostałych **wyłącznie** polityką.
fn spec(policy: Policy) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: PathBuf::from("."),
        prompt: "rename the widget".to_owned(),
        model: None,
        system_append: None,
        policy,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Wartość stojąca **zaraz za** flagą.
fn value_after<'a>(args: &[&'a OsStr], flag: &str) -> Option<&'a OsStr> {
    let at = args.iter().position(|arg| *arg == OsStr::new(flag))?;
    args.get(at + 1).copied()
}

/// Pozycja listy sprowadzona do samej nazwy narzędzia — `Bash(git *)` znaczy tu `Bash`.
///
/// Obie flagi normalizujemy **tak samo**, bo inaczej porównanie podzbioru byłoby porównaniem
/// dwóch różnych alfabetów: `Bash(git *)` z `--allowedTools` nigdy nie znalazłoby się w
/// `--tools`, choć narzędzie jest dokładnie to samo.
fn normalise(entry: &str) -> String {
    entry
        .split_once('(')
        .map_or(entry, |(name, _)| name)
        .trim()
        .to_owned()
}

/// To, o co pyta to kryterium, wyjęte z jednej zbudowanej komendy.
#[derive(Debug)]
struct Surface {
    /// Surowa wartość `--tools`: pusta i `default` to dwa słowa vendora o dwóch skrajnościach,
    /// a po rozbiciu po przecinku obie wyglądają jak zwykła jednoelementowa lista.
    raw: String,
    /// Co jest **dostępne**.
    tools: BTreeSet<String>,
    /// Co jest **auto-zatwierdzone**. `None`, kiedy flagi nie ma — tak wygląda `Unrestricted`
    /// od T-04 i tak ma zostać.
    allowed: Option<BTreeSet<String>>,
}

/// Powierzchnia narzędzi jednej polityki, wzięta ze zbudowanej komendy.
fn surface_of(policy: Policy) -> Result<Surface, Box<dyn Error>> {
    let spec = spec(policy);
    let command = ClaudeDriver::new().command(&spec);
    let args: Vec<&OsStr> = command.as_std().get_args().collect();

    let raw = value_after(&args, "--tools")
        .ok_or_else(|| {
            format!(
                "--tools is missing for {policy:?}: without it the agent keeps whatever the CLI \
                 ships with, and --allowedTools does not narrow that - it only decides what \
                 gets approved without asking. argv was {args:?}"
            )
        })?
        .to_string_lossy()
        .into_owned();

    let tools = raw.split(',').map(normalise).collect();
    let allowed = value_after(&args, "--allowedTools")
        .map(|value| value.to_string_lossy().split(',').map(normalise).collect());

    Ok(Surface {
        raw,
        tools,
        allowed,
    })
}

#[test]
fn unrestricted_is_not_handed_the_vendors_word_for_no_tools() -> Result<(), Box<dyn Error>> {
    let surface = surface_of(Policy::Unrestricted)?;

    assert!(
        !surface.raw.is_empty(),
        "'No limits' was handed a zero-length --tools, and in the vendor's own words that means \
         'disable all tools' - so the agent promised no limits could not read a single file. \
         The two extremes are the two values no policy may ever send"
    );
    assert_ne!(
        surface.raw, EVERYTHING,
        "'No limits' was handed the vendor's word for 'use all tools', which is the state this \
         task exists to leave behind: it puts all eight process-starting tools back in the set"
    );
    assert!(
        surface.tools.len() >= 2,
        "'No limits' reached the CLI with {} tool(s): {:?}. Anything this thin is not a policy, \
         it is a driver that forgot to fill the table in",
        surface.tools.len(),
        surface.tools
    );

    Ok(())
}

#[test]
fn the_three_policies_are_three_nested_surfaces() -> Result<(), Box<dyn Error>> {
    let read_only = surface_of(Policy::ReadOnly)?.tools;
    let editing = surface_of(Policy::EditInFolder)?.tools;
    let unlimited = surface_of(Policy::Unrestricted)?.tools;

    for verb in WRITING {
        assert!(
            !read_only.contains(verb),
            "'Read only' has {verb} available. This is an assertion about behaviour, not about \
             two strings being different: an agent promised as reading must not have writing \
             within reach. It came out as {read_only:?}"
        );
    }

    assert!(
        read_only.is_subset(&editing) && read_only != editing,
        "'Read only' has to be a STRICT subset of 'Can edit this folder'. They came out as \
         {read_only:?} and {editing:?} - and an adapter that prints one and the same list for \
         all three policies passes every check that only asks whether the list is non-empty"
    );
    assert!(
        editing.is_subset(&unlimited) && editing != unlimited,
        "'Can edit this folder' has to be a STRICT subset of 'No limits'. They came out as \
         {editing:?} and {unlimited:?} - an agent with no limits must not have LESS within \
         reach than one that only edits a folder"
    );

    Ok(())
}

#[test]
fn auto_approval_never_names_a_tool_that_is_not_available() -> Result<(), Box<dyn Error>> {
    for (policy, label) in POLICIES {
        let surface = surface_of(policy)?;
        let Some(allowed) = surface.allowed else {
            // `Unrestricted` nie wysyła `--allowedTools` od T-04 i to zadanie tego nie rusza:
            // lista dozwolonych nie wiąże `bypassPermissions`, więc jej wysłanie byłoby
            // kłamstwem o tym, co jest ograniczone [T1 §5.2].
            continue;
        };

        let promised: Vec<&String> = allowed.difference(&surface.tools).collect();
        assert!(
            promised.is_empty(),
            "'{label}' ({policy:?}) auto-approves {promised:?}, which is not in its --tools set \
             {:?}. A tool that is approved but unavailable is a promise the process cannot \
             keep, and whoever reads that argv line - in ps, in the log, in a bug report - \
             believes it",
            surface.tools
        );
    }

    Ok(())
}

#[test]
fn context_isolation_is_never_undone_by_a_second_setting_sources() -> Result<(), Box<dyn Error>> {
    for (policy, label) in POLICIES {
        let spec = spec(policy);
        let command = ClaudeDriver::new().command(&spec);
        let args: Vec<&OsStr> = command.as_std().get_args().collect();

        // LICZBA WYSTĄPIEŃ, nie obecność. Druga `--setting-sources project` dopisana „dla
        // pewności, żeby nasz plik ustawień się wczytał" przywraca ustawienia gospodarza
        // w całości, a każde sprawdzenie pytające o obecność flagi zostaje zielone.
        let count = args
            .iter()
            .filter(|arg| **arg == OsStr::new("--setting-sources"))
            .count();
        assert_eq!(
            count, 1,
            "'{label}' ({policy:?}) carries --setting-sources {count} time(s). More than once \
             means the last one wins and the host project's settings.json comes back with it - \
             its PreToolUse hook starts a process in its OWN group, that child gets ppid=1 and \
             outlives the exit of claude. Measured 2026-08-19: 14 orphans from one run, 30 \
             across the experiments. argv was {args:?}"
        );

        let sources = value_after(&args, "--setting-sources")
            .ok_or("--setting-sources was passed with nothing after it")?;
        assert!(
            sources.is_empty(),
            "'{label}' ({policy:?}) carries --setting-sources {sources:?}. The argument has to \
             be ZERO characters long: any value at all (say 'project') is context isolation \
             that does not isolate, and it is the only lever that puts out the host repo's hooks"
        );
    }

    Ok(())
}
