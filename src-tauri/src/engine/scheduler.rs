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
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use super::StepId;
use super::dag::Dag;
use super::step::{StepEvent, StepReport, StepState, next};

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
    let children = dag.children();
    // Kopia stopni wejściowych, nigdy sam graf: ten sam `Dag` ma dać się uruchomić drugi raz,
    // a AC-6 robi dokładnie to, żeby przyłapać stan przeciekający między biegami.
    let mut remaining = dag.in_degree();
    // 2026-08-15 — wektor stanów jest współdzielony, bo `Running` wpisuje ZADANIE, nie pętla:
    // dopiero ono wie, kiedy permit naprawdę jest w ręku (niezmiennik 11). Zamek jest
    // `std::sync::Mutex`, a każde jego wzięcie mieści się w jednym bloku bez `await`
    // (niezmiennik 8, `clippy::await_holding_lock` = deny).
    //
    // Wyścigu tu nie ma i nie jest to przypadek: pętla wpisuje stan terminalny kroku dopiero
    // po jego `join_next`, czyli po zakończeniu zadania, a stożek nie sięga kroku wysłanego —
    // jego rodzice są `Succeeded`, z którego [`next`] nie ma wyjścia. Zamek jest tu po to,
    // żeby dzielić wektor, nie żeby ratować kolejność.
    let states = Arc::new(Mutex::new(vec![StepState::Pending; dag.len()]));
    let mut ready: Vec<StepId> = (0..dag.len()).filter(|&id| remaining[id] == 0).collect();

    // `limit.max(1)`: semafor bez ani jednego permitu nie przepuściłby nikogo, pętla skończyłaby
    // się przy `inflight == 0` w pierwszym obrocie i bieg zameldowałby koniec, w którym nic nie
    // biegło. Zero jest pomyłką wołającego, nie prośbą o zatrzymanie — od tego jest token.
    let semaphore = Arc::new(Semaphore::new(limit.max(1)));
    let mut running: JoinSet<(StepId, StepReport)> = JoinSet::new();
    let mut inflight = 0usize;

    loop {
        // 1. Wyślij wszystko, co gotowe. Wysyłka nie czeka na permit — od czekania jest zadanie.
        while let Some(id) = ready.pop() {
            if cancel.is_cancelled() {
                // Krok, który po Stopie nigdy nie wystartował, jest `cancelled`, nie `skipped`
                // [T7 §9.3]: nikt wyżej nie padł, użytkownik zatrzymał bieg.
                let mut guard = lock(&states);
                guard[id] = StepState::Cancelled;
                mark_cone(children, &mut guard, id, StepEvent::UpstreamCancelled);
            } else {
                // `Ready` znaczy „w kolejce, jeszcze bez permitu". Wysyłka kończy się tutaj;
                // `Running` dopisuje sobie samo zadanie, kiedy permit jest już w ręku.
                lock(&states)[id] = StepState::Ready;
                let semaphore = Arc::clone(&semaphore);
                let run_step = run_step.clone();
                let cancel = cancel.clone();
                let states = Arc::clone(&states);
                running.spawn(async move {
                    // 2026-08-15 — permit bierzemy TUTAJ, wewnątrz zadania [T7 §2.3], nigdy
                    // w pętli wysyłki. Wersja z permitem w pętli przechodzi każdy test na górne
                    // ograniczenie (`peak <= limit`), a po cichu kasuje różnicę między „czeka
                    // w kolejce" a „działa" — i to jest dokładnie defekt poprzedniego prototypu, gdzie
                    // `max_parallel` było wyłącznie szerokością wysyłki (niezmiennik 11).
                    let Ok(_permit) = semaphore.acquire_owned().await else {
                        // Semafor zamyka się dopiero razem z biegiem i nikt go tu nie zamyka.
                        // Gdyby jednak: krok nie ruszył, a bieg się kończy — to jest anulowanie,
                        // nie awaria kroku.
                        return (id, StepReport::Cancelled);
                    };
                    {
                        // 2026-08-15 — `(Ready, PermitAcquired) → Running` z tabeli
                        // `docs/ARCHITECTURE.md` §5, wpisane DOKŁADNIE tutaj: permit jest
                        // wzięty, więc krok naprawdę działa. Wpis w pętli wysyłki (przed
                        // permitem) pokazywałby zakolejkowany krok jako działający i kasował
                        // rozróżnienie, którego pilnuje niezmiennik 11 — czyli meldowałby
                        // dokładnie tę nieprawdę, przez którą poprzedni prototyp „miał" równoległość.
                        //
                        // Przez tabelę, nie przypisaniem wprost: krok, który zdążył już zejść
                        // (np. stożek po anulowaniu), zwraca stąd `None` i zostaje na swoim
                        // stanie terminalnym.
                        let mut guard = lock(&states);
                        if let Some(state) = next(guard[id], StepEvent::PermitAcquired) {
                            guard[id] = state;
                        }
                        // Guard ginie razem z tym blokiem, PRZED jedynym `await` w tym
                        // zadaniu (niezmiennik 8).
                    }
                    // Token idzie DO ŚRODKA kroku. Zdjęcie zadania z zewnątrz też wróciłoby
                    // szybko i też wyglądało na anulowane, ale w T-03 zostawia żywą grupę
                    // procesów palącą limit u dostawcy [T7 §3.1].
                    (id, run_step(id, cancel).await)
                });
                inflight += 1;
            }
        }

        if inflight == 0 {
            break;
        }

        // 2. Czekaj na pierwszy krok, który wróci. Nie ma tu `select!` z anulowaniem i to jest
        // decyzja: po Stopie czekamy, aż kroki zwiną się SAME, bo tylko one wiedzą, co mają po
        // sobie posprzątać. Wyścig z nimi to `abort_all` pod inną nazwą.
        let Some(joined) = running.join_next().await else {
            break;
        };
        inflight -= 1;

        let Ok((id, report)) = joined else {
            // Zadanie padło paniką, więc nie wróciło ze swoim numerem i nie da się go stąd
            // nazwać. Zostaje `Ready`, a zamiatanie za pętlą zamknie je razem ze stożkiem —
            // bieg nie ma prawa wrócić z krokiem bez rozstrzygnięcia.
            continue;
        };

        let mut guard = lock(&states);
        match report {
            StepReport::Succeeded => {
                guard[id] = StepState::Succeeded;
                for &child in &children[id] {
                    remaining[child] -= 1;
                    // Warunek na `Pending` jest tym, co daje „dokładnie raz": węzeł zamknięty
                    // wcześniej przez stożek nie wraca do zbioru gotowych.
                    if remaining[child] == 0 && guard[child] == StepState::Pending {
                        ready.push(child);
                    }
                }
            }
            StepReport::Failed => {
                guard[id] = StepState::Failed;
                mark_cone(children, &mut guard, id, StepEvent::UpstreamFailed);
            }
            StepReport::Cancelled => {
                guard[id] = StepState::Cancelled;
                mark_cone(children, &mut guard, id, StepEvent::UpstreamCancelled);
            }
        }
    }

    let cancelled = cancel.is_cancelled();
    // Pętla wyszła przy `inflight == 0`, więc żadne zadanie już nie żyje i nikt poza tym
    // wątkiem nie sięga po wektor.
    let mut guard = lock(&states);
    settle_leftovers(children, &mut guard, cancelled);

    Outcome {
        states: std::mem::take(&mut *guard),
        cancelled,
    }
}

/// Zamek na wspólnym wektorze stanów.
///
/// Zatrute zamki odplatamy, zamiast panikować: `panic!` w silniku zabiera cały bieg
/// (AGENTS.md §4), a wektor stanów po panice jednego kroku jest dalej poprawny — krok,
/// który nie wrócił, i tak domknie [`settle_leftovers`].
fn lock(states: &Mutex<Vec<StepState>>) -> MutexGuard<'_, Vec<StepState>> {
    states.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Schodzi stożkiem w dół od `from` i wpisuje POWÓD, dla którego te kroki się nie odbędą.
///
/// Reguła: **wygrywa powód, który wystąpił pierwszy; status terminalny nigdy nie jest
/// przepisywany.** Nie ma jej tutaj jako `if` — niesie ją tabela przejść, bo z każdego stanu
/// końcowego [`next`] zwraca `None`. To samo `None` jest strażnikiem odwiedzin: węzeł zmienia
/// stan najwyżej raz, więc stos ma koniec także na diamencie.
///
/// Bez rozróżnienia na powód wszystko poniżej anulowanego kroku meldowałoby `Skipped` i UI
/// tłumaczyłoby świadomy Stop jako cudzą awarię — defekt z [T7 §2.4], znaleziony testem.
fn mark_cone(children: &[Vec<StepId>], states: &mut [StepState], from: StepId, event: StepEvent) {
    let mut stack: Vec<StepId> = children[from].clone();
    while let Some(id) = stack.pop() {
        if let Some(reason) = next(states[id], event) {
            states[id] = reason;
            stack.extend_from_slice(&children[id]);
        }
    }
}

/// Zamyka kroki, których pętla nie zamknęła. Po powrocie z [`execute`] nie ma prawa zostać
/// `Pending`, `Ready` ani `Running`: krok, który po końcu biegu dalej czyta się jako działający,
/// to wiersz kręcący się w UI w nieskończoność.
///
/// W zdrowym biegu ta funkcja nic nie robi. Dochodzi do głosu, kiedy zadanie kroku padło paniką
/// i nie wróciło ze swoim numerem — wtedy krok kończy jako `Failed`, a jego stożek jako
/// `Skipped`, tak samo jak przy zwykłym niepowodzeniu.
fn settle_leftovers(children: &[Vec<StepId>], states: &mut [StepState], cancelled: bool) {
    let stalled: Vec<StepId> = (0..states.len())
        .filter(|&id| matches!(states[id], StepState::Ready | StepState::Running))
        .collect();
    for id in stalled {
        states[id] = StepState::Failed;
        mark_cone(children, states, id, StepEvent::UpstreamFailed);
    }

    // Co zostało w `Pending`, nigdy nie doczekało się rodziców.
    for state in &mut *states {
        if *state == StepState::Pending {
            *state = if cancelled {
                StepState::Cancelled
            } else {
                StepState::Skipped
            };
        }
    }
}
