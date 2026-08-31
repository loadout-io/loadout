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
//! wady wraca powierzchnia po powierzchni. Dlatego EOF obu potoków uruchamia autonomiczne
//! zebranie, a wpis znika dopiero po `GroupProof::Dead`; dopóki dowodu nie ma, rejestr uczciwie
//! traktuje grupę jako żywą (niezmiennik 6, 2026-08-31).
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
use std::sync::{Arc, Mutex, PoisonError, Weak};

use tokio::sync::{Mutex as AsyncMutex, oneshot};
use tokio_util::sync::CancellationToken;

use crate::engine::drivers::AgentHandle;
use crate::engine::drivers::command::{CommandDriver, StartSpec, Staying, StayingOutput};
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
    /// Czy grupę nadal trzeba traktować jako żywą.
    ///
    /// Każdy zwrócony wpis ma tu `true`: `GroupProof::Alive` zachowuje go, a `Dead` usuwa cały
    /// wpis. Pole zostaje częścią drutu i czystej polityki widoku; nie wolno go zgasić na samym
    /// EOF ani wyniku lidera, zanim jądro odpowie `ESRCH` (2026-08-31).
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

/// Jeden wpis rejestru: fakty do synchronicznego odczytu i dokładnie jeden właściciel uchwytu.
#[derive(Debug)]
struct HeldProcess {
    command: String,
    pgid: i32,
    output: StayingOutput,
    /// Tokio, nie `std`, bo ten zamek CELOWO obejmuje `Staying::stop().await`: trzy konkurujące
    /// drogi muszą ustawić się w kolejce do tego samego `Supervised`, zamiast dwa razy wołać
    /// `wait` albo dwa razy sygnalizować tę samą grupę (niezmiennik 8, 2026-08-31).
    owner: AsyncMutex<Owner>,
}

/// Stan pojedynczego właściciela. `Released` powstaje dopiero po `GroupProof::Dead`, więc druga
/// droga może zwrócić `None` dopiero wtedy, gdy pierwsza naprawdę dostała `ESRCH`.
#[derive(Debug)]
enum Owner {
    Held(Staying),
    Released,
}

impl HeldProcess {
    /// Przechodzi przez istniejącą eskalację supervisora jako jedyny właściciel uchwytu.
    async fn prove(&self) -> Option<GroupProof> {
        let mut owner = self.owner.lock().await;
        let proof = match &mut *owner {
            Owner::Held(staying) => staying.stop().await,
            Owner::Released => return None,
        };
        if matches!(proof, GroupProof::Dead { .. }) {
            *owner = Owner::Released;
        }
        Some(proof)
    }
}

type Held = BTreeMap<i32, Arc<HeldProcess>>;

/// Wszystko, co Loadout uruchomił dla człowieka i jeszcze o tym wie.
///
/// Jeden na aplikację, w `ipc::AppState`, obok uchwytu biegu i rozmowy z liderem. Nie jeden na
/// zakres: rzecz uruchomiona w jednym folderze biegnie dalej po przełączeniu widoku, a lista,
/// która by ją wtedy ukryła, jest listą, po której zostaje osierocony proces palący maszynę.
#[derive(Debug)]
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
    held: Arc<Mutex<Held>>,

    /// Drop anuluje wyłącznie obserwatorów EOF. Zadania trzymają słabe odwołania do mapy i wpisu,
    /// a ten token zamyka także wąskie okno, w którym obserwator zdążył je podnieść tuż przed
    /// porzuceniem `Processes` (2026-08-31).
    natural_reapers: CancellationToken,

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

impl Default for Processes {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Processes {
    fn drop(&mut self) {
        // 2026-08-31 — najpierw odbieramy reaperom prawo do podnoszenia słabych wpisów. Potem
        // zwykły Drop mapy zwalnia jedyne mocne Arc i gwardia `Supervised` sprząta każdą grupę.
        self.natural_reapers.cancel();
    }
}

impl Processes {
    /// Ani jednej rzeczy — stan aplikacji, która właśnie wstała.
    #[must_use]
    pub fn new() -> Self {
        Self {
            held: Arc::new(Mutex::new(BTreeMap::new())),
            natural_reapers: CancellationToken::new(),
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
        let mut staying = CommandDriver::new().start_to_stay(spec)?;
        let natural_end = staying
            .natural_end()
            .ok_or_else(|| io::Error::other("a started command has no natural-end notification"))?;
        let group = staying.group();
        let entry = Arc::new(HeldProcess {
            command: staying.command().to_owned(),
            pgid: group.pgid,
            output: staying.output(),
            owner: AsyncMutex::new(Owner::Held(staying)),
        });
        let started = one_of(&entry);
        /* WPIS POWSTAJE PO STARCIE, NIGDY PRZED, i to nie jest kolejność dla porządku: komenda,
         * której nie dało się odpalić, nie ma grupy, więc wpis zrobiony wcześniej byłby kafelkiem
         * nad rzeczą, której nie ma (niezmiennik 17), i musiałby go potem ktoś zdjąć na ścieżce
         * błędu — czyli dokładnie na tej, na której wołający wychodzi przez `?`.
         *
         * Wpisu o tym samym `pgid` NIE NADPISUJEMY. Po `ESRCH` jądro może użyć liczby ponownie,
         * a spóźniony koniec starej rzeczy nie ma prawa ani usunąć, ani upuścić uchwytu nowej
         * (2026-08-31). Tożsamość `Arc` jest sprawdzana ponownie przy samym usunięciu. */
        {
            let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
            if held.contains_key(&started.pgid) {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!(
                        "process group {} is still owned by an earlier started command",
                        started.pgid
                    ),
                ));
            }
            held.insert(started.pgid, Arc::clone(&entry));
        }

        tokio::spawn(reap_after_natural_end(
            Arc::downgrade(&self.held),
            Arc::downgrade(&entry),
            natural_end,
            self.natural_reapers.clone(),
        ));
        Ok(started)
    }

    /// Wszystko, czego grupa nie ma jeszcze dowodu śmierci.
    ///
    /// Wpis znika dopiero po `GroupProof::Dead`; dlatego każdy wpis tej listy uczciwie pozostaje
    /// żywy także wtedy, gdy EOF uruchomił już autonomicznego reapera, ale jądro nie odpowiedziało
    /// jeszcze `ESRCH`.
    #[must_use]
    pub fn list(&self) -> Vec<StartedProcess> {
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .map(|entry| one_of(entry))
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
            .map(|entry| entry.output.said())
    }

    /// „Stop" na kafelku: prosi TĘ grupę o zejście i oddaje **dowód**.
    ///
    /// `None`, kiedy takiej grupy rejestr nie zna — wartość, nie błąd (niezmiennik 7): rzecz,
    /// która zeszła sama między jednym a drugim kliknięciem, nie jest awarią.
    ///
    /// `GroupProof`, nigdy `io::Result<()>`: `Ok(())` znaczyłoby „wysłałem sygnał", a wołający
    /// przeczytałby „nie żyje" i zgasił kafelek nad żywym procesem (niezmiennik 6).
    pub async fn stop(&self, pgid: i32) -> Option<GroupProof> {
        // Klon wpisu WYJMUJEMY pod zamkiem mapy, a eskalacja czeka po jego zwolnieniu. Właściciel
        // wewnątrz jest zamkiem Tokio właśnie dlatego, że stop, close i EOF muszą ustawić się
        // przy jednym uchwycie zamiast trzymać `std::sync::Mutex` przez await (niezmiennik 8).
        let entry = {
            let held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
            held.get(&pgid).cloned()
        };
        let entry = entry?;
        let proof = entry.prove().await;
        if proof
            .as_ref()
            .is_none_or(|proof| matches!(proof, GroupProof::Dead { .. }))
        {
            forget_if_current(&self.held, &entry);
        }
        proof
    }

    /// Zamknięcie okna: schodzą **wszystkie** i każda oddaje dowód śmierci swojej grupy.
    ///
    /// Powód stoi w nagłówku `recovery.rs`: rzecz, która przeżyje Loadouta, przechodzi pod PID 1
    /// i pracuje dalej — a odzyskiwanie po niej nie posprząta, bo nie ma wpisu w indeksie biegów.
    /// To jest ten sam defekt, który 2026-08-19 naprawiono dla biegów, i to samo, co
    /// [`super::chat::Threads::close`] robi dla rozmów.
    ///
    /// Po jednym dowodzie na każdy wpis, którego naturalny reaper nie dowiódł wcześniej. Jeden
    /// `Alive` wśród pięciu `Dead` zostaje w mapie razem z uchwytem; nie znika pod liczbą
    /// „zamknięto pięć".
    pub async fn close(&self) -> Vec<GroupProof> {
        // Migawka tożsamości, nie wyjęcie mapy: `Alive` musi zachować ten sam wpis i ten sam
        // uchwyt, a proces o ponownie użytym `pgid` nie może trafić do tej pętli bokiem.
        let taken = {
            let held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
            held.values().cloned().collect::<Vec<_>>()
        };

        // PO KOLEI, nie równolegle, i cena jest zapisana: przy pięciu rzeczach, z których żadna
        // nie schodzi po SIGTERM-ie, zamykanie okna trwa pięć okien łaski zamiast jednego.
        // Zmierzone na tej fiksturze: powłoka w pętli schodzi w milisekundach, bo pętla dowodowa
        // pyta jądro co 10 ms (`supervisor::PROOF_POLL`). Wersja równoległa wymaga `FuturesUnordered`,
        // czyli skrzyni `futures`, a `src-tauri/Cargo.toml` leży poza blokiem OWNS tego zadania
        // (AGENTS.md §7) — więc to jest dług zapisany, nie przemilczany.
        let mut proofs = Vec::with_capacity(taken.len());
        for entry in taken {
            let proof = entry.prove().await;
            if proof
                .as_ref()
                .is_none_or(|proof| matches!(proof, GroupProof::Dead { .. }))
            {
                forget_if_current(&self.held, &entry);
            }
            if let Some(proof) = proof {
                proofs.push(proof);
            }
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
fn one_of(staying: &HeldProcess) -> StartedProcess {
    StartedProcess {
        command: staying.command.clone(),
        pgid: staying.pgid,
        // Obecność wpisu znaczy „bez dowodu śmierci". `Alive` zostawia go tutaj; `Dead` usuwa.
        alive: true,
    }
}

/// Czy mapa nadal trzyma dokładnie TEN wpis, a nie nową rzecz pod ponownie użytym `pgid`.
fn is_current(held: &Mutex<Held>, entry: &Arc<HeldProcess>) -> bool {
    held.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&entry.pgid)
        .is_some_and(|current| Arc::ptr_eq(current, entry))
}

/// Usuwa wyłącznie własny wpis po dowodzie. Tożsamość `Arc`, nie sam PID, zamyka wyścig reuse.
fn forget_if_current(held: &Mutex<Held>, entry: &Arc<HeldProcess>) {
    let mut held = held.lock().unwrap_or_else(PoisonError::into_inner);
    if held
        .get(&entry.pgid)
        .is_some_and(|current| Arc::ptr_eq(current, entry))
    {
        held.remove(&entry.pgid);
    }
}

/// EOF obu drenowanych potoków uruchamia tę samą drogę dowodową co Stop i zamknięcie okna.
async fn reap_after_natural_end(
    held: Weak<Mutex<Held>>,
    entry: Weak<HeldProcess>,
    natural_end: oneshot::Receiver<()>,
    shutdown: CancellationToken,
) {
    let ended = tokio::select! {
        ended = natural_end => ended.is_ok(),
        () = shutdown.cancelled() => false,
    };
    if !ended {
        return;
    }

    // Dwa słabe odwołania są treścią, nie optymalizacją: żywy skrypt nie może utrzymywać ani
    // całego rejestru, ani jego `Supervised` po Drop `Processes` (2026-08-31).
    let Some(entry) = entry.upgrade() else {
        return;
    };
    let Some(registry) = held.upgrade() else {
        return;
    };
    if !is_current(&registry, &entry) {
        return;
    }
    drop(registry);

    let proof = tokio::select! {
        proof = entry.prove() => proof,
        () = shutdown.cancelled() => return,
    };
    if proof
        .as_ref()
        .is_none_or(|proof| matches!(proof, GroupProof::Dead { .. }))
        && let Some(registry) = held.upgrade()
    {
        forget_if_current(&registry, &entry);
    }
}
