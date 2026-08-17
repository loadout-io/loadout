//! AC-2 dla T-04: trzy polityki mapują się na dokładnie te flagi, a `Unrestricted` nie udaje,
//! że coś ogranicza.
//!
//! **Słaba wersja tego kryterium to `assert!(args.iter().any(|a| *a == "--permission-mode"))`
//! dla każdej z trzech polityk.** Przechodzi ją implementacja, która wypisuje `dontAsk`
//! wszystkim trzem — czyli agent, któremu obiecano „No limits", nie może nic napisać,
//! a agent, któremu obiecano „Read only", nie jest ograniczony żadnym testem. Oba kierunki
//! pomyłki są niewidoczne dla asercji o obecności. Dlatego porównujemy **wartość** stojącą
//! na indeksie `i + 1`, osobno dla każdego wariantu.
//!
//! **Druga rzecz, którą mierzy ten plik: `Unrestricted` nie wysyła `--allowedTools`.**
//! Lista dozwolonych narzędzi **nie ogranicza** `bypassPermissions` — wszystko jest
//! zatwierdzone niezależnie od niej [T1 §5.2]. Wysłanie obu naraz to kłamstwo o tym, co jest
//! ograniczone: w argv widać listę, w rzeczywistości nie obowiązuje nic, a ktoś czytający
//! `ps` albo dziennik uwierzy liście.
//!
//! Tłumaczenie polityki na flagi jest **jedną tabelą w jednym adapterze** (niezmiennik 23).
//! Cicha wersja złamania nie wygląda jak drugi adapter — wygląda jak `if agent == "claude"`
//! w miejscu wywołania, i tak właśnie po cichu umarło skanowanie sekretów w repo źródłowym.

use std::error::Error;
use std::ffi::OsStr;
use std::path::PathBuf;

use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::{Policy, RunSpec};
use uuid::Uuid;

/// Wartość, której `--permission-mode` nie ma prawa nieść w żadnym wariancie.
///
/// `default` jest przyjmowane w czasie wykonania na 2.1.233, ale CLI **nie wymienia go**
/// w komunikacie odrzucenia — tam stoi
/// `acceptEdits, auto, bypassPermissions, manual, dontAsk, plan` — a dokumentacja nazywa
/// `manual` jego aliasem [T1 korekta 10]. Opieranie się na nazwie, której własne CLI nie
/// przyznaje, to jedna wersja od cichego „unknown option".
const NEVER: &str = "default";

/// `RunSpec` różniący się od pozostałych **wyłącznie** polityką.
fn spec(policy: Policy) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: PathBuf::from("."),
        prompt: "rename the widget".to_owned(),
        model: None,
        system_append: None,
        policy,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Wartość stojąca **zaraz za** flagą.
fn value_after<'a>(args: &[&'a OsStr], flag: &str) -> Option<&'a OsStr> {
    let at = args.iter().position(|arg| *arg == OsStr::new(flag))?;
    args.get(at + 1).copied()
}

/// Para (tryb uprawnień, lista dozwolonych narzędzi) odczytana z gotowej komendy.
///
/// Zwraca **właścicielskie** `String`/`Option<String>`, bo `get_args()` pożycza z komendy,
/// a komenda ginie razem z tą funkcją.
fn permissions_of(policy: Policy) -> Result<(String, Option<String>), Box<dyn Error>> {
    let spec = spec(policy);
    let command = ClaudeDriver::new().command(&spec);
    let args: Vec<&OsStr> = command.as_std().get_args().collect();

    let mode = value_after(&args, "--permission-mode")
        .ok_or("--permission-mode is missing or was passed with nothing after it")?
        .to_string_lossy()
        .into_owned();
    let tools = value_after(&args, "--allowedTools").map(|v| v.to_string_lossy().into_owned());
    Ok((mode, tools))
}

#[test]
fn read_only_asks_for_the_three_reading_tools_and_nothing_else() -> Result<(), Box<dyn Error>> {
    let (mode, tools) = permissions_of(Policy::ReadOnly)?;

    assert_eq!(
        mode, "dontAsk",
        "'Read only' is the vendor's own recommendation for a fixed, explicit tool surface in \
         a headless agent. It came out as {mode:?}"
    );
    assert_eq!(
        tools.as_deref(),
        Some("Read,Grep,Glob"),
        "an agent promised 'Read only' has to be handed exactly the reading tools. Anything \
         wider is a promise the run does not keep"
    );
    assert_ne!(mode, NEVER, "see the reason at NEVER");

    Ok(())
}

#[test]
fn edit_in_folder_adds_writing_and_git_and_nothing_wider() -> Result<(), Box<dyn Error>> {
    let (mode, tools) = permissions_of(Policy::EditInFolder)?;

    assert_eq!(
        mode, "acceptEdits",
        "'Can edit this folder' means edits land without a question, inside a scratch \
         workspace. It came out as {mode:?}"
    );
    assert_eq!(
        tools.as_deref(),
        Some("Read,Grep,Glob,Edit,Write,Bash(git *)"),
        "the scoped rule syntax is the whole point: Bash(git *) is git and only git, while a \
         bare Bash would be every command on the machine"
    );
    assert_ne!(mode, NEVER, "see the reason at NEVER");

    Ok(())
}

#[test]
fn unrestricted_says_so_and_does_not_pretend_a_list_still_binds() -> Result<(), Box<dyn Error>> {
    let (mode, tools) = permissions_of(Policy::Unrestricted)?;

    assert_eq!(
        mode, "bypassPermissions",
        "'No limits' has to actually be no limits; an agent promised it and handed dontAsk \
         cannot write a line. It came out as {mode:?}"
    );
    assert_eq!(
        tools, None,
        "--allowedTools does not constrain bypassPermissions - everything is approved whatever \
         the list says. Sending both is a lie about what is restricted: argv shows a list, the \
         run obeys none of it, and whoever reads that list believes it. It carried {tools:?}"
    );
    assert_ne!(mode, NEVER, "see the reason at NEVER");

    Ok(())
}

#[test]
fn the_three_policies_do_not_collapse_into_one_mode() -> Result<(), Box<dyn Error>> {
    let read_only = permissions_of(Policy::ReadOnly)?.0;
    let editing = permissions_of(Policy::EditInFolder)?.0;
    let unlimited = permissions_of(Policy::Unrestricted)?.0;

    assert!(
        read_only != editing && editing != unlimited && read_only != unlimited,
        "three plain-language policies have to reach the CLI as three different modes. They \
         came out as {read_only:?}, {editing:?} and {unlimited:?} - and an adapter that prints \
         one mode for all three passes every check that only asks whether the flag is there"
    );

    Ok(())
}
