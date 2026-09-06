//! Krok „sprawdź": komendę odpala Loadout, werdykt wystawia Loadout, nigdy agent.
//!
//! Ten plik jest rodzajem STEROWNIKA, dokładnie tak jak `claude.rs` i `codex.rs` — i celowo
//! **nie implementuje** `AgentDriver`. Nie ma tu sesji, modelu, promptu ani tury: jest komenda,
//! jej wyjście i zdanie „przeszło / nie przeszło", wystawione przez nas. Rozróżnienie, którego
//! ten plik broni, jest tym jedynym, dla którego produkt powstał: **co agent powiedział** kontra
//! **co się stało** (`docs/FOUNDATIONS.md` §2.1). Krok agenta o instrukcji
//! „uruchom testy i powiedz, czy przeszły" waliduje się, biegnie i kłamie — a wygląda na
//! skończony.
//!
//! # Trzy rzeczy, których ten plik nie robi, i po co ta lista tu stoi
//!
//! 1. **Nie startuje procesu z ręki.** `process_group(0)`, `env_clear()` plus lista
//!    przepuszczanych zmiennych i potoki mieszkają w [`supervisor::spawn`] — polityka jest jedna
//!    i w rdzeniu (niezmiennik 23). Druga kopia tej polityki w sterowniku jest dokładnie tym,
//!    jak w repo źródłowym po cichu umarło skanowanie sekretów.
//! 2. **Nie woła `supervisor::run_with_deadline`.** Wygląda idealnie, bo robi całą eskalację —
//!    i podaje `StdinPlan::Null` oraz **nigdy nie opróżnia potoków**. `cargo test` piszący
//!    więcej niż ~64 KB staje wtedy na `write`, krok wisi na 100% „running", a wyjścia, czyli
//!    jedynej rzeczy, z której powstaje werdykt, i tak nie ma. Potoki czytamy sami, do EOF.
//! 3. **Nie orzeka na samym kodzie wyjścia** (niezmiennik 19). Suita, która nie uruchomiła ani
//!    jednego testu, wychodzi zerem; `os._exit(0)` na poziomie modułu zazielenia wszystko.
//!    Dlatego werdykt stoi na dwóch rzeczach naraz — kodzie wyjścia **i** dopasowaniu wzorca.
//!
//! # Jedno ograniczenie na cały plik
//!
//! Zero warunków platformowych, zero stałych sygnałów, zero `killpg`. Zabijanie i eskalacja
//! należą do `supervisor.rs` — to jest niezmiennik 3 i pilnuje go `checks/quick-boundary.sh`.
//! Ten plik prosi o zatrzymanie neutralnym czasownikiem i czyta zwrócony dowód.

use std::io;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use tokio::io::AsyncReadExt;
use tokio::process::{ChildStderr, ChildStdout};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use super::super::supervisor::{self, GroupId, GroupProof, StdinPlan, Supervised};

pub mod assessment;
pub mod required;
pub use assessment::ProofMode;
pub use required::RequiredTests;

/// Ile jeden krok „sprawdź" ma prawo trwać.
///
/// Trzydzieści minut, bo tyle wynosi budżet naszej własnej pełnej bramki (1800 s,
/// `checks/full-test.sh`) — a to jest najdłuższe sprawdzenie, jakie ten produkt zna z pomiaru,
/// nie z domysłu. Stała, nie pole na kafelku: pole w schemacie bez kontrolki w UI jest
/// kontrolką bez handlera (niezmiennik 16), a kontrolki jeszcze nie ma.
pub const GIVE_UP_AFTER: Duration = Duration::from_mins(30);

/// Powłoka, przez którą idzie komenda człowieka.
///
/// Przez powłokę, a nie listą argumentów, bo człowiek napisze `./verify.sh full && npm test`,
/// a nie `["./verify.sh", "full"]`. Dwie rzeczy do zapisania obok (niezmiennik 24):
///
/// (a) ten literał jest DŁUGIEM. W dniu, w którym pojawi się Windows, wybór powłoki przenosi
/// się do `supervisor.rs`, do tej samej gałęzi warunkowej, w której stoi `ProcessGroup::leader()`
/// — bo to tam mieszka jedyna wiedza o platformie w tym drzewie (niezmiennik 3).
///
/// (b) niezmiennik 9 **nie jest tu złamany**. Zakazuje promptów i sekretów w argumentach; komenda
/// sprawdzająca nie jest ani jednym, ani drugim i ma być widoczna w `ps`, żeby człowiek poznał
/// swój własny bieg.
const SHELL: &str = "/bin/sh";

/// Jedyny metaznak wzorca dowodu: „co najmniej jedna cyfra".
///
/// Stała, a nie literał w dwóch miejscach: ta sekwencja jest jednocześnie tym, co człowiek pisze
/// w linii `expect:` naszej własnej bramki (`AGENTS.md` §2a punkt 4), i tym, po czym [`proof_matches`]
/// rozcina wzorzec. Dwie kopie tego napisu rozjechałyby się przy pierwszej zmianie notacji, a wtedy
/// wzorce zapisane w plikach workflow przestałyby znaczyć to, co znaczyły.
const DIGIT_RUN: &str = r"(\d+)";

/// Co uruchomić, po czym poznać, że ruszyło, i gdzie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckSpec {
    /// Wiersz powłoki, dosłownie jak wpisał go człowiek.
    pub command: String,
    /// Wzorzec dowodu — zwykły tekst z jednym metaznakiem, patrz [`proof_matches`].
    pub proof: String,
    /// Katalog roboczy kroku.
    pub cwd: PathBuf,
    /// V-01: dokładne testy, które to sprawdzenie ma potwierdzić.
    ///
    /// Pusta lista zachowuje dotychczasowy kontrakt: werdykt liczy się z kodu wyjścia
    /// i wzorca dowodu. Niepusta dokłada pytanie, na które sam licznik nie odpowiada —
    /// KTÓRE testy przeszły (incydent I-04: dwa niezwiązane przejścia zamiast trzynastu
    /// wymaganych, exit 0, dodatni licznik, zielony krok).
    pub required_tests: Vec<String>,
}

/// Co z komendy wyszło.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
    /// Dowód z osobnego egzaminatora; None zachowuje dawny kontrakt command/proof.
    pub assessment: Option<assessment::Assessment>,
    /// V-01: co się stało z wymaganymi testami. `None`, kiedy sprawdzenie ich nie wymienia.
    pub required: Option<RequiredTests>,
    /// Werdykt Loadouta. Liczony z [`passed`], czyli z kodu wyjścia **i** dopasowania naraz.
    pub passed: bool,
    /// Kod wyjścia. `None`, kiedy proces zginął od sygnału i kodu po prostu nie ma — a `None`
    /// nigdy nie jest przejściem, bo `None` to nie zero.
    pub exit_code: Option<i32>,
    /// Czy wzorzec dowodu trafił w wyjście. Osobne pole, bo człowiek ma widzieć, KTÓRA połowa
    /// werdyktu zawiodła: „testy padły" i „nic nie uruchomiło się" naprawia się inaczej.
    pub matched: bool,
    /// Złączone stdout i stderr, w kolejności odczytu. Dwoje czytelników (niezmiennik 21):
    /// werdykt i przekazanie do następnego kroku.
    pub output: String,
    /// Ile to trwało, na naszym zegarze.
    pub took: Duration,
}

/// Czym skończył się jeden krok „sprawdź".
///
/// Trzy warianty, bo trzy rzeczy są prawdziwie różne: komenda wróciła sama, człowiek nacisnął
/// Stop, minął limit czasu. Dwa ostatnie niosą [`GroupProof`], a nie samo „zatrzymane" — dopóki
/// jądro nie odpowiedziało `ESRCH`, grupa jest żywa (niezmiennik 6), a `tokio::time::timeout`
/// wokół czekania anuluje zadanie Rusta, nie proces (niezmiennik 10).
#[derive(Debug)]
pub enum CheckHow {
    /// Komenda doszła do końca i mamy z czego orzekać.
    Ran(CheckReport),
    /// Zatrzymał to człowiek. **Wartość, nie błąd** (niezmiennik 7).
    Stopped(GroupProof),
    /// Krok przekroczył [`GIVE_UP_AFTER`].
    Overdue(GroupProof),
}

/// Wynik kroku razem z grupą procesów, w której biegł.
///
/// `group` jest tu dlatego, że ktoś ją czyta (niezmiennik 21): zapisuje ją księga biegu, zanim
/// popłynie cokolwiek z wyjścia, i po niej sprząta odzyskiwanie po awarii aplikacji.
#[derive(Debug)]
pub struct CheckEnd {
    /// `pid` lidera i `pgid` grupy — zwykła wartość, dostępna od razu po starcie [T7 §6.2].
    pub group: GroupId,
    pub how: CheckHow,
}

/// Żywa komenda sprawdzająca.
///
/// Uchwyt, a nie jedno wywołanie „zrób wszystko", i to jest wymóg z niezmiennika 6: `pgid` musi
/// dać się przeczytać, ZANIM ktokolwiek przeczyta pierwszy bajt wyjścia — inaczej po awarii
/// aplikacji nie ma kogo zapytać, co sprzątnąć.
#[derive(Debug)]
pub struct Checking {
    /// Zwykła wartość, wzięta ze [`supervisor::spawn`] synchronicznie.
    group: GroupId,
    /// Nadzorowana grupa procesów. Porzucenie tego pola też ją zabija — gwardia siedzi
    /// w `Drop` uchwytu, a normalną drogą jest [`Checking::cancel`].
    handle: Supervised,
    /// Wzorzec dowodu tego kroku, przepisany ze [`CheckSpec`].
    proof: String,
    /// V-01: wymagane testy tego kroku, przepisane ze [`CheckSpec`].
    required: Vec<String>,
    proof_mode: ProofMode,
    /// Od kiedy liczymy [`CheckReport::took`] i [`GIVE_UP_AFTER`].
    began: Instant,
}

impl Checking {
    /// `pid` i `pgid`, dostępne od razu po starcie i bez czekania na cokolwiek z wyjścia.
    #[must_use]
    pub const fn group(&self) -> GroupId {
        self.group
    }

    /// Ile temu krokowi zostało z [`GIVE_UP_AFTER`].
    ///
    /// `saturating_sub`, bo limit mógł już minąć: `Duration` nie umie być ujemna, a odejmowanie
    /// z przepełnieniem jest w trybie debug paniką — czyli awarią silnika (AGENTS.md §4) w miejscu,
    /// w którym poprawną odpowiedzią jest „zero, czas się skończył".
    fn left(&self) -> Duration {
        GIVE_UP_AFTER.saturating_sub(self.began.elapsed())
    }

    /// Czeka na koniec komendy, na Stop albo na limit czasu — i oddaje jedno z trzech.
    ///
    /// # Kolejność, której nie wolno odwrócić
    ///
    /// **Najpierw oba potoki do EOF, dopiero potem `wait()`.** Odwrotnie wygląda naturalniej
    /// i wiesza krok: bufor potoku ma ~64 KB, więc `cargo test` piszący więcej staje na `write`
    /// i nigdy nie dojdzie do wyjścia, na które czekamy. To jest ta sama pomyłka, dla której
    /// [`supervisor::run_with_deadline`] nie nadaje się na tę drogę, choć eskalację ma gotową.
    ///
    /// Stop i limit czasu są sprawdzane w OBU czekaniach, bo w obu można w nich utknąć: pierwsze
    /// stoi na potoku, który trzyma wnuk, drugie na liderze, który nie chce zejść. Każde z nich
    /// wychodzi tą samą drogą — przez [`Supervised::stop`], czyli przez eskalację i **dowód**,
    /// nie przez zdjęcie zadania Rusta (niezmienniki 6 i 10).
    pub async fn settle(&mut self, cancel: &CancellationToken) -> CheckEnd {
        let group = self.group;

        /* POTOKI WYJMUJEMY Z UCHWYTU, ZANIM ZACZNIE SIĘ CZEKANIE, i to nie jest kwestia stylu:
         * czytanie pożycza je na całe opróżnianie, a `Supervised::stop` pożycza uchwyt mutowalnie.
         * Wyjęte, jadą do zadania czytającego na własność i obie rzeczy mogą dziać się naraz. */
        let reading = read_to_eof(
            self.handle.stdout(),
            self.handle.stderr(),
            CheckCapture::for_mode(&self.proof, self.proof_mode, &self.required),
        );
        tokio::pin!(reading);

        let captured = {
            let overdue = tokio::time::sleep(self.left());
            tokio::pin!(overdue);
            tokio::select! {
                // `biased`, bo komenda, która właśnie zamknęła wyjście, ma pierwszeństwo przed
                // Stopem wpadającym w tej samej chwili: zatrzymywanie czegoś, co już zeszło,
                // zamieniałoby udane sprawdzenie w anulowane zależnie od tego, który poll wypadł
                // pierwszy. Limit czasu stoi PO Stopie z tego samego powodu.
                biased;
                said = &mut reading => said,
                () = cancel.cancelled() => return self.give_up(group, CheckHow::Stopped).await,
                () = &mut overdue => return self.give_up(group, CheckHow::Overdue).await,
            }
        };

        // EOF na obu potokach znaczy, że nikt już do nich nie pisze — więc dopiero TERAZ `wait()`
        // nie ma jak stanąć na pełnym buforze.
        let left = self.left();
        let ended = {
            let waiting = self.handle.wait();
            tokio::pin!(waiting);
            let overdue = tokio::time::sleep(left);
            tokio::pin!(overdue);
            tokio::select! {
                biased;
                got = &mut waiting => Settled::Exited(got),
                () = cancel.cancelled() => Settled::Stopped,
                () = &mut overdue => Settled::Overdue,
            }
            // Pożyczka uchwytu kończy się razem z tym blokiem — dopiero po nim wolno zawołać
            // `stop()` na tym samym uchwycie.
        };

        match ended {
            Settled::Exited(status) => CheckEnd {
                group,
                // Kod wyjścia albo jego BRAK. `None` przychodzi z dwóch stron: proces zginął od
                // sygnału (`ExitStatus::code()` nie ma czego oddać) albo statusu nie dało się
                // zebrać. Obie odpowiedzi znaczą to samo dla werdyktu — `None` to nie zero.
                how: CheckHow::Ran(self.report(status.ok().and_then(|how| how.code()), captured)),
            },
            Settled::Stopped => self.give_up(group, CheckHow::Stopped).await,
            Settled::Overdue => self.give_up(group, CheckHow::Overdue).await,
        }
    }

    /// Werdykt i wszystko, z czego powstał.
    ///
    /// `matched` obok `passed`, a nie zamiast: człowiek ma widzieć, KTÓRA połowa zawiodła. „Testy
    /// padły" naprawia się inaczej niż „nic się nie uruchomiło", a jedno pole `bool` na dwa różne
    /// stany wysyłałoby go w połowie przypadków w złe miejsce.
    fn report(&self, exit_code: Option<i32>, captured: Captured) -> CheckReport {
        let Captured {
            text,
            matched,
            required,
            assessment_bytes,
            read_failed,
        } = captured;
        let assessment =
            assessment_bytes.map(|bytes| assessment::assess(&bytes, exit_code, read_failed));
        CheckReport {
            passed: assessment.as_ref().map_or_else(
                || verdict(exit_code, matched),
                |one| one.outcome == assessment::Outcome::Passed,
            )
            /* LICZNIK PRZEJŚĆ NIE MÓWI, KTÓRE TESTY PRZESZŁY (V-01, incydent I-04). Dwa
             * niezwiązane przejścia dają dodatni licznik i zerowy kod wyjścia dokładnie tak
             * samo, jak trzynaście wymaganych. */
            && required.as_ref().is_none_or(RequiredTests::all_confirmed),
            exit_code,
            matched: assessment
                .as_ref()
                .map_or(matched, |one| one.receipt.is_some()),
            output: assessment.as_ref().map_or(text, |one| one.reason.clone()),
            assessment,
            required,
            took: self.began.elapsed(),
        }
    }

    /// Zatrzymanie na żądanie albo po limicie czasu — jedną drogą, bo różnica jest w NAZWIE
    /// wyniku, nie w tym, co trzeba zrobić z grupą procesów.
    ///
    /// Oba warianty niosą [`GroupProof`], więc obie drogi muszą przejść przez eskalację: nie da
    /// się zwrócić dowodu, nie zabijając grupy. To jest cały niezmiennik 10 zapisany w typie —
    /// `tokio::time::timeout` wokół czekania anuluje zadanie Rusta i zostawia grupę żywą.
    async fn give_up(&mut self, group: GroupId, how: fn(GroupProof) -> CheckHow) -> CheckEnd {
        CheckEnd {
            group,
            how: how(self.handle.stop(supervisor::DEFAULT_GRACE).await),
        }
    }

    /// Prosi grupę o zejście i oddaje **dowód**, nie potwierdzenie wysłania sygnału.
    ///
    /// Wołane drugi raz na tej samej grupie nadal odpowiada `Dead` i nie produkuje drugiego
    /// wyniku: powtórzone zatrzymanie jest normalną ścieżką, nie błędem (`Supervised::stop`).
    pub async fn cancel(&mut self) -> GroupProof {
        self.handle.stop(supervisor::DEFAULT_GRACE).await
    }

    /// **Każda** grupa, którą ta komenda uruchomiła — do zapisania w księdze biegu.
    ///
    /// 2026-09 (Z-01d) — pytanie idzie wprost do uchwytu, bo to on trzyma wiedzę o drzewie.
    /// Krok „sprawdź" odpala wiersz powłoki, a wiersz powłoki potrafi być całym `npm test`
    /// z serwerem w tle: `group()` mówi wtedy o liderze, a płaci się za wnuki [T7 §3.1].
    pub fn descendant_groups(&mut self) -> Vec<i32> {
        self.handle.descendant_groups()
    }
}

/// Komenda, która przeżyła pełną eskalację, jest ocalałym dokładnie tak samo jak sesja agenta.
///
/// 2026-09 (Z-4) — do tego dnia ten uchwyt po prostu spadał z ramki `run_check`: grupa dostawała
/// od `Drop` dziewiątkę, ale nikt nie dowodził `ESRCH`, więc rejestr aplikacji nie miał czego
/// ponawiać, a miejsce z puli wracało do niej razem z krokiem. Ten `impl` jest całą drogą, którą
/// komenda dojeżdża do [`crate::commands::processes::Processes`] — sam plik dalej nie wie, że taki
/// rejestr istnieje (niezmiennik 1).
#[async_trait::async_trait]
impl supervisor::Leftover for Checking {
    async fn ask_again(&mut self) -> GroupProof {
        self.cancel().await
    }

    fn address(&self) -> Option<GroupId> {
        Some(self.group)
    }
}

/// Czym skończyło się czekanie na komendę.
///
/// Trzy stany, bo trzy rzeczy są prawdziwie różne i każda kończy się czymś innym. `Option` umiał
/// powiedzieć dwa — ten sam powód stoi przy `commands::run::Ended`.
enum Settled {
    /// Lider zszedł sam. `io::Result`, bo statusu czasem nie da się zebrać, a to nie jest to samo
    /// co „wyszedł zerem".
    Exited(io::Result<ExitStatus>),
    /// Człowiek nacisnął Stop.
    Stopped,
    /// Minęło [`GIVE_UP_AFTER`].
    Overdue,
}

/// Który potok coś powiedział — i ile.
///
/// Odpowiedź **wychodzi** z `select!` zamiast dopisywać się do bufora w gałęzi, i to jest wymóg
/// pożyczek, nie ozdoba: futury odczytu trzymają swoje bufory pożyczone mutowalnie, dopóki całe
/// wyrażenie `select!` się nie skończy.
enum Said {
    /// Ze strumienia wyjścia.
    Out(io::Result<usize>),
    /// Ze strumienia skarg. `cargo test` pisze podsumowanie na wyjście, a `npm` swoje tutaj —
    /// dlatego werdykt czyta OBA (AC-2).
    Complaints(io::Result<usize>),
}

/// Ile bajtów bierzemy z potoku za jednym razem. Osiem kilobajtów, czyli ósma część potoku:
/// mniej znaczy więcej przebudzeń na tę samą treść, więcej nie przyspiesza już niczego.
///
/// Porcje leżą na **stercie**, nie na stosie, i to jest wymóg, nie gust: dwa bufory po 8 KB
/// wewnątrz `async fn` wchodzą do wielkości future'a, a ten future jedzie przez `Live::step`
/// i `CommandDriver::run` w górę biegu. Zmierzone: 17 440 bajtów na jedno wywołanie kroku
/// i `clippy::large_futures` na czerwono w pełnej bramce.
const CHUNK: usize = 8 * 1024;

/// Ile ostatnich bajtów wyjścia zachowuje jeden proces — sprawdzający albo zostający.
///
/// Sześćdziesiąt cztery kilobajty to kilkaset linii, czyli tyle, ile człowiek może przejrzeć.
/// Rzecz, która ZOSTAJE, trzyma tyle przez całe życie; krok „sprawdź" zostawia tyle po dojściu
/// obu potoków do EOF, ale jego dopasowanie czyta każdy znak przed odrzuceniem początku.
const KEEP_LAST: usize = 64 * 1024;

/// Pierwsze zdanie każdego przyciętego wyniku — także liczy się do [`KEEP_LAST`].
const OMITTED: &str = "[Loadout omitted earlier output from this check.]\n";

/// Jedna część wzorca dowodu: znak literalny albo jedyny metaznak `(\d+)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProofUnit {
    Literal(char),
    Digits,
}

/// Żywy stan dopasowania podciągu. Bool pamięta niezerową cyfrę w pierwszej grupie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProofState {
    /// Następna część wzorca jeszcze czeka na znak.
    At(usize, bool),
    /// Grupa pod tym indeksem dostała już co najmniej jedną cyfrę i może trwać albo się domknąć.
    InDigits(usize, bool),
}

/// Inkrementalny rdzeń dopasowania, wspólny dla strumienia procesu i [`proof_matches`].
struct ProofScan {
    units: Vec<ProofUnit>,
    primary_digit: Option<usize>,
    states: Vec<ProofState>,
    enabled: bool,
    matched: bool,
}

impl ProofScan {
    fn new(proof: &str) -> Self {
        let enabled = !proof.trim().is_empty();
        let mut units = Vec::new();
        if enabled {
            let mut parts = proof.split(DIGIT_RUN).peekable();
            while let Some(literal) = parts.next() {
                units.extend(literal.chars().map(ProofUnit::Literal));
                if parts.peek().is_some() {
                    units.push(ProofUnit::Digits);
                }
            }
        }
        let primary_digit = units
            .iter()
            .position(|unit| matches!(unit, ProofUnit::Digits));
        Self {
            units,
            primary_digit,
            states: Vec::new(),
            enabled,
            matched: false,
        }
    }

    fn take(&mut self, text: &str) {
        if self.matched || !self.enabled {
            return;
        }
        for character in text.chars() {
            self.take_character(character);
            if self.matched {
                return;
            }
        }
    }

    fn take_character(&mut self, character: char) {
        let mut next = Vec::with_capacity(self.states.len().saturating_add(2));

        // Nowa próba przy KAŻDYM znaku sprawia, że wzorzec szuka podciągu, nie tylko początku.
        self.advance(0, false, character, &mut next);
        for state in &self.states {
            match *state {
                ProofState::At(at, positive) => self.advance(at, positive, character, &mut next),
                ProofState::InDigits(at, positive) => {
                    if character.is_ascii_digit() {
                        let positive =
                            positive || (self.primary_digit == Some(at) && character != '0');
                        Self::push(&mut next, ProofState::InDigits(at, positive));
                    }
                    // 2026-08-31 — grupa może domknąć się PRZED tym znakiem. Ta druga droga
                    // jest nawrotem, dzięki któremu `(\d+)5` trafia w `125`, a wiele grup ma
                    // dokładnie tę samą semantykę w strumieniu i w gotowym tekście.
                    self.advance(at + 1, positive, character, &mut next);
                }
            }
        }

        self.matched = next.iter().any(|state| match *state {
            ProofState::At(at, positive) => {
                at == self.units.len() && (self.primary_digit.is_none() || positive)
            }
            ProofState::InDigits(at, positive) => at + 1 == self.units.len() && positive,
        });
        self.states = next;
    }

    fn advance(&self, at: usize, positive: bool, character: char, next: &mut Vec<ProofState>) {
        match self.units.get(at) {
            Some(ProofUnit::Literal(expected)) if *expected == character => {
                Self::push(next, ProofState::At(at + 1, positive));
            }
            Some(ProofUnit::Digits) if character.is_ascii_digit() => {
                // 2026-09-06 — `0 passed` dawało zielone po exit 0 (niezmiennik 19).
                // Pierwsza grupa jest licznikiem przejść; następne nie mogą go ratować.
                // Wystarcza niezerowa cyfra: brak przepełnienia i ten sam stan między porcjami.
                let positive = positive || (self.primary_digit == Some(at) && character != '0');
                Self::push(next, ProofState::InDigits(at, positive));
            }
            _ => {}
        }
    }

    fn push(states: &mut Vec<ProofState>, state: ProofState) {
        if !states.contains(&state) {
            states.push(state);
        }
    }
}

/// Wynik drenowania potoków: ograniczony tekst i werdykt policzony z pełnego strumienia.
struct Captured {
    text: String,
    matched: bool,
    /// V-01: co się stało z wymaganymi testami. `None`, kiedy krok ich nie wymienia.
    required: Option<RequiredTests>,
    assessment_bytes: Option<Vec<u8>>,
    read_failed: bool,
}

/// Streamingowy odbiorca jednego kroku „sprawdź".
///
/// `pending` trzyma najwyżej trzy bajty niedokończonego znaku UTF-8. `tail` nigdy nie rośnie
/// ponad sufit plus jedną porcję z potoku, a `proof` nie przechowuje wyjścia — tylko żywe stany.
struct CheckCapture {
    tail: String,
    pending: Vec<u8>,
    proof: ProofScan,
    /// V-01: wymagane testy śledzone w locie, wyłącznie po `stdout`.
    ///
    /// `stdout`, a nie złączony strumień, i to jest jedyna droga, na której cudze zdanie nie
    /// staje się wynikiem testu: runnery wypisują werdykty na `stdout`, a `stderr` niesie to,
    /// co napisał ktokolwiek inny w tej komendzie — łącznie z tekstem, który wygląda jak
    /// linia libtesta i nią nie jest.
    required: Option<required::Scan>,
    dropped: bool,
    assessment_bytes: Option<Vec<u8>>,
    read_failed: bool,
}

impl CheckCapture {
    fn new(proof: &str) -> Self {
        Self {
            tail: String::new(),
            pending: Vec::with_capacity(3),
            proof: ProofScan::new(proof),
            required: None,
            dropped: false,
            assessment_bytes: None,
            read_failed: false,
        }
    }

    fn for_mode(proof: &str, mode: ProofMode, required: &[String]) -> Self {
        let mut capture = Self::new(proof);
        if mode == ProofMode::ExternalAssessmentV1 {
            capture.assessment_bytes = Some(Vec::new());
        }
        if !required.is_empty() {
            capture.required = Some(required::Scan::new(required));
        }
        capture
    }

    fn take_stream(&mut self, stdout: bool, chunk: &[u8]) {
        if stdout && let Some(bytes) = &mut self.assessment_bytes {
            let left = (assessment::MAX_BYTES + 1).saturating_sub(bytes.len());
            bytes.extend_from_slice(&chunk[..left.min(chunk.len())]);
        }
        // Wymagane testy widzą WYŁĄCZNIE `stdout` — powód przy polu `required`.
        if stdout && let Some(scan) = &mut self.required {
            scan.take(&String::from_utf8_lossy(chunk));
        }
        self.take(chunk);
    }

    fn take(&mut self, chunk: &[u8]) {
        // Jedyna kopia wielkości porcji: łączy do trzech bajtów z poprzedniego odczytu z nowym
        // kawałkiem. Pełny strumień nigdy nie powstaje ani jako `Vec`, ani jako `String`.
        let mut bytes = std::mem::take(&mut self.pending);
        bytes.extend_from_slice(chunk);
        let mut from = 0;

        while from < bytes.len() {
            match std::str::from_utf8(&bytes[from..]) {
                Ok(text) => {
                    self.take_text(text);
                    return;
                }
                Err(error) => {
                    let valid_end = from + error.valid_up_to();
                    let valid = String::from_utf8_lossy(&bytes[from..valid_end]);
                    self.take_text(&valid);
                    from = valid_end;
                    if let Some(invalid) = error.error_len() {
                        self.take_text("\u{FFFD}");
                        from += invalid;
                    } else {
                        self.pending.extend_from_slice(&bytes[from..]);
                        debug_assert!(self.pending.len() <= 3);
                        return;
                    }
                }
            }
        }
    }

    fn take_text(&mut self, text: &str) {
        // Kolejność jest wiążąca: dopasowanie dostaje tekst ZANIM ogon ma prawo go odrzucić.
        self.proof.take(text);
        self.tail.push_str(text);

        let limit = if self.dropped {
            KEEP_LAST - OMITTED.len()
        } else {
            KEEP_LAST
        };
        if self.tail.len() <= limit {
            return;
        }

        self.dropped = true;
        let keep = KEEP_LAST - OMITTED.len();
        let mut cut = self.tail.len() - keep;
        // 2026-08-31 — sufit jest bajtowy, ale cięcie w środku znaku zrobiłoby z poprawnego
        // wyjścia `�`. Przesunięcie o najwyżej trzy bajty zachowuje poprawny UTF-8 i sufit.
        while !self.tail.is_char_boundary(cut) {
            cut += 1;
        }
        self.tail.drain(..cut);
    }

    fn finish(mut self) -> Captured {
        if !self.pending.is_empty() {
            let pending = std::mem::take(&mut self.pending);
            let decoded = String::from_utf8_lossy(&pending);
            self.take_text(decoded.as_ref());
        }
        if self.dropped {
            self.tail.insert_str(0, OMITTED);
        }
        debug_assert!(self.tail.len() <= KEEP_LAST);
        Captured {
            text: self.tail,
            matched: self.proof.matched,
            required: self.required.map(required::Scan::finish),
            assessment_bytes: self.assessment_bytes,
            read_failed: self.read_failed,
        }
    }
}

/// Oba potoki **do EOF**, złączone w jeden tekst w kolejności odczytu.
///
/// # Dlaczego jeden `select!`, a nie dwa zadania
///
/// Kolejność w buforze jest wtedy kolejnością, w jakiej komenda naprawdę pisała — a to jest
/// jedyna kolejność, po której człowiek pozna swój własny bieg: ostrzeżenie `npm` stoi PRZED
/// licznikiem, dokładnie tam, gdzie je wypisano. Dwa zadania zbierające do dwóch buforów dają
/// tekst, w którym wszystkie skargi lądują na końcu, choć dotyczą początku.
///
/// # Dlaczego to musi dojść do EOF
///
/// Bufor potoku ma ~64 KB. Potok, którego nikt nie opróżnia, zatrzymuje dziecko na `write` —
/// więc „czytamy później, najpierw poczekajmy na wyjście" jest zakleszczeniem, w którym krok wisi
/// na 100% „running", a wyjścia, czyli jedynej rzeczy, z której powstaje werdykt, i tak nie ma.
///
/// 2026-08-31 — tekst zachowuje tylko ogon, ale dopasowanie widzi cały zdekodowany strumień.
/// Dzięki temu sufit nie może skłamać o liczniku przejść z pierwszej linii; marker mówi
/// człowiekowi, że początek został świadomie pominięty.
async fn read_to_eof(
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
    mut capture: CheckCapture,
) -> Captured {
    capture.read_failed = !read_both(stdout, stderr, |stdout, chunk| {
        capture.take_stream(stdout, chunk);
    })
    .await;
    capture.finish()
}

/// Oba potoki do EOF, kawałek po kawałku, do `into`.
///
/// # Dlaczego to jest osobna funkcja od [`read_to_eof`]
///
/// Bo dwie rzeczy czytają te potoki i różnią się dokładnie jednym: KIEDY wołający dostaje bajty.
/// Krok „sprawdź" dostaje je raz, na końcu, bo dopiero wtedy jest z czego orzekać. Rzecz, która
/// ZOSTAJE ([`Staying`]), nie ma końca — więc tekst oddany na końcu jest tekstem oddanym w chwili,
/// w której przestał być komukolwiek potrzebny. Druga kopia tej pętli obok byłaby dwoma miejscami,
/// w których mieszka odpowiedź na „co znaczy do EOF" (niezmiennik 13), a jedna z nich zawsze
/// gubi gałąź: pierwsza wersja tego pliku miała ich siedem i każda była o jeden `select!`.
///
/// Powód, dla którego to MUSI dojść do EOF, i powód, dla którego to jest jeden `select!`, a nie
/// dwa zadania, stoją w całości przy [`read_to_eof`].
async fn read_both<Into: FnMut(bool, &[u8])>(
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
    mut into: Into,
) -> bool {
    let mut complete = true;
    let mut out = stdout;
    let mut complaints = stderr;
    let mut from_out = vec![0_u8; CHUNK];
    let mut from_complaints = vec![0_u8; CHUNK];

    loop {
        let heard = match (&mut out, &mut complaints) {
            (Some(one), Some(other)) => tokio::select! {
                // Odczyt porzucony w połowie nie gubi bajtów: `AsyncReadExt::read` jest
                // bezpieczny w `select!` — kiedy wygra druga gałąź, ten potok po prostu nie
                // został przeczytany.
                got = one.read(&mut from_out) => Said::Out(got),
                got = other.read(&mut from_complaints) => Said::Complaints(got),
            },
            (Some(one), None) => Said::Out(one.read(&mut from_out).await),
            (None, Some(other)) => Said::Complaints(other.read(&mut from_complaints).await),
            // Oba potoki na EOF: to jedyne wyjście z tej pętli, więc „do EOF" znaczy tu dokładnie
            // to, co mówi.
            (None, None) => return complete,
        };

        match heard {
            // Zero bajtów to EOF, a błąd odczytu znaczy dla nas to samo: z tego potoku nie
            // przyjdzie już nic. Zamknięty potok trzymany w pętli byłby czekaniem bez końca.
            Said::Out(Ok(0)) => out = None,
            Said::Complaints(Ok(0)) => complaints = None,
            Said::Out(Err(_)) => {
                out = None;
                complete = false;
            }
            Said::Complaints(Err(_)) => {
                complaints = None;
                complete = false;
            }
            Said::Out(Ok(how_many)) => into(true, &from_out[..how_many]),
            Said::Complaints(Ok(how_many)) => into(false, &from_complaints[..how_many]),
        }
    }
}

/// Rodzaj sterownika, który nie zna ani jednego vendora.
///
/// Stoi obok `claude.rs` i `absent.rs`, a nie w planiście, i to jest rozstrzygnięcie
/// architektoniczne: krok „sprawdź" nazywa **rodzaj sterownika**, nie etap biegu. Planista
/// dostaje z niego wynik i nie wie, że ten krok „jest bramką" — kolejność mieszka wyłącznie
/// w grafie (niezmiennik 27).
#[derive(Debug, Clone, Default)]
pub struct CommandDriver {
    /// Znacznik biegu i kroku dla procesów, które ten sterownik uruchomi (2026-09, Z-01d).
    ///
    /// POLE, a nie argument każdej z trzech dróg do systemu (`start`, `run`, `start_to_stay`),
    /// i to jest cała odpowiedź na to, jak krok `serve` zgubił znacznik w poprzednim podejściu:
    /// tam był argumentem jednej drogi, a `serve` szedł drugą i nie dostawał go wcale. Na polu
    /// niesie go każda.
    tag: Option<supervisor::StepTag>,
    filesystem_fence: Option<supervisor::FilesystemFence>,
    proof_mode: ProofMode,
    executable: Option<(std::path::PathBuf, Vec<std::ffi::OsString>)>,
    temporary_directory: Option<std::path::PathBuf>,
}

impl CommandDriver {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            tag: None,
            filesystem_fence: None,
            proof_mode: ProofMode::OutputPattern,
            executable: None,
            temporary_directory: None,
        }
    }

    /// Ten sam sterownik, tylko wiedzący, czyj bieg i czyj krok uruchamia.
    ///
    /// Bierze `self` przez wartość i oddaje nową wartość, bo `CommandDriver` jest tani i tworzony
    /// na wywołanie — tu nie ma czego klonować zza `Arc`, w odróżnieniu od sterowników vendorów.
    #[must_use]
    pub fn for_step(mut self, tag: supervisor::StepTag) -> Self {
        self.tag = Some(tag);
        self
    }

    #[must_use]
    pub fn with_filesystem_fence(mut self, fence: &supervisor::FilesystemFence) -> Self {
        self.filesystem_fence = Some(fence.clone());
        self
    }

    #[must_use]
    pub fn with_proof_mode(mut self, mode: ProofMode) -> Self {
        self.proof_mode = mode;
        self
    }

    /// Zaufany, zamrożony program nie przechodzi przez shell ani PATH subjectu.
    #[must_use]
    pub fn with_executable(
        mut self,
        program: std::path::PathBuf,
        arguments: Vec<std::ffi::OsString>,
    ) -> Self {
        self.executable = Some((program, arguments));
        self
    }

    #[must_use]
    pub fn with_temporary_directory(mut self, directory: std::path::PathBuf) -> Self {
        self.temporary_directory = Some(directory);
        self
    }

    /// Startuje komendę we **własnej grupie procesów**, przez [`supervisor::spawn`].
    ///
    /// Zwrócony uchwyt zna swój `pgid` natychmiast — to jest ta kolejność („wygeneruj, zapisz,
    /// dopiero potem czytaj cokolwiek z wyjścia"), która w ogóle czyni odzyskiwanie możliwym
    /// [T7 §6.2].
    pub fn start(&self, spec: &CheckSpec) -> io::Result<Checking> {
        self.start_with_input(spec, None)
    }

    /// 2026-09-05: hostowe dane Check idą stdin, nigdy przez interpolację do command.
    pub fn start_with_input(
        &self,
        spec: &CheckSpec,
        input: Option<String>,
    ) -> io::Result<Checking> {
        if self.proof_mode == ProofMode::Unknown {
            return Err(io::Error::other(
                "This kind of check evidence is not supported.",
            ));
        }
        /* V-01 (incydent I-03, 2026-09-06): KATALOG SPRAWDZAMY PRZED STARTEM, NIE PO NIM.
         *
         * Krok, którego `cd src-tauri` nie powiodło się, uruchamiał komendę tam, gdzie akurat
         * stał — a `cargo test` z innego katalogu to inna suita, nie brak suity. Sam system
         * odmawia tu `No such file or directory (os error 2)`: prawda, która nie mówi CZEGO
         * nie ma, więc człowiek czyta ją przy kroku i nie wie, czy zabrakło katalogu, komendy,
         * czy pliku, którego ta komenda szukała. */
        if !spec.cwd.is_dir() {
            return Err(io::Error::other(format!(
                "This check could not start: its folder {} does not exist.",
                spec.cwd.display()
            )));
        }
        let mut command = if let Some((program, arguments)) = &self.executable {
            let mut command = tokio::process::Command::new(program);
            command.args(arguments);
            command
        } else {
            let mut command = tokio::process::Command::new(SHELL);
            command.arg("-c").arg(&spec.command);
            command
        };
        command.current_dir(&spec.cwd);
        // `StdinPlan::Null` daje dziecku EOF natychmiast. Krok „sprawdź" nie ma promptu i nie ma
        // nic do powiedzenia komendzie — a odziedziczony stdin kosztuje sekundy czekania na
        // każdym kroku każdego biegu [T1 §4.6].
        let stdin = input.map_or(StdinPlan::Null, StdinPlan::Write);
        let environment = self
            .temporary_directory
            .as_ref()
            .map(|path| ("TMPDIR".to_owned(), path.clone().into_os_string()))
            .into_iter()
            .collect::<Vec<_>>();
        let handle = supervisor::spawn_tagged_with_fence(
            command,
            stdin,
            &environment,
            self.tag.as_ref(),
            self.filesystem_fence.as_ref(),
        )?;
        let group = handle.group();
        Ok(Checking {
            group,
            required: spec.required_tests.clone(),
            handle,
            proof: spec.proof.clone(),
            proof_mode: self.proof_mode,
            began: Instant::now(),
        })
    }

    /// Cały krok: start, czekanie, werdykt. To jest droga, którą wchodzi planista.
    pub async fn run(&self, spec: &CheckSpec, cancel: &CancellationToken) -> io::Result<CheckEnd> {
        let mut live = self.start(spec)?;
        Ok(live.settle(cancel).await)
    }

    /// Startuje komendę, która ma **zostać** — i oddaje uchwyt, nie wynik.
    ///
    /// Ta sama droga do systemu, co [`CommandDriver::start`]: [`supervisor::spawn`], własna grupa,
    /// `env_clear()` plus jawna lista, potoki. Różnica jest jedna i cała mieszka w tym, czego tu
    /// NIE ma: nie ma [`CheckSpec::proof`], bo nie ma werdyktu, i nie ma [`GIVE_UP_AFTER`], bo
    /// proces zamówiony przez człowieka żyje do własnego końca, żądania albo zamknięcia okna.
    ///
    /// Uchwyt, a nie `async fn` czekająca do końca, i to jest cała różnica wobec kroku „sprawdź".
    /// Wersja czekająca kompiluje się, czyta dobrze i zamienia tę drogę w krok sprawdzający
    /// z inną nazwą: wołający dowiaduje się o `pgid` dopiero wtedy, gdy proces już zszedł, więc
    /// przez cały czas jego życia nie ma go czym pokazać ani czym ubić.
    pub fn start_to_stay(&self, spec: &StartSpec) -> io::Result<Staying> {
        self.start_to_stay_with_environment(spec, &[])
    }

    /// Niesekretne ustawienia portów/uruchomienia przechodzą tę samą listę supervisora.
    pub fn start_to_stay_with_environment(
        &self,
        spec: &StartSpec,
        environment: &[(String, std::ffi::OsString)],
    ) -> io::Result<Staying> {
        let mut command = tokio::process::Command::new(SHELL);
        command.arg("-c").arg(&spec.command);
        command.current_dir(&spec.cwd);
        // `StdinPlan::Null` z tego samego powodu, co w [`CommandDriver::start`]: rzecz zamówiona
        // komendą nie ma promptu, a odziedziczony stdin kosztuje sekundy czekania [T1 §4.6].
        // Dzień, w którym `/start` ma przyjmować pisanie, jest dniem, w którym wchodzi tu
        // `StdinPlan::Keep` — nie ma go, bo nie ma kontrolki, która by to wysyłała
        // (niezmiennik 16).
        //
        // TĄ SAMĄ DROGĄ CO KROK „SPRAWDŹ", ze znacznikiem z pola (2026-09, Z-01d). To jest ta
        // droga, na której poprzednie podejście znacznik zgubiło: `serve` szedł przez `Processes::
        // start` → `start_to_stay`, czyli obok jedynej funkcji, która znacznik ustawiała.
        let mut handle = supervisor::spawn_tagged_with_fence(
            command,
            StdinPlan::Null,
            environment,
            self.tag.as_ref(),
            self.filesystem_fence.as_ref(),
        )?;
        let group = handle.group();

        let output = StayingOutput {
            said: Arc::new(Mutex::new(Vec::new())),
        };
        let (ended, natural_end) = oneshot::channel();

        // Potoki wyjmujemy PRZED oddaniem uchwytu do struktury, dokładnie jak w `Checking::settle`
        // i z tego samego powodu: czytanie pożycza je na cały swój czas, a `Supervised::stop`
        // pożycza uchwyt mutowalnie. Wyjęte, jadą do zadania czytającego na własność.
        let out = handle.stdout();
        let complaints = handle.stderr();

        /* POTOKI OPRÓŻNIA ZADANIE W TLE, I TO NIE JEST WYGODA — jest to jedyny kształt, w którym
         * ta rzecz może biec dłużej niż wywołanie, które ją zamówiło. Bufor potoku ma ~64 KB, więc
         * nieopróżniany zatrzymuje dziecko na `write`: dev server pisze pierwsze kilkadziesiąt
         * kilobajtów w ciągu sekund i zawiesza się na zawsze, a z okna wygląda to jak apka, która
         * wstała i zamilkła. Krok „sprawdź" opróżnia je w `settle()`, bo tam ktoś na nie czeka;
         * tutaj nie czeka nikt. */
        let keep = Arc::clone(&output.said);
        let _reading = tokio::spawn(async move {
            read_both(out, complaints, |_, chunk| remember(&keep, chunk)).await;
            /* EOF NA OBU POTOKACH URUCHAMIA DOWÓD, ALE NIM NIE JEST. Sierota dziedzicząca stdout
             * nie pozwala potokowi dojść do EOF (`lsof` pokazał obie na fd 1 i fd 2 [T7 §3.1]),
             * lecz proces może też świadomie zamknąć deskryptory. Dlatego ten dzwonek nie usuwa
             * wpisu ani nie gasi flagi: wspólny właściciel dopiero zbiera lidera i żąda od
             * supervisora `GroupProof::Dead` (niezmiennik 6, 2026-08-31).
             *
             * Dlaczego nie `wait()` na liderze: lider bywa najszybszy, a płacimy za wnuki —
             * `npm run dev` rozwidla dziecko i sam wychodzi, więc status lidera powiedziałby
             * „zeszło" nad rzeczą, która pracuje dalej. To jest ta sama różnica, dla której
             * niezmiennik 6 mówi o GRUPIE, nie o procesie. */
            // 2026-08-31 — odbiorca jest tylko dzwonkiem. Nie niesie uchwytu, ogona ani Arc do
            // rejestru, więc samo zadanie EOF nie może przedłużyć życia właściciela procesu.
            let _ = ended.send(());
        });

        Ok(Staying {
            group,
            command: spec.command.clone(),
            handle,
            alive: true,
            output,
            natural_end: Some(natural_end),
        })
    }
}

/* ── KOMENDA, KTÓRA MA ZOSTAĆ ───────────────────────────────────────────────────────────────
 *
 * DLACZEGO TO NIE JEST KROK „SPRAWDŹ" Z INNYM SUFITEM. Krok sprawdzający ma koniec, o którym
 * decyduje on sam: komenda wraca, my orzekamy. Rzecz zamówiona przez człowieka (`/start npm run
 * dev`) może zejść sama, ale start nie czeka na ten koniec: rejestr zbiera ją po EOF albo kończy
 * ją na żądanie człowieka czy przy zniknięciu okna.
 * Trzy rzeczy z [`CheckSpec`] tracą tu więc sens naraz: wzorzec dowodu (nie ma werdyktu),
 * [`GIVE_UP_AFTER`] (nie ma limitu) i sama forma „jedno wywołanie robi wszystko" (bo przez cały
 * czas życia tej rzeczy ktoś musi mieć czym ją pokazać i czym ją ubić).
 *
 * CZEGO TU CELOWO NIE MA: ani jednego warunku platformowego, ani jednej stałej sygnału, ani
 * jednego `killpg`. Zabijanie i eskalacja należą do `supervisor.rs` (niezmiennik 3) i pilnuje
 * tego `checks/quick-boundary.sh`. Ten plik prosi o zatrzymanie neutralnym czasownikiem i czyta
 * zwrócony dowód — dokładnie jak [`Checking`] o jeden ekran wyżej.
 */

/// Co uruchomić i gdzie — komenda zamówiona z wiersza wejścia.
///
/// Bez wzorca dowodu, i to jest różnica merytoryczna wobec [`CheckSpec`], nie oszczędność pola:
/// werdyktu tu nie ma, bo nie ma czego orzekać. Rzecz, która biegnie, biegnie; rzecz, która
/// zeszła, zeszła — a „przeszło / nie przeszło" jest pytaniem o krok sprawdzający.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartSpec {
    /// Wiersz powłoki, dosłownie jak wpisał go człowiek.
    ///
    /// Co do znaku, bo to ON jest nazwą tej rzeczy na ekranie: wymyślona etykieta („Dev server")
    /// byłaby relacją, której w danych nie ma (niezmiennik 17), a człowiek szuka na liście tego,
    /// co sam wpisał.
    pub command: String,
    /// Katalog, w którym ta komenda ma stanąć.
    pub cwd: PathBuf,
}

/// Klonowalny widok ograniczonego ogona, bez uchwytu do procesu.
///
/// Rejestr trzyma go obok asynchronicznego właściciela [`Staying`], żeby zwykłe odświeżenie
/// okna nie musiało brać zamka trzymanego podczas dowodzenia śmierci. Klon zachowuje wyłącznie
/// bajty; nie potrafi czekać, sygnalizować ani przedłużyć życia [`Supervised`].
#[derive(Debug, Clone)]
pub struct StayingOutput {
    said: Arc<Mutex<Vec<u8>>>,
}

impl StayingOutput {
    /// Co ta rzecz do tej pory wypisała — ogon długości [`KEEP_LAST`].
    #[must_use]
    pub fn said(&self) -> String {
        let kept = self.said.lock().unwrap_or_else(PoisonError::into_inner);
        String::from_utf8_lossy(&kept).into_owned()
    }
}

/// Żywa komenda, która ma zostać: własna grupa, potoki opróżniane do EOF, zejście z dowodem.
///
/// Uchwyt, a nie jedno wywołanie „zrób wszystko", i to jest ten sam wymóg z niezmiennika 6, co
/// przy [`Checking`]: `pgid` musi dać się przeczytać ZANIM ktokolwiek przeczyta pierwszy bajt
/// wyjścia. Tutaj waży to jeszcze więcej niż tam — kafelek na ekranie istnieje przez cały czas
/// życia tej rzeczy, więc bez uchwytu nie ma czego pokazać ani czego ubić.
#[derive(Debug)]
pub struct Staying {
    /// Zwykła wartość, wzięta ze [`supervisor::spawn`] synchronicznie [T7 §6.2].
    group: GroupId,
    /// Wiersz powłoki, co do znaku — patrz [`StartSpec::command`].
    command: String,
    /// Nadzorowana grupa procesów. Porzucenie tego pola też ją zabija — gwardia siedzi
    /// w `Drop` uchwytu, a normalną drogą jest [`Staying::stop`].
    handle: Supervised,
    /// Zwykły stan jedynego właściciela. `false` wolno zapisać dopiero po systemowym dowodzie
    /// śmierci całej grupy; EOF obu potoków nie zmienia go (niezmiennik 6, 2026-08-31).
    alive: bool,
    /// Ogon tego, co ta rzecz wypisała — oba potoki, w kolejności odczytu.
    ///
    /// **Bajty, nie tekst**, i to jest wymóg, nie gust: porcja bywa rozcięta w środku znaku
    /// wielobajtowego, więc `from_utf8_lossy` na każdej z nich osobno zamieniałby taki znak
    /// w znak zapytania. Tekst powstaje raz, w [`StayingOutput::said`].
    ///
    /// `std::sync::Mutex` i **nigdy trzymany przez `await`** (niezmiennik 8): oba wzięcia —
    /// dopisanie porcji w [`remember`] i klon w [`StayingOutput::said`] — mieszczą się w jednym
    /// wyrażeniu, w którym nie ma czego czekać.
    output: StayingOutput,
    /// Jednorazowy dzwonek po EOF obu potoków. Odbiera go rejestr dokładnie raz; sam czytelnik
    /// nie zna rejestru ani właściciela uchwytu (2026-08-31).
    natural_end: Option<oneshot::Receiver<()>>,
}

impl Staying {
    /// `pid` i `pgid`, dostępne od razu po starcie i bez czekania na cokolwiek z wyjścia.
    #[must_use]
    pub const fn group(&self) -> GroupId {
        self.group
    }

    /// Wiersz powłoki, co do znaku. To on jest nazwą tej rzeczy na ekranie.
    #[must_use]
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Czy tę grupę nadal trzeba traktować jako żywą.
    #[must_use]
    pub fn alive(&self) -> bool {
        self.alive
    }

    /// Co ta rzecz do tej pory wypisała — ogon długości [`KEEP_LAST`].
    #[must_use]
    pub fn said(&self) -> String {
        self.output.said()
    }

    /// Widok ogona bez prawa do procesu. Rejestr czyta go synchronicznie podczas odświeżenia.
    #[must_use]
    pub fn output(&self) -> StayingOutput {
        self.output.clone()
    }

    /// Odbiera jedyny dzwonek EOF. `None` przy drugiej próbie jest błędem programisty wołającego,
    /// nie stanem procesu; [`Processes`](crate::commands::processes::Processes) bierze go podczas
    /// wstawiania tego samego uchwytu do rejestru.
    pub fn natural_end(&mut self) -> Option<oneshot::Receiver<()>> {
        self.natural_end.take()
    }

    /// Prosi grupę o zejście i oddaje **dowód**, nie potwierdzenie wysłania sygnału.
    ///
    /// Wraca dopiero z `ESRCH` dla całej grupy (niezmiennik 6). `Ok(())` po sygnale czytałoby się
    /// u wołającego jako „nie żyje", a wnuki biegłyby dalej i dalej płaciły [T7 §3.1] — przy
    /// rzeczy, którą człowiek uruchomił świadomie, to jest ta sama klasa wady co „Running" nad
    /// komendą, która zeszła dwie minuty temu, tylko w drugą stronę.
    pub async fn stop(&mut self) -> GroupProof {
        let proof = self.handle.stop(supervisor::DEFAULT_GRACE).await;
        // 2026-08-31 — sygnał, EOF i wynik lidera nie są dowodem śmierci całej grupy; `Alive`
        // zachowuje więc dotychczasowy stan razem z uchwytem do ponownej eskalacji.
        if matches!(&proof, GroupProof::Dead { .. }) {
            self.alive = false;
        }
        proof
    }
}

/// Dopisuje porcję do ogona i przycina go do [`KEEP_LAST`].
///
/// Przycinamy PRZÓD, bo terminal pokazuje koniec: człowiek, który wchodzi w kafelek dev servera
/// po godzinie, chce zobaczyć ostatni błąd, a nie pierwszy wiersz startu. Cena jest zapisana
/// i jest świadoma — początek wyjścia przepada, więc to nie jest miejsce, z którego wolno
/// orzekać, czy cokolwiek ruszyło (niezmiennik 19); werdykty wystawia krok „sprawdź", który
/// wyjścia nie tnie wcale.
fn remember(said: &Mutex<Vec<u8>>, chunk: &[u8]) {
    let mut kept = said.lock().unwrap_or_else(PoisonError::into_inner);
    kept.extend_from_slice(chunk);
    if let Some(over) = kept.len().checked_sub(KEEP_LAST).filter(|over| *over > 0) {
        kept.drain(..over);
    }
}

/// Czy wzorzec dowodu trafia w wyjście komendy.
///
/// Wzorzec to zwykły tekst z **jednym rodzajem** metaznaku: każda sekwencja `(\d+)` znaczy „co
/// najmniej jedna cyfra", wszystko poza nimi jest literałem, a dopasowanie jest szukaniem
/// podciągu. To celowo **ta sama notacja**, którą człowiek pisze w linii `expect:` naszego
/// własnego harnessu (`AGENTS.md` §2a punkt 4) — jedna notacja, jedno znaczenie, w bramce
/// i w aplikacji.
///
/// Własny, mały automat, a nie skrzynia `regex`: `Cargo.toml` leży poza blokiem OWNS tego
/// zadania, więc dopisanie zależności jest pytaniem do człowieka (`AGENTS.md` §7), nie cichym
/// dopiskiem. Ten sam [`ProofScan`] dostaje gotowy tekst tutaj i porcje procesu w [`CheckCapture`].
///
/// Wzorzec pusty oddaje `false`, i to jest decyzja, nie skutek uboczny pętli. Puste szukanie
/// jest podciągiem każdego tekstu, więc „dopasowało się" znaczyłoby wtedy „nie sprawdzono nic" —
/// czyli werdykt spadłby z powrotem na sam kod wyjścia, przed czym stoi niezmiennik 19. Kroku bez
/// dowodu i tak nie da się zapisać (`workflow::check::a_check_without_a_proof`); to jest druga
/// zapora, na wypadek wywołania z innej strony.
#[must_use]
pub fn proof_matches(proof: &str, output: &str) -> bool {
    let mut scan = ProofScan::new(proof);
    scan.take(output);
    scan.matched
}

/// Jedyna koniunkcja werdyktu — także dla wyniku policzonego podczas drenowania potoków.
fn verdict(exit_code: Option<i32>, matched: bool) -> bool {
    exit_code == Some(0) && matched
}

/// Werdykt kroku „sprawdź": kod wyjścia **oraz** dopasowanie wzorca, nigdy jedno z dwóch.
///
/// To jest cała treść niezmiennika 19 i jedyny powód, dla którego pole `proof` w ogóle istnieje.
/// Dwa przypadki spoza przekątnej rozstrzygają, czy ta funkcja jest napisana:
///
/// * `rc == 0` i wyjście `error: no test target matched` — suita, która nie uruchomiła **ani
///   jednego** testu, wychodzi zerem. Werdykt: nie przeszło.
/// * `rc == 1` i wyjście `test result: FAILED. 11 passed; 1 failed` — licznik przejść jest
///   w wyjściu, a komenda padła. Werdykt: nie przeszło.
///
/// `None` w kodzie wyjścia nigdy nie jest przejściem: proces zginął od sygnału, więc kodu po
/// prostu nie ma, a `None` to nie zero.
///
/// KONIUNKCJA, i to jest cała funkcja. Każda z dwóch połówek osobno przechodzi trzy z czterech
/// przebiegów z AC-2 i myli się na czwartym — dlatego tabela w kryterium ma cztery wiersze, nie
/// dwa, i dlatego tu nie ma miejsca na `||`.
#[must_use]
pub fn passed(exit_code: Option<i32>, output: &str, proof: &str) -> bool {
    // `Some(0)`, nie `is_none_or`: `None` znaczy „proces zginął od sygnału, więc kodu po prostu
    // nie ma", a brak odpowiedzi nie jest odpowiedzią „udało się". Każde zatrzymane sprawdzenie
    // czytałoby się inaczej jako przeszłe.
    verdict(exit_code, proof_matches(proof, output))
}

#[cfg(test)]
mod tests {
    //! Gałęzie dopasowania, których nie dotyka ani jedno kryterium akceptacji — i dlatego są tutaj.
    //!
    //! AC-2 mierzy notację od strony człowieka: cztery przebiegi werdyktu i cztery wzorce z linii
    //! `expect:`. Wszystkie cztery przechodzą także dla wersji **zachłannej bez nawrotu**, bo żaden
    //! wzorzec z bramki nie stawia cyfry zaraz po grupie cyfr. Uproszczenie `digits_then` do jednej
    //! próby na najdłuższym ciągu byłoby więc zmianą, po której pełna bramka jest zielona, a wzorzec
    //! `(\d+)5` przestaje działać — czyli dokładnie tym rodzajem cichej regresji, przed którą stoją
    //! kryteria. Kryterium tego nie złapie, bo `check:` wskazuje pliki, których ta gałąź nie
    //! interesuje; więc łapie to test przy kodzie.
    //!
    //! Wzorzec jest w tym repo (`workflow/check.rs`, `workflow/unroll.rs`, `commands/run.rs`):
    //! `Result`, `assert!` i ani jednego `unwrap` — pełne clippy biegnie `--all-targets -- -D
    //! warnings`, a `unwrap_used` jest w tej skrzyni odmową.

    use super::{DIGIT_RUN, passed, proof_matches};

    #[test]
    fn a_digit_right_after_the_group_needs_a_step_back() {
        // Zachłannie: grupa bierze `125`, po niej nie ma `5`, odpowiedź „nie". Z nawrotem: grupa
        // bierze `12`, literał `5` stoi tam, gdzie ma stać.
        assert!(
            proof_matches(r"(\d+)5", "125"),
            "the group has to give a digit back so the literal after it can match; a greedy pass \
             with no step back answers 'no' to text that fits"
        );
        assert!(
            !proof_matches(r"(\d+)5", "12 5"),
            "and it may only give back DIGITS: a space is not one, so this must stay a miss"
        );
    }

    #[test]
    fn the_group_may_close_the_pattern_and_may_open_it() {
        assert!(
            proof_matches(r"passed (\d+)", "passed 12"),
            "a pattern that ends with the group matches when digits are the last thing there is"
        );
        assert!(
            !proof_matches(r"passed (\d+)", "passed none"),
            "and misses when they are not — one metacharacter, no second meaning"
        );
        assert!(
            proof_matches(DIGIT_RUN, "ran 7 of them"),
            "a pattern that is nothing BUT the group asks one question: is there a digit anywhere"
        );
        assert!(
            proof_matches(r"(\d+) of (\d+) passed", "result: 12 of 34 passed"),
            "each occurrence is a group, so two counters in one proof keep working"
        );
        assert!(
            !proof_matches(r"(\d+) of (\d+) passed", "result: 12 of none passed"),
            "and every group still needs at least one digit"
        );
    }

    #[test]
    fn an_empty_proof_is_never_a_match_and_never_a_pass() {
        // Puste szukanie jest podciągiem każdego tekstu, więc bez tej zapory werdykt spadłby na
        // sam kod wyjścia — a suita, która nie uruchomiła ani jednego testu, wychodzi zerem
        // (niezmiennik 19).
        assert!(
            !proof_matches("", "test result: ok. 12 passed; 0 failed"),
            "an empty pattern is a substring of everything, and 'matched everything' has to read \
             as 'checked nothing'"
        );
        assert!(
            !passed(Some(0), "test result: ok. 12 passed; 0 failed", "   "),
            "a check step with a blank proof cannot be saved, and if one arrives from anywhere \
             else it still may not pass on the exit code alone"
        );
    }

    #[test]
    fn output_that_is_not_ascii_is_scanned_without_falling_over() {
        // Wyjście przychodzi z potoku cudzej komendy, więc bywa w nim wszystko. Indeks w środku
        // znaku wielobajtowego jest paniką, a panika w silniku zabiera cały bieg (AGENTS.md §4).
        assert!(
            proof_matches(r"(\d+) passed", "✅ zrobione — 3 passed; 0 failed"),
            "the scan steps over characters, not bytes, and still finds the counter"
        );
        assert!(
            !proof_matches(r"(\d+) passed", "✅✅✅ nic nie ruszyło"),
            "and answers 'no' on the same kind of text instead of falling over on it"
        );
    }
}
