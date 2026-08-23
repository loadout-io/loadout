//! AC-3 dla T-88: kiedy stary bieg musiał odłożyć pełny tekst obok, wznowienie zabiera go
//! razem z przekazaniem.
//!
//! # Co robi cięcie i czego po nim brakowało
//!
//! `memory::handoff` tnie ciało na `BODY_CAP`, pisze ORYGINAŁ do `attachments/` i wstawia
//! w ciało wiersz `Moved to attachments/<nazwa>__full.md`. Ten wiersz składa Loadout, nie agent,
//! i jest liczony od katalogu biegu. Wznowienie kopiowało do nowego katalogu samo `handoffs/`,
//! więc krok dostawał **odnośnik prowadzący donikąd** — a to jest dokładnie ta wada, którą
//! zmierzono na biegu `20260819-223942`: krok dostał trzy takie wskaźniki, nie otworzył żadnego,
//! napisał, że pełnego tekstu „nie ma", i wyliczył cały dowód drugi raz wprost z repozytorium,
//! paląc 9 z 10 minut swojego limitu na pracę leżącą gotową obok.
//!
//! # SŁABĄ WERSJĄ jest sprawdzenie, że katalog `attachments/` istnieje
//!
//! Przechodzi ją kopia, która przeniosła pliki pod inną nazwą albo do innego katalogu — bo
//! pytanie brzmi nie „czy coś skopiowano", tylko „czy WSKAŹNIK Z CIAŁA rozwiązuje się w nowym
//! katalogu". Dlatego niżej ścieżka jest czytana z ciała skopiowanego przekazania i sprawdzana
//! na dysku, a nie składana w teście po raz drugi.
//!
//! # I DLATEGO BRAK `attachments/` MA BYĆ ZWYKŁYM DNIEM
//!
//! Katalog powstaje wyłącznie wtedy, gdy jakieś przekazanie zostało ucięte — czyli w większości
//! biegów nie powstaje wcale. Kopiowanie, które przewraca się na jego braku, zamieniłoby
//! najczęstszy bieg w odmowę.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
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
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony.
const PATIENCE: Duration = Duration::from_mins(2);

/// Katalog, w którym leżą pełne teksty odłożone obok uciętych przekazań.
///
/// SIOSTRA `handoffs/`, nie podkatalog — wskaźnik w ciele jest liczony od katalogu biegu.
const ATTACHMENTS: &str = "attachments";

/// Początek wiersza, którym Loadout mówi, gdzie leży pełny tekst.
const POINTER: &str = "Moved to ";

/// Początek instrukcji każdego kroku — po nim dubler poznaje, kto pyta.
const ASKED: [(&str, &str); 2] = [("research:", "Research"), ("draft:", "Draft")];

/// Ile znaków ma odpowiedź, która NA PEWNO nie mieści się w limicie ciała przekazania.
/// `memory::handoff::BODY_CAP` wynosi 8192; to jest ponad dwa razy tyle.
const TOO_LONG: usize = 20_000;

const HAND_FILE: &str = "---
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

/// `research → draft`. Dwa kroki wystarczą: cięcie dotyka pierwszego, a wznawiamy drugi.
const PAIR: &str = r#"{
  "format": 1,
  "id": "wf_resume_carries_the_attachments",
  "name": "Research, then draft",
  "steps": [
    {
      "kind": "agent",
      "id": "s_a",
      "name": "Research",
      "agent": "01990000-0000-7000-8000-0000000000e3",
      "overrides": {},
      "instructions": "research: find out how it works.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 24 }
    },
    {
      "kind": "agent",
      "id": "s_b",
      "name": "Draft",
      "agent": "01990000-0000-7000-8000-0000000000e3",
      "overrides": {},
      "instructions": "draft: write the first pass.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 168 }
    }
  ],
  "links": [{ "from": "s_a", "to": "s_b" }]
}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_full_text_travels_with_the_file_that_points_at_it() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("pair", PAIR)?;
    let store = Store::open(&bench.db())?;
    // Pierwszy krok mówi więcej, niż mieści się w przekazaniu — więc pełny tekst idzie obok.
    let watch = Arc::new(Watch::new(TOO_LONG));
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch)),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };

    let first = one_run(&deps, &plain(workflow)).await?;
    watch.take();
    assert_eq!(
        first.steps,
        vec![StepState::Succeeded, StepState::Succeeded],
        "the bench is only a bench if both steps of the first run finished"
    );
    let put_aside = files_in(&first.dir.join(ATTACHMENTS));
    assert_eq!(
        put_aside.len(),
        1,
        "the fixture is wrong unless the first step said more than fits in one file and Loadout \
         put the full text beside it. Found: {put_aside:?}"
    );

    // ── WZNOWIENIE OD DRUGIEGO KROKU ──────────────────────────────────────────────────────
    let second = pick_up_from(&deps, &bench, &first, "s_b").await?;
    let asked = watch.take();
    assert_eq!(
        (asked.len(), second.steps.clone()),
        (1, vec![StepState::Succeeded]),
        "picking up at the second step has to run that step alone"
    );

    // ── WSKAŹNIK Z CIAŁA ROZWIĄZUJE SIĘ W NOWYM KATALOGU ──────────────────────────────────
    let carried = the_one_file_listed(&asked[0].prompt)?;
    let points_at = pointed_at(&carried)?;
    let full = second.dir.join(&points_at);
    assert!(
        full.is_file(),
        "the file this run handed the step points at a full text that is not in this run's \
         folder. Loadout wrote that line itself, so the step is handed a reference by us and no \
         way to follow it — and the step does what the owner's run did: it says the full text is \
         missing and works the whole thing out a second time, from scratch, next to the answer. \
         The line said {points_at:?} and the folder holds: {:?}",
        files_in(&second.dir.join(ATTACHMENTS))
    );

    // ── I PRAWO OTWARCIA TEGO KATALOGU ────────────────────────────────────────────────────
    assert!(
        asked[0].extra_dirs.contains(&second.dir.join(ATTACHMENTS)),
        "the full text is in this run's folder and the step may not open it, which reads to the \
         agent exactly like a file that is not there. Got: {:?}",
        asked[0].extra_dirs
    );
    Ok(())
}

/// Bieg, w którym nic nie było za długie, nie ma katalogu z pełnymi tekstami — i to jest zwykły
/// dzień, nie brak.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_run_that_put_nothing_aside_is_picked_up_all_the_same() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("pair", PAIR)?;
    let store = Store::open(&bench.db())?;
    // Krótka odpowiedź: mieści się w przekazaniu, więc nic nie ląduje obok.
    let watch = Arc::new(Watch::new(0));
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch)),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };

    let first = one_run(&deps, &plain(workflow)).await?;
    watch.take();
    assert!(
        !first.dir.join(ATTACHMENTS).exists(),
        "the fixture is wrong if a short answer put anything aside — this run is the one that \
         has nothing to carry"
    );

    let second = pick_up_from(&deps, &bench, &first, "s_b").await?;
    let asked = watch.take();
    assert_eq!(
        (asked.len(), second.steps.clone()),
        (1, vec![StepState::Succeeded]),
        "a run with nothing put aside still has to be picked up. Refusing over a folder that \
         only exists when somebody wrote too much would turn the ordinary run into an error"
    );
    assert!(
        the_one_file_listed(&asked[0].prompt).is_ok(),
        "the picked-up step still has to be handed what the run before it left. The prompt was: \
         {:?}",
        asked[0].prompt
    );
    Ok(())
}

// ── czytanie promptu i plików ──────────────────────────────────────────────────────────────

fn plain(workflow: PathBuf) -> RunRequest {
    RunRequest {
        workflow,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    }
}

/// Wznawia wskazany bieg od wskazanego kroku — tą samą drogą, którą idzie ekran historii.
async fn pick_up_from(
    deps: &RunDeps<'_>,
    bench: &Bench,
    first: &RunReport,
    step: &str,
) -> Result<RunReport, Box<dyn Error>> {
    let folder = first
        .dir
        .file_name()
        .and_then(|one| one.to_str())
        .ok_or("the run directory has no name")?;
    let again = rerun::onward(bench.home.path(), bench.project.path(), folder, step, 1)?;
    one_run(deps, &again.request).await
}

/// Jedyny plik przekazania wymieniony w tym prompcie. Błąd, kiedy nie ma dokładnie jednego:
/// dalsze pytania dotyczą jego ciała, więc nie ma sensu zgadywać, o który chodzi.
fn the_one_file_listed(prompt: &str) -> Result<PathBuf, Box<dyn Error>> {
    let listed: Vec<PathBuf> = prompt
        .lines()
        .filter_map(|line| line.strip_prefix("- "))
        .filter_map(|line| line.split_once(": "))
        .map(|(_, rest)| rest)
        .filter(|rest| rest.contains("/handoffs/"))
        .filter_map(|rest| rest.rsplit_once(" ("))
        .map(|(path, _)| PathBuf::from(path))
        .collect();
    match listed.as_slice() {
        [one] => Ok(one.clone()),
        other => Err(format!(
            "the picked-up step was handed {} files where its prompt should list exactly the one \
             the step before it left. The prompt was: {prompt:?}",
            other.len()
        )
        .into()),
    }
}

/// Ścieżka, na którą wskazuje ciało tego przekazania — dosłownie tak, jak stoi w pliku.
fn pointed_at(handoff: &Path) -> Result<String, Box<dyn Error>> {
    let text = fs::read_to_string(handoff)?;
    text.lines()
        .find_map(|line| line.trim().strip_prefix(POINTER))
        .map(str::to_owned)
        .ok_or_else(|| {
            format!(
                "{} was cut short in the run before this one, so its body has to say where the \
                 full text went — and this copy says nothing of the kind",
                handoff.display()
            )
            .into()
        })
}

/// Nazwy plików w tym katalogu, posortowane. Pusto, kiedy katalogu nie ma.
fn files_in(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|one| one.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

// ── bieg ───────────────────────────────────────────────────────────────────────────────────

async fn one_run(deps: &RunDeps<'_>, request: &RunRequest) -> Result<RunReport, Box<dyn Error>> {
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(deps, request, sink))
        .await
        .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))??;
    let _ = tokio::time::timeout(PATIENCE, pump).await;
    Ok(report)
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Asked {
    prompt: String,
    extra_dirs: Vec<PathBuf>,
}

#[derive(Debug, Default)]
struct Watch {
    /// Ile znaków dokłada pierwszy krok do swojej odpowiedzi. `0` znaczy „mieści się".
    padding: usize,
    seen: Mutex<Vec<Asked>>,
}

impl Watch {
    fn new(padding: usize) -> Self {
        Self {
            padding,
            seen: Mutex::new(Vec::new()),
        }
    }

    fn entered(&self, spec: &RunSpec) -> String {
        let who = who_is_asked(&spec.prompt);
        self.lock().push(Asked {
            prompt: spec.prompt.clone(),
            extra_dirs: spec.extra_dirs.clone(),
        });
        let mut answer = format!("{who} is done.\n");
        if who == "Research" {
            // Jedno zdanie powtórzone tyle razy, żeby nie zmieściło się w pliku przekazania.
            answer.push_str(
                &"The measurement is written out in full here.\n".repeat(
                    self.padding
                        .div_ceil("The measurement is written out in full here.\n".len()),
                ),
            );
        }
        format!("## Answer\n{answer}\n## Evidence\nnotes.txt:1\n\n## Open\nnothing.\n")
    }

    fn take(&self) -> Vec<Asked> {
        std::mem::take(&mut *self.lock())
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Asked>> {
        self.seen.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn who_is_asked(prompt: &str) -> String {
    ASKED
        .iter()
        .find(|(opening, _)| prompt.starts_with(opening))
        .map_or_else(|| prompt.to_owned(), |(_, name)| (*name).to_owned())
}

fn fake_drivers(watch: Arc<Watch>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { watch });
    Arc::new(move |_vendor| Arc::clone(&driver))
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
        let said = self.watch.entered(&spec);
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
            said,
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
        fs::create_dir_all(project.path().join(".loadout"))?;
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
