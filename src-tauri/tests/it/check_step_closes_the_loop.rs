//! AC-5 dla T-55: pętla domyka się na WERDYKCIE kroku „sprawdź", a nie na słowie agenta.
//!
//! # Cicha porażka, która jest w produkcie DZISIAJ
//!
//! Pętla z limitem tur weszła 2026-08-19 (`Link::max_turns`, `workflow::unroll`,
//! `Live::verdict_after`) i domyka się na wierszu `outcome: pass` napisanym przez agenta-sędziego.
//! Protokół werdyktu ma więc wyłącznie połowę CZYTAJĄCĄ — nic w produkcie nie mówi sędziemu, żeby
//! ten wiersz napisał. Sędzia, który go nie napisze, dostaje `Verdict::Fail` z domyślnej wartości
//! i pętla kręci się do wyczerpania limitu; sędzia, który napisze go z uprzejmości nad czerwonymi
//! testami, zamyka pętlę na obietnicy. Oba przypadki kosztują prawdziwe tury i oba wyglądają jak
//! działający produkt.
//!
//! # SŁABA WERSJA
//!
//! „Bieg skończył się po dwóch rundach". Przechodzi dla implementacji, która dalej czyta
//! `outcome:` z ust agenta i tylko PRZYPADKIEM zatrzymała się w tym samym miejscu — a także dla
//! takiej, która zawsze robi dwie rundy. Rozróżniają to trzy rzeczy naraz i wszystkie trzy są
//! niżej:
//!
//! * strażnik (d), który zamienia tekst dublera w ODMOWĘ w starym protokole — więc implementacja
//!   domykająca pętlę na słowie agenta przepala wszystkie rundy i kończy porażką;
//! * licznik uruchomień KOMENDY (b), bo tylko on odróżnia „rundy nie było" od „runda przeszła";
//! * sufit `max_turns: 3`, przy którym zatrzymanie się na dwóch jest decyzją, a nie zbiegiem
//!   okoliczności.
//!
//! # Czego to kryterium NIE wymaga
//!
//! `memory::handoff::verdict_in` zostaje i nie jest tu ruszana. Sędzia-agent jest jedyną drogą dla
//! repo, które sprawdzeń nie ma (D7, „Co musi przetrwać nawet przy zerowej ceremonii"), więc
//! `runcmd_loop.rs` ma zostać zielone bez jednej zmiany w swoim pliku. Jeżeli zmiana w
//! `Live::verdict_after` je przewraca, to nie jest kolizja kryteriów, tylko znak, że ścieżka
//! awaryjna została skasowana, a nie uzupełniona drugą.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
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
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::memory::handoff;
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

const VENDOR: &str = "fake";

/// Sufit cierpliwości jednego biegu. Trzy tury dublera i dwa `/bin/sh` nie mają jak trwać dłużej.
const PATIENCE: Duration = Duration::from_secs(30);

/// Prompt kroku, który pisze kod — jedyna rzecz, po której dubler go rozpoznaje. `RunSpec` nie
/// niesie numeru kroku, więc instrukcje są jedynym rozróżnieniem (niezmiennik 9).
const WRITE_PROMPT: &str = "Make the change.";

/// Co dubler agenta mówi ZAWSZE. Ani jednego znacznika `outcome:` — patrz strażnik (d).
const SAID: &str = "I changed three files. The tests are somebody else's business.";

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000c1
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

/// Skrypt sprawdzający: dopisuje wiersz do licznika i **przy drugim uruchomieniu** wychodzi zerem.
///
/// Wzorzec dowodu dopasuje się w OBU rundach (`(\d+) passed` trafia i w `0 passed`, i w
/// `3 passed`), więc o werdykcie rozstrzyga wyłącznie kod wyjścia — i to jest celowe: gdyby
/// o wszystkim decydował wzorzec, kryterium nie odróżniłoby poprawnej implementacji od takiej,
/// która czyta samo dopasowanie.
const COUNTING: &str = r#"#!/bin/sh
# $1 = plik licznika (ścieżka BEZWZGLĘDNA — środowisko dziecka jest czyszczone)
echo run >> "$1"
if [ "$(wc -l < "$1" | tr -d ' ')" -ge 2 ]; then
  echo "test result: ok. 3 passed; 0 failed"
  exit 0
fi
echo "test result: FAILED. 0 passed; 3 failed"
exit 1
"#;

/// Ten sam kształt, który **nigdy** nie wychodzi zerem — wariant wyczerpania z punktu (f).
const NEVER_GREEN: &str = r#"#!/bin/sh
# $1 = plik licznika
echo run >> "$1"
echo "test result: FAILED. 0 passed; 3 failed"
exit 1
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_loop_closes_on_what_the_command_did() -> Result<(), Box<dyn Error>> {
    // ── (d) STRAŻNIK. Postawiony PIERWSZY, bo bez niego reszta nie znaczy nic ──────────────
    // Tekst, którym mówi dubler, jest według starego protokołu ODMOWĄ. Implementacja dalej
    // domykająca pętlę na słowie agenta przepaliłaby więc wszystkie trzy rundy i skończyła
    // porażką — i to jest jedyny sposób, żeby odróżnić „domknęło się na komendzie" od
    // „domknęło się przypadkiem w tym samym miejscu".
    assert_eq!(
        handoff::verdict_in(SAID),
        handoff::Verdict::Fail,
        "the fake agent's sentence has to read as a REFUSAL under the old protocol, or this \
         criterion cannot tell the two implementations apart"
    );

    let bench = Bench::new()?;
    bench.agent("hand", HAND_FILE)?;
    let script = bench.script("counting.sh", COUNTING)?;
    let counter = bench.counter("counted.txt");
    let workflow = bench.workflow("loop", &loop_file(&script, &counter, 3))?;
    let store = Store::open(&bench.db())?;

    let watch = Arc::new(Watch::new());
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

    let report = one_run(&deps, &request).await??;

    // ── (b) LICZNIK URUCHOMIEŃ KOMENDY ────────────────────────────────────────────────────
    // Dokładnie dwa wiersze: runda 0 padła, runda 1 przeszła, runda 2 została POMINIĘTA, a nie
    // przepalona. Ten licznik jest jedynym miejscem, w którym różnica między „rundy nie było"
    // i „runda przeszła" jest widoczna z zewnątrz — stan kroku mówi `succeeded` w obu.
    assert_eq!(
        lines_in(&counter),
        2,
        "the command has to have run exactly twice. Zero means the check step never started a \
         process at all; three means the run burned a whole round on work nobody needed and the \
         result looks identical from the outside. The counter says: {:?}",
        fs::read_to_string(&counter).unwrap_or_default()
    );

    // ── (c) I AGENT ZOBACZYŁ SWÓJ PROMPT DOKŁADNIE DWA RAZY ───────────────────────────────
    assert_eq!(
        watch.times(WRITE_PROMPT),
        2,
        "two rounds of work, because the second check passed. A third start means a paid agent \
         turn nobody needed. The driver saw: {:?}",
        watch.seen()
    );

    // ── (a) BIEG SIĘ UDAŁ ─────────────────────────────────────────────────────────────────
    assert!(
        report.steps.iter().all(|one| *one == StepState::Succeeded),
        "a loop that passed leaves nothing failed behind; it left {:?}",
        report.steps
    );

    // ── (e) I ZOSTAWIŁ PO SOBIE TO, CO PADŁO ──────────────────────────────────────────────
    // Bez tego runda 1 nie wie, co padło w rundzie 0, i pętla nie ma po co istnieć
    // (niezmiennik 21: wyjście komendy ma dwóch czytelników — werdykt i przekazanie).
    let handed = handoff::scan_run_dir(&report.dir)?;
    let from_check: Vec<&handoff::Handoff> = handed
        .iter()
        .filter(|one| one.meta.from == "Run the checks")
        .collect();
    assert!(
        !from_check.is_empty(),
        "the check step handed nothing over, so the next round has no idea what failed. The run \
         left: {:?}",
        handed
            .iter()
            .map(|one| one.meta.from.clone())
            .collect::<Vec<String>>()
    );
    assert!(
        from_check
            .iter()
            .any(|one| one.body.contains("0 passed; 3 failed")),
        "and the body has to carry the COMMAND's own output. A handoff that says only 'the check \
         failed' hands the next round a verdict instead of evidence. The bodies were: {:?}",
        from_check
            .iter()
            .map(|one| one.body.clone())
            .collect::<Vec<String>>()
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_that_never_passes_runs_out_of_tries_and_stops() -> Result<(), Box<dyn Error>> {
    // ── (f) WARIANT WYCZERPANIA ───────────────────────────────────────────────────────────
    // Ten sam graf z `max_turns: 2` i komendą, która nigdy nie wychodzi zerem. Bez tej połowy
    // limit tur jest ozdobą: wyczerpanie prób wyglądałoby jak sukces.
    let bench = Bench::new()?;
    bench.agent("hand", HAND_FILE)?;
    let script = bench.script("never-green.sh", NEVER_GREEN)?;
    let counter = bench.counter("counted.txt");
    let workflow = bench.workflow("loop", &loop_file(&script, &counter, 2))?;
    let store = Store::open(&bench.db())?;

    let watch = Arc::new(Watch::new());
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

    let report = one_run(&deps, &request).await??;

    assert_eq!(
        lines_in(&counter),
        2,
        "both tries have to be spent — a limit of two that gives up after one is a different \
         promise than the one on the arrow. The counter says: {:?}",
        fs::read_to_string(&counter).unwrap_or_default()
    );
    assert!(
        report.steps.contains(&StepState::Failed),
        "the run has to end red, or nothing tells the person their work never passed; it ended \
         {:?}",
        report.steps
    );
    Ok(())
}

/// `s_write` (agent) → `s_check` (sprawdzenie), plus powrót `s_check → s_write`.
///
/// Powrót wychodzi z KROKU SPRAWDZAJĄCEGO, więc to on jest sędzią pętli — i to jest cała treść
/// tego kryterium. Oba kroki pracują w folderze projektu, co jest legalne, bo strzałka między
/// nimi znaczy, że nigdy nie biegną równocześnie (niezmiennik 12).
fn loop_file(script: &Path, counter: &Path, turns: u8) -> String {
    format!(
        r#"{{
  "format": 1,
  "id": "wf_check_loop",
  "name": "Write and check",
  "steps": [
    {{
      "kind": "agent",
      "id": "s_write",
      "name": "Write the code",
      "agent": "01990000-0000-7000-8000-0000000000c1",
      "overrides": {{}},
      "copies": 1,
      "instructions": "Make the change.",
      "skills": "all",
      "folder": {{ "use": "project" }},
      "handover": "notes",
      "at": {{ "x": 24, "y": 24 }}
    }},
    {{
      "kind": "check",
      "id": "s_check",
      "name": "Run the checks",
      "command": "{} {}",
      "proof": "(\\d+) passed",
      "folder": {{ "use": "project" }},
      "at": {{ "x": 24, "y": 168 }}
    }}
  ],
  "links": [
    {{ "from": "s_write", "to": "s_check" }},
    {{ "from": "s_check", "to": "s_write", "max_turns": {turns} }}
  ]
}}"#,
        script.display(),
        counter.display()
    )
}

/// Ile wierszy dopisał skrypt. Plik, którego nie ma, to zero uruchomień — nie błąd testu.
fn lines_in(counter: &Path) -> usize {
    fs::read_to_string(counter)
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
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

/// Biblioteka użytkownika, projekt i katalog na skrypty — na czas jednego kryterium.
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
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        Ok(Self {
            home,
            project,
            scripts,
        })
    }

    fn agent(&self, slug: &str, text: &str) -> Result<(), Box<dyn Error>> {
        fs::write(
            self.home.path().join("agents").join(format!("{slug}.md")),
            text,
        )?;
        Ok(())
    }

    fn script(&self, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.scripts.path().join(name);
        fs::write(&path, body)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        Ok(path)
    }

    /// Ścieżka licznika. Plik jeszcze nie istnieje — dopisuje go skrypt.
    fn counter(&self, name: &str) -> PathBuf {
        self.scripts.path().join(name)
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
fn fake_drivers(watch: Arc<Watch>) -> Drivers {
    Arc::new(move |_| {
        Arc::new(Fake {
            watch: Arc::clone(&watch),
        }) as Arc<dyn AgentDriver>
    })
}

/// Co dubler widział, w kolejności.
struct Watch {
    seen: Mutex<Vec<String>>,
}

impl Watch {
    fn new() -> Self {
        Self {
            seen: Mutex::new(Vec::new()),
        }
    }

    fn entered(&self, prompt: &str) {
        self.lock().push(prompt.to_owned());
    }

    /// Ile razy dubler zobaczył prompt zawierający ten fragment.
    fn times(&self, needle: &str) -> usize {
        self.lock()
            .iter()
            .filter(|one| one.contains(needle))
            .count()
    }

    fn seen(&self) -> Vec<String> {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<String>> {
        self.seen.lock().unwrap_or_else(PoisonError::into_inner)
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
        self.watch.entered(&spec.prompt);
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

/// Jedna tura dublera. Mówi ZAWSZE to samo zdanie — bez ani jednego znacznika werdyktu.
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
            text: SAID.to_owned(),
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
