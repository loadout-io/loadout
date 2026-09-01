use std::path::Path;

use loadout_lib::import::{ImportSourceRole, ImportStatus, ItemKind};
use loadout_lib::workflow::Step;

const SOURCE_PATH: &str = ".claude/skills/ship-ui/SKILL.md";
const COMMAND: &str = "./verify.sh full";
const PROOF: &str = r"(\d+) passed";

#[test]
fn only_complete_native_workflow_is_runnable() -> Result<(), Box<dyn std::error::Error>> {
    let repo = tempfile::tempdir()?;
    write_legacy_fixture(repo.path(), Some(PROOF))?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;
    assert!(preview.draft.runnable());
    let workflow = preview
        .draft
        .workflows
        .iter()
        .find(|workflow| workflow.name == "Ship UI")
        .ok_or("the complete legacy source did not produce Ship UI")?;
    assert_eq!(workflow.steps.len(), 7);
    let checks: Vec<_> = workflow
        .steps
        .iter()
        .filter_map(|step| match step {
            Step::Check(check) => Some(check),
            Step::Agent(_) | Step::Checkpoint(_) | Step::Serve(_) => None,
        })
        .collect();
    assert_eq!(checks.len(), 2);
    assert!(
        checks
            .iter()
            .all(|check| check.command == COMMAND && check.proof == PROOF),
        "both legacy checks must preserve the literal command and proof from {SOURCE_PATH}"
    );
    assert!(
        workflow
            .links
            .iter()
            .any(|link| link.from == "ship-ui.plan" && link.to == "ship-ui.approve-plan")
    );
    assert!(
        workflow
            .links
            .iter()
            .any(|link| link.from == "ship-ui.plan" && link.to == "ship-ui.implement"),
        "the implementation must receive the planner handoff after approval"
    );
    let parallel: Vec<&str> = workflow
        .links
        .iter()
        .filter(|link| link.from == "ship-ui.check")
        .map(|link| link.to.as_str())
        .collect();
    assert_eq!(parallel, vec!["ship-ui.design-qa", "ship-ui.code-review"]);
    assert!(
        workflow
            .links
            .iter()
            .any(|link| link.from == "ship-ui.code-review"
                && link.to == "ship-ui.implement"
                && link.max_turns == Some(1))
    );
    assert!(
        workflow
            .links
            .iter()
            .any(|link| link.to == "ship-ui.final-check")
    );
    assert_eq!(
        workflow
            .links
            .iter()
            .filter(|link| link.to == "ship-ui.final-check")
            .count(),
        2,
        "the final check is a fan-in after both independent reviews"
    );
    assert_eq!(
        workflow.extra["expandedSubworkflows"][0],
        "ship-ui.parallel-review"
    );
    let home = tempfile::tempdir()?;
    let receipt = loadout_lib::import::apply::apply(home.path(), &preview.draft)?;
    assert_eq!(receipt.workflow_hashes.len(), 1);
    assert!(
        receipt
            .workflow_hashes
            .values()
            .all(|hash| hash.len() == 16)
    );
    Ok(())
}

#[test]
fn legacy_ship_ui_without_literal_proof_stays_named_and_unresolved()
-> Result<(), Box<dyn std::error::Error>> {
    let repo = tempfile::tempdir()?;
    write_legacy_fixture(repo.path(), None)?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;
    assert!(
        preview
            .draft
            .workflows
            .iter()
            .all(|workflow| workflow.name != "Ship UI"),
        "the importer must not invent a passing proof for a legacy coordinating skill"
    );
    let item = preview
        .draft
        .items
        .iter()
        .find(|item| {
            item.kind == ItemKind::Workflow
                && item.sources.iter().any(|source| {
                    source.path == Path::new(SOURCE_PATH)
                        && source.role == ImportSourceRole::Definition
                })
        })
        .ok_or("the unresolved legacy workflow disappeared from the import plan")?;
    assert_eq!(item.status, ImportStatus::NeedsChoice);
    assert!(item.target.is_none());
    let message = item.status_message.to_ascii_lowercase();
    assert!(
        message.contains("ship ui") && message.contains("proof"),
        "the choice must name Ship UI and the missing proof. It said: {}",
        item.status_message
    );
    Ok(())
}

fn write_legacy_fixture(
    root: &Path,
    proof: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(root.join(".claude/agents"))?;
    std::fs::create_dir_all(root.join(".claude/skills/ship-ui"))?;
    for role in ["frontend-dev", "design-qa", "code-reviewer"] {
        std::fs::write(
            root.join(format!(".claude/agents/{role}.md")),
            format!("---\nname: {role}\ndescription: Project role\n---\nDo the {role} work."),
        )?;
    }
    let proof = proof
        .map(|proof| format!("\nproof: `{proof}`"))
        .unwrap_or_default();
    std::fs::write(
        root.join(SOURCE_PATH),
        format!(
            "---\nname: ship-ui\ndescription: Coordinates UI delivery\n---\nship-task delegates to frontend-dev, design-qa, and code-reviewer. Run `{COMMAND}` before finishing.{proof}\n"
        ),
    )?;
    Ok(())
}
