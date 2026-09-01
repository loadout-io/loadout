//! Rejestr rzeczy, które Loadout uruchomił dla człowieka — i których jest właścicielem.
//!
//! # Po co to w ogóle istnieje
//!
//! Zgłoszenie właściciela 2026-08-20: „jak napiszę aby coś odpalił jakąś apkę to chcę mieć też
//! po prawej gdzie są agenci info o procesach odpalonych itp, i po kliku mogę tam wejść".
//!
//! Do tego dnia nie było czego pokazać, i nie było to kwestią brakującego ekranu. Aplikacja
//! odpalona przez agenta stoi w JEGO grupie procesów; Loadout widzi po niej wyłącznie
//! `Line::Ran` — wiersz o czynności **zakończonej** (`engine::line`, pole `ok`). Nośnika na
//! „to biegnie teraz" nie było wcale. Kafelek zbudowany z wiersza `ran` byłby relacją, której
//! w danych nie ma (niezmiennik 17), a przycisk „stop" pod nim nie miałby czego ubić: tej grupy
//! nie założyliśmy, więc nie mamy jak dowieść jej śmierci (niezmiennik 6).
//!
//! Dlatego rzecz zamawia się komendą, a właścicielem jest Loadout: startuje ją
//! [`crate::engine::drivers::command::CommandDriver::start_to_stay`], czyli ta sama droga, którą
//! idzie każdy inny proces tego produktu — `process_group(0)`, `env_clear()` plus jawna lista,
//! potoki czytane do EOF. Nie jest to PTY i nie udaje terminala (decyzja D4 zostaje w mocy).
//!
//! # Cicha porażka, przed którą stoi ten plik
//!
//! Kafelek, który zostaje po rzeczy, która zeszła. „Running" przy komendzie zeszłej dwie minuty
//! temu jest tym samym kłamstwem, co widmowy agent z T-66 — a tamta fala pokazała, że ta klasa
//! wady wraca powierzchnia po powierzchni. Stąd [`StartedProcess::alive`] jest polem, a nie
//! założeniem: rejestr mówi, co wie, a kafelka nie rysuje wcale temu, kto zszedł
//! (`src/sections/run/rail/processes.ts`).
//!
//! # Dlaczego w `commands/`, a nie w `engine/`
//!
//! Bo to jest stan JEDNEJ aplikacji, trzymany między wywołaniami komend — dokładnie ta rola, dla
//! której `ipc::AppState` w ogóle istnieje. Silnik nie ma prawa o oknie wiedzieć (niezmiennik 1),
//! a start i eskalacja mieszkają tam, gdzie mieszkały: w sterowniku i w `supervisor.rs`
//! (niezmiennik 23). Tutaj jest wyłącznie mapa uchwytów i cztery czasowniki nad nią.
//!
//! # Druga lista: grupy bez dowodu śmierci (2026-08-28)
//!
//! Obok rzeczy, które **mają** żyć, ten rejestr trzyma od tego dnia rzeczy, które miały zejść
//! i nie zeszły ([`Unproven`]). Powód jest ten sam, dla którego istnieje ten plik, tylko od
//! drugiej strony: krok, którego grupy nie dało się dowieść jako martwej, zwalniał wcześniej
//! swój jedyny uchwyt i swoje miejsce w puli — czyli w tej samej chwili, w której Loadout
//! przyznawał, że nie wie, czy coś jeszcze biegnie, przestawał móc o to zapytać. Ta lista nie ma
//! kafelka i nie wchodzi do [`Processes::list`]: nikt tych grup nie zamawiał, a jedyne, co się
//! z nimi robi, to pyta o dowód jeszcze raz.

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::sync::{Mutex, PoisonError};

use crate::engine::drivers::AgentHandle;
use crate::engine::drivers::command::{CommandDriver, StartSpec, Staying};
use crate::engine::limits::Slot;
use crate::engine::supervisor::{GroupId, GroupProof};

/// Co okno wie o jednej uruchomionej rzeczy.
///
/// Trzy pola i ani jednego więcej — każde z nich odpowiada na pytanie, które człowiek naprawdę
/// zadaje kafelkowi: co to jest, którą grupę ubić, i czy to jeszcze biegnie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedProcess {
    /// Wiersz powłoki, co do znaku — patrz [`StartSpec::command`].
    pub command: String,
    /// `pgid` grupy. Jedyna liczba, która tę rzecz naprawdę identyfikuje, i jednocześnie ta,
    /// po której człowiek pozna ją w `ps`, a odzyskiwanie po awarii po niej sprząta.
    ///
    /// `i32`, nie `u32`: POSIX-owy `pid_t` jest znakowany, a `kill(-pgid, …)` używa znaku jako
    /// selektora grupy (powód w całości przy `engine::supervisor::GroupId`).
    pub pgid: i32,
    /// Czy w tej grupie ktoś jeszcze jest.
    ///
    /// POLE, nie założenie, i to jest cała obrona przed „Running" nad rzeczą, która zeszła:
    /// rejestr, który po prostu zapomina wpis w chwili śmierci, nie ma jak POWIEDZIEĆ oknu, że
    /// coś zeszło — a okno, które o tym nie usłyszy, zostawia kafelek na ekranie. Kafelka nie
    /// rysuje wtedy widok, nie ten plik.
    pub alive: bool,
}

/// Krok, po którym została grupa procesów **bez dowodu śmierci** — razem ze wszystkim, czego
/// przy niej nie wolno zwolnić.
///
/// # Po co ten typ istnieje (2026-08-28)
///
/// Bo `GroupProof::Alive` znaczy „ktoś w tej grupie dalej biegnie", a krok kończący się na tym
/// dowodzie zwalniał do tego dnia dwie rzeczy naraz: **jedyny uchwyt** do sesji i **miejsce
/// z puli**. Każde z osobna jest wadą, a razem są tą samą wadą dwa razy:
///
/// * porzucony `Box<dyn AgentHandle>` to grupa, o którą nikt już nie może zapytać — z `run.json`
///   zostaje `pgid`, czyli adres, a nie właściciel; nikt nie ponowi na nim eskalacji;
/// * oddany permit to miejsce, które natychmiast zajmuje następny agent po ~583 MB — przy
///   grupie, która dalej pali limit u dostawcy. Pula przestaje wtedy mówić prawdę o tym, ile
///   naprawdę biegnie (niezmiennik 11).
///
/// Dlatego zwolnienie prowadzi wyłącznie przez [`Unproven::released_by`] — jedyną drogę, która
/// żąda dowodu i przechodzi się nią dokładnie raz.
///
/// `#[must_use]` z tego samego powodu, co na [`GroupProof`]: wartość, którą da się porzucić
/// instrukcją, jest zobowiązaniem opcjonalnym. Pola są prywatne i nie ma dla nich ani jednego
/// gettera oddającego własność, więc uchwytu i permitu nie da się z tego typu wyjąć bokiem.
#[must_use]
pub struct Unproven {
    /// Jedyny właściciel sesji tego kroku. Dopóki tu jest, jest komu zlecić kolejną eskalację.
    handle: Box<dyn AgentHandle>,
    /// Miejsce z puli, zajęte tak długo, jak długo żyje ta wartość (`limits::Slot` zwalnia się
    /// w `Drop`). `None` dla kroków, które miejsca nie brały.
    slot: Option<Slot>,
    /// Adres z dowodu `Alive`, jeśli był. Tędy człowiek znajdzie tę grupę w `ps`, a odzyskiwanie
    /// po niej sprząta przy następnym starcie.
    group: Option<GroupId>,
}

impl fmt::Debug for Unproven {
    /// Ręcznie, bo uchwytu sesji nie da się pokazać sensownie, a `missing_debug_implementations`
    /// jest w tej skrzyni ostrzeżeniem, czyli pod `-D warnings` odmową.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Unproven")
            .field("group", &self.group)
            .field("holds_a_slot", &self.slot.is_some())
            .finish_non_exhaustive()
    }
}

/// Co zostało po próbie zwolnienia grupy — patrz [`Unproven::released_by`].
#[derive(Debug)]
#[must_use]
pub enum Kept {
    /// Dowód był: uchwyt zwolniony **dokładnie raz**, a miejsce z puli wraca do wołającego.
    ///
    /// Permit wraca, zamiast zginąć tutaj, bo ma przeżyć grupę, a nie odwrotnie: krok trzyma go
    /// jeszcze przez zapis do księgi i linię stanu, żeby następny agent nie ruszył w tej samej
    /// chwili, w której ten dopiero melduje koniec.
    Released(Option<Slot>),
    /// Dowodu nie było: nie zwolniono **niczego** i wszystko wraca w całości.
    Retained(Unproven),
}

impl Unproven {
    /// Przejmuje uchwyt i miejsce kroku, którego grupy nie dało się dowieść.
    // `#[must_use]` stoi na samym typie, więc powtórzony tutaj byłby drugą kopią tej samej
    // reguły (clippy `double_must_use` mówi to samo).
    pub fn new(handle: Box<dyn AgentHandle>, slot: Option<Slot>, group: Option<GroupId>) -> Self {
        Self {
            handle,
            slot,
            group,
        }
    }

    /// Zwalnia zasoby tej grupy — **wyłącznie przez dowód** i **dokładnie raz**.
    ///
    /// Dowód wchodzi tu przez wartość i `self` też, i to jest cała gwarancja zapisana w typie:
    /// nie ma innej drogi, którą `handle` i `slot` mogą zniknąć, a `self` przez wartość znaczy,
    /// że tą drogą da się przejść raz. `Alive` oddaje wszystko z powrotem — bez `ESRCH` nie
    /// wolno porzucić ani uchwytu, ani miejsca (niezmiennik 6).
    ///
    /// Dowód wraca do wołającego, bo jest jego meldunkiem: to samo zdanie, które trafia do
    /// dziennika i do `run.json`, a nie rzecz do połknięcia po drodze.
    pub fn released_by(self, proof: GroupProof) -> (GroupProof, Kept) {
        match proof {
            // `self.slot` wychodzi, `self.handle` ginie razem z resztą — czyli dokładnie tutaj,
            // i tylko tutaj, kończy się życie tej sesji.
            GroupProof::Dead { .. } => (proof, Kept::Released(self.slot)),
            GroupProof::Alive { .. } => (proof, Kept::Retained(self)),
        }
    }

    /// Pyta TĘ grupę o dowód jeszcze raz. Pełna eskalacja z nadzoru, nie samo pytanie.
    async fn asked_again(&mut self) -> GroupProof {
        self.handle.proof_of_death().await
    }
}

/// Wszystko, co Loadout uruchomił dla człowieka i jeszcze o tym wie.
///
/// Jeden na aplikację, w `ipc::AppState`, obok uchwytu biegu i rozmowy z liderem. Nie jeden na
/// zakres: rzecz uruchomiona w jednym folderze biegnie dalej po przełączeniu widoku, a lista,
/// która by ją wtedy ukryła, jest listą, po której zostaje osierocony proces palący maszynę.
#[derive(Debug, Default)]
pub struct Processes {
    /// `pgid` → uchwyt do tej jednej rzeczy.
    ///
    /// MAPA, NIE POLE, i to jest asercja (a) z AC-2 zapisana w typie: implementacja trzymająca
    /// JEDEN uchwyt osieroca pierwszą rzecz w chwili, w której człowiek uruchomi drugą — kafelków
    /// jest wtedy dwa, oba mówią „running", a jedna z tych grup nie ma już nikogo, kto mógłby
    /// zażądać od niej dowodu śmierci. Ten sam kształt zamknęło T-69 po stronie biegów
    /// (`ipc::AppState::begin_run`) i wraca on powierzchnia po powierzchni.
    ///
    /// `BTreeMap`, nie `HashMap`: kolejność [`Processes::list`] jest kolejnością kafelków
    /// w oknie, a lista przestawiająca się przy każdym odświeżeniu jest listą, po której nie da
    /// się kliknąć. Ten sam powód stoi przy `RunControl::voices`.
    ///
    /// `std::sync::Mutex` i **nigdy trzymany przez `await`** (niezmiennik 8): każde wzięcie tego
    /// zamka mieści się w jednym bloku, który wyjmuje albo przepisuje wartości i oddaje zamek —
    /// eskalacja czeka DOPIERO po jego zwolnieniu. Zamek trzymany przez zatrzymywanie zawiesiłby
    /// całe okno na czas okna łaski, czyli dokładnie wtedy, kiedy człowiek na coś patrzy.
    held: Mutex<BTreeMap<i32, Staying>>,

    /// Grupy kroków, których **nie dało się dowieść** jako martwych — po jednej pozycji na grupę.
    ///
    /// 2026-08-28 — osobno od [`Processes::held`], i to nie jest porządkowanie. Tamta mapa opisuje
    /// rzeczy, które człowiek kazał uruchomić i zostawić: mają kafelek, mają „stop" i mają żyć.
    /// Ta lista opisuje coś odwrotnego — grupy, które miały zejść i nie zeszły. Kafelka nie
    /// dostają (nikt ich nie zamawiał i nie ma czego pokazać poza `pgid`), a jedyne, co się
    /// z nimi robi, to pyta o dowód jeszcze raz.
    ///
    /// `Vec`, nie mapa po `pgid`: `Alive` bywa bez adresu (`chat.rs`), a klucz, którego czasem
    /// nie ma, jest kluczem, po którym gubi się wpisy.
    ///
    /// `std::sync::Mutex` i **nigdy trzymany przez `await`** (niezmiennik 8) — ten sam blokowy
    /// kształt, co przy `held`.
    unproven: Mutex<Vec<Unproven>>,
}

impl Processes {
    /// Ani jednej rzeczy — stan aplikacji, która właśnie wstała.
    #[must_use]
    pub fn new() -> Self {
        Self {
            held: Mutex::new(BTreeMap::new()),
            unproven: Mutex::new(Vec::new()),
        }
    }

    /// Przejmuje na własność krok, po którym została grupa bez dowodu śmierci.
    ///
    /// Wołane z jednego miejsca — z kroku, który wyczerpał swoją eskalację ([`super::run`]) — i to
    /// jest cała droga, którą te zasoby przeżywają swój bieg. Bez niej uchwyt i permit ginęłyby
    /// razem z ramką `one_turn`, czyli w chwili, w której Loadout właśnie przyznał, że nie wie,
    /// czy coś jeszcze biegnie.
    pub fn keep_unproven(&self, unproven: Unproven) {
        self.unproven
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(unproven);
    }

    /// Odpala komendę, która ma zostać, i zapisuje ją w rejestrze.
    ///
    /// Wraca **natychmiast**, z `pgid` już w ręku, i to jest cała różnica wobec kroku „sprawdź":
    /// rzecz żyje po powrocie tego wywołania. Wersja czekająca do końca komendy oddawałaby
    /// wołającemu wyłącznie nekrolog — nie byłoby czego pokazać na kafelku ani czego ubić przez
    /// cały czas, kiedy to naprawdę biegnie.
    pub fn start(&self, spec: &StartSpec) -> io::Result<StartedProcess> {
        let staying = CommandDriver::new().start_to_stay(spec)?;
        let started = one_of(&staying);
        /* WPIS POWSTAJE PO STARCIE, NIGDY PRZED, i to nie jest kolejność dla porządku: komenda,
         * której nie dało się odpalić, nie ma grupy, więc wpis zrobiony wcześniej byłby kafelkiem
         * nad rzeczą, której nie ma (niezmiennik 17), i musiałby go potem ktoś zdjąć na ścieżce
         * błędu — czyli dokładnie na tej, na której wołający wychodzi przez `?`.
         *
         * `pgid` jest tu kluczem unikalnym z definicji: dopóki grupa żyje, jądro nie wyda tej
         * liczby drugi raz, a rzecz, która zeszła, zostaje pod swoim kluczem do
         * [`Processes::stop`] albo [`Processes::close`]. */
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(started.pgid, staying);
        Ok(started)
    }

    /// Wszystko, o czym ten rejestr jeszcze wie — także to, co zeszło i nie zostało jeszcze
    /// sprzątnięte.
    ///
    /// Zeszłe rzeczy zostają w tej odpowiedzi z rozmysłu: to jest jedyna droga, którą okno
    /// dowiaduje się o śmierci czegoś, czego nie zatrzymało samo. Kafelka takiemu wpisowi nie
    /// rysuje widok (`src/sections/run/rail/processes.ts`), więc lista może być uczciwa, a ekran
    /// mimo to nie kłamie.
    #[must_use]
    pub fn list(&self) -> Vec<StartedProcess> {
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .map(one_of)
            .collect()
    }

    /// Co ta jedna rzecz do tej pory wypisała. `None`, kiedy rejestr jej nie zna.
    ///
    /// Osobno od [`Processes::list`], bo [`StartedProcess`] ma trzy pola i ma je zachować:
    /// wyjście jest długie, a lista jedzie na drut przy każdym odświeżeniu okna. Kto pyta o nie,
    /// pyta o JEDNĄ rzecz — tę, w którą właśnie wszedł.
    ///
    /// `None` jest wartością, nie błędem (niezmiennik 7): rzecz zatrzymana między odświeżeniem
    /// listy a kliknięciem w kafelek nie jest awarią.
    #[must_use]
    pub fn said(&self, pgid: i32) -> Option<String> {
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&pgid)
            .map(Staying::said)
    }

    /// „Stop" na kafelku: prosi TĘ grupę o zejście i oddaje **dowód**.
    ///
    /// `None`, kiedy takiej grupy rejestr nie zna — wartość, nie błąd (niezmiennik 7): rzecz,
    /// która zeszła sama między jednym a drugim kliknięciem, nie jest awarią.
    ///
    /// `GroupProof`, nigdy `io::Result<()>`: `Ok(())` znaczyłoby „wysłałem sygnał", a wołający
    /// przeczytałby „nie żyje" i zgasił kafelek nad żywym procesem (niezmiennik 6).
    pub async fn stop(&self, pgid: i32) -> Option<GroupProof> {
        // Uchwyt WYJMUJEMY pod zamkiem, a eskalacja czeka po jego zwolnieniu — niezmiennik 8
        // zapisany blokiem, nie komentarzem: `clippy::await_holding_lock` jest w tej skrzyni
        // odmową, a zamek trzymany przez okno łaski zawiesza całe okno.
        let taken = {
            let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
            held.remove(&pgid)
        };
        // Wpis zdejmujemy PRZED eskalacją, nie po niej: między jednym a drugim jest okno łaski,
        // a w nim lista pokazywałaby jako żywe coś, co właśnie schodzi z ekranu na oczach
        // człowieka, który nacisnął Stop.
        let mut staying = taken?;
        let proof = staying.stop().await;
        /* ESKALACJA, KTÓRA NIE DOWIODŁA ŚMIERCI, ODDAJE UCHWYT REJESTROWI. Zdjęcie wpisu przed
         * eskalacją jest słuszne dla rzeczy, która zeszła — ale przy `GroupProof::Alive` w tej
         * grupie ktoś dalej biegnie, a rejestr, który o niej zapomniał, nie ma jak jej już
         * zgłosić: [`Processes::list`] jej nie wymieni, więc kafelek gaśnie (reguła 2 w
         * `src/sections/run/rail/processes.ts`), a drugie kliknięcie „stop" trafi na `None`,
         * czyli na `Ok(())` w `ipc::stop_process` — sukces zameldowany nad rzeczą, która nie
         * umarła. To jest to samo kłamstwo, przed którym stoi ten plik, tylko w drugą stronę:
         * cisza nad grupą, która pali maszynę. Uchwyt wraca ten sam, więc następny „stop" umie
         * powtórzyć eskalację na tej grupie, a nie na jej wspomnieniu. */
        if matches!(proof, GroupProof::Alive { .. }) {
            self.held
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(pgid, staying);
        }
        Some(proof)
    }

    /// Zamknięcie okna: schodzą **wszystkie** i każda oddaje dowód śmierci swojej grupy.
    ///
    /// Powód stoi w nagłówku `recovery.rs`: rzecz, która przeżyje Loadouta, przechodzi pod PID 1
    /// i pracuje dalej — a odzyskiwanie po niej nie posprząta, bo nie ma wpisu w indeksie biegów.
    /// To jest ten sam defekt, który 2026-08-19 naprawiono dla biegów, i to samo, co
    /// [`super::chat::Threads::close`] robi dla rozmów.
    ///
    /// Po jednym dowodzie na rzecz, bo bilans jest kompletny tylko wtedy, kiedy widać KAŻDY
    /// z nich: jeden `Alive` wśród pięciu `Dead` jest dokładnie tym stanem, o którym nikt się nie
    /// dowie z liczby „zamknięto pięć".
    pub async fn close(&self) -> Vec<GroupProof> {
        // Cały rejestr wyjęty JEDNYM ruchem, pod zamkiem, i dopiero potem eskalacja — powód ten
        // sam, co przy [`Processes::stop`]. Pusty rejestr od tej chwili: rzecz, która ma zejść,
        // nie jest już rzeczą, którą wolno komukolwiek pokazać.
        let taken = {
            let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
            std::mem::take(&mut *held)
        };

        // PO KOLEI, nie równolegle, i cena jest zapisana: przy pięciu rzeczach, z których żadna
        // nie schodzi po SIGTERM-ie, zamykanie okna trwa pięć okien łaski zamiast jednego.
        // Zmierzone na tej fiksturze: powłoka w pętli schodzi w milisekundach, bo pętla dowodowa
        // pyta jądro co 10 ms (`supervisor::PROOF_POLL`). Wersja równoległa wymaga `FuturesUnordered`,
        // czyli skrzyni `futures`, a `src-tauri/Cargo.toml` leży poza blokiem OWNS tego zadania
        // (AGENTS.md §7) — więc to jest dług zapisany, nie przemilczany.
        let mut proofs = Vec::with_capacity(taken.len());
        for mut staying in taken.into_values() {
            proofs.push(staying.stop().await);
        }
        proofs.extend(self.prove_the_unproven().await);
        proofs
    }

    /// Ponawia eskalację na każdej grupie, która została bez dowodu, i zwalnia **wyłącznie** te,
    /// które odpowiedziały `Dead`.
    ///
    /// To jest jedyny czytelnik [`Processes::keep_unproven`] i cały powód, dla którego tamta
    /// lista istnieje: uchwyt utrzymany i nigdy więcej niezapytany byłby wyciekiem z dodatkowym
    /// krokiem. Grupa, która i tu nie da dowodu, wraca na listę razem ze swoim miejscem w puli —
    /// zamknięcie okna nie jest powodem, żeby zacząć zgadywać.
    ///
    /// Po kolei, nie równolegle, i z tego samego powodu, co pętla wyżej.
    async fn prove_the_unproven(&self) -> Vec<GroupProof> {
        let kept = {
            let mut unproven = self.unproven.lock().unwrap_or_else(PoisonError::into_inner);
            std::mem::take(&mut *unproven)
        };
        let mut proofs = Vec::with_capacity(kept.len());
        for mut one in kept {
            let proof = one.asked_again().await;
            let (proof, kept) = one.released_by(proof);
            match kept {
                // Zwolnione dokładnie raz: `Unproven` już nie istnieje, więc drugiego zwolnienia
                // nie da się napisać, a miejsce z puli ginie razem z tym `drop`.
                Kept::Released(slot) => drop(slot),
                Kept::Retained(still) => {
                    // Adres jedzie do dziennika, bo jest jedyną rzeczą, z którą człowiek może
                    // tu cokolwiek zrobić: po `pgid` znajdzie tę grupę w Monitorze aktywności.
                    tracing::error!(
                        group = ?still.group,
                        "a step's process group is still alive while Loadout is closing; it keeps \
                         the handle and its seat in the pool"
                    );
                    self.keep_unproven(still);
                }
            }
            proofs.push(proof);
        }
        proofs
    }
}

/// Co okno wie o TEJ jednej rzeczy — jedno miejsce, w którym uchwyt zamienia się w trzy pola.
///
/// Funkcja, nie trzy literały w trzech metodach: [`Processes::start`] i [`Processes::list`]
/// odpowiadają na to samo pytanie w dwóch chwilach, a dwie kopie tego przepisania rozjechałyby
/// się przy pierwszym polu dołożonym do [`StartedProcess`] (niezmiennik 13). Wtedy rzecz
/// zgłoszona przy starcie i ta sama rzecz na liście mówiłyby o sobie co innego.
fn one_of(staying: &Staying) -> StartedProcess {
    StartedProcess {
        command: staying.command().to_owned(),
        pgid: staying.group().pgid,
        alive: staying.alive(),
    }
}
