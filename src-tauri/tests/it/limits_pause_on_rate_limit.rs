//! AC-2 dla T-31: limit dostawcy **pauzuje bieg i mówi, do kiedy** — zamiast go wywalać.
//!
//! Tabela z `docs/ARCHITECTURE.md` §5 obiecuje to od początku („limit zapytań u dostawcy → krok
//! zostaje `running`, pauza **biegu**, wznowienie o `resetsAt`"), a `engine::limits` umie to od
//! T-21. Czego nie ma, to drogi między jednym a drugim: `AgentEvent::RateLimit` dociera dziś do
//! kuratora, robi się z niego wiersz na ekranie — i bieg wysyła dalej, jakby nic nie zaszło.
//!
//! **Słaba wersja tego kryterium: sprawdzenie, że bieg się zatrzymał.** Przechodzi ją
//! implementacja, która traktuje limit jak błąd i kończy bieg — a wtedy użytkownik traci pracę,
//! która czekała pięć minut. Rozstrzygają cztery rzeczy naraz:
//!
//! 1. **`paused`, nie `failed`.** Czytane z `run.json`, bo `paused` jest jedynym stanem biegu,
//!    którego nie ma w maszynie stanów **kroku** — `StepState` nie da się o niego zapytać, więc
//!    plik na dysku jest jedynym miejscem, w którym da się złapać implementację wpisującą go
//!    przy kroku (`docs/ARCHITECTURE.md` §5, T-15 AC-6 czyta to tak samo).
//! 2. **Nic nowego nie startuje, dopóki trwa pauza.** Trzy kroki bez strzałek, `ile naraz = 1`:
//!    kolejność ustala wyłącznie limit, a odstęp między pierwszym a drugim uruchomieniem
//!    sterownika mierzy dokładnie to, czy wysyłka naprawdę czekała.
//! 3. **Bieg rusza dalej SAM.** Nikt tu nie woła `continue_run_inner` i nie ma czego nacisnąć:
//!    trzeci krok też ma ruszyć, i ma ruszyć **bez** drugiej pauzy. Bez tej drugiej połowy
//!    kryterium przechodzi implementacja, która przed każdym krokiem śpi tyle samo.
//! 4. **Jedno zdanie o tym, do kiedy czeka** (niezmiennik 13). Chwila powrotu ma mieszkać
//!    w **jednym** miejscu — wiersz kuratora niesie ją obok tekstu (`Line::Problem::resets_at`),
//!    a godzinę lokalną rysuje z niej front [T7 §7.2]. Drugi licznik opisujący to samo jest tą
//!    samą wadą, przez którą poprzedni prototyp pokazywał stan połączenia w sześciu miejscach.
//!
//! Zegar jest **prawdziwy**, nie `start_paused`: bieg woła tu i sterowniki, i pompę, i dysk,
//! a czas wirtualny przeskakuje do przodu, kiedy runtime staje bezczynny — wznowienie „samo"
//! przestałoby wtedy cokolwiek znaczyć [T7 §8.1]. Dlatego pauza trwa sekundy, nie minuty.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, Outcome, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
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

/// Ile kroków ma fikstura.
const STEPS: usize = 3;

/// Jak długo dostawca każe czekać. Sekundy, bo `resetsAt` jedzie drutem w **sekundach**
/// uniksowych `[T7 §7.2, V]` i krótszej pauzy nie da się z tej liczby wyrazić.
const PAUSE: Duration = Duration::from_secs(3);

/// Próg, po którym mówimy „to naprawdę czekało". O sekundę niżej niż [`PAUSE`], bo obie strony
/// liczą `resetsAt` w pełnych sekundach: kiedy granica sekundy wypadnie między znacznikiem testu
/// a chwilą, w której bieg zobaczył zdarzenie, prawdziwe czekanie jest o tę jedną sekundę
/// krótsze. Ten sam próg mówi w drugą stronę, że trzeci krok **nie** czekał drugi raz.
const MIN_WAIT: Duration = Duration::from_secs(2);

/// Jak długo trwa jedna tura dublera. Krótko, ale nie zero: krok kończący się w tym samym
/// obrocie, w którym wystartował, mierzyłby wyścig, a nie pauzę.
const HOLD: Duration = Duration::from_millis(50);

/// Ile czekamy na pauzę w `run.json`. Bieg, który wisi, jest dla bramki „nie uruchomiło się"
/// (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(5);

/// Jak często pytamy dysk o stan biegu.
const EVERY: Duration = Duration::from_millis(5);

/// Stan z drutu. **Nie `allowed`**, więc nic nowego nie wychodzi do chwili powrotu limitu; to
/// jest cała reguła bramy (`engine::limits::read_gate`, fail-closed).
const REFUSED: &str = "rejected";

/// Które okno limitu — dosłownie z fikstury `docs/research/fixtures/claude-stream.jsonl`.
const WINDOW: &str = "five_hour";

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000f2
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

/// Trzy kroki i **ani jednej strzałki**: kolejność ustala tu wyłącznie limit, więc odstęp między
/// uruchomieniami mierzy pauzę, a nie graf. Własna kopia plików, bo trzy kroki mogące biec
/// równocześnie w folderze projektu są odmową przy zapisie (niezmiennik 12).
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_three_after_the_limit",
  "name": "Three steps and a provider limit",
  "steps": [
    {
      "kind": "agent",
      "id": "s_one",
      "name": "One",
      "agent": "01990000-0000-7000-8000-0000000000f2",
      "overrides": {},
      "instructions": "one",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_two",
      "name": "Two",
      "agent": "01990000-0000-7000-8000-0000000000f2",
      "overrides": {},
      "instructions": "two",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_three",
      "name": "Three",
      "agent": "01990000-0000-7000-8000-0000000000f2",
      "overrides": {},
      "instructions": "three",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 240 }
    }
  ],
  "links": []
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_provider_limit_pauses_the_run_and_it_comes_back_on_its_own() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let hand = bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("three-after-the-limit", WORKFLOW)?;
    the_fixture_can_run(&workflow, &[&hand])?;
    let store = Store::open(&bench.db())?;

    // Chwila powrotu liczona **przed** biegiem, żeby dało się ją potem znaleźć na wierszu co do
    // sekundy: to jest ta sama liczba, którą wysyła dostawca, i to ona ma dojechać na ekran.
    let resets_at = now_unix()? + i64::try_from(PAUSE.as_secs())?;
    let watch = Arc::new(Watch::default());

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch), resets_at),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        // Jeden naraz, więc jedyną rzeczą, która może odsunąć drugi krok od pierwszego
        // o więcej niż długość tury, jest pauza biegu.
        how_many_at_once: 1,
        task: None,
    };

    let seen = Delivered::default();
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, seen.channel());
    let drain = async move {
        let _ = pump.await;
    };
    // Nikt tutaj nie odpowiada na nic. Jedyne, co ten obserwator robi, to zapisuje stan biegu
    // w chwili, w której bieg stoi — wznowienie ma przyjść samo, o `resetsAt`.
    let onlooker = wait_until_paused(bench.project.path());

    let (ran, paused, ()) = tokio::time::timeout(PATIENCE.saturating_mul(4), async {
        tokio::join!(run_workflow_inner(&deps, &request, sink), onlooker, drain)
    })
    .await
    .map_err(|_| "the run never came back after the provider limit".to_owned())?;
    let paused = paused?;
    let report = ran?;

    // (1) Pauza siedzi na BIEGU i żaden krok jej nie nosi ani na niej nie ginie.
    the_pause_sits_on_the_run(&paused)?;

    // (3) Bieg ruszył dalej sam: wszystkie trzy kroki się odbyły i wszystkie się udały.
    assert_eq!(
        report.outcome,
        Outcome::Done,
        "a run that waited out a provider limit ends on its own, not as cancelled"
    );
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; STEPS],
        "all three steps have to end `succeeded`. A provider limit is a pause, not a failure \
         [T7 §7.2] — a run that ends `failed` here throws away work that only had to wait. \
         They ended as {:?}",
        report.steps
    );

    // (2) Nic nowego nie ruszyło, dopóki trwała pauza.
    let starts = watch.starts();
    assert_eq!(
        starts.len(),
        STEPS,
        "the driver started {} step(s) out of {STEPS}; a step that never started cannot say \
         anything about when dispatch resumed",
        starts.len()
    );
    let waited = starts[1].saturating_duration_since(starts[0]);
    assert!(
        waited >= MIN_WAIT,
        "the second step started {waited:?} after the first one, and the provider said the limit \
         comes back in {PAUSE:?}. A run that sends the next step straight away treats the limit \
         as a line on the screen and nothing else — the agent it sends is refused again, and the \
         window is burnt on refusals"
    );
    let then = starts[2].saturating_duration_since(starts[1]);
    assert!(
        then < MIN_WAIT,
        "the third step waited another {then:?} even though only one limit event ever arrived. \
         Pausing is a state of the RUN with one trigger and one end (`resetsAt`); an \
         implementation that sleeps in front of every step would pass the assertion above \
         without ever having paused anything"
    );

    // (4) Jedno miejsce mówi, do kiedy bieg czeka — nie dwa (niezmiennik 13).
    let announced = seen.deadlines()?;
    let mirrored = times_the_instant_appears(&paused, resets_at);
    assert_eq!(
        announced.len() + mirrored,
        1,
        "the run says when it comes back in {} place(s): {} on screen and {mirrored} in run.json. \
         The limit for live regions per fact is one (invariant 13) — the curator's line already \
         carries `resetsAt` beside its sentence and the view renders the local hour from it \
         [T7 §7.2], so a second copy is a second thing to keep in step, and one of the two is \
         always the stale one",
        announced.len() + mirrored,
        announced.len()
    );
    for at in &announced {
        assert_eq!(
            *at, resets_at,
            "the one sentence has to name the instant that came off the wire, in the unit it \
             came in: `resetsAt` is Unix SECONDS, and the same number read as milliseconds says \
             the limit is back in 1970"
        );
    }
    Ok(())
}

/// (1) Pauza jest stanem **biegu**: żaden krok jej nie nosi i żaden przez nią nie ginie.
fn the_pause_sits_on_the_run(paused: &Json) -> Result<(), Box<dyn Error>> {
    let steps = paused
        .get("steps")
        .and_then(Json::as_array)
        .ok_or("run.json describes no steps while the run waits")?;
    assert_eq!(
        steps.len(),
        STEPS,
        "run.json has to describe all {STEPS} steps while the run waits, not only the ones that \
         already ran"
    );

    let wearing_it: Vec<&Json> = steps
        .iter()
        .filter(|step| step.get("status").and_then(Json::as_str) == Some("paused"))
        .collect();
    assert!(
        wearing_it.is_empty(),
        "a step is carrying `\"status\": \"paused\"`: {wearing_it:?}. Pausing is a property of \
         the RUN and of nothing else — keeping it out of the step machine removes a whole \
         quadrant of states nobody needs (docs/ARCHITECTURE.md §5)"
    );

    let written_off: Vec<&Json> = steps
        .iter()
        .filter(|step| {
            matches!(
                step.get("status").and_then(Json::as_str),
                Some("failed" | "cancelled" | "skipped")
            )
        })
        .collect();
    assert!(
        written_off.is_empty(),
        "the run wrote off {written_off:?} because the provider asked it to wait. `[T7 §7.2]` \
         names this one wrong in so many words — \"a pause, not a failure; do not mark steps \
         failed\" — and on screen it reads as a run that broke on the limit instead of one that \
         is waiting for it"
    );
    Ok(())
}

/// Czeka, aż `run.json` powie, że bieg stoi; oddaje jego treść z tej właśnie chwili.
///
/// Migawka, nie odczyt na końcu: po wznowieniu plik mówi już o biegu, który idzie, więc pytanie
/// „czy stanął, czy padł" ma dokładnie jedno okno, w którym da się je zadać.
async fn wait_until_paused(project: &Path) -> Result<Json, Box<dyn Error>> {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(run) = only_run_dir(project).and_then(|dir| run_file(&dir))
            && run.get("status").and_then(Json::as_str) == Some("paused")
        {
            return Ok(run);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "within {PATIENCE:?} the run never wrote `\"status\": \"paused\"` into run.json \
                 after the provider refused it until {PAUSE:?} from now. Either the limit event \
                 reached the screen and nothing else — which is what a run that keeps sending \
                 into a closed window looks like — or the pause never reached disk, and a state \
                 that never reaches disk does not survive a crash either (invariant 4)"
            )
            .into());
        }
        tokio::time::sleep(EVERY).await;
    }
}

/// Ile razy chwila powrotu limitu pada w tym kawałku stanu, w sekundach albo w milisekundach.
///
/// Rekurencyjnie po całym drzewie, bo pytanie brzmi „w ilu miejscach", a nie „czy w tym polu,
/// o którym pomyślałem". Tekst też się liczy: `"1786800600"` w polu tekstowym jest tym samym
/// drugim licznikiem, tylko zapisanym inaczej.
fn times_the_instant_appears(state: &Json, resets_at: i64) -> usize {
    match state {
        Json::Number(number) => usize::from(
            number
                .as_i64()
                .is_some_and(|found| found == resets_at || found == resets_at * 1_000),
        ),
        Json::String(text) => usize::from(text.contains(&resets_at.to_string())),
        Json::Array(items) => items
            .iter()
            .map(|item| times_the_instant_appears(item, resets_at))
            .sum(),
        Json::Object(fields) => fields
            .values()
            .map(|field| times_the_instant_appears(field, resets_at))
            .sum(),
        Json::Null | Json::Bool(_) => 0,
    }
}

/// Jedyny katalog biegu pod `<projekt>/.loadout/runs/`, albo nic, kiedy jeszcze nie powstał.
fn only_run_dir(project: &Path) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(project.join(".loadout").join("runs"))
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    match dirs.as_slice() {
        [only] => Some(only.clone()),
        _ => None,
    }
}

/// `run.json` z katalogu biegu — albo nic, jeśli akurat nie da się go przeczytać w całości.
fn run_file(dir: &Path) -> Option<Json> {
    serde_json::from_str(&fs::read_to_string(dir.join("run.json")).ok()?).ok()
}

/// Chwila teraz w sekundach uniksowych — tej samej jednostce, w której jedzie `resetsAt`.
fn now_unix() -> Result<i64, Box<dyn Error>> {
    Ok(i64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
    )?)
}

/// Wiersze, które **naprawdę wyszły kanałem** do okna.
///
/// Sądzimy to, co zobaczyłby człowiek, a nie to, co bieg oddał pompie: linia porzucona na pełnej
/// kolejce nie jest zdaniem, które ktokolwiek przeczyta.
#[derive(Debug, Clone, Default)]
struct Delivered(Arc<Mutex<Vec<InvokeResponseBody>>>);

impl Delivered {
    /// Kanał, który pompa dostanie zamiast okna.
    fn channel(&self) -> Channel<Vec<Line>> {
        let seen = Arc::clone(&self.0);
        Channel::new(move |body| {
            // `std::sync::Mutex` w domknięciu SYNCHRONICZNYM: nie ma tu `await`, więc
            // niezmiennik 8 stoi.
            if let Ok(mut seen) = seen.lock() {
                seen.push(body);
            }
            Ok(())
        })
    }

    /// Chwile powrotu limitu z **każdego** dostarczonego wiersza, w kolejności dostarczenia.
    ///
    /// Po polu, nie po tekście: wiersz mówiący „back at 5:30" słowami niósłby godzinę maszyny,
    /// która akurat parsowała strumień, a asercja na obecność stringa przechodzi na komentarzu
    /// (niezmiennik 20).
    fn deadlines(&self) -> Result<Vec<i64>, Box<dyn Error>> {
        let seen = self
            .0
            .lock()
            .map_err(|error| format!("the recorder was poisoned: {error}"))?;
        let mut found = Vec::new();
        for body in seen.iter().cloned() {
            for line in body.deserialize::<Vec<Json>>()? {
                if let Some(at) = line.get("resetsAt").and_then(Json::as_i64) {
                    found.push(at);
                }
            }
        }
        Ok(found)
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

/// Biblioteka użytkownika i folder pracy na czas jednego kryterium.
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
fn fake_drivers(watch: Arc<Watch>, resets_at: i64) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        watch,
        resets_at,
        told: AtomicBool::new(false),
    });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Obserwator sterownika: kiedy ruszył każdy krok.
#[derive(Debug, Default)]
struct Watch {
    starts: Mutex<Vec<Instant>>,
}

impl Watch {
    /// Krok wszedł do sterownika.
    ///
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym wywołaniu,
    /// więc nie ma wyrażenia, w którym dożyłby do `await`.
    fn entered(&self) {
        self.lock().push(Instant::now());
    }

    /// Chwile startu kolejnych kroków, w kolejności uruchomienia.
    fn starts(&self) -> Vec<Instant> {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Instant>> {
        // Zatruty zamek nie ma prawa zgubić pomiaru: panika w jednym kroku oślepiłaby asercję,
        // która akurat mierzy, kiedy ruszył następny.
        self.starts.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Dubler sterownika. **Limit dostawcy zgłasza dokładnie raz**, przy pierwszym kroku: pauza ma
/// mieć jeden początek i jeden koniec, więc drugie zdarzenie mieszałoby dwie odpowiedzi w jedną.
#[derive(Debug)]
struct Fake {
    watch: Arc<Watch>,
    /// Kiedy limit wraca, w sekundach uniksowych — dokładnie ta liczba, którą test potem szuka.
    resets_at: i64,
    /// Czy limit już poszedł na drut.
    told: AtomicBool,
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
        self.watch.entered();
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

        if !self.told.swap(true, Ordering::SeqCst) {
            // Kształt z drutu: `status` rozstrzyga, `resetsAt` mówi do kiedy, a `pause_run`
            // jest zdaniem vendora o tym samym. Wszystkie trzy zgodne, żeby kryterium nie
            // zależało od tego, które pole czyta implementacja.
            let _ = events
                .send(
                    (AgentEvent::RateLimit {
                        status: REFUSED.to_owned(),
                        resets_at: self.resets_at,
                        rate_limit_type: WINDOW.to_owned(),
                        pause_run: true,
                    })
                    .into(),
                )
                .await;
        }

        Ok(Box::new(Turn { events, session }))
    }
}

/// Jedna tura dublera.
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
        tokio::time::sleep(HOLD).await;
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: String::new(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: HOLD,
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
