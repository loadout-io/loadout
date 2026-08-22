//! AC-3 dla T-30: Stop wraca dopiero **po** dowodzie, że grupa procesów nie żyje.
//!
//! Niezmiennik 6 czyta się dosłownie: dopóki `kill(-pgid, 0)` nie odpowiedział `ESRCH`, grupa
//! jest żywa. Cicha wersja złamania nie wygląda jak zły kod — wygląda jak `stop_run_inner`,
//! które wysyła sygnał i wraca. UI mówi wtedy „zatrzymane", agent dalej pisze i **dalej płaci**;
//! to jest błąd finansowy, nie higieniczny [T7 §3.1: `total=2 orphaned=2` przy statusie dziecka
//! mówiącym „zabity"].
//!
//! # Słaba wersja tego kryterium: `assert!(stop.is_ok())`
//!
//! Przechodzi na implementacji, która wysyła SIGTERM i wraca. Rozstrzyga **kolejność**, i pytają
//! o nią dwie asercje z dwóch stron, obie bez wyścigu:
//!
//! * sonda **w chwili powrotu** ma oddać `ESRCH` — a jądro nie odpowie `ESRCH` grupie, w której
//!   ktoś jeszcze jest, więc ta odpowiedź znaczy „już nie żyło, kiedy wracało";
//! * powrót ma nastąpić **nie wcześniej** niż po oknie łaski [`GRACE`].
//!
//! **Krok ignoruje SIGTERM** (`trap '' TERM`) i to jest cała konstrukcja tego pomiaru. Proces,
//! który ginie od pierwszego sygnału, ginie w mikrosekundach — wtedy „wróciło po sygnale"
//! i „wróciło po dowodzie" wypadają w tej samej milisekundzie i nie da się ich odróżnić.
//! Z ignorowanym TERM-em żaden uczciwy dowód nie może przyjść przed końcem okna łaski
//! i eskalacją do dziewiątki, a implementacja bez dowodu wraca w tej samej mikrosekundzie,
//! w której wysłała sygnał. Między jednym a drugim leży cała sekunda.
//!
//! # Kontrola dodatnia
//!
//! Ta sama sonda **przed** Stopem musi oddać sukces. Bez niej `ESRCH` po Stopie znaczy równie
//! dobrze „procesu nigdy nie było" — a wtedy całe kryterium przechodzi na pustym zbiorze,
//! dokładnie tak, jak przechodzi skan `ps`, który nie znalazł niczego, bo nic nie wystartowało.
//!
//! Test odpala prawdziwe procesy i **nie jest** `#[ignore]`: kryterium woła go gołym
//! `cargo test --test run_stop_waits_for_proof`, a cel z samymi pominiętymi testami melduje
//! „0 passed" — czyli niczego nie dowodzi (niezmiennik 19).

use std::error::Error;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::anyhow;
use async_trait::async_trait;
use loadout_lib::commands::run::{run_workflow_inner, stop_run_inner};
use loadout_lib::commands::{Drivers, Outcome, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::supervisor::{self, GroupId, GroupProof, StdinPlan, Supervised};
use loadout_lib::ipc::{line_channel, spawn_pump};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Okno łaski między SIGTERM a SIGKILL, podane argumentem zamiast wzięte ze stałej produkcyjnej
/// (`DEFAULT_GRACE` to pięć sekund). Jest zarazem tym, co rozdziela „wysłałem sygnał" od
/// „mam dowód": krok ignoruje TERM, więc grupa schodzi dopiero po tym oknie.
const GRACE: Duration = Duration::from_secs(1);

/// Ile czekamy na to, żeby krok w ogóle wystał sobie grupę procesów i zainstalował trap.
///
/// HOJNE Z ROZMYSŁEM, i to niczego nie osłabia: obie bariery, które tej stałej używają, są
/// PRZYGOTOWANIEM, a nie pomiarem. Ten test mierzy KOLEJNOŚĆ dwóch chwil — czy Stop wrócił
/// przed dowodem zejścia grupy, czy po nim — a ta kolejność nie zależy od tego, jak długo
/// wcześniej wstawała powłoka.
///
/// Zmierzone 2026-08-17: przy dziesięciu sekundach test padał w pełnej suicie i przechodził
/// 3/3 na spokojnej maszynie w 1,2 s. Powód nie leżał w kodzie: `cargo test --tests` linkuje
/// 122 osobne binaria, więc powłoka kroku nie dostawała procesora w oknie, które wyglądało
/// na aż nadto szerokie. Krótki limit na barierze przygotowania nie chroni przed niczym —
/// zamienia tylko obciążenie maszyny w oskarżenie poprawnego kodu.
const START_LIMIT: Duration = Duration::from_mins(2);

/// Odstęp między pytaniami sondy. Krótki, bo mierzymy KOLEJNOŚĆ dwóch chwil, a nie czas.
const PROBE_POLL: Duration = Duration::from_millis(2);

/// Ile czekamy, zanim uznamy bieg albo Stop za zawieszone. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(30);

/// Pojemność kolejki do pompy. Ten krok nie mówi ani słowa, więc zapas jest tu formalnością —
/// szew jest prawdziwy, bo bieg ma dostać dokładnie to, co dostaje w oknie.
const ROOMY: usize = 1_024;

/// Krok, który **nie kończy się sam** i **ignoruje SIGTERM**.
///
/// Ten sam kształt, co `STUBBORN` w `tests/supervisor_term_then_kill.rs`, i z tych samych trzech
/// powodów:
///
/// * `trap '' TERM` przed pętlą — powłoka ma ignorować sygnał od pierwszej chwili;
/// * **plik gotowości powstaje PO zainstalowaniu trapu**. Bez tej synchronizacji SIGTERM potrafi
///   dotrzeć, zanim `trap` w ogóle się wykona: krok ginie wtedy od akcji domyślnej, Stop wraca
///   po dwóch milisekundach i test oskarża poprawną implementację o powrót bez dowodu;
/// * plik ze skryptem i **pętla**, nigdy pojedyncza komenda — powłoka exec-optymalizuje ostatnią
///   komendę, a wtedy trap instaluje się w procesie, którego już nie ma [T7 §8.2].
const NEVER_ENDS: &str = r#"#!/bin/sh
# $1 = plik gotowości
trap '' TERM
: > "$1"
while :; do
  sleep 0.2
done
"#;

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000e1
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

/// Jeden krok, który nie kończy się sam.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_never_ends",
  "name": "One step that never ends",
  "steps": [
    {
      "kind": "agent",
      "id": "s_hand",
      "name": "Hand",
      "agent": "01990000-0000-7000-8000-0000000000e1",
      "overrides": {},
      "instructions": "never end",
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stop_comes_back_only_after_the_group_is_proved_dead() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let hand = bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("never-ends", WORKFLOW)?;
    let script = write_script(bench.project.path(), "never-ends.sh", NEVER_ENDS)?;
    let ready = bench.project.path().join("trap-installed");
    the_fixture_can_run(&workflow, &[&hand])?;
    let store = Store::open(&bench.db())?;

    let started: Arc<Mutex<Option<GroupId>>> = Arc::new(Mutex::new(None));
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(script, ready.clone(), Arc::clone(&started)),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
        task: None,
        only: None,
        handoffs_from: None,
    };

    let recorder = Recorder::default();
    let (sink, source) = line_channel(ROOMY);
    let pump = spawn_pump(source, recorder.channel());

    let watching = async {
        // ── Kontrola dodatnia ─────────────────────────────────────────────────────────────
        // Bez niej `ESRCH` po Stopie znaczy „procesu nigdy nie było", a całe kryterium
        // przechodzi na pustym zbiorze.
        let group = wait_for_group(&started, START_LIMIT).await?;
        // Trap MUSI już stać, zanim naciśniemy Stop. SIGTERM, który dotarł przed nim, zabija
        // krok akcją domyślną — a wtedy Stop wraca po dwóch milisekundach i test oskarża
        // poprawną implementację o powrót bez dowodu.
        assert!(
            wait_for_file(&ready, START_LIMIT).await,
            "the step never reported that its TERM trap was installed, so nothing measured \
             below would be about a step that survives the first signal"
        );
        let alive = group_probe(group.pgid);
        assert!(
            alive.is_ok(),
            "kill(-{}, 0) does not find the step's process group even before Stop, so ESRCH \
             afterwards would prove nothing: it would mean the process was never there. The \
             probe said {alive:?}",
            group.pgid
        );

        let pressed = Instant::now();
        let outcome = tokio::time::timeout(PATIENCE, stop_run_inner(&deps))
            .await
            .map_err(|_| {
                format!(
                    "stop_run_inner did not come back within {PATIENCE:?}. It waits for the run \
                     to settle, and the run settles on every exit path — a Stop that hangs here \
                     is a run that never said it was over"
                )
            })??;
        let took = pressed.elapsed();

        // (a) Anulowanie jest WARTOŚCIĄ, nie błędem (niezmiennik 7).
        assert_eq!(
            outcome,
            Outcome::Cancelled,
            "stop_run_inner has to come back with Outcome::Cancelled — a value, never \
             Err(Cancelled)"
        );

        // (b) W CHWILI POWROTU grupa jest już dowiedzenie martwa. Ta asercja i kontrola dodatnia
        //     wyżej są razem całą kolejnością: żywa przed Stopem, `ESRCH` w chwili powrotu.
        //     Pomiar jest zrobiony po powrocie i nie ściga się z niczym — jądro nie odpowie
        //     `ESRCH` grupie, w której ktoś jeszcze jest.
        let after = group_probe(group.pgid);
        assert_eq!(
            after.err().and_then(|error| error.raw_os_error()),
            Some(libc::ESRCH),
            "stop_run_inner came back while kill(-{}, 0) still finds somebody in the group. \
             That is the whole defect: the screen says \"stopped\", the agent keeps writing and \
             keeps paying (invariant 6)",
            group.pgid
        );

        // (c) Drugi bok tej samej kolejności, i ten nie da się przejść przypadkiem. Krok
        //     ignoruje SIGTERM, więc ŻADEN uczciwy dowód nie może przyjść wcześniej niż po
        //     oknie łaski: dopiero po nim pada dziewiątka, a dopiero po niej `ESRCH`.
        //     Implementacja, która wysyła sygnał i wraca, wraca natychmiast — i to jest jedyna
        //     rzecz, którą widać z zewnątrz, bo wartość zwracana ma w obu przypadkach tę samą
        //     postać.
        assert!(
            took >= GRACE,
            "stop_run_inner came back after {took:?}, and the step it was stopping ignores \
             SIGTERM: nothing could have proved that group dead before the {GRACE:?} grace \
             window ran out and the escalation reached SIGKILL. A Stop that returns as soon as \
             the signal is away returns in microseconds and looks exactly like this one from \
             the outside (invariant 6)"
        );
        Ok::<(), Box<dyn Error>>(())
    };

    let (ran, checked) = tokio::time::timeout(PATIENCE, async {
        tokio::join!(run_workflow_inner(&deps, &request, sink), watching)
    })
    .await
    .map_err(|_| format!("neither the run nor the stop came back within {PATIENCE:?}"))?;

    checked?;
    let report = ran?;
    assert_eq!(
        report.outcome,
        Outcome::Cancelled,
        "the run itself has to report that a person stopped it, not that it merely ended"
    );

    // Pompa kończy się razem z biegiem; czekamy na nią, żeby zadanie nie przeżyło testu.
    tokio::time::timeout(PATIENCE, pump)
        .await
        .map_err(|_| format!("the pump did not finish within {PATIENCE:?}"))??;
    Ok(())
}

/// Pyta jądro, czy w grupie `pgid` jest jeszcze ktokolwiek — **nie wysyłając sygnału**.
///
/// To jedyny pomiar, który liczy się w niezmienniku 6, i jedyny spoza drzewa naszego procesu:
/// status zebrany przez `wait()` mówi wyłącznie o bezpośrednim dziecku, a zapłacone są wnuki.
// 2026-08-17 — `kill(2)` nie ma bezpiecznego opakowania w std, a `supervisor::reap_group` jest
// `unimplemented!` z powodu opisanego przy niej. Plik testowy jest wyłączony ze wszystkich
// trzech granic architektury po ŚCIEŻCE (checks/quick-boundary.sh), bo nie jest częścią
// wysyłanego artefaktu — a ten test z definicji pyta system operacyjny zamiast naszego kodu
// (niezmiennik 20). Ta sama konstrukcja stoi w tests/supervisor_group_death.rs.
#[allow(unsafe_code)]
fn group_probe(pgid: i32) -> io::Result<()> {
    // SAFETY: `kill` z sygnałem 0 niczego nie dostarcza — sprawdza tylko istnienie i prawa.
    // Argumenty to zwykłe liczby, więc nie ma tu żadnego wskaźnika ani czasu życia do złamania.
    let rc = unsafe { libc::kill(-pgid, 0) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
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
        let seen = *started
            .lock()
            .map_err(|error| anyhow!("the group handoff was poisoned: {error}"))?;
        if let Some(group) = seen {
            return Ok(group);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "no step started a process group within {limit:?}, so there is nothing to stop \
                 and nothing to prove dead. Either the run never reached the driver, or it came \
                 back before it got there"
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
/// To nie jest część kryterium, tylko jego przesłanka. Czerwień w fazie kontraktu wygląda
/// identycznie dla „zachowania jeszcze nie ma" i dla „tego kryterium nie da się spełnić nigdy":
/// workflow, który `workflow::check` odrzuca, byłby odmową w KAŻDEJ implementacji.
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

/// Kanał, który tylko połyka paczki: to kryterium nie pyta o linie, ale szew ma być prawdziwy.
#[derive(Debug, Clone, Default)]
struct Recorder(Arc<Mutex<usize>>);

impl Recorder {
    fn channel(&self) -> Channel<Vec<Line>> {
        let seen = Arc::clone(&self.0);
        Channel::new(move |_body| {
            if let Ok(mut seen) = seen.lock() {
                *seen += 1;
            }
            Ok(())
        })
    }
}

/// Fabryka, która dla każdego vendora oddaje ten sam dubler.
fn fake_drivers(script: PathBuf, ready: PathBuf, started: Arc<Mutex<Option<GroupId>>>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        script,
        ready,
        started,
    });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler sterownika: odpala **prawdziwy** proces we własnej grupie i oddaje jego `pgid`.
///
/// Prawdziwy proces, a nie atrapa, bo przedmiotem tego kryterium jest odpowiedź **jądra**.
/// Zmyślony `GroupProof::Dead` przechodziłby każdą asercję o wartości zwracanej i nie mówiłby
/// nic o tym, czy cokolwiek zginęło.
#[derive(Debug)]
struct Fake {
    /// Skrypt kroku, który nie kończy się sam.
    script: PathBuf,
    /// Plik, którym skrypt melduje, że jego `trap` już stoi.
    ready: PathBuf,
    /// Tędy test dowiaduje się, jaką grupę ma obserwować.
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

        let mut command = tokio::process::Command::new(&self.script);
        command.arg(&self.ready);
        let child = supervisor::spawn(command, StdinPlan::Null)?;
        let group = child.group();
        {
            let mut started = self
                .started
                .lock()
                .map_err(|error| anyhow!("the group handoff was poisoned: {error}"))?;
            *started = Some(group);
        }

        Ok(Box::new(Turn {
            session,
            child,
            group,
        }))
    }
}

/// Jedna tura dublera: żywa grupa procesów, która schodzi wyłącznie przez `cancel`.
#[derive(Debug)]
struct Turn {
    session: SessionRef,
    child: Supervised,
    group: GroupId,
}

#[async_trait]
impl AgentHandle for Turn {
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
        // Krok nie kończy się sam, więc to czekanie kończy dopiero śmierć procesu — czyli
        // eskalacja z `cancel`. Wartość jest tu dla kompletności typu, nie dla kryterium.
        let _status = self.child.wait().await?;
        Ok(TurnOutcome {
            ok: false,
            reason: FinishReason::Cancelled,
            text: String::new(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::ZERO,
            session: self.session.clone(),
        })
    }

    async fn cancel(&mut self) -> GroupProof {
        // Pełna eskalacja z nadzoru: TERM na grupę, okno łaski, KILL na grupę, i dopiero potem
        // dowód. Adapter, który skraca ją do `start_kill`, traci wznawialność sesji [T1 §4.6].
        self.child.stop(GRACE).await
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(None)
    }
}
