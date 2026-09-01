//! Flaga wysilku nie idzie do CLI, ktore jej nie zna.
//!
//! WADA, zmierzona 2026-09-01 na maszynie wlasciciela. Kazdy krok Claude'a padal natychmiast:
//!
//!   The agent stopped without ever sending its result. error: unknown option --effort
//!
//! `--effort` idzie ZAWSZE: „ile myslec" ma domyslnie `balanced`, a `effort_level` tlumaczy je
//! na `medium`, wiec nie ma ustawienia, przy ktorym flaga by nie poszla. Jedna flaga, ktorej
//! starsza binarka nie zna, zabija KAZDY krok tego vendora — a czlowiek widzi tylko „agent sie
//! poddal", bo nikt mu nie mowi, ktora binarke aplikacja wzięła ani co jej podala.
//!
//! DLACZEGO POMIJAMY, A NIE PADAMY. Bieg bez flagi dziala na domyslnym wysilku vendora; bieg
//! z flaga nie dziala wcale. Dzialajacy krok na domyslnym poziomie bije martwy krok.
//!
//! DLACZEGO POMIJAMY TYLKO NA DOWOD. Sonda, ktora nie wystartowala, nie jest dowodem, ze flagi
//! nie ma — jest dowodem, ze nic nie wiemy. Wtedy zachowanie zostaje takie, jak bylo, bo
//! zgadywanie „pewnie nie ma" odebraloby wysilek kazdemu, kto ma binarke w nietypowym miejscu.
use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use loadout_lib::engine::drivers::AgentDriver;
use loadout_lib::engine::drivers::claude::ClaudeDriver;

/// Atrapa CLI: `--help` wypisuje to, co jej kazemy, i konczy sie zerem.
fn stub(directory: &Path, name: &str, help: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = directory.join(name);
    fs::write(&path, format!("#!/bin/sh\ncat <<'HELP'\n{help}\nHELP\n"))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

#[test]
fn a_cli_without_the_flag_gets_no_effort_argument() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let binary = stub(
        home.path(),
        "claude-old",
        "Usage: claude [options]\n  --model <model>\n  --verbose",
    )?;

    let argv = ClaudeDriver::with_binary(binary).effort_argv("high");

    assert!(
        argv.is_empty(),
        "the driver handed --effort to a CLI whose own help does not list it. That binary \
         answers with `error: unknown option --effort` and exits before the first turn, so \
         every step of this vendor dies. Argv was {argv:?}"
    );
    Ok(())
}

#[test]
fn a_cli_that_knows_the_flag_still_gets_it() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let binary = stub(
        home.path(),
        "claude-new",
        "Usage: claude [options]\n  --effort <level>   Effort level for the current session\n  --verbose",
    )?;

    let argv = ClaudeDriver::with_binary(binary).effort_argv("high");

    assert_eq!(
        argv,
        vec!["--effort".to_owned(), "high".to_owned()],
        "the driver dropped the effort level for a CLI that advertises the flag. Then a person \
         who chose the deepest thinking silently runs at the vendor default, and nothing on the \
         screen says so."
    );
    Ok(())
}

#[test]
fn a_binary_that_cannot_be_asked_keeps_the_flag() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let missing = home.path().join("not-installed");

    let argv = ClaudeDriver::with_binary(missing).effort_argv("low");

    assert_eq!(
        argv,
        vec!["--effort".to_owned(), "low".to_owned()],
        "a probe that never ran was treated as proof the flag is absent. It is proof of \
         nothing, and guessing 'probably missing' quietly takes the effort level away from \
         everyone whose binary sits somewhere we could not ask."
    );
    Ok(())
}
