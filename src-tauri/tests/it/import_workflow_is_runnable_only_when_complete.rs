#[test]
fn only_complete_native_workflow_is_runnable() -> Result<(), Box<dyn std::error::Error>> {
    let repo = tempfile::tempdir()?;
    std::fs::create_dir_all(repo.path().join(".claude/agents"))?;
    std::fs::create_dir_all(repo.path().join(".claude/skills/ship-ui"))?;
    for role in ["frontend-dev", "design-qa", "code-reviewer"] {
        std::fs::write(
            repo.path().join(format!(".claude/agents/{role}.md")),
            format!("---\nname: {role}\ndescription: Project role\n---\nDo the {role} work."),
        )?;
    }
    std::fs::write(
        repo.path().join(".claude/skills/ship-ui/SKILL.md"),
        "---\nname: ship-ui\ndescription: Coordinates UI delivery\n---\nship-task delegates to frontend-dev, design-qa, and code-reviewer. Run `./verify.sh full` before finishing.",
    )?;
    let preview = loadout_lib::import::translate::preview(repo.path())?;
    assert!(preview.draft.runnable());
    let workflow = &preview.draft.workflows[0];
    assert_eq!(workflow.steps.len(), 7);
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
