//! Z-6: zerwane wyjście diagnostyczne nie zamienia kodu wyjścia w panikę.
//!
//! Z dziennika z 31.08: `failed printing to stderr: Broken pipe`. Tak panikuje `eprintln!` —
//! makro to porzuca `Result` i pada, kiedy zapis się nie uda. Most żyje razem z procesem vendora
//! i kończy się wtedy, kiedy tamten przestaje czytać, więc zerwany drugi koniec jest tam stanem
//! **normalnym**, nie awarią. W release stoi `panic = "abort"` (`Cargo.toml`), więc ta panika
//! zamienia uczciwy kod wyjścia w przerwany proces.
//!
//! Pierwszy test sądzi ZACHOWANIE: uruchamia prawdziwą binarkę z deskryptorem diagnostycznym
//! wpiętym w rurę, której nikt nie czyta, i pyta o kod wyjścia. Drugi jest jego uzupełnieniem,
//! nie zamiennikiem (niezmiennik 20): pilnuje, żeby panikujące makro nie wróciło do `src/`
//! którymkolwiek z pozostałych wywołań — tych, do których test procesu nie sięga, bo prowadzą
//! przez nieudany start runtime'u albo przez okno.

use std::fs;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Czym most odmawia, kiedy po fladze nie ma ścieżki gniazda (`main.rs`).
const NO_SOCKET_PATH: i32 = 2;

/// Czym most odmawia, kiedy gniazda pod wskazaną ścieżką nie da się użyć (`bridge/mod.rs`).
const SOCKET_UNUSABLE: i32 = 1;

/// Makra, które piszą i **panikują**, kiedy zapis się nie uda. Każde z nich niesie tę samą wadę.
const PANICKING_WRITERS: [&str; 4] = ["eprintln!", "eprint!", "println!", "print!"];

/// Wyjście diagnostyczne, którego drugi koniec jest zamknięty, ZANIM proces wstanie.
///
/// Para gniazd, a nie prawdziwa rura z `Command`: przy rurze parent musiałby jeszcze zdążyć ją
/// zamknąć i test miałby wyścig, który raz na jakiś czas zieleni się bez powodu. Tutaj drugi
/// koniec ginie przed `spawn`, więc PIERWSZY zapis potomka nie ma dokąd pójść.
fn a_stderr_nobody_reads() -> anyhow::Result<Stdio> {
    let (writable, reader) = UnixStream::pair()?;
    drop(reader);
    Ok(Stdio::from(OwnedFd::from(writable)))
}

/// Odpala Loadouta z podanymi argumentami i zerwanym wyjściem diagnostycznym.
fn loadout_writing_into_the_void(args: &[&str]) -> anyhow::Result<Option<i32>> {
    let status = Command::new(env!("CARGO_BIN_EXE_loadout"))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(a_stderr_nobody_reads()?)
        .status()?;
    Ok(status.code())
}

/// To samo, ale ktoś po drugiej stronie czyta — i oddaje, co przeczytał.
fn loadout_talking_to_someone(args: &[&str]) -> anyhow::Result<String> {
    let done = Command::new(env!("CARGO_BIN_EXE_loadout"))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()?;
    Ok(String::from_utf8_lossy(&done.stderr).into_owned())
}

/// Wszystkie pliki `.rs` pod `dir`.
fn rust_files_under(dir: &Path, found: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            rust_files_under(&path, found)?;
        } else if path.extension().is_some_and(|end| end == "rs") {
            found.push(path);
        }
    }
    Ok(())
}

/// Czy ta linia jest kodem, czy komentarzem.
///
/// Wywołanie w komentarzu jest tu spodziewane i pożądane: przy każdym z tych zapisów stoi
/// zdanie, które NAZYWA porzucone makro i mówi, czemu go tam nie ma (niezmiennik 24). Strażnik,
/// który zapala się na własnym uzasadnieniu, uczyłby kasowania uzasadnień.
fn is_code(line: &str) -> bool {
    let trimmed = line.trim_start();
    !(trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*'))
}

/// Czy ta linia woła DOKŁADNIE to makro.
///
/// `eprintln!` niesie w sobie `println!` co do znaku, więc gołe `contains` melduje jedną linię
/// dwa razy i za drugim razem nazywa wywołanie, którego tam nie ma. Znak przed dopasowaniem musi
/// więc być czymś, co do nazwy nie należy.
fn names_the_macro(line: &str, macro_name: &str) -> bool {
    line.match_indices(macro_name).any(|(at, _)| {
        line[..at]
            .chars()
            .next_back()
            .is_none_or(|before| !before.is_alphanumeric() && before != '_')
    })
}

#[test]
fn a_broken_stderr_does_not_kill_the_process() -> anyhow::Result<()> {
    // Bez ścieżki gniazda po fladze: `main.rs` ma jedno zdanie do napisania i kod wyjścia 2.
    let missing = loadout_writing_into_the_void(&["--bridge"])?;
    assert_eq!(
        missing,
        Some(NO_SOCKET_PATH),
        "asked for the bridge with no socket path and with nothing reading its diagnostic \
         output, Loadout came back as {missing:?} instead of {NO_SOCKET_PATH}. A macro that \
         panics when the write fails turns the one code the caller could act on into an \
         interrupted process — and in release this build aborts, so there is not even a message"
    );

    // Ścieżka, pod którą nie ma gniazda: tym razem odmawia `bridge/mod.rs`, kodem 1.
    let dir = tempfile::tempdir()?;
    let nowhere = dir.path().join("no-such-place").join("loadout.sock");
    let unusable = loadout_writing_into_the_void(&[
        "--bridge",
        nowhere
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("the temporary path is not text"))?,
    ])?;
    assert_eq!(
        unusable,
        Some(SOCKET_UNUSABLE),
        "asked for the bridge on a path where no socket can be reached, Loadout came back as \
         {unusable:?} instead of {SOCKET_UNUSABLE}. This is the second of the two places that \
         write before leaving, and it fails in exactly the situation this is about: the bridge \
         outlives nothing, so whoever it was talking to is usually already gone"
    );
    Ok(())
}

#[test]
fn the_sentence_still_stands_when_somebody_is_reading() -> anyhow::Result<()> {
    /* NIEZMIENNIK 29: kod wyjścia dowodzi, że mechanizm nie pada; TO dowodzi, że produkt dalej
     * mówi. Zamiana makra na zapis z porzuconym wynikiem jest dokładnie tym rodzajem poprawki,
     * po której zdanie znika, a wszystko inne wygląda tak samo — i nikt się nie dowie, bo pyta
     * się o kod wyjścia. */
    let said = loadout_talking_to_someone(&["--bridge"])?;
    assert!(
        said.contains("Loadout needs the socket path after --bridge"),
        "asked for the bridge with no socket path, Loadout said {said:?}. The whole point of \
         writing before leaving is that whoever started it learns what was missing"
    );

    let dir = tempfile::tempdir()?;
    let nowhere = dir.path().join("no-such-place").join("loadout.sock");
    let refused = loadout_talking_to_someone(&[
        "--bridge",
        nowhere
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("the temporary path is not text"))?,
    ])?;
    assert!(
        refused.contains("Loadout's bridge stopped"),
        "asked for the bridge on a path where no socket can be reached, Loadout said {refused:?}"
    );
    Ok(())
}

#[test]
fn no_writing_macro_that_panics_is_left_in_the_app() -> anyhow::Result<()> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files_under(&src, &mut files)?;
    assert!(
        !files.is_empty(),
        "found no Rust files under {}, so this guard is looking at the wrong place and would \
         stay quiet whatever the app does",
        src.display()
    );

    let mut caught = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file)?;
        for (number, line) in text.lines().enumerate().filter(|(_, one)| is_code(one)) {
            for writer in PANICKING_WRITERS {
                if names_the_macro(line, writer) {
                    caught.push(format!("{}:{} {writer}", file.display(), number + 1));
                }
            }
        }
    }
    assert!(
        caught.is_empty(),
        "these lines still write through a macro that panics when the write fails: {caught:?}. \
         Every one of them is a place where a closed pipe on the other side ends the process \
         instead of the sentence — and in release this build aborts. Write it with `writeln!` \
         and drop the result on purpose"
    );
    Ok(())
}
