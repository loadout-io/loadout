//! Cztery komendy źródeł biblioteki Context: dodaj, dokończ stronę, pokaż, usuń.
//!
//! Ta warstwa trzyma dokładnie to samo, co [`super::context`], i nic ponadto: **gdzie jest
//! biblioteka** (odwzorowanie `home → contexts/` w jednym miejscu, więc skorupa
//! `#[tauri::command]` nie zna układu katalogów) oraz **zegar** (`at` opisuje chwilę, w której
//! kliknął CZŁOWIEK, więc podaje go warstwa, która o kliknięciu wie).
//!
//! Osobny plik od `context.rs`, bo to jest osobne pytanie: tamten zapisuje NAZWĘ i SZKIC
//! zestawu, ten kładzie w nim PLIKI. Wspólny plik rósłby w miejsce, w którym mieszkają dwie
//! niepowiązane odpowiedzi (PLAN §12 nazywa go zresztą po imieniu).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::File;
use std::io::{self, Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use tokio_util::sync::CancellationToken;

use crate::context::access::{
    ContextAccess, ContextItemKind, FrozenFile as AccessFile, FrozenItem,
};
use crate::context::sources::{
    self, ImportItem, ImportReport, ImportRequest, PreparedPage, SourcePart,
};
use crate::context::{ContextSetRead, Error, files};
use crate::durable_file::{DurableFilePublisher, ModePolicy, PRIVATE_FILE_MODE};
pub(crate) use crate::engine::line::ReferenceMaterialState as DeliveryState;
use crate::engine::supervisor::{
    PrivateFileAccess, PrivateFileModePolicy, PublicationRoot, free_bytes, open_private_file,
};
use crate::evidence::{ContextKind, ContextSource as EvidenceSource};

const MANIFEST: &str = "context-sources/manifest.json";
const READS: &str = "context-sources/reads";
const MANIFEST_BYTES: usize = 16 * 1024 * 1024;
const READ_LOG_BYTES: usize = 4 * 1024 * 1024;
const READ_LOG_ENTRIES: usize = 4096;
const COPY_BUFFER_BYTES: usize = 64 * 1024;
const READER_LOCK: &str = "context-sources/.reader.lock";

type InputReader = (Box<dyn Read>, Option<(PublicationRoot, PathBuf)>);
type SavedSources = BTreeMap<String, (Snapshot, PathBuf, Arc<File>)>;

/// Kładzie w zestawie wszystko, co człowiek wybrał albo wkleił — i oddaje wynik KAŻDEJ pozycji.
pub fn import_context_sources_inner(
    home: &Path,
    set_id: &str,
    operation_id: &str,
    expected_revision: Option<String>,
    items: Vec<ImportItem>,
) -> Result<ImportReport, Error> {
    sources::import(
        &files::library_root(home),
        &ImportRequest {
            set_id: set_id.to_owned(),
            operation_id: operation_id.to_owned(),
            expected_revision,
            items,
            at: super::now_utc(),
        },
    )
}

/// Zatwierdza jedną przygotowaną stronę dokumentu.
pub fn complete_context_source_preparation_inner(
    home: &Path,
    set_id: &str,
    source_id: &str,
    page: &PreparedPage,
    expected_revision: Option<&str>,
) -> Result<ContextSetRead, Error> {
    sources::complete_preparation(
        &files::library_root(home),
        set_id,
        source_id,
        page,
        expected_revision,
        &super::now_utc(),
    )
}

/// Kawałek zatwierdzonego źródła — po jego identyfikatorze, nigdy po ścieżce od okna.
pub fn read_context_source_inner(
    home: &Path,
    set_id: &str,
    source_id: &str,
    page: Option<u32>,
) -> Result<SourcePart, Error> {
    sources::read_source(&files::library_root(home), set_id, source_id, page)
}

/// Zdejmuje źródło z zestawu.
pub fn remove_context_source_inner(
    home: &Path,
    set_id: &str,
    source_id: &str,
    expected_revision: Option<&str>,
) -> Result<ContextSetRead, Error> {
    sources::remove_source(
        &files::library_root(home),
        set_id,
        source_id,
        expected_revision,
        &super::now_utc(),
    )
}

// ── prywatny pakiet jednego biegu ─────────────────────────────────────────────────────────

/// Logiczny materiał gotowej wersji. Tożsamość nie zawiera odbiorcy; fizyczny adres dokłada
/// [`Snapshot::access_for`], żeby kopie i rundy nie dzieliły rachunku odczytu.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Address {
    pub set_id: String,
    pub version: String,
    pub item_id: String,
}

/// Ograniczony rachunek metadanych. Nie niesie ani jednego bajtu przeczytanej treści.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Delivery {
    pub set_name: String,
    pub version: String,
    pub item: String,
    pub kind: String,
    pub bytes: usize,
    pub state: DeliveryState,
    #[serde(default)]
    pub run_only: bool,
    /// 2026-09-08 (CT-06) — logiczna pozycja przydziału. Ścieżka `reference` jest dowodem
    /// pliku, nie tożsamością używaną do rozliczania późniejszych odczytów.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<Address>,
    pub reference: String,
}

/// Źródło bajtów kopiowanych do pakietu. Ręczny Debug nie ujawnia treści ani ścieżki.
#[derive(Clone)]
pub(crate) enum MaterialInput {
    Bytes(Vec<u8>),
    File(PathBuf),
}

impl fmt::Debug for MaterialInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = match self {
            Self::Bytes(bytes) => Some(bytes.len()),
            Self::File(_) => None,
        };
        formatter
            .debug_struct("ContextMaterialInput")
            .field("inline_bytes", &bytes)
            .finish_non_exhaustive()
    }
}

/// Jedna pozycja czytnika przed publikacją. Pliki w tym typie są wyłącznie wejściem kopiowania.
#[derive(Clone, Debug)]
pub(crate) struct PackageItem {
    pub address: Address,
    pub source_id: String,
    pub name: String,
    pub description: String,
    pub kind: ContextItemKind,
    pub page: Option<u32>,
    pub pages_total: Option<u32>,
    pub text: Option<MaterialInput>,
    pub preview: Option<MaterialInput>,
    pub agent_image: Option<MaterialInput>,
    pub run_only: bool,
}

impl PackageItem {
    pub fn reference(&self) -> String {
        let base = item_path(&self.address);
        let leaf = if self.text.is_some() {
            "text"
        } else if self.agent_image.is_some() {
            "for-the-agent.png"
        } else if self.preview.is_some() {
            "preview.png"
        } else {
            return base.to_string_lossy().into_owned();
        };
        base.join(leaf).to_string_lossy().into_owned()
    }

    pub fn bytes(&self) -> io::Result<usize> {
        [&self.text, &self.preview, &self.agent_image]
            .into_iter()
            .flatten()
            .try_fold(0usize, |total, input| {
                total
                    .checked_add(measure(input)?.0)
                    .ok_or_else(|| io::Error::other("Reference material is too large to measure."))
            })
    }
}

/// Przydział jednego fizycznego węzła.
#[derive(Clone, Debug)]
pub(crate) struct NodeSelection {
    pub items: Vec<Address>,
    pub delivery: Vec<Delivery>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Binding {
    pub id: String,
    pub digest: String,
}

/// Adres złożonego węzła Labu i odpowiadającego mu węzła historycznego przypadku.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SavedNode {
    pub source_run_id: String,
    pub source_node_key: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredFile {
    relative: String,
    digest: String,
    bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItemRecord {
    address: Address,
    source_id: String,
    name: String,
    description: String,
    kind: ContextItemKind,
    page: Option<u32>,
    pages_total: Option<u32>,
    text: Option<StoredFile>,
    preview: Option<StoredFile>,
    agent_image: Option<StoredFile>,
    /// 2026-09-08 (CT-07) — ten znacznik odróżnia nakładkę Startu od zapisu workflow.
    #[serde(default)]
    run_only: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Manifest {
    schema: u32,
    id: String,
    items: Vec<ItemRecord>,
    /// Ten sam kształt co w pakiecie pamięci: przydział jest mapą fizycznego odbiorcy na
    /// dokładne adresy. Rachunek stoi osobno, bo nie jest uprawnieniem do odczytu.
    nodes: BTreeMap<String, Vec<Address>>,
    delivery: BTreeMap<String, Vec<Delivery>>,
    /// 2026-09-08 (CT-08) — stary pakiet nie miał bloków replay, więc pusta mapa znika z
    /// serializacji: jego `Binding` pozostaje identyczny i historia dalej się otwiera.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    blocks: BTreeMap<String, super::context_inputs::ContextBlock>,
}

#[derive(Clone)]
struct CopyInput {
    stored: StoredFile,
    input: MaterialInput,
}

/// Debug celowo pomija zawartość i ścieżki plików użytkownika.
impl fmt::Debug for CopyInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ContextCopyInput")
            .field("bytes", &self.stored.bytes)
            .field("digest", &self.stored.digest)
            .finish_non_exhaustive()
    }
}

/// Zamrożony plan pakietu. `inputs` istnieją tylko do chwili publikacji; manifest i pliki są
/// jedyną prawdą historii biegu.
#[derive(Clone)]
pub struct Snapshot {
    manifest: Box<Manifest>,
    inputs: Vec<CopyInput>,
    source_dir: Option<Arc<PathBuf>>,
    /// Zamek każdego biegu przypadku żyje do publikacji wspólnego pakietu Labu.
    _source_holds: Option<Arc<Vec<Arc<File>>>>,
}

impl fmt::Debug for Snapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ContextSources")
            .field("id", &self.manifest.id)
            .field("items", &self.manifest.items.len())
            .field("nodes", &self.manifest.nodes.len())
            .finish_non_exhaustive()
    }
}

impl Snapshot {
    pub(crate) fn new(
        mut items: Vec<PackageItem>,
        nodes: BTreeMap<String, NodeSelection>,
        blocks: BTreeMap<String, super::context_inputs::ContextBlock>,
    ) -> io::Result<Self> {
        items.sort_by(|left, right| left.address.cmp(&right.address));
        let mut inputs = Vec::new();
        let mut records = Vec::with_capacity(items.len());
        for item in items {
            let base = item_path(&item.address);
            let text = measured(item.text, &base, "text", &mut inputs)?;
            let preview = measured(item.preview, &base, "preview.png", &mut inputs)?;
            let agent_image = measured(item.agent_image, &base, "for-the-agent.png", &mut inputs)?;
            records.push(ItemRecord {
                address: item.address,
                source_id: item.source_id,
                name: item.name,
                description: item.description,
                kind: item.kind,
                page: item.page,
                pages_total: item.pages_total,
                text,
                preview,
                agent_image,
                run_only: item.run_only,
            });
        }
        let mut delivery = BTreeMap::new();
        let nodes = nodes
            .into_iter()
            .map(|(key, node)| {
                delivery.insert(key.clone(), node.delivery);
                (key, node.items)
            })
            .collect();
        let manifest = Manifest {
            schema: 1,
            id: uuid::Uuid::now_v7().to_string(),
            items: records,
            nodes,
            delivery,
            blocks,
        };
        let snapshot = Self {
            manifest: Box::new(manifest),
            inputs,
            source_dir: None,
            _source_holds: None,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub(crate) fn binding(&self) -> io::Result<Binding> {
        Ok(Binding {
            id: self.manifest.id.clone(),
            digest: digest(&serde_json::to_vec(&self.manifest).map_err(io::Error::other)?),
        })
    }

    pub fn replay_blocks(
        &self,
    ) -> io::Result<&BTreeMap<String, super::context_inputs::ContextBlock>> {
        if self.manifest.blocks.keys().collect::<BTreeSet<_>>()
            != self.manifest.nodes.keys().collect::<BTreeSet<_>>()
        {
            return Err(unavailable(
                "the saved package has no matching replay input for every step",
            ));
        }
        Ok(&self.manifest.blocks)
    }

    /// Zamraża historyczne przydziały przypadków pod fizycznymi adresami komórek Labu.
    pub(crate) fn from_saved_nodes(
        project: &Path,
        bindings: &BTreeMap<String, SavedNode>,
    ) -> io::Result<Option<Self>> {
        let sources = held_saved_sources(project, bindings)?;
        let mut items = BTreeMap::<Address, (ItemRecord, PathBuf)>::new();
        let mut nodes = BTreeMap::new();
        let mut delivery = BTreeMap::new();
        let mut blocks = BTreeMap::new();
        for (target, binding) in bindings {
            let Some((snapshot, source_dir, _)) = sources.get(&binding.source_run_id) else {
                continue;
            };
            let Some(selected) = snapshot
                .manifest
                .nodes
                .get(&binding.source_node_key)
                .cloned()
            else {
                // 2026-09-08 (CT-08): brak wpisu jest historycznym faktem „ten krok nie dostał
                // Context", nie zgodą na rozwiązanie jego dzisiejszych przypięć z biblioteki.
                continue;
            };
            for address in &selected {
                let record = snapshot
                    .manifest
                    .items
                    .iter()
                    .find(|item| &item.address == address)
                    .ok_or_else(|| unavailable("a saved Lab input points to a missing item"))?;
                if let Some((known, _)) = items.get(address) {
                    if known != record {
                        return Err(unavailable(
                            "two saved Lab inputs disagree about one material version",
                        ));
                    }
                } else {
                    items.insert(address.clone(), (record.clone(), source_dir.clone()));
                }
            }
            nodes.insert(target.clone(), selected);
            delivery.insert(
                target.clone(),
                snapshot.manifest.delivery[&binding.source_node_key].clone(),
            );
            blocks.insert(
                target.clone(),
                snapshot.replay_blocks()?[&binding.source_node_key].clone(),
            );
        }
        if nodes.is_empty() {
            return Ok(None);
        }
        let mut inputs = Vec::new();
        let records = items
            .into_values()
            .map(|(record, source_dir)| {
                for stored in [&record.text, &record.preview, &record.agent_image]
                    .into_iter()
                    .flatten()
                {
                    inputs.push(CopyInput {
                        stored: stored.clone(),
                        input: MaterialInput::File(source_dir.join(&stored.relative)),
                    });
                }
                record
            })
            .collect();
        let snapshot = Self {
            manifest: Box::new(Manifest {
                schema: 1,
                id: uuid::Uuid::now_v7().to_string(),
                items: records,
                nodes,
                delivery,
                blocks,
            }),
            inputs,
            source_dir: None,
            _source_holds: Some(Arc::new(
                sources.into_values().map(|(_, _, held)| held).collect(),
            )),
        };
        snapshot.validate()?;
        Ok(Some(snapshot))
    }

    pub fn save_to(&self, run_dir: &Path) -> io::Result<()> {
        self.validate()?;
        let needed = self.inputs.iter().try_fold(0_u64, |total, input| {
            total.checked_add(input.stored.bytes as u64).ok_or_else(|| {
                io::Error::other("Reference materials are too large to measure safely.")
            })
        })?;
        if free_bytes(run_dir)? < needed {
            return Err(io::Error::other(
                "There is not enough free space to freeze the selected reference materials. Nothing started.",
            ));
        }
        let root = PublicationRoot::open(run_dir)?;
        root.ensure_directory(Path::new("context-sources"), 0o700)?;
        root.ensure_directory(Path::new(READS), 0o700)?;
        for input in &self.inputs {
            let relative = Path::new(&input.stored.relative);
            if let Some(parent) = relative.parent() {
                root.ensure_directory(parent, 0o700)?;
            }
            match root.open_private_existing(relative, PrivateFileModePolicy::ExactOwnerOnly) {
                Ok(Some(_)) => verify_stored(&root, &input.stored)?,
                Ok(None) => copy_input(&root, input)?,
                Err(error) => return Err(io::Error::other(error)),
            }
        }
        let bytes = serde_json::to_vec(&self.manifest).map_err(io::Error::other)?;
        if bytes.len() > MANIFEST_BYTES {
            return Err(io::Error::other(
                "The selected reference-material index is too large. Nothing started.",
            ));
        }
        DurableFilePublisher::new(run_dir)
            .atomic_create_if_absent(
                &run_dir.join(MANIFEST),
                &bytes,
                ModePolicy::Exact(PRIVATE_FILE_MODE),
            )
            .map_err(crate::durable_file::PublishError::into_io)?;
        let saved = read_package(run_dir)?;
        if saved.binding()? != self.binding()? {
            return Err(unavailable("the package changed while it was published"));
        }
        Ok(())
    }

    /// Kopiuje wyłącznie pliki związane odciskami w manifeście, nigdy ścieżkę podaną przez UI.
    pub fn copy_to(&self, run_dir: &Path) -> io::Result<()> {
        let source_dir = self
            .source_dir
            .as_deref()
            .ok_or_else(|| unavailable("the saved package has no source folder"))?;
        let _hold = hold(source_dir)?;
        self.validate()?;
        // 2026-09-08 (CT-08): zatwierdzony snapshot nie wystarcza, bo plik mógł zostać
        // podmieniony po zgodzie. Kopiowanie ponownie wiąże źródło przed pierwszym bajtem.
        if read_package(source_dir)?.binding()? != self.binding()? {
            return Err(unavailable(
                "the saved package changed before it could be copied",
            ));
        }
        let needed = stored_files(&self.manifest).try_fold(0_u64, |total, stored| {
            total
                .checked_add(stored.bytes as u64)
                .ok_or_else(|| unavailable("the package is too large to measure safely"))
        })?;
        if free_bytes(run_dir)? < needed {
            return Err(io::Error::other(
                "There is not enough free space to copy the recorded reference materials. Nothing started.",
            ));
        }
        let source = PublicationRoot::open(source_dir)?;
        let destination = PublicationRoot::open(run_dir)?;
        destination.ensure_directory(Path::new("context-sources"), 0o700)?;
        destination.ensure_directory(Path::new(READS), 0o700)?;
        for stored in stored_files(&self.manifest) {
            let relative = Path::new(&stored.relative);
            if let Some(parent) = relative.parent() {
                destination.ensure_directory(parent, 0o700)?;
            }
            copy_stored(&source, &destination, stored)?;
        }
        source.validate_path_identity(source_dir)?;
        for stored in stored_files(&self.manifest) {
            verify_stored(&destination, stored)?;
        }
        Ok(())
    }

    pub fn access_for(
        &self,
        run_dir: &Path,
        node_key: &str,
        expires: CancellationToken,
    ) -> io::Result<Option<ContextAccess>> {
        let Some(addresses) = self.manifest.nodes.get(node_key) else {
            return Ok(None);
        };
        if addresses.is_empty() {
            return Ok(None);
        }
        let by_address = self
            .manifest
            .items
            .iter()
            .map(|item| (&item.address, item))
            .collect::<BTreeMap<_, _>>();
        let mut frozen = Vec::with_capacity(addresses.len());
        for address in addresses {
            let item = by_address
                .get(address)
                .ok_or_else(|| unavailable("a step points to a missing package item"))?;
            frozen.push(FrozenItem {
                id: public_item_id(address),
                source_id: item.source_id.clone(),
                address: reader_address(&self.manifest.id, node_key, address),
                name: item.name.clone(),
                description: item.description.clone(),
                kind: item.kind,
                page: item.page,
                pages_total: item.pages_total,
                text: item.text.as_ref().map(|file| access_file(run_dir, file)),
                preview: item.preview.as_ref().map(|file| access_file(run_dir, file)),
                agent_image: item
                    .agent_image
                    .as_ref()
                    .map(|file| access_file(run_dir, file)),
            });
        }
        ContextAccess::frozen(
            format!("{}:{node_key}", self.manifest.id),
            self.manifest.id.clone(),
            frozen,
            expires,
        )
        .map(Some)
        .map_err(io::Error::other)
    }

    pub(crate) fn recorder_for(
        &self,
        run_dir: &Path,
        node_key: &str,
    ) -> io::Result<Option<Recorder>> {
        let Some(addresses) = self.manifest.nodes.get(node_key) else {
            return Ok(None);
        };
        if addresses.is_empty() {
            return Ok(None);
        }
        let written = read_opened(run_dir, node_key)?.len();
        let addresses = addresses
            .iter()
            .map(|address| {
                (
                    reader_address(&self.manifest.id, node_key, address),
                    address.clone(),
                )
            })
            .collect();
        Ok(Some(Recorder {
            run_dir: run_dir.to_path_buf(),
            node_key: node_key.to_owned(),
            addresses,
            // 2026-09-08 (CT-06) — zamek jest per odbiorca; wspólny zestaw nie serializuje
            // kroków, które mają naprawdę działać równolegle (niezmiennik 11).
            writes: Mutex::new(written),
        }))
    }

    pub fn recording_desk_for(
        &self,
        run_dir: &Path,
        node_key: &str,
        expires: CancellationToken,
    ) -> io::Result<Option<crate::bridge::context::ContextDesk>> {
        match (
            self.access_for(run_dir, node_key, expires)?,
            self.recorder_for(run_dir, node_key)?,
        ) {
            (Some(access), Some(recorder)) => Ok(Some(
                crate::bridge::context::ContextDesk::recording(access, recorder),
            )),
            (None, None) => Ok(None),
            _ => Err(io::Error::other(
                "The saved reference-material permissions do not match this step.",
            )),
        }
    }

    pub fn evidence_for(&self, node_key: &str) -> io::Result<Vec<EvidenceSource>> {
        let Some(addresses) = self.manifest.nodes.get(node_key) else {
            return Ok(Vec::new());
        };
        let by_address = self
            .manifest
            .items
            .iter()
            .map(|item| (&item.address, item))
            .collect::<BTreeMap<_, _>>();
        let mut evidence = Vec::new();
        for address in addresses {
            let item = by_address
                .get(address)
                .ok_or_else(|| unavailable("a step points to a missing package item"))?;
            for file in [&item.text, &item.agent_image, &item.preview]
                .into_iter()
                .flatten()
            {
                evidence.push(EvidenceSource {
                    kind: ContextKind::ReferenceMaterial,
                    reference: file.relative.clone(),
                    bytes: file.bytes,
                });
            }
        }
        Ok(evidence)
    }

    /// Łączy zamrożony przydział z ograniczonym dziennikiem skutecznych odczytów tego odbiorcy.
    pub(crate) fn delivery_for(&self, run_dir: &Path, node_key: &str) -> io::Result<Vec<Delivery>> {
        let Some(addresses) = self.manifest.nodes.get(node_key) else {
            return Ok(Vec::new());
        };
        let mut delivery = self
            .manifest
            .delivery
            .get(node_key)
            .cloned()
            .ok_or_else(|| unavailable("a step has no matching delivery record"))?;
        let opened = read_opened(run_dir, node_key)?;
        let mut totals = BTreeMap::<Address, usize>::new();
        for record in opened {
            if !addresses.contains(&record.address) {
                return Err(unavailable(
                    "the read history names material outside this step's allocation",
                ));
            }
            if record.bytes == 0 {
                continue;
            }
            let total = totals.entry(record.address).or_default();
            *total = total
                .checked_add(record.bytes)
                .ok_or_else(|| unavailable("the read history byte total overflowed"))?;
        }
        for record in &mut delivery {
            if record.state == DeliveryState::Available
                && let Some(address) = &record.address
                && let Some(bytes) = totals.get(address)
            {
                record.state = DeliveryState::Opened;
                record.bytes = *bytes;
            }
        }
        Ok(delivery)
    }

    /// Trzy liczby raportu wsparcia: zestawy, pozycje i pozycje naprawdę otwarte.
    pub fn diagnostic_counts(&self, run_dir: &Path, node_key: &str) -> io::Result<[usize; 3]> {
        let delivery = self.delivery_for(run_dir, node_key)?;
        let sets = delivery
            .iter()
            .map(|record| (&record.set_name, &record.version))
            .collect::<BTreeSet<_>>()
            .len();
        let opened = delivery
            .iter()
            .filter(|record| record.state == DeliveryState::Opened)
            .count();
        Ok([sets, delivery.len(), opened])
    }

    fn validate(&self) -> io::Result<()> {
        if self.manifest.schema != 1 || uuid::Uuid::parse_str(&self.manifest.id).is_err() {
            return Err(unavailable("the package format is not supported"));
        }
        let mut addresses = BTreeSet::new();
        let mut files = BTreeSet::new();
        for item in &self.manifest.items {
            if item.kind == ContextItemKind::Unknown {
                return Err(unavailable("a package item has an unsupported kind"));
            }
            normal_component(&item.address.set_id)?;
            normal_component(&item.address.version)?;
            if item.address.item_id.trim().is_empty() || !addresses.insert(item.address.clone()) {
                return Err(unavailable(
                    "a package item has an invalid or repeated address",
                ));
            }
            for file in [&item.text, &item.preview, &item.agent_image]
                .into_iter()
                .flatten()
            {
                package_path(&file.relative)?;
                valid_digest(&file.digest)?;
                if !files.insert(file.relative.clone()) {
                    return Err(unavailable("two package items share a writable file name"));
                }
            }
        }
        if self.manifest.nodes.keys().collect::<BTreeSet<_>>()
            != self.manifest.delivery.keys().collect::<BTreeSet<_>>()
        {
            return Err(unavailable(
                "the delivery records do not match their step permissions",
            ));
        }
        let mut reached = BTreeSet::new();
        for (node_key, selected_addresses) in &self.manifest.nodes {
            normal_component(node_key)?;
            let selected = selected_addresses.iter().collect::<BTreeSet<_>>();
            if selected.len() != selected_addresses.len()
                || selected.iter().any(|address| !addresses.contains(*address))
            {
                return Err(unavailable("a step points to missing reference materials"));
            }
            let delivery = &self.manifest.delivery[node_key];
            if delivery.len() > 4096 {
                return Err(unavailable("a step has too many delivery records"));
            }
            for record in delivery {
                package_path(&record.reference)?;
                if record.set_name.trim().is_empty()
                    || record.item.trim().is_empty()
                    || record.kind.trim().is_empty()
                {
                    return Err(unavailable("a delivery record is missing its identity"));
                }
                if record.state == DeliveryState::Available {
                    let address = record.address.as_ref().ok_or_else(|| {
                        unavailable("an available delivery record has no package address")
                    })?;
                    let item = self
                        .manifest
                        .items
                        .iter()
                        .find(|item| &item.address == address)
                        .ok_or_else(|| {
                            unavailable("a delivery record points to a missing package item")
                        })?;
                    if !selected.contains(address) || evidence_reference(item) != record.reference {
                        return Err(unavailable(
                            "a delivery record does not match this step's package item",
                        ));
                    }
                } else if record.address.is_some() {
                    return Err(unavailable(
                        "a delivery record without readable material has a package address",
                    ));
                }
            }
            reached.extend(selected.into_iter().cloned());
        }
        if reached != addresses {
            return Err(unavailable(
                "reference materials have no matching recipients",
            ));
        }
        Ok(())
    }
}

fn held_saved_sources(
    project: &Path,
    bindings: &BTreeMap<String, SavedNode>,
) -> io::Result<SavedSources> {
    let mut sources = SavedSources::new();
    for binding in bindings.values() {
        if sources.contains_key(&binding.source_run_id) {
            continue;
        }
        let source = super::lead_history::source_for(project, &binding.source_run_id)
            .map_err(|error| unavailable(&error))?;
        let source_dir = project.join(".loadout/runs").join(source.run_folder);
        let Some(before_hold) = read_bound(&source_dir)? else {
            continue;
        };
        let held = Arc::new(hold(&source_dir)?);
        let snapshot = read_bound(&source_dir)?
            .ok_or_else(|| unavailable("a saved Lab package disappeared while it was held"))?;
        if snapshot.binding()? != before_hold.binding()? {
            return Err(unavailable(
                "a saved Lab package changed while it was being held",
            ));
        }
        snapshot.replay_blocks()?;
        sources.insert(binding.source_run_id.clone(), (snapshot, source_dir, held));
    }
    Ok(sources)
}

/// Osobny, per-node zapis skutecznych odczytów. Mutex nie przeżywa await; w tym typie nie ma
/// nawet funkcji asynchronicznej (niezmiennik 8).
pub(crate) struct Recorder {
    run_dir: PathBuf,
    node_key: String,
    addresses: BTreeMap<String, Address>,
    writes: Mutex<usize>,
}

impl fmt::Debug for Recorder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ContextDeliveryRecorder")
            .field("node_key", &self.node_key)
            .field("items", &self.addresses.len())
            .finish_non_exhaustive()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Opened {
    schema: u32,
    address: Address,
    bytes: usize,
}

impl Recorder {
    /// Wywoływane wyłącznie po zbudowaniu odpowiedzi. Sam zamiar odczytu niczego nie zapisuje.
    pub fn opened(&self, address: &str, bytes: usize) -> io::Result<()> {
        let logical = self
            .addresses
            .get(address)
            .ok_or_else(|| unavailable("an opened item is outside this step's allocation"))?;
        if bytes == 0 {
            return Ok(());
        }
        let mut writes = self.writes.lock().unwrap_or_else(PoisonError::into_inner);
        if *writes >= READ_LOG_ENTRIES {
            return Err(io::Error::other(
                "The reference-material read history is full. Start a new try.",
            ));
        }
        let relative = Path::new(READS).join(format!("{}.jsonl", self.node_key));
        let mut file =
            match open_private_file(&self.run_dir, &relative, PrivateFileAccess::CreateAppend) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    open_private_file(&self.run_dir, &relative, PrivateFileAccess::Append)?
                }
                Err(error) => return Err(error),
            };
        let mut line = serde_json::to_vec(&Opened {
            schema: 1,
            address: logical.clone(),
            bytes,
        })
        .map_err(io::Error::other)?;
        line.push(b'\n');
        let current = file.metadata()?.len();
        if current.saturating_add(line.len() as u64) > READ_LOG_BYTES as u64 {
            return Err(io::Error::other(
                "The reference-material read history is full. Start a new try.",
            ));
        }
        file.write_all(&line)?;
        file.sync_data()?;
        *writes += 1;
        Ok(())
    }
}

pub fn read_bound(run_dir: &Path) -> io::Result<Option<Snapshot>> {
    let root = PublicationRoot::open(run_dir)?;
    let saved: Value = serde_json::from_slice(&bounded_read(
        &root,
        Path::new("run.json"),
        64 * 1024 * 1024,
    )?)
    .map_err(io::Error::other)?;
    let Some(binding) = saved
        .get("context_sources")
        .filter(|value| !value.is_null())
    else {
        return Ok(None);
    };
    let binding: Binding = serde_json::from_value(binding.clone())
        .map_err(|_| unavailable("the saved package binding cannot be read"))?;
    let snapshot = read_package(run_dir)?;
    if snapshot.binding()? != binding {
        return Err(unavailable("the package does not match this run's record"));
    }
    root.validate_path_identity(run_dir)?;
    Ok(Some(snapshot))
}

fn read_package(run_dir: &Path) -> io::Result<Snapshot> {
    let root = PublicationRoot::open(run_dir)?;
    let manifest: Manifest = serde_json::from_slice(&bounded_private_read(
        &root,
        Path::new(MANIFEST),
        MANIFEST_BYTES,
    )?)
    .map_err(|_| unavailable("the saved package manifest cannot be read"))?;
    let snapshot = Snapshot {
        manifest: Box::new(manifest),
        inputs: Vec::new(),
        source_dir: Some(Arc::new(run_dir.to_path_buf())),
        _source_holds: None,
    };
    snapshot.validate()?;
    for item in &snapshot.manifest.items {
        for file in [&item.text, &item.preview, &item.agent_image]
            .into_iter()
            .flatten()
        {
            verify_stored(&root, file)?;
        }
    }
    root.validate_path_identity(run_dir)?;
    Ok(snapshot)
}

/// Wspólny zamek chroni pakiet od podglądu replay do końca kopiowania.
pub(crate) fn hold(run_dir: &Path) -> io::Result<File> {
    package_lock(run_dir, false)
}

pub(crate) fn is_held(run_dir: &Path) -> io::Result<bool> {
    if !run_dir.join(MANIFEST).is_file() {
        return Ok(false);
    }
    match package_lock(run_dir, true) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(true),
        Err(error) => Err(error),
    }
}

/// Trzyma wyłączność od ostatniego sprawdzenia aż po usunięcie katalogu biegu.
pub(crate) fn removal_guard(run_dir: &Path) -> io::Result<Option<File>> {
    if !run_dir.join(MANIFEST).is_file() {
        return Ok(None);
    }
    package_lock(run_dir, true).map(Some)
}

fn package_lock(run_dir: &Path, exclusive: bool) -> io::Result<File> {
    let root = PublicationRoot::open(run_dir)?;
    let relative = Path::new(READER_LOCK);
    let opened = match root
        .open_private_existing(relative, PrivateFileModePolicy::ExactOwnerOnly)
        .map_err(io::Error::other)?
    {
        Some(opened) => opened,
        None => match root.create_private(relative) {
            Ok(opened) => opened,
            Err(_) => root
                .open_private_existing(relative, PrivateFileModePolicy::ExactOwnerOnly)
                .map_err(io::Error::other)?
                .ok_or_else(|| unavailable("the package reader lock could not be created"))?,
        },
    };
    let file = opened.file().try_clone()?;
    let locked = if exclusive {
        file.try_lock()
    } else {
        file.try_lock_shared()
    };
    locked.map_err(|_| {
        io::Error::new(
            io::ErrorKind::WouldBlock,
            "This run's reference materials are being read or copied. Keep the run until that finishes.",
        )
    })?;
    root.validate_path_identity(run_dir)?;
    if !root
        .validate_private_identity(relative, opened.identity())
        .map_err(io::Error::other)?
    {
        return Err(unavailable("the package reader lock changed identity"));
    }
    Ok(file)
}

fn read_opened(run_dir: &Path, node_key: &str) -> io::Result<Vec<Opened>> {
    normal_component(node_key)?;
    let root = PublicationRoot::open(run_dir)?;
    let relative = Path::new(READS).join(format!("{node_key}.jsonl"));
    let bytes = match root.open_private_existing(&relative, PrivateFileModePolicy::ExactOwnerOnly) {
        Ok(Some(mut file)) => {
            let mut bytes = Vec::new();
            file.file_mut()
                .take(READ_LOG_BYTES as u64 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() > READ_LOG_BYTES {
                return Err(unavailable("the read history exceeds its byte limit"));
            }
            bytes
        }
        Ok(None) => return Ok(Vec::new()),
        Err(error) => return Err(io::Error::other(error)),
    };
    // 2026-09-08 (CT-06) — prywatny plik sprawdzony przez deskryptor nie wystarcza, jeśli
    // nazwa katalogu biegu została podmieniona podczas odczytu historii.
    root.validate_path_identity(run_dir)?;
    let mut out = Vec::new();
    for line in bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        if out.len() >= READ_LOG_ENTRIES {
            return Err(unavailable("the read history exceeds its entry limit"));
        }
        let record: Opened = serde_json::from_slice(line)
            .map_err(|_| unavailable("the read history cannot be read"))?;
        if record.schema != 1 {
            return Err(unavailable("the read history format is not supported"));
        }
        out.push(record);
    }
    Ok(out)
}

fn measured(
    input: Option<MaterialInput>,
    base: &Path,
    suffix: &str,
    inputs: &mut Vec<CopyInput>,
) -> io::Result<Option<StoredFile>> {
    let Some(input) = input else {
        return Ok(None);
    };
    let (bytes, digest) = measure(&input)?;
    let relative = base.join(suffix).to_string_lossy().into_owned();
    let stored = StoredFile {
        relative,
        digest,
        bytes,
    };
    inputs.push(CopyInput {
        stored: stored.clone(),
        input,
    });
    Ok(Some(stored))
}

fn measure(input: &MaterialInput) -> io::Result<(usize, String)> {
    let (mut reader, root) = input_reader(input)?;
    let mut hasher = Sha256::new();
    let mut total = 0usize;
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES].into_boxed_slice();
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read)
            .ok_or_else(|| io::Error::other("Reference material is too large to measure."))?;
        hasher.update(&buffer[..read]);
    }
    if let Some((root, path)) = root {
        root.validate_path_identity(&path)?;
    }
    Ok((total, format!("{:x}", hasher.finalize())))
}

fn copy_input(root: &PublicationRoot, input: &CopyInput) -> io::Result<()> {
    let relative = Path::new(&input.stored.relative);
    let mut target = root.create_private(relative).map_err(io::Error::other)?;
    let (mut source, source_root) = input_reader(&input.input)?;
    let mut hasher = Sha256::new();
    let mut total = 0usize;
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES].into_boxed_slice();
    loop {
        let read = source.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        target.file_mut().write_all(&buffer[..read])?;
        hasher.update(&buffer[..read]);
        total = total
            .checked_add(read)
            .ok_or_else(|| io::Error::other("Reference material grew while it was copied."))?;
    }
    target.file().sync_all()?;
    if let Some((source_root, path)) = source_root {
        source_root.validate_path_identity(&path)?;
    }
    if total != input.stored.bytes || format!("{:x}", hasher.finalize()) != input.stored.digest {
        return Err(io::Error::other(
            "A selected reference source changed while it was frozen. Nothing started.",
        ));
    }
    if !root
        .validate_private_identity(relative, target.identity())
        .map_err(io::Error::other)?
    {
        return Err(unavailable("a copied source changed before publication"));
    }
    Ok(())
}

fn stored_files(manifest: &Manifest) -> impl Iterator<Item = &StoredFile> {
    manifest.items.iter().flat_map(|item| {
        [&item.text, &item.preview, &item.agent_image]
            .into_iter()
            .flatten()
    })
}

fn copy_stored(
    source: &PublicationRoot,
    destination: &PublicationRoot,
    stored: &StoredFile,
) -> io::Result<()> {
    let relative = Path::new(&stored.relative);
    if destination
        .open_private_existing(relative, PrivateFileModePolicy::ExactOwnerOnly)
        .map_err(io::Error::other)?
        .is_some()
    {
        return verify_stored(destination, stored);
    }
    let mut input = source
        .open_private_existing(relative, PrivateFileModePolicy::ExactOwnerOnly)
        .map_err(io::Error::other)?
        .ok_or_else(|| unavailable("a package source is missing while it is copied"))?;
    let mut output = destination
        .create_private(relative)
        .map_err(io::Error::other)?;
    let mut hasher = Sha256::new();
    let mut total = 0usize;
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES].into_boxed_slice();
    loop {
        let read = input.file_mut().read(&mut buffer)?;
        if read == 0 {
            break;
        }
        output.file_mut().write_all(&buffer[..read])?;
        hasher.update(&buffer[..read]);
        total = total
            .checked_add(read)
            .ok_or_else(|| unavailable("a package source grew while it was copied"))?;
    }
    output.file().sync_all()?;
    if total != stored.bytes || format!("{:x}", hasher.finalize()) != stored.digest {
        return Err(unavailable(
            "a package source changed or is incomplete while it is copied",
        ));
    }
    if !destination
        .validate_private_identity(relative, output.identity())
        .map_err(io::Error::other)?
    {
        return Err(unavailable(
            "a copied package source changed before publication",
        ));
    }
    Ok(())
}

fn input_reader(input: &MaterialInput) -> io::Result<InputReader> {
    match input {
        MaterialInput::Bytes(bytes) => Ok((Box::new(Cursor::new(bytes.clone())), None)),
        MaterialInput::File(path) => {
            let parent = path.parent().ok_or_else(|| {
                io::Error::other("A selected reference source has no containing folder.")
            })?;
            let name = path
                .file_name()
                .ok_or_else(|| io::Error::other("A selected reference source has no file name."))?;
            let root = PublicationRoot::open(parent)?;
            let file = root.open_regular_file(Path::new(name))?;
            Ok((Box::new(file), Some((root, parent.to_path_buf()))))
        }
    }
}

fn verify_stored(root: &PublicationRoot, stored: &StoredFile) -> io::Result<()> {
    valid_digest(&stored.digest)?;
    let mut file = root
        .open_private_existing(
            Path::new(&stored.relative),
            PrivateFileModePolicy::ExactOwnerOnly,
        )
        .map_err(io::Error::other)?
        .ok_or_else(|| unavailable("a package source is missing"))?;
    let mut hasher = Sha256::new();
    let mut total = 0usize;
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES].into_boxed_slice();
    loop {
        let read = file.file_mut().read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read)
            .ok_or_else(|| unavailable("a package source is too large"))?;
        hasher.update(&buffer[..read]);
    }
    if total != stored.bytes || format!("{:x}", hasher.finalize()) != stored.digest {
        return Err(unavailable("a package source changed or is incomplete"));
    }
    Ok(())
}

fn bounded_read(root: &PublicationRoot, relative: &Path, limit: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    root.open_regular_file(relative)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(unavailable("a package record exceeds its size limit"));
    }
    Ok(bytes)
}

fn bounded_private_read(
    root: &PublicationRoot,
    relative: &Path,
    limit: usize,
) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    root.open_private_existing(relative, PrivateFileModePolicy::ExactOwnerOnly)
        .map_err(io::Error::other)?
        .ok_or_else(|| unavailable("a private package record is missing"))?
        .file_mut()
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(unavailable("a package record exceeds its size limit"));
    }
    Ok(bytes)
}

fn item_path(address: &Address) -> PathBuf {
    let item_digest = digest(address.item_id.as_bytes());
    Path::new("context-sources")
        .join(&address.set_id)
        .join(&address.version)
        .join(format!(
            "{}-{}",
            safe_label(&address.item_id),
            &item_digest[..12]
        ))
}

fn evidence_reference(item: &ItemRecord) -> String {
    item.text
        .as_ref()
        .or(item.agent_image.as_ref())
        .or(item.preview.as_ref())
        .map_or_else(
            || item_path(&item.address).to_string_lossy().into_owned(),
            |file| file.relative.clone(),
        )
}

fn access_file(run_dir: &Path, file: &StoredFile) -> AccessFile {
    AccessFile {
        root: run_dir.to_path_buf(),
        relative: PathBuf::from(&file.relative),
        digest: file.digest.clone(),
        bytes: file.bytes,
    }
}

fn public_item_id(address: &Address) -> String {
    format!("{}--{}", address.set_id, address.item_id)
}

fn reader_address(package: &str, node_key: &str, address: &Address) -> String {
    format!(
        "context://{package}/{node_key}/{}@{}/{}",
        address.set_id, address.version, address.item_id
    )
}

fn package_path(value: &str) -> io::Result<()> {
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || !path.starts_with("context-sources")
    {
        return Err(unavailable("a package file points outside context-sources"));
    }
    Ok(())
}

fn normal_component(value: &str) -> io::Result<()> {
    let mut parts = Path::new(value).components();
    if !matches!(parts.next(), Some(Component::Normal(_))) || parts.next().is_some() {
        return Err(unavailable(
            "a saved context address leaves the run package",
        ));
    }
    Ok(())
}

fn safe_label(value: &str) -> String {
    let label = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if label.is_empty() {
        "item".to_owned()
    } else {
        label
    }
}

fn valid_digest(value: &str) -> io::Result<()> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(unavailable("a package source has an invalid fingerprint"))
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn unavailable(detail: &str) -> io::Error {
    io::Error::other(format!(
        "Saved reference materials are unavailable: {detail}."
    ))
}
