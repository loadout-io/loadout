//! Z-15: bieg odbudowany z 10 MB strumienia nie kopiuje do indeksu ani jednej linii — a człowiek
//! dalej widzi je wszystkie.
//!
//! Tabela `events` trzymała pełny surowy strumień agentów: 27 362 wiersze `raw`, 86 % z 72 MB
//! żywej biblioteki, każda linia zapisana drugi raz obok pliku, z którego przyszła. Nie czytał
//! ich w produkcie ani jeden `SELECT`: transkrypt na ekranie składa
//! `commands::history::read_run_inner` prosto z `logs/agent-<krok>.jsonl`. Pliki są prawdą,
//! `SQLite` jest indeksem (niezmiennik 4), a indeks, który przepisuje prawdę, płaci za to
//! dziennikiem wielkości biegu.
//!
//! **Słaba wersja tego kryterium to samo `count(*) = 0`.** Przechodzi ją odbudowa, która nie robi
//! nic — a taka gubi też bieg, jego kroki i wskazania na pliki. Rozróżniają je trzy rzeczy naraz:
//! wiersz `runs` musi wejść, wiersz `artifacts` musi znać rozmiar strumienia, a krok otwarty
//! **tą samą komendą, którą woła okno** musi oddać swoje wiersze razem z prozą agenta
//! (niezmiennik 29).
//!
//! # Dziennik: fikstura musi być BIBLIOTEKĄ SPRZED tej zmiany, nie pustą bazą
//!
//! Pomiar `-wal` jest o kryterium 3 i łatwo go zrobić tak, żeby niczego nie mierzył. Odbudowa po
//! poprawce czyta z logu wyłącznie `metadata`, więc te 10 MB nie ma jak wejść do dziennika — nad
//! pustym indeksem asercja rozmiaru przechodzi także wtedy, gdy zdejmie się z pisarza całe
//! domknięcie. Dlatego przed odbudową sadzimy tu **indeks, który ten bieg po sobie zostawił**:
//! komplet wierszy `raw` tego samego biegu, wpisany tą samą drogą, którą pisał je stary kod
//! (pisarz magazynu, wsadami po sto). To jego kaskada `DELETE FROM runs` przewala przez dziennik
//! i to ona jest tu mierzona.
//!
//! Dziennik mierzymy PRZED zamknięciem magazynu, i to jest cała różnica wobec Z-6: tamto
//! kryterium jest o drodze wyjścia z aplikacji, a odbudowa dzieje się w środku pracy — przy
//! otwartym oknie i żywym czytelniku.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use loadout_lib::commands::history::read_run_inner;
use loadout_lib::engine::limits::Limiter;
use loadout_lib::store::{NewEvent, NewRun, NewStep, Store, apply_pragmas, migrate};
use loadout_lib::workspace::{Registry, RunLine, RunOutcome};
use rusqlite::{Connection, params};

use super::quitting_leaves_the_index_small::{WAL_CEILING_BYTES, journal_beside};

/// Bieg, który odbudowujemy.
const RUN_ID: &str = "0199c150-0000-7000-8000-000000000015";

/// Jego jedyny krok. Po nim nazywa się plik strumienia.
const STEP_ID: &str = "0199c150-0000-7000-8000-00000000001a";

/// Nazwa katalogu biegu (`docs/ARCHITECTURE.md` §8) — i zarazem adres, którym okno prosi
/// o ten bieg.
const FOLDER: &str = "20260902-101500__0199c150-0000-7000-8000-000000000015";

/// Bieg starszy, którego wiersze `raw` stoją w indeksie z czasów przed tą zmianą.
const OLDER_RUN_ID: &str = "0199c150-0000-7000-8000-000000000009";

/// Bieg, którego linie idą drugą drogą do indeksu: pompą karty, nie odbudową.
const PUMPED_RUN_ID: &str = "0199c150-0000-7000-8000-000000000021";

/// Jedyna linia [`OLDER_RUN_ID`]. Migracja jest addytywna (niezmiennik 25): stare wiersze zostają
/// na miejscu, a odbudowa cudzego biegu nie ma prawa ich tknąć.
const OLDER_LINE: &str = r#"{"type":"assistant","message":"an older run wrote this"}"#;

/// Ile linii ma strumień tego kroku.
const LOG_LINES: usize = 2_048;

/// Ile bajtów dokłada jedna linia wypełniająca. Prawdziwy strumień to w większości wyniki
/// narzędzi tej wielkości — stąd taki kształt fikstury, a nie tysiąc krótkich zdań.
const FILLER_BYTES: usize = 5 * 1024;

/// Ile ma ważyć gotowy plik. Asercja, nie komentarz: fikstura przycięta do kilobajtów mierzyłaby
/// dziennik, którego nie ma jak zapełnić, i przechodziłaby na każdym kodzie.
const LOG_BYTES_AT_LEAST: u64 = 10 * 1024 * 1024;

/// Ile linii wchodziło do bazy w jednej transakcji. Ta sama setka, którą wsadza pisarz [T7 §5.3]
/// i którą wsadzała odbudowa sprzed tej zmiany.
const LINES_PER_TRANSACTION: usize = 100;

/// Jedyne zdanie, które agent naprawdę powiedział w tym strumieniu.
const PROSE: &str = "It splits on every comma, including the ones inside quotes.";

/// Pierwsza linia strumienia: agent mówi, czym dysponuje.
const OPENED: &str = r#"{"type":"system","subtype":"init","tools":["Read","Grep"]}"#;

/// Zdanie agenta. To ono ma przeżyć zmianę i pokazać się człowiekowi.
const SAID: &str = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"It splits on every comma, including the ones inside quotes."}]}}"#;

/// Sięgnięcie po plik. Wyniki niżej należą do niego.
const ASKED: &str = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_01","name":"Read","input":{"file_path":"src/csv.rs","description":"Read the splitter"}}]}}"#;

/// `run.json` — bieg i jego krok. Pisany ręcznie, bo to jest kontrakt na dysku: fikstura
/// zbudowana naszym serializatorem definiowałaby kształt, zamiast go sprawdzać [04 §6.4].
const RUN_JSON: &str = r#"{
  "id": "0199c150-0000-7000-8000-000000000015",
  "workflow_id": "ship-a-feature",
  "workflow_snapshot": {"nodes": [{"key": "fix", "agent": "claude"}], "edges": []},
  "title": "Fix the CSV parser",
  "status": "succeeded",
  "concurrency": 1,
  "created_at": 1787000000000,
  "started_at": 1787000001000,
  "ended_at": 1787000042000,
  "error": null,
  "steps": [
    {
      "id": "0199c150-0000-7000-8000-00000000001a",
      "node_key": "fix",
      "name": "Fix the parser",
      "agent": "claude",
      "depends_on": [],
      "status": "succeeded",
      "attempt": 0,
      "agent_session_id": "0199c150-0000-7000-8000-0000000000aa",
      "pid": 44101,
      "pgid": 44101,
      "exit_code": 0,
      "started_at": 1787000001000,
      "ended_at": 1787000019000,
      "cost_usd": 0.0123,
      "summary": "Changed one line and the checks went green",
      "error": null
    }
  ]
}
"#;

/// Strumień kroku: trzy linie, które coś znaczą, i reszta wyników narzędzia — razem ponad 10 MB.
fn a_long_transcript() -> String {
    let filler = "x".repeat(FILLER_BYTES);
    let mut text = String::with_capacity(LOG_LINES * (FILLER_BYTES + 256));
    for line in [OPENED, SAID, ASKED] {
        text.push_str(line);
        text.push('\n');
    }
    for line in 3..LOG_LINES {
        // `write!` do `String` nie ma jak się nie udać; `format!` w pętli po dwóch tysiącach
        // linii budowałby ten sam napis drugi raz, tylko po to, żeby go zaraz skopiować.
        let _ = write!(
            &mut text,
            r#"{{"type":"user","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"toolu_01","content":"{line} {filler}"}}]}}}}"#
        );
        text.push('\n');
    }
    text
}

/// Buduje prawdziwy katalog biegu pod `<projekt>/.loadout/runs/`.
fn write_run_directory(run_dir: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(run_dir.join("logs"))?;
    fs::write(run_dir.join("run.json"), RUN_JSON)?;
    let log = run_dir.join("logs").join(format!("agent-{STEP_ID}.jsonl"));
    fs::write(&log, a_long_transcript())?;
    let bytes = fs::metadata(&log)?.len();
    assert!(
        bytes >= LOG_BYTES_AT_LEAST,
        "the transcript this criterion needs weighs {bytes} bytes, under the \
         {LOG_BYTES_AT_LEAST} it was written against. A short one cannot fill a journal, so \
         everything below would pass on any code at all"
    );
    Ok(())
}

/// Sadzi w indeksie bieg starszy niż ta zmiana, razem z jego linią `raw`.
///
/// Gołym połączeniem, nie przez [`Store`], i to jest cała jego wartość: te wiersze mają wyglądać
/// jak zastane, a nie jak zapisane przez dzisiejszy kod.
fn plant_an_older_run(db: &Path) -> anyhow::Result<()> {
    let conn = Connection::open(db)?;
    apply_pragmas(&conn)?;
    migrate(&conn)?;
    conn.execute(
        "INSERT INTO runs (id, workflow_id, workflow_snapshot, title, status, concurrency, \
         created_at, started_at, ended_at, error, boot_id) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            OLDER_RUN_ID,
            "ship-a-feature",
            r#"{"nodes":[],"edges":[]}"#,
            "An older run",
            "succeeded",
            1_i64,
            1_786_000_000_000_i64,
            1_786_000_001_000_i64,
            1_786_000_002_000_i64,
            Option::<String>::None,
            Option::<String>::None,
        ],
    )?;
    conn.execute(
        "INSERT INTO events (run_id, step_id, ts, kind, level, body) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            OLDER_RUN_ID,
            Option::<String>::None,
            1_786_000_001_000_i64,
            "assistant",
            "raw",
            OLDER_LINE,
        ],
    )?;
    Ok(())
}

/// Bieg i krok w kształcie, w jakim stały w indeksie tego biegu, zanim ta zmiana powstała.
///
/// Wartości nie muszą zgadzać się z `run.json` co do joty — odbudowa i tak wymienia cały wiersz.
/// Muszą przejść `CHECK` na statusie i mieć te identyfikatory, bo to po nich idzie kaskada.
fn the_run_as_it_was_indexed() -> (NewRun, NewStep) {
    (
        a_run(RUN_ID),
        NewStep {
            id: STEP_ID.to_owned(),
            run_id: RUN_ID.to_owned(),
            node_key: "fix".to_owned(),
            name: "Fix the parser".to_owned(),
            agent: "claude".to_owned(),
            depends_on: "[]".to_owned(),
            status: "running".to_owned(),
            attempt: 0,
            agent_session_id: None,
            pid: None,
            pgid: None,
            exit_code: None,
            started_at: Some(1_787_000_001_000),
            ended_at: None,
            cost_usd: None,
            summary: None,
            error: None,
        },
    )
}

/// Sadzi indeks, który ten bieg po sobie zostawił: **każdą** linię strumienia jako wiersz `raw`.
///
/// Przez pisarza magazynu i wsadami po sto, czyli tą samą drogą, którą pisał je kod sprzed tej
/// zmiany — bo to o dziennik tego zapisu tu chodzi. Fikstura wpisana gołym połączeniem i zamknięta
/// przed odbudową nie zostawiłaby w `-wal` ani jednej strony (`SQLite` domyka i kasuje dziennik,
/// kiedy ginie ostatnie połączenie), a wtedy pomiar niżej mierzyłby pustą bazę i przechodziłby
/// także bez domknięcia po `Rows::Snapshot`.
///
/// Oddaje, ile wierszy weszło.
async fn plant_the_index_this_run_left_behind(
    store: &Store,
    run_dir: &Path,
) -> anyhow::Result<i64> {
    let writer = store.writer();
    let (run, step) = the_run_as_it_was_indexed();
    writer.insert_run(run).await?;
    writer.insert_step(step).await?;

    let log = fs::read_to_string(run_dir.join("logs").join(format!("agent-{STEP_ID}.jsonl")))?;
    let lines: Vec<&str> = log.lines().collect();
    for batch in lines.chunks(LINES_PER_TRANSACTION) {
        let rows = batch
            .iter()
            .map(|line| NewEvent {
                run_id: RUN_ID.to_owned(),
                step_id: Some(STEP_ID.to_owned()),
                ts: 1_787_000_001_000,
                kind: "assistant".to_owned(),
                level: "raw".to_owned(),
                body: Some((*line).to_owned()),
            })
            .collect();
        writer.append_events(rows).await?;
    }
    Ok(i64::try_from(lines.len())?)
}

/// Dziennik po odbudowie — razem z kontrolą, że przed nią naprawdę było co przycinać.
///
/// Obie połowy są potrzebne i to jest cała treść tej funkcji: bez `before` asercja o sufcie
/// przechodzi nad pustą bazą, czyli także na kodzie, który dziennika nie domyka wcale.
fn assert_the_journal_came_back_down(db: &Path, before: u64) -> anyhow::Result<()> {
    assert!(
        before > WAL_CEILING_BYTES,
        "the journal held {before} bytes before the rebuild, under the {WAL_CEILING_BYTES} this \
         criterion measures against - so the run never built the situation it is about and the \
         line below would pass on any code at all"
    );
    let after = journal_beside(db)?;
    assert!(
        after <= WAL_CEILING_BYTES,
        "with the app still open, the journal beside the index holds {after} bytes after one \
         rebuild - it stood at {before} and never came down, over the {WAL_CEILING_BYTES} the \
         library is allowed to leave lying about. Taking a run's old index away is one cascade \
         through tens of thousands of rows, and that is exactly how a 72 MB library ends up with \
         a 42 MB journal beside it"
    );
    Ok(())
}

/// Ile wierszy `events` ma ten bieg na tym poziomie.
fn events_at(conn: &Connection, run: &str, level: &str) -> anyhow::Result<i64> {
    Ok(conn.query_row(
        "SELECT count(*) FROM events WHERE run_id = ?1 AND level = ?2",
        params![run, level],
        |row| row.get(0),
    )?)
}

/// Treści linii tego biegu, w kolejności `seq`.
fn bodies_of(conn: &Connection, run: &str) -> anyhow::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT body FROM events WHERE run_id = ?1 ORDER BY seq")?;
    let rows = stmt.query_map([run], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Kontrola przeciw pustej asercji: odbudowa naprawdę weszła i naprawdę zajrzała na dysk.
///
/// Bez tego całe kryterium przechodzi na odbudowie, która nie robi NIC — a taka nie gubi
/// wyłącznie linii, tylko cały bieg razem z nimi.
fn assert_the_rebuild_landed(conn: &Connection) -> anyhow::Result<()> {
    let runs: i64 = conn.query_row("SELECT count(*) FROM runs WHERE id = ?1", [RUN_ID], |row| {
        row.get(0)
    })?;
    assert_eq!(
        runs, 1,
        "run.json describes one run and the index holds {runs} of them. A rebuild that writes \
         nothing at all also holds no lines, so without this line the whole thing passes on it"
    );

    let bytes: Option<i64> = conn.query_row(
        "SELECT bytes FROM artifacts WHERE run_id = ?1 AND kind = 'raw_log'",
        [RUN_ID],
        |row| row.get(0),
    )?;
    let wanted = i64::try_from(LOG_BYTES_AT_LEAST)?;
    assert!(
        bytes.is_some_and(|size| size >= wanted),
        "the index points at the step's transcript with {bytes:?} bytes, not the {wanted} that \
         file weighs. The row that names the file is what stays behind once the lines do not \
         come in any more - without it, deleting loadout.db costs the way back to the file too"
    );
    Ok(())
}

#[tokio::test]
async fn a_rebuilt_run_keeps_its_lines_in_files_and_not_in_the_index() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let project = root.path();
    let run_dir = project.join(".loadout").join("runs").join(FOLDER);
    write_run_directory(&run_dir)?;

    let db = project.join(".loadout").join("loadout.db");
    plant_an_older_run(&db)?;

    let store = Store::open(&db)?;
    // Czytelnik żyje przez całą fiksturę i całą odbudowę, bo tak żyje okno: otwiera swoje
    // połączenia raz i trzyma je przez cały czas pracy. Odbudowa bez tego układu domyka dziennik
    // za darmo — `SQLite` robi to sam, kiedy ginie ostatnie połączenie do bazy.
    let reader = store.reader()?;

    // ── Świat sprzed tej zmiany: indeks tego biegu pełen wierszy `raw` ────────────────────
    let planted = plant_the_index_this_run_left_behind(&store, &run_dir).await?;
    assert_eq!(
        events_at(&reader, RUN_ID, "raw")?,
        planted,
        "the fixture did not get the old index into the database, so the cascade below has \
         nothing to take away and the journal has nothing to grow from"
    );
    let before = journal_beside(&db)?;

    store.rebuild_from(&run_dir).await?;

    assert_the_rebuild_landed(&reader)?;

    let raw = events_at(&reader, RUN_ID, "raw")?;
    assert_eq!(
        raw, 0,
        "the index holds {raw} lines of this run's transcript, out of the {planted} that were \
         standing there before the rebuild. They are on disk, in logs/agent-{STEP_ID}.jsonl, \
         which is the only place anything reads them from - so each of those rows is the same \
         line written twice and paid for twice"
    );

    // PRZED `store.close()`. Domknięcie na drodze wyjścia z aplikacji istnieje od Z-6, więc
    // pomiar po nim przechodziłby także na kodzie, który nie domyka dziennika po odbudowie.
    assert_the_journal_came_back_down(&db, before)?;

    assert_eq!(
        bodies_of(&reader, OLDER_RUN_ID)?,
        vec![OLDER_LINE.to_owned()],
        "rebuilding one run took a line away from another one. Migrations here only ever add \
         (invariant 25): rows written before this change stay where they are, and only the run \
         being rebuilt is replaced"
    );

    drop(reader);
    store.close().await?;

    // TAM, GDZIE CZYTA JE CZŁOWIEK (niezmiennik 29). Ta sama komenda, którą woła okno; wartość
    // zwrócona przez odbudowę dowodzi tylko, że mechanizm istnieje.
    let past = read_run_inner(project, FOLDER)?;
    let step = past
        .steps
        .iter()
        .find(|one| one.id == STEP_ID)
        .expect("the run has the step run.json describes");
    assert!(
        !step.lines.is_empty(),
        "the step whose transcript weighs over ten megabytes reads back with no rows at all. \
         Not indexing those lines was supposed to cost nothing, and this is where the cost would \
         show up first"
    );
    assert!(
        step.lines.iter().any(|line| line.text() == PROSE),
        "the one thing the agent actually said is missing from the run read back off disk. That \
         sentence is the difference between 'the index stopped copying the transcript' and 'the \
         transcript is gone'"
    );
    Ok(())
}

/// Bieg, do którego mają do czego należeć linie: `events.run_id` wskazuje na `runs`, a klucze
/// obce są włączone na każdym połączeniu.
fn a_run(id: &str) -> NewRun {
    NewRun {
        id: id.to_owned(),
        workflow_id: "ship-a-feature".to_owned(),
        workflow_snapshot: r#"{"nodes":[],"edges":[]}"#.to_owned(),
        title: "Fix the CSV parser".to_owned(),
        status: "running".to_owned(),
        concurrency: 1,
        created_at: 1_787_000_000_000,
        started_at: Some(1_787_000_001_000),
        ended_at: None,
        boot_id: None,
        error: None,
    }
}

/// Linia na podanym poziomie. Treść mówi, którym — po niej poznajemy, co doszło.
fn a_line(level: &str) -> RunLine {
    RunLine {
        ts: 1_787_000_002_000,
        kind: "assistant".to_owned(),
        level: level.to_owned(),
        body: format!("a {level} line"),
    }
}

/// Druga droga do tabeli `events`, i jedyna, którą cokolwiek jeszcze do niej wchodzi.
///
/// Odbudowa nie zna poziomu linii, bo nie kuruje — pompa karty zna, bo dostaje go od kuracji
/// z `engine::line`. Dlatego to tutaj kryterium „indeks rośnie tylko o nagłówki" ma swoją drugą
/// połowę, a razem z nią pułapka, którą ta zmiana zastawia sama na siebie: karta liczy linie
/// PRZYJĘTE, a nie wstawione wiersze. Liczona po wstawionych, każda karta meldowałaby
/// [`RunOutcome::Interrupted`] — czyli bieg z dziurą w transkrypcie — po każdym zdrowym biegu.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tab_carries_every_line_and_the_index_keeps_only_the_headlines() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let folder = root.path().join("meetnotes");
    fs::create_dir_all(&folder)?;

    let registry = Registry::new(Limiter::new(1));
    let tab = registry.open(&folder)?;
    let store = registry.store(&tab).ok_or_else(|| {
        anyhow::anyhow!("the registry opened the folder and kept no store for it")
    })?;
    store.writer().insert_run(a_run(PUMPED_RUN_ID)).await?;

    let mut sink = registry.attach_run(&tab, PUMPED_RUN_ID)?;
    for level in ["headline", "detail", "raw"] {
        sink.send(a_line(level)).await?;
    }
    let ended = sink.finish(RunOutcome::Succeeded).await?;

    assert_eq!(
        ended,
        RunOutcome::Succeeded,
        "the tab took three lines, carried all three and still called the run interrupted. \
         A line the index does not want is not a line that went missing - it is on disk, in the \
         step's own file, and that is where the screen reads it from"
    );
    assert_eq!(
        bodies_of(&store.reader()?, PUMPED_RUN_ID)?,
        vec!["a headline line".to_owned()],
        "the index took something other than the one headline. Detail and raw lines are the \
         27 362 rows this change is about: written beside the file they came from, and read by \
         nothing"
    );
    Ok(())
}
