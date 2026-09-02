//! Z-2: sonda grupy w oknie łaski nie może dostarczać kolejnych SIGTERM-ów.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use loadout_lib::engine::supervisor::{self, GroupProof, StdinPlan};
use tokio::process::Command;

const GRACE: Duration = Duration::from_secs(2);

const LEADER: &str = r#"#!/bin/sh
# $1 = skrypt liczący, $2 = plik licznika, $3 = plik gotowości
"$1" "$2" "$3" &
exit 0
"#;

const COUNTER: &str = r#"#!/bin/sh
# $1 = plik licznika, $2 = plik gotowości
COUNT_FILE="$1"
n=0
trap 'n=$((n+1)); echo "$n" > "$COUNT_FILE"' TERM
: > "$2"
i=0
while [ "$i" -lt 100 ]; do
  sleep 0.01
  i=$((i+1))
done
exit 0
"#;

fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

async fn wait_for_ready(path: &Path, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_grace_window_delivers_exactly_one_sigterm() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let count_file = dir.path().join("term-count");
    let ready_file = dir.path().join("trap-installed");
    let counter = write_script(dir.path(), "counter.sh", COUNTER)?;
    let leader = write_script(dir.path(), "leader.sh", LEADER)?;

    let mut command = Command::new(&leader);
    command.arg(&counter).arg(&count_file).arg(&ready_file);

    let mut handle = supervisor::spawn(command, StdinPlan::Null)?;
    assert!(
        wait_for_ready(&ready_file, Duration::from_secs(5)).await,
        "the counter never reported that its TERM trap was installed, so this run cannot count delivered signals"
    );

    // 2026-09: lider wychodzi przed `stop()`, żeby całe okno dowodowe mierzyło osieroconego
    // wnuka. Gdyby lider czekał, jego `wait()` zużyłby okno i stary fallback też wysłałby raz.
    let proof = tokio::time::timeout(Duration::from_secs(10), handle.stop(GRACE))
        .await
        .map_err(|_| "stop() did not return within 10s")?;
    assert!(
        matches!(proof, GroupProof::Dead { .. }),
        "stop() must still prove the whole process group dead; it returned {proof:?}"
    );

    let delivered = fs::read_to_string(&count_file)?;
    assert_eq!(
        delivered.trim(),
        "1",
        "the grace window must deliver one leading SIGTERM; probes must not signal the group"
    );

    Ok(())
}
