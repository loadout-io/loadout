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
//! # Stan tego pliku: SZKIELET (2026-08-16)
//!
//! Ciała są `todo!()`. To jest wymagany kształt fazy, w której powstają kryteria: test ma się
//! skompilować i paść **w czasie wykonania, na braku ZACHOWANIA** — test, który się nie
//! kompiluje, niczego nie uruchomił (`AGENTS.md` §2a p. 5). Stub zwracający wartość oczekiwaną
//! byłby dokładnie tą awarią, której ta faza ma zapobiec.

use tokio::sync::mpsc;

use super::{Outcome, RunDeps, RunError, RunReport, RunRequest};
use crate::engine::line::Line;

/// Uruchamia workflow z pliku i wypuszcza jego linie na `lines`.
///
/// Kolejność: wczytaj → sprawdź → katalog biegu → migawka → planista → sterowniki → linie.
/// Odmowa przed pierwszym utworzonym katalogiem; szczegóły w nagłówku modułu.
///
/// `lines` jest zwykłym `tokio::sync::mpsc` i to jest granica zadania: sklejacz 16 ms / 2000
/// linii i adaptacja na `Channel` należą do T-07 (`docs/ARCHITECTURE.md` §4). Tutaj paczka
/// wychodzi wtedy, kiedy powstanie.
pub async fn run_workflow_inner(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    lines: mpsc::Sender<Vec<Line>>,
) -> Result<RunReport, RunError> {
    // SZKIELET: kanał zamykamy od razu, żeby wołający nie czekał na linie, których ta faza nie
    // produkuje. Całe to ciało zastępuje implementacja.
    drop(lines);
    todo!(
        "run {} at {} at a time, library {}, project {}",
        request.workflow.display(),
        request.how_many_at_once,
        deps.home.display(),
        deps.project.display()
    )
}

/// Zatrzymuje bieg i **wraca dopiero z dowodem**, że nic po nim nie żyje.
///
/// Zwraca [`Outcome::Cancelled`] jako wartość, nigdy `Err` (niezmiennik 7). `Ok(())` zaraz po
/// wysłaniu sygnału byłoby tym samym błędem, przed którym broni `GroupProof`: wołający
/// przeczytałby „nie żyje" tam, gdzie napisano „wysłałem SIGTERM" (niezmiennik 6).
pub async fn stop_run_inner(deps: &RunDeps<'_>) -> Result<Outcome, RunError> {
    todo!("stop the run in {}", deps.project.display())
}

/// Puszcza bieg dalej z punktu kontrolnego (T3 §6.1 reguła 5).
///
/// Punkt kontrolny zatrzymuje **bieg**, nie krok, i nic za nim nie startuje, dopóki człowiek nie
/// odpowie. Pytanie, które pojawia się na ekranie po tym, jak agent już zrobił swoje, nie jest
/// pytaniem.
pub async fn continue_run_inner(deps: &RunDeps<'_>) -> Result<(), RunError> {
    todo!("let the run in {} go on", deps.project.display())
}
