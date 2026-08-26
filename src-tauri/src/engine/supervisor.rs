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

use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::{Component, Path};
use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};

use process_wrap::tokio::{ChildWrapper, CommandWrap};
use tokio::io::AsyncWriteExt;
use tokio::process::{ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::oneshot;
use tokio::time::{sleep, timeout};

/// Sposób otwarcia istniejącego prywatnego artefaktu przez dowiedzioną ścieżkę katalogów.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateFileAccess {
    Read,
    Append,
    CreateAppend,
}

/// Otwiera istniejący prywatny plik bez śledzenia symlinków na żadnym poziomie ścieżki.
///
/// API jest neutralne wobec platformy; `openat`/`O_NOFOLLOW` i kontrola właściciela mieszkają
/// wyłącznie tutaj, zgodnie z niezmiennikiem 3. Wołający dostaje już sprawdzony uchwyt i nie
/// wykonuje rozdzielonego `metadata(path) -> open(path)`, w którym katalog może się podmienić.
#[cfg(unix)]
pub fn open_private_file(
    anchor: &Path,
    relative: &Path,
    access: PrivateFileAccess,
) -> io::Result<std::fs::File> {
    use nix::fcntl::{OFlag, openat};
    use nix::sys::stat::Mode;

    let access_flags = match access {
        PrivateFileAccess::Read => OFlag::O_RDONLY,
        PrivateFileAccess::Append => OFlag::O_WRONLY | OFlag::O_APPEND,
        PrivateFileAccess::CreateAppend => {
            OFlag::O_WRONLY | OFlag::O_APPEND | OFlag::O_CREAT | OFlag::O_EXCL
        }
    } | OFlag::O_NOFOLLOW
        | OFlag::O_CLOEXEC;
    let (directory, file_name) = private_parent(anchor, relative)?;
    let file = openat(
        &directory,
        Path::new(&file_name),
        access_flags,
        if access == PrivateFileAccess::CreateAppend {
            Mode::from_bits_truncate(0o600)
        } else {
            Mode::empty()
        },
    )
    .map_err(io::Error::from)?;
    validate_private_fd(&file)?;
    Ok(std::fs::File::from(file))
}

/// Publikuje mały prywatny dokument atomowo przez ten sam dowiedziony deskryptor katalogu.
///
/// To jest druga połowa [`open_private_file`]: samo sprawdzenie istniejącego pliku przez fd nie
/// wystarcza, jeżeli późniejszy `rename(path, path)` ponownie rozwiązuje ścieżkę. Tutaj zarówno
/// plik tymczasowy, jak i nazwa docelowa są operacjami `*at` na jednym otwartym katalogu. Zmiana
/// któregoś rodzica na symlink po otwarciu nie może więc przekierować prywatnych bajtów.
pub struct PrivateFilePublisher {
    #[cfg(unix)]
    directory: std::os::fd::OwnedFd,
    #[cfg(unix)]
    file_name: std::ffi::OsString,
}

impl fmt::Debug for PrivateFilePublisher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Nazwa docelowa może pochodzić z prywatnej tożsamości rozmowy; sam fakt zachowania
        // deskryptora wystarcza do diagnostyki i nie wypuszcza ścieżki do zwykłego logu.
        formatter
            .debug_struct("PrivateFilePublisher")
            .field("directory_held", &true)
            .finish_non_exhaustive()
    }
}

impl PrivateFilePublisher {
    /// Dowodzi całej ścieżki rodziców i zachowuje deskryptor ostatniego katalogu.
    ///
    /// Rozdzielenie `open` od [`Self::publish`] jest celowe i daje testowi awarii możliwość
    /// podmiany nazwy rodzica pomiędzy tymi operacjami. Produkcja korzysta z dokładnie tego
    /// samego obiektu co produkcyjny zapis evidence, więc oracle zabija powrót do ścieżkowego
    /// `metadata -> rename`, a nie wyłącznie testową kopię reguły.
    #[cfg(unix)]
    pub fn open(anchor: &Path, relative: &Path) -> io::Result<Self> {
        let (directory, file_name) = private_parent(anchor, relative)?;
        Ok(Self {
            directory,
            file_name,
        })
    }

    #[cfg(windows)]
    pub fn open(_anchor: &Path, _relative: &Path) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "private no-follow file publication is not implemented on Windows",
        ))
    }

    /// Zapisuje i publikuje względem zachowanego katalogu, bez ponownego rozwiązania ścieżki.
    #[cfg(unix)]
    pub fn publish(self, bytes: &[u8], replace: bool) -> io::Result<()> {
        use std::io::Write as _;

        use nix::errno::Errno;
        use nix::fcntl::{AtFlags, OFlag, openat, renameat};
        use nix::sys::stat::{Mode, fstatat};
        use nix::unistd::{UnlinkatFlags, fsync, linkat, unlinkat};

        let Self {
            directory,
            file_name,
        } = self;
        let mut guard_name = file_name.clone();
        guard_name.push(".writing");
        match fstatat(
            &directory,
            Path::new(&guard_name),
            AtFlags::AT_SYMLINK_NOFOLLOW,
        ) {
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "an evidence writing guard already exists",
                ));
            }
            Err(Errno::ENOENT) => {}
            Err(error) => return Err(io::Error::from(error)),
        }

        if replace {
            let existing = openat(
                &directory,
                Path::new(&file_name),
                OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .map_err(io::Error::from)?;
            validate_private_fd(&existing)?;
        }

        let temporary_name = format!(".loadout-writing-{}", uuid::Uuid::now_v7());
        let temporary = openat(
            &directory,
            temporary_name.as_str(),
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::from_bits_truncate(0o600),
        )
        .map_err(io::Error::from)?;
        validate_private_fd(&temporary)?;

        let written = (|| -> io::Result<()> {
            let mut file = std::fs::File::from(temporary);
            file.write_all(bytes)?;
            file.flush()?;
            file.sync_all()?;
            drop(file);
            if replace {
                renameat(
                    &directory,
                    temporary_name.as_str(),
                    &directory,
                    Path::new(&file_name),
                )
                .map_err(io::Error::from)?;
            } else {
                // `linkat` is the portable no-clobber publication primitive. A plain `renameat`
                // would silently replace a document that belongs to another conversation attempt.
                linkat(
                    &directory,
                    temporary_name.as_str(),
                    &directory,
                    Path::new(&file_name),
                    AtFlags::empty(),
                )
                .map_err(io::Error::from)?;
                unlinkat(
                    &directory,
                    temporary_name.as_str(),
                    UnlinkatFlags::NoRemoveDir,
                )
                .map_err(io::Error::from)?;
            }
            fsync(&directory).map_err(io::Error::from)
        })();

        // Po udanym `renameat` nazwa tymczasowa już nie istnieje. Po błędzie sprzątamy wyłącznie
        // losową nazwę, którą sami utworzyliśmy, nadal względem tego samego deskryptora katalogu.
        let _ = unlinkat(
            &directory,
            temporary_name.as_str(),
            UnlinkatFlags::NoRemoveDir,
        );
        written
    }

    #[cfg(windows)]
    pub fn publish(self, _bytes: &[u8], _replace: bool) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "private no-follow file publication is not implemented on Windows",
        ))
    }
}

#[cfg(unix)]
fn private_parent(
    anchor: &Path,
    relative: &Path,
) -> io::Result<(std::os::fd::OwnedFd, std::ffi::OsString)> {
    use nix::fcntl::{OFlag, open, openat};
    use nix::sys::stat::Mode;

    let parts = relative
        .components()
        .map(|part| match part {
            Component::Normal(name) => Ok(name.to_owned()),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a private evidence path is not relative and plain",
            )),
        })
        .collect::<io::Result<Vec<_>>>()?;
    let (file_name, parents) = parts
        .split_last()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty private file path"))?;

    let directory_flags =
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    // 2026-08-21: `open(anchor, O_NOFOLLOW)` chroni tylko ostatni komponent. Evidence ma
    // absolutny korzeń workspace'u, więc zaczynamy od `/` i otwieramy KAŻDY katalog przez
    // poprzedni deskryptor. Symlink pośrodku `anchor` nie może przekierować prywatnych bajtów
    // do sąsiedniego projektu.
    let anchor = private_anchor_path(anchor);
    let mut anchor_parts = anchor.components();
    if !matches!(anchor_parts.next(), Some(Component::RootDir)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a private evidence anchor is not absolute",
        ));
    }
    let mut directory =
        open(Path::new("/"), directory_flags, Mode::empty()).map_err(io::Error::from)?;
    for component in anchor_parts {
        let Component::Normal(parent) = component else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a private evidence anchor is not plain",
            ));
        };
        directory = openat(
            &directory,
            Path::new(parent),
            directory_flags,
            Mode::empty(),
        )
        .map_err(io::Error::from)?;
    }
    for parent in parents {
        directory = openat(
            &directory,
            Path::new(parent),
            directory_flags,
            Mode::empty(),
        )
        .map_err(io::Error::from)?;
    }
    Ok((directory, file_name.clone()))
}

#[cfg(target_os = "macos")]
fn private_anchor_path(anchor: &Path) -> std::path::PathBuf {
    // 2026-08-21: macOS dostarcza `/var` i `/tmp` jako stałe aliasy do `/private/*`.
    // TempDir i część workspace'ów przychodzą w tej publicznej pisowni. Normalizujemy tylko
    // te dwa systemowe aliasy, zanim zaczniemy deskryptorowy spacer; ogólne `canonicalize`
    // zaakceptowałoby natomiast dowolny symlink zasadzony przez workspace.
    for (alias, real) in [("/var", "/private/var"), ("/tmp", "/private/tmp")] {
        if let Ok(suffix) = anchor.strip_prefix(alias) {
            return Path::new(real).join(suffix);
        }
    }
    anchor.to_path_buf()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn private_anchor_path(anchor: &Path) -> std::path::PathBuf {
    anchor.to_path_buf()
}

#[cfg(unix)]
fn validate_private_fd(file: &impl std::os::fd::AsFd) -> io::Result<()> {
    use nix::sys::stat::{SFlag, fstat};
    use nix::unistd::geteuid;

    let stat = fstat(file).map_err(io::Error::from)?;
    let kind = SFlag::from_bits_truncate(stat.st_mode);
    if !kind.contains(SFlag::S_IFREG)
        || stat.st_mode & 0o777 != 0o600
        || stat.st_uid != geteuid().as_raw()
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "an existing private evidence file is not owner-only",
        ));
    }
    Ok(())
}

#[cfg(windows)]
pub fn open_private_file(
    _anchor: &Path,
    _relative: &Path,
    _access: PrivateFileAccess,
) -> io::Result<std::fs::File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "private no-follow file opening is not implemented on Windows",
    ))
}

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

    /// Odebrany strumień skarg, czekający na tego, kto go czyta. Oddawany raz.
    ///
    /// 2026-08-18 — TEGO POLA TU NIE BYŁO, choć `spawn` od pierwszego dnia ustawiał
    /// `command.stderr(Stdio::piped())`. Potok więc istniał i **nie dawał się odebrać**:
    /// uchwyt zostawał w dziecku, nikt go nie czytał, a krok padał zdaniem „The agent stopped
    /// without ever sending its result." — bez ani jednego słowa o przyczynie. Dwa skutki, oba
    /// zmierzone: (1) najczęstsza realna awaria — brak albo niezalogowane CLI — była
    /// niediagnozowalna z okna, a `which claude` na tej maszynie wskazuje wrapper, który przy
    /// braku binarki pisze WŁAŚNIE na stderr i wychodzi 127; (2) potok o pojemności ~64 KB,
    /// którego nikt nie opróżnia, blokuje dziecko na `write` — czyli agent gadatliwy na stderr
    /// wisiał, a wyglądało to jak agent, który myśli.
    stderr: Option<ChildStderr>,

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

    /// Odbiera strumień skarg. `None` przy drugim wywołaniu — dokładnie jak [`Supervised::stdout`].
    ///
    /// Ten, kto go weźmie, **musi go opróżniać do EOF**, a nie porzucić: porzucony uchwyt
    /// zamyka potok i dziecko dostaje `EPIPE` na pierwszym ostrzeżeniu, a nieopróżniany —
    /// blokuje je na pełnym buforze. Oba warianty są cichsze niż brak potoku.
    pub fn stderr(&mut self) -> Option<ChildStderr> {
        self.stderr.take()
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
pub fn spawn(command: Command, stdin: StdinPlan) -> io::Result<Supervised> {
    spawn_with_environment(command, stdin, &[])
}

/// Wariant dla jawnie zatwierdzonych Connections. Nazwy i wartości są rozstrzygnięte przez
/// backend tuż przed startem; wartości nie trafiają do argv, pliku ani webviewa.
pub fn spawn_with_environment(
    mut command: Command,
    stdin: StdinPlan,
    environment: &[(String, OsString)],
) -> io::Result<Supervised> {
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
    for (name, value) in environment {
        command.env(name, value);
    }

    let mut wrapped = into_own_group(command);
    let mut child: Box<dyn ChildWrapper> = wrapped.spawn()?;

    let Some(pid) = child.id() else {
        return Err(io::Error::other(
            "the child was gone before it could report a pid",
        ));
    };
    let pid = i32::try_from(pid).map_err(io::Error::other)?;

    // Potoki odbieramy od razu: uchwyt, który je w sobie trzyma, jest uchwytem, z którego T-05
    // nie przeczyta ani linii — a EOF na tym potoku ma osobne kryterium.
    let stdout = child.stdout().take();
    // Ten sam powód dla skarg, plus drugi: potok, którego nikt nie odbierze i nie opróżni,
    // zatrzymuje dziecko na `write` przy ~64 KB (powód w całości przy polu `stderr`).
    let stderr = child.stderr().take();

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
        stderr,
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

/// Zakłada dowiązanie symboliczne `at` wskazujące na `target`.
///
/// **Po co to jest TUTAJ, a nie tam, gdzie jest wołane.** `std::os::unix::fs::symlink` jest
/// kodem zależnym od platformy, a niezmiennik 3 daje takiemu kodowi dokładnie jeden dom: ten
/// plik. Wołający (`commands::isolate`) odtwarza dowiązania, kiedy robi krokowi kopię folderu,
/// który repozytorium nie jest — a dowiązanie skopiowane jako jego CEL wciąga do kopii każdego
/// kroku cały katalog po drugiej stronie (zmierzone 2026-08-19: drugie repozytorium).
///
/// Dzień, w którym powstanie gałąź windowsowa, jest dniem, w którym dopisuje się ją obok —
/// w tym pliku, razem z resztą decyzji platformowych, a nie w pięciu miejscach naraz.
pub fn link(target: &std::path::Path, at: &std::path::Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, at)
}

/// Własna grupa procesów Loadouta.
///
/// `0` w `killpg` znaczy „moja własna grupa", a wiersz z `pgid` równym tej wartości to my sami.
/// Odzyskiwanie po awarii porównuje z tym każdy zapisany `pgid` i to jest jego DRUGI strażnik
/// (pierwszym jest czas startu maszyny) — bez niego sprzątanie po poprzednim uruchomieniu
/// zabijałoby okno, które właśnie wstało.
///
/// Wywołanie systemowe stoi TUTAJ, a nie w `lib.rs`, i nie jest to kwestia porządku:
/// niezmiennik 3 mówi, że kod zależny od platformy mieszka wyłącznie w tym pliku, a
/// `checks/quick-boundary.sh` czyta to gerpem. Zmierzone 2026-08-17 — pierwsza wersja
/// odzyskiwania miała `libc::getpgrp()` w `lib.rs` i bramka słusznie zapaliła.
#[must_use]
pub fn own_process_group() -> i32 {
    // Bez argumentów, bez wskaźników, bez stanu: `getpgrp()` oddaje liczbę i nie może zawieść.
    #[allow(unsafe_code)]
    unsafe {
        libc::getpgrp()
    }
}

/// Kiedy ta maszyna ostatnio wstała, jako napis nadający się do zapisania w bazie.
///
/// DLACZEGO TO W OGÓLE ISTNIEJE — i to nie jest ciekawostka diagnostyczna. `kern.maxproc` na
/// macOS wynosi 16 000, więc PID-y przewijają się w godzinach, nie w latach. Po restarcie
/// maszyny `pgid` zapisany wczoraj z dużym prawdopodobieństwem należy do czegoś zupełnie
/// niewinnego, a `killpg` po nim jest błędem POPRAWNOŚCI, nie ryzykiem teoretycznym
/// [T7 ryzyko 2]. Odzyskiwanie po awarii porównuje tę wartość z tą zapisaną przy biegu
/// (`recovery::RecoveryRow::run_boot_id`) i strzela dopiero, gdy obie mówią o tym samym
/// uruchomieniu systemu.
///
/// Wołanie systemowe stoi TUTAJ, bo `recovery.rs` nie ma prawa go znać (niezmiennik 3):
/// tamten plik ma być czystą funkcją decyzji, dającą się przetestować bez maszyny.
///
/// `sysctl` przez podproces, nie przez `libc::sysctl`: ta skrzynia jest tu „tylko po stałe
/// sygnałów" (`Cargo.toml`), a odczyt raz na uruchomienie aplikacji nie jest miejscem, w którym
/// opłaca się kupować `unsafe` i strukturę `timeval` z ręki.
///
/// `None` znaczy „nie wiadomo" i jest odpowiedzią, nie awarią — brak strażnika ma wtedy
/// wstrzymać strzał, a nie go przepuścić (patrz `recovery::NO_BOOT_TIME`).
#[must_use]
pub fn machine_booted_at() -> Option<String> {
    let out = std::process::Command::new("sysctl")
        .args(["-n", "kern.boottime"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let said = String::from_utf8_lossy(&out.stdout);
    // `{ sec = 1755381234, usec = 123456 } Sun Aug 17 ...` — bierzemy SEKUNDY, bo reszta tej
    // linii to ta sama chwila zapisana po ludzku i zmienia się z lokalizacją systemu.
    let sec = said.split("sec = ").nth(1)?.split(&[',', ' '][..]).next()?;
    (!sec.is_empty() && sec.chars().all(|c| c.is_ascii_digit())).then(|| sec.to_owned())
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
/// # Decyzja: `libc::killpg` (2026-08-17)
///
/// Wybrana pierwsza z trzech dróg wypisanych wyżej. Powód jest jeden i wynika z niezmiennika 6:
/// **tylko ona daje dokładny `errno`**, a bez rozróżnienia `ESRCH` od `EPERM` nie da się
/// powiedzieć „nie żyje" uczciwie. `kill(1)` odróżnia te dwa stany wyłącznie brzmieniem zdania
/// na stderr — czyli dowód śmierci zależałby od języka systemu. Druga zależność (`nix`) daje
/// to samo co `libc`, którego już mamy, za cenę kolejnej skrzyni w drzewie.
///
/// `unsafe` jest tu jednym wyrażeniem i nie ma w nim wskaźników: `killpg` bierze dwa `i32`
/// i oddaje `i32`. Wyjątek od `unsafe_code = "deny"` stoi w `checks/suppressions-allowlist.json`
/// z pisemnym powodem, czyli przeszedł drogą, którą repo na to przewidziało.
///
/// Tak jak [`Supervised::stop`], sprzątanie prowadzi `SIGTERM`, daje grupie pełne okno łaski,
/// a dopiero potem eskaluje do `SIGKILL`. Brak uchwytu dziecka zmienia sposób czekania, nie
/// politykę: synchroniczna pętla pyta jądro sygnałem zerowym i ma te same jawne sufity.
#[must_use]
pub fn reap_group(pgid: i32) -> GroupProof {
    match signal_group(pgid, SIGNAL_TERM) {
        Ok(()) => {}
        Err(error) if error.raw_os_error() == Some(NO_SUCH_GROUP) => {
            return GroupProof::Dead { status: None };
        }
        // 2026-08-27: `EPERM` może oznaczać PGID przewinięty do cudzej grupy. Bez wysłanego
        // TERM-u nie mamy prawa próbować mocniejszego sygnału na tej samej liczbie.
        Err(_) => return GroupProof::Alive,
    }

    match wait_for_group_to_disappear(pgid, DEFAULT_GRACE) {
        Ok(true) => return GroupProof::Dead { status: None },
        // Każdy błąd inny niż `ESRCH` przerywa eskalację. Szczególnie `EPERM` między sondami
        // może znaczyć, że stara grupa zniknęła, a jej numer dostał już cudzy proces.
        Err(_) => return GroupProof::Alive,
        Ok(false) => {}
    }

    match signal_group(pgid, SIGNAL_KILL) {
        Ok(()) => {}
        Err(error) if error.raw_os_error() == Some(NO_SUCH_GROUP) => {
            return GroupProof::Dead { status: None };
        }
        Err(_) => return GroupProof::Alive,
    }

    match wait_for_group_to_disappear(pgid, PROOF_AFTER_KILL) {
        Ok(true) => GroupProof::Dead { status: None },
        Ok(false) | Err(_) => GroupProof::Alive,
    }
}

/// Wysyła sygnał do całej grupy i zachowuje dokładny `errno` dla decyzji dowodowej wyżej.
fn signal_group(pgid: i32, signal: i32) -> io::Result<()> {
    #[allow(unsafe_code)]
    let sent = unsafe { libc::killpg(pgid, signal) };
    if sent == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Czeka najwyżej `limit`, aż sygnał zerowy zwróci `ESRCH`.
///
/// `Ok(false)` znaczy wyłącznie „grupa odpowiadała przez cały limit". Inny błąd sondy jest
/// zachowany jako `Err`, żeby wołający nie pomylił odmowy uprawnień z prawem do eskalacji.
fn wait_for_group_to_disappear(pgid: i32, limit: Duration) -> io::Result<bool> {
    let began = Instant::now();
    loop {
        match signal_group(pgid, 0) {
            Ok(()) => {}
            Err(error) if error.raw_os_error() == Some(NO_SUCH_GROUP) => return Ok(true),
            Err(error) => return Err(error),
        }

        let elapsed = began.elapsed();
        if elapsed >= limit {
            return Ok(false);
        }
        std::thread::sleep(PROOF_POLL.min(limit.saturating_sub(elapsed)));
    }
}
