//! AC-4 dla T-03: po zabiciu grupy strumień wyjścia dochodzi do EOF.
//!
//! To jest efekt drugiego rzędu wycieku z T7 §3.1 i ten, który realnie wiesza silnik: sieroty
//! **dziedziczą stdout**, więc potok nigdy nie dochodzi do EOF. `lsof` pokazał obie sieroty
//! trzymające fd 1 i fd 2 na tym samym potoku [T7 §3.1, 2026-08-15]. „Czytaj do EOF" przeciwko
//! wyciekłej grupie to nie wyciek, tylko nieskończone oczekiwanie — a czytelnikiem będzie T-05.
//!
//! Słaba wersja tego kryterium to asercja na statusie wyjścia rodzica. Nie mówi o potoku nic:
//! rodzic wychodzi tu natychmiast i **z sukcesem**, a potok trzyma wnuk. Rozróżnia je dopiero
//! **para** ograniczonych czasowo odczytów w jednym teście: ten przed `stop()`, który musi się
//! nie udać, i ten po, który musi się udać.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use loadout_lib::engine::supervisor::{self, GroupProof, StdinPlan};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Ile czasu dajemy każdemu z dwóch odczytów do EOF. Ten sam próg dla obu, bo cała różnica
/// między nimi ma leżeć w tym, czy grupa jeszcze żyje.
const EOF_LIMIT: Duration = Duration::from_secs(2);

/// Okno łaski dla `stop()` w tym teście.
const GRACE: Duration = Duration::from_secs(2);

/// Rodzic: odpala wnuka i **wychodzi natychmiast**, zostawiając potok w rękach wnuka.
const PARENT: &str = r#"#!/bin/sh
# $1 = skrypt wnuka, $2 = znacznik
"$1" "$2" &
exit 0
"#;

/// Wnuk: dziedziczy stdout, pisze jedną linię i śpi 30 s, nie zamykając deskryptora.
const GRANDCHILD: &str = r#"#!/bin/sh
# $1 = znacznik; linia idzie na odziedziczony stdout
echo "grandchild speaking: $1"
sleep 30
exit 0
"#;

/// Znacznik unikalny dla tego biegu.
fn unique_marker(tag: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("loadout-t03-{tag}-{}-{nanos}", std::process::id())
}

/// Zapisuje wykonywalny skrypt `#!/bin/sh` i zwraca jego ścieżkę [T7 §8.2].
fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "odpala prawdziwe procesy; bramka woła to z --include-ignored"]
async fn the_pipe_reaches_eof_only_once_the_whole_group_is_dead() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let marker = unique_marker("pipe-eof");
    let grandchild = write_script(dir.path(), "grandchild.sh", GRANDCHILD)?;
    let parent = write_script(dir.path(), "parent.sh", PARENT)?;

    let mut command = Command::new(&parent);
    command.arg(&grandchild).arg(&marker);

    let mut handle = supervisor::spawn(command, StdinPlan::Null)?;
    // T-05 czyta dokładnie ten uchwyt, więc bez niego nie ma na czym mierzyć EOF.
    let mut stdout = handle
        .stdout()
        .ok_or("spawn() handed out no piped stdout")?;

    // ── Przed zatrzymaniem: potok NIE dochodzi do EOF ─────────────────────────────────────
    // `read_to_end` dopisuje do bufora w miarę czytania, więc porzucenie future'a po upływie
    // limitu zostawia w `seen` to, co zdążyło przyjść — linię wnuka czytamy z tego samego
    // bufora po `stop()`.
    let mut seen = Vec::new();
    let first = tokio::time::timeout(EOF_LIMIT, stdout.read_to_end(&mut seen)).await;
    assert!(
        first.is_err(),
        "stdout reached EOF while the grandchild was still alive and still holding fd 1. Then \
         this test is not reproducing T7 §3.1 at all, and the assertion after stop() would pass \
         for free"
    );

    // ── Zatrzymanie grupy ─────────────────────────────────────────────────────────────────
    let proof = tokio::time::timeout(Duration::from_secs(10), handle.stop(GRACE))
        .await
        .map_err(|_| "stop() did not return within 10s")?;
    assert!(
        matches!(proof, GroupProof::Dead { .. }),
        "stop() has to prove the group is gone before EOF means anything; it returned {proof:?}"
    );

    // ── Po zatrzymaniu: EOF przychodzi, i to szybko ───────────────────────────────────────
    let began = Instant::now();
    let second = tokio::time::timeout(EOF_LIMIT, stdout.read_to_end(&mut seen)).await;
    let closing = began.elapsed();
    assert!(
        matches!(second, Ok(Ok(_))),
        "stdout had still not reached EOF {EOF_LIMIT:?} after stop() reported the group dead. \
         Somebody in that group still holds the write end — the hang lsof caught in T7 §3.1, \
         which is an infinite wait rather than a leak you can ignore. Read ended as {second:?}"
    );
    assert!(
        closing < EOF_LIMIT,
        "EOF arrived after {closing:?}, which is not the prompt close a reader can rely on"
    );

    let text = String::from_utf8_lossy(&seen);
    assert!(
        text.contains(&marker),
        "the line the grandchild wrote before it was killed did not survive: EOF that costs us \
         the output is a different bug wearing the same green. stdout held {text:?}"
    );

    Ok(())
}
