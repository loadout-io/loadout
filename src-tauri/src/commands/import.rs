//! Granica IPC importu. Czysty rdzeń mieszka w `crate::import`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::import::apply::ImportReceipt;
use crate::import::{Compatibility, ImportError, ImportPreview, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplySetup {
    pub workspace: PathBuf,
    pub expected_source_hashes: BTreeMap<PathBuf, String>,
    pub enable_connections: Vec<String>,
    /// Elementy `needs_choice`, które człowiek jawnie postanowił zostawić poza migracją.
    #[serde(default)]
    pub leave_out: Vec<String>,
    /// Pozycje, których człowiek nie chce zapisać. Osobne od usunięcia jednego zachowania.
    #[serde(default)]
    pub excluded_items: Vec<String>,
    /// Pozycje zachowane w planie, ale bez zachowania wymagającego rozstrzygnięcia.
    #[serde(default)]
    pub without_behavior: Vec<String>,
}

pub fn scan_setup_inner(home: &Path, workspace: &Path) -> Result<ImportPreview> {
    // Katalog domowy CZŁOWIEKA, nie biblioteka Loadouta: stąd czytamy `~/.claude.json`, żeby
    // serwery zapisane `claude mcp add --scope local|user` też trafiły na listę.
    crate::import::translate::preview_with_personal(workspace, home)
}

/// Jeszcze raz czyta repo i akceptuje z webviewa wyłącznie wybór włączenia znanych połączeń.
pub fn apply_setup_inner(
    home: &Path,
    personal: &Path,
    request: &ApplySetup,
) -> Result<ImportReceipt> {
    /* TEN SAM WIDOK, CO PRZY SCANIE. Gdyby tu stała `preview()` bez twoich zakresów, włączenie
     * `linear-server` wracałoby jako „The import requested a connection that was not in the
     * latest Scan." — czyli odmowa dla pozycji, którą ekran właśnie pokazał. */
    let mut preview =
        crate::import::translate::preview_with_personal(&request.workspace, personal)?;
    if preview.draft.source_hashes != request.expected_source_hashes {
        return Err(ImportError::Changed);
    }
    let requested: BTreeSet<&str> = request
        .enable_connections
        .iter()
        .map(String::as_str)
        .collect();
    let known: BTreeSet<&str> = preview
        .draft
        .connections
        .iter()
        .map(|connection| connection.id.as_str())
        .collect();
    if !requested.is_subset(&known) {
        return Err(ImportError::Save(
            "The import requested a connection that was not in the latest Scan.".to_owned(),
        ));
    }
    let leave_out: BTreeSet<&str> = request.leave_out.iter().map(String::as_str).collect();
    let resolvable: BTreeSet<&str> = preview
        .draft
        .report
        .mappings
        .iter()
        .filter(|mapping| mapping.compatibility.blocks())
        .map(|mapping| mapping.item_id.as_str())
        .collect();
    if !leave_out.is_subset(&resolvable) {
        return Err(ImportError::Save(
            "The import tried to leave out an item that was not unresolved in the latest Scan."
                .to_owned(),
        ));
    }
    let excluded_items: BTreeSet<&str> =
        request.excluded_items.iter().map(String::as_str).collect();
    let known_items: BTreeSet<&str> = preview
        .draft
        .items
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    if !excluded_items.is_subset(&known_items) {
        return Err(ImportError::Save(
            "The import tried to exclude an item that was not in the latest Scan.".to_owned(),
        ));
    }
    let without_behavior: BTreeSet<&str> = request
        .without_behavior
        .iter()
        .map(String::as_str)
        .collect();
    let behavior_choices: BTreeSet<&str> = preview
        .draft
        .report
        .mappings
        .iter()
        .filter(|mapping| mapping.compatibility == Compatibility::NeedsChoice)
        .map(|mapping| mapping.item_id.as_str())
        .collect();
    if !without_behavior.is_subset(&behavior_choices) {
        return Err(ImportError::Save(
            "The import tried to remove behavior from an item that did not offer that choice in the latest Scan."
                .to_owned(),
        ));
    }
    let excluded: BTreeSet<&str> = leave_out.union(&excluded_items).copied().collect();
    if !excluded.is_disjoint(&without_behavior) {
        return Err(ImportError::Save(
            "An imported item cannot be excluded and kept without behavior at the same time."
                .to_owned(),
        ));
    }
    for mapping in &mut preview.draft.report.mappings {
        if excluded.contains(mapping.item_id.as_str()) {
            mapping.compatibility = Compatibility::Adjusted;
            "You chose not to import this item.".clone_into(&mut mapping.message);
        } else if without_behavior.contains(mapping.item_id.as_str()) {
            mapping.compatibility = Compatibility::Adjusted;
            "This item will be imported without that project behavior."
                .clone_into(&mut mapping.message);
        }
    }
    preview
        .draft
        .items
        .retain(|item| !excluded.contains(item.id.as_str()));
    crate::import::translate::keep_selected_outputs(&mut preview.draft);
    for connection in &mut preview.draft.connections {
        connection.enabled = requested.contains(connection.id.as_str());
    }
    crate::import::translate::refresh_statuses(&mut preview.draft);
    crate::import::apply::apply(home, &preview.draft)
}
