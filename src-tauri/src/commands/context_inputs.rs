//! Jedyny resolver przypięć Context na wejściu biegu.
//!
//! Ta warstwa rozwiązuje dziedziczenie raz, zamraża dokładne wersje i składa krótki blok
//! promptu. Adaptery widzą później wyłącznie gotowy tekst i ograniczony [`Snapshot`], więc
//! Lead, Claude, Codex ani Lab nie mogą mieć własnej interpretacji tego samego przypięcia
//! (niezmiennik 23).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::context::access::ContextItemKind;
use crate::context::limits::{SHORT_INDEX_BYTES, STEP_CONTEXT_SETS, STEP_PROMPT_BYTES};
use crate::context::{
    ContextFinding, ContextRevision, ContextSetRead, ContextSource, FindingKind, SourceKind,
    SourceReference,
};
use crate::workflow::WorkflowFile;
use crate::workflow::context::{ContextPin, Topics, effective_for};
use crate::workflow::execution::RunInputs;

use super::context_sources::{
    Address, Delivery, DeliveryState, MaterialInput, NodeSelection, PackageItem, Snapshot,
};
use super::workflow_context::ContextRefusal;

const HEADING: &str = "## Reference materials";
const REQUIREMENTS: &str = "Important requirements:";
const INDEX: &str = "Available topics and sources:";
const READ_MORE: &str = "Use list_context, search_context, read_context and view_context_image to read the full frozen material. These tools can only reach material given to this step.";

/// Jeden fizyczny odbiorca rozwiniętego grafu. `tile_key` wybiera przypięcia, a `node_key`
/// nadaje osobny adres kopii albo rundzie.
pub(crate) struct Recipient<'a> {
    pub node_key: &'a str,
    pub tile_key: &'a str,
    pub name: &'a str,
}

/// Wynik czystego planowania. Pliki wejściowe pozostają prywatne i są kopiowane dopiero po
/// utworzeniu prowizorycznego katalogu biegu.
pub(crate) struct Prepared {
    prompts: BTreeMap<String, String>,
    snapshot: Option<Snapshot>,
}

impl Prepared {
    pub fn prompt_for(&self, node_key: &str) -> &str {
        self.prompts.get(node_key).map_or("", String::as_str)
    }

    pub fn evidence_for(
        &self,
        node_key: &str,
    ) -> std::io::Result<Vec<crate::evidence::ContextSource>> {
        self.snapshot.as_ref().map_or_else(
            || Ok(Vec::new()),
            |snapshot| snapshot.evidence_for(node_key),
        )
    }

    pub fn into_snapshot(self) -> Option<Snapshot> {
        self.snapshot
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Requirement {
    text: String,
    condition: String,
    sources: Vec<SourceReference>,
    topic: String,
}

struct ResolvedSet {
    id: String,
    title: String,
    purpose: String,
    version: String,
    requirements: Vec<Requirement>,
    topics: Vec<(String, String)>,
    sources: Vec<(String, String)>,
    items: Vec<PackageItem>,
    omitted_topics: Vec<(String, String)>,
}

/// Rozwiązuje wszystkie fizyczne węzły przed pierwszym zapisem i pierwszym procesem.
pub(crate) fn prepare(
    home: &Path,
    file: &WorkflowFile,
    inputs: &RunInputs,
    recipients: &[Recipient<'_>],
) -> Result<Prepared, ContextRefusal> {
    let library = crate::context::files::library_root(home);
    let mut prompts = BTreeMap::new();
    let mut package = BTreeMap::<Address, PackageItem>::new();
    let mut nodes = BTreeMap::new();

    for recipient in recipients {
        let effective = effective_for(file, recipient.tile_key, inputs)
            .map_err(|message| refusal(recipient.tile_key, message))?;
        if effective.sets.is_empty() {
            continue;
        }
        if effective.sets.len() > STEP_CONTEXT_SETS {
            return Err(refusal(
                recipient.tile_key,
                format!(
                    "{} cannot start because it would receive more than eight context sets. Remove a set from this step.",
                    recipient.name
                ),
            ));
        }

        let mut resolved = Vec::with_capacity(effective.sets.len());
        for pin in &effective.sets {
            resolved.push(resolve_set(&library, pin, recipient)?);
        }
        let prompt = prompt_for(&resolved, recipient)?;
        let mut selected = Vec::new();
        let mut delivery = Vec::new();
        for set in resolved {
            account_for_set(set, recipient, &mut package, &mut selected, &mut delivery)?;
        }

        prompts.insert(recipient.node_key.to_owned(), prompt);
        nodes.insert(
            recipient.node_key.to_owned(),
            NodeSelection {
                items: selected,
                delivery,
            },
        );
    }

    let snapshot = prepared_snapshot(package, nodes)?;
    Ok(Prepared { prompts, snapshot })
}

/// Rachunek jednego zestawu dla jednego odbiorcy: wymagania, pozycje i pominięte tematy.
///
/// 2026-09-08 (CT-06) — wyjęte z `prepare()`, bo `clippy::too_many_lines` liczyło tam 102
/// przy sufiicie 100, a pod `-D warnings` to BŁĄD kompilacji całego `lib` i `lib test`.
/// Skutek jest szerszy niż jeden lint: `rust-test` leci wtedy jako POMINIĘTY i ani jeden
/// punkt akceptacji nie ma dowodu wykonania. Adnotacja `#[allow]` nie wchodzi w grę —
/// `checks/suppressions.sh` gerpuje ten wzorzec po całym `src-tauri/src`, więc zamieniłaby
/// czerwień clippy na czerwień suppressions.
fn account_for_set(
    set: ResolvedSet,
    recipient: &Recipient<'_>,
    package: &mut BTreeMap<Address, PackageItem>,
    selected: &mut Vec<Address>,
    delivery: &mut Vec<Delivery>,
) -> Result<(), ContextRefusal> {
    for requirement in &set.requirements {
        let item = set
            .topics
            .iter()
            .find(|(id, _)| id == &requirement.topic)
            .map_or("Set requirements", |(_, title)| title.as_str());
        delivery.push(Delivery {
            set_name: set.title.clone(),
            version: set.version.clone(),
            item: item.to_owned(),
            kind: "requirement".to_owned(),
            bytes: requirement_bytes(requirement),
            state: DeliveryState::Included,
            address: None,
            // 2026-09-08 — rachunek przechowuje adres i rozmiar bloku, nie jego
            // treść. Dokładne wymagania jadą wyłącznie w promptcie przez stdin.
            reference: format!("context-sources/{}/{}/requirements", set.id, set.version),
        });
    }
    for item in set.items {
        let bytes = item.bytes().map_err(|error| {
            unavailable(recipient, &set.title, &set.version, &error.to_string())
        })?;
        delivery.push(Delivery {
            set_name: set.title.clone(),
            version: set.version.clone(),
            item: item.name.clone(),
            kind: item_kind(item.kind),
            bytes,
            state: DeliveryState::Available,
            // 2026-09-08 (CT-06) — udany odczyt zapisuje ten adres logiczny. Ponowne
            // wyprowadzanie go ze ścieżki dowodu zostawiało stan `Available` mimo
            // zwrócenia treści w produkcyjnym biegu.
            address: Some(item.address.clone()),
            reference: item.reference(),
        });
        selected.push(item.address.clone());
        package.entry(item.address.clone()).or_insert(item);
    }
    for (id, title) in set.omitted_topics {
        let address = Address {
            set_id: set.id.clone(),
            version: set.version.clone(),
            item_id: format!("topic--{id}"),
        };
        delivery.push(Delivery {
            set_name: set.title.clone(),
            version: set.version.clone(),
            item: title,
            kind: "topic".to_owned(),
            bytes: 0,
            state: DeliveryState::NotIncluded,
            address: None,
            reference: PackageItem {
                address,
                source_id: id,
                name: String::new(),
                description: String::new(),
                kind: ContextItemKind::Text,
                page: None,
                pages_total: None,
                text: None,
                preview: None,
                agent_image: None,
            }
            .reference(),
        });
    }

    Ok(())
}

fn prepared_snapshot(
    package: BTreeMap<Address, PackageItem>,
    nodes: BTreeMap<String, NodeSelection>,
) -> Result<Option<Snapshot>, ContextRefusal> {
    if nodes.is_empty() {
        return Ok(None);
    }
    Snapshot::new(package.into_values().collect(), nodes)
        .map(Some)
        .map_err(|error| refusal("", error.to_string()))
}

fn resolve_set(
    library: &Path,
    pin: &ContextPin,
    recipient: &Recipient<'_>,
) -> Result<ResolvedSet, ContextRefusal> {
    let read = crate::context::files::read_set(library, &pin.id)
        .map_err(|error| unavailable(recipient, &pin.id, &pin.revision, &error.to_string()))?;
    let revision =
        crate::context::build::read_revision(library, &pin.id, &pin.revision).map_err(|error| {
            unavailable(
                recipient,
                &read.set.title,
                &pin.revision,
                &error.to_string(),
            )
        })?;
    let selected_ids = selected_topic_ids(&revision, pin, recipient, &read.set.title)?;

    let selected_findings = revision
        .findings
        .iter()
        .filter(|finding| selected_ids.contains(&finding.topic))
        .collect::<Vec<_>>();
    let requirements = selected_requirements(&read, &selected_findings);

    let sources_by_id = read
        .draft
        .sources
        .iter()
        .map(|source| (source.id.as_str(), source))
        .collect::<BTreeMap<_, _>>();
    let selected_sources = selected_source_parts(&selected_findings);
    let folder = crate::context::files::folder_of(library, &pin.id).map_err(|error| {
        unavailable(
            recipient,
            &read.set.title,
            &pin.revision,
            &error.to_string(),
        )
    })?;
    let mut items = topic_items(
        &folder,
        pin,
        &revision,
        &selected_ids,
        recipient,
        &read.set.title,
    )?;
    let mut sources = Vec::new();
    for (source_id, parts) in selected_sources {
        let source = sources_by_id.get(source_id).ok_or_else(|| {
            unavailable(
                recipient,
                &read.set.title,
                &pin.revision,
                &format!("source {source_id} is missing"),
            )
        })?;
        sources.push((source.id.clone(), source.name.clone()));
        items.extend(source_items(
            &folder,
            pin,
            source,
            &parts,
            recipient,
            &read.set.title,
        )?);
    }

    Ok(ResolvedSet {
        id: pin.id.clone(),
        title: read.set.title,
        purpose: read.set.description,
        version: pin.revision.clone(),
        requirements,
        topics: revision
            .topics
            .iter()
            .filter(|topic| selected_ids.contains(&topic.id))
            .map(|topic| (topic.id.clone(), topic.title.clone()))
            .collect(),
        sources,
        items,
        omitted_topics: revision
            .topics
            .iter()
            .filter(|topic| !selected_ids.contains(&topic.id))
            .map(|topic| (topic.id.clone(), topic.title.clone()))
            .collect(),
    })
}

fn selected_topic_ids(
    revision: &ContextRevision,
    pin: &ContextPin,
    recipient: &Recipient<'_>,
    title: &str,
) -> Result<BTreeSet<String>, ContextRefusal> {
    let selected: BTreeSet<String> = match &pin.topics {
        Topics::All => revision
            .topics
            .iter()
            .map(|topic| topic.id.clone())
            .collect(),
        Topics::Only(ids) => ids.iter().cloned().collect(),
    };
    let known = revision
        .topics
        .iter()
        .map(|topic| topic.id.as_str())
        .collect::<BTreeSet<_>>();
    if selected.iter().any(|id| !known.contains(id.as_str())) {
        return Err(refusal(
            recipient.tile_key,
            format!(
                "{} cannot start because context set {title} no longer contains every chosen topic. Open Context and choose its topics again.",
                recipient.name
            ),
        ));
    }
    Ok(selected)
}

fn selected_requirements(read: &ContextSetRead, findings: &[&ContextFinding]) -> Vec<Requirement> {
    let mut requirements = findings
        .iter()
        .filter(|finding| finding.kind == FindingKind::Requirement)
        .map(|finding| Requirement {
            text: finding.text.clone(),
            condition: finding.condition.clone(),
            sources: finding.sources.clone(),
            topic: finding.topic.clone(),
        })
        .collect::<Vec<_>>();
    let finding_texts = requirements
        .iter()
        .map(|one| one.text.clone())
        .collect::<BTreeSet<_>>();
    // 2026-09-08 — wymaganie szkicu, które gotowa wersja już niesie jako ustalenie, nie może
    // pojawić się drugi raz. Dwa ustalenia o tym samym tekście, lecz różnych warunkach zostają
    // osobno, bo ich tożsamością jest cały rekord, nie podobieństwo zdania (PLAN §9).
    requirements.extend(
        read.draft
            .requirements
            .iter()
            .filter(|text| !finding_texts.contains(text.as_str()))
            .map(|text| Requirement {
                text: text.clone(),
                condition: String::new(),
                sources: Vec::new(),
                topic: String::new(),
            }),
    );
    let mut identities = BTreeSet::new();
    requirements.retain(|one| {
        identities.insert((
            one.text.clone(),
            one.condition.clone(),
            one.sources.clone(),
            one.topic.clone(),
        ))
    });
    requirements
}

fn selected_source_parts<'a>(
    findings: &[&'a ContextFinding],
) -> BTreeMap<&'a str, BTreeSet<&'a str>> {
    let mut selected = BTreeMap::<&str, BTreeSet<&str>>::new();
    for reference in findings.iter().flat_map(|finding| &finding.sources) {
        if !reference.source_id.is_empty() {
            selected
                .entry(&reference.source_id)
                .or_default()
                .insert(&reference.part);
        }
    }
    selected
}

fn topic_items(
    folder: &Path,
    pin: &ContextPin,
    revision: &crate::context::ContextRevision,
    selected: &BTreeSet<String>,
    recipient: &Recipient<'_>,
    title: &str,
) -> Result<Vec<PackageItem>, ContextRefusal> {
    let root = folder.join("versions").join(&pin.revision);
    revision
        .topics
        .iter()
        .zip(&revision.topic_files)
        .filter(|(topic, _)| selected.contains(&topic.id))
        .map(|(topic, relative)| {
            Ok(PackageItem {
                address: Address {
                    set_id: pin.id.clone(),
                    version: pin.revision.clone(),
                    item_id: format!("topic--{}", topic.id),
                },
                source_id: format!("topic--{}", topic.id),
                name: topic.title.clone(),
                description: format!("Prepared topic from {title}."),
                kind: ContextItemKind::Text,
                page: None,
                pages_total: None,
                // 2026-09-08 — opracowanie jest kopiowane tym samym strumieniem co plik
                // źródłowy; wczytanie całego tematu tutaj omijałoby limit pamięci publikacji.
                text: Some(existing(
                    root.join(relative),
                    recipient,
                    title,
                    &pin.revision,
                )?),
                preview: None,
                agent_image: None,
            })
        })
        .collect()
}

fn source_items(
    folder: &Path,
    pin: &ContextPin,
    source: &ContextSource,
    selected_parts: &BTreeSet<&str>,
    recipient: &Recipient<'_>,
    set_title: &str,
) -> Result<Vec<PackageItem>, ContextRefusal> {
    let address = |item_id: String| Address {
        set_id: pin.id.clone(),
        version: pin.revision.clone(),
        item_id,
    };
    let one = |text, preview, agent_image, kind, page, pages_total, item_id| {
        vec![PackageItem {
            address: address(item_id),
            source_id: source.id.clone(),
            name: source.name.clone(),
            description: source.description.clone(),
            kind,
            page,
            pages_total,
            text,
            preview,
            agent_image,
        }]
    };
    match source.kind {
        SourceKind::Text | SourceKind::Unknown => {
            if source.text.is_empty() {
                return Err(unavailable(
                    recipient,
                    set_title,
                    &pin.revision,
                    &format!("source {} has no readable text", source.name),
                ));
            }
            Ok(one(
                Some(MaterialInput::Bytes(source.text.as_bytes().to_vec())),
                None,
                None,
                ContextItemKind::Text,
                None,
                None,
                format!("source--{}", source.id),
            ))
        }
        SourceKind::Document | SourceKind::Image | SourceKind::Pdf => {
            let file = source.file.as_ref().ok_or_else(|| {
                unavailable(
                    recipient,
                    set_title,
                    &pin.revision,
                    &format!("source {} has no saved file", source.name),
                )
            })?;
            let root = folder.join("sources").join(&source.id).join(&file.revision);
            match source.kind {
                SourceKind::Document => Ok(one(
                    Some(existing(
                        root.join("for-the-reader.txt"),
                        recipient,
                        set_title,
                        &pin.revision,
                    )?),
                    None,
                    None,
                    ContextItemKind::Text,
                    None,
                    None,
                    format!("source--{}", source.id),
                )),
                SourceKind::Image => Ok(one(
                    None,
                    None,
                    Some(existing(
                        root.join("for-the-agent.png"),
                        recipient,
                        set_title,
                        &pin.revision,
                    )?),
                    ContextItemKind::Image,
                    None,
                    None,
                    format!("source--{}", source.id),
                )),
                SourceKind::Pdf => pdf_items(
                    &root,
                    pin,
                    source,
                    selected_parts,
                    recipient,
                    set_title,
                    file.pages.unwrap_or(0),
                ),
                SourceKind::Text | SourceKind::Unknown => unreachable!(),
            }
        }
    }
}

fn pdf_items(
    root: &Path,
    pin: &ContextPin,
    source: &ContextSource,
    selected_parts: &BTreeSet<&str>,
    recipient: &Recipient<'_>,
    set_title: &str,
    pages: u32,
) -> Result<Vec<PackageItem>, ContextRefusal> {
    if pages == 0 {
        return Err(unavailable(
            recipient,
            set_title,
            &pin.revision,
            &format!("source {} has no prepared pages", source.name),
        ));
    }
    pdf_pages(
        selected_parts,
        pages,
        source,
        recipient,
        set_title,
        &pin.revision,
    )?
    .into_iter()
    .map(|page| {
        let stem = root.join("pages").join(format!("page-{page:04}"));
        let picture = stem.with_extension("png");
        Ok(PackageItem {
            address: Address {
                set_id: pin.id.clone(),
                version: pin.revision.clone(),
                item_id: format!("source--{}--page-{page:04}", source.id),
            },
            source_id: source.id.clone(),
            name: source.name.clone(),
            description: source.description.clone(),
            kind: ContextItemKind::Page,
            page: Some(page),
            pages_total: Some(pages),
            text: Some(existing(
                stem.with_extension("txt"),
                recipient,
                set_title,
                &pin.revision,
            )?),
            preview: None,
            agent_image: picture.is_file().then_some(MaterialInput::File(picture)),
        })
    })
    .collect()
}

fn pdf_pages(
    selected: &BTreeSet<&str>,
    pages: u32,
    source: &ContextSource,
    recipient: &Recipient<'_>,
    set_title: &str,
    version: &str,
) -> Result<BTreeSet<u32>, ContextRefusal> {
    let mut out = BTreeSet::new();
    for part in selected {
        if *part == "whole" {
            out.extend(1..=pages);
            continue;
        }
        let mut words = part.split_whitespace();
        let page = match (words.next(), words.next()) {
            (Some("page"), Some(number)) => number.parse::<u32>().ok(),
            _ => None,
        };
        let Some(page) = page.filter(|page| (1..=pages).contains(page)) else {
            // 2026-09-08 (CT-06) — nieznanego zakresu nie wolno rozszerzyć do całego PDF-a;
            // taka „naprawa" pokazałaby krokowi strony spoza wybranego tematu.
            return Err(unavailable(
                recipient,
                set_title,
                version,
                &format!(
                    "source {} has a page reference that cannot be matched",
                    source.name
                ),
            ));
        };
        out.insert(page);
    }
    if out.is_empty() {
        return Err(unavailable(
            recipient,
            set_title,
            version,
            &format!("source {} has no selected prepared pages", source.name),
        ));
    }
    Ok(out)
}

fn existing(
    path: PathBuf,
    recipient: &Recipient<'_>,
    title: &str,
    version: &str,
) -> Result<MaterialInput, ContextRefusal> {
    if path.is_file() {
        Ok(MaterialInput::File(path))
    } else {
        Err(unavailable(
            recipient,
            title,
            version,
            "a selected source file is missing",
        ))
    }
}

fn prompt_for(sets: &[ResolvedSet], recipient: &Recipient<'_>) -> Result<String, ContextRefusal> {
    let mut prompt = format!("{HEADING}\n{REQUIREMENTS}\n");
    for set in sets {
        let _ = write!(
            prompt,
            "\n### {} ({})\nPurpose: {}\n",
            set.title, set.version, set.purpose
        );
        for requirement in &set.requirements {
            let _ = write!(prompt, "- {}", requirement.text);
            if !requirement.condition.trim().is_empty() {
                let _ = write!(prompt, "\n  When: {}", requirement.condition);
            }
            prompt.push('\n');
        }
        if prompt.len() > STEP_PROMPT_BYTES {
            return Err(refusal(
                recipient.tile_key,
                format!(
                    "{} cannot start because the important requirements in {} exceed 24 KiB. They were not shortened. Choose fewer topics or split the set.",
                    recipient.name, set.title
                ),
            ));
        }
    }
    let fixed_tail = format!("\n{INDEX}\n\n{READ_MORE}");
    if prompt.len().saturating_add(fixed_tail.len()) > STEP_PROMPT_BYTES {
        let title = sets
            .last()
            .map_or("the selected sets", |set| set.title.as_str());
        return Err(refusal(
            recipient.tile_key,
            format!(
                "{} cannot start because the important requirements in {} exceed 24 KiB. They were not shortened. Choose fewer topics or split the set.",
                recipient.name, title
            ),
        ));
    }
    prompt.push('\n');
    prompt.push_str(INDEX);
    prompt.push('\n');
    let instruction_bytes = READ_MORE.len().saturating_add(2);
    for set in sets {
        let mut index = format!("\n### {} ({})\n", set.title, set.version);
        for (id, title) in &set.topics {
            let line = format!("- Topic: {title} — `{id}`\n");
            push_within(&mut index, &line, SHORT_INDEX_BYTES);
        }
        for (id, name) in &set.sources {
            let line = format!("- Source: {name} — `{id}`\n");
            push_within(&mut index, &line, SHORT_INDEX_BYTES);
        }
        let remaining = STEP_PROMPT_BYTES
            .saturating_sub(prompt.len())
            .saturating_sub(instruction_bytes);
        push_within(&mut prompt, &index, remaining.min(SHORT_INDEX_BYTES));
    }
    prompt.push_str("\n\n");
    prompt.push_str(READ_MORE);
    Ok(prompt)
}

fn push_within(target: &mut String, text: &str, room: usize) {
    if room == 0 {
        return;
    }
    let mut end = text.len().min(room);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    target.push_str(&text[..end]);
}

fn requirement_bytes(requirement: &Requirement) -> usize {
    requirement
        .text
        .len()
        .saturating_add(requirement.condition.len())
}

fn item_kind(kind: ContextItemKind) -> String {
    match kind {
        ContextItemKind::Text => "source",
        ContextItemKind::Image => "image",
        ContextItemKind::Page => "page",
        ContextItemKind::Unknown => "item",
    }
    .to_owned()
}

fn refusal(step_id: &str, message: String) -> ContextRefusal {
    ContextRefusal {
        step_id: step_id.to_owned(),
        message,
    }
}

fn unavailable(
    recipient: &Recipient<'_>,
    title: &str,
    version: &str,
    detail: &str,
) -> ContextRefusal {
    refusal(
        recipient.tile_key,
        format!(
            "{} cannot start because reference material {} version {} is missing or damaged ({detail}). Restore it or choose another ready version.",
            recipient.name, title, version
        ),
    )
}
