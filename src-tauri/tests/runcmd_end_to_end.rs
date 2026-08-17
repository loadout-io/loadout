//! AC-1 dla T-15: plik z płótna przechodzi przez silnik i wychodzi jako linie, w kolejności grafu.
//!
//! To jest kryterium, dla którego całe zadanie istnieje: nic tu nie jest nową zdolnością, a mimo
//! to tylko tutaj widać, czy zdolności zbudowane osobno do siebie pasują.
//!
//! **Słaba wersja brzmi `assert!(!lines.is_empty())`** i przechodzi dla implementacji, która woła
//! sterownik dla obu kroków **równolegle**, ignorując strzałkę — czyli dla tej, w której `build`
//! startuje, zanim `plan` cokolwiek napisał, a cały graf jest dekoracją. Rozróżniają je dwie
//! rzeczy: porównanie znaczników czasu (b) i kształt klucza na drucie (c), którego nie spełni
//! żadne `#[derive(Serialize)]` bez `rename_all` [04 §2.5 — brak `rename_all_fields` na enumie
//! niosącym dane położył kiedyś cały ekran].
//!
//! **Znacznik czasu jest chwilą ODBIORU paczki na kanale**, bo `Line` żadnego nie niesie i nie ma
//! nieść: kolejność na `mpsc` jest zachowana, więc chwila odbioru niesie to samo uporządkowanie,
//! co chwila powstania. Implementacja, która zbiera wszystkie linie i wysyła je jedną paczką na
//! koniec biegu, pada tu na równych znacznikach — i ma paść, bo taki bieg nie pokazuje na ekranie
//! niczego, dopóki się nie skończy (`docs/ARCHITECTURE.md` §4).
//!
//! **Nazwa agenta i nazwa kroku są w fiksturze te same** („Planner", „Builder") z rozmysłem:
//! `Line::agent` idzie wprost na ekran, więc kryterium nie ma powodu rozstrzygać za implementację,
//! czy etykietą wiersza jest nazwa roli, czy nazwa kafelka. Rozstrzyga tylko to, że **nie jest
//! nią żargon** — identyfikator kroku ani uuid agenta na ekranie nie mają czego szukać
//! (niezmiennik 14).
//!
//! Dubler sterownika stoi w tym pliku, a nie w `engine/drivers/fake.rs` (należy do T-02): tamten
//! jest dublerem **kroku planisty** — nie implementuje `AgentDriver`, nie emituje zdarzeń i nie ma
//! na nim czego zabić. TASK.md przewiduje dokładnie ten przypadek („owiń go obserwatorem w pliku
//! testowym").

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, FinishReason, Outcome as TurnOutcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use serde_json::Value as Json;
use tauri::ipc::{Channel, InvokeResponseBody};
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Ile trwa jedna tura dublera. Krótko, ale nie zero: dwa kroki muszą dać się od siebie odróżnić
/// na osi czasu, a zero znaczyłoby, że oba wpadają w tę samą milisekundę.
const TURN: Duration = Duration::from_millis(60);

/// Ile czekamy na cały bieg, zanim uznamy go za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(10);

/// Etykieta pierwszego kroku: nazwa kafelka **i** nazwa agenta.
const PLAN: &str = "Planner";
/// Etykieta drugiego kroku.
const BUILD: &str = "Builder";

const PLANNER_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000a1
name: Planner
summary: Lays out the work
color: slate
runsWith: claude-code
model: opus
thinking: balanced
fileAccess: look-only
giveUpAfterMinutes: 20
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Lay out the work.
";

const BUILDER_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000b1
name: Builder
summary: Writes the change
color: clay
runsWith: claude-code
model: sonnet
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 20
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Write the change.
";

/// Dwa kroki połączone **jedną** strzałką — kształt z T3 §3.1, pisany ręcznie.
///
/// Fikstura zbudowana naszym serializatorem definiowałaby kształt, zamiast go sprawdzać: zmiana
/// kształtu przechodziłaby wtedy po obu stronach naraz [04 §6.4].
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_plan_then_build",
  "name": "Plan then build",
  "steps": [
    {
      "kind": "agent",
      "id": "s_plan",
      "name": "Planner",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "plan",
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_build",
      "name": "Builder",
      "agent": "01990000-0000-7000-8000-0000000000b1",
      "overrides": {},
      "instructions": "build",
      "at": { "x": 240, "y": 0 }
    }
  ],
  "links": [{ "from": "s_plan", "to": "s_build" }]
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_file_from_the_canvas_comes_out_as_lines_in_graph_order() -> Result<(), Box<dyn Error>> {
    let (report, seen, bench) = plan_then_build().await?;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded, StepState::Succeeded],
        "both steps have to finish for the lines to mean anything; they ended as {:?}",
        report.steps
    );

    both_steps_spoke(&seen);
    the_arrow_means_after(&seen)?;
    every_key_is_camel_case(&seen)?;
    the_run_left_a_directory(&report, bench.project.path())?;
    Ok(())
}

/// (a) Na kanale pojawiły się linie **obu** kroków.
fn both_steps_spoke(seen: &[(Instant, Json)]) {
    for step in [PLAN, BUILD] {
        assert!(
            seen.iter().any(|(_, line)| agent_of(line) == step),
            "not one line came out for \"{step}\"; the channel carried {:?}",
            labels(seen)
        );
    }
}

/// (b) **Pierwsza** linia `build` jest późniejsza niż **ostatnia** linia `plan`.
fn the_arrow_means_after(seen: &[(Instant, Json)]) -> Result<(), Box<dyn Error>> {
    let last_plan = at_of(seen, PLAN)
        .last()
        .ok_or("\"Planner\" never reached the channel, so there is nothing to order against")?;
    let first_build = at_of(seen, BUILD)
        .next()
        .ok_or("\"Builder\" never reached the channel, so there is nothing to order")?;

    assert!(
        first_build > last_plan,
        "an arrow means \"after\", not \"beside\": the first \"{BUILD}\" line arrived {:?} \
         before the last \"{PLAN}\" line. Equal instants mean every line was sent in one batch \
         once the run was over — a run whose screen stays empty until it ends is the same defect \
         (docs/ARCHITECTURE.md §4)",
        last_plan.saturating_duration_since(first_build)
    );
    Ok(())
}

/// (c) Każda `Line` na drucie ma klucze wyłącznie w camelCase.
fn every_key_is_camel_case(seen: &[(Instant, Json)]) -> Result<(), Box<dyn Error>> {
    let mut compound = 0usize;
    for (_, wire) in seen {
        let snake = underscored(wire);
        assert!(
            snake.is_empty(),
            "a line went on the wire with {snake:?}; the front end reads camelCase only, so \
             `detail_id` and `duration_ms` arrive there as `undefined` and take the screen down \
             — and the first six fixes go into the view, because that is where the symptom is \
             [00-SYNTHESIS §3]"
        );
        compound += usize::from(has_compound_key(wire));
    }

    assert!(
        compound > 0,
        "not one line carried a key made of two words, so \"no underscores\" proved nothing \
         here: `kind` and `text` read the same under every naming rule. A finished run has to \
         emit at least one row with a compound key — `durationMs` on the closing line"
    );
    Ok(())
}

/// (d) Katalog biegu istnieje i ma w środku `run.json` oraz `logs/`.
fn the_run_left_a_directory(report: &RunReport, project: &Path) -> Result<(), Box<dyn Error>> {
    let dir = only_run_dir(project)?;
    assert_eq!(
        dir, report.dir,
        "the run reported one directory and left another one on disk"
    );

    let name = dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the run directory has a name that is not text")?;
    let (stamp, id) = name.split_once("__").ok_or(
        "a run directory is named <ts>__<id> (docs/ARCHITECTURE.md §8); this one has no `__`",
    )?;
    assert!(!stamp.is_empty(), "the <ts> half of {name} is empty");
    assert_eq!(
        id, report.id,
        "the <id> half of the directory name has to be the run's own id, or history sorts by a \
         number that names nothing"
    );

    assert!(
        dir.join("run.json").is_file(),
        "{} has no run.json — files are the truth and the database is only its index \
         (invariant 4)",
        dir.display()
    );
    assert!(
        dir.join("logs").is_dir(),
        "{} has no logs/ — the raw stream is teed to disk untouched (ARCHITECTURE §4)",
        dir.display()
    );
    let text = fs::read_to_string(dir.join("run.json"))?;
    let _: Json = serde_json::from_str(&text)?;
    Ok(())
}

/// Chwile odbioru linii tego kroku, w kolejności.
fn at_of<'a>(seen: &'a [(Instant, Json)], step: &'a str) -> impl Iterator<Item = Instant> + 'a {
    seen.iter()
        .filter(move |(_, line)| agent_of(line) == step)
        .map(|(at, _)| *at)
}

/// Etykiety wszystkich linii — do komunikatu, kiedy asercja padnie.
fn labels(seen: &[(Instant, Json)]) -> Vec<&str> {
    seen.iter().map(|(_, line)| agent_of(line)).collect()
}

/// Etykieta wiersza, przeczytana z drutu.
///
/// `Line::agent()` po tej stronie granicy nie istnieje — przez kanał przechodzi JSON — a klucz
/// nazywa się `agent` w każdym wariancie (`engine::line`, `rename_all_fields`). Wiersz bez tego
/// klucza dostaje pustą etykietę i nie pasuje do żadnego kroku, więc milcząco nie przechodzi
/// zamiast milcząco przechodzić.
fn agent_of(line: &Json) -> &str {
    line.get("agent").and_then(Json::as_str).unwrap_or("")
}

/// Klucze z podkreśleniem, na dowolnej głębokości.
fn underscored(value: &Json) -> Vec<String> {
    match value {
        Json::Object(fields) => fields
            .iter()
            .flat_map(|(key, child)| {
                let mut found = underscored(child);
                if key.contains('_') {
                    found.push(key.clone());
                }
                found
            })
            .collect(),
        Json::Array(items) => items.iter().flat_map(underscored).collect(),
        _ => Vec::new(),
    }
}

/// Czy obiekt niesie klucz złożony z dwóch słów — czyli taki, na którym reguła nazewnicza
/// w ogóle daje się złamać.
fn has_compound_key(value: &Json) -> bool {
    value.as_object().is_some_and(|fields| {
        fields
            .keys()
            .any(|key| key.contains('_') || key.chars().any(char::is_uppercase))
    })
}

/// Jeden bieg fikstury: raport, linie ze znacznikami odbioru i katalogi, które muszą go przeżyć.
async fn plan_then_build() -> Result<(RunReport, Vec<(Instant, Json)>, Bench), Box<dyn Error>> {
    let bench = Bench::new()?;
    let planner = bench.agent("planner", PLANNER_FILE)?;
    let builder = bench.agent("builder", BUILDER_FILE)?;
    let workflow = bench.workflow("plan-then-build", WORKFLOW)?;
    the_fixture_can_run(&workflow, &[&planner, &builder])?;
    let store = Store::open(&bench.db())?;

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(TURN),
        control: RunControl::new(),
    };
    // Limit dwóch naraz jest tu z rozmysłem: gdyby strzałka nie znaczyła „po", nic w tym biegu nie
    // powstrzymałoby obu kroków przed wystartowaniem razem. Kolejność ma pochodzić z grafu.
    let request = RunRequest {
        workflow,
        how_many_at_once: 2,
    };

    // 2026-08-17 (T-30) — bieg oddaje linie POJEDYNCZO do `LineSink`, a sklejaniem zajmuje się
    // pompa po drugiej stronie, więc kanał zakłada się tutaj tak, jak zakłada go komenda:
    // `line_channel` + `spawn_pump`. Znacznik czasu zostaje tym, czym był — **chwilą odbioru
    // paczki**, stemplowaną tam, gdzie stemplowało ją `rx.recv()`.
    //
    // Wiersze wracają jako JSON, a nie jako `Line`, i to nie jest wybór: `Channel` serializuje
    // paczkę przy wysyłce, a `Line` jest typem WYJŚCIOWYM i nie ma `Deserialize`. Dopisanie mu
    // derive'u to zmiana w `src-tauri/src/engine/line.rs`, którego T-30 nie posiada — czyli
    // pytanie do człowieka, nie cichy dopisek w cudzym pliku (AGENTS.md §7). Asercje pytają
    // dokładnie o to, o co pytały, tylko teraz o bajty, które NAPRAWDĘ przeszły granicę,
    // zamiast o ich powtórną serializację.
    let recorder = Delivered::default();
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, recorder.channel());
    // Pompa kończy się sama, kiedy zniknie ostatni nadajnik — a ten ginie razem z powrotem
    // biegu. Czekanie na nią stoi w `join!` dokładnie tam, gdzie stało czytanie kanału, więc
    // ostatnia, niepełna paczka zdąży wyjść, zanim ktokolwiek spyta o wiersze.
    let collect = async move {
        let _ = pump.await;
    };

    let both = tokio::time::timeout(PATIENCE, async {
        tokio::join!(run_workflow_inner(&deps, &request, sink), collect)
    })
    .await
    .map_err(|_| format!("the run did not finish within {PATIENCE:?}"))?;

    Ok((both.0?, recorder.lines()?, bench))
}

/// Paczki, które **naprawdę wyszły kanałem**, każda ze swoją chwilą odbioru.
///
/// Nagrywamy po stronie okna, a nie po stronie kolejki do pompy: to, co bieg oddał `LineSink`,
/// mówi wyłącznie o intencji, a pytanie tego kryterium brzmi „co dojechało i w jakiej
/// kolejności".
#[derive(Debug, Clone, Default)]
struct Delivered(Arc<Mutex<Vec<(Instant, InvokeResponseBody)>>>);

impl Delivered {
    /// Kanał, który pompa dostanie zamiast webviewa.
    fn channel(&self) -> Channel<Vec<Line>> {
        let seen = Arc::clone(&self.0);
        Channel::new(move |body| {
            // Chwila odbioru bierze się PRZED zamkiem: czekanie na zamek jest kosztem nagrywarki,
            // a nie opóźnieniem paczki.
            let at = Instant::now();
            // `std::sync::Mutex` w domknięciu SYNCHRONICZNYM: nie ma tu `await`, więc
            // niezmiennik 8 stoi z konstrukcji, a nie z uwagi w komentarzu.
            if let Ok(mut seen) = seen.lock() {
                seen.push((at, body));
            }
            Ok(())
        })
    }

    /// Wszystkie dostarczone wiersze, rozsypane z paczek, każdy z chwilą odbioru SWOJEJ paczki.
    fn lines(&self) -> Result<Vec<(Instant, Json)>, Box<dyn Error>> {
        let seen = self
            .0
            .lock()
            .map_err(|error| format!("the recorder was poisoned: {error}"))?;
        let mut out = Vec::new();
        for (at, body) in seen.iter() {
            let batch = body.clone().deserialize::<Vec<Json>>()?;
            out.extend(batch.into_iter().map(|line| (*at, line)));
        }
        Ok(out)
    }
}

/// Fikstura ma przejść walidator **bez ani jednego problemu**, a jej pliki agentów mają dać się
/// przeczytać.
///
/// To nie jest część kryterium, tylko jego przesłanka, i dlatego stoi przed biegiem. Czerwień
/// w fazie kontraktu wygląda identycznie dla „zachowania jeszcze nie ma" i dla „tego kryterium
/// nie da się spełnić nigdy": workflow, który `workflow::check` odrzuca, byłby odmową w KAŻDEJ
/// implementacji, a test nazywałby to brakiem zachowania.
fn the_fixture_can_run(workflow: &Path, agents: &[&Path]) -> Result<(), Box<dyn Error>> {
    let problems: Vec<String> = check(&load(workflow)?)
        .into_iter()
        .filter(|note| note.level == Level::Problem)
        .map(|note| note.message)
        .collect();
    assert!(
        problems.is_empty(),
        "the fixture would be refused before it ran, so this criterion could never pass: \
         {problems:?}"
    );
    for agent in agents {
        read_agent_file(agent).map_err(|error| format!("{}: {error}", agent.display()))?;
    }
    Ok(())
}

/// Biblioteka użytkownika i projekt na czas jednego kryterium.
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
        Ok(Self { home, project })
    }

    fn agent(&self, slug: &str, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.home.path().join("agents").join(format!("{slug}.md"));
        fs::write(&path, text)?;
        Ok(path)
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
        self.project.path().join(".loadout").join("loadout.db")
    }
}

/// Jedyny katalog biegu pod `<projekt>/.loadout/runs/`.
fn only_run_dir(project: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let runs = project.join(".loadout").join("runs");
    let mut dirs: Vec<PathBuf> = fs::read_dir(&runs)
        .map_err(|error| format!("{} could not be read: {error}", runs.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    match dirs.as_slice() {
        [only] => Ok(only.clone()),
        other => Err(format!(
            "expected exactly one run directory under {}, found {}",
            runs.display(),
            other.len()
        )
        .into()),
    }
}

/// Fabryka, która dla każdego vendora oddaje ten sam dubler.
fn fake_drivers(hold: Duration) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { hold });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler sterownika: trzy zdarzenia na krok i wyjście zerem.
#[derive(Debug)]
struct Fake {
    /// Ile trwa jedna tura. **Prawdziwy sen, nie czas wirtualny**: `start_paused` implikuje
    /// runtime jednowątkowy i przeskakuje zegar, kiedy runtime staje bezczynny, więc kolejność
    /// w czasie przestałaby cokolwiek znaczyć [T7 §8.1].
    hold: Duration,
}

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
        events: mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };

        // Dwa z trzech zdarzeń. Trzecie (`Finished`) jest dokładnie jedno na turę i wychodzi
        // z `wait`, bo dopiero tam wiadomo, czym tura się skończyła.
        let _ = events
            .send(AgentEvent::Started {
                session: session.clone(),
                model: spec.model.clone().unwrap_or_default(),
                tools: Vec::new(),
                capabilities: Vec::new(),
            })
            .await;
        let _ = events
            .send(AgentEvent::Said {
                text: format!("working on {}", spec.prompt),
            })
            .await;

        Ok(Box::new(Turn {
            events,
            session,
            hold: self.hold,
        }))
    }
}

/// Jedna tura dublera.
#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<AgentEvent>,
    session: SessionRef,
    hold: Duration,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        // Dubler nie ma procesu, więc nie ma grupy. Zmyślony `pgid` byłby liczbą, po której
        // sprzątanie z T-20 strzelałoby w cudzy proces.
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        tokio::time::sleep(self.hold).await;
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: String::new(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: self.hold,
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send(AgentEvent::Finished(outcome.clone()))
            .await;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}
