//! Druga tura Codeksa może ruszyć dopiero nad dowodem `ESRCH` dla grupy poprzedniej.
//!
//! `codex exec` ma osobny proces na turę, ale lider nie jest całą grupą. Atrapa zostawia po
//! liderze prawdziwego wnuka z przekierowanymi potokami: `wait()` umie zebrać lidera, lecz nie
//! widzi wnuka, a `send()` nie może przez to uznać starego uchwytu za bezpieczny do podmiany
//! (niezmiennik 6).

use std::error::Error;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::{AgentHandle, Policy, RunSpec};
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

const LIMIT: Duration = Duration::from_secs(8);
const SECOND: &str = "continue only after the previous process group is gone";

/// Potoki wnuka idą do `/dev/null`, bo inaczej samo opróżnianie wyjścia pierwszej tury
/// czekałoby na jego koniec i test mierzyłby potok zamiast grupy (2026-09, niezmiennik 20).
const LEAVES_A_DESCENDANT: &str = r#"#!/bin/sh
sh -c 'sleep 3' >/dev/null 2>&1 &
printf '{"type":"thread.started","thread_id":"thread-z5"}\n'
printf '{"type":"turn.completed","usage":{"input_tokens":1,"cached_input_tokens":0,"output_tokens":1}}\n'
exit 0
"#;

fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn spec(cwd: &Path) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: "finish the first turn".to_owned(),
        model: Some("gpt-5-codex".to_owned()),
        system_append: None,
        reaches_the_web: false,
        policy: Policy::ReadOnly,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Pyta jądro o całą grupę bez dostarczania sygnału; tylko `ESRCH` jest dowodem śmierci.
#[allow(unsafe_code)]
fn group_probe(pgid: i32) -> io::Result<()> {
    // SAFETY: sygnał zero nie jest dostarczany, a argument nie zawiera wskaźników.
    let rc = unsafe { libc::kill(-pgid, 0) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Sprząta wyłącznie grupę atrapy, gdy stary kod pozwolił jej przeżyć start drugiej tury.
#[allow(unsafe_code)]
fn clean_regression_group(pgid: i32) {
    // SAFETY: ujemny PGID pochodzi wprost z uruchomionej wyżej atrapy i nie może wskazać
    // procesu spoza jej grupy. Ta droga istnieje tylko po czerwonej obserwacji testu.
    let _ = unsafe { libc::kill(-pgid, libc::SIGKILL) };
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_turn_starts_only_after_the_first_group_is_gone() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let binary = write_script(dir.path(), "codex", LEAVES_A_DESCENDANT)?;
    let (tx, _rx) = mpsc::channel(16);
    let driver = CodexDriver::with_binary(binary);
    let mut handle = timeout(LIMIT, driver.start_session(spec(dir.path()), tx)).await??;

    let _first_outcome = timeout(LIMIT, handle.wait()).await??;
    let first = handle
        .group()
        .ok_or("the completed first turn lost the address of its process group")?;
    assert!(
        group_probe(first.pgid).is_ok(),
        "the fixture is dishonest: kill(-{}, 0) cannot find the descendant after the first \
         leader exited, so ESRCH after send() would prove nothing",
        first.pgid
    );

    timeout(LIMIT, handle.send(SECOND.to_owned())).await??;
    let after_send = group_probe(first.pgid)
        .err()
        .and_then(|error| error.raw_os_error());
    let second = handle
        .group()
        .ok_or("the follow-up returned without installing its own process group")?;

    // Obie grupy sprzątamy przed asercją regresyjną, żeby czerwona wersja testu nie zostawiała
    // trzysekundowego agenta po zakończeniu procesu testowego (2026-09, niezmiennik 6).
    let _second_proof = timeout(LIMIT, handle.cancel()).await?;
    if after_send != Some(libc::ESRCH) {
        clean_regression_group(first.pgid);
    }

    assert_eq!(
        after_send,
        Some(libc::ESRCH),
        "the second turn started while kill(-{}, 0) still found a descendant of the first one; \
         collecting only the leader is not proof that its group is gone (invariant 6)",
        first.pgid
    );
    assert_ne!(
        second.pgid, first.pgid,
        "the repair must start the requested second turn after proving the first group dead, not \
         refuse every follow-up"
    );

    Ok(())
}
