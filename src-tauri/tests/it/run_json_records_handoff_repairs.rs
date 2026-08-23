//! AC-3 dla T-86: `run.json` zapisuje, czy agent trzymał się umowy o kształcie odpowiedzi.
//!
//! `memory::handoff::write_handoff` oddaje `Written { repaired, truncated }` — które sekcje
//! Loadout musiał dopisać i czy musiał ciąć. Do dziś obie te liczby szły wyłącznie do
//! `tracing::debug!` w `Live::hand_over`, czyli nie widział ich NIKT: aplikacja nie ma włączonego
//! poziomu debug, a `run.json` jest jedynym miejscem, które przeżywa skasowanie `loadout.db`
//! (niezmiennik 4). Artefakt liczony i nieczytany jest dokładnie tym, czego zabrania
//! niezmiennik 21.
//!
//! # Co to zmienia dla człowieka
//!
//! „Agent nie oddał umówionego kształtu" jest z zewnątrz nieodróżnialne od „agent oddał kształt,
//! a Loadout go zgubił". Pierwsze naprawia się jednym zdaniem w prompcie kroku, drugie jest wadą
//! produktu — a bez tego pola obie wyglądają identycznie: przekazanie na dysku ma trzy nagłówki
//! w obu przypadkach, bo `reshape()` je dopisuje.
//!
//! # SŁABA WERSJA numer jeden: jeden krok
//!
//! Implementacja wpisująca stałą — pustą listę i `false`, albo trzy nagłówki zawsze — przechodzi
//! każde kryterium sądzące JEDEN krok. Ławka ma więc trzy kroki jednego biegu i trzy różne
//! odpowiedzi: kształt umówiony, goła proza i odpowiedź dłuższa niż `BODY_CAP`. Każda z nich musi
//! dać w pliku co innego.
//!
//! # SŁABA WERSJA numer dwa: sądzić `Written`, a nie plik
//!
//! `write_handoff` już dziś zwraca obie wartości i już dziś są prawdziwe — kryterium pytające
//! funkcję byłoby zielone przed napisaniem jednej linii. Pytanie brzmi, czy fakt DOJEŻDŻA do
//! pliku, który zostaje po biegu. Dlatego wszystko niżej stoi na `run.json`.
//!
//! # Addytywność, sprawdzona odczytem, nie deklaracją
//!
//! Nowe pole ma być dopiskiem: plik zapisany przez starszego Loadouta (czyli bez niego) musi się
//! dalej czytać, a `store::rebuild` nie ma prawa zgubić na nim ani jednego kroku. Kryterium
//! odbudowuje indeks **dwa razy** — raz z pliku, który to pole niesie, raz z tego samego pliku
//! z wyciętym polem — i wymaga tych samych wierszy. To jest jedyna forma tej asercji, której nie
//! da się przejść komentarzem o `#[serde(default)]`.
//!
//! # Czego to kryterium NIE rozstrzyga
//!
//! Jak ten zapis nazywa się w pliku. Jeden klucz z dwoma polami czy dwa klucze obok siebie —
//! obie formy odpowiadają na to samo pytanie i obie przechodzą. Rozstrzygane jest to, że
//! `repaired` wymienia dopisane nagłówki po nazwie, że `truncated` mówi prawdę o cięciu, i że
//! przy pustym jednym i fałszywym drugim w pliku nie przybywa ani jeden klucz.

// `unwrap()`/`expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
//
// `too_many_lines` **wyłącznie dodane**: pięć punktów tego kryterium mierzy JEDEN bieg trzech
// kroków i dwie odbudowy tego samego katalogu. Cięcie po granicy funkcji znaczyłoby trzy osobne
// biegi albo stan dzielony między testami, które cargo uruchamia równolegle.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::memory::handoff::BODY_CAP;
use loadout_lib::store::Store;
use rusqlite::Connection;
use serde_json::Value;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera. Nie „claude" i nie „codex": tamte dwie mają w biegu własne wymagania
/// co do dowodów, a to kryterium sądzi plik biegu, nie sterownik.
const VENDOR: &str = "fake";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(60);

/// Trzy nagłówki, o które prosi umowa — po nazwie, małymi literami.
const THE_THREE: [&str; 3] = ["answer", "evidence", "open"];

/// Agent trzech kroków tej ławki. Jeden, bo to kryterium sądzi ODPOWIEDŹ, nie konfigurację.
const HAND: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000e3
name: Hand
summary: Does the work
color: moss
runsWith: claude-code
model: opus
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 20
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Do the work.
";

/// Instrukcja kroku → nazwa kroku. Krok rozpoznajemy po treści zadania, bo `RunSpec` nie niesie
/// nazwy kroku, a instrukcja jest tym, co ten krok naprawdę dostał.
const STEPS: [(&str, &str); 3] = [("tidy: ", "Tidy"), ("messy: ", "Messy"), ("long: ", "Long")];

/// Krok, który oddaje trzy sekcje w dobrej kolejności i mieści się w limicie.
const TIDY: &str = "Tidy";
/// Krok, który oddaje gołą prozę — najczęstsza rzecz, jaką przyśle model [T6 §11.1].
const MESSY: &str = "Messy";
/// Krok, który oddaje umówiony kształt, ale dłuższy niż [`BODY_CAP`].
const LONG: &str = "Long";

/// Trzy kroki w łańcuchu, trzy różne odpowiedzi.
///
/// Każdy krok na WŁASNEJ KOPII plików: dwa kroki piszące po tych samych ścieżkach są odmową
/// `check_to_run` (niezmiennik 12), a nie fiksturą.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_records_repairs",
  "name": "Three answers, three shapes",
  "steps": [
    {
      "kind": "agent",
      "id": "s_tidy",
      "name": "Tidy",
      "agent": "01990000-0000-7000-8000-0000000000e3",
      "overrides": {},
      "instructions": "tidy: answer exactly the way you were asked to.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_messy",
      "name": "Messy",
      "agent": "01990000-0000-7000-8000-0000000000e3",
      "overrides": {},
      "instructions": "messy: answer in one paragraph and no headings.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_long",
      "name": "Long",
      "agent": "01990000-0000-7000-8000-0000000000e3",
      "overrides": {},
      "instructions": "long: answer the right way and at great length.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 480, "y": 0 }
    }
  ],
  "links": [
    { "from": "s_tidy", "to": "s_messy" },
    { "from": "s_messy", "to": "s_long" }
  ]
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_run_file_says_what_loadout_had_to_do_with_each_answer() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND)?;
    let workflow = bench.workflow("records-repairs", WORKFLOW)?;
    let store = Store::open(&bench.db())?;

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 2,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))??;
    let _ = tokio::time::timeout(PATIENCE, pump).await;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; STEPS.len()],
        "all {} steps have to finish, or nothing below was ever handed over. They ended as {:?}",
        STEPS.len(),
        report.steps
    );

    let run_file = report.dir.join("run.json");
    let run: Value = serde_json::from_str(&fs::read_to_string(&run_file)?)?;

    // ── (a) UMÓWIONY KSZTAŁT: W PLIKU NIE PRZYBYWA ANI JEDEN KLUCZ ──────────────────────────
    // Stoi pierwsza, bo implementacja zapisująca pusty rekord każdemu krokowi przechodzi (b)
    // i (c) — i dokłada klucz do KAŻDEGO kroku każdego biegu w historii, żeby powiedzieć „nic
    // się nie stało".
    let tidy = step_named(&run, TIDY).ok_or("run.json has no step called Tidy")?;
    assert_eq!(
        what_loadout_had_to_do(tidy),
        None,
        "this agent answered exactly as asked and its step still carries a record of repairs. \
         Nothing was added and nothing was cut, so there is nothing to say — and a key saying \
         \"nothing\" on every step of every run is length paid for silence. The entry was: {tidy}"
    );

    // ── (b) GOŁA PROZA: WYMIENIONE DOPISANE NAGŁÓWKI ────────────────────────────────────────
    let messy = step_named(&run, MESSY).ok_or("run.json has no step called Messy")?;
    let (repaired, truncated) = what_loadout_had_to_do(messy).ok_or_else(|| {
        format!(
            "this agent answered in bare prose, so Loadout wrote all three headings for it, and \
             the run file says nothing about it. Then \"the agent did not answer as asked\" and \
             \"Loadout lost the shape\" look identical from the outside. The entry was: {messy}"
        )
    })?;
    assert_eq!(
        repaired, THE_THREE,
        "the record has to NAME the headings Loadout added, in the order it added them. A count \
         alone sends the human to open the file and diff it by eye. The entry was: {messy}"
    );
    assert!(
        !truncated,
        "this answer is far shorter than the {BODY_CAP} byte limit and the run file says it was \
         cut. A record that reports work Loadout never did is worse than no record: it sends \
         somebody looking for the missing half of an answer that is whole"
    );

    // ── (c) ZA DŁUGA ODPOWIEDŹ: CIĘTA, ALE NIE NAPRAWIANA ───────────────────────────────────
    // Te dwie wartości są niezależne i ten krok jest jedynym, który to pokazuje: kształt był
    // umówiony (`repaired` puste), a mimo to część treści leży w `attachments/`.
    let long = step_named(&run, LONG).ok_or("run.json has no step called Long")?;
    let (repaired, truncated) = what_loadout_had_to_do(long).ok_or_else(|| {
        format!(
            "this answer is longer than the {BODY_CAP} byte limit, so part of it lives in \
             attachments/ and the next step will not see it in the file it is pointed at. The \
             run file says nothing about that. The entry was: {long}"
        )
    })?;
    assert!(
        truncated,
        "the answer was cut and the run file says it was not. The entry was: {long}"
    );
    assert!(
        repaired.is_empty(),
        "this agent gave all three headings in the right order, so Loadout added none — and the \
         run file lists {repaired:?}. Cutting and repairing are two different things that happen \
         to two different halves of the agreement"
    );

    // ── (d) ODBUDOWA CZYTA TEN PLIK ─────────────────────────────────────────────────────────
    let indexed = rebuild_and_read(&bench.db_named("rebuilt-new.db"), &report.dir).await?;
    assert_eq!(
        indexed.len(),
        STEPS.len(),
        "rebuilding the index from this run directory found {} steps, not {}. run.json is the \
         truth about a run (invariant 4), so a field it carries may never cost a row: {indexed:?}",
        indexed.len(),
        STEPS.len()
    );

    // ── (e) …I CZYTA TAK SAMO PLIK, KTÓRY TEGO POLA NIE MA ──────────────────────────────────
    // Czyli plik zapisany przez każdego Loadouta sprzed tego zadania. Ta asercja jest jedynym
    // sposobem, żeby „pole jest addytywne" znaczyło coś więcej niż atrybut w kodzie.
    let mut older = run.clone();
    forget_the_record(&mut older);
    assert_ne!(
        older, run,
        "the fixture is wrong if stripping the record changes nothing — then (e) proves only \
         that the same file reads twice"
    );
    fs::write(&run_file, serde_json::to_string_pretty(&older)?)?;

    let from_the_older = rebuild_and_read(&bench.db_named("rebuilt-old.db"), &report.dir).await?;
    assert_eq!(
        from_the_older, indexed,
        "a run.json written before this field existed rebuilds into different rows than the same \
         file with it. Every run in every project's history was written without it, and a reader \
         that needs it turns them all into rows nobody can open"
    );

    Ok(())
}

/// Wpis kroku o tej nazwie, prosto z `run.json`.
fn step_named<'a>(run: &'a Value, name: &str) -> Option<&'a Value> {
    run.get("steps")?
        .as_array()?
        .iter()
        .find(|step| step.get("name").and_then(Value::as_str) == Some(name))
}

/// Co Loadout musiał zrobić z odpowiedzią tego kroku — dopisane nagłówki i czy ciął.
///
/// KRYTERIUM NIE ROZSTRZYGA, JAK TEN ZAPIS NAZYWA SIĘ W PLIKU. Jeden klucz z dwoma polami i dwa
/// klucze obok siebie odpowiadają na to samo pytanie; nazwa jest sprawą tego, kto pisze, a nie
/// tego, kto sądzi. `None` znaczy „ten wpis nie mówi o tym nic", i to jest inna odpowiedź niż
/// „mówi, że nic się nie stało".
fn what_loadout_had_to_do(step: &Value) -> Option<(Vec<String>, bool)> {
    if let Some(found) = record_in(step) {
        return Some(found);
    }
    step.as_object()?.values().find_map(record_in)
}

/// Ten sam zapis, kiedy stoi wprost w tym obiekcie.
///
/// Nazwy nagłówków porównujemy MAŁYMI LITERAMI: `Answer` i `answer` to ta sama sekcja, a wybór
/// między nimi jest sprawą serializacji, nie kontraktu.
fn record_in(value: &Value) -> Option<(Vec<String>, bool)> {
    let fields = value.as_object()?;
    if !fields.contains_key("repaired") && !fields.contains_key("truncated") {
        return None;
    }
    let repaired = fields
        .get("repaired")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_lowercase)
                .collect()
        })
        .unwrap_or_default();
    let truncated = fields
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Some((repaired, truncated))
}

/// Ten sam `run.json`, tylko bez zapisu o naprawach — czyli tak, jak wyglądał przed T-86.
fn forget_the_record(run: &mut Value) {
    let Some(steps) = run.get_mut("steps").and_then(Value::as_array_mut) else {
        return;
    };
    for step in steps {
        let Some(fields) = step.as_object_mut() else {
            continue;
        };
        fields.remove("repaired");
        fields.remove("truncated");
        fields.retain(|_, value| record_in(value).is_none());
    }
}

/// Odbudowuje indeks z katalogu biegu do ŚWIEŻEJ bazy i oddaje kroki, które w niej wylądowały.
///
/// Świeżej, bo bieg zapisał już swoje wiersze do własnej bazy, a `UNIQUE (run_id, node_key)`
/// odmówiłby drugiego wstawienia tego samego biegu — i wyglądałoby to jak wada odbudowy.
async fn rebuild_and_read(db: &PathBuf, run_dir: &PathBuf) -> Result<Vec<String>, Box<dyn Error>> {
    let store = Store::open(db)?;
    store.rebuild_from(run_dir).await?;
    let rows = {
        let reader = store.reader()?;
        steps_in(&reader)?
    };
    store.close().await?;
    Ok(rows)
}

/// Kroki z indeksu, jeden wiersz na krok, w kolejności klucza węzła.
fn steps_in(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT node_key, name, status, summary FROM steps ORDER BY node_key")?;
    let rows = stmt.query_map([], |row| {
        let node_key: String = row.get(0)?;
        let name: String = row.get(1)?;
        let status: String = row.get(2)?;
        let summary: Option<String> = row.get(3)?;
        Ok(format!("{node_key} · {name} · {status} · {summary:?}"))
    })?;
    rows.collect()
}

// ── trzy odpowiedzi ────────────────────────────────────────────────────────────────────────

/// Co ten krok oddaje jako swoją ostatnią wypowiedź.
fn answer_from(step: &str) -> String {
    match step {
        MESSY => "The list rebuilds cleanly and nothing else changed. I did not open the second \
                  file, so I cannot say anything about it."
            .to_owned(),
        LONG => a_long_answer(),
        _ => "## Answer\nThe list rebuilds cleanly.\n\n## Evidence\nnotes.txt:1\n\n## Open\n\
              nothing.\n"
            .to_owned(),
    }
}

/// Umówiony kształt, tylko dłuższy niż limit ciała — więc cięty, ale nie naprawiany.
///
/// Długość liczona z [`BODY_CAP`], a nie wpisana liczbą: fikstura, która przestaje przekraczać
/// limit po jego podniesieniu, przestaje o cokolwiek pytać i nikt tego nie zauważy.
fn a_long_answer() -> String {
    let line = "notes.txt:1 - the same measured fact, written out once more so that the body of \
                this answer does not fit into the limit.\n";
    let mut body = String::from("## Answer\nThe list rebuilds cleanly.\n\n## Evidence\n");
    while body.len() < BODY_CAP + 2048 {
        body.push_str(line);
    }
    body.push_str("\n## Open\nnothing.\n");
    body
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers() -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake);
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct Fake;

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        VENDOR
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some(VENDOR.to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        // Zadanie, którego nie ma w tablicy, dostaje odpowiedź w umówionym kształcie: asercje
        // o krokach mają wtedy paść na tym, czego test nie rozpoznał, a nie na cudzej treści.
        let step = STEPS
            .iter()
            .find(|(instruction, _)| spec.prompt.starts_with(instruction))
            .map_or(TIDY, |(_, name)| *name);

        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };
        let _ = events
            .send(
                (AgentEvent::Started {
                    session: session.clone(),
                    model: spec.model.clone().unwrap_or_default(),
                    tools: Vec::new(),
                    capabilities: Vec::new(),
                })
                .into(),
            )
            .await;

        Ok(Box::new(Turn {
            events,
            session,
            said: answer_from(step),
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    said: String,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<loadout_lib::engine::supervisor::GroupId> {
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: self.said.clone(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::ZERO,
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send((AgentEvent::Finished(outcome.clone())).into())
            .await;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> loadout_lib::engine::supervisor::GroupProof {
        loadout_lib::engine::supervisor::GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

// ── ławka ──────────────────────────────────────────────────────────────────────────────────

struct Bench {
    home: TempDir,
    project: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        // `Store::open` zakłada plik bazy, ale nie katalog nad nim.
        fs::create_dir_all(project.path().join(".loadout"))?;
        // Żeby „własna kopia twoich plików" miała co kopiować.
        fs::write(project.path().join("notes.txt"), "written by the human")?;
        Ok(Self { home, project })
    }

    fn agent(&self, slug: &str, text: &str) -> Result<(), Box<dyn Error>> {
        fs::write(
            self.home.path().join("agents").join(format!("{slug}.md")),
            text,
        )?;
        Ok(())
    }

    fn workflow(&self, slug: &str, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self
            .home
            .path()
            .join("workflows")
            .join(format!("{slug}.json"));
        fs::write(&path, text)?;
        Ok(path)
    }

    fn db(&self) -> PathBuf {
        self.db_named("loadout.db")
    }

    fn db_named(&self, name: &str) -> PathBuf {
        self.project.path().join(".loadout").join(name)
    }
}
