//! Jedna odpowiedź na aliasy katalogów dla wykonawcy i rachunku przed Startem.
use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use super::Unrolled;
use crate::engine::dag::Dag;
use crate::workflow::{Folder, Step, WorkflowFile, execution::RunInputs};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Root {
    Project,
    Picked(PathBuf),
    Managed(String),
}

impl Root {
    #[must_use]
    pub fn at(&self, project: &Path, work: &Path) -> PathBuf {
        match self {
            Self::Project => project.to_path_buf(),
            Self::Picked(path) => path.clone(),
            Self::Managed(key) => work.join(key),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct NodeFolder {
    pub root: Option<Root>,
    /// Najbliższy węzeł nazywający miejsce i jego alias. Nazwa dla człowieka pochodzi z węzła.
    pub parents: Vec<(usize, Root)>,
    /// Fresh/Project/Pick albo fizyczne składanie; zwykłe `SameCopy` jest przezroczyste.
    pub establishes: bool,
}

#[derive(Debug, Clone)]
pub struct CopyPlan {
    pub nodes: Vec<NodeFolder>,
    pub managed: BTreeSet<String>,
}

fn folder(step: &Step) -> Option<&Folder> {
    match step {
        Step::Agent(one) => Some(&one.folder),
        Step::Check(one) => Some(&one.folder),
        Step::Serve(one) => Some(&one.folder),
        Step::Checkpoint(_) => None,
    }
}

/// `managed_project` jest hostową decyzją: Lab zamienia również nieprzypisany Project
/// na prywatną kopię komórki. Zwykły Run zarządza wyłącznie Project jawnych zakresów.
pub fn working_folders(
    file: &WorkflowFile,
    expanded: &Unrolled,
    inputs: &RunInputs,
    managed_project: bool,
) -> Result<CopyPlan, String> {
    plan(file, expanded, inputs, managed_project, None)
}

/// Wykonanie zna konkretne adresy: Project oraz Pick tego samego katalogu są jednym
/// miejscem. Preflight Labu nie ma Pick, więc wystarczają mu symboliczne klucze.
pub fn working_folders_at(
    file: &WorkflowFile,
    expanded: &Unrolled,
    inputs: &RunInputs,
    project: &Path,
    work: &Path,
) -> Result<CopyPlan, String> {
    plan(file, expanded, inputs, false, Some((project, work)))
}

fn plan(
    file: &WorkflowFile,
    expanded: &Unrolled,
    inputs: &RunInputs,
    managed_project: bool,
    places: Option<(&Path, &Path)>,
) -> Result<CopyPlan, String> {
    let dag =
        Dag::new(expanded.nodes.len(), &expanded.arrows).map_err(|error| error.to_string())?;
    let mut incoming = vec![Vec::new(); expanded.nodes.len()];
    for &(from, to) in &expanded.arrows {
        incoming[to].push(from);
    }
    let mut degree = dag.in_degree();
    let mut ready: VecDeque<_> = degree
        .iter()
        .enumerate()
        .filter_map(|(at, count)| (*count == 0).then_some(at))
        .collect();
    let mut result = CopyPlan {
        nodes: vec![NodeFolder::default(); expanded.nodes.len()],
        managed: BTreeSet::new(),
    };
    let unscoped_project = file
        .steps
        .iter()
        .find(|step| {
            inputs.context_key(step.id()).is_none() && matches!(folder(step), Some(Folder::Project))
        })
        .map(Step::id);
    while let Some(at) = ready.pop_front() {
        let node = expanded.nodes[at];
        let step = file
            .steps
            .get(node.step)
            .ok_or("A working folder names an unknown workflow step.")?;
        let work = super::super::check::work_key_for(step.id(), node.copy);
        let parents = parents_before(at, &incoming, &result.nodes, places);
        let named = match folder(step) {
            Some(Folder::Project) => {
                let owner = inputs
                    .project_owner(file, step.id())
                    .or_else(|| managed_project.then_some(unscoped_project).flatten());
                Some(owner.map_or(Root::Project, |key| Root::Managed(key.to_owned())))
            }
            Some(Folder::Pick { path }) => Some(Root::Picked(PathBuf::from(path))),
            Some(Folder::FreshCopy) => Some(Root::Managed(work.clone())),
            Some(Folder::SameCopy) | None => None,
        };
        let (root, establishes) = if let Some(root) = named {
            (Some(root), true)
        } else if matches!(folder(step), Some(Folder::SameCopy)) {
            match parents.as_slice() {
                [] => (None, false),
                [(_, root)] => (Some(root.clone()), false),
                _ => (Some(Root::Managed(work)), true),
            }
        } else {
            (None, false)
        };
        if let Some(Root::Managed(key)) = &root {
            result.managed.insert(key.clone());
        }
        result.nodes[at] = NodeFolder {
            root,
            parents,
            establishes,
        };
        for &child in &dag.children()[at] {
            degree[child] -= 1;
            if degree[child] == 0 {
                ready.push_back(child);
            }
        }
    }
    Ok(result)
}

/// Ten sam najbliższy-przodek, co dawny `trees_before`, ale wyliczone wcześniej aliasy
/// usuwają wzajemną rekurencję. Kolejność strzałek i przezroczystych węzłów pozostaje.
fn parents_before(
    node: usize,
    incoming: &[Vec<usize>],
    known: &[NodeFolder],
    places: Option<(&Path, &Path)>,
) -> Vec<(usize, Root)> {
    let mut seen = vec![false; known.len()];
    seen[node] = true;
    let mut stack = vec![node];
    let mut found = Vec::new();
    while let Some(at) = stack.pop() {
        for &from in &incoming[at] {
            if seen[from] {
                continue;
            }
            seen[from] = true;
            let parent = &known[from];
            if parent.establishes {
                if let Some(root) = &parent.root
                    && !found
                        .iter()
                        .any(|(_, existing): &(usize, Root)| match places {
                            Some((project, work)) => {
                                existing.at(project, work) == root.at(project, work)
                            }
                            None => existing == root,
                        })
                {
                    found.push((from, root.clone()));
                }
            } else {
                stack.push(from);
            }
        }
    }
    found
}
