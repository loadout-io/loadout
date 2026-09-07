//! Z-9: katalog roboczy kroku w folderze BEZ repozytorium schodzi po biegu — na każdej drodze.
//!
//! # Co było zepsute
//!
//! `close_the_trees` sprzątało wyłącznie drzewa gita: warunek `let Some(branch) = …` przepuszczał
//! krok bez gałęzi bez ani jednego skutku, a doc tej funkcji mówił wprost „katalog kopii nie jest
//! sprzątany nigdy". Kopia niesie cały projekt bez `.git`, `node_modules` i `target`, zostaje po
//! KAŻDYM biegu i nic w całej aplikacji nie umiało jej zdjąć — ani po biegu, ani przy otwarciu
//! folderu, ani przyciskiem. Zmierzone u właściciela 2026-09-02: 87 katalogów `work/`, 3,8 GB.
//!
//! # DRÓG ZEJŚCIA JEST PIĘĆ i każda ma tu swój przypadek
//!
//! Sukces, porażka kroku, Stop człowieka, limit czasu kroku i sufit wydatku. Wszystkie przechodzą
//! przez `finish_planned_run`, ale to jest fakt o kodzie DZISIEJSZYM: droga, której nie sądzi ani
//! jedno kryterium, jest tą, która wypadnie przy następnej poprawce i nikt tego nie zauważy.
//! Sufit wydatku jest tu najciekawszy, a nie najnudniejszy: katalog kroku powstaje przy układaniu
//! katalogu biegu, czyli PRZED pierwszym procesem, więc krok pominięty przez sufit ma swoją kopię
//! na dysku, mimo że nie ruszył ani na chwilę.
//!
//! # SŁABĄ WERSJĄ jest samo „katalog nie istnieje" po udanym biegu
//!
//! Przechodzi ją jedno `remove_dir_all` w gałęzi sukcesu, po którym cztery pozostałe drogi
//! zostawiają dokładnie to, co zostawiały do dziś.
//!
//! # I SZÓSTY PRZYPADEK: droga, na której skutku NIE MA
//!
//! Zdjęcie kopii umie odmówić, a wtedy jedyne, co człowiekowi zostaje, to zdanie o tym — ze
//! ŚCIEŻKĄ, bo bez niej szuka jednego katalogu wśród tych, które zostawił każdy inny bieg.
//! Zdanie sądzimy tam, gdzie okno je bierze (`history::read_run_inner`), a nie na wartości
//! zwróconej przez funkcję, która je składa: między jednym a drugim mieszka klasa wady, dla
//! której to repo powstało (niezmiennik 29).

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::run::{run_workflow_inner, run_workflow_with_budget};
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use tempfile::TempDir;
use tokio::sync::mpsc;

const VENDOR: &str = "claude-code";
const PATIENCE: Duration = Duration::from_secs(20);

/// Katalog, pod którym bieg zakłada katalogi robocze kroków.
const WORK: &str = "work";

/// Klucze kafelków — one są nazwami katalogów w `work/`.
const FIRST: &str = "s_first";
const SECOND: &str = "s_second";

/// Plik, który krok pisze w swoim katalogu. Po biegu nie ma go razem z katalogiem.
const MADE: &str = "the-work.txt";

/// Sufit, którego pierwszy krok nie zmieści się w środku ani przez chwilę.
const A_CENT: f64 = 0.01;
/// Ile pierwszy krok melduje, że wydał. Ponad sufit, żeby drugi krok został pominięty.
const SPENT: f64 = 5.0;

const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_copy_does_not_outlive",
  "name": "Two steps in their own copies",
  "steps": [
    {
      "kind": "agent",
      "id": "s_first",
      "name": "First",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "write something down",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_second",
      "name": "Second",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "write something down too",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    }
  ],
  "links": [{ "from": "s_first", "to": "s_second" }]
}
"#;

const AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000a1
name: Scribe
summary: Writes things down
color: slate
runsWith: claude-code
model: opus
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 2
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Do the work.
";

/// Limit z definicji agenta. Ta sama liczba, której czeka przypadek limitu czasu.
const GIVE_UP_MINUTES: u64 = 2;

// ── sukces, porażka kroku i sufit wydatku ─────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_copy_is_gone_after_a_run_that_worked() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let store = Store::open(&bench.db())?;
    let report = bench.run(&store, Habit::Finishes, None, None).await?;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded, StepState::Succeeded],
        "both steps have to finish, or nothing below is talking about a run that happened"
    );
    nothing_is_left(&bench, &report)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_copy_is_gone_after_a_step_that_failed() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let store = Store::open(&bench.db())?;
    let report = bench.run(&store, Habit::Fails, None, None).await?;

    assert_eq!(
        report.steps[0],
        StepState::Failed,
        "the first step has to end as failed, or this case is measuring the same path as the one \
         above it. It ended as {:?}",
        report.steps[0]
    );
    nothing_is_left(&bench, &report)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_copy_is_gone_after_the_spend_limit_stopped_the_run() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let store = Store::open(&bench.db())?;
    let report = bench.run(&store, Habit::Spends, Some(A_CENT), None).await?;

    // Krok pominięty przez sufit NIE RUSZYŁ ANI NA CHWILĘ, a jego katalog powstał przy układaniu
    // katalogu biegu — czyli jest to kopia całego projektu zrobiona dla pracy, której nie było.
    assert_eq!(
        report.steps[1],
        StepState::Skipped,
        "the second step has to be skipped by the spend limit, or this case does not reach the \
         path it exists for. It ended as {:?}",
        report.steps[1]
    );
    nothing_is_left(&bench, &report)?;
    Ok(())
}

// ── Stop człowieka ────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_copy_is_gone_after_a_person_pressed_stop() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let store = Store::open(&bench.db())?;
    let control = RunControl::new();
    let report = bench.run(&store, Habit::Hangs, None, Some(control)).await?;

    assert_eq!(
        report.outcome,
        loadout_lib::commands::Outcome::Cancelled,
        "the run has to come back cancelled, or Stop never reached it and this case is measuring \
         an ordinary run. It came back {:?}",
        report.outcome
    );
    nothing_is_left(&bench, &report)?;
    Ok(())
}

// ── limit czasu kroku ─────────────────────────────────────────────────────────────────────

/// Osobny zegar, bo prawdziwe dwie minuty w teście są niewykonalne: `start_paused` daje tokio
/// prawo przewinąć czas, kiedy jedyną śpiącą rzeczą jest limit kroku (ta sama technika, co
/// w `step_deadline_stops_the_agent`).
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_copy_is_gone_after_a_step_ran_out_of_time() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let store = Store::open(&bench.db())?;
    let report = bench.run(&store, Habit::Hangs, None, None).await?;

    assert_eq!(
        report.steps[0],
        StepState::Failed,
        "the wedged step has to end as failed on its own time limit — nobody cancelled this run. \
         It ended as {:?}",
        report.steps[0]
    );
    nothing_is_left(&bench, &report)?;
    Ok(())
}

// ── kopia, której NIE DAŁO SIĘ zdjąć, jest NAZWANA tam, gdzie człowiek o biegu czyta ─────

/// Niezmiennik 29: zdanie ze ścieżką czyta się przez tę samą komendę, którą woła okno.
///
/// # Dlaczego to jest osobny przypadek, a nie dopisek do pięciu wyżej
///
/// Tamte mierzą SKUTEK — katalogu nie ma. Ten mierzy jedyną drogę, którą skutku nie ma: kiedy
/// zdjęcie kopii odmawia, człowiek musi się o niej dowiedzieć, i to ZE ŚCIEŻKĄ. Bez tego bieg
/// czyta się zielono nad katalogiem, który niesie odpis całego projektu i którego nikt nie szuka,
/// bo nic o nim nie powiedziało. Wartość zwrócona przez `close_one_copy` dowodzi wyłącznie, że
/// zdanie się składa; `read_run_inner` jest miejscem, w którym okno je bierze.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_copy_that_could_not_be_cleared_away_is_named_in_the_run_record()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let store = Store::open(&bench.db())?;
    let report = bench.run(&store, Habit::SealTheFolder, None, None).await?;

    let sealed = report.dir.join(WORK).join(FIRST);
    // PRZESŁANKA, nie kryterium: gdyby zdjęcie się udało, zdanie nie miałoby o czym mówić i cały
    // ten przypadek przechodziłby nad niczym. Prawa przywracamy zaraz po asercjach.
    assert!(
        sealed.exists(),
        "the folder was cleared away after all, so this case is not measuring the path it exists \
         for: {}",
        sealed.display()
    );

    let past = read_run_inner(bench.project.path(), run_folder(&report)?)?;
    let step = past
        .steps
        .iter()
        .find(|one| one.tile == FIRST)
        .ok_or("the open run does not name that step at all")?;
    let said = step.error.clone();
    // Prawa oddajemy PRZED asercjami, żeby czerwień nie zostawiała katalogu, którego `TempDir`
    // nie umie po sobie zdjąć.
    unseal_the_folder(&sealed)?;

    assert!(
        !said.is_empty(),
        "the run's record says nothing about the copy that could not be cleared away. A green run \
         over a folder holding a copy of the whole project is a leftover nobody looks for: nothing \
         mentioned it, and it is left behind by every single run"
    );
    assert!(
        said.contains(&sealed.display().to_string()),
        "the sentence does not say WHERE the leftover is, so the person is told something went \
         wrong and then has to find one folder among the ones every other run left. It said: \
         {said}"
    );

    Ok(())
}

/// Nazwa katalogu biegu — tą samą wartością posługuje się okno, otwierając bieg.
fn run_folder(report: &RunReport) -> Result<&str, Box<dyn Error>> {
    report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "the run directory has no UTF-8 folder name".into())
}

// ── wspólna asercja ──────────────────────────────────────────────────────────────────────

/// Po biegu nie ma ani jednego katalogu roboczego, a katalog biegu **zostaje**.
///
/// Drugie zdanie jest tu równie ważne, co pierwsze: bieg wyczyszczony w całości zabiera
/// strumienie agentów, przekazania i cały opis, czyli historię, którą człowiek otwiera w oknie.
/// Sprzątamy kopie robocze, a nie bieg.
fn nothing_is_left(bench: &Bench, report: &RunReport) -> Result<(), Box<dyn Error>> {
    let past = read_run_inner(bench.project.path(), run_folder(report)?)?;

    /* KOPIA ZE ZMIANĄ ZOSTAJE — I MUSI BYĆ NAZWANA TAM, GDZIE OKNO CZYTA.
     *
     * Krok pracujący w kopii folderu bez repozytorium nie ma innego nośnika swojej pracy: nie ma
     * gałęzi, nie ma commita. Kasowanie tej kopii niszczyło więc wynik, a nie śmieć. Sierotą jest
     * katalog, który został i o którym nikt nic nie powiedział. */
    let kept = report.dir.join(WORK).join(FIRST);
    assert!(
        kept.exists(),
        "the only copy of this step's work was removed with the run: {}",
        kept.display()
    );
    let named = past
        .result_folders
        .iter()
        .find(|one| one.work_key == FIRST)
        .ok_or_else(|| {
            format!(
                "the copy at {} outlived its run and the run record says nothing about it.                  Nobody looks for what nothing mentioned",
                kept.display()
            )
        })?;
    assert_eq!(
        named.path,
        kept,
        "the run names a folder other than the one on disk, so the button in the window opens          somewhere else"
    );
    assert_eq!(
        named.state, "changed",
        "the copy outlived its run without being a checked result of the step ({} said {}).          Only work Loadout could verify may stay",
        kept.display(),
        named.state
    );

    /* KOPIA BEZ ZMIAN SCHODZI — to jest zdanie, dla którego ten plik powstał. */
    let gone = report.dir.join(WORK).join(SECOND);
    assert!(
        !gone.exists(),
        "an unchanged copy of the whole project is still on disk at {}. It is left behind by          every single run, and nothing in the app takes it away: not the end of the run, not          opening the folder, not a button",
        gone.display()
    );
    assert!(
        !past.result_folders.iter().any(|one| one.work_key == SECOND),
        "an unchanged copy was announced as a result of the run"
    );

    assert!(
        report.dir.join("run.json").exists(),
        "the record of the run went away with the copies. The copies are ours; the record is the          history a person opens in the window"
    );
    assert!(
        bench.project.path().join("notes.txt").exists(),
        "the person's own file in the project folder is gone. Clearing away our copies must never          reach outside our own folder"
    );
    Ok(())
}

// ── dubler ────────────────────────────────────────────────────────────────────────────────

/// Jak krok schodzi ze sceny — po jednym wariancie na drogę zejścia biegu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Habit {
    /// Pisze i kończy. Zwykły udany bieg.
    Finishes,
    /// Pisze i melduje, że mu nie wyszło.
    Fails,
    /// Pisze i nie wraca. Tak wygląda agent, którego zdejmuje Stop albo limit czasu.
    Hangs,
    /// **Nic nie pisze**, kończy i zamyka swój katalog na zapis, przez co zdjęcie kopii —
    /// której zawartość jest wtedy równa wejściu, więc rusza naprawdę — musi odmówić.
    SealTheFolder,
    /// Pisze, kończy i melduje wydatek ponad sufit biegu.
    Spends,
}

/// Zdejmuje prawo zapisu z katalogu kopii, przez co `remove_dir_all` na niej odmawia.
///
/// # 2026-09 (Z-9) — jedyny DETERMINISTYCZNY sposób, jaki znamy
///
/// Skasowanie pliku wymaga prawa zapisu do katalogu, w którym on leży — nie do samego pliku.
/// Katalog bez tego prawa, ale z zawartością (kopia niesie `notes.txt` człowieka), odmawia więc
/// na pierwszym wpisie i nie zdąży zniknąć. To jest odpowiednik zamka
/// gita z `a_step_that_commits_its_own_work`: kopia w rejestrze drzew nie stoi, więc zamka nie ma
/// czym założyć.
fn seal_the_folder(cwd: &Path) -> anyhow::Result<()> {
    let mut how = fs::metadata(cwd)?.permissions();
    how.set_mode(0o555);
    fs::set_permissions(cwd, how)?;
    Ok(())
}

/// Oddaje prawo zapisu, żeby `TempDir` umiał po sobie posprzątać.
fn unseal_the_folder(cwd: &Path) -> Result<(), Box<dyn Error>> {
    let mut how = fs::metadata(cwd)?.permissions();
    how.set_mode(0o755);
    fs::set_permissions(cwd, how)?;
    Ok(())
}

fn fake_drivers(habit: Habit, wrote: Arc<tokio::sync::Notify>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { habit, wrote });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct Fake {
    habit: Habit,
    /// Dzwoni, gdy pierwszy krok NAPRAWDĘ zapisał swoją pracę.
    ///
    /// Stop czekający na zegarze mierzył raz jedną stronę polityki, raz drugą — zależnie od
    /// obciążenia maszyny. Kryterium, które po cichu dryfuje między dwoma pytaniami, nie
    /// odpowiada na żadne.
    wrote: Arc<tokio::sync::Notify>,
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
        // Praca w katalogu kroku, bo katalog, w którym nic nie powstało, schodzi także po
        // implementacji, która o pracę nie dba.
        if self.habit == Habit::SealTheFolder && spec.cwd.ends_with(FIRST) {
            /* KROK, KTÓREMU ZAMYKAMY KOPIĘ, NIE ZMIENIA JEJ ZAWARTOŚCI — I TO JEST CAŁY SENS.
             *
             * Kopia ze zmianą jest zatrzymywana z założenia i `close_one_copy` nie próbuje jej
             * zdjąć, więc zamek nie miałby tam czego zatrzymać. Aż do 2026-09-07 ten dubler pisał
             * `MADE` także tutaj i przypadek przechodził nad ścieżką, której nie dotykał: bez
             * `seal_the_folder` był tak samo zielony. Bez tego zapisu `entries == input.entries()`,
             * więc zdjęcie kopii rusza naprawdę i odmawia.
             *
             * Zamek trzyma, bo kopia niesie `notes.txt` człowieka: skasowanie pliku pyta o prawo
             * zapisu do katalogu, w którym on leży, a nie do samego pliku. Na PUSTYM katalogu
             * `remove_dir_all` by przeszedł, bo `rmdir` pyta już katalog WYŻEJ, nietknięty. */
            seal_the_folder(&spec.cwd)?;
        } else if spec.cwd.ends_with(FIRST) {
            /* PISZE WYŁĄCZNIE PIERWSZY KROK — i to jest cały sens tej ławki.
             *
             * Polityka ma dwie strony i obie muszą być zmierzone na KAŻDEJ drodze zejścia:
             * kopia, w której krok coś zmienił, ZOSTAJE (bo bez repozytorium jest jedynym
             * nośnikiem jego pracy), a kopia, w której nie zmienił nic, SCHODZI. Gdyby oba
             * kroki pisały, druga strona nie byłaby sądzona nigdzie. */
            fs::write(spec.cwd.join(MADE), "this is what the step produced")?;
            self.wrote.notify_one();
        }
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
        Ok(Box::new(Turn {
            events,
            session,
            habit: self.habit,
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    habit: Habit,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        Some(GroupId {
            pid: 4242,
            pgid: 4242,
        })
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        if self.habit == Habit::Hangs {
            // Nigdy nie wraca — dokładnie tak wygląda zaklinowany agent z otwartym wejściem.
            std::future::pending::<()>().await;
            unreachable!("pending() never resolves");
        }
        let outcome = TurnOutcome {
            ok: self.habit != Habit::Fails,
            reason: if self.habit == Habit::Fails {
                FinishReason::Failed("the step could not do the work".to_owned())
            } else {
                FinishReason::Completed
            },
            text: String::new(),
            cost_usd: (self.habit == Habit::Spends).then_some(SPENT),
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
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

// ── ławka ─────────────────────────────────────────────────────────────────────────────────

/// Projekt, który **repozytorium nie jest** — i to jest cały warunek tego pliku.
///
/// Bez `git init` `isolate::make_from_after_add` schodzi gałęzią kopii plikowej, czyli tą jedną,
/// której sprzątanie nie dotyczyło. Folder z repozytorium sądzi obok
/// `a_step_that_commits_its_own_work`.
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
        fs::write(home.path().join("agents").join("scribe.md"), AGENT)?;
        fs::write(project.path().join("notes.txt"), "the person's own file")?;
        Ok(Self { home, project })
    }

    /// Jeden bieg tego workflow. `stop` naciska Stop w środku pracy pierwszego kroku.
    async fn run(
        &self,
        store: &Store,
        habit: Habit,
        budget_usd: Option<f64>,
        stop: Option<RunControl>,
    ) -> Result<RunReport, Box<dyn Error>> {
        let control = stop.clone().unwrap_or_default();
        let wrote = Arc::new(tokio::sync::Notify::new());
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store,
            drivers: fake_drivers(habit, Arc::clone(&wrote)),
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control,
        };
        let request = RunRequest {
            workflow: self.workflow("copy-does-not-outlive", WORKFLOW)?,
            how_many_at_once: 2,
            task: None,
            part: None,
            handoffs_from: None,
        };

        let recorder = Delivered::default();
        let (sink, source) = line_channel(QUEUE_CAP);
        let pump = spawn_pump(source, recorder.channel());
        /* Cierpliwość mieści DWA limity kroku, nie jeden. Kafelek, którego czas się skończył,
         * wypuszcza to, co po nim (domyślne `carry-on`), więc drugi krok zaklinuje się tak samo
         * i tak samo odczeka swoje. Przypadek limitu czasu biegnie na zatrzymanym zegarze, więc
         * tokio przewinie oba terminy sam, gdy jedyną śpiącą rzeczą będą one. */
        let waiting = PATIENCE + Duration::from_secs(2 * GIVE_UP_MINUTES * 60);
        let report = tokio::time::timeout(waiting, async {
            let (report, ()) = tokio::join!(
                async {
                    match budget_usd {
                        Some(ceiling) => {
                            run_workflow_with_budget(&deps, &request, sink, Some(ceiling)).await
                        }
                        None => run_workflow_inner(&deps, &request, sink).await,
                    }
                },
                async {
                    let Some(control) = stop else { return };
                    // W ŚRODKU PRACY, nie przed nią, i to sądzone SKUTKIEM, a nie zegarem:
                    // czekamy, aż pierwszy krok naprawdę zapisze swoją pracę. Stop, który pada
                    // wcześniej, mierzy bieg, który nie zdążył założyć ani jednego katalogu.
                    wrote.notified().await;
                    control.stop();
                }
            );
            report
        })
        .await
        .map_err(|_| "the run never came back")??;
        let _ = tokio::time::timeout(PATIENCE, pump).await;
        Ok(report)
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

/// Paczki, które wyszły kanałem. Ten plik ich nie sądzi — pompa musi mieć dokąd oddawać.
#[derive(Debug, Clone, Default)]
struct Delivered(Arc<Mutex<Vec<serde_json::Value>>>);

impl Delivered {
    fn channel(&self) -> tauri::ipc::Channel<Vec<loadout_lib::engine::line::Line>> {
        let sink = Arc::clone(&self.0);
        tauri::ipc::Channel::new(move |body| {
            if let tauri::ipc::InvokeResponseBody::Json(text) = body
                && let Ok(value) = serde_json::from_str(&text)
            {
                sink.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(value);
            }
            Ok(())
        })
    }
}
