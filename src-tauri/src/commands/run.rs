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
//!   scheduler::execute(graf, token)              ← zależnościami rządzi graf…
//!         │
//!         ├─ krok agenta:   limits::Run::dispatch → miejsce ze WSPÓLNEJ puli aplikacji
//!         │                 (niezmiennik 11: „ile naraz" jest liczbą APLIKACJI, nie biegu)
//!         │                 AgentDriver::start → AgentEvent → Curator → Vec<Line>
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
//! # Wynik kroku → przekazanie → prompt następnego
//!
//! Szew, dla którego istnieje T-32, i cała jego treść mieści się w dwóch zdaniach. Po udanej
//! turze wynik kroku ląduje w `handoffs/` ([`Live::hand_over`]); prompt kroku, który po nim
//! idzie, niesie **ścieżkę** tego pliku ([`Live::prompt_for`]) i nigdy jego treść.
//! Front-matter składa Loadout, ciałem jest dosłownie to, co oddał agent (`ARCHITECTURE` §8).
//!
//! **Indeks, nie transkrypt** (D6 punkt 5). Wklejenie ciała do promptu jest o linijkę tańsze
//! i w pierwszym biegu wygląda lepiej: krok dostaje wszystko, czego mógłby chcieć, i nie musi
//! otwierać ani jednego pliku. Płaci za to każdy krok po nim — przy czwartym prompt niesie trzy
//! poprzednie tury w całości i jest większy niż praca. [T6 §10.2] każe dostarczać „belt and
//! braces", czyli ciało **i** ścieżkę; to jest świadome odejście od tamtego akapitu, nie
//! przeoczenie, i jest jedynym miejscem, w którym te dwa dokumenty się nie zgadzają.
//!
//! Skoro ścieżka jest jedyną drogą do treści, to musi **działać**: katalog przekazań jedzie do
//! sterownika w `RunSpec::extra_dirs`, bo krok `fresh-copy` stoi w `work/<krok>` i bez tego
//! dostałby odnośnik, którego nie wolno mu otworzyć — czyli kontrolkę bez handlera
//! (niezmiennik 16).
//!
//! Kolejność wpisów bierze się **z grafu**, nigdy z chwili zakończenia: dwa biegi tego samego
//! workflow mają dać ten sam prompt, a to, który agent odpowiedział szybciej, zmienia się
//! z biegu na bieg.
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
//! - Kopiuje pliki projektu przy `fresh-copy` (T-33) — patrz [`copy_project_into`].

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{Outcome, RunControl, RunDeps, RunError, RunReport, RunRequest};
use crate::engine::StepId;
use crate::engine::dag::Dag;
use crate::engine::drivers::{AgentDriver, AgentEvent, AgentHandle, DecodedEvent, Policy, RunSpec};
use crate::engine::limits::{self, Limiter};
use crate::engine::line::{Curator, Line, Seen, Status};
use crate::engine::scheduler;
use crate::engine::step::{StepReport, StepState};
use crate::engine::supervisor::GroupProof;
use crate::ipc::LineSink;
use crate::library::agents::{Agent, FileAccess, Overrides, read_agent_file, resolve};
use crate::memory::handoff::{self, Kind, MetaDraft};
use crate::workflow::check::{Level, check_to_run};
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

/// Ile znaków ma tytuł przekazania.
///
/// Tytuł jest **jednym wierszem** płaskiego front-mattera (`memory::handoff`), a instrukcja kroku
/// bywa akapitem: `title:` na dwieście znaków czyta się jak plik, który ktoś uszkodził.
const TITLE_LIMIT: usize = 120;

/// Zdanie, po którym w prompcie zaczyna się indeks przekazań.
///
/// Po angielsku, jak wszystko, co czyta agent i człowiek (decyzja D5), i bez ani jednego naszego
/// słowa z drutu: „handoff" i „fan-in" nie znaczą nic dla kogoś, kto właśnie dostał zadanie
/// (niezmiennik 14).
const HANDOFF_INDEX_OPENS: &str = "Steps before this one left what they found in these files:";

/// I zdanie, którym się kończy. Mówi wprost, że treści w prompcie nie ma — inaczej agent, który
/// nie otworzy pliku, uzna brak cytatu za brak materiału.
const HANDOFF_INDEX_CLOSES: &str =
    "Read the ones you need; their contents were not copied into this prompt.";

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
/// **Pulę miejsc robi sobie sam, na ten jeden bieg** — i to jest wada, nie wygoda: dwie karty
/// dają wtedy `2 × limit` agentów naraz, a semafor ma być jeden na całą aplikację
/// (`docs/ARCHITECTURE.md` §6a, niezmiennik 11). Wołający, który ma pulę wspólną, wchodzi
/// [`run_workflow_with_slots`] i podaje ją argumentem; ta droga zostaje dla tego, kto żadnej
/// nie ma i chce bieg sam dla siebie.
pub async fn run_workflow_inner(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    lines: LineSink,
) -> Result<RunReport, RunError> {
    run_workflow_with_slots(deps, request, lines, Limiter::new(request.how_many_at_once)).await
}

/// Ten sam bieg, tylko miejsca bierze ze **wspólnej puli aplikacji** — jednej dla wszystkich
/// kart, nie jednej na bieg.
///
/// [`run_workflow_inner`] podaje `how_many_at_once` prosto do semafora, który planista zakłada
/// per bieg, więc dwie karty dają `2 × limit` agentów naraz. Przy ~583 MB na agenta
/// `[T7 ryzyko 3, V]` to jest zamrożony laptop, a nie szybsza praca — dlatego semafor ma być
/// jeden na całą aplikację (`docs/ARCHITECTURE.md` §6a, niezmiennik 11).
///
/// **Pula wchodzi argumentem.** Bieg, który robi ją sobie sam, jest nie do odróżnienia od biegu,
/// który robi po jednej na kartę — to samo zdanie stoi przy [`crate::workspace::Registry::new`],
/// i to jest dokładnie ten uchwyt, który tamten rejestr wydaje przez `Registry::slots`. Klon
/// [`Limiter`] dzieli tę samą pulę i to jest cały mechanizm (`engine::limits`).
///
/// 2026-08-17 — naturalnym miejscem tego uchwytu jest pole w [`RunDeps`], żeby wszystkie drzwi
/// do biegu miały je bez wyjątku. `commands/mod.rs` nie należy do T-31 (`AGENTS.md` §7), więc
/// pula wchodzi tędy, a nie tamtędy; scalenie obu dróg w jedną należy do tego, kto będzie mógł
/// dotknąć [`RunDeps`].
///
/// **`deps.control.settle()` musi zostać na KAŻDEJ drodze wyjścia**, także po odmowie: to na to
/// zdanie czeka [`stop_run_inner`], żeby móc wrócić z dowodem (niezmiennik 6). Settle wpisany
/// tylko na szczęśliwej ścieżce zawiesza Stop przy każdym biegu, który padł, i wygląda to jak
/// zawieszony agent, nie jak brakująca linijka. Dlatego cały bieg siedzi w [`the_whole_run`]:
/// stamtąd wychodzi się kilkoma `?`, a stąd — dokładnie jednym `return`.
pub async fn run_workflow_with_slots(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    lines: LineSink,
    slots: Limiter,
) -> Result<RunReport, RunError> {
    /* „Ruszyliśmy" zapala się PRZED pierwszym `?`, a nie po walidacji, i to jest celowe: bieg
     * odrzucony przez walidator też przechodzi tę funkcję, więc zapali za chwilę `settle()` —
     * a `is_working()` czyta oba znaczniki i odpowie wtedy „nie ma czego zatrzymywać". Zapalenie
     * dopiero po walidacji dałoby okno czasu, w którym bieg już czyta dysk, a zamknięcie okna
     * uznałoby, że nie ma nic do roboty. */
    deps.control.begin();
    /* Strumień oddajemy biegowi DO UCHWYTU, bo tura człowieka przychodzi spoza pętli kroku:
     * komendą z okna, w chwili, w której krok czeka na agenta. Klon, nie przekazanie: pompa ma
     * jednego właściciela, a `LineSink` jest klonowalny właśnie dlatego, że sypie do niej kilku
     * producentów naraz. */
    deps.control.lines_go_to(lines.clone());
    let report = the_whole_run(deps, request, lines, slots).await;
    /* PORZUCAMY NADAJNIK, ZANIM OGŁOSIMY ZEJŚCIE, i ta kolejność ma zmierzony powód. Pompa kończy
     * się na zamkniętej kolejce, czyli dopiero wtedy, gdy zniknie każdy `LineSink` — a nasz klon
     * siedzi w uchwycie. Bez tej linii wisiało piętnaście testów biegu i wisiałby każdy prawdziwy
     * bieg (powód w całości przy `RunControl::lines_go_quiet`). */
    deps.control.lines_go_quiet();
    deps.control.settle();
    report
}

/// Bieg od wczytania pliku do zamknięcia księgi. Wydzielony z [`run_workflow_inner`], żeby
/// dowód z `settle()` schodził dokładnie raz, niezależnie od tego, którym `?` się stąd wyszło.
async fn the_whole_run(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    lines: LineSink,
    slots: Limiter,
) -> Result<RunReport, RunError> {
    let plan = plan_run(deps, request)?;
    // Graf budujemy po walidatorze, ale przed katalogiem: `Dag::new` odmawia cyklu przy
    // konstrukcji i jest ostatnią linią obrony, nie pierwszą (`engine::dag`).
    let dag = Dag::new(plan.steps.len(), &plan.arrows)?;

    lay_out_the_run_dir(&plan, deps.project)?;
    let live = Arc::new(Live::new(plan, lines, deps.control.clone(), slots));
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
    // 2026-08-17 (T-31) — semafor planisty ma tu NIC nie ograniczać, i to jest cała treść tego
    // podpięcia. „Ile naraz" jest liczbą CAŁEJ APLIKACJI, więc miejsce bierze każdy krok
    // osobno, ze wspólnej puli ([`Live::a_slot_for_this_step`]). Semafor zakładany per bieg
    // odpowiadał poprawnie na pytanie o jeden bieg i nie odpowiadał w ogóle na to, które zadaje
    // niezmiennik 11: dwie karty po dwa agenty to cztery agenty po ~583 MB, czyli zamrożony
    // laptop, a nie szybsza praca (`docs/ARCHITECTURE.md` §6a).
    //
    // Tyle permitów, ile kroków — czyli tyle, ile trzeba, żeby ten semafor nie odmówił nigdy.
    // Nie zmienia to niczego, przed czym broni `engine::scheduler`: permit wspólnej puli bierze
    // dalej ZADANIE, nie pętla wysyłki, więc różnica między „w kolejce" a „działa" zostaje tam,
    // gdzie była, a szerokość wysyłki dalej nie udaje równoległości.
    let outcome = scheduler::execute(&dag, dag.len(), deps.control.cancel_token(), run_step).await;

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

/// Okno się zamyka: zatrzymuje bieg **z dowodem**, jeśli jest co zatrzymywać.
///
/// # Po co to istnieje
///
/// Zgłoszenie właściciela 2026-08-19: „co się dzieje jak zamykasz apkę a leci jakiś workflow?
/// on się wyłączy?", a zaraz po nim „odpalałem kilka workflow i apkę zamykałem w trakcie".
/// Nie wyłączał się. W `lib.rs` nie było ani jednego `on_window_event`, `CloseRequested` czy
/// `RunEvent`, więc zamknięcie okna kończyło proces Loadouta i **nic więcej**: agenci przechodzili
/// pod PID 1 i dalej pracowali, dalej pisali po plikach projektu i dalej palili limit u dostawcy,
/// aż ktoś odpalił Loadouta ponownie (`recovery.rs`, nagłówek: „Agenci nie giną razem
/// z Loadoutem"). Odzyskiwanie sprząta to dopiero przy NASTĘPNYM starcie, więc rachunek rósł przez
/// cały czas, w którym aplikacja była zamknięta — czyli dokładnie wtedy, kiedy nikt nie patrzył.
///
/// # Dlaczego to pytanie o `is_working` jest konieczne
///
/// [`stop_run_inner`] czeka na dowód śmierci grupy procesów (niezmiennik 6), a dowód zapala bieg,
/// który naprawdę przez siebie przeszedł. Wywołane na uchwycie biegu, którego nikt nie uruchomił,
/// czekałoby **bez końca** — czyli zamknięcie okna wieszałoby aplikację w najczęstszym przypadku
/// ze wszystkich: kiedy nic nie biegnie.
///
/// # Co ta funkcja świadomie robi wolno
///
/// Wraca dopiero z dowodem, więc zamknięcie okna trwa tyle, ile schodzenie agentów (TERM, potem
/// KILL — `engine::supervisor`). Okno, które zamyka się natychmiast i zostawia procesy, jest
/// szybsze i jest kłamstwem: człowiek czyta zniknięcie okna jako koniec pracy.
pub async fn stop_before_closing(deps: &RunDeps<'_>) -> Result<Outcome, RunError> {
    if !deps.control.is_working() {
        // Nie ma czego zatrzymywać i nie ma na co czekać. `Cancelled` byłoby tu zdaniem
        // o biegu, którego nie ma — a `Done` mówi prawdę: nic nie zostało niedokończone.
        return Ok(Outcome::Done);
    }
    stop_run_inner(deps).await
}

/// Puszcza bieg dalej z punktu kontrolnego (T3 §6.1 reguła 5).
///
/// Punkt kontrolny zatrzymuje **bieg**, nie krok, i nic za nim nie startuje, dopóki człowiek nie
/// odpowie. Pytanie, które pojawia się na ekranie po tym, jak agent już zrobił swoje, nie jest
/// pytaniem.
pub async fn continue_run_inner(
    deps: &RunDeps<'_>,
    answer: Option<String>,
) -> Result<(), RunError> {
    // Licznik, nie flaga (`RunControl::go_on`): bieg z dwoma punktami kontrolnymi przeszedłby
    // przez drugi bez pytania, gdyby zgoda była flagą, która raz zapalona zostaje zapalona.
    //
    // 2026-08-18 — TREŚĆ ODPOWIEDZI JEDZIE RAZEM ZE ZGODĄ. Do tego dnia ta komenda nie brała
    // żadnego argumentu: człowiek pisał zdanie, pytanie znikało z ekranu, bieg ruszał — i to
    // zdanie nie trafiało ani do promptu następnego kroku, ani na dysk. Kontrolka, która
    // przyjmuje tekst i go wyrzuca, jest gorsza niż jej brak (niezmiennik 16).
    deps.control.go_on_with(answer);
    // Wracamy dopiero, kiedy bieg naprawdę ruszył — tak samo jak Stop wraca dopiero z dowodem.
    // Bez tego ekran wraca do człowieka w chwili, w której bieg **jeszcze stoi**, i pierwsze,
    // co ten człowiek widzi po odpowiedzeniu na pytanie, to dalej „paused". Czekanie kończy się
    // natychmiast, gdy nie było na co odpowiadać, i kończy się także wtedy, gdy bieg w tym
    // czasie zszedł (`RunControl::wait_until_moving`).
    deps.control.wait_until_moving().await;
    Ok(())
}

/// Mówi coś agentowi, który **właśnie pracuje** — kolejna tura w jego żywej sesji.
///
/// # Po co to istnieje
///
/// Zgłoszenie właściciela 2026-08-18, dwa razy: „i pisać z nim nie mogę", potem „dalej nie działa
/// pisanie do agenta przez terminal". Droga do żywej sesji nie istniała, i nie z braku komendy:
/// `stdin` był polem uchwytu, więc pisanie wymagało `&mut`, a uchwyt jest pożyczony mutowalnie
/// przez całą turę ([`Live::one_turn`]). Naprawa poszła w przyczynę — potok należy dziś do
/// jednego zadania-pisarza, a bieg trzyma nadajniki pod nazwami kroków
/// ([`RunControl::step_can_hear`]).
///
/// # Dlaczego to stoi TUTAJ, a nie w skorupie komendy
///
/// Bo to jest polityka, a nie transport: cztery różne odmowy, każda innym zdaniem, i wybór
/// adresata przy jednym pracującym agencie. Do 2026-08-18 mieszkało to w całości w
/// `#[tauri::command]` w `ipc.rs` — czterdzieści linii decyzji w skorupie, która ma mieć dwie
/// (niezmiennik 1 i 23). Miało to jeden, konkretny koszt: `State<'_, AppState>` nie da się
/// zbudować bez żywego Tauri, więc na ANI JEDNĄ z tych czterech odmów nie dało się napisać
/// kryterium — a kryterium, którego nie da się napisać, jest zachowaniem, którego nikt nie
/// sprawdził.
///
/// # Adresat
///
/// `agent` jest opcjonalny i to jest wygoda z pomiarem, nie zgadywanie: kiedy pracuje dokładnie
/// jeden krok, nie ma czego wybierać. Przy dwóch i więcej **odmawiamy z listą nazw** — kontrolka,
/// która wysyła tekst do losowego z dwóch agentów, jest gorsza niż odmowa (niezmiennik 16).
pub async fn say_to_agent_inner(
    control: &RunControl,
    agent: Option<&str>,
    text: &str,
) -> Result<(), RunError> {
    let said = text.trim();
    if said.is_empty() {
        return Err(RunError::NothingToSay);
    }
    /* Lista brana RAZ i po niej rozstrzygamy wszystko: drugi odczyt między wyborem adresata
     * a wzięciem głosu dałby dwie różne odpowiedzi na jedno pytanie „kto pracuje", a wtedy
     * zdanie odmowy mogłoby wymieniać kroki, których w chwili wysyłki już nie ma. */
    let listening = control.who_is_listening();
    let named = agent.map(str::trim).filter(|one| !one.is_empty());

    let to = match (named, listening.as_slice()) {
        (Some(named), _) => named.to_owned(),
        (None, [only]) => only.clone(),
        (None, []) => return Err(RunError::NobodyIsWorking),
        (None, many) => {
            return Err(RunError::SeveralAreWorking {
                names: many.to_vec(),
            });
        }
    };

    let voice = control.voice_of(&to).ok_or_else(|| {
        if listening.is_empty() {
            RunError::ThatOneFinished
        } else {
            RunError::NoSuchAgentWorking {
                name: to.clone(),
                working: listening.clone(),
            }
        }
    })?;

    // Kanał, nie uchwyt: nadajnik jest klonowalny i nie wymaga `&mut`, czyli da się nim pisać
    // do sesji, której tura właśnie trwa. Cała naprawa mieści się w tym jednym zdaniu.
    voice
        .send(crate::engine::drivers::ToAgent::Turn(said.to_owned()))
        .await
        .map_err(|_| RunError::StoppedListening { name: to.clone() })?;

    /* DOPIERO TERAZ WIDAĆ TO NA EKRANIE, i kolejność jest tu treścią kryterium.
     *
     * Zgłoszenie właściciela 2026-08-19: „a może odpisuje on, ale na pewno nie widać moich
     * wiadomości". Zdanie dochodziło do modelu i nie zostawiało śladu w strumieniu, bo tura
     * człowieka nie miała nośnika na drucie (powód w całości przy `Line::Told`).
     *
     * PO wysłaniu, nie przed: wiersz dopisany wcześniej pokazywałby w historii zdanie, które za
     * chwilę odbije się o `StoppedListening` — czyli historia twierdziłaby, że agent coś usłyszał,
     * a nie usłyszał. Odwrotna kolejność jest tą, która kłamie w pliku (niezmiennik 4).
     *
     * Wynik świadomie porzucony: pełna kolejka do okna jest normalnym stanem szybkiego agenta
     * (`ipc::Sent`), a zdanie i tak POSZŁO. Odmowa w tym miejscu mówiłaby człowiekowi, że jego
     * tura nie doszła, kiedy doszła. */
    let _ = control.show_in_the_run(crate::engine::line::Line::Told {
        agent: to,
        text: said.to_owned(),
    });
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
    /// Kiedy wstała maszyna. Czytane RAZ, przy planowaniu: ten sam bieg ma nosić jedną
    /// odpowiedź, a nie tyle, ile razy ktoś zapyta system.
    boot_id: Option<String>,
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

/// Czym skończyło się czekanie na turę. Trzy stany, bo `Option` umiał powiedzieć dwa, a od
/// T-35 „skończył się czas" jest czymś innym niż „człowiek nacisnął Stop": pierwsze jest
/// porażką kroku z nazwanym powodem, drugie jest anulowaniem i nie jest niczyją winą.
enum Ended {
    /// Tura wróciła sama — z wynikiem albo z błędem sterownika.
    Turn(anyhow::Result<crate::engine::drivers::Outcome>),
    /// Człowiek nacisnął Stop.
    Stopped,
    /// Krok przekroczył swój limit czasu.
    Overdue,
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
    /// Instrukcje kroku, dosłownie z pliku workflow.
    ///
    /// To jeszcze **nie** jest prompt: prompt składa [`Live::prompt_for`] w chwili startu kroku,
    /// z tej instrukcji i z indeksu przekazań poprzedników. Przy planowaniu nie zszedł jeszcze
    /// nikt, więc indeksu nie ma tu z czego zbudować. Jedno i drugie jedzie do sterownika jako
    /// **dane** i wychodzi stdinem (niezmiennik 9).
    prompt: String,
    /// Model z konfiguracji efektywnej.
    model: Option<String>,
    /// Prompt systemowy agenta. To jest konfiguracja agenta, nie treść zadania.
    system_append: Option<String>,
    /// Co agentowi wolno zrobić z plikami — po ludzku, w trzech wariantach.
    policy: Policy,
    /// Po ilu minutach bez końca tury odbieramy krokowi robotę.
    ///
    /// 2026-08-17 (T-35) — do tego dnia `give_up_after_minutes` z definicji agenta NIE MIAŁO
    /// ANI JEDNEGO CZYTELNIKA: zaklinowany agent wisiał do ręcznego Stopu. Według taksonomii
    /// tego repo to błąd **finansowy**, nie higieniczny — proces pali limit u dostawcy tak
    /// długo, jak długo nikt nie patrzy. `ARCHITECTURE.md` §11 zapowiada właśnie tę ochronę
    /// zamiast `--max-turns`.
    give_up_after: Duration,
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
    // `check_to_run`, nie `check`: krok bez agenta jest przy zapisie ostrzeżeniem (szkic
    // w połowie zbudowany ma się zapisać), a tutaj problemem — bo za sekundę miałby ruszyć.
    // Powód w całości stoi przy `workflow::check::check_to_run`.
    if let Some(refusal) = check_to_run(&file)
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
        knows: what_the_agents_know(deps.home),
        /* Zadanie z wiersza wejścia, przycięte. Brak zadania i zadanie z samych spacji to jeden
         * fakt („nic nie kazano"), a dwa różne prompty za jeden fakt to dwie różne odpowiedzi
         * na pytanie, co ten bieg buduje. */
        task: request
            .task
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .to_owned(),
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
        // Pytamy system RAZ, tutaj: ten bieg ma nosić jedną odpowiedź przez całe życie.
        // Odczyt przy każdym zrzucie dałby wartości, które teoretycznie mogą się różnić —
        // i strażnik porównywałby wtedy coś z czymś innym.
        boot_id: crate::engine::supervisor::machine_booted_at(),
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

/// Notatki, które człowiek dopuścił do użytku, jako blok tekstu na początek promptu.
///
/// 2026-08-18 — PO CO TO ISTNIEJE. `memory::notes::what_you_know` istniało od T-17 i miało
/// wołających **wyłącznie w trzech plikach testowych**. Prompt kroku brzmiał
/// `step.instructions.clone()` i nic poza tym, więc człowiek przestawiał notatkę na „in use",
/// a agent w kolejnym biegu nic o niej nie wiedział. Siedem zielonych kryteriów T-17 stało nad
/// martwym końcem: cała sekcja Pamięć była mechanizmem bez odbiorcy.
///
/// DWA ZAKRESY, KAŻDY ZE SWOIM BUDŻETEM. `Scope::Everywhere` idzie pierwszy, bo jest szerszym
/// tłem, a `Scope::ThisProject` po nim — bliższy kontekst czyta się na końcu, tuż przed samym
/// zadaniem. Każdy zakres ma własny sufit długości (`Scope::cap`), więc dwa wywołania nie są
/// obejściem budżetu: to jest budżet policzony tak, jak go zaprojektowano [T6 §5.3].
///
/// `Scope::ThisAgent` NIE wchodzi i to jest zgłoszenie, nie przeoczenie: filtrowanie po agencie
/// wymaga tożsamości agenta w chwili liczenia bloku, a blok liczymy raz na bieg, nie raz na krok.
/// Zrobienie tego dobrze znaczy policzyć trzeci blok per krok — osobna zmiana.
///
/// **Odczyt, który się nie udał, nie zabiera biegu** (niezmiennik 5): katalog pamięci na świeżej
/// maszynie nie istnieje i to jest stan normalny. Wtedy agent po prostu nic nie wie.
fn what_the_agents_know(home: &Path) -> String {
    let root = super::memory::notes_root(home);
    let Ok(notes) = crate::memory::notes::scan_notes(&root) else {
        tracing::debug!(root = %root.display(), "the notes could not be read; no step will carry them");
        return String::new();
    };
    let mut text = String::new();
    for scope in [
        crate::memory::notes::Scope::Everywhere,
        crate::memory::notes::Scope::ThisProject,
    ] {
        let block =
            crate::memory::notes::what_you_know(&notes, crate::memory::notes::Budget::of(scope));
        if block.text.is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str(&block.text);
    }
    text
}

/// Zadanie kroku, poprzedzone tym, co wiadomo.
///
/// Pusty blok znaczy „nic nie wiadomo" i wtedy prompt jest DOKŁADNIE zadaniem kroku, bez ani
/// jednego dodatkowego bajtu: nagłówek nad pustką uczy model, że ta sekcja bywa pusta,
/// i kosztuje długość za nic (ten sam powód stoi przy `Block::text`).
fn with_what_we_know(knows: &str, task: &str) -> String {
    if knows.is_empty() {
        return task.to_owned();
    }
    format!("{knows}\n\n{task}")
}

/// Znacznik, którym plik workflow wskazuje, GDZIE w promptcie kroku ma stanąć zadanie człowieka.
///
/// Ta sama rodzina, co `{{copy}}` i `{{copies}}` [T3 §4.3] — plik już umie mówić o rzeczach,
/// które powstają dopiero przy starcie.
const TASK_MARK: &str = "{{task}}";

/// Nagłówek nad zadaniem, kiedy plik nie wskazał miejsca sam.
///
/// Zdanie, nie słowo: krok czyta to razem ze swoim promptem, więc musi wiedzieć, czyje to jest
/// polecenie i że dotyczy całego biegu, a nie tylko jego jednego.
const TASK_HEADING: &str = "What the person asked for, for this whole run:";

/// Zadanie kroku z wpisanym zadaniem CAŁEGO biegu — albo bez, kiedy nikt go nie podał.
///
/// # Dwa sposoby, jeden powód
///
/// Jeśli prompt kroku zawiera [`TASK_MARK`], zadanie ląduje **dokładnie tam** — bo plik, który
/// zadał sobie trud wskazania miejsca, wie o swoim promptcie więcej niż my. Jeśli nie zawiera,
/// zadanie idzie na GÓRĘ, pod nagłówkiem. Nie na dół: prompt kroku kończy się zwykle instrukcją
/// „co oddać", a zdanie doklejone po niej czyta się jak dopisek po podpisie.
///
/// Przy pustym zadaniu prompt NIE dostaje ani nagłówka, ani jednego dodatkowego bajtu — a sam
/// znacznik **znika**. To jedyne miejsce, w którym ta funkcja zmienia prompt bez zadania, i ma
/// nazwany powód: `{{task}}` zostawiony w tekście jest jedyną rzeczą w całym promptcie, której
/// model nie umie przeczytać inaczej niż jako literalny nawias — czyli wygląda jak zepsute
/// podstawienie, którym jest.
fn with_the_task(task: &str, instructions: &str) -> String {
    if task.is_empty() {
        return instructions.replace(TASK_MARK, "");
    }
    if instructions.contains(TASK_MARK) {
        return instructions.replace(TASK_MARK, task);
    }
    format!("{TASK_HEADING}\n{task}\n\n{instructions}")
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
    /// Co agent WIE, zanim przeczyta swoje zadanie — notatki, które człowiek dopuścił do użytku.
    ///
    /// Liczone RAZ, przy planowaniu, nie przy każdym kroku: ten sam bieg ma nieść jedną
    /// odpowiedź na pytanie „co wiadomo". Odczyt per krok dałby dwóm krokom tego samego biegu
    /// dwa różne konteksty, gdyby ktoś w międzyczasie dopuścił notatkę — a różnicy nie widać
    /// nigdzie poza rachunkiem za długość.
    knows: String,
    /// Co człowiek kazał zbudować TYM biegiem — puste, kiedy nie kazał nic ponad plik.
    ///
    /// Jedna wartość na bieg, dokładnie jak [`Setup::knows`] i z tego samego powodu: zadanie
    /// odczytane per krok mogłoby się różnić między krokami jednego biegu, a wtedy „co my właściwie
    /// budujemy" przestaje mieć jedną odpowiedź.
    task: String,
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
    let saved = find_agent(&setup.library, &step.agent, &step.name)?;
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
        // Zadanie kroku POPRZEDZONE tym, co człowiek dopuścił do użytku (`what_the_agents_know`).
        // Bez człowieka blok jest pusty i prompt jest dokładnie zadaniem kroku —
        // `docs/ARCHITECTURE.md` §2 pytanie 5 obiecuje właśnie to.
        // Zadanie CAŁEGO biegu wchodzi do zadania kroku (`with_the_task`), a dopiero to, co z tego
        // wyszło, dostaje blok „co wiadomo". Ta kolejność jest treścią: notatki są kontekstem
        // stojącym nad wszystkim, zadanie biegu jest polem pracy, a prompt kroku jest robotą
        // w tym polu — od najogólniejszego do najkonkretniejszego, czyli tak, jak to czyta model.
        prompt: with_what_we_know(
            &setup.knows,
            &with_the_task(&setup.task, &step.instructions),
        ),
        model: some_text(&effective.model),
        // Prompt systemowy agenta, nie treść zadania: treść zadania w tym polu byłaby
        // niezmiennikiem 9 złamanym po cichu, bo stąd wchodzi do argv.
        system_append: some_text(&effective.instructions),
        policy: policy_of(effective.file_access),
        // Minuty z definicji agenta. Zero znaczyłoby „poddaj się natychmiast", więc traktujemy
        // je jak brak zdania i zostawiamy domyślne dwadzieścia minut z `library::agents`:
        // limit, który ubija każdy krok w chwili startu, jest gorszy niż brak limitu.
        give_up_after: Duration::from_secs(u64::from(effective.give_up_after_minutes.max(1)) * 60),
        effective: serde_json::to_value(&effective)?,
    })
}

/// Znajduje w bibliotece agenta o tym identyfikatorze.
///
/// Szukamy po `id`, nie po nazwie pliku: krok workflow nazywa agenta identyfikatorem, bo ten
/// przeżywa zmianę nazwy (T3 §3.1). Plik, którego nie da się przeczytać, **nie zabiera biegu**,
/// który go nie używa — ale jeśli szukanego nie ma, to właśnie jego błąd jest odpowiedzią,
/// bo „nie ma takiego agenta" i „ten plik jest zepsuty" naprawia się inaczej [T4 §10].
fn find_agent(library: &Path, id: &str, step: &str) -> Result<Agent, RunError> {
    // 2026-08-18 — KATALOG, KTÓREGO NIE MA, TO ZDANIE O AGENTACH, NIE O SYSTEMIE PLIKÓW.
    // `fs::read_dir(library)?` szło tu wprost w `RunError::Io`, który jest przezroczysty, więc
    // pierwsze uruchomienie po instalacji kończyło się „No such file or directory (os error 2)".
    // Katalog `agents/` powstaje dopiero przy pierwszym zapisanym agencie, czyli na świeżej
    // maszynie NIE ISTNIEJE — to jest stan normalny, nie awaria dysku, i ma o tym mówić.
    let listing = match fs::read_dir(library) {
        Ok(listing) => listing,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(RunError::NoAgentsSaved {
                step: step.to_owned(),
            });
        }
        Err(error) => return Err(RunError::Io(error)),
    };
    let mut files: Vec<PathBuf> = listing
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    // Katalog, który istnieje i jest pusty, jest tym samym faktem co katalog, którego nie ma:
    // nie ma z czego wybrać. Dwa różne zdania o jednym stanie byłyby dwoma miejscami prawdy.
    if files.is_empty() {
        return Err(RunError::NoAgentsSaved {
            step: step.to_owned(),
        });
    }
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
/// 2026-08-17 (T-33) — `fresh-copy` dostaje **własny katalog z kopią plików projektu**.
///
/// Do tego dnia dostawał katalog PUSTY, a `ARCHITECTURE.md` §2 p. 4 obiecuje „każdy krok dostaje
/// własną kopię twoich plików". To nie była brakująca wygoda: `workflow::check` odmawia zapisu
/// workflow, w którym dwa kroki piszą po tych samych ścieżkach (T-12), i ta walidacja ZAKŁADA,
/// że fresh-copy chroni. Nie chroniła — więc krok „na własnej kopii" pracował na pustce zamiast
/// na projekcie, co jest gorsze od kolizji: agent nie widzi plików, które ma zmienić.
///
/// Ta funkcja dalej tylko WSKAZUJE katalog. Kopiowanie robi [`lay_out_the_run_dir`], bo dotyka
/// dysku, a plan ma być czystym rachunkiem — planowanie, które zapisuje, nie da się powtórzyć
/// przy wznowieniu.
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

/// Czego NIE kopiujemy do własnej kopii kroku.
///
/// `.loadout` jest tu obowiązkowy, nie kosmetyczny: katalog biegu leży pod
/// `<projekt>/.loadout/runs/<…>/work/<krok>`, więc kopiowanie projektu do siebie samego
/// schodziłoby w nieskończoność, aż do wyczerpania dysku albo limitu ścieżki.
///
/// Pozostałe trzy są wyborem, nie koniecznością, i wybór jest po stronie CZASU: `.git`
/// dużego repozytorium to gigabajty, `node_modules` i `target` odtwarza się jedną komendą.
/// Krok, który ich naprawdę potrzebuje, ma tryb „katalog projektu" i wtedy pracuje na
/// oryginale — świadomie, a nie przez przypadek.
const NOT_COPIED: [&str; 4] = [".git", ".loadout", "node_modules", "target"];

/// Kopiuje drzewo projektu do katalogu roboczego kroku.
///
/// Rekurencja jawna, bez zewnętrznej skrzyni: to jest ~20 wierszy, a każda zależność w tym
/// miejscu musiałaby jeszcze umieć pomijać `.loadout` (patrz [`NOT_COPIED`]).
///
/// Dowiązania symboliczne kopiujemy JAKO PLIKI (`fs::copy` podąża za nimi). Odtwarzanie
/// dowiązania wskazującego poza kopię dawałoby krokowi ścieżkę do oryginału — czyli dziurę
/// w izolacji, o którą całe to zadanie chodzi.
fn copy_project_into(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if NOT_COPIED.iter().any(|skip| name == *skip) {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        if entry.file_type()?.is_dir() {
            copy_project_into(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Tworzy katalog biegu i to, co do niego należy — **dopiero po planie**.
fn lay_out_the_run_dir(plan: &Plan, project: &Path) -> Result<(), RunError> {
    // `logs/` powstaje razem z katalogiem, a nie przy pierwszej linii: katalog biegu bez niego
    // czyta się jak bieg, w którym agent nic nie powiedział, zamiast jak bieg, który jeszcze nic
    // nie zapisał.
    fs::create_dir_all(plan.dir.join(LOGS_DIR))?;
    for step in &plan.steps {
        if let Job::Agent(job) = &step.job
            && job.ours
        {
            // Odmowa jest GŁOŚNA i zatrzymuje bieg, zanim ruszy jakikolwiek proces. Ciche
            // zejście do wspólnego katalogu dałoby dwa kroki piszące po tych samych plikach,
            // z których każdy skończyłby się „sukcesem" (niezmiennik 12).
            copy_project_into(project, &job.cwd).map_err(|error| RunError::NoFreshCopy {
                step: step.name.clone(),
                why: error.to_string(),
            })?;
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
    /// Wspólna pula miejsc **całej aplikacji** i pauza dostawcy — jedne drzwi dla obu
    /// (`engine::limits`, nagłówek pliku): wysyłka pyta bieg, bieg pyta pulę. Uchwyt jest
    /// klonem cudzej puli, nigdy własną: pula zakładana per bieg jest nie do odróżnienia od
    /// tej, przez którą dwie karty dają `2 × limit` agentów naraz (niezmiennik 11).
    gate: limits::Run,
    /// Chwila startu biegu. Kurator dostaje czas **argumentem**, bo kurator z własnym zegarem
    /// nie da się przetestować bez `sleep`.
    began: Instant,
    /// Gdzie leży przekazanie każdego kroku — po jednym wpisie na krok, w kolejności z pliku
    /// workflow. `None` znaczy „ten krok jeszcze nic nie oddał": kafelek kontrolny nie oddaje
    /// nigdy, a krok anulowany albo padnięty nie ma czego przekazać.
    ///
    /// Zamek osobny od [`Live::book`] z rozmysłem: to nie jest stan, który jedzie do `run.json`.
    /// Ścieżka przekazania **jest** w plikach — nazwa pliku otwiera się numerem kroku — więc
    /// druga kopia w `run.json` byłaby drugim miejscem, w którym mieszka jeden fakt
    /// (niezmiennik 13), i tym, które kłamie po pierwszej ręcznej edycji katalogu.
    ///
    /// **Nie przechodzi przez `await`** (niezmiennik 8): oba wywołania, które go biorą
    /// ([`Live::filed`], [`Live::handed_before`]), oddają go w tym samym wyrażeniu.
    handoffs: Mutex<Vec<Option<PathBuf>>>,
}

/// Zmienna połowa biegu — dokładnie to, co zmienia się między zrzutami `run.json`.
struct Book {
    /// Stan **biegu**. Jedyne miejsce, w którym istnieje `paused`.
    status: RunState,
    /// Czy bieg stoi na pytaniu do człowieka.
    ///
    /// 2026-08-17 (T-31) — powody, dla których bieg stoi, są od teraz DWA i mijają niezależnie:
    /// to pytanie i limit dostawcy. Bez tego pola oba pisałyby `status` bezwarunkowo i kasowały
    /// się nawzajem — ten, który skończył pierwszy, ogłaszałby bieg jako idący, choć drugi wciąż
    /// go trzyma. Do `run.json` to pole nie wychodzi: `RunFile` bierze z księgi sam `status`,
    /// a dwa pola o jednym fakcie na ekranie są dokładnie tym, czego zabrania niezmiennik 13.
    asking: bool,
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

/// Jedno wejście indeksu: przekazanie jednego poprzednika, gotowe do wpisania w prompt.
#[derive(Debug)]
struct Handed {
    /// Nazwa kafelka, który to oddał. Ta sama, która stoi na ekranie jako etykieta wiersza —
    /// prompt nazywa więc krok tym samym słowem, co UI (niezmiennik 13). Identyfikator kroku
    /// ani uuid agenta nie mają tu czego szukać (niezmiennik 14).
    from: String,
    /// Gdzie ten plik leży, **bezwzględnie**: katalogiem roboczym kroku `fresh-copy` jest
    /// `work/<krok>`, więc ścieżka względna katalogu biegu nie rozwiązałaby się z miejsca,
    /// w którym agent naprawdę stoi.
    path: PathBuf,
}

/// Co krok dostaje na wejściu: prompt, ślad po tym, co do niego wstrzyknięto, i katalogi,
/// które musi móc otworzyć.
#[derive(Debug)]
struct Told {
    /// Instrukcja kroku plus indeks przekazań poprzedników. Jedzie stdinem (niezmiennik 9).
    prompt: String,
    /// Dokładnie to, co Loadout wstrzyknął — nie to, co agent twierdzi, że przeczytał.
    /// Pochodzenie, o którym nie da się skłamać [T6 §10.2]; wchodzi jako `reads` do przekazania
    /// **tego** kroku.
    ///
    /// Ścieżki względem katalogu biegu, a nie bezwzględne jak w prompcie: przekazanie jest
    /// plikiem, który przeżywa `cp -r` katalogu biegu (niezmiennik 4), a ścieżka z `/var/folders`
    /// w środku przestaje po takiej kopii cokolwiek znaczyć.
    reads: Vec<String>,
    /// Katalog przekazań, kiedy krok ma co czytać. Pusty, kiedy nie ma: `--add-dir` na katalog,
    /// w którym nic dla tego kroku nie leży, poszerza mu dostęp bez powodu.
    extra_dirs: Vec<PathBuf>,
}

impl Live {
    /// Świeży bieg: wszystkie kroki czekają, nic jeszcze nie ruszyło.
    fn new(plan: Plan, lines: LineSink, control: RunControl, slots: Limiter) -> Self {
        // Kopia stanów kroków, którą dostaje limit dostawcy, jest **martwa z rozmysłem**:
        // `engine::limits::Run` ma pełny dostęp do statusów i podejść dokładnie po to, żeby
        // T-21 mogło dowieść, że pauza ich nie rusza (`[T7 §7.2]`: „a pause, not a failure").
        // Księga tego biegu żyje niżej, w [`Live::book`], i to ona jedzie do `run.json`.
        let gate = limits::Run::new(slots, &vec![StepState::Pending; plan.steps.len()]);
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
        let handoffs = Mutex::new(vec![None; plan.steps.len()]);
        Self {
            plan,
            book: Mutex::new(Book {
                status: RunState::Running,
                asking: false,
                started_at: None,
                ended_at: None,
                steps,
            }),
            lines,
            control,
            gate,
            began: Instant::now(),
            handoffs,
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
    /// Stan kroku **do okna**, jednym wierszem `stepState`.
    ///
    /// 2026-08-18 — PO CO TO ISTNIEJE. Stan kroku żył wyłącznie w księdze i w `run.json`, więc
    /// okno nie dostawało go nigdy: `RunState.steps` przychodziło z pliku workflow w chwili
    /// kliknięcia Start, z każdym krokiem na `pending`, i **zostawało tak do końca biegu**.
    /// Skutkiem nie była nieaktualna liczba: pasek loadoutu stał na samych obrysach, a kafelek
    /// agenta, który właśnie edytował pliki, pokazywał „waiting". Sześć z siedmiu stanów
    /// z `docs/ARCHITECTURE.md` §5 było po stronie okna NIEOSIĄGALNYCH.
    ///
    /// `node_key`, nie `id`: okno rozpoznaje swój kafelek po identyfikatorze **z pliku
    /// workflow**, bo z tego pliku powstał plan paska, zanim Rust powiedział pierwsze słowo
    /// (`src/state/run.ts`, `withStepStates` porównuje `step.id === line.stepId`). Świeży uuid
    /// biegu byłby tu kluczem, którego okno nigdy nie widziało.
    ///
    /// `name`, nie identyfikator, w polu `agent`: to ten sam podpis, którym ten krok mówi
    /// w każdym innym wierszu (`forward(…, plan.steps[id].name)`), więc szyna agentów nie
    /// dostaje dwóch nazw na jeden kafelek (niezmiennik 13).
    ///
    /// Wiersz idzie **poza kuratorem** i to jest wymóg, nie skrót: kuracja rozstrzyga, co
    /// człowiek czyta o CZYNNOŚCIACH agenta (niezmiennik 15), a to jest fakt o biegu. Puszczony
    /// przez sklejanie zniknąłby w grupie `read` albo poczekał na jej domknięcie — czyli pasek
    /// przestawiałby się z opóźnieniem względem tego, co widać w strumieniu.
    fn announce(&self, id: StepId, state: StepState) {
        let _ = self.lines.send(Line::StepState {
            agent: self.plan.steps[id].name.clone(),
            step_id: self.plan.steps[id].node_key.clone(),
            state: state.name().to_owned(),
        });
    }

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
            // Kiedy wstała maszyna, na której ten bieg ruszył. STRAŻNIK odzyskiwania po awarii:
            // bez niego `recovery::decide` odmawia sprzątania (`NO_BOOT_TIME`), bo po restarcie
            // zapisany `pgid` z dużym prawdopodobieństwem należy do niewinnego procesu
            // (`kern.maxproc` = 16 000, więc PID-y przewijają się w godzinach).
            boot_id: self.plan.boot_id.as_deref(),
            started_at: book.started_at,
            ended_at: book.ended_at,
            error: None,
            steps,
        }
    }

    /// Jeden krok, od pierwszego wpisu w księdze po ostatni.
    ///
    /// `self: Arc<Self>`, bo pętla czytająca zdarzenia sterownika jest **osobnym zadaniem**
    /// ([`forward`]) i musi umieć powiedzieć o limicie dostawcy temu samemu biegowi.
    async fn step(self: Arc<Self>, id: StepId, cancel: CancellationToken) -> StepReport {
        // Miejsce ZANIM cokolwiek wpiszemy do księgi: `running` z chwilą startu wpisaną przed
        // wzięciem miejsca to ten sam fałsz, przed którym stoi niezmiennik 11 — krok stojący
        // w kolejce czytałby się jak krok, który działa.
        //
        // Trzyma się do końca kroku, bo `Slot` oddaje miejsce w `Drop`: wychodzi więc także
        // przez panikę i przez anulowanie. Miejsce zwracane wyłącznie na szczęśliwej ścieżce
        // daje pulę, która kurczy się przez cały bieg, aż nic już nie startuje.
        let _slot = match &self.plan.steps[id].job {
            Job::Agent(_) => {
                let Some(slot) = self.a_slot_for_this_step(&cancel).await else {
                    // Stop, zanim ten krok w ogóle ruszył: `cancelled`, nie `skipped` — nikt
                    // wyżej nie padł, człowiek zatrzymał bieg [T7 §9.3]. Księga zostaje bez
                    // chwili startu, bo startu nie było, a stan końcowy dopisze planista.
                    return StepReport::Cancelled;
                };
                Some(slot)
            }
            // Kafelek kontrolny miejsca NIE bierze i to jest wybór, nie przeoczenie: pula liczy
            // agentów po ~583 MB (`[T7 §7.1, V]`), a pytanie do człowieka nie waży nic i potrafi
            // czekać godzinami. Pytanie trzymające miejsce ze wspólnej puli zagłodziłoby przy
            // limicie 1 wszystkie pozostałe karty, i to na tak długo, jak długo nikt nie patrzy
            // na ekran.
            Job::Ask { .. } => None,
        };

        let at = now_ms();
        self.update(|book| {
            book.started_at.get_or_insert(at);
            let step = &mut book.steps[id];
            step.status = StepState::Running;
            step.started_at = Some(at);
        });
        // Po zapisie do księgi, nie przed: ekran, który wie o kroku wcześniej niż `run.json`,
        // pokazywałby po awarii aplikacji stan, którego odzyskiwanie nie potrafi odtworzyć.
        self.announce(id, StepState::Running);

        let report = match &self.plan.steps[id].job {
            Job::Agent(job) => self.run_agent(id, job, &cancel).await,
            Job::Ask { question } => self.wait_for_a_person(id, question.as_deref()).await,
        };

        let ended = match report {
            StepReport::Succeeded => StepState::Succeeded,
            StepReport::Failed => StepState::Failed,
            StepReport::Cancelled => StepState::Cancelled,
        };
        self.update(|book| {
            let step = &mut book.steps[id];
            step.status = ended;
            step.ended_at = Some(now_ms());
        });
        self.announce(id, ended);
        report
    }

    /// Dostawca kazał czekać: bieg przestaje wysyłać i **mówi o tym dyskowi**.
    ///
    /// Adapter na pięć linii, polityka w jednym rdzeniu (niezmiennik 23): co znaczy „odmowa",
    /// rozstrzyga `engine::limits::read_gate` i wyłącznie on — prawdziwa linia z **udanego**
    /// biegu niesie `"status":"allowed"` obok dwóch pól ze słowem „rejected", a te zdarzenia to
    /// 1,3% normalnego strumienia `[T7 §4.3, V]`. Wersja czytająca tutaj `pause_run` byłaby
    /// drugą kopią tej reguły, a przy dwóch kopiach zawsze czyta się tę, która akurat kłamie.
    /// Wracają więc na drut dokładnie te dwa pola, które tamten czyta, pod nazwami z drutu.
    ///
    /// **Chwili powrotu nie zapisujemy do `run.json`** i to jest wybór (niezmiennik 13): niesie
    /// ją już wiersz kuratora (`Line::Problem::resets_at`), a godzinę lokalną rysuje z niej front
    /// [T7 §7.2]. Druga kopia byłaby drugą rzeczą do utrzymania w zgodzie, a jedna z dwóch jest
    /// zawsze tą nieaktualną. Na dysku zostaje sam fakt „bieg stoi" — bo stan, który nie dociera
    /// na dysk, nie przeżywa awarii aplikacji (niezmiennik 4).
    fn the_provider_said_wait(&self, status: &str, resets_at: i64) {
        let told = serde_json::json!({ "status": status, "resetsAt": resets_at });
        if let limits::Gate::PausedUntil(_) = self.gate.pause_handle().saw(&told, now_unix()) {
            self.update(|book| run_stands_or_moves(book, true));
        }
    }

    /// Miejsce ze **wspólnej puli aplikacji**, i limit dostawcy w tej samej pętli.
    ///
    /// `None` znaczy „nie ruszaj tego kroku": bieg zatrzymał człowiek, zanim to miejsce się
    /// zwolniło. Nie jest to błąd i nie jest to `Err` (niezmiennik 7) — Stop, który mimo
    /// wszystko wpuszcza agenta po to, żeby go zaraz zabić, płaci dostawcy za turę, której
    /// nikt nie zobaczy.
    ///
    /// **Pętla, nie jedno pytanie**, bo odmowy są dwie różne i tylko jedna z nich mija sama:
    /// [`limits::Refusal::Paused`] wraca natychmiast i mówi „nie teraz", a czekanie na wolne
    /// miejsce siedzi już w środku [`limits::Run::dispatch`]. Bieg, który po odmowie zaczeka na
    /// miejsce w puli, trzymałby zasób potrzebny komuś, kto może biec, i zajmował go przez całe
    /// pięciogodzinne okno limitu.
    async fn a_slot_for_this_step(&self, cancel: &CancellationToken) -> Option<limits::Slot> {
        // Czy ten krok kiedykolwiek odbił się od limitu. Tylko taki krok ma prawo ogłosić, że
        // bieg rusza dalej: inaczej każdy zwykły krok pisałby `running` po biegu, który stoi
        // z zupełnie innego powodu.
        let mut waited = false;
        loop {
            let asked = tokio::select! {
                // `biased`, żeby Stop wygrywał z miejscem zwalnianym w tej samej chwili: krok,
                // który po Stopie dostaje permit, startuje agenta i zabija go w następnej
                // linijce.
                biased;
                () = cancel.cancelled() => return None,
                asked = self.gate.dispatch() => asked,
            };
            match asked {
                limits::Dispatch::Granted(slot) => {
                    if waited {
                        // Bieg ruszył dalej SAM, o `resetsAt`, i nikt nie musiał nic nacisnąć.
                        // Pytamy bramę jeszcze raz zamiast wpisać `false`: druga linia limitu
                        // mogła wejść, kiedy spaliśmy, i wtedy bieg dalej stoi.
                        let still = !self.gate.still_paused_for().is_zero();
                        self.update(|book| run_stands_or_moves(book, still));
                    }
                    return Some(slot);
                }
                limits::Dispatch::Refused(limits::Refusal::Paused) => {
                    waited = true;
                    // Dokładnie do końca pauzy i ani razu wcześniej. Wersja pytająca co sto
                    // milisekund budzi bieg trzy tysiące razy w pięciogodzinnym oknie, żeby
                    // 2999 razy usłyszeć to samo — a `resetsAt` jest znane od pierwszego
                    // zdarzenia i nikt go po drodze nie skraca (`engine::limits`).
                    let left = self.gate.still_paused_for();
                    tokio::select! {
                        biased;
                        () = cancel.cancelled() => return None,
                        () = tokio::time::sleep(left) => {}
                    }
                }
            }
        }
    }

    /// Krok agenta: sterownik, zdarzenia, linie, koniec albo anulowanie.
    async fn run_agent(
        self: &Arc<Self>,
        id: StepId,
        job: &AgentJob,
        cancel: &CancellationToken,
    ) -> StepReport {
        let (events, inbox) = mpsc::channel::<DecodedEvent>(EVENT_QUEUE);
        // Odbiór staje PRZED startem sterownika: vendor ma prawo powiedzieć pierwsze zdarzenia
        // jeszcze w `start`, a kanał bez odbiorcy zatrzymałby go na pierwszym pełnym buforze.
        // Limit dostawcy przychodzi właśnie tędy, więc pętla dostaje CAŁY bieg, nie same linie.
        let pump = tokio::spawn(forward(
            Arc::clone(self),
            inbox,
            self.plan.steps[id].name.clone(),
        ));
        // Własny klon nadawcy zostaje po to, żeby o nieudanym starcie dało się powiedzieć tą samą
        // drogą, którą mówi agent. Musi zginąć na OBU gałęziach — nadawca, który przeżył krok,
        // trzyma kurator otwarty i `pump.await` niżej nie wróciłby nigdy.
        let ours = events.clone();

        // Prompt składamy TERAZ, a nie przy planowaniu: indeks przekazań ma co wymienić dopiero
        // wtedy, gdy poprzednicy zeszli, a przy planowaniu nie ruszył jeszcze nikt.
        let Told {
            prompt,
            reads,
            extra_dirs,
        } = self.prompt_for(id, &job.prompt);

        let spec = RunSpec {
            run_id: job.session,
            cwd: job.cwd.clone(),
            // Instrukcja i indeks jadą jako DANE. Ta warstwa nie skleja komendy i nie zna ani
            // jednej flagi vendora (niezmiennik 9).
            prompt,
            model: job.model.clone(),
            system_append: job.system_append.clone(),
            policy: job.policy,
            // Katalog przekazań, kiedy krok ma co czytać. Odnośnik do pliku, którego agentowi nie
            // wolno otworzyć, jest odnośnikiem bez handlera (niezmiennik 16).
            extra_dirs,
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
                self.one_turn(id, handle, cancel, &reads).await
            }
            Err(error) => {
                let text = format!("Loadout could not start this agent: {error}");
                // `.into()` — `DecodedEvent::from(AgentEvent)` podstawia `tool: None`. Nieudany
                // start nie jest czynnością narzędzia, więc brak faktu jest tu prawdą, nie luką.
                let _ = ours
                    .send(AgentEvent::Notice { text: text.clone() }.into())
                    .await;
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

    /// Jedna tura agenta: czekaj na koniec albo na Stop, a udany wynik oddaj następnym.
    ///
    /// `reads` jest listą tego, co Loadout wstrzyknął w prompt tej tury ([`Live::prompt_for`]),
    /// i jedzie prosto do front-mattera przekazania, które z niej powstanie.
    async fn one_turn(
        &self,
        id: StepId,
        mut handle: Box<dyn AgentHandle>,
        cancel: &CancellationToken,
        reads: &[String],
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

        /* GŁOS KROKU JEST DOSTĘPNY PRZEZ CAŁĄ TURĘ — i to jest cała naprawa „nie da się napisać
         * do agenta". Rejestrujemy pod NAZWĄ kroku, bo to ją człowiek widzi w strumieniu i na
         * kafelku szyny; identyfikator wewnętrzny byłby kluczem, którego okno nigdy nie dostało.
         *
         * Zdejmujemy w `finally`-podobnym miejscu na końcu tej funkcji, nie w gałęzi sukcesu:
         * krok anulowany i krok po limicie czasu też przestają słuchać, a głos zostawiony po nich
         * proponowałby rozmowę z sesją, której nie ma. */
        if let Some(voice) = handle.voice() {
            self.control.step_can_hear(&self.plan.steps[id].name, voice);
        }

        // Limit czasu kroku. Zegar rusza TUTAJ, przy czekaniu na turę, a nie przy planowaniu:
        // krok czekający w kolejce na wolne miejsce nie zużywa niczyich pieniędzy, więc liczenie
        // mu tego czasu ubijałoby kroki tym częściej, im dłuższa kolejka.
        let limit = match &self.plan.steps[id].job {
            Job::Agent(job) => job.give_up_after,
            // Kafelek kontrolny czeka na CZŁOWIEKA i nie pali niczyich pieniędzy, więc limit
            // agenta go nie dotyczy. Ubijanie pytania po dwudziestu minutach byłoby karą za to,
            // że ktoś poszedł na obiad.
            Job::Ask { .. } => Duration::MAX,
        };

        let finished = {
            let waiting = handle.wait();
            tokio::pin!(waiting);
            let overdue = tokio::time::sleep(limit);
            tokio::pin!(overdue);
            tokio::select! {
                // `biased`, bo tura, która właśnie się skończyła, ma pierwszeństwo przed Stopem
                // wpadającym w tej samej chwili: zabijanie czegoś, co już zeszło, zamieniłoby
                // udany krok w anulowany zależnie od tego, który poll wypadł pierwszy. Z tego
                // samego powodu limit czasu stoi PO Stopie: człowiek, który nacisnął Stop
                // w ostatniej sekundzie, ma dostać „anulowane", a nie „przekroczony limit".
                biased;
                done = &mut waiting => Ended::Turn(done),
                () = cancel.cancelled() => Ended::Stopped,
                () = &mut overdue => Ended::Overdue,
            }
            // Pożyczka `handle` kończy się razem z tym blokiem — dopiero po nim wolno zawołać
            // `cancel()` albo `close()` na tym samym uchwycie.
        };

        /* Głos zdejmujemy PO tym `match`, nie w gałęziach: krok anulowany i krok po limicie czasu
         * też przestają słuchać, a `report` jest jedynym miejscem, przez które przechodzą
         * wszystkie trzy drogi wyjścia. */
        let report = match finished {
            // PRZEKROCZONY LIMIT IDZIE TĄ SAMĄ DROGĄ, CO STOP: przez sterownik.
            //
            // `tokio::time::timeout` owinięty wokół `handle.wait()` wygląda identycznie i jest
            // o trzy linijki tańszy — i jest błędem, przed którym stoi niezmiennik 10: anuluje
            // ZADANIE RUSTA, a proces systemowy zostaje żywy i pali limit u dostawcy do końca
            // świata. Dlatego tutaj wołamy `cancel()` i pytamy o DOWÓD zejścia grupy.
            Ended::Overdue => {
                let proof = handle.cancel().await;
                let unproven = matches!(proof, GroupProof::Alive);
                self.update(|book| {
                    let step = &mut book.steps[id];
                    // Powód nazywa LIMIT CZASU, nie „coś poszło nie tak". Człowiek ma stąd
                    // wiedzieć, że to była nasza decyzja i którą liczbę zmienić, żeby jej nie
                    // było — inaczej szuka wady w agencie, którego nikt nie zepsuł.
                    step.error = Some(if unproven {
                        format!(
                            "This step ran longer than its {} minute limit, and Loadout could \
                             not make sure the agent stopped, so it may still be running.",
                            limit.as_secs() / 60
                        )
                    } else {
                        format!(
                            "This step ran longer than its {} minute limit, so Loadout stopped \
                             it. Give it more minutes in the agent, or split the work.",
                            limit.as_secs() / 60
                        )
                    });
                });
                StepReport::Failed
            }
            // ANULOWANIE IDZIE PRZEZ STEROWNIK, nie przez zdjęcie zadania Rusta. `tokio::time::
            // timeout` wokół kroku wygląda tak samo i jest o linijkę tańszy — i zostawia żywą
            // grupę procesów palącą limit u dostawcy (niezmienniki 6 i 10).
            Ended::Stopped => {
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
            Ended::Turn(Err(error)) => {
                self.update(|book| book.steps[id].error = Some(error.to_string()));
                StepReport::Failed
            }
            Ended::Turn(Ok(turn)) => {
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
                    // Przekazanie schodzi na dysk PRZED powrotem z tury, i to jest cały warunek
                    // poprawności tego szwu: stopień wejściowy potomkom zdejmuje planista dopiero
                    // po tym powrocie (`engine::scheduler`). Zapis dopisany za `run_agent`
                    // otwierałby okno, w którym następny krok już wystartował, a jego prompt nie
                    // ma jeszcze czego wymienić — i wyglądałoby to na przekazanie gubione raz na
                    // sto biegów, czyli na wyścig, którego nikt nie umie powtórzyć.
                    //
                    // Tylko po **udanym** kroku: agent, który wyszedł błędem, nie oddał wyniku,
                    // a plik z jego ostatnim zdaniem czytałby się jak wynik. Powód porażki jedzie
                    // do księgi i na ekran, tamtą drogą.
                    self.hand_over(id, &turn.text, reads);
                    StepReport::Succeeded
                } else {
                    StepReport::Failed
                }
            }
        };
        self.control.step_went_quiet(&self.plan.steps[id].name);
        report
    }

    /// Prompt kroku: jego **własna instrukcja** plus indeks przekazań poprzedników.
    ///
    /// Instrukcja stoi pierwsza i jest w prompcie zawsze. Prompt złożony z samych cudzych wyników
    /// oddaje agentowi pracę wszystkich pozostałych i ani jednego zdania o tym, co ma z nią
    /// zrobić.
    ///
    /// Indeks jest **listą ścieżek**, nigdy treścią (D6 punkt 5, nagłówek modułu). Krok bez
    /// poprzedników dostaje swoją instrukcję i nic więcej: pusty nagłówek „steps before this one"
    /// nad zerem wpisów jest zdaniem o niczym, a agent przeczyta go jako zgubione wejście.
    fn prompt_for(&self, id: StepId, instructions: &str) -> Told {
        let handed = self.handed_before(id);
        let mut told = Told {
            prompt: instructions.to_owned(),
            reads: Vec::with_capacity(handed.len()),
            extra_dirs: Vec::new(),
        };
        if handed.is_empty() {
            return told;
        }

        told.prompt.push_str("\n\n");
        told.prompt.push_str(HANDOFF_INDEX_OPENS);
        for hand in &handed {
            // `write!` do `String`, nie `push_str(&format!(…))`: ten drugi alokuje bufor
            // pośredni tylko po to, żeby go zaraz skopiować i wyrzucić (clippy
            // `format_push_string`). Zapis do `String` jest nieomylny — `fmt::Error` może
            // zwrócić tylko sam formatter — więc wynik idzie do `let _`, a nie do `expect()`,
            // który w tym drzewie jest `warn`, czyli pod `-D warnings` też fatalny.
            let _ = write!(told.prompt, "\n- {}: {}", hand.from, hand.path.display());
            told.reads.push(self.filed_as(&hand.path));
            // Jeden katalog na cały bieg, więc pętla dopisuje go raz — ale bierze go ze ścieżki,
            // a nie ze stałej: druga kopia nazwy `handoffs` byłaby drugim miejscem do poprawienia
            // w dniu, w którym `memory::handoff` zmieni nazwę katalogu, i tym niepoprawionym.
            if let Some(dir) = hand.path.parent()
                && !told.extra_dirs.iter().any(|had| had == dir)
            {
                told.extra_dirs.push(dir.to_owned());
            }
        }
        told.prompt.push_str("\n\n");
        told.prompt.push_str(HANDOFF_INDEX_CLOSES);
        told.prompt.push('\n');
        told
    }

    /// Przekazania kroków, po których idzie ten krok — **w kolejności z grafu**.
    ///
    /// Kolejnością jest pozycja kroku w pliku workflow, i to nie jest wybór kosmetyczny: druga
    /// możliwa kolejność — chwila zakończenia — zmienia się z biegu na bieg, bo zależy od tego,
    /// który agent akurat odpowiedział szybciej. Prompt, który dwa razy z rzędu wygląda inaczej,
    /// jest promptem, którego nie da się z niczym porównać, a przy trzech poprzednikach to
    /// przestaje być teorią. Pozycja w pliku jest przy tym tą samą liczbą, którą niesie prefiks
    /// nazwy pliku przekazania i wiersz w `run.json`, więc indeks w prompcie i `ls handoffs/`
    /// czyta się w jednym porządku.
    ///
    /// Poprzednik, który nic nie oddał, **wypada z listy**: kafelek kontrolny nie oddaje nigdy,
    /// a wpis bez pliku byłby ścieżką, której agent nie ma jak otworzyć.
    fn handed_before(&self, id: StepId) -> Vec<Handed> {
        let before = ends(&self.plan.arrows, |&(parent, child)| {
            (child == id).then_some(parent)
        });
        // Zamek żyje w jednym wyrażeniu i nie ma w nim ani jednego `await` (niezmiennik 8).
        let filed: Vec<Option<PathBuf>> = {
            let handoffs = self.handoffs.lock().unwrap_or_else(PoisonError::into_inner);
            before
                .iter()
                .map(|&step| handoffs.get(step).cloned().flatten())
                .collect()
        };

        before
            .into_iter()
            .zip(filed)
            .filter_map(|(step, path)| {
                Some(Handed {
                    from: self.plan.steps.get(step)?.name.clone(),
                    path: path?,
                })
            })
            .collect()
    }

    /// Ścieżka przekazania widziana **z katalogu biegu**, czyli tak, jak zapisuje ją plik.
    ///
    /// Bezwzględna ścieżka spoza tego katalogu byłaby w `reads` zapisem prawdziwym dokładnie do
    /// pierwszego przeniesienia katalogu biegu; nazwa pliku sama w sobie zostaje, kiedy `dir`
    /// nie jest przedrostkiem — a to znaczy, że przekazanie leży gdzieś, gdzie tego biegu nie ma.
    fn filed_as(&self, path: &Path) -> String {
        path.strip_prefix(&self.plan.dir)
            .unwrap_or(path)
            .display()
            .to_string()
    }

    /// Odnotowuje, gdzie leży przekazanie tego kroku.
    ///
    /// Zamek powstaje i ginie w jednym wyrażeniu, bez `await` w środku (niezmiennik 8).
    fn filed(&self, id: StepId, path: PathBuf) {
        if let Some(slot) = self
            .handoffs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(id)
        {
            *slot = Some(path);
        }
    }

    /// Wynik kroku → plik przekazania w `handoffs/`.
    ///
    /// **Front-matter składa Loadout, ciałem jest to, co oddał agent** (`ARCHITECTURE` §8,
    /// [T6 §10.2]). Ciała nie parsujemy i nie czyścimy: blok `---`, który model wkleił do swojej
    /// odpowiedzi, ma zostać tam, gdzie go postawił, bo jest jedynym śladem próby, na którą
    /// człowiek może zareagować. Wszystkie siedem pól niżej pochodzi z pliku workflow albo z tego
    /// biegu — ani jedno z tekstu, który przyszedł od modelu.
    ///
    /// Nieudany zapis **loguje się i nie przewraca kroku**, tą samą decyzją, co zrzut `run.json`
    /// w locie ([`Live::update`]): tura jest już zapłacona, a jej wynik jest dalej prawdziwy.
    /// Cena tej decyzji stoi w dzienniku wprost — następny krok dostaje wtedy prompt bez tego
    /// odnośnika.
    fn hand_over(&self, id: StepId, said: &str, reads: &[String]) {
        let step = &self.plan.steps[id];
        let draft = MetaDraft {
            run: self.plan.id.clone(),
            // Numer Loadouta, ten sam, którym ten krok nazywa się w `run.json` i w
            // `RunReport::steps`. Liczenie od jedynki byłoby drugą numeracją kroków, żyjącą
            // wyłącznie w nazwach plików — a wtedy `handoffs/03__…` i czwarty wiersz na ekranie
            // są tym samym krokiem tylko dla kogoś, kto zna przesunięcie.
            step: u32::try_from(id).unwrap_or(u32::MAX),
            from: step.name.clone(),
            to: ends(&self.plan.arrows, |&(parent, child)| {
                (parent == id).then_some(child)
            })
            .into_iter()
            .filter_map(|child| self.plan.steps.get(child).map(|step| step.name.clone()))
            .collect(),
            // Jeden rodzaj dla każdego kroku, i to jest niezmiennik 27 zapisany w danych: silnik
            // nie zna pojęcia „recenzja", więc nie ma jak nazwać wyniku kroku inaczej dlatego,
            // że ten krok recenzował. `findings` jest tym, czym `docs/ARCHITECTURE.md` §8 nazywa
            // wynik kroku w swoim własnym przykładzie (`02__research-auth__findings.md`).
            kind: Kind::Findings,
            title: title_of(step),
            reads: reads.to_vec(),
        };

        match handoff::write_handoff(&self.plan.dir, draft, said) {
            Ok(written) => {
                if !written.repaired.is_empty() || written.truncated {
                    // Licznik, który warto oglądać [T6 §11.1]: ile tur nie oddało umówionego
                    // kształtu i Loadout musiał go dopisać.
                    tracing::debug!(
                        run = %self.plan.id,
                        step = id,
                        repaired = written.repaired.len(),
                        truncated = written.truncated,
                        "the body of this handoff had to be reshaped"
                    );
                }
                self.filed(id, written.path);
            }
            Err(error) => tracing::error!(
                run = %self.plan.id,
                step = id,
                %error,
                "this step's result could not be handed over, so the next step is not told about it"
            ),
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
        self.update(|book| {
            book.asking = true;
            // `true`, bo pytanie stoi tu i teraz; drugi powód czyta [`run_stands_or_moves`].
            run_stands_or_moves(book, book.asking);
        });
        self.ask(id, question);

        if listening.wait().await {
            self.control.resume();
            /* ODPOWIEDŹ CZŁOWIEKA STAJE SIĘ PRZEKAZANIEM TEGO KROKU.
             *
             * To jest jedyne uczciwe miejsce, w które może pójść: kafelek kontrolny nie woła
             * żadnego agenta, więc nie ma komu jej „wysłać" — a krok, który idzie PO nim, i tak
             * czyta przekazania swoich rodziców (`hand_over`, indeks przekazań w prompcie).
             * Zdanie człowieka wchodzi więc do pracy tą samą drogą, którą wchodzi wynik agenta,
             * i widać je w `handoffs/` razem z resztą biegu (niezmiennik 4: pliki są prawdą).
             *
             * Nic nie piszemy, kiedy człowiek nie napisał nic: puste przekazanie dołożyłoby
             * do promptu następnego kroku nagłówek nad pustką, czyli kosztowałoby długość
             * za informację, której nie ma. */
            if let Some(said) = self.control.take_answer() {
                self.hand_over(id, &said, &[]);
            }
            // Limit dostawcy mógł wejść W TRAKCIE pytania i wtedy odpowiedź człowieka nie
            // wznawia niczego: bieg dalej stoi, tylko już z innego powodu.
            let still = !self.gate.still_paused_for().is_zero();
            self.update(|book| {
                book.asking = false;
                run_stands_or_moves(book, still);
            });
            StepReport::Succeeded
        } else {
            self.control.resume();
            // Stop przy pytaniu. Krok jest `cancelled`, a jego potomkowie też — nie `skipped`,
            // bo nikt nie padł: człowiek powiedział stop (ARCHITECTURE §5). Statusu biegu nie
            // ruszamy, bo bieg nie rusza dalej — zamyka go `close_the_book`. Gaśnie samo
            // `asking`, żeby krok kończący się obok nie ogłosił pauzy, której już nie ma.
            self.update(|book| book.asking = false);
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

/// Bieg stoi albo idzie — **wyliczone z obu powodów naraz**, nigdy wpisane z jednego.
///
/// Powody są dwa i mijają niezależnie: pytanie do człowieka ([`Live::wait_for_a_person`]) i limit
/// dostawcy ([`Live::the_provider_said_wait`]). Dwa bezwarunkowe przypisania do `status` kasują
/// się nawzajem — ten powód, który skończył pierwszy, ogłasza bieg jako idący, choć drugi wciąż
/// go trzyma, i na ekranie wygląda to jak bieg, który wysyła do zamkniętego okna.
///
/// Wolno to wołać **wyłącznie w trakcie biegu**: stany końcowe wpisuje [`Live::close_the_book`]
/// i nic po nim nie pyta już o to, czy bieg idzie.
fn run_stands_or_moves(book: &mut Book, paused_by_the_provider: bool) {
    book.status = if book.asking || paused_by_the_provider {
        RunState::Paused
    } else {
        RunState::Running
    };
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
/// 2026-08-17 (T-31) — pętla dostaje CAŁY bieg, a nie same linie, i to jest cała różnica między
/// „widać banner" a „bieg umie się wznowić o właściwej godzinie". `AgentEvent::RateLimit`
/// docierał tu i zostawał wierszem na ekranie, a wysyłka szła dalej, jakby nic nie zaszło —
/// czyli następny agent dostawał odmowę, a okno limitu paliło się na odmowach.
async fn forward(live: Arc<Live>, mut inbox: mpsc::Receiver<DecodedEvent>, agent: String) {
    let mut curator = Curator::new();
    /* Czy okno już wie, że ten agent myśli. Powód, dla którego to jest tu, a nie w kuratorze,
     * stoi przy wysyłce niżej. */
    let mut told_it_thinks = false;
    // 2026-08-18 — PACZKA NIESIE TERAZ FAKT O NARZEDZIU, nie samo zdarzenie. Do tego dnia
    // kanal sterownika mial typ `Sender<AgentEvent>`, wiec `Tool` — rodzina czynnosci, pelna
    // sciezka, pelne wyjscie — ginal na granicy sterownika, a ta petla musiala podac
    // `tool: None`. Skutkiem nie byla gorsza jakosc wiersza: `Curator` bez `seen.tool` zwraca
    // `Vec::new()`, wiec wiersze `read`, `search`, `edit` i `ran` NIE POWSTAWALY NIGDY
    // i strumien pokazywal wylacznie proze agenta. Powod, dla ktorego naprawa nalezy tutaj,
    // a nie do drugiej tabeli nazw narzedzi w tym pliku, stoi przy `engine::drivers::DecodedEvent`
    // (niezmienniki 15 i 23).
    while let Some(DecodedEvent { event, tool }) = inbox.recv().await {
        // PRZED kuracją, nie po niej: wiersz jest zdaniem dla człowieka, a to niżej jest
        // decyzją dla biegu. Kolejność odwrotna dokłada okno, w którym ekran już wie, a bieg
        // jeszcze wysyła.
        if let AgentEvent::RateLimit {
            status, resets_at, ..
        } = &event
        {
            live.the_provider_said_wait(status, *resets_at);
        }
        let at_ms = u64::try_from(live.began.elapsed().as_millis()).unwrap_or(u64::MAX);
        let seen = Seen {
            agent: &agent,
            at_ms,
            event: &event,
            tool: tool.as_ref(),
        };
        let batch = curator.observe(seen);

        /* SLOT „Thinking…" DOSTAJE SWÓJ NOŚNIK — i to jest jedyne miejsce, w którym wolno mu
         * go dostać.
         *
         * 2026-08-18. `docs/ARCHITECTURE.md` linia 178 daje dla `thinking` i `thinking_tokens`
         * wprost: „*nic w strumieniu* — stały slot na dole, nadpisywany". Kolumna tej tabeli
         * nazywa się „Co widać", więc zdanie mówi dwie rzeczy naraz: żadnego wiersza HISTORII,
         * ale slot ma pokazywać. Do dziś pokazywał nic: jedynym śladem myślenia był
         * `Curator::status`, którego w produkcji **nikt nie czytał**, więc dolna strefa ekranu
         * była martwa także wtedy, gdy agent myślał minutami.
         *
         * DLACZEGO TU, A NIE W KURATORZE. Próba odwrotna — `Curator::observe` oddające
         * `vec![Line::Thinking]` — przewróciła CZTERY kryteria w dwóch plikach, z których jedno
         * przepuszcza prawdziwą pompę przez złotą fiksturę szesnastu zdarzeń i żąda dokładnie
         * trzech wierszy. I miały rację: wektor kuratora JEST strumieniem historii, więc wiersz
         * w nim jest wierszem w historii. Tutaj nie jest: to jest osobna wysyłka obok kuracji,
         * a rejestr po stronie okna kieruje ten rodzaj na trasę `now`, gdzie widok go
         * NADPISUJE, nigdy nie dokłada (`src/sections/run/feed/model.ts`, gałąź `route === 'now'`
         * robi `continue`).
         *
         * TYLKO NA ZMIANĘ STANU i tylko wtedy, gdy kuracja nie oddała ani jednego wiersza.
         * Wiersz na każde zdarzenie myślenia to cztery wiadomości na turę przez pompę za jeden
         * fakt; wiersz wysłany RAZEM z prawdziwym wierszem zapalałby slot w tej samej paczce,
         * w której widok go gasi (prawdziwa linia gasi slot — [T2 §7.2 wiersz 4]). Gaśnięcia
         * nie wysyłamy wcale: robi je okno, na pierwszej prawdziwej linii, i to jest jego jedna
         * odpowiedź na to pytanie (niezmiennik 13). */
        if batch.is_empty() {
            if !told_it_thinks && curator.status() == Some(Status::Thinking) {
                let _ = live.lines.send(Line::Thinking {
                    agent: agent.clone(),
                });
                told_it_thinks = true;
            }
        } else {
            told_it_thinks = false;
        }

        send_batch(&live.lines, batch);
    }
    // Ostatnia grupa sklejania wyszłaby inaczej nigdy, a użytkownik zobaczyłby o wiersz mniej,
    // niż się wydarzyło — najgorszy rodzaj zgubienia, bo cichy.
    send_batch(&live.lines, curator.flush());
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
    one_line(text, SUMMARY_LIMIT)
}

/// Tytuł przekazania: **to, o co poproszono ten krok**, w jednym wierszu.
///
/// Z pliku workflow, nigdy z odpowiedzi modelu. `title` jest polem front-mattera, a te pisze
/// Loadout (`ARCHITECTURE` §8): tytuł wzięty z ciała oddawałby zdanie „co to za przekazanie" temu,
/// kto ma najwięcej do zyskania na tym, żeby ono dobrze brzmiało. Kafelek bez własnego zdania mówi
/// swoją nazwą — ona też jest zdaniem, które napisał człowiek.
fn title_of(step: &Planned) -> String {
    match &step.job {
        Job::Agent(job) => one_line(&job.prompt, TITLE_LIMIT),
        Job::Ask { question } => question
            .as_deref()
            .and_then(|question| one_line(question, TITLE_LIMIT)),
    }
    .unwrap_or_else(|| step.name.clone())
}

/// Tekst zwinięty do jednej linii i przycięty do `limit` bajtów, po granicy znaku. `None`, kiedy
/// nie zostało ani jedno słowo.
///
/// Jedna pętla na oba wywołania: podsumowanie kroku i tytuł przekazania różnią się wyłącznie
/// limitem, a druga kopia byłaby tą, która kiedyś zacznie ciąć w środku znaku — i przewróci bieg
/// na pierwszym emoji w odpowiedzi agenta.
fn one_line(text: &str, limit: usize) -> Option<String> {
    let line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.is_empty() {
        return None;
    }
    let mut end = line.len().min(limit);
    while end > 0 && !line.is_char_boundary(end) {
        end -= 1;
    }
    Some(line[..end].to_owned())
}

/// Numery kroków po drugiej stronie strzałek — **rosnąco i każdy raz**.
///
/// Rosnąco, czyli w kolejności z pliku workflow: to jedyny porządek, o którym nie decyduje
/// przypadek (patrz [`Live::handed_before`]). Bez powtórzeń, bo dwie strzałki między tą samą parą
/// kroków są w pliku legalne, a wpis wymieniony dwa razy każe krokowi zapłacić tokenami za tę samą
/// pracę dwa razy — i podwaja jedną z dwóch stron, którą krok syntezujący ma zważyć.
fn ends(
    arrows: &[(StepId, StepId)],
    pick: impl Fn(&(StepId, StepId)) -> Option<StepId>,
) -> Vec<StepId> {
    let mut out: Vec<StepId> = arrows.iter().filter_map(pick).collect();
    out.sort_unstable();
    out.dedup();
    out
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
    /// Kiedy wstała maszyna, na której ten bieg ruszył. Czyta to `store::rebuild` i po nim
    /// odzyskiwanie po awarii decyduje, czy wolno sprzątnąć zapisaną grupę procesów.
    #[serde(skip_serializing_if = "Option::is_none")]
    boot_id: Option<&'a str>,
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

/// Sekundy epoki — **ta sama jednostka, w której jedzie `resetsAt`** `[T7 §7.2, V]`.
///
/// Liczona z [`now_ms`], żeby bieg miał jeden zegar: druga droga do `SystemTime` znaczyłaby dwa
/// miejsca do poprawienia, kiedy zegar wymaga poprawki, i jedno z nich zostałoby stare. Ta sama
/// liczba w milisekundach mówi `duration_until_reset`, że limit wraca za 300 000 sekund — a to
/// wygląda na usterkę zegara, nie na pomyloną jednostkę, więc szuka się tego godzinami.
fn now_unix() -> i64 {
    now_ms() / 1_000
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

#[cfg(test)]
mod tests {
    //! Zadanie z wiersza wejścia wchodzi w prompt kroku — trzy przypadki, jeden na każdą drogę.
    //!
    //! # Dlaczego testy jednostkowe W TYM PLIKU, a nie w `tests/it/`
    //!
    //! [`with_the_task`] jest prywatna, a droga do niej z zewnątrz prowadzi przez `plan_run`, czyli
    //! przez katalog biblioteki, plik workflow, definicję agenta i fabrykę sterowników. Test tej
    //! jednej funkcji przez tamto stanowisko kosztowałby sto linii fikstury, żeby sprawdzić
    //! sklejanie dwóch napisów — a zapora, której koszt sprawdzenia jest wyższy niż koszt
    //! napisania, jest zaporą niesprawdzoną. Ten sam precedens i to samo uzasadnienie stoi przy
    //! `run_request` w `src-tauri/src/ipc.rs`.
    //!
    //! # Słaba wersja tych kryteriów
    //!
    //! `assert!(out.contains(TASK))`. Przechodzi dla implementacji, która **zawsze** dokleja
    //! nagłówek, także przy pustym zadaniu — czyli dla tej, która każdemu biegowi bez zadania
    //! dopisuje do promptu nagłówek nad pustką i każe za niego płacić długością. Rozstrzyga
    //! porównanie CO DO BAJTU w przypadku pustym.

    use super::{TASK_MARK, with_the_task};

    /// Prompt kroku, taki jak w pliku workflow.
    const STEP: &str = "Write the tests first, then the code.";

    /// Zadanie z wiersza wejścia.
    const TASK: &str = "build a pretty todo list";

    #[test]
    fn no_task_leaves_the_step_prompt_byte_for_byte() {
        assert_eq!(
            with_the_task("", STEP),
            STEP,
            "a run started without a task has to send the step's prompt exactly as the file has \
             it. An empty heading above nothing teaches the model that the section is sometimes \
             empty, and costs length for nothing"
        );
    }

    #[test]
    fn a_task_goes_where_the_file_pointed() {
        let pointed = format!("Context: {TASK_MARK}\n\nWrite the tests first.");
        assert_eq!(
            with_the_task(TASK, &pointed),
            format!("Context: {TASK}\n\nWrite the tests first."),
            "a file that took the trouble to mark the spot knows more about its own prompt than \
             we do, so the task belongs exactly there and nowhere else"
        );
    }

    #[test]
    fn without_a_mark_the_task_goes_on_top_under_a_heading() {
        let out = with_the_task(TASK, STEP);
        assert!(
            out.ends_with(STEP),
            "the step's own prompt has to stay whole and stay last: it usually ends with what to \
             hand back, and a sentence pasted after that reads like a note after the signature. \
             What came out was:\n{out}"
        );
        assert!(
            out.starts_with("What the person asked for"),
            "without a mark the task goes on top, named — an unlabelled sentence glued to a \
             prompt is indistinguishable from a typo in the file. What came out was:\n{out}"
        );
        assert!(
            out.contains(TASK),
            "and it has to actually carry the task. What came out was:\n{out}"
        );
    }

    #[test]
    fn a_mark_left_over_without_a_task_disappears() {
        let pointed = format!("Context: {TASK_MARK}\n\nGo.");
        assert_eq!(
            with_the_task("", &pointed),
            "Context: \n\nGo.",
            "`{{task}}` left in the text is the one thing in the whole prompt the model cannot \
             read as anything but a literal brace — it looks like the broken substitution it is"
        );
    }
}
