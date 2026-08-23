//! AC-2 dla T-86: agent wie, ile ma minut — i wie, kiedy nie ma limitu.
//!
//! `give_up_after` odbiera krokowi robotę po limicie (`Live::one_turn` → `Ended::Overdue`), a do
//! promptu nie wchodzi ani jedną literą. Agent planuje więc sześćdziesięciominutową robotę
//! w kroku, który ma dziesięć minut, i ginie w połowie bez jednego zdania w tym, co przekazuje
//! dalej. Limit, o którym wie wyłącznie ten, kto zabija, jest karą, a nie ograniczeniem —
//! `agents-are-never-told-their-deadline`.
//!
//! # Kontrakt, który to kryterium egzekwuje
//!
//! Blok z AC-1 nazywa limit **liczbą minut z definicji efektywnej** (agent plus nadpisanie kroku),
//! a przy `giveUpAfterMinutes: 0` mówi wprost, że limitu nie ma:
//!
//! ```text
//! You have 20 minutes for this step.       ← agent mówi 20, krok nie nadpisuje
//! You have 7 minutes for this step.        ← krok nadpisuje na 7
//! There is no time limit on this step.     ← krok nadpisuje na 0
//! ```
//!
//! # SŁABA WERSJA numer jeden: `assert!(prompt.contains("minutes"))`
//!
//! Przechodzi dla implementacji, która wpisuje wszystkim jedną liczbę — na przykład domyślne
//! dwadzieścia z `library::agents` — i wtedy krok zawężony do siedmiu minut dostaje zdanie, które
//! jest nieprawdą, a agent planuje pracę na trzy razy dłużej, niż mu wolno. Rozróżnia to bieg,
//! w którym **dwa kroki jednego agenta** mają dwa różne limity, sądzone liczbą wyjętą z promptu.
//!
//! # SŁABA WERSJA numer dwa: „zero to też liczba"
//!
//! `0` w definicji agenta znaczy „bez limitu" (`library::agents::Agent::give_up_after_minutes`).
//! Prompt mówiący „you have 0 minutes" jest zdaniem, po którym model nie ma nic sensownego do
//! zrobienia — a wygląda dokładnie tak samo w kodzie, który liczbę tylko podstawia. Dlatego trzeci
//! krok ławki nadpisuje limit na zero i kryterium pyta o **brak liczby** i o zdanie wprost.
//!
//! # Czego to kryterium NIE rozstrzyga
//!
//! Gdzie dokładnie wewnątrz bloku stoi to zdanie. Sądzi, że agent dostaje właściwą liczbę i że
//! dostaje ją w części promptu, którą Loadout dokleja za indeksem przekazań — nie kolejność zdań
//! w środku umowy. Kryterium o kolejności byłoby kryterium o stylu, a te w tym repo mieszkają
//! w recenzji, nie w bramce.

// `unwrap()`/`expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

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
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera. Nie „claude" i nie „codex": tamte dwie mają w biegu własne wymagania
/// co do dowodów, a to kryterium sądzi tekst promptu, nie sterownik.
const VENDOR: &str = "fake";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(60);

/// Zdanie, po którym poznajemy blok z AC-1. Bez niego nie ma czego pytać o limit.
const OPENS: &str = "Your last message";

/// Zdanie, którym blok mówi, że limitu nie ma. Fragment, nie całe zdanie: kryterium sądzi, że
/// brak limitu jest NAZWANY, a nie przepisuje copy słowo w słowo.
const SAYS_NO_LIMIT: &str = "no time limit";

/// Limit z definicji agenta. Krok, który go nie nadpisuje, ma dostać dokładnie tę liczbę.
const AGENTS_MINUTES: u32 = 20;
/// Limit zawężony na kroku. Inny niż agenta i inny niż zero — to jest cała treść punktu (b).
const SHORTER_MINUTES: u32 = 7;

/// Agent z limitem dwudziestu minut. `0` w tym polu znaczyłoby „bez limitu", więc stoi tu liczba,
/// żeby nadpisanie na kroku miało co przebić.
const HAND: &str = "---
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

/// Instrukcja kroku → nazwa kroku. Krok rozpoznajemy po treści zadania, bo `RunSpec` nie niesie
/// nazwy kroku. ŻADNA z tych instrukcji nie ma w sobie cyfry ani słowa o czasie: liczba wyjęta
/// z promptu ma pochodzić z bloku, a nie z zadania, które sami tam wpisaliśmy.
const STEPS: [(&str, &str); 3] = [
    ("without: ", "Without"),
    ("inherits: ", "Inherits"),
    ("shorter: ", "Shorter"),
];

/// Krok, który nadpisuje limit na zero, czyli „bez limitu".
const WITHOUT: &str = "Without";
/// Krok bez nadpisania: bierze limit swojego agenta.
const INHERITS: &str = "Inherits";
/// Krok, który zawęża limit do siedmiu minut.
const SHORTER: &str = "Shorter";

/// Trzy kroki jednego agenta w łańcuchu, trzy różne odpowiedzi na pytanie „ile mam czasu".
///
/// Łańcuch, a nie trzy luźne kafelki: dzięki strzałkom dwa z trzech kroków mają przed sobą
/// poprzednika, więc da się zapytać także o to, że zdanie o czasie stoi w części, którą Loadout
/// dokleja ZA indeksem przekazań — a nie zostało wpisane w zadanie kroku.
///
/// Każdy krok na WŁASNEJ KOPII plików: dwa kroki piszące po tych samych ścieżkach są odmową
/// `check_to_run` (niezmiennik 12), a nie fiksturą.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_knows_its_deadline",
  "name": "Three steps, three deadlines",
  "steps": [
    {
      "kind": "agent",
      "id": "s_without",
      "name": "Without",
      "agent": "01990000-0000-7000-8000-0000000000e2",
      "overrides": { "giveUpAfterMinutes": 0 },
      "instructions": "without: read the notes and say what they are for.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_inherits",
      "name": "Inherits",
      "agent": "01990000-0000-7000-8000-0000000000e2",
      "overrides": {},
      "instructions": "inherits: say the same thing in fewer words.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_shorter",
      "name": "Shorter",
      "agent": "01990000-0000-7000-8000-0000000000e2",
      "overrides": { "giveUpAfterMinutes": 7 },
      "instructions": "shorter: say which of the two answers is better.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 480, "y": 0 }
    }
  ],
  "links": [
    { "from": "s_without", "to": "s_inherits" },
    { "from": "s_inherits", "to": "s_shorter" }
  ]
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn each_step_is_told_the_minutes_its_own_definition_gives_it() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND)?;
    let workflow = bench.workflow("knows-its-deadline", WORKFLOW)?;
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

    let looked = seen.snapshot();
    let mut named = STEPS
        .iter()
        .map(|(_, name)| (*name).to_owned())
        .collect::<Vec<_>>();
    named.sort();
    assert_eq!(
        looked.keys().cloned().collect::<Vec<_>>(),
        named,
        "every step has to reach the driver under its own name, or the assertions below are true \
         of steps that never ran. The run ended as {:?} and the driver saw: {:?}",
        report.steps,
        looked.keys().collect::<Vec<_>>()
    );

    let without = looked.get(WITHOUT).cloned().unwrap_or_default();
    let inherits = looked.get(INHERITS).cloned().unwrap_or_default();
    let shorter = looked.get(SHORTER).cloned().unwrap_or_default();

    // ── (a) BEZ NADPISANIA: LICZBA Z DEFINICJI AGENTA ────────────────────────────────────────
    assert_eq!(
        minutes_named(&inherits),
        vec![AGENTS_MINUTES],
        "the step that overrides nothing has to be told the {AGENTS_MINUTES} minutes its agent \
         gives it, once and unambiguously. Its prompt was: {inherits:?}"
    );

    // ── (b) Z NADPISANIEM: LICZBA Z KROKU, NIE Z AGENTA ─────────────────────────────────────
    // TA asercja jest jedyną, która widzi implementację czytającą samą definicję agenta: dla
    // kroku bez nadpisania obie odpowiadają tak samo, a człowiek, który zawęził czas na panelu
    // kroku, dostaje agenta planującego pracę na trzy razy dłużej, niż mu wolno.
    assert_eq!(
        minutes_named(&shorter),
        vec![SHORTER_MINUTES],
        "this step narrows its agent down to {SHORTER_MINUTES} minutes, so that is the number it \
         must be told — {AGENTS_MINUTES} would be a sentence that is not true, and the agent \
         would plan against it. Its prompt was: {shorter:?}"
    );

    // …i to naprawdę są DWIE RÓŻNE liczby w jednym biegu, nie jedna powtórzona.
    assert_ne!(
        minutes_named(&inherits),
        minutes_named(&shorter),
        "two steps of one run have two different limits and were told the same number. A limit \
         that does not follow the step is a limit nobody can act on"
    );

    // ── (c) ZERO ZNACZY „BEZ LIMITU", NIE „ZERO MINUT" ──────────────────────────────────────
    assert!(
        minutes_named(&without).is_empty(),
        "the step whose limit is 0 was told a number of minutes: {:?}. Zero means there is no \
         limit at all (library::agents), so any number here is either a lie or an instruction to \
         give up at once. Its prompt was: {without:?}",
        minutes_named(&without)
    );
    assert!(
        without.contains(SAYS_NO_LIMIT),
        "the step whose limit is 0 was told nothing about time at all. Silence is not the same \
         answer as \"there is no limit\": an agent that is told nothing budgets for the limit it \
         guesses, and guesses low. Its prompt was: {without:?}"
    );

    // ── (d) I ZDANIE STOI W BLOKU, ZA INDEKSEM PRZEKAZAŃ ────────────────────────────────────
    // Bez tego punktu „prompt zawiera liczbę" byłoby prawdą także dla liczby doklejonej do
    // zadania kroku — czyli w miejscu, które jedzie do `run.json` i do każdej następnej rundy
    // pętli, więc rosłoby o kopię na rundę.
    for (name, prompt) in [
        (WITHOUT, &without),
        (INHERITS, &inherits),
        (SHORTER, &shorter),
    ] {
        assert!(
            prompt.contains(OPENS),
            "the step \"{name}\" got no block at all, so there is nowhere for a sentence about \
             time to stand. Its prompt was: {prompt:?}"
        );
    }
    for (name, prompt) in [(INHERITS, &inherits), (SHORTER, &shorter)] {
        let index_at = prompt
            .find("handoffs/")
            .ok_or_else(|| format!("the fixture is wrong: \"{name}\" has no step before it"))?;
        let said_at = prompt
            .find("minute")
            .ok_or_else(|| format!("\"{name}\" was told no number of minutes at all"))?;
        assert!(
            index_at < said_at,
            "\"{name}\" hears about its time limit BEFORE the list of what the steps before it \
             left — so the sentence was put into the step's own task instead of the block \
             Loadout adds. A task carrying it is written into run.json and repeated in every \
             turn of a loop"
        );
    }

    Ok(())
}

/// Każda liczba minut nazwana w tym prompcie, w kolejności wystąpień.
///
/// Bez wyrażeń regularnych — `regex` nie jest zależnością tego drzewa, a `Cargo.toml` nie należy
/// do tego zadania (AGENTS.md §7). Cyfry czytamy WSTECZ od słowa „minute", bo to jest jedyny
/// kształt, w którym liczba minut znaczy limit: `20 minutes`, `7 minutes`. Zdanie mówiące
/// „no time limit" nie niesie ani jednej cyfry i wraca stąd pustą listą — i to jest odpowiedź,
/// nie brak odpowiedzi.
fn minutes_named(prompt: &str) -> Vec<u32> {
    let mut found = Vec::new();
    for (at, _) in prompt.match_indices("minute") {
        let before = prompt[..at].trim_end();
        let digits: String = before
            .chars()
            .rev()
            .take_while(char::is_ascii_digit)
            .collect::<Vec<char>>()
            .into_iter()
            .rev()
            .collect();
        if let Ok(number) = digits.parse::<u32>() {
            found.push(number);
        }
    }
    found
}

// ── co dubler zobaczył ─────────────────────────────────────────────────────────────────────

/// Prompt, który dojechał do sterownika, po nazwie kroku.
#[derive(Debug, Default)]
struct Seen(Mutex<BTreeMap<String, String>>);

impl Seen {
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym wywołaniu,
    /// więc nie ma wyrażenia, w którym dożyłby do `await`.
    fn record(&self, step: &str, prompt: String) {
        self.lock().entry(step.to_owned()).or_insert(prompt);
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
        // Zadanie, którego nie ma w tablicy, ląduje pod SWOJĄ treścią, nie pod cudzą nazwą:
        // asercja o nazwach kroków ma wtedy paść i pokazać, czego test nie rozpoznał.
        let step = STEPS
            .iter()
            .find(|(instruction, _)| spec.prompt.starts_with(instruction))
            .map_or_else(|| spec.prompt.clone(), |(_, name)| (*name).to_owned());
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
            text: "## Answer\nthe step did the work.\n\n## Evidence\nnotes.txt:1\n\n## Open\nnothing.\n"
                .to_owned(),
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
