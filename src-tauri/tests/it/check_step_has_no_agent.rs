//! AC-4 dla T-55: krok „sprawdź" nie tworzy sesji agenta — ani modelu, ani promptu, ani jednego
//! wywołania.
//!
//! # Cicha porażka, przed którą to stoi
//!
//! Zrobić z etapu sprawdzenia krok AGENTA o instrukcji „uruchom `./verify.sh full` i powiedz, czy
//! przeszło". Plik się waliduje, bieg startuje, transkrypt mówi `checks passed`, kafelek jest
//! zielony — i sprzedaliśmy jedyne rozróżnienie, dla którego ten produkt powstał: **co agent
//! powiedział** kontra **co się stało** (`docs/research/projects/00-SYNTHESIS.md` §2.1,
//! `docs/harness-as-workflow.md` ustalenie U-1). Nikt tego nie zgłosi, bo wszystko wygląda na
//! skończone, a rachunek u vendora rośnie za opowiadanie o cudzym wyniku.
//!
//! # SŁABA WERSJA numer jeden
//!
//! `assert!(spec.model.is_none())`. Przechodzi dla implementacji, która **odpala agenta bez
//! modelu** — czyli płaci za turę u vendora, żeby ten opowiedział o wyniku komendy. To jest
//! dokładnie ta cicha porażka, przed którą broni U-1.
//!
//! # SŁABA WERSJA numer dwa, i ta wygląda na mocną
//!
//! Sam licznik równy zeru. Zero wywołań sterownika jest przecież prawdą także wtedy, gdy **nie
//! dzieje się nic** — więc ta asercja zazieleniłaby kryterium na szkielecie, który nie robi
//! kompletnie niczego. Rozróżniają to (c) i (d): krok musiał naprawdę wystartować, naprawdę
//! skończyć i naprawdę zapisać wynik WZIĘTY Z WYJŚCIA KOMENDY.
//!
//! # Dlaczego biblioteka agentów tu NIE ISTNIEJE
//!
//! To jest jednocześnie kontrola negatywna. Implementacja routująca ten krok przez `plan_agent`
//! przewróciłaby się na `RunError::NoAgentsSaved`, bo `find_agent` czyta katalog, którego nie ma.
//! Bieg, który mimo tego kończy się powodzeniem, dowodzi, że tamtej drogi nie tknął.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, Outcome, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use serde_json::Value;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

const VENDOR: &str = "fake";

/// Sufit cierpliwości jednego biegu. Jeden krok na `/bin/sh` nie ma jak trwać dłużej.
const PATIENCE: Duration = Duration::from_secs(20);

/// Zdanie, które wypisuje skrypt sprawdzający. Asercja (d) porównuje z NIM zapisany wynik, a nie
/// z jakimkolwiek napisem: „krok ma jakieś podsumowanie" przechodzi dla podsumowania wymyślonego
/// przez nas, a to jest ten sam błąd co werdykt wymyślony przez agenta.
const SAYS: &str = "test result: ok. 3 passed; 0 failed";

/// Skrypt sprawdzający: wypisuje zdanie z licznikiem i wychodzi zerem.
const SCRIPT: &str = r#"#!/bin/sh
echo "test result: ok. 3 passed; 0 failed"
exit 0
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_check_step_runs_without_ever_asking_for_an_agent() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let script = bench.script("checks.sh", SCRIPT)?;
    let workflow = bench.workflow("one-check", &one_check(&script))?;
    let store = Store::open(&bench.db())?;

    let watch = Arc::new(Watch::new());
    let handed_out = Arc::new(AtomicUsize::new(0));

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: counting_drivers(Arc::clone(&watch), Arc::clone(&handed_out)),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 2,
        task: None,
    };

    let report = one_run(&deps, &request).await??;

    // ── (b) NAJMOCNIEJSZA Z TRZECH: nikt nawet nie POPROSIŁ o sterownik ────────────────────
    // Mocniejsza niż licznik startów, bo łapie także implementację, która przechodzi przez
    // `plan_agent`, dostaje sterownik i dopiero potem się rozmyśla — ta zapłaciłaby już za
    // odczyt biblioteki i pomyliłaby się w `run.json` co do vendora.
    assert_eq!(
        handed_out.load(Ordering::SeqCst),
        0,
        "the Drivers factory was asked for a driver {} time(s). A check step has no vendor, so \
         nothing in this run has a reason to ask",
        handed_out.load(Ordering::SeqCst)
    );

    // ── (a) I ANI JEDNEJ SESJI ────────────────────────────────────────────────────────────
    assert_eq!(
        watch.starts(),
        0,
        "the driver was started {} time(s). A step that pays a vendor to narrate the result of a \
         command sells the only distinction this product exists for: what an agent SAID versus \
         what HAPPENED",
        watch.starts()
    );

    // ── (e) NIE MA CZEGO PAMIĘTAĆ, bo żaden RunSpec nie powstał ────────────────────────────
    // Asercja stoi na wewnętrznym wektorze dublera, nie na liczniku: dwie różne rzeczy, z których
    // pierwsza może być zerem także wtedy, gdy druga jest pełna (start policzony po fakcie).
    let specs = watch.specs();
    assert!(
        specs.is_empty(),
        "a RunSpec was built for a check step: {specs:?}. There is no prompt to carry and no \
         model to pick"
    );

    // ── (c) A BIEG SIĘ UDAŁ, MIMO ŻE BIBLIOTEKI AGENTÓW NIE MA ────────────────────────────
    // Kontrola negatywna: implementacja idąca przez `plan_agent` przewróciłaby się tu na
    // `RunError::NoAgentsSaved`, bo `find_agent` czyta katalog, którego nie ma.
    assert!(
        !bench.home.path().join("agents").exists(),
        "this run has to start with NO agent library on disk, or (c) proves nothing"
    );
    assert_eq!(
        report.outcome,
        Outcome::Done,
        "the run has to finish, and finish clean: a check step needs no agent, so an empty \
         library is not its problem"
    );
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded],
        "one step, succeeded — the command exited zero and printed its pass count"
    );

    // ── (d) I WYNIK POCHODZI Z WYJŚCIA KOMENDY, NIE OD NAS ────────────────────────────────
    let run: Value = serde_json::from_str(&fs::read_to_string(report.dir.join("run.json"))?)?;
    let steps = run
        .get("steps")
        .and_then(Value::as_array)
        .ok_or("run.json describes no steps")?;
    let step = steps.first().ok_or("run.json has an empty step list")?;

    assert_eq!(
        step.get("status").and_then(Value::as_str),
        Some("succeeded"),
        "run.json is the truth about this run and the database is only its index (invariant 4); \
         the step has to stand in a final state there. It says: {step}"
    );
    assert_eq!(
        step.get("agent").and_then(Value::as_str),
        Some(""),
        "the vendor label of a check step is EMPTY. `\"local\"` or `\"loadout\"` would be a made-up \
         fact, and resuming would one day go looking for a session that never existed. It says: \
         {step}"
    );
    let summary = step
        .get("summary")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("the check step wrote no result at all: {step}"))?;
    assert!(
        summary.contains(SAYS),
        "the recorded result has to be what the COMMAND printed, word for word. The script said \
         {SAYS:?} and run.json says {summary:?} — a summary Loadout invented for itself is the \
         same failure as a verdict an agent invented for us"
    );
    Ok(())
}

/// Plik workflow z dokładnie jednym krokiem — sprawdzającym.
fn one_check(script: &Path) -> String {
    // Ścieżka bezwzględna wprost w komendzie: środowisko dziecka jest czyszczone, więc przez
    // zmienną nie przejdzie, a katalog roboczy kroku jest folderem projektu, nie tym tempdirem.
    format!(
        r#"{{
  "format": 1,
  "id": "wf_one_check",
  "name": "Run the checks",
  "steps": [
    {{
      "kind": "check",
      "id": "s_check",
      "name": "Run the checks",
      "command": "{}",
      "proof": "(\\d+) passed",
      "folder": {{ "use": "project" }},
      "at": {{ "x": 24, "y": 24 }}
    }}
  ],
  "links": []
}}"#,
        script.display()
    )
}

/// Jeden bieg z limitem cierpliwości. Zewnętrzny `Result` mówi „bieg wrócił", wewnętrzny — czym.
async fn one_run(
    deps: &RunDeps<'_>,
    request: &RunRequest,
) -> Result<Result<RunReport, loadout_lib::commands::RunError>, Box<dyn Error>> {
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let drain = async move {
        let _ = pump.await;
    };

    let both = tokio::time::timeout(PATIENCE, async {
        tokio::join!(run_workflow_inner(deps, request, sink), drain)
    })
    .await
    .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))?;
    Ok(both.0)
}

/// Biblioteka użytkownika i projekt na czas jednego kryterium.
///
/// `agents/` NIE POWSTAJE i to jest treść tego kryterium, nie oszczędność: katalog biblioteki
/// istnieje dopiero po pierwszym zapisanym agencie, czyli na świeżej maszynie go nie ma.
struct Bench {
    home: TempDir,
    project: TempDir,
    scripts: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        let scripts = TempDir::new()?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        Ok(Self {
            home,
            project,
            scripts,
        })
    }

    fn script(&self, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.scripts.path().join(name);
        fs::write(&path, body)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
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

/// Fabryka, która LICZY, ile razy ktokolwiek poprosił o sterownik agenta.
///
/// To jest asercja (b) i jest mocniejsza od licznika startów: implementacja, która dostaje
/// sterownik i dopiero potem się rozmyśla, zapłaciła już za wczytanie biblioteki i wpisała vendora
/// do `run.json`.
fn counting_drivers(watch: Arc<Watch>, asked: Arc<AtomicUsize>) -> Drivers {
    Arc::new(move |_| {
        asked.fetch_add(1, Ordering::SeqCst);
        Arc::new(Fake {
            watch: Arc::clone(&watch),
        }) as Arc<dyn AgentDriver>
    })
}

/// Co dubler zobaczył. Wektor specyfikacji obok licznika, bo (a) i (e) są dwoma różnymi pytaniami.
struct Watch {
    specs: Mutex<Vec<String>>,
}

impl Watch {
    fn new() -> Self {
        Self {
            specs: Mutex::new(Vec::new()),
        }
    }

    fn entered(&self, spec: &RunSpec) {
        self.lock().push(format!(
            "prompt={:?} model={:?} policy={:?}",
            spec.prompt, spec.model, spec.policy
        ));
    }

    fn starts(&self) -> usize {
        self.lock().len()
    }

    fn specs(&self) -> Vec<String> {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<String>> {
        self.specs.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

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
        self.watch.entered(&spec);
        Ok(Box::new(Turn {
            session: SessionRef {
                vendor: VENDOR,
                id: spec.run_id.to_string(),
            },
            events,
        }))
    }
}

/// Jedna tura dublera. Nigdy nie powinna powstać w tym kryterium — istnieje, żeby licznik miał co
/// zliczyć, gdyby powstała.
#[derive(Debug)]
struct Turn {
    session: SessionRef,
    events: mpsc::Sender<DecodedEvent>,
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
            text: "A driver was started for a step that has no agent.".to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::ZERO,
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send(loadout_lib::engine::drivers::AgentEvent::Finished(outcome.clone()).into())
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
