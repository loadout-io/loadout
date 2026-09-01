use std::path::Path;

use loadout_lib::import::{ImportSourceRole, ImportStatus, ItemKind};
use loadout_lib::workflow::Step;

const SOURCE_PATH: &str = ".claude/skills/ship-ui/SKILL.md";
const COMMAND: &str = "./verify.sh full";
const PROOF: &str = r"(\d+) passed";
const MIRROR_NAME: &str = "Release Train";
const CLAUDE_MIRROR: &str = ".claude/commands/release.md";
const RULESYNC_MIRROR: &str = ".rulesync/commands/release.md";

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

#[test]
fn legacy_ship_ui_never_executes_a_forbidden_prose_command()
-> Result<(), Box<dyn std::error::Error>> {
    let repo = tempfile::tempdir()?;
    write_legacy_fixture(repo.path(), Some(PROOF))?;
    std::fs::write(
        repo.path().join(SOURCE_PATH),
        format!(
            "---\nname: ship-ui\ndescription: Coordinates UI delivery\n---\nship-task delegates to frontend-dev, design-qa, and code-reviewer. Never rerun `{COMMAND}`.\nproof: `{PROOF}`\n"
        ),
    )?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;
    assert!(
        preview
            .draft
            .workflows
            .iter()
            .all(|workflow| workflow.name != "Ship UI"),
        "an explicitly forbidden prose command must never become executable legacy checks"
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
        .ok_or("the forbidden legacy workflow disappeared from the import plan")?;
    assert_eq!(item.status, ImportStatus::NeedsChoice);
    assert!(item.target.is_none());
    assert!(
        item.status_message.contains(COMMAND),
        "the unresolved item must name the forbidden command. It said: {}",
        item.status_message
    );
    Ok(())
}

#[test]
fn identical_workflow_mirrors_share_one_native_file() -> Result<(), Box<dyn std::error::Error>> {
    let repo = tempfile::tempdir()?;
    write_routine(repo.path(), CLAUDE_MIRROR, "./verify.sh quick")?;
    write_routine(repo.path(), RULESYNC_MIRROR, "./verify.sh quick")?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;
    let items: Vec<_> = preview
        .draft
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::Workflow)
        .collect();
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|item| item.status == ImportStatus::Ready));
    assert!(
        items.iter().all(|item| {
            item.target.as_deref() == Some(Path::new("workflows/release-train.json"))
        })
    );
    assert_eq!(items[0].generated_hash, items[1].generated_hash);
    assert_eq!(
        preview
            .draft
            .workflows
            .iter()
            .filter(|workflow| workflow.name == MIRROR_NAME)
            .count(),
        1,
        "two source items may explain one target, but Apply must receive one native file"
    );

    let home = tempfile::tempdir()?;
    let receipt = loadout_lib::import::apply::apply(home.path(), &preview.draft)?;
    assert_eq!(receipt.workflow_hashes.len(), 1);
    assert!(home.path().join("workflows/release-train.json").is_file());
    Ok(())
}

#[test]
fn different_workflows_with_one_target_require_a_choice_before_apply()
-> Result<(), Box<dyn std::error::Error>> {
    let repo = tempfile::tempdir()?;
    write_routine(repo.path(), CLAUDE_MIRROR, "./verify.sh quick")?;
    write_routine(repo.path(), RULESYNC_MIRROR, "./verify.sh full")?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;
    assert!(
        preview
            .draft
            .workflows
            .iter()
            .all(|workflow| workflow.name != MIRROR_NAME)
    );
    let items: Vec<_> = preview
        .draft
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::Workflow)
        .collect();
    assert_eq!(items.len(), 2);
    assert!(
        items
            .iter()
            .all(|item| item.status == ImportStatus::NeedsChoice && item.target.is_none())
    );
    assert!(items.iter().all(|item| {
        item.status_message.contains(CLAUDE_MIRROR)
            && item.status_message.contains(RULESYNC_MIRROR)
            && item.status_message.contains("workflows/release-train.json")
    }));
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
            "---\nname: ship-ui\ndescription: Coordinates UI delivery\n---\nship-task delegates to frontend-dev, design-qa, and code-reviewer.\ncommand: `{COMMAND}`{proof}\n"
        ),
    )?;
    Ok(())
}

fn write_routine(
    root: &Path,
    relative: &str,
    command: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().ok_or("routine has no parent")?)?;
    std::fs::write(
        path,
        format!(
            "---\nname: {MIRROR_NAME}\ndescription: Ships one release\n---\ncommand: `{command}`\nproof: `{PROOF}`\n"
        ),
    )?;
    Ok(())
}
