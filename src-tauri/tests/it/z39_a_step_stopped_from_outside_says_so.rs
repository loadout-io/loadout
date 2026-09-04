//! Krok ubity SPOZA Loadouta mówi to na swojej karcie, a następny mówi, że jedzie bez jego wyniku.
//!
//! # Skąd to kryterium
//!
//! Bieg meetnotes `20260901-150035`. Lider nie miał czym zatrzymać biegu — most zna
//! `list_workflows`, `list_agents`, `start_workflow` i `ask_the_person` — więc przeczytał `pgid`
//! z `run.json` i wykonał `kill -TERM -38475 -38476` sam. Loadout zapisał wtedy to, co powiedział
//! vendor: „The agent stopped without ever sending its result". To jest prawda o tym, co widział
//! sterownik, i **zdanie o agencie, który nie zrobił nic złego** — a człowiek czytający historię
//! szuka po nim wady u agenta. Bieg pojechał dalej z `carry-on`: Final Plan 17 minut Codeksa bez
//! researchu, Combine 26 minut na ubitych krokach, aż człowiek nacisnął Stop.
//!
//! # Słaba wersja tego kryterium
//!
//! `assert!(!step.error.is_empty())`. Przechodzi dziś, bo zdanie tam stoi — tylko mówi o czym
//! innym. Rozstrzyga TREŚĆ, i to czytana tą samą drogą, którą czyta ją okno
//! ([`read_run_inner`], niezmiennik 29): karta kroku w historii bierze `error` i `ranWithout`
//! stamtąd, nie z pola struktury, do którego nikt nie zagląda.
//!
//! # Kontrola przeciw implementacji, która mówi to o KAŻDEJ porażce
//!
//! Trzeci przypadek w tym pliku. Krok, który po prostu wyszedł kodem 3, ma zachować swój własny
//! powód i **nie może** dostać ani zdania o obcym sygnale, ani klucza `stopped_from_outside`.
//! Bez niego całe kryterium przechodzi dla implementacji, dla której wszystko jest ubiciem
//! z zewnątrz — a wtedy zdanie przestaje cokolwiek rozróżniać.
//!
//! # Sześć dróg zejścia i gdzie sądzi je która wyrocznia
//!
//! Ten plik bierze trzy: kod zero (drugi krok każdego przypadku), kod ≠ 0 bez sygnału (przypadek
//! trzeci) i sygnał obcy (przypadki pierwszy i drugi, po jednym na każdy nośnik numeru).
//! Pozostałe trzy mają swoich sędziów od dawna i ten plik ich nie dubluje: Stop człowieka sądzi
//! `run_stop_waits_for_proof`, limit czasu kroku — `step_timeout_kills_the_group`, brak aplikacji
//! agenta — `a_vendor_failure_names_the_next_move`.
//!
//! Testy odpalają prawdziwe procesy i **nie są** `#[ignore]`: cel z samymi pominiętymi testami
//! melduje „0 passed", a to nie jest dowód (niezmiennik 19).

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::supervisor::{self, GroupId, GroupProof, StdinPlan, Supervised};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use serde_json::Value as Json;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Nazwa pierwszego kafelka. Pisownia jest z pliku workflow właściciela, znak w znak.
const REASERCH: &str = "Reaserch";

/// Nazwa kafelka za nim — tego, który pojechał bez cudzego wyniku.
const COMBINE: &str = "Combine";

/// Zdanie, którego szukamy na karcie ubitego kroku. Wartość oczekiwana, nie wyliczona z wyniku.
const SAYS_SOMEBODY_ELSE: &str = "Something outside Loadout stopped this step (signal 15)";

/// Zdanie, którego szukamy na karcie następnego kroku i w jego strumieniu.
const RUNS_WITHOUT: &str = "runs without Reaserch's result — it was stopped from outside";

/// Zdanie vendora o kroku, który przestał mówić. To jest DOKŁADNIE to, co `run.json` niosło
/// w biegu `20260901-150035` — i to, co ma tam przestać stać samo.
const VENDOR_SAW_NOTHING: &str = "The agent stopped without ever sending its result.";

/// Powód, który krok kończący się kodem 3 podaje sam. Kontrola przeciw zdaniu o obcym sygnale.
const ITS_OWN_REASON: &str = "This step could not read the file it was pointed at.";

/// Okno łaski między SIGTERM a SIGKILL. Sekunda wystarcza: krok, który tu schodzi, nie ignoruje
/// sygnału — jego zejście jest przedmiotem pomiaru, a nie przeszkodą.
const GRACE: Duration = Duration::from_secs(1);

/// Ile czekamy, aż krok wystawi swoją grupę procesów.
///
/// HOJNE Z ROZMYSŁEM i niczego to nie osłabia: bariera jest PRZYGOTOWANIEM, nie pomiarem. Ten
/// plik pyta o TREŚĆ zdania, a treść nie zależy od tego, jak długo wcześniej wstawała powłoka
/// na obciążonej maszynie.
const START_LIMIT: Duration = Duration::from_mins(2);

/// Odstęp między pytaniami o gotowość. Krótki, bo nic tu nie mierzy czasu.
const PROBE_POLL: Duration = Duration::from_millis(2);

/// Ile czekamy, zanim uznamy bieg za zawieszony. Bieg, który wisi, jest dla bramki „nie
/// uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_mins(1);

/// Pojemność kolejki linii. Z zapasem: ten plik czyta strumień, a nie mierzy przepustowość.
const ROOMY: usize = 1_024;

/// Krok, który stoi, dopóki ktoś go nie zabije. Ginie od SIGTERM-a akcją domyślną — i o to chodzi:
/// jego status ma nieść numer sygnału.
///
/// Plik ze skryptem i **pętla**, nigdy pojedyncza komenda: powłoka exec-optymalizuje ostatnią
/// komendę, a wtedy grupa, którą obserwuje test, należy do procesu, którego już nie ma [T7 §8.2].
const STANDS_STILL: &str = r#"#!/bin/sh
# $1 = plik gotowości
: > "$1"
while :; do
  sleep 0.2
done
"#;

/// To samo zejście, drugim nośnikiem: powłoka ŁAPIE piętnastkę i wychodzi `143`.
///
/// Tak zachowuje się każde opakowanie — `sh -c`, `npm`, wrapper vendora — i tak wygląda to
/// w eksporcie diagnostyki biegu `20260901-150035`: `exitCode: 143`, `turns: 0`. Status nie niesie
/// wtedy sygnału ani razu, a fakt jest ten sam.
const EXITS_143: &str = r#"#!/bin/sh
# $1 = plik gotowości
trap 'exit 143' TERM
: > "$1"
while :; do
  sleep 0.2
done
"#;

/// Krok, który wychodzi kodem 3 od razu. Nikt go nie zabija i nic z zewnątrz go nie dotyka.
const EXITS_THREE: &str = r"#!/bin/sh
exit 3
";

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000003a1
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

/// `Reaserch → Combine`, gdzie Reaserch ma „carry-on" — dokładnie kształt tamtego biegu.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_stopped_from_outside",
  "name": "A step stopped from outside",
  "steps": [
    {
      "kind": "agent",
      "id": "s_reaserch",
      "name": "Reaserch",
      "agent": "01990000-0000-7000-8000-0000000003a1",
      "overrides": {},
      "instructions": "reaserch: stand still until somebody stops it",
      "whenItFails": "carry-on",
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_combine",
      "name": "Combine",
      "agent": "01990000-0000-7000-8000-0000000003a1",
      "overrides": {},
      "instructions": "combine: put together what came before",
      "whenItFails": "carry-on",
      "at": { "x": 240, "y": 0 }
    }
  ],
  "links": [{ "from": "s_reaserch", "to": "s_combine" }]
}
"#;

/// Znacznik w prompcie pierwszego kroku. Po nim dubler poznaje, któremu krokowi ma postawić
/// prawdziwy proces — kolejność wywołań byłaby tu założeniem o planiście, a nie o kroku.
const FIRST_STEP_MARK: &str = "reaserch:";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_step_killed_from_outside_says_so_and_the_next_one_says_it_went_on_without_it()
-> Result<(), Box<dyn Error>> {
    let ran = a_run_whose_first_step(STANDS_STILL, Kill::FromOutside).await?;

    let stopped = ran.step(REASERCH)?;
    assert!(
        stopped.error.contains(SAYS_SOMEBODY_ELSE),
        "the card of a step somebody killed from outside says {:?}. That is what the vendor saw \
         — an agent that went quiet — and it sends the person looking for a fault in an agent \
         that did nothing: it was killed. The card has to name the signal and name that it came \
         from outside Loadout (invariant 29: this is read through read_run_inner, the same door \
         the window uses)",
        stopped.error
    );

    let carried = ran.step(COMBINE)?;
    assert!(
        carried.ran_without.iter().any(|said| said == RUNS_WITHOUT),
        "the step behind it ran on `carry-on` and its card says nothing about the material it \
         never got. That is how run 20260901-150035 spent 17 minutes of Codex on a Final Plan \
         with no research and 26 minutes on a Combine over dead steps — from the outside it looks \
         exactly like a step that simply answered that way. The card said: {:?}",
        carried.ran_without
    );
    assert!(
        ran.lines.iter().any(|line| {
            matches!(line, Line::Problem { agent, text, .. } if agent == COMBINE && text == RUNS_WITHOUT)
        }),
        "the same sentence never reached the stream, so the person watching the run live is not \
         told anything at all — they find out tomorrow, in history, after paying for the work. \
         The stream carried: {:?}",
        ran.problems()
    );

    // Kod zero dalej mówi swoje: krok, który wyszedł sam, nie ma ani zdania o sygnale, ani
    // klucza w pliku. Bez tego to samo zielone dostałaby implementacja, dla której KAŻDE
    // zejście jest ubiciem z zewnątrz.
    assert!(
        !carried.error.contains(SAYS_SOMEBODY_ELSE),
        "the step that exited cleanly was also called stopped-from-outside: {:?}",
        carried.error
    );
    assert_eq!(
        ran.in_the_file(COMBINE)?
            .get("stopped_from_outside")
            .and_then(Json::as_i64),
        None,
        "a step that exited on its own must not carry a signal number in run.json"
    );

    assert_eq!(
        ran.in_the_file(REASERCH)?
            .get("stopped_from_outside")
            .and_then(Json::as_i64),
        Some(15),
        "run.json is the only place this fact survives `loadout.db` being deleted (invariant 4), \
         and it is the one value that tells a fifteen from a nine — a polite stop from a kill"
    );
    Ok(())
}

/// Ten sam fakt, drugim nośnikiem: powłoka łapie sygnał i wychodzi `143`, więc status nie niesie
/// go ani razu.
///
/// Bez tego przypadku implementacja czytająca wyłącznie `ExitStatus::signal()` jest zielona,
/// a w eksporcie diagnostyki tamtego biegu stoi dokładnie `exitCode: 143`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_wrapper_that_turns_the_signal_into_exit_143_says_the_same_thing()
-> Result<(), Box<dyn Error>> {
    let ran = a_run_whose_first_step(EXITS_143, Kill::FromOutside).await?;

    let stopped = ran.step(REASERCH)?;
    assert!(
        stopped.error.contains(SAYS_SOMEBODY_ELSE),
        "a step whose wrapper caught the signal and exited 143 said {:?}. `128 + n` is how every \
         shell reports a signal it handled, and it is the shape the diagnostics export of run \
         20260901-150035 actually carries",
        stopped.error
    );
    assert!(
        ran.step(COMBINE)?
            .ran_without
            .iter()
            .any(|said| said == RUNS_WITHOUT),
        "and the step behind it still has to say it went on without that result"
    );
    Ok(())
}

/// Kontrola: krok, który po prostu wyszedł kodem 3, zachowuje swój własny powód.
///
/// To jest przypadek, dla którego cała reszta tego pliku cokolwiek znaczy. Implementacja, która
/// mówi „stopped from outside" o każdej porażce, przechodzi oba kryteria wyżej i nie rozróżnia
/// niczego — a zdanie, które pada zawsze, przestaje być informacją.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_step_that_just_exits_nonzero_keeps_its_own_reason() -> Result<(), Box<dyn Error>> {
    let ran = a_run_whose_first_step(EXITS_THREE, Kill::NobodyTouchesIt).await?;

    let failed = ran.step(REASERCH)?;
    assert!(
        !failed.error.contains("outside Loadout"),
        "nobody touched this step — it exited with code 3 on its own — and its card blames \
         somebody outside Loadout: {:?}. A sentence that appears for every failure stops telling \
         the person anything",
        failed.error
    );
    assert!(
        failed.error.contains(ITS_OWN_REASON),
        "and it lost the reason the agent gave, which is the only thing that says WHAT went \
         wrong. It said: {:?}",
        failed.error
    );
    assert_eq!(
        ran.in_the_file(REASERCH)?
            .get("stopped_from_outside")
            .and_then(Json::as_i64),
        None,
        "an exit code without a signal is not a signal, and run.json must not claim one"
    );
    assert!(
        ran.step(COMBINE)?.ran_without.is_empty(),
        "and the step behind it says nothing about material it never got: an ordinary failure \
         leaves the prose the agent managed to write, so there IS something to hand on. A line \
         at every carry-on in every run is noise (invariant 16 in spirit). It said: {:?}",
        ran.step(COMBINE)?.ran_without
    );
    Ok(())
}

// ── ŁAWA ───────────────────────────────────────────────────────────────────────────────────

/// Czy ktoś ma ubić pierwszy krok spoza Loadouta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kill {
    /// `/bin/kill -TERM -<pgid>` z osobnego procesu — dokładnie to, co zrobił lider 2026-09-01.
    FromOutside,
    /// Nikt. Krok schodzi sam i po swojemu.
    NobodyTouchesIt,
}

/// Co po biegu widzi człowiek: kroki tą samą drogą, którą czyta je okno, plus surowy `run.json`
/// i strumień.
struct WhatTheRunLeft {
    steps: Vec<StepAsShown>,
    file: Json,
    lines: Vec<Line>,
}

/// Krok tak, jak podaje go [`read_run_inner`] — czyli tak, jak rysuje go karta w historii.
struct StepAsShown {
    name: String,
    error: String,
    ran_without: Vec<String>,
}

impl WhatTheRunLeft {
    fn step(&self, name: &str) -> Result<&StepAsShown, Box<dyn Error>> {
        self.steps
            .iter()
            .find(|step| step.name == name)
            .ok_or_else(|| {
                format!(
                    "history has no step called {name}; it listed {:?}",
                    self.steps
                        .iter()
                        .map(|step| step.name.as_str())
                        .collect::<Vec<_>>()
                )
                .into()
            })
    }

    fn in_the_file(&self, name: &str) -> Result<&Json, Box<dyn Error>> {
        self.file
            .get("steps")
            .and_then(Json::as_array)
            .ok_or("run.json has no steps to look at")?
            .iter()
            .find(|step| step.get("name").and_then(Json::as_str) == Some(name))
            .ok_or_else(|| format!("run.json has no step named {name}").into())
    }

    /// Same wiersze problemów, do komunikatu porażki: cały strumień jest nieczytelny w asercji.
    fn problems(&self) -> Vec<(&str, &str)> {
        self.lines
            .iter()
            .filter_map(|line| match line {
                Line::Problem { agent, text, .. } => Some((agent.as_str(), text.as_str())),
                _ => None,
            })
            .collect()
    }
}

/// Odpala `Reaserch → Combine`, gdzie Reaserch jest tym skryptem, i oddaje wszystko, co po biegu
/// zostało.
async fn a_run_whose_first_step(
    script_body: &str,
    kill: Kill,
) -> Result<WhatTheRunLeft, Box<dyn Error>> {
    let bench = Bench::new()?;
    let hand = bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("stopped-from-outside", WORKFLOW)?;
    let script = write_script(bench.project.path(), "first-step.sh", script_body)?;
    let ready = bench.project.path().join("the-step-is-up");
    the_fixture_can_run(&workflow, &[&hand])?;
    let store = Store::open(&bench.db())?;

    let started: Arc<Mutex<Option<GroupId>>> = Arc::new(Mutex::new(None));
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: drivers_for(Arc::new(Fake {
            script,
            ready: ready.clone(),
            started: Arc::clone(&started),
        })),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let (sink, mut source) = loadout_lib::ipc::line_channel(ROOMY);
    let killing = async {
        if kill == Kill::NobodyTouchesIt {
            return Ok::<(), Box<dyn Error>>(());
        }
        let group = wait_for_group(&started, START_LIMIT).await?;
        // Krok MUSI już stać, zanim poleci sygnał: piętnastka wysłana w okno startu powłoki
        // zabiłaby coś, czego jeszcze nie ma, i cały pomiar dotyczyłby wtedy pustej grupy.
        assert!(
            wait_for_file(&ready, START_LIMIT).await,
            "the step never reported that it was up, so the signal below would go to a group \
             that had not started yet"
        );
        /* OSOBNY PROCES, nie `libc::kill` z tego wątku, i to jest cała treść tej linii: sygnał ma
         * przyjść SPOZA aplikacji, dokładnie tak, jak przyszedł 2026-09-01, kiedy lider wykonał
         * `kill -TERM -38475 -38476` z narzędzia Bash. Minus przed numerem adresuje GRUPĘ. */
        let sent = tokio::process::Command::new("/bin/kill")
            .args(["-TERM", &format!("-{}", group.pgid)])
            .status()
            .await?;
        assert!(
            sent.success(),
            "the outside kill did not go through ({sent:?}), so nothing below is about a step \
             somebody stopped"
        );
        Ok(())
    };

    let (ran, killed) = tokio::time::timeout(PATIENCE, async {
        tokio::join!(run_workflow_inner(&deps, &request, sink), killing)
    })
    .await
    .map_err(|_| format!("neither the run nor the outside kill came back within {PATIENCE:?}"))?;
    killed?;
    let report = ran?;

    let mut lines = Vec::new();
    while let Some(line) = source.try_next() {
        lines.push(line);
    }

    let folder = report
        .dir
        .file_name()
        .ok_or("the run directory has no name")?
        .to_string_lossy()
        .into_owned();
    let past = read_run_inner(bench.project.path(), &folder)?;
    let steps = past
        .steps
        .iter()
        .map(|step| StepAsShown {
            name: step.name.clone(),
            error: step.error.clone(),
            ran_without: step.ran_without.clone(),
        })
        .collect();

    Ok(WhatTheRunLeft {
        steps,
        file: serde_json::from_str(&fs::read_to_string(report.dir.join("run.json"))?)?,
        lines,
    })
}

/// Fabryka, która dla każdego vendora oddaje ten sam dubler.
fn drivers_for(driver: Arc<dyn AgentDriver>) -> Drivers {
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler sterownika: pierwszemu krokowi stawia **prawdziwy** proces we własnej grupie, każdemu
/// następnemu oddaje turę, która kończy się sama.
///
/// Prawdziwy proces, a nie atrapa, bo przedmiotem tego kryterium jest status, który jądro oddaje
/// po zabitym procesie. Zmyślony `GroupProof::Dead { status }` przechodziłby każdą asercję
/// i nie mówiłby nic o tym, czy Loadout umie ten status przeczytać.
#[derive(Debug)]
struct Fake {
    script: PathBuf,
    ready: PathBuf,
    started: Arc<Mutex<Option<GroupId>>>,
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
        _events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };
        if !spec.prompt.contains(FIRST_STEP_MARK) {
            return Ok(Box::new(Quiet { session }));
        }

        let mut command = tokio::process::Command::new(&self.script);
        command.arg(&self.ready);
        let child = supervisor::spawn(command, StdinPlan::Null)?;
        let group = child.group();
        *self.started.lock().unwrap_or_else(PoisonError::into_inner) = Some(group);

        Ok(Box::new(Standing {
            session,
            child,
            group,
        }))
    }
}

/// Tura pierwszego kroku: żywa grupa procesów, która schodzi wtedy, kiedy zejdzie jej proces.
///
/// Kształt trzech czasowników jest przepisany z `engine::drivers::claude`, bo to jego zachowanie
/// jest tu przedmiotem pomiaru: `close()` zbiera lidera i oddaje jego KOD (a nie sygnał),
/// `proof_of_death()` bierze dowód z nadzoru, a `wait()` oddaje to, co vendor mówi o agencie,
/// który przestał się odzywać.
#[derive(Debug)]
struct Standing {
    session: SessionRef,
    child: Supervised,
    group: GroupId,
}

#[async_trait]
impl AgentHandle for Standing {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        Some(self.group)
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        let status = self.child.wait().await?;
        // ZNAK W ZNAK TO, CO MÓWI PRAWDZIWY STEROWNIK (`claude.rs`), kiedy proces zszedł, zanim
        // wynik przyszedł: to jest zdanie, które stało w `run.json` biegu 20260901-150035.
        // Kod 3 dostaje własny powód, bo tylko wtedy agent MIAŁ co powiedzieć — to jest ta
        // różnica, którą sądzi kontrola z trzeciego przypadku tego pliku.
        let why = if status.code() == Some(3) {
            ITS_OWN_REASON.to_owned()
        } else {
            VENDOR_SAW_NOTHING.to_owned()
        };
        Ok(TurnOutcome {
            ok: false,
            reason: FinishReason::Failed(why),
            text: String::new(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 0,
            took: Duration::ZERO,
            session: self.session.clone(),
        })
    }

    async fn cancel(&mut self) -> GroupProof {
        self.child.stop(GRACE).await
    }

    async fn proof_of_death(&mut self) -> GroupProof {
        self.child.stop(GRACE).await
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        // Dokładnie tyle, ile robi prawdziwe zamknięcie: zbiera lidera i oddaje jego KOD wyjścia.
        // Numeru sygnału tędy nie ma — `ExitStatus::code()` oddaje wtedy `None`.
        Ok(self.child.wait().await?.code())
    }
}

/// Tura każdego kolejnego kroku: kończy się od razu i kodem zero.
#[derive(Debug)]
struct Quiet {
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for Quiet {
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
        Ok(TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "put together what came before".to_owned(),
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

/// Czeka, aż plik się pojawi. `false`, kiedy się nie doczekał.
async fn wait_for_file(path: &Path, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        tokio::time::sleep(PROBE_POLL).await;
    }
    false
}

/// Czeka, aż krok wystawi swoją grupę procesów.
async fn wait_for_group(
    started: &Mutex<Option<GroupId>>,
    limit: Duration,
) -> Result<GroupId, Box<dyn Error>> {
    let deadline = Instant::now() + limit;
    loop {
        // Zamek brany i oddany w jednym wyrażeniu: między nim a `await` niżej nie ma ani jednej
        // instrukcji (niezmiennik 8).
        let seen = *started.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(group) = seen {
            return Ok(group);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "no step started a process group within {limit:?}, so there is nothing to stop \
                 from outside. Either the run never reached the driver, or it came back before \
                 it got there"
            )
            .into());
        }
        tokio::time::sleep(PROBE_POLL).await;
    }
}

/// Zapisuje wykonywalny skrypt `#!/bin/sh` i zwraca jego ścieżkę.
fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// Fikstura ma przejść walidator **bez ani jednego problemu**, a jej plik agenta ma dać się
/// przeczytać.
///
/// To nie jest część kryterium, tylko jego przesłanka. Czerwień wygląda identycznie dla
/// „zachowania jeszcze nie ma" i dla „tego kryterium nie da się spełnić nigdy": workflow, który
/// `workflow::check` odrzuca, byłby odmową w KAŻDEJ implementacji.
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
