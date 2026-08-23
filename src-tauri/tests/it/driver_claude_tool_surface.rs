//! AC-1 dla T-53: `--tools` jest **białą listą** i nie ma na niej ani jednej ścieżki startu
//! procesu.
//!
//! **Ten test nigdy nie czyta `claude.rs` z dysku** (niezmiennik 20). Wyrocznią jest
//! **zbudowana komenda**: selftest w repo źródłowym asertował obecność flagi w skrypcie,
//! przechodził **na komentarzu**, a żywa flaga brzmiała inaczej [raport 06 §2]. Tu ten sam
//! kształt kosztuje pieniądze — zmierzone 2026-08-19: agent Loadouta wywołał **projektowego
//! podagenta repo gospodarza**, ten wystartował jako osobny proces i spalił **38–41 tys.
//! tokenów** całkowicie poza widokiem i rozliczeniem Loadouta. Ani jednej czerwieni, ani
//! jednego wiersza na ekranie pracy, ani jednego dolara w podsumowaniu kroku.
//!
//! **Lista zakazana jest wypisana TUTAJ, dosłownie, i nie jest importowana z `claude.rs`.**
//! Test, który czyta tę samą stałą co kod, zawsze się z nim zgadza i nie mierzy niczego:
//! wykreślenie pozycji po jednej stronie wykreśla ją po obu naraz.
//!
//! # Dwie słabe wersje tego kryterium, obie przechodzą na dzisiejszym kodzie
//!
//! **Pierwsza.** `assert!(forbidden.iter().all(|f| !tools.contains(f)))` przechodzi
//! **idealnie** dla sterownika, który `--tools` nie wysyła w ogóle — przecięcie z pustą listą
//! jest puste. Rozróżniają to dwie rzeczy i obie są niżej: `value_after(&args, "--tools")`
//! zakończone `ok_or` (brak flagi jest **porażką**, nie milczeniem) oraz kontrola „co najmniej
//! dwie pozycje" postawiona **osobno dla każdej z trzech polityk**.
//!
//! **Druga.** `value.contains("Task")` na **sklejonym** stringu jest jednocześnie za czułe
//! (zapala się na legalnym `TaskOutput`) i za mało czułe, bo porównanie **surowej** pozycji
//! przez `==` przepuszcza `Task(*)` — składnia zakresowa należy do `--allowedTools`, a w białej
//! liście byłaby tą samą ścieżką startu procesu w przebraniu. Dlatego każda pozycja jest
//! **normalizowana** do części przed nawiasem otwierającym i porównywana **równością**.

use std::error::Error;
use std::ffi::OsStr;
use std::path::PathBuf;

use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::{Policy, RunSpec};
use uuid::Uuid;

/// Dziesięć nazw, których nie ma prawa być w zestawie **żadnej** polityki.
///
/// Osiem z nich startuje proces **poza naszą grupą procesów**, czyli poza dowodem śmierci
/// z niezmiennika 6: `Task`, `Workflow`, `SendMessage`, `CronCreate`, `RemoteTrigger`,
/// `ScheduleWakeup`, `EnterWorktree`, `Monitor`. `Agent` i `Skill` dokładamy, bo są tą samą
/// czynnością pod inną nazwą u tego samego vendora.
const FORBIDDEN: [&str; 10] = [
    "Task",
    "Agent",
    "Skill",
    "Workflow",
    "SendMessage",
    "CronCreate",
    "RemoteTrigger",
    "ScheduleWakeup",
    "EnterWorktree",
    "Monitor",
];

/// Trzy polityki po ludzku, razem z brzmieniem, jakie mają na ekranie — komunikat porażki ma
/// nazywać tę, która się przedostała, a `Policy::EditInFolder` nie jest zdaniem, które ktoś
/// przeczyta w oknie.
const POLICIES: [(Policy, &str); 3] = [
    (Policy::ReadOnly, "Read only"),
    (Policy::EditInFolder, "Can edit this folder"),
    (Policy::Unrestricted, "No limits"),
];

/// Słowo vendora znaczące „wszystkie narzędzia" — dokładnie ten stan, przed którym stoi to
/// zadanie. Drugą skrajnością jest argument o zerowej długości, czyli „żadnych narzędzi".
const EVERYTHING: &str = "default";

/// `RunSpec` różniący się od pozostałych **wyłącznie** polityką.
fn spec(policy: Policy) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: PathBuf::from("."),
        prompt: "rename the widget".to_owned(),
        model: None,
        system_append: None,
        policy,
        reaches_the_web: false,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Wartość stojąca **zaraz za** flagą. `None`, kiedy flagi nie ma albo kiedy jest ostatnia
/// i nikt jej nic nie podał.
fn value_after<'a>(args: &[&'a OsStr], flag: &str) -> Option<&'a OsStr> {
    let at = args.iter().position(|arg| *arg == OsStr::new(flag))?;
    args.get(at + 1).copied()
}

/// Pozycja białej listy sprowadzona do samej nazwy narzędzia.
///
/// `Bash(git *)` i `Task(*)` to ta sama składnia zakresowa — należy do `--allowedTools`, ale
/// nic nie broni jej wpisać także tutaj. Po ucięciu na `(` porównanie równością widzi
/// `Task(*)` jako `Task`, a `TaskOutput` zostaje `TaskOutput` i nie zapala się na próżno.
fn normalise(entry: &str) -> String {
    entry
        .split_once('(')
        .map_or(entry, |(name, _)| name)
        .trim()
        .to_owned()
}

/// Biała lista jednej polityki, wzięta ze **zbudowanej komendy**: wartość surowa i jej pozycje
/// po normalizacji.
///
/// Brak `--tools` jest **porażką**, nie ciszą — i to jest cała różnica między tym kryterium
/// a jego ozdobną wersją.
fn tool_surface(policy: Policy) -> Result<(String, Vec<String>), Box<dyn Error>> {
    let spec = spec(policy);
    let command = ClaudeDriver::new().command(&spec);
    let args: Vec<&OsStr> = command.as_std().get_args().collect();

    let value = value_after(&args, "--tools")
        .ok_or_else(|| {
            format!(
                "--tools is missing for {policy:?}, so the tool surface is whatever the CLI \
                 ships with today. --allowedTools does not close this: it is an auto-approval \
                 list, not an availability filter, and a tool outside it is still in the set - \
                 it just asks. In a run with nobody at the keyboard 'asks' does not mean 'will \
                 not'. argv was {args:?}"
            )
        })?
        .to_string_lossy()
        .into_owned();

    let entries = value.split(',').map(normalise).collect();
    Ok((value, entries))
}

#[test]
fn not_one_process_starting_tool_reaches_any_policy() -> Result<(), Box<dyn Error>> {
    for (policy, label) in POLICIES {
        let (value, entries) = tool_surface(policy)?;

        let leaked: Vec<&String> = entries
            .iter()
            .filter(|entry| FORBIDDEN.contains(&entry.as_str()))
            .collect();

        assert!(
            leaked.is_empty(),
            "'{label}' ({policy:?}) hands the agent {leaked:?}, and every one of those starts a \
             process outside our process group: the death proof from invariant 6 stays true and \
             stops meaning anything, because it is not that group we killed. One of them burned \
             38-41k tokens entirely outside Loadout's own accounting on 2026-08-19. The whole \
             --tools value was {value:?}"
        );
    }

    Ok(())
}

#[test]
fn every_policy_names_at_least_two_tools_and_neither_vendor_extreme() -> Result<(), Box<dyn Error>>
{
    for (policy, label) in POLICIES {
        let (value, entries) = tool_surface(policy)?;

        assert!(
            !value.is_empty(),
            "'{label}' ({policy:?}) was handed a zero-length --tools, which is the vendor's own \
             word for 'disable all tools'. An agent that cannot read a single file is not a \
             narrower policy, it is a broken one"
        );
        assert_ne!(
            value, EVERYTHING,
            "'{label}' ({policy:?}) was handed the vendor's word for 'use all tools' - which is \
             exactly the state this task exists to leave behind"
        );
        assert!(
            entries.len() >= 2,
            "'{label}' ({policy:?}) reached the CLI with {} tool(s): {entries:?}. This check is \
             here because an empty whitelist has an empty intersection with the forbidden list, \
             so a driver that sends nothing passes the assertion above perfectly",
            entries.len()
        );
    }

    Ok(())
}
