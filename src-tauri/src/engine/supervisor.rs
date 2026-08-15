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
//! # Adres tego modułu jest TYMCZASOWY (2026-08-15)
//!
//! Docelowo `engine::supervisor`, a jedyną poprawną deklaracją jest `pub mod supervisor;`
//! w `src-tauri/src/engine/mod.rs`. Tej linii tam nie ma — jest wyłącznie w komentarzu, na
//! liście wierszy, które dołożą kolejne zadania (`engine/mod.rs:45`), mimo że `tasks/T-03.md`
//! twierdzi, że T-02 już ją wpisał. Jeden wiersz poza blokiem `OWNS` tego zadania to pytanie
//! do człowieka (`AGENTS.md` §7), więc T-03 go nie dopisuje: zamiast tego ten sam **plik** jest
//! wciągany z korzenia skrzyni, którą T-03 posiada. Szczegóły i warunek usunięcia stoją przy
//! deklaracji w `src-tauri/src/lib.rs`.
//!
//! # Stan tego pliku: SZKIELET (2026-08-15)
//!
//! Ciała są `unimplemented!` z powodem opisanym słowami. Test ma się **skompilować** i paść
//! **w czasie wykonania, na braku ZACHOWANIA** (`AGENTS.md` §2a p. 5) — test, który się nie
//! kompiluje, niczego nie uruchomił, więc niczego nie dowodzi. Żadnego z sześciu kryteriów nie
//! da się przejść na tym szkielecie: panika nie jest zieloną asercją.
//!
//! Dlaczego `unimplemented!`, a nie `todo!`: `clippy::todo = "deny"` w `Cargo.toml` obowiązuje
//! **w tej samej bramce**, w której `before` ma być czerwone. `todo!()` zaczerwieniłby
//! `checks/quick-clippy.sh` (`cargo clippy --lib -- -D warnings`), czyli sprawdzenie
//! PROJEKTOWE, a nie kryterium — zmierzone 2026-08-15, przy pierwszym uruchomieniu bramki dla
//! tego zadania. `clippy::unimplemented` nie jest w `Cargo.toml` włączony i nie należy do
//! `clippy::all`, więc szkielet może być czerwony dokładnie tam, gdzie ma być: w sześciu
//! kryteriach.
//!
//! Rzeczy, których tu świadomie nie ma, bo należą do innych zadań: zapis `pid`/`pgid` do bazy
//! (T-06 — my je tylko **zwracamy**, synchronicznie, zanim ktokolwiek przeczyta stdout
//! [T7 §6.2]), czytanie NDJSON i tee na dysk (T-05 — my dajemy `ChildStdout` i gwarancję EOF),
//! nazwy i argumenty vendorów (T-04 i T-10 — supervisor nie zna ani jednej), oraz
//! zabezpieczenie czasem startu przed ponownym użyciem PID-u (T-20 — my dajemy [`reap_group`],
//! decyzję *czy wolno* podejmuje odzyskiwanie).

use std::io;
use std::process::ExitStatus;
use std::time::Duration;

use tokio::process::{ChildStdout, Command};

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
#[derive(Debug)]
pub struct Supervised {
    /// SZKIELET (2026-08-15) — obok tego pola implementacja położy `Box<dyn ChildWrapper>`
    /// z `process-wrap` 9.1.0 (nie `command-group`: tamten nie był ruszany od 2023-11-18
    /// [T7 §3.2]) oraz odebrany `ChildStdout`.
    group: GroupId,
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
        unimplemented!("no pipe was ever opened for {:?}", self.group)
    }

    /// Czeka na naturalne wyjście lidera i **zbiera** go, żeby nie został zombie.
    ///
    /// `wait()` musi paść na każdej ścieżce terminalnej, inaczej `kill(-pgid, 0)` będzie dalej
    /// zwracać zero dla samego zombie i dowód z niezmiennika 6 nigdy nie nadejdzie.
    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        unimplemented!("nothing is being reaped for {:?}", self.group)
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
    pub async fn stop(&mut self, _grace: Duration) -> GroupProof {
        unimplemented!("no TERM, no grace, no KILL, no proof for {:?}", self.group)
    }
}

impl Drop for Supervised {
    /// Ostatnia linia obrony przed wyciekiem grupy na ścieżce błędu.
    ///
    /// Musi być **synchroniczna** i nie wolno jej niczego czekać w tokio: `Drop` biegnie także
    /// wtedy, gdy runtime się zwija. Dlatego tu stanie twardy `killpg` plus zebranie potomka,
    /// a łaska mieszka wyłącznie w [`Supervised::stop`] — kto chce, żeby `claude` zdążył
    /// zamknąć sesję, ten woła `stop()`, a nie liczy na `Drop`.
    fn drop(&mut self) {
        unimplemented!("a dropped handle still leaks {:?}", self.group)
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
pub fn spawn(_command: Command, _stdin: StdinPlan) -> io::Result<Supervised> {
    unimplemented!("nothing is started, so there is no group to hand back")
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
pub async fn run_with_deadline(_command: Command, _limit: Duration) -> io::Result<RunOutcome> {
    unimplemented!("the deadline never reaches the kill escalation")
}

/// Zabija grupę po samym `pgid` i zwraca dowód. Bez uchwytu — po nią sięga odzyskiwanie po
/// awarii aplikacji (T-20), które ma z bazy tylko liczbę.
///
/// **Decyzję, czy wolno**, podejmuje wołający, nie ta funkcja: PID-y są używane ponownie,
/// a zabicie cudzej grupy to prawdziwy błąd poprawności, nie teoretyczny [T7 §10.2].
/// Zabezpieczenie czasem startu (`sysctl kern.boottime`) mieszka w T-20 — tutaj wystawiamy
/// wyłącznie neutralny czasownik, żeby nikt nie musiał importować stałych sygnałów u siebie
/// i złamać niezmiennika 3 przy okazji.
#[must_use]
pub fn reap_group(_pgid: i32) -> GroupProof {
    unimplemented!("no group is killed and no ESRCH is ever proved")
}
