//! AC-1 dla T-62: jeden agent, jedno zdanie, **zwykły** bieg.
//!
//! Jednostką pracy jest dziś graf: `run_workflow` bierze nazwę PLIKU, więc żeby puścić jednego
//! agenta z jednym zdaniem, człowiek musi wejść do edytora, założyć workflow, postawić jeden
//! kafelek, zapisać go i wrócić. To jest cena płacona za najczęstszą czynność dnia — i to jest
//! wszystko, co to kryterium ma zmienić. Bieg jednokrokowy zostaje BIEGIEM: ma plan, ma katalog
//! `runs/<ts>__<id>/`, ma miejsce w puli i ma wpis w indeksie.
//!
//! # Słaba wersja tego kryterium: sprawdzenie samej LICZBY kroków
//!
//! `assert_eq!(report.steps.len(), 1)` przechodzi dla implementacji, która bierze **domyślnego**
//! agenta zamiast wskazanego — czyli odpala nie tego, o kogo poprosił człowiek, i wygląda przy
//! tym na sukces. Rozstrzyga to drugi przypadek w tym pliku: dwie definicje agentów różnią się
//! vendorem, modelem i pozycją dialu plików, a bieg ma wziąć TE trzy rzeczy z tej definicji,
//! którą nazwał człowiek.
//!
//! # Dlaczego polityka jest porównywana z `policy_of`, a nie z wpisanym `Policy::ReadOnly`
//!
//! Bo asercja na wpisanej wartości przechodzi także dla DRUGIEJ KOPII tego `match` — a to jest
//! dokładnie ten defekt, przed którym stoi niezmiennik 23: reguła przepisana obok rdzenia żyje
//! do pierwszej zmiany rdzenia i potem kłamie. Kryterium woła więc tę samą funkcję, którą woła
//! bieg z pliku, i osobno wymaga, żeby dwie różne pozycje dialu dały dwie różne polityki —
//! bez tego drugiego warunku „ta sama tabela" spełniałaby też tabela, która zawsze mówi to samo.
//!
//! # Kontrola: agent, którego nie ma
//!
//! Odmowa ma **nazwać, gdzie są agenci**, i ma być odmową PRZED pierwszym katalogiem. Cichy
//! start czegokolwiek jest tu gorszy niż zdanie: człowiek płaci za turę agenta, o którego nie
//! prosił, i dowiaduje się o tym z rachunku. Dlatego ostatni przypadek pyta o trzy rzeczy naraz:
//! że to jest `Err`, że zdanie prowadzi człowieka do biblioteki, i że na dysku nie powstał ani
//! jeden katalog biegu.
//!
//! Dubler sterownika stoi w tym pliku, a nie w `engine/drivers/fake.rs`: tamten jest dublerem
//! kroku planisty, nie implementuje `AgentDriver` i nie ma na nim czego zapytać o `RunSpec`.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::{AskRequest, policy_of, run_agent_inner};
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::{LineSink, QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::{FileAccess, Vendor, read_agent_file};
use loadout_lib::store::Store;
use serde_json::Value as Json;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Ile trwa jedna tura dublera. Krótko, ale nie zero — bieg ma naprawdę przez siebie przejść.
const TURN: Duration = Duration::from_millis(30);

/// Ile czekamy na bieg, zanim uznamy go za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(10);

/// Ile kroków ma naprawdę działać naraz. Ta sama liczba, co przy biegu z pliku, i nigdy stała
/// `1` w środku: bieg jednokrokowy bierze miejsce z TEJ SAMEJ puli (niezmiennik 11).
const AT_ONCE: usize = 3;

/// Identyfikator agenta, który patrzy i nie pisze.
const SCOUT_ID: &str = "01990000-0000-7000-8000-0000000000c1";
/// Identyfikator agenta, który pisze bez ograniczeń.
const FORGE_ID: &str = "01990000-0000-7000-8000-0000000000c2";
/// Identyfikator, którego w bibliotece NIE MA.
const NOBODY_ID: &str = "01990000-0000-7000-8000-0000000000ff";

/// Model, który ma dojechać do sterownika z definicji `Scout`.
const SCOUT_MODEL: &str = "o3";
/// Model `Forge`. Inny niż [`SCOUT_MODEL`], bo tylko różnica coś tu dowodzi.
const FORGE_MODEL: &str = "opus";

/// Zdanie człowieka. Ze znakami interpunkcyjnymi i podwójną spacją, bo ma dojechać CO DO ZNAKU.
const SENTENCE: &str = "read src/main.rs  and say what it does";

const SCOUT_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000c1
name: Scout
summary: Looks around
color: slate
runsWith: codex
model: o3
thinking: balanced
fileAccess: look-only
giveUpAfterMinutes: 20
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Look around and say what you found.
";

const FORGE_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000c2
name: Forge
summary: Writes the change
color: clay
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
Write the smallest change that works.
";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_agent_and_one_sentence_is_an_ordinary_run() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::default());
    let report = bench.ask(&store, &watch, SCOUT_ID, SENTENCE).await??;

    // (a) DOKŁADNIE JEDEN KROK. Nie „co najmniej jeden": plan, który dokłada krok od siebie,
    //     kupuje turę, o którą nikt nie prosił.
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded],
        "one agent and one sentence is a plan of exactly one step, and it has to finish for \
         anything below to mean something. This run ended as {:?}",
        report.steps
    );
    let started = watch.started();
    assert_eq!(
        started.len(),
        1,
        "the driver was started {} time(s) for a one-step run. More than once means the plan \
         grew a step nobody asked for; never means nothing ran and every assertion below would \
         be reading an empty list",
        started.len()
    );
    let (_, spec) = started.first().ok_or("no step ever reached the driver")?;

    // (c) ZDANIE CZŁOWIEKA LEŻY W PROMPCIE KROKU, co do znaku. Prompt bez niego jest promptem
    //     agenta, nie polecenia — czyli agentem uruchomionym bez powodu.
    assert!(
        spec.prompt.contains(SENTENCE),
        "the sentence a person typed has to reach the step's prompt word for word, including \
         the spacing, or the run is an agent started with no instruction. The prompt was: {:?}",
        spec.prompt
    );

    the_run_left_a_directory(&report, bench.project.path())?;
    the_index_knows_this_run(&store, &report)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_step_takes_vendor_model_and_policy_from_the_agent_it_names()
-> Result<(), Box<dyn Error>> {
    let scout = one_step_for(SCOUT_ID).await?;
    let forge = one_step_for(FORGE_ID).await?;

    // (b) TRZY POLA Z TEJ DEFINICJI, KTÓRĄ NAZWAŁ CZŁOWIEK. To jest asercja, która odróżnia
    //     „uruchomił wskazanego agenta" od „uruchomił domyślnego i policzył kroki".
    assert_eq!(
        (scout.0, forge.0),
        (Vendor::Codex, Vendor::ClaudeCode),
        "the factory was asked for {:?} and {:?}; the saved definitions say codex and \
         claude-code. A run that takes the vendor from anywhere else is running a different \
         agent than the one that was named",
        scout.0,
        forge.0
    );
    assert_eq!(
        (scout.1.model.as_deref(), forge.1.model.as_deref()),
        (Some(SCOUT_MODEL), Some(FORGE_MODEL)),
        "the model has to come from the named definition too: {:?} and {:?} reached the driver",
        scout.1.model,
        forge.1.model
    );

    // POLITYKA PRZEZ TĘ SAMĄ TABELĘ, CO BIEG Z PLIKU. Porównanie z wpisaną wartością
    // przechodziłoby także dla drugiej kopii tego `match` (niezmiennik 23).
    assert_eq!(
        (scout.1.policy, forge.1.policy),
        (
            policy_of(FileAccess::LookOnly),
            policy_of(FileAccess::WorkFreely)
        ),
        "what an agent may do with files is one table (`commands::run::policy_of`), and a \
         one-step run has to read it rather than keep a second copy: {:?} and {:?} reached the \
         driver",
        scout.1.policy,
        forge.1.policy
    );
    // KONTROLA PRZECIW TABELI, KTÓRA ZAWSZE MÓWI TO SAMO: bez tego warunku „ta sama tabela"
    // spełniałaby też funkcja zwracająca jedną politykę na wszystko.
    assert_ne!(
        policy_of(FileAccess::LookOnly),
        policy_of(FileAccess::WorkFreely),
        "the two dial positions in this fixture map to one policy, so the comparison above \
         proves nothing about where the policy came from"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_agent_nobody_saved_is_refused_and_says_where_the_agents_are()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::default());
    let said = match bench.ask(&store, &watch, NOBODY_ID, SENTENCE).await? {
        Ok(report) => {
            return Err(format!(
                "an agent id nobody saved started a run anyway ({}). A silent start is the \
                 expensive kind of wrong here: somebody pays for a turn they did not ask for \
                 and finds out from the bill",
                report.id
            )
            .into());
        }
        Err(refusal) => refusal.to_string(),
    };

    // (e) ZDANIE MÓWI, GDZIE SĄ AGENCI. „no agent with that id" zostawia człowieka dokładnie
    //     tam, gdzie był — a nazw, których nie widzi, nie ma jak zgadnąć (DESIGN §8).
    assert!(
        said.contains("Agents"),
        "the refusal has to point at the place where agents are kept, so the next move is one \
         a person can make. It said: {said}"
    );
    assert!(
        said.contains("Scout") || said.contains("Forge"),
        "and it has to name at least one agent that DOES exist, for the same reason the /run \
         refusal lists workflow names. It said: {said}"
    );

    assert!(
        watch.started().is_empty(),
        "the driver was started even though the agent was refused, so something ran and \
         somebody is paying for it"
    );
    let runs = bench.project.path().join(".loadout").join("runs");
    let left = match fs::read_dir(&runs) {
        Ok(listing) => listing.count(),
        // Katalogu nie ma — czyli odmowa padła przed pierwszym `create_dir_all`, dokładnie tak,
        // jak ma padać.
        Err(_) => 0,
    };
    assert_eq!(
        left,
        0,
        "the refusal left {left} run directory(ies) under {}. Refusing after the directory \
         exists leaves history claiming a run that never happened (invariant 4)",
        runs.display()
    );
    Ok(())
}

/// Jeden bieg jednokrokowy dla tego agenta; oddaje vendora, o którego poproszono fabrykę,
/// i to, co naprawdę dojechało do sterownika.
async fn one_step_for(agent: &str) -> Result<(Vendor, RunSpec), Box<dyn Error>> {
    let bench = Bench::new()?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::default());
    bench.ask(&store, &watch, agent, SENTENCE).await??;
    watch
        .started()
        .into_iter()
        .next()
        .ok_or_else(|| format!("nothing reached the driver for {agent}").into())
}

/// (d) Katalog biegu istnieje, nazywa się `<ts>__<id>` i ma w środku `run.json` oraz `logs/`.
///
/// Plik jest prawdą (niezmiennik 4), więc bieg bez śladu na dysku jest biegiem, którego nie da
/// się potem wyjaśnić — a to jest cała różnica między „zwykłym biegiem" a „lekkim trybem".
fn the_run_left_a_directory(report: &RunReport, project: &Path) -> Result<(), Box<dyn Error>> {
    let runs = project.join(".loadout").join("runs");
    let mut dirs: Vec<PathBuf> = fs::read_dir(&runs)
        .map_err(|error| format!("{} could not be read: {error}", runs.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    let [dir] = dirs.as_slice() else {
        return Err(format!(
            "expected exactly one run directory under {}, found {}",
            runs.display(),
            dirs.len()
        )
        .into());
    };
    assert_eq!(
        dir, &report.dir,
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
        "{} has no logs/ — a one-step run is laid out like every other one (ARCHITECTURE §4)",
        dir.display()
    );
    let text = fs::read_to_string(dir.join("run.json"))?;
    let _: Json = serde_json::from_str(&text)?;
    Ok(())
}

/// (d), druga połowa: bieg ma wpis w indeksie, a jego jedyny krok też.
///
/// Indeks czytamy dlatego, że tam sięga ekran historii. Bieg widoczny wyłącznie w `RunReport`
/// żyje tyle, co wywołanie — czyli nie da się o niego zapytać w chwili, w której ktokolwiek pyta.
fn the_index_knows_this_run(store: &Store, report: &RunReport) -> Result<(), Box<dyn Error>> {
    let reader = store.reader()?;
    let runs: i64 = reader.query_row(
        "SELECT COUNT(*) FROM runs WHERE id = ?1",
        [&report.id],
        |row| row.get(0),
    )?;
    assert_eq!(
        runs, 1,
        "the index holds {runs} row(s) for this run. It is rebuilt FROM the run directory \
         (invariant 4), so a missing row means the directory says one thing and the screen \
         another"
    );
    let steps: i64 = reader.query_row(
        "SELECT COUNT(*) FROM steps WHERE run_id = ?1",
        [&report.id],
        |row| row.get(0),
    )?;
    assert_eq!(
        steps, 1,
        "the index holds {steps} step row(s) for a one-step run"
    );
    Ok(())
}

/// Biblioteka użytkownika i folder pracy na czas jednego przypadku.
struct Bench {
    home: TempDir,
    project: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        // `Store::open` zakłada plik bazy, ale nie katalog nad nim.
        fs::create_dir_all(project.path().join(".loadout"))?;
        let bench = Self { home, project };
        for (slug, text) in [("scout", SCOUT_FILE), ("forge", FORGE_FILE)] {
            let path = bench.home.path().join("agents").join(format!("{slug}.md"));
            fs::write(&path, text)?;
            // PRZESŁANKA, NIE ASERCJA: definicja, której nie da się przeczytać, byłaby odmową
            // w KAŻDEJ implementacji, a kryterium nazwałoby to brakiem zachowania. Czerwień
            // w fazie kontraktu wygląda dla obu przypadków identycznie.
            read_agent_file(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        }
        Ok(bench)
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }

    /// Jedno `/ask`: ten agent, to zdanie. Zewnętrzny `Result` mówi, czy bieg w ogóle wrócił.
    async fn ask(
        &self,
        store: &Store,
        watch: &Arc<Watch>,
        agent: &str,
        task: &str,
    ) -> Result<Result<RunReport, loadout_lib::commands::RunError>, Box<dyn Error>> {
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store,
            drivers: fake_drivers(Arc::clone(watch), TURN),
            processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let ask = AskRequest {
            agent: agent.to_owned(),
            task: task.to_owned(),
            how_many_at_once: AT_ONCE,
        };
        let (sink, drain) = the_pump_seam();
        let (ran, ()) = tokio::time::timeout(PATIENCE, async {
            tokio::join!(run_agent_inner(&deps, &ask, sink), drain)
        })
        .await
        .map_err(|_| format!("the run did not finish within {PATIENCE:?}"))?;
        Ok(ran)
    }
}

/// Szew, którym bieg mówi do okna: nadajnik dla biegu i czekanie na pompę.
///
/// Kanał jest tu czarną dziurą — to kryterium sądzi plan, dysk i indeks, a nie wiersze. Pompa
/// kończy się sama, kiedy zniknie ostatni nadajnik, a ten ginie razem z powrotem biegu.
fn the_pump_seam() -> (LineSink, impl Future<Output = ()>) {
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    (sink, async move {
        let _ = pump.await;
    })
}

/// Fabryka sterowników, która **zapamiętuje, o kogo ją poproszono**.
///
/// To jest jedyne miejsce, w którym widać vendora: `RunSpec` go nie niesie, bo sterownik już
/// wie, kim jest. Bez tego zapisu asercja (b) nie miałaby czego porównać.
fn fake_drivers(watch: Arc<Watch>, hold: Duration) -> Drivers {
    Arc::new(move |vendor| {
        Arc::new(Fake {
            vendor,
            watch: Arc::clone(&watch),
            hold,
            evidence: None,
        })
    })
}

/// Co bieg naprawdę zamówił u sterowników.
#[derive(Debug, Default)]
struct Watch {
    started: Mutex<Vec<(Vendor, RunSpec)>>,
}

impl Watch {
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym
    /// wywołaniu, więc nie ma wyrażenia, w którym dożyłby do `await`.
    fn saw(&self, vendor: Vendor, spec: RunSpec) {
        self.lock().push((vendor, spec));
    }

    fn started(&self) -> Vec<(Vendor, RunSpec)> {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<(Vendor, RunSpec)>> {
        // Zatruty zamek nie ma prawa zgubić pomiaru: panika w kroku oślepiłaby asercję,
        // która akurat dowodzi, co ten krok dostał.
        self.started.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Dubler sterownika: dwa zdarzenia na krok i tura o mierzalnej długości.
#[derive(Debug)]
struct Fake {
    /// Vendor, o którego poproszono fabrykę. Zapisywany razem z `RunSpec`.
    vendor: Vendor,
    watch: Arc<Watch>,
    hold: Duration,
    /// Produkcyjny Codex nie może ruszyć bez prywatnego celu dowodowego. Dubler zachowuje ten
    /// sam kontrakt, choć samo I/O nadal należy do testowanego silnika.
    evidence: Option<EvidenceTarget>,
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        match self.vendor {
            Vendor::ClaudeCode => "claude-code",
            Vendor::Codex => "codex",
        }
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some(self.id().to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let _evidence = self
            .evidence
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("the run omitted its evidence target"))?;
        let session = SessionRef {
            vendor: self.id(),
            id: spec.run_id.to_string(),
        };
        self.watch.saw(self.vendor, spec.clone());

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
        let _ = events
            .send(
                (AgentEvent::Said {
                    text: format!("working on {}", spec.prompt),
                })
                .into(),
            )
            .await;

        Ok(Box::new(Turn {
            events,
            session,
            hold: self.hold,
        }))
    }

    fn with_evidence(&self, target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            vendor: self.vendor,
            watch: Arc::clone(&self.watch),
            hold: self.hold,
            evidence: Some(target),
        }))
    }
}

/// Jedna tura dublera.
#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
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
        // sprzątanie strzelałoby w cudzy proces.
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
