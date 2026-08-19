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
use std::time::{Duration, Instant};

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

    /// Czeka na koniec komendy, na Stop albo na limit czasu — i oddaje jedno z trzech.
    ///
    /// SZKIELET (T-55, 2026-08-19): czytania obu potoków do EOF, eskalacji zabijania i werdyktu
    /// tu jeszcze NIE MA. Ta funkcja oddaje dziś [`CheckHow::Ran`] z pustym wyjściem i bez kodu
    /// wyjścia, czyli świadomie złą wartość — `todo!()` jest w tej skrzyni `deny`, a wartość
    /// zwrócona z premedytacją źle daje się odróżnić od poprawnej **tylko** kryterium, które
    /// naprawdę mierzy zachowanie. Dowodzą tego AC-3 i AC-4 w warstwie `before`.
    pub async fn settle(&mut self, _cancel: &CancellationToken) -> CheckEnd {
        // Kształt tej funkcji jest asynchroniczny, bo opróżnianie dwóch potoków do EOF nim jest.
        // Samego opróżniania jeszcze nie ma, więc tu stoi jedno oddanie sterowania — po to, żeby
        // sygnatura była już ta docelowa i żeby wołający nie musiał się zmieniać drugi raz.
        tokio::task::yield_now().await;
        let output = String::new();
        CheckEnd {
            group: self.group,
            how: CheckHow::Ran(CheckReport {
                passed: passed(None, &output, &self.proof),
                exit_code: None,
                matched: proof_matches(&self.proof, &output),
                output,
                took: self.began.elapsed(),
            }),
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
/// SZKIELET (T-55, 2026-08-19): oddaje `false` zawsze. AC-2 mierzy trzy rzeczy, których to nie
/// umie — zero cyfr jest za mało, jedna wystarcza, wzorzec bez metaznaku jest zwykłym podciągiem.
#[must_use]
pub fn proof_matches(proof: &str, output: &str) -> bool {
    let _ = (proof, output);
    false
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
/// SZKIELET (T-55, 2026-08-19): oddaje `false` zawsze, czyli nie odróżnia ani jednego z czterech
/// przebiegów AC-2 od pozostałych.
#[must_use]
pub fn passed(exit_code: Option<i32>, output: &str, proof: &str) -> bool {
    let _ = (exit_code, output, proof);
    false
}
