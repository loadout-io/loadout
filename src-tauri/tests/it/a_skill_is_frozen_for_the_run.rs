//! T-154: materiał, po który bieg sięgnął, jest zamrożony RAZ — a powtórzenie tego biegu odmawia
//! **przed pierwszym procesem**, kiedy ten materiał zniknął albo przestał być tym samym.
//!
//! # Po co to istnieje
//!
//! Człowiek naciska „Run this step again", żeby zapytać: czy MOJA poprawka zmieniła wynik.
//! Odpowiedź nie znaczy nic, jeżeli w międzyczasie przesunęło się też wejście. Do 2026-08-28 obie
//! ciche wersje tej wady były możliwe i żadna nie zostawiała po sobie ani jednego zdania:
//! poprawiony `SKILL.md` dawał powtórzenie z INNYM materiałem, a umiejętność zdjęta z agenta —
//! powtórzenie z MNIEJSZYM. Jedno i drugie kończyło się `Succeeded`, więc z zewnątrz wyglądało
//! dokładnie jak powtórzenie, o które człowiek prosił.
//!
//! `StepSkills::for_the_step` nie łapie ani jednego z tych dwóch stanów i nie ma jak: ona pyta
//! „czy to, co WYBRANO, da się dostarczyć", a to jest pytanie o dziś. „Czy to jest to samo, co
//! wtedy" jest pytaniem o tamten bieg i odpowiada na nie wyłącznie jego `run.json`.
//!
//! # SŁABĄ WERSJĄ TEGO KRYTERIUM JEST `assert!(result.is_err())`
//!
//! Przechodzi ją sprawdzenie zrobione W KROKU — czyli takie, które zakłada katalog biegu, odpala
//! agenta, płaci za jego turę i odmawia dopiero potem. Niezmiennik 12 mówi wprost, kiedy ta
//! odmowa ma paść: najpóźniej przy Starcie, nigdy w trakcie. Rozróżnia to **wyłącznie licznik
//! uruchomień dublera**, który po powtórzeniu ma stać na tym samym jeden, co po pierwszym biegu.
//!
//! # I DRUGĄ: IMPLEMENTACJA, KTÓRA ODMAWIA ZAWSZE
//!
//! Odmowa przy każdym powtórzeniu przechodzi oba punkty wyżej i zabiera człowiekowi ruch, dla
//! którego cała ta ścieżka powstała. Dlatego trzeci test puszcza to samo powtórzenie nad
//! NIETKNIĘTĄ biblioteką i wymaga, żeby po prostu pobiegło.
//!
//! ZDANIA ODMOWY NIE MA W TYM PLIKU JAKO LITERAŁU i to jest połowa jego wartości. Składamy je
//! z `skills::NotAsItWas`, czyli z typu, w którym ono mieszka; przepisane tutaj byłoby drugą
//! kopią, a druga kopia jednego zdania jest zawsze tą nieaktualną (niezmiennik 23).

// `unwrap()`/`expect()` w teście: panika w teście JEST jego wynikiem. Bramka biegnie clippy
// z `--tests -- -D warnings`, więc bez tej linii ląduje to w niej, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest, rerun};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::skills::{Moved, NotAsItWas};
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera. Nie „claude" i nie „codex": tamte dwie mają własne wymagania co do
/// dowodów biegu, a to kryterium sądzi odmowę i licznik uruchomień.
const VENDOR: &str = "fake";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się", a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(30);

/// Umiejętność, którą ma agent i po którą sięga bieg.
const ALPHA: &str = "alpha";

/// Nazwa kroku, czyli to, czego człowiek szuka na płótnie. Odmowa bez niej zamienia jedno
/// przywrócenie pliku w przeszukiwanie workflow.
const STEP: &str = "Only step";

/// Klucz kafelka — tym `rerun::again` wskazuje krok do powtórzenia.
const TILE: &str = "s_only";

/// Nazwa pliku workflow w bibliotece — `rerun::again` szuka po niej najnowszego biegu.
const LIBRARY_FILE: &str = "frozen.json";

fn skill_file(name: &str, body: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: Reads one file and says what it is for.\n---\n\n{body}\n"
    )
}

/// Ciało `SKILL.md` w pierwszym biegu.
const AS_IT_WAS: &str = "Answer with a single sentence.";

/// To samo `SKILL.md` po poprawce człowieka — inna instrukcja, ten sam nagłówek.
///
/// Zmiana jest w CIELE, nie w nagłówku, i to jest wybór: plik dalej przechodzi walidator i dalej
/// jest tą samą umiejętnością pod tą samą nazwą, więc `StepSkills::for_the_step` nie ma o co się
/// zatrzymać. Gdyby fikstura psuła nagłówek, ten test mierzyłby `Why::Unusable` — czyli odmowę,
/// która istnieje od T-79 i o zamrożeniu nie mówi nic.
const AS_IT_IS_NOW: &str = "Answer with three paragraphs and a table.";

/// Agent z jedną umiejętnością. `skills` jest jedynym miejscem, które podmienia test o `Gone`.
fn agent_file(skills: &str) -> String {
    format!(
        "---
schema: 1
id: 01990000-0000-7000-8000-0000000000d3
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
skills: {skills}
connections: []
---
Do the work.
"
    )
}

/// Jeden krok na własnej kopii plików — krok pracujący wprost w folderze człowieka jest osobną
/// odmową (`Why::WouldWriteIntoYourFolder`) i fikstura, która by ją wywoływała, mierzyłaby ją
/// zamiast zamrożenia.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_a_skill_is_frozen_for_the_run",
  "name": "One step with one skill",
  "steps": [
    {
      "kind": "agent",
      "id": "s_only",
      "name": "Only step",
      "agent": "01990000-0000-7000-8000-0000000000d3",
      "overrides": {},
      "instructions": "do the work",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_changed_skill_stops_the_repeat_before_the_first_process() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent(&agent_file(&format!("[{ALPHA}]")))?;
    bench.skill(ALPHA, &skill_file(ALPHA, AS_IT_WAS))?;
    let workflow = bench.workflow()?;
    let started = Arc::new(AtomicUsize::new(0));

    let first = bench.run(&started, first_run(workflow)).await??;
    assert_eq!(
        (first.steps.clone(), started.load(Ordering::SeqCst)),
        (vec![StepState::Succeeded], 1),
        "the bench is only a bench if the first run really went through one agent; every point \
         below is about repeating that one step"
    );

    // ── CO TEN BIEG ZAMROZIŁ, ZAPISANE TAM, GDZIE STOJĄ NOTATKI ───────────────────────────
    let saved = the_skills_in(&first)?;
    assert_eq!(
        saved,
        vec![(
            ALPHA.to_owned(),
            skill_file(ALPHA, AS_IT_WAS).len(),
            vec![TILE.to_owned()]
        )],
        "the run reached for {ALPHA} and left no lasting record of what it reached for. Files are \
         the truth about a run (invariant 4): without the name, the fingerprint and the length \
         written down where the notes are written down, nobody can ever answer what this run was \
         given - and the repeat below has nothing to compare against. It saved: {saved:?}"
    );

    // ── CZŁOWIEK POPRAWIA `SKILL.md` I POWTARZA KROK ──────────────────────────────────────
    bench.skill(ALPHA, &skill_file(ALPHA, AS_IT_IS_NOW))?;
    let again = rerun::again(
        bench.home.path(),
        bench.project.path(),
        LIBRARY_FILE,
        TILE,
        1,
    )?;
    let repeat = bench.run(&started, again.request).await?;

    let expected = NotAsItWas {
        step: STEP.to_owned(),
        skill: ALPHA.to_owned(),
        why: Moved::Changed,
    }
    .to_string();
    assert!(
        repeat
            .as_ref()
            .err()
            .is_some_and(|said| said.contains(&expected)),
        "the library moved under this run and it repeated the step anyway, quietly, with material \
         the first run never had. Running one tile again is how a person asks \"did my fix change \
         the answer\" - and the answer means nothing if the input moved at the same time. Expected \
         to find {expected:?}; the repeat answered {:?}",
        repeat.as_ref().err()
    );
    assert_eq!(
        started.load(Ordering::SeqCst),
        1,
        "the repeat reached the agent before refusing. Refusing halfway is the expensive version \
         of this defect: the turn is paid for, and the human reads a refusal about work that \
         already happened (invariant 12 - refuse at Start, never mid-run)"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_skill_taken_off_the_agent_stops_the_repeat() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent(&agent_file(&format!("[{ALPHA}]")))?;
    bench.skill(ALPHA, &skill_file(ALPHA, AS_IT_WAS))?;
    let workflow = bench.workflow()?;
    let started = Arc::new(AtomicUsize::new(0));

    let first = bench.run(&started, first_run(workflow)).await??;
    assert_eq!(
        (first.steps.clone(), started.load(Ordering::SeqCst)),
        (vec![StepState::Succeeded], 1),
        "the bench is only a bench if the first run really went through one agent"
    );

    /* UMIEJĘTNOŚĆ ZOSTAJE W BIBLIOTECE, ZNIKA Z AGENTA. To jest ta połowa, której nie łapie
     * `StepSkills::for_the_step`: nazwy nie ma w zbiorze, więc nie ma czego szukać na dysku
     * i nie ma o co się zatrzymać. Powtórzenie jedzie wtedy z mniejszym materiałem, a „agent
     * nie ma tej umiejętności" jest z zewnątrz nieodróżnialne od „model po nią nie sięgnął". */
    bench.agent(&agent_file("[]"))?;
    let again = rerun::again(
        bench.home.path(),
        bench.project.path(),
        LIBRARY_FILE,
        TILE,
        1,
    )?;
    let repeat = bench.run(&started, again.request).await?;

    let expected = NotAsItWas {
        step: STEP.to_owned(),
        skill: ALPHA.to_owned(),
        why: Moved::Gone,
    }
    .to_string();
    assert!(
        repeat
            .as_ref()
            .err()
            .is_some_and(|said| said.contains(&expected)),
        "the step no longer reaches {ALPHA} and the repeat went ahead without it, saying nothing. \
         Two different sentences for two different fixes: this one is put right by giving the \
         skill back to the agent, the other by leaving the material alone. Expected to find \
         {expected:?}; the repeat answered {:?}",
        repeat.as_ref().err()
    );
    assert_eq!(
        started.load(Ordering::SeqCst),
        1,
        "the repeat reached the agent before refusing; the refusal has to land before that turn \
         is paid for"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_repeat_on_an_untouched_library_still_runs() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent(&agent_file(&format!("[{ALPHA}]")))?;
    bench.skill(ALPHA, &skill_file(ALPHA, AS_IT_WAS))?;
    let workflow = bench.workflow()?;
    let started = Arc::new(AtomicUsize::new(0));

    bench.run(&started, first_run(workflow)).await??;

    let again = rerun::again(
        bench.home.path(),
        bench.project.path(),
        LIBRARY_FILE,
        TILE,
        1,
    )?;
    let repeat = bench.run(&started, again.request).await?;

    assert_eq!(
        (
            repeat.as_ref().ok().map(|report| report.steps.clone()),
            started.load(Ordering::SeqCst)
        ),
        (Some(vec![StepState::Succeeded]), 2),
        "nobody touched the library, and the repeat refused anyway. A guard that says no every \
         time passes both points above and takes away the one move this whole path exists for. \
         It answered {:?}",
        repeat.as_ref().err()
    );
    Ok(())
}

/// Żądanie pierwszego biegu: cały plik, nic po czym iść.
fn first_run(workflow: PathBuf) -> RunRequest {
    RunRequest {
        workflow,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    }
}

/// Jak skończył się bieg: raport albo **zdanie**, którym odmówił.
///
/// Zdanie, nie sam fakt odmowy: kryterium o odmowie asertuje treść tam, gdzie czyta ją człowiek,
/// a nie to, że coś zwróciło błąd (niezmiennik 29).
type Ended = Result<RunReport, String>;

// ── co bieg zapisał o umiejętnościach ──────────────────────────────────────────────────────

/// Jedna pozycja rachunku z `run.json`: `(nazwa, liczba bajtów, kafelki)`.
type Reached = (String, usize, Vec<String>);

/// `(nazwa, liczba bajtów, kafelki)` z `run.json` tego biegu, w kolejności zapisu.
///
/// Odcisk czytamy tylko przez to, że MUSI tam być: jego wartość jest liczbą FNV-1a, a wpisanie
/// jej tutaj drugi raz byłoby przepisaniem implementacji do asercji. Pytamy więc, czy pole
/// istnieje i nie jest puste — czym ono jest naprawdę, sądzi odmowa niżej.
fn the_skills_in(report: &RunReport) -> Result<Vec<Reached>, Box<dyn Error>> {
    let text = fs::read_to_string(report.dir.join("run.json"))?;
    let file: serde_json::Value = serde_json::from_str(&text)?;
    let Some(listed) = file.get("skills").and_then(serde_json::Value::as_array) else {
        return Ok(Vec::new());
    };
    Ok(listed
        .iter()
        .filter(|one| {
            one.get("hash")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|hash| !hash.is_empty())
        })
        .map(|one| {
            (
                one.get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                usize::try_from(
                    one.get("bytes")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or_default(),
                )
                .unwrap_or_default(),
                one.get("steps")
                    .and_then(serde_json::Value::as_array)
                    .map(|keys| {
                        keys.iter()
                            .filter_map(serde_json::Value::as_str)
                            .map(ToOwned::to_owned)
                            .collect()
                    })
                    .unwrap_or_default(),
            )
        })
        .collect())
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn counting_drivers(started: Arc<AtomicUsize>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Counting { started });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler, którego jedyną treścią jest licznik uruchomień. To on odróżnia odmowę przed pierwszym
/// procesem od odmowy zrobionej w kroku.
#[derive(Debug)]
struct Counting {
    started: Arc<AtomicUsize>,
}

#[async_trait]
impl AgentDriver for Counting {
    fn id(&self) -> &'static str {
        VENDOR
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some(VENDOR.to_owned()),
        })
    }

    /// Ten dubler UMIE przyjąć gotowy fragment argv — inaczej krok stanąłby na braku szwu
    /// i licznik pokazywałby swoją liczbę z powodu, o którym to kryterium nie mówi.
    fn inheriting(&self, _flags: &[String]) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            started: Arc::clone(&self.started),
        }))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.started.fetch_add(1, Ordering::SeqCst);
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
        Ok(Box::new(Turn { events, session }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
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
        fs::create_dir_all(home.path().join("skills"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(project.path().join("notes.txt"), "written by the human")?;
        Ok(Self { home, project })
    }

    fn agent(&self, text: &str) -> Result<(), Box<dyn Error>> {
        fs::write(self.home.path().join("agents").join("hand.md"), text)?;
        Ok(())
    }

    fn skill(&self, name: &str, text: &str) -> Result<(), Box<dyn Error>> {
        let dir = self.home.path().join("skills").join(name);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("SKILL.md"), text)?;
        Ok(())
    }

    fn workflow(&self) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.home.path().join("workflows").join(LIBRARY_FILE);
        fs::write(&path, WORKFLOW)?;
        Ok(path)
    }

    /// Jeden bieg. `None` znaczy „odmówił", a zdanie odmowy niesie wtedy błąd wołającego.
    ///
    /// Własny magazyn na bieg, bo `Store::open` trzyma połączenie, a dwa biegi jednego testu idą
    /// po sobie, nie obok siebie.
    async fn run(
        &self,
        started: &Arc<AtomicUsize>,
        request: RunRequest,
    ) -> Result<Ended, Box<dyn Error>> {
        let store = Store::open(&self.project.path().join(".loadout").join("loadout.db"))?;
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store: &store,
            drivers: counting_drivers(Arc::clone(started)),
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let (sink, source) = line_channel(QUEUE_CAP);
        let pump = spawn_pump(source, Channel::new(|_| Ok(())));
        let outcome = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
            .await
            .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))?;
        let _ = tokio::time::timeout(PATIENCE, pump).await;
        Ok(outcome.map_err(|error| error.to_string()))
    }
}
