//! AC-5 dla T-23: graf jest ściśle sekwencyjny — żadna para kroków nie może być gotowa naraz.
//!
//! To jedyne kryterium tego zadania, które mówi coś o SEMANTYCE biegu, a nie o kształcie pliku,
//! i dlatego liczy się je z krawędzi. Sprawdzenie, że każdy krok ma `copies == 1`, przechodzi na
//! grafie z dwiema równoległymi gałęziami po jednej kopii — a to jest właśnie ten kształt, w którym
//! semantyka harnessu przestaje działać: dwa kroki gotowe naraz to dwie gałęzie wchodzące na trunk
//! równocześnie, a harness wchodzi po jednej i przepuszcza pełną bramkę po KAŻDEJ `[06 §10.7]`.
//!
//! „Najszerszy antyłańcuch" to nie ozdoba z teorii porządków, tylko dokładnie to pytanie: ile
//! kroków może mieć jednocześnie stopień wejściowy zero po dowolnym prefiksie wykonania. Dla
//! łańcucha odpowiedź brzmi 1, i tylko dla łańcucha. Liczymy go wyczerpująco po podzbiorach —
//! przy pięciu krokach to 32 sprawdzenia, więc dokładna odpowiedź jest tańsza niż przybliżona.
//!
//! Druga asercja zostaje mimo wszystko, bo mierzy inną rzecz: `copies` biegnie RÓWNOLEGLE SAM ZE
//! SOBĄ, bez żadnej strzałki. Krok „One fix round" w trzech kopiach byłby trzema rundami poprawek
//! zapisanymi jako jedna — a runda jest dokładnie jedna i nie da się jej wyrazić pętlą, bo pętli
//! w schemacie nie ma (T3 §1). Jedna runda jest tu jednym literalnym krokiem, w jednej kopii.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use loadout_lib::workflow::file;
use loadout_lib::workflow::{Link, Step, WorkflowFile};

/// Powyżej tylu kroków wyczerpujące liczenie po podzbiorach przestaje być tanie. Graf harnessu ma
/// ich pięć, a kryterium przybija tę liczbę — więc ten sufit nigdy nie powinien zaświecić.
const MOST_STEPS: usize = 16;

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

/// Ile kroków może być gotowych naraz: największy zbiór, w którym żaden krok nie biegnie po żadnym
/// innym. Liczone z KRAWĘDZI — pole `copies` nie ma tu głosu, bo mówi o jednym kroku, a pytanie
/// jest o parach.
fn widest_ready_at_once(reach: &[Vec<bool>]) -> usize {
    let count = reach.len();
    let mut widest = 0;
    for chosen in 1u32..(1u32 << count) {
        let members: Vec<usize> = (0..count)
            .filter(|&step| chosen & (1u32 << step) != 0)
            .collect();
        if members.len() <= widest {
            continue;
        }
        let ordered = members.iter().enumerate().any(|(at, &one)| {
            members
                .iter()
                .skip(at + 1)
                .any(|&other| reach[one][other] || reach[other][one])
        });
        if !ordered {
            widest = members.len();
        }
    }
    widest
}

#[test]
fn no_two_steps_of_the_harness_can_ever_be_ready_at_the_same_time() -> Result<(), Box<dyn Error>> {
    let workflow = load()?;
    let ids: Vec<&str> = workflow.steps.iter().map(id).collect();

    assert!(
        ids.len() <= MOST_STEPS,
        "this check counts the answer exactly, over every subset of steps, and {} steps is past \
         the point where that is cheap",
        ids.len()
    );

    let reach = reachable(&ids, &workflow.links);
    let widest = widest_ready_at_once(&reach);

    assert_eq!(
        widest, 1,
        "{widest} steps of the harness could be waiting to start at the same moment. Landing two \
         branches at once, or checking one while another is still being written, is the shape in \
         which 'run the checks after every branch' stops meaning anything — and the graph, not \
         the copy count, is what decides it"
    );
    Ok(())
}

#[test]
fn one_fix_round_is_one_step_running_once() -> Result<(), Box<dyn Error>> {
    let workflow = load()?;

    for step in &workflow.steps {
        let Step::Agent(agent) = step else {
            continue;
        };
        assert_eq!(
            agent.copies, 1,
            "\"{}\" would run {} sessions side by side, and copies of one step run at the same \
             time by definition — no arrow can separate them. Four tries are counted by the \
             script that starts the run, never by the graph: the schema has no loop and is not \
             getting one",
            agent.name, agent.copies
        );
    }
    Ok(())
}
