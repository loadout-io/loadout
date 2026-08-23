//! CAŁA DROGA PRODUKTU, przez prawdziwe ścieżki zapisu: agent → workflow → bieg → linie.
//!
//! PO CO TO ISTNIEJE, i to jest jedyny powód. Audyt 2026-08-18 przeliczył 231 kryteriów akceptacji
//! w `tasks/` i nie znalazł ANI JEDNEGO, które przechodzi drogę użytkownika. Droga „zapisz agenta
//! → zbuduj workflow, który go nazywa → uruchom → zobacz wynik" była pocięta na cztery odcinki
//! sądzone ROZŁĄCZNYMI wyroczniami: klik Start na atrapie `invoke`, bieg na fiksturze **zapisanej
//! przez test**, prawdziwy `claude` bez okna i za flagą `--ignored`, linia w oknie na podstawionym
//! kanale. Każdy z czterech był zielony, a produkt nie działał: baza miała `runs=0`, katalog
//! `~/.loadout/agents` nie istniał, a dziennik właściciela — siedemnaście nieudanych startów.
//!
//! CZYM TEN PLIK RÓŻNI SIĘ OD `runcmd_end_to_end.rs`, obok którego stoi. Tamten dowodzi, że plik
//! workflow przechodzi przez silnik w kolejności grafu — i **sam sobie pisze fiksturę**
//! (`fs::write` prosto do `home/agents/planner.md`). Czyli sądzi silnik, a nie produkt: gdyby
//! `save_agent_inner` odmawiał każdemu agentowi, tamto kryterium byłoby dalej zielone. Tutaj plik
//! agenta i plik workflow powstają **tymi samymi funkcjami, które wołają komendy okna**, a jedyne
//! `fs::write` w całym pliku to brak takiego wywołania.
//!
//! SŁABA WERSJA TEGO KRYTERIUM ma dwa kształty i oba tu odpadają:
//!
//!   1. `assert!(report.is_ok())`. Przechodzi dla biegu, który nie uruchomił ani jednego kroku:
//!      workflow z zerem kroków kończy się `Ok` i pustym katalogiem. Odróżnia je (d) — asercja
//!      na tym, że prompt Z KROKU dojechał do sterownika — i (e), która żąda przejść stanu.
//!   2. Fikstura pisana `fs::write`. Przechodzi na zepsutym zapisie agenta, czyli przepuszcza
//!      dokładnie tę awarię, która wywróciła produkt: zapis agenta padał w ciszy, katalog nie
//!      powstawał, i KAŻDY bieg kończył się „No such file or directory (os error 2)".
//!
//! CZEGO TO NIE DOWODZI, i to jest granica, nie kompromis. Nie klika po prawdziwym oknie: na
//! macOS okno Tauri to `WKWebView` i nie ma czym nim wysterować (`tauri-driver` obsługuje Linuksa
//! i Windows). Odcinek okno→Rust dowodzą kryteria po stronie frontu plus `commands-wired`, a ten
//! plik zaczyna się dokładnie tam, gdzie one się kończą — na funkcjach, które wołają skorupy
//! `#[tauri::command]`.

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::workflows::save_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::{Agent, Vendor};
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check, check_to_run};
use loadout_lib::workflow::{AgentStep, Link, Step, WorkflowFile};
use tauri::ipc::{Channel, InvokeResponseBody};
use tempfile::TempDir;
use uuid::Uuid;

/// Vendor dublera. Ten sam napis, którym `Vendor::ClaudeCode` nazywa się na dysku.
const VENDOR: &str = "claude";

/// Nazwa kroku. Jedzie na ekran jako podpis wiersza, więc jest zdaniem po ludzku.
const STEP_NAME: &str = "Ship the header row";

/// Klucz kroku w pliku workflow — to po nim okno rozpoznaje swój blok na pasku loadoutu.
const STEP_KEY: &str = "s_1";

/// Treść zadania. Ta wartość musi dojechać do sterownika, inaczej pole „What to do" jest ozdobą.
const WHAT_TO_DO: &str = "Add the header row and keep the tests green.";

/// Ile trwa tura dublera. Prawdziwy sen, nie czas wirtualny.
const TURN: Duration = Duration::from_millis(30);

/// Ile czekamy na cały bieg, zanim uznamy, że wisi.
const PATIENCE: Duration = Duration::from_secs(20);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_saved_agent_a_saved_workflow_and_a_run_that_actually_ran() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;

    // ── (a) AGENT POWSTAJE TĄ SAMĄ FUNKCJĄ, KTÓRĄ WOŁA `save_agent` ──────────────────────────
    //
    // Katalog `agents/` NIE jest tu zakładany z góry i to jest asercja sama w sobie: na świeżej
    // maszynie go nie ma, a `save_agent_inner` ma go zrobić. Właśnie dlatego produkt nie
    // działał — zapis padał w ciszy, katalogu nie było, a `find_agent` odpowiadał zdaniem
    // systemu plików.
    let mut agent = Agent::example();
    agent.id = Uuid::now_v7();
    agent.name = "Builder".to_owned();
    agent.runs_with = Vendor::ClaudeCode;
    let agent_file = save_agent_inner(bench.home.path(), &agent)?;
    assert!(
        agent_file.is_file(),
        "saving an agent has to leave a file behind, and it has to make its own directory: on a \
         fresh machine ~/.loadout/agents does not exist. This is the failure that broke the \
         product — the write refused in silence, so every run afterwards answered with a \
         filesystem error instead of a sentence about agents. Expected a file at {}",
        agent_file.display()
    );

    // ── (b) WORKFLOW NAZYWA TEGO AGENTA i przechodzi walidator biegu ─────────────────────────
    let workflow = one_step_for(&agent, WHAT_TO_DO);
    let blockers: Vec<String> = check_to_run(&workflow)
        .into_iter()
        .filter(|note| note.level == Level::Problem)
        .map(|note| note.message)
        .collect();
    assert!(
        blockers.is_empty(),
        "the workflow this test builds would be refused before it ran, so nothing below would \
         mean what it says. The validator said: {blockers:?}"
    );
    let saved = save_workflow_inner(bench.home.path(), "ship-a-feature.json", &workflow)?;
    assert!(
        saved.is_file(),
        "saving a workflow has to leave a file behind at {}",
        saved.display()
    );

    // ── (c) BIEG IDZIE NA TYM, CO NAPRAWDĘ LEŻY NA DYSKU ────────────────────────────────────
    //
    // `request.workflow` jest ścieżką ODDANĄ PRZEZ ZAPIS, nie ścieżką zbudowaną w teście: gdyby
    // zapis kładł plik gdzie indziej, niż twierdzi, bieg by go nie znalazł i to kryterium
    // padłoby tutaj, a nie na asercji o wierszach.
    let store = Store::open(&bench.db())?;
    let seen = Watched::default();
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: seen.drivers(),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: saved,
        how_many_at_once: 2,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let recorder = Delivered::default();
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, recorder.channel());
    let collect = async move {
        let _ = pump.await;
    };
    let (report, ()) = tokio::time::timeout(PATIENCE, async {
        tokio::join!(run_workflow_inner(&deps, &request, sink), collect)
    })
    .await
    .map_err(|_| format!("the run did not finish within {PATIENCE:?}"))?;
    let report = report?;

    // ── (d) PROMPT Z KROKU DOJECHAŁ DO STEROWNIKA ──────────────────────────────────────────
    //
    // To jest asercja, której brak przepuszczał najdroższy defekt frontu: pola „What to do" nie
    // było w panelu świeżego kroku, więc oba workflow właściciela miały `"instructions": ""`.
    // Bieg z pustym promptem kończy się `Ok` i wygląda dokładnie jak bieg udany.
    //
    // 2026-08-23 (T-86) — RÓWNOŚĆ CAŁEGO PROMPTU PRZESTAŁA BYĆ TYM PYTANIEM. Od T-86 każdy krok
    // agenta dostaje na końcu promptu stały blok o tym, co oddaje dalej i ile ma czasu, więc
    // prompt nie JEST już instrukcją — instrukcja stoi na jego początku. Zdanie tej asercji
    // zostaje słowo w słowo, bo jest nadal prawdziwe; zmienia się wyłącznie forma.
    //
    // Trzy warunki naraz, nie jedno `contains`. Samo `contains` przepuszcza prompt, w którym
    // zdanie człowieka jest doklejone na końcu albo stoi dwa razy — czyli dokładnie ten defekt,
    // po którym tę asercję napisano. `len() == 1` mówi „raz dojechało", `starts_with` — „dojechało
    // pierwsze, przed naszym blokiem", a liczba wystąpień — „dokładnie raz, nie w dwóch kopiach".
    let prompts = seen.prompts();
    assert!(
        prompts.len() == 1
            && prompts[0].starts_with(WHAT_TO_DO)
            && prompts[0].matches(WHAT_TO_DO).count() == 1,
        "the step's instructions have to reach the driver, once, word for word. Anything else \
         means the field a person types into is decoration: a run with an empty prompt finishes \
         Ok and looks exactly like a run that worked. What the driver was told: {prompts:#?}"
    );

    // ── (e) STAN KROKU WRÓCIŁ DO OKNA, i to w obu przejściach ───────────────────────────────
    //
    // Bez tego pasek loadoutu stoi na obrysach przez cały bieg, a kafelek agenta, który właśnie
    // edytuje pliki, pokazuje „waiting". Sześć z siedmiu stanów było po stronie okna
    // nieosiągalnych, bo na drucie nie było dla nich nośnika.
    let states = recorder.step_states()?;
    assert_eq!(
        states,
        vec![
            (STEP_KEY.to_owned(), "running".to_owned()),
            (STEP_KEY.to_owned(), "succeeded".to_owned()),
        ],
        "the window has to hear that the step started and that it finished, by the key it knows \
         from the workflow file. A run that only ever says `pending` leaves the loadout strip on \
         outlines for its whole length."
    );

    // ── (f) BIEG ZOSTAWIŁ PO SOBIE ŚLAD NA DYSKU ───────────────────────────────────────────
    assert!(
        report.dir.is_dir(),
        "the run has to leave its directory behind: files are the truth, the database is an \
         index (invariant 4). Expected {}",
        report.dir.display()
    );
    assert!(
        report.dir.join("run.json").is_file(),
        "and run.json inside it — that file, not the database, is what makes a finished run \
         readable after loadout.db is deleted"
    );

    Ok(())
}

/// Bieg NIE RUSZA, kiedy krok nie mówi, co zrobić — i mówi o tym zdaniem, nie ciszą.
///
/// PO CO TO ISTNIEJE, zmierzone na pierwszym prawdziwym biegu tego produktu (2026-08-18).
/// Właściciel uruchomił workflow, którego oba kroki miały `"instructions": ""`. Bieg wystartował,
/// dwóch agentów Claude przepracowało trzy tury i $0,12, a odpowiedź, którą dostał, brzmiała:
/// „both have empty `instructions` — so the task description is blank there too. What would you
/// like me to implement?". Loadout wiedział o tym PRZED startem — treść kroku leży w pliku, który
/// właśnie przeczytał — i nie powiedział ani słowa.
///
/// SŁABA WERSJA TEGO KRYTERIUM: sprawdzić, że `check_to_run` oddaje NIEPUSTĄ listę. Przechodzi
/// dla implementacji, która odmawia z jakiegokolwiek powodu — także złego. Odróżnia je pytanie
/// o TREŚĆ zdania: człowiek ma z niego wiedzieć, KTÓRY kafelek i KTÓRE pole, inaczej odmowa jest
/// tylko szybszym sposobem na utknięcie.
///
/// Druga połowa asercji jest równie ważna i łatwo ją pominąć: przy ZAPISIE ten sam plik nie ma
/// prawa być odrzucony. Szkic, w którym człowiek dodał kafelek i jeszcze nie napisał zadania,
/// jest normalnym stanem pracy — a zapis, który go odrzuca, kasuje pracę w chwili, gdy ktoś
/// pracuje. To jest ta sama para wag, co przy braku agenta.
#[test]
fn a_step_with_nothing_to_do_is_refused_before_anything_runs() {
    let mut agent = Agent::example();
    agent.id = Uuid::now_v7();
    /* Same spacje, nie pusty napis: `""` przechodziłby też przez implementację, która porównuje
     * z pustym napisem zamiast przycinać — a plik poprawiony ręcznie ma tam spację częściej niż
     * pustkę. Fikstura bierze treść ARGUMENTEM, więc krok nie musi być mutowany po zbudowaniu
     * i ten plik nie potrzebuje ani jednej paniki. */
    let workflow = one_step_for(&agent, "   ");

    let running: Vec<String> = check_to_run(&workflow)
        .into_iter()
        .filter(|note| note.level == Level::Problem)
        .map(|note| note.message)
        .collect();
    assert!(
        running.iter().any(|said| said.contains(STEP_NAME)),
        "pressing Run on a step with nothing to do has to be refused BY NAME. It answered: \
         {running:?}"
    );
    assert!(
        running.iter().any(|said| said.contains("What to do")),
        "and the refusal has to name the field to fill in — \"What to do\" is what the panel \
         calls it, so the person reads the words they can see. It answered: {running:?}"
    );

    let saving: Vec<String> = check(&workflow)
        .into_iter()
        .filter(|note| note.level == Level::Problem)
        .map(|note| note.message)
        .collect();
    assert!(
        saving.is_empty(),
        "the same file has to SAVE without complaint: a step added and not yet written is a \
         normal state of work, and a save that refuses it throws away what a person just typed. \
         Saving said: {saving:?}"
    );
}

/// Workflow z jednym krokiem, który nazywa tego agenta i nosi treść zadania.
///
/// Jeden krok, nie dwa, i to jest wybór: kolejność w grafie dowodzi `runcmd_end_to_end`, a to
/// kryterium pyta o coś innego — czy droga z okna na dysk i z dysku w proces jest ciągła. Drugi
/// krok dokładałby do niego regułę „dwa kroki, jeden folder", czyli mierzyłby walidator.
fn one_step_for(agent: &Agent, task: &str) -> WorkflowFile {
    WorkflowFile {
        format: 1,
        id: Uuid::now_v7().to_string(),
        name: "Ship a feature".to_owned(),
        description: None,
        steps: vec![Step::Agent(AgentStep {
            id: STEP_KEY.to_owned(),
            name: STEP_NAME.to_owned(),
            agent: agent.id.to_string(),
            overrides: serde_json::Map::new(),
            vendor_options: std::collections::BTreeMap::new(),
            copies: 1,
            instructions: task.to_owned(),
            skills: loadout_lib::workflow::Skills::default(),
            folder: loadout_lib::workflow::Folder::default(),
            handover: loadout_lib::workflow::Handover::default(),
            when_it_fails: loadout_lib::workflow::WhenItFails::Stop,
            at: loadout_lib::workflow::Point::default(),
            extra: serde_json::Map::new(),
        })],
        links: Vec::<Link>::new(),
        extra: serde_json::Map::new(),
    }
}

/// Biblioteka użytkownika i projekt na czas jednego kryterium.
///
/// `agents/` i `workflows/` **nie są** tu zakładane: ich powstanie jest częścią tego, co ten plik
/// sprawdza. Katalog `.loadout/` w projekcie jest, bo `Store::open` zakłada plik bazy, ale nie
/// katalog nad nim — i to jest fakt o bazie, nie o produkcie.
struct Bench {
    home: TempDir,
    project: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        Ok(Self { home, project })
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }
}

/// Co bieg powiedział sterownikowi. Nagrywamy prompt, bo to jedyne pole, które przychodzi
/// z kroku, a nie z definicji agenta.
#[derive(Debug, Clone, Default)]
struct Watched(Arc<Mutex<Vec<String>>>);

impl Watched {
    fn drivers(&self) -> Drivers {
        let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
            watched: Self(Arc::clone(&self.0)),
            evidence: None,
        });
        Arc::new(move |_vendor| Arc::clone(&driver))
    }

    fn prompts(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// Dubler sterownika: zapamiętuje prompt, mówi jedno zdanie i kończy turę sukcesem.
#[derive(Debug)]
struct Fake {
    watched: Watched,
    /// Zachowuje obowiązkowy cel prywatnego dowodu; zapis nadal wykonuje produkcyjny silnik.
    evidence: Option<EvidenceTarget>,
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
        events: tokio::sync::mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let _evidence = self
            .evidence
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("the run omitted its evidence target"))?;
        self.watched
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(spec.prompt.clone());

        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };
        events
            .send(
                AgentEvent::Said {
                    text: "Header row added.".to_owned(),
                }
                .into(),
            )
            .await?;
        Ok(Box::new(Turn {
            session,
            events,
            done: false,
        }))
    }

    fn with_evidence(&self, target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            watched: self.watched.clone(),
            evidence: Some(target),
        }))
    }
}

/// Jedna tura dublera.
struct Turn {
    session: SessionRef,
    events: tokio::sync::mpsc::Sender<DecodedEvent>,
    done: bool,
}

impl std::fmt::Debug for Turn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Turn")
            .field("session", &self.session.id)
            .field("done", &self.done)
            .finish_non_exhaustive()
    }
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
        if self.done {
            anyhow::bail!("this fake has exactly one turn");
        }
        self.done = true;
        // Prawdziwy sen, nie czas wirtualny: krok, który kończy się w tej samej mikrosekundzie,
        // w której zaczął, nie pozwala odróżnić dwóch przejść stanu od jednego.
        tokio::time::sleep(TURN).await;
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "Header row added.".to_owned(),
            cost_usd: Some(0.001),
            tokens: Tokens::default(),
            turns: 1,
            took: TURN,
            session: self.session.clone(),
        };
        self.events
            .send(AgentEvent::Finished(outcome.clone()).into())
            .await?;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> GroupProof {
        // Dubler nie ma procesu, więc dowód śmierci jest tu prawdą z konstrukcji, a nie
        // uproszczeniem: nie ma czego zabijać i nie ma czego przeżyć (niezmiennik 6).
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

/// Paczki, które naprawdę wyszły kanałem — czyli to, co dojechało do okna.
///
/// Nagrywamy po stronie OKNA, nie po stronie kolejki do pompy: to, co bieg oddał `LineSink`,
/// mówi wyłącznie o intencji, a pytanie tego kryterium brzmi „co dojechało".
#[derive(Debug, Clone, Default)]
struct Delivered(Arc<Mutex<Vec<InvokeResponseBody>>>);

impl Delivered {
    /// Kanał, który pompa dostanie zamiast webviewa.
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

    /// Pary `(stepId, state)` z wierszy `stepState`, w kolejności przybycia.
    fn step_states(&self) -> Result<Vec<(String, String)>, Box<dyn Error>> {
        let mut out = Vec::new();
        let seen = self
            .0
            .lock()
            .map_err(|error| format!("the recorder was poisoned: {error}"))?;
        let mut rows: Vec<serde_json::Value> = Vec::new();
        for body in seen.iter() {
            rows.extend(body.clone().deserialize::<Vec<serde_json::Value>>()?);
        }
        for row in &rows {
            if row.get("kind").and_then(serde_json::Value::as_str) != Some("stepState") {
                continue;
            }
            let key = row
                .get("stepId")
                .and_then(serde_json::Value::as_str)
                .ok_or("a stepState row arrived without a stepId")?;
            let state = row
                .get("state")
                .and_then(serde_json::Value::as_str)
                .ok_or("a stepState row arrived without a state")?;
            out.push((key.to_owned(), state.to_owned()));
        }
        Ok(out)
    }
}
