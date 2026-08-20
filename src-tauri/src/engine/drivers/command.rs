//! Krok „sprawdź": komendę odpala Loadout, werdykt wystawia Loadout, nigdy agent.
//!
//! Ten plik jest rodzajem STEROWNIKA, dokładnie tak jak `claude.rs` i `codex.rs` — i celowo
//! **nie implementuje** `AgentDriver`. Nie ma tu sesji, modelu, promptu ani tury: jest komenda,
//! jej wyjście i zdanie „przeszło / nie przeszło", wystawione przez nas. Rozróżnienie, którego
//! ten plik broni, jest tym jedynym, dla którego produkt powstał: **co agent powiedział** kontra
//! **co się stało** (`docs/research/projects/00-SYNTHESIS.md` §2.1). Krok agenta o instrukcji
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
use std::time::{Duration, Instant};

use tokio::io::AsyncReadExt;
use tokio::process::{ChildStderr, ChildStdout};
use tokio_util::sync::CancellationToken;

use super::super::supervisor::{self, GroupId, GroupProof, StdinPlan, Supervised};

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
}

/// Co z komendy wyszło.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
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
        let reading = read_to_eof(self.handle.stdout(), self.handle.stderr());
        tokio::pin!(reading);

        let output = {
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
                how: CheckHow::Ran(self.report(status.ok().and_then(|how| how.code()), output)),
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
    fn report(&self, exit_code: Option<i32>, output: String) -> CheckReport {
        CheckReport {
            passed: passed(exit_code, &output, &self.proof),
            exit_code,
            matched: proof_matches(&self.proof, &output),
            output,
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
/// Wyjścia NIE PRZYCINAMY i to jest wybór z ceną: komenda pisząca bez opamiętania zajmie tyle
/// pamięci, ile napisze, aż do [`GIVE_UP_AFTER`]. Przycięcie do ostatnich N kilobajtów byłoby
/// tańsze i kłamałoby o dowodzie — wzorzec bywa i na początku wyjścia (`error: no test target
/// matched`), więc obcięty tekst zamieniałby „nic nie ruszyło" w „nie wiadomo".
async fn read_to_eof(stdout: Option<ChildStdout>, stderr: Option<ChildStderr>) -> String {
    let mut said: Vec<u8> = Vec::new();
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
            // to, co mówi. Tekst składamy raz, na końcu — `from_utf8_lossy` na każdym kawałku
            // osobno zamieniałoby znak rozcięty na granicy porcji w znak zapytania.
            (None, None) => return String::from_utf8_lossy(&said).into_owned(),
        };

        match heard {
            // Zero bajtów to EOF, a błąd odczytu znaczy dla nas to samo: z tego potoku nie
            // przyjdzie już nic. Zamknięty potok trzymany w pętli byłby czekaniem bez końca.
            Said::Out(Ok(0) | Err(_)) => out = None,
            Said::Complaints(Ok(0) | Err(_)) => complaints = None,
            Said::Out(Ok(how_many)) => said.extend_from_slice(&from_out[..how_many]),
            Said::Complaints(Ok(how_many)) => {
                said.extend_from_slice(&from_complaints[..how_many]);
            }
        }
    }
}

/// Rodzaj sterownika, który nie zna ani jednego vendora.
///
/// Stoi obok `claude.rs` i `absent.rs`, a nie w planiście, i to jest rozstrzygnięcie
/// architektoniczne: krok „sprawdź" nazywa **rodzaj sterownika**, nie etap biegu. Planista
/// dostaje z niego wynik i nie wie, że ten krok „jest bramką" — kolejność mieszka wyłącznie
/// w grafie (niezmiennik 27).
#[derive(Debug, Clone, Copy, Default)]
pub struct CommandDriver;

impl CommandDriver {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Startuje komendę we **własnej grupie procesów**, przez [`supervisor::spawn`].
    ///
    /// Zwrócony uchwyt zna swój `pgid` natychmiast — to jest ta kolejność („wygeneruj, zapisz,
    /// dopiero potem czytaj cokolwiek z wyjścia"), która w ogóle czyni odzyskiwanie możliwym
    /// [T7 §6.2].
    pub fn start(&self, spec: &CheckSpec) -> io::Result<Checking> {
        let mut command = tokio::process::Command::new(SHELL);
        command.arg("-c").arg(&spec.command);
        command.current_dir(&spec.cwd);
        // `StdinPlan::Null` daje dziecku EOF natychmiast. Krok „sprawdź" nie ma promptu i nie ma
        // nic do powiedzenia komendzie — a odziedziczony stdin kosztuje sekundy czekania na
        // każdym kroku każdego biegu [T1 §4.6].
        let handle = supervisor::spawn(command, StdinPlan::Null)?;
        let group = handle.group();
        Ok(Checking {
            group,
            handle,
            proof: spec.proof.clone(),
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
    /// proces zamówiony przez człowieka kończy się na żądanie albo razem z oknem.
    ///
    /// Uchwyt, a nie `async fn` czekająca do końca, i to jest cała różnica wobec kroku „sprawdź".
    /// Wersja czekająca kompiluje się, czyta dobrze i zamienia tę drogę w krok sprawdzający
    /// z inną nazwą: wołający dowiaduje się o `pgid` dopiero wtedy, gdy proces już zszedł, więc
    /// przez cały czas jego życia nie ma go czym pokazać ani czym ubić.
    pub fn start_to_stay(&self, _spec: &StartSpec) -> io::Result<Staying> {
        todo!("T-72: własna grupa przez supervisor::spawn, potoki opróżniane do EOF, bez sufitu")
    }
}

/* ── KOMENDA, KTÓRA MA ZOSTAĆ ───────────────────────────────────────────────────────────────
 *
 * DLACZEGO TO NIE JEST KROK „SPRAWDŹ" Z INNYM SUFITEM. Krok sprawdzający ma koniec, o którym
 * decyduje on sam: komenda wraca, my orzekamy. Rzecz zamówiona przez człowieka (`/start npm run
 * dev`) nie ma takiego końca — kończy się, kiedy człowiek ją zatrzyma albo kiedy zniknie okno.
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

    /// Prosi grupę o zejście i oddaje **dowód**, nie potwierdzenie wysłania sygnału.
    ///
    /// Wraca dopiero z `ESRCH` dla całej grupy (niezmiennik 6). `Ok(())` po sygnale czytałoby się
    /// u wołającego jako „nie żyje", a wnuki biegłyby dalej i dalej płaciły [T7 §3.1] — przy
    /// rzeczy, którą człowiek uruchomił świadomie, to jest ta sama klasa wady co „Running" nad
    /// komendą, która zeszła dwie minuty temu, tylko w drugą stronę.
    pub async fn stop(&mut self) -> GroupProof {
        todo!("T-72: eskalacja przez Supervised::stop i dowód ESRCH dla całej grupy")
    }
}

/// Czy wzorzec dowodu trafia w wyjście komendy.
///
/// Wzorzec to zwykły tekst z **jednym** metaznakiem: sekwencja `(\d+)` znaczy „co najmniej jedna
/// cyfra", wszystko poza nią jest literałem, a dopasowanie jest szukaniem podciągu. To celowo
/// **ta sama notacja**, którą człowiek pisze w linii `expect:` naszego własnego harnessu
/// (`AGENTS.md` §2a punkt 4) — jedna notacja, jedno znaczenie, w bramce i w aplikacji.
///
/// Dwadzieścia wierszy własnego dopasowania, a nie skrzynia `regex`: `Cargo.toml` leży poza
/// blokiem OWNS tego zadania, więc dopisanie zależności jest pytaniem do człowieka
/// (`AGENTS.md` §7), nie cichym dopiskiem.
///
/// Wzorzec pusty oddaje `false`, i to jest decyzja, nie skutek uboczny pętli. Puste szukanie
/// jest podciągiem każdego tekstu, więc „dopasowało się" znaczyłoby wtedy „nie sprawdzono nic" —
/// czyli werdykt spadłby z powrotem na sam kod wyjścia, przed czym stoi niezmiennik 19. Kroku bez
/// dowodu i tak nie da się zapisać (`workflow::check::a_check_without_a_proof`); to jest druga
/// zapora, na wypadek wywołania z innej strony.
#[must_use]
pub fn proof_matches(proof: &str, output: &str) -> bool {
    if proof.trim().is_empty() {
        return false;
    }

    // Wzorzec rozcięty na literały. Jeden metaznak znaczy, że między dwoma literałami stoi
    // zawsze dokładnie jedna grupa cyfr — nie ma tu drzewa wyrażenia do zbudowania.
    let literals: Vec<&str> = proof.split(DIGIT_RUN).collect();
    let Some((first, rest)) = literals.split_first() else {
        return false;
    };
    // Wzorzec bez metaznaku jest zwykłym podciągiem i tak ma zostać: tak wygląda dziewięć
    // wzorców z dziesięciu, które napisze człowiek (`0 failed`).
    if rest.is_empty() {
        return output.contains(first);
    }

    // Każde miejsce, w którym stoi pierwszy literał — bo dopasowanie jest szukaniem PODCIĄGU,
    // a nie sprawdzeniem początku. Pierwszy literał bywa pusty (wzorzec otwiera się cyframi)
    // i wtedy ta pętla po prostu ogląda każdą pozycję po kolei.
    let mut from = 0;
    while let Some(at) = output.get(from..).and_then(|tail| tail.find(first)) {
        let after = from + at + first.len();
        if output
            .get(after..)
            .is_some_and(|tail| digits_then(rest, tail))
        {
            return true;
        }
        // O jeden ZNAK, nie o jeden bajt: `output` przychodzi z potoku komendy i bywa w nim
        // wszystko, a indeks w środku znaku wielobajtowego jest paniką w silniku (AGENTS.md §4).
        let step = output[from + at..].chars().next().map_or(1, char::len_utf8);
        from = from + at + step;
        if from > output.len() {
            break;
        }
    }
    false
}

/// Ogon wzorca: co najmniej jedna cyfra, potem kolejny literał — i tak do końca.
///
/// Nawroty są tu potrzebne i dlatego jest tu `any`, a nie jedna próba na najdłuższym ciągu cyfr:
/// wzorzec `(\d+)5` na wejściu `125` dopasowuje się wyłącznie wtedy, gdy grupie zostawimy dwie
/// cyfry z trzech. Wersja zachłanna bez nawrotu odpowiedziałaby „nie" na tekst, który pasuje.
fn digits_then(literals: &[&str], text: &str) -> bool {
    let Some((literal, rest)) = literals.split_first() else {
        return true;
    };
    // Cyfry są ASCII, więc liczba bajtów jest tu liczbą znaków i indeks nie może wpaść
    // w środek znaku.
    let how_many = text.bytes().take_while(u8::is_ascii_digit).count();
    // Od jedynki, bo `(\d+)` znaczy CO NAJMNIEJ JEDNA cyfra. Zero cyfr jest za mało i to jest
    // cała różnica między tym wzorcem a szukaniem samego napisu " passed".
    (1..=how_many).any(|digits| {
        text.get(digits..).is_some_and(|after| {
            after
                .strip_prefix(literal)
                .is_some_and(|then| digits_then(rest, then))
        })
    })
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
    exit_code == Some(0) && proof_matches(proof, output)
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
