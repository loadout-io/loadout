//! AC-3 dla T-34: żywy bieg produkuje wiersze **kurowane**, a nie same surowe.
//!
//! Kuracja zdarzenie → linia jest miejscem, w którym powstaje wartość produktu
//! (`docs/ARCHITECTURE.md` §6, decyzja D4). Do dziś powstawała wyłącznie w testach: żywa droga
//! podaje kuratorowi `tool: None`, a bez faktów o narzędziu nie da się wybrać wariantu wiersza —
//! `Read` to nie `Edit`, a etykieta, którą model napisał sobie sam, nie mówi, co się stało.
//! Wiersze `Read`, `Edited` i `Ran` nie powstawały więc w prawdziwym biegu ani razu.
//!
//! **Słaba wersja tego kryterium to „cokolwiek wyszło na kanał".** Przechodzi ją dzisiejszy
//! stan, bo wiersze powstają — proza i koniec tury — tylko wszystkie czynności agenta są w nich
//! nierozróżnialne. Rozróżnia **różnica między trzema rodzajami**: czytanie, zmiana pliku
//! i komenda mają trafić do trzech różnych wierszy, a wiersz komendy ma nieść to, jak się
//! skończyła.
//!
//! Trzy asercje robią razem robotę, której żadna nie zrobi sama:
//!
//! - **pełna ścieżka w wierszu czytania i zmiany.** `AgentEvent` jej nie niesie — niesie
//!   etykietę po ludzku („Read the splitter") — więc wiersz z `src/csv.rs` w `paths` dowodzi,
//!   że fakty o narzędziu dojechały z tej samej linii drutu, z której powstało zdarzenie;
//! - **wiersz odczytu czyta się inaczej niż wiersz edycji.** Ta sama etykieta w obu byłaby
//!   implementacją, która przepisuje `description` i nazywa to kuracją;
//! - **dwie komendy, jedna udana i jedna nie.** Wiersz z `ok` wpisanym na sztywno przechodzi
//!   test z jedną komendą; z dwiema nie przechodzi.
//!
//! Kontrola przeciw pustej asercji stoi na końcu i jest **dzisiejszym stanem**: te same
//! zdarzenia puszczone przez kuratora z `tool: None` — dokładnie tak, jak robi to
//! `commands::run` — nie dają ani jednego wiersza czynności. Bez niej całe kryterium mogłoby
//! przechodzić na zachowaniu, które ma zastąpić.
//!
//! Reguł zwijania i czternastu rodzajów wiersza to kryterium **nie sądzi** — mają swoje w T-05
//! i T-08. Pyta wyłącznie o to, czy żywa droga w ogóle do nich dociera.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use loadout_lib::engine::drivers::claude::{ClaudeDriver, Transcript};
use loadout_lib::engine::drivers::{AgentDriver, AgentEvent, AgentHandle, Policy, RunSpec};
use loadout_lib::engine::line::{Curator, Line, LineKind, Seen};
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

/// Sufit na każde pojedyncze oczekiwanie. Zawieszenie jest dla bramki kodem 124, czyli „nic się
/// nie wykonało" — a to nie jest dowód.
const LIMIT: Duration = Duration::from_secs(20);

/// Ile miejsca mają kanały.
const CHANNEL: usize = 256;

/// Krok, którego to strumień.
const STEP: &str = "01996500-0000-7000-8000-00000000000a";

/// Nazwa katalogu biegu z `docs/ARCHITECTURE.md` §8.
const RUN_DIR: &str = "2026-08-16T09-00-00Z__01996500";

/// Agent, którego strumień to jest. Wchodzi w każdy wiersz.
const AGENT: &str = "builder";

/// Plik, którego dotyczy odczyt i zmiana — **pełną ścieżką**, bo rozwinięcie wiersza pokazuje
/// pliki, a sama nazwa nie mówi, o który plik chodzi w drzewie z trzema `mod.rs`.
const TARGET: &str = "src/csv.rs";

/// Fragment wyjścia komendy, która się nie udała. To jest ta wartość, której nie ma
/// w jednolinijkowym podsumowaniu zdarzenia, a którą wiersz `ran` ma pokazać bez klikania.
const HOW_IT_ENDED: &str = "exit status 101";

/// Strumień atrapy: proza, odczyt, zmiana pliku, komenda nieudana, komenda udana, koniec tury.
///
/// Bloki `tool_use` niosą `description` — to jest ten prezent od modelu, z którego bierze się
/// etykieta zdarzenia [T1 §8.6]. Właśnie dlatego etykieta **nie wystarcza**: „Read the splitter"
/// i „Teach the splitter about quotes" są zdaniami o tym samym pliku i nic nie mówią o tym,
/// czy plik został przeczytany, czy zmieniony.
const STREAM: &str = concat!(
    r#"{"type":"system","subtype":"init","session_id":"01996500-0000-7000-8000-0000000000aa","tools":["Read","Write","Bash"]}"#,
    "\n",
    r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Let me look at how the parser splits a line."}]}}"#,
    "\n",
    r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_01","name":"Read","input":{"file_path":"src/csv.rs","description":"Read the splitter"}}]}}"#,
    "\n",
    r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_01","content":"pub fn split(line: &str) -> Vec<&str> { line.split(',').collect() }"}]}}"#,
    "\n",
    r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_02","name":"Write","input":{"file_path":"src/csv.rs","description":"Teach the splitter about quotes"}}]}}"#,
    "\n",
    r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_02","content":"Applied 1 edit to src/csv.rs"}]}}"#,
    "\n",
    r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_03","name":"Bash","input":{"command":"cargo test","description":"Run the checks"}}]}}"#,
    "\n",
    r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_03","is_error":true,"content":"running 1 test\ntest csv::quoted_commas ... FAILED\nerror: test failed, exit status 101"}]}}"#,
    "\n",
    r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_04","name":"Bash","input":{"command":"cargo fmt --check","description":"Check the formatting"}}]}}"#,
    "\n",
    r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_04","content":"1 file checked"}]}}"#,
    "\n",
    r#"{"type":"result","subtype":"success","is_error":false,"terminal_reason":"completed","num_turns":4,"duration_ms":22000,"total_cost_usd":0.0456,"result":"done"}"#,
    "\n",
);

/// Atrapa `claude`: odbiera kopertę stdinem i wypisuje przygotowany strumień.
const DUMMY: &str = r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "2.1.233 (Claude Code)"
  exit 0
fi

here="$(dirname "$0")"
IFS= read -r envelope
printf '%s\n' "$envelope" >> "$here/stdin.log"

cat "$here/stream.jsonl"
exit 0
"#;

/// Zapisuje wykonywalny skrypt i zwraca jego ścieżkę.
fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// `RunSpec` jednej tury.
fn spec(run_id: Uuid, cwd: &Path) -> RunSpec {
    RunSpec {
        run_id,
        cwd: cwd.to_path_buf(),
        prompt: "fix the parser".to_owned(),
        model: None,
        system_append: None,
        policy: Policy::EditInFolder,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Wiersze jednego rodzaju, w kolejności, w jakiej wyszły.
fn of_kind(lines: &[Line], kind: LineKind) -> Vec<&Line> {
    lines.iter().filter(|line| line.kind() == kind).collect()
}

/// Rodzaje wszystkich wierszy — do komunikatu, kiedy asercja pada.
fn kinds(lines: &[Line]) -> Vec<LineKind> {
    lines.iter().map(Line::kind).collect()
}

/// Czy wiersz dotyczy dokładnie tego jednego pliku, **pełną ścieżką**.
fn names_only(line: &Line, path: &str) -> bool {
    line.paths().iter().map(String::as_str).eq([path])
}

/// Puszcza jeden krok przez sterownik i oddaje wiersze, które wyszły na ekran.
///
/// Odbiornik wierszy siedzi we **własnym zadaniu**: pełny kanał zatrzymałby pętlę czytającą,
/// a zatrzymana pętla wygląda dokładnie jak zawieszony agent. Kończy się, kiedy znikną wszystkie
/// nadajniki — dlatego sterownik ginie tuż po starcie kroku, a nie na końcu funkcji.
async fn lines_of_one_step(home: &Path, run_dir: &Path) -> Result<Vec<Line>, Box<dyn Error>> {
    let binary = write_script(home, "claude", DUMMY)?;
    fs::write(home.join("stream.jsonl"), STREAM)?;
    fs::create_dir_all(run_dir.join("logs"))?;

    let (events_tx, mut events) = mpsc::channel(CHANNEL);
    let (lines_tx, mut lines) = mpsc::channel(CHANNEL);
    let collector = tokio::spawn(async move {
        let mut seen = Vec::new();
        while let Some(line) = lines.recv().await {
            seen.push(line);
        }
        seen
    });

    let driver = ClaudeDriver::with_binary(binary).with_transcript(Transcript {
        run_dir: run_dir.to_path_buf(),
        step: STEP.to_owned(),
        agent: AGENT.to_owned(),
        lines: lines_tx,
    });
    let mut handle: Box<dyn AgentHandle> =
        timeout(LIMIT, driver.start(spec(Uuid::now_v7(), home), events_tx)).await??;
    drop(driver);

    timeout(LIMIT, async { while events.recv().await.is_some() {} }).await?;
    let _code = timeout(LIMIT, handle.close()).await??;

    Ok(timeout(LIMIT, collector).await??)
}

/// Te same czynności, tak jak widzi je **dzisiejsza** żywa droga: samo zdarzenie, `tool: None`.
///
/// Etykiety są dosłownie tymi, które sterownik bierze z `description` — czyli najlepszym
/// tekstem, jaki ma bez faktów o narzędziu. To jest kontrola przeciw pustej asercji: gdyby
/// kryterium dało się przejść na tym, nie mierzyłoby niczego.
fn rows_without_tool_facts() -> Vec<Line> {
    let events = [
        AgentEvent::ToolStart {
            id: "toolu_01".to_owned(),
            label: "Read the splitter".to_owned(),
        },
        AgentEvent::ToolEnd {
            id: "toolu_01".to_owned(),
            ok: true,
            summary: "pub fn split(line: &str)".to_owned(),
        },
        AgentEvent::ToolStart {
            id: "toolu_02".to_owned(),
            label: "Teach the splitter about quotes".to_owned(),
        },
        AgentEvent::ToolEnd {
            id: "toolu_02".to_owned(),
            ok: true,
            summary: "Applied 1 edit to src/csv.rs".to_owned(),
        },
        AgentEvent::ToolStart {
            id: "toolu_03".to_owned(),
            label: "Run the checks".to_owned(),
        },
        AgentEvent::ToolEnd {
            id: "toolu_03".to_owned(),
            ok: false,
            summary: "error: test failed, exit status 101".to_owned(),
        },
    ];

    let mut curator = Curator::new();
    let mut rows = Vec::new();
    for (step, event) in events.iter().enumerate() {
        let at_ms = u64::try_from(step).unwrap_or_default() * 100;
        rows.extend(curator.observe(Seen {
            agent: AGENT,
            at_ms,
            event,
            tool: None,
        }));
    }
    rows.extend(curator.flush());
    rows
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_live_run_tells_reading_from_editing_from_running() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let run_dir = home.path().join(".loadout").join("runs").join(RUN_DIR);
    let rows = lines_of_one_step(home.path(), &run_dir).await?;

    // ── Czytanie ──────────────────────────────────────────────────────────────────────────
    let read = of_kind(&rows, LineKind::Read);
    assert_eq!(
        read.len(),
        1,
        "one file was read, so one row says so. The rows that came out were {:?}. Zero means \
         the live path handed the curator a tool nobody could name, and then every action of \
         the agent looks the same on screen - which is the whole product being absent",
        kinds(&rows),
    );
    let read = read[0];
    assert!(
        names_only(read, TARGET),
        "the reading row has to carry the FULL path, because expanding it shows files. The \
         event alone carries only the sentence the model wrote about itself, so a row without \
         this path is a row built from the label - and a label is not a fact. It carried {:?}",
        read.paths(),
    );

    // ── Zmiana pliku ──────────────────────────────────────────────────────────────────────
    let edit = of_kind(&rows, LineKind::Edit);
    assert_eq!(
        edit.len(),
        1,
        "one file was changed, so one row says so. The rows that came out were {:?}",
        kinds(&rows),
    );
    let edit = edit[0];
    assert!(
        names_only(edit, TARGET),
        "the row about a changed file has to name the file. It carried {:?}",
        edit.paths(),
    );
    assert_ne!(
        read.text(),
        edit.text(),
        "reading a file and changing it have to read differently on screen. The same sentence \
         in both means the row was copied from the model's own label instead of curated from \
         what the tool actually was: {:?} for the read, {:?} for the change",
        read.text(),
        edit.text(),
    );

    // ── Komendy: jedna nieudana, jedna udana ──────────────────────────────────────────────
    let ran = of_kind(&rows, LineKind::Ran);
    assert_eq!(
        ran.len(),
        2,
        "two commands ran, so two rows say so. The rows that came out were {:?}",
        kinds(&rows),
    );

    let Line::Ran {
        ok: first_ok,
        preview,
        detail,
        ..
    } = ran[0]
    else {
        unreachable!("of_kind returned a row that is not a Ran row")
    };
    assert!(
        !*first_ok,
        "the first command failed and its row says it went fine. How a command ended is the \
         one thing this row exists to say"
    );
    assert!(
        preview.contains(HOW_IT_ENDED),
        "the row about the failed command does not carry what the command said about how it \
         ended. The full output travels with the tool result and nowhere else - a row built \
         from the one-line summary loses exactly the line somebody needs. It carried {preview:?}"
    );
    assert!(
        !detail.is_empty(),
        "a command that did not work opens itself and shows its last lines (rule 3): a person \
         who has to click to find out why the build broke will not click"
    );

    let Line::Ran { ok: second_ok, .. } = ran[1] else {
        unreachable!("of_kind returned a row that is not a Ran row")
    };
    assert!(
        *second_ok,
        "the second command succeeded and its row says it did not. Two commands are here \
         precisely so that a hard-coded answer cannot pass: with one command, either constant \
         is right half the time"
    );

    // ── Kontrola przeciw pustej asercji: dzisiejszy stan ──────────────────────────────────
    let today = rows_without_tool_facts();
    let named: Vec<LineKind> = kinds(&today)
        .into_iter()
        .filter(|kind| matches!(kind, LineKind::Read | LineKind::Edit | LineKind::Ran))
        .collect();
    assert!(
        named.is_empty(),
        "the control is broken: the same three activities produce {named:?} even when the \
         curator is handed no tool facts at all. That is what `commands::run` does today, so \
         if it satisfied the assertions above, this criterion would be measuring nothing"
    );

    Ok(())
}
