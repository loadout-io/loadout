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

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};

use process_wrap::tokio::{ChildWrapper, CommandWrap};
use tokio::io::AsyncWriteExt;
use tokio::process::{ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::oneshot;
use tokio::time::{sleep, timeout};

/// Ile bajtów zostało na tym systemie plików, dla użytkownika, który o to pyta.
///
/// TUTAJ, bo `statvfs` jest pytaniem do systemu plików, a niezmiennik 3 pozwala na kod zależny
/// od platformy wyłącznie w tym pliku. Port na Windows podmienia jedno ciało, nie szuka
/// wywołania rozsianego po `commands/`.
///
/// `rustix`, nie `libc` — bo `unsafe_code = "deny"` stoi w `Cargo.toml` workspace'u i jest tam
/// z premedytacją. `rustix` leży już w `Cargo.lock` jako zależność przechodnia, więc bezpieczne
/// opakowanie kosztuje jeden wiersz w manifeście, a nie nowe drzewo zależności.
///
/// `f_bavail`, nie `f_bfree`: różnica to rezerwa roota, której zwykły proces i tak nie dostanie,
/// więc liczenie jej dałoby próg przepuszczający bieg na dysku bez miejsca (2026-08-29, T-208).
///
/// # Errors
///
/// Kiedy system plików nie odpowie — na przykład gdy ścieżki nie ma.
pub fn free_bytes(path: &Path) -> io::Result<u64> {
    let stats = rustix::fs::statvfs(path)?;
    Ok(stats.f_frsize.saturating_mul(stats.f_bavail))
}

pub use crate::durable_file::PrivateFilePublisher;

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

/// Tożsamość inode'u utrzymana obok deskryptora. Rdzeń używa jej wyłącznie do porównania,
/// czy nazwa nadal wskazuje dokładnie ten plik lub katalog, który został zwalidowany.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PublicationIdentity {
    device: u64,
    inode: u64,
}

/// Rodzaj wpisu zwrócony przez descriptor-relative listing. Symlink pozostaje osobnym faktem,
/// więc caller nie musi otwierać go, aby zdecydować, że go pominie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PublicationEntryKind {
    Regular,
    Directory,
    Symlink,
    Other,
}

/// Płaski wpis katalogu względem zakotwiczonego roota.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PublicationEntry {
    pub(crate) name: OsString,
    pub(crate) kind: PublicationEntryKind,
}

/// Otwarty, niesymlinkowany root publikacji. Wszystkie późniejsze operacje pozostają względem
/// tego deskryptora, nawet jeśli ktoś podmieni nazwę roota w trakcie recovery.
pub(crate) struct PublicationRoot {
    #[cfg(unix)]
    directory: std::os::fd::OwnedFd,
    identity: PublicationIdentity,
}

impl fmt::Debug for PublicationRoot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PublicationRoot")
            .field("directory", &"<held descriptor>")
            .field("identity", &self.identity)
            .finish()
    }
}

impl PublicationRoot {
    #[cfg(unix)]
    pub(crate) fn open(path: &Path) -> io::Result<Self> {
        let directory = private_directory(path)?;
        let identity = identity_of(&directory)?;
        Ok(Self {
            directory,
            identity,
        })
    }

    #[cfg(windows)]
    pub(crate) fn open(_path: &Path) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no-follow publication roots are not implemented on Windows",
        ))
    }

    pub(crate) const fn identity(&self) -> PublicationIdentity {
        self.identity
    }

    #[cfg(unix)]
    pub(crate) fn target(&self, relative: &Path) -> io::Result<PublicationTarget> {
        let (directory, file_name) = relative_parent(&self.directory, relative)?;
        Ok(PublicationTarget {
            directory,
            file_name,
        })
    }

    #[cfg(windows)]
    pub(crate) fn target(&self, _relative: &Path) -> io::Result<PublicationTarget> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no-follow publication targets are not implemented on Windows",
        ))
    }

    /// Dowodzi, że nazwa roota nadal prowadzi do utrzymanego katalogu. Błąd otwarcia (w tym
    /// symlink) jest odmową, a nie fałszywym `false`, którego caller mógłby zignorować.
    pub(crate) fn validate_path_identity(&self, path: &Path) -> io::Result<()> {
        let current = Self::open(path)?;
        if current.identity != self.identity {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the publication root changed while it was in use",
            ));
        }
        Ok(())
    }

    /// Usuwa tylko regularne pliki bezpośrednio z wybranego katalogu. Recovery nie spaceruje
    /// rekurencyjnie: szerszy anchor nie może sprzątnąć aktywnego tempu writera z własnym
    /// lifecycle niżej w drzewie.
    #[cfg(unix)]
    pub(crate) fn remove_matching_files_in(
        &self,
        relative: &Path,
        mut matches: impl FnMut(&Path) -> bool,
    ) -> io::Result<()> {
        let directory = self.open_directory(relative)?;
        let directory = nix::dir::Dir::from_fd(directory).map_err(io::Error::from)?;
        remove_matching_from(directory, relative, &mut matches)
    }

    #[cfg(windows)]
    pub(crate) fn remove_matching_files_in(
        &self,
        _relative: &Path,
        _matches: impl FnMut(&Path) -> bool,
    ) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "descriptor-relative recovery is not implemented on Windows",
        ))
    }

    /// Tworzy brakujące katalogi pod utrzymanym rootem. Istniejący symlink lub plik odmawia,
    /// a każdy nowy wpis katalogowy jest synchronizowany przed przejściem głębiej.
    #[cfg(unix)]
    pub(crate) fn ensure_directory(&self, relative: &Path, mode: u32) -> io::Result<()> {
        use nix::errno::Errno;
        use nix::fcntl::openat;
        use nix::sys::stat::{Mode, mkdirat};

        let requested_mode = nix::sys::stat::mode_t::try_from(mode).map_err(|_error| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "the requested publication-directory mode is out of range",
            )
        })?;
        let requested_mode = Mode::from_bits_truncate(requested_mode);
        let mut directory = nix::unistd::dup(&self.directory).map_err(io::Error::from)?;
        let flags = directory_flags();
        for component in plain_parts(relative, true)? {
            let opened = openat(&directory, Path::new(&component), flags, Mode::empty());
            directory = match opened {
                Ok(opened) => opened,
                Err(Errno::ENOENT) => {
                    let created = match mkdirat(&directory, Path::new(&component), requested_mode) {
                        Ok(()) => true,
                        // Dwa shared writery mogą zobaczyć ENOENT przed tym samym mkdirat.
                        // EEXIST nie jest sukcesem ścieżki: ponowny openat niżej nadal odmawia
                        // symlinka i zwykłego pliku, a zwycięzca ustawia docelowy tryb.
                        Err(Errno::EEXIST) => false,
                        Err(error) => return Err(io::Error::from(error)),
                    };
                    if created {
                        nix::unistd::fsync(&directory).map_err(io::Error::from)?;
                    }
                    openat(&directory, Path::new(&component), flags, Mode::empty())
                        .map_err(io::Error::from)?
                }
                Err(error) => return Err(io::Error::from(error)),
            };
            enforce_owned_directory_mode(&directory, requested_mode)?;
        }
        Ok(())
    }

    #[cfg(windows)]
    pub(crate) fn ensure_directory(&self, _relative: &Path, _mode: u32) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "descriptor-relative directory creation is not implemented on Windows",
        ))
    }

    /// Zwraca wyłącznie bezpośrednie wpisy katalogu, nigdy ich cele.
    #[cfg(unix)]
    pub(crate) fn list_directory(&self, relative: &Path) -> io::Result<Vec<PublicationEntry>> {
        let directory = self.open_directory(relative)?;
        directory_entries(directory)
    }

    #[cfg(windows)]
    pub(crate) fn list_directory(&self, _relative: &Path) -> io::Result<Vec<PublicationEntry>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "descriptor-relative directory listing is not implemented on Windows",
        ))
    }

    /// Czyta regularny leaf bez śledzenia symlinków. Prywatny wariant dodatkowo egzekwuje
    /// bieżącego właściciela i dokładne `0600` na otwartym deskryptorze.
    #[cfg(unix)]
    pub(crate) fn read_regular(&self, relative: &Path, private: bool) -> io::Result<Vec<u8>> {
        use std::io::Read as _;

        use nix::fcntl::{OFlag, openat};
        use nix::sys::stat::Mode;

        let target = self.target(relative)?;
        let opened = openat(
            &target.directory,
            Path::new(&target.file_name),
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(io::Error::from)?;
        if private {
            validate_private_fd(&opened)?;
        } else {
            validate_regular_fd(&opened)?;
        }
        let mut file = std::fs::File::from(opened);
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    #[cfg(windows)]
    pub(crate) fn read_regular(&self, _relative: &Path, _private: bool) -> io::Result<Vec<u8>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "descriptor-relative reads are not implemented on Windows",
        ))
    }

    /// Sprawdza typ leaf bez otwierania jego celu. Symlink celowo odpowiada `false`.
    #[cfg(unix)]
    pub(crate) fn regular_file_exists(&self, relative: &Path) -> io::Result<bool> {
        self.target(relative)?.is_regular()
    }

    #[cfg(windows)]
    pub(crate) fn regular_file_exists(&self, _relative: &Path) -> io::Result<bool> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "descriptor-relative metadata is not implemented on Windows",
        ))
    }

    /// Usuwa dokładnie regularny leaf i synchronizuje jego utrzymany katalog. Brak albo
    /// symlink nie są usuwane i zwracają `false`.
    #[cfg(unix)]
    pub(crate) fn remove_regular_file(&self, relative: &Path) -> io::Result<bool> {
        use nix::unistd::{UnlinkatFlags, unlinkat};

        let target = self.target(relative)?;
        if !target.is_regular()? {
            return Ok(false);
        }
        unlinkat(
            &target.directory,
            Path::new(&target.file_name),
            UnlinkatFlags::NoRemoveDir,
        )
        .map_err(io::Error::from)?;
        target.sync_directory()?;
        Ok(true)
    }

    #[cfg(windows)]
    pub(crate) fn remove_regular_file(&self, _relative: &Path) -> io::Result<bool> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "descriptor-relative removal is not implemented on Windows",
        ))
    }

    /// Usuwa regularny leaf po ostatniej kontroli, że nazwa wskazuje inode przejęty przez caller.
    /// Kontrola i `unlinkat` używają tego samego utrzymanego deskryptora parenta, więc rollback nie
    /// podąża za podmienionym parentem ani symlinkiem. POSIX nie daje compare-and-unlink dla nazwy:
    /// caller nadal musi serializować własnych writerów; obcy proces może trafić między kontrolę
    /// i `unlinkat`, tak samo jak w udokumentowanej granicy compare-and-rename durable publishera.
    #[cfg(unix)]
    pub(crate) fn remove_regular_file_if_identity(
        &self,
        relative: &Path,
        expected: PublicationIdentity,
    ) -> io::Result<bool> {
        use nix::unistd::{UnlinkatFlags, unlinkat};

        let target = self.target(relative)?;
        if target.regular_target_identity()? != Some(expected) {
            return Ok(false);
        }
        unlinkat(
            &target.directory,
            Path::new(&target.file_name),
            UnlinkatFlags::NoRemoveDir,
        )
        .map_err(io::Error::from)?;
        target.sync_directory()?;
        Ok(true)
    }

    #[cfg(windows)]
    pub(crate) fn remove_regular_file_if_identity(
        &self,
        _relative: &Path,
        _expected: PublicationIdentity,
    ) -> io::Result<bool> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "identity-bound descriptor-relative removal is not implemented on Windows",
        ))
    }

    #[cfg(unix)]
    fn open_directory(&self, relative: &Path) -> io::Result<std::os::fd::OwnedFd> {
        let mut directory = nix::unistd::dup(&self.directory).map_err(io::Error::from)?;
        for component in plain_parts(relative, true)? {
            directory = nix::fcntl::openat(
                &directory,
                Path::new(&component),
                directory_flags(),
                nix::sys::stat::Mode::empty(),
            )
            .map_err(io::Error::from)?;
        }
        Ok(directory)
    }
}

/// Platformowy uchwyt wspólnego publishera. Polityka kolejności, praw i fault pointów mieszka
/// w `durable_file`; tutaj zostają tylko operacje `*at` (niezmiennik 3).
pub(crate) struct PublicationTarget {
    #[cfg(unix)]
    directory: std::os::fd::OwnedFd,
    #[cfg(unix)]
    file_name: std::ffi::OsString,
}

impl fmt::Debug for PublicationTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PublicationTarget")
            .field("directory_held", &true)
            .finish_non_exhaustive()
    }
}

impl PublicationTarget {
    #[cfg(unix)]
    pub(crate) fn parent_identity(&self) -> io::Result<PublicationIdentity> {
        identity_of(&self.directory)
    }

    #[cfg(windows)]
    pub(crate) fn parent_identity(&self) -> io::Result<PublicationIdentity> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "publication parent identity is not implemented on Windows",
        ))
    }

    #[cfg(unix)]
    pub(crate) fn target_mode(&self) -> io::Result<Option<u32>> {
        use nix::errno::Errno;
        use nix::fcntl::AtFlags;
        use nix::sys::stat::{SFlag, fstatat};

        match fstatat(
            &self.directory,
            Path::new(&self.file_name),
            AtFlags::AT_SYMLINK_NOFOLLOW,
        ) {
            Ok(stat) => {
                let kind = SFlag::from_bits_truncate(stat.st_mode);
                if kind != SFlag::S_IFREG {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "the publication target is not a regular file",
                    ));
                }
                Ok(Some(u32::from(stat.st_mode & 0o777)))
            }
            Err(Errno::ENOENT) => Ok(None),
            Err(error) => Err(io::Error::from(error)),
        }
    }

    /// Otwiera istniejący prywatny leaf, waliduje jego prawa i zachowuje tożsamość inode'u.
    /// `ENOENT` jest osobnym wynikiem, bo create-if-absent nie potrzebuje istniejącego celu.
    #[cfg(unix)]
    pub(crate) fn private_target_identity(&self) -> io::Result<Option<PublicationIdentity>> {
        use nix::errno::Errno;
        use nix::fcntl::{OFlag, openat};
        use nix::sys::stat::Mode;

        let opened = match openat(
            &self.directory,
            Path::new(&self.file_name),
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        ) {
            Ok(opened) => opened,
            Err(Errno::ENOENT) => return Ok(None),
            Err(error) => return Err(io::Error::from(error)),
        };
        validate_private_fd(&opened)?;
        Ok(Some(identity_of(&opened)?))
    }

    #[cfg(windows)]
    pub(crate) fn private_target_identity(&self) -> io::Result<Option<PublicationIdentity>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "private target identity is not implemented on Windows",
        ))
    }

    /// Otwiera zwykły regularny leaf bez śledzenia symlinków i zwraca jego tożsamość. W
    /// przeciwieństwie do `private_target_identity` nie narzuca trybu 0600, bo historyczne
    /// receipts zachowują domyślne prawa plików użytkownika.
    #[cfg(unix)]
    pub(crate) fn regular_target_identity(&self) -> io::Result<Option<PublicationIdentity>> {
        use nix::errno::Errno;
        use nix::fcntl::{OFlag, openat};
        use nix::sys::stat::Mode;

        let opened = match openat(
            &self.directory,
            Path::new(&self.file_name),
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        ) {
            Ok(opened) => opened,
            Err(Errno::ENOENT) => return Ok(None),
            Err(error) => return Err(io::Error::from(error)),
        };
        validate_regular_fd(&opened)?;
        Ok(Some(identity_of(&opened)?))
    }

    #[cfg(windows)]
    pub(crate) fn regular_target_identity(&self) -> io::Result<Option<PublicationIdentity>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "regular target identity is not implemented on Windows",
        ))
    }

    /// Czy bieżąca nazwa nadal wskazuje zwalidowany wcześniej inode.
    pub(crate) fn has_identity(&self, expected: PublicationIdentity) -> io::Result<bool> {
        Ok(self.private_target_identity()? == Some(expected))
    }

    /// Stary `<target>.writing` jest terminalnym dowodem niedokończonej publikacji evidence.
    /// Sama obecność dowolnego wpisu blokuje nowy sukces; nie otwieramy ani nie kasujemy guarda.
    #[cfg(unix)]
    pub(crate) fn has_writing_guard(&self) -> io::Result<bool> {
        use nix::errno::Errno;
        use nix::fcntl::AtFlags;
        use nix::sys::stat::fstatat;

        let mut guard_name = self.file_name.clone();
        guard_name.push(".writing");
        match fstatat(
            &self.directory,
            Path::new(&guard_name),
            AtFlags::AT_SYMLINK_NOFOLLOW,
        ) {
            Ok(_) => Ok(true),
            Err(Errno::ENOENT) => Ok(false),
            Err(error) => Err(io::Error::from(error)),
        }
    }

    #[cfg(windows)]
    pub(crate) fn has_writing_guard(&self) -> io::Result<bool> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "private writing guards are not implemented on Windows",
        ))
    }

    #[cfg(unix)]
    fn is_regular(&self) -> io::Result<bool> {
        use nix::errno::Errno;
        use nix::fcntl::AtFlags;
        use nix::sys::stat::{SFlag, fstatat};

        match fstatat(
            &self.directory,
            Path::new(&self.file_name),
            AtFlags::AT_SYMLINK_NOFOLLOW,
        ) {
            Ok(stat) => Ok(SFlag::from_bits_truncate(stat.st_mode) == SFlag::S_IFREG),
            Err(Errno::ENOENT) => Ok(false),
            Err(error) => Err(io::Error::from(error)),
        }
    }

    #[cfg(windows)]
    pub(crate) fn target_mode(&self) -> io::Result<Option<u32>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no-follow file publication is not implemented on Windows",
        ))
    }

    #[cfg(unix)]
    pub(crate) fn create_temp(&self, name: &str, mode: u32) -> io::Result<std::fs::File> {
        use std::os::unix::fs::PermissionsExt as _;

        use nix::fcntl::{OFlag, openat};
        use nix::sys::stat::Mode;

        let platform_mode = mode.try_into().map_err(io::Error::other)?;
        let temporary = openat(
            &self.directory,
            name,
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::from_bits_truncate(platform_mode),
        )
        .map_err(io::Error::from)?;
        let file = std::fs::File::from(temporary);
        // `openat` respektuje umask. Jawne chmod na jeszcze nieopublikowanym inode sprawia,
        // że polityka definicji nie zależy od środowiska procesu.
        if let Err(error) = file.set_permissions(std::fs::Permissions::from_mode(mode)) {
            // 2026-08-28: temp istnieje już przed chmod. Caller nie może zaznaczyć ownershipu,
            // dopóki ta funkcja nie wróci, więc lokalny cleanup musi nastąpić właśnie tutaj.
            drop(file);
            let _removed = self.remove_temp(name);
            let _synced = self.sync_directory();
            return Err(error);
        }
        Ok(file)
    }

    #[cfg(windows)]
    pub(crate) fn create_temp(&self, _name: &str, _mode: u32) -> io::Result<std::fs::File> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no-follow file publication is not implemented on Windows",
        ))
    }

    #[cfg(unix)]
    pub(crate) fn commit_replace(&self, temporary_name: &str) -> io::Result<()> {
        use nix::fcntl::renameat;
        renameat(
            &self.directory,
            temporary_name,
            &self.directory,
            Path::new(&self.file_name),
        )
        .map_err(io::Error::from)
    }

    #[cfg(windows)]
    pub(crate) fn commit_replace(&self, _temporary_name: &str) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no-follow file publication is not implemented on Windows",
        ))
    }

    #[cfg(unix)]
    pub(crate) fn commit_create_if_absent(&self, temporary_name: &str) -> io::Result<()> {
        use nix::fcntl::AtFlags;
        use nix::unistd::linkat;
        linkat(
            &self.directory,
            temporary_name,
            &self.directory,
            Path::new(&self.file_name),
            AtFlags::empty(),
        )
        .map_err(io::Error::from)
    }

    #[cfg(windows)]
    pub(crate) fn commit_create_if_absent(&self, _temporary_name: &str) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no-follow file publication is not implemented on Windows",
        ))
    }

    #[cfg(unix)]
    pub(crate) fn remove_temp(&self, temporary_name: &str) -> io::Result<()> {
        use nix::errno::Errno;
        use nix::unistd::{UnlinkatFlags, unlinkat};
        match unlinkat(&self.directory, temporary_name, UnlinkatFlags::NoRemoveDir) {
            Ok(()) | Err(Errno::ENOENT) => Ok(()),
            Err(error) => Err(io::Error::from(error)),
        }
    }

    #[cfg(windows)]
    pub(crate) fn remove_temp(&self, _temporary_name: &str) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no-follow file publication is not implemented on Windows",
        ))
    }

    #[cfg(unix)]
    pub(crate) fn sync_directory(&self) -> io::Result<()> {
        nix::unistd::fsync(&self.directory).map_err(io::Error::from)
    }

    #[cfg(windows)]
    pub(crate) fn sync_directory(&self) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "directory sync is not implemented on Windows",
        ))
    }
}

/// Stabilny klucz wyłącznie dla współdzielenia lifecycle. Nie jest autoryzacją ścieżki:
/// każda operacja nadal otwiera root komponent po komponencie z `O_NOFOLLOW`.
pub(crate) fn publication_root_key(path: &Path) -> io::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let normalized = normalize_plain_absolute(&private_anchor_path(&absolute))?;
    match std::fs::canonicalize(&normalized) {
        Ok(canonical) => Ok(canonical),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(normalized),
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn private_directory(path: &Path) -> io::Result<std::os::fd::OwnedFd> {
    use nix::fcntl::{open, openat};
    use nix::sys::stat::Mode;

    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let normalized = normalize_plain_absolute(&private_anchor_path(&absolute))?;
    let mut components = normalized.components();
    if !matches!(components.next(), Some(Component::RootDir)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a publication root is not absolute",
        ));
    }
    let mut directory =
        open(Path::new("/"), directory_flags(), Mode::empty()).map_err(io::Error::from)?;
    for component in components {
        let Component::Normal(parent) = component else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a publication root is not plain",
            ));
        };
        directory = openat(
            &directory,
            Path::new(parent),
            directory_flags(),
            Mode::empty(),
        )
        .map_err(io::Error::from)?;
    }
    Ok(directory)
}

fn normalize_plain_absolute(path: &Path) -> io::Result<PathBuf> {
    let mut normalized = PathBuf::from("/");
    let mut components = path.components();
    if !matches!(components.next(), Some(Component::RootDir)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a publication root is not absolute",
        ));
    }
    for component in components {
        match component {
            Component::Normal(name) => normalized.push(name),
            Component::CurDir => {}
            Component::ParentDir => {
                if normalized.parent().is_none() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "a publication root escapes the filesystem root",
                    ));
                }
                normalized.pop();
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "a publication root is not plain",
                ));
            }
        }
    }
    Ok(normalized)
}

#[cfg(unix)]
fn relative_parent(
    root: &impl std::os::fd::AsFd,
    relative: &Path,
) -> io::Result<(std::os::fd::OwnedFd, OsString)> {
    use nix::fcntl::openat;
    use nix::sys::stat::Mode;

    let parts = plain_parts(relative, false)?;
    let (file_name, parents) = parts
        .split_last()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty private file path"))?;
    let mut directory = nix::unistd::dup(root).map_err(io::Error::from)?;
    for parent in parents {
        directory = openat(
            &directory,
            Path::new(parent),
            directory_flags(),
            Mode::empty(),
        )
        .map_err(io::Error::from)?;
    }
    Ok((directory, file_name.clone()))
}

fn plain_parts(relative: &Path, allow_empty: bool) -> io::Result<Vec<OsString>> {
    let parts = relative
        .components()
        .map(|part| match part {
            Component::Normal(name) => Ok(name.to_owned()),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a publication path is not relative and plain",
            )),
        })
        .collect::<io::Result<Vec<_>>>()?;
    if !allow_empty && parts.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a publication path is empty",
        ));
    }
    Ok(parts)
}

#[cfg(unix)]
fn directory_flags() -> nix::fcntl::OFlag {
    nix::fcntl::OFlag::O_RDONLY
        | nix::fcntl::OFlag::O_DIRECTORY
        | nix::fcntl::OFlag::O_NOFOLLOW
        | nix::fcntl::OFlag::O_CLOEXEC
}

#[cfg(unix)]
fn identity_of(file: &impl std::os::fd::AsFd) -> io::Result<PublicationIdentity> {
    let stat = nix::sys::stat::fstat(file).map_err(io::Error::from)?;
    Ok(PublicationIdentity {
        device: u64::try_from(stat.st_dev).map_err(|_error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "a publication device identifier was negative",
            )
        })?,
        inode: stat.st_ino as u64,
    })
}

/// Wiąże receipt publikacji z deskryptorem pliku tymczasowego jeszcze przed commit. Odczyt
/// identity po nazwie docelowej zostawiałby obcemu writerowi okno na podmianę między rename
/// a rejestracją rollbacku.
#[cfg(unix)]
pub(crate) fn publication_identity(file: &std::fs::File) -> io::Result<PublicationIdentity> {
    identity_of(file)
}

#[cfg(windows)]
pub(crate) fn publication_identity(_file: &std::fs::File) -> io::Result<PublicationIdentity> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "publication identity is not implemented on Windows",
    ))
}

#[cfg(unix)]
fn validate_regular_fd(file: &impl std::os::fd::AsFd) -> io::Result<()> {
    use nix::sys::stat::{SFlag, fstat};

    let stat = fstat(file).map_err(io::Error::from)?;
    if SFlag::from_bits_truncate(stat.st_mode) != SFlag::S_IFREG {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the publication leaf is not a regular file",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn directory_entries(directory: std::os::fd::OwnedFd) -> io::Result<Vec<PublicationEntry>> {
    use std::os::unix::ffi::OsStringExt as _;

    use nix::dir::Type;
    use nix::fcntl::AtFlags;
    use nix::sys::stat::{SFlag, fstatat};

    let mut directory = nix::dir::Dir::from_fd(directory).map_err(io::Error::from)?;
    let lookup = nix::unistd::dup(&directory).map_err(io::Error::from)?;
    let mut entries = Vec::new();
    for entry in directory.iter() {
        let entry = entry.map_err(io::Error::from)?;
        let name = entry.file_name().to_bytes();
        if name == b"." || name == b".." {
            continue;
        }
        let kind = match entry.file_type() {
            Some(Type::File) => PublicationEntryKind::Regular,
            Some(Type::Directory) => PublicationEntryKind::Directory,
            Some(Type::Symlink) => PublicationEntryKind::Symlink,
            Some(_) => PublicationEntryKind::Other,
            None => {
                let stat = fstatat(&lookup, entry.file_name(), AtFlags::AT_SYMLINK_NOFOLLOW)
                    .map_err(io::Error::from)?;
                let flags = SFlag::from_bits_truncate(stat.st_mode);
                if flags == SFlag::S_IFREG {
                    PublicationEntryKind::Regular
                } else if flags == SFlag::S_IFDIR {
                    PublicationEntryKind::Directory
                } else if flags == SFlag::S_IFLNK {
                    PublicationEntryKind::Symlink
                } else {
                    PublicationEntryKind::Other
                }
            }
        };
        entries.push(PublicationEntry {
            name: OsString::from_vec(name.to_vec()),
            kind,
        });
    }
    Ok(entries)
}

#[cfg(unix)]
fn remove_matching_from(
    mut directory: nix::dir::Dir,
    relative: &Path,
    matches: &mut impl FnMut(&Path) -> bool,
) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt as _;

    use nix::dir::Type;
    use nix::fcntl::AtFlags;
    use nix::sys::stat::{SFlag, fstatat};
    use nix::unistd::{UnlinkatFlags, unlinkat};

    let lookup = nix::unistd::dup(&directory).map_err(io::Error::from)?;
    let mut entries = Vec::new();
    for entry in directory.iter() {
        let entry = entry.map_err(io::Error::from)?;
        let name = entry.file_name().to_bytes();
        if name == b"." || name == b".." {
            continue;
        }
        let kind = match entry.file_type() {
            Some(Type::File) => PublicationEntryKind::Regular,
            Some(Type::Directory) => PublicationEntryKind::Directory,
            Some(Type::Symlink) => PublicationEntryKind::Symlink,
            Some(_) => PublicationEntryKind::Other,
            None => {
                let stat = fstatat(&lookup, entry.file_name(), AtFlags::AT_SYMLINK_NOFOLLOW)
                    .map_err(io::Error::from)?;
                let flags = SFlag::from_bits_truncate(stat.st_mode);
                if flags == SFlag::S_IFREG {
                    PublicationEntryKind::Regular
                } else if flags == SFlag::S_IFDIR {
                    PublicationEntryKind::Directory
                } else if flags == SFlag::S_IFLNK {
                    PublicationEntryKind::Symlink
                } else {
                    PublicationEntryKind::Other
                }
            }
        };
        entries.push((entry.file_name().to_owned(), kind));
    }

    let mut changed = false;
    for (name, kind) in entries {
        let part = OsStr::from_bytes(name.to_bytes());
        let child_relative = relative.join(part);
        match kind {
            PublicationEntryKind::Regular if matches(&child_relative) => {
                unlinkat(&directory, name.as_c_str(), UnlinkatFlags::NoRemoveDir)
                    .map_err(io::Error::from)?;
                changed = true;
            }
            PublicationEntryKind::Regular
            | PublicationEntryKind::Directory
            | PublicationEntryKind::Symlink
            | PublicationEntryKind::Other => {}
        }
    }
    if changed {
        nix::unistd::fsync(&directory).map_err(io::Error::from)?;
    }
    Ok(())
}

#[cfg(unix)]
fn private_parent(
    anchor: &Path,
    relative: &Path,
) -> io::Result<(std::os::fd::OwnedFd, std::ffi::OsString)> {
    let directory = private_directory(anchor)?;
    relative_parent(&directory, relative)
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

#[cfg(not(unix))]
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

#[cfg(unix)]
fn enforce_owned_directory_mode(
    directory: &impl std::os::fd::AsFd,
    requested: nix::sys::stat::Mode,
) -> io::Result<()> {
    use nix::sys::stat::{SFlag, fchmod, fstat};
    use nix::unistd::geteuid;

    let stat = fstat(directory).map_err(io::Error::from)?;
    if SFlag::from_bits_truncate(stat.st_mode) != SFlag::S_IFDIR
        || stat.st_uid != geteuid().as_raw()
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "a private publication directory is not owned by the current user",
        ));
    }
    let current = nix::sys::stat::Mode::from_bits_truncate(stat.st_mode);
    if current != requested {
        fchmod(directory, requested).map_err(io::Error::from)?;
        nix::unistd::fsync(directory).map_err(io::Error::from)?;
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

/// Dowód, że to program podany do [`Command`] nie istniał w chwili startu procesu.
///
/// `ENOENT` ze spawnu jest niejednoznaczne: ten sam kod wraca dla brakującego katalogu
/// roboczego. Payload powstaje wyłącznie przy hopie `spawn`, kiedy oba fakty są jeszcze
/// rozdzielne, żeby późniejsza warstwa produktu nie zgadywała po całym łańcuchu błędów.
#[derive(Debug)]
pub struct MissingProgram(OsString);

impl fmt::Display for MissingProgram {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "the configured program was not found: {}",
            self.0.to_string_lossy()
        )
    }
}

impl std::error::Error for MissingProgram {}

/// Zamrożone miejsca, w których aplikacja szuka CLI agentów przed zbudowaniem sterowników.
#[derive(Clone, Debug)]
pub struct AgentCliSearch {
    path: Option<OsString>,
    install_dirs: Vec<PathBuf>,
}

impl AgentCliSearch {
    /// Środowisko procesu aplikacji oraz platformowe katalogi instalacji.
    #[must_use]
    pub fn for_process() -> Self {
        Self {
            path: std::env::var_os("PATH"),
            install_dirs: platform_agent_cli_dirs(std::env::var_os("HOME").as_deref()),
        }
    }

    /// Jawny świat wyszukiwania używany przez kryterium bez mutowania globalnego środowiska.
    #[must_use]
    pub fn from_parts(path: Option<OsString>, install_dirs: Vec<PathBuf>) -> Self {
        Self { path, install_dirs }
    }

    /// Rozwiązuje nazwę CLI do pliku, który sterownik uruchomi bez pomocy powłoki.
    #[must_use]
    pub fn resolve(&self, name: &str) -> PathBuf {
        let from_path = self
            .path
            .as_deref()
            .into_iter()
            .flat_map(std::env::split_paths);
        for directory in from_path.chain(self.install_dirs.iter().cloned()) {
            let candidate = absolute_candidate(directory.join(name));
            if is_executable_file(&candidate) {
                return candidate;
            }
        }
        // Goła nazwa zachowuje zwykłą semantykę `Command` dla instalacji, których jeszcze nie
        // znamy. Nieudany spawn jest potem tłumaczony na zdanie o konkretnym CLI w żywej drodze
        // Run; wymyślona absolutna ścieżka byłaby fałszywą diagnostyką.
        PathBuf::from(name)
    }

    /// Ten sam świat wyszukiwania dla gołej nazwy, która zostaje po braku znanego kandydata.
    ///
    /// Fabryka zamraża go razem ze ścieżką binarki. Inaczej wstrzyknięty search rozstrzygałby
    /// ścieżki absolutne, ale fallback wracałby przy spawnie do środowiska procesu aplikacji.
    pub(crate) fn child_path(&self) -> Option<OsString> {
        let mut directories = self
            .path
            .as_deref()
            .into_iter()
            .flat_map(std::env::split_paths)
            .collect::<Vec<_>>();
        directories.extend(self.install_dirs.iter().cloned());
        std::env::join_paths(directories).ok()
    }
}

/// Katalogi, których `LaunchServices` nie dodaje do środowiska aplikacji GUI.
///
/// Kod platformowy stoi wyłącznie w supervisorze (niezmiennik 3). Kolejność jest częścią
/// polityki: `PATH` wygrywa wcześniej, potem stabilne linki Homebrew, na końcu instalacje
/// użytkownika. Nie wołamy login shella — jego pliki startowe mogą wykonywać arbitralny kod,
/// pisać na stdout albo wisieć, zanim człowiek uruchomi pierwszy krok.
#[cfg(target_os = "macos")]
#[must_use]
pub fn platform_agent_cli_dirs(home: Option<&std::ffi::OsStr>) -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ];
    if let Some(home) = home {
        let home = PathBuf::from(home);
        dirs.extend([
            home.join(".local/bin"),
            home.join(".npm-global/bin"),
            home.join(".bun/bin"),
            home.join(".volta/bin"),
        ]);
    }
    dirs
}

/// Typowe katalogi instalacji CLI poza macOS, bez uruchamiania powłoki logowania.
#[cfg(not(target_os = "macos"))]
#[must_use]
pub fn platform_agent_cli_dirs(home: Option<&std::ffi::OsStr>) -> Vec<PathBuf> {
    home.into_iter()
        .map(PathBuf::from)
        .flat_map(|home| {
            [
                home.join(".local/bin"),
                home.join(".npm-global/bin"),
                home.join(".bun/bin"),
                home.join(".volta/bin"),
            ]
        })
        .collect()
}

fn absolute_candidate(candidate: PathBuf) -> PathBuf {
    if candidate.is_absolute() {
        return candidate;
    }
    std::env::current_dir().map_or(candidate.clone(), |cwd| cwd.join(candidate))
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

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
///
/// `#[must_use]` na całym wyliczeniu, nie na pojedynczej funkcji (2026-08-28): dowód, który da
/// się porzucić instrukcją, jest dowodem opcjonalnym, a druga cicha wersja złamania niezmiennika
/// 6 wygląda dokładnie tak — `handle.stop(GRACE).await;` ze średnikiem czyta się jak „zatrzymaj",
/// kompiluje się i nie pyta nikogo o wynik. Kto naprawdę nie ma co zrobić z dowodem, pisze
/// `let _ = …` i to widać w diffie.
#[derive(Debug)]
#[must_use]
pub enum GroupProof {
    /// `kill(-pgid, 0)` zwrócił `ESRCH`: w grupie nie ma już **ani jednego** procesu — także
    /// żadnego zombie, bo zombie nadal odpowiada na sygnał zerowy. To jedyny stan, w którym
    /// wolno powiedzieć „nie żyje".
    ///
    /// **Powstaje na KAŻDEJ ścieżce terminalnej kroku**, nie tylko po Stopie i po limicie czasu
    /// (poprawka z 2026-08-28: do tego dnia ten nagłówek wymieniał tamte dwie i miał rację, bo
    /// tura, która skończyła się sama, nie pytała jądra o nic — `close()` zbiera lidera, a płaci
    /// się za wnuki [T7 §3.1]). Drogę udaną wołają dziś `AgentHandle::proof_of_death`
    /// i `Checking::cancel`.
    ///
    /// `status` niesie kod wyjścia lidera, jeśli to my go zebraliśmy — po nim poznaje się
    /// różnicę między czystym wyjściem po SIGTERM a sygnałem 9 po eskalacji. `None` przy
    /// powtórzonym zatrzymaniu tej samej grupy: status jest do odebrania raz, a drugie
    /// `stop()` nadal musi być bezbłędne.
    Dead { status: Option<ExitStatus> },

    /// Grupa nadal odpowiada na sygnał zerowy. To jest wynik do obsłużenia, nie błąd do
    /// zalogowania: osierocony `claude` pali limit w tle [T7 §10.1].
    ///
    /// # Dlaczego ten wariant NIE jest jednostkowy (2026-08-28)
    ///
    /// Bo `Alive` jest zdaniem o czymś, co **dalej istnieje**, a wołający musi mieć jak to coś
    /// zaadresować. Wariant jednostkowy czytał się jak „nie udało się" i nie niósł ani `pid`,
    /// ani `pgid` — więc nic w typie nie odróżniało go od porażki, po której wolno wszystko
    /// posprzątać. Adres w środku zmienia to w obowiązek: kto dostał `Alive`, ten wie, KOGO ma
    /// dalej pytać, i nie ma powodu porzucać uchwytu ani miejsca z puli.
    ///
    /// `None` jest stanem **gorszym** niż `Some`, nie brakiem znaczenia: grupa żyje, a my nie
    /// wiemy nawet, kogo zapytać. Powstaje w dokładnie jednym miejscu produkcji —
    /// `commands::chat::Conversation::stop`, kiedy kanał actora rozmowy urwał się, zanim
    /// ktokolwiek zapytał jądra — i jest tam zachowawczy z rozmysłu: utrata actora nie ma prawa
    /// zamienić się w fałszywy dowód śmierci.
    Alive { group: Option<GroupId> },
}

/// Neutralna operacja na grupie używana przez rdzeń startup reaper.
///
/// Nazwy POSIX-owych sygnałów pozostają w tym module (niezmiennik 3), a deterministyczny
/// standalone target może sterować odpowiedziami bez tworzenia prawdziwego procesu.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReapAction {
    Term,
    Probe,
    Kill,
}

/// Neutralna odpowiedź signalera, która nie wypuszcza platformowego `errno` poza ten moduł.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReapResponse {
    Delivered,
    NoSuchGroup,
    Refused,
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
        // obsłużenia przez wołającego, nie błąd do zalogowania: ktoś w tej grupie dalej biegnie
        // — i wraca razem z adresem, pod którym da się go dalej pytać.
        GroupProof::Alive {
            group: Some(self.group),
        }
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
    let program = command.as_std().get_program().to_os_string();
    let current_dir = command.as_std().get_current_dir().map(Path::to_path_buf);

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
    // Aplikacja z Docka nie ma katalogów Homebrew ani instalacji użytkownika w `PATH`, ale
    // samo CLI uruchamia kolejne programy (np. `node`). Zachowujemy kolejność odziedziczoną
    // od aplikacji i dopinamy te same platformowe miejsca, których używa odkrywanie CLI.
    let mut child_path = std::env::var_os("PATH")
        .as_deref()
        .into_iter()
        .flat_map(std::env::split_paths)
        .collect::<Vec<_>>();
    child_path.extend(platform_agent_cli_dirs(std::env::var_os("HOME").as_deref()));
    let child_path = std::env::join_paths(child_path)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    command.env("PATH", child_path);
    for (name, value) in environment {
        command.env(name, value);
    }

    let mut wrapped = into_own_group(command);
    let mut child: Box<dyn ChildWrapper> = match wrapped.spawn() {
        Ok(child) => child,
        Err(error)
            if error.kind() == io::ErrorKind::NotFound
                && current_dir.as_deref().is_none_or(Path::is_dir) =>
        {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                MissingProgram(program),
            ));
        }
        Err(error) => return Err(error),
    };

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
///
/// 2026-08-27 (T-147): ten szew jest publiczny wyłącznie dla standalone integration targetów.
/// Produkcyjny adapter i testy mają wykonywać ten sam rdzeń, ale tylko ten plik mapuje sygnały
/// i błędy platformy na neutralne wartości.
// `#[must_use]` stoi od 2026-08-28 na samym [`GroupProof`], więc powtórzony tutaj byłby drugą
// kopią tej samej reguły (clippy `double_must_use` mówi to samo).
///
/// Ten rdzeń nie zna adresu grupy — dostaje wyłącznie signaler — więc jego `Alive` wraca bez
/// niego, a dopisuje go [`reap_group`], czyli jedyny wołający, który ten adres ma (2026-08-28).
#[doc(hidden)]
pub fn reap_group_with_signaler(
    grace: Duration,
    proof_after_kill: Duration,
    mut signal: impl FnMut(ReapAction) -> ReapResponse,
) -> GroupProof {
    match signal(ReapAction::Term) {
        ReapResponse::Delivered => {}
        ReapResponse::NoSuchGroup => return GroupProof::Dead { status: None },
        ReapResponse::Refused => return GroupProof::Alive { group: None },
    }

    match wait_for_group_to_disappear(grace, &mut signal) {
        ReapWait::Gone => return GroupProof::Dead { status: None },
        ReapWait::Refused => return GroupProof::Alive { group: None },
        ReapWait::TimedOut => {}
    }

    match signal(ReapAction::Kill) {
        ReapResponse::Delivered => {}
        ReapResponse::NoSuchGroup => return GroupProof::Dead { status: None },
        ReapResponse::Refused => return GroupProof::Alive { group: None },
    }

    match wait_for_group_to_disappear(proof_after_kill, &mut signal) {
        ReapWait::Gone => GroupProof::Dead { status: None },
        ReapWait::Refused | ReapWait::TimedOut => GroupProof::Alive { group: None },
    }
}

pub fn reap_group(pgid: i32) -> GroupProof {
    let proof = reap_group_with_signaler(DEFAULT_GRACE, PROOF_AFTER_KILL, |action| {
        let platform_signal = match action {
            ReapAction::Term => SIGNAL_TERM,
            ReapAction::Probe => 0,
            ReapAction::Kill => SIGNAL_KILL,
        };
        match signal_group(pgid, platform_signal) {
            Ok(()) => ReapResponse::Delivered,
            Err(error) if error.raw_os_error() == Some(NO_SUCH_GROUP) => ReapResponse::NoSuchGroup,
            // 2026-08-27: szczególnie `EPERM` może oznaczać PGID przewinięty do cudzej
            // grupy. Rdzeń musi dostać odmowę, nie fałszywy dowód śmierci ani zgodę na KILL.
            Err(_) => ReapResponse::Refused,
        }
    });
    match proof {
        /* ADRES DOPISUJEMY TUTAJ, bo tylko tutaj jest znany. `pid` jest równy `pgid` nie przez
         * uproszczenie, tylko z definicji POSIX: identyfikatorem grupy JEST pid jej lidera, więc
         * to jest ta sama liczba, nawet kiedy lidera już nikt nie zbierze [T7 §6.2]. */
        GroupProof::Alive { .. } => GroupProof::Alive {
            group: Some(GroupId { pid: pgid, pgid }),
        },
        dead @ GroupProof::Dead { .. } => dead,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReapWait {
    Gone,
    Refused,
    TimedOut,
}

/// Czeka najwyżej `limit`, aż neutralna sonda zwróci brak grupy.
///
/// Pierwsza sonda zawsze poprzedza ocenę czasu. Dzięki temu zerowy limit nadal coś mierzy,
/// zamiast uznać upływ czasu za dowód albo zgodę na dalszą eskalację.
fn wait_for_group_to_disappear(
    limit: Duration,
    signal: &mut impl FnMut(ReapAction) -> ReapResponse,
) -> ReapWait {
    let began = Instant::now();
    loop {
        match signal(ReapAction::Probe) {
            ReapResponse::Delivered => {}
            ReapResponse::NoSuchGroup => return ReapWait::Gone,
            ReapResponse::Refused => return ReapWait::Refused,
        }

        let elapsed = began.elapsed();
        if elapsed >= limit {
            return ReapWait::TimedOut;
        }
        std::thread::sleep(PROOF_POLL.min(limit.saturating_sub(elapsed)));
    }
}
