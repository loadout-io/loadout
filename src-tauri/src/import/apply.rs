//! Atomowe zapisanie zatwierdzonego draftu.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::library::agents::write_agent_file;
use crate::skills::ingest;

use super::{ADAPTER_VERSION, ImportError, MigrationDraft, Result, translate};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReceipt {
    pub id: String,
    pub adapter_version: u32,
    pub source_hashes: BTreeMap<PathBuf, String>,
    pub workflow_hashes: BTreeMap<PathBuf, String>,
    pub written: Vec<PathBuf>,
    pub enabled_connections: Vec<String>,
    pub vendor_configurations: crate::connections::runtime::VendorConfigurations,
}

/// Zapisuje cały zaakceptowany setup albo nie zostawia żadnego z jego plików.
pub fn apply(home: &Path, draft: &MigrationDraft) -> Result<ImportReceipt> {
    apply_with_hook(home, draft, |_| Ok(()))
}

/// Hak jest miejscem fault-injection dla kryterium atomowości, nie polityką produktu.
pub fn apply_with_hook<F>(
    home: &Path,
    draft: &MigrationDraft,
    mut after_move: F,
) -> Result<ImportReceipt>
where
    F: FnMut(usize) -> std::result::Result<(), String>,
{
    if !draft.runnable() {
        return Err(ImportError::Blocked(draft.report.blockers()));
    }
    let fresh = translate::preview(&draft.root)?;
    if fresh.draft.source_hashes != draft.source_hashes {
        return Err(ImportError::Changed);
    }

    let receipt_id = Uuid::now_v7().to_string();
    preflight(home, draft)?;
    fs::create_dir_all(home).map_err(save_error)?;
    let stage = home.join(format!(".import-{receipt_id}.staging"));
    fs::create_dir(&stage).map_err(save_error)?;

    let result = stage_all(&stage, draft, &receipt_id)
        .and_then(|receipt| commit(home, &stage, receipt, &mut after_move));
    if stage.exists() {
        fs::remove_dir_all(&stage).map_err(save_error)?;
    }
    result
}

fn preflight(home: &Path, draft: &MigrationDraft) -> Result<()> {
    let mut targets = Vec::new();
    targets.extend(
        draft
            .agents
            .iter()
            .map(|agent| PathBuf::from("agents").join(format!("{}.md", slug(&agent.name)))),
    );
    targets.extend(
        draft
            .skills
            .iter()
            .map(|skill| PathBuf::from("skills").join(&skill.name)),
    );
    targets.extend(
        draft
            .connections
            .iter()
            .map(|connection| PathBuf::from("connections").join(format!("{}.json", connection.id))),
    );
    targets.extend(
        draft.workflows.iter().map(|workflow| {
            PathBuf::from("workflows").join(format!("{}.json", slug(&workflow.name)))
        }),
    );
    // Nazwa pliku notatki jest funkcją jej tytułu (`memory::slugify`), więc dwie strony pamięci
    // o tym samym tytule wskazują jeden plik. Bez tej linii druga po cichu podbiłaby licznik
    // wystąpień pierwszej i zniknęłaby jako osobne zdanie.
    targets.extend(draft.notes.iter().map(|note| note_target(&note.title)));

    let mut unique = BTreeSet::new();
    for target in targets {
        if !unique.insert(target.clone()) {
            return Err(ImportError::Save(format!(
                "Two imported items would both become {}. Choose different names before importing.",
                target.display()
            )));
        }
        if home.join(&target).exists() {
            return Err(ImportError::Save(format!(
                "{} already exists. Nothing was imported.",
                target.display()
            )));
        }
    }
    Ok(())
}

fn stage_all(stage: &Path, draft: &MigrationDraft, receipt_id: &str) -> Result<ImportReceipt> {
    let mut written = Vec::new();
    let mut workflow_hashes = BTreeMap::new();

    for agent in &draft.agents {
        let path = write_agent_file(&stage.join("agents"), agent)
            .map_err(|error| ImportError::Save(error.to_string()))?;
        written.push(relative(stage, &path)?);
    }

    for skill in &draft.skills {
        let imported = ingest::from_folder(&skill.source_dir)
            .map_err(|error| ImportError::Save(error.to_string()))?;
        let destination = stage.join("skills").join(&skill.name);
        fs::create_dir_all(&destination).map_err(save_error)?;
        // Review rozstrzyga bezpieczeństwo bundle, ale import jest migawką. Ponowna emisja
        // frontmatteru gubiłaby komentarze i formatowanie, choć raport obiecuje zachowanie pliku.
        fs::copy(
            skill.source_dir.join("SKILL.md"),
            destination.join("SKILL.md"),
        )
        .map_err(save_error)?;
        written.push(PathBuf::from("skills").join(&skill.name).join("SKILL.md"));
        for bundled in &imported.skill.files {
            let target = destination.join(&bundled.relative);
            let parent = target.parent().ok_or_else(|| {
                ImportError::Save("A bundled skill file has no parent folder.".to_owned())
            })?;
            fs::create_dir_all(parent).map_err(save_error)?;
            fs::copy(&bundled.source, &target).map_err(save_error)?;
            written.push(
                PathBuf::from("skills")
                    .join(&skill.name)
                    .join(&bundled.relative),
            );
        }
    }

    for connection in &draft.connections {
        let relative = PathBuf::from("connections").join(format!("{}.json", connection.id));
        write_json(&stage.join(&relative), connection)?;
        written.push(relative);
    }

    // Pamięć cudzego projektu jako PLIKI NOTATEK (2026-08-22, T-80). Draft jest wartością
    // w pamięci; pytanie brzmi „czy w bibliotece leży notatka", a na to odpowiada wyłącznie
    // dysk (niezmiennik 4). Zegar stoi jeden na cały import: dwie notatki przywiezione jednym
    // kliknięciem, które różnią się o sekundę, czytają się jak dwa zdarzenia.
    let at = crate::commands::now_utc();
    for note in &draft.notes {
        let written_note = crate::memory::notes::record_imported(
            &crate::commands::memory::notes_root(stage),
            crate::memory::notes::NoteDraft {
                title: note.title.clone(),
                rule: note.rule.clone(),
                because: note.because.clone(),
                scope: scope_from_word(&note.scope),
                // Rodzaj zdania rozstrzyga człowiek, nie zgadywanie po treści: `fact` jest tym,
                // czym jest zdanie, które ktoś zapisał jako prawdę o swoim projekcie.
                kind: crate::memory::notes::Kind::Fact,
                // Czytane i wyrzucane przez pisarza notatek — import przywozi zdanie, nie zgodę
                // na nie (ARCHITECTURE §2 pyt. 5).
                status: crate::memory::notes::Status::Suggested,
                at: at.clone(),
            },
            note.agent.as_deref(),
            &crate::memory::notes::Origin {
                from: super::project_name(&draft.root),
                source: note.source.clone(),
                source_hash: note.source_hash.clone(),
                app: app_word(note.app).to_owned(),
            },
        )
        .map_err(|error| ImportError::Save(error.to_string()))?;
        written.push(relative(stage, &written_note.path)?);
    }

    for workflow in &draft.workflows {
        let relative = PathBuf::from("workflows").join(format!("{}.json", slug(&workflow.name)));
        let target = stage.join(&relative);
        create_parent(&target)?;
        crate::workflow::file::save(workflow, &target)
            .map_err(|error| ImportError::Save(error.to_string()))?;
        workflow_hashes.insert(
            relative.clone(),
            fingerprint(&fs::read(&target).map_err(save_error)?),
        );
        written.push(relative);
    }

    written.sort();
    let mut enabled_connections: Vec<String> = draft
        .connections
        .iter()
        .filter(|connection| connection.enabled)
        .map(|connection| connection.id.clone())
        .collect();
    enabled_connections.sort();
    let receipt_path = PathBuf::from("imports").join(format!("{receipt_id}.json"));
    written.push(receipt_path.clone());
    let receipt = ImportReceipt {
        id: receipt_id.to_owned(),
        adapter_version: ADAPTER_VERSION,
        source_hashes: draft.source_hashes.clone(),
        workflow_hashes,
        written,
        enabled_connections,
        vendor_configurations: crate::connections::runtime::for_connections(&draft.connections),
    };
    write_json(&stage.join(&receipt_path), &receipt)?;
    Ok(receipt)
}

fn commit<F>(
    home: &Path,
    stage: &Path,
    receipt: ImportReceipt,
    after_move: &mut F,
) -> Result<ImportReceipt>
where
    F: FnMut(usize) -> std::result::Result<(), String>,
{
    for relative in &receipt.written {
        if home.join(relative).exists() {
            return Err(ImportError::Save(format!(
                "{} already exists. Nothing was imported.",
                relative.display()
            )));
        }
    }

    let mut moved = Vec::new();
    let mut made_dirs = Vec::new();
    for relative in &receipt.written {
        let source = stage.join(relative);
        let target = home.join(relative);
        if let Err(error) = create_parent_recording(&target, &mut made_dirs)
            .and_then(|()| fs::rename(&source, &target))
        {
            rollback(home, stage, &moved, &made_dirs).map_err(|rollback| {
                ImportError::Save(format!(
                    "Import failed ({error}) and Loadout could not fully restore the library ({rollback})."
                ))
            })?;
            return Err(save_error(error));
        }
        moved.push(relative.clone());
        if let Err(detail) = after_move(moved.len()) {
            rollback(home, stage, &moved, &made_dirs).map_err(|rollback| {
                ImportError::Save(format!(
                    "Import stopped ({detail}) and Loadout could not fully restore the library ({rollback})."
                ))
            })?;
            return Err(ImportError::Save(detail));
        }
    }
    Ok(receipt)
}

fn rollback(
    home: &Path,
    stage: &Path,
    moved: &[PathBuf],
    made_dirs: &[PathBuf],
) -> std::io::Result<()> {
    for relative in moved.iter().rev() {
        let source = home.join(relative);
        let target = stage.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(source, target)?;
    }
    for directory in made_dirs.iter().rev() {
        fs::remove_dir(directory)?;
    }
    Ok(())
}

fn create_parent_recording(path: &Path, made: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let mut missing = Vec::new();
    let mut cursor = parent;
    while !cursor.exists() {
        missing.push(cursor.to_path_buf());
        let Some(next) = cursor.parent() else {
            break;
        };
        cursor = next;
    }
    for directory in missing.iter().rev() {
        fs::create_dir(directory)?;
        made.push(directory.clone());
    }
    Ok(())
}

fn create_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| ImportError::Save("An imported file has no parent folder.".to_owned()))?;
    fs::create_dir_all(parent).map_err(save_error)
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    create_parent(path)?;
    let mut text = serde_json::to_string_pretty(value)
        .map_err(|error| ImportError::Save(error.to_string()))?;
    text.push('\n');
    fs::write(path, text).map_err(save_error)
}

fn relative(root: &Path, path: &Path) -> Result<PathBuf> {
    path.strip_prefix(root)
        .map(Path::to_path_buf)
        .map_err(|error| ImportError::Save(error.to_string()))
}

fn save_error(error: std::io::Error) -> ImportError {
    let detail = error.to_string();
    drop(error);
    ImportError::Save(detail)
}

/// Gdzie w bibliotece wyląduje notatka o tym tytule.
///
/// Ta sama nazwa katalogu, co [`crate::commands::memory::notes_root`], i ta sama nazwa pliku,
/// co [`crate::memory::notes::record_imported`] — obie policzone ich własnymi funkcjami. Druga
/// odpowiedź na pytanie „gdzie leży notatka" rozjechałaby się z pierwszą przy pierwszej zmianie
/// któregokolwiek z nich, a rozjazd byłoby widać dopiero jako import, który nie odmówił nadpisania.
fn note_target(title: &str) -> PathBuf {
    let root = crate::commands::memory::notes_root(Path::new(""));
    root.join("notes")
        .join(format!("{}.md", crate::memory::slugify(title)))
}

/// Słowo zakresu z drafu na typ notatki. Nieznane czyta się jako zakres projektu — ten sam
/// kierunek błędu, co w `memory::notes::scope_from`: nigdy szerzej, niż napisano.
fn scope_from_word(word: &str) -> crate::memory::notes::Scope {
    match word {
        "everywhere" => crate::memory::notes::Scope::Everywhere,
        "this-agent" => crate::memory::notes::Scope::ThisAgent,
        _ => crate::memory::notes::Scope::ThisProject,
    }
}

/// Z czyjego katalogu wzięliśmy to zdanie — słowem, nie numerem wariantu.
///
/// Wypisane, a nie wzięte z `serde`: ta wartość ląduje w pliku, który czyta człowiek w edytorze,
/// więc przemianowanie wariantu w Ruście nie ma prawa zmienić tego, co stoi w notatkach, które
/// już leżą na dysku.
const fn app_word(app: super::SourceKind) -> &'static str {
    match app {
        super::SourceKind::Claude => "claude",
        super::SourceKind::Codex => "codex",
        super::SourceKind::AgentSkills => "agent-skills",
        super::SourceKind::Rulesync => "rulesync",
        super::SourceKind::OpenStandard => "open-standard",
        super::SourceKind::Unknown => "unknown",
    }
}

fn slug(value: &str) -> String {
    let mut out = String::new();
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            out.push(character);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

fn fingerprint(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
