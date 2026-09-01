//! T-82 AC-1: rekonstrukcja czyta role i strzałki ze źródła zamiast rozpoznawać jedną fiksturę.
//!
//! Nazwy w tym pliku celowo nie zawierają `frontend-dev`, `design-qa` ani `code-reviewer`.
//! Poprzednia implementacja pytała tylko o obecność tych trzech napisów i zawsze składała
//! `Ship UI`; każde repo z własnym słownikiem dostawało pustą listę workflow. Druga fikstura
//! ma warunek, którego dzisiejszy graf nie wyraża: importer nie może go pominąć i nazwać wyniku
//! zgodnym, ale ma zachować źródło jako konkretną pozycję `NeedsChoice`.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::import::{ImportSourceRole, ImportStatus, ItemKind};
use loadout_lib::workflow::{Skills, Step, WorkflowFile};

const WORKFLOW_NAME: &str = "Delivery Circuit";
const SOURCE_PATH: &str = ".claude/workflows/delivery-circuit.js";
const SKILL: &str = "assembly-guide";

/// Jawny, mały język źródłowy: nazwa roli, instrukcja, zależności i wybory kroku stoją przy
/// jednym wywołaniu. Test nie zgaduje kolejności z pozycji w pliku — porównuje zapisane `after`.
const SOURCE: &str = r#"workflow("Delivery Circuit", () => {
  const plan = agent("pathfinder", {
    name: "Pathfinder",
    task: "map the delivery",
    folder: "fresh-copy"
  });
  const build = agent("maker", {
    name: "Maker",
    task: "build the delivery",
    after: [plan],
    skills: ["assembly-guide"],
    folder: "fresh-copy"
  });
  const visual = agent("prism", {
    name: "Prism",
    task: "inspect the visible result",
    after: [build],
    folder: "fresh-copy"
  });
  const code = agent("sentinel", {
    name: "Sentinel",
    task: "inspect the implementation",
    after: [build],
    folder: "fresh-copy"
  });
  agent("binder", {
    name: "Binder",
    task: "combine both reviews",
    after: [visual, code],
    folder: "fresh-copy"
  });
});
"#;

const CONDITIONAL_NAME: &str = "Conditional Delivery";
const CONDITIONAL_PATH: &str = ".claude/workflows/conditional-delivery.js";
const CONDITIONAL: &str = r#"workflow("Conditional Delivery", () => {
  const plan = agent("pathfinder", { task: "map the delivery" });
  if (risk_is_high()) {
    agent("prism", { task: "inspect risk", after: [plan] });
  } else {
    agent("maker", { task: "build immediately", after: [plan] });
  }
});
"#;

const ROLES: [&str; 5] = ["pathfinder", "maker", "prism", "sentinel", "binder"];

#[test]
fn arbitrary_role_names_and_source_edges_make_a_native_workflow() -> Result<(), Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    write_roles(repo.path())?;
    write_skill(repo.path())?;
    write_source(repo.path(), SOURCE_PATH, SOURCE)?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;
    let workflow = preview
        .draft
        .workflows
        .iter()
        .find(|workflow| workflow.name == WORKFLOW_NAME)
        .ok_or_else(|| {
            format!(
                "{WORKFLOW_NAME} was not reconstructed. The importer still recognizes one \"Ship UI\" fixture instead of the roles and edges in {SOURCE_PATH}"
            )
        })?;

    let native: BTreeMap<String, String> = preview
        .draft
        .agents
        .iter()
        .map(|agent| (agent.id.to_string(), agent.name.to_ascii_lowercase()))
        .collect();
    let by_role = steps_by_native_role(workflow, &native)?;

    assert_eq!(
        by_role.keys().cloned().collect::<BTreeSet<_>>(),
        ROLES.into_iter().map(str::to_owned).collect(),
        "every imported step must point at one of the five native agents from this repository"
    );

    let expected = BTreeSet::from([
        edge(&by_role, "pathfinder", "maker")?,
        edge(&by_role, "maker", "prism")?,
        edge(&by_role, "maker", "sentinel")?,
        edge(&by_role, "prism", "binder")?,
        edge(&by_role, "sentinel", "binder")?,
    ]);
    let actual: BTreeSet<(String, String)> = workflow
        .links
        .iter()
        .map(|link| (link.from.clone(), link.to.clone()))
        .collect();
    assert_eq!(
        actual, expected,
        "the fan-out and fan-in must come from `after` in the source; a hard-coded graph is not an import"
    );

    let maker_id = by_role
        .get("maker")
        .ok_or("the imported workflow lost the maker step")?;
    let maker = workflow
        .steps
        .iter()
        .find_map(|step| match step {
            Step::Agent(step) if &step.id == maker_id => Some(step),
            Step::Agent(_) | Step::Check(_) | Step::Checkpoint(_) | Step::Serve(_) => None,
        })
        .ok_or("the maker id no longer points at an agent step")?;
    assert_eq!(
        maker.skills,
        Skills::Only(vec![SKILL.to_owned()]),
        "the source assigns one skill to the maker; silently widening it to all skills changes the imported behavior"
    );
    let item = item_from(&preview.draft.items, Path::new(SOURCE_PATH))?;
    assert!(
        preview.draft.items.iter().all(|candidate| {
            candidate.kind != ItemKind::Workflow
                || candidate.status != ImportStatus::Ready
                || candidate.target.is_some()
        }),
        "a Ready workflow without a generated target would fail only after the person approves the import"
    );
    assert!(
        item.dependencies.contains(&format!("skill:{SKILL}")),
        "a step-level skill must remain a workflow dependency instead of failing later at Start"
    );
    Ok(())
}

#[test]
fn a_missing_step_skill_keeps_the_workflow_not_ready() -> Result<(), Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    write_roles(repo.path())?;
    write_source(repo.path(), SOURCE_PATH, SOURCE)?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;
    assert!(
        preview
            .draft
            .workflows
            .iter()
            .all(|workflow| workflow.name != WORKFLOW_NAME)
    );
    let item = item_from(&preview.draft.items, Path::new(SOURCE_PATH))?;
    assert_eq!(item.status, ImportStatus::MissingDependencies);
    assert!(item.target.is_none());
    assert!(item.dependencies.contains(&format!("skill:{SKILL}")));
    assert!(item.status_message.contains(&format!("skill:{SKILL}")));
    Ok(())
}

#[test]
fn a_step_cannot_select_a_skill_not_assigned_to_its_agent() -> Result<(), Box<dyn Error>> {
    const FORBIDDEN: &str = "forbidden-guide";

    let repo = tempfile::tempdir()?;
    write_roles(repo.path())?;
    write_skill(repo.path())?;
    write_named_skill(repo.path(), FORBIDDEN)?;
    let source = SOURCE.replace(SKILL, FORBIDDEN);
    write_source(repo.path(), SOURCE_PATH, &source)?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;
    assert!(
        preview
            .draft
            .workflows
            .iter()
            .all(|workflow| workflow.name != WORKFLOW_NAME)
    );
    let item = item_from(&preview.draft.items, Path::new(SOURCE_PATH))?;
    assert_eq!(item.status, ImportStatus::NeedsChoice);
    assert!(item.target.is_none());
    assert!(item.dependencies.contains(&format!("skill:{FORBIDDEN}")));
    let message = item.status_message.to_ascii_lowercase();
    assert!(
        message.contains(FORBIDDEN) && message.contains("maker"),
        "the unresolved choice must name the skill and assigned agent. It said: {}",
        item.status_message
    );
    Ok(())
}

#[test]
fn source_skill_case_is_normalized_to_the_runtime_identity() -> Result<(), Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    write_roles(repo.path())?;
    write_skill(repo.path())?;
    let mismatched = SKILL.to_ascii_uppercase();
    let source = SOURCE.replace(SKILL, &mismatched);
    write_source(repo.path(), SOURCE_PATH, &source)?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;
    let workflow = preview
        .draft
        .workflows
        .iter()
        .find(|workflow| workflow.name == WORKFLOW_NAME)
        .ok_or("a case-only source spelling did not resolve to the imported skill")?;
    let selected = workflow.steps.iter().find_map(|step| match step {
        Step::Agent(step) if step.name == "Maker" => Some(&step.skills),
        Step::Agent(_) | Step::Check(_) | Step::Checkpoint(_) | Step::Serve(_) => None,
    });
    assert_eq!(selected, Some(&Skills::Only(vec![SKILL.to_owned()])));
    let item = item_from(&preview.draft.items, Path::new(SOURCE_PATH))?;
    assert_eq!(item.status, ImportStatus::Ready);
    assert!(item.target.is_some());
    assert!(item.dependencies.contains(&format!("skill:{SKILL}")));
    assert!(
        !item.dependencies.contains(&format!("skill:{mismatched}")),
        "the generated workflow must carry the canonical library name, not a source-only spelling"
    );
    Ok(())
}

#[test]
fn a_skill_that_the_runtime_would_reject_never_makes_a_ready_workflow() -> Result<(), Box<dyn Error>>
{
    let repo = tempfile::tempdir()?;
    write_roles_with_maker_skill(repo.path(), &SKILL.to_ascii_uppercase())?;
    let invalid = SKILL.to_ascii_uppercase();
    write_named_skill(repo.path(), &invalid)?;
    write_source(repo.path(), SOURCE_PATH, &SOURCE.replace(SKILL, &invalid))?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;
    assert!(
        preview
            .draft
            .workflows
            .iter()
            .all(|workflow| workflow.name != WORKFLOW_NAME)
    );
    let item = item_from(&preview.draft.items, Path::new(SOURCE_PATH))?;
    assert_eq!(item.status, ImportStatus::NeedsChoice);
    assert!(item.target.is_none());
    assert!(item.status_message.contains(&invalid));
    assert!(
        item.status_message
            .to_ascii_lowercase()
            .contains("cannot run"),
        "the import screen must explain that the selected skill is unusable. It said: {}",
        item.status_message
    );
    Ok(())
}

#[test]
fn a_runtime_usable_vendor_skill_field_does_not_block_the_workflow() -> Result<(), Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    write_roles(repo.path())?;
    write_named_skill_with_extra(repo.path(), SKILL, "user-invocable: true\n")?;
    write_source(repo.path(), SOURCE_PATH, SOURCE)?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;
    let item = item_from(&preview.draft.items, Path::new(SOURCE_PATH))?;
    assert_eq!(item.status, ImportStatus::Ready);
    assert!(item.target.is_some());
    assert!(
        preview
            .draft
            .workflows
            .iter()
            .any(|workflow| workflow.name == WORKFLOW_NAME),
        "the importer must use the same usable-skill contract as Start, not the stricter publishing contract"
    );
    Ok(())
}

#[test]
fn a_slug_equivalent_role_resolves_once_and_depends_on_the_native_agent()
-> Result<(), Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    write_roles(repo.path())?;
    write_named_role(repo.path(), "maker.md", "maker role", SKILL)?;
    write_skill(repo.path())?;
    let source = SOURCE.replace("agent(\"maker\"", "agent(\"maker_role\"");
    write_source(repo.path(), SOURCE_PATH, &source)?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;
    let item = item_from(&preview.draft.items, Path::new(SOURCE_PATH))?;
    assert_eq!(item.status, ImportStatus::Ready);
    assert!(item.target.is_some());
    assert!(
        !item.dependencies.contains(&"agent:maker_role".to_owned()),
        "a reconstructed workflow must depend on the resolved native id, not the source spelling"
    );
    assert!(item.dependencies.iter().any(|dependency| {
        dependency
            .strip_prefix("agent:")
            .is_some_and(|id| id.parse::<uuid::Uuid>().is_ok())
    }));
    Ok(())
}

#[test]
fn two_agents_with_the_same_role_identity_require_a_named_choice() -> Result<(), Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    write_roles(repo.path())?;
    write_named_role(repo.path(), "maker.md", "maker_role", SKILL)?;
    write_named_role(repo.path(), "maker-two.md", "maker-role", SKILL)?;
    write_skill(repo.path())?;
    let source = SOURCE.replace("agent(\"maker\"", "agent(\"maker_role\"");
    write_source(repo.path(), SOURCE_PATH, &source)?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;
    let item = item_from(&preview.draft.items, Path::new(SOURCE_PATH))?;
    assert_eq!(item.status, ImportStatus::NeedsChoice);
    assert!(item.target.is_none());
    assert!(
        item.status_message.contains("maker_role")
            && item.status_message.contains("maker-role")
            && item.status_message.contains("Maker"),
        "the choice must name the source role, both candidates, and the visible step. It said: {}",
        item.status_message
    );
    Ok(())
}

#[test]
fn a_parallel_graph_that_would_share_the_project_folder_is_not_ready() -> Result<(), Box<dyn Error>>
{
    let repo = tempfile::tempdir()?;
    write_roles(repo.path())?;
    write_skill(repo.path())?;
    let source = SOURCE.replace("    folder: \"fresh-copy\"\n", "");
    write_source(repo.path(), SOURCE_PATH, &source)?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;
    let item = item_from(&preview.draft.items, Path::new(SOURCE_PATH))?;
    assert_eq!(item.status, ImportStatus::NeedsChoice);
    assert!(item.target.is_none());
    let message = item.status_message.to_ascii_lowercase();
    assert!(
        message.contains("same time") && message.contains("fresh copy"),
        "the import screen must show the same folder collision that would stop Run. It said: {}",
        item.status_message
    );
    Ok(())
}

#[test]
fn an_unrepresentable_branch_stays_named_and_unresolved() -> Result<(), Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    write_roles(repo.path())?;
    write_skill(repo.path())?;
    write_source(repo.path(), CONDITIONAL_PATH, CONDITIONAL)?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;
    assert!(
        preview
            .draft
            .workflows
            .iter()
            .all(|workflow| workflow.name != CONDITIONAL_NAME),
        "a runtime condition cannot be represented by today's graph and must not be flattened away"
    );

    let item = item_from(&preview.draft.items, Path::new(CONDITIONAL_PATH))?;
    assert_eq!(item.kind, ItemKind::Workflow);
    assert_eq!(
        item.status,
        ImportStatus::NeedsChoice,
        "an unrepresentable branch is a choice for the person, not an empty successful import"
    );
    assert!(
        item.target.is_none(),
        "an unresolved branch must not claim a generated workflow target"
    );
    let message = item.status_message.to_ascii_lowercase();
    assert!(
        message.contains("conditional delivery") || message.contains("conditional-delivery"),
        "the unresolved item must name the behavior a person can find. It said: {}",
        item.status_message
    );
    Ok(())
}

fn write_roles(root: &Path) -> Result<(), Box<dyn Error>> {
    write_roles_with_maker_skill(root, SKILL)
}

fn write_roles_with_maker_skill(root: &Path, maker_skill: &str) -> Result<(), Box<dyn Error>> {
    let agents = root.join(".claude/agents");
    fs::create_dir_all(&agents)?;
    for role in ROLES {
        let skills = if role == "maker" { maker_skill } else { "" };
        let skill_field = if skills.is_empty() {
            String::new()
        } else {
            format!("skills: [{skills}]\n")
        };
        fs::write(
            agents.join(format!("{role}.md")),
            format!(
                "---\nname: {role}\ndescription: Does the {role} part\nmodel: sonnet\ntools: [Read, Write]\n{skill_field}---\nDo the {role} work.\n"
            ),
        )?;
    }
    Ok(())
}

fn write_named_role(
    root: &Path,
    file: &str,
    name: &str,
    skill: &str,
) -> Result<(), Box<dyn Error>> {
    fs::write(
        root.join(".claude/agents").join(file),
        format!(
            "---\nname: {name}\ndescription: Does the maker part\nmodel: sonnet\ntools: [Read, Write]\nskills: [{skill}]\n---\nDo the maker work.\n"
        ),
    )?;
    Ok(())
}

fn write_skill(root: &Path) -> Result<(), Box<dyn Error>> {
    write_named_skill(root, SKILL)
}

fn write_named_skill(root: &Path, name: &str) -> Result<(), Box<dyn Error>> {
    write_named_skill_with_extra(root, name, "")
}

fn write_named_skill_with_extra(
    root: &Path,
    name: &str,
    extra: &str,
) -> Result<(), Box<dyn Error>> {
    let skill = root.join(".agents/skills").join(name);
    fs::create_dir_all(&skill)?;
    fs::write(
        skill.join("SKILL.md"),
        format!(
            "---\nname: {name}\ndescription: Builds the delivery consistently\n{extra}---\nFollow the delivery conventions.\n"
        ),
    )?;
    Ok(())
}

fn write_source(root: &Path, relative: &str, content: &str) -> Result<(), Box<dyn Error>> {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().ok_or("workflow source has no parent")?)?;
    fs::write(path, content)?;
    Ok(())
}

fn steps_by_native_role(
    workflow: &WorkflowFile,
    native: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let mut out = BTreeMap::new();
    for step in &workflow.steps {
        let Step::Agent(step) = step else {
            return Err(format!(
                "{WORKFLOW_NAME} is an all-agent source, but imported a non-agent step named {}",
                step.name()
            )
            .into());
        };
        let role = native.get(&step.agent).ok_or_else(|| {
            format!(
                "step {} points at {}, which is not one of the imported native agents",
                step.name, step.agent
            )
        })?;
        out.insert(role.clone(), step.id.clone());
    }
    Ok(out)
}

fn edge(
    steps: &BTreeMap<String, String>,
    from: &str,
    to: &str,
) -> Result<(String, String), Box<dyn Error>> {
    Ok((
        steps
            .get(from)
            .ok_or_else(|| format!("the source role {from} has no imported step"))?
            .clone(),
        steps
            .get(to)
            .ok_or_else(|| format!("the source role {to} has no imported step"))?
            .clone(),
    ))
}

fn item_from<'a>(
    items: &'a [loadout_lib::import::ImportItem],
    source: &Path,
) -> Result<&'a loadout_lib::import::ImportItem, Box<dyn Error>> {
    items
        .iter()
        .find(|item| {
            item.sources.iter().any(|candidate| {
                candidate.path == source && candidate.role == ImportSourceRole::Definition
            })
        })
        .ok_or_else(|| {
            format!(
                "{} disappeared instead of becoming a typed import item",
                source.display()
            )
            .into()
        })
}
