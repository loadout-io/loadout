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

use std::collections::{BTreeMap, BTreeSet};
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

/// Rodzaj wpisu widziany przez `stat` bez podążania za symlinkiem.
///
/// Osobny typ, bo `io::ErrorKind` nie odróżnia przenośnie dowiązania od katalogu, a to jest
/// dokładnie ta różnica, którą człowiek musi przeczytać w zdaniu odmowy (niezmiennik 29).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateFileKind {
    Regular,
    Directory,
    Symlink,
    Other,
}

/// Fakty z `fstat`/`fstatat` w postaci niezależnej od platformy, podawane wspólnej polityce.
///
/// Pola są publiczne, żeby kryterium akceptacji mogło uczciwie wykonać gałąź obcego właściciela:
/// stworzenie inode'u należącego do innego UID-a wymaga roota, a bez tej gałęzi ta jedna odmowa
/// byłaby jedyną, której nikt nigdy nie sprawdził (2026-08-31).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrivateFileFacts {
    pub kind: PrivateFileKind,
    pub owner: u32,
    pub mode: u32,
}

/// Czy istniejący prywatny leaf musi mieć `0600` od początku, czy regularny plik należący do
/// bieżącego użytkownika wolno zacieśnić przed pierwszym I/O.
///
/// Dwie polityki, jeden rdzeń (niezmiennik 23): evidence żąda `ExactOwnerOnly`, a dziennik
/// zastany po starszym wydaniu Loadouta jest zacieśniany, bo skasowanie go zabrałoby jedyny
/// ślad po ostatniej awarii.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateFileModePolicy {
    ExactOwnerOnly,
    TightenOwnedLegacy,
}

/// Co opener ma jeszcze zrobić z otwartym deskryptorem, zanim odda go wołającemu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateFileDisposition {
    Ready,
    TightenPermissions,
}

/// Typowana przyczyna odmowy. Zdania są po angielsku, bo dochodzą do człowieka (decyzja D5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PrivateFileProblem {
    #[error("the private path is not a regular file ({kind:?})")]
    NotRegular { kind: PrivateFileKind },
    #[error("the private file belongs to uid {actual}, not effective uid {expected}")]
    ForeignOwner { actual: u32, expected: u32 },
    #[error("the private file has mode {actual:o}, not 600")]
    UnsafeMode { actual: u32 },
}

/// Wynik operacji na prywatnym leafie. Odmowa jest osobnym wariantem, a nie `io::Error`, bo
/// wołający ma ją nazwać człowiekowi; zwykłe I/O zachowuje swój `ErrorKind`.
#[derive(Debug, thiserror::Error)]
pub(crate) enum PrivateLeafError {
    #[error(transparent)]
    Unsafe(#[from] PrivateFileProblem),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Otwarty prywatny plik razem z tożsamością inode'u dowiedzioną po `openat` + `fstat`.
///
/// Tożsamość jest trzymana obok deskryptora, a nie odczytywana z nazwy przy każdym użyciu:
/// między odczytem po nazwie a operacją mieści się cała podmiana (ten sam powód, dla którego
/// istnieje [`PublicationIdentity`]).
#[derive(Debug)]
pub(crate) struct PrivateFileHandle {
    file: std::fs::File,
    identity: PublicationIdentity,
}

impl PrivateFileHandle {
    pub(crate) const fn file(&self) -> &std::fs::File {
        &self.file
    }

    pub(crate) const fn file_mut(&mut self) -> &mut std::fs::File {
        &mut self.file
    }

    pub(crate) const fn identity(&self) -> PublicationIdentity {
        self.identity
    }
}

/// Jedna czysta polityka dla wszystkich prywatnych openerów.
///
/// Czysta, bo dopiero rozdzielenie „co widać" od „co z tym zrobić" pozwala kryterium wykonać
/// gałąź, której nie da się zasadzić na dysku bez roota. Opener nadal musi zapytać o fakty
/// PONOWNIE na otwartym deskryptorze — `TightenPermissions` nie jest zgodą na `chmod` po
/// nazwie, tylko na `fchmod` uchwytu, który już trzyma.
pub fn validate_private_file_facts(
    facts: PrivateFileFacts,
    effective_uid: u32,
    mode_policy: PrivateFileModePolicy,
) -> Result<PrivateFileDisposition, PrivateFileProblem> {
    if facts.kind != PrivateFileKind::Regular {
        return Err(PrivateFileProblem::NotRegular { kind: facts.kind });
    }
    if facts.owner != effective_uid {
        return Err(PrivateFileProblem::ForeignOwner {
            actual: facts.owner,
            expected: effective_uid,
        });
    }
    if facts.mode == 0o600 {
        return Ok(PrivateFileDisposition::Ready);
    }
    match mode_policy {
        PrivateFileModePolicy::ExactOwnerOnly => {
            Err(PrivateFileProblem::UnsafeMode { actual: facts.mode })
        }
        PrivateFileModePolicy::TightenOwnedLegacy => Ok(PrivateFileDisposition::TightenPermissions),
    }
}

/// Nadaje istniejącej ścieżce prawa „tylko właściciel" (`0600`).
///
/// # Dlaczego to mieszka TUTAJ, a nie tam, gdzie jest potrzebne
///
/// Niezmiennik 3: kod zależny od platformy ma w tym drzewie **jeden dom**, i jest nim ten plik.
/// `#[cfg(unix)]` gdziekolwiek indziej przewraca bramkę — i to jest jedyny powód, dla którego
/// port na Windows będzie gałęzią `cfg`, a nie przepisaniem. Wołający dostaje API neutralne
/// wobec platformy i nie wie, że `PermissionsExt` istnieje.
///
/// Pierwszym wołającym jest gniazdo mostu (`crate::bridge::host`), gdzie **prawo do pliku JEST
/// zdolnością**: kto go otworzy, ten dostaje czasowniki tej sesji.
#[cfg(unix)]
pub fn owner_only(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

/// Jak wyżej. Windows dostanie swoją gałąź razem z resztą portu (`docs/PLAN.md`, linia cięcia).
#[cfg(windows)]
pub fn owner_only(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "owner-only permissions are not implemented on Windows",
    ))
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

    /// Otwiera istniejący prywatny leaf względem utrzymanego roota. `None` znaczy „nie ma go",
    /// bo create-if-absent nie potrzebuje istniejącego celu i nie jest to awaria.
    ///
    /// Typ, właściciel i prawa są sprawdzane DWA razy: raz po nazwie, żeby nie otwierać
    /// dowiązania ani cudzego pliku, i raz na otwartym deskryptorze, bo między jednym a drugim
    /// mieści się cała podmiana. Zacieśnienie idzie wyłącznie przez `fchmod` tego deskryptora.
    pub(crate) fn open_private_existing(
        &self,
        relative: &Path,
        mode_policy: PrivateFileModePolicy,
    ) -> Result<Option<PrivateFileHandle>, PrivateLeafError> {
        self.target(relative)?.open_private_existing(mode_policy)
    }

    /// Tworzy nowy prywatny plik `0600` przez `openat(O_EXCL)` i utrwala nową nazwę w katalogu.
    pub(crate) fn create_private(
        &self,
        relative: &Path,
    ) -> Result<PrivateFileHandle, PrivateLeafError> {
        self.target(relative)?.create_private()
    }

    /// Dowodzi, że nazwa nadal prowadzi do dokładnie tego prywatnego inode'u i nadal ma `0600`.
    pub(crate) fn validate_private_identity(
        &self,
        relative: &Path,
        expected: PublicationIdentity,
    ) -> Result<bool, PrivateLeafError> {
        Ok(self
            .open_private_existing(relative, PrivateFileModePolicy::ExactOwnerOnly)?
            .is_some_and(|opened| opened.identity == expected))
    }

    /// Podmienia nazwę docelową nazwą źródłową względem utrzymanych deskryptorów katalogów.
    ///
    /// Oba leafy muszą być prywatnymi plikami bieżącego użytkownika, a źródło musi nadal mieć
    /// wcześniej dowiedzioną tożsamość. Cel jest OTWIERANY przed `renameat` z premedytacją:
    /// bez tego rotacja nadpisałaby plik, którego ktoś z zewnątrz rozluźnił prawa, i zrobiłaby
    /// to po cichu (2026-08-31).
    #[cfg(unix)]
    pub(crate) fn replace_private_name(
        &self,
        source: &Path,
        destination: &Path,
        expected_source: PublicationIdentity,
    ) -> Result<(), PrivateLeafError> {
        use nix::fcntl::renameat;

        let source = self.target(source)?;
        let destination = self.target(destination)?;
        let source_identity = source
            .open_private_existing(PrivateFileModePolicy::ExactOwnerOnly)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "the private source vanished"))?
            .identity;
        if source_identity != expected_source {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the private source changed before rename",
            )
            .into());
        }
        let _validated_destination =
            destination.open_private_existing(PrivateFileModePolicy::ExactOwnerOnly)?;
        renameat(
            &source.directory,
            Path::new(&source.file_name),
            &destination.directory,
            Path::new(&destination.file_name),
        )
        .map_err(io::Error::from)?;
        destination.sync_directory()?;
        Ok(())
    }

    #[cfg(windows)]
    pub(crate) fn replace_private_name(
        &self,
        _source: &Path,
        _destination: &Path,
        _expected_source: PublicationIdentity,
    ) -> Result<(), PrivateLeafError> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "descriptor-relative private rename is not implemented on Windows",
        )
        .into())
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
    /// Fakty leafu spod nazwy, bez otwierania go i bez podążania za dowiązaniem.
    #[cfg(unix)]
    fn private_facts(&self) -> Result<Option<PrivateFileFacts>, PrivateLeafError> {
        use nix::errno::Errno;
        use nix::fcntl::AtFlags;
        use nix::sys::stat::fstatat;

        match fstatat(
            &self.directory,
            Path::new(&self.file_name),
            AtFlags::AT_SYMLINK_NOFOLLOW,
        ) {
            Ok(stat) => Ok(Some(private_file_facts(&stat))),
            Err(Errno::ENOENT) => Ok(None),
            Err(error) => Err(io::Error::from(error).into()),
        }
    }

    #[cfg(windows)]
    fn private_facts(&self) -> Result<Option<PrivateFileFacts>, PrivateLeafError> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "descriptor-relative private metadata is not implemented on Windows",
        )
        .into())
    }

    #[cfg(unix)]
    fn open_private_existing(
        &self,
        mode_policy: PrivateFileModePolicy,
    ) -> Result<Option<PrivateFileHandle>, PrivateLeafError> {
        use nix::fcntl::{OFlag, openat};
        use nix::sys::stat::{Mode, fchmod};
        use nix::unistd::{fsync, geteuid};

        let Some(named) = self.private_facts()? else {
            return Ok(None);
        };
        // Pierwsza kontrola po nazwie: dowiązanie ani cudzy plik nie mają być NAWET otwarte.
        validate_private_file_facts(named, geteuid().as_raw(), mode_policy)?;

        let opened = openat(
            &self.directory,
            Path::new(&self.file_name),
            OFlag::O_RDWR | OFlag::O_APPEND | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(io::Error::from)?;
        let disposition =
            validate_private_file_facts(facts_of(&opened)?, geteuid().as_raw(), mode_policy)?;
        if disposition == PrivateFileDisposition::TightenPermissions {
            fchmod(&opened, Mode::from_bits_truncate(0o600)).map_err(io::Error::from)?;
            fsync(&opened).map_err(io::Error::from)?;
            // Trzeci raz i nie z ostrożności: `fchmod` mógł trafić w plik, który w międzyczasie
            // dostał inne prawa. Dopiero ten odczyt dowodzi, że deskryptor JEST już 0600.
            validate_private_file_facts(
                facts_of(&opened)?,
                geteuid().as_raw(),
                PrivateFileModePolicy::ExactOwnerOnly,
            )?;
        }
        let identity = identity_of(&opened)?;
        Ok(Some(PrivateFileHandle {
            file: std::fs::File::from(opened),
            identity,
        }))
    }

    #[cfg(windows)]
    fn open_private_existing(
        &self,
        _mode_policy: PrivateFileModePolicy,
    ) -> Result<Option<PrivateFileHandle>, PrivateLeafError> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "descriptor-relative private open is not implemented on Windows",
        )
        .into())
    }

    #[cfg(unix)]
    fn create_private(&self) -> Result<PrivateFileHandle, PrivateLeafError> {
        use nix::fcntl::{OFlag, openat};
        use nix::sys::stat::{Mode, fchmod};
        use nix::unistd::{fsync, geteuid};

        let opened = openat(
            &self.directory,
            Path::new(&self.file_name),
            OFlag::O_RDWR
                | OFlag::O_APPEND
                | OFlag::O_CREAT
                | OFlag::O_EXCL
                | OFlag::O_NOFOLLOW
                | OFlag::O_CLOEXEC,
            Mode::from_bits_truncate(0o600),
        )
        .map_err(io::Error::from)?;
        // `openat` respektuje umask, więc tryb z jego argumentu jest życzeniem, nie faktem.
        // Jawny `fchmod` sprawia, że polityka nie zależy od środowiska procesu.
        fchmod(&opened, Mode::from_bits_truncate(0o600)).map_err(io::Error::from)?;
        fsync(&opened).map_err(io::Error::from)?;
        validate_private_file_facts(
            facts_of(&opened)?,
            geteuid().as_raw(),
            PrivateFileModePolicy::ExactOwnerOnly,
        )?;
        let identity = identity_of(&opened)?;
        self.sync_directory()?;
        Ok(PrivateFileHandle {
            file: std::fs::File::from(opened),
            identity,
        })
    }

    #[cfg(windows)]
    fn create_private(&self) -> Result<PrivateFileHandle, PrivateLeafError> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "descriptor-relative private create is not implemented on Windows",
        )
        .into())
    }

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

/// Przekłada platformowy `stat` na fakty, o które pyta polityka. Jedno miejsce, w którym bity
/// trybu i UID zamieniają się w rzeczowniki — dzięki temu polityka nie zna `SFlag`.
#[cfg(unix)]
fn private_file_facts(stat: &nix::sys::stat::FileStat) -> PrivateFileFacts {
    use nix::sys::stat::SFlag;

    let flags = SFlag::from_bits_truncate(stat.st_mode);
    let kind = if flags == SFlag::S_IFREG {
        PrivateFileKind::Regular
    } else if flags == SFlag::S_IFDIR {
        PrivateFileKind::Directory
    } else if flags == SFlag::S_IFLNK {
        PrivateFileKind::Symlink
    } else {
        PrivateFileKind::Other
    };
    PrivateFileFacts {
        kind,
        owner: stat.st_uid,
        mode: u32::from(stat.st_mode & 0o777),
    }
}

#[cfg(unix)]
fn facts_of(file: &impl std::os::fd::AsFd) -> io::Result<PrivateFileFacts> {
    let stat = nix::sys::stat::fstat(file).map_err(io::Error::from)?;
    Ok(private_file_facts(&stat))
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

/// Evidence trzyma się polityki ścisłej: nic tu nie wolno zacieśnić w locie.
///
/// 2026-08-31: ciało przeszło pod [`validate_private_file_facts`], żeby dziennik i evidence
/// czytały te same fakty tą samą regułą (niezmiennik 23). Przy okazji zniknęło `contains(S_IFREG)`
/// — test podzbioru bitów przepuszczał `S_IFLNK` i `S_IFSOCK`, bo obie te wartości zawierają
/// bit `0o100000`. Porównanie `==` mówi o RODZAJU, a nie o wspólnym bicie.
#[cfg(unix)]
fn validate_private_fd(file: &impl std::os::fd::AsFd) -> io::Result<()> {
    use nix::unistd::geteuid;

    validate_private_file_facts(
        facts_of(file)?,
        geteuid().as_raw(),
        PrivateFileModePolicy::ExactOwnerOnly,
    )
    .map(|_ready| ())
    .map_err(|problem| io::Error::new(io::ErrorKind::PermissionDenied, problem))
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

/// Druga lista: czym agent **loguje się do swojego dostawcy** i jak wychodzi z tej sieci.
///
/// 2026-09 (Z-23, audyt D-8) — OSOBNA STAŁA, NIE DOPISEK DO [`PASSTHROUGH`], i to jest treść,
/// nie porządki. Tamta lista odpowiada na „czym w ogóle jest proces w tym systemie" i nie ma
/// w niej ani jednej nazwy, której wartość byłaby sekretem. Ta odpowiada na „czym ten agent
/// płaci i którędy wychodzi", więc każda pozycja tutaj jest decyzją o przepuszczeniu czegoś,
/// co bywa sekretem. Zlanie ich w jedną listę zabiera to rozróżnienie diffowi — a diff jest
/// jedynym miejscem, w którym ktokolwiek tę politykę ogląda.
///
/// **Nazwy, nigdy wartości.** Ta stała nie trzyma ani jednego bajtu sekretu i nie ma prawa
/// zacząć: wartości przychodzą ze środowiska okna albo z nośnika Loadouta
/// (`connections::secrets`), a stąd bierze się wyłącznie decyzja, którą nazwę przepuścić.
/// Ten sam wzorzec ma `codex mcp get`, które pokazuje `FIGMA_TOKEN=*****` (zmierzone
/// 2026-09-04 na `codex-cli 0.153.0`).
///
/// Dlaczego te trzy grupy. **Klucze dostawców** — bez nich `claude` i `codex` uruchomione
/// przez Loadouta widzą wylogowanego użytkownika, choć w terminalu tego samego człowieka
/// działają; to jest cała klasa „u mnie działa, z ikony nie". **Proxy** — w sieci firmowej
/// jest jedyną drogą na zewnątrz, a jego brak wygląda jak awaria dostawcy, nie jak wycięta
/// zmienna. **Własne CA** — bez niego to samo proxy zrywa TLS. Obie pisownie proxy, bo
/// narzędzia czytają raz wielkie, raz małe, a przepuszczenie połowy jest gorsze niż zera:
/// działa u jednego vendora i milczy u drugiego.
pub const VENDOR_AUTH_PASSTHROUGH: &[&str] = &[
    // czym agent płaci
    "ANTHROPIC_API_KEY",
    "OPENAI_API_KEY",
    // którędy wychodzi
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "no_proxy",
    // czemu ufa po drodze
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "NODE_EXTRA_CA_CERTS",
];

/// Nazwa zmiennej, którą **każdy** proces kroku niesie identyfikator swojego biegu.
///
/// 2026-09 (Z-01d) — po awarii aplikacji z całego biegu zostaje `run.json` i garść liczb; sam
/// `pgid` nie mówi, czyja to grupa, bo `kern.maxproc` na macOS wynosi 16 000 i numery przewijają
/// się w godzinach [T7 ryzyko 2]. Odzyskiwanie porównuje więc tę wartość z `id` odzyskiwanego
/// biegu, a grupa oznaczona **cudzym** biegiem nie dostaje ani jednego sygnału.
///
/// Wartością jest identyfikator BIEGU (`run.json` → `id`), nigdy identyfikator sesji vendora.
/// Poprzednie podejście brało tu `RunSpec::run_id`, czyli `job.session` — a odzyskiwanie
/// porównywało to z `run.json.id`, więc własna żywa grupa wychodziła obca i nie dostawała
/// sygnału w ogóle. Jedynym konstruktorem jest [`StepTag`] i tylko dlatego tej pomyłki nie da
/// się powtórzyć przez podanie innego napisu.
///
/// Zmienna, a nie argv, i to nie jest wygoda: dziecko przekazuje środowisko wnukom bez ani jednej
/// linii kodu po naszej stronie, a argv widzi wyłącznie ten jeden proces, którego my uruchomiliśmy
/// — czyli dokładnie nie ten, który przeżywa awarię [T7 §3.1].
pub const TAG_RUN: &str = "LOADOUT_RUN";

/// To samo dla kroku. Nie uczestniczy w decyzji o strzale — jest po to, żeby człowiek patrzący
/// w `ps` wiedział, **który kafelek** zostawił tę grupę.
pub const TAG_STEP: &str = "LOADOUT_STEP";

/// Znacznik jednego kroku jednego biegu, wpuszczany do środowiska każdego jego procesu.
///
/// Typ, a nie para napisów, wyłącznie po to, żeby konstruktor mógł żądać tych dwóch wartości,
/// które naprawdę stoją w `run.json` — powód w całości przy [`TAG_RUN`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepTag {
    run: String,
    step: String,
}

impl StepTag {
    /// `run` to `run.json` → `id`, `step` to `run.json` → `steps[].id`. Inne wartości nie mają tu
    /// czego szukać: odzyskiwanie porównuje pierwszą z nich **co do bajta**.
    #[must_use]
    pub fn new(run: &str, step: &str) -> Self {
        Self {
            run: run.to_owned(),
            step: step.to_owned(),
        }
    }

    /// Bieg, do którego należy proces noszący ten znacznik.
    #[must_use]
    pub fn run(&self) -> &str {
        &self.run
    }

    /// Krok, który go uruchomił.
    #[must_use]
    pub fn step(&self) -> &str {
        &self.step
    }
}

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
            // Cel WŁASNEGO instalatora Claude Code. Stoi przed menedżerami wersji, bo kiedy
            // vendor instaluje sam siebie, to jest ta kopia, którą człowiek właśnie zaktualizował.
            home.join(".claude/local"),
            home.join(".local/bin"),
            home.join(".npm-global/bin"),
            home.join(".bun/bin"),
            home.join(".volta/bin"),
            home.join(".fnm/aliases/default/bin"),
            home.join(".asdf/shims"),
        ]);
        dirs.extend(node_version_manager_dirs(&home));
    }
    dirs
}

/// Jedna instalacja node'a: rozpoznana wersja (albo jej brak) i katalog `bin`, ktory niesie.
#[cfg(target_os = "macos")]
type NodeInstall = (Option<(u64, u64, u64)>, PathBuf);

/// Katalogi `bin` instalacji node'a trzymanych przez `nvm`, od NAJNOWSZEJ wersji.
///
/// `nvm` nie kładzie niczego w stałym miejscu: każda wersja node'a ma własne drzewo, a wybór
/// robi powłoka, której aplikacja GUI nie uruchamia. Bez tego kroku instalacja, która dla
/// człowieka w terminalu jest jedyną, jaką ma, jest dla biegu niewidzialna.
///
/// **Porządek jest numeryczny, nie leksykograficzny.** `v10.0.0` sortowane jako napis stoi przed
/// `v9.0.0`, więc lista posortowana tekstem oddałaby najstarsze node'y jako pierwsze — i bieg
/// wziąłby CLI sprzed roku, mając nowsze obok. Wersji nieparsowalnych nie zgadujemy: idą na
/// koniec, bo nie umiemy powiedzieć, gdzie ich miejsce.
#[cfg(target_os = "macos")]
fn node_version_manager_dirs(home: &Path) -> Vec<PathBuf> {
    let root = home.join(".nvm/versions/node");
    let Ok(entries) = std::fs::read_dir(&root) else {
        return vec![root];
    };
    let mut found: Vec<NodeInstall> = entries
        .flatten()
        .map(|entry| {
            (
                parse_node_version(&entry.file_name()),
                entry.path().join("bin"),
            )
        })
        .collect();
    found.sort_by_key(|(version, _)| std::cmp::Reverse(*version));
    let mut dirs: Vec<PathBuf> = found.into_iter().map(|(_, path)| path).collect();
    if dirs.is_empty() {
        dirs.push(root);
    }
    dirs
}

/// `v22.11.0` → `(22, 11, 0)`. Cokolwiek innego → `None`, czyli „nie wiem, gdzie to postawić".
#[cfg(target_os = "macos")]
fn parse_node_version(name: &std::ffi::OsStr) -> Option<(u64, u64, u64)> {
    let text = name.to_str()?.strip_prefix('v')?;
    let mut parts = text.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
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

/// Czy ten plik mogą przeczytać także inne konta na tej maszynie.
///
/// 2026-09 (Z-23) — MIESZKA TUTAJ, a pyta o to `connections::secrets`, bo prawa pliku są
/// pojęciem systemu, a niezmiennik 3 trzyma kod zależny od platformy w tym jednym pliku.
/// Pytanie zadaje nośnik wartości Połączeń: plik z kluczami do wszystkich narzędzi człowieka
/// jest wart dokładnie tyle, ile jego prawa dostępu, więc czytelny dla grupy albo dla świata
/// jest pomijany w całości.
///
/// Nieodczytane prawa czytamy jako „czytelny", nie „prywatny": nośnik, o którego prawa nie
/// umiemy zapytać, ma zostać pominięty, a nie użyty w ciemno.
#[cfg(unix)]
#[must_use]
pub fn others_can_read_it(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path).map_or(true, |metadata| metadata.permissions().mode() & 0o077 != 0)
}

#[cfg(not(unix))]
#[must_use]
pub fn others_can_read_it(_path: &Path) -> bool {
    false
}

/// Okno między SIGTERM a SIGKILL w produkcji.
///
/// 5–10 s i **jedno ukryte ustawienie, nigdy kontrolka w UI** [T7 §3.3]. Powód, dla którego
/// w ogóle czekamy: `claude` na SIGTERM dosypuje transkrypt, zwalnia zamek sesji i odpala hooki
/// `SessionEnd`, wychodząc 143 — na SIGKILL nie robi nic z tych rzeczy, a skutek jest
/// niewidoczny aż do pierwszej sesji, której nie da się wznowić [T1 §4.6, 2026-08-15]. Dlatego
/// nigdy nie prowadzimy KILL-em.
pub const DEFAULT_GRACE: Duration = Duration::from_secs(5);

/// Ile sterownik czeka, aż agent wyjdzie sam po zamknięciu wejścia.
///
/// 2026-09 — jedna krotność okna łaski należy do polityki nadzoru, nie do trzech adapterów ani
/// ich wołających. Po tym suficie sterownik przechodzi przez zwykłe TERM → łaska → KILL → dowód,
/// zamiast zostawić krok na zawsze w `running` (niezmienniki 10 i 23).
pub const CLOSE_CEILING: Duration = Duration::from_secs(DEFAULT_GRACE.as_secs() * 2);

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

/// Jak często w trakcie zatrzymania przeglądamy drzewo potomków w poszukiwaniu grup, o których
/// jeszcze nie wiemy.
///
/// # Dlaczego to w ogóle istnieje (2026-09, Z-01c)
///
/// Bo **`pgid` lidera to nie cała prawda**. Claude Code uruchamia każdą komendę narzędzia Bash
/// we WŁASNEJ grupie procesów, więc wszystko, co taka powłoka odpali — `cargo test`, serwer
/// deweloperski — leży poza grupą, którą zabijamy i której śmierci dowodzimy. Zmierzone
/// w `../meetnotes` 2026-09-01: krok Combine miał `death_proof: true` w `run.json`, a binarium
/// testowe (PPID 1, inny `pgid`, 262 MB) żyło jeszcze szesnaście godzin po anulowaniu biegu.
/// Niezmiennik 6 nazywa to błędem finansowym, nie higienicznym.
///
/// 250 ms, a nie [`PROOF_POLL`]: przegląd kosztuje fork `ps`, a sonda `killpg(pgid, 0)` jest
/// gołym wywołaniem systemowym. Pytanie `ps` co 10 ms kosztowałoby więcej niż eskalacja, której
/// służy — a nowa grupa i tak potrzebuje kilkudziesięciu milisekund, żeby powstać.
const RESCAN_EVERY: Duration = Duration::from_millis(250);

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

/// Jedyny właściciel grupy, której **nie dało się** uznać za martwą — i jedyne, co się z nim robi.
///
/// # Po co ten trait istnieje i dlaczego mieszka TUTAJ (2026-09, Z-4)
///
/// Bo „ocalały" jest pojęciem tego modułu, nie pojęciem kroku. Do tego dnia rejestr aplikacji
/// ([`crate::commands::processes::Unproven`]) umiał przejąć wyłącznie `Box<dyn AgentHandle>`, więc
/// dwie inne rzeczy, które dokładnie tak samo przeżywają pełną eskalację — komenda kroku
/// „sprawdź" ([`crate::engine::drivers::command::Checking`]) i App Server, którego start padł
/// ([`Supervised`]) — nie miały gdzie pojechać. Obie były po prostu upuszczane: `Drop` posyłał
/// grupie dziewiątkę, ale **nie dowodził `ESRCH`**, więc zostawała grupa bez właściciela, której
/// nikt już nie mógł zapytać, plus miejsce z puli oddane czemuś, co dalej biegnie (niezmienniki
/// 6 i 11).
///
/// **Trait, nie enum wariantów** — i to jest cała odpowiedź na „bez importowania `commands` do
/// `engine`". Zależność idzie w jedyną dozwoloną stronę: `engine/` mówi, czym jest ocalały,
/// `commands/` go przechowuje. Enum z wariantem na każdy rodzaj właściciela kazałby rejestrowi
/// znać `Checking` i `Supervised` po nazwie, a każdy nowy rodzaj procesu dopisywałby tam ramię.
///
/// Tylko `Send`, bez `Sync`, bo dokładnie tyle ma [`crate::engine::drivers::AgentHandle`], a
/// rejestr trzyma tę wartość pod `std::sync::Mutex` — a ten jest `Sync` już przy `T: Send`.
#[async_trait::async_trait]
pub trait Leftover: Send {
    /// Pełna eskalacja jeszcze raz, tym samym czasownikiem, którym schodzi ten rodzaj procesu.
    ///
    /// `Dead` wolno oddać wyłącznie po `ESRCH` (niezmiennik 6) — implementacja, która zwraca go
    /// po samym wysłaniu sygnału, kasuje wpis z rejestru i kończy pytanie kłamstwem.
    async fn ask_again(&mut self) -> GroupProof;

    /// Adres tej grupy, jeśli jest znany. Po nim człowiek pozna ją w `ps`, a odzyskiwanie przy
    /// następnym starcie ma po czym szukać.
    fn address(&self) -> Option<GroupId>;
}

/// Gdzie odkłada się ocalałego, kiedy próby się skończyły.
///
/// Drugi trait, a nie argument typu `&Processes`, z tego samego powodu co wyżej: sterownik
/// vendora musi umieć oddać ocalałego, nie wiedząc, że po drugiej stronie stoi stan okna
/// (niezmiennik 1). Implementuje to [`crate::commands::processes::Processes`], a sterownik
/// dostaje go zwykłym szwem `Arc<dyn …>` — tak samo, jak dostaje target dowodów.
pub trait KeepsLeftovers: Send + Sync {
    /// Przejmuje właściciela grupy razem z miejscem, które ta grupa **nadal** zajmuje.
    ///
    /// `slot` jest `None` wszędzie tam, gdzie ten proces miejsca z puli nie brał — nieudany start
    /// App Servera jest właśnie takim przypadkiem, bo permit należy do kroku, a nie do sterownika.
    fn keep_leftover(&self, owner: Box<dyn Leftover>, slot: Option<crate::engine::limits::Slot>);
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

    /// Co ten uchwyt wie o drzewie, które z niego wyrosło (Z-01c, 2026-09).
    ///
    /// `Box`, choć to dwa zwykłe zbiory: `Supervised` leży W CAŁOŚCI wewnątrz wariantu
    /// `commands::processes::Owner::Held`, a `clippy::large_enum_variant` (przez `clippy::all`
    /// = `deny`) daje temu wariantowi sufit 200 bajtów. Dwa `BTreeSet` wprost w polach to +48
    /// bajtów i czerwona bramka w cudzym pliku; jeden wskaźnik to +8.
    tree: Box<ProcessTree>,
}

/// Wiedza o drzewie potomków, zbierana przy każdym przeglądzie w [`Supervised::stop`].
///
/// Mieszka NA UCHWYCIE, a nie w zmiennej lokalnej `stop()`, z dwóch powodów. Po pierwsze, drugi
/// przegląd biegnie już po śmierci lidera — a wtedy jego potomkowie mają `ppid == 1` i domknięcie
/// od samego lidera nie doprowadzi do nich NIGDY. Po drugie, `stop()` bywa wołane kilka razy pod
/// rząd (`stop_startup_process` w sterowniku Codeksa) i druga tura bez pamięci meldowałaby `Dead`
/// nad żywą grupą.
///
/// Ryzyko rezydualne, świadome: PID-y są używane ponownie, więc pamięć o zmarłym potomku mogłaby
/// wskazać cudzą grupę [T7 ryzyko 2]. Okno tej pamięci to jednak czas jednego zatrzymania —
/// sekundy przy `kern.maxproc` równym 16 000 — a alternatywą jest wyciek, dla którego to pole
/// powstało.
#[derive(Debug)]
struct ProcessTree {
    /// Każda grupa procesów, o której ten uchwyt kiedykolwiek wiedział — nie tylko `group.pgid`.
    ///
    /// Powód przy [`RESCAN_EVERY`]: krok potrafi odpalić komendę, która zakłada WŁASNĄ grupę,
    /// a wtedy `killpg` po `pgid` lidera nigdy jej nie dosięgnie. Ten zbiór jest jednostką
    /// eskalacji i jednostką dowodu; `group.pgid` jest w nim od startu.
    groups: BTreeSet<i32>,

    /// `pid`-y z domknięcia przechodniego drzewa potomków, czyli zasiew następnego przeglądu.
    descendants: BTreeSet<i32>,
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
        // 2026-09 (Z-30) — DOWÓD BIERZEMY TU, BO TU JEST DARMOWY. Lider właśnie został zebrany,
        // więc jeżeli żadna znana grupa nie odpowiada już na sygnał zerowy, mamy `ESRCH`
        // z niezmiennika 6 bez wysyłania czegokolwiek. Bez tego uchwyt po naturalnym wyjściu
        // ginął z `proved_dead == false` i gwardia `Drop` strzelała dziewiątką w numer, który od
        // czasu przeglądu mógł już należeć do kogoś innego (powód przy [`machine_booted_at`]).
        if self.first_survivor().is_none() {
            self.proved_dead = true;
        }
        Ok(status)
    }

    /// SIGTERM na **każdą grupę, którą krok uruchomił**, okno łaski, potem SIGKILL — i dopiero
    /// wtedy dowód.
    ///
    /// Nigdy nie prowadzimy KILL-em: `claude` na SIGTERM dosypuje transkrypt i zwalnia zamek
    /// sesji, na SIGKILL nie robi nic [T1 §4.6]. Zwrócona wartość jest wynikiem pętli
    /// dowodowej, a nie potwierdzeniem wysłania sygnału: `GroupProof::Dead` wolno zwrócić
    /// dopiero wtedy, gdy `kill(-pgid, 0)` odpowiedział `ESRCH` — bo to jest ten pomiar, który
    /// w T7 §3.1 pokazał `total=2 orphaned=2` w chwili, w której status bezpośredniego dziecka
    /// mówił „zabity".
    ///
    /// 2026-09 (Z-01c) — „grupa" to od tego dnia LICZBA MNOGA. Do tej pory cała eskalacja i cały
    /// dowód były zdaniem o `pgid` lidera, a Claude Code uruchamia każdą komendę narzędzia Bash
    /// we własnej grupie: wszystko, co taka powłoka odpali, leżało poza tym zdaniem. Powód
    /// i pomiar stoją przy [`RESCAN_EVERY`]. Każda znaleziona grupa ma **własny** zegar łaski,
    /// także ta odkryta w trakcie — pełna eskalacja od piętnastki, nigdy dziewiątka od razu.
    ///
    /// Wołane drugi raz na tej samej grupie nadal zwraca `Dead`, tylko bez statusu: powtórzone
    /// zatrzymanie jest normalną ścieżką (anulowanie biegu, po którym idzie `Drop`), a nie
    /// błędem.
    ///
    /// 2026-09 (Z-30) — od tego dnia wchodzi tędy także uchwyt, którego dowód wziął `wait()` po
    /// naturalnym wyjściu.
    ///
    /// 2026-09 (Z-30b) — i dlatego status oddaje **pierwsze** zatrzymanie, a nie „ten, kto
    /// czekał": tak brzmiało to zdanie do tego dnia i było nieprawdą, bo `ClaudeDriver::cancel`
    /// czeka na wyjście w `timeout(…)` i wynik PORZUCA. Obie ścieżki oddające pierwszy dowód
    /// biorą go dziś przez `take()`, więc status wychodzi stąd dokładnie raz — niezależnie od
    /// tego, która z nich go zebrała — a drugie zatrzymanie zastaje puste pole
    /// (`stopping_a_group_twice_still_answers_dead_without_a_status`).
    pub async fn stop(&mut self, grace: Duration) -> GroupProof {
        if self.proved_dead {
            // 2026-09 (Z-30b) — `take()`, nie stałe `None`. Tędy wychodzi uchwyt, któremu dowód
            // z niezmiennika 6 dał już `wait()`, a jego statusu nikt jeszcze nie odebrał: pierwsze
            // podejście Z-30 zwracało tu `None` i kasowało jedyny obserwowalny ślad różnicy między
            // sesją, która wyszła SAMA po grzecznym przerwaniu (dopisany transkrypt, haki
            // `SessionEnd`, sesja do wznowienia), a zabitą dziewiątką. Pole zostaje puste, więc
            // powtórzone zatrzymanie nadal nie ma czego oddać — status odbiera się raz.
            return GroupProof::Dead {
                status: self.status.take(),
            };
        }

        let began = Instant::now();

        // 1. Przegląd drzewa PRZED pierwszym sygnałem. Kto uciekł do własnej grupy, ten jest
        //    powiązany z nami wyłącznie dopóki jego rodzic żyje — po pierwszej piętnastce
        //    zostaje po nim `ppid == 1` i żadne domknięcie już go nie znajdzie (Z-01c).
        self.rescan();

        // 2. Prowadzimy TERM-em i wysyłamy go na KAŻDĄ znaną grupę, nie tylko na grupę lidera:
        //    to wnuki przeżyły pomiar z T7 §3.1, a wnuk we własnej grupie przeżywa nawet ten.
        let mut termed: BTreeMap<i32, Instant> = BTreeMap::new();
        let mut killed: BTreeSet<i32> = BTreeSet::new();
        self.term_new_groups(&mut termed);

        // Sufit CAŁOŚCI, nie pojedynczej grupy: każda ma własny zegar łaski, więc grupa odkryta
        // w oknie schodzi później niż lider. Trzy okna mieszczą dwie fale nowych grup; po nich
        // uczciwą odpowiedzią jest `Alive`, a nie kolejne czekanie (niezmiennik 6).
        let ceiling = began + grace * 3 + PROOF_AFTER_KILL;
        let mut next_rescan = began + RESCAN_EVERY;

        loop {
            // Zebranie lidera jest częścią dowodu, nie sprzątaniem po nim: zombie NADAL
            // odpowiada na sygnał zerowy, więc grupa z zombie w środku nigdy nie da `ESRCH`.
            // `try_wait`, a nie `wait().await`: pożyczka na całe okno łaski nie zostawiłaby
            // uchwytu na przegląd drzewa, a przegląd jest tu ważniejszy niż jedno przebudzenie.
            if self.status.is_none()
                && let Ok(Some(status)) = self.child.try_wait()
            {
                self.status = Some(status);
            }

            // 3. Przegląd powtarzamy NIEZALEŻNIE od tego, czy lider jeszcze żyje. Poprzednia
            //    wersja patrzyła drugi raz tylko przy żywym liderze — a potomek utworzony
            //    w obsłudze SIGTERM rodzi się dokładnie wtedy, kiedy lider od tej samej
            //    piętnastki umiera, więc wymykał się w całości (Z-01c).
            let now = Instant::now();
            if now >= next_rescan {
                self.rescan();
                next_rescan = now + RESCAN_EVERY;
                // Grupa odkryta później dostaje pełną eskalację OD SIGTERM-a, z własnym zegarem.
                self.term_new_groups(&mut termed);
            }

            // 4. Okno minęło — dopiero teraz dziewiątka, osobno dla każdej grupy.
            self.kill_overdue(&termed, &mut killed, grace);

            // 5. `Dead` wolno oddać dopiero, kiedy KAŻDA znaleziona grupa milczy. Bez `ESRCH`
            //    nie wolno powiedzieć „nie żyje" (niezmiennik 6) — a to jest wynik do obsłużenia
            //    przez wołającego, nie błąd do zalogowania: ktoś dalej biegnie i wraca razem
            //    z adresem, pod którym da się go pytać.
            let Some(alive) = self.first_survivor() else {
                self.proved_dead = true;
                // 2026-09 (Z-30b) — `take()` także tutaj, i to nie jest symetria dla symetrii:
                // odkąd wyjście wyżej oddaje status, kopia zostawiona w polu wyszłaby stąd
                // DRUGI raz przy kolejnym zatrzymaniu tej samej martwej grupy.
                return GroupProof::Dead {
                    status: self.status.take(),
                };
            };
            if Instant::now() >= ceiling {
                return GroupProof::Alive { group: Some(alive) };
            }

            sleep(PROOF_POLL).await;
        }
    }

    /// Każda grupa procesów, o której ten uchwyt wie — po świeżym przeglądzie drzewa.
    ///
    /// # Po co to jest osobno od [`Supervised::stop`] (2026-09, Z-01d)
    ///
    /// Bo to jest zdanie **do zapisania w `run.json`**, a nie do wykonania. Krok, który schodzi,
    /// zabiera ze sobą jedyny uchwyt: po nim zostaje plik, a w pliku do tego dnia stała jedna
    /// liczba — `pgid` lidera. Wszystko, co krok odpalił we własnej grupie (a Claude Code robi
    /// tak z KAŻDĄ komendą narzędzia Bash), było dla odzyskiwania po awarii niewidzialne, bo
    /// nigdy nie zostało nigdzie zapisane.
    ///
    /// **Zapamiętuje**, nie tylko oddaje: `tree.groups` rośnie, więc następna eskalacja tego
    /// samego uchwytu obejmie także to, co widać było tylko teraz. Cena, którą trzeba nazwać:
    /// przegląd idzie domknięciem po `ppid`, więc potomek osierocony **przed** tym wywołaniem
    /// jest tu tak samo niewidzialny, jak w `stop()`. To jest ta luka, którą zamyka dopiero
    /// znacznik z [`TAG_RUN`] i skan po nim w odzyskiwaniu.
    #[cfg(unix)]
    pub fn descendant_groups(&mut self) -> Vec<i32> {
        self.rescan();
        self.tree.groups.iter().copied().collect()
    }

    /// Dopisuje do wiedzy tego uchwytu wszystko, co `ps` wie o naszym drzewie **teraz**.
    ///
    /// Wyłącznie dopisuje: `pgid`, który raz był nasz, zostaje jednostką dowodu do końca:
    /// grupa zniknięta z `ps` między dwoma przeglądami i tak musi odpowiedzieć `ESRCH`.
    #[cfg(unix)]
    fn rescan(&mut self) {
        let (pids, groups) = descendants_of(&self.tree.descendants);
        self.tree.descendants.extend(pids);
        self.tree.groups.extend(groups);
    }

    /// Piętnastka dla każdej grupy, która jeszcze jej nie dostała — i **tylko** dla takiej.
    ///
    /// Zegar łaski jest per grupa, bo to, że coś powstało później, nie czyni tego mniej wartym
    /// łaski: prowadzenie dziewiątką kosztuje transkrypt i zamek sesji [T1 §4.6]. Mapa `termed`
    /// jest jednocześnie strażnikiem powtórzeń — druga piętnastka w to samo miejsce przewraca
    /// kryterium Z-2 („okno łaski dostarcza dokładnie jeden SIGTERM").
    #[cfg(unix)]
    fn term_new_groups(&mut self, termed: &mut BTreeMap<i32, Instant>) {
        let leader = self.group.pgid;
        for &pgid in &self.tree.groups {
            if termed.contains_key(&pgid) {
                continue;
            }
            termed.insert(pgid, Instant::now());
            if pgid == leader {
                // Lider idzie przez `process-wrap`, bo to ten sam uchwyt, który zbiera jego
                // status. Gołe `killpg` obok niego byłoby DRUGIM nadawcą do tej samej grupy.
                let _ = self.child.signal(SIGNAL_TERM);
            } else {
                let _ = signal_group(pgid, SIGNAL_TERM);
            }
        }
    }

    /// Dziewiątka dla każdej grupy, której **własne** okno łaski już minęło. Raz na grupę.
    #[cfg(unix)]
    fn kill_overdue(
        &mut self,
        termed: &BTreeMap<i32, Instant>,
        killed: &mut BTreeSet<i32>,
        grace: Duration,
    ) {
        let leader = self.group.pgid;
        for (&pgid, &at) in termed {
            if killed.contains(&pgid) || at.elapsed() < grace {
                continue;
            }
            killed.insert(pgid);
            if pgid == leader {
                let _ = self.child.start_kill();
            } else {
                let _ = signal_group(pgid, SIGNAL_KILL);
            }
        }
    }

    /// Pierwsza grupa, która nadal odpowiada na sygnał zerowy — czyli adres, pod którym wołający
    /// ma kogo dalej pytać. `None` znaczy, że milczą wszystkie, i tylko wtedy wolno mówić
    /// „nie żyje" (niezmiennik 6).
    ///
    /// Grupa lidera idzie pierwsza, bo to o niej wołający wie najwięcej i to jej adres widzi
    /// człowiek w dzienniku nieudanego startu (`codex::stop_startup_process`).
    #[cfg(unix)]
    fn first_survivor(&self) -> Option<GroupId> {
        if !group_is_gone(self.group.pgid) {
            return Some(self.group);
        }
        self.tree
            .groups
            .iter()
            .copied()
            .find(|&pgid| pgid != self.group.pgid && !group_is_gone(pgid))
            // `pid` równe `pgid` nie przez uproszczenie, tylko z definicji POSIX: identyfikatorem
            // grupy JEST pid jej lidera, nawet kiedy tego lidera już nikt nie zbierze [T7 §6.2].
            .map(|pgid| GroupId { pid: pgid, pgid })
    }
}

/// Czy w grupie `pgid` nie ma już **nikogo**.
///
/// 2026-09 — pytamy przez `nix::killpg` z `None`, bo `process-wrap` odrzuca `signal(0)` jako
/// `EINVAL`. Fallback z prawdziwym sygnałem zamieniał sondę w salwę TERM/KILL co 10 ms.
///
/// Każda inna odpowiedź to „żywa", łącznie z `EPERM`, który znaczy, że grupa istnieje, tylko
/// nie jest nasza. Niezmiennik 6 nie zna stanu „chyba nie żyje".
///
/// 2026-09 (Z-01c) — wolna funkcja, a nie metoda: od tego dnia dowód dotyczy KAŻDEJ grupy,
/// którą krok uruchomił, a nie wyłącznie grupy lidera.
#[cfg(unix)]
fn group_is_gone(pgid: i32) -> bool {
    use nix::errno::Errno;
    use nix::sys::signal::killpg;
    use nix::unistd::Pid;

    match killpg(Pid::from_raw(pgid), None) {
        Err(Errno::ESRCH) => true,
        Ok(()) | Err(_) => false,
    }
}

/// Dobija te zapamiętane grupy, które na sondę jeszcze odpowiadają — i **żadnej innej**.
///
/// 2026-09 (Z-30) — SONDA ZEREM PRZED DZIEWIĄTKĄ, i to jest ta sama ostrożność, którą przy
/// odzyskiwaniu po awarii niesie strażnik czasu startu maszyny. `pgid` trafia do
/// [`ProcessTree::groups`] przy przeglądzie i **zostaje tam do końca życia uchwytu**, a numery
/// procesów przewijają się na macOS w godzinach (`kern.maxproc` = 16 000, powód przy
/// [`machine_booted_at`]). Dziewiątka w numer, który od tamtego przeglądu zdążył zwolnić się
/// i przypaść komuś innemu, jest błędem POPRAWNOŚCI, nie ryzykiem teoretycznym [T7 ryzyko 2].
///
/// **KAŻDA grupa, razem z grupą lidera.** Pierwsza wersja tej poprawki wyjmowała lidera przed
/// pętlą, bo schodzi on przez uchwyt dziecka (`ChildWrapper::start_kill`), a nie przez `killpg`
/// po gołym numerze — i to była luka, nie skrót: uchwyt zna swojego potomka tylko dopóki ten
/// jest jego potomkiem, a `start_kill` po zebranym dziecku jest strzałem bez sondy dokładnie tak
/// samo, jak każdy inny. Kto strzela, ten przechodzi tędy; sposób dostarczenia sygnału wybiera
/// wołający, w domknięciu `kill`.
///
/// Obie czynności wjeżdżają argumentem, bo to jedyne dwie rzeczy w tej decyzji, które rozmawiają
/// z systemem: dzięki temu kryterium akceptacji sprawdza SAMĄ politykę — łącznie z kolejnością,
/// w jakiej te dwie czynności padają — bez zabijania czegokolwiek na prawdziwej maszynie.
#[cfg(unix)]
#[doc(hidden)]
pub fn kill_what_still_answers(
    groups: &[i32],
    mut is_gone: impl FnMut(i32) -> bool,
    mut kill: impl FnMut(i32),
) {
    for &pgid in groups {
        // Grupa, która nie odpowiada, jest już pusta — nie ma tu czego dobijać, a jedyne, co
        // dziewiątka mogłaby w tym miejscu trafić, to cudza praca pod przewiniętym numerem.
        if is_gone(pgid) {
            continue;
        }
        kill(pgid);
    }
}

/// Czy w grupie `pgid` nie ma już nikogo — **publiczne okno** na ten sam pomiar, którym schodzi
/// każda grupa tego pliku.
///
/// 2026-09 (Z-01d) — istnieje dla odzyskiwania po awarii aplikacji, które musi odróżnić grupę
/// pustą (nie ma czego sprzątać, i to jest dowód, nie domysł) od grupy żywej, **zanim** wyśle do
/// niej pierwszy sygnał. Bez tego rozróżnienia sprzątanie meldowałoby jako posprzątane wszystko,
/// czego nawet nie tknęło.
///
/// Nazwa mówi „pusta", a nie „martwa", i to jest różnica z niezmiennika 6: `EPERM` znaczy „grupa
/// istnieje i nie jest nasza", więc `false` obejmuje także ją.
#[cfg(unix)]
#[must_use]
pub fn group_is_empty(pgid: i32) -> bool {
    group_is_gone(pgid)
}

/// Identyfikator biegu, którego proces siedzi w grupie `pgid` — odczytany z jego środowiska.
///
/// # Dlaczego `None` NIE znaczy „cudza grupa" (2026-09, Z-01d)
///
/// Bo znaczy „nie da się przeczytać". Zmierzone na tym macOS: `ps -E` oddaje pełne środowisko
/// procesu uruchomionego z binarium spoza ochrony systemu, ale dla `/bin/sh` i `/bin/sleep` nie
/// pokazuje ani jednej zmiennej. Potraktowanie braku znacznika jako obcości zamieniłoby więc
/// każdą sierotę po powłoce w grupę, której nie wolno tknąć — czyli skasowałoby całe sprzątanie
/// dokładnie tam, gdzie jest najbardziej potrzebne. Odmowa strzału należy się wyłącznie grupie,
/// która **powiedziała**, że jest czyjaś.
///
/// Dopasowanie po całym słowie: `LOADOUT_RUN=abc` nie ma prawa złapać `LOADOUT_RUN=abcdef`, bo
/// identyfikatory biegów bywają prefiksami swoich sąsiadów, a różnica między własną a cudzą
/// grupą jest tu różnicą między sprzątaniem a strzałem w niewinny proces.
///
/// `ps` przez podproces, dokładnie jak [`descendants_of`] i [`machine_booted_at`]: przejście po
/// `kinfo_proc` z ręki kupuje `unsafe`, które w tej skrzyni jest `deny`.
#[cfg(unix)]
#[must_use]
pub fn run_behind_group(pgid: i32) -> Option<String> {
    let listed = std::process::Command::new("ps")
        .args(["-E", "-ax", "-o", "pid=,ppid=,pgid=,command="])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&listed.stdout);

    let wanted = format!("{TAG_RUN}=");
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        // Trzy liczby, potem komenda razem z doklejonym środowiskiem. `pid` i `ppid` są tu
        // wyłącznie po to, żeby zjeść kolumny stojące przed `pgid` — tej trójki nie da się
        // zamówić w innej kolejności, a licząc od końca nie wiadomo, gdzie kończy się argv.
        let (Some(_pid), Some(_parent), Some(Ok(group))) = (
            fields.next(),
            fields.next(),
            fields.next().map(str::parse::<i32>),
        ) else {
            continue;
        };
        if group != pgid {
            continue;
        }
        let said = fields.collect::<Vec<_>>().join(" ");
        for word in said.split_whitespace() {
            if let Some(value) = word.strip_prefix(&wanted)
                && !value.is_empty()
            {
                return Some(value.to_owned());
            }
        }
    }
    None
}

/// Domknięcie przechodnie drzewa potomków `seeds` i zbiór grup, w których ono siedzi.
///
/// Domykamy od WSZYSTKICH znanych `pid`-ów, nie od samego lidera: po jego śmierci potomkowie
/// mają `ppid == 1` i od lidera nie prowadzi do nich już żadna krawędź (Z-01c, 2026-09).
///
/// `ps` przez podproces, tak samo jak [`machine_booted_at`] czyta `sysctl`: `libc` jest w tej
/// skrzyni „tylko po stałe sygnałów" (`Cargo.toml`), a przejście po `kinfo_proc` z ręki kupuje
/// `unsafe` i strukturę jądra za odczyt, który dzieje się cztery razy na sekundę przez sekundy.
///
/// Awaria `ps` oddaje puste zbiory i jest odpowiedzią, nie wyjątkiem: wiedza o grupie lidera
/// mieszka w [`Supervised::groups`] od startu, więc brak przeglądu zawęża eskalację do tego, co
/// wiedzieliśmy wcześniej — nigdy jej nie zeruje.
#[cfg(unix)]
fn descendants_of(seeds: &BTreeSet<i32>) -> (BTreeSet<i32>, BTreeSet<i32>) {
    let Ok(listed) = std::process::Command::new("ps")
        .args(["-axo", "pid=,ppid=,pgid="])
        .output()
    else {
        return (BTreeSet::new(), BTreeSet::new());
    };
    let text = String::from_utf8_lossy(&listed.stdout);

    let mut tree: Vec<(i32, i32, i32)> = Vec::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let (Some(Ok(pid)), Some(Ok(parent)), Some(Ok(group))) = (
            fields.next().map(str::parse::<i32>),
            fields.next().map(str::parse::<i32>),
            fields.next().map(str::parse::<i32>),
        ) else {
            continue;
        };
        tree.push((pid, parent, group));
    }

    // Powtarzamy przebieg, dopóki cokolwiek dochodzi: `ps` nie obiecuje, że dziecko stoi w
    // wydruku niżej niż rodzic, a jeden przebieg po nieuporządkowanej liście gubi wnuki.
    let mut found = seeds.clone();
    loop {
        let before = found.len();
        for &(pid, parent, _) in &tree {
            if found.contains(&parent) {
                found.insert(pid);
            }
        }
        if found.len() == before {
            break;
        }
    }

    let groups = tree
        .iter()
        .filter(|(pid, _, _)| found.contains(pid))
        .map(|&(_, _, group)| group)
        // DWAJ STRAŻNICY, oba o poprawności, nie o higienie [T7 §10.2]. Pierwszy: grupę bierzemy
        // tylko wtedy, gdy jej liderem jest ktoś z NASZEGO domknięcia — `killpg` po cudzej
        // grupie jest błędem poprawności, a proces, który przeszedł do grupy obcej, nie jest już
        // czymś, co ten krok uruchomił. Drugi: nigdy własna grupa Loadouta ani `pgid <= 0`,
        // bo `killpg(0, …)` to strzał w samego siebie razem z całym oknem.
        .filter(|group| *group > 0 && *group != own_process_group() && found.contains(group))
        .collect();

    (found, groups)
}

/// Nadzorowana grupa jest ocalałym sama z siebie: ma adres i ma czasownik, którym schodzi.
///
/// 2026-09 (Z-4) — tędy App Server, którego start padł, przestaje ginąć razem z ramką sterownika.
/// `stop` jest tu tym samym pełnym TERM → łaska → KILL → dowód, którym schodzi każda inna grupa;
/// gwardia `Drop` niżej zostaje ostatnią linią obrony dla tego, kogo nikt nie przejął.
#[async_trait::async_trait]
impl Leftover for Supervised {
    async fn ask_again(&mut self) -> GroupProof {
        self.stop(DEFAULT_GRACE).await
    }

    fn address(&self) -> Option<GroupId> {
        Some(self.group)
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

        // 2026-09 (Z-01c) — gwardia obejmuje ten sam zbiór grup co [`Supervised::stop`], bo
        // przegląd drzewa JEST synchroniczny: `ps` przez `std::process::Command`, bez runtime'u.
        // Do tego dnia stała tu dziewiątka w sam `pgid` lidera, więc wszystko, co krok odpalił
        // we własnej grupie, przeżywało porzucenie uchwytu w całości.
        //
        // Przegląd idzie PRZED zabiciem lidera i to jest cała jego wartość: po piętnastce
        // krawędzie `ppid` do uciekinierów już nie istnieją.
        //
        // Czego ta gwardia dalej NIE robi i nie zrobi: łaski ani dowodu `ESRCH`. Jest
        // synchroniczna i biegnie także wtedy, gdy runtime się zwija, więc nie ma tu czego
        // czekać — kto chce, żeby `claude` zdążył zamknąć sesję, ten woła `stop()`.
        let (pids, groups) = descendants_of(&self.tree.descendants);
        self.tree.descendants.extend(pids);
        self.tree.groups.extend(groups);

        // 2026-08-15 — dziewiątka bez łaski, bo to jest ścieżka, na której wołający wyszedł
        // wcześniej przez `?` i nikt już nie trzyma niczego, czym dałoby się poczekać.
        // Zostawiona grupa to `claude` palący limit w tle, zmierzone jako `total=2 orphaned=2`
        // [T7 §3.1].
        //
        // 2026-09 (Z-30) — GRUPA LIDERA IDZIE TĄ SAMĄ DROGĄ, CO KAŻDA INNA: najpierw sonda, potem
        // sygnał. Sposób dostarczenia zostaje różny i to jest jedyna różnica, jaka lidera dotyczy
        // — sygnał wysyła jego własny uchwyt, bo obok niego gołe `killpg` byłoby DRUGIM nadawcą
        // do tej samej grupy (ten sam powód, co w [`Supervised::term_new_groups`]).
        let remembered: Vec<i32> = self.tree.groups.iter().copied().collect();
        let leader = self.group.pgid;
        let child = &mut self.child;
        kill_what_still_answers(&remembered, group_is_gone, |pgid| {
            if pgid == leader {
                let _ = child.start_kill();
            } else {
                let _ = signal_group(pgid, SIGNAL_KILL);
            }
        });

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
    spawn_tagged(command, stdin, &[], None)
}

/// Wariant dla jawnie zatwierdzonych Connections. Nazwy i wartości są rozstrzygnięte przez
/// backend tuż przed startem; wartości nie trafiają do argv, pliku ani webviewa.
pub fn spawn_with_environment(
    command: Command,
    stdin: StdinPlan,
    environment: &[(String, OsString)],
) -> io::Result<Supervised> {
    spawn_tagged(command, stdin, environment, None)
}

/// Ta sama droga do systemu, plus **znacznik biegu** w środowisku dziecka.
///
/// 2026-09 (Z-01d) — jedyna droga, którą znacznik z [`StepTag`] wchodzi do procesu, i dlatego
/// jedyna, którą wolno startować cokolwiek należącego do kroku. Dwie pomyłki, obie zmierzone
/// w poprzednim podejściu i obie zamknięte tutaj kształtem, a nie umową:
///
/// * krok `serve` szedł do systemu z pominięciem drogi, która znacznik ustawia, więc jego procesy
///   nie dostawały go wcale. Dlatego `spawn` i [`spawn_with_environment`] są dziś **opakowaniami**
///   tej funkcji z jawnym `None`, a nie osobnymi drogami — pominięcie znacznika trzeba napisać;
/// * znacznik stał PRZED pętlą po `environment`, więc zatwierdzone Połączenie ze zmienną o tej
///   samej nazwie cicho go nadpisywało. Dlatego stoi **za** wszystkim innym: to ostatni zapis
///   wygrywa, a odzyskiwanie porównuje tę wartość z `run.json`.
pub fn spawn_tagged(
    mut command: Command,
    stdin: StdinPlan,
    environment: &[(String, OsString)],
    tag: Option<&StepTag>,
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
    // Dwie listy, jedna pętla: obie są tą samą polityką („co dziecko dostaje") zapisaną w dwóch
    // stałych, bo dopisanie nazwy do drugiej jest decyzją innej wagi niż do pierwszej
    // (2026-09, Z-23 — powód w całości przy [`VENDOR_AUTH_PASSTHROUGH`]).
    for &name in PASSTHROUGH.iter().chain(VENDOR_AUTH_PASSTHROUGH) {
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
    // ZNACZNIK NA SAMYM KOŃCU, za listą przepuszczaną i za Połączeniami (2026-09, Z-01d).
    // Zmienna o tej nazwie przyniesiona z Połączeń nadpisałaby go, gdyby stał wyżej — a wtedy
    // odzyskiwanie widziałoby cudzy napis tam, gdzie porównuje identyfikator biegu, i albo
    // odpuściłoby własną sierotę, albo strzeliło do niewinnego procesu.
    if let Some(tag) = tag {
        command.env(TAG_RUN, tag.run());
        command.env(TAG_STEP, tag.step());
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
        // Zasiew wiedzy o drzewie: jedna grupa i jeden `pid`. Reszta dochodzi z przeglądów
        // w [`Supervised::stop`] — tam, gdzie wiadomo, że coś ma zejść (Z-01c, 2026-09).
        tree: Box::new(ProcessTree {
            groups: BTreeSet::from([pid]),
            descendants: BTreeSet::from([pid]),
        }),
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
