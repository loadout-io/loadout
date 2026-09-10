//! Z-13b: sufit wydatku jest DZIELONY między kroki, które ruszają razem — i nie kosztuje to
//! równoległości.
//!
//! # Wada, którą to sądzi
//!
//! Reszta sufitu liczyła się z tur, które już **wróciły**, więc przy `how_many_at_once = 3`
//! każdy z trzech startujących obok siebie kroków dostawał tę samą, pełną resztę: bieg mógł
//! wydać około trzykrotności kwoty, którą postawił człowiek, zanim ktokolwiek to zauważył.
//!
//! # Dlaczego to są TRZY asercje w jednym biegu, a nie jedna
//!
//! Poprzednie podejście sumowało przekazane kwoty i dopuszczało jedną flagę równą całej
//! reszcie — więc przechodziło nad implementacją, która dzieli resztę przez „ile biegnie
//! teraz + 1". Ta formuła przy PIERWSZYM kroku dzieli przez jeden, czyli pierwszy krok
//! rezerwuje wszystko, a pozostałe równoległe kończą jako pominięte. Sufit robi się twardy
//! kosztem równoległości, czyli kosztem całej przesłanki tego produktu (niezmiennik 11).
//!
//! Dlatego jeden bieg odpowiada naraz na trzy pytania:
//!
//! - czy trzy kroki NAPRAWDĘ nakładają się w czasie (przecięcie okien, nie „wszystkie się
//!   skończyły" — to drugie przechodzi dla wykonania po kolei);
//! - czy sterownik wszedł trzy razy, a nie raz;
//! - czy każdy dostał INNĄ, mniejszą kwotę, a nie trzy razy całą resztę.
//!
//! Dwie pierwsze przechodzą już przed poprawką i to jest ich rola: są zaporą przed
//! implementacją, która podzieliłaby kwoty, zamieniając trójkę w szereg.
//!
//! Runtime jest **wielowątkowy z prawdziwymi snami**, nigdy `start_paused`: czas wirtualny
//! przeskakuje do przodu, kiedy runtime staje bezczynny, więc „nakładanie się" przestaje
//! wtedy cokolwiek znaczyć [T7 §8.1]. Ten sam wybór, co w `copies_run_side_by_side`.
//!
//! # Druga ławka: vendor bez własnej flagi sufitu
//!
//! Claude dostaje `--max-budget-usd` i zatrzyma turę sam. Codex takiej flagi nie ma, więc turę
//! przekraczającą udział kroku musi przerwać Loadout — a zdanie o tym ma dojść tam, gdzie
//! człowiek je czyta: do wiersza na ekranie **i** do pliku biegu (niezmiennik 29).

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_with_budget;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::claude::VENDOR as CLAUDE;
use loadout_lib::engine::drivers::codex::VENDOR as CODEX;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, DriverConfiguration, FinishReason,
    Outcome as TurnOutcome, Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::{Vendor, read_agent_file};
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use serde_json::Value as Json;
use tauri::ipc::{Channel, InvokeResponseBody};
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Nazwa flagi — przepisana z `claude --help`, nie z naszego kodu (niezmiennik 20).
const FLAG: &str = "--max-budget-usd";

/// Sufit pierwszej ławki. Dziewięć, bo dzieli się przez trzy na okrągłe kwoty i widać po nich
/// gołym okiem, że kolejne udziały maleją.
const BUDGET: f64 = 9.0;

/// Ile kroków startuje obok siebie.
const AT_ONCE: usize = 3;

/// Kwoty, które trzy kroki mają dostać — posortowane, bo kolejność wejścia do puli jest
/// wyścigiem, a udziały nie są.
///
/// `9/3 = 3.00`, potem `(9-3)/3 = 2.00`, potem `(9-5)/3 = 1.33`. Wypisane słowo w słowo, nie
/// policzone tą samą formułą, którą sprawdzają: kryterium liczące własną stałą zawsze się
/// z nią zgadza i nie mierzy niczego (niezmiennik 20).
const SHARES: [&str; AT_ONCE] = ["1.33", "2.00", "3.00"];

/// Ile trwa jedna tura pierwszej ławki. Prawdziwy sen, bo przecięcie okien liczy się na zegarze.
///
/// DŁUGA Z POMIARU, nie z ostrożności. Przy 240 ms i całej suicie biegnącej obok trzy okna
/// wyszły 365–502 ms (timer tokio spóźniony pod obciążeniem), a ich przecięcie 79,6 ms — czyli
/// równoległość BYŁA, a próg jej nie widział. Rozjazd startów jest stały (każdy krok robi
/// najpierw własną kopię plików), więc dłuższa tura zjada go jako coraz mniejszy ułamek.
const TURN: Duration = Duration::from_millis(600);

/// Ile z tego musi być wspólne dla WSZYSTKICH trzech kroków. Prawdziwa równoległość daje tu
/// prawie całą turę, a wykonanie po kolei daje zero — próg w jednej trzeciej nie rozstrzyga się
/// na styk i zostawia dwukrotny zapas nad zmierzonym rozjazdem startów.
const MIN_SHARED: Duration = Duration::from_millis(200);

/// Ile czekamy, zanim uznamy bieg za zawieszony.
const PATIENCE: Duration = Duration::from_secs(30);

/// Sufit drugiej ławki. Jeden krok, więc jego udziałem jest cała ta kwota.
const CODEX_BUDGET: f64 = 2.0;

/// Ile ten krok zdąży wydać, zanim ktokolwiek go zapyta — ponad jego udział.
const OVER_ITS_SHARE: f64 = 2.5;

/// Zdanie, które ma zobaczyć człowiek, kiedy sufit przerwie turę vendora bez własnej flagi.
///
/// Wypisane tutaj słowo w słowo, nie sklejone z funkcji produkcyjnej, i to jest cała treść
/// niezmiennika 29: kryterium czytające własną stałą kodu zawsze się z nią zgadza.
const STOPPED_SENTENCE: &str = "Stopped: this step reached the $2.00 it was allowed to spend, \
                                so Loadout ended it before it cost more.";

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-00000000013b
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

const CODEX_HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-00000000013c
name: Codex Hand
summary: Does the work with the other agent app
color: plum
runsWith: codex
model: gpt-5.6-sol
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

/// Trzy kroki BEZ ANI JEDNEJ STRZAŁKI — to jest treść fikstury, nie jej kształt: strzałka
/// ustawiłaby je w szereg i pytanie o podział reszty nie miałoby jak paść.
///
/// `fresh-copy`, bo trzy kroki pracujące równocześnie w jednym katalogu są odmową przed
/// pierwszym procesem (niezmiennik 12) i ta ławka nie doszłaby wtedy do planisty.
const SIDE_BY_SIDE: &str = r#"{
  "format": 1,
  "id": "wf_shared_ceiling",
  "name": "Three steps and one ceiling",
  "steps": [
    {
      "kind": "agent",
      "id": "s_one",
      "name": "One",
      "agent": "01990000-0000-7000-8000-00000000013b",
      "overrides": {},
      "instructions": "one: do your part",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_two",
      "name": "Two",
      "agent": "01990000-0000-7000-8000-00000000013b",
      "overrides": {},
      "instructions": "two: do your part",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_three",
      "name": "Three",
      "agent": "01990000-0000-7000-8000-00000000013b",
      "overrides": {},
      "instructions": "three: do your part",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 480, "y": 0 }
    }
  ],
  "links": []
}
"#;

/// Jeden krok vendora, który nie ma flagi sufitu w argv.
///
/// `whenItFails: stop` jest tu treścią, nie ustawieniem domyślnym: wspólna polityka porażki
/// dopisuje do zdania w pliku biegu zdanie o tym, co się dzieje dalej („carry on anyway"),
/// a to kryterium pyta o jedno — czy ekran i plik niosą TO SAMO zdanie o sufcie. Polityka
/// „co, kiedy ten nie przejdzie" ma własną wyrocznię (`every_failure_shares_one_door`).
const ONE_CODEX_STEP: &str = r#"{
  "format": 1,
  "id": "wf_codex_over_its_share",
  "name": "A turn that passes its share",
  "steps": [
    {
      "kind": "agent",
      "id": "s_spender",
      "name": "Spender",
      "agent": "01990000-0000-7000-8000-00000000013c",
      "overrides": {},
      "instructions": "spend: keep going",
      "folder": { "use": "fresh-copy" },
      "whenItFails": "stop",
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_steps_that_start_together_each_get_a_share_of_what_is_left()
-> Result<(), Box<dyn Error>> {
    let ran = one_run("shared-ceiling", SIDE_BY_SIDE, HAND_FILE, AT_ONCE, BUDGET).await?;

    // ── (a) TRZY KROKI NAPRAWDĘ RUSZYŁY ─────────────────────────────────────────────────────
    let amounts = ran.watch.amounts();
    assert_eq!(
        amounts.len(),
        AT_ONCE,
        "the agent app was entered {} time(s) for {AT_ONCE} steps standing side by side. A \
         ceiling made hard by quietly turning three steps into one is not a ceiling this \
         product may have. What reached it was {amounts:?}",
        amounts.len()
    );
    assert_eq!(
        ran.report.steps,
        vec![StepState::Succeeded; AT_ONCE],
        "all three had money to run on, so all three have to finish. A step that ends as \
         skipped here means the first one reserved everything — the exact defect this measures. \
         It came out as {:?}",
        ran.report.steps
    );

    // ── (b) I NAPRAWDĘ NARAZ ────────────────────────────────────────────────────────────────
    // Przecięcie WSZYSTKICH trzech okien, nie pary: implementacja puszczająca dwa kroki razem
    // i trzeci po nich przechodzi każde pytanie o parę.
    let windows = ran.watch.windows();
    let shared = all_of_them_share(&windows);
    assert!(
        shared >= MIN_SHARED,
        "the three steps shared {shared:?} of a {TURN:?} turn, so there was no moment in which \
         all three were working. \"How many at once\" has to keep meaning at once even when a \
         person sets a ceiling (invariant 11): a ceiling that buys itself by running the steps \
         one after another takes away the whole reason this product exists. The windows were \
         {:?}",
        spans(&windows)
    );

    // ── (c) I KAŻDY DOSTAŁ SWÓJ UDZIAŁ, NIE CAŁĄ RESZTĘ ─────────────────────────────────────
    let mut handed: Vec<String> = amounts.iter().flatten().cloned().collect();
    handed.sort();
    assert_eq!(
        handed,
        SHARES.map(str::to_owned).to_vec(),
        "the three steps were told they may spend {handed:?} of a ${BUDGET:.2} ceiling. Each \
         one has to be told its SHARE of what is left, and the shares have to differ: handing \
         every step that starts the whole remainder lets a run of three spend three ceilings, \
         and reads as correct in any check that only asks whether the amount is there"
    );

    // ── (d) I RACHUNEK BIEGU MIEŚCI SIĘ W SUFICIE ───────────────────────────────────────────
    let spent = ran
        .run_file()?
        .get("spent_usd")
        .and_then(Json::as_f64)
        .ok_or("the run's own record does not say what it spent")?;
    assert!(
        spent <= BUDGET,
        "the run spent ${spent:.2} of a ${BUDGET:.2} ceiling. Each step spends what it was \
         allowed, so three steps told they may each spend the whole remainder spend three \
         remainders — and the person finds out from the invoice"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_codex_step_that_passes_its_share_is_stopped_and_says_so() -> Result<(), Box<dyn Error>> {
    let ran = one_run(
        "codex-over-its-share",
        ONE_CODEX_STEP,
        CODEX_HAND_FILE,
        1,
        CODEX_BUDGET,
    )
    .await?;

    let said_on_screen: Vec<String> = ran
        .delivered()?
        .iter()
        .filter(|row| row.get("kind").and_then(Json::as_str) == Some("problem"))
        .filter_map(|row| row.get("text").and_then(Json::as_str).map(str::to_owned))
        .collect();
    assert!(
        said_on_screen.iter().any(|text| text == STOPPED_SENTENCE),
        "the row this step left on screen said {said_on_screen:?}. This agent app has no \
         ceiling of its own to pass in its command line, so Loadout has to end the turn — and \
         say so where the person is looking. A mechanism that works and says nothing is the \
         blind spot this repository exists to remove (invariant 29)"
    );

    let step = ran
        .run_file()?
        .get("steps")
        .and_then(Json::as_array)
        .and_then(|steps| steps.first().cloned())
        .ok_or("the run's own record describes no steps at all")?;
    assert_eq!(
        step.get("error").and_then(Json::as_str),
        Some(STOPPED_SENTENCE),
        "the run's own record says {step}. The screen and the file have to carry the SAME \
         sentence: a person who comes back to a finished run reads the file, and a run that \
         only explains itself while it is open explains itself to nobody"
    );
    assert_ne!(
        ran.report.steps.first(),
        Some(&StepState::Succeeded),
        "a step Loadout had to end half way through its turn did not do what it was given, so \
         it may not be recorded as having succeeded. It came out as {:?}",
        ran.report.steps
    );
    Ok(())
}

/// Największe przecięcie WSZYSTKICH okien: od najpóźniejszego startu do najwcześniejszego końca.
/// Zero, kiedy choć jedna para się rozmija.
fn all_of_them_share(windows: &[(Instant, Instant)]) -> Duration {
    let (Some(latest_start), Some(earliest_end)) = (
        windows.iter().map(|&(from, _)| from).max(),
        windows.iter().map(|&(_, to)| to).min(),
    ) else {
        return Duration::ZERO;
    };
    earliest_end.saturating_duration_since(latest_start)
}

/// Okna jako czasy trwania — czytelne w komunikacie asercji.
fn spans(windows: &[(Instant, Instant)]) -> Vec<Duration> {
    windows
        .iter()
        .map(|&(from, to)| to.saturating_duration_since(from))
        .collect()
}

// ── jeden bieg ─────────────────────────────────────────────────────────────────────────────

/// Wszystko, co po biegu jest potrzebne do sądzenia.
struct Ran {
    /// Ławka trzymana przy życiu do końca sądzenia: `TempDir` kasuje swój katalog przy
    /// upuszczeniu, a `run.json` czytamy DOPIERO POTEM.
    _bench: Bench,
    report: RunReport,
    watch: Arc<Watch>,
    recorder: Delivered,
}

impl Ran {
    fn run_file(&self) -> Result<Json, Box<dyn Error>> {
        let text = fs::read_to_string(self.report.dir.join("run.json"))?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Wiersze, które **naprawdę wyszły kanałem** do okna.
    fn delivered(&self) -> Result<Vec<Json>, Box<dyn Error>> {
        self.recorder.lines()
    }
}

async fn one_run(
    slug: &str,
    workflow: &str,
    agent_file: &str,
    how_many_at_once: usize,
    budget_usd: f64,
) -> Result<Ran, Box<dyn Error>> {
    let bench = Bench::new()?;
    let hand = bench.agent(slug, agent_file)?;
    let path = bench.workflow(slug, workflow)?;
    the_fixture_can_run(&path, &[&hand])?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::default());

    let deps = RunDeps {
        home: bench.home.path(),
        library: bench.home.path().to_path_buf(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch)),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: path,
        how_many_at_once,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let recorder = Delivered::default();
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, recorder.channel());

    // `sink` wjeżdża do biegu i ginie razem z jego powrotem — dopiero wtedy pompa widzi koniec
    // producenta i wypycha ostatnią, niepełną paczkę. Wiersz, którego druga ławka szuka, bywa
    // właśnie w niej.
    let report = tokio::time::timeout(
        PATIENCE,
        run_workflow_with_budget(&deps, &request, sink, Some(budget_usd)),
    )
    .await
    .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))??;
    let _ = tokio::time::timeout(PATIENCE, pump).await;

    Ok(Ran {
        _bench: bench,
        report,
        watch,
        recorder,
    })
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

// ── co dubler zobaczył ─────────────────────────────────────────────────────────────────────

/// Jedno wejście do sterownika: z jaką kwotą i w jakim oknie czasu.
#[derive(Debug)]
struct Entered {
    /// Kwota stojąca zaraz za flagą sufitu, jeśli ten fragment argv ją w ogóle niósł.
    amount: Option<String>,
    from: Instant,
    to: Option<Instant>,
}

/// Obserwator sterownika: okno i kwota każdego uruchomienia.
///
/// Wejście zapisuje `start`, a wyjście — koniec tury, **przed** oddaniem miejsca z puli.
/// Zapisane okna leżą więc wewnątrz okien miejsc, nigdy poza nimi: pomiar może zaniżyć
/// nakładanie się, ale nie może go zmyślić.
#[derive(Debug, Default)]
struct Watch {
    seen: Mutex<Vec<Entered>>,
}

impl Watch {
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym
    /// wywołaniu, więc nie ma wyrażenia, w którym dożyłby do `await`.
    fn entered(&self, arguments: &[String]) -> usize {
        let mut seen = self.lock();
        seen.push(Entered {
            amount: amount_in(arguments),
            from: Instant::now(),
            to: None,
        });
        seen.len() - 1
    }

    /// Krok wyszedł, jakkolwiek się skończył. Pierwsze wyjście wygrywa.
    fn left(&self, at: usize) {
        let mut seen = self.lock();
        if let Some(one) = seen.get_mut(at) {
            one.to.get_or_insert_with(Instant::now);
        }
    }

    /// Kwoty w kolejności wejścia do sterownika — po jednej na uruchomienie.
    fn amounts(&self) -> Vec<Option<String>> {
        self.lock().iter().map(|one| one.amount.clone()).collect()
    }

    /// Domknięte okna. Okno bez końca nie wchodzi, i dlatego liczba wejść jest sprawdzana
    /// osobno.
    fn windows(&self) -> Vec<(Instant, Instant)> {
        self.lock()
            .iter()
            .filter_map(|one| Some((one.from, one.to?)))
            .collect()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Entered>> {
        // Zatruty zamek nie ma prawa zgubić pomiaru: panika w jednym kroku oślepiłaby asercję,
        // która akurat dowodzi, że pozostałe biegły naraz.
        self.seen.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Kwota stojąca zaraz za flagą sufitu, jeśli flaga w tym fragmencie w ogóle jest.
fn amount_in(fragment: &[String]) -> Option<String> {
    let at = fragment.iter().position(|argument| argument == FLAG)?;
    fragment.get(at + 1).cloned()
}

/// Wiersze, które **naprawdę wyszły kanałem** do okna, w kolejności wyjścia.
#[derive(Debug, Clone, Default)]
struct Delivered(Arc<Mutex<Vec<InvokeResponseBody>>>);

impl Delivered {
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

    fn lines(&self) -> Result<Vec<Json>, Box<dyn Error>> {
        let seen = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let mut out = Vec::new();
        for body in seen.iter().cloned() {
            out.extend(body.deserialize::<Vec<Json>>()?);
        }
        Ok(out)
    }
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers(watch: Arc<Watch>) -> Drivers {
    Arc::new(move |vendor| {
        Arc::new(Fake {
            watch: Arc::clone(&watch),
            vendor: match vendor {
                Vendor::ClaudeCode => CLAUDE,
                Vendor::Codex => CODEX,
            },
            arguments: Vec::new(),
        }) as Arc<dyn AgentDriver>
    })
}

/// Dubler, który **nazywa się tak, jak prawdziwy vendor**.
///
/// Ta nazwa jest treścią, nie ozdobą: fragment argv z sufitem niesie flagę, którą zna dokładnie
/// jeden vendor, więc rdzeń pyta krok o to, czym on jest. Dubler o cudzej nazwie nie dostałby
/// tego fragmentu i pomiar mierzyłby wtedy własną atrapę.
#[derive(Clone, Debug)]
struct Fake {
    watch: Arc<Watch>,
    vendor: &'static str,
    arguments: Vec<String>,
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        self.vendor
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some(self.vendor.to_owned()),
        })
    }

    fn configured(&self, configuration: &DriverConfiguration) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            arguments: configuration.arguments.clone(),
            ..self.clone()
        }))
    }

    /// Dubler nazywający się jak prawdziwy vendor MUSI umieć wziąć cel dowodów: bieg odmawia
    /// startu krokowi vendora, który tego szwu nie ma („cannot preserve its private run
    /// evidence"). Sam plik dowodu nie jest przedmiotem tego pomiaru.
    fn with_evidence(&self, _target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        // Zapisujemy PRZY STARCIE, nie w `configured`: sterownik bez ani jednego argumentu nie
        // przechodzi przez tamtą drogę w ogóle, więc krok bez flagi nie zostawiłby tam wpisu.
        let at = self.watch.entered(&self.arguments);
        let session = SessionRef {
            vendor: self.vendor,
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
            watch: Arc::clone(&self.watch),
            events,
            session,
            at,
            // KROK WYDAJE TO, NA CO MU POZWOLONO, i to jest cała treść tej linii: kwota z argv
            // wraca jako cena tury, więc bieg, który każdemu obiecuje całą resztę, naprawdę
            // wydaje trzy reszty. Krok, którego vendor flagi nie zna, wydaje ponad swój udział
            // i musi go zatrzymać Loadout.
            spends: amount_in(&self.arguments)
                .and_then(|amount| amount.parse::<f64>().ok())
                .unwrap_or(OVER_ITS_SHARE),
            says_it_is_spending: self.vendor == "codex",
        }))
    }
}

/// Jedna tura dublera.
#[derive(Debug)]
struct Turn {
    watch: Arc<Watch>,
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    at: usize,
    spends: f64,
    says_it_is_spending: bool,
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
        if self.says_it_is_spending {
            // W POŁOWIE TURY, nie na jej końcu: to jest jedyna chwila, w której zatrzymanie
            // jeszcze cokolwiek oszczędza.
            let _ = self
                .events
                .send(
                    AgentEvent::Spending {
                        estimate_usd: OVER_ITS_SHARE,
                    }
                    .into(),
                )
                .await;
        }
        tokio::time::sleep(TURN).await;
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "## Answer\nThe part is done.\n\n## Evidence\nnone\n\n## Open\nnothing.\n"
                .to_owned(),
            cost_usd: Some(self.spends),
            tokens: Tokens::default(),
            turns: 1,
            took: TURN,
            session: self.session.clone(),
        };
        self.watch.left(self.at);
        let _ = self
            .events
            .send((AgentEvent::Finished(outcome.clone())).into())
            .await;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> GroupProof {
        self.watch.left(self.at);
        // Dubler nie ma procesu, więc dowód zejścia jest tu prawdą z konstrukcji, a nie
        // uproszczeniem: nie ma czego zabijać i nie ma czego przeżyć (niezmiennik 6).
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        self.watch.left(self.at);
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
        // `Store::open` zakłada plik bazy, ale nie katalog nad nim.
        fs::create_dir_all(project.path().join(".loadout"))?;
        // Żeby „własna kopia twoich plików" miała co kopiować.
        fs::write(project.path().join("notes.txt"), "written by the human")?;
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
