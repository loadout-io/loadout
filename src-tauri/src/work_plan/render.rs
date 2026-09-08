use std::fmt::Write as _;

use super::{AcceptanceCriterion, Origin, PlanDocument, Status};

/// Pierwsza wersja deterministycznego tekstu planu.
pub const RENDERER: u32 = 1;

#[must_use]
pub fn render_plan(document: &PlanDocument) -> String {
    let mut text = String::new();
    heading_text(&mut text, "Goal", &document.goal);
    heading_list(&mut text, "In scope", &document.in_scope);
    heading_list(&mut text, "Not in scope", &document.out_of_scope);

    text.push_str("## Requirements\n\n");
    let mut agreed = 0_usize;
    for requirement in &document.requirements {
        if requirement.origin == Origin::Human && requirement.status == Status::Agreed {
            agreed = agreed.saturating_add(1);
            let _ = writeln!(text, "- {}: {}", requirement.id, requirement.text);
            render_acceptance(&mut text, &requirement.acceptance, "  ");
        }
    }
    if agreed == 0 {
        text.push_str("_None._\n");
    }
    text.push('\n');

    text.push_str("## Acceptance\n\n");
    if document.acceptance.is_empty() {
        text.push_str("_None._\n");
    } else {
        render_acceptance(&mut text, &document.acceptance, "");
    }
    text.push('\n');

    heading_list(&mut text, "Decisions and limits", &document.decisions);

    text.push_str("## Details\n\n");
    if document.sections.is_empty() {
        text.push_str("_None._\n\n");
    } else {
        for (key, content) in &document.sections {
            let _ = writeln!(text, "### {}\n", key.label());
            text.push_str(content);
            text.push_str("\n\n");
        }
    }

    text.push_str("## Proposals\n\n");
    let mut proposed = 0_usize;
    for requirement in &document.requirements {
        if requirement.origin != Origin::Human || requirement.status != Status::Agreed {
            proposed = proposed.saturating_add(1);
            let _ = writeln!(
                text,
                "- [{}] Model proposal — not agreed by a person: {}",
                requirement.id, requirement.text
            );
            render_acceptance(&mut text, &requirement.acceptance, "  ");
        }
    }
    for proposal in &document.proposals {
        proposed = proposed.saturating_add(1);
        let _ = writeln!(text, "- {proposal}");
    }
    if proposed == 0 {
        text.push_str("_None._\n");
    }
    text.push('\n');

    heading_list(&mut text, "Assumptions", &document.assumptions);
    heading_list(&mut text, "Questions", &document.questions);
    heading_list(&mut text, "Unresolved conflicts", &document.conflicts);

    text.push_str("## Sources\n\n");
    if document.sources.is_empty() {
        text.push_str("_None._\n");
    } else {
        for source in &document.sources {
            let _ = writeln!(
                text,
                "- {} — {} (version: {})",
                source.name, source.locator, source.version
            );
        }
    }
    text
}

fn heading_text(out: &mut String, heading: &str, value: &str) {
    let _ = writeln!(out, "## {heading}\n");
    if value.is_empty() {
        out.push_str("_None._\n\n");
    } else {
        out.push_str(value);
        out.push_str("\n\n");
    }
}

fn heading_list(out: &mut String, heading: &str, values: &[String]) {
    let _ = writeln!(out, "## {heading}\n");
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
