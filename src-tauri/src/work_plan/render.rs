use std::fmt::Write as _;

use super::{AcceptanceCriterion, Origin, PlanDocument, PlanVersion, SectionKey, Status};

/// Pierwsza wersja deterministycznego tekstu planu.
pub const RENDERER: u32 = 1;

#[must_use]
pub fn render_core(version: &PlanVersion) -> String {
    let mut text = String::new();
    let _ = writeln!(
        text,
        "### Plan identity\n\nVersion: {}\nVersion ID: {}\n",
        version.version, version.version_id
    );
    render_goal_and_scope(&mut text, &version.document, "###");
    render_requirements(&mut text, &version.document, "###");
    render_acceptance_section(&mut text, &version.document, "###");
    heading_list(
        &mut text,
        "###",
        "Decisions and limits",
        &version.document.decisions,
    );
    heading_list(
        &mut text,
        "###",
        "Unresolved conflicts",
        &version.document.conflicts,
    );
    text
}

#[must_use]
pub fn render_details(document: &PlanDocument, focus: &[SectionKey]) -> String {
    let mut text = String::new();
    render_detail_sections(&mut text, document, focus, "###", "####");
    render_proposals(&mut text, document, "###");
    heading_list(&mut text, "###", "Assumptions", &document.assumptions);
    heading_list(&mut text, "###", "Questions", &document.questions);
    render_sources(&mut text, document, "###");
    text
}

#[must_use]
pub fn detail_index(document: &PlanDocument) -> Vec<(String, String)> {
    let mut index = document
        .sections
        .iter()
        .map(|(key, content)| {
            (
                key.label().to_owned(),
                format!(
                    "{} detail section; scope: {} bytes.",
                    key.label(),
                    content.len()
                ),
            )
        })
        .collect::<Vec<_>>();
    if document.requirements.iter().any(|requirement| {
        requirement.origin != Origin::Human || requirement.status != Status::Agreed
    }) || !document.proposals.is_empty()
    {
        index.push(index_entry(
            "Proposals",
            "model proposals and requirements not agreed by a person",
            rendered_proposals(document),
        ));
    }
    push_list_index(
        &mut index,
        "Assumptions",
        "assumptions recorded with this version",
        &document.assumptions,
    );
    push_list_index(
        &mut index,
        "Questions",
        "questions left with this version",
        &document.questions,
    );
    if !document.sources.is_empty() {
        let bytes = document
            .sources
            .iter()
            .map(|source| source.name.len() + source.locator.len() + source.version.len())
            .sum();
        index.push(index_entry(
            "Sources",
            "source references named by this version",
            bytes,
        ));
    }
    index
}

#[must_use]
pub fn render_plan(document: &PlanDocument) -> String {
    let mut text = String::new();
    render_goal_and_scope(&mut text, document, "##");
    render_requirements(&mut text, document, "##");
    render_acceptance_section(&mut text, document, "##");
    heading_list(&mut text, "##", "Decisions and limits", &document.decisions);
    render_detail_sections(&mut text, document, &[], "##", "###");
    render_proposals(&mut text, document, "##");
    heading_list(&mut text, "##", "Assumptions", &document.assumptions);
    heading_list(&mut text, "##", "Questions", &document.questions);
    heading_list(&mut text, "##", "Unresolved conflicts", &document.conflicts);
    render_sources(&mut text, document, "##");
    text
}

fn render_goal_and_scope(out: &mut String, document: &PlanDocument, level: &str) {
    heading_text(out, level, "Goal", &document.goal);
    heading_list(out, level, "In scope", &document.in_scope);
    heading_list(out, level, "Not in scope", &document.out_of_scope);
}

fn render_requirements(out: &mut String, document: &PlanDocument, level: &str) {
    let _ = writeln!(out, "{level} Requirements\n");
    let mut agreed = 0_usize;
    for requirement in &document.requirements {
        if requirement.origin == Origin::Human && requirement.status == Status::Agreed {
            agreed = agreed.saturating_add(1);
            let _ = writeln!(out, "- {}: {}", requirement.id, requirement.text);
            render_acceptance(out, &requirement.acceptance, "  ");
        }
    }
    if agreed == 0 {
        out.push_str("_None._\n");
    }
    out.push('\n');
}

fn render_acceptance_section(out: &mut String, document: &PlanDocument, level: &str) {
    let _ = writeln!(out, "{level} Acceptance\n");
    if document.acceptance.is_empty() {
        out.push_str("_None._\n");
    } else {
        render_acceptance(out, &document.acceptance, "");
    }
    out.push('\n');
}

fn render_detail_sections(
    out: &mut String,
    document: &PlanDocument,
    focus: &[SectionKey],
    level: &str,
    item_level: &str,
) {
    let _ = writeln!(out, "{level} Details\n");
    let selected = document
        .sections
        .iter()
        .filter(|(key, _)| focus.is_empty() || focus.contains(key))
        .collect::<Vec<_>>();
    if selected.is_empty() {
        out.push_str("_None._\n\n");
    } else {
        for (key, content) in selected {
            let _ = writeln!(out, "{item_level} {}\n", key.label());
            out.push_str(content);
            out.push_str("\n\n");
        }
    }
}

fn render_proposals(out: &mut String, document: &PlanDocument, level: &str) {
    let _ = writeln!(out, "{level} Proposals\n");
    let mut proposed = 0_usize;
    for requirement in &document.requirements {
        if requirement.origin != Origin::Human || requirement.status != Status::Agreed {
            proposed = proposed.saturating_add(1);
            let _ = writeln!(
                out,
                "- [{}] Model proposal — not agreed by a person: {}",
                requirement.id, requirement.text
            );
            render_acceptance(out, &requirement.acceptance, "  ");
        }
    }
    for proposal in &document.proposals {
        proposed = proposed.saturating_add(1);
        let _ = writeln!(out, "- {proposal}");
    }
    if proposed == 0 {
        out.push_str("_None._\n");
    }
    out.push('\n');
}

fn rendered_proposals(document: &PlanDocument) -> usize {
    let mut rendered = String::new();
    render_proposals(&mut rendered, document, "###");
    rendered.len()
}

fn push_list_index(
    index: &mut Vec<(String, String)>,
    id: &str,
    description: &str,
    values: &[String],
) {
    if values.is_empty() {
        return;
    }
    index.push(index_entry(
        id,
        description,
        values.iter().map(String::len).sum(),
    ));
}

fn index_entry(id: &str, description: &str, bytes: usize) -> (String, String) {
    (
        id.to_owned(),
        format!("{description}; scope: {bytes} bytes."),
    )
}

fn render_sources(out: &mut String, document: &PlanDocument, level: &str) {
    let _ = writeln!(out, "{level} Sources\n");
    if document.sources.is_empty() {
        out.push_str("_None._\n");
    } else {
        for source in &document.sources {
            let _ = writeln!(
                out,
                "- {} — {} (version: {})",
                source.name, source.locator, source.version
            );
        }
    }
}

fn heading_text(out: &mut String, level: &str, heading: &str, value: &str) {
    let _ = writeln!(out, "{level} {heading}\n");
    if value.is_empty() {
        out.push_str("_None._\n\n");
    } else {
        out.push_str(value);
        out.push_str("\n\n");
    }
}

fn heading_list(out: &mut String, level: &str, heading: &str, values: &[String]) {
    let _ = writeln!(out, "{level} {heading}\n");
    if values.is_empty() {
        out.push_str("_None._\n\n");
        return;
    }
    for value in values {
        let _ = writeln!(out, "- {value}");
    }
    out.push('\n');
}

fn render_acceptance(out: &mut String, criteria: &[AcceptanceCriterion], indent: &str) {
    for criterion in criteria {
        let _ = writeln!(out, "{indent}  - Acceptance: {}", criterion.text);
        let _ = writeln!(out, "{indent}    Check with: {}", criterion.verification);
    }
}
