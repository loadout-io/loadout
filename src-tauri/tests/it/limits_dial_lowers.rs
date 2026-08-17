//! AC-2 dla T-21: obniżenie suwaka nic nie zabija i wchodzi w życie dopiero przy zwalnianiu.
//!
//! **Słaba wersja tego kryterium to `assert_eq!(limiter.at_once(), 2)` zaraz po wywołaniu
//! settera.** To jest asercja o setterze, nie o limiterze — przechodzi ją zapis do pola
//! i przechodzi ją także implementacja, która przy obniżeniu zdejmuje trwające zadania.
//!
//! Rozstrzygają trzy rzeczy naraz. Piąte zadanie musi być udowodnione jako **wciąż czekające**
//! po dwóch zwolnieniach (`tokio::time::timeout` na jego starcie kończy się `Err`) i jako
//! wystartowane po trzecim. Cztery trwające zadania muszą wrócić własnym `Outcome::Done` —
//! anulowanie jest tu wartością, nie błędem (niezmiennik 7), więc implementacja, która
//! obniżenie suwaka realizuje odmową, nie ma jak się schować za panikę. A `at_once()`
//! i `running_now()` muszą przez ten czas zwracać **dwie różne liczby**: UI, który po obniżeniu
//! suwaka natychmiast pokazuje „2 at once", kłamie o tym, co w tej chwili dzieje się na maszynie.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use loadout_lib::engine::limits::{Dispatch, Limiter, Outcome, Run};
use loadout_lib::engine::step::StepState;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::timeout;

/// Limit, przy którym startujemy, i zarazem liczba zadań, które trzymają miejsca.
const AT_START: usize = 4;

/// Do ilu schodzi suwak w trakcie.
const LOWERED: usize = 2;

/// Ile kroków ma bieg: cztery trzymające plus jeden czekający.
const STEPS: usize = 5;

/// Ile czekamy, zanim uznamy, że zadanie NIE dostało miejsca. Musi z zapasem starczyć na
/// przekazanie miejsca między zadaniami, bo inaczej test meldowałby „czeka" o implementacji,
/// która po prostu jest wolna.
const GRACE: Duration = Duration::from_millis(120);

/// Ile czekamy na rzeczy, które mają się zdarzyć od razu. Sufit jest po to, żeby błędna
/// implementacja padła z nazwanym powodem zamiast wisieć do końca świata.
const PATIENCE: Duration = Duration::from_secs(2);

/// Zadanie, które bierze miejsce, melduje to i trzyma je aż do odwołania.
///
/// Melduje kanałem, a nie snem: sen mierzyłby szybkość maszyny, a pytanie brzmi „czy w ogóle
/// dostało miejsce".
fn spawn_holder(
    run: &Arc<Run>,
    started: oneshot::Sender<()>,
    release: oneshot::Receiver<()>,
) -> JoinHandle<Outcome> {
    let run = Arc::clone(run);
    tokio::spawn(async move {
        match run.dispatch().await {
            Dispatch::Granted(slot) => {
                // Drugi koniec kanału może już nie żyć, kiedy test przewrócił się wcześniej.
                // To nie jest powód, żeby zgubić miejsce w puli.
                let _ = started.send(());
                let _ = release.await;
                drop(slot);
                Outcome::Done
            }
            Dispatch::Refused(_) => Outcome::Cancelled,
        }
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lowering_the_dial_cancels_nothing_and_bites_only_when_a_slot_comes_back()
-> Result<(), Box<dyn Error>> {
    let limiter = Limiter::new(AT_START);
    let run = Arc::new(Run::new(limiter.clone(), &[StepState::Ready; STEPS]));

    let mut holders = Vec::with_capacity(AT_START);
    let mut releases = Vec::with_capacity(AT_START);
    for n in 1..=AT_START {
        let (started_tx, started_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        holders.push(spawn_holder(&run, started_tx, release_rx));
        releases.push(release_tx);
        // Czekamy na każde z osobna: bez tego bieg mógłby nigdy nie dojść do limitu startowego,
        // a wszystko niżej byłoby zmierzone na mniejszym biegu, niż zamówione.
        timeout(PATIENCE, started_rx)
            .await
            .map_err(|_| format!("task {n} of {AT_START} never got a slot"))??;
    }

    assert_eq!(
        limiter.running_now(),
        AT_START,
        "all {AT_START} tasks reported that they hold a slot, so the limiter has to agree"
    );

    limiter.set_at_once(LOWERED);

    assert_eq!(
        limiter.at_once(),
        LOWERED,
        "the dial shows what is supposed to hold from now on"
    );
    assert_eq!(
        limiter.running_now(),
        AT_START,
        "and this is the other number: {AT_START} agents are still on the machine. An \
         implementation that makes these two agree the moment the dial moves has either killed \
         running work or is lying about it — and a screen saying '{LOWERED} at once' while \
         {AT_START} agents burn memory is the second one"
    );

    // Piąte zadanie wchodzi do kolejki JUŻ PO obniżeniu, więc każde miejsce, jakie dostanie,
    // dostanie na nowych zasadach.
    let (fifth_started_tx, mut fifth_started_rx) = oneshot::channel();
    let (fifth_release_tx, fifth_release_rx) = oneshot::channel();
    let fifth = spawn_holder(&run, fifth_started_tx, fifth_release_rx);

    let mut waiting = releases.into_iter();
    for round in 1..=2 {
        let release = waiting
            .next()
            .ok_or("the test ran out of tasks to release before it ran out of rounds")?;
        let _ = release.send(());
        assert!(
            timeout(GRACE, &mut fifth_started_rx).await.is_err(),
            "after release {round} there are still at least as many agents on the machine as \
             the dial allows, so the freed slot has to disappear instead of being handed on. \
             The fifth task started anyway, which means lowering the dial changed a number and \
             nothing else"
        );
    }

    let release = waiting
        .next()
        .ok_or("the test ran out of tasks to release before the third round")?;
    let _ = release.send(());
    timeout(PATIENCE, &mut fifth_started_rx)
        .await
        .map_err(|_| {
            format!(
                "the third release is the one that finally brings the machine below the dial, \
                 so the fifth task has to start here. It did not, which is a dial that swallows \
                 slots forever instead of holding at {LOWERED}"
            )
        })??;

    // Ostatni kanał ginie razem z iteratorem. Dla zadania zamknięty kanał znaczy to samo co
    // sygnał: puść miejsce i wróć — więc nie ma tu czego wysyłać osobno.
    drop(waiting);
    let _ = fifth_release_tx.send(());

    for (index, holder) in holders.into_iter().enumerate() {
        let outcome = timeout(PATIENCE, holder)
            .await
            .map_err(|_| format!("task {index} never came back at all"))??;
        assert_eq!(
            outcome,
            Outcome::Done,
            "task {index} was already running when the dial went down, so it has to finish on \
             its own terms. Lowering how many may start says nothing about what is already \
             started — killing one is a cancelled agent the user never asked to cancel"
        );
    }

    let fifth_outcome = timeout(PATIENCE, fifth)
        .await
        .map_err(|_| "the fifth task never came back at all")??;
    assert_eq!(
        fifth_outcome,
        Outcome::Done,
        "the fifth task got its slot the honest way and has to finish like the others"
    );

    Ok(())
}
