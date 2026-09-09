//! 2026-09-09: izolacja danych okna nie może wylogowywać jego agentów przez podmianę HOME.

use std::error::Error;
use std::path::PathBuf;
use std::process::Command;

/// Osobny proces chroni równoległe testy przed zmianą środowiska. Ten sam resolver jest
/// wywoływany przez produkcyjny start okna dla biblioteki, SQLite oraz dziennika.
#[test]
fn an_isolated_library_keeps_the_agent_login_home() -> Result<(), Box<dyn Error>> {
    const CHILD: &str = "LOADOUT_DATA_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let expected =
            PathBuf::from(std::env::var_os("LOADOUT_DATA_DIR").ok_or("missing data dir")?);
        assert_eq!(loadout_lib::loadout_dir(), expected);
        assert_eq!(
            std::env::var_os("HOME"),
            std::env::var_os("LOADOUT_DATA_TEST_LOGIN_HOME"),
            "isolating the library must preserve the HOME used by both agent apps"
        );
        // Dane naprawdę trafiają pod adres, z którego korzysta uruchomione okno.
        let set = loadout_lib::commands::context::create_context_set_inner(
            &loadout_lib::loadout_dir(),
            "Isolated context",
        )?;
        let folder = loadout_lib::context::files::folder_of(
            &loadout_lib::context::files::library_root(&expected),
            &set.set.id,
        )?;
        assert!(folder.starts_with(&expected) && folder.join("manifest.json").is_file());
        return Ok(());
    }
    let data = tempfile::tempdir()?;
    let login_home = std::env::var_os("HOME").ok_or("the fixture needs the person's HOME")?;
    let result = Command::new(std::env::current_exe()?)
        .args([
            "isolated_app_data::an_isolated_library_keeps_the_agent_login_home",
            "--exact",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .env("LOADOUT_DATA_DIR", data.path())
        .env("LOADOUT_DATA_TEST_LOGIN_HOME", login_home)
        .output()?;
    assert!(
        result.status.success(),
        "isolated app data: {}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(())
}

#[test]
fn the_default_library_stays_in_the_persons_home() -> Result<(), Box<dyn Error>> {
    const CHILD: &str = "LOADOUT_DEFAULT_DATA_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let home = PathBuf::from(std::env::var_os("HOME").ok_or("missing HOME")?);
        assert_eq!(loadout_lib::loadout_dir(), home.join(".loadout"));
        return Ok(());
    }
    for empty in [false, true] {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args([
                "isolated_app_data::the_default_library_stays_in_the_persons_home",
                "--exact",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env_remove("LOADOUT_DATA_DIR");
        if empty {
            command.env("LOADOUT_DATA_DIR", "");
        }
        let result = command.output()?;
        assert!(
            result.status.success(),
            "default library: {}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    Ok(())
}
