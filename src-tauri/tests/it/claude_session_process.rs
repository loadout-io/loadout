//! AC-6 dla T-04: jeden proces obsługuje wiele tur w jednej sesji, a `probe()` odróżnia brak
//! CLI od awarii.
//!
//! **Słaba wersja tego kryterium to „oba wyniki mają ten sam identyfikator sesji".** Spełnia
//! ją wariant awaryjny B z T1 §8.1 — nowy proces na turę z `--resume` — który jest legalnym
//! fallbackiem, ale płaci zimny start i odbudowę cache'u przy **każdej** turze, czyli
//! dokładnie ten koszt, którego to zadanie ma uniknąć. Rozróżnia `pid.log` z **jedną** linią.
//!
//! Dla `probe()` słabą wersją jest `assert!(probe().is_ok())`. Rozróżnia sprawdzenie pola
//! `found` przy ścieżce, której nie ma: brak CLI to ekran ustawień, a nie awaria startu
//! aplikacji — sterownik, który zwraca tam `Err`, wywala Loadouta zanim ktokolwiek zobaczy,
//! co jest do naprawienia.
//!
//! Atrapa loguje **obok siebie** (`"$(dirname "$0")/pid.log"`), nigdy przez zmienną
//! środowiskową: supervisor z T-03 robi `env_clear()` i przepuszcza sześć nazw, więc fikstura
//! sterowana envem po cichu przestałaby działać i test zrobiłby się zielony na niczym.
//!
//! Test odpala prawdziwy proces, więc jest `#[ignore]` i nie biegnie w pętli wewnętrznej.
//! Uruchamia go bramka, linią `check:` z `-- --include-ignored`; bez tej flagi cargo zamelduje
//! `0 passed`, a to nie jest dowód (niezmiennik 19).

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::{AgentDriver, AgentHandle, Policy, RunSpec};
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

/// Sufit na każde pojedyncze oczekiwanie. Regresja ma się objawić jako **czerwony test**,
/// nie jako zawieszenie: bramka zwraca wtedy rc 124, a to jest fałszywa czerwień, nie dowód.
const LIMIT: Duration = Duration::from_secs(10);

/// Ile miejsca ma kanał zdarzeń. Z zapasem: odbiornik w tym teście nic nie wyjmuje, a pełny
/// kanał zatrzymałby sterownik na wysyłce i wyglądałoby to jak zawieszony agent.
const CHANNEL: usize = 256;

/// Atrapa `claude`: jeden proces, jedna sesja, jeden `result` na każdą kopertę użytkownika.
///
/// `--version` odpowiada i wychodzi **przed** dopisaniem do `pid.log`, bo `probe()` woła tę
/// samą binarkę — inaczej licznik uruchomień liczyłby także pytanie o wersję i asercja
/// „dokładnie jedno uruchomienie" mierzyłaby coś innego, niż mówi.
const DUMMY: &str = r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "2.1.233 (Claude Code)"
  exit 0
fi

here="$(dirname "$0")"
echo "$$" >> "$here/pid.log"

session=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--session-id" ]; then
    session="$2"
  fi
  shift
done

printf '{"type":"system","subtype":"init","session_id":"%s","capabilities":["interrupt_receipt_v1","interrupt_cancel_queued_v1","msg_lifecycle_v1"]}\n' "$session"

while IFS= read -r line; do
  printf '%s\n' "$line" >> "$here/stdin.log"
  printf '{"type":"result","subtype":"success","is_error":false,"terminal_reason":"completed","session_id":"%s","num_turns":1,"total_cost_usd":0.001,"usage":{"input_tokens":1,"cache_read_input_tokens":2,"output_tokens":3},"duration_ms":1,"result":"ok"}\n' "$session"
done

exit 0
"#;

/// Zapisuje wykonywalny skrypt i zwraca jego ścieżkę.
///
/// Plik ze skryptem, nigdy `sh -c "…"` i nigdy kopia binarki systemowej: skopiowany plik
/// systemowy dostaje na macOS SIGKILL od podpisu kodu (`Killed: 9`) [T7 §8.2].
fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// Linie pliku, który mógł jeszcze nie powstać. Nieistniejący plik to zero linii, a nie błąd —
/// ale asercja, która na tym stoi, musi o tym wiedzieć.
fn lines_of(path: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(fs::read_to_string(path)?
        .lines()
        .map(String::from)
        .collect())
}

/// `RunSpec` jednej tury.
fn spec(run_id: Uuid, cwd: &Path) -> RunSpec {
    RunSpec {
        run_id,
        cwd: cwd.to_path_buf(),
        prompt: "say what this folder is for".to_owned(),
        model: None,
        system_append: None,
        policy: Policy::ReadOnly,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "odpala prawdziwy proces; bramka woła to z --include-ignored"]
async fn two_turns_share_one_process_and_one_session() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let binary = write_script(dir.path(), "claude", DUMMY)?;

    // Sesję nadajemy MY, zanim cokolwiek wystartuje: dopiero to znosi wyścig o to, pod jakim
    // numerem zapisać krok, i dopiero to czyni odzyskiwanie możliwym [T7 §6.2].
    let run_id = Uuid::now_v7();
    let minted = run_id.to_string();

    // Odbiornik musi przeżyć uchwyt. Kanał bez odbiornika zaczyna odrzucać wysyłki, a wtedy
    // mierzylibyśmy sterownik radzący sobie z naszym błędem, a nie sterownik robiący swoje.
    let (tx, _events) = mpsc::channel(CHANNEL);
    let driver = ClaudeDriver::with_binary(binary);

    let mut handle: Box<dyn AgentHandle> =
        timeout(LIMIT, driver.start(spec(run_id, dir.path()), tx)).await??;

    let first = timeout(LIMIT, handle.wait()).await??;
    timeout(LIMIT, handle.send("and what changed last week".to_owned())).await??;
    let second = timeout(LIMIT, handle.wait()).await??;

    assert_eq!(
        first.session.id, minted,
        "the first turn has to come back under the session we minted before starting"
    );
    assert_eq!(
        second.session.id, minted,
        "the follow-up turn has to stay in the same session; a new id here means the \
         conversation was lost and the agent answered the second question without the first"
    );

    // ── To jest ta asercja, której nie przechodzi wariant „proces na turę" ────────────────
    let starts = lines_of(&dir.path().join("pid.log"))?;
    assert_eq!(
        starts.len(),
        1,
        "two turns have to be served by ONE process. Two lines here means fallback B from \
         T1 section 8.1: a fresh process per turn with --resume. It is a legal fallback and it \
         is also a cold start plus a cache rebuild on every single turn, which is the cost this \
         whole design exists to avoid. pid.log held {starts:?}"
    );

    // ── Koniec sesji, nie koniec tury ─────────────────────────────────────────────────────
    // Bez tego czasownika każdy skończony krok zostawia żywy proces: `claude` z otwartym
    // stdinem czeka w nieskończoność [T1 §2].
    let code = timeout(LIMIT, handle.close()).await??;
    assert_eq!(
        code,
        Some(0),
        "closing stdin is how a session ends cleanly - the CLI sees EOF and exits 0. Anything \
         else means the process had to be signalled, and a signalled session does not append \
         its transcript"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "odpala prawdziwy proces; bramka woła to z --include-ignored"]
async fn a_missing_binary_is_an_answer_not_a_crash() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let binary = write_script(dir.path(), "claude", DUMMY)?;

    let installed = ClaudeDriver::with_binary(binary);
    let present = timeout(LIMIT, installed.probe()).await??;
    assert!(
        present.found,
        "a binary that answers --version is present; it reported {present:?}"
    );
    assert!(
        present.version.is_some(),
        "the version is the number that goes into every bug report, because both vendors add \
         and drop flags weekly. It reported {present:?}"
    );

    let nowhere = dir.path().join("no-such-claude");
    assert!(
        !nowhere.exists(),
        "this path has to really be missing, otherwise the case below measures nothing"
    );
    let uninstalled = ClaudeDriver::with_binary(nowhere);
    let absent = timeout(LIMIT, uninstalled.probe()).await?;

    let absent = absent.map_err(|error| {
        format!(
            "a missing CLI is the setup screen, not a failed application start: returning Err \
             here takes down Loadout before anybody can see what to install. It returned {error}"
        )
    })?;
    assert!(
        !absent.found,
        "the probe has to say the binary is not there, out loud, so the setup screen can say \
         so too. It reported {absent:?}"
    );

    Ok(())
}
