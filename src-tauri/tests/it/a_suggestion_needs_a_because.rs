//! AC-2 dla T-92: kandydatka bez powodu nie powstaje, a powtórzenie nie tworzy drugiej.
//!
//! Dwie reguły, jedno źródło. „No because, no memory" [T6 §10.3] jest w `record_candidate`
//! od T-17 i broni **zapisu**; to kryterium pyta o coś innego — czy pisarz, który dopiero co
//! powstał (AC-1), w ogóle podaje mu parę do odrzucenia, czy sam sobie dopisuje uzasadnienie,
//! żeby zapis przeszedł. Powód, dla którego to jest tania i skuteczna droga do nieprawdy:
//! `Error::NoBecause` widzi wyłącznie wołający, a wołającym jest teraz pętla, która ma trzy
//! pary i chce zapisać trzy notatki.
//!
//! Uzasadnienie nie jest ozdobą. arXiv 2608.11095: uzasadnienie instrukcji rozpada się szybciej
//! niż sama instrukcja, a instrukcja **bez** uzasadnienia jest nieusuwalna — skasowanie kosztuje
//! `O(2^|D|)`, bo trzeba od nowa wyprowadzić jej interakcje z każdą inną [T6 §5.1]. Notatka bez
//! `because` zostaje więc w pamięci na zawsze, niezależnie od tego, czy jest prawdziwa.
//!
//! Druga połowa dotyczy powtórzenia. [T6 §5.3] proponuje auto-promocję przy drugim wystąpieniu
//! i **ARCHITECTURE §2 pyt. 5 to unieważnia**: powtórzenie podbija `occurrences` i nic poza tym.
//! To zostaje i ma zostać — dwa biegi, które niezależnie powiedziały to samo, są mocnym sygnałem
//! dla człowieka, ale nie ma podstaw sądzić, że agenci Loadouta będą lepszymi kuratorami niż
//! ludzie, którzy te pliki utrzymywali [T6 §5.3].
//!
//! # Trzy słabe wersje tego kryterium
//!
//! **Pierwsza: para z pustym `because:` i nic więcej.** Przechodzi na implementacji, która
//! wymaga samego KLUCZA, a nie wartości. Dlatego brakujące uzasadnienie występuje tu w obu
//! kształtach, w jakich naprawdę przychodzi od modelu: wiersz `because:` bez treści i para,
//! po której tego wiersza nie ma wcale.
//!
//! **Druga: policzyć notatki po drugim biegu i nie zajrzeć do środka.** „Jedna notatka" jest
//! prawdą także wtedy, gdy drugi bieg nadpisał pierwszą i zgubił licznik — a `occurrences` jest
//! jedyną rzeczą, po której człowiek pozna, że dwa niezależne biegi powiedziały to samo.
//!
//! **Trzecia: nie sprawdzić statusu po powtórzeniu.** Auto-promocja przy drugim wystąpieniu
//! wygląda w liczniku plików identycznie i różni się wyłącznie tym, że zdanie, na które nikt
//! nie przystał, od tej chwili jedzie do każdego promptu w tym projekcie.
//!
//! # Czego ten plik świadomie NIE mierzy
//!
//! „Policzona w dzienniku". Ślad po odrzuconej parze idzie do `tracing`, a poziom, na którym
//! stoi, nie jest włączony w żadnym biegu tej bramki — asercja o nim byłaby asercją o konfiguracji
//! logowania, nie o zachowaniu (niezmiennik 20). Mierzalna połowa tej reguły jest tutaj: para bez
//! powodu **nie staje się plikiem**, a bieg mimo niej kończy się normalnie.

// `unwrap()`/`expect()` w teście: panika w teście JEST jego wynikiem, a `?` w tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::memory::notes_root;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::memory::notes::{Note, Status, scan_notes};
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

const VENDOR: &str = "fake";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią.
const PATIENCE: Duration = Duration::from_secs(20);

/// Znacznik instrukcji kroku grafu.
const STEP_MARK: &str = "IBEX-STEP-ONE";

/// Reguła z uzasadnieniem — jedyna, która ma zostać plikiem.
const KEEP_RULE: &str = "IBEX-KEEP the queue is drained in exactly one place";
const KEEP_REASON: &str = "IBEX-KEEP-REASON run 7f3a step 2 reproduced it twice";

/// Reguła z wierszem `because:`, po którym nic nie stoi.
const EMPTY_RULE: &str = "IBEX-EMPTY retries hide the flaky test instead of finding it";

/// Reguła, po której wiersza `because:` nie ma wcale.
const MISSING_RULE: &str = "IBEX-MISSING the cache is warm after the first build";

const AGENT_ID: &str = "01990000-0000-7000-8000-0000000000a2";

const AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000a2
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

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_suggestion_needs_a_because",
  "name": "One step that finishes something",
  "steps": [
    {
      "kind": "agent",
      "id": "s_one",
      "name": "Backend",
      "agent": "01990000-0000-7000-8000-0000000000a2",
      "overrides": {},
      "instructions": "IBEX-STEP-ONE look at the queue and say what it is doing.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

/// Odpowiedź modelu z trzema parami, z których **jedna** jest poprawna.
///
/// Kolejność jest treścią: dobra para stoi w środku, więc implementacja, która przerywa pętlę
/// na pierwszej złej parze, gubi ją tak samo jak ta, która nie sprawdza niczego — a te dwie
/// wady różnią się dla człowieka wszystkim.
fn three_pairs_one_good() -> String {
    format!(
        "Here is what I would keep from this run.\n\n\
         rule: {EMPTY_RULE}\n\
         because:\n\n\
         rule: {KEEP_RULE}\n\
         because: {KEEP_REASON}\n\n\
         rule: {MISSING_RULE}\n\n"
    )
}

/// Ten sam bieg mówi to samo drugi raz — co do bajtu, bo ta sama kandydatka to ten sam plik.
fn the_same_thing_again() -> String {
    format!(
        "I noticed the same thing again.\n\n\
         rule: {KEEP_RULE}\n\
         because: {KEEP_REASON}\n\n"
    )
}

fn notes_left(bench: &Bench) -> Vec<Note> {
    scan_notes(&notes_root(bench.home.path())).expect("the notes root has to be readable")
}

/// Jeden bieg jednego kroku, z podaną odpowiedzią na pytanie o naukę.
async fn a_run_saying(bench: &Bench, reflection_says: String) -> Result<RunReport, Box<dyn Error>> {
    let workflow = bench.workflow("suggestion-needs-a-because", WORKFLOW)?;
    let store = Store::open(&bench.db())?;

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(reflection_says),
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
    Ok(report)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pair_without_a_reason_is_dropped_and_the_same_rule_twice_is_one_note()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("backend", AGENT)?;
    assert!(
        AGENT.contains(AGENT_ID) && WORKFLOW.contains(AGENT_ID),
        "the fixture names {AGENT_ID} in only one of the two files that have to agree on it"
    );

    // ── Pierwszy bieg: trzy pary, jedna z uzasadnieniem ───────────────────────────────────
    let first = a_run_saying(&bench, three_pairs_one_good()).await?;
    assert_eq!(
        first.steps,
        vec![StepState::Succeeded; 1],
        "the step has to finish and hand something on, or nothing is ever reflected about and \
         every assertion below is true of a run that never got there. It ended as {:?}",
        first.steps
    );

    let after_first = notes_left(&bench);
    let rules: Vec<&str> = after_first.iter().map(|note| note.rule.as_str()).collect();

    assert_eq!(
        after_first.len(),
        1,
        "three pairs with one reason between them left {} note(s): {rules:?}. Exactly one has a \
         `because:` line with anything after it, and `no because, no memory` [T6 section 10.3] \
         holds for the writer as well as for the store — a reason Loadout invents on the model's \
         behalf is what makes an instruction unremovable rather than justified [T6 section 5.1]",
        after_first.len()
    );
    assert!(
        rules.iter().any(|rule| rule.contains("IBEX-KEEP")),
        "the one pair that DID carry a reason is not on disk: {rules:?}. It stands in the middle \
         of the answer on purpose — an implementation that stops at the first bad pair loses it \
         exactly like one that checks nothing, and those two defects look nothing alike to a \
         person"
    );
    assert!(
        !rules.iter().any(|rule| rule.contains("IBEX-EMPTY")),
        "a pair whose `because:` line was empty became a note anyway: {rules:?}. The key being \
         present is not the reason being present, and a note that cannot say why it is true can \
         never be safely retired"
    );
    assert!(
        !rules.iter().any(|rule| rule.contains("IBEX-MISSING")),
        "a pair with no `because:` line at all became a note anyway: {rules:?}"
    );
    assert_eq!(
        after_first[0].occurrences, 1,
        "a rule proposed once came out with occurrences {}",
        after_first[0].occurrences
    );
    let first_modified = after_first[0].modified.clone();

    // ── Drugi bieg: to samo zdanie jeszcze raz ────────────────────────────────────────────
    let second = a_run_saying(&bench, the_same_thing_again()).await?;
    assert_eq!(second.steps, vec![StepState::Succeeded; 1]);
    assert_ne!(
        second.id, first.id,
        "the fixture ran the same run twice; two separate sightings need two separate runs"
    );

    let after_second = notes_left(&bench);
    let rules: Vec<&str> = after_second.iter().map(|note| note.rule.as_str()).collect();
    assert_eq!(
        after_second.len(),
        1,
        "the same rule seen in two runs left {} notes: {rules:?}. The same candidate is the same \
         file — the filename is a function of the normalised title, so a second sighting has \
         nowhere else to land. Two files here mean the section shows a person one sentence twice \
         and counts it twice against the budget of its scope",
        after_second.len()
    );

    let note = &after_second[0];
    assert_eq!(
        note.occurrences, 2,
        "the note came back with occurrences {} after being proposed in two separate runs. That \
         number is the whole signal a repetition carries: it is what tells a person that two \
         runs independently arrived at the same sentence, and it is a number, not a gate",
        note.occurrences
    );
    assert_eq!(
        note.status,
        Status::Suggested,
        "a rule proposed twice promoted itself to {:?}. Auto-promotion at the second sighting is \
         in [T6 section 5.3] and ARCHITECTURE section 2 q. 5 overrules it: a repetition is a \
         signal FOR the person, never a decision taken on their behalf. From that moment the \
         sentence goes into every prompt in this project and nobody ever agreed to it",
        note.status
    );
    assert_eq!(
        note.because, after_first[0].because,
        "the second sighting rewrote the reason. The file may have passed through a person's \
         hands, and an agent's report has no business overwriting somebody else's editing \
         (invariant 4 — the file is the truth, also when a person wrote it)"
    );
    assert_ne!(
        note.modified, first_modified,
        "the note was not restamped when it was seen again. `modified` is what a person reads to \
         tell what is fresh, and a repetition IS a change to this file"
    );

    Ok(())
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers(reflection_says: String) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { reflection_says });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct Fake {
    reflection_says: String,
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
        // Krok grafu poznajemy po znaczniku jego instrukcji; wszystko inne jest turą, o którą
        // ten krok nie prosił — czyli refleksją.
        let is_step = spec.prompt.contains(STEP_MARK);
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
            says: if is_step {
                "The queue drains in one place.".to_owned()
            } else {
                self.reflection_says.clone()
            },
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    says: String,
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
            text: self.says.clone(),
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
        // Ten sam korzeń, który rozwiązuje `commands::memory::notes_root`. ISTNIEJE i jest PUSTY:
        // „zero notatek" ma znaczyć „nikt nic nie zapisał", a nie „nie ma gdzie zapisywać".
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
