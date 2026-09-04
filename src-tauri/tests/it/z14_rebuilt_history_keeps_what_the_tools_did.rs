//! Z-14, kryterium 2: historia odbudowana z samych plików mówi, CO krok zrobił.
//!
//! Niezmiennik 4 („pliki są prawdą, `loadout.db` jest indeksem") ma jedną drogę odczytu i to
//! jest ta, którą otwiera okno: [`read_run_inner`] → `recorded_lines` → ten sam kurator, który
//! składał wiersze w trakcie biegu. Bazy tu nie ma w ogóle — nikt jej nie zakłada, nikt jej nie
//! kasuje — bo pytanie brzmi „czy plik wystarczy", a nie „czy kasowanie zadziałało".
//!
//! # Co dokładnie się psuło
//!
//! Wynik czynności przyjeżdża od `claude` linią `type:"user"` z blokiem `tool_result` i **tylko**
//! nią: z niej powstaje `ToolEnd`, a z niego `FileEdit`. Filtr prywatności sterownika odrzucał
//! całą tę rodzinę linii po samym `type`, więc plik dowodowy dostawał początki czynności bez ani
//! jednego końca. Kurator czytający taki plik domyka komendę bez wyniku jedynym uczciwym
//! wierszem, jaki mu został — „didn't work", bez podglądu wyjścia — i robi to dopiero
//! w `flush()`, czyli PO wierszu kończącym krok. Człowiek widzi zatem krok, który zdał, i pod
//! nim komendę, która „się nie udała", bez ani jednej litery z jej wyjścia.
//!
//! # Słaba wersja tego kryterium
//!
//! `assert!(!lines.is_empty())` albo asercja na samym wierszu `Edited`. Przechodzi ją dzisiejsze
//! drzewo: wiersz o zmianie pliku powstaje z **początku** czynności i stoi w historii nawet
//! wtedy, gdy żaden wynik nigdy nie dojechał. Rozstrzyga wiersz o komendzie: jak poszła i czy
//! niesie to, co powiedziała.
//!
//! Niezmiennik 29: sądzone jest zdanie, które czyta człowiek, wzięte tą samą komendą, którą
//! woła okno — nie wartość zwrócona przez filtr.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use loadout_lib::commands::history::read_run_inner;
use loadout_lib::engine::drivers::claude::{ClaudeDriver, Transcript};
use loadout_lib::engine::drivers::{AgentDriver, AgentHandle, Policy, RunSpec};
use loadout_lib::engine::line::{Line, LineKind};
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

/// Sufit na każde pojedyncze oczekiwanie. Regresja ma się objawić jako **czerwony test**, nie
/// jako zawieszenie: bramka czyta rc 124 jako „nic się nie wykonało", a to nie jest dowód.
const LIMIT: Duration = Duration::from_secs(20);

/// Ile miejsca mają kanały. Z zapasem, bo pełny kanał zatrzymuje pętlę czytającą.
const CHANNEL: usize = 256;

/// Katalog biegu i jego krok. Nazwa katalogu jest z `docs/ARCHITECTURE.md` §8: `<ts>__<id>`.
const RUN_DIR: &str = "20260904-101500__0199c14a-0000-7000-8000-00000000e14a";
const STEP: &str = "0199c14a-0000-7000-8000-00000000e14b";

/// Nazwa kafelka. Podpisuje każdy wiersz historii tego kroku.
const TILE: &str = "Build";

/// Tura człowieka. Dłuższa niż dolna granica igły prywatności, więc **jest** skanowana jako
/// podciąg — i ani jedna linia strumienia jej nie niesie.
const TURN: &str = "teach the comma splitter about quoted fields";

/// Zdanie o zmianie pliku, dosłownie tak, jak stoi na ekranie.
const EDITED: &str = "Edited csv.rs";

/// Zdanie o komendzie, dosłownie tak, jak stoi na ekranie po udanym przebiegu.
const RAN: &str = "Ran ./run-tests — ok";

/// Zdanie o tej samej komendzie, kiedy ona jeszcze szła.
///
/// 2026-09 (Z-36) — odbudowana historia niesie OBA wpisy, bo kurator wypuszcza wiersz w chwili,
/// w której komenda rusza, i przepisuje go wynikiem; oba mają ten sam `call_id`, więc okno
/// pokazuje z nich jeden. Czasu nie ma w tym zdaniu żadnego prawdziwego: surowy strumień nie
/// niesie znaczników czasu, więc odbudowa podaje kuratorowi zero (`recorded_lines`), a
/// heartbeatu w tej fiksturze nie ma.
const WORKING: &str = "Working: ./run-tests · 0s";

/// Fragment wyjścia komendy. Jedzie WYŁĄCZNIE blokiem `tool_result`, więc jego obecność
/// w podglądzie jest dowodem, że tamta linia dojechała na dysk w całości.
const SAID_BY_THE_COMMAND: &str = "test csv::quoted_commas ... ok";

/// Strumień atrapy: zmiana pliku, jej wynik, komenda, jej wynik, koniec tury.
///
/// Sklejony z `concat!`, żeby „posprzątanie" pliku przez edytor nie skasowało po cichu żadnej
/// z linii `type:"user"` — one są całą treścią tego kryterium.
const STREAM: &str = concat!(
    r#"{"type":"system","subtype":"init","session_id":"0199c14a-0000-7000-8000-0000000000aa","model":"opus","tools":["Write","Bash"]}"#,
    "\n",
    r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_01","name":"Write","input":{"file_path":"src/csv.rs","description":"Teach the splitter about quotes"}}]}}"#,
    "\n",
    r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_01","content":"Applied 1 edit to src/csv.rs"}]}}"#,
    "\n",
    r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_02","name":"Bash","input":{"command":"./run-tests","description":"Run the checks"}}]}}"#,
    "\n",
    r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_02","content":"running 1 test\ntest csv::quoted_commas ... ok\ntest result: ok. 1 passed"}]}}"#,
    "\n",
    r#"{"type":"result","subtype":"success","is_error":false,"terminal_reason":"completed","num_turns":2,"duration_ms":9000,"total_cost_usd":0.02,"result":"done"}"#,
    "\n",
);

/// `run.json` tego biegu, wypisany literalnie.
///
/// Ręcznie, nigdy przez kod produkcyjny: odczyt, który czyta wyłącznie to, co sam zapisał, nie
/// odpowiada na pytanie o niezmiennik 4 ani trochę (ta sama zasada, co w `history_reads_the_runs`).
const DESCRIPTION: &str = r#"{
  "id": "0199c14a-0000-7000-8000-00000000e14a",
  "workflow_id": "teach-the-splitter.json",
  "workflow_hash": "0123456789abcdef",
  "title": "Teach the splitter",
  "status": "succeeded",
  "concurrency": 1,
  "steps": [
    {
      "id": "0199c14a-0000-7000-8000-00000000e14b",
      "node_key": "build",
      "name": "Build",
      "agent": "claude",
      "kind": "agent",
      "depends_on": [],
      "status": "succeeded",
      "attempt": 0,
      "cost_usd": 0.02,
      "summary": "Taught the splitter about quotes.",
      "error": null
    }
  ]
}"#;

/// Atrapa `claude`: odbiera kopertę stdinem i wypisuje przygotowany strumień, bajt w bajt.
///
/// Strumień leży **w pliku obok skryptu**, a nie w treści skryptu: powłoka rozwijałaby w nim
/// escape'y i cudzysłowy.
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
///
/// Plik ze skryptem, nigdy `sh -c "…"` i nigdy kopia binarki systemowej: skopiowany plik
/// systemowy dostaje na `macOS` `SIGKILL` od podpisu kodu [T7 §8.2].
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
        prompt: TURN.to_owned(),
        model: None,
        system_append: None,
        reaches_the_web: false,
        policy: Policy::EditInFolder,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Puszcza jeden prawdziwy krok i wraca dopiero wtedy, gdy pętla czytająca skończyła.
///
/// Zamknięcie kanału zdarzeń jest **jedynym** uczciwym punktem synchronizacji: pętla porzuca
/// oba nadajniki, kiedy strumień się skończył, więc dopiero po nim wolno pytać dysk o plik.
async fn run_one_step(home: &Path, run_dir: &Path) -> Result<(), Box<dyn Error>> {
    let binary = write_script(home, "claude", DUMMY)?;
    fs::write(home.join("stream.jsonl"), STREAM)?;
    fs::create_dir_all(run_dir.join("logs"))?;

    let (events_tx, mut events) = mpsc::channel(CHANNEL);
    // Odbiornik wierszy żyje do końca funkcji: to kryterium jest o ścieżce DYSKU, a ta nie ma
    // prawa zależeć od tego, czy widok nadąża [T7 §4.1].
    let (lines_tx, _lines) = mpsc::channel(CHANNEL);

    let driver = ClaudeDriver::with_binary(binary).with_transcript(Transcript {
        run_dir: run_dir.to_path_buf(),
        step: STEP.to_owned(),
        agent: TILE.to_owned(),
        lines: lines_tx,
    });

    let mut handle: Box<dyn AgentHandle> =
        timeout(LIMIT, driver.start(spec(Uuid::now_v7(), home), events_tx)).await??;
    timeout(LIMIT, async { while events.recv().await.is_some() {} }).await?;

    // Koniec sesji, nie koniec tury: bez tego czasownika skończony krok zostawia żywy proces.
    let _code = timeout(LIMIT, handle.close()).await??;
    Ok(())
}

/// Teksty wierszy jednego rodzaju, w kolejności, w jakiej stoją na ekranie.
fn texts(lines: &[Line], kind: LineKind) -> Vec<&str> {
    lines
        .iter()
        .filter(|line| line.kind() == kind)
        .map(Line::text)
        .collect()
}

/// Wszystkie teksty — do komunikatu, kiedy asercja pada.
fn all_texts(lines: &[Line]) -> Vec<&str> {
    lines.iter().map(Line::text).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_history_built_from_files_alone_says_what_the_step_changed_and_ran()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let run_dir = project.path().join(".loadout").join("runs").join(RUN_DIR);

    run_one_step(home.path(), &run_dir).await?;
    fs::write(run_dir.join("run.json"), DESCRIPTION)?;

    // Tą samą drogą, którą chodzi okno. Bazy nie ma tu ani przez chwilę.
    let opened = read_run_inner(project.path(), RUN_DIR)?;
    let step = opened
        .steps
        .iter()
        .find(|one| one.id == STEP)
        .ok_or("the run description names exactly this step")?;
    let lines = step.lines.as_slice();

    assert_eq!(
        texts(lines, LineKind::Edit),
        vec![EDITED],
        "the history rebuilt from files alone has to name the file this step changed. All of \
         its rows were {:?}",
        all_texts(lines),
    );

    let ran = texts(lines, LineKind::Ran);
    assert_eq!(
        ran,
        vec![WORKING, RAN],
        "the command finished and said so, and the only place that answer travels is the line \
         the filter used to throw away. A row reading \"didn't work\" here is a rebuild that \
         never saw the end of the command - and it tells a person the opposite of what happened. \
         All of the rows were {:?}",
        all_texts(lines),
    );

    let Some(Line::Ran { preview, .. }) = lines.iter().find(|line| line.text() == RAN) else {
        unreachable!("the row asserted above is a Ran row")
    };
    assert!(
        preview.contains(SAID_BY_THE_COMMAND),
        "the row about the command has to carry what the command said. That output exists in \
         exactly one place on the wire, and a rebuild without it leaves a person opening a run \
         to look at a failure they cannot see. It carried {preview:?}"
    );

    // Kolejność: najpierw zmiana, potem komenda — tak, jak się wydarzyły. Komenda domknięta
    // dopiero w `flush()` ląduje ZA wierszem kończącym krok, czyli poza swoim miejscem.
    let order = all_texts(lines);
    let edited = order.iter().position(|text| *text == EDITED);
    let finished = order.iter().position(|text| *text == RAN);
    assert!(
        edited < finished,
        "the file was changed before the command ran, so the two rows have to stand in that \
         order. Rows closed only when the stream ends drift to the bottom, below the row that \
         says the step is over. The rows were {order:?}"
    );

    assert_eq!(
        opened.steps.len(),
        1,
        "this run has one step in its description and nothing may invent a second one"
    );
    assert_eq!(
        step.name, TILE,
        "every row of this history is signed with the tile name, and it comes from the file"
    );
    assert_eq!(
        opened.folder, RUN_DIR,
        "the run opened here is the one on disk, named the way the folder names it"
    );
    assert_eq!(
        opened.title, "Teach the splitter",
        "the open run has to name itself the way the file names it"
    );

    Ok(())
}
