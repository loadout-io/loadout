//! Wspólny kontrakt publikowania plików będących prawdą.
//!
//! Rdzeń zna kolejność durability, semantykę konfliktu, prawa i fault pointy. Operacje
//! platformowe względem utrzymanego deskryptora katalogu zostają w `engine::supervisor`,
//! jedynym miejscu dopuszczonym przez niezmiennik 3.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::io::{self, Write as _};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, RwLock, RwLockReadGuard, Weak};

use crate::engine::supervisor::{
    PublicationIdentity, PublicationRoot, PublicationTarget, publication_identity,
    publication_root_key,
};

/// Jawny tryb nowych plików definicji. Istniejący cel zachowuje swój tryb przy replace.
pub const DEFINITION_FILE_MODE: u32 = 0o644;

/// Prywatne handoffy, attachmenty i evidence nie dostają praw grupy ani innych użytkowników.
pub const PRIVATE_FILE_MODE: u32 = 0o600;

const TEMP_PREFIX: &str = ".loadout-writing-";
const TEMP_SUFFIX: &str = ".tmp";
const RECENT_LIFECYCLES: usize = 256;

/// Dwie semantyki publikacji; konflikt dotyczy wyłącznie niepowtarzalnego claimu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicationOperation {
    Replace,
    CreateIfAbsent,
}

/// Punkty produkcyjnego algorytmu, w których acceptance target może odtworzyć awarię.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaultPoint {
    Begin,
    AfterTempCreated,
    AfterPartialWrite,
    AfterWrite,
    AfterFileSync,
    BeforeCommit,
    AfterCommit,
    BeforeDirectorySync,
}

/// Decyzja fault-injectora. `Crash` pomija obsługiwany cleanup i zostawia pracę recovery.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FaultAction {
    #[default]
    Continue,
    Fail,
    Crash,
}

/// Obserwowalne granice recovery. Szew steruje wyłącznie interleavingiem testu; nie zastępuje
/// produkcyjnego spaceru ani jego blokady.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryPoint {
    BeforeLock,
    AfterRootOpened,
}

/// Jedno wejście recovery zakotwiczone w kontrolowanym root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryEvent {
    pub point: RecoveryPoint,
    pub root: PathBuf,
}

/// Jedno wejście do wspólnego rdzenia, widoczne także dla zgodnościowych adapterów.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationEvent {
    pub operation: PublicationOperation,
    pub point: FaultPoint,
    pub target: PathBuf,
}

/// Mały produkcyjny szew do fault injection; nie zawiera alternatywnego sposobu zapisu.
pub trait FaultInjector: Send + Sync {
    fn action(&self, event: &PublicationEvent) -> FaultAction;

    fn recovery_action(&self, _event: &RecoveryEvent) -> FaultAction {
        FaultAction::Continue
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PublishError {
    #[error("{} already exists", target.display())]
    Conflict { target: PathBuf },
    #[error("{} is not a safe target inside the controlled root", target.display())]
    InvalidTarget { target: PathBuf },
    #[error("publication stopped at {point:?}; simulated crash: {crashed}")]
    Injected { point: FaultPoint, crashed: bool },
    #[error("recovery stopped at {point:?}; simulated crash: {crashed}")]
    RecoveryInjected { point: RecoveryPoint, crashed: bool },
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl PublishError {
    /// Zgodnościowy adapter supervisora zachowuje dotychczasowe `io::Result` callera evidence.
    #[must_use]
    pub fn into_io(self) -> io::Error {
        match self {
            Self::Io(error) => error,
            Self::Conflict { .. } => io::Error::new(io::ErrorKind::AlreadyExists, self),
            Self::InvalidTarget { .. } => io::Error::new(io::ErrorKind::InvalidInput, self),
            Self::Injected { .. } | Self::RecoveryInjected { .. } => io::Error::other(self),
        }
    }
}

/// Polityka praw jest argumentem rdzenia, więc caller nie może odtworzyć jej własnym zapisem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModePolicy {
    PreserveExistingOr(u32),
    Exact(u32),
}

/// Jeden publisher zakotwiczony w kontrolowanym root.
#[derive(Clone)]
pub struct DurableFilePublisher {
    root: PathBuf,
    lifecycle: Arc<PublicationLifecycle>,
}

impl fmt::Debug for DurableFilePublisher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DurableFilePublisher")
            .field("root", &self.root)
            .field("lifecycle", &"<shared>")
            .finish()
    }
}

#[derive(Debug, Default)]
struct Initialization {
    root_identity: Option<PublicationIdentity>,
    ready_generation: u64,
    domains: BTreeMap<&'static str, u64>,
}

struct PublicationLifecycle {
    /// Zawsze brany przed `gate`. Guard jest synchroniczny i nigdy nie przechodzi przez await.
    initialization: Mutex<Initialization>,
    /// Read oznacza aktywną publikację/snapshot, write oznacza recovery całego roota.
    gate: RwLock<()>,
    /// Błąd wieloplikowej domeny jest publikowany przed oddaniem read guarda. Następny writer
    /// widzi inną generację i nie prześlizguje się przez jeszcze-zielony wpis `domains`.
    dirty_generation: AtomicU64,
}

impl PublicationLifecycle {
    fn pending() -> Self {
        Self {
            initialization: Mutex::new(Initialization::default()),
            gate: RwLock::new(()),
            dirty_generation: AtomicU64::new(0),
        }
    }
}

#[derive(Default)]
struct LifecycleRegistry {
    entries: BTreeMap<PathBuf, Weak<PublicationLifecycle>>,
    recent: VecDeque<Arc<PublicationLifecycle>>,
}

fn lifecycle_registry() -> &'static Mutex<LifecycleRegistry> {
    static REGISTRY: OnceLock<Mutex<LifecycleRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(LifecycleRegistry::default()))
}

fn lifecycle_for(root: &Path) -> Arc<PublicationLifecycle> {
    // Klucz scala `/tmp` z `/private/tmp` i równoważne pisownie, ale nie jest autoryzacją.
    // Otwarcie komponent po komponencie następuje dopiero pod właściwym guardem.
    let key = publication_root_key(root).unwrap_or_else(|_error| root.to_owned());
    let mut registry = lock(lifecycle_registry());
    registry
        .entries
        .retain(|_path, lifecycle| lifecycle.strong_count() > 0);
    let lifecycle = registry
        .entries
        .get(&key)
        .and_then(Weak::upgrade)
        .unwrap_or_else(|| {
            let lifecycle = Arc::new(PublicationLifecycle::pending());
            registry.entries.insert(key, Arc::downgrade(&lifecycle));
            lifecycle
        });

    // 2026-08-28: krótki cache zachowuje fakt pierwszego recovery pomiędzy małymi handle'ami
    // save/list, ale nie zamienia każdego historycznego runu w dożywotni wpis procesu.
    registry
        .recent
        .retain(|recent| !Arc::ptr_eq(recent, &lifecycle));
    registry.recent.push_back(Arc::clone(&lifecycle));
    while registry.recent.len() > RECENT_LIFECYCLES {
        registry.recent.pop_front();
    }
    lifecycle
}

/// Jedna współdzielona publikacja. Guard i descriptor root żyją do końca closure, dzięki
/// czemu wieloplikowa transakcja nie zostawia okna dla recovery między commitami.
pub(crate) struct PublicationBatch<'a> {
    publisher: &'a DurableFilePublisher,
    root: PublicationRoot,
    _guard: RwLockReadGuard<'a, ()>,
}

impl PublicationBatch<'_> {
    pub(crate) fn root(&self) -> &PublicationRoot {
        &self.root
    }

    pub(crate) fn atomic_replace(
        &self,
        target: &Path,
        bytes: &[u8],
        mode: ModePolicy,
    ) -> Result<(), PublishError> {
        self.publish(
            target,
            bytes,
            mode,
            PublicationOperation::Replace,
            |_| Ok(()),
            |_| Ok(()),
        )
    }

    pub(crate) fn atomic_create_if_absent(
        &self,
        target: &Path,
        bytes: &[u8],
        mode: ModePolicy,
    ) -> Result<(), PublishError> {
        self.publish(
            target,
            bytes,
            mode,
            PublicationOperation::CreateIfAbsent,
            |_| Ok(()),
            |_| Ok(()),
        )
    }

    /// Zwraca identity dokładnie tego inode'u, który został podlinkowany pod nazwę celu.
    /// Identity jest pobierane z utrzymanego temp-fd przed commit, nie przez ponowne
    /// otwarcie nazwy podatne na podmianę między publikacją i rejestracją rollbacku.
    pub(crate) fn atomic_create_if_absent_with_identity(
        &self,
        target: &Path,
        bytes: &[u8],
        mode: ModePolicy,
    ) -> Result<PublicationIdentity, PublishError> {
        self.publish(
            target,
            bytes,
            mode,
            PublicationOperation::CreateIfAbsent,
            |temporary| publication_identity(temporary).map_err(PublishError::Io),
            |_| Ok(()),
        )
    }

    fn publish<T>(
        &self,
        target: &Path,
        bytes: &[u8],
        mode: ModePolicy,
        operation: PublicationOperation,
        capture: impl FnOnce(&std::fs::File) -> Result<T, PublishError>,
        validate: impl Fn(&PublicationTarget) -> Result<(), PublishError>,
    ) -> Result<T, PublishError> {
        let prepared = self.publisher.prepare(&self.root, target)?;
        DurableFilePublisher::publish_prepared(
            &prepared, target, bytes, mode, operation, capture, validate,
        )
    }
}

impl DurableFilePublisher {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            lifecycle: lifecycle_for(&root),
            root,
        }
    }

    pub fn atomic_replace(
        &self,
        target: &Path,
        bytes: &[u8],
        mode: ModePolicy,
    ) -> Result<(), PublishError> {
        self.with_publication(|batch| batch.atomic_replace(target, bytes, mode))
    }

    pub fn atomic_create_if_absent(
        &self,
        target: &Path,
        bytes: &[u8],
        mode: ModePolicy,
    ) -> Result<(), PublishError> {
        self.with_publication(|batch| batch.atomic_create_if_absent(target, bytes, mode))
    }

    /// Trzyma jeden shared guard i jeden descriptor root przez cały callback. Handoff używa
    /// tej granicy dla attachmentu i pointera; pojedynczy plik korzysta z tego samego wejścia.
    pub(crate) fn with_publication<T>(
        &self,
        publish: impl FnOnce(&PublicationBatch<'_>) -> Result<T, PublishError>,
    ) -> Result<T, PublishError> {
        self.enter_publication(None, |_root| Ok(()), publish)
    }

    /// Jak [`Self::with_publication`], ale pierwszy publisher danego lifecycle wykonuje pod
    /// exclusive guardem także cleanup domenowy. Dzięki temu nie ma luki core-recovery →
    /// attachment-recovery → pierwszy writer.
    pub(crate) fn with_initialized_publication<T>(
        &self,
        domain: &'static str,
        initialize: impl FnOnce(&PublicationRoot) -> Result<(), PublishError>,
        publish: impl FnOnce(&PublicationBatch<'_>) -> Result<T, PublishError>,
    ) -> Result<T, PublishError> {
        self.enter_publication(Some(domain), initialize, publish)
    }

    fn enter_publication<T>(
        &self,
        domain: Option<&'static str>,
        initialize: impl FnOnce(&PublicationRoot) -> Result<(), PublishError>,
        publish: impl FnOnce(&PublicationBatch<'_>) -> Result<T, PublishError>,
    ) -> Result<T, PublishError> {
        let faults = self.recovery_faults();
        let mut initialization = lock(&self.lifecycle.initialization);
        let generation = self.lifecycle.dirty_generation.load(Ordering::Acquire);
        let mut needs_initialization = initialization.root_identity.is_none()
            || initialization.ready_generation != generation
            || domain.is_some_and(|name| initialization.domains.get(name) != Some(&generation));

        let mut ready = None;
        if !needs_initialization {
            let guard = self
                .lifecycle
                .gate
                .read()
                .unwrap_or_else(PoisonError::into_inner);
            let root = self.open_root()?;
            if initialization.root_identity == Some(root.identity()) {
                ready = Some((root, guard));
            } else {
                // Ta sama nazwa może już wskazywać nowy katalog. Stan gotowości należy do
                // inode'u, nie napisu ścieżki; nowy root musi przejść pełne recovery domeny.
                drop(guard);
                initialization.root_identity = None;
                initialization.domains.clear();
                needs_initialization = true;
            }
        }

        if needs_initialization {
            check_recovery_fault(faults.as_ref(), RecoveryPoint::BeforeLock, &self.root)?;
            let recovery_guard = self
                .lifecycle
                .gate
                .write()
                .unwrap_or_else(PoisonError::into_inner);
            let root = self.open_root()?;
            check_recovery_fault(faults.as_ref(), RecoveryPoint::AfterRootOpened, &self.root)?;
            recover_owned_temps_in(&root, Path::new(""))?;
            initialize(&root)?;
            root.validate_path_identity(&self.root)?;
            let identity = root.identity();
            if initialization.root_identity != Some(identity) {
                initialization.domains.clear();
            }
            initialization.root_identity = Some(identity);
            let recovered_generation = self.lifecycle.dirty_generation.load(Ordering::Acquire);
            initialization.ready_generation = recovered_generation;
            if let Some(domain) = domain {
                initialization.domains.insert(domain, recovered_generation);
            }
            drop(recovery_guard);

            // Nazwa mogła zostać podmieniona zaraz po udanym recovery. Otwieramy ją ponownie
            // już pod read guardem i odmawiamy tego wywołania zamiast publikować do innego inode'u.
            let guard = self
                .lifecycle
                .gate
                .read()
                .unwrap_or_else(PoisonError::into_inner);
            let current = self.open_root()?;
            if initialization.root_identity != Some(current.identity()) {
                initialization.root_identity = None;
                initialization.domains.clear();
                return Err(PublishError::InvalidTarget {
                    target: self.root.clone(),
                });
            }
            ready = Some((current, guard));
        }

        // Initialization pozostaje zablokowane do chwili zdobycia read guarda. Explicit
        // recovery nie może więc wślizgnąć się pomiędzy te dwie operacje.
        let (root, publication_guard) = ready.ok_or_else(|| PublishError::InvalidTarget {
            target: self.root.clone(),
        })?;
        drop(initialization);
        let batch = PublicationBatch {
            publisher: self,
            root,
            _guard: publication_guard,
        };
        let mut result = publish(&batch);
        if result.is_ok()
            && let Err(error) = batch.root.validate_path_identity(&self.root)
        {
            result = Err(PublishError::Io(error));
        }
        if result.is_err() {
            // Publikujemy brudną generację JESZCZE pod read guardem. Writer wchodzący po tej
            // odmowie musi zobaczyć core/domain recovery, także gdy cleanup błędu dysku zawiódł.
            self.lifecycle
                .dirty_generation
                .fetch_add(1, Ordering::AcqRel);
        }
        drop(batch);
        result
    }

    fn prepare(
        &self,
        root: &PublicationRoot,
        target: &Path,
    ) -> Result<PublicationTarget, PublishError> {
        let relative =
            target
                .strip_prefix(&self.root)
                .map_err(|_error| PublishError::InvalidTarget {
                    target: target.to_owned(),
                })?;
        if relative.as_os_str().is_empty()
            || !relative
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
        {
            return Err(PublishError::InvalidTarget {
                target: target.to_owned(),
            });
        }
        root.target(relative).map_err(|error| {
            if matches!(
                error.kind(),
                io::ErrorKind::InvalidInput | io::ErrorKind::NotADirectory
            ) {
                PublishError::InvalidTarget {
                    target: target.to_owned(),
                }
            } else {
                PublishError::Io(error)
            }
        })
    }

    fn publish_prepared<T>(
        prepared: &PublicationTarget,
        target: &Path,
        bytes: &[u8],
        mode: ModePolicy,
        operation: PublicationOperation,
        capture: impl FnOnce(&std::fs::File) -> Result<T, PublishError>,
        validate: impl Fn(&PublicationTarget) -> Result<(), PublishError>,
    ) -> Result<T, PublishError> {
        let faults = scoped_injector_for(target);
        let selected_mode = match (mode, prepared.target_mode()?) {
            (ModePolicy::PreserveExistingOr(_default), Some(existing)) => existing,
            (ModePolicy::PreserveExistingOr(default), None) => default,
            (ModePolicy::Exact(exact), _) => exact,
        };

        check_fault(faults.as_ref(), operation, FaultPoint::Begin, target)?;
        validate(prepared)?;

        let temporary_name = format!("{TEMP_PREFIX}{}{TEMP_SUFFIX}", uuid::Uuid::now_v7());
        let mut temporary_exists = false;
        let result = (|| {
            let mut temporary = prepared.create_temp(&temporary_name, selected_mode)?;
            temporary_exists = true;
            check_fault(
                faults.as_ref(),
                operation,
                FaultPoint::AfterTempCreated,
                target,
            )?;

            let split = bytes.len().div_ceil(2);
            temporary.write_all(&bytes[..split])?;
            check_fault(
                faults.as_ref(),
                operation,
                FaultPoint::AfterPartialWrite,
                target,
            )?;
            temporary.write_all(&bytes[split..])?;
            temporary.flush()?;
            check_fault(faults.as_ref(), operation, FaultPoint::AfterWrite, target)?;
            temporary.sync_all()?;
            check_fault(
                faults.as_ref(),
                operation,
                FaultPoint::AfterFileSync,
                target,
            )?;
            let published = capture(&temporary)?;
            drop(temporary);

            check_fault(faults.as_ref(), operation, FaultPoint::BeforeCommit, target)?;
            // Prywatny replace porównuje tutaj held-fd identity z bieżącą nazwą. Callback jest
            // no-opem dla zwykłych definicji, więc polityka prywatności nie wycieka do adapterów.
            validate(prepared)?;
            match operation {
                PublicationOperation::Replace => {
                    prepared.commit_replace(&temporary_name)?;
                    temporary_exists = false;
                }
                PublicationOperation::CreateIfAbsent => {
                    match prepared.commit_create_if_absent(&temporary_name) {
                        Ok(()) => {}
                        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                            return Err(PublishError::Conflict {
                                target: target.to_owned(),
                            });
                        }
                        Err(error) => return Err(PublishError::Io(error)),
                    }
                }
            }
            check_fault(faults.as_ref(), operation, FaultPoint::AfterCommit, target)?;

            // `linkat` zostawia dwa nazwiska tego samego inode. Temp znika przed jedynym
            // syncem katalogu, aby sukces utrwalał zarazem docelową nazwę i brak śmiecia.
            if operation == PublicationOperation::CreateIfAbsent {
                prepared.remove_temp(&temporary_name)?;
                temporary_exists = false;
            }
            check_fault(
                faults.as_ref(),
                operation,
                FaultPoint::BeforeDirectorySync,
                target,
            )?;
            prepared.sync_directory()?;
            Ok(published)
        })();

        let crashed = matches!(result, Err(PublishError::Injected { crashed: true, .. }));
        if temporary_exists && !crashed {
            // 2026-08-28: sprzątamy wyłącznie losową nazwę utworzoną w tej operacji. Błąd
            // cleanupu nie może zasłonić pierwotnego błędu zapisu lub syncu.
            let _cleanup = prepared.remove_temp(&temporary_name);
            let _synced = prepared.sync_directory();
        }
        result
    }

    /// Sprząta wyłącznie własne tempy pod tym rootem, bez naruszania opublikowanych celów.
    pub fn recover(&self) -> Result<(), PublishError> {
        self.recover_with(|_root| Ok(()))
    }

    /// Core temp cleanup i domenowy cleanup wykonują się pod tym samym exclusive guardem i
    /// względem tego samego utrzymanego roota.
    pub(crate) fn recover_with<T>(
        &self,
        recover_domain: impl FnOnce(&PublicationRoot) -> Result<T, PublishError>,
    ) -> Result<T, PublishError> {
        self.recover_with_expected_root(None, recover_domain)
    }

    fn recover_with_expected_root<T>(
        &self,
        expected_root: Option<PublicationIdentity>,
        recover_domain: impl FnOnce(&PublicationRoot) -> Result<T, PublishError>,
    ) -> Result<T, PublishError> {
        let faults = self.recovery_faults();
        if let Err(error) =
            check_recovery_fault(faults.as_ref(), RecoveryPoint::BeforeLock, &self.root)
        {
            self.lifecycle
                .dirty_generation
                .fetch_add(1, Ordering::AcqRel);
            return Err(error);
        }
        let mut initialization = lock(&self.lifecycle.initialization);
        let _recovery_guard = self
            .lifecycle
            .gate
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        let recovered = (|| {
            let root = self.open_root()?;
            check_recovery_fault(faults.as_ref(), RecoveryPoint::AfterRootOpened, &self.root)?;
            if expected_root.is_some_and(|expected| root.identity() != expected) {
                return Err(PublishError::InvalidTarget {
                    target: self.root.clone(),
                });
            }
            recover_owned_temps_in(&root, Path::new(""))?;
            let recovered = recover_domain(&root)?;
            root.validate_path_identity(&self.root)?;
            Ok((recovered, root.identity()))
        })();
        match recovered {
            Ok((recovered, identity)) => {
                if initialization.root_identity != Some(identity) {
                    initialization.domains.clear();
                }
                initialization.root_identity = Some(identity);
                initialization.ready_generation =
                    self.lifecycle.dirty_generation.load(Ordering::Acquire);
                Ok(recovered)
            }
            Err(error) => {
                // Recovery może odmówić po częściowym cleanupie. Stara gotowość nie przeżywa
                // takiej próby; następne wejście musi ponowić core oraz domenę.
                initialization.root_identity = None;
                initialization.domains.clear();
                self.lifecycle
                    .dirty_generation
                    .fetch_add(1, Ordering::AcqRel);
                Err(error)
            }
        }
    }

    fn recovery_faults(&self) -> Option<Arc<dyn FaultInjector>> {
        scoped_injector_for(&self.root)
    }

    fn open_root(&self) -> Result<PublicationRoot, PublishError> {
        PublicationRoot::open(&self.root).map_err(PublishError::Io)
    }
}

/// Domena z jawnymi podkatalogami (handoff + attachment) sprząta każdy katalog osobno pod
/// swoim exclusive guardem. Rdzeń nigdy nie zgaduje, że szeroki anchor jest właścicielem dzieci.
pub(crate) fn recover_owned_temps_in(
    root: &PublicationRoot,
    relative: &Path,
) -> Result<(), PublishError> {
    root.remove_matching_files_in(relative, |candidate| {
        candidate
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_owned_temp)
    })?;
    Ok(())
}

fn check_recovery_fault(
    faults: Option<&Arc<dyn FaultInjector>>,
    point: RecoveryPoint,
    root: &Path,
) -> Result<(), PublishError> {
    let Some(faults) = faults else {
        return Ok(());
    };
    match faults.recovery_action(&RecoveryEvent {
        point,
        root: root.to_owned(),
    }) {
        FaultAction::Continue => Ok(()),
        FaultAction::Fail => Err(PublishError::RecoveryInjected {
            point,
            crashed: false,
        }),
        FaultAction::Crash => Err(PublishError::RecoveryInjected {
            point,
            crashed: true,
        }),
    }
}

fn check_fault(
    faults: Option<&Arc<dyn FaultInjector>>,
    operation: PublicationOperation,
    point: FaultPoint,
    target: &Path,
) -> Result<(), PublishError> {
    let Some(faults) = faults else {
        return Ok(());
    };
    match faults.action(&PublicationEvent {
        operation,
        point,
        target: target.to_owned(),
    }) {
        FaultAction::Continue => Ok(()),
        FaultAction::Fail => Err(PublishError::Injected {
            point,
            crashed: false,
        }),
        FaultAction::Crash => Err(PublishError::Injected {
            point,
            crashed: true,
        }),
    }
}

fn is_owned_temp(name: &str) -> bool {
    name.strip_prefix(TEMP_PREFIX)
        .and_then(|rest| rest.strip_suffix(TEMP_SUFFIX))
        .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
}

#[derive(Clone)]
struct FaultRegistration {
    id: u64,
    root: PathBuf,
    faults: Arc<dyn FaultInjector>,
}

fn fault_registrations() -> &'static Mutex<Vec<FaultRegistration>> {
    static REGISTRATIONS: OnceLock<Mutex<Vec<FaultRegistration>>> = OnceLock::new();
    REGISTRATIONS.get_or_init(|| Mutex::new(Vec::new()))
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn scoped_injector_for(target: &Path) -> Option<Arc<dyn FaultInjector>> {
    let target = publication_root_key(target).unwrap_or_else(|_error| target.to_owned());
    lock(fault_registrations())
        .iter()
        .rev()
        .find(|registration| target.starts_with(&registration.root))
        .map(|registration| Arc::clone(&registration.faults))
}

/// Zakres, dzięki któremu niejawni produkcyjni callerzy pod tym rootem widzą ten sam injector.
pub struct FaultScope {
    id: u64,
}

impl fmt::Debug for FaultScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("FaultScope").finish_non_exhaustive()
    }
}

impl Drop for FaultScope {
    fn drop(&mut self) {
        lock(fault_registrations()).retain(|registration| registration.id != self.id);
    }
}

/// Instaluje fault injection także dla callerów zachowujących zgodnościowy interfejs supervisora.
pub fn scoped_faults(
    root: &Path,
    faults: Arc<dyn FaultInjector>,
) -> Result<FaultScope, PublishError> {
    static NEXT_SCOPE: AtomicU64 = AtomicU64::new(1);
    DurableFilePublisher::new(root).recover()?;
    let id = NEXT_SCOPE.fetch_add(1, Ordering::Relaxed);
    let root = publication_root_key(root).unwrap_or_else(|_error| root.to_owned());
    lock(fault_registrations()).push(FaultRegistration { id, root, faults });
    Ok(FaultScope { id })
}

/// Zgodnościowa powierzchnia evidence; supervisor re-eksportuje typ bez drugiej polityki.
pub struct PrivateFilePublisher {
    publisher: DurableFilePublisher,
    target: PathBuf,
    relative: PathBuf,
    expected_root: PublicationIdentity,
    expected_target: Option<PublicationIdentity>,
}

impl fmt::Debug for PrivateFilePublisher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrivateFilePublisher")
            .field("validated_root", &true)
            .field("validated_identity", &self.expected_target.is_some())
            .finish_non_exhaustive()
    }
}

impl PrivateFilePublisher {
    pub fn open(anchor: &Path, relative: &Path) -> io::Result<Self> {
        if relative.as_os_str().is_empty()
            || !relative
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a private publication path is not relative and plain",
            ));
        }
        let target = anchor.join(relative);
        let root = PublicationRoot::open(anchor)?;
        let prepared = root.target(relative)?;
        if prepared.has_writing_guard()? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "an evidence writing guard already exists",
            ));
        }
        let expected_target = prepared.private_target_identity()?;
        let expected_root = prepared.parent_identity()?;
        let publication_root = target
            .parent()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "a private publication target has no controlled parent",
                )
            })?
            .to_owned();
        let leaf = target
            .file_name()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "a private publication target has no file name",
                )
            })?
            .to_owned();
        Ok(Self {
            // 2026-08-28: lock i recovery należą do faktycznego katalogu publikacji. Anchor
            // workspace służy autoryzacji ścieżki, ale nie jest właścicielem tempów całego repo.
            publisher: DurableFilePublisher::new(publication_root),
            target,
            relative: PathBuf::from(leaf),
            expected_root,
            expected_target,
        })
    }

    pub fn publish(self, bytes: &[u8], replace: bool) -> io::Result<()> {
        let Self {
            publisher,
            target,
            relative,
            expected_root,
            expected_target,
        } = self;
        let operation = if replace {
            PublicationOperation::Replace
        } else {
            PublicationOperation::CreateIfAbsent
        };
        if replace && expected_target.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "the private publication target does not exist",
            ));
        }
        publisher
            .recover_with_expected_root(Some(expected_root), |root| {
                // 2026-08-28: open zwalidował leaf pod wcześniejszym rootem, ale dopiero ten
                // exclusive root jest autorytetem commit pointu. Ponowne przygotowanie z held fd
                // odmawia także podmiany całego roota, nie tylko samej nazwy pliku.
                // Exclusive recovery guard obejmuje także commit: dwa replace'y tego samego
                // prywatnego inode'u nie mogą oba przejść walidacji i wygrać późniejszym rename.
                if root.identity() != expected_root {
                    return Err(PublishError::InvalidTarget {
                        target: target.clone(),
                    });
                }
                let prepared = root.target(&relative)?;
                DurableFilePublisher::publish_prepared(
                    &prepared,
                    &target,
                    bytes,
                    ModePolicy::Exact(PRIVATE_FILE_MODE),
                    operation,
                    |_| Ok(()),
                    |prepared| {
                        if prepared.has_writing_guard()? {
                            return Err(PublishError::Io(io::Error::new(
                                io::ErrorKind::AlreadyExists,
                                "an evidence writing guard already exists",
                            )));
                        }
                        if replace
                            && !prepared.has_identity(expected_target.ok_or_else(|| {
                                PublishError::Io(io::Error::new(
                                    io::ErrorKind::NotFound,
                                    "the private publication target does not exist",
                                ))
                            })?)?
                        {
                            return Err(PublishError::Io(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "the private publication target changed after validation",
                            )));
                        }
                        Ok(())
                    },
                )
            })
            .map_err(PublishError::into_io)
    }
}
