//! Deterministyczny dubler kroku dla testów planisty.
//!
//! **Nie implementuje żadnego traitu i nie będzie musiał.** `trait AgentDriver` dostaje drugą
//! implementację dopiero w T-04, a trait z jedną implementacją to trait wymyślony. Dublerem na
//! poziomie sterownika są w T-04 skrypty na dysku, bo one przechodzą prawdziwą ścieżkę
//! uruchomienia procesu; ten plik istnieje wyłącznie po to, żeby dało się mierzyć **planistę**.
//!
//! Mierzy trzy rzeczy, których nie da się zobaczyć po samych stanach końcowych:
//! kiedy krok wszedł i wyszedł (nakładanie się okien), ilu było w środku naraz (szczyt
//! równoczesności) i czy anulowanie **dotarło do wnętrza kroku**.
//!
//! **Niezmiennik 8 w jednej linii.** Rejestrator kusi dokładnie do
//! `log.lock().push(mark); sleep(d).await;` w jednym wyrażeniu — a to zakleszcza bieg przy
//! `limit > 1` i wygląda jak zawieszony agent, nie jak błąd blokady. Dlatego [`Recorder::mark`]
//! jest **synchroniczne**: guard powstaje i ginie w jednym wywołaniu, po którym dopiero
//! zaczyna się jakikolwiek `await`.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use super::step::StepReport;
use super::StepId;

/// Jak długo trwa [`Behaviour::Hang`]. Rzędy wielkości ponad każdy limit czasu w testach:
/// krok, który to przesiedział do końca, znaczy, że anulowanie nigdy nie doszło.
pub const HANG: Duration = Duration::from_secs(30);

/// Co dubler ma zrobić z krokiem o danym numerze.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Behaviour {
    /// Kończy się natychmiast, powodzeniem.
    Succeed,
    /// Kończy się natychmiast, niepowodzeniem.
    Fail,
    /// Zajmuje krok na podany czas, potem powodzenie. **Prawdziwy sen, nie czas wirtualny**:
    /// `start_paused` implikuje runtime jednowątkowy i przeskakuje zegar do przodu, kiedy
    /// runtime staje bezczynny, więc „nakładanie się" przestałoby cokolwiek znaczyć [T7 §8.1].
    Busy(Duration),
    /// Nie kończy się sam przez [`HANG`]. Zdejmuje go wyłącznie anulowanie.
    Hang,
}

/// Ślad, który dubler zostawia w rejestratorze.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// Krok wszedł do środka — **po wzięciu permitu**, wewnątrz zadania. Znacznik postawiony
    /// w pętli wysyłki mierzyłby moment zakolejkowania i pokazywałby nakładanie się tam,
    /// gdzie go nie ma.
    Enter,
    /// Krok wyszedł, jakkolwiek się skończył.
    Exit,
    /// Krok **zobaczył anulowanie w środku**. To jest jedyny ślad, który odróżnia token, który
    /// doszedł do kroku, od `JoinSet::abort_all`, po którym w T-03 zostaje żywy proces.
    CancelSeen,
}

/// Jeden wpis rejestratora.
#[derive(Debug, Clone, Copy)]
pub struct Entry {
    /// Którego kroku dotyczy.
    pub step: StepId,
    /// Co się stało.
    pub mark: Mark,
    /// Monotoniczny numer w skali całego biegu. Numer, nie czas: porównanie „rodzic skończył
    /// przed startem dziecka" ma być wolne od rozdzielczości zegara.
    pub seq: u64,
    /// Kiedy, zegarem monotonicznym. Do mierzenia nakładania się okien.
    pub at: Instant,
}

/// Wspólny rejestrator jednego biegu.
#[derive(Debug, Default)]
pub struct Recorder {
    /// Wpisy w kolejności zapisu.
    ///
    /// **Ten guard nigdy nie przechodzi przez `await`** (niezmiennik 8). Cały dostęp jest
    /// zamknięty w synchronicznym [`Recorder::mark`], więc nie ma wyrażenia, w którym guard
    /// dożyłby do punktu zawieszenia. `clippy::await_holding_lock` (deny w `Cargo.toml`)
    /// pilnuje reszty, ale sam w sobie jest siatką, nie projektem.
    log: Mutex<Vec<Entry>>,
    /// Źródło monotonicznych numerów.
    seq: AtomicU64,
    /// Ilu kroków jest w środku w tej chwili.
    live: AtomicUsize,
    /// Ilu było w środku najwięcej naraz. To jest liczba, na której przewraca się planista
    /// z jednym workerem: przy ośmiu gotowych i limicie 3 zostaje na 1.
    peak: AtomicUsize,
}

impl Recorder {
    /// Pusty rejestrator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Zapisuje znacznik. **Synchroniczne z rozmysłem** — patrz niezmiennik 8 przy polu `log`.
    pub fn mark(&self, step: StepId, mark: Mark) {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        match mark {
            Mark::Enter => {
                let live = self.live.fetch_add(1, Ordering::SeqCst) + 1;
                self.peak.fetch_max(live, Ordering::SeqCst);
            }
            Mark::Exit => {
                self.live.fetch_sub(1, Ordering::SeqCst);
            }
            Mark::CancelSeen => {}
        }
        let entry = Entry {
            step,
            mark,
            seq,
            at: Instant::now(),
        };
        // Zatruty zamek nie może zgubić wpisu: panika w jednym kroku nie ma prawa oślepić
        // pomiaru, który akurat dowodzi, że pozostałe kroki biegły naraz.
        match self.log.lock() {
            Ok(mut log) => log.push(entry),
            Err(poisoned) => poisoned.into_inner().push(entry),
        }
    }

    /// Kopia wszystkich wpisów, w kolejności zapisu.
    #[must_use]
    pub fn entries(&self) -> Vec<Entry> {
        match self.log.lock() {
            Ok(log) => log.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Największa liczba kroków, które były w środku naraz.
    #[must_use]
    pub fn peak(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }

    /// Ile razy krok wszedł do środka. Musi wyjść 1 — 0 znaczy, że planista go pominął,
    /// a 2 znaczy, że policzył go dwa razy.
    #[must_use]
    pub fn run_count(&self, step: StepId) -> usize {
        self.entries()
            .iter()
            .filter(|entry| entry.step == step && entry.mark == Mark::Enter)
            .count()
    }

    /// Okno czasu kroku: wejście i wyjście. `None`, dopóki nie ma obu.
    #[must_use]
    pub fn span(&self, step: StepId) -> Option<(Instant, Instant)> {
        let entered = self.first(step, Mark::Enter)?;
        let left = self.first(step, Mark::Exit)?;
        Some((entered.at, left.at))
    }

    /// Numer sekwencji wejścia kroku.
    #[must_use]
    pub fn enter_seq(&self, step: StepId) -> Option<u64> {
        Some(self.first(step, Mark::Enter)?.seq)
    }

    /// Numer sekwencji wyjścia kroku.
    #[must_use]
    pub fn exit_seq(&self, step: StepId) -> Option<u64> {
        Some(self.first(step, Mark::Exit)?.seq)
    }

    /// Czy anulowanie doszło do wnętrza tego kroku.
    #[must_use]
    pub fn saw_cancel(&self, step: StepId) -> bool {
        self.first(step, Mark::CancelSeen).is_some()
    }

    fn first(&self, step: StepId, mark: Mark) -> Option<Entry> {
        self.entries()
            .into_iter()
            .find(|entry| entry.step == step && entry.mark == mark)
    }
}

/// Dubler kroku. Klonowalny, bo planista dostaje domknięcie, które woła go dla każdego węzła.
#[derive(Debug, Clone)]
pub struct FakeDriver {
    recorder: Arc<Recorder>,
    behaviours: Arc<Vec<Behaviour>>,
}

impl FakeDriver {
    /// Dubler zapisujący do `recorder`, z zachowaniem `behaviours[step]` dla każdego kroku.
    #[must_use]
    pub fn new(recorder: Arc<Recorder>, behaviours: Vec<Behaviour>) -> Self {
        Self {
            recorder,
            behaviours: Arc::new(behaviours),
        }
    }

    /// Jeden krok. Bierze `self` przez wartość, żeby zwrócony future był `'static` i dało się
    /// go wpuścić do `JoinSet` bez pożyczek.
    pub async fn run(self, step: StepId, cancel: CancellationToken) -> StepReport {
        // SZKIELET (2026-08-15) — świadomie zła odpowiedź: nie zapisuje do rejestratora ANI
        // JEDNEGO znacznika i nie patrzy na zachowanie. Dzięki temu każde kryterium mierzące
        // czas, szczyt równoczesności, kolejność albo dotarcie anulowania pada na pustym
        // rejestratorze, czyli na braku zachowania, a nie na braku pliku. `Failed` zamiast
        // `Succeeded`, bo krok, który nigdy nie biegł, nie ma jak zameldować powodzenia.
        //
        // Implementacja: `mark(Enter)`, potem `select!` z `biased;` — najpierw
        // `cancel.cancelled()` (wtedy `mark(CancelSeen)` i `Cancelled`), potem sen zależny od
        // zachowania — i `mark(Exit)` na KAŻDEJ ścieżce wyjścia. Guard zamka nie ma prawa
        // dożyć do żadnego `await` (niezmiennik 8).
        let _ = (self.recorder, self.behaviours, step, cancel);
        StepReport::Failed
    }
}
