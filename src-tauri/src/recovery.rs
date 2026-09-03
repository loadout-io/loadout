//! Uzgodnienie stanu przy starcie: co zabić i co przepisać.
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
//! # Czego tu świadomie nie ma
//!
//! Wznawiania ani pytania o wznowienie przerwanego agenta. Startup ma wyłącznie konsumentów
//! sprzątania i zmian statusu, więc recovery nie produkuje decyzji, której nikt nie może wykonać.
//! Istniejącą sesję nadal może jawnie przekazać wołający adaptera przez `RunSpec.resume`; recovery
//! tego transportu nie konstruuje.
//!
//! # Skąd biorą się wartości, które ten plik ustawia (niezmiennik 4)
//!
//! Z plików, nie z bazy. `failed` i powód `interrupted` muszą dać się odtworzyć
//! z `.loadout/runs/<ts>__<id>/run.json` i surowych `logs/agent-<id>.jsonl`. Ten plik zwraca
//! **plan**, a nie zapis: kto go wczyta i kto go zapisze, rozstrzygają T-06 i T-15.

use serde::Deserialize as _;
use serde::Serialize;
use serde::de::IntoDeserializer as _;
use serde::de::value::StrDeserializer;

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
    /// PGID grupy procesów agenta. Liczba, po której da się sprzątnąć sierotę stojącą w grupie
    /// lidera — i jedna z tych, które wolno podać domykaczowi z [`apply`].
    pub pgid: Option<i32>,
    /// **Każda** grupa procesów, którą ten krok uruchomił, razem z grupą lidera.
    ///
    /// 2026-09 (Z-01d) — do tego dnia było tu wyłącznie [`RecoveryRow::pgid`], czyli zdanie
    /// o JEDNEJ grupie. Claude Code uruchamia każdą komendę narzędzia Bash we własnej grupie, więc
    /// wszystko, co taka powłoka odpali, leżało poza sprzątaniem po awarii — a osierocony proces
    /// pali limit u dostawcy w tle (niezmiennik 6). Pole jest **obok** `pgid`, nigdy zamiast:
    /// starszy plik biegu ma tu pustą listę i nadal daje się posprzątać po liderze
    /// (niezmiennik 25).
    pub pgids: Vec<i32>,
    /// Czy krok dostał z jądra dowód, że po jego grupach nie zostało nic.
    ///
    /// Czyta to jedyna decyzja, która patrzy na kroki **skończone**: krok zamknięty bez dowodu
    /// zostawił coś, co dalej może biec ([`RowVerdict::LeftBehind`]).
    pub death_proof: bool,
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
/// Cztery listy, każda adresowana identyfikatorem, i żadnej sesji ani decyzji o wznowieniu.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RecoveryPlan {
    /// Grupy do sprzątnięcia, w kolejności wierszy, bez duplikatów. Pusta lista jest
    /// poprawnym planem: po restarcie maszyny sieroty już nie żyją.
    pub reap: Vec<ReapTarget>,
    /// Biegi, które mają dostać [`RUN_INTERRUPTED`].
    pub run_status: Vec<RunStatusChange>,
    /// Kroki, które mają dostać [`STEP_FAILED`] z powodem [`STEP_REASON_INTERRUPTED`].
    pub step_status: Vec<StepStatusChange>,
    /// Wiersze, o których nie dało się rozstrzygnąć — **wypisane, nie pominięte**.
    pub unreadable: Vec<Unreadable>,
}

/// Jedna grupa procesów do sprzątnięcia razem z biegiem, do którego **ma** należeć.
///
/// # Dlaczego to nie jest gołe `i32` (2026-09, Z-01d)
///
/// Bo sam numer grupy nie wystarcza do podjęcia decyzji, a poprzednie podejście przyjmowało, że
/// wystarcza. `kern.maxproc` na macOS wynosi 16 000, więc numery przewijają się w godzinach:
/// grupa pod zapisanym `pgid` bywa dziś czymś zupełnie innym. Domykacz musi więc móc **zapytać
/// tę grupę**, do kogo należy — a do tego potrzebuje identyfikatora biegu, z którym porówna
/// znacznik z jej środowiska (`engine::supervisor::TAG_RUN`).
///
/// Sam plik dalej nie wykonuje ani jednego wywołania systemowego: porównanie dzieje się
/// w domykaczu, który wjeżdża do [`apply`] argumentem (nagłówek modułu).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReapTarget {
    /// Bieg, którego ta grupa ma być. Ta sama wartość, którą niesie `run.json` → `id`.
    pub run_id: String,
    /// Grupa procesów. Przeszła już przez [`usable_pgid`], więc jest dodatnia i nie jest nasza.
    pub pgid: i32,
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

/// `pgid`, którego zabicie jest bezpieczne. `Err` niesie zdanie do
/// [`RecoveryPlan::unreadable`].
///
/// Cztery odmowy, każda z własnym powodem, bo wiersz odrzucony po cichu i wiersz, którego filtr
/// w ogóle nie zobaczył, dają identyczne [`RecoveryPlan::reap`] i różnią się dopiero na tej
/// liście.
///
/// 2026-08-27 — sprawdzenie dotyczy wyłącznie bieżącego bootu. Po restarcie nie wysyłamy
/// sygnału, więc wartość `pgid` nie uczestniczy już w decyzji i nie może zablokować uczciwego
/// oznaczenia przerwanego kroku. Na bieżącym boocie wszystkie cztery odmowy nadal obowiązują.
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

/// Wszystkie grupy tego wiersza, które wolno zabić — grupa lidera i każda, którą krok założył
/// obok niej.
///
/// 2026-09 (Z-01d) — `pgid` idzie PIERWSZY i to nie jest kolejność dla porządku: to o nim wołający
/// wie najwięcej, to on stoi w starszych plikach biegów i to jego numer widzi człowiek w zdaniu
/// o ocalałym. Duplikaty znikają, bo `pgids` niesie grupę lidera także wtedy, gdy `pgid` ją już
/// wymienił, a dwa sygnały do jednej grupy to drugi sygnał wysłany do grupy, której już nie ma.
///
/// Odmowa jest ta sama, co dla pojedynczej wartości, i dotyczy CAŁEGO wiersza: `0`, liczba ujemna
/// albo nasza własna grupa na dowolnej pozycji znaczą, że tego wiersza nie umiemy przeczytać —
/// a wiersz odrzucony po cichu i wiersz, którego filtr nie zobaczył, dają identyczny plan
/// i różnią się dopiero na liście nieczytelnych.
fn usable_pgids(row: &RecoveryRow, own_pgid: i32) -> Result<Vec<i32>, &'static str> {
    let mut usable: Vec<i32> = Vec::new();
    for candidate in row.pgid.into_iter().chain(row.pgids.iter().copied()) {
        let pgid = usable_pgid(Some(candidate), own_pgid)?;
        if !usable.contains(&pgid) {
            usable.push(pgid);
        }
    }
    if usable.is_empty() {
        // Ani jednego numeru: spawn nie doszedł do zapisu. Ta sama odmowa, co przed
        // wprowadzeniem `pgids`, żeby wiersz bez grupy dalej mówił, dlaczego nic z nim nie robimy.
        return Err(reason::PGID_MISSING);
    }
    Ok(usable)
}

/// Co jeden wiersz znaczy dla planu.
#[derive(Debug)]
enum RowVerdict {
    /// Krok, którego odzyskiwanie nie dotyczy: już skończony albo jeszcze nieruszony.
    Settled,
    /// Krok przerwany awarią aplikacji.
    CutOff {
        /// Grupy do sprzątnięcia, w kolejności zapisu. Pusto, kiedy strażnik czasu startu nie
        /// przepuścił albo kiedy spawn nie doszedł do zapisu żadnego numeru.
        reap: Vec<i32>,
    },
    /// Krok **skończony**, po którym zostały grupy bez dowodu śmierci.
    ///
    /// 2026-09 (Z-01d) — trzeci werdykt, bo dwa poprzednie nie umiały opisać najczęstszego
    /// przypadku, dla którego to sprzątanie w ogóle istnieje: bieg zatrzymany przez człowieka
    /// kończy się jako `cancelled`, jego kroki jako `cancelled`, a wnuk, którego eskalacja nie
    /// dosięgła, biegnie dalej. Dla `Settled` był to wiersz bez znaczenia, a `CutOff` przepisałby
    /// status zamkniętego biegu na „przerwany" i skłamał o tym, co się stało.
    ///
    /// Sam reap, zero zmian statusu: bieg zszedł tak, jak zszedł, a jedyne, czego jeszcze
    /// potrzebuje, to żeby ktoś dobił to, co po nim zostało.
    LeftBehind {
        /// Grupy do sprzątnięcia. Pusto oznacza wiersz, o którym nie ma nic do powiedzenia.
        reap: Vec<i32>,
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
    let cut_off = match state {
        // Jedyne dwa stany, w których awaria aplikacji mogła przerwać krok w locie.
        StepState::Ready | StepState::Running => true,
        // Wyliczone po jednym zamiast `_`: ósmy stan kroku ma tutaj **nie skompilować**.
        // Cichym skutkiem `_` byłoby uznanie nowego stanu za skończony, czyli porzucenie
        // sierocego procesu bez ani jednego słowa w planie.
        //
        // 2026-09 (Z-01d) — te pięć stanów NIE kończy już wiersza bezwarunkowo: krok zamknięty
        // bez dowodu śmierci zostawił coś, co dalej może biec, i to jest osobny werdykt niżej.
        StepState::Pending
        | StepState::Succeeded
        | StepState::Failed
        | StepState::Cancelled
        | StepState::Skipped => false,
    };
    // Krok skończony wchodzi dalej wyłącznie wtedy, gdy nie ma dowodu ORAZ wypisał, jakie grupy
    // po sobie zostawił. Drugi warunek jest tu strażnikiem, nie wygodą (2026-09, Z-01d): pusta
    // lista znaczy „ten wiersz o grupach nic nie mówi", a nie „grup nie było". Tak wygląda
    // KAŻDY wiersz z indeksu SQLite (`rows_to_judge`), bo pełną listę niosą wyłącznie pliki
    // (niezmiennik 4) — bez tego warunku skasowanie `loadout.db` zmieniałoby to, do czego
    // sprzątanie strzela.
    if !cut_off && (row.death_proof || row.pgids.is_empty()) {
        return Ok(RowVerdict::Settled);
    }

    // Strażnik. Rozróżnienie, które tu stoi, jest całym AC-1: BRAK czasu startu to niewiedza
    // (wiersz idzie do `unreadable` i nic się z nim nie dzieje), a czas INNY niż ten jest
    // odpowiedzią — „restart maszyny już zabił sieroty" — więc wiersz zostaje obsłużony
    // w całości, tylko bez sprzątania. Nie ma czego zabijać, zostaje fakt przerwania do zapisu.
    let Some(recorded_boot) = row.run_boot_id.as_deref() else {
        // Krok skończony bez dowodu i bez czasu startu jest wierszem, o którym nie wiemy nic
        // ponad to, co już stoi w pliku: statusu nie przepisujemy, a strzelać nie wolno.
        // Wypisanie go jako nieczytelnego przy KAŻDYM otwarciu folderu zamieniłoby jedną
        // starą awarię w listę, która nigdy nie maleje.
        if !cut_off {
            return Ok(RowVerdict::Settled);
        }
        return Err(reason::NO_BOOT_TIME);
    };
    if recorded_boot != machine.boot_id {
        if !cut_off {
            return Ok(RowVerdict::Settled);
        }
        return Ok(RowVerdict::CutOff { reap: Vec::new() });
    }

    let reap = usable_pgids(row, machine.own_pgid)?;
    if cut_off {
        Ok(RowVerdict::CutOff { reap })
    } else {
        Ok(RowVerdict::LeftBehind { reap })
    }
}

/// Wiersze, które odzyskiwanie ma osądzić: kroki biegów, które baza wciąż uważa za żywe.
///
/// SQL stoi TUTAJ, a nie w `store/`, i to jest świadome: to jest jedyne zapytanie, które
/// istnieje wyłącznie dla odzyskiwania, a `store` jest wspólnym magazynem i nie ma powodu
/// znać jego pojęć. Odczyt idzie przez połączenie TYLKO DO ODCZYTU (`Store::reader`), bo
/// odzyskiwanie najpierw patrzy, a dopiero potem — osobno i świadomie — zapisuje.
///
/// `LEFT JOIN` nie jest tu potrzebny: krok bez biegu nie istnieje (klucz obcy z `ON DELETE
/// CASCADE`), a krok, którego biegu nie da się przeczytać, i tak wypadłby z decyzji jako
/// `UNKNOWN_RUN`.
pub fn rows_to_judge(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<RecoveryRow>> {
    let mut q = conn.prepare(
        "SELECT s.id, s.run_id, r.status, s.status, r.boot_id, s.pid, s.pgid
           FROM steps s
           JOIN runs r ON r.id = s.run_id
          WHERE r.status IN ('running', 'paused')
             OR s.status IN ('ready', 'running')",
    )?;
    let rows = q.query_map([], |row| {
        Ok(RecoveryRow {
            step_id: row.get(0)?,
            run_id: row.get(1)?,
            run_status: row.get(2)?,
            step_status: row.get(3)?,
            run_boot_id: row.get(4)?,
            pid: row.get(5)?,
            pgid: row.get(6)?,
            /* PUSTO I `false`, I TO NIE JEST BRAKUJĄCA KOLUMNA (2026-09, Z-01d). Pliki są prawdą,
             * SQLite jest indeksem (niezmiennik 4): `loadout.db` musi dać się skasować bez utraty
             * czegokolwiek, więc pole, którego nie da się odtworzyć z plików, nie ma prawa tam
             * powstać. Pełną listę grup i dowód śmierci niesie `run.json`, a czyta je
             * `commands::reconcile::rows_from_files` — czyli ta droga, która widzi biegi folderu.
             * Ta tutaj zna wyłącznie bibliotekę i zostaje przy tym, co w niej naprawdę stoi. */
            pgids: Vec::new(),
            death_proof: false,
        })
    })?;
    rows.collect()
}

/// Dopisuje grupy tego wiersza do planu, po jednej pozycji na grupę.
///
/// Duplikat znika bez słowa i to jest decyzja, nie usterka: dwa `SIGTERM` do tej samej grupy to
/// drugi sygnał wysłany do grupy, która już nie istnieje. Porównujemy po samym `pgid`, nie po
/// całym celu: ta sama grupa wpisana przy dwóch krokach jednego biegu jest jedną grupą, a numer
/// powtórzony w DWÓCH biegach znaczy, że co najmniej jeden z nich się myli — i wtedy jedno
/// pytanie o znacznik jest tym, co rozstrzyga, zamiast dwóch sygnałów w ciemno.
///
/// Wektor zamiast zbioru, bo kolejność wierszy jest częścią kontraktu, a wierszy jest tyle, ile
/// kroków w biegu (~20).
fn plan_the_reap(plan: &mut RecoveryPlan, row: &RecoveryRow, reap: &[i32]) {
    for &pgid in reap {
        if plan.reap.iter().any(|target| target.pgid == pgid) {
            continue;
        }
        plan.reap.push(ReapTarget {
            run_id: row.run_id.clone(),
            pgid,
        });
    }
}

/// Rozstrzyga, co zrobić z wierszami zastanymi przy starcie. **Niczego nie wykonuje.**
///
/// Cały stan systemu wjeżdża w [`Machine`], więc nie ma tu skąd wziąć czasu startu po raz drugi
/// i porównać go ze sobą (patrz [`Machine::boot_id`]).
///
/// Nie panikuje na żadnym wejściu. Nieznany status oraz nieużywalny `pgid` na bieżącym boocie
/// kończą się wpisem w [`RecoveryPlan::unreadable`] — niezmiennik 5.
#[must_use]
pub fn decide(rows: &[RecoveryRow], machine: &Machine) -> RecoveryPlan {
    let mut plan = RecoveryPlan::default();

    for row in rows {
        // Status biegu czytamy przed statusem kroku, ale zapis planujemy dopiero po dowodzie,
        // że ten konkretny krok został przerwany. Sam napis `running` przy biegu nie wystarcza:
        // starszy wiersz może zawierać wyłącznie skończone kroki i wtedy recovery nie ma czego
        // oznaczać jako przerwane.
        let Some(run_state) = RunState::from_wire(&row.run_status) else {
            plan.unreadable.push(Unreadable {
                step_id: row.step_id.clone(),
                reason: reason::UNKNOWN_RUN.to_owned(),
            });
            continue;
        };
        match read_row(row, machine) {
            Ok(RowVerdict::Settled) => {}
            // Krok skończony bez dowodu: sam reap, ani jednej zmiany statusu. Powód w całości
            // przy [`RowVerdict::LeftBehind`] (2026-09, Z-01d).
            Ok(RowVerdict::LeftBehind { reap }) => plan_the_reap(&mut plan, row, &reap),
            Ok(RowVerdict::CutOff { reap }) => {
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
                plan_the_reap(&mut plan, row, &reap);
                plan.step_status.push(StepStatusChange {
                    step_id: row.step_id.clone(),
                    status: STEP_FAILED.to_owned(),
                    reason: STEP_REASON_INTERRUPTED.to_owned(),
                });
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
pub fn apply(
    plan: &RecoveryPlan,
    reap: &mut dyn FnMut(&ReapTarget) -> ReapOutcome,
) -> RecoveryReport {
    let mut report = RecoveryReport::default();

    for target in &plan.reap {
        // Trzy odpowiedzi, trzy listy, i tylko jedna z nich jest dowodem. Cichy błąd, którego
        // ten `match` nie dopuszcza: `_ => report.reaped.push(pgid)`, czyli potraktowanie
        // każdego niezerowego wyniku `kill` jako „już nie żyje" i zameldowanie posprzątanego
        // biegu, którego nikt nie sprzątnął (niezmiennik 6).
        match reap(target) {
            ReapOutcome::ProvenDead => report.reaped.push(target.pgid),
            ReapOutcome::StillAlive => report.unproven.push(target.pgid),
            // Bez `continue`, bez drugiego wywołania: eskalacja do `SIGKILL` na cudzej grupie
            // trafiłaby dokładnie w ten niewinny proces, przed którym broni strażnik czasu
            // startu. Jedno wywołanie na grupę jest tu własnością pętli, nie zaleceniem.
            ReapOutcome::Foreign => report.foreign.push(target.pgid),
        }
    }

    report
}
