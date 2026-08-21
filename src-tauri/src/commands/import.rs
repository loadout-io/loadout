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
}

pub fn scan_setup_inner(workspace: &Path) -> Result<ImportPreview> {
    crate::import::translate::preview(workspace)
}

/// Jeszcze raz czyta repo i akceptuje z webviewa wyłącznie wybór włączenia znanych połączeń.
pub fn apply_setup_inner(home: &Path, request: &ApplySetup) -> Result<ImportReceipt> {
    let mut preview = crate::import::translate::preview(&request.workspace)?;
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
        .filter(|mapping| mapping.compatibility == Compatibility::NeedsChoice)
        .map(|mapping| mapping.item_id.as_str())
        .collect();
    if !leave_out.is_subset(&resolvable) {
        return Err(ImportError::Save(
            "The import tried to resolve an item that was not waiting for a choice in the latest Scan."
                .to_owned(),
        ));
    }
    for mapping in &mut preview.draft.report.mappings {
        if leave_out.contains(mapping.item_id.as_str()) {
            mapping.compatibility = Compatibility::Adjusted;
            "You chose to leave this project behavior out.".clone_into(&mut mapping.message);
        }
    }
    for connection in &mut preview.draft.connections {
        connection.enabled = requested.contains(connection.id.as_str());
    }
    crate::import::apply::apply(home, &preview.draft)
}
