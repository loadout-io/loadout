//! AC-1 dla T-70: rozmowa widzi bibliotekę, bieg jej nie widzi.
//!
//! # Po co to istnieje
//!
//! Lider startuje z `extra_dirs: Vec::new()` (`commands::chat`), więc widzi **wyłącznie folder
//! zakresu**. Workflow i agenci leżą w `~/.loadout/workflows` i `~/.loadout/agents`, czyli poza
//! jego zasięgiem. Skutek jest dokładnie taki, jak brzmi: „załóż mi agenta do recenzji" kończy
//! się zdaniem, jak to zrobić RĘCZNIE. Lider, który zna twoją bibliotekę z rozmowy i nie ma do
//! niej dostępu, jest doradcą odciętym od jedynych plików, o których rozmawiacie.
//!
//! # Cicha porażka, przed którą stoi ten plik
//!
//! Dosypanie katalogów **wszystkim**. Krok biegu z dostępem do `~/.loadout` może nadpisać
//! definicję agenta w trakcie biegu, który z niej właśnie korzysta — a bieg czyta ten plik raz,
//! przy starcie kroku. Nic się wtedy nie przewraca i nikt nie dostaje ani jednego ostrzeżenia:
//! awarię widać dopiero przy NASTĘPNYM biegu, kiedy „ten sam workflow" robi co innego.
//!
//! Z zewnątrz obie implementacje wyglądają identycznie — dopóki nie zapyta się o krok. Dlatego
//! ten plik pyta o oba końce naraz i dlatego drugi przypadek jest tu tak samo obowiązkowy jak
//! pierwszy.
//!
//! # Słaba wersja tego kryterium
//!
//! Sam punkt (a), czyli „rozmowa ma niepuste `extra_dirs`". Przechodzi dla implementacji, która
//! dosypuje katalogi wszystkim — czyli dla tej jednej wersji, przed którą całe to zadanie stoi.
//! Rozróżnia je (b): ani jeden z tych dwóch katalogów nie ma prawa dojechać do kroku.
//!
//! Druga słaba wersja jest cichsza: porównanie dwóch pustek. Ścieżka wpisana tu z literału ma
//! szansę nie istnieć w drzewie fikstury, a wtedy „krok jej nie dostał" jest prawdą o niczym.
//! Stąd obie ścieżki biorą się z **produkcyjnego zapisu biblioteki** (`save_agent_inner`,
//! `save_workflow_inner`) albo z pliku, który bieg naprawdę przeczytał — i stąd asercje
//! o ich istnieniu przed każdym pomiarem.

// `expect()` w teście: panika w teście JEST jego wynikiem, a `?` w tej samej linii zamieniłby
// nazwany komunikat asercji w bezimienne `Err`. Ten sam idiom i ten sam powód, co
// w `lead_comes_from_the_agent` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::commands::chat::{Lead, Threads};
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::workflows::save_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens, Voice,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel};
use loadout_lib::library::agents::{Agent, FileAccess, Vendor};
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use tempfile::TempDir;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Ile miejsca w strumieniu linii rozmowy. Z zapasem — mierzymy specyfikację sesji, nie
/// przepustowość.
const LINES: usize = 32;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Ile trwa jedna tura dublera.
const TURN: Duration = Duration::from_millis(20);

/// Ile czekamy na cały bieg, zanim uznamy go za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(20);

/// Identyfikator agenta pierwszego kroku — ten sam napis, co w [`WORKFLOW`].
const PLANNER_ID: &str = "01990000-0000-7000-8000-0000000000f1";
/// To samo dla drugiego kroku.
const BUILDER_ID: &str = "01990000-0000-7000-8000-0000000000f2";

/// Instrukcja pierwszego kroku. Rozpoznawalna, bo po niej — a nie po katalogu roboczym —
/// rozpoznajemy jego specyfikację: oba kroki tej fikstury stoją w folderze projektu.
const PLANNER_SAYS: &str = "lay out the work before anybody writes anything";
/// To samo dla drugiego kroku, czyli tego, o którego prawa do czytania pyta punkt (c).
const BUILDER_SAYS: &str = "write the change the first step described";

/// Odpowiedź dublera. Krótka: przekazanie ma powstać, ale bez cięcia, bo `attachments/`
/// dokładałoby do `extra_dirs` drugi katalog i zaciemniało punkt (c).
const REPLY: &str = "Done, and the index column is the place to start.";

/// Dwa kroki połączone **jedną** strzałką, pisane ręcznie.
///
/// Fikstura zbudowana naszym serializatorem definiowałaby kształt, zamiast go sprawdzać: zmiana
/// kształtu przechodziłaby wtedy po obu stronach naraz [04 §6.4]. Strzałka jest tu obowiązkowa —
/// bez niej drugi krok nie ma poprzednika, nie dostaje przekazania i punkt (c) nie ma o co pytać.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_plan_then_build",
  "name": "Plan then build",
  "steps": [
    {
      "kind": "agent",
      "id": "s_plan",
      "name": "Planner",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "lay out the work before anybody writes anything",
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_build",
      "name": "Builder",
      "agent": "01990000-0000-7000-8000-0000000000f2",
      "overrides": {},
      "instructions": "write the change the first step described",
      "at": { "x": 240, "y": 0 }
    }
  ],
  "links": [{ "from": "s_plan", "to": "s_build" }]
}
"#;

// ── (a) + (d) ROZMOWA DOSTAJE OBA KATALOGI BIBLIOTEKI ──────────────────────────────────────

#[tokio::test]
async fn the_conversation_is_handed_both_folders_of_the_library() -> Result<(), Box<dyn Error>> {
    let library = TempDir::new()?;
    let scope = TempDir::new()?;

    // GDZIE BIBLIOTEKA TRZYMA JEDNO I DRUGIE — spytane jej własnego zapisu, nie sklejone tutaj
    // z literałów. Napis `"agents"` wpisany w ten plik zgadzałby się z produkcją dokładnie do
    // dnia, w którym produkcja go zmieni, a wtedy asercja niżej porównywałaby ścieżkę, której
    // nikt nie czyta, ze ścieżką, której nikt nie pisze.
    let lead_agent = definition(
        PLANNER_ID,
        "Lead",
        FileAccess::LookOnly,
        "advise, do not run",
    )?;
    let agents_dir = folder_of(&save_agent_inner(library.path(), &lead_agent)?)?;
    let workflows_dir = folder_of(&saved_workflow(library.path(), scope.path())?)?;

    // KONTROLA PRZECIW PUSTEMU PRZEJŚCIU. Bez tych trzech linii wszystko niżej przechodzi dla
    // fikstury, w której jedna ze ścieżek nie istnieje albo obie są tą samą ścieżką — czyli
    // mierzyłoby jeden katalog i nazywało go dwoma.
    assert!(
        agents_dir.is_dir(),
        "the fixture has no agents folder at {}, so \"the lead can reach it\" would be a \
         sentence about nothing",
        agents_dir.display()
    );
    assert!(
        workflows_dir.is_dir(),
        "the fixture has no workflow folder at {}, so \"the lead can reach it\" would be a \
         sentence about nothing",
        workflows_dir.display()
    );
    assert_ne!(
        agents_dir, workflows_dir,
        "both halves of the library resolved to one folder, so this criterion would judge a \
         single path twice and call it two"
    );
    // I kontrola do tej samej rodziny: folder, w którym człowiek pracuje, leży POZA biblioteką.
    // Zakres wskazany w nią zdawałby lidera z prawami, których nikt mu nie nadał.
    assert!(
        !scope.path().starts_with(library.path()),
        "the fixture's working folder lies inside the library, so the lead would reach these two \
         folders with nothing added at all"
    );

    let lead = Lead::pointed_at(library.path(), Some(&lead_agent.id.to_string()))
        .map_err(|refusal| refusal.to_string())
        .expect("the agent was just saved, so the pointed-at lead has to resolve");

    let (drivers, watch) = one_vendor();
    let spec = one_sentence(&drivers, &watch, library.path(), &lead, scope.path()).await?;

    assert!(
        spec.extra_dirs.iter().any(|dir| dir == &agents_dir),
        "the lead was not given {}, so \"set up an agent for reviews\" ends in instructions for \
         doing it by hand: the one file the two of you are talking about is out of reach. It was \
         given {:?}",
        agents_dir.display(),
        spec.extra_dirs
    );
    assert!(
        spec.extra_dirs.iter().any(|dir| dir == &workflows_dir),
        "the lead was not given {}, so \"fix that step in my workflow\" is a promise it cannot \
         keep. It was given {:?}",
        workflows_dir.display(),
        spec.extra_dirs
    );
    Ok(())
}

// ── (b) + (c) + (d) KROK BIEGU NIE DOSTAJE ANI JEDNEGO Z NICH, A SWÓJ ZOSTAJE ───────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_run_step_is_handed_neither_and_keeps_what_it_had() -> Result<(), Box<dyn Error>> {
    // `_bench` żyje do końca funkcji: to w jego katalogach leży bieg, a `TempDir` kasuje je
    // w `Drop`.
    let (report, watch, bench) = plan_then_build().await?;
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded, StepState::Succeeded],
        "both steps have to run for their reading rights to exist; they ended as {:?}",
        report.steps
    );

    // Te same dwa katalogi, wzięte z plików, które ten bieg NAPRAWDĘ przeczytał: agent stąd
    // został rozwiązany po identyfikatorze, a plik workflow stąd wczytany. Ścieżka wymyślona
    // w teście dałaby zieleń dla każdej implementacji, bo krok i tak by jej nie miał.
    let agents_dir = folder_of(&bench.planner)?;
    let workflows_dir = folder_of(&bench.workflow)?;
    assert!(
        agents_dir.is_dir() && workflows_dir.is_dir(),
        "the run read its agent from {} and its workflow from {}, so both folders have to exist \
         — otherwise this case compares two nothings",
        agents_dir.display(),
        workflows_dir.display()
    );

    let started = watch.started();
    assert_eq!(
        started.len(),
        2,
        "two steps ran, so two specifications had to reach the driver; {} did",
        started.len()
    );

    // (b) ANI JEDEN KROK, ANI JEDEN KATALOG.
    for spec in &started {
        for forbidden in [&agents_dir, &workflows_dir] {
            assert!(
                !spec.extra_dirs.iter().any(|dir| dir == forbidden),
                "a run step was given {}. A step that may write into the library can overwrite \
                 the definition of an agent the very run it belongs to is using — and the run \
                 read that file once, at the step's start, so nothing breaks today. The failure \
                 shows up in the NEXT run, when \"the same workflow\" does something else. It was \
                 given {:?}",
                forbidden.display(),
                spec.extra_dirs
            );
        }
    }

    // (c) TO, CO KROK DOSTAWAŁ DOTĄD, ZOSTAJE. Ta zmiana nie ma prawa nic zabrać.
    let handoffs = where_the_handoffs_landed(&report.dir)?;
    let second = started
        .iter()
        .find(|spec| spec.prompt.contains(BUILDER_SAYS))
        .ok_or("the second step never reached the driver, so it has no reading rights to judge")?;
    assert!(
        second.extra_dirs.iter().any(|dir| dir == &handoffs),
        "the second step lost {}, and that is the folder holding the only copy of what the first \
         step handed it. Its prompt carries paths, never bodies, so a path it may not open is a \
         link with no handler (invariant 16). It was given {:?}",
        handoffs.display(),
        second.extra_dirs
    );
    Ok(())
}

// ── Fikstura rozmowy ───────────────────────────────────────────────────────────────────────

/// Definicja agenta, jaką człowiek zapisał w bibliotece.
///
/// `Agent::example()` jako baza, bo „jak wygląda zapisany agent" ma w tym repo jedną odpowiedź
/// (`library::agents`), a ręcznie wypisane piętnaście pól byłoby drugą — i tą, która przestanie
/// się deserializować przy pierwszym nowym kluczu. `write_results_to` czyszczone z rozmysłem:
/// przykład niesie tam ścieżkę, a dwa kroki piszące po tej samej ścieżce to odmowa przy Starcie
/// (niezmiennik 12), czyli kryterium, którego nie dałoby się spełnić nigdy.
fn definition(
    id: &str,
    name: &str,
    access: FileAccess,
    says: &str,
) -> Result<Agent, Box<dyn Error>> {
    Ok(Agent {
        id: Uuid::parse_str(id)?,
        name: name.to_owned(),
        runs_with: Vendor::ClaudeCode,
        file_access: access,
        instructions: says.to_owned(),
        write_results_to: String::new(),
        ..Agent::example()
    })
}

/// Workflow zapisany **produkcyjną drogą**, żeby oddał ścieżkę, pod którą biblioteka go trzyma.
///
/// Bajty pisze ręka ([`WORKFLOW`]), a czyta je produkcyjny wczytywacz: fikstura zbudowana naszym
/// serializatorem definiowałaby kształt, zamiast go sprawdzać. Szkic ląduje POZA biblioteką, bo
/// plik leżący w jej korzeniu byłby wpisem, którego nie zapisała żadna komenda.
fn saved_workflow(library: &Path, scratch: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let drafted = scratch.join("drafted-by-hand.json");
    fs::write(&drafted, WORKFLOW)?;
    Ok(save_workflow_inner(
        library,
        "plan-then-build.json",
        &load(&drafted)?,
    )?)
}

/// Katalog, w którym leży ten plik.
fn folder_of(file: &Path) -> Result<PathBuf, Box<dyn Error>> {
    Ok(file
        .parent()
        .ok_or_else(|| format!("{} has no folder above it", file.display()))?
        .to_path_buf())
}

/// Jedno zdanie powiedziane wskazanemu liderowi w tym zakresie → specyfikacja jego sesji.
///
/// Strumień zakładamy tak, jak zakłada go okno (`open_chat` → `lines_go_to`), bo wątek bez kanału
/// jest wątkiem, którego wierszy nikt nie odbiera — a to jest inny stan niż ten, o który pytamy.
async fn one_sentence(
    drivers: &Drivers,
    watch: &Watch,
    library: &Path,
    lead: &Lead,
    cwd: &Path,
) -> Result<RunSpec, Box<dyn Error>> {
    let (sink, _source) = line_channel(LINES);
    let threads = Threads::new();
    threads.library_is(library.to_path_buf());
    threads.lines_go_to(cwd.to_path_buf(), sink);
    threads
        .say(
            drivers,
            lead,
            cwd.to_path_buf(),
            "what workflows do I have saved?",
        )
        .await
        .map_err(|refusal| refusal.to_string())?;
    watch
        .started()
        .into_iter()
        .next()
        .ok_or_else(|| "the first sentence to a pointed-at lead has to open a session".into())
}

// ── Fikstura biegu ─────────────────────────────────────────────────────────────────────────

/// Bieg fikstury: planista oddaje pole budowniczemu, obaj kończą.
async fn plan_then_build() -> Result<(RunReport, Arc<Watch>, Bench), Box<dyn Error>> {
    let bench = Bench::new()?;
    the_fixture_can_run(&bench.workflow)?;
    let store = Store::open(&bench.db())?;

    let (drivers, watch) = one_vendor();
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers,
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: bench.workflow.clone(),
        how_many_at_once: 2,
        task: None,
        part: None,
        handoffs_from: None,
    };

    // Linie tego kryterium nie interesują: sądzi ono `RunSpec`, który przeszedł do sterownika.
    let (lines, _source) = line_channel(QUEUE_CAP);
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, lines))
        .await
        .map_err(|_| format!("the run did not finish within {PATIENCE:?}"))??;
    Ok((report, watch, bench))
}

/// Fikstura ma przejść walidator **bez ani jednego problemu**.
///
/// To nie jest część kryterium, tylko jego przesłanka, i dlatego stoi przed biegiem. Czerwień
/// w fazie kontraktu wygląda identycznie dla „zachowania jeszcze nie ma" i dla „tego kryterium
/// nie da się spełnić nigdy": workflow odrzucony przez `workflow::check` byłby odmową w KAŻDEJ
/// implementacji, a test nazywałby to brakiem zachowania.
fn the_fixture_can_run(workflow: &Path) -> Result<(), Box<dyn Error>> {
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
    Ok(())
}

/// Katalog, w którym naprawdę wylądowały przekazania tego biegu.
///
/// Znaleziony **po pliku, nie po nazwie**: druga kopia napisu `handoffs` byłaby drugim miejscem
/// do poprawienia w dniu, w którym `memory::handoff` tę nazwę zmieni, i tym niepoprawionym —
/// dokładnie ten sam powód, dla którego `commands::run::prompt_for` bierze ten katalog ze ścieżki
/// przekazania, a nie ze stałej.
fn where_the_handoffs_landed(run_dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let mut holding: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(run_dir)? {
        let dir = entry?.path();
        if !dir.is_dir() {
            continue;
        }
        let has_handoff = fs::read_dir(&dir)?.flatten().any(|file| {
            file.path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        });
        if has_handoff {
            holding.push(dir);
        }
    }
    match holding.as_slice() {
        [only] => Ok(only.clone()),
        other => Err(format!(
            "expected exactly one folder of handoffs under {}, found {}: without it the first \
             step handed the second one nothing and there are no reading rights to judge",
            run_dir.display(),
            other.len()
        )
        .into()),
    }
}

/// Biblioteka użytkownika i projekt na czas jednego kryterium — obie połowy zapisane produkcyjną
/// drogą, więc bez ani jednej nazwy katalogu wpisanej tutaj.
#[derive(Debug)]
struct Bench {
    home: TempDir,
    project: TempDir,
    /// Plik agenta pierwszego kroku, tam gdzie położyła go biblioteka.
    planner: PathBuf,
    /// Plik workflow, tam gdzie położyła go biblioteka.
    workflow: PathBuf,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        // `Store::open` zakłada plik bazy, ale nie katalog nad nim.
        fs::create_dir_all(project.path().join(".loadout"))?;

        let planner = save_agent_inner(
            home.path(),
            &definition(PLANNER_ID, "Planner", FileAccess::LookOnly, PLANNER_SAYS)?,
        )?;
        save_agent_inner(
            home.path(),
            &definition(BUILDER_ID, "Builder", FileAccess::WorkFreely, BUILDER_SAYS)?,
        )?;
        let workflow = saved_workflow(home.path(), project.path())?;
        Ok(Self {
            home,
            project,
            planner,
            workflow,
        })
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }
}

// ── Dubler sterownika ──────────────────────────────────────────────────────────────────────

/// Co dubler zapamiętał: specyfikacja KAŻDEGO uruchomienia, w kolejności startu.
///
/// **Ten zamek nigdy nie przechodzi przez `await`** (niezmiennik 8): cały dostęp jest zamknięty
/// w synchronicznych metodach, więc nie ma wyrażenia, w którym guard dożyłby do punktu
/// zawieszenia.
#[derive(Debug, Default)]
struct Watch {
    started: Mutex<Vec<RunSpec>>,
}

impl Watch {
    fn started(&self) -> Vec<RunSpec> {
        self.started
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn saw(&self, spec: RunSpec) {
        self.started
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(spec);
    }
}

/// Fabryka oddająca ten sam dubler każdemu vendorowi: o wybór vendora pyta AC-1 z T-60, nie ten
/// plik.
fn one_vendor() -> (Drivers, Arc<Watch>) {
    let watch = Arc::new(Watch::default());
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        watch: Arc::clone(&watch),
    });
    (Arc::new(move |_vendor| Arc::clone(&driver)), watch)
}

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
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };
        // Zapis PRZED pierwszym zdarzeniem: prawa do czytania są tym, co ten krok dostał na
        // wejściu, a nie tym, co z niego wynikło.
        self.watch.saw(spec);
        /* Odbiornik głosu żyje tak długo, jak sesja: porzucony razem ze `start` zamykałby kanał,
         * a wtedy każda następna tura odbijałaby się o „stopped listening" i mierzylibyśmy własne
         * sprzątanie. */
        let (voice, mut heard) = mpsc::channel(4);
        tokio::spawn(async move { while heard.recv().await.is_some() {} });
        Ok(Box::new(Turn {
            events,
            session,
            voice,
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    voice: Voice,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn voice(&self) -> Option<Voice> {
        Some(self.voice.clone())
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
        tokio::time::sleep(TURN).await;
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: REPLY.to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: TURN,
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
