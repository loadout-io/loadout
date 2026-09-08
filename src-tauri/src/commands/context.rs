//! Cztery komendy biblioteki Context: wypisz, przeczytaj, utwórz, zapisz.
//!
//! CO TA WARSTWA NAPRAWDĘ ROBI, skoro cała mechanika plików stoi w [`crate::context::files`]:
//! trzyma dwie rzeczy, których tamten moduł znać nie ma i których `ipc.rs` znać nie powinien.
//!
//! - **gdzie jest biblioteka.** Odwzorowanie `home → contexts/` mieszka tu w jednym miejscu, więc
//!   skorupa `#[tauri::command]` nie zna układu katalogów, a drugie okno nie ma jak zapytać
//!   o inny (ta sama umowa, co `memory::notes_root` w `list_notes`).
//! - **zegar.** `at` opisuje chwilę, w której kliknął CZŁOWIEK, a nie moment, w którym bajty
//!   dotarły na dysk — więc podaje go warstwa, która o kliknięciu wie, a nie funkcja zapisu.

use std::collections::BTreeSet;
use std::path::Path;

use crate::context::files::{self, DraftEdit};
use crate::context::{ContextDraft, ContextSet, ContextSetRead, Error};
use crate::library::definition::Definition;
use crate::workflow::Step;

/// Wszystkie gotowe zestawy tej biblioteki.
pub fn list_context_sets_inner(home: &Path) -> Result<Vec<ContextSet>, Error> {
    files::list_sets(&files::library_root(home))
}

pub fn list_context_sets_by_archive_inner(
    home: &Path,
    archived: bool,
) -> Result<Vec<ContextSet>, Error> {
    files::list_sets_by_archive(&files::library_root(home), archived)
}

/// Jeden zestaw w całości — manifest, szkic i rewizja, którą okno odda przy zapisie.
pub fn read_context_set_inner(home: &Path, id: &str) -> Result<ContextSetRead, Error> {
    files::read_set(&files::library_root(home), id)
}

/// Nowy zestaw pod nazwą, którą wpisał człowiek.
pub fn create_context_set_inner(home: &Path, title: &str) -> Result<ContextSetRead, Error> {
    files::create_set(&files::library_root(home), title, &super::now_utc())
}

/// Zapisuje tytuł, opis i szkic — albo odmawia, kiedy na dysku leży nowsza praca.
pub fn save_context_draft_inner(
    home: &Path,
    id: &str,
    title: &str,
    description: &str,
    draft: ContextDraft,
    expected_revision: Option<String>,
) -> Result<ContextSetRead, Error> {
    files::save_draft(
        &files::library_root(home),
        &DraftEdit {
            id: id.to_owned(),
            title: title.to_owned(),
            description: description.to_owned(),
            draft,
            expected_revision,
            at: super::now_utc(),
        },
    )
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteContextSet {
    pub set_id: String,
    pub title: String,
    pub uses: Vec<String>,
    pub deleted: bool,
    pub said: String,
}

pub fn archive_context_set_inner(
    home: &Path,
    id: &str,
    archived: bool,
) -> Result<ContextSet, Error> {
    files::set_archived(&files::library_root(home), id, archived, &super::now_utc())
}

pub fn delete_context_set_inner(
    home: &Path,
    project: &Path,
    id: &str,
    confirmed: bool,
) -> Result<DeleteContextSet, String> {
    let library = files::library_root(home);
    let set = files::read_set(&library, id).map_err(|error| error.to_string())?;
    let uses = uses_in_workflows(home, project, id)?;
    let said = delete_sentence(&set.set.title, &uses);
    if confirmed {
        files::delete_set(&library, id).map_err(|error| error.to_string())?;
    }
    Ok(DeleteContextSet {
        set_id: id.to_owned(),
        title: set.set.title,
        uses,
        deleted: confirmed,
        said,
    })
}

pub fn uses_in_workflows(home: &Path, project: &Path, id: &str) -> Result<Vec<String>, String> {
    let definitions = super::workflows::list_workflow_definitions_inner(home, Some(project))
        .map_err(|error| error.to_string())?;
    let mut uses = BTreeSet::new();
    for definition in definitions {
        let Definition::Healthy { value, .. } = definition else {
            continue;
        };
        if workflow_uses(&value.workflow, id)? {
            uses.insert(value.workflow.name);
        }
    }
    Ok(uses.into_iter().collect())
}

fn workflow_uses(file: &crate::workflow::WorkflowFile, id: &str) -> Result<bool, String> {
    if file
        .context()?
        .is_some_and(|context| context.sets.iter().any(|pin| pin.id == id))
    {
        return Ok(true);
    }
    for step in &file.steps {
        let Step::Agent(agent) = step else {
            continue;
        };
        if agent.context()?.is_some_and(|context| {
            context.sets.iter().any(|pin| pin.id == id)
                || context.exclude.iter().any(|excluded| excluded == id)
        }) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn delete_sentence(title: &str, uses: &[String]) -> String {
    let use_text = match uses {
        [] => "No workflow uses it.".to_owned(),
        [one] => format!("It is used in {one}."),
        many => format!("It is used in {}.", many.join(", ")),
    };
    format!(
        "Delete {title}? {use_text} Saved runs keep their historical copies, but future starts using this set will stop until you choose another ready version."
    )
}
