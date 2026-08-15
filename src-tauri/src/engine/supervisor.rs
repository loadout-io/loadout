//! Nadzór procesów: własna grupa, eskalacja SIGTERM→SIGKILL i **dowód**, że grupa nie żyje.
//!
//! `claude` na tej maszynie nie jest programem, tylko skryptem powłoki, który odpala Node —
//! `Command::new("claude")` daje ci powłokę, a model biegnie we wnuku. `Child::kill()`
//! sygnalizuje wyłącznie bezpośrednie dziecko: zmierzone `A after kill: total=2 orphaned=2`,
//! czyli dwoje wnucząt przeniesionych pod PID 1, dalej mielących i dalej palących limit
//! [T7 §3.1, 2026-08-15]. To jest błąd finansowy, nie higieniczny, i jest całkowicie
//! niewidoczny: `wait()` wrócił, status brzmi „zabity", test jest zielony, a rachunek rośnie.
//!
//! Drugi efekt tego samego wycieku wiesza silnik: sieroty dziedziczą stdout, więc potok **nigdy
//! nie dochodzi do EOF** — `lsof` pokazał obie sieroty trzymające fd 1 i fd 2 na tym samym
//! potoku [T7 §3.1]. „Czytaj do EOF" przeciwko wyciekłej grupie to nie wyciek, tylko wieczne
//! oczekiwanie.
//!
//! Dlatego zatrzymanie zwraca **wartość dowodu** ([`GroupProof`]), nigdy `io::Result<()>`
//! (niezmiennik 6): `Ok(())` znaczyłoby „wysłałem sygnał", a wołający przeczytałby „nie żyje".
//!
//! **To jest jedyny plik w repo, w którym wolno stać kodowi platformowemu** (niezmiennik 3,
//! `docs/ARCHITECTURE.md` §3). Gałąź `#[cfg(windows)]` z `JobObject` wchodzi dokładnie w to
//! samo miejsce wywołania co `ProcessGroup::leader()` [T7 §9.2] — i zostaje `unimplemented!`
//! z powodem opisanym słowami, bo nie ma tu hosta Windows, na którym dałoby się ją zweryfikować
//! [T7 §11.3]. Na zewnątrz ten plik wystawia wyłącznie **funkcje neutralne** —
//! [`Supervised::stop`] i [`reap_group`] — a **nigdy stałych sygnałów**: `libc::SIGTERM`
//! zaimportowany „na chwilę" w pliku wywołującym łamie niezmiennik 3 po cichu, bo w diffie
//! wygląda jak zwykły `use`.
//!
//! # Adres tego modułu: `engine::supervisor` (2026-08-15)
//!
//! W fazie kontraktu ten sam plik był wciągany także z korzenia skrzyni
//! (`#[path = "engine/supervisor.rs"] pub mod supervisor;` w `lib.rs`), bo `engine/mod.rs` nie
//! miało jeszcze `pub mod supervisor;` — a to jest jeden wiersz poza blokiem OWNS tego zadania,
//! czyli pytanie do człowieka (`AGENTS.md` §7), nie cichy dopisek. Odpowiedź stoi w commicie
//! 687712a: linia jest w `engine/mod.rs`, więc deklaracja z korzenia znikła. Obie naraz budują
//! ten sam plik dwa razy, jako dwa różne moduły — to nie jest błąd kompilacji, tylko dwa
//! niezależne typy [`GroupProof`], których kompilator nie zamieni jeden w drugi.
//!
//! # Wszystkie sygnały idą przez bezpieczne opakowanie (2026-08-15)
//!
//! W tej skrzyni obowiązuje `unsafe_code = "deny"` (`Cargo.toml`, `[workspace.lints.rust]`),
//! a atrybut `allow(unsafe_code)` w `src-tauri/src/**` przewraca `checks/quick-suppressions.sh`.
//! Dlatego `killpg` woła tu opakowanie z `process-wrap` (`ProcessGroupChild`), a `libc` jest
//! użyty **wyłącznie po stałe** — `SIGTERM`, `SIGKILL`, `ESRCH` — dokładnie tak, jak zapowiada
//! komentarz przy tej zależności w `src-tauri/Cargo.toml`.
//!
//! Nazwa tego atrybutu stoi wyżej bez `#` i nawiasu kwadratowego celowo (2026-08-15):
//! `quick-suppressions` gerpuje SUROWY tekst pliku, więc wypisany w pełni wywraca to sprawdzenie
//! także z komentarza, w którym jest tylko wzmianką. Zmierzone na tym pliku, dwa trafienia.
//!
//! Jedna konsekwencja tego jest widoczna w [`reap_group`] i jest **zgłoszona, a nie obejściona**:
//! zabicie grupy, dla której nie mamy uchwytu, wymaga `killpg` po gołym `pgid`, a `process-wrap`
//! wystawia sygnały wyłącznie jako metody uchwytu dziecka. Powód i trzy możliwe drogi stoją
//! przy tej funkcji.
//!
//! Rzeczy, których tu świadomie nie ma, bo należą do innych zadań: zapis `pid`/`pgid` do bazy
//! (T-06 — my je tylko **zwracamy**, synchronicznie, zanim ktokolwiek przeczyta stdout
//! [T7 §6.2]), czytanie NDJSON i tee na dysk (T-05 — my dajemy `ChildStdout` i gwarancję EOF),
//! nazwy i argumenty vendorów (T-04 i T-10 — supervisor nie zna ani jednej), oraz
//! zabezpieczenie czasem startu przed ponownym użyciem PID-u (T-20 — my dajemy [`reap_group`],
//! decyzję *czy wolno* podejmuje odzyskiwanie).

use std::fmt;
use std::io;
use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};

use process_wrap::tokio::{ChildWrapper, CommandWrap};
use tokio::io::AsyncWriteExt;
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::sync::oneshot;
use tokio::time::{sleep, timeout};

/// Jedyne nazwy zmiennych środowiskowych, które przechodzą do dziecka. Wszystko poza tą listą
/// znika przez `env_clear()` (niezmiennik 9).
///
/// Lista stoi w **jednej** stałej, w rdzeniu, a nie w adapterze per vendor (niezmiennik 23):
/// dokładnie tak umarło skanowanie sekretów w repo źródłowym — sterownik dokładał sobie
/// zmienną inline „bo tak szybciej", aż polityka przestała istnieć w jednym miejscu
/// [raport 05 §4]. Dopisanie tu nazwy widać w diffie jako zmianę polityki; dopisanie jej
/// w sterowniku wygląda jak zwykły kod.
///
/// Dlaczego akurat te sześć: `PATH` — bez niej powłoka nie znajdzie ani `node`, ani niczego,
/// co agent uruchamia; `HOME` — tam leżą poświadczenia i konfiguracja CLI; `LANG` i `TERM` —
/// kodowanie wyjścia i to, czy narzędzie sypie kodami sterującymi; `TMPDIR` — na macOS jest
/// per-użytkownik i bez niej narzędzia lądują w `/tmp`; `USER` — część narzędzi buduje z niej
/// ścieżki cache'u. Sekrety i prompt do tej listy nie należą i nigdy nie będą: idą stdinem
/// ([`StdinPlan`]), nigdy w argv i nigdy w pliku tymczasowym.
pub const PASSTHROUGH: &[&str] = &["PATH", "HOME", "LANG", "TERM", "TMPDIR", "USER"];

/// Okno między SIGTERM a SIGKILL w produkcji.
///
/// 5–10 s i **jedno ukryte ustawienie, nigdy kontrolka w UI** [T7 §3.3]. Powód, dla którego
/// w ogóle czekamy: `claude` na SIGTERM dosypuje transkrypt, zwalnia zamek sesji i odpala hooki
/// `SessionEnd`, wychodząc 143 — na SIGKILL nie robi nic z tych rzeczy, a skutek jest
/// niewidoczny aż do pierwszej sesji, której nie da się wznowić [T1 §4.6, 2026-08-15]. Dlatego
/// nigdy nie prowadzimy KILL-em.
pub const DEFAULT_GRACE: Duration = Duration::from_secs(5);

/// Odstęp między dwoma pytaniami „czy w tej grupie ktoś jeszcze jest".
///
/// 2026-08-15 — pętla dowodowa istnieje dlatego, że pomiar z T7 §3.1 (`total=2 orphaned=2`)
/// dotyczył **wnucząt**, a wnuk nie jest naszym dzieckiem: nie zobaczy go żaden `wait()` i nie
/// ma po nim zdarzenia, na którym dałoby się poczekać. Jedyne, co o nim wie, to jądro — więc
/// pytamy jądro, dopóki nie odpowie `ESRCH`. Dziesięć milisekund, bo śmierć po sygnale jest
/// kwestią mikrosekund, a wnuka musi jeszcze zebrać PID 1.
const PROOF_POLL: Duration = Duration::from_millis(10);

/// Ile czekamy na dowód **po** SIGKILL-u. Po dziewiątce nie ma czego negocjować: to sufit na
/// zebranie sierot przez PID 1, a nie drugie okno łaski.
const PROOF_AFTER_KILL: Duration = Duration::from_secs(2);

/// Ile [`Drop`] czeka na zebranie lidera. Musi być krótkie: `Drop` jest synchroniczny i biegnie
/// na wątku roboczym tokio, a po SIGKILL-u lider ginie w mikrosekundach.
const DROP_REAP_LIMIT: Duration = Duration::from_millis(500);

/// Odstęp między próbami zebrania lidera w [`Drop`]. `std::thread::sleep`, bo w `Drop` nie ma
/// czego czekać asynchronicznie — runtime może się w tej chwili zwijać.
const DROP_REAP_POLL: Duration = Duration::from_millis(2);

/// Sygnał, którym **prowadzimy**. Stała, nie liczba w kodzie wywołującym: to jest jedyny plik
/// w repo, który ma prawo znać numery sygnałów (niezmiennik 3).
#[cfg(unix)]
const SIGNAL_TERM: i32 = libc::SIGTERM;

/// Sygnał eskalacji. Nigdy pierwszy — powód stoi przy [`DEFAULT_GRACE`].
#[cfg(unix)]
const SIGNAL_KILL: i32 = libc::SIGKILL;

/// Odpowiedź jądra „w tej grupie nie ma nikogo". Jedyny stan, w którym wolno powiedzieć
/// „nie żyje" (niezmiennik 6).
#[cfg(unix)]
const NO_SUCH_GROUP: i32 = libc::ESRCH;

/// `pid` lidera i `pgid` jego grupy, w jednej wartości, zwracane **synchronicznie** ze
/// [`spawn`].
///
/// Kolejność „wygeneruj, zapisz, dopiero potem czytaj cokolwiek ze stdout" jest tym, co w ogóle
/// czyni odzyskiwanie możliwym [T7 §6.2] — dlatego to jest zwykła wartość dostępna od razu po
/// starcie, a nie coś, co trzeba wyłuskać z pierwszego zdarzenia. Zapisuje ją T-06, sprząta po
/// niej T-20; poza tymi dwoma nikt jej nie potrzebuje i nic więcej „na przyszłość" ten plik nie
/// produkuje (niezmiennik 21).
///
/// Oba pola są `i32`, choć `Child::id()` daje `u32`: POSIX-owy `pid_t` jest **znakowany**,
/// a `kill(-pgid, …)` używa znaku jako selektora grupy. Trzymanie `pgid` w `u32` znaczyłoby, że
/// każde użycie zaczyna się od rzutowania — a rzutowanie w miejscu, gdzie znak jest częścią
/// znaczenia, to najtańszy możliwy sposób na wysłanie sygnału nie tam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupId {
    /// PID lidera grupy, czyli procesu, który naprawdę uruchomiliśmy.
    pub pid: i32,
    /// PGID całej grupy. Na uniksie równy `pid` lidera, ale nazwany osobno, bo to jego używamy
    /// ze znakiem minus i to on, a nie `pid`, jest jednostką zabijania i dowodzenia.
    pub pgid: i32,
}

/// Co [`Supervised::stop`] i [`reap_group`] mają prawo powiedzieć o grupie.
///
/// Niezmiennik 6 czyta się dosłownie: **dopóki `kill(-pgid, 0)` nie dał `ESRCH`, grupa jest
/// żywa.** Cicha wersja złamania tego niezmiennika to `stop() -> io::Result<()>` — `Ok(())`
/// znaczy wtedy „wysłałem sygnał", a wołający czyta „nie żyje". Dlatego zatrzymanie zwraca
/// wartość dowodu, nie jednostkę.
#[derive(Debug)]
pub enum GroupProof {
    /// `kill(-pgid, 0)` zwrócił `ESRCH`: w grupie nie ma już **ani jednego** procesu — także
    /// żadnego zombie, bo zombie nadal odpowiada na sygnał zerowy. To jedyny stan, w którym
    /// wolno powiedzieć „nie żyje".
    ///
    /// `status` niesie kod wyjścia lidera, jeśli to my go zebraliśmy — po nim poznaje się
    /// różnicę między czystym wyjściem po SIGTERM a sygnałem 9 po eskalacji. `None` przy
    /// powtórzonym zatrzymaniu tej samej grupy: status jest do odebrania raz, a drugie
    /// `stop()` nadal musi być bezbłędne.
    Dead { status: Option<ExitStatus> },

    /// Grupa nadal odpowiada na sygnał zerowy. To jest wynik do obsłużenia, nie błąd do
    /// zalogowania: osierocony `claude` pali limit w tle [T7 §10.1].
    Alive,
}

/// Co dziecko dostaje na stdin. Jedyna droga, którą wchodzą prompt i sekrety (niezmiennik 9):
/// nigdy argv, nigdy plik tymczasowy, nigdy dziennik.
#[derive(Debug, Clone)]
pub enum StdinPlan {
    /// `/dev/null` — dziecko dostaje EOF natychmiast.
    ///
    /// Bez tego `claude` czeka ~3 s i wypisuje `Warning: no stdin data received in 3s…`
    /// [T1 §4.6, 2026-08-15]; przy czterech agentach to dwanaście sekund niczego, przy każdym
    /// kroku każdego biegu.
    Null,
    /// Jeden zapis na stdin, potem zamknięcie deskryptora — czyli EOF, którego dziecko i tak
    /// czeka. Tędy idzie prompt i tędy idą sekrety.
    Write(String),
    /// Ten sam pierwszy zapis, ale deskryptor **zostaje otwarty** i wraca do wołającego przez
    /// [`Supervised::stdin`]. Kanał na drugą turę i na przerwanie w paśmie.
    ///
    /// 2026-08-15 — bez tego wariantu jeden proces obsługuje dokładnie jedną turę: koperta
    /// kolejnej tury nie ma dokąd pojechać, a `control_request`/`interrupt` — który jedzie tą
    /// samą drogą — nie ma czym wyjść, więc anulowanie prowadzi sygnałem i traci wznawialność
    /// sesji [T1 §4.6]. Alternatywą byłby świeży proces na turę z `--resume`, czyli zimny start
    /// i odbudowa cache'u przy **każdej** turze [T1 §8.1]; to jest ten koszt, którego cały ten
    /// kształt ma uniknąć.
    ///
    /// EOF jest tu **osobnym czasownikiem**: dziecko dostaje go dopiero wtedy, gdy wołający
    /// porzuci potok oddany mu przez [`Supervised::stdin`]. To jest różnica między „koniec tury"
    /// a „koniec sesji".
    Keep(String),
}

/// Jak skończył się bieg z limitem czasu.
///
/// Wariant limitu niesie [`GroupProof`], a nie samą informację „upłynęło", bo niezmiennik 10
/// jest właśnie o tym: `tokio::time::timeout` wokół kroku anuluje **zadanie Rusta, nie proces
/// systemowy**. Kod, który zwraca gołe „Timeout", kompiluje się, czyta się dobrze i zostawia
/// żywego agenta [T7 §10.8 — jedyny defekt w tym raporcie z adnotacją „łatwo zregresować,
/// pokryj testem"].
#[derive(Debug)]
pub enum RunOutcome {
    /// Proces skończył się sam, w oknie limitu.
    Exited { group: GroupId, status: ExitStatus },
    /// Limit upłynął, a grupa przeszła przez pełną eskalację zabijania — `proof` jest tym, co
    /// z niej zostało. Wołający dostaje `pgid`, żeby móc zapytać system, a nie nas.
    TimedOut { group: GroupId, proof: GroupProof },
}

/// Uchwyt do żywej grupy procesów.
///
/// Porzucenie uchwytu **też** zabija grupę: wołający wychodzi z funkcji spawnującej przez
/// wczesne `?` częściej niż ścieżką, na której pamiętał o zatrzymaniu, a osierocona grupa
/// kosztuje pieniądze [T7 §3.1]. Gwardia w `Drop` jest ostatnią linią, nie pierwszą: normalna
/// droga to [`Supervised::stop`], bo tylko ona umie poczekać na łaskę.
pub struct Supervised {
    /// `pid` i `pgid`, gotowe od razu po starcie — T-06 zapisuje je, zanim popłynie stdout.
    group: GroupId,

    /// Dziecko opakowane przez `process-wrap` 9.1.0 (nie `command-group`: tamten nie był
    /// ruszany od 2023-11-18 [T7 §3.2]). To opakowanie, a nie `tokio::process::Child`, jest tu
    /// istotne: jego sygnały idą na **grupę**, a nie na jeden proces.
    child: Box<dyn ChildWrapper>,

    /// Odebrany strumień wyjścia, czekający na tego, kto go czyta (T-05). Oddawany raz.
    stdout: Option<ChildStdout>,

    /// Potok wejściowy wracający z zadania, które wykonało pierwszy zapis z
    /// [`StdinPlan::Keep`]. `None` dla planów, które ten deskryptor zamykają.
    ///
    /// Kanałem, a nie gołym uchwytem, bo pierwszy zapis biegnie **w zadaniu** (powód przy
    /// [`spawn`]), a potok jest jeden: dopóki tamten zapis trwa, nie ma czego oddać.
    stdin: Option<oneshot::Receiver<ChildStdin>>,

    /// Status lidera, jeśli to my go zebraliśmy. Bez niego nie da się odróżnić czystego wyjścia
    /// po SIGTERM od sygnału 9 po eskalacji, czyli nie widać, czy łaska w ogóle działa.
    status: Option<ExitStatus>,

    /// Czy `ESRCH` już padło. Dowód jest jednorazowy z dwóch stron: powtórzone `stop()` ma nadal
    /// odpowiadać `Dead`, a `Drop` po udanym `stop()` nie ma już czego zabijać — zwolniony
    /// `pgid` może w tej chwili należeć do kogoś innego [T7 §10.2].
    proved_dead: bool,
}

impl fmt::Debug for Supervised {
    /// Ręcznie, bo uchwytu dziecka nie da się pokazać sensownie, a `Debug` na tym typie trafia
    /// wprost do komunikatów asercji w testach nadzoru.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Supervised")
            .field("group", &self.group)
            .field("status", &self.status)
            .field("proved_dead", &self.proved_dead)
            .finish_non_exhaustive()
    }
}

impl Supervised {
    /// `pid` i `pgid`, dostępne od razu po starcie i bez czekania na cokolwiek ze stdout.
    #[must_use]
    pub fn group(&self) -> GroupId {
        self.group
    }

    /// Odbiera strumień wyjścia. `None` przy drugim wywołaniu — strumień jest jeden i oddaje
    /// się go raz, temu, kto go czyta (T-05).
    pub fn stdout(&mut self) -> Option<ChildStdout> {
        self.stdout.take()
    }

    /// Odbiera potok wejściowy — ten sam, przez który poszedł pierwszy zapis. `None` przy
    /// każdym planie poza [`StdinPlan::Keep`] i przy drugim wywołaniu: potok jest jeden
    /// i oddaje się go raz, dokładnie jak strumień wyjścia.
    ///
    /// Czeka, aż pierwszy zapis dojdzie do końca, i to nie jest kwestia gustu: druga koperta
    /// wysłana w środek pierwszej przeplotłaby się z nią w tym samym potoku, a CLI czyta stdin
    /// **linia po linii** — rozjechana linia to cała tura zgubiona po drugiej stronie.
    ///
    /// Zamknięcie deskryptora należy do tego, kto go stąd wziął: porzucenie zwróconej wartości
    /// jest tym EOF-em, po którym `claude` wychodzi sam [T1 §2].
    pub async fn stdin(&mut self) -> Option<ChildStdin> {
        self.stdin.take()?.await.ok()
    }

    /// Czeka na naturalne wyjście lidera i **zbiera** go, żeby nie został zombie.
    ///
    /// `wait()` musi paść na każdej ścieżce terminalnej, inaczej `kill(-pgid, 0)` będzie dalej
    /// zwracać zero dla samego zombie i dowód z niezmiennika 6 nigdy nie nadejdzie.
    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        if let Some(status) = self.status {
            return Ok(status);
        }
        let status = self.child.wait().await?;
        self.status = Some(status);
        Ok(status)
    }

    /// SIGTERM na **grupę**, okno łaski, potem SIGKILL na grupę — i dopiero wtedy dowód.
    ///
    /// Nigdy nie prowadzimy KILL-em: `claude` na SIGTERM dosypuje transkrypt i zwalnia zamek
    /// sesji, na SIGKILL nie robi nic [T1 §4.6]. Zwrócona wartość jest wynikiem pętli
    /// dowodowej, a nie potwierdzeniem wysłania sygnału: `GroupProof::Dead` wolno zwrócić
    /// dopiero wtedy, gdy `kill(-pgid, 0)` odpowiedział `ESRCH` — bo to jest ten pomiar, który
    /// w T7 §3.1 pokazał `total=2 orphaned=2` w chwili, w której status bezpośredniego dziecka
    /// mówił „zabity".
    ///
    /// Wołane drugi raz na tej samej grupie nadal zwraca `Dead`, tylko bez statusu: powtórzone
    /// zatrzymanie jest normalną ścieżką (anulowanie biegu, po którym idzie `Drop`), a nie
    /// błędem.
    pub async fn stop(&mut self, grace: Duration) -> GroupProof {
        if self.proved_dead {
            return GroupProof::Dead { status: None };
        }

        let began = Instant::now();

        // 1. Prowadzimy TERM-em i wysyłamy go na CAŁĄ grupę, nie na lidera: to wnuki przeżyły
        //    pomiar z T7 §3.1, a lider zginął już wtedy.
        let _ = self.child.signal(SIGNAL_TERM);

        // 2. Czekamy na lidera, ale najwyżej przez okno łaski. Porzucenie tego future'a niczego
        //    nie zostawia przy życiu: proces dostał sygnał, a eskalacja jest niżej w TEJ SAMEJ
        //    funkcji — na tym polega niezmiennik 10.
        let waited = timeout(grace, self.child.wait()).await;
        if let Ok(Ok(status)) = waited {
            self.status = Some(status);
        }

        // 3. Dowód, wciąż w oknie łaski: lider bywa najszybszy, a płacimy za wnuki.
        let left = grace.saturating_sub(began.elapsed());
        if self.prove_gone(SIGNAL_TERM, left).await {
            self.proved_dead = true;
            return GroupProof::Dead {
                status: self.status,
            };
        }

        // 4. Okno minęło — dopiero teraz dziewiątka, i też na grupę.
        let _ = self.child.start_kill();
        let reaped = timeout(PROOF_AFTER_KILL, self.child.wait()).await;
        if let Ok(Ok(status)) = reaped {
            self.status = Some(status);
        }
        if self.prove_gone(SIGNAL_KILL, PROOF_AFTER_KILL).await {
            self.proved_dead = true;
            return GroupProof::Dead {
                status: self.status,
            };
        }

        // Bez `ESRCH` nie wolno powiedzieć „nie żyje" (niezmiennik 6). To jest wynik do
        // obsłużenia przez wołającego, nie błąd do zalogowania: ktoś w tej grupie dalej biegnie.
        GroupProof::Alive
    }

    /// Czy w grupie nie ma już **nikogo**.
    ///
    /// Pytamy sygnałem zerowym: nic nie dostarcza, sprawdza wyłącznie istnienie i prawa. Kiedy
    /// opakowanie odmówi zera — bo mapuje `i32` na wyliczenie sygnałów, w którym zera nie ma —
    /// pytamy jeszcze raz tym sygnałem, który tej grupie i tak już posłaliśmy. Powtórzenie nie
    /// zmienia intencji, a `ESRCH` znaczy wtedy dokładnie to samo: nie ma komu odpowiedzieć.
    ///
    /// Każda inna odpowiedź to „żywa", łącznie z `EPERM`, który znaczy, że grupa istnieje, tylko
    /// nie jest nasza. Niezmiennik 6 nie zna stanu „chyba nie żyje".
    #[cfg(unix)]
    fn group_is_gone(&mut self, fallback: i32) -> bool {
        let asked = self.child.signal(0);
        match asked {
            Ok(()) => false,
            Err(error) if error.raw_os_error() == Some(NO_SUCH_GROUP) => true,
            Err(_) => means_empty_group(&self.child.signal(fallback)),
        }
    }

    /// Pętla dowodowa: pyta jądro co [`PROOF_POLL`], aż odpowie `ESRCH` albo minie `limit`.
    ///
    /// 2026-08-15 — to jest ta pętla, której brak dał w T7 §3.1 `total=2 orphaned=2`: status
    /// lidera mówił „zabity", a dwoje wnucząt biegło pod PID 1 i paliło limit. Wnuka nie widzi
    /// żaden nasz `wait()`, więc jedynym źródłem prawdy jest jądro.
    async fn prove_gone(&mut self, fallback: i32, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        loop {
            if self.group_is_gone(fallback) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            sleep(PROOF_POLL).await;
        }
    }
}

impl Drop for Supervised {
    /// Ostatnia linia obrony przed wyciekiem grupy na ścieżce błędu.
    ///
    /// Musi być **synchroniczna** i nie wolno jej niczego czekać w tokio: `Drop` biegnie także
    /// wtedy, gdy runtime się zwija. Dlatego tu stoi twardy `killpg` plus zebranie potomka,
    /// a łaska mieszka wyłącznie w [`Supervised::stop`] — kto chce, żeby `claude` zdążył
    /// zamknąć sesję, ten woła `stop()`, a nie liczy na `Drop`.
    fn drop(&mut self) {
        if self.proved_dead {
            return;
        }

        // 2026-08-15 — dziewiątka bez łaski, bo to jest ścieżka, na której wołający wyszedł
        // wcześniej przez `?` i nikt już nie trzyma niczego, czym dałoby się poczekać.
        // Zostawiona grupa to `claude` palący limit w tle, zmierzone jako `total=2 orphaned=2`
        // [T7 §3.1].
        let _ = self.child.start_kill();

        // Zebranie lidera jest częścią zabijania, nie sprzątaniem po nim: zombie **nadal
        // odpowiada** na sygnał zerowy, więc grupa z zombie w środku nigdy nie da `ESRCH` —
        // ani tutaj, ani w odzyskiwaniu, które zobaczy z bazy sam `pgid`.
        let deadline = Instant::now() + DROP_REAP_LIMIT;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) | Err(_) => break,
                Ok(None) => {}
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(DROP_REAP_POLL);
        }
    }
}

/// Czy ta odpowiedź jądra znaczy „w tej grupie nie ma nikogo".
#[cfg(unix)]
fn means_empty_group(answer: &io::Result<()>) -> bool {
    match answer {
        Ok(()) => false,
        Err(error) => error.raw_os_error() == Some(NO_SUCH_GROUP),
    }
}

/// Startuje komendę we **własnej grupie procesów** i zwraca uchwyt.
///
/// Trzy rzeczy dzieją się tutaj i nigdzie indziej, bo polityka mieszka w jednym rdzeniu
/// (niezmiennik 23):
///
/// 1. `ProcessGroup::leader()` z `process-wrap` — na uniksie `setpgid`, na Windows `JobObject`
///    w tym samym miejscu wywołania [T7 §3.2, §9.2]. To jest jedyny powód, dla którego
///    `kill(-pgid, …)` w ogóle ma sens: bez własnej grupy wnuki `claude` przeżywają
///    zatrzymanie [T7 §3.1].
/// 2. `env_clear()` plus [`PASSTHROUGH`] — dziecko nie dziedziczy niczego, czego mu jawnie nie
///    daliśmy (niezmiennik 9).
/// 3. stdio: stdout i stderr na potoki (T-05 je czyta), stdin według [`StdinPlan`]. Nigdy
///    odziedziczony stdin — to on kosztuje ~3 s ostrzeżenia na każdym kroku [T1 §4.6].
///
/// Zwracane [`GroupId`] jest dostępne **zanim** cokolwiek zostanie przeczytane ze stdout, bo
/// dopiero to czyni odzyskiwanie możliwym [T7 §6.2].
///
/// Cooldown po nieudanym spawnie — ochrona przed burzą restartów — wszedłby dokładnie tutaj,
/// wokół gałęzi błędu. Nie w v1: bez pętli ponawiania nie ma czego tłumić.
pub fn spawn(mut command: Command, stdin: StdinPlan) -> io::Result<Supervised> {
    // Prompt i sekrety wchodzą wyłącznie tędy (niezmiennik 9). `Null` to `/dev/null`, czyli EOF
    // natychmiast — bez tego `claude` czeka ~3 s na każdym kroku [T1 §4.6].
    let (plan, prompt) = match stdin {
        StdinPlan::Null => (Stdio::null(), None),
        StdinPlan::Write(text) => (Stdio::piped(), Some((text, false))),
        StdinPlan::Keep(text) => (Stdio::piped(), Some((text, true))),
    };
    command.stdin(plan);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    // Najpierw pusto, potem jawna lista. Odwrotna kolejność nie istnieje: `env_clear()` po
    // dołożeniu nazw skasowałoby także je.
    command.env_clear();
    for &name in PASSTHROUGH {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }

    let mut wrapped = into_own_group(command);
    let mut child: Box<dyn ChildWrapper> = wrapped.spawn()?;

    let Some(pid) = child.id() else {
        return Err(io::Error::other(
            "the child was gone before it could report a pid",
        ));
    };
    let pid = i32::try_from(pid).map_err(io::Error::other)?;

    // Potok odbieramy od razu: uchwyt, który go w sobie trzyma, jest uchwytem, z którego T-05
    // nie przeczyta ani linii — a EOF na tym potoku ma osobne kryterium.
    let stdout = child.stdout().take();

    let mut kept = None;
    if let Some((text, keep)) = prompt
        && let Some(mut pipe) = child.stdin().take()
    {
        // Zapis w osobnym zadaniu, nie tutaj: bufor potoku ma ~64 KB, a prompt bywa
        // większy — zapis synchroniczny stanąłby na pełnym buforze, czekając na dziecko,
        // które czeka na resztę promptu. Ta funkcja nie jest asynchroniczna, więc nie ma tu
        // nawet czego czekać.
        if keep {
            // Potok WRACA do uchwytu zamiast zniknąć razem z zadaniem: to jest cała różnica
            // między jedną turą na proces a sesją, która przyjmuje kolejne koperty. EOF wyśle
            // dopiero ten, kto go stąd weźmie i porzuci.
            let (give, take) = oneshot::channel();
            let _writer = tokio::spawn(async move {
                let _ = pipe.write_all(text.as_bytes()).await;
                let _ = pipe.flush().await;
                // Odbiorca mógł już zniknąć — wtedy potok ginie razem z tą wartością i dziecko
                // dostaje EOF, czyli dokładnie to samo, co przy planie zamykającym.
                let _ = give.send(pipe);
            });
            kept = Some(take);
        } else {
            // Zamknięcie deskryptora po zapisie jest tym EOF-em, którego agent i tak wypatruje.
            let _writer = tokio::spawn(async move {
                let _ = pipe.write_all(text.as_bytes()).await;
                let _ = pipe.shutdown().await;
            });
        }
    }

    Ok(Supervised {
        // `ProcessGroup::leader()` woła na uniksie `setpgid(0, 0)`, więc `pgid` lidera jest
        // równy jego `pid`. Trzymamy oba pod własnymi nazwami, bo to `pgid` jedzie ze znakiem
        // minus i to on, a nie `pid`, jest jednostką zabijania i dowodzenia.
        group: GroupId { pid, pgid: pid },
        child,
        stdout,
        stdin: kept,
        status: None,
        proved_dead: false,
    })
}

/// Wkłada komendę do własnej grupy procesów — **jedyne** miejsce w repo, które zna różnicę
/// między systemami (niezmiennik 3).
///
/// 2026-08-15 — bez tej jednej linii `Child::kill()` sygnalizuje wyłącznie bezpośrednie
/// dziecko, a `claude` jest skryptem powłoki: zmierzone `A after kill: total=2 orphaned=2`,
/// czyli dwoje wnucząt pod PID 1, dalej mielących i dalej palących limit [T7 §3.1]. Ten sam
/// pomiar z własną grupą dał `total=0 orphaned=0` [T7 §3.2].
#[cfg(unix)]
fn into_own_group(command: Command) -> CommandWrap {
    let mut wrapped = CommandWrap::from(command);
    let leader = process_wrap::tokio::ProcessGroup::leader();
    // `let _ =`, bo budowniczy oddaje `&mut Self`, a `unused_must_use` jest w tej skrzyni
    // ustawione na `deny` — statement, który zgubi taki zwrot, przewraca bramkę, nie kod.
    let _ = wrapped.wrap(leader);
    wrapped
}

/// Windows: to samo miejsce wywołania, `JobObject` zamiast grupy procesów [T7 §9.2].
///
/// Zostaje `unimplemented!` z powodem opisanym słowami, bo nie ma tu hosta Windows, na którym
/// dałoby się to sprawdzić [T7 §11.3] — a gałąź platformowa, której nikt nigdy nie uruchomił,
/// jest warta dokładnie tyle, ile jej test. Wejdzie razem z własną eskalacją: `JobObject` nie
/// zna SIGTERM-a, więc łaska po tamtej stronie znaczy co innego niż „wyślij piętnastkę".
#[cfg(windows)]
fn into_own_group(_command: Command) -> CommandWrap {
    unimplemented!("a JobObject goes here; nobody has run it")
}

/// Uruchamia komendę i pilnuje, żeby przekroczenie `limit` przeszło **ścieżką zabijania**.
///
/// Niezmiennik 10 w jednym zdaniu: `tokio::time::timeout` wokół kroku anuluje zadanie Rusta,
/// nie proces systemowy. Kod, który po upływie limitu robi `return Timeout`, kompiluje się,
/// czyta się dobrze i zostawia żywego agenta [T7 §10.8] — dlatego wariant limitu w
/// [`RunOutcome`] niesie [`GroupProof`], czyli rzecz, której nie da się zwrócić bez zabicia
/// grupy.
///
/// Stdin dostaje [`StdinPlan::Null`]: ta droga jest dla kroków bez promptu, a prompt idzie
/// przez [`spawn`] i [`StdinPlan::Write`]. Okno łaski to [`DEFAULT_GRACE`].
///
/// Limit Loadouta musi być **krótszy** niż sufit vendora: `claude -p` czeka na subagentów
/// w tle domyślnie do 10 minut [T1, „Worth adding"], więc bez własnego, krótszego limitu
/// zaklinowany subagent trzyma proces sterownika tak długo, jak zechce.
pub async fn run_with_deadline(command: Command, limit: Duration) -> io::Result<RunOutcome> {
    let mut handle = spawn(command, StdinPlan::Null)?;
    let group = handle.group();

    // Wynik idzie do własnej zmiennej, a nie wprost do `match`: future z `wait()` pożycza
    // uchwyt, a pożyczka trwa do końca instrukcji. W `match` byłaby żywa jeszcze w ramieniu,
    // w którym wołamy `stop()` — czyli dokładnie tam, gdzie musi jej już nie być.
    let ended = timeout(limit, handle.wait()).await;

    match ended {
        Ok(status) => Ok(RunOutcome::Exited {
            group,
            status: status?,
        }),
        // Upłynięcie limitu nie kończy tej funkcji, tylko wprowadza ją w eskalację. To jest cała
        // różnica między „zgłosiliśmy limit" a „limit czegokolwiek dokonał".
        Err(_elapsed) => {
            let proof = handle.stop(DEFAULT_GRACE).await;
            Ok(RunOutcome::TimedOut { group, proof })
        }
    }
}

/// Zabija grupę po samym `pgid` i zwraca dowód. Bez uchwytu — po nią sięga odzyskiwanie po
/// awarii aplikacji (T-20), które ma z bazy tylko liczbę.
///
/// **Decyzję, czy wolno**, podejmuje wołający, nie ta funkcja: PID-y są używane ponownie,
/// a zabicie cudzej grupy to prawdziwy błąd poprawności, nie teoretyczny [T7 §10.2].
/// Zabezpieczenie czasem startu (`sysctl kern.boottime`) mieszka w T-20 — tutaj wystawiamy
/// wyłącznie neutralny czasownik, żeby nikt nie musiał importować stałych sygnałów u siebie
/// i złamać niezmiennika 3 przy okazji.
///
/// # Ciała nie ma, i to jest zgłoszenie, nie niedopatrzenie (2026-08-15)
///
/// Ta funkcja jako jedyna w tym pliku potrzebuje `killpg` po **gołym `pgid`**, bez uchwytu
/// dziecka. `process-wrap` wystawia sygnały wyłącznie jako metody `ProcessGroupChild`, czyli
/// zawsze przez uchwyt, którego odzyskiwanie po awarii z definicji nie ma. Zostają trzy drogi
/// i każda wychodzi poza to zadanie:
///
/// * `libc::killpg` — wymaga `unsafe`, a w tej skrzyni stoi `unsafe_code = "deny"`
///   (`Cargo.toml`, poza blokiem OWNS). Atrybut `allow(unsafe_code)` przewraca
///   `checks/quick-suppressions.sh`, a jedyne przejście przez nie —
///   `checks/suppressions-allowlist.json` z pisemnym powodem — leży w `checks/`, czyli w tym,
///   co nas sądzi (`AGENTS.md` §7).
/// * Druga zależność (`nix`) — dopisek do `src-tauri/Cargo.toml`, którego to zadanie wprost nie
///   posiada („nie dopisuj nic do `Cargo.toml`").
/// * `std::process::Command::new("kill")` — wykluczone przez samo zadanie.
///
/// Żadne kryterium akceptacji tej funkcji nie dotyka, więc jej brak niczego nie zazielenia
/// nieuczciwie. Sygnatura zostaje, bo po nią sięgnie T-20; ciało czeka na decyzję człowieka,
/// którą z trzech dróg repo wybiera.
#[must_use]
pub fn reap_group(_pgid: i32) -> GroupProof {
    unimplemented!("killpg by bare pgid needs unsafe")
}
