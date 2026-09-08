//! Ograniczony czytelnik jednego, zamrożonego zestawu Context.
//!
//! Host otwiera [`ContextShelf`] po identyfikatorze zestawu, a potem wydaje z niego
//! [`ContextAccess`] dla KONKRETNEGO odbiorcy. Model nie podaje katalogu, rewizji ani odbiorcy:
//! ma wyłącznie identyfikatory zwrócone przez [`ContextAccess::list`] i nieprzezroczyste kursory.
//! Ten sam rdzeń obsługuje most i podgląd źródła, więc limit tekstu i wybór pochodnej nie mają
//! drugiej, rozjeżdżającej się implementacji.
//!
//! # Dlaczego kursor jest zapisem hosta, a nie zakodowanym przesunięciem
//!
//! Samo base64 z numerem bajtu da się podrobić i przenieść do innego odbiorcy. Od 2026-09-08
//! kursor jest losowym kluczem do rekordu trzymanego przez zamrożoną półkę: rekord wiąże rodzaj
//! odczytu, odbiorcę, materiał i następny bajt. Dzięki temu cudzy, zmyślony i wskazujący poza
//! przydział kursor kończą trzema różnymi, nazwanymi odmowami.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::Read as _;
use std::ops::Range;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tokio_util::sync::CancellationToken;

use super::sources::{FOR_THE_AGENT, PAGES, READER_TEXT, THUMBNAIL};
use super::{Error, SourceKind, files, limits};

/// Jeden przydział wybrany przez hosta. Model nigdy nie serializuje tego typu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Allotment {
    selector: Selector,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Selector {
    Source(String),
    Page { source: String, number: u32 },
    Bytes { id: String, from: usize, to: usize },
}

impl Allotment {
    /// Całe źródło; dla PDF-a oznacza wszystkie jego przygotowane strony, nigdy oryginał.
    #[must_use]
    pub fn source(id: impl Into<String>) -> Self {
        Self {
            selector: Selector::Source(id.into()),
        }
    }

    /// Jedna przygotowana strona PDF-a. Numer wybiera host, a model dostaje jej gotowe ID.
    #[must_use]
    pub fn page(source: impl Into<String>, number: u32) -> Self {
        Self {
            selector: Selector::Page {
                source: source.into(),
                number,
            },
        }
    }

    /// Fragment materiału wybrany przez hosta, na przykład temat z większego tekstu.
    ///
    /// Granice są bajtowe, bo budżety PLAN §9 są bajtowe. Konstruktor jest publiczny dla
    /// przyszłego resolvera tematów; most nigdy nie wystawia pól `from` ani `to` modelowi.
    #[must_use]
    pub fn bytes(id: impl Into<String>, from: usize, to: usize) -> Self {
        Self {
            selector: Selector::Bytes {
                id: id.into(),
                from,
                to,
            },
        }
    }
}

/// Dlaczego czytelnik odmówił — każdy wariant jest gotowym zdaniem dla człowieka.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Denied {
    UnknownIdentifier,
    NotGranted,
    InvalidCursor,
    OutsideRange,
    DifferentRecipient,
    Expired,
    NoText,
    NoImage,
    ImageTooLarge,
    EmptySearch,
    AnswerTooLarge,
    Unavailable,
}

impl fmt::Display for Denied {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownIdentifier => {
                formatter.write_str("That context item does not exist in this frozen set.")
            }
            Self::NotGranted => {
                formatter.write_str("This context item was not given to this agent.")
            }
            Self::InvalidCursor => formatter
                .write_str("That reading cursor was not issued by Loadout. Start this item again."),
            Self::OutsideRange => formatter
                .write_str("That reading cursor points outside the part given to this agent."),
            Self::DifferentRecipient => {
                formatter.write_str("That reading cursor belongs to a different agent.")
            }
            Self::Expired => formatter
                .write_str("This context access has ended. Start a new try to read it again."),
            Self::NoText => formatter.write_str("This context item has no text to read."),
            Self::NoImage => formatter.write_str("This context item has no picture to show."),
            Self::ImageTooLarge => formatter.write_str(
                "This picture is larger than 5 MiB, so it was not sent. Add a smaller copy.",
            ),
            Self::EmptySearch => {
                formatter.write_str("Write something to search for in this context.")
            }
            Self::AnswerTooLarge => formatter
                .write_str("This context answer cannot fit in one page. Choose a narrower item."),
            Self::Unavailable => formatter.write_str(
                "This context item cannot be read from the frozen material. Start a new try.",
            ),
        }
    }
}

impl std::error::Error for Denied {}

/// Rodzaj pozycji na liście czytelnika.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContextItemKind {
    #[default]
    Text,
    Image,
    Page,
    #[serde(other)]
    Unknown,
}

/// Jedna pozycja, którą odbiorca naprawdę dostał.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextItem {
    pub id: String,
    pub source_id: String,
    pub address: String,
    pub name: String,
    pub description: String,
    pub kind: ContextItemKind,
    pub page: Option<u32>,
    pub pages_total: Option<u32>,
    pub has_text: bool,
    pub has_image: bool,
}

/// Lista ograniczona do przydziału TEGO odbiorcy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextList {
    pub set_id: String,
    pub revision: String,
    pub items: Vec<ContextItem>,
}

/// Jedna odpowiedź tekstowa.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextText {
    pub id: String,
    pub address: String,
    pub text: String,
    pub cursor: Option<String>,
}

/// Jeden wynik zwykłego wyszukiwania tekstowego.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub id: String,
    pub address: String,
    pub excerpt: String,
}

/// Jedna ograniczona strona wyszukiwania.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSearch {
    pub results: Vec<SearchResult>,
    pub cursor: Option<String>,
}

/// Obraz z wybranej pochodnej. Wariant dla agenta i miniatura mają ten sam zakres i odmowy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextImage {
    pub id: String,
    pub address: String,
    pub mime: String,
    pub data: String,
    /// Liczba zwróconych bajtów przed base64; historia dostawy nie zgaduje jej z tekstu.
    pub bytes: usize,
}

/// Którą pochodną obrazu czyta wołający. Model tej wartości nie wybiera.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageVariant {
    Preview,
    Agent,
}

#[derive(Clone, Debug)]
enum TextAt {
    Inline(Arc<str>),
    File(PathBuf),
    Verified(FrozenFile),
}

/// 2026-09-08 (CT-06) — odcisk jest sprawdzany przy KAŻDYM odczycie, więc proces z szerokim
/// dostępem nie może podmienić materiału między listą i późniejszą stroną tekstu.
#[derive(Clone, Debug)]
pub(crate) struct FrozenFile {
    pub root: PathBuf,
    pub relative: PathBuf,
    pub digest: String,
    pub bytes: usize,
}

/// Pozycja gotowa do złożenia półki jednego fizycznego odbiorcy.
#[derive(Clone, Debug)]
pub(crate) struct FrozenItem {
    pub id: String,
    pub source_id: String,
    pub address: String,
    pub name: String,
    pub description: String,
    pub kind: ContextItemKind,
    pub page: Option<u32>,
    pub pages_total: Option<u32>,
    pub text: Option<FrozenFile>,
    pub preview: Option<FrozenFile>,
    pub agent_image: Option<FrozenFile>,
}

#[derive(Clone, Debug)]
enum ImageAt {
    File(PathBuf),
    Verified(FrozenFile),
}

impl ImageAt {
    fn read(&self) -> Result<Vec<u8>, Denied> {
        match self {
            Self::File(path) => fs::read(path).map_err(|_error| Denied::Unavailable),
            Self::Verified(file) => file.read(),
        }
    }
}

impl FrozenFile {
    fn read(&self) -> Result<Vec<u8>, Denied> {
        let file = crate::engine::supervisor::open_private_file(
            &self.root,
            &self.relative,
            crate::engine::supervisor::PrivateFileAccess::Read,
        )
        .map_err(|_error| Denied::Unavailable)?;
        let mut bytes = Vec::new();
        file.take(self.bytes as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_error| Denied::Unavailable)?;
        if bytes.len() != self.bytes || format!("{:x}", Sha256::digest(&bytes)) != self.digest {
            return Err(Denied::Unavailable);
        }
        Ok(bytes)
    }
}

#[derive(Clone, Debug)]
struct Material {
    listed: ContextItem,
    text: Option<TextAt>,
    preview: Option<ImageAt>,
    agent_image: Option<ImageAt>,
}

#[derive(Clone, Debug)]
enum CursorKind {
    Read { id: String, next: usize },
    Search { query: String, next: usize },
}

#[derive(Clone, Debug)]
struct CursorRecord {
    holder: String,
    kind: CursorKind,
}

#[derive(Debug)]
struct Shelf {
    set_id: String,
    revision: String,
    materials: BTreeMap<String, Material>,
    by_source: BTreeMap<String, Vec<String>>,
    sources: BTreeMap<String, FrozenSource>,
    /// Wyłącznie krótkie operacje synchroniczne; w tym module nie ma ani jednego `await`.
    cursors: Mutex<BTreeMap<String, CursorRecord>>,
}

/// Metadane potrzebne wyłącznie lokalnemu podglądowi i workerowi PDF-a.
#[derive(Clone, Debug)]
pub(super) struct FrozenSource {
    pub kind: SourceKind,
    pub file: Option<super::StoredFile>,
    pub folder: PathBuf,
}

/// Zestaw otwarty razem z dokładną rewizją szkicu.
#[derive(Clone, Debug)]
pub struct ContextShelf {
    shelf: Arc<Shelf>,
}

impl ContextShelf {
    /// Otwiera jeden zestaw i zamraża jego mapę źródeł razem z rewizją szkicu.
    pub fn open(library: &Path, set_id: &str) -> Result<Self, Error> {
        let folder = files::folder_of(library, set_id)?;
        let read = files::read_set(library, set_id)?;
        // `revision_of` niesie CAŁE bajty szkicu w base64, bo służy warunkowemu zapisowi.
        // Od 2026-09-08 adres czytelnika dostaje ich odcisk: pełna rewizja przy długim tekście
        // sama przekraczała 8 KiB odpowiedzi wyszukiwania, zanim pojawił się pierwszy wynik.
        // Odcisk jest tylko adresem dla człowieka; uprawnienie nadal wiąże obiekt tej półki.
        let revision = format!("{:x}", Sha256::digest(read.revision.as_bytes()));
        let mut materials = BTreeMap::new();
        let mut by_source = BTreeMap::new();
        let mut sources = BTreeMap::new();

        for source in read.draft.sources {
            sources.insert(
                source.id.clone(),
                FrozenSource {
                    kind: source.kind,
                    file: source.file.clone(),
                    folder: folder.clone(),
                },
            );
            let source_materials = materials_of(&read.set.id, &revision, &folder, &source)?;
            let source_items = source_materials
                .iter()
                .map(|(id, _material)| id.clone())
                .collect();
            materials.extend(source_materials);
            by_source.insert(source.id, source_items);
        }

        Ok(Self {
            shelf: Arc::new(Shelf {
                set_id: read.set.id,
                revision,
                materials,
                by_source,
                sources,
                cursors: Mutex::new(BTreeMap::new()),
            }),
        })
    }

    /// Wydaje przydział jednemu odbiorcy. Nieznany albo powtórzony wpis odmawia przed użyciem.
    pub fn grant(
        &self,
        holder: impl Into<String>,
        allotments: Vec<Allotment>,
        expires: CancellationToken,
    ) -> Result<ContextAccess, Error> {
        let mut grants = BTreeMap::new();
        for allotment in allotments {
            match allotment.selector {
                Selector::Source(source) => {
                    let ids = self
                        .shelf
                        .by_source
                        .get(&source)
                        .ok_or(Denied::UnknownIdentifier)?;
                    // Nieprzygotowany PDF istnieje, lecz nie ma bezpiecznej pochodnej. Od
                    // 2026-09-08 grant odmawia zamiast wydawać pozornie poprawny pusty zakres.
                    if ids.is_empty() {
                        return Err(Denied::Unavailable.into());
                    }
                    for id in ids {
                        insert_grant(&mut grants, id, 0..usize::MAX)?;
                    }
                }
                Selector::Page { source, number } => {
                    let id = page_id(&source, number);
                    if !self.shelf.materials.contains_key(&id) {
                        return Err(Denied::UnknownIdentifier.into());
                    }
                    insert_grant(&mut grants, &id, 0..usize::MAX)?;
                }
                Selector::Bytes { id, from, to } => {
                    let material = self
                        .shelf
                        .materials
                        .get(&id)
                        .ok_or(Denied::UnknownIdentifier)?;
                    let text = text_of(material)?;
                    if from > to
                        || to > text.len()
                        || !text.is_char_boundary(from)
                        || !text.is_char_boundary(to)
                    {
                        return Err(Denied::OutsideRange.into());
                    }
                    insert_grant(&mut grants, &id, from..to)?;
                }
            }
        }
        Ok(ContextAccess {
            shelf: Arc::clone(&self.shelf),
            holder: holder.into(),
            grants,
            expires,
        })
    }

    /// Pełny zakres dla podglądu właściciela — dalej bez drogi do oryginału PDF-a.
    #[must_use]
    pub fn everything(&self) -> ContextAccess {
        ContextAccess {
            shelf: Arc::clone(&self.shelf),
            holder: format!("window:{}:{}", self.shelf.set_id, self.shelf.revision),
            grants: self
                .shelf
                .materials
                .keys()
                .map(|id| (id.clone(), 0..usize::MAX))
                .collect(),
            expires: CancellationToken::new(),
        }
    }

    /// Metadane z TEGO SAMEGO otwarcia co czytelnik. Lokalny worker używa ich tylko po to,
    /// żeby pobrać cały PDF przed przygotowaniem; żaden czasownik modelu nie ma tej drogi.
    pub(super) fn source(&self, id: &str) -> Result<FrozenSource, Error> {
        self.shelf
            .sources
            .get(id)
            .cloned()
            .ok_or(Error::NoSuchSource)
    }
}

/// Czytelnik związany z jednym odbiorcą i jedną rewizją zestawu.
#[derive(Clone, Debug)]
pub struct ContextAccess {
    shelf: Arc<Shelf>,
    holder: String,
    grants: BTreeMap<String, Range<usize>>,
    expires: CancellationToken,
}

impl ContextAccess {
    /// Składa czytelnik wyłącznie z plików prywatnego pakietu biegu. Każde wywołanie dostaje
    /// własną półkę i własny rejestr kursorów; wspólny zestaw nie tworzy globalnej blokady.
    pub(crate) fn frozen(
        holder: impl Into<String>,
        package: impl Into<String>,
        items: Vec<FrozenItem>,
        expires: CancellationToken,
    ) -> Result<Self, Error> {
        let package = package.into();
        let mut materials = BTreeMap::new();
        for item in items {
            let id = item.id.clone();
            let material = Material {
                listed: ContextItem {
                    id: item.id,
                    source_id: item.source_id,
                    address: item.address,
                    name: item.name,
                    description: item.description,
                    kind: item.kind,
                    page: item.page,
                    pages_total: item.pages_total,
                    has_text: item.text.is_some(),
                    has_image: item.agent_image.is_some(),
                },
                text: item.text.map(TextAt::Verified),
                preview: item.preview.map(ImageAt::Verified),
                agent_image: item.agent_image.map(ImageAt::Verified),
            };
            if materials.insert(id, material).is_some() {
                return Err(Denied::OutsideRange.into());
            }
        }
        let grants = materials
            .keys()
            .map(|id| (id.clone(), 0..usize::MAX))
            .collect();
        Ok(Self {
            shelf: Arc::new(Shelf {
                set_id: package.clone(),
                revision: package,
                materials,
                by_source: BTreeMap::new(),
                sources: BTreeMap::new(),
                cursors: Mutex::new(BTreeMap::new()),
            }),
            holder: holder.into(),
            grants,
            expires,
        })
    }

    /// Buduje jeden zamrożony, tekstowy przydział bez tworzenia drugiego czytnika.
    #[must_use]
    pub(crate) fn one_text(
        holder: impl Into<String>,
        set_id: impl Into<String>,
        revision: impl Into<String>,
        id: impl Into<String>,
        name: impl Into<String>,
        text: impl Into<String>,
        expires: CancellationToken,
    ) -> Self {
        let set_id = set_id.into();
        let revision = revision.into();
        let id = id.into();
        let text: Arc<str> = text.into().into();
        let mut materials = BTreeMap::new();
        materials.insert(
            id.clone(),
            Material {
                listed: ContextItem {
                    id: id.clone(),
                    source_id: id.clone(),
                    address: format!("context://{set_id}@{revision}/{id}"),
                    name: name.into(),
                    description: String::new(),
                    kind: ContextItemKind::Text,
                    page: None,
                    pages_total: None,
                    has_text: true,
                    has_image: false,
                },
                text: Some(TextAt::Inline(text)),
                preview: None,
                agent_image: None,
            },
        );
        let grants = BTreeMap::from([(id, 0..usize::MAX)]);
        // 2026-09-08 — plan używa tego samego rejestru kursorów i limitu 16 KiB co Context;
        // osobny pager rozszedłby się z granicą dostępu zmierzoną w CT-03b.
        Self {
            shelf: Arc::new(Shelf {
                set_id,
                revision,
                materials,
                by_source: BTreeMap::new(),
                sources: BTreeMap::new(),
                cursors: Mutex::new(BTreeMap::new()),
            }),
            holder: holder.into(),
            grants,
            expires,
        }
    }

    /// Wypisuje tylko to, co naprawdę znajduje się w tym przydziale.
    pub fn list(&self) -> Result<ContextList, Denied> {
        self.alive()?;
        let items = self
            .grants
            .keys()
            .filter_map(|id| self.shelf.materials.get(id))
            .map(|material| material.listed.clone())
            .collect();
        Ok(ContextList {
            set_id: self.shelf.set_id.clone(),
            revision: self.shelf.revision.clone(),
            items,
        })
    }

    /// Szuka zwykłym porównaniem tekstu, wyłącznie wewnątrz przydzielonych zakresów.
    pub fn search(&self, query: &str, cursor: Option<&str>) -> Result<ContextSearch, Denied> {
        self.alive()?;
        let query = query.trim();
        if query.is_empty() {
            return Err(Denied::EmptySearch);
        }
        let start = match cursor {
            Some(cursor) => match self.cursor(cursor)? {
                CursorKind::Search { query: known, next } if known == query => next,
                _ => return Err(Denied::InvalidCursor),
            },
            None => 0,
        };
        let hits = self.search_hits(query)?;
        if start > hits.len() {
            return Err(Denied::InvalidCursor);
        }

        let mut results = Vec::new();
        let mut next = start;
        while next < hits.len() && results.len() < limits::SEARCH_RESULTS {
            results.push(hits[next].clone());
            next += 1;
            let more = next < hits.len();
            let candidate = ContextSearch {
                results: results.clone(),
                // 2026-09-08 — UUID ma zawsze 36 bajtów; próbna wartość mierzy dokładnie ten
                // sam kształt, zanim wpiszemy prawdziwy kursor do rejestru odbiorcy.
                cursor: more.then(|| "00000000-0000-0000-0000-000000000000".to_owned()),
            };
            if answer_bytes(&candidate)? > limits::SEARCH_ANSWER_BYTES {
                results.pop();
                next -= 1;
                break;
            }
        }
        if results.is_empty() && start < hits.len() {
            return Err(Denied::AnswerTooLarge);
        }
        let cursor = (next < hits.len()).then(|| {
            self.issue(CursorKind::Search {
                query: query.to_owned(),
                next,
            })
        });
        let answer = ContextSearch { results, cursor };
        if answer_bytes(&answer)? > limits::SEARCH_ANSWER_BYTES {
            return Err(Denied::AnswerTooLarge);
        }
        Ok(answer)
    }

    /// Czyta najwyżej 16 KiB tekstu i wydaje dokładne miejsce następnego bajtu jako kursor.
    pub fn read(&self, id: &str, cursor: Option<&str>) -> Result<ContextText, Denied> {
        self.alive()?;
        let material = self.material(id)?;
        let grant = self.grant_for(id)?;
        let text = text_of(material)?;
        let end = grant.end.min(text.len());
        let start = match cursor {
            Some(cursor) => match self.cursor(cursor)? {
                CursorKind::Read { id: known, next } if known == id => next,
                _ => return Err(Denied::InvalidCursor),
            },
            None => grant.start,
        };
        if start < grant.start || start > end || !text.is_char_boundary(start) {
            return Err(Denied::OutsideRange);
        }

        let cut = frame_end(&text, start, end);
        let part = text.get(start..cut).ok_or(Denied::OutsideRange)?.to_owned();
        let cursor = (cut < end).then(|| {
            self.issue(CursorKind::Read {
                id: id.to_owned(),
                next: cut,
            })
        });
        Ok(ContextText {
            id: id.to_owned(),
            address: material.listed.address.clone(),
            text: part,
            cursor,
        })
    }

    /// Czyta dokładnie jeden obraz z pochodnej wybranej przez hosta.
    pub fn image(&self, id: &str, variant: ImageVariant) -> Result<ContextImage, Denied> {
        self.alive()?;
        let material = self.material(id)?;
        self.grant_for(id)?;
        let image = match variant {
            ImageVariant::Preview => material.preview.as_ref(),
            ImageVariant::Agent => material.agent_image.as_ref(),
        }
        .ok_or(Denied::NoImage)?;
        let bytes = image.read()?;
        if bytes.len() > limits::IMAGE_ANSWER_BYTES {
            return Err(Denied::ImageTooLarge);
        }
        let byte_count = bytes.len();
        Ok(ContextImage {
            id: id.to_owned(),
            address: material.listed.address.clone(),
            mime: "image/png".to_owned(),
            data: base64::engine::general_purpose::STANDARD.encode(bytes),
            bytes: byte_count,
        })
    }

    fn alive(&self) -> Result<(), Denied> {
        if self.expires.is_cancelled() {
            Err(Denied::Expired)
        } else {
            Ok(())
        }
    }

    fn material(&self, id: &str) -> Result<&Material, Denied> {
        self.shelf
            .materials
            .get(id)
            .ok_or(Denied::UnknownIdentifier)
    }

    fn grant_for(&self, id: &str) -> Result<&Range<usize>, Denied> {
        self.grants.get(id).ok_or(Denied::NotGranted)
    }

    fn issue(&self, kind: CursorKind) -> String {
        let cursor = uuid::Uuid::now_v7().to_string();
        let mut cursors = self
            .shelf
            .cursors
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        cursors.insert(
            cursor.clone(),
            CursorRecord {
                holder: self.holder.clone(),
                kind,
            },
        );
        cursor
    }

    fn cursor(&self, cursor: &str) -> Result<CursorKind, Denied> {
        let cursors = self
            .shelf
            .cursors
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let record = cursors.get(cursor).ok_or(Denied::InvalidCursor)?;
        if record.holder != self.holder {
            return Err(Denied::DifferentRecipient);
        }
        Ok(record.kind.clone())
    }

    fn search_hits(&self, query: &str) -> Result<Vec<SearchResult>, Denied> {
        let sought = query.to_ascii_lowercase();
        let mut hits = Vec::new();
        for (id, grant) in &self.grants {
            let Some(material) = self.shelf.materials.get(id) else {
                continue;
            };
            let Some(_) = material.text else {
                continue;
            };
            let text = text_of(material)?;
            let end = grant.end.min(text.len());
            if grant.start > end
                || !text.is_char_boundary(grant.start)
                || !text.is_char_boundary(end)
            {
                return Err(Denied::OutsideRange);
            }
            let scoped = text.get(grant.start..end).ok_or(Denied::OutsideRange)?;
            let lowered = scoped.to_ascii_lowercase();
            let Some(found) = lowered.find(&sought) else {
                continue;
            };
            hits.push(SearchResult {
                id: id.clone(),
                address: material.listed.address.clone(),
                excerpt: excerpt(scoped, found, sought.len()),
            });
        }
        Ok(hits)
    }
}

fn insert_grant(
    grants: &mut BTreeMap<String, Range<usize>>,
    id: &str,
    range: Range<usize>,
) -> Result<(), Error> {
    if grants.insert(id.to_owned(), range).is_some() {
        return Err(Denied::OutsideRange.into());
    }
    Ok(())
}

#[derive(Debug, Default)]
struct MaterialSpec {
    id: String,
    kind: ContextItemKind,
    page: Option<u32>,
    pages_total: Option<u32>,
    text: Option<TextAt>,
    preview: Option<ImageAt>,
    agent_image: Option<ImageAt>,
}

/// Buduje wyłącznie bezpieczne pochodne jednego źródła; `open` składa z nich zamrożoną półkę.
fn materials_of(
    set_id: &str,
    revision: &str,
    folder: &Path,
    source: &super::ContextSource,
) -> Result<Vec<(String, Material)>, Error> {
    let one = |id: String, spec: MaterialSpec| {
        let made = material(set_id, revision, source, spec);
        vec![(id, made)]
    };
    match source.kind {
        // Nieznany rodzaj nie otwiera pliku z dowolnym rozszerzeniem. Jeżeli nowszy zapis
        // zostawił tekst w szkicu, starszy build może go bezpiecznie pokazać.
        SourceKind::Text | SourceKind::Unknown => {
            let id = source.id.clone();
            Ok(one(
                id.clone(),
                MaterialSpec {
                    id,
                    kind: ContextItemKind::Text,
                    text: Some(TextAt::Inline(Arc::from(source.text.as_str()))),
                    ..MaterialSpec::default()
                },
            ))
        }
        SourceKind::Document => {
            let version_root = revision_folder(folder, source)?;
            let id = source.id.clone();
            Ok(one(
                id.clone(),
                MaterialSpec {
                    id,
                    kind: ContextItemKind::Text,
                    text: Some(TextAt::File(version_root.join(READER_TEXT))),
                    ..MaterialSpec::default()
                },
            ))
        }
        SourceKind::Image => {
            let version_root = revision_folder(folder, source)?;
            let id = source.id.clone();
            Ok(one(
                id.clone(),
                MaterialSpec {
                    id,
                    kind: ContextItemKind::Image,
                    preview: Some(ImageAt::File(version_root.join(THUMBNAIL))),
                    agent_image: Some(ImageAt::File(version_root.join(FOR_THE_AGENT))),
                    ..MaterialSpec::default()
                },
            ))
        }
        SourceKind::Pdf => {
            let version_root = revision_folder(folder, source)?;
            let pages = source
                .file
                .as_ref()
                .and_then(|file| file.pages)
                .unwrap_or(0);
            let mut made = Vec::new();
            for number in 1..=pages {
                let id = page_id(&source.id, number);
                let stem = version_root.join(PAGES).join(format!("page-{number:04}"));
                let picture = stem.with_extension("png");
                let found = material(
                    set_id,
                    revision,
                    source,
                    MaterialSpec {
                        id: id.clone(),
                        kind: ContextItemKind::Page,
                        page: Some(number),
                        pages_total: Some(pages),
                        text: Some(TextAt::File(stem.with_extension("txt"))),
                        preview: picture.is_file().then(|| ImageAt::File(picture.clone())),
                        agent_image: picture.is_file().then_some(ImageAt::File(picture)),
                    },
                );
                made.push((id, found));
            }
            Ok(made)
        }
    }
}

fn material(
    set_id: &str,
    revision: &str,
    source: &super::ContextSource,
    spec: MaterialSpec,
) -> Material {
    let MaterialSpec {
        id,
        kind,
        page,
        pages_total,
        text,
        preview,
        agent_image,
    } = spec;
    Material {
        listed: ContextItem {
            address: format!("context://{set_id}@{revision}/{id}"),
            source_id: source.id.clone(),
            name: source.name.clone(),
            description: source.description.clone(),
            has_text: text.is_some(),
            has_image: agent_image.is_some(),
            id,
            kind,
            page,
            pages_total,
        },
        text,
        preview,
        agent_image,
    }
}

fn revision_folder(folder: &Path, source: &super::ContextSource) -> Result<PathBuf, Error> {
    let file = source.file.as_ref().ok_or(Denied::Unavailable)?;
    if !safe_component(&source.id) || !safe_component(&file.revision) {
        return Err(Denied::Unavailable.into());
    }
    Ok(folder.join("sources").join(&source.id).join(&file.revision))
}

fn safe_component(value: &str) -> bool {
    !value.is_empty()
        && Path::new(value).components().count() == 1
        && matches!(
            Path::new(value).components().next(),
            Some(Component::Normal(_))
        )
}

fn page_id(source: &str, number: u32) -> String {
    format!("{source}/page/{number:04}")
}

fn text_of(material: &Material) -> Result<String, Denied> {
    match material.text.as_ref().ok_or(Denied::NoText)? {
        TextAt::Inline(text) => Ok(text.to_string()),
        TextAt::File(path) => fs::read_to_string(path).map_err(|_error| Denied::Unavailable),
        TextAt::Verified(file) => {
            String::from_utf8(file.read()?).map_err(|_error| Denied::Unavailable)
        }
    }
}

/// Pierwsza ramka tekstu dla zgodności podglądu starych dokumentów bez bezpiecznej pochodnej.
pub(super) fn first_text_frame(text: &str) -> (&str, bool) {
    let cut = frame_end(text, 0, text.len());
    (text.get(..cut).unwrap_or_default(), cut < text.len())
}

fn frame_end(text: &str, start: usize, end: usize) -> usize {
    let mut cut = start.saturating_add(limits::READ_TEXT_BYTES).min(end);
    while cut > start && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    cut
}

fn excerpt(text: &str, found: usize, sought: usize) -> String {
    const SIDE: usize = 192;
    let mut from = found.saturating_sub(SIDE);
    while from < found && !text.is_char_boundary(from) {
        from += 1;
    }
    let mut to = found
        .saturating_add(sought)
        .saturating_add(SIDE)
        .min(text.len());
    while to > found && !text.is_char_boundary(to) {
        to -= 1;
    }
    text.get(from..to).unwrap_or_default().to_owned()
}

fn answer_bytes(answer: &ContextSearch) -> Result<usize, Denied> {
    let value = serde_json::to_value(answer).map_err(|_error| Denied::Unavailable)?;
    serde_json::to_string_pretty(&value)
        .map(|text| text.len())
        .map_err(|_error| Denied::Unavailable)
}
