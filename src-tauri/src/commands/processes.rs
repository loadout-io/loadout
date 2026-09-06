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

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, Weak};

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex as AsyncMutex, oneshot};
use tokio_util::sync::CancellationToken;

use crate::durable_file::{DurableFilePublisher, ModePolicy, PRIVATE_FILE_MODE};
use crate::engine::drivers::command::{CommandDriver, StartSpec, Staying, StayingOutput};
use crate::engine::drivers::{AgentHandle, SessionLeftover};
use crate::engine::limits::Slot;
use crate::engine::supervisor::{self, PublicationIdentity, PublicationRoot};
use crate::engine::supervisor::{GroupId, GroupProof, KeepsLeftovers, Leftover, StepTag};
use crate::workflow::ServiceLifetime;

pub(crate) mod launch;
mod readiness;
pub mod services;
pub use readiness::{ReadinessEnd, ReadinessState, ServiceEndpoint, ServiceReadiness};

/// Uchwyt usługi, nie numer procesu podlegający ponownemu użyciu przez system.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ServiceRef {
    pub workspace: PathBuf,
    pub run_id: String,
    pub node_key: String,
    pub service_id: String,
    pub generation: u64,
}

/// Zamrożony właściciel przekazywany przez wykonawcę Serve. Nie zawiera komendy.
#[derive(Debug, Clone)]
pub struct ServiceOwner {
    pub reference: ServiceRef,
    pub run_dir: PathBuf,
    pub cwd: PathBuf,
    pub lifetime: ServiceLifetime,
}

#[derive(Debug)]
pub struct ServiceStopReport {
    pub proofs: Vec<GroupProof>,
    pub window_owned: Vec<StartedProcess>,
}

/// Plikowy stan potrzebny recovery i retencji także po zakończeniu samego grafu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ServiceState {
    Configured,
    Starting,
    Running,
    Unproven,
    Dead,
    #[serde(other)]
    Unknown,
}

/// `services/<service_id>.json` w katalogu biegu; nigdy command/stdout/sekrety.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ServiceRecord {
    pub schema: u8,
    pub reference: ServiceRef,
    pub cwd: PathBuf,
    /// Katalog procesu może być podkatalogiem; cwd nadal jest korzeniem lease całej kopii.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_cwd: Option<PathBuf>,
    pub copy_identity: PublicationIdentity,
    pub lifetime: ServiceLifetime,
    pub step_id: String,
    pub pgid: Option<i32>,
    pub state: ServiceState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readiness: Option<ServiceReadiness>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endpoints: Vec<ServiceEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_reason: Option<String>,
}

impl ServiceRecord {
    #[must_use]
    pub(super) fn keeps_copy(&self) -> bool {
        self.state != ServiceState::Dead
    }
}

/// Odczyt jest ograniczony przed alokacją, przez no-follow; brak katalogu oznacza starszy bieg.
pub(super) fn read_service_records(run_dir: &Path) -> io::Result<Vec<ServiceRecord>> {
    let root = PublicationRoot::open(run_dir)?;
    let directory = Path::new("services");
    let entries = match root.list_directory(directory) {
        Ok(entries) => entries,
        Err(why) if why.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(why) => return Err(why),
    };
    let mut records = Vec::new();
    for entry in entries {
        if !entry.name.to_string_lossy().ends_with(".json") {
            continue;
        }
        let relative = directory.join(&entry.name);
        let mut file = root.open_regular_file(&relative)?;
        let size = file.metadata()?.len();
        if size > 64 * 1024 {
            return Err(io::Error::other(
                "the saved service description is too large",
            ));
        }
        let mut bytes = Vec::new();
        (&mut file).take(64 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 64 * 1024 {
            return Err(io::Error::other(
                "the saved service description grew while being read",
            ));
        }
        let record: ServiceRecord = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        if record.schema != 1
            || uuid::Uuid::parse_str(&record.reference.service_id).is_err()
            || entry.name != format!("{}.json", record.reference.service_id).as_str()
        {
            return Err(io::Error::other(
                "the saved service identity does not match its file",
            ));
        }
        records.push(record);
    }
    root.validate_path_identity(run_dir)?;
    Ok(records)
}

/// Recovery publikuje Dead dopiero po swoim dowodzie; ścieżkę rekordu składa ten sam writer.
pub(super) fn save_service_record(run_dir: &Path, record: &ServiceRecord) -> io::Result<()> {
    publish_service_record(run_dir, record, false)
}

fn publish_service_record(run_dir: &Path, record: &ServiceRecord, create: bool) -> io::Result<()> {
    if uuid::Uuid::parse_str(&record.reference.service_id).is_err() {
        return Err(io::Error::other("the saved service identity is not valid"));
    }
    let relative = PathBuf::from("services").join(format!("{}.json", record.reference.service_id));
    let target = run_dir.join(relative);
    let bytes = serde_json::to_vec(record).map_err(io::Error::other)?;
    let publisher = DurableFilePublisher::new(run_dir);
    let result = if create {
        publisher.atomic_create_if_absent(&target, &bytes, ModePolicy::Exact(PRIVATE_FILE_MODE))
    } else {
        publisher.atomic_replace(&target, &bytes, ModePolicy::Exact(PRIVATE_FILE_MODE))
    };
    result.map_err(super::super::durable_file::PublishError::into_io)
}

#[derive(Debug)]
struct ServiceOwnership {
    owner: ServiceOwner,
    /// Krótki zapis małego rekordu; zamek nigdy nie przechodzi przez await.
    record: Mutex<ServiceRecord>,
    ended: CancellationToken,
}

impl ServiceOwnership {
    fn create(
        owner: ServiceOwner,
        tag: &StepTag,
        identity: PublicationIdentity,
        process_cwd: PathBuf,
        state: ServiceState,
        create: bool,
    ) -> io::Result<Self> {
        if owner.reference.run_id != tag.run()
            || uuid::Uuid::parse_str(&owner.reference.run_id).is_err()
            || uuid::Uuid::parse_str(&owner.reference.service_id).is_err()
            || owner.reference.generation == 0
            || owner.lifetime == ServiceLifetime::Unknown
        {
            return Err(io::Error::other(
                "the service owner or lifetime is not supported",
            ));
        }
        let workspace = supervisor::publication_root_key(&owner.reference.workspace)?;
        let runs = workspace.join(".loadout/runs");
        let run_dir = supervisor::publication_root_key(&owner.run_dir)?;
        let cwd = supervisor::publication_root_key(&owner.cwd)?;
        if run_dir.parent() != Some(runs.as_path()) || !cwd.starts_with(&workspace) {
            return Err(io::Error::other(
                "the service folder is outside its workspace",
            ));
        }
        PublicationRoot::open(&owner.run_dir)?.ensure_directory(Path::new("services"), 0o700)?;
        let record = ServiceRecord {
            schema: 1,
            reference: owner.reference.clone(),
            cwd: owner.cwd.clone(),
            process_cwd: Some(process_cwd),
            copy_identity: identity,
            lifetime: owner.lifetime,
            step_id: tag.step().to_owned(),
            pgid: None,
            state,
            readiness: None,
            endpoints: Vec::new(),
            exit_code: None,
            exit_reason: None,
        };
        let ownership = Self {
            owner,
            record: Mutex::new(record),
            ended: CancellationToken::new(),
        };
        ownership.write(create)?;
        Ok(ownership)
    }

    fn write(&self, create: bool) -> io::Result<()> {
        let record = self.record.lock().unwrap_or_else(PoisonError::into_inner);
        publish_service_record(&self.owner.run_dir, &record, create)
    }

    fn state(&self, state: ServiceState, pgid: Option<i32>) {
        if state == ServiceState::Dead {
            self.ended.cancel();
        }
        {
            let mut record = self.record.lock().unwrap_or_else(PoisonError::into_inner);
            record.state = state;
            if state == ServiceState::Dead {
                if let Some(ready) = &mut record.readiness
                    && ready.state != ReadinessState::Failed
                {
                    *ready = ServiceReadiness {
                        state: ReadinessState::Stopped,
                        message: "The app stopped.".to_owned(),
                    };
                }
                for endpoint in &mut record.endpoints {
                    endpoint.state = ReadinessState::Stopped;
                }
            }
            if pgid.is_some() {
                record.pgid = pgid;
            }
        }
        if let Err(why) = self.write(false) {
            // Stary żywy rekord jest ostrożną blokadą recovery, nigdy zgodą na cleanup.
            tracing::error!(service = %self.owner.reference.service_id, %why,
                "the service ownership update could not be saved");
        }
    }

    fn died(&self, status: Option<&std::process::ExitStatus>) {
        {
            let mut record = self.record.lock().unwrap_or_else(PoisonError::into_inner);
            record.exit_code = status.and_then(std::process::ExitStatus::code);
            record.exit_reason = Some(match record.exit_code {
                Some(0) => "The app exited successfully.".to_owned(),
                Some(code) => format!("The app exited with code {code}."),
                None => "The app stopped; its exit code was not available.".to_owned(),
            });
        }
        self.state(ServiceState::Dead, None);
    }
}

type FinalizeCopy = Box<dyn FnOnce() + Send + 'static>;

struct CopyEntry {
    identity: PublicationIdentity,
    leases: BTreeSet<ServiceRef>,
    closing: bool,
    deferred: Option<FinalizeCopy>,
}

impl fmt::Debug for CopyEntry {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.debug_struct("CopyEntry")
            .field("identity", &self.identity)
            .field("leases", &self.leases)
            .field("closing", &self.closing)
            .field("deferred", &self.deferred.is_some())
            .finish()
    }
}

type CopyRegistry = Arc<Mutex<BTreeMap<PathBuf, CopyEntry>>>;

/// Prawo kończenia jednej kopii wyklucza nowy start aż do końca finalizacji.
#[derive(Debug)]
pub struct CopyFinalizationGuard {
    copies: CopyRegistry,
    cwd: PathBuf,
    identity: PublicationIdentity,
}

impl Drop for CopyFinalizationGuard {
    fn drop(&mut self) {
        let mut copies = self.copies.lock().unwrap_or_else(PoisonError::into_inner);
        if copies
            .get(&self.cwd)
            .is_some_and(|copy| copy.identity == self.identity)
        {
            copies.remove(&self.cwd);
        }
    }
}

/// Brak automatycznego zwalniania w Drop: porzucony uchwyt nie jest dowodem śmierci.
#[derive(Debug)]
struct CopyLease {
    copies: CopyRegistry,
    cwd: PathBuf,
    identity: PublicationIdentity,
    service: ServiceRef,
}

impl CopyLease {
    fn release(self) {
        let ready = {
            let mut copies = self.copies.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(copy) = copies.get_mut(&self.cwd) else {
                return;
            };
            if copy.identity != self.identity || !copy.leases.remove(&self.service) {
                return;
            }
            if copy.leases.is_empty() && !copy.closing {
                copy.deferred.take().inspect(|_finish| {
                    copy.closing = true;
                })
            } else {
                None
            }
        };
        if let Some(finish) = ready {
            dispatch_finalization(
                CopyFinalizationGuard {
                    copies: Arc::clone(&self.copies),
                    cwd: self.cwd,
                    identity: self.identity,
                },
                finish,
            );
        }
    }
}

fn dispatch_finalization(guard: CopyFinalizationGuard, finish: FinalizeCopy) {
    // Wykorzystujemy istniejącą pulę blocking, nie nowego schedulera czy workera.
    tokio::task::spawn_blocking(move || {
        if PublicationRoot::open(&guard.cwd).is_ok_and(|root| root.identity() == guard.identity) {
            finish();
        } else {
            tracing::error!(cwd = %guard.cwd.display(), "the service copy changed identity before finalization");
        }
        drop(guard);
    });
}

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
    /// Brak tylko dla dawnego, ręcznego `/start` poza biegiem.
    pub service: Option<ServiceRef>,
    pub cwd: Option<PathBuf>,
    pub lifetime: ServiceLifetime,
    pub readiness: Option<ServiceReadiness>,
    pub endpoints: Vec<ServiceEndpoint>,
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
    /// Jedyny właściciel tej grupy. Dopóki tu jest, jest komu zlecić kolejną eskalację.
    ///
    /// 2026-09 (Z-4) — `Box<dyn Leftover>`, nie `Box<dyn AgentHandle>`. Ocalałym jest tak samo
    /// komenda kroku „sprawdź" i App Server, którego start padł, a żadne z nich nie jest sesją
    /// agenta i nigdy nią nie będzie. Rejestr potrzebuje z całego uchwytu **jednego czasownika**,
    /// więc tyle właśnie żąda; sesja agenta wchodzi tu przez `drivers::SessionLeftover`.
    owner: Box<dyn Leftover>,
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
        Self::left_behind(Box::new(SessionLeftover::new(handle)), slot, group)
    }

    /// To samo dla ocalałego, który **nie jest** sesją agenta.
    ///
    /// 2026-09 (Z-4) — tędy wchodzą komenda kroku „sprawdź" i App Server, którego start padł.
    /// Osobny konstruktor, a nie zmiana [`Unproven::new`], bo tamten podpis wołają kroki agenta
    /// i nie ma powodu, żeby każdy z nich opakowywał uchwyt ręcznie.
    pub fn left_behind(
        owner: Box<dyn Leftover>,
        slot: Option<Slot>,
        group: Option<GroupId>,
    ) -> Self {
        Self { owner, slot, group }
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
        self.owner.ask_again().await
    }
}

/// Jeden wpis rejestru: fakty do synchronicznego odczytu i dokładnie jeden właściciel uchwytu.
#[derive(Debug)]
struct HeldProcess {
    command: String,
    cwd: PathBuf,
    pgid: i32,
    output: StayingOutput,
    /// Tokio, nie `std`, bo ten zamek CELOWO obejmuje `Staying::stop().await`: trzy konkurujące
    /// drogi muszą ustawić się w kolejce do tego samego `Supervised`, zamiast dwa razy wołać
    /// `wait` albo dwa razy sygnalizować tę samą grupę (niezmiennik 8, 2026-08-31).
    owner: AsyncMutex<Owner>,
    service: Option<Arc<ServiceOwnership>>,
    /// Wyjęcie po Dead jest jednorazowe; zamek nigdy nie przechodzi przez await.
    copy_lease: Mutex<Option<CopyLease>>,
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
        if let GroupProof::Dead { status } = &proof {
            *owner = Owner::Released;
            if let Some(service) = &self.service {
                service.died(status.as_ref());
            }
            let lease = self
                .copy_lease
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            if let Some(lease) = lease {
                lease.release();
            }
        } else if let Some(service) = &self.service {
            service.state(ServiceState::Unproven, None);
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
    /// Zamrożone opisy i ostatni wynik usługi, nie drugi właściciel systemowego procesu.
    /// Krótki zamek mapy nigdy nie przechodzi przez await.
    services: Mutex<BTreeMap<String, Arc<services::ManagedService>>>,
    /// Lease i finalizacja konkurują pod jednym krótkim zamkiem, nigdy przez await.
    copies: CopyRegistry,
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
    /// P-03b: czym pytamy system, czy instancja natywna naprawdę pokazała okno.
    native_ui: std::path::PathBuf,

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

/// Ten rejestr JEST miejscem, w które silnik odkłada ocalałych.
///
/// 2026-09 (Z-4) — trait mieszka w `engine::supervisor`, a implementacja tutaj, i to jest cała
/// odpowiedź na „bez importowania `commands` do `engine`": zależność idzie w jedyną stronę,
/// w którą wolno. Sterownik vendora dostaje `Arc<dyn KeepsLeftovers>` tym samym szwem, którym
/// dostaje target dowodów, i nie wie ani że po drugiej stronie stoi stan okna, ani że ten stan
/// ma drugą listę na rzeczy, które człowiek kazał zostawić.
impl KeepsLeftovers for Processes {
    fn keep_leftover(&self, owner: Box<dyn Leftover>, slot: Option<Slot>) {
        let group = owner.address();
        self.keep_unproven(Unproven::left_behind(owner, slot, group));
    }
}

impl Processes {
    /// Ani jednej rzeczy — stan aplikacji, która właśnie wstała.
    #[must_use]
    pub fn new() -> Self {
        Self::confirming_windows_with(crate::engine::native_ui::interpreter())
    }

    /// P-03b: ten sam rejestr, z podanym interpreterem potwierdzania okna.
    ///
    /// Szew, nie furtka: produkcja wchodzi tędy przez [`Processes::new`] z interpreterem
    /// systemowym, a kryterium podstawia własny — inaczej sądziłoby ZGODY TEJ MASZYNY zamiast
    /// tego kodu, i musiałoby być czerwone u każdego, kto ich nie nadał.
    #[must_use]
    pub fn confirming_windows_with(native_ui: std::path::PathBuf) -> Self {
        Self {
            services: Mutex::new(BTreeMap::new()),
            copies: Arc::new(Mutex::new(BTreeMap::new())),
            held: Arc::new(Mutex::new(BTreeMap::new())),
            natural_reapers: CancellationToken::new(),
            unproven: Mutex::new(Vec::new()),
            native_ui,
        }
    }

    /// Czym ten rejestr pyta system o okno instancji natywnej.
    pub(super) fn native_ui_program(&self) -> &Path {
        &self.native_ui
    }

    fn acquire_copy_lease(&self, cwd: &Path, service: &ServiceRef) -> io::Result<CopyLease> {
        let root = PublicationRoot::open(cwd)?;
        let identity = root.identity();
        let cwd = supervisor::publication_root_key(cwd)?;
        let mut copies = self.copies.lock().unwrap_or_else(PoisonError::into_inner);
        let copy = copies.entry(cwd.clone()).or_insert_with(|| CopyEntry {
            identity,
            leases: BTreeSet::new(),
            closing: false,
            deferred: None,
        });
        if copy.identity != identity || copy.closing || copy.deferred.is_some() {
            return Err(io::Error::other(
                "the service copy has changed or is being finalized",
            ));
        }
        if !copy.leases.insert(service.clone()) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "this service already owns the copy",
            ));
        }
        Ok(CopyLease {
            copies: Arc::clone(&self.copies),
            cwd,
            identity,
            service: service.clone(),
        })
    }

    /// Restart istniejącego właściciela może przejść przez oczekującą finalizację,
    /// ale dopiero po atomowym odłożeniu lease następnej generacji, PRZED Stop.
    fn next_copy_lease(
        &self,
        cwd: &Path,
        previous: &ServiceRef,
        next: &ServiceRef,
    ) -> io::Result<CopyLease> {
        let root = PublicationRoot::open(cwd)?;
        let identity = root.identity();
        let cwd = supervisor::publication_root_key(cwd)?;
        let mut copies = self.copies.lock().unwrap_or_else(PoisonError::into_inner);
        let copy = copies
            .get_mut(&cwd)
            .ok_or_else(|| io::Error::other("the app no longer holds its working folder"))?;
        if copy.identity != identity
            || copy.closing
            || !copy.leases.contains(previous)
            || previous.service_id != next.service_id
            || previous.generation.checked_add(1) != Some(next.generation)
            || !copy.leases.insert(next.clone())
        {
            return Err(io::Error::other(
                "the app's working folder or instance changed; nothing was restarted",
            ));
        }
        Ok(CopyLease {
            copies: Arc::clone(&self.copies),
            cwd,
            identity,
            service: next.clone(),
        })
    }

    /// Jedna atomowa decyzja względem nowych usług. None to wciąż zajęta kopia, nie błąd.
    pub fn try_finalize_copy(&self, cwd: &Path) -> io::Result<Option<CopyFinalizationGuard>> {
        let root = PublicationRoot::open(cwd)?;
        let identity = root.identity();
        let cwd = supervisor::publication_root_key(cwd)?;
        let mut copies = self.copies.lock().unwrap_or_else(PoisonError::into_inner);
        let copy = copies.entry(cwd.clone()).or_insert_with(|| CopyEntry {
            identity,
            leases: BTreeSet::new(),
            closing: false,
            deferred: None,
        });
        if copy.identity != identity {
            return Err(io::Error::other(
                "the service copy changed identity; nothing was finalized",
            ));
        }
        if copy.closing || !copy.leases.is_empty() || copy.deferred.is_some() {
            return Ok(None);
        }
        copy.closing = true;
        Ok(Some(CopyFinalizationGuard {
            copies: Arc::clone(&self.copies),
            cwd,
            identity,
        }))
    }

    /// Rejestruje jedną finalizację. Śmierć między `try_finalize` a tym wywołaniem jej nie gubi.
    pub fn defer_copy_finalization(&self, cwd: &Path, finish: FinalizeCopy) -> io::Result<()> {
        let root = PublicationRoot::open(cwd)?;
        let identity = root.identity();
        let cwd = supervisor::publication_root_key(cwd)?;
        let ready = {
            let mut copies = self.copies.lock().unwrap_or_else(PoisonError::into_inner);
            let copy = copies.entry(cwd.clone()).or_insert_with(|| CopyEntry {
                identity,
                leases: BTreeSet::new(),
                closing: false,
                deferred: None,
            });
            if copy.identity != identity || copy.closing || copy.deferred.is_some() {
                return Err(io::Error::other(
                    "the copy already has a finalizer or changed identity",
                ));
            }
            if copy.leases.is_empty() {
                copy.closing = true;
                Some(finish)
            } else {
                copy.deferred = Some(finish);
                None
            }
        };
        if let Some(finish) = ready {
            dispatch_finalization(
                CopyFinalizationGuard {
                    copies: Arc::clone(&self.copies),
                    cwd,
                    identity,
                },
                finish,
            );
        }
        Ok(())
    }

    /// Żywe/unproven usługi blokują snapshot rodzica fan-in, nawet gdy Serve step już succeeded.
    pub fn copy_has_services(&self, cwd: &Path) -> io::Result<bool> {
        let cwd = supervisor::publication_root_key(cwd)?;
        Ok(self
            .copies
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&cwd)
            .is_some_and(|copy| !copy.leases.is_empty()))
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
    ///
    /// `tag` jest **argumentem**, nie czymś, co ten rejestr zna sam z siebie (2026-09, Z-01d):
    /// tędy idzie zarówno kafelek „uruchom i zostaw" z grafu, który do biegu należy, jak i
    /// `/start` z wiersza wejścia, który nie należy do żadnego. `None` mówi to drugie wprost,
    /// zamiast wychodzić z pominiętego argumentu.
    pub fn start(&self, spec: &StartSpec, tag: Option<StepTag>) -> io::Result<StartedProcess> {
        self.start_registered(spec, tag, None, None, &[])
    }

    /// Przejmuje kopię i zapisuje właściciela PRZED spawn. Samo Started nie jest readiness.
    pub fn start_owned(
        &self,
        spec: &StartSpec,
        tag: StepTag,
        owner: ServiceOwner,
    ) -> io::Result<StartedProcess> {
        self.start_owned_configured(spec, tag, owner, &[], None)
    }

    pub fn start_owned_configured(
        &self,
        spec: &StartSpec,
        tag: StepTag,
        owner: ServiceOwner,
        endpoints: &[crate::workflow::ServiceEndpointSpec],
        ready: Option<&crate::workflow::ReadinessSpec>,
    ) -> io::Result<StartedProcess> {
        let canonical_copy = supervisor::publication_root_key(&owner.cwd)?;
        let relative = spec
            .cwd
            .strip_prefix(&owner.cwd)
            .or_else(|_| spec.cwd.strip_prefix(&canonical_copy))
            .map_err(|_| io::Error::other("the app folder is outside its working folder"))?;
        self.start_owned_description(
            &crate::workflow::LaunchDescription {
                command: spec.command.clone(),
                kind: crate::workflow::TargetKind::default(),
                test_data_env: None,
                subdirectory: relative.to_string_lossy().into_owned(),
                environment: BTreeMap::new(),
                required_env: Vec::new(),
                endpoints: endpoints.to_vec(),
                readiness: ready.cloned(),
            },
            tag,
            owner,
        )
    }

    /// Ta sama droga procesu i lease; opis nie tworzy drugiego runnera ani środowiska.
    pub fn start_owned_description(
        &self,
        description: &crate::workflow::LaunchDescription,
        tag: StepTag,
        owner: ServiceOwner,
    ) -> io::Result<StartedProcess> {
        let reference = self.configure_description(description, tag, owner)?;
        let slot = self.managed_service(&reference)?;
        match self.start_prepared(&slot, &reference) {
            Ok(started) => Ok(started),
            Err(why) => {
                slot.release_if_configured();
                Err(why)
            }
        }
    }

    fn start_registered(
        &self,
        spec: &StartSpec,
        tag: Option<StepTag>,
        service: Option<Arc<ServiceOwnership>>,
        copy_lease: Option<CopyLease>,
        environment: &[(String, std::ffi::OsString)],
    ) -> io::Result<StartedProcess> {
        let driver = match tag {
            Some(tag) => CommandDriver::new().for_step(tag),
            None => CommandDriver::new(),
        };
        let mut staying = match driver.start_to_stay_with_environment(spec, environment) {
            Ok(staying) => staying,
            Err(why) => {
                // spawn odmówił bez procesu; nie mylić tej drogi z nieudanym Stopem.
                if let Some(service) = &service {
                    service.state(ServiceState::Dead, None);
                }
                if let Some(lease) = copy_lease {
                    lease.release();
                }
                return Err(why);
            }
        };
        let natural_end = staying
            .natural_end()
            .ok_or_else(|| io::Error::other("a started command has no natural-end notification"))?;
        let group = staying.group();
        if let Some(service) = &service {
            service.state(ServiceState::Running, Some(group.pgid));
        }
        let entry = Arc::new(HeldProcess {
            command: staying.command().to_owned(),
            cwd: spec.cwd.clone(),
            pgid: group.pgid,
            output: staying.output(),
            owner: AsyncMutex::new(Owner::Held(staying)),
            service,
            copy_lease: Mutex::new(copy_lease),
        });
        let started = one_of(&entry);
        if let Some(service) = &entry.service {
            let slot = self
                .services
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&service.owner.reference.service_id)
                .cloned();
            if let Some(slot) = slot {
                slot.current
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .process = Some(Arc::clone(&entry));
            }
        }
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

    /// Operacja adresuje tożsamość instancji; spóźniony Stop nie sięga nowej generation.
    pub async fn stop_service(&self, reference: &ServiceRef) -> io::Result<Option<GroupProof>> {
        let managed = self
            .services
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&reference.service_id)
            .cloned();
        if let Some(slot) = managed {
            return self.stop_managed(&slot, reference).await;
        }
        let entry = {
            let held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
            held.values()
                .find(|entry| {
                    entry.service.as_ref().is_some_and(|service| {
                        service.owner.reference.service_id == reference.service_id
                    })
                })
                .cloned()
        };
        let Some(entry) = entry else {
            return Ok(None);
        };
        if entry
            .service
            .as_ref()
            .is_none_or(|service| &service.owner.reference != reference)
        {
            return Err(io::Error::other(
                "this service reference is stale or belongs to another run",
            ));
        }
        let proof = entry.prove().await;
        if proof
            .as_ref()
            .is_none_or(|proof| matches!(proof, GroupProof::Dead { .. }))
        {
            forget_if_current(&self.held, &entry);
        }
        Ok(proof)
    }

    /// Kończy tylko run-owned usługi TEGO biegu; pozostawione window-owned wracają jawnie.
    pub async fn stop_run(&self, workspace: &Path, run_id: &str) -> io::Result<ServiceStopReport> {
        let workspace = supervisor::publication_root_key(workspace)?;
        self.release_configured(Some(&workspace), Some(run_id));
        let belongs = |entry: &HeldProcess| {
            entry.service.as_ref().is_some_and(|service| {
                service.owner.reference.workspace == workspace
                    && service.owner.reference.run_id == run_id
            })
        };
        let taken = {
            let held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
            held.values()
                .filter(|entry| {
                    belongs(entry)
                        && entry
                            .service
                            .as_ref()
                            .is_some_and(|service| service.owner.lifetime == ServiceLifetime::Run)
                })
                .cloned()
                .collect::<Vec<_>>()
        };
        let mut proofs = Vec::new();
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
        let window_owned =
            self.held
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .values()
                .filter(|entry| {
                    belongs(entry)
                        && entry.service.as_ref().is_some_and(|service| {
                            service.owner.lifetime == ServiceLifetime::Window
                        })
                })
                .map(|entry| one_of(entry))
                .collect();
        Ok(ServiceStopReport {
            proofs,
            window_owned,
        })
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
        self.release_configured(None, None);
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
    let settings = staying.service.as_ref().map(|service| {
        let record = service
            .record
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        (record.readiness.clone(), record.endpoints.clone())
    });
    StartedProcess {
        command: staying.command.clone(),
        pgid: staying.pgid,
        // Obecność wpisu znaczy „bez dowodu śmierci". `Alive` zostawia go tutaj; `Dead` usuwa.
        alive: true,
        service: staying
            .service
            .as_ref()
            .map(|service| service.owner.reference.clone()),
        cwd: staying.service.as_ref().map(|_| staying.cwd.clone()),
        lifetime: staying
            .service
            .as_ref()
            .map_or(ServiceLifetime::Window, |service| service.owner.lifetime),
        readiness: settings.as_ref().and_then(|(ready, _)| ready.clone()),
        endpoints: settings.map_or_else(Vec::new, |(_, endpoints)| endpoints),
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
