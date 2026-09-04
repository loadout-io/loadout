//! AC-2 dla T-34: po skasowaniu bazy bieg odtwarza się z plików, co do zdarzenia.
//!
//! Niezmiennik 4 ma dwie połowy i do dziś istniała tylko jedna. `store::rebuild_from` działa
//! i ma na to kryterium (T-06 AC-4) — tylko odbudowuje z pliku, którego **nikt nie pisał**.
//! Tamto kryterium karmi odbudowę fiksturą napisaną ręcznie, więc przechodziło i przechodzi
//! niezależnie od tego, czy prawdziwy bieg zostawia po sobie cokolwiek. Tutaj plik pisze
//! **żywy krok**, i dopiero to zamyka zdanie „`loadout.db` wolno skasować".
//!
//! **Słaba wersja tego kryterium to porównanie licznika linii przed i po.** Przechodzi ją
//! odbudowa, która gubi treść i zostawia puste wiersze — dwa razy tyle samo `NULL`i to nadal
//! równość. Rozróżnia je pytanie zadane **tam, gdzie linie czyta człowiek**
//! (`history::read_run_inner`, niezmiennik 29), o zdanie, które agent naprawdę powiedział.
//!
//! 2026-09 (Z-15) — GDZIE TE LINIE MIESZKAJĄ. Do tej zmiany transkrypt wchodził przy odbudowie
//! także do tabeli `events`, i to tamtą kopię sprawdzał ten plik. Kopii już nie ma: 27 362 wiersze
//! `raw` były 86 % żywej biblioteki i nie czytał ich ani jeden `SELECT`. Pytanie zostaje to samo
//! — „co kosztuje skasowanie `loadout.db`" — tylko zadane jedynej stronie, która na nie odpowiada.
//!
//! Dlatego asercje idą w tej kolejności:
//!
//! 1. krok zostawił transkrypt i jest w nim tyle linii, ile wypluł proces — bez tego wszystko
//!    poniżej porównuje pustkę z pustką;
//! 2. po pierwszym zaindeksowaniu bieg otwarty komendą okna niesie te linie razem z prozą agenta,
//!    a indeks nie niesie ani jednej;
//! 3. plik bazy znika razem z `-wal` i `-shm` — zostawienie tamtych dwóch nie jest skasowaniem
//!    bazy, tylko skasowaniem jednego z jej trzech plików;
//! 4. odbudowa oddaje **te same wiersze** biegu, kroków i wskazań na pliki, kolumna po kolumnie,
//!    a człowiek czyta dokładnie to samo, co przed skasowaniem.
//!
//! `run.json` jest tu napisany ręcznie, a nie wyprodukowany naszym serializatorem, i to jest
//! cała jego wartość: fikstura zbudowana naszym kodem definiuje kształt, zamiast go sprawdzać
//! [04 §6.4]. Plik transkryptu — odwrotnie — **musi** powstać z biegu, bo o to jest to
//! kryterium.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use loadout_lib::commands::history::read_run_inner;
use loadout_lib::engine::drivers::claude::{ClaudeDriver, Transcript};
use loadout_lib::engine::drivers::{AgentDriver, AgentHandle, Policy, RunSpec};
use loadout_lib::store::Store;
use rusqlite::Connection;
use rusqlite::types::Value;
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

/// Sufit na każde pojedyncze oczekiwanie. Zawieszenie jest dla bramki kodem 124, czyli
/// „nic się nie wykonało" — a to nie jest dowód.
const LIMIT: Duration = Duration::from_secs(20);

/// Ile miejsca mają kanały.
const CHANNEL: usize = 256;

/// Bieg z fikstury. Ten sam identyfikator stoi w `run.json` i w nazwie katalogu.
const RUN_ID: &str = "01996500-0000-7000-8000-000000000001";

/// Krok, którego to strumień. Plik nazywa się po nim i po nim odbudowa wie, do kogo należą
/// zdarzenia.
const STEP: &str = "01996500-0000-7000-8000-00000000000a";

/// Nazwa katalogu biegu z `docs/ARCHITECTURE.md` §8.
const RUN_DIR: &str = "2026-08-16T09-00-00Z__01996500";

/// Agent, którego strumień to jest.
const AGENT: &str = "builder";

/// Jedyne zdanie, które agent naprawdę powiedział w tym strumieniu. Reszta linii to jego praca.
const PROSE: &str = "It splits on every comma, including the ones inside quotes.";

/// `run.json` — bieg i jego jeden krok. Pisany ręcznie, bo to jest kontrakt na dysku.
///
/// Klucze są dokładnie tymi, które czyta `store::rebuild`; rozjazd znaczy, że po skasowaniu
/// bazy dostaje się co innego, niż się miało.
const RUN_JSON: &str = r#"{
  "id": "01996500-0000-7000-8000-000000000001",
  "workflow_id": "ship-a-feature",
  "title": "Fix the CSV parser",
  "status": "succeeded",
  "concurrency": 3,
  "created_at": 1755300000000,
  "started_at": 1755300001000,
  "ended_at": 1755300042000,
  "error": null,
  "workflow_snapshot": {
    "nodes": [{ "key": "fix", "agent": "claude", "model": "opus" }],
    "edges": []
  },
  "steps": [
    {
      "id": "01996500-0000-7000-8000-00000000000a",
      "node_key": "fix",
      "name": "Fix the parser",
      "agent": "claude",
      "depends_on": [],
      "status": "succeeded",
      "attempt": 0,
      "agent_session_id": "01996500-0000-7000-8000-0000000000aa",
      "pid": 44101,
      "pgid": 44101,
      "exit_code": 0,
      "started_at": 1755300001000,
      "ended_at": 1755300019000,
      "cost_usd": 0.0123,
      "summary": "Changed one line and the checks went green",
      "error": null
    }
  ]
}
"#;

/// Pięć linii, które wypisuje atrapa. Każda niesie **inną** treść, i to jest warunek, żeby
/// „ta sama treść" cokolwiek znaczyło: pięć identycznych linii przeszłoby także wtedy, gdyby
/// odbudowa pomieszała im kolejność.
///
/// Linia trzecia ma `type`, którego nikt nigdy nie wysłał. Jest tu, bo odbudowa nazywa rodzaj
/// zdarzenia polem z drutu — więc linia, której nie umiemy nazwać, dalej jest linią, która się
/// wydarzyła, i ma przeżyć skasowanie bazy tak samo jak reszta.
const STREAM: &str = concat!(
    r#"{"type":"system","subtype":"init","session_id":"01996500-0000-7000-8000-0000000000aa","tools":["Read","Edit"]}"#,
    "\n",
    r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"It splits on every comma, including the ones inside quotes."}]}}"#,
    "\n",
    r#"{"type":"quantum_flux","payload":{"seen":"never"}}"#,
    "\n",
    r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_01","name":"Read","input":{"file_path":"src/csv.rs","description":"Read the splitter"}}]}}"#,
    "\n",
    r#"{"type":"result","subtype":"success","is_error":false,"terminal_reason":"completed","num_turns":3,"duration_ms":18000,"total_cost_usd":0.0123,"result":"done"}"#,
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
        reaches_the_web: false,
        policy: Policy::EditInFolder,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Puszcza jeden krok przez sterownik i wraca, kiedy pętla czytająca skończyła.
///
/// Zamknięcie kanału zdarzeń jest punktem synchronizacji: pętla porzuca nadajniki na końcu, więc
/// dopiero po nim wolno pytać dysk o plik.
async fn run_one_step(home: &Path, run_dir: &Path) -> Result<(), Box<dyn Error>> {
    let binary = write_script(home, "claude", DUMMY)?;
    fs::write(home.join("stream.jsonl"), STREAM)?;
    fs::create_dir_all(run_dir.join("logs"))?;
    fs::write(run_dir.join("run.json"), RUN_JSON)?;

    let (events_tx, mut events) = mpsc::channel(CHANNEL);
    let (lines_tx, _lines) = mpsc::channel(CHANNEL);

    let driver = ClaudeDriver::with_binary(binary).with_transcript(Transcript {
        run_dir: run_dir.to_path_buf(),
        step: STEP.to_owned(),
        agent: AGENT.to_owned(),
        lines: lines_tx,
    });

    let mut handle: Box<dyn AgentHandle> =
        timeout(LIMIT, driver.start(spec(Uuid::now_v7(), home), events_tx)).await??;
    timeout(LIMIT, async { while events.recv().await.is_some() {} }).await?;
    let _code = timeout(LIMIT, handle.close()).await??;
    Ok(())
}

/// Kasuje bazę — **razem** z `-wal` i `-shm`.
///
/// Zostawienie tamtych dwóch nie byłoby skasowaniem bazy: to są pliki tej samej bazy i zostaje
/// w nich każdy zapis, którego nie zdążył przenieść checkpoint.
fn remove_database(db: &Path) -> Result<(), Box<dyn Error>> {
    for suffix in ["", "-wal", "-shm"] {
        let mut name = db.as_os_str().to_os_string();
        name.push(suffix);
        let path = PathBuf::from(name);
        if path.exists() {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// Wszystko, co po tym biegu zostaje w indeksie: bieg, jego kroki i wskazania na pliki.
///
/// Kolumny bierzemy gwiazdką, więc do porównania wchodzi także ta, którą ktoś doda jutro — a `ts`
/// i `created_at` są w nim celowo: to je najłatwiej zapisać zegarem zamiast plikiem, i wtedy
/// odbudowa oddaje coś innego, niż skasowano.
fn dump_index(conn: &Connection) -> Result<Vec<String>, Box<dyn Error>> {
    let mut dump = Vec::new();
    for table in ["runs", "steps", "artifacts"] {
        let mut stmt = conn.prepare(&format!("SELECT * FROM \"{table}\" ORDER BY 1"))?;
        let width = stmt.column_count();
        let rows = stmt.query_map([], move |row| {
            let mut cells = Vec::with_capacity(width);
            for index in 0..width {
                cells.push(format!("{:?}", row.get::<_, Value>(index)?));
            }
            Ok(cells.join(" | "))
        })?;
        for row in rows {
            dump.push(format!("{table} == {}", row?));
        }
    }
    Ok(dump)
}

/// Sprawdza, że linie tego kroku dochodzą **do człowieka** — i że nie ma ich w indeksie.
///
/// `read_run_inner` jest komendą, którą woła okno, a nie skrótem do plików: gdyby transkrypt
/// zniknął z dysku, ta droga oddałaby krok bez ani jednego wiersza i to jest jedyny sposób, żeby
/// zobaczyć różnicę między „indeks przestał kopiować" a „transkrypt zginął".
fn assert_the_lines_reach_the_person(
    project: &Path,
    conn: &Connection,
    raw: &str,
    when: &str,
) -> Result<(), Box<dyn Error>> {
    assert_eq!(
        raw.lines().count(),
        STREAM.lines().count(),
        "{when}: the transcript on disk holds {} lines and the process wrote {}. Indexing is not \
         allowed to touch the file it reads",
        raw.lines().count(),
        STREAM.lines().count(),
    );

    let past = read_run_inner(project, RUN_DIR)?;
    let step = past
        .steps
        .iter()
        .find(|one| one.id == STEP)
        .ok_or("the run read back off disk has no step, though run.json describes one")?;
    assert!(
        !step.lines.is_empty(),
        "{when}: the step whose stream was recorded reads back with no rows at all. The lines are \
         in logs/agent-{STEP}.jsonl and this is the command the window asks with"
    );
    assert!(
        step.lines.iter().any(|line| line.text() == PROSE),
        "{when}: the one thing the agent actually said is missing from the run read back off \
         disk. A count of rows would not see this: curation can hand back the right number of \
         empty ones"
    );

    // Kontrola przeciw pustej asercji: dwa zrzuty pustego indeksu też są sobie równe.
    let runs: i64 = conn.query_row("SELECT count(*) FROM runs WHERE id = ?1", [RUN_ID], |row| {
        row.get(0)
    })?;
    assert_eq!(
        runs, 1,
        "{when}: run.json describes one run and the index holds {runs} of them"
    );

    let lines: i64 = conn.query_row("SELECT count(*) FROM events", [], |row| row.get(0))?;
    assert_eq!(
        lines, 0,
        "{when}: the index holds {lines} lines of a transcript that is already on disk. Each of \
         those rows is the same line written twice, and nothing ever read them from there"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deleting_the_database_costs_nothing_because_the_step_wrote_its_transcript()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let run_dir = home.path().join(".loadout").join("runs").join(RUN_DIR);
    run_one_step(home.path(), &run_dir).await?;

    // ── Krok zostawił po sobie plik ───────────────────────────────────────────────────────
    let tee = run_dir.join("logs").join(format!("agent-{STEP}.jsonl"));
    assert!(
        tee.exists(),
        "a real step ran and left no transcript at {}. Everything below this line would then \
         compare an empty index with an empty index and call it equality",
        tee.display(),
    );
    let raw = fs::read_to_string(&tee).unwrap_or_default();
    assert_eq!(
        raw.lines().count(),
        STREAM.lines().count(),
        "the process wrote {} lines and the transcript holds {}",
        STREAM.lines().count(),
        raw.lines().count(),
    );

    // ── Zaindeksuj katalog biegu ──────────────────────────────────────────────────────────
    let db = home.path().join(".loadout").join("loadout.db");
    let store = Store::open(&db)?;
    store.rebuild_from(&run_dir).await?;
    let reader = store.reader()?;
    assert_the_lines_reach_the_person(home.path(), &reader, &raw, "after the first index")?;
    let indexed = dump_index(&reader)?;
    drop(reader);
    store.close().await?;

    // ── Skasuj plik bazy ──────────────────────────────────────────────────────────────────
    remove_database(&db)?;
    assert!(
        !db.exists(),
        "the database file is still there, so nothing below is about rebuilding it"
    );

    // ── Odbuduj z plików, które zostały ───────────────────────────────────────────────────
    let store = Store::open(&db)?;
    store.rebuild_from(&run_dir).await?;
    let reader = store.reader()?;
    let raw = fs::read_to_string(&tee).unwrap_or_default();
    assert_the_lines_reach_the_person(home.path(), &reader, &raw, "after the rebuild")?;
    let rebuilt = dump_index(&reader)?;

    let divergence = rebuilt
        .iter()
        .zip(&indexed)
        .position(|(after, before)| after != before);
    assert!(
        divergence.is_none(),
        "the rebuilt index is not the index that was there before the database went away. \
         They part company at row {divergence:?}: after the rebuild {:?}, before it {:?}. Every \
         column has to be a function of the files on disk - a value stamped with the current \
         time or a fresh key during the rebuild lands here",
        divergence.and_then(|at| rebuilt.get(at)),
        divergence.and_then(|at| indexed.get(at)),
    );
    assert_eq!(
        rebuilt.len(),
        indexed.len(),
        "the rebuilt index holds a different number of rows than the one that was deleted. \
         The shorter side is the answer to 'what does deleting loadout.db cost'"
    );

    drop(reader);
    store.close().await?;
    Ok(())
}
