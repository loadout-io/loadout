//! AC-4 dla T-94: krok „sprawdź" bierze miejsce **ciężkie**, więc dwa takie kroki biegną po kolei.
//!
//! Niezmiennik 26 mówi to jednym zdaniem i mówi to o tej maszynie: dwa `cargo`/`rustc` naraz
//! przypinają kompresor pamięci macOS i zamrażają laptopa przy zerowym swapie. Do 2026-08-24
//! egzekwowała go wyłącznie ludzka dyscyplina przy pisaniu promptów — `Weight::Heavy`
//! i `Limiter::with_heavy` istniały, były przetestowane (`heavy_step_takes_its_own_slot.rs`)
//! i **nie miały ani jednego produkcyjnego wołającego**: produkt wołał `dispatch()` dla każdego
//! rodzaju kroku, a komentarz przy `a_slot_for_this_step` przyznawał to wprost.
//!
//! **Dowód jest o KOLEJNOŚCI W CZASIE, nie o tym, że oba się skończyły.** Dwa kroki „sprawdź"
//! gotowe naraz zapisują cztery znaczniki do jednego pliku, w kolejności, w której naprawdę
//! się wydarzyły. `A-start, A-end, B-start, B-end` znaczy po kolei; `A-start, B-start, …`
//! znaczy dwa `cargo` naraz — czyli dokładnie to, przed czym stoi ten niezmiennik.
//!
//! **Drugi przypadek jest kontrolą i bez niego kryterium jest wręcz szkodliwe.** Sufit „nigdy
//! obok siebie" spełnia najlepiej implementacja, która nie zrównolegla NICZEGO — a to jest
//! śmierć całej przesłanki produktu (niezmiennik 11). Krok agenta stojący obok tych dwóch musi
//! więc naprawdę pracować RAZEM z którymś z nich: miejsce ciężkie jest węższym limitem
//! **wewnątrz** puli, nie drugą pulą obok niej.
//!
//! Trzeci przypadek pyta aplikację, ile jest tych miejsc ciężkich. Jedynka ma stać w jednym
//! miejscu — tam, gdzie powstaje pula całej aplikacji — bo pula z dwoma miejscami ciężkimi
//! spełnia oba pomiary wyżej i nie egzekwuje niczego.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::limits::Limiter;
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{AppState, LineSink, QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Ile miejsc ma pula. Trzy, żeby o kolejności kroków „sprawdź" decydowało wyłącznie węższe
/// miejsce ciężkie: przy jednym miejscu w puli po kolei szłoby WSZYSTKO i pomiar byłby ślepy.
const AT_ONCE: usize = 3;

/// Ile kroków ciężkich naraz. Jedynka z niezmiennika 26.
const HEAVY_AT_ONCE: usize = 1;

/// Jak długo trwa jeden krok „sprawdź".
///
/// Rzędy wielkości ponad koszt uruchomienia komendy: przy krótkich oknach zajęta maszyna potrafi
/// wystartować drugie sprawdzenie już po pierwszym i pomiar zamienia się w wyścig — zmierzone
/// 2026-08-24, przy 400 ms i trzech biegach naraz w jednym programie testowym.
const CHECK_TAKES: Duration = Duration::from_millis(800);

/// Jak długo pracuje krok agenta. Dłużej niż oba sprawdzenia razem, żeby nakładanie się nie
/// zależało od tego, jak szybko maszyna wystartuje kolejną komendę.
const AGENT_TAKES: Duration = Duration::from_secs(2);

/// Ile czekamy, zanim uznamy bieg za zawieszony.
const PATIENCE: Duration = Duration::from_secs(30);

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000f1
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

/// Obie strony jednym biegiem, i to jest wymóg pomiaru, nie oszczędność.
///
/// Dwa biegi tej samej fikstury w jednym programie testowym walczą o tę samą maszynę, a wtedy
/// „nie nakładały się" bywa zdaniem o obciążeniu, nie o limicie. Jeden bieg odpowiada na oba
/// pytania z jednej osi czasu i żadne z nich nie da się spełnić przypadkiem.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_checks_wait_for_each_other_while_the_agent_beside_them_works()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let marks = bench.run().await?;

    assert!(
        one_after_the_other(&marks, "A", "B"),
        "the two check steps were inside the machine at the same moment. Running two heavy \
         builds side by side on this Mac pins the memory compressor and freezes the laptop at \
         zero swap (invariant 26) — the narrower limit exists to make that impossible, not to \
         make it unlikely. The order was {marks:?}"
    );
    assert!(
        overlaps(&marks, "agent", "A") || overlaps(&marks, "agent", "B"),
        "the agent step never ran beside either check, so the ceiling asserted just above says \
         nothing: an application that runs everything one at a time satisfies it perfectly and \
         throws away the only reason this product exists (invariant 11). The narrower limit is \
         INSIDE the pool, not a second pool beside it. The order was {marks:?}"
    );
    Ok(())
}

#[tokio::test]
async fn the_application_pool_holds_exactly_one_heavy_place() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let folder = TempDir::new()?;
    let store = Store::open(&bench.db())?;
    let state = AppState::new(
        bench.home.path().to_path_buf(),
        bench.project.path().to_path_buf(),
        store,
        fake_drivers(bench.marks_path()),
    );

    let deps = state
        .begin_run(folder.path())
        .map_err(|said| format!("the Start was turned down with nothing going: {said}"))?;

    assert_eq!(
        deps.control.slots().heavy_at_once(),
        HEAVY_AT_ONCE,
        "the one pool this application hands out has to allow exactly one heavy step at a time. \
         Two would satisfy both measurements above — nothing in them needs the number to be one \
         — and would still freeze this machine (invariant 26)"
    );
    Ok(())
}

/// Czy okna dwóch znaczników **nie** nachodzą na siebie.
fn one_after_the_other(marks: &[String], first: &str, second: &str) -> bool {
    !overlaps(marks, first, second)
}

/// Czy okna dwóch znaczników nachodzą na siebie — czytane z samej KOLEJNOŚCI zdarzeń.
///
/// Bez zegara, i to jest wybór: `date` na macOS nie ma nanosekund, a dwa czasy ścienne zapisane
/// przez dwa procesy porównuje się już tylko z przymrużeniem oka. Kolejność wpisów w jednym
/// pliku jest tym samym pomiarem bez ani jednej z tych wątpliwości.
fn overlaps(marks: &[String], first: &str, second: &str) -> bool {
    let at = |what: &str| -> Option<(usize, usize)> {
        let from = marks
            .iter()
            .position(|mark| mark == &format!("{what}-start"))?;
        let to = marks
            .iter()
            .position(|mark| mark == &format!("{what}-end"))?;
        Some((from, to))
    };
    match (at(first), at(second)) {
        (Some((from_one, to_one)), Some((from_two, to_two))) => {
            from_one < to_two && from_two < to_one
        }
        // Okno bez końca nie jest oknem: brak znacznika ma się czytać jako „nie zmierzono",
        // nigdy jako „nie nakładały się".
        _ => false,
    }
}

/// Biblioteka użytkownika, folder pracy, skrypty i jeden bieg.
struct Bench {
    home: TempDir,
    project: TempDir,
    scripts: TempDir,
    marks: PathBuf,
    workflow: PathBuf,
    store: Store,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        let scripts = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;

        let marks = scripts.path().join("marks.txt");
        fs::write(&marks, "")?;
        let first = script(scripts.path(), "check-a.sh", "A", &marks)?;
        let second = script(scripts.path(), "check-b.sh", "B", &marks)?;

        let agent = home.path().join("agents").join("hand.md");
        fs::write(&agent, HAND_FILE)?;
        let workflow = home.path().join("workflows").join("two-checks.json");
        fs::write(&workflow, two_checks_and_an_agent(&first, &second))?;
        the_fixture_can_run(&workflow, &[&agent])?;

        let store = Store::open(&project.path().join(".loadout").join("loadout.db"))?;
        Ok(Self {
            home,
            project,
            scripts,
            marks,
            workflow,
            store,
        })
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }

    fn marks_path(&self) -> PathBuf {
        self.marks.clone()
    }

    /// Jeden bieg; oddaje znaczniki w kolejności, w której naprawdę się wydarzyły.
    async fn run(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store: &self.store,
            drivers: fake_drivers(self.marks.clone()),
            processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
            /* Pula z JEDNYM miejscem ciężkim — dokładnie ta, którą wręcza każdemu startowi
             * `AppState::begin_run`; że tamta ma tę samą jedynkę, pyta trzeci przypadek. */
            control: RunControl::sharing(Limiter::with_heavy(AT_ONCE, HEAVY_AT_ONCE)),
        };
        let request = RunRequest {
            workflow: self.workflow.clone(),
            how_many_at_once: AT_ONCE,
            task: None,
            part: None,
            handoffs_from: None,
        };
        let (sink, drain) = the_pump_seam();

        let (ran, ()) = tokio::time::timeout(PATIENCE, async {
            tokio::join!(run_workflow_inner(&deps, &request, sink), drain)
        })
        .await
        .map_err(|_| format!("the run did not finish within {PATIENCE:?}"))?;

        let report = ran?;
        assert_eq!(
            report.steps,
            vec![StepState::Succeeded; 3],
            "all three steps have to finish, or the order below belongs to a run that fell over \
             for some other reason. It ended as {:?}",
            report.steps
        );

        let text = fs::read_to_string(&self.marks)?;
        let marks: Vec<String> = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect();
        assert_eq!(
            marks.len(),
            6,
            "six marks were expected — a start and an end for each of the three steps — and \
             {} arrived. A missing end silently reads as \"they did not overlap\", so the \
             measurement would understate exactly what it is here to catch: {marks:?}",
            marks.len()
        );
        // `scripts` musi dożyć do tego miejsca: to w nim leży plik znaczników.
        let _ = &self.scripts;
        Ok(marks)
    }
}

/// Skrypt kroku „sprawdź": znacznik wejścia, praca, znacznik wyjścia, dowód.
///
/// Ścieżka pliku znaczników jest **bezwzględna i wpisana w skrypt**, bo środowisko dziecka jest
/// czyszczone (niezmiennik 9), więc przez zmienną nic tu nie przejdzie.
fn script(dir: &Path, name: &str, mark: &str, marks: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    let body = format!(
        "#!/bin/sh\nprintf '%s\\n' '{mark}-start' >> '{}'\nsleep {}\nprintf '%s\\n' '{mark}-end' \
         >> '{}'\necho '1 passed'\n",
        marks.display(),
        CHECK_TAKES.as_secs_f32(),
        marks.display(),
    );
    fs::write(&path, body)?;
    let mut mode = fs::metadata(&path)?.permissions();
    mode.set_mode(0o755);
    fs::set_permissions(&path, mode)?;
    Ok(path)
}

/// Dwa kroki „sprawdź" i jeden krok agenta, wszystkie gotowe naraz i bez ani jednej strzałki.
///
/// Każdy pracuje na własnej kopii plików: trzy kroki mogące biec równocześnie w folderze
/// projektu są odmową przy zapisie (niezmiennik 12), więc bez tego fikstura nie doszłaby nawet
/// do planisty.
fn two_checks_and_an_agent(first: &Path, second: &Path) -> String {
    format!(
        r#"{{
  "format": 1,
  "id": "wf_two_checks",
  "name": "Two checks and an agent",
  "steps": [
    {{
      "kind": "check",
      "id": "s_check_a",
      "name": "Check A",
      "command": "{}",
      "proof": "(\\d+) passed",
      "folder": {{ "use": "fresh-copy" }},
      "at": {{ "x": 0, "y": 0 }}
    }},
    {{
      "kind": "check",
      "id": "s_check_b",
      "name": "Check B",
      "command": "{}",
      "proof": "(\\d+) passed",
      "folder": {{ "use": "fresh-copy" }},
      "at": {{ "x": 240, "y": 0 }}
    }},
    {{
      "kind": "agent",
      "id": "s_agent",
      "name": "Beside them",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {{}},
      "instructions": "work beside the checks",
      "folder": {{ "use": "fresh-copy" }},
      "at": {{ "x": 0, "y": 240 }}
    }}
  ],
  "links": []
}}"#,
        first.display(),
        second.display(),
    )
}

/// Szew, którym bieg mówi do okna.
fn the_pump_seam() -> (LineSink, impl Future<Output = ()>) {
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    (sink, async move {
        let _ = pump.await;
    })
}

/// Fikstura ma przejść walidator bez ani jednego problemu. Przesłanka kryterium, nie kryterium.
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

/// Fabryka, która dla każdego vendora oddaje ten sam dubler.
fn fake_drivers(marks: PathBuf) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { marks });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler agenta: te same znaczniki, ten sam plik, ta sama oś czasu, co kroki „sprawdź".
#[derive(Debug)]
struct Fake {
    marks: PathBuf,
}

/// Dopisuje znacznik. Dopisanie, nie przepisanie: pomiar jest kolejnością wpisów.
fn mark(path: &Path, what: &str) {
    use std::io::Write as _;
    if let Ok(mut file) = fs::OpenOptions::new().append(true).open(path) {
        let _ = writeln!(file, "{what}");
    }
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
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        mark(&self.marks, "agent-start");
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
            marks: self.marks.clone(),
        }))
    }
}

/// Jedna tura dublera.
#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    marks: PathBuf,
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
        tokio::time::sleep(AGENT_TAKES).await;
        mark(&self.marks, "agent-end");
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: String::new(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: AGENT_TAKES,
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send((AgentEvent::Finished(outcome.clone())).into())
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
