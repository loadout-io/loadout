//! WF-13: jeden odczyt całego katalogu skilla. Prozy nie interpretujemy jako listy plików.
//! Rozwiązanie źródła nie jest zgodą na wykonanie helpera ani rozszerzeniem praw agenta.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{Roots, ingest, place};
use crate::durable_file::{DurableFilePublisher, ModePolicy, PRIVATE_FILE_MODE};
use crate::engine::supervisor::{PublicationEntryKind, PublicationRoot};

pub(crate) mod native;

pub const FILE_LIMIT: usize = 10_000;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Entry {
    Directory,
    File { bytes: Vec<u8>, executable: bool },
    Link { target: PathBuf },
}

/// Bajty są ograniczone istniejącym limitem importu. Debug nigdy ich nie wypisuje.
#[derive(Clone, PartialEq, Eq)]
pub struct Bundle {
    entries: BTreeMap<PathBuf, Entry>,
    pub digest: String,
    pub bytes: u64,
}

impl fmt::Debug for Bundle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bundle")
            .field("digest", &self.digest)
            .field("bytes", &self.bytes)
            .field("entries", &self.entries.len())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSkill {
    pub name: String,
    pub source: PathBuf,
    pub bundle: Bundle,
}

const DELIVERED: &str = "delivered-skills.json";

#[derive(Debug, Serialize, Deserialize)]
struct DeliveredRecord {
    name: String,
    source: PathBuf,
    digest: String,
    bytes: u64,
}

/// Oryginalna source jest wyłącznie pochodzeniem; nigdy nie jest ponownie czytana.
pub fn read_delivered(plugin: &Path) -> io::Result<Vec<ResolvedSkill>> {
    let root = PublicationRoot::open(plugin)?;
    let mut bytes = Vec::new();
    root.open_regular_file(Path::new(DELIVERED))?
        .take(ingest::FILE_CAP + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > ingest::FILE_CAP {
        return Err(refusal("The saved skill list exceeds its read limit."));
    }
    let records: Vec<DeliveredRecord> = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    let mut names = BTreeSet::new();
    let mut result = Vec::new();
    for record in records {
        if !place::is_slug(&record.name) || !names.insert(record.name.clone()) {
            return Err(refusal(
                "The saved skill list contains an invalid or repeated name.",
            ));
        }
        let source = plugin.join("skills").join(&record.name);
        let bundle = Bundle::read(&source)?;
        if bundle.digest != record.digest || bundle.bytes != record.bytes {
            return Err(refusal(format!(
                "Saved resources for skill {} changed or are incomplete.",
                record.name
            )));
        }
        result.push(ResolvedSkill {
            name: record.name,
            source: record.source,
            bundle,
        });
    }
    root.validate_path_identity(plugin)?;
    Ok(result)
}

pub fn write_delivered(plugin: &Path, skills: &[ResolvedSkill]) -> io::Result<()> {
    let records = skills
        .iter()
        .map(|skill| DeliveredRecord {
            name: skill.name.clone(),
            source: skill.source.clone(),
            digest: skill.bundle.digest.clone(),
            bytes: skill.bundle.bytes,
        })
        .collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&records).map_err(io::Error::other)?;
    if bytes.len() as u64 > ingest::FILE_CAP {
        return Err(refusal("The saved skill list exceeds its read limit."));
    }
    DurableFilePublisher::new(plugin)
        .atomic_create_if_absent(
            &plugin.join(DELIVERED),
            &bytes,
            ModePolicy::Exact(PRIVATE_FILE_MODE),
        )
        .map_err(super::super::durable_file::PublishError::into_io)?;
    let actual = read_delivered(plugin)?;
    if actual != skills {
        return Err(refusal(
            "The saved skill list does not describe the files delivered.",
        ));
    }
    Ok(())
}

/* `Deserialize` dołożone 2026-09-07: `SkillSource` jedzie od tego dnia w polu `InstalledWire`,
 * a tamta struktura deserializuje się w testach granicy. */
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSource {
    pub path: PathBuf,
    pub digest: Option<String>,
    pub bytes: Option<u64>,
    pub files: Option<usize>,
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSources {
    pub name: String,
    pub requires_choice: bool,
    pub sources: Vec<SkillSource>,
}

/// Jawne źródło musi być jedną ze znanych półek TEGO projektu/użytkownika.
pub fn resolve(roots: &Roots, name: &str, selected: Option<&Path>) -> io::Result<ResolvedSkill> {
    if !place::is_slug(name) {
        return Err(refusal("The skill name is not valid."));
    }
    let candidates = place::shelves_of(roots, name);
    if let Some(selected) = selected {
        if !candidates.iter().any(|candidate| candidate == selected) {
            return Err(refusal(format!(
                "The selected source for {name} is not a known skill folder in this project or library."
            )));
        }
        return from_source(name, selected);
    }
    let mut found = Vec::new();
    let mut first_error = None;
    for candidate in candidates {
        match std::fs::symlink_metadata(&candidate) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
            Ok(_) => (),
        }
        match from_source(name, &candidate) {
            Ok(skill) => found.push(skill),
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    let Some(first) = found.first() else {
        return Err(first_error.unwrap_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("No readable source for skill {name} was found."),
            )
        }));
    };
    // ODMOWA NAZYWA KATALOGI, BO WYBÓR, O KTÓRY PROSI, JEST WYBOREM MIĘDZY NIMI. Samo „wybierz
    // kopię" zostawia człowieka z pytaniem, na które produkt trzyma odpowiedź w ręku: które
    // foldery się rozjechały. Bez nich jedyny ruch, jaki zostaje, to obejść po kolei pięć półek
    // (`place::shelves_of`) i porównać je ręcznie — a to jest praca, którą Loadout właśnie
    // wykonał. To ta sama reguła, co przy [`super::Why::WouldWriteIntoYourFolder`]
    // i [`super::place::Discovery::NotSeen::looked_in`] — człowiek szuka ścieżki, nie werdyktu —
    // i dokładnie to, co ścieżka SUKCESU mówi już przez `Whence::also`.
    //
    // Kolejność jest kolejnością pytania, czyli najbliższa katalogowi pracy pierwsza: ta sama, po
    // której człowiek pozna, którą kopię dostałby bez wyboru.
    if found.iter().any(|skill| skill.bundle != first.bundle) {
        let places: Vec<String> = found
            .iter()
            .map(|skill| skill.source.display().to_string())
            .collect();
        return Err(refusal(format!(
            "Different copies of skill {name} were found in {}. Choose which copy of this skill \
             to use.",
            places.join(", ")
        )));
    }
    Ok(found.remove(0))
}

pub fn sources(roots: &Roots, names: &[String]) -> io::Result<Vec<SkillSources>> {
    let mut result = Vec::new();
    for name in names.iter().collect::<BTreeSet<_>>() {
        if !place::is_slug(name) {
            return Err(refusal("The skill name is not valid."));
        }
        let mut sources = Vec::new();
        let mut digests = BTreeSet::new();
        for path in place::shelves_of(roots, name) {
            match std::fs::symlink_metadata(&path) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
                Ok(_) => (),
            }
            let source = match from_source(name, &path) {
                Ok(skill) => {
                    digests.insert(skill.bundle.digest.clone());
                    SkillSource {
                        path,
                        digest: Some(skill.bundle.digest),
                        bytes: Some(skill.bundle.bytes),
                        files: Some(skill.bundle.entries.len()),
                        available: true,
                        reason: None,
                    }
                }
                Err(error) => SkillSource {
                    path,
                    digest: None,
                    bytes: None,
                    files: None,
                    available: false,
                    reason: Some(error.to_string()),
                },
            };
            sources.push(source);
        }
        result.push(SkillSources {
            name: name.clone(),
            requires_choice: digests.len() > 1,
            sources,
        });
    }
    Ok(result)
}

pub fn choices(
    extra: &serde_json::Map<String, serde_json::Value>,
) -> io::Result<BTreeMap<String, PathBuf>> {
    match extra.get("skillSources") {
        None | Some(serde_json::Value::Null) => Ok(BTreeMap::new()),
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|_| refusal("Skill sources must map skill names to known source folders.")),
    }
}

/// Cudzy katalog przeczytany w całości i **nie** oceniony naszymi regułami autorskimi.
///
/// Borrow CYTUJE umiejętność gospodarza. Ekran wyboru pokazuje każdy katalog z `SKILL.md`
/// (`inherit::scan::skills`), więc plik bez front-mattera jest tam normalnym wpisem, a nie
/// awarią — a odmowa za brakujące `name:`/`description:` w CUDZYM repozytorium zabiera cały
/// bieg za pole, którego tam nigdy nie było (niezmiennik 5). Kompletność rozstrzyga dalej
/// `Bundle::read`: bez czytelnego `SKILL.md` nadal jest odmowa.
pub fn borrowed_from_source(name: &str, source: &Path) -> io::Result<ResolvedSkill> {
    Ok(ResolvedSkill {
        name: name.to_owned(),
        source: source.to_path_buf(),
        bundle: Bundle::read(source)?,
    })
}

/// Ta sama lektura plus reguły autorskie — dla umiejętności, które Loadout POSIADA.
pub fn from_source(name: &str, source: &Path) -> io::Result<ResolvedSkill> {
    let skill = borrowed_from_source(name, source)?;
    place::validate_usable(name, &place::read_doc(skill.bundle.skill_text()?))
        .map_err(|problems| io::Error::new(io::ErrorKind::InvalidData, problems.join("; ")))?;
    Ok(skill)
}

impl Bundle {
    pub fn read(source: &Path) -> io::Result<Self> {
        let root = PublicationRoot::open(source)?;
        let first = read_entries(&root)?;
        let second = read_entries(&root)?;
        root.validate_path_identity(source)?;
        if first != second {
            return Err(refusal(
                "The selected skill changed while it was being read. Try again.",
            ));
        }
        Self::from_entries(first)
    }

    fn from_entries(entries: BTreeMap<PathBuf, Entry>) -> io::Result<Self> {
        for (path, entry) in &entries {
            if matches!(entry, Entry::Link { .. }) {
                resolve_link(&entries, path)?;
            }
        }
        let bytes = entries
            .values()
            .map(|entry| match entry {
                Entry::File { bytes, .. } => bytes.len() as u64,
                _ => 0,
            })
            .sum();
        let digest = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&entries).map_err(io::Error::other)?)
        );
        let bundle = Self {
            entries,
            digest,
            bytes,
        };
        bundle.skill_text()?;
        Ok(bundle)
    }

    /// Lista na kartę importu, bez otwierania źródeł drugi raz. Link pozostaje zasobem.
    pub(super) fn resources(&self, source: &Path) -> Vec<super::BundledFile> {
        self.entries
            .iter()
            .filter(|(relative, entry)| {
                relative.as_path() != Path::new("SKILL.md") && !matches!(entry, Entry::Directory)
            })
            .map(|(relative, _)| super::BundledFile {
                relative: relative.clone(),
                source: source.join(relative),
            })
            .collect()
    }

    /// Emiter normalizuje wyłącznie dokument; pozostałe zasoby pozostają przeglądanymi bajtami.
    pub(super) fn with_document(&self, text: &str) -> io::Result<Self> {
        if text.len() as u64 > ingest::FILE_CAP {
            return Err(refusal("The emitted SKILL.md exceeds its file limit."));
        }
        let mut entries = self.entries.clone();
        let executable = matches!(
            entries.get(Path::new("SKILL.md")),
            Some(Entry::File {
                executable: true,
                ..
            })
        );
        entries.insert(
            PathBuf::from("SKILL.md"),
            Entry::File {
                bytes: text.as_bytes().to_vec(),
                executable,
            },
        );
        let bundle = Self::from_entries(entries)?;
        if bundle.bytes > ingest::TOTAL_CAP {
            return Err(refusal("The emitted skill exceeds its total size limit."));
        }
        Ok(bundle)
    }

    pub fn skill_text(&self) -> io::Result<&str> {
        match self.entries.get(Path::new("SKILL.md")) {
            Some(Entry::File { bytes, .. }) => std::str::from_utf8(bytes).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "The selected SKILL.md is not UTF-8 text.",
                )
            }),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The selected skill needs a readable regular SKILL.md.",
            )),
        }
    }

    /// Publikacja do własnej, nowej ścieżki. Istniejącego odmiennego katalogu nie nadpisuje.
    pub fn materialize(&self, into: &Path) -> io::Result<()> {
        if !into.is_absolute() {
            return Err(refusal("A skill copy needs an absolute destination."));
        }
        match std::fs::symlink_metadata(into) {
            Ok(_) => {
                if Self::read(into)? == *self {
                    return Ok(());
                }
                return Err(refusal(
                    "A different skill already occupies the selected destination.",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
        let mut ancestor = into
            .parent()
            .ok_or_else(|| refusal("The skill destination has no parent."))?;
        while !ancestor.exists() {
            ancestor = ancestor
                .parent()
                .ok_or_else(|| refusal("The skill destination has no existing parent."))?;
        }
        let parent = PublicationRoot::open(ancestor)?;
        parent.ensure_directory(
            into.strip_prefix(ancestor).map_err(io::Error::other)?,
            0o700,
        )?;
        parent.validate_path_identity(ancestor)?;
        let root = PublicationRoot::open(into)?;
        self.write_entries(into, &root, false)?;
        root.validate_path_identity(into)?;
        if Self::read(into)? != *self {
            return Err(refusal("The selected skill copy is incomplete."));
        }
        Ok(())
    }

    /// Jawna instalacja z istniejącego planu zachowuje aktualizację własnych kopii.
    /// Źródło jest już zamrożone; wszystkie zmiany celu są względem utrzymanego roota.
    pub(super) fn install(&self, into: &Path) -> io::Result<()> {
        match std::fs::symlink_metadata(into) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return self.materialize(into),
            Err(error) => return Err(error),
            Ok(_) => (),
        }
        let root = PublicationRoot::open(into)?;
        let previous = read_entries(&root)?;
        // Najpierw dzieci, potem katalog. Nie usuwamy regularnego pliku, który publisher
        // może zastąpić atomowo. Usunięcie wymaga tej samej tożsamości inode'u.
        for (relative, old) in previous.iter().rev() {
            let keep = match (old, self.entries.get(relative)) {
                (Entry::Directory, Some(Entry::Directory))
                | (Entry::File { .. }, Some(Entry::File { .. })) => true,
                (Entry::Link { target: old }, Some(Entry::Link { target: new })) => old == new,
                _ => false,
            };
            if !keep {
                let expected = root
                    .entry_identity(relative)?
                    .ok_or_else(|| refusal("The skill installation changed before update."))?;
                if !root.remove_entry_if_identity(relative, expected)? {
                    return Err(refusal("The skill installation changed during update."));
                }
            }
        }
        self.write_entries(into, &root, true)?;
        root.validate_path_identity(into)?;
        if Self::read(into)? != *self {
            return Err(refusal("The updated skill copy is incomplete."));
        }
        Ok(())
    }

    fn write_entries(&self, into: &Path, root: &PublicationRoot, replace: bool) -> io::Result<()> {
        for (relative, entry) in &self.entries {
            match entry {
                Entry::Directory => root.ensure_directory(relative, 0o700)?,
                Entry::File { bytes, executable } => {
                    if let Some(parent) = relative.parent() {
                        root.ensure_directory(parent, 0o700)?;
                    }
                    let mode = if *executable {
                        0o700
                    } else {
                        PRIVATE_FILE_MODE
                    };
                    let publisher = DurableFilePublisher::new(into);
                    let result = if replace {
                        publisher.atomic_replace(
                            &into.join(relative),
                            bytes,
                            ModePolicy::Exact(mode),
                        )
                    } else {
                        publisher.atomic_create_if_absent(
                            &into.join(relative),
                            bytes,
                            ModePolicy::Exact(mode),
                        )
                    };
                    result.map_err(super::super::durable_file::PublishError::into_io)?;
                }
                Entry::Link { target } => match root.entry_identity(relative)? {
                    None => root.create_link(relative, target)?,
                    Some((PublicationEntryKind::Symlink, _))
                        if replace && root.read_link(relative)? == *target => {}
                    Some(_) => {
                        return Err(refusal(
                            "A different entry occupies the selected skill link.",
                        ));
                    }
                },
            }
        }
        Ok(())
    }
}

fn read_entries(root: &PublicationRoot) -> io::Result<BTreeMap<PathBuf, Entry>> {
    let mut entries = BTreeMap::new();
    let mut pending = vec![PathBuf::new()];
    let mut total = 0u64;
    while let Some(directory) = pending.pop() {
        for found in root.list_directory(&directory)? {
            if entries.len() >= FILE_LIMIT {
                return Err(refusal(format!(
                    "The skill contains more than {FILE_LIMIT} entries."
                )));
            }
            let path = directory.join(found.name);
            let entry = match found.kind {
                PublicationEntryKind::Directory => {
                    pending.push(path.clone());
                    Entry::Directory
                }
                PublicationEntryKind::Regular => {
                    let file = root.open_regular_file(&path)?;
                    let executable = crate::engine::supervisor::executable_bits(&file.metadata()?);
                    let mut bytes = Vec::new();
                    file.take(ingest::FILE_CAP + 1).read_to_end(&mut bytes)?;
                    if bytes.len() as u64 > ingest::FILE_CAP {
                        return Err(refusal(format!(
                            "Skill file {} exceeds the {} byte limit.",
                            path.display(),
                            ingest::FILE_CAP
                        )));
                    }
                    total = total.saturating_add(bytes.len() as u64);
                    if total > ingest::TOTAL_CAP {
                        return Err(refusal(format!(
                            "The complete skill exceeds the {} byte limit.",
                            ingest::TOTAL_CAP
                        )));
                    }
                    Entry::File { bytes, executable }
                }
                PublicationEntryKind::Symlink => Entry::Link {
                    target: root.read_link(&path)?,
                },
                PublicationEntryKind::Other => {
                    return Err(refusal(format!(
                        "Skill resource {} is not a regular file, directory, or contained link.",
                        path.display()
                    )));
                }
            };
            entries.insert(path, entry);
        }
    }
    Ok(entries)
}

fn resolve_link(entries: &BTreeMap<PathBuf, Entry>, path: &Path) -> io::Result<PathBuf> {
    let mut current = path.to_path_buf();
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(current.clone()) {
            return Err(refusal(format!(
                "Skill resource {} contains a link cycle.",
                path.display()
            )));
        }
        let mut prefix = PathBuf::new();
        let components = current.components().collect::<Vec<_>>();
        let mut replaced = false;
        for (index, component) in components.iter().enumerate() {
            prefix.push(component.as_os_str());
            match entries.get(&prefix) {
                Some(Entry::Link { target }) => {
                    let mut next = normalize(prefix.parent().unwrap_or(Path::new("")), target)?;
                    for rest in &components[index + 1..] {
                        next.push(rest.as_os_str());
                    }
                    current = next;
                    replaced = true;
                    break;
                }
                Some(Entry::Directory) => (),
                Some(Entry::File { .. }) if index + 1 == components.len() => (),
                _ => {
                    return Err(refusal(format!(
                        "Skill resource {} points to a missing resource.",
                        path.display()
                    )));
                }
            }
        }
        if !replaced {
            return Ok(current);
        }
    }
}

fn normalize(parent: &Path, target: &Path) -> io::Result<PathBuf> {
    let mut result = parent.to_path_buf();
    for component in target.components() {
        match component {
            Component::Normal(value) => result.push(value),
            Component::CurDir => (),
            Component::ParentDir if result.pop() => (),
            _ => return Err(refusal("A skill link points outside its selected folder.")),
        }
    }
    Ok(result)
}

fn refusal(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}
