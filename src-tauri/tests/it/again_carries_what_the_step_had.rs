//! AC-2 dla T-88: powtórzony kafelek dostaje **to, co dostał za pierwszym razem** — nie mniej
//! i nie więcej.
//!
//! # Dwa czasowniki, dwa różne pytania o stary bieg
//!
//! `rerun::onward` mówi „idź dalej od tego miejsca", więc krok na czele wycinka pyta, co się
//! przed nim wydarzyło, i dostaje wszystko, na czym miał budować (`resume_carries_the_earlier_handoffs`).
//! `rerun::again` mówi co innego: „zrób ten jeden kafelek jeszcze raz". Powtórzenie, które
//! dokłada materiał, którego tamten krok nigdy nie widział, nie jest powtórzeniem — to inny
//! krok pod tą samą nazwą, a człowiek powtarza kafelek właśnie po to, żeby zobaczyć, czy jego
//! poprawka zmieniła wynik przy TYCH SAMYCH wejściach.
//!
//! # SŁABĄ WERSJĄ jest powtórzenie środkowego kafelka
//!
//! W łańcuchu `A → B → C` krok `B` widział pierwotnie `A`, a jego jedynym przodkiem jest też
//! `A` — więc obie odpowiedzi są identyczne i asercja o `B` nie rozróżnia niczego. Rozróżnia
//! dopiero `C`: widział pierwotnie **samo `B`**, a przodków ma dwóch. Implementacja, która
//! na oba czasowniki odpowiada „wszystko, co stoi wyżej w grafie", przechodzi punkt o `B`
//! i przewraca się na punkcie o `C` — i dlatego oba stoją niżej.
//!
//! # I KAFELEK, KTÓRY NIGDY NIE MIAŁ WEJŚCIA
//!
//! `A` nie ma poprzedników, więc po powtórzeniu ma dostać swoją instrukcję i nic poza nią.
//! Pusty nagłówek nad zerem wpisów jest zdaniem o niczym, a agent czyta go jak zgubione wejście.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
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

/// Etykieta vendora dublera. Nie „claude" i nie „codex": tamte dwie mają własne wymagania co do
/// dowodów biegu, a to kryterium sądzi tekst promptu.
const VENDOR: &str = "fake";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony.
const PATIENCE: Duration = Duration::from_mins(2);

/// Zdanie, którym wiersz indeksu mówi, że plik przyszedł z wcześniejszego biegu.
const FROM_THE_RUN_BEFORE: &str = "what an earlier run left here";

/// Nazwa pliku workflow w bibliotece — `rerun::again` wskazuje kafelek właśnie nią.
const LIBRARY_FILE: &str = "chain.json";

/// Początek instrukcji każdego kroku — po nim, i tylko po nim, dubler poznaje, kto pyta.
const ASKED: [(&str, &str); 3] = [
    ("research:", "Research"),
    ("draft:", "Draft"),
    ("assemble:", "Assemble"),
];

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000e2
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

/// `research → draft → assemble`. Wszystkie trzy kroki kończą się dobrze: powtarza się kafelek
/// biegu, który przeszedł, bo poprawka poszła w agenta albo w instrukcję.
const CHAIN: &str = r#"{
  "format": 1,
  "id": "wf_again_carries_what_the_step_had",
  "name": "Research, draft, assemble",
  "steps": [
    {
      "kind": "agent",
      "id": "s_a",
      "name": "Research",
      "agent": "01990000-0000-7000-8000-0000000000e2",
      "overrides": {},
      "instructions": "research: find out how it works.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 24 }
    },
    {
      "kind": "agent",
      "id": "s_b",
      "name": "Draft",
      "agent": "01990000-0000-7000-8000-0000000000e2",
      "overrides": {},
      "instructions": "draft: write the first pass.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 168 }
    },
    {
      "kind": "agent",
      "id": "s_c",
      "name": "Assemble",
      "agent": "01990000-0000-7000-8000-0000000000e2",
      "overrides": {},
      "instructions": "assemble: put the whole thing together.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 312 }
    }
  ],
  "links": [
    { "from": "s_a", "to": "s_b" },
    { "from": "s_b", "to": "s_c" }
  ]
}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_tile_run_again_is_handed_exactly_what_it_had() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow(LIBRARY_FILE, CHAIN)?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::default());
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch)),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };

    // ── PIERWSZY BIEG, CAŁY ───────────────────────────────────────────────────────────────
    let left = the_whole_chain(&deps, &watch, workflow).await?;

    /* WSZYSTKIE TRZY ŻĄDANIA POWSTAJĄ TERAZ, PRZED PIERWSZYM POWTÓRZENIEM. `rerun::again`
     * szuka NAJNOWSZEGO biegu tego workflow, więc żądanie zbudowane po pierwszym powtórzeniu
     * wskazywałoby na to powtórzenie, a nie na bieg, którego dotyczy to kryterium. */
    let repeat = |tile: &str| {
        rerun::again(
            bench.home.path(),
            bench.project.path(),
            LIBRARY_FILE,
            tile,
            1,
        )
    };
    let (repeat_c, repeat_b, repeat_a) = (repeat("s_c")?, repeat("s_b")?, repeat("s_a")?);

    // ── ŚRODKOWY KAFELEK: DOKŁADNIE TO, CO WIDZIAŁ ────────────────────────────────────────
    let (run_b, asked_b) = one_tile(&deps, &watch, &repeat_b.request, "Draft").await?;
    assert_eq!(
        index_rows(&asked_b.prompt),
        vec![row("Research", &run_b.dir, &left["Research"])],
        "the repeated tile was handed nothing at all. `Part::Just` strips every arrow — the \
         repeated tile has nothing to walk after — so today its prompt has no index, and the \
         work the step before it did sits copied into this run's folder with nobody naming it. \
         The agent redoes from an empty page what the graph already did. The prompt was: {:?}",
        asked_b.prompt
    );
    assert!(
        asked_b.extra_dirs.contains(&run_b.dir.join("handoffs")),
        "the step was given a path it is not allowed to open, which is a reference and no way to \
         follow it. Got: {:?}",
        asked_b.extra_dirs
    );

    // ── OSTATNI KAFELEK: SAM `Draft`, NIGDY `Research` ────────────────────────────────────
    let (run_c, asked_c) = one_tile(&deps, &watch, &repeat_c.request, "Assemble").await?;
    assert_eq!(
        index_rows(&asked_c.prompt),
        vec![row("Draft", &run_c.dir, &left["Draft"])],
        "the repeated tile was handed a different set than it had the first time. Running one \
         tile again is how a person asks `did my fix change the answer` — and the answer means \
         nothing if the inputs moved at the same time. Handing it everything upstream is the \
         easy way to pass the point above and it fails right here: this tile never saw the \
         first step's file, and now it does. The prompt was: {:?}",
        asked_c.prompt
    );

    // ── I KAFELEK, KTÓRY NIGDY NIE MIAŁ WEJŚCIA ───────────────────────────────────────────
    let (_, asked_a) = one_tile(&deps, &watch, &repeat_a.request, "Research").await?;
    assert!(
        index_rows(&asked_a.prompt).is_empty() && !asked_a.prompt.contains("/handoffs/"),
        "the first tile of the chain was handed files it never had. It has no step before it, so \
         a list of what earlier steps left is a list of somebody else's work — and the whole \
         folder of copied files is sitting right there, one loose condition away from being \
         pasted into every prompt of the run. The prompt was: {:?}",
        asked_a.prompt
    );
    Ok(())
}

/// Cały łańcuch od zera — bieg, którego kafelki się potem powtarza — i kto co po nim zostawił.
///
/// Osobno od kryterium, bo to jest ŁAWKA: same asercje o tym, że fikstura jest fiksturą.
async fn the_whole_chain(
    deps: &RunDeps<'_>,
    watch: &Watch,
    workflow: PathBuf,
) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let first = one_run(
        deps,
        &RunRequest {
            workflow,
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        },
    )
    .await?;
    assert_eq!(
        (who(&watch.take()), first.steps.clone()),
        (
            vec![
                "Research".to_owned(),
                "Draft".to_owned(),
                "Assemble".to_owned()
            ],
            vec![
                StepState::Succeeded,
                StepState::Succeeded,
                StepState::Succeeded
            ]
        ),
        "the bench is only a bench if the whole chain ran and finished; every point below is \
         about repeating one of its tiles"
    );
    let left = who_left_what(&first.dir)?;
    assert_eq!(
        left.keys().cloned().collect::<Vec<_>>(),
        vec![
            "Assemble".to_owned(),
            "Draft".to_owned(),
            "Research".to_owned()
        ],
        "all three steps have to leave a file behind, or the sets below are short for a reason \
         that has nothing to do with this criterion. Found: {left:?}"
    );
    Ok(left)
}

/// Puszcza powtórzenie i oddaje to, o co poproszono jedyny krok, który miał pobiec.
///
/// Asercja o tym, KTO pobiegł, stoi tutaj, bo bez niej każde porównanie indeksu niżej mogłoby
/// dotyczyć promptu cudzego kroku.
async fn one_tile(
    deps: &RunDeps<'_>,
    watch: &Watch,
    request: &RunRequest,
    should_run: &str,
) -> Result<(RunReport, Asked), Box<dyn Error>> {
    let report = one_run(deps, request).await?;
    let asked = watch.take();
    assert_eq!(
        (who(&asked), report.steps.clone()),
        (vec![should_run.to_owned()], vec![StepState::Succeeded]),
        "repeating one tile has to run that tile alone"
    );
    let one = asked
        .into_iter()
        .next()
        .ok_or("no step was asked to run at all")?;
    Ok((report, one))
}

// ── czytanie promptu ───────────────────────────────────────────────────────────────────────

/// Wiersze indeksu z promptu: `(kto zostawił, ścieżka, czym to jest)`, w kolejności wystąpień.
fn index_rows(prompt: &str) -> Vec<(String, String, String)> {
    prompt
        .lines()
        .filter_map(|line| line.strip_prefix("- "))
        .filter(|line| line.contains("/handoffs/"))
        .filter_map(|line| {
            let (from, rest) = line.split_once(": ")?;
            let (path, what) = rest.rsplit_once(" (")?;
            Some((
                from.to_owned(),
                path.to_owned(),
                what.trim_end_matches(')').to_owned(),
            ))
        })
        .collect()
}

/// Wiersz, którego się spodziewamy: ten plik, w katalogu TEGO biegu, z etykietą wcześniejszego.
fn row(from: &str, run_dir: &Path, file: &str) -> (String, String, String) {
    (
        from.to_owned(),
        run_dir.join("handoffs").join(file).display().to_string(),
        FROM_THE_RUN_BEFORE.to_owned(),
    )
}

/// Nazwa kroku → nazwa pliku, który ten krok zostawił w tym biegu. Z front-mattera, nie z nazwy.
fn who_left_what(run_dir: &Path) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let mut out = BTreeMap::new();
    let Ok(entries) = fs::read_dir(run_dir.join("handoffs")) else {
        return Ok(out);
    };
    for entry in entries.flatten() {
        if !entry.file_type()?.is_file() {
            continue;
        }
        let text = fs::read_to_string(entry.path())?;
        if let Some(from) = text
            .lines()
            .find_map(|line| line.strip_prefix("from: "))
            .map(|value| value.trim().trim_matches('"').to_owned())
        {
            out.insert(from, entry.file_name().to_string_lossy().into_owned());
        }
    }
    Ok(out)
}

fn who(asked: &[Asked]) -> Vec<String> {
    asked.iter().map(|one| one.who.clone()).collect()
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

/// Kto został o co poproszony, co dostał na wejściu i co wolno mu było otworzyć.
#[derive(Debug, Clone)]
struct Asked {
    who: String,
    prompt: String,
    extra_dirs: Vec<PathBuf>,
}

#[derive(Debug, Default)]
struct Watch(Mutex<Vec<Asked>>);

impl Watch {
    fn entered(&self, spec: &RunSpec) -> String {
        let mut seen = self.lock();
        let who = who_is_asked(&spec.prompt);
        seen.push(Asked {
            who: who.clone(),
            prompt: spec.prompt.clone(),
            extra_dirs: spec.extra_dirs.clone(),
        });
        format!("## Answer\n{who} is done.\n\n## Evidence\nnotes.txt:1\n\n## Open\nnothing.\n")
    }

    /// Zdejmuje i oddaje wszystko, co widziała: cztery biegi w jednym teście mają być czytane
    /// osobno, a nie jednym ciągiem.
    fn take(&self) -> Vec<Asked> {
        std::mem::take(&mut *self.lock())
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Asked>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Nazwa kafelka po początku jego instrukcji.
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

    fn workflow(&self, file_name: &str, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.home.path().join("workflows").join(file_name);
        fs::write(&path, text)?;
        Ok(path)
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }
}
