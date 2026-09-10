//! Stop, „dalej" i „powiedz agentowi" sięgają do biegu SWOJEJ karty, nigdy do cudzej.
//!
//! # Co dokładnie było zepsute (audyt 2026-09-04, A-1 i A-2)
//!
//! `stop_run` nie brał folderu i wołał `AppState::stop_every_live_run`, czyli kończył bieg
//! w KAŻDYM żywym workspace. `continue_run` i `say_to_agent` brały `AppState::deps()`, czyli
//! „uchwyt, który ruszył ostatni". Skutek był jeden i ten sam trzy razy: człowiek naciskał Stop
//! na karcie `ledger`, a schodził też bieg w `atlas`; odpowiadał na punkt kontrolny w `ledger`,
//! a licznik zgód podbijał się biegowi w `atlas`; pisał do agenta w `ledger`, a zdanie szło do
//! agenta w `atlas`. Cudza praca ubita bez pytania i bez śladu jest błędem finansowym, nie
//! higienicznym (niezmiennik 6).
//!
//! Zapadka [`AppState::live`] jest kluczowana workspace'em od 2026-08-28, więc brakowało tylko
//! adresu — a ten okno ZNA, bo samo wysłało folder do `run_workflow`.
//!
//! # Dlaczego stan biegu czytamy `read_run_inner`, a nie z `RunReport`
//!
//! Niezmiennik 29. `RunReport::outcome` dowodzi, że mechanizm istnieje; słowo `succeeded`
//! przeczytane tą samą komendą, którą rysuje panel historii, dowodzi, że produkt działa.
//! Między jednym a drugim mieszka klasa wady, dla której to repo powstało.
//!
//! # Drogi zatrzymania — którą sądzi który plik
//!
//! Stop z karty i Stop z `/stop` w wierszu wejścia idą jedną skorupą (`ipc::stop_run`) i są tu,
//! niżej. Stop biegu z triggera idzie tą samą drogą: uchwyt siedzi w zapadce pod workspace'em
//! claimu (`AppState::begin_triggered_run`), więc adresuje go karta tego folderu. ⌘Q
//! i zamknięcie okna zostają JEDYNĄ drogą „każdy żywy folder"
//! ([`AppState::stop_every_live_run_before_closing`]) i sądzi je
//! `runs_latch_per_workspace.rs::closing_the_window_leaves_no_run_going`. Zamknięcie karty `×`
//! woła tę samą skorupę z folderem karty (`src/sections/run/tabs/store.ts`, `stopRunOf`).
//!
//! # Stany biegu, które muszą przeżyć cudzy Stop
//!
//! `succeeded` i `failed` — oba niżej. Adresem jest uchwyt, a nie wynik, więc bieg, którego
//! Stop nie dotknął, kończy się swoim własnym zakończeniem, jakiekolwiek by ono nie było;
//! „failed zamienione w cancelled" jest tą wersją wady, która wygląda jak zwykłe anulowanie.
//!
//! Runtime jest `current_thread` z zegarem wstrzymanym: pomiar nie dotyczy nakładania się
//! w czasie (od tego jest `runs_latch_per_workspace.rs`), tylko tego, KTÓRY uchwyt dostał
//! sygnał — a zegar wirtualny czyni tę kolejność deterministyczną.

// `expect()` w teście: panika w teście JEST jego wynikiem, a `?` w tej samej linii oddawałby
// błąd jako wartość, której nikt nie czyta. Ten sam idiom i ten sam powód, co w
// `say_to_agent_refusals.rs`.
#![allow(clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, GoOn, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, ToAgent, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{AppState, LineSink, QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::{Barrier, mpsc};

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Identyfikator zapisanego agenta, ten sam dla obu pasów.
const HAND: &str = "01990000-0000-7000-8000-0000000000f1";

/// Znak pasa pierwszego. Jedzie w instrukcji kroku, więc dubler czyta go z promptu.
const LEDGER: &str = "workledger";

/// Znak pasa drugiego.
const ATLAS: &str = "workatlas";

/// Nazwa kroku pasa pierwszego — tą nazwą okno adresuje „powiedz agentowi".
const LEDGER_STEP: &str = "Ledger hand";

/// Nazwa kroku pasa drugiego.
const ATLAS_STEP: &str = "Atlas hand";

/// Jak długo trzyma krok, zanim odda turę. Krótsze niż [`PATIENCE`], bo pod wstrzymanym zegarem
/// tokio przeskakuje do NAJBLIŻSZEGO terminu — sen dłuższy od limitu zamieniłby ten test
/// w „nie doczekaliśmy się" przy poprawnym kodzie.
const HOLD: Duration = Duration::from_secs(1);

/// Ile czekamy, zanim uznamy biegi za zawieszone. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_mins(1);

/// Suwak „ile naraz". Dwa, bo dwa pasy po jednym kroku mają się zmieścić razem — inaczej drugi
/// czekałby na miejsce i nigdy nie doszedł do bariery niżej.
const TOGETHER: usize = 2;

/// Pojemność udawanego głosu kroku. Jeden wpis wystarcza: sprawdzamy pierwszą linię.
const ROOM: usize = 4;

/// Powód porażki tury w paśmie, które ma paść samo z siebie.
const BROKE: &str = "the tool this step needed is not installed";

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000f1
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

/// (AC-1) Stop w `ledger` zostawia bieg w `atlas` przy życiu — i ten kończy się `succeeded`.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn stop_in_one_folder_leaves_the_other_run_alone() -> Result<(), Box<dyn Error>> {
    let (ledger, atlas) = stop_in_ledger_and_read_both(None).await?;

    assert_eq!(
        atlas, "succeeded",
        "a person pressed Stop on the ledger card and the run in atlas came back as {atlas:?}. \
         Stop reached every live folder at once, so one card took the other card's work down \
         with it — without a question and without a trace (invariants 6 and 11). The latch is \
         keyed by workspace; the only thing missing was the address."
    );
    assert_eq!(
        ledger, "cancelled",
        "the folder a person actually pressed Stop on came back as {ledger:?}. Stop that \
         addresses a folder still has to reach the run in it, or the button does nothing at all."
    );
    Ok(())
}

/// (AC-1) Bieg, którego Stop nie dotknął, kończy się SWOIM zakończeniem — także porażką.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_failing_run_elsewhere_keeps_its_own_ending() -> Result<(), Box<dyn Error>> {
    let (ledger, atlas) = stop_in_ledger_and_read_both(Some(ATLAS)).await?;

    assert_eq!(
        atlas, "failed",
        "the run in atlas broke on its own and the history says {atlas:?}. A cancellation \
         written over somebody else's failure is the version of this defect that looks like an \
         ordinary Stop: the person goes looking for what went wrong and finds a run they were \
         told they cancelled."
    );
    assert_eq!(
        ledger, "cancelled",
        "and the stopped folder is still the stopped one"
    );
    Ok(())
}

/// (AC-2) „Dalej" w `ledger` nie rusza licznika zgód uchwytu z `atlas`.
#[tokio::test]
async fn carry_on_in_one_folder_does_not_move_the_other_run() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let ledger = bench.folder("ledger")?;
    let atlas = bench.folder("atlas")?;
    let state = bench.app(fake_drivers(Bench::quiet(), None))?;
    let ledger_run = started_in(&state, &ledger)?;
    let atlas_run = started_in(&state, &atlas)?;

    /* NASŁUCH ZAŁOŻONY PRZED „DALEJ", tak jak zakłada go punkt kontrolny
     * (`RunControl::listen_for_go_on`): świeża subskrypcja `watch` liczy dopiero NASTĘPNĄ
     * zmianę, więc nasłuch założony po zgodzie nie zobaczyłby jej nigdy. */
    let ledger_told = ledger_run.control.listen_for_go_on();
    let atlas_told = atlas_run.control.listen_for_go_on();

    state
        .continue_the_run_in(&ledger, Some("ship it".to_owned()))
        .await?;

    assert!(
        said_go_on(ledger_told),
        "the checkpoint of the card a person answered is still waiting. \"Continue\" that does \
         not reach its own run leaves the workflow parked forever (commands::run::wait_for_a_person)"
    );
    assert!(
        !said_go_on(atlas_told),
        "answering a checkpoint on the ledger card let the run in atlas past ITS checkpoint. \
         The old road took \"the handle that started last\", so the question a person read on \
         one screen was answered on another — and nobody sees that happen"
    );
    assert_eq!(
        ledger_run.control.take_answer().as_deref(),
        Some("ship it"),
        "what the person wrote has to reach the run they wrote it to, not just the go-ahead"
    );
    Ok(())
}

/// (AC-2) Zdanie z karty `ledger` dochodzi do agenta z `ledger` i do żadnego innego.
#[tokio::test]
async fn a_word_for_one_folder_never_reaches_the_agent_in_the_other() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let ledger = bench.folder("ledger")?;
    let atlas = bench.folder("atlas")?;
    let state = bench.app(fake_drivers(Bench::quiet(), None))?;
    let ledger_run = started_in(&state, &ledger)?;
    let atlas_run = started_in(&state, &atlas)?;
    let mut ledger_heard = listening(&ledger_run.control, LEDGER_STEP);
    let mut atlas_heard = listening(&atlas_run.control, ATLAS_STEP);

    // Bez nazwy: „ten jeden, który pracuje" ma znaczyć „w TYM folderze", nie „gdziekolwiek".
    state
        .say_in_the_run_at(&ledger, None, "also add a dark mode toggle")
        .await?;

    let got = ledger_heard
        .try_recv()
        .expect("the line has to reach the channel of the agent working in this folder");
    assert_eq!(
        turn(&got),
        "also add a dark mode toggle",
        "the agent of this card has to get what the person wrote"
    );
    assert!(
        atlas_heard.try_recv().is_err(),
        "a sentence typed on the ledger card reached the agent working in atlas. The old road \
         picked the handle that started last, so the answer to \"who is working\" came from a \
         folder the person was not looking at — and the turn it costs is paid for either way"
    );

    // Z nazwą kroku z DRUGIEGO folderu: odmowa, nie doręczenie. Nazwa jest adresem wewnątrz
    // biegu, nigdy drogą do biegu obok.
    let said = state
        .say_in_the_run_at(&ledger, Some(ATLAS_STEP), "and here too")
        .await
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(
        said.contains(ATLAS_STEP),
        "naming a step of another folder's run has to be refused by name, not delivered: {said:?}"
    );
    assert!(
        atlas_heard.try_recv().is_err(),
        "the refusal above still let the line through to the other folder's agent"
    );
    Ok(())
}

/// (AC-3) Folder, w którym nic nie idzie, odpowiada `false` — i wraca bez czekania.
///
/// To jest rustowa połowa zdania `Nothing is running in <folder>.` Druga połowa — samo zdanie —
/// stoi tam, gdzie czyta je człowiek (`src/sections/run/entry/entry.tsx`, `whatStopSaid`).
///
/// Zegar wstrzymany, bo sufit czasu jest tu TREŚCIĄ, nie ostrożnością: na starym kodzie ta droga
/// nie wracała wcale — `stop_run_inner` czekał na dowód od biegu w SĄSIEDNIM folderze, którego
/// nikt nie zamierzał zatrzymywać. Wirtualny zegar zamienia to zawieszenie w zdanie, zamiast
/// w minutę ciszy.
#[tokio::test(start_paused = true)]
async fn stop_in_a_folder_where_nothing_is_going_answers_false() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let ledger = bench.folder("ledger")?;
    let quiet = bench.folder("quiet")?;
    let state = bench.app(fake_drivers(Bench::quiet(), None))?;
    let ledger_run = started_in(&state, &ledger)?;
    /* W SĄSIEDNIM FOLDERZE COŚ NAPRAWDĘ IDZIE, i bez tej linii to kryterium przechodzi także dla
     * Stopu sięgającego wszędzie: „nic tu nie idzie" byłoby wtedy prawdą o całej aplikacji.
     * `begin()` jest tą samą linią, którą bieg zapala u siebie (`run_workflow_with_slots`). */
    ledger_run.control.begin();

    let answered = tokio::time::timeout(PATIENCE, state.stop_the_run_in(&quiet))
        .await
        .map_err(|_| {
            format!(
                "Stop pressed over a folder where nothing ever ran did not come back within \
                 {PATIENCE:?}: it went looking for proof of death from the run in the folder next \
                 door. A button that hangs the window is worse than one that does nothing"
            )
        })??;

    assert!(
        !answered,
        "Stop over a folder with nothing going answered \"something was stopped\". That answer \
         is what the window turns into a sentence, so a wrong one reads as \"stopped\" over a \
         card where nothing ever started"
    );
    assert!(
        ledger_run.control.is_working(),
        "Stop pressed on a card where nothing runs took down the run in the folder next to it. \
         That is the same defect one screen further: the answer a person gets is about their own \
         card, and the work that dies belongs to somebody else"
    );
    Ok(())
}

/// Dwa biegi w dwóch folderach, Stop w `ledger` — i stan OBU biegów tam, gdzie czyta go człowiek.
///
/// `breaks` nazywa pas, którego tura ma wrócić porażką; `None` znaczy „obie się udają".
async fn stop_in_ledger_and_read_both(
    breaks: Option<&'static str>,
) -> Result<(String, String), Box<dyn Error>> {
    let bench = Bench::new()?;
    let ledger = bench.folder("ledger")?;
    let atlas = bench.folder("atlas")?;
    let ledger_file = bench.workflow(LEDGER, LEDGER_STEP)?;
    let atlas_file = bench.workflow(ATLAS, ATLAS_STEP)?;

    /* BARIERA NA TRZY, nie odpytywanie „czy oba pracują": `is_working` zapala się przy wejściu
     * w bieg, czyli ZANIM krok wszedł do sterownika, a Stop wysłany w tej szczelinie mierzyłby
     * co innego niż to, co się dzieje na ekranie. Trzecim uczestnikiem jest sam Stop, więc
     * kolejność jest ustalona bez ani jednego snu. */
    let both_inside = Arc::new(Barrier::new(3));
    let state = bench.app(fake_drivers(Arc::clone(&both_inside), breaks))?;
    let ledger_run = started_in(&state, &ledger)?;
    let atlas_run = started_in(&state, &atlas)?;

    let (ledger_sink, ledger_pump) = the_pump_seam();
    let (atlas_sink, atlas_pump) = the_pump_seam();
    let ledger_asks = asked_for(&ledger_file);
    let atlas_asks = asked_for(&atlas_file);

    let (ledger_ended, atlas_ended, answered, (), ()) = tokio::time::timeout(PATIENCE, async {
        tokio::join!(
            run_workflow_inner(&ledger_run, &ledger_asks, ledger_sink),
            run_workflow_inner(&atlas_run, &atlas_asks, atlas_sink),
            async {
                both_inside.wait().await;
                state
                    .stop_the_run_in(&ledger)
                    .await
                    .map_err(|error| error.to_string())
            },
            ledger_pump,
            atlas_pump,
        )
    })
    .await
    .map_err(|_| format!("the two runs did not both finish within {PATIENCE:?}"))?;

    assert!(
        answered?,
        "Stop pressed with a run going in this folder answered \"there was nothing to stop\""
    );
    // Tą samą komendą, którą rysuje panel historii (niezmiennik 29) — nie z `RunReport`.
    Ok((
        state_of(&ledger, &ledger_ended?)?,
        state_of(&atlas, &atlas_ended?)?,
    ))
}

/// Stan biegu przeczytany tak, jak czyta go okno.
fn state_of(project: &Path, report: &RunReport) -> Result<String, Box<dyn Error>> {
    let folder = report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the run directory has no UTF-8 folder name")?;
    Ok(read_run_inner(project, folder)?.state)
}

/// Czy ten nasłuch dostał już „dalej". Jedno odpytanie i ani jednego czekania.
///
/// Sonda, a nie `await`: pytanie brzmi „czy zgoda PADŁA", a `wait()` na nasłuchu, do którego
/// nikt się nie odezwał, po prostu nie wraca. Ten sam kształt i ten sam `Waker::noop()`, co
/// w zapadce startu biegu (`ipc.rs`, `proved_down`).
fn said_go_on(told: GoOn) -> bool {
    let mut waiting = pin!(told.wait());
    matches!(
        waiting
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(true)
    )
}

/// Krok, który słucha, plus odbiornik jego linii.
///
/// Odbiornik wraca do wołającego, bo bez niego kanał ginie razem z funkcją i każda wysyłka
/// odmawiałaby „stopped listening" — czyli test mierzyłby własne sprzątanie.
fn listening(control: &RunControl, step: &str) -> mpsc::Receiver<ToAgent> {
    let (voice, heard) = mpsc::channel(ROOM);
    control.step_can_hear(step, voice);
    heard
}

/// Treść tury, którą usłyszał kanał kroku.
fn turn(said: &ToAgent) -> &str {
    match said {
        ToAgent::Turn(text) => text,
        ToAgent::Interrupt(_) => "an interrupt, which is not a turn from a person",
    }
}

/// Żądanie biegu z tego pliku workflow.
fn asked_for(file: &Path) -> RunRequest {
    RunRequest {
        workflow: file.to_path_buf(),
        how_many_at_once: TOGETHER,
        task: None,
        part: None,
        handoffs_from: None,
    }
}

/// Uchwyt biegu w tym folderze — PRODUKCYJNYMI DRZWIAMI, nie ręcznie sklejonym `RunDeps`.
///
/// Bierzemy go tam, gdzie bierze go skorupa komendy ([`AppState::begin_run`]), bo zapadka
/// mieszka dokładnie tam i tylko tędy da się o nią zapytać. Wolna funkcja, nie metoda [`Bench`]:
/// ławka nie ma tu nic do dodania, a `&self`, którego ciało nie czyta, jest zaproszeniem do
/// wiary, że coś z niej bierze.
fn started_in<'a>(state: &'a AppState, project: &'a Path) -> Result<RunDeps<'a>, Box<dyn Error>> {
    state.begin_run(project).map_err(|said| {
        format!(
            "a Start in a second folder was turned down while a run was going in the first \
             one: {said}"
        )
        .into()
    })
}

/// Szew, którym bieg mówi do okna: nadajnik dla biegu i czekanie na pompę.
fn the_pump_seam() -> (LineSink, impl Future<Output = ()>) {
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    (sink, async move {
        let _ = pump.await;
    })
}

/// Biblioteka użytkownika, folder startowy aplikacji i foldery obu pasów.
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
        let bench = Self { home, project };
        bench.agent(HAND_FILE)?;
        Ok(bench)
    }

    /// Bariera dla ławek, w których nikt na nikogo nie czeka — dubler nigdy do niej nie dochodzi.
    fn quiet() -> Arc<Barrier> {
        Arc::new(Barrier::new(1))
    }

    /// Folder pracy jednego pasa. Pod folderem startowym aplikacji, bo `TempDir` na macOS leży
    /// pod dowiązaniem `/var` → `/private/var`: dwa różne zapisy tej samej ścieżki są dokładnie
    /// tym, co kanoniczna tożsamość ma skleić w jeden klucz.
    fn folder(&self, name: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.project.path().join(name);
        // 2026-09-10: each test workspace explicitly owns its agent definition.
        fs::create_dir_all(path.join(".loadout/agents"))?;
        fs::write(path.join(".loadout/agents/hand.md"), HAND_FILE)?;
        Ok(path)
    }

    fn agent(&self, text: &str) -> Result<(), Box<dyn Error>> {
        fs::write(self.home.path().join("agents").join("hand.md"), text)?;
        Ok(())
    }

    fn app(&self, drivers: Drivers) -> Result<AppState, Box<dyn Error>> {
        Ok(AppState::new(
            self.home.path().to_path_buf(),
            self.project.path().to_path_buf(),
            Store::open(&self.project.path().join(".loadout").join("loadout.db"))?,
            drivers,
        ))
    }

    /// Plik workflow jednego pasa: JEDEN krok ze znakiem pasa w instrukcji.
    fn workflow(&self, mark: &str, step: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self
            .home
            .path()
            .join("workflows")
            .join(format!("{mark}.json"));
        fs::write(
            &path,
            format!(
                "{{\"format\":1,\"id\":\"wf_{mark}\",\"name\":\"Lane {mark}\",\"steps\":[\
                 {{\"kind\":\"agent\",\"id\":\"s_{mark}\",\"name\":\"{step}\",\
                 \"agent\":\"{HAND}\",\"overrides\":{{}},\"instructions\":\"{mark}\",\
                 \"folder\":{{\"use\":\"project\"}},\"at\":{{\"x\":0,\"y\":0}}}}],\"links\":[]}}"
            ),
        )?;
        the_fixture_can_run(&path, &self.home.path().join("agents").join("hand.md"))?;
        Ok(path)
    }
}

/// Fikstura ma przejść walidator **bez ani jednego problemu**, a jej plik agenta ma dać się
/// przeczytać.
///
/// To nie jest część kryterium, tylko jego przesłanka, i dlatego stoi przed biegiem. Czerwień
/// w fazie kontraktu wygląda identycznie dla „zachowania jeszcze nie ma" i dla „tego kryterium
/// nie da się spełnić nigdy".
fn the_fixture_can_run(workflow: &Path, agent: &Path) -> Result<(), Box<dyn Error>> {
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
    read_agent_file(agent).map_err(|error| format!("{}: {error}", agent.display()))?;
    Ok(())
}

/// Fabryka, która dla każdego vendora oddaje ten sam dubler.
fn fake_drivers(inside: Arc<Barrier>, breaks: Option<&'static str>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { inside, breaks });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Znak pasa odczytany z promptu kroku. Nieznany prompt to wada fikstury, nie wynik pomiaru.
fn lane_of(prompt: &str) -> &'static str {
    if prompt.contains(LEDGER) {
        LEDGER
    } else {
        assert!(
            prompt.contains(ATLAS),
            "a step reached the agent app carrying neither folder's mark: {prompt:?}"
        );
        ATLAS
    }
}

/// Dubler: melduje wejście na barierze i trzyma turę mierzalnie długo.
#[derive(Debug)]
struct Fake {
    inside: Arc<Barrier>,
    breaks: Option<&'static str>,
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
        let lane = lane_of(&spec.prompt);
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

        // OBA PASY SĄ W ŚRODKU, ZANIM KTOKOLWIEK NACIŚNIE STOP — powód przy `both_inside`.
        self.inside.wait().await;

        Ok(Box::new(Turn {
            events,
            session,
            broke: self.breaks == Some(lane),
        }))
    }
}

/// Jedna tura dublera.
#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    broke: bool,
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
            ok: !self.broke,
            reason: if self.broke {
                FinishReason::Failed(BROKE.to_owned())
            } else {
                FinishReason::Completed
            },
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
