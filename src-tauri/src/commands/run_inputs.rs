//! WF-16: ogólne, jawne wejścia wykonania. Nie ma tu pojęcia Lab ani drugiego schedulera.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};
use std::path::Path;

use serde::Serialize;

use crate::engine::supervisor::PublicationRoot;
use crate::memory::handoff;
pub use crate::workflow::execution::{EndCause, RunInputs};

pub const MAX_CHECK_INPUT_BYTES: usize = 256 * 1024;
const MAX_RESULTS: usize = 64;

/// Numery są wyłącznie lokalną reprezentacją zweryfikowanych `node_key` tego planu.
#[derive(Debug, Default)]
pub struct BoundInputs {
    checks: BTreeMap<usize, Vec<usize>>,
    pub configuration: RunInputs,
}

#[derive(Debug)]
pub struct InputNode<'a> {
    pub node_key: &'a str,
    pub is_check: bool,
}

impl RunInputs {
    pub fn bind_checks(
        self,
        nodes: &[InputNode<'_>],
        arrows: &[(usize, usize)],
    ) -> Result<BoundInputs, String> {
        let by_key: BTreeMap<&str, usize> = nodes
            .iter()
            .enumerate()
            .map(|(at, node)| (node.node_key, at))
            .collect();
        let mut bound = BoundInputs {
            configuration: self.clone(),
            ..BoundInputs::default()
        };
        for (key, input) in self.checks {
            let at = by_key
                .get(key.as_str())
                .copied()
                .filter(|at| nodes[*at].is_check)
                .ok_or_else(|| {
                    format!("The input names a check that is not in this run: {key}.")
                })?;
            if input.results.is_empty() || input.results.len() > MAX_RESULTS {
                return Err(format!(
                    "The input for {key} must name between 1 and {MAX_RESULTS} results."
                ));
            }
            let ancestors = ancestors_of(at, arrows);
            let mut selected = Vec::new();
            for name in input.results {
                let source = by_key.get(name.as_str()).copied()
                    .filter(|source| *source != at && ancestors.contains(source))
                    .ok_or_else(|| format!("The input for {key} names {name}, which is not an exact preceding step in this run."))?;
                if selected.contains(&source) {
                    return Err(format!("The input for {key} names {name} more than once."));
                }
                selected.push(source);
            }
            bound.checks.insert(at, selected);
        }
        Ok(bound)
    }
}

impl BoundInputs {
    #[must_use]
    pub fn for_check(&self, id: usize) -> Option<&[usize]> {
        self.checks.get(&id).map(Vec::as_slice)
    }
    #[must_use]
    pub fn is_producer(&self, id: usize) -> bool {
        self.checks.values().any(|sources| sources.contains(&id))
    }
}

pub(super) fn ancestors_of(at: usize, arrows: &[(usize, usize)]) -> BTreeSet<usize> {
    let mut found = BTreeSet::new();
    let mut todo = vec![at];
    while let Some(child) = todo.pop() {
        for &(parent, to) in arrows {
            if to == child && found.insert(parent) {
                todo.push(parent);
            }
        }
    }
    found
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputResult {
    pub node_key: String,
    pub status: String,
    pub output: Option<String>,
    pub error: Option<String>,
    pub cause: EndCause,
    pub files: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResultFiles {
    pub snapshot: String,
    pub directory: String,
}

pub fn encode(results: &[InputResult]) -> io::Result<String> {
    let value = serde_json::to_string(&serde_json::json!({ "format": 1, "results": results }))
        .map_err(io::Error::other)?;
    if value.len() > MAX_CHECK_INPUT_BYTES {
        return Err(io::Error::other(
            "The selected check input exceeds 256 KiB. Nothing was passed to the command.",
        ));
    }
    Ok(value)
}

/// Trwały adres konkretnej publikacji, nie skan po nazwach i nie ostatnio zakończony krok.
pub fn read_output(
    run_dir: &Path,
    run_id: &str,
    step: usize,
    written: &handoff::Written,
) -> io::Result<String> {
    let root = PublicationRoot::open(run_dir)?;
    let bytes = read_bound(&root, run_dir, &written.path)?;
    let parsed = handoff::parse_handoff(&written.path, &bytes).map_err(io::Error::other)?;
    if parsed.bytes_mismatch()
        || parsed.meta.run != run_id
        || usize::try_from(parsed.meta.step).ok() != Some(step)
    {
        return Err(io::Error::other(
            "The selected result no longer matches the step that published it.",
        ));
    }
    match &written.attachment {
        None => Ok(parsed.body),
        Some(path) => {
            let bytes = read_bound(&root, run_dir, path)?;
            if written.attachment_bytes != Some(bytes.len()) {
                return Err(io::Error::other(
                    "The selected result's full output changed after publication.",
                ));
            }
            String::from_utf8(bytes).map_err(io::Error::other)
        }
    }
}

fn read_bound(root: &PublicationRoot, directory: &Path, path: &Path) -> io::Result<Vec<u8>> {
    let relative = path
        .strip_prefix(directory)
        .map_err(|_| io::Error::other("The result is outside this run."))?;
    let mut bytes = Vec::new();
    root.open_regular_file(relative)?
        .take(MAX_CHECK_INPUT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_CHECK_INPUT_BYTES {
        return Err(io::Error::other(
            "The selected check input exceeds 256 KiB.",
        ));
    }
    Ok(bytes)
}
