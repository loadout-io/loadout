//! WF-13: pochodzenie natywnej półki. Nazwa `.agents` nigdy sama nie dowodzi własności.
//! Marker kopii utrwala ten opis przed publikacją i po niej. Przerwany zapis nie jest wynikiem.

use super::{Bundle, Entry, refusal};
use crate::durable_file::{DurableFilePublisher, ModePolicy};
use crate::engine::supervisor::{PublicationEntryKind, PublicationIdentity, PublicationRoot};
use crate::skills::{SHELF_THE_OTHER_FIVE_READ, StepSkills};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;
use std::io::{self, Read as _};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct NativeDelivery {
    root: PublicationIdentity,
    pub(crate) complete: bool,
    entries: BTreeMap<PathBuf, OwnedEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OwnedEntry {
    identity: PublicationIdentity,
    kind: Kind,
    digest: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Directory,
    File,
    Link,
}

impl Kind {
    fn publication(self) -> PublicationEntryKind {
        match self {
            Self::Directory => PublicationEntryKind::Directory,
            Self::File => PublicationEntryKind::Regular,
            Self::Link => PublicationEntryKind::Symlink,
        }
    }
}

impl NativeDelivery {
    pub(crate) fn new(cwd: &Path) -> io::Result<Self> {
        Ok(Self {
            root: PublicationRoot::open(cwd)?.identity(),
            complete: true,
            entries: BTreeMap::new(),
        })
    }

    pub(crate) fn needs_cleanup(&self) -> bool {
        !self.complete || !self.entries.is_empty()
    }

    /// Pełna walidacja PRZED pierwszym usunięciem. Równe bajty na innym inode nie są nasze.
    pub(crate) fn validate(&self, cwd: &Path) -> io::Result<()> {
        if !self.complete {
            return Err(refusal(
                "The skill files were not completely prepared. This folder was kept for inspection, not saved as a result.",
            ));
        }
        let root = self.open(cwd)?;
        for (path, expected) in &self.entries {
            Self::validate_entry(&root, path, expected)?;
        }
        root.validate_path_identity(cwd)
    }

    fn open(&self, cwd: &Path) -> io::Result<PublicationRoot> {
        let root = PublicationRoot::open(cwd)?;
        if root.identity() != self.root {
            return Err(refusal(
                "The skill working folder was replaced. Nothing was removed.",
            ));
        }
        Ok(root)
    }

    fn validate_entry(
        root: &PublicationRoot,
        path: &Path,
        expected: &OwnedEntry,
    ) -> io::Result<()> {
        if !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
            || !(path == Path::new(".agents") || path.starts_with(SHELF_THE_OTHER_FIVE_READ))
            || root.entry_identity(path)? != Some((expected.kind.publication(), expected.identity))
        {
            return Err(changed(path));
        }
        let entry = match expected.kind {
            Kind::Directory => Entry::Directory,
            Kind::Link => Entry::Link {
                target: root.read_link(path)?,
            },
            Kind::File => {
                let file = root.open_regular_file(path)?;
                let executable = crate::engine::supervisor::executable_bits(&file.metadata()?);
                let mut bytes = Vec::new();
                file.take(super::ingest::FILE_CAP + 1)
                    .read_to_end(&mut bytes)?;
                if bytes.len() as u64 > super::ingest::FILE_CAP {
                    return Err(changed(path));
                }
                Entry::File { bytes, executable }
            }
        };
        if fingerprint(&entry)? != expected.digest {
            return Err(changed(path));
        }
        Ok(())
    }

    /// Caller ma już trwałe complete=false. Czyta WYŁĄCZNIE zamrożone zasoby kroku.
    pub(crate) fn deliver(&mut self, cwd: &Path, skills: &StepSkills) -> io::Result<()> {
        let root = self.open(cwd)?;
        if skills.names.len() != skills.dirs.len() {
            return Err(refusal("The selected skill files are incomplete."));
        }
        for (name, source) in skills.names.iter().zip(&skills.dirs) {
            if !super::place::is_slug(name) {
                return Err(refusal("The selected skill name is invalid."));
            }
            let bundle = Bundle::read(source)?;
            let destination = Path::new(SHELF_THE_OTHER_FIVE_READ).join(name);
            if root.entry_identity(Path::new(".agents"))?.is_some()
                && root
                    .entry_identity(Path::new(SHELF_THE_OTHER_FIVE_READ))?
                    .is_some()
                && root.entry_identity(&destination)?.is_some()
            {
                // Istniejący równy pakiet jest wejściem człowieka albo wcześniejszą publikacją.
                // Nie nabywamy własności na podstawie nazwy ani równości bajtów.
                if Bundle::read(&cwd.join(&destination))? != bundle {
                    return Err(refusal(
                        "A different skill already occupies the selected destination.",
                    ));
                }
                continue;
            }
            self.directory(&root, Path::new(".agents"))?;
            self.directory(&root, Path::new(SHELF_THE_OTHER_FIVE_READ))?;
            self.directory(&root, &destination)?;
            for (relative, entry) in &bundle.entries {
                let path = destination.join(relative);
                match entry {
                    Entry::Directory => self.directory(&root, &path)?,
                    Entry::File { bytes, executable } => {
                        // Tożsamość pochodzi z temp-fd publishera PRZED rename/link, nie z
                        // ponownego odczytu ścieżki po potencjalnej podmianie.
                        let identity = DurableFilePublisher::new(cwd)
                            .with_publication(|batch| {
                                batch.atomic_create_if_absent_with_identity(
                                    &cwd.join(&path),
                                    bytes,
                                    ModePolicy::Exact(if *executable { 0o700 } else { 0o600 }),
                                )
                            })
                            .map_err(crate::durable_file::PublishError::into_io)?;
                        self.remember(path, Kind::File, identity, entry)?;
                    }
                    Entry::Link { target } => {
                        root.create_link(&path, target)?;
                        let (kind, identity) =
                            root.entry_identity(&path)?.ok_or_else(|| changed(&path))?;
                        if kind != PublicationEntryKind::Symlink {
                            return Err(changed(&path));
                        }
                        self.remember(path, Kind::Link, identity, entry)?;
                    }
                }
            }
        }
        self.complete = true;
        self.validate(cwd)
    }

    fn directory(&mut self, root: &PublicationRoot, path: &Path) -> io::Result<()> {
        match root.entry_identity(path)? {
            Some((PublicationEntryKind::Directory, _)) => Ok(()),
            Some(_) => Err(changed(path)),
            None => {
                let directory = root.create_directory_exclusive(path, 0o700)?;
                self.remember(
                    path.to_path_buf(),
                    Kind::Directory,
                    directory.identity(),
                    &Entry::Directory,
                )
            }
        }
    }

    fn remember(
        &mut self,
        path: PathBuf,
        kind: Kind,
        identity: PublicationIdentity,
        entry: &Entry,
    ) -> io::Result<()> {
        self.entries.insert(
            path,
            OwnedEntry {
                identity,
                kind,
                digest: fingerprint(entry)?,
            },
        );
        Ok(())
    }

    /// Nie jest rekurencyjnym kasowaniem. Nowe pliki agenta nie należą do listy, a katalog
    /// z takimi dziećmi zostaje wynikiem pracy. Caller trzyma wyłączność kopii/deathproof.
    pub(crate) fn clean(&mut self, cwd: &Path) -> io::Result<()> {
        let root = self.open(cwd)?;
        for (path, expected) in self.entries.iter().rev() {
            Self::validate_entry(&root, path, expected)?;
            if matches!(expected.kind, Kind::Directory) && !root.list_directory(path)?.is_empty() {
                continue;
            }
            if !root
                .remove_entry_if_identity(path, (expected.kind.publication(), expected.identity))?
            {
                return Err(changed(path));
            }
        }
        root.validate_path_identity(cwd)?;
        self.entries.clear();
        self.complete = true;
        Ok(())
    }
}

fn changed(path: &Path) -> io::Error {
    refusal(format!(
        "Skill delivery file {} changed after publication. The working folder was kept; its files were not accepted as a result.",
        path.display()
    ))
}

fn fingerprint(entry: &Entry) -> io::Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(entry).map_err(io::Error::other)?)
    ))
}
