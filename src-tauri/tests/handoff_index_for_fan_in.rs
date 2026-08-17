//! AC-3 dla T-32: krok z dwoma poprzednikami dostaje indeks **obu**, w kolejności z grafu.
//!
//! To jest D6 punkt 4 — synteza wyników — sprowadzony do jednego pytania: czy krok syntezujący
//! w ogóle widzi, co ma zsyntetyzować. Zmierzone na wyładowanym trunku: prompt kroku to dosłownie
//! `step.instructions`, więc widzi zero z dwóch.
//!
//! **Słabą wersją jest „prompt wymienia jakieś przekazanie".** Przechodzi ją implementacja, która
//! gubi drugie przy wyścigu — czyli dokładnie ta, przy której synteza jest niepełna i nikt tego
//! nie widzi, bo krok kończy się sukcesem i mówi rzeczy prawdziwe o połowie materiału.
//!
//! Fikstura jest więc zbudowana tak, żeby **kolejność w grafie i kolejność zakończeń się nie
//! zgadzały**. `Alpha` stoi w pliku pierwsza, ma pierwszą strzałkę do `Gamma` i pracuje
//! [`SLOW`]; `Beta` stoi druga i pracuje [`FAST`]. Wszystko, co pochodzi z grafu — pozycja
//! w pliku, kolejność strzałek, numer kroku w nazwie pliku przekazania — mówi „alpha, potem
//! beta". Jedyne, co mówi odwrotnie, to chwila zakończenia. Implementacja, która dopisuje
//! przekazania do listy w miarę, jak kroki schodzą, pada tu i **ma paść**: przy trzech
//! poprzednikach jej kolejność zmienia się z biegu na bieg, a prompt, który dwa razy z rzędu
//! wygląda inaczej, jest promptem, którego nie da się z niczym porównać.
//!
//! Ta odwrotność jest przesłanką, nie asercją o produkcie, więc sprawdzamy ją osobno
//! ([`the_fixture_reversed_the_finish_order`]) i mówimy o niej wprost, kiedy nie wyszła.

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
    AgentDriver, AgentEvent, AgentHandle, FinishReason, Outcome as TurnOutcome, Probe, RunSpec,
    SessionRef, Tokens,
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

/// Tura kroku, który ma skończyć **pierwszy**, choć w grafie stoi drugi.
const FAST: Duration = Duration::from_millis(20);

/// Tura kroku, który w grafie stoi **pierwszy**, a kończy się jako drugi. Różnica rzędu
/// kilkunastu razy, żeby o kolejności zakończeń nie rozstrzygało obciążenie maszyny.
const SLOW: Duration = Duration::from_millis(300);

/// Ile czekamy na cały bieg, zanim uznamy go za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(20);

/// Identyfikatory kroków z pliku workflow — i ostatnie człony ich katalogów roboczych.
const ALPHA: &str = "s_alpha";
const BETA: &str = "s_beta";
const GAMMA: &str = "s_gamma";

/// Instrukcja kroku syntezującego, słowo w słowo z [`WORKFLOW`]. Zgodność obu miejsc sprawdza
/// [`the_fixture_can_run`].
const GAMMA_INSTRUCTIONS: &str = "Merge both lists into one report.";

/// Po tych zdaniach poznajemy, które przekazanie jest czyje. Po treści, nie po nazwie pliku:
/// nazwę składa implementacja i to kryterium nie ma powodu jej dyktować.
const ALPHA_MARKER: &str = "Four tables, two without a primary key";
const BETA_MARKER: &str = "Three migrations rewrite rows in place";

/// Odpowiedź kroku, który w grafie stoi pierwszy.
const ALPHA_REPLY: &str = "\
Four tables, two without a primary key. A row written twice cannot be told apart from a
row written once, so a rebuild after a crash can insert the same run a second time and
nothing anywhere says a word about it.
";

/// Odpowiedź kroku, który w grafie stoi drugi — i kończy się pierwszy.
const BETA_REPLY: &str = "\
Three migrations rewrite rows in place instead of adding columns, so an older build that
opens the same database reads values it cannot make sense of. The other nine are additive
and idempotent, and those I would leave alone.
";

/// Odpowiedź kroku syntezującego. Jego przekazania to kryterium nie ogląda — sądzi jego prompt.
const GAMMA_REPLY: &str = "\
One report, two halves: the missing keys first, because they let a duplicate through, then
the three migrations that rewrite rows.
";

const READER_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000f1
name: Reader
summary: Reads one thing and says what it found
color: moss
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
Read one thing and say what you found.
";

/// Dwa kroki bez strzałki między sobą i trzeci, który czeka na oba — kształt „fan-in" z T3 §3.1,
/// pisany ręcznie.
///
/// Fikstura zbudowana naszym serializatorem definiowałaby kształt, zamiast go sprawdzać: zmiana
/// kształtu przechodziłaby wtedy po obu stronach naraz [04 §6.4].
///
/// Kolejność strzałek jest ta sama, co kolejność kroków w pliku (`s_alpha` przed `s_beta`), i to
/// jest wybór na korzyść implementacji: każdy porządek wzięty z grafu — pozycja w pliku, lista
/// `depends_on`, numer kroku w nazwie pliku — daje tę samą odpowiedź, więc kryterium nie zmusza
/// do jednego konkretnego. Tylko kolejność zakończeń mówi coś innego.
///
/// Każdy krok pracuje na własnej kopii plików: dwa kroki, które mogą biec równocześnie
/// w folderze projektu, są odmową przy zapisie (niezmiennik 12), więc bez tego fikstura nie
/// doszłaby nawet do planisty. Po ostatnim członie `cwd` dubler poznaje przy okazji, który krok
/// właśnie ruszył.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_two_then_merge",
  "name": "Two reads then a merge",
  "steps": [
    {
      "kind": "agent",
      "id": "s_alpha",
      "name": "Alpha",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "List every table that has no primary key.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_beta",
      "name": "Beta",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "List every migration that rewrites rows.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 240 }
    },
    {
      "kind": "agent",
      "id": "s_gamma",
      "name": "Gamma",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "Merge both lists into one report.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 120 }
    }
  ],
  "links": [
    { "from": "s_alpha", "to": "s_gamma" },
    { "from": "s_beta", "to": "s_gamma" }
  ]
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_merging_step_sees_both_handoffs_in_graph_order() -> Result<(), Box<dyn Error>> {
    // `_bench` żyje do końca funkcji: to w jego katalogach leży bieg, a `TempDir` kasuje je
    // w `Drop`.
    let (report, seen, _bench) = two_reads_then_a_merge().await?;
    assert_eq!(
        report.steps,
        vec![
            StepState::Succeeded,
            StepState::Succeeded,
            StepState::Succeeded
        ],
        "all three steps have to run for the merge to have anything to see; they ended as {:?}",
        report.steps
    );
    the_fixture_reversed_the_finish_order(&seen);

    let handoffs = scan_run_dir(&report.dir)?;
    let alpha = the_handoff_carrying(&handoffs, ALPHA_MARKER, "Alpha", &report)?;
    let beta = the_handoff_carrying(&handoffs, BETA_MARKER, "Beta", &report)?;
    let prompt = seen.prompt_of(GAMMA).ok_or(
        "the merging step never reached the driver, so there is no prompt to judge — and it is \
         the only step in this fixture that has two predecessors",
    )?;

    both_handoffs_are_named(&prompt, alpha, beta);
    they_stand_in_graph_order(&prompt, alpha, beta, &seen);
    each_one_is_named_once(&prompt, alpha, beta);
    Ok(())
}

/// Przesłanka fikstury: kroki skończyły w kolejności **odwrotnej** do grafu.
///
/// Bez tego kryterium nie odróżnia „posortowane po grafie" od „dopisywane po kolei", bo obie
/// odpowiedzi wyglądają tak samo. To jest zdanie o fiksturze, nie o produkcie — dlatego mówi
/// wprost, że to ona nie wyszła.
fn the_fixture_reversed_the_finish_order(seen: &Seen) {
    let order = seen.finish_order();
    let first = order.first().map(String::as_str);
    assert_eq!(
        first,
        Some(BETA),
        "the fixture did not set up the race it needs: \"{BETA}\" runs {FAST:?} and \"{ALPHA}\" \
         runs {SLOW:?}, so the fast one has to finish first and this run ended in the order \
         {order:?}. Until the two orders disagree, listing the handoffs by finish time and \
         listing them by the graph look identical"
    );
}

/// (a) Prompt kroku syntezującego wymienia **oba** przekazania.
fn both_handoffs_are_named(prompt: &str, alpha: &Handoff, beta: &Handoff) {
    for (handoff, step) in [(alpha, "Alpha"), (beta, "Beta")] {
        let reference = reference_of(&handoff.path);
        assert!(
            prompt.contains(&reference),
            "the merging step is never told about the handoff from \"{step}\" ({reference}). A \
             step that synthesises what it cannot see writes a report about half the material and \
             finishes green (D6 point 4). The prompt reads:\n{prompt}"
        );
    }
}

/// (b) I wymienia je w kolejności z grafu, nie w kolejności zakończeń.
fn they_stand_in_graph_order(prompt: &str, alpha: &Handoff, beta: &Handoff, seen: &Seen) {
    let (first, second) = (reference_of(&alpha.path), reference_of(&beta.path));
    // Odnośnik, którego w prompcie nie ma, dostaje pozycję na końcu świata, a nie `None`:
    // `None < Some(_)` przechodzi tę asercję na przekazaniu, którego nie wymieniono wcale.
    let at_alpha = prompt.find(&first).unwrap_or(usize::MAX);
    let at_beta = prompt.find(&second).unwrap_or(usize::MAX);
    assert!(
        at_alpha < at_beta,
        "the index lists \"Beta\" before \"Alpha\", and the only thing that puts them in that \
         order is which one finished first ({:?}). Everything the graph says — the order of the \
         steps in the file, the order of the arrows into this step, the step number in the \
         handoff's own file name — says \"Alpha\" first. An order that comes from timing is an \
         order that changes between two runs of the same workflow. The prompt reads:\n{prompt}",
        seen.finish_order()
    );
}

/// (c) I każde dokładnie raz.
fn each_one_is_named_once(prompt: &str, alpha: &Handoff, beta: &Handoff) {
    for (handoff, step) in [(alpha, "Alpha"), (beta, "Beta")] {
        let reference = reference_of(&handoff.path);
        let times = prompt.matches(reference.as_str()).count();
        assert_eq!(
            times, 1,
            "the handoff from \"{step}\" ({reference}) is named {times} times in one prompt. \
             An index that repeats an entry makes the same work look like two pieces of work, \
             and the step pays for the difference in tokens. The prompt reads:\n{prompt}"
        );
    }
}

/// Przekazanie niosące to zdanie — po treści, nie po nazwie pliku ani pozycji na liście.
fn the_handoff_carrying<'a>(
    handoffs: &'a [Handoff],
    marker: &str,
    step: &str,
    report: &RunReport,
) -> Result<&'a Handoff, Box<dyn Error>> {
    assert!(
        !handoffs.is_empty(),
        "the run finished all three steps and left {}/handoffs/ empty, so the merging step had \
         nothing to be told about. AC-1 judges that a handoff is written at all; this one judges \
         what a step with two predecessors is given",
        report.dir.display()
    );

    let mine: Vec<&Handoff> = handoffs
        .iter()
        .filter(|handoff| handoff.body.contains(marker))
        .collect();
    match mine.as_slice() {
        [only] => Ok(*only),
        other => Err(format!(
            "exactly one handoff carries what \"{step}\" said, and {} do. The run left {:?}",
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
/// do promptu — rozstrzyga, że wpisała którąś, i w jakiej kolejności.
fn reference_of(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    format!("handoffs/{name}")
}

/// Jeden bieg fikstury: raport, to, co zobaczył dubler, i katalogi, które muszą to przeżyć.
async fn two_reads_then_a_merge() -> Result<(RunReport, Arc<Seen>, Bench), Box<dyn Error>> {
    let bench = Bench::new()?;
    let reader = bench.agent("reader", READER_FILE)?;
    let workflow = bench.workflow("two-reads-then-a-merge", WORKFLOW)?;
    the_fixture_can_run(&workflow, &[&reader])?;
    let store = Store::open(&bench.db())?;

    let seen = Arc::new(Seen::default());
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&seen)),
        control: RunControl::new(),
    };
    // Dwa naraz, bo obaj poprzednicy mają biec równocześnie — inaczej o kolejności zakończeń
    // rozstrzygałaby szerokość wysyłki, a nie czas ich pracy (niezmiennik 11).
    let request = RunRequest {
        workflow,
        how_many_at_once: 2,
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

/// Fikstura ma przejść walidator **bez ani jednego problemu**, jej plik agenta ma dać się
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
        WORKFLOW.contains(GAMMA_INSTRUCTIONS),
        "the fixture and GAMMA_INSTRUCTIONS drifted apart"
    );
    // Znacznik, którego nie ma w odpowiedzi, nie rozróżnia dwóch przekazań — dobiera po prostu
    // zero z dwóch, a wygląda przy tym jak sprawne wyszukanie. Proza zawija wiersze, więc fraza
    // rozcięta na dwie linie nie pasuje do niczego.
    assert!(
        ALPHA_REPLY.contains(ALPHA_MARKER) && BETA_REPLY.contains(BETA_MARKER),
        "each marker has to occur in the reply it is supposed to identify"
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

/// Co dubler zobaczył: prompt każdego kroku i kolejność, w jakiej kroki kończyły.
///
/// **Te zamki nigdy nie przechodzą przez `await`** (niezmiennik 8): cały dostęp jest zamknięty
/// w synchronicznych metodach, więc nie ma wyrażenia, w którym guard dożyłby do punktu
/// zawieszenia. `clippy::await_holding_lock` (deny) pilnuje reszty, ale sam w sobie jest siatką,
/// nie projektem.
#[derive(Debug, Default)]
struct Seen {
    prompts: Mutex<Vec<(String, String)>>,
    finished: Mutex<Vec<String>>,
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

    /// Odnotowuje, że tura tego kroku właśnie się skończyła.
    fn done(&self, step: &str) {
        match self.finished.lock() {
            Ok(mut finished) => finished.push(step.to_owned()),
            Err(poisoned) => poisoned.into_inner().push(step.to_owned()),
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

    /// Kroki w kolejności, w jakiej kończyły.
    fn finish_order(&self) -> Vec<String> {
        match self.finished.lock() {
            Ok(finished) => finished.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
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
/// prompt kroku syntezującego jest dokładnie tym, co to kryterium sądzi.
fn step_of(cwd: &Path) -> &str {
    cwd.file_name().and_then(|name| name.to_str()).unwrap_or("")
}

/// Ile trwa tura tego kroku. Powolny jest ten, który w grafie stoi pierwszy — to jest cała
/// odwrotność, na której stoi to kryterium.
fn turn_of(step: &str) -> Duration {
    if step == ALPHA { SLOW } else { FAST }
}

/// Co ten krok oddaje jako wynik tury.
fn reply_of(step: &str) -> &'static str {
    match step {
        ALPHA => ALPHA_REPLY,
        BETA => BETA_REPLY,
        _ => GAMMA_REPLY,
    }
}

/// Dubler sterownika: zapisuje prompt, pracuje tyle, ile mu przypisano, i wychodzi zerem.
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
        events: mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let step = step_of(&spec.cwd).to_owned();
        // Zapis PRZED pierwszym zdarzeniem: prompt jest tym, co ten krok dostał na wejściu,
        // a nie tym, co z niego wynikło.
        self.seen.saw(&step, &spec.prompt);

        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };
        let reply = reply_of(&step);
        let hold = turn_of(&step);

        let _ = events
            .send(AgentEvent::Started {
                session: session.clone(),
                model: spec.model.clone().unwrap_or_default(),
                tools: Vec::new(),
                capabilities: Vec::new(),
            })
            .await;
        // Ta sama treść dwiema drogami: jako proza w trakcie tury i jako `Outcome::text` na jej
        // końcu. Implementacja ma prawo wziąć przekazanie z każdej z nich i to kryterium nie
        // sądzi z której.
        let _ = events
            .send(AgentEvent::Said {
                text: reply.to_owned(),
            })
            .await;

        Ok(Box::new(Turn {
            events,
            session,
            seen: Arc::clone(&self.seen),
            step,
            reply,
            hold,
        }))
    }
}

/// Jedna tura dublera.
#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<AgentEvent>,
    session: SessionRef,
    seen: Arc<Seen>,
    step: String,
    reply: &'static str,
    /// Ile trwa ta tura. **Prawdziwy sen, nie czas wirtualny**: `start_paused` implikuje runtime
    /// jednowątkowy i przeskakuje zegar do przodu, kiedy runtime staje bezczynny, więc kolejność
    /// zakończeń przestałaby cokolwiek znaczyć [T7 §8.1].
    hold: Duration,
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
        tokio::time::sleep(self.hold).await;
        // Znacznik stawiamy tutaj, a nie w `close`: to jest chwila, w której krok oddał wynik,
        // czyli ta, po której implementacja miałaby zapisać jego przekazanie.
        self.seen.done(&self.step);

        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: self.reply.to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: self.hold,
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send(AgentEvent::Finished(outcome.clone()))
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
