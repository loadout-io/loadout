//! Uruchom, zatrzymaj, wznów. Trzy funkcje domykające pętlę *płótno → plik → silnik → linie*.
//!
//! Nic tutaj nie jest nową zdolnością. Wszystko jest już zbudowane osobno: planista dowiódł
//! równoległości (T-02), nadzór dowiódł śmierci grupy procesów (T-03), walidator dowiódł odmów
//! (T-12), płótno dowiodło zapisu (T-13). Ten plik jest jedynym miejscem, w którym widać, czy
//! te rzeczy do siebie pasują — i dlatego cicha porażka wygląda tu inaczej niż gdziekolwiek
//! indziej: **wszystko działa osobno, a bieg i tak idzie sekwencyjnie**, bo liczba „ile naraz"
//! z UI jest wczytywana, logowana i nigdzie nie podawana. Semafor dostaje `1`, każdy test
//! przechodzi, bo wszyscy agenci naprawdę skończyli, i dokładnie tak przegrał poprzedni prototyp
//! (`docs/handoff.md:144-165`, niezmiennik 11).
//!
//! # Kolejność, której nie wolno odwrócić
//!
//! `docs/ARCHITECTURE.md` §4, czytane od góry: **wczytaj plik → sprawdź go jeszcze raz → dopiero
//! potem cokolwiek utwórz.** Bieg nie ufa UI (T3 §5.2): plik mógł zostać zmergowany gitem między
//! zapisem a naciśnięciem Start, więc odmowa pada **przed** katalogiem biegu i przed pierwszym
//! procesem. Implementacja, która najpierw tworzy katalog i odpala krok, a waliduje po drodze,
//! pali pieniądze na workflow odrzuconym pięć sekund później i zostawia po sobie pusty
//! `runs/<ts>__<id>/`.
//!
//! # Cztery pułapki, każda o jedną linijkę tańsza od wersji poprawnej
//!
//! 1. **`tokio::time::timeout(dur, step)` wokół kroku.** Wygląda na limit czasu i anuluje
//!    **zadanie Rusta, nie proces systemowy** (niezmienniki 6 i 10). Zostawia żywego agenta
//!    palącego limit u dostawcy. Każda ścieżka anulowania przechodzi przez `AgentHandle::cancel`,
//!    bo tylko ona wraca z `GroupProof`, a nie z „wysłałem sygnał".
//! 2. **`Err(Cancelled)`.** Anulowanie jest wartością (niezmiennik 7, [`Outcome::Cancelled`]).
//!    Krok po anulowaniu jest `cancelled`, jego potomkowie też — **nie `skipped`**, bo `skipped`
//!    znaczy „ktoś wyżej padł" i UI kłamałoby o powodzie (`docs/ARCHITECTURE.md` §5).
//! 3. **Instrukcje kroku w argv.** Prompt jedzie wyłącznie stdinem (niezmiennik 9); ta warstwa
//!    nie skleja komendy i nie zna ani jednej flagi vendora — wkłada instrukcje do
//!    `RunSpec::prompt` jako dane i oddaje je sterownikowi.
//! 4. **Referencja zamiast migawki.** `run.json` zapisuje konfigurację **efektywną**
//!    (`library::agents::resolve`) zamrożoną w chwili startu [T4 §5.2 p. 3]. Migawka będąca
//!    referencją zostawia pytanie „dlaczego zeszłotygodniowy bieg zachował się inaczej" bez
//!    odpowiedzi po każdej edycji szablonu [T4 §10, ryzyko 1].
//!
//! # `run.json` — kształt, który czytają dwa zadania
//!
//! Plik leży w `<projekt>/.loadout/runs/<ts>__<id>/run.json` i jest **prawdą** o biegu;
//! `loadout.db` jest jego indeksem i wolno go skasować (niezmiennik 4). Klucze biegu i kroków
//! są dokładnie tymi, które czyta `store::rebuild` — rozjazd znaczy, że po skasowaniu bazy
//! dostaje się co innego, niż się miało. Do tego dwa klucze, których wymaga T-15:
//!
//! ```json
//! {
//!   "id": "…uuid v7…",
//!   "workflow_id": "ship-a-feature",
//!   "workflow_hash": "…",          // ← „czy to był ten sam plan?"
//!   "workflow_snapshot": { … },    // graf JAK BIEGŁ
//!   "title": "Ship a feature",
//!   "status": "running | paused | succeeded | failed | cancelled",
//!   "concurrency": 3,
//!   "steps": [
//!     {
//!       "id": "…uuid v7…", "node_key": "build", "name": "Build", "agent": "claude",
//!       "depends_on": ["plan"], "status": "succeeded", "attempt": 0,
//!       "effective": { "id": "…uuid agenta…", "model": "opus", "thinking": "deep", … }
//!     }
//!   ]
//! }
//! ```
//!
//! `effective` jest **dosłowną** serializacją `library::agents::Agent` po złożeniu nadpisań
//! kroku, więc jego klucze są w camelCase — to jest migawka cudzego kształtu, nie nasz schemat.
//! `status` biegu jest jedynym miejscem, w którym istnieje `paused`: to jest stan **biegu**,
//! nigdy kroku (`docs/ARCHITECTURE.md` §5, to usuwa całą ćwiartkę stanów).
//!
//! # Kto tu z kim rozmawia
//!
//! ```text
//! run_workflow_inner ─ plan_run ─ workflow::{load, check} ─ library::agents::resolve
//!         │                            odmowa pada TUTAJ, przed pierwszym katalogiem
//!         ▼
//!   katalog biegu + run.json          ← plik istnieje, zanim ruszy pierwszy krok
//!         │
//!         ▼
//!   scheduler::execute(graf, ile naraz, token)   ← liczba z ŻĄDANIA, nie ze stałej
//!         │                                        (niezmiennik 11)
//!         ├─ krok agenta:   AgentDriver::start → AgentEvent → Curator → Vec<Line>
//!         └─ krok kontrolny: status biegu = paused, czekaj na „dalej"
//!         │
//!         ▼
//!   run.json (stany końcowe od planisty) → store::rebuild_from   ← indeks Z PLIKÓW
//! ```
//!
//! Ostatnia strzałka jest tu z rozmysłem i jest całym niezmiennikiem 4: do bazy nie idzie ani
//! jedna wartość, której nie ma w katalogu biegu, bo baza powstaje **z tego katalogu**. Wersja
//! zapisująca do bazy po drodze wygląda tak samo przez trzy tygodnie — do pierwszego skasowania
//! `loadout.db`.
//!
//! # Czego ta warstwa świadomie NIE robi
//!
//! - **Nie tee'uje surowego strumienia do `logs/agent-<id>.jsonl`.** `AgentDriver` oddaje już
//!   zdarzenie neutralne, a surowe bajty widzi wyłącznie `stream::pump` (T-05) — i to on ma
//!   `tee`, tylko nie ma dziś skąd wziąć ścieżki, bo `RunSpec` jej nie niesie. Katalog `logs/`
//!   powstaje mimo to, bo `store::rebuild` czyta go po nazwie; dopóki nikt tam nie pisze,
//!   transkrypt biegu żyje w liniach, a nie w plikach. Szew należy do T-07 (`ARCHITECTURE` §4).
//! - **Nie rozwija `copies`** [T3 §4.4]. Krok z `copies: 3` biegnie tu jako jedna sesja:
//!   rozwinięcie zmienia liczbę węzłów grafu, a `RunReport::steps` jest kontraktem „jeden wpis
//!   na krok pliku". To jest zadanie dla tego, kto zrobi też własne kopie plików.
//! - **Nie kopiuje plików projektu przy `fresh-copy`** — patrz [`workspace`].

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{Outcome, RunControl, RunDeps, RunError, RunReport, RunRequest};
use crate::engine::StepId;
use crate::engine::dag::Dag;
use crate::engine::drivers::{AgentDriver, AgentEvent, AgentHandle, Policy, RunSpec};
use crate::engine::line::{Curator, Line, Seen};
use crate::engine::scheduler;
use crate::engine::step::{StepReport, StepState};
use crate::engine::supervisor::GroupProof;
use crate::ipc::LineSink;
use crate::library::agents::{Agent, FileAccess, Overrides, read_agent_file, resolve};
use crate::workflow::check::{Level, check};
use crate::workflow::file::load;
use crate::workflow::{AgentStep, Folder, Step, WorkflowFile};

/// Biblioteka agentów pod katalogiem domowym Loadouta (`docs/ARCHITECTURE.md` §8).
const AGENTS_DIR: &str = "agents";

/// Katalog projektowy, w którym mieszkają biegi.
const PROJECT_DIR: &str = ".loadout";

/// Katalog biegów pod [`PROJECT_DIR`].
const RUNS_DIR: &str = "runs";

/// Opis biegu: bieg, jego kroki i migawki. To jest **prawda** (niezmiennik 4).
const RUN_FILE: &str = "run.json";

/// Nazwa, pod którą `run.json` powstaje przed przemianowaniem.
///
/// Zapis jest dwustopniowy, bo ten plik czyta ktoś inny **w trakcie** biegu: UI odpytuje o stan,
/// a punkt kontrolny ogłasza pauzę właśnie nim. `fs::write` prosto na `run.json` ma okno, w którym
/// plik jest przycięty do zera — czytelnik dostaje wtedy „to nie jest JSON" i nie ma jak odróżnić
/// tego od uszkodzenia.
const RUN_FILE_WRITING: &str = "run.json.writing";

/// Surowe strumienie agentów, po jednym pliku na krok (`logs/agent-<id>.jsonl`).
const LOGS_DIR: &str = "logs";

/// Katalog, pod którym powstają własne kopie plików dla kroków `fresh-copy`.
const WORK_DIR: &str = "work";

/// Ile zdarzeń sterownika mieści się w kanale, zanim ten zaczeka.
///
/// Kanał **ograniczony**, nigdy `unbounded_channel`: agent, który mówi szybciej, niż kurator
/// nadąża, ma zaczekać, a nie rosnąć w pamięci do końca biegu.
const EVENT_QUEUE: usize = 256;

/// Ile znaków przepisujemy z ostatniej wypowiedzi agenta do jednolinijkowego podsumowania kroku.
const SUMMARY_LIMIT: usize = 240;

/// Uruchamia workflow z pliku i oddaje jego linie pompie — **linia po linii**.
///
/// Kolejność: wczytaj → sprawdź → katalog biegu → migawka → planista → sterowniki → linie.
/// Odmowa przed pierwszym utworzonym katalogiem; szczegóły w nagłówku modułu.
///
/// `lines` jest [`LineSink`] z T-07, a nie `mpsc::Sender<Vec<Line>>`, i to jest cała zmiana
/// tego zadania. Sklejanie mieszka **po stronie pompy**, bo tam je zmierzono (16 ms / 2000
/// linii, [T8 §5.3]), a `LineSink::send` nigdy nie blokuje producenta: na pełnej kolejce linia
/// jest porzucana i **policzona**. Kanał, który każe czekać pętli czytającej stdout agenta,
/// kasuje dokładnie tę własność, dla której ta pompa powstała.
///
/// [`LineSink`] jedzie stąd w dół jedną drogą i nigdzie się nie rozgałęzia:
/// [`the_whole_run`] → [`Live::lines`] → [`forward`] → [`send_batch`], gdzie paczka kuratora
/// rozsypuje się na pojedyncze `sink.send(line)`. Sklejanie z powrotem robi pompa, po drugiej
/// stronie kolejki — i to jest jedyne miejsce, w którym wolno je zrobić, bo tam je zmierzono.
///
/// **`deps.control.settle()` musi zostać na KAŻDEJ drodze wyjścia**, także po odmowie: to na to
/// zdanie czeka [`stop_run_inner`], żeby móc wrócić z dowodem (niezmiennik 6). Settle wpisany
/// tylko na szczęśliwej ścieżce zawiesza Stop przy każdym biegu, który padł, i wygląda to jak
/// zawieszony agent, nie jak brakująca linijka. Dlatego cały bieg siedzi w [`the_whole_run`]:
/// stamtąd wychodzi się kilkoma `?`, a stąd — dokładnie jednym `return`.
pub async fn run_workflow_inner(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    lines: LineSink,
) -> Result<RunReport, RunError> {
    let report = the_whole_run(deps, request, lines).await;
    deps.control.settle();
    report
}

/// Bieg od wczytania pliku do zamknięcia księgi. Wydzielony z [`run_workflow_inner`], żeby
/// dowód z `settle()` schodził dokładnie raz, niezależnie od tego, którym `?` się stąd wyszło.
async fn the_whole_run(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    lines: LineSink,
) -> Result<RunReport, RunError> {
    let plan = plan_run(deps, request)?;
    // Graf budujemy po walidatorze, ale przed katalogiem: `Dag::new` odmawia cyklu przy
    // konstrukcji i jest ostatnią linią obrony, nie pierwszą (`engine::dag`).
    let dag = Dag::new(plan.steps.len(), &plan.arrows)?;

    lay_out_the_run_dir(&plan)?;
    let live = Arc::new(Live::new(plan, lines, deps.control.clone()));
    // Pierwszy zrzut idzie z `?`: bieg, którego nie da się zapisać na dysk, nie ma prawa ruszyć,
    // bo plikami stoi cała jego historia. Zrzuty w locie są już tylko logowane — patrz
    // [`Live::update`].
    live.open_the_book()?;

    let run_step = {
        let live = Arc::clone(&live);
        move |id: StepId, cancel: CancellationToken| {
            let live = Arc::clone(&live);
            async move { live.step(id, cancel).await }
        }
    };
    // Liczba „ile naraz" jedzie z ŻĄDANIA prosto do semafora planisty i nigdzie po drodze nie
    // ma stałej, na którą dałoby się ją podmienić (niezmiennik 11). To jest jedyny wiersz
    // w tym pliku, którego zniknięcie wygląda jak działający bieg.
    let outcome = scheduler::execute(
        &dag,
        request.how_many_at_once,
        deps.control.cancel_token(),
        run_step,
    )
    .await;

    live.close_the_book(&outcome.states, outcome.cancelled);
    // Indeks powstaje Z KATALOGU BIEGU, nigdy obok niego (niezmiennik 4): baza nie ma jak
    // powiedzieć niczego, czego nie ma w plikach, bo czyta dokładnie te pliki.
    deps.store.rebuild_from(&live.plan.dir).await?;

    Ok(RunReport {
        id: live.plan.id.clone(),
        dir: live.plan.dir.clone(),
        outcome: if outcome.cancelled {
            Outcome::Cancelled
        } else {
            Outcome::Done
        },
        steps: outcome.states,
    })
}

/// Zatrzymuje bieg i **wraca dopiero z dowodem**, że nic po nim nie żyje.
///
/// Zwraca [`Outcome::Cancelled`] jako wartość, nigdy `Err` (niezmiennik 7). `Ok(())` zaraz po
/// wysłaniu sygnału byłoby tym samym błędem, przed którym broni `GroupProof`: wołający
/// przeczytałby „nie żyje" tam, gdzie napisano „wysłałem SIGTERM" (niezmiennik 6).
///
/// **Warunek dla wołającego (T-07):** ten `RunControl` ma należeć do biegu, który ruszył albo
/// już zszedł. Dowód zapala [`run_workflow_inner`] na każdej swojej drodze wyjścia, więc bieg
/// zakończony i bieg odrzucony wracają stąd natychmiast — ale uchwyt biegu, którego nikt nigdy
/// nie uruchomił, nie ma czego dowieść i czekanie na niego nie ma końca.
pub async fn stop_run_inner(deps: &RunDeps<'_>) -> Result<Outcome, RunError> {
    deps.control.stop();
    // Czekamy na bieg, a nie na siebie. Kroki schodzą po swoich grupach procesów same — tylko
    // one wiedzą, co mają po sobie posprzątać — a `settle()` zapala się dopiero, kiedy
    // `run_workflow_inner` naprawdę wróciło.
    deps.control.wait_until_settled().await;
    // Bieg, którego token jest anulowany, melduje `cancelled` także wtedy, gdy ostatni krok
    // zdążył się udać (`scheduler::execute` czyta token na końcu). Dwa różne zdania o jednym
    // biegu byłyby dwoma miejscami, w których mieszka jedna odpowiedź.
    Ok(Outcome::Cancelled)
}

/// Puszcza bieg dalej z punktu kontrolnego (T3 §6.1 reguła 5).
///
/// Punkt kontrolny zatrzymuje **bieg**, nie krok, i nic za nim nie startuje, dopóki człowiek nie
/// odpowie. Pytanie, które pojawia się na ekranie po tym, jak agent już zrobił swoje, nie jest
/// pytaniem.
pub async fn continue_run_inner(deps: &RunDeps<'_>) -> Result<(), RunError> {
    // Licznik, nie flaga (`RunControl::go_on`): bieg z dwoma punktami kontrolnymi przeszedłby
    // przez drugi bez pytania, gdyby zgoda była flagą, która raz zapalona zostaje zapalona.
    deps.control.go_on();
    // Wracamy dopiero, kiedy bieg naprawdę ruszył — tak samo jak Stop wraca dopiero z dowodem.
    // Bez tego ekran wraca do człowieka w chwili, w której bieg **jeszcze stoi**, i pierwsze,
    // co ten człowiek widzi po odpowiedzeniu na pytanie, to dalej „paused". Czekanie kończy się
    // natychmiast, gdy nie było na co odpowiadać, i kończy się także wtedy, gdy bieg w tym
    // czasie zszedł (`RunControl::wait_until_moving`).
    deps.control.wait_until_moving().await;
    Ok(())
}

// ── PLAN: wszystko, co da się rozstrzygnąć, ZANIM cokolwiek powstanie ───────────────────────

/// Bieg rozpisany do końca i **jeszcze niczego niedotykający na dysku**.
///
/// Wszystko, co może odmówić — nieczytelny plik, koło w grafie, agent, którego nie ma
/// w bibliotece — odmawia przy budowie tej struktury. Dzięki temu „odmowa nie tworzy katalogu"
/// jest własnością kolejności wywołań, a nie obietnicą powtarzaną w komentarzach.
struct Plan {
    /// uuid v7 biegu — sortuje się po czasie.
    id: String,
    /// `<projekt>/.loadout/runs/<ts>__<id>/`. Policzony tutaj, tworzony dopiero po planie.
    dir: PathBuf,
    /// Tytuł widoczny w historii.
    title: String,
    /// Który workflow to był.
    workflow_id: String,
    /// Odcisk pliku — druga połowa pytania „czy to był ten sam plan".
    hash: String,
    /// Graf **jak biegł**, dosłownie taki, jaki wczytaliśmy.
    graph: Value,
    /// Krawędzie po numerach kroków, gotowe dla `engine::dag`.
    arrows: Vec<(StepId, StepId)>,
    /// Ile kroków ma naprawdę działać naraz — prosto z żądania.
    concurrency: usize,
    /// Kroki w kolejności z pliku workflow. Ta kolejność jest kontraktem `RunReport::steps`.
    steps: Vec<Planned>,
    /// Milisekundy epoki: kiedy ten bieg powstał.
    created_at: i64,
}

/// Jeden krok, rozpisany przed startem.
struct Planned {
    /// uuid v7 kroku — klucz wiersza w indeksie.
    id: String,
    /// Stabilny klucz węzła z grafu, czyli `id` kroku w pliku workflow.
    node_key: String,
    /// Nazwa z kafelka. To ona jedzie na ekran jako etykieta wiersza — identyfikator kroku
    /// ani uuid agenta nie mają tam czego szukać (niezmiennik 14).
    name: String,
    /// Klucze węzłów, po których ten krok idzie.
    depends_on: Vec<String>,
    /// Etykieta vendora, którym poszedł ten krok. Pusta dla kafelka kontrolnego: nie woła
    /// żadnego agenta, a wpisanie mu vendora byłoby wymyśleniem faktu, po którym wznowienie
    /// szukałoby kiedyś sesji, której nigdy nie było.
    vendor: String,
    /// Co ten krok robi.
    job: Job,
}

/// Dwa rodzaje kafelka i ani jednego więcej (D6, `ARCHITECTURE` §6b).
enum Job {
    /// Krok, który woła agenta.
    Agent(Box<AgentJob>),
    /// Kafelek kontrolny: bieg staje i pyta człowieka (T3 §6.1 reguła 5).
    Ask {
        /// Pytanie z kafelka, gotowe na ekran.
        question: Option<String>,
    },
}

/// Wszystko, czego krok agenta potrzebuje, żeby ruszyć — policzone przed startem biegu.
struct AgentJob {
    /// Sterownik vendora, wzięty z fabryki raz, przy planowaniu.
    driver: Arc<dyn AgentDriver>,
    /// Identyfikator sesji przydzielony **z góry**, przed startem procesu [T7 §6.2]. Dzięki
    /// temu wiadomo, pod jakim numerem zapisać krok, zanim vendor cokolwiek powie.
    session: Uuid,
    /// Katalog roboczy kroku.
    cwd: PathBuf,
    /// Czy ten katalog jest nasz, czyli czy mamy go utworzyć.
    ours: bool,
    /// Instrukcje kroku. Jadą do sterownika jako **dane** i wychodzą stdinem (niezmiennik 9).
    prompt: String,
    /// Model z konfiguracji efektywnej.
    model: Option<String>,
    /// Prompt systemowy agenta. To jest konfiguracja agenta, nie treść zadania.
    system_append: Option<String>,
    /// Co agentowi wolno zrobić z plikami — po ludzku, w trzech wariantach.
    policy: Policy,
    /// Migawka konfiguracji **efektywnej**, zamrożona w chwili startu [T4 §5.2 p. 3].
    effective: Value,
}

/// Wczytuje plik, sprawdza go drugi raz i rozpisuje bieg — **bez dotykania dysku**.
fn plan_run(deps: &RunDeps<'_>, request: &RunRequest) -> Result<Plan, RunError> {
    // Bajty czytamy osobno od `load()`, bo odcisk ma odpowiadać na pytanie „czy to ten sam
    // PLIK". Odcisk liczony z naszej serializacji odpowiadałby na pytanie „czy to ten sam plik
    // po przejściu przez nas", czyli milczałby o każdej zmianie, której nie rozumiemy.
    let bytes = fs::read(&request.workflow)?;
    let file = load(&request.workflow)?;

    // Bieg nie ufa UI (T3 §5.2): plik mógł zostać zmergowany gitem albo poprawiony ręcznie
    // między zapisem a naciśnięciem Start. Odmawiamy zdaniem WALIDATORA, słowo w słowo —
    // własne tłumaczenie byłoby drugim miejscem, w którym mieszka ten sam komunikat.
    if let Some(refusal) = check(&file)
        .into_iter()
        .find(|note| note.level == Level::Problem)
    {
        return Err(RunError::Refused(refusal));
    }

    let id = Uuid::now_v7().to_string();
    let created_at = now_ms();
    let dir = deps
        .project
        .join(PROJECT_DIR)
        .join(RUNS_DIR)
        .join(format!("{}__{id}", stamp(created_at)));

    let setup = Setup {
        library: deps.home.join(AGENTS_DIR),
        project: deps.project,
        dir: &dir,
        drivers: &deps.drivers,
    };
    let mut steps = Vec::with_capacity(file.steps.len());
    for step in &file.steps {
        steps.push(plan_step(step, &setup)?);
    }
    let arrows = arrows(&file);
    // Klucze najpierw, dopiero potem dopisywanie: `steps[child]` i `steps[parent]` naraz to
    // dwie pożyczki jednego wektora, a nie dwie różne rzeczy.
    let keys: Vec<String> = steps.iter().map(|step| step.node_key.clone()).collect();
    for &(parent, child) in &arrows {
        steps[child].depends_on.push(keys[parent].clone());
    }

    Ok(Plan {
        id,
        dir,
        title: file.name.clone(),
        workflow_id: file.id.clone(),
        hash: fingerprint(&bytes),
        graph: serde_json::to_value(&file)?,
        arrows,
        concurrency: request.how_many_at_once,
        steps,
        created_at,
    })
}

/// Krawędzie pliku przełożone na numery kroków.
///
/// Strzałkę, której koniec nie istnieje, pomijamy — i wolno to zrobić dokładnie dlatego, że
/// `check()` odmówiłby takiego pliku kilka linii wyżej (`arrows_into_nowhere`). Numer kroku to
/// jego pozycja w pliku, a przy powtórzonym id wygrywa pierwszy: ta sama reguła, co
/// w `workflow::check`, żeby strzałka nie celowała raz w jeden krok, raz w drugi.
fn arrows(file: &WorkflowFile) -> Vec<(StepId, StepId)> {
    let mut position: std::collections::BTreeMap<&str, StepId> = std::collections::BTreeMap::new();
    for (index, step) in file.steps.iter().enumerate() {
        position.entry(key_of(step)).or_insert(index);
    }
    file.links
        .iter()
        .filter_map(|link| {
            Some((
                *position.get(link.from.as_str())?,
                *position.get(link.to.as_str())?,
            ))
        })
        .collect()
}

/// Stabilny klucz węzła, niezależny od rodzaju kafelka.
fn key_of(step: &Step) -> &str {
    match step {
        Step::Agent(agent) => &agent.id,
        Step::Checkpoint(ask) => &ask.id,
    }
}

/// Wobec czego planujemy krok: gdzie leży biblioteka, gdzie projekt, gdzie katalog tego biegu
/// i skąd biorą się sterowniki.
struct Setup<'a> {
    /// `~/.loadout/agents` — stąd bierzemy agenta, którego nazywa krok.
    library: PathBuf,
    /// Katalog projektu, w którym biegnie workflow.
    project: &'a Path,
    /// Katalog tego biegu. Jeszcze nie istnieje: pod nim lądują własne kopie plików.
    dir: &'a Path,
    /// Fabryka sterowników z [`RunDeps`].
    drivers: &'a super::Drivers,
}

/// Jeden krok pliku → jeden krok planu.
fn plan_step(step: &Step, setup: &Setup<'_>) -> Result<Planned, RunError> {
    match step {
        Step::Checkpoint(ask) => Ok(Planned {
            id: Uuid::now_v7().to_string(),
            node_key: ask.id.clone(),
            name: ask.name.clone(),
            depends_on: Vec::new(),
            vendor: String::new(),
            job: Job::Ask {
                question: ask.question.clone(),
            },
        }),
        Step::Agent(agent) => {
            let job = plan_agent(agent, setup)?;
            Ok(Planned {
                id: Uuid::now_v7().to_string(),
                node_key: agent.id.clone(),
                name: agent.name.clone(),
                depends_on: Vec::new(),
                vendor: job.driver.id().to_owned(),
                job: Job::Agent(Box::new(job)),
            })
        }
    }
}

/// Krok agenta: konfiguracja efektywna, sterownik, katalog roboczy.
fn plan_agent(step: &AgentStep, setup: &Setup<'_>) -> Result<AgentJob, RunError> {
    let saved = find_agent(&setup.library, &step.agent)?;
    // Nadpisania kroku przechodzą przez `Overrides`, więc klucz, którego krok nie ma prawa
    // ruszyć (`id`, `name`, `runsWith`), odbija się o typ, a nie o walidator do zapamiętania.
    let overrides: Overrides = serde_json::from_value(Value::Object(step.overrides.clone()))?;
    let effective = resolve(&saved, &overrides)?.agent;

    let (cwd, ours) = workspace(&step.folder, setup.project, setup.dir, &step.id);
    Ok(AgentJob {
        // Fabryka wołana **raz, przy planowaniu**, a nie w kroku: etykieta vendora stoi
        // w `run.json` od pierwszego zrzutu, więc historia biegu wie, do kogo wracać, także
        // wtedy, gdy krok nigdy nie ruszył.
        driver: (setup.drivers)(effective.runs_with),
        session: Uuid::now_v7(),
        cwd,
        ours,
        // Treść zadania. `{{copy}}` i `{{copies}}` podstawia dopiero rozwinięcie kroku na kopie
        // [T3 §4.3, §4.4] — tego rozwinięcia w tym zadaniu nie ma i `copies > 1` biegnie tu
        // jako jedna sesja. Podstawienie bez rozwinięcia wpisywałoby w prompt liczbę, której
        // nic po drugiej stronie nie odpowiada.
        prompt: step.instructions.clone(),
        model: some_text(&effective.model),
        // Prompt systemowy agenta, nie treść zadania: treść zadania w tym polu byłaby
        // niezmiennikiem 9 złamanym po cichu, bo stąd wchodzi do argv.
        system_append: some_text(&effective.instructions),
        policy: policy_of(effective.file_access),
        effective: serde_json::to_value(&effective)?,
    })
}

/// Znajduje w bibliotece agenta o tym identyfikatorze.
///
/// Szukamy po `id`, nie po nazwie pliku: krok workflow nazywa agenta identyfikatorem, bo ten
/// przeżywa zmianę nazwy (T3 §3.1). Plik, którego nie da się przeczytać, **nie zabiera biegu**,
/// który go nie używa — ale jeśli szukanego nie ma, to właśnie jego błąd jest odpowiedzią,
/// bo „nie ma takiego agenta" i „ten plik jest zepsuty" naprawia się inaczej [T4 §10].
fn find_agent(library: &Path, id: &str) -> Result<Agent, RunError> {
    let mut files: Vec<PathBuf> = fs::read_dir(library)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    // `read_dir` nie obiecuje żadnej kolejności, a odpowiedź „którego agenta wzięliśmy" nie ma
    // prawa zależeć od systemu plików.
    files.sort();

    let mut broken = None;
    for path in files {
        match read_agent_file(&path) {
            Ok(agent) if agent.id.to_string() == id => return Ok(agent),
            Ok(_) => {}
            Err(error) => broken = broken.or(Some(error)),
        }
    }
    Err(RunError::Agent(broken.unwrap_or_else(|| {
        crate::library::agents::AgentError::Unreadable {
            file: library.display().to_string(),
            detail: format!("no agent saved here has the id {id}"),
        }
    })))
}

/// Gdzie krok pracuje i czy ten katalog jest nasz.
///
/// 2026-08-16 — `fresh-copy` dostaje **własny, pusty** katalog pod katalogiem biegu, a nie
/// katalog projektu, i to jest świadomy wybór między dwoma niepełnymi odpowiedziami. Kopiowania
/// plików projektu nie ma jeszcze w żadnym zadaniu (`ARCHITECTURE` §2 p. 4 obiecuje je jako
/// zdolność produktu), więc krok dostanie tu pustkę i powie o tym głośno przy pierwszym `ls`.
/// Wersja tańsza o jedną linijkę — podstawienie katalogu projektu — jest gorsza i cicha: cztery
/// kroki bez strzałek zaczęłyby pisać po tych samych plikach, czyli robić dokładnie to, czego
/// walidator odmawia przy zapisie (niezmiennik 12).
fn workspace(folder: &Folder, project: &Path, dir: &Path, node_key: &str) -> (PathBuf, bool) {
    match folder {
        Folder::Project => (project.to_path_buf(), false),
        // Katalog wskazany ręcznie jest cudzy: nie tworzymy go, bo „nie ma takiego folderu" jest
        // odpowiedzią, a utworzenie go po cichu zamienia literówkę w pusty bieg.
        Folder::Pick { path } => (PathBuf::from(path), false),
        Folder::FreshCopy => (dir.join(WORK_DIR).join(node_key), true),
    }
}

/// Dial „co agent może zrobić z plikami" → polityka, którą rozumie sterownik.
///
/// Trzy pozycje na trzy warianty, po kolei. Środkowa jest przybliżeniem i tak jest opisana
/// w macierzy T4 §6.3 (`fileAccess` jest `Approximate` u obu vendorów): `Policy` nie ma dziś
/// wariantu „pytaj", więc `ask-first` ląduje na „edytuje w swoim folderze". Sklejenie dwóch
/// pozycji dialu w jedną politykę byłoby gorsze — dial miałby wtedy pozycję, która nic nie
/// robi, czyli kontrolkę bez handlera (niezmiennik 16).
fn policy_of(access: FileAccess) -> Policy {
    match access {
        FileAccess::LookOnly => Policy::ReadOnly,
        FileAccess::AskFirst => Policy::EditInFolder,
        FileAccess::WorkFreely => Policy::Unrestricted,
    }
}

/// Napis albo nic. Puste pole w definicji agenta znaczy „nie mam zdania", a nie „ustaw pustkę".
fn some_text(text: &str) -> Option<String> {
    (!text.trim().is_empty()).then(|| text.to_owned())
}

/// Tworzy katalog biegu i to, co do niego należy — **dopiero po planie**.
fn lay_out_the_run_dir(plan: &Plan) -> Result<(), RunError> {
    // `logs/` powstaje razem z katalogiem, a nie przy pierwszej linii: katalog biegu bez niego
    // czyta się jak bieg, w którym agent nic nie powiedział, zamiast jak bieg, który jeszcze nic
    // nie zapisał.
    fs::create_dir_all(plan.dir.join(LOGS_DIR))?;
    for step in &plan.steps {
        if let Job::Agent(job) = &step.job
            && job.ours
        {
            fs::create_dir_all(&job.cwd)?;
        }
    }
    Ok(())
}

// ── ŻYWY BIEG ──────────────────────────────────────────────────────────────────────────────

/// Bieg w trakcie: plan (niezmienny) plus księga (zmienna), plus to, czym mówi do świata.
struct Live {
    /// Wszystko, co rozstrzygnięto przed startem.
    plan: Plan,
    /// Stan, który zmienia się w trakcie. Zamek jest `std::sync::Mutex`, a każde jego wzięcie
    /// mieści się w jednym wywołaniu bez `await` (niezmiennik 8, `clippy::await_holding_lock`
    /// = deny).
    book: Mutex<Book>,
    /// Linie na ekran, **po jednej**. Sklejaniem zajmuje się pompa z T-07 i tylko ona: bieg,
    /// który skleja u siebie, ustala okno, którego nikt nie zmierzył, i odbiera pompie jedyną
    /// rzecz, dla której ta pompa powstała.
    lines: LineSink,
    /// Stop i Continue sięgają tędy do środka biegu.
    control: RunControl,
    /// Chwila startu biegu. Kurator dostaje czas **argumentem**, bo kurator z własnym zegarem
    /// nie da się przetestować bez `sleep`.
    began: Instant,
}

/// Zmienna połowa biegu — dokładnie to, co zmienia się między zrzutami `run.json`.
struct Book {
    /// Stan **biegu**. Jedyne miejsce, w którym istnieje `paused`.
    status: RunState,
    /// Kiedy ruszył pierwszy krok.
    started_at: Option<i64>,
    /// Kiedy skończył się ostatni.
    ended_at: Option<i64>,
    /// Po jednym wpisie na krok, w kolejności z pliku workflow.
    steps: Vec<StepRun>,
}

/// Co bieg wie o jednym kroku.
#[derive(Debug, Clone)]
struct StepRun {
    /// Stan kroku. `paused` tu nie istnieje i nie ma go w [`StepState`] — to jest stan biegu.
    status: StepState,
    /// Kiedy krok ruszył.
    started_at: Option<i64>,
    /// Kiedy się skończył.
    ended_at: Option<i64>,
    /// Proces potomny, jeśli sterownik go miał.
    pid: Option<i32>,
    /// Grupa procesów — to po niej sprząta odzyskiwanie po awarii (T-20).
    pgid: Option<i32>,
    /// Kod wyjścia.
    exit_code: Option<i32>,
    /// Ile kosztował.
    cost_usd: Option<f64>,
    /// Jedna linia dla szyny agentów.
    summary: Option<String>,
    /// Powód, jeśli coś poszło nie tak.
    error: Option<String>,
}

/// Stan **biegu**: pięć wartości z `CHECK` przy tabeli `runs` w `store::schema`.
///
/// Szóstej — `interrupted` — stąd nie da się napisać i tak ma być: wpisuje ją odzyskiwanie po
/// awarii aplikacji (T-20), przy starcie, biegom, które nie miały jak dokończyć. Bieg, który
/// sam siebie melduje jako przerwany, to bieg, który jeszcze żyje.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum RunState {
    /// Bieg idzie.
    Running,
    /// Bieg stoi na punkcie kontrolnym i czeka na człowieka.
    Paused,
    /// Koniec, wszystko się udało.
    Succeeded,
    /// Koniec, coś padło.
    Failed,
    /// Koniec, bo zatrzymał go człowiek.
    Cancelled,
}

impl Live {
    /// Świeży bieg: wszystkie kroki czekają, nic jeszcze nie ruszyło.
    fn new(plan: Plan, lines: LineSink, control: RunControl) -> Self {
        let steps = plan
            .steps
            .iter()
            .map(|_| StepRun {
                status: StepState::Pending,
                started_at: None,
                ended_at: None,
                pid: None,
                pgid: None,
                exit_code: None,
                cost_usd: None,
                summary: None,
                error: None,
            })
            .collect();
        Self {
            plan,
            book: Mutex::new(Book {
                status: RunState::Running,
                started_at: None,
                ended_at: None,
                steps,
            }),
            lines,
            control,
            began: Instant::now(),
        }
    }

    /// Zamek na księdze. Zatruty odplatamy zamiast panikować: `panic!` w silniku zabiera cały
    /// bieg (AGENTS.md §4), a księga po panice jednego kroku jest dalej poprawna.
    fn book(&self) -> MutexGuard<'_, Book> {
        self.book.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Pierwszy zrzut `run.json`. Jego błąd zatrzymuje bieg, bo bieg bez pliku nie ma historii.
    fn open_the_book(&self) -> Result<(), RunError> {
        let book = self.book();
        self.spill(&book)
    }

    /// Zmienia księgę i **od razu** zrzuca ją na dysk — wszystko pod jednym zamkiem.
    ///
    /// Zapis siedzi pod zamkiem nie z ostrożności, tylko dlatego, że inaczej plik potrafi cofnąć
    /// się w czasie: dwa kroki kończące się w tej samej chwili budują JSON z dwóch różnych
    /// stanów, a wygrywa ten, który zdąży później do `rename`. Ogłoszona pauza nadpisana zrzutem
    /// sprzed pauzy jest awarią, której nikt nie zobaczy — bieg po prostu stoi, a plik mówi, że
    /// idzie.
    ///
    /// Błąd zrzutu w locie **loguje się i nie zatrzymuje biegu**: cztery żywe agenty to zły
    /// moment na przewracanie wszystkiego z powodu jednego nieudanego zapisu. Pierwszy zrzut
    /// jest inny i idzie przez [`Live::open_the_book`].
    fn update(&self, edit: impl FnOnce(&mut Book)) {
        let mut book = self.book();
        edit(&mut book);
        if let Err(error) = self.spill(&book) {
            tracing::error!(run = %self.plan.id, %error, "the run file could not be written");
        }
    }

    /// Księga → `run.json`, przez plik tymczasowy i `rename`.
    fn spill(&self, book: &Book) -> Result<(), RunError> {
        let text = serde_json::to_string_pretty(&self.run_file(book))?;
        let writing = self.plan.dir.join(RUN_FILE_WRITING);
        fs::write(&writing, text)?;
        // `rename` w obrębie jednego katalogu jest atomowe: czytelnik widzi albo poprzedni plik
        // w całości, albo nowy w całości, i nigdy zera bajtów w środku.
        fs::rename(&writing, self.plan.dir.join(RUN_FILE))?;
        Ok(())
    }

    /// Widok `run.json` na tę chwilę.
    fn run_file<'a>(&'a self, book: &'a Book) -> RunFile<'a> {
        let steps = self
            .plan
            .steps
            .iter()
            .zip(&book.steps)
            .map(|(planned, run)| StepEntry {
                id: &planned.id,
                node_key: &planned.node_key,
                name: &planned.name,
                agent: &planned.vendor,
                depends_on: &planned.depends_on,
                status: run.status,
                // Ponowienie kroku („uruchom jeszcze raz od tego miejsca") jest w v1.1
                // [PLAN §7], więc każdy krok ma tu dziś dokładnie jedno podejście.
                attempt: 0,
                agent_session_id: match &planned.job {
                    Job::Agent(job) => Some(job.session.to_string()),
                    Job::Ask { .. } => None,
                },
                pid: run.pid,
                pgid: run.pgid,
                exit_code: run.exit_code,
                started_at: run.started_at,
                ended_at: run.ended_at,
                cost_usd: run.cost_usd,
                summary: run.summary.as_deref(),
                error: run.error.as_deref(),
                effective: match &planned.job {
                    Job::Agent(job) => Some(&job.effective),
                    Job::Ask { .. } => None,
                },
            })
            .collect();

        RunFile {
            id: &self.plan.id,
            workflow_id: &self.plan.workflow_id,
            workflow_hash: &self.plan.hash,
            workflow_snapshot: &self.plan.graph,
            title: &self.plan.title,
            status: book.status,
            concurrency: self.plan.concurrency,
            created_at: self.plan.created_at,
            started_at: book.started_at,
            ended_at: book.ended_at,
            error: None,
            steps,
        }
    }

    /// Jeden krok, od pierwszego wpisu w księdze po ostatni.
    async fn step(&self, id: StepId, cancel: CancellationToken) -> StepReport {
        let at = now_ms();
        self.update(|book| {
            book.started_at.get_or_insert(at);
            let step = &mut book.steps[id];
            step.status = StepState::Running;
            step.started_at = Some(at);
        });

        let report = match &self.plan.steps[id].job {
            Job::Agent(job) => self.run_agent(id, job, &cancel).await,
            Job::Ask { question } => self.wait_for_a_person(id, question.as_deref()).await,
        };

        self.update(|book| {
            let step = &mut book.steps[id];
            step.status = match report {
                StepReport::Succeeded => StepState::Succeeded,
                StepReport::Failed => StepState::Failed,
                StepReport::Cancelled => StepState::Cancelled,
            };
            step.ended_at = Some(now_ms());
        });
        report
    }

    /// Krok agenta: sterownik, zdarzenia, linie, koniec albo anulowanie.
    async fn run_agent(
        &self,
        id: StepId,
        job: &AgentJob,
        cancel: &CancellationToken,
    ) -> StepReport {
        let (events, inbox) = mpsc::channel::<AgentEvent>(EVENT_QUEUE);
        // Odbiór staje PRZED startem sterownika: vendor ma prawo powiedzieć pierwsze zdarzenia
        // jeszcze w `start`, a kanał bez odbiorcy zatrzymałby go na pierwszym pełnym buforze.
        let pump = tokio::spawn(forward(
            inbox,
            self.lines.clone(),
            self.plan.steps[id].name.clone(),
            self.began,
        ));
        // Własny klon nadawcy zostaje po to, żeby o nieudanym starcie dało się powiedzieć tą samą
        // drogą, którą mówi agent. Musi zginąć na OBU gałęziach — nadawca, który przeżył krok,
        // trzyma kurator otwarty i `pump.await` niżej nie wróciłby nigdy.
        let ours = events.clone();

        let spec = RunSpec {
            run_id: job.session,
            cwd: job.cwd.clone(),
            // Instrukcje jadą jako DANE. Ta warstwa nie skleja komendy i nie zna ani jednej
            // flagi vendora (niezmiennik 9).
            prompt: job.prompt.clone(),
            model: job.model.clone(),
            system_append: job.system_append.clone(),
            policy: job.policy,
            extra_dirs: Vec::new(),
            resume: None,
        };

        // Start **nie** ściga się z anulowaniem i to jest wybór, nie przeoczenie: żeby zejść po
        // grupie procesów, trzeba mieć uchwyt, a uchwyt wydaje dopiero `start`. Zdjęcie tego
        // `await` w połowie zostawiłoby proces, który właśnie wstał, bez nikogo, kto by o nim
        // wiedział — czyli dokładnie ten osierocony `claude` palący limit w tle, przed którym
        // stoją niezmienniki 6 i 10. Token widzi więc dopiero tura, i widzi go od środka.
        let report = match job.driver.start(spec, events).await {
            Ok(handle) => {
                drop(ours);
                self.one_turn(id, handle, cancel).await
            }
            Err(error) => {
                let text = format!("Loadout could not start this agent: {error}");
                let _ = ours.send(AgentEvent::Notice { text: text.clone() }).await;
                drop(ours);
                self.update(|book| book.steps[id].error = Some(text));
                StepReport::Failed
            }
        };

        // Czekamy na kurator, zanim krok wróci: linie kroku muszą wyjść, ZANIM planista wypuści
        // następny. Bez tego strzałka „po" przestaje znaczyć „po" na ekranie, choć w silniku
        // dalej znaczy.
        let _ = pump.await;
        report
    }

    /// Jedna tura agenta: czekaj na koniec albo na Stop.
    async fn one_turn(
        &self,
        id: StepId,
        mut handle: Box<dyn AgentHandle>,
        cancel: &CancellationToken,
    ) -> StepReport {
        // `pid` i `pgid` zapisujemy, ZANIM cokolwiek popłynie ze stdout [T7 §6.2]: po awarii
        // aplikacji nie ma już kogo o nie zapytać, a to po nich sprząta odzyskiwanie (T-20).
        if let Some(group) = handle.group() {
            self.update(|book| {
                let step = &mut book.steps[id];
                step.pid = Some(group.pid);
                step.pgid = Some(group.pgid);
            });
        }

        let finished = {
            let waiting = handle.wait();
            tokio::pin!(waiting);
            tokio::select! {
                // `biased`, bo tura, która właśnie się skończyła, ma pierwszeństwo przed Stopem
                // wpadającym w tej samej chwili: zabijanie czegoś, co już zeszło, zamieniłoby
                // udany krok w anulowany zależnie od tego, który poll wypadł pierwszy.
                biased;
                done = &mut waiting => Some(done),
                () = cancel.cancelled() => None,
            }
            // Pożyczka `handle` kończy się razem z tym blokiem — dopiero po nim wolno zawołać
            // `cancel()` albo `close()` na tym samym uchwycie.
        };

        match finished {
            // ANULOWANIE IDZIE PRZEZ STEROWNIK, nie przez zdjęcie zadania Rusta. `tokio::time::
            // timeout` wokół kroku wygląda tak samo i jest o linijkę tańszy — i zostawia żywą
            // grupę procesów palącą limit u dostawcy (niezmienniki 6 i 10).
            None => {
                let proof = handle.cancel().await;
                if let GroupProof::Alive = proof {
                    // Dopóki nie ma dowodu, traktujemy jako żywe (niezmiennik 6). To jest zdanie
                    // dla człowieka, bo osierocony agent pali pieniądze w tle.
                    self.update(|book| {
                        book.steps[id].error = Some(
                            "Loadout could not make sure this agent stopped, so it may still be \
                             running."
                                .to_owned(),
                        );
                    });
                }
                StepReport::Cancelled
            }
            Some(Err(error)) => {
                self.update(|book| book.steps[id].error = Some(error.to_string()));
                StepReport::Failed
            }
            Some(Ok(turn)) => {
                // Normalne zakończenie idzie przez `close`: `claude` z otwartym stdinem czeka
                // w nieskończoność, więc krok bez tego zostawia żywy proces [T1 §2, §4.6].
                let code = handle.close().await.ok().flatten();
                // Sukces to zero **i** `is_error == false` (niezmiennik 19, ARCHITECTURE §5).
                // Samo zero z drivera nie kończy kroku sukcesem — agent, który wypisał „nie dam
                // rady" i wyszedł czysto, nie zrobił tego, o co go proszono.
                let ok = turn.ok && matches!(code, None | Some(0));
                self.update(|book| {
                    let step = &mut book.steps[id];
                    step.exit_code = code;
                    step.cost_usd = turn.cost_usd;
                    step.summary = summary_of(&turn.text);
                });
                if ok {
                    StepReport::Succeeded
                } else {
                    StepReport::Failed
                }
            }
        }
    }

    /// Kafelek kontrolny: bieg staje i pyta człowieka (T3 §6.1 reguła 5).
    ///
    /// Stoi **bieg**, nie krok: `paused` jest stanem biegu i nie ma go w maszynie stanów kroku
    /// (`docs/ARCHITECTURE.md` §5). Nic za pytaniem nie startuje, bo dopóki ten krok nie wróci
    /// z `Succeeded`, planista nie zdejmuje stopnia wejściowego jego potomkom — a pytanie, które
    /// pojawia się po tym, jak agent już zrobił swoje, nie jest pytaniem.
    async fn wait_for_a_person(&self, id: StepId, question: Option<&str>) -> StepReport {
        // Nasłuch PRZED ogłoszeniem pauzy. Powód stoi przy `RunControl::listen_for_go_on`:
        // odpowiedź przychodzi w reakcji na to, co widać na dysku, więc kolejność odwrotna ma
        // okno, w którym Continue trafia do nikogo i bieg stoi już na zawsze.
        let listening = self.control.listen_for_go_on();
        // Fakt „bieg stoi" ma jednego właściciela — [`RunControl`] — a wpis w `run.json` jest
        // jego trwałym lustrem: stan, który nie dociera na dysk, nie przeżywa awarii aplikacji
        // (niezmiennik 4), a stan, który istnieje wyłącznie na dysku, nie da się o nic zapytać
        // z drugiej strony okna.
        self.control.pause();
        self.update(|book| book.status = RunState::Paused);
        self.ask(id, question);

        if listening.wait().await {
            self.control.resume();
            self.update(|book| book.status = RunState::Running);
            StepReport::Succeeded
        } else {
            self.control.resume();
            // Stop przy pytaniu. Krok jest `cancelled`, a jego potomkowie też — nie `skipped`,
            // bo nikt nie padł: człowiek powiedział stop (ARCHITECTURE §5).
            StepReport::Cancelled
        }
    }

    /// Pytanie na ekran.
    ///
    /// Wiersz powstaje tutaj, a nie w kuratorze, bo punkt kontrolny nie jest zdarzeniem agenta —
    /// jest kafelkiem w pliku workflow. To ta sama droga, którą `Line::Run` i `Line::Step`
    /// dokłada planista (`engine::line`, nagłówek [`Line`]). Bez tego wiersza pole `question`
    /// nie miałoby ani jednego czytelnika, a pytanie, którego nie widać, zatrzymuje bieg bez
    /// powodu widocznego dla człowieka.
    ///
    /// 2026-08-17 — synchroniczna, odkąd wiersz jedzie do pompy przez `try_send`. `async fn`
    /// bez ani jednego `await` w środku jest czerwony u `clippy::unused_async`, a udawane
    /// czekanie przed pytaniem byłoby jedynym miejscem w tym pliku, w którym punkt kontrolny
    /// zależy od tego, czy okno nadąża.
    fn ask(&self, id: StepId, question: Option<&str>) {
        let step = &self.plan.steps[id];
        let line = Line::Asked {
            agent: step.name.clone(),
            // Kafelek bez wpisanego pytania mówi swoją nazwą — ona też jest zdaniem, które
            // napisał człowiek.
            text: question.unwrap_or(&step.name).to_owned(),
            // Warianty odpowiedzi są polem kroku dopiero w T3 §7.1; pusta lista znaczy
            // „odpowiedz własnymi słowami", nie „pytanie bez treści".
            options: Vec::new(),
        };
        send_batch(&self.lines, vec![line]);
    }

    /// Zamyka księgę stanami **od planisty**.
    ///
    /// Stany bierzemy stamtąd, a nie z tego, co zapisały same kroki, bo tylko planista wie
    /// o stożku: krok, który nigdy nie ruszył, bo ktoś wyżej padł albo bo bieg zatrzymano, ma
    /// tu swój powód (`skipped` kontra `cancelled`) i to jest różnica, o którą UI pyta pierwsze.
    fn close_the_book(&self, states: &[StepState], cancelled: bool) {
        let at = now_ms();
        self.update(|book| {
            for (row, &state) in book.steps.iter_mut().zip(states) {
                row.status = state;
            }
            book.status = if cancelled {
                RunState::Cancelled
            } else if states.contains(&StepState::Failed) {
                RunState::Failed
            } else {
                RunState::Succeeded
            };
            book.ended_at = Some(at);
        });
    }
}

/// Zdarzenia jednego kroku → wiersze na ekran.
///
/// Kuracja mieszka w [`Curator`] i **tylko** tam (niezmiennik 15): ta pętla nie decyduje, który
/// wiersz istnieje ani co mówi, tylko podaje zdarzenia po kolei i wypuszcza to, co się domknęło.
///
/// 2026-08-16 — `tool: None` jest tu granicą, nie niedopatrzeniem. Fakty o narzędziu
/// (`engine::line::Tool`) wyjmuje z linii drutu `stream::decode`, a `AgentDriver` oddaje już
/// samo zdarzenie neutralne, więc na tej drodze wiersze `read`/`edit`/`ran` nie mają z czego
/// powstać. Szew, w którym te dwie drogi mają się spotkać, należy do T-07 (`ARCHITECTURE` §4:
/// `stream.rs` stoi między nadzorem a kuratorem); dopisanie tu drugiej klasyfikacji byłoby
/// drugą implementacją kuracji, czyli tą, o której nikt by nie pamiętał.
async fn forward(
    mut inbox: mpsc::Receiver<AgentEvent>,
    lines: LineSink,
    agent: String,
    began: Instant,
) {
    let mut curator = Curator::new();
    while let Some(event) = inbox.recv().await {
        let at_ms = u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
        let seen = Seen {
            agent: &agent,
            at_ms,
            event: &event,
            tool: None,
        };
        send_batch(&lines, curator.observe(seen));
    }
    // Ostatnia grupa sklejania wyszłaby inaczej nigdy, a użytkownik zobaczyłby o wiersz mniej,
    // niż się wydarzyło — najgorszy rodzaj zgubienia, bo cichy.
    send_batch(&lines, curator.flush());
}

/// Wiersze kuratora oddane pompie, **po jednym**.
///
/// 2026-08-17 — funkcja jest synchroniczna i to jest cała treść tego szwu. `LineSink::send`
/// robi `try_send`: albo ma miejsce, albo nie ma, i nigdy nie każe czekać. Wersja z `await`
/// zatrzymywałaby na pełnej kolejce pętlę czytającą stdout agenta — czyli spowalniała agenta
/// dlatego, że okno nie nadąża, co jest dokładnie tą własnością, którą pompa miała skasować
/// (`ipc::LineSink`, [T7 §4.1]).
///
/// Odpowiedzi `Sent` nie liczymy tutaj i to też jest wybór: bilans przyjętych i porzuconych
/// wraca JEDNĄ drogą, z `PumpStats` po drugiej stronie [`crate::ipc::spawn_pump`]
/// (niezmiennik 13). Drugi licznik w biegu byłby drugą liczbą o tym samym zdarzeniu — a przy
/// dwóch zawsze czyta się tę, która akurat kłamie.
fn send_batch(lines: &LineSink, batch: Vec<Line>) {
    for line in batch {
        let _ = lines.send(line);
    }
}

/// Jedna linia podsumowania kroku dla szyny agentów. `None`, kiedy agent nic nie powiedział.
fn summary_of(text: &str) -> Option<String> {
    let line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.is_empty() {
        return None;
    }
    let mut end = line.len().min(SUMMARY_LIMIT);
    while end > 0 && !line.is_char_boundary(end) {
        end -= 1;
    }
    Some(line[..end].to_owned())
}

// ── KSZTAŁT `run.json` ─────────────────────────────────────────────────────────────────────

/// `run.json`, tak jak ląduje na dysku.
///
/// Nazwy pól są dokładnie tymi, które czyta `store::rebuild` — rozjazd znaczy, że po skasowaniu
/// bazy dostaje się co innego, niż się miało (niezmiennik 4). Dlatego są w `snake_case`,
/// a `effective` w środku kroku zostaje w `camelCase`: to jest migawka cudzego kształtu
/// (`library::agents::Agent`), nie nasz schemat.
#[derive(Debug, Serialize)]
struct RunFile<'a> {
    id: &'a str,
    workflow_id: &'a str,
    /// Odcisk pliku workflow — „czy to był ten sam plan?".
    workflow_hash: &'a str,
    /// Graf **jak biegł**. Bez niego poprawiony workflow po cichu zmienia opowieść starych
    /// biegów stojących w historii [T7 §5.4].
    workflow_snapshot: &'a Value,
    title: &'a str,
    status: RunState,
    concurrency: usize,
    created_at: i64,
    started_at: Option<i64>,
    ended_at: Option<i64>,
    error: Option<&'a str>,
    steps: Vec<StepEntry<'a>>,
}

/// Krok w `run.json`.
#[derive(Debug, Serialize)]
struct StepEntry<'a> {
    id: &'a str,
    node_key: &'a str,
    name: &'a str,
    agent: &'a str,
    depends_on: &'a [String],
    status: StepState,
    attempt: u32,
    agent_session_id: Option<String>,
    pid: Option<i32>,
    pgid: Option<i32>,
    exit_code: Option<i32>,
    started_at: Option<i64>,
    ended_at: Option<i64>,
    cost_usd: Option<f64>,
    summary: Option<&'a str>,
    error: Option<&'a str>,
    /// Konfiguracja **efektywna**, zamrożona w chwili startu [T4 §5.2 p. 3]. `None` dla kafelka
    /// kontrolnego: on nie woła agenta, więc nie ma czego zamrażać.
    effective: Option<&'a Value>,
}

// ── DROBIAZGI ──────────────────────────────────────────────────────────────────────────────

/// Milisekundy epoki. Zegar przestawiony wstecz daje zero zamiast liczby ujemnej: kolumna
/// `created_at` sortuje historię i data sprzed epoki wywróciłaby tę kolejność.
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

/// `<ts>` z nazwy katalogu biegu: `20260816-194804`, czas UTC.
///
/// Bez dwukropków i bez podkreśleń — nazwę katalogu rozcina się na pierwszym `__`, a dwukropek
/// nie jest znakiem, który przeżyje port na Windows. Sortuje się leksykograficznie, więc
/// `ls` w katalogu biegów daje historię w kolejności.
///
/// Algorytm dni→data jest standardowy (proleptyczny kalendarz gregoriański, era 400-letnia)
/// i stoi tu drugi raz w tym drzewie, obok `memory::handoff`. To nie jest przeoczenie: tamta
/// funkcja jest prywatna, a `src-tauri/src/memory/handoff.rs` nie należy do tego zadania, więc
/// jej udostępnienie jest pytaniem do człowieka (AGENTS.md §7), nie cichym dopiskiem w cudzym
/// pliku. `chrono` odpada z tego samego powodu — `Cargo.toml` też nie jest nasz.
fn stamp(at_ms: i64) -> String {
    let secs = u64::try_from(at_ms.max(0) / 1_000).unwrap_or(0);
    let (days, rest) = (secs / 86_400, secs % 86_400);
    let (hour, minute, second) = (rest / 3600, (rest % 3600) / 60, rest % 60);

    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);

    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

/// Odcisk pliku workflow: odpowiada na pytanie **„czy to był ten sam plan"** i na żadne inne.
///
/// FNV-1a po bajtach z dysku, szesnaście znaków szesnastkowo. Nie jest to funkcja
/// kryptograficzna i nie ma nią być: pytanie brzmi „czy plik jest ten sam", a nie „czy ktoś go
/// podrobił". `sha2` nie jest zależnością tego drzewa, a `Cargo.toml` nie należy do tego
/// zadania (AGENTS.md §7) — więc wybór jest między tymi ośmioma wierszami a odciskiem, którego
/// nie ma wcale.
fn fingerprint(bytes: &[u8]) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    format!("{hash:016x}")
}
