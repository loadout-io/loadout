//! WF-17: prawdziwy plan Labu musi policzyć rozwinięcie, zanim zapisze plan lub założy kopie.
use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::commands::lab::plan_a_run_inner;
use loadout_lib::workflow::{WorkflowFile, unroll::unroll};
use serde_json::{Value, json};

fn check(id: &str, folder: &str) -> Value {
    json!({"kind":"check","id":id,"name":id,"command":"printf '1 passed\\n'",
        "proof":"(\\d+) passed","folder":{"use":folder},"at":{"x":0,"y":0}})
}

fn prepare(project: &Path, graph: &Value) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(project.join(".loadout/workflows"))?;
    fs::create_dir_all(project.join(".loadout/evals"))?;
    fs::write(
        project.join(".loadout/workflows/subject.json"),
        serde_json::to_vec(graph)?,
    )?;
    fs::write(
        project.join(".loadout/evals/size.json"),
        serde_json::to_vec(&json!({
            "format":2,"id":"size","name":"Size before Start","subject":{"kind":"workflow","id":"subject"},
            "cases":[{"id":"case","name":"Case","task":"Compare","status":"in-use",
                "command":"printf '1 passed\\n'","proof":"(\\d+) passed"}],
            "variants":[{"id":"variant","name":"Variant","workflow":{"id":"subject","outputStep":"out"}}]
        }))?,
    )?;
    Ok(())
}

#[test]
fn a_small_literal_graph_cannot_hide_more_than_five_thousand_expanded_connections()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let agent = uuid::Uuid::now_v7();
    let mut steps: Vec<_> = (0..10).map(|at| json!({
        "kind":"agent","id":format!("s_{at}"),"name":format!("Step {at}"),"agent":agent,
        "instructions":"Do this step.","copies":8,"folder":{"use":"fresh-copy"},"at":{"x":0,"y":0}
    })).collect();
    steps.push(check("judge", "same-copy"));
    steps.push(check("out", "same-copy"));
    let mut links: Vec<_> = (0..9)
        .map(|at| json!({"from":format!("s_{at}"),"to":format!("s_{}",at+1)}))
        .collect();
    links.extend([
        json!({"from":"s_9","to":"judge"}),
        json!({"from":"judge","to":"s_0","max_turns":10}),
        json!({"from":"judge","to":"out"}),
    ]);
    let graph = json!({"format":1,"id":"subject","name":"Small drawing, large execution","steps":steps,"links":links});
    let subject: WorkflowFile = serde_json::from_value(graph.clone())?;
    let expanded = unroll(&subject);
    assert_eq!(
        expanded.nodes.len() + 1,
        812,
        "fixture must remain below the node limit including its grader"
    );
    assert_eq!(
        expanded.arrows.len() + 1,
        5914,
        "fixture must exceed the edge limit only after loop × copy expansion"
    );
    prepare(project.path(), &graph)?;
    let planned = plan_a_run_inner(project.path(), "size", 8);
    let refusal = planned
        .err()
        .ok_or("the actual Lab planner accepted more than 5000 expanded connections")?;
    assert!(
        refusal.to_string().contains("5000"),
        "the refusal did not explain the connection limit: {refusal}"
    );
    assert!(
        !loadout_lib::lab::project_plans(project.path()).exists(),
        "the refused matrix left a partially prepared plan"
    );
    assert!(
        !project.path().join(".loadout/runs").exists(),
        "sizing refusal created a run"
    );
    Ok(())
}

#[test]
fn a_long_same_copy_chain_does_not_invent_one_working_folder_per_step() -> Result<(), Box<dyn Error>>
{
    let project = tempfile::tempdir()?;
    let mut steps = vec![check("first", "fresh-copy")];
    steps.extend((0..110).map(|at| check(&format!("same_{at}"), "same-copy")));
    steps.push(check("out", "same-copy"));
    let links: Vec<_> = steps
        .windows(2)
        .map(|pair| json!({"from":pair[0]["id"],"to":pair[1]["id"]}))
        .collect();
    let graph = json!({"format":1,"id":"subject","name":"One long working copy","steps":steps,"links":links});
    prepare(project.path(), &graph)?;
    let planned = plan_a_run_inner(project.path(), "size", 1);
    assert!(
        planned.is_ok(),
        "112 sequential subject steps need one shared copy, plus one grader copy, not 113 folders: {planned:?}"
    );
    let graph: WorkflowFile = serde_json::from_slice(&fs::read(planned?.path)?)?;
    let expanded = unroll(&graph);
    assert_eq!(
        (expanded.nodes.len(), expanded.arrows.len()),
        (113, 112),
        "the accepted plan must preserve the whole chain and external grader"
    );
    Ok(())
}

fn one_set(
    repeats: usize,
    variants: usize,
) -> Result<loadout_lib::lab::EvalSet, serde_json::Error> {
    serde_json::from_value(json!({
        "format":2,"id":"size","name":"Sizes","subject":{"kind":"workflow","id":"subject"},
        "cases":[{"id":"case","name":"Case","task":"Compare","status":"in-use","repeats":repeats,
            "command":"printf '1 passed\\n'","proof":"(\\d+) passed"}],
        "variants":(0..variants).map(|at| json!({"id":format!("v{at}"),"name":format!("Variant {at}"),
            "workflow":{"id":"subject","outputStep":"out"}})).collect::<Vec<_>>()
    }))
}

#[test]
fn the_shared_preflight_matches_real_copy_round_fan_in_and_duplicate_link_expansion()
-> Result<(), Box<dyn Error>> {
    let source: WorkflowFile = serde_json::from_value(json!({
        "format":1,"id":"subject","name":"Copies and rounds",
        "steps":[{"kind":"agent","id":"work","name":"Work","agent":uuid::Uuid::now_v7(),
            "instructions":"Work","copies":3,"folder":{"use":"fresh-copy"},"at":{"x":0,"y":0}},
            check("judge","same-copy"),check("out","same-copy")],
        "links":[{"from":"work","to":"judge"},{"from":"work","to":"judge"},
            {"from":"judge","to":"work","max_turns":3},{"from":"judge","to":"out"}]
    }))?;
    let expanded = unroll(&source);
    let checked = loadout_lib::workflow::unroll::checked_size(&source)?;
    assert_eq!(
        (checked.nodes, checked.edges),
        (expanded.nodes.len(), expanded.arrows.len())
    );
    let set = one_set(2, 2)?;
    let selected = vec![source.clone(), source];
    let size =
        loadout_lib::lab::workflow_plan::preflight_selected_with_subjects(&selected, &set)?.0;
    assert_eq!(
        (size.cells, size.nodes, size.edges, size.trees),
        (4, 56, 68, 20)
    );
    let graph = loadout_lib::lab::workflow_plan::compose_selected(
        &selected,
        &set,
        "eval:size".into(),
        "Size".into(),
    )?;
    let expanded = unroll(&graph);
    let inputs = loadout_lib::workflow::execution::RunInputs::from_graph(&graph)?;
    let actual =
        loadout_lib::workflow::unroll::folders::working_folders(&graph, &expanded, &inputs, false)?;
    assert_eq!(
        (size.nodes, size.edges, size.trees),
        (
            expanded.nodes.len(),
            expanded.arrows.len(),
            actual.managed.len()
        ),
        "preflight and the executable graph disagree"
    );
    Ok(())
}

#[test]
fn the_hundred_folder_limit_includes_the_merge_target_and_external_grader()
-> Result<(), Box<dyn Error>> {
    let set = one_set(1, 1)?;
    for branches in [98, 99] {
        let mut steps: Vec<_> = (0..branches)
            .map(|at| check(&format!("branch_{at}"), "fresh-copy"))
            .collect();
        steps.push(check("out", "same-copy"));
        let links: Vec<_> = (0..branches)
            .map(|at| json!({"from":format!("branch_{at}"),"to":"out"}))
            .collect();
        let source = serde_json::from_value(
            json!({"format":1,"id":"subject","name":"Many folders","steps":steps,"links":links}),
        )?;
        let size =
            loadout_lib::lab::workflow_plan::preflight_selected_with_subjects(&[source], &set)
                .map(|(size, _)| size);
        if branches == 98 {
            assert_eq!(
                size?.trees, 100,
                "98 branches + merge + grader must fit exactly"
            );
        } else {
            assert!(
                size.is_err(),
                "the merge or grader working folder was not counted"
            );
        }
    }
    Ok(())
}

#[test]
fn a_managed_project_chain_uses_one_scope_copy_and_one_grader_copy() -> Result<(), Box<dyn Error>> {
    let source = serde_json::from_value(json!({"format":1,"id":"subject","name":"Project scope",
        "steps":[check("first","project"),check("middle","same-copy"),check("out","project")],
        "links":[{"from":"first","to":"middle"},{"from":"middle","to":"out"}]}))?;
    let size = loadout_lib::lab::workflow_plan::preflight_selected_with_subjects(
        &[source],
        &one_set(1, 1)?,
    )?
    .0;
    assert_eq!(
        (size.cells, size.nodes, size.edges, size.trees),
        (1, 4, 3, 2)
    );
    Ok(())
}

#[tokio::test]
async fn a_project_and_an_exact_pick_alias_still_share_the_real_run_folder()
-> Result<(), Box<dyn Error>> {
    use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
    use loadout_lib::engine::drivers::{AgentDriver, absent::Absent};
    use std::sync::Arc;
    let project = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    let root = fs::canonicalize(project.path())?;
    fs::create_dir_all(root.join(".loadout"))?;
    let mut picked = check("picked", "pick");
    picked["folder"]["path"] = json!(root);
    let mut joined = check("out", "same-copy");
    joined["command"] = json!("pwd > joined-cwd; printf '1 passed\\n'");
    let workflow = home.path().join("aliases.json");
    fs::write(
        &workflow,
        serde_json::to_vec(
            &json!({"format":1,"id":"aliases","name":"Two names, one folder",
        "steps":[check("project","project"),picked,joined],
        "links":[{"from":"project","to":"picked"},{"from":"project","to":"out"},{"from":"picked","to":"out"}]}),
        )?,
    )?;
    let absent: Arc<dyn AgentDriver> =
        Arc::new(Absent::new("unused", "the fixture uses only real checks"));
    let drivers: Drivers = Arc::new(move |_| Arc::clone(&absent));
    let store = loadout_lib::store::Store::open(&root.join(".loadout/loadout.db"))?;
    let processes = Arc::new(loadout_lib::commands::processes::Processes::new());
    let deps = RunDeps {
        home: home.path(),
        project: &root,
        store: &store,
        drivers,
        processes: Arc::clone(&processes),
        control: RunControl::new(),
    };
    let (sink, _lines) = loadout_lib::ipc::line_channel(256);
    let report = loadout_lib::commands::run::run_workflow_inner(
        &deps,
        &RunRequest {
            workflow,
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        },
        sink,
    )
    .await;
    let proofs = processes.close().await;
    assert!(proofs.iter().all(|proof| matches!(
        proof,
        loadout_lib::engine::supervisor::GroupProof::Dead { .. }
    )));
    let report = report?;
    let actual = fs::read_to_string(root.join("joined-cwd")).ok();
    assert_eq!(
        actual.as_deref().map(str::trim),
        root.to_str(),
        "the real join treated two equal folders as a new fan-in copy: {report:?}"
    );
    Ok(())
}
