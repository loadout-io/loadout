//! Preflight lokalnych aplikacji agentów mierzy PROCES, nie sam tekst wersji.
//!
//! Wszystko niżej sądzi to, co robi się z systemem: ile razy program ruszył, z iloma argumentami,
//! co dostał na wejściu, co odziedziczył ze środowiska, kiedy zginął i czy ktokolwiek to udowodnił
//! (niezmiennik 6). Zdanie, które z tego powstaje dla człowieka, sądzą kryteria po drugiej stronie
//! granicy — `src/ui/shell/agent-apps-status.test.tsx` i `e2e/tests/agent-app-preflight-is-visible`
//! (niezmiennik 29).

use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::anyhow;
use async_trait::async_trait;
use loadout_lib::commands::Drivers;
use loadout_lib::commands::agent_apps::check_agent_apps_inner;
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentHandle, DecodedEvent, DriverConfiguration, Probe, RunSpec,
};
use loadout_lib::library::agents::Vendor;
use serde_json::json;
use tokio::sync::mpsc;
use tokio::time::timeout;

/// Sufit CAŁEGO kryterium. Produkcyjny limit to pięć sekund, więc wszystko ponad to jest już
/// zawieszonym sprzątaniem, a nie wolnym programem.
const OUTER_LIMIT: Duration = Duration::from_secs(8);

const VERSION_THEN_FAILURE: &str = r"#!/bin/sh
printf 'claude 9.9.9\n'
exit 7
";

const EMPTY_STDOUT: &str = r"#!/bin/sh
exit 0
";

const LARGE_STDERR: &str = r"#!/bin/sh
dd if=/dev/zero bs=131072 count=1 >&2 2>/dev/null
printf 'codex-cli 8.7.6\n'
exit 0
";

fn executable(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// Program, który zapisuje na dysk wszystko, co dostał: argumenty, wejście i środowisko.
fn successful_fixture(dir: &Path) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf), Box<dyn Error>> {
    let arguments = dir.join("arguments.txt");
    let stdin = dir.join("stdin.txt");
    let environment = dir.join("environment.txt");
    let body = format!(
        r#"#!/bin/sh
printf '%s\n%s\n' "$#" "$1" > '{}'
if IFS= read -r line; then
  printf 'data:%s\n' "$line" > '{}'
else
  printf 'eof\n' > '{}'
fi
printenv > '{}'
printf 'claude-code 3.4.5\nignored second line\n'
exit 0
"#,
        arguments.display(),
        stdin.display(),
        stdin.display(),
        environment.display()
    );
    let binary = executable(dir, "claude", &body)?;
    Ok((binary, arguments, stdin, environment))
}

/// Sterownik, który szuka CLI wyłącznie w tym jednym katalogu.
fn only_this_folder(dir: &Path) -> DriverConfiguration {
    DriverConfiguration {
        environment: vec![(String::from("PATH"), OsString::from(dir))],
        ..DriverConfiguration::default()
    }
}

/// Pyta jądro o grupę bez wysyłania jej sygnału; testy są poza granicą kodu platformowego.
#[allow(unsafe_code)]
fn group_probe(pgid: i32) -> io::Result<()> {
    // SAFETY: sygnał zero niczego nie dostarcza, a argumenty są liczbami bez wskaźników.
    let result = unsafe { libc::kill(-pgid, 0) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[tokio::test]
async fn a_version_from_a_failed_process_is_not_a_success() -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let binary = executable(fixture.path(), "claude", VERSION_THEN_FAILURE)?;

    let result = ClaudeDriver::with_binary(binary).probe().await;

    assert!(
        result.is_err(),
        "stdout carried a plausible version, but the process exited nonzero; accepting it would \
         turn a broken local app into a successful preflight: {result:?}"
    );
    Ok(())
}

#[tokio::test]
async fn the_probe_runs_one_version_with_eof_and_only_path() -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let (binary, arguments, stdin, environment) = successful_fixture(fixture.path())?;

    let result = ClaudeDriver::with_binary(binary).probe().await?;

    assert_eq!(
        result,
        Probe {
            found: true,
            version: Some("claude-code 3.4.5".to_owned()),
        }
    );
    assert_eq!(
        fs::read_to_string(arguments)?.lines().collect::<Vec<_>>(),
        ["1", "--version"],
        "the local app must run exactly once with exactly one argument"
    );
    assert_eq!(
        fs::read_to_string(stdin)?,
        "eof\n",
        "the local app waited for input nobody was going to send; that is three seconds of \
         nothing on every window that opens"
    );

    let environment = fs::read_to_string(environment)?;
    assert!(
        environment.lines().any(|line| line.starts_with("PATH=")),
        "without PATH the version program cannot find the tools it runs itself"
    );
    for forbidden in ["HOME", "LANG", "TERM", "TMPDIR", "USER"] {
        assert!(
            !environment
                .lines()
                .any(|line| line.starts_with(&format!("{forbidden}="))),
            "the version process received {forbidden}, although asking for a version needs only \
             PATH. Everything a turn of an agent inherits is a credential or a home directory it \
             has no business reading here"
        );
    }
    Ok(())
}

/// Nic nie stoi między Loadoutem a lokalną aplikacją — ani `env`, ani powłoka.
///
/// Kryterium jest zbudowane tak, żeby wykryć POŚREDNIKA, a nie żeby o niego zapytać: na `PATH`
/// dziecka stoi wyłącznie katalog tej fikstury, więc program-pośrednik trzeba by najpierw tam
/// znaleźć — a nie ma go tam. Do 2026-09 wspólna sonda uruchamiała `env -i PATH=… <binary>`
/// i przechodziła wszystko powyżej: środowisko wyglądało tak samo, a cudzy kod wyjścia (126, 127)
/// wchodził na miejsce naszego.
#[tokio::test]
async fn the_version_starts_the_local_app_itself_with_nothing_in_between()
-> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let (binary, arguments, _stdin, _environment) = successful_fixture(fixture.path())?;

    let driver =
        ClaudeDriver::with_binary(binary).with_configuration(only_this_folder(fixture.path()));
    let result = driver.probe().await;

    assert_eq!(
        result.as_ref().ok().and_then(|probe| probe.version.clone()),
        Some("claude-code 3.4.5".to_owned()),
        "the local app itself is on PATH and nothing else is, yet the version could not be \
         taken: something between Loadout and that app had to be found first: {result:?}"
    );
    assert_eq!(
        fs::read_to_string(arguments)?.lines().collect::<Vec<_>>(),
        ["1", "--version"],
        "a go-between would show up here as extra arguments or as a different first one"
    );
    Ok(())
}

#[tokio::test]
async fn a_missing_path_is_not_found_but_permission_denied_is_an_error()
-> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let missing = ClaudeDriver::with_binary(fixture.path().join("not-there"))
        .probe()
        .await?;
    assert_eq!(
        missing,
        Probe {
            found: false,
            version: None,
        }
    );

    let denied = fixture.path().join("not-executable");
    fs::write(&denied, "#!/bin/sh\nprintf 'should not run\\n'\n")?;
    fs::set_permissions(&denied, fs::Permissions::from_mode(0o644))?;
    assert!(
        ClaudeDriver::with_binary(denied).probe().await.is_err(),
        "a file that exists but cannot be executed is not the same state as a missing app"
    );
    Ok(())
}

#[tokio::test]
async fn empty_stdout_is_an_error_and_large_stderr_does_not_deadlock() -> Result<(), Box<dyn Error>>
{
    let fixture = tempfile::tempdir()?;
    let empty = executable(fixture.path(), "empty", EMPTY_STDOUT)?;
    assert!(
        ClaudeDriver::with_binary(empty).probe().await.is_err(),
        "a clean exit that said nothing is not a version, and \"Claude Code · \" with nothing \
         after it looks like an answer"
    );

    let noisy = executable(fixture.path(), "codex", LARGE_STDERR)?;
    let result = timeout(OUTER_LIMIT, CodexDriver::with_binary(noisy).probe())
        .await
        .map_err(|_| "the complaints pipe filled and stopped the version program")??;
    assert_eq!(result.version.as_deref(), Some("codex-cli 8.7.6"));
    Ok(())
}

#[tokio::test]
async fn a_hanging_version_is_killed_after_five_seconds_and_proven_gone()
-> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let pid_file = fixture.path().join("pid.txt");
    let body = format!(
        r#"#!/bin/sh
printf '%s\n' "$$" > '{}'
while :; do
  sleep 60
done
"#,
        pid_file.display()
    );
    let binary = executable(fixture.path(), "hanging", &body)?;

    let began = Instant::now();
    let result = timeout(OUTER_LIMIT, ClaudeDriver::with_binary(binary).probe())
        .await
        .map_err(|_| "the production timeout and its cleanup exceeded the eight second oracle")?;
    let elapsed = began.elapsed();
    assert!(
        result.is_err(),
        "a program that never exits cannot be a successful preflight"
    );
    assert!(
        elapsed >= Duration::from_millis(4_500) && elapsed < OUTER_LIMIT,
        "the production limit should expire at about five seconds, not {elapsed:?}"
    );

    let pgid: i32 = fs::read_to_string(pid_file)?.trim().parse()?;
    assert_eq!(
        group_probe(pgid)
            .err()
            .and_then(|error| error.raw_os_error()),
        Some(libc::ESRCH),
        "the timed-out version program returned before its whole group disappeared. An orphan \
         left behind by a window that only wanted a version number burns money in the background \
         (invariant 6)"
    );
    Ok(())
}

#[derive(Clone)]
enum StubAnswer {
    Found(&'static str),
    Error,
}

struct StubDriver {
    id: &'static str,
    answer: StubAnswer,
}

#[async_trait]
impl AgentDriver for StubDriver {
    fn id(&self) -> &'static str {
        self.id
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        match self.answer {
            StubAnswer::Found(version) => Ok(Probe {
                found: true,
                version: Some(version.to_owned()),
            }),
            StubAnswer::Error => Err(anyhow!("fixture failure")),
        }
    }

    async fn start(
        &self,
        _spec: RunSpec,
        _tx: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        Err(anyhow!("this version-only fixture never starts an agent"))
    }
}

#[tokio::test]
async fn the_aggregator_keeps_order_and_isolates_one_failed_probe() -> Result<(), Box<dyn Error>> {
    let claude: Arc<dyn AgentDriver> = Arc::new(StubDriver {
        id: "claude",
        answer: StubAnswer::Error,
    });
    let codex: Arc<dyn AgentDriver> = Arc::new(StubDriver {
        id: "codex",
        answer: StubAnswer::Found("codex-cli 6.5.4"),
    });
    let drivers: Drivers = Arc::new(move |vendor| match vendor {
        Vendor::ClaudeCode => Arc::clone(&claude),
        Vendor::Codex => Arc::clone(&codex),
    });

    let result = check_agent_apps_inner(&drivers).await;

    assert_eq!(
        serde_json::to_value(result)?,
        json!([
            { "app": "claude-code", "state": "could-not-check" },
            { "app": "codex", "state": "found", "version": "codex-cli 6.5.4" }
        ]),
        "one failed measurement must keep its own slot and leave the other result intact"
    );
    Ok(())
}
