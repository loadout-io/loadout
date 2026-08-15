//! Maszyna stanów kroku: siedem stanów i funkcja przejścia.
//!
//! Tabela wiążąca stoi w `docs/ARCHITECTURE.md` §5, jej wersja z efektami ubocznymi
//! w [T7 §9.3]. **`paused` nie jest stanem kroku** — to stan **biegu**: pauza wstrzymuje
//! wysyłkę i pozwala biegnącym krokom się skończyć. Trzymanie pauzy poza maszyną kroku usuwa
//! całą ćwiartkę stanów, których nikt nie potrzebuje.
//!
//! **Dlaczego funkcja przejścia bierze stan wejściowy, a nie tylko zdarzenie.** Wersja
//! `fn next(_from, ev) -> Some(target_of(ev))` przechodzi każdą asercję na przejściach
//! legalnych i **w biegu pozwala anulować krok, który już się udał** — a wtedy jego dzieci
//! zostają policzone drugi raz. Cztery przypadki zwracające `None` istnieją właśnie po to,
//! żeby ta wersja nie przeszła.

use serde::{Deserialize, Serialize};

/// Stan pojedynczego kroku.
///
/// Nazwy z drutu (`"pending"`, `"ready"`, …) są **tymi samymi siedmioma wartościami**, które
/// niesie `CHECK` w kolumnie `steps.status` [T7 §5.4]. Dlatego serializacja jest częścią
/// kontraktu, a nie szczegółem: rozjazd między tym enumem a schematem bazy skończyłby się
/// wierszem, którego `SQLite` nie przyjmie, w trakcie biegu.
///
/// **Wyjątek od niezmiennika 5, świadomy.** Enumy z drutu dostają `#[serde(other)]`, bo
/// vendorzy dokładają typy zdarzeń co tydzień. Ten enum nie przychodzi z drutu — jest nasz,
/// a jego zbiór wartości jest zamknięty po obu stronach (kod i `CHECK`). Wariant „coś innego"
/// zamieniłby nieznany stan w cichy błąd zamiast w odmowę: `"paused"` **ma** zostać odrzucone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    /// Wpisany do biegu, czeka na rodziców.
    Pending,
    /// Stopień wejściowy spadł do zera; w kolejce, jeszcze bez permitu.
    Ready,
    /// Permit wzięty, krok naprawdę działa.
    Running,
    /// Koniec, powodzenie.
    Succeeded,
    /// Koniec, niepowodzenie.
    Failed,
    /// Koniec, bo użytkownik zatrzymał bieg.
    Cancelled,
    /// Koniec, bo ktoś wyżej padł.
    Skipped,
}

/// Zdarzenie, które może ruszyć krok z miejsca.
///
/// Nazwy mówią, **co się stało**, nie do jakiego stanu prowadzą — dzięki temu tabela przejść
/// jest jedynym miejscem, które wie, dokąd prowadzą.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepEvent {
    /// Ostatni rodzic się skończył.
    InDegreeZero,
    /// Któryś krok wyżej padł.
    UpstreamFailed,
    /// Któryś krok wyżej został anulowany.
    UpstreamCancelled,
    /// Semafor przepuścił krok.
    PermitAcquired,
    /// Proces wyszedł czysto.
    ExitOk,
    /// Proces wyszedł błędem.
    ExitError,
    /// Minął limit czasu kroku.
    Timeout,
    /// Użytkownik zatrzymał bieg.
    UserCancelled,
    /// Użytkownik ponawia krok (T-15; w tej fazie nikt tego nie woła).
    Retry,
}

/// Co krok zameldował, kiedy wrócił.
///
/// **Anulowanie jest wariantem wartości, nie błędem** (niezmiennik 7). `Err(Cancelled)`
/// zmuszałoby każdego wołającego do rozpakowywania błędu, który awarią nie jest, a stamtąd
/// jest już tylko krok do potraktowania świadomego Stopu jak usterki.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepReport {
    /// Krok doszedł do końca i się udał.
    Succeeded,
    /// Krok doszedł do końca i się nie udał.
    Failed,
    /// Krok zobaczył anulowanie w środku i zwinął się sam.
    Cancelled,
}

/// Przejście maszyny stanów kroku. `None` znaczy „to przejście nie istnieje".
///
/// `None`, nie `Err`: nielegalne przejście nie jest awarią biegu, tylko zdarzeniem, które
/// w tym stanie nic nie znaczy i które planista ma po prostu porzucić.
#[must_use]
pub fn next(state: StepState, event: StepEvent) -> Option<StepState> {
    use StepEvent as Event;
    use StepState as State;

    // Tabela `docs/ARCHITECTURE.md` §5, przepisana wprost. Pary sklejone `|` są sklejone
    // dlatego, że `clippy::match_same_arms` (pedantic, a `-D warnings` w bramce) nie przepuszcza
    // dwóch ramion o identycznym ciele — nie dlatego, że znaczą to samo. Kolejność ramion jest
    // kolejnością wierszy tabeli na tyle, na ile to sklejenie pozwala.
    match (state, event) {
        (State::Pending, Event::InDegreeZero) => Some(State::Ready),
        (State::Pending, Event::UpstreamFailed) => Some(State::Skipped),
        // Krok wyżej anulowany i Stop w samym kroku prowadzą w to samo miejsce, ale są dwoma
        // różnymi zdaniami dla użytkownika. Rozróżnia je zdarzenie, nie stan docelowy.
        (State::Pending, Event::UpstreamCancelled) | (State::Running, Event::UserCancelled) => {
            Some(State::Cancelled)
        }
        (State::Ready, Event::PermitAcquired) => Some(State::Running),
        (State::Running, Event::ExitOk) => Some(State::Succeeded),
        // Limit czasu jest niepowodzeniem kroku, nie osobnym stanem. Zabicie grupy procesów,
        // które za nim stoi, mieszka w supervisorze (T-03) — tutaj widać wyłącznie skutek.
        (State::Running, Event::ExitError | Event::Timeout) => Some(State::Failed),
        (State::Failed | State::Cancelled | State::Skipped, Event::Retry) => Some(State::Pending),
        // Wszystko inne nie istnieje. `Succeeded` nie ma stąd wyjścia w ogóle: krok, który się
        // udał, ma już policzone dzieci, więc anulowanie go albo powtórzenie policzyłoby je
        // drugi raz. To jest jedyny powód, dla którego ta funkcja bierze stan wejściowy.
        _ => None,
    }
}
