//! Złożenie inventory w raport zgodności i natywny graf.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{Map, Value};
use uuid::Uuid;

use crate::library::agents::{Agent, Color, SCHEMA, Thinking, Tools, Vendor, VendorOptions};
use crate::workflow::{
    AgentStep, CheckStep, CheckpointStep, Folder, Handover, Link, PlainNotes, Point, Skills, Step,
    WorkflowFile,
};

pub use crate::workflow::{CheckOutcome, Condition, ConditionalLink, RouteEvidence as Evidence};

use super::adapters::{adapt, check_command, knows_ship_ui};
use super::discover::{Inspection, scan};
use super::{
    ADAPTER_VERSION, AnalyzedFolder, AnalyzedStep, AnalyzedWorkflow, Compatibility,
    CompatibilityReport, ImportError, ImportPreview, ItemKind, MigrationDraft, Result,
    SemanticAnalysis,
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
        analysis: None,
    }
}

/// Dokłada do deterministycznej migawki wyłącznie te propozycje modelu, które przechodzą
/// zamknięty schemat i natywny walidator workflow. Model nie rozstrzyga własnej poprawności.
pub fn with_analysis(
    mut preview: ImportPreview,
    analysis: SemanticAnalysis,
) -> Result<ImportPreview> {
    if analysis.source_hashes != preview.draft.source_hashes {
        return Err(ImportError::Changed);
    }
    let items: BTreeMap<_, _> = preview
        .snapshot
        .items
        .iter()
        .cloned()
        .map(|item| (item.id.clone(), item))
        .collect();
    let mut covered = BTreeSet::new();
    apply_analyzed_agents(&mut preview, &analysis, &items, &mut covered)?;
    apply_analyzed_workflows(&mut preview, &analysis, &items, &mut covered)?;

    for mapping in &mut preview.draft.report.mappings {
        if covered.contains(mapping.item_id.as_str()) && mapping.compatibility.blocks() {
            mapping.compatibility = Compatibility::Adjusted;
            "An agent converted this project behavior into the native draft shown below."
                .clone_into(&mut mapping.message);
        }
    }
    preview
        .draft
        .agents
        .sort_by(|left, right| left.name.cmp(&right.name));
    preview
        .draft
        .workflows
        .sort_by(|left, right| left.name.cmp(&right.name));
    preview.analysis = Some(analysis);
    Ok(preview)
}

fn apply_analyzed_agents(
    preview: &mut ImportPreview,
    analysis: &SemanticAnalysis,
    items: &BTreeMap<String, super::SourceItem>,
    covered: &mut BTreeSet<String>,
) -> Result<()> {
    for proposed in &analysis.agents {
        validate_text("agent name", &proposed.name)?;
        validate_text("agent purpose", &proposed.summary)?;
        validate_text("agent instructions", &proposed.instructions)?;
        cover_sources(
            items,
            &proposed.source_items,
            &[
                ItemKind::Agent,
                ItemKind::Memory,
                ItemKind::Rule,
                ItemKind::Unknown,
            ],
            covered,
        )?;
        let unknown_skill = proposed.skills.iter().find(|name| {
            !preview
                .draft
                .skills
                .iter()
                .any(|skill| skill.name.eq_ignore_ascii_case(name))
        });
        if let Some(name) = unknown_skill {
            return Err(ImportError::Analyze(format!(
                "The analyzed agent {} refers to skill {name}, which was not found by Scan.",
                proposed.name
            )));
        }
        if let Some(existing) = preview
            .draft
            .agents
            .iter_mut()
            .find(|agent| agent.name.eq_ignore_ascii_case(&proposed.name))
        {
            if !existing.instructions.contains(proposed.instructions.trim()) {
                existing.instructions.push_str("\n\n");
                existing.instructions.push_str(proposed.instructions.trim());
            }
            merge_names(&mut existing.skills, &proposed.skills);
        } else {
            let colour = colour(preview.draft.agents.len());
            preview.draft.agents.push(Agent {
                schema: SCHEMA,
                id: Uuid::now_v7(),
                name: proposed.name.trim().to_owned(),
                summary: proposed.summary.trim().to_owned(),
                color: colour,
                instructions: proposed.instructions.trim().to_owned(),
                runs_with: analysis.vendor,
                model: default_model(analysis.vendor).to_owned(),
                thinking: Thinking::Balanced,
                file_access: proposed.file_access,
                give_up_after_minutes: 20,
                tools: Tools::Everything,
                skills: proposed.skills.clone(),
                connections: Vec::new(),
                write_results_to: String::new(),
                vendor_options: VendorOptions::new(),
            });
        }
    }
    Ok(())
}

fn apply_analyzed_workflows(
    preview: &mut ImportPreview,
    analysis: &SemanticAnalysis,
    items: &BTreeMap<String, super::SourceItem>,
    covered: &mut BTreeSet<String>,
) -> Result<()> {
    for proposed in &analysis.workflows {
        validate_text("workflow name", &proposed.name)?;
        cover_sources(
            items,
            &proposed.source_items,
            &[
                ItemKind::Workflow,
                ItemKind::Hook,
                ItemKind::Rule,
                ItemKind::Unknown,
            ],
            covered,
        )?;
        let workflow = analyzed_workflow(proposed, &preview.draft.agents, &preview.draft.root)?;
        let problems: Vec<_> = crate::workflow::check::check_to_run(&workflow)
            .into_iter()
            .filter(|note| note.level == crate::workflow::check::Level::Problem)
            .collect();
        if let Some(problem) = problems.first() {
            return Err(ImportError::Analyze(format!(
                "The analyzed workflow {} is not runnable: {}",
                proposed.name, problem.message
            )));
        }
        if preview
            .draft
            .workflows
            .iter()
            .any(|existing| existing.name.eq_ignore_ascii_case(&workflow.name))
        {
            return Err(ImportError::Analyze(format!(
                "The analysis produced another workflow named {}.",
                workflow.name
            )));
        }
        preview.draft.workflows.push(workflow);
    }
    Ok(())
}

fn validate_text(label: &str, text: &str) -> Result<()> {
    if text.trim().is_empty() || text.len() > 32_768 {
        return Err(ImportError::Analyze(format!(
            "The analysis returned an invalid {label}."
        )));
    }
    Ok(())
}

fn cover_sources(
    items: &BTreeMap<String, super::SourceItem>,
    source_items: &[String],
    allowed: &[ItemKind],
    covered: &mut BTreeSet<String>,
) -> Result<()> {
    if source_items.is_empty() {
        return Err(ImportError::Analyze(
            "Every analyzed item must name the Scan items it reproduces.".to_owned(),
        ));
    }
    for id in source_items {
        let item = items.get(id.as_str()).ok_or_else(|| {
            ImportError::Analyze(
                "The analysis refers to a setup item that is not in this Scan.".to_owned(),
            )
        })?;
        if !allowed.contains(&item.kind) {
            return Err(ImportError::Analyze(format!(
                "{} cannot be reproduced by this kind of analyzed item.",
                item.path.display()
            )));
        }
        if !covered.insert(id.clone()) {
            return Err(ImportError::Analyze(format!(
                "{} was associated with more than one analyzed item.",
                item.path.display()
            )));
        }
    }
    Ok(())
}

fn analyzed_workflow(
    proposed: &AnalyzedWorkflow,
    agents: &[Agent],
    root: &Path,
) -> Result<WorkflowFile> {
    if proposed.steps.is_empty() {
        return Err(ImportError::Analyze(format!(
            "The analyzed workflow {} has no steps.",
            proposed.name
        )));
    }
    let mut steps = Vec::with_capacity(proposed.steps.len());
    for (index, proposed_step) in proposed.steps.iter().enumerate() {
        steps.push(analyzed_step(proposed_step, index, agents, root)?);
    }
    let links = proposed
        .links
        .iter()
        .map(|link| Link {
            from: link.from.clone(),
            to: link.to.clone(),
            max_turns: link.max_turns,
        })
        .collect();
    let mut extra = Map::new();
    extra.insert(
        "importedBy".to_owned(),
        Value::String(format!("loadout-agent-analysis-v{ADAPTER_VERSION}")),
    );
    extra.insert(
        "sourceItems".to_owned(),
        Value::Array(
            proposed
                .source_items
                .iter()
                .cloned()
                .map(Value::String)
                .collect(),
        ),
    );
    Ok(WorkflowFile {
        format: 1,
        id: Uuid::now_v7().to_string(),
        name: proposed.name.trim().to_owned(),
        description: proposed.description.clone(),
        steps,
        links,
        extra,
    })
}

fn analyzed_step(
    proposed: &AnalyzedStep,
    index: usize,
    agents: &[Agent],
    root: &Path,
) -> Result<Step> {
    let column = u32::try_from(index).map_err(|_| {
        ImportError::Analyze("The analyzed workflow has too many steps to display.".to_owned())
    })?;
    let at = point(f64::from(column) * 288.0, 0.0);
    match proposed {
        AnalyzedStep::Agent {
            id,
            name,
            agent,
            instructions,
            skills,
            folder,
        } => {
            let saved = agents
                .iter()
                .find(|saved| saved.name.eq_ignore_ascii_case(agent))
                .ok_or_else(|| {
                    ImportError::Analyze(format!(
                        "The analyzed step {name} refers to agent {agent}, which is not in the draft."
                    ))
                })?;
            let unknown_skill = skills.iter().find(|skill| {
                !saved
                    .skills
                    .iter()
                    .any(|known| known.eq_ignore_ascii_case(skill))
            });
            if let Some(skill) = unknown_skill {
                return Err(ImportError::Analyze(format!(
                    "The analyzed step {name} asks agent {agent} for skill {skill}, which that agent does not have."
                )));
            }
            Ok(Step::Agent(AgentStep {
                id: id.clone(),
                name: name.clone(),
                agent: saved.id.to_string(),
                overrides: Map::new(),
                vendor_options: BTreeMap::new(),
                copies: 1,
                instructions: instructions.clone(),
                skills: if skills.is_empty() {
                    Skills::default()
                } else {
                    Skills::Only(skills.clone())
                },
                folder: folder.into(),
                handover: Handover::Plain(PlainNotes::Notes),
                at,
                extra: Map::new(),
            }))
        }
        AnalyzedStep::Check {
            id,
            name,
            command,
            proof,
            evidence,
            folder,
        } => {
            if !super::discover::command_has_evidence(root, evidence, command) {
                return Err(ImportError::Analyze(format!(
                    "The analyzed check {name} does not quote a command from {}.",
                    evidence.display()
                )));
            }
            let mut extra = Map::new();
            extra.insert(
                "sourceFile".to_owned(),
                Value::String(evidence.to_string_lossy().into_owned()),
            );
            Ok(Step::Check(CheckStep {
                id: id.clone(),
                name: name.clone(),
                command: command.clone(),
                proof: proof.clone(),
                folder: folder.into(),
                at,
                extra,
            }))
        }
        AnalyzedStep::Checkpoint { id, name, question } => Ok(Step::Checkpoint(CheckpointStep {
            id: id.clone(),
            name: name.clone(),
            question: Some(question.clone()),
            at,
            extra: Map::new(),
        })),
    }
}

impl From<&AnalyzedFolder> for Folder {
    fn from(value: &AnalyzedFolder) -> Self {
        match value {
            AnalyzedFolder::Project => Self::Project,
            AnalyzedFolder::FreshCopy => Self::FreshCopy,
            AnalyzedFolder::SameCopy => Self::SameCopy,
        }
    }
}

fn merge_names(existing: &mut Vec<String>, additional: &[String]) {
    for name in additional {
        if !existing.iter().any(|one| one.eq_ignore_ascii_case(name)) {
            existing.push(name.clone());
        }
    }
    existing.sort();
}

fn colour(index: usize) -> Color {
    match index % 5 {
        0 => Color::Slate,
        1 => Color::Plum,
        2 => Color::Clay,
        3 => Color::Moss,
        _ => Color::Rose,
    }
}

fn default_model(vendor: Vendor) -> &'static str {
    match vendor {
        Vendor::ClaudeCode => "sonnet",
        Vendor::Codex => "gpt-5.6-sol",
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
