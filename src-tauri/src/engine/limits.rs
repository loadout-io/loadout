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
//! **Dwie liczby, nie jedna** (2026-08-16). [`Limiter::at_once`] mówi, ile ma biec, a
//! [`Limiter::running_now`], ile biegnie. Po obniżeniu suwaka różnią się i tak ma być: ekran
//! pokazujący nową wartość jako fakt kłamie o tym, co w tej chwili zajmuje pamięć maszyny.
//! Zejście do nowej wartości dzieje się wyłącznie przy zwalnianiu miejsc — obniżenie suwaka
//! nie dotyka niczego, co już działa.

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

/// Ile gigabajtów maszyny przypada w podpowiedzi na jednego agenta.
///
/// Cztery przy zmierzonych 583 MB szczytowego RSS `[T7 §7.1, V]`, a nie „583 MB, więc zmieści
/// się ich tyle, ile razy wejdą": pozostała pamięć nie jest wolna. Bierze ją system, ta
/// aplikacja, przeglądarka i skoki, których szczyt nie widzi. Dzielnik jest częścią wzoru
/// z raportu, nie zaokrągleniem w wygodną stronę.
const GB_PER_AGENT: u64 = 4;

/// Przycięcie do `1..=8`, w jednym miejscu dla wszystkich wejść.
///
/// Ta sama liczba przychodzi z suwaka, z pliku biegu (`runs.concurrency`) i z zapisanego
/// workflow — a dwa z tych trzech wejść nigdy nie widziały żadnej kontrolki. Przycięcie
/// wyłącznie w komponencie znaczy „przycięte, dopóki nikt nie wznowi biegu".
fn clamp_at_once(at_once: usize) -> usize {
    at_once.clamp(MIN_AT_ONCE, MAX_AT_ONCE)
}

/// Podpowiedź przy pierwszym uruchomieniu: `clamp(total_gb / 4, 1, 8)` `[T7 §7.1]`.
///
/// Pamięć maszyny wchodzi **argumentem**. Kto ją zmierzy, potrzebuje `sysinfo`, czyli zmiany
/// `Cargo.toml` — a to jest moment na zatrzymanie się i zapytanie człowieka (`AGENTS.md` §7),
/// nie cichy dopisek.
#[must_use]
pub fn suggested_at_once(total_gb: u64) -> usize {
    // Dzielenie całkowite w dół, a potem podłoga: 2 GB wychodzą na zero agentów, a zero nie
    // jest odpowiedzią na pytanie „ilu naraz" — od zatrzymywania biegu jest Stop.
    // `try_from` zamiast `as`: tam, gdzie `u64` nie mieści się w `usize`, odpowiedzią jest
    // sufit, nie obcięte bity.
    let by_memory = usize::try_from(total_gb / GB_PER_AGENT).unwrap_or(MAX_AT_ONCE);
    clamp_at_once(by_memory)
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
    // Obie liczby są w sekundach, więc różnica też jest w sekundach i `from_secs` jest jedynym
    // konstruktorem, który tu pasuje. `from_millis` na tej samej parze daje 300 ms zamiast
    // pięciu minut — bieg wznawia się natychmiast i przepala resztę okna na odmowach.
    let seconds_left = resets_at_unix.saturating_sub(now_unix);
    // Limit, który już wrócił, to zero, nie liczba ujemna: `Duration` nie ma znaku, więc
    // odejmowanie w drugą stronę byłoby paniką — a panika w agentowym runtime zabiera
    // cały bieg (`AGENTS.md` §4).
    Duration::from_secs(u64::try_from(seconds_left).unwrap_or(0))
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
    let Some(status) = info.get("status").and_then(Value::as_str) else {
        tracing::debug!(
            shape = %info,
            "provider limit line has no status field; dropping the line and sending on"
        );
        return Gate::Open;
    };

    // Jedno porównanie i jedno pole. Nie ma tu `overageStatus`, nie ma
    // `overageDisabledReason` i nie ma sprawdzenia, czy obiekt w ogóle przyszedł: linia
    // z UDANEGO biegu niesie wszystkie trzy i mówi „allowed", a te zdarzenia to 1,3%
    // normalnego strumienia `[T7 §4.3, V]`.
    if status == "allowed" {
        return Gate::Open;
    }

    let Some(resets_at) = info.get("resetsAt").and_then(Value::as_i64) else {
        // Odmowa bez chwili powrotu jest kształtem, którego nie znamy: pauza bez końca to
        // bieg, który wisi, dopóki człowiek go nie zatrzyma, a to jest gorsze niż wysłanie
        // kroku, który znowu dostanie odmowę. Idzie do dziennika i zostaje porzucona
        // (niezmiennik 5). Gdyby ten kształt kiedykolwiek pojawił się na drucie, to jest
        // ta jedna linia do zmiany — i dopiero wtedy wiadomo, na co ją zmienić.
        tracing::debug!(
            status,
            shape = %info,
            "provider limit line refuses without saying when it comes back; dropping the line"
        );
        return Gate::Open;
    };

    Gate::PausedUntil(resets_at)
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
    /// Ile miejsc jeszcze trzeba połknąć, żeby obniżony suwak wszedł w życie.
    ///
    /// Obniżenie suwaka w chwili, w której wszystkie miejsca są zajęte, nie ma czego zabrać
    /// z puli — a zabranie tego, co już biegnie, byłoby anulowaniem agenta, o które nikt nie
    /// prosił. Więc różnica zostaje tutaj jako dług i spłaca ją pierwsze zwolnienie: slot
    /// wraca, ale nie do puli, tylko na spłatę.
    owed: AtomicUsize,
}

impl Pool {
    /// Suwak poszedł w górę: brakujące miejsca wchodzą do puli natychmiast.
    fn hand_out(&self, more: usize) {
        // Najpierw umorzenie długu z wcześniejszego obniżenia. Bez tego droga 4 → 2 → 4
        // zostawia pulę o dwa miejsca uboższą, niż pokazuje suwak, i to na stałe: dołożone
        // permity poszłyby przy zwalnianiu na spłatę długu, którego podniesienie już umorzyło.
        let forgiven = self.forgive(more);
        self.slots.add_permits(more - forgiven);
    }

    /// Suwak poszedł w dół. Nic nie ginie: schodzą miejsca, które akurat leżą wolne,
    /// a reszta zostaje długiem do spłacenia przy zwalnianiu.
    fn take_back(&self, fewer: usize) {
        let mut taken = 0;
        while taken < fewer {
            let Ok(free) = self.slots.try_acquire() else {
                // Nie ma już wolnych miejsc: cała reszta różnicy siedzi w rękach zadań,
                // które biegną, a tych nie ruszamy.
                break;
            };
            // `forget`, nie upuszczenie: upuszczony permit wraca do puli, a ten ma zniknąć.
            free.forget();
            taken += 1;
        }
        self.owed.fetch_add(fewer - taken, Ordering::SeqCst);
    }

    /// Umarza do `most` miejsc długu i mówi, ile umorzyła.
    ///
    /// Pętla CAS, a nie „odczytaj, odejmij, zapisz": dwa zadania kończące się w tej samej
    /// chwili czytałyby ten sam dług i spłaciły go dwa razy, po czym pula na zawsze byłaby
    /// o jedno miejsce mniejsza, niż mówi suwak.
    fn forgive(&self, most: usize) -> usize {
        let mut owed = self.owed.load(Ordering::SeqCst);
        loop {
            let paid = most.min(owed);
            if paid == 0 {
                return 0;
            }
            match self.owed.compare_exchange_weak(
                owed,
                owed - paid,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return paid,
                Err(seen) => owed = seen,
            }
        }
    }

    /// Slot wraca do puli. Tu, i tylko tu, obniżenie suwaka wchodzi w życie.
    fn give_back(&self) {
        // Najpierw licznik biegnących, dopiero potem miejsce. W odwrotnej kolejności następne
        // zadanie zdąży wystartować, zanim to policzy swoje wyjście, i `running_now()` pokaże
        // wtedy o jeden za dużo — akurat w chwili, w której ktoś patrzy, ile biegnie.
        self.running.fetch_sub(1, Ordering::SeqCst);
        if self.forgive(1) == 0 {
            self.slots.add_permits(1);
        }
        // Kiedy dług był niezerowy, miejsce znika po cichu i nikt go nie dostaje. To jest cała
        // różnica między „od teraz dwa naraz" a „zabij dwa biegnące": obniżenie suwaka nie
        // dotyka niczego, co już działa.
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
        let at_once = clamp_at_once(at_once);
        Self {
            pool: Arc::new(Pool {
                slots: Semaphore::new(at_once),
                at_once: AtomicUsize::new(at_once),
                running: AtomicUsize::new(0),
                owed: AtomicUsize::new(0),
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
        let wanted = clamp_at_once(at_once);
        let previous = self.pool.at_once.swap(wanted, Ordering::SeqCst);
        // Dwie różnice, z których co najwyżej jedna jest niezerowa, a obie operacje na zerze
        // są niczym. Zapisane bez `if`, bo `if w > p … else if w < p` to ten sam kod plus
        // trzecie ramię, którego nikt nigdy nie przeczyta.
        //
        // Sam zapis liczby to jest właśnie defekt poprzedniego prototypu: `max_parallel` był tam tylko
        // szerokością wysyłki, a nakładania się w czasie nie było wcale (niezmiennik 11).
        // Miejsce, w którym ten suwak przestaje być liczbą, jest o dwie linie niżej.
        self.pool.hand_out(wanted.saturating_sub(previous));
        self.pool.take_back(previous.saturating_sub(wanted));
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
    ///
    /// Porównania z zegarem nie ma tutaj, tylko w [`Run::still_paused_for`], i to jest ta sama
    /// zasada, co przy dwóch licznikach na ekranie: dwa miejsca liczące jedną granicę różnią
    /// się o milisekundę dokładnie wtedy, kiedy ta milisekunda decyduje.
    #[must_use]
    pub fn status(&self) -> RunStatus {
        if self.still_paused_for().is_zero() {
            RunStatus::Running
        } else {
            RunStatus::Paused
        }
    }

    /// Ile jeszcze bieg nie wysyła. `Duration::ZERO` znaczy „wysyła".
    ///
    /// Odpowiedź na pytanie **„to na jak długo"**, którego [`Run::status`] nie umie zadać.
    /// Wołający, który dostał [`Refusal::Paused`], ma zaczekać dokładnie tyle i spróbować raz —
    /// wersja pytająca co sto milisekund robi z pauzy odpytywanie i budzi bieg 3000 razy
    /// w pięciogodzinnym oknie limitu, żeby 2999 razy usłyszeć to samo.
    ///
    /// Liczba maleje monotonicznie do zera, bo pauzy nie da się odwołać: kończy ją wyłącznie
    /// upływ czasu do `resetsAt`, a zdarzenie ze statusem `allowed` jej nie skraca
    /// (patrz [`Run::saw_rate_limit`]).
    #[must_use]
    pub fn still_paused_for(&self) -> Duration {
        self.paused_until.map_or(Duration::ZERO, |deadline| {
            // `saturating_*`, bo pauza po terminie ma dawać zero, a nie liczbę ujemną, której
            // `Duration` i tak nie umie unieść — odejmowanie w drugą stronę byłoby paniką
            // w silniku (`AGENTS.md` §4).
            deadline.saturating_duration_since(Instant::now())
        })
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
        let gate = read_gate(info);
        if let Gate::PausedUntil(resets_at) = gate {
            let deadline = Instant::now() + duration_until_reset(resets_at, now_unix);
            // Dalsza z dwóch chwil, nigdy bliższa. Kroki, które biegną, strumieniują dalej
            // (pauza wstrzymuje wysyłkę, nie egzekucję), więc druga linia limitu potrafi
            // wejść zaraz po pierwszej i być od niej starsza. Skrócenie pauzy taką linią
            // wygląda potem jak „wznowiło się samo, o minutę za wcześnie", a szuka się tego
            // w zegarze, nie tutaj.
            self.paused_until = Some(self.paused_until.map_or(deadline, |set| set.max(deadline)));
        }
        // Zdarzenie ze statusem `allowed` NIE kończy trwającej pauzy i to jest decyzja, nie
        // przeoczenie: wznowienie ma jeden wyzwalacz, `resetsAt` `[T7 §7.2]`. Te zdarzenia to
        // 1,3% normalnego strumienia, a w pauzie strumieniują wciąż dwa czy trzy kroki —
        // pierwsze z nich skasowałoby pauzę milisekundy po jej wejściu.
        //
        // Żaden krok nie zmienia tu stanu i żaden nie dostaje podbitego podejścia. To nie jest
        // przeoczenie w drugą stronę: `[T7 §7.2]` nazywa wprost błędem wersję, która oznacza
        // kroki jako `failed` („a pause, not a failure"), a na ekranie wygląda ona jak bieg,
        // który się wywrócił na limicie, zamiast takiego, który na niego czeka.
        gate
    }

    /// Prośba o miejsce dla jednego kroku. Jedyne wejście do puli.
    ///
    /// W pauzie odmawia **od razu i wartością**; poza pauzą czeka na wolne miejsce.
    pub async fn dispatch(&self) -> Dispatch {
        if self.status() == RunStatus::Paused {
            // Odmowa przed `await`, nie po nim: czekanie na miejsce w biegu, który i tak nic
            // nie wyśle, zajmowałoby slot potrzebny komuś, kto może biec.
            return Dispatch::Refused(Refusal::Paused);
        }
        Dispatch::Granted(self.limiter.take_slot().await)
    }
}
