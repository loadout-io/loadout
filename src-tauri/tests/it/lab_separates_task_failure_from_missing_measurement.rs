//! WF-18: właściwy bieg i właściwa tablica, nie sam parser protokołu.
use loadout_lib::commands::{Drivers, RunControl, RunDeps};
use loadout_lib::ipc::line_channel;
use loadout_lib::store::Store;
use serde_json::{Value, json};
use std::{error::Error, fs, sync::Arc};

#[tokio::test]
async fn an_independent_examiner_counts_subject_failure_but_not_its_own_missing_dependency()
-> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let project = temp.path().join("project");
    let home = temp.path().join("home");
    fs::create_dir_all(project.join(".loadout/workflows"))?;
    fs::create_dir_all(project.join(".loadout/evals"))?;
    fs::create_dir_all(&home)?;
    let mut variants = Vec::new();
    for (id, program) in [
        ("good", "print('42')"),
        (
            "malicious",
            "import os; print('100 passed', flush=True); os._exit(0)",
        ),
        ("syntax", "def broken(:"),
    ] {
        let command = format!(
            "printf '%s' '{}' > result.py && printf '1 passed\\n'",
            program.replace('\'', "'\\''")
        );
        let graph = json!({"format":1,"id":id,"name":id,"steps":[{"kind":"check","id":"output","name":"Output",
            "command":command,"proof":"(\\d+) passed","folder":{"use":"project"}}],"links":[]});
        fs::write(
            project.join(format!(".loadout/workflows/{id}.json")),
            serde_json::to_vec(&graph)?,
        )?;
        variants.push(json!({"id":id,"name":id,"workflow":{"id":id,"outputStep":"output"}}));
    }
    let examiner = temp.path().join("examiner.py");
    fs::write(
        &examiner,
        r"import json, pathlib, subprocess, sys
request = json.load(sys.stdin)
program = pathlib.Path(request['results'][0]['files']['root']) / 'result.py'
child = subprocess.run([sys.executable, str(program)], capture_output=True, text=True)
if child.returncode:
    result = dict(format=1, status='subject-cannot-load', passed=0, failed=1, reason='The solution could not be loaded.')
else:
    correct = child.stdout.strip() == '42'
    result = dict(format=1, status='completed', passed=int(correct), failed=int(not correct), reason='' if correct else 'The solution did not return 42.')
print(json.dumps(result))
",
    )?;
    let missing = temp.path().join("missing.py");
    fs::write(&missing, "import loadout_missing_examiner_dependency\n")?;
    let quote = |path: &std::path::Path| {
        format!(
            "/usr/bin/python3 '{}'",
            path.to_string_lossy().replace('\'', "'\\''")
        )
    };
    let set = json!({"format":2,"id":"classification","name":"Honest assessment","subject":{"kind":"workflow","id":"good"},
        "cases":[{"id":"normal","name":"Independent behavior","task":"Return 42","status":"in-use","command":quote(&examiner),"proofMode":"external-assessment-v1","proof":"unused pattern"},
                 {"id":"missing","name":"Unavailable examiner","task":"Return 42","status":"in-use","command":quote(&missing),"proofMode":"external-assessment-v1","proof":"unused pattern"}],
        "variants":variants});
    fs::write(
        project.join(".loadout/evals/classification.json"),
        serde_json::to_vec(&set)?,
    )?;
    let planned = loadout_lib::commands::lab::plan_a_run_inner(&project, "classification", 4)?;
    let store = Store::open(&project.join(".loadout/loadout.db"))?;
    let drivers: Drivers =
        Arc::new(|_| unreachable!("there are no paid steps in this acceptance test"));
    let deps = RunDeps {
        home: &home,
        project: &project,
        store: &store,
        drivers,
        control: RunControl::new(),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
    };
    let (sink, _source) = line_channel(2048);
    let run = loadout_lib::commands::run::run_workflow_inner(&deps, &planned.request, sink).await?;
    let board = loadout_lib::commands::lab::read_board_inner(&project, "classification", 5)?;
    let latest = board.runs.first().ok_or("missing visible comparison")?;
    assert_eq!(
        (latest.passed, latest.judged, latest.cells.len()),
        (1, 3, 6),
        "a fake positive count, SyntaxError and a missing examiner dependency mean three different things: {latest:?}"
    );
    for cell in &latest.cells {
        let expected = if cell.case == "missing" {
            "not-judged"
        } else if cell.variant == "good" {
            "passed"
        } else {
            "did-not-pass"
        };
        assert_eq!(cell.outcome, expected);
    }
    let mut receipt: Value = serde_json::from_slice(&fs::read(run.dir.join("run.json"))?)?;
    assert!(
        receipt["steps"]
            .as_array()
            .ok_or("missing steps")?
            .iter()
            .filter(|step| step["process_started"] == true)
            .all(|step| step["death_proof"] == true)
    );
    let original_receipt = receipt.clone();
    let bindings = receipt["workflow_snapshot"]["cellBindings"]
        .as_array_mut()
        .ok_or("missing cell map")?;
    let good = bindings
        .iter()
        .find(|one| one["case"] == "normal" && one["variant"] == "good")
        .ok_or("missing good mapping")?["grader"]
        .clone();
    let false_green = bindings
        .iter_mut()
        .find(|one| one["case"] == "normal" && one["variant"] == "malicious")
        .ok_or("missing malicious mapping")?;
    false_green["grader"] = good;
    fs::write(run.dir.join("run.json"), serde_json::to_vec(&receipt)?)?;
    let changed = loadout_lib::commands::lab::read_board_inner(&project, "classification", 5)?;
    assert_eq!(
        (changed.runs[0].passed, changed.runs[0].judged),
        (0, 0),
        "a changed cell map must invalidate measurement, not award another cell's successful examiner"
    );
    assert!(
        changed.runs[0]
            .cells
            .iter()
            .all(|one| one.said.contains("changed")),
        "the human needs the reason the saved measurement is no longer valid"
    );
    for field in ["executionInputs", "links", "steps"] {
        let mut receipt = original_receipt.clone();
        receipt["workflow_snapshot"][field] = match field {
            "executionInputs" => json!({"isolateContexts":true}),
            _ => json!([]),
        };
        fs::write(run.dir.join("run.json"), serde_json::to_vec(&receipt)?)?;
        let changed = loadout_lib::commands::lab::read_board_inner(&project, "classification", 5)?;
        assert_eq!(
            (changed.runs[0].passed, changed.runs[0].judged),
            (0, 0),
            "changing the measured {field} cannot reuse an old result"
        );
        assert!(
            changed.runs[0]
                .cells
                .iter()
                .all(|one| one.said.contains("changed"))
        );
    }
    Ok(())
}
