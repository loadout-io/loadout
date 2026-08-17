//! AC-4 dla T-23: druga opinia jest innego agenta niż pisarz, a poprawkę wykonuje ten, kto pisał.
//!
//! Trzy zdania z `06 §10.11` i decyzji D3, zapisane jako relacje między krokami, a nie jako nazwy:
//! test nie zakłada, że pisarzem jest akurat Codex ani że recenzentem jest akurat Claude — cztery
//! kombinacje mają działać, więc kryterium, które przybija konkretnego vendora, byłoby fałszem
//! o trzech z nich.
//!
//! `assert_ne!(review.agent, implement.agent)` samo w sobie przechodzi na pliku, w którym
//! poprawkę też robi recenzent. To jest odwrotność reguły: recenzent planuje tylko do odczytu,
//! a wykonuje pisarz — plan, którego sam nie napisał. Recenzent piszący kod jest recenzentem
//! recenzującym siebie o jedną rundę później.
//!
//! Trzeci warunek jest strukturalny i to on jest tu najciekawszy. Schemat odpowiedzi recenzenta ma
//! `verdict ∈ {concern, none}` — nie ma czego zatwierdzić i nie ma czym zablokować. Krawędź
//! z `s_review` wracająca do wcześniejszego kroku dałaby mu tę władzę z powrotem, tylnymi drzwiami
//! w grafie: „wróć i popraw, aż uwag nie będzie" to jest właśnie ta nieograniczona pętla recenzji,
//! przez którą jedno zadanie zajmuje cały dzień. Sprawdzamy więc nie dwie nazwane krawędzie, tylko
//! wszystkie kroki, które biegną PRZED `s_review` — dziś są to `s_implement` i `s_gate`, jutro
//! może dojść trzeci i reguła ma go objąć bez poprawki.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use loadout_lib::workflow::file;
use loadout_lib::workflow::{AgentStep, Link, Step, WorkflowFile};

const IMPLEMENT: &str = "s_implement";
const REVIEW: &str = "s_review";
const FIX: &str = "s_fix";

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

fn agent_step<'a>(workflow: &'a WorkflowFile, wanted: &str) -> Result<&'a AgentStep, String> {
    workflow
        .steps
        .iter()
        .find_map(|step| match step {
            Step::Agent(agent) if agent.id == wanted => Some(agent),
            _ => None,
        })
        .ok_or_else(|| format!("the workflow has no agent step called {wanted}"))
}

/// Domknięcie przechodnie strzałek: `reach[a][b]` znaczy „`b` biegnie kiedyś po `a`".
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
fn the_second_opinion_is_not_the_one_who_wrote_the_code() -> Result<(), Box<dyn Error>> {
    let workflow = load()?;

    let implement = agent_step(&workflow, IMPLEMENT)?;
    let review = agent_step(&workflow, REVIEW)?;

    assert_ne!(
        review.agent, implement.agent,
        "a second opinion from the one who wrote the code is a first opinion read twice. Every \
         real defect in the first version of the source harness was found by the other vendor on \
         a green board, which is why cross-vendor is the default"
    );
    Ok(())
}

#[test]
fn the_fix_is_carried_out_by_the_one_who_wrote_the_code() -> Result<(), Box<dyn Error>> {
    let workflow = load()?;

    let implement = agent_step(&workflow, IMPLEMENT)?;
    let fix = agent_step(&workflow, FIX)?;

    assert_eq!(
        fix.agent, implement.agent,
        "the reviewer plans, read-only, and the writer carries out a plan it did not write. A \
         reviewer that writes the fix is a reviewer reviewing itself one round later — and \
         'the reviewer is different from the writer' alone reads the same either way"
    );
    Ok(())
}

#[test]
fn the_second_opinion_has_no_arrow_that_goes_back() -> Result<(), Box<dyn Error>> {
    let workflow = load()?;
    let ids: Vec<&str> = workflow.steps.iter().map(id).collect();
    let reach = reachable(&ids, &workflow.links);

    let position: BTreeMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect();
    let &review = position
        .get(REVIEW)
        .ok_or("the workflow has no step for the second opinion")?;

    // Kroki, po których `s_review` biegnie. Krawędź w którykolwiek z nich zawraca bieg i oddaje
    // recenzentowi władzę, której jego schemat odpowiedzi nie ma.
    let earlier: Vec<&str> = ids
        .iter()
        .copied()
        .enumerate()
        .filter(|&(step, _)| reach[step][review])
        .map(|(_, name)| name)
        .collect();

    for link in &workflow.links {
        if link.from != REVIEW {
            continue;
        }
        assert!(
            !earlier.contains(&link.to.as_str()),
            "the second opinion points back at {}, and {earlier:?} all run before it. An arrow \
             that goes back means 'go around again until there are no concerns' — which is the \
             reviewer approving and blocking, in a schema that has no way to say either",
            link.to
        );
    }
    Ok(())
}
