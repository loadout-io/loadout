//! AC-3 dla T-23: łańcuch jest łańcuchem harnessu, a „workspace" jest właściwością kroku, nie
//! kafelkiem.
//!
//! `assert_eq!(steps.len(), 5)` przechodzi na gwieździe, na grafie rozłącznym i na pięciu
//! kafelkach bez ani jednej strzałki. Liczba kroków nie odróżnia łańcucha od niczego. Odróżniają
//! go trzy rzeczy naraz: jedyne źródło, jedyne ujście i pełny porządek przechodni — wszystkie
//! cztery pary po kolei, a nie sama obecność czterech strzałek.
//!
//! Drugą połową kryterium jest to, czego w pliku NIE MA. „Workspace" to pierwszy etap harnessu
//! (`worktree.sh` wycina własną kopię repo), a mimo to nie dostaje kafelka: własna kopia plików
//! jest polem kroku implementacji (T3 §3.1, `Folder`). Kafelek „utwórz workspace" byłby agentem
//! raportującym własny efekt uboczny — czyli dokładnie tym rozróżnieniem „co agent powiedział" vs
//! „co się stało", dla którego ten produkt powstał. Dlatego liczba kroków wynosi 5, a nie 6,
//! i dlatego `s_implement` musi nieść `fresh-copy`.
//!
//! Osiągalność liczymy z krawędzi w tym pliku, obchodem iteracyjnym. To nie jest drugi walidator:
//! T-12 nie wystawia domknięcia przechodniego, a zbudowanie go z gotowej listy strzałek to
//! kilkanaście wierszy, nie drugi algorytm.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use loadout_lib::workflow::file;
use loadout_lib::workflow::{AgentStep, Folder, Link, Step, WorkflowFile};

/// Łańcuch harnessu w kolejności, w jakiej biegnie.
const CHAIN: [&str; 5] = ["s_implement", "s_gate", "s_review", "s_fix", "s_land"];

fn graph_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../.loadout/workflows/ship-task.json")
}

fn load() -> Result<WorkflowFile, Box<dyn Error>> {
    let path = graph_path();
    assert!(
        path.exists(),
        "the harness workflow has not been written yet: {}",
        path.display()
    );
    Ok(file::load(&path)?)
}

fn id(step: &Step) -> &str {
    match step {
        Step::Agent(agent) => &agent.id,
        Step::Checkpoint(checkpoint) => &checkpoint.id,
    }
}

fn agent_step<'a>(workflow: &'a WorkflowFile, wanted: &str) -> Option<&'a AgentStep> {
    workflow.steps.iter().find_map(|step| match step {
        Step::Agent(agent) if agent.id == wanted => Some(agent),
        _ => None,
    })
}

/// Domknięcie przechodnie strzałek: `reach[a][b]` znaczy „`b` biegnie kiedyś po `a`".
///
/// Obchód iteracyjny, ze zbiorem odwiedzonych — graf z kołem ma się skończyć tak samo jak każdy
/// inny, a nie przepełnić stos.
fn reachable(ids: &[&str], links: &[Link]) -> Vec<Vec<bool>> {
    let position: BTreeMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect();

    let mut next: Vec<Vec<usize>> = vec![Vec::new(); ids.len()];
    for link in links {
        if let (Some(&from), Some(&to)) = (
            position.get(link.from.as_str()),
            position.get(link.to.as_str()),
        ) {
            next[from].push(to);
        }
    }

    let mut reach = vec![vec![false; ids.len()]; ids.len()];
    let mut stack: Vec<usize> = Vec::new();
    for (start, from_here) in reach.iter_mut().enumerate() {
        stack.push(start);
        while let Some(step) = stack.pop() {
            for &after in &next[step] {
                if !from_here[after] {
                    from_here[after] = true;
                    stack.push(after);
                }
            }
        }
    }
    reach
}

#[test]
fn the_workspace_stage_is_a_field_on_a_step_and_not_a_step() -> Result<(), Box<dyn Error>> {
    let workflow = load()?;

    assert_eq!(
        workflow.steps.len(),
        5,
        "six stages, five tiles. A sixth tile for 'make a workspace' would be an agent step \
         reporting its own side effect — the app would be taking an agent's word for something \
         it can do itself, which is the one distinction this product is built on"
    );

    let implement = agent_step(&workflow, CHAIN[0])
        .ok_or("the workflow has no agent step that writes the code")?;

    assert_eq!(
        implement.folder,
        Folder::FreshCopy,
        "the workspace stage lives here, as a property of the step that needs it: its own copy \
         of your files. Written as a tile it would be a stage; written as a field it is a \
         setting — and only one of those two is true of the harness"
    );
    Ok(())
}

#[test]
fn the_chain_has_one_start_and_one_finish() -> Result<(), Box<dyn Error>> {
    let workflow = load()?;
    let ids: Vec<&str> = workflow.steps.iter().map(id).collect();

    let starts: Vec<&str> = ids
        .iter()
        .copied()
        .filter(|step| !workflow.links.iter().any(|link| link.to == *step))
        .collect();
    let finishes: Vec<&str> = ids
        .iter()
        .copied()
        .filter(|step| !workflow.links.iter().any(|link| link.from == *step))
        .collect();

    assert_eq!(
        starts,
        vec![CHAIN[0]],
        "a second step with nothing before it is a second place the run could begin, and two \
         beginnings is not a chain — it is two branches drawn next to each other"
    );
    assert_eq!(
        finishes,
        vec![CHAIN[4]],
        "landing the branch is the last thing that happens; anything else with no arrow out of \
         it is work the harness would finish without"
    );
    Ok(())
}

#[test]
fn every_step_of_the_harness_runs_after_every_earlier_one() -> Result<(), Box<dyn Error>> {
    let workflow = load()?;
    let ids: Vec<&str> = workflow.steps.iter().map(id).collect();
    let reach = reachable(&ids, &workflow.links);

    let position: BTreeMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect();

    for (earlier, before) in CHAIN.iter().enumerate() {
        for after in CHAIN.iter().skip(earlier + 1) {
            let (Some(&from), Some(&to)) = (position.get(before), position.get(after)) else {
                return Err(format!("the workflow is missing {before} or {after}").into());
            };
            assert!(
                reach[from][to],
                "{after} has to run after {before}, and the whole order has to hold, not just \
                 the four arrows next to each other: four arrows also draw a star. Second \
                 opinion before the checks have run is a reviewer reading code nobody compiled"
            );
        }
    }
    Ok(())
}
