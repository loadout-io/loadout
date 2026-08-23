//! AC-4 dla T-97: krok Codeksa raportuje tokeny, a koszt — dopiero kiedy go zna.
//!
//! # Po co to istnieje
//!
//! `codex exec --json` zamyka turę linią `turn.completed`, a ta niesie `usage` z trzema
//! liczbami. Dekoder czytał je od T-10 i wkładał do [`Outcome::tokens`] — po czym wiersz
//! zamykający je **wyrzucał**, bo `Line::Done` ich nie miało. Na ekranie zostawał więc krok
//! bez ani jednej cyfry: `cost_usd` u Codeksa jest `None` (i ma być, bo vendor kwoty nie podaje),
//! a tokeny, które podał, nie docierały nigdzie. Budżet z T-94 liczy takie kroki jako zero.
//!
//! # Czego to kryterium NIE kupuje
//!
//! Cennika. `cost_usd` zostaje `None`, dopóki vendor nie poda kwoty — przeliczenie tokenów na
//! dolary po tabeli wpisanej w kod jest trzecim miejscem, w którym trzeba by ją aktualizować,
//! a jedyne, co widać, kiedy się zdezaktualizuje, to rachunek, który się nie zgadza. `Some(0.0)`
//! jest jeszcze gorsze: wypisuje `$0.00` i uczy człowieka, że Codex jest darmowy.
//!
//! # Trzy asercje, bo dwie kłamią
//!
//! (a) Liczby z drutu docierają do wiersza zamykającego **co do sztuki**. Nie „są niezerowe":
//!     wiersz z przepisaną nie tą liczbą wygląda dokładnie tak samo jak wiersz poprawny.
//!
//! (b) I docierają NA DRUT pod kluczami, które zna okno. To nie jest ta sama asercja: pole
//!     dopisane bez `rename_all_fields` jedzie na front pod nazwą, której on nie zna, widok
//!     dostaje `undefined` i pierwsze sześć poprawek idzie w złą warstwę [00-SYNTHESIS §3].
//!
//! (c) A koszt zostaje pusty. Bez tej asercji zieleń przechodzi dla implementacji, która przy
//!     okazji dopisała cennik — czyli dla tej, która na ekranie pokazuje kwotę, jakiej nikt
//!     nigdy nie zapłacił.
//!
//! # Słaba wersja tego kryterium
//!
//! `assert!(input > 0)`. Przechodzi dla implementacji, która wstawia w to pole **cokolwiek** —
//! na przykład długość promptu albo liczbę tur. Rozróżnia to równość z liczbą stojącą w linii
//! `turn.completed`, wpisaną w tym pliku raz i czytaną w obu asercjach.

// `expect()`/`unwrap()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam
// powód, co w pozostałych plikach tego celu.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::codex::CodexDecoder;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::line::{Curator, Line, LineKind, Seen};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use serde_json::Value;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Ile czekamy na bieg. Powód w całości przy tej samej stałej w `skills_reach_codex.rs`.
const PATIENCE: Duration = Duration::from_secs(20);

/// Trzy liczby stojące w `turn.completed`. Wpisane RAZ i czytane w każdej asercji — dwa razy
/// wpisana ta sama liczba to dwa miejsca, w których można się pomylić inaczej.
const INPUT: u64 = 24_763;
const CACHED: u64 = 24_448;
const OUTPUT: u64 = 122;

/// Kto pracuje.
const AGENT: &str = "builder";

/// Zamknięcie tury tak, jak wypisuje je `codex exec --json`. Kształt `usage` jest tym samym,
/// który czyta `CodexDecoder` od T-10 (`Usage`, `drivers/codex.rs`) i który stoi w `ITEMS`
/// w `driver_codex_stream.rs` — nie drugą jego kopią z wyobraźni.
fn closing_stream() -> String {
    format!(
        "{{\"type\":\"thread.started\",\"thread_id\":\"01a01b33-t97\"}}\n\
         {{\"type\":\"item.completed\",\"item\":{{\"type\":\"agent_message\",\"id\":\"item_0\",\
         \"text\":\"Renamed the widget.\"}}}}\n\
         {{\"type\":\"turn.completed\",\"usage\":{{\"input_tokens\":{INPUT},\
         \"cached_input_tokens\":{CACHED},\"output_tokens\":{OUTPUT}}}}}\n"
    )
}

/// Wiersz zamykający, zbudowany tą samą drogą, którą buduje go bieg: strumień → dekoder →
/// kurator. Nie `done_line` wołane wprost — ta funkcja jest prywatna, i słusznie: kryterium
/// pyta o to, co dochodzi na ekran, a nie o to, co umie jedna funkcja.
fn closing_row() -> Result<Line, Box<dyn Error>> {
    let stream = closing_stream();
    let mut decoder = CodexDecoder::new();
    let mut curator = Curator::new();
    let mut lines = Vec::new();

    for line in stream.lines() {
        for event in decoder.push(line) {
            lines.extend(curator.observe(Seen {
                agent: AGENT,
                at_ms: 0,
                event: &event,
                tool: None,
            }));
        }
    }
    lines.extend(curator.flush());

    lines
        .into_iter()
        .find(|line| line.kind() == LineKind::Done)
        .ok_or_else(|| {
            "the stream ends with turn.completed, so a closing row has to come out of \
                        it - and none did"
                .into()
        })
}

#[test]
fn the_closing_row_carries_the_tokens_the_vendor_reported() -> Result<(), Box<dyn Error>> {
    let row = closing_row()?;
    let Line::Done {
        input_tokens,
        output_tokens,
        cached_tokens,
        cost_usd,
        ..
    } = &row
    else {
        return Err(format!("the closing row is not a closing row: {row:?}").into());
    };

    // (a) CO DO SZTUKI. `> 0` przechodzi dla implementacji, która wpisuje tam długość promptu.
    assert_eq!(
        *input_tokens, INPUT,
        "the vendor said it read {INPUT} tokens of fresh input, and the row a person sees says \
         {input_tokens}. This row is the only thing the view ever receives, so what is not here \
         is nowhere"
    );
    assert_eq!(
        *output_tokens, OUTPUT,
        "the vendor said it wrote {OUTPUT} tokens, and the row says {output_tokens}"
    );
    assert_eq!(
        *cached_tokens, CACHED,
        "the vendor said {CACHED} tokens came from its cache, and the row says {cached_tokens}. \
         That one number is the whole answer to whether context isolation works at all"
    );

    // (c) I ANI JEDNEGO DOLARA. Codex kwoty nie podaje; policzona przez nas z tokenów byłaby
    //     kwotą, której nikt nie zapłacił, wyglądającą dokładnie jak kwota z rachunku.
    assert_eq!(
        *cost_usd, None,
        "Codex does not report what a turn cost, so this stays empty until it does. A price list \
         written into the code is a third place to keep up to date, and the only sign that it \
         fell behind is a bill that does not add up. It came out as {cost_usd:?}"
    );

    Ok(())
}

#[test]
fn the_tokens_reach_the_wire_under_the_names_the_window_reads() -> Result<(), Box<dyn Error>> {
    let row = closing_row()?;
    let wire = serde_json::to_value(&row)?;
    let fields = wire
        .as_object()
        .ok_or("a row goes out as a flat object, so the window gets one kind and one field")?;

    // Po drucie, nie po polu Rusta: bez `rename_all_fields` te trzy jadą jako `input_tokens`,
    // okno czyta `inputTokens`, dostaje `undefined` i wywraca widok — a przyczyna jest w derive,
    // nie w komponencie, więc pierwsze poprawki idą w złą warstwę [00-SYNTHESIS section 3].
    for (name, expected) in [
        ("inputTokens", INPUT),
        ("outputTokens", OUTPUT),
        ("cachedTokens", CACHED),
    ] {
        let carried = fields.get(name).and_then(Value::as_u64).ok_or_else(|| {
            format!(
                "the closing row has to carry {name} on the wire, or the strip has no number to \
                 show for a step whose vendor reports no price. The row went out as {wire}"
            )
        })?;
        assert_eq!(
            carried, expected,
            "{name} went out as {carried} and the vendor said {expected}"
        );
    }

    assert!(
        fields.get("costUsd").is_some_and(Value::is_null),
        "the cost stays empty on the wire too, and empty is not zero: $0.00 on a step that cost \
         unknown money teaches a person that this app is free. It went out as {wire}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_run_file_keeps_the_tokens_of_a_step_whose_vendor_reports_no_price()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let done = bench.one_run().await?;
    let report = done.map_err(|said| format!("the run did not finish: {said}"))?;
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded],
        "the step has to finish, or every assertion below is true of a step that never ran. It \
         ended as {:?}",
        report.steps
    );

    // `run.json`, nie indeks: pliki są prawdą, a `loadout.db` wolno skasować (niezmiennik 4).
    let book: Value = serde_json::from_str(&fs::read_to_string(report.dir.join("run.json"))?)?;
    let step = book
        .get("steps")
        .and_then(Value::as_array)
        .and_then(|steps| steps.first())
        .ok_or("run.json has no steps in it")?;

    // KLUCZE `run.json` SĄ TE, KTÓRE PISZE `StepEntry` — czyli po rustowemu, nie po drucie okna.
    // To są dwa różne artefakty i dwie różne umowy: drut nazywa pola tak, jak czyta je okno,
    // a ten plik tak, jak czyta go `store::rebuild` po skasowaniu indeksu (niezmiennik 4).
    for (name, expected) in [
        ("input_tokens", INPUT),
        ("output_tokens", OUTPUT),
        ("cached_tokens", CACHED),
    ] {
        let carried = step.get(name).and_then(Value::as_u64).ok_or_else(|| {
            format!(
                "run.json has to keep {name} for this step, or a run whose vendor reports no \
                 price counts as zero in every total anybody adds up later. The step reads as \
                 {step}"
            )
        })?;
        assert_eq!(carried, expected, "{name} was written as {carried}");
    }

    assert!(
        step.get("cost_usd").is_some_and(Value::is_null),
        "and the price stays empty, because this vendor never said one. The step reads as {step}"
    );

    Ok(())
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

const AGENT_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-00000000c974
name: Builder
summary: Does the work
color: moss
runsWith: codex
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

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_codex_tokens",
  "name": "One priced-in-tokens step",
  "steps": [
    {
      "kind": "agent",
      "id": "s_only",
      "name": "Renames the widget",
      "agent": "01990000-0000-7000-8000-00000000c974",
      "overrides": {},
      "instructions": "Renames the widget",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

fn counting_drivers() -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake);
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler vendora, który podaje tokeny i **nie podaje kwoty** — czyli dokładnie to, co robi Codex.
#[derive(Debug)]
struct Fake;

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        "fake"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("fake".to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let session = SessionRef {
            vendor: "fake",
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
            text: "Renamed the widget.".to_owned(),
            // Kwoty ten vendor nie podaje i to jest treść tego dubla.
            cost_usd: None,
            tokens: Tokens {
                input: INPUT,
                output: OUTPUT,
                cached: CACHED,
            },
            turns: 1,
            took: Duration::from_millis(6_200),
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
        fs::write(home.path().join("agents").join("builder.md"), AGENT_FILE)?;
        Ok(Self { home, project })
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }

    async fn one_run(
        &self,
    ) -> Result<Result<loadout_lib::commands::RunReport, String>, Box<dyn Error>> {
        let path = self.home.path().join("workflows").join("tokens.json");
        fs::write(&path, WORKFLOW)?;

        let store = Store::open(&self.db())?;
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store: &store,
            drivers: counting_drivers(),
            processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow: path,
            how_many_at_once: 2,
            task: None,
            part: None,
            handoffs_from: None,
        };

        let (sink, source) = line_channel(QUEUE_CAP);
        let pump = spawn_pump(source, Channel::new(|_| Ok(())));
        let outcome = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
            .await
            .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))?;
        let _ = tokio::time::timeout(PATIENCE, pump).await;

        Ok(outcome.map_err(|error| error.to_string()))
    }
}
