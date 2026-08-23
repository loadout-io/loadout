//! AC-4 dla T-92: notatka pamięta, kiedy ostatnio weszła do promptu.
//!
//! `Note::last_used_at` istnieje od T-17 i pole w pliku mówi o nim tak: „zapisuje krok składania
//! promptu (T-15)". **Nie zapisuje.** Zmierzone 2026-08-23 po 23 biegach właściciela: wartość
//! jest pisana raz, jako `null` przy powstaniu notatki, i nigdy potem nie rusza się z miejsca.
//!
//! Skutek nie jest kosmetyczny i ma jednego adresata. Kiedy zakres jest pełny, `promote` odmawia
//! i pokazuje człowiekowi **wymuszony wybór** — listę notatek do odstawienia, „najdawniej użyte
//! pierwsze" [T6 §5.3]. Ta lista sortuje się po `last_used_at`, a skoro tam stoi wszędzie `null`,
//! to sortuje się po identyfikatorze, czyli **po nazwie pliku**. Człowiekowi, który ma zdecydować,
//! co przestaje docierać do modelu, pokazujemy zdania ułożone alfabetycznie i mówimy, że są
//! ułożone po tym, jak dawno były potrzebne. To jest zła odpowiedź udzielona pewnym głosem —
//! ta sama rodzina, co „mechanizm istnieje, odbiorcy nie ma" (niezmiennik 29), tylko z drugiej
//! strony: odbiorca jest, a fakt, który dostaje, jest zmyślony.
//!
//! # Trzy słabe wersje tego kryterium
//!
//! **Pierwsza: zawołać funkcję stemplującą wprost i sprawdzić plik.** Przechodzi na funkcji bez
//! ani jednego produkcyjnego wołającego — dokładnie tak `what_you_know` przeżyło od T-17 do T-30
//! z trzema plikami testowymi i zerem czytelników. Dlatego stempluje tu **prawdziwy bieg**,
//! przez `run_workflow_inner`, a dubler stoi tam, gdzie stoi vendor.
//!
//! **Druga: sprawdzić, że stempel dostała każda notatka `in-use`.** Przechodzi na implementacji,
//! która stempluje CAŁY zamrożony zbiór, nie oglądając się na budżet. Notatka, która nie zmieściła
//! się w suficie zakresu (`Block::dropped`), do modelu nie dojechała — i notatka „użyta wczoraj",
//! która wtedy nigdy nie była w żadnym promptcie, jest gorsza niż `null`, bo `null` przynajmniej
//! nie kłamie. Rozróżnia to notatka zasiana **ponad sufitem** i asercja, że jej plik się nie ruszył.
//!
//! **Trzecia: sprawdzić sam stempel i nie sprawdzić, po co jest.** Wymuszony wybór jest jedynym
//! czytelnikiem tego pola i jedynym miejscem, w którym człowiek widzi skutek. Ostatnia część tego
//! testu prosi więc o promocję, która przepełnia zakres, i czyta kolejność **z odmowy** — czyli
//! stamtąd, skąd bierze ją sekcja Pamięć (`src/state/memory.ts`, `isMemoryFull`).
//!
//! # Jak fikstura rozróżnia obie implementacje
//!
//! Dwie notatki w użyciu, obie `this-project`, obie zasiane z `last_used_at: null`:
//!
//! | plik | długość | co się z nią dzieje |
//! |---|---|---|
//! | `a-locks-and-waiting` | mała | wchodzi do bloku, więc dostaje stempel |
//! | `z-everything-about-the-queue` | ponad sufit zakresu | wypada w `dropped`, więc stempla nie dostaje |
//!
//! Identyfikatory są dobrane **wbrew** oczekiwanej kolejności: alfabetycznie `a-…` stoi pierwsze.
//! Kiedy nikt nie stempluje, obie mają `null` i wymuszony wybór wypisze `[a-…, z-…]`. Kiedy
//! stempluje, `z-…` (nigdy nieużyta) idzie przed `a-…` (użytą przed chwilą) i wychodzi
//! `[z-…, a-…]`. Jedna asercja, dwie implementacje, żadnego marginesu.

// `unwrap()`/`expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
//
// `too_many_lines` — wszystkie punkty tego kryterium mierzą JEDEN bieg: stempel powstaje w czasie
// jego trwania, a wymuszony wybór czyta pliki, które ten bieg zostawił. Cięcie po granicy funkcji
// znaczyłoby dwa biegi albo stan dzielony między testami, które cargo uruchamia równolegle.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::memory::{NoteRefusal, notes_root, put_note_to_use_inner};
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::memory::notes::{Scope, scan_notes};
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera. Nie „claude" i nie „codex": tamte mają w biegu własne wymagania
/// co do dowodów, a to kryterium sądzi pliki notatek, nie sterownik.
const VENDOR: &str = "fake";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(20);

/// Notatka, która wchodzi do bloku. `a-…`, żeby alfabetycznie stała PRZED tą drugą.
const USED_ID: &str = "a-locks-and-waiting";
/// Notatka, która się w suficie nie mieści. `z-…`, żeby alfabetycznie stała ZA tamtą.
const DROPPED_ID: &str = "z-everything-about-the-queue";
/// Kandydatka, której promocja przepełni zakres i otworzy wymuszony wybór.
const WAITING_ID: &str = "m-retry-the-flaky-suite";

/// Znaczniki reguł. Na tyle dziwne, żeby nie mogły powstać z żadnego innego fragmentu tekstu.
const USED_MARK: &str = "IBEX-IN-THE-BLOCK";
const DROPPED_MARK: &str = "IBEX-OVER-THE-CEILING";

/// Znacznik instrukcji kroku: prompt zaczyna się od bloku „co wiadomo", więc kroku nie da się
/// rozpoznać po jego początku.
const STEP_MARK: &str = "IBEX-STEP-ONE";

/// Ile jednostek ma notatka, która ma się zmieścić.
const SMALL: usize = 40;
/// Ile jednostek ma notatka, która sama jedna przekracza sufit zakresu projektu (1500).
const OVER_THE_CEILING: usize = 1600;

/// Chwila, w której człowiek klika „Use this". Nie ma nic wspólnego z chwilą biegu.
const CLICKED: &str = "2026-08-23T12:00:00Z";

const AGENT_ID: &str = "01990000-0000-7000-8000-0000000000a4";

const AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000a4
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

/// Jeden krok, jeden agent. Własna kopia plików, żeby niezmiennik 12 nie miał tu nic do
/// powiedzenia — to kryterium sądzi pliki notatek, nie odmowę przed startem.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_memory_stamps",
  "name": "One step that reads what it knows",
  "steps": [
    {
      "kind": "agent",
      "id": "s_one",
      "name": "Backend",
      "agent": "01990000-0000-7000-8000-0000000000a4",
      "overrides": {},
      "instructions": "IBEX-STEP-ONE look at the queue and say what it is doing.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
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

/// Plik notatki, wypisany co do bajtu — z `last_used_at: null`, czyli tak, jak wygląda KAŻDA
/// notatka w tym repo dzisiaj. Żaden nie powstał przez zapis Loadouta: pliki są prawdą
/// (niezmiennik 4), a stempel, który umie postawić wyłącznie na tym, co sam zapisał, nie
/// odpowiada na pytanie zadane przez to kryterium.
fn note_file(status: &str, title: &str, rule: &str) -> String {
    format!(
        "---\n\
         scope: this-project\n\
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

/// Wartość `last_used_at:` tak, jak leży w pliku. `None` znaczy „null albo brak klucza", czyli
/// to samo, co czyta `scan_notes`.
fn stamp_on_disk(bench: &Bench, id: &str) -> Option<String> {
    scan_notes(&notes_root(bench.home.path()))
        .expect("the notes root has to be readable")
        .into_iter()
        .find(|note| note.id.to_string() == id)
        .unwrap_or_else(|| panic!("no note called {id} is on disk any more"))
        .last_used_at
}

/// Ile razy `needle` stoi w `haystack`. Obecność nie wystarcza: blok policzony dwa razy daje
/// tę samą notatkę dwa razy w jednym prompcie i wygląda poprawnie na `contains`.
fn times(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_note_that_reached_the_model_says_when_and_the_forced_choice_reads_it()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("backend", AGENT)?;

    assert!(
        AGENT.contains(AGENT_ID) && WORKFLOW.contains(AGENT_ID),
        "the fixture names {AGENT_ID} in only one of the two files that have to agree on it — a \
         step without an agent is a run that never starts, and then every assertion below is \
         true of a prompt nobody assembled"
    );
    assert!(
        OVER_THE_CEILING > Scope::ThisProject.cap(),
        "the note that is meant to fall out of the block is worth {OVER_THE_CEILING} units and \
         the scope holds {}. It has to sit ABOVE the ceiling on its own, or it quietly joins the \
         block and stops telling the two implementations apart",
        Scope::ThisProject.cap()
    );

    bench.note(
        USED_ID,
        &note_file(
            "in-use",
            "Never hold a lock across an await",
            &rule_worth(SMALL, USED_MARK),
        ),
    )?;
    bench.note(
        DROPPED_ID,
        &note_file(
            "in-use",
            "Everything there is to know about the queue",
            &rule_worth(OVER_THE_CEILING, DROPPED_MARK),
        ),
    )?;
    bench.note(
        WAITING_ID,
        &note_file(
            "suggested",
            "Retry the flaky suite",
            "A suite that fails once and passes twice is not telling you about the code.",
        ),
    )?;

    let workflow = bench.workflow("memory-stamps", WORKFLOW)?;
    let store = Store::open(&bench.db())?;
    let seen = Arc::new(Seen::default());

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&seen)),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
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
        vec![StepState::Succeeded; 1],
        "the step has to finish, or every assertion below is true of a step that never ran. It \
         ended as {:?}",
        report.steps
    );

    // ── 1. KONTROLA: która notatka NAPRAWDĘ pojechała do modelu ───────────────────────────
    //
    // Bez tego wszystko niżej jest też prawdą o biegu, w którym blok pamięci jest pusty: stempel
    // „bo notatka jest in-use" i stempel „bo notatka weszła do promptu" różnią się dokładnie tym
    // jednym przypadkiem, a ta linia mówi, który z nich mierzymy.
    let prompt = seen
        .snapshot()
        .ok_or("the step never reached the driver, so nothing was ever put in front of a model")?;
    assert_eq!(
        times(&prompt, USED_MARK),
        1,
        "the small note reached the prompt {} time(s), and once is the whole answer. The prompt \
         reads:\n{prompt}",
        times(&prompt, USED_MARK)
    );
    assert_eq!(
        times(&prompt, DROPPED_MARK),
        0,
        "a note worth {OVER_THE_CEILING} units reached a prompt whose scope holds {}. The fixture \
         depends on this one falling out of the block: it is the only note here that separates \
         `stamp what went into the prompt` from `stamp everything in use`. The prompt \
         reads:\n{prompt}",
        Scope::ThisProject.cap()
    );

    // ── 2. Notatka, która weszła do promptu, mówi KIEDY ───────────────────────────────────
    let used = stamp_on_disk(&bench, USED_ID).ok_or_else(|| {
        format!(
            "the note that went into this run's prompt still says `last_used_at: null`. The file \
             says this field is written by the step that assembles the prompt, and it is not: \
             after twenty-three runs on this machine every note still claims it was never used. \
             The forced choice sorts on this field, so a person deciding what stops reaching the \
             model is handed sentences in alphabetical order and told they are in order of last \
             need"
        )
    })?;
    assert!(
        used.starts_with("20") && used.ends_with('Z') && used.len() >= 20,
        "the note came back stamped {used:?}. ISO 8601 UTC is not decoration here: the ordering \
         of that text IS the chronological ordering, which is the only reason the forced choice \
         needs no date parser and this repo needs no clock crate"
    );

    // ── 3. Notatka, która się nie zmieściła, NIE mówi nic ─────────────────────────────────
    assert_eq!(
        stamp_on_disk(&bench, DROPPED_ID),
        None,
        "a note that did not fit the ceiling was stamped as used. It never reached the model — \
         `Block::dropped` is exactly the list of notes that did not — and a note claiming it was \
         used yesterday when it was in no prompt at all is worse than `null`, because `null` at \
         least does not lie. It then outranks genuinely used notes in the forced choice"
    );

    // ── 4. WYMUSZONY WYBÓR CZYTA TO POLE, i to jest jedyne miejsce, w którym widzi to człowiek ─
    //
    // Odmowa jedzie tą samą drogą, którą jedzie do sekcji Pamięć (`put_note_to_use` →
    // `NoteRefusal::Full`), więc kolejność sądzimy dokładnie tam, gdzie ląduje na ekranie.
    let root = notes_root(bench.home.path());
    let refusal = put_note_to_use_inner(&root, WAITING_ID, CLICKED)
        .expect_err("the scope is over its ceiling, so this promotion has to be refused");
    let NoteRefusal::Full { retire, .. } = refusal else {
        panic!(
            "promoting into a full scope came back as an ordinary refusal. The forced choice is \
             the whole point of the ceiling [T6 section 5.3]: silent trimming looks identical to \
             success in the window and differs only in that a note the person approved stops \
             reaching the model"
        )
    };

    assert_eq!(
        retire,
        vec![DROPPED_ID.to_owned(), USED_ID.to_owned()],
        "the forced choice offered {retire:?}. Least recently used comes first, and the order IS \
         the content of that list — the note that was never used at all stands ahead of the one \
         this run used minutes ago. Alphabetically these two are the other way round, which is \
         exactly what an unwritten `last_used_at` produces: a list sorted by filename, presented \
         to a person as a list sorted by how long ago the model needed it"
    );

    Ok(())
}

// ── co dubler zobaczył ─────────────────────────────────────────────────────────────────────

/// Prompt jedynego kroku, dokładnie te bajty, które pojechałyby stdinem.
#[derive(Debug, Default)]
struct Seen(Mutex<Option<String>>);

impl Seen {
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym
    /// wywołaniu, więc nie ma wyrażenia, w którym dożyłby do `await`.
    fn record(&self, prompt: String) {
        *self.lock() = Some(prompt);
    }

    fn snapshot(&self) -> Option<String> {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, Option<String>> {
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
        // Krok grafu poznajemy po znaczniku jego instrukcji. Wszystko inne, co ten bieg
        // uruchomi, przechodzi tędy i NIE jest tym, co ten test mierzy.
        if spec.prompt.contains(STEP_MARK) {
            self.seen.record(spec.prompt.clone());
        }

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
            text: "The queue drains in one place.".to_owned(),
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
