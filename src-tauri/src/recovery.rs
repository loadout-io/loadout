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
//! Konsekwencja, którą trzeba nazwać: [`decide`] jest czystą funkcją dwóch argumentów. Nie ma
//! skąd wziąć ani czasu, ani stanu systemu, więc nie ma jak porównać wartości samej ze sobą —
//! patrz datowana notka przy [`Machine::boot_id`].
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

use serde::Serialize;

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

/// Rozstrzyga, co zrobić z wierszami zastanymi przy starcie. **Niczego nie wykonuje.**
///
/// Czysta funkcja dwóch argumentów: cały stan systemu wjeżdża w [`Machine`], więc nie ma tu
/// skąd wziąć czasu startu po raz drugi i porównać go ze sobą (patrz [`Machine::boot_id`]).
///
/// Nie panikuje na żadnym wejściu. Nieznany status, `pgid = NULL`, brak sesji i próba, której
/// nie da się powiększyć, kończą się wpisem w [`RecoveryPlan::unreadable`] — niezmiennik 5.
// SZKIELET FAZY KONTRAKTU: pusty plan, żeby kryteria skompilowały się i padły na asercjach.
// `todo!()` jest zabroniony przez `[workspace.lints.clippy] todo = "deny"`, a test, który się
// nie kompiluje, niczego nie uruchomił i bramka odrzuca go jako fałszywą czerwień.
#[must_use]
pub fn decide(_rows: &[RecoveryRow], _machine: &Machine) -> RecoveryPlan {
    RecoveryPlan::default()
}

/// Przepuszcza `plan.reap` przez domykacza i zbiera dowody.
///
/// Domykacz jest jedyną drogą, którą z tego pliku wychodzi cokolwiek do systemu operacyjnego:
/// `killpg` razem z eskalacją `SIGTERM` → łaska → `SIGKILL` mieszka w `engine/supervisor.rs`
/// (niezmiennik 3, niezmiennik 6). Każda grupa dostaje **dokładnie jedno** wywołanie — eskalacja
/// jest w środku domykacza, nie tutaj, i [`ReapOutcome::Foreign`] nie ma jej prawa dostać.
// SZKIELET FAZY KONTRAKTU: pusty raport. Patrz notka przy `decide`.
#[must_use]
pub fn apply(_plan: &RecoveryPlan, _reap: &mut dyn FnMut(i32) -> ReapOutcome) -> RecoveryReport {
    RecoveryReport::default()
}
