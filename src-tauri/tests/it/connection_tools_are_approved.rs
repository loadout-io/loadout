//! Narzędzia zatwierdzonego połączenia idą bez pytania — a czasowniki plikowe dalej wybiera dial.
//!
//! # Co to mierzy
//!
//! 2026-08-22, bieg właściciela na `urc-monorepo`. Serwer `figma` zameldował się w linii `init`
//! jako `{"name":"figma","status":"connected"}`, CLI zarejestrowało **32** jego narzędzia, agent
//! zawołał `get_design_context` — i dostał `permission_denied`. Powód: `--allowedTools` niosło
//! wyłącznie czasowniki plikowe z dialu, a `--permission-mode dontAsk` odrzuca resztę **bez
//! pytania**. Bieg kosztował 20 minut i padł na kroku, który nie miał czego sprawdzić.
//!
//! Dial odpowiada na pytanie „co agent może zrobić z PLIKAMI". `mcp__figma__get_design_context`
//! nie jest czasownikiem plikowym; o tym, czy wolno go użyć, człowiek zdecydował, WŁĄCZAJĄC
//! połączenie w imporcie. Połączenie, które się łączy i którego nie wolno użyć, jest kontrolką
//! bez skutku (niezmiennik 16).
//!
//! # Słabą wersją tego kryterium jest sprawdzenie, że lista urosła
//!
//! Przechodzi ją implementacja, która przy okazji dokłada `Bash` albo zamienia zakres serwera na
//! `mcp__*`. Dlatego niżej sądzone są OBIE strony: że wzorzec serwera jest, i że ani jeden
//! czasownik plikowy spoza dialu się nie przemycił.

use std::error::Error;
use std::ffi::OsStr;
use std::path::PathBuf;

use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::{DriverConfiguration, Policy, RunSpec};
use uuid::Uuid;

fn spec(policy: Policy) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: PathBuf::from("."),
        prompt: "read the design".to_owned(),
        model: None,
        system_append: None,
        policy,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

fn value_after<'a>(args: &[&'a OsStr], flag: &str) -> Option<&'a OsStr> {
    let at = args.iter().position(|arg| *arg == OsStr::new(flag))?;
    args.get(at + 1).copied()
}

/// Lista auto-zatwierdzania kroku, który dostał te serwery.
fn approved(policy: Policy, servers: &[&str]) -> Result<String, Box<dyn Error>> {
    let driver = ClaudeDriver::new().with_configuration(DriverConfiguration {
        arguments: vec!["--mcp-config".to_owned(), "/tmp/claude-mcp.json".to_owned()],
        environment: Vec::new(),
        servers: servers.iter().map(|one| (*one).to_owned()).collect(),
    });
    let command = driver.command(&spec(policy));
    let args: Vec<&OsStr> = command.as_std().get_args().collect();
    Ok(value_after(&args, "--allowedTools")
        .ok_or("--allowedTools is missing")?
        .to_string_lossy()
        .into_owned())
}

#[test]
fn a_step_that_got_a_connection_may_use_it_without_asking() -> Result<(), Box<dyn Error>> {
    let said = approved(Policy::ReadOnly, &["figma"])?;

    assert!(
        said.split(',').any(|one| one == "mcp__figma"),
        "the person switched this connection on; that IS the approval, and there is no other one \
         for a tool server. Without this entry the server connects, registers its tools, and \
         every call comes back denied. Got: {said}"
    );
    Ok(())
}

#[test]
fn the_dial_still_decides_every_file_verb() -> Result<(), Box<dyn Error>> {
    let said = approved(Policy::ReadOnly, &["figma"])?;

    for forbidden in ["Bash", "Write", "Edit"] {
        assert!(
            !said.split(',').any(|one| one.starts_with(forbidden)),
            "'look only' promises the person that this agent does not change files, and a \
             connection says nothing about files. '{forbidden}' arriving through this door would \
             make the tool list a second road to permissions beside the dial. Got: {said}"
        );
    }
    Ok(())
}

#[test]
fn every_server_the_step_got_is_named_and_no_others() -> Result<(), Box<dyn Error>> {
    let said = approved(Policy::EditInFolder, &["figma", "playwright"])?;
    let servers: Vec<&str> = said
        .split(',')
        .filter(|one| one.starts_with("mcp__"))
        .collect();

    assert_eq!(
        servers,
        vec!["mcp__figma", "mcp__playwright"],
        "two connections are two entries — and a wildcard over every server would approve one the \
         person never switched on, which is the opposite of what the import promises"
    );
    Ok(())
}

#[test]
fn a_step_with_no_connection_gets_exactly_what_it_got_before() -> Result<(), Box<dyn Error>> {
    let said = approved(Policy::ReadOnly, &[])?;

    assert!(
        !said.contains("mcp__"),
        "a step with no connections has nothing to approve, and an empty entry here would be a \
         permission granted to a server that does not exist. Got: {said}"
    );
    Ok(())
}
