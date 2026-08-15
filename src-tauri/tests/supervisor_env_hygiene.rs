//! AC-5 dla T-03: dziecko dostaje wyczyszczone środowisko z jawnej listy i puste stdin —
//! a nie to, co odziedziczyło.
//!
//! Słaba wersja tego kryterium to `assert!(!format!("{cmd:?}").contains(secret))` albo przegląd
//! `cmd.get_envs()`. Obie certyfikują zero: `get_envs()` zwraca **wyłącznie zmienne ustawione
//! jawnie** i nie mówi ani słowa o tym, czy `env_clear()` w ogóle padło, a środowisko
//! odziedziczone nie pojawia się w `Debug`. To jest niezmiennik 20 w czystej postaci — test
//! czyta reprezentację zamiast zachowania.
//!
//! Rozróżnia je odczyt **środowiska widzianego przez dziecko, wypisanego przez samo dziecko**.
//!
//! Obie części mieszkają w JEDNEJ funkcji testowej celowo: `std::env::set_var` jest w edycji
//! 2024 `unsafe` właśnie dlatego, że zmienia środowisko całego procesu, a harness testowy Rusta
//! biegnie po wątkach. Dwie funkcje testowe w tym pliku to zapis środowiska równolegle z jego
//! odczytem w drugim wątku — czyli wyścig w teście, który ma pilnować higieny.

use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use loadout_lib::engine::supervisor::{self, PASSTHROUGH, StdinPlan};
use tokio::process::Command;

/// Nazwa, której w środowisku dziecka być nie może. Sekret nazwany wprost, żeby test mógł
/// szukać zarówno nazwy, jak i wartości: wyciek jednego bez drugiego to nadal wyciek.
const SECRET_NAME: &str = "LOADOUT_SECRET_MARKER";

/// Zmienne, które dokłada **sama powłoka** już po starcie, więc nie ma ich w `PASSTHROUGH`
/// i nie są dowodem na nieszczelność.
const SHELL_ADDS: [&str; 3] = ["PWD", "SHLVL", "_"];

/// Dziecko wypisuje swoje środowisko do pliku. Nie na stdout: chcemy je czytać po zakończeniu
/// procesu, bez wchodzenia w drogę potokowi, który ma osobne kryterium (AC-4).
const ENV_SCRIPT: &str = r#"#!/bin/sh
# $1 = plik, do którego ma trafić środowisko widziane przez dziecko
printenv > "$1"
exit 0
"#;

/// Dziecko czyta stdin do EOF i zapisuje, co dostało.
const STDIN_SCRIPT: &str = r#"#!/bin/sh
# $1 = plik, do którego ma trafić to, co przyszło na stdin
cat > "$1"
exit 0
"#;

/// Wartość unikalna dla tego biegu — inaczej „nie ma sekretu w pliku" mogłoby być prawdą
/// dlatego, że szukamy stałej, której nikt nigdy nie ustawił.
fn unique_secret() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("loadout-t03-secret-{}-{nanos}", std::process::id())
}

/// Zapisuje wykonywalny skrypt `#!/bin/sh` i zwraca jego ścieżkę [T7 §8.2].
fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// Sadzi sekret w środowisku **tego** procesu, żeby dziecko miało co odziedziczyć, gdyby
/// `env_clear()` nie padło. Zasadzone naruszenie zamiast szukania stringa — niezmiennik 20.
// 2026-08-15 — w edycji 2024 `set_var` jest unsafe, bo dotyka całego procesu. Ten plik ma
// dokładnie JEDNĄ funkcję testową właśnie po to, żeby nie było drugiego wątku, który
// równolegle czyta środowisko.
#[allow(unsafe_code)]
fn plant_secret(name: &str, value: &str) {
    // SAFETY: w tym pliku jest jeden test, a wywołanie stoi przed jakimkolwiek uruchomieniem
    // procesu potomnego, więc nie ma tu współbieżnego czytelnika środowiska.
    unsafe { std::env::set_var(name, value) };
}

/// Część pierwsza: co dziecko widzi w swoim środowisku.
async fn the_environment_is_scrubbed(dir: &Path, secret: &str) -> Result<(), Box<dyn Error>> {
    let dumped_to = dir.join("child-environment.txt");
    let script = write_script(dir, "printenv.sh", ENV_SCRIPT)?;

    let mut command = Command::new(&script);
    command.arg(&dumped_to);

    let mut handle = supervisor::spawn(command, StdinPlan::Null)?;
    let status = tokio::time::timeout(Duration::from_secs(5), handle.wait())
        .await
        .map_err(|_| "the child never finished writing its environment out")??;
    assert!(
        status.success(),
        "the child failed before it could report its environment: {status:?}"
    );

    let dumped = fs::read_to_string(&dumped_to)?;

    assert!(
        !dumped.contains(SECRET_NAME),
        "{SECRET_NAME} reached the child. env_clear() either never ran or ran after the \
         inherited environment was copied — and this is how secret scanning quietly died in \
         the source repo [raport 05 §4]"
    );
    assert!(
        !dumped.contains(secret),
        "the secret's VALUE reached the child under some other name, which is the same leak \
         wearing a different label"
    );
    assert!(
        dumped.lines().any(|line| line.starts_with("PATH=")),
        "PATH did not reach the child, so the passthrough list is not a list but a wall: \
         without it the shell cannot find node, and every agent step fails for a reason that \
         has nothing to do with the agent"
    );

    let allowed: HashSet<&str> = PASSTHROUGH.iter().copied().chain(SHELL_ADDS).collect();
    let leaked: Vec<&str> = dumped
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(name, _)| name)
        .filter(|name| !allowed.contains(name))
        .collect();
    assert!(
        leaked.is_empty(),
        "the child's environment holds names that are neither in PASSTHROUGH nor added by the \
         shell itself: {leaked:?}. Policy lives in one constant on purpose (invariant 23) — a \
         name that arrives without passing through it arrived by inheritance"
    );

    Ok(())
}

/// Część druga: stdin zamknięte natychmiast, a nie „za trzy sekundy".
async fn stdin_closes_at_once(dir: &Path) -> Result<(), Box<dyn Error>> {
    let received = dir.join("what-came-in-on-stdin.txt");
    let script = write_script(dir, "cat.sh", STDIN_SCRIPT)?;

    let mut command = Command::new(&script);
    command.arg(&received);

    // Sekunda to cały budżet: bez `/dev/null` claude czeka ~3 s i wypisuje
    // `Warning: no stdin data received in 3s…` [T1 §4.6]; przy czterech agentach to dwanaście
    // sekund niczego, na każdym kroku każdego biegu.
    let mut handle = supervisor::spawn(command, StdinPlan::Null)?;
    let status = tokio::time::timeout(Duration::from_secs(1), handle.wait())
        .await
        .map_err(|_| "`cat` never got EOF, so StdinPlan::Null is not /dev/null")??;
    assert!(
        status.success(),
        "the child that reads stdin ended badly: {status:?}"
    );

    let piped = fs::read(&received)?;
    assert!(
        piped.is_empty(),
        "StdinPlan::Null still delivered {} byte(s) to the child; an empty plan that writes \
         anything is a plan that can write a secret",
        piped.len()
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "odpala prawdziwe procesy; bramka woła to z --include-ignored"]
async fn the_child_gets_a_scrubbed_environment_and_an_immediately_closed_stdin()
-> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let secret = unique_secret();
    plant_secret(SECRET_NAME, &secret);

    the_environment_is_scrubbed(dir.path(), &secret).await?;
    stdin_closes_at_once(dir.path()).await?;

    Ok(())
}
