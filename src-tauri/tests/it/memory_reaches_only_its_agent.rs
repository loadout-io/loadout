//! AC-2 dla T-80: do promptu kroku wchodzi pamięć **tego** agenta i niczyja więcej.
//!
//! `what_the_agents_know` (`commands/run.rs`) składa blok z DWÓCH zakresów i pomija
//! `Scope::ThisAgent` — komentarz przy nim nazywa to zgłoszeniem, nie przeoczeniem: blok jest
//! liczony raz na bieg, a filtrowanie po agencie wymaga tożsamości agenta w chwili liczenia.
//! Skutek jest taki, że trzeci zakres nie dociera do nikogo: człowiek przestawia notatkę
//! `this-agent` na „in use", widzi ją na ekranie w sekcji Pamięć i żaden krok nigdy się o niej
//! nie dowiaduje. To jest ta sama klasa, którą niezmiennik 29 nazywa wprost — mechanizm
//! istnieje, ekran o nim mówi, odbiorcy nie ma.
//!
//! DLACZEGO TO MIERZYMY NA PROMPCIE, A NIE NA WARTOŚCI FUNKCJI. Bo pytanie brzmi „co dostał
//! agent", a nie „co policzyliśmy". Funkcja składająca blok może zwracać idealny tekst i nie
//! mieć wołającego — dokładnie tak `what_you_know` przeżyło od T-17 do T-30 z trzema plikami
//! testowymi i zerem produkcyjnych czytelników. Dubler stoi więc tam, gdzie stoi vendor:
//! dostaje `RunSpec` i zapisuje `prompt`, czyli te bajty, które naprawdę pojechałyby stdinem.
//!
//! **Słabą wersją tego kryterium jest `assert!(prompt.contains(BACK))` na jednym kroku.**
//! Przechodzi dla implementacji, która dokleja KAŻDĄ notatkę `this-agent` do KAŻDEGO kroku —
//! czyli dla tej, która zamienia zakres agenta w drugi zakres projektu i cicho podwaja rachunek
//! za długość każdego promptu. Rozróżniają to dwa kroki dwóch różnych agentów, sądzone w obie
//! strony: swoje ma dojść, cudze nie ma prawa.
//!
//! **Drugą słabą wersją jest liczenie samej obecności.** Blok policzony per krok i dopisany
//! OBOK bloku policzonego per bieg daje notatkę agenta dwa razy w jednym promptcie; wygląda to
//! poprawnie na `contains` i kosztuje podwójnie w każdej turze. Dlatego każda asercja niżej
//! liczy WYSTĄPIENIA, nie obecność.
//!
//! **Trzecią jest zamiana budżetu.** Trzeci blok ma się DOLICZAĆ do dwóch pozostałych, każdy
//! przeciw własnemu sufitowi (`Scope::cap`: 1000 / 1500 / 800). Implementacja, która liczy blok
//! agenta budżetem projektu, przepuszcza notatkę wartą 900 jednostek — mieści się w 1500 i nie
//! mieści w 800. Taka notatka jest tu zasiana i ma NIE dojechać, a dwa pozostałe zakresy mają
//! w tym samym promptcie dojechać mimo niej.
//!
//! JAK NOTATKA WSKAZUJE AGENTA. Nazwą, którą w pliku pisze człowiek (`agent: backend-dev`),
//! a agent nazywa się `Backend Dev` — porównanie idzie więc przez tę samą normalizację, która
//! robi z tytułu nazwę pliku (`memory::slugify`). Fikstura celowo różni jedno od drugiego
//! wielkością liter i spacją: identyfikator z biblioteki w tym polu byłby wartością, której
//! człowiek nie umie ani napisać, ani przeczytać w edytorze (niezmiennik 4).

// `unwrap()`/`expect()` w teście: panika w teście JEST jego wynikiem, a `?` w tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
//
// `too_many_lines` — wszystkie punkty tego kryterium mierzą JEDEN bieg i muszą stać w jednym
// `#[test]`: dwa kroki dzielą jeden magazyn notatek, jednego dublera i jedną migawkę tego, co
// dubler zobaczył, więc cięcie po granicy funkcji znaczyłoby dwa osobne biegi albo stan
// dzielony między testami, które cargo uruchamia równolegle.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
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
use loadout_lib::memory::notes::Scope;
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera. Nie „claude" i nie „codex": tamte mają w biegu własne wymagania
/// co do dowodów, a to kryterium sądzi prompt, nie sterownik.
const VENDOR: &str = "fake";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(20);

/// Znaczniki notatek. Na tyle dziwne, żeby nie mogły powstać z żadnego innego fragmentu tekstu.
const EVERY: &str = "IBEX-EVERYWHERE";
const PROJECT: &str = "IBEX-THIS-PROJECT";
const BACKEND_KNOWS: &str = "IBEX-BACKEND-ONLY";
const FRONTEND_KNOWS: &str = "IBEX-FRONTEND-ONLY";
const BACKEND_SUGGESTED: &str = "IBEX-BACKEND-SUGGESTED";
const BACKEND_TOO_LONG: &str = "IBEX-BACKEND-OVER-THE-CEILING";

/// Znaczniki instrukcji kroków. Prompt zaczyna się od bloku „co wiadomo", więc kroku nie da się
/// rozpoznać po jego początku — rozpoznajemy po treści zadania, która jest tym, co ten krok
/// naprawdę dostał.
const BACKEND_STEP: &str = "IBEX-STEP-BACKEND";
const FRONTEND_STEP: &str = "IBEX-STEP-FRONTEND";

/// Ile jednostek długości ma notatka, która ma się zmieścić. Cztery zdania zmieszczą się
/// w każdym z trzech sufitów.
const SMALL: usize = 40;

/// Ile jednostek ma notatka, która ma się NIE zmieścić w suficie agenta (800) i zmieściłaby
/// się w suficie projektu (1500). To jest jedyna liczba w tym pliku, która rozróżnia „każdy
/// zakres ma własny budżet" od „wszystkie trzy dzielą jeden".
const OVER_THE_AGENT_CEILING: usize = 900;

const BACKEND_ID: &str = "01990000-0000-7000-8000-0000000000b1";
const FRONTEND_ID: &str = "01990000-0000-7000-8000-0000000000f1";

/// Agent nazywa się inaczej, niż pisze o nim plik notatki: `Backend Dev` kontra `backend-dev`.
const BACKEND_AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000b1
name: Backend Dev
summary: Works where the data is
color: slate
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

const FRONTEND_AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000f1
name: Frontend Dev
summary: Works where the window is
color: slate
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

/// Dwa kroki, dwóch agentów, ani jednej strzałki. Każdy na własnej kopii: dwa kroki bez
/// strzałki pracujące w folderze człowieka są odmową z niezmiennika 12, a to kryterium sądzi
/// prompt, nie odmowę.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_memory_per_agent",
  "name": "Two agents, two memories",
  "steps": [
    {
      "kind": "agent",
      "id": "s_backend",
      "name": "Backend",
      "agent": "01990000-0000-7000-8000-0000000000b1",
      "overrides": {},
      "instructions": "IBEX-STEP-BACKEND look at the queue and say what it is doing.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_frontend",
      "name": "Frontend",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "IBEX-STEP-FRONTEND look at the window and say what it is doing.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    }
  ],
  "links": []
}
"#;

/// Reguła o **dokładnie** `units` jednostkach długości. `est_tokens` liczy cztery bajty na
/// jednostkę, więc długość reguły jest jedyną rzeczą, która o tym decyduje.
fn rule_worth(units: usize, sentinel: &str) -> String {
    let wanted = units * 4;
    let mut rule = format!("{sentinel} a sentence long enough to be worth {units} units ");
    assert!(
        rule.len() < wanted,
        "the sentinel alone is longer than the note is supposed to be"
    );
    while rule.len() < wanted {
        rule.push('x');
    }
    rule
}

/// Plik notatki, wypisany co do bajtu. Żaden nie powstał przez zapis Loadouta: pliki są prawdą,
/// a skan czytający wyłącznie to, co sam zapisał, nie odpowiada na to pytanie.
fn note_file(scope: &str, agent: Option<&str>, status: &str, title: &str, rule: &str) -> String {
    let owner = agent.map_or_else(String::new, |name| format!("agent: {name}\n"));
    format!(
        "---\n\
         scope: {scope}\n\
         {owner}\
         kind: rule\n\
         title: {title}\n\
         rule: {rule}\n\
         because: somebody watched this happen twice and wrote it down the second time\n\
         status: {status}\n\
         occurrences: 1\n\
         modified: 2026-08-20T09:00:00Z\n\
         last_used_at: null\n\
         ---\n"
    )
}

/// Ile razy `needle` stoi w `haystack`. Obecność nie wystarcza: blok policzony per krok
/// i dopisany obok bloku policzonego per bieg daje tę samą notatkę dwa razy.
fn times(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn each_step_is_told_what_its_own_agent_knows_and_nothing_of_the_other()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("backend", BACKEND_AGENT)?;
    bench.agent("frontend", FRONTEND_AGENT)?;

    bench.note(
        "everyone-knows-this",
        &note_file(
            "everywhere",
            None,
            "in-use",
            "Prompts travel on stdin",
            &rule_worth(SMALL, EVERY),
        ),
    )?;
    bench.note(
        "this-project-knows-this",
        &note_file(
            "this-project",
            None,
            "in-use",
            "The tenant is resolved before the guard",
            &rule_worth(SMALL, PROJECT),
        ),
    )?;
    bench.note(
        "backend-knows-this",
        &note_file(
            "this-agent",
            Some("backend-dev"),
            "in-use",
            "The queue is drained in one place",
            &rule_worth(SMALL, BACKEND_KNOWS),
        ),
    )?;
    bench.note(
        "frontend-knows-this",
        &note_file(
            "this-agent",
            Some("frontend-dev"),
            "in-use",
            "One fact has one live region",
            &rule_worth(SMALL, FRONTEND_KNOWS),
        ),
    )?;
    bench.note(
        "backend-was-only-suggested",
        &note_file(
            "this-agent",
            Some("backend-dev"),
            "suggested",
            "Retry the flaky suite",
            &rule_worth(SMALL, BACKEND_SUGGESTED),
        ),
    )?;
    bench.note(
        "backend-wrote-far-too-much",
        &note_file(
            "this-agent",
            Some("backend-dev"),
            "in-use",
            "Everything there is to know about the queue",
            &rule_worth(OVER_THE_AGENT_CEILING, BACKEND_TOO_LONG),
        ),
    )?;

    let workflow = bench.workflow("memory-per-agent", WORKFLOW)?;
    let store = Store::open(&bench.db())?;
    let seen = Arc::new(Seen::default());

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&seen)),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 2,
        task: None,
    };

    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))??;
    let _ = tokio::time::timeout(PATIENCE, pump).await;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; 2],
        "both steps have to finish, or every assertion below is true of a step that never ran. \
         They ended as {:?}",
        report.steps
    );

    let looked = seen.snapshot();
    let backend = looked
        .get(BACKEND_STEP)
        .ok_or("the backend step never reached the driver")?;
    let frontend = looked
        .get(FRONTEND_STEP)
        .ok_or("the frontend step never reached the driver")?;

    // (a) SWOJA NOTATKA DOCHODZI, I DOKŁADNIE RAZ. Ten punkt stoi pierwszy, bo cała reszta
    //     pliku jest spełniona przez implementację, która nie dokleja niczego — czyli przez tę,
    //     która jest tu dziś.
    assert_eq!(
        times(backend, BACKEND_KNOWS),
        1,
        "the note this agent's own memory holds reached its step {} time(s). Once is the whole \
         answer: zero means the third scope still goes nowhere and the person who approved it \
         was told a story about a prompt nobody assembles; twice means the block is counted \
         once per run AND once per step, and every turn pays for the same sentence two times. \
         The prompt reads:\n{backend}",
        times(backend, BACKEND_KNOWS)
    );

    // (b) CUDZA NIE DOCHODZI. Bez tego punktu implementacja doklejająca każdemu krokowi każdą
    //     notatkę `this-agent` wygląda dokładnie jak poprawna.
    assert_eq!(
        times(backend, FRONTEND_KNOWS),
        0,
        "the other agent's private note reached this step. A scope that reaches everybody is a \
         second project scope wearing the name of the first, and the ceiling it is counted \
         against stops meaning anything. The prompt reads:\n{backend}"
    );
    assert_eq!(
        times(frontend, BACKEND_KNOWS),
        0,
        "and the same in the other direction. The prompt reads:\n{frontend}"
    );
    assert_eq!(
        times(frontend, FRONTEND_KNOWS),
        1,
        "and this step gets its own, exactly once. The prompt reads:\n{frontend}"
    );

    // (c) TRZECI BLOK DOLICZA SIĘ DO DWÓCH POZOSTAŁYCH, a nie zamiast nich. Implementacja,
    //     która zamienia bloki, przechodzi (a) i (b) i cicho zabiera obu krokom wszystko, co
    //     człowiek zatwierdził dla całego projektu.
    for (name, prompt) in [("backend", backend), ("frontend", frontend)] {
        assert_eq!(
            times(prompt, EVERY),
            1,
            "the {name} step lost the scope that holds for every project, or got it twice. The \
             agent's block is counted BESIDE the other two, never instead of them. The prompt \
             reads:\n{prompt}"
        );
        assert_eq!(
            times(prompt, PROJECT),
            1,
            "the {name} step lost this project's scope, or got it twice. The prompt \
             reads:\n{prompt}"
        );
    }

    // (d) TYLKO TO, CO CZŁOWIEK DOPUŚCIŁ. Filtr po statusie stoi w jednym miejscu i ma tam
    //     zostać: kandydatka doklejona „żeby model miał kontekst" zamienia jedną halucynację
    //     w trwałe prawo projektu [00-SYNTHESIS §2.1].
    assert_eq!(
        times(backend, BACKEND_SUGGESTED),
        0,
        "a note nobody ever approved reached the model, on the road that was just built for the \
         third scope. The prompt reads:\n{backend}"
    );

    // (e) WŁASNY SUFIT. 900 jednostek mieści się w budżecie projektu (1500) i nie mieści się
    //     w budżecie agenta (800), więc ta notatka rozróżnia „każdy zakres ma własny sufit" od
    //     „trzy zakresy dzielą jeden".
    assert!(
        OVER_THE_AGENT_CEILING > Scope::ThisAgent.cap()
            && OVER_THE_AGENT_CEILING < Scope::ThisProject.cap(),
        "the fixture has to sit BETWEEN the two ceilings, or it stops telling the two \
         implementations apart: {OVER_THE_AGENT_CEILING} units against {} for an agent and {} \
         for a project",
        Scope::ThisAgent.cap(),
        Scope::ThisProject.cap()
    );
    assert_eq!(
        times(backend, BACKEND_TOO_LONG),
        0,
        "a single note worth {OVER_THE_AGENT_CEILING} units reached a step whose agent scope \
         holds {}. Each scope is counted against its own ceiling [T6 §5.3]; borrowing the \
         project's budget for the agent's block is how the number in `Scope::cap` stops \
         limiting anything at all. The prompt reads:\n{backend}",
        Scope::ThisAgent.cap()
    );

    // (f) I KROK DALEJ MA SWOJE ZADANIE. Implementacja, która prompt kroku ZASTĘPUJE blokiem
    //     pamięci, przechodzi wszystko powyżej, a agent nie dostaje roboty.
    assert!(
        backend.contains(BACKEND_STEP) && frontend.contains(FRONTEND_STEP),
        "a step lost its own instructions somewhere under the memory block. What the agent knows \
         stands ABOVE the work, never in place of it"
    );

    Ok(())
}

// ── co dubler zobaczył ─────────────────────────────────────────────────────────────────────

/// Prompt każdego kroku, po znaczniku jego instrukcji.
#[derive(Debug, Default)]
struct Seen(Mutex<BTreeMap<String, String>>);

impl Seen {
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym
    /// wywołaniu, więc nie ma wyrażenia, w którym dożyłby do `await`.
    fn record(&self, step: &str, prompt: String) {
        self.lock().insert(step.to_owned(), prompt);
    }

    fn snapshot(&self) -> BTreeMap<String, String> {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, String>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers(seen: Arc<Seen>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { seen });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct Fake {
    seen: Arc<Seen>,
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
        // Krok rozpoznajemy po znaczniku jego instrukcji. Prompt, w którym nie ma żadnego,
        // ląduje pod SWOJĄ treścią: asercja o nazwach kroków ma wtedy paść i pokazać, czego
        // test nie rozpoznał, zamiast po cichu przypisać cudzy prompt.
        let step = [BACKEND_STEP, FRONTEND_STEP]
            .into_iter()
            .find(|marker| spec.prompt.contains(marker))
            .map_or_else(|| spec.prompt.clone(), ToOwned::to_owned);
        self.seen.record(&step, spec.prompt.clone());

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
        // Ten sam korzeń, który rozwiązuje `commands::memory::notes_root`.
        fs::create_dir_all(home.path().join("memory").join("notes"))?;
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

    fn note(&self, slug: &str, text: &str) -> Result<(), Box<dyn Error>> {
        fs::write(
            self.home
                .path()
                .join("memory")
                .join("notes")
                .join(format!("{slug}.md")),
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
