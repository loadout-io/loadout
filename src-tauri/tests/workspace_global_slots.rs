//! AC-2 dla T-24: „ile naraz" jest liczbą dla CAŁEJ aplikacji, nie dla jednego biegu.
//!
//! Niezmiennik 11 czyta się tu w drugą stronę niż w T-02. Tam chodziło o to, żeby limit
//! naprawdę pozwalał biec dwóm krokom naraz; tutaj o to, żeby nie pozwolił biec sześciu.
//! Trzy karty po trzech agentach to dziewięciu agentów po ~583 MB szczytowego RSS
//! [T7 ryzyko 3, V] — na 16 GB to zamrożony laptop, a nie szybsza praca. Rejestr, który robi
//! po jednej puli na kartę, zachowuje się identycznie z limitem po 2 i **naprawdę** uruchamia
//! sześciu agentów; nic tego nie złapie, dopóki ktoś nie otworzy trzeciej karty.
//!
//! **Słaba wersja: „wszystkie trzy się skończyły".** Przechodzi przy trzech osobnych semaforach
//! po 2, bo wtedy też wszystkie trzy się kończą — szybciej, i to jest cała różnica, której
//! taka asercja nie widzi. Rozróżnia **maksimum nakładających się przedziałów czasowych**,
//! ta sama technika co w T-02: zapisujemy chwilę wejścia i wyjścia każdego kroku i liczymy,
//! ilu było w środku naraz.
//!
//! Drugi przypadek w tym pliku jest **kontrolą dodatnią** dla tej techniki. Sam próg `peak <= 2`
//! przechodzi także wtedy, gdy pomiar jest ślepy i zawsze widzi jeden — a bieg, w którym nic
//! nigdy nie nachodzi na nic, wygląda dokładnie jak poprzedni prototyp. Dlatego ten sam rejestr, ten sam
//! kod i te same trzy karty przy puli **3** muszą dać trzy nachodzące okna. Jedna stała nie
//! zaspokoi obu przypadków naraz.
//!
//! Runtime jest wielowątkowy z prawdziwymi snami, nigdy `start_paused`: czas wirtualny
//! przeskakuje do przodu, kiedy runtime staje bezczynny, więc „nakładanie się" przestaje
//! cokolwiek znaczyć [T7 §8.1].

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::anyhow;
use loadout_lib::engine::limits::{Dispatch, Limiter, Run};
use loadout_lib::engine::step::StepState;
use loadout_lib::workspace::{Registry, WorkspaceId};
use tokio::task::JoinSet;

/// Ile miejsc ma pula w przypadku właściwym.
const AT_ONCE: usize = 2;

/// Ile kart otwieramy. Trzy, bo przy dwóch limit per bieg i limit globalny dają ten sam obraz.
const WORKSPACES: usize = 3;

/// Jak długo krok trzyma miejsce. Rzędy wielkości ponad koszt wzięcia permitu, żeby próg nie
/// zależał od tego, jak szybko maszyna wystartuje trzecie zadanie.
const STEP: Duration = Duration::from_millis(300);

/// Okno czasu jednego kroku: kiedy wszedł i kiedy wyszedł.
type Span = (Instant, Instant);

/// Największa liczba okien, które nachodzą na siebie w jakiejkolwiek chwili.
///
/// Zamiatanie po krawędziach, nie porównywanie par: przy trzech oknach różnica jest żadna,
/// ale odpowiedź na pytanie „ilu było w środku naraz" ma brzmieć tak samo przy trzydziestu.
fn most_at_once(spans: &[Span]) -> usize {
    let mut edges: Vec<(Instant, i32)> = Vec::with_capacity(spans.len() * 2);
    for (entered, left) in spans {
        edges.push((*entered, 1));
        edges.push((*left, -1));
    }
    // Przy równym znaczniku wyjście idzie PRZED wejściem: krok, który wyszedł dokładnie
    // w chwili, w której wszedł następny, oddał mu swoje miejsce, a nie zajął drugie.
    edges.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));

    let mut live = 0i32;
    let mut peak = 0i32;
    for (_, delta) in edges {
        live += delta;
        peak = peak.max(live);
    }
    usize::try_from(peak).unwrap_or(0)
}

/// Jeden krok jednej karty: poproś o miejsce, zajmij je na [`STEP`], oddaj.
///
/// Prośba idzie przez [`Run::dispatch`], bo to jest jedyne wejście do puli — wzięcie permitu
/// bokiem mierzyłoby uprzejmość tego testu, nie własność rejestru.
async fn one_blocking_step(
    registry: Arc<Registry>,
    id: WorkspaceId,
    at_once: usize,
) -> anyhow::Result<Span> {
    let run = Run::new(registry.slots(&id)?, &[StepState::Ready]);
    match run.dispatch().await {
        Dispatch::Granted(slot) => {
            let entered = Instant::now();
            tokio::time::sleep(STEP).await;
            let left = Instant::now();
            // Zwolnienie siedzi w `Drop`, więc nazywamy je wprost: gdyby slot ginął dopiero
            // na końcu funkcji, chwila zwolnienia byłaby po znaczniku wyjścia i sąsiednie
            // okna czytałyby się jako rozłączne o kilka mikrosekund za wcześnie.
            drop(slot);
            Ok((entered, left))
        }
        Dispatch::Refused(reason) => Err(anyhow!(
            "a run in {id} was refused a slot at a limit of {at_once} instead of waiting for \
             one: {reason:?}. Refusing is what a paused run gets; a run that only has to queue \
             gets to wait"
        )),
    }
}

/// Trzy karty, w każdej jeden blokujący krok, wszystkie nad jedną pulą o zadanej wielkości.
async fn steps_in_three_workspaces(at_once: usize) -> anyhow::Result<Vec<Span>> {
    let root = tempfile::tempdir()?;
    // Pula wchodzi do rejestru z zewnątrz i jest JEDNA. Rejestr, który zrobiłby ją sobie sam
    // per karta, przeszedłby ten sam kod testu — dlatego liczby niżej, a nie kształt tutaj,
    // są tym, co rozstrzyga.
    let registry = Arc::new(Registry::new(Limiter::new(at_once)));

    let mut running: JoinSet<anyhow::Result<Span>> = JoinSet::new();
    for n in 0..WORKSPACES {
        let path: &Path = &root.path().join(format!("project-{n}"));
        std::fs::create_dir_all(path)?;
        let id = registry.open(path)?;
        running.spawn(one_blocking_step(Arc::clone(&registry), id, at_once));
    }

    let mut spans = Vec::with_capacity(WORKSPACES);
    while let Some(joined) = running.join_next().await {
        spans.push(joined??);
    }
    Ok(spans)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_workspaces_share_one_pool_of_slots() -> anyhow::Result<()> {
    let spans = steps_in_three_workspaces(AT_ONCE).await?;
    assert_eq!(
        spans.len(),
        WORKSPACES,
        "all {WORKSPACES} steps have to finish; measured windows only mean something when every \
         one of them has both ends"
    );

    let peak = most_at_once(&spans);
    assert!(
        peak <= AT_ONCE,
        "{peak} steps were inside at the same moment across the whole application, and the \
         limit is {AT_ONCE}. That is a pool per run: three tabs at two apiece is six agents at \
         ~583 MB each, which is a frozen laptop rather than faster work [T7 risk 3]"
    );

    let mut by_start = spans.clone();
    by_start.sort_by_key(|(entered, _)| *entered);
    let [(_, first_left), (_, second_left), (third_entered, _)] = by_start[..] else {
        return Err(anyhow!(
            "expected exactly {WORKSPACES} windows, measured {}",
            by_start.len()
        ));
    };
    let earliest_end = first_left.min(second_left);
    assert!(
        third_entered >= earliest_end,
        "the third step began {:?} before either of the first two had finished. Waiting for a \
         free slot is the whole point of a shared pool — a third step that starts early did not \
         wait for anybody",
        earliest_end.saturating_duration_since(third_entered)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_same_three_workspaces_do_run_three_at_once_when_the_pool_says_three()
-> anyhow::Result<()> {
    let spans = steps_in_three_workspaces(WORKSPACES).await?;
    assert_eq!(
        spans.len(),
        WORKSPACES,
        "all {WORKSPACES} steps have to finish here too"
    );

    let peak = most_at_once(&spans);
    assert_eq!(
        peak, WORKSPACES,
        "at a pool of {WORKSPACES} the same three steps have to occupy overlapping windows. \
         This case exists so the ceiling asserted in the other one means something: a \
         measurement that can only ever report one would satisfy `peak <= {AT_ONCE}` while \
         nothing in this application ever ran beside anything else — which is precisely what \
         the earlier prototype's max_parallel did"
    );
    Ok(())
}
