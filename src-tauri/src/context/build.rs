//! Trwały przebieg budowania i publikacja niezmiennych wersji Context.
//!
//! `state.json` jest prawdą ekranu, a wynik każdej partii jest prawdą wznowienia. Gotowa wersja
//! staje się bieżąca dopiero po treści, własnym manifeście i kontroli kompletności. Ta kolejność
//! jest mechanizmem zachowania wcześniejszej wersji po przerwanym zapisie, nie konwencją.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::durable_file::{
    DEFINITION_FILE_MODE, DurableFilePublisher, ModePolicy, PublishError, revision_of,
};
use crate::engine::drivers::{ImageInput, ImageMime, ValidatedImages};
use crate::engine::supervisor::PublicationRoot;

use super::files;
use super::findings::{self, Findings};
use super::limits::{BATCH_TEXT_BYTES, Refusal};
use super::{
    BuildEnd, BuildStage, ContextBuild, ContextDraft, ContextFinding, ContextRevision, ContextSet,
    ContextTopic, Error, FindingKind, Origin, Preparation, RevisionEdit, SCHEMA, SourceKind,
    SourceOutcome, SourceProgress, SourceReference,
};

const BUILDS: &str = "builds";
const VERSIONS: &str = "versions";
const STATE: &str = "state.json";
const RESULTS: &str = "results";
const MANIFEST: &str = "manifest.json";
const FINDINGS: &str = "findings.json";
const INDEX: &str = "index.md";
const TOPICS: &str = "topics";
const EXTRACTOR: &str = "context-findings-v1";
const INSTRUCTION: &str = "context-build-instruction-v1";
const MATERIAL_HEADER_BYTES: usize = 512;

/// Dlaczego partii nie dało się nawet wyłożyć przed agentem.
#[derive(Debug)]
pub struct FreezeFailure {
    pub stage: BuildStage,
    pub said: String,
    pub sources: Vec<SourceProgress>,
}

/// Zamrożone wejście całej operacji.
#[derive(Debug)]
pub struct Frozen {
    pub set: ContextSet,
    pub draft: ContextDraft,
    pub draft_bytes_revision: String,
    pub folder: PathBuf,
    pub batches: Vec<Batch>,
    pub sources: Vec<SourceProgress>,
    pub prior_human: Vec<ContextFinding>,
    pub input_fingerprint: String,
}

/// Jedna ograniczona partia, gotowa do wysłania przez świeżą rozmowę.
#[derive(Debug)]
pub struct Batch {
    pub id: String,
    pub material: String,
    pub references: BTreeSet<SourceReference>,
    pub images: ValidatedImages,
    pub readable_files: Vec<PathBuf>,
    pub fingerprint: String,
}

#[derive(Debug)]
struct Unit {
    reference: SourceReference,
    text: String,
    image: Option<(PathBuf, Vec<u8>)>,
    readable_files: Vec<PathBuf>,
}

#[derive(Debug)]
struct UnitFailure {
    said: String,
    sources: Vec<SourceProgress>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SavedBatch {
    fingerprint: String,
    findings: Findings,
}

/// Zamraża szkic i dzieli go deterministycznie po źródłach, stronach i granicach akapitów.
pub fn freeze_and_split(
    library: &Path,
    set_id: &str,
    app: &str,
    model: Option<&str>,
    budget_usd: Option<f64>,
) -> Result<Frozen, FreezeFailure> {
    let read = files::read_set(library, set_id).map_err(|error| FreezeFailure {
        stage: BuildStage::Freezing,
        said: error.to_string(),
        sources: Vec::new(),
    })?;
    let folder = files::folder_of(library, set_id).map_err(|error| FreezeFailure {
        stage: BuildStage::Freezing,
        said: error.to_string(),
        sources: Vec::new(),
    })?;
    let prior_human = read_latest_revision(library, set_id)
        .ok()
        .flatten()
        .map(|revision| {
            revision
                .findings
                .into_iter()
                .filter(|finding| finding.origin == Origin::Human)
                .collect()
        })
        .unwrap_or_default();
    let mut units = Vec::new();
    let mut progress = Vec::new();
    let mut first_failure = None::<(BuildStage, String)>;
    for source in &read.draft.sources {
        if read.draft.excluded.iter().any(|id| id == &source.id) {
            progress.push(SourceProgress {
                source_id: source.id.clone(),
                part: "whole".to_owned(),
                outcome: SourceOutcome::Excluded,
                said: "You left this source out of the build.".to_owned(),
            });
            continue;
        }
        if source.kind == SourceKind::Pdf && source.preparation != Preparation::Ready {
            // 2026-09-08 (CT-04) — `Failed` jest końcowym wynikiem wcześniejszego przygotowania,
            // nie kolejnym `Needs`. Zastąpienie jego zdania ogólnym „Prepare” cofało człowieka
            // do czynności, która już raz dowiodła, dlaczego nie może się udać.
            let said = match &source.preparation {
                Preparation::Failed { said } if !said.trim().is_empty() => said.clone(),
                _ => Refusal::SourceNotReady {
                    name: source.name.clone(),
                }
                .to_string(),
            };
            progress.push(SourceProgress {
                source_id: source.id.clone(),
                part: "whole".to_owned(),
                outcome: SourceOutcome::Failed,
                said: said.clone(),
            });
            first_failure.get_or_insert((BuildStage::Freezing, said));
            continue;
        }
        let made = match units_for(&folder, source) {
            Ok(made) => made,
            Err(failure) => {
                progress.extend(failure.sources);
                first_failure.get_or_insert((BuildStage::Splitting, failure.said));
                continue;
            }
        };
        for unit in &made {
            progress.push(SourceProgress {
                source_id: unit.reference.source_id.clone(),
                part: unit.reference.part.clone(),
                outcome: SourceOutcome::Unknown,
                said: String::new(),
            });
        }
        units.extend(made);
    }
    let (units, progress) = ready_units(units, progress, first_failure)?;
    let input_fingerprint = fingerprint(&(
        &read.revision,
        read.set.draft_revision,
        app,
        model,
        budget_usd,
        EXTRACTOR,
        INSTRUCTION,
        &read.draft.how_to_prepare,
        &read.draft.requirements,
        &prior_human,
    ));
    let batches = batches_from(units, &input_fingerprint).map_err(|said| FreezeFailure {
        stage: BuildStage::Splitting,
        said,
        sources: progress.clone(),
    })?;
    Ok(Frozen {
        set: read.set,
        draft: read.draft,
        draft_bytes_revision: read.revision,
        folder,
        batches,
        sources: progress,
        prior_human,
        input_fingerprint,
    })
}

fn ready_units(
    units: Vec<Unit>,
    mut progress: Vec<SourceProgress>,
    first_failure: Option<(BuildStage, String)>,
) -> Result<(Vec<Unit>, Vec<SourceProgress>), FreezeFailure> {
    if let Some((stage, said)) = first_failure {
        // 2026-09-08 (CT-04) — stan porażki jest czytany jako kompletna tabela pokrycia.
        // `Unknown` po końcu operacji wyglądałby jak nadal trwająca praca, choć żaden agent
        // już nie wystartuje; dlatego również poprawne fragmenty niedoszłego wejścia dostają
        // końcową wartość zamiast wisieć bez rozstrzygnięcia.
        for source in &mut progress {
            if source.outcome == SourceOutcome::Unknown {
                source.outcome = SourceOutcome::Failed;
                "This part was not read because another source was not ready."
                    .clone_into(&mut source.said);
            }
        }
        return Err(FreezeFailure {
            stage,
            said,
            sources: progress,
        });
    }
    if units.is_empty() {
        return Err(FreezeFailure {
            stage: BuildStage::Freezing,
            said: "Add or include at least one ready source before building this context."
                .to_owned(),
            sources: progress,
        });
    }
    Ok((units, progress))
}

/// Pierwszy trwały stan, zanim ruszy jakakolwiek płatna praca.
#[must_use]
pub fn initial_state(
    request: &crate::commands::context_build::BuildContextRequest,
    app: &str,
    draft_revision: u64,
    input_fingerprint: String,
    sources: Vec<SourceProgress>,
    at: &str,
) -> ContextBuild {
    ContextBuild {
        schema: SCHEMA,
        operation_id: request.operation_id.clone(),
        set_id: request.set_id.clone(),
        generation: request.generation,
        draft_revision,
        stage: BuildStage::Freezing,
        end: BuildEnd::Running,
        app: app.to_owned(),
        requested_model: request.model.clone(),
        model: None,
        batches_done: 0,
        batches_total: 0,
        sources,
        said: "The material is frozen for this build.".to_owned(),
        revision_id: None,
        input_fingerprint,
        started_at: at.to_owned(),
        changed_at: at.to_owned(),
    }
}

/// Zapisuje stan pod stałym adresem tej operacji.
pub fn write_state(library: &Path, state: &ContextBuild) -> Result<(), Error> {
    let folder = files::folder_of(library, &state.set_id)?;
    let relative = Path::new(BUILDS).join(&state.operation_id);
    let publisher = DurableFilePublisher::new(&folder);
    publisher
        .with_initialized_publication(
            "context-builds",
            |root| {
                root.ensure_directory(Path::new(BUILDS), 0o700)
                    .map_err(PublishError::Io)
            },
            |batch| {
                // 2026-09-08 (CT-04) — domenowy inicjalizator biegnie raz, a katalog jest per
                // operacja. Utworzenie go tam pozwalało zapisać pierwszy build w zestawie,
                // po czym każdy następny kończył się ENOENT przed pierwszym stanem.
                batch
                    .root()
                    .ensure_directory(&relative, 0o700)
                    .map_err(PublishError::Io)?;
                batch
                    .root()
                    .ensure_directory(&relative.join(RESULTS), 0o700)
                    .map_err(PublishError::Io)?;
                let bytes =
                    as_file(state).map_err(|error| PublishError::Io(error_to_io(&error)))?;
                batch.atomic_replace(
                    &folder.join(&relative).join(STATE),
                    bytes.as_bytes(),
                    ModePolicy::PreserveExistingOr(DEFINITION_FILE_MODE),
                )
            },
        )
        .map_err(|error| Error::Unwritable(error.into_io()))
}

/// Odczytuje najnowszą albo wskazaną operację z dysku.
pub fn read_build(
    library: &Path,
    set_id: &str,
    operation_id: Option<&str>,
) -> Result<Option<ContextBuild>, Error> {
    let folder = files::folder_of(library, set_id)?;
    let builds = folder.join(BUILDS);
    if let Some(operation) = operation_id {
        return match fs::read(builds.join(operation).join(STATE)) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        };
    }
    let entries = match fs::read_dir(builds) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut latest = None::<ContextBuild>;
    for entry in entries.filter_map(Result::ok) {
        let Ok(bytes) = fs::read(entry.path().join(STATE)) else {
            continue;
        };
        let Ok(candidate) = serde_json::from_slice::<ContextBuild>(&bytes) else {
            continue;
        };
        let is_newer = latest.as_ref().is_none_or(|saved| {
            // 2026-09-08 (CT-04) — zegar plikowy ma rozdzielczość jednej sekundy, a generacja
            // jest monotoniczna w życiu aplikacji. Bez niej dwa szybkie Starty wybierały stan
            // według losowego tekstu operation-id, więc ekran mógł wrócić do starszego builda.
            (
                &candidate.started_at,
                candidate.generation,
                &candidate.changed_at,
                &candidate.operation_id,
            ) > (
                &saved.started_at,
                saved.generation,
                &saved.changed_at,
                &saved.operation_id,
            )
        });
        if is_newer {
            latest = Some(candidate);
        }
    }
    Ok(latest)
}

/// Stan bez żywego właściciela staje się przerwany, nigdy gotowy.
pub fn interrupt_unowned(
    library: &Path,
    mut state: ContextBuild,
    at: &str,
) -> Result<ContextBuild, Error> {
    if matches!(state.end, BuildEnd::Running | BuildEnd::StillRunning) {
        // 2026-09-08 (CT-04) — `StillRunning` jest brakiem dowodu końca, nie trwałym końcem.
        // Po restarcie nie ma już właściciela uchwytu, więc oba niedomknięte warianty mają tę
        // samą uczciwą nazwę: przerwane, nigdy gotowe ani anulowane.
        state.stage = BuildStage::Interrupted;
        state.end = BuildEnd::Interrupted;
        "This build ended when Loadout closed. Rebuild context to continue from saved batches."
            .clone_into(&mut state.said);
        at.clone_into(&mut state.changed_at);
        write_state(library, &state)?;
    }
    Ok(state)
}

/// Gotowa partia wraca wyłącznie dla zgodnego odcisku wszystkich jej wejść.
pub fn cached_batch(frozen: &Frozen, operation_id: &str, batch: &Batch) -> Option<Findings> {
    let builds = frozen.folder.join(BUILDS);
    let current = builds.join(operation_id);
    let current_for_filter = current.clone();
    let others = fs::read_dir(&builds)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(move |path| path != &current_for_filter);
    std::iter::once(current)
        .chain(others)
        .find_map(|operation| {
            let path = operation.join(RESULTS).join(format!("{}.json", batch.id));
            let saved: SavedBatch = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
            (saved.fingerprint == batch.fingerprint).then_some(saved.findings)
        })
}

/// Zachowuje gotową partię pod adresem, który następne wznowienie potrafi przeczytać.
pub fn save_batch(
    frozen: &Frozen,
    operation_id: &str,
    batch: &Batch,
    findings: &Findings,
) -> Result<(), Error> {
    let target = frozen
        .folder
        .join(BUILDS)
        .join(operation_id)
        .join(RESULTS)
        .join(format!("{}.json", batch.id));
    DurableFilePublisher::new(&frozen.folder)
        .atomic_replace(
            &target,
            as_file(&SavedBatch {
                fingerprint: batch.fingerprint.clone(),
                findings: findings.clone(),
            })?
            .as_bytes(),
            ModePolicy::PreserveExistingOr(DEFINITION_FILE_MODE),
        )
        .map_err(|error| Error::Unwritable(error.into_io()))
}

/// Publikuje treść, potem manifest wersji, a wskaźnik zestawu bezwzględnie na końcu.
pub fn publish_revision(
    library: &Path,
    frozen: &Frozen,
    findings: &Findings,
    state: &ContextBuild,
    at: &str,
) -> Result<ContextRevision, Error> {
    let current = files::read_set(library, &frozen.set.id)?;
    if current.revision != frozen.draft_bytes_revision
        || current.set.draft_revision != frozen.set.draft_revision
        // 2026-09-08 (CT-04) — korekta człowieka tworzy wersję bez zmiany draftu. Sam test
        // rewizji draftu przepuszczał więc starszą odpowiedź agenta NAD tę korektę i zabierał
        // ją z wejścia następnej przebudowy; wskaźnik gotowej wersji jest drugim zamrożonym
        // wejściem publikacji i musi nadal wskazywać dokładnie tę samą wersję.
        || current.set.latest_ready_revision != frozen.set.latest_ready_revision
    {
        return Err(Error::BuildFailed(
            "This context changed while it was being built. The older answer was not saved. Try Rebuild context."
                .to_owned(),
        ));
    }
    if !findings.missing.is_empty() || findings.findings.is_empty() {
        let why = if findings.missing.is_empty() {
            "it contained no finding with an exact source".to_owned()
        } else {
            findings.missing.join(" ")
        };
        return Err(Refusal::BuildNotPublishable { said: why }.into());
    }
    let revision_id = crate::commands::mint::new_id_inner().to_string();
    let topic_files = findings
        .topics
        .iter()
        .map(|topic| format!("topics/{}.md", topic.id))
        .collect::<Vec<_>>();
    let revision = ContextRevision {
        schema: SCHEMA,
        id: revision_id.clone(),
        set_id: frozen.set.id.clone(),
        draft_revision: frozen.set.draft_revision,
        app: state.app.clone(),
        requested_model: state.requested_model.clone(),
        model: state.model.clone(),
        created_at: at.to_owned(),
        origin: Origin::Generated,
        topics: findings.topics.clone(),
        findings: findings.findings.clone(),
        questions: findings.questions.clone(),
        conflicts: findings
            .findings
            .iter()
            .filter(|one| one.kind == FindingKind::Conflict || !one.conflicts_with.is_empty())
            .map(|one| one.text.clone())
            .collect(),
        sources: state.sources.clone(),
        index_file: INDEX.to_owned(),
        findings_file: FINDINGS.to_owned(),
        topic_files,
    };
    publish_complete(&frozen.folder, &current.set, &revision, findings)?;
    Ok(revision)
}

/// Odczytuje gotową wersję wskazaną przez manifest zestawu i sprawdza wszystkie jej pliki.
pub fn read_latest_revision(
    library: &Path,
    set_id: &str,
) -> Result<Option<ContextRevision>, Error> {
    let read = files::read_set(library, set_id)?;
    let Some(id) = read.set.latest_ready_revision.as_deref() else {
        return Ok(None);
    };
    read_revision(library, set_id, id).map(Some)
}

/// 2026-09-08 (CT-05): odczytuje dokładną, przypiętą wersję — nigdy nie zastępuje jej bieżącą.
pub fn read_revision(
    library: &Path,
    set_id: &str,
    revision_id: &str,
) -> Result<ContextRevision, Error> {
    if !safe_directory_name(revision_id) {
        return Err(incomplete_revision());
    }
    let folder = files::folder_of(library, set_id)?;
    let revision_folder = folder.join(VERSIONS).join(revision_id);
    let held = PublicationRoot::open(&revision_folder).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            Error::NoSuchRevision
        } else {
            incomplete_revision()
        }
    })?;
    let manifest = read_revision_file(&held, Path::new(MANIFEST))?;
    let mut revision: ContextRevision = serde_json::from_slice(&manifest)?;
    let expected_topics = revision
        .topics
        .iter()
        .map(|topic| format!("topics/{}.md", topic.id))
        .collect::<Vec<_>>();
    if revision.id != revision_id
        || revision.set_id != set_id
        || revision.index_file != INDEX
        || revision.findings_file != FINDINGS
        || revision.topic_files != expected_topics
    {
        return Err(incomplete_revision());
    }
    read_revision_file(&held, Path::new(INDEX))?;
    for path in &revision.topic_files {
        read_revision_file(&held, Path::new(path))?;
    }
    revision.findings = serde_json::from_slice(&read_revision_file(&held, Path::new(FINDINGS))?)?;
    Ok(revision)
}

fn read_revision_file(root: &PublicationRoot, relative: &Path) -> Result<Vec<u8>, Error> {
    let mut file = root
        .open_regular_file(relative)
        .map_err(|_| incomplete_revision())?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|_| incomplete_revision())?;
    Ok(bytes)
}

fn incomplete_revision() -> Error {
    Error::BuildFailed(
        "The ready context version is incomplete. Try Rebuild context to publish a complete one."
            .to_owned(),
    )
}

fn safe_directory_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

/// Zapisuje korektę człowieka jako nową wersję, bez zmiany bajtów poprzedniej.
pub fn save_human_revision(
    library: &Path,
    set_id: &str,
    edit: &RevisionEdit,
    at: &str,
) -> Result<ContextRevision, Error> {
    let mut revision = read_latest_revision(library, set_id)?.ok_or(Error::NoSuchRevision)?;
    if let Some(id) = edit.finding_id.as_deref() {
        let text = edit.text.as_deref().unwrap_or_default().trim();
        let Some(finding) = revision.findings.iter_mut().find(|one| one.id == id) else {
            return Err(Error::BuildFailed(
                "That finding is no longer in the ready version. Open the set again before saving."
                    .to_owned(),
            ));
        };
        if text.is_empty() {
            return Err(Error::BuildFailed(
                "Write the corrected finding before saving it.".to_owned(),
            ));
        }
        text.clone_into(&mut finding.text);
        finding.origin = Origin::Human;
        finding.id = fingerprint(&(finding.kind, text, &finding.condition, &finding.sources));
    }
    if !edit.correction.trim().is_empty() {
        let topic = ContextTopic {
            id: "your-corrections".to_owned(),
            title: "Your corrections".to_owned(),
        };
        if !revision.topics.iter().any(|one| one.id == topic.id) {
            revision.topics.push(topic);
        }
        revision.findings.push(ContextFinding {
            id: fingerprint(&("human", edit.correction.trim(), at)),
            kind: FindingKind::Requirement,
            text: edit.correction.trim().to_owned(),
            condition: String::new(),
            sources: Vec::new(),
            topic: "your-corrections".to_owned(),
            conflicts_with: Vec::new(),
            origin: Origin::Human,
        });
    }
    if edit.correction.trim().is_empty() && edit.finding_id.is_none() {
        return Err(Error::BuildFailed(
            "Write a correction or change a finding before saving.".to_owned(),
        ));
    }
    let read = files::read_set(library, set_id)?;
    let folder = files::folder_of(library, set_id)?;
    revision.id = crate::commands::mint::new_id_inner().to_string();
    at.clone_into(&mut revision.created_at);
    revision.origin = Origin::Human;
    revision.topic_files = revision
        .topics
        .iter()
        .map(|topic| format!("topics/{}.md", topic.id))
        .collect();
    let findings = Findings {
        topics: revision.topics.clone(),
        findings: revision.findings.clone(),
        questions: revision.questions.clone(),
        missing: Vec::new(),
    };
    publish_complete(&folder, &read.set, &revision, &findings)?;
    Ok(revision)
}

fn units_for(folder: &Path, source: &super::ContextSource) -> Result<Vec<Unit>, UnitFailure> {
    match source.kind {
        SourceKind::Text if source.file.is_none() => required_text_units(source, &source.text, &[]),
        SourceKind::Document | SourceKind::Text => {
            let file = source.file.as_ref().ok_or_else(|| {
                whole_failure(
                    source,
                    format!("{} has no saved file to read. Add it again.", source.name),
                )
            })?;
            let (path, bytes) = read_saved_file(folder, Path::new(&file.path), &source.name)
                .map_err(|said| whole_failure(source, said))?;
            let text = String::from_utf8(bytes).map_err(|_| {
                whole_failure(
                    source,
                    format!(
                        "{} is not readable text. Add a plain-text copy.",
                        source.name
                    ),
                )
            })?;
            required_text_units(source, &text, std::slice::from_ref(&path))
        }
        SourceKind::Image => {
            let file = source.file.as_ref().ok_or_else(|| {
                whole_failure(
                    source,
                    format!(
                        "{} has no saved picture to read. Add it again.",
                        source.name
                    ),
                )
            })?;
            let relative = Path::new(&file.path)
                .parent()
                .map(|parent| parent.join("for-the-agent.png"))
                .ok_or_else(|| {
                    whole_failure(
                        source,
                        format!("{} has no safe saved picture path.", source.name),
                    )
                })?;
            let (path, bytes) = read_saved_file(folder, &relative, &source.name)
                .map_err(|said| whole_failure(source, said))?;
            Ok(vec![Unit {
                reference: SourceReference {
                    source_id: source.id.clone(),
                    part: "whole".to_owned(),
                },
                text: format!("[Picture {}]", source.name),
                image: Some((path.clone(), bytes)),
                readable_files: vec![path],
            }])
        }
        SourceKind::Pdf => pdf_units(folder, source),
        SourceKind::Unknown => Err(whole_failure(
            source,
            format!(
                "{} is a source kind this version of Loadout cannot build from.",
                source.name
            ),
        )),
    }
}

fn pdf_units(folder: &Path, source: &super::ContextSource) -> Result<Vec<Unit>, UnitFailure> {
    let file = source.file.as_ref().ok_or_else(|| {
        whole_failure(
            source,
            format!(
                "{} has no saved document to read. Add it again.",
                source.name
            ),
        )
    })?;
    let pages = file.pages.ok_or_else(|| {
        whole_failure(
            source,
            format!(
                "{} has no prepared pages. Prepare it before building.",
                source.name
            ),
        )
    })?;
    let base = Path::new(&file.path)
        .parent()
        .map(|parent| parent.join("pages"))
        .ok_or_else(|| {
            whole_failure(
                source,
                format!("{} has no safe saved page path.", source.name),
            )
        })?;
    let mut out = Vec::new();
    let mut progress = Vec::new();
    let mut first_failure = None;
    for number in 1..=pages {
        let stem = base.join(format!("page-{number:04}"));
        let text_path = stem.with_extension("txt");
        let (text_path, text) =
            match read_saved_file(folder, &text_path, &source.name).and_then(|(path, bytes)| {
                String::from_utf8(bytes)
                    .map(|text| (path, text))
                    .map_err(|_| format!("Page {number} of {} is not readable text.", source.name))
            }) {
                Ok(found) => found,
                Err(said) => {
                    first_failure.get_or_insert_with(|| said.clone());
                    progress.push(failed_part(source, format!("page {number}"), said));
                    continue;
                }
            };
        let image_path = stem.with_extension("png");
        let image = match read_optional_saved_file(folder, &image_path, &source.name) {
            Ok(image) => image,
            Err(said) => {
                first_failure.get_or_insert_with(|| said.clone());
                progress.push(failed_part(source, format!("page {number}"), said));
                continue;
            }
        };
        let mut readable_files = vec![text_path];
        if let Some((path, _)) = &image {
            readable_files.push(path.clone());
        }
        let mut sections = split_text(&text);
        if sections.is_empty() && image.is_some() {
            // 2026-09-08 (CT-04) — skan może nie mieć ani jednego znaku, ale jego obraz jest
            // właśnie materiałem do odczytu. Pusta lista sekcji po cichu wyrzucałaby całą stronę.
            sections.push("[Page image]".to_owned());
        }
        if sections.is_empty() {
            let said = format!(
                "Page {number} of {} has neither readable text nor a prepared picture.",
                source.name
            );
            first_failure.get_or_insert_with(|| said.clone());
            progress.push(failed_part(source, format!("page {number}"), said));
            continue;
        }
        for (at, text) in sections.iter().enumerate() {
            let reference = SourceReference {
                source_id: source.id.clone(),
                part: if sections.len() == 1 {
                    format!("page {number}")
                } else {
                    format!("page {number} section {}", at + 1)
                },
            };
            progress.push(failed_part(
                source,
                reference.part.clone(),
                "This part was not read because another page was not ready.".to_owned(),
            ));
            out.push(Unit {
                reference,
                text: text.clone(),
                // Wygląd strony jedzie raz: powielenie go przy każdym kawałku tekstu zużywałoby
                // limit obrazów bez dodania informacji (2026-09-08, CT-04; PLAN §6).
                image: (at == 0).then(|| image.clone()).flatten(),
                readable_files: readable_files.clone(),
            });
        }
    }
    finish_pdf_units(out, progress, first_failure)
}

fn finish_pdf_units(
    units: Vec<Unit>,
    progress: Vec<SourceProgress>,
    first_failure: Option<String>,
) -> Result<Vec<Unit>, UnitFailure> {
    first_failure.map_or_else(
        || Ok(units),
        |said| {
            Err(UnitFailure {
                said,
                sources: progress,
            })
        },
    )
}

fn whole_failure(source: &super::ContextSource, said: String) -> UnitFailure {
    UnitFailure {
        sources: vec![failed_part(source, "whole".to_owned(), said.clone())],
        said,
    }
}

fn failed_part(source: &super::ContextSource, part: String, said: String) -> SourceProgress {
    SourceProgress {
        source_id: source.id.clone(),
        part,
        outcome: SourceOutcome::Failed,
        said,
    }
}

fn read_saved_file(
    folder: &Path,
    relative: &Path,
    name: &str,
) -> Result<(PathBuf, Vec<u8>), String> {
    // 2026-09-08 (CT-04) — draft wraca z webviewa, więc sama konkatenacja ścieżek nie jest
    // granicą. Odczyt względem trzymanego katalogu odmawia `..`, ścieżek bezwzględnych,
    // dowiązań i podmiany katalogu zanim jakiekolwiek bajty trafią do prośby dla agenta.
    read_optional_saved_file(folder, relative, name)?.ok_or_else(|| {
        format!("{name} could not be read because its saved file is missing. Add it again.")
    })
}

fn read_optional_saved_file(
    folder: &Path,
    relative: &Path,
    name: &str,
) -> Result<Option<(PathBuf, Vec<u8>)>, String> {
    let root = PublicationRoot::open(folder)
        .map_err(|error| format!("{name} has no safe saved file path: {error}. Add it again."))?;
    let mut file = match root.open_regular_file(relative) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "{name} has no safe saved file path: {error}. Add it again."
            ));
        }
    };
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| format!("{name} could not be read: {error}."))?;
    Ok(Some((folder.join(relative), bytes)))
}

fn text_units(source_id: &str, text: &str, readable_files: &[PathBuf]) -> Vec<Unit> {
    split_text(text)
        .into_iter()
        .enumerate()
        .map(|(at, text)| Unit {
            reference: SourceReference {
                source_id: source_id.to_owned(),
                part: format!("fragment {}", at + 1),
            },
            text,
            image: None,
            readable_files: readable_files.to_vec(),
        })
        .collect()
}

fn required_text_units(
    source: &super::ContextSource,
    text: &str,
    readable_files: &[PathBuf],
) -> Result<Vec<Unit>, UnitFailure> {
    let units = text_units(&source.id, text, readable_files);
    if units.is_empty() {
        // 2026-09-08 (CT-04) — puste źródło obok poprawnego wcześniej znikało całkowicie:
        // nie miało partii ani końcowego wyniku. Kompletność dotyczy każdego źródła, więc pusty
        // tekst jest nazwaną porażką podziału, a nie materiałem pominiętym po cichu.
        return Err(whole_failure(
            source,
            format!(
                "{} has no readable text. Add its contents or leave it out explicitly.",
                source.name
            ),
        ));
    }
    Ok(units)
}

fn split_text(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for paragraph in text.split_inclusive("\n\n") {
        let body_limit = BATCH_TEXT_BYTES.saturating_sub(MATERIAL_HEADER_BYTES);
        if paragraph.len() > body_limit {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            let mut left = paragraph;
            while !left.is_empty() {
                let mut cut = left.len().min(body_limit);
                while !left.is_char_boundary(cut) {
                    cut = cut.saturating_sub(1);
                }
                out.push(left[..cut].to_owned());
                left = &left[cut..];
            }
        } else if current.len().saturating_add(paragraph.len()) > body_limit {
            out.push(std::mem::take(&mut current));
            current.push_str(paragraph);
        } else {
            current.push_str(paragraph);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn batches_from(units: Vec<Unit>, input_fingerprint: &str) -> Result<Vec<Batch>, String> {
    let mut grouped = Vec::<Vec<Unit>>::new();
    for unit in units {
        let needs_new = grouped.last().is_some_and(|batch| {
            let text = batch.iter().map(material_bytes).sum::<usize>();
            let image_would_not_fit = unit.image.as_ref().is_some_and(|(_, bytes)| {
                let mut images = batch
                    .iter()
                    .filter_map(|one| one.image.as_ref())
                    .map(|(_, bytes)| ImageInput::new(ImageMime::Png, bytes.clone()))
                    .collect::<Vec<_>>();
                images.push(ImageInput::new(ImageMime::Png, bytes.clone()));
                // 2026-09-08 (CT-04) — liczby limitów obrazów mają jedno źródło prawdy.
                // Ta sama walidacja, która chroni transport, wyznacza więc także granicę partii.
                ValidatedImages::validate(images).is_err()
            });
            text.saturating_add(material_bytes(&unit)) > BATCH_TEXT_BYTES || image_would_not_fit
        });
        if needs_new || grouped.is_empty() {
            grouped.push(Vec::new());
        }
        if let Some(batch) = grouped.last_mut() {
            batch.push(unit);
        }
    }
    grouped
        .into_iter()
        .enumerate()
        .map(|(at, units)| {
            let mut material = String::new();
            let mut references = BTreeSet::new();
            let mut images = Vec::new();
            let mut readable_files = Vec::new();
            for unit in units {
                let _ = write!(
                    material,
                    "\n\n[SOURCE {} · {}]\n{}",
                    unit.reference.source_id, unit.reference.part, unit.text
                );
                references.insert(unit.reference);
                readable_files.extend(unit.readable_files);
                if let Some((_path, bytes)) = unit.image {
                    images.push(ImageInput::new(ImageMime::Png, bytes));
                }
            }
            if material.len() > BATCH_TEXT_BYTES {
                return Err("One part is longer than the 24 KiB batch limit. Split that source into smaller sections before rebuilding."
                    .to_owned());
            }
            readable_files.sort();
            readable_files.dedup();
            let images = ValidatedImages::validate(images).map_err(|error| error.to_string())?;
            let fingerprint = fingerprint(&(
                // 2026-09-08 (CT-04) — korekta człowieka nie zmienia `draft.json`, ale zmienia
                // prośbę następnej przebudowy. Odcisk samego draftu pozwalał więc użyć wyniku
                // sprzed korekty i ominąć dokładnie tę nową informację.
                input_fingerprint,
                material.as_str(),
                references.iter().collect::<Vec<_>>(),
                images
                    .as_slice()
                    .iter()
                    .map(|image| Sha256::digest(image.bytes()).to_vec())
                    .collect::<Vec<_>>(),
            ));
            Ok(Batch {
                id: format!("batch-{:04}", at + 1),
                material,
                references,
                images,
                readable_files,
                fingerprint,
            })
        })
        .collect()
}

fn material_bytes(unit: &Unit) -> usize {
    unit.text
        .len()
        .saturating_add(unit.reference.source_id.len())
        .saturating_add(unit.reference.part.len())
        .saturating_add("\n\n[SOURCE  · ]\n".len())
}

fn publish_complete(
    folder: &Path,
    current: &ContextSet,
    revision: &ContextRevision,
    findings: &Findings,
) -> Result<(), Error> {
    let version = Path::new(VERSIONS).join(&revision.id);
    let index = findings::render_index(&current.title, findings);
    let findings_text = as_file(&revision.findings)?;
    let manifest = as_file(revision)?;
    let manifest_bytes = fs::read(folder.join(MANIFEST))?;
    let expected_manifest = revision_of(&manifest_bytes);
    let mut changed = current.clone();
    changed.latest_ready_revision = Some(revision.id.clone());
    changed.changed_at.clone_from(&revision.created_at);
    let changed = as_file(&changed)?;

    DurableFilePublisher::new(folder)
        .with_initialized_publication(
            "context-versions",
            |root| {
                root.ensure_directory(Path::new(VERSIONS), 0o700)
                    .map_err(|error| publication_io("making the versions folder", &error))
            },
            |batch| {
                // 2026-09-08 (CT-04) — inicjalizacja domeny biegnie raz na generację roota,
                // a identyfikator wersji jest nowy przy KAŻDEJ publikacji. Te dwa katalogi
                // muszą więc powstać w samej partii; w inicjalizatorze druga wersja dostałaby
                // ENOENT mimo poprawnie opublikowanej pierwszej.
                batch
                    .root()
                    .ensure_directory(&version, 0o700)
                    .map_err(|error| publication_io("making the new version folder", &error))?;
                batch
                    .root()
                    .ensure_directory(&version.join(TOPICS), 0o700)
                    .map_err(|error| publication_io("making the topics folder", &error))?;
                immutable("writing the short index", batch.atomic_create_if_absent(
                    &folder.join(&version).join(INDEX),
                    index.as_bytes(),
                    ModePolicy::PreserveExistingOr(DEFINITION_FILE_MODE),
                ))?;
                immutable("writing the findings", batch.atomic_create_if_absent(
                    &folder.join(&version).join(FINDINGS),
                    findings_text.as_bytes(),
                    ModePolicy::PreserveExistingOr(DEFINITION_FILE_MODE),
                ))?;
                for topic in &revision.topics {
                    let text = findings::render_topic(topic, &revision.findings);
                    immutable("writing a topic", batch.atomic_create_if_absent(
                        &folder
                            .join(&version)
                            .join(TOPICS)
                            .join(format!("{}.md", topic.id)),
                        text.as_bytes(),
                        ModePolicy::PreserveExistingOr(DEFINITION_FILE_MODE),
                    ))?;
                }
                for file in std::iter::once(INDEX)
                    .chain(std::iter::once(FINDINGS))
                    .chain(revision.topic_files.iter().map(String::as_str))
                {
                    if !batch
                        .root()
                        .regular_file_exists(&version.join(file))
                        .map_err(|error| publication_io("checking a new version file", &error))?
                    {
                        return Err(PublishError::Io(io::Error::other(
                            "a context version file was missing before its manifest",
                        )));
                    }
                }
                immutable("writing the version manifest", batch.atomic_create_if_absent(
                    &folder.join(&version).join(MANIFEST),
                    manifest.as_bytes(),
                    ModePolicy::PreserveExistingOr(DEFINITION_FILE_MODE),
                ))?;
                if !batch.root().regular_file_exists(&version.join(MANIFEST))? {
                    return Err(PublishError::Io(io::Error::other(
                        "the context version manifest was missing before it became ready",
                    )));
                }
                // Wskaźnik NA KOŃCU: przerwanie dowolnego zapisu wyżej zostawia wcześniejszą
                // gotową wersję bieżącą (2026-09-08, CT-04; PLAN §4).
                batch.publish_definition(
                    &folder.join(MANIFEST),
                    changed.as_bytes(),
                    ModePolicy::PreserveExistingOr(DEFINITION_FILE_MODE),
                    Some(&expected_manifest),
                )
            },
        )
        .map_err(|error| {
            Error::BuildFailed(format!(
                "The new version could not be published completely: {}. Try Rebuild context; the earlier ready version is unchanged.",
                error.into_io()
            ))
        })
}

fn immutable(action: &str, result: Result<(), PublishError>) -> Result<(), PublishError> {
    match result {
        Ok(()) | Err(PublishError::Conflict { .. }) => Ok(()),
        Err(PublishError::Io(error)) => Err(publication_io(action, &error)),
        Err(error) => Err(error),
    }
}

fn publication_io(action: &str, error: &io::Error) -> PublishError {
    PublishError::Io(io::Error::new(error.kind(), format!("{action}: {error}")))
}

fn as_file<T: Serialize>(value: &T) -> Result<String, Error> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    Ok(text)
}

fn fingerprint<T: Serialize>(value: &T) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).unwrap_or_default())
    )
}

fn error_to_io(error: &Error) -> io::Error {
    io::Error::other(error.to_string())
}
