//! AC-4 dla T-87: każdy wiersz indeksu przekazań mówi, CZYM jest plik, który wskazuje.
//!
//! # Po co etykieta, skoro jest nazwa kafelka i ścieżka
//!
//! Bo od AC-1 i AC-2 runda trzecia dostaje pięć pozycji, z których trzy pochodzą od dwóch
//! kafelków. Wiersz `- Work: …/handoffs/03__work__findings.md` powtórzony dwa razy pod rząd nie
//! mówi agentowi, który z tych plików jest jego próbą pierwszą, a który drugą — a to jest cała
//! różnica między „popraw to, co zostało odrzucone" a „przeczytaj cokolwiek".
//!
//! Lista jest ZAMKNIĘTA i po angielsku, bez ani jednego naszego słowa z drutu (niezmiennik 14):
//! agent, który właśnie dostał robotę, nie wie, co znaczy „handoff", „verdict" ani „judge",
//! a etykieta wymyślana per wiersz przestaje być etykietą i staje się kolejnym akapitem promptu.
//!
//! # SŁABĄ WERSJĄ jest „wiersz zawiera jakiś tekst poza ścieżką"
//!
//! Przechodzi ją implementacja, która dokleja każdemu wierszowi jedno i to samo zdanie — czyli
//! taka, która wygląda na opisową i nie rozróżnia niczego. Dlatego punkt trzeci pyta o TRZY
//! różne etykiety w JEDNYM prompcie, przy pozycjach o znanym pochodzeniu.
//!
//! # I DLATEGO JEST TU KROK, KTÓREGO PĘTLA NIE DOTYCZY
//!
//! „Aside" ma jednego poprzednika i widzi tylko ostatnią z tych etykiet — tę o kroku przed nim.
//! Implementacja, która nazywa cudzą pracę „twoją poprzednią odpowiedzią", przechodzi wszystko
//! powyżej i przewraca się tutaj.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
//
// `too_many_lines` **wyłącznie dodane**, nie w miejsce niczego: cztery punkty tego kryterium
// czytają JEDEN bieg dziewięciu kroków, dzielących jedną ławkę, jeden magazyn i jedną migawkę
// tego, co zobaczył dubler. Cięcie po granicy funkcji znaczyłoby cztery osobne biegi albo stan
// dzielony między testami, które cargo uruchamia równolegle.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use std::collections::{BTreeMap, BTreeSet};
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

/// Ile czekamy na bieg, zanim uznamy go za zawieszony.
const PATIENCE: Duration = Duration::from_mins(2);

/// Ile razy pętla wolno próbuje.
const TRIES: usize = 3;

/// Początek instrukcji każdego kroku — po nim, i tylko po nim, dubler poznaje, kto pyta.
const ASKED: [(&str, &str); 5] = [
    ("plan:", "Plan"),
    ("work:", "Work"),
    ("test:", "Tester"),
    ("ship:", "Ship"),
    ("aside:", "Aside"),
];

const NAMES: [&str; 5] = ["Plan", "Work", "Tester", "Aside", "Ship"];

/// Kafelek, który stoi obok pętli. Kontrola tego kryterium.
const ASIDE: &str = "Aside";

/// Słowa z drutu, które nie mają prawa dojechać do agenta (niezmiennik 14). Nikt, kto właśnie
/// dostał robotę, nie wie, co znaczy którekolwiek z nich.
const NOT_OUR_WORDS: [&str; 4] = ["handoff", "verdict", "judge", "loop"];

/// Ile najwyżej różnych KSZTAŁTÓW etykiety wolno mieć całemu biegowi. „Zamknięta lista" znaczy
/// garść ustalonych zdań, a nie jedno na wiersz; luz nad czterema przykładami z kontraktu jest
/// zapasem na rozsądny piąty, nie na dowolność.
const AT_MOST_SHAPES: usize = 6;

/// Ile wierszy indeksu musi mieć ten bieg, żeby pytanie o zamkniętą listę w ogóle coś znaczyło.
///
/// Osiem, bo tyle wypisuje ta ławka nawet wtedy, gdy pętla nie niesie jeszcze swoich rund
/// (AC-1) — a to kryterium ma sądzić etykiety, nie czekać na sąsiada.
const AT_LEAST_ROWS: usize = 8;

/// Sufit długości etykiety. „Krótka etykieta" przestaje być etykietą, gdy jest akapitem.
const LABEL_CEILING: usize = 80;

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000d4
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

/// `plan → work → tester → ship`, powrót `tester → work` do trzech rund, i „aside" obok pętli.
const LOOP_FILE: &str = r#"{
  "format": 1,
  "id": "wf_index_says_what_each_file_is",
  "name": "An index that says what things are",
  "steps": [
    {
      "kind": "agent",
      "id": "s_plan",
      "name": "Plan",
      "agent": "01990000-0000-7000-8000-0000000000d4",
      "overrides": {},
      "instructions": "plan: say what to build.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 24 }
    },
    {
      "kind": "agent",
      "id": "s_work",
      "name": "Work",
      "agent": "01990000-0000-7000-8000-0000000000d4",
      "overrides": {},
      "instructions": "work: make the change.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 168 }
    },
    {
      "kind": "agent",
      "id": "s_test",
      "name": "Tester",
      "agent": "01990000-0000-7000-8000-0000000000d4",
      "overrides": {},
      "instructions": "test: say whether it is good enough to build on.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 312 }
    },
    {
      "kind": "agent",
      "id": "s_aside",
      "name": "Aside",
      "agent": "01990000-0000-7000-8000-0000000000d4",
      "overrides": {},
      "instructions": "aside: write the note nobody sends back.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 264, "y": 168 }
    },
    {
      "kind": "agent",
      "id": "s_ship",
      "name": "Ship",
      "agent": "01990000-0000-7000-8000-0000000000d4",
      "overrides": {},
      "instructions": "ship: put it out.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 456 }
    }
  ],
  "links": [
    { "from": "s_plan", "to": "s_work" },
    { "from": "s_plan", "to": "s_aside" },
    { "from": "s_work", "to": "s_test" },
    { "from": "s_test", "to": "s_ship" },
    { "from": "s_test", "to": "s_work", "max_turns": 3 }
  ]
}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_row_of_the_index_says_what_its_file_is() -> Result<(), Box<dyn Error>> {
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
    let every_row: Vec<String> = seen
        .iter()
        .flat_map(|(_, prompt)| index_rows(prompt))
        .collect();
    // ── (a) KAŻDY WIERSZ NAZWANY ─────────────────────────────────────────────────────────────
    // PIERWSZY PUNKT, i to jest wybór: pytanie „czy etykiet jest mało" nad zerem etykiet ma
    // odpowiedź „tak" i nie znaczy nic. Bieg bez ani jednej nazwanej pozycji ma paść tutaj,
    // z powodem, który mówi, czego brakuje.
    for row in &every_row {
        let label = label_of(row);
        assert!(
            !label.is_empty(),
            "a row of the index names a file and says nothing about what it is. From the third \
             try onwards two rows of one index come from the same step, so a person — and an \
             agent — cannot tell the try that was turned down from the one before it. The row \
             was: {row:?}"
        );
        assert!(
            label.len() <= LABEL_CEILING,
            "the label on this row is {} characters long, which is a paragraph, not a label. \
             The index is a list of what is available; the reading happens in the files. It \
             said: {label:?}",
            label.len()
        );
        for word in NOT_OUR_WORDS {
            assert!(
                !label.to_lowercase().contains(word),
                "the label says {word:?} to the agent. That is our word for our own machinery \
                 and it means nothing to somebody who has just been handed a piece of work \
                 (invariant 14). It said: {label:?}"
            );
        }
    }

    // ── (b) ZAMKNIĘTA LISTA, NIE ZDANIE NA WIERSZ ────────────────────────────────────────────
    assert!(
        every_row.len() >= AT_LEAST_ROWS,
        "this run left {} row(s) in the indexes of its steps, and below {AT_LEAST_ROWS} the \
         question underneath — whether a handful of fixed sentences covers them all — is a \
         question about nothing. The run ended as {:?} and the driver was asked by: {:?}",
        every_row.len(),
        report.steps,
        watch.who()
    );
    let shapes: BTreeSet<String> = every_row
        .iter()
        .map(|row| shape_of(&label_of(row)))
        .collect();
    assert!(
        shapes.len() <= AT_MOST_SHAPES,
        "this run produced {} different labels over {} rows. A label written per row is prose, \
         not a closed list: it grows with every branch of the code that composes it, and no two \
         runs read the same. They were: {shapes:?}",
        shapes.len(),
        every_row.len()
    );

    // ── (c) TRZY RÓŻNE RZECZY, TRZY RÓŻNE ETYKIETY ───────────────────────────────────────────
    let work = prompts_of("Work", &seen);
    assert_eq!(
        work.len(),
        TRIES,
        "the fixture is wrong unless all three tries really ran; the point below would then be \
         about a round that never happened. The driver was asked by: {:?}",
        watch.who()
    );
    let wrote = who_wrote_what(&report.dir)?;
    let labelled = labels_by_who(&work[2], &wrote);
    let mut distinct = BTreeSet::new();
    for who in ["Plan try 1", "Work try 1", "Tester try 1"] {
        let label = labelled.get(who).cloned().ok_or_else(|| {
            format!(
                "the last try of the work step was not given {who:?} at all, so this criterion \
                 has nothing to read. It was given: {labelled:?}"
            )
        })?;
        distinct.insert(label);
    }
    assert_eq!(
        distinct.len(),
        3,
        "the input of the loop, this step's own earlier answer and what the tester said carry \
         the same label. One sentence repeated on every row looks descriptive and separates \
         nothing — the agent still has to open all five files to find out which is which. They \
         said: {distinct:?}"
    );

    // ── (d) KONTROLA: KROK, KTÓREGO PĘTLA NIE DOTYCZY ────────────────────────────────────────
    let aside = prompts_of(ASIDE, &seen);
    assert_eq!(
        aside.len(),
        1,
        "the fixture is wrong if the step beside the loop did not run exactly once"
    );
    let beside: BTreeSet<String> = index_rows(&aside[0])
        .iter()
        .map(|row| label_of(row))
        .collect();
    assert_eq!(
        beside.len(),
        1,
        "the step beside the loop has one step before it and reads more than one kind of label. \
         It said: {beside:?}"
    );
    for label in &beside {
        assert!(
            !distinct.contains(label),
            "the step beside the loop is told that somebody else's work is its own earlier \
             answer, or that it was handed the start of a loop it is not in. Only the last of \
             the labels — the one about the step before it — belongs here. It said: {label:?}"
        );
    }
    Ok(())
}

// ── indeks, wiersz po wierszu ──────────────────────────────────────────────────────────────

/// Wiersze indeksu tego promptu — po jednym na wymieniony plik.
fn index_rows(prompt: &str) -> Vec<String> {
    prompt
        .lines()
        .filter(|line| line.contains("handoffs/"))
        .map(str::to_owned)
        .collect()
}

/// Etykieta wiersza: to, co zostaje po wyjęciu ścieżki, nazwy kafelka i interpunkcji listy.
///
/// Liczona ODEJMOWANIEM, a nie dopasowaniem do wzorca, bo kształt wiersza jest wyborem
/// implementacji: etykieta wolno stoi przed ścieżką albo za nią, byle w TYM SAMYM wierszu —
/// odnośnik i to, czym jest, czytane osobno są dwiema listami do zestawienia w głowie.
fn label_of(row: &str) -> String {
    let kept: Vec<&str> = row
        .split_whitespace()
        .filter(|word| !word.contains("handoffs/"))
        .collect();
    let mut text = kept.join(" ");
    for name in NAMES {
        text = text.replace(name, " ");
    }
    text.trim_matches(|glyph: char| !glyph.is_alphanumeric())
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
}

/// Etykieta bez liczb: „(try 1 of 3)" i „(try 2 of 3)" to jedno zdanie z listy, nie dwa.
fn shape_of(label: &str) -> String {
    label
        .chars()
        .map(|glyph| if glyph.is_ascii_digit() { '#' } else { glyph })
        .collect()
}

/// Podpis autora → etykieta, którą jego plik dostał w tym prompcie.
fn labels_by_who(prompt: &str, wrote: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for row in index_rows(prompt) {
        for (file, who) in wrote {
            if row.contains(file.as_str()) {
                out.insert(who.clone(), label_of(&row));
            }
        }
    }
    out
}

// ── co dojechało do sterownika ─────────────────────────────────────────────────────────────

fn prompts_of(name: &str, seen: &[(String, String)]) -> Vec<String> {
    seen.iter()
        .filter(|(who, _)| who == name)
        .map(|(_, prompt)| prompt.clone())
        .collect()
}

/// Plik przekazania → podpis tego, kto go zostawił („Work try 2").
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
        if who == "Tester" {
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
