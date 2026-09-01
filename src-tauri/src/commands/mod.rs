//! Warstwa komend: co się dzieje, kiedy człowiek naciśnie Start, Stop albo Continue.
//!
//! **W tym katalogu nie ma ani jednego `#[tauri::command]` i ani jednego `use tauri::`.**
//! `docs/ARCHITECTURE.md` §3 daje słowo „Tauri" wyłącznie plikowi `ipc.rs`, a mapa własności daje
//! ten katalog zadaniu T-15. Godzimy to tak: tutaj mieszkają **wyłącznie** funkcje `*_inner`
//! biorące [`RunDeps`], a dwuliniowe skorupy `#[tauri::command]` i jedna lista
//! `generate_handler!` należą do T-07. Powód jest testowy, nie estetyczny: `State<'_, AppState>`
//! nie da się zbudować w teście jednostkowym, a `&RunDeps` da się [04 §2.1].
//!
//! 2026-08-16 — zdanie wyżej mówiło „należą do T-07". T-07 wylądował z ośmioma zielonymi
//! kryteriami o pompie i **bez ani jednej skorupy**, bo żadne kryterium nie sięgało szwu:
//! `Failed to launch` jest na liście `NOT_A_REAL_RED`, więc nic, co wymaga żywego Tauri, nie
//! może być kryterium. Adresatem jest T-27 i tam ten dług jest spłacany razem z dowodem, który
//! nie potrzebuje okna: `src-tauri/commands.golden.txt` czytany z obu stron granicy.
//!
//! # Co gdzie mieszka
//!
//! Ten plik to **typy i uchwyty**: [`RunDeps`], [`RunRequest`], [`RunReport`], [`RunError`]
//! i [`RunControl`] — czyli wszystko, czym woła się bieg i czym sięga się do niego w trakcie.
//! Same trzy funkcje biegu (`run_workflow_inner`, `stop_run_inner`, `continue_run_inner`)
//! siedzą w [`run`], razem z całym zapisem `run.json`.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::engine::drivers::AgentDriver;
use crate::engine::limits::Limiter;
use crate::engine::step::StepState;
use crate::library::agents::{AgentError, Vendor};
use crate::store::{Store, StoreError};
use crate::workflow::check::Note;
use crate::workflow::file::LoadError;

/// Biblioteka agentów: wypisz, zapisz, usuń. Wypełnia T-27.
pub mod agents;
/// Rozmowa z orchestratorem — i jedyne miejsce, które NIE umie uruchomić biegu.
pub mod chat;
/// Allowlistowany raport wsparcia dla aktywnego workspace. Wypelnia T-34.
pub mod diagnostics;
/// Praca kilku kroków zniesiona do jednej kopii — i odmowa, kiedy dwa z nich piszą co innego.
mod fan_in;
/// Przekazania między krokami: co jeden krok oddał następnemu, odczytane z plików.
pub mod handoffs;
/// Historia biegów TEGO projektu: co tu już ruszyło i co z tego zostało na dysku.
pub mod history;
pub mod import;
pub mod isolate;

/// Lab: zestawy przypadkow, kandydatki i macierz wynikow.
pub mod lab;
/// Pamięć: weź notatkę do użytku i przestań jej używać. Wypełnia T-27.
pub mod memory;
/// Mennica identyfikatorów uuid v7 — jedna dla wszystkich sekcji. Wypełnia T-27.
pub mod mint;
/// Rzeczy, które Loadout uruchomił dla człowieka: rejestr, kafelki, dowód śmierci. Wypełnia T-72.
pub mod processes;
/// Uzgodnienie biegów z plikami przy otwarciu folderu — po awarii aplikacji.
pub mod reconcile;
pub mod rerun;
pub mod run;
/// Co Loadout robi domyślnie, kiedy człowiek nie powiedział inaczej. Dziś: kto prowadzi rozmowę.
pub mod settings;
/// Umiejętności: przeczytaj link, zainstaluj przejrzane. Wypełnia T-27.
pub mod skills;
/// Zrodla zdarzen, ktore pytaja zewnetrzny serwis i pamietaja kursor w pliku.
pub mod triggers;
/// Pliki workflow: wczytaj, zapisz, sprawdź. Wypełnia T-27.
pub mod workflows;
pub mod workspaces;

/// Chwila **teraz** w ISO 8601 UTC — to, co `memory::notes::Actor::You` nazywa `at`.
///
/// Zegar stoi tutaj, w warstwie komend, bo `memory::notes` go świadomie nie ma: `at` opisuje
/// chwilę, w której **człowiek** kliknął, a moduł, który sam czyta zegar, nie da się przetestować
/// bez czekania. Okno też go nie podaje — front, który stempluje czas zapisu, stempluje czas
/// SWOJEGO zegara, a plik ma nieść jeden.
///
/// 2026-08-16 — algorytm dni→data (proleptyczny kalendarz gregoriański, era 400-letnia) stoi
/// w tym drzewie trzeci raz, obok `memory::handoff::now_utc` i `commands::run::stamp`. To nie
/// jest przeoczenie: tamta pierwsza jest **prywatna** w pliku, który nie należy do tego zadania,
/// a `chrono`/`time` odpadają, bo `src-tauri/Cargo.toml` też nie jest nasz (AGENTS.md §7). Trzy
/// kopie jednego rachunku to jest rzecz do zgłoszenia człowiekowi, nie do rozstrzygnięcia po
/// cichu w cudzym pliku.
#[must_use]
pub fn now_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());

    let (days, rest) = (secs / 86_400, secs % 86_400);
    let (hour, minute, second) = (rest / 3600, (rest % 3600) / 60, rest % 60);

    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Skąd bieg bierze sterownik dla vendora, którym biegnie agent kroku.
///
/// Funkcja, nie mapa: vendorów jest dwóch od pierwszego dnia (decyzja D3), a trzeci ma wejść bez
/// wydania Loadouta. Uchwyt jest `Arc`iem, bo zadanie każdego kroku dostaje własny klon — planista
/// wymaga od domknięcia `'static` (`engine::scheduler::execute`), więc pożyczka tu nie przejdzie.
pub type Drivers = Arc<dyn Fn(Vendor) -> Arc<dyn AgentDriver> + Send + Sync>;

/// Współpracownicy **jednego** biegu.
///
/// `RunDeps` zamiast globalnego `AppState` i to jest cała różnica między kryterium, które da się
/// napisać, a kryterium, które potrzebuje okna: `State<'_, AppState>` nie da się zbudować
/// w teście, a tę strukturę da się w sześciu wierszach. `AppState` po stronie Tauri (T-01/T-07)
/// tylko ją składa.
pub struct RunDeps<'a> {
    /// `~/.loadout` — biblioteka użytkownika: `agents/`, `workflows/`, `skills/`
    /// (`docs/ARCHITECTURE.md` §8). Przychodzi **argumentem**, nigdy z `HOME` czytanego w środku:
    /// katalog domowy odczytany tutaj znaczyłby, że każdy test pisze do prawdziwej biblioteki.
    pub home: &'a Path,
    /// Katalog projektu, w którym biegnie workflow. To pod nim ląduje
    /// `.loadout/runs/<ts>__<id>/`.
    pub project: &'a Path,
    /// Indeks biegu. **Nie jest prawdą** (niezmiennik 4): wszystko, co tu wchodzi, musi dać się
    /// odtworzyć z `run.json` i `logs/`, bo `loadout.db` wolno skasować.
    pub store: &'a Store,
    /// Fabryka sterowników. Uchwyt, nie pożyczka — patrz [`Drivers`].
    pub drivers: Drivers,
    /// Rejestr rzeczy, które mają zostać żywe po swoim kroku.
    ///
    /// 2026-08-23 — doszedł dla kafelka „uruchom i zostaw". Uchwyt, nie pożyczka: bieg żyje
    /// dłużej niż wywołanie, które go zaczęło, a proces ma przeżyć jeszcze dłużej niż bieg.
    pub processes: std::sync::Arc<processes::Processes>,
    /// Uchwyt do tego biegu: Stop i Continue sięgają nim do środka.
    ///
    /// 2026-08-16 — `TASK.md` wymienia w tym miejscu `CancellationToken` i on tu jest, wewnątrz
    /// [`RunControl`] (`RunControl::cancel_token`). Osobny typ, bo token umie powiedzieć
    /// dokładnie jedno słowo — „stop" — a punkt kontrolny potrzebuje drugiego: „dalej"
    /// (T3 §6.1 reguła 5). Dwa tokeny obok siebie w tej strukturze byłyby tym samym typem
    /// z dwoma znaczeniami, czyli parą, którą prędzej czy później ktoś zamieni miejscami.
    pub control: RunControl,
}

impl fmt::Debug for RunDeps<'_> {
    /// Ręcznie, bo [`Drivers`] jest domknięciem i `Debug` nie ma dla niego nic do powiedzenia.
    /// `missing_debug_implementations` jest w `Cargo.toml` ostrzeżeniem, a bramka woła clippy
    /// z `-D warnings`, więc „ta struktura po prostu nie ma `Debug`" nie jest tu wyjściem.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RunDeps")
            .field("home", &self.home)
            .field("project", &self.project)
            .field("drivers", &"<factory>")
            .field("control", &self.control)
            .finish_non_exhaustive()
    }
}

/// Uchwyt do żywego biegu — to, czym Stop i Continue sięgają do środka.
///
/// Klon dzieli te same sygnały; jeden bieg, jeden uchwyt, dowolnie wiele klonów.
#[derive(Clone, Debug)]
pub struct RunControl {
    inner: Arc<Signals>,
}

/// Trzy sygnały jednego biegu.
#[derive(Debug)]
struct Signals {
    /// Token **tego** biegu, nigdy globalny `AtomicBool` (niezmiennik 7): bool przecieka między
    /// biegami, więc drugi bieg po anulowanym startuje jako już anulowany i kończy się
    /// w milisekundach z samymi `Cancelled` — co wygląda jak szybki bieg, a nie jak awaria.
    cancel: CancellationToken,
    /// Ile razy człowiek powiedział „dalej". **Licznik, nie flaga**: bieg z dwoma punktami
    /// kontrolnymi przeszedłby przez drugi bez pytania, gdyby zgoda była flagą, która raz
    /// zapalona zostaje zapalona. Pytanie, które nie pyta, jest gorsze od jego braku.
    go_on: watch::Sender<u64>,
    /// Czy bieg **stoi** na punkcie kontrolnym.
    ///
    /// Tu jest właściciel tego faktu; `"status": "paused"` w `run.json` jest jego trwałym
    /// lustrem, bo stan, który nie dociera na dysk, nie przeżywa awarii aplikacji
    /// (niezmiennik 4). `paused` jest stanem **biegu** i nigdy stanem kroku
    /// (`docs/ARCHITECTURE.md` §5).
    paused: watch::Sender<bool>,
    /// Głosy żywych kroków: nazwa kroku → nadajnik do jego sesji.
    ///
    /// 2026-08-18 — POWSTAŁO, ŻEBY OKNO MIAŁO JAK NAPISAĆ DO AGENTA, KTÓRY PRACUJE. Zgłoszenie
    /// właściciela było jednozdaniowe („dalej nie działa pisanie do agenta przez terminal"), a
    /// przyczyna dwuwarstwowa: `stdin` był polem uchwytu, więc pisanie wymagało `&mut` (naprawione
    /// w `engine::drivers`, patrz `Voice`), i **nikt poza pętlą tury nie miał tego uchwytu**.
    /// Bieg zna swoje kroki, więc to on jest miejscem, w którym te głosy mogą leżeć.
    ///
    /// Kluczem jest NAZWA KROKU — ta sama, którą widzi człowiek w strumieniu i na kafelku szyny
    /// (`forward(…, plan.steps[id].name)`). Identyfikator wewnętrzny byłby kluczem, którego okno
    /// nigdy nie widziało, więc nie dałoby się go wpisać.
    ///
    /// `BTreeMap`, nie `HashMap`: kolejność jest deterministyczna, a lista nazw jedzie w zdaniu
    /// odmowy („powiedz, do którego") — dwie odpowiedzi w różnej kolejności na to samo pytanie
    /// czytają się jak dwie różne odpowiedzi.
    ///
    /// `std::sync::Mutex` i nigdy trzymany przez `await` (niezmiennik 8): każde wzięcie mieści się
    /// w jednym wyrażeniu, które kopiuje nadajnik albo listę nazw i oddaje zamek.
    voices: Mutex<BTreeMap<String, crate::engine::drivers::Voice>>,
    /// Co człowiek napisał, odpowiadając na punkt kontrolny — do odebrania RAZ.
    ///
    /// 2026-08-18 — POWSTAŁO, BO ODPOWIEDŹ NIE DOCHODZIŁA NIGDZIE. `go_on` podbijał licznik
    /// i to było wszystko: pytanie znikało z ekranu, bieg ruszał, a treść odpowiedzi nie trafiała
    /// ani do promptu następnego kroku, ani na dysk. Człowiek pisał zdanie, po którym nic się
    /// nie działo — czyli najgorszy rodzaj kontrolki, ta, która KŁAMIE.
    ///
    /// `std::sync::Mutex`, nigdy trzymany przez `await` (niezmiennik 8): każde wzięcie tego
    /// zamka mieści się w jednym wyrażeniu, które zabiera wartość i oddaje zamek.
    ///
    /// Do odebrania raz, bo odpowiedź należy do TEGO punktu kontrolnego. Wartość zostawiona
    /// w polu weszłaby w prompt następnego kroku po następnym „dalej", w którym człowiek nic
    /// nie napisał — czyli powtórzyłaby zdanie sprzed dziesięciu minut jako świeże.
    answer: Mutex<Option<String>>,
    /// Zapalane, kiedy bieg naprawdę zszedł — po ostatnim kroku, nie po wysłaniu Stopu.
    /// Bez tego `stop_run_inner` mówiłby „zatrzymane" w chwili, w której wysłał sygnał
    /// (niezmiennik 6: dopóki nie ma dowodu, traktujemy jako żywe).
    settled: CancellationToken,
    /// Zapalane, kiedy bieg **wszedł do roboty** — raz, przed wczytaniem pliku.
    ///
    /// # Po co to jest, skoro jest już [`Signals::settled`]
    ///
    /// Bo `settled` odpowiada na „czy bieg zszedł", a to jest inne pytanie niż „czy jest co
    /// zatrzymywać". Świeży [`RunControl`] ma oba znaczniki zgaszone, więc uchwyt biegu, którego
    /// nikt nigdy nie uruchomił, jest **nieodróżnialny** od biegu w trakcie — a `stop_run_inner`
    /// czeka wtedy na dowód, który nigdy nie zapadnie (jego własna dokumentacja nazywa ten
    /// warunek wołającemu).
    ///
    /// # Czego to naprawia
    ///
    /// Zgłoszenie właściciela 2026-08-19: „co się dzieje jak zamykasz apkę a leci jakiś workflow?
    /// on się wyłączy?". Nie wyłączał się: w `lib.rs` nie było ani jednej obsługi zamknięcia okna,
    /// więc agenci przechodzili pod PID 1 i dalej palili limit (`recovery.rs`, nagłówek), aż do
    /// następnego uruchomienia Loadouta. Zamknięcie ma dziś zatrzymać bieg z dowodem — ale żeby
    /// móc to zrobić bezpiecznie, musi najpierw umieć zapytać, czy w ogóle jest co zatrzymywać.
    began: CancellationToken,
    /// Strumień linii TEGO biegu — żeby zdarzenie spoza pętli kroku miało jak do niego wejść.
    ///
    /// # Po co to jest
    ///
    /// Tura człowieka („siema") jest zdarzeniem biegu, ale powstaje POZA nim: przychodzi komendą
    /// z okna, w chwili, w której pętla kroku czeka na agenta. Bez tego pola nie ma jak jej
    /// wpisać do historii, i to była przyczyna zgłoszenia „na pewno nie widać moich wiadomości" —
    /// zdanie dochodziło do modelu i nie pojawiało się na ekranie, więc wiersz wejścia wyglądał
    /// na martwy.
    ///
    /// `Option`, bo uchwyt biegu istnieje przed biegiem: dopóki nikt nie ruszył, nie ma strumienia,
    /// do którego można by pisać. `None` znaczy „nie ma gdzie tego pokazać" i jest odpowiedzią,
    /// nie awarią — zdanie i tak dojdzie do agenta.
    ///
    /// `std::sync::Mutex` i nigdy trzymany przez `await` (niezmiennik 8): wzięcia mieszczą się
    /// w jednym wyrażeniu, bo [`crate::ipc::LineSink`] jest klonowalny, a jego `send` jest
    /// synchroniczny i nie blokuje producenta.
    heard: Mutex<Option<crate::ipc::LineSink>>,
    /// Wspólna pula miejsc **całej aplikacji** — „ile naraz" (niezmiennik 11).
    ///
    /// # Dlaczego pula jedzie TĘDY, a nie polem [`RunDeps`] (2026-08-24, T-94)
    ///
    /// Bo `RunDeps` jest strukturą, którą buduje się literałem, a literał `RunDeps { … }` stoi
    /// w tym drzewie w **84 miejscach w 58 plikach** (zmierzone 2026-08-24), z czego wszystkie
    /// poza `ipc.rs` leżą w plikach kryteriów cudzych zadań. Nowe pole przewróciłoby je co do jednego, a `AGENTS.md` §7
    /// mówi, co wtedy zrobić: zatrzymać się, nie przepisywać cudzych plików. Ten uchwyt
    /// powstaje wywołaniem ([`RunControl::new`]), więc dołożenie go tutaj nie zmienia ani
    /// jednego wołającego.
    ///
    /// To nie jest obejście na jedno miejsce: uchwyt biegu jest DOKŁADNIE tą rzeczą, którą
    /// aplikacja wręcza każdemu startowi ([`crate::ipc::AppState::begin_run`] wymienia go przy
    /// każdym biegu). Klon [`Limiter`] dzieli tę samą pulę, więc wpisanie do świeżego uchwytu
    /// klonu puli aplikacji znaczy, że **każdy** bieg tej aplikacji bierze miejsca z jednej puli,
    /// którymikolwiek drzwiami wszedł. Bieg, który zakłada sobie pulę sam, jest nie do
    /// odróżnienia od biegu, który robi po jednej na kartę — a wtedy dwie karty dają `2 × limit`
    /// agentów po ~583 MB, czyli zamrożony laptop (`docs/ARCHITECTURE.md` §6a).
    slots: Limiter,
}

/// Ile miejsc ma pula uchwytu, którego nikt jeszcze nie postawił przy żadnym żądaniu.
///
/// Jedno, nie osiem: uchwyt bez żądania nie zna liczby, którą wybrał człowiek, a pula
/// zaczynająca szeroko wypuściłaby przy pierwszym biegu więcej agentów, niż stoi na suwaku.
/// Pierwszy bieg podnosi ją do swojej liczby, zanim ruszy pierwszy krok.
const UNTIL_THE_FIRST_START: usize = 1;

impl RunControl {
    /// Świeży uchwyt: bieg jeszcze nie ruszył, nikt go nie zatrzymał i nikt nie powiedział
    /// „dalej".
    #[must_use]
    pub fn new() -> Self {
        // Własna pula, bo ten uchwyt nie dostał cudzej. Liczba jest tymczasowa i nie jest
        // wyborem: „ile naraz" przychodzi z żądaniem człowieka i ustawia ją pierwszy bieg
        // (`run::run_workflow_inner`), a suwak przed pierwszym Startem nie mówi jeszcze nic.
        Self::sharing(Limiter::new(UNTIL_THE_FIRST_START))
    }

    /// Świeży uchwyt biegu, który bierze miejsca z **cudzej** puli.
    ///
    /// Tędy, i tylko tędy, aplikacja wręcza biegowi swoją jedyną pulę
    /// ([`crate::ipc::AppState::begin_run`]). Powód, dla którego pula mieszka w uchwycie biegu,
    /// a nie w polu [`RunDeps`], stoi w całości przy [`Signals::slots`].
    #[must_use]
    pub fn sharing(slots: Limiter) -> Self {
        Self {
            inner: Arc::new(Signals {
                cancel: CancellationToken::new(),
                go_on: watch::Sender::new(0),
                answer: Mutex::new(None),
                voices: Mutex::new(BTreeMap::new()),
                paused: watch::Sender::new(false),
                settled: CancellationToken::new(),
                began: CancellationToken::new(),
                heard: Mutex::new(None),
                slots,
            }),
        }
    }

    /// Pula miejsc, z której ten bieg ma brać — klon, więc ta sama pula.
    #[must_use]
    pub fn slots(&self) -> Limiter {
        self.inner.slots.clone()
    }

    /// Bieg wszedł do roboty. Woła to [`run::run_workflow_with_slots`], obok `settle()`.
    pub fn begin(&self) {
        self.inner.began.cancel();
    }

    /// Tędy bieg pokazuje linie oknu — zapisane raz, przy starcie.
    pub fn lines_go_to(&self, lines: crate::ipc::LineSink) {
        *self
            .inner
            .heard
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(lines);
    }

    /// Bieg zszedł: uchwyt PORZUCA nadajnik.
    ///
    /// # Dlaczego to musi istnieć, a nie wystarczy `Option` sam z siebie
    ///
    /// Zmierzone 2026-08-19, natychmiast po dołożeniu [`Signals::heard`]: **piętnaście testów
    /// biegu zawisło** („the run did not finish within 10s"). Pompa kończy się dopiero, gdy zniknie
    /// KAŻDY [`crate::ipc::LineSink`] — a uchwyt biegu trzymał klon w tym polu, więc kolejka nigdy
    /// się nie zamykała, `spawn_pump` nigdy nie oddawał bilansu i bieg nie wracał NIGDY.
    ///
    /// To jest ta sama klasa błędu, którą tego samego dnia naprawiono w
    /// `engine::drivers::claude::close()`: żywy klon nadajnika trzyma kanał otwarty, a czekanie na
    /// jego koniec jest wtedy czekaniem na siebie. Dlatego stoi tu osobna metoda, a nie założenie,
    /// że „kiedyś się posprząta".
    pub fn lines_go_quiet(&self) {
        *self
            .inner
            .heard
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// Wpisuje wiersz do historii tego biegu; `false`, kiedy nie ma gdzie.
    ///
    /// Odpowiedź jest wartością, nie błędem (niezmiennik 7): bieg, który zszedł między jednym
    /// a drugim naciśnięciem Enter, nie jest awarią, a zdanie i tak zostało wysłane.
    pub fn show_in_the_run(&self, line: crate::engine::line::Line) -> bool {
        let lines = self
            .inner
            .heard
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        // Klon POD zamkiem, wysyłka NAD nim: `send` jest synchroniczny, ale trzymanie zamka przez
        // cudzy kod jest tym, z czego robi się zakleszczenie, którego nikt nie umie odtworzyć.
        lines.is_some_and(|lines| lines.send(line) == crate::ipc::Sent::Queued)
    }

    /// Czy jest co zatrzymywać: bieg ruszył i jeszcze nie zszedł.
    ///
    /// Odpowiedź jest złożona z DWÓCH znaczników, bo żaden z nich sam jej nie daje: `began`
    /// zgaszone znaczy „ten uchwyt nigdy nie prowadził biegu", a `settled` zapalone znaczy „już
    /// zszedł". Zatrzymywanie w którymkolwiek z tych stanów to czekanie na dowód, którego nikt
    /// nie zapali.
    #[must_use]
    pub fn is_working(&self) -> bool {
        self.inner.began.is_cancelled() && !self.inner.settled.is_cancelled()
    }

    /// Token anulowania **tego** biegu. Klon dostaje planista i klon dostaje każdy krok —
    /// do środka, nie obok: zdjęcie zadania Rusta z zewnątrz zostawia żywy proces palący limit
    /// u dostawcy [T7 §3.1].
    #[must_use]
    pub fn cancel_token(&self) -> CancellationToken {
        self.inner.cancel.clone()
    }

    /// Człowiek nacisnął Stop.
    pub fn stop(&self) {
        self.inner.cancel.cancel();
    }

    /// Człowiek nacisnął Continue przy punkcie kontrolnym.
    pub fn go_on(&self) {
        self.go_on_with(None);
    }

    /// „Dalej" razem z tym, co człowiek napisał.
    ///
    /// Zapis odpowiedzi idzie PRZED podbiciem licznika i to nie jest kosmetyka: licznik jest
    /// tym, co budzi krok czekający na punkcie kontrolnym, więc odwrotna kolejność ma okno,
    /// w którym krok już ruszył, a odpowiedzi jeszcze nie ma czym odebrać.
    pub fn go_on_with(&self, answer: Option<String>) {
        {
            let mut slot = self
                .inner
                .answer
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            *slot = answer.filter(|said| !said.trim().is_empty());
        }
        self.inner.go_on.send_modify(|times| *times += 1);
    }

    /// Zapisuje głos kroku, dopóki ten krok żyje.
    ///
    /// Wołane po starcie sterownika, zdejmowane w [`RunControl::step_went_quiet`]. Krok bez głosu
    /// (dubler bez procesu, kafelek kontrolny) po prostu nie ma tu wpisu — a wtedy okno dostaje
    /// odpowiedź „nie da się", nie ciszę.
    pub fn step_can_hear(&self, step: &str, voice: crate::engine::drivers::Voice) {
        self.inner
            .voices
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(step.to_owned(), voice);
    }

    /// Zdejmuje głos kroku, który zszedł.
    ///
    /// Bez tego okno proponowałoby rozmowę z sesją, która już nie istnieje, i dowiadywałoby się
    /// o tym z ciszy — a cisza jest tu nieodróżnialna od agenta, który myśli.
    pub fn step_went_quiet(&self, step: &str) {
        self.inner
            .voices
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(step);
    }

    /// Głos tego kroku, jeśli krok jeszcze słucha.
    #[must_use]
    pub fn voice_of(&self, step: &str) -> Option<crate::engine::drivers::Voice> {
        self.inner
            .voices
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(step)
            .cloned()
    }

    /// Kroki, które w tej chwili słuchają — po nazwie, w kolejności alfabetycznej.
    #[must_use]
    pub fn who_is_listening(&self) -> Vec<String> {
        self.inner
            .voices
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .cloned()
            .collect()
    }

    /// Odbiera odpowiedź człowieka. `None`, kiedy nic nie napisał albo kiedy ktoś już ją zabrał.
    pub fn take_answer(&self) -> Option<String> {
        self.inner
            .answer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    /// Zakłada nasłuch na „dalej" i oddaje go wołającemu, **zanim** bieg ogłosi, że stoi.
    ///
    /// 2026-08-16 — to nie jest wariant [`RunControl::wait_for_go_on`] dla wygody, tylko jedyny
    /// kształt, w którym punkt kontrolny nie ma wyścigu. Pauza staje się widoczna przez
    /// `run.json` na dysku, a Continue przychodzi z zewnątrz w reakcji na to, co widać —
    /// więc kolejność „zapisz pauzę, potem zacznij słuchać" ma okno, w którym odpowiedź
    /// człowieka trafia do nikogo. Licznik podbity w tym oknie nie budzi nikogo, bo świeża
    /// subskrypcja `watch` liczy dopiero **następną** zmianę, i bieg stoi już do końca świata.
    ///
    /// Kolejność, która działa, jest odwrotna i egzekwuje ją typ: `GoOn` istnieje **przed**
    /// zapisem pauzy, bo bez tej wartości nie ma na czym czekać.
    #[must_use]
    pub fn listen_for_go_on(&self) -> GoOn {
        let mut told = self.inner.go_on.subscribe();
        // Liczba zapamiętana TERAZ jest tym, co odróżnia zgodę na **ten** punkt kontrolny
        // od zgody sprzed dziesięciu minut.
        let before = *told.borrow_and_update();
        GoOn {
            told,
            before,
            cancel: self.inner.cancel.clone(),
        }
    }

    /// Czeka, aż ktoś powie „dalej" **albo** zatrzyma bieg. Wraca `true`, kiedy padło „dalej".
    ///
    /// Nasłuch zaczyna się dopiero tutaj, więc ta droga jest dobra tam, gdzie nikt nie zdąży
    /// odpowiedzieć wcześniej, niż zaczniemy słuchać. Punkt kontrolny bierze
    /// [`RunControl::listen_for_go_on`] i powód stoi przy nim.
    pub async fn wait_for_go_on(&self) -> bool {
        self.listen_for_go_on().wait().await
    }

    /// Bieg stanął na punkcie kontrolnym i czeka na człowieka.
    pub fn pause(&self) {
        self.inner.paused.send_replace(true);
    }

    /// Bieg rusza dalej: pytanie ma odpowiedź albo przestało mieć znaczenie.
    pub fn resume(&self) {
        self.inner.paused.send_replace(false);
    }

    /// Czeka, aż bieg przestanie stać na punkcie kontrolnym.
    ///
    /// Wraca **od razu**, kiedy bieg nie stoi, i wraca też wtedy, gdy bieg zszedł — bo inaczej
    /// Continue naciśnięte w biegu bez pytania wisiałoby do końca świata, a przycisk, który
    /// zawiesza okno, jest gorszy od przycisku, który nic nie robi.
    pub async fn wait_until_moving(&self) {
        let mut paused = self.inner.paused.subscribe();
        loop {
            if !*paused.borrow_and_update() || self.inner.settled.is_cancelled() {
                return;
            }
            tokio::select! {
                biased;
                () = self.inner.settled.cancelled() => return,
                changed = paused.changed() => {
                    if changed.is_err() {
                        // Nadawca zginął razem z biegiem; nie ma na co czekać.
                        return;
                    }
                }
            }
        }
    }

    /// Bieg zszedł: wszystkie kroki są rozstrzygnięte i nic po nim nie żyje.
    pub fn settle(&self) {
        self.inner.settled.cancel();
    }

    /// Czeka na dowód z [`RunControl::settle`].
    pub async fn wait_until_settled(&self) {
        self.inner.settled.cancelled().await;
    }
}

impl Default for RunControl {
    fn default() -> Self {
        Self::new()
    }
}

/// Założony nasłuch na „dalej" — wartość, którą punkt kontrolny trzyma w ręku, zanim ogłosi
/// pauzę. Powód, dla którego to jest osobny typ, stoi przy [`RunControl::listen_for_go_on`].
#[derive(Debug)]
pub struct GoOn {
    /// Licznik zgód, obserwowany od chwili założenia nasłuchu.
    told: watch::Receiver<u64>,
    /// Ile razy padło „dalej", zanim ten punkt kontrolny zaczął słuchać.
    before: u64,
    /// Ten sam token, którym Stop kończy bieg: pytanie bez odpowiedzi musi dać się zamknąć.
    cancel: CancellationToken,
}

impl GoOn {
    /// Czeka, aż ktoś powie „dalej" **albo** zatrzyma bieg. Wraca `true`, kiedy padło „dalej".
    ///
    /// Bierze `self` przez wartość, bo nasłuch odpowiada na **jedno** pytanie: nasłuch użyty
    /// drugi raz odpowiadałby na drugie pytanie zgodą wydaną na pierwsze.
    pub async fn wait(mut self) -> bool {
        loop {
            if self.cancel.is_cancelled() {
                return false;
            }
            if *self.told.borrow_and_update() > self.before {
                return true;
            }
            tokio::select! {
                biased;
                () = self.cancel.cancelled() => return false,
                changed = self.told.changed() => {
                    if changed.is_err() {
                        // Nadawca zginął razem z biegiem; nie ma na co czekać.
                        return false;
                    }
                }
            }
        }
    }
}

/// Którą część grafu bieg ma wykonać.
///
/// # Dlaczego JEDNO pole z dwoma odpowiedziami, a nie dwa pola
///
/// Bo to jest jedno pytanie („co z tego grafu ma pobiec") i dwie odpowiedzi, które się WYKLUCZAJĄ.
/// Dwa pola obok siebie dałyby stan, w którym oba są wypełnione — a wtedy ktoś musi wybrać, które
/// wygrywa, i ten wybór żyje w kodzie zamiast w typie. Ta sama reguła i ten sam powód, co przy
/// [`RunControl`]: para tego samego kształtu z dwoma znaczeniami jest parą, którą prędzej czy
/// później ktoś zamieni miejscami.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    /// Dokładnie te kroki i nic poza nimi, po kluczu kafelka.
    ///
    /// **Wymienione kroki nie mają między sobą zależności.** Bieg złożony z jednego kroku nie ma
    /// po czym iść, a jego wejście przychodzi z przekazań poprzedniego biegu
    /// ([`RunRequest::handoffs_from`]) — czyli z tego samego miejsca, z którego przyszło za
    /// pierwszym razem. To jest „uruchom ten kafelek jeszcze raz".
    Just(Vec<String>),
    /// Ten krok i **wszystko, co graf stawia po nim**, ze strzałkami między nimi.
    ///
    /// 2026-08-23, pytanie właściciela nad ekranem historii: „a z history możemy kontynuować?".
    /// Bieg, który padł na siódmym kroku z dziesięciu, ma sześć skończonych kroków, których nikt
    /// nie chce powtarzać, i trzy, które nigdy nie ruszyły. [`Part::Just`] tego nie wyraża
    /// i wyrazić nie może: ona ZDEJMUJE strzałki, bo powtarzany kafelek nie ma po czym iść —
    /// a tutaj kroki po wskazanym mają iść po nim i po sobie nawzajem, dokładnie tak, jak
    /// narysował je człowiek.
    Onward(String),
}

/// Żądanie z interfejsu: co uruchomić i ile naraz.
#[derive(Debug, Clone)]
pub struct RunRequest {
    /// Plik workflow — **pełna ścieżka**, nie slug. Bieg nie ufa UI (T3 §5.2): ten plik mógł
    /// zostać zmergowany gitem albo poprawiony ręcznie między zapisem a naciśnięciem Start,
    /// więc jedyne, co o nim wiadomo na pewno, to gdzie leży.
    pub workflow: PathBuf,
    /// Ile kroków ma **naprawdę** działać naraz.
    ///
    /// Liczba przychodzi w żądaniu, nigdy ze stałej w kodzie (niezmiennik 11). Cicha wersja
    /// złamania nie wygląda jak zły algorytm — wygląda jak pole, które jest wczytywane,
    /// logowane i nigdzie nie podawane, a semafor dostaje `1`. Tak przegrał poprzedni prototyp.
    pub how_many_at_once: usize,
    /// Co ma zostać zbudowane w tym biegu — zdanie od człowieka, wspólne dla wszystkich kroków.
    ///
    /// # Po co to pole istnieje
    ///
    /// Zgłoszenie właściciela 2026-08-19: „jak ja mam np puścić jakieś workflow i przekazać
    /// prompta?". Do tego dnia bieg brał WYŁĄCZNIE to, co stało w pliku, więc workflow był
    /// jednorazowy: sześciu agentów ustawionych raz umiało zbudować dokładnie tę jedną rzecz,
    /// którą ktoś wcześniej wpisał w `instructions` każdego kroku. Kształt pracy (kto z kim,
    /// w jakiej kolejności) i treść pracy (co konkretnie robimy) są dwiema różnymi rzeczami,
    /// a plik trzymał je zlepione — i to jest powód, dla którego makieta obiecuje w wierszu
    /// wejścia `/run`, a nie tylko listę wyboru.
    ///
    /// `None` znaczy „bieg bez zadania z wiersza" i wtedy prompt kroku jest **co do bajtu** tym,
    /// co stoi w pliku. To nie jest wygoda dla wołających: pusty blok „TWOJE ZADANIE" nad
    /// promptem uczyłby model, że ta sekcja bywa pusta, i kosztowałby długość za nic — ten sam
    /// powód stoi przy [`run::with_what_we_know`].
    pub task: Option<String>,
    /// Która CZĘŚĆ grafu ma pobiec. `None` znaczy „cały".
    ///
    /// 2026-08-23 — POLE POWSTAŁO DLA PONOWNEGO ODPALENIA KROKU, na prośbę właściciela po biegu,
    /// który kosztował 48 minut i padł na ostatnim sprawdzeniu z powodu środowiskowego. Bez tego
    /// jedynym sposobem poprawienia jednego kroku było puszczenie całej dziesiątki od zera.
    pub part: Option<Part>,
    /// Katalog biegu, z którego ten bieg przejmuje przekazania na wejściu.
    ///
    /// Kopia, nie wskazanie: bieg pisze wyłącznie do swojego katalogu (`ARCHITECTURE` §8), a
    /// skończony `run.json` jest historią i nie ma prawa się zmienić dlatego, że ktoś powtórzył
    /// jeden krok. Ponowne odpalenie jest więc **nowym biegiem**, a nie dopisaniem do starego.
    pub handoffs_from: Option<PathBuf>,
}

/// Czym skończył się bieg.
///
/// **Wartość, nie `Err`** (niezmiennik 7): anulowanie jest jednym z normalnych zakończeń,
/// a `Err(Cancelled)` zmusza każdego wołającego do rozróżniania „to się nie udało" od „to
/// zatrzymał człowiek" — rozróżnienie zgubione raz jest zgubione wszędzie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Bieg doszedł do końca sam.
    Done,
    /// Bieg zatrzymał człowiek.
    Cancelled,
}

/// Co bieg zostawił po sobie wołającemu.
///
/// Wszystko, co tu stoi, stoi też w `run.json` — to nie jest duplikat, tylko dwa czasy: ta
/// struktura odpowiada wołającemu **teraz**, a plik odpowiada za tydzień, po skasowaniu bazy
/// (niezmiennik 4).
#[derive(Debug, Clone)]
pub struct RunReport {
    /// uuid v7 biegu — sortuje się po czasie.
    pub id: String,
    /// `<projekt>/.loadout/runs/<ts>__<id>/`.
    pub dir: PathBuf,
    /// Czym się skończył.
    pub outcome: Outcome,
    /// Stan końcowy każdego kroku, **w kolejności z pliku workflow**. Po powrocie nie ma tu
    /// prawa zostać `Pending`, `Ready` ani `Running`.
    pub steps: Vec<StepState>,
}

/// Czym bieg umie odmówić.
///
/// Każdy wariant jest osobnym zdaniem dla użytkownika, bo każdy naprawia się inaczej.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    /// Zamknięcie okna czekało na koniec biegu tyle, ile mu wolno, i się nie doczekało.
    ///
    /// Jedyny wariant tego wyliczenia, który mówi o CZASIE, a nie o tym, co się nie udało —
    /// i ma swój własny powód. Reszta drogi zamykania jest ograniczona co do sekundy
    /// (`engine::supervisor`: pięć sekund łaski, dwie na dowód po dziewiątce), więc czekanie
    /// dłuższe niż [`crate::commands::run::HOW_LONG_CLOSING_MAY_WAIT`] nie jest schodzeniem,
    /// które się przeciąga — jest zaciętym zadaniem, które nie zejdzie już nigdy.
    ///
    /// Bez sufitu okno wisi WTEDY: `prevent_close` jest już podniesione, `destroy()` czeka za
    /// tym wywołaniem, więc człowiek zostaje z aplikacją, której nie da się zamknąć, i jedynym
    /// wyjściem jest ubicie jej z zewnątrz — czyli dokładnie ta droga, która zostawia sieroty.
    ///
    /// Zdanie mówi też, co się z tym stanie, bo inaczej byłoby samym niepokojem: sprzątanie po
    /// zamkniętym oknie biegnie przy następnym starcie i naprawdę biegnie
    /// ([`crate::ipc::AppState::settle_everything_left_behind`]).
    #[error(
        "Loadout waited {seconds} seconds for this run to come to a stop and it did not, so the \
         window is closing anyway. Anything it left behind is written off, and its agents are \
         stopped, the next time you open Loadout."
    )]
    StillGoingAtClose {
        /// Ile na nie czekano, w sekundach — to samo, co widzi człowiek.
        seconds: u64,
    },
    /// Trwały ledger triggera odmówił związania albo akceptacji biegu.
    ///
    /// Własny wariant zachowuje zdanie z rdzenia triggerów i nie udaje błędu `SQLite`: pliki są
    /// prawdą tej dostawy, tak samo jak `run.json` jest prawdą biegu (niezmienniki 2 i 4).
    #[error(transparent)]
    Trigger(#[from] triggers::TriggerError),
    /// [`crate::workflow::check`] znalazło problem. **Nic nie ruszyło** — ani jeden proces,
    /// ani jeden katalog.
    ///
    /// Zdanie jest **tym samym zdaniem**, które zwrócił walidator, słowo w słowo. Własne
    /// tłumaczenie byłoby drugim miejscem, w którym mieszka ten sam komunikat, i jedno z nich
    /// zawsze jest nieaktualne (tak samo czyta to `workflow::file::SaveError::Refused`).
    #[error("{}", .0.message)]
    Refused(Note),
    /// Pliku workflow nie dało się wczytać.
    #[error(transparent)]
    Unreadable(#[from] LoadError),
    /// Krok nazywa agenta, którego nie da się przeczytać albo którego nie ma w bibliotece.
    #[error(transparent)]
    Agent(#[from] AgentError),
    /// Grafu nie dało się zbudować.
    #[error(transparent)]
    Graph(#[from] crate::engine::dag::DagError),
    /// Indeks odmówił.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// Katalog biegu albo `run.json` nie dały się zapisać.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Kroku w trybie „własna kopia twoich plików" nie dało się przygotować.
    ///
    /// WŁASNY WARIANT, a nie [`RunError::Io`], i to jest cała jego treść. `Io` jest
    /// przezroczysty, więc człowiek zobaczyłby „Permission denied (os error 13)" — zdanie,
    /// które mówi, co się nie udało systemowi, i nie mówi, co się nie udało JEMU. Tutaj
    /// odpowiedź brzmi: ten krok miał dostać kopię twoich plików, nie dostał, i dlatego
    /// nie ruszył.
    ///
    /// Bieg **musi** się na tym zatrzymać. Cicha degradacja do wspólnego katalogu jest
    /// groźniejsza niż odmowa: dwa kroki pisałyby wtedy po tych samych plikach — czyli
    /// robiły dokładnie to, czego `workflow::check` odmawia przy zapisie (niezmiennik 12) —
    /// a każdy z nich skończyłby się „sukcesem" i bramka nie miałaby jak tego zobaczyć.
    #[error(
        "step \"{step}\" was set to work on its own copy of your files, and Loadout could \
             not make that copy: {why}. Nothing ran: sharing one folder between steps would let \
             them overwrite each other's work."
    )]
    NoFreshCopy {
        /// Nazwa kroku, którego to dotyczy — człowiek szuka kafelka, nie identyfikatora.
        step: String,
        /// Co dokładnie odmówiło, zdaniem systemu plików.
        why: String,
    },
    /// W bibliotece nie ma ani jednego agenta, więc krok nie ma czym ruszyć.
    ///
    /// WŁASNY WARIANT, a nie [`RunError::Io`], i powód jest ten sam co przy
    /// [`RunError::NoFreshCopy`]: `Io` jest przezroczysty, więc człowiek czytał
    /// „No such file or directory (os error 2)" — zdanie, które mówi, co się nie udało
    /// systemowi plików, i nie mówi ani co się nie udało JEMU, ani co ma z tym zrobić.
    /// Zmierzone 2026-08-18: `~/.loadout/agents` nie istniał, bo zapis agenta padał cicho,
    /// a siedemnaście naciśnięć Start skończyło się dokładnie tym komunikatem.
    #[error(
        "No agents are saved yet, so \"{step}\" has nothing to run. Create an agent in \
         Agents, then pick it on the step."
    )]
    NoAgentsSaved {
        /// Nazwa kroku, który o agenta poprosił — człowiek szuka kafelka, nie identyfikatora.
        step: String,
    },
    /// Czegoś nie dało się zamienić w JSON albo z niego wyjąć.
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    /* ── ODMOWY ROZMOWY Z ŻYWYM AGENTEM ────────────────────────────────────────────────────
     *
     * Pięć wariantów, pięć różnych zdań, i to nie jest rozrzutność: każde z nich człowiek
     * naprawia inaczej. Jedno wspólne „could not send" kazałoby mu zgadywać, czy nic nie
     * pracuje, czy pomylił nazwę, czy ten krok właśnie skończył — a wiersz wejścia jest jedynym
     * miejscem, w którym te zdania widać (niezmiennik 14: zero żargonu).
     *
     * Powstały 2026-08-18 razem z przeniesieniem polityki z `#[tauri::command]` do
     * `run::say_to_agent_inner`; przedtem były pięcioma napisami sklejanymi w skorupie, na którą
     * nie dało się napisać kryterium. */
    /// Człowiek nacisnął Enter na pustym wierszu.
    #[error("Write something first, then press Enter.")]
    NothingToSay,
    /// Nic nie pracuje, więc nie ma z kim rozmawiać.
    #[error("No agent is working right now, so there is nobody to talk to. Press Start first.")]
    NobodyIsWorking,
    /// Pracuje kilku i nie powiedziano, do którego.
    ///
    /// Zdanie **wymienia nazwy**, bo odmowa, która nie mówi, z czego wybrać, zamienia jedno
    /// kliknięcie w zgadywanie.
    #[error(
        "{} agents are working, so say which one: put its name first, like \"{} …\".",
        names.len(),
        names.first().map_or("Builder", String::as_str)
    )]
    SeveralAreWorking {
        /// Kroki, które w tej chwili słuchają.
        names: Vec<String>,
    },
    /// Wskazany krok już zszedł, a nic innego nie pracuje.
    #[error("That agent already finished, so there is nothing listening any more.")]
    ThatOneFinished,
    /// Takiego kroku nie ma wśród pracujących — ale inne pracują.
    #[error("There is no agent called \"{name}\" working right now. These are: {}.", working.join(", "))]
    NoSuchAgentWorking {
        /// Nazwa, którą podało okno.
        name: String,
        /// Kroki, które naprawdę słuchają.
        working: Vec<String>,
    },
    /// Sesja przestała czytać wejście między wyborem adresata a wysyłką.
    #[error("\"{name}\" stopped listening before that could reach it.")]
    StoppedListening {
        /// Krok, do którego mówiliśmy.
        name: String,
    },
}
