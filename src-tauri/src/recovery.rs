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
    /// PGID grupy procesów agenta. Jedyna liczba, po której da się sprzątnąć sierotę, i jedyna,
    /// którą wolno podać domykaczowi z [`apply`].
    pub pgid: Option<i32>,
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
    /// `pgid`-y do sprzątnięcia, w kolejności wierszy, bez duplikatów. Pusta lista jest
    /// poprawnym planem: po restarcie maszyny sieroty już nie żyją.
    pub reap: Vec<i32>,
    /// Biegi, które mają dostać [`RUN_INTERRUPTED`].
    pub run_status: Vec<RunStatusChange>,
    /// Kroki, które mają dostać [`STEP_FAILED`] z powodem [`STEP_REASON_INTERRUPTED`].
    pub step_status: Vec<StepStatusChange>,
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

/// Co jeden wiersz znaczy dla planu.
#[derive(Debug)]
enum RowVerdict {
    /// Krok, którego odzyskiwanie nie dotyczy: już skończony albo jeszcze nieruszony.
    Settled,
    /// Krok przerwany awarią aplikacji.
    CutOff {
        /// Grupa do sprzątnięcia. `None`, kiedy strażnik czasu startu nie przepuścił.
        reap: Option<i32>,
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
    // w całości, tylko bez sprzątania. Nie ma czego zabijać, zostaje fakt przerwania do zapisu.
    let Some(recorded_boot) = row.run_boot_id.as_deref() else {
        return Err(reason::NO_BOOT_TIME);
    };
    if recorded_boot != machine.boot_id {
        return Ok(RowVerdict::CutOff { reap: None });
    }

    let pgid = usable_pgid(row.pgid, machine.own_pgid)?;
    Ok(RowVerdict::CutOff { reap: Some(pgid) })
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
          WHERE r.status = 'running' OR s.status = 'running'",
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
        })
    })?;
    rows.collect()
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
