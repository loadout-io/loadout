//! Wskaźnik, który Loadout SAM wpisuje w ucięte przekazanie, musi dać się otworzyć.
//!
//! `memory::handoff` ucina ciało na `BODY_CAP` (8 KB), wpisuje w nie wiersz
//! `Moved to attachments/<nazwa>__full.md` i pisze tam oryginał. Prompt następnego kroku niesie
//! **ścieżkę**, nigdy treść (D6 punkt 5), więc ten wskaźnik jest jedyną drogą do zdania, które
//! cięcie zabrało. Nagłówek `commands::run` mówi o tym wprost: „skoro ścieżka jest jedyną drogą
//! do treści, to musi **działać**" — i dokładnie dlatego katalog przekazań jedzie do sterownika
//! w `RunSpec::extra_dirs`. Katalog `attachments/` leży **obok** `handoffs/`, więc tamten wpis
//! go nie pokrywa.
//!
//! **Zmierzone na biegu `20260819-223942`, czyli nie teoria:** krok `Analysis` dostał przekazanie
//! z trzema wskaźnikami do `attachments/00__plan__findings__full.md`, nie mógł otworzyć ani
//! jednego, napisał „the plan's full attachment is missing (only the truncated handoff exists)"
//! i wyliczył cały dowód po raz drugi wprost z repo — 9 minut z 10-minutowego limitu, po którym
//! krok jest zabijany. Odnośnik bez prawa otwarcia jest odnośnikiem bez handlera (niezmiennik 16),
//! a plik, którego nie ma kto przeczytać, jest artefaktem z niezmiennika 21 czytanego od drugiej
//! strony.
//!
//! **Słabą wersją jest asercja, że `extra_dirs` jest niepuste.** Przechodzi ją dzisiejszy kod,
//! który wkłada tam `handoffs/` i nic więcej — czyli stan, w którym wskaźnik dalej nie prowadzi
//! nigdzie. Rozstrzyga dopiero para: przekazanie **jest** ucięte (przesłanka, sprawdzana przed
//! biegiem asercją o istnieniu załącznika) i katalog załącznika **jest** wśród tych, które krok
//! ma prawo czytać.
//!
//! Oba kroki mają `fresh-copy`, bo to jest jedyny układ, w którym ta różnica jest widoczna:
//! krok stoi wtedy w `work/<krok>`, a katalog biegu leży poza jego drzewem.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::memory::handoff::{ATTACHMENTS_DIR, BODY_CAP};
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Ile trwa jedna tura dublera.
const TURN: Duration = Duration::from_millis(40);

/// Ile czekamy na cały bieg, zanim uznamy go za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(20);

/// Identyfikator pierwszego kroku — i ostatni człon jego katalogu roboczego.
const SCOUT: &str = "s_scout";
/// To samo dla kroku, którego prawa do czytania sądzi to kryterium.
const DECIDER: &str = "s_decider";

/// Jedno zdanie odpowiedzi zwiadowcy, powtarzane do przekroczenia [`BODY_CAP`].
///
/// Proza, nie `"x".repeat(n)`: `cap` cięte po granicy wiersza (`last_line_boundary`), więc ciało
/// bez znaków nowej linii przechodziłoby tę ścieżkę inaczej niż cokolwiek, co przyśle model.
const SENTENCE: &str = "The rebuild path reads the run directory and nothing else, so a column \
                        that never reaches a file stops existing the moment the index is dropped.";

/// Odpowiedź drugiego kroku. Krótka, bo jego przekazania nikt tu nie czyta.
const DECIDER_REPLY: &str = "Start with the index on the token column.";

const SCOUT_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000e1
name: Scout
summary: Reads the ground
color: slate
runsWith: claude-code
model: opus
thinking: balanced
fileAccess: look-only
giveUpAfterMinutes: 20
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Read the ground.
";

const DECIDER_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000e2
name: Decider
summary: Picks what to do first
color: clay
runsWith: claude-code
model: sonnet
thinking: balanced
fileAccess: work-freely
writeResultsTo: \"\"
giveUpAfterMinutes: 20
tools: everything
skills: []
connections: []
---
Pick what to do first.
";

/// Dwa kroki i jedna strzałka, pisane ręcznie.
///
/// Fikstura zbudowana naszym serializatorem definiowałaby kształt, zamiast go sprawdzać: zmiana
/// kształtu przechodziłaby wtedy po obu stronach naraz [04 §6.4].
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_scout_then_open",
  "name": "Scout then open the attachment",
  "steps": [
    {
      "kind": "agent",
      "id": "s_scout",
      "name": "Scout",
      "agent": "01990000-0000-7000-8000-0000000000e1",
      "overrides": {},
      "instructions": "Say more than a handoff body can hold.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_decider",
      "name": "Decider",
      "agent": "01990000-0000-7000-8000-0000000000e2",
      "overrides": {},
      "instructions": "Open what the first step left behind.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    }
  ],
  "links": [{ "from": "s_scout", "to": "s_decider" }]
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_next_step_may_open_the_attachment_its_handoff_points_at() -> Result<(), Box<dyn Error>>
{
    // `_bench` żyje do końca funkcji: to w jego katalogach leży bieg, a `TempDir` kasuje je
    // w `Drop`.
    let (report, seen, _bench) = scout_then_open().await?;
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded, StepState::Succeeded],
        "both steps have to run for the second one's reading rights to exist; they ended as {:?}",
        report.steps
    );

    // PRZESŁANKA, nie kryterium: bez ucięcia nie ma załącznika, nie ma wskaźnika i nie ma o co
    // pytać — a asercja niżej wyglądałaby wtedy dokładnie tak samo jak spełniona.
    let attachments = report.dir.join(ATTACHMENTS_DIR);
    let full: Vec<PathBuf> = fs::read_dir(&attachments)
        .map_err(|error| {
            format!(
                "the scout's answer was supposed to be longer than BODY_CAP ({BODY_CAP} B), so \
                 {} had to exist: {error}",
                attachments.display()
            )
        })?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    assert!(
        !full.is_empty(),
        "the run wrote no attachment, so no handoff carried a pointer and this criterion would \
         pass on an empty premise"
    );

    // Wskaźnik naprawdę stoi w ciele przekazania. Gdyby go tam nie było, prawo do czytania
    // katalogu nie byłoby niczego warte, a test dalej by przechodził.
    let handoffs = report.dir.join("handoffs");
    let bodies: String = fs::read_dir(&handoffs)?
        .filter_map(Result::ok)
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .collect();
    assert!(
        bodies.contains(ATTACHMENTS_DIR),
        "no handoff body points at {ATTACHMENTS_DIR}/, so there is no pointer whose openability \
         this criterion could judge"
    );

    // SĄDZONE: krok, który dostał ten wskaźnik, ma prawo otworzyć plik, do którego on prowadzi.
    let dirs = seen.dirs_of(DECIDER).ok_or(
        "the second step never reached the driver, so it has no reading rights to judge — and \
         the arrow from the first step is the only thing that orders them",
    )?;
    assert!(
        dirs.iter().any(|dir| dir == &attachments),
        "the handoff tells the step to open {}, and the step was given reading rights to {:?} \
         only — so the only route to the sentence the cap took is closed (invariant 16)",
        attachments.display(),
        dirs
    );

    Ok(())
}

/// Bieg fikstury: zwiadowca mówi więcej, niż mieści ciało przekazania, i oddaje pole następnemu.
async fn scout_then_open() -> Result<(RunReport, Arc<Seen>, Bench), Box<dyn Error>> {
    let bench = Bench::new()?;
    let scout = bench.agent("scout", SCOUT_FILE)?;
    let decider = bench.agent("decider", DECIDER_FILE)?;
    let workflow = bench.workflow("scout-then-open", WORKFLOW)?;
    the_fixture_can_run(&workflow, &[&scout, &decider])?;
    let store = Store::open(&bench.db())?;

    let seen = Arc::new(Seen::default());
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&seen)),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 2,
        task: None,
    };

    // Linie tego kryterium nie interesują: sądzi ono `RunSpec`, który przeszedł do sterownika.
    let (lines, _source) = line_channel(QUEUE_CAP);
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, lines))
        .await
        .map_err(|_| format!("the run did not finish within {PATIENCE:?}"))??;

    Ok((report, seen, bench))
}

/// Fikstura ma przejść walidator bez ani jednego problemu, a odpowiedź zwiadowcy ma naprawdę
/// przekraczać [`BODY_CAP`].
///
/// Czerwień w fazie kontraktu wygląda identycznie dla „zachowania jeszcze nie ma" i dla „tego
/// kryterium nie da się spełnić nigdy": fikstura odrzucona przez `workflow::check` byłaby odmową
/// w KAŻDEJ implementacji, a odpowiedź krótsza od capa nie utworzyłaby załącznika w żadnej.
fn the_fixture_can_run(workflow: &Path, agents: &[&Path]) -> Result<(), Box<dyn Error>> {
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
    let reply = scout_reply();
    assert!(
        reply.len() > BODY_CAP,
        "the scout's answer is {} B and the cap is {BODY_CAP} B, so nothing would be moved to \
         {ATTACHMENTS_DIR}/ and this criterion would judge a pointer that was never written",
        reply.len()
    );
    for agent in agents {
        read_agent_file(agent).map_err(|error| format!("{}: {error}", agent.display()))?;
    }
    Ok(())
}

/// Odpowiedź zwiadowcy: proza dłuższa niż [`BODY_CAP`], numerowana wierszami.
///
/// Numer w wierszu jest tu po to, żeby cięcie było widoczne w razie awarii testu: dwa identyczne
/// wiersze nie powiedziałyby, gdzie ciało się urwało.
fn scout_reply() -> String {
    let mut out = String::with_capacity(BODY_CAP * 2);
    let mut line = 0;
    while out.len() <= BODY_CAP + 512 {
        line += 1;
        let _ = writeln!(out, "{line}. {SENTENCE}");
    }
    out
}

/// Biblioteka użytkownika i projekt na czas jednego kryterium.
#[derive(Debug)]
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
        Ok(Self { home, project })
    }

    fn agent(&self, slug: &str, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.home.path().join("agents").join(format!("{slug}.md"));
        fs::write(&path, text)?;
        Ok(path)
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

/// Katalogi, które naprawdę dojechały do sterownika, po jednym wpisie na krok.
///
/// **Ten zamek nigdy nie przechodzi przez `await`** (niezmiennik 8): cały dostęp jest zamknięty
/// w synchronicznych metodach, więc nie ma wyrażenia, w którym guard dożyłby do punktu
/// zawieszenia.
#[derive(Debug, Default)]
struct Seen {
    dirs: Mutex<Vec<(String, Vec<PathBuf>)>>,
}

impl Seen {
    /// Zapisuje prawa do czytania, z którymi krok ruszył. Synchroniczne z rozmysłem.
    fn saw(&self, step: &str, dirs: &[PathBuf]) {
        let entry = (step.to_owned(), dirs.to_vec());
        // Zatruty zamek nie może zgubić wpisu: panika w jednym kroku nie ma prawa oślepić
        // pomiaru, który dotyczy drugiego.
        match self.dirs.lock() {
            Ok(mut dirs) => dirs.push(entry),
            Err(poisoned) => poisoned.into_inner().push(entry),
        }
    }

    /// Katalogi kroku o tym identyfikatorze, jeśli krok w ogóle ruszył.
    fn dirs_of(&self, step: &str) -> Option<Vec<PathBuf>> {
        let dirs = match self.dirs.lock() {
            Ok(dirs) => dirs.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        dirs.into_iter()
            .find(|(key, _)| key == step)
            .map(|(_, dirs)| dirs)
    }
}

/// Fabryka, która dla każdego vendora oddaje ten sam dubler.
fn fake_drivers(seen: Arc<Seen>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { seen });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Który krok właśnie ruszył — po katalogu roboczym, nie po treści promptu.
///
/// Każdy krok fikstury ma `fresh-copy`, więc jego `cwd` to `<katalog biegu>/work/<id kroku>`
/// (`commands::run::workspace`).
fn step_of(cwd: &Path) -> &str {
    cwd.file_name().and_then(|name| name.to_str()).unwrap_or("")
}

/// Co ten krok oddaje jako wynik tury.
fn reply_of(step: &str) -> String {
    if step == SCOUT {
        scout_reply()
    } else {
        DECIDER_REPLY.to_owned()
    }
}

/// Dubler sterownika: zapisuje katalogi kroku, oddaje jego odpowiedź i wychodzi zerem.
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
        let step = step_of(&spec.cwd);
        // Zapis PRZED pierwszym zdarzeniem: prawa do czytania są tym, co ten krok dostał na
        // wejściu, a nie tym, co z niego wynikło.
        self.seen.saw(step, &spec.extra_dirs);

        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };
        let reply = reply_of(step);

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
        let _ = events
            .send(
                (AgentEvent::Said {
                    text: reply.clone(),
                })
                .into(),
            )
            .await;

        Ok(Box::new(Turn {
            events,
            session,
            reply,
        }))
    }
}

/// Jedna tura dublera.
#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    reply: String,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        // Dubler nie ma procesu, więc nie ma grupy. Zmyślony `pgid` byłby liczbą, po której
        // sprzątanie z T-20 strzelałoby w cudzy proces.
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
            text: self.reply.clone(),
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
