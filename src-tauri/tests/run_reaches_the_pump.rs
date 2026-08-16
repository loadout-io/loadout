//! AC-1 dla T-30: każda linia biegu dochodzi do pompy, w kolejności i bez gubienia.
//!
//! To jest kryterium, dla którego całe zadanie istnieje. Dwa końce pompy stoją dziś obok siebie
//! i nie pasują typami: `run_workflow_inner` wypuszczał `Vec<Line>` na zwykły `mpsc`, a
//! `LineSink::send` bierze **pojedynczą** `Line` (`ipc.rs`) — więc pompa 16 ms / 2000 linii jest
//! kodem wołanym wyłącznie z testów pompy. Ten plik puszcza prawdziwy bieg przez prawdziwą
//! `run_workflow_inner` z prawdziwą pompą po drugiej stronie i pyta o jedno: czy przeżyła
//! **każda** linia.
//!
//! # Słaba wersja tego kryterium: „coś dotarło"
//!
//! `assert!(!delivered.is_empty())` przechodzi na moście, który pod obciążeniem gubi paczki —
//! czyli w jedynym warunku, dla którego ta pompa w ogóle powstała (agent robiący
//! `find /usr/share` sypie 121 000 linii/s [T2 §6.1]). Rozróżniają je trzy rzeczy naraz:
//!
//! 1. **równość liczników**: pompa oddała dokładnie [`LINES`], nie „co najmniej" i nie „mniej
//!    więcej";
//! 2. **bilans**: `delivered + dropped` domyka się co do sztuki wobec tego, co producent oddał.
//!    Most, który gubi po cichu, ma tu dziurę; most, który liczy, ale gubi, pada na (1).
//!    Ta jedna liczba wraca **jedną** drogą — z [`PumpStats`], nie z trzech liczników prawie
//!    się zgadzających (niezmiennik 13);
//! 3. **treść i kolejność**: sklejone paczki są porównywane z ciągiem oczekiwanym **bez
//!    sortowania** i co do znaku. Posortowanie skasowałoby dokładnie tę własność, którą tu
//!    mierzymy — kolejność jest połową powodu, dla którego ta granica używa `Channel`, a nie
//!    systemu zdarzeń [T8 §5.2].
//!
//! # Dlaczego dubler emituje wyłącznie `Said` i ani jednego zdarzenia więcej
//!
//! Kurator zamienia `Said` **jeden do jednego** w `Line::Note` (`engine::line`), a każde inne
//! zdarzenie dokłada albo zabiera wiersz: `Started` nie daje żadnego, `Finished` daje `Done`.
//! Liczba w kryterium jest dokładna, więc dubler ma produkować dokładnie tyle linii, ile ta
//! liczba mówi. Fikstura dosypująca wiersz „na wszelki wypadek" zmusiłaby asercję do postaci
//! „co najmniej 300" albo do filtrowania — czyli wprost do tej słabej wersji, której to
//! kryterium ma nie przepuścić.
//!
//! Kolejka do pompy jest tu **przestronna** ([`ROOMY`]) i to też jest wybór, nie wygoda:
//! przepełnienie kolejki ma własne kryterium w T-07 (`ipc_pump_backpressure.rs`), a tutaj
//! przedmiotem pomiaru jest szew. Pojemność jest argumentem `line_channel` właśnie po to, żeby
//! te dwa pytania dały się zadać osobno.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::anyhow;
use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, FinishReason, Outcome as TurnOutcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{PumpStats, line_channel, spawn_pump};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use serde_json::Value as Json;
use tauri::ipc::{Channel, InvokeResponseBody};
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Ile linii produkuje bieg. Liczba z kryterium, nie z gustu — i dokładna, bo obie asercje
/// liczbowe są równościami.
const LINES: u64 = 300;

/// Nazwa kroku, czyli etykieta wiersza: `forward` podaje kuratorowi nazwę kafelka, a ta jedzie
/// na ekran jako `Line::agent`.
const STEP: &str = "Scribe";

/// Pojemność kolejki producent → pompa. Z zapasem, bo przedmiotem pomiaru jest szew, a nie
/// przepełnienie (tamto ma własne kryterium w T-07).
const ROOMY: usize = 8_192;

/// Ile czekamy, zanim uznamy bieg za zawieszony. Bieg, który wisi, jest dla bramki „nie
/// uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(20);

const SCRIBE_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000c1
name: Scribe
summary: Says a lot
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
Say a lot.
";

/// Jeden krok agenta i ani jednego więcej: kafelek kontrolny dokłada wiersz `asked`, a liczba
/// linii w tym kryterium jest dokładna.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_three_hundred_lines",
  "name": "Three hundred lines",
  "steps": [
    {
      "kind": "agent",
      "id": "s_scribe",
      "name": "Scribe",
      "agent": "01990000-0000-7000-8000-0000000000c1",
      "overrides": {},
      "instructions": "say a lot",
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_line_of_the_run_reaches_the_pump_in_order() -> Result<(), Box<dyn Error>> {
    let (report, stats, delivered) = three_hundred_lines().await?;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded],
        "the step has to finish for its lines to mean anything; it ended as {:?}",
        report.steps
    );

    // (a) Pompa oddała DOKŁADNIE tyle linii, ile bieg wyprodukował.
    assert_eq!(
        stats.delivered, LINES,
        "the pump delivered {} of the {LINES} lines the run produced. \"Some of them arrived\" \
         is the assertion this criterion exists to refuse: a bridge that drops packets under \
         load passes it, and load is the only condition this pump was built for",
        stats.delivered
    );

    // (b) Bilans domyka się co do sztuki. To jest ta asercja, którą łamie most gubiący po cichu.
    assert_eq!(
        stats.delivered + stats.dropped,
        LINES,
        "the pump's books do not close: {} delivered plus {} dropped is not {LINES}. Every line \
         the run handed over is either delivered or counted as lost — a line that falls out of \
         both numbers at once is exactly the line nobody ever notices (invariant 13)",
        stats.delivered,
        stats.dropped
    );

    // (c) Ta sama treść, w tej samej kolejności, bez sortowania.
    let wanted: Vec<Json> = (1..=LINES)
        .map(|number| serde_json::to_value(line(number)))
        .collect::<Result<Vec<Json>, _>>()?;
    assert_eq!(
        delivered.len(),
        wanted.len(),
        "gluing the batches back together gives {} rows against {} sent",
        delivered.len(),
        wanted.len()
    );
    assert_eq!(
        delivered, wanted,
        "the lines came out of the pump changed, reordered or repeated. They are compared \
         WITHOUT SORTING and character for character: sorting first would erase the one property \
         being measured, and comparing presence instead of content would pass over a bridge that \
         delivers the right number of the wrong rows"
    );
    Ok(())
}

/// Wiersz numer `number`, taki, jaki ma wyjść z kuratora dla `AgentEvent::Said`.
fn line(number: u64) -> Line {
    Line::Note {
        agent: STEP.to_owned(),
        text: number.to_string(),
    }
}

/// Jeden bieg fikstury: raport, bilans pompy i wiersze, które NAPRAWDĘ wyszły kanałem.
async fn three_hundred_lines() -> Result<(RunReport, PumpStats, Vec<Json>), Box<dyn Error>> {
    let bench = Bench::new()?;
    let scribe = bench.agent("scribe", SCRIBE_FILE)?;
    let workflow = bench.workflow("three-hundred-lines", WORKFLOW)?;
    the_fixture_can_run(&workflow, &[&scribe])?;
    let store = Store::open(&bench.db())?;

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(LINES),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
    };

    // Prawdziwy szew: bieg pisze do `LineSink`, pompa czyta z `LineSource` i wypycha paczki
    // kanałem. Nic po drodze nie jest atrapą poza samym oknem, którego w teście nie ma.
    let recorder = Delivered::default();
    let (sink, source) = line_channel(ROOMY);
    let pump = spawn_pump(source, recorder.channel());

    // `sink` wjeżdża do biegu i ginie razem z jego powrotem — dopiero wtedy pompa widzi koniec
    // producenta, wypycha ostatnią, niepełną paczkę i oddaje bilans. Pompy nie wolno zabijać
    // z zewnątrz: bilans jest kompletny wyłącznie w chwili, w której kończy się ona sama.
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| format!("the run did not finish within {PATIENCE:?}"))??;
    let stats = tokio::time::timeout(PATIENCE, pump)
        .await
        .map_err(|_| format!("the pump did not finish within {PATIENCE:?}"))??;

    Ok((report, stats, recorder.lines()?))
}

/// Paczki, które **naprawdę wyszły kanałem**, w kolejności wyjścia.
///
/// Nagrywamy po stronie okna, a nie po stronie kolejki: to, co bieg oddał `LineSink`, mówi
/// wyłącznie o intencji, a pytanie brzmi „co dojechało".
#[derive(Debug, Clone, Default)]
struct Delivered(Arc<Mutex<Vec<InvokeResponseBody>>>);

impl Delivered {
    /// Kanał, który pompa dostanie zamiast webviewa.
    fn channel(&self) -> Channel<Vec<Line>> {
        let seen = Arc::clone(&self.0);
        Channel::new(move |body| {
            // `std::sync::Mutex` w domknięciu SYNCHRONICZNYM: nie ma tu `await`, więc
            // niezmiennik 8 stoi z konstrukcji, a nie z uwagi w komentarzu.
            if let Ok(mut seen) = seen.lock() {
                seen.push(body);
            }
            Ok(())
        })
    }

    /// Wszystkie dostarczone wiersze, sklejone z paczek, w kolejności wyjścia.
    fn lines(&self) -> Result<Vec<Json>, Box<dyn Error>> {
        let seen = self
            .0
            .lock()
            .map_err(|error| anyhow!("the recorder was poisoned: {error}"))?;
        let mut out = Vec::new();
        for body in seen.iter().cloned() {
            out.extend(body.deserialize::<Vec<Json>>()?);
        }
        Ok(out)
    }
}

/// Fikstura ma przejść walidator **bez ani jednego problemu**, a jej plik agenta ma dać się
/// przeczytać.
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
    for agent in agents {
        read_agent_file(agent).map_err(|error| format!("{}: {error}", agent.display()))?;
    }
    Ok(())
}

/// Biblioteka użytkownika i projekt na czas jednego kryterium.
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

/// Fabryka, która dla każdego vendora oddaje ten sam dubler.
fn fake_drivers(lines: u64) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { lines });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler sterownika: `lines` wypowiedzi i ani jednego innego zdarzenia.
#[derive(Debug)]
struct Fake {
    /// Ile linii ma wyprodukować krok.
    lines: u64,
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
        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };

        // Numer jedzie w treści, bo `Line` nie ma pola sekwencji — a numer jest tu jedyną
        // treścią, która ma znaczenie. Odmowa kanału jest błędem STARTU, nie cichym „wysłano
        // mniej": dubler, który po cichu emituje 297 linii, zamieniłby to kryterium
        // w porównanie dwóch tych samych pomyłek.
        for number in 1..=self.lines {
            events
                .send(AgentEvent::Said {
                    text: number.to_string(),
                })
                .await
                .map_err(|_| anyhow!("the curator stopped listening after {} lines", number - 1))?;
        }

        Ok(Box::new(Turn { session }))
    }
}

/// Jedna tura dublera. Cała treść wyszła już w `start`, więc tura tylko się kończy.
#[derive(Debug)]
struct Turn {
    session: SessionRef,
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
        // Bez `AgentEvent::Finished`: to zdarzenie dołożyłoby wiersz `done`, czyli 301 linię
        // w kryterium, którego obie asercje liczbowe są równościami. Powód w nagłówku pliku.
        Ok(TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: String::new(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::ZERO,
            session: self.session.clone(),
        })
    }

    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}
