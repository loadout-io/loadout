//! AC-1 dla T-04: zbudowana komenda niesie zweryfikowane argv transportu i kontekstu,
//! a promptu w niej nie ma.
//!
//! **Ten test nigdy nie czyta `claude.rs` z dysku** (niezmiennik 20). Selftest w repo
//! źródłowym asertował `"--sandbox workspace-write" in ship-task.sh`, przechodził **na
//! komentarzu**, a żywa flaga brzmiała `danger-full-access` [raport 06 §2]. Tutaj ten sam
//! kształt kosztuje pieniądze: bez `--strict-mcp-config --setting-sources ""` jeden bieg
//! ładuje 73 narzędzia z 9 serwerów i pali 36 870 tokenów tworzenia cache'u zamiast 4 725
//! [T1 §3.3, korekta 4]. Nic nie pęka — jest tylko drożej i wolniej, na każdym kroku,
//! na zawsze. Dlatego pytamy zbudowaną komendę, a nie plik źródłowy.
//!
//! **Asercja jest na SĄSIEDZTWIE, nie na obecności.**
//! `assert!(args.contains(&OsStr::new("--setting-sources")))` przechodzi także wtedy, gdy
//! wartością jest `"user,project"` — czyli dokładnie wtedy, gdy izolacja kontekstu **nie
//! działa** i bieg dalej ładuje wszystko z `~/.claude`. Flaga stoi na indeksie `i`, wartość
//! na `i + 1`, i to wartość jest tym, co cokolwiek znaczy.

use std::error::Error;
use std::ffi::OsStr;
use std::path::PathBuf;

use loadout_lib::engine::drivers::claude::{ClaudeDriver, VENDOR};
use loadout_lib::engine::drivers::{Policy, RunSpec, SessionRef};
use uuid::Uuid;

/// Znacznik wklejony w `spec.prompt`. Musi być na tyle osobliwy, żeby jego obecność
/// w JAKIMKOLWIEK argumencie nie dała się wytłumaczyć zbiegiem okoliczności — prompt w argv
/// widzi `ps` każdego użytkownika maszyny (niezmiennik 9).
const PROMPT_MARK: &str = "loadout-t04-prompt-must-never-reach-argv-9d41c7";

/// Sesja, którą wznawiamy w wariancie z `--resume`.
const RESUMED: &str = "2f1b2be9-3f47-4d5e-9a1c-0b7e6c4a8d20";

/// Jeden `RunSpec` do zbudowania komendy. Polityka jest tu bez znaczenia — mierzy ją AC-2.
fn spec(resume: Option<SessionRef>) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: PathBuf::from("."),
        prompt: format!("summarise this repository -- {PROMPT_MARK}"),
        model: None,
        system_append: None,
        policy: Policy::ReadOnly,
        extra_dirs: Vec::new(),
        resume,
    }
}

/// Wartość stojąca **zaraz za** flagą. `None`, kiedy flagi nie ma albo kiedy jest ostatnia
/// i nikt jej nic nie podał.
fn value_after<'a>(args: &[&'a OsStr], flag: &str) -> Option<&'a OsStr> {
    let at = args.iter().position(|arg| *arg == OsStr::new(flag))?;
    args.get(at + 1).copied()
}

/// Czy flaga w ogóle padła. Używane wyłącznie tam, gdzie flaga nie ma wartości albo gdzie
/// mierzymy jej **nieobecność**.
fn has_flag(args: &[&OsStr], flag: &str) -> bool {
    args.iter().any(|arg| *arg == OsStr::new(flag))
}

#[test]
fn transport_flags_stand_next_to_the_values_that_were_verified() -> Result<(), Box<dyn Error>> {
    let fresh = spec(None);
    let command = ClaudeDriver::new().command(&fresh);
    let args: Vec<&OsStr> = command.as_std().get_args().collect();

    assert!(
        has_flag(&args, "-p"),
        "-p is the gate for every other flag here; without it the CLI runs interactively and \
         this process never sees a JSON line. argv was {args:?}"
    );
    assert_eq!(
        value_after(&args, "--output-format"),
        Some(OsStr::new("stream-json")),
        "the run has to emit structured events, not terminal bytes. argv was {args:?}"
    );
    assert_eq!(
        value_after(&args, "--input-format"),
        Some(OsStr::new("stream-json")),
        "this is the flag that keeps one process alive across turns; without it every turn \
         pays a cold start and a cache rebuild. argv was {args:?}"
    );
    assert!(
        has_flag(&args, "--verbose"),
        "the CLI refuses the run without it, verbatim: 'Error: When using --print, \
         --output-format=stream-json requires --verbose'. argv was {args:?}"
    );

    // ── Izolacja kontekstu ────────────────────────────────────────────────────────────────
    assert!(
        has_flag(&args, "--strict-mcp-config"),
        "without it the run loads 73 tools from 9 servers and burns 36870 cache-creation \
         tokens instead of 4725. Nothing breaks; it is just more expensive on every step, \
         forever. argv was {args:?}"
    );
    let sources = value_after(&args, "--setting-sources")
        .ok_or("--setting-sources was passed with nothing after it")?;
    assert!(
        sources.is_empty(),
        "--setting-sources must carry a zero-length argument. It was {sources:?}, and any \
         non-empty value (say 'user,project') is context isolation that does not isolate: \
         the flag is present, the check that only asks for presence is green, and the run \
         still loads everything from the user's own settings"
    );

    Ok(())
}

#[test]
fn a_fresh_run_pins_the_session_and_a_resumed_one_names_it() -> Result<(), Box<dyn Error>> {
    let fresh = spec(None);
    let command = ClaudeDriver::new().command(&fresh);
    let args: Vec<&OsStr> = command.as_std().get_args().collect();

    // Sesję nadajemy MY, przed startem procesu: dopiero to czyni odzyskiwanie możliwym,
    // bo krok ma numer, zanim przyjdzie pierwsze zdarzenie [T7 §6.2].
    let minted = fresh.run_id.to_string();
    assert_eq!(
        value_after(&args, "--session-id"),
        Some(OsStr::new(&minted)),
        "a fresh run has to pre-assign its own session id. argv was {args:?}"
    );
    assert!(
        !has_flag(&args, "--resume"),
        "a fresh run must not also ask to resume something; the two flags name two different \
         sessions and the CLI would have to pick one. argv was {args:?}"
    );

    let again = spec(Some(SessionRef {
        vendor: VENDOR,
        id: RESUMED.to_owned(),
    }));
    let command = ClaudeDriver::new().command(&again);
    let args: Vec<&OsStr> = command.as_std().get_args().collect();

    assert_eq!(
        value_after(&args, "--resume"),
        Some(OsStr::new(RESUMED)),
        "a resumed run has to name the session it is continuing. argv was {args:?}"
    );
    assert!(
        !has_flag(&args, "--session-id"),
        "a resumed run must not also pre-assign a session id: that is the run_id of this step, \
         not the session being continued, and passing both makes which one wins a coin flip. \
         argv was {args:?}"
    );

    Ok(())
}

#[test]
fn the_prompt_and_the_three_forbidden_flags_never_reach_argv() -> Result<(), Box<dyn Error>> {
    let fresh = spec(None);
    let command = ClaudeDriver::new().command(&fresh);
    let args: Vec<&OsStr> = command.as_std().get_args().collect();

    // ── Prompt ────────────────────────────────────────────────────────────────────────────
    // Nie „nie ma argumentu równego promptowi", tylko „znacznik nie jest PODCIĄGIEM żadnego
    // argumentu": treść zadania wklejona do --append-system-prompt jest tym samym wyciekiem,
    // tylko trudniej ją zobaczyć.
    let leaked: Vec<&OsStr> = args
        .iter()
        .filter(|arg| arg.to_string_lossy().contains(PROMPT_MARK))
        .copied()
        .collect();
    assert!(
        leaked.is_empty(),
        "the prompt reached argv, and argv is readable by every user on this machine via ps. \
         It travels on stdin, in a user envelope, and nowhere else. Leaking arguments: {leaked:?}"
    );

    // ── Trzy flagi, których tu nie ma ─────────────────────────────────────────────────────
    assert!(
        !has_flag(&args, "--bare"),
        "--bare is the vendor's own recommendation for scripted use and it never reads OAuth \
         or the keychain: on this machine it failed with 'Not logged in - Please run /login' \
         and terminal_reason api_error. A subscription user cannot run with it. argv was {args:?}"
    );
    assert!(
        !has_flag(&args, "--max-turns"),
        "spike S-2 has not settled whether this flag exists at all (T1 says yes by probe, T4 \
         says no by --help), so nothing is built on it. The wall-clock limit from T-03 is what \
         the user actually means by 'do not grind forever'. argv was {args:?}"
    );
    assert!(
        !has_flag(&args, "--max-budget-usd"),
        "same spike, same answer: a ceiling nobody has verified is a ceiling nobody can trust. \
         argv was {args:?}"
    );

    Ok(())
}
