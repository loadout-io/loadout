//! AC-2 dla T-32: prompt drugiego kroku niesie **odnośnik** do przekazania, nie jego treść.
//!
//! Zmierzone na wyładowanym trunku: prompt kroku to dosłownie `step.instructions`, więc drugi
//! krok nie wie ani tego, że pierwszy coś oddał, ani gdzie to leży. To kryterium sądzi drugą
//! połowę szwu z T-32 — pierwszą (czy plik w ogóle powstaje) sądzi AC-1.
//!
//! **Słabą wersją jest `assert_ne!(prompt, step.instructions)`.** Przechodzi ją implementacja,
//! która wkleja do promptu cały transkrypt poprzednika — a to jest dokładnie ta, przed którą stoi
//! D6 punkt 5: orchestrator ma dostać **indeks**, nie sklejone transkrypty, bo inaczej każdy krok
//! płaci tokenami za wszystko, co było przed nim, i przy czwartym kroku prompt jest większy niż
//! praca. Rozróżnia dopiero para: **jest** odnośnik i **nie ma** pełnej treści.
//!
//! Stąd dwa znaczniki w odpowiedzi zwiadowcy. [`SCOUT_MARKER`] otwiera ją, więc trafia do
//! każdego streszczenia i nie rozstrzyga niczego; [`DEEP_MARKER`] leży kilkaset znaków dalej,
//! więc nie mieści się w żadnej jednolinijkowej etykiecie kroku i pojawia się w prompcie
//! wyłącznie wtedy, gdy ktoś wkleił tam ciało.
//!
//! Dubler poznaje krok po **katalogu roboczym**, nie po treści promptu: prompt jest tu rzeczą
//! sądzoną, więc dobieranie po nim odpowiedzi agenta byłoby mierzeniem samego siebie.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
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
use loadout_lib::memory::handoff::{Handoff, scan_run_dir};
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

/// Identyfikator pierwszego kroku w pliku workflow — i ostatni człon jego katalogu roboczego.
const SCOUT: &str = "s_scout";
/// To samo dla drugiego kroku, czyli tego, którego prompt sądzi to kryterium.
const DECIDER: &str = "s_decider";

/// Instrukcja drugiego kroku, słowo w słowo z [`WORKFLOW`].
///
/// Zgodność obu miejsc sprawdza [`the_fixture_can_run`], bo stała i plik fikstury rozjeżdżają się
/// po cichu: asercja „instrukcja dalej jest w prompcie" przechodziłaby wtedy na tekście, którego
/// żaden krok nigdy nie dostał.
const DECIDER_INSTRUCTIONS: &str = "Decide which of the missing pieces to build first.";

/// Pierwsze zdanie odpowiedzi zwiadowcy. Mieści się w każdym streszczeniu, więc **nie**
/// rozstrzyga o niczym — stoi tu po to, żeby odróżnić jego przekazanie od innych.
const SCOUT_MARKER: &str = "Two of the four tables have no primary key";

/// Zdanie z końca tej samej odpowiedzi, kilkaset znaków za jej początkiem.
///
/// To jest cały dyskryminator tego kryterium: nie mieści się w jednolinijkowym podsumowaniu kroku
/// (`commands::run::summary_of`, 240 znaków), więc w prompcie następnego kroku może się znaleźć
/// wyłącznie przez wklejenie ciała przekazania.
const DEEP_MARKER: &str = "sessions.token has no unique index either";

/// Odpowiedź pierwszego kroku. Dwa znaczniki, daleko od siebie; reszta to zwykła proza, żeby
/// odległość między nimi była prawdziwa, a nie zrobiona spacjami.
const SCOUT_REPLY: &str = "\
Two of the four tables have no primary key, so a row written twice cannot be told apart
from a row written once. Both `runs` and `steps` declare their id as plain text, which is
why a rebuild after a crash can insert the same run a second time without anything
anywhere saying a word about it.

The migration itself is fine and I read every statement in it: it adds columns in place,
it drops nothing, and an older build that opens the same database keeps working. So the
work here is not a rewrite, it is one index and one constraint.

The part that is not fine is narrower and easier to miss.
sessions.token has no unique index either, so two sessions can carry one token and the
lookup returns whichever row the planner happened to visit first.
";

/// Odpowiedź drugiego kroku. Nie niesie żadnego ze znaczników pierwszego.
const DECIDER_REPLY: &str = "\
Start with the unique index on the token column. It is one statement, it is reversible,
and it closes the case where two sessions answer to one name.
";

const SCOUT_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000d1
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
id: 01990000-0000-7000-8000-0000000000d2
name: Decider
summary: Picks what to do first
color: clay
runsWith: claude-code
model: sonnet
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 20
writeResultsTo: \"\"
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
///
/// Własna kopia plików dla każdego kroku jest tu po to, żeby dubler poznawał krok po ostatnim
/// członie `cwd` (`commands::run::workspace`) zamiast po prompcie, który to kryterium sądzi.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_scout_then_decide",
  "name": "Scout then decide",
  "steps": [
    {
      "kind": "agent",
      "id": "s_scout",
      "name": "Scout",
      "agent": "01990000-0000-7000-8000-0000000000d1",
      "overrides": {},
      "instructions": "Look at the schema and say what is missing.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_decider",
      "name": "Decider",
      "agent": "01990000-0000-7000-8000-0000000000d2",
      "overrides": {},
      "instructions": "Decide which of the missing pieces to build first.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    }
  ],
  "links": [{ "from": "s_scout", "to": "s_decider" }]
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_second_prompt_points_at_the_handoff_instead_of_repeating_it()
-> Result<(), Box<dyn Error>> {
    // `_bench` żyje do końca funkcji: to w jego katalogach leży bieg, a `TempDir` kasuje je
    // w `Drop`.
    let (report, seen, _bench) = scout_then_decide().await?;
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded, StepState::Succeeded],
        "both steps have to run for the second one's prompt to exist; they ended as {:?}",
        report.steps
    );

    let handoffs = scan_run_dir(&report.dir)?;
    let scouts = the_scouts_handoff(&handoffs, &report)?;
    let prompt = seen.prompt_of(DECIDER).ok_or(
        "the second step never reached the driver, so there is no prompt to judge — and the \
         arrow from the first step is the only thing that orders them",
    )?;

    the_prompt_names_where_the_handoff_lies(&prompt, scouts);
    the_prompt_does_not_carry_the_handoff_itself(&prompt, scouts);
    the_step_still_gets_its_own_instruction(&prompt);
    Ok(())
}

/// (a) Prompt niesie **ścieżkę** przekazania pierwszego kroku.
fn the_prompt_names_where_the_handoff_lies(prompt: &str, scouts: &Handoff) {
    let reference = reference_of(&scouts.path);
    assert!(
        prompt.contains(&reference),
        "the prompt of the second step says nothing about where the first step's handoff lies. \
         Both forms count and both end with `{reference}` — the full path or one relative to the \
         run directory — so this asks only that the pointer is there. Without it the file exists \
         and reaches nobody, which is the state this task was written to end. The prompt \
         reads:\n{prompt}"
    );
}

/// (b) I **nie** niesie jego treści.
fn the_prompt_does_not_carry_the_handoff_itself(prompt: &str, scouts: &Handoff) {
    assert!(
        !prompt.contains(DEEP_MARKER),
        "the prompt carries the tail of the first step's answer, so this is not an index — it is \
         the transcript, pasted. That is the implementation D6 point 5 exists to rule out: every \
         step then pays tokens for everything before it, and by the fourth step the prompt is \
         larger than the work. The prompt reads:\n{prompt}"
    );

    let body = scouts.body.trim();
    assert!(
        !body.is_empty(),
        "the handoff on disk has an empty body, so \"the prompt does not repeat it\" would pass \
         on nothing at all"
    );
    assert!(
        !prompt.contains(body),
        "the whole body of {} was pasted into the next prompt",
        scouts.path.display()
    );
}

/// (c) A instrukcja samego kroku dalej w nim jest.
fn the_step_still_gets_its_own_instruction(prompt: &str) {
    assert!(
        prompt.contains(DECIDER_INSTRUCTIONS),
        "the step's own instruction is gone from its prompt. A prompt built out of handoffs and \
         nothing else hands the agent everybody else's work and never says what to do with it. \
         The prompt reads:\n{prompt}"
    );
}

/// Przekazanie niosące to, co oddał pierwszy krok — po treści, nie po pozycji na liście.
fn the_scouts_handoff<'a>(
    handoffs: &'a [Handoff],
    report: &RunReport,
) -> Result<&'a Handoff, Box<dyn Error>> {
    assert!(
        !handoffs.is_empty(),
        "the run finished both steps and left {}/handoffs/ empty, so there is no pointer for the \
         second prompt to carry. AC-1 judges that half; this one judges what the next step is \
         told about it",
        report.dir.display()
    );

    let mine: Vec<&Handoff> = handoffs
        .iter()
        .filter(|handoff| handoff.body.contains(SCOUT_MARKER))
        .collect();
    match mine.as_slice() {
        [only] => Ok(*only),
        other => Err(format!(
            "exactly one handoff carries what \"Scout\" said, and {} do. The run left {:?}",
            other.len(),
            handoffs
                .iter()
                .map(|handoff| handoff.path.display().to_string())
                .collect::<Vec<_>>()
        )
        .into()),
    }
}

/// Ostatnie dwa człony ścieżki przekazania: `handoffs/<nazwa pliku>`.
///
/// Tyle wystarczy i ani znaku więcej: ten napis jest końcówką ścieżki bezwzględnej **i** ścieżki
/// względem katalogu biegu, więc kryterium nie rozstrzyga za implementację, którą z nich wpisać
/// do promptu — rozstrzyga, że wpisała którąś.
fn reference_of(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    format!("handoffs/{name}")
}

/// Jeden bieg fikstury: raport, prompty widziane przez dubler i katalogi, które muszą je przeżyć.
async fn scout_then_decide() -> Result<(RunReport, Arc<Seen>, Bench), Box<dyn Error>> {
    let bench = Bench::new()?;
    let scout = bench.agent("scout", SCOUT_FILE)?;
    let decider = bench.agent("decider", DECIDER_FILE)?;
    let workflow = bench.workflow("scout-then-decide", WORKFLOW)?;
    the_fixture_can_run(&workflow, &[&scout, &decider])?;
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

    // Linie tego kryterium nie interesują: sądzi ono prompt, który przeszedł do sterownika.
    // Odbiornik zostaje przy życiu, bo `LineSink::send` robi `try_send` i pełna kolejka jest dla
    // biegu tym samym co brak okna — porzuconą linią, nigdy czekaniem (`ipc::LineSink`).
    let (lines, _source) = line_channel(QUEUE_CAP);
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, lines))
        .await
        .map_err(|_| format!("the run did not finish within {PATIENCE:?}"))??;

    Ok((report, seen, bench))
}

/// Fikstura ma przejść walidator **bez ani jednego problemu**, jej pliki agentów mają dać się
/// przeczytać, a stała z instrukcją ma stać w pliku workflow słowo w słowo.
///
/// To nie jest część kryterium, tylko jego przesłanka, i dlatego stoi przed biegiem. Czerwień
/// w fazie kontraktu wygląda identycznie dla „zachowania jeszcze nie ma" i dla „tego kryterium
/// nie da się spełnić nigdy": workflow, który `workflow::check` odrzuca, byłby odmową w KAŻDEJ
/// implementacji, a test nazywałby to brakiem zachowania.
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
    assert!(
        WORKFLOW.contains(DECIDER_INSTRUCTIONS),
        "the fixture and DECIDER_INSTRUCTIONS drifted apart, so \"the instruction survives\" \
         would be asserted about a sentence no step was ever given"
    );
    // Znacznik, którego nie ma w odpowiedzi, zamienia asercję „prompt tego nie niesie" w zdanie
    // o niczym — i wygląda przy tym dokładnie tak, jak asercja spełniona. Zmierzone tu
    // 2026-08-17: proza zawija wiersze, więc fraza rozcięta na dwie linie nie pasuje do niczego.
    let deep_at = SCOUT_REPLY.find(DEEP_MARKER);
    assert!(
        SCOUT_REPLY.contains(SCOUT_MARKER) && deep_at.is_some_and(|at| at > 240),
        "both markers have to occur in the reply, and DEEP_MARKER has to sit past the 240 \
         characters a one-line step summary can hold — otherwise an implementation that puts a \
         short title in the index would fail this criterion for the wrong reason"
    );
    for agent in agents {
        read_agent_file(agent).map_err(|error| format!("{}: {error}", agent.display()))?;
    }
    Ok(())
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

/// Prompt, który naprawdę dojechał do sterownika, po jednym na krok.
///
/// **Ten zamek nigdy nie przechodzi przez `await`** (niezmiennik 8): cały dostęp jest zamknięty
/// w synchronicznych metodach, więc nie ma wyrażenia, w którym guard dożyłby do punktu
/// zawieszenia. `clippy::await_holding_lock` (deny) pilnuje reszty, ale sam w sobie jest siatką,
/// nie projektem.
#[derive(Debug, Default)]
struct Seen {
    prompts: Mutex<Vec<(String, String)>>,
}

impl Seen {
    /// Zapisuje prompt kroku. Synchroniczne z rozmysłem — patrz nagłówek typu.
    fn saw(&self, step: &str, prompt: &str) {
        let entry = (step.to_owned(), prompt.to_owned());
        // Zatruty zamek nie może zgubić wpisu: panika w jednym kroku nie ma prawa oślepić
        // pomiaru, który dotyczy drugiego.
        match self.prompts.lock() {
            Ok(mut prompts) => prompts.push(entry),
            Err(poisoned) => poisoned.into_inner().push(entry),
        }
    }

    /// Prompt kroku o tym identyfikatorze, jeśli krok w ogóle ruszył.
    fn prompt_of(&self, step: &str) -> Option<String> {
        let prompts = match self.prompts.lock() {
            Ok(prompts) => prompts.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        prompts
            .into_iter()
            .find(|(key, _)| key == step)
            .map(|(_, prompt)| prompt)
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
/// (`commands::run::workspace`). Rozpoznawanie po prompcie byłoby tu mierzeniem samego siebie:
/// prompt jest dokładnie tym, co to kryterium sądzi.
fn step_of(cwd: &Path) -> &str {
    cwd.file_name().and_then(|name| name.to_str()).unwrap_or("")
}

/// Co ten krok oddaje jako wynik tury.
fn reply_of(step: &str) -> &'static str {
    if step == SCOUT {
        SCOUT_REPLY
    } else {
        DECIDER_REPLY
    }
}

/// Dubler sterownika: zapisuje prompt, oddaje odpowiedź kroku i wychodzi zerem.
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
        // Zapis PRZED pierwszym zdarzeniem: prompt jest tym, co ten krok dostał na wejściu,
        // a nie tym, co z niego wynikło.
        self.seen.saw(step, &spec.prompt);

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
        // Ta sama treść dwiema drogami: jako proza w trakcie tury i jako `Outcome::text` na jej
        // końcu. Implementacja ma prawo wziąć przekazanie z każdej z nich i to kryterium nie
        // sądzi z której.
        let _ = events
            .send(
                (AgentEvent::Said {
                    text: reply.to_owned(),
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
    reply: &'static str,
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
            text: self.reply.to_owned(),
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
