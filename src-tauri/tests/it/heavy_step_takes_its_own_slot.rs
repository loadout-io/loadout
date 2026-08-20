//! AC-2 dla T-56: krok ciężki bierze miejsce z węższego limitu WEWNĄTRZ puli, więc trzy ciężkie
//! nie nakładają się na siebie, a trzy zwykłe dalej nakładają.
//!
//! Pula miejsc jest jedna na aplikację, ma zakres `1..=8` (domyślnie 3) i **nic nie odróżniało
//! kroku ciężkiego od zwykłego**: krok, który odpala `cargo test`, kosztował w niej dokładnie
//! tyle, co rozmowa. Niezmiennik 26 mówi „nie uruchamiaj dwóch ciężkich `cargo` naraz na tym
//! Macu", a przy suwaku na 3 harness-jako-workflow uruchamiał trzy — maszyna zamarza przy zerowym
//! swapie, a bieg wygląda na wolny, nie na zepsuty.
//!
//! # Słaba wersja tego kryterium
//!
//! `assert_eq!(limiter.heavy_at_once(), 1)` — albo jakakolwiek asercja o OBECNOŚCI pola.
//! Przechodzi dla implementacji, która zapisuje liczbę i nigdy o nic jej nie pyta, czyli dla
//! defektu z niezmiennika 11: poprzedni prototyp miał `max_parallel`, miał zielone testy i nigdy nie
//! uruchomił dwóch agentów naraz. Druga słaba wersja: sama rozłączność trzech okien ciężkich.
//! Przechodzi przy limicie ciężkich równym zero (nic nie biegnie, pusty zbiór okien jest
//! rozłączny) i dla implementacji, w której każda prośba wraca natychmiast (okna zerowej długości
//! nigdy się nie przecinają). Rozstrzygają trzy asercje, których jedna stała nie zaspokoi naraz:
//! trzy wejścia i trzy wyjścia, okno długie dokładnie jak sen, oraz bieg kontrolny ze zwykłymi
//! prośbami **w tym samym pliku**, który musi się NAKŁADAĆ — implementacja szeregująca wszystko
//! przechodzi połowę ciężką i pada na zwykłej.
//!
//! # Zegar wirtualny, i dlaczego akurat tu
//!
//! Pięć testów w tym repo mówi wprost coś odwrotnego (`engine_overlap`,
//! `limits_are_global_across_runs`, `limits_dial_raises`, `runcmd_parallel`,
//! `workspace_global_slots`: „nigdy `start_paused`") i mają rację **u siebie** — ich praca jest
//! prawdziwa (dubler sterownika, prawdziwy proces, prawdziwy sen), więc czas wirtualny
//! przeskoczyłby dokładnie to, co mierzą. Tutaj „pracą" prośby jest jeden `tokio::time::sleep`
//! i nic więcej, więc zegar wirtualny mierzy to, co pula zrobiła z prośbą, i **nic** o tym, jak
//! obciążona jest maszyna. Cena prawdziwego zegara jest zmierzona: cztery testy biegu mierzą czas
//! ściennie i na zajętej maszynie dają fałszywą czerwień.
//!
//! Dwie pułapki mechaniczne, które zamieniłyby ten test w kłamstwo:
//!
//! 1. `Recorder` z `engine/drivers/fake.rs` stempluje `std::time::Instant`, który za zegarem
//!    wirtualnym NIE idzie — pod `start_paused` każde jego okno ma kilka mikrosekund i wszystkie
//!    się nakładają. Ten test stawia więc **własne** znaczniki, na `tokio::time::Instant`.
//! 2. `start_paused` implikuje runtime jednowątkowy, który przesuwa zegar, kiedy staje bezczynny.
//!    Jeden `std::thread::sleep`, jeden blokujący zamek albo jeden prawdziwy proces w środku
//!    prośby zamraża cały runtime i pomiar przestaje być pomiarem. Nic tutaj nie ma prawa
//!    blokować wątku — dlatego liczniki są atomowe, a nie pod zamkiem.

use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use loadout_lib::engine::limits::{Dispatch, Limiter, Run, Weight};
use loadout_lib::engine::step::StepState;
use tokio::task::JoinSet;
use tokio::time::{Instant, sleep};

/// Ile miejsc ma pula.
const POOL: usize = 3;

/// Ile z nich wolno naraz zająć krokom ciężkim.
const HEAVY_AT_ONCE: usize = 1;

/// Ile prośb staje w kolejce w jednym biegu.
const TASKS: usize = 3;

/// Cała „praca" jednej prośby. Jeden sen i nic więcej — patrz nagłówek.
const WORK: Duration = Duration::from_millis(250);

/// Ile czekamy na wszystkie prośby, zanim uznamy pulę za zakleszczoną. Na zegarze wirtualnym
/// ten limit nic nie kosztuje: runtime przesuwa czas dopiero wtedy, gdy nie ma nic do roboty,
/// więc dosięga go wyłącznie bieg, który naprawdę stanął.
///
/// Minuta, nie 60 s: `clippy::duration_suboptimal_units` biegnie w `full` na `-D warnings`,
/// a to jest ta sama liczba, nie inna wartość — tłumienie byłoby tu droższe niż zapis wprost.
const PATIENCE: Duration = Duration::from_mins(1);

/// Okno jednej prośby: kiedy dostała miejsce i kiedy je oddała.
type Span = (Instant, Instant);

/// Co wyszło z jednego biegu prośb.
#[derive(Debug)]
struct Asked {
    /// Okna prośb, którym miejsca przyznano. Prośba bez okna to prośba, która nie weszła
    /// albo nie wyszła.
    spans: Vec<Span>,
    /// Najwięcej prośb w środku naraz, zmierzone przez `running_now()` **w trakcie** okien.
    most_inside: usize,
}

/// Trzy prośby o tej samej wadze, wszystkie tymi samymi drzwiami: [`Run::dispatch_as`].
///
/// Waga jest argumentem prośby, nie drugą pulą z własnym wejściem: druga pula łamałaby zasadę
/// z nagłówka `engine/limits.rs` („wysyłka pyta bieg, bieg pyta pulę"), bo krok ciężki wziąłby
/// miejsce bokiem, z pominięciem pauzy limitu dostawcy.
async fn three_requests(seats: &Limiter, weight: Weight) -> Result<Asked, Box<dyn Error>> {
    let run = Arc::new(Run::new(seats.clone(), &[StepState::Ready; TASKS]));
    let most = Arc::new(AtomicUsize::new(0));

    let mut queued: JoinSet<Option<Span>> = JoinSet::new();
    for _ in 0..TASKS {
        let run = Arc::clone(&run);
        let seats = seats.clone();
        let most = Arc::clone(&most);
        queued.spawn(async move {
            match run.dispatch_as(weight).await {
                Dispatch::Granted(slot) => {
                    let start = Instant::now();
                    // Pytanie zadane W ŚRODKU okna, dwa razy. Odpowiedź sprzed wejścia albo po
                    // wyjściu nie mówi nic o nakładaniu się.
                    most.fetch_max(seats.running_now(), Ordering::SeqCst);
                    sleep(WORK).await;
                    most.fetch_max(seats.running_now(), Ordering::SeqCst);
                    let end = Instant::now();
                    // Miejsce wraca do puli PO odczycie końca, więc zapisane okno jest węższe
                    // niż prawdziwe trzymanie: pomiar nakładania się jest zaniżony, nie zawyżony.
                    drop(slot);
                    Some((start, end))
                }
                // Odmowa w biegu, który nigdy nie widział limitu dostawcy, jest awarią samego
                // kryterium — dlatego nie znika po cichu, tylko wraca jako brak okna.
                Dispatch::Refused(_) => None,
            }
        });
    }

    let joined = tokio::time::timeout(PATIENCE, async move {
        let mut out: Vec<Option<Span>> = Vec::new();
        while let Some(one) = queued.join_next().await {
            out.push(one.ok().flatten());
        }
        out
    })
    .await
    .map_err(|_| "not every request came back, so the pool never let somebody in and never will")?;

    Ok(Asked {
        spans: joined.into_iter().flatten().collect(),
        most_inside: most.load(Ordering::SeqCst),
    })
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn three_heavy_requests_never_share_a_moment() -> Result<(), Box<dyn Error>> {
    let asked = three_requests(&Limiter::with_heavy(POOL, HEAVY_AT_ONCE), Weight::Heavy).await?;

    // (b) WSZYSTKIE TRZY NAPRAWDĘ WESZŁY I WYSZŁY. Okno zapisuje się po powrocie ze snu, więc
    //     trzy okna znaczą trzy wejścia i trzy wyjścia. Bez tego rozłączność niżej przechodzi
    //     przy limicie ciężkich równym zero: pusty zbiór okien jest rozłączny.
    assert_eq!(
        asked.spans.len(),
        TASKS,
        "all {TASKS} heavy requests have to be let in and come back out. A heavy limit that \
         never lets anybody in is not caution, it is a run that stops. Got: {asked:?}"
    );

    // (c) OKNO DŁUGIE DOKŁADNIE JAK SEN. Implementacja, w której prośba wraca natychmiast, daje
    //     okna zerowej długości — a te nigdy się nie przecinają, więc bez tej asercji „nie
    //     nakładają się" jest spełnione przez pulę, która nic nie robi.
    for (start, end) in &asked.spans {
        assert_eq!(
            end.saturating_duration_since(*start),
            WORK,
            "on the virtual clock a request holds its place for exactly as long as its work \
             sleeps. This one held it for {:?} instead of {WORK:?}, which means the window is \
             measuring something other than the work",
            end.saturating_duration_since(*start)
        );
    }

    // (a) PARAMI ROZŁĄCZNE.
    for (first, one) in asked.spans.iter().enumerate() {
        for other in asked.spans.iter().skip(first + 1) {
            let shared = one
                .1
                .min(other.1)
                .saturating_duration_since(one.0.max(other.0));
            assert_eq!(
                shared,
                Duration::ZERO,
                "two heavy requests shared {shared:?} of a {WORK:?} window with a heavy limit of \
                 {HEAVY_AT_ONCE}. Heavy is what invariant 26 is about: two `cargo` builds at once \
                 pin the memory compressor and freeze this machine at zero swap, and the run then \
                 looks slow rather than broken. Windows: {:?}",
                asked.spans
            );
        }
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn three_ordinary_requests_still_share_one_moment() -> Result<(), Box<dyn Error>> {
    // BIEG KONTROLNY, W TYM SAMYM PLIKU. Implementacja szeregująca wszystko przechodzi cały test
    // wyżej i pada tutaj — bez tej połowy „ciężkie nie nakładają się" jest spełnione przez pulę
    // o jednym miejscu, czyli przez zabranie równoległości, która jest całą przesłanką produktu
    // (niezmiennik 11).
    let asked = three_requests(&Limiter::with_heavy(POOL, HEAVY_AT_ONCE), Weight::Ordinary).await?;

    assert_eq!(
        asked.spans.len(),
        TASKS,
        "every ordinary request has to be let in and come back out, or the count below is \
         measured on a smaller run than the one that was asked for. Got: {asked:?}"
    );

    // (d) WSPÓLNA CHWILA, ZMIERZONA DWOMA SPOSOBAMI: liczbą biegnących i przecięciem okien.
    assert_eq!(
        asked.most_inside, POOL,
        "with {POOL} places in the pool all {TASKS} ordinary requests have to be inside at one \
         moment; this run peaked at {}. Fewer means the narrower heavy limit is bounding ordinary \
         work too, and 'how many at once' stops being true about the machine",
        asked.most_inside
    );
    let latest_start = asked
        .spans
        .iter()
        .map(|span| span.0)
        .max()
        .ok_or("no windows to compare")?;
    let earliest_end = asked
        .spans
        .iter()
        .map(|span| span.1)
        .min()
        .ok_or("no windows to compare")?;
    assert!(
        latest_start < earliest_end,
        "the three ordinary windows have to overlap in one instant: the last one to start does so \
         before the first one ends. Windows: {:?}",
        asked.spans
    );
    Ok(())
}

// `clippy::int_plus_one` chce tu `< POOL` zamiast `<= POOL - 1`, a to jest ta sama liczba
// napisana inaczej niż zdanie, które przy niej stoi: komunikat asercji mówi „at most {POOL - 1}
// ordinary ones fit beside it", więc porównanie ma brzmieć tak samo, jak to, co przeczyta ktoś,
// komu ta asercja padnie. Wyłączone na jednym teście, nie na pliku — reszta ma tę regułę dalej.
#[allow(clippy::int_plus_one)]
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_heavy_request_also_takes_one_of_the_ordinary_places() -> Result<(), Box<dyn Error>> {
    // (e) ZAGNIEŻDŻENIE. Bez niego osiem kroków ciężkich biegłoby OBOK trzech zwykłych i sufit
    //     pamięci z niezmiennika 26 przestaje cokolwiek znaczyć: krok ciężki bierze miejsce
    //     z puli **i** miejsce z węższego limitu, nie zamiast.
    let seats = Limiter::with_heavy(POOL, HEAVY_AT_ONCE);
    let run = Arc::new(Run::new(seats.clone(), &[StepState::Ready; POOL + 1]));
    // Ile zwykłych prośb jest w środku w tej chwili.
    let inside = Arc::new(AtomicUsize::new(0));
    // Najwięcej zwykłych, jakie ciężka prośba widziała obok siebie.
    let alongside = Arc::new(AtomicUsize::new(0));
    // Najwięcej prośb w środku naraz, cokolwiek ważą.
    let most_inside = Arc::new(AtomicUsize::new(0));

    let mut queued: JoinSet<bool> = JoinSet::new();
    // Ciężka prośba idzie PIERWSZA, bo runtime jednowątkowy poleca zadania w kolejności
    // zgłoszenia: wtedy trzyma miejsce w chwili, w której zwykłe o nie proszą, i pomiar niżej
    // mówi o czymś. Gdyby weszła później, asercja dalej jest prawdziwa — tylko słabsza.
    {
        let run = Arc::clone(&run);
        let seats = seats.clone();
        let inside = Arc::clone(&inside);
        let alongside = Arc::clone(&alongside);
        let most_inside = Arc::clone(&most_inside);
        queued.spawn(async move {
            let Dispatch::Granted(slot) = run.dispatch_as(Weight::Heavy).await else {
                return false;
            };
            // Pomiar w POŁOWIE okna: zwykłe prośby, które weszły po nas, są wtedy w środku.
            sleep(WORK / 2).await;
            alongside.fetch_max(inside.load(Ordering::SeqCst), Ordering::SeqCst);
            most_inside.fetch_max(seats.running_now(), Ordering::SeqCst);
            sleep(WORK / 2).await;
            drop(slot);
            true
        });
    }
    for _ in 0..POOL {
        let run = Arc::clone(&run);
        let seats = seats.clone();
        let inside = Arc::clone(&inside);
        let most_inside = Arc::clone(&most_inside);
        queued.spawn(async move {
            let Dispatch::Granted(slot) = run.dispatch_as(Weight::Ordinary).await else {
                return false;
            };
            inside.fetch_add(1, Ordering::SeqCst);
            most_inside.fetch_max(seats.running_now(), Ordering::SeqCst);
            sleep(WORK).await;
            inside.fetch_sub(1, Ordering::SeqCst);
            drop(slot);
            true
        });
    }

    let granted = tokio::time::timeout(PATIENCE, async move {
        let mut out: Vec<bool> = Vec::new();
        while let Some(one) = queued.join_next().await {
            out.push(one.unwrap_or(false));
        }
        out
    })
    .await
    .map_err(|_| "not every request came back, so the pool never let somebody in and never will")?;

    assert!(
        granted.len() == POOL + 1 && granted.iter().all(|got| *got),
        "every request has to be let in for the two counts below to mean anything; got {granted:?}"
    );
    assert!(
        alongside.load(Ordering::SeqCst) <= POOL - 1,
        "with one heavy request inside, at most {} ordinary ones fit beside it — the heavy one is \
         holding a place from the same pool. It saw {} of them, which means heavy work runs \
         BESIDE the pool instead of inside it",
        POOL - 1,
        alongside.load(Ordering::SeqCst)
    );
    assert!(
        most_inside.load(Ordering::SeqCst) <= POOL,
        "'how many at once' is {POOL}, so nothing may ever put {} requests inside at one moment, \
         whatever they weigh",
        most_inside.load(Ordering::SeqCst)
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_heavy_limit_is_cut_to_size_where_at_once_is() -> Result<(), Box<dyn Error>> {
    // (f) PRZYCIĘCIE W TYM SAMYM JEDNYM MIEJSCU, CO `at_once`. Ta liczba przychodzi też z pliku
    //     biegu i z zapisanego workflow, a tamtędy nie przechodzi przez żadną kontrolkę.
    let none = Limiter::with_heavy(POOL, 0);
    assert_eq!(
        none.heavy_at_once(),
        1,
        "a heavy limit of zero is a pool in which no heavy step ever starts. That is not being \
         careful, it is a run that stops, so the floor is one — the same floor 'how many at \
         once' has, for the same reason"
    );
    let plenty = Limiter::with_heavy(POOL, 99);
    assert_eq!(
        plenty.heavy_at_once(),
        POOL,
        "the heavy limit lives INSIDE the pool, so it cannot be wider than the pool itself: a \
         heavy step takes a place from the pool and a place from the narrower limit, and a \
         ceiling above {POOL} would bound nothing"
    );

    // I TO SAMO ZACHOWANIEM, bo asercja o samej liczbie przechodzi dla implementacji, która ją
    // zapisuje i nigdy o nic jej nie pyta — czyli dla defektu z niezmiennika 11.
    let asked = three_requests(&none, Weight::Heavy).await?;
    assert_eq!(
        asked.spans.len(),
        TASKS,
        "the clamped floor has to reach the pool, not just the getter: at a heavy limit of zero \
         all {TASKS} heavy requests still have to get in and come back out. Got: {asked:?}"
    );
    Ok(())
}
