//! Z-3: miejsce ciężkie wraca do puli, kiedy prośba o nie znika w trakcie czekania na pulę.
//!
//! `Limiter::take_slot` bierze najpierw miejsce ciężkie, woła na nim `forget()` i **dopiero
//! potem** czeka na miejsce z puli. Między tymi dwiema linijkami jest punkt anulowania, a `Slot`
//! — jedyna droga zwrotu — powstaje dopiero za nimi. Kiedy `Live::a_slot_for_this_step` porzuci
//! future prośby przez `tokio::select!` na Stopie, zapomniane miejsce ciężkie nie ma kto oddać.
//!
//! **Skala tej jednej linijki.** Limiter jest JEDEN NA APLIKACJĘ (`engine/limits.rs`, nagłówek),
//! a miejsc ciężkich jest dokładnie jedno (niezmiennik 26). Jeden Stop trafiony w to okno zabiera
//! więc jedyne miejsce ciężkie **całej aplikacji**: od tej chwili żaden krok „sprawdź", w żadnej
//! karcie i w żadnym biegu, już nie ruszy — aż do restartu. Bez czerwieni, bez komunikatu
//! i bez przycisku: kafelek po prostu nigdy się nie zapala. Miejsce z puli nie przecieka nigdy,
//! bo między jego `forget()` a budową `Slot` nie ma ani jednego `await`.
//!
//! # Cztery pomiary i po co każdy z nich
//!
//! 1. [`a_cancelled_heavy_request_leaves_its_seat_for_the_next_one`] — kryterium wprost z audytu,
//!    na zegarze wirtualnym: `Limiter::with_heavy(1, 1)`, prośba ciężka porzucona w trakcie
//!    czekania na pulę, potem następna prośba ciężka pod sufitem jednej sekundy.
//! 2. [`the_recovered_heavy_place_is_exactly_one`] — naprawa nie ma prawa oddać więcej, niż
//!    wzięła. Bez tego pomiaru „miejsce wraca" spełnia też `add_permits` w złym miejscu, po
//!    którym dwa `cargo` idą obok siebie i maszyna zamarza (niezmiennik 26).
//! 3. [`ordinary_work_still_shares_one_moment_after_the_cancelling`] — bieg kontrolny. Pula, która
//!    po anulowaniu przestaje zrównoleglać cokolwiek, spełnia oba pomiary wyżej i zabiera całą
//!    przesłankę produktu (niezmiennik 11). Ten jeden przechodzi tak samo przed naprawą, jak
//!    i po niej, i to jest jego rola: pilnuje, żeby naprawa niczego nie zwęziła.
//! 4. [`a_stop_does_not_stop_the_next_check_step`] — niezmiennik 29: to samo pytanie zadane tam,
//!    gdzie odpowiedź widzi CZŁOWIEK. Nie „co zwróciło `take_slot`", tylko „czy kafelek `sprawdź`
//!    w następnym biegu tej samej aplikacji zapala się i dochodzi do końca".
//!
//! **Zegar wirtualny w pierwszych trzech, prawdziwy w czwartym**, i to nie jest niekonsekwencja:
//! „pracą" prośby jest tam jeden `tokio::time::sleep` i nic więcej, więc czas wirtualny mierzy
//! wyłącznie to, co pula zrobiła z prośbą, i nic o tym, jak obciążona jest maszyna. Czwarty
//! uruchamia prawdziwy proces, więc każdy jego sufit musi być ścienny.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{AgentDriver, AgentHandle, DecodedEvent, Probe, RunSpec};
use loadout_lib::engine::limits::{Dispatch, Limiter, Run, Weight};
use loadout_lib::engine::step::StepState;
use loadout_lib::ipc::{LineSink, QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio::time::{Instant, sleep};

/// Pula z kryterium audytu: jedno miejsce i jedno miejsce ciężkie.
const THE_NARROWEST: usize = 1;

/// Pula szeroka na trzy prośby zwykłe — tylko w takiej widać, że praca zwykła dalej się nakłada.
const POOL: usize = 3;

/// Ile miejsc ciężkich. Jedynka z niezmiennika 26.
const HEAVY_AT_ONCE: usize = 1;

/// Ile prośb staje w kolejce w pomiarach kontrolnych.
const TASKS: usize = 3;

/// Cała „praca" jednej prośby: jeden sen i nic więcej — patrz nagłówek.
const WORK: Duration = Duration::from_millis(250);

/// Ile prośba ciężka czeka na miejsce z puli, zanim ją porzucimy. Na zegarze wirtualnym ta liczba
/// nic nie kosztuje; jest tu tylko po to, żeby porzucenie padło na DRUGIM `await`.
const A_MOMENT: Duration = Duration::from_millis(50);

/// Sufit z kryterium audytu: następna prośba ciężka ma wrócić w mniej niż sekundę.
const A_SECOND: Duration = Duration::from_secs(1);

/// Ile czekamy na całą kolejkę prośb, zanim uznamy pulę za zakleszczoną.
///
/// Minuta, nie 60 s: `clippy::duration_suboptimal_units` biegnie w `full` na `-D warnings`.
const PATIENCE: Duration = Duration::from_mins(1);

/// Ile bieg z prawdziwym procesem ma na powrót. Zegar ŚCIENNY — tu nie ma czasu wirtualnego.
const THE_RUN_HAS: Duration = Duration::from_secs(30);

/// Ile czekamy, zanim człowiek naciśnie Stop.
///
/// Krok „sprawdź" sięga po miejsce zaraz po złożeniu planu, czyli w kilkanaście milisekund; ta
/// sekunda jest marginesem, nie pomiarem. Gdyby okazała się za krótka, Stop trafiłby w krok,
/// który jeszcze o nic nie poprosił — dlatego niżej stoi asercja o tym, że skrypt nie ruszył.
const BEFORE_THE_STOP: Duration = Duration::from_secs(1);

/// Okno jednej prośby: kiedy dostała miejsce i kiedy je oddała.
type Span = (Instant, Instant);

/// Prośba ciężka, która znika w trakcie czekania na miejsce z puli.
///
/// Dokładnie to, co robi Stop w `Live::a_slot_for_this_step`: `tokio::select!` porzuca future
/// prośby tam, gdzie akurat stoi. Cała pula jest wtedy w rękach tej funkcji, więc prośba MUSI
/// stanąć na drugim `await` — pierwszy, ten po miejsce ciężkie, przechodzi natychmiast.
async fn a_heavy_request_goes_away_while_it_waits(seats: &Limiter) -> Result<(), Box<dyn Error>> {
    let run = Run::new(seats.clone(), &[StepState::Ready; TASKS]);
    let mut held = Vec::new();
    for _ in 0..seats.at_once() {
        let Dispatch::Granted(slot) = run.dispatch().await else {
            return Err(
                "the pool turned down an ordinary request in a run that never saw a \
                        provider limit, so this fixture never filled it"
                    .into(),
            );
        };
        held.push(slot);
    }

    let granted = tokio::select! {
        asked = run.dispatch_as(Weight::Heavy) => matches!(asked, Dispatch::Granted(_)),
        () = sleep(A_MOMENT) => false,
    };
    assert!(
        !granted,
        "the heavy request was let in while every place in the pool was held, so it never waited \
         on the pool and nothing in this file measures what it says it measures"
    );

    // Miejsca z puli wracają; miejsce ciężkie miała oddać porzucona prośba.
    drop(held);
    Ok(())
}

/// Trzy prośby o tej samej wadze, wszystkie tymi samymi drzwiami: [`Run::dispatch_as`].
///
/// Waga jest argumentem prośby, nie drugą pulą z własnym wejściem — powód stoi w nagłówku
/// `engine/limits.rs`.
async fn three_requests(seats: &Limiter, weight: Weight) -> Result<Vec<Span>, Box<dyn Error>> {
    let run = Arc::new(Run::new(seats.clone(), &[StepState::Ready; TASKS]));

    let mut queued: JoinSet<Option<Span>> = JoinSet::new();
    for _ in 0..TASKS {
        let run = Arc::clone(&run);
        queued.spawn(async move {
            match run.dispatch_as(weight).await {
                Dispatch::Granted(slot) => {
                    let start = Instant::now();
                    sleep(WORK).await;
                    let end = Instant::now();
                    // Miejsce wraca PO odczycie końca, więc zapisane okno jest węższe niż
                    // prawdziwe trzymanie: nakładanie się jest zaniżone, nie zawyżone.
                    drop(slot);
                    Some((start, end))
                }
                // Odmowa w biegu, który nigdy nie widział limitu dostawcy, jest awarią samego
                // pomiaru — dlatego nie znika po cichu, tylko wraca jako brak okna.
                Dispatch::Refused(_) => None,
            }
        });
    }

    let joined = tokio::time::timeout(PATIENCE, async move {
        let mut out: Vec<Option<Span>> = Vec::new();
        while let Some(one) = queued.join_next().await {
            out.push(one.ok().flatten());
        }
        out
    })
    .await
    .map_err(|_| "not every request came back, so the pool never let somebody in and never will")?;

    Ok(joined.into_iter().flatten().collect())
}

/// KRYTERIUM AUDYTU: `Limiter::with_heavy(1, 1)`, prośba ciężka anulowana w trakcie czekania na
/// pulę, a następna prośba ciężka ma się skończyć.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_cancelled_heavy_request_leaves_its_seat_for_the_next_one() -> Result<(), Box<dyn Error>>
{
    let seats = Limiter::with_heavy(THE_NARROWEST, THE_NARROWEST);
    a_heavy_request_goes_away_while_it_waits(&seats).await?;

    let run = Run::new(seats, &[StepState::Ready]);
    let next = tokio::time::timeout(A_SECOND, run.dispatch_as(Weight::Heavy))
        .await
        .map_err(|_| {
            format!(
                "the next heavy request never came back within {A_SECOND:?}. The request before it \
                 went away while it waited for a place in the pool, and it had already forgotten \
                 the heavy place it was holding — so there is nobody left to give that place back. \
                 One pool serves the whole application, so from here on no check step starts in \
                 any card and in any run, until somebody restarts the application"
            )
        })?;
    assert!(
        matches!(next, Dispatch::Granted(_)),
        "the pool has to hand the recovered heavy place to the next request, not turn it down"
    );
    Ok(())
}

/// Odzyskane miejsce jest DOKŁADNIE JEDNO: po anulowaniu trzy prośby ciężkie dalej nie dzielą
/// ani jednej chwili.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_recovered_heavy_place_is_exactly_one() -> Result<(), Box<dyn Error>> {
    let seats = Limiter::with_heavy(POOL, HEAVY_AT_ONCE);
    a_heavy_request_goes_away_while_it_waits(&seats).await?;

    let spans = three_requests(&seats, Weight::Heavy).await?;
    assert_eq!(
        spans.len(),
        TASKS,
        "all {TASKS} heavy requests have to be let in and come back out after the cancelling, or \
         the windows below belong to a pool that stopped letting anybody in: {spans:?}"
    );
    for (first, one) in spans.iter().enumerate() {
        for other in spans.iter().skip(first + 1) {
            let shared = one
                .1
                .min(other.1)
                .saturating_duration_since(one.0.max(other.0));
            assert_eq!(
                shared,
                Duration::ZERO,
                "two heavy requests shared {shared:?} of a {WORK:?} window after one request was \
                 cancelled, so giving the place back handed out more than was taken. Two `cargo` \
                 builds side by side pin the memory compressor and freeze this machine at zero \
                 swap (invariant 26). Windows: {spans:?}"
            );
        }
    }
    Ok(())
}

/// BIEG KONTROLNY: praca zwykła po anulowaniu dalej nakłada się w jednej chwili.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn ordinary_work_still_shares_one_moment_after_the_cancelling() -> Result<(), Box<dyn Error>>
{
    let seats = Limiter::with_heavy(POOL, HEAVY_AT_ONCE);
    a_heavy_request_goes_away_while_it_waits(&seats).await?;

    let spans = three_requests(&seats, Weight::Ordinary).await?;
    assert_eq!(
        spans.len(),
        TASKS,
        "every ordinary request has to be let in and come back out, or the overlap below is \
         measured on a smaller run than the one that was asked for: {spans:?}"
    );
    let latest_start = spans
        .iter()
        .map(|span| span.0)
        .max()
        .ok_or("no windows to compare")?;
    let earliest_end = spans
        .iter()
        .map(|span| span.1)
        .min()
        .ok_or("no windows to compare")?;
    assert!(
        latest_start < earliest_end,
        "the {TASKS} ordinary windows still have to overlap in one instant after a heavy request \
         was cancelled: the last one to start does so before the first one ends. A pool that \
         stops running things side by side throws away the only reason this product exists \
         (invariant 11). Windows: {spans:?}"
    );
    Ok(())
}

/// NIEZMIENNIK 29: to samo, ale tam, gdzie widzi to człowiek.
///
/// Człowiek naciska Stop, kiedy krok „sprawdź" czeka na miejsce z puli. Pytanie nie brzmi „co
/// zwróciło `take_slot`", tylko „czy kafelek `sprawdź` w NASTĘPNYM biegu tej samej aplikacji
/// zapala się i dochodzi do końca".
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_stop_does_not_stop_the_next_check_step() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let store = Store::open(&bench.db())?;
    let request = bench.request();
    // Jedna pula na całą aplikację — dokładnie ta, którą `AppState::begin_run` wręcza każdemu
    // startowi. Dwa biegi niżej dostają jej klon, nie własną pulę.
    let seats = Limiter::with_heavy(THE_NARROWEST, THE_NARROWEST);

    // Jedyne miejsce z puli w rękach testu, więc krok „sprawdź" pierwszego biegu MUSI zaparkować
    // na puli — z miejscem ciężkim już w ręku.
    let outside = Run::new(seats.clone(), &[StepState::Ready]);
    let Dispatch::Granted(held) = outside.dispatch().await else {
        return Err("the pool turned the test down on its only ordinary place".into());
    };

    let stopped = RunControl::sharing(seats.clone());
    let first = bench.deps(&store, stopped.clone());
    let ran = tokio::time::timeout(THE_RUN_HAS, async {
        tokio::join!(one_run(&first, &request), async {
            sleep(BEFORE_THE_STOP).await;
            stopped.stop();
        })
    })
    .await
    .map_err(|_| format!("the stopped run did not come back within {THE_RUN_HAS:?}"))?;
    let report = ran.0?;
    assert!(
        bench.marks()?.is_empty(),
        "the check step of the stopped run has to be waiting for a place in the pool when Stop \
         arrives — its command may not have run. It did run, so Stop landed somewhere else and \
         the run below would pass whatever the pool does with the heavy place. Report: {:?}",
        report.steps
    );

    // Miejsce z puli wraca do aplikacji. Miejsce ciężkie miał oddać Stop.
    drop(held);

    let next = bench.deps(&store, RunControl::sharing(seats));
    let report = tokio::time::timeout(THE_RUN_HAS, one_run(&next, &request))
        .await
        .map_err(|_| {
            format!(
                "the check step of the next run never started: it is still waiting for the one \
                 heavy place this application has, and that place went away with the request Stop \
                 cancelled. On the screen this is a card that simply never lights up — no red, no \
                 message and nothing left to press — and it stays that way until somebody \
                 restarts the application. It did not come back within {THE_RUN_HAS:?}"
            )
        })??;
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded],
        "the check step of the next run has to finish. It ended as {:?}",
        report.steps
    );
    assert_eq!(
        bench.marks()?.len(),
        1,
        "and it has to finish because its command really ran, not because the run gave up on it"
    );
    Ok(())
}

/// Jeden bieg z pompą do okna. Zewnętrzny `Result` mówi „bieg wrócił", wewnętrzny — czym.
async fn one_run(
    deps: &RunDeps<'_>,
    request: &RunRequest,
) -> Result<RunReport, loadout_lib::commands::RunError> {
    let (sink, drain) = the_pump_seam();
    let (ran, ()) = tokio::join!(run_workflow_inner(deps, request, sink), drain);
    ran
}

/// Szew, którym bieg mówi do okna.
fn the_pump_seam() -> (LineSink, impl Future<Output = ()>) {
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    (sink, async move {
        let _ = pump.await;
    })
}

/// Biblioteka użytkownika, projekt, skrypt i workflow z jednym krokiem „sprawdź".
struct Bench {
    home: TempDir,
    project: TempDir,
    scripts: TempDir,
    marks: PathBuf,
    workflow: PathBuf,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        let scripts = TempDir::new()?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;

        let marks = scripts.path().join("marks.txt");
        fs::write(&marks, "")?;
        let command = script(scripts.path(), "check.sh", &marks)?;
        let workflow = home.path().join("workflows").join("one-check.json");
        fs::write(&workflow, one_check_workflow(&command))?;

        Ok(Self {
            home,
            project,
            scripts,
            marks,
            workflow,
        })
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }

    fn deps<'a>(&'a self, store: &'a Store, control: RunControl) -> RunDeps<'a> {
        RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store,
            drivers: no_drivers(),
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control,
        }
    }

    fn request(&self) -> RunRequest {
        RunRequest {
            workflow: self.workflow.clone(),
            how_many_at_once: THE_NARROWEST,
            task: None,
            part: None,
            handoffs_from: None,
        }
    }

    /// Po jednym wierszu na każde uruchomienie komendy kroku „sprawdź".
    fn marks(&self) -> Result<Vec<String>, Box<dyn Error>> {
        // `scripts` musi dożyć do tego miejsca: to w nim leży plik znaczników.
        let _ = &self.scripts;
        Ok(fs::read_to_string(&self.marks)?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect())
    }
}

/// Komenda kroku „sprawdź": znacznik uruchomienia i dowód przejścia.
///
/// Ścieżka pliku znaczników jest **bezwzględna i wpisana w skrypt**, bo środowisko dziecka jest
/// czyszczone (niezmiennik 9), więc przez zmienną nic tu nie przejdzie.
fn script(dir: &Path, name: &str, marks: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    let body = format!(
        "#!/bin/sh\nprintf '%s\\n' 'ran' >> '{}'\necho '1 passed'\n",
        marks.display()
    );
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn one_check_workflow(command: &Path) -> String {
    format!(
        r#"{{
  "format": 1,
  "id": "wf_z3_one_check",
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
        command.display()
    )
}

/// Fabryka sterowników, która nie umie oddać ani jednego.
///
/// Krok „sprawdź" nie ma vendora, więc w tym biegu nikt nie ma powodu prosić o sterownik — a
/// gdyby poprosił, tura kończy się nazwaną odmową zamiast cichym dublerem, który udaje, że
/// wszystko jest w porządku.
fn no_drivers() -> Drivers {
    Arc::new(|_| Arc::new(NoDriver) as Arc<dyn AgentDriver>)
}

#[derive(Debug)]
struct NoDriver;

#[async_trait]
impl AgentDriver for NoDriver {
    fn id(&self) -> &'static str {
        "none"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: None,
        })
    }

    async fn start(
        &self,
        _spec: RunSpec,
        _events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        Err(anyhow::anyhow!(
            "a check step has no vendor, so nothing in this run may ask for one"
        ))
    }
}
