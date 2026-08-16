//! AC-2 dla T-15: bieg waliduje plik jeszcze raz i przy problemie nie odpala niczego.
//!
//! *„The runner must never trust the UI"* (T3 §5.2). Zapis z płótna nie przepuściłby koła
//! (T-12 AC-7), ale plik na dysku mógł powstać inaczej: z merge'a gita, z ręcznej poprawki,
//! z nowszego builda. Między zapisem a naciśnięciem Start jest czas, a plik jest jedyną rzeczą,
//! którą użytkownik może w Loadoucie **stracić**.
//!
//! **Słaba wersja brzmi `assert!(result.is_err())`** i przechodzi dla implementacji, która
//! najpierw tworzy katalog biegu i odpala pierwszy krok, a waliduje po drodze — czyli dla tej,
//! która pali pieniądze na workflow odrzuconym pięć sekund później i zostawia po sobie pusty
//! `runs/<ts>__<id>/`. Rozróżniają je trzy rzeczy naraz: **licznik uruchomień równy zeru**,
//! **brak katalogu biegu** i przypadek z samym ostrzeżeniem, który musi wystartować.
//!
//! Zdania odmowy **nie ma w tym pliku jako literału** i to jest połowa jego wartości. Test woła
//! `workflow::check` na tej samej fiksturze i porównuje komunikat błędu z jej uwagą, więc
//! dowodzi tego, co trzeba: bieg mówi **tym samym zdaniem**, a nie własnym tłumaczeniem. Kopia
//! zdania w drugim miejscu jest zawsze tą nieaktualną.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

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
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(10);

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000c1
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

/// Koło: dwa kroki wskazujące na siebie nawzajem. Taki plik nie mógł powstać z płótna, ale mógł
/// powstać z merge'a gita — i to jest dokładnie powód, dla którego bieg sprawdza drugi raz.
const CIRCLE: &str = r#"{
  "format": 1,
  "id": "wf_circle",
  "name": "A circle",
  "steps": [
    {
      "kind": "agent",
      "id": "s_first",
      "name": "First",
      "agent": "01990000-0000-7000-8000-0000000000c1",
      "overrides": {},
      "instructions": "first",
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_second",
      "name": "Second",
      "agent": "01990000-0000-7000-8000-0000000000c1",
      "overrides": {},
      "instructions": "second",
      "at": { "x": 240, "y": 0 }
    }
  ],
  "links": [
    { "from": "s_first", "to": "s_second" },
    { "from": "s_second", "to": "s_first" }
  ]
}
"#;

/// Sam `Warning`: „Lonely" nie jest podłączony do reszty. Taki workflow wolno uruchomić —
/// ostrzeżenie nie blokuje niczego, a wyspa bywa świadoma.
///
/// „Lonely" pracuje na **własnej kopii** plików nie dla ozdoby: dwa kroki, które mogą biec
/// równocześnie i celują w ten sam folder, są odmową przy zapisie (niezmiennik 12), więc bez tego
/// ta fikstura niosłaby `Problem` i nie mówiłaby nic o ostrzeżeniach.
const WARNED: &str = r#"{
  "format": 1,
  "id": "wf_warned",
  "name": "One loose step",
  "steps": [
    {
      "kind": "agent",
      "id": "s_one",
      "name": "One",
      "agent": "01990000-0000-7000-8000-0000000000c1",
      "overrides": {},
      "instructions": "one",
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_two",
      "name": "Two",
      "agent": "01990000-0000-7000-8000-0000000000c1",
      "overrides": {},
      "instructions": "two",
      "at": { "x": 240, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_lonely",
      "name": "Lonely",
      "agent": "01990000-0000-7000-8000-0000000000c1",
      "overrides": {},
      "instructions": "lonely",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 240 }
    }
  ],
  "links": [{ "from": "s_one", "to": "s_two" }]
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_circle_is_refused_before_anything_starts() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("circle", CIRCLE)?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::default());

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch)),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: workflow.clone(),
        how_many_at_once: 2,
    };

    let refusal = match one_run(&deps, &request).await? {
        Ok(report) => {
            return Err(format!(
                "a workflow with a circle in it ran anyway and ended as {:?}",
                report.outcome
            )
            .into());
        }
        Err(refusal) => refusal.to_string(),
    };

    // Zdanie ma być TYM SAMYM zdaniem, które zwrócił walidator — nie własnym tłumaczeniem.
    // Bierzemy je stąd, gdzie mieszka, zamiast przepisywać do tego pliku.
    let file = load(&workflow)?;
    let note = check(&file)
        .into_iter()
        .find(|note| note.level == Level::Problem)
        .ok_or("the fixture stopped being circular: workflow::check found nothing to refuse")?;
    assert_eq!(
        refusal, note.message,
        "the run has to refuse with the validator's own sentence; two sentences about one thing \
         are two places to keep it right, and one of them is always out of date"
    );

    assert_eq!(
        watch.total(),
        0,
        "the driver was started {} time(s) for a workflow that was refused; validating on the way \
         is how a run spends money on a plan it rejects five seconds later",
        watch.total()
    );
    assert!(
        run_dirs(bench.project.path())?.is_empty(),
        "a refused run left {:?} behind; an empty runs/<ts>__<id>/ is a run that never happened \
         showing up in history",
        run_dirs(bench.project.path())?
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_warning_alone_does_not_stop_the_run() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("warned", WARNED)?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::default());

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch)),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: workflow.clone(),
        how_many_at_once: 2,
    };

    // Fikstura ma nieść ostrzeżenie i ANI JEDNEGO problemu — inaczej ten test mówiłby o czymś
    // innym, niż myśli.
    let file = load(&workflow)?;
    let notes = check(&file);
    assert!(
        notes.iter().any(|note| note.level == Level::Warning)
            && !notes.iter().any(|note| note.level == Level::Problem),
        "the fixture has to carry a warning and no problem at all; it carries {notes:?}"
    );

    let report = one_run(&deps, &request).await??;
    assert_eq!(
        report.steps,
        vec![
            StepState::Succeeded,
            StepState::Succeeded,
            StepState::Succeeded
        ],
        "a warning blocks nothing: all three steps have to run. They ended as {:?}",
        report.steps
    );
    assert_eq!(
        watch.total(),
        3,
        "three agent steps, three driver starts; the driver saw {}",
        watch.total()
    );
    assert_eq!(
        run_dirs(bench.project.path())?.len(),
        1,
        "a run that started has to leave exactly one directory behind"
    );
    Ok(())
}

/// Jeden bieg z limitem cierpliwości. Zewnętrzny `Result` mówi „bieg wrócił", wewnętrzny —
/// czym wrócił.
async fn one_run(
    deps: &RunDeps<'_>,
    request: &RunRequest,
) -> Result<Result<RunReport, loadout_lib::commands::RunError>, Box<dyn Error>> {
    let (tx, mut rx) = mpsc::channel::<Vec<Line>>(64);
    let drain = async move { while rx.recv().await.is_some() {} };

    let both = tokio::time::timeout(PATIENCE, async {
        tokio::join!(run_workflow_inner(deps, request, tx), drain)
    })
    .await
    .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))?;
    Ok(both.0)
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
        self.project.path().join(".loadout").join("loadout.db")
    }
}

/// Katalogi biegów pod `<projekt>/.loadout/runs/`. Brak samego `runs/` znaczy „ani jednego",
/// a nie awarię: odmowa ma nie tworzyć nawet tego katalogu.
fn run_dirs(project: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let runs = project.join(".loadout").join("runs");
    if !runs.exists() {
        return Ok(Vec::new());
    }
    let mut dirs: Vec<PathBuf> = fs::read_dir(&runs)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    Ok(dirs)
}

/// Fabryka, która dla każdego vendora oddaje ten sam dubler.
fn fake_drivers(watch: Arc<Watch>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { watch });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Obserwator sterownika: co ruszyło i ile razy.
#[derive(Debug, Default)]
struct Watch {
    runs: Mutex<Vec<String>>,
}

impl Watch {
    /// Krok wszedł do sterownika.
    ///
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym wywołaniu,
    /// więc nie ma wyrażenia, w którym dożyłby do `await`.
    fn entered(&self, prompt: &str) {
        self.lock().push(prompt.to_owned());
    }

    /// Ile razy sterownik w ogóle ruszył. Zero jest tu całą asercją.
    fn total(&self) -> usize {
        self.lock().len()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<String>> {
        self.runs.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Dubler sterownika: trzy zdarzenia na krok i wyjście zerem, natychmiast.
#[derive(Debug)]
struct Fake {
    watch: Arc<Watch>,
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
        // `RunSpec` nie niesie numeru kroku — niesie jego instrukcje, i to jest jedyne pole,
        // po którym da się kroki rozróżnić (niezmiennik 9: jadą tam jako **dane**).
        self.watch.entered(&spec.prompt);
        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };

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

        Ok(Box::new(Turn { events, session }))
    }
}

/// Jedna tura dublera.
#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<AgentEvent>,
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: String::new(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::ZERO,
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
