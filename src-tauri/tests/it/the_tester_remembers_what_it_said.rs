//! AC-2 dla T-87: sędzia rundy k+1 ma przed sobą to, co sam zarzucił w rundach wcześniejszych.
//!
//! # Dlaczego to jest osobne kryterium od AC-1
//!
//! AC-1 pyta o krok, DO którego wraca powrót — o tego, kto poprawia. Ten pyta o krok, Z którego
//! powrót wychodzi, i te dwa nie są tym samym miejscem w kodzie: pierwszy jest wejściem pętli,
//! drugi jej sędzią. Implementacja, która dokłada pamięć wyłącznie wejściu, przechodzi AC-1
//! w całości i zostawia sędziego dokładnie tam, gdzie był.
//!
//! A tam, gdzie był, jest źle. Zmierzone w biegu `20260823-145648`: `s_7#1` dostał wyłącznie
//! przekazanie pracy z rundy 1. Sędzia, który w rundzie 1 wypisał trzy zarzuty, w rundzie 2
//! zaczynał od pustej kartki — więc mógł uznać za dobre to, co przed chwilą odrzucił, albo
//! wymyślić czwarty zarzut zamiast sprawdzić, czy trzy poprzednie zniknęły. Pętla bez pamięci
//! sędziego nie jest pętlą, tylko trzema niezależnymi ocenami.
//!
//! # SŁABĄ WERSJĄ jest zapytać o rundę drugą i poprzestać
//!
//! Runda druga ma dokładnie jedno „poprzednio", więc przechodzi ją także implementacja, która
//! niesie WYŁĄCZNIE ostatni werdykt. Rozróżnia je dopiero runda trzecia: ma zobaczyć oba
//! wcześniejsze, nie sam przedostatni. Dlatego niżej jest pytanie o obie rundy naraz.
//!
//! # Kryterium pyta o OBECNOŚĆ ŚCIEŻKI, nie o treść zarzutów
//!
//! Co sędzia napisał, jest jego sprawą; nasza kończy się na tym, że plik z tym tekstem stoi
//! w jego indeksie i wolno mu go otworzyć.

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

/// Ile czekamy na bieg, zanim uznamy go za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią.
const PATIENCE: Duration = Duration::from_mins(2);

/// Ile razy pętla wolno próbuje.
const TRIES: usize = 3;

/// Początek instrukcji każdego kroku — po nim, i tylko po nim, dubler poznaje, kto pyta.
const ASKED: [(&str, &str); 4] = [
    ("plan:", "Plan"),
    ("work:", "Work"),
    ("test:", "Tester"),
    ("ship:", "Ship"),
];

const NAMES: [&str; 4] = ["Plan", "Work", "Tester", "Ship"];

/// Kafelek, z którego wychodzi powrót. To on orzeka i to on ma pamiętać.
const TESTER: &str = "Tester";

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000d2
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

/// `plan → work → tester → ship`, powrót `tester → work` do trzech rund.
const LOOP_FILE: &str = r#"{
  "format": 1,
  "id": "wf_tester_remembers",
  "name": "A tester that remembers",
  "steps": [
    {
      "kind": "agent",
      "id": "s_plan",
      "name": "Plan",
      "agent": "01990000-0000-7000-8000-0000000000d2",
      "overrides": {},
      "instructions": "plan: say what to build.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 24 }
    },
    {
      "kind": "agent",
      "id": "s_work",
      "name": "Work",
      "agent": "01990000-0000-7000-8000-0000000000d2",
      "overrides": {},
      "instructions": "work: make the change.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 168 }
    },
    {
      "kind": "agent",
      "id": "s_test",
      "name": "Tester",
      "agent": "01990000-0000-7000-8000-0000000000d2",
      "overrides": {},
      "instructions": "test: say whether it is good enough to build on.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 312 }
    },
    {
      "kind": "agent",
      "id": "s_ship",
      "name": "Ship",
      "agent": "01990000-0000-7000-8000-0000000000d2",
      "overrides": {},
      "instructions": "ship: put it out.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 456 }
    }
  ],
  "links": [
    { "from": "s_plan", "to": "s_work" },
    { "from": "s_work", "to": "s_test" },
    { "from": "s_test", "to": "s_ship" },
    { "from": "s_test", "to": "s_work", "max_turns": 3 }
  ]
}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_later_try_of_the_tester_is_given_what_it_said_before() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("loop", LOOP_FILE)?;
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
        how_many_at_once: 2,
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
    let judged = prompts_of(TESTER, &seen);
    assert_eq!(
        judged.len(),
        TRIES,
        "the fixture is wrong unless the tester really ran three times; the points below would \
         be about rounds that never happened. The run ended as {:?} and the driver was asked \
         by: {:?}",
        report.steps,
        watch.who()
    );

    let wrote = who_wrote_what(&report.dir)?;

    // ── RUNDA PIERWSZA: NIE MA CZEGO PAMIĘTAĆ, I TAK MA ZOSTAĆ ───────────────────────────────
    // Bez tego punktu implementacja, która wpisuje sędziemu jego własne przekazanie ZAWSZE,
    // dawałaby mu w rundzie pierwszej odnośnik do pliku, którego jeszcze nie ma.
    let first = named(&files_listed(&judged[0]), &wrote);
    assert!(
        !first.iter().any(|one| one.starts_with(TESTER)),
        "the tester was handed something of its own in its FIRST try, when it had said nothing \
         yet. Either the run points it at a file that does not exist, or it points it at \
         somebody else's answer under its own name. It was given: {first:?}"
    );

    // ── RUNDA DRUGA: PRACA, KTÓRĄ OCENIA, I WŁASNY POPRZEDNI WERDYKT ─────────────────────────
    let second = named(&files_listed(&judged[1]), &wrote);
    assert!(
        second.contains(&"Work try 2".to_owned()),
        "the fixture is wrong if the tester's second try is not judging the second try of the \
         work; it was given {second:?}"
    );
    assert!(
        second.contains(&"Tester try 1".to_owned()),
        "the tester's second try does not have in front of it what it said the first time. A \
         tester that wrote down three things to fix and then starts from a blank page can pass \
         the very work it turned down, or invent a fourth complaint instead of checking whether \
         the first three are gone. Three of one owner's loops went nine rounds this way and \
         converged not once. It was given: {second:?}"
    );

    // ── RUNDA TRZECIA: OBA POPRZEDNIE, NIE SAM PRZEDOSTATNI ──────────────────────────────────
    let third = named(&files_listed(&judged[2]), &wrote);
    assert!(
        third.contains(&"Work try 3".to_owned()),
        "the fixture is wrong if the tester's third try is not judging the third try of the \
         work; it was given {third:?}"
    );
    for said_before in ["Tester try 1", "Tester try 2"] {
        assert!(
            third.contains(&said_before.to_owned()),
            "the tester's last try is missing {said_before:?}. Carrying only the round just \
             before it looks identical on the second try and loses the first complaint entirely \
             on the third — which is the round where the loop either converges or is paid for \
             and thrown away. It was given: {third:?}"
        );
    }
    Ok(())
}

// ── co dojechało do sterownika ─────────────────────────────────────────────────────────────

/// Prompty tego kafelka, w kolejności startów. Rundy pętli idą po sobie, nigdy równolegle,
/// więc ta kolejność JEST kolejnością rund.
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

/// Plik przekazania → podpis tego, kto go zostawił („Tester try 2").
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
    fn entered(&self, prompt: &str) -> String {
        let mut seen = self.lock();
        let who = who_is_asked(prompt);
        seen.push((who.clone(), prompt.to_owned()));
        let try_number = seen.iter().filter(|(name, _)| *name == who).count();
        let body = format!(
            "## Answer\n{who} try {try_number} is done.\n\n## Evidence\nnotes.txt:1\n\n## Open\n\
             nothing.\n"
        );
        if who == TESTER {
            // Nigdy nie przepuszcza: wszystkie trzy rundy mają naprawdę pobiec.
            return format!("{body}\noutcome: fail\n");
        }
        body
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
