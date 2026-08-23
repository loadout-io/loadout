//! AC-3 dla T-87: pętla, która przeszła, oddaje krokowi za sobą to, czym przeszła.
//!
//! # Wada, którą to kryterium opisuje
//!
//! Rundy po tej, w której padł werdykt `pass`, są pomijane bez sterownika (`already_settled`)
//! i nie oddają przekazania — a strzałka z pętli na zewnątrz wychodzi z rundy OSTATNIEJ
//! (`workflow::unroll`). Krok za pętlą wisi więc na węźle, który z definicji nic nie napisał.
//!
//! Zmierzone w biegu `20260823-145648`: krok syntezy z TRZEMA strzałkami wchodzącymi dostał
//! dwa pliki — obie krytyki negatywne — i **zero** z gałęzi, które przeszły. Produkt tego biegu
//! (Design, Implementation) powstał na syntezie, która widziała same odmowy. To nie jest brak
//! wygody: to jest bieg, za który właściciel zapłacił i który zbudował nie to, co miał.
//!
//! # Dlaczego trzy gałęzie, a nie jedna
//!
//! Bo tylko przy trzech widać różnicę między „ostatnia runda" a „ostatnia WYPRODUKOWANA runda".
//! Jedna gałąź przechodzi w rundzie pierwszej, druga w drugiej, trzecia nie przechodzi wcale
//! i jedzie dalej mimo to. Trzy różne odpowiedzi na jedno pytanie, w jednym biegu — a każda
//! implementacja, która bierze „rundę ostatnią" albo „rundę pierwszą", myli się na co najmniej
//! dwóch z nich.
//!
//! # SŁABĄ WERSJĄ jest policzyć pozycje indeksu
//!
//! Sześć pozycji daje też implementacja, która wpisuje syntezie WSZYSTKIE rundy każdej gałęzi —
//! czyli oddaje jej pracę odrzuconą razem z przyjętą i nie mówi, która jest która. Dlatego niżej
//! stoi porównanie całej listy, po imieniu i numerze próby, a nie jej długości.
//!
//! # Kontrola: dzisiejsze zachowanie ma być czerwone
//!
//! Dziś ta lista ma jedną pozycję — z gałęzi, która NIE przeszła, bo tylko ona naprawdę
//! wykonała rundę ostatnią.

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
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

const VENDOR: &str = "fake";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony. Trzy gałęzie po trzy rundy to najdłuższa
/// ławka w tym zadaniu.
const PATIENCE: Duration = Duration::from_mins(3);

/// Ile razy każda z trzech pętli wolno próbuje.
const TRIES: usize = 3;

/// Początek instrukcji każdego kroku — po nim, i tylko po nim, dubler poznaje, kto pyta.
const ASKED: [(&str, &str); 8] = [
    ("plan:", "Plan"),
    ("alpha-work:", "Alpha"),
    ("alpha-test:", "Alpha check"),
    ("bravo-work:", "Bravo"),
    ("bravo-test:", "Bravo check"),
    ("charlie-work:", "Charlie"),
    ("charlie-test:", "Charlie check"),
    ("join:", "Join"),
];

/// Nazwy kafelków. DŁUŻSZE PRZED KRÓTSZYMI nie jest tu potrzebne — „Alpha try 2" nie jest
/// fragmentem „Alpha check try 2" — ale podpisy i tak szukamy w całości, nie po przedrostku.
const NAMES: [&str; 8] = [
    "Plan",
    "Alpha check",
    "Alpha",
    "Bravo check",
    "Bravo",
    "Charlie check",
    "Charlie",
    "Join",
];

/// Krok, przed którym stoją trzy pętle. To jego indeks jest całym tym kryterium.
const JOIN: &str = "Join";

/// W której próbie sędzia danej gałęzi mówi „przeszło". Większe niż [`TRIES`] znaczy „nigdy".
const PASSES_ON: [(&str, usize); 3] = [
    ("Alpha check", 1),
    ("Bravo check", 2),
    ("Charlie check", usize::MAX),
];

const HAND_FILE: &str = "---
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
skills: []
connections: []
---
Do the work.
";

/// Jeden plan, trzy gałęzie z własnym sprawdzeniem i własnym powrotem, jedna synteza.
///
/// Ciała pętli są ROZŁĄCZNE, więc `workflow::check::loops_that_cross` je przepuszcza, a każda
/// gałąź liczy swoje rundy osobno. Trzecia gałąź ma zapisane `carry-on`, bo to ona nie przejdzie
/// i to ona ma mimo wszystko dojechać do syntezy.
const THREE_BRANCHES: &str = r#"{
  "format": 1,
  "id": "wf_passed_loop_reaches_on",
  "name": "Three branches, one synthesis",
  "steps": [
    {
      "kind": "agent",
      "id": "s_plan",
      "name": "Plan",
      "agent": "01990000-0000-7000-8000-0000000000d3",
      "overrides": {},
      "instructions": "plan: say what to build.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 264, "y": 24 }
    },
    {
      "kind": "agent",
      "id": "s_alpha_work",
      "name": "Alpha",
      "agent": "01990000-0000-7000-8000-0000000000d3",
      "overrides": {},
      "instructions": "alpha-work: do the first part.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 168 }
    },
    {
      "kind": "agent",
      "id": "s_alpha_test",
      "name": "Alpha check",
      "agent": "01990000-0000-7000-8000-0000000000d3",
      "overrides": {},
      "instructions": "alpha-test: say whether the first part is good enough.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 312 }
    },
    {
      "kind": "agent",
      "id": "s_bravo_work",
      "name": "Bravo",
      "agent": "01990000-0000-7000-8000-0000000000d3",
      "overrides": {},
      "instructions": "bravo-work: do the second part.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 264, "y": 168 }
    },
    {
      "kind": "agent",
      "id": "s_bravo_test",
      "name": "Bravo check",
      "agent": "01990000-0000-7000-8000-0000000000d3",
      "overrides": {},
      "instructions": "bravo-test: say whether the second part is good enough.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 264, "y": 312 }
    },
    {
      "kind": "agent",
      "id": "s_charlie_work",
      "name": "Charlie",
      "agent": "01990000-0000-7000-8000-0000000000d3",
      "overrides": {},
      "instructions": "charlie-work: do the third part.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 504, "y": 168 }
    },
    {
      "kind": "agent",
      "id": "s_charlie_test",
      "name": "Charlie check",
      "agent": "01990000-0000-7000-8000-0000000000d3",
      "overrides": {},
      "whenItFails": "carry-on",
      "instructions": "charlie-test: say whether the third part is good enough.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 504, "y": 312 }
    },
    {
      "kind": "agent",
      "id": "s_join",
      "name": "Join",
      "agent": "01990000-0000-7000-8000-0000000000d3",
      "overrides": {},
      "instructions": "join: put the three parts together.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 264, "y": 456 }
    }
  ],
  "links": [
    { "from": "s_plan", "to": "s_alpha_work" },
    { "from": "s_plan", "to": "s_bravo_work" },
    { "from": "s_plan", "to": "s_charlie_work" },
    { "from": "s_alpha_work", "to": "s_alpha_test" },
    { "from": "s_bravo_work", "to": "s_bravo_test" },
    { "from": "s_charlie_work", "to": "s_charlie_test" },
    { "from": "s_alpha_test", "to": "s_join" },
    { "from": "s_bravo_test", "to": "s_join" },
    { "from": "s_charlie_test", "to": "s_join" },
    { "from": "s_alpha_test", "to": "s_alpha_work", "max_turns": 3 },
    { "from": "s_bravo_test", "to": "s_bravo_work", "max_turns": 3 },
    { "from": "s_charlie_test", "to": "s_charlie_work", "max_turns": 3 }
  ]
}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_step_after_three_loops_is_given_what_each_of_them_ended_on()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("branches", THREE_BRANCHES)?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::default());

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch)),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 3,
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

    let seen = watch.seen();

    // Ławka jest ławką tylko wtedy, gdy trzy gałęzie skończyły się na trzy różne sposoby.
    for (branch, tries) in [("Alpha check", 1), ("Bravo check", 2), ("Charlie check", 3)] {
        assert_eq!(
            prompts_of(branch, &seen).len(),
            tries,
            "the fixture is wrong: {branch:?} was supposed to run {tries} time(s), so that one \
             branch ends on its first round, one on its second and one never passes at all. The \
             run ended as {:?} and the driver was asked by: {:?}",
            report.steps,
            watch.who()
        );
    }

    let joined = prompts_of(JOIN, &seen);
    assert_eq!(
        joined.len(),
        1,
        "the fixture is wrong if the synthesis did not run exactly once; it is the only thing \
         this criterion measures. The driver was asked by: {:?}",
        watch.who()
    );

    let wrote = who_wrote_what(&report.dir)?;
    assert_eq!(
        named(&files_listed(&joined[0]), &wrote),
        vec![
            "Alpha try 1",
            "Alpha check try 1",
            "Bravo try 2",
            "Bravo check try 2",
            "Charlie try 3",
            "Charlie check try 3"
        ],
        "the synthesis was not given the work and the verdict each branch really ended on. \
         Today it gets one file — from the only branch that did NOT pass, because that is the \
         only one whose last round actually ran; the two branches that succeeded reach it as \
         silence. One owner's run built its Design and its Implementation on exactly that: two \
         negative reviews and nothing from the work that was accepted. The prompt was: {:?}",
        joined[0]
    );
    Ok(())
}

// ── co dojechało do sterownika ─────────────────────────────────────────────────────────────

/// Prompty tego kafelka, w kolejności startów.
fn prompts_of(name: &str, seen: &[(String, String)]) -> Vec<String> {
    seen.iter()
        .filter(|(who, _)| who == name)
        .map(|(_, prompt)| prompt.clone())
        .collect()
}

/// Nazwy plików przekazań wymienionych w tym prompcie, w kolejności wystąpień.
fn files_listed(prompt: &str) -> Vec<String> {
    prompt
        .match_indices("handoffs/")
        .map(|(at, marker)| {
            let rest = &prompt[at + marker.len()..];
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            rest[..end]
                .trim_end_matches([',', ';', ':', ')'])
                .to_owned()
        })
        .collect()
}

/// Te same pliki, nazwane tym, kto je napisał i w której próbie.
fn named(files: &[String], wrote: &BTreeMap<String, String>) -> Vec<String> {
    files
        .iter()
        .map(|file| {
            wrote
                .get(file)
                .cloned()
                .unwrap_or_else(|| format!("nobody we know wrote {file}"))
        })
        .collect()
}

/// Plik przekazania → podpis tego, kto go zostawił („Bravo check try 2").
fn who_wrote_what(dir: &Path) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let mut out = BTreeMap::new();
    let Ok(entries) = fs::read_dir(dir.join("handoffs")) else {
        return Ok(out);
    };
    for entry in entries.flatten() {
        if !entry.file_type()?.is_file() {
            continue;
        }
        let text = fs::read_to_string(entry.path())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(mark) = signature_in(&text) {
            out.insert(name, mark);
        }
    }
    Ok(out)
}

fn signature_in(text: &str) -> Option<String> {
    for name in NAMES {
        for try_number in 1..=TRIES {
            let mark = format!("{name} try {try_number}");
            if text.contains(&mark) {
                return Some(mark);
            }
        }
    }
    None
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct Watch(Mutex<Vec<(String, String)>>);

impl Watch {
    /// Zapisuje start i oddaje tekst, którym ta tura się skończy. Werdykt liczony z LICZBY
    /// startów TEGO sędziego: instrukcja jest w każdej rundzie identyczna, dokładnie jak
    /// u prawdziwego agenta w nowej sesji.
    fn entered(&self, prompt: &str) -> String {
        let mut seen = self.lock();
        let who = who_is_asked(prompt);
        seen.push((who.clone(), prompt.to_owned()));
        let try_number = seen.iter().filter(|(name, _)| *name == who).count();
        let body = format!(
            "## Answer\n{who} try {try_number} is done.\n\n## Evidence\nnotes.txt:1\n\n## Open\n\
             nothing.\n"
        );
        let Some((_, passes_on)) = PASSES_ON.iter().find(|(name, _)| *name == who) else {
            return body;
        };
        if try_number >= *passes_on {
            format!("{body}\noutcome: pass\n")
        } else {
            format!("{body}\noutcome: fail\n")
        }
    }

    fn seen(&self) -> Vec<(String, String)> {
        self.lock().clone()
    }

    fn who(&self) -> Vec<String> {
        self.lock().iter().map(|(who, _)| who.clone()).collect()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<(String, String)>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
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
        let said = self.watch.entered(&spec.prompt);
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
