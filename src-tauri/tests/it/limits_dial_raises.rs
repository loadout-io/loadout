//! AC-1 dla T-21: podniesienie suwaka **w trakcie biegu** naprawdę zwiększa nakładanie się
//! agentów w czasie.
//!
//! To jest kryterium, którego to zadanie jest bezpośrednią realizacją (niezmiennik 11).
//! Poprzedni prototyp miał `max_parallel`, miał zielone testy i **nigdy nie uruchomił czterech agentów
//! naraz**: liczba była wyłącznie szerokością wysyłki — jeden worker, cztery „równoległe" pasy
//! w rozłącznych oknach po ~0,5 s (`docs/handoff.md:144-165`). Żaden test tego nie złapał, bo
//! każdy pytał „czy wszyscy skończyli", a wszyscy skończyli.
//!
//! **Dlatego mierzone są zapisane przedziały czasu, nie stan limitera.** Słaba wersja tego
//! kryterium — `assert_eq!(limiter.available_permits(), 3)` albo `assert_eq!(limiter.at_once(),
//! 3)` — przechodzi na limiterze z trzema miejscami, o które nikt nigdy nie prosi więcej niż
//! raz, czyli dokładnie na defekcie, który to kryterium ma odrzucić. Rozstrzygają dwie rzeczy
//! naraz: **maksymalne pokrycie policzone z przedziałów** i **istnienie pary, która ściśle
//! na siebie zachodzi**. Obie są fałszywe dla wysyłki sekwencyjnej.
//!
//! Runtime jest **wielowątkowy, z prawdziwymi snami**, nigdy `start_paused`: czas wirtualny
//! implikuje runtime jednowątkowy i przeskakuje do przodu, kiedy runtime staje bezczynny,
//! więc „nakładanie się" przestaje pod nim cokolwiek znaczyć `[T7 §8.1, V]`.

use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use loadout_lib::engine::limits::{Dispatch, Limiter, Run};
use loadout_lib::engine::step::StepState;
use tokio::task::JoinSet;

/// Ile zadań staje w kolejce po miejsce.
const TASKS: usize = 6;

/// Jak długo każde trzyma miejsce. Sześćdziesiąt milisekund jest o rzędy wielkości dłuższe
/// niż przekazanie miejsca między zadaniami, więc próg nie zależy od obciążenia maszyny.
const HOLD: Duration = Duration::from_millis(60);

/// Limit, przy którym zaczynamy: jeden agent, czyli zero nakładania się.
const AT_START: usize = 1;

/// Do ilu podnosimy suwak w trakcie.
const RAISED: usize = 3;

/// Okno czasu jednego zadania: kiedy dostało miejsce i kiedy je oddało.
type Span = (Instant, Instant);

/// Największa liczba okien przecinających się w jednym punkcie.
///
/// Zamiatanie po zdarzeniach, a nie porównywanie par: para wystarczy do „czy w ogóle
/// zachodzą", ale nie odpowiada na pytanie „ilu naraz", które jest tu całą stawką.
fn peak_overlap(spans: &[Span]) -> usize {
    let mut events: Vec<(Instant, i32)> = Vec::with_capacity(spans.len() * 2);
    for &(start, end) in spans {
        events.push((start, 1));
        events.push((end, -1));
    }
    // Przy identycznym znaczniku czasu koniec idzie PRZED startem: dwa zadania stykające się
    // końcami nie biegły razem, a `Instant` na obciążonej maszynie potrafi dać ten sam odczyt.
    events.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));

    let mut open = 0i32;
    let mut peak = 0i32;
    for (_, delta) in events {
        open += delta;
        peak = peak.max(open);
    }
    usize::try_from(peak).unwrap_or(0)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn raising_the_dial_mid_run_widens_the_window_agents_share() -> Result<(), Box<dyn Error>> {
    let limiter = Limiter::new(AT_START);
    let run = Arc::new(Run::new(limiter.clone(), &[StepState::Ready; TASKS]));

    let mut queued: JoinSet<Option<Span>> = JoinSet::new();
    for _ in 0..TASKS {
        let run = Arc::clone(&run);
        queued.spawn(async move {
            match run.dispatch().await {
                Dispatch::Granted(slot) => {
                    let start = Instant::now();
                    tokio::time::sleep(HOLD).await;
                    let end = Instant::now();
                    // Miejsce wraca do puli PO odczycie końca, więc zapisane okno jest węższe
                    // niż prawdziwe trzymanie: każdy pomiar nakładania się jest zaniżony,
                    // nigdy zawyżony.
                    drop(slot);
                    Some((start, end))
                }
                // Odmowa w biegu, który nigdy nie był w pauzie, jest sama w sobie awarią
                // kryterium — dlatego nie znika tu po cichu, tylko wraca jako brak okna.
                Dispatch::Refused(_) => None,
            }
        });
    }

    // Suwak rusza dopiero PO tym, jak pierwsze zadanie oddało miejsce — czyli w środku biegu,
    // a nie przed nim. Limiter, który przyjmuje nową wartość wyłącznie przy starcie, jest tu
    // nie do odróżnienia od takiego, który jej w ogóle nie przyjmuje.
    let first = queued
        .join_next()
        .await
        .ok_or("not one task ever ran, so there are no windows to compare")??
        .ok_or("the first task was refused a slot in a run that was never paused")?;
    limiter.set_at_once(RAISED);

    let mut spans = vec![first];
    while let Some(joined) = queued.join_next().await {
        spans.push(joined?.ok_or("a task was refused a slot in a run that was never paused")?);
    }
    assert_eq!(
        spans.len(),
        TASKS,
        "every queued task has to report its own window, otherwise the count below is measured \
         on a smaller run than the one that was asked for"
    );

    let peak = peak_overlap(&spans);
    assert_eq!(
        peak, RAISED,
        "after the dial went to {RAISED} exactly that many agents have to occupy one moment in \
         time. This run peaked at {peak}. One means the dial changed a number and nothing else — \
         a limiter that counts correctly but is asked for a slot by a single worker is the \
         the earlier prototype defect verbatim (invariant 11). More than {RAISED} means the raise handed out \
         places nobody bounded. Windows: {spans:?}"
    );

    let truly_shared = spans.iter().any(|held| {
        spans
            .iter()
            .any(|other| held.0 < other.0 && other.0 < held.1)
    });
    assert!(
        truly_shared,
        "some window has to start strictly inside another one that is still open — that is what \
         'at once' means. Sequential dispatch satisfies every count-based assertion and fails \
         this one. Windows: {spans:?}"
    );

    // Kontrola w tym samym pliku: zanim suwak poszedł w górę, limit wynosił jeden, więc okno
    // pierwszego zadania nie ma prawa dzielić czasu z żadnym innym. Bez tej połowy kryterium
    // przechodzi implementacja, która ignoruje wartość startową i od pierwszej sekundy puszcza
    // trzech — a wtedy „podniesienie w trakcie" niczego nie dowodzi, bo nie było czego podnosić.
    let others: Vec<Span> = spans
        .iter()
        .copied()
        .filter(|span| *span != first)
        .collect();
    let first_ran_alone = others
        .iter()
        .all(|&(start, end)| start >= first.1 || first.0 >= end);
    assert!(
        first_ran_alone,
        "at a starting limit of {AT_START} the first window has to be disjoint from every other \
         one; this one shared time with a neighbour, which means the starting value bounded \
         nothing. First: {first:?}, others: {others:?}"
    );

    Ok(())
}
