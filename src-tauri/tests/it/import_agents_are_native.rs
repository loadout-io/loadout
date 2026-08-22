#[test]
fn both_vendor_agents_translate_to_native_agents() -> Result<(), Box<dyn std::error::Error>> {
    use loadout_lib::library::agents::{FileAccess, Thinking, Tools, Vendor};
    let repo = tempfile::tempdir()?;
    std::fs::create_dir_all(repo.path().join(".claude/agents"))?;
    std::fs::create_dir_all(repo.path().join(".codex/agents"))?;
    std::fs::write(
        repo.path().join(".claude/agents/frontend-dev.md"),
        "---\nname: frontend-dev\ndescription: Builds UI\nmodel: opus\npermissionMode: acceptEdits\ntools: [Read, Write]\nskills: [ship-ui]\n---\nBuild the interface.",
    )?;
    std::fs::write(
        repo.path().join(".codex/agents/reviewer.toml"),
        "name = \"reviewer\"\ndescription = \"Reviews code\"\nmodel = \"gpt-5.6-sol\"\nmodel_reasoning_effort = \"high\"\nsandbox_mode = \"workspace-write\"\ndeveloper_instructions = \"\"\"Review the patch.\"\"\"\n",
    )?;
    let preview = loadout_lib::import::translate::preview(repo.path())?;
    assert_eq!(preview.draft.agents.len(), 2);
    let claude = preview
        .draft
        .agents
        .iter()
        .find(|agent| agent.name == "frontend-dev")
        .ok_or("Claude agent was not imported")?;
    assert_eq!(claude.runs_with, Vendor::ClaudeCode);
    assert_eq!(claude.file_access, FileAccess::AskFirst);
    assert_eq!(
        claude.tools,
        Tools::Only(vec!["Read".into(), "Write".into()])
    );
    assert_eq!(claude.skills, vec!["ship-ui"]);
    let codex = preview
        .draft
        .agents
        .iter()
        .find(|agent| agent.name == "reviewer")
        .ok_or("Codex agent was not imported")?;
    assert_eq!(codex.runs_with, Vendor::Codex);
    assert_eq!(codex.thinking, Thinking::Deep);
    assert_eq!(codex.file_access, FileAccess::AskFirst);
    assert_ne!(claude.id, codex.id);

    std::fs::write(
        repo.path().join(".claude/agents/frontend-dev.md"),
        "---\nname: frontend-dev\npermissionMode: bypassPermissions\n---\nBuild the interface.",
    )?;
    let blocked = loadout_lib::import::translate::preview(repo.path())?;
    assert!(!blocked.draft.runnable());
    assert!(blocked.draft.report.mappings.iter().any(|mapping| {
        mapping.message.contains("bypasses permission checks")
            && mapping.compatibility == loadout_lib::import::Compatibility::Unsupported
    }));
    Ok(())
}

#[test]
fn a_real_claude_role_is_visible_even_when_one_behavior_needs_a_choice()
-> Result<(), Box<dyn std::error::Error>> {
    use loadout_lib::import::Compatibility;
    use loadout_lib::library::agents::{Color, Tools};

    let repo = tempfile::tempdir()?;
    std::fs::create_dir_all(repo.path().join(".claude/agents"))?;
    std::fs::write(
        repo.path().join(".claude/agents/frontend-dev.md"),
        "---\nname: frontend-dev\ndescription: >\n  Senior Angular developer.\n  Builds the project interface.\ntools: Read, Write, Bash\ndisallowedTools: Bash\nmodel: opus\nmaxTurns: 35\npermissionMode: acceptEdits\nmemory: project\ncolor: green\nskills: design-system-reference\n---\nBuild production-grade Angular features.",
    )?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;

    let agent = preview.draft.agents.first().ok_or("agent disappeared")?;
    assert_eq!(agent.name, "frontend-dev");
    assert_eq!(
        agent.summary,
        "Senior Angular developer. Builds the project interface."
    );
    assert_eq!(agent.color, Color::Moss);
    assert_eq!(
        agent.tools,
        Tools::Only(vec!["Read".into(), "Write".into()])
    );
    assert_eq!(agent.skills, vec!["design-system-reference"]);
    assert!(preview.draft.report.mappings.iter().any(|mapping| {
        mapping.compatibility == Compatibility::NeedsChoice
            && mapping.message.contains("project memory")
            && mapping.message.contains("turn limit")
    }));
    assert!(!preview.draft.runnable());
    Ok(())
}
