//! AC-5 dla T-87: każda porażka idzie tą samą drogą, i ta, która ma jechać dalej, zostawia po
//! sobie plik.
//!
//! # Trzy ścieżki, które omijają jedyne miejsce decydujące o porażce
//!
//! `Live::when_this_one_fails` (`commands/run.rs`) istnieje po to, żeby `carry-on` i `ask-me`
//! znaczyły to samo niezależnie od tego, CO padło. Trzy drogi wychodzą dziś obok niej i wracają
//! gołym `StepReport::Failed`:
//!
//! * komenda kroku „sprawdź", która nie wystartowała (`run_check`, gałąź `Err` przy `start`),
//! * ta sama komenda po przekroczeniu limitu czasu (`CheckHow::Overdue`),
//! * tura agenta, która wróciła błędem (`Ended::Turn(Err(_))` w `one_turn`).
//!
//! Na każdej z nich ustawienie człowieka „jedź dalej mimo wszystko" jest martwe: stożek za
//! krokiem schodzi jako `skipped`, cicho i bez wyboru. To jest dokładnie ten ślepy punkt, którego
//! `whenItFails` miało nie mieć — tylko że sprawdza go wyłącznie jedna z czterech ścieżek.
//!
//! # Druga połowa: milcząca luka w indeksie jest gorsza niż zła wiadomość
//!
//! Krok, który padł i przepuścił robotę dalej, ma zostawić przekazanie z tym, co zdążył
//! powiedzieć — choćby puste — a następny krok ma zobaczyć wiersz mówiący, że ten krok nie
//! przeszedł. Bez tego wiersza synteza dostaje o jedno wejście MNIEJ i nie ma jak się dowiedzieć,
//! że czegoś brakuje: cisza w indeksie wygląda identycznie jak gałąź, której nigdy nie było.
//!
//! # SŁABĄ WERSJĄ jest „krok za nim pobiegł"
//!
//! Przechodzi ją implementacja, która po prostu przestaje malować stożek na czerwono i nie mówi
//! następnemu krokowi ani słowa — czyli buduje syntezę na robocie, której nikt nie przyjął, i
//! nazywa to sukcesem. Dlatego każdy z trzech punktów niżej pyta też o wiersz w indeksie.
//!
//! # Kontrola: `stop` nadal nie oddaje nic dalej
//!
//! Bo za nim nic nie biegnie. Bez tego punktu wszystkie trzy asercje przechodzą dla
//! implementacji, która ustawienie człowieka ignoruje i zawsze jedzie dalej.

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
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
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

/// Początek instrukcji kroku, który ma paść.
const BREAKS: &str = "break:";

/// Początek instrukcji kroku stojącego ZA tym, który padł. To jego prompt jest odpowiedzią.
const NEXT: &str = "next:";

/// Nazwa kafelka, który stoi za porażką — ta sama we wszystkich czterech ławkach.
const AFTER: &str = "After";

/// Fragment, po którym poznajemy etykietę mówiącą, że poprzednik nie przeszedł. Zdanie wolno
/// napisać inaczej; ta jedna rzecz musi w nim stać, bo inaczej wiersz nie odróżnia materiału
/// przyjętego od odrzuconego — a to jest jedyny powód, dla którego on tam jest.
const DID_NOT_PASS: &str = "did not pass";

/// To, co agent zdążył powiedzieć, zanim jego tura się przewróciła. Zdanie jest rozpoznawalne
/// z rozmysłu: ma je napisać AGENT, więc nie może dać się pomylić z niczym, co pisze Loadout.
const LAST_WORDS: &str = "I got as far as reading the notes and writing down two of them.";

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000d5
name: Hand
summary: Does the work
color: moss
runsWith: claude-code
model: opus
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 0
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Do the work.
";

/// Krok agenta, który przewróci się w połowie tury, i krok za nim.
///
/// `__WHEN_IT_FAILS__` podstawiamy w teście: te same dwa kroki sądzą i `carry-on`, i `stop`.
const BROKEN_AGENT: &str = r#"{
  "format": 1,
  "id": "wf_last_words_agent",
  "name": "An agent that breaks",
  "steps": [
    {
      "kind": "agent",
      "id": "s_break",
      "name": "Broken",
      "agent": "01990000-0000-7000-8000-0000000000d5",
      "overrides": {},
      "whenItFails": "__WHEN_IT_FAILS__",
      "instructions": "break: start the work and fall over.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 24 }
    },
    {
      "kind": "agent",
      "id": "s_after",
      "name": "After",
      "agent": "01990000-0000-7000-8000-0000000000d5",
      "overrides": {},
      "instructions": "next: carry on with whatever came out of it.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 168 }
    }
  ],
  "links": [{ "from": "s_break", "to": "s_after" }]
}"#;

/// Krok „sprawdź", którego komenda nie ma jak wystartować, i krok za nim.
///
/// Katalog wskazany ręcznie jest CUDZY — Loadout go nie tworzy (`commands::run::workspace`) —
/// więc wskazanie nieistniejącego jest jedyną drogą do gałęzi `Err` przy starcie komendy, i jest
/// nią także u człowieka, który zrobi literówkę w ścieżce.
const CHECK_THAT_CANNOT_START: &str = r#"{
  "format": 1,
  "id": "wf_last_words_no_start",
  "name": "A check that cannot start",
  "steps": [
    {
      "kind": "check",
      "id": "s_break",
      "name": "Broken",
      "command": "echo 1 passed",
      "proof": "(\\d+) passed",
      "whenItFails": "carry-on",
      "folder": { "use": "pick", "path": "__NOWHERE__" },
      "at": { "x": 24, "y": 24 }
    },
    {
      "kind": "agent",
      "id": "s_after",
      "name": "After",
      "agent": "01990000-0000-7000-8000-0000000000d5",
      "overrides": {},
      "instructions": "next: carry on with whatever came out of it.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 168 }
    }
  ],
  "links": [{ "from": "s_break", "to": "s_after" }]
}"#;

/// Krok „sprawdź", którego komenda nie kończy się nigdy, i krok za nim.
const CHECK_THAT_NEVER_ENDS: &str = r#"{
  "format": 1,
  "id": "wf_last_words_overdue",
  "name": "A check that never ends",
  "steps": [
    {
      "kind": "check",
      "id": "s_break",
      "name": "Broken",
      "command": "sleep 100000",
      "proof": "(\\d+) passed",
      "whenItFails": "carry-on",
      "folder": { "use": "project" },
      "at": { "x": 24, "y": 24 }
    },
    {
      "kind": "agent",
      "id": "s_after",
      "name": "After",
      "agent": "01990000-0000-7000-8000-0000000000d5",
      "overrides": {},
      "instructions": "next: carry on with whatever came out of it.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 168 }
    }
  ],
  "links": [{ "from": "s_break", "to": "s_after" }]
}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_agent_whose_turn_broke_leaves_its_last_words() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let file = BROKEN_AGENT.replace("__WHEN_IT_FAILS__", "carry-on");
    let watch = Arc::new(Watch::breaking_on(BREAKS));
    let (report, watch) = run_it(&bench, &file, watch).await?;

    what_the_next_step_was_told(
        &report,
        &watch,
        "an agent turn that came back with an error goes past `when_this_one_fails` entirely, so \
         the person's choice to carry on is dead on this path — the steps after it are painted \
         over in silence",
    )?;

    /* DRUGA POŁOWA: WIERSZ W INDEKSIE MA PROWADZIĆ DO CZEGOŚ. Krok, który padł, oddaje dalej
     * „to, co zdążył powiedzieć" — a jedynym miejscem, w którym te słowa istniały, była kolejka
     * zdarzeń idąca na ekran. Plik pusty przechodzi każdą asercję wyżej i nie mówi następnemu
     * agentowi ani słowa o tym, jak daleko doszedł ten przed nim. */
    let seen = watch.seen();
    let told = prompts_of(AFTER, &seen);
    let path = the_file_it_was_given(&told[0])?;
    let handed = fs::read_to_string(&path)?;
    assert!(
        handed.contains(LAST_WORDS),
        "the step that fell over handed on a file without a word of what its agent had already \
         said. The prose reached the screen and stopped there: nothing on this path keeps it, so \
         the next step opens the path in its index and finds a heading over nothing. The file \
         {path:?} read: {handed:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_check_that_could_not_start_leaves_its_last_words() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let nowhere = bench.project.path().join("no-such-folder");
    assert!(
        !nowhere.exists(),
        "the fixture is wrong if the folder the check is pointed at already exists"
    );
    let file = CHECK_THAT_CANNOT_START.replace("__NOWHERE__", &nowhere.display().to_string());
    let (report, watch) = run_it(&bench, &file, Arc::new(Watch::default())).await?;

    // Ławka ma dotknąć TEJ gałęzi, nie sąsiedniej: komenda, która wystartowała i nie przeszła,
    // oddaje przekazanie od zawsze, więc kryterium mierzyłoby wtedy zachowanie już istniejące.
    let why = the_reason_for(&report.dir, "Broken")?;
    assert!(
        why.contains("could not start"),
        "the fixture is wrong: this check was supposed to fail because its command never \
         started. It failed for another reason, so the path this criterion is about was never \
         reached. It said: {why:?}"
    );

    what_the_next_step_was_told(
        &report,
        &watch,
        "a check whose command never started goes past `when_this_one_fails` entirely, so the \
         person's choice to carry on is dead on this path — and a typo in a folder name takes \
         the rest of the run down with it",
    )
}

/* ZEGAR ZATRZYMANY, KOMENDA PRAWDZIWA. Limit kroku „sprawdź" to trzydzieści minut stałej
 * (`engine::drivers::command::GIVE_UP_AFTER`) i nie da się go zawęzić z zewnątrz — a kryterium,
 * które naprawdę czeka pół godziny, jest kryterium, którego nikt nigdy nie uruchomi.
 *
 * `start_paused` przewija zegar, kiedy biegowi nie zostaje nic do zrobienia poza czekaniem, więc
 * komenda, która nie kończy się nigdy, przekracza swój limit natychmiast i drogą PRAWDZIWĄ:
 * przez `Checking::settle`, eskalację zabijania i dowód zejścia grupy (niezmienniki 6 i 10).
 *
 * I DLATEGO NIE MA TU `tokio::time::timeout` WOKÓŁ BIEGU. Pod zatrzymanym zegarem strażnik
 * cierpliwości jest po prostu najbliższym terminem, więc to on wypaliłby pierwszy — czyli
 * kryterium mierzyłoby własny limit zamiast limitu kroku. Zawieszony bieg łapie limit bramki.
 */
#[tokio::test(start_paused = true)]
async fn a_check_that_ran_out_of_time_leaves_its_last_words() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let watch = Arc::new(Watch::default());
    let store = Store::open(&bench.db())?;
    let workflow = bench.workflow("overdue", CHECK_THAT_NEVER_ENDS)?;

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
    let report = run_workflow_inner(&deps, &request, sink).await?;
    pump.abort();

    // Ławka ma dotknąć TEJ gałęzi, nie sąsiedniej: gdyby `sleep` nie wstał, byłoby to drugie
    // kryterium o starcie komendy, a limit czasu nie byłby sprawdzony przez nic.
    let why = the_reason_for(&report.dir, "Broken")?;
    assert!(
        why.contains("longer than"),
        "the fixture is wrong: this check was supposed to be stopped by its own time limit. It \
         ended some other way, so the path this criterion is about was never reached. It said: \
         {why:?}"
    );

    what_the_next_step_was_told(
        &report,
        &watch,
        "a check stopped by its own time limit goes past `when_this_one_fails` entirely, so the \
         person's choice to carry on is dead on this path — one slow command ends the run",
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_step_told_to_stop_hands_nothing_on() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let file = BROKEN_AGENT.replace("__WHEN_IT_FAILS__", "stop");
    let watch = Arc::new(Watch::breaking_on(BREAKS));
    let (report, watch) = run_it(&bench, &file, watch).await?;

    assert!(
        prompts_of(AFTER, &watch.seen()).is_empty(),
        "the step after a failure the person asked to STOP on was started anyway. Then the \
         setting means nothing and every run carries on regardless of what the person chose. \
         The driver was asked by: {:?}",
        watch.who()
    );
    let states = states_of(&report.dir)?;
    assert!(
        states
            .iter()
            .any(|(name, state)| name == AFTER && state == "skipped"),
        "the step after it did not read as skipped, so nothing tells the person where the run \
         stopped. It read: {states:?}"
    );
    Ok(())
}

// ── jedno pytanie zadane trzem drogom porażki ──────────────────────────────────────────────

/// Czy krok stojący za porażką ruszył i co dostał w indeksie.
///
/// Jedna funkcja na trzy ścieżki, bo pytanie jest jedno: `carry-on` ma znaczyć to samo,
/// niezależnie od tego, CO padło. Trzy kopie tych asercji rozjechałyby się przy pierwszej
/// poprawce, a rozjazd znaczyłby, że jedna z dróg znowu chodzi obok.
fn what_the_next_step_was_told(
    report: &RunReport,
    watch: &Watch,
    what_is_broken: &str,
) -> Result<(), Box<dyn Error>> {
    let seen = watch.seen();
    let after = prompts_of(AFTER, &seen);
    assert_eq!(
        after.len(),
        1,
        "the step after the failure never ran. {what_is_broken}. The run ended as {:?} and the \
         driver was asked by: {:?}",
        report.steps,
        watch.who()
    );

    let states = states_of(&report.dir)?;
    assert!(
        states
            .iter()
            .any(|(name, state)| name == "Broken" && state == "failed"),
        "the step that broke does not read as failed. Carrying the work on is not the same as \
         pretending the step worked — a filled block over a step that fell over is the one lie \
         this product exists to prevent. It read: {states:?}"
    );

    let rows = index_rows(&after[0]);
    assert_eq!(
        rows.len(),
        1,
        "the step after the failure was given {} row(s) in its index, not the one file the step \
         before it left. A step that fell over and let the work through has to hand on what it \
         managed to say — even nothing at all — because a silent gap in the index looks exactly \
         like a branch that never existed. Its prompt was: {:?}",
        rows.len(),
        after[0]
    );
    assert!(
        rows[0].contains(DID_NOT_PASS),
        "the one row it did get does not say that the step before it {DID_NOT_PASS}. Then the \
         next agent builds on material nobody accepted and has no way of knowing. The row was: \
         {:?}",
        rows[0]
    );
    Ok(())
}

// ── ławka i bieg ───────────────────────────────────────────────────────────────────────────

/// Jeden bieg tego pliku workflow. Oddaje raport i to, co zobaczył dubler.
async fn run_it(
    bench: &Bench,
    file: &str,
    watch: Arc<Watch>,
) -> Result<(RunReport, Arc<Watch>), Box<dyn Error>> {
    let store = Store::open(&bench.db())?;
    let workflow = bench.workflow("last-words", file)?;
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
    Ok((report, watch))
}

/// Stany kroków z `run.json`, po nazwie kafelka.
fn states_of(dir: &Path) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let text = fs::read_to_string(dir.join("run.json"))?;
    let described: serde_json::Value = serde_json::from_str(&text)?;
    Ok(described["steps"]
        .as_array()
        .ok_or("run.json has no steps")?
        .iter()
        .map(|one| {
            (
                one["name"].as_str().unwrap_or_default().to_owned(),
                one["status"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect())
}

/// Powód zapisany przy tym kroku w `run.json` — pusty napis, gdy nikt go nie zapisał.
fn the_reason_for(dir: &Path, name: &str) -> Result<String, Box<dyn Error>> {
    let text = fs::read_to_string(dir.join("run.json"))?;
    let described: serde_json::Value = serde_json::from_str(&text)?;
    Ok(described["steps"]
        .as_array()
        .ok_or("run.json has no steps")?
        .iter()
        .filter(|one| one["name"].as_str() == Some(name))
        .find_map(|one| one["error"].as_str())
        .unwrap_or_default()
        .to_owned())
}

/// Ścieżka pliku, który ten prompt wymienia w indeksie — pierwszego z brzegu.
fn the_file_it_was_given(prompt: &str) -> Result<PathBuf, Box<dyn Error>> {
    let named = prompt
        .split_whitespace()
        .find(|word| word.contains("handoffs/"))
        .ok_or("the step after the failure was given no file at all")?;
    Ok(PathBuf::from(named.trim_end_matches([',', ';', ':', ')'])))
}

/// Wiersze indeksu tego promptu — po jednym na wymieniony plik.
fn index_rows(prompt: &str) -> Vec<String> {
    prompt
        .lines()
        .filter(|line| line.contains("handoffs/"))
        .map(str::to_owned)
        .collect()
}

fn prompts_of(name: &str, seen: &[(String, String)]) -> Vec<String> {
    seen.iter()
        .filter(|(who, _)| who == name)
        .map(|(_, prompt)| prompt.clone())
        .collect()
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

/// Kto został o co poproszony — i czyja tura ma wrócić błędem.
#[derive(Debug, Default)]
struct Watch {
    seen: Mutex<Vec<(String, String)>>,
    /// Początek instrukcji kroku, którego tura się przewróci. `None` znaczy „żaden".
    breaks_on: Option<&'static str>,
}

impl Watch {
    fn breaking_on(opening: &'static str) -> Self {
        Self {
            seen: Mutex::new(Vec::new()),
            breaks_on: Some(opening),
        }
    }

    /// Zapisuje start i mówi, czy ta tura ma się przewrócić.
    fn entered(&self, prompt: &str) -> bool {
        let who = if prompt.starts_with(NEXT) {
            AFTER.to_owned()
        } else {
            prompt.to_owned()
        };
        self.lock().push((who, prompt.to_owned()));
        self.breaks_on
            .is_some_and(|opening| prompt.starts_with(opening))
    }

    fn seen(&self) -> Vec<(String, String)> {
        self.lock().clone()
    }

    fn who(&self) -> Vec<String> {
        self.lock().iter().map(|(who, _)| who.clone()).collect()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<(String, String)>> {
        self.seen.lock().unwrap_or_else(PoisonError::into_inner)
    }
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
        let breaks = self.watch.entered(&spec.prompt);
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
            breaks,
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    /// Czy ta tura ma wrócić błędem, zamiast wynikiem.
    breaks: bool,
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
        if self.breaks {
            /* AGENT ZDĄŻYŁ COŚ POWIEDZIEĆ, i dopiero potem jego aplikacja odeszła. Tak wygląda
             * prawdziwa awaria w środku tury: proza już poszła zdarzeniami, a wyniku nie ma
             * i nie będzie. Tura, która milczy do końca, nie odróżniłaby pliku z ostatnimi
             * słowami od pliku pustego. */
            let _ = self
                .events
                .send(
                    (AgentEvent::Said {
                        text: LAST_WORDS.to_owned(),
                    })
                    .into(),
                )
                .await;
            // Ta sama forma, jaką ma prawdziwa awaria sterownika: tury nie ma, jest błąd.
            anyhow::bail!("the agent app went away in the middle of the turn");
        }
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "## Answer\nThe step after it did its work.\n\n## Evidence\nnotes.txt:1\n\n\
                   ## Open\nnothing.\n"
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
        fs::write(home.path().join("agents").join("hand.md"), HAND_FILE)?;
        Ok(Self { home, project })
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
