//! WF-23: źródłowe pliki pamięci, nie gotowy prompt. Odbiorcą jest fizyczny `node_key`.
//! 2026-09-06: sam odcisk reguły pozwalał opisać historię, ale nie powtórzyć jej bez hosta.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::memory::{NoteAddress, NotePlace};
use crate::durable_file::{DurableFilePublisher, ModePolicy, PRIVATE_FILE_MODE};
use crate::engine::supervisor::PublicationRoot;

const LIMIT: usize = 512 * 1024;
const MANIFEST: &str = "memory-sources/manifest.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Binding {
    pub id: String,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SourceRecord {
    address: NoteAddress,
    digest: String,
    bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    schema: u32,
    id: String,
    sources: Vec<SourceRecord>,
    nodes: BTreeMap<String, Vec<NoteAddress>>,
}

/// Debug celowo pomija treść źródła; run.json również zawiera tylko adresy i odciski.
#[derive(Clone)]
pub(crate) struct Source {
    pub address: NoteAddress,
    pub raw: String,
}

impl fmt::Debug for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Source")
            .field("address", &self.address)
            .field("bytes", &self.raw.len())
            .finish()
    }
}

#[derive(Clone)]
pub(crate) struct Snapshot {
    manifest: Manifest,
    sources: Vec<Source>,
}

impl fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemorySources")
            .field("id", &self.manifest.id)
            .field("sources", &self.sources.len())
            .field("nodes", &self.manifest.nodes.len())
            .finish()
    }
}

impl Snapshot {
    pub fn new(
        mut sources: Vec<Source>,
        nodes: BTreeMap<String, Vec<NoteAddress>>,
    ) -> io::Result<Self> {
        sources.sort_by(|left, right| left.address.cmp(&right.address));
        let snapshot = Self {
            manifest: Manifest {
                schema: 1,
                id: uuid::Uuid::now_v7().to_string(),
                sources: sources
                    .iter()
                    .map(|one| SourceRecord {
                        address: one.address.clone(),
                        digest: digest(one.raw.as_bytes()),
                        bytes: one.raw.len(),
                    })
                    .collect(),
                nodes,
            },
            sources,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn binding(&self) -> io::Result<Binding> {
        Ok(Binding {
            id: self.manifest.id.clone(),
            digest: digest(&serde_json::to_vec(&self.manifest).map_err(io::Error::other)?),
        })
    }

    fn validate(&self) -> io::Result<()> {
        if self.manifest.schema != 1
            || uuid::Uuid::parse_str(&self.manifest.id).is_err()
            || self.sources.len() != self.manifest.sources.len()
        {
            return Err(unavailable("the saved memory package is not supported"));
        }
        let mut total = 0usize;
        let mut addresses = BTreeSet::new();
        for (source, record) in self.sources.iter().zip(&self.manifest.sources) {
            normal_id(&record.address.id)?;
            total = total.checked_add(source.raw.len()).ok_or_else(too_large)?;
            if total > LIMIT {
                return Err(too_large());
            }
            if source.address != record.address
                || source.raw.len() != record.bytes
                || digest(source.raw.as_bytes()) != record.digest
                || !addresses.insert(&source.address)
            {
                return Err(unavailable(
                    "a saved memory source changed or is incomplete",
                ));
            }
            // Parser bierze wirtualną nazwę oryginalnej notatki; nazwa pliku z SHA nie jest ID.
            let note =
                crate::memory::notes::parse_note(&relative_path(&source.address), &source.raw)
                    .map_err(|_| unavailable("a saved memory source cannot be read"))?;
            let valid_scope = match source.address.place {
                NotePlace::Library => matches!(
                    note.scope,
                    crate::memory::notes::Scope::Everywhere
                        | crate::memory::notes::Scope::ThisAgent
                ),
                NotePlace::Project => note.scope == crate::memory::notes::Scope::ThisProject,
            };
            if !valid_scope || note.status != crate::memory::notes::Status::InUse {
                return Err(unavailable(
                    "a saved memory source has an unsupported scope",
                ));
            }
        }
        let mut reached = BTreeSet::new();
        for (key, selected) in &self.manifest.nodes {
            normal_id(key)?;
            let unique: BTreeSet<_> = selected.iter().collect();
            if unique.len() != selected.len()
                || unique.iter().any(|address| !addresses.contains(*address))
            {
                return Err(unavailable("a saved step points to missing memory sources"));
            }
            reached.extend(unique);
        }
        if reached != addresses {
            return Err(unavailable(
                "the saved memory sources have no matching recipients",
            ));
        }
        Ok(())
    }

    /// Wspólny parser i compositor dostają te same źródła, lecz wyłącznie wskazanego węzła.
    /// Żadnego odczytu tych wirtualnych ścieżek ani zapisu do dzisiejszych notatek.
    pub fn notes(
        &self,
        home: &Path,
        project: &Path,
        node: Option<&str>,
    ) -> io::Result<Vec<(crate::memory::notes::Note, String)>> {
        let selected = node
            .map(|key| {
                self.manifest
                    .nodes
                    .get(key)
                    .ok_or_else(|| unavailable("this physical step has no saved memory selection"))
            })
            .transpose()?;
        self.sources
            .iter()
            .filter(|source| selected.is_none_or(|list| list.contains(&source.address)))
            .map(|source| {
                let root = match source.address.place {
                    NotePlace::Library => home,
                    NotePlace::Project => project,
                };
                let parsed = crate::memory::notes::parse_note(
                    &root.join(relative_path(&source.address)),
                    &source.raw,
                )
                .map_err(|_| unavailable("a saved memory source cannot be read"))?;
                Ok((parsed, source.raw.clone()))
            })
            .collect()
    }

    pub fn selected_for(&self, node: &str) -> io::Result<&[NoteAddress]> {
        self.manifest
            .nodes
            .get(node)
            .map(Vec::as_slice)
            .ok_or_else(|| unavailable("this physical step has no saved memory selection"))
    }

    /// Skan planu jest punktem zamrożenia; kontrola deskryptorem odmawia podmienionego źródła,
    /// zamiast składać drugi prompt z nowych bajtów. Recorded tej metody nigdy nie wywołuje.
    pub fn verify_live_sources(&self, home: &Path, project: &Path) -> io::Result<()> {
        for source in &self.sources {
            let path = match source.address.place {
                NotePlace::Library => home,
                NotePlace::Project => project,
            };
            let root = PublicationRoot::open(path)?;
            let actual = bounded_read(&root, &relative_path(&source.address), LIMIT)?;
            root.validate_path_identity(path)?;
            if actual != source.raw.as_bytes() {
                return Err(unavailable(
                    "a memory source changed during preparation; start again",
                ));
            }
        }
        Ok(())
    }

    pub fn save_to(&self, run_dir: &Path) -> io::Result<()> {
        self.validate()?;
        let expected = self.binding()?;
        let manifest = serde_json::to_vec(&self.manifest).map_err(io::Error::other)?;
        DurableFilePublisher::new(run_dir)
            .with_publication(|batch| {
                let root = batch.root();
                root.ensure_directory(Path::new("memory-sources/files"), 0o700)
                    .map_err(crate::durable_file::PublishError::from)?;
                for (source, record) in self.sources.iter().zip(&self.manifest.sources) {
                    let relative = source_file(record);
                    match root.open_regular_file(&relative) {
                        Ok(_) => {
                            let bytes = bounded_read(root, &relative, LIMIT)
                                .map_err(crate::durable_file::PublishError::from)?;
                            if bytes != source.raw.as_bytes() {
                                return Err(crate::durable_file::PublishError::from(unavailable(
                                    "a different memory source already exists in this run",
                                )));
                            }
                        }
                        Err(error) if error.kind() == io::ErrorKind::NotFound => {
                            batch.atomic_create_if_absent(
                                &run_dir.join(relative),
                                source.raw.as_bytes(),
                                ModePolicy::Exact(PRIVATE_FILE_MODE),
                            )?;
                        }
                        Err(error) => return Err(error.into()),
                    }
                }
                batch.atomic_create_if_absent(
                    &run_dir.join(MANIFEST),
                    &manifest,
                    ModePolicy::Exact(PRIVATE_FILE_MODE),
                )
            })
            .map_err(super::super::durable_file::PublishError::into_io)?;
        let actual = read_package(run_dir)?;
        if actual.binding()? != expected {
            return Err(unavailable(
                "the saved memory package changed during publication",
            ));
        }
        Ok(())
    }
}

/// Brak wpisu jest legacy wyłącznie bez dawnych opisów pamięci. Usunięty plik nigdy nie
/// zamienia jawnie związanego pakietu w pustą pamięć ani w dzisiejszy katalog.
pub(crate) fn read_bound(run_dir: &Path) -> io::Result<Option<Snapshot>> {
    let root = PublicationRoot::open(run_dir)?;
    let saved: Value = serde_json::from_slice(&bounded_read(
        &root,
        Path::new("run.json"),
        64 * 1024 * 1024,
    )?)
    .map_err(io::Error::other)?;
    let Some(binding) = saved.get("memory_sources").filter(|value| !value.is_null()) else {
        if saved
            .get("memory")
            .and_then(Value::as_array)
            .is_some_and(|notes| !notes.is_empty())
        {
            return Err(unavailable(
                "this run saved memory descriptions but not the source note files",
            ));
        }
        return Ok(None);
    };
    let binding: Binding = serde_json::from_value(binding.clone())
        .map_err(|_| unavailable("the saved memory record cannot be read"))?;
    let snapshot = read_package(run_dir)?;
    if snapshot.binding()? != binding {
        return Err(unavailable(
            "the memory package does not match this run's record",
        ));
    }
    root.validate_path_identity(run_dir)?;
    Ok(Some(snapshot))
}

fn read_package(run_dir: &Path) -> io::Result<Snapshot> {
    let root = PublicationRoot::open(run_dir)?;
    let manifest: Manifest =
        serde_json::from_slice(&bounded_read(&root, Path::new(MANIFEST), 4 * 1024 * 1024)?)
            .map_err(|_| unavailable("the saved memory manifest cannot be read"))?;
    let mut total = 0usize;
    let mut sources = Vec::new();
    for record in &manifest.sources {
        if record.digest.len() != 64
            || !record
                .digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(unavailable(
                "a saved memory source has an invalid fingerprint",
            ));
        }
        total = total.checked_add(record.bytes).ok_or_else(too_large)?;
        if total > LIMIT {
            return Err(too_large());
        }
        let bytes = bounded_read(&root, &source_file(record), record.bytes)?;
        let raw = String::from_utf8(bytes)
            .map_err(|_| unavailable("a saved memory source is not text"))?;
        sources.push(Source {
            address: record.address.clone(),
            raw,
        });
    }
    let snapshot = Snapshot { manifest, sources };
    snapshot.validate()?;
    root.validate_path_identity(run_dir)?;
    Ok(snapshot)
}

fn relative_path(address: &NoteAddress) -> PathBuf {
    let root = match address.place {
        NotePlace::Library => "memory/notes",
        NotePlace::Project => ".loadout/memory/notes",
    };
    Path::new(root).join(format!("{}.md", address.id))
}

fn source_file(record: &SourceRecord) -> PathBuf {
    Path::new("memory-sources/files").join(format!("{}.md", record.digest))
}

fn bounded_read(root: &PublicationRoot, relative: &Path, limit: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    root.open_regular_file(relative)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(too_large());
    }
    Ok(bytes)
}

fn normal_id(value: &str) -> io::Result<()> {
    let mut parts = Path::new(value).components();
    if !matches!(parts.next(), Some(Component::Normal(_))) || parts.next().is_some() {
        return Err(unavailable("a saved memory address is outside its package"));
    }
    Ok(())
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn too_large() -> io::Error {
    io::Error::other("Memory source files exceed the 512 KiB package limit; nothing started.")
}
fn unavailable(detail: &str) -> io::Error {
    io::Error::other(format!("Saved memory is unavailable: {detail}."))
}
