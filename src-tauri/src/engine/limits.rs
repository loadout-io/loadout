//! Limiter współbieżności („ile naraz") i brama limitu dostawcy.
//!
//! Dwie rzeczy, jedna wspólna zasada: **wysyłka pyta bieg, bieg pyta pulę.** Jedno drzwi,
//! więc nie da się ominąć pauzy, biorąc slot bokiem.
//!
//! **Granica tego pliku** (niezmiennik 1). Nie ma tu ani jednego typu z okna aplikacji i ani
//! jednego z `stream.rs`. Sygnał limitu wchodzi jako surowy `&serde_json::Value` obiektu
//! `rate_limit_info`, więc wybór właściwego pola jest testowalny bez okna i bez procesu.
//! Kto zamieni te zdarzenia na wiersze dla użytkownika, decydują T-05 i T-07 — kuszące
//! `use crate::ipc::…` łamie tę granicę po cichu, bo słowa „okno" nie ma wtedy w pliku
//! i grep w bramce tego nie widzi.
//!
//! **Zasięg puli.** Limiter jest JEDEN NA APLIKACJĘ, nie jeden na bieg: trzy karty po trzech
//! agentach to dziewięciu agentów po ~583 MB, czyli zamrożony laptop, a nie szybsza praca
//! (`docs/ARCHITECTURE.md` §6a). Klon [`Limiter`] dzieli tę samą pulę i to jest cały mechanizm;
//! dowód, że dwa biegi w dwóch workspace'ach naprawdę ją dzielą, należy do T-24 AC-2.
//!
//! # Stan tego pliku: SZKIELET (2026-08-16)
//!
//! Ciała zwracają **świadomie złą wartość** i każde jest tak oznaczone komentarzem `SZKIELET`.
//! To jest wymagany kształt fazy, w której powstają kryteria: test ma się skompilować i paść
//! **w czasie wykonania, na braku ZACHOWANIA** (`AGENTS.md` §2a p. 5). `todo!()` jest tu
//! zakazany polityką lintów repo (`clippy::todo = deny`), więc rolę „jeszcze nie napisane"
//! grają wartości dobrane tak, żeby żadnego kryterium nie dało się na nich przejść:
//! stały limit 1 i brama zawsze otwarta.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde_json::Value;
use tokio::sync::Semaphore;
use tokio::time::Instant;

use super::step::StepState;

/// Ile agentów naraz, kiedy człowiek jeszcze nic nie wybrał.
///
/// Zmierzone: jeden proces `claude` to **583 MB** szczytowego RSS (to Node, nie cienki
/// klient), więc na typowych 16 GB mieszczą się realnie 3–4 agenty `[T7 §7.1, V]`.
pub const DEFAULT_AT_ONCE: usize = 3;

/// Sufit suwaka. Powyżej ośmiu wiąże już nie pamięć, tylko limit u dostawcy `[T7 §7.1]`.
pub const MAX_AT_ONCE: usize = 8;

/// Podłoga suwaka. Zero agentów to nie jest „wolniej" — to bieg, który nigdy nie ruszy,
/// a od zatrzymywania biegu jest Stop, nie suwak.
const MIN_AT_ONCE: usize = 1;

/// Podpowiedź przy pierwszym uruchomieniu: `clamp(total_gb / 4, 1, 8)` `[T7 §7.1]`.
///
/// Pamięć maszyny wchodzi **argumentem**. Kto ją zmierzy, potrzebuje `sysinfo`, czyli zmiany
/// `Cargo.toml` — a to jest moment na zatrzymanie się i zapytanie człowieka (`AGENTS.md` §7),
/// nie cichy dopisek.
#[must_use]
pub fn suggested_at_once(total_gb: u64) -> usize {
    let _ = total_gb;
    // SZKIELET — wartość stała. Implementacja, która zwraca to samo dla każdej maszyny,
    // proponuje trzech agentów także na laptopie z 8 GB, gdzie sam ich RSS to 1,7 GB.
    MIN_AT_ONCE
}

/// Ile jeszcze czekać na odnowienie limitu.
///
/// **`resetsAt` jest w SEKUNDACH uniksowych** `[T7 §7.2, V]`. Potraktowany jak milisekundy
/// daje albo wznowienie natychmiast, albo za 300 000 s — i w obu przypadkach wygląda to na
/// „coś z zegarem", a nie na pomyloną jednostkę, więc szuka się tego godzinami.
///
/// Limit, który już się odnowił, daje zero zamiast ujemnej różnicy: `Duration` nie ma znaku,
/// a odejmowanie w drugą stronę byłoby paniką w silniku (`AGENTS.md` §4).
#[must_use]
pub fn duration_until_reset(resets_at_unix: i64, now_unix: i64) -> Duration {
    let _ = (resets_at_unix, now_unix);
    // SZKIELET — zero znaczy „wznów natychmiast", czyli dokładnie ten wynik, który daje
    // najczęstsza pomyłka jednostki.
    Duration::ZERO
}

/// Odpowiedź bramy na jedno zdarzenie limitu dostawcy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// Wysyłamy dalej.
    Open,
    /// Nic nowego nie wychodzi do tej chwili — sekundy uniksowe, prosto z drutu.
    PausedUntil(i64),
}

/// Czyta surowy obiekt `rate_limit_info` i mówi, czy wolno dalej wysyłać.
///
/// **Decyduje `status`, i tylko `status`.** Prawdziwa linia z **udanego** biegu niesie
/// `"status":"allowed"` OBOK `"overageStatus":"rejected"` i
/// `"overageDisabledReason":"out_of_credits"` — dwa pola ze słowem „rejected" tuż przy polu,
/// które mówi „allowed" (`docs/research/fixtures/claude-stream.jsonl`). Implementacja czytająca
/// którekolwiek z tych dwóch pauzuje **każdy** bieg po pierwszym takim zdarzeniu, a te są 1,3%
/// normalnego strumienia `[T7 §4.3, V]`: produkt nie uruchamia się nigdy, a testy „pauzy po
/// limicie" świecą na zielono, bo pauza faktycznie działa.
///
/// Fail-closed: reguła brzmi `status != "allowed"`, nie `status == "rejected"`. Wartość,
/// której dziś nie ma w żadnym pomiarze, ma zatrzymać wysyłkę, a nie przejść bokiem.
///
/// Brak pola `status` to nieznany kształt: idzie do dziennika debug i zostaje porzucony
/// (niezmiennik 5). Vendorzy dokładają pola co tydzień, po cichu — nieznana linia nie ma
/// prawa wywalić biegu.
#[must_use]
pub fn read_gate(info: &Value) -> Gate {
    let _ = info;
    // SZKIELET — brama zawsze otwarta i nic nie trafia do dziennika.
    Gate::Open
}

/// Stan **biegu**.
///
/// Lista jest zamknięta `[T7 §5.4]`: nie ma trzeciego stanu między `running` a `paused`.
/// `paused` nie jest i nie będzie stanem kroku (`docs/ARCHITECTURE.md` §5) — trzymanie pauzy
/// poza maszyną kroku usuwa całą ćwiartkę stanów, których nikt nie potrzebuje.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    /// Bieg wysyła.
    Running,
    /// Bieg czeka na odnowienie limitu. Kroki, które już działają, działają dalej.
    Paused,
}

impl RunStatus {
    /// Nazwa z drutu — ta sama wartość, którą niesie kolumna `runs.status`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Paused => "paused",
        }
    }
}

/// Dlaczego wysyłka dostała odmowę.
///
/// Osobny typ mimo jednego wariantu: drugi powód („człowiek zatrzymał bieg") należy do T-07
/// i ma się dopisać **tutaj**, a nie rozlać po [`Dispatch`] jako kolejny wariant obok
/// [`Dispatch::Granted`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Bieg czeka na odnowienie limitu dostawcy.
    Paused,
}

/// Odpowiedź na prośbę o slot.
///
/// **Wartość, nie `Result`** (niezmiennik 7): odmowa nie jest awarią, tylko drugim normalnym
/// zakończeniem prośby. `Err(Paused)` zmuszałoby każdego wołającego do rozpakowywania błędu,
/// który błędem nie jest — a stamtąd jest już tylko krok do policzenia pauzy jako usterki.
#[derive(Debug)]
pub enum Dispatch {
    /// Slot jest twój, dopóki żyje ta wartość.
    Granted(Slot),
    /// Nic nie wysyłamy; powód jest wartością.
    Refused(Refusal),
}

/// Czym skończył ten, kto prosił o slot. Anulowanie jest wartością (niezmiennik 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Doszedł do końca.
    Done,
    /// Nie doszedł, bo go nie wpuszczono albo zwinął się sam. To nie jest błąd.
    Cancelled,
}

/// Jedno miejsce w puli, zajęte tak długo, jak długo żyje ta wartość.
///
/// Zwolnienie siedzi w `Drop`, a nie w metodzie `release()`, bo krok potrafi wrócić także
/// paniką albo anulowaniem — slot oddawany wyłącznie na szczęśliwej ścieżce daje pulę, która
/// kurczy się przez cały bieg, aż nic już nie startuje.
#[derive(Debug)]
pub struct Slot {
    pool: Arc<Pool>,
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.pool.give_back();
    }
}

/// Wspólna pula miejsc. Jedna na aplikację; [`Limiter`] jest tylko uchwytem do niej.
#[derive(Debug)]
struct Pool {
    /// Miejsca do wzięcia. Permity są **zapominane** przy braniu, a oddaje je [`Pool::give_back`]
    /// — inaczej zwolnienie byłoby bezwarunkowe i obniżenie suwaka nie miałoby gdzie wejść
    /// w życie.
    slots: Semaphore,
    /// Ile ma biec, czyli co pokazuje suwak.
    at_once: AtomicUsize,
    /// Ile biegnie naprawdę. Po obniżeniu suwaka te dwie liczby przez chwilę się różnią i to
    /// jest cała prawda o tym stanie: UI, który natychmiast pokazuje nową wartość jako fakt,
    /// kłamie o tym, co dzieje się na maszynie.
    running: AtomicUsize,
}

impl Pool {
    /// Slot wraca do puli. Tu, i tylko tu, obniżenie suwaka wchodzi w życie.
    fn give_back(&self) {
        self.running.fetch_sub(1, Ordering::SeqCst);
        // SZKIELET — zwrot bezwarunkowy. Wersja docelowa oddaje permit dopiero wtedy, kiedy po
        // tym zwolnieniu naprawdę biegnie mniej, niż mówi suwak; do tego czasu slot znika po
        // cichu. Obniżenie NIGDY nie zabija tego, co już biegnie.
        self.slots.add_permits(1);
    }
}

/// Suwak „ile naraz" nad wspólną pulą miejsc.
///
/// Klon dzieli tę samą pulę — to jest cały mechanizm „jeden limiter na aplikację".
/// Nie twórz drugiej puli „na razie per bieg, potem się scali": po scaleniu nikt nie sprawdzi,
/// czy stara zniknęła.
#[derive(Clone, Debug)]
pub struct Limiter {
    pool: Arc<Pool>,
}

impl Limiter {
    /// Nowa pula. Wartość spoza `1..=8` jest przycinana **tutaj**, nie w kontrolce: ta liczba
    /// wraca też z pliku biegu (`runs.concurrency`) i z zapisanego workflow, a tamtędy nie
    /// przechodzi przez żaden suwak.
    #[must_use]
    pub fn new(at_once: usize) -> Self {
        let _ = at_once;
        // SZKIELET — stały limit 1 i żadnego przycinania.
        Self {
            pool: Arc::new(Pool {
                slots: Semaphore::new(MIN_AT_ONCE),
                at_once: AtomicUsize::new(MIN_AT_ONCE),
                running: AtomicUsize::new(0),
            }),
        }
    }

    /// Ile ma biec naraz — liczba, którą pokazuje suwak.
    #[must_use]
    pub fn at_once(&self) -> usize {
        self.pool.at_once.load(Ordering::SeqCst)
    }

    /// Ile biegnie naraz **naprawdę**, w tej chwili.
    #[must_use]
    pub fn running_now(&self) -> usize {
        self.pool.running.load(Ordering::SeqCst)
    }

    /// Przesuwa suwak w trakcie biegu.
    ///
    /// W górę: nowe miejsca są do wzięcia od razu. W dół: nic nie ginie, a nadmiar schodzi
    /// dopiero przy zwalnianiu (`[T7 §7.1]`).
    pub fn set_at_once(&self, at_once: usize) {
        let _ = at_once;
        // SZKIELET — suwak zapisuje stałą zamiast tego, o co poproszono, i nie dokłada ani
        // nie zabiera ani jednego miejsca. Limiter, który poprawnie liczy miejsca, ale nikt
        // nigdy nie prosi o więcej niż jedno, to defekt poprzedniego prototypu: liczba rośnie, nakładanie
        // się w czasie nie (niezmiennik 11).
        self.pool.at_once.store(MIN_AT_ONCE, Ordering::SeqCst);
    }

    /// Bierze miejsce z puli, czekając, aż będzie wolne.
    ///
    /// Prywatne z rozmysłem: jedyne wejście do puli prowadzi przez [`Run::dispatch`], więc nie
    /// da się wziąć slotu z pominięciem pauzy biegu.
    async fn take_slot(&self) -> Slot {
        if let Ok(permit) = self.pool.slots.acquire().await {
            // Permit zapominamy, bo o zwrocie decyduje [`Pool::give_back`], nie `Drop` permitu.
            permit.forget();
        }
        // `Err` znaczy zamkniętą pulę, czyli gaszenie aplikacji. Nie ma wtedy do czego wracać
        // i nie ma komu tego zgłosić — slot i tak zaraz zginie razem z biegiem.
        self.pool.running.fetch_add(1, Ordering::SeqCst);
        Slot {
            pool: Arc::clone(&self.pool),
        }
    }
}

/// Jeden bieg widziany przez limit dostawcy: jego status, jego kroki i wspólna pula miejsc.
///
/// **Dlaczego kroki są tutaj.** Pauza jest stanem BIEGU i nie ma prawa dotknąć ani jednego
/// kroku (`docs/ARCHITECTURE.md` §5, `[T7 §9.3]`). Asercja o czymś, czego typ i tak zabrania,
/// niczego nie dowodzi — więc ta struktura ma pełny dostęp do statusów kroków i do numerów
/// podejść, a kryteria AC-3 i AC-4 dowodzą, że ich nie rusza. Wersja, która przy pauzie
/// oznacza trwające kroki jako `failed`, jest tutaj **wyrażalna** i dlatego jest łapalna;
/// `[T7 §7.2]` nazywa ją wprost błędem („a pause, not a failure; do not mark steps failed").
#[derive(Debug)]
pub struct Run {
    /// Uchwyt do wspólnej puli. Klon, nie własna pula — patrz nagłówek pliku.
    limiter: Limiter,
    /// Do kiedy nic nie wychodzi. `None` znaczy „bieg wysyła".
    ///
    /// Chwila na **zegarze wykonania**, nie sekundy uniksowe z drutu: tylko ona idzie za czasem
    /// wirtualnym w testach i tylko ona przeżywa przestawienie zegara maszyny w trakcie pauzy.
    /// Liczbę z drutu, tę, z której UI robi godzinę lokalną, niesie [`Gate::PausedUntil`].
    paused_until: Option<Instant>,
    steps: Vec<Step>,
}

/// Krok tak, jak widzi go limit dostawcy: stan i numer podejścia. Nic więcej stąd nie widać
/// i nic więcej nie jest do niczego potrzebne.
#[derive(Debug, Clone, Copy)]
struct Step {
    state: StepState,
    attempt: u32,
}

impl Run {
    /// Nowy bieg w stanie `running`, z podanymi stanami kroków i podejściem 1 dla każdego.
    #[must_use]
    pub fn new(limiter: Limiter, steps: &[StepState]) -> Self {
        Self {
            limiter,
            paused_until: None,
            steps: steps
                .iter()
                .map(|&state| Step { state, attempt: 1 })
                .collect(),
        }
    }

    /// Status biegu — **wyliczany, nie zapamiętany**.
    ///
    /// Pauza kończy się sama o `resetsAt`, więc powrót do `running` nie potrzebuje ani zadania
    /// w tle, ani budzika. To nie jest oszczędność, tylko granica: przejście, które nie ma
    /// własnego kodu, nie ma też jak przy okazji ruszyć kroku ani podbić podejścia.
    #[must_use]
    pub fn status(&self) -> RunStatus {
        match self.paused_until {
            Some(deadline) if Instant::now() < deadline => RunStatus::Paused,
            _ => RunStatus::Running,
        }
    }

    /// Stany kroków, w kolejności z [`Run::new`].
    #[must_use]
    pub fn step_states(&self) -> Vec<StepState> {
        self.steps.iter().map(|step| step.state).collect()
    }

    /// Numery podejść kroków, w tej samej kolejności.
    #[must_use]
    pub fn attempts(&self) -> Vec<u32> {
        self.steps.iter().map(|step| step.attempt).collect()
    }

    /// Wchodzi surowe `rate_limit_info` i chwila, w której je zobaczyliśmy.
    ///
    /// `now_unix` jest argumentem, bo `resetsAt` przychodzi z drutu jako czas ścienny, a ten
    /// plik ma dać się przetestować bez zegara maszyny — tak samo, jak daje się przetestować
    /// bez okna.
    pub fn saw_rate_limit(&mut self, info: &Value, now_unix: i64) -> Gate {
        let _ = now_unix;
        // SZKIELET — decyzja idzie z [`read_gate`] (dziś zawsze otwarta), a bieg zaraz o niej
        // zapomina: pauza nie zostaje nigdzie zapisana, więc wysyłka nie ustaje ani na chwilę.
        self.paused_until = None;
        read_gate(info)
    }

    /// Prośba o miejsce dla jednego kroku. Jedyne wejście do puli.
    ///
    /// W pauzie odmawia **od razu i wartością**; poza pauzą czeka na wolne miejsce.
    pub async fn dispatch(&self) -> Dispatch {
        // SZKIELET — nikt tu nie pyta o pauzę, więc wysyłka nie zatrzymuje się nigdy.
        Dispatch::Granted(self.limiter.take_slot().await)
    }
}
