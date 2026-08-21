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
