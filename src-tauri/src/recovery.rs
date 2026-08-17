//! Uzgodnienie stanu przy starcie: co zabić, co przepisać, o co zapytać.
//!
//! Agenci **nie giną razem z Loadoutem**. Po jego śmierci przechodzą pod PID 1 i dalej palą
//! limit [T7 §6.1, V]; zapisany `pgid` przeżywa i nadal daje się zabić z nowego procesu, i to
//! jest jedyny powód, dla którego odzyskiwanie w ogóle jest możliwe.
//!
//! # Ten plik nie wykonuje ani jednego wywołania systemowego
//!
//! To jest jego jedyne ograniczenie trzymające się w poprzek całej treści, i łamie się tutaj
//! najciszej z całego repo, bo odzyskiwanie *chce* zawołać `libc::kill`, `sysctl kern.boottime`
//! i `getpgrp()`. Każde z nich przewraca `checks/quick-boundary.sh` i zamienia port na Windows
//! z gałęzi `cfg` w przepisanie (niezmiennik 3). Wszystkie trzy wchodzą tu jako argumenty: czas
//! startu systemu i własny `pgid` przyjeżdżają w [`Machine`], a zabijanie w [`apply`] jako
//! domykacz `FnMut(i32) -> ReapOutcome`.
//!
//! Konsekwencja, którą trzeba nazwać: [`decide`] nie ma skąd wziąć ani czasu startu systemu,
//! ani własnego `pgid` po raz drugi, więc nie ma jak porównać wartości samej ze sobą — patrz
//! datowana notka przy [`Machine::boot_id`].
//!
//! 2026-08-16 — jedyne, po co ten plik sięga poza swoje dwa argumenty, to **świeży identyfikator
//! sesji** dla opcji „zacznij od nowa" ([`fresh_session`]). Jest to nazwane tutaj, bo inaczej byłoby
//! ciche: `Uuid::now_v7()` czyta zegar, a nie stan procesów, więc nie dotyka granicy, której
//! pilnuje niezmiennik 3 — `checks/quick-boundary.sh` szuka `#[cfg(unix)]`, a tutaj nie ma ani
//! jednej gałęzi platformowej. Zamiany na wartość wyliczoną z wiersza nie ma: identyfikator
//! wyliczony z `step_id` i próby byłby ten sam po każdej awarii tego samego kroku, czyli
//! dokładnie tym sklejeniem dwóch tur w jedną sesję, przed którym broni AC-4.
//!
//! # Czego tu świadomie nie ma
//!
//! Automatycznego wznowienia przerwanego agenta. Loadout wykrywa, sprząta, oznacza i **pyta**
//! [T7 §6.3]. Błędne auto-wznowienie jest znacznie gorsze niż jedno uczciwe pytanie, a
//! `--resume` na sesji zabitej w połowie tury nie było testowane [T7 §11.1]. Brak automatyki
//! jest własnością typu: w [`RecoveryPlan`] nie ma pola, które by ją włączało.
//!
//! # Skąd biorą się wartości, które ten plik ustawia (niezmiennik 4)
//!
//! Z plików, nie z bazy. `failed`, powód `interrupted` i `attempt + 1` muszą dać się odtworzyć
//! z `.loadout/runs/<ts>__<id>/run.json` i surowych `logs/agent-<id>.jsonl`. Ten plik zwraca
//! **plan**, a nie zapis: kto go wczyta i kto go zapisze, rozstrzygają T-06 i T-15.

use serde::Deserialize as _;
use serde::Serialize;
use serde::de::IntoDeserializer as _;
use serde::de::value::StrDeserializer;
use uuid::Uuid;

use crate::engine::step::StepState;

/// Status, który po odzyskaniu dostaje **bieg** — nigdy krok.
///
/// `docs/ARCHITECTURE.md` §5 i `CHECK` w `store::schema`: sześć stanów biegu, wśród nich
/// `interrupted`, i siedem stanów kroku, wśród których `interrupted` **nie występuje**.
/// Wpisanie go w kolumnę statusu kroku jest tą pomyłką, przed którą broni AC-3.
pub const RUN_INTERRUPTED: &str = "interrupted";

/// Status, który po odzyskaniu dostaje **krok** przerwany awarią aplikacji.
pub const STEP_FAILED: &str = "failed";

/// Powód wpisywany krokowi obok statusu [`STEP_FAILED`], w osobne pole.
pub const STEP_REASON_INTERRUPTED: &str = "interrupted";

/// Pierwsza z dwóch opcji pytania. Tekst jest **daną** ustaloną w tym pliku; jego wyświetlenie
/// i obsługa kliknięcia to widok pracy (T-08 / T-09). Po angielsku, bo widzi go człowiek
/// (`docs/DECISIONS-LOCKED.md` D5).
pub const PICK_UP_LABEL: &str = "Pick up where it left off";

/// Druga z dwóch opcji pytania.
pub const START_OVER_LABEL: &str = "Start this step again";

/// Wiersz, który odzyskiwanie dostaje na wejściu — jeden krok razem z tym, co wiadomo o jego
/// biegu.
///
/// Wszystkie pola są takie, jakie **stoją w bazie**, łącznie z wartościami, których ta wersja
/// Loadouta nie zna: te wiersze zapisała jego **starsza** wersja (niezmiennik 5). Dlatego
/// `run_status` i `step_status` są napisami, a nie enumami — enum z drutu wywala się na
/// wartości dołożonej w przyszłym tygodniu, a odzyskiwanie ma prawo paść ostatnie.
///
/// `pid` jest opcjonalny wbrew literalnemu kształtowi z `TASK.md`, bo kolumna `steps.pid` jest
/// `NULL`-owalna: krok, który nigdy nie doszedł do spawnu, nie ma czym jej wypełnić, a wpisanie
/// tam zera oznaczałoby coś zupełnie innego niż „nie wiadomo". Żadne kryterium tego pola nie
/// dotyka — czyta je wyłącznie diagnostyka.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryRow {
    /// Identyfikator kroku. To po nim adresowane jest **wszystko**, co ten plan mówi.
    pub step_id: String,
    /// Bieg, do którego krok należy.
    pub run_id: String,
    /// Status biegu, tak jak stoi w bazie.
    pub run_status: String,
    /// Status kroku, tak jak stoi w bazie.
    pub step_status: String,
    /// Czas startu systemu **zapisany przy biegu**. `None` znaczy „wiersz sprzed wprowadzenia
    /// pola" i jest brakiem strażnika, a nie zgodą na strzał — patrz [`Machine::boot_id`].
    pub run_boot_id: Option<String>,
    /// PID lidera grupy, jeśli spawn do niego doszedł. Nieużywany przez żadną decyzję.
    pub pid: Option<i32>,
    /// PGID grupy procesów agenta. Jedyna liczba, po której da się sprzątnąć sierotę, i jedyna,
    /// którą wolno podać domykaczowi z [`apply`].
    pub pgid: Option<i32>,
    /// Sesja agenta, przydzielona **przed** spawnem [T7 §6.2, V]. Bez niej nie ma czego wznowić
    /// i nie ma o co zapytać: istnieje proces, którego sesji nie umiemy nazwać.
    pub session_id: Option<String>,
    /// Numer próby. Ponowienie kroku znaczy `attempt + 1`.
    pub attempt: i64,
}

/// Maszyna, na której Loadout właśnie wstał. Obie liczby przyjeżdżają z zewnątrz, bo obie
/// wymagają wywołania systemowego, a to mieszka w `engine/supervisor.rs` (niezmiennik 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Machine {
    /// Czas startu **tego** systemu, odczytany przez wołającego z `sysctl kern.boottime`.
    ///
    // 2026-08-16 — TO POLE JEST STRAŻNIKIEM, nie kopią czegoś, co i tak wiadomo.
    //
    // `kern.maxproc` na macOS wynosi 16 000 [T7 §6.3, V], więc PID-y przewijają się w godzinach,
    // nie w latach. Po restarcie maszyny zapisany `pgid` z dużym prawdopodobieństwem należy do
    // czegoś zupełnie niewinnego, a `killpg` po nim jest błędem poprawności, nie ryzykiem
    // teoretycznym [T7 ryzyko 2]. Porównanie tego napisu z `RecoveryRow::run_boot_id` JEST tym
    // strażnikiem — przy pierwszym refaktorze wygląda jak porównanie dwóch stringów o niczym
    // i nim nie jest (niezmiennik 24).
    //
    // Cicha awaria tej ochrony wygląda tak: kod odczytuje czas startu z `sysctl` po OBU stronach
    // porównania i porównuje wartość samą ze sobą. Strażnik jest wtedy w kodzie, jest zielony
    // w testach i nie strzeli nigdy. Dlatego jedna strona przyjeżdża z bazy (`run_boot_id`),
    // druga od wołającego (to pole), a ten plik nie ma skąd wziąć trzeciej.
    pub boot_id: String,
    /// Własna grupa procesów Loadouta, odczytana przez wołającego z `getpgrp()`.
    ///
    // 2026-08-16 — DRUGI STRAŻNIK, i ten pilnuje nas przed nami samymi. `0` w `killpg` znaczy
    // „moja własna grupa", więc wiersz z `pgid = 0` albo z `pgid` równym tej wartości to Loadout
    // zabijający sam siebie w pętli startowej — awaria, która wygląda jak crash odzyskiwania.
    pub own_pgid: i32,
}

/// Co [`decide`] postanowiło zrobić. Sama treść decyzji: nic tu nie zostało wykonane.
///
/// Pięć list, każda adresowana identyfikatorem, i **żadnego pola, które by cokolwiek wznowiło
/// samo** — brak automatyki jest własnością tego typu, nie ustawieniem [T7 §6.3, §9.4].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RecoveryPlan {
    /// `pgid`-y do sprzątnięcia, w kolejności wierszy, bez duplikatów. Pusta lista jest
    /// poprawnym planem: po restarcie maszyny sieroty już nie żyją.
    pub reap: Vec<i32>,
    /// Biegi, które mają dostać [`RUN_INTERRUPTED`].
    pub run_status: Vec<RunStatusChange>,
    /// Kroki, które mają dostać [`STEP_FAILED`] z powodem [`STEP_REASON_INTERRUPTED`].
    pub step_status: Vec<StepStatusChange>,
    /// Po jednym pytaniu na przerwany krok. Nigdy więcej niż jedno na krok.
    pub ask: Vec<Question>,
    /// Wiersze, o których nie dało się rozstrzygnąć — **wypisane, nie pominięte**.
    pub unreadable: Vec<Unreadable>,
}

/// Bieg i status, który ma dostać.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunStatusChange {
    /// Bieg, którego to dotyczy.
    pub run_id: String,
    /// Docelowy status. Stoi tu jako **wartość**, a nie jako nazwa typu, bo cała pomyłka,
    /// przed którą to broni, polega na wpisaniu właściwego słowa w niewłaściwą kolumnę.
    pub status: String,
}

/// Krok, jego docelowy status i **osobno** powód.
///
/// `docs/ARCHITECTURE.md` §5 rozdziela te dwa pola: krok idzie do `failed`, a `interrupted`
/// jest powodem. Sklejenie ich w jedno pole daje status kroku, którego `CHECK` w `store::schema`
/// nie przyjmie — w środku startu po awarii.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StepStatusChange {
    /// Krok, którego to dotyczy.
    pub step_id: String,
    /// Docelowy status kroku.
    pub status: String,
    /// Powód, w osobne pole.
    pub reason: String,
}

/// Jedno pytanie o jeden przerwany krok. Dwie opcje, **żadnej wybranej z góry**.
///
/// Opcje są tablicą o stałym rozmiarze, a nie wektorem: „dokładnie dwie" jest wtedy własnością
/// typu, a nie rzeczą do sprawdzenia przy każdym użyciu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Question {
    /// Krok, o który pytamy.
    pub step_id: String,
    /// Dwie opcje w ustalonej kolejności: najpierw [`PICK_UP_LABEL`], potem [`START_OVER_LABEL`].
    /// Kolejność nie jest preferencją — jest kolejnością, w jakiej widzi je człowiek.
    pub options: [QuestionOption; 2],
}

/// Jedna z dwóch opcji pytania: tekst po angielsku i to, co się stanie po kliknięciu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuestionOption {
    /// Zdanie, które zobaczy człowiek. Dana, nie widok.
    pub label: String,
    /// Co ta opcja robi.
    pub effect: OptionEffect,
}

/// Skutek wybrania opcji. Dwa warianty, bo opcje są dwie.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OptionEffect {
    /// Kontynuacja tej samej rozmowy: `--resume <session_id>` z sesją **zapisaną w wierszu**
    /// [T7 §6.2, V].
    PickUp {
        /// Sesja z wiersza, nietknięta.
        session_id: String,
    },
    /// Krok od nowa: świeża sesja i `attempt + 1`.
    ///
    /// Sesja **musi** być inna niż zapisana. Przepisanie tego samego identyfikatora skleiłoby
    /// dwie tury w jedną sesję i zgubiło granicę próby.
    StartOver {
        /// Nowa sesja, różna od zapisanej i różna od sesji każdego innego pytania.
        session_id: String,
        /// `attempt` z wiersza powiększony o jeden.
        attempt: i64,
    },
}

/// Wiersz, o którym nie dało się rozstrzygnąć, razem z jednozdaniowym powodem po angielsku.
///
/// Nie jest to błąd i nie jest to cisza: wiersz zapisany starszą wersją Loadouta ma prawo być
/// niezrozumiały, ale nie ma prawa zniknąć. Panika w tym miejscu to aplikacja, która nie startuje
/// **dokładnie po tym, jak się wywaliła** (niezmiennik 5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Unreadable {
    /// Krok, którego wiersz to jest. Bez tego pola wiersz znika po cichu.
    pub step_id: String,
    /// Jedno zdanie po angielsku: co w tym wierszu było nie tak.
    pub reason: String,
}

/// Co domykacz z [`apply`] ma prawo powiedzieć o grupie procesów.
///
/// Trzy warianty, bo `kill` odpowiada na trzy sposoby i **dwa z nich nie są śmiercią**:
/// niezmiennik 6 czyta się dosłownie — dopóki nie ma `ESRCH`, grupa jest żywa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReapOutcome {
    /// `ESRCH`: w grupie nie ma już nikogo. Jedyny stan, w którym wolno powiedzieć „nie żyje".
    ProvenDead,
    /// Grupa nadal odpowiada na sygnał zerowy. Wynik do obsłużenia, nie błąd do zalogowania:
    /// osierocony `claude` pali limit w tle.
    StillAlive,
    /// `EPERM`: grupa **istnieje i należy do kogoś innego**, czyli `pgid` został przewinięty.
    ///
    /// To nie jest dowód śmierci i nie wolno tego eskalować — po drugiej stronie stoi dokładnie
    /// ten niewinny proces, przed którym broni strażnik z [`Machine::boot_id`]. Cichy błąd,
    /// którego ten wariant nie dopuszcza: potraktowanie każdego niezerowego wyniku `kill` jako
    /// „już nie żyje" i zameldowanie posprzątanego biegu.
    Foreign,
}

/// Co się naprawdę stało, kiedy plan przeszedł przez domykacza.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecoveryReport {
    /// Grupy z dowodem śmierci.
    pub reaped: Vec<i32>,
    /// Grupy, które nadal żyją. Bez dowodu grupa jest żywa — także wtedy, gdy sygnał poszedł.
    pub unproven: Vec<i32>,
    /// Grupy należące do kogoś innego. `pgid` przewinięty; nie tykamy.
    pub foreign: Vec<i32>,
}

impl RecoveryReport {
    /// Czy po sprzątaniu nie została ani jedna wątpliwość.
    ///
    /// Prawda **wyłącznie** wtedy, gdy `unproven` i `foreign` są puste. Raport z niepustym
    /// `foreign` nie jest czysty, choć nikogo nie zabiliśmy: cudza grupa pod naszym `pgid`
    /// znaczy, że nasza sierota mogła zginąć przy restarcie — albo że biegnie do dziś pod
    /// numerem, którego już nie znamy.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.unproven.is_empty() && self.foreign.is_empty()
    }
}

/// Powody, dla których wiersz trafia do [`RecoveryPlan::unreadable`].
///
/// Po angielsku, bo czyta je człowiek po awarii (`docs/DECISIONS-LOCKED.md` D5), i po **jednym
/// zdaniu** każdy: to jest pozycja listy, nie raport. Stoją jako stałe, a nie w miejscu użycia,
/// bo lista powodów jest tu jedyną odpowiedzią na pytanie „czego to odzyskiwanie nie umie".
///
/// 2026-08-17 — MODUŁ JEST `pub`, i to nie jest ustępstwo na rzecz testu. Te zdania są WYJŚCIEM
/// tej funkcji: lądują w `RecoveryPlan::unreadable` i stamtąd na ekranie człowieka po awarii.
/// Kryterium AC-3 z `tasks/T-35.md` nazywa jedno z nich wprost („`decide()` na tych danych
/// **nie** zwraca `NO_BOOT_TIME`"), więc jest częścią kontraktu, a nie szczegółem środka.
///
/// Alternatywa, której świadomie NIE wybrano: zostawić moduł prywatny i wkleić to samo zdanie
/// do testu. Wklejone zdanie przestaje cokolwiek znaczyć w dniu, w którym ktoś poprawi tutaj
/// brzmienie — test dalej jest zielony i dalej porównuje napis, którego produkt już nie mówi
/// (niezmiennik 13).
pub mod reason {
    /// Status biegu spoza szóstki z `CHECK` przy tabeli `runs`.
    pub const UNKNOWN_RUN: &str = "This run is in a state this version of Loadout does not know, so nothing about it \
         could be decided.";
    /// Status kroku spoza siódemki z `CHECK` przy tabeli `steps`.
    pub const UNKNOWN_STEP: &str = "This step is in a state this version of Loadout does not know, so there is no telling \
         whether it had already finished.";
    /// Wiersz sprzed wprowadzenia kolumny z czasem startu systemu.
    pub const NO_BOOT_TIME: &str = "This run does not say when the machine it started on was last booted, so there is no \
         way to tell whether its group number still belongs to it.";
    /// `pgid = 0`, czyli w `killpg` własna grupa wołającego.
    pub const PGID_IS_ZERO: &str = "The group number written down for this step is 0, which always means 'whoever \
         is asking', so using it would stop Loadout itself during startup.";
    /// Wiersz bez `pgid`: spawn nie doszedł do zapisu.
    pub const PGID_MISSING: &str = "No group number was ever written down for this step, so there is nothing that \
         could be cleaned up after it.";
    /// `pgid` ujemny. Znak jest selektorem w `kill`, nie częścią numeru.
    pub const PGID_NEGATIVE: &str = "The group number written down for this step is negative, and a negative number \
         is not a group.";
    /// `pgid` równy własnej grupie Loadouta.
    pub const PGID_IS_OURS: &str = "The group number written down for this step is the one Loadout itself runs in, \
         so using it would stop Loadout during startup.";
    /// Krok przerwany w locie, któremu nikt nie zapisał sesji.
    pub const NO_SESSION: &str = "This step was cut off in flight, but nothing was written down that its agent could \
         be picked up from.";
    /// Licznik prób, którego nie da się powiększyć.
    pub const TRY_COUNT_MAXED: &str =
        "The number of tries written down for this step cannot be counted any higher.";
    /// Licznik prób poniżej zera.
    pub const TRY_COUNT_BELOW_ZERO: &str = "The number of tries written down for this step is below zero, so the next try \
         could not be numbered.";
}

/// Sześć stanów **biegu**, tak jak stoją w `CHECK` przy tabeli `runs` w `store::schema`.
///
/// Enum stoi tutaj, choć `store::NewRun::status` jest `String`iem i to też jest decyzja: tam
/// o dozwolonych wartościach rozstrzyga `CHECK`, a nie typ w Ruście. Odzyskiwanie potrzebuje
/// jednak czegoś więcej niż „dozwolone / niedozwolone" — musi odróżnić wartość **znaną
/// i skończoną** od wartości, której ta wersja nie zna. Pierwsza znaczy „nie ma nic do roboty",
/// druga „nie wiem, więc wypisz wiersz i nie strzelaj" (niezmiennik 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunState {
    /// Bieg prowadzony przez planistę.
    Running,
    /// Bieg wstrzymany. Stan **biegu**, nigdy kroku [T7 §9.3].
    Paused,
    /// Koniec, powodzenie.
    Succeeded,
    /// Koniec, niepowodzenie.
    Failed,
    /// Koniec, bo użytkownik zatrzymał bieg.
    Cancelled,
    /// Koniec postawiony przez to odzyskiwanie przy poprzednim starcie.
    Interrupted,
}

impl RunState {
    /// Czyta wartość z bazy. `None` znaczy „napisała to wersja, której nie znamy".
    fn from_wire(text: &str) -> Option<Self> {
        match text {
            "running" => Some(Self::Running),
            "paused" => Some(Self::Paused),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            RUN_INTERRUPTED => Some(Self::Interrupted),
            _ => None,
        }
    }

    /// Czy awaria aplikacji zastała ten bieg w locie.
    ///
    /// 2026-08-16 — `Interrupted` jest tu po stronie „nie", i to jest cała druga połowa AC-3: odzyskiwanie
    /// biegnie przy KAŻDYM starcie, więc zobaczy także wiersze, które samo poprawiło godzinę
    /// wcześniej. Bieg, który już nosi ten status, jest zamknięty — dopisanie go drugi raz
    /// zamieniłoby jedną awarię w kolejkę identycznych zapisów.
    fn was_cut_off(self) -> bool {
        match self {
            Self::Running | Self::Paused => true,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Interrupted => false,
        }
    }
}

/// Siedem stanów **kroku**, czytanych przez [`StepState`] — nie przepisanych tutaj po raz trzeci.
///
/// Ta sama siódemka stoi w `CHECK` przy `steps.status` i w `engine::step::StepState`. Trzecia
/// kopia rozjechałaby się przy pierwszym dołożonym stanie, i rozjazd byłby cichy: nowy stan
/// wyglądałby tu jak „skończony", czyli jak zgoda na porzucenie sieroty.
///
/// `StepState` jest enumem **zamkniętym** i taki ma zostać — nieznaną wartość odrzuca. Tutaj ta
/// odmowa nie jest awarią: zamienia się w pozycję w [`RecoveryPlan::unreadable`], bo wiersz
/// zapisała starsza wersja Loadouta (niezmiennik 5).
fn step_state(text: &str) -> Option<StepState> {
    let wire: StrDeserializer<serde::de::value::Error> = text.into_deserializer();
    StepState::deserialize(wire).ok()
}

/// Świeży identyfikator sesji dla opcji „zacznij od nowa".
///
/// `now_v7`, jak wszędzie indziej w repo: sortuje się po czasie, więc dwie próby tego samego
/// kroku dają się ustawić w kolejności bez czytania czegokolwiek innego. Wartość **musi** być
/// nowa przy każdym wywołaniu — patrz notka o wyjątku w nagłówku pliku.
fn fresh_session() -> String {
    Uuid::now_v7().to_string()
}

/// `pgid`, którego zabicie jest bezpieczne. `Err` niesie zdanie do
/// [`RecoveryPlan::unreadable`].
///
/// Cztery odmowy, każda z własnym powodem, bo wiersz odrzucony po cichu i wiersz, którego filtr
/// w ogóle nie zobaczył, dają identyczne [`RecoveryPlan::reap`] i różnią się dopiero na tej
/// liście.
///
/// 2026-08-16 — sprawdzenie biegnie **niezależnie** od strażnika czasu startu, choć po restarcie
/// maszyny i tak nikogo nie zabijemy. Powód stoi wprost w AC-1: pytania nie mają prawa zależeć od tego,
/// czy maszyna się zrestartowała. Gdyby ten filtr stał za strażnikiem, wiersz z `pgid = 0`
/// dostawałby pytanie po restarcie i nie dostawał bez restartu — jedna awaria, dwie różne listy.
fn usable_pgid(pgid: Option<i32>, own_pgid: i32) -> Result<i32, &'static str> {
    // `None` nie znaczy „zero" i nie znaczy „nieważne": spawn nie doszedł do zapisu.
    let Some(pgid) = pgid else {
        return Err(reason::PGID_MISSING);
    };
    // `0` w `killpg` znaczy „moja własna grupa". Wiersz z tą wartością to Loadout zabijający
    // sam siebie w pętli startowej — awaria, która wygląda jak crash odzyskiwania.
    if pgid == 0 {
        return Err(reason::PGID_IS_ZERO);
    }
    // Znak jest selektorem w `kill` („grupa, nie proces"), nie częścią numeru: `-9` w kolumnie
    // to nie jest grupa 9, tylko wiersz, którego nie umiemy przeczytać.
    if pgid < 0 {
        return Err(reason::PGID_NEGATIVE);
    }
    // To samo co `0`, tylko napisane wprost.
    if pgid == own_pgid {
        return Err(reason::PGID_IS_OURS);
    }
    Ok(pgid)
}

/// Co jeden wiersz znaczy dla planu.
#[derive(Debug)]
enum RowVerdict {
    /// Krok, którego odzyskiwanie nie dotyczy: już skończony albo jeszcze nieruszony.
    Settled,
    /// Krok przerwany awarią aplikacji.
    CutOff {
        /// Grupa do sprzątnięcia. `None`, kiedy strażnik czasu startu nie przepuścił.
        reap: Option<i32>,
        /// Sesja z wiersza, przydzielona przed spawnem [T7 §6.2, V].
        session_id: String,
        /// Numer próby po ponowieniu kroku.
        next_attempt: i64,
    },
}

/// Czyta jeden wiersz i mówi, co z nim zrobić. `Err` niesie zdanie do
/// [`RecoveryPlan::unreadable`].
///
/// Kolejność sprawdzeń jest treścią, nie stylem: wiersz dostaje powód **pierwszej** rzeczy,
/// której o nim nie wiemy, a strażnik czasu startu stoi przed wszystkim, co dotyczy `pgid`.
fn read_row(row: &RecoveryRow, machine: &Machine) -> Result<RowVerdict, &'static str> {
    let Some(state) = step_state(&row.step_status) else {
        return Err(reason::UNKNOWN_STEP);
    };
    match state {
        // Jedyne dwa stany, w których awaria aplikacji mogła przerwać krok w locie.
        StepState::Ready | StepState::Running => {}
        // Wyliczone po jednym zamiast `_`: ósmy stan kroku ma tutaj **nie skompilować**.
        // Cichym skutkiem `_` byłoby uznanie nowego stanu za skończony, czyli porzucenie
        // sierocego procesu bez ani jednego słowa w planie.
        StepState::Pending
        | StepState::Succeeded
        | StepState::Failed
        | StepState::Cancelled
        | StepState::Skipped => return Ok(RowVerdict::Settled),
    }

    // Strażnik. Rozróżnienie, które tu stoi, jest całym AC-1: BRAK czasu startu to niewiedza
    // (wiersz idzie do `unreadable` i nic się z nim nie dzieje), a czas INNY niż ten jest
    // odpowiedzią — „restart maszyny już zabił sieroty" — więc wiersz zostaje obsłużony
    // w całości, tylko bez sprzątania. Nie ma czego zabijać, jest o co zapytać [T7 §6.3].
    let Some(recorded_boot) = row.run_boot_id.as_deref() else {
        return Err(reason::NO_BOOT_TIME);
    };
    let same_machine_since = recorded_boot == machine.boot_id;
    let pgid = usable_pgid(row.pgid, machine.own_pgid)?;

    // Sesja jest przydzielana PRZED spawnem [T7 §6.2, V], więc krok, który biegł, ma ją mieć.
    // Kiedy jej nie ma, istnieje proces, którego sesji nie umiemy nazwać: opcja „podejmij tam,
    // gdzie stanęło" nie miałaby czego nieść, a pytanie z jedną opcją nie jest pytaniem.
    let Some(session_id) = row
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|session| !session.is_empty())
    else {
        return Err(reason::NO_SESSION);
    };

    // `attempt + 1` na `i64::MAX` się przekręca, a próba mniejsza od poprzedniej to ponowienie,
    // które ląduje na wierszu innej próby. Ujemna próba przewraca to samo w drugą stronę
    // i wchodzi tu razem z tamtą, bo obie znaczą to samo: tego licznika nie da się kontynuować.
    if row.attempt < 0 {
        return Err(reason::TRY_COUNT_BELOW_ZERO);
    }
    let Some(next_attempt) = row.attempt.checked_add(1) else {
        return Err(reason::TRY_COUNT_MAXED);
    };

    Ok(RowVerdict::CutOff {
        reap: same_machine_since.then_some(pgid),
        session_id: session_id.to_owned(),
        next_attempt,
    })
}

/// Jedno pytanie o jeden przerwany krok: dwie opcje, w ustalonej kolejności, żadna nie jest
/// wybrana z góry.
fn question(step_id: &str, session_id: &str, next_attempt: i64) -> Question {
    Question {
        step_id: step_id.to_owned(),
        options: [
            QuestionOption {
                label: PICK_UP_LABEL.to_owned(),
                effect: OptionEffect::PickUp {
                    session_id: session_id.to_owned(),
                },
            },
            QuestionOption {
                label: START_OVER_LABEL.to_owned(),
                effect: OptionEffect::StartOver {
                    session_id: fresh_session(),
                    attempt: next_attempt,
                },
            },
        ],
    }
}

/// Rozstrzyga, co zrobić z wierszami zastanymi przy starcie. **Niczego nie wykonuje.**
///
/// Cały stan systemu wjeżdża w [`Machine`], więc nie ma tu skąd wziąć czasu startu po raz drugi
/// i porównać go ze sobą (patrz [`Machine::boot_id`]).
///
/// Nie panikuje na żadnym wejściu. Nieznany status, `pgid = NULL`, brak sesji i próba, której
/// nie da się powiększyć, kończą się wpisem w [`RecoveryPlan::unreadable`] — niezmiennik 5.
#[must_use]
pub fn decide(rows: &[RecoveryRow], machine: &Machine) -> RecoveryPlan {
    let mut plan = RecoveryPlan::default();

    for row in rows {
        // Status biegu czytamy przed statusem kroku i NIEZALEŻNIE od niego. Bieg zastany
        // w `running` po awarii jest przerwany także wtedy, kiedy o jego kroku nie umiemy
        // powiedzieć nic — inaczej bieg, którego nikt już nie prowadzi, zostaje na ekranie
        // jako żywy, bo jedyny jego wiersz zapisała starsza wersja Loadouta.
        let Some(run_state) = RunState::from_wire(&row.run_status) else {
            plan.unreadable.push(Unreadable {
                step_id: row.step_id.clone(),
                reason: reason::UNKNOWN_RUN.to_owned(),
            });
            continue;
        };
        let known_run = plan
            .run_status
            .iter()
            .any(|change| change.run_id == row.run_id);
        if run_state.was_cut_off() && !known_run {
            plan.run_status.push(RunStatusChange {
                run_id: row.run_id.clone(),
                status: RUN_INTERRUPTED.to_owned(),
            });
        }

        match read_row(row, machine) {
            Ok(RowVerdict::Settled) => {}
            Ok(RowVerdict::CutOff {
                reap,
                session_id,
                next_attempt,
            }) => {
                // Duplikat znika bez słowa i to jest decyzja, nie usterka: dwa `SIGTERM` do tej
                // samej grupy to drugi sygnał wysłany do grupy, która już nie istnieje. Wektor
                // zamiast zbioru, bo kolejność wierszy jest częścią kontraktu, a wierszy jest
                // tyle, ile kroków w biegu (~20).
                if let Some(pgid) = reap.filter(|pgid| !plan.reap.contains(pgid)) {
                    plan.reap.push(pgid);
                }
                plan.step_status.push(StepStatusChange {
                    step_id: row.step_id.clone(),
                    status: STEP_FAILED.to_owned(),
                    reason: STEP_REASON_INTERRUPTED.to_owned(),
                });
                plan.ask
                    .push(question(&row.step_id, &session_id, next_attempt));
            }
            Err(reason) => plan.unreadable.push(Unreadable {
                step_id: row.step_id.clone(),
                reason: reason.to_owned(),
            }),
        }
    }

    plan
}

/// Przepuszcza `plan.reap` przez domykacza i zbiera dowody.
///
/// Domykacz jest jedyną drogą, którą z tego pliku wychodzi cokolwiek do systemu operacyjnego:
/// `killpg` razem z eskalacją `SIGTERM` → łaska → `SIGKILL` mieszka w `engine/supervisor.rs`
/// (niezmiennik 3, niezmiennik 6). Każda grupa dostaje **dokładnie jedno** wywołanie — eskalacja
/// jest w środku domykacza, nie tutaj, i [`ReapOutcome::Foreign`] nie ma jej prawa dostać.
#[must_use]
pub fn apply(plan: &RecoveryPlan, reap: &mut dyn FnMut(i32) -> ReapOutcome) -> RecoveryReport {
    let mut report = RecoveryReport::default();

    for &pgid in &plan.reap {
        // Trzy odpowiedzi, trzy listy, i tylko jedna z nich jest dowodem. Cichy błąd, którego
        // ten `match` nie dopuszcza: `_ => report.reaped.push(pgid)`, czyli potraktowanie
        // każdego niezerowego wyniku `kill` jako „już nie żyje" i zameldowanie posprzątanego
        // biegu, którego nikt nie sprzątnął (niezmiennik 6).
        match reap(pgid) {
            ReapOutcome::ProvenDead => report.reaped.push(pgid),
            ReapOutcome::StillAlive => report.unproven.push(pgid),
            // Bez `continue`, bez drugiego wywołania: eskalacja do `SIGKILL` na cudzej grupie
            // trafiłaby dokładnie w ten niewinny proces, przed którym broni strażnik czasu
            // startu. Jedno wywołanie na grupę jest tu własnością pętli, nie zaleceniem.
            ReapOutcome::Foreign => report.foreign.push(pgid),
        }
    }

    report
}
