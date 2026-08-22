//! Złożenie inventory w raport zgodności i natywny graf.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{Map, Value};
use uuid::Uuid;

use crate::library::agents::Agent;
use crate::workflow::{
    AgentStep, CheckStep, CheckpointStep, Folder, Handover, Link, PlainNotes, Point, Skills, Step,
    WorkflowFile,
};

pub use crate::workflow::{CheckOutcome, Condition, ConditionalLink, RouteEvidence as Evidence};

use super::adapters::{adapt, check_command, knows_ship_ui};
use super::discover::{Inspection, scan};
use super::{
    ADAPTER_VERSION, CompatibilityReport, ImportPreview, ItemKind, MigrationDraft, Result,
};

/// Pełny Scan: odczyt i translacja w jednym backendowym przebiegu, zanim dane trafią do okna.
pub fn preview(root: &Path) -> Result<ImportPreview> {
    let inspection = scan(root)?;
    Ok(from_inspection(inspection))
}

fn from_inspection(inspection: Inspection) -> ImportPreview {
    let adapted = adapt(&inspection);
    let source_hashes = inspection
        .snapshot
        .items
        .iter()
        .map(|item| (item.path.clone(), item.hash.clone()))
        .collect();
    let workflows = imported_workflows(&inspection, &adapted.agents);
    let draft = MigrationDraft {
        root: inspection.snapshot.root.clone(),
        source_hashes,
        agents: adapted.agents,
        skills: adapted.skills,
        connections: adapted.connections,
        workflows,
        report: CompatibilityReport {
            mappings: adapted.mappings,
        },
    };
    ImportPreview {
        snapshot: inspection.snapshot,
        draft,
    }
}

fn imported_workflows(inspection: &Inspection, agents: &[Agent]) -> Vec<WorkflowFile> {
    let mut out = Vec::new();
    for file in &inspection.files {
        if file.item.kind == ItemKind::Workflow
            && knows_ship_ui(&file.content)
            && let Some(workflow) = ship_ui(agents, &file.content)
        {
            out.push(workflow);
        }
    }
    out
}

fn ship_ui(agents: &[Agent], source: &str) -> Option<WorkflowFile> {
    let frontend = find_agent(agents, "frontend-dev")?;
    let design = find_agent(agents, "design-qa")?;
    let review = find_agent(agents, "code-reviewer")?;
    let command = check_command(source)?;
    let proof = if source.contains("(\\d+) passed") {
        "(\\d+) passed".to_owned()
    } else {
        // Import nazywa tę adaptację w raporcie. Sam kod wyjścia nigdy nie jest dowodem;
        // najwęższy wspólny licznik akceptowany przez harness jest jawny tutaj.
        "(\\d+) passed".to_owned()
    };
    let review_template = review_subworkflow(design, review);
    let expanded_review = flatten(&review_template, "ship-ui");
    let expanded_review_id = expanded_review.id.clone();
    let mut steps = vec![
        Step::Agent({
            let mut plan = agent_step(
                "ship-ui.plan",
                "Plan the change",
                frontend,
                "Turn the person's request into an implementation plan and hand it to the approval step.",
                point(0.0, 0.0),
            );
            plan.folder = Folder::Project;
            plan
        }),
        Step::Checkpoint(CheckpointStep {
            id: "ship-ui.approve-plan".to_owned(),
            name: "Approve the plan".to_owned(),
            question: Some("Does the implementation plan match the task?".to_owned()),
            at: point(288.0, 0.0),
            extra: Map::new(),
        }),
        Step::Agent(agent_step(
            "ship-ui.implement",
            "Build the UI",
            frontend,
            "Implement the approved UI plan. Use the imported project skills and write a handoff for the checks.",
            point(576.0, 0.0),
        )),
        Step::Check(CheckStep {
            id: "ship-ui.check".to_owned(),
            name: "Run the project checks".to_owned(),
            command: command.clone(),
            proof: proof.clone(),
            folder: Folder::SameCopy,
            at: point(864.0, 0.0),
            extra: Map::new(),
        }),
    ];
    steps.extend(expanded_review.steps);
    steps.push(Step::Check(CheckStep {
        id: "ship-ui.final-check".to_owned(),
        name: "Run the final checks".to_owned(),
        command,
        proof,
        folder: Folder::SameCopy,
        at: point(1440.0, 0.0),
        extra: Map::new(),
    }));
    let links = vec![
        link("ship-ui.plan", "ship-ui.approve-plan"),
        // Implementacja czeka także bezpośrednio na handoff planisty; sam Checkpoint niesie
        // decyzję człowieka, ale nie kopiuje cudzej odpowiedzi do własnego przekazania.
        link("ship-ui.plan", "ship-ui.implement"),
        link("ship-ui.approve-plan", "ship-ui.implement"),
        link("ship-ui.implement", "ship-ui.check"),
        link("ship-ui.check", "ship-ui.design-qa"),
        link("ship-ui.check", "ship-ui.code-review"),
        link("ship-ui.design-qa", "ship-ui.final-check"),
        link("ship-ui.code-review", "ship-ui.final-check"),
        Link {
            from: "ship-ui.code-review".to_owned(),
            to: "ship-ui.implement".to_owned(),
            max_turns: Some(1),
        },
    ];
    let mut extra = Map::new();
    extra.insert(
        "importedBy".to_owned(),
        Value::String(format!("loadout-import-v{ADAPTER_VERSION}")),
    );
    extra.insert(
        "expandedSubworkflows".to_owned(),
        Value::Array(vec![Value::String(expanded_review_id)]),
    );
    Some(WorkflowFile {
        format: 1,
        id: Uuid::now_v7().to_string(),
        name: "Ship UI".to_owned(),
        description: Some("Imported from the project's coordinating skill.".to_owned()),
        steps,
        links,
        extra,
    })
}

fn review_subworkflow(design: &Agent, review: &Agent) -> WorkflowFile {
    WorkflowFile {
        format: 1,
        id: "parallel-review".to_owned(),
        name: "Parallel review".to_owned(),
        description: None,
        steps: vec![
            Step::Agent(agent_step(
                "design-qa",
                "Review the UI",
                design,
                "Inspect the implementation and the check handoff. Report concrete visual concerns only.",
                point(1152.0, -144.0),
            )),
            Step::Agent(agent_step(
                "code-review",
                "Review the code",
                review,
                "Review the implementation and prior handoffs. End with outcome: pass or outcome: fail.",
                point(1152.0, 144.0),
            )),
        ],
        links: Vec::new(),
        extra: Map::new(),
    }
}

fn find_agent<'a>(agents: &'a [Agent], wanted: &str) -> Option<&'a Agent> {
    agents.iter().find(|agent| {
        agent.name.eq_ignore_ascii_case(wanted)
            || agent
                .name
                .to_ascii_lowercase()
                .replace(' ', "-")
                .contains(wanted)
    })
}

fn agent_step(id: &str, name: &str, agent: &Agent, instructions: &str, at: Point) -> AgentStep {
    AgentStep {
        id: id.to_owned(),
        name: name.to_owned(),
        agent: agent.id.to_string(),
        overrides: Map::new(),
        vendor_options: BTreeMap::new(),
        copies: 1,
        instructions: instructions.to_owned(),
        skills: Skills::default(),
        folder: Folder::SameCopy,
        handover: Handover::Plain(PlainNotes::Notes),
        at,
        extra: Map::new(),
    }
}

fn point(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn link(from: &str, to: &str) -> Link {
    Link {
        from: from.to_owned(),
        to: to.to_owned(),
        max_turns: None,
    }
}

/// Rozwija szablon bez zmiany rodzajów kroków. Zagnieżdżenie jest faktem importera, nie silnika.
#[must_use]
pub fn flatten(template: &WorkflowFile, namespace: &str) -> WorkflowFile {
    let ids: BTreeSet<String> = template
        .steps
        .iter()
        .map(|step| step_id(step).to_owned())
        .collect();
    let rename = |id: &str| {
        if ids.contains(id) {
            format!("{namespace}.{id}")
        } else {
            id.to_owned()
        }
    };
    let steps = template
        .steps
        .iter()
        .cloned()
        .map(|mut step| {
            match &mut step {
                Step::Agent(step) => step.id = rename(&step.id),
                Step::Checkpoint(step) => step.id = rename(&step.id),
                Step::Check(step) => step.id = rename(&step.id),
            }
            step
        })
        .collect();
    let links = template
        .links
        .iter()
        .map(|link| Link {
            from: rename(&link.from),
            to: rename(&link.to),
            max_turns: link.max_turns,
        })
        .collect();
    let mut extra = template.extra.clone();
    extra.insert(
        "expandedFrom".to_owned(),
        Value::String(template.id.clone()),
    );
    WorkflowFile {
        format: template.format,
        id: format!("{namespace}.{}", template.id),
        name: template.name.clone(),
        description: template.description.clone(),
        steps,
        links,
        extra,
    }
}

fn step_id(step: &Step) -> &str {
    match step {
        Step::Agent(step) => &step.id,
        Step::Checkpoint(step) => &step.id,
        Step::Check(step) => &step.id,
    }
}
