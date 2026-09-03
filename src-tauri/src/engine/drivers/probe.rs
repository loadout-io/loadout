//! Wspólna polityka taniej sondy lokalnych aplikacji agentów.
//!
//! Ta droga uruchamia wyłącznie `<binary> --version`: bez modelu, logowania, sieci i protokołu
//! aplikacji. Oba adaptery podają tu tylko ścieżkę oraz zamrożony `PATH`; cała interpretacja
//! procesu, wyjścia i limitu mieszka w tym jednym miejscu (niezmiennik 23).

use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::anyhow;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::task::JoinHandle;
use tokio::time::{Instant, timeout};

use super::{DriverConfiguration, Probe};
use crate::engine::supervisor::{self, AgentCliSearch, DEFAULT_GRACE, GroupProof, StdinPlan};

/// Sufit samego programu. Sprzątanie grupy zaczyna się dopiero po nim i nie dziedziczy limitu.
const PROGRAM_LIMIT: Duration = Duration::from_secs(5);

/// Zachowujemy tylko początek obu strumieni, ale czytamy je do EOF, żeby pełny potok nie
/// zatrzymał procesu przed `wait()`.
const PREFIX_LIMIT: usize = 8 * 1024;

type Drain = JoinHandle<io::Result<Vec<u8>>>;

fn path_from(configuration: &DriverConfiguration) -> OsString {
    configuration
        .environment
        .iter()
        .rev()
        .find_map(|(name, value)| (name == "PATH").then(|| value.clone()))
        .or_else(|| AgentCliSearch::for_process().child_path())
        .unwrap_or_default()
}

/// Rozstrzyga brak celu przed `env`, bo jego kod 127 nie odróżnia nieobecnego programu od
/// programu, który sam tak zakończył własne `--version`.
fn executable(binary: &Path, path: &OsStr) -> io::Result<Option<PathBuf>> {
    if binary.is_absolute() || binary.components().count() > 1 {
        return match std::fs::metadata(binary) {
            Ok(_) => Ok(Some(binary.to_path_buf())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        };
    }

    for directory in std::env::split_paths(path) {
        let candidate = directory.join(binary);
        match std::fs::metadata(&candidate) {
            Ok(_) => return Ok(Some(candidate)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(None)
}

async fn prefix<R>(mut stream: Option<R>) -> io::Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let Some(ref mut stream) = stream else {
        return Err(io::Error::other(
            "the supervised child exposed no output pipe",
        ));
    };
    let mut kept = Vec::with_capacity(PREFIX_LIMIT);
    let mut chunk = [0_u8; 4096];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Ok(kept);
        }
        let room = PREFIX_LIMIT.saturating_sub(kept.len());
        kept.extend_from_slice(&chunk[..read.min(room)]);
    }
}

async fn drained(stdout: &mut Drain, stderr: &mut Drain) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let (stdout, stderr) = tokio::join!(stdout, stderr);
    let stdout =
        stdout.map_err(|_| anyhow!("the version output reader stopped unexpectedly"))??;
    let stderr = stderr.map_err(|_| anyhow!("the version error reader stopped unexpectedly"))??;
    Ok((stdout, stderr))
}

/// Wraca dopiero po `ESRCH`. Powtórzenie całej supervisorowej eskalacji jest celowe: jej
/// `Alive` jest adresem do dalszego sprzątania, nigdy pozwoleniem na oddanie wyniku sondy.
async fn prove_dead(process: &mut supervisor::Supervised) {
    loop {
        // 2026-08-31 — sprzątanie nie dziedziczy pięciu sekund programu. Skrócenie tej pętli
        // do jednego `stop` oddawałoby `could-not-check`, zostawiając kosztowny proces żywy.
        if matches!(process.stop(DEFAULT_GRACE).await, GroupProof::Dead { .. }) {
            return;
        }
    }
}

fn first_line(stdout: &[u8]) -> anyhow::Result<String> {
    let end = stdout
        .iter()
        .position(|byte| *byte == b'\n')
        .unwrap_or(stdout.len());
    let line = std::str::from_utf8(&stdout[..end])?.trim();
    if line.is_empty() {
        return Err(anyhow!(
            "the version command returned no version on its first line"
        ));
    }
    Ok(line.to_owned())
}

/// Uruchamia lokalny program dokładnie raz i przyjmuje wyłącznie udane `--version`.
pub async fn run(binary: &Path, configuration: &DriverConfiguration) -> anyhow::Result<Probe> {
    let child_path = path_from(configuration);
    let Some(binary) = executable(binary, &child_path)? else {
        return Ok(Probe {
            found: false,
            version: None,
        });
    };

    /* `supervisor::spawn` słusznie przepuszcza sześć nazw potrzebnych pełnej turze agenta.
     * Sonda potrzebuje wyłącznie PATH. `env -i` robi ostatni `exec` w tej samej grupie, więc
     * właściwa binarka nadal dostaje dokładnie jeden argument `--version`, a supervisor nadal
     * posiada TERM → KILL → ESRCH bez kopiowania kodu platformowego poza swój moduł. */
    let mut clean_path = OsString::from("PATH=");
    clean_path.push(&child_path);
    let mut command = Command::new("env");
    command
        .arg("-i")
        .arg(clean_path)
        .arg(binary)
        .arg("--version")
        .env_clear()
        .env("PATH", child_path);

    let mut process = supervisor::spawn(command, StdinPlan::Null)?;
    let mut stdout = tokio::spawn(prefix(process.stdout()));
    let mut stderr = tokio::spawn(prefix(process.stderr()));
    let began = Instant::now();

    let status = match timeout(PROGRAM_LIMIT, process.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(error)) => {
            prove_dead(&mut process).await;
            let _ = drained(&mut stdout, &mut stderr).await;
            return Err(error.into());
        }
        Err(_elapsed) => {
            prove_dead(&mut process).await;
            let _ = drained(&mut stdout, &mut stderr).await;
            return Err(anyhow!(
                "the version command exceeded its five second limit"
            ));
        }
    };

    let remaining = PROGRAM_LIMIT.saturating_sub(began.elapsed());
    let captured = timeout(remaining, drained(&mut stdout, &mut stderr)).await;
    prove_dead(&mut process).await;
    let (stdout, _stderr) = match captured {
        Ok(captured) => captured?,
        Err(_elapsed) => {
            let _ = drained(&mut stdout, &mut stderr).await;
            return Err(anyhow!(
                "the version command exceeded its five second limit"
            ));
        }
    };

    if !status.success() {
        return Err(anyhow!("the version command exited unsuccessfully"));
    }

    Ok(Probe {
        found: true,
        version: Some(first_line(&stdout)?),
    })
}
