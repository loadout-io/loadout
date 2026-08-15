//! Planista: zbiór gotowych (Kahn) + `JoinSet` + `Semaphore` + `CancellationToken`.
//!
//! Kształt pętli z [T7 §2.3] w jednym zdaniu: **zbiór gotowych rządzi zależnościami, semafor
//! rządzi zasobami.** Te dwie rzeczy są niezależne i właśnie dlatego kod zostaje mały.
//!
//! **Permit bierzemy WEWNĄTRZ zadania z `JoinSet`, nigdy w pętli wysyłki** (niezmiennik 11).
//! Wersja z permitem w pętli przechodzi każdy test na górne ograniczenie (`peak <= limit`),
//! a po cichu kasuje rozróżnienie `ready` / `running` — i to jest dokładnie defekt poprzedniego prototypu,
//! gdzie `max_parallel` było tylko szerokością wysyłki: jeden worker, cztery „równoległe" pasy
//! w rozłącznych oknach po ~0,5 s, i **ani jednej sekundy, w której działały dwa agenty**.
//!
//! **Niezmiennik 27:** w tym pliku nie ma prawa istnieć `if review_enabled` ani żaden inny
//! warunek nazywający etap biegu. Kolejność mieszka wyłącznie w grafie; krok z agentem-
//! recenzentem jest tu zwykłym krokiem i niczym więcej (decyzja D7).

use std::future::Future;

use tokio_util::sync::CancellationToken;

use super::dag::Dag;
use super::step::{StepReport, StepState};
use super::StepId;

/// Wynik całego biegu.
///
/// **Wartość, nie `Result`** (niezmiennik 7): anulowanie jest jednym z normalnych zakończeń
/// biegu, więc `execute` nie ma jak zwrócić `Err(Cancelled)` — i o to chodzi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// Stan końcowy każdego węzła, indeksowany numerem kroku. Po powrocie z [`execute`] nie
    /// ma tu prawa zostać `Pending`, `Ready` ani `Running`: każdy węzeł jest rozstrzygnięty.
    pub states: Vec<StepState>,
    /// Czy bieg został anulowany. Osobne pole, bo bieg złożony z samych `Skipped` po awarii
    /// i bieg zatrzymany przez człowieka to dwie różne historie dla UI.
    pub cancelled: bool,
}

/// Wykonuje graf i zwraca stan końcowy każdego węzła.
///
/// `limit` to liczba kroków, które **naprawdę** mogą działać naraz. `cancel` jest tokenem
/// **tego** biegu — nigdy globalnym `AtomicBool`: bool przecieka między biegami, więc drugi
/// bieg po anulowanym startuje jako już anulowany i kończy się w milisekundach z samymi
/// `Cancelled`, co wygląda jak szybki bieg, a nie jak awaria.
///
/// `run_step` dostaje ten token **do środka**. To nie jest wygoda, tylko warunek konieczny:
/// zdjęcie zadania Rusta (`JoinSet::abort_all`) zostawia po drugiej stronie żywy proces
/// systemowy, który dalej pali limit u dostawcy [T7 §3.1]. Krok musi zobaczyć anulowanie sam,
/// żeby móc zejść po swoim procesie — w T-03 przez eskalację SIGTERM → SIGKILL.
pub async fn execute<F, Fut>(
    dag: &Dag,
    limit: usize,
    cancel: CancellationToken,
    run_step: F,
) -> Outcome
where
    F: Fn(StepId, CancellationToken) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = StepReport> + Send + 'static,
{
    // SZKIELET (2026-08-15) — świadomie zła odpowiedź: nie woła `run_step` ani razu i zostawia
    // każdy węzeł w stanie, w którym go zastał. `Pending` jest tu wybrane dlatego, że jest
    // NIELEGALNYM stanem końcowym: AC-6 asertuje wprost, że w zwróconym wektorze nie ma
    // `Pending`, `Ready` ani `Running`, więc na tym stubie nie da się przejść nawet przypadkiem.
    //
    // Implementacja [T7 §2.3]: kopia stopni wejściowych, zbiór gotowych, `JoinSet`,
    // `Arc<Semaphore>` z `limit.max(1)`, `select!` z `biased;` (anulowanie ma wygrywać remis
    // z krokiem kończącym się w tym samym obrocie) i przejście po stożku w dół z POWODEM:
    // pod `Failed` idzie `Skipped`, pod `Cancelled` idzie `Cancelled`.
    let _ = (limit, cancel, run_step);
    Outcome {
        states: vec![StepState::Pending; dag.len()],
        cancelled: false,
    }
}
