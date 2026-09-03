//! Z-01d: po awarii aplikacji odzyskiwanie ma **czego szukać** — i nie tyka cudzego.
//!
//! Z-01c zamknęło wyciek przy ŻYWYM zatrzymaniu: `Supervised::stop` zabija dziś każdą grupę,
//! którą krok utworzył i którą widzi domknięcie po `ppid`. Ten plik jest o drugiej połowie —
//! o tym, co zostaje na dysku, kiedy Loadout ginie, a procesy nie. Zostaje `run.json`, a w nim
//! do niedawna była przy kroku jedna liczba: `pgid` lidera. Wszystko, co krok odpalił we własnej
//! grupie, nie było nigdzie zapisane, więc przy następnym otwarciu folderu nie istniało.
//!
//! # Słaba wersja tego kryterium
//!
//! `assert!(run.json ma pgids)` nad plikiem wpisanym ręką w teście. Przechodzi ją każda
//! implementacja, łącznie z taką, która pola nigdy nie zapisuje — bo to test je zapisał.
//! Dlatego pierwszy test idzie CAŁĄ DROGĄ PRODUKCYJNĄ: prawdziwy `run_workflow_inner`, prawdziwy
//! `stop_run_inner`, `run.json` wygenerowany przez bieg i produkcyjne `reconcile_runs`.
//! Podstawiony jest wyłącznie sterownik vendora, bo w teście nie ma
//! `claude` — i to on odtwarza stan po awarii: startuje przez produkcyjne `spawn_tagged`
//! znacznikiem, **który dostał z `for_step`**, jego proces sadzi potomka we własnej grupie,
//! a zatrzymanie NICZEGO nie zabija, dokładnie tak jak nie zabija zabity Loadout.
//!
//! # Dlaczego nośnikiem potomka jest to samo binarium testowe
//!
//! Bo odzyskiwanie musi umieć **przeczytać środowisko** zastanego procesu, żeby odróżnić własną
//! sierotę od cudzej pracy pod tym samym numerem grupy (`kern.maxproc` = 16 000, więc numery
//! przewijają się w godzinach). Zmierzone na tym macOS: `ps -E` oddaje pełne środowisko procesu
//! uruchomionego z binarium spoza ochrony systemu, ale dla `/bin/sh` i `/bin/sleep` nie pokazuje
//! ani jednej zmiennej. Binarium testowe jest zwykłym plikiem z `target/`, więc jest widoczne —
//! a skrypt powłoki, którego używa `stop_kills_every_group_the_step_started.rs`, nie byłby.
//!
//! Oba procesy pomocnicze są `#[ignore]` i uruchamiane przez `--ignored --exact`, czyli tą samą
//! drogą, którą uruchamia je człowiek. Każdy ma własny sufit czasu, żeby czerwony przebieg nie
//! zostawił po sobie dokładnie tej sieroty, którą ten plik opisuje.

use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::anyhow;
use async_trait::async_trait;
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::reconcile::reconcile_runs;
use loadout_lib::commands::run::{run_workflow_inner, stop_run_inner};
use loadout_lib::commands::{Drivers, Outcome, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::line::{Action, Line, Tool};
use loadout_lib::engine::supervisor::{
    self, GroupId, GroupProof, StdinPlan, StepTag, Supervised, machine_booted_at, run_behind_group,
};
use loadout_lib::ipc::{line_channel, spawn_pump};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use serde_json::Value;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Ile czekamy, aż bieg dojdzie do produkcyjnego szwu, który wydaje znacznik.
///
/// KRÓTKO I OSOBNO OD [`START_LIMIT`], i to jest różnica o czytelność czerwieni (2026-09,
/// Z-01d). Za tą barierą nie stoi ani jeden proces — bieg ma tylko zbudować plan i poskładać
/// sterownik — więc jej przekroczenie ma dokładnie jedną przyczynę: żadna warstwa nie zawołała
/// `for_step`. Wspólna, hojna bariera zamieniała ten przypadek w cichy limit całego testu, a
/// zawieszenie czyta się jak flak, nie jak wada.
const SEAM_LIMIT: Duration = Duration::from_secs(30);

/// Ile czekamy na to, żeby proces kroku wstał i posadził potomka.
///
/// HOJNE Z ROZMYSŁEM, i to niczego nie osłabia: ta bariera jest PRZYGOTOWANIEM, nie pomiarem.
/// Ten test mierzy zawartość `run.json` i odpowiedź jądra po sprzątaniu, a nie to, jak długo
/// wcześniej wstawało drugie binarium testowe pod pełnym obciążeniem maszyny.
const START_LIMIT: Duration = Duration::from_mins(2);

/// Ile czekamy, zanim uznamy bieg albo Stop za zawieszone. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
///
/// MUSI BYĆ WIĘKSZE NIŻ [`START_LIMIT`] (2026-09, Z-01d): ten limit obejmuje obie bariery
/// wewnętrzne, więc krótszy od nich zabiera im głos i każdą ich wadę melduje jako „nic nie
/// wróciło w oknie", czyli zdaniem, które nie mówi, co się zepsuło.
const PATIENCE: Duration = Duration::from_mins(3);

/// Odstęp między pytaniami sondy.
const PROBE_POLL: Duration = Duration::from_millis(20);

/// Pojemność kolejki do pompy.
const ROOMY: usize = 1_024;

/// Ile identycznych porażek jednego narzędzia zatrzymuje krok. Lustro
/// `commands::run::REPEATED_TOOL_FAILURE_LIMIT`; ta stała jest prywatna, a kryterium potrzebuje
/// dokładnie tylu zdarzeń, żeby pompa podniosła awarię.
const REPEATED_FAILURES: u8 = 3;

/// Pełna nazwa procesu-lidera, tak jak widzi ją `--exact`.
const LEADER_TEST: &str = "a_run_leaves_its_groups_where_recovery_can_find_them::a_step_process_that_plants_a_child_in_its_own_group";

/// Pełna nazwa procesu-potomka.
const SURVIVOR_TEST: &str = "a_run_leaves_its_groups_where_recovery_can_find_them::a_child_that_outlives_whoever_started_it";

/// Sufit życia obu procesów pomocniczych. Po nim schodzą same, także wtedy, gdy test padł
/// i nikt ich nie posprzątał.
const HELPER_CEILING: Duration = Duration::from_mins(2);

/// Bieg, do którego potomek z drugiego testu **nie należy**.
const SOMEBODY_ELSES_RUN: &str = "01990000-0000-7000-8000-0000000001dd";

/// Bieg, po którym w drugim teście sprzątamy.
const OUR_RUN: &str = "01990000-0000-7000-8000-0000000001de";

/// Katalog biegu z drugiego testu. Nazwa jest w UTC, tak jak nazwy prawdziwych biegów.
const OUR_FOLDER: &str = "20260903-101500__01990000-0000-7000-8000-0000000001de";

/// Krok drugiego testu.
const OUR_STEP: &str = "01990000-0000-7000-8000-0000000001df";

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000001d1
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
  "id": "wf_leaves_its_groups",
  "name": "One step that plants a child",
  "steps": [
    {
      "kind": "agent",
      "id": "s_hand",
      "name": "Hand",
      "agent": "01990000-0000-7000-8000-0000000001d1",
      "overrides": {},
      "instructions": "plant a child",
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

// ── PROCESY POMOCNICZE ─────────────────────────────────────────────────────────────────────
//
// Oba są `#[ignore]`, więc zwykły przebieg ich nie uruchamia; wywołuje je test wyżej przez
// `--ignored --exact`. Ścieżki rendez-vous liczą sobie same ze znacznika, który dostały
// w środowisku — argumentów podać się nie da (`--exact` przyjmuje nazwę testu, nie ładunek),
// a `spawn_tagged` robi `env_clear()`, więc jedyne, co tu dojeżdża, to lista przepuszczana
// plus sam znacznik.

/// Gdzie proces-lider zostawia numer swojego potomka. `TMPDIR` jest na liście przepuszczanej.
fn rendezvous(run: &str) -> PathBuf {
    std::env::temp_dir().join(format!("loadout-z01d-{run}"))
}

/// Proces kroku: sadzi potomka we **własnej grupie procesów** i zostaje przy życiu.
///
/// `process_group(0)` to `setpgid(0, 0)` w dziecku po `fork` — dokładnie to, co robi każda
/// komenda narzędzia Bash u Claude Code, i dokładnie to, czego `killpg` po `pgid` lidera nigdy
/// nie dosięgnie.
#[test]
#[ignore = "carrier process: the criterion below starts it with --ignored --exact"]
fn a_step_process_that_plants_a_child_in_its_own_group() -> Result<(), Box<dyn Error>> {
    use std::os::unix::process::CommandExt as _;

    let run = std::env::var(supervisor::TAG_RUN)?;
    let dir = rendezvous(&run);
    fs::create_dir_all(&dir)?;

    let mut child = std::process::Command::new(std::env::current_exe()?);
    child.args(["--ignored", "--exact", SURVIVOR_TEST]);
    child.process_group(0);
    let mut planted = child.spawn()?;

    fs::write(dir.join("child.pgid"), planted.id().to_string())?;

    sleep_until_the_ceiling();
    /* ZBIERAMY, ALE NIE ZABIJAMY. `try_wait`, nie `wait`: ten potomek ma nas przeżyć — o to
     * w całym tym pliku chodzi — a czekanie na niego zatrzymałoby lidera do jego własnego
     * sufitu. Bez tej linii clippy słusznie mówi, że nikt nigdy nie zbierze tego dziecka. */
    let _left_running = planted.try_wait()?;
    Ok(())
}

/// Potomek: nie robi nic poza tym, że **żyje** i niesie w środowisku znacznik swojego biegu.
#[test]
#[ignore = "carrier process: the criteria below start it with --ignored --exact"]
fn a_child_that_outlives_whoever_started_it() {
    sleep_until_the_ceiling();
}

/// Wspólny sufit obu procesów pomocniczych.
fn sleep_until_the_ceiling() {
    let deadline = Instant::now() + HELPER_CEILING;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
}

// ── KRYTERIUM 1 ────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pgids_from_a_stopped_step_let_the_reaper_kill_the_survivor() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let hand = bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("leaves-its-groups", WORKFLOW)?;
    the_fixture_can_run(&workflow, &[&hand])?;
    let store = Store::open(&bench.db())?;

    let seen: Arc<Mutex<Seen>> = Arc::new(Mutex::new(Seen::default()));
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&seen)),
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

    let (sink, source) = line_channel(ROOMY);
    let pump = spawn_pump(source, swallowing_channel());

    let watching = async {
        let found = async {
            // Znacznik przyjechał z produkcji, przez `AgentDriver::for_step` — nie zbudował go
            // test. Własna, krótka bariera: jej przekroczenie ma jedną przyczynę i ma ją nazwać.
            let tag = wait_for_tag(&seen, SEAM_LIMIT).await?;
            let survivor = wait_for_child_group(tag.run(), START_LIMIT).await?;

            // ── Kontrola dodatnia ─────────────────────────────────────────────────────────
            // Bez niej `ESRCH` na końcu znaczy równie dobrze „procesu nigdy nie było", a całe
            // kryterium przechodzi na pustym zbiorze.
            assert!(
                group_probe(survivor).is_ok(),
                "kill(-{survivor}, 0) does not find the planted child even before Stop, so \
                 nothing measured below would be about a process that outlived the run"
            );
            Ok::<i32, Box<dyn Error>>(survivor)
        }
        .await;

        /* STOP IDZIE TAKŻE PO BŁĘDZIE, i to jest o czytelności czerwieni (2026-09, Z-01d): ten
         * bieg nie kończy się sam, a `join!` niżej czeka na OBIE połowy. Wyjście stąd przez `?`
         * bez zatrzymania biegu zamieniało każdą wadę wykrytą wyżej w limit całego testu — czyli
         * w zdanie „nic nie wróciło", które nie mówi, co się zepsuło. */
        let stopped = tokio::time::timeout(PATIENCE, stop_run_inner(&deps))
            .await
            .map_err(|_| format!("stop_run_inner did not come back within {PATIENCE:?}"));
        let survivor = found?;
        assert_eq!(stopped??, Outcome::Cancelled);
        Ok::<i32, Box<dyn Error>>(survivor)
    };

    let (ran, watched) = tokio::time::timeout(PATIENCE, async {
        tokio::join!(run_workflow_inner(&deps, &request, sink), watching)
    })
    .await
    .map_err(|_| format!("neither the run nor the stop came back within {PATIENCE:?}"))?;
    let survivor = watched?;
    ran?;
    tokio::time::timeout(PATIENCE, pump)
        .await
        .map_err(|_| format!("the pump did not finish within {PATIENCE:?}"))??;

    // ── (a) Bieg WYGENEROWAŁ plik, który niesie grupę potomka ─────────────────────────────
    let dir = the_only_run_dir(bench.project.path())?;
    let run: Value = serde_json::from_slice(&fs::read(dir.join("run.json"))?)?;
    let run_id = run["id"]
        .as_str()
        .ok_or("the generated run.json has no run id")?
        .to_owned();
    let step = run["steps"]
        .as_array()
        .and_then(|steps| steps.first())
        .ok_or("the generated run.json has no steps")?;
    let written: Vec<i64> = step["pgids"]
        .as_array()
        .map(|list| list.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default();
    assert!(
        written.contains(&i64::from(survivor)),
        "run.json says pgids {written:?} and the child the step planted sits in group {survivor}. \
         Without that number the next start has nothing to look for: the leader's own pgid is \
         the only thing recovery ever saw, and everything the step ran in a group of its own \
         outlives it invisibly (invariant 6)"
    );

    // ── (b) Znacznik w środowisku ŻYWEGO potomka jest identyfikatorem BIEGU, co do bajta ──
    // To jest ta pomyłka, na której padło poprzednie podejście: znacznik niósł identyfikator
    // sesji vendora, a odzyskiwanie porównywało go z `run.json` → `id`. Te dwie wartości nigdy
    // nie są równe, więc własna żywa grupa wychodziła obca i nie dostawała sygnału w ogóle.
    assert_eq!(
        run_behind_group(survivor).as_deref(),
        Some(run_id.as_str()),
        "ps -E says the surviving group belongs to {:?}, and the generated run.json calls this \
         run {run_id:?}. Recovery compares exactly these two strings, so a mismatch means it \
         treats its own live group as somebody else's and never signals it",
        run_behind_group(survivor)
    );

    // ── (c) Ocalały żył PRZED sprzątaniem i zginął DOPIERO od niego ───────────────────────
    assert!(
        group_probe(survivor).is_ok(),
        "the planted child was already gone before reconcile_runs ran, so the next assertion \
         would prove nothing"
    );
    let reconciled = reconcile_runs(bench.project.path());
    assert_eq!(
        group_probe(survivor)
            .err()
            .and_then(|error| error.raw_os_error()),
        Some(libc::ESRCH),
        "reopening the folder ran the production cleanup and kill(-{survivor}, 0) still finds \
         somebody in the group. That is the sixteen-hour leak from ../meetnotes verbatim: the \
         run reads as closed and the process keeps burning quota. Cleanup reported {reconciled:?}"
    );
    Ok(())
}

// ── KRYTERIUM 1b ───────────────────────────────────────────────────────────────────────────

/// Krok schodzący **bez Stopu** też zapisuje swoje grupy.
///
/// # Po co to jest osobno od kryterium wyżej (2026-09, Z-01d)
///
/// Bo `pgids` mają się zapisać na KAŻDEJ drodze zejścia, a tamto kryterium chodzi wyłącznie
/// drogą Stopu. Dwie gałęzie `finish_agent_turn` — powtórzona awaria narzędzia i tura, która
/// wróciła błędem — brały dowód śmierci i zapisywały go bez zapisu grup, więc ocalały wnuk we
/// własnej grupie nie miał jak trafić do pliku. Bramka była zielona, bo żaden test tamtędy nie
/// szedł: to jest dokładnie ta klasa wady, dla której istnieje niezmiennik 20 („test sprawdza
/// zachowanie, nie obecność stringa") i dla której obie gałęzie mają tu własny przebieg.
///
/// Sprzątania nie powtarzamy — dowodzi go kryterium wyżej. Tu pytamy o jedną rzecz: czy numer
/// grupy potomka stoi w **wygenerowanym** `run.json`, bo bez niego reaper nie ma czego szukać.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_way_a_step_goes_down_writes_the_groups_it_left() -> Result<(), Box<dyn Error>> {
    for ends in [
        HowTheTurnEnds::TurnBreaks,
        HowTheTurnEnds::TheSameToolKeepsFailing,
    ] {
        let (run_id, survivor, written) = a_run_that_goes_down(ends).await?;
        assert!(
            written.contains(&i64::from(survivor)),
            "the step went down by {ends:?} and run.json says pgids {written:?}, but the child \
             it planted sits in group {survivor}. Every way down has to write the same thing: a \
             group missing from the file is a group the next start cannot even ask about \
             (invariant 6). The run was {run_id}"
        );
        // Ocalałego sprzątamy sami: to kryterium jest o zapisie, a nie o eskalacji, a wnuk
        // zostawiony żywy przeżyłby ten test o dwie minuty.
        let _ = supervisor::reap_group(survivor);
    }
    Ok(())
}

/// Prowadzi jeden prawdziwy bieg do końca wybraną drogą i oddaje to, co zapisał `run.json`.
///
/// Oddaje `(id biegu, grupa potomka, pgids ze wygenerowanego pliku)`.
async fn a_run_that_goes_down(
    ends: HowTheTurnEnds,
) -> Result<(String, i32, Vec<i64>), Box<dyn Error>> {
    let bench = Bench::new()?;
    let hand = bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("goes-down", WORKFLOW)?;
    the_fixture_can_run(&workflow, &[&hand])?;
    let store = Store::open(&bench.db())?;

    let seen: Arc<Mutex<Seen>> = Arc::new(Mutex::new(Seen::default()));
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: drivers_that_end(Arc::clone(&seen), ends),
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

    let (sink, source) = line_channel(ROOMY);
    let pump = spawn_pump(source, swallowing_channel());
    /* BEZ STOPU I BEZ OBSERWATORA: ten bieg schodzi sam, drogą wybraną w dublerze. To jest cała
     * różnica wobec kryterium wyżej — tam zejście zamawia człowiek, tutaj bierze się z tego, co
     * powiedział sterownik, i przechodzi przez inne gałęzie `finish_agent_turn`. */
    tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))??;
    tokio::time::timeout(PATIENCE, pump)
        .await
        .map_err(|_| format!("the pump did not finish within {PATIENCE:?}"))??;

    let tag = wait_for_tag(&seen, SEAM_LIMIT).await?;
    let survivor = wait_for_child_group(tag.run(), START_LIMIT).await?;

    let dir = the_only_run_dir(bench.project.path())?;
    let run: Value = serde_json::from_slice(&fs::read(dir.join("run.json"))?)?;
    let run_id = run["id"]
        .as_str()
        .ok_or("the generated run.json has no run id")?
        .to_owned();
    let step = run["steps"]
        .as_array()
        .and_then(|steps| steps.first())
        .ok_or("the generated run.json has no steps")?;
    let written: Vec<i64> = step["pgids"]
        .as_array()
        .map(|list| list.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default();
    Ok((run_id, survivor, written))
}

// ── KRYTERIUM 2 ────────────────────────────────────────────────────────────────────────────

// Asynchroniczny, choć nic tu nie czeka na nic: `supervisor::spawn_tagged` idzie przez
// `tokio::process`, a ten żąda żywego reaktora już przy zakładaniu potoków. Produkcja woła go
// wyłącznie z runtime'u, więc test bez niego mierzyłby drogę, której nie ma.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_group_marked_for_another_run_survives_and_says_so_in_history()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let project = root.path();

    // Żywa grupa oznaczona CUDZYM biegiem — przez produkcyjny `spawn_tagged`, bo tylko on
    // wpuszcza znacznik do środowiska.
    let mine = std::env::current_exe()?;
    let mut command = tokio::process::Command::new(mine);
    command.args(["--ignored", "--exact", SURVIVOR_TEST]);
    let tag = StepTag::new(SOMEBODY_ELSES_RUN, "somebody-elses-step");
    let stranger = supervisor::spawn_tagged(command, StdinPlan::Null, &[], Some(&tag))?;
    let pgid = stranger.group().pgid;

    // ── Kontrola dodatnia ─────────────────────────────────────────────────────────────────
    // Bez niej „grupa przeżyła" znaczy równie dobrze „nikt jej nie znalazł" — i to samo zdanie
    // byłoby prawdą nad implementacją, która nie sprząta nigdy.
    let behind = wait_for_marker(pgid, START_LIMIT);
    assert_eq!(
        behind.as_deref(),
        Some(SOMEBODY_ELSES_RUN),
        "ps -E does not report the run marker of the group we just started, so nothing below \
         would be about telling one run's group from another's. It said {behind:?}"
    );

    let boot = machine_booted_at().unwrap_or_default();
    put_run(
        project,
        OUR_FOLDER,
        &a_run_that_left_a_stranger_behind(&boot, pgid),
    )?;

    let reconciled = reconcile_runs(project);

    // ── (a) Cudza grupa przeżywa ──────────────────────────────────────────────────────────
    assert!(
        group_probe(pgid).is_ok(),
        "cleanup signalled a group whose marker names a different run. Any Some(...) is not the \
         same answer as this run's id, and killing on the first is how a stranger's work dies \
         under a recycled group number. Cleanup reported {reconciled:?}"
    );

    // ── (b) Człowiek czyta o tym tą samą drogą, którą czyta okno (niezmiennik 29) ─────────
    let past = read_run_inner(project, OUR_FOLDER)?;
    let said = past
        .steps
        .iter()
        .find(|one| one.id == OUR_STEP)
        .ok_or("history omitted the step whose group was left alone")?
        .error
        .clone();
    let lowered = said.to_ascii_lowercase();
    assert!(
        lowered.contains("different run") && lowered.contains("left it alone"),
        "PastStepWire.error -- the one field history renders -- does not tell the person that \
         this group belongs to another run and was not touched. It says: {said:?}"
    );
    assert!(
        said.contains(&format!("PGID {pgid}")),
        "the sentence does not name the group the person would look for in ps: {said:?}"
    );

    drop(stranger);
    Ok(())
}

// ── KRYTERIUM 3 ────────────────────────────────────────────────────────────────────────────

/// Drogi, którymi krok uruchamia proces **bez CLI vendora** — plus zapora przed nadpisaniem.
///
/// Tury Claude'a i Codeksa oraz App Servera nie da się tu zmierzyć, bo na maszynie testowej nie
/// ma żadnego z tych CLI; wszystkie trzy idą jednak przez tę samą jedną funkcję
/// ([`supervisor::spawn_tagged`]) i dostają znacznik z pola sterownika, a `spawn` i
/// `spawn_with_environment` są dziś jej opakowaniami z jawnym `None`. Tutaj stoją te dwie drogi,
/// które da się uruchomić naprawdę, plus sonda, która znacznika dostać NIE ma.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_way_a_step_starts_a_process_carries_the_run_marker() -> Result<(), Box<dyn Error>> {
    use loadout_lib::commands::processes::Processes;
    use loadout_lib::engine::drivers::command::{CheckSpec, CommandDriver, StartSpec};

    let dir = tempfile::tempdir()?;
    let mine = std::env::current_exe()?;
    let line = format!("{} --ignored --exact {SURVIVOR_TEST}", mine.display());
    let tag = StepTag::new(OUR_RUN, OUR_STEP);

    // ── (a) Krok „sprawdź" ────────────────────────────────────────────────────────────────
    let mut checking = CommandDriver::new()
        .for_step(tag.clone())
        .start(&CheckSpec {
            command: line.clone(),
            proof: "never matches".to_owned(),
            cwd: dir.path().to_path_buf(),
        })?;
    let checked = wait_for_marker(checking.group().pgid, START_LIMIT);
    let _checked_proof = checking.cancel().await;
    assert_eq!(
        checked.as_deref(),
        Some(OUR_RUN),
        "the check step started a process that does not say which run it belongs to, so after a \
         crash nothing tells its group apart from a stranger's under the same recycled number"
    );

    // ── (b) Kafelek „uruchom i zostaw" ────────────────────────────────────────────────────
    // TA DROGA ZGUBIŁA ZNACZNIK W POPRZEDNIM PODEJŚCIU: idzie przez `Processes::start` →
    // `start_to_stay`, czyli obok tej jednej funkcji, w której znacznik wtedy stał.
    let processes = Processes::new();
    let started = processes.start(
        &StartSpec {
            command: line,
            cwd: dir.path().to_path_buf(),
        },
        Some(tag.clone()),
    )?;
    let staying = wait_for_marker(started.pgid, START_LIMIT);
    let _stopped = processes.stop(started.pgid).await;
    assert_eq!(
        staying.as_deref(),
        Some(OUR_RUN),
        "the run-and-leave tile started a process with no run marker. That process is meant to \
         outlive its step, so it is exactly the one recovery finds after a crash"
    );

    // ── (c) Zatwierdzone Połączenie o tej samej nazwie NIE nadpisuje znacznika ────────────
    // Znacznik stoi za pętlą po zmiennych z Połączeń. Postawiony przed nią przegrywa z tą jedną
    // nazwą, a odzyskiwanie porównuje właśnie ją.
    let mut command = tokio::process::Command::new(std::env::current_exe()?);
    command.args(["--ignored", "--exact", SURVIVOR_TEST]);
    let hijack = vec![(
        supervisor::TAG_RUN.to_owned(),
        std::ffi::OsString::from(SOMEBODY_ELSES_RUN),
    )];
    let mut guarded = supervisor::spawn_tagged(command, StdinPlan::Null, &hijack, Some(&tag))?;
    let guarded_pgid = guarded.group().pgid;
    let said = wait_for_marker(guarded_pgid, START_LIMIT);
    let _guarded_proof = guarded.stop(Duration::from_secs(1)).await;
    assert_eq!(
        said.as_deref(),
        Some(OUR_RUN),
        "an approved Connection carrying a variable of the same name overwrote the run marker, \
         so this run's own group would read as {SOMEBODY_ELSES_RUN}'s and never be signalled"
    );

    // ── (d) Sonda vendora znacznika NIE dostaje, i mówi to wprost ─────────────────────────
    let mut probe = tokio::process::Command::new(std::env::current_exe()?);
    probe.args(["--ignored", "--exact", SURVIVOR_TEST]);
    let mut unmarked = supervisor::spawn_tagged(probe, StdinPlan::Null, &[], None)?;
    let unmarked_pgid = unmarked.group().pgid;
    // Kontrola dodatnia: proces musi ŻYĆ, zanim brak znacznika cokolwiek znaczy. Bez niej
    // „nie znaleziono znacznika" jest też prawdą o procesie, który nigdy nie wstał.
    assert!(
        group_probe(unmarked_pgid).is_ok(),
        "the unmarked process was gone before it could be asked about, so its silence proves \
         nothing"
    );
    let nothing = wait_for_marker(unmarked_pgid, Duration::from_millis(300));
    let _unmarked_proof = unmarked.stop(Duration::from_secs(1)).await;
    assert_eq!(
        nothing, None,
        "a process started without a step behind it carries a run marker anyway, so recovery \
         would treat the version probe as an orphan of some run and kill it"
    );
    Ok(())
}

// ── KRYTERIUM 4 ────────────────────────────────────────────────────────────────────────────

/// Fałszywe CLI vendora, które **melduje własne środowisko**.
///
/// Znacznik czytamy z dziecka, nie z `ps -E`, i to jest tu mocniejsze, nie słabsze: proces sam
/// zapisuje, co odziedziczył, więc odpowiedź nie zależy od tego, czy jądro pokazuje środowisko
/// binarium pod ochroną systemu. Reszta kształtu jest minimalna — jedna linia `system/init`
/// i jedna odpowiedź na kopertę — bo to kryterium pyta wyłącznie o zmienną.
const VENDOR_THAT_REPORTS_ITS_ENVIRONMENT: &str = r#"#!/bin/sh
if [ "${1-}" = "--version" ]; then
  printf '%s\n' '2.1.238 (Claude Code)'
  exit 0
fi
here="$(dirname "$0")"
printf '%s' "${LOADOUT_RUN-}" > "$here/marker.run"
printf '%s' "${LOADOUT_STEP-}" > "$here/marker.step"
printf '%s\n' '{"type":"system","subtype":"init","session_id":"lead-tagging","model":"sonnet","tools":[]}'
while IFS= read -r line; do
  printf '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"done"}\n'
done
exit 0
"#;

/// Rozmowa lidera też jest procesem, który przeżywa awarię — więc też niesie znacznik.
///
/// # Po co to jest osobnym kryterium (2026-09, Z-01d)
///
/// Bo `begin_thread` jest DRUGĄ produkcyjną drogą do vendora i ma własne opakowania sterownika.
/// Kryterium biegu ich nie dotyka: tam znacznik zakłada `commands::run`, tutaj `commands::chat`,
/// i jedno może działać przy drugim martwym. Dokładnie tak było do dziś — droga biegu znacznik
/// zakładała, droga rozmowy nie wołała `for_step` ani razu.
///
/// Idzie przez okno (`say_to_orchestrator_from_window` → `AppState` → aktor rozmowy →
/// `begin_thread`) i przez **produkcyjny** dekorator z fabryki, więc mierzy całą drogę, nie sam
/// adapter vendora. Codex wybiera na tej drodze App Server, Claude zwykły proces; znacznik
/// zakłada `begin_thread` PRZED `start_conversation`, czyli w jednym miejscu dla obu.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lead_conversation_starts_its_vendor_with_a_marker() -> Result<(), Box<dyn Error>> {
    use loadout_lib::commands::agents::save_agent_inner;
    use loadout_lib::engine::drivers::claude::ClaudeDriver;
    use loadout_lib::ipc::{AppState, QUEUE_CAP, say_to_orchestrator_from_window};
    use loadout_lib::library::agents::{Agent, Vendor};

    let library = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    fs::create_dir_all(workspace.path().join(".loadout"))?;
    let binary = executable(
        fixture.path(),
        "claude",
        VENDOR_THAT_REPORTS_ITS_ENVIRONMENT,
    )?;

    let inner: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(binary));
    // Przez ten sam dekorator, co u człowieka — powód w całości przy `fake_drivers`.
    let driver =
        loadout_lib::driver_with_frozen_search(inner, std::env::var_os("PATH").unwrap_or_default());
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));

    let store = Store::open(&workspace.path().join(".loadout/loadout.db"))?;
    let state = AppState::new(
        library.path().to_path_buf(),
        workspace.path().to_path_buf(),
        store,
        drivers,
    );
    let mut lead = Agent::example();
    lead.id = uuid::Uuid::now_v7();
    lead.name = "Marker Lead".to_owned();
    lead.runs_with = Vendor::ClaudeCode;
    save_agent_inner(library.path(), &lead, None)?;

    let folder = workspace.path().to_string_lossy().into_owned();
    let (lines, _source) = line_channel(QUEUE_CAP);
    state.watching_the_lead("product-marker", Some(&folder), lines)?;
    say_to_orchestrator_from_window(
        &state,
        "product-marker",
        Some(&folder),
        Some(&lead.id.to_string()),
        "say something",
        Vec::new(),
    )
    .await?;

    // Proces vendora startuje w aktorze rozmowy, więc plik pojawia się chwilę po powrocie stąd.
    // Bariera przygotowania, nie pomiar — ale bez niej „brak znacznika" znaczy też „jeszcze nie
    // zdążył", czyli czerwień, która nie mówi, co się zepsuło.
    /* NA TREŚĆ, NIE NA ISTNIENIE PLIKU, i to jest różnica zmierzona jako flak (2026-09, Z-01d):
     * `printf` tworzy plik, zanim wpisze do niego bajty, więc czekanie na `exists()` łapało
     * pustą chwilę i meldowało brak znacznika nad procesem, który go właśnie dostawał. Pomiar
     * jest ten sam — po tej pętli pusto znaczy pusto — a bariera przestaje udawać wadę. */
    let marker = fixture.path().join("marker.run");
    let deadline = Instant::now() + SEAM_LIMIT;
    let mut said = String::new();
    while said.trim().is_empty() && Instant::now() < deadline {
        said = fs::read_to_string(&marker).unwrap_or_default();
        if said.trim().is_empty() {
            tokio::time::sleep(PROBE_POLL).await;
        }
    }
    let ran: Vec<String> = fs::read_dir(fixture.path())?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        marker.exists(),
        "the lead's vendor process never started at all, so this criterion measured nothing. \
         The fixture directory holds {ran:?}"
    );
    assert!(
        !said.trim().is_empty(),
        "the lead's vendor process started with no {} in its environment, so after a crash its \
         group is indistinguishable from a stranger's under a recycled number. begin_thread is \
         where the marker goes on, and it is the same call site the Codex App Server uses",
        supervisor::TAG_RUN
    );
    // Druga zmienna leci tym samym `printf`-em zaraz po pierwszej, więc dostaje tę samą barierę
    // na treść — z tego samego powodu, co wyżej.
    let step_marker = fixture.path().join("marker.step");
    let deadline = Instant::now() + SEAM_LIMIT;
    let mut step = String::new();
    while step.trim().is_empty() && Instant::now() < deadline {
        step = fs::read_to_string(&step_marker).unwrap_or_default();
        if step.trim().is_empty() {
            tokio::time::sleep(PROBE_POLL).await;
        }
    }
    assert_eq!(
        step.trim(),
        "lead-conversation",
        "the step slot has to say plainly that this is not a step of any run, so nobody reading \
         ps goes looking for a tile with this number in the history"
    );
    Ok(())
}

/// Zapisuje wykonywalny plik i oddaje jego ścieżkę.
fn executable(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    use std::os::unix::fs::PermissionsExt as _;
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

// ── NARZĘDZIA ──────────────────────────────────────────────────────────────────────────────

/// Pyta jądro, czy w grupie `pgid` jest jeszcze ktokolwiek — **nie wysyłając sygnału**.
///
/// To jedyny pomiar, który liczy się w niezmienniku 6, i jedyny spoza drzewa naszego procesu.
// 2026-09 — `kill(2)` nie ma bezpiecznego opakowania w std. Plik testowy jest wyłączony ze
// wszystkich trzech granic architektury po ŚCIEŻCE (checks/boundary.sh), bo nie jest częścią
// wysyłanego artefaktu — a ten test z definicji pyta system operacyjny zamiast naszego kodu
// (niezmiennik 20). Ta sama konstrukcja stoi w tests/it/run_stop_waits_for_proof.rs.
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

/// Czeka, aż `ps` zacznie pokazywać znacznik świeżo uruchomionej grupy.
///
/// Sonda w pętli, bo między `spawn` a pierwszym wierszem w `ps` jest okno kilkunastu
/// milisekund, a to jest przygotowanie, nie pomiar.
fn wait_for_marker(pgid: i32, limit: Duration) -> Option<String> {
    let deadline = Instant::now() + limit;
    loop {
        if let Some(run) = run_behind_group(pgid) {
            return Some(run);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(PROBE_POLL);
    }
}

/// Czeka, aż sterownik dostanie znacznik z produkcji.
async fn wait_for_tag(seen: &Mutex<Seen>, limit: Duration) -> Result<StepTag, Box<dyn Error>> {
    let deadline = Instant::now() + limit;
    loop {
        // Zamek brany i oddany w jednym wyrażeniu: między nim a `await` niżej nie ma ani jednej
        // instrukcji (niezmiennik 8).
        let tag = seen
            .lock()
            .map_err(|error| anyhow!("the handoff was poisoned: {error}"))?
            .tag
            .clone();
        if let Some(tag) = tag {
            return Ok(tag);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "no step ever asked the driver for a run marker within {limit:?}, so the run \
                 never reached the production seam that carries it"
            )
            .into());
        }
        tokio::time::sleep(PROBE_POLL).await;
    }
}

/// Czeka, aż proces kroku zamelduje grupę swojego potomka.
async fn wait_for_child_group(run: &str, limit: Duration) -> Result<i32, Box<dyn Error>> {
    let handover = rendezvous(run).join("child.pgid");
    let deadline = Instant::now() + limit;
    loop {
        if let Ok(said) = fs::read_to_string(&handover)
            && let Ok(pgid) = said.trim().parse::<i32>()
        {
            return Ok(pgid);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "the step process never reported a child group within {limit:?}; without one \
                 there is no group outside the leader's for this criterion to be about"
            )
            .into());
        }
        tokio::time::sleep(PROBE_POLL).await;
    }
}

/// Jedyny katalog biegu tego folderu.
fn the_only_run_dir(project: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let mut found: Vec<PathBuf> = fs::read_dir(project.join(".loadout").join("runs"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    found.sort();
    match found.len() {
        1 => Ok(found.remove(0)),
        other => {
            Err(format!("this criterion expects exactly one run directory, found {other}").into())
        }
    }
}

fn put_run(project: &Path, folder: &str, text: &str) -> Result<(), Box<dyn Error>> {
    let dir = project.join(".loadout").join("runs").join(folder);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("run.json"), text)?;
    Ok(())
}

/// Bieg zamknięty uczciwie jako `cancelled`, po którym została jedna grupa bez dowodu śmierci.
///
/// Bez `pgid` i wyłącznie z `pgids`, bo to jest to pole, które ten test sądzi: implementacja
/// czytająca dalej sam `pgid` nie zobaczy tu żadnej grupy i nie zapyta o nią nikogo.
fn a_run_that_left_a_stranger_behind(boot: &str, pgid: i32) -> String {
    format!(
        r#"{{
  "id": "{OUR_RUN}",
  "workflow_id": "z01d.json",
  "workflow_hash": "z01d-hash",
  "workflow_snapshot": {{ "format": 1 }},
  "title": "Stopped by a person",
  "status": "cancelled",
  "concurrency": 1,
  "created_at": 1788000000000,
  "boot_id": "{boot}",
  "started_at": 1788000001000,
  "ended_at": 1788000002000,
  "error": null,
  "steps": [
    {{
      "id": "{OUR_STEP}",
      "node_key": "hand",
      "name": "Hand",
      "agent": "claude",
      "kind": "agent",
      "depends_on": [],
      "status": "failed",
      "attempt": 0,
      "agent_session_id": "a-session",
      "pid": null,
      "pgid": null,
      "pgids": [{pgid}],
      "death_proof": false,
      "started_at": 1788000001000,
      "ended_at": 1788000002000,
      "error": null
    }}
  ]
}}
"#
    )
}

/// Fikstura ma przejść walidator **bez ani jednego problemu**, a jej plik agenta ma dać się
/// przeczytać. To nie jest część kryterium, tylko jego przesłanka.
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
fn swallowing_channel() -> Channel<Vec<Line>> {
    Channel::new(|_body| Ok(()))
}

/// Co test podpatrzył na produkcyjnej drodze.
#[derive(Debug, Default)]
struct Seen {
    /// Znacznik, o który bieg poprosił sterownik. Test go **nie buduje** — to jest cała różnica
    /// między „ta funkcja umie ustawić zmienną" a „produkcja ustawia w niej właściwą wartość".
    tag: Option<StepTag>,
}

/// Czym kończy się tura dublera — czyli KTÓRĄ drogą zejścia schodzi krok.
///
/// 2026-09 (Z-01d) — pokrętło, a nie trzy osobne duble, bo tu chodzi dokładnie o to, że wszystkie
/// te drogi mają robić TO SAMO. Weryfikator znalazł lukę właśnie tam, gdzie kryterium chodziło
/// wyłącznie drogą Stopu: dwie gałęzie `finish_agent_turn` zapisywały dowód śmierci bez zapisu
/// grup, więc ocalały wnuk nie miał jak trafić do `run.json` (`AGENTS.md` niezmiennik 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HowTheTurnEnds {
    /// Krok nie kończy się sam. Schodzi dopiero Stopem człowieka (`Ended::Stopped`).
    OnlyWhenStopped,
    /// Tura wraca błędem sterownika (`Ended::Turn(Err)`).
    TurnBreaks,
    /// To samo narzędzie pada trzy razy z tym samym wyjściem (`Ended::RepeatedToolFailure`).
    TheSameToolKeepsFailing,
}

/// Fabryka, która dla każdego vendora oddaje ten sam dubler.
fn fake_drivers(seen: Arc<Mutex<Seen>>) -> Drivers {
    drivers_that_end(seen, HowTheTurnEnds::OnlyWhenStopped)
}

/// To samo, z wybraną drogą zejścia.
fn drivers_that_end(seen: Arc<Mutex<Seen>>, ends: HowTheTurnEnds) -> Drivers {
    /* PRZEZ PRODUKCYJNY DEKORATOR, nie obok niego (2026-09, Z-01d). Bieg u człowieka nigdy nie
     * dostaje sterownika vendora wprost: fabryka `agent_drivers_with_search` opakowuje oba
     * w `SearchEnvironmentDriver`, a `commands::run` woła wszystkie szwy traitu WŁAŚNIE na tym
     * opakowaniu. Szew, którego ono nie deleguje, jest w produkcji martwy przy w pełni poprawnym
     * adapterze pod spodem — i dokładnie tak zniknął znacznik przy pierwszym podejściu, przy
     * zielonej bramce. Kryterium, które podstawia dubler OBOK dekoratora, sądzi inny kształt niż
     * ten, który biegnie; ta jedna linia jest całą różnicą.
     *
     * `PATH` procesu testowego, bo dekorator dokłada go do Połączeń, a nie do znacznika: gdyby
     * była tu ścieżka zmyślona, ten test mierzyłby też odkrywanie CLI, którego nie dotyczy. */
    let inner: Arc<dyn AgentDriver> = Arc::new(Fake {
        seen,
        tag: None,
        ends,
    });
    let driver =
        loadout_lib::driver_with_frozen_search(inner, std::env::var_os("PATH").unwrap_or_default());
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler sterownika, który odtwarza stan po awarii aplikacji.
///
/// Prawdziwy proces, prawdziwa grupa i prawdziwe `spawn_tagged` — bo przedmiotem tego kryterium
/// jest odpowiedź **jądra** i zawartość pliku, który bieg naprawdę zapisał. Jedyne, co jest tu
/// udawane, to zachowanie zatrzymania: `cancel()` nie eskaluje niczego i oddaje `Alive`,
/// dokładnie jak nie eskaluje nic zabity Loadout.
#[derive(Debug)]
struct Fake {
    seen: Arc<Mutex<Seen>>,
    tag: Option<StepTag>,
    ends: HowTheTurnEnds,
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

    fn for_step(&self, tag: &StepTag) -> Option<Arc<dyn AgentDriver>> {
        if let Ok(mut seen) = self.seen.lock() {
            seen.tag = Some(tag.clone());
        }
        Some(Arc::new(Self {
            seen: Arc::clone(&self.seen),
            tag: Some(tag.clone()),
            ends: self.ends,
        }))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };
        let mine = std::env::current_exe()?;
        let mut command = tokio::process::Command::new(mine);
        command.args(["--ignored", "--exact", LEADER_TEST]);
        // PRODUKCYJNY SPAWN, ZE ZNACZNIKIEM Z `for_step`. Znacznik zbudowany tutaj z `spec` byłby
        // dokładnie tą zieloną atrapą, przez którą poprzednie podejście nie zauważyło, że produkcja
        // wpisuje do niego identyfikator sesji zamiast identyfikatora biegu.
        let child = supervisor::spawn_tagged(command, StdinPlan::Null, &[], self.tag.as_ref())?;
        let group = child.group();
        Ok(Box::new(Turn {
            session,
            child,
            group,
            ends: self.ends,
            events,
            planted: self.tag.as_ref().map(|tag| rendezvous(tag.run())),
        }))
    }
}

/// Jedna tura dublera: żywa grupa, której zatrzymanie niczego nie zabija.
#[derive(Debug)]
struct Turn {
    session: SessionRef,
    child: Supervised,
    group: GroupId,
    /// Którą drogą ta tura schodzi — powód przy [`HowTheTurnEnds`].
    ends: HowTheTurnEnds,
    /// Kanał zdarzeń tej sesji. Trzymany, bo droga „to samo narzędzie pada trzy razy" powstaje
    /// wyłącznie z tego, co pompa policzy na tym kanale — nie da się jej zamówić inaczej.
    events: mpsc::Sender<DecodedEvent>,
    /// Katalog, w którym proces kroku melduje grupę swojego potomka. `None`, gdy sterownik nie
    /// dostał znacznika, czyli gdy nie ma jak policzyć tej ścieżki.
    planted: Option<PathBuf>,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        Some(self.group)
    }

    fn descendant_groups(&mut self) -> Vec<i32> {
        // PRODUKCYJNY PRZEGLĄD DRZEWA, nie lista zapamiętana przez test: gdyby test podał tu
        // numer, który sam zna, kryterium mówiłoby o tym, że `run.json` umie zapisać liczbę,
        // a nie o tym, że Loadout umie ją znaleźć.
        self.child.descendant_groups()
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        /* POTOMEK MUSI JUŻ STAĆ, ZANIM TURA ZEJDZIE (2026-09, Z-01d). Grupy czyta produkcyjny
         * przegląd drzewa w gałęzi zejścia, a on widzi wyłącznie to, co istnieje w chwili
         * pytania: tura, która pada przed posadzeniem wnuka, mierzyłaby pusty zbiór i zieleniła
         * się nad implementacją, która nie zapisuje niczego. */
        if self.ends != HowTheTurnEnds::OnlyWhenStopped {
            let planted = self
                .planted
                .clone()
                .ok_or_else(|| anyhow::anyhow!("the driver never got a marker to plant under"))?;
            let deadline = Instant::now() + START_LIMIT;
            while !planted.join("child.pgid").exists() && Instant::now() < deadline {
                tokio::time::sleep(PROBE_POLL).await;
            }
        }
        match self.ends {
            HowTheTurnEnds::OnlyWhenStopped => {
                // Krok nie kończy się sam, więc to czekanie kończy dopiero Stop.
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
            // Awaria sterownika w środku tury: `commands::run` czyta to jako `Ended::Turn(Err)`.
            HowTheTurnEnds::TurnBreaks => Err(anyhow::anyhow!(
                "the agent stopped in the middle of its turn"
            )),
            HowTheTurnEnds::TheSameToolKeepsFailing => {
                // Trzy identyczne porażki tego samego narzędzia. Pompa liczy je sama i sama
                // podnosi awarię — my nie mamy jak jej zamówić inaczej niż tym, co widzi.
                for attempt in 0..REPEATED_FAILURES {
                    let id = format!("call-{attempt}");
                    let _ = self
                        .events
                        .send(DecodedEvent {
                            event: AgentEvent::ToolStart {
                                id: id.clone(),
                                label: "Running the same thing again".to_owned(),
                            },
                            tool: Some(Tool::Started {
                                action: Action::Ran,
                                target: "./always-fails.sh".to_owned(),
                            }),
                        })
                        .await;
                    let _ = self
                        .events
                        .send(DecodedEvent {
                            event: AgentEvent::ToolEnd {
                                id,
                                ok: false,
                                summary: "the same error, again".to_owned(),
                            },
                            tool: Some(Tool::Ended {
                                output: "the same error, again".to_owned(),
                            }),
                        })
                        .await;
                }
                // Po podniesieniu awarii `commands::run` nie czeka już na wynik tury, więc to
                // czekanie ma tylko nie wrócić przed nią.
                std::future::pending::<()>().await;
                unreachable!("the run raises the tool failure before this turn ever returns")
            }
        }
    }

    async fn cancel(&mut self) -> GroupProof {
        // NIC NIE ZABIJAMY, i to jest cała treść tego dublera: tak wygląda zatrzymanie widziane
        // z perspektywy procesów, kiedy Loadout ginie razem z oknem. Zostaje uczciwe „nie wiem,
        // czy zeszło" — czyli `Alive` z adresem, po którym da się dalej pytać.
        GroupProof::Alive {
            group: Some(self.group),
        }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(None)
    }
}
